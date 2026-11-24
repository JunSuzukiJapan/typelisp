//! The numeric built-in methods: fixed-width integers, floats, `int`, `ratio`
//! and `rational`.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, ToPrimitive, Zero};
use crate::{wk, Heap, SymRef, TypeId, Value};
use crate::eval::value::EvalError;
use typelisp_mem::fixed::{self, Fault, Fixed, FixedType};
use typelisp_mem::{with_fixed, IntArg};
use super::{expect_str, rt_i64};

/// Evaluate a built-in method of a fixed-width integer receiver of type `ty`
/// (`typeck::builtins::primitive::fixed_width`, and the C words' casts): the
/// instance of `typelisp_mem::fixed` for that type, which is what the method
/// means. `None` for a method the type has no built-in body for.
pub(super) fn eval_fixed_builtin(heap: &mut Heap, method: SymRef, args: &[Value], ty: FixedType, ret_type: TypeId) -> Option<Result<Value, EvalError>> {
    with_fixed!(ty, T => fixed_builtin::<T>(heap, method, args, ret_type).transpose())
}

fn fixed_builtin<T: Fixed>(heap: &mut Heap, method: SymRef, args: &[Value], ret_type: TypeId) -> Result<Option<Value>, EvalError> {
    let arg = |i: usize| rt_i64(&args[i]).map(T::from_word);
    let num = |v: T| Value::Int(v.to_word());
    let fault = |f: Fault| EvalError::Panic(f.message());
    Ok(Some(match method.well_known() {
        wk::M_OP_ADD => num(fixed::add(arg(0)?, arg(1)?)),
        wk::M_OP_SUB => num(fixed::sub(arg(0)?, arg(1)?)),
        wk::M_OP_MUL => num(fixed::mul(arg(0)?, arg(1)?)),
        wk::M_OP_DIV => num(fixed::div(arg(0)?, arg(1)?).map_err(fault)?),
        wk::M_MOD => num(fixed::modulo(arg(0)?, arg(1)?).map_err(fault)?),
        wk::M_REM => num(fixed::rem(arg(0)?, arg(1)?).map_err(fault)?),
        wk::M_MAX => num(fixed::max(arg(0)?, arg(1)?)),
        wk::M_MIN => num(fixed::min(arg(0)?, arg(1)?)),
        wk::M_LOGAND => num(fixed::logand(arg(0)?, arg(1)?)),
        wk::M_LOGIOR => num(fixed::logior(arg(0)?, arg(1)?)),
        wk::M_LOGXOR => num(fixed::logxor(arg(0)?, arg(1)?)),
        wk::M_LOGTEST => Value::Bool(fixed::logtest(arg(0)?, arg(1)?)),
        wk::M_OP_LT => Value::Bool(arg(0)? < arg(1)?),
        wk::M_OP_LE => Value::Bool(arg(0)? <= arg(1)?),
        wk::M_OP_GT => Value::Bool(arg(0)? > arg(1)?),
        wk::M_OP_GE => Value::Bool(arg(0)? >= arg(1)?),
        wk::RETIRED_EQUALS | wk::M_EQ | wk::M_EQL | wk::M_EQUAL | wk::M_EQUALP => Value::Bool(arg(0)? == arg(1)?),
        wk::M_OP_NE => Value::Bool(arg(0)? != arg(1)?),
        wk::M_ASH => num(fixed::ash(arg(0)?, bit_count(heap, &args[1])?)),
        wk::M_LOGBITP => Value::Bool(fixed::logbitp(arg(0)?, bit_count(heap, &args[1])?)),
        wk::M_LOGNOT => num(fixed::lognot(arg(0)?)),
        wk::M_LOGCOUNT => num(fixed::logcount(arg(0)?)),
        wk::M_INTEGER_LENGTH => num(fixed::integer_length(arg(0)?)),
        wk::M_INT_TO_FLOAT => heap.alloc_f64(fixed::to_float(arg(0)?)),
        wk::M_INT_TO_INT => heap.canonical_int(fixed::to_int(arg(0)?)),
        wk::M_INT_TO_RATIO => ratio_rt(heap, BigRational::from_integer(BigInt::from(fixed::to_int(arg(0)?)))),
        wk::M_INT_TO_CHAR => int_to_char(args)?,
        wk::M_TRY_INT_TO_CHAR => try_int_to_char(heap, args, ret_type)?,
        _ => match width_cast_target(method) {
            Some((false, to)) => with_fixed!(to, U => Value::Int(fixed::cast::<T, U>(arg(0)?).to_word())),
            Some((true, to)) => {
                let fits = with_fixed!(to, U => fixed::try_cast::<T, U>(arg(0)?).map(|u| Value::Int(u.to_word())));
                heap.alloc_option(ret_type, fits)
            }
            None => return Ok(None),
        },
    }))
}

