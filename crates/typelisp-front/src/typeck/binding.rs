//! `let` and `progn`: binding locals, and running forms in order.

use crate::types::Type;
use crate::{Error, Heap, Loc, Value};

use super::expr::{check_body, check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// `(let ((name value)...) body...)`. Every value is checked outside all the
/// bindings, as in CL; the body sees them all.
pub(crate) fn check_let(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let Some((bindings, _)) = args.first() else {
        return Err(Error::TypeError("let: (let ((name val)...) body...)".into()));
    };
    let mut binds = Vec::new();
    let mut written = Vec::new();
    for (binding, _) in heap.list_to_vec_locs(*bindings)? {
        let pair = heap.list_to_vec_locs(binding)?;
        let [(name, name_loc), (value, value_loc)] = pair.as_slice() else {
            return Err(Error::TypeError("let: binding must be (name val)".into()));
        };
        let Value::Symbol(name) = *name else {
            return Err(Error::TypeError("let: binding name must be a symbol".into()));
        };
        let checked = check_expr(cx, heap, locals, *value, None, value_loc.clone());
        binds.push((name, cx.recovered(checked, value_loc.clone())?));
        written.push(name_loc.clone());
    }
    let inner = locals.bind(binds.iter().zip(written).map(|((n, t), at)| (*n, t.ty.clone(), at)).collect())?;
    let (body, ty) = check_body(cx, heap, &inner, &args[1..], expected)?;
    Ok(Typed { node: Node::Let { binds, body }, ty, loc })
}

/// `(let* ((name val)...) body...)`: each binding's value checked where
/// the bindings before it are bound, as nested `let`s.
pub(crate) fn check_let_star(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let Some(((bindings, _), body)) = args.split_first() else {
        return Err(Error::TypeError("let*: (let* ((name val)...) body...)".into()));
    };
    let bindings = heap.list_to_vec(*bindings)?;
    let_star(cx, heap, locals, &bindings, body, expected, loc)
}

/// The `let*` of `bindings` around `body`.
fn let_star(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    bindings: &[Value],
    body: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let Some((binding, later)) = bindings.split_first() else {
        let (body, ty) = check_body(cx, heap, locals, body, expected)?;
        return Ok(Typed { node: Node::Let { binds: Vec::new(), body }, ty, loc });
    };
    let pair = heap.list_to_vec_locs(*binding)?;
    let [(name, name_loc), (value, value_loc)] = pair.as_slice() else {
        return Err(Error::TypeError("let*: binding must be (name val)".into()));
    };
    let Value::Symbol(name) = *name else {
        return Err(Error::TypeError("let*: binding name must be a symbol".into()));
    };
    let value = check_expr(cx, heap, locals, *value, None, value_loc.clone());
    let value = cx.recovered(value, value_loc.clone())?;
    let inner_locals = locals.bind(vec![(name, value.ty.clone(), name_loc.clone())])?;
    let inner = typelisp_mem::grow_stack(|| let_star(cx, heap, &inner_locals, later, body, expected, None))?;
    let ty = inner.ty.clone();
    Ok(Typed { node: Node::Let { binds: vec![(name, value)], body: vec![inner] }, ty, loc })
}

/// `(progn form...)`: a `let` with no bindings.
pub(crate) fn check_progn(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let (body, ty) = check_body(cx, heap, locals, args, expected)?;
    Ok(Typed { node: Node::Let { binds: Vec::new(), body }, ty, loc })
}
