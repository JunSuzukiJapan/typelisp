//! CL's `dribble`: a copy of the interactive session, written to a file.
//!
//! # Why this is *here*
//!
//! What CL calls "the session" leaves this process through three different
//! doors, and no two of them are in the same crate:
//!
//! - `print`/`println`/`format`'s own output, which `typelisp_print`'s
//!   `runtime::write_stdout` puts on stdout directly;
//! - anything written to a stream whose backend is stdout, which
//!   `typelisp_rt`'s `StreamTable::write_str` puts there;
//! - the REPL's own two halves — the line the user typed and the value it
//!   printed back — which `typl`'s `main.rs` writes with `println!`.
//!
//! `typelisp-abi` is the one crate below all three (`typelisp-rt` depends on
//! `typelisp-print`, and the binary on both), which is the same reason
//! [`ACTIVE_HEAP`](crate::set_active_heap) lives here. Putting the sink
//! anywhere else would mean two of the three doors could not reach it.
//!
//! # Why a thread-local
//!
//! Identical reasoning to `ACTIVE_HEAP`: `cargo test` drives many independent
//! interpreters in one process, on different threads, and a global file
//! handle would let one test's dribble collect another's output.

use alloc::boxed::Box;
use alloc::string::String;
use core::cell::RefCell;

/// Where a dribble is recorded: a file, opened by the runtime crate that has
/// files (`typelisp-rt-sys`). This crate only keeps the open one and copies
/// the session into it.
pub trait Sink {
    /// Copies `text` in, and makes it readable at once. `false` when it could
    /// not, which stops the recording ([`note`]).
    fn write(&mut self, text: &str) -> bool;
    /// Flushes and closes: the error a full disk shows up as, here or
    /// nowhere.
    fn finish(self: Box<Self>) -> Result<(), String>;
}

typelisp_mem::thread_local! {
    /// This thread's open dribble, if any.
    static DRIBBLE: RefCell<Option<Box<dyn Sink>>> = const { RefCell::new(None) };
}

/// Puts `sink` in the place of the open dribble, and returns the one it
/// replaces for the caller to [`Sink::finish`]. `None` stops recording.
///
/// CL's `(dribble path)` while a dribble is already open closes the first,
/// and `(dribble)` with no argument closes the open one; both are this.
pub fn replace(sink: Option<Box<dyn Sink>>) -> Option<Box<dyn Sink>> {
    DRIBBLE.with(|cell| core::mem::replace(&mut *cell.borrow_mut(), sink))
}

/// Copies `text` into the open dribble, if there is one.
///
/// Deliberately infallible: this is called from the middle of writing to
/// stdout, and a program's `println` must not start failing because the
/// dribble file's disk filled up. A write error stops the recording rather
/// than propagating — the alternative is an error channel on every print in
/// the language, for a facility that is a convenience.
pub fn note(text: &str) {
    if text.is_empty() {
        return;
    }
    DRIBBLE.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(w) = slot.as_mut() else { return };
        if !w.write(text) {
            *slot = None;
        }
    });
}
