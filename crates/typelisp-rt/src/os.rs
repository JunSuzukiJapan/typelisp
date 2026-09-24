//! The four things the operating system knows and `std` will not tell us.
//!
//! **This is the only module in the workspace that calls the C library
//! directly.** Everything else reaches libc the way all Rust does — through a
//! `std` wrapper that owns the `unsafe`. Here that ownership is ours, so it
//! is worth having one file to read rather than four scattered `unsafe`
//! blocks: every function below is a *safe* Rust signature over one C call,
//! and the `unsafe` never escapes.
//!
//! What forced it: CPU time (`getrusage`), the host's name and OS release
//! (`uname`), a file's owner (`getpwuid_r`) and the local time zone
//! (`localtime_r`). `std` has no equivalent of any of them, deliberately in
//! the last case — see [`timezone_at`].
//!
//! Since the C FFI, one more: the dynamic linker ([`dl_open`]/[`dl_sym`]),
//! which is how a `(defffi ...)` finds the function it names. Same rule as
//! the rest — a safe signature over one C call, `unsafe` kept inside.
//!
//! Since TCP, two more: [`poll_ready`], the one place a socket is waited on
//! (the scheduler's, not any stream's), and [`tcp_connect_begin`], a connect
//! that returns before the handshake does — `std` offers neither.
//!
//! Since tasks run on several OS threads, one more: [`SelfPipe`], which is
//! how one thread interrupts another's `poll`.
//!
//! Every one of these returns `Option`, and the `None`s are real: CL says
//! `machine-instance`, `machine-version`, `software-version` and
//! `file-author` may all answer `NIL`. That is what makes reporting a failure
//! honest rather than a made-up constant.
//!
//! **Not portable off Unix.** `getrusage`/`uname`/`getpwuid_r`/`tm_gmtoff`
//! are POSIX, and this workspace's first `#[cfg]` split is here. The non-Unix
//! arms are `unimplemented!()` rather than a plausible-looking answer.

/// Whole microseconds of CPU time this process has used — user plus system,
/// which is what CL's `get-internal-run-time` counts and what
/// `get-internal-real-time` deliberately does not.
///
/// `RUSAGE_SELF` sums every thread of the process. `getrusage` can only fail
/// on an invalid `who` or pointer, neither of which is reachable from here,
/// so the `None` arm is unreachable in practice — it is still an arm and not
/// a `.unwrap()`, because the caller has an honest thing to do with it.
#[cfg(unix)]
pub fn run_time_micros() -> Option<i64> {
    // SAFETY: `ru` is a fully-owned, zeroed `rusage` that outlives the call,
    // and `RUSAGE_SELF` is the constant libc defines for it. `getrusage`
    // writes only through the pointer we pass and touches no global state.
    let ru = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 {
            return None;
        }
        ru
    };
    let secs = ru.ru_utime.tv_sec as i64 + ru.ru_stime.tv_sec as i64;
    let usecs = ru.ru_utime.tv_usec as i64 + ru.ru_stime.tv_usec as i64;
    Some(secs * 1_000_000 + usecs)
}

#[cfg(not(unix))]
pub fn run_time_micros() -> Option<i64> {
    unimplemented!("get-internal-run-time needs getrusage, which is POSIX")
}

/// The host's name (`uname`'s `nodename`) — CL's `machine-instance`.
#[cfg(unix)]
pub fn host_name() -> Option<String> {
    uname_field(|u| u.nodename.as_ptr())
}

#[cfg(not(unix))]
pub fn host_name() -> Option<String> {
    unimplemented!("machine-instance needs uname, which is POSIX")
}

/// The operating system's release string (`uname`'s `release`, e.g.
/// `24.6.0`) — CL's `software-version`. Its companion `software-type` is a
/// compile-time constant and needs no call.
#[cfg(unix)]
pub fn os_release() -> Option<String> {
    uname_field(|u| u.release.as_ptr())
}

#[cfg(not(unix))]
pub fn os_release() -> Option<String> {
    unimplemented!("software-version needs uname, which is POSIX")
}

