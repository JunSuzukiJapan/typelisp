//! Waiting on a descriptor: what the scheduler needs of the runtime crate
//! that has sockets (`typelisp-rt-posix`), which implements it.
//!
//! The scheduler keeps the waits and decides when to look at them; the
//! looking — the operating system's `poll` — and the way one thread wakes
//! another out of it are the other crate's. A program that makes no socket
//! makes no such wait, and a target without sockets has none to make.

use typelisp_mem::prelude::*;

/// What a socket wait is for: the readiness a [`Poller`] reports and the
/// scheduler parks a task on. The wire form is `net-wait`'s second
/// argument: 0 readable, 1 writable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Interest {
    Readable,
    Writable,
}

impl Interest {
    /// The integer `net-wait` takes — `0` readable, `1` writable — or `None`
    /// for anything else, which is the caller's error to report.
    pub fn from_code(code: i64) -> Option<Interest> {
        match code {
            0 => Some(Interest::Readable),
            1 => Some(Interest::Writable),
            _ => None,
        }
    }
}

/// How the descriptors tasks wait on are watched.
pub trait Poller: Sync {
    /// Which of `fds` are ready, waiting at most `timeout` (`None`: as long
    /// as it takes).
    ///
    /// The answer for each descriptor is a plain `bool`: ready as asked,
    /// **or in error, hung up, or invalid**. Those three are reported as
    /// ready on purpose — a task waiting on a socket the peer has closed must
    /// wake so that its next `read` can see the end of input, and one waiting
    /// on a descriptor that has since been closed must wake so that its next
    /// operation can fail with a message instead of parking forever.
    fn poll_ready(&self, fds: &[(i32, Interest)], timeout: Option<core::time::Duration>) -> Result<Vec<(i32, bool)>, String>;
}

/// The [`Poller`] a socket wait is watched with — the one the socket's
/// backend named (`ExtBackend::wait_fd`). Compared by address: two waits are
/// the same wait only on the same poller.
#[derive(Clone, Copy)]
pub struct PollerRef(pub &'static dyn Poller);

impl PartialEq for PollerRef {
    fn eq(&self, other: &PollerRef) -> bool {
        core::ptr::addr_eq(self.0 as *const dyn Poller, other.0 as *const dyn Poller)
    }
}

impl core::fmt::Debug for PollerRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Poller@{:p}", self.0 as *const dyn Poller)
    }
}

/// How a thread changing what the scheduler waits for wakes the thread
/// that is in [`Poller::poll_ready`] with the scheduler's lock let go of:
/// a descriptor that thread polls alongside the sockets, made readable by
/// [`Self::kick`].
pub trait Waker: Send + Sync {
    /// The descriptor to poll for readability.
    fn read_fd(&self) -> i32;
    /// Wakes the poller, if one is waiting — and makes the next poll return
    /// at once if none is yet.
    fn kick(&self);
    /// Empties what a kick left, after a poll it woke.
    fn drain(&self);
}
