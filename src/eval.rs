//! The default execution path: a tree-walking interpreter over the checker's
//! typed AST. See [`interp::Interp`] for the entry point and [`value::RtValue`]
//! for the runtime value representation.

mod value;
pub(crate) mod interp;
pub(crate) mod scope;

pub use value::{EvalError, RtValue};
pub use interp::Interp;
