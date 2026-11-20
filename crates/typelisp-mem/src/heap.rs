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
//! * **Growth is bounded.** The arena grows by appending chunks up to
//!   [`GROWTH_FACTOR`] times its initial capacity; past that ceiling, a GC
//!   that frees nothing makes allocation return [`Error::HeapExhausted`],
//!   which is what turns a runaway leak into a loud error rather than an
//!   out-of-memory kill. [`Heap::set_growth_limit`] moves the ceiling, and
//!   `set_growth_limit(0)` fixes the arena at the capacity it has.
//! * **Symbols** are interned (name -> [`SymRef`]) and never collected — they are
//!   few and live for the heap's lifetime, like CL symbols in a package.
//! * **Strings** live in a slot store and ARE collected: the mark phase marks
//!   every reachable [`StrId`], the sweep frees unmarked slots (recycling
//!   indices). So unreachable strings do not leak. Boxed objects (`BoxedObj`)
//!   use the same growable-slot-store shape. Both stores are a [`Slab`], not
//!   a plain `Vec<Option<_>>`: growth appends a chunk rather than
//!   reallocating everything, so an already-issued slot's address never
//!   moves — see `Slab`'s doc comment.
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
//!   the bracket's end (see [`Heap::push_session_root`]); one set per view,
//!   kept where a collector on another view can reach it. The fourth is not a
//!   stack of its own but one slot *inside each task's `RootStack`*: the value
//!   a `throw` is carrying while the frames that held it are being discarded
//!   (see [`Heap::set_in_flight_throw`]). It lives there rather than on `Heap`
//!   itself so that a task whose `unwind-protect` cleanup suspends (a `sleep`,
//!   a channel op) mid-flight keeps its own parked value untouched while a
//!   *different* task's `throw` runs on the same thread in between — the two
//!   tasks' `RootStack`s are different objects, so they cannot collide.
//!   `gc()` marks from all four.
//!
//! * **Stop-the-world across threads.** Several OS threads can each hold a
//!   view ([`Heap::attach`]) onto one shared heap. A collection, started from
//!   any of them, first waits until every other thread is stopped — at a
//!   safepoint ([`Heap::cons`], [`Heap::safepoint`]) or inside a native
//!   section ([`Heap::native`]) — see [`Heap::gc`].
//!
//! All `unsafe` is confined here; the public API is safe.

use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use parking_lot::RwLock;

use super::symbols::{self, SymRef};
use super::value::{BoxId, BoxedObj, Cell, ConsRef, LocId, Value};

mod frames;
mod slab;
mod gc;
mod roots;
mod objects;
mod hashtable;
mod scope;
mod cons;

pub use frames::*;
use slab::*;
use gc::*;

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

/// One task's GC root stack, heap-allocated as its own object (`Box`) rather
/// than stored inline in `root_stacks`'s `Vec`. The indirection is what lets
/// [`Heap::current`] cache a raw pointer straight at the running stack: a
/// `Box`'s target address is stable even while the outer `Vec<Option<Box<_>>>`
/// grows and reallocates (only the pointers move, never what they point at),
/// so `push_root`/`pop_root` — the hottest path in the whole heap — never
/// have to re-index `root_stacks[current_stack]`. The same stability is what
/// will later let a per-thread `Heap` *view* hold this pointer directly while
/// the stack itself lives in `HeapShared`'s locked registry — see
/// `docs/dev/os-threads-design.md` §3.
#[derive(Default)]
struct RootStack {
    roots: Vec<Value>,
    /// The value a `throw` currently travelling through *this task's* frames
    /// is carrying — see [`Heap::set_in_flight_throw`]. Per stack rather than
    /// a single field on `Heap`, so that a different task running on the same
    /// thread while this one is suspended mid-unwind (a cleanup that
    /// `sleep`s) cannot overwrite it: `Heap::current` points at whichever
    /// stack is running, and each stack carries its own copy.
    in_flight_throw: Option<Value>,
    /// The task's compiled frames. See [`FrameArena`].
    frames: FrameArena,
}


