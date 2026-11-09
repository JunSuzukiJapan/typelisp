//! `if`, `panic` and `unwind-protect`.

use crate::types::Type;
use crate::{Error, Heap, Loc, Value};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// `(if cond then else)`. The two branches have one type; a branch that
/// never returns (`!`) leaves it to the other.
pub(crate) fn check_if(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let [(cond, cond_loc), (then, then_loc), (els, els_loc)] = args else {
        return Err(Error::TypeError("if: (if cond then else)".into()));
    };
    let cond = check_expr(cx, heap, locals, *cond, Some(&Type::Bool), cond_loc.clone())?;
    let then = check_expr(cx, heap, locals, *then, expected, then_loc.clone())?;
    let else_expected = if then.ty == Type::Never { expected } else { Some(&then.ty) };
    let els = check_expr(cx, heap, locals, *els, else_expected, els_loc.clone())?;
    let ty = if then.ty == Type::Never { els.ty.clone() } else { then.ty.clone() };
    Ok(Typed { node: Node::If { cond: Box::new(cond), then: Box::new(then), els: Box::new(els) }, ty, loc })
}

/// `(unwind-protect protected cleanup)`: `protected`'s value, `cleanup` run
/// after it however it leaves. The cleanup's value is dropped, so it may be
/// of any type.
pub(crate) fn check_unwind_protect(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let [(protected, protected_loc), (cleanup, cleanup_loc)] = args else {
        return Err(Error::TypeError("unwind-protect: (unwind-protect protected cleanup)".into()));
    };
    let protected = check_expr(cx, heap, locals, *protected, expected, protected_loc.clone())?;
    let cleanup = check_expr(cx, heap, locals, *cleanup, None, cleanup_loc.clone())?;
    let ty = protected.ty.clone();
    Ok(Typed { node: Node::UnwindProtect { protected: Box::new(protected), cleanup: Box::new(cleanup) }, ty, loc })
}

/// `(panic message)`: stop with the string `message`. It never returns.
pub(crate) fn check_panic(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(message, message_loc)] = args else {
        return Err(Error::TypeError("panic: (panic message)".into()));
    };
    let message = check_expr(cx, heap, locals, *message, Some(&Type::Str), message_loc.clone())?;
    Ok(Typed { node: Node::Panic { message: Box::new(message) }, ty: Type::Never, loc })
}
