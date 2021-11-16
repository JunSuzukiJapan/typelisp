use std::rc::Rc;

#[derive(PartialEq, Debug)]
pub enum Object {
    Null,
    True,
    False,
    Int(i64),
    Symbol(String),
    String(String),
    Cons { car: Rc<Object>, cdr: Rc<Object> }
}

impl Object {
    pub fn new_i64(num: i64) -> Object {
        Object::Int(num)
    }

    pub fn new_symbol(s: String) -> Object {
        Object::Symbol(s)
    }

    pub fn new_string(s: String) -> Object {
        Object::String(s)
    }

    pub fn new_cons(car: Object, cdr: Object) -> Object {
        Object::Cons {
            car: Rc::new(car),
            cdr: Rc::new(cdr),
        }
    }
}