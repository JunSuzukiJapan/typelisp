//! The Common Lisp reader: source text -> [`typelisp_mem::Value`].
//!
//! # Why this is a crate and not a module
//!
//! It used to be `typelisp::read`, which meant a compiled function calling
//! `read` could not be compiled at all: the lowering would have had to name
//! an `rt_*` shim, and the shim would have had to reach into `typelisp`,
//! which depends on the runtime rather than the other way round. The same
//! move `typelisp-print` made, for the same reason — and the same crate (not
//! module) boundary, so a program that never reads does not link the reader:
//! see `typelisp_print`'s crate doc comment for the measurements behind that.
//!
//! Nothing was lost by moving it down. The reader knows about characters,
//! cons cells and `Loc`s; it has never known about types, scopes or
//! evaluation.

pub mod name_lexer;
pub mod reader;
pub mod runtime;
pub mod shim;

pub use reader::*;
