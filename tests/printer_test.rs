//! Tests for the one-object printers and the `~/name/` directive —
//! cl-parity-plan.md Phase 8a.
//!
//! `prin1`/`princ`/`write` and the three `-to-string` forms are macros over
//! `format`, so what is worth testing is the *choice* each makes (`~s` vs
//! `~a`), that they return their object the way CL's do, and that the stream
//! argument is optional. `~/name/` is the one new piece of machinery: it
//! resolves a name against the argument's own type at run time, which is a
//! deliberate departure from CL's global-function lookup (see
//! `PrintEnv::format_call` for why the CL reading is not reachable here).

extern crate typelisp;

mod common;
use common::is_true;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader};

/// The first *check*-time error `src` raises.
///
/// Since the control string became a literal (`Checker::control_string`), a
/// `~/name/` that names nothing, names something of the wrong shape, or sits
/// in a control string that does not parse is caught here rather than in the
/// middle of printing — the whole point of scanning it at compile time. These
/// used to be [`eval_err`] cases.
fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Err(e) => return format!("{:?}", e),
            Ok(tl) => {
                interp.exec(&mut h, tl).expect("exec failed");
            }
        }
    }
    panic!("expected a check error, got none");
}

// ---- prin1 / princ / write ---------------------------------------------------

