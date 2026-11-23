//! The host through `std`, over an operating system (WASI included).
//!
//! Two answers `std` does not give come from the C library: the local time
//! zone (`localtime_r`) and, on macOS, the CPU's name (`sysctlbyname`). Each
//! is a safe signature over one C call, with the `unsafe` kept inside.

use alloc::string::{String, ToString};
use core::time::Duration;
use std::io::{IsTerminal, Read, Write};

pub use std::time::Instant;

/// The architecture this build is for, spelled as `std::env::consts::ARCH`
/// spells it.
pub const ARCH: &str = std::env::consts::ARCH;

/// The operating system this build is for, spelled as
/// `std::env::consts::OS` spells it.
pub const OS: &str = std::env::consts::OS;

/// Why reading or writing a standard stream failed.
pub type HostError = std::io::Error;

/// Writes `bytes` to standard output.
pub fn write_stdout(bytes: &[u8]) -> Result<(), HostError> {
    std::io::stdout().write_all(bytes)
}

/// Writes `bytes` to standard error.
pub fn write_stderr(bytes: &[u8]) -> Result<(), HostError> {
    std::io::stderr().write_all(bytes)
}

/// Flushes standard output.
pub fn flush_stdout() -> Result<(), HostError> {
    std::io::stdout().flush()
}

/// Flushes standard error.
pub fn flush_stderr() -> Result<(), HostError> {
    std::io::stderr().flush()
}

/// Reads what standard input has into `buf`, waiting for at least one byte:
/// how many were read, `0` at the end of input.
pub fn read_stdin(buf: &mut [u8]) -> Result<usize, HostError> {
    std::io::stdin().lock().read(buf)
}

/// Whether standard input is a terminal.
pub fn stdin_is_tty() -> bool {
    std::io::stdin().is_terminal()
}

/// Ends the process with `code`.
pub fn exit(code: i32) -> ! {
    std::process::exit(code)
}

/// Ends the process at once: what a runtime error that cannot be reported to
/// the program does.
pub fn abort() -> ! {
    std::process::abort()
}

/// Waits `d`.
pub fn sleep(d: Duration) {
    std::thread::sleep(d)
}

/// The time since 1970-01-01T00:00:00Z, or, for a clock set before it, how
/// long before.
pub fn unix_time() -> Result<Duration, Duration> {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.duration())
}

/// How many threads could run at once, as the system counts it.
pub fn available_parallelism() -> Result<usize, String> {
    std::thread::available_parallelism().map(|n| n.get()).map_err(|e| e.to_string())
}

/// Nothing to install: `std` reports a panic itself.
pub fn install_panic_report() {}

/// The local time zone in effect at `unix_secs`, as `(seconds west of
/// Greenwich, daylight saving in effect)` — the pair SBCL's `get_timezone`
/// returns from the same call, for the same reason — or `None` when the C
/// library cannot represent that instant.
///
/// **Why the offset takes a time**: it is not a property of the machine but
/// of the instant. The same host is 5 hours west in January and 4 in July,
/// and a program decoding a stored timestamp must use the rules that were in
/// force *then*.
///
/// Sign: CL's zone is positive going **west**, `tm_gmtoff` is positive going
/// **east**, so this negates. Seconds and not hours because the offset is not
/// always a whole number of them — India is +5:30, Nepal +5:45, Chatham
/// +12:45 — and rounding here would be a silent lie for a billion people.
///
/// **On thread safety.** `localtime_r` reads `TZ` through `getenv`, which
/// races with any concurrent `setenv`: that is [RUSTSEC-2020-0071], and it is
/// why Rust made `std::env::set_var` `unsafe` in edition 2024 rather than
/// blame the C library.
///
/// Nothing in this workspace writes an environment variable — there is no
/// `setenv` builtin and `std::env::set_var` appears nowhere. The C FFI is the
/// way a program can: `(defffi (c-setenv "setenv") (string string i32) i32)`
/// is a line any program may write, and no promise this workspace makes about
/// its own code can cover it. The obligation belongs to whoever writes that
/// declaration, which is exactly what `(unsafe ...)` is for — the mark that a
/// premise nothing can check has been taken on. A program that writes the
/// environment from one thread while another asks for the time zone has the
/// race, and `docs/ja/reference/syntax.md` §4.1 lists process-global state
/// among the things `unsafe` accepts responsibility for. Adding a `setenv`
/// *builtin* — reachable with no `unsafe` at all — is the thing not to do.
///
/// [RUSTSEC-2020-0071]: https://rustsec.org/advisories/RUSTSEC-2020-0071
#[cfg(unix)]
pub fn timezone_at(unix_secs: i64) -> Option<(i32, bool)> {
    // SAFETY: `t` and `tm` are owned locals that outlive the call, and
    // `localtime_r` writes only through the `tm` pointer (the `_r` form keeps
    // no static state and returns either that same pointer or null). The
    // environment race the doc comment describes is the declaring program's.
    unsafe {
        let t = unix_secs as libc::time_t;
        let mut tm: libc::tm = core::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        Some((-(tm.tm_gmtoff as i64) as i32, tm.tm_isdst > 0))
    }
}

