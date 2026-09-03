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
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

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

fn is_true(src: &str) {
    assert_eq!(eval_ok(src), Value::Bool(true), "{}", src);
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
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
"#;

#[test]
fn a_call_directive_runs_the_method_on_the_arguments_type() {
    is_true(&format!(
        r#"{} (equal (format false "~/brief/" (point::new 3 4)) "3/4")"#,
        POINT
    ));
}

#[test]
fn the_colon_flag_reaches_the_method() {
    is_true(&format!(
        r#"{} (equal (format false "~:/brief/" (point::new 3 4)) "<3,4>")"#,
        POINT
    ));
}

#[test]
fn the_at_flag_reaches_the_method() {
    is_true(
        r#"(defmethod flags ((self string) (colon bool) (at bool)) string
             (if at "at" "plain"))
           (equal (format false "~@/flags/" "x") "at")"#,
    );
}

#[test]
fn a_call_directive_sits_inside_surrounding_text() {
    is_true(&format!(
        r#"{} (equal (format false "p=~/brief/!" (point::new 1 2)) "p=1/2!")"#,
        POINT
    ));
}

#[test]
fn a_call_directive_consumes_exactly_one_argument() {
    is_true(&format!(
        r#"{} (equal (format false "~a ~/brief/ ~a" 1 (point::new 2 3) 4) "1 2/3 4")"#,
        POINT
    ));
}

#[test]
fn a_string_argument_dispatches_on_string() {
    is_true(
        r#"(defmethod shout ((self string) (colon bool) (at bool)) string (append self "!"))
           (equal (format false "~/shout/" "hi") "hi!")"#,
    );
}

#[test]
fn an_integer_argument_dispatches_when_only_one_width_defines_the_name() {
    is_true(
        r#"(defmethod twice ((self i32) (colon bool) (at bool)) string (format false "~a ~a" self self))
           (equal (format false "~/twice/" 7) "7 7")"#,
    );
}

