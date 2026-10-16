//! LLVM's half of optimization: what the recorded level
//! ([`crate::opt_level`]) means once the island has finished a module.
//!
//! The island does the simple rewrites itself, on the IR it is handed
//! (`optimize-ir` in `compiler.rs`); everything past those is LLVM's — its
//! `default<ON>` pass pipeline over the module, and code generation at the
//! same level. At `0` neither runs, and the machine code is what the island
//! emitted.
//!
//! Every path to machine code asks here, after
//! [`crate::compile::codegen_prep::prepare`]: the JIT, `compile-file`, and
//! `disassemble`'s assembly — so what `disassemble` shows is what the other
//! two run.

use inkwell::module::Module;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::TargetMachine;
use inkwell::OptimizationLevel;

/// The code generator's level for the recorded one.
pub(crate) fn codegen_level() -> OptimizationLevel {
    level_of(crate::opt_level::opt_level())
}

pub(crate) fn level_of(level: u8) -> OptimizationLevel {
    match level {
        0 => OptimizationLevel::None,
        1 => OptimizationLevel::Less,
        2 => OptimizationLevel::Default,
        3 => OptimizationLevel::Aggressive,
        _ => unreachable!("optimization level {} is past {}", level, crate::opt_level::MAX),
    }
}

/// Runs LLVM's pass pipeline for the recorded level over `module`, for
/// `machine` to generate code from. Nothing at `0`.
///
/// The module is verified afterwards as [`prepare`](crate::compile::codegen_prep::prepare)
/// verifies it: a pass that broke it is a bug, and code generation from a
/// broken module fails far from the cause.
pub(crate) fn run_llvm_passes(module: &Module<'static>, machine: &TargetMachine) -> Result<(), String> {
    let level = crate::opt_level::opt_level();
    if level == 0 {
        return Ok(());
    }
    module
        .run_passes(&format!("default<O{}>", level), machine, PassBuilderOptions::create())
        .map_err(|e| format!("internal error: LLVM's -O{} pipeline failed: {}", level, e))?;
    module
        .verify()
        .map_err(|e| format!("internal error: the module failed verification after optimizing it at -O{}: {}", level, e))
}
