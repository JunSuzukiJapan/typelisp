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
use inkwell::values::{BasicValue, BasicValueEnum, InstructionValue, IntValue, PointerValue};

use crate::check::repr::Repr;
use typelisp_front::sexpr_variant::carrier as c;
use typelisp_mem::fixed::FixedType;

thread_local! {
    /// How wide the word of the code being produced on this thread is.
    static WORD_BITS: std::cell::Cell<u32> = const { std::cell::Cell::new(usize::BITS) };
}

/// The word: what a compiled function takes, answers and keeps in its frame,
/// and the width of an address held as an integer. As wide as a pointer of
/// the machine the code is for: this one's, except while an executable for
/// another is emitted ([`word_of`]).
pub(crate) fn word() -> IntType<'static> {
    int(WORD_BITS.with(|w| w.get()))
}

/// While the answer lives, code emitted on this thread is for a machine whose
/// pointers are `bits` wide. Nothing emitted meanwhile can run in this
/// process.
pub(crate) fn word_of(bits: u32) -> WordWidth {
    WordWidth { before: WORD_BITS.with(|w| w.replace(bits)) }
}

/// [`word_of`]'s guard: puts back the width it replaced.
pub(crate) struct WordWidth {
    before: u32,
}

impl Drop for WordWidth {
    fn drop(&mut self) {
        WORD_BITS.with(|w| w.set(self.before));
    }
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

    /// How many words a value of this takes where words are what there is.
    pub(crate) fn words(self) -> usize {
        self.llvm().map_or(1, words_of)
    }

    /// `v` as this: itself when it is carried so already, and otherwise the
    /// value whose bits are the low ones of the integer `v`.
    pub(crate) fn coerce(self, b: &Builder<'static>, v: BasicValueEnum<'static>) -> Result<BasicValueEnum<'static>, String> {
        let Some(ty) = self.llvm() else { return Ok(word().const_zero().into()) };
        match v {
            _ if v.get_type() == ty => Ok(v),
            BasicValueEnum::IntValue(w) if w.get_type().get_bit_width() >= bits_of(ty) => from_bits(b, w, ty),
            _ => Err(format!("a {} where a {:?} is wanted", v.get_type().print_to_string(), self)),
        }
    }

    /// The value the words `ws` hold, carried as this.
    pub(crate) fn from_words(self, b: &Builder<'static>, ws: &[IntValue<'static>]) -> Result<BasicValueEnum<'static>, String> {
        if ws.len() != self.words() {
            return Err(format!("a {:?} out of {} words", self, ws.len()));
        }
        let Some(ty) = self.llvm() else { return Ok(word().const_zero().into()) };
        from_bits(b, join(b, ws, word())?, ty)
    }
}

/// How many bits a value of `ty` takes.
pub(crate) fn bits_of(ty: BasicTypeEnum<'static>) -> u32 {
    match ty {
        BasicTypeEnum::IntType(t) => t.get_bit_width(),
        BasicTypeEnum::FloatType(t) => t.get_bit_width(),
        BasicTypeEnum::PointerType(_) => word().get_bit_width(),
        other => unreachable!("no value is carried in a {}", other.print_to_string()),
    }
}

/// How many words a value of `ty` takes.
pub(crate) fn words_of(ty: BasicTypeEnum<'static>) -> usize {
    bits_of(ty).div_ceil(word().get_bit_width()) as usize
}

/// Whether the machine the code is for has the address `n`.
pub(crate) fn is_address(n: u64) -> bool {
    n >> (word().get_bit_width() - 1) <= 1
}

/// Whether the `int` `n` is a fixnum in the code being produced.
pub(crate) fn fixnum_fits(n: i64) -> bool {
    fits_below(n, word().get_bit_width())
}

/// Whether `n` fits a word of `bits` bits with one of them spent on the tag.
fn fits_below(n: i64, bits: u32) -> bool {
    let unused = i64::BITS + typelisp_mem::tagged::FIXNUM_SHIFT as u32 - bits;
    (n << unused) >> unused == n
}

/// How many words the widest value takes: what a frame's value slot is.
pub(crate) fn value_words() -> usize {
    i64::BITS.div_ceil(word().get_bit_width()) as usize
}

