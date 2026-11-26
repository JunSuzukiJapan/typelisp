//! The island's numeric builders: one operation on a fixed-width integer or
//! a float, computed at the type's own width.
//!
//! Every operand and every result is in the type's own LLVM type — `i8` for
//! `u8`, `float` for `f32` — and a comparison answers an `i1`.
//!
//! The island passes the type (a fixed-width integer as its `wsig`,
//! `width * 2 + signed`; a float as its width in bits) and the method's name,
//! and emits no width handling of its own. What each operation means is
//! `typelisp_mem::fixed`'s and `typelisp_mem::float`'s; these builders are
//! that meaning written as instructions.

use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::{FloatType, IntType};
use inkwell::values::{FloatValue, IntValue};
use inkwell::{FloatPredicate, IntPredicate};

use typelisp_mem::{Heap, Value};

use super::values::value_or_return;
use super::{expect_int, expect_llvm_builder, expect_llvm_module, llvm_value_value};
use crate::eval::interp::{expect_str, EvalError};

/// A fixed-width integer type as the island names it.
#[derive(Clone, Copy)]
struct Fixed {
    bits: u32,
    signed: bool,
}

impl Fixed {
    fn of(wsig: i64) -> Result<Fixed, EvalError> {
        let (bits, signed) = ((wsig >> 1) as u32, wsig & 1 == 1);
        match bits {
            8 | 16 | 32 | 64 => Ok(Fixed { bits, signed }),
            _ => Err(EvalError::Internal(format!("not a fixed-width integer type: wsig {}", wsig))),
        }
    }

    fn ty(self) -> IntType<'static> {
        crate::compile::carriers::int(self.bits)
    }

    /// The type of the target a `int->W`/`try-int->W` method converts to,
    /// read off its name.
    fn target_of(method: &str) -> Option<Fixed> {
        let to = method.strip_prefix("try-int->").or_else(|| method.strip_prefix("int->"))?;
        Some(match to {
            "i8" => Fixed { bits: 8, signed: true },
            "i16" => Fixed { bits: 16, signed: true },
            "i32" => Fixed { bits: 32, signed: true },
            "c-long" => Fixed { bits: typelisp_mem::target::DataModel::current().c_long_bits(), signed: true },
            "u8" => Fixed { bits: 8, signed: false },
            "u16" => Fixed { bits: 16, signed: false },
            "u32" => Fixed { bits: 32, signed: false },
            "c-ulong" => Fixed { bits: typelisp_mem::target::DataModel::current().c_long_bits(), signed: false },
            _ => return None,
        })
    }
}

/// One builder's instructions, with the error every failed one becomes.
struct Emit<'b> {
    b: &'b Builder<'static>,
    who: &'static str,
}

impl Emit<'_> {
    fn err(&self, e: impl std::fmt::Display) -> EvalError {
        EvalError::Internal(format!("{}: {}", self.who, e))
    }

    /// `v`, which is of `from`, at the type `to`.
    fn cast(&self, from: Fixed, to: Fixed, v: IntValue<'static>) -> Result<IntValue<'static>, EvalError> {
        self.b.build_int_cast_sign_flag(v, to.ty(), from.signed, "cast").map_err(|e| self.err(e))
    }

    fn cmp(&self, p: IntPredicate, a: IntValue<'static>, b: IntValue<'static>) -> Result<IntValue<'static>, EvalError> {
        self.b.build_int_compare(p, a, b, "cmp").map_err(|e| self.err(e))
    }

    fn select(&self, c: IntValue<'static>, a: IntValue<'static>, b: IntValue<'static>) -> Result<IntValue<'static>, EvalError> {
        self.b.build_select(c, a, b, "select").map(|v| v.into_int_value()).map_err(|e| self.err(e))
    }

    fn intrinsic(&self, m: &Module<'static>, name: &str, types: &[inkwell::types::BasicTypeEnum<'static>], args: &[inkwell::values::BasicMetadataValueEnum<'static>]) -> Result<inkwell::values::BasicValueEnum<'static>, EvalError> {
        let decl = inkwell::intrinsics::Intrinsic::find(name)
            .ok_or_else(|| self.err(format!("no such LLVM intrinsic {}", name)))?
            .get_declaration(m, types)
            .ok_or_else(|| self.err(format!("failed to declare {}", name)))?;
        match self.b.build_call(decl, args, name).map_err(|e| self.err(e))?.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v),
            inkwell::values::ValueKind::Instruction(_) => Err(self.err(format!("{} produced no value", name))),
        }
    }
}

