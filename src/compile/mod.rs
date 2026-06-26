//! Shared core for the LLVM-based `compile` (JIT) and `compile-file` (AOT)
//! paths. The compiler body itself (AST -> LLVM IR) is typelisp code loaded
//! the same way `prelude.rs` loads the standard library; this module only
//! provides the pieces that have to live in Rust: the LLVM context/locking,
//! and (in later phases) the AST bridge and runtime shims.

pub mod aot;
pub mod ast_bridge;
pub mod freevars;
pub mod runtime;

use std::sync::{Mutex, OnceLock};

use inkwell::context::Context;
use inkwell::execution_engine::JitFunction;
use inkwell::module::Module;
use inkwell::OptimizationLevel;

/// Serializes every LLVM-Context-touching operation. LLVM's C API is not
/// safe to call concurrently against one `Context` from multiple threads
/// (its type-uniquing tables aren't synchronized), so this is held for the
/// duration of any compile even though nothing here spawns compiler threads
/// today — cheap insurance against a known class of non-deterministic crash.
pub static COMPILE_LOCK: Mutex<()> = Mutex::new(());

/// `Context` holds a raw `LLVMContextRef`, so it isn't `Sync` and can't sit
/// in a `static` directly. Wrapping it asserts that's fine: every access goes
/// through [`llvm_context`], and every LLVM-Context-touching call site is
/// expected to hold [`COMPILE_LOCK`] first, so two threads never actually
/// touch the inner `Context` concurrently.
struct ContextCell(Context);
unsafe impl Sync for ContextCell {}

static LLVM_CONTEXT: OnceLock<ContextCell> = OnceLock::new();

/// The single process-wide LLVM `Context`. Modules/builders/values created
/// from it borrow it for the life of the process (no per-compile teardown),
/// which is what lets [`crate::eval::value::RtValue`]'s LLVM variants hold
/// `'static` inkwell types instead of threading a lifetime through `RtValue`.
pub fn llvm_context() -> &'static Context {
    &LLVM_CONTEXT.get_or_init(|| ContextCell(Context::create())).0
}

/// The fixed C ABI every JIT-compiled function uses, regardless of its
/// typelisp-level arity: an `i64` argument array (and its length) in, one
/// `i64` out — see `registry::llvm_module_def`'s doc comment for why
/// `compiler.rs`'s `compile-function` always builds LLVM functions under
/// this exact signature.
pub type CompiledSignature = unsafe extern "C" fn(*const i64, u32) -> i64;

/// A JIT-compiled function. Holding the `JitFunction` is enough to keep the
/// code (and its owning `ExecutionEngine`) alive — see
/// [`inkwell::execution_engine::JitFunction`]'s doc comment, it carries its
/// engine along internally.
pub struct CompiledFn {
    f: JitFunction<'static, CompiledSignature>,
    /// This function's own JIT-resolved address — see [`Self::address`].
    addr: usize,
}

impl CompiledFn {
    /// JIT-compiles `fn_name` out of `module`. `externals` (labels/closures
    /// Stage 3) is `(name, address)` for every *other* already-`compile`d
    /// top-level function `fn_name`'s body calls (`Expr::Call`): each must
    /// already be forward-declared, with no body, in `module` under that
    /// same name — see [`crate::eval::interp::Interp::compile_function`]'s
    /// doc comment for why that declaration has to exist *before* the
    /// typelisp compiler body ever runs (`compile-call`'s `get-function`
    /// needs to find *something* by that name). Wiring each one's real
    /// address via `add_global_mapping` here, before resolving `fn_name`
    /// itself, makes a call through that declaration jump straight to the
    /// real, already-running JIT code instead of an unresolved symbol. Empty
    /// for self-recursion only or no calls at all — the common case, and
    /// every call before Stage 3. Must be called with [`COMPILE_LOCK`]
    /// held.
    pub fn new(module: &Module<'static>, fn_name: &str, externals: &[(String, usize)]) -> Result<CompiledFn, String> {
        let engine = module.create_jit_execution_engine(OptimizationLevel::None).map_err(|e| e.to_string())?;
        for (name, addr) in externals {
            let decl = module
                .get_function(name)
                .ok_or_else(|| format!("internal error: no forward declaration for \"{}\" in this module", name))?;
            engine.add_global_mapping(&decl, *addr);
        }
        let f = unsafe { engine.get_function::<CompiledSignature>(fn_name).map_err(|e| e.to_string())? };
        let addr = engine.get_function_address(fn_name).map_err(|e| e.to_string())?;
        Ok(CompiledFn { f, addr })
    }

    pub fn call(&self, args: &[i64]) -> i64 {
        unsafe { self.f.call(args.as_ptr(), args.len() as u32) }
    }

    /// This function's own JIT-resolved address — used to wire
    /// `add_global_mapping` when a *later* `compile`d function's body calls
    /// this one (labels/closures Stage 3, see [`Self::new`]'s `externals`
    /// parameter).
    pub fn address(&self) -> usize {
        self.addr
    }
}

#[cfg(test)]
mod tests {
    use inkwell::AddressSpace;

    use super::{llvm_context, CompiledFn, COMPILE_LOCK};
    use crate::compile::runtime::rt_ping;

    /// Stage 0's JIT-side half of the proof that a `#[no_mangle]` Rust
    /// function from [`crate::compile::runtime`] is callable through the
    /// exact same `externals`/`add_global_mapping` wiring labels/closures
    /// Stage 3 already built for calling another JIT-compiled typelisp
    /// function by name — `rt_ping`'s real address is just another `usize`
    /// to map, indistinguishable to this machinery from a previously-JIT'd
    /// function's address. See `aot_output_can_call_an_rt_extern_function`
    /// (`aot.rs`) for the AOT-side counterpart.
    #[test]
    fn jit_can_call_an_rt_extern_function() {
        let ctx = llvm_context();
        let _guard = COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("jit_ping_test");

        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let rt_ping_decl = module.add_function("rt_ping", fn_ty, None);
        let caller = module.add_function("jit_ping_test", fn_ty, None);

        let builder = ctx.create_builder();
        let entry = ctx.append_basic_block(caller, "entry");
        builder.position_at_end(entry);
        let one_slot = builder.build_alloca(ctx.i64_type(), "one_slot").unwrap();
        builder.build_store(one_slot, ctx.i64_type().const_int(41, false)).unwrap();
        let argc_one = ctx.i32_type().const_int(1, false);
        let call = builder.build_call(rt_ping_decl, &[one_slot.into(), argc_one.into()], "rt_ping_result").unwrap();
        let result = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => panic!("rt_ping call produced no value"),
        };
        builder.build_return(Some(&result)).unwrap();
        module.verify().expect("module failed verification");

        let externals = vec![("rt_ping".to_string(), rt_ping as usize)];
        let compiled = CompiledFn::new(&module, "jit_ping_test", &externals).expect("CompiledFn::new failed");
        assert_eq!(compiled.call(&[]), 42);
    }
}
