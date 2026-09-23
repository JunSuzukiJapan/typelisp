//! The `PrintShared` `Arc` handle Phase 1d of `docs/dev/os-threads-design.md`
//! promotes `aot`'s `ENUM_NAMES` table into — see `typelisp_rt::shared`'s
//! module doc comment for the same story on the runtime crate's side; this
//! is its `typelisp-print` counterpart (§5's classification table splits
//! the thread-local tables across the two crates they already lived in).
//!
//! Only `enum_names` moves here in this phase. `aot`'s `FIELD_TEMPLATES`/
//! `PRINT_OBJECT`/`FORMAT_CALL` stay bare `thread_local!`s until Phase 5c
//! gives them the "type key -> compiled body address" shape a `typl`
//! worker thread needs (`docs/dev/os-threads-design.md` §7); `PRINTING` is
//! per-*call* re-entry-guard state for whichever `print-object` is running
//! on this thread right now, not runtime-entity data, so it never moves at
//! all.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

/// Everything Phase 1d moves off a bare `thread_local!` in `aot.rs` — see
/// the module doc comment for what stays behind and why.
#[derive(Default)]
pub struct PrintShared {
    /// `(base type key, variant index) -> variant name` — the old
    /// `ENUM_NAMES`.
    pub(crate) enum_names: RwLock<HashMap<(String, usize), String>>,
}

thread_local! {
    /// This thread's `PrintShared` handle — `None` until [`print_shared`]
    /// first creates one.
    static PRINT_SHARED: RefCell<Option<Arc<PrintShared>>> = const { RefCell::new(None) };
}

/// This thread's `PrintShared`, creating one the first time it is asked
/// for — the same "auto-initializes on first use" behavior the bare
/// `thread_local!` it replaces already had.
pub fn print_shared() -> Arc<PrintShared> {
    PRINT_SHARED.with(|cell| cell.borrow_mut().get_or_insert_with(|| Arc::new(PrintShared::default())).clone())
}
