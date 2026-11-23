//! The host of a `wasm32-unknown-unknown` module: the page or program that
//! loaded it, through the functions it imports from the module `typelisp`.

use alloc::string::String;
use core::time::Duration;

pub use super::clock::Instant;

#[link(wasm_import_module = "typelisp")]
extern "C" {
    /// Writes `len` bytes at `ptr` to descriptor `fd` (1 standard output, 2
    /// standard error).
    fn write(fd: i32, ptr: *const u8, len: usize);
    /// Ends the program with `code`. Does not come back.
    #[link_name = "exit"]
    fn host_exit(code: i32);
    /// Milliseconds on a clock that never goes back.
    #[link_name = "now"]
    fn host_now() -> f64;
    /// Milliseconds since 1970-01-01T00:00:00Z.
    #[link_name = "unix_time"]
    fn host_unix_time() -> f64;
    /// Waits `ms` milliseconds.
    #[link_name = "sleep"]
    fn host_sleep(ms: f64);
    /// Minutes west of Greenwich in the local time zone at `ms`
    /// milliseconds since 1970-01-01T00:00:00Z, or NaN when the host
    /// cannot represent that instant.
    fn timezone_offset(ms: f64) -> f64;
    /// 1 when daylight saving time is in force locally at `ms`, 0 when
    /// not.
    fn daylight(ms: f64) -> i32;
}

/// The architecture this build is for, spelled as `std::env::consts::ARCH`
/// spells it.
pub const ARCH: &str = std::env::consts::ARCH;

/// The operating system this build is for, spelled as
/// `std::env::consts::OS` spells it.
pub const OS: &str = std::env::consts::OS;

/// Why reading or writing a standard stream failed. The host's own do not
/// fail; the type is `std`'s so that the other hosts' callers need nothing
/// different.
pub type HostError = std::io::Error;

fn host_write(fd: i32, bytes: &[u8]) {
    // SAFETY: the host reads `bytes` while the call lasts.
    unsafe { write(fd, bytes.as_ptr(), bytes.len()) }
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

/// Nothing to flush: the host's writes are not buffered here.
pub fn flush_stdout() -> Result<(), HostError> {
    Ok(())
}

/// Nothing to flush: the host's writes are not buffered here.
pub fn flush_stderr() -> Result<(), HostError> {
    Ok(())
}

/// The host gives no standard input: it reads as empty.
pub fn read_stdin(_buf: &mut [u8]) -> Result<usize, HostError> {
    Ok(0)
}

/// No standard input, so not a terminal.
pub fn stdin_is_tty() -> bool {
    false
}

/// Ends the program with `code`.
pub fn exit(code: i32) -> ! {
    // SAFETY: an import with no preconditions.
    unsafe { host_exit(code) };
    // The host leaves the module by throwing; nothing returns here.
    core::arch::wasm32::unreachable()
}

/// Ends the program at once.
pub fn abort() -> ! {
    std::process::abort()
}

/// Waits `d`.
pub fn sleep(d: Duration) {
    // SAFETY: an import with no preconditions.
    unsafe { host_sleep(d.as_secs_f64() * 1000.0) }
}

/// The time on the host's clock that never goes back, for [`Instant`].
pub(super) fn now() -> Duration {
    // SAFETY: an import with no preconditions.
    let ms = unsafe { host_now() };
    Duration::from_secs_f64(ms.max(0.0) / 1000.0)
}

/// The time since 1970-01-01T00:00:00Z, or, for a clock set before it, how
/// long before.
pub fn unix_time() -> Result<Duration, Duration> {
    // SAFETY: an import with no preconditions.
    let ms = unsafe { host_unix_time() };
    if ms < 0.0 {
        return Err(Duration::from_secs_f64(-ms / 1000.0));
    }
    Ok(Duration::from_secs_f64(ms / 1000.0))
}

/// One thread: a module without the threads proposal has no other.
pub fn available_parallelism() -> Result<usize, String> {
    Ok(1)
}

/// Reports a Rust panic of the runtime's own through [`write_stderr`], since
/// `std`'s report goes nowhere on this target.
pub fn install_panic_report() {
    std::panic::set_hook(alloc::boxed::Box::new(|info| super::eprintln(format_args!("{}", info))));
}

/// The local time zone at `unix_secs` seconds since 1970-01-01T00:00:00Z, as
/// `(seconds west of Greenwich, daylight saving in force)`, from
/// JavaScript's `Date`, or `None` when the host cannot represent that
/// instant.
pub fn timezone_at(unix_secs: i64) -> Option<(i32, bool)> {
    let ms = unix_secs as f64 * 1000.0;
    // SAFETY: imports with no preconditions.
    let west = unsafe { timezone_offset(ms) };
    if !west.is_finite() {
        return None;
    }
    let daylight = unsafe { daylight(ms) } != 0;
    Some(((west * 60.0).round() as i32, daylight))
}

/// Nothing says what CPU is under a module.
pub fn machine_version() -> Option<String> {
    None
}
