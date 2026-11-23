//! Construction, arithmetic and conversion of boxed bignums and ratios.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::Value;

// ---- bignum/ratio compiled representation -------------------------------
//
// `bignum`/`ratio` (`num_bigint::BigInt`/`num_rational::BigRational`) are
// `Sign`+`Vec<u32>`-shaped Rust structs with no fixed-width FFI-safe layout —
// unlike `f64`, there is no "native register" form to fall back to, so a
// `Type::Bignum`/`Type::Ratio` value's compiled representation is *always* a
// tagged `TAG_BOXED` `i64` pointing at a `BoxedObj::Bignum`/`Ratio`, exactly
// the same convention `Type::Str` already uses (never bare bits the way
// `f64` is). `rt_box_kind` (`scalar_box`) already discriminates them (`2`/`3`); the
// functions below are the construction/arithmetic/conversion primitives that
// were still missing (`docs/dev/implementation-log.md`'s "`bignum`/`ratio`型の
// compile対応"
// follow-up).
//
// None of these need extra GC-root bookkeeping beyond what the caller
// already does for any tagged argument it passes in: allocating a new
// `BoxedObj` (`box_slots`) never triggers the cons-heap's mark-sweep (that
// only fires on cons-arena exhaustion), the same assumption `rt_struct_new`/
// `rt_f64_new` already rely on.
//
// Division by zero is checked explicitly (`Sign::NoSign`) before calling
// into `BigInt`/`BigRational`'s `Div`/`Rem` operators, which otherwise raise
// a genuine Rust `panic!` — unwinding that across this `extern "C"` boundary
// is undefined behavior (see `fatal`'s doc comment), unlike an `i64` divide
// by zero, which traps at the machine level instead.

use num_bigint::{BigInt, BigUint, Sign};
use num_rational::BigRational;
use num_traits::{FromPrimitive, ToPrimitive};

/// Decodes `args[idx]` as a boxed bignum and clones its `BigInt` out —
/// shared preamble for every `rt_bignum_*`/`rt_ratio_from_bignums` function
/// below.
///
/// # Safety
///
/// `args` must be valid for at least `idx + 1` `i64`s, the one at `idx`
/// decoding to a `Value::Boxed` bignum; a `Heap` must already be registered
/// on this thread.
unsafe fn bignum_arg(args: *const i64, idx: isize, who: &str) -> BigInt {
    match decode(*args.offset(idx)) {
        // An `int` in either shape: `rt_bignum_new` answers a fixnum for a
        // literal that fits one (a ratio literal's numerator, say).
        Value::Int(n) => BigInt::from(n),
        Value::Boxed(id) => active_heap().bignum_value(id).clone(),
        _ => fatal(&format!("{who}: argument is not an int")),
    }
}

/// The two-bignum-operand preamble every `rt_bignum_*` binop/comparison
/// shares.
///
/// # Safety
///
/// Same as [`bignum_arg`], for both `args[0]` and `args[1]`.
unsafe fn bignum_pair(args: *const i64, argc: u32, who: &str) -> (BigInt, BigInt) {
    if argc < 2 {
        fatal(&format!("{who}: expected 2 arguments"));
    }
    (bignum_arg(args, 0, who), bignum_arg(args, 1, who))
}

/// Decodes `args[idx]` as a boxed ratio and clones its `BigRational` out —
/// the `ratio` counterpart of [`bignum_arg`].
///
/// # Safety
///
/// Same as [`bignum_arg`].
unsafe fn ratio_arg(args: *const i64, idx: isize, who: &str) -> BigRational {
    match decode(*args.offset(idx)) {
        Value::Boxed(id) => active_heap().ratio_value(id).clone(),
        _ => fatal(&format!("{who}: argument is not a boxed ratio")),
    }
}

/// The two-ratio-operand preamble every `rt_ratio_*` binop/comparison
/// shares — the `ratio` counterpart of [`bignum_pair`].
///
/// # Safety
///
/// Same as [`bignum_pair`].
unsafe fn ratio_pair(args: *const i64, argc: u32, who: &str) -> (BigRational, BigRational) {
    if argc < 2 {
        fatal(&format!("{who}: expected 2 arguments"));
    }
    (ratio_arg(args, 0, who), ratio_arg(args, 1, who))
}

/// Allocates a boxed bignum from a compile-time-embedded literal:
/// `args[0]` is the sign (`-1`/`0`/`1`), `args[1..]` are the magnitude's
/// base-2^32 digits, least-significant first (`BigInt::to_u32_digits`'s own
/// shape, reversed at construction time by `compiler.rs`'s
/// `compile-bignum-literal`) — the same "argc-many raw payload scalars"
/// convention [`rt_str_new`] uses for a literal's characters.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least `argc` valid
/// `i64`s; a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_bignum_new: expected at least 1 argument (the sign)");
    }
    let sign = match *args {
        -1 => Sign::Minus,
        0 => Sign::NoSign,
        1 => Sign::Plus,
        other => fatal(&format!("rt_bignum_new: invalid sign {other}")),
    };
    let mut digits = Vec::with_capacity(argc as usize - 1);
    for i in 1..argc as isize {
        digits.push(*args.offset(i) as u32);
    }
    let n = BigInt::from_biguint(sign, BigUint::new(digits));
    // Canonical: a literal the checker spelled as `(bignum ..)` is past the
    // fixnum range, but a ratio literal's parts arrive here too and may not
    // be.
    encode(active_heap().int_from_bigint(n))
}

