//! Tests for the numeric extension
//! ([cl-equivalence-catalog.md](../docs/dev/cl-equivalence-catalog.md) §2.2 f,
//! roadmap step 6): integer/`f64` arithmetic/comparison instance methods
//! (`registry::int_assoc`/`float_assoc`), `f64`'s `expt`/`sqrt`/`floor`/
//! `ceiling`/`round`/`truncate`, and the free function `random`.
//!
//! `int`'s arithmetic moved from free functions to an instance method too
//! (so the same `+`/`<`/etc. symbol can dispatch per receiver type — see
//! `Checker::check_instance_method`); its own regression coverage already
//! lives in `tests/eval_test.rs` (`arithmetic`, `comparison`,
//! `builtin_as_value`, the `dotimes`/`while` family, etc.) and isn't
//! duplicated here.

extern crate typelisp;
use std::cell::RefCell;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

// `abs`/`signum`/`gcd`/`lcm`/`rem` (and `f64` `mod`/`rem`) are `prelude.rs`
// methods, not registry builtins, so the prelude must be loaded. Shared
// per-thread as `prelude_test` does.
thread_local! {
    static CTX: RefCell<Option<(Heap, Checker, Interp)>> = const { RefCell::new(None) };
}

fn run(src: &str) -> Result<Value, EvalError> {
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
        let mut last = Value::Empty;
        for v in vs {
            let tl = chk.check_form(h, &*interp, v).expect("check failed");
            if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
                last = val;
            }
        }
        Ok(last)
    })
}

