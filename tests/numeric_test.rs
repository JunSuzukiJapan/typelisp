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
use std::cell::RefCell;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue};

// `abs`/`signum`/`gcd`/`lcm`/`rem` (and `f64` `mod`/`rem`) are `prelude.rs`
// methods, not registry builtins, so the prelude must be loaded. Shared
// per-thread as `prelude_test` does.
thread_local! {
    static CTX: RefCell<Option<(Heap, Checker, Interp)>> = const { RefCell::new(None) };
}

fn run(src: &str) -> Result<RtValue, EvalError> {
    CTX.with(|cell| {
        let mut opt = cell.borrow_mut();
        let (h, chk, interp) = opt.get_or_insert_with(|| {
            let mut h = Heap::with_capacity(1 << 16);
            let mut chk = Checker::new();
            let mut interp = Interp::new();
            load_prelude(&mut h, &mut chk, &mut interp);
            (h, chk, interp)
        });
        let r = Reader::new();
        let vs = r.read_all(h, src).expect("read failed");
        let mut last = RtValue::Unit;
        for v in vs {
            let tl = chk.check_form(h, &*interp, v).expect("check failed");
            if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
                last = val;
            }
        }
        Ok(last)
    })
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
fn i64_mod_is_floored_and_rem_is_truncated() {
    let m = "(defun f ((a i64) (b i64)) i64 (mod a b)) (f -7 3)";
    let r = "(defun f ((a i64) (b i64)) i64 (rem a b)) (f -7 3)";
    assert_eq!(eval_ok(m), RtValue::Int(2));
    assert_eq!(eval_ok(r), RtValue::Int(-1));
}

#[test]
fn i64_abs_and_signum() {
    let a = "(defun f ((x i64)) i64 (abs x)) (f -9000000000)";
    let s = "(defun f ((x i64)) i64 (signum x)) (f -9000000000)";
    assert_eq!(eval_ok(a), RtValue::Int(9000000000));
    assert_eq!(eval_ok(s), RtValue::Int(-1));
}

#[test]
fn i64_gcd_and_lcm() {
    let g = "(defun f ((a i64) (b i64)) i64 (gcd a b)) (f 12 18)";
    let l = "(defun f ((a i64) (b i64)) i64 (lcm a b)) (f 4 6)";
    let l0 = "(defun f ((a i64) (b i64)) i64 (lcm a b)) (f 0 5)";
    assert_eq!(eval_ok(g), RtValue::Int(6));
    assert_eq!(eval_ok(l), RtValue::Int(12));
    assert_eq!(eval_ok(l0), RtValue::Int(0));
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

// ---- 2-argument floor/ceiling/round/truncate (`*-div`, quotient+remainder as
// a `cons-cell`, standing in for CL's multiple values — see `prelude.rs`'s
// comment just above `floor-div`) -----------------------------------------------

#[test]
fn i64_floor_div() {
    // CL: (floor 7 2) => 3, 1 ; (floor -7 2) => -4, 1
    assert_eq!(eval_ok("(car (floor-div 7 2))"), RtValue::Int(3));
    assert_eq!(eval_ok("(cdr (floor-div 7 2))"), RtValue::Int(1));
    assert_eq!(eval_ok("(car (floor-div -7 2))"), RtValue::Int(-4));
    assert_eq!(eval_ok("(cdr (floor-div -7 2))"), RtValue::Int(1));
}

#[test]
fn i64_ceiling_div() {
    // CL: (ceiling 7 2) => 4, -1 ; (ceiling -7 2) => -3, -1 ; exact division
    // carries no remainder.
    assert_eq!(eval_ok("(car (ceiling-div 7 2))"), RtValue::Int(4));
    assert_eq!(eval_ok("(cdr (ceiling-div 7 2))"), RtValue::Int(-1));
    assert_eq!(eval_ok("(car (ceiling-div -7 2))"), RtValue::Int(-3));
    assert_eq!(eval_ok("(cdr (ceiling-div -7 2))"), RtValue::Int(-1));
    assert_eq!(eval_ok("(car (ceiling-div 6 2))"), RtValue::Int(3));
    assert_eq!(eval_ok("(cdr (ceiling-div 6 2))"), RtValue::Int(0));
}

#[test]
fn i64_truncate_div() {
    // CL: (truncate -7 2) => -3, -1 (remainder's sign follows the dividend,
    // unlike `floor-div`'s).
    assert_eq!(eval_ok("(car (truncate-div -7 2))"), RtValue::Int(-3));
    assert_eq!(eval_ok("(cdr (truncate-div -7 2))"), RtValue::Int(-1));
}

#[test]
fn i64_round_div_ties_to_even() {
    // CL round-half-to-even: (round 7 2) => 4, -1 (3.5 -> 4, even);
    // (round 5 2) => 2, 1 (2.5 -> 2, even); (round 3 2) => 2, -1 (1.5 -> 2,
    // even); (round -5 2) => -2, -1 (-2.5 -> -2, even).
    assert_eq!(eval_ok("(car (round-div 7 2))"), RtValue::Int(4));
    assert_eq!(eval_ok("(cdr (round-div 7 2))"), RtValue::Int(-1));
    assert_eq!(eval_ok("(car (round-div 5 2))"), RtValue::Int(2));
    assert_eq!(eval_ok("(cdr (round-div 5 2))"), RtValue::Int(1));
    assert_eq!(eval_ok("(car (round-div 3 2))"), RtValue::Int(2));
    assert_eq!(eval_ok("(cdr (round-div 3 2))"), RtValue::Int(-1));
    assert_eq!(eval_ok("(car (round-div -5 2))"), RtValue::Int(-2));
    assert_eq!(eval_ok("(cdr (round-div -5 2))"), RtValue::Int(-1));
}

#[test]
fn f64_floor_div() {
    assert_close(eval_ok("(car (floor-div 5.5 2.0))"), 2.0);
    assert_close(eval_ok("(cdr (floor-div 5.5 2.0))"), 1.5);
}

#[test]
fn f64_ceiling_div() {
    assert_close(eval_ok("(car (ceiling-div 5.5 2.0))"), 3.0);
    assert_close(eval_ok("(cdr (ceiling-div 5.5 2.0))"), -0.5);
}

#[test]
fn f64_truncate_div() {
    assert_close(eval_ok("(car (truncate-div (- 0.0 5.5) 2.0))"), -2.0);
    assert_close(eval_ok("(cdr (truncate-div (- 0.0 5.5) 2.0))"), -1.5);
}

#[test]
fn f64_round_div_ties_to_even() {
    assert_close(eval_ok("(car (round-div 5.0 2.0))"), 2.0);
    assert_close(eval_ok("(cdr (round-div 5.0 2.0))"), 1.0);
}

#[test]
fn f64_abs() {
    let src = "(defun f ((a f64)) f64 (abs a)) (f (- 0.0 2.5))";
    assert_close(eval_ok(src), 2.5);
}

#[test]
fn f64_signum() {
    assert_close(eval_ok("(defun f ((a f64)) f64 (signum a)) (f (- 0.0 2.5))"), -1.0);
    assert_close(eval_ok("(defun f ((a f64)) f64 (signum a)) (f 2.5)"), 1.0);
    // CL returns the zero itself for 0.0 (Rust's `f64::signum` would give 1.0).
    assert_close(eval_ok("(defun f ((a f64)) f64 (signum a)) (f 0.0)"), 0.0);
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
