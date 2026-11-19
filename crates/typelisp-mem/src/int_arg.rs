//! An `int` operand, read out of either of its two shapes: the interpreter's
//! builtins and the compiled code's shims both take one apart this way.

use num_bigint::BigInt;
use num_traits::ToPrimitive;

/// One `int` operand: a fixnum, or the `BigInt` cloned out of its box (owned,
/// because every caller allocates next, and a `&BigInt` into the heap cannot
/// survive that).
pub enum IntArg {
    Fix(i64),
    Big(BigInt),
}

impl IntArg {
    pub fn into_big(self) -> BigInt {
        match self {
            IntArg::Fix(n) => BigInt::from(n),
            IntArg::Big(n) => n,
        }
    }

    /// The low 64 bits, two's complement — what a width cast out of an `int`
    /// starts from. A fixnum is already that; a bignum is cut, which is what
    /// `int->W` means.
    pub fn low_word(self) -> i64 {
        match self {
            IntArg::Fix(n) => n,
            IntArg::Big(n) => (n & BigInt::from(u64::MAX)).to_u64().expect("masked to 64 bits") as i64,
        }
    }
}
