use std::error;
use crate::{Cons, Error};

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
    List(Cons<Expr>),
    CallFunction(String, Option<Vec<Expr>>),
    CallLambdaFunction(Cons<Expr>, Option<Vec<Expr>>),

    Plus(Vec<Expr>),
}

impl Expr {
    pub fn get_value_as_int(&self) -> Result<i64, Box<dyn error::Error>> {
        if let Expr::Int(x) = self {
            Ok(*x)
        }else{
            Err(Box::new(Error::AddNotNumber))
        }
    }
}