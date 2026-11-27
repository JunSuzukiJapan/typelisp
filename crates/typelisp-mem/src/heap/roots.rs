//! GC roots: the root stacks, session roots and the in-flight throw.

use crate::prelude::*;
use crate::value::Value;
use super::{Heap, RootStack, RootStackId};

impl Heap {
    // ---- roots ------------------------------------------------------------

    /// The running task's roots.
    ///
    /// Reads through the cached `current` pointer rather than indexing
    /// `root_stacks[current_stack]` — see `current`'s doc comment. Safe
    /// because `current` is kept in sync with `current_stack` by every
    /// method that changes either, and always points at a live `RootStack`
    /// owned by `root_stacks` (never dangling: `drop_root_stack` refuses to
    /// drop the running stack).
    fn roots(&self) -> &crate::chunked::ChunkStack<Value> {
        &unsafe { &*self.current }.roots
    }

    pub(super) fn roots_mut(&mut self) -> &mut crate::chunked::ChunkStack<Value> {
        &mut unsafe { &mut *self.current }.roots
    }

    /// Register `v` as a GC root.
    pub fn push_root(&mut self, v: Value) {
        self.debug_assert_not_native("push_root");
        if self.gc_stress() {
            self.assert_not_freed("push_root", v);
        }
        self.roots_mut().push(v);
    }

    /// Debug-only (`gc_stress`): trap the *moment* an already-reclaimed cell is
    /// installed as a root, rather than at the next collection. This is the
    /// difference between a backtrace through the culprit and one through some
    /// unrelated allocation much later — it is what located the missing root in
    /// `compile::ast_bridge::tagged_sym_list`, which had gone unexplained long
    /// enough to have two tests `#[ignore]`d for it.
    ///
    /// O(1) rather than a free-list walk, which is what makes it affordable on
    /// every push: `cons` clears `next_free` when it hands a cell out and the
    /// sweep sets it when it takes one back, so a non-null `next_free` means
    /// the cell is on the free list. The one free cell this cannot see is the
    /// list's tail, whose link is null.
    ///
    /// The O(1) form is not a micro-optimization. Walking the free list here
    /// (or auditing every root at each collection, which was the first thing
    /// tried) is O(free) per allocation under `gc_stress` — measured at a 10x
    /// slowdown on `tests/scope_gc_stress_test.rs`, which would have made the
    /// check too expensive to leave on. This one costs nothing measurable.
    fn assert_not_freed(&self, what: &str, v: Value) {
        if let Value::Cons(c) = v {
            if !unsafe { (*c.0).next_free }.is_null() {
                panic!("gc-root-audit: {} given freed cell {:p}", what, c.0);
            }
        }
    }

    /// Remove the most recently pushed root.
    pub fn pop_root(&mut self) -> Option<Value> {
        self.roots_mut().pop()
    }

    /// Number of registered roots.
    pub fn root_count(&self) -> usize {
        self.roots().len()
    }

    /// Discards every root pushed since the stack was `len` roots deep, in
    /// one shot — the "unwind to a known depth" a `break`/`return` that
    /// jumps out of an arbitrary number of nested lexical scopes needs,
    /// where the strict one-at-a-time LIFO discipline
    /// [`push_root`](Self::push_root)/[`pop_root`](Self::pop_root) assume
    /// (every scope pops exactly what it pushed, in order, as control flow
    /// passes back through it) breaks down: a `break`/`return` skips that
    /// unwind entirely, jumping straight past however many enclosing
    /// `let`/`match` scopes happen to be open. A no-op if `len >= root_count()`
    /// already (a `break`/`return` with nothing open above the loop it's
    /// jumping out of) — same "truncating to at-or-past the current length
    /// does nothing" behavior as [`Vec::truncate`].
    ///
    /// Closes every compiled frame opened at `len` or deeper with them (see
    /// [`open_frame`](Self::open_frame)).
    pub fn truncate_roots(&mut self, len: usize) {
        let stack = unsafe { &mut *self.current };
        stack.roots.truncate(len);
        if stack.frames.open.last().is_some() {
            stack.frames.close_from(len);
        }
    }

