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

use crate::check::analyze::{as_conversion, called_width, OPTION_SOME};
use crate::types::{prim_type_path, Path, Type};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::method::check_method_call;
use super::tree::{Node, Typed};
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
    let [(target, _), (src, src_loc)] = args else {
        return Err(Error::TypeError(format!("{}: ({} Type expr)", what, what)));
    };
    let target = parse_type(heap, &cx.types(), *target)?;
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
    if let Type::Dyn(..) = target {
        unimplemented!("typeck: `{}` to the trait object `{}`", what, target)
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
    if !try_variant {
        return value;
    }
    let inner = value.ty.clone();
    let ty = Type::Named(Path::builtin(typelisp_mem::wk::OPTION), vec![inner.clone()]);
    let node = Node::Construct {
        owner: Path::builtin(typelisp_mem::wk::OPTION),
        targs: vec![inner.clone()],
        variant: OPTION_SOME,
        mutable: false,
        field_tys: vec![inner],
        args: vec![value],
    };
    Typed { node, ty, loc }
}
