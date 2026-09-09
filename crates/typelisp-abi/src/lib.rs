//! The calling convention compiled typelisp code uses: how a `Value` is
//! spelled as a machine word, which `Heap` a shim operates on, and how a
//! shim fails.
//!
//! # Why this is its own crate
//!
//! Everything here used to live in `typelisp-rt`, which was fine while
//! `typelisp-rt` was the only crate defining `rt_*` shims. It no longer is:
//! `typelisp-print` defines the printer's shims, and it defines them *itself*
//! rather than letting `typelisp-rt` wrap them, because a wrapper in
//! `typelisp-rt` would be linked into every program and would drag the whole
//! directive engine in behind it (see that crate's doc comment).
//!
//! Those shims need exactly what is in this file — `encode`, `active_heap`,
//! `fatal` — and `typelisp-rt` already depends on `typelisp-print`, so they
//! cannot come from there. Hence a crate below both.
//!
//! Nothing here is re-implemented on either side. In particular
//! [`ACTIVE_HEAP`](set_active_heap) must be *one* thread-local: two copies
//! would mean a printer shim looking for a heap the interpreter registered in
//! the other one.

pub mod dribble;

use std::cell::Cell;

use typelisp_mem::{Heap, Value};


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
pub unsafe fn active_heap() -> &'static mut Heap {
    let ptr = ACTIVE_HEAP.with(|cell| cell.get());
    debug_assert!(!ptr.is_null(), "rt_* function called with no active Heap registered on this thread");
    &mut *ptr
}

/// Prints `msg` to stderr and aborts the process — how an `rt_*` function
/// fails when the *runtime itself* has been violated.
///
/// **Runtime corruption only.** Everything that reaches here is a contract
/// violation by `compiler.rs` or by whoever built the call: a wrong argument
/// count, an argument carrying the wrong tag, a root-stack invariant broken.
/// None of it is expressible as a typelisp-level error, because a program
/// that reaches one is no longer the program the checker approved — the same
/// division of labor SBCL draws between `lose()` (the runtime is broken) and
/// signalling a condition (the *program* did something the language defines
/// an error for).
///
/// A language-level failure — a zero divisor, an index past the end of a
/// vector or string, a non-positive `random` bound, an explicit
/// `(panic ...)` — is the other kind, and goes to [`raise`] instead, which
/// unwinds and is catchable. Those callers moved off this function on
/// 2026-08-17; what is left here is the ~190 sites that genuinely mean the
/// runtime is broken.
pub fn fatal(msg: &str) -> ! {
    eprintln!("typelisp runtime error: {}", msg);
    std::process::abort();
}

/// The payload a compiled `(panic ...)` unwinds with, so the interpreter can
/// tell *its* panic apart from a genuine Rust bug that happens to cross the
/// same boundary.
///
/// A plain `String` payload would be ambiguous — any `panic!("...")` inside
/// the runtime produces one — and swallowing a real bug as if it were a
/// typelisp-level `panic` would turn a crash into a silently wrong
/// `EvalError`. Catchers downcast to this type and re-raise anything else.
#[derive(Debug)]
pub struct CompiledPanic {
    /// The message the `(panic ...)` form was given, without the `"panic: "`
    /// prefix `EvalError::Panic`'s `Display` adds.
    pub message: String,
}

/// The payload marking an unwind that carries an *interpreted* callee's
/// error, raised when compiled code called into the interpreter and got an
/// error back ([`rt_apply_any`]/[`rt_dyn_call`] reaching their interpreter
/// hooks).
///
/// **Carries nothing.** The error is a `typelisp::EvalError`, which is
/// neither nameable from this crate (it depends on `typelisp-mem` alone) nor
/// `Send`, as a panic payload must be — it holds `Rc<str>` and raw heap
/// pointers. So the `typelisp` crate parks the error in a thread-local and
/// this type only says *which* thread-local to look in. That is not a
/// workaround so much as the honest shape: the error never leaves the thread
/// that raised it, and a payload asserting otherwise would be a lie held up
/// by an `unsafe impl Send` over raw pointers.
#[derive(Debug)]
pub struct InterpretedUnwind;

