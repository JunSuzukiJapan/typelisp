//! The type information layer: what the names a form is checked against
//! mean — the registry of definitions, the module the checker is in, and the
//! lexical scopes (local variables, type variables, local macros). The
//! analysis writes it; checking and lowering only read it
//! (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

use typelisp_mem::symbols::{self, NOT_WELL_KNOWN};
use typelisp_mem::{NsId, SymRef};

use crate::{Error, Loc, Path, Type, Value};
use super::registry::{AssocFn, Namespace, Registry, TraitBound, TypeAlias, VarInfo};
use super::analyze::*;

/// The `Env` type a `symbol-macrolet` name is bound to. Not a type any
/// program can write — a path segment cannot contain a space — so finding it
/// in the environment means exactly one thing: this name stands for the form
/// [`Info::local_symbol_macros`] holds for it.
///
/// Binding the name at all is the point. It puts symbol macros in the same
/// shadowing order as every other binding, instead of a second lookup that
/// would have to reproduce the order by hand.
pub(super) fn symbol_macro_mark() -> Type {
    Type::Named(Path::root(" symbol-macro"), Vec::new())
}

/// One binding of a local: its name, type, and — where known — the source
/// position of the name in its binding form (see [`Env::get_loc`]).
pub(super) type LocalBinding = (String, Type, Option<Loc>);

/// The bindings one [`Env::extended_with_locs`] added, on top of the scope it
/// extended. A chain rather than one flat list copied at every level: a body
/// nested `n` binding forms deep would otherwise hold `n` copies of a list
/// that grows to `n`, all alive at once down the recursion.
pub(super) struct Scope {
    binds: Vec<LocalBinding>,
    parent: Option<std::rc::Rc<Scope>>,
    /// How many bindings the scopes above this one hold — what makes a
    /// binding's [`Env::position`] stable down the chain.
    start: usize,
}

/// A lexical environment mapping variable names to their types.
#[derive(Clone)]
pub(super) struct Env {
    /// Each binding's name, type, and — where known — the source position of
    /// the name in its binding form (a `let`/`let*` binding, a `lambda`/
    /// `labels`/`defun`/`defmethod` parameter or receiver name, a `labels`
    /// function name, or a `match`-pattern binding). `None` for a binding
    /// this checker doesn't bother tracking a position for
    /// (monomorphization's synthesized re-checks) — consulted by
    /// `Checker::check_at` to populate `DefLocs::local_refs` when a `Var`
    /// reference resolves here.
    vars: Option<std::rc::Rc<Scope>>,
    /// Trait bounds on this function's own generic type parameters, declared
    /// by a `(:where (Trait T (Assoc Concrete)...)...)` clause
    /// (`Checker::check_defun`) — e.g. `{"t": [TraitBound { trait_path:
    /// Path::std("iter"), assoc: {} }]}`. Fixed for the lifetime of one
    /// function body (a nested `let`/`lambda` never introduces new type
    /// parameters), so `extended` just carries the same `Rc` forward instead
    /// of letting it vary per-binding the way `vars` does. Consulted only by
    /// `Checker::receiver_method`'s bounded-type-variable step — see
    /// its doc comment. The same bounds are also stored on the registered
    /// `FnSig` (`Checker::check_defun`), for `Checker::check_call`'s
    /// call-site validation — this copy exists only because body-checking
    /// needs it readily available in `Env`, not threaded through `Registry`
    /// lookups on every method call.
    pub(super) bounds: std::rc::Rc<BTreeMap<String, Vec<TraitBound>>>,
}

impl Env {
    pub(super) fn new() -> Env {
        Env { vars: None, bounds: std::rc::Rc::new(BTreeMap::new()) }
    }

    /// The nearest binding of `name`, with where it sits counted from the
    /// outermost binding.
    pub(super) fn lookup(&self, name: &str) -> Option<(usize, &LocalBinding)> {
        let mut scope = self.vars.as_deref();
        while let Some(s) = scope {
            if let Some(i) = s.binds.iter().rposition(|(n, _, _)| n == name) {
                return Some((s.start + i, &s.binds[i]));
            }
            scope = s.parent.as_deref();
        }
        None
    }

    pub(super) fn get(&self, name: &str) -> Option<&Type> {
        self.lookup(name).map(|(_, (_, t, _))| t)
    }

