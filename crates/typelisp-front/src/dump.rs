//! Dumps: what a compilation produced, written down so it never has to be
//! derived again.
//!
//! A *unit* is one compilation's output — the prelude, the compiler island, a
//! source file, a REPL session — and it always has two halves: the native
//! bodies (LLVM bitcode, held by the backend) and the checked state that says
//! what those bodies are (this module). A *dump* is an ordered sequence of
//! units, applied front to back.
//!
//! **Why not a memory image.** SBCL's `save-lisp-and-die` writes the heap out
//! verbatim, which works because in SBCL compiled code, the environment and
//! every value are all objects in that one heap. Here only the third is: the
//! native bodies live in LLVM's JIT memory as bare addresses, and the checker's
//! tables are Rust `HashMap`s outside the cons arena. So the code has to be
//! written as bitcode (not addresses) and the environment as serialized state
//! — the same position ECL is in, and for the same reason.
//!
//! **A unit carries what it added, not everything in scope.** The island unit
//! holds the island's definitions, not the prelude's as well: what the
//! checker's registry was written since the unit began
//! ([`RegistrySignature`]). Cumulative units would mean replacing the
//! checker wholesale on each load, which would re-root every generic body
//! the previous unit permanently rooted with no way to drop the old ones.
//!
//! **What a dump does not save.** Values: `defvar` initializers are re-run when
//! the unit loads, so a global comes back at its initial value rather than
//! whatever the session last stored. That is the one place this deliberately
//! differs from SBCL, and it is what makes stream handles, closures and foreign
//! pointers non-problems — none of them are ever written down. Non-definition
//! top-level forms are not saved either (a dump that re-prints your session's
//! output on load is nobody's idea of a dump), nor are the places definitions
//! were written, which only `typl-lsp` reads.

use typelisp_mem::wk;
use std::collections::hash_map::DefaultHasher;
use std::convert::TryInto;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::owned_form::OwnedForm;
use crate::types::Path;

/// Bumped whenever the wire shape changes. Producer and consumer ship
/// together, so this catches a stale artifact rather than a version skew
/// anyone has to migrate — the same stance SBCL takes with its core files
/// ("there is absolutely no binary compatibility of core images between
/// different runtime support programs").
pub const FORMAT_VERSION: u32 = 38;

/// Where the checker's registry's writes were at some point: taken before a
/// unit is loaded and handed to [`crate::typeck::checker::Checker::capture_delta`]
/// afterwards, which writes down everything written since.
pub struct RegistrySignature(u64);

impl RegistrySignature {
    /// The registry's writes as of the generation `generation`.
    pub(crate) fn at(generation: u64) -> RegistrySignature {
        RegistrySignature(generation)
    }

    pub(crate) fn generation(&self) -> u64 {
        self.0
    }
}

/// Identifies the file and, with [`FORMAT_VERSION`], what is in it.
pub const MAGIC: &[u8; 6] = b"TYPLD\0";

/// The container's own version, distinct from [`FORMAT_VERSION`] (the unit
/// payload's): a change to the directory layout and a change to what a unit
/// records are different events, and either one alone should be able to reject
/// an old file.
pub const CONTAINER_VERSION: u32 = 1;

const HEADER: usize = 6 + 4 + 4;
const DIRECTORY_ENTRY: usize = 8 * 4;

/// One unit as it sits in a dump: the bytes of its checked state, and the
/// bitcode holding the bodies that state describes.
///
/// Borrowed, not owned: the prelude's and island's dumps are `include_bytes!`
/// statics, and the bitcode is handed straight to LLVM, which copies it into
/// its own `MemoryBuffer` anyway.
pub struct UnitRef<'a> {
    pub types: &'a [u8],
    pub bitcode: &'a [u8],
}

/// Lays out `units` as a dump.
pub fn write(units: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    out.extend_from_slice(&(units.len() as u32).to_le_bytes());

    let mut offset = (HEADER + DIRECTORY_ENTRY * units.len()) as u64;
    for (types, bitcode) in units {
        for section in [types, bitcode] {
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&(section.len() as u64).to_le_bytes());
            offset += section.len() as u64;
        }
    }
    for (types, bitcode) in units {
        out.extend_from_slice(types);
        out.extend_from_slice(bitcode);
    }
    out
}

