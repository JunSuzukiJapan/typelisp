//! Fixed-width integer arithmetic and `f64` math.

use typelisp_mem::Word;
use typelisp_mem::fixed::Fault;
use typelisp_abi::raise;

/// The failing side of a compiled fixed-width `/` or `rem`: the divisor
/// `b` is zero, or the dividend `a` is its type's most negative
/// value and the divisor `-1`. Compiled code tests for both and does the
/// division itself; it calls this only when one of them holds, and this
/// [`raise`]s the message `typelisp_mem::fixed::Fault` gives the interpreter.
///
/// No width is passed: the overflow happens only for a signed type, whose
/// words are sign-extended, so the two words print as the numbers they are.
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_div_fault(a: i64, b: i64) -> Word {
    let fault = if b == 0 { Fault::DivideByZero } else { Fault::Overflow { dividend: i128::from(a), divisor: i128::from(b) } };
    raise(fault.message()).into_word()
}

/// The failing side of a compiled fixed-width `mod`: a zero divisor, the one
/// way it fails.
///
/// # Safety
///
/// Takes the operands as [`rt_int_div_fault`] does, and reads neither: the
/// divisor is zero.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_mod_fault(_dividend: i64, _divisor: i64) -> Word {
    raise(Fault::ModByZero.message()).into_word()
}

/// `try-int->char`'s question — is `n` a Unicode scalar value?
///
/// Not merely "in `u32` range": the surrogate range `D800..=DFFF` is `u32` and
/// is not a `char`, which is exactly the case `int->char` raises on and this
/// answers `0` for (`interp::try_int_to_char`).
///
/// # Safety
///
/// The words must be valid.
#[no_mangle]
pub unsafe extern "C" fn rt_int_fits_char(n: i64) -> bool {
    u32::try_from(n).is_ok_and(|n| char::from_u32(n).is_some())
}

/// `try-float->f32`'s question — does `v` survive the round trip
/// through binary32 unchanged?
///
/// Bit equality rather than `==` so the answer matches
/// `interp::try_float_to_f32` on the values `==` is silent about: a `NaN` is
/// never `==` itself but does round-trip, and `-0.0 == 0.0` while their bits
/// differ.
///
/// # Safety
///
/// None: every `f64` is an answerable argument. `unsafe` as every runtime
/// export is, for the one function type `externs.rs` casts them to.
#[no_mangle]
pub unsafe extern "C" fn rt_f64_fits_f32(v: f64) -> bool {
    f64::from(v as f32).to_bits() == v.to_bits()
}

/// The `f64` transcendental unaries that have no LLVM intrinsic in the LLVM
/// version this project pins (`sin`/`cos`/`exp`/`log` do and go straight to
/// LLVM IR in `compiler.rs` instead — see
/// `llvm_builder_build_float_unary_intrinsic`).
macro_rules! rt_f64_unary_exports {
    ($($export:ident => $method:ident,)*) => {$(
        /// # Safety
        ///
        /// None: every `f64` is an answerable argument. `unsafe` as every
        /// runtime export is, for the one function type `externs.rs` casts
        /// them to.
        #[no_mangle]
        pub unsafe extern "C" fn $export(x: f64) -> f64 {
            <f64 as num_traits::Float>::$method(x)
        }
    )*};
}

/// Their `f32` counterparts, computed in binary32.
macro_rules! rt_f32_unary_exports {
    ($($export:ident => $method:ident,)*) => {$(
        /// # Safety
        ///
        /// None: every `f32` is an answerable argument. `unsafe` as every
        /// runtime export is, for the one function type `externs.rs` casts
        /// them to.
        #[no_mangle]
        pub unsafe extern "C" fn $export(x: f32) -> f32 {
            <f32 as num_traits::Float>::$method(x)
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
