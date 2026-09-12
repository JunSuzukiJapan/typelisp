//! A manually-managed value heap with a precise mark-sweep collector.
//!
//! # Design
//!
//! * **Chunked cons arena.** `with_capacity(n)` allocates `n` cons cells as one
//!   chunk. A chunk is never reallocated or moved, so the raw `*mut Cell` inside
//!   every [`Value::Cons`] stays valid for the heap's lifetime; growth only ever
//!   *appends* a new chunk, leaving every existing cell exactly where it is.
//!   Each chunk is an owned allocation freed in `Drop` — the backing memory
//!   cannot leak.
//! * **Free list.** Unused cells are chained through `Cell::next_free`.
//!   Allocation pops the head; if the list is empty it runs a GC and retries.
//! * **Growth is opt-in and bounded.** By default the arena never grows: a GC
//!   that frees nothing makes allocation return [`Error::HeapExhausted`], which
//!   is what turns a runaway leak into a loud error rather than an out-of-memory
//!   kill. A driver that genuinely cannot size its heap up front — the
//!   interpreter, once function bodies are themselves cons structure — calls
//!   [`Heap::set_growth_limit`] to permit appending chunks up to a ceiling;
//!   exhaustion is then reported at that ceiling instead.
//! * **Symbols** are interned (name -> [`SymRef`]) and never collected — they are
//!   few and live for the heap's lifetime, like CL symbols in a package.
//! * **Strings** live in a slot store and ARE collected: the mark phase marks
//!   every reachable [`StrId`], the sweep frees unmarked slots (recycling
//!   indices). So unreachable strings do not leak.
//! * **Mark-sweep.** `gc()` marks everything reachable from the root set
//!   (iteratively — no native recursion), then rebuilds the cons free list and
//!   sweeps strings. Cycles are reclaimed (unlike reference counting).
//! * **Four root sets.** `roots` is a strict LIFO stack (push on scope entry,
//!   pop on scope exit) — **one per task**, since a suspended task keeps the
//!   roots it will resume into; only the running one is pushed and popped, and
//!   the discipline holds within each stack separately (see
//!   [`Heap::new_root_stack`]). `permanent_roots` holds values whose owner outlives
//!   any single activation (e.g. a field inside a heap-external, never-freed
//!   box) — appended to, never popped. `session_roots` holds values that must
//!   survive a bracketed span of work but not outlive it, released in bulk at
//!   the bracket's end (see [`Heap::push_session_root`]). `in_flight_throw` is
//!   the one value a `throw` is carrying while the stack that held it is being
//!   discarded (see [`Heap::set_in_flight_throw`]). `gc()` marks from all four.
//!
//! All `unsafe` is confined here; the public API is safe.

use std::collections::HashMap;
use std::ptr;

use crate::Error;
use super::symbols::{self, SymRef};
use super::value::{BoxId, BoxedObj, Cell, ConsRef, FloatBox, LocId, NarrowInt, PathId, StrId, StructPayload, TypeKeyId, Value};

/// How far past its initial capacity a heap may grow by default — see
/// [`Heap::with_capacity`]. Large enough that program text plus a working set
/// never hits it, small enough that a runaway leak still stops loudly instead
/// of being OOM-killed.
pub const GROWTH_FACTOR: usize = 256;

/// How much of the arena must be free after a collection for the arena to be
/// left at the size it is: below `1/HEADROOM_DIVISOR` free, the next `cons`
/// grows instead of collecting again — see [`Heap::cons`].
const HEADROOM_DIVISOR: usize = 4;

/// One owned run of cons cells. Chunks are only ever appended (see
/// [`Heap::set_growth_limit`]) and never moved or freed individually, which is
/// what lets a raw `*mut Cell` stay valid for the heap's whole lifetime.
struct Chunk {
    base: *mut Cell,
    len: usize,
}

/// Identifies one task's root stack — see [`Heap::new_root_stack`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RootStackId(usize);

pub struct Heap {
    // The cons arena, as one or more chunks. Always at least one entry (a
    // zero-capacity heap holds a single zero-length chunk). Never reordered:
    // `gc`'s sweep walks it back to front so the rebuilt free list keeps
    // handing out low addresses first, exactly as the single-chunk version did.
    chunks: Vec<Chunk>,
    cap: usize, // sum of every chunk's `len`
    // Ceiling on `cap` for growth, in cells. `0` (the default) means the arena
    // is fixed at its initial capacity and exhaustion is an error — see the
    // module doc comment.
    growth_limit: usize,
    free: *mut Cell, // head of the free list (null when empty)
    free_count: usize,
    // When set, every `cons` collects first — see `set_gc_stress`.
    gc_stress: bool,
    // How many collections have run, for tests that assert an allocation
    // pattern does not thrash — see `gc_count`.
    gc_count: u64,
    // Every task's roots. `current_stack` names the one that is running; the
    // rest belong to suspended tasks and are walked by the collector but by
    // nothing else — a task that is not running pushes and pops nothing. See
    // `new_root_stack`.
    root_stacks: Vec<Option<Vec<Value>>>,
    current_stack: usize,
    permanent_roots: Vec<Value>,
    // Roots for the duration of a bracketed session — see `push_session_root`.
    session_roots: Vec<Value>,
    // The value of a `throw` currently travelling up the stack — see
    // `set_in_flight_throw`.
    in_flight_throw: Option<Value>,
    // Reader macros: the function each macro character dispatches to, and for
    // a dispatching character (`#`-like) the function each sub-character
    // dispatches to. See `set_macro_character`.
    //
    // Here rather than in a thread-local of the reader's, for two reasons
    // that point the same way. A stored function is a heap value, so it only
    // means anything alongside the heap it came from — a table outliving the
    // heap would hand the next session cells belonging to nobody. And the
    // values need rooting: they are pushed as *permanent* roots when
    // registered, which is a thing only a `Heap` can do.
    macro_chars: HashMap<char, Value>,
    dispatch_chars: HashMap<(char, char), Value>,

    // Symbols are *not* here: they live in one process-global, permanent
    // table (`super::symbols`), so a symbol means the same thing in every
    // heap and compiled code can reach one without an active `Heap` at all.

    // interned type identities (permanent) — see `TypeKeyId`. The first
    // `BUILTIN_TYPE_KEYS.len()` entries are pre-interned by `Heap::with_capacity`, so their
    // ids are the `TypeKeyId` associated constants.
    type_keys: Vec<String>,
    type_key_ids: HashMap<String, u32>,


    // interned `::` paths (permanent; reference only permanent symbols)
    paths: Vec<Vec<SymRef>>,
    path_ids: HashMap<Vec<SymRef>, u32>,

    // GC-managed string store
    str_slots: Vec<Option<String>>,
    str_free: Vec<u32>,
    str_marks: Vec<bool>,

    // content -> StrId for strings interned via `intern_string` (HashTable
    // string keys) — a separate index from the ordinary string store above,
    // which deliberately does *not* dedupe (see `alloc_string`'s doc
    // comment on `Sexpr::Str`'s `eq`-by-identity semantics).
    str_intern: HashMap<String, StrId>,

    // GC-managed general boxed-object store (see `BoxedObj`) — same
    // growable-slot-store shape as the string store above, generalized to
    // hold a payload that may itself reference nested `Value`s (so the mark
    // phase must trace into it, not just flag the slot).
    box_slots: Vec<Option<BoxedObj>>,
    box_free: Vec<u32>,
    box_marks: Vec<bool>,

    // Liveness registry for `BoxedObj::Cell`s (see `alloc_cell`): a cell is
    // an *implicit* GC root for exactly as long as some binding holds the
    // `Rc<BoxId>` handed out at allocation. Registered here (weakly) rather
    // than through the caller-managed `roots`/`permanent_roots` stacks
    // because a binding's lifetime follows Rust scopes, not this heap's
    // strict LIFO root discipline — and, crucially, `gc()` reads this
    // registry *itself*, so a collection triggered from anywhere (including
    // compiled code, which knows nothing about the interpreter's
    // root-resyncing) can never sweep a live binding cell.
    cell_registry: Vec<std::rc::Weak<BoxId>>,

    // Every distinct source span any live cell refers to, indexed by
    // `LocId` — see `Cell`'s doc comment for why a cell holds an index rather
    // than the `Loc` itself, and why the location lives in the cell at all.
    //
    // Index 0 is a reserved dummy so `LocId::NONE` can be the all-zero
    // pattern a blank cell starts as; the real entries begin at 1.
    //
    // Never shrinks. Interning bounds it by the number of *distinct* spans
    // ever read, which is on the order of the source's own size, and a span
    // is 32 bytes. Compaction is possible whenever it is wanted — `gc`'s
    // sweep already walks every cell, so it could mark the ids in use — and
    // is deliberately left until a measurement asks for it.
    locs: Vec<crate::errors::Loc>,
    loc_ids: HashMap<crate::errors::Loc, LocId>,
}

impl Heap {
    /// Allocate `len` blank cells as one owned run and thread them onto a free
    /// list ending in `tail`. Returns the chunk and the new free-list head
    /// (`tail` itself when `len` is 0).
    fn alloc_chunk(len: usize, tail: *mut Cell) -> (Chunk, *mut Cell) {
        let mut v: Vec<Cell> = Vec::with_capacity(len);
        for _ in 0..len {
            v.push(Cell::blank());
        }
        let boxed: Box<[Cell]> = v.into_boxed_slice();
        let base: *mut Cell = Box::into_raw(boxed) as *mut Cell;

        unsafe {
            for i in 0..len {
                let p = base.add(i);
                (*p).next_free = if i + 1 < len { base.add(i + 1) } else { tail };
            }
        }

        (Chunk { base, len }, if len > 0 { base } else { tail })
    }

    /// Create a heap with `capacity` cons cells pre-allocated, growing by
    /// appended chunks up to [`GROWTH_FACTOR`] times that — call
    /// `set_growth_limit(0)` for a strictly fixed arena.
    ///
    /// Growth is the default because the heap now holds the *program*, not just
    /// its data: since the checker lowers code into cons cells, an initial
    /// capacity cannot be chosen up front to fit a program whose size is only
    /// known after reading it. Measured, the prelude alone has a live set of
    /// ~53k cells and the prelude plus the compiler island ~118k — so the
    /// historical `1 << 16` default did not even hold the two of them, and no
    /// amount of collection would have helped: that is live data, not garbage.
    ///
    /// The initial capacity therefore means "allocate this much up front" and
    /// the ceiling means "past here, call it a leak" — which is what the two
    /// numbers should always have meant. Proportional rather than absolute so a
    /// deliberately tiny heap stays deliberately tiny.
    pub fn with_capacity(capacity: usize) -> Heap {
        let (chunk, free) = Self::alloc_chunk(capacity, ptr::null_mut());

        let mut heap = Heap {
            chunks: vec![chunk],
            cap: capacity,
            growth_limit: capacity.saturating_mul(GROWTH_FACTOR),
            free,
            free_count: capacity,
            gc_stress: false,
            gc_count: 0,
            root_stacks: vec![Some(Vec::new())],
            current_stack: 0,
            permanent_roots: Vec::new(),
            session_roots: Vec::new(),
            in_flight_throw: None,
            macro_chars: HashMap::new(),
            dispatch_chars: HashMap::new(),
            type_keys: Vec::new(),
            type_key_ids: HashMap::new(),
            paths: Vec::new(),
            path_ids: HashMap::new(),
            str_slots: Vec::new(),
            str_free: Vec::new(),
            str_marks: Vec::new(),
            str_intern: HashMap::new(),
            box_slots: Vec::new(),
            box_free: Vec::new(),
            box_marks: Vec::new(),
            cell_registry: Vec::new(),
            locs: Vec::new(),
            loc_ids: HashMap::new(),
        };
        for key in crate::value::BUILTIN_TYPE_KEYS {
            heap.intern_type_key(key);
        }
        heap
    }

