//! Tests for `as`/`try-as` (`Checker::check_as`) — a Rust-`as`-flavored
//! primitive cast between the numeric/`char` types, unrelated to `Sexpr`
//! (that's `match`'s job, see `tests/match_sexpr_test.rs`). `as` panics on a
//! partial conversion's failure; `try-as` returns `Option<Type>` instead.
//!
//! Both desugar to an existing `registry.rs` instance method
//! (`int_assoc`/`float_assoc`/`bignum_assoc`/`ratio_assoc`/`char_assoc`) —
//! see `as_conversion`'s doc comment in `src/check/checker.rs` for the exact
//! table, and `Checker::width_cast` for the same-family width casts it
//! chains onto the result.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

/// Loads the prelude first — `unwrap`/`is-some`/`is-none` are `defmethod`s
/// in `src/prelude.rs`, not checker-native.
fn run_with_heap(src: &str) -> Result<(Heap, Value), EvalError> {
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
    Ok((h, last))
}

fn run(src: &str) -> Result<Value, EvalError> {
    run_with_heap(src).map(|(_, v)| v)
}

fn eval_ok(src: &str) -> Value {
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
        typelisp::Value::Boxed(id) if h.is_bignum(id) => {
            assert_eq!(*h.bignum_value(id), bignum(expected))
        }
        other => panic!("expected a bignum, got {:?}", other),
    }
}

/// The `ratio` counterpart of [`assert_bignum`].
fn assert_ratio(src: &str, numer: &str, denom: &str) {
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        typelisp::Value::Boxed(id) if h.is_ratio(id) => {
            let r = h.ratio_value(id);
            assert_eq!(*r.numer(), bignum(numer));
            assert_eq!(*r.denom(), bignum(denom));
        }
        other => panic!("expected a ratio, got {:?}", other),
    }
}

/// The `f64` counterpart of [`assert_bignum`]: an `f64` is a
/// `BoxedObj::Float` since the scalar unification, so reading one needs the
/// heap it lives in.
fn eval_f64(src: &str) -> f64 {
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        typelisp::Value::Boxed(id) if h.is_float(id) => h.float_value(id),
        other => panic!("expected an f64, got {:?}", other),
    }
}

// ---- identity ----------------------------------------------------------------

#[test]
fn as_on_the_same_type_is_a_no_op() {
    assert_eq!(eval_ok("(as i32 5)"), Value::Int(5));
    assert_eq!(eval_f64("(as f64 2.5)"), 2.5);
    assert_eq!(eval_ok("(as char #\\a)"), Value::Char('a'));
}

#[test]
fn try_as_on_the_same_type_wraps_in_some() {
    assert_eq!(
        eval_ok("(unwrap (try-as i32 5))"),
        Value::Int(5)
    );
    assert_eq!(eval_ok("(is-some (try-as i32 5))"), Value::Bool(true));
}

// ---- between two integer widths: a real conversion ----------------------------

/// Crossing widths used to be a relabel, back when every integer shared one
/// 64-bit representation. It converts now: `as` truncates to the target's
/// width, which is what a width cast means.
#[test]
fn as_crosses_integer_widths_by_truncating() {
    assert_eq!(eval_ok("(as u8 (the i32 7))"), Value::Int(7));
    assert_eq!(eval_ok("(as i32 (the u8 7))"), Value::Int(7));
    assert_eq!(eval_ok("(as u8 (the i32 300))"), Value::Int(44));
    assert_eq!(eval_ok("(as i8 (the i32 200))"), Value::Int(-56));
}

#[test]
fn try_as_between_integer_widths_answers_whether_it_fits() {
    assert_eq!(eval_ok("(is-some (try-as u8 (the i32 7)))"), Value::Bool(true));
    assert_eq!(eval_ok("(unwrap (try-as u8 (the i32 7)))"), Value::Int(7));
    assert_eq!(eval_ok("(is-none (try-as u8 (the i32 300)))"), Value::Bool(true));
}

// ---- total conversions: int <-> f64 -------------------------------------------

#[test]
fn as_widens_an_int_to_f64() {
    assert_eq!(eval_f64("(as f64 42)"), 42.0);
    assert_eq!(eval_f64("(as f64 (the u8 42))"), 42.0);
}

#[test]
fn as_narrows_f64_to_int_truncating_toward_zero() {
    assert_eq!(eval_ok("(as i32 3.9)"), Value::Int(3));
    assert_eq!(eval_ok("(as i32 (- 0.0 3.9))"), Value::Int(-3));
    // A narrower target: the same `float->int` call, with the width cast
    // chained onto its `i32` result.
    assert_eq!(eval_ok("(as u8 3.9)"), Value::Int(3));
}

#[test]
fn try_as_int_f64_round_trip_always_succeeds() {
    assert_eq!(eval_f64("(unwrap (try-as f64 42))"), 42.0);
    assert_eq!(eval_ok("(unwrap (try-as i32 3.9))"), Value::Int(3));
    assert_eq!(eval_ok("(unwrap (try-as u8 3.9))"), Value::Int(3));
}

