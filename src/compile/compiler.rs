//! LLVM JIT code generation (native-primitive tier).
//!
//! Lowers the checked AST for integer/bool programs: literals, variables,
//! arithmetic/comparison, `let`, `if`/`cond`/`when`/`unless`/`progn`,
//! `while`/`dotimes`/`loop`, `setf`/`incf`/`decf`, and `defun` with calls and
//! recursion. Aggregates (String/Vec/struct/closure) and floats arrive in
//! later milestones; lowering them is an explicit error for now.

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::ExecutionEngine;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, IntType};
use inkwell::values::{BasicMetadataValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::{IntPredicate, OptimizationLevel};
use std::collections::HashMap;
use std::error;

use crate::{BinOp, Defun, ExprKind, Form, Place, Type, TypedExpr, UnOp};

type CErr = Box<dyn error::Error>;

struct FnInfo<'ctx> {
    value: FunctionValue<'ctx>,
    params: Vec<Type>,
    ret: Type,
}

pub struct Compiler<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    execution_engine: ExecutionEngine<'ctx>,
    functions: HashMap<String, FnInfo<'ctx>>,
    /// Lexical scopes for the function currently being compiled.
    scopes: Vec<HashMap<String, (PointerValue<'ctx>, Type)>>,
    /// `(continue_block, break_block)` stack for nested loops.
    loops: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
    temp: u64,
}

