use std::collections::HashMap;
use inkwell::values::FunctionValue;
use crate::expr::Expr;

#[derive(Debug)]
pub struct Env<'env> {
    var_tbl: HashMap<String, Expr>,
    fun_tbl: HashMap<String, FunctionValue<'env>>,
}

impl<'env> Env<'env> {
    pub fn new() -> Env<'env> {
        Env {
            var_tbl: HashMap::new(),
            fun_tbl: HashMap::new(),
        }
    }

    pub fn add_function(&mut self, name: &String, expr: FunctionValue<'env>) {
        self.fun_tbl.insert(name.clone(), expr);
    }

    pub fn add_var(&mut self, name: &String, expr: Expr) {
        self.var_tbl.insert(name.clone(), expr);
    }

    pub fn get_function(&self, name: &String) -> Option<&FunctionValue<'env>> {
        self.fun_tbl.get(name)
    }

    pub fn get_var(&self, name: &String) -> Option<&Expr> {
        self.var_tbl.get(name)
    }
}