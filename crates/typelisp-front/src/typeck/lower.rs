//! Lowering: the typed tree ([`super::tree`]) to core IR.
//!
//! One arm per kind of node. Lowering decides nothing a check depends on: by
//! the time a tree is lowered, every type in it is settled. The core forms
//! are built by [`CoreForms`], which the current checker builds with too.

use crate::check::core;
use crate::check::forms;
use crate::check::lower::{Arg, CoreForms};
use crate::{Error, Heap, Value};

use super::tree::{Node, Typed};

/// `t`'s core form, unrooted. It carries `t`'s position on its first cell.
pub(crate) fn lower(heap: &mut Heap, defs: &impl CoreForms, t: &Typed) -> Result<Value, Error> {
    let form = typelisp_mem::grow_stack(|| lower_node(heap, defs, &t.node))?;
    if let (Value::Cons(cell), Some(loc)) = (form, &t.loc) {
        if heap.cons_loc(form).is_none() {
            heap.set_cons_loc(cell, loc.clone());
        }
    }
    Ok(form)
}

/// `t` lowered and rooted on `heap`'s root stack.
fn sub(heap: &mut Heap, defs: &impl CoreForms, t: &Typed) -> Result<Value, Error> {
    let form = lower(heap, defs, t)?;
    heap.push_root(form);
    Ok(form)
}

fn lower_node(heap: &mut Heap, defs: &impl CoreForms, node: &Node) -> Result<Value, Error> {
    match node {
        Node::Literal { tag, value } => match value {
            Some(v) => core::tagged(heap, *tag, &[*v]),
            None => core::tagged(heap, *tag, &[]),
        },
        Node::Local { name } => forms::var_form(heap, *name),
        Node::Call { path, args } => {
            let mut lowered = Vec::with_capacity(args.len());
            for a in args {
                lowered.push(Arg { ty: &a.ty, form: sub(heap, defs, a)? });
            }
            defs.call_form(heap, path, &lowered)
        }
        Node::Let { binds, body } => {
            let mut lowered = Vec::with_capacity(binds.len());
            for (name, t) in binds {
                lowered.push((*name, t.ty.clone(), sub(heap, defs, t)?));
            }
            let body = body.iter().map(|t| sub(heap, defs, t)).collect::<Result<Vec<_>, _>>()?;
            defs.let_form(heap, &lowered, &body)
        }
        Node::If { cond, then, els } => {
            let cond = sub(heap, defs, cond)?;
            let then = sub(heap, defs, then)?;
            let els = sub(heap, defs, els)?;
            forms::if_form(heap, cond, then, els)
        }
    }
}