/// The payload marking an unwind raised by a compiled `(throw 'tag value)`.
///
/// Carries nothing, for the same reason [`InterpretedUnwind`] does not: the
/// tag and the thrown value wait in [`take_throw`]'s slot on this thread while
/// the unwind travels. Unlike an interpreted error, though, the reason is not
/// that they are unnameable here — a `String` and a `Value` both are — but
/// that the *value* needs a GC root for the whole flight
/// (`Heap::set_in_flight_throw`), and a panic payload is invisible to the
/// collector. Keeping both halves in one place keeps them from disagreeing.
#[derive(Debug)]
pub struct CompiledThrow;

/// Makes the default panic hook stay quiet for the two payloads this crate
/// raises deliberately ([`CompiledPanic`] and [`InterpretedUnwind`]),
/// installed once on the first such unwind of the process.
///
/// Without this, unwinding one of them prints Rust's own
/// `thread '...' panicked at ...: Box<dyn Any>` line before the interpreter
/// ever gets to report the error properly. Any other payload — a real bug in
/// the runtime — still reaches the previous hook untouched, so this hides
/// nothing that was not raised on purpose as a typelisp-level failure.
pub fn install_quiet_panic_hook() {
    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INSTALLED.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let ours = info.payload().downcast_ref::<CompiledPanic>().is_some()
                || info.payload().downcast_ref::<InterpretedUnwind>().is_some()
                || info.payload().downcast_ref::<CompiledThrow>().is_some();
            if !ours {
                previous(info);
            }
        }));
    });
}

/// Unwinds out of compiled code with `msg` as a recoverable typelisp-level
/// failure — the counterpart of [`fatal`] for everything the *language*
/// defines an error for rather than the runtime.
///
/// Caught by [`crate::rt_run_entry`] in an AOT executable and by
/// `typelisp::compile::catch_compiled_panic` on the JIT side, both of which
/// turn it back into the `EvalError::Panic` the interpreted path produces for
/// the same form — so a `catch`/`unwind-protect` region in between sees it,
/// cleanups run, and the process survives.
///
/// **Every function that can reach this must be `extern "C-unwind"`.** A
/// panic crossing a plain `extern "C"` boundary is defined to abort; see
/// [`rt_panic`], where that constraint is spelled out in full.
pub fn raise(msg: String) -> ! {
    install_quiet_panic_hook();
    std::panic::panic_any(CompiledPanic { message: msg })
}

/// Unwinds out of compiled code because an interpreted callee failed.
///
/// The counterpart of [`rt_panic`] for the other direction of the boundary:
/// compiled code called into the interpreter, and the interpreter failed.
/// Before this existed, [`rt_apply_any`]'s and [`rt_dyn_call`]'s interpreter
/// hooks had nowhere to report to and aborted the process — including for
/// ordinary, recoverable failures like a `(panic ...)` in an interpreted
/// callback.
///
/// The caller must have parked the error where its catcher will look for it
/// *before* calling this; see [`InterpretedUnwind`] for why it does not
/// travel in the payload.
pub fn unwind_interpreted_error() -> ! {
    install_quiet_panic_hook();
    std::panic::panic_any(InterpretedUnwind)
}


/// The tagged-`i64` representation compiled code uses for a `Value`.
///
/// **Defined in `typelisp-mem`** ([`typelisp_mem::tagged`]) and re-exported
/// here, where every caller has always named it. It moved down for the
/// compiled-CPS work: a compiled frame holds tagged *words* rather than
/// `Value`s, so the collector — which lives in `typelisp-mem`, below this
/// crate — has to be able to decode them.
pub use typelisp_mem::tagged::{decode, encode};

/// Decodes one tagged argument.
///
/// # Safety
///
/// `args` must point to at least `i + 1` valid `i64`s.
pub unsafe fn tagged_arg(args: *const i64, i: usize) -> Value {
    decode(*args.add(i))
}
