//! What the runtime asks of the machine it runs on: writing to standard output
//! and standard error, ending the process, the time, sleeping, and (on
//! `wasm32-unknown-unknown` only) the local time zone.
//!
//! Everywhere but `wasm32-unknown-unknown` these are `std`'s. That target has
//! no operating system under it: its `std` throws standard output and error
//! away, panics on asking the time or sleeping, and traps on exiting. There
//! the page or program that loaded the module provides them, as functions it
//! imports from the module `typelisp` (what `typl -c --target
//! wasm32-unknown-unknown` writes beside the module says how).
//!
//! Without std (a microcontroller) there is no operating system either, and
//! the executable provides them as C functions (`typelisp_host_*`, in
//! [`board`]): the board's own way to write, stop and keep time.
//!
//! In this crate, the lowest the runtime has, so that every crate above can
//! report through it.

use core::time::Duration;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
mod imports {
    #[link(wasm_import_module = "typelisp")]
    extern "C" {
        /// Writes `len` bytes at `ptr` to descriptor `fd` (1 standard output, 2
        /// standard error).
        pub fn write(fd: i32, ptr: *const u8, len: usize);
        /// Ends the program with `code`. Does not come back.
        pub fn exit(code: i32);
        /// Milliseconds on a clock that never goes back.
        pub fn now() -> f64;
        /// Milliseconds since 1970-01-01T00:00:00Z.
        pub fn unix_time() -> f64;
        /// Waits `ms` milliseconds.
        pub fn sleep(ms: f64);
        /// Minutes west of Greenwich in the local time zone at `ms`
        /// milliseconds since 1970-01-01T00:00:00Z, or NaN when the host
        /// cannot represent that instant.
        pub fn timezone_offset(ms: f64) -> f64;
        /// 1 when daylight saving time is in force locally at `ms`, 0 when
        /// not.
        pub fn daylight(ms: f64) -> i32;
    }
}

/// What a board provides an executable built without std.
#[cfg(not(feature = "std"))]
pub mod board {
    extern "C" {
        /// Writes `len` bytes at `ptr` to descriptor `fd` (1 standard output, 2
        /// standard error).
        pub fn typelisp_host_write(fd: i32, ptr: *const u8, len: usize);
        /// Ends the program with `code`. Does not come back.
        pub fn typelisp_host_exit(code: i32) -> !;
        /// Microseconds on a clock that never goes back.
        pub fn typelisp_host_now_us() -> u64;
        /// Microseconds since 1970-01-01T00:00:00Z, negative before it.
        pub fn typelisp_host_unix_us() -> i64;
        /// Waits `us` microseconds.
        pub fn typelisp_host_sleep_us(us: u64);
    }
}

/// Why writing to standard output or error failed.
#[cfg(feature = "std")]
pub type HostError = std::io::Error;

/// Without std a board's write does not fail, so there is nothing to say.
#[cfg(not(feature = "std"))]
#[derive(Debug)]
pub enum HostError {}

#[cfg(not(feature = "std"))]
impl core::fmt::Display for HostError {
    fn fmt(&self, _: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {}
    }
}

/// Writes `bytes` to descriptor `fd` of the host (1 or 2).
#[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
fn host_write(fd: i32, bytes: &[u8]) {
    // SAFETY: the host reads `bytes` while the call lasts.
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    unsafe {
        imports::write(fd, bytes.as_ptr(), bytes.len())
    };
    #[cfg(not(feature = "std"))]
    unsafe {
        board::typelisp_host_write(fd, bytes.as_ptr(), bytes.len())
    };
}

/// Writes `bytes` to standard output.
pub fn write_stdout(bytes: &[u8]) -> Result<(), HostError> {
    #[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
    {
        host_write(1, bytes);
        Ok(())
    }
    #[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
    {
        use std::io::Write;
        std::io::stdout().write_all(bytes)
    }
}

/// Writes `bytes` to standard error.
pub fn write_stderr(bytes: &[u8]) -> Result<(), HostError> {
    #[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
    {
        host_write(2, bytes);
        Ok(())
    }
    #[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
    {
        use std::io::Write;
        std::io::stderr().write_all(bytes)
    }
}

/// Flushes standard output. The host's writes are not buffered here.
pub fn flush_stdout() -> Result<(), HostError> {
    #[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
    {
        Ok(())
    }
    #[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
    {
        use std::io::Write;
        std::io::stdout().flush()
    }
}

/// Flushes standard error.
pub fn flush_stderr() -> Result<(), HostError> {
    #[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
    {
        Ok(())
    }
    #[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
    {
        use std::io::Write;
        std::io::stderr().flush()
    }
}

