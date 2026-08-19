//! The typelisp *backend*: the compiler island's source ([`compiler`]) and
//! the LLVM plumbing that turns checked programs into machine code
//! ([`compile`]).
//!
//! The front end — reader, checker, interpreter, module loader, prelude
//! source — is the `typelisp-front` crate, re-exported below under the module
//! paths it has always had here, so `typelisp::Heap`, `typelisp::check::core`
//! and friends keep naming the same things from outside. See that crate's doc
//! comment for why the split exists.

pub mod compile;
pub mod compiler;

pub use typelisp_front::{check, dump, errors, eval, mem, owned_form, prelude, project, read, type_key, types};

pub use typelisp_front::errors::*;
pub use typelisp_front::read::*;
pub use typelisp_front::mem::*;
pub use typelisp_front::types::*;
pub use typelisp_front::check::*;
pub use typelisp_front::eval::*;
// The two LLVM-handle accessors moved to the backend with the rest of the
// island's IR builders; re-exported here under the names they have always had
// from outside this crate (tests inspect generated IR through them).
pub use compile::llvm_builtins::{llvm_module_of, llvm_value_of};
// The prelude's source is front-end; installing its committed native bodies
// is not — so the loader that does both lives with the backend.
pub use compile::prelude_bootstrap::load as load_prelude;
// interp-closure removal: every island load goes through the native AOT
// loader (`load_aot`), so nothing tree-walks the island's own bodies — the
// prerequisite for deleting interpreted closures (Stage 8a wired this;
// Stage 8c removed the interpreted `compiler::load` entirely). `load_compiler`
// is kept as the historical name for the one loader there is.
pub use compiler::load_aot as load_compiler;
pub use compiler::load_aot as load_compiler_aot;