    /// Where the nearest binding of `name` sits, counted from the outermost
    /// binding — stable down a chain of `extended` environments, which only
    /// ever add.
    pub(super) fn position(&self, name: &str) -> Option<usize> {
        self.lookup(name).map(|(i, _)| i)
    }

    /// How many bindings are in scope, shadowed ones included.
    pub(super) fn len(&self) -> usize {
        self.vars.as_ref().map_or(0, |s| s.start + s.binds.len())
    }

    /// The nearest binding of `name`'s own recorded source position, if any —
    /// see [`Self::vars`]'s doc comment. Consulted by `Checker::check_at`
    /// alongside [`Self::get`] when a `Var` reference resolves to a local.
    pub(super) fn get_loc(&self, name: &str) -> Option<Loc> {
        self.lookup(name).and_then(|(_, (_, _, l))| l.clone())
    }

    /// A child environment with `binds` added (later bindings shadow earlier),
    /// with no recorded position for any of them — used by callers that don't
    /// track individual binding positions (monomorphization's synthesized
    /// re-checks). See [`Self::extended_with_locs`] for the counterpart that
    /// does.
    pub(super) fn extended(&self, binds: Vec<(String, Type)>) -> Result<Env, Error> {
        self.extended_with_locs(binds.into_iter().map(|(n, t)| (n, t, None)).collect())
    }

    /// Like [`Self::extended`], but each binding also carries the source
    /// position of its name in the binding form — used by `let`/`let*`,
    /// `lambda`/`labels` parameters, and `defun`/`defmethod` parameters and
    /// receivers, so a later reference to the bound name can resolve back to
    /// where it was bound (`Checker::check_at`, `DefLocs::local_refs`).
    /// Every local binding in the language is made here — a `let`/`let*`
    /// binding, a `lambda`/`labels`/`defun`/`defmethod` parameter or receiver,
    /// a `labels` function name, a `match`-pattern binding — which is what
    /// makes this the one place a reserved name can be refused.
    pub(super) fn extended_with_locs(&self, binds: Vec<(String, Type, Option<Loc>)>) -> Result<Env, Error> {
        for (name, _, _) in &binds {
            if is_reserved_name(name) {
                return Err(Error::TypeError(format!("`{}` is a reserved word and cannot be bound", name)));
            }
        }
        let start = self.len();
        let vars = Some(std::rc::Rc::new(Scope { binds, parent: self.vars.clone(), start }));
        Ok(Env { vars, bounds: self.bounds.clone() })
    }

    /// A child environment that additionally declares `bounds` (a function's
    /// own `where`-clause trait bounds) — see the field's doc comment.
    pub(super) fn with_bounds(&self, bounds: BTreeMap<String, Vec<TraitBound>>) -> Env {
        Env { vars: self.vars.clone(), bounds: std::rc::Rc::new(bounds) }
    }
}

/// Names no binding may take.
///
/// `task` is always a special form in head position, so a variable called
/// `task` could be bound but never called — a name that silently does nothing
/// is worse than a rejected one. `thread` is `task`'s twin, closed for the
/// same reason.
///
/// The other special forms are *not* here. `(let ((if f)) ...)` has been legal
/// since the beginning and the prelude and tests lean on names like `list` and
/// `format` being ordinary words; closing that is a separate, breaking change.
/// `task` and `thread` were closed when they arrived, before anything had the
/// chance to depend on them (`task` replaced `go` on 2026-09-25, when no
/// program used `task` as a name).
pub(super) fn is_reserved_name(name: &str) -> bool {
    name == "task" || name == "thread"
}

