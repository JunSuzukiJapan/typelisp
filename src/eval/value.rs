//! Runtime values and errors for the tree-walking interpreter.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::{error, fmt};

use num_bigint::BigInt;
use num_rational::BigRational;

use crate::{BoxId, Heap, Loc, Path, Type, Value};


/// Which of the interpreter's two binding-slot representations a binding
/// uses — decided *statically*, from the binding's declared type, never from
/// a value's runtime shape:
///
/// * `Heap` — the type's runtime representation is always
///   `RtValue::Sexpr(Value)` (the built-in `Sexpr`, `defstruct`/`Vector<T>`/
///   `cons-cell<K,V>` boxed structs, `HashTable<K,V>`, and — Stage 8 — a
///   `Scope<V>` whose `V` is itself in this list), so the binding lives
///   in a GC-heap `BoxedObj::Cell` the collector traces directly.
/// * `Native` — everything else: scalars (whose `mem::Value` encodings would
///   be ambiguous to decode — a cell holding `Value::Int(42)` couldn't say
///   whether it was an `i32` or a quoted `Sexpr` datum), `Str` (whose
///   `Rc::ptr_eq` `eq`-identity a `StrId` round-trip would destroy),
///   `Data`/function values/`Scope<V>` of any *other* `V` (not
///   `Value`-representable), and the
///   five LLVM FFI handle kinds (never `Value`-representable **by design** —
///   this split is what keeps the compiler-internal LLVM universe strictly
///   out of the GC heap).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Heap,
    Native,
}

/// A mutable variable slot (shared so `setf` mutations are visible to every
/// holder of the binding, e.g. across `loop` iterations). Two-tier, routed
/// by the binding's *static type* ([`SlotKind`], never a value's runtime
/// shape):
///
/// * `Heap` — a GC-heap `BoxedObj::Cell` (see `Heap::alloc_cell`), for
///   bindings whose runtime representation is always `RtValue::Sexpr`. The
///   collector traces the cell (and thus its current contents) directly;
///   [`Interp::sync_roots`] only has to push one root per live cell. The
///   `Rc` wrapper exists purely so cell liveness is observable through a
///   `Weak` (a bare `BoxId` is `Copy` and its drop invisible) — the cell
///   payload itself lives in the heap, not behind this `Rc`.
/// * `Native` — the classic `Rc<RefCell<RtValue>>`, for every type whose
///   values a `mem::Value` can't (or must not) carry — scalars, `Str`,
///   `Data`, function values, `Scope<V>` of a native-repr `V`, and the five
///   compiler-internal LLVM
///   handle kinds, which this split keeps out of the GC heap *structurally*:
///   a `Slot::Heap` can only ever be created from an `RtValue::Sexpr`.
///
/// (`PartialEq`/`Debug` exist only for `Closure`'s own derives: a `Heap`
/// slot compares by cell identity (`BoxId`), a `Native` one by contents —
/// the same contents-comparison `Rc<RefCell<..>>` always had here.)
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    Heap(Rc<BoxId>),
    Native(Rc<RefCell<RtValue>>),
    /// A GC-heap `BoxedObj::Cell` for a binding whose static type is
    /// otherwise `Native` (a scalar, `Str`, `Fn`, ...) — closure
    /// unification Stage 7's answer to "a nested `lambda`/`labels` captures
    /// this name, and the capture must be visible to *compiled* code too".
    /// The underlying cell (`Heap::alloc_cell`/`cell_get`/`cell_set`) is
    /// already generic over any `Value`, not `Sexpr`-only — `Slot::Heap`'s
    /// restriction to `RtValue::Sexpr` is this wrapper's own choice, not the
    /// heap's, so this variant simply carries the declared `Type` alongside
    /// the cell and routes get/set through the same
    /// encode/decode(`rtvalue_to_struct_field`/`decode_field_typed`) a
    /// `defstruct` field already uses — a scalar-typed capture becomes a
    /// tagged `i64` `rt_cell_get`/`rt_cell_set` on the compiled side can
    /// read/write directly, without requiring a heap-repr type. Chosen only
    /// at binding sites `freevars::names_captured_by_nested` marks as
    /// captured-by-a-nested-closure (see `Interp::apply`/`Expr::Let`); every
    /// other `Native`-typed binding is untouched.
    TypedCell(Rc<BoxId>, Type),
}

