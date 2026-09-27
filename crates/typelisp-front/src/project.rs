//! File-to-module mapping and on-demand module loading.
//!
//! The rule (see `docs/dev/language-design.md` §2.4): a source file's path
//! relative to the project's source root *is* its module path — directories
//! and the file's basename each become one `Path` segment, so
//! `<root>/geo/point.typl` provides `geo::point`. A file's whole content is
//! implicitly wrapped in that module; an explicit `(module bar ...)` inside
//! the file nests *inside* it (`geo::point::bar`), so the derived path and
//! explicit `module` forms can never conflict.
//!
//! The source root is marked by a `typelisp.toml` manifest, found by walking
//! up from the file being processed. The manifest may be empty; its one
//! recognized key is `src = "dir"`, naming the source directory relative to
//! the manifest (hand-parsed — one optional key doesn't justify a TOML
//! dependency, the same call `main.rs::parse_redef_policy` makes). With no
//! manifest, the entry file's own directory is the root.
//!
//! Dependencies load on demand: before a file's forms are checked, its
//! top-level `(use path)` forms (including ones nested in `(module ...)`) are
//! scanned, and each path that maps to an existing not-yet-loaded `.typl`
//! file is loaded recursively — read, checked into the *same*
//! `Checker`/`Registry`, and queued for execution. Scanning ahead of checking
//! (rather than catching `Error::ModuleNotLoaded` mid-check and retrying the
//! form) means no form is ever checked twice, so loading can't produce
//! spurious redefinition warnings. The structured error still exists as the
//! checker's signal for a `use` naming a path no file maps to.
//!
//! A `use` produced by *macro expansion* can't be seen by that pre-check scan
//! (the macro it comes from is only registered by checking an earlier form of
//! the very same file, so a scan running before any checking has nothing to
//! expand — the reason the scan alone can't cover this). Instead, the check
//! loop pre-expands each top-level macro call
//! (`Checker::try_expand_toplevel_macro`), scans the expansion the same way
//! (loading any dependency it `use`s — with the checker's namespace context
//! suspended, see `try_load_module`), and checks the expanded form directly,
//! so nothing expands twice. Consequence: a macro-generated `use` works in
//! any form *after* the `defmacro`, which is the strongest guarantee possible
//! under single-pass checking. See `tests/macro_use_test.rs`.
//!
//! Circular dependencies are a hard error, reported with the whole chain
//! (`circular module dependency: a -> b -> a`). The checker is single-pass —
//! declarations aren't separated from definitions the way C headers allow —
//! so silently skipping an in-progress module would just surface later as a
//! baffling "no such function" on some unloaded item.
//!
//! A caller with in-editor buffers open (the LSP) can register them via
//! [`Loader::set_overlay`] so a dependency load sees an unsaved edit
//! immediately rather than the stale file on disk.
//!
//! GC-root discipline (the `main.rs::try_run_pending` invariant): each file's
//! read roots are popped as soon as that file's forms are checked — nested
//! loads are strictly LIFO above the outer file's roots, so stack order
//! holds. A form is rooted by [`crate::read::Forms::next_form`] as it is
//! read, and the roots of a file accumulate interleaved with its checked
//! forms' roots until the whole file is done; nothing pops in between, which
//! is what keeps the stack LIFO even though the read no longer happens all at
//! once. Checked forms are queued (dependency-first, since a dependency's
//! load completes before the dependent's check resumes) and executed by the
//! driver only after every root is popped; the one exception is `defmacro`
//! registration, which must precede later macro *uses* in the same load and
//! is safe mid-load because it never touches the root stack.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path as FsPath, PathBuf};

use crate::check::core;
use crate::{wk, Checker, Error, Heap, Interp, Path, Reader, TopLevelForm, Value, MONO_BUNDLE_MODULE};

/// The manifest file that marks a project root.
pub const MANIFEST_NAME: &str = "typelisp.toml";

