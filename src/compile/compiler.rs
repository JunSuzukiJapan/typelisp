use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::{ExecutionEngine, JitFunction};
use inkwell::module::Module;
use inkwell::values::*;
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use std::error::Error;

use crate::{Env, Expr};

static TEMP_COUNT: Lazy<Arc<Mutex<u64>>> = Lazy::new(|| {
    Arc::new(Mutex::new(0))
});

#[derive(Debug)]
pub struct Compiler<'ctx> {
    context: Context,
    module: Option<Module<'ctx>>,
    builder: Option<Builder<'ctx>>,
    execution_engine: Option<ExecutionEngine<'ctx>>,
}

impl<'ctx> Compiler<'ctx> {
    pub fn new() -> Compiler<'ctx> {
        let context = Context::create();

        Compiler {
            context: context,
            module: None,
            builder: None,
            execution_engine: None,
        }
    }

    pub fn compile(&'ctx mut self, expr: &Expr, env: &mut Env) -> Result<BasicValueEnum, Box<dyn Error>> {
        let module = self.context.create_module("main");
        let builder = self.context.create_builder();
        self.module = Some(module);
        self.builder = Some(builder);
        let engine = self.module.as_ref().unwrap().create_jit_execution_engine(OptimizationLevel::None)?;

        match expr {
            Expr::Int(value) => self.compile_int(value),
            Expr::CallFunction(name, args) => self.compile_call_function(name, args, env),


            _ => unimplemented!(),
        }
    }

    fn get_temp_name(&self, name: &str) -> String {
        let temp = TEMP_COUNT.clone();
        let mut num = temp.lock().unwrap();
        *num += 1;
        format!("{}{}", name, TEMP_COUNT.lock().unwrap())
    }

    fn compile_int(&self, value: &i64) -> Result<BasicValueEnum, Box<dyn Error>> {
        // let v = self.context.SInt64(val as u64);
        // let ptr = self.builder.build_alloca(self.types.int64_type);
        // self.builder.build_store(v, ptr);
        // self.builder.build_load(ptr)

        let int_64_type = self.context.i64_type();
        let const_int = int_64_type.const_int(*value as u64, false);
        let builder = self.builder.as_ref().unwrap();
        let ptr = builder.build_alloca(self.context.i64_type(), &self.get_temp_name("int"));
        builder.build_store(ptr, const_int);
        Ok(builder.build_load(ptr, &self.get_temp_name("int")))
    }

    fn compile_call_function(&self, name: &String, args: &Option<Vec<Expr>>, env: &mut Env) -> Result<BasicValueEnum, Box<dyn Error>> {
        let fun = env.get_function(name).ok_or(crate::Error::NoSuchFunction(name.clone()))?;






        unimplemented!()
    }
}