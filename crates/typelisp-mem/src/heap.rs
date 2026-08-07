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
//! * **Symbols** are interned (name -> [`SymId`]) and never collected — they are
//!   few and live for the heap's lifetime, like CL symbols in a package.
//! * **Strings** live in a slot store and ARE collected: the mark phase marks
//!   every reachable [`StrId`], the sweep frees unmarked slots (recycling
//!   indices). So unreachable strings do not leak.
//! * **Mark-sweep.** `gc()` marks everything reachable from the root set
//!   (iteratively — no native recursion), then rebuilds the cons free list and
//!   sweeps strings. Cycles are reclaimed (unlike reference counting).
//! * **Three root sets.** `roots` is a strict LIFO stack (push on scope entry,
//!   pop on scope exit). `permanent_roots` holds values whose owner outlives
//!   any single activation (e.g. a field inside a heap-external, never-freed
//!   box) — appended to, never popped. `session_roots` holds values that must
//!   survive a bracketed span of work but not outlive it, released in bulk at
//!   the bracket's end (see [`Heap::push_session_root`]). `gc()` marks from all
//!   three.
//!
//! All `unsafe` is confined here; the public API is safe.

use std::collections::HashMap;
use std::ptr;

use crate::Error;
use super::value::{BoxId, BoxedObj, Cell, ConsRef, MemHashKey, PathId, StrId, StructPayload, SymId, Value};

