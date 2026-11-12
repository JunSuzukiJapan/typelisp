//! `as` and `try-as`: `(as Type expr)` converts `expr` to `Type`, between
//! the numeric types and `char`; `try-as` gives `Option<Type>`, `none` where
//! the conversion would lose the value.
//!
//! Each conversion is a method of the source type (`int->float`,
//! `float->int`, `int->u8` ...), so the form becomes a call of it. A
//! conversion into an integer or float family is registered at the family's
//! widest type, and a narrower target is reached by converting the width
//! after it. The table of conversions is the old checker's
//! ([`as_conversion`]).


use crate::types::{prim_type_path, Type};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::method::check_method_call;
use super::tree::Typed;
use super::type_syntax::parse_type;

/// `(as Type expr)`, or `(try-as Type expr)` when `try_variant` — `args`
/// without the head.
pub(crate) fn check_as(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    try_variant: bool,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let what = if try_variant { "try-as" } else { "as" };
    let [(target, target_at), (src, src_loc)] = args else {
        return Err(Error::TypeError(format!("{}: ({} Type expr)", what, what)));
    };
    let target = parse_type(heap, &cx.types(), *target, target_at.as_ref())?;
    let mut src = check_expr(cx, heap, locals, *src, None, src_loc.clone())?;
    // A number literal no type decided is converted from its default type.
    cx.decide(&src.ty);
    let from = cx.resolve(&src.ty);
    src.ty = from.clone();
    if from == target {
        return Ok(wrapped(src, try_variant, loc));
    }
    // The same word.
    if (from == Type::Int && target == Type::Rational) || (matches!(from, Type::PtrTo(_)) && target == Type::Ptr && !try_variant) {
        src.ty = target;
        return Ok(wrapped(src, try_variant, loc));
    }
    let int_like = |t: &Type| t.is_int_family() || t.is_c_word();
    if (int_like(&from) && int_like(&target)) || (from.is_float() && target.is_float()) {
        // `try-int->u8` is an `Option<u8>` already.
        return width_cast(cx, heap, locals, src, &target, try_variant, loc);
    }
    if let Type::Dyn(to, pins) = &target {
        // Whether a type implements a trait is known statically, so the
        // conversion always succeeds or is a type error.
        if try_variant {
            return Err(Error::TypeError(format!(
                "try-as: `{}` either always succeeds or is a type error (a type's trait impls are known statically) — use `as`",
                target
            )));
        }
        let mut boxed = super::dyn_trait::coerce(cx, src, to, pins)?;
        boxed.loc = loc;
        return Ok(boxed);
    }
    let Some((panic_method, try_method)) = as_conversion(&from, &target) else {
        return Err(Error::TypeError(format!(
            "{}: no conversion from `{}` to `{}` (as/try-as cover only the numeric/char catalog: int/f64/bignum/ratio/char)",
            what, from, target
        )));
    };
    let name = if try_variant { try_method.unwrap_or(panic_method) } else { panic_method };
    let called = call_conversion(cx, heap, locals, src, name, loc.clone())?;
    // A partial conversion's `try-` method gives the `Option` itself, and
    // has no narrower target to reach.
    if try_variant && try_method.is_some() {
        return Ok(called);
    }
    let called = if target != called_width(&target) { width_cast(cx, heap, locals, called, &target, false, loc.clone())? } else { called };
    Ok(wrapped(called, try_variant, loc))
}

/// `src` converted to `target`, of the same integer or float family as it:
/// `int->W` or `float->W`, or the `try-` one asking whether nothing was lost.
fn width_cast(cx: &Cx, heap: &mut Heap, locals: &Locals, src: Typed, target: &Type, try_variant: bool, loc: Option<Loc>) -> Result<Typed, Error> {
    let family = if target.is_int_family() || target.is_c_word() { "int" } else { "float" };
    let width = target.scalar_name().expect("a width cast's target is a scalar");
    let name = if try_variant { format!("try-{}->{}", family, width) } else { format!("{}->{}", family, width) };
    call_conversion(cx, heap, locals, src, &name, loc)
}

