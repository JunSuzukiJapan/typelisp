//! `(task (f args...))` and `(thread (f args...))`: the call run as a task
//! of its own, or on an OS thread of its own. The value is a handle on it,
//! `Task<T>` or `Thread<T>` for the call's `T`.
//!
//! What starts it has to be a call: the call is checked as one would be
//! without the form, and it has to lower to one.

use crate::check::analyze::{SPECIAL_FORM_HEADS, TOPLEVEL_FORM_HEADS};
use crate::check::registry::{task_of, thread_of};
use crate::{wk, Error, Heap, Loc, Value};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// What a malformed form is told.
pub(crate) fn shape(tag: u32) -> String {
    let name = form_name(tag);
    format!("`{0}` takes a call form: ({0} (f args...)). To run an arbitrary body, call a lambda: ({0} ((lambda () RetType body...)))", name)
}

/// `task` or `thread`, as `tag` says.
pub(crate) fn form_name(tag: u32) -> &'static str {
    if tag == wk::TASK { "task" } else { "thread" }
}

/// `(task call)` or `(thread call)`, `tag` saying which — `args` without
/// the head.
pub(crate) fn check_spawn(cx: &Cx, heap: &mut Heap, locals: &Locals, tag: u32, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(call, call_loc)] = args else { return Err(Error::TypeError(shape(tag))) };
    let Ok(head) = heap.car(*call) else { return Err(Error::TypeError(shape(tag))) };
    // The real problem, named before the special form complains about a
    // position it was never going to be allowed in.
    if let Value::Symbol(id) = head {
        if SPECIAL_FORM_HEADS.contains(&id.well_known()) || TOPLEVEL_FORM_HEADS.contains(&id.well_known()) {
            return Err(Error::TypeError(format!("`{}` cannot start `{}`. {}", form_name(tag), id.spelling(), shape(tag))));
        }
    }
    let call = check_expr(cx, heap, locals, *call, None, call_loc.clone())?;
    // The task can outlive the `unsafe` that owns the memory.
    let passed: Vec<&Typed> = match &call.node {
        Node::Call { args, .. } | Node::MethodCall { args, .. } | Node::TraitCall { args, .. } | Node::DynCall { args, .. } => args.iter().collect(),
        Node::Apply { callee, args } => std::iter::once(&**callee).chain(args).collect(),
        _ => Vec::new(),
    };
    if passed.iter().any(|a| super::c_memory::mentions_typed_ptr(&cx.resolve(&a.ty))) {
        return Err(Error::TypeError(format!(
            "a typed pointer cannot be passed to `{}`: the task can run after the `unsafe` that \
             allocated the memory is left and has freed it",
            form_name(tag)
        )));
    }
    let ty = if tag == wk::TASK { task_of(call.ty.clone()) } else { thread_of(call.ty.clone()) };
    Ok(Typed { node: Node::Spawn { tag, call: Box::new(call) }, ty, loc })
}
