use std::collections::HashMap;
use std::rc::Rc;
use crate::{Object, Cons, Error};

pub struct Evaluator {
    fun_table: HashMap<String, Rc<dyn Fn(&Option<&Cons>) -> Result<Object, Error>>>,
}

impl Evaluator {
    //
    // Constructor
    //
    pub fn new() -> Evaluator {
        let mut tbl = HashMap::new();
        Self::init_fun_table(&mut tbl);

        Evaluator {
            fun_table: tbl,
        }
    }

    //
    // member functions
    //

    pub fn eval(&self, obj: &Object) -> Result<Object, Error> {
        match obj {
            Object::Int(val) => Ok(Object::Int(*val)),
            Object::True => Ok(Object::True),
            Object::False => Ok(Object::False),
            Object::Symbol(name) => Ok(Object::Symbol(name.to_string())),
            Object::String(s) => Ok(Object::String(s.clone())),
            Object::Null => Ok(Object::Null),
            Object::List(l) => self.call_function_or_macro(l),
            // _ => unimplemented!()
        }
    }

    fn call_function_or_macro(&self, obj: &Cons) -> Result<Object, Error> {
        println!("call obj = {:?}", obj);
        match obj.car.as_ref() {
            Object::Symbol(name) => {
                if let Some(f) = self.fun_table.get(name) {
                    let f = f.as_ref();
                    let args = obj.cdr.clone();
                    f(&args.as_ref().as_ref())
                }else{
                    return  Err(Error::NoSuchFunction(name.clone()));
                }
            },
            _ => Err(Error::CallNotFunction),
        }
    }

    //
    // static functions
    //

    fn init_fun_table(tbl: &mut HashMap<String, Rc<dyn Fn(&Option<&Cons>) -> Result<Object, Error>>>){
        tbl.insert("+".to_string(), Rc::new(Self::builtin_add));
    }

    fn builtin_add(obj: &Option<&Cons>) -> Result<Object, Error> {
        if let Some(cons) = obj {
            let mut val = 0;

            for i in cons.iter() {
                if let Object::Int(num) = i {
                    val = val + num;
                }else{
                    return Err(Error::AddNotNumber)
                }
            }
            Ok(Object::Int(val))
            
        }else{
            Ok(Object::Int(0))
        }
    }
}