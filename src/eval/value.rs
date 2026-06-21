//! Runtime values and errors for the tree-walking interpreter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::{error, fmt};

use crate::{Path, Typed, Value};

/// A wrapper around an LLVM IR builder so [`RtValue`] can derive `PartialEq`
/// uniformly: `inkwell::builder::Builder` itself has no `PartialEq` impl
/// (unlike `Module`/`FunctionValue`/`BasicBlock`/`IntValue`, which all do),
/// so this newtype supplies one by pointer identity instead — two builder
/// values are only ever the same builder, never structurally compared, in
/// practice (no `eq` method is registered for `LlvmBuilder` — see
/// `check::registry::llvm_builder_def`).
#[cfg(feature = "compile")]
#[derive(Clone, Debug)]
pub struct LlvmBuilderHandle(pub(crate) Rc<RefCell<inkwell::builder::Builder<'static>>>);

#[cfg(feature = "compile")]
impl PartialEq for LlvmBuilderHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

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
    /// A `Vector<T>`. Same `Rc<RefCell<..>>`/Rust-ownership pattern, and the
    /// same cycle-leak trade-off, as [`RtValue::HashTable`].
    Vector(Rc<RefCell<Vec<RtValue>>>),
    /// An LLVM `Module` under construction (`compile`'s LLVM-builder
    /// bindings, see `check::registry::llvm_module_def`). Bound to the
    /// process-wide `'static` `Context` (`eval::interp::llvm_context`), so it
    /// can live in an `RtValue` with no lifetime parameter of its own.
    #[cfg(feature = "compile")]
    LlvmModule(Rc<RefCell<inkwell::module::Module<'static>>>),
    /// An LLVM IR builder positioned at some basic block.
    #[cfg(feature = "compile")]
    LlvmBuilder(LlvmBuilderHandle),
    /// A declared/defined LLVM function (`Copy` in inkwell already, unlike
    /// `Module`/`Builder` — no `Rc<RefCell<..>>` needed).
    #[cfg(feature = "compile")]
    LlvmFunction(inkwell::values::FunctionValue<'static>),
    /// An LLVM basic block (`Copy` in inkwell already).
    #[cfg(feature = "compile")]
    LlvmBasicBlock(inkwell::basic_block::BasicBlock<'static>),
    /// An LLVM IR value — `IntValue` (`i64`/`i1`, covering `i64` and `bool`)
    /// or `FloatValue` (`f64`), per [`BasicValueEnum`](inkwell::values::BasicValueEnum)
    /// — see `eval::interp::{expect_llvm_int_value,expect_llvm_float_value}`
    /// for the typed extractors callers actually use (Phase 2,
    /// [docs/TODO.md](../../docs/TODO.md)「ステップ5」: `bool` joined in
    /// Phase 2b, `f64` in Phase 2c).
    #[cfg(feature = "compile")]
    LlvmValue(inkwell::values::BasicValueEnum<'static>),
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
