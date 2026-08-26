//! The typelisp front end: everything between source text and a running
//! program, with no knowledge of how (or whether) anything gets compiled to
//! machine code.
//!
//! Reader glue ([`read`]), types ([`types`], [`type_key`]), the checker
//! ([`check`]), the tree-walking interpreter ([`eval`]), the file/module
//! loader ([`project`]), and the prelude's source ([`prelude`]).
//!
//! **Why a crate and not a module.** The backend (`typelisp::compile`) is
//! built on this, so the dependency runs front → nothing, backend → front. The
//! separation buys two things. Statically, it is what keeps `inkwell` out of
//! the front end: the interpreter reaches the compiler through a small
//! function-pointer `Backend` it hands out, not by naming backend types.
//! Physically, it is what lets an AOT-compiled program run `eval` — which
//! needs a checker and an interpreter — without every *other* AOT-compiled
//! program paying for them, since the linker pulls a static archive by member
//! and only a crate boundary gives whole objects that nothing references.

pub mod errors;
pub mod owned_form;
pub mod read;
pub mod mem;
pub mod types;
pub mod type_key;
pub mod check;
pub mod eval;
pub mod core_macros;
pub mod prelude;
pub mod project;
pub mod shim;
pub mod dump;

pub use errors::*;
pub use read::*;
pub use mem::*;
pub use types::*;
pub use check::*;
pub use eval::*;
