//! Direct functions: compiled functions that cannot stop in the middle, built
//! as ordinary functions of their own signature rather than under the
//! coroutine ABI.
//!
//! `define i32 @f(i8, i8)`: the arguments arrive in registers at their types'
//! widths, the locals live on the machine stack, and a call to one is an
//! LLVM `call`. Only an executable built ahead of time has any: it is the one
//! place every caller of a function is known (`aot::direct_functions` decides
//! which), so nothing can come along later and call one under the coroutine
//! ABI. The JIT builds every function as a coroutine.
//!
//! Which functions are direct, and under what signature, is a table for the
//! build (here); the island asks nothing about it. The builders it already
//! calls to build a function's frame, its calls and its return
//! (`llvm_builtins::coroutine`) look the function up and build the direct
//! shape when it is in the table.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use inkwell::basic_block::BasicBlock;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, FunctionType};
use inkwell::values::{AsValueRef, FunctionValue};

use crate::check::repr::Repr;
use crate::compile::carriers::Carrier;

/// A direct function's signature: how each parameter and the result are
/// carried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirectSig {
    pub params: Vec<Carrier>,
    pub ret: Carrier,
}

impl DirectSig {
    pub(crate) fn of(params: &[Repr], ret: &Repr) -> DirectSig {
        DirectSig { params: params.iter().map(Carrier::of).collect(), ret: Carrier::of(ret) }
    }

    /// The LLVM function type. A `()` parameter is left out, and a `()`
    /// result is `void`.
    pub(crate) fn fn_type(&self) -> FunctionType<'static> {
        let params: Vec<BasicMetadataTypeEnum<'static>> = self.params.iter().filter_map(|c| c.llvm()).map(Into::into).collect();
        match self.ret.llvm() {
            Some(t) => t.fn_type(&params, false),
            None => crate::compile::llvm_context().void_type().fn_type(&params, false),
        }
    }

    /// Which LLVM parameter Lisp parameter `i` is, or `None` for a `()` one,
    /// which has none.
    pub(crate) fn llvm_index(&self, i: usize) -> Option<u32> {
        match self.params.get(i)? {
            Carrier::Unit => None,
            _ => Some(self.params[..i].iter().filter(|c| **c != Carrier::Unit).count() as u32),
        }
    }
}

thread_local! {
    /// The build's direct functions, by the symbol each is emitted under.
    /// Empty outside an ahead-of-time build.
    static DIRECT: RefCell<HashMap<String, DirectSig>> = RefCell::new(HashMap::new());

    /// The functions that exist under both ABIs: the symbol of the coroutine,
    /// which is what a call names, and the direct copy's. A prelude function
    /// is one: its coroutine is in the prelude's own bitcode.
    static COPIES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());

    /// Each direct function under construction, by its `LLVMValueRef`, and
    /// the block an unwind leaves it from — for the raise check after a
    /// call, which has the function and nothing else.
    static UNWIND_EXITS: RefCell<HashMap<usize, BasicBlock<'static>>> = RefCell::new(HashMap::new());

    /// How many loop safepoints direct functions were built without.
    static SKIPPED_SAFEPOINTS: Cell<usize> = const { Cell::new(0) };
}

/// The direct functions of a build, in effect until dropped.
pub(crate) struct Producing(());

impl Drop for Producing {
    fn drop(&mut self) {
        DIRECT.with(|d| d.borrow_mut().clear());
        COPIES.with(|c| c.borrow_mut().clear());
        UNWIND_EXITS.with(|u| u.borrow_mut().clear());
        SKIPPED_SAFEPOINTS.with(|s| s.set(0));
    }
}

/// Builds `functions` as direct functions, and calls each key of `copies` at
/// its value, until the answer is dropped.
pub(crate) fn producing(functions: HashMap<String, DirectSig>, copies: HashMap<String, String>) -> Producing {
    DIRECT.with(|d| *d.borrow_mut() = functions);
    COPIES.with(|c| *c.borrow_mut() = copies);
    SKIPPED_SAFEPOINTS.with(|s| s.set(0));
    Producing(())
}

/// The signature of `name` when it is a direct function.
pub(crate) fn sig_of(name: &str) -> Option<DirectSig> {
    DIRECT.with(|d| d.borrow().get(name).cloned())
}

/// The symbol and signature a call to `name` is made directly at, when there
/// is a direct function to call.
pub(crate) fn callee(name: &str) -> Option<(String, DirectSig)> {
    let symbol = COPIES.with(|c| c.borrow().get(name).cloned()).unwrap_or_else(|| name.to_string());
    let sig = sig_of(&symbol)?;
    Some((symbol, sig))
}

pub(crate) fn set_unwind_exit(f: FunctionValue<'static>, block: BasicBlock<'static>) {
    UNWIND_EXITS.with(|u| u.borrow_mut().insert(f.as_value_ref() as usize, block));
}

pub(crate) fn take_unwind_exit(f: FunctionValue<'static>) {
    UNWIND_EXITS.with(|u| u.borrow_mut().remove(&(f.as_value_ref() as usize)));
}

/// The block an unwind leaves `f` from, when `f` is a direct function under
/// construction.
pub(crate) fn unwind_exit(f: FunctionValue<'static>) -> Option<BasicBlock<'static>> {
    UNWIND_EXITS.with(|u| u.borrow().get(&(f.as_value_ref() as usize)).copied())
}

/// A direct function's loop went without its safepoint: right only in a
/// program that runs no other task.
pub(crate) fn skipped_safepoint() {
    SKIPPED_SAFEPOINTS.with(|s| s.set(s.get() + 1));
}

pub(crate) fn skipped_safepoints() -> usize {
    SKIPPED_SAFEPOINTS.with(|s| s.get())
}