/// The predicate a comparison method names, at `t`'s signedness.
fn int_predicate(method: &str, t: Fixed) -> Option<IntPredicate> {
    let s = t.signed;
    Some(match method {
        "<" => if s { IntPredicate::SLT } else { IntPredicate::ULT },
        "<=" => if s { IntPredicate::SLE } else { IntPredicate::ULE },
        ">" => if s { IntPredicate::SGT } else { IntPredicate::UGT },
        ">=" => if s { IntPredicate::SGE } else { IntPredicate::UGE },
        "=" | "eq" | "eql" | "equal" | "equalp" => IntPredicate::EQ,
        "/=" => IntPredicate::NE,
        _ => return None,
    })
}

/// `(build-fixed-binop builder wsig method a b)`: a two-operand method of a
/// fixed-width integer — arithmetic, comparison, bitwise, `max`/`min`, the
/// divisions, `ash` and `logbitp`. A comparison answers an `i1`.
///
/// `/`, `mod` and `rem` assume their operands passed
/// `build-fixed-div-check`: the island branches to the runtime's fault on
/// the other side. `ash`'s and `logbitp`'s `b` is a bit count, an untagged word
/// whatever the receiver's type.
pub(super) fn build_fixed_binop(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let t = Fixed::of(expect_int(&args[1])?)?;
    let method = expect_str(heap, &args[2])?.to_string();
    let (a, b) = (value_or_return!(&args[3]).into_int_value(), value_or_return!(&args[4]).into_int_value());
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-fixed-binop" };
    if method == "ash" || method == "logbitp" {
        return Ok(llvm_value_value(shift(&e, t, &method, a, b)?.into()));
    }
    if let Some(p) = int_predicate(&method, t) {
        return Ok(llvm_value_value(e.cmp(p, a, b)?.into()));
    }
    let zero = t.ty().const_zero();
    let r = match method.as_str() {
        "+" => bld.build_int_add(a, b, "add"),
        "-" => bld.build_int_sub(a, b, "sub"),
        "*" => bld.build_int_mul(a, b, "mul"),
        "logand" => bld.build_and(a, b, "and"),
        "logior" => bld.build_or(a, b, "or"),
        "logxor" => bld.build_xor(a, b, "xor"),
        "logtest" => {
            let both = bld.build_and(a, b, "and").map_err(|x| e.err(x))?;
            return Ok(llvm_value_value(e.cmp(IntPredicate::NE, both, zero)?.into()));
        }
        // The first operand unless the second is strictly greater (smaller):
        // `typelisp_mem::fixed::max`/`min`.
        "max" => {
            let p = int_predicate(">", t).expect("a comparison");
            Ok(e.select(e.cmp(p, b, a)?, b, a)?)
        }
        "min" => {
            let p = int_predicate("<", t).expect("a comparison");
            Ok(e.select(e.cmp(p, b, a)?, b, a)?)
        }
        "/" => if t.signed { bld.build_int_signed_div(a, b, "div") } else { bld.build_int_unsigned_div(a, b, "div") },
        "rem" | "mod" if !t.signed => bld.build_int_unsigned_rem(a, b, "rem"),
        "rem" | "mod" => {
            // `srem` of the most negative value by `-1` is undefined in LLVM,
            // and the answer is `0` — which is also the remainder by `1`.
            let minus_one = t.ty().const_all_ones();
            let d = e.select(e.cmp(IntPredicate::EQ, b, minus_one)?, t.ty().const_int(1, false), b)?;
            let r = bld.build_int_signed_rem(a, d, "rem").map_err(|x| e.err(x))?;
            if method == "rem" {
                Ok(r)
            } else {
                // Floored: a nonzero remainder whose sign differs from the
                // divisor's moves by one divisor.
                let differs = e.cmp(IntPredicate::SLT, bld.build_xor(r, b, "signs").map_err(|x| e.err(x))?, zero)?;
                let nonzero = e.cmp(IntPredicate::NE, r, zero)?;
                let adjust = bld.build_and(differs, nonzero, "adjust").map_err(|x| e.err(x))?;
                let moved = bld.build_int_add(r, b, "moved").map_err(|x| e.err(x))?;
                Ok(e.select(adjust, moved, r)?)
            }
        }
        _ => return Err(e.err(format!("not a fixed-width binary method: {}", method))),
    }
    .map_err(|x| e.err(x))?;
    Ok(llvm_value_value(r.into()))
}

