//! Shared core for the LLVM-based `compile` (JIT) and `compile-file` (AOT)
//! paths. The compiler body itself (AST -> LLVM IR) is typelisp code loaded
//! the same way `prelude.rs` loads the standard library; this module only
//! provides the pieces that have to live in Rust: the LLVM context/locking,
//! and (in later phases) the AST bridge and runtime shims.

pub mod aot;
pub mod ast_bridge;
pub mod freevars;

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
}

impl CompiledFn {
    /// JIT-compiles `fn_name` out of `module`. Must be called with
    /// [`COMPILE_LOCK`] held — see that constant's doc comment.
    pub fn new(module: &Module<'static>, fn_name: &str) -> Result<CompiledFn, String> {
        let engine = module.create_jit_execution_engine(OptimizationLevel::None).map_err(|e| e.to_string())?;
        let f = unsafe { engine.get_function::<CompiledSignature>(fn_name).map_err(|e| e.to_string())? };
        Ok(CompiledFn { f })
    }

    pub fn call(&self, args: &[i64]) -> i64 {
        unsafe { self.f.call(args.as_ptr(), args.len() as u32) }
    }
}
