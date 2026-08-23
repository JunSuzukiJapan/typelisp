//! Loading a dump: applying a unit's checked state and then installing the
//! native bodies its bitcode section holds.
//!
//! The container itself — the header, the directory, the byte layout — lives in
//! the front end too ([`typelisp_front::dump::write`]/`parse`), because an AOT
//! executable has to read one and links only that crate. What is left here is
//! the half that needs LLVM: turning a unit's bitcode section into installed
//! native bodies.

use typelisp_front::dump::{apply_types, UnitItem, UnitState};


use crate::compile::symbols::CompiledItem;
use crate::{Checker, Heap, Interp};

/// Reads one unit's checked state, without applying it.
///
/// Split out from [`load_unit`] because a caller with a source to check
/// against (`prelude_bootstrap::load` and the island's) has to see
/// `source_digest` *before* anything is applied.
pub fn read_types(unit: &typelisp_front::dump::UnitRef, label: &str) -> Result<UnitState, String> {
    typelisp_front::dump::read_state(unit.types, label)
}

/// Applies one unit: its checker state and definitions, then the native bodies
/// its bitcode holds.
///
/// `chk`/`interp` must be the pair every previously applied unit went into —
/// a unit records what its own compilation added, so applying one out of order
/// leaves it referring to names that are not there yet.
pub fn load_unit(
    heap: &mut Heap,
    chk: &mut Checker,
    interp: &mut Interp,
    state: UnitState,
    bitcode: &[u8],
) -> Result<(), String> {
    load_unit_with(heap, chk, interp, state, Some(bitcode))
}

/// [`load_unit`] without installing the bodies: the checked state, the
/// definitions, and the globals, and no JIT at all.
///
/// For a loader that will never *call* the unit's functions in this process.
/// `compile::aot::compile_file` is the one: it links the same bitcode into the
/// executable it is building instead, so JIT-installing it would be a second
/// compilation of every prelude body per `compile-file` — measured, the
/// dominant cost of an AOT compile — for addresses nothing would ever call.
pub fn load_unit_types_only(
    heap: &mut Heap,
    chk: &mut Checker,
    interp: &mut Interp,
    state: UnitState,
) -> Result<(), String> {
    load_unit_with(heap, chk, interp, state, None)
}

fn load_unit_with(
    heap: &mut Heap,
    chk: &mut Checker,
    interp: &mut Interp,
    state: UnitState,
    bitcode: Option<&[u8]>,
) -> Result<(), String> {
    let label = state.label.clone();
    let globals = state.globals.clone();
    let items: Vec<CompiledItem> = state
        .items
        .iter()
        .map(|i| match i {
            UnitItem::Fn(path) => CompiledItem::Fn(path.clone()),
            UnitItem::Method(path, name) => CompiledItem::Method(path.clone(), name.clone()),
        })
        .collect();

    apply_types(heap, chk, interp, state)?;

    // Storage for the unit's globals, created here and in the recorded order:
    // `typelisp_rt`'s table hands ids out sequentially from zero per `Interp`,
    // and the bitcode addresses them by the id it was compiled against. The
    // check is not paranoia — `Interp::promote_global` is also called *lazily*
    // by the compile driver the first time a body mentions a global, so a load
    // that did anything before this point could already have consumed an id.
    for (name, id) in &globals {
        let path = typelisp_front::dump::parse_path(name);
        let got = interp
            .promote_global(heap, &path)
            .map_err(|e| format!("{}: creating storage for global `{}`: {}", label, path, e))?;
        if got != *id {
            return Err(format!(
                "{}: global `{}` was compiled against slot {} but this load put it in slot {} — \
                 something claimed a compiled-global id before the dump was applied",
                label, path, id, got
            ));
        }
    }

    // A unit with nothing compiled — a session that never called `(compile)` —
    // has no bitcode section at all, and asking LLVM to parse zero bytes is a
    // parse error rather than an empty module. `None` is the caller saying it
    // does not want the bodies installed at all (`load_unit_types_only`).
    let Some(bitcode) = bitcode.filter(|b| !b.is_empty()) else {
        return Ok(());
    };
    crate::compile::driver::install_compiled_library(
        interp,
        crate::compile::CompiledLibrary {
            label: &label,
            bitcode,
            items: &items,
        },
    )
}

