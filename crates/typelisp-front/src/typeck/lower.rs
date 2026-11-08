//! Lowering: the typed tree ([`super::tree`]) to core IR.
//!
//! One arm per kind of node. Lowering decides nothing a check depends on: by
//! the time a tree is lowered, every type in it is settled. The core forms
//! are built by [`CoreForms`], which the current checker builds with too.

use crate::check::core;
use crate::check::forms;
use crate::check::lower::{Arg, CoreForms};
use crate::types::Type;
use crate::{Error, Heap, Value};

use super::generic::Requests;
use super::registry::Registry;
use super::subst::Subst;
use super::tree::{Node, Typed};

/// What lowering one tree needs: the definitions, the substitution that
/// makes a generic body one specialization (empty for any other tree), and
/// the specializations the form asks for.
pub(crate) struct Lowering<'a> {
    pub(crate) reg: &'a Registry,
    pub(crate) subst: &'a Subst,
    pub(crate) requests: &'a mut Requests,
}

impl Lowering<'_> {
    /// `ty` as this lowering writes it.
    pub(crate) fn settle(&self, ty: &Type) -> Type {
        self.subst.apply(ty)
    }
}

/// `t`'s core form, unrooted. It carries `t`'s position on its first cell.
pub(crate) fn lower(heap: &mut Heap, lw: &mut Lowering, t: &Typed) -> Result<Value, Error> {
    let form = typelisp_mem::grow_stack(|| lower_node(heap, lw, t))?;
    if let (Value::Cons(cell), Some(loc)) = (form, &t.loc) {
        if heap.cons_loc(form).is_none() {
            heap.set_cons_loc(cell, loc.clone());
        }
    }
    Ok(form)
}

/// `t` lowered and rooted on `heap`'s root stack.
pub(crate) fn lower_rooted(heap: &mut Heap, lw: &mut Lowering, t: &Typed) -> Result<Value, Error> {
    let form = lower(heap, lw, t)?;
    heap.push_root(form);
    Ok(form)
}

fn lower_node(heap: &mut Heap, lw: &mut Lowering, t: &Typed) -> Result<Value, Error> {
    match &t.node {
        Node::Literal { tag, value } => match value {
            Some(v) => core::tagged(heap, *tag, &[*v]),
            None => core::tagged(heap, *tag, &[]),
        },
        Node::Local { name } => forms::var_form(heap, *name),
        Node::Call { path, targs, args } => {
            let path = if targs.is_empty() {
                *path
            } else {
                let targs = targs.iter().map(|t| lw.settle(t)).collect();
                lw.requests.ask_fn(path, targs)
            };
            let mut lowered = Vec::with_capacity(args.len());
            for a in args {
                lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
            }
            let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
            lw.reg.call_form(heap, &path, &lowered)
        }
        Node::MethodCall { owner, method, targs, args } => {
            let method = if targs.is_empty() {
                *method
            } else {
                let targs = targs.iter().map(|t| lw.settle(t)).collect();
                lw.requests.ask_method(owner, *method, targs)
            };
            let mut lowered = Vec::with_capacity(args.len());
            for a in args {
                lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
            }
            let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
            let ret = lw.settle(&t.ty);
            lw.reg.assoc_form(heap, owner, method, true, &ret, &lowered)
        }
        Node::Construct { owner, targs, variant, mutable, field_tys, args } => {
            let targs: Vec<Type> = targs.iter().map(|t| lw.settle(t)).collect();
            let field_tys: Vec<Type> = field_tys.iter().map(|t| lw.settle(t)).collect();
            let args = args.iter().map(|a| lower_rooted(heap, lw, a)).collect::<Result<Vec<_>, _>>()?;
            lw.reg.construct_form(heap, owner, &targs, *variant, *mutable, &field_tys, &args)
        }
        Node::FieldGet { obj, index } => {
            let obj = lower_rooted(heap, lw, obj)?;
            let field_ty = lw.settle(&t.ty);
            lw.reg.field_get_form(heap, obj, *index, &field_ty)
        }
        Node::FieldSet { obj, index, field_ty, value } => {
            let obj = lower_rooted(heap, lw, obj)?;
            let value = lower_rooted(heap, lw, value)?;
            let field_ty = lw.settle(field_ty);
            lw.reg.field_set_form(heap, obj, *index, &field_ty, value)
        }
        Node::Let { binds, body } => {
            let mut lowered = Vec::with_capacity(binds.len());
            for (name, t) in binds {
                lowered.push((*name, lw.settle(&t.ty), lower_rooted(heap, lw, t)?));
            }
            let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
            lw.reg.let_form(heap, &lowered, &body)
        }
        Node::If { cond, then, els } => {
            let cond = lower_rooted(heap, lw, cond)?;
            let then = lower_rooted(heap, lw, then)?;
            let els = lower_rooted(heap, lw, els)?;
            forms::if_form(heap, cond, then, els)
        }
    }
}