/// Reads a dump's directory, without touching the payloads.
pub fn parse<'a>(bytes: &'a [u8], label: &str) -> Result<Vec<UnitRef<'a>>, String> {
    if bytes.len() < HEADER || &bytes[..6] != MAGIC {
        return Err(format!("{}: not a typelisp dump", label));
    }
    let version = u32::from_le_bytes(bytes[6..10].try_into().expect("4 bytes"));
    if version != CONTAINER_VERSION {
        return Err(format!(
            "{}: dump container version {}, expected {} — the file and this build are from \
             different sources",
            label, version, CONTAINER_VERSION
        ));
    }
    let count = u32::from_le_bytes(bytes[10..14].try_into().expect("4 bytes")) as usize;
    let directory_end = HEADER + DIRECTORY_ENTRY * count;
    if bytes.len() < directory_end {
        return Err(format!("{}: dump claims {} units but ends inside its directory", label, count));
    }

    let word = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 bytes")) as usize;
    let mut units = Vec::with_capacity(count);
    for i in 0..count {
        let base = HEADER + DIRECTORY_ENTRY * i;
        let section = |half: usize| -> Result<&[u8], String> {
            let (off, len) = (word(base + half * 16), word(base + half * 16 + 8));
            bytes
                .get(off..off + len)
                .ok_or_else(|| format!("{}: dump unit {} points past the end of the file", label, i))
        };
        units.push(UnitRef { types: section(0)?, bitcode: section(1)? });
    }
    Ok(units)
}

/// What one unit wrote of the checker's registry
/// (`typeck::wire::RebuiltUnit`, serialized): what the unit defined, and its
/// generic bodies.
#[derive(Default, Serialize, Deserialize)]
pub struct CheckerDelta {
    pub registry: Vec<u8>,
}

/// One compilation's output: what it added to the checker, and what its
/// bitcode holds bodies for.
///
/// Written into a dump next to the bitcode it describes. The two are produced
/// together, by the same pass, and are meaningless apart — an address with no
/// signature answers to no name, and a signature with no body type-checks a
/// call that then has nothing to run.
#[derive(Serialize, Deserialize)]
pub struct UnitState {
    /// [`FORMAT_VERSION`] as of writing. Checked by [`read_state`]: producer
    /// and consumer ship together, so a mismatch is a stale file rather than a
    /// version skew anyone has to migrate.
    pub version: u32,
    /// For diagnostics: "prelude", "island", a source path, "session".
    pub label: String,
    /// The hash of the source text this unit was built from, for the units
    /// (prelude, island) whose source is compiled into the binary and can
    /// therefore drift out from under the artifact. `None` for a unit built
    /// from source the loader will never see again.
    pub source_digest: Option<u64>,
    /// The hash of the *forms* the source read into, position-insensitive
    /// (`compile::bootstrap::hash_read_forms`). Not consulted at load time —
    /// `source_digest` is what says the dump is current. This is for the
    /// regeneration script, which reuses the committed bitcode section
    /// whenever the forms are unchanged, so editing a comment does not cost a
    /// full recompilation of the prelude or the island.
    pub forms_digest: Option<u64>,
    pub checker: CheckerDelta,
    /// Every checked top-level core form this unit defines, in declaration
    /// order. Re-executed on load, which is what fills the interpreter's
    /// function/type/macro tables.
    pub forms: Vec<OwnedForm>,
    /// Every definition the bitcode section carries a body for.
    pub items: Vec<crate::CompileTarget>,
    /// Which ABI the bodies in this unit's bitcode were emitted under
    /// (`typelisp_abi::BODY_ABI_CLASSIC` or `BODY_ABI_COROUTINE`).
    ///
    /// **The one thing a dump cannot be asked to guess.** An artifact outlives
    /// the process that made it, and the ABI is not visible in the bitcode —
    /// calling a `f(args, argc)` body as `f(frame)` is not a type error
    /// anywhere, it is a wrong answer or a crash. So it is written down.
    ///
    /// It is also not derivable from [`version`](Self::version): during the
    /// changeover the island is compiled *by its predecessor*, so exactly one
    /// generation has bodies in the old ABI inside a dump written by the new
    /// code. That generation is the reason this field exists.
    pub body_abi: u8,
    /// For a unit whose bodies are themselves a *compiler* (the island): which
    /// ABI the code **they emit** uses.
    ///
    /// Distinct from [`body_abi`](Self::body_abi), and the distinction is the
    /// whole of the changeover: the middle generation's bodies are old while
    /// what they emit is new. Meaningless, and equal to `body_abi`, for
    /// everything that is not the island.
    pub emits_abi: u8,
    /// Which tag layout the bodies in this unit's bitcode were emitted under
    /// (`typelisp_mem::tagged::LAYOUT_*`).
    ///
    /// The layout is the ABI's twin in every respect that made
    /// [`body_abi`](Self::body_abi) necessary: baked into the bodies as inline
    /// tag tests and shifts, invisible in the bitcode, and a wrong answer
    /// rather than an error when mismatched. A body under another layout
    /// cannot run on this runtime at all, so the loader refuses it outright.
    pub body_layout: u8,
    /// For the island: which layout the code its bodies **emit** uses. The
    /// middle generation of a layout changeover has old bodies emitting new
    /// code, the same shape as [`emits_abi`](Self::emits_abi). Equal to
    /// `body_layout` for everything that is not the island.
    pub emits_layout: u8,
    /// `(global path, compiled slot id)`. Written as the pairs that were
    /// actually assigned rather than as an order to re-derive: `promote_global`
    /// is called both eagerly in declaration order *and* lazily by the compile
    /// driver when it first sees a reference, so the sequence is not something
    /// a second process can reproduce by walking the source.
    pub globals: Vec<(Path, usize)>,
}

