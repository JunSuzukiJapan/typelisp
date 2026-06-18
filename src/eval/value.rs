//! Runtime values and errors for the tree-walking interpreter.

use std::cell::RefCell;
use std::rc::Rc;
use std::{error, fmt};

use crate::{Path, Typed, Value};

/// A closure: a lambda body with its parameter names and the lexical environment
/// captured at creation (shared slots, so captured mutable variables persist).
#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    pub params: Vec<String>,
    pub body: Vec<Typed>,
    pub env: Vec<(String, Rc<RefCell<RtValue>>)>,
}

/// A runtime value. Data-type instances (constructors of `Option`/`Result`/
/// user structs) are represented uniformly by [`RtValue::Data`]; `Sexpr` is
/// the one exception — it is the cons/nil-bearing builtin the GC-managed cons
/// heap exists for, so its values live there instead, in [`RtValue::Sexpr`].
#[derive(Clone, Debug, PartialEq)]
pub enum RtValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(String),
    Unit,
    /// A constructor instance: the type's [`Path`], the variant index, and the
    /// evaluated field values.
    Data {
        type_name: Path,
        variant: usize,
        fields: Vec<RtValue>,
    },
    /// A `Sexpr` value (`Nil`/`Int`/`Float`/`Char`/`Bool`/`Sym`/`Str`/`Cons`),
    /// backed by the GC-managed cons heap shared with the reader rather than a
    /// Rust-heap encoding — so `cons` cells built at runtime are subject to the
    /// same mark-sweep collection as ones read from source.
    Sexpr(Value),
    /// A function value (from a `lambda` or a reified named function).
    Closure(Rc<Closure>),
    /// A built-in operator used as a function value (e.g. `+`).
    Builtin(String),
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
    Return(Box<RtValue>),
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
        }
    }
}

impl error::Error for EvalError {}
