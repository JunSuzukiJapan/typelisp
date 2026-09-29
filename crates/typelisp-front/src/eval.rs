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

pub use value::EvalError;
pub use interp::Interp;

