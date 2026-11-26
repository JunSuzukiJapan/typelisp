//! Static globals: global variables of an executable that are LLVM global
//! variables, of the type their values are carried in, instead of places in
//! the runtime's table.
//!
//! Only an executable built ahead of time has any, and only for globals the
//! collector has nothing to see in (`aot` decides which).
//! Which ones, by the id the compiled code names a global by, is a table for
//! the build (here); the island asks nothing about it. The builders it reads,
//! writes and makes a global with (`global-get`, `global-put`, `global-new`,
//! below) look the id up.
//!
//! One that is assigned is read and written as `volatile`: an interrupt
//! handler or another task may be what the program waits for. On a machine
//! whose tasks run on several threads (the 64-bit ones) an access is atomic
//! too. On one whose instructions access a byte, what an interrupt handler
//! must not come between the bytes of is seen to once all of it is emitted
//! (`aot::masking`).

use std::cell::RefCell;
use std::collections::HashMap;

use inkwell::builder::Builder;
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicTypeEnum, IntType};
use inkwell::values::{BasicValue, BasicValueEnum, GlobalValue, InstructionValue, IntValue};
use inkwell::AtomicOrdering;

use typelisp_mem::Value;

use super::values::{tag_field, untag_field, value_or_return};
use super::{expect_int, expect_llvm_builder, expect_llvm_module, llvm_value_value};
use crate::compile::carriers::{self, Carrier};
use crate::compile::{raise_check, runtime_abi};
use crate::eval::interp::EvalError;

thread_local! {
    /// The build's static globals by id, and whether each can be assigned.
    static STATIC: RefCell<HashMap<i64, bool>> = RefCell::new(HashMap::new());
}

/// The static globals of a build, in effect until dropped.
pub(crate) struct Producing(());

impl Drop for Producing {
    fn drop(&mut self) {
        STATIC.with(|s| s.borrow_mut().clear());
    }
}

/// Makes the globals with the ids of `globals` static, each with whether it
/// can be assigned, until the answer is dropped.
pub(crate) fn producing(globals: HashMap<i64, bool>) -> Producing {
    STATIC.with(|s| *s.borrow_mut() = globals);
    Producing(())
}

fn assigned(id: i64) -> Option<bool> {
    STATIC.with(|s| s.borrow().get(&id).copied())
}

/// The LLVM global variable's name.
pub(crate) fn symbol(id: usize) -> String {
    format!("tl_global${}", id)
}

/// The type a global of the field kind `kind` is kept in, if it can be kept
/// so.
pub(crate) fn kept_in(kind: i64) -> Option<BasicTypeEnum<'static>> {
    let carrier = Carrier::of_field_kind(kind).ok()?;
    // A byte: what is accessed atomically is whole bytes.
    if carrier == Carrier::Bool { Some(carriers::int(8).into()) } else { carrier.llvm() }
}

fn variable(m: &Module<'static>, id: i64, kind: i64) -> Result<(GlobalValue<'static>, BasicTypeEnum<'static>), String> {
    let ty = kept_in(kind).ok_or_else(|| format!("a static global of the field kind {}", kind))?;
    let name = symbol(id as usize);
    let g = m.get_global(&name).unwrap_or_else(|| {
        let g = m.add_global(ty, None, &name);
        g.set_linkage(Linkage::Internal);
        g.set_initializer(&raise_check::zero_of(ty));
        g
    });
    Ok((g, ty))
}

/// Keeps the access `i` of a value of type `ty` where it is written.
fn kept(i: InstructionValue<'static>, ty: BasicTypeEnum<'static>) -> Result<(), String> {
    i.set_volatile(true).map_err(|e| format!("{:?}", e))?;
    let bits = carriers::bits_of(ty);
    if carriers::word().get_bit_width() == 64 {
        i.set_alignment(bits / 8).map_err(|e| format!("{:?}", e))?;
        i.set_atomic_ordering(AtomicOrdering::Monotonic).map_err(|e| format!("{:?}", e))?;
    }
    Ok(())
}

fn id_word(id: i64) -> IntValue<'static> {
    carriers::word().const_int(id as u64, false)
}

fn byte() -> IntType<'static> {
    carriers::int(8)
}

