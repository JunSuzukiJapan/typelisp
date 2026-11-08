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
    for (binding, _) in heap.list_to_vec_locs(*bindings)? {
        let pair = heap.list_to_vec_locs(binding)?;
        let [(name, _), (value, value_loc)] = pair.as_slice() else {
            return Err(Error::TypeError("let: binding must be (name val)".into()));
        };
        let Value::Symbol(name) = *name else {
            return Err(Error::TypeError("let: binding name must be a symbol".into()));
        };
        binds.push((name, check_expr(cx, heap, locals, *value, None, value_loc.clone())?));
    }
    let inner = locals.bind(binds.iter().map(|(n, t)| (*n, t.ty.clone())).collect())?;
    let (body, ty) = check_body(cx, heap, &inner, &args[1..], expected)?;
    Ok(Typed { node: Node::Let { binds, body }, ty, loc })
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
