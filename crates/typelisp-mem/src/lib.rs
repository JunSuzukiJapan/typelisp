//! The GC-managed cons heap and its error type, extracted into their own
//! crate (no `inkwell`/LLVM dependency at all) so [`typelisp-rt`](../typelisp_rt/index.html)
//! — the shared Rust-only runtime library AOT-compiled executables link
//! against — can depend on just this, instead of the whole `typelisp` crate
//! (which embeds all of LLVM via `inkwell`; linking *that* into every tiny
//! AOT output is what Stage 1 found doesn't work — see
//! `docs/dev/implementation-log.md`'s "Sexpr表現 + Match/Construct/共有Rust
//! ライブラリ 実装計画" section). The main `typelisp` crate re-exports everything here
//! unchanged (`src/mem.rs`/`src/errors.rs` are now thin `pub use` shims), so
//! no call site elsewhere in that crate had to change.

pub mod errors;
pub mod heap;
pub mod symbols;
pub mod tagged;
pub mod value;

pub use errors::{Error, Loc};
pub use heap::{base_type_key, inner_type_key, Heap, RootScope, RootStackId};
pub use symbols::{wk, NsId, SymRef, Symbol, BUILTIN_SYMBOLS, NOT_WELL_KNOWN};
pub use tagged::{decode, encode, references_heap};
pub use value::{BoxId, ConsRef, FloatBox, NarrowInt, PathId, StrId, TypeKeyId, Value, BUILTIN_TYPE_KEYS};

/// `v` cut back to `width` bits and re-extended into the 64-bit word both
/// engines carry integers in: sign-extended when `signed`, zero-extended
/// otherwise.
///
/// This is *the* invariant of the integer representation — a value always
/// **is** the number its type names — so it lives at the bottom of the crate
/// stack, below both the heap that stores narrow integers
/// ([`Heap::alloc_narrow`]) and the front end that decides their types
/// (`typelisp_front::types::normalize_int` re-exports this one).
///
/// Written as a shift pair rather than a mask because the mask for a 32-bit
/// width is itself past `i32` — a constant this language cannot write, and
/// the island compiles the identical pair for the same reason.
pub fn normalize_int(v: i64, width: u32, signed: bool) -> i64 {
    let sh = 64 - width;
    if signed {
        (v << sh) >> sh
    } else {
        (((v as u64) << sh) >> sh) as i64
    }
}