/// See the module documentation.
pub(super) struct Info {
    pub(super) reg: Registry,
    /// The module forms are checked in now.
    pub(super) ns: NsId,
    /// Stack of enclosing *files'* own modules, innermost last — set by
    /// [`Checker::enter_file_module`]/[`Checker::exit_file_module`] (`project.rs`'s
    /// file-level wrapping only, never [`Checker::enter_module`]'s nested
    /// `(module ...)` forms), so it stays fixed at "the loading file's own
    /// module" even while `self.ns` moves into a nested `module` block inside
    /// that file. `Checker::use_target`'s sibling-file
    /// resolution reads the top of this stack (never `self.ns` itself)
    /// so `(use foo)` inside a nested `module` block still resolves `foo`
    /// relative to the *file's* directory, not that inner block's own
    /// namespace (which has no filesystem counterpart to be a sibling of).
    /// A recursively-loaded dependency pushes and fully pops its own frame
    /// before the file that `use`s it resumes checking (`project.rs`'s
    /// scan-before-check ordering), so this never needs more than simple
    /// stack discipline.
    pub(super) file_ns: Vec<NsId>,
    /// The modules to return to on [`Checker::exit_file_module`], innermost
    /// last: `(in-module ...)` moves the current module for the rest of the
    /// enclosing unit with no closing form, so leaving the unit restores the
    /// module it was entered from.
    pub(super) ns_base: Vec<NsId>,
    /// The type variables each enclosing definition binds, innermost last —
    /// what lets [`Checker::parse_type_here_at`] tell a type variable from a
    /// type name that resolves to nothing.
    ///
    /// Both are a bare name `canon` cannot resolve, so without this an
    /// unknown name was accepted as if it were a type variable and surfaced
    /// much later, as a mismatch between two spellings of the same name.
    /// Each definition pushes a frame of the variables its declaration part
    /// introduces: the `<...>` of a `defun`/`defstruct`/`defenum`/`deftype`
    /// name, the unresolved names of a `defmethod` receiver or an `impl`
    /// target (plus `impl<T>`'s own), a `deftrait`'s `Self` and associated
    /// types — and nothing for the forms that bind none. While the stack is
    /// non-empty every name must be a type or one of the frames' variables.
    /// [`Checker::enter_specialization`] clears it for the same reason it clears
    /// `loop_stack`: a specialization re-checks with every variable bound.
    pub(super) type_names: RefCell<Vec<HashSet<String>>>,
    /// `macrolet` scopes, innermost last: each maps a locally bound macro
    /// name to the synthesized global path its body was installed under and
    /// the call-site arity shape [`Self::resolve_macro`] answers with.
    ///
    /// A `RefCell` because expression checking is `&self` — the whole reason
    /// `macrolet` needed a table of its own rather than the registry's.
    local_macros: RefCell<Vec<HashMap<String, (Path, MacroShape)>>>,
    /// `symbol-macrolet` scopes, innermost last: name -> the form the name
    /// stands for.
    ///
    /// Shadowing is not decided here. Each scope also binds its names in the
    /// `Env` at [`symbol_macro_mark`], so an inner `let` of the same name
    /// hides the symbol macro (it rebinds the name to a real type) and the
    /// symbol macro hides an outer variable (it rebinds the name to the
    /// mark) — which is exactly CL's rule, arrived at by the environment
    /// doing what it already does.
    local_symbol_macros: RefCell<Vec<HashMap<String, Value>>>,
}

impl Info {
    pub(super) fn new() -> Info {
        Info {
            reg: Registry::with_builtins(),
            ns: NsId::ROOT,
            file_ns: Vec::new(),
            ns_base: Vec::new(),
            type_names: RefCell::new(Vec::new()),
            local_macros: RefCell::new(Vec::new()),
            local_symbol_macros: RefCell::new(Vec::new()),
        }
    }

    /// `name` defined in the current module.
    pub(super) fn fq(&self, name: &str) -> Path {
        Path::in_ns(self.ns, name)
    }

    /// The current namespace.
    pub(super) fn cur_ns(&self) -> &Namespace {
        self.reg.module_at(self.ns).expect("current namespace exists")
    }

    /// Whether a definition of the module `item_ns` is visible from the
    /// current module without `pub`: only from that module itself. An
    /// enclosing or an enclosed module is another module.
    ///
    /// Until the standard library is sorted into its own modules
    /// (`docs/dev/explicit-symbols-plan.md`, stage 6), the library is in
    /// scope everywhere — its private layer (`%internal`, a stream's handle,
    /// the built-ins under `HashTable`'s `get`/`set`) included.
    pub(super) fn in_scope(&self, item_ns: NsId) -> bool {
        item_ns == self.ns || in_std(item_ns)
    }