/// The cons arena, as one or more chunks. Always at least one entry (a
/// zero-capacity heap holds a single zero-length chunk). Never reordered:
/// `gc`'s sweep walks it back to front so the rebuilt free list keeps
/// handing out low addresses first, exactly as the single-chunk version did.
///
/// Its own type, behind [`HeapShared`]'s `Mutex`, rather than fields inline
/// on `Heap` — this is the first piece of `docs/dev/os-threads-design.md`
/// §3's split to actually move: `cons` is the hottest allocation path in the
/// heap, so it is where the "one `Arc<HeapShared>`, many per-thread views"
/// shape has to prove itself first. Everything else `Heap` owns (the string
/// and box slot stores, `root_stacks`, the interning tables) stays exactly
/// where it was for now — this is a deliberately narrow slice of the split,
/// not the whole thing; see the struct's own fields for what has *not* moved
/// yet.
struct Arena {
    chunks: Vec<Chunk>,
    cap: usize, // sum of every chunk's `len`
    // Ceiling on `cap` for growth, in cells. At or below `cap` (`0`, say) the
    // arena is fixed and exhaustion is an error — see the module doc comment.
    growth_limit: usize,
    free: *mut Cell, // head of the free list (null when empty)
    free_count: usize,
}

// `Chunk`/`*mut Cell` make `Arena` `!Send` by Rust's default auto-trait
// derivation, the same as `ConsRef` before its own `unsafe impl Send`
// (`crates/typelisp-mem/src/value.rs`) — a raw pointer into heap memory that
// is safe to move between threads because every access to it goes through
// `HeapShared::arena`'s `Mutex`, which is exactly what makes `Mutex<Arena>`
// itself `Send + Sync` once `Arena: Send` (a `Mutex`'s own `Sync` impl needs
// only its contents to be `Send`, not `Sync` — the lock supplies the
// exclusion). Not `Sync` for `Arena` itself: nothing reads it without the
// lock, so it does not need to be.
unsafe impl Send for Arena {}

impl Drop for Arena {
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

/// Permanent, append-only identity tables — see `HeapShared::interning`.
/// Grouped into one struct (rather than one lock per field) so that
/// `HeapShared` does not grow a lock per table.
#[derive(Default)]
struct Interning {
    // interned `::` paths (permanent; reference only permanent symbols)
    paths: Vec<Vec<SymRef>>,
    path_ids: HashMap<Vec<SymRef>, u32>,

    // Reader macros: the function each macro character dispatches to, and for
    // a dispatching character (`#`-like) the function each sub-character
    // dispatches to. See `Heap::set_macro_character`.
    //
    // Here rather than in a thread-local of the reader's, for two reasons
    // that point the same way. A stored function is a heap value, so it only
    // means anything alongside the heap it came from — a table outliving the
    // heap would hand the next session cells belonging to nobody. And the
    // values need rooting: they are pushed as *permanent* roots when
    // registered, which is a thing only a `Heap` can do. Shared rather than
    // per-view because a `read` in a task on another thread must see the
    // readtable the program set up.
    macro_chars: HashMap<char, Value>,
    dispatch_chars: HashMap<(char, char), Value>,

    // Every distinct source span any live cell refers to, indexed by
    // `LocId` — see `Cell`'s doc comment for why a cell holds an index rather
    // than the `Loc` itself, and why the location lives in the cell at all.
    // Shared because the cell is: a `LocId` written through one view has to
    // resolve through every other.
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

/// A GC-collected slot store: content, free list, and mark bits. Grouped
/// under one lock rather than one apiece because allocation and `gc`'s
/// sweep touch all three together on every call — splitting them would only
/// add lock traffic, not concurrency, since nothing needs one without the
/// others.
struct SlotStore<T> {
    slots: Slab<T>,
    free: Vec<u32>,
    marks: Vec<bool>,
}

// Written by hand for the same reason as `Slab`'s: a derive would demand
// `T: Default`.
impl<T> Default for SlotStore<T> {
    fn default() -> SlotStore<T> {
        SlotStore { slots: Slab::new(), free: Vec::new(), marks: Vec::new() }
    }
}

impl<T> SlotStore<T> {
    /// Stores `item`, reusing a freed slot when there is one, and returns its
    /// index. The slot starts unmarked.
    fn alloc(&mut self, item: T) -> u32 {
        if let Some(idx) = self.free.pop() {
            self.slots[idx as usize] = Some(item);
            self.marks[idx as usize] = false;
            idx
        } else {
            let idx = self.slots.len() as u32;
            self.slots.push(Some(item));
            self.marks.push(false);
            idx
        }
    }

