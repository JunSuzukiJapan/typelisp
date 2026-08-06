//! Tests for `as`/`try-as` (`Checker::check_as`) — a Rust-`as`-flavored
//! primitive cast between the numeric/`char` types, unrelated to `Sexpr`
//! (that's `match`'s job, see `tests/match_sexpr_test.rs`). `as` panics on a
//! partial conversion's failure; `try-as` returns `Option<Type>` instead.
//!
//! Both desugar to an existing `registry.rs` instance method
//! (`int_assoc`/`float_assoc`/`bignum_assoc`/`ratio_assoc`/`char_assoc`) —
//! see `as_conversion`'s doc comment in `src/check/checker.rs` for the exact
//! table and the `i32`/`i64`-target relabeling it does.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue};

/// Loads the prelude first — `unwrap`/`is-some`/`is-none` are `defmethod`s
/// in `src/prelude.rs`, not checker-native.
fn run_with_heap(src: &str) -> Result<(Heap, RtValue), EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok((h, last))
}

fn run(src: &str) -> Result<RtValue, EvalError> {
    run_with_heap(src).map(|(_, v)| v)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

fn eval_panics(src: &str) -> String {
    match run(src) {
        Err(EvalError::Panic(msg)) => msg,
        other => panic!("expected a panic, got {:?}", other),
    }
}

fn check(src: &str) -> Result<(), Error> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    for v in vs {
        chk.check_form(&mut h, &interp, v)?;
    }
    Ok(())
}

fn bignum(s: &str) -> num_bigint::BigInt {
    s.parse().unwrap()
}

/// Evaluate `src` and assert its result is the `bignum` `expected`.
///
/// Takes the source rather than an already-evaluated `RtValue` because a
/// `bignum` is a GC-heap box now, not a Rust-side `Rc` — reading one back
/// needs the heap it lives in, and `run` drops its heap on return.
fn assert_bignum(src: &str, expected: &str) {
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        RtValue::Sexpr(typelisp::Value::Boxed(id)) if h.is_bignum(id) => {
            assert_eq!(*h.bignum_value(id), bignum(expected))
        }
        other => panic!("expected a bignum, got {:?}", other),
    }
}

/// The `ratio` counterpart of [`assert_bignum`].
fn assert_ratio(src: &str, numer: &str, denom: &str) {
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        RtValue::Sexpr(typelisp::Value::Boxed(id)) if h.is_ratio(id) => {
            let r = h.ratio_value(id);
            assert_eq!(*r.numer(), bignum(numer));
            assert_eq!(*r.denom(), bignum(denom));
        }
        other => panic!("expected a ratio, got {:?}", other),
    }
}

// ---- identity ----------------------------------------------------------------

#[test]
fn as_on_the_same_type_is_a_no_op() {
    assert_eq!(eval_ok("(as i32 5)"), RtValue::Int(5));
    assert_eq!(eval_ok("(as f64 2.5)"), RtValue::Float(2.5));
    assert_eq!(eval_ok("(as char #\\a)"), RtValue::Char('a'));
}

#[test]
fn try_as_on_the_same_type_wraps_in_some() {
    assert_eq!(
        eval_ok("(unwrap (try-as i32 5))"),
        RtValue::Int(5)
    );
    assert_eq!(eval_ok("(is-some (try-as i32 5))"), RtValue::Bool(true));
}

// ---- i32 <-> i64: pure relabel ------------------------------------------------

#[test]
fn as_crosses_i32_and_i64_by_relabeling() {
    assert_eq!(eval_ok("(as i64 (the i32 7))"), RtValue::Int(7));
    assert_eq!(eval_ok("(as i32 (the i64 7))"), RtValue::Int(7));
}

#[test]
fn try_as_crosses_i32_and_i64_and_wraps_in_some() {
    assert_eq!(eval_ok("(is-some (try-as i64 (the i32 7)))"), RtValue::Bool(true));
    assert_eq!(eval_ok("(unwrap (try-as i64 (the i32 7)))"), RtValue::Int(7));
}

// ---- total conversions: int <-> f64 -------------------------------------------

#[test]
fn as_widens_an_int_to_f64() {
    assert_eq!(eval_ok("(as f64 42)"), RtValue::Float(42.0));
    assert_eq!(eval_ok("(as f64 (the i64 42))"), RtValue::Float(42.0));
}

#[test]
fn as_narrows_f64_to_int_truncating_toward_zero() {
    assert_eq!(eval_ok("(as i32 3.9)"), RtValue::Int(3));
    assert_eq!(eval_ok("(as i32 (- 0.0 3.9))"), RtValue::Int(-3));
    // i64 target: same underlying `float->int` call, relabeled.
    assert_eq!(eval_ok("(as i64 3.9)"), RtValue::Int(3));
}

#[test]
fn try_as_int_f64_round_trip_always_succeeds() {
    assert_eq!(eval_ok("(unwrap (try-as f64 42))"), RtValue::Float(42.0));
    assert_eq!(eval_ok("(unwrap (try-as i32 3.9))"), RtValue::Int(3));
    assert_eq!(eval_ok("(unwrap (try-as i64 3.9))"), RtValue::Int(3));
}

// ---- total conversions: int/bignum/ratio widening -----------------------------

#[test]
fn as_widens_int_to_bignum_and_ratio() {
    assert_bignum("(as bignum 42)", "42");
    assert_bignum("(as bignum (the i64 42))", "42");
    assert_ratio("(as ratio 42)", "42", "1");
}