/// A shift distance or a bit position: an `int`, which has to be a fixnum to
/// be either. A bignum is refused as `untag-int` refuses it.
fn bit_count(heap: &Heap, v: &Value) -> Result<i64, EvalError> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Boxed(id) if heap.is_bignum(*id) => Err(EvalError::Panic("an integer argument does not fit a fixnum".to_string())),
        other => Err(EvalError::Internal(format!("expected an int, got {:?}", other))),
    }
}

/// A float operand's number, whichever width labels it. An `f32` widens
/// exactly, so [`FloatWidth::of`] gets it back unchanged.
fn expect_float(heap: &Heap, v: &Value) -> Result<f64, EvalError> {
    match v {
        Value::Boxed(id) => match heap.float_box(*id) {
            Some(typelisp_mem::FloatBox::F32(f)) => Ok(f64::from(f)),
            Some(typelisp_mem::FloatBox::F64(f)) => Ok(f),
            None => Err(EvalError::Internal(format!("expected a float, got {:?}", v))),
        },
        other => Err(EvalError::Internal(format!("expected a float, got {:?}", other))),
    }
}

/// `int->char` (`registry::int_assoc`): a Unicode scalar value back to
/// `char`. Panics (same precedent as `car`/`cdr` on a non-`Cons` `Sexpr`) if
/// the value is outside the valid range — a surrogate code point or past
/// `U+10FFFF` — since the type system can't express "valid scalar value".
pub(super) fn int_to_char(args: &[Value]) -> Result<Value, EvalError> {
    match args.first() {
        Some(Value::Int(n)) => {
            let in_u32_range = *n >= 0 && *n <= i64::from(u32::MAX);
            in_u32_range
                .then_some(*n as u32)
                .and_then(char::from_u32)
                .map(Value::Char)
                .ok_or_else(|| EvalError::Panic(format!("int->char: {} is not a valid Unicode scalar value", n)))
        }
        other => Err(EvalError::Internal(format!("int->char: expected an integer, got {:?}", other))),
    }
}

/// `try-int->char` (`registry::int_assoc`): the `Option`-returning
/// counterpart of [`int_to_char`], for `typeck::cast`'s `try-as` —
/// same Unicode-scalar-value validity check, `None` instead of a panic.
pub(super) fn try_int_to_char(heap: &mut Heap, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    match args.first() {
        Some(Value::Int(n)) => {
            let in_u32_range = *n >= 0 && *n <= i64::from(u32::MAX);
            let c = in_u32_range.then_some(*n as u32).and_then(char::from_u32);
            Ok(heap.alloc_option(ret_type, c.map(Value::Char)))
        }
        other => Err(EvalError::Internal(format!("try-int->char: expected an integer, got {:?}", other))),
    }
}

/// One float width, as the Rust type that is it: an `f32` operation is
/// computed in `f32` and an `f64` one in `f64`, as compiled code computes
/// them.
trait FloatWidth: num_traits::Float {
    /// An operand of this width, read out of its box.
    fn of(v: f64) -> Self;
    fn boxed(self, heap: &mut Heap) -> Value;
    fn round_ties_even(self) -> Self;
}

impl FloatWidth for f32 {
    fn of(v: f64) -> Self {
        v as f32
    }
    fn boxed(self, heap: &mut Heap) -> Value {
        heap.alloc_f32(self)
    }
    fn round_ties_even(self) -> Self {
        f32::round_ties_even(self)
    }
}

impl FloatWidth for f64 {
    fn of(v: f64) -> Self {
        v
    }
    fn boxed(self, heap: &mut Heap) -> Value {
        heap.alloc_f64(self)
    }
    fn round_ties_even(self) -> Self {
        f64::round_ties_even(self)
    }
}