    // -- Source locations (see `Cell`'s doc comment) -------------------------

    /// Intern `loc`, returning the id a cell stores.
    fn intern_loc(&mut self, loc: crate::errors::Loc) -> LocId {
        if let Some(id) = self.loc_ids.get(&loc) {
            return *id;
        }
        // Ids are 1-based so that `LocId::NONE` is the all-zero pattern a blank
        // cell already has, with no dummy entry to keep in step.
        self.locs.push(loc.clone());
        let id = LocId(self.locs.len() as u32);
        self.loc_ids.insert(loc, id);
        id
    }

    /// The `Loc` behind an id, or `None` for [`LocId::NONE`].
    fn loc_of(&self, id: LocId) -> Option<crate::errors::Loc> {
        if id.is_none() {
            return None;
        }
        self.locs.get(id.0 as usize - 1).cloned()
    }

    /// Record the span of the form `cr` heads — for the reader, a list form's
    /// `(` through `)`; for the checker, the source the node it just built was
    /// lowered from. Read back by [`Heap::cons_loc`].
    ///
    /// One setter for both because there is one fact: the span of the form this
    /// cell is the head of. The two used to be separate tables with separate
    /// lifetimes (`cons_locs` bulk-cleared per read batch, `code_locs` swept),
    /// which was a property of living outside the cell, not of the fact itself.
    pub fn set_cons_loc(&mut self, cr: ConsRef, loc: crate::errors::Loc) {
        let id = self.intern_loc(loc);
        unsafe {
            (*cr.0).self_loc = id;
        }
    }

    /// The span of the form `v` heads, if one was recorded. Non-cons values
    /// (atoms, symbols, ...) never carry their own location — see
    /// [`Heap::list_to_vec_locs`] for how an atom *element* gets one.
    pub fn cons_loc(&self, v: Value) -> Option<crate::errors::Loc> {
        match v {
            Value::Cons(cr) => self.loc_of(unsafe { (*cr.0).self_loc }),
            _ => None,
        }
    }

    /// Record where the element in `cr`'s `car` began. The reader calls this
    /// for every element as it builds a list; the checker reads it back via
    /// [`Heap::list_to_vec_locs`].
    ///
    /// This is the only way an atom gets a position: an interned symbol or an
    /// immediate scalar has no per-occurrence identity of its own, so the span
    /// hangs on the cell that holds it.
    pub fn set_elem_loc(&mut self, cr: ConsRef, loc: crate::errors::Loc) {
        let id = self.intern_loc(loc);
        unsafe {
            (*cr.0).car_loc = id;
        }
    }

    /// The span of the element in `cr`'s `car`, if one was recorded — the
    /// single-cell counterpart of [`Heap::list_to_vec_locs`], for a caller that
    /// has the cell rather than the list (`crate::fasl`'s serializer).
    pub fn elem_loc(&self, cr: ConsRef) -> Option<crate::errors::Loc> {
        self.loc_of(unsafe { (*cr.0).car_loc })
    }

    /// How many distinct source spans are interned — for tests about the
    /// location table's growth.
    pub fn loc_count(&self) -> usize {
        self.locs.len()
    }

    // ---- statistics -------------------------------------------------------

    /// Total number of cons cells in the arena.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Cons cells currently on the free list (available to allocate).
    pub fn free_count(&self) -> usize {
        self.free_count
    }

    /// Cons cells currently in use (`capacity - free_count`).
    pub fn live_count(&self) -> usize {
        self.cap - self.free_count
    }

    /// How many collections have run over this heap's lifetime. A ratio, not
    /// an absolute: what it is for is asserting that a workload whose live set
    /// sits close to capacity collects a handful of times rather than once per
    /// allocation (see [`Heap::cons`]).
    pub fn gc_count(&self) -> u64 {
        self.gc_count
    }

    /// Distinct interned symbols.
    pub fn symbol_count(&self) -> usize {
        symbols::symbol_count()
    }

    /// Strings currently allocated (occupied slots).
    pub fn string_count(&self) -> usize {
        self.str_slots.iter().filter(|s| s.is_some()).count()
    }

    /// Boxed objects currently allocated (occupied slots) — see [`BoxedObj`].
    pub fn box_count(&self) -> usize {
        self.box_slots.iter().filter(|b| b.is_some()).count()
    }

    // ---- roots ------------------------------------------------------------

    /// The running task's roots.
    ///
    /// `current_stack` always names an occupied slot: `switch_to_root_stack`
    /// refuses an empty one, and `drop_root_stack` refuses the running one.
    fn roots(&self) -> &[Value] {
        self.root_stacks[self.current_stack]
            .as_ref()
            .expect("the running root stack is always present")
    }

    fn roots_mut(&mut self) -> &mut Vec<Value> {
        self.root_stacks[self.current_stack]
            .as_mut()
            .expect("the running root stack is always present")
    }

