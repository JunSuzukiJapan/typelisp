//! Tests for error-handling primitives: `Result<T,E>`, the built-in `Error`
//! type, the `Never` (`!`) type, and the `panic!` special form.

extern crate typelisp;
use typelisp::{Checker, Error, Heap, Reader, TopLevel, Type};

fn program(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&h, v)?);
    }
    Ok(last.expect("no forms"))
}

fn ty(src: &str) -> Type {
    match program(src).expect("check failed") {
        TopLevel::Expr(t) => t.ty,
        other => panic!("expected expression, got {:?}", other),
    }
}

fn assert_type_error(src: &str) {
    match program(src) {
        Err(Error::TypeError(_)) => {}
        other => panic!("expected TypeError, got {:?}", other),
    }
}

// ---- Never / panic! ---------------------------------------------------------

#[test]
fn panic_has_never_type() {
    assert_eq!(ty("(panic! \"boom\")"), Type::Never);
}

#[test]
fn panic_message_must_be_string() {
    assert_type_error("(panic! 42)");
}

#[test]
fn if_branch_may_panic() {
    // The else branch diverges; the if still has the then branch's type.
    assert_eq!(ty("(if true 1 (panic! \"x\"))"), Type::I32);
    assert_eq!(ty("(if true (panic! \"x\") 2)"), Type::I32);
}

#[test]
fn defun_branch_may_panic() {
    let src = "(defun f ((b bool) (x i32)) i32 (if b (panic! \"neg\") x))";
    assert!(matches!(program(src), Ok(TopLevel::Defun { .. })));
}

#[test]
fn parses_never_type_annotation() {
    // A function whose body always diverges has return type `!`.
    let src = "(defun boom () ! (panic! \"always\"))";
    match program(src).unwrap() {
        TopLevel::Defun { ret, .. } => assert_eq!(ret, Type::Never),
        other => panic!("expected defun, got {:?}", other),
    }
}

// ---- Result / Error ---------------------------------------------------------

#[test]
fn result_ok_infers_from_return_type() {
    let src = "(defun mk () Result<i32,Error> (Ok 1))";
    match program(src).unwrap() {
        TopLevel::Defun { ret, .. } => {
            assert_eq!(ret, Type::Named("result".into(), vec![Type::I32, error_ty()]));
        }
        other => panic!("expected defun, got {:?}", other),
    }
}

#[test]
fn result_err_takes_error_value() {
    let src = "(defun bad () Result<i32,Error> (Err (Error \"boom\")))";
    assert!(matches!(program(src), Ok(TopLevel::Defun { .. })));
}

#[test]
fn match_result_exhaustive_with_panic_arm() {
    // Unwrapping a Result: the Err arm diverges, so the match has type i32.
    let src = "(defun unwrap-i ((r Result<i32,Error>)) i32 \
                 (match r ((Ok v) v) ((Err e) (panic! \"unwrap on Err\"))))";
    match program(src).unwrap() {
        TopLevel::Defun { ret, .. } => assert_eq!(ret, Type::I32),
        other => panic!("expected defun, got {:?}", other),
    }
}

#[test]
fn match_result_non_exhaustive_rejected() {
    let src = "(defun f ((r Result<i32,Error>)) i32 (match r ((Ok v) v)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

#[test]
fn match_result_arms_must_agree() {
    let src = "(defun f ((r Result<i32,Error>)) i32 \
                 (match r ((Ok v) v) ((Err e) true)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

fn error_ty() -> Type {
    Type::Named("error".into(), vec![])
}
