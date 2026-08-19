//! The default execution path: a tree-walking interpreter over the checker's
//! typed AST. See [`interp::Interp`] for the entry point.
//!
//! Runtime values are plain [`Value`](typelisp_mem::Value)s — there is no
//! interpreter-private value type any more, and no `Slot` kind to choose:
//! every binding is a heap cell. What is left in [`value`] is the error type
//! and the binding handle.

mod value;
pub mod crossing;
pub mod interp;
pub mod scope;

/// The CL printer moved to its own crate ([`typelisp_print`]) so a compiled
/// function that calls `format`/`print`/`println`/`pprint` can reach it
/// through an `rt_*` shim — the interpreter cannot be a call target of
/// compiled code, but a crate below it can. Re-exported under the old paths
/// so every `crate::eval::format::…` / `crate::eval::pprint::…` reference
/// keeps working.
pub(crate) use typelisp_print::format;
pub(crate) use typelisp_print::pprint;

pub use value::EvalError;
pub use interp::Interp;

