//! The typed AST produced by the checker.
//!
//! Checking lowers a read `Sexpr` ([`Value`](crate::Value)) directly into a
//! [`Typed`] tree: every node carries the [`Type`] it was checked at, so the
//! later interpreter (step 4) can walk this tree without re-deriving types.

use num_bigint::BigInt;
use num_rational::BigRational;

use crate::{Loc, Path, Type};

/// An expression node annotated with its checked type.
///
/// `loc` is the source span the node was read from (when known — a list
/// form always carries one from its own `cons_loc`; a bare atom does too
/// when it was read as a list element (`elem_locs`) or as a spanned
/// top-level datum (`Reader::read_all_in_spanned` -> `Checker::
/// check_form_at`'s `loc_hint`) — see `check::locate`'s module doc comment),
/// set by [`Checker::check`](crate::Checker) so the interpreter can report a
/// runtime error with its `file:line:col` and the LSP can underline the
/// node's exact extent. It is deliberately excluded from equality (see the
/// manual [`PartialEq`] impl): two structurally identical trees read from
/// different places are still equal, which keeps the checker's AST-shape
/// tests location-independent.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Typed {
    pub expr: Expr,
    pub ty: Type,
    pub loc: Option<Loc>,
}

impl Typed {
    /// A `Typed` with no source location — the default for nodes the checker
    /// synthesizes or that were read from a non-list form. [`Checker::check`]
    /// fills in `loc` afterwards for nodes that have one.
    pub fn new(expr: Expr, ty: Type) -> Typed {
        Typed { expr, ty, loc: None }
    }
}

impl PartialEq for Typed {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr && self.ty == other.ty
    }
}

/// One function definition inside `Expr::Labels`: its name, typed
/// parameters, and checked body.
pub type LabelDef = (String, Vec<(String, Type)>, Vec<Typed>);

/// A reference to a free function or global variable — everything both
/// `Interp` and the compile pipeline need to resolve one, kept as two
/// independent halves rather than a single collapsed `Path`:
///
/// - `written`/`home` are what `eval::scope::ModuleScope` (see that
///   module's doc comment) uses to *re-derive* the target itself at
///   runtime, walking its own tree from `home` — mirroring
///   `Checker::resolve_fn`/`resolve_fn_path`/`resolve_global`/
///   `resolve_global_path` (checker.rs:1061-1093, 1374-1410) — rather than
///   trusting `resolved` as a lookup key. `written` is the name exactly as
///   it appeared at the reference site (`["inc"]` bare, `["m","inc"]`
///   qualified) and `home` is the lexically enclosing module's own raw
///   segments (`Checker::ns` at the point this reference was checked, e.g.
///   `[]` at the root) — the walk's starting point. Plain `Vec<String>`
///   rather than [`Path`] because the root module has *zero* segments,
///   which `Path` (always at least one — the local name) can't represent.
/// - `resolved` is the checker's own fully-qualified answer, kept only for
///   compile-time-only consumers that have no way to redo resolution
///   themselves: `compile::ast_bridge` (deliberately `Registry`-free),
///   `compile::aot`, and `check::locate`'s LSP goto-definition. `Interp`
///   itself never reads this field.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ref {
    pub written: Vec<String>,
    pub home: Vec<String>,
    pub resolved: Path,
}

impl Ref {
    /// A synthesized reference with no real "as written" source form (the
    /// checker's own internal rewrites — e.g. `sexpr-cons`, quasiquote's
    /// `append_fq` — where `resolved` is already exactly right and there's
    /// no user-facing shadowing concern `written`+`home` would need to
    /// disambiguate). `home` is `resolved`'s own parent module, so a
    /// bare-name ancestor-walk starting there still finds the same target
    /// as direct descent would.
    pub fn synthetic(resolved: Path) -> Ref {
        let written = vec![resolved.local().to_string()];
        let home = resolved.parent().to_vec();
        Ref { written, home, resolved }
    }
}

/// The target of a `(compile name)` / `(compile type::method)` form —
/// `Checker::check_compile` builds this from whichever resolution it
/// already performs for its own generic-target check (`resolve_fn`/
/// `resolve_type_name`), instead of discarding that result and handing
/// `Interp` a bare name string to re-resolve by an unqualified, module-blind
/// search (the bug this replaces — see `docs/implementation-log.md`).
/// `Fn`'s `Ref` gets the exact same independent `written`+`home`
/// re-resolution at runtime as an ordinary `Expr::Call`; `Method`'s
/// `type_name` is already a fully resolved type identity (never searched,
/// same reasoning as `Expr::Assoc`/`MethodRef`), so only `home` is needed
/// for the `pub`-or-`in_scope` check.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CompileTarget {
    Fn(Ref),
    Method { type_name: Path, method: String, home: Vec<String> },
}