/// One NUL-terminated field of a fresh `utsname`.
///
/// `uname` is called once per field rather than cached: these are read at
/// most a handful of times in a program's life, and a cache would be one more
/// piece of process-global state to reason about.
#[cfg(unix)]
fn uname_field(field: impl Fn(&libc::utsname) -> *const libc::c_char) -> Option<String> {
    // SAFETY: `u` is a fully-owned, zeroed `utsname` that outlives the call.
    // `uname` fills it in and returns < 0 only on EFAULT, which a stack
    // pointer cannot provoke. The pointer `field` picks out is into `u`, so
    // it is valid for the `CStr` read below, and every `utsname` field is
    // NUL-terminated by contract.
    unsafe {
        let mut u: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut u) != 0 {
            return None;
        }
        let s = std::ffi::CStr::from_ptr(field(&u));
        s.to_str().ok().filter(|s| !s.is_empty()).map(|s| s.to_string())
    }
}

/// A human-readable name for the hardware — CL's `machine-version`.
///
/// No POSIX call answers this, so each platform is asked its own way and
/// anything else says `None` (which CL explicitly allows). Note the contrast
/// with the neighbours above: `machine-type` is `std::env::consts::ARCH`, the
/// architecture this *binary* was built for; this is the chip it is running
/// on.
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
    // CL allows `NIL` here, and this is the honest answer on a platform
    // nobody has taught this function about — not an `unimplemented!()`,
    // because "cannot be computed" is a value the specification defines.
    None
}

