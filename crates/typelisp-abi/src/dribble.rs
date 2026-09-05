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

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufWriter, Write};

thread_local! {
    /// This thread's open dribble file, if any.
    static DRIBBLE: RefCell<Option<BufWriter<File>>> = const { RefCell::new(None) };
}

/// Starts recording into `path`, truncating it — CL's `(dribble path)`.
///
/// Starting a second one closes the first, which is what CL says happens when
/// `dribble` is called while a dribble is already open.
pub fn start(path: &str) -> std::io::Result<()> {
    let f = File::create(path)?;
    DRIBBLE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(old) = slot.take() {
            drop(old);
        }
        *slot = Some(BufWriter::new(f));
    });
    Ok(())
}

/// Stops recording and closes the file — CL's `(dribble)` with no argument.
/// A no-op when nothing is being recorded, as CL's is.
pub fn stop() -> std::io::Result<()> {
    let mut w = match DRIBBLE.with(|cell| cell.borrow_mut().take()) {
        Some(w) => w,
        None => return Ok(()),
    };
    w.flush()
}

/// Whether a dribble file is open.
pub fn is_open() -> bool {
    DRIBBLE.with(|cell| cell.borrow().is_some())
}

/// Copies `text` into the open dribble file, if there is one.
///
/// Deliberately infallible: this is called from the middle of writing to
/// stdout, and a program's `println` must not start failing because the
/// dribble file's disk filled up. A write error stops the recording rather
/// than propagating — the alternative is an error channel on every print in
/// the language, for a facility that is a convenience.
///
/// Flushed on every note, for the same reason `write_stdout` flushes: a
/// dribble file is read while the session is still running.
pub fn note(text: &str) {
    if text.is_empty() {
        return;
    }
    DRIBBLE.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(w) = slot.as_mut() else { return };
        if w.write_all(text.as_bytes()).and_then(|()| w.flush()).is_err() {
            *slot = None;
        }
    });
}