/// Builds a boxed ratio from two already-boxed bignums (`args[0]`=numerator,
/// `args[1]`=denominator) — `BigRational::new` reduces to lowest terms and
/// normalizes the sign, the same invariant every other `ratio` construction
/// path (the reader, `bignum->ratio`, ...) already relies on. Used both for
/// a ratio literal (`compiler.rs`'s `compile-ratio-literal`, over two nested
/// `compile-bignum-literal` calls) and `bignum->ratio`.
///
/// The zero-denominator `fatal` below stays a `fatal` — checked, and it is
/// unreachable from source. The only way to build one is a literal, and the
/// reader rejects `1/0` outright ("ratio literal with zero denominator", as
/// CL's reader does); a value that got here with a zero denominator was
/// mis-built by the compiler, not written by a user.
///
/// # Safety
///
/// `argc` must be `>= 2`, both decoding to boxed bignums; a `Heap` must
/// already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_from_bignums(args: *const i64, argc: u32) -> i64 {
    let (numer, denom) = bignum_pair(args, argc, "rt_ratio_from_bignums");
    if denom.sign() == Sign::NoSign {
        fatal("rt_ratio_from_bignums: zero denominator");
    }
    encode(active_heap().alloc_ratio(BigRational::new(numer, denom)))
}

/// `int->ratio` for compiled code — always exact widening, denominator `1`.
/// A direct primitive rather than routing through [`rt_int_to_bignum`] +
/// [`rt_ratio_from_bignums`], since a native-int receiver's compiled
/// representation (a plain `i64`) never needs the intermediate bignum boxed
/// at all here.
///
/// # Safety
///
/// Same as [`rt_int_to_bignum`].
#[no_mangle]
pub unsafe extern "C" fn rt_int_to_ratio(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_int_to_ratio: expected 1 argument");
    }
    encode(active_heap().alloc_ratio(BigRational::from_integer(BigInt::from(*args))))
}

/// `float->int` for compiled code — truncating toward zero into the
/// arbitrary-precision `int` (`BigInt::from_f64`, exact for any finite
/// value, then `Heap::int_from_bigint` for the canonical word). A
/// non-finite input [`raise`]s: there is no integer to truncate to, and the
/// interpreter's `float_to_int` panics the same way.
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` an `f64`'s raw bit pattern; a `Heap` must
/// be registered.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_float_to_int(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_float_to_int: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    match BigInt::from_f64(f) {
        Some(n) => encode(active_heap().int_from_bigint(n)),
        None => raise(format!("float->int: {} is not finite", f)).into_word(),
    }
}

/// `float->ratio` for compiled code — widening and exact: every finite
/// `f64` is itself an exact dyadic rational (`BigRational::from_float`, CL's
/// `rational` rather than the lossy-round-trip `rationalize`). Fatal on a
/// non-finite input.
///
/// # Safety
///
/// Same as [`rt_float_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_float_to_ratio(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_float_to_ratio: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    match BigRational::from_float(f) {
        Some(r) => encode(active_heap().alloc_ratio(r)),
        None => fatal("rt_float_to_ratio: value is not finite"),
    }
}

/// `ratio::+` for compiled code.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_add(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_add");
    encode(active_heap().alloc_ratio(a + b))
}

/// `ratio::-` for compiled code.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_sub(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_sub");
    encode(active_heap().alloc_ratio(a - b))
}

/// `ratio::*` for compiled code.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_mul(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_mul");
    encode(active_heap().alloc_ratio(a * b))
}

/// `ratio::/` for compiled code. A zero divisor (checked via the divisor's
/// numerator sign — a reduced ratio is zero iff its numerator is, its
/// denominator always being a positive nonzero invariant) [`raise`]s
/// `"divide by zero"`, `eval_ratio_builtin`'s own wording.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ratio_div(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_div");
    if b.numer().sign() == Sign::NoSign {
        return raise("divide by zero".to_string()).into_word();
    }
    encode(active_heap().alloc_ratio(a / b))
}

/// Three-way comparison for compiled code — the `ratio` counterpart of
/// [`rt_bignum_cmp`], same derivation shape for all six comparisons +
/// `eq`/`eql`/`equal`/`equalp`.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_cmp(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_cmp");
    match a.cmp(&b) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

/// `ratio->int` for compiled code — truncates toward zero
/// (`Ratio::to_integer`).
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` a boxed ratio; a `Heap` must be
/// registered.
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_to_int(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_to_int: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_to_int");
    encode(active_heap().int_from_bigint(r.to_integer()))
}

/// `ratio->float` for compiled code — widening, rounded.
///
/// # Safety
///
/// Same as [`rt_ratio_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_to_float(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_to_float: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_to_float");
    let f = match r.to_f64() {
        Some(f) => f,
        None => fatal("rt_ratio_to_float: conversion failed"),
    };
    f.to_bits() as i64
}

/// `numerator` for compiled code — the (already-reduced) numerator as an
/// `int`; `ratio` itself is never mutated.
///
/// # Safety
///
/// Same as [`rt_ratio_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_numerator(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_numerator: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_numerator");
    encode(active_heap().int_from_bigint(r.numer().clone()))
}

/// `denominator` for compiled code — the `numerator` counterpart.
///
/// # Safety
///
/// Same as [`rt_ratio_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_denominator(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_denominator: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_denominator");
    encode(active_heap().int_from_bigint(r.denom().clone()))
}