    fn occupied(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    fn clear_marks(&mut self) {
        for m in self.marks.iter_mut() {
            *m = false;
        }
    }

    /// Frees every occupied slot the mark phase did not reach, recycling its
    /// index.
    fn sweep(&mut self) {
        for i in 0..self.slots.len() {
            if self.slots[i].is_some() && !self.marks[i] {
                self.slots[i] = None;
                self.free.push(i as u32);
            }
        }
    }
}

/// The string store.
#[derive(Default)]
struct StrStore {
    store: SlotStore<String>,
}


/// State one `Heap` view shares with every other view attached to the same
/// underlying heap (`docs/dev/os-threads-design.md` §3): the cons arena,
/// every task's root stack, `permanent_roots`, the interning tables
/// (`type_keys`/`paths`), the string and box stores, and the registry of
/// views a collection stops (`threads`, see [`Heap::gc`]).
/// `Heap::with_capacity` creates it with the first view;
/// [`Heap::attach`] adds more, one per thread that runs on it.
pub struct HeapShared {
    arena: Mutex<Arena>,
    // Every task's roots, keyed by `RootStackId`. Shared (not per-view)
    // because a task's stack must stay walkable by whichever view ends up
    // driving a collection, regardless of which view created the task — see
    // `RootStack`'s doc comment. `current`, the *running* stack, is cached
    // per-view below instead of read through this lock on every push/pop.
    root_stacks: Mutex<Vec<Option<Box<RootStack>>>>,
    // Always `Some` once written — `Slab` rather than a plain `Vec<Value>`
    // for the address stability described on `Slab`'s doc comment, not
    // because any entry is ever removed (`push_permanent_root` never
    // provides a way to). Shared because a `defvar` global (what this
    // backs — see `push_permanent_root`) must read the same way from every
    // thread that names it.
    permanent_roots: Mutex<Slab<Value>>,
    // `parking_lot::RwLock`, not `std::sync::RwLock`: `path_segments`
    // returns a reference borrowed from inside this lock, and
    // only `parking_lot`'s guards can be `.map()`ped to shrink that
    // reference down from "the whole table" to "the one entry" while still
    // keeping the lock held for exactly the returned guard's lifetime.
    //
    // **Every read of this and the two locks below is `read_recursive`.**
    // Those guards are handed out by `&self` methods, so one thread can hold
    // two at once (`rt_str_eq` compares two strings through two
    // `Heap::string` guards). A plain `read` queues behind a waiting writer,
    // and the writer waits for the first guard: another thread's
    // `alloc_string` between the two reads deadlocked both. A thread cannot
    // hold a guard and then write — writing takes `&mut self` — so recursive
    // reads are the whole of what one thread can do to itself here.
    interning: RwLock<Interning>,
    // What names mean in this session — `Heap::names`. A lock of its own:
    // the reader consults it per path, which must not wait on the arena.
    // Lock order: this one, then the symbol table's (`SessionNames`' methods
    // take that one while this is held), never the other way round.
    names: parking_lot::Mutex<symbols::SessionNames>,
    // Same `parking_lot::RwLock`-for-mapped-guards reasoning as `interning`,
    // for `Heap::string`'s return.
    strings: RwLock<StrStore>,
    // Same reasoning again, for `Heap::bignum_value`/`ratio_value`'s return
    // and `bucket`'s (`Heap::hashtable_bucket_count` and friends).
    boxes: RwLock<SlotStore<BoxedObj>>,
    // Liveness registry for `BoxedObj::Cell`s (see `alloc_cell`): a cell is
    // an *implicit* GC root for exactly as long as some binding holds the
    // `Arc<BoxId>` handed out at allocation. Registered here (weakly) rather
    // than through the caller-managed `roots`/`permanent_roots` stacks
    // because a binding's lifetime follows Rust scopes, not this heap's
    // strict LIFO root discipline — and, crucially, `gc()` reads this
    // registry *itself*, so a collection triggered from anywhere (including
    // compiled code, or a different thread's view once more than one
    // exists) can never sweep a live binding cell. `std::sync::Mutex`
    // rather than `parking_lot`'s: nothing here ever needs a borrowed
    // return, only the plain `Vec` operations `alloc_cell`/`gc` already use
    // (`retain`/`push`/iteration), the same reasoning as `root_stacks`/
    // `permanent_roots`.
    cell_registry: Mutex<Vec<std::sync::Weak<BoxId>>>,
    // Every view, and the stop-the-world flag — see `Heap::gc`.
    threads: ThreadRegistry,
    // How many collections have run, for tests that assert an allocation
    // pattern does not thrash — see `gc_count`. Shared, because a
    // collection run from any view collects the whole heap.
    gc_count: AtomicU64,
    // When set, every `cons` collects first — see `set_gc_stress`. Shared
    // so that a view attached after it was switched on is under stress too.
    gc_stress: AtomicBool,
}

// Every field is behind a lock or an atomic, so the auto traits fail only
// because what those locks guard contains `Value`s (`BoxedObj`'s fields,
// `root_stacks`), and `Value` is `Send` but not `Sync`: a `ConsRef` is a raw
// pointer into the arena. Sharing `&HeapShared` hands another thread no
// more than the ability to take those locks. The cells a `Value` points at
// are the part no lock covers, and they are covered instead by the two
// rules of `docs/dev/os-threads-design.md`: a collection runs only with
// every other thread stopped (`Heap::gc`), and two threads that race on the
// same cell without a `Mutex<T>`/`Chan<T>` between them have written a
// program with undefined behaviour — the Go position (§6).
unsafe impl Send for HeapShared {}
unsafe impl Sync for HeapShared {}

pub struct Heap {
    shared: Arc<HeapShared>,
    // This view's entry in `shared.threads` — its stop-the-world state and
    // its session roots.
    slot: Arc<ViewSlot>,
    // The root stack this view was created with, freed when the view is
    // dropped — see `Drop for Heap`.
    home_stack: usize,
    // Which of `shared.root_stacks`' entries this view is currently running.
    current_stack: usize,
    // A raw pointer at `*shared.root_stacks[current_stack]`, kept in sync by
    // every place that changes either — `with_capacity` and
    // `switch_to_root_stack` — so `roots`/`roots_mut` can dereference it
    // directly instead of taking `shared.root_stacks`'s lock on every
    // push/pop. Never null: "the running root stack is always present" is
    // the same invariant `roots()`'s `.expect` used to state. See
    // `RootStack`'s doc comment for why the `Box` indirection inside
    // `shared.root_stacks` keeps this valid across that `Vec` reallocating —
    // the same reasoning applies whether the `Vec` sits behind a lock or not.
    current: *mut RootStack,
    // Symbols are *not* here: they live in one process-global, permanent
    // table (`super::symbols`), so a symbol means the same thing in every
    // heap and compiled code can reach one without an active `Heap` at all.
    // Type identities, `::` paths, reader macros and source locations are
    // not here either: they are `shared.interning`'s, so every view reads
    // the same ones. Nor are the string and box stores or the binding-cell
    // registry — see `HeapShared`.
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
    /// The largest initial capacity [`Self::with_capacity`] can be asked
    /// for: the one chunk it allocates is a single `[Cell]`, and a Rust
    /// allocation is at most `isize::MAX` bytes. A request past this is not
    /// memory the machine lacks but an arena no address space can hold, so a
    /// driver can refuse it by name instead of letting the allocation panic.
    pub const fn max_capacity() -> usize {
        isize::MAX as usize / std::mem::size_of::<Cell>()
    }

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
        assert!(capacity <= Self::max_capacity(), "cons arena of {} cells exceeds the address space", capacity);
        let (chunk, free) = Self::alloc_chunk(capacity, ptr::null_mut());
        let arena = Arena { chunks: vec![chunk], cap: capacity, growth_limit: capacity.saturating_mul(GROWTH_FACTOR), free, free_count: capacity };

