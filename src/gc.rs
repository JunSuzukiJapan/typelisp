//! Precise, non-moving mark-sweep garbage collector for the typelisp runtime.
//!
//! Roots are found precisely via a shadow stack that compiled code maintains
//! (and via registered globals). Only aggregates (String, Vec, struct, closure)
//! are managed; primitives are unboxed and never scanned. The collector never
//! moves objects, so pointers held by compiled code stay valid.
//!
//! M9 implements and tests the runtime in isolation (Rust only). Code
//! generation wires `gc_alloc`/the shadow stack in later milestones.

pub mod types;
pub mod shadow;
pub mod heap;
pub mod api;

pub use api::*;
pub use heap::*;
pub use shadow::*;
pub use types::*;
