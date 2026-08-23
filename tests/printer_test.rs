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

fn eval_err(src: &str) -> String {
    match run(src) {
        Err(e) => format!("{:?}", e),
        Ok(v) => panic!("expected a runtime error, got {:?}", v),
    }
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
(defstruct point (x i64) (y i64))
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
        r#"(defmethod twice ((self i64) (colon bool) (at bool)) string (format false "~a ~a" self self))
           (equal (format false "~/twice/" 7) "7 7")"#,
    );
}

#[test]
fn an_integer_argument_is_refused_when_both_widths_define_the_name() {
    // Nothing in the value says which width was written, and guessing would
    // silently run the wrong body.
    let e = eval_err(
        r#"(defmethod both ((self i64) (colon bool) (at bool)) string "64")
           (defmethod both ((self i32) (colon bool) (at bool)) string "32")
           (format false "~/both/" 7)"#,
    );
    assert!(e.contains("either width"), "{}", e);
}

#[test]
fn a_missing_method_is_an_error_not_a_fallback() {
    // Unlike `print-object`, which has the built-in rendering to fall back
    // to: the control string asked for something by name.
    let e = eval_err(r#"(format false "~/nope/" "hi")"#);
    assert!(e.contains("has no method"), "{}", e);
}

#[test]
fn a_method_of_the_wrong_shape_is_reported_as_such() {
    let e = eval_err(
        r#"(defmethod short ((self string)) string self)
           (format false "~/short/" "hi")"#,
    );
    assert!(e.contains("(colon bool) (at bool)"), "{}", e);
}

#[test]
fn an_unterminated_call_directive_is_a_syntax_error() {
    let e = eval_err(r#"(format false "~/brief" "hi")"#);
    assert!(e.contains("closing"), "{}", e);
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

// ---- print-object on a generic type (a known gap, pinned here) ---------------

#[test]
fn print_object_does_not_reach_a_generic_type() {
    // Not the behaviour anyone wants — it is recorded as a test because the
    // impl below *type-checks* and can be called by name, so nothing else
    // would notice it silently not applying. The printer looks a method up by
    // the type key the value carries, and monomorphization has erased the
    // type argument by then: the key is `gen`, not `gen<i64>`. See
    // docs/dev/TODO.md; when that is fixed this test should start failing.
    let src = r#"
      (defstruct gen<T> (v T))
      (impl print-object gen<T> (print-object ((self Self) (escape bool)) string "GEN"))
      (format false "~a" (gen::new 1))"#;
    let printed = eval_ok(src);
    let shown = format!("{:?}", printed);
    assert!(!shown.contains("GEN"), "the generic gap closed — update this test: {}", shown);

    // The same method *is* reachable by name, which is what makes the gap a
    // trap rather than a missing feature.
    is_true(
        r#"(defstruct gen<T> (v T))
           (impl print-object gen<T> (print-object ((self Self) (escape bool)) string "GEN"))
           (equal (print-object (gen::new 1) true) "GEN")"#,
    );
}
