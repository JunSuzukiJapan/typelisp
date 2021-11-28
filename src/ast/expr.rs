use crate::Cons;

pub enum Expr {
    Null,
    Int(i64),
    True,
    False,
    Symbol(String),
    String(String),
    Var(String),
    List(Cons),

    Plus(Vec<Box<Expr>>),
}