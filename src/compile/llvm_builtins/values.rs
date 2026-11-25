//! The island's constants and conversions between the types values are
//! carried in, and the one place that tags a value for a field and takes it
//! back out.

use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue};
use inkwell::IntPredicate;

use typelisp_front::sexpr_variant::field_kind as k;
use typelisp_mem::Value;

use super::{expect_int, expect_llvm_builder, expect_llvm_module, llvm_value_value};
use crate::compile::carriers::{self, Carrier};
use crate::eval::interp::EvalError;

/// The value an argument is, returning no value from the builder around it
/// when it is none.
macro_rules! value_or_return {
    ($v:expr) => {
        match $crate::compile::llvm_builtins::slots::operand($v)? {
            Some(v) => v,
            None => return $crate::compile::llvm_builtins::slots::no_value(),
        }
    };
}
pub(super) use value_or_return;

/// The 64 bits the island passes as two 32-bit halves.
fn bits_of(hi: &Value, lo: &Value) -> Result<u64, EvalError> {
    Ok(((expect_int(hi)? as u64) << 32) | (expect_int(lo)? as u64 & 0xFFFF_FFFF))
}

fn constant(ty: BasicTypeEnum<'static>, bits: u64, what: &str) -> Result<BasicValueEnum<'static>, EvalError> {
    Ok(match ty {
        BasicTypeEnum::IntType(t) => t.const_int(bits, false).into(),
        // An `f32` constant is given as the `double` of its value.
        BasicTypeEnum::FloatType(t) => t.const_float(f64::from_bits(bits)).into(),
        other => return Err(EvalError::Internal(format!("{}: no constant of {}", what, other.print_to_string()))),
    })
}

/// `(const-value builder carrier hi lo)` — a constant, its bits in two
/// 32-bit halves: an integer's value, or a float's as the bits of a `double`.
pub(super) fn const_value(args: &[Value]) -> Result<Value, EvalError> {
    let carrier = Carrier::of_code(expect_int(&args[1])?).map_err(EvalError::Internal)?;
    let Some(ty) = carrier.llvm() else { return Ok(llvm_value_value(carrier.zero())) };
    Ok(llvm_value_value(constant(ty, bits_of(&args[2], &args[3])?, "const-value")?))
}

/// `(const-like builder v hi lo)` — the constant of `v`'s own type.
pub(super) fn const_like(args: &[Value]) -> Result<Value, EvalError> {
    let v = value_or_return!(&args[1]);
    Ok(llvm_value_value(constant(v.get_type(), bits_of(&args[2], &args[3])?, "const-like")?))
}

/// `(convert builder from to v)` — the integer `v`, carried as `from`, at
/// `to`: cut to a narrower type, or extended to a wider one by `from`'s sign.
pub(super) fn convert(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let carrier = |v: &Value| Carrier::of_code(expect_int(v)?).map_err(EvalError::Internal);
    let (from, to) = (carrier(&args[1])?, carrier(&args[2])?);
    let v = value_or_return!(&args[3]).into_int_value();
    let err = |e: String| EvalError::Internal(format!("convert: {}", e));
    let Some(BasicTypeEnum::IntType(ty)) = to.llvm() else { return Err(err(format!("to {:?}", to))) };
    let signed = matches!(from, Carrier::Word | Carrier::Int { signed: true, .. });
    let r = builder.borrow().build_int_cast_sign_flag(v, ty, signed, "convert").map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(r.into()))
}

/// `(coerce builder carrier v)` — `v`, which is carried as `carrier` or is
/// the word such a value came in.
pub(super) fn coerce(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let carrier = Carrier::of_code(expect_int(&args[1])?).map_err(EvalError::Internal)?;
    let v = value_or_return!(&args[2]);
    let r = carrier.coerce(&builder.borrow(), v).map_err(EvalError::Internal)?;
    Ok(llvm_value_value(r))
}

/// `(build-not builder v)` — the other `bool`.
pub(super) fn build_not(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let v = value_or_return!(&args[1]).into_int_value();
    let r = builder.borrow().build_not(v, "not").map_err(|e| EvalError::Internal(format!("build-not: {}", e)))?;
    Ok(llvm_value_value(r.into()))
}

