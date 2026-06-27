//! Runtime values and errors for the tree-walking interpreter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::{error, fmt};

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue};

use crate::{Path, Typed, Value};

/// One `Scope<V>` frame — see [`RtValue::Scope`]'s doc comment.
pub type ScopeFrame = Rc<RefCell<HashMap<String, RtValue>>>;

/// A `HashTable<K,V>` key. Restricted to the scalar `RtValue` variants with a
/// natural, total `Eq`/`Hash` (notably excluding `Float` — `f64` has no `Eq`
/// because of `NaN` — and any reference-counted variant, where a structural
/// notion of equality wouldn't be meaningful). See [`HashKey::from_rtvalue`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HashKey {
    Int(i64),
    Bool(bool),
    Char(char),
    Str(String),
}

impl HashKey {
    /// Converts a key argument at the `HashTable` method boundary. The type
    /// checker can't express a "hashable" bound (no traits in this language),
    /// so an unsupported key type — `Float`/`Closure`/`HashTable` itself, or a
    /// user `Data` instance — is a runtime panic, the same fallback used for
    /// e.g. `car`/`cdr` on a non-`Cons` `Sexpr`.
    pub fn from_rtvalue(v: &RtValue) -> Result<HashKey, EvalError> {
        match v {
            RtValue::Int(n) => Ok(HashKey::Int(*n)),
            RtValue::Bool(b) => Ok(HashKey::Bool(*b)),
            RtValue::Char(c) => Ok(HashKey::Char(*c)),
            RtValue::Str(s) => Ok(HashKey::Str(s.clone())),
            other => Err(EvalError::Panic(format!("HashTable: unsupported key type {:?}", other))),
        }
    }
}

/// A closure: a lambda body with its parameter names and the lexical environment
/// captured at creation (shared slots, so captured mutable variables persist).
#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    pub params: Vec<String>,
    pub body: Vec<Typed>,
    pub env: Vec<(String, Rc<RefCell<RtValue>>)>,
}

/// A `defstruct` instance's fields, behind the `Rc<RefCell<..>>` that gives
/// [`RtValue::Struct`] its reference (not value) semantics. `type_name` is a
/// plain `String`, not a [`Path`] like [`RtValue::Data`]'s — name resolution
/// is finished by check time, so a runtime value only ever needs this for
/// display (`Debug`/the REPL printer), never to look anything back up (see
/// that field's doc comment for why `Data` itself doesn't actually need a
/// `Path` either). Fields are positional, not named — `Checker::check_defstruct`
/// resolves a field name to its index once, at check time (`Expr::FieldGet`/
/// `FieldSet`), so the runtime representation doesn't need to carry names.
#[derive(Clone, Debug, PartialEq)]
pub struct StructData {
    pub type_name: String,
    pub fields: Vec<RtValue>,
}

