//! LLVM JIT code generation.
//!
//! Native primitives are unboxed; aggregates (currently `defstruct` instances)
//! are GC-managed heap objects referenced by pointer. The GC runtime
//! (`crate::gc`) is reached via `extern "C"` symbols bound with
//! `add_global_mapping`.
//!
//! Root management: every GC-pointer *named local* (struct-typed params and
//! `let` bindings) lives in an alloca slot registered in a per-function shadow
//! frame, pushed on entry and popped before the (single) return. Anonymous
//! struct temporaries held across a nested allocation are not yet rooted (a
//! documented limitation; collection only triggers past a 1 MiB threshold).

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::ExecutionEngine;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, IntType, StructType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};
use inkwell::{AddressSpace, IntPredicate, OptimizationLevel};
use std::collections::HashMap;
use std::error;

use crate::gc::{self, TypeInfo};
use crate::{BinOp, CallTarget, DefStruct, Defun, ExprKind, Form, Place, Type, TypedExpr, UnOp};

type CErr = Box<dyn error::Error>;

struct FnInfo<'ctx> {
    value: FunctionValue<'ctx>,
    ret: Type,
}

struct StructLayout<'ctx> {
    payload_ty: StructType<'ctx>,
    fields: Vec<(String, Type)>,
    ti: *const TypeInfo,
}

impl<'ctx> StructLayout<'ctx> {
    fn field_index(&self, name: &str) -> Option<(u32, Type)> {
        self.fields
            .iter()
            .position(|(n, _)| n == name)
            .map(|i| (i as u32, self.fields[i].1.clone()))
    }
}

struct RtFns<'ctx> {
    gc_alloc: FunctionValue<'ctx>,
    gc_push_frame: FunctionValue<'ctx>,
    gc_pop_frame: FunctionValue<'ctx>,
}

pub struct Compiler<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    execution_engine: ExecutionEngine<'ctx>,
    functions: HashMap<String, FnInfo<'ctx>>,
    structs: HashMap<String, StructLayout<'ctx>>,
    rt: RtFns<'ctx>,
    scopes: Vec<HashMap<String, (PointerValue<'ctx>, Type)>>,
    loops: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
    /// Pre-reserved root slots for the function currently being compiled.
    root_slots: Vec<PointerValue<'ctx>>,
    next_root: usize,
    temp: u64,
}

impl<'ctx> Compiler<'ctx> {
    fn new(context: &'ctx Context) -> Result<Compiler<'ctx>, CErr> {
        let module = context.create_module("main");
        let execution_engine = module.create_jit_execution_engine(OptimizationLevel::None)?;

        let ptr = context.ptr_type(AddressSpace::default());
        let i64t = context.i64_type();
        let void = context.void_type();

        let gc_alloc = module.add_function(
            "gc_alloc",
            ptr.fn_type(&[i64t.into(), ptr.into()], false),
            None,
        );
        let gc_push_frame =
            module.add_function("gc_push_frame", void.fn_type(&[ptr.into()], false), None);
        let gc_pop_frame =
            module.add_function("gc_pop_frame", void.fn_type(&[], false), None);

        execution_engine.add_global_mapping(&gc_alloc, gc::gc_alloc as usize);
        execution_engine.add_global_mapping(&gc_push_frame, gc::gc_push_frame as usize);
        execution_engine.add_global_mapping(&gc_pop_frame, gc::gc_pop_frame as usize);

        Ok(Compiler {
            context,
            builder: context.create_builder(),
            module,
            execution_engine,
            functions: HashMap::new(),
            structs: HashMap::new(),
            rt: RtFns { gc_alloc, gc_push_frame, gc_pop_frame },
            scopes: Vec::new(),
            loops: Vec::new(),
            root_slots: Vec::new(),
            next_root: 0,
            temp: 0,
        })
    }

    fn tmp(&mut self, base: &str) -> String {
        self.temp += 1;
        format!("{}{}", base, self.temp)
    }

    fn ptr_ty(&self) -> inkwell::types::PointerType<'ctx> {
        self.context.ptr_type(AddressSpace::default())
    }

    // ------------------------------------------------------------------
    // Public entry points
    // ------------------------------------------------------------------