/// `v` in words, the low one first: a value narrower than a word
/// zero-extended, a float as the bits of its own width. Whoever reads the
/// words back reads the bits its type has, so what fills the rest does not
/// matter.
pub(crate) fn to_words(b: &Builder<'static>, v: BasicValueEnum<'static>) -> Result<Vec<IntValue<'static>>, String> {
    split(b, to_bits(b, v)?, word())
}

/// Stores `v` at `ptr`, which is as aligned as a word is.
pub(crate) fn store(b: &Builder<'static>, ptr: PointerValue<'static>, v: BasicValueEnum<'static>) -> Result<(), String> {
    let i = b.build_store(ptr, v).map_err(|e| e.to_string())?;
    align_as_words(i, v.get_type())
}

/// Loads a value of type `ty` from `ptr`, which is as aligned as a word is.
pub(crate) fn load(b: &Builder<'static>, ty: BasicTypeEnum<'static>, ptr: PointerValue<'static>, name: &str) -> Result<BasicValueEnum<'static>, String> {
    let v = b.build_load(ty, ptr, name).map_err(|e| e.to_string())?;
    let i = v.as_instruction_value().ok_or("a load is an instruction")?;
    align_as_words(i, ty)?;
    Ok(v)
}

/// LLVM takes an access to be as aligned as its type is, and a value wider
/// than a word is kept in words.
fn align_as_words(i: InstructionValue<'static>, ty: BasicTypeEnum<'static>) -> Result<(), String> {
    if words_of(ty) > 1 {
        i.set_alignment(word().get_bit_width() / 8).map_err(|e| format!("{:?}", e))?;
    }
    Ok(())
}

/// `v` as an integer of its own width.
pub(crate) fn to_bits(b: &Builder<'static>, v: BasicValueEnum<'static>) -> Result<IntValue<'static>, String> {
    let e = |e: inkwell::builder::BuilderError| format!("to bits: {}", e);
    Ok(match v {
        BasicValueEnum::IntValue(v) => v,
        BasicValueEnum::FloatValue(f) => b.build_bit_cast(f, int(f.get_type().get_bit_width()), "bits").map_err(e)?.into_int_value(),
        BasicValueEnum::PointerValue(p) => b.build_ptr_to_int(p, word(), "bits").map_err(e)?,
        other => return Err(format!("to bits: a {}", other.get_type().print_to_string())),
    })
}

