//! Runtime values and errors for the tree-walking interpreter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::{error, fmt};

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue};
use num_bigint::BigInt;
use num_rational::BigRational;

use crate::{BoxId, Heap, Loc, Path, Type, Value};

/// One `Scope<V>` frame — see [`RtValue::Scope`]'s doc comment.
pub type ScopeFrame = Rc<RefCell<HashMap<String, RtValue>>>;

/// The Rust-native `Scope<V>` representation behind [`RtValue::Scope`] — a
/// stack of shared frames, encapsulated so the scope-chain invariants
/// (frames are shared by `Rc`, never copied entry-by-entry; the frame
/// *stack* is per-scope) live behind this method surface instead of being
/// re-derived at every call site. The mirror of the heap representation's
/// `Heap::scope_*` method family (`StructPayload::Frames`, for heap-repr
/// `V` — see [`RtValue::Scope`]'s doc comment for how the two families
/// split); the six methods here are the native halves of the same six
/// builtin methods.
#[derive(Clone, Debug)]
pub struct NativeScope {
    frames: Rc<RefCell<Vec<ScopeFrame>>>,
}

impl NativeScope {
    /// One fresh empty frame, already pushed — the frame a function's own
    /// parameters/captures bind into (`compiler.rs`'s `bind-params`/
    /// `bind-captures`), matching `HashTable::new`'s "ready to use
    /// immediately" convention.
    pub fn new() -> NativeScope {
        NativeScope { frames: Rc::new(RefCell::new(vec![Rc::new(RefCell::new(HashMap::new()))])) }
    }

    /// Shares every current frame (by `Rc::clone` — a pointer copy per
    /// frame, never copying a frame's own entries) into a brand new scope
    /// with its own independent stack. `compiler.rs`'s `compile-labels`
    /// uses this to give each `labels` def's own new lexical scope every
    /// enclosing scope's frames "for free" before pushing that def's own
    /// fresh parameter frame on top.
    pub fn clone_frames(&self) -> NativeScope {
        NativeScope { frames: Rc::new(RefCell::new(self.frames.borrow().clone())) }
    }

    pub fn push_frame(&self) {
        self.frames.borrow_mut().push(Rc::new(RefCell::new(HashMap::new())));
    }

    pub fn pop_frame(&self) {
        self.frames.borrow_mut().pop();
    }

    /// Searches from the most-recently-pushed frame outward — see
    /// `compiler.rs`'s module doc comment for why this terminates at the
    /// scope's own first (function-entry) frame rather than reaching into
    /// an *enclosing* function's scope (a different value entirely; there
    /// is nothing further to search here).
    pub fn get(&self, name: &str) -> Option<RtValue> {
        for frame in self.frames.borrow().iter().rev() {
            if let Some(v) = frame.borrow().get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    /// Always writes into the most-recently-pushed frame — never an
    /// enclosing (shared, via [`clone_frames`](Self::clone_frames)) one, so
    /// a `let`/`labels` def's own bindings never leak into whatever scope
    /// it borrowed frames from. Errors if every frame has been popped —
    /// only reachable through an unbalanced `pop-frame`, kept a catchable
    /// evaluation error (the same one the heap representation's guard
    /// reports) rather than a panic.
    pub fn set(&self, name: &str, value: RtValue) -> Result<(), EvalError> {
        let frames = self.frames.borrow();
        let top = frames.last().ok_or_else(|| EvalError::Internal("Scope::set: no frame to write into".into()))?;
        top.borrow_mut().insert(name.to_string(), value);
        Ok(())
    }

    /// The current frame count — `#<scope depth=N>`'s display number.
    pub fn depth(&self) -> usize {
        self.frames.borrow().len()
    }

    /// Visits every value bound in every frame — `collect_sexpr_roots`'s
    /// hook for rooting the `Sexpr` values a native scope's frames may hold
    /// (see `Interp::sync_roots`), without exposing the frame storage.
    pub fn for_each_value(&self, mut f: impl FnMut(&RtValue)) {
        for frame in self.frames.borrow().iter() {
            for v in frame.borrow().values() {
                f(v);
            }
        }
    }
}

impl Default for NativeScope {
    fn default() -> Self {
        NativeScope::new()
    }
}

/// Content comparison, frame by frame — the same semantics the raw
/// `Rc<RefCell<Vec<ScopeFrame>>>` payload's `PartialEq` arm always had.
impl PartialEq for NativeScope {
    fn eq(&self, other: &Self) -> bool {
        *self.frames.borrow() == *other.frames.borrow()
    }
}

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
    Sexpr(Value),
    /// A built-in *free* function used as a function value (e.g. `gensym`).
    Builtin(String),
    /// A built-in *instance method* used as a function value (e.g. `+` on
    /// `i32` — see [`Expr::MethodRef`](crate::Expr::MethodRef)).
    BuiltinMethod(Path, String),
    /// A `Scope<V>` **of a native-repr `V`**: a stack of frames (each an
    /// ordinary `String`-keyed map),
    /// used by the (typelisp-hosted) compiler body (`src/compiler.rs`) to
    /// track lexically-nested name resolution (`env`/`fn-env`) the way a
    /// real interpreter's environment chain would — see that module's doc
    /// comment for the "list of scopes" model this implements. A frame is
    /// `Rc<RefCell<HashMap<..>>>` specifically so a "clone the frame list" operation
    /// is just a `Vec` of cloned `Rc`s — cheap (pointer copies, no per-entry
    /// work) and exactly what lets a `labels` def's own new scope start from
    /// every enclosing scope's frames without copying their contents.
    ///
    /// Since the unification's Stage 8 this variant backs only scopes whose
    /// element type `V` is *not* itself heap-repr — above all the
    /// compiler's `Scope<llvm-value>`/`Scope<llvm-function>`, whose LLVM
    /// handles a `mem::Value` cannot carry. A `Scope<V>` of a heap-repr `V`
    /// (`Sexpr`/boxed structs/`HashTable`/such a scope itself) is instead an
    /// `RtValue::Sexpr` boxed `StructPayload::Frames` scope, with the same
    /// share-frames-by-reference semantics; which family a value belongs to
    /// is decided statically from `V` (`Interp::scope_is_heap`), never from
    /// the value's shape. The payload is the [`NativeScope`] struct — the
    /// frame-stack invariants live behind its method surface.
    Scope(NativeScope),
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
            (RtValue::Builtin(a), RtValue::Builtin(b)) => a == b,
            (RtValue::BuiltinMethod(p1, m1), RtValue::BuiltinMethod(p2, m2)) => {
                p1 == p2 && m1 == m2
            }
            (RtValue::Scope(a), RtValue::Scope(b)) => a == b,
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