/// A runtime value. Data-type instances (constructors of `Option`/`Result`/
/// user structs) are represented uniformly by [`RtValue::Data`]; `Sexpr` is
/// the one exception — it is the cons/nil-bearing builtin the GC-managed cons
/// heap exists for, so its values live there instead, in [`RtValue::Sexpr`].
///
/// `PartialEq` is hand-written, not derived: the `Llvm*` variants wrap
/// inkwell types that don't implement it (and structural equality wouldn't
/// be meaningful for them anyway — see [`RtValue::eq`]).
#[derive(Clone, Debug)]
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
    /// A built-in *free* function used as a function value (e.g. `gensym`).
    Builtin(String),
    /// A built-in *instance method* used as a function value (e.g. `+` on
    /// `i32` — see [`Expr::MethodRef`](crate::Expr::MethodRef)).
    BuiltinMethod(Path, String),
    /// A `HashTable<K,V>`. Lives in ordinary Rust-managed memory
    /// (`Rc<RefCell<..>>`, reclaimed by reference counting), not the
    /// GC-managed cons heap — the same pattern [`RtValue::Data`] and
    /// [`Closure::env`]'s captured slots already use; see
    /// `crate::eval::interp::collect_sexpr_roots` for how a `Sexpr` value
    /// nested inside one stays rooted. A `HashTable` holding itself (directly
    /// or through a cycle of values) leaks rather than being collected —
    /// the same accepted trade-off `Closure`'s captured-slot cycles already
    /// have; mark-sweep cycle collection is deliberately only for the cons
    /// heap (`Sexpr`/`cons`/strings).
    HashTable(Rc<RefCell<HashMap<HashKey, RtValue>>>),
    /// A `Scope<V>`: a stack of frames (each an ordinary `String`-keyed map),
    /// used by the (typelisp-hosted) compiler body (`src/compiler.rs`) to
    /// track lexically-nested name resolution (`env`/`fn-env`) the way a
    /// real interpreter's environment chain would — see that module's doc
    /// comment for the "list of scopes" model this implements. A frame is
    /// `Rc<RefCell<HashMap<..>>>` (the same representation `HashTable`
    /// already uses) specifically so a "clone the frame list" operation
    /// is just a `Vec` of cloned `Rc`s — cheap (pointer copies, no per-entry
    /// work) and exactly what lets a `labels` def's own new scope start from
    /// every enclosing scope's frames without copying their contents.
    Scope(Rc<RefCell<Vec<ScopeFrame>>>),
    /// A `defstruct` instance — mutable, reference-identity-bearing, unlike
    /// `Data`'s value semantics (see [`StructData`]'s doc comment for why
    /// this needed its own variant rather than reusing `Data`). Lives in
    /// ordinary Rust-managed memory, the same `Rc<RefCell<..>>` pattern as
    /// `HashTable` above (and the same accepted cycle-leak trade-off).
    Struct(Rc<RefCell<StructData>>),
    /// An in-progress LLVM module being built by the (typelisp-hosted)
    /// compiler. `Rc<RefCell<..>>` because `inkwell::module::Module` owns
    /// the underlying LLVM module and isn't `Clone` (dropping it disposes
    /// the LLVM-side object).
    LlvmModule(Rc<RefCell<Module<'static>>>),
    /// An LLVM IR builder positioned at some point in a function. Same
    /// `Rc<RefCell<..>>` reasoning as `LlvmModule`.
    LlvmBuilder(Rc<RefCell<Builder<'static>>>),
    /// A declared/defined LLVM function. Inkwell's value/block handles
    /// (unlike `Module`/`Builder`) are cheap `Copy` references into the
    /// owning module, not separately-owned resources.
    LlvmFunction(FunctionValue<'static>),
    LlvmBasicBlock(BasicBlock<'static>),
    LlvmValue(BasicValueEnum<'static>),
}

impl PartialEq for RtValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RtValue::Int(a), RtValue::Int(b)) => a == b,
            (RtValue::Float(a), RtValue::Float(b)) => a == b,
            (RtValue::Bool(a), RtValue::Bool(b)) => a == b,
            (RtValue::Char(a), RtValue::Char(b)) => a == b,
            (RtValue::Str(a), RtValue::Str(b)) => a == b,
            (RtValue::Unit, RtValue::Unit) => true,
            (
                RtValue::Data { type_name: tn1, variant: v1, fields: f1 },
                RtValue::Data { type_name: tn2, variant: v2, fields: f2 },
            ) => tn1 == tn2 && v1 == v2 && f1 == f2,
            (RtValue::Sexpr(a), RtValue::Sexpr(b)) => a == b,
            (RtValue::Closure(a), RtValue::Closure(b)) => a == b,
            (RtValue::Builtin(a), RtValue::Builtin(b)) => a == b,
            (RtValue::BuiltinMethod(p1, m1), RtValue::BuiltinMethod(p2, m2)) => {
                p1 == p2 && m1 == m2
            }
            (RtValue::HashTable(a), RtValue::HashTable(b)) => *a.borrow() == *b.borrow(),
            (RtValue::Scope(a), RtValue::Scope(b)) => *a.borrow() == *b.borrow(),
            (RtValue::Struct(a), RtValue::Struct(b)) => *a.borrow() == *b.borrow(),
            // Compiler-internal LLVM handles have no meaningful structural
            // equality, and inkwell's types don't implement `PartialEq`
            // anyway — they (and any other non-matching pair) fall through.
            _ => false,
        }
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
