//! `loop`, and `break` and `return`, which leave it.
//!
//! `(loop body...)` runs its body again and again. Its value is what a
//! `(return value)` in it gives (`()` for `(return)` and `(break)`), every
//! exit giving one type; a loop nothing leaves never returns, and is `!`.
//! The exits are met while the body is checked, so the loop's type is a hole
//! they solve.

use crate::check::loop_dsl::{self, is_dsl, Acc, Verdict};
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::unify::Kind;
use super::tree::{Node, Typed};

/// A loop being checked: its type, and whether anything leaves it with a
/// value (an exit whose value never returns leaves it with none).
pub(crate) struct LoopExit {
    ty: Type,
    left: bool,
}

/// `(loop body...)` — `args` without the `loop`.
pub(crate) fn check_loop(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let raw: Vec<Value> = args.iter().map(|(v, _)| *v).collect();
    if is_dsl(&raw) {
        return check_extended(cx, heap, locals, args, expected, loc);
    }
    let hole = cx.holes.borrow_mut().fresh();
    cx.loops.borrow_mut().push(LoopExit { ty: hole.clone(), left: false });
    let body: Result<Vec<Typed>, Error> = args.iter().map(|(v, l)| check_expr(cx, heap, locals, *v, None, l.clone())).collect();
    let exit = cx.loops.borrow_mut().pop().expect("pushed above");
    let ty = if exit.left { hole } else { Type::Never };
    Ok(Typed { node: Node::Loop { body: body? }, ty, loc })
}

/// CL's extended `loop` (`(loop :for x :in xs :collect x)`), written as the
/// simple loop and the bindings it is (`loop_dsl::build`, the old
/// checker's), which is checked in its place.
///
/// What each accumulator starts as is a value whose type what is
/// accumulated decides: an empty `Vector`, `none`, or a zero of any numeric
/// type (bound, so that the zero is one checked value outside the form).
fn check_extended(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let raw: Vec<Value> = args.iter().map(|(v, _)| *v).collect();
    let locs: Vec<Option<Loc>> = args.iter().map(|(_, l)| l.clone()).collect();
    let plan = loop_dsl::parse(heap, &raw, &locs)?;
    let mut zeros: Vec<(SymRef, Typed)> = Vec::new();
    let mut acc_inits = Vec::with_capacity(plan.accs.len());
    for acc in &plan.accs {
        let init = match acc.acc {
            Acc::Collect | Acc::Append => call_of(heap, &Path::builtin(wk::VECTOR), "new")?,
            Acc::Maximize | Acc::Minimize => call_of(heap, &Path::builtin(wk::OPTION), "none")?,
            Acc::Count => Value::Int(0),
            Acc::Sum => {
                let zero = cx.temp();
                let ty = cx.number_hole(Kind::Number);
                zeros.push((zero, Typed { node: Node::Number { value: Value::Int(0) }, ty, loc: None }));
                Value::Symbol(zero)
            }
        };
        acc_inits.push((acc.name, init));
    }
    let thereis_none = match plan.verdict {
        Some(Verdict::Optional) => Some(call_of(heap, &Path::builtin(wk::OPTION), "none")?),
        _ => None,
    };
    let built = loop_dsl::build(heap, &plan, &loop_dsl::Resolved { acc_inits, thereis_none })?;
    heap.push_root(built);
    if zeros.is_empty() {
        return check_expr(cx, heap, locals, built, expected, loc);
    }
    let inner = locals.bind(zeros.iter().map(|(n, t)| (*n, t.ty.clone(), None)).collect())?;
    let body = check_expr(cx, heap, &inner, built, expected, loc.clone())?;
    let ty = body.ty.clone();
    Ok(Typed { node: Node::Let { binds: zeros, body: vec![body] }, ty, loc })
}

/// `(Type::member)`, as the checker writes it, rooted.
fn call_of(heap: &mut Heap, owner: &Path, member: &str) -> Result<Value, Error> {
    let head = crate::check::forms::member_form(heap, owner, member)?;
    heap.push_root(head);
    let call = crate::check::forms::list_from_vec_locs(heap, &[(head, None)])?;
    heap.push_root(call);
    Ok(call)
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
