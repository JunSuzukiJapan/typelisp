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
    /// A data-type constructor application, e.g. `(Some x)` / `(Cons a d)`.
    Construct {
        /// The nominal type's fully-qualified [`Path`], e.g. `option`, `sexpr`.
        type_name: Path,
        /// Index of the variant within the type's definition.
        variant: usize,
        args: Vec<Typed>,
    },
    /// `(match scrutinee (pattern body...)...)`.
    Match(Box<Typed>, Vec<Arm>),
    /// `(setf var value)` — assign to a local variable; evaluates to the value.
    Set(String, Box<Typed>),
    /// `(setf global value)` — assign to a global; evaluates to the value.
    SetGlobal(Path, Box<Typed>),
    /// `(while cond body...)` — loop while `cond` holds. Has type `Unit`.
    While(Box<Typed>, Vec<Typed>),
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
    /// A variable pattern — matches anything, binding it to the named variable.
    Bind(String),
    Int(i64),
    Bool(bool),
    Char(char),
    /// A constructor pattern, e.g. `(Some v)` / `(Cons a d)`.
    Ctor {
        type_name: Path,
        variant: usize,
        args: Vec<Pattern>,
    },
}
