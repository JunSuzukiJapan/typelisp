//! The numeric built-in methods: fixed-width integers, floats, `int`, `ratio`
//! and `rational`.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, ToPrimitive, Zero};
use crate::{wk, Heap, SymRef, TypeId, Value};
use crate::eval::value::EvalError;
use super::{expect_str, rt_i64};

/// Evaluate a built-in integer arithmetic/comparison instance method
/// (`registry::int_assoc`), at the receiver type's own `width` and
/// signedness.
///
/// One function for all six integer types, but *not* one behaviour: a type
/// name means its width and its signedness and nothing else
/// (`types::int_width_signed`), so every arithmetic result is cut back to
/// that width here. The operands arrive already normalized — that is the
/// invariant every producer of an integer maintains — which is what lets the
/// comparisons, the division and the remainder below stay plain `i64`
/// operations for the unsigned types too: a normalized `u32` is a
/// non-negative `i64`.
pub(super) fn eval_int_builtin(name: SymRef, args: &[Value], width: u32, signed: bool) -> Option<Result<Value, EvalError>> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(Value::Int(a)), Some(Value::Int(b))) => (*a, *b),
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two integers", name)))),
    };
    let norm = |v: i64| Value::Int(crate::types::normalize_int(v, width, signed));
    let v = match name.well_known() {
        // Wrapping, then cut: the low `width` bits of a 64-bit sum are the
        // sum's own low `width` bits, so the wide operation is a correct way
        // to compute the narrow one. `wrapping_*` rather than `+`/`-`/`*`
        // only because Rust's own overflow check would fire on the 64-bit
        // intermediate, which is not the overflow anyone asked about.
        wk::M_OP_ADD => norm(a.wrapping_add(b)),
        wk::M_OP_SUB => norm(a.wrapping_sub(b)),
        wk::M_OP_MUL => norm(a.wrapping_mul(b)),
        wk::M_OP_DIV => {
            if b == 0 {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            // The one overflow a division has: the most negative value over
            // `-1`, whose quotient is one past the type's maximum. CL would
            // widen to a bignum, but the *declared* type of this expression
            // is a fixed width and a checked program cannot be handed a wider
            // result than it asked for — so it fails, with `rt_int_div`'s
            // wording. Unsigned types have no such case (no negative
            // operands), and the check below says so by construction.
            if signed && b == -1 && a == crate::types::normalize_int(1i64 << (width - 1), width, true) {
                return Some(Err(EvalError::Panic(format!("arithmetic overflow: {} / {}", a, b))));
            }
            norm(a / b)
        }
        wk::M_MOD => {
            if b == 0 {
                return Some(Err(EvalError::Panic("mod by zero".into())));
            }
            // CL `mod`: floored remainder, result takes the sign of the
            // divisor (`-7 mod 3 = 2`). The most-negative-over-`-1` case that
            // traps for `/` is mathematically 0 here, and `wrapping_rem`
            // answers exactly that.
            let r = a.wrapping_rem(b);
            norm(if r != 0 && (r < 0) != (b < 0) { r + b } else { r })
        }
        wk::M_OP_LT => Value::Bool(a < b),
        wk::M_OP_LE => Value::Bool(a <= b),
        wk::M_OP_GT => Value::Bool(a > b),
        wk::M_OP_GE => Value::Bool(a >= b),
        wk::RETIRED_EQUALS => Value::Bool(a == b),
        wk::M_OP_NE => Value::Bool(a != b),
        wk::M_MAX => norm(a.max(b)),
        wk::M_MIN => norm(a.min(b)),
        // Bitwise: the operands' bits above `width` are the extension, and
        // the result's have to be the extension of the *result* — which for
        // an unsigned `and`/`or`/`xor` they already are, but for `xor` on
        // signed operands (or `lognot`, below) they are not.
        wk::M_LOGAND => norm(a & b),
        wk::M_LOGIOR => norm(a | b),
        wk::M_LOGXOR => norm(a ^ b),
        // `(ash integer count)`: positive `count` shifts left, negative
        // shifts right (arithmetic — sign-extending), matching CL §12.10.
        // Shifts by `width` or more places are clamped rather than handed to
        // Rust's `<<`/`>>`: the result at that point is just `0` (left), and
        // for a right shift `0` or, for a signed negative operand, `-1` —
        // the sign bit smeared across every bit.
        wk::M_ASH => {
            let w = i64::from(width);
            norm(if b >= 0 {
                if b >= w { 0 } else { a.wrapping_shl(b as u32) }
            } else if -b >= w {
                if signed && a < 0 { -1 } else { 0 }
            } else {
                a >> (-b)
            })
        }
        // Past the type's width there is no bit to read: for a signed value
        // every one of them is the sign, for an unsigned value every one is
        // zero.
        // `a` is the integer and `b` the bit position — the argument order
        // reversed when the position became an `i32` (`registry::int_assoc`).
        wk::M_LOGBITP => Value::Bool(if b >= i64::from(width) { signed && a < 0 } else { (a >> b) & 1 == 1 }),
        wk::M_LOGTEST => Value::Bool((a & b) != 0),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

/// `lognot` at the receiver's width: complement, then cut — the complement of
/// a normalized value is not itself normalized (`(lognot (the u8 0))` is
/// `255`, not `-1`).
pub(super) fn int_lognot(args: &[Value], width: u32, signed: bool) -> Result<Value, EvalError> {
    Ok(Value::Int(crate::types::normalize_int(!rt_i64(&args[0])?, width, signed)))
}

/// CL's `logcount` at the receiver's width: the number of one bits in a
/// non-negative value, of zero bits in a negative one — counted inside
/// `width`, not across the 64-bit word the value is carried in.
pub(super) fn int_logcount(args: &[Value], width: u32) -> Result<Value, EvalError> {
    let n = rt_i64(&args[0])?;
    let mask = if width == 64 { !0u64 } else { (1u64 << width) - 1 };
    let bits = (n as u64) & mask;
    Ok(Value::Int(i64::from(if n >= 0 { bits.count_ones() } else { (!bits & mask).count_ones() })))
}

/// CL's `integer-length` at the receiver's width: the bits needed to
/// represent the value excluding the sign.
pub(super) fn int_integer_length(args: &[Value], width: u32) -> Result<Value, EvalError> {
    let n = rt_i64(&args[0])?;
    let v = if n >= 0 { n as u64 } else { !(n as u64) };
    let mask = if width == 64 { !0u64 } else { (1u64 << width) - 1 };
    Ok(Value::Int(i64::from(64 - (v & mask).leading_zeros())))
}

/// A float operand's number, whichever width labels it.
///
/// The arithmetic below computes in binary64 and rounds afterwards
/// ([`float_at`]), so an operand is read as `f64` here — an `f32` widens
/// exactly. The width is not lost by doing so: it is the *receiver's declared
/// type* that decides the rounding and the result box, not the operand's box.
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

/// `int->float` (`registry::int_assoc`): widen an integer to `f64`. Every
/// width holds its number sign- or zero-extended in one `Value::Int(i64)`
/// (see `eval_int_builtin`'s doc comment), so one implementation covers all
/// six — the carrier already says which number it is.
pub(super) fn int_to_float(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    match args.first() {
        Some(Value::Int(n)) => Ok(heap.alloc_f64(*n as f64)),
        other => Err(EvalError::Internal(format!("int->float: expected an integer, got {:?}", other))),
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

/// Evaluate a built-in `f64` arithmetic/comparison instance method
/// (`registry::float_assoc`). Unlike [`eval_int_builtin`], `/` never panics on
/// a zero divisor — IEEE-754 division yields `inf`/`NaN` instead, the natural
/// float semantics (no "can't express nonzero" gap to plug). `mod`/`rem` are
/// defined in `prelude.rs` as typelisp methods (`a - b*floor|truncate(a/b)`).
pub(super) fn eval_float_builtin(heap: &mut Heap, name: SymRef, args: &[Value], single: bool) -> Option<Result<Value, EvalError>> {
    let (a, b) = match (args.first().map(|v| expect_float(heap, v)), args.get(1).map(|v| expect_float(heap, v))) {
        (Some(Ok(a)), Some(Ok(b))) => (a, b),
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two floats", name)))),
    };
    let v = match name.well_known() {
        wk::M_OP_ADD => float_at(heap, a + b, single),
        wk::M_OP_SUB => float_at(heap, a - b, single),
        wk::M_OP_MUL => float_at(heap, a * b, single),
        wk::M_OP_DIV => float_at(heap, a / b, single),
        wk::M_OP_LT => Value::Bool(a < b),
        wk::M_OP_LE => Value::Bool(a <= b),
        wk::M_OP_GT => Value::Bool(a > b),
        wk::M_OP_GE => Value::Bool(a >= b),
        wk::RETIRED_EQUALS => Value::Bool(a == b),
        wk::M_OP_NE => Value::Bool(a != b),
        wk::M_MAX => float_at(heap, a.max(b), single),
        wk::M_MIN => float_at(heap, a.min(b), single),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

/// `v` boxed at the receiver's precision: rounded to binary32 first when the
/// receiver is `f32`.
///
/// The *arithmetic* is the one the type names: it is computed in binary64 and
/// rounded here, which is what makes `f32` binary32 rather than a label on an
/// `f64` (`docs/ja/reference/functions/numbers.md` §1). And the box is the one the type names
/// too — an `f32` result goes in an `f32` box, so that everything downstream
/// (printing, `eql`, a dump) can still tell which width it is.
fn float_at(heap: &mut Heap, v: f64, single: bool) -> Value {
    if single {
        heap.alloc_f32(v as f32)
    } else {
        heap.alloc_f64(v)
    }
}

pub(super) fn float_unary(heap: &mut Heap, args: &[Value], f: fn(f64) -> f64, single: bool) -> Result<Value, EvalError> {
    let r = f(expect_float(heap, &args[0])?);
    Ok(float_at(heap, r, single))
}

pub(super) fn float_expt(heap: &mut Heap, args: &[Value], single: bool) -> Result<Value, EvalError> {
    let r = expect_float(heap, &args[0])?.powf(expect_float(heap, &args[1])?);
    Ok(float_at(heap, r, single))
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

/// `int->i8`/`int->u32`/... (`registry::int_assoc`): a cast between two
/// integer widths — truncating, which is what a width cast means.
pub(super) fn int_to_width(args: &[Value], width: u32, signed: bool) -> Result<Value, EvalError> {
    Ok(Value::Int(crate::types::normalize_int(rt_i64(&args[0])?, width, signed)))
}

/// `try-int->i8`/... (`registry::int_assoc`): [`int_to_width`] asked as a
/// question — `None` when the value does not fit the target width, which is
/// exactly when truncating would change it.
pub(super) fn try_int_to_width(heap: &mut Heap, args: &[Value], width: u32, signed: bool, ret_type: TypeId) -> Result<Value, EvalError> {
    let n = rt_i64(&args[0])?;
    let fits = crate::types::normalize_int(n, width, signed) == n;
    Ok(heap.alloc_option(ret_type, fits.then_some(Value::Int(n))))
}

/// Whether `method` is a `try-int->W` cast rather than an `int->W` one, and
/// `W`'s width and signedness — `None` for any other method. The pairs are
/// [`crate::types::int_width_signed`]'s, for the targets
/// `registry::int_assoc` gives these casts.
pub(super) fn width_cast_target(method: SymRef) -> Option<(bool, u32, bool)> {
    Some(match method.well_known() {
        wk::M_INT_TO_I8 => (false, 8, true),
        wk::M_INT_TO_I16 => (false, 16, true),
        wk::M_INT_TO_I32 => (false, 32, true),
        wk::M_INT_TO_U8 => (false, 8, false),
        wk::M_INT_TO_U16 => (false, 16, false),
        wk::M_INT_TO_U32 => (false, 32, false),
        wk::M_INT_TO_C_LONG => (false, 64, true),
        wk::M_INT_TO_C_ULONG => (false, 64, false),
        wk::M_TRY_INT_TO_I8 => (true, 8, true),
        wk::M_TRY_INT_TO_I16 => (true, 16, true),
        wk::M_TRY_INT_TO_I32 => (true, 32, true),
        wk::M_TRY_INT_TO_U8 => (true, 8, false),
        wk::M_TRY_INT_TO_U16 => (true, 16, false),
        wk::M_TRY_INT_TO_U32 => (true, 32, false),
        wk::M_TRY_INT_TO_C_LONG => (true, 64, true),
        wk::M_TRY_INT_TO_C_ULONG => (true, 64, false),
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

/// An `int` operand as the interpreter reads it: a fixnum straight out of
/// its `Value::Int`, or the `BigInt` cloned out of its box (owned, for the
/// reason [`expect_bignum`] gives — every caller goes on to allocate).
pub(super) enum IntArg {
    Fix(i64),
    Big(BigInt),
}

impl IntArg {
    fn into_big(self) -> BigInt {
        match self {
            IntArg::Fix(n) => BigInt::from(n),
            IntArg::Big(n) => n,
        }
    }
}

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
            let n = if a.sign() == num_bigint::Sign::Minus { !&a } else { a.clone() };
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

/// The low 64 bits of an `int`, two's complement — what a width cast out of
/// it starts from. A fixnum is already that; a bignum is cut, which is what
/// `int->W` means (`registry::int_assoc`'s "truncates").
fn int_low_word(a: IntArg) -> i64 {
    match a {
        IntArg::Fix(n) => n,
        IntArg::Big(n) => (n & BigInt::from(u64::MAX)).to_u64().expect("masked to 64 bits") as i64,
    }
}

/// `int->W`/`try-int->W` on an `int` receiver: the cast truncates to the
/// target (`int_to_width`'s meaning), and the question is answered exactly —
/// a bignum fits a 64-bit C word when it is in that word's range, and never
/// fits anything narrower.
pub(super) fn integer_to_width(heap: &mut Heap, args: &[Value], width: u32, signed: bool, try_variant: bool, ret_type: TypeId) -> Result<Value, EvalError> {
    let a = expect_int(heap, &args[0])?;
    if !try_variant {
        return Ok(Value::Int(crate::types::normalize_int(int_low_word(a), width, signed)));
    }
    let fits = match &a {
        IntArg::Fix(n) => crate::types::normalize_int(*n, width, signed) == *n,
        IntArg::Big(n) => width == 64 && if signed { n.to_i64().is_some() } else { n.to_u64().is_some() },
    };
    let word = int_low_word(a);
    Ok(heap.alloc_option(ret_type, fits.then_some(Value::Int(word))))
}

/// `int->int` on a fixed-width receiver (`(as int x)`): exact widening.
/// `c-ulong` is the one receiver whose 64 bits are unsigned, so its word is
/// read as such; every narrower unsigned type is already non-negative in the
/// register.
pub(super) fn width_to_int(heap: &mut Heap, args: &[Value], width: u32, signed: bool) -> Result<Value, EvalError> {
    let word = rt_i64(&args[0])?;
    let n = if !signed && width == 64 { i128::from(word as u64) } else { i128::from(word) };
    Ok(heap.canonical_int(n))
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

/// `int->ratio` (`registry::int_assoc`): always-exact widening.
pub(super) fn int_to_ratio(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(ratio_rt(heap, BigRational::from_integer(BigInt::from(rt_i64(&args[0])?))))
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
