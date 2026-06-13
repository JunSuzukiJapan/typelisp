//! Values and cons cells for the managed heap.
//!
//! A [`Value`] is a small `Copy` tagged value. This is the runtime encoding of
//! the reader's `Sexpr` type:
//!
//! ```text
//! read : &str -> Option<Sexpr>          (None = the empty list `()`)
//! Sexpr = Int | Float | Char | Bool | Sym | Str
//!       | Cons(Option<Sexpr>, Option<Sexpr>)
//! ```
//!
//! At the type level the empty list is `Option<Sexpr>::None`; at runtime that is
//! encoded by [`Value::Empty`]. (`Empty` is the empty-list datum, **not** the
//! removed language-level `nil`.)
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

/// Reference to an interned symbol in the heap's symbol table.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SymId(pub(crate) u32);

/// Reference to a string in the heap's string store.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StrId(pub(crate) u32);

/// A Lisp value — the runtime encoding of `Sexpr` (and `Option<Sexpr>`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Value {
    /// The empty list `()` — runtime encoding of `Option<Sexpr>::None`.
    Empty,
    Int(i64),
    Float(f64),
    Char(char),
    Bool(bool),
    Symbol(SymId),
    Str(StrId),
    Cons(ConsRef),
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
