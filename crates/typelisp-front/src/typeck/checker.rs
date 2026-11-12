//! The checker as a driver holds it: the registry of what the session has
//! defined, the module forms are checked in, and what checking has written
//! down beside the forms it returns. Each top-level form is checked by
//! [`super::toplevel`].

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

use typelisp_mem::{NsId, SymRef};

use crate::check::semantic::TypeUse;
use crate::check::{CompletionCandidate, DefLocs, Defined, Docs, Imported};
use crate::dump::{CheckerDelta, RegistrySignature};
use crate::types::{Path, Type};
use crate::{CompileTarget, Error, Heap, Loc, Value};

use super::attribute::{TestCase, TestForms, Tests};
use super::expr::Notes;
use super::registry::{AdtKind, Registry, Since};
use super::toplevel::{check_toplevel, in_module, publish, publish_all, publish_name_view, publish_uses, CREATED};
use super::use_form::{register_for_reading, Site};
use super::wire::RebuiltUnit;

/// Expands a macro call while a form is checked: `path` names a `defmacro`,
/// `raw_args` are the call's argument forms as written. Returns the
/// expansion, which is checked in place of the call.
///
/// Implemented by the evaluator, which runs the macro's body; the checker
/// names only this trait, so it does not depend on the evaluator.
pub trait MacroExpander {
    fn expand_macro(&self, heap: &mut Heap, path: &Path, raw_args: Vec<Value>) -> Result<Value, String>;

    /// Runs a checked form whose effect the next form's checking depends on —
    /// a `(defmacro ...)`, so that a later [`Self::expand_macro`] on its path
    /// finds a body to run, or a readtable registration.
    ///
    /// Every other definition reaches the evaluator through `exec`, called by
    /// the driver on the form the checker returned, and between top-level
    /// forms the driver runs these too (`project::needs_immediate_exec`).
    /// Between the forms *inside* one top-level form there is no driver: a
    /// `macrolet` binding is created while an expression is being checked, and
    /// a `defmacro` in a `(module ...)` block is checked along with the rest
    /// of the block, yet each has to be runnable before the very next form of
    /// the same top-level form is checked. These are the edges where checking
    /// installs something itself.
    fn install(&self, heap: &mut Heap, form: Value) -> Result<(), String>;
}

/// What checking a top-level form produces: a *core top-level form*, one of
/// `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`/`defenum`/`module`/
/// `use`/`load`/`expr` (`tests/core_vocabulary_test.rs` has an example of
/// each).
///
/// A generic definition is not in the vocabulary: the checker keeps its
/// checked body and emits only the specializations calls ask for, so nothing
/// downstream needs a node for a body that must never run.
pub type TopLevelForm = Value;

/// What happens when a `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`
/// reuses a name already defined by *non-builtin* code (the same module, in
/// the same namespace). Redefining a builtin (`typeck::builtins`) is always an
/// error, regardless of this setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RedefPolicy {
    /// Allow the redefinition, but record a warning ([`Checker::take_warnings`]).
    #[default]
    Warn,
    /// Reject the redefinition with a `TypeError`.
    Error,
    /// Allow the redefinition silently.
    Silent,
}

/// The module path under which checking a form bundles the specializations
/// it asked for with the form itself. Contains a space, which
/// [`crate::read::Reader`] treats as a delimiter — so no user-written
/// `module` form can ever collide with it.
pub const MONO_BUNDLE_MODULE: &str = "<monomorph specializations>";

/// Opaque saved namespace context — see [`Checker::suspend_ns_context`].
pub(crate) struct NsContext {
    ns: NsId,
    file_ns: Vec<NsId>,
}

/// See the module documentation.
#[derive(Debug)]
pub struct Checker {
    /// What the program has defined, the built-in definitions first.
    pub(super) reg: Registry,
    /// Whether the session's names have been told everything the registry
    /// holds ([`Self::publish_names`]); after that, each form tells them
    /// what it wrote.
    published: Cell<bool>,
    /// The type of the last form, when it was an expression.
    expr_ty: Option<Type>,
    /// The warnings checking has given and no one has taken yet.
    warnings: RefCell<Vec<String>>,
    /// Every C callback entry the checked code asks for, in the order first
    /// asked.
    callbacks: Vec<crate::ffi_callback::CallbackSig>,
    /// Every definition a `~/name/` directive calls.
    format_calls: BTreeSet<CompileTarget>,
    /// What a `#[test]` definition is checked into, and the tests found.
    tests: Tests,
    /// What checking has written down beside the trees.
    notes: Notes,
    /// The module forms are checked in now.
    ns: NsId,
    /// The modules of the files being loaded, innermost last
    /// ([`Self::enter_file_module`]). A nested `(module ...)` moves
    /// [`Self::ns`] but not this, so a `(use foo)` inside one still finds
    /// `foo` beside the file.
    file_ns: Vec<NsId>,
    /// The modules to return to on [`Self::exit_file_module`], innermost
    /// last: `(in-module ...)` moves the current module for the rest of the
    /// enclosing unit with no closing form, so leaving the unit restores the
    /// module it was entered from.
    ns_base: Vec<NsId>,
    /// What to do when a definition reuses a non-builtin name.
    redef_policy: RedefPolicy,
}