/// `ash` and `logbitp` (`typelisp_mem::fixed::ash`/`logbitp`): a distance
/// of the type's width or more saturates, which a bare `shl`/`ashr` cannot
/// say (they are undefined there), so the in-range shift is computed on a
/// safe distance and the saturated value chosen by `select`.
fn shift(e: &Emit, t: Fixed, method: &str, a: IntValue<'static>, count: IntValue<'static>) -> Result<IntValue<'static>, EvalError> {
    let wt = count.get_type();
    let zero_w = wt.const_zero();
    let width = wt.const_int(u64::from(t.bits), false);
    // A distance below the type's width fits the type.
    let in_t = |n: IntValue<'static>| e.b.build_int_cast_sign_flag(n, t.ty(), false, "distance").map_err(|x| e.err(x));
    let zero = t.ty().const_zero();
    let sign = if t.signed { e.cmp(IntPredicate::SLT, a, zero)? } else { crate::compile::llvm_context().bool_type().const_zero() };
    if method == "logbitp" {
        // A negative index, read unsigned, is past the width too.
        let past = e.cmp(IntPredicate::UGE, count, width)?;
        let at = in_t(e.select(past, zero_w, count)?)?;
        let shifted = e.b.build_right_shift(a, at, false, "bit").map_err(|x| e.err(x))?;
        let bit = e.b.build_and(shifted, t.ty().const_int(1, false), "bit").map_err(|x| e.err(x))?;
        let set = e.cmp(IntPredicate::NE, bit, zero)?;
        return e.select(past, sign, set);
    }
    let left = e.cmp(IntPredicate::SGE, count, zero_w)?;
    let negated = e.b.build_int_sub(zero_w, count, "neg").map_err(|x| e.err(x))?;
    let distance = e.select(left, count, negated)?;
    // `-i64::MIN` wraps to itself, which read unsigned is past every width.
    let past = e.cmp(IntPredicate::UGE, distance, width)?;
    let by = in_t(e.select(past, zero_w, distance)?)?;
    let shl = e.b.build_left_shift(a, by, "shl").map_err(|x| e.err(x))?;
    let shr = e.b.build_right_shift(a, by, t.signed, "shr").map_err(|x| e.err(x))?;
    let all = t.ty().const_all_ones();
    let filled = e.select(sign, all, zero)?;
    let l = e.select(past, zero, shl)?;
    let r = e.select(past, filled, shr)?;
    e.select(left, l, r)
}

