//! `int`'s shims — CL's `integer` for compiled code: a 63-bit fixnum that
//! promotes to a bignum box when a result outgrows it and demotes back when
//! it fits.
//!
//! Every function here takes tagged words and answers one (or a raw `0`/`1`
//! for a predicate, a raw `-1`/`0`/`1` for `cmp`, a raw word for a width
//! cast — the non-`int` results the island reads directly). An `int`'s
//! word is a fixnum or a bignum box and nothing else; [`integer_arg`]
//! aborts on anything else, and on a box holding a fixnum-range value,
//! because that is a producer that skipped `Heap::canonical_int` and the
//! damage is best reported where it is first seen.
//!
//! The island calls these only off its fast path: two fixnums add, subtract,
//! multiply and compare as overflow-checked instructions on the words
//! themselves (`compile-assoc`'s `int` branch), and come here when a check
//! fails or an operand is a box. So the common case never enters Rust, and
//! the shims are written for correctness over the whole domain rather than
//! for the fixnum case in particular.

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};
use typelisp_abi::{active_heap, fatal, raise};
use typelisp_mem::fixed;
use typelisp_mem::{decode, encode, fixnum_fits, IntArg, Value};

/// # Safety
///
/// A `Heap` must be registered on this thread.
unsafe fn integer_arg(word: Word, who: &str) -> IntArg {
    match decode(word) {
        Value::Int(n) if fixnum_fits(n) => IntArg::Fix(n),
        Value::Int(n) => fatal(&format!("{who}: an int fixnum out of range: {n}")),
        Value::Boxed(id) => {
            let heap = active_heap();
            if !heap.is_bignum(id) {
                fatal(&format!("{who}: argument is a box but not a bignum"));
            }
            if heap.bignum_fits_fixnum(id) {
                fatal(&format!("{who}: an int boxed as a bignum that fits a fixnum: {}", heap.bignum_value(id)));
            }
            IntArg::Big(heap.bignum_value(id).clone())
        }
        _ => fatal(&format!("{who}: argument is not an int")),
    }
}

/// An `int` result, canonical.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
unsafe fn int_result(n: BigInt) -> Word {
    encode(active_heap().int_from_bigint(n))
}

/// # Safety
///
/// A `Heap` must be registered on this thread.
unsafe fn wide_result(n: i128) -> Word {
    encode(active_heap().canonical_int(n))
}

macro_rules! binop {
    ($(#[$doc:meta])* $name:ident, $who:literal, |$a:ident, $b:ident| $fix:expr, |$x:ident, $y:ident| $big:expr) => {
        $(#[$doc])*
        ///
        /// # Safety
        ///
        /// Same as [`integer_pair`].
        #[no_mangle]
        pub unsafe extern "C" fn $name(a: Word, b: Word) -> Word {
            match (integer_arg(a, $who), integer_arg(b, $who)) {
                (IntArg::Fix($a), IntArg::Fix($b)) => {
                    let ($a, $b) = (i128::from($a), i128::from($b));
                    wide_result($fix)
                }
                (a, b) => {
                    let ($x, $y) = (a.into_big(), b.into_big());
                    int_result($big)
                }
            }
        }
    };
}

binop!(
    /// `int::+` — the slow path of the island's overflow-checked `add`.
    rt_integer_add, "rt_integer_add", |a, b| a + b, |x, y| x + y
);
binop!(
    /// `int::-`.
    rt_integer_sub, "rt_integer_sub", |a, b| a - b, |x, y| x - y
);
binop!(
    /// `int::*`.
    rt_integer_mul, "rt_integer_mul", |a, b| a * b, |x, y| x * y
);
binop!(
    /// `int::logand`.
    rt_integer_logand, "rt_integer_logand", |a, b| a & b, |x, y| x & y
);
binop!(
    /// `int::logior`.
    rt_integer_logior, "rt_integer_logior", |a, b| a | b, |x, y| x | y
);
binop!(
    /// `int::logxor`.
    rt_integer_logxor, "rt_integer_logxor", |a, b| a ^ b, |x, y| x ^ y
);

/// `int::truncate` — the integer quotient, truncating (CL `truncate`). `int`'s
/// `/` is CL's rational one (`rational::rt_rational_div`). A
/// zero divisor [`raise`]s the interpreter's own `"divide by zero"`; the
/// most-negative-over-`-1` quotient the fixed widths cannot hold simply
/// promotes.
///
/// # Safety
///
/// Same as [`integer_pair`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_integer_div(a: Word, b: Word) -> Word {
    match (integer_arg(a, "rt_integer_div"), integer_arg(b, "rt_integer_div")) {
        (IntArg::Fix(a), IntArg::Fix(b)) => {
            if b == 0 {
                return raise("divide by zero".to_string()).into_word();
            }
            wide_result(i128::from(a) / i128::from(b))
        }
        (a, b) => {
            let (a, b) = (a.into_big(), b.into_big());
            if b.is_zero() {
                return raise("divide by zero".to_string()).into_word();
            }
            int_result(a / b)
        }
    }
}

/// `int::mod` — floored (CL `mod`, the sign of the divisor). A zero divisor
/// [`raise`]s `"mod by zero"`.
///
/// # Safety
///
/// Same as [`integer_pair`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_integer_mod(a: Word, b: Word) -> Word {
    match (integer_arg(a, "rt_integer_mod"), integer_arg(b, "rt_integer_mod")) {
        (IntArg::Fix(a), IntArg::Fix(b)) => {
            if b == 0 {
                return raise("mod by zero".to_string()).into_word();
            }
            let (a, b) = (i128::from(a), i128::from(b));
            let r = a % b;
            wide_result(if r != 0 && ((r < 0) != (b < 0)) { r + b } else { r })
        }
        (a, b) => {
            let (a, b) = (a.into_big(), b.into_big());
            if b.is_zero() {
                return raise("mod by zero".to_string()).into_word();
            }
            int_result(a.mod_floor(&b))
        }
    }
}