    /// Whether an associated function/method `af` of `type_fq` is reachable
    /// from the current namespace: public, or in scope (declared in the
    /// same module, [`Self::in_scope`]) — mirrors [`Self::find_written`]'s
    /// cross-module `public` check. Used at every call site that looks up
    /// `def.assoc` directly (`try_field_access`, `check_field_set`,
    /// `resolve_name`'s type-member step, `receiver_method`,
    /// `receiver_method`) so a private member — including a
    /// `defstruct` field whose accessor wasn't declared `pub` — is treated
    /// as absent rather than merely forbidden, matching how a private free
    /// function or constructor "doesn't resolve" instead of erroring with a
    /// privacy-specific message.
    pub(super) fn assoc_visible(&self, type_fq: &Path, af: &AssocFn) -> bool {
        af.sig.public || self.in_scope(type_fq.ns())
    }

    /// What `sym` — a bare name as read in the current module — names, as
    /// `lookup` finds it among one module's definitions (`lookup` is given
    /// the module and the symbol to look up there), with the definition's
    /// path.
    ///
    /// This module's own definition comes first, then what a `use` here
    /// brought in under the name, then the definition in the module the
    /// symbol belongs to (a name a macro's expansion carried in from the
    /// macro's module), then what the standard library offers every module
    /// under it. No
    /// enclosing module is consulted: a module sees what it defines and what
    /// it brings in (`docs/dev/explicit-symbols-plan.md`, 3.3).
    ///
    /// An import of something `lookup` does not find (a type, where a
    /// function is asked for) leaves the search going.
    pub(super) fn find_named<T>(&self, sym: SymRef, mut lookup: impl FnMut(&Namespace, SymRef) -> Option<T>) -> Option<(Path, T)> {
        let mut at = |ns: NsId, key: SymRef| -> Option<(Path, T)> {
            let m = self.reg.module_at(ns)?;
            lookup(m, key).map(|v| (Path::of_symbol(ns, key), v))
        };
        let here = self.cur_ns();
        if let Some(found) = at(here.ns, sym) {
            return Some(found);
        }
        if let Some(target) = here.aliases.get_sym(sym) {
            if let Some(found) = at(target.ns(), target.name_symbol()) {
                return Some(found);
            }
        }
        // A member of the fixed vocabulary belongs to every module alike, so
        // where it was made says nothing about which definition it names.
        if sym.home() != here.ns && sym.well_known() == NOT_WELL_KNOWN {
            if let Some(found) = at(sym.home(), sym) {
                return Some(found);
            }
        }
        at(NsId::STD, sym)
    }

    /// What a bare module-level name means — a function, a global variable
    /// or a macro, never two of them ([`Checker::check_value_clash`]) — with
    /// its path ([`Self::find_named`]). The three are one name space: a
    /// function defined here hides a macro of the same name the standard
    /// library offers.
    pub(super) fn resolve_module_entry(&self, sym: SymRef) -> Option<(Path, ModuleEntry)> {
        self.find_named(sym, module_entry_of)
    }

    /// [`Self::resolve_module_entry`] for a written name ([`Self::find_written`]):
    /// the last segment of a `module::...::name` path, in the module the path
    /// reached.
    pub(super) fn module_entry_at(&self, p: &Path) -> Option<(Path, ModuleEntry)> {
        self.find_written(p, |m, k| module_entry_of(m, k).map(|entry| (entry.public(), entry)))
    }

    /// What a bare module-level name holds, if it is a function or a global
    /// variable — see [`Self::resolve_module_entry`]. `None` when the nearest
    /// definition is a macro, which hides any value further out.
    pub(super) fn resolve_module_value(&self, sym: SymRef) -> Option<ModuleValue> {
        match self.resolve_module_entry(sym)? {
            (path, ModuleEntry::Value(kind)) => Some(kind.at(path)),
            (_, ModuleEntry::Macro { .. }) => None,
        }
    }

    /// [`Self::resolve_module_value`] for a written name ([`Self::module_entry_at`]).
    pub(super) fn module_value_at(&self, p: &Path) -> Option<ModuleValue> {
        match self.module_entry_at(p)? {
            (path, ModuleEntry::Value(kind)) => Some(kind.at(path)),
            (_, ModuleEntry::Macro { .. }) => None,
        }
    }

    /// The function a bare name holds, if what it holds is a function — see
    /// [`Self::resolve_module_value`].
    pub(super) fn resolve_fn(&self, sym: SymRef) -> Option<Path> {
        match self.resolve_module_value(sym)? {
            ModuleValue::Fn(path) => Some(path),
            ModuleValue::Var(..) => None,
        }
    }

