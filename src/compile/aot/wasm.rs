//! The WebAssembly targets: emitting the module as a WebAssembly object and
//! linking it with `wasm-ld` — into a WASI command module for
//! `wasm32-wasip1`, and for `wasm32-unknown-unknown` into a module whose host
//! is JavaScript, written out beside it (`wasm_loader.mjs`).
//!
//! The module is the one a native executable is made from. Nothing in it
//! depends on the width of a pointer: a word is an `i64` everywhere, and a
//! pointer becomes one only through `ptrtoint`/`inttoptr`, which on a 32-bit
//! target widen and narrow. So the prelude's committed bitcode serves these
//! targets too — it carries no data layout of its own — and the module is
//! given the target's layout here, before the passes run.

use std::fs;
use std::process::Command;

use inkwell::llvm_sys::core::{
    LLVMAddGlobal, LLVMConstInt, LLVMConstStructInContext, LLVMDeleteGlobal, LLVMGetAlignment, LLVMGetConstOpcode,
    LLVMGetFirstGlobal, LLVMGetInitializer, LLVMGetLinkage, LLVMGetNextGlobal, LLVMGetNumOperands, LLVMGetOperand,
    LLVMGetUnnamedAddress, LLVMGetValueName2, LLVMInt32TypeInContext, LLVMIsAConstantArray, LLVMIsAConstantExpr,
    LLVMIsAConstantInt, LLVMIsGlobalConstant, LLVMReplaceAllUsesWith, LLVMSetAlignment, LLVMSetGlobalConstant,
    LLVMSetInitializer, LLVMSetLinkage, LLVMSetUnnamedAddress, LLVMSetValueName2, LLVMTypeOf,
};
use inkwell::llvm_sys::prelude::LLVMValueRef;
use inkwell::llvm_sys::LLVMOpcode;
use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple};

use super::{wasm_linker, wasm_runtime_dir, ENTRY_POINT_NAME, STATICLIB_NAME};
use crate::CompileTarget;

/// What WASI's start file (`crt1-command.o`) calls: `main` taking no
/// arguments, under the name C's `int main(void)` is given on this target.
/// The runtime reads its arguments from WASI itself, not from `main`.
const WASI_MAIN: &str = "__main_void";

/// What a `wasm32-unknown-unknown` module exports for its loader to call:
/// there is no start file, and the answer is the exit code.
const JS_MAIN: &str = "typelisp_main";

/// The JavaScript written beside a `wasm32-unknown-unknown` module, with
/// [`LOADER_MODULE_NAME`] standing for the module's file name.
const LOADER: &str = include_str!("wasm_loader.mjs");
const LOADER_MODULE_NAME: &str = "__TYPELISP_MODULE__";

/// The stack a native executable's main thread gets, so that recursion runs
/// as deep on WebAssembly as it does natively.
const STACK_SIZE: u32 = 8 * 1024 * 1024;

fn target_machine(llvm_triple: &str) -> Result<TargetMachine, String> {
    Target::initialize_webassembly(&InitializationConfig::default());
    let triple = TargetTriple::create(llvm_triple);
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    target
        .create_target_machine(
            &triple,
            "generic",
            "",
            crate::compile::optimize::codegen_level(),
            RelocMode::Static,
            CodeModel::Default,
        )
        .ok_or_else(|| format!("failed to create a target machine for {}", llvm_triple))
}

/// Emits `module` as a WebAssembly object and links it, with `target`'s
/// runtime, into a module at `output_path` — and for
/// `wasm32-unknown-unknown`, the loader beside it (`output_path` with
/// `.mjs`). Must be called with [`crate::compile::COMPILE_LOCK`] held.
pub(super) fn write_module(
    module: &Module<'static>,
    output_path: &str,
    items: &[CompileTarget],
    target: super::Target,
) -> Result<(), String> {
    let (llvm_triple, entry_name) = match target {
        super::Target::Wasip1 => ("wasm32-unknown-wasip1", WASI_MAIN),
        super::Target::Wasm32Unknown => ("wasm32-unknown-unknown", JS_MAIN),
        super::Target::Host | super::Target::Thumbv7em | super::Target::Thumbv8mMain => {
            return Err(format!("internal error: write_module called for {}", target.name()))
        }
    };
    let loader = match target {
        super::Target::Wasm32Unknown => Some(loader_path(output_path)?),
        _ => None,
    };
    let runtime = wasm_runtime_dir(target.name())?;
    let linker = wasm_linker(&runtime)?;
    let tm = target_machine(llvm_triple)?;
    module.set_triple(&tm.get_triple());
    module.set_data_layout(&tm.get_target_data().get_data_layout());
    module
        .get_function(ENTRY_POINT_NAME)
        .ok_or("internal error: the module has no `main`")?
        .as_global_value()
        .set_name(entry_name);
    split_address_words(module)?;
    crate::compile::codegen_prep::prepare(module)?;
    crate::compile::optimize::run_llvm_passes(module, &tm)?;

    let object_path = format!("{}.o", output_path);
    tm.write_to_file(module, FileType::Object, std::path::Path::new(&object_path))
        .map_err(|e| format!("failed to emit object file: {}", e))?;
    for warning in crate::compile::codegen_prep::take_frame_warnings(items) {
        eprintln!("{}", warning);
    }

    let mut link = Command::new(&linker);
    link.args(["-flavor", "wasm"]);
    match target {
        super::Target::Wasip1 => {
            link.arg(runtime.join("crt1-command.o"))
                .arg(&object_path)
                .arg(runtime.join(STATICLIB_NAME))
                .arg(runtime.join("libc.a"));
        }
        _ => {
            link.arg("--no-entry")
                .arg(format!("--export={}", JS_MAIN))
                .arg(&object_path)
                .arg(runtime.join(STATICLIB_NAME));
        }
    }
    let status = link
        .arg("-z")
        .arg(format!("stack-size={}", STACK_SIZE))
        .arg("-o")
        .arg(output_path)
        .status()
        .map_err(|e| format!("failed to invoke {}: {}", linker.display(), e));
    let _ = fs::remove_file(&object_path);
    let status = status?;
    if !status.success() {
        return Err(format!("wasm-ld failed with status {}", status));
    }
    if let Some((loader, name)) = loader {
        let text = LOADER.replace(LOADER_MODULE_NAME, &name.replace('\\', "\\\\").replace('\'', "\\'"));
        fs::write(&loader, text).map_err(|e| format!("failed to write {}: {}", loader.display(), e))?;
    }
    Ok(())
}