/// Three-way comparison, `-1`/`0`/`1` raw — every `int` comparison the
/// island cannot settle on two fixnum words derives from it, the shape
/// `rt_bignum_cmp` established.
///
/// # Safety
///
/// Same as [`integer_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_cmp(a: Word, b: Word) -> Word {
    let (a, b) = (integer_arg(a, "rt_integer_cmp"), integer_arg(b, "rt_integer_cmp"));
    let ord = match (a, b) {
        (IntArg::Fix(a), IntArg::Fix(b)) => a.cmp(&b),
        (a, b) => a.into_big().cmp(&b.into_big()),
    };
    ord as Word
}

/// `int::logtest` — raw `0`/`1`.
///
/// # Safety
///
/// Same as [`integer_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_logtest(a: Word, b: Word) -> bool {
    match (integer_arg(a, "rt_integer_logtest"), integer_arg(b, "rt_integer_logtest")) {
        (IntArg::Fix(a), IntArg::Fix(b)) => a & b != 0,
        (a, b) => !(a.into_big() & b.into_big()).is_zero(),
    }
}

/// `int::ash` — `count` is a raw bit count, like `rt_bignum_ash`'s.
///
/// # Safety
///
/// `argc >= 2`; `n` as [`integer_arg`], `count` a raw `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_integer_ash(n: Word, count: Word) -> Word {
    let n = integer_arg(n, "rt_integer_ash").into_big();
    int_result(if count >= 0 { n << (count as u64) } else { n >> ((-count) as u64) })
}

/// `int::logbitp` — raw `0`/`1`; `pos` is a raw bit position. A negative
/// position reads the sign, the "infinite two's complement" answer.
///
/// # Safety
///
/// Same as [`rt_integer_ash`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_logbitp(n: Word, pos: Word) -> bool {
    let n = integer_arg(n, "rt_integer_logbitp").into_big();
    if pos < 0 { n.sign() == Sign::Minus } else { ((n >> (pos as u64)) & BigInt::from(1)) == BigInt::from(1) }
}

/// `int::lognot`.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_lognot(x: Word) -> Word {
    match integer_arg(x, "rt_integer_lognot") {
        IntArg::Fix(n) => wide_result(i128::from(!n)),
        IntArg::Big(n) => int_result(!n),
    }
}

/// `int::logcount` — CL's count of 1-bits for a nonnegative value and of
/// 0-bits for a negative one (`popcount(n) = popcount(!n)` when `n < 0`).
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_logcount(x: Word) -> Word {
    let a = integer_arg(x, "rt_integer_logcount").into_big();
    let n = if a.sign() == Sign::Minus { !&a } else { a };
    let count: u64 = n.magnitude().to_u32_digits().iter().map(|d| u64::from(d.count_ones())).sum();
    wide_result(i128::from(count))
}

/// `int::integer-length` — bits needed excluding the sign.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_integer_length(x: Word) -> Word {
    let a = integer_arg(x, "rt_integer_integer_length").into_big();
    let bits = if a.sign() == Sign::Minus { (-(&a) - 1u32).magnitude().bits() } else { a.magnitude().bits() };
    wide_result(i128::from(bits))
}

/// `int->float` — the raw `f64` bit pattern a compiled float register holds;
/// a magnitude past `f64` saturates to infinity, as `bignum->float` does.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_to_float(x: Word) -> f64 {
    match integer_arg(x, "rt_integer_to_float") {
        IntArg::Fix(n) => n as f64,
        IntArg::Big(n) => n.to_f64().unwrap_or(f64::INFINITY.copysign(if n.sign() == Sign::Minus { -1.0 } else { 1.0 })),
    }
}

