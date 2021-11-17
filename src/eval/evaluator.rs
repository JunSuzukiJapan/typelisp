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
            &Object::String(ref s) => Ok(Object::String(s.clone())),
            _ => unimplemented!()
        }
    }
}