/// Where the loader for the module at `output_path` goes — `output_path` with
/// `.mjs`, so the two are found side by side — and the module's file name, by
/// which the loader finds it.
fn loader_path(output_path: &str) -> Result<(std::path::PathBuf, String), String> {
    let module = std::path::Path::new(output_path);
    let name = module
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{} has no file name to give the loader", output_path))?;
    let loader = module.with_extension("mjs");
    if loader == module {
        return Err(format!("{} ends in .mjs, which is the loader's name; name the module .wasm", output_path));
    }
    Ok((loader, name.to_string()))
}

/// Rewrites every table of words whose initialiser holds an address
/// (`ptrtoint (ptr @g to i64)`) so that the address is a pointer of its own.
///
/// A word is 64 bits and an address on a 32-bit target (WebAssembly, a
/// Cortex-M) 32, and the object format has no relocation that writes a 32-bit address into a 64-bit slot — the
/// backend refuses such an initialiser. Each such word becomes
/// `<{ ptr @g, i32 0 }>` in a packed struct standing in for the array: the
/// same eight bytes, little-endian, as the address widened to a word, so
/// what reads the table reads the same words. The tables are the type and
/// shape tables and the frame layouts' (`compile::type_table`); anything
/// else in such an initialiser is not something this knows how to lay out.
pub(super) fn split_address_words(module: &Module<'static>) -> Result<(), String> {
    let ctx = crate::compile::llvm_context();
    let ctx_ref = ctx.raw();
    let module_ref = module.as_mut_ptr();
    let mut globals = Vec::new();
    let mut g = unsafe { LLVMGetFirstGlobal(module_ref) };
    while !g.is_null() {
        globals.push(g);
        g = unsafe { LLVMGetNextGlobal(g) };
    }
    for g in globals {
        // SAFETY: every value below is a live constant or global of `module`.
        unsafe {
            let init = LLVMGetInitializer(g);
            if init.is_null() || LLVMIsAConstantArray(init).is_null() {
                continue;
            }
            let n = LLVMGetNumOperands(init) as u32;
            let elements: Vec<LLVMValueRef> = (0..n).map(|k| LLVMGetOperand(init, k)).collect();
            if !elements.iter().any(|e| !LLVMIsAConstantExpr(*e).is_null()) {
                continue;
            }
            let i32t = LLVMInt32TypeInContext(ctx_ref);
            let mut fields = Vec::with_capacity(elements.len() * 2);
            for e in elements {
                if !LLVMIsAConstantInt(e).is_null() {
                    fields.push(e);
                } else if !LLVMIsAConstantExpr(e).is_null() && LLVMGetConstOpcode(e) == LLVMOpcode::LLVMPtrToInt {
                    fields.push(LLVMGetOperand(e, 0));
                    fields.push(LLVMConstInt(i32t, 0, 0));
                } else {
                    return Err(format!(
                        "internal error: the initialiser of {} holds a word that is neither a number nor an address",
                        value_name(g)
                    ));
                }
            }
            let packed = LLVMConstStructInContext(ctx_ref, fields.as_mut_ptr(), fields.len() as u32, 1);
            let name = value_name(g);
            let replacement = LLVMAddGlobal(module_ref, LLVMTypeOf(packed), c"".as_ptr());
            LLVMSetInitializer(replacement, packed);
            LLVMSetLinkage(replacement, LLVMGetLinkage(g));
            LLVMSetGlobalConstant(replacement, LLVMIsGlobalConstant(g));
            LLVMSetUnnamedAddress(replacement, LLVMGetUnnamedAddress(g));
            LLVMSetAlignment(replacement, LLVMGetAlignment(g).max(8));
            LLVMReplaceAllUsesWith(g, replacement);
            LLVMDeleteGlobal(g);
            LLVMSetValueName2(replacement, name.as_ptr() as *const _, name.len());
        }
    }
    Ok(())
}

fn value_name(v: LLVMValueRef) -> String {
    let mut len = 0;
    // SAFETY: `v` is a live value; the name is copied before anything renames it.
    let ptr = unsafe { LLVMGetValueName2(v, &mut len) };
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    String::from_utf8_lossy(bytes).into_owned()
}
