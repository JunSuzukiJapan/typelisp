//! Lowering: the analyzed tree ([`Node`]) to core IR
//! (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).
//!
//! One function per kind of node, each calling the `forms` builder for its
//! core form. A node's children are [`Checked`] values whose `form` is
//! already lowered, so lowering a node builds that node's own form only.
//! Lowering decides nothing a check depends on: the types it reads were all
//! settled by the analysis.

use typelisp_mem::{Error, Heap, Value};

use super::core::{Checked, Node};
use super::{fold, forms};

/// `node`'s core form.
pub(super) fn lower(heap: &mut Heap, node: &Node) -> Result<Value, Error> {
    match node {
        Node::Lowered => unreachable!("a `Lowered` expression is built with its form"),
        Node::If { cond, then, els } => lower_if(heap, cond, then, els),
        Node::Panic { message } => forms::panic_form(heap, message.form),
    }
}

/// A literal condition picks its branch now. Both branches were still
/// analyzed and checked: a branch that can never run is still a program that
/// has to be well-typed.
fn lower_if(heap: &mut Heap, cond: &Checked, then: &Checked, els: &Checked) -> Result<Value, Error> {
    match fold::literal_bool(heap, cond.form) {
        Some(true) => Ok(then.form),
        Some(false) => Ok(els.form),
        None => forms::if_form(heap, cond.form, then.form, els.form),
    }
}
