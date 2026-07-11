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
//! checker's signal for a `use` no scan could see (e.g. one produced by a
//! macro expansion) — such a `use` reports "unresolved" rather than loading.
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
//! `read_all_in` roots are popped as soon as that file's forms are checked —
//! nested loads are strictly LIFO above the outer file's roots, so stack
//! order holds. Checked forms are queued (dependency-first, since a
//! dependency's load completes before the dependent's check resumes) and
//! executed by the driver only after every root is popped; the one exception
//! is `defmacro` registration, which must precede later macro *uses* in the
//! same load and is safe mid-load because it never touches the root stack.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path as FsPath, PathBuf};

use crate::{Checker, Error, Heap, Interp, Path, Reader, TopLevel, Value, MONO_BUNDLE_MODULE};

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
    pending: Vec<TopLevel>,
    /// In-editor buffer contents, keyed by filesystem path, consulted before
    /// disk for every dependency load — an unsaved edit to a file another
    /// file `use`s should be visible immediately, not only after a save.
    /// Empty (the default from [`Loader::new`]) for drivers with no editor
    /// buffers (`typl`'s file/REPL modes), which always read disk.
    overlay: HashMap<PathBuf, String>,
}

impl Loader {
    pub fn new(src_root: PathBuf) -> Loader {
        Loader { src_root, loading: Vec::new(), loaded: HashSet::new(), pending: Vec::new(), overlay: HashMap::new() }
    }

    /// Supply in-editor buffer contents to consult before disk. See the
    /// `overlay` field's doc comment.
    pub fn set_overlay(&mut self, overlay: HashMap<PathBuf, String>) {
        self.overlay = overlay;
    }

    /// The checked-but-unexecuted top-level forms accumulated by loads so
    /// far, in dependency order. The driver must execute these (only) after
    /// the heap's read roots are back to their pre-load mark.
    pub fn take_pending(&mut self) -> Vec<TopLevel> {
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
        // A fresh read session: wipe the cons-location table once, here.
        // Every read below (this file and, recursively, its dependencies)
        // must *keep* locations — see `Reader::read_all_in_keep_locs` — or a
        // dependency's read would erase the locations of outer-file forms
        // that are still waiting to be checked.
        heap.clear_cons_locs();
        self.load_source(heap, reader, checker, interp, file, src, segs)
    }

    /// Scan `forms` (a batch read from stdin — the REPL's case, where no
    /// entry file exists) for `use` dependencies and load them. The REPL then
    /// checks the batch itself as today, in the root namespace.
    pub fn load_uses_in(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        forms: &[Value],
    ) -> Result<(), Error> {
        for v in forms {
            self.scan_form(heap, reader, checker, interp, *v)?;
        }
        Ok(())
    }

    /// Read, scan, check, and queue one file's worth of source under module
    /// path `segs`. See the module doc comment for the root-stack discipline
    /// this follows.
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
        let forms = match reader.read_all_in_keep_locs(heap, &file_name, src) {
            Ok(forms) => forms,
            Err(e) => {
                pop_roots_to(heap, mark);
                return Err(e);
            }
        };

        // Load dependencies first (recursive; each nested load pushes and
        // pops its own roots strictly above ours).
        for v in &forms {
            if let Err(e) = self.scan_form(heap, reader, checker, interp, *v) {
                pop_roots_to(heap, mark);
                return Err(e);
            }
        }

        let path = checker.enter_module(segs);
        let mut body = Vec::new();
        let mut check_err = None;
        for v in forms {
            match checker.check_form(heap, &*interp, v) {
                Ok(tl) if needs_immediate_exec(&tl) => {
                    let _ = interp.exec(heap, tl);
                }
                Ok(tl) => body.push(tl),
                Err(e) => {
                    check_err = Some(e);
                    break;
                }
            }
        }
        checker.exit_module(segs.len());
        pop_roots_to(heap, mark);
        if let Some(e) = check_err {
            return Err(e);
        }
        self.pending.push(TopLevel::Module { path, body });
        Ok(())
    }

    /// If `v` is a `(use path)` form, load the file its path maps to (no-op
    /// when no file exists or it is already loaded — the checker's own
    /// `check_use` then resolves or reports as usual). Recurses into
    /// `(module name body...)` bodies, whose `use`s are equally top-level
    /// declarations.
    fn scan_form(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        v: Value,
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
            "use" => {
                let rest = heap.cdr(v)?;
                if let Value::Cons(_) = rest {
                    let arg = heap.car(rest)?;
                    if let Some(segs) = value_path_segs(heap, arg) {
                        self.ensure_loaded(heap, reader, checker, interp, &segs)?;
                    }
                }
                Ok(())
            }
            "module" => {
                // Skip the path argument, scan the body forms.
                let mut rest = heap.cdr(v)?;
                if let Value::Cons(_) = rest {
                    rest = heap.cdr(rest)?;
                    while let Value::Cons(_) = rest {
                        let form = heap.car(rest)?;
                        self.scan_form(heap, reader, checker, interp, form)?;
                        rest = heap.cdr(rest)?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Map a `use` path to a file and load it if it exists and hasn't been.
    /// `use a::b::c` may name module `a::b::c` itself or item `c` of module
    /// `a::b`, so candidate files are tried longest-prefix-first:
    /// `a/b/c.typl`, then `a/b.typl`, then `a.typl`.
    fn ensure_loaded(
        &mut self,
        heap: &mut Heap,
        reader: &Reader,
        checker: &mut Checker,
        interp: &mut Interp,
        use_segs: &[String],
    ) -> Result<(), Error> {
        for n in (1..=use_segs.len()).rev() {
            let module_segs: Vec<String> = use_segs[..n].to_vec();
            let mut file = self.src_root.clone();
            for s in &module_segs {
                file.push(s);
            }
            file.set_extension("typl");
            if !file.is_file() {
                continue;
            }
            if self.loaded.contains(&module_segs) {
                return Ok(());
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
            return self.load_source(heap, reader, checker, interp, &file, &src, module_segs);
        }
        Ok(()) // no file — leave resolution (or its failure) to `check_use`
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
pub fn needs_immediate_exec(tl: &TopLevel) -> bool {
    match tl {
        TopLevel::Defmacro { .. } => true,
        TopLevel::Module { path, body } if *path == Path::root(MONO_BUNDLE_MODULE) => {
            body.iter().any(needs_immediate_exec)
        }
        _ => false,
    }
}
