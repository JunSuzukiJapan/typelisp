pub mod compile;
pub mod compiler;
pub mod errors;
pub mod fasl;
mod name_lexer;
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
pub use prelude::load as load_prelude;
// interp-closure removal: every island load goes through the native AOT
// loader (`load_aot`), so nothing tree-walks the island's own bodies — the
// prerequisite for deleting interpreted closures (Stage 8a wired this;
// Stage 8c removed the interpreted `compiler::load` entirely). `load_compiler`
// is kept as the historical name for the one loader there is.
pub use compiler::load_aot as load_compiler;
pub use compiler::load_aot as load_compiler_aot;
