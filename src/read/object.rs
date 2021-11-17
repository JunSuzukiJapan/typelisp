#![allow(dead_code)]

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

    pub fn is_null(&self) -> bool {
        if let Object::Null = self {
            true
        }else{
            false
        }
    }

    pub fn is_int(&self) -> bool {
        if let Object::Int(_) = self {
            true
        }else{
            false
        }
    }

    pub fn is_true(&self) -> bool {
        if let Object::True = self {
            true
        }else{
            false
        }
    }

    pub fn is_false(&self) -> bool {
        if let Object::False = self {
            true
        }else{
            false
        }
    }

    pub fn is_bool(&self) -> bool {
        match self {
            &Object::True | &Object::False => true,
            _ => false,
        }
    }

    pub fn is_symbol(&self) -> bool {
        if let Object::Symbol(_) = self {
            true
        }else{
            false
        }
    }

    pub fn is_string(&self) -> bool {
        if let Object::String(_) = self {
            true
        }else{
            false
        }
    }

    pub fn is_cons(&self) -> bool {
        if let Object::Cons {car: _, cdr: _} = self {
            true
        }else{
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_null_test() {
        let obj = Object::Null;
        assert!(obj.is_null());

        assert!( ! obj.is_bool());
        assert!( ! obj.is_true());
        assert!( ! obj.is_false());
        assert!( ! obj.is_int());
        assert!( ! obj.is_symbol());
        assert!( ! obj.is_string());
        assert!( ! obj.is_cons());
    }

    #[test]
    fn is_int_test() {
        let obj = Object::Int(0);
        assert!(obj.is_int());

        assert!( ! obj.is_null());
        assert!( ! obj.is_bool());
        assert!( ! obj.is_true());
        assert!( ! obj.is_false());
        assert!( ! obj.is_symbol());
        assert!( ! obj.is_string());
        assert!( ! obj.is_cons());
    }

    #[test]
    fn is_bool_test() {
        let true_obj = Object::True;
        let false_obj = Object::False;

        assert!(true_obj.is_bool());
        assert!(true_obj.is_true());
        assert!( ! true_obj.is_false());
        assert!(false_obj.is_bool());
        assert!( ! false_obj.is_true());
        assert!(false_obj.is_false());

        assert!( ! true_obj.is_null());
        assert!( ! true_obj.is_int());
        assert!( ! true_obj.is_symbol());
        assert!( ! true_obj.is_string());
        assert!( ! true_obj.is_cons());

        assert!( ! false_obj.is_null());
        assert!( ! false_obj.is_int());
        assert!( ! false_obj.is_symbol());
        assert!( ! false_obj.is_string());
        assert!( ! false_obj.is_cons());
    }

    #[test]
    fn is_symbol_test() {
        let obj = Object::Symbol("foo".to_string());
        assert!(obj.is_symbol());

        assert!( ! obj.is_null());
        assert!( ! obj.is_bool());
        assert!( ! obj.is_true());
        assert!( ! obj.is_false());
        assert!( ! obj.is_int());
        assert!( ! obj.is_string());
        assert!( ! obj.is_cons());
    }

    #[test]
    fn is_string_test() {
        let obj = Object::String("\"Hello, World!\"".to_string());
        assert!(obj.is_string());

        assert!( ! obj.is_null());
        assert!( ! obj.is_bool());
        assert!( ! obj.is_true());
        assert!( ! obj.is_false());
        assert!( ! obj.is_int());
        assert!( ! obj.is_symbol());
        assert!( ! obj.is_cons());
    }

    #[test]
    fn is_cons_test() {
        let obj = Object::Cons {car: Rc::new(Object::Null), cdr: Rc::new(Object::Null)};
        assert!(obj.is_cons());

        assert!( ! obj.is_null());
        assert!( ! obj.is_bool());
        assert!( ! obj.is_true());
        assert!( ! obj.is_false());
        assert!( ! obj.is_int());
        assert!( ! obj.is_symbol());
        assert!( ! obj.is_string());
    }
}
