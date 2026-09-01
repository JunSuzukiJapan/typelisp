//! The GC-managed cons heap and its error type, extracted into their own
//! crate (no `inkwell`/LLVM dependency at all) so [`typelisp-rt`](../typelisp_rt/index.html)
//! — the shared Rust-only runtime library AOT-compiled executables link
//! against — can depend on just this, instead of the whole `typelisp` crate
//! (which embeds all of LLVM via `inkwell`; linking *that* into every tiny
//! AOT output is what Stage 1 found doesn't work — see
//! `docs/TODO.md`'s "Sexpr表現 + Match/Construct/共有Rustライブラリ
//! 実装計画" section). The main `typelisp` crate re-exports everything here
//! unchanged (`src/mem.rs`/`src/errors.rs` are now thin `pub use` shims), so
//! no call site elsewhere in that crate had to change.

pub mod errors;
pub mod heap;
pub mod symbols;
pub mod value;

pub use errors::{Error, Loc};
pub use heap::{base_type_key, inner_type_key, Heap, RootScope};
pub use symbols::{wk, NsId, SymRef, Symbol, BUILTIN_SYMBOLS, NOT_WELL_KNOWN};
pub use value::{BoxId, ConsRef, FloatBox, PathId, StrId, TypeKeyId, Value, BUILTIN_TYPE_KEYS};
