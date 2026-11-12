//! `select`: wait until one of several channel operations can go, and run
//! that arm.
//!
//! `(select ((v (recv ch)) body...) ((send ch x) body...) (:else body...))`.
//! A receive arm binds `v` to the `Option<T>` the receive answers, so a
//! closed channel is an answer too; `:else`, last, runs when nothing is
//! ready. The arms' bodies have one type, as a `match`'s do.
//!
//! Every channel and every value to send is evaluated once, left to right,
//! before anything waits: they are bound by a `let` around the form, under
//! names no program can write.

use crate::check::bound_name;
use crate::types::{path_is_builtin, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::intern;

use super::expr::{check_body, check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, SelectArm, Typed};

const SHAPE: &str = "`select` takes arms: (select ((v (recv ch)) body...) ((send ch x) body...) (:else body...))";

/// `(select arm...)` — `args` without the `select`.
pub(crate) fn check_select(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    if args.is_empty() {
        return Err(Error::TypeError(format!("`select` with no arms would wait forever. {}", SHAPE)));
    }
    let result = match expected {
        Some(e) => e.clone(),
        None => cx.holes.borrow_mut().fresh(),
    };
    let mut returns = false;
    let mut binds: Vec<(typelisp_mem::SymRef, Typed)> = Vec::new();
    let mut arms = Vec::with_capacity(args.len());
    let mut saw_else = false;
    for (arm, arm_loc) in args {
        let parts = heap.list_to_vec_locs(*arm)?;
        let Some(((head, _), rest)) = parts.split_first() else {
            return Err(Error::TypeError(format!("`select`: an arm cannot be empty. {}", SHAPE)));
        };
        if saw_else {
            return Err(Error::TypeError("`select`: `:else` must be the last arm — every arm after it is unreachable".into()));
        }
        let mut body = |cx: &Cx, heap: &mut Heap, locals: &Locals| -> Result<Typed, Error> {
            let (body, ty) = check_body(cx, heap, locals, rest, Some(&result))?;
            returns |= cx.resolve(&ty) != Type::Never;
            Ok(Typed { node: Node::Let { binds: Vec::new(), body }, ty, loc: arm_loc.clone() })
        };
        if matches!(head, Value::Symbol(id) if id.is(wk::KW_ELSE)) {
            saw_else = true;
            arms.push(SelectArm::Else { body: body(cx, heap, locals)? });
            continue;
        }
        let op = heap.list_to_vec(*head).map_err(|_| Error::TypeError(SHAPE.to_string()))?;
        match op.first() {
            // `((send ch x) body...)`
            Some(Value::Symbol(s)) if s.is(wk::SEND) => {
                let [_, ch, value] = op[..] else {
                    return Err(Error::TypeError(format!("`select`: a send arm is (send ch value). {}", SHAPE)));
                };
                let chan = check_expr(cx, heap, locals, ch, None, arm_loc.clone())?;
                let elem = element(cx, &chan.ty, "send")?;
                let value = check_expr(cx, heap, locals, value, Some(&elem), arm_loc.clone())?;
                let chan = operand(&mut binds, chan);
                let value = operand(&mut binds, value);
                arms.push(SelectArm::Send { elem, chan, value, body: body(cx, heap, locals)? });
            }
            // `((v (recv ch)) body...)`
            Some(Value::Symbol(var)) => {
                bound_name(*var, "select")?;
                let var_at = heap.list_to_vec_locs(*head)?.into_iter().next().and_then(|(_, l)| l);
                let recv = match op[..] {
                    [_, inner] => heap.list_to_vec(inner).map_err(|_| Error::TypeError(SHAPE.to_string()))?,
                    _ => return Err(Error::TypeError(format!("`select`: a receive arm is (v (recv ch)). {}", SHAPE))),
                };
                let [Value::Symbol(r), ch] = recv[..] else {
                    return Err(Error::TypeError(format!("`select`: a receive arm is (v (recv ch)). {}", SHAPE)));
                };
                if !r.is(wk::RECV) {
                    return Err(Error::TypeError(format!("`select`: a receive arm is (v (recv ch)). {}", SHAPE)));
                }
                let chan = check_expr(cx, heap, locals, ch, None, arm_loc.clone())?;
                let elem = element(cx, &chan.ty, "recv")?;
                let bound = Type::Named(Path::builtin(wk::OPTION), vec![elem]);
                let chan = operand(&mut binds, chan);
                let arm_locals = locals.bind(vec![(*var, bound.clone(), var_at)])?;
                arms.push(SelectArm::Recv { var: *var, bound, chan, body: body(cx, heap, &arm_locals)? });
            }
            _ => return Err(Error::TypeError(format!("`select`: an arm is either (v (recv ch)) or (send ch value). {}", SHAPE))),
        }
    }
    if arms.iter().all(|a| matches!(a, SelectArm::Else { .. })) {
        return Err(Error::TypeError("`select` needs at least one channel arm — one with only `:else` is just its body".into()));
    }
    // Arms that all never return leave nothing to decide the type by.
    let ty = if returns { cx.resolve(&result) } else { Type::Never };
    let select = Typed { node: Node::Select { arms }, ty: ty.clone(), loc: loc.clone() };
    Ok(Typed { node: Node::Let { binds, body: vec![select] }, ty, loc })
}

/// `t` bound by the `let` around the `select`, and the local it is bound
/// to there.
fn operand(binds: &mut Vec<(typelisp_mem::SymRef, Typed)>, t: Typed) -> Typed {
    let name = intern(&format!("select operand {}", binds.len()));
    let local = Typed { node: Node::Local { name }, ty: t.ty.clone(), loc: t.loc.clone() };
    binds.push((name, t));
    local
}

/// `T` of a `Chan<T>`.
fn element(cx: &Cx, ty: &Type, what: &str) -> Result<Type, Error> {
    // A number literal is its default type here.
    match cx.with_default(ty) {
        Type::Named(p, args) if path_is_builtin(&p, wk::CHAN) && args.len() == 1 => Ok(args[0].clone()),
        other => Err(Error::TypeError(format!("select: `{}` needs a Chan<T>, and this is `{}`", what, other))),
    }
}