/// Writes down what a unit added, for a unit that **carries no bodies**.
///
/// `checker` comes from [`crate::Checker::capture_delta`], taken against a
/// signature from before the unit loaded — and taken *before any later unit
/// loads*, since the checker they all share cannot say afterwards which of
/// them added what. `forms` are the unit's checked top-level forms in
/// declaration order, and must still be rooted: this reads them out of `heap`.
///
/// **It takes no `items` on purpose.** The ABI fields it fills in are
/// meaningless for a unit with no bitcode, so it fills them in classic and
/// says nothing — and a caller that *did* have bodies would get that silence
/// recorded as a convention. That mistake was made twice (the session dump
/// and the prelude, both noted at their call sites) and neither announced
/// itself: the label is only read when something *calls* one of those bodies,
/// and then it is a wrong convention rather than an error. A unit with
/// bodies has to use [`capture_types_with_abi`], which cannot be called
/// without answering.
pub fn capture_types(
    heap: &crate::Heap,
    checker: CheckerDelta,
    label: &str,
    source_digest: Option<u64>,
    forms_digest: Option<u64>,
    forms: &[crate::Value],
    globals: Vec<(Path, usize)>,
) -> Result<UnitState, String> {
    capture_types_with_abi(
        heap,
        checker,
        label,
        source_digest,
        forms_digest,
        forms,
        Vec::new(),
        globals,
        typelisp_abi::BODY_ABI_CLASSIC,
        typelisp_abi::BODY_ABI_CLASSIC,
        typelisp_mem::tagged::LAYOUT,
        typelisp_mem::tagged::LAYOUT,
    )
}

