//! Collection, and stopping the threads that share a heap for it.

use core::ptr;
use core::cell::UnsafeCell;
use alloc::sync::Arc;
use crate::prelude::*;
use crate::sync::{Condvar, Mutex, ThreadKey};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use crate::value::{BoxedObj, Cell, StructPayload, Value};
use super::{FrameArena, FrameRaw, Heap};

// ---- stop-the-world (`docs/dev/os-threads-design.md` §4) -------------------

/// A view is running Lisp code and may touch the heap at any moment.
const RUNNING: u8 = 0;
/// A view is parked inside [`Heap::safepoint`] (or a demoted [`Heap::gc`])
/// until the collection in progress ends.
const AT_SAFEPOINT: u8 = 1;
/// A view is between [`Heap::enter_native`] and [`Heap::leave_native`]: it
/// promises not to touch the heap, so a collection may run without waiting
/// for it.
const NATIVE: u8 = 2;

/// How long a collector waits for the other threads to stop before it
/// names the ones that have not — see [`Heap::gc`]. Not a timeout: it keeps
/// waiting afterwards. A thread that never stops is a thread that blocked
/// without [`Heap::enter_native`], and the only useful thing to do about
/// that is say which one it was.
const WATCHDOG: core::time::Duration = core::time::Duration::from_secs(5);

/// One view's entry in [`ThreadRegistry`]: what a collector running on a
/// *different* view has to know about it.
pub(super) struct ViewSlot {
    /// `RUNNING` / `AT_SAFEPOINT` / `NATIVE`. Written only by the owning
    /// view; read by collectors.
    state: AtomicU8,
    /// Which OS thread owns the view. A collector does not wait for views
    /// on its own thread: they cannot be in the middle of anything while
    /// this thread is collecting (the tests that attach a second view on
    /// the same thread depend on that).
    thread: ThreadKey,
    /// For the watchdog's message.
    name: String,
    /// The view's session roots — see [`Heap::push_session_root`]. Here
    /// rather than on `Heap` because a collector on another view has to
    /// mark them.
    pub(super) session_roots: UnsafeCell<Vec<Value>>,
}

// `session_roots` is the only field that is not already `Sync`. It is
// written only by the owning view, and read by a collector only while the
// owner is stopped (`AT_SAFEPOINT`/`NATIVE`, i.e. not touching it) or is
// the collector's own thread — the same exclusion every other heap
// structure relies on during a stop-the-world collection.
unsafe impl Sync for ViewSlot {}

/// Every view attached to one [`HeapShared`], and the flag that stops them
/// all. See [`Heap::gc`] for the protocol.
///
/// `views` is also the `Mutex` `changed` waits with. A collector holds it
/// from the moment every other thread has stopped until the collection
/// ends, which is what keeps a view from attaching, detaching or waking up
/// half-way through one.
pub(super) struct ThreadRegistry {
    gc_requested: AtomicBool,
    views: Mutex<Vec<Arc<ViewSlot>>>,
    changed: Condvar,
}

impl ThreadRegistry {
    pub(super) fn new() -> ThreadRegistry {
        ThreadRegistry { gc_requested: AtomicBool::new(false), views: Mutex::new(Vec::new()), changed: Condvar::new() }
    }

    /// Adds a new, running view for the calling thread. Waits out a
    /// collection in progress first — a view that appeared running in the
    /// middle of one would be a thread the collector never stopped.
    pub(super) fn register(&self) -> Arc<ViewSlot> {
        let (thread, name) = crate::sync::current_thread();
        let slot = Arc::new(ViewSlot {
            state: AtomicU8::new(RUNNING),
            thread,
            name,
            session_roots: UnsafeCell::new(Vec::new()),
        });
        let mut views = self.views.lock();
        while self.gc_requested.load(Ordering::SeqCst) {
            self.changed.wait(&mut views);
        }
        views.push(Arc::clone(&slot));
        slot
    }

    /// Removes `slot`, and tells a collector waiting for it to stop that it
    /// no longer has to.
    pub(super) fn unregister(&self, slot: &Arc<ViewSlot>) {
        let mut views = self.views.lock();
        views.retain(|v| !Arc::ptr_eq(v, slot));
        self.changed.notify_all();
    }
}

impl Heap {
    // ---- collection -------------------------------------------------------