/// `eprintln!` through [`write_stderr`]: one line, and a failure to write it
/// ignored, as `eprintln!`'s own is not (it panics) — what is reported here
/// is already a failure being reported.
pub fn eprintln(args: core::fmt::Arguments) {
    let line = alloc::format!("{}\n", args);
    let _ = write_stderr(line.as_bytes());
}

/// Ends the process with `code`, as `std::process::exit` does.
pub fn exit(code: i32) -> ! {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        // SAFETY: an import with no preconditions.
        unsafe { imports::exit(code) };
        // The host leaves the module by throwing; nothing returns here.
        core::arch::wasm32::unreachable()
    }
    #[cfg(not(feature = "std"))]
    // SAFETY: a board function with no preconditions.
    unsafe {
        board::typelisp_host_exit(code)
    }
    #[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
    std::process::exit(code)
}

/// Ends the process at once, as `std::process::abort` does: what a runtime
/// error that cannot be reported to the program does.
pub fn abort() -> ! {
    #[cfg(feature = "std")]
    std::process::abort();
    #[cfg(not(feature = "std"))]
    exit(134)
}

/// Waits `d`.
pub fn sleep(d: Duration) {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    // SAFETY: an import with no preconditions.
    unsafe {
        imports::sleep(d.as_secs_f64() * 1000.0)
    };
    #[cfg(not(feature = "std"))]
    // SAFETY: a board function with no preconditions.
    unsafe {
        board::typelisp_host_sleep_us(d.as_micros().min(u128::from(u64::MAX)) as u64)
    };
    #[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
    std::thread::sleep(d)
}

/// The time since 1970-01-01T00:00:00Z, or, for a clock set before it, how
/// long before.
pub fn unix_time() -> Result<Duration, Duration> {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        // SAFETY: an import with no preconditions.
        let ms = unsafe { imports::unix_time() };
        if ms < 0.0 {
            return Err(Duration::from_secs_f64(-ms / 1000.0));
        }
        Ok(Duration::from_secs_f64(ms / 1000.0))
    }
    #[cfg(not(feature = "std"))]
    {
        // SAFETY: a board function with no preconditions.
        let us = unsafe { board::typelisp_host_unix_us() };
        if us < 0 {
            return Err(Duration::from_micros(us.unsigned_abs()));
        }
        Ok(Duration::from_micros(us as u64))
    }
    #[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.duration())
}
/// The local time zone at `unix_secs` seconds since 1970-01-01T00:00:00Z, as
/// `(seconds west of Greenwich, daylight saving in force)`, or `None` when the
/// host cannot represent that instant. Elsewhere the C library answers
/// (`typelisp_rt::os::timezone_at`).
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub fn timezone_at(unix_secs: i64) -> Option<(i32, bool)> {
    let ms = unix_secs as f64 * 1000.0;
    // SAFETY: imports with no preconditions.
    let west = unsafe { imports::timezone_offset(ms) };
    if !west.is_finite() {
        return None;
    }
    let daylight = unsafe { imports::daylight(ms) } != 0;
    Some(((west * 60.0).round() as i32, daylight))
}

#[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
pub use std::time::Instant;

/// A point on a clock that never goes back: `std::time::Instant`'s part the
/// runtime uses, read from the host.
#[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(Duration);

#[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
impl Instant {
    pub fn now() -> Instant {
        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
        {
            // SAFETY: an import with no preconditions.
            let ms = unsafe { imports::now() };
            Instant(Duration::from_secs_f64(ms.max(0.0) / 1000.0))
        }
        #[cfg(not(feature = "std"))]
        // SAFETY: a board function with no preconditions.
        {
            Instant(Duration::from_micros(unsafe { board::typelisp_host_now_us() }))
        }
    }

    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.0.saturating_sub(earlier.0)
    }

    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.saturating_duration_since(earlier)
    }

    pub fn elapsed(&self) -> Duration {
        Instant::now().saturating_duration_since(*self)
    }
}

#[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
impl core::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, d: Duration) -> Instant {
        Instant(self.0 + d)
    }
}

/// Reports a Rust panic of the runtime's own through [`write_stderr`], since
/// `std`'s report goes nowhere on `wasm32-unknown-unknown`. Nothing to do
/// elsewhere: without std the executable's panic handler reports it.
pub fn install_panic_report() {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    std::panic::set_hook(alloc::boxed::Box::new(|info| eprintln(format_args!("{}", info))));
}