        let mut home = Box::new(RootStack::default());
        let current: *mut RootStack = &mut *home;
        let shared = Arc::new(HeapShared {
            arena: Mutex::new(arena),
            root_stacks: Mutex::new(vec![Some(home)]),
            permanent_roots: Mutex::new(Slab::new()),
            interning: RwLock::new(Interning::default()),
            names: parking_lot::Mutex::new(symbols::SessionNames::new()),
            strings: RwLock::new(StrStore::default()),
            boxes: RwLock::new(SlotStore::default()),
            cell_registry: Mutex::new(Vec::new()),
            threads: ThreadRegistry::new(),
            gc_count: AtomicU64::new(0),
            gc_stress: AtomicBool::new(false),
        });

        let slot = shared.threads.register();
        Heap {
            shared,
            slot,
            home_stack: 0,
            current_stack: 0,
            current,
        }
    }

    /// A second (or third, ...) view onto the same underlying heap —
    /// `docs/dev/os-threads-design.md` §1's "one `Arc<HeapShared>`, many
    /// views" split. Gives the new view its own home root stack (registered
    /// in `shared.root_stacks`, so `gc()` run from *any* view walks it, and
    /// starting with an empty `in_flight_throw` of its own like any other
    /// fresh `RootStack`) and otherwise starts it exactly like
    /// [`with_capacity`](Self::with_capacity) starts the first one — empty
    /// `session_roots`; the interned paths, the reader macro tables and the
    /// source locations are already in `shared.interning`.
    ///
    /// The view is registered in `shared`'s thread registry for as long as
    /// it lives, as running on the calling thread. From then on a collection
    /// started from any *other* thread waits for this one to stop, so a
    /// thread holding a view must keep reaching a stopping point: an
    /// allocation ([`cons`](Self::cons)), a [`safepoint`](Self::safepoint),
    /// or — around anything that blocks — [`native`](Self::native). See
    /// [`gc`](Self::gc).
    ///
    /// There is no per-view allocation cache: `cons`, `alloc_string` and
    /// `alloc_boxed` lock straight through to `shared` on every call.
    pub fn attach(shared: &Arc<HeapShared>) -> Heap {
        let mut heap = Heap {
            shared: Arc::clone(shared),
            slot: shared.threads.register(),
            home_stack: 0,
            current_stack: 0,
            current: ptr::null_mut(),
        };
        let home = heap.new_root_stack();
        heap.home_stack = home.0;
        heap.switch_to_root_stack(home);
        heap
    }