    /// Push every `Value` nested directly inside a `BoxedObj`'s payload onto
    /// `stack`, for `gc`'s mark phase to trace into. `Float` holds no nested
    /// `Value` (a fully-immediate `f64` payload), so that arm is a no-op —
    /// `Struct`'s `Fields` payload holds a `Vec<Value>` of fields, each of
    /// which must be traced the same as a cons cell's `car`/`cdr` (a struct
    /// field can itself hold a cons, a string, or another boxed struct).
    /// `Struct`'s `Map` payload traces every key and every value: a key is
    /// an ordinary `Value` (a `defstruct` can be one), so "a live map keeps
    /// its own keys and values live" is a claim the mark phase has to make
    /// on its own rather than lean on any interning. Later `BoxedObj` kinds (a closure's
    /// captured environment, ...) will extend this `match` with their own
    /// fan-out.
    fn push_boxed_nested(obj: &BoxedObj, stack: &mut Vec<Value>) {
        match obj {
            // `Bignum`/`Ratio` payloads live in ordinary Rust memory (like a
            // `Str`'s buffer) and hold no nested `Value` — nothing to trace.
            // A `RandomState`'s payload is a bare `u64` seed — same "ordinary
            // Rust memory, nothing nested" case as the numeric boxes.
            BoxedObj::Float32(_) | BoxedObj::Float64(_) | BoxedObj::Narrow(_) | BoxedObj::Bignum(_) | BoxedObj::Ratio(_) | BoxedObj::RandomState(_) => {}
            BoxedObj::Struct { payload: StructPayload::Fields(fields), .. } => {
                for &v in fields {
                    stack.push(v);
                }
            }
            // A live enum value keeps its variant's fields live — the same
            // fan-out as a struct's fields (a field can itself hold a cons,
            // a string, or another boxed value).
            BoxedObj::Enum { fields, .. } => {
                for &v in fields {
                    stack.push(v);
                }
            }
            // A built-in function value holds a symbol and a `PathId`, both
            // permanent, so there is nothing to trace.
            BoxedObj::Builtin { .. } => {}
            // An interpreted closure keeps its own code alive, not just its
            // captures: `params`/`body` are cons cells like any other, and
            // nothing else necessarily refers to a `lambda`'s body once the
            // form that built it is gone.
            BoxedObj::Closure { params, ret, body, env } => {
                stack.push(*params);
                stack.push(*ret);
                stack.push(*body);
                stack.push(*env);
            }
            // A live trait object keeps the concrete value it wraps live.
            // `vtable_id` names a table outside the heap whose entries are
            // method identities and raw function pointers, so there is
            // nothing else here to trace (see [`BoxedObj::Dyn`]).
            BoxedObj::Dyn { value, .. } => stack.push(*value),
            // Both halves of every entry: a key is an ordinary `Value` now
            // (it has to be — a `defstruct` can be one), so it is traced the
            // same way its value is.
            BoxedObj::Struct { payload: StructPayload::Map(map, fns), .. } => {
                stack.extend_from_slice(fns);
                for bucket in map.values() {
                    for &(k, v) in bucket {
                        stack.push(k);
                        stack.push(v);
                    }
                }
            }
            // A live frame keeps its bound values live (keys are plain Rust
            // `String`s — nothing heap-resident to trace).
            BoxedObj::Struct { payload: StructPayload::Frame(map), .. } => {
                for &v in map.values() {
                    stack.push(v);
                }
            }
            // A live scope keeps each of its frames live — including frames
            // shared with other scopes (`scope_clone_frames`): a shared
            // frame survives as long as *any* scope still stacks it.
            BoxedObj::Struct { payload: StructPayload::Frames(frames), .. } => {
                for &f in frames {
                    stack.push(Value::Boxed(f));
                }
            }
            // Rust data only (see `Native`).
            BoxedObj::Struct { payload: StructPayload::Native(_), .. } => {}
            // A live binding cell keeps whatever it currently holds live.
            BoxedObj::Cell(v) => stack.push(*v),
            // A live compiled closure keeps its captured slots live. Every
            // slot is traced uniformly — an unmasked (raw native bits) slot
            // rides in an immediate `Value::Int`, so tracing it is a no-op
            // by construction (see `BoxedObj::CompiledClosure`); `fn_ptr`
            // is machine code, not a heap value, nothing to trace.
            BoxedObj::CompiledClosure { env, .. } => {
                for &v in env {
                    stack.push(v);
                }
            }
        }
    }