/// `(build-fixed-unary builder module wsig method a)`: a one-operand method
/// of a fixed-width integer — `lognot`, `logcount`, `integer-length`,
/// `int->float`, and the width casts. `int->W` answers the cast value;
/// `try-int->W` answers whether `a` fits `W`, an `i1`.
pub(super) fn build_fixed_unary(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let t = Fixed::of(expect_int(&args[2])?)?;
    let method = expect_str(heap, &args[3])?.to_string();
    let a = value_or_return!(&args[4]).into_int_value();
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-fixed-unary" };
    let zero = t.ty().const_zero();
    if let Some(to) = Fixed::target_of(&method) {
        let cast = e.cast(t, to, a)?;
        if method.starts_with("try-") {
            // It fits when it comes back the same and keeps its sign.
            let kept = e.cmp(IntPredicate::EQ, e.cast(to, t, cast)?, a)?;
            let negative = |t: Fixed, v: IntValue<'static>| match t.signed {
                true => e.cmp(IntPredicate::SLT, v, t.ty().const_zero()),
                false => Ok(crate::compile::llvm_context().bool_type().const_zero()),
            };
            let same_sign = e.cmp(IntPredicate::EQ, negative(t, a)?, negative(to, cast)?)?;
            let fits = bld.build_and(kept, same_sign, "fits").map_err(|x| e.err(x))?;
            return Ok(llvm_value_value(fits.into()));
        }
        return Ok(llvm_value_value(cast.into()));
    }
    // `logcount`/`integer-length` read a negative value through its
    // complement (`typelisp_mem::fixed`).
    let magnitude = || -> Result<IntValue<'static>, EvalError> {
        let not = bld.build_not(a, "not").map_err(|x| e.err(x))?;
        if t.signed { e.select(e.cmp(IntPredicate::SLT, a, zero)?, not, a) } else { Ok(a) }
    };
    let r = match method.as_str() {
        "lognot" => bld.build_not(a, "lognot").map_err(|x| e.err(x))?,
        "logcount" => e.intrinsic(&module.borrow(), "llvm.ctpop", &[t.ty().into()], &[magnitude()?.into()])?.into_int_value(),
        "integer-length" => {
            let poison = crate::compile::llvm_context().bool_type().const_zero();
            let lz = e.intrinsic(&module.borrow(), "llvm.ctlz", &[t.ty().into()], &[magnitude()?.into(), poison.into()])?.into_int_value();
            bld.build_int_sub(t.ty().const_int(u64::from(t.bits), false), lz, "length").map_err(|x| e.err(x))?
        }
        "int->float" => {
            let ctx = crate::compile::llvm_context();
            let f = if t.signed { bld.build_signed_int_to_float(a, ctx.f64_type(), "float") } else { bld.build_unsigned_int_to_float(a, ctx.f64_type(), "float") }
                .map_err(|x| e.err(x))?;
            return Ok(llvm_value_value(f.into()));
        }
        _ => return Err(e.err(format!("not a fixed-width unary method: {}", method))),
    };
    Ok(llvm_value_value(r.into()))
}

/// `(build-fixed-div-check builder wsig method a b)`: whether `/`, `mod` or
/// `rem` fails on these operands, an `i1` — a zero divisor, and
/// for `/` also the most negative value over `-1`
/// (`typelisp_mem::fixed::Fault`). The island takes the failing side to
/// the runtime's `rt_int_div_fault`/`rt_int_mod_fault`.
pub(super) fn build_fixed_div_check(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let t = Fixed::of(expect_int(&args[1])?)?;
    let method = expect_str(heap, &args[2])?.to_string();
    let (a, b) = (value_or_return!(&args[3]).into_int_value(), value_or_return!(&args[4]).into_int_value());
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-fixed-div-check" };
    let zero = e.cmp(IntPredicate::EQ, b, t.ty().const_zero())?;
    let fails = if method == "/" && t.signed {
        let min = t.ty().const_int(1u64 << (t.bits - 1), false);
        let is_min = e.cmp(IntPredicate::EQ, a, min)?;
        let minus_one = e.cmp(IntPredicate::EQ, b, t.ty().const_all_ones())?;
        let overflow = bld.build_and(is_min, minus_one, "overflow").map_err(|x| e.err(x))?;
        bld.build_or(zero, overflow, "fails").map_err(|x| e.err(x))?
    } else {
        zero
    };
    Ok(llvm_value_value(fails.into()))
}

/// A float width as the island names it, in bits.
fn float_ty(bits: i64) -> Result<FloatType<'static>, EvalError> {
    let ctx = crate::compile::llvm_context();
    match bits {
        32 => Ok(ctx.f32_type()),
        64 => Ok(ctx.f64_type()),
        _ => Err(EvalError::Internal(format!("not a float width: {}", bits))),
    }
}

