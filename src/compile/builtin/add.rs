use std::error;
// use inkwell::OptimizationLevel;
// use inkwell::builder::Builder;
// use inkwell::context::Context;
// use inkwell::execution_engine::ExecutionEngine;
// use inkwell::module::Module;
use inkwell::values::FunctionValue;
use inkwell::values::*;
use crate::Compiler;

use crate::{CompiledValue, NUMBER_ZERO, Number};

#[no_mangle]
pub extern "C" fn builtin_add(args: *mut Vec<BasicValueEnum>) -> *mut Result<CompiledValue, Box<dyn error::Error>> {
    let mut result = NUMBER_ZERO.clone();
    let args = unsafe { Box::from_raw(args) };

    for value in *args {
        let num = value.into_int_value().get_sign_extended_constant().unwrap();
        result = result + &Number::I64(num);
    }

    let ok = Box::new(Ok(CompiledValue::Number(result)));
    Box::into_raw(ok)
}
