//! Fixed-width integer arithmetic and `f64` math.

use typelisp_mem::prelude::*;
use typelisp_abi::{fatal, raise};

/// The receiver type's width and signedness, as the extra raw operand every
/// integer shim below takes: `width * 2 + (signed as i64)`.
///
/// A type name means its width and its signedness and nothing else
/// (`types::int_width_signed`), so the six integer types share one shim per
/// operation and are told apart by this one number rather than by six copies
/// of each. The island builds it as a constant at the call site — the
/// receiver's type name is right there in `compile-assoc`.
pub(crate) fn wsig(code: i64) -> (u32, bool) {
    ((code >> 1) as u32, code & 1 == 1)
}

/// `v` cut back to `width` bits and re-extended into the 64-bit word compiled
/// code carries every integer in.
///
/// One statement of the invariant, in `typelisp-mem` (below both this crate
/// and the checker), rather than a copy here that could drift from it.
use typelisp_mem::normalize_int as normalize;

/// Integer division `a / b` at the receiver's width — the compiled-code half
/// of `/`, which (unlike `+`/`-`/`*`) can't be a bare LLVM instruction
/// because LLVM `sdiv` by zero is undefined behavior. Operands are raw
/// untagged `i64`s (the int branch of `compile-assoc` computes them the same
/// way `build-add` does), so there is no decode/encode here.
///
/// `sdiv` serves the unsigned types too: every width here is at most 32 and
/// a normalized unsigned value is a non-negative `i64`, so the signed
/// quotient *is* the unsigned one.
///
/// A zero divisor [`raise`]s `"divide by zero"` — the very message
/// `eval_int_builtin` gives the interpreted path, since the two paths running
/// the same form must fail the same way (`tests/runtime_error_parity_test.rs`
/// is the guard). The one other failure is the type's most negative value
/// over `-1`, whose quotient is one past its maximum; that is an overflow
/// rather than a divisor problem and says so. Unsigned types have no such
/// case.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_div(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_int_div: expected 3 arguments");
    }
    let (a, b) = (*args, *args.add(1));
    let (width, signed) = wsig(*args.add(2));
    if b == 0 {
        return raise("divide by zero".to_string()).into_word();
    }
    if signed && b == -1 && a == normalize(1i64 << (width - 1), width, true) {
        return raise(format!("arithmetic overflow: {} / {}", a, b)).into_word();
    }
    normalize(a / b, width, signed)
}

/// Floored remainder `a mod b` (CL `mod`) at the receiver's width — the
/// compiled-code half of `mod`, matching the interpreter's
/// `eval_int_builtin`. The result takes the sign of the divisor `b` (unlike
/// `srem`/`rem`, which take the sign of the dividend), so `-7 mod 3 = 2`.
/// Same zero-divisor handling as [`rt_int_div`] — a catchable
/// `"mod by zero"`, `eval_int_builtin`'s own wording; the most-negative-over-
/// `-1` case that overflows for `/` is mathematically `0` here, which
/// `wrapping_rem` answers directly. Raw-`i64` operand convention.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_mod(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_int_mod: expected 3 arguments");
    }
    let (a, b) = (*args, *args.add(1));
    let (width, signed) = wsig(*args.add(2));
    if b == 0 {
        return raise("mod by zero".to_string()).into_word();
    }
    let r = a.wrapping_rem(b);
    normalize(if r != 0 && (r < 0) != (b < 0) { r + b } else { r }, width, signed)
}

/// `(ash integer count)` at the receiver's width — the compiled-code half,
/// matching `eval_int_builtin`'s. Not a bare LLVM `shl`/`ashr` because both
/// are undefined behavior once the shift amount reaches the operand's bit
/// width; a count at or past the *type's* width is clamped here to its
/// limiting value (`0` left, and right either `0` or, for a signed negative
/// operand, the sign smeared to `-1`). `args[0]` is the integer, `args[1]`
/// the count (receiver-first, CL's own argument order).
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_ash(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_int_ash: expected 3 arguments");
    }
    let (n, count) = (*args, *args.add(1));
    let (width, signed) = wsig(*args.add(2));
    let w = i64::from(width);
    let r = if count >= 0 {
        if count >= w {
            0
        } else {
            n.wrapping_shl(count as u32)
        }
    } else if -count >= w {
        if signed && n < 0 {
            -1
        } else {
            0
        }
    } else {
        n >> (-count)
    };
    normalize(r, width, signed)
}

/// `(logbitp index integer)` at the receiver's width — the compiled-code
/// half, matching `eval_int_builtin`'s. Not a bare `lshr`+`and` because a
/// variable shift by an index at or past the width is undefined behavior in
/// LLVM; that case is handled directly (past the width there is no bit to
/// read — every one is the sign for a signed value, zero for an unsigned
/// one). `args[0]` is the index (receiver-first), `args[1]` the integer.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_logbitp(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_int_logbitp: expected 3 arguments");
    }
    // Integer first, position second — the order `registry::int_assoc`
    // registers and `eval_int_builtin` reads. The island passes the two
    // operands through unchanged, so this shim is where the order lives.
    let (n, index) = (*args, *args.add(1));
    let (width, signed) = wsig(*args.add(2));
    if index >= i64::from(width) {
        (signed && n < 0) as i64
    } else {
        (n >> index) & 1
    }
}

