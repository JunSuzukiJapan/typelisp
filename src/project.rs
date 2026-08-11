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
//! `read_all_in` roots are popped as soon as that file's forms are checked —
//! nested loads are strictly LIFO above the outer file's roots, so stack
//! order holds. Checked forms are queued (dependency-first, since a
//! dependency's load completes before the dependent's check resumes) and
//! executed by the driver only after every root is popped; the one exception
//! is `defmacro` registration, which must precede later macro *uses* in the
//! same load and is safe mid-load because it never touches the root stack.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path as FsPath, PathBuf};
use std::rc::Rc;

use crate::fasl::{registry_mark, source_hash, Fasl, RegistryMark};
use crate::check::core;
use crate::{Checker, Error, Heap, Interp, Path, Reader, TopLevelForm, Value, MONO_BUNDLE_MODULE};

/// A cached fasl for one dependency file, keyed by its path in
/// [`ModuleCache`]. Shared (via `Rc`) across `Loader` instances so a caller
/// with a long-lived session (the LSP) can keep reusing it across many
/// separate `diagnostics_for` passes, each of which builds a fresh `Loader`.
///
/// Staleness isn't just this file's own [`source_hash`]: `fasl` was captured
/// with this file's dependencies already baked into its registry delta (see
/// [`Loader::try_load_module`]'s doc comment), so every file the load
/// transitively touched must still hash the same, or the cache entry is
/// treated as stale (falls back to a real load, which re-populates the
/// cache). `deps` always includes this file itself as its first entry.
#[derive(Clone)]
pub struct ModuleCacheEntry {
    /// `(file path, its module path segments, its content hash at capture
    /// time)` for this file and every dependency its load transitively
    /// touched — the segments let a cache *hit* replay
    /// `Loader::loaded`/`Loader::loaded_files` bookkeeping without re-reading
    /// each file's directory structure.
    pub deps: Vec<(PathBuf, Vec<String>, u64)>,
    pub fasl: Rc<Fasl>,
}

/// A [`Loader`]-external cache of already-checked dependency modules, shared
/// across many separate load sessions (see [`Loader::set_module_cache`]).
pub type ModuleCache = Rc<RefCell<HashMap<PathBuf, ModuleCacheEntry>>>;

