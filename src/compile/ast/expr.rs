use crate::Cons;

#[derive(Debug)]
pub struct Function {
    params: Vec<String>,
    body: Expr,
}

impl Function {
    pub fn new(body: Expr, params: Vec<String>) -> Function {
        Function {
            params: params,
            body: body,
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum Expr {
    Null,
    Int(i64),
    True,
    False,
    Symbol(String),
    String(String),
    Var(String),
    List(Cons),
    CallFunction(String, Option<Vec<Expr>>),
    CallLambdaFunction(Cons, Option<Vec<Expr>>),

    Plus(Vec<Expr>),
}