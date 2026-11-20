//! What the runtime asks of the machine it runs on: writing to standard output
//! and standard error, ending the process, the time, and sleeping.
//!
//! Everywhere but `wasm32-unknown-unknown` these are `std`'s. That target has
//! no operating system under it: its `std` throws standard output and error
//! away, panics on asking the time or sleeping, and traps on exiting. There
//! the page or program that loaded the module provides them, as functions it
//! imports from the module `typelisp` (what `typl -c --target
//! wasm32-unknown-unknown` writes beside the module says how).
//!
//! In this crate, the lowest the runtime has, so that every crate above can
//! report through it.

use std::time::Duration;

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
    }
}

/// Writes `bytes` to standard output.
pub fn write_stdout(bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        // SAFETY: the host reads `bytes` while the call lasts.
        unsafe { imports::write(1, bytes.as_ptr(), bytes.len()) };
        Ok(())
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        use std::io::Write;
        std::io::stdout().write_all(bytes)
    }
}

/// Writes `bytes` to standard error.
pub fn write_stderr(bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        // SAFETY: as in `write_stdout`.
        unsafe { imports::write(2, bytes.as_ptr(), bytes.len()) };
        Ok(())
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        use std::io::Write;
        std::io::stderr().write_all(bytes)
    }
}

/// Flushes standard output. The host's writes are not buffered here.
pub fn flush_stdout() -> std::io::Result<()> {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        Ok(())
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        use std::io::Write;
        std::io::stdout().flush()
    }
}

/// Flushes standard error.
pub fn flush_stderr() -> std::io::Result<()> {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        Ok(())
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        use std::io::Write;
        std::io::stderr().flush()
    }
}

/// `eprintln!` through [`write_stderr`]: one line, and a failure to write it
/// ignored, as `eprintln!`'s own is not (it panics) — what is reported here
/// is already a failure being reported.
pub fn eprintln(args: std::fmt::Arguments) {
    let line = format!("{}\n", args);
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
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    std::process::exit(code)
}

/// Waits `d`.
pub fn sleep(d: Duration) {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    // SAFETY: an import with no preconditions.
    unsafe {
        imports::sleep(d.as_secs_f64() * 1000.0)
    };
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
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
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.duration())
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use std::time::Instant;

/// A point on a clock that never goes back: `std::time::Instant`'s part the
/// runtime uses, read from the host.
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(Duration);

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl Instant {
    pub fn now() -> Instant {
        // SAFETY: an import with no preconditions.
        let ms = unsafe { imports::now() };
        Instant(Duration::from_secs_f64(ms.max(0.0) / 1000.0))
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

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl std::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, d: Duration) -> Instant {
        Instant(self.0 + d)
    }
}

/// Reports a Rust panic of the runtime's own through [`write_stderr`], since
/// `std`'s report goes nowhere on `wasm32-unknown-unknown`. Nothing to do
/// elsewhere.
pub fn install_panic_report() {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    std::panic::set_hook(Box::new(|info| eprintln(format_args!("{}", info))));
}