/// [`Loader::load_source_inner`]'s result: everything [`Loader::try_load_module`]
/// needs to decide whether (and how) to populate the module cache.
struct LoadOutcome {
    /// Whether this load is safe to capture into the module cache: `false`
    /// when it contained a nested `(load ...)` — `load_file_flat` applies it
    /// inline (its definitions land in the registry `checker` already
    /// mutated, but never get their own `TopLevel::Module` entry pushed to
    /// `Loader::pending`), so replaying only `Loader::pending`'s growth on a
    /// cache hit (see `Loader::try_load_module`'s cache-store branch) would
    /// silently miss the loaded file's function bodies at `exec` time — a
    /// checker-registry-only load without a matching interpreter-side replay.
    /// Rare in practice; a `false` here just means this file's
    /// `try_load_module` call falls back to an ordinary (uncached) load every
    /// time, same as before this cache existed.
    cacheable: bool,
}

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
    /// An external cache of already-checked *dependency* files (never the
    /// entry file — see [`Loader::try_load_module`]'s doc comment), consulted
    /// by every `use` resolution. `None` (the default from [`Loader::new`])
    /// for drivers with no reason to skip re-checking (`typl`'s file/REPL/
    /// `compile-module` modes, each a one-shot process) — only the LSP, whose
    /// `Loader` is rebuilt fresh for every `diagnostics_for` pass, opts in via
    /// [`Loader::set_module_cache`].
    module_cache: Option<ModuleCache>,
    /// `(cache hits, cache misses)` this `Loader` served — see
    /// [`Loader::cache_stats`]. Both stay `0` with no `module_cache` set.
    cache_hits: usize,
    cache_misses: usize,
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
            module_cache: None,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// `(cache hits, cache misses)` served by this `Loader`'s `use`
    /// resolutions so far — a miss counts a dependency that either found no
    /// cache entry or found a stale one; both are `0` with no
    /// [`Loader::set_module_cache`] call. For a caller (the LSP) to log
    /// effectiveness, or a test to observe that a second load actually
    /// skipped re-checking rather than merely producing the same result.
    pub fn cache_stats(&self) -> (usize, usize) {
        (self.cache_hits, self.cache_misses)
    }

    /// Supply a cache of already-checked dependency modules, consulted (and
    /// populated) by every `use` resolution this `Loader` performs — see
    /// [`ModuleCacheEntry`]'s doc comment for what makes an entry stale.
    pub fn set_module_cache(&mut self, cache: ModuleCache) {
        self.module_cache = Some(cache);
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
        // The entry file is never itself a cache candidate (only its
        // dependencies, loaded through `try_load_module`, are) — discard the
        // `LoadOutcome`.
        self.load_source(heap, reader, checker, interp, file, src, segs).map(|_| ())
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
    ) -> Result<LoadOutcome, Error> {
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
    ) -> Result<LoadOutcome, Error> {
        let file_name = file.to_string_lossy();
        let mark = heap.root_count();
        let forms = match reader.read_all_in_spanned(heap, &file_name, src) {
            Ok(forms) => forms,
            Err(e) => {
                pop_roots_to(heap, mark);
                return Err(e);
            }
        };

        // Load dependencies first (recursive; each nested load pushes and
        // pops its own roots strictly above ours). `segs` is threaded
        // through as the enclosing file's own module path, so a `use` can
        // also resolve against a sibling file in the same directory — see
        // `ensure_loaded`'s doc comment.
        for (v, _) in &forms {
            if let Err(e) = self.scan_form(heap, reader, checker, interp, *v, segs) {
                pop_roots_to(heap, mark);
                return Err(e);
            }
        }
        // Any dependency load from here on (a `(load ...)` form, or a `use`
        // surfaced by macro expansion mid-check) is the harder-to-reproduce
        // case a fasl capture below deliberately opts out of — see
        // `LoadOutcome::cacheable`'s doc comment.
        let loaded_files_before_body = self.loaded_files.len();
        let mut cacheable = true;

        let file_dir = file.parent().unwrap_or_else(|| FsPath::new(".")).to_path_buf();
        let path = checker.enter_file_module(segs);
        checker.predeclare_program(heap, &forms.iter().map(|(v, _)| *v).collect::<Vec<_>>());
        let mut body = Vec::new();
        let mut check_err = None;
        for (v, loc) in forms {
            // A top-level macro call may expand to `(use ...)` (or to a
            // `(module ...)` containing one) — a dependency the pre-check
            // scan above cannot see, since the macro only got registered by
            // checking an earlier form of this very file. Expand here, scan
            // each expansion so its dependency files get loaded, and check
            // the final expansion directly (the checker then has nothing
            // left to expand at top level, so nothing expands twice).
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
                // `(load ...)` loads inline (so subsequent forms see the
                // definitions), preferring a compiled fasl — see
                // `load_file_flat`.
                Ok(tl) if core::op(heap, tl) == Some("load") => {
                    cacheable = false;
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
                    let _ = interp.exec(heap, tl);
                }
                Ok(tl) => body.push(tl),
                Err(e) => {
                    check_err = Some(e);
                    break;
                }
            }
        }
        checker.exit_file_module(segs.len());
        pop_roots_to(heap, mark);
        if let Some(e) = check_err {
            return Err(e);
        }
        if self.loaded_files.len() != loaded_files_before_body {
            cacheable = false;
        }
        // `(module PATH BODY...)` — the file's own definitions as one unit.
        // Built here rather than by the checker because the *driver* is what
        // knows a file's forms belong together.
        let wrapper = match crate::check::core::tagged_module(heap, &path, &body) {
            Ok(v) => v,
            Err(e) => return Err(Error::TypeError(format!("load: {}", e))),
        };
        heap.push_permanent_root(wrapper);
        self.pending.push(wrapper);
        Ok(LoadOutcome { cacheable })
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
            "use" => {
                let rest = heap.cdr(v)?;
                if let Value::Cons(_) = rest {
                    let arg = heap.car(rest)?;
                    if let Some(segs) = value_path_segs(heap, arg) {
                        self.ensure_loaded(heap, reader, checker, interp, &segs, cur_segs)?;
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
    ///
    /// When [`Loader::module_cache`] is set, a fresh cache entry for `file`
    /// (see [`ModuleCacheEntry`]) is applied via [`Fasl::load_into`] instead
    /// of reading/checking — skipping parse and type-check entirely, the
    /// same win `prelude::load_cached` gets from its own fasl (see
    /// [[typelisp-fasl-compiled-modules]]). A capture on a cache *miss*
    /// covers this file and every dependency its own load transitively
    /// touched (a fasl's registry delta is a name-diff against a mark taken
    /// before any of them loaded, and `diff_namespace` walks the whole
    /// namespace tree — see `fasl.rs`'s module doc comment) — so the cached
    /// entry's own staleness check must re-verify all of them, not just
    /// `file` itself, tracked via `ModuleCacheEntry::deps`.
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

        let cache = self.module_cache.clone();
        if let Some(cache) = &cache {
            let hit = cache.borrow().get(&file).cloned();
            if let Some(entry) = hit {
                if self.cache_entry_is_fresh(&entry.deps) {
                    entry.fasl.load_into(heap, checker, interp).map_err(|e| {
                        Error::TypeError(format!("module cache: `{}`: {}", file.display(), e))
                    })?;
                    for (p, segs, _) in &entry.deps {
                        self.loaded.insert(segs.clone());
                        self.loaded_files.insert(p.clone());
                    }
                    self.cache_hits += 1;
                    return Ok(true);
                }
                cache.borrow_mut().remove(&file);
            }
            self.cache_misses += 1;
        }

        let src = self.read_source(&file)?;
        let files_before = self.loaded_files.clone();
        self.loaded_files.insert(file.clone());
        // The exact `TopLevel::Module` entries this call (and every nested
        // dependency it recursively loads) pushes to `self.pending` —
        // captured as-is into the fasl below, so a later cache *hit* replays
        // precisely what a live load would have queued for the driver to
        // `exec`, dependency-first order included (see
        // `Loader::capture_into_cache`'s doc comment on why this — not just
        // this file's own top-levels — is what must be captured).
        let pending_before = self.pending.len();
        // A dependency file's own module must never nest under whichever
        // module the checker is currently inside: during the normal
        // pre-check scan the context is already clean (making the suspend a
        // no-op), but a load triggered mid-check — a `use` surfaced by macro
        // expansion, see `load_source_inner`'s check loop — runs inside the
        // requesting file's module context.
        let saved = checker.suspend_ns_context();
        let mark = cache.as_ref().map(|_| registry_mark(checker));
        let result = self.load_source(heap, reader, checker, interp, &file, &src, module_segs.to_vec());
        checker.resume_ns_context(saved);
        let outcome = result?;

        if let (Some(cache), Some(mark)) = (&cache, &mark) {
            if outcome.cacheable {
                let top_levels = self.pending[pending_before..].to_vec();
                if let Err(e) = self.capture_into_cache(heap, checker, mark, &file, top_levels, &files_before, cache) {
                    // A capture failure just means this file won't be cached
                    // this round (it already loaded fine live) — not worth
                    // failing the whole load over.
                    let _ = e;
                }
            }
        }
        Ok(true)
    }

    /// Builds a [`ModuleCacheEntry`] for the file just loaded at `file` and
    /// inserts it into `cache` — the store half of [`Loader::try_load_module`]'s
    /// cache-miss path, split out so its early-return error handling doesn't
    /// clutter the caller.
    ///
    /// `top_levels` — the newly-grown tail of `self.pending` — is exactly
    /// what a live load queued for the driver, **not** just this file's own
    /// `TopLevel::Module`: a `use`d dependency reached during this call gets
    /// its *own* `TopLevel::Module` entry pushed by its own (nested)
    /// `try_load_module` call, entirely separate from this file's. Capturing
    /// only this file's own forms would produce a fasl whose registry delta
    /// (via `Fasl::capture`'s `diff_namespace`, which walks the whole
    /// namespace tree) still *type-checks* calls into that dependency —
    /// `checker`'s state has it registered — but whose replayed `exec` never
    /// re-registers the dependency's function bodies with the interpreter,
    /// surfacing as a `NoSuchFunction` at call time instead: a strictly
    /// worse failure mode (works until someone actually calls it) that a
    /// dedicated regression test guards against.
    #[allow(clippy::too_many_arguments)]
    fn capture_into_cache(
        &self,
        heap: &Heap,
        checker: &Checker,
        mark: &RegistryMark,
        file: &FsPath,
        top_levels: Vec<TopLevelForm>,
        files_before: &HashSet<PathBuf>,
        cache: &ModuleCache,
    ) -> Result<(), Error> {
        let mut deps = Vec::new();
        for p in self.loaded_files.difference(files_before) {
            let segs = module_segs_for(p, &self.src_root)?;
            let hash = source_hash(&self.read_source(p)?);
            deps.push((p.clone(), segs, hash));
        }
        let this_hash = deps
            .iter()
            .find(|(p, ..)| p == file)
            .map(|(_, _, h)| *h)
            .unwrap_or_else(|| source_hash(""));
        let fasl = Fasl::capture(heap, checker, mark, top_levels, this_hash)?;
        cache.borrow_mut().insert(file.to_path_buf(), ModuleCacheEntry { deps, fasl: Rc::new(fasl) });
        Ok(())
    }

    /// Whether every file in a [`ModuleCacheEntry::deps`] list still hashes
    /// the same as when it was captured — re-reads each (respecting the
    /// overlay, same as a live load) but never re-parses/re-checks.
    fn cache_entry_is_fresh(&self, deps: &[(PathBuf, Vec<String>, u64)]) -> bool {
        deps.iter().all(|(p, _, h)| self.read_source(p).map(|s| source_hash(&s) == *h).unwrap_or(false))
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

/// The extension a compiled (fasl) module file carries on disk — see
/// [`load_file_flat`].
pub const FASL_EXTENSION: &str = "fastl";

/// The CL-style `(load "path")` mechanism (`TopLevel::Load`): loads `path`'s
/// definitions into the *current* environment (root namespace, no module
/// wrap), preferring a compiled `.fasl` over source.
///
/// Resolution:
/// - `path` is taken relative to `dir` (the loading file's directory, or the
///   process cwd for a REPL `(load)`), with `.typl` appended if it has no
///   extension.
/// - If a sibling `<stem>.fasl` exists and its `source_hash` matches the
///   `.typl`'s current bytes (or the `.typl` is absent), the fasl is loaded
///   via [`Fasl::load_into`] — no read/typecheck.
/// - Otherwise the `.typl` source is read, checked form-by-form into the same
///   `checker`/`interp`, and each form `exec`d (so later forms — here or in
///   the caller — see the definitions). **Never auto-compiles** a missing
///   fasl (per the design: compilation is an explicit `compile-module` step).
///
/// A `load`ed file's own `(load ...)`/`(use ...)` are honored recursively
/// (the recursive `load_file_flat` / this `Loader`'s scan). Unlike a source
/// load, the fasl path skips reading entirely, so a fasl's transitive
/// `(load)`s are already baked into its `top_levels`/delta.
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
    let fasl = typl.with_extension(FASL_EXTENSION);

    // Prefer a fresh-enough fasl.
    if fasl.is_file() {
        if let Ok(bytes) = fs::read(&fasl) {
            if let Ok(f) = crate::fasl::Fasl::from_bytes(&bytes) {
                let source_ok = match fs::read_to_string(&typl) {
                    Ok(src) => crate::fasl::source_hash(&src) == f.source_hash,
                    // No source alongside the fasl — trust the fasl.
                    Err(_) => true,
                };
                if source_ok {
                    return f.load_into(heap, checker, interp);
                }
            }
        }
        // A stale/unreadable fasl falls through to the source below.
    }

    let src = fs::read_to_string(&typl)
        .map_err(|e| Error::TypeError(format!("load: cannot read `{}`: {}", typl.display(), e)))?;
    load_source_flat(heap, reader, checker, interp, &typl, &src)
}

/// Reads, checks, and execs `src`'s forms into the current environment (root
/// namespace) — the source half of [`load_file_flat`], factored out so
/// `prelude::load`'s fallback and a `compile-module` capture can share it.
/// A nested `(load ...)` resolves relative to `file`'s own directory.
pub fn load_source_flat(
    heap: &mut Heap,
    reader: &Reader,
    checker: &mut Checker,
    interp: &mut Interp,
    file: &FsPath,
    src: &str,
) -> Result<(), Error> {
    let file_name = file.to_string_lossy();
    let mark = heap.root_count();
    let forms = match reader.read_all_in_spanned(heap, &file_name, src) {
        Ok(forms) => forms,
        Err(e) => {
            pop_roots_to(heap, mark);
            return Err(e);
        }
    };
    let dir = file.parent().unwrap_or_else(|| FsPath::new(".")).to_path_buf();
    let mut result = Ok(());
    checker.predeclare_program(heap, &forms.iter().map(|(v, _)| *v).collect::<Vec<_>>());
    for (v, loc) in forms {
        match checker.check_form_at(heap, &*interp, v, Some(loc)) {
            Ok(tl) if core::op(heap, tl) == Some("load") => {
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
    result
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
    match core::op(heap, tl) {
        Some("defmacro") => true,
        Some("module") => {
            let bundle = match core::field(heap, tl, 0) {
                Some(Value::Path(id)) => crate::types::path_from_id(heap, id) == Path::root(MONO_BUNDLE_MODULE),
                Some(Value::Symbol(id)) => heap.symbol_name(id) == MONO_BUNDLE_MODULE,
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

/// The path string a `(load "PATH")` form names, or `None` for any other form.
///
/// The tag check is part of the answer, not a precondition: every driver
/// (`main.rs`'s two, this module's two) has a top-level form in hand and wants
/// to know "is this a load, and of what?" as one question.
pub fn load_path_of(heap: &Heap, tl: TopLevelForm) -> Option<String> {
    if core::op(heap, tl) != Some("load") {
        return None;
    }
    match core::field(heap, tl, 0)? {
        Value::Str(id) => Some(heap.string(id).to_string()),
        _ => None,
    }
}
