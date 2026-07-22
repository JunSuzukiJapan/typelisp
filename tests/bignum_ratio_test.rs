//! Tests for `bignum` (arbitrary-precision integer) and `ratio` (exact
//! rational), added following Common Lisp's bignum/ratio semantics: reader
//! literals (an integer past `i64`'s range, and CL's `n/d` ratio syntax,
//! both normalizing the same way CL's reader does), arithmetic/comparison
//! instance methods (`registry::bignum_assoc`/`ratio_assoc`), and mutual
//! conversions with `i32`/`i64`/`f64`.

extern crate typelisp;
use std::cell::RefCell;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, RtValue};

// `abs`/`signum`/`gcd`/`lcm`/`rem`/`expt` (and `f64`/`ratio` `mod`/`rem`) are
// `prelude.rs` methods, not registry builtins, so the prelude must be loaded.
// Shared per-thread (prelude load reused) exactly as `prelude_test` does.
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

fn bignum(s: &str) -> num_bigint::BigInt {
    s.parse().unwrap()
}

fn assert_bignum(actual: RtValue, expected: &str) {
    match actual {
        RtValue::Bignum(n) => assert_eq!(*n, bignum(expected)),
        other => panic!("expected a Bignum, got {:?}", other),
    }
}

fn assert_ratio(actual: RtValue, numer: &str, denom: &str) {
    match actual {
        RtValue::Ratio(r) => {
            assert_eq!(*r.numer(), bignum(numer));
            assert_eq!(*r.denom(), bignum(denom));
        }
        other => panic!("expected a Ratio, got {:?}", other),
    }
}

// ---- reader literals --------------------------------------------------------

#[test]
fn an_integer_literal_past_i64_range_reads_as_a_bignum() {
    let src = "(defun f () bignum 99999999999999999999999999999) (f)";
    assert_bignum(eval_ok(src), "99999999999999999999999999999");
}

#[test]
fn a_negative_bignum_literal_reads_correctly() {
    let src = "(defun f () bignum -99999999999999999999999999999) (f)";
    assert_bignum(eval_ok(src), "-99999999999999999999999999999");
}

#[test]
fn a_ratio_literal_reads_in_reduced_form() {
    let src = "(defun f () ratio 4/6) (f)";
    assert_ratio(eval_ok(src), "2", "3");
}

