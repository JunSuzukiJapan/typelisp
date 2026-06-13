// use once_cell::sync::Lazy;
// use std::collections::HashMap;
// use std::sync::Mutex;
// use crate::{Expr, Function, Object, Cons, Error};
use crate::{Expr, Object, Cons, Error};

// static FUNC_TABLE: Lazy<Mutex<HashMap<String, Function>>> = Lazy::new(|| {
//     Mutex::new(HashMap::new())
// });

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

    fn make_ast_from_list(&self, pair: &Cons<Object>) -> Result<Expr, Error> {
        match pair.car.as_ref() {
            Object::Symbol(s) => self.make_call_function(s, pair.cdr.as_ref()),
            Object::List(l) => self.make_call_lambda_function(l, pair.cdr.as_ref()),
            _ => return Err(Error::CallNotFunction),
        }
    }

    fn make_args(&self, args: &Option<Cons<Object>>) -> Result<Option<Vec<Expr>>, Error> {
        if let Some(args) = args {
            let mut v = Vec::new();

            for arg in args.iter() {
                v.push(self.make_ast(arg)?);
            }

            Ok(Some(v))
        }else{
            Ok(None)
        }        
    }

    fn make_call_function(&self, name: &String, args: &Option<Cons<Object>>) -> Result<Expr, Error> {
        let args = self.make_args(args)?;
        Ok(Expr::CallFunction(name.clone(), args))
    }

    fn make_call_lambda_function(&self, lambda: &Cons<Object>, args: &Option<Cons<Object>>) -> Result<Expr, Error> {
        fn convert_lambda(_l: &Cons<Object>) -> Result<Cons<Expr>, Error> {
            unimplemented!()


        }

        let lambda_function = convert_lambda(lambda)?;
        let args = self.make_args(args)?;
        Ok(Expr::CallLambdaFunction(lambda_function, args))
    }
}