/// `int->ratio` — exact.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_to_ratio(x: Word) -> Word {
    let n = integer_arg(x, "rt_integer_to_ratio").into_big();
    encode(active_heap().alloc_ratio(BigRational::from_integer(n)))
}

/// `int->char` — the raw scalar value, or a [`raise`] for a value that is
/// not one (`rt_int_to_char`'s wording).
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_integer_to_char(x: Word) -> u32 {
    let a = integer_arg(x, "rt_integer_to_char");
    match scalar_of(&a) {
        Some(c) => c,
        None => raise(format!("int->char: {} is not a valid Unicode scalar value", a.into_big())).into_zero(),
    }
}

/// `try-int->char`'s question — raw `0`/`1`.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_fits_char(x: Word) -> bool {
    scalar_of(&integer_arg(x, "rt_integer_fits_char")).is_some()
}

fn scalar_of(a: &IntArg) -> Option<u32> {
    match a {
        IntArg::Fix(n) if *n >= 0 && *n <= i64::from(u32::MAX) => char::from_u32(*n as u32).map(u32::from),
        _ => None,
    }
}

/// `(largest-fixnum)`: the largest `int` a word of this machine holds, as a
/// raw word.
///
/// # Safety
///
/// Takes no arguments.
#[no_mangle]
pub unsafe extern "C" fn rt_largest_fixnum() -> Word {
    typelisp_mem::FIXNUM_MAX as Word
}

macro_rules! integer_to_fixed {
    ($($t:ident: $to:ident;)*) => {$(
        /// `int->W` for this `W`: `typelisp_mem::fixed::from_int`, the
        /// value's low bits as the type. A C integer's are 64 bits whatever
        /// its width is: the caller cuts them to it.
        ///
        /// # Safety
        ///
        /// `argc >= 1`; `x` as [`integer_arg`].
        #[no_mangle]
        pub unsafe extern "C" fn $to(x: Word) -> $t {
            fixed::from_int::<$t>(integer_arg(x, stringify!($to)))
        }
    )*};
}
integer_to_fixed! {
    i8: rt_integer_to_i8;
    i16: rt_integer_to_i16;
    i32: rt_integer_to_i32;
    i64: rt_integer_to_c_long;
    u8: rt_integer_to_u8;
    u16: rt_integer_to_u16;
    u32: rt_integer_to_u32;
    u64: rt_integer_to_c_ulong;
}

macro_rules! integer_fits_fixed {
    ($($t:ident: $fits:ident;)*) => {$(
        /// `try-int->W`'s question for this `W` — raw `0`/`1`. When it is
        /// yes, the value is the one `int->W` gives.
        ///
        /// # Safety
        ///
        /// `argc >= 1`; `x` as [`integer_arg`].
        #[no_mangle]
        pub unsafe extern "C" fn $fits(x: Word) -> bool {
            fixed::try_from_int::<$t>(&integer_arg(x, stringify!($fits))).is_some()
        }
    )*};
}
integer_fits_fixed! {
    i8: rt_integer_fits_i8;
    i16: rt_integer_fits_i16;
    i32: rt_integer_fits_i32;
    u8: rt_integer_fits_u8;
    u16: rt_integer_fits_u16;
    u32: rt_integer_fits_u32;
}

/// `int->int` on a signed fixed-width receiver or one narrower than 64 bits
/// (`(as int x)`): the raw word `x`, which is the number, made
/// canonical.
///
/// # Safety
///
/// `argc >= 1`; `x` a raw word; a `Heap` must be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C" fn rt_integer_from_word(x: i64) -> Word {
    wide_result(fixed::to_int(x))
}

/// `int->int` on a `c-ulong`: the one receiver whose top bit is not a sign.
///
/// # Safety
///
/// Same as [`rt_integer_from_word`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_from_c_ulong(x: u64) -> Word {
    wide_result(fixed::to_int(x))
}

/// `(untag-int E)`'s refusal: an `int` in a machine-word position (an index,
/// a count) that is a bignum. [`raise`]s the same language error the
/// interpreter's `untag-int` does; the island calls this only off the
/// fixnum fast path, so the argument is never a fixnum here.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_not_fixnum(text: Word) -> Word {
    // Written through `bignum_text`, so that a program that makes no bignum
    // (and calls this only on a path it never takes) does not carry the
    // arbitrary-precision arithmetic for the message.
    let text = match decode(text) {
        Value::Boxed(id) if active_heap().is_bignum(id) => active_heap().bignum_text(id),
        _ => fatal("rt_int_not_fixnum: the argument is not a bignum"),
    };
    raise(format!("an integer argument does not fit a fixnum: {}", text)).into_word()
}
