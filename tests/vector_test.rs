//! Tests for `Vector<T>` — a builtin growable sequence reusing
//! [`RtValue::Struct`] (no dedicated `RtValue` variant — see
//! [[typelisp-vector-defstruct-revert]] for why a dedicated variant was
//! rejected the first time `Vector<T>` was attempted).

extern crate typelisp;
use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, RtValue, TopLevel};

fn check(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl)? {
            last = v;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

#[test]
fn new_makes_an_empty_vector_with_len_zero() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (len (make-v))";
    assert_eq!(eval_ok(src), RtValue::Int(0));
}

#[test]
fn push_then_get_round_trips() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (push v 2)
                 (push v 3)
                 (get v 1))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn push_increments_len() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 10)
                 (push v 20)
                 (len v))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn set_overwrites_an_existing_element() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (push v 2)
                 (set v 0 99)
                 (get v 0))";
    assert_eq!(eval_ok(src), RtValue::Int(99));
}

#[test]
fn get_out_of_bounds_panics() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v))) (push v 1) (get v 5))";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn set_out_of_bounds_panics() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v))) (set v 0 1))";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn pushing_a_mismatched_element_type_is_a_type_error() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v))) (push v \"oops\"))";
    assert!(check(src).is_err());
}

#[test]
fn vector_holds_strings_too() {
    let src = "(defun make-v () Vector<string> (Vector::new))
               (let ((v (make-v)))
                 (push v \"a\")
                 (push v \"b\")
                 (get v 1))";
    assert_eq!(eval_ok(src), RtValue::Str("b".into()));
}

#[test]
fn two_vectors_with_different_element_types_coexist() {
    let src = "(defun make-i () Vector<i32> (Vector::new))
               (defun make-s () Vector<string> (Vector::new))
               (let ((vi (make-i)) (vs (make-s)))
                 (push vi 1)
                 (push vs \"x\")
                 (+ (get vi 0) (len vs)))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}
