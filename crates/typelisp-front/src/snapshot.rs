//! The AOT `eval` environment, written down at compile time.
//!
//! `eval` type-checks its argument against the program's *current global
//! environment* and then runs it, so a standalone executable that calls it
//! needs a `Checker` holding every signature, type and macro in scope, and an
//! `Interp` holding a runnable body for each of them. Neither is in the
//! machine code: a compiled `tl_double` is an address, and an address carries
//! no parameter types and answers to no name.
//!
//! The first version of this built that environment at startup, by reading and
//! type-checking the prelude and the program's own embedded source all over
//! again. This module replaces that with what `compile-file` already knows:
//! it builds the environment once, at compile time ([`capture`]), and writes
//! the result into the executable, where startup only has to rebuild it
//! ([`restore`]).
//!
//! **What is saved is checked state, not "type information".** Signatures
//! alone would let `(eval '(double 21))` type-check and then have nothing to
//! run. So the snapshot carries both halves: the checker's registry, and every
//! checked top-level core form, which `restore` re-executes to repopulate the
//! interpreter's own tables.
//!
//! **What is not saved.** `Registry::def_locs` — goto-definition/hover data,
//! read only by `typl-lsp`, which is not what an executable does. It comes
//! back empty; nothing in the `eval` path consults it.
//!
//! The format is deliberately private and versioned: producer and consumer are
//! the same build of the same binary, so a mismatch is a build bug and
//! [`restore`] says so rather than guessing. Note that the bytes are *not*
//! reproducible across runs — `Namespace` is built out of `HashMap`s and
//! bincode writes a map in iteration order. Nothing depends on byte equality
//! today; making it hold would mean ordered maps in the checker.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::check::registry::{Docs, Namespace};
use crate::check::Checker;
use crate::eval::interp::Interp;
use crate::owned_form::{owned_to_value, OwnedForm};
use crate::types::Path;
use crate::{Heap, Reader, Type, Value};

/// Bumped whenever the wire shape changes. Producer and consumer ship
/// together, so this catches a stale artifact rather than a version skew
/// anyone has to migrate.
pub const FORMAT_VERSION: u32 = 1;

/// Everything an executable needs to answer `eval`.
#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    checker: CheckerState,
    /// Every checked top-level core form, in declaration order — the prelude's
    /// followed by the program's. Re-executed by [`restore`], which is what
    /// fills the interpreter's function/type/macro tables.
    forms: Vec<OwnedForm>,
    /// `(global path, compiled slot id)`. The interpreter consults
    /// `compiled_globals` before its own module tree, so binding these is the
    /// whole of sharing storage with the compiled program.
    globals: Vec<(String, usize)>,
}

/// The durable half of a [`Checker`] — what a restored one must have to check
/// a new form exactly as the original would.
///
/// Everything else on a `Checker` is either per-form scratch (the loop stack,
/// the specialization queue, the LSP accumulators) or a caller's policy
/// choice, and a fresh one starts with the right value for both.
#[derive(Serialize, Deserialize)]
pub struct CheckerState {
    pub root: Namespace,
    pub docs: Docs,
    pub throw_tags: BTreeMap<String, Type>,
    pub fn_templates: Vec<(Path, WireFnTemplate)>,
    pub method_templates: Vec<((Path, String), WireMethodTemplate)>,
    pub predeclared: Vec<String>,
}

/// A generic `defun`'s retained source form, heap-independent.
///
/// The live `FnTemplate` holds `Vec<Value>` — raw read forms, permanently
/// rooted. 61 of the prelude's 86 `defun`s are generic, so this is the bulk of
/// a snapshot: an eval'd call at a new type argument re-checks the template.
#[derive(Serialize, Deserialize)]
pub struct WireFnTemplate {
    pub parts: Vec<OwnedForm>,
    pub ns: Vec<String>,
    pub type_params: Vec<String>,
}

/// The `MethodTemplate` counterpart. `Getter`/`Setter` carry no form at all —
/// `check_defstruct` synthesizes those bodies, so specialization re-synthesizes
/// them from the owner's `AdtDef`.
#[derive(Serialize, Deserialize)]
pub enum WireMethodTemplate {
    Form { parts: Vec<OwnedForm>, ns: Vec<String>, written_vars: Vec<String> },
    Getter { index: usize },
    Setter { index: usize },
}

