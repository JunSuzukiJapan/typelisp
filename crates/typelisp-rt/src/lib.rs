//! The shared Rust-only runtime library compiled code calls into directly —
//! the destination for builtins that genuinely can't be written in typelisp
//! (direct cons-heap access, raw GC-heap bookkeeping; see the `typelisp`
//! crate's `docs/TODO.md`, "Sexpr表現 + Match/Construct/共有Rustライブラリ
//! 実装計画" section, for the full plan this crate is Stage 0/1 of).
//!
//! A separate crate, depending on nothing but `typelisp-mem` (no
//! `inkwell`/LLVM) — Stage 1 found that linking the *whole* `typelisp` crate
//! (which embeds all of LLVM) into an AOT-compiled executable just to reach
//! these few functions drags in LLVM's entire system-library footprint
//! (`libc++`, zlib, libffi, terminfo, ...) for no benefit. This crate's
//! `staticlib` artifact stays small and dependency-free, which is all
//! `compile-file`'s linker step needs.
//!
//! Every function here is declared under the exact same ABI as a compiled
//! typelisp function (`unsafe extern "C" fn(*const i64, u32) -> i64`,
//! `typelisp::compile::CompiledSignature`) — so the existing cross-function-
//! call wiring (JIT: `add_global_mapping`'s `externals` list in
//! `typelisp::compile::CompiledFn::new`; AOT: ordinary linker symbol
//! resolution against this crate's `staticlib`, see `typelisp::compile::aot`'s
//! `write_executable`) treats a call to one of these exactly like a call to
//! another already-compiled typelisp function — no new call mechanism is
//! needed, only `#[no_mangle]` so the linker sees a plain, unmangled C
//! symbol name.
//!
//! `rt_ping` is Stage 0's deliberately trivial placeholder: it exists only
//! to prove that *some* Rust function defined in this crate is callable
//! from both a JIT-compiled function (via `add_global_mapping`) and an
//! AOT-linked native executable (via the system linker) before any real
//! heap/Sexpr machinery is built on top.

/// Returns `args[0] + 1` if `argc >= 1`, otherwise `0`. No real runtime
/// behavior depends on this — see the module doc comment.
///
/// # Safety
///
/// If `argc >= 1`, `args` must be non-null and point to at least one valid,
/// readable `i64` — exactly what every compiled-function call site already
/// guarantees for its own `args` array.
#[no_mangle]
pub unsafe extern "C" fn rt_ping(args: *const i64, argc: u32) -> i64 {
    if argc >= 1 {
        *args + 1
    } else {
        0
    }
}

// ---- Stage 1: the active `Heap` ---------------------------------------

use std::cell::Cell;

use typelisp_mem::Heap;

// The `Heap` every other `rt_*` function in this crate (from Stage 3
// onward) implicitly operates on — see the module doc comment's framing of
// this as CL/Scheme-style "the heap is one implicit, process-wide thing",
// not something compiled code ever holds or passes a handle to itself.
//
// `thread_local!`, not a plain `static`: `cargo test` runs many tests
// concurrently on different OS threads within *one process*, and each test
// that exercises `compile`/`compile-file` owns its own independent `Heap`.
// A single global pointer would let two tests' compiled code race on each
// other's heap. Each real OS thread only ever drives one `Interp`/`Heap`
// at a time (compiling and running compiled code is synchronous, never
// handed off mid-call to another thread), so a thread-local slot gives the
// same "implicit, no handle passed around" ergonomics with no cross-test
// hazard. A single-threaded AOT-compiled executable (its own OS process)
// just has the one thread's slot — behaves identically to a plain global
// there.
thread_local! {
    static ACTIVE_HEAP: Cell<*mut Heap> = const { Cell::new(std::ptr::null_mut()) };
}

/// Registers `heap` as this thread's active `Heap` for any `rt_*` call that
/// follows. The JIT path (`typelisp::eval::Interp::eval`'s compiled-call
/// dispatch) calls this with its own `heap: &mut Heap` immediately before
/// every call into compiled code — cheap enough (one pointer store) to just
/// always do, rather than trying to detect whether the callee might
/// transitively touch the heap. The AOT path has no Rust caller to do this
/// for it, so [`rt_heap_init`] does the equivalent at process startup
/// instead (see its doc comment).
///
/// `pub`, not `pub(crate)`: the `typelisp` crate is a different crate from
/// this one and has to call this directly — there is no "visible to one
/// specific dependent crate" visibility in Rust narrower than plain `pub`.
pub fn set_active_heap(heap: *mut Heap) {
    ACTIVE_HEAP.with(|cell| cell.set(heap));
}