impl Default for Checker {
    fn default() -> Checker {
        Checker::new()
    }
}

impl Checker {
    pub fn new() -> Checker {
        let mut reg = Registry::default();
        super::builtins::define(&mut reg);
        reg.commit();
        debug_assert_eq!(reg.generation(), CREATED);
        Checker {
            reg,
            published: Cell::new(false),
            expr_ty: None,
            warnings: RefCell::default(),
            callbacks: Vec::new(),
            format_calls: BTreeSet::new(),
            tests: Tests::default(),
            notes: Notes::default(),
            ns: NsId::ROOT,
            file_ns: Vec::new(),
            ns_base: Vec::new(),
            redef_policy: RedefPolicy::default(),
        }
    }

    /// Check one top-level form in the current module: its core form,
    /// unrooted. Its specializations and the functions lifted for its
    /// callbacks are bundled with it (a module at [`MONO_BUNDLE_MODULE`],
    /// they first). What the form defines is kept only if checking it
    /// succeeds.
    pub fn check_form(&mut self, heap: &mut Heap, expander: &dyn MacroExpander, v: Value) -> Result<TopLevelForm, Error> {
        self.check_form_at(heap, expander, v, None)
    }

    /// [`Self::check_form`] with a location for `v` itself. A list form
    /// carries its own (`heap.cons_loc`) and ignores the hint; a bare atom
    /// at top level has none, and the reader hands its span beside it.
    pub fn check_form_at(&mut self, heap: &mut Heap, expander: &dyn MacroExpander, v: Value, loc_hint: Option<Loc>) -> Result<TopLevelForm, Error> {
        // The form is run in the module it is checked in, which is where a
        // `read` it makes at run time reads.
        heap.names().enter_module(self.ns);
        let loc = heap.cons_loc(v).or(loc_hint);
        let site = self.site(self.ns);
        if !self.published.get() {
            self.publish_names(heap);
        }
        let reg = &mut self.reg;
        publish_name_view(reg, site.ns);
        self.expr_ty = None;
        let entered = in_module(heap, site, v).map_err(|e| match &loc {
            Some(l) => e.at(l.clone()),
            None => e,
        })?;
        if let Some(inner) = entered {
            self.ns = inner.ns;
            reg.modules.insert(inner.ns, ());
            reg.commit();
            publish(reg, heap, &[inner.ns], &[], &[]);
            return crate::check::forms::module_form(heap, &Path::of_module(inner.ns), &[]);
        }
        // Every value the tree holds is rooted from here until it is lowered
        // into the form, which is then reachable from nothing but the caller.
        // Nothing allocates between releasing them and the caller rooting it.
        let roots_base = heap.root_count();
        let result = check_toplevel(reg, expander, heap, site, v, loc.clone(), &mut self.warnings.borrow_mut(), &self.notes, &mut self.tests);
        heap.truncate_roots(roots_base);
        match result {
            Ok((form, ty, reached)) => {
                self.format_calls.extend(reached.format_calls);
                for sig in reached.callbacks {
                    if !self.callbacks.contains(&sig) {
                        self.callbacks.push(sig);
                    }
                }
                if crate::types::in_std(site.ns) {
                    super::library::reexport(reg);
                    super::library::publish(reg, &mut heap.names(), Since::Commit);
                }
                let modules = reg.modules.keys(Since::Commit);
                let types = reg.types.keys(Since::Commit);
                let methods = reg.method_keys(Since::Commit);
                reg.commit();
                publish(reg, heap, &modules, &types, &methods);
                publish_name_view(reg, site.ns);
                self.expr_ty = ty;
                Ok(form)
            }
            Err(e) => {
                reg.rollback();
                let e = match &loc {
                    Some(l) => e.at(l.clone()),
                    None => e,
                };
                // In recover mode the form is a hole, and what follows it
                // is still checked.
                if !self.notes.recover {
                    return Err(e);
                }
                self.notes.errors.borrow_mut().push(e);
                let hole = crate::check::forms::hole_form(heap, loc)?;
                crate::check::forms::expr_form(heap, hole)
            }
        }
    }

