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
    // existing `round` builtin, recorded in `docs/ja/reference/functions/numbers.md` §4 rather than
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
    // The largest exact square an `i32` holds: 46340^2 = 2147395600.
    assert_eq!(show("(isqrt 2147395600)"), "46340");
    assert_eq!(show("(isqrt 1000000000)"), "31622");
}

#[test]
fn integer_expt_squares_and_rejects_a_negative_exponent() {
    assert_eq!(show("(expt 2 10)"), "1024");
    assert_eq!(show("(expt 3 0)"), "1");
    assert_eq!(show("(expt 5 3)"), "125");
    assert_eq!(show("(expt -2 3)"), "-8");
    // 2^30, the largest power of two an `i32` holds. Past that the result
    // wraps at the type's own width, as every other `i32` operation does.
    assert_eq!(show("(expt (the i32 2) (the i32 30))"), "1073741824");
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
    assert_eq!(show("(decode-float 8.0)"), "#<cons-cell<f64,int> car: 0.5 cdr: 4>");
    assert_eq!(show("(decode-float 0.75)"), "#<cons-cell<f64,int> car: 0.75 cdr: 0>");
    assert_eq!(show("(decode-float 0.0)"), "#<cons-cell<f64,int> car: 0.0 cdr: 0>");
    // The significand is unsigned, as CL specifies.
    assert_eq!(show("(decode-float -8.0)"), "#<cons-cell<f64,int> car: 0.5 cdr: 4>");
    // `integer-decode-float` gives the same split with an exact 53-bit
    // significand: 2^52 * 2^-49 = 8.
    assert_eq!(show("(integer-decode-float 8.0)"), "#<cons-cell<int,int> car: 4503599627370496 cdr: -49>");
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
    // A fixnum is an `int` that fits its 63-bit immediate word (one bit
    // goes to the tag); past these an `int` is a bignum box, and the
    // arithmetic goes on.
    assert_eq!(show("most-positive-fixnum"), "4611686018427387903");
    assert_eq!(show("most-negative-fixnum"), "-4611686018427387904");
    assert_eq!(show("(+ most-positive-fixnum 1)"), "4611686018427387904");
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

// ---- the byte-specifier family (CLHS 22.1.3) ------------------------------

/// `ldb`/`ldb-test`/`mask-field`/`dpb`/`deposit-field` work at every integer
/// width, and take the integer **first**.
///
/// Both halves of that sentence are new, and the second is what bought the
/// first. CL writes `(ldb bytespec integer)`, which put the specifier in the
/// receiver slot — and a specifier is a `cons-cell<i32,i32>` whatever it is
/// applied to, so a method keyed on it could serve exactly one integer type.
/// `ldb` was stuck at `i32` for as long as that order held. With the integer
/// first these are ordinary generic functions bounded by `Bits`, one
/// definition each rather than one per width.
///
/// They had no test at all before this.
#[test]
fn the_byte_specifier_family_works_at_every_width() {
    // i32: bits 0..7 of 0xf580 are 0x80.
    assert_eq!(show("(ldb 62848 (byte 8 0))"), "128");
    // u8: the high nibble of 0xa5.
    assert_eq!(show("(ldb (the u8 165) (byte 4 4))"), "10");
    // u32: bits 8..15 of 0xEE6B2800.
    assert_eq!(show("(ldb (the u32 4000000000) (byte 8 8))"), "40");
    // i8: every bit of -1 is a one, so any field of it is all ones.
    assert_eq!(show("(ldb (the i8 -1) (byte 3 0))"), "7");
    // bignum: bits 4..11 of 0xFFFFF.
    assert_eq!(show("(ldb (as int 1048575) (byte 8 4))"), "255");
    // The rest of the family, on the width they were written for.
    assert_eq!(show("(ldb-test 62848 (byte 8 0))"), "true");
    assert_eq!(show("(ldb-test 62720 (byte 8 0))"), "false");
    assert_eq!(show("(mask-field 62848 (byte 8 8))"), "62720");
    assert_eq!(show("(dpb 62848 255 (byte 8 0))"), "62975");
    assert_eq!(show("(deposit-field 62848 255 (byte 8 0))"), "62975");
    // ...and on one that could not have been written before.
    assert_eq!(show("(dpb (the u8 0) (the u8 5) (byte 4 4))"), "80");
    assert_eq!(show("(mask-field (the u16 65535) (byte 4 4))"), "240");
}

/// `logbitp` takes the integer first too, and its index is an `i32` at every
/// width.
///
/// It had the same shape as `ldb`: CL's `(logbitp index integer)` made the
/// *index* the receiver, so an `i32` index could carry only one signature and
/// the integer could never be anything but an `i32` either.
#[test]
fn logbitp_takes_the_integer_first_at_every_width() {
    assert_eq!(show("(logbitp 62848 7)"), "true");
    assert_eq!(show("(logbitp 62848 0)"), "false");
    assert_eq!(show("(logbitp (the u8 128) 7)"), "true");
    assert_eq!(show("(logbitp (the u8 128) 0)"), "false");
    // Past the width: every bit of a negative signed value is the sign, and
    // every bit of an unsigned one is zero.
    assert_eq!(show("(logbitp (the i8 -1) 40)"), "true");
    assert_eq!(show("(logbitp (the u8 255) 40)"), "false");
    // bignum has no width to run past.
    assert_eq!(show("(logbitp (as int 1048575) 19)"), "true");
    assert_eq!(show("(logbitp (as int 1048575) 20)"), "false");
}

/// `boole`'s sixteen op codes, at a width the old `i32`-only definition could
/// not reach. `op` stays first: a generic free function dispatches on
/// nothing, so there is no receiver slot for the integer to claim and CL's
/// own argument order costs nothing to keep.
#[test]
fn boole_selects_all_sixteen_operations_generically() {
    assert_eq!(show("(boole boole-and 240 60)"), "48");
    assert_eq!(show("(boole boole-ior 240 60)"), "252");
    assert_eq!(show("(boole boole-xor 240 60)"), "204");
    assert_eq!(show("(boole boole-clr 240 60)"), "0");
    assert_eq!(show("(boole boole-1 240 60)"), "240");
    assert_eq!(show("(boole boole-2 240 60)"), "60");
    // -1 is all ones in a signed type; in `u8` the same bits read as 255.
    assert_eq!(show("(boole boole-set 240 60)"), "-1");
    assert_eq!(show("(boole boole-set (the u8 240) (the u8 60))"), "255");
    assert_eq!(show("(boole boole-nand (the u8 240) (the u8 60))"), "207");
    assert_eq!(show("(boole boole-andc1 (the u8 240) (the u8 60))"), "12");
    assert_eq!(show("(boole boole-eqv (the u8 240) (the u8 60))"), "51");
}

/// `bignum`'s `ash` counts bits like every other width's, rather than taking
/// a second bignum.
///
/// A shift distance that could itself be arbitrary precision describes no
/// usable call — `(ash big huge)` names a result with `huge` more bits than
/// `big`, which no machine finishes — so the wider type only made the
/// ordinary call awkward to write.
#[test]
fn an_int_shifts_by_a_bit_count() {
    assert_eq!(show("(ash (as int 1) 100)"), "1267650600228229401496703205376");
    assert_eq!(show("(ash (as int 1267650600228229401496703205376) -100)"), "1");
    // Right past every bit: the sign, as CL's infinite two's complement says.
    assert_eq!(show("(ash (as int 255) -1000)"), "0");
    assert_eq!(show("(ash (as int -255) -1000)"), "-1");
}
