use inkwell::values::BasicValueEnum;

use crate::Number;

#[repr(C)]
#[derive(Clone, Debug)]
pub enum CompiledValue<'ctx> {
    BasicValue(BasicValueEnum<'ctx>),
    Number(Number),
}

impl<'ctx> CompiledValue<'ctx> {
    pub fn from_i64(value: i64) -> CompiledValue<'ctx> {
        CompiledValue::Number(Number::from_i64(value))
    }

    pub fn get_number_value(&self) -> Option<&Number> {
        if let CompiledValue::Number(num) = self {
            Some(num)
        }else{
            None
        }
    }
}