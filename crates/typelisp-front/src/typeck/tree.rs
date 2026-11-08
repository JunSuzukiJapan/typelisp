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
    /// A call to the function `path`.
    Call { path: Path, args: Vec<Typed> },
    /// `let`: the bindings, each checked outside all of them, then the body
    /// in order. With no bindings it is `progn`.
    Let { binds: Vec<(SymRef, Typed)>, body: Vec<Typed> },
    If { cond: Box<Typed>, then: Box<Typed>, els: Box<Typed> },
}
