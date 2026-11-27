//! Interrupt handlers: the typelisp functions a board runs when one of its
//! interrupts, or a core exception, comes (`#[interrupt(N)]`,
//! `#[exception(NAME)]`).
//!
//! The generated `main` registers each handler under the number of its
//! vector-table entry ([`rt_interrupt_register`]); the board's runtime
//! (`typelisp-rt-embedded`) hands every exception it does not handle itself
//! to [`arrived`], and installs the [`Controller`] that masks and unmasks an
//! interrupt.
//!
//! # When a handler runs
//!
//! At once, from the exception the hardware entered, if the program is
//! running its own compiled code. Not while it is inside the runtime: a
//! runtime function may be part way through changing the heap, the scheduler,
//! a channel or a stream, and the handler would see that half done. The
//! runtime is **busy** from the moment compiled code calls one of its
//! functions until that function returns — a board's executable brackets
//! every such call with [`typelisp_busy_enter`] and [`typelisp_busy_leave`]
//! (`compile::aot::embedded`) — except while the scheduler runs a task's
//! compiled code ([`outside`]).
//!
//! A handler that comes while the runtime is busy, or while another handler
//! runs, is **pending**: its interrupt is masked so that it does not come
//! again and again, and the handler runs as soon as the runtime is no longer
//! busy — when the outermost runtime function returns, or when the scheduler
//! next runs compiled code — and the interrupt is unmasked again if the
//! program had enabled it. Handlers do not interrupt each other.
//!
//! # What a handler may not do
//!
//! - Collect: the program may have been stopped holding a value in a
//!   register only, which no root shows. A `cons` in a handler grows the
//!   arena instead ([`typelisp_mem::Heap::inhibit_collection`]).
//! - Wait: it runs on top of the program it stopped, with nobody to switch
//!   to. What the scheduler can answer at once (a buffered `recv`, a `send`
//!   with room, a new task) it answers; anything else is a panic.
//! - Leave by `panic` or `throw`: the program it stopped cannot be made to
//!   see either, so the program ends as an uncaught one would end it.

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use typelisp_mem::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering::SeqCst};

use typelisp_abi::raise;

/// How many entries the vector table has: the core's 16 exceptions and up
/// to 240 interrupts, as a Cortex-M numbers them. Every core's runtime
/// numbers its entries so (`typelisp-rt-embedded`): a core with fewer
/// interrupts says how many it has ([`Controller::interrupts`]).
pub const VECTORS: usize = 256;

/// The first entry that is an interrupt rather than a core exception.
const FIRST_INTERRUPT: usize = 16;

/// What the board's runtime gives this module: how many interrupts the core
/// has, and masking and unmasking one (by its number, not its vector-table
/// entry).
pub struct Controller {
    /// The interrupts are numbered from 0 to one less than this, at most
    /// `VECTORS - 16`.
    pub interrupts: u16,
    pub mask: fn(u16),
    pub unmask: fn(u16),
}

static CONTROLLER: AtomicPtr<Controller> = AtomicPtr::new(core::ptr::null_mut());
/// How deep the runtime is in functions compiled code called.
static BUSY: AtomicU32 = AtomicU32::new(0);
/// Whether a handler is running.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Whether any bit of [`PENDING`] may be set.
static ANY_PENDING: AtomicBool = AtomicBool::new(false);
/// The entries whose handler is waiting to run, a bit each.
static PENDING: [AtomicU32; VECTORS / 32] = [const { AtomicU32::new(0) }; VECTORS / 32];
/// The interrupts the program enabled, a bit per vector-table entry.
static ENABLED: [AtomicU32; VECTORS / 32] = [const { AtomicU32::new(0) }; VECTORS / 32];
/// Each entry's handler: the address of a coroutine-ABI body taking
/// nothing, or 0.
static HANDLERS: [AtomicUsize; VECTORS] = [const { AtomicUsize::new(0) }; VECTORS];
/// What runs a handler ([`run`]), stored by [`rt_interrupt_register`]: the
/// board's runtime reaches [`arrived`] whether or not the program has a
/// handler, and through this, what drives one is linked into an executable
/// only when its `main` registers one.
///
/// An exported symbol, so that the optimizer does not take `run` for the
/// only value it can hold and call `run` directly from [`arrived`].
#[no_mangle]
static TYPELISP_INTERRUPT_RUN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Runs `vector`'s handler with what [`TYPELISP_INTERRUPT_RUN`] holds.
fn run_registered(vector: usize) {
    let run = TYPELISP_INTERRUPT_RUN.load(SeqCst);
    if run.is_null() {
        typelisp_abi::fatal("internal error: an interrupt handler is registered and nothing runs it");
    }
    // SAFETY: only ever `run`'s address, stored by `rt_interrupt_register`.
    let run: fn(usize) = unsafe { core::mem::transmute(run) };
    run(vector)
}