    /// Register `v` as a GC root.
    pub fn push_root(&mut self, v: Value) {
        if self.gc_stress {
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
    /// does nothing" behavior as [`Vec::truncate`], which this delegates to
    /// directly.
    pub fn truncate_roots(&mut self, len: usize) {
        self.roots_mut().truncate(len);
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
        if self.gc_stress {
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
        // Reuse a slot a finished task freed, so spawning many short-lived
        // tasks does not grow this vector without bound.
        if let Some(i) = self.root_stacks.iter().position(|s| s.is_none()) {
            self.root_stacks[i] = Some(Vec::new());
            RootStackId(i)
        } else {
            self.root_stacks.push(Some(Vec::new()));
            RootStackId(self.root_stacks.len() - 1)
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
        assert!(
            self.root_stacks.get(id.0).is_some_and(|s| s.is_some()),
            "switch_to_root_stack: {:?} is not a live stack",
            id
        );
        self.current_stack = id.0;
    }

    /// Frees a finished task's stack.
    ///
    /// It must not be the running one. A non-empty stack here means the task
    /// left roots behind, which is a bug in whoever ran it — the values would
    /// stop being reachable at exactly this point.
    pub fn drop_root_stack(&mut self, id: RootStackId) {
        assert_ne!(id.0, self.current_stack, "drop_root_stack: that stack is running");
        let stack = self.root_stacks[id.0].take();
        debug_assert!(
            stack.map(|s| s.is_empty()).unwrap_or(true),
            "drop_root_stack: {:?} still holds roots",
            id
        );
    }

    /// Registers `v` as a root with no matching pop, ever — for a value
    /// whose owner isn't a call-stack activation but a heap-external,
    /// never-freed allocation (e.g. compiled code's general-ADT box fields:
    /// a `defstruct`/`Option`/`Result` value stored in a `malloc`'d box
    /// outlives the activation that built it, so it can't use
    /// [`push_root`](Self::push_root)'s push-on-entry/pop-on-exit discipline
    /// without desyncing every *other* caller's strict LIFO pairing on the
    /// same `roots` stack). Kept in a wholly separate `Vec` rather than
    /// appended to `roots` for exactly that reason — `gc()` walks both, but
    /// only `roots` has to nest correctly.
    ///
    /// This leaks one root slot per call, forever (no removal API): the
    /// matching trade-off compiled code already makes for the box itself
    /// (never `build-free`'d) — see `compiler.rs`'s `compile-construct-box`
    /// doc comment.
    pub fn push_permanent_root(&mut self, v: Value) {
        self.permanent_roots.push(v);
    }

    /// Number of registered permanent roots — see
    /// [`push_permanent_root`](Self::push_permanent_root).
    pub fn permanent_root_count(&self) -> usize {
        self.permanent_roots.len()
    }

    /// Read permanent root `idx`'s current value (a global variable's
    /// storage — `typelisp-rt`'s `rt_global_get`) — the mark phase already
    /// re-reads `permanent_roots[i]` by index every cycle
    /// ([`Self::gc`](Heap::gc)'s root walk), so an entry can be overwritten
    /// in place ([`Self::set_permanent_root`]) with no change to that logic.
    pub fn permanent_root(&self, idx: usize) -> Value {
        self.permanent_roots[idx]
    }

    /// Overwrite permanent root `idx`'s value in place — `rt_global_set`'s
    /// storage half. See [`Self::permanent_root`].
    pub fn set_permanent_root(&mut self, idx: usize, v: Value) {
        self.permanent_roots[idx] = v;
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
        self.session_roots.push(v);
    }

    /// The current session-root count — pass to
    /// [`truncate_session_roots`](Self::truncate_session_roots) to release
    /// everything registered after this point.
    pub fn session_root_count(&self) -> usize {
        self.session_roots.len()
    }

    /// Release every session root registered since `len` was observed. A no-op
    /// if `len` is already at or past the current count, like [`Vec::truncate`].
    ///
    /// Brackets nest: an inner session releases only its own registrations, so
    /// a nested compile leaves the outer one's values rooted.
    pub fn truncate_session_roots(&mut self, len: usize) {
        self.session_roots.truncate(len);
    }

    // ---- the in-flight throw ----------------------------------------------

    /// The value a `(throw ...)` currently travelling up the stack carries, or
    /// `None` when no throw is in flight.
    ///
    /// A single slot rather than a stack, and none of the other three root
    /// sets, because the lifetime it has to express is neither an activation's
    /// nor a session's: the value is live from the `throw` that raised it
    /// until the `catch` that consumes it, while the stack in between is being
    /// *discarded*. `roots` is exactly what a non-local exit cuts back
    /// (`truncate_roots`, from both the compiled and the interpreted side), so
    /// a value parked there would be released by the very unwind that is
    /// carrying it; `permanent_roots` never releases; `session_roots` is not
    /// bracketed by a throw.
    ///
    /// One slot suffices because at most one throw is ever in flight: an exit
    /// raised from an `unwind-protect` cleanup while another is travelling
    /// *replaces* it (CLHS — the cleanup's own exit wins), which is what
    /// overwriting this slot does.
    pub fn in_flight_throw(&self) -> Option<Value> {
        self.in_flight_throw
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
        self.macro_chars.insert(ch, f);
    }

    /// The reader macro registered for `ch`, if any.
    pub fn macro_character(&self, ch: char) -> Option<Value> {
        self.macro_chars.get(&ch).copied()
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
        self.dispatch_chars.insert((disp, sub), f);
    }

    /// The function registered for the two-character sequence, if any.
    pub fn dispatch_macro_character(&self, disp: char, sub: char) -> Option<Value> {
        self.dispatch_chars.get(&(disp, sub)).copied()
    }

    /// Whether `ch` begins a two-character dispatch sequence — i.e. whether
    /// anything at all has been registered under it.
    ///
    /// `#` is *not* reported here even when it has entries: the reader hands
    /// `#` to its own built-in dispatch, which consults
    /// [`Self::dispatch_macro_character`] before any of `#b`/`#x`/`#.`/…
    /// and so already covers the user's registrations.
    pub fn is_dispatch_char(&self, ch: char) -> bool {
        ch != '#' && self.dispatch_chars.keys().any(|(d, _)| *d == ch)
    }

    /// Park (or, with `None`, release) the in-flight throw's value — see
    /// [`in_flight_throw`](Self::in_flight_throw).
    pub fn set_in_flight_throw(&mut self, v: Option<Value>) {
        self.in_flight_throw = v;
    }

    // ---- symbols ----------------------------------------------------------

    /// Intern a symbol by name, returning its `Value::Symbol`.
    ///
    /// typelisp symbols are **case-insensitive**, so names are folded to a
    /// canonical lowercase form: `Foo`, `FOO`, `foo` all intern to the same
    /// [`SymRef`], and [`symbol_name`](Self::symbol_name) returns the canonical
    /// (lowercase) spelling.
    pub fn intern_symbol(&mut self, name: &str) -> Value {
        Value::Symbol(symbols::intern(name))
    }

    /// Intern a symbol as written inside module `ns` — the reader's entry
    /// point, and the only one that can place a symbol anywhere but the root
    /// (see [`symbols::intern_in`] for the inheritance rule).
    pub fn intern_symbol_in(&mut self, ns: symbols::NsId, name: &str) -> Value {
        Value::Symbol(symbols::intern_in(ns, name))
    }

    /// The name of an interned symbol.
    ///
    /// Takes `&self` only for call-site compatibility: a symbol carries its
    /// own name, so this needs no heap at all — [`SymRef::name`] is the
    /// heap-free way to ask.
    pub fn symbol_name(&self, id: SymRef) -> &'static str {
        id.name()
    }


    // ---- type identities ---------------------------------------------------

    /// Intern a type identity by name, returning the [`TypeKeyId`] every
    /// instance of that type carries. Permanent, like a symbol: equal names
    /// always share one id, so "same type?" is `==` on the id.
    ///
    /// Names are taken verbatim — unlike symbols, a type key is not folded.
    /// The spelling is the type's whole `::` path, and producing it is the
    /// front end's job (`typelisp-front`'s `type_key` module); this layer only
    /// stores what it is given.
    pub fn intern_type_key(&mut self, name: &str) -> TypeKeyId {
        if let Some(&id) = self.type_key_ids.get(name) {
            return TypeKeyId(id);
        }
        let id = self.type_keys.len() as u32;
        self.type_keys.push(name.to_string());
        self.type_key_ids.insert(name.to_string(), id);
        TypeKeyId(id)
    }

    /// The id `name` interns to, if it has ever been interned in this heap.
    ///
    /// The read-only half of [`intern_type_key`](Self::intern_type_key), for
    /// asking "is this value of type `name`?" without minting an id for a type
    /// no value has: `None` answers the question already (nothing can be an
    /// instance of a type never named here).
    pub fn type_key_id(&self, name: &str) -> Option<TypeKeyId> {
        self.type_key_ids.get(name).map(|&id| TypeKeyId(id))
    }

    /// The name behind an interned type identity — for printing, `Debug`, and
    /// error messages. Not for comparison: compare the ids.
    pub fn type_key_name(&self, key: TypeKeyId) -> &str {
        &self.type_keys[key.0 as usize]
    }


    // ---- paths ------------------------------------------------------------

    /// Intern a `::` path from its symbol segments, returning a `Value::Path`.
    /// Paths are permanent (they reference only permanent symbols), and equal
    /// segment sequences share one [`PathId`].
    pub fn intern_path(&mut self, segs: &[SymRef]) -> Value {
        if let Some(&id) = self.path_ids.get(segs) {
            return Value::Path(PathId(id));
        }
        let id = self.paths.len() as u32;
        self.paths.push(segs.to_vec());
        self.path_ids.insert(segs.to_vec(), id);
        Value::Path(PathId(id))
    }

    /// The symbol segments of an interned path.
    pub fn path_segments(&self, id: PathId) -> &[SymRef] {
        &self.paths[id.0 as usize]
    }

    // ---- strings ----------------------------------------------------------

    /// Store a string, returning its `Value::Str`. Strings are GC-collected.
    pub fn alloc_string(&mut self, s: String) -> Value {
        if let Some(idx) = self.str_free.pop() {
            self.str_slots[idx as usize] = Some(s);
            self.str_marks[idx as usize] = false;
            Value::Str(StrId(idx))
        } else {
            let idx = self.str_slots.len() as u32;
            self.str_slots.push(Some(s));
            self.str_marks.push(false);
            Value::Str(StrId(idx))
        }
    }

    /// The contents of a stored string.
    pub fn string(&self, id: StrId) -> &str {
        self.str_slots[id.0 as usize].as_deref().expect("dangling StrId")
    }

    /// Stores a string with content-based deduplication, returning its
    /// `Value::Str` — unlike [`alloc_string`](Self::alloc_string) (a fresh,
    /// independently-identified allocation every call, preserving `Sexpr`'s
    /// `eq`-by-identity semantics for ordinary strings), a repeated call
    /// with equal content always returns the *same* `StrId`. This is what
    /// is what a `Sexpr` symbol name and a quoted string literal share, and
    /// it is not meant as a general-purpose string constructor. (A
    /// `HashTable<K,V>` string key no longer relies on it: keys are compared
    /// with the key type's own `equals`, which for `string` is content
    /// equality, so nothing has to be interned to be found.)
    ///
    /// Interned strings are rooted permanently (never swept, like
    /// [`push_permanent_root`](Self::push_permanent_root)): a `str_intern`
    /// entry pointing at a since-freed slot would let a later call silently
    /// hand back a dangling/reused `StrId`, so this trades "a key string
    /// leaks for the heap's lifetime once interned" for that correctness —
    /// the same leak-forever trade-off already accepted elsewhere for
    /// low-cardinality, long-lived heap-external data (e.g. interned
    /// symbols, compiled code's box fields).
    pub fn intern_string(&mut self, s: &str) -> Value {
        if let Some(&id) = self.str_intern.get(s) {
            return Value::Str(id);
        }
        let v = self.alloc_string(s.to_string());
        let id = match v {
            Value::Str(id) => id,
            _ => unreachable!("alloc_string always returns Value::Str"),
        };
        self.str_intern.insert(s.to_string(), id);
        self.permanent_roots.push(v);
        v
    }

    // ---- boxed objects ------------------------------------------------------

    /// Store a [`BoxedObj`], returning its `Value::Boxed`. Boxed objects are
    /// GC-collected — same growable-slot-store shape as
    /// [`alloc_string`](Self::alloc_string), generalized so `gc`'s mark phase
    /// can trace into whatever `Value`s the payload itself holds (see
    /// [`gc`](Self::gc)).
    fn alloc_boxed(&mut self, obj: BoxedObj) -> Value {
        if let Some(idx) = self.box_free.pop() {
            self.box_slots[idx as usize] = Some(obj);
            self.box_marks[idx as usize] = false;
            Value::Boxed(BoxId(idx))
        } else {
            let idx = self.box_slots.len() as u32;
            self.box_slots.push(Some(obj));
            self.box_marks.push(false);
            Value::Boxed(BoxId(idx))
        }
    }

    /// Store an `f64`, returning its `Value::Boxed` — `Sexpr`'s `f64`
    /// variant's runtime representation (see [`BoxedObj`]'s doc comment for
    /// why a float can't be an immediate `Value` variant the way `Int`/
    /// `Char`/`Bool` are).
    pub fn alloc_f64(&mut self, f: f64) -> Value {
        self.alloc_boxed(BoxedObj::Float64(f))
    }

    /// Store an `f32`. A separate box from [`alloc_f64`](Self::alloc_f64),
    /// because `f32` and `f64` are separate types and the box has to say
    /// which one it is — see [`BoxedObj`]'s doc comment.
    pub fn alloc_f32(&mut self, f: f32) -> Value {
        self.alloc_boxed(BoxedObj::Float32(f))
    }

    /// The `f64` behind a boxed `f64`. Panics if `id` doesn't hold a
    /// `BoxedObj::Float64` — an internal-invariant trap, not a user-facing
    /// error, the same convention [`string`](Self::string)'s dangling-`StrId`
    /// panic already uses: a correctly type-checked program never passes a
    /// mismatched `BoxId` here.
    ///
    /// Deliberately *not* widening an `f32` box: a caller that would accept
    /// either width has to say so, with [`float_box`](Self::float_box).
    pub fn f64_value(&self, id: BoxId) -> f64 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Float64(f)) => *f,
            _ => panic!("BoxId does not hold an f64"),
        }
    }

