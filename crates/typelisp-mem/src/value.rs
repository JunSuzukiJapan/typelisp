//! Values and cons cells for the managed heap.
//!
//! A [`Value`] is a small `Copy` tagged value. This is the runtime encoding of
//! the typelisp `Sexpr` type:
//!
//! ```text
//! Sexpr = Nil | Int | Float | Char | Bool | Sym | Str | Cons(Sexpr, Sexpr)
//! ```
//!
//! The empty list `()` is the `Sexpr::Nil` value, encoded at runtime by
//! [`Value::Empty`] — a peer of [`Value::Cons`] in its own right, on the same
//! footing as the other variants (not a wrapper around them). (`Empty` is the
//! empty-list datum, **not** the removed language-level `nil`.)
//!
//! Cons cells live in a [`Heap`](super::heap::Heap) arena and are referenced
//! through the opaque [`ConsRef`] (a raw pointer that is never dereferenced
//! outside `mem`). Symbols and strings are stored in the heap and referenced by
//! [`SymRef`] / [`StrId`]. `Float` is heap-resident too, behind [`BoxId`] (see
//! [`BoxedObj`]) — an `f64` doesn't fit alongside a tag in one 64-bit word,
//! the same reason `Str` isn't stored inline. The public surface is entirely
//! safe.

use std::collections::HashMap;
use std::fmt;

use crate::symbols::SymRef;
use num_bigint::BigInt;
use num_rational::BigRational;

/// A source location, as a cell holds one: an index into the heap's own
/// location table, or [`LocId::NONE`].
///
/// An index rather than the [`Loc`](crate::Loc) itself because a `Loc` owns an
/// `Rc<str>` file name and is four `u32`s besides — 32 bytes and not `Copy`,
/// where the arena wants a cell to stay small and trivially copyable. The
/// table interns, so the many cells read from one line share one entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LocId(pub(crate) u32);

impl LocId {
    /// No location recorded. Every freshly allocated cell starts here.
    pub const NONE: LocId = LocId(0);

    pub(crate) fn is_none(self) -> bool {
        self == LocId::NONE
    }
}

/// One cons cell as laid out in the arena.
///
/// `next_free` chains cells on the free list and is meaningful only while the
/// cell is free; `mark` is the GC mark bit, meaningful only during a sweep.
///
/// # Why the source locations live here
///
/// A location is a property of the datum it was read from, so it belongs *in*
/// the datum. The alternative — side tables keyed by the cell's address — is
/// what this replaced, and each of that design's three tables existed only to
/// work around a consequence of being outside the cell: a reader table had to
/// be bulk-cleared per read batch (so a recycled address could not mislabel a
/// new form), which meant lowered code needed a *second*, uncleared table to
/// outlive its batch, which in turn needed the GC's sweep to delete entries
/// for reclaimed cells. Inline, a location is reclaimed with its cell and a
/// fresh cell has none by construction — see [`super::heap::Heap::cons`].
///
/// Two slots, because a list form and its elements are different spans and
/// both are needed. `self_loc` is the span of the form this cell heads
/// (`(` through `)`); `car_loc` is the span of the element in this cell's
/// `car`. The latter is the only way an *atom* gets a position at all:
/// symbols are interned and small scalars are immediate, so no atom has a
/// per-occurrence identity to hang one on — the cell holding it does.
pub(crate) struct Cell {
    pub(crate) car: Value,
    pub(crate) cdr: Value,
    pub(crate) next_free: *mut Cell,
    pub(crate) mark: bool,
    /// The span of this cell's `car` — a list element's own position.
    pub(crate) car_loc: LocId,
    /// The span of the form this cell heads.
    pub(crate) self_loc: LocId,
}

impl Cell {
    pub(crate) fn blank() -> Cell {
        Cell {
            car: Value::Empty,
            cdr: Value::Empty,
            next_free: std::ptr::null_mut(),
            mark: false,
            car_loc: LocId::NONE,
            self_loc: LocId::NONE,
        }
    }
}

/// An opaque reference to a cons cell. `Copy`, compared by identity.
#[derive(Clone, Copy)]
pub struct ConsRef(pub(crate) *mut Cell);

impl PartialEq for ConsRef {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl fmt::Debug for ConsRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConsRef(0x{:x})", self.0 as usize)
    }
}