/// Evaluate a built-in arithmetic, comparison or transcendental method of a
/// float receiver (`typeck::builtins::primitive::float`), at the receiver's
/// width (`single` for `f32`). `/` never panics on a zero divisor — IEEE-754
/// division yields `inf`/`NaN` instead. `max`/`min` are
/// `typelisp_mem::float`'s. `mod`/`rem` are defined in `prelude.rs` as
/// typelisp methods (`a - b*floor|truncate(a/b)`).
pub(super) fn eval_float_builtin(heap: &mut Heap, method: SymRef, args: &[Value], single: bool) -> Option<Result<Value, EvalError>> {
    if single { float_builtin::<f32>(heap, method, args) } else { float_builtin::<f64>(heap, method, args) }.transpose()
}

fn float_builtin<F: FloatWidth>(heap: &mut Heap, method: SymRef, args: &[Value]) -> Result<Option<Value>, EvalError> {
    let a = F::of(expect_float(heap, &args[0])?);
    let b = match args.get(1) {
        Some(v) => Some(F::of(expect_float(heap, v)?)),
        None => None,
    };
    let b = || b.ok_or_else(|| EvalError::Internal(format!("{}: expected two floats", method)));
    let v = match method.well_known() {
        wk::M_OP_ADD => a + b()?,
        wk::M_OP_SUB => a - b()?,
        wk::M_OP_MUL => a * b()?,
        wk::M_OP_DIV => a / b()?,
        wk::M_MAX => typelisp_mem::float::max(a, b()?),
        wk::M_MIN => typelisp_mem::float::min(a, b()?),
        wk::M_EXPT => a.powf(b()?),
        wk::M_OP_LT => return Ok(Some(Value::Bool(a < b()?))),
        wk::M_OP_LE => return Ok(Some(Value::Bool(a <= b()?))),
        wk::M_OP_GT => return Ok(Some(Value::Bool(a > b()?))),
        wk::M_OP_GE => return Ok(Some(Value::Bool(a >= b()?))),
        wk::RETIRED_EQUALS | wk::M_EQ | wk::M_EQL | wk::M_EQUAL | wk::M_EQUALP => return Ok(Some(Value::Bool(a == b()?))),
        wk::M_OP_NE => return Ok(Some(Value::Bool(a != b()?))),
        wk::M_SQRT => a.sqrt(),
        wk::M_FLOOR => a.floor(),
        wk::M_CEILING => a.ceil(),
        wk::M_ROUND => FloatWidth::round_ties_even(a),
        wk::M_TRUNCATE => a.trunc(),
        wk::M_SIN => a.sin(),
        wk::M_COS => a.cos(),
        wk::M_TAN => a.tan(),
        wk::M_ASIN => a.asin(),
        wk::M_ACOS => a.acos(),
        wk::M_ATAN => a.atan(),
        wk::M_SINH => a.sinh(),
        wk::M_COSH => a.cosh(),
        wk::M_TANH => a.tanh(),
        wk::M_ASINH => a.asinh(),
        wk::M_ACOSH => a.acosh(),
        wk::M_ATANH => a.atanh(),
        wk::M_EXP => a.exp(),
        wk::M_LOG => a.ln(),
        _ => return Ok(None),
    };
    Ok(Some(v.boxed(heap)))
}

/// `v` boxed at the receiver's precision: rounded to binary32 first when the
/// receiver is `f32`. The box is the one the type names, so that everything
/// downstream (printing, `eql`, a dump) can still tell which width it is.
fn float_at(heap: &mut Heap, v: f64, single: bool) -> Value {
    if single {
        heap.alloc_f32(v as f32)
    } else {
        heap.alloc_f64(v)
    }
}

/// `float->f32`/`float->f64` (`registry::float_assoc`): a cast between the
/// two float widths. Narrowing rounds to nearest binary32; widening is exact,
/// since the value in the box is already a binary32 one.
pub(super) fn float_to_width(heap: &mut Heap, args: &[Value], single: bool) -> Result<Value, EvalError> {
    let v = expect_float(heap, &args[0])?;
    Ok(float_at(heap, v, single))
}

