//! The default execution path: a tree-walking interpreter over the checker's
//! typed AST. See [`interp::Interp`] for the entry point and [`value::RtValue`]
//! for the runtime value representation.

mod value;
mod interp;

pub use value::{Closure, EvalError, RtValue, StructData};
pub use interp::Interp;