/// One owned run of cons cells. Chunks are only ever appended (see
/// [`Heap::set_growth_limit`]) and never moved or freed individually, which is
/// what lets a raw `*mut Cell` stay valid for the heap's whole lifetime.
struct Chunk {
    base: *mut Cell,
    len: usize,
}

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
    roots: Vec<Value>,
    permanent_roots: Vec<Value>,
    // Roots for the duration of a bracketed session — see `push_session_root`.
    session_roots: Vec<Value>,

    // interned symbols (permanent)
    sym_names: Vec<String>,
    sym_ids: HashMap<String, u32>,

    // Monotonic counter backing `gensym`. Lives on the `Heap` — not the
    // `Interp` — so both the interpreter's `gensym` builtin and the compiled
    // `rt_gensym` shim (which only ever sees this shared, thread-registered
    // heap) draw from *one* sequence: a macro expansion mixing interpreted
    // `gensym` calls (a macro body's own, e.g. `case`'s `tmp`) with compiled
    // ones (a JIT'd expansion-lambda's, e.g. `do`'s per-binding temporaries)
    // must never mint the same " gensym-N" symbol twice, or two "fresh"
    // symbols would be `eq`-identical (symbols are interned/permanent) and
    // hygiene would break.
    gensym_counter: u64,

    // interned `::` paths (permanent; reference only permanent symbols)
    paths: Vec<Vec<SymId>>,
    path_ids: HashMap<Vec<SymId>, u32>,

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

    // Source *spans* (opening `(` through closing `)`) of cons cells produced
    // by the reader, keyed by the cell's raw address (`ConsRef::addr`), so an
    // error about a form can be reported with the `file:line:col` it was read
    // from, and the LSP can underline its full extent. This is a pure
    // side table: it never keeps a cell alive (the mark phase ignores it) and
    // a stale entry (a freed cell's address later reused) is harmless because
    // the reader overwrites it on every fresh allocation and
    // `clear_cons_locs` wipes it at the start of each read batch. Cleared —
    // not GC-swept — because the forms it describes stay rooted for the whole
    // read→check cycle that consults it.
    cons_locs: HashMap<usize, crate::errors::Loc>,

    // Source spans of *each list element*, keyed by the spine cons cell
    // whose `car` holds that element (`ConsRef::addr`). Where `cons_locs`
    // records a list form's own span (head cell), this records the span of
    // every element — including a bare atom, which has no per-occurrence
    // identity of its own (interned symbols are shared) — begins. Same pure
    // side-table discipline as `cons_locs`: never keeps a cell alive, cleared
    // by `clear_cons_locs`, harmless if a freed cell's address is later reused
    // (the reader overwrites on every fresh allocation). Consumed by the
    // checker (`list_to_vec_locs`) so an atom node can carry its own `Loc`,
    // which the LSP's hover/goto-definition need (`src/check/locate.rs`).
    elem_locs: HashMap<usize, crate::errors::Loc>,

    // Source spans of *lowered code* nodes, keyed by the node's own head cons
    // cell (`ConsRef::addr`). The checker records one here as it builds each
    // node, and the interpreter reads it back to place a runtime error at the
    // source position the node came from.
    //
    // Distinct from `cons_locs`/`elem_locs` above in both lifetime and upkeep,
    // which is why it is a third table rather than an extra entry in either:
    //
    // * Those two describe cells the *reader* just produced and are bulk-cleared
    //   by `clear_cons_locs` at the start of every read batch. Lowered code
    //   outlives its read batch — it is what a registered function body *is* —
    //   so an entry here must survive that clear.
    // * Surviving the clear means a stale entry can no longer be shrugged off.
    //   Those tables tolerate a freed cell's address being reused because the
    //   reader overwrites the entry on every fresh allocation; nothing
    //   overwrites an entry here, so `gc`'s sweep removes the entry for each
    //   cell it reclaims (see the SWEEP cons loop).
    //
    // Still a pure side table: an entry never keeps a cell alive.
    code_locs: HashMap<usize, crate::errors::Loc>,
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

    /// Create a heap with `capacity` cons cells pre-allocated. The arena is
    /// fixed at that size unless [`Heap::set_growth_limit`] permits otherwise.
    pub fn with_capacity(capacity: usize) -> Heap {
        let (chunk, free) = Self::alloc_chunk(capacity, ptr::null_mut());

        Heap {
            chunks: vec![chunk],
            cap: capacity,
            growth_limit: 0,
            free,
            free_count: capacity,
            gc_stress: false,
            roots: Vec::new(),
            permanent_roots: Vec::new(),
            session_roots: Vec::new(),
            sym_names: Vec::new(),
            sym_ids: HashMap::new(),
            gensym_counter: 0,
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
            cons_locs: HashMap::new(),
            elem_locs: HashMap::new(),
            code_locs: HashMap::new(),
        }
    }

    // -- Reader source-location side table (see the `cons_locs` field) -------

    /// Record the source location a cons cell was read from. The reader calls
    /// this for each list form's head cell; the checker/interpreter later
    /// look it up via [`Heap::cons_loc`] to place an error message.
    pub fn set_cons_loc(&mut self, cr: ConsRef, loc: crate::errors::Loc) {
        self.cons_locs.insert(cr.addr(), loc);
    }

    /// The source location recorded for `v` when it is a cons cell, if any.
    /// Non-cons values (atoms, symbols, ...) never carry their own location.
    pub fn cons_loc(&self, v: Value) -> Option<crate::errors::Loc> {
        match v {
            Value::Cons(cr) => self.cons_locs.get(&cr.addr()).cloned(),
            _ => None,
        }
    }

    /// Record where a list *element* began, keyed by the spine cell `cr` whose
    /// `car` holds it — see the `elem_locs` field doc comment. The reader calls
    /// this for every element as it builds a list; the checker later reads it
    /// back via [`Heap::list_to_vec_locs`].
    pub fn set_elem_loc(&mut self, cr: ConsRef, loc: crate::errors::Loc) {
        self.elem_locs.insert(cr.addr(), loc);
    }

    /// Drop every recorded source location (both the list-form `cons_locs` and
    /// the per-element `elem_locs`). The reader calls this at the start of each
    /// read batch so stale entries from an earlier batch can never mislabel a
    /// newly read form (see the `cons_locs` field doc comment).
    pub fn clear_cons_locs(&mut self) {
        self.cons_locs.clear();
        self.elem_locs.clear();
    }

    // -- Lowered-code source-location side table (see the `code_locs` field) --

    /// Record the source location a *lowered code* node came from, keyed by the
    /// node's own head cons cell. The checker calls this as it builds each node;
    /// the interpreter reads it back via [`Heap::code_loc`] to place a runtime
    /// error. Deliberately survives [`Heap::clear_cons_locs`] — see the
    /// `code_locs` field doc comment.
    pub fn set_code_loc(&mut self, cr: ConsRef, loc: crate::errors::Loc) {
        self.code_locs.insert(cr.addr(), loc);
    }

    /// The source location recorded for lowered node `v`, if any. Only a cons
    /// cell can carry one: a node is always a tagged list.
    pub fn code_loc(&self, v: Value) -> Option<crate::errors::Loc> {
        match v {
            Value::Cons(cr) => self.code_locs.get(&cr.addr()).cloned(),
            _ => None,
        }
    }

    /// How many lowered-code locations are currently recorded — for tests that
    /// check the sweep really does drop a reclaimed cell's entry.
    pub fn code_loc_count(&self) -> usize {
        self.code_locs.len()
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

    /// Distinct interned symbols.
    pub fn symbol_count(&self) -> usize {
        self.sym_names.len()
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

    /// Register `v` as a GC root.
    pub fn push_root(&mut self, v: Value) {
        if self.gc_stress {
            self.assert_not_freed("push_root", v);
        }
        self.roots.push(v);
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
        self.roots.pop()
    }

    /// Number of registered roots.
    pub fn root_count(&self) -> usize {
        self.roots.len()
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
        self.roots.truncate(len);
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
        self.roots[idx] = v;
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

    // ---- symbols ----------------------------------------------------------

    /// Intern a symbol by name, returning its `Value::Symbol`.
    ///
    /// typelisp symbols are **case-insensitive**, so names are folded to a
    /// canonical lowercase form: `Foo`, `FOO`, `foo` all intern to the same
    /// [`SymId`], and [`symbol_name`](Self::symbol_name) returns the canonical
    /// (lowercase) spelling.
    pub fn intern_symbol(&mut self, name: &str) -> Value {
        let key = name.to_lowercase();
        if let Some(&id) = self.sym_ids.get(&key) {
            return Value::Symbol(SymId(id));
        }
        let id = self.sym_names.len() as u32;
        self.sym_names.push(key.clone());
        self.sym_ids.insert(key, id);
        Value::Symbol(SymId(id))
    }

    /// The name of an interned symbol.
    pub fn symbol_name(&self, id: SymId) -> &str {
        &self.sym_names[id.0 as usize]
    }

    /// Mint a fresh, unforgeable symbol for `gensym`, returning its
    /// `Value::Symbol`. The name uses a leading space (` gensym-N`) the
    /// reader's tokenizer can never produce mid-token, so it can never collide
    /// with a symbol a user actually types; `N` is a per-heap monotonic
    /// counter, so it never collides with an earlier `gensym` either. Both the
    /// interpreter's `gensym` builtin and the compiled `rt_gensym` shim funnel
    /// through here (see [`Heap::gensym_counter`]) so the two never overlap.
    pub fn gensym(&mut self) -> Value {
        let n = self.gensym_counter;
        self.gensym_counter += 1;
        self.intern_symbol(&format!(" gensym-{}", n))
    }

    // ---- paths ------------------------------------------------------------

    /// Intern a `::` path from its symbol segments, returning a `Value::Path`.
    /// Paths are permanent (they reference only permanent symbols), and equal
    /// segment sequences share one [`PathId`].
    pub fn intern_path(&mut self, segs: &[SymId]) -> Value {
        if let Some(&id) = self.path_ids.get(segs) {
            return Value::Path(PathId(id));
        }
        let id = self.paths.len() as u32;
        self.paths.push(segs.to_vec());
        self.path_ids.insert(segs.to_vec(), id);
        Value::Path(PathId(id))
    }

    /// The symbol segments of an interned path.
    pub fn path_segments(&self, id: PathId) -> &[SymId] {
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
    /// gives a `HashTable<K,V>` string key "equal, not eq" lookup semantics
    /// (see [`MemHashKey`]'s doc comment) — it is not meant as a
    /// general-purpose string constructor.
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

    /// Store an `f64`, returning its `Value::Boxed` — `Sexpr::Float`'s
    /// runtime representation (see [`BoxedObj`]'s doc comment for why a
    /// float can't be an immediate `Value` variant the way `Int`/`Char`/
    /// `Bool` are).
    pub fn alloc_float(&mut self, f: f64) -> Value {
        self.alloc_boxed(BoxedObj::Float(f))
    }

    /// The `f64` behind a boxed float. Panics if `id` doesn't hold a
    /// `BoxedObj::Float` — an internal-invariant trap, not a user-facing
    /// error, the same convention [`string`](Self::string)'s dangling-`StrId`
    /// panic already uses: a correctly type-checked program never passes a
    /// mismatched `BoxId` here.
    pub fn float_value(&self, id: BoxId) -> f64 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Float(f)) => *f,
            _ => panic!("BoxId does not hold a Float"),
        }
    }

    /// Store a `bignum` (arbitrary-precision integer), returning its
    /// `Value::Boxed` — heap-boxed like [`alloc_float`](Self::alloc_float),
    /// and for the same reason (the payload can't ride alongside a tag in
    /// one 64-bit word).
    pub fn alloc_bignum(&mut self, n: num_bigint::BigInt) -> Value {
        self.alloc_boxed(BoxedObj::Bignum(n))
    }

    /// The `BigInt` behind a boxed bignum. Panics if `id` doesn't hold a
    /// `BoxedObj::Bignum` — same internal-invariant-trap convention as
    /// [`float_value`](Self::float_value).
    pub fn bignum_value(&self, id: BoxId) -> &num_bigint::BigInt {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Bignum(n)) => n,
            _ => panic!("BoxId does not hold a Bignum"),
        }
    }

    /// True if `id` holds a `BoxedObj::Bignum` — the peer of
    /// [`is_float`](Self::is_float) for callers decoding an unknown
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
    /// (see `BoxedObj`'s doc comment). `type_name` is what a later builtin
    /// method dispatch (`eval_builtin_method`) uses to decide what the
    /// fields mean; this layer itself doesn't interpret it.
    pub fn alloc_struct(&mut self, type_name: String, fields: Vec<Value>) -> Value {
        self.alloc_boxed(BoxedObj::Struct { type_name, payload: StructPayload::Fields(fields) })
    }

    /// The type name of a boxed struct. Panics if `id` doesn't hold a
    /// `BoxedObj::Struct` — same internal-invariant-trap convention as
    /// [`float_value`](Self::float_value).
    pub fn struct_type_name(&self, id: BoxId) -> &str {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { type_name, .. }) => type_name,
            _ => panic!("BoxId does not hold a Struct"),
        }
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
    /// `HashTable` without risking [`float_value`](Self::float_value)'s or
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

    /// Store an enum (sum-ADT) value — variant index `variant` of `type_name`
    /// with that variant's `fields` — returning its `Value::Boxed`; the
    /// runtime representation `Option<T>`/`Result<T,E>`/user `defenum`
    /// instances all share (see [`BoxedObj::Enum`]'s doc comment for why
    /// this is a variant of its own, not a `Struct`). The enum counterpart
    /// of [`alloc_struct`](Self::alloc_struct); like `type_name` there,
    /// `variant` is stored uninterpreted — which variant means what is the
    /// checker's business, this layer only carries the index.
    pub fn alloc_enum(&mut self, type_name: String, variant: usize, fields: Vec<Value>) -> Value {
        self.alloc_boxed(BoxedObj::Enum { type_name, variant, fields })
    }

    /// True if `id` holds a `BoxedObj::Enum` — the enum peer of
    /// [`is_struct`](Self::is_struct), for callers decoding an unknown
    /// `Value::Boxed`.
    pub fn is_enum(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Enum { .. }))
    }

    /// The type name of a boxed enum value. Panics if `id` doesn't hold a
    /// `BoxedObj::Enum` — same internal-invariant-trap convention as
    /// [`struct_type_name`](Self::struct_type_name).
    pub fn enum_type_name(&self, id: BoxId) -> &str {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Enum { type_name, .. }) => type_name,
            _ => panic!("BoxId does not hold an Enum"),
        }
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
    /// [`float_value`](Self::float_value).
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

    /// True if `id` holds a `BoxedObj::Float` — the *positive* float test
    /// callers decoding an unknown `Value::Boxed` must use now that "not a
    /// struct and not a hashtable" no longer implies float (cells and
    /// closures are boxed too).
    pub fn is_float(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Float(_)))
    }

    // ---- compiled closures ------------------------------------------------------

    /// Store a compiled function value — native entry point `fn_ptr` plus
    /// its captured environment — returning its `Value::Boxed`; see
    /// [`BoxedObj::CompiledClosure`] for the `env`/`sexpr_mask` slot
    /// convention. Like [`alloc_cell`](Self::alloc_cell), never itself
    /// triggers a collection, so the env values may be un-rooted at the
    /// moment of the call.
    pub fn alloc_compiled_closure(&mut self, fn_ptr: usize, env: Vec<Value>, sexpr_mask: u64) -> Value {
        self.alloc_boxed(BoxedObj::CompiledClosure { fn_ptr, env, sexpr_mask })
    }

    /// True if `id` holds a `BoxedObj::CompiledClosure` — the only *compiled*
    /// closure box there is since interp-closure removal Stage 8c. A built-in
    /// used as a function value is [`is_builtin_fn`](Self::is_builtin_fn)
    /// instead; the two together are every `Type::Fn` value.
    pub fn is_compiled_closure(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::CompiledClosure { .. }))
    }

    /// Store an *interpreted* closure — parameter list, body, and captured
    /// environment, all core forms and all `Value` — returning its
    /// `Value::Boxed`. See [`BoxedObj::Closure`].
    pub fn alloc_closure(&mut self, params: Value, body: Value, env: Value) -> Value {
        self.alloc_boxed(BoxedObj::Closure { params, body, env })
    }

    /// True if `id` holds a [`BoxedObj::Closure`].
    pub fn is_closure(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Closure { .. }))
    }

    /// An interpreted closure's parts: `(params, body, env)`. Panics if `id`
    /// does not hold one — the same internal-invariant-trap convention as
    /// [`float_value`](Self::float_value).
    pub fn closure_parts(&self, id: BoxId) -> (Value, Value, Value) {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Closure { params, body, env }) => (*params, *body, *env),
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
    /// *method* (`i32::+`) and `None` for a free built-in (`gensym`). See
    /// [`BoxedObj::Builtin`].
    ///
    /// `name` is interned, so the box is two permanent indices wide and
    /// allocating one never triggers a collection (like
    /// [`alloc_cell`](Self::alloc_cell)).
    pub fn alloc_builtin_fn(&mut self, recv_type: Option<PathId>, name: &str) -> Value {
        let name = match self.intern_string(name) {
            Value::Str(id) => id,
            _ => unreachable!("intern_string always returns Value::Str"),
        };
        self.alloc_boxed(BoxedObj::Builtin { recv_type, name })
    }

    /// True if `id` holds a `BoxedObj::Builtin`.
    pub fn is_builtin_fn(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Builtin { .. }))
    }

    /// A built-in function value's receiver type — `None` for a free
    /// built-in. Panics if `id` doesn't hold a `BoxedObj::Builtin`, the same
    /// internal-invariant-trap convention as
    /// [`float_value`](Self::float_value).
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
    /// [`float_value`](Self::float_value).
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
    /// convention as [`float_value`](Self::float_value).
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

    /// Converts a key argument at the `HashTable` method boundary into a
    /// [`MemHashKey`] *without* interning a not-yet-seen string — used by
    /// read-only lookups ([`hashtable_get`](Self::hashtable_get)/
    /// [`hashtable_remove`](Self::hashtable_remove)): if `key`'s content was
    /// never interned, it cannot possibly be present as a map key (every key
    /// actually stored went through [`intern_hash_key`](Self::intern_hash_key)
    /// first), so `None` here correctly means "not in the map" rather than
    /// requiring a spurious allocation just to look. Panics on a
    /// non-hashable `Value` shape (`Cons`/`Symbol`/`Path`/a non-`Float`
    /// boxed object) — the type checker can't express a "hashable" bound (no
    /// traits in this language), so this is the same runtime-panic fallback
    /// [`struct_field`](Self::struct_field) uses for an out-of-range index.
    fn lookup_hash_key(&self, key: Value) -> Option<MemHashKey> {
        match key {
            Value::Int(n) => Some(MemHashKey::Int(n)),
            Value::Bool(b) => Some(MemHashKey::Bool(b)),
            Value::Char(c) => Some(MemHashKey::Char(c)),
            Value::Str(id) => self.str_intern.get(self.string(id)).copied().map(MemHashKey::Str),
            other => panic!("HashTable: unsupported key type {:?}", other),
        }
    }

    /// [`lookup_hash_key`](Self::lookup_hash_key)'s mutating counterpart,
    /// used by [`hashtable_set`](Self::hashtable_set): interns `key`'s
    /// string content (via [`intern_string`](Self::intern_string)) if this
    /// is the first time it's been used as a key, so the resulting
    /// `MemHashKey::Str` will compare equal to any other string with the
    /// same content used as a key from now on.
    fn intern_hash_key(&mut self, key: Value) -> MemHashKey {
        match key {
            Value::Int(n) => MemHashKey::Int(n),
            Value::Bool(b) => MemHashKey::Bool(b),
            Value::Char(c) => MemHashKey::Char(c),
            Value::Str(id) => {
                let s = self.string(id).to_string();
                match self.intern_string(&s) {
                    Value::Str(interned) => MemHashKey::Str(interned),
                    _ => unreachable!("intern_string always returns Value::Str"),
                }
            }
            other => panic!("HashTable: unsupported key type {:?}", other),
        }
    }

    /// Stores an empty hash map, returning its `Value::Boxed` —
    /// `HashTable<K,V>`'s runtime representation (see `BoxedObj`/
    /// `StructPayload::Map`'s doc comments).
    pub fn alloc_hashtable(&mut self) -> Value {
        self.alloc_boxed(BoxedObj::Struct { type_name: "hashtable".to_string(), payload: StructPayload::Map(HashMap::new()) })
    }

    /// `(gethash key table)`'s primitive: the value `key` maps to, or `None`
    /// if absent. Panics if `id` doesn't hold a `BoxedObj::Struct` with a
    /// `StructPayload::Map` payload.
    pub fn hashtable_get(&self, id: BoxId, key: Value) -> Option<Value> {
        let hk = self.lookup_hash_key(key)?;
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => map.get(&hk).copied(),
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// `(sethash key table val)`'s primitive: inserts/overwrites `key` ->
    /// `val`, returning the previous value if `key` was already present
    /// (same convention as `HashMap::insert`). Panics if `id` doesn't hold a
    /// `BoxedObj::Struct` with a `StructPayload::Map` payload.
    pub fn hashtable_set(&mut self, id: BoxId, key: Value, val: Value) -> Option<Value> {
        let hk = self.intern_hash_key(key);
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => map.insert(hk, val),
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// Removes `key`, returning its value if it was present. Panics if `id`
    /// doesn't hold a `BoxedObj::Struct` with a `StructPayload::Map` payload.
    pub fn hashtable_remove(&mut self, id: BoxId, key: Value) -> Option<Value> {
        let hk = self.lookup_hash_key(key)?;
        match self.box_slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => map.remove(&hk),
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// The number of entries in a boxed hash map. Panics if `id` doesn't
    /// hold a `BoxedObj::Struct` with a `StructPayload::Map` payload.
    pub fn hashtable_count(&self, id: BoxId) -> usize {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => map.len(),
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
    /// [`MemHashKey`] has no direct `Value` counterpart (that's the whole
    /// point of interning it in the first place — see that type's doc
    /// comment), so this reconstructs one from each key, the mem-layer
    /// mirror of the pre-unification `HashKey::from_rtvalue`'s inverse.
    /// Order is whatever the underlying `HashMap` iterates in (unspecified,
    /// like `HashTable<K,V>`'s method surface always has been). Panics if
    /// `id` doesn't hold a `BoxedObj::Struct` with a `StructPayload::Map`
    /// payload.
    pub fn hashtable_pairs(&self, id: BoxId) -> Vec<(Value, Value)> {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map), .. }) => {
                map.iter().map(|(k, v)| (Self::hash_key_to_value(*k), *v)).collect()
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    fn hash_key_to_value(k: MemHashKey) -> Value {
        match k {
            MemHashKey::Int(n) => Value::Int(n),
            MemHashKey::Bool(b) => Value::Bool(b),
            MemHashKey::Char(c) => Value::Char(c),
            MemHashKey::Str(id) => Value::Str(id),
        }
    }

    // ---- scopes -------------------------------------------------------------

    /// Allocates one fresh, empty scope frame box — see
    /// [`StructPayload`]'s doc comment for why a frame is a box of its own
    /// (frame *sharing* across `scope_clone_frames`) rather than a `HashMap`
    /// stored inline in the scope's payload.
    fn alloc_scope_frame(&mut self) -> BoxId {
        let obj = BoxedObj::Struct { type_name: "scope-frame".to_string(), payload: StructPayload::Frame(HashMap::new()) };
        match self.alloc_boxed(obj) {
            Value::Boxed(id) => id,
            _ => unreachable!("alloc_boxed always returns Value::Boxed"),
        }
    }

    /// The frame stack of a scope box. Panics if `id` doesn't hold a
    /// `BoxedObj::Struct` with a `StructPayload::Frames` payload — same
    /// internal-invariant-trap convention as
    /// [`hashtable_get`](Self::hashtable_get)'s.
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
        self.alloc_boxed(BoxedObj::Struct { type_name: "scope".to_string(), payload: StructPayload::Frames(vec![frame]) })
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
        self.alloc_boxed(BoxedObj::Struct { type_name: "scope".to_string(), payload: StructPayload::Frames(frames) })
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
    /// location the reader recorded for it (`elem_locs`, keyed by the spine
    /// cell that holds it) — `None` for an element read from a source with no
    /// location (e.g. a macro-expanded or otherwise synthesized list). The
    /// checker uses this so a bare atom in argument position can carry its own
    /// `Loc`, which the LSP's hover/goto-definition need.
    pub fn list_to_vec_locs(&self, v: Value) -> Result<Vec<(Value, Option<crate::errors::Loc>)>, Error> {
        let mut out = Vec::new();
        let mut cur = v;
        loop {
            match cur {
                Value::Empty => return Ok(out),
                Value::Cons(c) => {
                    let loc = self.elem_locs.get(&c.addr()).cloned();
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
    /// `Struct`'s `Map` payload traces both the values *and* any `Str` keys
    /// (`MemHashKey`'s other variants are immediate, nothing to trace) —
    /// belt-and-suspenders alongside `intern_string`'s permanent rooting:
    /// tracing here keeps the invariant "a live map keeps its own keys/
    /// values live" true from the mark phase's perspective alone, without
    /// leaning on that rooting detail. Later `BoxedObj` kinds (a closure's
    /// captured environment, ...) will extend this `match` with their own
    /// fan-out.
    fn push_boxed_nested(obj: &BoxedObj, stack: &mut Vec<Value>) {
        match obj {
            // `Bignum`/`Ratio` payloads live in ordinary Rust memory (like a
            // `Str`'s buffer) and hold no nested `Value` — nothing to trace.
            // A `RandomState`'s payload is a bare `u64` seed — same "ordinary
            // Rust memory, nothing nested" case as the numeric boxes.
            BoxedObj::Float(_) | BoxedObj::Bignum(_) | BoxedObj::Ratio(_) | BoxedObj::RandomState(_) => {}
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
            BoxedObj::Closure { params, body, env } => {
                stack.push(*params);
                stack.push(*body);
                stack.push(*env);
            }
            // A live trait object keeps the concrete value it wraps live.
            // `vtable_id` names a table outside the heap whose entries are
            // method identities and raw function pointers, so there is
            // nothing else here to trace (see [`BoxedObj::Dyn`]).
            BoxedObj::Dyn { value, .. } => stack.push(*value),
            BoxedObj::Struct { payload: StructPayload::Map(map), .. } => {
                for (k, &v) in map {
                    if let MemHashKey::Str(id) = k {
                        stack.push(Value::Str(*id));
                    }
                    stack.push(v);
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
        for (i, &v) in self.roots.iter().enumerate() {
            check("roots", i, v);
        }
        for (i, &v) in self.permanent_roots.iter().enumerate() {
            check("permanent_roots", i, v);
        }
        for (i, &v) in self.session_roots.iter().enumerate() {
            check("session_roots", i, v);
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
        for i in 0..self.roots.len() {
            stack.push(self.roots[i]);
        }
        for i in 0..self.permanent_roots.len() {
            stack.push(self.permanent_roots[i]);
        }
        for i in 0..self.session_roots.len() {
            stack.push(self.session_roots[i]);
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
                        // A cell being reclaimed loses whatever source location
                        // was recorded for it: its address is about to be handed
                        // to an unrelated form, and `code_locs` (unlike the
                        // reader's `cons_locs`) is never bulk-cleared, so a stale
                        // entry would silently mislabel that form. See the
                        // `code_locs` field doc comment.
                        if !self.code_locs.is_empty() {
                            self.code_locs.remove(&(p as usize));
                        }
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
/// fragile at scale — one builder in `compile::ast_bridge` balances *sixteen*
/// pops by hand, and every `?` early return is a chance to skip them. This
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
