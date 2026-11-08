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
    /// A call to the method `method` of the type `owner`, at the type
    /// arguments `targs` when it is generic. An instance method's receiver is
    /// the first argument.
    MethodCall { owner: Path, method: SymRef, instance: bool, targs: Vec<Type>, args: Vec<Typed> },
    /// A new value of the type `owner<targs>`: its variant `variant`'s fields,
    /// whose declared types at these arguments are `field_tys`. A struct's is
    /// mutable.
    Construct { owner: Path, targs: Vec<Type>, variant: usize, mutable: bool, field_tys: Vec<Type>, args: Vec<Typed> },
    /// The field `index` of the struct `obj`.
    FieldGet { obj: Box<Typed>, index: usize },
    /// Set the field `index` (declared `field_ty` at the struct's arguments)
    /// of the struct `obj` to `value`.
    FieldSet { obj: Box<Typed>, index: usize, field_ty: Type, value: Box<Typed> },
    /// `let`: the bindings, each checked outside all of them, then the body
    /// in order. With no bindings it is `progn`.
    Let { binds: Vec<(SymRef, Typed)>, body: Vec<Typed> },
    If { cond: Box<Typed>, then: Box<Typed>, els: Box<Typed> },
    /// `match`: the first arm whose pattern matches the scrutinee's value is
    /// run, the names its pattern binds bound.
    Match { scrut: Box<Typed>, arms: Vec<(Pat, Vec<Typed>)> },
}

/// A checked pattern.
#[derive(Debug)]
pub(crate) enum Pat {
    /// `_`: matches anything.
    Wild,
    /// A name: matches anything, and binds the name to it.
    Bind(SymRef),
    /// The variant `variant` of `owner<targs>`, its fields (declared
    /// `field_tys` at these arguments) matched by `subs`.
    Ctor { owner: Path, targs: Vec<Type>, variant: usize, field_tys: Vec<Type>, subs: Vec<Pat> },
}

impl Pat {
    fn map_types(&mut self, f: &impl Fn(&Type) -> Type) {
        if let Pat::Ctor { targs, field_tys, subs, .. } = self {
            for t in targs.iter_mut().chain(field_tys.iter_mut()) {
                *t = f(t);
            }
            subs.iter_mut().for_each(|p| p.map_types(f));
        }
    }
}

/// A tree nested as deep as the program is is dropped with the stack grown
/// as it goes, like every other walk over it.
impl Drop for Typed {
    fn drop(&mut self) {
        let node = std::mem::replace(&mut self.node, Node::Let { binds: Vec::new(), body: Vec::new() });
        typelisp_mem::grow_stack(move || drop(node));
    }
}

impl Typed {
    /// Every type in this tree replaced with `f` of it.
    pub(crate) fn map_types(&mut self, f: &impl Fn(&Type) -> Type) {
        typelisp_mem::grow_stack(|| self.map_types_here(f))
    }

    fn map_types_here(&mut self, f: &impl Fn(&Type) -> Type) {
        self.ty = f(&self.ty);
        match &mut self.node {
            Node::Literal { .. } | Node::Local { .. } => {}
            Node::Call { targs, args, .. } => {
                for t in targs.iter_mut() {
                    *t = f(t);
                }
                args.iter_mut().for_each(|a| a.map_types(f));
            }
            Node::MethodCall { targs, args, .. } => {
                for t in targs.iter_mut() {
                    *t = f(t);
                }
                args.iter_mut().for_each(|a| a.map_types(f));
            }
            Node::Construct { targs, field_tys, args, .. } => {
                for t in targs.iter_mut().chain(field_tys.iter_mut()) {
                    *t = f(t);
                }
                args.iter_mut().for_each(|a| a.map_types(f));
            }
            Node::FieldGet { obj, .. } => obj.map_types(f),
            Node::FieldSet { obj, field_ty, value, .. } => {
                *field_ty = f(field_ty);
                obj.map_types(f);
                value.map_types(f);
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
            Node::Match { scrut, arms } => {
                scrut.map_types(f);
                for (pat, body) in arms {
                    pat.map_types(f);
                    body.iter_mut().for_each(|t| t.map_types(f));
                }
            }
        }
    }

    /// Call `f` on every value this tree holds.
    pub(crate) fn each_value(&self, f: &mut impl FnMut(Value)) {
        typelisp_mem::grow_stack(|| self.each_value_here(f))
    }

    fn each_value_here(&self, f: &mut impl FnMut(Value)) {
        match &self.node {
            Node::Literal { value: Some(v), .. } => f(*v),
            Node::Literal { value: None, .. } | Node::Local { .. } => {}
            Node::Call { args, .. } | Node::MethodCall { args, .. } | Node::Construct { args, .. } => {
                args.iter().for_each(|a| a.each_value(f))
            }
            Node::FieldGet { obj, .. } => obj.each_value(f),
            Node::FieldSet { obj, value, .. } => {
                obj.each_value(f);
                value.each_value(f);
            }
            Node::Let { binds, body } => {
                binds.iter().for_each(|(_, t)| t.each_value(f));
                body.iter().for_each(|t| t.each_value(f));
            }
            Node::If { cond, then, els } => {
                cond.each_value(f);
                then.each_value(f);
                els.each_value(f);
            }
            Node::Match { scrut, arms } => {
                scrut.each_value(f);
                arms.iter().flat_map(|(_, body)| body).for_each(|t| t.each_value(f));
            }
        }
    }
}