/// The conversion method `name` of `src`'s type, called on `src`.
fn call_conversion(cx: &Cx, heap: &mut Heap, locals: &Locals, src: Typed, name: &str, loc: Option<Loc>) -> Result<Typed, Error> {
    let owner = prim_type_path(&src.ty).expect("a conversion's source is a primitive type");
    let method = member_symbol(name);
    let Some(def) = cx.reg.method(&owner, method) else {
        return Err(Error::TypeError(format!("as: `{}` has no conversion `{}`", src.ty, name)));
    };
    check_method_call(cx, heap, locals, owner, method, def, Some(src), &[], None, loc)
}

/// `value`, or `(some value)` for `try-as`.
fn wrapped(value: Typed, try_variant: bool, loc: Option<Loc>) -> Typed {
    if try_variant {
        Typed::some(value, loc)
    } else {
        value
    }
}

/// `Checker::check_as`'s conversion table: `(from, to)` -> `(panic_method,
/// try_method)`, where `panic_method` is the existing `registry.rs` instance
/// method `as` always calls, and `try_method` — `Some` only for the two
/// *partial* pairs — is the `Option`-returning counterpart `try-as` calls
/// instead (`None` here means the pair is total: `try-as` calls
/// `panic_method` too and wraps the result in `Some`).
///
/// Every `to`-is-`i64` request is looked up as if `to` were `i32` instead:
/// `float->int`/`bignum->int`/`char->int` (`registry.rs`) are always
/// registered with a hardcoded `I32` return even though the underlying
/// computation already produces a full `i64` (`Value::Int` is uniformly
/// `i64` regardless of which static width labels it) — `check_as` relabels
/// the result to `I64` afterward on that branch, so this table only ever
/// needs to name the `i32` method once. An `i32`<->`i64` source/target pair
/// itself never reaches this table — `check_as` relabels that directly,
/// before the lookup.
pub(crate) fn as_conversion(from: &Type, to: &Type) -> Option<(&'static str, Option<&'static str>)> {
    use Type::*;
    // Widths do not appear in a conversion method's name: every integer type
    // has the same `int->float`/`int->bignum`/... catalog (they share one
    // runtime representation), and both float types the same `float->int`.
    // So the table is written between *families*, and `check_as` relabels the
    // result to the width that was actually asked for.
    let to_key = called_width(to);
    match (from, &to_key) {
        // A C word can be *read* as a `bignum`, and as nothing else here. It is
        // the only target that never loses anything, which is the whole point:
        // it is how a `size_t` too large for `i32` is read at all (`as i32`
        // truncates and `try-as i32` answers `none`). `f64` would round,
        // `ratio` and `char` are not what a machine word means, and none of
        // the three is registered for these types anyway
        // (`registry::c_word_assoc`). Ahead of the integer arm, which these
        // deliberately fail.
        _ if from.is_c_word() => match &to_key {
            Int => Some(("int->int", None)),
            _ => None,
        },
        _ if from.is_int_family() => match &to_key {
            F64 => Some(("int->float", None)),
            Ratio => Some(("int->ratio", None)),
            Char => Some(("int->char", Some("try-int->char"))),
            _ => None,
        },
        _ if from.is_float() => match &to_key {
            Int => Some(("float->int", None)),
            Ratio => Some(("float->ratio", None)),
            _ => None,
        },
        (Char, Int) => Some(("char->int", None)),
        (Ratio, F64) => Some(("ratio->float", None)),
        (Ratio, Int) => Some(("ratio->int", None)),
        (Ratio, Rational) => Some(("ratio->rational", None)),
        // `as int` truncates, as `ratio`'s does; `try-as int` answers whether
        // the value is an integer (CL's `integerp`).
        (Rational, Int) => Some(("rational->int", Some("try-rational->int"))),
        (Rational, Ratio) => Some(("rational->ratio", None)),
        (Rational, F64) => Some(("rational->float", None)),
        _ => None,
    }
}

/// The width a conversion *into* `ty`'s family is actually registered at:
/// `i32` for every integer, `f64` for both floats, and `ty` itself for
/// everything else. See [`as_conversion`].
pub(crate) fn called_width(ty: &Type) -> Type {
    if ty.is_integer() {
        Type::Int
    } else if ty.is_float() {
        Type::F64
    } else {
        ty.clone()
    }
}
