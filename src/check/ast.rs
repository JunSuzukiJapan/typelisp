//! The typed AST produced by the checker.
//!
//! Checking lowers a read `Sexpr` ([`Value`](crate::Value)) directly into a
//! [`Typed`] tree: every node carries the [`Type`] it was checked at, so the
//! later interpreter (step 4) can walk this tree without re-deriving types.

use crate::Type;

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
    /// A reference to a bound variable.
    Var(String),
    /// `(if cond then else)`.
    If(Box<Typed>, Box<Typed>, Box<Typed>),
    /// `(let ((name val)...) body...)` — bindings, then a body sequence.
    Let(Vec<(String, Typed)>, Vec<Typed>),
    /// A call to a named function.
    Call(String, Vec<Typed>),
    /// A data-type constructor application, e.g. `(Some x)` / `(Cons a d)`.
    Construct {
        /// The (lowercased) nominal type name, e.g. `"option"`, `"sexpr"`.
        type_name: String,
        /// Index of the variant within the type's definition.
        variant: usize,
        args: Vec<Typed>,
    },
    /// `(match scrutinee (pattern body...)...)`.
    Match(Box<Typed>, Vec<Arm>),
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
        type_name: String,
        variant: usize,
        args: Vec<Pattern>,
    },
}
