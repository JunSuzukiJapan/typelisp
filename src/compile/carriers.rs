//! The LLVM types compiled code carries values in.
//!
//! One place, so the width of the word is decided once. A value that sits in
//! a word carrier — a frame slot, an argument array, a runtime function's
//! parameter — is the word: a tagged value, a fixed-width integer sign- or
//! zero-extended into it, a float's bits. Where a value is not in a word (a
//! direct function's parameters and result), it is carried at its type's own
//! LLVM type, [`Carrier`], and converted to and from the word where it enters
//! and leaves one.

use inkwell::builder::Builder;
use inkwell::types::{BasicTypeEnum, IntType};
use inkwell::values::{BasicValueEnum, IntValue};

use crate::check::repr::Repr;
use typelisp_mem::fixed::FixedType;

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

/// The LLVM type a value of one representation is carried in outside a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Carrier {
    /// The word itself: every tagged value, and the untagged words (a handle,
    /// a C word) that are already as wide as one.
    Word,
    /// A fixed-width integer or a character, at its width.
    Int { bits: u32, signed: bool },
    F32,
    F64,
    /// `i1`.
    Bool,
    /// `()`: carried nowhere. A parameter of it is left out and a result of it
    /// is `void`.
    Unit,
}

impl Carrier {
    /// How a value of `repr` is carried.
    pub(crate) fn of(repr: &Repr) -> Carrier {
        match repr {
            Repr::Narrow(t) => {
                let (bits, signed) = match t {
                    FixedType::I8 => (8, true),
                    FixedType::I16 => (16, true),
                    FixedType::I32 => (32, true),
                    FixedType::I64 => (64, true),
                    FixedType::U8 => (8, false),
                    FixedType::U16 => (16, false),
                    FixedType::U32 => (32, false),
                    FixedType::U64 => (64, false),
                };
                Carrier::Int { bits, signed }
            }
            Repr::Char => Carrier::Int { bits: 32, signed: false },
            Repr::F32 => Carrier::F32,
            Repr::F64 => Carrier::F64,
            Repr::Bool => Carrier::Bool,
            Repr::Unit => Carrier::Unit,
            _ => Carrier::Word,
        }
    }

    /// The LLVM type, or `None` for [`Carrier::Unit`].
    pub(crate) fn llvm(self) -> Option<BasicTypeEnum<'static>> {
        let ctx = super::llvm_context();
        Some(match self {
            Carrier::Word => word().into(),
            Carrier::Int { bits, .. } => int(bits).into(),
            Carrier::F32 => ctx.f32_type().into(),
            Carrier::F64 => ctx.f64_type().into(),
            Carrier::Bool => ctx.bool_type().into(),
            Carrier::Unit => return None,
        })
    }

    /// `v`, carried as this, in a word.
    pub(crate) fn to_word(self, b: &Builder<'static>, v: BasicValueEnum<'static>) -> Result<IntValue<'static>, String> {
        let ctx = super::llvm_context();
        let e = |e: inkwell::builder::BuilderError| format!("to the word: {}", e);
        Ok(match self {
            Carrier::Word => v.into_int_value(),
            Carrier::Int { bits: 64, .. } => v.into_int_value(),
            Carrier::Int { signed: true, .. } => b.build_int_s_extend(v.into_int_value(), word(), "word").map_err(e)?,
            Carrier::Int { signed: false, .. } | Carrier::Bool => b.build_int_z_extend(v.into_int_value(), word(), "word").map_err(e)?,
            Carrier::F32 => {
                let wide = b.build_float_ext(v.into_float_value(), ctx.f64_type(), "wide").map_err(e)?;
                b.build_bit_cast(wide, word(), "word").map_err(e)?.into_int_value()
            }
            Carrier::F64 => b.build_bit_cast(v, word(), "word").map_err(e)?.into_int_value(),
            Carrier::Unit => word().const_zero(),
        })
    }

    /// The value a word holds, carried as this. `None` for [`Carrier::Unit`].
    pub(crate) fn from_word(self, b: &Builder<'static>, w: IntValue<'static>) -> Result<Option<BasicValueEnum<'static>>, String> {
        let ctx = super::llvm_context();
        let e = |e: inkwell::builder::BuilderError| format!("from the word: {}", e);
        Ok(Some(match self {
            Carrier::Word | Carrier::Int { bits: 64, .. } => w.into(),
            Carrier::Int { bits, .. } => b.build_int_truncate(w, int(bits), "narrow").map_err(e)?.into(),
            Carrier::Bool => b.build_int_truncate(w, ctx.bool_type(), "flag").map_err(e)?.into(),
            Carrier::F32 => {
                let wide = b.build_bit_cast(w, ctx.f64_type(), "wide").map_err(e)?.into_float_value();
                b.build_float_trunc(wide, ctx.f32_type(), "narrow").map_err(e)?.into()
            }
            Carrier::F64 => b.build_bit_cast(w, ctx.f64_type(), "float").map_err(e)?,
            Carrier::Unit => return Ok(None),
        }))
    }
}