    /// Overwrites the root at absolute stack position `idx` in place — unlike
    /// [`push_root`](Self::push_root)/[`pop_root`](Self::pop_root) (strict
    /// LIFO), this lets a caller that already knows a specific root's stack
    /// position (recorded once when that root was first pushed, e.g. a
    /// `let`/parameter binding's own slot) replace *just that one* root's
    /// value — e.g. when a compiled `setf` reassigns a `Sexpr`-typed local:
    /// the binding keeps the same stack slot for its whole lifetime, only the
    /// pointer stored there changes. Panics on an out-of-bounds `idx`, like
    /// indexing a `Vec` directly — `idx` is always a value `root_count()`
    /// itself returned earlier in the same dynamic scope, never user input.
    pub fn set_root(&mut self, idx: usize, v: Value) {
        if self.gc_stress() {
            self.assert_not_freed("set_root", v);
        }
        self.roots_mut()[idx] = v;
    }

    /// Reads the root at absolute stack position `idx` back out — the getter
    /// [`set_root`](Self::set_root) is the setter for, for a caller holding a
    /// slot it filled earlier (a channel's ring buffer is the one that needs
    /// it). Panics on an out-of-bounds `idx`, as `set_root` does.
    pub fn root(&self, idx: usize) -> Value {
        self.roots()[idx]
    }

    // ---- root stacks, one per task ----------------------------------------

    /// Adds an empty root stack for a new task, and returns its id. It is *not*
    /// made current — [`switch_to_root_stack`](Self::switch_to_root_stack)
    /// does that.
    ///
    /// A task that is waiting keeps the roots its continuation frames hold, so
    /// they have to outlive collections triggered by whatever runs meanwhile.
    /// Splitting the stack rather than sharing one is what makes that possible:
    /// a single stack could only be truncated in the order it was pushed, and
    /// tasks do not finish in that order.
    pub fn new_root_stack(&mut self) -> RootStackId {
        let mut root_stacks = self.shared.root_stacks.lock();
        // Reuse a slot a finished task freed, so spawning many short-lived
        // tasks does not grow this vector without bound.
        if let Some(i) = root_stacks.iter().position(|s| s.is_none()) {
            root_stacks[i] = Some(Box::new(RootStack::default()));
            RootStackId(i)
        } else {
            root_stacks.push(Some(Box::new(RootStack::default())));
            RootStackId(root_stacks.len() - 1)
        }
    }

    /// The stack that is running.
    pub fn current_root_stack(&self) -> RootStackId {
        RootStackId(self.current_stack)
    }

    /// Makes `id` the running stack. The outgoing one stays where it is —
    /// suspended, and still walked by the collector.
    ///
    /// **Only safe where no Rust frame holds a root index.** A `RootScope`, or
    /// anything holding a `root_count()` it means to truncate back to, would be
    /// pointing into a different stack afterwards. In the evaluator that means
    /// a task-step boundary and nothing finer.
    pub fn switch_to_root_stack(&mut self, id: RootStackId) {
        let mut root_stacks = self.shared.root_stacks.lock();
        let Some(stack) = root_stacks.get_mut(id.0).and_then(|s| s.as_mut()) else {
            panic!("switch_to_root_stack: {:?} is not a live stack", id);
        };
        self.current_stack = id.0;
        // Safe to keep past the lock guard's drop below: the pointer is at
        // the `RootStack`'s own heap allocation (the `Box`'s target), not at
        // anything inside the locked `Vec` — see `current`'s doc comment.
        self.current = &mut **stack;
    }

    /// Pushes `v` onto the root stack `id` without making it the running one.
    ///
    /// For a holder whose values must outlive every [`RootScope`] opened on the
    /// running stack meanwhile: the analyzed tree of a top-level form keeps
    /// heap values until the form is lowered at its end, while the scopes
    /// opened to check its parts close one by one.
    pub fn push_root_on(&mut self, id: RootStackId, v: Value) {
        if self.gc_stress() {
            self.assert_not_freed("push_root_on", v);
        }
        let mut root_stacks = self.shared.root_stacks.lock();
        let Some(stack) = root_stacks.get_mut(id.0).and_then(|s| s.as_mut()) else {
            panic!("push_root_on: {:?} is not a live stack", id);
        };
        stack.roots.push(v);
    }

    /// Empties the root stack `id`, which must not be the running one.
    pub fn clear_root_stack(&mut self, id: RootStackId) {
        assert_ne!(id.0, self.current_stack, "clear_root_stack: that stack is running");
        let mut root_stacks = self.shared.root_stacks.lock();
        let Some(stack) = root_stacks.get_mut(id.0).and_then(|s| s.as_mut()) else {
            panic!("clear_root_stack: {:?} is not a live stack", id);
        };
        stack.roots.clear();
    }