// ---- total conversions: int/bignum/ratio widening -----------------------------

#[test]
fn as_widens_int_to_bignum_and_ratio() {
    assert_bignum("(as bignum 42)", "42");
    assert_bignum("(as bignum (the u8 42))", "42");
    assert_ratio("(as ratio 42)", "42", "1");
}

#[test]
fn as_widens_bignum_to_ratio_and_f64() {
    assert_ratio("(as ratio 99999999999999999999999999999)", "99999999999999999999999999999", "1");
    let f = eval_f64("(as f64 99999999999999999999999999999)");
    assert!(f > 9.9e28 && f < 1.1e29, "unexpected float: {}", f);
}

#[test]
fn as_widens_ratio_to_f64() {
    let f = eval_f64("(as f64 2/3)");
    assert!((f - 2.0 / 3.0).abs() < 1e-12, "unexpected float: {}", f);
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
    assert_eq!(eval_ok("(as i32 #\\A)"), Value::Int(65));
    assert_eq!(eval_ok("(as u16 #\\A)"), Value::Int(65));
}

#[test]
fn try_as_char_to_int_always_succeeds() {
    assert_eq!(eval_ok("(unwrap (try-as i32 #\\A))"), Value::Int(65));
}

// ---- partial conversion: int -> char ------------------------------------------

#[test]
fn as_narrows_a_valid_scalar_int_to_char() {
    assert_eq!(eval_ok("(as char 65)"), Value::Char('A'));
    assert_eq!(eval_ok("(as char (the u8 65))"), Value::Char('A'));
}

#[test]
fn as_panics_converting_an_invalid_scalar_int_to_char() {
    let msg = eval_panics("(as char -1)");
    assert!(msg.contains("int->char"), "unexpected message: {}", msg);
}

#[test]
fn try_as_returns_some_or_none_converting_int_to_char() {
    assert_eq!(eval_ok("(unwrap (try-as char 65))"), Value::Char('A'));
    assert_eq!(eval_ok("(is-none (try-as char -1))"), Value::Bool(true));
    // Another width's source reaches the same `try-int->char` builtin.
    assert_eq!(eval_ok("(is-none (try-as char (the i16 -1)))"), Value::Bool(true));
}

// ---- partial conversion: bignum -> int ----------------------------------------

#[test]
fn as_narrows_an_in_range_bignum_to_int() {
    assert_eq!(eval_ok("(as i32 42)"), Value::Int(42));
}

#[test]
fn as_panics_narrowing_an_out_of_range_bignum_to_int() {
    let msg = eval_panics("(as i32 99999999999999999999999999999)");
    assert!(msg.contains("bignum->int"), "unexpected message: {}", msg);
    // A narrower target fails in the same place: `bignum->int` runs first,
    // and the width cast is only chained onto a result it produced.
    let msg8 = eval_panics("(as u8 99999999999999999999999999999)");
    assert!(msg8.contains("bignum->int"), "unexpected message: {}", msg8);
}

#[test]
fn try_as_returns_none_narrowing_an_out_of_range_bignum_to_int() {
    assert_eq!(eval_ok("(is-none (try-as i32 99999999999999999999999999999))"), Value::Bool(true));
}

#[test]
fn try_as_returns_some_narrowing_an_in_range_bignum_to_int() {
    // `(as bignum 42)` widens a plain int literal into a genuine `bignum`
    // value that comfortably fits back in an `i32`.
    assert_eq!(eval_ok("(unwrap (try-as i32 (as bignum 42)))"), Value::Int(42));
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

/// A `try-as` that would have to cross families *and* narrow is refused
/// rather than answered: `(try-as u8 some-bignum)` would pack "did it
/// convert" and "does it then fit `u8`" into one `option`, and the `none`
/// could not say which. The message spells out the two-step form.
#[test]
fn try_as_refuses_to_cross_a_family_and_narrow_in_one_question() {
    let err = check("(try-as u8 99999999999999999999999999999)")
        .expect_err("bignum -> u8 in one `try-as` should be refused");
    let msg = format!("{:?}", err);
    assert!(msg.contains("two questions"), "unexpected error: {}", msg);
}

// ---- between the two float widths ---------------------------------------------

/// `f32` is binary32, so narrowing rounds and `try-as` reports whether the
/// rounding lost anything. Widening is exact in both spellings.
#[test]
fn as_crosses_float_widths_by_rounding() {
    assert!((eval_f64("(as f64 (as f32 0.5))") - 0.5).abs() < f64::EPSILON);
    assert_eq!(eval_ok("(is-some (try-as f32 0.5))"), Value::Bool(true));
    assert_eq!(eval_ok("(is-none (try-as f32 0.1))"), Value::Bool(true));
    // Widening never fails, and `try-as` says so.
    assert_eq!(eval_ok("(is-some (try-as f64 (as f32 0.5)))"), Value::Bool(true));
}
