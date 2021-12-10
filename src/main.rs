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


use inkwell::context::Context;
use inkwell::OptimizationLevel;


#[no_mangle]
pub extern "C" fn add(x: i64, y: i64) -> i64 {
    x + y
}

fn main() {
    let context = Context::create();
    // moduleを作成
    let module = context.create_module("main");
    // builderを作成
    let builder = context.create_builder();

    // 型関係の変数
    let i32_type = context.i32_type();
    let i8_type = context.i8_type();
    let i8_ptr_type = i8_type.ptr_type(inkwell::AddressSpace::Generic);

    // call my add
    let i64_type = context.i64_type();
    let add_fn_type = i64_type.fn_type(&[i64_type.into(), i64_type.into()], false);
    let _add_function = module.add_function("add", add_fn_type, None);


    // printf関数を宣言
    let printf_fn_type = i32_type.fn_type(&[i8_ptr_type.into()], true);
    let _printf_function = module.add_function("printf", printf_fn_type, None);

    // main関数を宣言
    let main_fn_type = i32_type.fn_type(&[], false);
    let main_function = module.add_function("main", main_fn_type, None);

    // main関数にBasic Blockを追加
    let entry_basic_block = context.append_basic_block(main_function, "entry");
    // builderのpositionをentry Basic Blockに設定
    builder.position_at_end(entry_basic_block);

    // ここからmain関数に命令をビルドしていく
/*
    // globalに文字列を宣言
    let hw_string_ptr = builder.build_global_string_ptr("Hello, inkwell!", "hw");
    // printfをcall
    builder.build_call(printf_function, &[hw_string_ptr.as_pointer_value().into()], "call");
    // main関数は0を返す
    builder.build_return(Some(&i32_type.const_int(0, false)));
*/

    // let x = i64_type.const_int(1, true);
    // let y = i64_type.const_int(2, true);
    // let ret = builder.build_call(add_function, &[x.into(), y.into()] , "add");
    // let ret_i = ret.get_called_fn_value().get_last_param().unwrap();
    // builder.build_return(Some(&ret_i));
    builder.build_return(Some(&i32_type.const_int(0, false)));

    // JIT実行エンジンを作成し、main関数を実行
    let execution_engine = module.create_jit_execution_engine(OptimizationLevel::Aggressive).unwrap();
    unsafe {
        execution_engine.get_function::<unsafe extern "C" fn()>("main").unwrap().call();
    }
}