fn eval_ok(src: &str) -> Value {
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

/// The `f64` a result carries. An `f64` is a `BoxedObj::Float` since the
/// scalar unification, so reading one needs the heap it lives in — which is
/// the same thread-local `CTX` the evaluation used, so call sites stay
/// unchanged.
fn as_f64(actual: Value) -> f64 {
    CTX.with(|cell| {
        let opt = cell.borrow();
        let (h, _, _) = opt.as_ref().expect("no evaluation has run yet");
        match actual {
            typelisp::Value::Boxed(id) if h.is_f64(id) => h.f64_value(id),
            other => panic!("expected an f64, got {:?}", other),
        }
    })
}

fn assert_close(actual: Value, expected: f64) {
    let f = as_f64(actual);
    assert!((f - expected).abs() < 1e-9, "{} != {}", f, expected);
}

// The block that used to sit here re-ran every `int` test above under `i64`,
// a second width with the same catalog. There is no 64-bit-wide integer type
// any more (`types::Type::is_integer`), and the widths that remain are
// covered — as *widths*, with their own wrapping — by
// `tests/numeric_widths_test.rs`.

#[test]
fn int_literal_adopts_the_dispatched_operands_expected_type() {
    // A bare literal's default type is `int`, but `+`'s `u16` instance method
    // expects `u16` for its second operand too — the literal `1` adopts that
    // expected type rather than forcing dispatch back to `int`.
    let src = "(defun f ((a u16)) u16 (+ a 1)) (f 1)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn a_value_of_one_width_cannot_be_passed_where_another_is_expected() {
    let src = "(defun mk () int 5) (defun f ((a u16)) u16 (+ a 1)) (f (mk))";
    type_error(src);
}

// ---- f64 ------------------------------------------------------------------------

#[test]
fn f64_arithmetic() {
    let src = "(defun add ((a f64) (b f64)) f64 (+ a b)) (add 1.5 2.5)";
    assert_eq!(as_f64(eval_ok(src)), 4.0);
}

#[test]
fn f64_comparison() {
    let src = "(defun lt ((a f64) (b f64)) bool (< a b)) (lt 1.0 2.0)";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn f64_division_by_zero_is_infinity_not_a_panic() {
    let src = "(defun f ((a f64) (b f64)) f64 (/ a b)) (f 1.0 0.0)";
    let f = as_f64(eval_ok(src));
    assert!(f.is_infinite() && f > 0.0);
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
fn i32_floor_div() {
    // CL: (floor 7 2) => 3, 1 ; (floor -7 2) => -4, 1
    assert_eq!(eval_ok("(car (floor-div 7 2))"), Value::Int(3));
    assert_eq!(eval_ok("(cdr (floor-div 7 2))"), Value::Int(1));
    assert_eq!(eval_ok("(car (floor-div -7 2))"), Value::Int(-4));
    assert_eq!(eval_ok("(cdr (floor-div -7 2))"), Value::Int(1));
}

#[test]
fn i32_ceiling_div() {
    // CL: (ceiling 7 2) => 4, -1 ; (ceiling -7 2) => -3, -1 ; exact division
    // carries no remainder.
    assert_eq!(eval_ok("(car (ceiling-div 7 2))"), Value::Int(4));
    assert_eq!(eval_ok("(cdr (ceiling-div 7 2))"), Value::Int(-1));
    assert_eq!(eval_ok("(car (ceiling-div -7 2))"), Value::Int(-3));
    assert_eq!(eval_ok("(cdr (ceiling-div -7 2))"), Value::Int(-1));
    assert_eq!(eval_ok("(car (ceiling-div 6 2))"), Value::Int(3));
    assert_eq!(eval_ok("(cdr (ceiling-div 6 2))"), Value::Int(0));
}

#[test]
fn i32_truncate_div() {
    // CL: (truncate -7 2) => -3, -1 (remainder's sign follows the dividend,
    // unlike `floor-div`'s).
    assert_eq!(eval_ok("(car (truncate-div -7 2))"), Value::Int(-3));
    assert_eq!(eval_ok("(cdr (truncate-div -7 2))"), Value::Int(-1));
}

#[test]
fn i32_round_div_ties_to_even() {
    // CL round-half-to-even: (round 7 2) => 4, -1 (3.5 -> 4, even);
    // (round 5 2) => 2, 1 (2.5 -> 2, even); (round 3 2) => 2, -1 (1.5 -> 2,
    // even); (round -5 2) => -2, -1 (-2.5 -> -2, even).
    assert_eq!(eval_ok("(car (round-div 7 2))"), Value::Int(4));
    assert_eq!(eval_ok("(cdr (round-div 7 2))"), Value::Int(-1));
    assert_eq!(eval_ok("(car (round-div 5 2))"), Value::Int(2));
    assert_eq!(eval_ok("(cdr (round-div 5 2))"), Value::Int(1));
    assert_eq!(eval_ok("(car (round-div 3 2))"), Value::Int(2));
    assert_eq!(eval_ok("(cdr (round-div 3 2))"), Value::Int(-1));
    assert_eq!(eval_ok("(car (round-div -5 2))"), Value::Int(-2));
    assert_eq!(eval_ok("(cdr (round-div -5 2))"), Value::Int(-1));
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
    // Mirrors `tests/eval_test.rs`'s `builtin_as_value` (which covers `int`'s
    // `+`) — confirms `Checker::method_value`/`Expr::MethodRef` generalizes
    // to other receiver types, not just the one the regression surfaced on.
    let src = "(defun apply2 ((f (fn (f64 f64) f64)) (a f64) (b f64)) f64 (f a b)) \
               (apply2 + 1.5 2.5)";
    assert_eq!(as_f64(eval_ok(src)), 4.0);
}

// ---- random ---------------------------------------------------------------------

#[test]
fn random_is_within_bounds() {
    let src = "(defun f () int (random 10)) (f)";
    for _ in 0..50 {
        match eval_ok(src) {
            Value::Int(n) => assert!((0..10).contains(&n), "{} out of range", n),
            other => panic!("expected an Int, got {:?}", other),
        }
    }
}

#[test]
fn random_varies_across_calls() {
    // Not a statistical test — just confirms successive calls aren't frozen
    // at the same value (the seed/state advances).
    let src = "(defun f () int (random 1000000)) (f)";
    let mut seen = std::collections::HashSet::new();
    for _ in 0..20 {
        if let Value::Int(n) = eval_ok(src) {
            seen.insert(n);
        }
    }
    assert!(seen.len() > 1, "random never changed across 20 calls");
}

/// The bound check against the *ordinary* prelude — the compiled one every
/// other test here runs on.
///
/// These two used to need a one-off interpreted prelude, because the check
/// lives in `typelisp_rt::rt_random_state_next` and that function aborted the
/// process rather than reporting anything. It raises now, so the failure comes
/// back as the same `EvalError::Panic` on either tier and the escape hatch is
/// gone; `tests/runtime_error_parity_test.rs` asserts the wording matches.
#[test]
fn random_with_zero_bound_panics() {
    let src = "(defun f () int (random 0)) (f)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn random_with_negative_bound_panics() {
    let src = "(defun f () int (random -5)) (f)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}