/// `try-float->f32` (`registry::float_assoc`): [`float_to_width`] asked as a
/// question — `None` when rounding to binary32 would lose something. NaN
/// converts (every NaN is a NaN in either width), which is why the test is
/// written as a round trip rather than an equality.
pub(super) fn try_float_to_f32(heap: &mut Heap, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    let v = expect_float(heap, &args[0])?;
    let exact = f64::from(v as f32).to_bits() == v.to_bits();
    let narrowed = if exact { Some(heap.alloc_f32(v as f32)) } else { None };
    Ok(heap.alloc_option(ret_type, narrowed))
}

/// Whether `method` is a `try-int->W` cast rather than an `int->W` one, and
/// `W` — `None` for any other method. The targets are the ones
/// `typeck::builtins::primitive::width_casts` gives these casts.
pub(super) fn width_cast_target(method: SymRef) -> Option<(bool, FixedType)> {
    Some(match method.well_known() {
        wk::M_INT_TO_I8 => (false, FixedType::I8),
        wk::M_INT_TO_I16 => (false, FixedType::I16),
        wk::M_INT_TO_I32 => (false, FixedType::I32),
        wk::M_INT_TO_U8 => (false, FixedType::U8),
        wk::M_INT_TO_U16 => (false, FixedType::U16),
        wk::M_INT_TO_U32 => (false, FixedType::U32),
        wk::M_INT_TO_C_LONG => (false, FixedType::I64),
        wk::M_INT_TO_C_ULONG => (false, FixedType::U64),
        wk::M_TRY_INT_TO_I8 => (true, FixedType::I8),
        wk::M_TRY_INT_TO_I16 => (true, FixedType::I16),
        wk::M_TRY_INT_TO_I32 => (true, FixedType::I32),
        wk::M_TRY_INT_TO_U8 => (true, FixedType::U8),
        wk::M_TRY_INT_TO_U16 => (true, FixedType::U16),
        wk::M_TRY_INT_TO_U32 => (true, FixedType::U32),
        _ => return None,
    })
}

/// The `BigRational` behind a `ratio` value — see [`expect_bignum`] for why
/// this is owned rather than borrowed.
fn expect_ratio(heap: &Heap, v: &Value) -> Result<BigRational, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_ratio(*id) => Ok(heap.ratio_value(*id).clone()),
        other => Err(EvalError::Internal(format!("expected a ratio, got {:?}", other))),
    }
}

/// A `ratio` value — the [`bignum_rt`] counterpart.
fn ratio_rt(heap: &mut Heap, r: BigRational) -> Value {
    heap.alloc_ratio(r)
}

/// A `string` value: `s` allocated onto the GC heap. The one constructor.
///
/// `Heap::alloc_string` deliberately does *not* dedupe, so two separately
/// evaluated literals with equal content get distinct `StrId`s — which is
/// what keeps `eq` a genuine identity test rather than collapsing to content
/// comparison. See [`string_identity_eq`].
pub fn str_rt(heap: &mut Heap, s: impl Into<String>) -> Value {
    heap.alloc_string(s.into())
}

// ---- `int`: fixnum ∪ bignum ------------------------------------------------

/// The `int` behind a value, in either of its two shapes — and nothing else:
/// a `Value::Int` past the fixnum range or a bignum box holding one inside it
/// is a producer that skipped `Heap::canonical_int`, reported as the internal
/// error it is rather than computed with.
pub(super) fn expect_int(heap: &Heap, v: &Value) -> Result<IntArg, EvalError> {
    match v {
        Value::Int(n) if typelisp_mem::fixnum_fits(*n) => Ok(IntArg::Fix(*n)),
        Value::Int(n) => Err(EvalError::Internal(format!("an int fixnum out of range: {}", n))),
        Value::Boxed(id) if heap.is_bignum(*id) => {
            if heap.bignum_fits_fixnum(*id) {
                return Err(EvalError::Internal(format!("an int boxed as a bignum that fits a fixnum: {}", heap.bignum_value(*id))));
            }
            Ok(IntArg::Big(heap.bignum_value(*id).clone()))
        }
        other => Err(EvalError::Internal(format!("expected an int, got {:?}", other))),
    }
}

