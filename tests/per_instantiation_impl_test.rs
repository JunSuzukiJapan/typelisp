//! An `impl` for one instantiation of a generic type (`(impl Box<int> ...)`,
//! `(impl Show Box<int> ...)`) belongs to that instantiation: each
//! instantiation can have its own, and a method or trait is written either
//! for every instantiation or for each one it applies to, never both.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

/// `body` (a `string`-valued expression) after `defs`, interpreted and inside
/// a compiled function, both expected to give `expected`.
fn both(defs: &str, body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", defs, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", defs, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

const BOXES: &str = r##"
(defstruct Box<T> (v T))
(impl Box<int>
  (pub twice (self) int (* 2 self::v))
  (pub make () Box<int> (make-instance 'Box :v 7)))
(impl Box<string>
  (pub twice (self) string (format false "~a~a" self::v self::v))
  (pub make () Box<string> (make-instance 'Box :v "s")))
"##;

// ---- inherent impls ---------------------------------------------------------

/// Two instantiations, two bodies under one name.
#[test]
fn each_instantiation_has_its_own_method() {
    both(BOXES, r##"(format false "~a ~a" (twice (make-instance 'Box :v 4)) (twice (make-instance 'Box :v "ab")))"##, "8 abab");
}

/// An instantiation with no `impl` of its own has none of their methods.
#[test]
fn an_instantiation_without_an_impl_has_no_method() {
    let msg = check_err(&format!("{}(defun f () int (twice (make-instance 'Box :v 1.5)))", BOXES));
    assert!(msg.contains("no method `twice` for type `Box<f64>`"), "{}", msg);
    assert!(msg.contains("`Box<int>`") && msg.contains("`Box<string>`"), "{}", msg);
}

/// A static method is picked by the type the call is expected to return.
#[test]
fn a_static_method_is_picked_by_the_expected_type() {
    both(
        BOXES,
        r##"(format false "~a ~a" (twice (the Box<int> (Box::make))) (twice (the Box<string> (Box::make))))"##,
        "14 ss",
    );
    let msg = check_err(&format!("{}(defun f () int (progn (Box::make) 0))", BOXES));
    assert!(msg.contains("`Box::make` is written for `Box<int>`, `Box<string>`"), "{}", msg);
}

/// With only one instantiation writing it, a static method needs nothing to
/// pick it.
#[test]
fn a_static_method_written_once_needs_no_expected_type() {
    let defs = "(defstruct Box<T> (v T))\n(impl Box<int> (pub make () Box<int> (make-instance 'Box :v 3)))\n\
                (impl Box<int> (pub get (self) int self::v))\n";
    both(defs, r##"(format false "~a" (get (Box::make)))"##, "3");
}

/// Taken as a value, the method is the one for the expected receiver.
#[test]
fn a_method_value_is_the_one_for_the_expected_receiver() {
    let defs = format!(
        "{}(defun app-i ((f (fn (Box<int>) int)) (b Box<int>)) int (f b))\n\
         (defun app-s ((f (fn (Box<string>) string)) (b Box<string>)) string (f b))\n",
        BOXES
    );
    both(&defs, r##"(format false "~a ~a" (app-i twice (make-instance 'Box :v 5)) (app-s twice (make-instance 'Box :v "x")))"##, "10 xx");
}

/// A method with type parameters of its own, written for two instantiations.
#[test]
fn a_method_with_its_own_type_parameters_for_each_instantiation() {
    let defs = r##"
(defstruct Box<T> (v T))
(impl Box<int> (pub with<U> (self (f (fn (int) U))) U (f (+ self::v 1))))
(impl Box<string> (pub with<U> (self (f (fn (string) U))) U (f (format false "~a!" self::v))))
"##;
    both(
        defs,
        r##"(format false "~a ~a" (with (make-instance 'Box :v 1) (lambda ((n int)) string (format false "i~a" n)))
                              (with (make-instance 'Box :v "b") (lambda ((s string)) string (format false "s~a" s))))"##,
        "i2 sb!",
    );
}

/// `~/name/` resolves like the call it stands for.
#[test]
fn a_call_directive_finds_each_instantiations_method() {
    let defs = r##"
(defstruct Box<T> (v T))
(impl Box<int> (pub tag (self (colon bool) (at bool)) string (format false "i~a" self::v)))
(impl Box<string> (pub tag (self (colon bool) (at bool)) string (format false "s~a" self::v)))
"##;
    both(defs, r##"(format false "~/tag/ ~/tag/" (make-instance 'Box :v 1) (make-instance 'Box :v "x"))"##, "i1 sx");
}

/// Written for every instantiation and for one, in either order.
#[test]
fn a_method_for_every_instantiation_and_for_one_is_refused() {
    let msg = check_err("(defstruct Box<T> (v T))\n(impl Box<T> (pub m (self) int 1))\n(impl Box<int> (pub m (self) int 2))");
    assert!(msg.contains("has a `m` for every instantiation already"), "{}", msg);
    let msg = check_err("(defstruct Box<T> (v T))\n(impl Box<int> (pub m (self) int 2))\n(impl Box<T> (pub m (self) int 1))");
    assert!(msg.contains("`m` is written for `Box<int>` already"), "{}", msg);
    // A field's accessor is a method for every instantiation too.
    let msg = check_err("(defstruct Box<T> (v T))\n(impl Box<int> (pub v (self) int 2))");
    assert!(msg.contains("has a `v` for every instantiation already"), "{}", msg);
}

// ---- trait impls ------------------------------------------------------------

const SHOW: &str = r##"
(deftrait Show () (show ((self Self)) string))
(defstruct Box<T> (v T))
(impl Show Box<int> (show ((self Self)) string (format false "int ~a" self::v)))
(impl Show Box<string> (show ((self Self)) string (format false "string ~a" self::v)))
(defun via-dyn ((s :dyn Show)) string (show s))
(defun via-bound<T> ((s T)) string (:where (Show T)) (show s))
"##;

#[test]
fn each_instantiation_has_its_own_trait_impl() {
    both(SHOW, r##"(format false "~a|~a" (show (make-instance 'Box :v 1)) (show (make-instance 'Box :v "a")))"##, "int 1|string a");
}

#[test]
fn a_trait_object_dispatches_to_its_instantiations_impl() {
    both(
        SHOW,
        r##"(format false "~a|~a" (via-dyn (make-instance 'Box :v 2)) (via-dyn (make-instance 'Box :v "b")))"##,
        "int 2|string b",
    );
    let msg = check_err(&format!("{}(defun f () string (via-dyn (make-instance 'Box :v 1.5)))", SHOW));
    assert!(msg.contains("Box<f64>` does not implement `Show`"), "{}", msg);
}

#[test]
fn a_where_bound_holds_at_each_instantiation_with_an_impl() {
    both(
        SHOW,
        r##"(format false "~a|~a" (via-bound (make-instance 'Box :v 3)) (via-bound (make-instance 'Box :v "c")))"##,
        "int 3|string c",
    );
    let msg = check_err(&format!("{}(defun f () string (via-bound (make-instance 'Box :v 1.5)))", SHOW));
    assert!(msg.contains("Box<f64>` does not implement trait `Show`"), "{}", msg);
}

/// Each instantiation binds the trait's associated types itself.
#[test]
fn each_instantiation_binds_its_own_associated_type() {
    let defs = r##"
(deftrait Unwrap () (:type Item) (unwrap ((self Self)) Item))
(defstruct Box<T> (v T))
(impl Unwrap Box<int> (:type Item int) (unwrap ((self Self)) int (+ self::v 1)))
(impl Unwrap Box<string> (:type Item string) (unwrap ((self Self)) string (format false "~a!" self::v)))
(defun get-int ((u :dyn Unwrap<int>)) int (unwrap u))
"##;
    both(defs, r##"(format false "~a ~a ~a" (unwrap (make-instance 'Box :v 1)) (unwrap (make-instance 'Box :v "s")) (get-int (make-instance 'Box :v 5)))"##, "2 s! 6");
    let msg = check_err(&format!("{}(defun f () int (get-int (make-instance 'Box :v \"s\")))", defs));
    assert!(msg.contains("with `Item = string`, but `:dyn Unwrap` requires `Item = int`"), "{}", msg);
}

/// A supertrait has to be implemented at the instantiation (by its own
/// `impl`, or by one for every instantiation).
#[test]
fn a_supertrait_is_needed_at_the_instantiation() {
    let base = "(deftrait A () (a ((self Self)) int))\n(deftrait B (A) (b ((self Self)) int))\n(defstruct Box<T> (v T))\n";
    both(
        &format!("{}(impl A Box<int> (a ((self Self)) int 1))\n(impl B Box<int> (b ((self Self)) int 2))\n", base),
        r##"(format false "~a" (+ (a (make-instance 'Box :v 0)) (b (make-instance 'Box :v 0))))"##,
        "3",
    );
    both(
        &format!("{}(impl A Box<T> (a ((self Self)) int 1))\n(impl B Box<int> (b ((self Self)) int 2))\n", base),
        r##"(format false "~a" (b (make-instance 'Box :v 0)))"##,
        "2",
    );
    let msg = check_err(&format!("{}(impl A Box<string> (a ((self Self)) int 1))\n(impl B Box<int> (b ((self Self)) int 2))", base));
    assert!(msg.contains("requires `A`"), "{}", msg);
}

#[test]
fn a_trait_for_every_instantiation_and_for_one_is_refused() {
    let base = "(deftrait Show () (show ((self Self)) string))\n(defstruct Box<T> (v T))\n";
    let msg = check_err(&format!(
        "{}(impl Show Box<T> (show ((self Self)) string \"any\"))\n(impl Show Box<int> (show ((self Self)) string \"int\"))",
        base
    ));
    assert!(msg.contains("implements `Show` for every instantiation already"), "{}", msg);
    let msg = check_err(&format!(
        "{}(impl Show Box<int> (show ((self Self)) string \"int\"))\n(impl Show Box<T> (show ((self Self)) string \"any\"))",
        base
    ));
    assert!(msg.contains("`Show` is implemented for `Box<int>` already"), "{}", msg);
}

/// A blanket impl materialized at two instantiations gives each its own
/// methods; the second used to replace the first.
#[test]
fn a_blanket_impl_at_two_instantiations_keeps_both() {
    let defs = r##"
(deftrait Ranked () (rank ((self Self)) int))
(deftrait Twice (Ranked) (twice-rank ((self Self)) int (* 2 (rank self))))
(defstruct Box<T> (v T))
(impl Ranked Box<int> (rank ((self Self)) int self::v))
(impl Ranked Box<string> (rank ((self Self)) int 100))
(impl<T> Twice T (:where (Ranked T)))
"##;
    both(defs, r##"(format false "~a ~a" (twice-rank (make-instance 'Box :v 4)) (twice-rank (make-instance 'Box :v "s")))"##, "8 200");
}

/// `print-object` written for each instantiation prints each its own way.
#[test]
fn print_object_for_each_instantiation() {
    let defs = r##"
(defstruct Box<T> (v T))
(impl print-object Box<int> (print-object ((self Self) (escape bool)) string (format false "#<int-box ~a>" self::v)))
(impl print-object Box<string> (print-object ((self Self) (escape bool)) string (format false "#<str-box ~a>" self::v)))
"##;
    both(defs, r##"(format false "~a ~a" (make-instance 'Box :v 1) (make-instance 'Box :v "s"))"##, "#<int-box 1> #<str-box s>");
}

// ---- naming one body ----------------------------------------------------------

const DOCUMENTED: &str = r##"
(defstruct Box<T> (v T))
(impl Box<int> (pub twice (self) int "doubles the int" (* 2 self::v)))
(impl Box<string> (pub twice (self) string "repeats the string" (format false "~a~a" self::v self::v)))
(impl Box<f64> (pub half (self) f64 "halves the float" (/ self::v 2.0)))
"##;

/// `Type<args>::method` names one instantiation's method; `Type::method`
/// does too when only one instantiation wrote it.
#[test]
fn documentation_names_one_instantiations_method() {
    let doc = |name: &str| {
        eval_string(&format!("{}(match (documentation {}) ((some d) d) (none \"none\"))", DOCUMENTED, name))
    };
    assert_eq!(doc("Box<int>::twice"), "doubles the int");
    assert_eq!(doc("Box<string>::twice"), "repeats the string");
    assert_eq!(doc("Box::half"), "halves the float");
    let msg = check_err(&format!("{}(documentation Box::twice)", DOCUMENTED));
    assert!(msg.contains("is written for more than one instantiation"), "{}", msg);
    assert!(msg.contains("`Box<int>::twice`") && msg.contains("`Box<string>::twice`"), "{}", msg);
    let msg = check_err(&format!("{}(documentation Box<f64>::twice)", DOCUMENTED));
    assert!(msg.contains("`Box<f64>` has no method `twice` of its own"), "{}", msg);
}

/// `compile` takes the same names.
#[test]
fn compile_names_one_instantiations_method() {
    assert_eq!(
        eval_string_compiled(&format!(
            "{}(progn (compile Box<int>::twice) (compile Box::half) (format false \"~a ~a\" (twice (make-instance 'Box :v 3)) (half (make-instance 'Box :v 3.0))))",
            DOCUMENTED
        )),
        "6 1.5"
    );
    let msg = check_err(&format!("{}(compile Box::twice)", DOCUMENTED));
    assert!(msg.contains("is written for more than one instantiation"), "{}", msg);
}

/// `(setf (ACCESSOR x) v)` finds a `set-ACCESSOR` one instantiation's `impl`
/// defines, as a call finds that instantiation's methods.
#[test]
fn setf_finds_a_setter_of_one_instantiation() {
    both(
        r##"
(defstruct Box<T> (v T))
(impl Box<int>
  (pub doubled (self) int (* 2 self::v))
  (pub set-doubled (self (n int)) int (progn (setf self::v (truncate n 2)) n)))
"##,
        "(let ((b (make-instance 'Box<int> :v 1))) (progn (setf (doubled b) 10) (format false \"~a\" (doubled b))))",
        "10",
    );
}

/// A macro cannot take over a special form's name — at top level, where the
/// loader expands macro calls first, the same as inside a function.
#[test]
fn a_macro_does_not_take_over_a_special_form_at_top_level() {
    let src = r#"(defmacro block (name &rest body) "taken over")
(block outer "the special form")"#;
    assert_eq!(eval_string(src), "the special form");
}
