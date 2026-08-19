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
    regen_script: &str,
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
    // parse error rather than an empty module.
    if bitcode.is_empty() {
        return Ok(());
    }
    crate::compile::driver::install_compiled_library(
        interp,
        crate::compile::CompiledLibrary {
            label: &label,
            regen_script,
            bitcode,
            items: &items,
            // The unit and its bitcode were written by one pass into one file,
            // so there is no second artifact to have drifted from. What *can*
            // drift is the source both were built from, and that is checked
            // against `UnitState::source_digest` before this is reached.
            expected_hash: None,
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
    // type-checking 114KB of Lisp at every `compile-file`. Its `globals` are
    // deliberately left unbound — this executable never compiled the prelude,
    // so nothing made compiled storage for them, and the `defvar`s applied
    // above put their values where an interpreted `eval` reads them.
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
    chk.predeclare_program(heap, &program);
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
