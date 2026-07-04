//! Runtime values and errors for the tree-walking interpreter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::{error, fmt};

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue};

use crate::{BoxId, Heap, Path, Typed, Value};

/// One `Scope<V>` frame — see [`RtValue::Scope`]'s doc comment.
pub type ScopeFrame = Rc<RefCell<HashMap<String, RtValue>>>;

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
/// (`PartialEq`/`Debug` exist only for `Closure`'s own derives: a `Heap`
/// slot compares by cell identity (`BoxId`), a `Native` one by contents —
/// the same contents-comparison `Rc<RefCell<..>>` always had here.)
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    Heap(Rc<BoxId>),
    Native(Rc<RefCell<RtValue>>),
}

impl Slot {
    /// The binding's current value.
    pub fn get(&self, heap: &Heap) -> RtValue {
        match self {
            Slot::Heap(id) => RtValue::Sexpr(heap.cell_get(**id)),
            Slot::Native(rc) => rc.borrow().clone(),
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
        }
    }
}

/// Where one of a closure's captured bindings lives — the two-tier [`Slot`]
/// split carried through capture:
///
/// * `Heap(i)` — the capture is a heap cell; index `i` into the closure
///   box's own `env: Vec<Value>` (`BoxedObj::Closure`), which is the part
///   the GC traces.
/// * `Native(rc)` — the capture is a `Native` slot (a scalar, an LLVM
///   handle, ...); held right here in the side table, invisible to the GC
///   by construction.
///
/// `layout` (below) keeps name → capture in original environment order, so
/// shadowing resolves exactly as it did in the defining scope.
#[derive(Clone, Debug)]
pub enum Capture {
    Heap(usize),
    Native(Rc<RefCell<RtValue>>),
}

/// The interpreter-side half of a closure — everything `crate::mem` cannot
/// hold: the checked body, the parameter names/slot kinds, and the `Native`
/// captures. Lives in `Interp`'s side table keyed by the closure box's
/// `body_token` (`BoxedObj::Closure`), and is dropped when the GC reports
/// the box swept (`Heap::take_dead_closure_tokens`) — which releases the
/// `Native` captures' `Rc`s (LLVM handles included), so the side table can
/// never leak what the heap already freed.
#[derive(Debug)]
pub struct ClosureBody {
    pub params: Vec<(String, SlotKind)>,
    pub body: Vec<Typed>,
    pub layout: Vec<(String, Capture)>,
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
    /// same mark-sweep collection as ones read from source. Since the
    /// `Sexpr`/`RtValue` unification's Stage 2, this is also where a
    /// `defstruct` instance/`Vector<T>`/`cons-cell<K,V>` lives: each is a
    /// `Value::Boxed` pointing at a `BoxedObj::Struct` (see `crate::mem`),
    /// wrapped in this same variant rather than a dedicated `RtValue::Struct`
    /// — `heap.is_struct`/`struct_type_name`/`struct_field`/etc. distinguish
    /// it from a boxed float or a genuine quoted `Sexpr` datum at each read
    /// site (`interp.rs`'s `expect_struct_box`/`decode_field_typed`).
    /// Since Stage 6b this is also where a *closure* lives: a
    /// `Value::Boxed` pointing at a `BoxedObj::Closure` (heap-cell captures
    /// + a token into `Interp`'s `ClosureBody` side table) — so closure
    /// identity, GC tracing, and cycle collection (`labels`) all come from
    /// the same heap machinery as every other boxed value, and no dedicated
    /// `RtValue::Closure` variant exists anymore.
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
    /// the value's shape.
    Scope(Rc<RefCell<Vec<ScopeFrame>>>),
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
            // Closures compare as the `Sexpr` boxes they are — `Value`'s
            // own `Boxed(id) == Boxed(id)`, i.e. identity, matching the old
            // dedicated variant's `Rc` semantics.
            (RtValue::Sexpr(a), RtValue::Sexpr(b)) => a == b,
            (RtValue::Builtin(a), RtValue::Builtin(b)) => a == b,
            (RtValue::BuiltinMethod(p1, m1), RtValue::BuiltinMethod(p2, m2)) => {
                p1 == p2 && m1 == m2
            }
            (RtValue::Scope(a), RtValue::Scope(b)) => *a.borrow() == *b.borrow(),
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
