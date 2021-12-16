use std::error;
use inkwell::values::*;
use once_cell::sync::Lazy;

use crate::{CompiledValue, NUMBER_ZERO};

#[no_mangle]
pub extern "C" fn builtin_add(args: Option<Vec<CompiledValue>>) -> Result<CompiledValue, Box<dyn error::Error>> {
    let mut result = NUMBER_ZERO.clone();

    if let Some(vec) = args {
        for value in vec {
            let num = value.get_number_value().unwrap();
            result = result + num;
        }
    }

    Ok(CompiledValue::Number(result))
}