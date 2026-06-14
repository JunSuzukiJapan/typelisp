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
    /// `(panic! message)` — diverges. Has type [`Type::Never`](crate::Type).
    Panic(Box<Typed>),
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
