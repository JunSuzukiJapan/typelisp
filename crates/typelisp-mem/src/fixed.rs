//! What every operation on a fixed-width integer means, written once.
//!
//! The fixed-width types (`i8` `i16` `i32` `u8` `u16` `u32`, and the C words
//! `c-long` `c-ulong`) are the Rust integer types of the same width and sign,
//! and [`Fixed`] is implemented on those Rust types themselves. The
//! operations below are generic over it, so each one is written once and
//! instantiated per width: the interpreter picks the instance from the
//! receiver's type ([`FixedType`], [`with_fixed!`]), constant folding runs the
//! interpreter, and the runtime's per-width shims are instances too. A
//! program that uses one width links one width's instances.
//!
//! The value of a fixed-width integer travels in a 64-bit word, sign-extended
//! from its width when signed and zero-extended when unsigned
//! ([`Fixed::to_word`]); a `c-ulong` is the word's 64 bits read unsigned.

use crate::prelude::*;
use crate::IntArg;
use core::fmt::Display;
use num_traits::ToPrimitive;

/// A fixed-width integer type, as the Rust type that is it.
pub trait Fixed: Copy + Eq + Ord + Display + 'static {
    const BITS: u32;
    const SIGNED: bool;
    const MIN: Self;
    const ZERO: Self;
    /// The low [`Fixed::BITS`] bits of `w`: a width cast truncates.
    fn from_word(w: i64) -> Self;
    /// The value in the 64-bit word it travels in.
    fn to_word(self) -> i64;
    fn to_i128(self) -> i128;
    /// `n` when this type can represent it.
    fn from_i128(n: i128) -> Option<Self>;
    fn to_f64(self) -> f64;
    fn wrapping_add(self, o: Self) -> Self;
    fn wrapping_sub(self, o: Self) -> Self;
    fn wrapping_mul(self, o: Self) -> Self;
    /// `None` for a zero divisor and for the one quotient that overflows.
    fn checked_div(self, o: Self) -> Option<Self>;
    fn wrapping_rem(self, o: Self) -> Self;
    fn not(self) -> Self;
    fn and(self, o: Self) -> Self;
    fn or(self, o: Self) -> Self;
    fn xor(self, o: Self) -> Self;
    /// Left shift by `n < BITS`.
    fn shl(self, n: u32) -> Self;
    /// Right shift by `n < BITS`: arithmetic for a signed type, logical for
    /// an unsigned one.
    fn shr(self, n: u32) -> Self;
    fn count_ones(self) -> u32;
    fn leading_zeros(self) -> u32;
    /// A small count (a bit count, at most 64) as this type.
    fn of_count(n: u32) -> Self;
}

macro_rules! fixed_impl {
    ($($t:ty => $signed:expr,)*) => {$(
        impl Fixed for $t {
            const BITS: u32 = <$t>::BITS;
            const SIGNED: bool = $signed;
            const MIN: Self = <$t>::MIN;
            const ZERO: Self = 0;
            fn from_word(w: i64) -> Self { w as $t }
            fn to_word(self) -> i64 { self as i64 }
            fn to_i128(self) -> i128 { self as i128 }
            fn from_i128(n: i128) -> Option<Self> { <$t>::try_from(n).ok() }
            fn to_f64(self) -> f64 { self as f64 }
            fn wrapping_add(self, o: Self) -> Self { <$t>::wrapping_add(self, o) }
            fn wrapping_sub(self, o: Self) -> Self { <$t>::wrapping_sub(self, o) }
            fn wrapping_mul(self, o: Self) -> Self { <$t>::wrapping_mul(self, o) }
            fn checked_div(self, o: Self) -> Option<Self> { <$t>::checked_div(self, o) }
            fn wrapping_rem(self, o: Self) -> Self { <$t>::wrapping_rem(self, o) }
            fn not(self) -> Self { !self }
            fn and(self, o: Self) -> Self { self & o }
            fn or(self, o: Self) -> Self { self | o }
            fn xor(self, o: Self) -> Self { self ^ o }
            fn shl(self, n: u32) -> Self { self << n }
            fn shr(self, n: u32) -> Self { self >> n }
            fn count_ones(self) -> u32 { <$t>::count_ones(self) }
            fn leading_zeros(self) -> u32 { <$t>::leading_zeros(self) }
            fn of_count(n: u32) -> Self { n as $t }
        }
    )*};
}
fixed_impl! {
    i8 => true, i16 => true, i32 => true, i64 => true,
    u8 => false, u16 => false, u32 => false, u64 => false,
}

/// Which fixed-width type a value is, for a caller that learns it at run
/// time (the interpreter, from the receiver's type). Compiled code never
/// needs one: its types are constants at every call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedType {
    I8,
    I16,
    I32,
    /// `c-long`.
    I64,
    U8,
    U16,
    U32,
    /// `c-ulong`.
    U64,
}

