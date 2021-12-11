#![allow(dead_code)]

use std::sync::Arc;

#[derive(PartialEq, Clone, Debug)]
pub struct Cons<T> {
    pub car: Arc<T>,
    pub cdr: Arc<Option<Cons<T>>>,
}

impl<T> Cons<T> {
    pub fn new_cons(car: T, cdr: Option<Cons<T>>) -> Cons<T> {
        Cons {
            car: Arc::new(car),
            cdr: Arc::new(cdr),
        }
    }

    pub fn set_cdr(&mut self, cdr: Cons<T>) {
        self.cdr = Arc::new(Some(cdr));
    }

    pub fn iter(&self) -> ConsIter<T> {
        ConsIter::new(self)
    }

}

#[derive(PartialEq, Debug)]
pub struct ConsIter<'a, T> {
    opt_cons: Option<&'a Cons<T>>,
}

impl<'a, T> ConsIter<'a, T> {
    pub fn new(cons: &'a Cons<T>) -> ConsIter<'a, T> {
        ConsIter {
            opt_cons: Some(cons)
        }
    }
}

impl<'a, T> Iterator for ConsIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(cons) = self.opt_cons {
            let result = cons.car.as_ref();
            self.opt_cons = cons.cdr.as_ref().as_ref();

            Some(result)
        }else{
            None
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum Object {
    Null,
    True,
    False,
    Int(i64),
    Symbol(String),
    String(String),
    List(Cons<Object>),
    // Cons { car: Arc<Object>, cdr: Arc<Object> }
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

    pub fn new_list_from_vec(v: Vec<Object>) -> Object {
        let mut list = None;

        for o in v {
            list = Some(Cons::new_cons(o, list));
        }

        if let Some(l) = list {
            Object::List(l)
        }else{
            Object::Null
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

    pub fn is_list(&self) -> bool {
        if let Object::List(_) = self {
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
        assert!( ! obj.is_list());
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
        assert!( ! obj.is_list());
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
        assert!( ! true_obj.is_list());

        assert!( ! false_obj.is_null());
        assert!( ! false_obj.is_int());
        assert!( ! false_obj.is_symbol());
        assert!( ! false_obj.is_string());
        assert!( ! false_obj.is_list());
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
        assert!( ! obj.is_list());
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
        assert!( ! obj.is_list());
    }

    #[test]
    fn is_list_test() {
        let obj = Object::new_list_from_vec(vec![Object::new_i64(1), Object::new_string("foo".to_string())]);
        assert!(obj.is_list());

        assert!( ! obj.is_null());
        assert!( ! obj.is_bool());
        assert!( ! obj.is_true());
        assert!( ! obj.is_false());
        assert!( ! obj.is_int());
        assert!( ! obj.is_symbol());
        assert!( ! obj.is_string());
    }
}
