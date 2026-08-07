//! The default execution path: a tree-walking interpreter over the checker's
//! typed AST. See [`interp::Interp`] for the entry point.
//!
//! Runtime values are plain [`Value`](typelisp_mem::Value)s — there is no
//! interpreter-private value type any more, and no `Slot` kind to choose:
//! every binding is a heap cell. What is left in [`value`] is the error type
//! and the binding handle.

mod value;
pub(crate) mod format;
pub(crate) mod interp;
pub(crate) mod pprint;
pub(crate) mod scope;
pub(crate) mod stream;

pub use value::EvalError;
pub use interp::{llvm_module_of, llvm_value_of, Interp};