/// Installs the board's [`Controller`]. Done once, before the program runs.
pub fn set_controller(controller: &'static Controller) {
    CONTROLLER.store(controller as *const Controller as *mut Controller, SeqCst);
}

fn controller() -> Option<&'static Controller> {
    // SAFETY: only ever a `&'static` stored by `set_controller`.
    unsafe { CONTROLLER.load(SeqCst).as_ref() }
}

fn bit(vector: usize) -> (usize, u32) {
    (vector / 32, 1 << (vector % 32))
}

/// Compiled code is calling a runtime function. A board's executable calls
/// this before each.
#[no_mangle]
pub extern "C" fn typelisp_busy_enter() {
    BUSY.fetch_add(1, SeqCst);
}

/// The runtime function compiled code called has returned; when it was the
/// outermost, the handlers that came meanwhile run.
#[no_mangle]
pub extern "C" fn typelisp_busy_leave() {
    if BUSY.fetch_sub(1, SeqCst) == 1 {
        run_pending();
    }
}

/// Runs `f`, which runs compiled code, with the runtime not busy when
/// `interruptible` — a task's compiled code the scheduler runs, which a
/// handler may break into. Only a board has interrupts; anywhere else this is
/// `f()`.
pub fn outside<R>(interruptible: bool, f: impl FnOnce() -> R) -> R {
    if !cfg!(target_os = "none") || !interruptible {
        return f();
    }
    let busy = BUSY.swap(0, SeqCst);
    run_pending();
    let r = f();
    BUSY.store(busy, SeqCst);
    r
}

/// The board's runtime got the exception or interrupt at vector-table entry
/// `vector`. Whether the program has a handler for it: if not, the caller
/// does what it does with one nobody handles.
pub fn arrived(vector: usize) -> bool {
    if vector >= VECTORS || HANDLERS[vector].load(SeqCst) == 0 {
        return false;
    }
    if BUSY.load(SeqCst) > 0 || RUNNING.swap(true, SeqCst) {
        pend(vector);
        return true;
    }
    run_registered(vector);
    drain();
    RUNNING.store(false, SeqCst);
    run_pending();
    true
}

/// Records that `vector`'s handler waits, and masks its interrupt meanwhile.
fn pend(vector: usize) {
    let (word, mask) = bit(vector);
    PENDING[word].fetch_or(mask, SeqCst);
    ANY_PENDING.store(true, SeqCst);
    if vector >= FIRST_INTERRUPT {
        if let Some(c) = controller() {
            (c.mask)((vector - FIRST_INTERRUPT) as u16);
        }
    }
}

/// Runs the handlers waiting, if the runtime is not busy and none runs.
fn run_pending() {
    while ANY_PENDING.load(SeqCst) && BUSY.load(SeqCst) == 0 {
        if RUNNING.swap(true, SeqCst) {
            return;
        }
        drain();
        RUNNING.store(false, SeqCst);
    }
}

/// Runs every waiting handler, the lowest entry first, with [`RUNNING`]
/// held, until none waits.
fn drain() {
    loop {
        ANY_PENDING.store(false, SeqCst);
        let mut ran = false;
        for (word, pending) in PENDING.iter().enumerate() {
            let bits = pending.load(SeqCst);
            if bits == 0 {
                continue;
            }
            let low = bits & bits.wrapping_neg();
            pending.fetch_and(!low, SeqCst);
            let vector = word * 32 + low.trailing_zeros() as usize;
            run_registered(vector);
            if vector >= FIRST_INTERRUPT && ENABLED[word].load(SeqCst) & low != 0 {
                if let Some(c) = controller() {
                    (c.unmask)((vector - FIRST_INTERRUPT) as u16);
                }
            }
            ran = true;
            break;
        }
        if !ran && !ANY_PENDING.load(SeqCst) {
            return;
        }
    }
}

