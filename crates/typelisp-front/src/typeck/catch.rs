//! `(catch 'tag body)`, and `(throw 'tag value)`, which leaves for the
//! nearest `catch` of the tag around it at run time.
//!
//! The type a tag carries is each use's own, decided where it is checked —
//! in a generic body, per instantiation — and lowering names the pair of the
//! symbol and the type (`typelisp_mem::symbols::throw_tag_symbol`). A `throw`
//! and the `catch` it reaches carry one type: one written inside a `catch`
//! of its symbol is checked against it here, and one that reaches a `catch`
//! of its symbol from elsewhere is compared at run time
//! (`typelisp_mem::symbols::throw_tag_mismatch`). The tag is a literal symbol, so that the
//! symbol is known where it is written; two modules' `'done` are two
//! symbols, and two tags.

use crate::types::Type;
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// A `catch` being checked: its tag, the type it carries, and whether a
/// `throw` inside it has said that type.
pub(crate) struct Enclosing {
    tag: SymRef,
    carried: Type,
    thrown: bool,
}

/// `(catch 'tag body)` — `args` without the `catch`. Its value is `body`'s,
/// or what a throw gave, so the two are one type.
pub(crate) fn check_catch(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let [(tag, _), (body, body_loc)] = args else {
        return Err(Error::TypeError("catch: (catch 'tag body)".into()));
    };
    let tag = tag_of(heap, *tag, "catch")?;
    let carried = match expected {
        Some(t) => t.clone(),
        None => cx.holes.borrow_mut().fresh(),
    };
    cx.tags.borrow_mut().push(Enclosing { tag, carried: carried.clone(), thrown: false });
    let body = check_expr(cx, heap, locals, *body, Some(&carried), body_loc.clone());
    let thrown = cx.tags.borrow_mut().pop().is_some_and(|c| c.thrown);
    let body = body?;
    agree(cx, tag, &carried, &body.ty, "catch")?;
    // Nothing said the type: no expectation, no `throw` inside, and a body
    // that never returns. The `catch` gives nothing either, so it carries `!`.
    let carried = if expected.is_none() && !thrown && cx.resolve(&body.ty) == Type::Never { Type::Never } else { carried };
    Ok(Typed { node: Node::Catch { tag, carried: carried.clone(), body: Box::new(body) }, ty: carried, loc })
}

/// `(throw 'tag value)` — `args` without the `throw`. It never returns.
pub(crate) fn check_throw(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(tag, _), (value, value_loc)] = args else {
        return Err(Error::TypeError("throw: (throw 'tag value)".into()));
    };
    let tag = tag_of(heap, *tag, "throw")?;
    let enclosing = cx.tags.borrow().iter().rev().find(|c| c.tag == tag).map(|c| c.carried.clone());
    let value = check_expr(cx, heap, locals, *value, enclosing.as_ref(), value_loc.clone())?;
    match &enclosing {
        Some(carried) => {
            agree(cx, tag, carried, &value.ty, "throw")?;
            if let Some(c) = cx.tags.borrow_mut().iter_mut().rev().find(|c| c.tag == tag) {
                c.thrown = true;
            }
        }
        None => no_typed_ptr(cx, tag, &value.ty, "throw")?,
    }
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

/// A use inside the `catch` of `tag` carrying `carried`, of type `ty`: the
/// two agree. `!` says nothing: a body that only throws has it.
fn agree(cx: &Cx, tag: SymRef, carried: &Type, ty: &Type, who: &str) -> Result<(), Error> {
    if cx.resolve(ty) == Type::Never {
        return Ok(());
    }
    no_typed_ptr(cx, tag, ty, who)?;
    let unified = cx.holes.borrow_mut().unify(carried, ty);
    unified.map_err(|_| {
        let holes = cx.holes.borrow();
        Error::TypeError(format!("{}: the `catch` of `{}` around it carries `{}`, but this carries `{}`", who, tag.name(), holes.shown(carried), holes.shown(ty)))
    })
}

/// A tag cannot carry a typed pointer out of the `unsafe` that allocated it.
fn no_typed_ptr(cx: &Cx, tag: SymRef, ty: &Type, who: &str) -> Result<(), Error> {
    if super::c_memory::mentions_typed_ptr(&cx.resolve(ty)) {
        return Err(Error::TypeError(format!(
            "{}: `{}` would carry `{}`, and a typed pointer cannot leave the `unsafe` that \
             allocated its memory — a `catch` outside it would receive freed memory",
            who,
            tag.name(),
            cx.resolve(ty)
        )));
    }
    Ok(())
}
