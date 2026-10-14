//! Whether this is a release build — `typl --release`.
//!
//! One flag for the whole process. `--release` only sets it; whatever behaves
//! differently in a release build asks it, at the time it does its work
//! (today: `eprint`/`eprintln`, which the checker leaves out —
//! `Checker::check_eprint`). An executable `compile-file` builds while the
//! flag is set carries it, so code it `eval`s asks the same question and
//! gets the same answer (`shim::rt_eval_state`).

use std::sync::atomic::{AtomicBool, Ordering};

static RELEASE: AtomicBool = AtomicBool::new(false);

pub fn is_release() -> bool {
    RELEASE.load(Ordering::Relaxed)
}

pub fn set_release(release: bool) {
    RELEASE.store(release, Ordering::Relaxed);
}