    /// Frees a finished task's stack.
    ///
    /// It must not be the running one. A non-empty stack here means the task
    /// left roots behind, which is a bug in whoever ran it — the values would
    /// stop being reachable at exactly this point.
    pub fn drop_root_stack(&mut self, id: RootStackId) {
        assert_ne!(id.0, self.current_stack, "drop_root_stack: that stack is running");
        let stack = self.shared.root_stacks.lock()[id.0].take();
        debug_assert!(
            stack.map(|s| s.roots.is_empty() && s.frames.open.is_empty()).unwrap_or(true),
            "drop_root_stack: {:?} still holds roots",
            id
        );
    }

    /// Registers `v` as a root with no matching pop, ever — for a value that
    /// lives as long as the process: a global variable, the program's own
    /// forms. Kept apart from `roots`, which has to nest.
    ///
    /// Not for a value a program makes: it would never be collected.
    pub fn push_permanent_root(&mut self, v: Value) {
        self.shared.permanent_roots.lock().push(Some(v));
    }

    /// Number of registered permanent roots — see
    /// [`push_permanent_root`](Self::push_permanent_root).
    pub fn permanent_root_count(&self) -> usize {
        self.shared.permanent_roots.lock().len()
    }

    /// Read permanent root `idx`'s current value (a global variable's
    /// storage — `typelisp-rt`'s `rt_global_get`) — the mark phase already
    /// re-reads `permanent_roots[i]` by index every cycle
    /// ([`Self::gc`](Heap::gc)'s root walk), so an entry can be overwritten
    /// in place ([`Self::set_permanent_root`]) with no change to that logic.
    pub fn permanent_root(&self, idx: usize) -> Value {
        self.shared.permanent_roots.lock()[idx].expect("permanent_root: slot was never populated")
    }

    /// Overwrite permanent root `idx`'s value in place — `rt_global_set`'s
    /// storage half. See [`Self::permanent_root`].
    pub fn set_permanent_root(&mut self, idx: usize, v: Value) {
        self.shared.permanent_roots.lock()[idx] = Some(v);
    }

    // ---- session roots ----------------------------------------------------

    /// Register `v` as a root for the rest of the current *session* — a
    /// bracketed span of work whose intermediate values outlive individual
    /// activations but must not outlive the span. Released in bulk by
    /// [`truncate_session_roots`](Self::truncate_session_roots).
    ///
    /// This is the third root set because neither of the other two can do the
    /// job for values compiled code holds without rooting them itself:
    ///
    /// * `roots` is a strict LIFO stack that compiled code actively unwinds —
    ///   `rt_truncate_sexpr_roots` cuts it back to a recorded base on every
    ///   `break`/`return`. A root pushed from underneath, by a runtime shim
    ///   the compiled frame does not know about, would be silently discarded
    ///   by the next such unwind (or would desync somebody else's pairing).
    /// * `permanent_roots` never releases anything, and compiled-global
    ///   promotion appends to it at interleaved times, so its indices must
    ///   stay stable — truncating it is not available even in principle.
    ///
    /// The motivating case is the self-hosted compiler's `Scope<V>`: the
    /// committed island bitcode binds scope values at the untraced integer
    /// kind (they used to be registry handles), so a heap scope living only in
    /// a compiled local would be swept by any collection during compilation.
    /// Rooting it at birth, for the compile session, restores exactly the
    /// lifetime the handle registry used to give it.
    pub fn push_session_root(&mut self, v: Value) {
        self.session_roots_mut().push(v);
    }

    /// The current session-root count — pass to
    /// [`truncate_session_roots`](Self::truncate_session_roots) to release
    /// everything registered after this point.
    pub fn session_root_count(&self) -> usize {
        self.session_roots().len()
    }

    /// Release every session root registered since `len` was observed. A no-op
    /// if `len` is already at or past the current count, like [`Vec::truncate`].
    ///
    /// Brackets nest: an inner session releases only its own registrations, so
    /// a nested compile leaves the outer one's values rooted.
    pub fn truncate_session_roots(&mut self, len: usize) {
        self.session_roots_mut().truncate(len);
    }

