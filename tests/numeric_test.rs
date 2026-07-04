//! Tests for the numeric extension
//! ([cl-equivalence-catalog.md](../docs/dev/cl-equivalence-catalog.md) §2.2 f,
//! roadmap step 6): `i64`/`f64` arithmetic/comparison instance methods
//! (`registry::int_assoc`/`float_assoc`), `f64`'s `expt`/`sqrt`/`floor`/
//! `ceiling`/`round`/`truncate`, and the free function `random`.
//!
//! `i32`'s arithmetic moved from free functions to an instance method too
//! (so the same `+`/`<`/etc. symbol can dispatch per receiver type — see
//! `Checker::check_instance_method`); its own regression coverage already
//! lives in `tests/eval_test.rs` (`arithmetic`, `comparison`,
//! `builtin_as_value`, the `dotimes`/`while` family, etc.) and isn't
//! duplicated here.

extern crate typelisp;
use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

fn type_error(src: &str) {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut result: Result<_, Error> = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    assert!(result.is_err(), "expected a type error");
}

fn assert_close(actual: RtValue, expected: f64) {
    match actual {
        RtValue::Float(f) => assert!((f - expected).abs() < 1e-9, "{} != {}", f, expected),
        other => panic!("expected a Float, got {:?}", other),
    }
}

// ---- i64 ----------------------------------------------------------------------

#[test]
fn i64_arithmetic() {
    let src = "(defun add ((a i64) (b i64)) i64 (+ a b)) (add 3 4)";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn i64_comparison() {
    let src = "(defun lt ((a i64) (b i64)) bool (< a b)) (lt 3 4)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn i64_divide_by_zero_panics() {
    let src = "(defun f ((a i64) (b i64)) i64 (/ a b)) (f 1 0)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn int_literal_adopts_i64_from_the_dispatched_operands_expected_type() {
    // A bare literal's default type is `i32`, but `+`'s `i64` instance
    // method expects `i64` for its second operand too — the literal `1`
    // adopts that expected type rather than forcing dispatch back to `i32`.
    let src = "(defun f ((a i64)) i64 (+ a 1)) (f 1)";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn i32_value_cannot_be_passed_where_i64_is_expected() {
    let src = "(defun mk () i32 5) (defun f ((a i64)) i64 (+ a 1)) (f (mk))";
    type_error(src);
}

// ---- f64 ------------------------------------------------------------------------

#[test]
fn f64_arithmetic() {
    let src = "(defun add ((a f64) (b f64)) f64 (+ a b)) (add 1.5 2.5)";
    assert_eq!(eval_ok(src), RtValue::Float(4.0));
}

#[test]
fn f64_comparison() {
    let src = "(defun lt ((a f64) (b f64)) bool (< a b)) (lt 1.0 2.0)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn f64_division_by_zero_is_infinity_not_a_panic() {
    let src = "(defun f ((a f64) (b f64)) f64 (/ a b)) (f 1.0 0.0)";
    match eval_ok(src) {
        RtValue::Float(f) => assert!(f.is_infinite() && f > 0.0),
        other => panic!("expected a Float, got {:?}", other),
    }
}

#[test]
fn f64_mod() {
    let src = "(defun f ((a f64) (b f64)) f64 (mod a b)) (f 5.5 2.0)";
    assert_close(eval_ok(src), 1.5);
}

// ---- f64 transcendental / rounding ---------------------------------------------

#[test]
fn f64_sqrt() {
    let src = "(defun f ((a f64)) f64 (sqrt a)) (f 9.0)";
    assert_close(eval_ok(src), 3.0);
}

#[test]
fn f64_expt() {
    let src = "(defun f ((a f64) (b f64)) f64 (expt a b)) (f 2.0 10.0)";
    assert_close(eval_ok(src), 1024.0);
}

#[test]
fn f64_floor() {
    let src = "(defun f ((a f64)) f64 (floor a)) (f 1.7)";
    assert_close(eval_ok(src), 1.0);
}

#[test]
fn f64_ceiling() {
    let src = "(defun f ((a f64)) f64 (ceiling a)) (f 1.2)";
    assert_close(eval_ok(src), 2.0);
}

#[test]
fn f64_round() {
    let src = "(defun f ((a f64)) f64 (round a)) (f 1.5)";
    assert_close(eval_ok(src), 2.0);
}

#[test]
fn f64_truncate() {
    let src = "(defun f ((a f64)) f64 (truncate a)) (f (- 0.0 1.7))";
    assert_close(eval_ok(src), -1.0);
}

// ---- operator as a first-class value -------------------------------------------

#[test]
fn f64_operator_as_a_value() {
    // Mirrors `tests/eval_test.rs`'s `builtin_as_value` (which covers `i32`'s
    // `+`) — confirms `Checker::method_value`/`Expr::MethodRef` generalizes
    // to other receiver types, not just the one the regression surfaced on.
    let src = "(defun apply2 ((f (fn (f64 f64) f64)) (a f64) (b f64)) f64 (f a b)) \
               (apply2 + 1.5 2.5)";
    assert_eq!(eval_ok(src), RtValue::Float(4.0));
}

// ---- random ---------------------------------------------------------------------

#[test]
fn random_is_within_bounds() {
    let src = "(defun f () i32 (random 10)) (f)";
    for _ in 0..50 {
        match eval_ok(src) {
            RtValue::Int(n) => assert!((0..10).contains(&n), "{} out of range", n),
            other => panic!("expected an Int, got {:?}", other),
        }
    }
}

#[test]
fn random_varies_across_calls() {
    // Not a statistical test — just confirms successive calls aren't frozen
    // at the same value (the seed/state advances).
    let src = "(defun f () i32 (random 1000000)) (f)";
    let mut seen = std::collections::HashSet::new();
    for _ in 0..20 {
        if let RtValue::Int(n) = eval_ok(src) {
            seen.insert(n);
        }
    }
    assert!(seen.len() > 1, "random never changed across 20 calls");
}

#[test]
fn random_with_zero_bound_panics() {
    let src = "(defun f () i32 (random 0)) (f)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn random_with_negative_bound_panics() {
    let src = "(defun f () i32 (random -5)) (f)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}