/// An expression. Children are [`Typed`] so the whole tree stays annotated.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Expr {
    Int(i64),
    Float(f64),
    /// A `bignum` literal — extracted from its heap box at check time
    /// (`Heap::bignum_value`), the same "own the scalar payload, no live
    /// heap pointer baked into the tree" treatment `Float` already gets (see
    /// this type's own doc comment on why `f64` there needs no special
    /// heap-independence handling: a `BigInt` is likewise plain owned Rust
    /// memory, not a GC-tracked heap reference).
    Bignum(BigInt),
    /// A `ratio` literal, same treatment as [`Expr::Bignum`].
    Ratio(BigRational),
    Bool(bool),
    Char(char),
    Str(String),
    /// The unit value `()`.
    Unit,
    /// A reference to a bound (local) variable.
    Var(String),
    /// A reference to a global variable/constant.
    Global(Ref),
    /// A named free function used as a value (reified into a function value).
    FnRef(Ref),
    /// A named *instance* method used as a value, e.g. passing `+` (an `i32`
    /// instance method, see `registry::int_assoc`) where a `(fn (i32 i32)
    /// i32)` is expected — the receiver type is resolved from that expected
    /// function type at the use site (`Checker::method_value`), since there's
    /// no receiver expression to dispatch on. `type_name` is already a fully
    /// resolved type identity (never searched, unlike a `Ref`'s `written`
    /// name) — see `Expr::Assoc`'s doc comment for why `home` alone (no full
    /// `Ref`) suffices here.
    MethodRef { type_name: Path, method: String, home: Vec<String> },
    /// `(if cond then else)`.
    If(Box<Typed>, Box<Typed>, Box<Typed>),
    /// `(let ((name val)...) body...)` — bindings, then a body sequence.
    Let(Vec<(String, Typed)>, Vec<Typed>),
    /// A call to a free function.
    Call(Ref, Vec<Typed>),
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
    /// or a static associated function. `type_name` is the type's [`Path`] —
    /// already a fully resolved, statically-known identity from type
    /// inference (never an ambiguous bare name to search for), so unlike
    /// `Call`/`Global`/`FnRef` this never needs `eval::scope::ModuleScope`'s
    /// ancestor-chain walk; `Interp` only needs a direct descent to
    /// `type_name`'s own module plus a `public`-or-`in_scope` check against
    /// `home` (the reference's own lexical module, read against the target
    /// `FnDef`'s own `public` bit at eval time) — `Interp` re-derives this
    /// independently, the same `Checker::assoc_visible` (checker.rs:989-991)
    /// rule, rather than trusting a precomputed bit.
    Assoc {
        type_name: Path,
        method: String,
        instance: bool,
        args: Vec<Typed>,
        home: Vec<String>,
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
    /// pipeline never sees it either — a generic function can't be
    /// `compile`d, so `ast_bridge` maps this node to an `unsupported`
    /// placeholder. Carries only what that trap and the `check`-time walk
    /// need (`method`/`args`); no `trait_name`/`impls` are retained, since
    /// the closed dispatch chain those once fed has been removed.
    TraitCall {
        method: String,
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
    SetGlobal(Ref, Box<Typed>),
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
    /// `(compile name)` / `(compile type::method)` — JIT-compiles the named
    /// function/method against the running `Interp`, returning `bool`. See
    /// `CompileTarget`'s doc comment for why this is its own node rather
    /// than a disguised `Call` to a builtin taking a string argument.
    CompileFn(CompileTarget),
}

/// An owned, GC-heap-independent mirror of `Sexpr`'s shape, used by
/// [`Expr::Quote`] (see its doc comment for why this can't just hold a raw
/// [`Value`](crate::Value)).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum QuotedSexpr {
    Nil,
    Int(i64),
    Float(f64),
    Bignum(BigInt),
    Ratio(BigRational),
    Char(char),
    Bool(bool),
    Sym(String),
    /// A `::`-qualified path (e.g. `dep::head`), segments in written order —
    /// the reader's `Value::Path` mirrored heap-independently, same reason
    /// as the rest of this type (see [`Expr::Quote`]'s doc comment).
    Path(Vec<String>),
    Str(String),
    Cons(Box<QuotedSexpr>, Box<QuotedSexpr>),
}

/// One arm of a `match`: a pattern and the body sequence it guards.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Arm {
    pub pat: Pattern,
    pub body: Vec<Typed>,
}

/// A match pattern.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
        /// Per-field: the same fully-instantiated `Type`
        /// `Checker::check_ctor_pattern` computes `sexpr_fields` from, kept
        /// in full rather than reduced to a single bool. The interpreter
        /// doesn't need this (its boxed-struct destructuring arm decodes by
        /// runtime shape, `interp.rs`'s `decode_nonsexpr_field`), but
        /// compiled code can't — a boxed-struct scrutinee's compiled
        /// `match` (`ast_bridge::pattern_to_sexpr`'s struct-kind branch)
        /// needs each field's own `ast_bridge::struct_field_kind`
        /// (int/float/char/bool/passthrough), which `sexpr_fields`'s single
        /// bit can't express.
        field_types: Vec<Type>,
    },
}
