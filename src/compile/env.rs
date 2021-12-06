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
}