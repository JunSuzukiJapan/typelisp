#![allow(unused_variables)]

use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::ExecutionEngine;
use inkwell::module::Module;
use inkwell::values::*;
// use inkwell::basic_block::BasicBlock;
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use std::error;

use crate::{Env, Expr, Error};

static TEMP_COUNT: Lazy<Arc<Mutex<u64>>> = Lazy::new(|| {
    Arc::new(Mutex::new(0))
});

#[macro_export]
macro_rules! fn_type {
    ($result_type:expr) => (
        unsafe {
            let mut param_types = [];
            LLVMFunctionType($result_type, param_types.as_mut_ptr(), param_types.len() as u32, 0)
        }
    );
    ($result_type:expr,,,) => (
        unsafe {
            let mut param_types = [];
            LLVMFunctionType($result_type, param_types.as_mut_ptr(), param_types.len() as u32, 1)
        }
    );
    ($result_type:expr, $( $param_type:expr ),* ) => (
        unsafe {
            let mut param_types = [ $( $param_type ),* ];
            LLVMFunctionType($result_type, param_types.as_mut_ptr(), param_types.len() as u32, 0)
        }
    );
    ($result_type:expr, $( $param_type:expr ),* ,,,) => (
        unsafe {
            let mut param_types = [ $( $param_type ),* ];
            LLVMFunctionType($result_type, param_types.as_mut_ptr(), param_types.len() as u32, 1)
        }
    )
}

#[derive(Debug)]
pub struct Compiler<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    execution_engine: ExecutionEngine<'ctx>,
}

impl<'ctx> Compiler<'ctx> {
    fn new(context: &'ctx Context) -> Result<Compiler<'ctx>, Box<dyn error::Error>> {
        let module = context.create_module("main");
        let builder = context.create_builder();
        let engine = module.create_jit_execution_engine(OptimizationLevel::None)?;

        let mut compiler = Compiler {
            context: context,
            module: module,
            builder: builder,
            execution_engine: engine,
        };
        compiler.init_builtin_functions();

        Ok(compiler)
    }

    fn init_builtin_functions(&mut self) {

    }

    pub fn compile_and_run(expr: &Expr) -> Result<Expr, Box<dyn error::Error>> {
        let context = Context::create();
        let compiler = Compiler::new(&context)?;
        let mut env = Env::new();
        let main_function = compiler.generate_function_entry_point(expr)?;

        let compiled = compiler.compile_with_env(expr, &mut env)?;
        compiler.generate_return(&compiled)?;

        let value = unsafe { compiler.execution_engine.run_function(main_function, &[]) };

        Ok(Expr::Int(value.as_int(true) as i64))
    }

    pub fn compile_with_env(&self, expr: &Expr, env: &mut Env) -> Result<BasicValueEnum<'ctx>, Box<dyn error::Error>> {
        match expr {
            Expr::Int(value) => self.compile_int(value),
            Expr::CallFunction(name, args) => self.compile_call_function(name, args, env),


            _ => unimplemented!(),
        }
    }

    fn generate_function_entry_point(&self, expr: &Expr) -> Result<FunctionValue, Box<dyn error::Error>> {
        match expr {
            Expr::Int(_) => {
                let i64_type = self.context.i64_type();
                let arg_types = [];
                let fn_type = i64_type.fn_type(&arg_types, false);
                let fn_value = self.module.add_function("main", fn_type, None);
                let entry = self.context.append_basic_block(fn_value, "entry");
                self.builder.position_at_end(entry);
                Ok(fn_value)
            },
            Expr::CallFunction(name, opt_args) => {
                let function = self.module.get_function(name).ok_or(Error::NoSuchFunction(name.to_string()))?;


                unimplemented!()
            },


            _ => unimplemented!(),
        }
    }

    fn generate_return(&self, expr: &BasicValueEnum) -> Result<(), Box<dyn error::Error>> {
        match expr {
            BasicValueEnum::IntValue(value) => {
                self.builder.build_return(Some(value));
                Ok(())
            },



            _ => unimplemented!(),
        }
    }

    fn compile_int(&self, value: &i64) -> Result<BasicValueEnum<'ctx>, Box<dyn error::Error>> {
        let int_64_type = self.context.i64_type();
        let const_int = int_64_type.const_int(*value as u64, false);
        let ptr = self.builder.build_alloca(self.context.i64_type(), &self.get_temp_name("int"));
        self.builder.build_store(ptr, const_int);
        Ok(self.builder.build_load(ptr, &self.get_temp_name("int")))
    }

    fn compile_call_function(&self, name: &String, args: &Option<Vec<Expr>>, env: &mut Env) -> Result<BasicValueEnum<'ctx>, Box<dyn error::Error>> {
        let function = env.get_function(name).ok_or(crate::Error::NoSuchFunction(name.clone()))?;

        let mut params = Vec::new();
        if let Some(v) = args {
            let mut params = Vec::new();

            for e in v {
                params.push(self.compile_with_env(e, env)?);
            }
        };

        self.builder.build_call(*function, &params, name);






        unimplemented!()
    }

    fn get_temp_name(&self, name: &str) -> String {
        let temp = TEMP_COUNT.clone();
        let mut num = temp.lock().unwrap();
        *num += 1;
        format!("{}{}", name, *num)
    }
}