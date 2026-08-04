//! interp-closure removal Stage 3: producing the committed, precompiled
//! compiler-island artifact that Stage 4's `compiler::load_aot` loads at
//! runtime *instead of* interpreting the island's bodies.
//!
//! [`build_island_bitcode`] compiles every island `defun` (`compiler.rs`'s
//! `SOURCE`) into one shared LLVM module and serializes it to bitcode. The
//! bootstrap binary (`src/bin/bootstrap_island.rs`) writes those bytes to
//! `src/compiler_island.bc`, which is committed and `include_bytes!`'d by
//! `load_aot`.
//!
//! Only the *native bodies* are committed — no checker/interpreter state.
//! `load_aot` rebuilds that by re-checking `SOURCE` (registering each
//! `defun`'s `FnDef`, which allocates no closures — an island body is never
//! *called* interpreted, only compiled); the bitcode supplies the compiled
//! function bodies that make those calls native.
//! A [`SOURCE_HASH_GLOBAL`] i64 global carrying [`crate::fasl::source_hash`]
//! of `SOURCE` is embedded in the module so `load_aot` and the
//! `island_artifacts_are_fresh` test can detect a `.bc` gone stale relative
//! to `SOURCE` — the same staleness check `prelude`'s fasl cache uses, with
//! no separate sidecar file.
//!
//! Building this installs the *committed* (previous) `.bc` first and drives
//! its **native** `compile-function` to emit each defun's IR — the snapshot
//! chain (interp-closure removal Stage 8b), so regeneration no longer depends
//! on interpreted closures and they can be deleted entirely (Stage 8c). The
//! very first `.bc` was built by the interpreted island (before this chain
//! existed); every one since is recompiled by its predecessor. See
//! [`build_island_bitcode`]'s own `install_island_bitcode(..., false)` call.

use std::cell::RefCell;
use std::rc::Rc;

use inkwell::module::Module;
use inkwell::AddressSpace;

use crate::fasl::source_hash;
use crate::{Checker, Heap, Interp, Reader, TopLevel};

/// The name of the i64 global the island bitcode carries its source hash in.
/// Read back by [`read_embedded_source_hash`].
pub const SOURCE_HASH_GLOBAL: &str = "__typelisp_island_source_hash";

/// Builds the compiler island's AOT bitcode in a throwaway environment.
///
/// The module mirrors [`crate::compile::aot::compile_file`]'s shared module
/// (rt_* forward declarations + one `add_compiled_function` per defun, each
/// defun's `labels` siblings emitted alongside it by the island's own
/// `compile-labels`) minus the `main` wrapper — the island is a library of
/// compiled functions, not an executable. No `add_global_mapping` is done
/// here: the rt_* addresses are supplied at load time by `load_aot`'s
/// `CompiledFn::new_multi`, and the bitcode only needs to carry the
/// declarations (which it does).
pub fn build_island_bitcode() -> Result<Vec<u8>, String> {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();

    crate::load_prelude(&mut heap, &mut chk, &mut interp);

    let reader = Reader::new();
    let forms = reader
        .read_all(&mut heap, crate::compiler::SOURCE)
        .map_err(|e| format!("island read failed: {}", e))?;
    chk.predeclare_program(&mut heap, &forms);
    let mut fn_names: Vec<String> = Vec::new();
    for v in forms {
        let tl = chk.check_form(&mut heap, &interp, v).map_err(|e| format!("island check failed: {}", e))?;
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        if let TopLevel::Defun { name, .. } = &tl {
            fn_names.push(name.last_segment().to_string());
        }
        interp.exec(&mut heap, tl).map_err(|e| format!("island exec failed: {}", e))?;
    }

    // Install the *committed* island bitcode so the compile loop below drives
    // the previous build's **native** `compile-function`, not an interpreted
    // one (the snapshot chain — interp-closure removal Stage 8b). `check_hash`
    // is `false`: the committed `.bc` is by construction one generation behind
    // the `SOURCE` we're recompiling, so a hash mismatch is expected. Any
    // island `defun` added since that `.bc` is simply absent from it and gets
    // compiled fresh by the just-installed native `compile-function`. This is
    // what lets interpreted closures be deleted (Stage 8c): regenerating the
    // island no longer needs the interpreter to tree-walk `compile-function`'s
    // own `labels`/`lambda`s. (The very first `.bc`, before this chain existed,
    // was built by the interpreted island; every one since is built by its
    // predecessor.)
    interp
        .install_island_bitcode(crate::compiler::ISLAND_BITCODE, &fn_names, false)
        .map_err(|e| format!("island bootstrap install of the committed .bc failed: {}", e))?;

    let ctx = crate::compile::llvm_context();
    let module = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("compiler_island");
        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        for (name, _) in crate::eval::interp::rt_extern_functions() {
            module.add_function(name, fn_ty, None);
        }
        // Embed the source hash as an i64 global with a constant initializer
        // (internal linkage — it's read by value from the parsed module, never
        // linked against).
        let i64_ty = ctx.i64_type();
        let hash_global = module.add_global(i64_ty, None, SOURCE_HASH_GLOBAL);
        hash_global.set_initializer(&i64_ty.const_int(source_hash(crate::compiler::SOURCE), false));
        hash_global.set_constant(true);
        // Forward-declare *every* island function before compiling any body.
        // The island's own `compile-call` resolves a call target with
        // `(get-function m "tl_<callee>")`, which fails outright if the callee
        // has no declaration yet — so compiling one at a time only works while
        // the island's call graph happens to be a DAG in declaration order.
        // It is not: since top-level `defun`s may reference each other freely
        // (`Checker::predeclare_program`) the island is written as ~60 mutually
        // recursive top-level functions rather than one `labels` block, and
        // `compile-value` calls helpers declared below it. `add-function`
        // reuses an existing declaration rather than adding a second one
        // (`llvm_module_add_function`), so a body compiled later simply fills
        // in the shell declared here. This is the same shape `Interp::
        // compile_scc` already uses for a JIT'd cycle.
        for name in &fn_names {
            let sym = crate::compile::ast_bridge::user_symbol_name(name);
            if module.get_function(&sym).is_none() {
                module.add_function(&sym, fn_ty, None);
            }
        }
        Rc::new(RefCell::new(module))
    };
    // `add_compiled_function` locks `COMPILE_LOCK` per LLVM builtin call
    // (`eval_llvm_builtin_method`) and `Mutex` isn't reentrant, so it must
    // run without the lock held — same constraint `aot::compile_file`
    // documents at its own loop.
    for name in &fn_names {
        let internal_name = crate::compile::ast_bridge::user_symbol_name(name);
        interp
            .add_compiled_function(&mut heap, module.clone(), name, &internal_name)
            .map_err(|e| format!("island compile of `{}` failed: {}", name, e))?;
    }

    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    let module = module.borrow();
    module.verify().map_err(|e| format!("island module failed verification: {}", e))?;
    Ok(module.write_bitcode_to_memory().as_slice().to_vec())
}

/// Reads the [`SOURCE_HASH_GLOBAL`] i64 constant back out of an
/// already-parsed island `module` — the staleness key `load_aot` and the
/// freshness test compare against [`source_hash`]`(SOURCE)`. `None` if the
/// global is absent or not a constant integer (a `.bc` from before this
/// global existed, or a corrupt one).
pub fn read_embedded_source_hash(module: &Module<'static>) -> Option<u64> {
    let global = module.get_global(SOURCE_HASH_GLOBAL)?;
    global.get_initializer()?.into_int_value().get_zero_extended_constant()
}