    // ---- stopping for another thread's collection --------------------------

    /// Stop here if another thread wants to collect, until it has.
    ///
    /// A safepoint is a place where everything this thread still needs is
    /// reachable from a root — the same condition a `cons` that collects
    /// already requires, which is why `cons` calls this. The other callers
    /// are the places a thread can run for a long time without consing: a
    /// compiled loop's back edge and the scheduler's step boundary.
    ///
    /// It is also where this thread collects itself, when strings and boxes
    /// have been allocated up to [`short_of_memory`](Self::short_of_memory):
    /// those allocations cannot ([`alloc_boxed`](Self::alloc_boxed)), so a
    /// program that makes nothing else depends on reaching a safepoint. Every
    /// loop's back edge and every function's entry is one, where anything is
    /// allocated.
    ///
    /// A few relaxed loads when nobody is collecting.
    pub fn safepoint(&mut self) {
        if self.shared.threads.gc_requested.load(Ordering::Relaxed) {
            self.park();
        }
        if (self.short_of_memory() || self.gc_stress()) && !self.shared.collect_inhibited.load(Ordering::SeqCst) {
            self.gc();
        }
    }

    /// Whether as much has been allocated since the last collection as makes
    /// another one due.
    fn short_of_memory(&self) -> bool {
        self.memory_used() >= self.shared.next_collection.load(Ordering::Relaxed)
    }

    /// The process's memory in use ([`crate::memory`]), less what the slots
    /// this heap has free take of it: a collection empties slots and keeps
    /// them, so what is left for it to do is not in how many there are.
    fn memory_used(&self) -> usize {
        let spare = self.shared.spare_strings.load(Ordering::Relaxed) + self.shared.spare_boxes.load(Ordering::Relaxed);
        crate::memory::in_use().saturating_sub(spare)
    }

    /// Wait at a safepoint until the collection in progress ends.
    fn park(&self) {
        let threads = &self.shared.threads;
        let mut views = threads.views.lock();
        self.slot.state.store(AT_SAFEPOINT, Ordering::SeqCst);
        threads.changed.notify_all();
        while threads.gc_requested.load(Ordering::SeqCst) {
            threads.changed.wait(&mut views);
        }
        self.slot.state.store(RUNNING, Ordering::SeqCst);
    }

    /// Declare that this thread will not touch the heap until
    /// [`leave_native`](Self::leave_native), so another thread's collection
    /// need not wait for it.
    ///
    /// **Every wait that can depend on another typelisp thread's progress
    /// must be inside one** — a `sleep`, a `poll`, a read from a pipe, a
    /// child process, a blocking C call, a lock another thread holds while
    /// allocating. A thread blocked outside one is a thread the collector
    /// waits for, and the other thread may be waiting for the collection.
    ///
    /// What it holds must be rooted, as at any other point that may
    /// collect. Touching the heap before `leave_native` panics in debug
    /// builds. Prefer [`native`](Self::native), which cannot forget to
    /// leave.
    pub fn enter_native(&mut self) {
        let was = self.slot.state.swap(NATIVE, Ordering::SeqCst);
        debug_assert_eq!(was, RUNNING, "enter_native: the view is already native");
        let threads = &self.shared.threads;
        // A collector may already be waiting for this thread to stop; the
        // lock orders this notification after its check, so it cannot be
        // lost between the check and the wait.
        if threads.gc_requested.load(Ordering::SeqCst) {
            let _views = threads.views.lock();
            threads.changed.notify_all();
        }
    }

    /// Come back from [`enter_native`](Self::enter_native) — after waiting
    /// out a collection in progress, if there is one.
    pub fn leave_native(&mut self) {
        let was = self.slot.state.swap(RUNNING, Ordering::SeqCst);
        debug_assert_eq!(was, NATIVE, "leave_native: the view is not native");
        // Running again before looking at the flag: a collector that raises
        // it after this load sees `RUNNING` and waits; one that raised it
        // before is found here, and this thread parks without having
        // touched anything.
        if self.shared.threads.gc_requested.load(Ordering::SeqCst) {
            self.park();
        }
    }

