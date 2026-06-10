//! The typed AST for typelisp.
//!
//! `make_ast` (see `make_ast.rs`) builds these from the reader's `Object`s.
//! The type checker (M5) fills in `TypedExpr::ty`; code generation (M6+)
//! consumes the annotated tree.

use crate::{Object, Type};

/// A top-level program item.
#[derive(Clone, PartialEq, Debug)]
pub enum Form {
    Defun(Defun),
    DefStruct(DefStruct),
    DefMethod(DefMethod),
    DefVar(DefVar),
    DefConstant(DefVar),
    Use(Vec<String>),
    Module { pub_: bool, name: String, items: Vec<Form> },
    /// A bare top-level expression.
    Expr(TypedExpr),
}

/// An expression together with its (eventually inferred) type.
#[derive(Clone, PartialEq, Debug)]
pub struct TypedExpr {
    pub kind: ExprKind,
    pub ty: Option<Type>,
}

impl TypedExpr {
    /// Build an untyped node (the checker fills `ty` later).
    pub fn new(kind: ExprKind) -> TypedExpr {
        TypedExpr { kind, ty: None }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum ExprKind {
    // literals (numeric literals carry an optional explicit suffix type)
    Int(i64, Option<Type>),
    Float(f64, Option<Type>),
    Bool(bool),
    Char(char),
    Str(String),
    /// `()` / `nil` / `null`
    Unit,

    /// A variable reference.
    Var(String),

    Let { bindings: Vec<LetBinding>, body: Vec<TypedExpr> },
    If { cond: Box<TypedExpr>, then: Box<TypedExpr>, els: Option<Box<TypedExpr>> },
    When { cond: Box<TypedExpr>, body: Vec<TypedExpr> },
    Unless { cond: Box<TypedExpr>, body: Vec<TypedExpr> },
    Cond { clauses: Vec<CondClause> },
    Match { scrutinee: Box<TypedExpr>, arms: Vec<MatchArm> },
    Loop { body: Vec<TypedExpr> },
    While { cond: Box<TypedExpr>, body: Vec<TypedExpr> },
    Dotimes { var: String, count: Box<TypedExpr>, body: Vec<TypedExpr> },
    Progn { body: Vec<TypedExpr> },
    Break,

    Setf { place: Place, value: Box<TypedExpr> },
    Incf(Place),
    Decf(Place),

    /// `(quote datum)` — the quoted datum is the raw read `Object`.
    Quote(Object),

    Lambda { move_: bool, params: Vec<(String, Type)>, body: Vec<TypedExpr> },

    BinOp { op: BinOp, lhs: Box<TypedExpr>, rhs: Box<TypedExpr> },
    UnOp { op: UnOp, operand: Box<TypedExpr> },

    Call { target: CallTarget, args: Vec<TypedExpr> },
    /// `(.method obj args...)`
    MethodCall { method: String, receiver: Box<TypedExpr>, args: Vec<TypedExpr> },
    /// `(. obj field)`
    FieldAccess { object: Box<TypedExpr>, field: String },

    /// `defvar`/`defconstant` used in expression (body) position.
    DefVarLocal(Box<DefVar>),
}

#[derive(Clone, PartialEq, Debug)]
pub enum CallTarget {
    Sym(String),
    Path(Vec<String>),
    Lambda(Box<TypedExpr>),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BinOp {
    Add, Sub, Mul, Div, Rem,
    Eq, Ne, Lt, Gt, Le, Ge,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum UnOp {
    Inc1, // 1+
    Dec1, // 1-
    Not,  // !
}

#[derive(Clone, PartialEq, Debug)]
pub enum Place {
    Var(String),
    Field { object: Box<TypedExpr>, field: String },
    Index { object: Box<TypedExpr>, index: Box<TypedExpr> },
}

#[derive(Clone, PartialEq, Debug)]
pub struct LetBinding {
    pub name: String,
    pub ty: Option<Type>,
    pub init: TypedExpr,
}

#[derive(Clone, PartialEq, Debug)]
pub struct CondClause {
    pub test: TypedExpr,
    pub body: Vec<TypedExpr>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct MatchArm {
    pub patterns: Vec<Pattern>,
    pub body: Vec<TypedExpr>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Pattern {
    Int(i64),
    Float(f64),
    Char(char),
    Str(String),
    Bool(bool),
    /// binds a symbol
    Bind(String),
    /// `_`
    Wildcard,
    Range { lo: i64, hi: i64, inclusive: bool },
    Struct { name: Vec<String>, fields: Vec<Pattern> },
    Tuple(Vec<Pattern>),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Defun {
    pub pub_: bool,
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub body: Vec<TypedExpr>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct DefStruct {
    pub pub_: bool,
    pub name: String,
    pub generics: Vec<String>,
    pub pub_fields: Vec<(String, Type)>,
    pub priv_fields: Vec<(String, Type)>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Receiver {
    /// `(self T)` — instance method on `T`.
    Instance(Type),
    /// `(T)` — static method of `T`.
    Static(Type),
}

#[derive(Clone, PartialEq, Debug)]
pub struct DefMethod {
    pub pub_: bool,
    pub name: String,
    pub recv: Receiver,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub body: Vec<TypedExpr>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct DefVar {
    pub name: String,
    pub ty: Option<Type>,
    pub init: Box<TypedExpr>,
    /// `true` for `defvar`, `false` for `defconstant`.
    pub mutable: bool,
}