/// Dereferences the current thread's active `Heap`.
///
/// # Safety
///
/// Some prior call on this thread — [`set_active_heap`] (JIT) or
/// [`rt_heap_init`] (AOT) — must have registered a still-valid `Heap`
/// pointer, and no other reference to that same `Heap` may be live (this
/// produces an exclusive `&mut`). Every `rt_*` function below calls this
/// exactly once per invocation and lets the borrow end with the call, so
/// two `rt_*` calls never alias each other's borrow.
unsafe fn active_heap() -> &'static mut Heap {
    let ptr = ACTIVE_HEAP.with(|cell| cell.get());
    debug_assert!(!ptr.is_null(), "rt_* function called with no active Heap registered on this thread");
    &mut *ptr
}

/// The cons-cell arena size an AOT-compiled executable allocates for itself
/// at startup (see [`rt_heap_init`]) when its `main` doesn't otherwise say
/// — matches the capacity `compile-file` itself already uses for the
/// *compiler's own* (unrelated) `Heap` in `typelisp::compile::aot::compile_file`.
const DEFAULT_AOT_HEAP_CAPACITY: usize = 1 << 16;

/// AOT's counterpart to the JIT path's [`set_active_heap`]: an AOT-compiled
/// executable is its own standalone process with no embedding Rust caller
/// to register a `Heap` for it, so it has to create and register one for
/// itself. `typelisp::compile::aot::build_main_wrapper` emits a call to this
/// as the very first thing the generated `main` does, before calling the
/// file's own `tl_main`.
///
/// Reads the desired capacity from `args[0]` if `argc >= 1`, otherwise uses
/// [`DEFAULT_AOT_HEAP_CAPACITY`]. The allocated `Heap` is deliberately
/// never freed — it must outlive every other compiled function in the
/// process, i.e. live until the OS reclaims everything at process exit, the
/// same trade-off `ClosureBox`/`RtValue::HashTable` already accept for
/// values that outlive their last explicit owner.
///
/// # Safety
///
/// Same as [`rt_ping`]: if `argc >= 1`, `args` must point to at least one
/// valid, readable `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_heap_init(args: *const i64, argc: u32) -> i64 {
    let capacity = if argc >= 1 { *args as usize } else { DEFAULT_AOT_HEAP_CAPACITY };
    let heap = Box::new(Heap::with_capacity(capacity));
    set_active_heap(Box::into_raw(heap));
    0
}

/// Diagnostic-only: the active `Heap`'s live cons-cell count
/// (`Heap::live_count`). Stage 1's proof that the registered `Heap` is
/// genuinely reachable and reflects real state, not a placeholder for any
/// actual language feature — Stage 3 replaces the need for this with real
/// `rt-cons`/`rt-car`/... functions.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread (see [`active_heap`]).
#[no_mangle]
pub unsafe extern "C" fn rt_heap_live_count(_args: *const i64, _argc: u32) -> i64 {
    active_heap().live_count() as i64
}

#[cfg(test)]
mod tests {
    use typelisp_mem::{Heap, Value};

    use super::{rt_heap_init, rt_heap_live_count, rt_ping, set_active_heap};

    #[test]
    fn rt_ping_adds_one_to_its_first_argument() {
        let args = [41i64];
        assert_eq!(unsafe { rt_ping(args.as_ptr(), 1) }, 42);
    }

    #[test]
    fn rt_ping_returns_zero_with_no_arguments() {
        assert_eq!(unsafe { rt_ping(std::ptr::null(), 0) }, 0);
    }

    #[test]
    fn rt_heap_live_count_reflects_the_registered_heaps_real_state() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);
        assert_eq!(unsafe { rt_heap_live_count(std::ptr::null(), 0) }, 0);

        heap.cons(Value::Int(1), Value::Empty).expect("cons failed");
        assert_eq!(unsafe { rt_heap_live_count(std::ptr::null(), 0) }, 1);
    }

    #[test]
    fn rt_heap_init_registers_a_freshly_created_heap() {
        let args = [4i64];
        assert_eq!(unsafe { rt_heap_init(args.as_ptr(), 1) }, 0);
        assert_eq!(unsafe { rt_heap_live_count(std::ptr::null(), 0) }, 0);
    }
}
