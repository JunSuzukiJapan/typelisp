//! A manually-managed value heap with a precise mark-sweep collector.
//!
//! # Design
//!
//! * **Fixed cons arena.** `with_capacity(n)` allocates `n` cons cells once. The
//!   buffer is never reallocated, so the raw `*mut Cell` inside every
//!   [`Value::Cons`] stays valid for the heap's lifetime. It is a single owned
//!   allocation, freed in `Drop` — the backing memory cannot leak.
//! * **Free list.** Unused cells are chained through `Cell::next_free`.
//!   Allocation pops the head; if the list is empty it runs a GC and retries.
//!   The arena never grows: a GC that frees nothing makes allocation return
//!   [`Error::HeapExhausted`] (size the arena up front instead).
//! * **Symbols** are interned (name -> [`SymId`]) and never collected — they are
//!   few and live for the heap's lifetime, like CL symbols in a package.
//! * **Strings** live in a slot store and ARE collected: the mark phase marks
//!   every reachable [`StrId`], the sweep frees unmarked slots (recycling
//!   indices). So unreachable strings do not leak.
//! * **Mark-sweep.** `gc()` marks everything reachable from the root set
//!   (iteratively — no native recursion), then rebuilds the cons free list and
//!   sweeps strings. Cycles are reclaimed (unlike reference counting).
//! * **Two root sets.** `roots` is a strict LIFO stack (push on scope entry,
//!   pop on scope exit). `permanent_roots` holds values whose owner outlives
//!   any single activation (e.g. a field inside a heap-external, never-freed
//!   box) — appended to, never popped. `gc()` marks from both.
//!
//! All `unsafe` is confined here; the public API is safe.

use std::collections::HashMap;
use std::ptr;

use crate::Error;
use super::value::{BoxId, BoxedObj, Cell, ConsRef, MemHashKey, PathId, StrId, StructPayload, SymId, Value};

pub struct Heap {
    base: *mut Cell, // start of the cons arena; owns the allocation
    cap: usize,
    free: *mut Cell, // head of the free list (null when empty)
    free_count: usize,
    roots: Vec<Value>,
    permanent_roots: Vec<Value>,

    // interned symbols (permanent)
    sym_names: Vec<String>,
    sym_ids: HashMap<String, u32>,

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

    // Tokens of `BoxedObj::Closure`s freed by the most recent sweeps, not
    // yet drained by the interpreter (`take_dead_closure_tokens`) — the
    // bridge that lets the side table holding each closure's body (and its
    // GC-invisible `Native` captures) release entries in step with the
    // heap. Accumulated rather than returned from `gc()` because most
    // collections run *implicitly* inside `cons()`, where a return value
    // has no consumer.
    dead_closure_tokens: Vec<u32>,
}

impl Heap {
    /// Create a heap with `capacity` cons cells pre-allocated.
    pub fn with_capacity(capacity: usize) -> Heap {
        let mut v: Vec<Cell> = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            v.push(Cell::blank());
        }
        let boxed: Box<[Cell]> = v.into_boxed_slice();
        let base: *mut Cell = Box::into_raw(boxed) as *mut Cell;

        unsafe {
            for i in 0..capacity {
                let p = base.add(i);
                (*p).next_free = if i + 1 < capacity { base.add(i + 1) } else { ptr::null_mut() };
            }
        }

        Heap {
            base,
            cap: capacity,
            free: if capacity > 0 { base } else { ptr::null_mut() },
            free_count: capacity,
            roots: Vec::new(),
            permanent_roots: Vec::new(),
            sym_names: Vec::new(),
            sym_ids: HashMap::new(),
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
            dead_closure_tokens: Vec::new(),
        }
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
        self.roots.push(v);
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

    /// Re-registers an *existing*, live cell in the liveness registry,
    /// returning a fresh owning handle — for a caller that reached the cell
    /// through a heap reference (a closure's captured environment) rather
    /// than an `Rc` it already holds, and now needs the cell to outlive
    /// that reference (e.g. a call frame binding that must survive the
    /// closure box itself being swept mid-call). Panics if `id` is not a
    /// live cell, same convention as [`cell_get`](Self::cell_get).
    pub fn adopt_cell(&mut self, id: BoxId) -> std::rc::Rc<BoxId> {
        assert!(self.is_cell(id), "adopt_cell: BoxId does not hold a live Cell");
        let rc = std::rc::Rc::new(id);
        self.cell_registry.push(std::rc::Rc::downgrade(&rc));
        rc
    }

    // ---- closures --------------------------------------------------------------