#[test]
fn prin1_to_string_writes_reader_syntax() {
    is_true(r#"(equal (prin1-to-string "hi") "\"hi\"")"#);
}

#[test]
fn princ_to_string_writes_human_text() {
    is_true(r#"(equal (princ-to-string "hi") "hi")"#);
}

#[test]
fn the_two_agree_on_a_number() {
    is_true(r#"(equal (prin1-to-string 42) (princ-to-string 42))"#);
}

#[test]
fn prin1_returns_its_object() {
    is_true(r#"(let ((s (make-string-output-stream))) (= (prin1 7 s) 7))"#);
}

#[test]
fn princ_returns_its_object() {
    is_true(r#"(let ((s (make-string-output-stream))) (equal (princ "x" s) "x"))"#);
}

#[test]
fn write_returns_its_object() {
    is_true(r#"(let ((s (make-string-output-stream))) (= (write 7 s) 7))"#);
}

#[test]
fn prin1_writes_to_the_stream_it_is_given() {
    is_true(
        r#"(let ((s (make-string-output-stream)))
             (progn (prin1 "hi" s) (equal (get-output-stream-string s) "\"hi\"")))"#,
    );
}

#[test]
fn princ_writes_to_the_stream_it_is_given() {
    is_true(
        r#"(let ((s (make-string-output-stream)))
             (progn (princ "hi" s) (equal (get-output-stream-string s) "hi")))"#,
    );
}

#[test]
fn a_stream_argument_is_evaluated_once() {
    // The macro binds the object, not the stream, so the stream form is
    // written into the expansion once and evaluated once by `format`.
    is_true(
        r#"(let ((s (make-string-output-stream)) (n 0))
             (progn (prin1 1 (progn (setf n (+ n 1)) s)) (= n 1)))"#,
    );
}

#[test]
fn the_object_form_is_evaluated_once() {
    is_true(
        r#"(let ((s (make-string-output-stream)) (n 0))
             (progn (prin1 (progn (setf n (+ n 1)) 5) s) (= n 1)))"#,
    );
}

// ---- *print-escape* ---------------------------------------------------------

#[test]
fn print_escape_starts_true() {
    is_true("(equal *print-escape* true)");
}

#[test]
fn write_to_string_escapes_by_default() {
    is_true(r#"(equal (write-to-string "hi") "\"hi\"")"#);
}

#[test]
fn write_to_string_stops_escaping_when_the_variable_says_so() {
    is_true(r#"(dlet ((*print-escape* false)) (equal (write-to-string "hi") "hi"))"#);
}

#[test]
fn write_follows_the_variable_too() {
    is_true(
        r#"(dlet ((*print-escape* false))
             (let ((s (make-string-output-stream)))
               (progn (write "hi" s) (equal (get-output-stream-string s) "hi"))))"#,
    );
}

#[test]
fn prin1_ignores_the_variable() {
    // `~s` binds `*print-escape*` to true for its own call, CL says, so
    // `prin1` is unaffected by what the variable happens to hold.
    is_true(r#"(dlet ((*print-escape* false)) (equal (prin1-to-string "hi") "\"hi\""))"#);
}

#[test]
fn princ_ignores_the_variable() {
    is_true(r#"(dlet ((*print-escape* true)) (equal (princ-to-string "hi") "hi"))"#);
}

#[test]
fn with_standard_io_syntax_restores_the_escape_flag() {
    is_true(
        r#"(dlet ((*print-escape* false))
             (with-standard-io-syntax (equal (write-to-string "hi") "\"hi\"")))"#,
    );
}

// ---- ~/name/ ----------------------------------------------------------------

const POINT: &str = r#"
(defstruct point (x int) (y int))
(impl point
  (brief (self (colon bool) (at bool)) string
    (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y))))
"#;

#[test]
fn a_call_directive_runs_the_method_on_the_arguments_type() {
    is_true(&format!(
        r#"{} (equal (format false "~/brief/" (make-instance 'point :x 3 :y 4)) "3/4")"#,
        POINT
    ));
}

#[test]
fn the_colon_flag_reaches_the_method() {
    is_true(&format!(
        r#"{} (equal (format false "~:/brief/" (make-instance 'point :x 3 :y 4)) "<3,4>")"#,
        POINT
    ));
}

#[test]
fn the_at_flag_reaches_the_callee() {
    is_true(
        r#"(defun flags ((s string) (colon bool) (at bool)) string (if at "at" "plain"))
           (equal (format false "~@/flags/" "x") "at")"#,
    );
}

#[test]
fn a_call_directive_sits_inside_surrounding_text() {
    is_true(&format!(
        r#"{} (equal (format false "p=~/brief/!" (make-instance 'point :x 1 :y 2)) "p=1/2!")"#,
        POINT
    ));
}

#[test]
fn a_call_directive_consumes_exactly_one_argument() {
    is_true(&format!(
        r#"{} (equal (format false "~a ~/brief/ ~a" 1 (make-instance 'point :x 2 :y 3) 4) "1 2/3 4")"#,
        POINT
    ));
}

/// A type that takes no methods from a program — a built-in one — is
/// reached through a function: the directive calls what `(shout arg colon
/// at)` would call at the site.
#[test]
fn a_call_directive_calls_a_function() {
    is_true(
        r#"(defun shout ((s string) (colon bool) (at bool)) string (append s "!"))
           (equal (format false "~/shout/" "hi") "hi!")"#,
    );
}

#[test]
fn an_integer_argument_reaches_a_function_taking_an_integer() {
    is_true(
        r#"(defun twice ((n int) (colon bool) (at bool)) string (format false "~a ~a" n n))
           (equal (format false "~/twice/" 7) "7 7")"#,
    );
}

/// A narrow integer reaches a function taking its own width with its value
/// intact: in the argument list it is a box naming its width, and the
/// function takes the plain word.
#[test]
fn a_narrow_integer_reaches_a_function_taking_its_width() {
    is_true(
        r#"(defun thrice ((n u8) (colon bool) (at bool)) string (format false "~a ~a ~a" n n n))
           (equal (format false "~/thrice/" (the u8 200)) "200 200 200")"#,
    );
}

/// An argument of another width is a type error, as the call `(thrice 7
/// colon at)` would be: a bare literal is an `int`.
#[test]
fn an_argument_of_another_width_is_a_type_error() {
    let e = check_err(
        r#"(defun thrice ((n u8) (colon bool) (at bool)) string (format false "~a" n))
           (format false "~/thrice/" 7)"#,
    );
    assert!(e.contains("~/thrice/ calls what (thrice arg colon at) with arg a `int` would"), "{}", e);
}

#[test]
fn a_missing_name_is_an_error_not_a_fallback() {
    // Unlike `print-object`, which has the built-in rendering to fall back
    // to: the control string asked for something by name. Reported at check
    // time — the argument's type is known there.
    let e = check_err(r#"(format false "~/nope/" "hi")"#);
    assert!(e.contains("~/nope/ calls what (nope arg colon at) with arg a `string` would"), "{}", e);
}

#[test]
fn a_callee_of_the_wrong_shape_is_reported_as_such() {
    let e = check_err(
        r#"(defun short ((s string)) string s)
           (format false "~/short/" "hi")"#,
    );
    assert!(e.contains("(short arg colon at)"), "{}", e);
}

/// The argument's type is the receiver, as in a call: its method comes
/// before a function of the same name, and an argument whose type has no
/// such method reaches the function.
#[test]
fn a_method_of_the_arguments_type_comes_before_a_function() {
    is_true(&format!(
        r#"{} (defun brief ((s string) (colon bool) (at bool)) string (append "s:" s))
              (equal (format false "~/brief/ ~/brief/" (make-instance 'point :x 3 :y 4) "t") "3/4 s:t")"#,
        POINT
    ));
}

/// A function in a module is named by its path, as in a call.
#[test]
fn a_call_directive_names_a_function_by_its_path() {
    is_true(
        r#"(module m (pub defun shout ((s string) (colon bool) (at bool)) string (append s "!")))
           (equal (format false "~/m::shout/" "hi") "hi!")"#,
    );
}

/// Inside `~{` the directive is handed the list's elements, whose type is
/// only `Sexpr` — no receiver to resolve the name against, so it is refused.
#[test]
fn a_call_directive_on_list_elements_is_refused() {
    let e = check_err(&format!(
        r#"{} (format false "~{{~/brief/~}}" (list (make-instance 'point :x 1 :y 2)))"#,
        POINT
    ));
    assert!(e.contains("element of a list argument"), "{}", e);
}

/// `~@{` goes round the call's own arguments, whose types are known.
#[test]
fn a_call_directive_over_the_remaining_arguments_is_resolved() {
    is_true(
        r#"(defun shout ((s string) (colon bool) (at bool)) string (append s "!"))
           (equal (format false "~@{~/shout/ ~}" "a" "b") "a! b! ")"#,
    );
}

/// One directive calls one definition: one that can be handed arguments of
/// two types calling different definitions is refused.
#[test]
fn a_directive_that_would_call_two_definitions_is_refused() {
    let e = check_err(&format!(
        r#"{} (defun brief ((s string) (colon bool) (at bool)) string s)
              (format false "~:[~;~*~]~/brief/" true "s" (make-instance 'point :x 1 :y 2))"#,
        POINT
    ));
    assert!(e.contains("which call different definitions"), "{}", e);
    // In the source's spelling: `''` is the method's internal name.
    assert!(e.contains("point::brief`)") && !e.contains("''"), "{}", e);
}

#[test]
fn an_unterminated_call_directive_is_a_syntax_error() {
    let e = check_err(r#"(format false "~/brief" "hi")"#);
    assert!(e.contains("closing"), "{}", e);
}

/// The control string is a literal, like Rust's `format!` — a string built at
/// run time cannot be scanned for the directives that decide what `~/name/`
/// calls, and resolving that at check time is what makes the directive work
/// at all.
#[test]
fn a_computed_control_string_is_refused() {
    let e = check_err(r#"(defun go ((ctrl string)) string (format false ctrl))"#);
    assert!(e.contains("must be a literal"), "{}", e);
}

#[test]
fn the_name_is_case_sensitive_like_every_other_name() {
    is_true(
        r#"(defun shout ((s string) (colon bool) (at bool)) string (append s "!"))
           (equal (format false "~/shout/" "hi") "hi!")"#,
    );
    let e = check_err(
        r#"(defun shout ((s string) (colon bool) (at bool)) string (append s "!"))
           (format false "~/SHOUT/" "hi")"#,
    );
    assert!(e.contains("SHOUT"), "{}", e);
}

#[test]
fn a_call_directive_works_through_a_stream_destination() {
    is_true(&format!(
        r#"{} (let ((s (make-string-output-stream)))
                (progn (format s "~/brief/" (make-instance 'point :x 5 :y 6))
                       (equal (get-output-stream-string s) "5/6")))"#,
        POINT
    ));
}

// ---- print-object on a generic type ----------------------------------------

/// The printer looks a method up by the key the value carries, and that key
/// includes the instantiation now (`gen<int>`), so the body monomorphization
/// registered for it (`print-object <int>`) is findable.
///
/// This test used to assert the opposite, pinning a known gap: the key was
/// `gen`, the registration was `print-object <int>`, and the impl silently
/// never applied — while still type-checking and still working when called by
/// name, which is what made it a trap rather than a missing feature.
#[test]
fn print_object_reaches_a_generic_type() {
    is_true(
        r#"(defstruct gen<T> (v T))
           (impl print-object gen<T> (print-object ((self Self) (escape bool)) string "GEN"))
           (equal (format false "~a" (make-instance 'gen :v 1)) "GEN")"#,
    );
    // Still reachable by name, as it always was.
    is_true(
        r#"(defstruct gen<T> (v T))
           (impl print-object gen<T> (print-object ((self Self) (escape bool)) string "GEN"))
           (equal (print-object (make-instance 'gen :v 1) true) "GEN")"#,
    );
}

/// Each instantiation gets *its own* body — what a key with no type arguments
/// could not express even in principle, since one key would have to choose
/// between two compiled bodies.
#[test]
fn each_instantiation_reaches_its_own_print_object() {
    is_true(
        r#"(defstruct wi (n int))
           (impl print-object wi (print-object ((self Self) (escape bool)) string "INT"))
           (defstruct ws (s string))
           (impl print-object ws (print-object ((self Self) (escape bool)) string "STR"))
           (defstruct gen<T> (v T))
           (impl print-object gen<T>
             (:where (print-object T))
             (print-object ((self Self) (escape bool)) string
               (append "gen-of-" (print-object (v self) escape))))
           (and (equal (format false "~a" (make-instance 'gen :v (make-instance 'wi :n 1))) "gen-of-INT")
                (equal (format false "~a" (make-instance 'gen :v (make-instance 'ws :s "x"))) "gen-of-STR"))"#,
    );
}

/// A built-in generic container reaches its own `print-object` too. `Vector`
/// builds its box in Rust with no `construct` node, so the specialization the
/// printer needs is requested from the `assoc` node instead — without that the
/// impl type-checks, is callable by name, and is silently never selected.
#[test]
fn print_object_reaches_a_builtin_generic_type() {
    is_true(
        r#"(defstruct point (x int) (y int))
           (impl print-object point (print-object ((self Self) (escape bool)) string "P"))
           (impl print-object Vector<T> (:where (print-object T))
             (print-object ((self Self) (escape bool)) string "V"))
           (equal (format false "~a" (the Vector<point> (Vector::new))) "V")"#,
    );
}

/// An enum's variant *name* still resolves once the value's key carries an
/// instantiation.
///
/// Variant names are registered per enum *type* (`option`), while the key a
/// value carries names an instantiation (`Option<int>`). Looking one up
/// without splitting the base off printed every `Option` as
/// `(<unknown-variant> 1)` — 28 tests across nine suites, and none of them was
/// about printing an `Option`, which is why the whole class only showed up in
/// the serial full run.
#[test]
fn an_enum_variant_name_survives_an_instantiated_key() {
    is_true(r#"(equal (format false "~a" (Option::some 1)) "(some 1)")"#);
    is_true(r#"(equal (format false "~a" (the Option<int> (Option::none))) "none")"#);
    is_true(
        r#"(defenum box<T> (full T) (empty))
           (equal (format false "~a" (the box<string> (box::full "x"))) "(full x)")"#,
    );
}

// ---- `Array<T>` and `*print-array*` ------------------------------------------
//
// The remainder of Phase 8a, deferred until a generic type's `print-object`
// could be reached at all. What makes an array printable in generic code is
// the trait used as a *bound*: `(impl print-object Array<T> (:where
// (print-object T)))` needs its elements renderable, so the scalar types
// implement the trait for that purpose alone (the printer never consults it
// for them — a scalar carries no type key).

/// CL's array syntax: `#(…)` at rank 1, `#nA` plus one nesting level per
/// dimension otherwise, and `#0A` on the lone element of a rank-0 array.
#[test]
fn an_array_prints_in_cl_syntax() {
    is_true(
        r##"(let ((d (the Vector<int> (Vector::new))))
             (progn (push d 2) (push d 3)
               (let ((a (Array::new d 0)))
                 (progn (setf (aref a 0 0) 7)
                        (equal (format false "~a" a) "#2A((7 0 0) (0 0 0))")))))"##,
    );
    is_true(
        r##"(let ((d (the Vector<int> (Vector::new))))
             (progn (push d 3)
               (equal (format false "~a" (Array::new d 1)) "#(1 1 1)")))"##,
    );
    is_true(
        r##"(let ((d (the Vector<int> (Vector::new))))
             (equal (format false "~a" (Array::new d 5)) "#0A5"))"##,
    );
}

/// `escape` reaches the elements, so `~s` prints an array of strings the way
/// the reader would want them back and `~a` prints them bare.
#[test]
fn an_arrays_elements_follow_the_escape_choice() {
    is_true(
        r##"(let ((d (the Vector<int> (Vector::new))))
             (progn (push d 2)
               (let ((a (Array::new d "q")))
                 (and (equal (format false "~s" a) "#(\"q\" \"q\")")
                      (equal (format false "~a" a) "#(q q)")))))"##,
    );
}

/// A fill pointer cuts the printed contents, as CL's printer does: past it
/// the storage is not part of the array's contents.
#[test]
fn a_fill_pointer_cuts_what_prints() {
    is_true(
        r##"(let ((d (the Vector<int> (Vector::new))))
             (progn (push d 4)
               (equal (format false "~a" (Array::new d 0 :fill-pointer 2)) "#(0 0)")))"##,
    );
}

/// `*print-array*` false prints the shape instead of the contents — CL's "in
/// a way that does not reveal the contents", spelled for the one array type
/// this language has.
#[test]
fn print_array_false_shows_only_the_shape() {
    is_true(
        r##"(let ((d (the Vector<int> (Vector::new))))
             (progn (push d 2) (push d 3)
               (let ((a (Array::new d 0)))
                 (dlet ((*print-array* false))
                   (equal (format false "~a" a) "#<array 2x3>")))))"##,
    );
    // And it is scoped: `dlet` restores it.
    is_true(r##"*print-array*"##);
}

/// An element type that never implemented the trait leaves the impl
/// inapplicable, and the array prints the built-in way. What matters is that
/// this is not an *error*: the specialization the printer would need is
/// requested by nobody the user can see, so an unsatisfiable request has to be
/// dropped rather than reported at the site that merely built the array.
#[test]
fn an_array_of_a_type_with_no_print_object_still_builds() {
    is_true(
        r##"(defstruct plain (n int))
           (let ((d (the Vector<int> (Vector::new))))
             (progn (push d 1)
               (let ((a (Array::new d (make-instance 'plain :n 1))))
                 (equal (format false "~a" a)
                        "#<Array<plain> dims: #(1) data: #(#<plain n: 1>) fill-pointer: none>"))))"##,
    );
}

/// The bound is what generic code asks for, and the scalars answer it —
/// including through a container of containers, where the element being
/// rendered is itself an `Array`.
#[test]
fn arrays_nest_through_the_bound() {
    is_true(
        r##"(let ((d (the Vector<int> (Vector::new))))
             (progn (push d 2)
               (let ((inner (Array::new d 1)))
                 (equal (format false "~a" (Array::new d inner)) "#(#(1 1) #(1 1))"))))"##,
    );
}

/// A scalar's own impl is callable by name and says what the printer would
/// have said — the point of having it at all, since the printer itself never
/// consults the trait for a value that carries no type key.
#[test]
fn the_scalar_impls_render_what_the_printer_renders() {
    is_true(r##"(equal (print-object 5 false) "5")"##);
    is_true(r##"(equal (print-object "x" true) "\"x\"")"##);
    is_true(r##"(equal (print-object "x" false) "x")"##);
    is_true(r##"(equal (print-object #\a true) "#\\a")"##);
}

/// A `where`-bounded receiver keeps its method even when some *other* type
/// implements the same trait with a matching argument in the second position:
/// the method comes from the first argument's type only, so
/// `(print-object x true)` under `(:where (print-object T))` never resolves
/// against `bool`'s impl.
#[test]
fn a_bound_method_is_not_re_read_with_swapped_arguments() {
    is_true(
        r##"(defun render<T> ((x T)) string (:where (print-object T)) (print-object x true))
           (equal (render 7) "7")"##,
    );
}

// ---- type names are written from the current module ------------------------

/// A struct's name is printed the way the module the program is in can write
/// it: its own definitions by the last segment, a module below it by the path
/// from it, anything else from the root.
#[test]
fn a_type_name_is_printed_from_the_current_module() {
    is_true(
        r##"(module m (pub defstruct p (pub n int)) (module k (pub defstruct q (pub n int))))
           (module r (pub defun show () string
                       (format false "~a ~a" (make-instance 'm::p :n 1) (make-instance 'm::k::q :n 2))))
           (equal (r::show) "#<m::p n: 1> #<m::k::q n: 2>")"##,
    );
    is_true(
        r##"(module m (pub defstruct p (pub n int))
                     (module k (pub defstruct q (pub n int)))
                     (pub defun show () string
                       (format false "~a ~a" (make-instance 'p :n 1) (make-instance 'k::q :n 2))))
           (in-module m)
           (equal (show) "#<p n: 1> #<k::q n: 2>")"##,
    );
}

/// A type the current module has brought in with `use` is written by its last
/// segment, as the module writes it.
#[test]
fn a_type_brought_in_is_printed_by_its_last_segment() {
    is_true(
        r##"(module m (pub defstruct p (pub n int)))
           (use m::p)
           (equal (format false "~a" (make-instance 'p :n 1)) "#<p n: 1>")"##,
    );
}

/// An error message writes a type the same way.
#[test]
fn an_error_message_writes_a_type_from_the_current_module() {
    let e = check_err(
        r##"(module m (pub defstruct p (pub n int)) (module k (pub defstruct q (pub n int))))
           (module m (defun f () int (make-instance 'k::q :n 1)))"##,
    );
    assert!(e.contains("k::q") && !e.contains("m::k::q"), "{}", e);
}
