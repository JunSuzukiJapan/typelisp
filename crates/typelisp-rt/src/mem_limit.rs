//! The upper bound on what a process allocates (`typl -c --max-memory`).
//!
//! Every allocation of the process goes through [`CountingAlloc`], which
//! keeps the number of bytes currently allocated. Everything counts: the cons
//! arena, the string and box stores, and the runtime's own Rust values. The
//! bound is unset (unbounded) until [`set_memory_limit`] gives one; an
//! executable's `main` sets it through `rt_heap_init` before anything else
//! allocates on its behalf.
//!
//! An allocation that would take the total over the bound is not made: the
//! process prints why and exits with [`crate::program::EXIT_CODE_PANIC`]. It
//! cannot be a `panic` a `catch` sees — unwinding out of an allocator is not
//! allowed — and it must not allocate while it reports, since allocating is
//! what just failed.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The process's allocator: [`System`], counted.
pub struct CountingAlloc;

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

/// Bytes currently allocated.
static IN_USE: AtomicUsize = AtomicUsize::new(0);

/// The bound on [`IN_USE`]; `usize::MAX` is no bound.
static LIMIT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Bounds the process's allocations at `bytes` from now on. Must be called
/// before the bound matters: what is already allocated counts toward it, so a
/// bound below the current use fails the next allocation.
pub fn set_memory_limit(bytes: usize) {
    LIMIT.store(bytes, Ordering::Relaxed);
}

/// Bytes the process has allocated and not freed.
pub fn memory_in_use() -> usize {
    IN_USE.load(Ordering::Relaxed)
}

/// Takes `size` more bytes into [`IN_USE`], or does not and reports why the
/// bound refuses them.
fn reserve(size: usize) {
    let before = IN_USE.fetch_add(size, Ordering::Relaxed);
    let limit = LIMIT.load(Ordering::Relaxed);
    if before.checked_add(size).is_none_or(|after| after > limit) {
        IN_USE.fetch_sub(size, Ordering::Relaxed);
        exceeded(size, before, limit);
    }
}

fn release(size: usize) {
    IN_USE.fetch_sub(size, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        reserve(layout.size());
        let p = System.alloc(layout);
        if p.is_null() {
            release(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        reserve(layout.size());
        let p = System.alloc_zeroed(layout);
        if p.is_null() {
            release(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        release(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old_size = layout.size();
        if new_size > old_size {
            reserve(new_size - old_size);
        }
        let p = System.realloc(ptr, layout, new_size);
        if p.is_null() {
            if new_size > old_size {
                release(new_size - old_size);
            }
        } else if new_size < old_size {
            release(old_size - new_size);
        }
        p
    }
}

/// Reports the refused allocation and ends the process. Writes from a buffer
/// on the stack: nothing here may allocate.
fn exceeded(size: usize, in_use: usize, limit: usize) -> ! {
    let mut buf = [0u8; 192];
    let mut len = 0;
    for part in [
        Part::Text("typelisp runtime error: memory limit exceeded: "),
        Part::Number(size),
        Part::Text(" more bytes requested with "),
        Part::Number(in_use),
        Part::Text(" in use (limit "),
        Part::Number(limit),
        Part::Text(")\n"),
    ] {
        len += part.write(&mut buf[len..]);
    }
    // Straight to the descriptor: `std::io::stderr()`'s lock asks for the
    // current thread's identity, whose first use allocates — which comes
    // back here. Likewise `process::exit` runs the C library's exit handlers,
    // which may allocate; `_exit` ends the process at once.
    let mut written = 0;
    while written < len {
        let n = unsafe { libc::write(2, buf[written..len].as_ptr().cast(), len - written) };
        if n <= 0 {
            break;
        }
        written += n as usize;
    }
    unsafe { libc::_exit(crate::program::EXIT_CODE_PANIC as libc::c_int) }
}

enum Part {
    Text(&'static str),
    Number(usize),
}

impl Part {
    /// Writes itself at the start of `out` and returns how many bytes it took.
    fn write(&self, out: &mut [u8]) -> usize {
        match self {
            Part::Text(s) => {
                out[..s.len()].copy_from_slice(s.as_bytes());
                s.len()
            }
            Part::Number(n) => {
                let mut digits = [0u8; 20];
                let mut k = digits.len();
                let mut n = *n;
                loop {
                    k -= 1;
                    digits[k] = b'0' + (n % 10) as u8;
                    n /= 10;
                    if n == 0 {
                        break;
                    }
                }
                let d = &digits[k..];
                out[..d.len()].copy_from_slice(d);
                d.len()
            }
        }
    }
}