impl ConsRef {
    /// The raw address backing this cons cell, as an opaque integer rather
    /// than the (crate-private) `*mut Cell` itself — for embedding in
    /// `typelisp-rt`'s tagged compiled-code representation of a `Sexpr`
    /// value. Never meaningful to do arithmetic on; only ever round-tripped
    /// back through [`ConsRef::from_addr`] and the ordinary `Heap` API
    /// (`car`/`cdr`/`set_car`/`set_cdr`).
    pub fn addr(&self) -> usize {
        self.0 as usize
    }

    /// # Safety
    ///
    /// `addr` must have come from [`ConsRef::addr`] on a cons cell that is
    /// still live — not freed by a [`super::heap::Heap::gc`] call since.
    pub unsafe fn from_addr(addr: usize) -> ConsRef {
        ConsRef(addr as *mut Cell)
    }
}

/// Reference to a string in the heap's string store.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StrId(pub(crate) u32);

/// Reference to an interned `::` path (a sequence of symbols) in the heap.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PathId(pub(crate) u32);

/// A type's **runtime identity**, interned in the heap's type-key table.
///
/// Every [`BoxedObj::Struct`]/[`BoxedObj::Enum`] carries one of these, and
/// asking "are these two values the same type?" or "is this value a `point`?"
/// is an integer comparison on it. The identity used to be the type's name
/// spelled out as a `String` on every instance, which meant every such test
/// was a string comparison and every allocation copied the name again; the
/// spelling now lives once, in the table, and only printing and diagnostics
/// go back to it (see [`super::heap::Heap::type_key_name`]).
///
/// Interning is permanent — an id stays valid for the heap's lifetime and
/// equal names always share one id, exactly like an interned [`SymRef`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeKeyId(pub(crate) u32);

/// The type keys every heap has from birth, interned by `Heap::with_capacity` in this
/// order so that their ids are compile-time constants (the `TypeKeyId`
/// associated constants below).
///
/// These are the types the *runtime* itself builds without a checker in the
/// loop: what `typelisp-rt`'s shims wrap results in, what the reader returns,
/// and what the heap's own `HashTable`/`Scope` machinery allocates. A user
/// type is interned on first use like any other name; only these need to be
/// nameable from Rust without a heap in hand.
pub const BUILTIN_TYPE_KEYS: &[&str] = &[
    "option",
    "result",
    "vector",
    "cons-cell",
    "hashtable",
    "scope",
    "scope-frame",
    "readerror",
    "fileerror",
    "parseinterror",
    "parsefloaterror",
    "universal-time",
    "internal-time",
    "heap-info",
];