/// Builds the dump an AOT executable embeds to answer `eval`: the prelude's
/// unit, copied verbatim from the committed artifact, followed by the program's
/// own.
///
/// Only the checked-state halves; both bitcode sections are empty. An eval'd
/// form runs interpreted — the program's compiled bodies are in the executable's
/// own object code, reachable by name, and the prelude's are not needed at all.
///
/// Run by `compile-file` against a throwaway `Heap`. `globals` is each
/// `defvar`'s path with the compiled-slot id the machine code addresses it by;
/// binding them *before* the replay is what makes the program's own `defvar`s
/// recognizable as somebody else's storage.
///
/// # Panics on nothing, fails on everything
///
/// Every error here is a build bug — the same source just type-checked — but it
/// is returned rather than aborted, because the caller is `compile-file` and a
/// user watching a compile deserves the message with the rest of its
/// diagnostics.
pub fn capture_program_dump(
    heap: &mut Heap,
    source: &str,
    globals: &[(String, usize)],
) -> Result<Vec<u8>, String> {
    let prelude = typelisp_front::dump::parse(typelisp_front::prelude::DUMP, "prelude")?;
    let prelude_unit = prelude.first().ok_or_else(|| "prelude: the committed dump holds no units".to_string())?;
    let prelude_state = typelisp_front::dump::read_state(prelude_unit.types, "prelude")?;
    typelisp_front::dump::verify_digest(&prelude_state, typelisp_front::prelude::SOURCE, typelisp_front::prelude::REGEN_SCRIPT)?;

    let mut chk = Checker::new();
    let mut interp = Interp::new();
    // The prelude's *checked state*, not a fresh interpreted load of its
    // source: this is the same environment, reached without reading and
    // type-checking 114KB of Lisp at every `compile-file`. This is a
    // compile-time environment for checking the program's source — the
    // executable rebuilds its own from the units written below
    // (`typelisp_front::dump::restore_dump`), and *that* is where the globals
    // get bound to the storage the startup sequence made for them.
    apply_types(heap, &mut chk, &mut interp, prelude_state)?;

    let before = chk.signature(heap)?;
    typelisp_front::dump::bind_globals(&interp, globals);

    // Collected as they are checked, and each one rooted for the whole of this
    // function: `exec` permanently roots a definition's *body*, not the
    // top-level node this list holds, and a later form's checking allocates.
    let mark = heap.root_count();
    let mut forms: Vec<crate::Value> = Vec::new();
    let reader = crate::Reader::new();
    let program = reader.read_all_in(heap, typelisp_front::dump::PROGRAM_LABEL, source).map_err(|e| e.to_string())?;
    for v in program {
        let tl = chk.check_form(heap, &interp, v).map_err(|e| e.to_string())?;
        // The warnings were already reported by the caller's own check of this
        // same source; repeating them would double every one.
        let _ = chk.take_warnings();
        heap.push_root(tl);
        forms.push(tl);
        if typelisp_front::dump::already_initialized_global(heap, &interp, tl) {
            continue;
        }
        interp.exec(heap, tl).map_err(|e| e.to_string())?;
    }

    let delta = chk.capture_delta(heap, &before)?;
    let state = typelisp_front::dump::capture_types(heap, delta, typelisp_front::dump::PROGRAM_LABEL, None, None, &forms, Vec::new(), globals.to_vec())?;
    while heap.root_count() > mark {
        heap.pop_root();
    }
    Ok(typelisp_front::dump::write(&[
        (prelude_unit.types.to_vec(), Vec::new()),
        (typelisp_front::dump::write_state(&state)?, Vec::new()),
    ]))
}

/// `(dump path)`: writes this session's whole environment to one file.
///
/// The dumps this environment was loaded from are re-emitted first, unit for
/// unit and byte for byte, followed by one unit for what the session itself
/// defined. What comes out is therefore self-contained: `typl --image` on it
/// rebuilds the same environment from nothing.
///
/// **Definitions, not history.** The session's `(println ...)` calls are not in
/// it, and a global comes back at whatever its `defvar` initializer produces
/// rather than the value the session last stored — see [`typelisp_front::dump`]
/// for why that trade (the one place this differs from SBCL's
/// `save-lisp-and-die`) is what makes stream handles and closures non-problems.
///
/// Unlike `save-lisp-and-die`, this does not end the process: nothing here
/// destroys the image it is writing.
pub fn dump_image(interp: &Interp, heap: &mut Heap, path: &str) -> Result<(), String> {
    let checker = interp
        .checker_handle()
        .ok_or_else(|| "dump: this environment has no checker — only the CLI and the REPL can dump".to_string())?;

    // The delta and the form list, taken while the recording is borrowed and
    // nothing else touches the heap.
    let (delta, forms, globals_before) = interp
        .with_recording(|baseline, forms, globals_before| {
            let chk = checker.borrow();
            Ok::<_, String>((chk.capture_delta(heap, baseline)?, forms.to_vec(), globals_before))
        })
        .ok_or_else(|| "dump: this session was not recording what it defines".to_string())??;

    // Every definition the session compiled, re-emitted as bitcode: a JIT'd
    // body lives in LLVM's memory as an address, and an address is not
    // something a file can carry (see this module's SBCL comparison). The
    // island is what translates them, so it has to be loaded — in `typl` it
    // always is.
    let mut plan = crate::compile::prelude_bootstrap::PreludePlan::default();
    for tl in &forms {
        crate::compile::prelude_bootstrap::collect_item(heap, *tl, &mut plan)?;
    }
    let compiled: Vec<CompiledItem> = plan
        .items
        .iter()
        .filter(|item| is_compiled(interp, item))
        .cloned()
        .collect();
    let bitcode = if compiled.is_empty() { Vec::new() } else { emit_bitcode(interp, heap, &compiled)? };

    // The globals this session promoted: ids are handed out sequentially, so
    // "past where the session started" is exactly its own.
    let mut globals: Vec<(String, usize)> = interp
        .compiled_globals
        .borrow()
        .iter()
        .filter(|(_, id)| **id >= globals_before)
        .map(|(p, id)| (p.to_string(), *id))
        .collect();
    globals.sort_by_key(|(_, id)| *id);

    let state = typelisp_front::dump::capture_types(
        heap,
        delta,
        "session",
        None,
        None,
        &forms,
        compiled.iter().map(crate::compile::prelude_bootstrap::unit_item).collect(),
        globals,
    )?;

    let mut units: Vec<(Vec<u8>, Vec<u8>)> = interp.with_dump_sources(|sources| {
        let mut out = Vec::new();
        for bytes in sources {
            for unit in typelisp_front::dump::parse(bytes, "dump")? {
                out.push((unit.types.to_vec(), unit.bitcode.to_vec()));
            }
        }
        Ok::<_, String>(out)
    })?;
    units.push((typelisp_front::dump::write_state(&state)?, bitcode));

    std::fs::write(path, typelisp_front::dump::write(&units))
        .map_err(|e| format!("dump: writing \"{}\": {}", path, e))
}

