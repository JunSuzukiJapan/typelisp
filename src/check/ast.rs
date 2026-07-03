//! The typed AST produced by the checker.
//!
//! Checking lowers a read `Sexpr` ([`Value`](crate::Value)) directly into a
//! [`Typed`] tree: every node carries the [`Type`] it was checked at, so the
//! later interpreter (step 4) can walk this tree without re-deriving types.

use crate::{Path, Type};

/// An expression node annotated with its checked type.
#[derive(Clone, Debug, PartialEq)]
pub struct Typed {
    pub expr: Expr,
    pub ty: Type,
}

/// One function definition inside `Expr::Labels`: its name, typed
/// parameters, and checked body.
pub type LabelDef = (String, Vec<(String, Type)>, Vec<Typed>);

/// An expression. Children are [`Typed`] so the whole tree stays annotated.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(String),
    /// The unit value `()`.
    Unit,
    /// A reference to a bound (local) variable.
    Var(String),
    /// A reference to a global variable/constant, by its [`Path`].
    Global(Path),
    /// A named free function used as a value (reified into a function value).
    FnRef(Path),
    /// A named *instance* method used as a value, e.g. passing `+` (an `i32`
    /// instance method, see `registry::int_assoc`) where a `(fn (i32 i32)
    /// i32)` is expected — the receiver type is resolved from that expected
    /// function type at the use site (`Checker::method_value`), since there's
    /// no receiver expression to dispatch on.
    MethodRef { type_name: Path, method: String },
    /// `(if cond then else)`.
    If(Box<Typed>, Box<Typed>, Box<Typed>),
    /// `(let ((name val)...) body...)` — bindings, then a body sequence.
    Let(Vec<(String, Typed)>, Vec<Typed>),
    /// A call to a free function, identified by its fully-qualified [`Path`].
    Call(Path, Vec<Typed>),
    /// An anonymous function `(lambda (params) ret body...)`. Its type is
    /// [`Type::Fn`](crate::Type).
    Lambda { params: Vec<(String, Type)>, body: Vec<Typed> },
    /// `(labels ((name (params) ret body...)...) body...)`: mutually (and
    /// self-) recursive local function definitions. Unlike `lambda`, each
    /// function's body — and the trailing `body` — sees every name in `defs`
    /// in scope, so they can call themselves or each other. Evaluated by
    /// giving each function a placeholder slot before any closure is built,
    /// then having every closure capture an environment containing all the
    /// slots (including its own) and only afterwards filling each slot in —
    /// the same `Rc<RefCell<..>>` self-reference trick the interpreter has no
    /// existing precedent for elsewhere (`Closure`'s captured environment is
    /// otherwise acyclic).
    Labels { defs: Vec<LabelDef>, body: Vec<Typed> },
    /// Apply a function *value* (a closure) to arguments.
    Apply(Box<Typed>, Vec<Typed>),
    /// A type-associated call: an instance method (`args[0]` is the receiver)
    /// or a static associated function. `type_name` is the type's [`Path`].
    Assoc {
        type_name: Path,
        method: String,
        instance: bool,
        args: Vec<Typed>,
    },
    /// A call to a trait method on a still-generic type-variable receiver
    /// inside a `where`-bounded function body (`Checker::check_instance_method`'s
    /// type-variable branch) — e.g. `(next it)` where `it: T` and the
    /// enclosing function declared `(where (Iter T))`.
    ///
    /// **Diagnostics-only since monomorphization**: this node only ever
    /// appears in a generic function's definition-time body check, which is
    /// never executed (`Interp::exec` skips registering an erased generic
    /// body) — each specialization re-checks the same call with the receiver
    /// type concrete, where `check_instance_method` resolves it statically
    /// to an ordinary `Expr::Assoc`. The interpreter's `TraitCall` eval arm
    /// is accordingly an internal-error trap, and the old runtime dispatch
    /// (reading the receiver value's own type tag) is gone. The `compile`
    /// pipeline's own `TraitCall` lowering (`ast_bridge`/`compiler.rs`'s
    /// dispatch chain) is likewise unreachable from source — a generic
    /// function can't be `compile`d — and is kept only pending its own
    /// removal.
    TraitCall {
        trait_name: Path,
        method: String,
        /// Every concrete type registered (in `Registry`) as `impl`ing
        /// `trait_name` as of this call site's own check time — resolved
        /// once, here, purely so `compile`'s `ast_bridge`/`compiler.rs` can
        /// build a closed dispatch chain without registry access of their
        /// own. Unreachable from source today (see above).
        impls: Vec<Path>,
        args: Vec<Typed>,
    },
    /// A data-type constructor application, e.g. `(Some x)` / `(Cons a d)`.
    Construct {
        /// The nominal type's fully-qualified [`Path`], e.g. `option`, `sexpr`.
        type_name: Path,
        /// Index of the variant within the type's definition.
        variant: usize,
        args: Vec<Typed>,
        /// Whether the type is `AdtKind::Struct` (a `defstruct`) rather than
        /// `AdtKind::Sum` — set by `Checker::check_construct` from the
        /// resolved `AdtDef::kind`. Chooses the interpreter's runtime
        /// representation: `true` builds a mutable, reference-semantics
        /// boxed struct (`RtValue::Sexpr` wrapping a `BoxedObj::Struct`);
        /// `false` (every built-in ADT) builds the existing value-semantics
        /// `RtValue::Data`, unchanged.
        mutable: bool,
    },
    /// Reads field `.1` of a `defstruct` instance (a boxed struct, see
    /// `Expr::Construct`'s `mutable` doc comment) by
    /// position. Synthesized only by `Checker::check_defstruct` as a field
    /// accessor's body (the `p::x` surface syntax desugars to an ordinary
    /// instance-method call on that accessor, `Expr::Assoc` — see
    /// `Checker::check`'s `Value::Path` case — so this node itself is never
    /// produced directly from user-written source).
    FieldGet(Box<Typed>, usize),
    /// Writes field `.1` of a `defstruct` instance in place, evaluating to
    /// `Unit`. Synthesized only by `Checker::check_defstruct` as a field
    /// setter's body — `(setf p::x v)` desugars to an ordinary instance-
    /// method call on that setter (`Expr::Assoc`), the same way `p::x` reads
    /// desugar to the getter call (see `Expr::FieldGet`'s doc comment and
    /// `Checker::check_setf`).
    FieldSet(Box<Typed>, usize, Box<Typed>),
    /// `(match scrutinee (pattern body...)...)`.
    Match(Box<Typed>, Vec<Arm>),
    /// `(setf var value)` — assign to a local variable; evaluates to the value.
    Set(String, Box<Typed>),
    /// `(setf global value)` — assign to a global; evaluates to the value.
    SetGlobal(Path, Box<Typed>),
    /// `(loop body...)` — loop forever, exited via `break`/`return`. Its type is
    /// the join of every `break`/`return` reached directly inside it (not
    /// crossing a nested loop or `lambda`); `Never` if it never exits.
    Loop(Vec<Typed>),
    /// `(break)` — exit the nearest enclosing loop with no value. Type `Never`.
    Break,
    /// `(return)` / `(return value)` — exit the nearest enclosing loop,
    /// optionally with a value (`Unit` if omitted). Type `Never`.
    Return(Option<Box<Typed>>),
    /// `(panic message)` — diverges. Has type [`Type::Never`](crate::Type).
    Panic(Box<Typed>),
    /// `(quote datum)` — the literal `datum`, unevaluated, as a `Sexpr` value.
    /// Carries an owned [`QuotedSexpr`] rather than a raw [`Value`](crate::Value)
    /// cons pointer: `Expr`/`Typed` trees are kept indefinitely in `Interp::fns`,
    /// but the GC's root-walk only reaches values through live `slots`, never
    /// through stored function bodies — so a live heap pointer baked into a
    /// literal here would be invisible to the collector. The interpreter
    /// reconstructs a fresh heap value from this on every evaluation.
    Quote(QuotedSexpr),
}