impl TypeKeyId {
    pub const OPTION: TypeKeyId = TypeKeyId(0);
    pub const RESULT: TypeKeyId = TypeKeyId(1);
    pub const VECTOR: TypeKeyId = TypeKeyId(2);
    pub const CONS_CELL: TypeKeyId = TypeKeyId(3);
    pub const HASHTABLE: TypeKeyId = TypeKeyId(4);
    pub const SCOPE: TypeKeyId = TypeKeyId(5);
    pub const SCOPE_FRAME: TypeKeyId = TypeKeyId(6);
    pub const READ_ERROR: TypeKeyId = TypeKeyId(7);
    pub const FILE_ERROR: TypeKeyId = TypeKeyId(8);
    pub const PARSE_INT_ERROR: TypeKeyId = TypeKeyId(9);
    pub const PARSE_FLOAT_ERROR: TypeKeyId = TypeKeyId(10);
    pub const UNIVERSAL_TIME: TypeKeyId = TypeKeyId(11);
    pub const INTERNAL_TIME: TypeKeyId = TypeKeyId(12);
    /// `(heap-info)`'s result — `room`'s report, as a struct a program can
    /// also read the numbers out of (`crate::Heap`'s own statistics).
    pub const HEAP_INFO: TypeKeyId = TypeKeyId(13);

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

// `as_u32`/`from_u32`: for `typelisp-rt`'s tagged compiled-code
// representation, which embeds these as plain integer payloads (see
// `typelisp-rt`'s `encode`/`decode`).
impl StrId {
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn from_u32(v: u32) -> StrId {
        StrId(v)
    }
}

impl PathId {
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn from_u32(v: u32) -> PathId {
        PathId(v)
    }
}

/// Reference to a boxed (heap-resident, GC-collected) object in the heap's
/// box store — see [`BoxedObj`] and `Heap`'s `box_slots`. The general-purpose
/// counterpart to [`StrId`]: unlike a string, a boxed object's payload may
/// itself hold nested `Value`s (a struct's fields, a closure's captured
/// environment, ...), so the mark phase must trace *into* it, not just flag
/// the slot — see `Heap::gc`'s unified `Vec<Value>` mark stack.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BoxId(pub(crate) u32);

impl BoxId {
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn from_u32(v: u32) -> BoxId {
        BoxId(v)
    }
}

/// The payload behind a [`Value::Boxed`] slot. Every "Lisp-writable" value
/// beyond `Sexpr`'s own remaining built-in immediate/heap shapes (structs/
/// `Vector<T>`/`HashTable<K,V>`/`Scope<V>`/closures) is meant to eventually
/// live here, as one uniform heap-resident, GC-traced representation instead
/// of a separate, `Rc`-managed value universe — see the project's `Sexpr`/
/// `RtValue` unification plan. The floats are the first case, and a
/// deliberately forced one: an `f64` doesn't fit losslessly alongside a 3-bit
/// tag in a 64-bit word (unlike `Int`/`Char`/`Bool`/every interned-index
/// variant), so a float was never representable in the tagged compiled-code
/// ABI at all (`typelisp-rt`'s `encode`/`decode` used to `fatal()` on it) —
/// boxing it here (heap-resident behind a small index, exactly like
/// `Value::Str` already is) is what makes it representable, closing that gap
/// as the first proof of this mechanism.
///
/// **One variant per width.** `f32` and `f64` are different types, and a
/// value always *is* the number its type names — so the box says which,
/// rather than storing both as `f64` and leaving every reader to guess. One
/// `Float(f64)` variant was the older design, and it made `(the f32 0.1)`
/// print as `0.10000000149011612`: the printer had no way to ask for
/// binary32's shortest round-trip. Detecting it from the value is unsound —
/// `0.10000000149011612` *is* a binary32-representable `f64`, so a genuine
/// `f64` holding it would print as `0.1` and read back as a different
/// number.
///
/// `Struct` is the second case: `defstruct` instances, `Vector<T>`, and
/// `cons-cell<K,V>` all share this one variant rather than getting one each
/// — `type_key` plus whichever builtin method dispatches on it is what
/// gives the fields their meaning (the same "no special-cased runtime
/// shape, just a box + a name" treatment `Vector<T>` already got when it was
/// reintroduced on top of `RtValue::Struct`), not three parallel encodings
/// of the same "a name and some fields" shape.
///
/// No longer `Copy` (a `Struct`'s `Vec<Value>` owns heap memory of its own,
/// unlike a float's bare payload) — every read site now borrows instead of
/// implicitly copying, e.g. [`super::heap::Heap::f64_value`].
/// A boxed float, read back as the width it actually is.
///
/// [`BoxedObj`] is crate-private, so this is how a caller outside
/// `typelisp-mem` handles "a float of either width" without the heap having
/// to pick one for it. Every arm names a width: there is no way to read a
/// float out of the heap without saying which one you got.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FloatBox {
    F32(f32),
    F64(f64),
}

/// A boxed narrow integer, read back as the type it actually is.
///
/// The integer counterpart of [`FloatBox`], and for the same reason: `i8`,
/// `i16`, `u8`, `u16` and `u32` all ride in an `i64` carrier, so the carrier
/// alone cannot say which of the five it is. Reading one out means being told.
///
/// `width` is in bits and `signed` says which end the value was extended
/// from; `value` is normalized (`crate::normalize_int`'s invariant: the word
/// *is* the number the type names, sign-extended or zero-extended into 64
/// bits), so `value` may be compared and printed directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NarrowInt {
    pub width: u8,
    pub signed: bool,
    pub value: i64,
}

