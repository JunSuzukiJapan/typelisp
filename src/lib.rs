pub mod compile;
pub mod compiler;
pub mod errors;
pub mod owned_form;
pub mod read;
pub mod mem;
pub mod types;
pub mod type_key;
pub mod check;
pub mod eval;
pub mod prelude;
pub mod project;

pub use errors::*;
pub use read::*;
pub use mem::*;
pub use types::*;
pub use check::*;
pub use eval::*;
// The two LLVM-handle accessors moved to the backend with the rest of the
// island's IR builders; re-exported here under the names they have always had
// from outside this crate (tests inspect generated IR through them).
pub use compile::llvm_builtins::{llvm_module_of, llvm_value_of};
pub use prelude::load as load_prelude;
// interp-closure removal: every island load goes through the native AOT
// loader (`load_aot`), so nothing tree-walks the island's own bodies — the
// prerequisite for deleting interpreted closures (Stage 8a wired this;
// Stage 8c removed the interpreted `compiler::load` entirely). `load_compiler`
// is kept as the historical name for the one loader there is.
pub use compiler::load_aot as load_compiler;
pub use compiler::load_aot as load_compiler_aot;
