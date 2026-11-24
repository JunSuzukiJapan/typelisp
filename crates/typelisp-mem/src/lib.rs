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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod collections;
pub mod errors;
pub mod fixed;
pub mod float;
pub mod heap;
pub mod host;
pub mod int_arg;
pub mod name_view;
pub mod names;
pub mod option;
pub mod specialization;
pub mod stack;
pub mod symbols;
pub mod sync;
pub mod tagged;
pub mod tuple;
pub mod type_form;
pub mod type_shape;
pub mod type_words;
pub mod value;

pub use errors::{Error, Loc, LIBRARY_FILE};
pub use heap::{FrameRaw, Heap, RootScope, RootStackId};
pub use name_view::NameView;
pub use int_arg::IntArg;
pub use names::{ConcreteNames, TypeKey};
pub use stack::grow_stack;
pub use option::{is_niched_option, option_payload_niches, option_prints_wrapped};
pub use symbols::{wk, NsId, StdModule, SymRef, Symbol, BUILTIN_SYMBOLS, NOT_WELL_KNOWN};
pub use type_form::{existing_type_of_form, type_key_of_form, type_of_form};
pub use type_shape::{find_type, instantiate, intern_spec, intern_type, spec, type_name, type_shape, TypeShape, TypeSpec};
pub use tuple::{sexpr_tuple_arity, sexpr_tuple_type, tuple_type_arity, TUPLE_MAX_ARITY};
pub use tagged::{decode, encode, fixnum_fits, references_heap, try_encode, FIXNUM_MAX, FIXNUM_MIN, NIL_WORD};
pub use value::{BoxId, ConsRef, FloatBox, NarrowInt, PathId, StrId, TypeId, Value, BUILTIN_TYPES, NAMED_NOT_BUILT};
pub use value::runtime_builds;

/// What std's prelude gives a crate and `core`'s does not: the crate's files
/// bring it in with `use crate::prelude::*`, so they read the same with std
/// and without it.
pub mod prelude {
    pub use alloc::borrow::ToOwned;
    pub use alloc::boxed::Box;
    pub use alloc::format;
    pub use alloc::string::{String, ToString};
    pub use alloc::vec;
    pub use alloc::vec::Vec;
}