    /// The `f32` behind a boxed `f32`. Panics like
    /// [`f64_value`](Self::f64_value), and for the same reason.
    pub fn f32_value(&self, id: BoxId) -> f32 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Float32(f)) => *f,
            _ => panic!("BoxId does not hold an f32"),
        }
    }

    /// A boxed float of either width, as the width it actually is — `None`
    /// when `id` is not a float box at all.
    ///
    /// The one reader for code that genuinely handles both (printing,
    /// equality, `Sexpr` dispatch). It hands back the width rather than
    /// hiding it, so those call sites still have to decide what each width
    /// means instead of silently treating everything as `f64`.
    pub fn float_box(&self, id: BoxId) -> Option<FloatBox> {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Float32(f)) => Some(FloatBox::F32(*f)),
            Some(BoxedObj::Float64(f)) => Some(FloatBox::F64(*f)),
            _ => None,
        }
    }

    /// Store an `i8`/`i16`/`u8`/`u16`/`u32`, returning its `Value::Boxed` —
    /// the runtime representation of those five `Sexpr` variants.
    ///
    /// `value` is normalized on the way in rather than trusted, so the box's
    /// contents satisfy [`NarrowInt`]'s invariant no matter what the caller
    /// had in its register. `i32` has no box: a bare `Value::Int` already
    /// means exactly that width.
    ///
    /// Panics on a width this can't be (`32` signed, or anything not in
    /// `{8, 16, 32}`) — an internal-invariant trap like
    /// [`f64_value`](Self::f64_value)'s, since the caller reaches here from a
    /// declared type.
    pub fn alloc_narrow(&mut self, width: u8, signed: bool, value: i64) -> Value {
        if !matches!((width, signed), (8, _) | (16, _) | (32, false)) {
            panic!("alloc_narrow: {}{} is not a boxed integer type", if signed { "i" } else { "u" }, width);
        }
        let value = crate::normalize_int(value, u32::from(width), signed);
        self.alloc_boxed(BoxedObj::Narrow(NarrowInt { width, signed, value }))
    }

    /// A boxed narrow integer, as the type it actually is — `None` when `id`
    /// is not one.
    ///
    /// The integer twin of [`float_box`](Self::float_box), and the only
    /// reader: there is deliberately no `narrow_value` that hands back the
    /// word without its width, because that word alone is what the five
    /// types were being confused through in the first place.
    pub fn narrow_box(&self, id: BoxId) -> Option<NarrowInt> {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Narrow(n)) => Some(*n),
            _ => None,
        }
    }

    /// True if `id` holds a boxed narrow integer of exactly this width and
    /// signedness — the positive per-type test, like
    /// [`is_f32`](Self::is_f32)/[`is_f64`](Self::is_f64). There is no
    /// width-agnostic `is_narrow` for the same reason there is no
    /// `is_float`.
    pub fn is_narrow(&self, id: BoxId, width: u8, signed: bool) -> bool {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Narrow(n)) => n.width == width && n.signed == signed,
            _ => false,
        }
    }

    /// The primitive type a numeric box *is*, spelled the way the checker
    /// spells it (`typelisp_front::type_key::type_key_of_type`) — `None` for
    /// every other box (a struct, an enum, a closure, a cell, ...), which
    /// carries an interned type key instead.
    ///
    /// The boxes below are the values whose type is written down nowhere but
    /// in the box itself: a struct says what it is through its stored key, an
    /// immediate `Value::Char` *is* a `char`, but a box holding the number
    /// `200` is a `u8` or a `u16` only because it says so. Dispatchers that
    /// need the receiver's type name (`format`'s `~/name/`) ask here.
    pub fn primitive_box_type_name(&self, id: BoxId) -> Option<&'static str> {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Float32(_)) => Some("f32"),
            Some(BoxedObj::Float64(_)) => Some("f64"),
            Some(BoxedObj::Narrow(n)) => Some(match (n.width, n.signed) {
                (8, true) => "i8",
                (16, true) => "i16",
                (8, false) => "u8",
                (16, false) => "u16",
                _ => "u32",
            }),
            Some(BoxedObj::Bignum(_)) => Some("bignum"),
            Some(BoxedObj::Ratio(_)) => Some("ratio"),
            Some(BoxedObj::RandomState(_)) => Some("random-state"),
            _ => None,
        }
    }

    /// Store a `bignum` (arbitrary-precision integer), returning its
    /// `Value::Boxed` — heap-boxed like [`alloc_f64`](Self::alloc_f64),
    /// and for the same reason (the payload can't ride alongside a tag in
    /// one 64-bit word).
    pub fn alloc_bignum(&mut self, n: num_bigint::BigInt) -> Value {
        self.alloc_boxed(BoxedObj::Bignum(n))
    }

    /// The `BigInt` behind a boxed bignum. Panics if `id` doesn't hold a
    /// `BoxedObj::Bignum` — same internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn bignum_value(&self, id: BoxId) -> &num_bigint::BigInt {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Bignum(n)) => n,
            _ => panic!("BoxId does not hold a Bignum"),
        }
    }

    /// True if `id` holds a `BoxedObj::Bignum` — the peer of
    /// [`is_f64`](Self::is_f64) for callers decoding an unknown
    /// `Value::Boxed`.
    pub fn is_bignum(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Bignum(_)))
    }

    /// Store a `ratio` (exact rational), returning its `Value::Boxed` — the
    /// `ratio` counterpart of [`alloc_bignum`](Self::alloc_bignum). The
    /// caller passes any `BigRational`; the crate keeps it reduced with a
    /// positive denominator.
    pub fn alloc_ratio(&mut self, r: num_rational::BigRational) -> Value {
        self.alloc_boxed(BoxedObj::Ratio(r))
    }

    /// The `BigRational` behind a boxed ratio. Panics if `id` doesn't hold a
    /// `BoxedObj::Ratio` — same convention as [`bignum_value`](Self::bignum_value).
    pub fn ratio_value(&self, id: BoxId) -> &num_rational::BigRational {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Ratio(r)) => r,
            _ => panic!("BoxId does not hold a Ratio"),
        }
    }

    /// True if `id` holds a `BoxedObj::Ratio`.
    pub fn is_ratio(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Ratio(_)))
    }

    /// Store a struct-shaped [`BoxedObj`] with fixed- or variable-length
    /// `fields`, returning its `Value::Boxed` — the runtime representation a
    /// `defstruct` instance, `Vector<T>`, and `cons-cell<K,V>` all share
    /// (see `BoxedObj`'s doc comment). `type_key` is what a later builtin
    /// method dispatch (`eval_builtin_method`) uses to decide what the
    /// fields mean; this layer itself doesn't interpret it.
    ///
    /// Takes an interned [`TypeKeyId`] rather than a name, so a struct cannot
    /// be built without its type identity already existing as a comparable id
    /// — see [`intern_type_key`](Self::intern_type_key).
    pub fn alloc_struct(&mut self, type_key: TypeKeyId, fields: Vec<Value>) -> Value {
        self.alloc_boxed(BoxedObj::Struct { type_key, payload: StructPayload::Fields(fields) })
    }

    /// The type identity of a boxed struct — what a "is this a `point`?" test
    /// compares. Panics if `id` doesn't hold a `BoxedObj::Struct` — same
    /// internal-invariant-trap convention as [`f64_value`](Self::f64_value).
    pub fn struct_type_key(&self, id: BoxId) -> TypeKeyId {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { type_key, .. }) => *type_key,
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// The *name* of a boxed struct's type, for printing and diagnostics.
    /// Comparisons use [`struct_type_key`](Self::struct_type_key) instead.
    pub fn struct_type_name(&self, id: BoxId) -> &str {
        self.type_key_name(self.struct_type_key(id))
    }

    /// The number of fields a boxed struct holds — e.g. `Vector<T>::length`
    /// reads this directly, unlike a `defstruct` instance's fixed arity
    /// (known statically, so callers with a static field index rarely need
    /// this). Panics if `id` doesn't hold a `BoxedObj::Struct`.
    pub fn struct_field_count(&self, id: BoxId) -> usize {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => fields.len(),
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// The `idx`-th field of a boxed struct. Panics if `id` doesn't hold a
    /// `BoxedObj::Struct`, or if `idx` is out of range — the checker (for a
    /// `defstruct` field access) or the builtin method itself (for
    /// `Vector<T>`'s own bounds check) is responsible for guaranteeing that
    /// never happens, the same convention [`car`](Self::car)/[`cdr`](Self::cdr)
    /// use for a non-cons argument.
    pub fn struct_field(&self, id: BoxId, idx: usize) -> Value {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => {
                *fields.get(idx).unwrap_or_else(|| panic!("struct field index {} out of range", idx))
            }
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// Overwrites the `idx`-th field of a boxed struct in place — the
    /// mutable-reference-semantics counterpart to [`struct_field`](Self::struct_field),
    /// mirroring [`set_car`](Self::set_car)/[`set_cdr`](Self::set_cdr)'s
    /// in-place mutation of a cons cell. Panics under the same conditions as
    /// [`struct_field`](Self::struct_field).
    pub fn struct_set_field(&mut self, id: BoxId, idx: usize, val: Value) {
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => {
                if idx >= fields.len() {
                    panic!("struct field index {} out of range", idx);
                }
                fields[idx] = val;
            }
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// Appends a new field, growing a boxed struct's field count by one —
    /// `Vector<T>::push`'s primitive (a `defstruct` instance's field count is
    /// otherwise fixed for its lifetime once [`alloc_struct`](Self::alloc_struct)
    /// returns, since only `Vector<T>`'s builtin methods ever call this).
    /// Panics if `id` doesn't hold a `BoxedObj::Struct`.
    pub fn struct_push_field(&mut self, id: BoxId, val: Value) {
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => fields.push(val),
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// Removes and returns the last field of a boxed struct, shrinking its
    /// field count by one — `Vector<T>::pop`'s primitive, the inverse of
    /// [`struct_push_field`](Self::struct_push_field). Unlike
    /// [`struct_field`](Self::struct_field)'s out-of-range panic, an empty
    /// vector is a legitimate outcome here (`None`), not an invariant
    /// violation — `pop` returns `Option<T>` (matching `HashTable<K,V>`'s
    /// `remove`, and `Vec::pop`'s own idiomatic Rust signature), unlike
    /// `get`/`set`'s "out of range panics" convention. Panics only if `id`
    /// doesn't hold a `BoxedObj::Struct` at all.
    pub fn struct_pop_field(&mut self, id: BoxId) -> Option<Value> {
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => fields.pop(),
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// True if `id` holds a `BoxedObj::Struct` with a `StructPayload::Fields`
    /// payload (a `defstruct`/`Vector<T>`/`cons-cell<K,V>` instance) —
    /// deliberately narrower than "any `Struct`" now that [`StructPayload`]
    /// has more than one shape (see [`is_hashtable`](Self::is_hashtable)),
    /// so a caller holding only a `Value::Boxed` (e.g. decoding a struct
    /// field or a generic `match` scrutinee back into an interpreter-level
    /// value) can tell a nested struct apart from a boxed float *or* a
    /// `HashTable` without risking [`f64_value`](Self::f64_value)'s or
    /// [`struct_field`](Self::struct_field)'s "wrong kind" panic.
    pub fn is_struct(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Struct { payload: StructPayload::Fields(_), .. }))
    }

    /// True if `id` holds a `BoxedObj::Struct` with a `StructPayload::Map`
    /// payload (a `HashTable<K,V>` instance) — see
    /// [`is_struct`](Self::is_struct)'s doc comment for why this needs to be
    /// a separate predicate rather than folded into it.
    pub fn is_hashtable(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Struct { payload: StructPayload::Map(_), .. }))
    }

    // ---- enums ----------------------------------------------------------------

    /// Store an enum (sum-ADT) value — variant index `variant` of `type_key`
    /// with that variant's `fields` — returning its `Value::Boxed`; the
    /// runtime representation `Option<T>`/`Result<T,E>`/user `defenum`
    /// instances all share (see [`BoxedObj::Enum`]'s doc comment for why
    /// this is a variant of its own, not a `Struct`). The enum counterpart
    /// of [`alloc_struct`](Self::alloc_struct); like `type_key` there,
    /// `variant` is stored uninterpreted — which variant means what is the
    /// checker's business, this layer only carries the index.
    pub fn alloc_enum(&mut self, type_key: TypeKeyId, variant: usize, fields: Vec<Value>) -> Value {
        self.alloc_boxed(BoxedObj::Enum { type_key, variant, fields })
    }

    /// True if `id` holds a `BoxedObj::Enum` — the enum peer of
    /// [`is_struct`](Self::is_struct), for callers decoding an unknown
    /// `Value::Boxed`.
    pub fn is_enum(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Enum { .. }))
    }

    /// The type identity of a boxed enum value — the enum peer of
    /// [`struct_type_key`](Self::struct_type_key). Panics if `id` doesn't hold
    /// a `BoxedObj::Enum`.
    pub fn enum_type_key(&self, id: BoxId) -> TypeKeyId {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Enum { type_key, .. }) => *type_key,
            _ => panic!("BoxId does not hold an Enum"),
        }
    }

    /// The *name* of a boxed enum value's type, for printing and diagnostics.
    /// Comparisons use [`enum_type_key`](Self::enum_type_key) instead.
    pub fn enum_type_name(&self, id: BoxId) -> &str {
        self.type_key_name(self.enum_type_key(id))
    }

    /// The variant index of a boxed enum value — what a `match` arm's tag
    /// test compares against. Panics if `id` doesn't hold a `BoxedObj::Enum`.
    pub fn enum_variant(&self, id: BoxId) -> usize {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Enum { variant, .. }) => *variant,
            _ => panic!("BoxId does not hold an Enum"),
        }
    }

    /// The number of fields the boxed enum value's variant carries — a
    /// `match` pattern's arity check reads this alongside
    /// [`enum_variant`](Self::enum_variant). Panics if `id` doesn't hold a
    /// `BoxedObj::Enum`.
    pub fn enum_field_count(&self, id: BoxId) -> usize {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Enum { fields, .. }) => fields.len(),
            _ => panic!("BoxId does not hold an Enum"),
        }
    }

    /// The `idx`-th field of a boxed enum value. Panics if `id` doesn't hold
    /// a `BoxedObj::Enum` or if `idx` is out of range — the checker's
    /// pattern-arity guarantee makes that an internal invariant, the same
    /// convention as [`struct_field`](Self::struct_field). No `set`
    /// counterpart exists: enum values are immutable.
    pub fn enum_field(&self, id: BoxId, idx: usize) -> Value {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Enum { fields, .. }) => {
                *fields.get(idx).unwrap_or_else(|| panic!("enum field index {} out of range", idx))
            }
            _ => panic!("BoxId does not hold an Enum"),
        }
    }

    // ---- trait objects --------------------------------------------------------

    /// Box `value` as a trait object dispatching through vtable `vtable_id` —
    /// see [`BoxedObj::Dyn`]. Like `alloc_enum`, `vtable_id` is stored
    /// uninterpreted: which table it names is the interpreter's/runtime's
    /// business, this layer only carries the number.
    pub fn alloc_dyn(&mut self, vtable_id: u32, value: Value) -> Value {
        self.alloc_boxed(BoxedObj::Dyn { vtable_id, value })
    }

    /// True if `id` holds a `BoxedObj::Dyn`, for a caller decoding an unknown
    /// `Value::Boxed` — note this is `false` for the *concrete* value inside,
    /// so `is_struct`/`is_enum` stay unaffected by boxing.
    pub fn is_dyn(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Dyn { .. }))
    }

    /// The vtable identifier of a trait object. Panics if `id` doesn't hold a
    /// `BoxedObj::Dyn` — same internal-invariant-trap convention as
    /// [`enum_variant`](Self::enum_variant).
    pub fn dyn_vtable_id(&self, id: BoxId) -> u32 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Dyn { vtable_id, .. }) => *vtable_id,
            _ => panic!("BoxId does not hold a Dyn"),
        }
    }

    /// The concrete value inside a trait object. Panics if `id` doesn't hold
    /// a `BoxedObj::Dyn`.
    pub fn dyn_value(&self, id: BoxId) -> Value {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Dyn { value, .. }) => *value,
            _ => panic!("BoxId does not hold a Dyn"),
        }
    }

    // ---- cells ----------------------------------------------------------------

    /// Store a mutable variable slot holding `v`, returning an owning
    /// `Rc<BoxId>` handle — see [`BoxedObj::Cell`]. The cell (and thus its
    /// current contents) stays live for exactly as long as any clone of the
    /// handle does: `gc()` treats every still-referenced cell as a root by
    /// consulting the weak registry this populates (`cell_registry`), so no
    /// caller-side root bookkeeping exists for cells at all — a collection
    /// triggered from *anywhere* (interpreter or compiled code) sees them.
    /// Unlike [`cons`](Self::cons), this can never itself trigger a
    /// collection (the box store grows on demand, it is not a fixed arena),
    /// so `v` may be un-rooted at the moment of the call.
    pub fn alloc_cell(&mut self, v: Value) -> std::rc::Rc<BoxId> {
        let id = match self.alloc_boxed(BoxedObj::Cell(v)) {
            Value::Boxed(id) => id,
            _ => unreachable!("alloc_boxed always returns Value::Boxed"),
        };
        let rc = std::rc::Rc::new(id);
        self.cell_registry.push(std::rc::Rc::downgrade(&rc));
        rc
    }

    /// The current contents of a cell. Panics if `id` doesn't hold a
    /// `BoxedObj::Cell` — same internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn cell_get(&self, id: BoxId) -> Value {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Cell(v)) => *v,
            _ => panic!("BoxId does not hold a Cell"),
        }
    }

    /// Overwrites a cell's contents in place — `setf`'s primitive for a
    /// heap-cell-backed binding. Panics under the same conditions as
    /// [`cell_get`](Self::cell_get).
    pub fn cell_set(&mut self, id: BoxId, v: Value) {
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Cell(slot)) => *slot = v,
            _ => panic!("BoxId does not hold a Cell"),
        }
    }

    /// True if `id` holds a `BoxedObj::Cell` — the peer of
    /// [`is_struct`](Self::is_struct)/[`is_hashtable`](Self::is_hashtable)
    /// for callers that decode a `Value::Boxed` without knowing its kind.
    pub fn is_cell(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Cell(_)))
    }

    /// [`alloc_cell`](Self::alloc_cell) without the owning-`Rc` liveness
    /// registration — for *compiled* code (`rt_cell_new`), which has no Rust
    /// `Rc` to hold: the cell's lifetime is governed purely by ordinary
    /// reachability (a GC root, a compiled closure's env slot, another
    /// cell, ...), exactly like any other boxed object. The caller must
    /// therefore make the returned value reachable before anything can
    /// trigger a collection; like `alloc_cell`, the allocation itself never
    /// triggers one.
    pub fn alloc_cell_unregistered(&mut self, v: Value) -> Value {
        self.alloc_boxed(BoxedObj::Cell(v))
    }

    // ---- closures --------------------------------------------------------------

    /// True if `id` holds a `BoxedObj::Float64` — the *positive* `f64` test
    /// callers decoding an unknown `Value::Boxed` must use now that "not a
    /// struct and not a hashtable" no longer implies float (cells and
    /// closures are boxed too).
    ///
    /// There is deliberately no width-agnostic `is_float`: a caller that
    /// accepts either width has to name both, or read the width out with
    /// [`float_box`](Self::float_box).
    pub fn is_f64(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Float64(_)))
    }

    /// True if `id` holds a `BoxedObj::Float32`. See [`is_f64`](Self::is_f64).
    pub fn is_f32(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Float32(_)))
    }

    // ---- compiled closures ------------------------------------------------------

    /// Store a compiled function value — native entry point `fn_ptr` plus
    /// its captured environment — returning its `Value::Boxed`; see
    /// [`BoxedObj::CompiledClosure`] for the `env`/`sexpr_mask` slot
    /// convention. Like [`alloc_cell`](Self::alloc_cell), never itself
    /// triggers a collection, so the env values may be un-rooted at the
    /// moment of the call.
    pub fn alloc_compiled_closure(
        &mut self,
        fn_ptr: usize,
        env: Vec<Value>,
        sexpr_mask: u64,
        body_abi: u8,
    ) -> Value {
        self.alloc_boxed(BoxedObj::CompiledClosure { fn_ptr, env, sexpr_mask, body_abi })
    }

    /// Which calling convention `fn_ptr` was compiled under — the classic
    /// `f(args, argc, env, envlen)` or the coroutine `f(frame)` (Phase C2).
    ///
    /// The closure has to carry this rather than the process deciding it once,
    /// because a single process really can hold both: the island loaded from
    /// its committed dump makes closures under whatever ABI *it* was compiled
    /// under, while code JIT-compiled in the same process makes them under the
    /// ABI this binary emits, and across an ABI change those are two different
    /// answers.
    ///
    /// Panics like [`compiled_closure_fnptr`](Self::compiled_closure_fnptr).
    pub fn compiled_closure_body_abi(&self, id: BoxId) -> u8 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { body_abi, .. }) => *body_abi,
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    /// True if `id` holds a `BoxedObj::CompiledClosure`. A function value can
    /// equally be an *interpreted* closure ([`is_closure`](Self::is_closure))
    /// or a built-in ([`is_builtin_fn`](Self::is_builtin_fn)); the three
    /// together are every `Type::Fn` value.
    pub fn is_compiled_closure(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::CompiledClosure { .. }))
    }

    // ---- compiled frames --------------------------------------------------

    /// Allocate a compiled function's activation record: `nslots` words, all
    /// zero, and an all-clear mask. See [`BoxedObj::Frame`].
    ///
    /// **The mask is not an argument.** Which slots hold tagged values is
    /// settled one binding at a time by
    /// [`set_frame_mask_bit`](Self::set_frame_mask_bit), so allocating a
    /// frame needs only the slot count — which the compiler knows from
    /// counting binding sites, without classifying them. An all-clear mask
    /// is also the safe starting point: a slot nothing has bound yet is not
    /// traced, and cannot be mistaken for a reference.
    ///
    /// Zero, not `Value::Empty`'s encoding: an unmasked slot is a raw word and
    /// zero is a fine raw word, while a masked slot reads as `TAG_FIXNUM` 0
    /// until the prologue writes it — a fixnum references nothing, so a
    /// collection between the allocation and the first store traces nothing
    /// bogus.
    ///
    /// Like [`alloc_cell`](Self::alloc_cell), never itself triggers a
    /// collection.
    pub fn alloc_frame(&mut self, nslots: usize) -> Value {
        let words = (nslots + 63) / 64;
        self.alloc_boxed(BoxedObj::Frame { words: vec![0; nslots], mask: vec![0; words], pc: 0 })
    }

    /// True if `id` holds a [`BoxedObj::Frame`].
    pub fn is_frame(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Frame { .. }))
    }

    /// The address of a frame's word array, for compiled code to load and
    /// store its locals through.
    ///
    /// # Stability
    ///
    /// The address stays valid for as long as the frame is *live*. Growing
    /// `box_slots` moves the `BoxedObj` but not the `Vec`'s buffer, and the
    /// sweep writes `None` in place rather than compacting — so the only way
    /// to invalidate this pointer is to let the frame be collected, which is
    /// the caller's job to prevent by keeping it reachable (the task's stack
    /// does).
    ///
    /// Panics if `id` does not hold a frame — the compiler's own bookkeeping,
    /// so an internal invariant.
    pub fn frame_data_ptr(&mut self, id: BoxId) -> *mut i64 {
        match &mut self.box_slots[id.0 as usize] {
            Some(BoxedObj::Frame { words, .. }) => words.as_mut_ptr(),
            _ => panic!("BoxId does not hold a Frame"),
        }
    }

    /// How many slots a frame holds. Panics like [`frame_data_ptr`](Self::frame_data_ptr).
    pub fn frame_len(&self, id: BoxId) -> usize {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Frame { words, .. }) => words.len(),
            _ => panic!("BoxId does not hold a Frame"),
        }
    }

    /// One slot, as the raw word it is stored as. Panics like
    /// [`frame_data_ptr`](Self::frame_data_ptr), and on an out-of-range slot —
    /// the compiler's layout guarantee makes that an internal invariant too.
    pub fn frame_word(&self, id: BoxId, idx: usize) -> i64 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Frame { words, .. }) => {
                *words.get(idx).unwrap_or_else(|| panic!("frame slot {} out of range", idx))
            }
            _ => panic!("BoxId does not hold a Frame"),
        }
    }

    /// Writes one slot. Panics like [`frame_word`](Self::frame_word).
    pub fn set_frame_word(&mut self, id: BoxId, idx: usize, w: i64) {
        match &mut self.box_slots[id.0 as usize] {
            Some(BoxedObj::Frame { words, .. }) => match words.get_mut(idx) {
                Some(slot) => *slot = w,
                None => panic!("frame slot {} out of range", idx),
            },
            _ => panic!("BoxId does not hold a Frame"),
        }
    }

    /// Whether slot `idx` holds a tagged value the collector should trace.
    /// Panics like [`frame_word`](Self::frame_word).
    pub fn frame_mask_bit(&self, id: BoxId, idx: usize) -> bool {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Frame { words, mask, .. }) => {
                assert!(idx < words.len(), "frame slot {} out of range", idx);
                mask[idx / 64] & (1u64 << (idx % 64)) != 0
            }
            _ => panic!("BoxId does not hold a Frame"),
        }
    }

    /// Marks slot `idx` as holding a tagged value, so the collector traces
    /// it from here on. Called once per collectable binding, where that
    /// binding's `rt_push_sexpr_root` used to stand.
    ///
    /// There is deliberately no way to *clear* a bit. A slot belongs to one
    /// binding for the whole activation — the compiler gives each binding
    /// site its own slot rather than reusing one across disjoint scopes —
    /// so a bit that could be cleared would mean a slot whose kind changes
    /// under it, and then neither setting nor clearing is safe at the right
    /// moment. Panics like [`frame_word`](Self::frame_word).
    pub fn set_frame_mask_bit(&mut self, id: BoxId, idx: usize) {
        match &mut self.box_slots[id.0 as usize] {
            Some(BoxedObj::Frame { words, mask, .. }) => {
                assert!(idx < words.len(), "frame slot {} out of range", idx);
                mask[idx / 64] |= 1u64 << (idx % 64);
            }
            _ => panic!("BoxId does not hold a Frame"),
        }
    }

    /// Where the function resumes: 0 on entry, a call site's id afterwards.
    /// Panics like [`frame_word`](Self::frame_word).
    pub fn frame_pc(&self, id: BoxId) -> u32 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Frame { pc, .. }) => *pc,
            _ => panic!("BoxId does not hold a Frame"),
        }
    }

    /// Sets the resume point. Panics like [`frame_word`](Self::frame_word).
    pub fn set_frame_pc(&mut self, id: BoxId, pc: u32) {
        match &mut self.box_slots[id.0 as usize] {
            Some(BoxedObj::Frame { pc: slot, .. }) => *slot = pc,
            _ => panic!("BoxId does not hold a Frame"),
        }
    }

    /// Store an *interpreted* closure — parameter list, return
    /// representation, body, and captured environment, all core forms and all
    /// `Value` — returning its `Value::Boxed`. See [`BoxedObj::Closure`].
    pub fn alloc_closure(&mut self, params: Value, ret: Value, body: Value, env: Value) -> Value {
        self.alloc_boxed(BoxedObj::Closure { params, ret, body, env })
    }

    /// True if `id` holds a [`BoxedObj::Closure`].
    pub fn is_closure(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Closure { .. }))
    }

    /// An interpreted closure's parts: `(params, body, env)`. Panics if `id`
    /// does not hold one — the same internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn closure_parts(&self, id: BoxId) -> (Value, Value, Value) {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Closure { params, body, env, .. }) => (*params, *body, *env),
            _ => panic!("BoxId does not hold a Closure"),
        }
    }

    /// An interpreted closure's declared return representation — the core
    /// form, not a decoded `Repr` (this crate has no notion of one). Only a
    /// boundary crossing needs it; see [`BoxedObj::Closure`]. Panics like
    /// [`closure_parts`](Self::closure_parts).
    pub fn closure_ret(&self, id: BoxId) -> Value {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Closure { ret, .. }) => *ret,
            _ => panic!("BoxId does not hold a Closure"),
        }
    }

    /// An interpreted closure is immutable, deliberately: `labels` needs a
    /// group of siblings that can all see each other, and it gets that by
    /// putting empty *cells* in a frame first and filling them in after each
    /// closure is built. The closures capture the finished environment from
    /// the start, so there is nothing to tie back — and so no setter here.

    /// Store a built-in used as a function value, returning its
    /// `Value::Boxed` — `recv_type` is the receiver type for a built-in
    /// *method* (`i32::+`) and `None` for a free built-in (`eval`). See
    /// [`BoxedObj::Builtin`].
    ///
    /// `name` is interned, so the box is two permanent indices wide and
    /// allocating one never triggers a collection (like
    /// [`alloc_cell`](Self::alloc_cell)).
    pub fn alloc_builtin_fn(&mut self, recv_type: Option<PathId>, name: &str, ret_key: &str) -> Value {
        let name = match self.intern_string(name) {
            Value::Str(id) => id,
            _ => unreachable!("intern_string always returns Value::Str"),
        };
        let ret_key = match self.intern_string(ret_key) {
            Value::Str(id) => id,
            _ => unreachable!("intern_string always returns Value::Str"),
        };
        self.alloc_boxed(BoxedObj::Builtin { recv_type, name, ret_key })
    }

    /// True if `id` holds a `BoxedObj::Builtin`.
    pub fn is_builtin_fn(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Builtin { .. }))
    }

    /// A built-in function value's receiver type — `None` for a free
    /// built-in. Panics if `id` doesn't hold a `BoxedObj::Builtin`, the same
    /// internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn builtin_fn_recv(&self, id: BoxId) -> Option<PathId> {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Builtin { recv_type, .. }) => *recv_type,
            _ => panic!("BoxId does not hold a Builtin"),
        }
    }

    /// A built-in function value's name. Panics like
    /// [`builtin_fn_recv`](Self::builtin_fn_recv).
    pub fn builtin_fn_name(&self, id: BoxId) -> &str {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Builtin { name, .. }) => self.string(*name),
            _ => panic!("BoxId does not hold a Builtin"),
        }
    }

    /// The runtime identity of what a built-in function value returns — see
    /// [`BoxedObj::Builtin`]'s `ret_key`.
    pub fn builtin_fn_ret_key(&self, id: BoxId) -> &str {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Builtin { ret_key, .. }) => self.string(*ret_key),
            _ => panic!("BoxId does not hold a Builtin"),
        }
    }

    // ---- random states ------------------------------------------------------

    /// Store a `random-state` at `seed`, returning its `Value::Boxed` — see
    /// [`BoxedObj::RandomState`] for why a stream is a box rather than an
    /// immediate. Allocates one box slot and never collects.
    pub fn alloc_random_state(&mut self, seed: u64) -> Value {
        self.alloc_boxed(BoxedObj::RandomState(seed))
    }

    /// True if `id` holds a `BoxedObj::RandomState`.
    pub fn is_random_state(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::RandomState(_)))
    }

    /// A `random-state`'s current seed. Panics if `id` doesn't hold a
    /// `BoxedObj::RandomState` — the same internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn random_state_seed(&self, id: BoxId) -> u64 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::RandomState(seed)) => *seed,
            _ => panic!("BoxId does not hold a RandomState"),
        }
    }

    /// Advance a `random-state` to `seed`, in place — every holder of this box
    /// observes the change, which is what makes it one shared stream. Panics
    /// like [`random_state_seed`](Self::random_state_seed).
    pub fn set_random_state_seed(&mut self, id: BoxId, seed: u64) {
        match &mut self.box_slots[id.0 as usize] {
            Some(BoxedObj::RandomState(s)) => *s = seed,
            _ => panic!("BoxId does not hold a RandomState"),
        }
    }

    /// A compiled closure's native entry point. Panics if `id` doesn't hold
    /// a `BoxedObj::CompiledClosure` — the same internal-invariant-trap
    /// convention as [`f64_value`](Self::f64_value).
    pub fn compiled_closure_fnptr(&self, id: BoxId) -> usize {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { fn_ptr, .. }) => *fn_ptr,
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    /// A compiled closure's capture mask — bit `i` set means env slot `i`
    /// holds a real tagged value (see [`BoxedObj::CompiledClosure`]).
    /// Panics like [`compiled_closure_fnptr`](Self::compiled_closure_fnptr).
    pub fn compiled_closure_mask(&self, id: BoxId) -> u64 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { sexpr_mask, .. }) => *sexpr_mask,
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    /// The number of captured slots a compiled closure carries. Panics like
    /// [`compiled_closure_fnptr`](Self::compiled_closure_fnptr).
    pub fn compiled_closure_env_len(&self, id: BoxId) -> usize {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { env, .. }) => env.len(),
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    /// The `idx`-th captured slot of a compiled closure. Panics if `id`
    /// doesn't hold a `BoxedObj::CompiledClosure` or if `idx` is out of
    /// range — the compiler's capture-layout guarantee makes that an
    /// internal invariant, same convention as [`enum_field`](Self::enum_field).
    pub fn compiled_closure_env_get(&self, id: BoxId, idx: usize) -> Value {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { env, .. }) => {
                *env.get(idx).unwrap_or_else(|| panic!("compiled closure env index {} out of range", idx))
            }
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    // ---- hash tables --------------------------------------------------------

    /// Stores an empty hash map, returning its `Value::Boxed` —
    /// `HashTable<K,V>`'s runtime representation (see `BoxedObj`/
    /// `StructPayload::Map`'s doc comments).
    pub fn alloc_hashtable(&mut self, type_key: TypeKeyId) -> Value {
        self.alloc_boxed(BoxedObj::Struct { type_key, payload: StructPayload::Map(HashMap::new()) })
    }

    /// The bucket at `hash`, or `&[]` when there is none. Every read below
    /// goes through here.
    fn bucket(&self, id: BoxId, hash: i64) -> &[(Value, Value)] {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => {
                map.get(&hash).map(Vec::as_slice).unwrap_or(&[])
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// How many entries share `hash`. The caller walks them with
    /// [`hashtable_bucket_key`](Self::hashtable_bucket_key) and compares each
    /// with the key type's own `equals` — this layer has no opinion about
    /// what makes two keys the same.
    pub fn hashtable_bucket_count(&self, id: BoxId, hash: i64) -> usize {
        self.bucket(id, hash).len()
    }

    /// The `i`th key in `hash`'s bucket. Panics on an index the caller did
    /// not get from [`hashtable_bucket_count`](Self::hashtable_bucket_count)
    /// — an internal-invariant trap, like every other index here.
    pub fn hashtable_bucket_key(&self, id: BoxId, hash: i64, i: usize) -> Value {
        match self.bucket(id, hash).get(i) {
            Some((k, _)) => *k,
            None => panic!("HashTable: bucket index {} out of range", i),
        }
    }

    /// The `i`th value in `hash`'s bucket. See
    /// [`hashtable_bucket_key`](Self::hashtable_bucket_key).
    pub fn hashtable_bucket_value(&self, id: BoxId, hash: i64, i: usize) -> Value {
        match self.bucket(id, hash).get(i) {
            Some((_, v)) => *v,
            None => panic!("HashTable: bucket index {} out of range", i),
        }
    }

    /// Writes `key -> val` at position `i` of `hash`'s bucket — replacing the
    /// entry already there, or appending when `i` is the bucket's current
    /// length. The caller decides which by looking first, which is the same
    /// call it had to make anyway to answer "was this key already present".
    pub fn hashtable_bucket_put(&mut self, id: BoxId, hash: i64, i: usize, key: Value, val: Value) {
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => {
                let bucket = map.entry(hash).or_default();
                if i < bucket.len() {
                    bucket[i] = (key, val);
                } else if i == bucket.len() {
                    bucket.push((key, val));
                } else {
                    panic!("HashTable: bucket index {} out of range (len {})", i, bucket.len());
                }
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// Removes the `i`th entry of `hash`'s bucket, keeping the rest in order.
    pub fn hashtable_bucket_delete(&mut self, id: BoxId, hash: i64, i: usize) {
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => {
                let Some(bucket) = map.get_mut(&hash) else {
                    panic!("HashTable: no bucket at hash {}", hash);
                };
                if i >= bucket.len() {
                    panic!("HashTable: bucket index {} out of range (len {})", i, bucket.len());
                }
                bucket.remove(i);
                if bucket.is_empty() {
                    map.remove(&hash);
                }
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// The number of entries in a boxed hash map. Panics if `id` doesn't
    /// hold a `BoxedObj::Struct` with a `StructPayload::Map` payload.
    pub fn hashtable_count(&self, id: BoxId) -> usize {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => {
                map.values().map(Vec::len).sum()
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// Removes every entry in place. Panics if `id` doesn't hold a
    /// `BoxedObj::Struct` with a `StructPayload::Map` payload.
    pub fn hashtable_clear(&mut self, id: BoxId) {
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => map.clear(),
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// Every `(key, value)` pair currently stored, as plain [`Value`]s — the
    /// primitive `HashTable<K,V>::keys`/`values`/`entries` snapshot off of
    /// (see those builtins in `crate::eval::interp`'s `"hashtable"` arm).
    /// Order is whatever the underlying `HashMap` iterates in (unspecified,
    /// like `HashTable<K,V>`'s method surface always has been). Panics if
    /// `id` doesn't hold a `BoxedObj::Struct` with a `StructPayload::Map`
    /// payload.
    pub fn hashtable_pairs(&self, id: BoxId) -> Vec<(Value, Value)> {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => {
                map.values().flat_map(|b| b.iter().copied()).collect()
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    // ---- scopes -------------------------------------------------------------

    /// Allocates one fresh, empty scope frame box — see
    /// [`StructPayload`]'s doc comment for why a frame is a box of its own
    /// (frame *sharing* across `scope_clone_frames`) rather than a `HashMap`
    /// stored inline in the scope's payload.
    fn alloc_scope_frame(&mut self) -> BoxId {
        let obj = BoxedObj::Struct { type_key: TypeKeyId::SCOPE_FRAME, payload: StructPayload::Frame(HashMap::new()) };
        match self.alloc_boxed(obj) {
            Value::Boxed(id) => id,
            _ => unreachable!("alloc_boxed always returns Value::Boxed"),
        }
    }

    /// The frame stack of a scope box. Panics if `id` doesn't hold a
    /// `BoxedObj::Struct` with a `StructPayload::Frames` payload — same
    /// internal-invariant-trap convention as
    /// [`hashtable_bucket_key`](Self::hashtable_bucket_key)'s.
    fn scope_frames(&self, id: BoxId) -> &Vec<BoxId> {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Frames(frames), .. }) => frames,
            _ => panic!("BoxId does not hold a Scope"),
        }
    }

    /// One frame's bindings, by the frame's own `BoxId`. Panics if `fid`
    /// doesn't hold a `StructPayload::Frame` payload — every `BoxId` on a
    /// scope's frame stack does, by construction.
    fn frame_bindings(&self, fid: BoxId) -> &HashMap<String, Value> {
        match &self.box_slots[fid.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Frame(map), .. }) => map,
            _ => panic!("BoxId does not hold a Scope frame"),
        }
    }

    /// Stores a fresh `Scope<V>` with one empty frame already pushed — the
    /// frame a function's own parameters/captures bind into, matching
    /// `Scope::new`'s "ready to use immediately" convention (see the
    /// interpreter's `scope_new`) — returning its `Value::Boxed`. Two boxes
    /// are allocated (the scope and its first frame); like every box-store
    /// allocation this never itself triggers a collection.
    pub fn alloc_scope(&mut self) -> Value {
        let frame = self.alloc_scope_frame();
        self.alloc_boxed(BoxedObj::Struct { type_key: TypeKeyId::SCOPE, payload: StructPayload::Frames(vec![frame]) })
    }

    /// `Scope::clone-frames`'s primitive: a brand-new scope box whose stack
    /// holds the *same* frame boxes `id`'s currently does (a `BoxId` copy
    /// per frame, never a copy of a frame's entries) — a later
    /// [`scope_set`](Self::scope_set) into a shared frame is visible through
    /// both scopes, while each scope's stack grows/shrinks independently
    /// ([`scope_push_frame`](Self::scope_push_frame)/
    /// [`scope_pop_frame`](Self::scope_pop_frame) touch only the one scope
    /// they're called on). Panics like [`scope_get`](Self::scope_get).
    pub fn scope_clone_frames(&mut self, id: BoxId) -> Value {
        let frames = self.scope_frames(id).clone();
        self.alloc_boxed(BoxedObj::Struct { type_key: TypeKeyId::SCOPE, payload: StructPayload::Frames(frames) })
    }

    /// Pushes a fresh, empty frame onto a scope's stack. Panics like
    /// [`scope_get`](Self::scope_get).
    pub fn scope_push_frame(&mut self, id: BoxId) {
        let frame = self.alloc_scope_frame();
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Frames(frames), .. }) => frames.push(frame),
            _ => panic!("BoxId does not hold a Scope"),
        }
    }

    /// Pops the newest frame off a scope's stack (the frame box itself
    /// becomes garbage once no other scope shares it). A no-op on an empty
    /// stack — the pre-unification `scope_pop_frame`'s `Vec::pop` behavior.
    /// Panics like [`scope_get`](Self::scope_get).
    pub fn scope_pop_frame(&mut self, id: BoxId) {
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Frames(frames), .. }) => {
                frames.pop();
            }
            _ => panic!("BoxId does not hold a Scope"),
        }
    }

    /// `Scope::get`'s primitive: searches from the most-recently-pushed
    /// frame outward, returning the first binding of `name` found, or `None`
    /// if no frame binds it (the search deliberately ends at this scope's
    /// own first frame — an *enclosing function's* scope is a different
    /// value entirely; see `compiler.rs`'s module doc comment). Panics if
    /// `id` doesn't hold a `StructPayload::Frames` payload.
    pub fn scope_get(&self, id: BoxId, name: &str) -> Option<Value> {
        for fid in self.scope_frames(id).iter().rev() {
            if let Some(v) = self.frame_bindings(*fid).get(name) {
                return Some(*v);
            }
        }
        None
    }

    /// `Scope::set`'s primitive: binds `name` to `v` in the newest frame —
    /// always, even if an older (possibly shared, via
    /// [`scope_clone_frames`](Self::scope_clone_frames)) frame already binds
    /// `name`, so a `let`/`labels` def's own bindings never leak into
    /// whatever scope it borrowed frames from. Panics if `id` doesn't hold a
    /// `StructPayload::Frames` payload, or if the stack has no frame to
    /// write into (every frame popped) — the caller-side invariant the
    /// interpreter turns into an `EvalError` at its own boundary, same as
    /// the hash-table key-shape panics.
    pub fn scope_set(&mut self, id: BoxId, name: &str, v: Value) {
        let top = *self.scope_frames(id).last().expect("Scope::set: no frame to write into");
        match self.box_slots[top.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Frame(map), .. }) => {
                map.insert(name.to_string(), v);
            }
            _ => panic!("BoxId does not hold a Scope frame"),
        }
    }

    /// True if `id` holds a `BoxedObj::Struct` with a
    /// `StructPayload::Frames` payload (a `Scope<V>` value) — the peer of
    /// [`is_struct`](Self::is_struct)/[`is_hashtable`](Self::is_hashtable)
    /// for callers that decode a `Value::Boxed` without knowing its kind.
    /// (Frame boxes get no public predicate: they are internal constituents
    /// of some scope, never handed out as standalone values.)
    pub fn is_scope(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Struct { payload: StructPayload::Frames(_), .. }))
    }

    /// The number of frames currently on a scope's stack — what the
    /// interpreter's `#<scope depth=N>` display shows, and its guard for
    /// turning [`scope_set`](Self::scope_set)'s "no frame to write into"
    /// panic into a catchable evaluation error before calling in (the same
    /// pre-check split as the hash-table key-shape panics). Panics like
    /// [`scope_get`](Self::scope_get).
    pub fn scope_frame_count(&self, id: BoxId) -> usize {
        self.scope_frames(id).len()
    }

    // ---- allocation -------------------------------------------------------

    /// Allow the arena to grow by appending chunks until it holds `max_cells`,
    /// instead of reporting [`Error::HeapExhausted`] the moment a GC frees
    /// nothing. Growth never moves an existing cell (see the module doc
    /// comment), so live [`Value::Cons`] pointers stay valid across it.
    ///
    /// A limit at or below the current capacity disables growth, so
    /// `set_growth_limit(0)` restores the fixed-arena default. The ceiling is
    /// the point: it is what still turns a runaway leak into a loud error
    /// rather than letting the process be OOM-killed.
    pub fn set_growth_limit(&mut self, max_cells: usize) {
        self.growth_limit = max_cells;
    }

    /// The current growth ceiling in cells (`0` when the arena is fixed).
    pub fn growth_limit(&self) -> usize {
        self.growth_limit
    }

    /// Collect before *every* cons allocation. Enormously slow — this is a
    /// test/debug mode, never a production setting.
    ///
    /// It exists because a missing GC root is otherwise close to undebuggable:
    /// the allocation that frees an unrooted intermediate is rarely the one
    /// that later reads the corrupted value, so the symptom appears far from
    /// the cause and only under whatever allocation pattern happens to empty
    /// the free list. Under stress mode the two coincide — the very next
    /// `cons` after a value goes unrooted is the one that collects it — so a
    /// test that would fail once in a hundred runs fails every run.
    pub fn set_gc_stress(&mut self, on: bool) {
        self.gc_stress = on;
    }

    /// Whether [`Heap::set_gc_stress`] is currently on.
    pub fn gc_stress(&self) -> bool {
        self.gc_stress
    }

    /// Append one chunk, doubling the arena but never passing `growth_limit`.
    /// Returns false when growth is disabled or the ceiling is already reached,
    /// which is what makes the caller report [`Error::HeapExhausted`].
    fn grow(&mut self) -> bool {
        if self.growth_limit <= self.cap {
            return false;
        }
        // Doubling keeps the amortized cost of growth constant. `MIN_CHUNK`
        // stops a heap created with a tiny (or zero) capacity from growing one
        // cell at a time.
        const MIN_CHUNK: usize = 1024;
        let want = self.cap.max(MIN_CHUNK);
        let len = want.min(self.growth_limit - self.cap);
        if len == 0 {
            return false;
        }
        let (chunk, free) = Self::alloc_chunk(len, self.free);
        self.chunks.push(chunk);
        self.cap += len;
        self.free = free;
        self.free_count += len;
        true
    }

    /// Allocate a cons cell `(car . cdr)`. Runs a GC if the free list is empty,
    /// then grows the arena if [`Heap::set_growth_limit`] permits; returns
    /// [`Error::HeapExhausted`] if even then no cell is available.
    pub fn cons(&mut self, car: Value, cdr: Value) -> Result<Value, Error> {
        if self.gc_stress {
            // Deliberately collect before *every* allocation so a caller that
            // failed to root an intermediate value is caught here, at the
            // allocation that invalidates it, instead of surfacing later as
            // corruption far from the missing root. Test-only — see
            // `set_gc_stress`.
            self.gc();
        }
        if self.free.is_null() {
            self.gc();
            // A collection that hands back one cell is not a collection that
            // helped: with a live set just under capacity the *next* cons
            // empties the free list again, so nearly every allocation pays for
            // a full mark of the whole live set. (Measured before this check:
            // loading the prelude into a `1 << 16` heap took ~108s against 1.2s
            // in a `1 << 18` one — the same work, ~90x the time.) So grow
            // whenever the collection failed to restore a working margin, not
            // only when it freed nothing at all.
            //
            // A fixed arena is unaffected: `grow` refuses immediately when no
            // ceiling was set, so `Error::HeapExhausted` still means exactly
            // "a collection freed nothing and growth is not permitted".
            if self.free_count.saturating_mul(HEADROOM_DIVISOR) < self.cap {
                self.grow();
            }
            if self.free.is_null() && !self.grow() {
                return Err(Error::HeapExhausted);
            }
        }
        let p = self.free;
        unsafe {
            self.free = (*p).next_free;
            (*p).car = car;
            (*p).cdr = cdr;
            (*p).mark = false;
            (*p).next_free = ptr::null_mut();
            // A recycled cell must not inherit the position of whatever form
            // used to live at this address. Blanking here is what replaced both
            // the reader table's per-batch bulk clear and the sweep's
            // entry-by-entry removal — see `Cell`'s doc comment.
            (*p).car_loc = LocId::NONE;
            (*p).self_loc = LocId::NONE;
        }
        self.free_count -= 1;
        Ok(Value::Cons(ConsRef(p)))
    }

    // ---- accessors --------------------------------------------------------

    /// `(car c)`. Errors unless `c` is a cons (the empty list has no car).
    pub fn car(&self, v: Value) -> Result<Value, Error> {
        match v {
            Value::Cons(c) => Ok(unsafe { (*c.0).car }),
            _ => Err(Error::NotACons),
        }
    }

    /// `(cdr c)`. Errors unless `c` is a cons.
    pub fn cdr(&self, v: Value) -> Result<Value, Error> {
        match v {
            Value::Cons(c) => Ok(unsafe { (*c.0).cdr }),
            _ => Err(Error::NotACons),
        }
    }

    /// `(rplaca c val)`. Errors unless `c` is a cons.
    pub fn set_car(&mut self, c: Value, val: Value) -> Result<(), Error> {
        match c {
            Value::Cons(cell) => {
                unsafe { (*cell.0).car = val };
                Ok(())
            }
            _ => Err(Error::NotACons),
        }
    }

    /// `(rplacd c val)`. Errors unless `c` is a cons.
    pub fn set_cdr(&mut self, c: Value, val: Value) -> Result<(), Error> {
        match c {
            Value::Cons(cell) => {
                unsafe { (*cell.0).cdr = val };
                Ok(())
            }
            _ => Err(Error::NotACons),
        }
    }

    /// Collect the elements of a proper list into a vector. Errors on an
    /// improper list (a non-`Empty`, non-cons tail).
    pub fn list_to_vec(&self, v: Value) -> Result<Vec<Value>, Error> {
        let mut out = Vec::new();
        let mut cur = v;
        loop {
            match cur {
                Value::Empty => return Ok(out),
                Value::Cons(c) => {
                    out.push(unsafe { (*c.0).car });
                    cur = unsafe { (*c.0).cdr };
                }
                _ => return Err(Error::ImproperList),
            }
        }
    }

    /// Like [`Heap::list_to_vec`], but pairs each element with the source
    /// location the reader recorded for it (the spine cell's own `car_loc`) —
    /// `None` for an element from a list with no location (e.g. a
    /// macro-expanded or otherwise synthesized one). The checker uses this so a
    /// bare atom in argument position can carry its own `Loc`, which the LSP's
    /// hover/goto-definition need.
    pub fn list_to_vec_locs(&self, v: Value) -> Result<Vec<(Value, Option<crate::errors::Loc>)>, Error> {
        let mut out = Vec::new();
        let mut cur = v;
        loop {
            match cur {
                Value::Empty => return Ok(out),
                Value::Cons(c) => {
                    let loc = self.loc_of(unsafe { (*c.0).car_loc });
                    out.push((unsafe { (*c.0).car }, loc));
                    cur = unsafe { (*c.0).cdr };
                }
                _ => return Err(Error::ImproperList),
            }
        }
    }

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
            // A built-in function value's `name` is already permanently
            // rooted (`intern_string`) and its `recv_type` is a permanent
            // `PathId`, so nothing here can be swept — tracing the name
            // anyway keeps "a live box keeps its own constituents live" true
            // from the mark phase alone, the same belt-and-suspenders the
            // `Map` arm below applies to its interned string keys.
            BoxedObj::Builtin { name, .. } => stack.push(Value::Str(*name)),
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
            BoxedObj::Struct { payload: StructPayload::Map(map), .. } => {
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
            // Two filters, both needed. `mask` says the slot holds a *tagged*
            // word at all — an unmasked slot is a raw `i32`/`f64` bit pattern
            // whose low three bits are not a tag, and reading one as a tag
            // would invent a heap reference out of arithmetic. The tag test
            // then says whether that tagged word points at anything: a
            // `Sexpr`-repr local legitimately holds fixnums and immediates.
            BoxedObj::Frame { words, mask, .. } => {
                for (i, &w) in words.iter().enumerate() {
                    if mask[i / 64] & (1u64 << (i % 64)) != 0 && crate::tagged::references_heap(w) {
                        stack.push(crate::tagged::decode(w));
                    }
                }
            }
        }
    }

    /// Run a mark-sweep collection. Returns the number of cons cells reclaimed.
    /// Debug-only (runs under `gc_stress`): report any root that points at a
    /// cell already on the free list. Such a root makes the mark phase mark a
    /// cell the sweep then refuses to hand back, which is what drives `gc`'s
    /// reclaim count below zero. Panics naming the root set and index — the
    /// fact the underflow itself never reveals.
    fn audit_roots_against_free_list(&self) {
        use std::collections::HashSet;
        let mut freed: HashSet<usize> = HashSet::new();
        let mut p = self.free;
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
        for (s, stack) in self.root_stacks.iter().enumerate() {
            let Some(stack) = stack else { continue };
            let what = if s == self.current_stack { "roots".to_string() } else { format!("roots[task {}]", s) };
            for (i, &v) in stack.iter().enumerate() {
                check(&what, i, v);
            }
        }
        for (i, &v) in self.permanent_roots.iter().enumerate() {
            check("permanent_roots", i, v);
        }
        for (i, &v) in self.session_roots.iter().enumerate() {
            check("session_roots", i, v);
        }
        if let Some(v) = self.in_flight_throw {
            check("in_flight_throw", 0, v);
        }
        for w in &self.cell_registry {
            if let Some(id) = w.upgrade() {
                if let Some(BoxedObj::Cell(v)) = &self.box_slots[id.0 as usize] {
                    check("cell_registry", id.0 as usize, *v);
                }
            }
        }
    }

    pub fn gc(&mut self) -> usize {
        self.gc_count += 1;
        if false {
            self.audit_roots_against_free_list();
        }
        // reset string/box marks
        for m in self.str_marks.iter_mut() {
            *m = false;
        }
        for m in self.box_marks.iter_mut() {
            *m = false;
        }

        // MARK: iterative DFS from the roots (no native recursion). Two
        // independent root sets feed the same walk — `roots` (the strict
        // LIFO call-stack discipline) and `permanent_roots` (never popped,
        // see `push_permanent_root`) — a value is live if either reaches it.
        // The worklist holds `Value`s directly (not just `*mut Cell`) so a
        // `Value::Boxed` payload's own nested `Value`s can be pushed onto
        // the very same stack once traced — see `push_boxed_nested`.
        let mut stack: Vec<Value> = Vec::new();
        // Every task's roots, not just the running one: a suspended task will
        // resume into the frames these belong to.
        for roots in self.root_stacks.iter().flatten() {
            for &v in roots {
                stack.push(v);
            }
        }
        for i in 0..self.permanent_roots.len() {
            stack.push(self.permanent_roots[i]);
        }
        for i in 0..self.session_roots.len() {
            stack.push(self.session_roots[i]);
        }
        // The value a throw is carrying past the frames being discarded — see
        // `set_in_flight_throw` for why it cannot live in any of the three
        // stacks above.
        if let Some(v) = self.in_flight_throw {
            stack.push(v);
        }
        // Every binding cell still referenced by a live `Rc<BoxId>` handle
        // is a root of its own — see `alloc_cell`. Dead entries (the last
        // handle dropped) are pruned here; their cells become collectible
        // like any other unreachable box.
        self.cell_registry.retain(|w| w.upgrade().is_some());
        for w in &self.cell_registry {
            if let Some(id) = w.upgrade() {
                stack.push(Value::Boxed(*id));
            }
        }
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
                    self.str_marks[s.0 as usize] = true;
                }
                Value::Boxed(b) => {
                    let idx = b.0 as usize;
                    if self.box_marks[idx] {
                        continue;
                    }
                    self.box_marks[idx] = true;
                    if let Some(obj) = &self.box_slots[idx] {
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
        let old_free = self.free_count;
        let mut new_free: *mut Cell = ptr::null_mut();
        let mut new_free_count = 0usize;
        for chunk in self.chunks.iter().rev() {
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
        self.free = new_free;
        self.free_count = new_free_count;

        // SWEEP strings: free unmarked occupied slots, recycling indices.
        for i in 0..self.str_slots.len() {
            if self.str_slots[i].is_some() && !self.str_marks[i] {
                self.str_slots[i] = None;
                self.str_free.push(i as u32);
            }
        }

        // SWEEP boxed objects: same recycling scheme as strings.
        for i in 0..self.box_slots.len() {
            if self.box_slots[i].is_some() && !self.box_marks[i] {
                self.box_slots[i] = None;
                self.box_free.push(i as u32);
            }
        }

        new_free_count - old_free
    }
}

/// A scoped guard over the LIFO root stack: it records the root count on entry
/// and truncates back to it on drop, however the scope is left.
///
/// Hand-balanced `push_root`/`pop_root` pairs are the standard way to keep an
/// intermediate alive across an allocation that might collect, but they are
/// fragile at scale — one builder in the translator this replaced balanced
/// *sixteen* pops by hand, and every `?` early return is a chance to skip
/// them. This
/// guard makes the unwind automatic, so an error path cannot leave the stack
/// unbalanced and a later "pop back to my own mark" caller cannot pop
/// somebody else's roots.
///
/// Derefs to the [`Heap`], so a scope is used exactly like the heap it wraps:
///
/// ```ignore
/// let node = {
///     let mut s = RootScope::new(heap);
///     let a = s.cons(x, Value::Empty)?;
///     s.push_root(a);              // popped automatically below
///     s.cons(head, a)?
/// };                               // roots truncated here
/// heap.push_root(node);            // caller roots the result before allocating again
/// ```
///
/// The returned value is *not* rooted when the scope ends — truncating the
/// stack frees nothing by itself, but the next allocation may collect, so the
/// caller must root a result it intends to keep before allocating again. That
/// is the same contract every builder in this codebase already follows.
pub struct RootScope<'h> {
    heap: &'h mut Heap,
    base: usize,
}

