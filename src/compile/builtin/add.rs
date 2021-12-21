use std::error;
// use inkwell::values::*;
// use once_cell::sync::Lazy;

use crate::{CompiledValue, NUMBER_ZERO};

#[no_mangle]
pub extern "C" fn builtin_add(args: *mut Option<Vec<CompiledValue>>) -> *mut Result<CompiledValue, Box<dyn error::Error>> {
    let mut result = NUMBER_ZERO.clone();
    let args = unsafe { Box::from_raw(args) };

    if let Some(vec) = *args {
        for value in vec {
            let num = value.get_number_value().unwrap();
            result = result + num;
        }
    }

    let ok = Box::new(Ok(CompiledValue::Number(result)));
    Box::into_raw(ok)
}