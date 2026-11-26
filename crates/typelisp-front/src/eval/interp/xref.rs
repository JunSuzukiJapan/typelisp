//! Who calls a function: the REPL's `:who-calls`, answered from the checked
//! bodies the interpreter holds, which name every function and method they
//! call by its definition — a generic one by the specialization they call,
//! which says what it is of.

use typelisp_mem::specialization::{specialized, Generic};
use typelisp_mem::{wk, Heap, SymRef, Value};

use crate::check::core;
use crate::check::CompileTarget;

use super::Interp;

impl Interp {
    /// The functions and methods whose bodies call `target` or take it as a
    /// value, by the name `trace` gives them, ordered by that name.
    pub fn callers_of(&self, heap: &Heap, target: &CompileTarget) -> Vec<String> {
        let mut callers: Vec<String> = self
            .root
            .borrow()
            .every_fn()
            .filter(|f| f.lambda.is_none() && f.body.iter().any(|form| mentions(heap, *form, target)))
            .map(|f| f.target.written_name())
            .collect();
        callers.sort();
        callers.dedup();
        callers
    }
}

/// Whether the checked form `form` calls `target` or takes it as a value.
/// Walked with a stack of its own: a body nests as deep as its source does.
fn mentions(heap: &Heap, form: Value, target: &CompileTarget) -> bool {
    let mut pending = vec![form];
    while let Some(v) = pending.pop() {
        if !matches!(v, Value::Cons(_)) {
            continue;
        }
        match core::op_wk(heap, v) {
            // Data, not code: a quoted list that looks like a call is none.
            Some(wk::QUOTE) => continue,
            Some(wk::CALL | wk::FNREF) => {
                if let (CompileTarget::Fn(path), Some(called)) = (target, core::definition_field(heap, v, 0)) {
                    let of_it = |g| matches!(g, Generic::Fn(ns, name) if ns == path.ns() && name == path.name_symbol());
                    if called == *path || specialization_of(called.name_symbol()).is_some_and(of_it) {
                        return true;
                    }
                }
            }
            Some(wk::ASSOC | wk::METHODREF) => {
                if let (CompileTarget::Method(owner, method), Some(ty), Some(Value::Symbol(called))) =
                    (target, core::definition_field(heap, v, 0), core::field(heap, v, 1))
                {
                    let of_it = |g| {
                        matches!(g, Generic::Method(ns, t, m) if ns == owner.ns() && t == owner.name_symbol() && m == *method)
                    };
                    if (ty == *owner && called == *method) || specialization_of(called).is_some_and(of_it) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        if let (Ok(car), Ok(cdr)) = (heap.car(v), heap.cdr(v)) {
            pending.push(cdr);
            pending.push(car);
        }
    }
    false
}

/// What `sym` is a specialization of, when it is one.
fn specialization_of(sym: SymRef) -> Option<Generic> {
    specialized(sym).map(|(generic, _)| generic)
}
