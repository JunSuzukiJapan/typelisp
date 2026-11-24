//! The LLVM types compiled code carries values in.
//!
//! One place, so the width of the word is decided once. A value is carried at
//! its type's own LLVM type, [`Carrier`], from where it is made to where it
//! is used: in an expression, in a frame's slot, in a direct function's
//! parameters and in a runtime function's. It goes into a word only where
//! words are what there is: the arguments a call leaves for the driver, the
//! answer that comes back, and the arrays a runtime function of any number of
//! arguments takes.

use inkwell::builder::Builder;
use inkwell::types::{BasicTypeEnum, IntType};
use inkwell::values::{BasicValueEnum, IntValue};

use crate::check::repr::Repr;
use typelisp_front::sexpr_variant::carrier as c;
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

/// The LLVM type a value of one representation is carried in.
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

    /// The number the island names this by.
    pub(crate) fn code(self) -> i64 {
        match self {
            Carrier::Word => c::WORD as i64,
            Carrier::Bool => c::BOOL as i64,
            Carrier::Unit => c::UNIT as i64,
            Carrier::F32 => c::F32 as i64,
            Carrier::F64 => c::F64 as i64,
            Carrier::Int { bits, signed } => i64::from(bits) * 2 + i64::from(signed),
        }
    }

    /// The inverse of [`Carrier::code`].
    pub(crate) fn of_code(code: i64) -> Result<Carrier, String> {
        let no = || format!("no carrier is numbered {}", code);
        Ok(match usize::try_from(code).map_err(|_| no())? {
            c::WORD => Carrier::Word,
            c::BOOL => Carrier::Bool,
            c::UNIT => Carrier::Unit,
            c::F32 => Carrier::F32,
            c::F64 => Carrier::F64,
            16 | 17 | 32 | 33 | 64 | 65 | 128 | 129 => Carrier::Int { bits: (code >> 1) as u32, signed: code & 1 == 1 },
            _ => return Err(no()),
        })
    }

    /// How a value is carried that a tagged slot holds under the field kind
    /// `kind` (`Repr::field_kind`).
    pub(crate) fn of_field_kind(kind: i64) -> Result<Carrier, String> {
        use typelisp_front::sexpr_variant::field_kind as k;
        let Ok(kind) = usize::try_from(kind) else { return Err(format!("no field kind is numbered {}", kind)) };
        Ok(match kind {
            k::F64 => Carrier::F64,
            k::F32 => Carrier::F32,
            k::CHAR => Carrier::Int { bits: 32, signed: false },
            k::BOOL => Carrier::Bool,
            k::UNIT => Carrier::Unit,
            k::RAW_INT | k::TAGGED | k::NICHE => Carrier::Word,
            _ if kind >= k::FIXED => Carrier::of_code((kind - k::FIXED) as i64)?,
            _ => return Err(format!("the field kind {} has no carrier", kind)),
        })
    }

    /// How a parameter or a captured value is carried, from the number the
    /// island's parameter list gives it: `2` for a rooted word, `10` and up
    /// for a cell over a field kind, and otherwise the carrier's own number,
    /// negated (`core_bridge::param_kind`).
    pub(crate) fn of_param_kind(kind: i64) -> Result<Carrier, String> {
        match kind {
            2 => Ok(Carrier::Word),
            10.. => Carrier::of_field_kind(kind - 10),
            _ => Carrier::of_code(-kind),
        }
    }

    /// The zero of this carrier; `()` is the word zero.
    pub(crate) fn zero(self) -> BasicValueEnum<'static> {
        super::raise_check::zero_of(self.llvm().unwrap_or_else(|| word().into()))
    }

    /// `v` as this: itself when it is carried so already, and otherwise the
    /// value the word `v` holds.
    pub(crate) fn coerce(self, b: &Builder<'static>, v: BasicValueEnum<'static>) -> Result<BasicValueEnum<'static>, String> {
        let Some(ty) = self.llvm() else { return Ok(word().const_zero().into()) };
        match v {
            _ if v.get_type() == ty => Ok(v),
            BasicValueEnum::IntValue(w) if w.get_type() == word() => self.from_word(b, w),
            // A narrower type's value in a wider integer's low bits: a
            // volatile read of any width answers 32.
            BasicValueEnum::IntValue(w) => match ty {
                BasicTypeEnum::IntType(t) if t.get_bit_width() < w.get_type().get_bit_width() => {
                    Ok(b.build_int_truncate(w, t, "narrow").map_err(|e| e.to_string())?.into())
                }
                _ => Err(format!("a {} where a {:?} is wanted", v.get_type().print_to_string(), self)),
            },
            _ => Err(format!("a {} where a {:?} is wanted", v.get_type().print_to_string(), self)),
        }
    }

    /// The value the word `w` holds, carried as this.
    pub(crate) fn from_word(self, b: &Builder<'static>, w: IntValue<'static>) -> Result<BasicValueEnum<'static>, String> {
        let ctx = super::llvm_context();
        let e = |e: inkwell::builder::BuilderError| format!("from the word: {}", e);
        Ok(match self {
            Carrier::Word | Carrier::Int { bits: 64, .. } => w.into(),
            Carrier::Int { bits, .. } => b.build_int_truncate(w, int(bits), "narrow").map_err(e)?.into(),
            Carrier::Bool => b.build_int_truncate(w, ctx.bool_type(), "flag").map_err(e)?.into(),
            Carrier::F32 => {
                let wide = b.build_bit_cast(w, ctx.f64_type(), "wide").map_err(e)?.into_float_value();
                b.build_float_trunc(wide, ctx.f32_type(), "narrow").map_err(e)?.into()
            }
            Carrier::F64 => b.build_bit_cast(w, ctx.f64_type(), "float").map_err(e)?,
            Carrier::Unit => word().const_zero().into(),
        })
    }
}

/// `v` in a word: an integer zero-extended, a float as the bits of a
/// `double`. Whoever reads the word back reads the low bits its type has, so
/// what fills the rest does not matter.
pub(crate) fn to_word(b: &Builder<'static>, v: BasicValueEnum<'static>) -> Result<IntValue<'static>, String> {
    let ctx = super::llvm_context();
    let e = |e: inkwell::builder::BuilderError| format!("to the word: {}", e);
    Ok(match v {
        BasicValueEnum::IntValue(v) if v.get_type() == word() => v,
        BasicValueEnum::IntValue(v) if v.get_type().get_bit_width() < word().get_bit_width() => b.build_int_z_extend(v, word(), "word").map_err(e)?,
        BasicValueEnum::FloatValue(f) => {
            let wide = if f.get_type() == ctx.f64_type() { f } else { b.build_float_ext(f, ctx.f64_type(), "wide").map_err(e)? };
            b.build_bit_cast(wide, word(), "word").map_err(e)?.into_int_value()
        }
        BasicValueEnum::PointerValue(p) => b.build_ptr_to_int(p, word(), "word").map_err(e)?,
        other => return Err(format!("to the word: a {}", other.get_type().print_to_string())),
    })
}