    pub fn compile_and_run(expr: &TypedExpr) -> Result<i64, CErr> {
        let context = Context::create();
        let mut c = Compiler::new(&context)?;
        let body = std::slice::from_ref(expr);
        gc::gc_init();
        let r = c.build_and_run_main(body);
        gc::gc_shutdown();
        r
    }

    pub fn run_program(forms: &[Form]) -> Result<i64, CErr> {
        let context = Context::create();
        let mut c = Compiler::new(&context)?;

        for form in forms {
            if let Form::DefStruct(s) = form {
                c.declare_struct(s)?;
            }
        }
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

        gc::gc_init();
        let r = c.build_and_run_main(&top);
        gc::gc_shutdown();
        r
    }

    fn build_and_run_main(&mut self, body: &[TypedExpr]) -> Result<i64, CErr> {
        let i64_type = self.context.i64_type();
        let fn_type = i64_type.fn_type(&[], false);
        let function = self.module.add_function("__main__", fn_type, None);
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        let n_roots = self.count_gc_lets_in(body);
        self.begin_frame(function, n_roots)?;

        self.scopes.push(HashMap::new());
        let mut last: BasicValueEnum = i64_type.const_int(0, true).into();
        for e in body {
            last = self.lower(e)?;
        }
        self.scopes.pop();

        self.end_frame()?;
        let ret = self.to_i64(last.into_int_value())?;
        self.builder.build_return(Some(&ret))?;

        let result = unsafe { self.execution_engine.run_function(function, &[]) };
        Ok(result.as_int(true) as i64)
    }

    // ------------------------------------------------------------------
    // Structs
    // ------------------------------------------------------------------