/// Two widths defining the same name is not ambiguous: each argument says
/// which type it is, so each reaches its own method.
///
/// This used to be an *error* — "could be any width" — because a normalized
/// `u8` and a normalized `i32` were the same `Value::Int` when they held the
/// same small number, and the runtime offered all six integer types as
/// candidates rather than guess. Giving the narrow widths a box that names
/// them settled it: `7` is an `i32` and `(the u8 7)` is a `u8`.
#[test]
fn each_integer_width_dispatches_to_its_own_method() {
    const DEFS: &str = r#"(defmethod both ((self i32) (colon bool) (at bool)) string "32")
           (defmethod both ((self u8) (colon bool) (at bool)) string "8")"#;
    is_true(&format!(r#"{DEFS}
           (equal (format false "~/both/" 7) "32")"#));
    is_true(&format!(r#"{DEFS}
           (equal (format false "~/both/" (the u8 7)) "8")"#));
}

/// A method on a width other than the argument's own is *not* reached — the
/// argument's type is the whole of the dispatch, and a bare literal is an
/// `i32`.
#[test]
fn an_integer_argument_reaches_only_its_own_widths_method() {
    const DEF: &str = r#"(defmethod thrice ((self u8) (colon bool) (at bool)) string (format false "~a ~a ~a" self self self))"#;
    let e = check_err(&format!(r#"{DEF}
           (format false "~/thrice/" 7)"#));
    assert!(e.contains("no argument here has a method `thrice`"), "{}", e);
    is_true(&format!(r#"{DEF}
           (equal (format false "~/thrice/" (the u8 7)) "7 7 7")"#));
}

#[test]
fn a_missing_method_is_an_error_not_a_fallback() {
    // Unlike `print-object`, which has the built-in rendering to fall back
    // to: the control string asked for something by name. Reported at check
    // time now — the scan of the literal knows the argument's type.
    let e = check_err(r#"(format false "~/nope/" "hi")"#);
    assert!(e.contains("no argument here has a method `nope`"), "{}", e);
}

#[test]
fn a_method_of_the_wrong_shape_is_reported_as_such() {
    let e = check_err(
        r#"(defmethod short ((self string)) string self)
           (format false "~/short/" "hi")"#,
    );
    assert!(e.contains("(colon bool) (at bool)"), "{}", e);
}

#[test]
fn an_unterminated_call_directive_is_a_syntax_error() {
    let e = check_err(r#"(format false "~/brief" "hi")"#);
    assert!(e.contains("closing"), "{}", e);
}

/// The control string is a literal, like Rust's `format!` — a string built at
/// run time cannot be scanned for the directives that decide which methods
/// `~/name/` reaches, and scanning is what makes the directive work in an AOT
/// executable at all.
#[test]
fn a_computed_control_string_is_refused() {
    let e = check_err(r#"(defun go ((ctrl string)) string (format false ctrl))"#);
    assert!(e.contains("must be a literal"), "{}", e);
}

#[test]
fn the_name_is_case_folded_like_every_other_name() {
    is_true(
        r#"(defmethod shout ((self string) (colon bool) (at bool)) string (append self "!"))
           (equal (format false "~/SHOUT/" "hi") "hi!")"#,
    );
}

#[test]
fn a_call_directive_works_through_a_stream_destination() {
    is_true(&format!(
        r#"{} (let ((s (make-string-output-stream)))
                (progn (format s "~/brief/" (point::new 5 6))
                       (equal (get-output-stream-string s) "5/6")))"#,
        POINT
    ));
}

// ---- print-object on a generic type ----------------------------------------

/// The printer looks a method up by the key the value carries, and that key
/// includes the instantiation now (`gen<i32>`), so the body monomorphization
/// registered for it (`print-object <i32>`) is findable.
///
/// This test used to assert the opposite, pinning a known gap: the key was
/// `gen`, the registration was `print-object <i32>`, and the impl silently
/// never applied — while still type-checking and still working when called by
/// name, which is what made it a trap rather than a missing feature.
#[test]
fn print_object_reaches_a_generic_type() {
    is_true(
        r#"(defstruct gen<T> (v T))
           (impl print-object gen<T> (print-object ((self Self) (escape bool)) string "GEN"))
           (equal (format false "~a" (gen::new 1)) "GEN")"#,
    );
    // Still reachable by name, as it always was.
    is_true(
        r#"(defstruct gen<T> (v T))
           (impl print-object gen<T> (print-object ((self Self) (escape bool)) string "GEN"))
           (equal (print-object (gen::new 1) true) "GEN")"#,
    );
}

/// Each instantiation gets *its own* body — what a key with no type arguments
/// could not express even in principle, since one key would have to choose
/// between two compiled bodies.
#[test]
fn each_instantiation_reaches_its_own_print_object() {
    is_true(
        r#"(defstruct wi (n i32))
           (impl print-object wi (print-object ((self Self) (escape bool)) string "INT"))
           (defstruct ws (s string))
           (impl print-object ws (print-object ((self Self) (escape bool)) string "STR"))
           (defstruct gen<T> (v T))
           (impl print-object gen<T>
             (where (print-object T))
             (print-object ((self Self) (escape bool)) string
               (append "gen-of-" (print-object (v self) escape))))
           (and (equal (format false "~a" (gen::new (wi::new 1))) "gen-of-INT")
                (equal (format false "~a" (gen::new (ws::new "x"))) "gen-of-STR"))"#,
    );
}

/// A built-in generic container reaches its own `print-object` too. `Vector`
/// builds its box in Rust with no `construct` node, so the specialization the
/// printer needs is requested from the `assoc` node instead — without that the
/// impl type-checks, is callable by name, and is silently never selected.
#[test]
fn print_object_reaches_a_builtin_generic_type() {
    is_true(
        r#"(defstruct point (x i32) (y i32))
           (impl print-object point (print-object ((self Self) (escape bool)) string "P"))
           (impl print-object Vector<T> (where (print-object T))
             (print-object ((self Self) (escape bool)) string "V"))
           (equal (format false "~a" (the Vector<point> (Vector::new))) "V")"#,
    );
}

/// An enum's variant *name* still resolves once the value's key carries an
/// instantiation.
///
/// Variant names are registered per enum *type* (`option`), while the key a
/// value carries names an instantiation (`option<i32>`). Looking one up
/// without splitting the base off printed every `Option` as
/// `(<unknown-variant> 1)` — 28 tests across nine suites, and none of them was
/// about printing an `Option`, which is why the whole class only showed up in
/// the serial full run.
#[test]
fn an_enum_variant_name_survives_an_instantiated_key() {
    is_true(r#"(equal (format false "~a" (option::some 1)) "(some 1)")"#);
    is_true(r#"(equal (format false "~a" (the Option<i32> (option::none))) "none")"#);
    is_true(
        r#"(defenum box<T> (full T) (empty))
           (equal (format false "~a" (the box<string> (box::full "x"))) "(full x)")"#,
    );
}

// ---- `Array<T>` and `*print-array*` ------------------------------------------
//
// The remainder of Phase 8a, deferred until a generic type's `print-object`
// could be reached at all. What makes an array printable in generic code is
// the trait used as a *bound*: `(impl print-object Array<T> (where
// (print-object T)))` needs its elements renderable, so the scalar types
// implement the trait for that purpose alone (the printer never consults it
// for them — a scalar carries no type key).

/// CL's array syntax: `#(…)` at rank 1, `#nA` plus one nesting level per
/// dimension otherwise, and `#0A` on the lone element of a rank-0 array.
#[test]
fn an_array_prints_in_cl_syntax() {
    is_true(
        r##"(let ((d (the Vector<i32> (Vector::new))))
             (progn (push d 2) (push d 3)
               (let ((a (Array::make d 0)))
                 (progn (setf (aref a 0 0) 7)
                        (equal (format false "~a" a) "#2A((7 0 0) (0 0 0))")))))"##,
    );
    is_true(
        r##"(let ((d (the Vector<i32> (Vector::new))))
             (progn (push d 3)
               (equal (format false "~a" (Array::make d 1)) "#(1 1 1)")))"##,
    );
    is_true(
        r##"(let ((d (the Vector<i32> (Vector::new))))
             (equal (format false "~a" (Array::make d 5)) "#0A5"))"##,
    );
}

/// `escape` reaches the elements, so `~s` prints an array of strings the way
/// the reader would want them back and `~a` prints them bare.
#[test]
fn an_arrays_elements_follow_the_escape_choice() {
    is_true(
        r##"(let ((d (the Vector<i32> (Vector::new))))
             (progn (push d 2)
               (let ((a (Array::make d "q")))
                 (and (equal (format false "~s" a) "#(\"q\" \"q\")")
                      (equal (format false "~a" a) "#(q q)")))))"##,
    );
}

/// A fill pointer cuts the printed contents, as CL's printer does: past it
/// the storage is not part of the array's contents.
#[test]
fn a_fill_pointer_cuts_what_prints() {
    is_true(
        r##"(let ((d (the Vector<i32> (Vector::new))))
             (progn (push d 4)
               (equal (format false "~a" (Array::make d 0 :fill-pointer 2)) "#(0 0)")))"##,
    );
}

/// `*print-array*` false prints the shape instead of the contents — CL's "in
/// a way that does not reveal the contents", spelled for the one array type
/// this language has.
#[test]
fn print_array_false_shows_only_the_shape() {
    is_true(
        r##"(let ((d (the Vector<i32> (Vector::new))))
             (progn (push d 2) (push d 3)
               (let ((a (Array::make d 0)))
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
        r##"(defstruct plain (n i32))
           (let ((d (the Vector<i32> (Vector::new))))
             (progn (push d 1)
               (let ((a (Array::make d (plain::new 1))))
                 (equal (format false "~a" a)
                        "#<array<plain> #<vector<i32> 1> #<vector<plain> #<plain 1>> none>"))))"##,
    );
}

/// The bound is what generic code asks for, and the scalars answer it —
/// including through a container of containers, where the element being
/// rendered is itself an `Array`.
#[test]
fn arrays_nest_through_the_bound() {
    is_true(
        r##"(let ((d (the Vector<i32> (Vector::new))))
             (progn (push d 2)
               (let ((inner (Array::make d 1)))
                 (equal (format false "~a" (Array::make d inner)) "#(#(1 1) #(1 1))"))))"##,
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
/// implements the same trait with a matching argument in the second position.
/// `(print-object x true)` under `(where (print-object T))` used to be re-read
/// with its arguments swapped — `try_instance_method`'s peek stops before the
/// bounds, and the swapped order then resolved against `bool`'s impl, so the
/// error named a call nobody wrote ("expected Bool, found t").
#[test]
fn a_bound_method_is_not_re_read_with_swapped_arguments() {
    is_true(
        r##"(defun render<T> ((x T)) string (where (print-object T)) (print-object x true))
           (equal (render 7) "7")"##,
    );
}
