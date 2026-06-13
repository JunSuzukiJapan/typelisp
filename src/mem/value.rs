//! Values and cons cells for the managed heap.
//!
//! A [`Value`] is a small `Copy` tagged value: either an immediate
//! (`Nil`/`Int`/`Float`/`Char`) or a reference to a cons cell living in a
//! [`Heap`](super::heap::Heap) arena. The cons reference ([`ConsRef`]) wraps a
//! raw `*mut Cell`, but that pointer is **never** dereferenced outside this
//! module — all access goes through safe `Heap` methods. Callers therefore
//! never write `unsafe`, and never see a raw pointer.

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
        Cell { car: Value::Nil, cdr: Value::Nil, mark: false, next_free: std::ptr::null_mut() }
    }
}

/// An opaque reference to a cons cell. `Copy`, compared by identity.
///
/// The inner pointer is private and only the `mem` module may dereference it.
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

/// A Lisp value. `nil` (the empty list / false) is `Value::Nil`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Value {
    Nil,
    Int(i64),
    Float(f64),
    Char(char),
    Cons(ConsRef),
}

impl Value {
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }

    pub fn is_cons(&self) -> bool {
        matches!(self, Value::Cons(_))
    }
}
