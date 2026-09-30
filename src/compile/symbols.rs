//! Names and ids the compile boundary agrees on with the island.
//!
//! Everything here is a *convention*, not a translation: how a user-defined
//! function's LLVM symbol is spelled, how an `rt_llvm_call` operation is
//! numbered, which builtin methods bypass the generic `assoc` path, and the
//! two trait-object id tables a translation is handed. `compiler.rs`'s own
//! Lisp source mangles and re-derives these same strings and numbers, so a
//! change on either side has to be matched on the other — which is exactly
//! why they live in one small module of their own rather than inside the
//! translator that happens to use them most.
//!
//! Extracted from `ast_bridge` when that translator was deleted (Phase 2
//! Stage C): none of this ever touched the typed AST, so none of it went with
//! it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::Path;

/// The two trait-object id tables a translation reads, interned by `Interp`
/// *before* translation starts: both are baked into the emitted IR as
/// constants (`(dyn-new id ...)`, `(dyn-upcast trait-id ...)`), so there is
/// nothing to resolve lazily mid-translation, exactly like [`Ctx::globals`].
///
/// One parameter rather than two because every translation entry point and
/// every nested-scope hand-off needs both, and they are interned together
/// (`Interp::register_dyn_box`).
#[derive(Clone, Copy)]
pub struct DynTables<'a> {
    /// `(concrete type key, trait path)` -> vtable id, for every trait
    /// object this translation boxes (`dyn-new`).
    pub vtables: &'a HashMap<(String, Path), u32>,
    /// Trait path -> trait id, for every supertrait this translation upcasts
    /// to (`dyn-upcast`). The upcast's *target vtable* is not here: it
    /// depends on the value's concrete type and is looked up at run time
    /// (`rt_dyn_upcast`).
    pub trait_ids: &'a HashMap<Path, u32>,
}

/// A process-wide counter for synthesizing unique LLVM symbol names for
/// anonymous functions — every `lambda`/`fnref`-forwarding-wrapper/
/// immediately-invoked-lambda gets one (labels/closures Stage 4), since none
/// of those have a name from user source the way a `labels` def or `defun`
/// does. Process-wide (not per-translation-pass or per-outer-function) so
/// two lambdas compiled into the same shared AOT module — even from two
/// *different* `defun`s — can never collide, with no mangling scheme to get
/// right.
static LAMBDA_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn fresh_lambda_name(prefix: &str) -> String {
    let n = LAMBDA_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}${}", prefix, n)
}

/// A user-defined free function/`labels` sibling's own LLVM symbol name —
/// see [`crate::compile::USER_SYMBOL_PREFIX`]'s doc comment for why this is
/// never `logical_name` unprefixed. The single place [`translate_call`]/
/// [`translate_fnref`]'s embedded `(call ...)`-node name string is built,
/// so it always agrees with whatever `Interp::compile_scc`/
/// `compile::aot` declare/wire the *actual* LLVM function under.
pub fn user_symbol_name(logical_name: &str) -> String {
    format!("{}{}", crate::compile::USER_SYMBOL_PREFIX, logical_name)
}

/// The LLVM symbol a call to `path` must name: the `typelisp-rt` shim, when
/// `path` is a free builtin with no typelisp body
/// ([`crate::compile::externs::builtin_shim`]), and otherwise the mangled
/// name of the compiled function.
///
/// The one place that choice is made. `core_bridge` writes the answer into
/// the node it hands the island, which looks the name up and calls it —
/// there is no second derivation on the island side to disagree with this
/// one.
pub fn callee_symbol_name(path: &crate::Path) -> String {
    match crate::compile::externs::builtin_shim(path) {
        Some(shim) => shim.to_string(),
        None => user_symbol_name(&path.segments().join("::")),
    }
}

/// One definition with a compiled body, named the two ways the compile
/// boundary needs it: as a *node* (what `Interp::resolve_fn_def` and
/// `add_compiled_function` take) and as an *LLVM symbol* (what a call site
/// emits and the linker/JIT resolves).
///
/// Keeping both derivations on one type is the point: a `defun` and a
/// `defmethod` spell each of the two names differently, and every place that
/// builds a precompiled library — the generators in
/// [`crate::compile::bootstrap`] and [`crate::compile::prelude_bootstrap`],
/// and `Interp::install_compiled_library` on the loading side — needs the
/// same pair. Deriving them separately in each is how the island's own
/// `defun`-only, root-path-only shortcut got baked in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompiledItem {
    /// A top-level `defun`, by its fully-qualified path.
    Fn(Path),
    /// A `defmethod`, by its receiver type's fully-qualified path and its own
    /// name.
    Method(Path, String),
}

impl CompiledItem {
    /// The name `Interp::resolve_fn_def`/`add_compiled_function` accept: a
    /// `::`-joined function path, or `type-path::method`. The two are
    /// deliberately indistinguishable as strings — `resolve_fn_def` tries the
    /// method table first and falls through to the function table, so a
    /// module-qualified `defun` and a method never need separate spellings.
    pub fn node_name(&self) -> String {
        match self {
            CompiledItem::Fn(path) => path.to_string(),
            CompiledItem::Method(type_path, method) => format!("{}::{}", type_path, method),
        }
    }