/// `(tag-field builder m v kind)` — the tagged word a field of the field kind
/// `kind` (`Repr::field_kind`) holds for the value `v`.
pub(super) fn tag_field(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let kind = expect_int(&args[3])?;
    let err = |e: String| EvalError::Internal(format!("tag-field: {}", e));
    let carrier = Carrier::of_field_kind(kind).map_err(err)?;
    let v = value_or_return!(&args[2]);
    let b = builder.borrow();
    let word_ty = carriers::word();
    let fail = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("tag-field: {}", e));
    let call = |name: &str| -> Result<BasicValueEnum<'static>, EvalError> {
        let (f, r) = crate::compile::runtime_abi::call(&b, &module.borrow(), name, &[v]).map_err(err)?;
        crate::compile::raise_check::after_call(&b, f).map_err(err)?;
        Ok(r)
    };
    let fixnum = |n: IntValue<'static>| b.build_left_shift(n, word_ty.const_int(1, false), "fixnum").map_err(fail);
    let tagged: BasicValueEnum<'static> = match kind as usize {
        k::F64 => call("rt_f64_new")?,
        k::F32 => call("rt_f32_new")?,
        k::CHAR => {
            let n = b.build_int_z_extend(v.into_int_value(), word_ty, "scalar").map_err(fail)?;
            let n = b.build_left_shift(n, word_ty.const_int(typelisp_mem::tagged::SMALL_SHIFT as u64, false), "scalar").map_err(fail)?;
            b.build_or(n, word_ty.const_int(typelisp_mem::tagged::TAG_CHAR as u64, false), "char").map_err(fail)?.into()
        }
        k::BOOL => b.build_select(v.into_int_value(), bool_word(true), bool_word(false), "bool").map_err(fail)?,
        k::UNIT => word_ty.const_int(typelisp_mem::tagged::NIL_WORD as u64, false).into(),
        k::RAW_INT => fixnum(v.into_int_value())?.into(),
        k::TAGGED | k::NICHE => carrier.coerce(&b, v).map_err(err)?,
        _ => {
            let Carrier::Int { signed, .. } = carrier else { return Err(err(format!("the field kind {}", kind))) };
            let wide = b.build_int_cast_sign_flag(v.into_int_value(), word_ty, signed, "wide").map_err(fail)?;
            fixnum(wide)?.into()
        }
    };
    Ok(llvm_value_value(tagged))
}

/// `(untag-field builder m v kind)` — the value a field of the field kind
/// `kind` holds, out of its tagged word.
pub(super) fn untag_field(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let kind = expect_int(&args[3])?;
    let err = |e: String| EvalError::Internal(format!("untag-field: {}", e));
    let carrier = Carrier::of_field_kind(kind).map_err(err)?;
    let word = value_or_return!(&args[2]);
    let b = builder.borrow();
    let word_ty = carriers::word();
    let fail = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("untag-field: {}", e));
    let call = |name: &str| -> Result<BasicValueEnum<'static>, EvalError> {
        let (f, r) = crate::compile::runtime_abi::call(&b, &module.borrow(), name, &[word]).map_err(err)?;
        crate::compile::raise_check::after_call(&b, f).map_err(err)?;
        Ok(r)
    };
    let fixnum = |w: IntValue<'static>| b.build_right_shift(w, word_ty.const_int(1, false), true, "payload").map_err(fail);
    let v: BasicValueEnum<'static> = match kind as usize {
        k::F64 => call("rt_f64_value")?,
        k::F32 => call("rt_f32_value")?,
        k::CHAR => {
            let n = b
                .build_right_shift(word.into_int_value(), word_ty.const_int(typelisp_mem::tagged::SMALL_SHIFT as u64, false), false, "scalar")
                .map_err(fail)?;
            b.build_int_truncate(n, carriers::int(32), "char").map_err(fail)?.into()
        }
        k::BOOL => b.build_int_compare(IntPredicate::EQ, word.into_int_value(), bool_word(true), "bool").map_err(fail)?.into(),
        k::UNIT => Carrier::Unit.zero(),
        k::RAW_INT => fixnum(word.into_int_value())?.into(),
        k::TAGGED | k::NICHE => word,
        _ => carrier.coerce(&b, fixnum(word.into_int_value())?.into()).map_err(err)?,
    };
    Ok(llvm_value_value(v))
}

/// The word of `false` or `true`.
fn bool_word(b: bool) -> IntValue<'static> {
    use typelisp_mem::tagged::{IMMEDIATE_FALSE, IMMEDIATE_TRUE, SMALL_SHIFT, TAG_IMMEDIATE};
    let n = if b { IMMEDIATE_TRUE } else { IMMEDIATE_FALSE };
    carriers::word().const_int(((n << SMALL_SHIFT) | TAG_IMMEDIATE) as u64, false)
}