impl FixedType {
    pub fn of(width: u32, signed: bool) -> Option<FixedType> {
        Some(match (width, signed) {
            (8, true) => FixedType::I8,
            (16, true) => FixedType::I16,
            (32, true) => FixedType::I32,
            (64, true) => FixedType::I64,
            (8, false) => FixedType::U8,
            (16, false) => FixedType::U16,
            (32, false) => FixedType::U32,
            (64, false) => FixedType::U64,
            _ => return None,
        })
    }

    pub fn bits(self) -> u32 {
        crate::with_fixed!(self, T => <T as Fixed>::BITS)
    }

    pub fn signed(self) -> bool {
        crate::with_fixed!(self, T => <T as Fixed>::SIGNED)
    }
}

/// `$body` with `$t` naming the Rust type of the [`FixedType`] `$ty`.
#[macro_export]
macro_rules! with_fixed {
    ($ty:expr, $t:ident => $body:expr) => {
        match $ty {
            $crate::fixed::FixedType::I8 => { type $t = i8; $body }
            $crate::fixed::FixedType::I16 => { type $t = i16; $body }
            $crate::fixed::FixedType::I32 => { type $t = i32; $body }
            $crate::fixed::FixedType::I64 => { type $t = i64; $body }
            $crate::fixed::FixedType::U8 => { type $t = u8; $body }
            $crate::fixed::FixedType::U16 => { type $t = u16; $body }
            $crate::fixed::FixedType::U32 => { type $t = u32; $body }
            $crate::fixed::FixedType::U64 => { type $t = u64; $body }
        }
    };
}

/// Why a division did not produce a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    DivideByZero,
    ModByZero,
    /// The most negative value over `-1`: the quotient is one past the
    /// type's maximum.
    Overflow { dividend: i128, divisor: i128 },
}

impl Fault {
    /// The panic message, the same from every engine.
    pub fn message(&self) -> String {
        match self {
            Fault::DivideByZero => "divide by zero".to_string(),
            Fault::ModByZero => "mod by zero".to_string(),
            Fault::Overflow { dividend, divisor } => format!("arithmetic overflow: {} / {}", dividend, divisor),
        }
    }
}

pub fn add<T: Fixed>(a: T, b: T) -> T {
    a.wrapping_add(b)
}

pub fn sub<T: Fixed>(a: T, b: T) -> T {
    a.wrapping_sub(b)
}

pub fn mul<T: Fixed>(a: T, b: T) -> T {
    a.wrapping_mul(b)
}

/// `/`: truncated toward zero.
pub fn div<T: Fixed>(a: T, b: T) -> Result<T, Fault> {
    if b == T::ZERO {
        return Err(Fault::DivideByZero);
    }
    a.checked_div(b).ok_or(Fault::Overflow { dividend: a.to_i128(), divisor: b.to_i128() })
}

/// `mod`: floored, so the result takes the divisor's sign (`-7 mod 3` is
/// `2`). The most negative value mod `-1` is `0`, not an overflow.
pub fn modulo<T: Fixed>(a: T, b: T) -> Result<T, Fault> {
    if b == T::ZERO {
        return Err(Fault::ModByZero);
    }
    let r = a.wrapping_rem(b);
    Ok(if r != T::ZERO && (r < T::ZERO) != (b < T::ZERO) { r.wrapping_add(b) } else { r })
}

/// `rem`: truncated, so the result takes the dividend's sign. The most
/// negative value rem `-1` is `0`, not an overflow.
pub fn rem<T: Fixed>(a: T, b: T) -> Result<T, Fault> {
    if b == T::ZERO {
        return Err(Fault::DivideByZero);
    }
    Ok(a.wrapping_rem(b))
}

pub fn max<T: Fixed>(a: T, b: T) -> T {
    if a >= b { a } else { b }
}

pub fn min<T: Fixed>(a: T, b: T) -> T {
    if a <= b { a } else { b }
}

pub fn lognot<T: Fixed>(a: T) -> T {
    a.not()
}

pub fn logand<T: Fixed>(a: T, b: T) -> T {
    a.and(b)
}

pub fn logior<T: Fixed>(a: T, b: T) -> T {
    a.or(b)
}

pub fn logxor<T: Fixed>(a: T, b: T) -> T {
    a.xor(b)
}

pub fn logtest<T: Fixed>(a: T, b: T) -> bool {
    a.and(b) != T::ZERO
}

