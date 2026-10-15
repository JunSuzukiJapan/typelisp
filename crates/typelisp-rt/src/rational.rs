//! `rational`'s shims — CL's `rational` for compiled code, and the two
//! helpers the interpreter shares with them.
//!
//! A `rational` is an `int` or a `ratio`, and its word is that value's own
//! word: a fixnum, a bignum box or a ratio box. It is always normalized the
//! way CL's arithmetic leaves a rational — a value with denominator 1 is the
//! `int`, never a ratio box — so whether a `rational` is an integer is a
//! question about the word's shape alone ([`rt_rational_is_int`]).
//!
//! Every operation here reads its operands as `BigRational`s and answers the
//! normalized word ([`rational_result`]). An `int` operand is a valid
//! `rational` word as it stands, which is how `int`'s own `/` (whose result
//! is a `rational`) uses [`rt_rational_div`] directly.

use num_bigint::Sign;
use num_rational::BigRational;
use num_traits::ToPrimitive;
use typelisp_abi::{active_heap, fatal, raise};
use typelisp_mem::{decode, encode, Heap, Value};

/// The rational `v` is, if it is an `int` (fixnum or bignum box) or a
/// `ratio` box.
pub fn rational_of(heap: &Heap, v: Value) -> Option<BigRational> {
    match v {
        Value::Int(n) => Some(BigRational::from_integer(n.into())),
        Value::Boxed(id) if heap.is_bignum(id) => Some(BigRational::from_integer(heap.bignum_value(id).clone())),
        Value::Boxed(id) if heap.is_ratio(id) => Some(heap.ratio_value(id).clone()),
        _ => None,
    }
}

/// `r` as a normalized `rational` value: the `int` when its denominator is
/// 1, a `ratio` box otherwise.
pub fn rational_result(heap: &mut Heap, r: BigRational) -> Value {
    if r.is_integer() {
        heap.int_from_bigint(r.to_integer())
    } else {
        heap.alloc_ratio(r)
    }
}

/// # Safety
///
/// `args` must be valid for at least `idx + 1` `i64`s; a `Heap` must be
/// registered on this thread.
unsafe fn rational_arg(args: *const i64, idx: isize, who: &str) -> BigRational {
    match rational_of(active_heap(), decode(*args.offset(idx))) {
        Some(r) => r,
        None => fatal(&format!("{who}: argument is neither an int nor a ratio")),
    }
}

/// # Safety
///
/// Same as [`rational_arg`], for `args[0]` and `args[1]`.
unsafe fn rational_pair(args: *const i64, argc: u32, who: &str) -> (BigRational, BigRational) {
    if argc < 2 {
        fatal(&format!("{who}: expected 2 arguments"));
    }
    (rational_arg(args, 0, who), rational_arg(args, 1, who))
}

/// # Safety
///
/// Same as [`rational_arg`], for `args[0]`.
unsafe fn rational_single(args: *const i64, argc: u32, who: &str) -> BigRational {
    if argc < 1 {
        fatal(&format!("{who}: expected 1 argument"));
    }
    rational_arg(args, 0, who)
}

/// # Safety
///
/// A `Heap` must be registered on this thread.
unsafe fn answer(r: BigRational) -> i64 {
    encode(rational_result(active_heap(), r))
}

/// `rational::+`.
///
/// # Safety
///
/// Same as [`rational_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_add(args: *const i64, argc: u32) -> i64 {
    let (a, b) = rational_pair(args, argc, "rt_rational_add");
    answer(a + b)
}

/// `rational::-`.
///
/// # Safety
///
/// Same as [`rational_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_sub(args: *const i64, argc: u32) -> i64 {
    let (a, b) = rational_pair(args, argc, "rt_rational_sub");
    answer(a - b)
}

/// `rational::*`.
///
/// # Safety
///
/// Same as [`rational_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_mul(args: *const i64, argc: u32) -> i64 {
    let (a, b) = rational_pair(args, argc, "rt_rational_mul");
    answer(a * b)
}

/// `rational::/`, and `int::/`. A zero divisor [`raise`]s the interpreter's
/// own `"divide by zero"`.
///
/// # Safety
///
/// Same as [`rational_pair`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_rational_div(args: *const i64, argc: u32) -> i64 {
    let (a, b) = rational_pair(args, argc, "rt_rational_div");
    if b.numer().sign() == Sign::NoSign {
        raise("divide by zero".to_string());
    }
    answer(a / b)
}

/// Three-way comparison: a raw `-1`/`0`/`1`.
///
/// # Safety
///
/// Same as [`rational_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_cmp(args: *const i64, argc: u32) -> i64 {
    let (a, b) = rational_pair(args, argc, "rt_rational_cmp");
    match a.cmp(&b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// `ratio->rational`: a `ratio` box whose denominator is 1 becomes the
/// `int`; any other comes back as it is.
///
/// # Safety
///
/// Same as [`rational_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_from_ratio(args: *const i64, argc: u32) -> i64 {
    let r = rational_single(args, argc, "rt_rational_from_ratio");
    if r.is_integer() {
        answer(r)
    } else {
        *args
    }
}

/// `rational->ratio`: always a `ratio` box (an integer one when the value is
/// an integer, which a `ratio` can hold).
///
/// # Safety
///
/// Same as [`rational_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_to_ratio(args: *const i64, argc: u32) -> i64 {
    let r = rational_single(args, argc, "rt_rational_to_ratio");
    encode(active_heap().alloc_ratio(r))
}

/// `rational->int`: truncates toward zero.
///
/// # Safety
///
/// Same as [`rational_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_to_int(args: *const i64, argc: u32) -> i64 {
    let r = rational_single(args, argc, "rt_rational_to_int");
    encode(active_heap().int_from_bigint(r.to_integer()))
}

/// Whether the `rational` is an integer: a raw `0`/`1`. A normalized
/// `rational` is one exactly when its word is not a ratio box.
///
/// # Safety
///
/// Same as [`rational_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_is_int(args: *const i64, argc: u32) -> i64 {
    let r = rational_single(args, argc, "rt_rational_is_int");
    i64::from(r.is_integer())
}

/// `rational->float`: the raw `f64` bits, rounded.
///
/// # Safety
///
/// Same as [`rational_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_to_float(args: *const i64, argc: u32) -> i64 {
    let r = rational_single(args, argc, "rt_rational_to_float");
    match r.to_f64() {
        Some(f) => f.to_bits() as i64,
        None => fatal("rt_rational_to_float: conversion failed"),
    }
}

/// `numerator`.
///
/// # Safety
///
/// Same as [`rational_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_numerator(args: *const i64, argc: u32) -> i64 {
    let r = rational_single(args, argc, "rt_rational_numerator");
    encode(active_heap().int_from_bigint(r.numer().clone()))
}

/// `denominator`.
///
/// # Safety
///
/// Same as [`rational_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_rational_denominator(args: *const i64, argc: u32) -> i64 {
    let r = rational_single(args, argc, "rt_rational_denominator");
    encode(active_heap().int_from_bigint(r.denom().clone()))
}