    /// Resolve a bare macro name to its absolute [`Path`] and the call-site
    /// arity shape ([`MacroShape`]): a `macrolet` binding, or else the
    /// module-level name, if the nearest definition of it is a macro
    /// ([`Self::resolve_module_entry`]).
    pub(super) fn resolve_macro(&self, sym: SymRef) -> Option<(Path, MacroShape)> {
        // `macrolet` first, innermost scope first: a local macro shadows a
        // global one of the same name for the extent of its body, which is
        // what "lexically scoped macro definition" means.
        for scope in self.local_macros.borrow().iter().rev() {
            if let Some(found) = scope.get(sym.name()) {
                return Some(found.clone());
            }
        }
        match self.resolve_module_entry(sym)? {
            (path, ModuleEntry::Macro { shape, .. }) => Some((path, shape)),
            (_, ModuleEntry::Value(_)) => None,
        }
    }

    /// Runs `body` with `scope`'s `macrolet` bindings in view, innermost.
    pub(super) fn with_local_macros<R>(&self, scope: HashMap<String, (Path, MacroShape)>, body: impl FnOnce() -> R) -> R {
        self.local_macros.borrow_mut().push(scope);
        let result = body();
        self.local_macros.borrow_mut().pop();
        result
    }

    /// Runs `body` with `scope`'s `symbol-macrolet` bindings in view,
    /// innermost. The forms are heap values this table holds where the
    /// collector cannot see them, so the caller roots them for as long.
    pub(super) fn with_symbol_macros<R>(&self, scope: HashMap<String, Value>, body: impl FnOnce() -> R) -> R {
        self.local_symbol_macros.borrow_mut().push(scope);
        let result = body();
        self.local_symbol_macros.borrow_mut().pop();
        result
    }

    /// The form `name` stands for, if a `symbol-macrolet` scope in view binds
    /// it. Innermost scope first, the way `Env` itself resolves.
    pub(super) fn symbol_macro_form(&self, name: &str) -> Option<Value> {
        for scope in self.local_symbol_macros.borrow().iter().rev() {
            if let Some(form) = scope.get(name) {
                return Some(*form);
            }
        }
        None
    }

    /// The macro a written name names ([`Self::module_entry_at`]), with its
    /// call-site arity shape ([`MacroShape`]) — the written-name counterpart
    /// of [`Self::resolve_macro`].
    pub(super) fn macro_at(&self, p: &Path) -> Option<(Path, MacroShape)> {
        match self.module_entry_at(p)? {
            (path, ModuleEntry::Macro { shape, .. }) => Some((path, shape)),
            (_, ModuleEntry::Value(_)) => None,
        }
    }

    /// Resolve a bare constructor name to its `(type path, variant index)`
    /// ([`Self::find_named`]): `Sexpr`'s, which the standard library offers,
    /// or one a `(use Type)` brought in.
    pub(super) fn resolve_ctor(&self, sym: SymRef) -> Option<(Path, usize)> {
        self.find_named(sym, |m, k| m.ctors.get_sym(k).cloned()).map(|(_, ctor)| ctor)
    }

    /// The `Sexpr` variant `name` — the one a value of a type with a `Sexpr`
    /// encoding is wrapped in — as `(type path, variant index)`.
    pub(super) fn sexpr_variant(&self, name: &str) -> (Path, usize) {
        let path = Path::std("sexpr");
        let def = self.reg.type_def(&path).expect("sexpr is built in");
        let idx = def.variants.iter().position(|v| v.name == name).expect("sexpr has the variant");
        (path, idx)
    }

    /// The function the standard library offers every module as `name` — for
    /// a form the checker rewrites into a call of a library function, which
    /// means the library's whatever the module defines.
    pub(super) fn offered_fn(&self, name: &str) -> Option<Path> {
        let sym = symbols::intern_exact(NsId::STD, name);
        let in_std = Path::of_symbol(NsId::STD, sym);
        self.reg.fn_sig(&in_std).is_some().then_some(in_std)
    }

