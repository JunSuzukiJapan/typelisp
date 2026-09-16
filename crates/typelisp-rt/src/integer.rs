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

use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};
use typelisp_abi::{active_heap, fatal, raise};
use typelisp_mem::{decode, encode, fixnum_fits, normalize_int, Value};

/// One `int` operand, cloned out of its box when it is one (owned, for the
/// reason the interpreter's `expect_bignum` gives: every caller allocates
/// next, and a `&BigInt` into the heap cannot survive that).
enum IntArg {
    Fix(i64),
    Big(BigInt),
}

impl IntArg {
    fn into_big(self) -> BigInt {
        match self {
            IntArg::Fix(n) => BigInt::from(n),
            IntArg::Big(n) => n,
        }
    }
}

/// # Safety
///
/// `args` must be valid for at least `idx + 1` `i64`s; a `Heap` must be
/// registered on this thread.
unsafe fn integer_arg(args: *const i64, idx: isize, who: &str) -> IntArg {
    match decode(*args.offset(idx)) {
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

/// # Safety
///
/// Same as [`integer_arg`], for `args[0]` and `args[1]`.
unsafe fn integer_pair(args: *const i64, argc: u32, who: &str) -> (IntArg, IntArg) {
    if argc < 2 {
        fatal(&format!("{who}: expected 2 arguments"));
    }
    (integer_arg(args, 0, who), integer_arg(args, 1, who))
}

/// # Safety
///
/// Same as [`integer_arg`], for `args[0]`.
unsafe fn integer_single(args: *const i64, argc: u32, who: &str) -> IntArg {
    if argc < 1 {
        fatal(&format!("{who}: expected 1 argument"));
    }
    integer_arg(args, 0, who)
}

/// An `int` result, canonical.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
unsafe fn int_result(n: BigInt) -> i64 {
    encode(active_heap().int_from_bigint(n))
}

/// # Safety
///
/// A `Heap` must be registered on this thread.
unsafe fn wide_result(n: i128) -> i64 {
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
        pub unsafe extern "C" fn $name(args: *const i64, argc: u32) -> i64 {
            match integer_pair(args, argc, $who) {
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

/// `int::/` — truncating (CL `truncate`), like every integer `/` here. A
/// zero divisor [`raise`]s the interpreter's own `"divide by zero"`; the
/// most-negative-over-`-1` quotient the fixed widths cannot hold simply
/// promotes.
///
/// # Safety
///
/// Same as [`integer_pair`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_integer_div(args: *const i64, argc: u32) -> i64 {
    match integer_pair(args, argc, "rt_integer_div") {
        (IntArg::Fix(a), IntArg::Fix(b)) => {
            if b == 0 {
                raise("divide by zero".to_string());
            }
            wide_result(i128::from(a) / i128::from(b))
        }
        (a, b) => {
            let (a, b) = (a.into_big(), b.into_big());
            if b.is_zero() {
                raise("divide by zero".to_string());
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
pub unsafe extern "C-unwind" fn rt_integer_mod(args: *const i64, argc: u32) -> i64 {
    match integer_pair(args, argc, "rt_integer_mod") {
        (IntArg::Fix(a), IntArg::Fix(b)) => {
            if b == 0 {
                raise("mod by zero".to_string());
            }
            let (a, b) = (i128::from(a), i128::from(b));
            let r = a % b;
            wide_result(if r != 0 && ((r < 0) != (b < 0)) { r + b } else { r })
        }
        (a, b) => {
            let (a, b) = (a.into_big(), b.into_big());
            if b.is_zero() {
                raise("mod by zero".to_string());
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
pub unsafe extern "C" fn rt_integer_cmp(args: *const i64, argc: u32) -> i64 {
    let (a, b) = integer_pair(args, argc, "rt_integer_cmp");
    let ord = match (a, b) {
        (IntArg::Fix(a), IntArg::Fix(b)) => a.cmp(&b),
        (a, b) => a.into_big().cmp(&b.into_big()),
    };
    ord as i64
}

/// `int::logtest` — raw `0`/`1`.
///
/// # Safety
///
/// Same as [`integer_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_logtest(args: *const i64, argc: u32) -> i64 {
    match integer_pair(args, argc, "rt_integer_logtest") {
        (IntArg::Fix(a), IntArg::Fix(b)) => (a & b != 0) as i64,
        (a, b) => (!(a.into_big() & b.into_big()).is_zero()) as i64,
    }
}

/// `int::ash` — `args[1]` is a raw bit count, like `rt_bignum_ash`'s.
///
/// # Safety
///
/// `argc >= 2`; `args[0]` as [`integer_arg`], `args[1]` a raw `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_integer_ash(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_integer_ash: expected 2 arguments");
    }
    let n = integer_arg(args, 0, "rt_integer_ash").into_big();
    let count = *args.add(1);
    int_result(if count >= 0 { n << (count as u64) } else { n >> ((-count) as u64) })
}

/// `int::logbitp` — raw `0`/`1`; `args[1]` is a raw bit position. A negative
/// position reads the sign, the "infinite two's complement" answer.
///
/// # Safety
///
/// Same as [`rt_integer_ash`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_logbitp(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_integer_logbitp: expected 2 arguments");
    }
    let n = integer_arg(args, 0, "rt_integer_logbitp").into_big();
    let pos = *args.add(1);
    let bit = if pos < 0 { n.sign() == Sign::Minus } else { ((n >> (pos as u64)) & BigInt::from(1)) == BigInt::from(1) };
    bit as i64
}

/// `int::lognot`.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_lognot(args: *const i64, argc: u32) -> i64 {
    match integer_single(args, argc, "rt_integer_lognot") {
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
pub unsafe extern "C" fn rt_integer_logcount(args: *const i64, argc: u32) -> i64 {
    let a = integer_single(args, argc, "rt_integer_logcount").into_big();
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
pub unsafe extern "C" fn rt_integer_integer_length(args: *const i64, argc: u32) -> i64 {
    let a = integer_single(args, argc, "rt_integer_integer_length").into_big();
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
pub unsafe extern "C" fn rt_integer_to_float(args: *const i64, argc: u32) -> i64 {
    let f = match integer_single(args, argc, "rt_integer_to_float") {
        IntArg::Fix(n) => n as f64,
        IntArg::Big(n) => n.to_f64().unwrap_or(f64::INFINITY.copysign(if n.sign() == Sign::Minus { -1.0 } else { 1.0 })),
    };
    f.to_bits() as i64
}

/// `int->ratio` — exact.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_to_ratio(args: *const i64, argc: u32) -> i64 {
    let n = integer_single(args, argc, "rt_integer_to_ratio").into_big();
    encode(active_heap().alloc_ratio(BigRational::from_integer(n)))
}

/// `int->char` — the raw scalar value, or a [`raise`] for a value that is
/// not one (`rt_int_to_char`'s wording).
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_integer_to_char(args: *const i64, argc: u32) -> i64 {
    let a = integer_single(args, argc, "rt_integer_to_char");
    match scalar_of(&a) {
        Some(c) => i64::from(c),
        None => raise(format!("int->char: {} is not a valid Unicode scalar value", a.into_big())),
    }
}

/// `try-int->char`'s question — raw `0`/`1`.
///
/// # Safety
///
/// Same as [`integer_single`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_fits_char(args: *const i64, argc: u32) -> i64 {
    scalar_of(&integer_single(args, argc, "rt_integer_fits_char")).is_some() as i64
}

fn scalar_of(a: &IntArg) -> Option<u32> {
    match a {
        IntArg::Fix(n) if *n >= 0 && *n <= i64::from(u32::MAX) => char::from_u32(*n as u32).map(u32::from),
        _ => None,
    }
}

/// The low 64 bits of an `int`, two's complement — what a width cast out of
/// it starts from.
fn low_word(a: IntArg) -> i64 {
    match a {
        IntArg::Fix(n) => n,
        IntArg::Big(n) => (n & BigInt::from(u64::MAX)).to_u64().expect("masked to 64 bits") as i64,
    }
}

/// `int->W` — the raw normalized word of width `wsig` (`args[1]`, the
/// `width * 2 + signed` code every `rt_int_*` shim takes): a cast out of an
/// `int` truncates, as every width cast does.
///
/// # Safety
///
/// `argc >= 2`; `args[0]` as [`integer_arg`], `args[1]` a raw `wsig`.
#[no_mangle]
pub unsafe extern "C" fn rt_integer_narrow(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_integer_narrow: expected 2 arguments");
    }
    let a = integer_arg(args, 0, "rt_integer_narrow");
    let (width, signed) = crate::wsig(*args.add(1));
    normalize_int(low_word(a), width, signed)
}

/// `try-int->W`'s question — raw `0`/`1`: a fixnum fits when normalizing
/// leaves it alone; a bignum fits a 64-bit C word when it is in that word's
/// range, and never anything narrower.
///
/// # Safety
///
/// Same as [`rt_integer_narrow`].
#[no_mangle]
pub unsafe extern "C" fn rt_integer_fits(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_integer_fits: expected 2 arguments");
    }
    let a = integer_arg(args, 0, "rt_integer_fits");
    let (width, signed) = crate::wsig(*args.add(1));
    let fits = match &a {
        IntArg::Fix(n) => normalize_int(*n, width, signed) == *n,
        IntArg::Big(n) => width == 64 && if signed { n.to_i64().is_some() } else { n.to_u64().is_some() },
    };
    fits as i64
}

/// `int->int` on a fixed-width receiver (`(as int x)`): the raw word
/// `args[0]` read at width `wsig` (`args[1]`), made canonical. `c-ulong`
/// (wsig 128) is the one receiver whose 64 bits are unsigned; every
/// narrower unsigned type is already non-negative in the register.
///
/// # Safety
///
/// `argc >= 2`; `args[0]` a raw word, `args[1]` a raw `wsig`; a `Heap`
/// must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_integer_from_word(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_integer_from_word: expected 2 arguments");
    }
    let word = *args;
    let (width, signed) = crate::wsig(*args.add(1));
    wide_result(if !signed && width == 64 { i128::from(word as u64) } else { i128::from(word) })
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
pub unsafe extern "C-unwind" fn rt_int_not_fixnum(args: *const i64, argc: u32) -> i64 {
    let a = integer_single(args, argc, "rt_int_not_fixnum");
    raise(format!("an integer argument does not fit a fixnum: {}", a.into_big()))
}