/// `(build-float-binop builder module bits method a b)`: a two-operand
/// method of `f32` (`bits` 32) or `f64` (64) — arithmetic, `expt`,
/// `max`/`min`, comparisons. A comparison answers an `i1`: the
/// ordered predicates, so a `NaN` operand makes `<` and `=` false, and `/=`
/// unordered, so it makes `/=` true (Rust's `!=`).
pub(super) fn build_float_binop(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let ty = float_ty(expect_int(&args[2])?)?;
    let method = expect_str(heap, &args[3])?.to_string();
    let (a, b) = (value_or_return!(&args[4]).into_float_value(), value_or_return!(&args[5]).into_float_value());
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-float-binop" };
    let fcmp = |p: FloatPredicate, x: FloatValue<'static>, y: FloatValue<'static>| bld.build_float_compare(p, x, y, "fcmp").map_err(|z| e.err(z));
    let predicate = match method.as_str() {
        "<" => Some(FloatPredicate::OLT),
        "<=" => Some(FloatPredicate::OLE),
        ">" => Some(FloatPredicate::OGT),
        ">=" => Some(FloatPredicate::OGE),
        "=" | "eq" | "eql" | "equal" | "equalp" => Some(FloatPredicate::OEQ),
        "/=" => Some(FloatPredicate::UNE),
        _ => None,
    };
    if let Some(p) = predicate {
        return Ok(llvm_value_value(fcmp(p, a, b)?.into()));
    }
    let r = match method.as_str() {
        "+" => bld.build_float_add(a, b, "fadd").map_err(|x| e.err(x))?,
        "-" => bld.build_float_sub(a, b, "fsub").map_err(|x| e.err(x))?,
        "*" => bld.build_float_mul(a, b, "fmul").map_err(|x| e.err(x))?,
        "/" => bld.build_float_div(a, b, "fdiv").map_err(|x| e.err(x))?,
        "expt" => e.intrinsic(&module.borrow(), "llvm.pow", &[ty.into()], &[a.into(), b.into()])?.into_float_value(),
        // `typelisp_mem::float::max`/`min`: the first operand unless the
        // second is strictly greater (smaller).
        "max" | "min" => {
            let p = if method == "max" { FloatPredicate::OGT } else { FloatPredicate::OLT };
            bld.build_select(fcmp(p, b, a)?, b, a, "select").map_err(|x| e.err(x))?.into_float_value()
        }
        _ => return Err(e.err(format!("not a float binary method: {}", method))),
    };
    Ok(llvm_value_value(r.into()))
}

/// `(build-float-unary builder module bits method a)`: a one-operand method
/// of `f32` or `f64` with an LLVM intrinsic, and the width casts.
/// `float->f32` rounds to binary32; `float->f64` is exact; `try-float->f32`
/// answers whether rounding to binary32 keeps the value's bits, an `i1`.
///
/// `round` is `llvm.rint` in the default rounding mode (nearest, ties to
/// even, CL's `round`) rather than `llvm.roundeven`, which some targets
/// lower to a `roundeven` libcall that macOS's libm does not have.
pub(super) fn build_float_unary(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let ty = float_ty(expect_int(&args[2])?)?;
    let method = expect_str(heap, &args[3])?.to_string();
    let a = value_or_return!(&args[4]).into_float_value();
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-float-unary" };
    let ctx = crate::compile::llvm_context();
    let to = |ty: FloatType<'static>, v: FloatValue<'static>| bld.build_float_cast(v, ty, "cast").map_err(|x| e.err(x));
    match method.as_str() {
        "float->f32" => return Ok(llvm_value_value(to(ctx.f32_type(), a)?.into())),
        "float->f64" => return Ok(llvm_value_value(to(ctx.f64_type(), a)?.into())),
        "try-float->f32" => {
            let wide = to(ctx.f64_type(), a)?;
            let back = to(ctx.f64_type(), to(ctx.f32_type(), wide)?)?;
            let bits = |v: FloatValue<'static>| bld.build_bit_cast(v, ctx.i64_type(), "bits").map_err(|x| e.err(x));
            let kept = e.cmp(IntPredicate::EQ, bits(back)?.into_int_value(), bits(wide)?.into_int_value())?;
            return Ok(llvm_value_value(kept.into()));
        }
        _ => {}
    }
    let intrinsic = match method.as_str() {
        "sqrt" => "llvm.sqrt",
        "floor" => "llvm.floor",
        "ceiling" => "llvm.ceil",
        "round" => "llvm.rint",
        "truncate" => "llvm.trunc",
        "sin" => "llvm.sin",
        "cos" => "llvm.cos",
        "exp" => "llvm.exp",
        "log" => "llvm.log",
        _ => return Err(e.err(format!("not a float unary method: {}", method))),
    };
    let r = e.intrinsic(&module.borrow(), intrinsic, &[ty.into()], &[a.into()])?.into_float_value();
    Ok(llvm_value_value(r.into()))
}