/// Whether the interpreter has a native body for `item`.
fn is_compiled(interp: &Interp, item: &CompiledItem) -> bool {
    let def = match item {
        CompiledItem::Fn(path) => interp.root.borrow().get_fn(path),
        CompiledItem::Method(type_path, method) => interp.root.borrow().get_method(type_path, method),
    };
    def.is_some_and(|d| d.compiled.borrow().is_some())
}

/// Compiles `items` into one module and serializes it — the same shape the
/// prelude and island generators build, minus their forward-declaration pass
/// (a session's definitions were compiled once already, so their call graph is
/// known to work).
fn emit_bitcode(interp: &Interp, heap: &mut Heap, items: &[CompiledItem]) -> Result<Vec<u8>, String> {
    use inkwell::AddressSpace;
    use std::cell::RefCell;
    use std::rc::Rc;

    let ctx = crate::compile::llvm_context();
    let module = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("session");
        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        for (name, _) in crate::compile::externs::rt_extern_functions() {
            module.add_function(name, fn_ty, None);
        }
        for item in items {
            let sym = item.symbol_name();
            if module.get_function(&sym).is_none() {
                module.add_function(&sym, fn_ty, None);
            }
        }
        Rc::new(RefCell::new(module))
    };

    // Without the lock held: `add_compiled_function` takes it per LLVM builtin
    // call and `Mutex` is not reentrant — the same constraint every other
    // compile loop in this crate documents.
    for item in items {
        let node = item.node_name();
        crate::compile::driver::add_compiled_function(interp, heap, module.clone(), &node, &item.symbol_name())
            .map_err(|e| format!("dump: re-compiling `{}` failed: {}", node, e))?;
    }

    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    let bitcode = {
        let m = module.borrow();
        m.verify().map_err(|e| format!("dump: the session module failed verification: {}", e)).map(|()| {
            // Trailing NUL included by design — see `bootstrap.rs`'s write site.
            m.write_bitcode_to_memory().as_slice().to_vec()
        })
    };
    // Destroyed with the guard still held, like every other module here.
    drop(module);
    bitcode
}

/// Applies every unit in a dump file, in order — `typl --image`.
///
/// `chk`/`interp` must be fresh: a dump is a whole environment, not something
/// to layer on top of one. The prelude's and island's units are digest-checked
/// against the sources compiled into *this* binary, so an image written by a
/// different build is refused rather than half-applied.
pub fn load_image(
    heap: &mut Heap,
    chk: &mut Checker,
    interp: &mut Interp,
    bytes: std::borrow::Cow<'static, [u8]>,
    label: &str,
) -> Result<(), String> {
    // The backend has to be available before any unit's bitcode is installed,
    // and `compiler::load_aot` — which normally does this — is exactly what an
    // image load replaces.
    crate::compile::install_llvm_backend();

    for unit in typelisp_front::dump::parse(&bytes, label)? {
        let state = typelisp_front::dump::read_state(unit.types, label)?;
        // A unit built from source this binary also carries has to match it.
        // Which source is decided by the unit's own label, the only thing that
        // says what it was built from.
        match state.label.as_str() {
            "prelude" => typelisp_front::dump::verify_digest(
                &state,
                typelisp_front::prelude::SOURCE,
                typelisp_front::prelude::REGEN_SCRIPT,
            )?,
            "compiler island" => typelisp_front::dump::verify_digest(
                &state,
                crate::compiler::SOURCE,
                crate::compiler::REGEN_SCRIPT,
            )?,
            _ => {}
        }
        load_unit(heap, chk, interp, state, unit.bitcode)?;
    }
    interp.push_dump_source(bytes);
    Ok(())
}
