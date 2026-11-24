//! Fixed-width integer arithmetic and `f64` math.

use typelisp_mem::prelude::*;
use typelisp_mem::fixed::Fault;
use typelisp_abi::{fatal, raise};

/// The failing side of a compiled fixed-width `/` or `rem`: the divisor
/// `args[1]` is zero, or the dividend `args[0]` is its type's most negative
/// value and the divisor `-1`. Compiled code tests for both and does the
/// division itself; it calls this only when one of them holds, and this
/// [`raise`]s the message `typelisp_mem::fixed::Fault` gives the interpreter.
///
/// No width is passed: the overflow happens only for a signed type, whose
/// words are sign-extended, so the two words print as the numbers they are.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_div_fault(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_int_div_fault: expected 2 arguments");
    }
    let (a, b) = (*args, *args.add(1));
    let fault = if b == 0 { Fault::DivideByZero } else { Fault::Overflow { dividend: i128::from(a), divisor: i128::from(b) } };
    raise(fault.message()).into_word()
}

/// The failing side of a compiled fixed-width `mod`: a zero divisor, the one
/// way it fails.
///
/// # Safety
///
/// No arguments are read.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_mod_fault(_args: *const i64, _argc: u32) -> i64 {
    raise(Fault::ModByZero.message()).into_word()
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

/// [`rt_f64_unary`] for `f32`: the word holds an `f32`'s value as a `double`
/// (exactly), and the function is computed in binary32.
///
/// # Safety
///
/// As [`rt_f64_unary`].
unsafe fn rt_f32_unary(args: *const i64, argc: u32, name: &str, f: fn(f32) -> f32) -> i64 {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", name));
    }
    f64::from(f(f64::from_bits(*args as u64) as f32)).to_bits() as i64
}

macro_rules! rt_f32_unary_exports {
    ($($export:ident => $method:ident,)*) => {$(
        #[no_mangle]
        pub unsafe extern "C" fn $export(args: *const i64, argc: u32) -> i64 {
            rt_f32_unary(args, argc, stringify!($export), <f32 as num_traits::Float>::$method)
        }
    )*};
}
rt_f32_unary_exports! {
    rt_f32_tan => tan,
    rt_f32_asin => asin,
    rt_f32_acos => acos,
    rt_f32_atan => atan,
    rt_f32_sinh => sinh,
    rt_f32_cosh => cosh,
    rt_f32_tanh => tanh,
    rt_f32_asinh => asinh,
    rt_f32_acosh => acosh,
    rt_f32_atanh => atanh,
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