    /// This view's session roots. They live in the view's registry slot so
    /// that a collector on another thread can mark them — see `ViewSlot`.
    fn session_roots(&self) -> &Vec<Value> {
        // Only the owning view writes them, and a collector reads them only
        // while this view is stopped — see `ViewSlot`'s `Sync` impl.
        unsafe { &*self.slot.session_roots.get() }
    }

    fn session_roots_mut(&mut self) -> &mut Vec<Value> {
        unsafe { &mut *self.slot.session_roots.get() }
    }

    // ---- the in-flight throw ----------------------------------------------

    /// The value a `(throw ...)` currently travelling up **the running
    /// task's** frames carries, or `None` when no throw is in flight in it.
    ///
    /// A single slot per task rather than a stack, and none of the other
    /// three root sets, because the lifetime it has to express is neither an
    /// activation's nor a session's: the value is live from the `throw` that
    /// raised it until the `catch` that consumes it, while the stack in
    /// between is being *discarded*. `roots` is exactly what a non-local exit
    /// cuts back (`truncate_roots`, from both the compiled and the
    /// interpreted side), so a value parked there would be released by the
    /// very unwind that is carrying it; `permanent_roots` never releases;
    /// `session_roots` is not bracketed by a throw.
    ///
    /// One slot *per task* rather than one on `Heap` — see [`RootStack`]'s
    /// `in_flight_throw` field for why a shared slot is not safe once a
    /// cleanup can suspend. One slot per task suffices because at most one
    /// throw is ever in flight *in that task*: an exit raised from an
    /// `unwind-protect` cleanup while another is travelling *replaces* it
    /// (CLHS — the cleanup's own exit wins), which is what overwriting this
    /// slot does.
    pub fn in_flight_throw(&self) -> Option<Value> {
        unsafe { &*self.current }.in_flight_throw
    }

    /// Register `f` as the reader macro for `ch` — the function the reader
    /// calls when it meets that character where a datum would start.
    ///
    /// `f` becomes a **permanent root**: it has to survive every collection
    /// between now and the last read that might use it, and nothing else
    /// refers to it. Re-registering the same character replaces the entry and
    /// leaves the old function rooted, which is the same small, bounded leak
    /// a redefined global has and for the same reason (`permanent_roots` is
    /// not LIFO, so an entry cannot be withdrawn).
    pub fn set_macro_character(&mut self, ch: char, f: Value) {
        self.push_permanent_root(f);
        self.shared.interning.write().macro_chars.insert(ch, f);
    }

    /// The reader macro registered for `ch`, if any.
    pub fn macro_character(&self, ch: char) -> Option<Value> {
        self.shared.interning.read_recursive().macro_chars.get(&ch).copied()
    }

    /// [`Self::set_macro_character`] for a *dispatching* character: `f`
    /// handles `disp` followed by `sub` (CL's `#`-style two-character
    /// syntax).
    ///
    /// Registering makes `disp` dispatching; there is no separate
    /// `make-dispatch-macro-character` step, because after this there would
    /// be nothing left for one to do.
    pub fn set_dispatch_macro_character(&mut self, disp: char, sub: char, f: Value) {
        self.push_permanent_root(f);
        self.shared.interning.write().dispatch_chars.insert((disp, sub), f);
    }

    /// The function registered for the two-character sequence, if any.
    pub fn dispatch_macro_character(&self, disp: char, sub: char) -> Option<Value> {
        self.shared.interning.read_recursive().dispatch_chars.get(&(disp, sub)).copied()
    }

    /// Whether `ch` begins a two-character dispatch sequence — i.e. whether
    /// anything at all has been registered under it.
    ///
    /// `#` is *not* reported here even when it has entries: the reader hands
    /// `#` to its own built-in dispatch, which consults
    /// [`Self::dispatch_macro_character`] before any of `#b`/`#x`/`#.`/…
    /// and so already covers the user's registrations.
    pub fn is_dispatch_char(&self, ch: char) -> bool {
        ch != '#' && self.shared.interning.read_recursive().dispatch_chars.keys().any(|(d, _)| *d == ch)
    }

    /// Park (or, with `None`, release) **the running task's** in-flight
    /// throw value — see [`in_flight_throw`](Self::in_flight_throw).
    pub fn set_in_flight_throw(&mut self, v: Option<Value>) {
        unsafe { &mut *self.current }.in_flight_throw = v;
    }
}
