use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::ExecutionEngine;
use inkwell::module::Module;
use inkwell::values::IntValue;
use std::error;

use crate::Expr;

/// LLVM JIT compiler.
///
/// NOTE (M0): this is an intentionally minimal compiler that lowers integer
/// literals and variadic `+` to native `i64` arithmetic. The full
/// statically-typed pipeline (typed AST, type checker, GC, structs, closures,
/// methods) is built out in later milestones; see docs and the plan file.
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
        let execution_engine = module.create_jit_execution_engine(OptimizationLevel::None)?;

        Ok(Compiler {
            context,
            module,
            builder,
            execution_engine,
        })
    }

    /// Compile a single expression into a `main` function and JIT-run it,
    /// returning the result as `Expr::Int`.
    pub fn compile_and_run(expr: &Expr) -> Result<Expr, Box<dyn error::Error>> {
        let context = Context::create();
        let compiler = Compiler::new(&context)?;

        let i64_type = compiler.context.i64_type();
        let fn_type = i64_type.fn_type(&[], false);
        let function = compiler.module.add_function("main", fn_type, None);
        let entry = compiler.context.append_basic_block(function, "entry");
        compiler.builder.position_at_end(entry);

        let value = compiler.lower(expr)?;
        compiler.builder.build_return(Some(&value))?;

        let result = unsafe { compiler.execution_engine.run_function(function, &[]) };
        Ok(Expr::Int(result.as_int(true) as i64))
    }

    /// Lower an expression to a native `i64` value.
    fn lower(&self, expr: &Expr) -> Result<IntValue<'ctx>, Box<dyn error::Error>> {
        let i64_type = self.context.i64_type();
        match expr {
            Expr::Int(value) => Ok(i64_type.const_int(*value as u64, true)),
            Expr::CallFunction(name, args) if name == "+" => {
                let mut acc = i64_type.const_int(0, true);
                if let Some(args) = args {
                    for e in args {
                        let x = self.lower(e)?;
                        acc = self.builder.build_int_add(acc, x, "add")?;
                    }
                }
                Ok(acc)
            }
            _ => unimplemented!(),
        }
    }
}
