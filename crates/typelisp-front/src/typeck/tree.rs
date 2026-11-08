//! The typed tree: what checking a form produces, before it is lowered to
//! core IR ([`super::lower`]).
//!
//! Lowering waits for the whole form to be checked because a type may still
//! hold an inference hole while the form is being checked, and the core IR
//! writes each value's representation, which only a solved type has.

use crate::types::{Path, Type};
use crate::{Loc, Value};
use typelisp_mem::SymRef;

/// A checked expression: what it does, its type, and where it was written.
#[derive(Debug)]
pub(crate) struct Typed {
    pub(crate) node: Node,
    pub(crate) ty: Type,
    pub(crate) loc: Option<Loc>,
}

#[derive(Debug)]
pub(crate) enum Node {
    /// A literal: its core tag and its value (none for the unit value).
    Literal { tag: u32, value: Option<Value> },
    /// A local variable's value.
    Local { name: SymRef },
    /// A call to the function `path`, at the type arguments `targs` when it
    /// is generic.
    Call { path: Path, targs: Vec<Type>, args: Vec<Typed> },
    /// `let`: the bindings, each checked outside all of them, then the body
    /// in order. With no bindings it is `progn`.
    Let { binds: Vec<(SymRef, Typed)>, body: Vec<Typed> },
    If { cond: Box<Typed>, then: Box<Typed>, els: Box<Typed> },
}

impl Typed {
    /// Every type in this tree replaced with `f` of it.
    pub(crate) fn map_types(&mut self, f: &impl Fn(&Type) -> Type) {
        self.ty = f(&self.ty);
        match &mut self.node {
            Node::Literal { .. } | Node::Local { .. } => {}
            Node::Call { targs, args, .. } => {
                for t in targs.iter_mut() {
                    *t = f(t);
                }
                args.iter_mut().for_each(|a| a.map_types(f));
            }
            Node::Let { binds, body } => {
                binds.iter_mut().for_each(|(_, t)| t.map_types(f));
                body.iter_mut().for_each(|t| t.map_types(f));
            }
            Node::If { cond, then, els } => {
                cond.map_types(f);
                then.map_types(f);
                els.map_types(f);
            }
        }
    }

    /// Call `f` on every value this tree holds.
    pub(crate) fn each_value(&self, f: &mut impl FnMut(Value)) {
        match &self.node {
            Node::Literal { value: Some(v), .. } => f(*v),
            Node::Literal { value: None, .. } | Node::Local { .. } => {}
            Node::Call { args, .. } => args.iter().for_each(|a| a.each_value(f)),
            Node::Let { binds, body } => {
                binds.iter().for_each(|(_, t)| t.each_value(f));
                body.iter().for_each(|t| t.each_value(f));
            }
            Node::If { cond, then, els } => {
                cond.each_value(f);
                then.each_value(f);
                els.each_value(f);
            }
        }
    }
}
