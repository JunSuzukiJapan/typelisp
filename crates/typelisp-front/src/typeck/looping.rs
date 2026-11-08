//! `loop`, and `break` and `return`, which leave it.
//!
//! `(loop body...)` runs its body again and again. Its value is what a
//! `(return value)` in it gives (`()` for `(return)` and `(break)`), every
//! exit giving one type; a loop nothing leaves never returns, and is `!`.
//! The exits are met while the body is checked, so the loop's type is a hole
//! they solve.

use crate::check::loop_dsl::is_dsl;
use crate::types::Type;
use crate::{Error, Heap, Loc, Value};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// A loop being checked: its type, and whether anything leaves it with a
/// value (an exit whose value never returns leaves it with none).
pub(crate) struct LoopExit {
    ty: Type,
    left: bool,
}

/// `(loop body...)` — `args` without the `loop`.
pub(crate) fn check_loop(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let raw: Vec<Value> = args.iter().map(|(v, _)| *v).collect();
    if is_dsl(&raw) {
        unimplemented!("typeck: the extended `loop` (`:for`, `:collect` ...)")
    }
    let hole = cx.holes.borrow_mut().fresh();
    cx.loops.borrow_mut().push(LoopExit { ty: hole.clone(), left: false });
    let body: Result<Vec<Typed>, Error> = args.iter().map(|(v, l)| check_expr(cx, heap, locals, *v, None, l.clone())).collect();
    let exit = cx.loops.borrow_mut().pop().expect("pushed above");
    let ty = if exit.left { hole } else { Type::Never };
    Ok(Typed { node: Node::Loop { body: body? }, ty, loc })
}

/// `(break)`: leave the nearest loop with `()`.
pub(crate) fn check_break(cx: &Cx, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    if !args.is_empty() {
        return Err(Error::TypeError("break: (break), takes no arguments".into()));
    }
    leave_with_unit(cx)?;
    Ok(Typed { node: Node::Break, ty: Type::Never, loc })
}

/// The nearest loop left with `()`, by a `(break)` or a `(return)`.
fn leave_with_unit(cx: &Cx) -> Result<(), Error> {
    let mut loops = cx.loops.borrow_mut();
    let Some(exit) = loops.last_mut() else {
        return Err(Error::TypeError("break/return: not inside a loop".into()));
    };
    let mut holes = cx.holes.borrow_mut();
    if holes.unify(&exit.ty, &Type::Unit).is_err() {
        return Err(Error::TypeError(format!("branches have incompatible types: `{}` vs `()`", holes.shown(&exit.ty))));
    }
    exit.left = true;
    Ok(())
}

/// `(return)` or `(return value)`: leave the nearest loop with `value`, or
/// `()`.
pub(crate) fn check_return(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    if args.len() > 1 {
        return Err(Error::TypeError("return: (return) or (return value)".into()));
    }
    let Some(ty) = cx.loops.borrow().last().map(|e| e.ty.clone()) else {
        return Err(Error::TypeError("return: not inside a loop".into()));
    };
    let value = match args.first() {
        Some((v, l)) => {
            let value = check_expr(cx, heap, locals, *v, Some(&ty), l.clone())?;
            if cx.resolve(&value.ty) != Type::Never {
                cx.loops.borrow_mut().last_mut().expect("seen above").left = true;
            }
            Some(value)
        }
        None => {
            leave_with_unit(cx)?;
            None
        }
    };
    Ok(Typed { node: Node::Return { value: value.map(Box::new) }, ty: Type::Never, loc })
}
