use crate::{Object, Error};

pub struct Evaluator;

impl Evaluator {
    pub fn new() -> Evaluator {
        Evaluator {}
    }    

    pub fn eval(&self, obj: &Object) -> Result<Object, Error> {
        match obj {
            Object::Int(val) => Ok(Object::Int(*val)),
            Object::True => Ok(Object::True),
            Object::False => Ok(Object::False),
            Object::Symbol(name) => Ok(Object::Symbol(name.to_string())),
            Object::String(s) => Ok(Object::String(s.clone())),
            Object::Null => Ok(Object::Null),
            Object::Cons {car: _, cdr: _} => self.call_function_or_macro(obj),
            // _ => unimplemented!()
        }
    }

    fn call_function_or_macro(&self, obj: &Object) -> Result<Object, Error> {
        if let Object::Cons {car, cdr} = obj {
            unimplemented!()
            
        }else{
            Err(Error::InternalErrorEvalNotCons)
        }
    }
}