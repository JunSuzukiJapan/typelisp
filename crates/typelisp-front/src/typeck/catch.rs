//! `(catch 'tag body)`, and `(throw 'tag value)`, which leaves for the
//! nearest `catch` of the tag around it at run time.
//!
//! Which `catch` a `throw` reaches is decided at run time, so the type a
//! tag carries is the session's: every `catch` and `throw` of the tag, in
//! any form, gives it one type. The tag is a literal symbol, so that the
//! type is known where it is written; two modules' `'done` are two symbols,
//! and two tags. A form's uses say the type while it is checked, and the
//! registry keeps it once the form is settled.

use crate::types::Type;
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};
use super::unify::walk;

/// `(catch 'tag body)` — `args` without the `catch`. Its value is `body`'s,
/// or the tag's when `body` never returns, which is what a throw gave.
pub(crate) fn check_catch(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let [(tag, _), (body, body_loc)] = args else {
        return Err(Error::TypeError("catch: (catch 'tag body)".into()));
    };
    let tag = tag_of(heap, *tag, "catch")?;
    let known = carried(cx, tag);
    let body = check_expr(cx, heap, locals, *body, expected.or(known.as_ref()), body_loc.clone())?;
    note(cx, tag, &body.ty, "catch")?;
    let carried = carried(cx, tag);
    let ty = match (&carried, cx.resolve(&body.ty)) {
        (Some(c), Type::Never) => c.clone(),
        _ => body.ty.clone(),
    };
    let carried = carried.unwrap_or_else(|| ty.clone());
    Ok(Typed { node: Node::Catch { tag, carried, body: Box::new(body) }, ty, loc })
}

/// `(throw 'tag value)` — `args` without the `throw`. It never returns.
pub(crate) fn check_throw(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(tag, _), (value, value_loc)] = args else {
        return Err(Error::TypeError("throw: (throw 'tag value)".into()));
    };
    let tag = tag_of(heap, *tag, "throw")?;
    // Against the tag's type when it has one, so a mismatch is the value's.
    let known = carried(cx, tag);
    let value = check_expr(cx, heap, locals, *value, known.as_ref(), value_loc.clone())?;
    note(cx, tag, &value.ty, "throw")?;
    Ok(Typed { node: Node::Throw { tag, value: Box::new(value) }, ty: Type::Never, loc })
}

/// The symbol of `'tag`.
fn tag_of(heap: &Heap, form: Value, who: &str) -> Result<SymRef, Error> {
    let quoted = heap.list_to_vec(form).ok().filter(|elems| elems.len() == 2 && matches!(elems[0], Value::Symbol(id) if id.is(wk::QUOTE))).map(|elems| elems[1]);
    match quoted {
        Some(Value::Symbol(id)) => Ok(id),
        _ => Err(Error::TypeError(format!("{}: the tag must be a literal symbol, as in ({} 'done ...)", who, who))),
    }
}

/// The type `tag` carries, as this form or an earlier one has said.
fn carried(cx: &Cx, tag: SymRef) -> Option<Type> {
    let pending = cx.tags.borrow().iter().rev().find(|(t, _)| *t == tag).map(|(_, ty)| ty.clone());
    pending.or_else(|| cx.reg.throw_tag(tag))
}

/// A use of `tag` carrying `ty`: the tag's type, or one that agrees with
/// it. `!` says nothing: a body that only throws has it.
fn note(cx: &Cx, tag: SymRef, ty: &Type, who: &str) -> Result<(), Error> {
    if cx.resolve(ty) == Type::Never {
        return Ok(());
    }
    match carried(cx, tag) {
        Some(known) => {
            let unified = cx.holes.borrow_mut().unify(&known, ty);
            unified.map_err(|_| {
                let holes = cx.holes.borrow();
                Error::TypeError(format!("{}: `{}` carries `{}`, but this use carries `{}`", who, tag.name(), holes.shown(&known), holes.shown(ty)))
            })
        }
        None => {
            cx.tags.borrow_mut().push((tag, ty.clone()));
            Ok(())
        }
    }
}

/// The type the form said `tag` carries, `ty`, kept for the session's
/// later forms.
pub(crate) fn record(cx: &Cx, tag: SymRef, ty: &Type) -> Result<(), Error> {
    let ty = cx.resolve(ty);
    if cx.holes.borrow().unsolved(&ty).is_some() {
        return Err(Error::TypeError(format!("catch/throw: cannot infer the type the tag `{}` carries", tag.name())));
    }
    let mut generic = false;
    walk(&ty, &mut |t| generic |= matches!(t, Type::Param(_)));
    if generic {
        unimplemented!("typeck: the tag `{}` carrying `{}`, a type parameter of a generic body", tag.name(), ty)
    }
    cx.reg.record_throw_tag(tag, ty);
    Ok(())
}