    /// The LLVM symbol its compiled body is defined under — the same string a
    /// *call site* emits for it (`core_bridge::translate_call`'s
    /// `user_symbol_name(&path.segments().join("::"))`, `translate_assoc`'s
    /// `user_method_symbol_name`), so a body and the calls to it can't be
    /// declared under two different names.
    pub fn symbol_name(&self) -> String {
        match self {
            CompiledItem::Fn(path) => user_symbol_name(&path.to_string()),
            CompiledItem::Method(type_path, method) => user_method_symbol_name(type_path, method),
        }
    }
}

/// A user-defined method's own LLVM symbol name — the `assoc`
/// counterpart of [`user_symbol_name`]. `compiler.rs`'s `compile-assoc-user`
/// mangles `type-name`/`method` back into this exact same
/// `tl_type::path::method` string (its own `(append "tl_" (append type-name
/// (append "::" method)))`, where `type-name` is whatever full `::`-joined
/// path [`ast_to_sexpr_scoped`]'s `assoc`/`methodref` arms
/// embedded — never just the type's local segment, or two same-named types
/// in different modules would mangle to the same symbol) before its
/// `get-function` lookup, so this must stay in lockstep with that — see
/// [`crate::compile::USER_SYMBOL_PREFIX`]'s doc comment.
pub fn user_method_symbol_name(type_name: &Path, method: &str) -> String {
    format!("{}{}::{}", crate::compile::USER_SYMBOL_PREFIX, type_name, method)
}

/// `Vector<T>`'s builtin methods that have no compiled `defmethod` body and so
/// must be lowered to a dedicated `vector-op` node (`translate_vector_method`)
/// rather than routed through the generic `assoc` path. Deliberately *not*
/// `iter`: that one is a real prelude `defmethod` (`vector-iter::new`), a
/// normal compiled method, so it stays on the `assoc` path. `HashTable<K,V>`
/// shares the `new`/`get`/`set` names but a different `type_name`, so the
/// [`path_is_builtin`] guard at the call site keeps them apart.
pub const VECTOR_BUILTIN_METHODS: [&str; 6] = ["new", "get", "set", "len", "push", "pop"];

/// `HashTable<K,V>`'s builtin methods lowered to a `hashtable-op` node
/// (`translate_hashtable_method` -> the `rt_hashtable_*` family) rather than
/// the generic `assoc` path — every one except `iter`, `get`, `set`,
/// `remove`, `maphash` and `size`, which are genuine prelude `defmethod`s.
///
/// `get`/`set`/`remove` were here once, as three whole-lookup shims. They
/// could not stay: looking a key up means hashing it and comparing it, and
/// both of those are the key type's own `sxhash`/`equals` — typelisp
/// methods a Rust shim cannot call. The bucket primitives take the hash the
/// prelude computed and an index it found, which is the part this layer can
/// answer on its own.
pub const HASHTABLE_BUILTIN_METHODS: [&str; 11] = [
    "new", "count", "clear", "keys", "values", "entries",
    "bucket-count", "bucket-key", "bucket-value", "bucket-put", "bucket-delete",
];

/// The stable operation id compiled code passes as `rt_llvm_call`'s first
/// argument: FNV-1a over `"type-key::method"`, folded into a tagged-`Sexpr`
/// integer's 61-bit signed payload. A *hash*, not a table index, so the id
/// embedded in a committed compiler-island bitcode artifact (interp-closure
/// removal Stage 3) survives methods being added to or removed from the
/// table in a later build — an index would silently shift. Collisions are
/// checked once at table construction (`interp`'s `llvm_op_table`), which
/// panics rather than dispatching two methods through one id.
///
/// The `<< 3 >> 3` fold is load-bearing, not cosmetic: this id is baked into
/// the `llvm-op` node as a `Sexpr` `Int`, and the *compiled* island reads
/// `Sexpr` ints back in the 3-bit-tagged representation (`raw >> 3`), which
/// would silently drop the top 3 bits of a full-width hash — so a
/// natively-compiled `rt_llvm_call` would pass a truncated id the table (keyed
/// on the full hash) has no entry for, aborting at runtime. Folding here means
/// the id already fits 61 bits, so it round-trips identically through the
/// tagged compiled path and the interpreter's full-width `Value::Int`, and the
/// table (built from this same function) keys on exactly what compiled code
/// passes. (A change to this fold requires regenerating `compiler_island.bc`,
/// whose own native-scope ops carry ids baked by it — the freshness test
/// catches a stale artifact.)
pub fn llvm_op_id(type_key: &str, method: &str) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in type_key.as_bytes().iter().chain(b"::").chain(method.as_bytes()) {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    ((h as i64) << 3) >> 3
}