    /// Run `f` — which must not touch this heap — as native: another
    /// thread's collection may run meanwhile. See
    /// [`enter_native`](Self::enter_native).
    ///
    /// Leaves native again even if `f` panics, so that the unwind can go on
    /// to use the heap (a `catch` further up) like any other.
    pub fn native<R>(&mut self, f: impl FnOnce() -> R) -> R {
        struct Leave<'h>(&'h mut Heap);
        impl Drop for Leave<'_> {
            fn drop(&mut self) {
                self.0.leave_native();
            }
        }
        self.enter_native();
        let _leave = Leave(self);
        f()
    }

    /// Whether this view is between `enter_native` and `leave_native`.
    pub fn is_native(&self) -> bool {
        self.slot.state.load(Ordering::Relaxed) == NATIVE
    }

    /// Debug builds: a thread that promised not to touch the heap did.
    /// Checked at the entry points everything else funnels through
    /// (allocation, rooting, collection) rather than at every accessor.
    pub(super) fn debug_assert_not_native(&self, what: &str) {
        debug_assert!(!self.is_native(), "{}: the heap was touched inside a native section", what);
    }

    /// Pushes what a root stack's open frames reference.
    ///
    /// Two filters, both needed. The mask says the slot holds a *tagged* word
    /// at all — an unmasked slot is a raw `i32`/`f64` bit pattern whose low
    /// three bits are not a tag, and reading one as a tag would invent a heap
    /// reference out of arithmetic. The tag test then says whether that tagged
    /// word points at anything: a `Sexpr`-repr local legitimately holds
    /// fixnums and immediates.
    fn trace_frames(frames: &FrameArena, stack: &mut Vec<Value>) {
        for &data in &frames.open {
            // SAFETY: an open frame is live, and every thread is stopped.
            unsafe {
                let raw = FrameRaw::from_data(data);
                for i in 0..raw.len() {
                    if raw.is_rooted(i) {
                        let w = raw.word(i);
                        if crate::tagged::references_heap(w) {
                            stack.push(crate::tagged::decode(w));
                        }
                    }
                }
            }
        }
    }

    /// Debug-only (runs under `gc_stress`): report any root that points at a
    /// cell already on the free list. Such a root makes the mark phase mark a
    /// cell the sweep then refuses to hand back, which is what drives `gc`'s
    /// reclaim count below zero. Panics naming the root set and index — the
    /// fact the underflow itself never reveals.
    fn audit_roots_against_free_list(&self, session_roots: &[Value]) {
        use crate::collections::HashSet;
        let mut freed: HashSet<usize> = HashSet::new();
        let mut p = self.shared.arena.lock().free;
        while !p.is_null() {
            freed.insert(p as usize);
            p = unsafe { (*p).next_free };
        }
        let check = |what: &str, i: usize, v: Value| {
            if let Value::Cons(c) = v {
                if freed.contains(&(c.0 as usize)) {
                    panic!("gc-root-audit: {}[{}] points at freed cell {:p}", what, i, c.0);
                }
            }
        };
        for (s, stack) in self.shared.root_stacks.lock().iter().enumerate() {
            let Some(stack) = stack else { continue };
            let what = if s == self.current_stack { "roots".to_string() } else { format!("roots[task {}]", s) };
            for (i, &v) in stack.roots.iter().enumerate() {
                check(&what, i, v);
            }
            if let Some(v) = stack.in_flight_throw {
                check(&format!("{}.in_flight_throw", what), 0, v);
            }
        }
        for (i, v) in self.shared.permanent_roots.lock().iter().enumerate() {
            if let Some(v) = v {
                check("permanent_roots", i, *v);
            }
        }
        for (i, &v) in session_roots.iter().enumerate() {
            check("session_roots", i, v);
        }
        for w in self.shared.cell_registry.lock().iter() {
            if let Some(id) = w.upgrade() {
                if let Some(BoxedObj::Cell(v)) = &self.shared.boxes.read_recursive().slots[id.0 as usize] {
                    check("cell_registry", id.0 as usize, *v);
                }
            }
        }
    }

    /// Run a mark-sweep collection. Returns the number of cons cells reclaimed.
    ///
    /// **Stop-the-world.** Every other thread with a view onto this heap is
    /// stopped for the duration (`docs/dev/os-threads-design.md` §4):
    ///
    /// 1. Raise `gc_requested`. If another thread already has, this one is
    ///    not the collector: it parks like any thread at a safepoint until
    ///    that collection ends, then starts its own — a caller of `gc()`
    ///    is promised a collection that began after the call.
    /// 2. Wait until every view on another thread is `AT_SAFEPOINT` (it
    ///    polled the flag: [`safepoint`](Self::safepoint), which `cons`
    ///    calls) or `NATIVE` (it promised not to touch the heap:
    ///    [`native`](Self::native)). Views on this thread are not waited
    ///    for — this thread is here, so they are not in the middle of
    ///    anything.
    /// 3. Mark and sweep, holding the registry lock so no view can attach,
    ///    detach or wake meanwhile.
    /// 4. Lower the flag and wake everyone.
    ///
    /// Only then are the heap's own locks taken — never while waiting in
    /// step 2, so a thread that stops with none of them held (every
    /// stopping point is outside them) can never be what the collector is
    /// waiting on.
    ///
    /// Stopping points are only ever at the boundary of a heap operation,
    /// never inside one, so the collector never reads a `Vec`/`HashMap` a
    /// stopped thread was half-way through changing.
    pub fn gc(&mut self) -> usize {
        self.debug_assert_not_native("gc");
        // A handle of its own so the registry lock can be held across the
        // `&mut self` collection below.
        let shared = Arc::clone(&self.shared);
        let threads = &shared.threads;
        while threads.gc_requested.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            self.park();
        }
        let mut views = threads.views.lock();
        let me = crate::sync::current_thread().0;
        loop {
            let running: Vec<&Arc<ViewSlot>> =
                views.iter().filter(|v| v.thread != me && v.state.load(Ordering::SeqCst) == RUNNING).collect();
            if running.is_empty() {
                break;
            }
            let waited = threads.changed.wait_for(&mut views, WATCHDOG);
            if waited.timed_out() && cfg!(debug_assertions) {
                let names: Vec<&str> = views
                    .iter()
                    .filter(|v| v.thread != me && v.state.load(Ordering::SeqCst) == RUNNING)
                    .map(|v| v.name.as_str())
                    .collect();
                crate::host::eprintln(format_args!(
                    "gc: still waiting after {:?} for {} to stop — a thread blocked without `Heap::native`?",
                    WATCHDOG,
                    names.join(", ")
                ));
            }
        }
        let session_roots: Vec<Value> =
            views.iter().flat_map(|v| unsafe { &*v.session_roots.get() }.iter().copied()).collect();
        let reclaimed = self.collect(&session_roots);
        threads.gc_requested.store(false, Ordering::SeqCst);
        threads.changed.notify_all();
        drop(views);
        reclaimed
    }

    /// The mark-sweep itself, with every other thread stopped — see
    /// [`gc`](Self::gc). `session_roots` is every view's, gathered while
    /// the registry was locked.
    fn collect(&mut self, session_roots: &[Value]) -> usize {
        self.shared.gc_count.fetch_add(1, Ordering::Relaxed);
        if false {
            self.audit_roots_against_free_list(session_roots);
        }
        // reset string/box marks. `strings` is locked only for this
        // statement (see the mark loop's `Value::Str` arm below for why it
        // is not held across the rest of `gc`): this collector is still the
        // only thread touching the heap, so nothing is actually contending
        // for it yet, but never holding two of `HeapShared`'s locks at once
        // is the discipline the design doc's lock-ordering rule depends on
        // (`docs/dev/os-threads-design.md` §2) — easier to keep from the
        // start than to retrofit once a second lock actually matters.
        self.shared.strings.write().store.clear_marks();
        self.shared.boxes.write().clear_marks();

        // MARK: iterative DFS from the roots (no native recursion). Two
        // independent root sets feed the same walk — `roots` (the strict
        // LIFO call-stack discipline) and `permanent_roots` (never popped,
        // see `push_permanent_root`) — a value is live if either reaches it.
        // The worklist holds `Value`s directly (not just `*mut Cell`) so a
        // `Value::Boxed` payload's own nested `Value`s can be pushed onto
        // the very same stack once traced — see `push_boxed_nested`.
        let mut stack: Vec<Value> = Vec::new();
        // Every task's roots, not just the running one: a suspended task will
        // resume into the frames these belong to. Each stack's own
        // `in_flight_throw` rides along here too — see `RootStack`'s doc
        // comment for why it is not the separate root set it used to be.
        for roots in self.shared.root_stacks.lock().iter().flatten() {
            for &v in &roots.roots {
                stack.push(v);
            }
            if let Some(v) = roots.in_flight_throw {
                stack.push(v);
            }
            Self::trace_frames(&roots.frames, &mut stack);
        }
        {
            let permanent_roots = self.shared.permanent_roots.lock();
            for i in 0..permanent_roots.len() {
                if let Some(v) = permanent_roots[i] {
                    stack.push(v);
                }
            }
        }
        stack.extend_from_slice(session_roots);
        // Every binding cell still referenced by a live `Arc<BoxId>` handle
        // is a root of its own — see `alloc_cell`. Dead entries (the last
        // handle dropped) are pruned here; their cells become collectible
        // like any other unreachable box.
        let mut cell_registry = self.shared.cell_registry.lock();
        cell_registry.retain(|w| w.upgrade().is_some());
        for w in cell_registry.iter() {
            if let Some(id) = w.upgrade() {
                stack.push(Value::Boxed(*id));
            }
        }
        drop(cell_registry);
        while let Some(v) = stack.pop() {
            match v {
                Value::Cons(c) => unsafe {
                    if (*c.0).mark {
                        continue;
                    }
                    (*c.0).mark = true;
                    stack.push((*c.0).car);
                    stack.push((*c.0).cdr);
                },
                Value::Str(s) => {
                    self.shared.strings.write().store.marks[s.0 as usize] = true;
                }
                Value::Boxed(b) => {
                    let idx = b.0 as usize;
                    let mut boxes = self.shared.boxes.write();
                    if boxes.marks[idx] {
                        continue;
                    }
                    boxes.marks[idx] = true;
                    if let Some(obj) = &boxes.slots[idx] {
                        Self::push_boxed_nested(obj, &mut stack);
                    }
                }
                _ => {}
            }
        }

        // SWEEP cons: rebuild the free list over every chunk. Walking chunks
        // (and cells within a chunk) back to front leaves the rebuilt list
        // ordered lowest-address-first, the same allocation order the
        // single-chunk arena had.
        //
        // Locked for this section only — the mark phase above never touches
        // the arena (it dereferences `*mut Cell`s directly, not through
        // `chunks`), so there is nothing to hold the lock across.
        let mut arena = self.shared.arena.lock();
        let old_free = arena.free_count;
        let mut new_free: *mut Cell = ptr::null_mut();
        let mut new_free_count = 0usize;
        for chunk in arena.chunks.iter().rev() {
            for i in (0..chunk.len).rev() {
                unsafe {
                    let p = chunk.base.add(i);
                    if (*p).mark {
                        (*p).mark = false;
                    } else {
                        // Nothing to do about this cell's source locations:
                        // they are *in* the cell, so they are reclaimed with
                        // it, and `cons` blanks both slots when the cell is
                        // handed out again. That is the whole reason the
                        // location moved inside — the side tables this
                        // replaced needed a removal here (and a bulk clear per
                        // read batch) to keep a recycled address from
                        // mislabelling an unrelated form.
                        (*p).car = Value::Empty;
                        (*p).cdr = Value::Empty;
                        (*p).next_free = new_free;
                        new_free = p;
                        new_free_count += 1;
                    }
                }
            }
        }
        arena.free = new_free;
        arena.free_count = new_free_count;
        drop(arena);

        // SWEEP strings and boxed objects, one lock at a time.
        {
            let mut strings = self.shared.strings.write();
            strings.store.sweep();
            self.shared.spare_strings.store(strings.store.spare_bytes(), Ordering::Relaxed);
        }
        {
            let mut boxes = self.shared.boxes.write();
            boxes.sweep();
            self.shared.spare_boxes.store(boxes.spare_bytes(), Ordering::Relaxed);
        }
        drop(stack);
        self.shared.next_collection.store(crate::memory::next_collection(self.memory_used()), Ordering::Relaxed);

        new_free_count - old_free
    }
}
