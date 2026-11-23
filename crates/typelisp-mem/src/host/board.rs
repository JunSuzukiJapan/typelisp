//! The host of an executable built without std: the board, through the C
//! functions it provides (`typelisp_host_*`).

use alloc::string::String;
use core::time::Duration;

pub use super::clock::Instant;

extern "C" {
    /// Writes `len` bytes at `ptr` to descriptor `fd` (1 standard output, 2
    /// standard error).
    pub fn typelisp_host_write(fd: i32, ptr: *const u8, len: usize);
    /// Reads at most `len` bytes from descriptor `fd` (0 standard input)
    /// into `ptr`, waiting for at least one: how many were read, `0` at the
    /// end of input.
    pub fn typelisp_host_read(fd: i32, ptr: *mut u8, len: usize) -> usize;
    /// Ends the program with `code`. Does not come back.
    pub fn typelisp_host_exit(code: i32) -> !;
    /// Microseconds on a clock that never goes back.
    pub fn typelisp_host_now_us() -> u64;
    /// Microseconds since 1970-01-01T00:00:00Z, negative before it.
    pub fn typelisp_host_unix_us() -> i64;
    /// Waits `us` microseconds.
    pub fn typelisp_host_sleep_us(us: u64);
    /// `size` bytes aligned to `align` (a power of two), or null when
    /// there are none left.
    pub fn typelisp_host_alloc(size: usize, align: usize) -> *mut u8;
    /// Gives back what [`typelisp_host_alloc`] returned for the same
    /// `size` and `align`.
    pub fn typelisp_host_dealloc(ptr: *mut u8, size: usize, align: usize);
}

/// The memory the board hands out, as an allocator: what `std`'s `System`
/// is where there is an operating system.
pub struct BoardAlloc;

// SAFETY: the board's functions keep `GlobalAlloc`'s contract, which is what
// a board that provides them promises.
unsafe impl core::alloc::GlobalAlloc for BoardAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        typelisp_host_alloc(layout.size(), layout.align())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        typelisp_host_dealloc(ptr, layout.size(), layout.align())
    }
}

/// The architecture this build is for, spelled as `std::env::consts::ARCH`
/// spells it.
#[cfg(target_arch = "arm")]
pub const ARCH: &str = "arm";

#[cfg(not(target_arch = "arm"))]
compile_error!("typelisp_mem::host::ARCH does not name this architecture yet");

/// The operating system this build is for, spelled as
/// `std::env::consts::OS` spells it: `"none"` for a target with none.
#[cfg(target_os = "none")]
pub const OS: &str = "none";

#[cfg(not(target_os = "none"))]
compile_error!("typelisp_mem::host::OS does not name this operating system yet");

/// Why reading or writing a standard stream failed: a board's do not, so
/// there is nothing to say.
#[derive(Debug)]
pub enum HostError {}

impl core::fmt::Display for HostError {
    fn fmt(&self, _: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {}
    }
}

fn host_write(fd: i32, bytes: &[u8]) {
    // SAFETY: the board reads `bytes` while the call lasts.
    unsafe { typelisp_host_write(fd, bytes.as_ptr(), bytes.len()) }
}

/// Writes `bytes` to standard output.
pub fn write_stdout(bytes: &[u8]) -> Result<(), HostError> {
    host_write(1, bytes);
    Ok(())
}

/// Writes `bytes` to standard error.
pub fn write_stderr(bytes: &[u8]) -> Result<(), HostError> {
    host_write(2, bytes);
    Ok(())
}

/// Nothing to flush: the board's writes are not buffered here.
pub fn flush_stdout() -> Result<(), HostError> {
    Ok(())
}

/// Nothing to flush: the board's writes are not buffered here.
pub fn flush_stderr() -> Result<(), HostError> {
    Ok(())
}

/// Reads what standard input has into `buf`, waiting for at least one byte:
/// how many were read, `0` at the end of input.
pub fn read_stdin(buf: &mut [u8]) -> Result<usize, HostError> {
    // SAFETY: the board writes at most `buf.len()` bytes into `buf` while
    // the call lasts.
    Ok(unsafe { typelisp_host_read(0, buf.as_mut_ptr(), buf.len()) })
}

/// What the board's standard input is attached to is not known here, and a
/// terminal is what it would have to be shown to be.
pub fn stdin_is_tty() -> bool {
    false
}

/// Ends the program with `code`.
pub fn exit(code: i32) -> ! {
    // SAFETY: a board function with no preconditions.
    unsafe { typelisp_host_exit(code) }
}

/// Ends the program at once, with the code a C `abort` exits with.
pub fn abort() -> ! {
    exit(134)
}

/// Waits `d`.
pub fn sleep(d: Duration) {
    // SAFETY: a board function with no preconditions.
    unsafe { typelisp_host_sleep_us(d.as_micros().min(u128::from(u64::MAX)) as u64) }
}

/// The time on the board's clock that never goes back, for [`Instant`].
pub(super) fn now() -> Duration {
    // SAFETY: a board function with no preconditions.
    Duration::from_micros(unsafe { typelisp_host_now_us() })
}

/// The time since 1970-01-01T00:00:00Z, or, for a clock set before it, how
/// long before.
pub fn unix_time() -> Result<Duration, Duration> {
    // SAFETY: a board function with no preconditions.
    let us = unsafe { typelisp_host_unix_us() };
    if us < 0 {
        return Err(Duration::from_micros(us.unsigned_abs()));
    }
    Ok(Duration::from_micros(us as u64))
}

/// One core, one thread.
pub fn available_parallelism() -> Result<usize, String> {
    Ok(1)
}

/// Nothing to install: the executable's panic handler reports a panic.
pub fn install_panic_report() {}

/// A board has no time zone to ask about, so the local one is unknown, as
/// on WASI.
pub fn timezone_at(_unix_secs: i64) -> Option<(i32, bool)> {
    None
}

/// Nothing says what CPU is under the program.
pub fn machine_version() -> Option<String> {
    None
}