    /// Store a function value, returning its `Value::Boxed` — see
    /// [`BoxedObj::Closure`]. `env` must hold only `Value::Boxed` cell
    /// references (the closure's heap-cell captures); the body lives in the
    /// caller's side table under `body_token`. Like
    /// [`alloc_cell`](Self::alloc_cell), never itself triggers a collection.
    pub fn alloc_closure(&mut self, body_token: u32, env: Vec<Value>) -> Value {
        self.alloc_boxed(BoxedObj::Closure { body_token, env })
    }

    /// True if `id` holds a `BoxedObj::Closure`.
    pub fn is_closure(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Closure { .. }))
    }

    /// True if `id` holds a `BoxedObj::Float` — the *positive* float test
    /// callers decoding an unknown `Value::Boxed` must use now that "not a
    /// struct and not a hashtable" no longer implies float (cells and
    /// closures are boxed too).
    pub fn is_float(&self, id: BoxId) -> bool {
        matches!(self.box_slots[id.0 as usize], Some(BoxedObj::Float(_)))
    }

    /// A closure's side-table key. Panics if `id` doesn't hold a
    /// `BoxedObj::Closure` — same internal-invariant-trap convention as
    /// [`float_value`](Self::float_value).
    pub fn closure_token(&self, id: BoxId) -> u32 {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Closure { body_token, .. }) => *body_token,
            _ => panic!("BoxId does not hold a Closure"),
        }
    }

    /// A closure's heap-cell captures, in layout order. Panics like
    /// [`closure_token`](Self::closure_token).
    pub fn closure_env(&self, id: BoxId) -> &[Value] {
        match &self.box_slots[id.0 as usize] {
            Some(BoxedObj::Closure { env, .. }) => env,
            _ => panic!("BoxId does not hold a Closure"),
        }
    }

    /// Drains the tokens of every closure freed by collections since the
    /// last drain — see the `dead_closure_tokens` field. The interpreter
    /// calls this from `sync_roots` (which precedes every allocation) so a
    /// side-table entry outlives its swept closure by at most one
    /// allocation.
    pub fn take_dead_closure_tokens(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.dead_closure_tokens)
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

    // ---- allocation -------------------------------------------------------

    /// Allocate a cons cell `(car . cdr)`. Runs a GC if the free list is empty;
    /// returns [`Error::HeapExhausted`] if even then no cell is available.
    pub fn cons(&mut self, car: Value, cdr: Value) -> Result<Value, Error> {
        if self.free.is_null() {
            self.gc();
            if self.free.is_null() {
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
            BoxedObj::Float(_) => {}
            BoxedObj::Struct { payload: StructPayload::Fields(fields), .. } => {
                for &v in fields {
                    stack.push(v);
                }
            }
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
            // A live closure keeps its captured cells (and, through them,
            // their contents) live — including a `labels` cycle
            // (cell -> closure -> sibling cell -> ...), which mark-sweep
            // reclaims as a unit once nothing external reaches it.
            BoxedObj::Closure { env, .. } => {
                for &v in env {
                    stack.push(v);
                }
            }
        }
    }

    /// Run a mark-sweep collection. Returns the number of cons cells reclaimed.
    pub fn gc(&mut self) -> usize {
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

        // SWEEP cons: rebuild the free list over the whole arena.
        let old_free = self.free_count;
        let mut new_free: *mut Cell = ptr::null_mut();
        let mut new_free_count = 0usize;
        for i in (0..self.cap).rev() {
            unsafe {
                let p = self.base.add(i);
                if (*p).mark {
                    (*p).mark = false;
                } else {
                    (*p).car = Value::Empty;
                    (*p).cdr = Value::Empty;
                    (*p).next_free = new_free;
                    new_free = p;
                    new_free_count += 1;
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

        // SWEEP boxed objects: same recycling scheme as strings. A swept
        // closure additionally reports its side-table token — see
        // `take_dead_closure_tokens`.
        for i in 0..self.box_slots.len() {
            if self.box_slots[i].is_some() && !self.box_marks[i] {
                if let Some(BoxedObj::Closure { body_token, .. }) = &self.box_slots[i] {
                    self.dead_closure_tokens.push(*body_token);
                }
                self.box_slots[i] = None;
                self.box_free.push(i as u32);
            }
        }

        new_free_count - old_free
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        if !self.base.is_null() {
            // Reconstitute the owning Box and drop it, freeing the arena.
            unsafe {
                let raw = ptr::slice_from_raw_parts_mut(self.base, self.cap);
                drop(Box::from_raw(raw));
            }
        }
    }
}