    /// The `Sexpr` `cons` variant's `(type path, variant index)`, read from the
    /// type definition directly rather than the bare-name `ctors` map — that
    /// bare `cons` name was removed (Symbol/Sexpr redesign Phase 4b) so `(cons
    /// a b)` resolves to the free `cons<T,U>` pair function. Internal `Sexpr`
    /// cons construction (`list` literals, quasiquote) still needs the variant,
    /// and reaches it through here.
    pub(super) fn sexpr_cons_ctor(&self) -> (Path, usize) {
        self.sexpr_variant("cons")
    }

    /// Resolve a bare name snapshotted by `(use Type)` (see `Self::check_use`)
    /// to the `(type path, method name)` of the static associated function it
    /// names — the static-method counterpart of [`Self::resolve_ctor`],
    /// found the same way.
    pub(super) fn resolve_static_use(&self, sym: SymRef) -> Option<(Path, String)> {
        self.find_named(sym, |m, k| m.static_uses.get_sym(k).cloned()).map(|(_, member)| member)
    }

    /// What a written name `p` names ([`crate::types::written_path`]), as
    /// `lookup` finds it among one module's definitions; `lookup` answers the
    /// definition's visibility along with it.
    ///
    /// The definition at `p` itself comes first: a path's, which must be
    /// `pub` unless it is in scope, or the current module's own. A name of
    /// the current module that it does not define is then looked up as a
    /// bare name is ([`Self::find_named`]). A path into another module
    /// reaches what that module re-exports under the name
    /// (`Namespace::reexports`) as the re-exported definition's own path
    /// does; anything else it leads to names nothing.
    pub(super) fn find_written<T>(
        &self,
        p: &Path,
        mut lookup: impl FnMut(&Namespace, SymRef) -> Option<(bool, T)>,
    ) -> Option<(Path, T)> {
        let module = self.reg.module_at(p.ns());
        if let Some((public, v)) = module.and_then(|m| lookup(m, p.name_symbol())) {
            return (public || self.in_scope(p.ns())).then_some((*p, v));
        }
        if p.ns() != self.cur_ns().ns {
            let m = module?;
            m.reexports.get_sym(p.name_symbol())?;
            let target = *m.aliases.get_sym(p.name_symbol())?;
            return self.find_written(&target, lookup);
        }
        self.find_named(p.name_symbol(), |m, k| lookup(m, k).map(|(_, v)| v))
    }

    /// The type a written name names ([`Self::find_written`]). Every place a
    /// type name is written goes through here — a type annotation
    /// ([`Self::resolve_type_name`]), a pattern head, `compile`/`documentation`
    /// of a type — so a type brought in by `(use m::point)` is the same
    /// `point` in all of them.
    pub(super) fn type_at(&self, p: &Path) -> Option<Path> {
        self.find_written(p, |m, k| m.types.get_sym(k).map(|def| (def.public, def.name))).map(|(_, name)| name)
    }

    /// The `deftrait` trait a written name names ([`Self::find_written`]).
    /// Traits are not types, and have a table of their own.
    pub(super) fn trait_at(&self, p: &Path) -> Option<Path> {
        self.find_written(p, |m, k| m.traits.get_sym(k).map(|td| (td.public, td.name))).map(|(_, name)| name)
    }

    /// The `deftype` alias a written name names, if it names one
    /// ([`Self::find_written`]): an alias is a spelling for a type and is
    /// reached the same ways one is.
    pub(super) fn type_alias_at(&self, p: &Path) -> Option<TypeAlias> {
        self.find_written(p, |m, k| m.type_aliases.get_sym(k).map(|a| (a.public, a.clone()))).map(|(_, a)| a)
    }

    /// Resolve a nominal type [`Path`] to the type it names if there is one;
    /// otherwise (type variables, unknown names) return it unchanged.
    pub(super) fn resolve_type_name(&self, path: &Path) -> Path {
        self.type_at(path).unwrap_or(*path)
    }

    /// The global a bare name holds, if what it holds is a variable — see
    /// [`Self::resolve_module_value`].
    pub(super) fn resolve_global(&self, sym: SymRef) -> Option<(Path, VarInfo)> {
        match self.resolve_module_value(sym)? {
            ModuleValue::Var(path, vi) => Some((path, vi)),
            ModuleValue::Fn(_) => None,
        }
    }
}

/// Whether `ns` is the standard library's module or one inside it.
fn in_std(ns: NsId) -> bool {
    std::iter::successors(Some(ns), |m| symbols::ns_parent(*m)).any(|m| m == NsId::STD)
}