/// The value of type `ty` whose bits are the low ones of `w`.
fn from_bits(b: &Builder<'static>, w: IntValue<'static>, ty: BasicTypeEnum<'static>) -> Result<BasicValueEnum<'static>, String> {
    let e = |e: inkwell::builder::BuilderError| format!("from bits: {}", e);
    let bits = bits_of(ty);
    let low = match ty {
        BasicTypeEnum::IntType(t) => t,
        _ => int(bits),
    };
    let w = match w.get_type().get_bit_width().cmp(&bits) {
        std::cmp::Ordering::Greater => b.build_int_truncate(w, low, "low").map_err(e)?,
        _ => w,
    };
    Ok(match ty {
        BasicTypeEnum::FloatType(t) => b.build_bit_cast(w, t, "float").map_err(e)?,
        _ => w.into(),
    })
}

/// The integer `v` in integers of type `part`, the low one first.
fn split(b: &Builder<'static>, v: IntValue<'static>, part: IntType<'static>) -> Result<Vec<IntValue<'static>>, String> {
    let e = |e: inkwell::builder::BuilderError| format!("to words: {}", e);
    let ty = v.get_type();
    let (bits, each) = (ty.get_bit_width(), part.get_bit_width());
    if bits == each {
        return Ok(vec![v]);
    }
    if bits < each {
        return Ok(vec![b.build_int_z_extend(v, part, "word").map_err(e)?]);
    }
    (0..bits.div_ceil(each))
        .map(|i| {
            let shift = ty.const_int(u64::from(i * each), false);
            let high = if i == 0 { v } else { b.build_right_shift(v, shift, false, "high").map_err(e)? };
            b.build_int_truncate(high, part, "word").map_err(e)
        })
        .collect()
}

/// The integer whose parts, each of type `part`, are `ws`: [`split`] undone.
fn join(b: &Builder<'static>, ws: &[IntValue<'static>], part: IntType<'static>) -> Result<IntValue<'static>, String> {
    let e = |e: inkwell::builder::BuilderError| format!("from words: {}", e);
    let [low, rest @ ..] = ws else { return Err("from words: there are none".to_string()) };
    if rest.is_empty() {
        return Ok(*low);
    }
    let each = part.get_bit_width();
    let ty = int(each * ws.len() as u32);
    let mut v = b.build_int_z_extend(*low, ty, "low").map_err(e)?;
    for (i, w) in rest.iter().enumerate() {
        let wide = b.build_int_z_extend(*w, ty, "high").map_err(e)?;
        let shift = ty.const_int(u64::from((i as u32 + 1) * each), false);
        let high = b.build_left_shift(wide, shift, "high").map_err(e)?;
        v = b.build_or(v, high, "joined").map_err(e)?;
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a fixnum ends, by the width of the word it is in.
    #[test]
    fn a_fixnum_is_one_bit_narrower_than_the_word() {
        for bits in [16, 32, 64u32] {
            let max = i64::MAX >> (65 - bits);
            assert!(fits_below(max, bits) && fits_below(-max - 1, bits));
            assert!(!fits_below(max + 1, bits) && !fits_below(-max - 2, bits));
        }
        assert_eq!(i64::MAX >> 1, typelisp_mem::FIXNUM_MAX);
    }

    /// A value is carried as wide as its type is, on every machine there is
    /// code for: a `bool` is a bit of its byte, and a C word is as wide as C's.
    #[test]
    fn a_carrier_is_as_wide_as_its_type() {
        use crate::Type;
        use typelisp_mem::target::DataModel;
        for model in [DataModel::HOST, DataModel::ILP32, DataModel::AVR] {
            let _model = DataModel::producing(model);
            let _word = word_of(model.pointer_bits());
            for (ty, bits) in [
                (Type::I8, 8),
                (Type::U8, 8),
                (Type::I16, 16),
                (Type::U16, 16),
                (Type::I32, 32),
                (Type::U32, 32),
                (Type::F32, 32),
                (Type::F64, 64),
                (Type::Char, 32),
                (Type::Bool, 1),
                (Type::CLong, model.c_long_bits()),
                (Type::CULong, model.c_long_bits()),
                (Type::Ptr, model.pointer_bits()),
                (Type::Int, model.pointer_bits()),
                (Type::Str, model.pointer_bits()),
            ] {
                let carrier = Carrier::of(&Repr::of_by(&ty, &|_| None));
                assert_eq!(carrier.llvm().map(bits_of), Some(bits), "{} on {:?}", ty, model);
            }
        }
    }

    /// A value wider than the part it is kept in comes back whole: a `f64`
    /// and an `i64` in 32-bit and 16-bit words.
    #[test]
    fn a_wide_value_survives_narrow_words() {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = crate::compile::llvm_context();
        let module = ctx.create_module("carriers");
        let f = module.add_function("f", ctx.void_type().fn_type(&[], false), None);
        let b = ctx.create_builder();
        b.position_at_end(ctx.append_basic_block(f, "entry"));
        for part in [int(16), int(32), int(64)] {
            let n = 64 / part.get_bit_width() as usize;
            let v = int(64).const_int(0x8123_4567_89ab_cdef, false);
            let ws = split(&b, v, part).unwrap();
            assert_eq!(ws.len(), n);
            assert_eq!(ws[0].get_zero_extended_constant(), Some(0x8123_4567_89ab_cdef & (u64::MAX >> (64 - part.get_bit_width()))));
            assert_eq!(join(&b, &ws, part).unwrap().get_zero_extended_constant(), Some(0x8123_4567_89ab_cdef));

            let x = ctx.f64_type().const_float(-1.5);
            let ws = split(&b, to_bits(&b, x.into()).unwrap(), part).unwrap();
            let back = from_bits(&b, join(&b, &ws, part).unwrap(), ctx.f64_type().into()).unwrap();
            assert_eq!(back.into_float_value().get_constant().map(|c| c.0), Some(-1.5));
        }
        let x = ctx.f32_type().const_float(0.25);
        let ws = split(&b, to_bits(&b, x.into()).unwrap(), int(16)).unwrap();
        assert_eq!(ws.len(), 2);
    }
}