#[test]
fn as_widens_bignum_to_ratio_and_f64() {
    assert_ratio("(as ratio 99999999999999999999999999999)", "99999999999999999999999999999", "1");
    match eval_ok("(as f64 99999999999999999999999999999)") {
        RtValue::Float(f) => assert!(f > 9.9e28 && f < 1.1e29, "unexpected float: {}", f),
        other => panic!("expected a Float, got {:?}", other),
    }
}

#[test]
fn as_widens_ratio_to_f64() {
    match eval_ok("(as f64 2/3)") {
        RtValue::Float(f) => assert!((f - 2.0 / 3.0).abs() < 1e-12, "unexpected float: {}", f),
        other => panic!("expected a Float, got {:?}", other),
    }
}

#[test]
fn as_converts_f64_to_bignum_and_ratio() {
    // `float->bignum` truncates toward zero (like `float->int`); `float->ratio`
    // is exact. Both methods already existed — `as` now reaches them.
    assert_bignum("(as bignum 3.9)", "3");
    assert_bignum("(as bignum (- 0.0 3.9))", "-3");
    assert_ratio("(as ratio 0.5)", "1", "2");
}

#[test]
fn as_narrows_ratio_to_bignum_by_truncating() {
    // `ratio->bignum` truncates toward zero, consistent with `float->int`.
    assert_bignum("(as bignum 2/3)", "0");
    assert_bignum("(as bignum 7/2)", "3");
    assert_bignum("(as bignum -7/2)", "-3");
}

// ---- total conversion: char <-> int -------------------------------------------

#[test]
fn as_widens_char_to_int() {
    assert_eq!(eval_ok("(as i32 #\\A)"), RtValue::Int(65));
    assert_eq!(eval_ok("(as i64 #\\A)"), RtValue::Int(65));
}

#[test]
fn try_as_char_to_int_always_succeeds() {
    assert_eq!(eval_ok("(unwrap (try-as i32 #\\A))"), RtValue::Int(65));
}

// ---- partial conversion: int -> char ------------------------------------------

#[test]
fn as_narrows_a_valid_scalar_int_to_char() {
    assert_eq!(eval_ok("(as char 65)"), RtValue::Char('A'));
    assert_eq!(eval_ok("(as char (the i64 65))"), RtValue::Char('A'));
}

#[test]
fn as_panics_converting_an_invalid_scalar_int_to_char() {
    let msg = eval_panics("(as char -1)");
    assert!(msg.contains("int->char"), "unexpected message: {}", msg);
}

#[test]
fn try_as_returns_some_or_none_converting_int_to_char() {
    assert_eq!(eval_ok("(unwrap (try-as char 65))"), RtValue::Char('A'));
    assert_eq!(eval_ok("(is-none (try-as char -1))"), RtValue::Bool(true));
    // i64 source reaches the same `try-int->char` builtin.
    assert_eq!(eval_ok("(is-none (try-as char (the i64 -1)))"), RtValue::Bool(true));
}

// ---- partial conversion: bignum -> int ----------------------------------------

#[test]
fn as_narrows_an_in_range_bignum_to_int() {
    assert_eq!(eval_ok("(as i32 42)"), RtValue::Int(42));
}

#[test]
fn as_panics_narrowing_an_out_of_range_bignum_to_int() {
    let msg = eval_panics("(as i32 99999999999999999999999999999)");
    assert!(msg.contains("bignum->int"), "unexpected message: {}", msg);
    let msg64 = eval_panics("(as i64 99999999999999999999999999999)");
    assert!(msg64.contains("bignum->int"), "unexpected message: {}", msg64);
}

#[test]
fn try_as_returns_none_narrowing_an_out_of_range_bignum_to_int() {
    assert_eq!(eval_ok("(is-none (try-as i32 99999999999999999999999999999))"), RtValue::Bool(true));
    assert_eq!(eval_ok("(is-none (try-as i64 99999999999999999999999999999))"), RtValue::Bool(true));
}

#[test]
fn try_as_returns_some_narrowing_an_in_range_bignum_to_int() {
    // `(as bignum 42)` widens a plain int literal into a genuine `bignum`
    // value that comfortably fits back in an `i64`.
    assert_eq!(eval_ok("(unwrap (try-as i32 (as bignum 42)))"), RtValue::Int(42));
    assert_eq!(eval_ok("(unwrap (try-as i64 (as bignum 42)))"), RtValue::Int(42));
}

// ---- excluded pairs and out-of-domain types (check-time errors) --------------

#[test]
fn ratio_to_int_is_not_covered_by_as() {
    // `ratio->bignum`/`ratio->float` exist (and `as` reaches them), but there
    // is no direct `ratio->int` method, so `(as i32 2/3)` stays excluded —
    // route through `bignum` (`(as i32 (as bignum 2/3))`) instead.
    let err = check("(as i32 2/3)").expect_err("ratio->int should be excluded");
    assert!(format!("{:?}", err).contains("no conversion"), "unexpected error: {:?}", err);
}

#[test]
fn as_rejects_a_type_outside_the_numeric_char_catalog() {
    let err = check("(as string 5)").expect_err("str is out of as's domain");
    assert!(format!("{:?}", err).contains("no conversion"), "unexpected error: {:?}", err);
}

#[test]
fn try_as_rejects_a_type_outside_the_numeric_char_catalog() {
    let err = check("(try-as string 5)").expect_err("str is out of try-as's domain");
    assert!(format!("{:?}", err).contains("no conversion"), "unexpected error: {:?}", err);
}