/// Walk up from `start_dir` looking for [`MANIFEST_NAME`]; on a hit, return
/// the source root it declares (the manifest's directory joined with its
/// optional `src = "dir"` key). `None` if no manifest is found — the caller
/// falls back to the entry file's own directory.
pub fn find_src_root(start_dir: &FsPath) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let manifest = dir.join(MANIFEST_NAME);
        if manifest.is_file() {
            let src = fs::read_to_string(&manifest)
                .ok()
                .and_then(|text| parse_manifest_src(&text))
                .unwrap_or_default();
            return Some(if src.is_empty() { dir } else { dir.join(src) });
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The `src = "dir"` line of a manifest, if present. Quotes optional.
fn parse_manifest_src(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("src") {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Derive a file's module path segments from its location under `src_root`:
/// each directory is one segment, the basename (minus `.typl`) is the last,
/// all lowercased (the reader's own symbol normalization). A file outside
/// `src_root` falls back to just its basename — a single root-level segment —
/// so running a stray script still works.
pub fn module_segs_for(file: &FsPath, src_root: &FsPath) -> Result<Vec<String>, Error> {
    let rel = file.strip_prefix(src_root).unwrap_or_else(|_| FsPath::new(file.file_name().unwrap_or_default()));
    let mut segs = Vec::new();
    for comp in rel.components() {
        let s = comp.as_os_str().to_string_lossy().to_lowercase();
        segs.push(s);
    }
    if let Some(last) = segs.last_mut() {
        if let Some(stem) = last.strip_suffix(".typl") {
            *last = stem.to_string();
        }
    }
    if segs.iter().any(|s| s.is_empty()) || segs.is_empty() {
        return Err(Error::TypeError(format!(
            "cannot derive a module path from `{}` (empty path segment)",
            file.display()
        )));
    }
    Ok(segs)
}

/// Loads `.typl` files as modules into a shared `Heap`/`Checker`/`Interp`,
/// tracking what is loaded (never load a file twice) and what is *loading*
/// (to detect and report cycles). Checked top-level forms accumulate in a
/// pending queue the driver executes once all read roots are popped.
pub struct Loader {
    src_root: PathBuf,
    /// Modules currently being loaded, outermost first — the cycle-report chain.
    loading: Vec<String>,
    loaded: HashSet<Vec<String>>,
    /// Filesystem paths of every file successfully loaded as a dependency
    /// (not the entry file itself — see [`Loader::loaded_files`]). Lets a
    /// caller with several files open (the LSP) know which other open
    /// documents an entry's diagnostics actually depend on, so editing one
    /// of them can trigger re-diagnosing the ones that loaded it.
    loaded_files: HashSet<PathBuf>,
    pending: Vec<TopLevelForm>,
    /// In-editor buffer contents, keyed by filesystem path, consulted before
    /// disk for every dependency load — an unsaved edit to a file another
    /// file `use`s should be visible immediately, not only after a save.
    /// Empty (the default from [`Loader::new`]) for drivers with no editor
    /// buffers (`typl`'s file/REPL modes), which always read disk.
    overlay: HashMap<PathBuf, String>,
}

impl Loader {
    pub fn new(src_root: PathBuf) -> Loader {
        Loader {
            src_root,
            loading: Vec::new(),
            loaded: HashSet::new(),
            loaded_files: HashSet::new(),
            pending: Vec::new(),
            overlay: HashMap::new(),
        }
    }

    /// Supply in-editor buffer contents to consult before disk. See the
    /// `overlay` field's doc comment.
    pub fn set_overlay(&mut self, overlay: HashMap<PathBuf, String>) {
        self.overlay = overlay;
    }

    /// The filesystem paths of every dependency file this load pulled in
    /// (excludes the entry file itself, which the caller already knows the
    /// path of). See the `loaded_files` field's doc comment.
    pub fn loaded_files(&self) -> &HashSet<PathBuf> {
        &self.loaded_files
    }

    /// The checked-but-unexecuted top-level forms accumulated by loads so
    /// far, in dependency order. The driver must execute these (only) after
    /// the heap's read roots are back to their pre-load mark.
    pub fn take_pending(&mut self) -> Vec<TopLevelForm> {
        std::mem::take(&mut self.pending)
    }

    /// Load the entry file itself — same pipeline as a dependency (its
    /// content is wrapped in the module path its location derives to), with
    /// the source read from disk.
    pub fn load_entry(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        file: &FsPath,
    ) -> Result<(), Error> {
        let src = self.read_source(file)?;
        self.load_entry_src(heap, reader, checker, interp, file, &src)
    }

    /// [`Loader::load_entry`] with the source text supplied by the caller —
    /// for the LSP, whose "entry" is an editor buffer that may be newer than
    /// the file on disk.
    pub fn load_entry_src(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        file: &FsPath,
        src: &str,
    ) -> Result<(), Error> {
        let segs = module_segs_for(file, &self.src_root)?;
        self.load_source(heap, reader, checker, interp, file, src, segs)
    }

    /// Scan `forms` for `use` dependencies and load them — the REPL's case,
    /// where there is no entry file. The REPL then checks and runs the forms
    /// themselves, in the root namespace.
    ///
    /// It passes one form at a time (it reads them one at a time), so a `use`
    /// is a dependency of what comes after it, exactly as in a file — see
    /// [`Loader::load_source_inner`]'s loop. The slice stays in the signature
    /// because nothing about the scan needs it to be a single form.
    pub fn load_uses_in(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        forms: &[Value],
    ) -> Result<(), Error> {
        // No enclosing file — a REPL batch has no directory to be a sibling
        // of, so `ensure_loaded`'s sibling-relative fallback never triggers.
        for v in forms {
            self.scan_form(heap, reader, checker, interp, *v, &[])?;
        }
        Ok(())
    }

    /// Read, scan, check, and queue one file's worth of source under module
    /// path `segs`. See the module doc comment for the root-stack discipline
    /// this follows.
    // `heap`/`reader`/`checker`/`interp` are the invariant loading context
    // threaded through every load step; `file`/`src`/`segs` identify the one
    // file. Bundling the context would carry several independent `&mut`
    // borrows in one struct and ripple through the loader for no clarity gain.
    #[allow(clippy::too_many_arguments)]
    fn load_source(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        file: &FsPath,
        src: &str,
        segs: Vec<String>,
    ) -> Result<(), Error> {
        self.loading.push(segs.join("::"));
        let result = self.load_source_inner(heap, reader, checker, interp, file, src, &segs);
        self.loading.pop();
        if result.is_ok() {
            self.loaded.insert(segs);
        }
        result
    }

    // Same invariant loading context as `load_source` — see its comment.
    #[allow(clippy::too_many_arguments)]
    fn load_source_inner(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        file: &FsPath,
        src: &str,
        segs: &[String],
    ) -> Result<(), Error> {
        let file_name = file.to_string_lossy();
        let mark = heap.root_count();
        // A file *is* a module, so its symbols belong to that module's table
        // (`segs` is the path `module_segs_for` derived). An explicit
        // `(module ...)` inside nests further, handled while reading.
        let ns = crate::mem::symbols::ns_of(segs);
        let file_dir = file.parent().unwrap_or_else(|| FsPath::new(".")).to_path_buf();
        let path = checker.enter_file_module(segs);
        // One form at a time: read, resolve that form's dependencies, check
        // it, and run it, before the next form is read at all
        // (`Reader::forms_within`). The file used to be read to the end first
        // and scanned for `use` in a pass of its own, which is what made
        // read-time evaluation impossible — the text was fixed before any of
        // it ran.
        //
        // Two things follow from the change, both deliberate:
        //
        // * A `use` is a dependency of the forms that come *after* it, not of
        //   the whole file. A file that names `m::f` above its own `(use m)`
        //   used to work because the scan had already loaded `m`; now it
        //   reports the unresolved name. Every convention already puts the
        //   `use`s at the top, and this is the reading a per-form loader can
        //   give.
        // * The scan now runs *inside* this file's module context rather than
        //   before `enter_file_module`. `ensure_loaded` already suspends the
        //   namespace context around a dependency load for exactly this case
        //   (a `use` surfaced mid-check by macro expansion), so a dependency
        //   still registers under its own path.
        let mut forms = reader.forms_within(&file_name, src, ns);
        let mut body = Vec::new();
        let mut check_err = None;
        loop {
            let read = {
                let hook = crate::read::DriverReadEval::new(checker, interp);
                forms.next_form_with(heap, Some(&hook))
            };
            let read = match read {
                Ok(Some(pair)) => pair,
                Ok(None) => break,
                Err(e) => {
                    check_err = Some(e);
                    break;
                }
            };
            let (v, loc) = read;
            // Dependencies of this form (recursive; each nested load pushes
            // and pops its own roots strictly above ours). `segs` is threaded
            // through as the enclosing file's own module path, so a `use` can
            // also resolve against a sibling file in the same directory — see
            // `ensure_loaded`'s doc comment.
            if let Err(e) = self.scan_form(heap, reader, checker, interp, v, segs) {
                check_err = Some(e);
                break;
            }
            // A top-level macro call may expand to `(use ...)` (or to a
            // `(module ...)` containing one) — a dependency the scan of the
            // raw form above cannot see. Expand here, scan each expansion so
            // its dependency files get loaded, and check the final expansion
            // directly (the checker then has nothing left to expand at top
            // level, so nothing expands twice).
            // Expansion *errors* are deliberately ignored: `check_form_at`
            // re-expands and reports them with proper location and recovery
            // handling.
            let mut v = v;
            while let Ok(Some(expanded)) = checker.try_expand_toplevel_macro(heap, &*interp, v) {
                heap.push_root(expanded); // popped by `pop_roots_to(mark)` below
                if let Err(e) = self.scan_form(heap, reader, checker, interp, expanded, segs) {
                    check_err = Some(e);
                    break;
                }
                v = expanded;
            }
            if check_err.is_some() {
                break;
            }
            match checker.check_form_at(heap, &*interp, v, Some(loc)) {
                // `(load ...)` loads inline, so subsequent forms see the
                // definitions — see `load_file_flat`.
                Ok(tl) if core::op_is(heap, tl, wk::LOAD) => {
                    let load_path = match load_path_of(heap, tl) {
                        Some(p) => p,
                        None => {
                            check_err = Some(Error::TypeError("load: expected a path string".into()));
                            break;
                        }
                    };
                    if let Err(e) = load_file_flat(heap, reader, checker, interp, &file_dir, &load_path) {
                        check_err = Some(e);
                        break;
                    }
                }
                Ok(tl) if needs_immediate_exec(heap, tl) => {
                    // Reported, not swallowed: a registration that has to
                    // happen *now* and silently did not is a file that reads
                    // differently from here on, with nothing said about why.
                    if let Err(e) = interp.exec(heap, tl) {
                        check_err = Some(Error::TypeError(e.to_string()));
                        break;
                    }
                }
                // Rooted as it lands. `body` is a `Vec<Value>`, which the
                // collector cannot see, and checking the *next* form of this
                // file allocates heavily — so an earlier member left unrooted
                // is collected and its cell recycled, and the module bundle
                // built below ends up holding a fragment of it. That shows up
                // at exec as `not a top-level core form: (())`, the same
                // signature (and the same fix) as `Checker::check_impl`'s own
                // `body` vector. Released by the `pop_roots_to(mark)` below,
                // once the bundle holding them is permanently rooted.
                Ok(tl) => {
                    heap.push_root(tl);
                    body.push(tl);
                }
                Err(e) => {
                    check_err = Some(e);
                    break;
                }
            }
        }
        checker.exit_file_module(segs.len());
        if let Some(e) = check_err {
            pop_roots_to(heap, mark);
            return Err(e);
        }
        // Every `defsignature` in this file promised a definition; this is
        // where the promise comes due. Checked before the bundle is built so
        // a file with an unmet declaration never becomes a loadable module.
        if let Err(e) = checker.finish_unit() {
            pop_roots_to(heap, mark);
            return Err(e);
        }
        // `(module PATH BODY...)` — the file's own definitions as one unit.
        // Built here rather than by the checker because the *driver* is what
        // knows a file's forms belong together.
        //
        // Still under the per-form roots pushed above. `tagged_module` conses
        // the bundle *around* `body`'s members, so in principle they have to
        // survive a collection triggered by that consing too — a narrower
        // window than the one above, and one `tests/loader_gc_test.rs` does
        // not currently provoke (the bundle is N+1 cells against a whole
        // file's checking). Closing it costs one moved line, so it is closed
        // rather than argued about.
        let wrapper = match crate::check::core::tagged_module(heap, &path, &body) {
            Ok(v) => v,
            Err(e) => {
                pop_roots_to(heap, mark);
                return Err(Error::TypeError(format!("load: {}", e)));
            }
        };
        heap.push_permanent_root(wrapper);
        // Only now: the bundle keeps every member reachable, so this file's
        // read roots and per-form roots can all go — restoring the LIFO
        // discipline this function's callers rely on (see the module doc).
        pop_roots_to(heap, mark);
        self.pending.push(wrapper);
        Ok(())
    }

    /// If `v` is a `(use path)` form, load the file its path maps to (no-op
    /// when no file exists or it is already loaded — the checker's own
    /// `check_use` then resolves or reports as usual). Recurses into
    /// `(module name body...)` bodies, whose `use`s are equally top-level
    /// declarations. `cur_segs` is the enclosing *file's* own module path
    /// (unchanged by a nested `(module ...)` body's recursion — see
    /// `ensure_loaded`'s doc comment), empty for a REPL batch with no file.
    fn scan_form(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        v: Value,
        cur_segs: &[String],
    ) -> Result<(), Error> {
        if !matches!(v, Value::Cons(_)) {
            return Ok(());
        }
        let head = heap.car(v)?;
        let name = match head {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Ok(()),
        };
        match name.as_str() {
            // Every spelling of an import, and *every* path in one: the forms
            // take a list (`(use a::f b::g)`), and a path whose file has not
            // been loaded resolves for no other reason than this scan.
            "use" | "import" | "shadowing-import" => {
                let mut rest = heap.cdr(v)?;
                while let Value::Cons(_) = rest {
                    let arg = heap.car(rest)?;
                    if let Some(segs) = value_path_segs(heap, arg) {
                        self.ensure_loaded(heap, reader, checker, interp, &segs, cur_segs)?;
                    }
                    rest = heap.cdr(rest)?;
                }
                Ok(())
            }
            // `(in-module path)` names a module to *enter*, not one to load:
            // it nests inside this file, so there is no file behind it.
            "in-module" => Ok(()),
            "module" => {
                // Skip the path argument, scan the body forms.
                let mut rest = heap.cdr(v)?;
                if let Value::Cons(_) = rest {
                    rest = heap.cdr(rest)?;
                    while let Value::Cons(_) = rest {
                        let form = heap.car(rest)?;
                        self.scan_form(heap, reader, checker, interp, form, cur_segs)?;
                        rest = heap.cdr(rest)?;
                    }
                }
                Ok(())
            }
            _ => {
                // A macro call encountered while scanning (inside a nested
                // `(module ...)` body, or a macro→macro chain link from
                // `load_source_inner`'s pre-expansion) may itself expand to
                // `(use ...)`: expand, scan the expansion, and discard it —
                // the checker re-expands when it checks this form. Only
                // macros already registered at this point resolve (the
                // pre-check scan runs before any of this file's own
                // `defmacro`s are checked; `load_source_inner`'s per-form
                // pre-expansion covers those). Expansion failures are
                // ignored: the checker reports them with proper location
                // and recovery handling.
                if let Ok(Some(expanded)) = checker.try_expand_toplevel_macro(heap, &*interp, v) {
                    heap.push_root(expanded);
                    let result = self.scan_form(heap, reader, checker, interp, expanded, cur_segs);
                    heap.pop_root();
                    return result;
                }
                Ok(())
            }
        }
    }

    /// Map a `use` path to a file and load it if it exists and hasn't been.
    /// `use a::b::c` may name module `a::b::c` itself or item `c` of module
    /// `a::b`, so candidate files are tried longest-prefix-first:
    /// `a/b/c.typl`, then `a/b.typl`, then `a.typl`. For each prefix length,
    /// a root-relative candidate (`<src-root>/a/b/c.typl`) is tried first —
    /// unchanged from before, so an existing root-relative `use` is never
    /// shadowed — and only if that doesn't exist, a *sibling-relative*
    /// candidate (`<src-root>/<cur_segs' directory>/a/b/c.typl`) is tried, so
    /// `(use vector)` inside `geo/point.typl` can also reach a sibling
    /// `geo/vector.typl` without spelling out `(use geo::vector)`. Mirrored
    /// on the checker side by `Checker::find_module`'s own sibling tier
    /// (`file_ns`), which resolves the *name* `(use vector)` binds once this
    /// has loaded the file under its true root-derived path (`geo::vector`,
    /// never a synthetic relative one).
    fn ensure_loaded(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        use_segs: &[String],
        cur_segs: &[String],
    ) -> Result<(), Error> {
        for n in (1..=use_segs.len()).rev() {
            let module_segs: Vec<String> = use_segs[..n].to_vec();
            if self.try_load_module(heap, reader, checker, interp, &module_segs)? {
                return Ok(());
            }
            if cur_segs.len() > 1 {
                let mut sib_segs = cur_segs[..cur_segs.len() - 1].to_vec();
                sib_segs.extend_from_slice(&module_segs);
                if self.try_load_module(heap, reader, checker, interp, &sib_segs)? {
                    return Ok(());
                }
            }
        }
        Ok(()) // no file — leave resolution (or its failure) to `check_use`
    }

    /// Loads `module_segs` as a root-relative file (`ensure_loaded`'s one
    /// candidate-path shape, tried for both the root-relative and
    /// sibling-relative cases). Returns `true` if `module_segs` names a real
    /// file — whether it was freshly loaded here or already loaded/loading
    /// before — so the caller stops trying shorter/sibling candidates;
    /// `false` if no such file exists, so the caller should keep trying.
    fn try_load_module(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        module_segs: &[String],
    ) -> Result<bool, Error> {
        let mut file = self.src_root.clone();
        for s in module_segs {
            file.push(s);
        }
        file.set_extension("typl");
        // An overlay-only file (open in the editor, not yet saved) is a
        // valid dependency too — checking the overlay alongside disk keeps a
        // brand-new not-yet-saved file discoverable.
        if !file.is_file() && !self.overlay.contains_key(&file) {
            return Ok(false);
        }
        if self.loaded.contains(module_segs) {
            return Ok(true);
        }
        let display = module_segs.join("::");
        if let Some(pos) = self.loading.iter().position(|m| *m == display) {
            let mut chain: Vec<&str> = self.loading[pos..].iter().map(String::as_str).collect();
            chain.push(&display);
            return Err(Error::TypeError(format!(
                "circular module dependency: {}",
                chain.join(" -> ")
            )));
        }

        let src = self.read_source(&file)?;
        self.loaded_files.insert(file.clone());
        // A dependency file's own module must never nest under whichever
        // module the checker is currently inside: during the normal
        // pre-check scan the context is already clean (making the suspend a
        // no-op), but a load triggered mid-check — a `use` surfaced by macro
        // expansion, see `load_source_inner`'s check loop — runs inside the
        // requesting file's module context.
        let saved = checker.suspend_ns_context();
        let result = self.load_source(heap, reader, checker, interp, &file, &src, module_segs.to_vec());
        checker.resume_ns_context(saved);
        result?;
        Ok(true)
    }

    /// `file`'s content: the overlay's copy if one exists (an open, possibly
    /// unsaved editor buffer), otherwise disk.
    fn read_source(&self, file: &FsPath) -> Result<String, Error> {
        if let Some(src) = self.overlay.get(file) {
            return Ok(src.clone());
        }
        fs::read_to_string(file).map_err(|e| Error::TypeError(format!("cannot read `{}`: {}", file.display(), e)))
    }
}

fn pop_roots_to(heap: &mut Heap, mark: usize) {
    while heap.root_count() > mark {
        heap.pop_root();
    }
}

/// The CL-style `(load "path")` mechanism (`TopLevel::Load`): loads `path`'s
/// definitions into the *current* environment (root namespace, no module
/// wrap).
///
/// Resolution:
/// - `path` is taken relative to `dir` (the loading file's directory, or the
///   process cwd for a REPL `(load)`), with `.typl` appended if it has no
///   extension.
/// - The `.typl` source is read, checked form-by-form into the same
///   `checker`/`interp`, and each form `exec`d (so later forms — here or in
///   the caller — see the definitions).
///
/// A `load`ed file's own `(load ...)`/`(use ...)` are honored recursively
/// (the recursive `load_file_flat` / this `Loader`'s scan).
pub fn load_file_flat(
    heap: &mut Heap,
    reader: &Reader,
    checker: &mut Checker,
    interp: &mut Interp,
    dir: &FsPath,
    path: &str,
) -> Result<(), Error> {
    let raw = FsPath::new(path);
    let base = if raw.is_absolute() { raw.to_path_buf() } else { dir.join(raw) };
    let typl = if base.extension().is_some() { base.clone() } else { base.with_extension("typl") };

    let src = fs::read_to_string(&typl)
        .map_err(|e| Error::TypeError(format!("load: cannot read `{}`: {}", typl.display(), e)))?;
    load_source_flat(heap, reader, checker, interp, &typl, &src)
}

/// Reads, checks, and execs `src`'s forms into the current environment (root
/// namespace) — the body of [`load_file_flat`], split out so its path
/// resolution stays readable. A nested `(load ...)` resolves relative to
/// `file`'s own directory.
fn load_source_flat(
    heap: &mut Heap,
    reader: &Reader,
    checker: &mut Checker,
    interp: &mut Interp,
    file: &FsPath,
    src: &str,
) -> Result<(), Error> {
    let file_name = file.to_string_lossy();
    let mark = heap.root_count();
    let dir = file.parent().unwrap_or_else(|| FsPath::new(".")).to_path_buf();
    // Read, check and run one form before the next is read — see
    // `load_source_inner`'s loop for why the read is interleaved.
    let mut forms = reader.forms_in(&file_name, src);
    let mut result = Ok(());
    loop {
        let next = {
            let hook = crate::read::DriverReadEval::new(checker, interp);
            forms.next_form_with(heap, Some(&hook))
        };
        let (v, loc) = match next {
            Ok(Some(pair)) => pair,
            Ok(None) => break,
            Err(e) => {
                result = Err(e);
                break;
            }
        };
        match checker.check_form_at(heap, &*interp, v, Some(loc)) {
            Ok(tl) if core::op_is(heap, tl, wk::LOAD) => {
                let Some(path) = load_path_of(heap, tl) else {
                    result = Err(Error::TypeError("load: expected a path string".into()));
                    break;
                };
                if let Err(e) = load_file_flat(heap, reader, checker, interp, &dir, &path) {
                    result = Err(e);
                    break;
                }
            }
            Ok(tl) => {
                if let Err(e) = interp.exec(heap, tl) {
                    result = Err(Error::TypeError(format!("load: exec failed: {}", e)));
                    break;
                }
            }
            Err(e) => {
                result = Err(e);
                break;
            }
        }
    }
    pop_roots_to(heap, mark);
    result.and_then(|()| checker.finish_unit())
}

/// The segments of a `use` argument: a bare symbol is one segment, a
/// `::`-path is its segment list. Anything else (a malformed `use`) is left
/// for `check_use` to report.
fn value_path_segs(heap: &Heap, v: Value) -> Option<Vec<String>> {
    match v {
        Value::Symbol(id) => Some(vec![heap.symbol_name(id).to_string()]),
        Value::Path(id) => {
            Some(heap.path_segments(id).iter().map(|s| heap.symbol_name(*s).to_string()).collect())
        }
        _ => None,
    }
}

/// True for a `Defmacro` — or a checker-synthesized monomorphization bundle
/// (a `Module` at [`MONO_BUNDLE_MODULE`]) containing one: a macro whose body
/// calls a generic function comes back wrapped alongside the specializations
/// that call needs. Everything inside is a pure registration
/// (`Defun`/`Defmacro`), so executing it early keeps the "exec never touches
/// the root stack mid-batch" invariant this module and `main.rs` rely on. A
/// user-written `module` is deliberately *not* matched — executing one early
/// would run arbitrary body expressions out of order.
pub fn needs_immediate_exec(heap: &Heap, tl: TopLevelForm) -> bool {
    match core::op_sym(heap, tl).map(|s| s.well_known()) {
        Some(wk::DEFMACRO) => true,
        // A readtable registration, which has to take effect before the
        // *next form is read* — earlier still than a `defmacro`, which only
        // has to beat the next form's check.
        //
        // What it registers has to exist *now*, which a `defun` in the same
        // file does not: that one is collected into the module's bundle and
        // runs when the bundle does. So the function is written as a
        // `lambda`, or comes from the prelude. See `docs/ja/reference/syntax.md` §11.
        Some(wk::EXPR) => core::field(heap, tl, 0).map(|e| is_readtable_registration(heap, e)).unwrap_or(false),
        Some(wk::MODULE) => {
            let bundle = match core::field(heap, tl, 0) {
                Some(Value::Path(id)) => crate::types::path_from_id(heap, id) == Path::root(MONO_BUNDLE_MODULE),
                Some(Value::Symbol(id)) => id.is(wk::MONO_BUNDLE),
                _ => false,
            };
            bundle
                && core::fields(heap, tl)
                    .map(|fs| fs.iter().skip(1).any(|f| needs_immediate_exec(heap, *f)))
                    .unwrap_or(false)
        }
        _ => false,
    }
}

/// Whether `form` is a call to one of the two builtins that install a reader
/// macro.
///
/// By callee, because it is an ordinary call and not a definition: field 2 of
/// a `call` node is the resolved path (`core_eval::call_core`). Only a call
/// *directly* at top level counts — one buried in a `progn` or a `let` would
/// have to be evaluated to be found, which is the thing being decided.
fn is_readtable_registration(heap: &Heap, form: Value) -> bool {
    core::op_sym(heap, form).map(|s| s.well_known()) == Some(wk::CALL)
        && matches!(
            core::path_field(heap, form, 2).as_ref().map(|p| p.to_string()).as_deref(),
            Some("set-macro-character") | Some("set-dispatch-macro-character")
        )
}

/// The path string a `(load "PATH")` form names, or `None` for any other form.
///
/// The tag check is part of the answer, not a precondition: every driver
/// (`main.rs`'s two, this module's two) has a top-level form in hand and wants
/// to know "is this a load, and of what?" as one question.
pub fn load_path_of(heap: &Heap, tl: TopLevelForm) -> Option<String> {
    if !core::op_is(heap, tl, wk::LOAD) {
        return None;
    }
    match core::field(heap, tl, 0)? {
        Value::Str(id) => Some(heap.string(id).to_string()),
        _ => None,
    }
}