#[derive(Clone, Debug)]
pub(crate) enum BoxedObj {
    /// A `f32`, stored as a `f32`: the type says binary32, so the box holds
    /// binary32. Storing it widened to `f64` would be a second place the
    /// width could drift from the type.
    Float32(f32),
    Float64(f64),
    /// An `i8`/`i16`/`u8`/`u16`/`u32`, carrying its own width and signedness.
    ///
    /// Boxed for the same reason a float is: the tagged word has three bits
    /// of tag and all eight patterns are spoken for, so a value that must
    /// also say *which* integer type it is has nowhere to put that. A plain
    /// `Value::Int` is the `i32` case and needs no box — it is the width the
    /// bare tagged word already means.
    ///
    /// A narrow integer is boxed **only where its type is not otherwise
    /// written down**: inside a `Sexpr`. In a statically typed position (a
    /// local, a parameter, a `defstruct` field) the declared type says the
    /// width, and the value stays the raw normalized word it always was —
    /// nothing about arithmetic changes.
    Narrow(NarrowInt),
    /// A `bignum` (arbitrary-precision integer, CL's bignum). Heap-boxed for
    /// the same reason `Float` is — the value doesn't fit alongside a tag in
    /// one 64-bit word — with the payload (a `num_bigint::BigInt`) living in
    /// ordinary Rust memory like a `Str`'s `String` buffer: nothing nested
    /// for the mark phase to trace.
    Bignum(BigInt),
    /// A `ratio` (exact rational, CL's ratio type): a `num_rational::BigRational`,
    /// always kept in reduced form with a positive denominator (the crate
    /// normalizes on construction). Same heap-boxing rationale as `Bignum`.
    Ratio(BigRational),
    Struct { type_key: TypeKeyId, payload: StructPayload },
    /// An enum (sum-ADT) value: `Option<T>`/`Result<T,E>`/a user `defenum`
    /// instance — one variant's index plus that variant's field values. The
    /// third case of the unification mechanism (after `Float` and `Struct`),
    /// replacing *two* prior representations at once: the interpreter's
    /// Rust-side `RtValue::Data` (heap-invisible, so it could never sit in
    /// a struct field, cross `call_compiled`, or nest) and compiled code's
    /// raw un-GC-managed `malloc` box (deliberately leaked, its heap-tagged
    /// fields pinned as permanent roots forever). A *separate* variant from
    /// [`Struct`], not `Fields` with the tag squeezed in as `fields[0]`:
    /// `Heap::is_struct` must stay `false` for an enum (the interpreter's
    /// `match` dispatch, struct field accessors, and `rt_struct_*` all key
    /// on it), and `Some(x)` would otherwise be indistinguishable from a
    /// one-field struct. `type_key` is kept for display/debugging (variant
    /// *names* live in the checker's registry; the runtime needs only the
    /// index), mirroring `Struct`'s own. Enum values are immutable — no
    /// setter exists at any layer — so sharing one box between bindings is
    /// unobservable.
    Enum { type_key: TypeKeyId, variant: usize, fields: Vec<Value> },
    /// A shared mutable variable slot — the heap-resident replacement for
    /// the interpreter's `Rc<RefCell<RtValue>>` binding cells, for bindings
    /// whose static type's runtime representation is a `Value` (`Sexpr`,
    /// boxed structs, `HashTable<K,V>`): `let`/parameters/`match` bindings/
    /// globals of those types, and (Stage 6b) the slots sibling closures
    /// share. Living here — instead of Rust-side `Rc` cells the GC can't
    /// see — makes the binding itself traceable: rooting the cell keeps its
    /// current contents live with no per-slot re-collection pass, and a
    /// closure's captured environment can be an ordinary `Vec<Value>` of
    /// these. Bindings of every *other* type (scalars, `Option`/`Result`,
    /// LLVM handles — nothing the GC needs to trace, or nothing `Value` can
    /// represent) stay in Rust-side cells; see `Interp`'s `Slot`.
    Cell(Value),
    /// A *compiled* function value (a `lambda`, a `labels` sibling, or a
    /// reified named function): a native entry point plus its captured
    /// environment — the closure case of the unification mechanism (the
    /// enum-style flip of compiled code's raw un-GC-managed `malloc`
    /// `ClosureBox`, whose reference-counting header, retain/release calls,
    /// and capture mask globals it replaces wholesale; lifetime is the
    /// GC's business now, which also reclaims `labels` sibling cycles the
    /// refcount scheme deliberately leaked). One of the two closure boxes,
    /// the other being its interpreted twin [`Closure`](BoxedObj::Closure) —
    /// which values of the same `Type::Fn` can equally be, so anything that
    /// applies a function value has to admit both. In compiled code that
    /// dispatch is `typelisp_rt::rt_apply_any`'s.
    ///
    /// `fn_ptr` is the native entry point (the `compiled_fn_type_with_env`
    /// ABI: `(args_ptr, argc, env_ptr, env_len) -> i64`), opaque at this
    /// layer. `env` holds one `Value`
    /// per captured slot, but only slots whose bit is set in `sexpr_mask`
    /// (bit `i` = slot `i`, so at most 64 captures — the same limit the
    /// replaced `ClosureBox` fn-mask had) are *real* tagged values the mark
    /// phase must trace; an unset bit means the slot carries raw native
    /// bits (an untagged scalar, float bits, ...) smuggled through
    /// `Value::Int`, GC-invisible by construction. Tracing every slot
    /// uniformly is still correct — a raw slot's `Value::Int` is immediate
    /// — the mask exists so the *accessors* can hand compiled code back the
    /// exact raw word it stored (`rt_closure_env_get` re-encodes masked
    /// slots and unwraps unmasked ones).
    CompiledClosure { fn_ptr: usize, env: Vec<Value>, sexpr_mask: u64, body_abi: u8 },
    /// A **compiled function's activation record** (Phase C): its locals, as
    /// the tagged machine words compiled code loads and stores them as.
    ///
    /// The shape is [`CompiledClosure`]'s env with one difference that
    /// decides everything else: `words` is `Vec<i64>`, not `Vec<Value>`.
    /// Compiled code reaches a local through a raw pointer
    /// (`Heap::frame_data_ptr`) and a plain `load`/`store` — that is what a
    /// local *is* once the machine has it — so the storage has to be machine
    /// words. A closure's env can afford `Vec<Value>` because it is converted
    /// once, at the apply.
    ///
    /// `mask` says which slots hold a tagged value rather than a raw one, bit
    /// `i` for slot `i` — but as a **`Vec<u64>` of bit words**, not
    /// `sexpr_mask`'s single `u64`, so a frame is not capped at 64 slots.
    /// The cap is not hypothetical: the island's own `compile-assoc` has 69
    /// binding sites, and a frame slot is allocated per site (the faithful
    /// translation of today's one-`alloca-args`-per-binding scheme, where
    /// disjoint `match` arms each get their own stack slot too). One
    /// function over the line on day one is enough to pay for a `Vec`.
    ///
    /// **Only masked slots are interpreted at all**: an unmasked slot holds
    /// a raw `i32`/`f64` bit pattern whose low three bits mean nothing, and
    /// reading it as a tag would invent a heap reference. A masked slot
    /// still needs the tag checked — a `Sexpr`-repr local may hold a fixnum
    /// — which is `tagged::references_heap`'s job.
    ///
    /// The mask starts empty and is filled in one bit at a time, at each
    /// binding, by [`Heap::set_frame_mask_bit`]. That is not a concession to
    /// the `Vec`: it is what lets the frame be allocated knowing only how
    /// many slots the function needs, with no second pass over the body to
    /// classify them first. It also costs nothing — the call that sets the
    /// bit stands exactly where the `rt_push_sexpr_root` for that binding
    /// used to.
    ///
    /// `pc` is where to resume: 0 on entry, and the id of a call site
    /// afterwards. The frame is what makes a compiled call suspendable —
    /// everything the function still needs is in here rather than on the
    /// machine stack, so the driver can put it down and pick it up later.
    Frame { words: Vec<i64>, mask: Vec<u64>, pc: u32 },
    /// An *interpreted* closure: a `lambda` or a `labels` sibling the
    /// evaluator walks rather than a native entry point it jumps to.
    ///
    /// A closure box of this kind existed before and was deleted at
    /// interp-closure removal Stage 8c, when every closure became
    /// JIT-compiled at definition time. It could not carry its own body:
    /// the body was a checked Rust AST, invisible to the collector, so the
    /// box held a `body_token` into an interpreter-side table instead. With
    /// the program itself made of cons cells there is no side table and no
    /// token — `params`, `ret`, `body` and `env` are ordinary `Value`s the
    /// mark phase traces like any other, which is what lets a closure be
    /// collected (and a `labels` cycle reclaimed) by the same rules
    /// everything else follows.
    ///
    /// Its return is what demotes the JIT from *required* to an
    /// optimisation: a `lambda` whose parameters have no compiled
    /// representation used to be a hard error at definition time, and can
    /// now simply be interpreted.
    ///
    /// `params` is the core `((SYM REPR)...)` list and `ret` the return
    /// representation, so the box carries its own whole signature. The
    /// interpreter needs neither — it binds and returns `Value`s — but
    /// `typelisp_rt::rt_apply_any` does: when compiled code applies one of
    /// these, the argument words arriving from the compiled side and the
    /// word going back have to be decoded and encoded by *declared*
    /// representation, and the apply site does not carry one at runtime. A
    /// closure is the only thing that knows its own.
    Closure { params: Value, ret: Value, body: Value, env: Value },
    /// A *built-in* function used as a function value: `symbol->string`
    /// passed to a higher-order function, `+` reified as `i32::+`. The second function
    /// case alongside [`CompiledClosure`](BoxedObj::CompiledClosure), and the
    /// reason it exists is that a built-in has no compiled entry point to
    /// point at — it is a name the interpreter's own `eval_builtin`/
    /// `eval_builtin_method` dispatch on.
    ///
    /// This box carries only that name (plus the receiver type for a method),
    /// which is the whole point: the interpreter used to hold built-in
    /// function values in Rust-side `RtValue::Builtin(String)`/
    /// `BuiltinMethod(Path, String)` variants with no `Value` form at all,
    /// and that gap is what made `Type::Fn` unstorable in a heap
    /// `Enum`/`Struct` field — `(Option::some symbol->string)` had to fall back to
    /// the heap-invisible `RtValue::Data`. Giving them a box closes it.
    ///
    /// `name` is a [`super::heap::Heap::intern_string`] id and `recv_type` an
    /// interned [`PathId`], so both are permanent and low-cardinality (there
    /// are finitely many built-ins) — nothing here is per-value allocation.
    /// A built-in is *not* callable from compiled code: the `rt_closure_*`
    /// shims all test `is_compiled_closure` and refuse anything else, and the
    /// interpreter refuses to marshal one across the boundary, so this box
    /// stays interpreter-side until `rt_apply_any` exists.
    /// `ret_key` is the runtime identity of what this builtin *returns*, for
    /// the container constructors whose result is a box with no field to read
    /// an instantiation off (`Vector::new`, `HashTable::keys`). Spelled by the
    /// checker at the reference site, since applying the value later is too
    /// late to ask.
    Builtin { recv_type: Option<PathId>, name: StrId, ret_key: StrId },
    /// A `random-state` (CL's `random-state`): the current seed of a mutable
    /// PRNG stream, advanced in place on every draw.
    ///
    /// A box rather than an immediate because CL gives a `random-state`
    /// *identity* — every binding that shares one observes the same draws, and
    /// an independent stream is only ever made explicitly, by
    /// `random-state-copy`. That is exactly "one mutable cell, shared by
    /// reference", which a `BoxId` gives and a copied `u64` would destroy.
    ///
    /// It was an `Rc<Cell<u64>>` on the interpreter's side of the fence, which
    /// the collector could not see and no `Value` could carry — the last
    /// runtime value with no heap form, and so the last thing standing between
    /// the two value worlds and their unification.
    RandomState(u64),
    /// A trait object (`:dyn Trait`): a vtable identifier alongside
    /// the concrete value it dispatches for.
    ///
    /// This is the C++ vtbl scheme with the vptr moved off the *object* and
    /// onto the *reference* — one vtable per (concrete type, trait) pair,
    /// chosen where the value is boxed (both are statically known there), so
    /// a call site indexes a constant slot and jumps. Keeping the pointer
    /// here rather than in [`Struct`]/[`Enum`] is what lets every existing
    /// `defstruct`/`defenum` value be used as a trait object with no change
    /// to its own layout, its allocation path (`rt_struct_new`/`rt_data_new`),
    /// or the cost of code that never mentions `:dyn` — and it sidesteps the
    /// one-vptr-per-object limit, since a type may implement many traits.
    ///
    /// `vtable_id` indexes tables held *outside* the heap (the interpreter's
    /// `Interp::vtables` and compiled code's `typelisp_rt` vtable registry),
    /// whose entries are method identities and raw function pointers — never
    /// heap values. So the mark phase has exactly one thing to trace here:
    /// `value`.
    Dyn { vtable_id: u32, value: Value },
}


