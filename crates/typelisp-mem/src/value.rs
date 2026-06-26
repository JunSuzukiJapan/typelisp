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
//! [`SymId`] / [`StrId`]. The public surface is entirely safe.

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

/// A Lisp value — the runtime encoding of `Sexpr`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Value {
    /// The empty list `()` — `Sexpr::Nil`.
    Empty,
    Int(i64),
    Float(f64),
    Char(char),
    Bool(bool),
    Symbol(SymId),
    Str(StrId),
    Cons(ConsRef),
    /// A `::`-qualified path such as `std::process::exit`, produced by the
    /// reader by splitting the token into interned symbol segments. The checker
    /// decides whether each segment names a module or a type.
    Path(PathId),
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