/// `logcount` at the receiver's width (population count of a nonnegative
/// integer, or of the 0-bits of a negative one — CL's own "infinite two's
/// complement" reading, cut to the type's own bits) — the compiled-code
/// half, matching `eval_int_builtin`'s. Not a bare `llvm.ctpop.i64` call
/// because the negative case first needs a conditional `lognot` and a mask,
/// cheaper to express directly here than as extra IR at every call site.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_logcount(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_int_logcount: expected 2 arguments");
    }
    let n = *args;
    let (width, _) = wsig(*args.add(1));
    let mask = if width >= 64 { !0u64 } else { (1u64 << width) - 1 };
    let bits = (n as u64) & mask;
    i64::from(if n >= 0 { bits.count_ones() } else { (!bits & mask).count_ones() })
}

/// `integer-length` at the receiver's width (bits needed, excluding sign) —
/// the compiled-code half, matching `eval_int_builtin`'s: `n`'s own bit
/// length when nonnegative, else `!n`'s (CL's negative-integer-length
/// identity), both read inside the type's own bits.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_integer_length(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_int_integer_length: expected 2 arguments");
    }
    let n = *args;
    let (width, _) = wsig(*args.add(1));
    let v = if n >= 0 { n as u64 } else { !(n as u64) };
    let mask = if width >= 64 { !0u64 } else { (1u64 << width) - 1 };
    i64::from(64 - (v & mask).leading_zeros())
}

/// `try-int->i8`/.../`try-int->u32`'s question, for compiled code — does
/// `args[0]` already hold a number the target width and signedness can
/// represent? `args[1]` is that target's `wsig` code.
///
/// The *answer* is all a compiled `try-int->W` needs from a shim: when it is
/// yes the value is already the result (an integer that fits its target is
/// unchanged by the cast — `interp::try_int_to_width`), so the island builds
/// the `Some` box around the operand it already has rather than calling back
/// for a converted one.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_fits(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_int_fits: expected 2 arguments");
    }
    let n = *args;
    let (width, signed) = wsig(*args.add(1));
    (normalize(n, width, signed) == n) as i64
}

/// `try-int->char`'s question — is `args[0]` a Unicode scalar value?
///
/// Not merely "in `u32` range": the surrogate range `D800..=DFFF` is `u32` and
/// is not a `char`, which is exactly the case `int->char` raises on and this
/// answers `0` for (`interp::try_int_to_char`).
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_int_fits_char(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_int_fits_char: expected 1 argument");
    }
    let n = *args;
    let in_u32_range = n >= 0 && n <= i64::from(u32::MAX);
    (in_u32_range && char::from_u32(n as u32).is_some()) as i64
}

/// `try-float->f32`'s question — does `args[0]` survive the round trip
/// through binary32 unchanged?
///
/// Bit equality rather than `==` so the answer matches
/// `interp::try_float_to_f32` on the values `==` is silent about: a `NaN` is
/// never `==` itself but does round-trip, and `-0.0 == 0.0` while their bits
/// differ.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64` holding an
/// `f64`'s bit pattern.
#[no_mangle]
pub unsafe extern "C" fn rt_f64_fits_f32(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f64_fits_f32: expected 1 argument");
    }
    let v = f64::from_bits(*args as u64);
    (f64::from(v as f32).to_bits() == v.to_bits()) as i64
}

/// Shared shape for the `f64` transcendental unaries that have no LLVM
/// intrinsic in the LLVM version this project pins (`sin`/`cos`/`exp`/`log`
/// do and go straight to LLVM IR in `compiler.rs` instead — see
/// `llvm_builder_build_float_unary_intrinsic`): decode the raw bit-pattern
/// argument, apply `f`, re-encode. `argc`/pointer validity is the caller's
/// (`compiler.rs`'s `alloca-args`/`store-arg`) responsibility, same as every
/// other `rt_*` shim here.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// (an `f64`'s raw bit pattern).
unsafe fn rt_f64_unary(args: *const i64, argc: u32, name: &str, f: fn(f64) -> f64) -> i64 {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", name));
    }
    f(f64::from_bits(*args as u64)).to_bits() as i64
}

macro_rules! rt_f64_unary_exports {
    ($($export:ident => $method:ident,)*) => {$(
        #[no_mangle]
        pub unsafe extern "C" fn $export(args: *const i64, argc: u32) -> i64 {
            rt_f64_unary(args, argc, stringify!($export), <f64 as num_traits::Float>::$method)
        }
    )*};
}
rt_f64_unary_exports! {
    rt_f64_tan => tan,
    rt_f64_asin => asin,
    rt_f64_acos => acos,
    rt_f64_atan => atan,
    rt_f64_sinh => sinh,
    rt_f64_cosh => cosh,
    rt_f64_tanh => tanh,
    rt_f64_asinh => asinh,
    rt_f64_acosh => acosh,
    rt_f64_atanh => atanh,
}
