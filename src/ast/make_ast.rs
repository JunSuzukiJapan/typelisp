use crate::{Expr, Object, Cons, Error};

pub struct ASTConstructor;

impl ASTConstructor {
    pub fn new() -> ASTConstructor {
        ASTConstructor {}
    }

    pub fn make_ast(&self, obj: &Object) -> Result<Expr, Error> {
        match obj {
            Object::Null => Ok(Expr::Null),
            Object::True => Ok(Expr::True),
            Object::False => Ok(Expr::False),
            Object::Int(x) => Ok(Expr::Int(*x)),
            Object::String(s) => Ok(Expr::String(s.clone())),
            Object::Symbol(s) => Ok(Expr::Var(s.clone())),
            Object::List(p) => self.make_ast_from_list(p),
            // _ => unimplemented!(),
        }
    }

    fn make_ast_from_list(&self, pair: &Cons) -> Result<Expr, Error> {
        match pair.car.as_ref() {
            Object::Symbol(s) => self.make_call_function(s, pair.cdr.as_ref()),
            Object::List(l) => self.make_call_lambda_function(pair.cdr.as_ref()),
            _ => return Err(Error::CallNotFunction),
        }
    }

    fn make_call_function(&self, name: &String, args: &Option<Cons>) -> Result<Expr, Error> {


        unimplemented!()
    }

    fn make_call_lambda_function(&self, args: &Option<Cons>) -> Result<Expr, Error> {

        unimplemented!()
    }
}