/// An `int` result: `Heap::int_from_bigint`, the one constructor, so a result
/// that fits a fixnum is one.
fn int_rt(heap: &mut Heap, n: BigInt) -> Value {
    heap.int_from_bigint(n)
}

/// Evaluate a built-in `int` arithmetic/comparison instance method
/// (`registry::integer_assoc`). CL's `integer`: `+ - *` never wrap — two
/// fixnums compute in `i128` and the result is boxed only if it outgrew
/// 63 bits (`Heap::canonical_int`); anything involving a bignum computes as
/// one and demotes when it fits. `truncate` (the integer quotient) truncates
/// toward zero and `mod` floors, both panicking on a zero divisor like every
/// other numeric type here; the fixed-width types' `MIN / -1` overflow simply
/// promotes. `/` is CL's, a `rational` (`eval_rational_builtin`).
pub(super) fn eval_integer_builtin(heap: &mut Heap, name: SymRef, args: &[Value]) -> Option<Result<Value, EvalError>> {
    use num_integer::Integer;
    if name.is(wk::M_ASH) || name.is(wk::M_LOGBITP) {
        return Some(eval_integer_shift(heap, name, args));
    }
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => match (expect_int(heap, a), expect_int(heap, b)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Some(Err(e)),
        },
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two ints", name)))),
    };
    if let (IntArg::Fix(x), IntArg::Fix(y)) = (&a, &b) {
        let (x, y) = (*x, *y);
        let wide = |heap: &mut Heap, n: i128| heap.canonical_int(n);
        let v = match name.well_known() {
            wk::M_OP_ADD => wide(heap, i128::from(x) + i128::from(y)),
            wk::M_OP_SUB => wide(heap, i128::from(x) - i128::from(y)),
            wk::M_OP_MUL => wide(heap, i128::from(x) * i128::from(y)),
            wk::M_TRUNCATE => {
                if y == 0 {
                    return Some(Err(EvalError::Panic("divide by zero".into())));
                }
                wide(heap, i128::from(x) / i128::from(y))
            }
            wk::M_MOD => {
                if y == 0 {
                    return Some(Err(EvalError::Panic("mod by zero".into())));
                }
                // Floored (CL `mod`, the sign of the divisor), in `i128` so
                // that `MIN % -1` is an ordinary `0` rather than an overflow.
                let (x, y) = (i128::from(x), i128::from(y));
                let r = x % y;
                wide(heap, if r != 0 && ((r < 0) != (y < 0)) { r + y } else { r })
            }
            wk::M_OP_LT => Value::Bool(x < y),
            wk::M_OP_LE => Value::Bool(x <= y),
            wk::M_OP_GT => Value::Bool(x > y),
            wk::M_OP_GE => Value::Bool(x >= y),
            wk::RETIRED_EQUALS => Value::Bool(x == y),
            wk::M_OP_NE => Value::Bool(x != y),
            wk::M_MAX => Value::Int(x.max(y)),
            wk::M_MIN => Value::Int(x.min(y)),
            wk::M_LOGAND => Value::Int(x & y),
            wk::M_LOGIOR => Value::Int(x | y),
            wk::M_LOGXOR => Value::Int(x ^ y),
            wk::M_LOGTEST => Value::Bool(x & y != 0),
            _ => unreachable!("eval_integer_builtin: {}", name),
        };
        return Some(Ok(v));
    }
    let (a, b) = (a.into_big(), b.into_big());
    let v = match name.well_known() {
        wk::M_OP_ADD => int_rt(heap, &a + &b),
        wk::M_OP_SUB => int_rt(heap, &a - &b),
        wk::M_OP_MUL => int_rt(heap, &a * &b),
        wk::M_TRUNCATE => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            int_rt(heap, &a / &b)
        }
        wk::M_MOD => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("mod by zero".into())));
            }
            int_rt(heap, a.mod_floor(&b))
        }
        wk::M_OP_LT => Value::Bool(a < b),
        wk::M_OP_LE => Value::Bool(a <= b),
        wk::M_OP_GT => Value::Bool(a > b),
        wk::M_OP_GE => Value::Bool(a >= b),
        wk::RETIRED_EQUALS => Value::Bool(a == b),
        wk::M_OP_NE => Value::Bool(a != b),
        wk::M_MAX => int_rt(heap, if a >= b { a } else { b }),
        wk::M_MIN => int_rt(heap, if a <= b { a } else { b }),
        wk::M_LOGAND => int_rt(heap, &a & &b),
        wk::M_LOGIOR => int_rt(heap, &a | &b),
        wk::M_LOGXOR => int_rt(heap, &a ^ &b),
        wk::M_LOGTEST => Value::Bool(!(&a & &b).is_zero()),
        _ => unreachable!("eval_integer_builtin: {}", name),
    };
    Some(Ok(v))
}

