//! The interactive interrupt: what Ctrl-C at the REPL asks of the evaluation
//! it is running.
//!
//! The REPL's signal handler only [`request`]s; it can do nothing else, since
//! a handler may touch nothing but atomics. The evaluation answers at its own
//! points: the interpreter between two steps of the task the REPL is driving,
//! and the scheduler when that task is waiting (a `sleep`, a `recv`, a
//! socket). A loop in compiled code is reached through its safepoint, which
//! hands the task back to the interpreter every so many rounds.
//!
//! Here rather than in the runtime because this crate is under every one
//! that answers: the interpreter, the scheduler and the REPL.

use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;

static WATCHED: AtomicBool = AtomicBool::new(false);
static REQUESTED: AtomicBool = AtomicBool::new(false);

/// How long the scheduler waits without looking at [`pending`] while an
/// interrupt can be requested: what bounds the time from Ctrl-C to the stop
/// when the task is asleep or waiting for a message.
pub const LOOK_EVERY: Duration = Duration::from_millis(50);

/// Says an interrupt can be requested from now on: the REPL calls it once
/// its handler is installed. Until then no wait is cut short to look.
pub fn watch() {
    WATCHED.store(true, Ordering::SeqCst);
}

/// Whether [`watch`] was called.
pub fn watched() -> bool {
    WATCHED.load(Ordering::Relaxed)
}

/// Asks the evaluation to stop. Safe to call from a signal handler.
pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

/// Whether a stop was asked for and nobody has answered it yet.
pub fn pending() -> bool {
    REQUESTED.load(Ordering::Relaxed)
}

/// Answers the stop that was asked for: whoever makes the evaluation stop
/// calls this, and the REPL before it starts the next one.
pub fn clear() {
    REQUESTED.store(false, Ordering::SeqCst);
}