impl Slot {
    /// The binding's current value.
    pub fn get(&self, heap: &Heap) -> RtValue {
        match self {
            Slot::Heap(id) => RtValue::Sexpr(heap.cell_get(**id)),
            Slot::Native(rc) => rc.borrow().clone(),
            Slot::TypedCell(id, ty) => super::interp::decode_field_typed(heap, heap.cell_get(**id), ty),
        }
    }

    /// Overwrites the binding (`setf`). Writing a non-`Sexpr` value into a
    /// `Heap` slot is an internal invariant violation — the checker
    /// guarantees a binding's static type (and hence its runtime
    /// representation) never changes over its lifetime — reported loudly
    /// rather than silently mis-stored.
    pub fn set(&self, heap: &mut Heap, v: RtValue) -> Result<(), EvalError> {
        match self {
            Slot::Heap(id) => match v {
                RtValue::Sexpr(val) => {
                    heap.cell_set(**id, val);
                    Ok(())
                }
                other => Err(EvalError::Internal(format!(
                    "heap-cell binding assigned a non-Sexpr value: {:?}",
                    other
                ))),
            },
            Slot::Native(rc) => {
                *rc.borrow_mut() = v;
                Ok(())
            }
            Slot::TypedCell(id, _ty) => {
                let encoded = super::interp::rtvalue_to_struct_field(heap, &v)?;
                heap.cell_set(**id, encoded);
                Ok(())
            }
        }
    }
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
    /// A `bignum` value (arbitrary-precision integer, CL's bignum). `Rc`-wrapped
    /// for the same reason `Str` is (see that variant's doc comment): a
    /// binding read/clone should be a cheap pointer/refcount bump, not a deep
    /// copy of however many limbs the integer holds.
    Bignum(Rc<BigInt>),
    /// A `ratio` value (exact rational, CL's ratio), same `Rc`-wrapping
    /// rationale as [`RtValue::Bignum`].
    Ratio(Rc<BigRational>),
    Bool(bool),
    Char(char),
    /// `Rc<str>`, not a plain owned `String` — this language's `string`
    /// values are immutable, and cloning a `String` on every variable read
    /// (the interpreter's ordinary evaluation pattern) would otherwise
    /// silently deep-copy the buffer each time, leaving no way to observe
    /// "the same string object" ever again — not even `(let ((s "hi"))
    /// (eq s s))`, since each read of `s` would hand back an independently
    /// allocated copy. `Rc::clone` is a pointer/refcount bump instead, so
    /// two reads of the same binding stay the *same* object, and `eq`'s
    /// `Rc::ptr_eq` (`eval_builtin_method`'s `"eq"` case for `Str`) can
    /// give this type genuine CL identity semantics rather than falling
    /// back to (incorrect) content comparison or a meaningless "always
    /// false". Content comparison itself now lives under `eql`/`equal`/
    /// `equalp` instead — see `docs/cl-equivalence-catalog.md`'s eq/eql/
    /// equal/equalp section.
    Str(Rc<str>),
    Unit,
    /// An enum value (`Option`/`Result`/user `defenum` instance) **of a
    /// native-repr instantiation only**: one whose variant fields include a
    /// type the GC heap cannot store — an LLVM handle, a native-`V`
    /// `Scope`, a function type (whose value may be a [`RtValue::Builtin`],
    /// not a heap closure box), or `Unit` — e.g. the `Option<llvm-value>`
    /// the (typelisp-hosted) compiler body's `scope::get` returns. Every
    /// *other* enum instantiation is a heap `BoxedObj::Enum` behind
    /// [`RtValue::Sexpr`] since the enum-representation unification; the
    /// tier is decided statically from the concrete instantiated type
    /// (`Interp::enum_ty_is_native` / the checker's `is_heap_repr` twin),
    /// never from a value's shape — the exact split [`RtValue::Scope`]
    /// already established for `Scope<V>`, and for the same reason: an
    /// LLVM handle can never reach the GC heap through any container.
    Data {
        type_name: Path,
        variant: usize,
        fields: Vec<RtValue>,
    },
    /// A `Sexpr` value (`Nil`/`Int`/`Float`/`Char`/`Bool`/`Sym`/`Str`/`Cons`),
    /// backed by the GC-managed cons heap shared with the reader rather than a
    /// Rust-heap encoding — so `cons` cells built at runtime are subject to the
    /// same mark-sweep collection as ones read from source. Since the
    /// `Sexpr`/`RtValue` unification's Stage 2, this is also where a
    /// `defstruct` instance/`Vector<T>`/`cons-cell<K,V>` lives: each is a
    /// `Value::Boxed` pointing at a `BoxedObj::Struct` (see `crate::mem`),
    /// wrapped in this same variant rather than a dedicated `RtValue::Struct`
    /// — `heap.is_struct`/`struct_type_name`/`struct_field`/etc. distinguish
    /// it from a boxed float or a genuine quoted `Sexpr` datum at each read
    /// site (`interp.rs`'s `expect_struct_box`/`decode_field_typed`).
    /// This is also where a *closure* lives: a `Value::Boxed` pointing at a
    /// `BoxedObj::CompiledClosure` (a JIT/AOT function pointer + its captured
    /// environment) — so closure identity and GC tracing come from the same
    /// heap machinery as every other boxed value, and no dedicated
    /// `RtValue::Closure` variant exists. (The interpreted `BoxedObj::Closure`
    /// that stood here through Stages 6b–8b was removed in interp-closure
    /// removal Stage 8c; every closure is compiled now.)
    /// Since the enum-representation unification this is also where an
    /// *enum value* (`Option`/`Result`/`Error`/user `defenum`) lives: a
    /// `Value::Boxed` pointing at a `BoxedObj::Enum` (variant index +
    /// fields) — the dedicated `RtValue::Data` variant is gone, and the
    /// same one heap object is what compiled code reads/writes through
    /// `rt_data_*`.
    /// Since the built-in-function-value unification this is also where a
    /// *built-in used as a function value* lives (`gensym` passed to a
    /// higher-order function, `+` reified as `i32::+`): a `Value::Boxed`
    /// pointing at a `BoxedObj::Builtin` (an optional receiver type + the
    /// name), replacing the dedicated `Builtin(String)`/
    /// `BuiltinMethod(Path, String)` variants — so *every* `Type::Fn` value
    /// is a heap box now, which is what makes `Fn` storable in an enum field.
    Sexpr(Value),
}

impl PartialEq for RtValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RtValue::Int(a), RtValue::Int(b)) => a == b,
            (RtValue::Float(a), RtValue::Float(b)) => a == b,
            (RtValue::Bignum(a), RtValue::Bignum(b)) => a == b,
            (RtValue::Ratio(a), RtValue::Ratio(b)) => a == b,
            (RtValue::Bool(a), RtValue::Bool(b)) => a == b,
            (RtValue::Char(a), RtValue::Char(b)) => a == b,
            (RtValue::Str(a), RtValue::Str(b)) => a == b,
            (RtValue::Unit, RtValue::Unit) => true,
            (
                RtValue::Data { type_name: tn1, variant: v1, fields: f1 },
                RtValue::Data { type_name: tn2, variant: v2, fields: f2 },
            ) => tn1 == tn2 && v1 == v2 && f1 == f2,
            // Closures compare as the `Sexpr` boxes they are — `Value`'s
            // own `Boxed(id) == Boxed(id)`, i.e. identity, matching the old
            // dedicated variant's `Rc` semantics.
            (RtValue::Sexpr(a), RtValue::Sexpr(b)) => a == b,
            // Any other pair (including a mismatch of variants) is unequal.
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
