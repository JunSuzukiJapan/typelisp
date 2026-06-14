//! Managed memory: a fixed cons-cell arena with mark-sweep garbage collection.
//!
//! See [`heap::Heap`] for the design. The public surface is safe; raw pointers
//! are encapsulated inside [`value::ConsRef`] and never escape this module.

pub mod value;
pub mod heap;

pub use value::{ConsRef, PathId, StrId, SymId, Value};
pub use heap::Heap;
