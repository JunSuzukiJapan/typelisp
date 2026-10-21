//! The type information layer: what the names a form is checked against
//! mean — the registry of definitions, the module the checker is in, and the
//! lexical scopes (local variables, type variables, local macros). The
//! analysis writes it; checking and lowering only read it
//! (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

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
    pub(super) ns: Vec<String>,
    /// Stack of enclosing *files'* own module paths, innermost last — set by
    /// [`Checker::enter_file_module`]/[`Checker::exit_file_module`] (`project.rs`'s
    /// file-level wrapping only, never [`Checker::enter_module`]'s nested
    /// `(module ...)` forms), so it stays fixed at "the loading file's own
    /// path" even while `self.ns` grows deeper through a nested `module`
    /// block inside that file. [`Self::find_module`]'s sibling-file
    /// resolution tier reads the top of this stack (never `self.ns` itself)
    /// so `(use foo)` inside a nested `module` block still resolves `foo`
    /// relative to the *file's* directory, not that inner block's own
    /// namespace (which has no filesystem counterpart to be a sibling of).
    /// A recursively-loaded dependency pushes and fully pops its own frame
    /// before the file that `use`s it resumes checking (`project.rs`'s
    /// scan-before-check ordering), so this never needs more than simple
    /// stack discipline.
    pub(super) file_ns: Vec<Vec<String>>,
    /// Namespace lengths to restore on [`Checker::exit_file_module`] /
    /// [`Checker::check_module`], innermost last. A closing form used to be
    /// enough — every push had one — but `(in-module ...)` pushes segments
    /// that stay in effect for the rest of the enclosing unit, so how far to
    /// unwind is the saved base rather than a count of segments.
    pub(super) ns_base: Vec<usize>,
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
            ns: Vec::new(),
            file_ns: Vec::new(),
            ns_base: Vec::new(),
            type_names: RefCell::new(Vec::new()),
            local_macros: RefCell::new(Vec::new()),
            local_symbol_macros: RefCell::new(Vec::new()),
        }
    }

    /// Qualify a bare name with the current namespace path.
    pub(super) fn fq(&self, name: &str) -> Path {
        let mut segments = self.ns.clone();
        segments.push(name.to_string());
        Path::from_segments(segments)
    }

    /// The current namespace.
    pub(super) fn cur_ns(&self) -> &Namespace {
        self.reg.module(&self.ns).expect("current namespace exists")
    }

    /// If `path` starts with an empty segment (reader-encoded `::foo` absolute
    /// path), strip it and return `(true, rest)`.  Otherwise `(false, path)`.
    pub(super) fn split_abs<'a>(&self, path: &'a [String]) -> (bool, &'a [String]) {
        if path.first().map(|s| s.is_empty()).unwrap_or(false) {
            (true, &path[1..])
        } else {
            (false, path)
        }
    }

    /// The current namespace's ancestor chain, nearest first, ending at root:
    /// `self.ns`, `self.ns[..len-1]`, ..., `[]`. The module-tree analogue of
    /// walking parent lexical-scope frames — every bare-name resolver walks
    /// this instead of only checking "current module or literal root", so a
    /// submodule sees not just its own definitions but every enclosing
    /// module's too (including root's, however deeply nested the caller is).
    pub(super) fn ns_ancestors(&self) -> impl Iterator<Item = &[String]> {
        // The standard library's module comes last: while its definitions are
        // still resolved by name, this is what keeps them in scope from a
        // module that is not inside it.
        (0..=self.ns.len()).rev().map(move |k| &self.ns[..k]).chain(std::iter::once(STD_SEGMENTS.as_slice()))
    }

    /// The first hit `lookup` finds walking the ancestor chain, nearest module
    /// first. `lookup` is given each module's absolute path along with it.
    pub(super) fn find_on_chain<T>(&self, mut lookup: impl FnMut(&[String], &Namespace) -> Option<T>) -> Option<T> {
        self.ns_ancestors().find_map(|prefix| lookup(prefix, self.reg.module(prefix)?))
    }

    /// Looks up the last segment of `segs` in the module the other segments
    /// name ([`Self::find_module`]), under the cross-module visibility rule:
    /// a private item is found only from inside its own module. `lookup`
    /// answers the item's visibility and value; the result carries the
    /// item's absolute path.
    pub(super) fn find_in_module<T>(
        &self,
        segs: &[String],
        lookup: impl FnOnce(&Namespace, &str) -> Option<(bool, T)>,
    ) -> Option<(Path, T)> {
        let (last, mods) = segs.split_last()?;
        let (abs, m) = self.find_module(mods)?;
        let (public, v) = lookup(m, last)?;
        if !public && !self.in_scope(&abs) {
            return None;
        }
        Some((item_path(&abs, last), v))
    }

    /// Whether `item_ns` (a definition's owning module) is visible from the
    /// current namespace without needing `pub` — Rust-style module privacy:
    /// the current module IS `item_ns`, or is nested inside it (a
    /// descendant). Replaces the old exact-equality `same_module` check;
    /// root (`item_ns == []`) is always an ancestor, so this also subsumes
    /// the former "literal root caller" special case.
    pub(super) fn in_scope(&self, item_ns: &[String]) -> bool {
        // The standard library's module is in scope everywhere, as the root
        // it was moved out of was, while its names are still resolved by
        // name.
        item_ns.first().map(String::as_str) == Some(crate::types::STD_MODULE)
            || (self.ns.len() >= item_ns.len() && self.ns[..item_ns.len()] == *item_ns)
    }

    /// Whether an associated function/method `af` of `type_fq` is reachable
    /// from the current namespace: public, or in scope (declared in the
    /// same module or an ancestor of it) — mirrors `resolve_module_value_path`'s
    /// cross-module `public` check. Used at every call site that looks up
    /// `def.assoc` directly (`try_field_access`, `check_field_set`,
    /// `resolve_name`'s type-member step, `receiver_method`,
    /// `receiver_method`) so a private member — including a
    /// `defstruct` field whose accessor wasn't declared `pub` — is treated
    /// as absent rather than merely forbidden, matching how a private free
    /// function or constructor "doesn't resolve" instead of erroring with a
    /// privacy-specific message.
    pub(super) fn assoc_visible(&self, type_fq: &Path, af: &AssocFn) -> bool {
        af.sig.public || self.in_scope(&type_fq.parent())
    }

    /// Locate a child-module `path`, resolving it relative to the current
    /// namespace first, then the root, then via `mod_aliases`. Handles absolute
    /// paths (leading empty segment) by going straight to root.
    /// Returns `(absolute_segments, &Namespace)`.
    pub(super) fn find_module(&self, path: &[String]) -> Option<(Vec<String>, &Namespace)> {
        let (is_abs, eff) = self.split_abs(path);
        if is_abs {
            return self.reg.module(eff).map(|m| (eff.to_vec(), m));
        }

        // 1. Child of the current namespace.
        let mut abs = self.ns.clone();
        abs.extend_from_slice(eff);
        if let Some(m) = self.reg.module(&abs) {
            return Some((abs, m));
        }
        // 2. Child of root.
        if let Some(m) = self.reg.module(eff) {
            return Some((eff.to_vec(), m));
        }
        // 2b. Child of the standard library's module — a library macro's
        // expansion spells its helpers `%internal::name` wherever it lands.
        let mut in_std = STD_SEGMENTS.clone();
        in_std.extend_from_slice(eff);
        if let Some(m) = self.reg.module(&in_std) {
            return Some((in_std, m));
        }
        // 3. Module alias: expand the first segment via mod_aliases.
        if !eff.is_empty() {
            let first = &eff[0];
            let rest = &eff[1..];
            let alias = self
                .cur_ns()
                .mod_aliases
                .get(first)
                .or_else(|| self.reg.root().mod_aliases.get(first));
            if let Some(base) = alias {
                let mut expanded = base.clone();
                expanded.extend_from_slice(rest);
                if let Some(m) = self.reg.module(&expanded) {
                    return Some((expanded, m));
                }
            }
        }
        // 4. Sibling of the current *file* (not `self.ns`, which may be
        // deeper than the file's own path inside a nested `(module ...)`
        // block — see `file_ns`'s doc comment): a file in the same directory
        // as the one currently being checked, reached without spelling out
        // its full root-relative path. Tried last so it never shadows an
        // existing root-relative/current-namespace/alias resolution — see
        // `docs/dev/language-design.md` §2.4.
        if let Some(cur_file) = self.file_ns.last() {
            if !cur_file.is_empty() {
                let mut sib = cur_file[..cur_file.len() - 1].to_vec();
                sib.extend_from_slice(eff);
                if let Some(m) = self.reg.module(&sib) {
                    return Some((sib, m));
                }
            }
        }
        None
    }

    /// A `use` alias for a bare `name`, looked up current namespace then root.
    pub(super) fn lookup_alias(&self, name: &str) -> Option<Vec<String>> {
        if let Some(p) = self.cur_ns().aliases.get(name) {
            return Some(p.clone());
        }
        self.reg.root().aliases.get(name).cloned()
    }

    /// What a bare module-level name means — a function, a global variable
    /// or a macro, never two of them ([`Checker::check_value_clash`]) — with its
    /// absolute path, walking the current namespace's ancestor chain (see
    /// [`Self::ns_ancestors`]), nearest module first. A name found anywhere on
    /// that chain is in scope without needing `pub`, exactly like a
    /// same-module reference.
    ///
    /// The one walk all three kinds share: a nearer definition hides a
    /// further one of any kind, because a name has one meaning whichever kind
    /// it is. A function in a module therefore hides a root macro of the same
    /// name inside that module.
    ///
    /// A `use` import comes after this module's own definition of the name and
    /// before everything further out: an import never beats a definition made
    /// here, which is why `check_use` warns that such an import does nothing.
    /// An import that names no function, global or macro (it names a type)
    /// leaves the walk going, as [`Self::resolve_bare_type`] does for the
    /// other name space.
    pub(super) fn resolve_module_entry(&self, name: &str) -> Option<(Path, ModuleEntry)> {
        if let Some(entry) = module_entry_in(self.cur_ns(), name) {
            return Some((item_path(&self.ns, name), entry));
        }
        if let Some(path) = self.lookup_alias(name) {
            if let Some(found) = self.resolve_module_entry_path(&path) {
                return Some(found);
            }
        }
        self.find_on_chain(|prefix, m| module_entry_in(m, name).map(|entry| (item_path(prefix, name), entry)))
    }

    /// [`Self::resolve_module_entry`] for a qualified `module::...::name`.
    pub(super) fn resolve_module_entry_path(&self, segs: &[String]) -> Option<(Path, ModuleEntry)> {
        self.find_in_module(segs, |m, n| module_entry_in(m, n).map(|entry| (entry.public(), entry)))
    }

    /// What a bare module-level name holds, if it is a function or a global
    /// variable — see [`Self::resolve_module_entry`]. `None` when the nearest
    /// definition is a macro, which hides any value further out.
    pub(super) fn resolve_module_value(&self, name: &str) -> Option<ModuleValue> {
        match self.resolve_module_entry(name)? {
            (path, ModuleEntry::Value(kind)) => Some(kind.at(path)),
            (_, ModuleEntry::Macro { .. }) => None,
        }
    }

    /// [`Self::resolve_module_value`] for a qualified `module::...::name`.
    pub(super) fn resolve_module_value_path(&self, segs: &[String]) -> Option<ModuleValue> {
        match self.resolve_module_entry_path(segs)? {
            (path, ModuleEntry::Value(kind)) => Some(kind.at(path)),
            (_, ModuleEntry::Macro { .. }) => None,
        }
    }

    /// The function a bare name holds, if what it holds is a function — see
    /// [`Self::resolve_module_value`].
    pub(super) fn resolve_fn(&self, name: &str) -> Option<Path> {
        match self.resolve_module_value(name)? {
            ModuleValue::Fn(path) => Some(path),
            ModuleValue::Var(..) => None,
        }
    }

    /// Resolve a bare macro name to its absolute [`Path`] and the call-site
    /// arity shape ([`MacroShape`]): a `macrolet` binding, or else the
    /// module-level name, if the nearest definition of it is a macro
    /// ([`Self::resolve_module_entry`]).
    pub(super) fn resolve_macro(&self, name: &str) -> Option<(Path, MacroShape)> {
        // `macrolet` first, innermost scope first: a local macro shadows a
        // global one of the same name for the extent of its body, which is
        // what "lexically scoped macro definition" means.
        for scope in self.local_macros.borrow().iter().rev() {
            if let Some(found) = scope.get(name) {
                return Some(found.clone());
            }
        }
        match self.resolve_module_entry(name)? {
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

    /// Resolve a module-qualified macro name (`mod::macro-name`) to its
    /// absolute [`Path`] and call-site arity shape ([`MacroShape`]) — the
    /// path-qualified counterpart of [`Self::resolve_macro`].
    pub(super) fn resolve_macro_path(&self, segs: &[String]) -> Option<(Path, MacroShape)> {
        match self.resolve_module_entry_path(segs)? {
            (path, ModuleEntry::Macro { shape, .. }) => Some((path, shape)),
            (_, ModuleEntry::Value(_)) => None,
        }
    }

    /// Resolve a bare constructor name to its `(type path, variant index)`,
    /// walking the ancestor chain (see [`Self::ns_ancestors`]) — every ctor
    /// found this way is in scope by construction, so no separate `public`
    /// check is needed (mirrors [`Self::resolve_fn`]).
    pub(super) fn resolve_ctor(&self, name: &str) -> Option<(Path, usize)> {
        self.find_on_chain(|_, m| m.ctors.get(name).cloned())
    }

    /// The `Sexpr` `cons` variant's `(type path, variant index)`, read from the
    /// type definition directly rather than the bare-name `ctors` map — that
    /// bare `cons` name was removed (Symbol/Sexpr redesign Phase 4b) so `(cons
    /// a b)` resolves to the free `cons<T,U>` pair function. Internal `Sexpr`
    /// cons construction (`list` literals, quasiquote) still needs the variant,
    /// and reaches it through here.
    pub(super) fn sexpr_cons_ctor(&self) -> (Path, usize) {
        let path = Path::root("sexpr");
        let def = self.reg.type_def(&path).expect("sexpr is built in");
        let idx = def
            .variants
            .iter()
            .position(|v| v.name == "cons")
            .expect("sexpr has a cons variant");
        (path, idx)
    }

    /// Resolve a bare name snapshotted by `(use Type)` (see `Self::check_use`)
    /// to the `(type path, method name)` of the static associated function it
    /// names — the static-method counterpart of [`Self::resolve_ctor`],
    /// checked the same way (current namespace then root).
    pub(super) fn resolve_static_use(&self, name: &str) -> Option<(Path, String)> {
        if let Some(hit) = self.cur_ns().static_uses.get(name) {
            return Some(hit.clone());
        }
        self.reg.root().static_uses.get(name).cloned()
    }

    /// Resolve a bare (unqualified) type name: this module's own type of that
    /// name, then a `use` alias, then the rest of the ancestor chain (see
    /// [`Self::ns_ancestors`]) — the order [`Self::resolve_module_value`]
    /// gives a function or a global. Every place a bare type
    /// name is written goes through here — a type annotation
    /// ([`Self::resolve_type_name`]), the `Type` of `Type::member`
    /// ([`Self::resolve_type_path`] with no module qualifier), a pattern head,
    /// `compile`/`documentation` of `Type::method` — so a type brought in by
    /// `(use m::point)` is the same `point` in all of them.
    ///
    /// An alias that names something other than a type (`(use m::f)`) does
    /// not answer the question, and the chain is searched as for any name.
    pub(super) fn resolve_bare_type(&self, name: &str) -> Option<Path> {
        if let Some(def) = self.cur_ns().types.get(name) {
            return Some(def.name.clone());
        }
        if let Some(target) = self.lookup_alias(name) {
            // A one-segment target is a root-level name; resolving it through
            // `resolve_type_path` would come back here and consult the same
            // alias again.
            let found = match target.as_slice() {
                [only] => self.resolve_unaliased_bare_type(only),
                _ => self.resolve_type_path(&target),
            };
            if found.is_some() {
                return found;
            }
        }
        self.resolve_unaliased_bare_type(name)
    }

    /// [`Self::resolve_bare_type`] without the `use` alias: the ancestor
    /// chain alone.
    pub(super) fn resolve_unaliased_bare_type(&self, name: &str) -> Option<Path> {
        self.find_on_chain(|_, m| m.types.get(name).map(|def| def.name.clone()))
    }

    /// Resolve a `module::...::Type` segment path to the type's [`Path`].
    pub(super) fn resolve_type_path(&self, segs: &[String]) -> Option<Path> {
        let (local, mods) = segs.split_last()?;
        if mods.is_empty() {
            // No module qualifier — `find_module(&[])` would just hand back
            // the current namespace itself (its loop never runs), missing
            // every ancestor including root; walk the chain instead.
            return self.resolve_bare_type(local);
        }
        self.find_in_module(segs, |m, n| m.types.get(n).map(|def| (def.public, def.name.clone()))).map(|(_, name)| name)
    }

    /// Resolve a written type name to a `deftype` alias, if it names one.
    ///
    /// Searched exactly like a type ([`Self::resolve_bare_type`] /
    /// [`Self::resolve_type_path`]), including `use` aliases and the
    /// `pub`-visibility rule for a qualified name — an alias is a spelling
    /// for a type and is reached the same ways one is.
    pub(super) fn resolve_type_alias(&self, path: &Path) -> Option<TypeAlias> {
        if path.is_simple() {
            let name = path.last_segment();
            if let Some(a) = self.cur_ns().type_aliases.get(name) {
                return Some(a.clone());
            }
            if let Some(target) = self.lookup_alias(name) {
                if let Some(a) = self.resolve_type_alias_path(&target) {
                    return Some(a);
                }
            }
            return self.find_on_chain(|_, m| m.type_aliases.get(name).cloned());
        }
        self.resolve_type_alias_path(&path.segments())
    }

    /// [`Self::resolve_type_alias`] for a `module::...::Name` segment path.
    pub(super) fn resolve_type_alias_path(&self, segs: &[String]) -> Option<TypeAlias> {
        let (local, mods) = segs.split_last()?;
        if mods.is_empty() {
            return self.find_on_chain(|_, m| m.type_aliases.get(local).cloned());
        }
        self.find_in_module(segs, |m, n| m.type_aliases.get(n).map(|a| (a.public, a.clone()))).map(|(_, a)| a)
    }

    /// Resolve a nominal type [`Path`] to its canonical (located) form if a
    /// matching type exists; otherwise (type variables, unknown names) return it
    /// unchanged.
    pub(super) fn resolve_type_name(&self, path: &Path) -> Path {
        if path.is_simple() {
            return self.resolve_bare_type(path.last_segment()).unwrap_or_else(|| path.clone());
        }
        self.resolve_type_path(&path.segments()).unwrap_or_else(|| path.clone())
    }

    /// The global a bare name holds, if what it holds is a variable — see
    /// [`Self::resolve_module_value`].
    pub(super) fn resolve_global(&self, name: &str) -> Option<(Path, VarInfo)> {
        match self.resolve_module_value(name)? {
            ModuleValue::Var(path, vi) => Some((path, vi)),
            ModuleValue::Fn(_) => None,
        }
    }

    /// Resolve a `deftrait`-defined trait's bare name to its fully-qualified
    /// [`Path`] — mirrors `Checker::resolve_type_name`, but for the separate
    /// `traits` table (traits aren't types and don't share its namespace).
    pub(super) fn resolve_trait_name(&self, name: &str) -> Result<Path, Error> {
        let mut ns = self.ns.clone();
        loop {
            if let Some(td) = self.reg.module(&ns).and_then(|m| m.traits.get(name)) {
                return Ok(td.name.clone());
            }
            if ns.is_empty() {
                break;
            }
            ns.pop();
        }
        // The standard library's module last, as `Self::ns_ancestors` has it.
        if let Some(td) = self.reg.module(&STD_SEGMENTS).and_then(|m| m.traits.get(name)) {
            return Ok(td.name.clone());
        }
        Err(Error::TypeError(format!("impl: unknown trait `{}`", name)))
    }

    /// [`Self::resolve_trait_name`] for a written [`Path`], returning `None`
    /// instead of an error: a bare name searches the ancestor module chain
    /// (the same walk), a qualified one is looked up in the named module.
    /// Used by `canon` to normalize a [`Type::Dyn`]'s trait head, which must
    /// not fail — an unresolvable trait is reported later, with a message
    /// about the trait object rather than about an `impl`.
    pub(super) fn resolve_trait_path(&self, p: &Path) -> Option<Path> {
        if p.is_simple() {
            return self.resolve_trait_name(p.last_segment()).ok();
        }
        self.reg
            
            .module(&p.parent())
            .and_then(|m| m.traits.get(p.last_segment()))
            .map(|td| td.name.clone())
    }
}