#[test]
fn a_ratio_literal_that_reduces_to_an_integer_reads_as_an_int_not_a_ratio() {
    // `4/2` reduces to the integer `2` at read time (CL's own ratio-literal
    // normalization) — it must type-check as a plain `i32`, not `ratio`.
    let src = "(defun f () i32 4/2) (f)";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn a_negative_ratio_literal_normalizes_the_sign_onto_the_numerator() {
    let src = "(defun f () ratio -3/4) (f)";
    assert_ratio(eval_ok(src), "-3", "4");
}

// ---- bignum arithmetic/comparison -------------------------------------------

#[test]
fn bignum_addition() {
    // A plain `i32` literal never implicitly widens to `bignum` (no implicit
    // numeric coercions anywhere in this language — same as `i32`/`i64`);
    // `int->bignum` makes the small operand's type explicit.
    let src = "(defun f ((a bignum) (b bignum)) bignum (+ a b)) \
               (f 99999999999999999999 (int->bignum 1))";
    assert_bignum(eval_ok(src), "100000000000000000000");
}

#[test]
fn bignum_subtraction_and_multiplication() {
    let src = "(defun f ((a bignum) (b bignum)) bignum (* (- a b) b)) \
               (f 100000000000000000000 (int->bignum 1))";
    assert_bignum(eval_ok(src), "99999999999999999999");
}

#[test]
fn bignum_truncating_division_and_mod_on_positive_operands() {
    // `/` truncates toward zero; `mod`/`rem` agree for positive operands (the
    // sign-sensitive floored-vs-truncated distinction is exercised separately
    // in `bignum_mod_is_floored_and_rem_is_truncated`).
    let d = "(defun d ((a bignum) (b bignum)) bignum (/ a b)) (d 100000000000000000007 100000000000000000000)";
    let m = "(defun m ((a bignum) (b bignum)) bignum (mod a b)) (m 100000000000000000007 100000000000000000000)";
    assert_bignum(eval_ok(d), "1");
    assert_bignum(eval_ok(m), "7");
}

#[test]
fn bignum_divide_by_zero_panics() {
    let src = "(defun f ((a bignum) (b bignum)) bignum (/ a b)) (f 100000000000000000000 (int->bignum 0))";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn bignum_comparison() {
    let src = "(defun f ((a bignum) (b bignum)) bool (< a b)) \
               (f 99999999999999999999 100000000000000000000)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn bignum_mod_is_floored_and_rem_is_truncated() {
    // CL: `mod` takes the sign of the divisor, `rem` the sign of the dividend.
    // `(mod -7 3) = 2`, `(rem -7 3) = -1`; `(mod 7 -3) = -2`, `(rem 7 -3) = 1`.
    let m = |a: i32, b: i32| format!("(defun f ((a bignum) (b bignum)) bignum (mod a b)) (f (int->bignum {a}) (int->bignum {b}))");
    let r = |a: i32, b: i32| format!("(defun f ((a bignum) (b bignum)) bignum (rem a b)) (f (int->bignum {a}) (int->bignum {b}))");
    assert_bignum(eval_ok(&m(-7, 3)), "2");
    assert_bignum(eval_ok(&r(-7, 3)), "-1");
    assert_bignum(eval_ok(&m(7, -3)), "-2");
    assert_bignum(eval_ok(&r(7, -3)), "1");
}

#[test]
fn bignum_abs_and_signum() {
    let a = "(defun f ((x bignum)) bignum (abs x)) (f -99999999999999999999)";
    let s = "(defun f ((x bignum)) bignum (signum x)) (f -99999999999999999999)";
    assert_bignum(eval_ok(a), "99999999999999999999");
    assert_bignum(eval_ok(s), "-1");
}

#[test]
fn bignum_gcd_and_lcm() {
    let g = "(defun f ((a bignum) (b bignum)) bignum (gcd a b)) (f (int->bignum 12) (int->bignum 18))";
    let l = "(defun f ((a bignum) (b bignum)) bignum (lcm a b)) (f (int->bignum 4) (int->bignum 6))";
    assert_bignum(eval_ok(g), "6");
    assert_bignum(eval_ok(l), "12");
}

#[test]
fn bignum_lcm_with_a_zero_operand_is_zero() {
    // `num_integer::lcm` would divide by `gcd(0,5) = 5`... but `lcm(0,0)`'s
    // `gcd` is 0, so the zero guard (matching CL and the `i32` prelude `lcm`)
    // returns 0 whenever either operand is 0.
    let l = "(defun f ((a bignum) (b bignum)) bignum (lcm a b)) (f (int->bignum 0) (int->bignum 0))";
    assert_bignum(eval_ok(l), "0");
}

#[test]
fn bignum_expt_produces_a_large_exact_result() {
    let e = "(defun f ((a bignum) (b bignum)) bignum (expt a b)) (f (int->bignum 2) (int->bignum 100))";
    assert_bignum(eval_ok(e), "1267650600228229401496703205376");
}

#[test]
fn bignum_expt_with_a_negative_exponent_panics() {
    let e = "(defun f ((a bignum) (b bignum)) bignum (expt a b)) (f (int->bignum 2) (int->bignum -1))";
    assert!(matches!(run(e), Err(EvalError::Panic(_))));
}

// ---- ratio arithmetic/comparison --------------------------------------------

#[test]
fn ratio_addition_reduces_the_result() {
    let src = "(defun f ((a ratio) (b ratio)) ratio (+ a b)) (f 1/3 1/6)";
    assert_ratio(eval_ok(src), "1", "2");
}

#[test]
fn ratio_multiplication() {
    let src = "(defun f ((a ratio) (b ratio)) ratio (* a b)) (f 2/3 3/4)";
    assert_ratio(eval_ok(src), "1", "2");
}

#[test]
fn ratio_division_by_zero_panics() {
    // `0/1` itself reduces to the plain integer `0` at read time (ratio
    // literals normalize like every other CL ratio literal), so the zero
    // divisor is built via `int->ratio` to keep it genuinely `ratio`-typed.
    let src = "(defun f ((a ratio) (b ratio)) ratio (/ a b)) (f 1/2 (int->ratio 0))";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn ratio_comparison() {
    let src = "(defun f ((a ratio) (b ratio)) bool (< a b)) (f 1/3 1/2)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn ratio_numerator_and_denominator() {
    let src = "(defun f ((r ratio)) bignum (numerator r)) (f 4/6)";
    assert_bignum(eval_ok(src), "2");
    let src = "(defun f ((r ratio)) bignum (denominator r)) (f 4/6)";
    assert_bignum(eval_ok(src), "3");
}

#[test]
fn ratio_abs_and_signum() {
    assert_ratio(eval_ok("(defun f ((r ratio)) ratio (abs r)) (f -3/4)"), "3", "4");
    assert_ratio(eval_ok("(defun f ((r ratio)) ratio (signum r)) (f -3/4)"), "-1", "1");
    assert_ratio(eval_ok("(defun f ((r ratio)) ratio (signum r)) (f 3/4)"), "1", "1");
}

#[test]
fn ratio_expt_with_integer_exponent() {
    // (expt 2/3 3) = 8/27; a negative exponent takes the reciprocal.
    assert_ratio(eval_ok("(defun f ((a ratio) (b ratio)) ratio (expt a b)) (f 2/3 (int->ratio 3))"), "8", "27");
    assert_ratio(eval_ok("(defun f ((a ratio) (b ratio)) ratio (expt a b)) (f 2/3 (int->ratio -2))"), "9", "4");
}

#[test]
fn ratio_expt_with_a_non_integer_exponent_panics() {
    let src = "(defun f ((a ratio) (b ratio)) ratio (expt a b)) (f 2/3 1/2)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

// ---- conversions --------------------------------------------------------------

#[test]
fn int_to_bignum_and_back() {
    let src = "(defun f ((a i32)) bignum (int->bignum a)) (f 42)";
    assert_bignum(eval_ok(src), "42");
    let src = "(defun f ((a bignum)) i32 (bignum->int a)) (f (int->bignum 42))";
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

#[test]
fn bignum_to_int_out_of_range_panics() {
    let src = "(defun f ((a bignum)) i32 (bignum->int a)) \
               (f 999999999999999999999999999999)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn bignum_to_float_and_back() {
    let src = "(defun f ((a bignum)) f64 (bignum->float a)) (f (int->bignum 2))";
    assert_eq!(eval_ok(src), RtValue::Float(2.0));
    let src = "(defun f ((a f64)) bignum (float->bignum a)) (f 2.0)";
    assert_bignum(eval_ok(src), "2");
}

#[test]
fn bignum_to_ratio_is_exact() {
    let src = "(defun f ((a bignum)) ratio (bignum->ratio a)) (f (int->bignum 5))";
    assert_ratio(eval_ok(src), "5", "1");
}

#[test]
fn int_to_ratio_is_exact() {
    let src = "(defun f ((a i32)) ratio (int->ratio a)) (f 7)";
    assert_ratio(eval_ok(src), "7", "1");
}

#[test]
fn ratio_to_bignum_truncates_toward_zero() {
    let src = "(defun f ((r ratio)) bignum (ratio->bignum r)) (f 7/2)";
    assert_bignum(eval_ok(src), "3");
    let src = "(defun f ((r ratio)) bignum (ratio->bignum r)) (f -7/2)";
    assert_bignum(eval_ok(src), "-3");
}

#[test]
fn ratio_to_float() {
    let src = "(defun f ((r ratio)) f64 (ratio->float r)) (f 1/2)";
    assert_eq!(eval_ok(src), RtValue::Float(0.5));
}

#[test]
fn float_to_ratio_is_exact() {
    let src = "(defun f ((a f64)) ratio (float->ratio a)) (f 0.5)";
    assert_ratio(eval_ok(src), "1", "2");
}

// ---- eq/eql/equal/equalp (typed instance methods, value comparison) --------

#[test]
fn bignum_equal_family_is_value_comparison() {
    let src = "(defun f ((a bignum) (b bignum)) bool (equal a b)) \
               (f 100000000000000000000 100000000000000000000)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn ratio_equal_family_is_value_comparison() {
    let src = "(defun f ((a ratio) (b ratio)) bool (equal a b)) (f 1/2 2/4)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

// ---- quoted Sexpr datum round-trip -------------------------------------------

#[test]
fn a_quoted_bignum_survives_a_round_trip_through_equal() {
    let src = "(defun f () bool (equal (quote 99999999999999999999) (quote 99999999999999999999))) (f)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}