/// Builds the `eval` environment — prelude, then `source`'s own definitions —
/// and serializes it.
///
/// Run by `compile-file` (`compile::aot`), once, against a throwaway `Heap`.
/// `globals` is each `defvar`'s path with the compiled-slot id the machine code
/// addresses it by; binding them *before* the replay is what makes the
/// program's own `defvar`s recognizable as somebody else's storage.
///
/// # Panics on nothing, fails on everything
///
/// Every error here is a build bug — the same source just type-checked — but it
/// is returned rather than aborted, because the caller is `compile-file` and a
/// user watching a compile deserves the message with the rest of its
/// diagnostics.
pub fn capture(heap: &mut Heap, source: &str, globals: &[(String, usize)]) -> Result<Vec<u8>, String> {
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    // Collected as they are checked, and each one rooted for the whole of
    // `capture`: `exec` permanently roots a definition's *body*, not the
    // top-level node this list holds, and a later form's checking allocates.
    let mut forms: Vec<Value> = Vec::new();
    let mark = heap.root_count();

    crate::prelude::load_interpreted_with(
        heap,
        &mut chk,
        &mut interp,
        &mut |_, _| {},
        &mut |heap, tl| {
            heap.push_root(tl);
            forms.push(tl);
        },
    );

    for (name, id) in globals {
        interp.bind_compiled_global(parse_path(name), *id);
    }

    let reader = Reader::new();
    let program = reader.read_all_in(heap, "<program>", source).map_err(|e| e.to_string())?;
    chk.predeclare_program(heap, &program);
    for v in program {
        let tl = chk.check_form(heap, &interp, v).map_err(|e| e.to_string())?;
        // The warnings were already reported by the caller's own check of this
        // same source; repeating them would double every one.
        let _ = chk.take_warnings();
        heap.push_root(tl);
        forms.push(tl);
        if already_initialized_global(heap, &interp, tl) {
            continue;
        }
        interp.exec(heap, tl).map_err(|e| e.to_string())?;
    }

    let snapshot = Snapshot {
        version: FORMAT_VERSION,
        checker: chk.capture_state(heap).map_err(|e| e.to_string())?,
        forms: forms
            .iter()
            .map(|v| crate::owned_form::value_to_owned(heap, *v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
        globals: globals.to_vec(),
    };
    while heap.root_count() > mark {
        heap.pop_root();
    }
    bincode::serialize(&snapshot).map_err(|e| format!("eval snapshot: serializing failed: {}", e))
}

/// [`capture`]'s inverse: rebuild the environment in `heap`.
///
/// The forms are rebuilt and executed one at a time rather than all at once —
/// each is rooted only across its own `exec`, which is where it acquires the
/// permanent root that keeps it alive after.
pub fn restore(heap: &mut Heap, bytes: &[u8]) -> Result<Interp, String> {
    let snapshot: Snapshot =
        bincode::deserialize(bytes).map_err(|e| format!("eval snapshot: reading failed: {}", e))?;
    if snapshot.version != FORMAT_VERSION {
        return Err(format!(
            "eval snapshot: format version {}, expected {} — the executable and the runtime were built \
             from different sources",
            snapshot.version, FORMAT_VERSION
        ));
    }

    let chk = Checker::from_state(snapshot.checker, heap).map_err(|e| e.to_string())?;
    // After the checker, and before anything runs: `Interp::new` resets the
    // runtime global table (`typelisp_rt::reset_global_table`), which is why an
    // AOT program's startup calls this ahead of its own global-init sequence.
    let mut interp = Interp::new();
    for (name, id) in &snapshot.globals {
        interp.bind_compiled_global(parse_path(name), *id);
    }

    for f in &snapshot.forms {
        let tl = owned_to_value(heap, f).map_err(|e| e.to_string())?;
        heap.push_root(tl);
        let skip = already_initialized_global(heap, &interp, tl);
        let r = if skip { Ok(None) } else { interp.exec(heap, tl) };
        heap.pop_root();
        r.map_err(|e| e.to_string())?;
    }

    chk.take_warnings();
    interp.set_checker(std::rc::Rc::new(std::cell::RefCell::new(chk)));
    Ok(interp)
}

/// Whether `tl` is a `defvar` for a global the compiled program has already
/// created and initialized.
///
/// Executing it would run the initializer a second time — with its side
/// effects, and overwriting whatever the program has since stored — because the
/// interpreter's `defvar` always evaluates and assigns (typelisp's `defvar` is
/// not CL's "only if unbound"). Skipping it costs nothing else: the *checker*
/// still saw the form, which is what `eval` needs to type-check a reference,
/// and both reads and writes consult `compiled_globals` before the module tree.
///
/// Through `core::path_field`, not by matching `Value::Path`: a root-level name
/// is stored as a bare `Value::Symbol`, so reading the field by hand silently
/// misses every unqualified global — which is most of them.
fn already_initialized_global(heap: &Heap, interp: &Interp, tl: Value) -> bool {
    if crate::check::core::op(heap, tl) != Some("defvar") {
        return false;
    }
    match crate::check::core::path_field(heap, tl, 0) {
        Some(path) => interp.has_compiled_global(&path),
        None => false,
    }
}

/// `m::x` back into the `Path` the interpreter keys its tables by — the inverse
/// of the `Display` `compile-file` wrote the name with.
fn parse_path(name: &str) -> Path {
    Path::from_segments(name.split("::").map(str::to_string).collect())
}
