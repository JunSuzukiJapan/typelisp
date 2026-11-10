//! `(block name body...)`, and `(return-from name value?)`, which leaves it.
//!
//! A block's value is its body's, or what a `return-from` naming it gives
//! (`()` when it gives none); every one gives one type. A `return-from`
//! names the innermost block of that name around it, in the same function:
//! a `defun`, a `defmethod`, an `impl`'s method, a trait's default body and
//! a `labels` function each have a block named after themselves; a `lambda`
//! has none, and no block outside it is one its body can leave. The exits are
//! met while the body is checked, so the type is the first one an exit or
//! the expected type gives.

use crate::types::Type;
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::{agree, check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// A block being checked: its name, its name in the core IR, the type its
/// exits give once one has, and whether a `return-from` names it.
pub(crate) struct BlockExit {
    name: SymRef,
    label: Box<str>,
    ty: Option<Type>,
    used: bool,
}

/// `(block name body...)` — `args` without the `block`.
pub(crate) fn check_block(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let name = block_name(args.first().map(|(v, _)| *v), "block")?;
    let seed = expected.map(|e| cx.resolve(e)).filter(|t| *t != Type::Never);
    let (body, ty) = within(cx, heap, locals, name, seed, &args[1..])?;
    let mut t = body;
    t.loc = loc;
    t.ty = ty;
    Ok(t)
}

/// A definition's body, in its block named `name`, its value `ret`.
pub(crate) fn check_named_body(cx: &Cx, heap: &mut Heap, locals: &Locals, name: SymRef, forms: &[(Value, Option<Loc>)], ret: &Type) -> Result<Vec<Typed>, Error> {
    let (mut body, _) = within(cx, heap, locals, name, Some(ret.clone()), forms)?;
    if let Node::Block { .. } = body.node {
        return Ok(vec![body]);
    }
    match std::mem::replace(&mut body.node, Node::Break) {
        Node::Let { body, .. } => Ok(body),
        _ => unreachable!("`within` makes a block or a `let`"),
    }
}

/// `forms` checked in a block named `name` whose exits give `seed`, if it
/// is given: the block (a `let` of no bindings when nothing leaves it), and
/// its type.
fn within(cx: &Cx, heap: &mut Heap, locals: &Locals, name: SymRef, seed: Option<Type>, forms: &[(Value, Option<Loc>)]) -> Result<(Typed, Type), Error> {
    let label = cx.block_label(name);
    cx.blocks.borrow_mut().push(BlockExit { name, label, ty: seed, used: false });
    let checked = body(cx, heap, locals, forms);
    let exit = cx.blocks.borrow_mut().pop().expect("pushed above");
    let (body, body_ty) = checked?;
    let ty = match exit.ty {
        Some(t) => {
            if cx.resolve(&body_ty) != Type::Never {
                agree(cx, &body_ty, Some(&t))?;
            }
            t
        }
        None => body_ty,
    };
    let seq = Typed { node: Node::Let { binds: Vec::new(), body }, ty: ty.clone(), loc: None };
    if !exit.used {
        return Ok((seq, ty));
    }
    let block = Typed { node: Node::Block { label: exit.label, body: Box::new(seq) }, ty: ty.clone(), loc: None };
    Ok((block, ty))
}

/// The block's body in order, the last form checked against the type its
/// exits have given by then.
fn body(cx: &Cx, heap: &mut Heap, locals: &Locals, forms: &[(Value, Option<Loc>)]) -> Result<(Vec<Typed>, Type), Error> {
    let mut out = Vec::with_capacity(forms.len());
    for (i, (v, loc)) in forms.iter().enumerate() {
        let expected = if i + 1 == forms.len() { innermost_type(cx) } else { None };
        out.push(check_expr(cx, heap, locals, *v, expected.as_ref(), loc.clone())?);
    }
    let ty = out.last().map_or(Type::Unit, |t| t.ty.clone());
    Ok((out, ty))
}

/// The type the innermost block's exits have given.
fn innermost_type(cx: &Cx) -> Option<Type> {
    cx.blocks.borrow().last().and_then(|b| b.ty.clone())
}

/// `(return-from name)` or `(return-from name value)` — `args` without the
/// `return-from`.
pub(crate) fn check_return_from(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    if args.len() > 2 {
        return Err(Error::TypeError("return-from: (return-from name) or (return-from name value)".into()));
    }
    let name = block_name(args.first().map(|(v, _)| *v), "return-from")?;
    // The innermost block of the name, so a block may shadow an outer one.
    let at = cx.blocks.borrow().iter().rposition(|b| b.name == name);
    let Some(at) = at else {
        return Err(Error::TypeError(format!("return-from: no enclosing block named `{}`", name.name())));
    };
    let expected = cx.blocks.borrow()[at].ty.clone();
    let value = match args.get(1) {
        Some((v, l)) => Some(check_expr(cx, heap, locals, *v, expected.as_ref(), l.clone())?),
        None => {
            agree(cx, &Type::Unit, expected.as_ref())?;
            None
        }
    };
    let given = value.as_ref().map_or(Type::Unit, |v| cx.resolve(&v.ty));
    let mut blocks = cx.blocks.borrow_mut();
    let exit = &mut blocks[at];
    exit.used = true;
    if exit.ty.is_none() && given != Type::Never {
        exit.ty = Some(given);
    }
    Ok(Typed { node: Node::ReturnFrom { label: exit.label.clone(), value: value.map(Box::new) }, ty: Type::Never, loc })
}

/// The bare symbol a `block` or a `return-from` names: resolved where it is
/// written, so nothing that would be looked up elsewhere.
fn block_name(arg: Option<Value>, who: &str) -> Result<SymRef, Error> {
    match arg {
        Some(Value::Symbol(id)) => Ok(id),
        Some(_) => Err(Error::TypeError(format!("{}: the name must be a bare symbol", who))),
        None => Err(Error::TypeError(format!("{}: missing the block name", who))),
    }
}