/// `int`'s `ash`/`logbitp` — [`eval_bignum_shift`] over either shape, with
/// the result demoted when it fits.
fn eval_integer_shift(heap: &mut Heap, name: SymRef, args: &[Value]) -> Result<Value, EvalError> {
    let n = match args.first() {
        Some(v) => expect_int(heap, v)?.into_big(),
        None => return Err(EvalError::Internal(format!("{}: expected an int receiver", name))),
    };
    let count = match args.get(1) {
        Some(v) => rt_i64(v)?,
        None => return Err(EvalError::Internal(format!("{}: expected a bit count", name))),
    };
    Ok(match name.well_known() {
        wk::M_ASH => {
            if count >= 0 {
                int_rt(heap, n << (count as u64))
            } else {
                int_rt(heap, n >> ((-count) as u64))
            }
        }
        wk::M_LOGBITP => Value::Bool(if count < 0 {
            n.sign() == num_bigint::Sign::Minus
        } else {
            ((n >> (count as u64)) & BigInt::from(1)) == BigInt::from(1)
        }),
        _ => unreachable!("eval_integer_shift: {}", name),
    })
}

/// Unary `int` builtins (`lognot`/`logcount`/`integer-length`) — the
/// "infinite two's complement" reading [`bignum_unary`] gives, on either
/// shape.
pub(super) fn integer_unary(heap: &mut Heap, args: &[Value], name: SymRef) -> Result<Value, EvalError> {
    let a = expect_int(heap, &args[0])?.into_big();
    let v = match name.well_known() {
        wk::M_LOGNOT => int_rt(heap, !&a),
        wk::M_LOGCOUNT => {
            let n = if a.sign() == num_bigint::Sign::Minus { !&a } else { a };
            let count: u64 = n.magnitude().to_u32_digits().iter().map(|d| d.count_ones() as u64).sum();
            int_rt(heap, BigInt::from(count))
        }
        wk::M_INTEGER_LENGTH => {
            let bits = if a.sign() == num_bigint::Sign::Minus { (-(&a) - 1u32).magnitude().bits() } else { a.magnitude().bits() };
            int_rt(heap, BigInt::from(bits))
        }
        _ => unreachable!("integer_unary: {}", name),
    };
    Ok(v)
}

/// `int->W`/`try-int->W` on an `int` receiver: `typelisp_mem::fixed`'s
/// `from_int` and `try_from_int` at the target `to`.
pub(super) fn integer_to_width(heap: &mut Heap, args: &[Value], to: FixedType, try_variant: bool, ret_type: TypeId) -> Result<Value, EvalError> {
    let a = expect_int(heap, &args[0])?;
    if !try_variant {
        return Ok(with_fixed!(to, U => Value::Int(fixed::from_int::<U>(a).to_word())));
    }
    let fits = with_fixed!(to, U => fixed::try_from_int::<U>(&a).map(|u| Value::Int(u.to_word())));
    Ok(heap.alloc_option(ret_type, fits))
}

