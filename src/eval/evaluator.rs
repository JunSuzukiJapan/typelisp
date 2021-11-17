use crate::{Object, Error};

pub struct Evaluator;

impl Evaluator {
    pub fn new() -> Evaluator {
        Evaluator {}
    }    

    pub fn eval(&self, obj: &Object) -> Result<Object, Error> {
        match *obj {
            Object::Int(val) => Ok(Object::Int(val)),
            _ => unimplemented!()
        }
    }
}