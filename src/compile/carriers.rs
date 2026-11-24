//! The LLVM types compiled code carries values in.
//!
//! One place, so the width of the word is decided once. Every value an
//! island-compiled function holds is the word: a tagged value, a fixed-width
//! integer sign- or zero-extended into it, a float's bits. The fixed-width
//! integers and C's types are computed and passed at their own widths
//! ([`int`]) and converted to and from the word where they enter and leave it.

use inkwell::types::IntType;

/// The word: what a compiled function takes, answers and keeps in its frame,
/// and the width of an address held as an integer.
pub(crate) fn word() -> IntType<'static> {
    super::llvm_context().i64_type()
}

/// The integer type `bits` wide: one of the widths a fixed-width integer or
/// a C integer has.
pub(crate) fn int(bits: u32) -> IntType<'static> {
    let ctx = super::llvm_context();
    match bits {
        8 => ctx.i8_type(),
        16 => ctx.i16_type(),
        32 => ctx.i32_type(),
        64 => ctx.i64_type(),
        _ => unimplemented!("a {}-bit integer", bits),
    }
}