/// `int->float`/`int->ratio`/`int->char`/`try-int->char` on an `int`
/// receiver.
pub(super) fn integer_conversion(heap: &mut Heap, method: SymRef, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    let a = expect_int(heap, &args[0])?;
    match method.well_known() {
        wk::M_INT_TO_FLOAT => {
            let f = match &a {
                IntArg::Fix(n) => *n as f64,
                IntArg::Big(n) => n.to_f64().unwrap_or(f64::INFINITY.copysign(if n.sign() == num_bigint::Sign::Minus { -1.0 } else { 1.0 })),
            };
            Ok(heap.alloc_f64(f))
        }
        wk::M_INT_TO_RATIO => Ok(ratio_rt(heap, BigRational::from_integer(a.into_big()))),
        wk::M_INT_TO_CHAR | wk::M_TRY_INT_TO_CHAR => {
            let c = match &a {
                IntArg::Fix(n) if *n >= 0 && *n <= i64::from(u32::MAX) => char::from_u32(*n as u32),
                _ => None,
            };
            if method.is(wk::M_INT_TO_CHAR) {
                c.map(Value::Char).ok_or_else(|| EvalError::Panic(format!("int->char: {} is not a valid Unicode scalar value", a.into_big())))
            } else {
                Ok(heap.alloc_option(ret_type, c.map(Value::Char)))
            }
        }
        wk::M_INT_TO_INT => Ok(args[0]),
        _ => Err(EvalError::Internal(format!("integer_conversion: {}", method))),
    }
}

/// Evaluate a built-in `ratio` arithmetic/comparison instance method
/// (`registry::ratio_assoc`). Core operations: `+ - * /` (`/` panics on a zero
/// divisor). CL's `mod`/`rem`/`expt`/`abs`/`signum` on rationals live in
/// `prelude.rs` as typelisp methods built from these plus the
/// `ratio->bignum`/`bignum->ratio` truncation pair.
pub(super) fn eval_ratio_builtin(heap: &mut Heap, name: SymRef, args: &[Value]) -> Option<Result<Value, EvalError>> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => match (expect_ratio(heap, a), expect_ratio(heap, b)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Some(Err(e)),
        },
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two ratios", name)))),
    };
    let v = match name.well_known() {
        wk::M_OP_ADD => ratio_rt(heap, &a + &b),
        wk::M_OP_SUB => ratio_rt(heap, &a - &b),
        wk::M_OP_MUL => ratio_rt(heap, &a * &b),
        wk::M_OP_DIV => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            ratio_rt(heap, &a / &b)
        }
        wk::M_OP_LT => Value::Bool(a < b),
        wk::M_OP_LE => Value::Bool(a <= b),
        wk::M_OP_GT => Value::Bool(a > b),
        wk::M_OP_GE => Value::Bool(a >= b),
        wk::RETIRED_EQUALS => Value::Bool(a == b),
        wk::M_OP_NE => Value::Bool(a != b),
        wk::M_MAX => ratio_rt(heap, if a >= b { a } else { b }),
        wk::M_MIN => ratio_rt(heap, if a <= b { a } else { b }),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

/// `rational`'s arithmetic and comparisons (`registry::rational_assoc`), and
/// `int`'s `/`: the operands read as rationals whichever of `int`/`ratio`
/// they are, and an arithmetic result comes back normalized
/// (`typelisp_rt::rational::rational_result`).
pub(super) fn eval_rational_builtin(heap: &mut Heap, name: SymRef, args: &[Value]) -> Option<Result<Value, EvalError>> {
    use typelisp_rt::rational::{rational_of, rational_result};
    let (a, b) = match (args.first().and_then(|v| rational_of(heap, *v)), args.get(1).and_then(|v| rational_of(heap, *v))) {
        (Some(a), Some(b)) => (a, b),
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two rationals", name)))),
    };
    let v = match name.well_known() {
        wk::M_OP_ADD => rational_result(heap, a + b),
        wk::M_OP_SUB => rational_result(heap, a - b),
        wk::M_OP_MUL => rational_result(heap, a * b),
        wk::M_OP_DIV => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            rational_result(heap, a / b)
        }
        wk::M_OP_LT => Value::Bool(a < b),
        wk::M_OP_LE => Value::Bool(a <= b),
        wk::M_OP_GT => Value::Bool(a > b),
        wk::M_OP_GE => Value::Bool(a >= b),
        wk::RETIRED_EQUALS => Value::Bool(a == b),
        wk::M_OP_NE => Value::Bool(a != b),
        wk::M_MAX => rational_result(heap, if a >= b { a } else { b }),
        wk::M_MIN => rational_result(heap, if a <= b { a } else { b }),
        _ => unreachable!("eval_rational_builtin: {}", name),
    };
    Some(Ok(v))
}

