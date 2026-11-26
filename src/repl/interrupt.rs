//! Ctrl-C while the REPL is evaluating: the signal asks the evaluation to
//! stop (`typelisp_abi::interrupt`) instead of ending the process.
//!
//! While `readline` waits for a line the terminal is in raw mode, so Ctrl-C
//! arrives there as a key and discards the line being typed, as it did
//! before this handler existed.

/// Installs the SIGINT handler and says an interrupt can now be requested.
#[cfg(unix)]
pub fn install() {
    extern "C" fn on_sigint(_: libc::c_int) {
        typelisp_abi::interrupt::request();
    }
    // SAFETY: `action` is fully initialised before the call, the handler
    // only stores to an atomic (async-signal-safe), and `SA_RESTART` keeps
    // a system call the signal lands in from failing with `EINTR`.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_sigint as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut()) != 0 {
            eprintln!("warning: Ctrl-C cannot stop an evaluation: {}", std::io::Error::last_os_error());
            return;
        }
    }
    typelisp_abi::interrupt::watch();
}

#[cfg(not(unix))]
pub fn install() {
    unimplemented!("stopping an evaluation with Ctrl-C needs POSIX signals")
}
