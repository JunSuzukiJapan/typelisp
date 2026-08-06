//! The default execution path: a tree-walking interpreter over the checker's
//! typed AST. See [`interp::Interp`] for the entry point and [`value::RtValue`]
//! for the runtime value representation.

mod value;
pub(crate) mod format;
pub(crate) mod interp;
pub(crate) mod pprint;
pub(crate) mod scope;
pub(crate) mod stream;

pub use value::EvalError;
pub use interp::{llvm_module_of, llvm_value_of, Interp};
