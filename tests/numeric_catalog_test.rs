//! The rest of CL's numeric catalog, added by
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 1c: the `f`-prefixed
//! rounding names, `isqrt`, integer `expt`, the float-representation
//! accessors, `rationalize`, the limit constants, and n-ary `gcd`/`lcm`.
//!
//! Everything but the `gcd`/`lcm` arities is a prelude definition; those are
//! checker sugar (`check_variadic_arith` / `check_nullary_or_unary_numeric_op`),
//! which is why a one- and a zero-argument call are tested explicitly.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

/// Runs `expr` with the prelude loaded and renders it with `~a`.
fn show(expr: &str) -> String {
    let src = format!("(format false \"~a\" {})", expr);
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, &src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// CL's `ffloor`/`fceiling`/`fround`/`ftruncate` round but stay a float —
/// which is what this language's `f64` `floor`/`ceiling`/`round`/`truncate`
/// already do, so these are aliases carrying the CL spelling.
#[test]
fn the_f_prefixed_rounding_names_stay_floats() {
    assert_eq!(show("(ffloor 2.7)"), "2.0");
    assert_eq!(show("(fceiling 2.1)"), "3.0");
    assert_eq!(show("(ftruncate -2.7)"), "-2.0");
    assert_eq!(show("(ffloor -2.1)"), "-3.0");
    // Ties round *away from zero*, not to even. CL specifies to-even, so
    // `(fround 2.5)` is 2 there and 3.0 here — a divergence inherited from the
    // existing `round` builtin, recorded in `docs/functions.md` §2 rather than
    // silently fixed in the alias (which would leave `fround` and `round`
    // disagreeing with each other).
    assert_eq!(show("(fround 2.5)"), "3.0");
    assert_eq!(show("(fround 2.4)"), "2.0");
}

#[test]
fn isqrt_is_the_greatest_root_that_does_not_overshoot() {
    assert_eq!(show("(isqrt 0)"), "0");
    assert_eq!(show("(isqrt 1)"), "1");
    // 15 is between 3^2 and 4^2, and 16 is exact — the two cases the
    // oscillating form of the Newton loop got wrong.
    assert_eq!(show("(isqrt 15)"), "3");
    assert_eq!(show("(isqrt 16)"), "4");
    assert_eq!(show("(isqrt 24)"), "4");
    assert_eq!(show("(isqrt 1000000)"), "1000");
    assert_eq!(show("(isqrt (the i32 1000000000000))"), "1000000");
}

#[test]
fn integer_expt_squares_and_rejects_a_negative_exponent() {
    assert_eq!(show("(expt 2 10)"), "1024");
    assert_eq!(show("(expt 3 0)"), "1");
    assert_eq!(show("(expt 5 3)"), "125");
    assert_eq!(show("(expt -2 3)"), "-8");
    assert_eq!(show("(expt (the i32 2) (the i32 40))"), "1099511627776");
}

#[test]
fn the_float_accessors_report_the_binary64_representation() {
    assert_eq!(show("(float-radix 1.0)"), "2");
    assert_eq!(show("(float-digits 1.0)"), "53");
    assert_eq!(show("(float-precision 1.0)"), "53");
    // CL: a zero has no precision.
    assert_eq!(show("(float-precision 0.0)"), "0");
    assert_eq!(show("(float-sign -3.5)"), "-1.0");
    assert_eq!(show("(float-sign 3.5)"), "1.0");
    assert_eq!(show("(scale-float 1.5 3)"), "12.0");
    assert_eq!(show("(scale-float 8.0 -2)"), "2.0");
    assert_eq!(show("(scale-float 1.5 0)"), "1.5");
}

/// `decode-float` splits into a significand in `[1/2,1)` and the exponent that
/// puts it back. CL returns three values; the sign is `float-sign` and the
/// pair carries the other two.
#[test]
fn decode_float_normalizes_into_a_half_open_significand() {
    assert_eq!(show("(decode-float 8.0)"), "#<cons-cell<f64,i32> 0.5 4>");
    assert_eq!(show("(decode-float 0.75)"), "#<cons-cell<f64,i32> 0.75 0>");
    assert_eq!(show("(decode-float 0.0)"), "#<cons-cell<f64,i32> 0.0 0>");
    // The significand is unsigned, as CL specifies.
    assert_eq!(show("(decode-float -8.0)"), "#<cons-cell<f64,i32> 0.5 4>");
    // `integer-decode-float` gives the same split with an exact 53-bit
    // significand: 2^52 * 2^-49 = 8.
    assert_eq!(show("(integer-decode-float 8.0)"), "#<cons-cell<bignum,i32> 4503599627370496 -49>");
}

/// CL's `rationalize` is the *simplest* rational reading back as the float;
/// `float->ratio` (CL's `rational`) is the exact binary value. `0.1` is where
/// the two visibly part company.
#[test]
fn rationalize_finds_the_simplest_rational_not_the_exact_one() {
    assert_eq!(show("(rationalize 0.1)"), "1/10");
    assert_eq!(show("(rationalize 0.5)"), "1/2");
    assert_eq!(show("(rationalize 0.25)"), "1/4");
    assert_eq!(show("(rationalize 1.5)"), "3/2");
    assert_eq!(show("(rationalize 3.0)"), "3/1");
    assert_eq!(show("(float->ratio 0.1)"), "3602879701896397/36028797018963968");
}

#[test]
fn the_limit_constants_hold_their_defining_properties() {
    assert_eq!(show("most-positive-fixnum"), "9223372036854775807");
    assert_eq!(show("most-negative-fixnum"), "-9223372036854775808");
    // CL defines the epsilons by these predicates, and they are the reason
    // `double-float-epsilon` is one ULP above 2^-53 rather than 2^-53 itself.
    assert_eq!(show("(/= (+ 1.0 double-float-epsilon) 1.0)"), "true");
    assert_eq!(show("(/= (- 1.0 double-float-negative-epsilon) 1.0)"), "true");
    assert_eq!(show("(> most-positive-double-float 0.0)"), "true");
    assert_eq!(show("(> least-positive-double-float 0.0)"), "true");
    assert_eq!(show("(< least-positive-double-float least-positive-normalized-double-float)"), "true");
}

/// `gcd`/`lcm` were binary methods; the zero-, one- and n-argument forms are
/// checker sugar over them, the same expansion `+`/`max` get.
#[test]
fn gcd_and_lcm_take_any_number_of_arguments() {
    // CL's identities: every integer divides 0, and 1 is the multiplicative one.
    assert_eq!(show("(gcd)"), "0");
    assert_eq!(show("(lcm)"), "1");
    // One argument is the *absolute value*, not the argument.
    assert_eq!(show("(gcd -4)"), "4");
    assert_eq!(show("(lcm -4)"), "4");
    assert_eq!(show("(gcd 12 18)"), "6");
    assert_eq!(show("(gcd 12 18 27)"), "3");
    assert_eq!(show("(lcm 4 6)"), "12");
    assert_eq!(show("(lcm 4 6 10)"), "60");
}