impl<'ctx> Compiler<'ctx> {
    fn new(context: &'ctx Context) -> Result<Compiler<'ctx>, CErr> {
        let module = context.create_module("main");
        let execution_engine = module.create_jit_execution_engine(OptimizationLevel::None)?;
        Ok(Compiler {
            context,
            builder: context.create_builder(),
            module,
            execution_engine,
            functions: HashMap::new(),
            scopes: Vec::new(),
            loops: Vec::new(),
            temp: 0,
        })
    }

    fn tmp(&mut self, base: &str) -> String {
        self.temp += 1;
        format!("{}{}", base, self.temp)
    }

    // ------------------------------------------------------------------
    // Public entry points
    // ------------------------------------------------------------------

    /// Compile and JIT-run a single expression, returning its `i64` value.
    pub fn compile_and_run(expr: &TypedExpr) -> Result<i64, CErr> {
        let context = Context::create();
        let mut c = Compiler::new(&context)?;
        let body = std::slice::from_ref(expr);
        c.build_and_run_main(body)
    }

    /// Compile a checked program's `defun`s and run its top-level expressions
    /// (as an implicit `main`), returning the last expression's `i64` value.
    pub fn run_program(forms: &[Form]) -> Result<i64, CErr> {
        let context = Context::create();
        let mut c = Compiler::new(&context)?;

        for form in forms {
            if let Form::Defun(d) = form {
                c.declare_fn(d)?;
            }
        }
        for form in forms {
            if let Form::Defun(d) = form {
                c.compile_fn(d)?;
            }
        }

        let top: Vec<TypedExpr> = forms
            .iter()
            .filter_map(|f| if let Form::Expr(e) = f { Some(e.clone()) } else { None })
            .collect();
        c.build_and_run_main(&top)
    }

    fn build_and_run_main(&mut self, body: &[TypedExpr]) -> Result<i64, CErr> {
        let i64_type = self.context.i64_type();
        let fn_type = i64_type.fn_type(&[], false);
        let function = self.module.add_function("__main__", fn_type, None);
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        self.scopes.push(HashMap::new());
        let mut last = i64_type.const_int(0, true);
        for e in body {
            last = self.lower(e)?;
        }
        self.scopes.pop();

        // widen the result to i64 for a uniform JIT return
        let ret = self.to_i64(last)?;
        self.builder.build_return(Some(&ret))?;

        let result = unsafe { self.execution_engine.run_function(function, &[]) };
        Ok(result.as_int(true) as i64)
    }

    // ------------------------------------------------------------------
    // Functions
    // ------------------------------------------------------------------

    fn declare_fn(&mut self, d: &Defun) -> Result<(), CErr> {
        let param_tys: Vec<BasicMetadataTypeEnum> = d
            .params
            .iter()
            .map(|(_, t)| self.int_type(t).map(|it| it.into()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| err("only integer/bool parameter types are supported so far"))?;

        let fn_type = if d.ret == Type::Unit {
            self.context.void_type().fn_type(&param_tys, false)
        } else {
            let rt = self
                .int_type(&d.ret)
                .ok_or_else(|| err("only integer/bool return types are supported so far"))?;
            rt.fn_type(&param_tys, false)
        };
        let value = self.module.add_function(&d.name, fn_type, None);
        self.functions.insert(
            d.name.clone(),
            FnInfo { value, params: d.params.iter().map(|(_, t)| t.clone()).collect(), ret: d.ret.clone() },
        );
        Ok(())
    }

    fn compile_fn(&mut self, d: &Defun) -> Result<(), CErr> {
        let function = self.functions.get(&d.name).unwrap().value;
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        self.scopes.push(HashMap::new());
        // bind params into alloca slots
        for (i, (name, ty)) in d.params.iter().enumerate() {
            let it = self.int_type(ty).ok_or_else(|| err("unsupported param type"))?;
            let slot = self.builder.build_alloca(it, name)?;
            let arg = function.get_nth_param(i as u32).unwrap().into_int_value();
            self.builder.build_store(slot, arg)?;
            self.scopes.last_mut().unwrap().insert(name.clone(), (slot, ty.clone()));
        }

        let mut last = self.context.i64_type().const_int(0, true);
        for e in &d.body {
            last = self.lower(e)?;
        }
        self.scopes.pop();

        if d.ret == Type::Unit {
            self.builder.build_return(None)?;
        } else {
            self.builder.build_return(Some(&last))?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Type lowering
    // ------------------------------------------------------------------

    fn int_type(&self, t: &Type) -> Option<IntType<'ctx>> {
        let it = match t {
            Type::I8 | Type::U8 => self.context.i8_type(),
            Type::I16 | Type::U16 => self.context.i16_type(),
            Type::I32 | Type::U32 | Type::Char => self.context.i32_type(),
            Type::I64 | Type::U64 | Type::Isize | Type::Usize => self.context.i64_type(),
            Type::Bool => self.context.bool_type(),
            _ => return None,
        };
        Some(it)
    }

    fn int_type_of(&self, e: &TypedExpr) -> IntType<'ctx> {
        e.ty
            .as_ref()
            .and_then(|t| self.int_type(t))
            .unwrap_or_else(|| self.context.i64_type())
    }

    /// Widen an integer value to i64 for a uniform JIT return (zero-extend a
    /// bool `i1`, sign-extend wider integers).
    fn to_i64(&mut self, v: IntValue<'ctx>) -> Result<IntValue<'ctx>, CErr> {
        let i64t = self.context.i64_type();
        let width = v.get_type().get_bit_width();
        if width == 64 {
            Ok(v)
        } else if width == 1 {
            let name = self.tmp("zext");
            Ok(self.builder.build_int_z_extend(v, i64t, &name)?)
        } else {
            let name = self.tmp("sext");
            Ok(self.builder.build_int_s_extend(v, i64t, &name)?)
        }
    }

    // ------------------------------------------------------------------
    // Expression lowering
    // ------------------------------------------------------------------

    fn lower(&mut self, e: &TypedExpr) -> Result<IntValue<'ctx>, CErr> {
        match &e.kind {
            ExprKind::Int(v, _) => Ok(self.int_type_of(e).const_int(*v as u64, true)),
            ExprKind::Bool(b) => Ok(self.context.bool_type().const_int(*b as u64, false)),
            ExprKind::Char(c) => Ok(self.context.i32_type().const_int(*c as u64, false)),
            ExprKind::Unit => Ok(self.context.i64_type().const_int(0, true)),

            ExprKind::Var(name) => {
                let (slot, ty) = self
                    .lookup(name)
                    .ok_or_else(|| err(&format!("unbound variable in codegen: {}", name)))?;
                let it = self.int_type(&ty).ok_or_else(|| err("unsupported var type"))?;
                let name2 = self.tmp("load");
                Ok(self.builder.build_load(it, slot, &name2)?.into_int_value())
            }

            ExprKind::BinOp { op, lhs, rhs } => self.lower_binop(*op, lhs, rhs, e),
            ExprKind::UnOp { op, operand } => self.lower_unop(*op, operand),

            ExprKind::Let { bindings, body } => {
                self.scopes.push(HashMap::new());
                let r = (|| {
                    for b in bindings {
                        let v = self.lower(&b.init)?;
                        let ty = b.init.ty.clone().or_else(|| b.ty.clone()).unwrap_or(Type::I64);
                        let it = v.get_type();
                        let slot = self.builder.build_alloca(it, &b.name)?;
                        self.builder.build_store(slot, v)?;
                        self.scopes.last_mut().unwrap().insert(b.name.clone(), (slot, ty));
                    }
                    self.lower_body(body)
                })();
                self.scopes.pop();
                r
            }

            ExprKind::If { cond, then, els } => self.lower_if(cond, then, els.as_deref(), e),
            ExprKind::Cond { clauses } => self.lower_cond(clauses, e),
            ExprKind::When { cond, body } => self.lower_when(cond, body, false),
            ExprKind::Unless { cond, body } => self.lower_when(cond, body, true),
            ExprKind::Progn { body } => self.lower_body(body),

            ExprKind::While { cond, body } => self.lower_while(cond, body),
            ExprKind::Dotimes { var, count, body } => self.lower_dotimes(var, count, body),
            ExprKind::Loop { body } => self.lower_loop(body),
            ExprKind::Break => self.lower_break(),

            ExprKind::Setf { place, value } => {
                let v = self.lower(value)?;
                self.store_place(place, v)?;
                Ok(self.context.i64_type().const_int(0, true))
            }
            ExprKind::Incf(place) => self.lower_incdec(place, 1),
            ExprKind::Decf(place) => self.lower_incdec(place, -1),

            ExprKind::Call { target, args } => self.lower_call(target, args),

            other => Err(err(&format!("codegen not yet implemented for {:?}", other))),
        }
    }

    fn lower_body(&mut self, body: &[TypedExpr]) -> Result<IntValue<'ctx>, CErr> {
        let mut last = self.context.i64_type().const_int(0, true);
        for e in body {
            last = self.lower(e)?;
        }
        Ok(last)
    }

    fn lower_binop(
        &mut self,
        op: BinOp,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        e: &TypedExpr,
    ) -> Result<IntValue<'ctx>, CErr> {
        let l = self.lower(lhs)?;
        let r = self.lower(rhs)?;
        let signed = lhs.ty.as_ref().map(|t| t.is_signed()).unwrap_or(true);
        let n = self.tmp("op");
        let v = match op {
            BinOp::Add => self.builder.build_int_add(l, r, &n)?,
            BinOp::Sub => self.builder.build_int_sub(l, r, &n)?,
            BinOp::Mul => self.builder.build_int_mul(l, r, &n)?,
            BinOp::Div if signed => self.builder.build_int_signed_div(l, r, &n)?,
            BinOp::Div => self.builder.build_int_unsigned_div(l, r, &n)?,
            BinOp::Rem if signed => self.builder.build_int_signed_rem(l, r, &n)?,
            BinOp::Rem => self.builder.build_int_unsigned_rem(l, r, &n)?,
            _ => {
                let pred = int_predicate(op, signed);
                return Ok(self.builder.build_int_compare(pred, l, r, &n)?);
            }
        };
        let _ = e;
        Ok(v)
    }

