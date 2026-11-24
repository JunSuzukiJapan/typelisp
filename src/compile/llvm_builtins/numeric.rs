//! The island's numeric builders: one operation on a fixed-width integer or
//! a float, computed at the type's own width.
//!
//! A value travels through compiled code in a 64-bit word: a fixed-width
//! integer sign-extended from its width when signed and zero-extended when
//! unsigned, a float as the bits of a `double` (an `f32` is exactly a
//! `double`). That is the *word*. An operation is computed in the type's own
//! LLVM type — `i8` for `u8`, `float` for `f32` — so every builder here does
//! three things: cut the word down to that type, compute, and extend the
//! result back. Between two operations of one expression LLVM folds the
//! extension and the cut away, so the expression stays in the narrow type.
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

use super::{expect_int, expect_llvm_builder, expect_llvm_module, expect_llvm_value, llvm_value_value};
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

    fn word_ty(&self) -> IntType<'static> {
        crate::compile::carriers::word()
    }

    /// The word cut to `t`: its low bits are the value.
    fn narrow(&self, t: Fixed, word: IntValue<'static>) -> Result<IntValue<'static>, EvalError> {
        if t.bits == 64 {
            return Ok(word);
        }
        self.b.build_int_truncate(word, t.ty(), "narrow").map_err(|e| self.err(e))
    }

    /// A value of `t` back in its word.
    fn widen(&self, t: Fixed, v: IntValue<'static>) -> Result<IntValue<'static>, EvalError> {
        if t.bits == 64 {
            return Ok(v);
        }
        let r = if t.signed { self.b.build_int_s_extend(v, self.word_ty(), "widen") } else { self.b.build_int_z_extend(v, self.word_ty(), "widen") };
        r.map_err(|e| self.err(e))
    }

    /// An `i1` as the `0`/`1` word a `bool` is.
    fn flag(&self, v: IntValue<'static>) -> Result<IntValue<'static>, EvalError> {
        self.b.build_int_z_extend(v, self.word_ty(), "flag").map_err(|e| self.err(e))
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
/// divisions, `ash` and `logbitp`. A comparison answers a `0`/`1` word.
///
/// `/`, `mod` and `rem` assume their operands passed
/// `build-fixed-div-check`: the island branches to the runtime's fault on
/// the other side. `ash`'s and `logbitp`'s `b` is a bit count, a raw word
/// whatever the receiver's type.
pub(super) fn build_fixed_binop(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let t = Fixed::of(expect_int(&args[1])?)?;
    let method = expect_str(heap, &args[2])?.to_string();
    let (aw, bw) = (expect_llvm_value(&args[3])?.into_int_value(), expect_llvm_value(&args[4])?.into_int_value());
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-fixed-binop" };
    let a = e.narrow(t, aw)?;
    if method == "ash" || method == "logbitp" {
        return Ok(llvm_value_value(shift(&e, t, &method, a, bw)?.into()));
    }
    let b = e.narrow(t, bw)?;
    if let Some(p) = int_predicate(&method, t) {
        return Ok(llvm_value_value(e.flag(e.cmp(p, a, b)?)?.into()));
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
            return Ok(llvm_value_value(e.flag(e.cmp(IntPredicate::NE, both, zero)?)?.into()));
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
    Ok(llvm_value_value(e.widen(t, r)?.into()))
}

/// `ash` and `logbitp` (`typelisp_mem::fixed::ash`/`logbitp`): a distance
/// of the type's width or more saturates, which a bare `shl`/`ashr` cannot
/// say (they are undefined there), so the in-range shift is computed on a
/// safe distance and the saturated value chosen by `select`.
fn shift(e: &Emit, t: Fixed, method: &str, a: IntValue<'static>, count: IntValue<'static>) -> Result<IntValue<'static>, EvalError> {
    let wt = e.word_ty();
    let zero_w = wt.const_zero();
    let width = wt.const_int(u64::from(t.bits), false);
    let zero = t.ty().const_zero();
    let sign = if t.signed { e.cmp(IntPredicate::SLT, a, zero)? } else { crate::compile::llvm_context().bool_type().const_zero() };
    if method == "logbitp" {
        // A negative index, read unsigned, is past the width too.
        let past = e.cmp(IntPredicate::UGE, count, width)?;
        let at = e.narrow(t, e.select(past, zero_w, count)?)?;
        let shifted = e.b.build_right_shift(a, at, false, "bit").map_err(|x| e.err(x))?;
        let bit = e.b.build_and(shifted, t.ty().const_int(1, false), "bit").map_err(|x| e.err(x))?;
        let set = e.cmp(IntPredicate::NE, bit, zero)?;
        let r = e.b.build_select(past, sign, set, "logbitp").map_err(|x| e.err(x))?.into_int_value();
        return e.flag(r);
    }
    let left = e.cmp(IntPredicate::SGE, count, zero_w)?;
    let negated = e.b.build_int_sub(zero_w, count, "neg").map_err(|x| e.err(x))?;
    let distance = e.select(left, count, negated)?;
    // `-i64::MIN` wraps to itself, which read unsigned is past every width.
    let past = e.cmp(IntPredicate::UGE, distance, width)?;
    let by = e.narrow(t, e.select(past, zero_w, distance)?)?;
    let shl = e.b.build_left_shift(a, by, "shl").map_err(|x| e.err(x))?;
    let shr = e.b.build_right_shift(a, by, t.signed, "shr").map_err(|x| e.err(x))?;
    let all = t.ty().const_all_ones();
    let filled = e.select(sign, all, zero)?;
    let l = e.select(past, zero, shl)?;
    let r = e.select(past, filled, shr)?;
    e.widen(t, e.select(left, l, r)?)
}

/// `(build-fixed-unary builder module wsig method a)`: a one-operand method
/// of a fixed-width integer — `lognot`, `logcount`, `integer-length`,
/// `int->float`, and the width casts. `int->W` answers the cast value;
/// `try-int->W` answers whether `a` fits `W`, a `0`/`1` word (a value that
/// fits is the same word in both types, so the island's `Some` holds `a`).
pub(super) fn build_fixed_unary(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let t = Fixed::of(expect_int(&args[2])?)?;
    let method = expect_str(heap, &args[3])?.to_string();
    let word = expect_llvm_value(&args[4])?.into_int_value();
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-fixed-unary" };
    if let Some(to) = Fixed::target_of(&method) {
        let cast = e.widen(to, e.narrow(to, word)?)?;
        if method.starts_with("try-") {
            let kept = e.cmp(IntPredicate::EQ, cast, word)?;
            // A word below zero is a negative value of a signed type and,
            // of `c-ulong`, one past `c-long`'s maximum: either way it is out
            // of range for a type of the other signedness.
            let fits = if t.signed != to.signed {
                let non_negative = e.cmp(IntPredicate::SGE, word, e.word_ty().const_zero())?;
                bld.build_and(kept, non_negative, "fits").map_err(|x| e.err(x))?
            } else {
                kept
            };
            return Ok(llvm_value_value(e.flag(fits)?.into()));
        }
        return Ok(llvm_value_value(cast.into()));
    }
    let a = e.narrow(t, word)?;
    let zero = t.ty().const_zero();
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
            let bits = bld.build_bit_cast(f, ctx.i64_type(), "bits").map_err(|x| e.err(x))?;
            return Ok(llvm_value_value(bits));
        }
        _ => return Err(e.err(format!("not a fixed-width unary method: {}", method))),
    };
    Ok(llvm_value_value(e.widen(t, r)?.into()))
}

/// `(build-fixed-div-check builder wsig method a b)`: whether `/`, `mod` or
/// `rem` fails on these operands, as a `0`/`1` word — a zero divisor, and
/// for `/` also the most negative value over `-1`
/// (`typelisp_mem::fixed::Fault`). The island takes the failing side to
/// the runtime's `rt_int_div_fault`/`rt_int_mod_fault`.
pub(super) fn build_fixed_div_check(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let t = Fixed::of(expect_int(&args[1])?)?;
    let method = expect_str(heap, &args[2])?.to_string();
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-fixed-div-check" };
    let a = e.narrow(t, expect_llvm_value(&args[3])?.into_int_value())?;
    let b = e.narrow(t, expect_llvm_value(&args[4])?.into_int_value())?;
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
    Ok(llvm_value_value(e.flag(fails)?.into()))
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

/// A float's word as a value of `ty`: the `double` it holds, rounded to
/// `float` for an `f32` (exactly, since the word holds an `f32`'s value).
fn float_word(e: &Emit, word: IntValue<'static>, ty: FloatType<'static>) -> Result<FloatValue<'static>, EvalError> {
    let ctx = crate::compile::llvm_context();
    let d = e.b.build_bit_cast(word, ctx.f64_type(), "double").map_err(|x| e.err(x))?.into_float_value();
    if ty == ctx.f64_type() {
        return Ok(d);
    }
    e.b.build_float_trunc(d, ty, "narrow").map_err(|x| e.err(x))
}

/// A float back in its word: as a `double`'s bits.
fn float_back(e: &Emit, v: FloatValue<'static>) -> Result<IntValue<'static>, EvalError> {
    let ctx = crate::compile::llvm_context();
    let d = if v.get_type() == ctx.f64_type() { v } else { e.b.build_float_ext(v, ctx.f64_type(), "widen").map_err(|x| e.err(x))? };
    Ok(e.b.build_bit_cast(d, ctx.i64_type(), "bits").map_err(|x| e.err(x))?.into_int_value())
}

/// `(build-float-binop builder module bits method a b)`: a two-operand
/// method of `f32` (`bits` 32) or `f64` (64) — arithmetic, `expt`,
/// `max`/`min`, comparisons. A comparison answers a `0`/`1` word: the
/// ordered predicates, so a `NaN` operand makes `<` and `=` false, and `/=`
/// unordered, so it makes `/=` true (Rust's `!=`).
pub(super) fn build_float_binop(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let ty = float_ty(expect_int(&args[2])?)?;
    let method = expect_str(heap, &args[3])?.to_string();
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-float-binop" };
    let a = float_word(&e, expect_llvm_value(&args[4])?.into_int_value(), ty)?;
    let b = float_word(&e, expect_llvm_value(&args[5])?.into_int_value(), ty)?;
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
        return Ok(llvm_value_value(e.flag(fcmp(p, a, b)?)?.into()));
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
    Ok(llvm_value_value(float_back(&e, r)?.into()))
}

/// `(build-float-unary builder module bits method a)`: a one-operand method
/// of `f32` or `f64` with an LLVM intrinsic, and the width casts.
/// `float->f32` rounds to binary32; `float->f64` is exact; `try-float->f32`
/// answers whether rounding to binary32 keeps the value's bits, a `0`/`1`
/// word.
///
/// `round` is `llvm.rint` in the default rounding mode (nearest, ties to
/// even, CL's `round`) rather than `llvm.roundeven`, which some targets
/// lower to a `roundeven` libcall that macOS's libm does not have.
pub(super) fn build_float_unary(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let ty = float_ty(expect_int(&args[2])?)?;
    let method = expect_str(heap, &args[3])?.to_string();
    let word = expect_llvm_value(&args[4])?.into_int_value();
    let bld = builder.borrow();
    let e = Emit { b: &bld, who: "build-float-unary" };
    let ctx = crate::compile::llvm_context();
    match method.as_str() {
        "float->f32" => {
            let single = float_word(&e, word, ctx.f32_type())?;
            return Ok(llvm_value_value(float_back(&e, single)?.into()));
        }
        "float->f64" => return Ok(llvm_value_value(word.into())),
        "try-float->f32" => {
            let single = float_word(&e, word, ctx.f32_type())?;
            let back = float_back(&e, single)?;
            return Ok(llvm_value_value(e.flag(e.cmp(IntPredicate::EQ, back, word)?)?.into()));
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
    let a = float_word(&e, word, ty)?;
    let r = e.intrinsic(&module.borrow(), intrinsic, &[ty.into()], &[a.into()])?.into_float_value();
    Ok(llvm_value_value(float_back(&e, r)?.into()))
}
