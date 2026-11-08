//! Lowering: the typed tree ([`super::tree`]) to core IR.
//!
//! One arm per kind of node. Lowering decides nothing a check depends on: by
//! the time a tree is lowered, every type in it is settled.

use crate::check::core;
use crate::{Error, Heap, Value};

use super::tree::{Node, Typed};

/// `t`'s core form, unrooted. It carries `t`'s position on its first cell.
pub(crate) fn lower(heap: &mut Heap, t: &Typed) -> Result<Value, Error> {
    let form = match &t.node {
        Node::Literal { tag, value } => match value {
            Some(v) => core::tagged(heap, *tag, &[*v])?,
            None => core::tagged(heap, *tag, &[])?,
        },
    };
    if let (Value::Cons(cell), Some(loc)) = (form, &t.loc) {
        if heap.cons_loc(form).is_none() {
            heap.set_cons_loc(cell, loc.clone());
        }
    }
    Ok(form)
}