    fn lower_unop(&mut self, op: UnOp, operand: &TypedExpr) -> Result<IntValue<'ctx>, CErr> {
        let v = self.lower(operand)?;
        let n = self.tmp("un");
        match op {
            UnOp::Inc1 => {
                let one = v.get_type().const_int(1, false);
                Ok(self.builder.build_int_add(v, one, &n)?)
            }
            UnOp::Dec1 => {
                let one = v.get_type().const_int(1, false);
                Ok(self.builder.build_int_sub(v, one, &n)?)
            }
            UnOp::Not => Ok(self.builder.build_not(v, &n)?),
        }
    }

    fn lower_if(
        &mut self,
        cond: &TypedExpr,
        then: &TypedExpr,
        els: Option<&TypedExpr>,
        e: &TypedExpr,
    ) -> Result<IntValue<'ctx>, CErr> {
        let func = self.current_fn();
        let cv = self.lower(cond)?;

        let then_bb = self.context.append_basic_block(func, "then");
        let else_bb = self.context.append_basic_block(func, "else");
        let merge_bb = self.context.append_basic_block(func, "ifmerge");

        self.builder.build_conditional_branch(cv, then_bb, else_bb)?;

        // then
        self.builder.position_at_end(then_bb);
        let tv = self.lower(then)?;
        let then_end = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_bb)?;

        // else
        self.builder.position_at_end(else_bb);
        let ev = match els {
            Some(els) => self.lower(els)?,
            None => self.int_type_of(e).const_int(0, true),
        };
        let else_end = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_bb)?;

        // merge + phi
        self.builder.position_at_end(merge_bb);
        let phi_ty = tv.get_type();
        let name = self.tmp("ifphi");
        let phi = self.builder.build_phi(phi_ty, &name)?;
        phi.add_incoming(&[(&tv, then_end), (&ev, else_end)]);
        Ok(phi.as_basic_value().into_int_value())
    }

    fn lower_cond(&mut self, clauses: &[crate::CondClause], e: &TypedExpr) -> Result<IntValue<'ctx>, CErr> {
        // Desugar to a right-nested if/else chain.
        // Build from the back so each clause's else is the rest.
        let func = self.current_fn();
        let result_ty = self.int_type_of(e);
        let merge_bb = self.context.append_basic_block(func, "condmerge");
        let mut incomings: Vec<(IntValue<'ctx>, BasicBlock<'ctx>)> = Vec::new();

        for clause in clauses {
            let body_bb = self.context.append_basic_block(func, "condbody");
            let next_bb = self.context.append_basic_block(func, "condnext");
            let test = self.lower(&clause.test)?;
            self.builder.build_conditional_branch(test, body_bb, next_bb)?;

            self.builder.position_at_end(body_bb);
            let v = self.lower_body(&clause.body)?;
            let end = self.builder.get_insert_block().unwrap();
            incomings.push((v, end));
            self.builder.build_unconditional_branch(merge_bb)?;

            self.builder.position_at_end(next_bb);
        }
        // fell through all clauses: default value 0
        let dflt = result_ty.const_int(0, true);
        let dflt_end = self.builder.get_insert_block().unwrap();
        incomings.push((dflt, dflt_end));
        self.builder.build_unconditional_branch(merge_bb)?;

        self.builder.position_at_end(merge_bb);
        let name = self.tmp("condphi");
        let phi = self.builder.build_phi(result_ty, &name)?;
        let refs: Vec<(&dyn inkwell::values::BasicValue, BasicBlock)> =
            incomings.iter().map(|(v, b)| (v as &dyn inkwell::values::BasicValue, *b)).collect();
        phi.add_incoming(&refs);
        Ok(phi.as_basic_value().into_int_value())
    }

    fn lower_when(&mut self, cond: &TypedExpr, body: &[TypedExpr], negate: bool) -> Result<IntValue<'ctx>, CErr> {
        let func = self.current_fn();
        let mut cv = self.lower(cond)?;
        if negate {
            let n = self.tmp("not");
            cv = self.builder.build_not(cv, &n)?;
        }
        let body_bb = self.context.append_basic_block(func, "whenbody");
        let merge_bb = self.context.append_basic_block(func, "whenmerge");
        self.builder.build_conditional_branch(cv, body_bb, merge_bb)?;

        self.builder.position_at_end(body_bb);
        self.lower_body(body)?;
        self.builder.build_unconditional_branch(merge_bb)?;

        self.builder.position_at_end(merge_bb);
        Ok(self.context.i64_type().const_int(0, true))
    }

    fn lower_while(&mut self, cond: &TypedExpr, body: &[TypedExpr]) -> Result<IntValue<'ctx>, CErr> {
        let func = self.current_fn();
        let head = self.context.append_basic_block(func, "whilehead");
        let body_bb = self.context.append_basic_block(func, "whilebody");
        let exit = self.context.append_basic_block(func, "whileexit");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let cv = self.lower(cond)?;
        self.builder.build_conditional_branch(cv, body_bb, exit)?;

        self.builder.position_at_end(body_bb);
        self.loops.push((head, exit));
        self.lower_body(body)?;
        self.loops.pop();
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(exit);
        Ok(self.context.i64_type().const_int(0, true))
    }

    fn lower_dotimes(&mut self, var: &str, count: &TypedExpr, body: &[TypedExpr]) -> Result<IntValue<'ctx>, CErr> {
        let func = self.current_fn();
        let cv = self.lower(count)?;
        let it = cv.get_type();
        let slot = self.builder.build_alloca(it, var)?;
        self.builder.build_store(slot, it.const_int(0, false))?;

        self.scopes.push(HashMap::new());
        let var_ty = count.ty.clone().unwrap_or(Type::I64);
        self.scopes.last_mut().unwrap().insert(var.to_string(), (slot, var_ty));

        let head = self.context.append_basic_block(func, "dohead");
        let body_bb = self.context.append_basic_block(func, "dobody");
        let exit = self.context.append_basic_block(func, "doexit");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let cur = self.builder.build_load(it, slot, "doi")?.into_int_value();
        let cond = self.builder.build_int_compare(IntPredicate::SLT, cur, cv, "docmp")?;
        self.builder.build_conditional_branch(cond, body_bb, exit)?;

        self.builder.position_at_end(body_bb);
        self.loops.push((head, exit));
        self.lower_body(body)?;
        self.loops.pop();
        let cur2 = self.builder.build_load(it, slot, "doi2")?.into_int_value();
        let next = self.builder.build_int_add(cur2, it.const_int(1, false), "doinc")?;
        self.builder.build_store(slot, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(exit);
        self.scopes.pop();
        Ok(self.context.i64_type().const_int(0, true))
    }

    fn lower_loop(&mut self, body: &[TypedExpr]) -> Result<IntValue<'ctx>, CErr> {
        let func = self.current_fn();
        let head = self.context.append_basic_block(func, "loophead");
        let exit = self.context.append_basic_block(func, "loopexit");
        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        self.loops.push((head, exit));
        self.lower_body(body)?;
        self.loops.pop();
        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(exit);
        Ok(self.context.i64_type().const_int(0, true))
    }

    fn lower_break(&mut self) -> Result<IntValue<'ctx>, CErr> {
        let (_, exit) = *self.loops.last().ok_or_else(|| err("break outside of a loop"))?;
        self.builder.build_unconditional_branch(exit)?;
        // place subsequent (dead) code in a fresh block
        let func = self.current_fn();
        let dead = self.context.append_basic_block(func, "afterbreak");
        self.builder.position_at_end(dead);
        Ok(self.context.i64_type().const_int(0, true))
    }

    fn lower_incdec(&mut self, place: &Place, delta: i64) -> Result<IntValue<'ctx>, CErr> {
        let (slot, ty) = self.resolve_place(place)?;
        let it = self.int_type(&ty).ok_or_else(|| err("unsupported place type"))?;
        let cur = self.builder.build_load(it, slot, "cur")?.into_int_value();
        let d = it.const_int(delta as u64, true);
        let n = self.tmp("incdec");
        let next = self.builder.build_int_add(cur, d, &n)?;
        self.builder.build_store(slot, next)?;
        Ok(self.context.i64_type().const_int(0, true))
    }

    fn lower_call(&mut self, target: &crate::CallTarget, args: &[TypedExpr]) -> Result<IntValue<'ctx>, CErr> {
        use crate::CallTarget;
        let name = match target {
            CallTarget::Sym(s) => s.clone(),
            other => return Err(err(&format!("codegen call target not supported yet: {:?}", other))),
        };
        let info = self
            .functions
            .get(&name)
            .ok_or_else(|| err(&format!("call to unknown function: {}", name)))?;
        let fv = info.value;
        let ret = info.ret.clone();

        let mut argvals: Vec<BasicMetadataValueEnum> = Vec::new();
        for a in args {
            argvals.push(self.lower(a)?.into());
        }
        let n = self.tmp("call");
        let call = self.builder.build_call(fv, &argvals, &n)?;
        match call.try_as_basic_value().left() {
            Some(v) => Ok(v.into_int_value()),
            None => {
                // unit-returning call
                let _ = ret;
                Ok(self.context.i64_type().const_int(0, true))
            }
        }
    }

    // ------------------------------------------------------------------
    // places & scopes
    // ------------------------------------------------------------------

    fn store_place(&mut self, place: &Place, v: IntValue<'ctx>) -> Result<(), CErr> {
        let (slot, _) = self.resolve_place(place)?;
        self.builder.build_store(slot, v)?;
        Ok(())
    }

    fn resolve_place(&mut self, place: &Place) -> Result<(PointerValue<'ctx>, Type), CErr> {
        match place {
            Place::Var(name) => self
                .lookup(name)
                .ok_or_else(|| err(&format!("unbound place: {}", name))),
            other => Err(err(&format!("codegen place not supported yet: {:?}", other))),
        }
    }

    fn lookup(&self, name: &str) -> Option<(PointerValue<'ctx>, Type)> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn current_fn(&self) -> FunctionValue<'ctx> {
        self.builder.get_insert_block().unwrap().get_parent().unwrap()
    }
}

fn err(msg: &str) -> CErr {
    Box::<dyn error::Error>::from(msg.to_string())
}

fn int_predicate(op: BinOp, signed: bool) -> IntPredicate {
    match op {
        BinOp::Eq => IntPredicate::EQ,
        BinOp::Ne => IntPredicate::NE,
        BinOp::Lt => if signed { IntPredicate::SLT } else { IntPredicate::ULT },
        BinOp::Gt => if signed { IntPredicate::SGT } else { IntPredicate::UGT },
        BinOp::Le => if signed { IntPredicate::SLE } else { IntPredicate::ULE },
        BinOp::Ge => if signed { IntPredicate::SGE } else { IntPredicate::UGE },
        _ => IntPredicate::EQ,
    }
}
