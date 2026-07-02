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
//! [`SymId`] / [`StrId`]. `Float` is heap-resident too, behind [`BoxId`] (see
//! [`BoxedObj`]) — an `f64` doesn't fit alongside a tag in one 64-bit word,
//! the same reason `Str` isn't stored inline. The public surface is entirely
//! safe.

use std::fmt;

/// One cons cell as laid out in the arena.
///
/// `next_free` chains cells on the free list and is meaningful only while the
/// cell is free; `mark` is the GC mark bit, meaningful only during a sweep.
pub(crate) struct Cell {
    pub(crate) car: Value,
    pub(crate) cdr: Value,
    pub(crate) mark: bool,
    pub(crate) next_free: *mut Cell,
}

impl Cell {
    pub(crate) fn blank() -> Cell {
        Cell { car: Value::Empty, cdr: Value::Empty, mark: false, next_free: std::ptr::null_mut() }
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

/// Reference to an interned symbol in the heap's symbol table.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SymId(pub(crate) u32);

/// Reference to a string in the heap's string store.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StrId(pub(crate) u32);

/// Reference to an interned `::` path (a sequence of symbols) in the heap.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PathId(pub(crate) u32);

// `as_u32`/`from_u32`: for `typelisp-rt`'s tagged compiled-code
// representation, which embeds these as plain integer payloads (see
// `typelisp-rt`'s `encode`/`decode`).
impl SymId {
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn from_u32(v: u32) -> SymId {
        SymId(v)
    }
}

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
/// `RtValue` unification plan. `Float` is the first case, and a deliberately
/// forced one: an `f64` doesn't fit losslessly alongside a 3-bit tag in a
/// 64-bit word (unlike `Int`/`Char`/`Bool`/every interned-index variant), so
/// `Sexpr::Float` was never representable in the tagged compiled-code ABI at
/// all (`typelisp-rt`'s `encode`/`decode` used to `fatal()` on it) — boxing
/// it here (heap-resident behind a small index, exactly like `Value::Str`
/// already is) is what makes it representable, closing that gap as the
/// first proof of this mechanism.
///
/// `Struct` is the second case: `defstruct` instances, `Vector<T>`, and
/// `cons-cell<K,V>` all share this one variant rather than getting one each
/// — `type_name` plus whichever builtin method dispatches on it is what
/// gives the fields their meaning (the same "no special-cased runtime
/// shape, just a box + a name" treatment `Vector<T>` already got when it was
/// reintroduced on top of `RtValue::Struct`), not three parallel encodings
/// of the same "a name and some fields" shape.
///
/// No longer `Copy` (a `Struct`'s `Vec<Value>` owns heap memory of its own,
/// unlike `Float`'s bare `f64`) — every read site now borrows instead of
/// implicitly copying, e.g. [`super::heap::Heap::float_value`].
#[derive(Clone, Debug)]
pub(crate) enum BoxedObj {
    Float(f64),
    Struct { type_name: String, payload: StructPayload },
}

/// The fields behind a [`BoxedObj::Struct`]. `Fields` is the only shape
/// today (a fixed-length `defstruct` instance and a variable-length
/// `Vector<T>`/`cons-cell<K,V>` are both just "a `Vec<Value>`" at this
/// layer — length-checking a fixed-arity struct's field count is the
/// caller's job, same as it already is for `RtValue::Struct`). `Map`
/// (`HashTable<K,V>`) and `Frames` (`Scope<V>`) are planned additions from
/// later stages of the unification plan, not implemented yet.
#[derive(Clone, Debug)]
pub(crate) enum StructPayload {
    Fields(Vec<Value>),
}

/// A Lisp value — the runtime encoding of `Sexpr`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Value {
    /// The empty list `()` — `Sexpr::Nil`.
    Empty,
    Int(i64),
    Char(char),
    Bool(bool),
    Symbol(SymId),
    Str(StrId),
    Cons(ConsRef),
    /// A `::`-qualified path such as `std::process::exit`, produced by the
    /// reader by splitting the token into interned symbol segments. The checker
    /// decides whether each segment names a module or a type.
    Path(PathId),
    /// A heap-resident, GC-collected boxed object — see [`BoxedObj`].
    /// `Sexpr::Float` is the first case (`f64` doesn't fit an immediate
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
