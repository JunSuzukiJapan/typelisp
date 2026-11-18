//! Global variables (`defvar`/`defconstant`) for compiled code.

use typelisp_abi::{active_heap, decode, encode, fatal};
use typelisp_mem::{Heap, Value};
use crate::shared;

// ---- Global variables (compile `Global`/`SetGlobal` support) ------------
//
// A `defvar`/`defconstant` a compiled function references needs storage
// compiled code can reach — the interpreter's own `Interp.globals` is a
// plain Rust `HashMap` on the other side of the JIT/AOT boundary, invisible
// here. Each such global is instead backed by a permanent GC root
// (`Heap::push_permanent_root`, the same mechanism `rt_push_permanent_sexpr_root`
// already uses for boxed-struct fields — "lives for the process, no matching
// pop" is exactly a global's actual lifetime). `rt_global_new`/`rt_global_get`/
// `rt_global_set` are the read/write surface; see `rt_global_new`'s doc
// comment for why a global's *id* (what `compiler.rs`'s generated code
// addresses it by) and its raw permanent-root position aren't the same
// number in general.

// Global id -> the `Heap::permanent_root` position it landed at, in
// `rt_global_new` call order — see that function's doc comment — now
// `shared::RtShared::globals` (`docs/dev/os-threads-design.md` §5/Phase
// 1d), reached through `shared::rt_shared()`. Thread-local for the same
// cross-test-isolation reason `ACTIVE_HEAP` is (module doc comment); unlike
// `ACTIVE_HEAP`, nothing re-points this automatically on every call, so
// [`reset_global_table`] must be called whenever a fresh `Heap` begins its
// lifetime.

/// Clears this thread's global-id table. Must be called whenever a fresh
/// `Heap`/`Interp` pair begins its lifetime (`typelisp::eval::Interp::new`)
/// — otherwise a stale entry left over from an earlier `Heap` on a reused
/// `cargo test` worker thread would resolve to a permanent-root position
/// that has nothing to do with the *current* `Heap`'s (empty, freshly
/// created) `permanent_roots`. AOT's standalone executable never needs to
/// call this itself: [`rt_heap_init`] runs exactly once, before any
/// `rt_global_new` call, in a process that only ever has the one `Heap`.
pub fn reset_global_table() {
    shared::rt_shared().globals.write().clear();
}

/// Allocates a new global-variable slot holding `v` and returns its id — a
/// small, sequential index that later [`rt_global_get`]/[`rt_global_set`]
/// calls address it by. The Rust-side (non-FFI) counterpart of
/// [`rt_global_new`], for the interpreter itself to promote an
/// already-`defvar`d global to a compiled-global slot before generating IR
/// (`typelisp::eval::Interp::add_compiled_function`) — avoids that caller
/// round-tripping through the `extern "C"` tagged-`i64` ABI just to reach
/// this same bookkeeping, the same reason [`encode`]/[`decode`] are already
/// plain `pub fn`s rather than FFI-only.
///
/// The id is *not* the slot's `Heap::permanent_root` position: unrelated
/// code ([`rt_push_permanent_sexpr_root`], called by `compiler.rs`'s
/// `compile-construct-box-fields` for an ordinary boxed-struct field) also
/// pushes permanent roots, so a global's raw heap-root position isn't
/// predictable at the point `compiler.rs` generates a reference to it. What
/// *is* predictable: only this function ever appends to `GLOBAL_INDEX`, and
/// its callers (this crate's [`rt_global_new`], and `add_compiled_function`)
/// control the order those calls happen in — so `GLOBAL_INDEX`'s own length
/// at the moment of a given call is exactly the id the compiler already
/// expects for it.
pub fn global_new(heap: &mut Heap, v: Value) -> usize {
    let perm_idx = heap.permanent_root_count();
    heap.push_permanent_root(v);
    let shared = shared::rt_shared();
    let mut globals = shared.globals.write();
    globals.push(perm_idx);
    globals.len() - 1
}

/// `args[0]` (its initial tagged value) — see [`global_new`] for the id this
/// assigns.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_global_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_global_new: expected 1 argument");
    }
    let v = decode(*args);
    global_new(active_heap(), v) as i64
}

/// [`global_new`]'s id -> `Heap::permanent_root` position lookup, `pub` for
/// the same cross-crate reason [`encode`]/[`decode`] are: `typelisp::eval::
/// interp`'s `Expr::Global` (an *interpreted* read of a promoted global,
/// bypassing the `rt_global_get`/`encode` FFI round trip entirely since it
/// already holds the live `Heap`) needs the exact same id -> position
/// resolution [`rt_global_get`] does below — reading `Heap::permanent_root(id)`
/// directly would only coincidentally work were `id` and the position always
/// equal, true only when nothing else has pushed a permanent root between
/// two [`global_new`] calls (see [`global_new`]'s own doc comment for a case
/// where that's no longer true: an `Option`/`Result`-typed global whose
/// heap-referencing field also needs its own protecting permanent root).
/// `None` if `id` was never returned by [`global_new`] on this thread since
/// the last [`reset_global_table`] — callers across the FFI boundary
/// (`rt_global_get`/`rt_global_set`) turn that into their own `fatal` abort
/// rather than let a panic try to unwind through compiled native code; a
/// plain Rust-to-Rust caller may still choose to panic on it.
pub fn global_perm_idx(id: usize) -> Option<usize> {
    shared::rt_shared().globals.read().get(id).copied()
}

/// Reads global `id`'s (an [`rt_global_new`] return value) current value.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread; `args` must point to
/// at least 1 valid `i64`; `id` must be a value `rt_global_new` actually
/// returned on this thread since the last [`reset_global_table`].
#[no_mangle]
pub unsafe extern "C" fn rt_global_get(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_global_get: expected 1 argument (the global id)");
    }
    let id = *args as usize;
    let perm_idx = global_perm_idx(id).unwrap_or_else(|| fatal("rt_global_get: unknown global id"));
    encode(active_heap().permanent_root(perm_idx))
}

/// Overwrites global `id`'s value (`args[1]`, tagged) in place
/// (`Heap::set_permanent_root`); returns the new value unchanged, like
/// [`rt_push_sexpr_root`].
///
/// # Safety
///
/// Same as [`rt_global_get`], plus `args` must point to at least 2 valid
/// `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_global_set(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_global_set: expected 2 arguments (the global id and its new value)");
    }
    let id = *args as usize;
    let tagged = *args.add(1);
    let perm_idx = global_perm_idx(id).unwrap_or_else(|| fatal("rt_global_set: unknown global id"));
    active_heap().set_permanent_root(perm_idx, decode(tagged));
    tagged
}
