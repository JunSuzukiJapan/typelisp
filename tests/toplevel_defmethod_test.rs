//! `defmethod` outside an `impl`: a method of its first parameter's type,
//! which may be any type with methods — one the standard library defines, or
//! another module's. Methods are looked up from the first argument's type
//! alone, so one name can have a method on each type.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

/// `body` (a `string`-valued expression) after `defs`, interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(defs: &str, body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", defs, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", defs, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

/// One name, a method on each of two standard library types, chosen by the
/// first argument's type.
#[test]
fn one_name_on_two_standard_library_types() {
    let defs = r##"
(defmethod some-method ((x int) (y int)) int (+ x y))
(defmethod some-method ((x f64) (y f64)) f64 (* x y))
"##;
    both(defs, r##"(format false "~a ~a" (some-method 2 3) (some-method 2.0 3.0))"##, "5 6.0");
}

/// A program's own type and a generic standard library type.
#[test]
fn on_a_user_type_and_a_generic_type() {
    let defs = r##"
(defstruct dog (pub name string))
(defmethod hello ((d dog) (to string)) string (append d::name " greets " to))
(defmethod second-elem<T> ((v Vector<T>)) T (get v 1))
"##;
    both(
        defs,
        r##"(format false "~a / ~a" (hello (make-instance 'dog :name "pochi") "tama") (second-elem #(1 2 3)))"##,
        "pochi greets tama / 2",
    );
}

/// `pub defmethod` is usable from another module; without `pub` it is not.
#[test]
fn pub_decides_whether_another_module_sees_it() {
    let src = r##"
(module m
  (pub defmethod twice ((x int)) int (* 2 x))
  (defmethod thrice ((x int)) int (* 3 x)))
(format false "~a" (twice 21))
"##;
    assert_eq!(eval_string(src), "42");
    let msg = check_err(
        r##"
(module m (defmethod thrice ((x int)) int (* 3 x)))
(defun f () int (thrice 1))
"##,
    );
    assert!(msg.contains("thrice"), "{}", msg);
}

/// `self` belongs to an `impl`; outside one the receiver is named and typed.
#[test]
fn self_is_refused_outside_an_impl() {
    for src in ["(defmethod f (self) int 1)", "(defmethod f ((self int)) int 1)"] {
        let msg = check_err(src);
        assert!(msg.contains("`self` is written only inside an `impl`"), "{}: {}", src, msg);
    }
}

/// The first parameter is the receiver and must say its type.
#[test]
fn the_receiver_needs_a_type() {
    for src in ["(defmethod f () int 1)", "(defmethod f (x) int 1)"] {
        let msg = check_err(src);
        assert!(msg.contains("defmethod `f`"), "{}: {}", src, msg);
    }
}

/// A receiver typed by one of the method's own type parameters makes the
/// method every type's, specialized at each call like a generic function.
#[test]
fn a_type_parameter_receiver_is_every_types() {
    let defs = r##"
(defmethod pair-up<T, U> ((t T) (u U)) (T U) `(,t ,u))
"##;
    both(defs, r##"(format false "~a ~a" (pair-up 1 "a") (pair-up "x" 2.5))"##, "(1 a) (x 2.5)");
}

/// A type's own method of the name is looked up before every type's, and
/// defining both is no conflict, in either order.
#[test]
fn a_types_own_method_comes_first() {
    let defs = r##"
(defstruct pt (x int))
(defmethod describe ((p pt)) string "a point")
(defmethod describe<T> ((v T)) string "something")
(defmethod describe ((n int)) string "an int")
"##;
    both(
        defs,
        r##"(format false "~a / ~a / ~a" (describe (make-instance 'pt :x 1)) (describe 1) (describe "s"))"##,
        "a point / an int / something",
    );
}

/// In a generic body, a bound on the type parameter that gives the method
/// comes before every type's.
#[test]
fn a_bound_comes_first_in_a_generic_body() {
    let defs = r##"
(deftrait Named () (defmethod label (self) string))
(impl Named int (defmethod label (self) string "int's"))
(defmethod label<T> ((v T)) string "every type's")
(defun via-bound<T> ((x T)) string (:where (Named T)) (label x))
"##;
    both(defs, r##"(via-bound 1)"##, "int's");
}

/// A generic body's call of every type's method is the receiver's own once
/// the body is specialized at a type that has one: in a call, in a method
/// taken as a value, and in a `~/name/` directive.
#[test]
fn a_specialized_receiver_calls_its_own_method() {
    let defs = r##"
(defmethod f<T, U> ((t T) (u U)) string "every type's")
(defmethod f ((x int) (y int)) string (format false "int's ~a" (+ x y)))
(defun g<T> ((x T)) string (f x x))
(defmethod show<T> ((v T) (colon bool) (at bool)) string "every")
(defmethod show ((v int) (colon bool) (at bool)) string "int")
(defun shown<T> ((x T)) string (format false "~/show/" x))
(defun call1<T> ((h (fn (T T) string)) (x T)) string (h x x))
(defun as-value<T> ((x T)) string (call1 f x))
"##;
    both(
        defs,
        r##"(format false "~a / ~a / ~a / ~a / ~a / ~a" (g 4) (g "s") (shown 1) (shown "s") (as-value 2) (as-value "s"))"##,
        "int's 8 / every type's / int / every / int's 4 / every type's",
    );
}

/// A trait object specialized for the type parameter calls its trait's
/// method of the name through its table.
#[test]
fn a_specialized_trait_object_calls_its_traits_method() {
    let defs = r##"
(deftrait Named () (defmethod label (self) string))
(defstruct cat (n int))
(impl Named cat (defmethod label (self) string "cat"))
(defmethod label<T> ((v T)) string "every type's")
(defun unbound<T> ((x T)) string (label x))
(defun boxed ((d :dyn Named)) string (unbound d))
"##;
    both(defs, r##"(format false "~a / ~a" (boxed (make-instance 'cat :n 1)) (unbound 1))"##, "cat / every type's");
}

/// The receiver's own method has to take and give what every type's does
/// at the call's types; otherwise the specialization is refused.
#[test]
fn an_own_method_of_another_signature_is_refused() {
    let msg = check_err(
        r##"
(defmethod f<T> ((v T)) T v)
(defmethod f ((x f64)) int 1)
(defun g<T> ((x T)) T (f x))
(defun h () f64 (g 1.5))
"##,
    );
    assert!(msg.contains("does not take and give what every type's does"), "{}", msg);
}

/// A type's own method the body's module does not see is not called.
#[test]
fn an_unseen_own_method_is_not_called() {
    let src = r##"
(defstruct pt (x int))
(defmethod d ((p pt)) string "pt's")
(pub defmethod d<T> ((v T)) string "every type's")
(module m (pub defun g<T> ((x T)) string (d x)))
(format false "~a / ~a" (m::g (make-instance 'pt :x 1)) (d (make-instance 'pt :x 1)))
"##;
    assert_eq!(eval_string(src), "every type's / pt's");
}

/// Without `pub`, every type's method is seen only from the module it is
/// written in.
#[test]
fn every_types_method_needs_pub_for_another_module() {
    let src = r##"
(module m (pub defmethod shown<T> ((v T)) int 2))
(format false "~a" (shown "a"))
"##;
    assert_eq!(eval_string(src), "2");
    let msg = check_err(
        r##"
(module m (defmethod hidden<T> ((v T)) int 1))
(defun f () int (hidden 1))
"##,
    );
    assert!(msg.contains("hidden"), "{}", msg);
}
