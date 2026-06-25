//! `compile-file`: the AOT exit. Reads an independent typelisp source file
//! (its own fresh `Heap`/`Checker`/`Interp` — deliberately *not* the
//! caller's, since the whole point of AOT output is a self-contained
//! executable that doesn't depend on whatever else happens to be loaded in
//! the process that requested it), compiles every top-level `defun` into
//! one shared LLVM module (reusing
//! [`crate::eval::interp::Interp::add_compiled_function`], the exact same
//! per-function compile step the JIT path (`Interp::compile_function`)
//! uses — see that method's doc comment for why the module is built once,
//! up front, in this file rather than per function), and links the result
//! into a native executable via the system `cc`.
//!
//! Scope matches [`crate::compile::ast_bridge`]'s current translation
//! coverage: every `defun` in the file must be non-generic with a
//! single-expression body using only literals/vars/`+`/`-`/`*`,
//! `labels`-sibling/self calls, and top-level `defun`-to-`defun` calls
//! including self-recursion (`Expr::Call`, labels/closures Stage 3) — a
//! callee must already be defined earlier in the file, the same forward-
//! reference restriction `Checker::resolve_fn` enforces at type-checking
//! time regardless of AOT/JIT (see `compile::CompiledFn::new`'s doc comment
//! for the JIT-side counterpart of this same restriction). The loop below
//! needs no special handling for that: by the time a later `defun` in the
//! file is compiled, every earlier one already has its real body — not just
//! a declaration — in this same shared module, so `compile-call`'s
//! `get-function` always finds it. The file must contain a zero-parameter
//! `main` defun — the entry point — under whatever integer return type its
//! body's arithmetic happens to check as (see
//! `compiles_and_runs_arithmetic_in_main` in `tests/compile_file_test.rs`
//! for why that's `i32` more often than not).
//!
//! ## The `main` name problem
//!
//! Every compiled function uses the fixed ABI `i64 fn(i64* args, i32
//! argc)` (see `crate::compile::CompiledSignature`'s doc comment) — but the
//! C runtime's startup code calls the executable's `main` expecting
//! roughly `int main(void)` (Phase 2 doesn't thread argc/argv through to
//! typelisp). Those two signatures aren't link-compatible, so the file's
//! `main` defun is compiled under the internal name `tl_main` instead, and
//! [`build_main_wrapper`] adds a separate, hand-built LLVM function
//! actually named `main` that just calls `tl_main` and truncates its `i64`
//! result to the `i32` process exit code.

use std::cell::RefCell;
use std::fs;
use std::process::Command;
use std::rc::Rc;

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::values::CallSiteValue;
use inkwell::AddressSpace;
use inkwell::OptimizationLevel;

use crate::{Checker, Heap, Interp, Reader, TopLevel};

const ENTRY_POINT_NAME: &str = "main";
const ENTRY_POINT_INTERNAL_NAME: &str = "tl_main";