    /// A handle to this view's underlying shared heap, for
    /// [`attach`](Self::attach)ing another view onto it — the shape a worker
    /// thread's spawn closure needs: clone this *before* spawning (on the
    /// spawning view), then call `attach` *inside* the new thread, since
    /// `Heap` itself is not `Send` yet (only `HeapShared` is meant to cross
    /// threads — see `docs/dev/os-threads-design.md` §1).
    pub fn shared_handle(&self) -> Arc<HeapShared> {
        Arc::clone(&self.shared)
    }

    // -- Source locations (see `Cell`'s doc comment) -------------------------

    /// Intern `loc`, returning the id a cell stores.
    fn intern_loc(&mut self, loc: crate::errors::Loc) -> LocId {
        let mut interning = self.shared.interning.write();
        if let Some(id) = interning.loc_ids.get(&loc) {
            return *id;
        }
        // Ids are 1-based so that `LocId::NONE` is the all-zero pattern a blank
        // cell already has, with no dummy entry to keep in step.
        interning.locs.push(loc.clone());
        let id = LocId(interning.locs.len() as u32);
        interning.loc_ids.insert(loc, id);
        id
    }

    /// The `Loc` behind an id, or `None` for [`LocId::NONE`].
    fn loc_of(&self, id: LocId) -> Option<crate::errors::Loc> {
        if id.is_none() {
            return None;
        }
        self.shared.interning.read_recursive().locs.get(id.0 as usize - 1).cloned()
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
        self.shared.interning.read_recursive().locs.len()
    }