/// `(global-get builder m id kind)` — global `id`'s value, whose storage is of
/// the field kind `kind`.
pub(crate) fn global_get(args: &[Value]) -> Result<Value, EvalError> {
    let (builder, module) = (expect_llvm_builder(&args[0])?, expect_llvm_module(&args[1])?);
    let (id, kind) = (expect_int(&args[2])?, expect_int(&args[3])?);
    let err = |e: String| EvalError::Internal(format!("global-get: {}", e));
    let Some(assigned) = assigned(id) else {
        let word = {
            let b = builder.borrow();
            let (f, word) = runtime_abi::call(&b, &module.borrow(), "rt_global_get", &[id_word(id).into()]).map_err(err)?;
            raise_check::after_call(&b, f).map_err(err)?;
            word
        };
        return untag_field(&[args[0], args[1], llvm_value_value(word), args[3]]);
    };
    let b = builder.borrow();
    let (g, ty) = variable(&module.borrow(), id, kind).map_err(err)?;
    let v = load(&b, g, ty, assigned).map_err(err)?;
    Ok(llvm_value_value(match Carrier::of_field_kind(kind).map_err(err)? {
        Carrier::Bool => b
            .build_int_compare(inkwell::IntPredicate::NE, v.into_int_value(), byte().const_zero(), "flag")
            .map_err(|e| err(e.to_string()))?
            .into(),
        _ => v,
    }))
}

fn load(b: &Builder<'static>, g: GlobalValue<'static>, ty: BasicTypeEnum<'static>, assigned: bool) -> Result<BasicValueEnum<'static>, String> {
    let v = b.build_load(ty, g.as_pointer_value(), "global").map_err(|e| e.to_string())?;
    if assigned {
        kept(v.as_instruction_value().ok_or("a load is an instruction")?, ty)?;
    }
    Ok(v)
}

/// Stores `v` in the static global `id`.
fn store(b: &Builder<'static>, m: &Module<'static>, id: i64, kind: i64, v: BasicValueEnum<'static>, keep: bool) -> Result<(), String> {
    let (g, ty) = variable(m, id, kind)?;
    let v = match Carrier::of_field_kind(kind)? {
        Carrier::Bool => b.build_int_z_extend(v.into_int_value(), byte(), "flag").map_err(|e| e.to_string())?.into(),
        carrier => carrier.coerce(b, v)?,
    };
    let i = b.build_store(g.as_pointer_value(), v).map_err(|e| e.to_string())?;
    if keep { kept(i, ty) } else { Ok(()) }
}

/// `(global-put builder m id kind v)` — assigns `v` to global `id`.
pub(crate) fn global_put(args: &[Value]) -> Result<Value, EvalError> {
    let (builder, module) = (expect_llvm_builder(&args[0])?, expect_llvm_module(&args[1])?);
    let (id, kind) = (expect_int(&args[2])?, expect_int(&args[3])?);
    let err = |e: String| EvalError::Internal(format!("global-put: {}", e));
    let v = value_or_return!(&args[4]);
    if assigned(id).is_some() {
        store(&builder.borrow(), &module.borrow(), id, kind, v, true).map_err(err)?;
        return Ok(llvm_value_value(v));
    }
    let word = tag_field(&[args[0], args[1], args[4], args[3]])?;
    let word = value_or_return!(&word);
    let b = builder.borrow();
    let (f, _) = runtime_abi::call(&b, &module.borrow(), "rt_global_set", &[id_word(id).into(), word]).map_err(err)?;
    raise_check::after_call(&b, f).map_err(err)?;
    Ok(llvm_value_value(v))
}

/// `(global-new builder m id kind v)` — makes global `id`, holding `v`. The
/// runtime numbers its globals in the order they are made, so `id` is the
/// one this is made as.
pub(crate) fn global_new(args: &[Value]) -> Result<Value, EvalError> {
    let (builder, module) = (expect_llvm_builder(&args[0])?, expect_llvm_module(&args[1])?);
    let (id, kind) = (expect_int(&args[2])?, expect_int(&args[3])?);
    let err = |e: String| EvalError::Internal(format!("global-new: {}", e));
    let v = value_or_return!(&args[4]);
    if assigned(id).is_some() {
        store(&builder.borrow(), &module.borrow(), id, kind, v, false).map_err(err)?;
        return Ok(llvm_value_value(Carrier::Unit.zero()));
    }
    let word = tag_field(&[args[0], args[1], args[4], args[3]])?;
    let word = value_or_return!(&word);
    let b = builder.borrow();
    let (f, answer) = runtime_abi::call(&b, &module.borrow(), "rt_global_new", &[word]).map_err(err)?;
    raise_check::after_call(&b, f).map_err(err)?;
    Ok(llvm_value_value(answer))
}
