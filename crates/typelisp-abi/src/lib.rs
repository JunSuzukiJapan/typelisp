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

// The ABI version: which `docs/dev/api_version/history/api_<N>.md` describes
// everything compiled code assumes about the archive it links. A literal in a
// macro rather than a `const`, because the symbol name below is built from it
// and `concat!` takes literals only. `scripts/regen-abi-version.sh` is what
// rewrites it, and only when that description changed.
macro_rules! abi_version { () => { 2 }; }

/// See the comment on `abi_version!` above.
pub const ABI_VERSION: u32 = abi_version!();

/// The symbol an archive of ABI version [`ABI_VERSION`] defines, and every
/// AOT executable refers to (`compile::aot::build_main_wrapper`).
///
/// A reference rather than a value read and compared at startup: linking an
/// archive of another version fails with this name undefined, before there is
/// an executable to run, and there is no path to the program that skips it.
pub const ABI_SYMBOL: &str = concat!("typelisp_abi_v", abi_version!());

#[export_name = concat!("typelisp_abi_v", abi_version!())]
pub static ABI_MARKER: u8 = 0;

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

/// Whether this thread has an active `Heap` registered — asked by the one
/// entry point that can be reached from a thread typelisp never ran on (a C
/// callback), where [`active_heap`] would dereference nothing.
pub fn heap_registered() -> bool {
    !ACTIVE_HEAP.with(|cell| cell.get()).is_null()
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
/// error back (a compiled `apply` or `:dyn` call reaching its interpreter
/// hooks).
///
/// **Carries nothing.** The error is a `typelisp::EvalError`, which is
/// not nameable from this crate (it depends on `typelisp-mem` alone). So the
/// `typelisp` crate parks the error in a thread-local and this type only says
/// *which* thread-local to look in: the error never leaves the thread that
/// raised it, which is the interpreter's.
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
/// Before this existed, the compiled `apply` and `:dyn` boundaries' interpreter
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


// ---- the driver protocol (Phase C2) -------------------------------------
//
// A compiled function under the coroutine ABI is `i64 f(i64 frame)`, and what
// it returns is not its value but a **status word** saying why it stopped.
// Its value, when it has one, is in the frame.
//
// The status is what makes suspension possible at all. Under the old ABI a
// function could only stop by returning, so "stop here and come back later"
// had nowhere to be said — the machine stack held the rest of the work and
// nothing could put it down. A frame plus a status word is exactly the pair
// that can: the frame is the work, the status is why it paused.
//
// The value rides in **frame slot 0**, reserved by the prologue and never
// handed out to a local. It carries the returned value on `RETURN`, and on
// resumption it carries the value the call produced — the same slot, because
// from the resuming function's side those are the same thing: the answer it
// was waiting for.

/// The function is finished; its value is in frame slot 0.
pub const STATUS_RETURN: i64 = 0;
/// The function wants to call another; the driver runs the callee and
/// resumes this frame with the result in slot 0.
pub const STATUS_CALL: i64 = 1;
/// The function is suspending (a task yielded, slept, or blocked). The
/// scheduler decides when its frame runs again.
pub const STATUS_SUSPEND: i64 = 2;
/// The function is unwinding — a `throw`, a `panic`, or a non-local exit
/// that leaves this frame. Phase C4 gives this a payload; until then no
/// compiled function produces it.
pub const STATUS_UNWIND: i64 = 3;

/// A compiled body under the original ABI: `i64 f(const i64 *args, u32 argc)`,
/// running to completion and returning its value.
pub const BODY_ABI_CLASSIC: u8 = 0;
/// A compiled body under the coroutine ABI: `i64 f(i64 frame)`, returning a
/// status word.
pub const BODY_ABI_COROUTINE: u8 = 1;

/// The frame slot the driver protocol reserves for the value in flight —
/// a function's result, and the result of a call it is waiting on. The
/// island's slot allocator hands out 2 upward.
pub const FRAME_VALUE_SLOT: usize = 0;

/// The frame slot that says where an unwind reaching this frame resumes: the
/// `pc` of the enclosing `catch`/`unwind-protect` region's dispatch block, or
/// `0` for "this frame catches nothing".
///
/// The driver reads it on [`STATUS_UNWIND`] to decide whether to re-enter the
/// frame or pop it. A *slot* rather than a static table keyed by the call's
/// `pc`, because the answer is not a fact about the call — it is a fact about
/// the region the call is written in, and the island already knows which that
/// is at every point it emits. Written on entering and leaving a region, and
/// on entering any block that leaves one (a loop's exit, a named block's
/// exit, a cleanup copy), which is the whole discipline: a frame with no
/// region never touches it, and `rt_frame_new` zero-fills.
///
/// Never masked: a `pc` is a raw integer, not a tagged value.
pub const FRAME_HANDLER_SLOT: usize = 1;

/// How many slots at the bottom of a frame the driver protocol owns:
/// [`FRAME_VALUE_SLOT`] and [`FRAME_HANDLER_SLOT`].
///
/// **Every frame a driver drives must have at least this many**, whoever
/// built it — the island's prologue hands locals out from here upward, and a
/// hand-written entry shim has to reserve them even when it uses neither.
/// `FrameStack::unwind` reads the handler slot of every frame it walks, so a
/// frame one word short is a frame the driver cannot ask; it says so rather
/// than reading past the end.
pub const FRAME_RESERVED_SLOTS: usize = 2;

/// The state one coroutine-ABI call is in flight through.
///
/// Three thread-locals, for the same reason [`ACTIVE_HEAP`](set_active_heap)
/// is one: the ABI has exactly **one** parameter — a frame, or `0` on first
/// entry — and it must, so that entering a function and resuming it are the
/// same call. Anything else that has to cross that boundary has nowhere to
/// ride but here.
///
/// Each is live across exactly one call, between adjacent statements on the
/// two sides. Nothing may allocate in those windows: the words are raw and
/// tagged, sitting in Rust `Vec`s the collector does not trace.
pub mod call_state {
    use std::cell::RefCell;

    thread_local! {
        /// The arguments a not-yet-entered function will copy into its own
        /// frame. Written by the driver, read by that function's prologue.
        ///
        /// The callee allocates its own frame, not the caller: only the callee
        /// knows how many slots it needs, and under mutual recursion the
        /// caller may be compiled before the callee exists at all.
        static PENDING_ARGS: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
        /// The frames prologues have allocated on their way in, innermost
        /// last. The driver cannot learn one any other way — it is made
        /// inside the callee, after the call has started.
        ///
        /// **A stack, not a slot.** The driver takes the frame only once the
        /// entering call has *returned*, and a callee can start a driver of
        /// its own before then: `rt_drive_body`, the C FFI thunk and a body
        /// the interpreter calls from a Rust frame all run a nested
        /// `FrameStack` on the machine stack (C4
        /// retired `rt_protected_drive` and C5 retired `rt_apply_any` and
        /// `rt_dyn_call`, which were three more).
        /// Every one of those publishes and takes in balanced
        /// pairs within the outer callee's own activation, so LIFO hands each
        /// driver back exactly the frame its own entry made. A single slot
        /// let the innermost driver consume the outermost's, which surfaced
        /// as "a coroutine function did not publish its frame on entry" —
        /// blamed on the callee, whose prologue had in fact published.
        static CURRENT_FRAME: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
        /// The captured values a not-yet-entered closure body will copy into
        /// its own frame, alongside `PENDING_ARGS`.
        ///
        /// Separate from the arguments because the callee reads the two by
        /// separate indices — a capture and a parameter are different names in
        /// different lists, and concatenating them would make every prologue
        /// know how long the other list was.
        static PENDING_ENV: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
        /// What a frame is asking the driver to call, set immediately before
        /// it returns `STATUS_CALL`.
        static PENDING_CALL: RefCell<Option<Pending>> = const { RefCell::new(None) };
        /// What a frame is asking to wait for, set immediately before it
        /// returns `STATUS_SUSPEND`: a `SUSPEND_*` kind and one payload word.
        ///
        /// **Two raw words, not a `Waiting`.** The scheduler's own type names
        /// a `TaskId` and an `Instant`, both of which live in the front end;
        /// this crate is below it and below `typelisp-rt` (see this module's
        /// doc comment), and a compiled body has nothing but words to hand
        /// over anyway. The front end turns the pair back into a `Waiting`,
        /// which is the same division `PENDING_CALL` makes between an address
        /// and a `CoroutineFn`.
        static PENDING_SUSPEND: RefCell<Option<(i64, i64, i64)>> = const { RefCell::new(None) };
        /// The arms a `select` is waiting on, when the kind is
        /// [`SUSPEND_CHAN_SELECT`]. Its own slot because the number of them
        /// is the program's, and the pair above has room for two words.
        static PENDING_SELECT: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
    }

    /// `(yield)` — nothing to wait for; the task gives up the rest of its
    /// turn. Payload unused.
    pub const SUSPEND_YIELD: i64 = 0;
    /// `(sleep secs)` — the clock. Payload: the seconds as `f64::to_bits`.
    pub const SUSPEND_SLEEP: i64 = 1;
    /// `(wait t)` — another task's result. Payload: its scheduler id.
    pub const SUSPEND_WAIT: i64 = 2;
    /// A loop's own safepoint (Phase C7) — nobody asked; the back edge of a
    /// compiled `loop` offered. Payload unused.
    ///
    /// **Distinct from [`SUSPEND_YIELD`] on purpose.** A driver standing on a
    /// machine frame cannot honour a real wait, and says so; but this one it
    /// can honour by resuming at once, because nothing is being waited for.
    /// Folding it into `SUSPEND_YIELD` would have made an explicit `(yield)`
    /// silently do nothing in a `print-object` method or a reader macro,
    /// which is a different promise from the one `(yield)` makes.
    pub const SUSPEND_SAFEPOINT: i64 = 3;

    /// `(Chan::new cap)` — make a channel. Payload: the capacity.
    ///
    /// The five channel operations are suspensions for a reason that has
    /// nothing to do with waiting: the table of channels belongs to the
    /// **scheduler**, which lives in the front end and is created per
    /// session, and only the driver can reach it. A table in this crate or in
    /// `typelisp-rt` would outlive the `Heap` whose root stack holds the
    /// buffered values — the dangling-`ACTIVE_HEAP` bug, with values in it.
    ///
    /// So three of these six always resume at once (`new`/`len`/`cap`, and
    /// `close` unless it is a second one), exactly as
    /// [`SUSPEND_SAFEPOINT`] does. Only `send` and `recv` can really park.
    pub const SUSPEND_CHAN_NEW: i64 = 4;
    /// `(len ch)` — how many values are buffered. Payload: the handle.
    pub const SUSPEND_CHAN_LEN: i64 = 5;
    /// `(cap ch)` — the channel's capacity. Payload: the handle.
    pub const SUSPEND_CHAN_CAP: i64 = 6;
    /// `(close ch)` — close it. Payload: the handle.
    pub const SUSPEND_CHAN_CLOSE: i64 = 7;
    /// `(send ch v)` — hand a value over, waiting for room. Payloads: the
    /// handle and the value, **both tagged** (the value by the suspension
    /// site, per its own `Repr::field_kind`, because a raw word does not say
    /// what it is).
    pub const SUSPEND_CHAN_SEND: i64 = 8;
    /// `(recv ch)` — take a value, waiting for one. Payloads: the handle and
    /// the `Option<T>` type key the answer is built with.
    pub const SUSPEND_CHAN_RECV: i64 = 9;
    /// `(select ...)` — any one of several channel operations. The arms
    /// travel in [`take_pending_select`] rather than in the payload words:
    /// how many there are is the program's business.
    pub const SUSPEND_CHAN_SELECT: i64 = 10;
    /// `(net-wait h interest)` — a socket becoming ready. Payloads: the
    /// stream handle and the interest code (`typelisp_rt::os::Interest`),
    /// **both raw** — two plain integers, like `sleep`'s bits. The front end
    /// resolves the handle to a descriptor, exactly as it does for the
    /// interpreted call.
    pub const SUSPEND_IO: i64 = 11;
    /// `(net-wait-for h interest secs)` — a socket becoming ready **or** the
    /// clock running out. Payloads: `(handle << 1) | interest` and the
    /// seconds as `f64::to_bits`. Answers with a tagged `bool`.
    pub const SUSPEND_IO_FOR: i64 = 12;
    /// `(task (f a b))` — start a task. Payload: a **tagged** compiled closure
    /// that makes the call and answers with the result tagged, built by the
    /// site that wrote the `task` (`core_bridge::translate_spawn_call`).
    ///
    /// A suspension rather than a call for the reason the channel operations
    /// are: the table of tasks belongs to the scheduler, and only the driver
    /// can reach it. It always resumes at once, like `SUSPEND_CHAN_NEW`, and
    /// the answer is the `Task<T>` handle.
    pub const SUSPEND_TASK: i64 = 13;
    /// `(thread (f a b))` — start a task on an OS thread of its own. Payload:
    /// the same closure [`SUSPEND_TASK`] carries, built by the same site; only
    /// where the scheduler runs it differs. Resumes at once with the
    /// `Thread<T>` handle.
    pub const SUSPEND_THREAD: i64 = 14;
    /// Before a call only the interpreter can answer (`eval`, `read`, ...):
    /// go on on the thread the interpreter runs on. Payload unused. Answered
    /// at once — by the interpreter's own task, and by every driver that has
    /// nowhere to move to — except in a compiled task a worker is stepping,
    /// which stops there and moves (`sched::Progress::NeedsMain`).
    pub const SUSPEND_MAIN: i64 = 15;

    /// Records the arms of a `select`, as `[n, has-else, kind, chan, extra]...`
    /// with every word **tagged** — the array is a compiled frame's slots,
    /// which the collector walks, so a raw integer in one would be read as a
    /// pointer.
    pub fn set_pending_select(words: &[i64]) {
        PENDING_SELECT.with(|p| {
            let mut v = p.borrow_mut();
            v.clear();
            v.extend_from_slice(words);
        });
    }

    /// Takes them back out.
    pub fn take_pending_select() -> Vec<i64> {
        PENDING_SELECT.with(|p| std::mem::take(&mut *p.borrow_mut()))
    }

    /// Records what the frame about to return `STATUS_SUSPEND` is waiting for.
    pub fn set_pending_suspend(kind: i64, payload: i64) {
        PENDING_SUSPEND.with(|p| *p.borrow_mut() = Some((kind, payload, 0)));
    }

    /// [`set_pending_suspend`] for the kinds that need two words.
    ///
    /// Two and not a slice: the pair is what a channel operation needs (a
    /// handle and one other word), and a `Vec` here would allocate on a path
    /// that must not — the shim runs with a compiled frame's arguments in
    /// slots the collector is not looking at.
    pub fn set_pending_suspend_2(kind: i64, first: i64, second: i64) {
        PENDING_SUSPEND.with(|p| *p.borrow_mut() = Some((kind, first, second)));
    }

    /// Takes it back out. `None` means a frame returned `STATUS_SUSPEND`
    /// without saying what it was waiting for, which is a broken convention
    /// rather than "waiting for nothing" — the caller says so and stops.
    pub fn take_pending_suspend() -> Option<(i64, i64, i64)> {
        PENDING_SUSPEND.with(|p| p.borrow_mut().take())
    }

    pub fn set_pending_args(args: &[i64]) {
        PENDING_ARGS.with(|a| {
            let mut v = a.borrow_mut();
            v.clear();
            v.extend_from_slice(args);
        });
    }

    /// The `i`th pending argument, or `None` if the caller passed fewer than
    /// that. A prologue reads exactly as many as it has parameters, so a miss
    /// is a broken calling convention rather than a case to paper over — the
    /// caller turns it into a fatal error rather than a zero.
    pub fn pending_arg(i: usize) -> Option<i64> {
        PENDING_ARGS.with(|a| a.borrow().get(i).copied())
    }

    pub fn pending_argc() -> usize {
        PENDING_ARGS.with(|a| a.borrow().len())
    }

    pub fn set_pending_env(env: &[i64]) {
        PENDING_ENV.with(|e| {
            let mut v = e.borrow_mut();
            v.clear();
            v.extend_from_slice(env);
        });
    }

    /// The `i`th pending capture, or `None` — same contract as
    /// [`pending_arg`].
    pub fn pending_env(i: usize) -> Option<i64> {
        PENDING_ENV.with(|e| e.borrow().get(i).copied())
    }

    pub fn pending_envc() -> usize {
        PENDING_ENV.with(|e| e.borrow().len())
    }

    pub fn set_current_frame(f: i64) {
        CURRENT_FRAME.with(|c| c.borrow_mut().push(f));
    }

    pub fn take_current_frame() -> Option<i64> {
        CURRENT_FRAME.with(|c| c.borrow_mut().pop())
    }

    /// How many entering calls have published a frame that nobody has taken
    /// yet — the depth a catcher records so it can put the stack back.
    pub fn current_frame_depth() -> usize {
        CURRENT_FRAME.with(|c| c.borrow().len())
    }

    /// Drops everything published above `depth`.
    ///
    /// An unwind travels *between* a prologue's publish and the driver's
    /// take, so the entries in that window are never taken: the compiled
    /// frame that raised, and every frame below it in the same driver. A
    /// catcher restores this the same way it restores the GC root stack —
    /// without it, the next `take_current_frame` hands the caller a frame
    /// belonging to a call that has already left, and the value read out of
    /// it is whatever that frame happened to hold.
    pub fn truncate_current_frames(depth: usize) {
        CURRENT_FRAME.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() > depth {
                c.truncate(depth);
            }
        });
    }

    /// What one `STATUS_CALL` is asking for.
    ///
    /// Two shapes, because compiled code knows two different amounts about
    /// its callee. A direct call knows the address — the checker resolved the
    /// name. An `apply` knows only a *value*, and what that value turns out
    /// to be (a compiled body under either ABI, or an interpreted closure)
    /// decides who runs it. **That decision belongs to the driver**: it is
    /// the only party that can push a frame onto the chain, and for an
    /// interpreted callee it is the only party that can hand the call to the
    /// continuation stack instead of running it on a machine frame.
    #[derive(Debug)]
    pub enum Pending {
        /// A statically-known compiled callee: its address, its arguments,
        /// and its captures.
        Direct { target: usize, args: Vec<i64>, env: Vec<i64> },
        /// A function *value*, tagged, with its arguments in its own
        /// declared representations.
        Apply { closure: i64, args: Vec<i64> },
        /// A `:dyn` method: the vtable the trait object dispatches through,
        /// the slot the checker picked, and the arguments with the *concrete*
        /// receiver first. What the slot holds decides who runs it, exactly
        /// as for [`Pending::Apply`] — and it can hold nothing at all, when
        /// the concrete type's method is interpreted.
        Dyn { vtable: u32, slot: u32, args: Vec<i64> },
    }

    pub fn set_pending_call(target: usize, args: Vec<i64>, env: Vec<i64>) {
        PENDING_CALL.with(|c| *c.borrow_mut() = Some(Pending::Direct { target, args, env }));
    }

    /// [`set_pending_call`] for an `apply`: the callee is a value rather than
    /// an address, so the driver resolves it.
    pub fn set_pending_apply(closure: i64, args: Vec<i64>) {
        PENDING_CALL.with(|c| *c.borrow_mut() = Some(Pending::Apply { closure, args }));
    }

    /// [`set_pending_call`] for a `:dyn` method call.
    pub fn set_pending_dyn(vtable: u32, slot: u32, args: Vec<i64>) {
        PENDING_CALL.with(|c| *c.borrow_mut() = Some(Pending::Dyn { vtable, slot, args }));
    }

    pub fn take_pending_call() -> Option<Pending> {
        PENDING_CALL.with(|c| c.borrow_mut().take())
    }
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