/// The fields behind a [`BoxedObj::Struct`]. `Fields` is a fixed-length
/// `defstruct` instance or a variable-length `Vector<T>`/`cons-cell<K,V>`
/// (both just "a `Vec<Value>`" at this layer — length-checking a fixed-arity
/// struct's field count is the caller's job, same as it already is for the
/// pre-unification `RtValue::Struct`). `Map` is a `HashTable<K,V>`, stored as
/// **hash -> bucket**: this layer neither hashes a key nor compares two,
/// because it cannot. Both questions belong to the key's type — `sxhash` and
/// `equals`, written in typelisp — so the prelude's own `get`/`set`/`remove`
/// ask them and hand the answers down here as a plain number and an index
/// into a bucket. That is what lets a `defstruct` be a key at all; before it,
/// a mem-layer key enum listed the four scalar shapes it knew how to
/// hash, and no user type could ever join them.
///
/// The `HashMap`'s own bucket storage is ordinary Rust memory (like
/// `str_slots`'s `String` buffers), so the mark phase only needs to trace
/// the [`Value`]s it holds — every key and every value — not the map
/// structure itself.
///
/// `Frames` is a `Scope<V>`: a stack of frames, each an ordinary
/// name-keyed binding map. The frames are **not stored inline** — each is a
/// box of its own (a `Frame` payload) that the scope references by
/// [`BoxId`], because `Scope::clone-frames` shares every existing frame *by
/// reference* between the original and the clone (a pointer copy per frame,
/// never a copy of a frame's entries — the scope-chain design real language
/// implementations use; a write into a shared frame is visible through
/// every scope that holds it). Only the frame *stack* is per-scope: a
/// `push-frame` after cloning grows one scope's stack without affecting the
/// other. `Frame`'s keys are plain owned `String`s (Rust memory, like
/// `Map`'s buckets — nothing heap-resident to trace), matching the
/// pre-unification `ScopeFrame`'s `HashMap<String, RtValue>`; a `Frame` box
/// is an internal constituent of some scope, never handed out as a
/// standalone language value.
#[derive(Clone, Debug)]
pub(crate) enum StructPayload {
    Fields(Vec<Value>),
    Map(HashMap<i64, Vec<(Value, Value)>>),
    Frame(HashMap<String, Value>),
    Frames(Vec<BoxId>),
}