impl<'h> RootScope<'h> {
    /// Open a scope over `heap`'s current root stack.
    pub fn new(heap: &'h mut Heap) -> RootScope<'h> {
        let base = heap.root_count();
        RootScope { heap, base }
    }

    /// The root count this scope will truncate back to.
    pub fn base(&self) -> usize {
        self.base
    }
}

impl std::ops::Deref for RootScope<'_> {
    type Target = Heap;
    fn deref(&self) -> &Heap {
        self.heap
    }
}

impl std::ops::DerefMut for RootScope<'_> {
    fn deref_mut(&mut self) -> &mut Heap {
        self.heap
    }
}

impl Drop for RootScope<'_> {
    fn drop(&mut self) {
        self.heap.truncate_roots(self.base);
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        for chunk in self.chunks.drain(..) {
            if chunk.base.is_null() {
                continue;
            }
            // Reconstitute each chunk's owning Box and drop it, freeing it.
            unsafe {
                let raw = ptr::slice_from_raw_parts_mut(chunk.base, chunk.len);
                drop(Box::from_raw(raw));
            }
        }
    }
}

/// A type key's base name, without its instantiation: `option<char>` ->
/// `option`, `point` -> `point`.
///
/// Lives here, below both tiers, because both of them look a *type* up by a
/// key a *value* carries. Registration is per type (an enum's variant names,
/// its definition); a key names an instantiation. Splitting in only one tier
/// is how `(some 1)` came out as `(<unknown-variant> 1)` in the interpreter
/// while the AOT printer had the same gap the other way round.
pub fn base_type_key(key: &str) -> &str {
    match key.find('<') {
        Some(i) if key.ends_with('>') => &key[..i],
        _ => key,
    }
}

/// The key one level inside `key`: `vector<cons-cell<i32,string>>` ->
/// `cons-cell<i32,string>`, `option<char>` -> `char`.
///
/// The inverse of a type key's `Name<args>` spelling for a *single* argument.
/// A type's runtime identity includes its instantiation, and two callers build
/// an inner box out of an outer box's key: `HashTable::entries` (interpreted
/// and compiled both) builds `cons-cell`s for a `Vector<cons-cell<K,V>>`.
/// They must agree on the spelling, so the unwrapping lives here — below both
/// — rather than once in each.
///
/// `None` when the key names no instantiation, or names more than one
/// argument: "the first of several" is not a question any caller is asking.
pub fn inner_type_key(key: &str) -> Option<&str> {
    let open = key.find('<')?;
    let inner = key[open + 1..].strip_suffix('>')?;
    let mut depth = 0i32;
    for c in inner.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => return None,
            _ => {}
        }
    }
    Some(inner)
}