/// A `sysctl` string by name, sized by the two-call protocol `sysctlbyname`
/// specifies (ask with a null buffer, then read).
#[cfg(target_os = "macos")]
fn sysctl_string(name: &str) -> Option<String> {
    let cname = std::ffi::CString::new(name).ok()?;
    // SAFETY: both calls pass a NUL-terminated name and a `len` that
    // describes the buffer being written. The first passes a null buffer,
    // which is how `sysctlbyname` is asked for the size; the second passes a
    // `Vec` of exactly that size and cannot overrun it. `newp`/`newlen` are
    // null/0, so nothing is written back into the kernel.
    unsafe {
        let mut len: libc::size_t = 0;
        if libc::sysctlbyname(cname.as_ptr(), std::ptr::null_mut(), &mut len, std::ptr::null_mut(), 0) != 0 {
            return None;
        }
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if libc::sysctlbyname(
            cname.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
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

/// The login name that owns user id `uid`, or `None` when the id has no
/// entry in the password database — CL's `file-author` answering `NIL`.
#[cfg(unix)]
pub fn user_name(uid: u32) -> Option<String> {
    // The buffer size is negotiated rather than guessed: `getpwuid_r` returns
    // `ERANGE` when what it was given is too small, and a directory service
    // (LDAP, AD) can return entries far larger than the usual suggestion of
    // 1 KiB. Doubling until it fits terminates because the entry has a size.
    let mut buf: Vec<libc::c_char> = vec![0; 1024];
    loop {
        // SAFETY: `pwd` and `result` are owned locals that outlive the call;
        // `buf` is a live allocation of exactly `buf.len()` elements, which
        // is the length passed. `getpwuid_r` is the reentrant form — unlike
        // `getpwuid` it writes only through these pointers and returns no
        // pointer into shared state. `pwd.pw_name` points into `buf` and so
        // is read before either is dropped.
        let (rc, found, name) = unsafe {
            let mut pwd: libc::passwd = std::mem::zeroed();
            let mut result: *mut libc::passwd = std::ptr::null_mut();
            let rc = libc::getpwuid_r(uid as libc::uid_t, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result);
            let found = !result.is_null();
            let name = if rc == 0 && found && !pwd.pw_name.is_null() {
                std::ffi::CStr::from_ptr(pwd.pw_name).to_str().ok().map(|s| s.to_string())
            } else {
                None
            };
            (rc, found, name)
        };
        if rc == libc::ERANGE {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        // `rc != 0` is a lookup error and `!found` is "no such user"; CL's
        // `file-author` answers `NIL` to both, so they collapse here.
        return if rc == 0 && found { name } else { None };
    }
}

#[cfg(not(unix))]
pub fn user_name(_uid: u32) -> Option<String> {
    unimplemented!("file-author needs getpwuid_r, which is POSIX")
}

/// The login name owning `path` — CL's `file-author`.
///
/// Two failures, kept apart because CL keeps them apart: the file not being
/// there is an error, and its owner having no password-database entry is
/// `NIL`. Only the second is an `Ok(None)`.
///
/// The uid itself comes from `std` (`MetadataExt::uid`), which is why only
/// the name lookup needs [`user_name`].
#[cfg(unix)]
pub fn file_owner(path: &str) -> std::io::Result<Option<String>> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata(path)?.uid();
    Ok(user_name(uid))
}

#[cfg(not(unix))]
pub fn file_owner(_path: &str) -> std::io::Result<Option<String>> {
    unimplemented!("file-author needs a uid and getpwuid_r, which are POSIX")
}

/// The local time zone in effect at `unix_secs`, as `(seconds west of
/// Greenwich, daylight saving in effect)` — the pair SBCL's `get_timezone`
/// returns from the same call, for the same reason.
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
/// `setenv` builtin and `std::env::set_var` appears nowhere — and that used to
/// be the whole argument. **The C FFI ended it.** `(defffi (c-setenv "setenv")
/// (string string i32) i32)` is a line any program may now write, and no
/// promise this workspace makes about its own code can cover it.
///
/// So the obligation moved rather than disappeared: it belongs to whoever
/// writes that declaration, which is exactly what `(unsafe ...)` is for — the
/// mark that a premise nothing can check has been taken on. A program that
/// writes the environment from one thread while another asks for the time zone
/// has the race, and `docs/syntax.md` lists process-global state among the
/// things `unsafe` accepts responsibility for. Adding a `setenv` *builtin*
/// — reachable with no `unsafe` at all — is still the thing not to do.
///
/// [RUSTSEC-2020-0071]: https://rustsec.org/advisories/RUSTSEC-2020-0071
#[cfg(unix)]
pub fn timezone_at(unix_secs: i64) -> Option<(i32, bool)> {
    // SAFETY: `t` and `tm` are owned locals that outlive the call, and
    // `localtime_r` writes only through the `tm` pointer (the `_r` form keeps
    // no static state and returns either that same pointer or null). The
    // environment race the doc comment describes is ruled out by this
    // workspace never writing one.
    unsafe {
        let t = unix_secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        Some((-(tm.tm_gmtoff as i64) as i32, tm.tm_isdst > 0))
    }
}

#[cfg(not(unix))]
pub fn timezone_at(_unix_secs: i64) -> Option<(i32, bool)> {
    unimplemented!("the local time zone needs localtime_r and tm_gmtoff, which are POSIX")
}

/// Open the shared library at `path`, or `None` if it cannot be opened.
///
/// **Never closed.** JIT-compiled thunks and, in an AOT build, the program's
/// own code hold pointers into whatever this maps; the only lifetime that is
/// correct for those is the process's. `dlclose` would be a way to unmap code
/// that is about to be called.
///
/// `RTLD_NOW` so a missing symbol is a failure here rather than a crash at the
/// first call, and `RTLD_GLOBAL` so a library loaded later can resolve against
/// this one — the ordinary way a set of related libraries is loaded.
#[cfg(unix)]
pub fn dl_open(path: &str) -> Option<usize> {
    let c = std::ffi::CString::new(path).ok()?;
    // SAFETY: `c` is a NUL-terminated string that outlives the call, and the
    // flags are the constants libc defines. `dlopen` reads the name and
    // returns either a handle or null; it writes nothing through our pointer.
    let handle = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
    if handle.is_null() {
        None
    } else {
        Some(handle as usize)
    }
}

/// The address of `symbol` in `handle`, or `None` if it is not there.
///
/// A `handle` of 0 means the running process and everything already linked
/// into it (`RTLD_DEFAULT`) — which is how a declaration with no `:library`
/// reaches libc.
///
/// A symbol whose value is genuinely 0 would be indistinguishable from
/// absence. No such symbol is callable, which is all this is used for.
#[cfg(unix)]
pub fn dl_sym(handle: usize, symbol: &str) -> Option<usize> {
    let c = std::ffi::CString::new(symbol).ok()?;
    let h = if handle == 0 { libc::RTLD_DEFAULT } else { handle as *mut libc::c_void };
    // SAFETY: `c` outlives the call. `handle` is either `RTLD_DEFAULT` or a
    // value `dl_open` returned from `dlopen` and never closed, so it is still
    // a live handle. `dlsym` reads both and returns an address.
    let addr = unsafe { libc::dlsym(h, c.as_ptr()) };
    if addr.is_null() {
        None
    } else {
        Some(addr as usize)
    }
}

#[cfg(not(unix))]
pub fn dl_open(_path: &str) -> Option<usize> {
    unimplemented!("the FFI needs dlopen, which is POSIX")
}

#[cfg(not(unix))]
pub fn dl_sym(_handle: usize, _symbol: &str) -> Option<usize> {
    unimplemented!("the FFI needs dlsym, which is POSIX")
}

/// What a socket wait is for: the readiness `poll_ready` reports and the
/// scheduler parks a task on.
///
/// Defined here, in the crate that makes the `poll` call, so that the front
/// end (which owns the scheduler) and the builtins (which own the sockets)
/// name the same two things without either depending on the other for it.
/// The wire form is `net-wait`'s second argument: 0 readable, 1 writable.
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

/// Which of `fds` are ready, waiting at most `timeout` (`None`: as long as
/// it takes) — one `poll(2)` call over the whole set.
///
/// **The only place this program waits on a socket.** The scheduler calls
/// it when nothing else can run, and with a zero timeout before each task
/// switch; nothing else ever does. That is what makes every socket
/// operation the language exposes non-blocking by construction rather than
/// by discipline, and it is also the one piece that moves when tasks are
/// spread over OS threads: the *set* and the *call* stay, only the caller
/// changes.
///
/// The answer for each descriptor is a plain `bool`: readable or writable
/// as asked, **or in error, hung up, or invalid**. Those three are reported
/// as ready on purpose — a task waiting on a socket the peer has closed must
/// wake so that its next `read` can see the end of input, and one waiting on
/// a descriptor that has since been closed must wake so that its next
/// operation can fail with a message instead of parking forever.
///
/// `EINTR` is retried with the remaining timeout: a signal is not an answer.
#[cfg(unix)]
pub fn poll_ready(fds: &[(i32, Interest)], timeout: Option<std::time::Duration>) -> std::io::Result<Vec<(i32, bool)>> {
    let mut polls: Vec<libc::pollfd> = fds
        .iter()
        .map(|(fd, interest)| libc::pollfd {
            fd: *fd,
            events: match interest {
                Interest::Readable => libc::POLLIN,
                Interest::Writable => libc::POLLOUT,
            },
            revents: 0,
        })
        .collect();
    let deadline = timeout.map(|t| std::time::Instant::now() + t);
    loop {
        // Milliseconds, rounded *up*: rounding down would turn a 0.5ms wait
        // into a busy loop. `-1` is "no timeout".
        let ms: libc::c_int = match deadline {
            None => -1,
            Some(d) => {
                let left = d.saturating_duration_since(std::time::Instant::now());
                let ms = left.as_millis() + u128::from(left.subsec_nanos() % 1_000_000 != 0);
                libc::c_int::try_from(ms).unwrap_or(libc::c_int::MAX)
            }
        };
        // SAFETY: `polls` is a live, fully-initialised array of `pollfd`
        // whose length is passed alongside it, and it outlives the call.
        // `poll` writes only the `revents` fields and touches no global
        // state.
        let n = unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, ms) };
        if n >= 0 {
            break;
        }
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
    const READY_ANYWAY: libc::c_short = libc::POLLERR | libc::POLLHUP | libc::POLLNVAL;
    Ok(polls
        .iter()
        .map(|p| (p.fd, p.revents & (p.events | READY_ANYWAY) != 0))
        .collect())
}

#[cfg(not(unix))]
pub fn poll_ready(_fds: &[(i32, Interest)], _timeout: Option<std::time::Duration>) -> std::io::Result<Vec<(i32, bool)>> {
    unimplemented!("socket waits need poll, which is POSIX")
}

/// A pipe a thread writes a byte into to wake another thread out of
/// [`poll_ready`] — the scheduler's poller includes [`Self::read_fd`] in
/// every set it polls, and whoever changes what the poller should be
/// waiting for (a new socket wait, a nearer deadline, a task made ready)
/// calls [`Self::kick`].
///
/// Both ends are non-blocking: a full pipe means a wake is already pending,
/// so a `kick` that cannot write has nothing left to do, and [`Self::drain`]
/// reads until there is nothing. Close-on-exec, so a `run-program` child
/// does not inherit it.
pub struct SelfPipe {
    read: i32,
    write: i32,
}

#[cfg(unix)]
impl SelfPipe {
    pub fn new() -> std::io::Result<SelfPipe> {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: `fds` is a writable array of the two descriptors `pipe`
        // fills.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let pipe = SelfPipe { read: fds[0], write: fds[1] };
        for fd in fds {
            // SAFETY: `fd` was just returned by `pipe` and is owned by
            // `pipe`, whose `Drop` closes it if this fails.
            unsafe {
                let fl = libc::fcntl(fd, libc::F_GETFL);
                if fl < 0 || libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let fd_fl = libc::fcntl(fd, libc::F_GETFD);
                if fd_fl < 0 || libc::fcntl(fd, libc::F_SETFD, fd_fl | libc::FD_CLOEXEC) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
        }
        Ok(pipe)
    }

    /// The descriptor to poll for readability.
    pub fn read_fd(&self) -> i32 {
        self.read
    }

    /// Wakes the poller, if one is waiting — and makes the next poll return
    /// at once if none is yet.
    pub fn kick(&self) {
        let byte = 1u8;
        // SAFETY: one byte from a live local. The result is ignored on
        // purpose: the only failure a non-blocking pipe write can have here
        // is `EAGAIN`, a full pipe — a wake is already pending.
        unsafe { libc::write(self.write, &byte as *const u8 as *const libc::c_void, 1) };
    }

    /// Empties the pipe after a poll that it woke.
    pub fn drain(&self) {
        let mut buf = [0u8; 64];
        // SAFETY: reads into a live local buffer of the length passed.
        while unsafe { libc::read(self.read, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) } > 0 {}
    }
}

#[cfg(not(unix))]
impl SelfPipe {
    pub fn new() -> std::io::Result<SelfPipe> {
        unimplemented!("the scheduler's poller wakes through a pipe, which is POSIX")
    }

    pub fn read_fd(&self) -> i32 {
        unimplemented!("the scheduler's poller wakes through a pipe, which is POSIX")
    }

    pub fn kick(&self) {
        unimplemented!("the scheduler's poller wakes through a pipe, which is POSIX")
    }

    pub fn drain(&self) {
        unimplemented!("the scheduler's poller wakes through a pipe, which is POSIX")
    }
}

impl Drop for SelfPipe {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: both descriptors are this value's, and nothing uses them
        // after it is dropped.
        unsafe {
            libc::close(self.read);
            libc::close(self.write);
        }
    }
}

/// Starts a TCP connection to `addr` without waiting for it: the socket
/// comes back non-blocking with the connect *in progress*. The caller waits
/// for it to become writable ([`poll_ready`]) and then asks
/// `TcpStream::take_error` whether the connect succeeded.
///
/// `std` has no way to say this — `TcpStream::connect` blocks until the
/// handshake completes or times out, which for an unreachable host is
/// the better part of a minute with every task stopped. So the three calls
/// are made here: `socket`, `fcntl(O_NONBLOCK)`, `connect`, with
/// `EINPROGRESS` being the expected answer rather than a failure. Anything
/// else `connect` reports (a refused loopback connect can fail at once) is
/// returned as the error it is.
#[cfg(unix)]
pub fn tcp_connect_begin(addr: std::net::SocketAddr) -> std::io::Result<std::net::TcpStream> {
    use std::os::unix::io::FromRawFd;
    let family = match addr {
        std::net::SocketAddr::V4(_) => libc::AF_INET,
        std::net::SocketAddr::V6(_) => libc::AF_INET6,
    };
    // SAFETY: `socket` takes three integers and returns a descriptor or -1.
    let fd = unsafe { libc::socket(family, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // From here the descriptor is owned by `stream`, whose `Drop` closes it
    // on every error path below.
    // SAFETY: `fd` was just returned by `socket` and nothing else owns it.
    let stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };
    stream.set_nonblocking(true)?;
    // The address as the kernel wants it. `sockaddr_storage` is large enough
    // for either family; only the family's own prefix is written.
    let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let len: libc::socklen_t = match addr {
        std::net::SocketAddr::V4(a) => {
            let sin = libc::sockaddr_in {
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: a.port().to_be(),
                sin_addr: libc::in_addr { s_addr: u32::from_ne_bytes(a.ip().octets()) },
                sin_zero: [0; 8],
                #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
                sin_len: std::mem::size_of::<libc::sockaddr_in>() as u8,
            };
            // SAFETY: `storage` is at least as large as `sockaddr_in`, and both
            // are plain-old-data.
            unsafe { std::ptr::write(&mut storage as *mut _ as *mut libc::sockaddr_in, sin) };
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t
        }
        std::net::SocketAddr::V6(a) => {
            let sin6 = libc::sockaddr_in6 {
                sin6_family: libc::AF_INET6 as libc::sa_family_t,
                sin6_port: a.port().to_be(),
                sin6_flowinfo: a.flowinfo(),
                sin6_addr: libc::in6_addr { s6_addr: a.ip().octets() },
                sin6_scope_id: a.scope_id(),
                #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
                sin6_len: std::mem::size_of::<libc::sockaddr_in6>() as u8,
            };
            // SAFETY: as above, for `sockaddr_in6`.
            unsafe { std::ptr::write(&mut storage as *mut _ as *mut libc::sockaddr_in6, sin6) };
            std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t
        }
    };
    // SAFETY: `fd` is the live descriptor `stream` owns, `storage` holds a
    // fully-initialised address of `len` bytes and outlives the call.
    let rc = unsafe { libc::connect(fd, &storage as *const _ as *const libc::sockaddr, len) };
    if rc == 0 {
        return Ok(stream);
    }
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EINPROGRESS) => Ok(stream),
        _ => Err(err),
    }
}

/// Turns TCP keepalive probes on or off for a connected socket
/// (`SO_KEEPALIVE`). With them on, an idle connection whose peer has
/// silently gone (a pulled cable, a crashed host) is noticed and reset,
/// where without them it stays open forever; `std` dropped its setter for
/// this, so the call is made here.
#[cfg(unix)]
pub fn set_keepalive(fd: i32, on: bool) -> std::io::Result<()> {
    setsockopt_int(fd, libc::SOL_SOCKET, libc::SO_KEEPALIVE, i32::from(on))
}

/// How long a connection must be idle before the first keepalive probe,
/// and then how long between probes — both `secs`, since one number is
/// what a program has in mind ("notice a dead peer within about a
/// minute") and Go's `SetKeepAlivePeriod` sets the two together the same
/// way. The names of the options differ per platform (`TCP_KEEPALIVE` on
/// macOS is Linux's `TCP_KEEPIDLE`); the meaning is the same.
#[cfg(target_os = "macos")]
pub fn set_keepalive_period(fd: i32, secs: u32) -> std::io::Result<()> {
    setsockopt_int(fd, libc::IPPROTO_TCP, libc::TCP_KEEPALIVE, secs as i32)?;
    setsockopt_int(fd, libc::IPPROTO_TCP, libc::TCP_KEEPINTVL, secs as i32)
}

#[cfg(target_os = "linux")]
pub fn set_keepalive_period(fd: i32, secs: u32) -> std::io::Result<()> {
    setsockopt_int(fd, libc::IPPROTO_TCP, libc::TCP_KEEPIDLE, secs as i32)?;
    setsockopt_int(fd, libc::IPPROTO_TCP, libc::TCP_KEEPINTVL, secs as i32)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn set_keepalive_period(_fd: i32, _secs: u32) -> std::io::Result<()> {
    unimplemented!("keepalive period: only macOS and Linux name the socket options")
}

#[cfg(unix)]
fn setsockopt_int(fd: i32, level: i32, name: i32, value: i32) -> std::io::Result<()> {
    // SAFETY: `fd` is an open socket the caller owns; the option value is an
    // `int` the kernel copies from `value`'s address, with the length given.
    let rc = unsafe {
        libc::setsockopt(
            fd,
            level,
            name,
            &value as *const i32 as *const libc::c_void,
            std::mem::size_of::<i32>() as libc::socklen_t,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
pub fn set_keepalive(_fd: i32, _on: bool) -> std::io::Result<()> {
    unimplemented!("keepalive: only implemented on Unix")
}

#[cfg(not(unix))]
pub fn tcp_connect_begin(_addr: std::net::SocketAddr) -> std::io::Result<std::net::TcpStream> {
    unimplemented!("a non-blocking connect needs socket/fcntl/connect, which are POSIX")
}

#[cfg(test)]
mod tests {
    //! Shape, not values: every answer here is the machine's, so a test can
    //! only assert what must hold on any of them. Together they still catch
    //! the failure that matters — a wrong `struct` layout or a bad pointer
    //! shows up as garbage or a crash, not as a plausible-but-different
    //! string.

    #[test]
    fn cpu_time_is_present_and_advances() {
        let a = super::run_time_micros().expect("getrusage(RUSAGE_SELF) works");
        assert!(a >= 0, "negative CPU time: {}", a);
        // Burn a little, then read again. Monotonic by construction, and the
        // second reading proves the field is being read rather than a
        // constant returned.
        let mut acc = 0u64;
        for i in 0..2_000_000u64 {
            acc = acc.wrapping_add(i);
        }
        assert!(acc > 0);
        let b = super::run_time_micros().expect("getrusage works twice");
        assert!(b >= a, "CPU time went backwards: {} -> {}", a, b);
    }

    #[test]
    fn the_host_has_a_name_and_a_release() {
        let host = super::host_name().expect("every Unix host has a nodename");
        assert!(!host.is_empty());
        // A NUL-terminated field read past its end would drag the next one
        // in; `utsname`'s fields are adjacent, so this is the check that
        // catches it.
        assert!(!host.contains('\0'), "the field was read past its terminator: {:?}", host);
        let rel = super::os_release().expect("every Unix reports a release");
        assert!(!rel.is_empty() && !rel.contains('\0'), "{:?}", rel);
    }

    #[test]
    fn the_current_user_has_a_name() {
        // SAFETY: `getuid` takes nothing, returns a value, and cannot fail.
        let uid = unsafe { libc::getuid() };
        let who = super::user_name(uid).expect("the running user has a passwd entry");
        assert!(!who.is_empty() && !who.contains('\0'), "{:?}", who);
    }

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

    #[test]
    fn the_process_resolves_its_own_libc_symbols() {
        // `abs` is in libc, and this process links libc.
        assert!(super::dl_sym(0, "abs").is_some(), "abs is not resolvable in this process");
        assert!(
            super::dl_sym(0, "a_symbol_no_library_defines_xyzzy").is_none(),
            "a name nothing defines resolved to something"
        );
    }

    #[test]
    fn a_library_that_is_not_there_does_not_open() {
        assert!(super::dl_open("libnothing-typelisp-probe.so").is_none());
    }

    #[test]
    fn a_file_has_an_owner_and_a_missing_one_is_an_error() {
        let me = super::file_owner(env!("CARGO_MANIFEST_DIR")).expect("the crate directory exists");
        assert!(me.is_some(), "the crate directory has no owner name");
        let missing = super::file_owner("/nonexistent-typelisp-probe-path");
        assert!(missing.is_err(), "a missing path must be an error, not a `none`");
    }
}