/// Runs `vector`'s handler to its end, on what crosses between compiled code
/// and its driver set aside, and without collecting. A failure ends the
/// program.
fn run(vector: usize) {
    let f = HANDLERS[vector].load(SeqCst);
    // SAFETY: the address was registered by the generated `main` as a
    // coroutine-ABI body taking nothing; a `Heap` was registered before it.
    unsafe {
        let crossing = typelisp_abi::set_aside_crossing();
        let heap = typelisp_abi::active_heap();
        let collecting = heap.inhibit_collection(true);
        let f: crate::coroutine::CoroutineFn = core::mem::transmute(f);
        let outcome = crate::program::drive_to_completion(f, &[], &[], "an interrupt handler");
        typelisp_abi::active_heap().inhibit_collection(collecting);
        if outcome.is_err() {
            let code = crate::program::exit_code(crate::program::EXIT_CODE_PANIC);
            typelisp_mem::host::exit(code as i32);
        }
        typelisp_abi::restore_crossing(crossing);
    }
}

/// The number of the vector-table entry of interrupt `n`, if the core `c`
/// has one.
fn interrupt_vector(c: &Controller, n: i64) -> Result<usize, String> {
    let interrupts = usize::from(c.interrupts).min(VECTORS - FIRST_INTERRUPT);
    match usize::try_from(n) {
        Ok(n) if n < interrupts => Ok(FIRST_INTERRUPT + n),
        _ => Err(format!("interrupt {} does not exist: interrupts are numbered from 0 to {}", n, interrupts - 1)),
    }
}

/// `(enable-interrupt n)`: interrupt `n` comes from now on.
pub fn enable(n: i64) -> Result<(), String> {
    let c = controller().ok_or_else(|| "enable-interrupt: there are interrupts only on a board".to_string())?;
    let vector = interrupt_vector(c, n)?;
    let (word, mask) = bit(vector);
    ENABLED[word].fetch_or(mask, SeqCst);
    // Still masked while its handler waits: running it unmasks it.
    if PENDING[word].load(SeqCst) & mask == 0 {
        (c.unmask)(n as u16);
    }
    Ok(())
}

/// `(disable-interrupt n)`: interrupt `n` no longer comes.
pub fn disable(n: i64) -> Result<(), String> {
    let c = controller().ok_or_else(|| "disable-interrupt: there are interrupts only on a board".to_string())?;
    let vector = interrupt_vector(c, n)?;
    let (word, mask) = bit(vector);
    ENABLED[word].fetch_and(!mask, SeqCst);
    (c.mask)(n as u16);
    Ok(())
}

/// `(" interrupt-register" VECTOR BODY)`: the generated `main` registering
/// the handler at vector-table entry `VECTOR`, the address of its body.
///
/// # Safety
///
/// The words must be valid, the second the address of a
/// coroutine-ABI body that takes nothing.
#[no_mangle]
pub unsafe extern "C" fn rt_interrupt_register(vector: Word, body: Word) -> Word {
    let vector = vector as usize;
    if vector >= VECTORS {
        typelisp_abi::fatal(&format!("rt_interrupt_register: there is no vector-table entry {}", vector));
    }
    TYPELISP_INTERRUPT_RUN.store(run as fn(usize) as *mut (), SeqCst);
    HANDLERS[vector].store(body as usize, SeqCst);
    0
}

/// `(enable-interrupt n)` for compiled code.
///
/// # Safety
///
/// The word must be one raw `int`.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_interrupt_enable(x: Word) -> Word {
    match enable(x as i64) {
        Ok(()) => 0,
        Err(msg) => raise(msg).into_word(),
    }
}

/// `(disable-interrupt n)` for compiled code.
///
/// # Safety
///
/// The word must be one raw `int`.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_interrupt_disable(x: Word) -> Word {
    match disable(x as i64) {
        Ok(()) => 0,
        Err(msg) => raise(msg).into_word(),
    }
}