    // ---- statistics -------------------------------------------------------

    /// Total number of cons cells in the arena.
    pub fn capacity(&self) -> usize {
        self.shared.arena.lock().unwrap().cap
    }

    /// Cons cells currently on the free list (available to allocate).
    pub fn free_count(&self) -> usize {
        self.shared.arena.lock().unwrap().free_count
    }

    /// Cons cells currently in use (`capacity - free_count`).
    pub fn live_count(&self) -> usize {
        let arena = self.shared.arena.lock().unwrap();
        arena.cap - arena.free_count
    }

    /// How many collections have run over this heap's lifetime. A ratio, not
    /// an absolute: what it is for is asserting that a workload whose live set
    /// sits close to capacity collects a handful of times rather than once per
    /// allocation (see [`Heap::cons`]).
    pub fn gc_count(&self) -> u64 {
        self.shared.gc_count.load(Ordering::Relaxed)
    }

    /// Distinct interned symbols.
    pub fn symbol_count(&self) -> usize {
        symbols::symbol_count()
    }

    /// Strings currently allocated (occupied slots).
    pub fn string_count(&self) -> usize {
        self.shared.strings.read_recursive().store.occupied()
    }

    /// Boxed objects currently allocated (occupied slots) — see [`BoxedObj`].
    pub fn box_count(&self) -> usize {
        self.shared.boxes.read_recursive().occupied()
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

/// Dropping a view frees its home root stack and takes it out of the
/// thread registry — without the second, the next collection from another
/// thread would wait forever for a view nobody will ever run again.
///
/// Freeing the arena's chunks is `Arena`'s own `Drop` (see its doc
/// comment), which runs when the last view sharing this `Arc<HeapShared>`
/// goes away — not when any one view is dropped, since a view does not own
/// the arena outright.
impl Drop for Heap {
    fn drop(&mut self) {
        // Before unregistering: until then this view counts as running, so
        // no collection on another thread can be walking `root_stacks`
        // while this edits it. The home stack is only ever pushed to by
        // this view, so nothing else can still need what it holds.
        if self.current_stack == self.home_stack {
            // The view is going away; `current` must not point at a freed
            // stack in the meantime, and nothing reads it after this.
            self.current = ptr::null_mut();
        }
        if let Ok(mut root_stacks) = self.shared.root_stacks.lock() {
            if let Some(s) = root_stacks.get_mut(self.home_stack) {
                *s = None;
            }
        }
        // A view dropped while native (a panic unwinding out of `native`'s
        // closure) is not running either; unregistering is all it needs.
        self.shared.threads.unregister(&self.slot);
    }
}

thread_local! {
    // The `SessionNames` mutexes this thread holds, by address (`Heap::names`).
    static NAMES_HELD: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// [`Heap::names`]'s guard: the session's names, locked by this thread.
pub struct NamesGuard<'a> {
    guard: parking_lot::MutexGuard<'a, symbols::SessionNames>,
    at: usize,
}

impl std::ops::Deref for NamesGuard<'_> {
    type Target = symbols::SessionNames;
    fn deref(&self) -> &symbols::SessionNames {
        &self.guard
    }
}

impl std::ops::DerefMut for NamesGuard<'_> {
    fn deref_mut(&mut self) -> &mut symbols::SessionNames {
        &mut self.guard
    }
}

impl Drop for NamesGuard<'_> {
    fn drop(&mut self) {
        NAMES_HELD.with(|held| {
            let mut held = held.borrow_mut();
            let i = held.iter().rposition(|a| *a == self.at).expect("a held guard is recorded");
            held.remove(i);
        });
    }
}