/// `rational`'s one-argument conversions, and `ratio->rational`.
pub(super) fn rational_unary(heap: &mut Heap, name: SymRef, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    use typelisp_rt::rational::{rational_of, rational_result};
    let r = args
        .first()
        .and_then(|v| rational_of(heap, *v))
        .ok_or_else(|| EvalError::Internal(format!("{}: expected a rational", name)))?;
    Ok(match name.well_known() {
        wk::M_RATIO_TO_RATIONAL => rational_result(heap, r),
        wk::M_RATIONAL_TO_INT => heap.int_from_bigint(r.to_integer()),
        wk::M_TRY_RATIONAL_TO_INT => {
            let inner = if r.is_integer() { Some(heap.int_from_bigint(r.to_integer())) } else { None };
            heap.alloc_option(ret_type, inner)
        }
        wk::M_RATIONAL_TO_RATIO => heap.alloc_ratio(r),
        wk::M_RATIONAL_TO_FLOAT => heap.alloc_f64(r.to_f64().ok_or_else(|| EvalError::Internal("rational->float: conversion failed".into()))?),
        wk::M_NUMERATOR => heap.int_from_bigint(r.numer().clone()),
        wk::M_DENOMINATOR => heap.int_from_bigint(r.denom().clone()),
        _ => unreachable!("rational_unary: {}", name),
    })
}

/// `float->int` (`registry::float_assoc`): truncating toward zero into the
/// arbitrary-precision `int` — exact for every finite float, whatever its
/// magnitude (CL's `truncate`). Panics on a non-finite float (`NaN`/`inf`):
/// there is no integer to truncate to, the same "value outside the
/// representable range" panic precedent as `int->char`.
pub(super) fn float_to_int(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let f = expect_float(heap, &args[0])?;
    if !f.is_finite() {
        return Err(EvalError::Panic(format!("float->int: {} is not finite", f)));
    }
    Ok(heap.int_from_bigint(BigInt::from_f64(f.trunc()).expect("a finite float truncates to a representable BigInt")))
}

/// `float->ratio` (`registry::float_assoc`): widening and *exact* — every
/// finite `f64` is itself an exact dyadic rational (CL's `rational`, not the
/// lossy-round-trip-through-decimal `rationalize`). Panics on a non-finite
/// float, same precedent as [`float_to_int`].
pub(super) fn float_to_ratio(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let f = expect_float(heap, &args[0])?;
    BigRational::from_float(f)
        .map(|r| ratio_rt(heap, r))
        .ok_or_else(|| EvalError::Panic(format!("float->ratio: {} is not finite", f)))
}

/// `ratio->int` (`registry::ratio_assoc`): narrowing, truncating toward
/// zero (CL's `truncate`) — `Ratio::to_integer` already does exactly this.
pub(super) fn ratio_to_int(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let r = expect_ratio(heap, &args[0])?;
    Ok(heap.int_from_bigint(r.to_integer()))
}

/// `ratio->float` (`registry::ratio_assoc`): widening, possibly lossy
/// (IEEE-754 rounds to the nearest representable `f64`).
pub(super) fn ratio_to_float(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let r = expect_ratio(heap, &args[0])?;
    let f = r.to_f64().unwrap_or(f64::NAN);
    Ok(heap.alloc_f64(f))
}

/// `numerator`/`denominator` (`registry::ratio_assoc`): the reduced
/// components of a `ratio` — CL's own accessors of the same names — as
/// `int`. The denominator of a normalized `ratio` is always positive (see
/// `BoxedObj::Ratio`'s doc comment), matching CL's guarantee.
pub(super) fn ratio_numerator(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let n = expect_ratio(heap, &args[0])?.numer().clone();
    Ok(heap.int_from_bigint(n))
}

pub(super) fn ratio_denominator(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let n = expect_ratio(heap, &args[0])?.denom().clone();
    Ok(heap.int_from_bigint(n))
}

/// `parse-float` (`registry.rs`'s free-function entry): an `f64` literal via
/// `str::parse` (accepts everything Rust's own `FromStr for f64` does,
/// including `inf`/`nan`), `Err` on anything else.
pub(super) fn eval_parse_float(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let s = expect_str(heap, &args[0])?.to_string();
    Ok(typelisp_rt::sys_builtin::parse_float(heap, &s))
}