    /// Where a form written in `ns` is: that module, the file being loaded,
    /// and the redefinition policy.
    fn site(&self, ns: NsId) -> Site {
        Site { ns, file: self.file_ns.last().copied(), policy: self.redef_policy }
    }

    /// Writes what the registry defines into `heap`'s table of what names
    /// mean. The first form [`Self::check_form_at`] checks does it, and
    /// each form after tells it what it wrote; a driver that reads before
    /// checking anything — a prelude or a dump just loaded — does it first.
    pub fn publish_names(&self, heap: &Heap) {
        publish_all(&self.reg, heap);
        self.published.set(true);
    }

    /// The type the last [`Self::check_form_at`] proved for its top-level
    /// expression, or `None` if that form was a definition (which has no
    /// value).
    pub fn expr_type(&self) -> Option<&Type> {
        self.expr_ty.as_ref()
    }

    /// Overrides the default [`RedefPolicy`] (`Warn`) for non-builtin
    /// redefinitions. Redefining a builtin is always an error regardless.
    pub fn set_redef_policy(&mut self, policy: RedefPolicy) {
        self.redef_policy = policy;
    }

    /// Drains and returns every warning recorded so far (e.g. by a
    /// `RedefPolicy::Warn` redefinition) — call after each `check_form` to
    /// surface them.
    pub fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.borrow_mut())
    }

    /// Turns error-recovery mode on or off. The LSP enables it before a check
    /// so a file with type errors still yields a partial tree and a full list
    /// of errors; everything else leaves it off and stops at the first error.
    pub fn set_recover(&mut self, on: bool) {
        self.notes.recover = on;
    }

    /// Drains and returns every error recover mode has checked past. Each
    /// already carries its own source location.
    pub fn take_errors(&self) -> Vec<Error> {
        std::mem::take(&mut *self.notes.errors.borrow_mut())
    }

    /// Drains and returns each place a type or a trait the program defines
    /// was named — the LSP's `textDocument/semanticTokens`.
    pub fn take_type_uses(&self) -> Vec<TypeUse> {
        std::mem::take(&mut *self.notes.type_uses.borrow_mut())
    }

    /// What a `#[test]` definition and a top-level expression are checked
    /// into — see [`TestForms`].
    pub fn set_test_forms(&mut self, forms: TestForms) {
        self.tests.forms = forms;
    }

    /// The tests recorded under [`TestForms::Collect`] since the last call,
    /// in the order they were defined.
    pub fn take_tests(&mut self) -> Vec<TestCase> {
        std::mem::take(&mut self.tests.found)
    }

    /// Every callback entry the checked code asks for, in the order it first
    /// asked. Read by `compile::aot::compile_file`, which emits one entry per
    /// callback and registers it at startup.
    pub fn ffi_callbacks(&self) -> Vec<crate::ffi_callback::CallbackSig> {
        self.callbacks.clone()
    }

    /// Every definition a `~/name/` directive checked so far calls. Read by
    /// `compile::aot::compile_file`, which turns each into a startup
    /// registration so the directive works in a standalone executable.
    pub fn format_call_targets(&self) -> Vec<CompileTarget> {
        self.format_calls.iter().cloned().collect()
    }

    /// Reports every `defsignature` in this unit that never got a definition.
    ///
    /// Called by a file/module loader once the unit's forms are all checked.
    /// The REPL deliberately does *not* call it: a declaration typed on one
    /// line and its definition on the next are two batches, and both are one
    /// session.
    pub fn finish_unit(&mut self) -> Result<(), Error> {
        let reg = &mut self.reg;
        let mut paths: Vec<Path> = reg.predeclared.iter().map(|(p, _)| *p).collect();
        for p in &paths {
            reg.predeclared.remove(p);
        }
        reg.commit();
        if paths.is_empty() {
            return Ok(());
        }
        paths.sort_by_key(|p| p.to_string());
        let list = paths.iter().map(Path::to_string).collect::<Vec<_>>().join("`, `");
        let (subject, verb) = if paths.len() == 1 { ("declaration", "has") } else { ("declarations", "have") };
        Err(Error::TypeError(format!(
            "defsignature: `{}` {} no definition in this file — a {} promises one",
            list, verb, subject
        )))
    }

    // ---- dumps --------------------------------------------------------------

    /// Where the registry's writes are as of now: the "before" half of
    /// [`Self::capture_delta`]. Take one, load a unit, then ask what changed.
    pub fn signature(&self) -> RegistrySignature {
        RegistrySignature::at(self.reg.generation())
    }

    /// Every entry the registry holds beyond the built-in definitions, one
    /// line each and sorted, for a test to compare two sessions by.
    pub fn registry_entries(&self) -> Vec<String> {
        let d = self.reg.changed_since(CREATED);
        let mut out = Vec::new();
        macro_rules! each {
            ($($table:ident),*) => {
                $(out.extend(d.$table.iter().map(|e| format!("{}: {:?}", stringify!($table), e)));)*
            };
        }
        each!(values, types, methods, inst_methods, method_names, imports, reexports, ctors, static_uses, module_aliases, modules, trait_impls, blanket_impls, blanket_owners, docs, predeclared, throw_tags);
        out.sort();
        out
    }

    /// Everything the registry was written since `before`, for a unit of a
    /// dump. See [`crate::dump`] for why a unit carries its own additions
    /// rather than the whole world.
    pub fn capture_delta(&self, heap: &Heap, before: &RegistrySignature) -> Result<CheckerDelta, String> {
        let unit = RebuiltUnit::of(heap, &self.reg, before.generation()).map_err(|e| format!("writing the checker's registry failed: {}", e))?;
        let registry = bincode::serialize(&unit).map_err(|e| format!("writing the checker's registry failed: {}", e))?;
        Ok(CheckerDelta { registry })
    }

    /// Reinstates what a unit of a dump wrote of the registry, and tells the
    /// session's names what it defined.
    pub(crate) fn apply_delta(&mut self, delta: CheckerDelta, heap: &mut Heap) -> Result<(), String> {
        let written: RebuiltUnit = bincode::deserialize(&delta.registry).map_err(|e| {
            format!(
                "reading the checker's part of the dump failed: {} — its shape has changed since the dump was made; run scripts/regen-prelude-bitcode.sh",
                e
            )
        })?;
        let unit = written.restore(heap).map_err(|e| e.to_string())?;
        let reg = &mut self.reg;
        let (modules, types, methods) = unit.written();
        unit.apply(reg);
        publish_uses(reg, heap, Since::Commit);
        reg.commit();
        publish(reg, heap, &modules, &types, &methods);
        Ok(())
    }

    // ---- what the session defines, for the editor ---------------------------

    /// Every bare name visible for completion while checking the module
    /// `module_path`; none for a module the session does not have.
    pub fn completion_candidates(&self, module_path: &[String]) -> Vec<CompletionCandidate> {
        let Some(here) = typelisp_mem::symbols::find_ns(module_path) else {
            return Vec::new();
        };
        super::editor::candidates(&self.reg, here)
    }

    /// Where each definition, and each local variable a reference names,
    /// was written, and the type of each expression: what the LSP's
    /// goto-definition and hover read.
    pub fn def_locs(&self) -> DefLocs {
        super::editor::def_locs(&self.reg, &self.notes)
    }

    /// The documentation of each definition.
    pub fn docs(&self) -> Docs {
        super::editor::docs(&self.reg)
    }

    /// Every definition the session holds, the library's and the program's.
    pub fn definitions(&self) -> Vec<Defined> {
        super::editor::definitions(&self.reg)
    }

    /// Every name a `use` brought into a module.
    pub fn imports(&self) -> Vec<Imported> {
        super::editor::imports(&self.reg)
    }

    /// What the module-level function `path` returns, when there is one.
    pub fn fn_ret(&self, path: &Path) -> Option<Type> {
        self.reg.fn_def(path).map(|def| def.ret.clone())
    }

    /// The kind of the data type `path`, when there is one.
    pub fn type_kind(&self, path: &Path) -> Option<AdtKind> {
        self.reg.type_def(path).map(|def| def.kind)
    }

    // ---- modules ------------------------------------------------------------

    /// The symbol table's module for the namespace forms are checked in now —
    /// the one the next form has to be read in for its names to be this
    /// module's.
    pub fn current_module(&self) -> NsId {
        self.ns
    }

    /// Set the module that later forms are checked in. Used by the CLI's
    /// `run_file` to place a runtime `(eval ...)` in the script's own module,
    /// as its forms were checked at load time.
    pub fn set_current_ns(&mut self, ns: NsId) {
        self.ns = ns;
        self.publish_name_view();
    }

    /// Move into the module `segs` names from the current one, one child per
    /// segment, and ensure the (possibly empty) module exists, returning the
    /// module as a [`Path`].
    ///
    /// The root's `keyword` is the keyword namespace (CL's `KEYWORD`
    /// package), which owns every keyword and nothing else, so no module may
    /// take its place.
    pub(crate) fn enter_module(&mut self, segs: &[String]) -> Result<Path, Error> {
        if self.ns == NsId::ROOT && segs.first().is_some_and(|s| s.eq_ignore_ascii_case(typelisp_mem::symbols::KEYWORD_MODULE)) {
            return Err(Error::TypeError(format!(
                "module `{}` is the keyword namespace, which holds the keywords and nothing else — choose another name",
                typelisp_mem::symbols::KEYWORD_MODULE
            )));
        }
        self.ns = segs.iter().fold(self.ns, |ns, s| typelisp_mem::symbols::child_ns(ns, s));
        if self.reg.modules.get(&self.ns).is_none() {
            self.reg.modules.insert(self.ns, ());
            self.reg.commit();
            self.published.set(false);
        }
        self.publish_name_view();
        Ok(Path::of_module(self.ns))
    }

    /// [`Self::enter_module`] for the module a file's location derives to,
    /// which the file's whole content is checked in: `crate::project` calls
    /// it, and a nested `(module ...)` in the file does not.
    pub fn enter_file_module(&mut self, segs: &[String]) -> Result<Path, Error> {
        let base = self.ns;
        let path = self.enter_module(segs)?;
        self.ns_base.push(base);
        self.file_ns.push(self.ns);
        Ok(path)
    }

    /// Undo [`Self::enter_file_module`]: return to the module this file was
    /// entered from (an `(in-module ...)` in the file may have moved the
    /// current one anywhere) and drop its file's frame.
    pub fn exit_file_module(&mut self) {
        self.ns = self.ns_base.pop().expect("exit_file_module without enter_file_module");
        self.file_ns.pop();
        self.publish_name_view();
    }

    /// Temporarily clear the module context so the loader can load a
    /// dependency file from inside another file's check loop without
    /// registering it under that file's module: a `use` surfaced by macro
    /// expansion is only discovered mid-check (`Loader::load_source_inner`).
    /// Pair with [`Self::resume_ns_context`].
    pub(crate) fn suspend_ns_context(&mut self) -> NsContext {
        NsContext {
            ns: std::mem::replace(&mut self.ns, NsId::ROOT),
            file_ns: std::mem::take(&mut self.file_ns),
        }
    }

    /// Restore the context saved by [`Self::suspend_ns_context`]. The
    /// in-between load must have left the context empty again.
    pub(crate) fn resume_ns_context(&mut self, saved: NsContext) {
        debug_assert!(self.ns == NsId::ROOT && self.file_ns.is_empty(), "dependency load left an unbalanced namespace context");
        self.ns = saved.ns;
        self.file_ns = saved.file_ns;
        self.publish_name_view();
    }

    /// Makes the printer write paths for the module being checked now
    /// (`typelisp_mem::NameView`). Called wherever the current module
    /// changes, and before each form, since a form can bring a type in.
    fn publish_name_view(&self) {
        publish_name_view(&self.reg, self.ns);
    }

    /// What a `(use PATH)` written in the module `ns` brings in, registered
    /// in `heap`'s table of names for the forms read after it, so they read
    /// each name it brings in as `ns`'s own symbol. A path that resolves to
    /// nothing is left for checking the `use` to report.
    pub(crate) fn register_use_for_reading(&self, heap: &Heap, ns: NsId, path: &[SymRef]) {
        register_for_reading(&self.reg, heap, self.site(ns), path);
    }

    /// `v`, written in the current module, expanded once when it is a call of
    /// a macro; `Ok(None)` when it is not. The loader (`crate::project`)
    /// reads what the expansion `use`s before the form is checked. The
    /// caller must root the expansion before anything that may allocate.
    pub(crate) fn try_expand_toplevel_macro(&self, heap: &mut Heap, expander: &dyn MacroExpander, v: Value) -> Result<Option<Value>, Error> {
        super::macros::expand_toplevel(&self.reg, expander, heap, self.ns, v)
    }
}
