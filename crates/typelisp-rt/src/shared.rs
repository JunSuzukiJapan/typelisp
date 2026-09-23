//! The `Arc` handle Phase 1d of `docs/dev/os-threads-design.md` promotes
//! several `thread_local!` tables into — see that doc's §5 classification
//! table for which tables and why.
//!
//! Every table here already lived on exactly one OS thread, guarded by
//! nothing but the fact that only that thread ever touched it (a bare
//! `RefCell`). Wrapping them together in one `Arc<RtShared>` behind a
//! thread-local *handle* changes nothing about that today: there is still
//! exactly one handle per thread, created lazily on first use
//! ([`rt_shared`]) and never handed to anyone else, so a `cargo test`
//! worker thread that reuses an OS thread for a fresh `Heap`/`Interp` pair
//! still starts from an empty table exactly as it did before (nothing
//! survives the process boundary that isolates one test run from another
//! any differently than the old `RefCell`s did). What this buys for later
//! phases: `Heap::attach`'s multi-threaded counterpart (Phase 3) can clone
//! the *same* `Arc` onto a worker thread so it sees the same globals,
//! vtables and open streams as the thread that spawned it — cloning an
//! `Arc` is the only change multi-threading needs here, because the locks
//! are already in place.
//!
//! [`reset_global_table`](crate::reset_global_table)/
//! [`reset_vtable_table`](crate::reset_vtable_table) clear this handle's
//! fields in place rather than swap in a fresh `Arc`. That matters for
//! [`RtShared::streams`]: unlike the global/vtable tables, the stream table
//! deliberately is *not* reset by `Interp::new` (`crate::stream`'s module
//! doc comment explains why — a stream handle must outlive the `Interp`
//! that opened it), so replacing the whole `Arc` on every fresh `Interp`
//! would have thrown it away by accident.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use crate::stream::StreamTable;

/// Everything Phase 1d moves off a bare `thread_local!`. See the module doc
/// comment for what stays behind (still its own `thread_local!`, not a
/// field here) and why: `IN_FLIGHT_TAG`/`CAUGHT_UNWIND` move to per-task
/// state in Phase 1e instead, and `DYN_SLOT_CLOSURE`/`APPLY_INTERPRETED`
/// never move at all (§5: a worker thread has no interpreter to re-enter).
#[derive(Default)]
pub struct RtShared {
    /// [`crate::global_new`]'s id -> `Heap::permanent_root` position table
    /// — the old `GLOBAL_INDEX`.
    pub(crate) globals: RwLock<Vec<usize>>,
    /// vtable id -> slot -> `(native entry point, ABI)` — the old
    /// `VTABLES`.
    pub(crate) vtables: RwLock<Vec<Vec<(usize, u8)>>>,
    /// `(vtable id, trait id) -> supertrait vtable id` — the old `UPCASTS`.
    pub(crate) upcasts: RwLock<HashMap<(u32, u32), u32>>,
    /// This thread's open streams — see `crate::stream`'s module doc
    /// comment for why a handle here outlives any one `Interp`.
    pub(crate) streams: Mutex<StreamTable>,
}

thread_local! {
    /// This thread's `RtShared` handle — `None` until [`rt_shared`] first
    /// creates one.
    static RT_SHARED: RefCell<Option<Arc<RtShared>>> = const { RefCell::new(None) };
}

/// This thread's `RtShared`, creating one the first time it is asked for —
/// the same "auto-initializes on first use" behavior the bare
/// `thread_local!`s it replaces already had.
pub fn rt_shared() -> Arc<RtShared> {
    RT_SHARED.with(|cell| cell.borrow_mut().get_or_insert_with(|| Arc::new(RtShared::default())).clone())
}

/// Makes this thread use `shared` — another thread's [`rt_shared`] — so the
/// two see the same globals, vtables and open streams. What a thread that
/// runs on a heap it did not create (`Heap::attach`) calls before it runs
/// anything.
pub fn set_rt_shared(shared: Arc<RtShared>) {
    RT_SHARED.with(|cell| *cell.borrow_mut() = Some(shared));
}