/// An owned, GC-heap-independent mirror of `Sexpr`'s shape, used by
/// [`Expr::Quote`] (see its doc comment for why this can't just hold a raw
/// [`Value`](crate::Value)).
#[derive(Clone, Debug, PartialEq)]
pub enum QuotedSexpr {
    Nil,
    Int(i64),
    Float(f64),
    Char(char),
    Bool(bool),
    Sym(String),
    Str(String),
    Cons(Box<QuotedSexpr>, Box<QuotedSexpr>),
}

/// One arm of a `match`: a pattern and the body sequence it guards.
#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub pat: Pattern,
    pub body: Vec<Typed>,
}

/// A match pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    /// `_` — matches anything, binds nothing.
    Wildcard,
    /// A variable pattern — matches anything, binding it to the named
    /// variable. The `bool` is whether the binding's static type (known at
    /// this pattern position at check time — `Checker::check_pattern`'s
    /// `expected`) has the heap-cell runtime representation (`Sexpr`/boxed
    /// struct/`HashTable<K,V>` — `Checker::is_heap_repr`), which decides the
    /// binding-slot routing (`Interp`'s `Slot::Heap` vs `Slot::Native`) —
    /// baked here because the evaluator's `match_pattern` otherwise sees
    /// only the bound *value*, and slot routing is deliberately static-type-
    /// driven, never value-shape-driven.
    Bind(String, bool),
    Int(i64),
    Bool(bool),
    Char(char),
    /// A constructor pattern, e.g. `(Some v)` / `(Cons a d)`.
    Ctor {
        type_name: Path,
        variant: usize,
        args: Vec<Pattern>,
        /// Per-field: whether the matched field's declared type (as
        /// instantiated by the scrutinee's own type arguments —
        /// `Checker::check_ctor_pattern`'s `subst`) is the built-in `Sexpr`.
        /// Resolved fully at check time, the same convention
        /// `Expr::Construct::mutable` follows, so the interpreter's boxed-
        /// struct destructuring arm can hand a `Sexpr`-declared field back
        /// as the `Sexpr` it is (`interp.rs`'s `decode_field_typed` makes
        /// the same decision from a `Type`; this is that bit precomputed
        /// per field — a stored quoted `42`/`3.14` must not rebind as a
        /// plain `Int`/`Float`, contradicting the static type). Exact even
        /// for a generic scrutinee, since generic bodies/patterns are
        /// checked monomorphized. Only consulted for a boxed-struct
        /// scrutinee — a sum-type
        /// `RtValue::Data`'s fields are already `RtValue`s and a `Sexpr`
        /// scrutinee's destructuring (`match_sexpr_ctor`) is variant-driven.
        sexpr_fields: Vec<bool>,
    },
}
