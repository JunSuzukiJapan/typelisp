use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::ExecutionEngine;
use inkwell::module::Module;
use inkwell::values::IntValue;
use std::error;

use crate::{BinOp, ExprKind, TypedExpr};

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
    /// returning the result as a native `i64`.
    ///
    /// NOTE (M3 shim): only integer literals and integer arithmetic lower for
    /// now; the full pipeline (type checker, GC, structs, ...) arrives in later
    /// milestones.
    pub fn compile_and_run(expr: &TypedExpr) -> Result<i64, Box<dyn error::Error>> {
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
        Ok(result.as_int(true) as i64)
    }

    /// Lower an expression to a native `i64` value.
    fn lower(&self, expr: &TypedExpr) -> Result<IntValue<'ctx>, Box<dyn error::Error>> {
        let i64_type = self.context.i64_type();
        match &expr.kind {
            ExprKind::Int(value, _) => Ok(i64_type.const_int(*value as u64, true)),
            ExprKind::BinOp { op, lhs, rhs } => {
                let l = self.lower(lhs)?;
                let r = self.lower(rhs)?;
                let v = match op {
                    BinOp::Add => self.builder.build_int_add(l, r, "add")?,
                    BinOp::Sub => self.builder.build_int_sub(l, r, "sub")?,
                    BinOp::Mul => self.builder.build_int_mul(l, r, "mul")?,
                    BinOp::Div => self.builder.build_int_signed_div(l, r, "div")?,
                    BinOp::Rem => self.builder.build_int_signed_rem(l, r, "rem")?,
                    _ => unimplemented!("comparison lowering arrives in a later milestone"),
                };
                Ok(v)
            }
            _ => unimplemented!("expression lowering arrives in later milestones"),
        }
    }
}