/// `(ash a count)`: left for a positive `count`, right (arithmetic for a
/// signed type) for a negative one. A distance of the type's width or more
/// saturates: `0` to the left, and to the right `0`, or `-1` for a negative
/// signed value.
pub fn ash<T: Fixed>(a: T, count: i64) -> T {
    let bits = i64::from(T::BITS);
    if count >= 0 {
        if count >= bits { T::ZERO } else { a.shl(count as u32) }
    } else if count <= -bits {
        if a < T::ZERO { T::from_word(-1) } else { T::ZERO }
    } else {
        a.shr((-count) as u32)
    }
}

/// `(logbitp a index)`: bit `index` of `a`. Past the type's width every bit
/// is the sign, and a negative index reads the sign too, as `int`'s does.
pub fn logbitp<T: Fixed>(a: T, index: i64) -> bool {
    if index < 0 || index >= i64::from(T::BITS) {
        a < T::ZERO
    } else {
        a.shr(index as u32).and(T::from_word(1)) != T::ZERO
    }
}

/// `logcount`: the one bits of a non-negative value, the zero bits of a
/// negative one.
pub fn logcount<T: Fixed>(a: T) -> T {
    T::of_count(if a < T::ZERO { a.not().count_ones() } else { a.count_ones() })
}

/// `integer-length`: the bits needed to write `a` without its sign.
pub fn integer_length<T: Fixed>(a: T) -> T {
    let v = if a < T::ZERO { a.not() } else { a };
    T::of_count(T::BITS - v.leading_zeros())
}

/// `int->W` from another fixed width: truncated.
pub fn cast<T: Fixed, U: Fixed>(a: T) -> U {
    U::from_word(a.to_word())
}

/// `try-int->W` from another fixed width: the value when `U` can hold it.
pub fn try_cast<T: Fixed, U: Fixed>(a: T) -> Option<U> {
    U::from_i128(a.to_i128())
}

/// `int->W` from an `int`: its low bits.
pub fn from_int<U: Fixed>(n: IntArg) -> U {
    U::from_word(n.low_word())
}

/// `try-int->W` from an `int`.
pub fn try_from_int<U: Fixed>(n: &IntArg) -> Option<U> {
    match n {
        IntArg::Fix(n) => U::from_i128(i128::from(*n)),
        IntArg::Big(n) => n.to_i128().and_then(U::from_i128),
    }
}

/// `int->float`: the nearest `f64`.
pub fn to_float<T: Fixed>(a: T) -> f64 {
    a.to_f64()
}

/// `int->int`: exact.
pub fn to_int<T: Fixed>(a: T) -> i128 {
    a.to_i128()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_wraps_at_the_types_width() {
        assert_eq!(add::<i8>(127, 1), -128);
        assert_eq!(sub::<u8>(0, 1), 255);
        assert_eq!(mul::<u16>(300, 300), 24464);
    }

    #[test]
    fn division_faults() {
        assert_eq!(div::<i8>(-128, -1), Err(Fault::Overflow { dividend: -128, divisor: -1 }));
        assert_eq!(div::<u8>(1, 0), Err(Fault::DivideByZero));
        assert_eq!(modulo::<i8>(-128, -1), Ok(0));
        assert_eq!(modulo::<i8>(-7, 3), Ok(2));
        assert_eq!(modulo::<i8>(7, -3), Ok(-2));
        assert_eq!(modulo::<u32>(1, 0), Err(Fault::ModByZero));
        assert_eq!(rem::<i8>(-128, -1), Ok(0));
        assert_eq!(rem::<i8>(-7, 3), Ok(-1));
    }

    #[test]
    fn shifts_saturate() {
        assert_eq!(ash::<i8>(1, 8), 0);
        assert_eq!(ash::<i8>(-1, -8), -1);
        assert_eq!(ash::<u8>(255, -8), 0);
        assert_eq!(ash::<u8>(255, -7), 1);
        assert_eq!(ash::<i16>(-4, -1), -2);
        assert!(logbitp::<i8>(-1, 100));
        assert!(!logbitp::<u8>(255, 8));
    }

    #[test]
    fn counts_read_inside_the_width() {
        assert_eq!(logcount::<i8>(-1), 0);
        assert_eq!(logcount::<u8>(255), 8);
        assert_eq!(integer_length::<i8>(-128), 7);
        assert_eq!(integer_length::<u32>(u32::MAX), 32);
    }

    #[test]
    fn casts_truncate_and_try_casts_ask_the_range() {
        assert_eq!(cast::<i32, u8>(300), 44);
        assert_eq!(cast::<i8, u16>(-1), 65535);
        assert_eq!(try_cast::<i64, u64>(-1), None);
        assert_eq!(try_cast::<u64, i64>(u64::MAX), None);
        assert_eq!(try_cast::<i32, u8>(200), Some(200));
        assert_eq!(to_float::<u64>(u64::MAX), 18446744073709551615.0);
    }
}