    fn declare_struct(&mut self, s: &DefStruct) -> Result<(), CErr> {
        let mut fields: Vec<(String, Type)> = s.pub_fields.clone();
        fields.extend(s.priv_fields.clone());

        // Build the payload LLVM struct type (no header; the header precedes the
        // payload pointer and is managed by the runtime).
        let field_tys: Vec<BasicTypeEnum> = fields
            .iter()
            .map(|(_, t)| self.basic_type(t).ok_or_else(|| err("unsupported field type")))
            .collect::<Result<_, _>>()?;
        let payload_ty = self.context.opaque_struct_type(&s.name);
        payload_ty.set_body(&field_tys, false);

        // Compute GC-pointer field byte offsets and the payload size.
        let td = self.execution_engine.get_target_data();
        let mut ptr_offsets: Vec<usize> = Vec::new();
        for (i, (_, t)) in fields.iter().enumerate() {
            if self.is_gc_type(t) {
                if let Some(off) = td.offset_of_element(&payload_ty, i as u32) {
                    ptr_offsets.push(off as usize);
                }
            }
        }
        let fixed_size = td.get_store_size(&payload_ty) as usize;

        // Leak a 'static TypeInfo for this struct (JIT-lifetime).
        let offsets: &'static [usize] = Box::leak(ptr_offsets.into_boxed_slice());
        let ti: &'static TypeInfo = Box::leak(Box::new(TypeInfo {
            kind: gc::GcKind::Struct,
            fixed_size,
            ptr_offsets: offsets,
            elem: None,
        }));

        self.structs.insert(
            s.name.clone(),
            StructLayout { payload_ty, fields, ti: ti as *const TypeInfo },
        );
        Ok(())
    }

    // ------------------------------------------------------------------
    // Functions
    // ------------------------------------------------------------------

    fn declare_fn(&mut self, d: &Defun) -> Result<(), CErr> {
        let param_tys: Vec<BasicMetadataTypeEnum> = d
            .params
            .iter()
            .map(|(_, t)| self.basic_type(t).map(|b| b.into()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| err("unsupported parameter type"))?;

        let fn_type = if d.ret == Type::Unit {
            self.context.void_type().fn_type(&param_tys, false)
        } else {
            let rt = self.basic_type(&d.ret).ok_or_else(|| err("unsupported return type"))?;
            rt.fn_type(&param_tys, false)
        };
        let value = self.module.add_function(&d.name, fn_type, None);
        self.functions.insert(d.name.clone(), FnInfo { value, ret: d.ret.clone() });
        Ok(())
    }

    fn compile_fn(&mut self, d: &Defun) -> Result<(), CErr> {
        let function = self.functions.get(&d.name).unwrap().value;
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        let n_gc_params = d.params.iter().filter(|(_, t)| self.is_gc_type(t)).count();
        let n_roots = n_gc_params + self.count_gc_lets(&d.body);
        self.begin_frame(function, n_roots)?;

        self.scopes.push(HashMap::new());
        for (i, (name, ty)) in d.params.iter().enumerate() {
            let arg = function.get_nth_param(i as u32).unwrap();
            let slot = if self.is_gc_type(ty) {
                self.take_root_slot()
            } else {
                let bt = self.basic_type(ty).ok_or_else(|| err("unsupported param type"))?;
                self.builder.build_alloca(bt, name)?
            };
            self.builder.build_store(slot, arg)?;
            self.scopes.last_mut().unwrap().insert(name.clone(), (slot, ty.clone()));
        }

        let mut last: BasicValueEnum = self.context.i64_type().const_int(0, true).into();
        for e in &d.body {
            last = self.lower(e)?;
        }
        self.scopes.pop();

        self.end_frame()?;
        if d.ret == Type::Unit {
            self.builder.build_return(None)?;
        } else {
            self.builder.build_return(Some(&last))?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Shadow frame
    // ------------------------------------------------------------------

    fn begin_frame(&mut self, function: FunctionValue<'ctx>, n_roots: usize) -> Result<(), CErr> {
        self.root_slots.clear();
        self.next_root = 0;
        if n_roots == 0 {
            return Ok(());
        }
        let _ = function;
        let ptr = self.ptr_ty();
        let i64t = self.context.i64_type();

        // root slots (each holds one GC pointer), zero-initialised
        for _ in 0..n_roots {
            let s = self.builder.build_alloca(ptr, "root")?;
            self.builder.build_store(s, ptr.const_null())?;
            self.root_slots.push(s);
        }
        // slots array: [N x ptr] holding pointers to the root slots
        let arr_ty = ptr.array_type(n_roots as u32);
        let arr = self.builder.build_alloca(arr_ty, "roots")?;
        for (i, s) in self.root_slots.clone().iter().enumerate() {
            let gep = unsafe {
                self.builder.build_in_bounds_gep(
                    arr_ty,
                    arr,
                    &[i64t.const_int(0, false), i64t.const_int(i as u64, false)],
                    "rootslot",
                )?
            };
            self.builder.build_store(gep, *s)?;
        }
        // frame { prev: ptr, count: i64, slots: ptr }
        let frame_ty = self.context.struct_type(&[ptr.into(), i64t.into(), ptr.into()], false);
        let frame = self.builder.build_alloca(frame_ty, "frame")?;
        let prev = self.builder.build_struct_gep(frame_ty, frame, 0, "prev")?;
        self.builder.build_store(prev, ptr.const_null())?;
        let cnt = self.builder.build_struct_gep(frame_ty, frame, 1, "count")?;
        self.builder.build_store(cnt, i64t.const_int(n_roots as u64, false))?;
        let slots = self.builder.build_struct_gep(frame_ty, frame, 2, "slotsptr")?;
        let arr0 = unsafe {
            self.builder.build_in_bounds_gep(
                arr_ty,
                arr,
                &[i64t.const_int(0, false), i64t.const_int(0, false)],
                "arr0",
            )?
        };
        self.builder.build_store(slots, arr0)?;

        self.builder.build_call(self.rt.gc_push_frame, &[frame.into()], "pushframe")?;
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), CErr> {
        if !self.root_slots.is_empty() {
            self.builder.build_call(self.rt.gc_pop_frame, &[], "popframe")?;
        }
        Ok(())
    }

    fn take_root_slot(&mut self) -> PointerValue<'ctx> {
        let s = self.root_slots[self.next_root];
        self.next_root += 1;
        s
    }

    /// Count `let` bindings of GC type in a body (upper bound on root slots).
    fn count_gc_lets_in(&self, body: &[TypedExpr]) -> usize {
        body.iter().map(|e| self.count_gc_lets(std::slice::from_ref(e))).sum()
    }

    fn count_gc_lets(&self, body: &[TypedExpr]) -> usize {
        let mut n = 0;
        for e in body {
            self.walk_count(e, &mut n);
        }
        n
    }

    fn walk_count(&self, e: &TypedExpr, n: &mut usize) {
        use ExprKind::*;
        match &e.kind {
            Let { bindings, body } => {
                for b in bindings {
                    let ty = b.ty.as_ref().or(b.init.ty.as_ref());
                    if matches!(ty, Some(t) if self.is_gc_type(t)) {
                        *n += 1;
                    }
                    self.walk_count(&b.init, n);
                }
                for x in body {
                    self.walk_count(x, n);
                }
            }
            If { cond, then, els } => {
                self.walk_count(cond, n);
                self.walk_count(then, n);
                if let Some(e) = els {
                    self.walk_count(e, n);
                }
            }
            When { cond, body } | Unless { cond, body } | While { cond, body } => {
                self.walk_count(cond, n);
                for x in body {
                    self.walk_count(x, n);
                }
            }
            Dotimes { count, body, .. } => {
                self.walk_count(count, n);
                for x in body {
                    self.walk_count(x, n);
                }
            }
            Loop { body } | Progn { body } => {
                for x in body {
                    self.walk_count(x, n);
                }
            }
            Cond { clauses } => {
                for c in clauses {
                    self.walk_count(&c.test, n);
                    for x in &c.body {
                        self.walk_count(x, n);
                    }
                }
            }
            BinOp { lhs, rhs, .. } => {
                self.walk_count(lhs, n);
                self.walk_count(rhs, n);
            }
            UnOp { operand, .. } => self.walk_count(operand, n),
            Setf { value, .. } => self.walk_count(value, n),
            Call { args, .. } => {
                for a in args {
                    self.walk_count(a, n);
                }
            }
            MethodCall { receiver, args, .. } => {
                self.walk_count(receiver, n);
                for a in args {
                    self.walk_count(a, n);
                }
            }
            FieldAccess { object, .. } => self.walk_count(object, n),
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Type lowering
    // ------------------------------------------------------------------

    fn is_gc_type(&self, t: &Type) -> bool {
        matches!(t, Type::Named(n, _) if self.structs.contains_key(n))
    }

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

    fn basic_type(&self, t: &Type) -> Option<BasicTypeEnum<'ctx>> {
        if let Some(it) = self.int_type(t) {
            return Some(it.into());
        }
        if self.is_gc_type(t) {
            return Some(self.ptr_ty().into());
        }
        None
    }

    fn int_type_of(&self, e: &TypedExpr) -> IntType<'ctx> {
        e.ty
            .as_ref()
            .and_then(|t| self.int_type(t))
            .unwrap_or_else(|| self.context.i64_type())
    }

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

    fn lower_int(&mut self, e: &TypedExpr) -> Result<IntValue<'ctx>, CErr> {
        Ok(self.lower(e)?.into_int_value())
    }

    fn lower(&mut self, e: &TypedExpr) -> Result<BasicValueEnum<'ctx>, CErr> {
        match &e.kind {
            ExprKind::Int(v, _) => Ok(self.int_type_of(e).const_int(*v as u64, true).into()),
            ExprKind::Bool(b) => Ok(self.context.bool_type().const_int(*b as u64, false).into()),
            ExprKind::Char(c) => Ok(self.context.i32_type().const_int(*c as u64, false).into()),
            ExprKind::Unit => Ok(self.context.i64_type().const_int(0, true).into()),

            ExprKind::Var(name) => {
                let (slot, ty) = self
                    .lookup(name)
                    .ok_or_else(|| err(&format!("unbound variable in codegen: {}", name)))?;
                let bt = self.basic_type(&ty).ok_or_else(|| err("unsupported var type"))?;
                let n = self.tmp("load");
                Ok(self.builder.build_load(bt, slot, &n)?)
            }

            ExprKind::BinOp { op, lhs, rhs } => self.lower_binop(*op, lhs, rhs),
            ExprKind::UnOp { op, operand } => self.lower_unop(*op, operand),

            ExprKind::Let { bindings, body } => {
                self.scopes.push(HashMap::new());
                let r = (|| {
                    for b in bindings {
                        let v = self.lower(&b.init)?;
                        let ty = b.ty.clone().or_else(|| b.init.ty.clone()).unwrap_or(Type::I64);
                        let slot = if self.is_gc_type(&ty) {
                            self.take_root_slot()
                        } else {
                            self.builder.build_alloca(v.get_type(), &b.name)?
                        };
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
                Ok(self.unit())
            }
            ExprKind::Incf(place) => self.lower_incdec(place, 1),
            ExprKind::Decf(place) => self.lower_incdec(place, -1),

            ExprKind::FieldAccess { object, field } => self.lower_field_access(object, field),
            ExprKind::MethodCall { method, receiver, args } => {
                // `.field` sugar (no args) -> field read. Real methods: M13.
                if args.is_empty() {
                    if let Some(Type::Named(sname, _)) = &receiver.ty {
                        if let Some(layout) = self.structs.get(sname) {
                            if layout.field_index(method).is_some() {
                                return self.lower_field_access(receiver, method);
                            }
                        }
                    }
                }
                Err(err("method-call codegen arrives in M13"))
            }

            ExprKind::Call { target, args } => self.lower_call(target, args),

            other => Err(err(&format!("codegen not yet implemented for {:?}", other))),
        }
    }

    fn unit(&self) -> BasicValueEnum<'ctx> {
        self.context.i64_type().const_int(0, true).into()
    }

    fn lower_body(&mut self, body: &[TypedExpr]) -> Result<BasicValueEnum<'ctx>, CErr> {
        let mut last = self.unit();
        for e in body {
            last = self.lower(e)?;
        }
        Ok(last)
    }

    fn lower_binop(&mut self, op: BinOp, lhs: &TypedExpr, rhs: &TypedExpr) -> Result<BasicValueEnum<'ctx>, CErr> {
        let l = self.lower_int(lhs)?;
        let r = self.lower_int(rhs)?;
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
            _ => self.builder.build_int_compare(int_predicate(op, signed), l, r, &n)?,
        };
        Ok(v.into())
    }

    fn lower_unop(&mut self, op: UnOp, operand: &TypedExpr) -> Result<BasicValueEnum<'ctx>, CErr> {
        let v = self.lower_int(operand)?;
        let n = self.tmp("un");
        let r = match op {
            UnOp::Inc1 => self.builder.build_int_add(v, v.get_type().const_int(1, false), &n)?,
            UnOp::Dec1 => self.builder.build_int_sub(v, v.get_type().const_int(1, false), &n)?,
            UnOp::Not => self.builder.build_not(v, &n)?,
        };
        Ok(r.into())
    }

    fn lower_if(
        &mut self,
        cond: &TypedExpr,
        then: &TypedExpr,
        els: Option<&TypedExpr>,
        e: &TypedExpr,
    ) -> Result<BasicValueEnum<'ctx>, CErr> {
        let func = self.current_fn();
        let cv = self.lower_int(cond)?;

        let then_bb = self.context.append_basic_block(func, "then");
        let else_bb = self.context.append_basic_block(func, "else");
        let merge_bb = self.context.append_basic_block(func, "ifmerge");
        self.builder.build_conditional_branch(cv, then_bb, else_bb)?;

        self.builder.position_at_end(then_bb);
        let tv = self.lower(then)?;
        let then_end = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_bb)?;

        self.builder.position_at_end(else_bb);
        let ev = match els {
            Some(els) => self.lower(els)?,
            None => self.unit(),
        };
        let else_end = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_bb)?;

        self.builder.position_at_end(merge_bb);
        let _ = e;
        let name = self.tmp("ifphi");
        let phi = self.builder.build_phi(tv.get_type(), &name)?;
        phi.add_incoming(&[(&tv, then_end), (&ev, else_end)]);
        Ok(phi.as_basic_value())
    }

    fn lower_cond(&mut self, clauses: &[crate::CondClause], e: &TypedExpr) -> Result<BasicValueEnum<'ctx>, CErr> {
        let func = self.current_fn();
        let result_ty = self.int_type_of(e);
        let merge_bb = self.context.append_basic_block(func, "condmerge");
        let mut incomings: Vec<(BasicValueEnum<'ctx>, BasicBlock<'ctx>)> = Vec::new();

        for clause in clauses {
            let body_bb = self.context.append_basic_block(func, "condbody");
            let next_bb = self.context.append_basic_block(func, "condnext");
            let test = self.lower_int(&clause.test)?;
            self.builder.build_conditional_branch(test, body_bb, next_bb)?;
            self.builder.position_at_end(body_bb);
            let v = self.lower_body(&clause.body)?;
            let end = self.builder.get_insert_block().unwrap();
            incomings.push((v, end));
            self.builder.build_unconditional_branch(merge_bb)?;
            self.builder.position_at_end(next_bb);
        }
        let dflt: BasicValueEnum = result_ty.const_int(0, true).into();
        let dflt_end = self.builder.get_insert_block().unwrap();
        incomings.push((dflt, dflt_end));
        self.builder.build_unconditional_branch(merge_bb)?;

        self.builder.position_at_end(merge_bb);
        let name = self.tmp("condphi");
        let phi = self.builder.build_phi(result_ty, &name)?;
        let refs: Vec<(&dyn inkwell::values::BasicValue, BasicBlock)> =
            incomings.iter().map(|(v, b)| (v as &dyn inkwell::values::BasicValue, *b)).collect();
        phi.add_incoming(&refs);
        Ok(phi.as_basic_value())
    }

    fn lower_when(&mut self, cond: &TypedExpr, body: &[TypedExpr], negate: bool) -> Result<BasicValueEnum<'ctx>, CErr> {
        let func = self.current_fn();
        let mut cv = self.lower_int(cond)?;
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
        Ok(self.unit())
    }

    fn lower_while(&mut self, cond: &TypedExpr, body: &[TypedExpr]) -> Result<BasicValueEnum<'ctx>, CErr> {
        let func = self.current_fn();
        let head = self.context.append_basic_block(func, "whilehead");
        let body_bb = self.context.append_basic_block(func, "whilebody");
        let exit = self.context.append_basic_block(func, "whileexit");
        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let cv = self.lower_int(cond)?;
        self.builder.build_conditional_branch(cv, body_bb, exit)?;
        self.builder.position_at_end(body_bb);
        self.loops.push((head, exit));
        self.lower_body(body)?;
        self.loops.pop();
        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(exit);
        Ok(self.unit())
    }

    fn lower_dotimes(&mut self, var: &str, count: &TypedExpr, body: &[TypedExpr]) -> Result<BasicValueEnum<'ctx>, CErr> {
        let func = self.current_fn();
        let cv = self.lower_int(count)?;
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
        Ok(self.unit())
    }

    fn lower_loop(&mut self, body: &[TypedExpr]) -> Result<BasicValueEnum<'ctx>, CErr> {
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
        Ok(self.unit())
    }

    fn lower_break(&mut self) -> Result<BasicValueEnum<'ctx>, CErr> {
        let (_, exit) = *self.loops.last().ok_or_else(|| err("break outside of a loop"))?;
        self.builder.build_unconditional_branch(exit)?;
        let func = self.current_fn();
        let dead = self.context.append_basic_block(func, "afterbreak");
        self.builder.position_at_end(dead);
        Ok(self.unit())
    }

    fn lower_incdec(&mut self, place: &Place, delta: i64) -> Result<BasicValueEnum<'ctx>, CErr> {
        let (slot, ty) = self.resolve_place(place)?;
        let it = self.int_type(&ty).ok_or_else(|| err("unsupported place type"))?;
        let cur = self.builder.build_load(it, slot, "cur")?.into_int_value();
        let n = self.tmp("incdec");
        let next = self.builder.build_int_add(cur, it.const_int(delta as u64, true), &n)?;
        self.builder.build_store(slot, next)?;
        Ok(self.unit())
    }

    // ------------------------------------------------------------------
    // Structs: construction & access
    // ------------------------------------------------------------------

    fn lower_struct_ctor(&mut self, name: &str, args: &[TypedExpr]) -> Result<BasicValueEnum<'ctx>, CErr> {
        let (payload_ty, ti, fixed) = {
            let layout = self.structs.get(name).unwrap();
            let td = self.execution_engine.get_target_data();
            (layout.payload_ty, layout.ti, td.get_store_size(&layout.payload_ty))
        };
        let i64t = self.context.i64_type();
        let ti_const = i64t.const_int(ti as usize as u64, false);
        let ti_ptr = self.builder.build_int_to_ptr(ti_const, self.ptr_ty(), "ti")?;
        let size = i64t.const_int(fixed, false);
        let n = self.tmp("alloc");
        let raw = self
            .builder
            .build_call(self.rt.gc_alloc, &[size.into(), ti_ptr.into()], &n)?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| err("gc_alloc returned void"))?
            .into_pointer_value();

        for (i, a) in args.iter().enumerate() {
            let v = self.lower(a)?;
            let gep = self.builder.build_struct_gep(payload_ty, raw, i as u32, "field")?;
            self.builder.build_store(gep, v)?;
        }
        Ok(raw.into())
    }

    fn lower_field_access(&mut self, object: &TypedExpr, field: &str) -> Result<BasicValueEnum<'ctx>, CErr> {
        let sname = match &object.ty {
            Some(Type::Named(n, _)) => n.clone(),
            _ => return Err(err("field access on non-struct")),
        };
        let (payload_ty, idx, fty) = {
            let layout = self.structs.get(&sname).ok_or_else(|| err("unknown struct"))?;
            let (idx, fty) = layout.field_index(field).ok_or_else(|| err("unknown field"))?;
            (layout.payload_ty, idx, fty)
        };
        let obj = self.lower(object)?.into_pointer_value();
        let gep = self.builder.build_struct_gep(payload_ty, obj, idx, "fieldp")?;
        let bt = self.basic_type(&fty).ok_or_else(|| err("unsupported field type"))?;
        let n = self.tmp("fload");
        Ok(self.builder.build_load(bt, gep, &n)?)
    }

    fn lower_call(&mut self, target: &CallTarget, args: &[TypedExpr]) -> Result<BasicValueEnum<'ctx>, CErr> {
        let name = match target {
            CallTarget::Sym(s) => s.clone(),
            other => return Err(err(&format!("codegen call target not supported yet: {:?}", other))),
        };
        // struct constructor?
        if self.structs.contains_key(&name) {
            return self.lower_struct_ctor(&name, args);
        }
        let (fv, ret) = {
            let info = self
                .functions
                .get(&name)
                .ok_or_else(|| err(&format!("call to unknown function: {}", name)))?;
            (info.value, info.ret.clone())
        };
        let mut argvals: Vec<BasicMetadataValueEnum> = Vec::new();
        for a in args {
            argvals.push(self.lower(a)?.into());
        }
        let n = self.tmp("call");
        let call = self.builder.build_call(fv, &argvals, &n)?;
        match call.try_as_basic_value().left() {
            Some(v) => Ok(v),
            None => {
                let _ = ret;
                Ok(self.unit())
            }
        }
    }

    // ------------------------------------------------------------------
    // places & scopes
    // ------------------------------------------------------------------

    fn store_place(&mut self, place: &Place, v: BasicValueEnum<'ctx>) -> Result<(), CErr> {
        match place {
            Place::Var(_) => {
                let (slot, _) = self.resolve_place(place)?;
                self.builder.build_store(slot, v)?;
                Ok(())
            }
            Place::Field { object, field } => {
                let sname = match &object.ty {
                    Some(Type::Named(n, _)) => n.clone(),
                    _ => return Err(err("setf field on non-struct")),
                };
                let (payload_ty, idx) = {
                    let layout = self.structs.get(&sname).ok_or_else(|| err("unknown struct"))?;
                    let (idx, _) = layout.field_index(field).ok_or_else(|| err("unknown field"))?;
                    (layout.payload_ty, idx)
                };
                let obj = self.lower(object)?.into_pointer_value();
                let gep = self.builder.build_struct_gep(payload_ty, obj, idx, "setfp")?;
                self.builder.build_store(gep, v)?;
                Ok(())
            }
            other => Err(err(&format!("codegen place not supported yet: {:?}", other))),
        }
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