/// WASI preview 1 has no time zone to ask about, so the local one is unknown
/// at every instant: the prelude's `local-zone-west` panics on that `None`,
/// which is where a program that decodes into local time stops.
#[cfg(target_os = "wasi")]
pub fn timezone_at(_unix_secs: i64) -> Option<(i32, bool)> {
    None
}

#[cfg(all(not(unix), not(target_os = "wasi")))]
pub fn timezone_at(_unix_secs: i64) -> Option<(i32, bool)> {
    unimplemented!("the local time zone needs localtime_r and tm_gmtoff, which are POSIX")
}

/// A human-readable name for the CPU this is running on — CL's
/// `machine-version` — or `None` where nothing can say (which CL allows).
///
/// Not the architecture this binary was built for ([`ARCH`], CL's
/// `machine-type`): the chip under it.
#[cfg(target_os = "macos")]
pub fn machine_version() -> Option<String> {
    sysctl_string("machdep.cpu.brand_string")
}

#[cfg(target_os = "linux")]
pub fn machine_version() -> Option<String> {
    // `/proc/cpuinfo` rather than a syscall: there is no sysctl for this on
    // Linux, and the file is the source every tool reads. x86 spells the
    // field `model name`; on ARM the closest is `Model` (the board), so both
    // are accepted, in that order of preference.
    let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().to_string()))
            .filter(|v| !v.is_empty())
    };
    field("model name").or_else(|| field("Model"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn machine_version() -> Option<String> {
    // "Cannot be computed" is a value the specification defines.
    None
}

/// A `sysctl` string by name, sized by the two-call protocol `sysctlbyname`
/// specifies (ask with a null buffer, then read).
#[cfg(target_os = "macos")]
fn sysctl_string(name: &str) -> Option<String> {
    let cname = alloc::ffi::CString::new(name).ok()?;
    // SAFETY: both calls pass a NUL-terminated name and a `len` that
    // describes the buffer being written. The first passes a null buffer,
    // which is how `sysctlbyname` is asked for the size; the second passes a
    // `Vec` of exactly that size and cannot overrun it. `newp`/`newlen` are
    // null/0, so nothing is written back into the kernel.
    unsafe {
        let mut len: libc::size_t = 0;
        if libc::sysctlbyname(cname.as_ptr(), core::ptr::null_mut(), &mut len, core::ptr::null_mut(), 0) != 0 {
            return None;
        }
        if len == 0 {
            return None;
        }
        let mut buf = alloc::vec![0u8; len];
        if libc::sysctlbyname(
            cname.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            core::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        // The value is NUL-terminated and `len` counts the NUL.
        buf.truncate(len.saturating_sub(1));
        String::from_utf8(buf).ok().filter(|s| !s.is_empty())
    }
}

#[cfg(all(test, unix))]
mod tests {
    //! Shape, not values: every answer here is the machine's, so a test can
    //! only assert what must hold on any of them.

    #[test]
    fn the_local_zone_is_a_plausible_offset() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_secs() as i64;
        let (west, _dst) = super::timezone_at(now).expect("the host has a local zone");
        // Real offsets run from +14:00 (Kiritimati, -50400 west) to -12:00
        // (Baker Island, 43200 west).
        assert!((-50_400..=43_200).contains(&west), "implausible offset: {}s west", west);
        // Every zone in use is a whole number of minutes; the seconds-level
        // offsets are pre-1900 local mean time.
        assert_eq!(west % 60, 0, "offset is not a whole number of minutes: {}s", west);
    }

    /// The offset is a property of the instant, not of the machine — which is
    /// why [`super::timezone_at`] takes one. A zone that observes daylight
    /// saving reports different offsets in January and July; one that does
    /// not reports the same. Both are correct, so the assertion is only that
    /// asking twice does not fail.
    #[test]
    fn the_zone_can_be_asked_about_any_instant() {
        let jan = super::timezone_at(1_704_067_200).expect("2024-01-01");
        let jul = super::timezone_at(1_720_000_000).expect("2024-07-03");
        assert!(jan.0 % 60 == 0 && jul.0 % 60 == 0);
        // A pre-Unix-epoch instant is an ordinary negative `time_t`, and
        // `decode-universal-time` reaches one for any date before 1970.
        let pre = super::timezone_at(-2_208_988_800).expect("1900-01-01");
        assert!((-50_400..=43_200).contains(&pre.0), "1900: {}s west", pre.0);
    }
}
