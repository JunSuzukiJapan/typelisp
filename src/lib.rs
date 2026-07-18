pub mod compile;
pub mod compiler;
pub mod errors;
pub mod fasl;
mod name_lexer;
pub mod read;
pub mod mem;
pub mod types;
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
// interp-closure removal Stage 8a: every island load now goes through the
// native AOT loader (`load_aot`), so no test or embedder tree-walks the
// island's own bodies anymore — the prerequisite for deleting interpreted
// closures. `load_compiler` is kept as the historical name; the interpreted
// `compiler::load` still exists but is no longer wired to anything (removed
// with the rest of the interp-closure machinery in Stage 8c).
pub use compiler::load_aot as load_compiler;
pub use compiler::load_aot as load_compiler_aot;
