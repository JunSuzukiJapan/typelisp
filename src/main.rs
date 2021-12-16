#![allow(unused_variables)]

pub mod errors;
pub mod read;
pub mod eval;
pub mod compile;

// use errors::*;
// use read::*;
// use read::reader::Reader;
use typelisp::*;

/*
fn main() -> Result<(), Error> {
    // let reader = Reader::new();
    // let obj = reader.read("(1 2 \"foo\")")?;
    // println!("obj: {:?}", obj);

    let reader = Reader::new();
    let evaluator = Evaluator::new();
    let obj = reader.read("(+ 1 2)")?;
    let obj = evaluator.eval(&obj)?;
    println!("obj: {:?}", obj);

    Ok(())
}
*/

use inkwell::targets::{InitializationConfig, Target};
use inkwell::context::Context;
use inkwell::OptimizationLevel;
use inkwell::support::LLVMString;


#[no_mangle]
pub extern "C" fn add(x: i32, y: i32) -> i32 {
    x + y
}

fn main() -> Result<(), LLVMString> {
    Target::initialize_native(&InitializationConfig::default()).unwrap();

    let context = Context::create();
    // moduleを作成
    let module = context.create_module("main");
    // builderを作成
    let builder = context.create_builder();

    // 型関係の変数
    let i32_type = context.i32_type();
    let i8_type = context.i8_type();
    let i8_ptr_type = i8_type.ptr_type(inkwell::AddressSpace::Generic);

    // add関数を宣言
    let i32_type = context.i32_type();
    let add_fn_type = i32_type.fn_type(&[i32_type.into(), i32_type.into()], false);
    let add_function = module.add_function("add", add_fn_type, None);

    // printf関数を宣言
    // let printf_fn_type = i32_type.fn_type(&[i8_ptr_type.into()], true);
    // let printf_function = module.add_function("printf", printf_fn_type, None);

    // main関数を宣言
    let main_fn_type = i32_type.fn_type(&[], false);
    let main_function = module.add_function("main", main_fn_type, None);

    // main関数にBasic Blockを追加
    let entry_basic_block = context.append_basic_block(main_function, "entry");
    // builderのpositionをentry Basic Blockに設定
    builder.position_at_end(entry_basic_block);

    // ここからmain関数に命令をビルドしていく
    let x = i32_type.const_int(1, true);
    let y = i32_type.const_int(2, true);
    let ret = builder.build_call(add_function, &[x.into(), y.into()] , "call_add");
    // let ret_i = ret.get_called_fn_value().get_last_param().unwrap();
    let ret_i = ret.try_as_basic_value().left().unwrap().into_int_value();
    builder.build_return(Some(&ret_i));
    // builder.build_return(Some(&i32_type.const_int(0, false)));

    // JIT実行エンジンを作成し、main関数を実行
    let execution_engine = module.create_jit_execution_engine(OptimizationLevel::None)?;
    // let execution_engine = module.create_jit_execution_engine(OptimizationLevel::Aggressive).unwrap();
    execution_engine.add_global_mapping(&add_function, add as usize);

    println!("run jit engine");
    let result = unsafe {
        execution_engine.get_function::<unsafe extern "C" fn() -> i32>("main").unwrap().call()
        // execution_engine.run_function(main_function, &[]).as_int(true)
    };
    println!("result = {}", result);

    Ok(())
}