/// A Lisp value — the runtime encoding of `Sexpr`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Value {
    /// The empty list `()` — `Sexpr::Nil`.
    Empty,
    Int(i64),
    Char(char),
    Bool(bool),
    Symbol(SymRef),
    Str(StrId),
    Cons(ConsRef),
    /// A `::`-qualified path such as `std::process::exit`, produced by the
    /// reader by splitting the token into interned symbol segments. The checker
    /// decides whether each segment names a module or a type.
    Path(PathId),
    /// A heap-resident, GC-collected boxed object — see [`BoxedObj`].
    /// `Sexpr::f64` is the first case (`f64` doesn't fit an immediate
    /// tagged word); further "Lisp-writable" runtime values (structs,
    /// closures, `HashTable<K,V>`, `Scope<V>`) are planned to migrate here
    /// too, per the `Sexpr`/`RtValue` unification plan.
    Boxed(BoxId),
}

impl Value {
    /// True for the empty list `()`.
    pub fn is_empty(&self) -> bool {
        matches!(self, Value::Empty)
    }

    pub fn is_cons(&self) -> bool {
        matches!(self, Value::Cons(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arena's per-cell cost, written down.
    ///
    /// A cell is the unit the whole heap is measured in — the default arena is
    /// 65,536 of them and `bootstrap` asks for 262,144 — so a field added here
    /// is multiplied by that. This is not a limit anyone must never exceed; it
    /// is a number that should change only when someone means to change it.
    ///
    /// The two location slots cost 8 of these bytes. They could have been one
    /// (a `u32` fits in the padding after `mark` for free), but one slot cannot
    /// hold both facts a form needs: the span of the list and the span of the
    /// element in the `car` — and the latter is the only position an interned
    /// atom can ever have. See [`Cell`]'s doc comment.
    #[test]
    fn a_cell_is_seven_words() {
        assert_eq!(std::mem::size_of::<Value>(), 16, "a tagged value is two words");
        assert_eq!(std::mem::size_of::<LocId>(), 4);
        assert_eq!(
            std::mem::size_of::<Cell>(),
            56,
            "car(16) + cdr(16) + next_free(8) + mark(1, padded) + two LocIds(8)"
        );
    }
}
