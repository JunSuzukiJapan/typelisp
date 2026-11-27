//! How much memory the process has allocated, and how much it may: what says
//! when a [`Heap`](crate::Heap) collects for memory rather than for a cons
//! cell.
//!
//! The process's allocator keeps the count (`typelisp_rt::mem_limit`). In a
//! process whose allocator does not, nothing is ever counted and no
//! collection is made for memory.

use crate::sync::atomic::{AtomicUsize, Ordering};

/// Bytes currently allocated.
static IN_USE: AtomicUsize = AtomicUsize::new(0);

/// The most the process is allowed to allocate; `usize::MAX` is no bound.
static LIMIT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// The most there is to allocate, where that is known: a board's heap.
static CAPACITY: AtomicUsize = AtomicUsize::new(usize::MAX);

/// How much a heap lets be allocated between two collections, at least, where
/// there is room for it. It bounds how often a small program collects.
const MIN_STEP: usize = 16 << 20;

/// The same where there is hardly room for anything.
const LAST_STEP: usize = 4 << 10;

/// Takes `size` more bytes into the count, and answers what it was.
pub fn taken(size: usize) -> usize {
    IN_USE.fetch_add(size, Ordering::Relaxed)
}

/// Takes `size` bytes out of the count again.
pub fn given_back(size: usize) {
    IN_USE.fetch_sub(size, Ordering::Relaxed);
}

/// Bytes the process has allocated and not freed.
pub fn in_use() -> usize {
    IN_USE.load(Ordering::Relaxed)
}

/// Bounds the process's allocations at `bytes` from now on.
pub fn set_limit(bytes: usize) {
    LIMIT.store(bytes, Ordering::Relaxed);
}

pub fn limit() -> usize {
    LIMIT.load(Ordering::Relaxed)
}

/// The most there is to allocate, where that is known (a board's heap);
/// `usize::MAX` where it is not.
pub fn capacity() -> usize {
    CAPACITY.load(Ordering::Relaxed)
}

/// Says there are `bytes` bytes to allocate from in all.
pub fn set_capacity(bytes: usize) {
    CAPACITY.store(bytes, Ordering::Relaxed);
}

/// The use at which a heap collects next, when `used` is the use now: twice
/// as much, so that what a collection costs is paid for by as much
/// allocation — but never more than half way to the end of the memory.
pub(crate) fn next_collection(used: usize) -> usize {
    step_from(used, limit().min(CAPACITY.load(Ordering::Relaxed)))
}

fn step_from(used: usize, most: usize) -> usize {
    let room = most.saturating_sub(used);
    used.saturating_add(used.max(MIN_STEP).min(room / 2).max(LAST_STEP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_use_waits_for_the_least_step() {
        assert_eq!(step_from(0, usize::MAX), MIN_STEP);
    }

    #[test]
    fn a_large_use_doubles() {
        assert_eq!(step_from(3 * MIN_STEP, usize::MAX), 6 * MIN_STEP);
    }

    #[test]
    fn a_collection_comes_before_the_memory_ends() {
        assert_eq!(step_from(2 << 20, 4 << 20), 3 << 20);
        assert_eq!(step_from(4 << 20, 4 << 20), (4 << 20) + LAST_STEP);
    }
}