/// Reads `source_path`, compiles every `defun` in it, and links a native
/// executable at `output_path`. See the module doc comment for scope.
pub fn compile_file(source_path: &str, output_path: &str) -> Result<(), String> {
    let source =
        fs::read_to_string(source_path).map_err(|e| format!("failed to read \"{}\": {}", source_path, e))?;

    let mut heap = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    crate::load_compiler(&mut heap, &mut chk, &mut interp);

    let reader = Reader::new();
    let forms = reader.read_all(&mut heap, &source).map_err(|e| e.to_string())?;

    // Every top-level form in an AOT source file must be a `defun` (see the
    // module doc comment's scope note) — collected in declaration order so
    // later steps know exactly which `interp.fns` entries are this file's,
    // as opposed to `load_compiler`'s own helper `defun`s sharing the same
    // table.
    let mut fn_names: Vec<String> = Vec::new();
    for v in forms {
        let tl = chk.check_form(&mut heap, &interp, v).map_err(|e| e.to_string())?;
        let name = match &tl {
            TopLevel::Defun { name, .. } => name.local().to_string(),
            other => return Err(format!("compile-file only supports top-level `defun`, found {:?}", other)),
        };
        interp.exec(&mut heap, tl).map_err(|e| e.to_string())?;
        fn_names.push(name);
    }

    if !fn_names.iter().any(|n| n == ENTRY_POINT_NAME) {
        return Err(format!(
            "no zero-argument `{}` defun found (required as the entry point)",
            ENTRY_POINT_NAME
        ));
    }

    let ctx = crate::compile::llvm_context();
    let module = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        Rc::new(RefCell::new(ctx.create_module("compiled_file")))
    };

    // One shared module, one `add_compiled_function` call per `defun` —
    // *not* per-function modules merged afterward, see this module's doc
    // comment for why. `add_compiled_function` internally locks
    // `COMPILE_LOCK` per LLVM builtin call (via `eval_llvm_builtin_method`),
    // and `Mutex` isn't reentrant, so it must be called without the lock
    // already held — unlike the rest of this function from here on, which
    // touches the LLVM Context family of APIs directly with no calls back
    // into `Interp`/the typelisp compiler body.
    for name in &fn_names {
        let internal_name = if name == ENTRY_POINT_NAME { ENTRY_POINT_INTERNAL_NAME } else { name.as_str() };
        interp.add_compiled_function(&mut heap, module.clone(), name, internal_name).map_err(|e| e.to_string())?;
    }

    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    let module = module.borrow();
    build_main_wrapper(ctx, &module)?;
    module.verify().map_err(|e| format!("module failed verification: {}", e))?;
    write_executable(&module, output_path)
}

/// Adds a real, C-ABI `main` to `module` that calls the compiled entry
/// point (`tl_main`, see [`ENTRY_POINT_INTERNAL_NAME`]) with no logical
/// arguments and returns its `i64` result truncated to an `i32` exit code.
/// See the module doc comment for why this can't just compile the file's
/// `main` defun under that name directly.
fn build_main_wrapper(ctx: &'static Context, module: &Module<'static>) -> Result<(), String> {
    let tl_main = module
        .get_function(ENTRY_POINT_INTERNAL_NAME)
        .ok_or_else(|| "internal error: compiled entry point not found in module".to_string())?;

    let i32_type = ctx.i32_type();
    let main_fn = module.add_function(ENTRY_POINT_NAME, i32_type.fn_type(&[], false), None);
    let entry_block = ctx.append_basic_block(main_fn, "entry");
    let builder = ctx.create_builder();
    builder.position_at_end(entry_block);

    let null_args = ctx.ptr_type(AddressSpace::default()).const_null();
    let argc_zero = ctx.i32_type().const_int(0, false);
    let call: CallSiteValue = builder
        .build_call(tl_main, &[null_args.into(), argc_zero.into()], "tl_main_result")
        .map_err(|e| format!("failed to build entry-point call: {}", e))?;
    let result = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => {
            return Err("internal error: entry point produced no value".to_string())
        }
    };
    let exit_code = builder
        .build_int_truncate(result, i32_type, "exit_code")
        .map_err(|e| format!("failed to truncate exit code: {}", e))?;
    builder.build_return(Some(&exit_code)).map_err(|e| format!("failed to build entry-point return: {}", e))?;
    Ok(())
}

/// Emits `module` to an object file and links it into a native executable
/// at `output_path` via the system `cc`. Must be called with
/// [`crate::compile::COMPILE_LOCK`] held.
fn write_executable(module: &Module<'static>, output_path: &str) -> Result<(), String> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("failed to initialize native target: {}", e))?;

    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let target_machine = target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            OptimizationLevel::None,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create a target machine for the host triple".to_string())?;

    // Named from `output_path` (not e.g. the process id) so it can't
    // collide across concurrently-running `cargo test` threads in the same
    // process — every caller already needs a distinct `output_path` of its
    // own, for the same reason.
    let object_path = format!("{}.o", output_path);
    target_machine
        .write_to_file(module, FileType::Object, std::path::Path::new(&object_path))
        .map_err(|e| format!("failed to emit object file: {}", e))?;

    let status = Command::new("cc")
        .arg(&object_path)
        .arg("-o")
        .arg(output_path)
        .status()
        .map_err(|e| format!("failed to invoke the system linker (`cc`): {}", e));
    let _ = fs::remove_file(&object_path);
    let status = status?;

    if !status.success() {
        return Err(format!("linker failed with status {}", status));
    }
    Ok(())
}
