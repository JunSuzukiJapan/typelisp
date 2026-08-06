//! Runtime values and errors for the tree-walking interpreter.

use std::rc::Rc;
use std::{error, fmt};

use crate::{BoxId, Heap, Loc, Value};


/// A mutable variable slot, shared so `setf` mutations are visible to every
/// holder of the binding (e.g. across `loop` iterations).
///
/// One representation: a GC-heap `BoxedObj::Cell` (see `Heap::alloc_cell`).
/// This used to be a three-way split routed by the binding's static type —
/// a `Heap` cell for types whose runtime form was already a heap `Value`, a
/// Rust-side `Rc<RefCell<RtValue>>` for the rest, and a `TypedCell` for a
/// scalar a nested closure captured. The split existed only because
/// `RtValue` was a second value universe: a cell holding `Value::Int(42)`
/// could not say whether it meant an `i32` or a quoted `Sexpr` datum, so a
/// declared `Type` had to ride along to pick the `RtValue` wrapper back out.
/// With `RtValue` gone there is no wrapper to pick and the encode/decode pair
/// is the identity, so every binding is simply a cell.
///
/// That also removes `Interp::sync_roots`: the collector walks its own
/// `cell_registry` on *every* collection, so a cell-backed binding is rooted
/// without the interpreter pushing anything. The `Rc<BoxId>` wrapper is what
/// makes cell liveness observable through that registry's `Weak` (a bare
/// `BoxId` is `Copy`, and its drop invisible); the payload lives in the heap,
/// not behind this `Rc`.
///
/// (`PartialEq`/`Debug` exist only for derives elsewhere; slots compare by
/// cell identity.)
#[derive(Clone, Debug, PartialEq)]
pub struct Slot(Rc<BoxId>);

impl Slot {
    pub fn new(id: Rc<BoxId>) -> Slot {
        Slot(id)
    }

    /// The cell this binding lives in — for the JIT, which hands compiled
    /// code the cell directly so a capture is shared rather than copied.
    pub fn cell(&self) -> &Rc<BoxId> {
        &self.0
    }

    /// The binding's current value.
    pub fn get(&self, heap: &Heap) -> Value {
        heap.cell_get(*self.0)
    }

    /// Overwrites the binding (`setf`).
    pub fn set(&self, heap: &mut Heap, v: Value) -> Result<(), EvalError> {
        heap.cell_set(*self.0, v);
        Ok(())
    }
}


/// A runtime error. `Panic` is a deliberate `panic`; `Break`/`Return` are not
/// errors at all but internal non-local-exit signals (`break`/`return`
/// unwinding to the nearest enclosing loop), reusing `Result`'s `?`-propagation
/// to implement them; the checker guarantees they are always caught by a
/// `while`/`loop`/etc. before they could reach [`Interp::exec`](super::Interp)
/// — surfacing there would be a checker/interpreter bug. The remaining
/// variants are bugs that a well-typed program should not produce.
#[derive(Clone, Debug, PartialEq)]
pub enum EvalError {
    /// A `panic` reached at runtime, carrying its message.
    Panic(String),
    /// Reference to an unbound variable (should not happen post-checking).
    Unbound(String),
    /// Call to a function/method with no definition.
    NoSuchFunction(String),
    /// An internal invariant was violated (a checker/interpreter bug).
    Internal(String),
    /// `break`: unwinding to the nearest enclosing loop, no value.
    Break,
    /// `return value`: unwinding to the nearest enclosing loop with `value`.
    Return(Box<Value>),
    /// A runtime error carrying the source location where it occurred. Wraps
    /// the underlying error; [`fmt::Display`] prefixes it with `file:line:col`.
    /// Built only via [`EvalError::at`], which never wraps the `Break`/`Return`
    /// control-flow signals (they must stay pattern-matchable by the loop that
    /// catches them) nor double-wraps an already-located error.
    At(Loc, Box<EvalError>),
}

impl EvalError {
    /// Attach a source location to this error. The `Break`/`Return` non-local-
    /// exit signals are returned unchanged — they are control flow, not errors,
    /// and the loop that catches them matches on the bare variant. An
    /// already-located error also keeps its original (innermost) location.
    pub fn at(self, loc: Loc) -> EvalError {
        match self {
            EvalError::Break | EvalError::Return(_) | EvalError::At(..) => self,
            other => EvalError::At(loc, Box::new(other)),
        }
    }

    /// The underlying error with any location wrapper(s) stripped.
    pub fn kind(&self) -> &EvalError {
        match self {
            EvalError::At(_, inner) => inner.kind(),
            other => other,
        }
    }

    /// Consume this error and return its underlying kind with any location
    /// wrapper(s) stripped — the owned counterpart of [`EvalError::kind`], for
    /// code (mainly tests) that pattern-matches the error by value.
    pub fn into_kind(self) -> EvalError {
        match self {
            EvalError::At(_, inner) => inner.into_kind(),
            other => other,
        }
    }

    /// The source location attached to this error, if any.
    pub fn loc(&self) -> Option<&Loc> {
        match self {
            EvalError::At(loc, _) => Some(loc),
            _ => None,
        }
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EvalError::Panic(m) => write!(f, "panic: {}", m),
            EvalError::Unbound(n) => write!(f, "unbound variable: {}", n),
            EvalError::NoSuchFunction(n) => write!(f, "no such function: {}", n),
            EvalError::Internal(m) => write!(f, "internal error: {}", m),
            EvalError::Break => write!(f, "internal error: break escaped its loop"),
            EvalError::Return(_) => write!(f, "internal error: return escaped its loop"),
            EvalError::At(loc, inner) => write!(f, "{}: {}", loc, inner),
        }
    }
}

impl error::Error for EvalError {}