/// [`capture_types`] for a unit that has something to say about ABIs — the
/// island, whose bodies and whose *output* can differ for one generation.
#[allow(clippy::too_many_arguments)]
pub fn capture_types_with_abi(
    heap: &crate::Heap,
    checker: CheckerDelta,
    label: &str,
    source_digest: Option<u64>,
    forms_digest: Option<u64>,
    forms: &[crate::Value],
    items: Vec<crate::CompileTarget>,
    globals: Vec<(Path, usize)>,
    body_abi: u8,
    emits_abi: u8,
    body_layout: u8,
    emits_layout: u8,
) -> Result<UnitState, String> {
    Ok(UnitState {
        version: FORMAT_VERSION,
        label: label.to_string(),
        source_digest,
        forms_digest,
        body_abi,
        emits_abi,
        body_layout,
        emits_layout,
        checker,
        forms: forms
            .iter()
            .map(|v| crate::owned_form::value_to_owned(heap, *v).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?,
        items,
        globals,
    })
}

/// [`capture_types`]'s inverse: put a unit's checker state and definitions
/// back, in `heap`.
///
/// The forms are rebuilt and executed one at a time rather than all at once —
/// each is rooted only across its own `exec`, which is where it acquires the
/// permanent root that keeps it alive after.
///
/// This is the front end's half of loading a unit. Installing the native
/// bodies over the definitions it registers is the backend's
/// (`compile::dump::load_unit`), and a build with no LLVM backend can do this
/// half alone and run everything interpreted.
///
/// Deliberately does **not** touch the unit's globals: whether they have to be
/// *created* here or merely *bound* to storage somebody else made is the one
/// thing that differs between the two callers, and neither answer is safe to
/// guess. A JIT load owns the storage and calls `Interp::promote_global` after
/// this (`compile::dump::load_unit`); an AOT executable's compiled startup
/// creates it, so that caller calls [`bind_globals`] *before* this — which is
/// also what lets `already_initialized_global` below recognise the program's
/// own `defvar`s.
pub fn apply_types(
    heap: &mut crate::Heap,
    chk: &mut crate::Checker,
    interp: &mut crate::Interp,
    unit: UnitState,
) -> Result<(), String> {
    chk.apply_delta(unit.checker, heap)?;
    for f in &unit.forms {
        let tl = crate::owned_form::owned_to_value(heap, f).map_err(|e| e.to_string())?;
        heap.push_root(tl);
        let skip = already_initialized_global(heap, interp, tl);
        let r = if skip { Ok(None) } else { interp.exec(heap, tl) };
        heap.pop_root();
        r.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Records that these globals already have storage, made by somebody else.
///
/// The AOT half of the split described on [`apply_types`].
pub fn bind_globals(interp: &crate::Interp, globals: &[(Path, usize)]) {
    for (path, id) in globals {
        interp.bind_compiled_global(path.clone(), *id);
    }
}

/// Whether `tl` is a `defvar` for a global the compiled program has already
/// created and initialized.
///
/// Executing it would run the initializer a second time — with its side
/// effects, and overwriting whatever the program has since stored — because the
/// interpreter's `defvar` always evaluates and assigns (typelisp's `defvar` is
/// not CL's "only if unbound"). Skipping it costs nothing else: the *checker*
/// still saw the form, which is what a later `eval` needs to type-check a
/// reference, and both reads and writes consult `compiled_globals` before the
/// module tree.
///
/// Through `core::definition_field`, not by matching `Value::Path`: a root-level name
/// is stored as a bare `Value::Symbol`, so reading the field by hand silently
/// misses every unqualified global — which is most of them.
pub fn already_initialized_global(heap: &crate::Heap, interp: &crate::Interp, tl: crate::Value) -> bool {
    if !crate::check::core::op_is(heap, tl, wk::DEFVAR) {
        return false;
    }
    match crate::check::core::definition_field(heap, tl, 0) {
        Some(path) => interp.has_compiled_global(&path),
        None => false,
    }
}

/// A unit's checked state as bytes.
///
/// The wire format is this crate's, so the encoder lives here rather than in
/// the backend that assembles the file around it — nothing outside this module
/// needs to know it is bincode.
pub fn write_state(state: &UnitState) -> Result<Vec<u8>, String> {
    bincode::serialize(state).map_err(|e| format!("{}: writing the dump failed: {}", state.label, e))
}

/// [`write_state`]'s inverse.
pub fn read_state(bytes: &[u8], label: &str) -> Result<UnitState, String> {
    let state: UnitState = bincode::deserialize(bytes)
        .map_err(|e| format!("{}: reading the dump's checked state failed: {}", label, e))?;
    if state.version != FORMAT_VERSION {
        return Err(format!(
            "{}: dump unit version {}, expected {} — the file and this build are from different \
             sources",
            label, state.version, FORMAT_VERSION
        ));
    }
    Ok(state)
}

/// The staleness key for a unit whose source ships in the same binary as the
/// dump — the prelude and the compiler island.
///
/// Over the source *bytes*, deliberately, and not over the forms the reader
/// produced the way the old `.bc` freshness check was. A unit records checked
/// forms, and a checked form carries its `Loc`: adding a comment line shifts
/// every position after it, so an artifact that survived a comment edit would
/// be reporting the wrong line for the rest of its life. Comment edits
/// therefore need a regeneration now — cheap, since the regeneration script
/// keeps the bitcode section it already has whenever the *forms* are unchanged.
pub fn source_digest(source: &str) -> u64 {
    sources_digest(&[source])
}

/// [`source_digest`] over more than one source, for a dump built from several.
///
/// The prelude's is: its unit holds the core macro layer's definitions
/// (`typelisp_front::core_macros::SOURCE`) as well as its own, so hashing only
/// one of the two would let an edit to the other ship against a stale dump —
/// the "several inputs, one hash" failure the island artifact already had once.
/// Order matters and is the load order.
pub fn sources_digest(sources: &[&str]) -> u64 {
    let mut h = DefaultHasher::new();
    for s in sources {
        s.hash(&mut h);
    }
    h.finish()
}

/// Fails unless `state` was built from `source`.
///
/// An error, never a silent rebuild: a dump that disagrees with the source
/// compiled into the same binary is a build the developer has to fix, and
/// quietly falling back to reading the source would hide exactly the case this
/// exists to catch — an edit that appears to do nothing.
pub fn verify_digest(state: &UnitState, source: &str, regen_script: &str) -> Result<(), String> {
    verify_sources_digest(state, &[source], regen_script)
}

/// [`verify_digest`] for a dump built from several sources — see
/// [`sources_digest`].
pub fn verify_sources_digest(state: &UnitState, sources: &[&str], regen_script: &str) -> Result<(), String> {
    match state.source_digest {
        Some(d) if d == sources_digest(sources) => Ok(()),
        Some(_) => Err(format!(
            "{}: the dump is stale relative to its source — run {}",
            state.label, regen_script
        )),
        None => Err(format!(
            "{}: the dump carries no source digest, but is being loaded against a source that could \
             have changed — run {}",
            state.label, regen_script
        )),
    }
}

/// The label a program's own unit carries in an AOT executable's embedded dump.
pub const PROGRAM_LABEL: &str = "<program>";

/// Rebuilds an embedded environment dump in `heap` — the inverse of
/// `compile::dump::capture_program_dump`, which is what wrote it.
///
/// Applies each unit in order. No bitcode is installed — a dump embedded in an
/// executable carries none, and this crate has no installer anyway.
///
/// `ns` is the namespace a later eval'd form is checked in: the entry file's
/// module, where `typl file.typl` places one too, so a bare name in it
/// resolves to the program's own definition.
pub fn restore_dump(heap: &mut crate::Heap, bytes: &[u8], ns: typelisp_mem::NsId) -> Result<crate::Interp, String> {
    let mut chk = crate::Checker::new();
    // Before anything runs: `Interp::new` resets the runtime global table
    // (`typelisp_rt::reset_global_table`), which is why an AOT program's startup
    // calls this ahead of its own global-init sequence.
    let mut interp = crate::Interp::new();

    for unit in parse(bytes, "eval environment")? {
        let state = read_state(unit.types, "eval environment")?;
        // Every unit's globals are somebody else's storage: an AOT executable
        // carries the prelude's compiled bodies as well as its own, and those
        // bodies address their globals by baked-in slot id, so the executable's
        // startup created the storage for the prelude's `defvar`s before this
        // ran (`compile::aot::compile_file`'s global-init sequence). Binding
        // them here is what makes an eval'd `*print-pretty*` read that same
        // storage instead of a second, interpreted copy of it — and it is why
        // the `defvar`s below are skipped rather than re-assigned.
        bind_globals(&interp, &state.globals);
        apply_types(heap, &mut chk, &mut interp, state)?;
    }

    chk.take_warnings();
    chk.set_current_ns(ns);
    interp.set_checker(std::rc::Rc::new(std::cell::RefCell::new(chk)));
    Ok(interp)
}

/// Collects the definitions in one checked top-level form, for a session that
/// is recording what it defines.
///
/// A dump records definitions, not history: re-running a session's `(println
/// ...)` on load is nobody's idea of a dump, and a file-derived `(module ...)`
/// wrapping the whole script would drag every one of them in — including the
/// `(dump ...)` call itself. So a container contributes *nothing* here: its
/// items come back through `Interp::exec` on their own and each records itself,
/// which also puts them in the order they ran. That covers a written `module`,
/// an `impl` block's grouping, a `deftrait`'s empty one, and the checker's
/// monomorphization bundle (whose specializations are real `defun`s compiled
/// code may call by their mangled names) with one rule instead of four.
pub fn record_definitions(heap: &crate::Heap, tl: crate::Value, out: &mut Vec<crate::Value>) {
    use typelisp_mem::wk;
    if matches!(
        crate::check::core::op_wk(heap, tl),
        Some(wk::DEFUN | wk::DEFMETHOD | wk::DEFVAR | wk::DEFSTRUCT | wk::DEFENUM | wk::DEFMACRO | wk::USE | wk::DEFFFI)
    ) {
        out.push(tl);
    }
}
