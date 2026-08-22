//! Tests for the general-purpose error type, error wrapping, `assert` and
//! `warn` — cl-parity-plan.md Phase 7a.
//!
//! The condition *system* is not adopted (there is no way to catch a `panic`
//! and continue — language-design.md §9). What is tested here is the mapping
//! of CL's condition *types* onto the `Error` trait, and the two things that
//! mapping left missing: a type a program can reach for when it just wants to
//! say what went wrong, and a way to report something without either
//! returning a `Result` or ending the program.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

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

fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut result = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    match result.map_err(Error::into_kind) {
        Err(Error::TypeError(msg)) => msg,
        other => panic!("expected a TypeError, got {:?}", other),
    }
}

fn is_true(src: &str) {
    assert_eq!(eval_ok(src), Value::Bool(true), "{}", src);
}

// ---- SimpleError ------------------------------------------------------------

#[test]
fn a_simple_error_carries_its_message() {
    is_true(r#"(equal (message (SimpleError::new "boom")) "boom")"#);
    is_true(r#"(equal (message (simple-error "boom")) "boom")"#);
}

#[test]
fn a_simple_error_is_the_root_of_its_chain() {
    is_true(r#"(is-none (source (SimpleError::new "boom")))"#);
}

#[test]
fn a_simple_error_travels_as_a_dyn_error() {
    let src = r#"(defun f () Result<i32, :dyn Error>
                   (as-dyn-error (the Result<i32,SimpleError> (result::err (simple-error "no")))))
                 (equal (match (f) ((ok _) "?") ((err e) (message e))) "no")"#;
    is_true(src);
}

// ---- wrapping ---------------------------------------------------------------

#[test]
fn a_wrapped_error_reports_its_own_message_and_keeps_the_cause() {
    is_true(r#"(equal (message (wrap-error "reading" (simple-error "no such file"))) "reading")"#);
    is_true(
        r#"(match (source (wrap-error "reading" (simple-error "no such file")))
             ((some c) (equal (message c) "no such file"))
             ((none) false))"#,
    );
}

#[test]
fn wrapping_works_over_any_concrete_error_type() {
    // The cause is widened where the call site specializes the body, the same
    // way `as-dyn-error` does it.
    is_true(
        r#"(match (source (wrap-error "parsing" (ParseIntError::ParseIntError "bad digit")))
             ((some c) (equal (message c) "bad digit"))
             ((none) false))"#,
    );
}

#[test]
fn a_cause_that_is_not_an_error_is_a_type_error() {
    let msg = check_err(r#"(wrap-error "x" 42)"#);
    assert!(msg.contains("does not implement"), "{}", msg);
}

// ---- the chain --------------------------------------------------------------

#[test]
fn describe_error_is_just_the_message_for_a_leaf() {
    is_true(r#"(equal (describe-error (simple-error "boom")) "boom")"#);
}

#[test]
fn describe_error_walks_every_cause() {
    let src = r#"(describe-error
                   (wrap-error "starting up"
                     (wrap-error "reading the config" (simple-error "no such file"))))"#;
    is_true(&format!(
        "(equal {} \"starting up\n  caused by: reading the config\n  caused by: no such file\")",
        src.replace('\n', " ")
    ));
}

#[test]
fn describe_error_accepts_a_dyn_error_too() {
    // The parameter is generic over `Error`, and a `:dyn Error` implements it.
    is_true(r#"(equal (describe-error (as :dyn Error (simple-error "boom"))) "boom")"#);
}

// ---- assert -----------------------------------------------------------------

#[test]
fn assert_passes_a_true_test_silently() {
    assert_eq!(eval_ok("(assert (= 1 1))"), Value::Empty);
}

#[test]
fn assert_names_the_test_as_written_when_it_fails() {
    let msg = eval_err("(assert (= 1 2))");
    assert!(msg.contains("assertion failed"), "{}", msg);
    assert!(msg.contains("(= 1 2)"), "{}", msg);
}

#[test]
fn assert_uses_a_given_message_instead() {
    let msg = eval_err(r#"(assert (= 1 2) "one is not two")"#);
    assert!(msg.contains("one is not two"), "{}", msg);
    assert!(!msg.contains("assertion failed"), "{}", msg);
}

#[test]
fn assert_evaluates_its_test_exactly_once() {
    let src = "(let ((n 0))
                 (labels ((bump () bool (progn (setf n (+ n 1)) true)))
                   (progn (assert (bump)) n)))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

// ---- warn -------------------------------------------------------------------

// `warn`'s text goes to the process's stderr and is not observed here.
// `*error-output*` is a `standard-stream`, and a string stream is a different
// type, so there is nothing in-process to rebind it to — making the standard
// streams rebindable is Phase 7b's business, and the test that reads the text
// back belongs there. What these pin down is the half that *is* observable:
// `warn` type-checks against a control string with arguments, evaluates them,
// and — the whole point of it — carries on.

#[test]
fn warn_evaluates_its_arguments() {
    let src = "(let ((n 0))
                 (labels ((bump () i32 (progn (setf n (+ n 1)) n)))
                   (progn (warn \"~a\" (bump)) n)))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn warn_returns_unit_so_it_can_sit_in_a_progn() {
    assert_eq!(eval_ok(r#"(progn (warn "~a" "hi") 7)"#), Value::Int(7));
}

#[test]
fn warn_takes_a_control_string_with_arguments() {
    assert_eq!(
        eval_ok(r#"(progn (warn "~a and ~a" 1 2) 7)"#),
        Value::Int(7)
    );
}
