//! Literals: the atoms that are their own value.
//!
//! What a literal is depends on where it is written: an integer literal in a
//! `u8` position is a `u8`, and a float literal in an `f32` position is an
//! `f32`. A token cannot know that when it is read, so the reader gives every
//! integer past `i32` a `bignum` box and every float an `f64` box, and the
//! type is decided here.

use crate::types::Type;
use crate::{wk, Error, Heap, Value};

/// A literal: its core tag, its value (none for the unit value) and its type.
pub(crate) struct Literal {
    pub(crate) tag: u32,
    pub(crate) value: Option<Value>,
    pub(crate) ty: Type,
}

impl Literal {
    fn of(tag: u32, value: Value, ty: Type) -> Literal {
        Literal { tag, value: Some(value), ty }
    }
}

/// The literal `v` is where `expected` is expected, or `None` when `v` is not
/// a literal: a symbol that is not a keyword, a vector, array or tuple
/// literal, an object put into S-expression data, or the empty list where an
/// `Option` is expected (that is `Option::none`).
///
/// A value this allocates (an `f32` box) comes back unrooted.
pub(crate) fn literal(heap: &mut Heap, v: Value, expected: Option<&Type>) -> Result<Option<Literal>, Error> {
    let lit = match v {
        Value::Int(n) => {
            let ty = int_lit_ty(expected);
            if !int_lit_in_range(n, &ty) {
                return Err(int_lit_range_error(&n.to_string(), &ty, true));
            }
            // `int` is a tagged word and a fixed width a raw one, so the kind
            // is the node's: nothing downstream could read it off the number.
            let tag = if ty == Type::Int { wk::INT } else { wk::INT_ANY_WIDTH };
            Literal::of(tag, Value::Int(n), ty)
        }
        Value::Boxed(id) if heap.is_bignum(id) => match expected {
            // In an `int` position: a fixnum node when the number fits one,
            // and the box itself otherwise. A box holding a fixnum-range
            // value must never exist (`Heap::canonical_int`), and a `bignum`
            // node boxes again every time it is evaluated.
            Some(Type::Int) => {
                let fitted = i64::try_from(&*heap.bignum_value(id)).ok().filter(|v| typelisp_mem::fixnum_fits(*v));
                match fitted {
                    Some(n) => Literal::of(wk::INT, Value::Int(n), Type::Int),
                    None => Literal::of(wk::BIGNUM, v, Type::Int),
                }
            }
            // In a fixed-width position: that type, when the number fits it.
            // This is what makes the top half of `u32` writable at all.
            Some(t) if t.is_integer() => {
                let (fitted, text) = {
                    let big = heap.bignum_value(id);
                    (i64::try_from(&*big).ok().filter(|n| int_lit_in_range(*n, t)), big.to_string())
                };
                match fitted {
                    Some(n) => Literal::of(wk::INT_ANY_WIDTH, Value::Int(n), t.clone()),
                    None => return Err(int_lit_range_error(&text, t, false)),
                }
            }
            _ => Literal::of(wk::BIGNUM, v, Type::Int),
        },
        Value::Boxed(id) if heap.is_ratio(id) => Literal::of(wk::RATIO, v, Type::Ratio),
        // In an `f32` position the literal is an `f32`, boxed as one and
        // rounded to binary32, so the box and the type agree from the start.
        Value::Boxed(id) if heap.is_f64(id) || heap.is_f32(id) => {
            let ty = float_lit_ty(expected);
            let boxed = if ty == Type::F32 && heap.is_f64(id) {
                let f = heap.f64_value(id);
                heap.alloc_f32(f as f32)
            } else {
                v
            };
            Literal::of(wk::FLOAT_ANY_WIDTH, boxed, ty)
        }
        Value::Bool(_) => Literal::of(wk::BOOL, v, Type::Bool),
        Value::Char(_) => Literal::of(wk::CHAR, v, Type::Char),
        Value::Str(_) => Literal::of(wk::STR, v, Type::Str),
        // A keyword is self-evaluating (CL): it never names a variable.
        Value::Symbol(id) if id.is_keyword() => Literal::of(wk::SYM, v, Type::Symbol),
        // The empty list is `Option::none` where an `Option` is expected, and
        // the unit value anywhere else. It is not an empty `Sexpr`: `Sexpr`
        // means a non-empty S-expression.
        Value::Empty => match expected {
            Some(Type::Named(n, _)) if crate::types::path_is_builtin(n, wk::OPTION) => return Ok(None),
            _ => Literal { tag: wk::UNIT, value: None, ty: Type::Unit },
        },
        _ => return Ok(None),
    };
    Ok(Some(lit))
}

/// The type of an integer literal: the expected integer type, or `int`.
///
/// A C word counts as one, so `(c-malloc 16)` reads as it should. Nothing is
/// loosened by that: the literal is then a `c-ulong`, which is refused outside
/// `(unsafe ...)` like any C word, and [`int_lit_in_range`] still checks it.
fn int_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t) if t.is_integer() || t.is_c_word() => t.clone(),
        _ => Type::Int,
    }
}

/// Whether `n` is a number `ty` can hold — vacuously true for a `ty` that is
/// not a fixed-width integer type.
///
/// A value always is the number its type names. A literal is the one place a
/// program states a number and a type independently, so it is refused here
/// rather than cut: a cut is spelled `(as u8 300)`.
fn int_lit_in_range(n: i64, ty: &Type) -> bool {
    match crate::types::int_width_signed(ty) {
        Some((width, signed)) => crate::types::normalize_int(n, width, signed) == n,
        None => true,
    }
}

/// The error [`int_lit_in_range`] reports, naming the range that was missed.
///
/// `n` is text because the literal may be a `bignum` far outside the
/// carrier's range. `castable` says whether `(as T n)` would get the caller
/// what they asked for: only a literal that fit `i32` arrived as a
/// `Value::Int`, and a `bignum` reaches the cast through `bignum->int`, which
/// is itself `i32`-wide.
fn int_lit_range_error(n: &str, ty: &Type, castable: bool) -> Error {
    let name = ty.to_string();
    let range = match crate::types::int_width_signed(ty) {
        Some((width, true)) => format!("{}..={}", -(1i64 << (width - 1)), (1i64 << (width - 1)) - 1),
        Some((width, false)) => format!("0..={}", (1i64 << width) - 1),
        None => String::new(),
    };
    let advice = if castable {
        format!(" `(as {} {})` asks for the cut-back explicitly.", name, n)
    } else {
        " It is past `i32`, so it read as a `bignum`, and no fixed-width type here holds it.".to_string()
    };
    Error::TypeError(format!(
        "integer literal {} is out of range for {} ({}) — a value always holds the number its \
         type names, so this one has no type here.{}",
        n, name, range, advice
    ))
}

/// The type of a float literal: the expected float type, or `f64`.
fn float_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t) if t.is_float() => t.clone(),
        _ => Type::F64,
    }
}
