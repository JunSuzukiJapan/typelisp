//! The type checker: lowers read `Sexpr` values into a typed AST while checking.
//!
//! Checking is bidirectional in a small way: literal and constructor nodes may
//! be checked *against* an expected type (so integer literals adopt the expected
//! integer type and a nullary `None` learns its type argument), while everything
//! else synthesizes its own type and is reconciled against the expectation.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::{parse_type, prim_type_path, Error, Heap, Path, Type, Value};

use super::ast::{Arm, Expr, Pattern, QuotedSexpr, Typed};
use super::registry::{AdtDef, AdtKind, AssocFn, FnSig, MacroDef, Namespace, Registry, VarInfo, Variant};

/// Expands a macro call *during* type-checking: `path` names a `defmacro`,
/// `raw_args` are the call's unevaluated argument forms (exactly as written —
/// this is what makes macros unhygienic/CL-style: no implicit quoting
/// conversion is needed since `Sexpr`'s runtime representation already *is*
/// the reader's raw `Value`). Returns the expansion to check in place of the
/// original call.
///
/// Defined here (in `check`, not `eval`) and implemented by `Interp` in
/// `eval/interp.rs`, so this module — pure static analysis otherwise — never
/// has to name the concrete `Interp` type. `eval` already depends on
/// `check`'s typed AST (`Expr`/`TopLevel`/`Typed`), so a direct `use
/// crate::eval::Interp` here would make the two modules mutually dependent;
/// this trait keeps that dependency one-directional.
pub trait MacroExpander {
    fn expand_macro(&self, heap: &mut Heap, path: &Path, raw_args: Vec<Value>) -> Result<Value, String>;
}

/// A checked top-level form.
#[derive(Clone, Debug, PartialEq)]
pub enum TopLevel {
    /// A `defun`: fully-qualified [`Path`], typed parameters, return type, body.
    /// `type_params` are the names declared by `(defun (name T1 T2...) ...)`
    /// (empty for an ordinary, non-generic function) — the evaluator ignores
    /// them (type information is erased before execution; see
    /// `Checker::check_call`).
    /// `params` includes a trailing `&rest` parameter's name last (bound to
    /// a plain `Sexpr` list), exactly like `Defmacro::params` — see
    /// `Checker::check_defun`/`parse_params_rest`.
    Defun {
        name: Path,
        type_params: Vec<String>,
        params: Vec<(String, Type)>,
        ret: Type,
        body: Vec<Typed>,
    },
    /// A `defmethod`: instance or static associated function of a type.
    Defmethod {
        type_name: Path,
        method: String,
        instance: bool,
        /// The receiver's variable name (e.g. `self`) for instance methods.
        self_name: Option<String>,
        params: Vec<(String, Type)>,
        ret: Type,
        body: Vec<Typed>,
    },
    /// A `defmacro`: a compile-time, `Sexpr`-to-`Sexpr` code transformer.
    /// Stored as an ordinary callable body — calling it (at macro-expansion
    /// time, via [`MacroExpander`]) is identical to calling a `defun`. `params`
    /// includes the `&rest` parameter's name last (with the `&rest` marker
    /// itself dropped) when `rest` is true; see [`Checker::check_defmacro`].
    Defmacro { name: Path, params: Vec<String>, body: Vec<Typed>, rest: bool },
    /// A `defvar`/`defconstant`: a global variable (`mutable`) or constant.
    Defvar { name: Path, ty: Type, value: Typed, mutable: bool },
    /// A `module`: a namespace and its checked body forms.
    Module { path: Path, body: Vec<TopLevel> },
    /// A `use`: a name brought into the current scope (alias -> target path).
    Use { alias: Path, target: Path },
    /// A `defstruct`: registers a single-variant `AdtKind::Struct` type
    /// (`Checker::check_defstruct`). Carries only the name — unlike
    /// `Defun`/`Defmethod`, there's no body to run; the registry mutation
    /// already happened at check time, and the constructor/field accessors
    /// are ordinary `Type::ctor`/`defmethod` machinery reached through
    /// `Expr::Construct`/`Assoc` at use sites, not through this node.
    /// `Interp::exec` treats it as a no-op, the same as `Option`/`Result`
    /// needing no runtime registration of their own.
    Defstruct { name: Path },
    /// A bare top-level expression.
    Expr(Typed),
}

/// The result of [`Checker::parse_params_rest`]: fixed `(name, type)` params,
/// plus the `&rest` parameter's `(name, elem-type)` if the list ends in one.
type ParamsAndRest = (Vec<(String, Type)>, Option<(String, Type)>);

/// A lexical environment mapping variable names to their types.
#[derive(Clone)]
struct Env {
    vars: Vec<(String, Type)>,
}

impl Env {
    fn new() -> Env {
        Env { vars: Vec::new() }
    }

    fn get(&self, name: &str) -> Option<&Type> {
        self.vars.iter().rev().find(|(n, _)| n == name).map(|(_, t)| t)
    }

    /// A child environment with `binds` added (later bindings shadow earlier).
    fn extended(&self, binds: Vec<(String, Type)>) -> Env {
        let mut vars = self.vars.clone();
        vars.extend(binds);
        Env { vars }
    }
}

/// Bundles [`Checker::check_assoc_call`]'s arguments to keep its arity down.
struct AssocCall<'a> {
    type_fq: &'a Path,
    method: &'a str,
    /// `Some` for an instance call (already-checked receiver, fills the
    /// method's first parameter slot); `None` for a static call.
    receiver: Option<Typed>,
    /// The call site's expected type. Only consulted for a static call with
    /// no receiver to read concrete type arguments from (e.g. `HashTable::new`
    /// learning `K`/`V` the same way a field-less constructor like `None`
    /// learns its type argument from `expected`).
    expected: Option<&'a Type>,
}

/// What happens when a `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`
/// reuses a name already defined by *non-builtin* code (the same module, in
/// the same namespace). Redefining a builtin (`with_builtins`-registered) is
/// always an error, regardless of this setting — see [`Checker::check_redef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RedefPolicy {
    /// Allow the redefinition, but record a warning (`Checker::take_warnings`).
    #[default]
    Warn,
    /// Reject the redefinition with a `TypeError`.
    Error,
    /// Allow the redefinition silently.
    Silent,
}

/// Anything that can occupy a name in one of a [`Namespace`]'s tables (or a
/// type's `assoc` table), so [`Checker::check_redef`] can be written once and
/// shared by every `check_def*` instead of duplicating the warn/error/silent
/// decision at each call site.
trait Definable {
    fn builtin(&self) -> bool;
}
impl Definable for FnSig {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for MacroDef {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for VarInfo {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for AdtDef {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for AssocFn {
    fn builtin(&self) -> bool {
        self.builtin
    }
}

/// The type checker, holding the data-type and function registries plus the
/// current namespace (module) path.
pub struct Checker {
    reg: Registry,
    ns: Vec<String>,
    /// Stack of enclosing loops' accumulated exit type, innermost last.
    /// `break`/`return` unify their (optional) value's type into the top
    /// frame; `while`/`dotimes`/`dolist` seed it with `Unit` (their fixed
    /// result type), `loop` seeds it with `Never` (refined by any exit found).
    /// `lambda` bodies see an empty stack — `break`/`return` cannot cross a
    /// function boundary (there is no labelled non-local exit in this
    /// language, only "nearest enclosing loop").
    loop_stack: RefCell<Vec<Type>>,
    /// What to do when a `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`
    /// reuses a non-builtin name — see [`RedefPolicy`]. Defaults to `Warn`;
    /// override with [`Self::set_redef_policy`] (e.g. from a CLI flag).
    redef_policy: RedefPolicy,
    /// Non-fatal diagnostics accumulated by [`Self::check_redef`] (currently
    /// just `RedefPolicy::Warn` redefinitions); drained by [`Self::take_warnings`].
    warnings: RefCell<Vec<String>>,
}

impl Checker {
    pub fn new() -> Checker {
        Checker {
            reg: Registry::with_builtins(),
            ns: Vec::new(),
            loop_stack: RefCell::new(Vec::new()),
            redef_policy: RedefPolicy::default(),
            warnings: RefCell::new(Vec::new()),
        }
    }

    /// Overrides the default [`RedefPolicy`] (`Warn`) for non-builtin
    /// redefinitions. Redefining a builtin is always an error regardless.
    pub fn set_redef_policy(&mut self, policy: RedefPolicy) {
        self.redef_policy = policy;
    }

    /// Drains and returns every warning recorded so far (e.g. by a
    /// `RedefPolicy::Warn` redefinition) — call after each `check_form` to
    /// surface them (see `main.rs`/`prelude.rs`).
    pub fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.borrow_mut())
    }

    /// The one place the warn/error/silent (and "never touch a builtin")
    /// decision actually lives, given only whether the name already in use
    /// is a builtin. [`Self::check_redef`] (the `Definable`-generic path used
    /// by `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`) and
    /// `check_use`'s constructor/static-method snapshot (whose tables,
    /// `Namespace::ctors`/`static_uses`, are plain `(Path, ..)` tuples with
    /// no `Definable` impl of their own — "builtin" there means "owned by a
    /// builtin type") both reduce to calling this.
    fn check_redef_outcome(&self, kind: &str, name: &str, is_builtin: bool) -> Result<(), Error> {
        if is_builtin {
            return Err(Error::TypeError(format!("cannot redefine built-in {} `{}`", kind, name)));
        }
        match self.redef_policy {
            RedefPolicy::Error => Err(Error::TypeError(format!("{} `{}` is already defined", kind, name))),
            RedefPolicy::Warn => {
                self.warnings.borrow_mut().push(format!("warning: redefining {} `{}`", kind, name));
                Ok(())
            }
            RedefPolicy::Silent => Ok(()),
        }
    }

    /// The one place that decides whether a `defun`/`defmethod`/`defmacro`/
    /// `defvar`/`defstruct` may reuse `name` (already a `kind`, e.g.
    /// `"function"`/`"type"`), given whatever already occupies that name in
    /// the table it's about to be inserted into (`None` if the name is free).
    /// A builtin is never redefinable; anything else follows `redef_policy`.
    fn check_redef<T: Definable>(&self, kind: &str, name: &str, existing: Option<&T>) -> Result<(), Error> {
        match existing {
            None => Ok(()),
            Some(e) => self.check_redef_outcome(kind, name, e.builtin()),
        }
    }

    /// Check one top-level form. Definition forms (`defun`/`module`/
    /// `defmethod`/`use`) register into the current namespace; anything else is
    /// checked as an expression.
    pub fn check_form(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value) -> Result<TopLevel, Error> {
        if let Value::Cons(_) = v {
            let elems = heap.list_to_vec(v)?;
            if let Some(Value::Symbol(id)) = elems.first() {
                match heap.symbol_name(*id) {
                    "pub" => return self.check_pub(heap, interp, &elems[1..]),
                    "defun" => return self.check_defun(heap, interp, &elems[1..], false),
                    "defvar" => return self.check_defvar(heap, interp, &elems[1..], true, false),
                    "defconstant" => return self.check_defvar(heap, interp, &elems[1..], false, false),
                    "defmacro" => return self.check_defmacro(heap, interp, &elems[1..], false),
                    "module" => return self.check_module(heap, interp, &elems[1..]),
                    "defmethod" => return self.check_defmethod(heap, interp, &elems[1..], false),
                    "defstruct" => return self.check_defstruct(heap, &elems[1..], false),
                    "use" => return self.check_use(heap, &elems[1..]),
                    _ => {}
                }
            }
        }
        let env = Env::new();
        let t = self.check(heap, interp, &env, v, None)?;
        Ok(TopLevel::Expr(t))
    }

    /// The type/constructor registry — exposed so a caller (e.g. the REPL)
    /// can resolve an ADT variant's constructor name when printing a
    /// `RtValue::Data` result.
    pub fn registry(&self) -> &Registry {
        &self.reg
    }

    /// `(pub defun ...)` / `(pub defmethod ...)` etc. — mark the next definition public.
    fn check_pub(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("pub: expected a definition form".into()));
        }
        if let Value::Symbol(id) = parts[0] {
            match heap.symbol_name(id) {
                "defun" => return self.check_defun(heap, interp, &parts[1..], true),
                "defvar" => return self.check_defvar(heap, interp, &parts[1..], true, true),
                "defconstant" => return self.check_defvar(heap, interp, &parts[1..], false, true),
                "defmacro" => return self.check_defmacro(heap, interp, &parts[1..], true),
                "defmethod" => return self.check_defmethod(heap, interp, &parts[1..], true),
                "defstruct" => return self.check_defstruct(heap, &parts[1..], true),
                _ => {}
            }
        }
        Err(Error::TypeError("pub: expected defun/defmacro/defmethod/defstruct/defvar/defconstant".into()))
    }

    // ---- namespace navigation & resolution --------------------------------
    //
    // Identities are structured `Path`s, never joined `"a::b"` strings: the only
    // `::`-string decoding lives in the reader/parser. Resolution navigates the
    // namespace tree, current namespace first then root.

    /// Qualify a bare name with the current namespace path.
    fn fq(&self, name: &str) -> Path {
        let mut segments = self.ns.clone();
        segments.push(name.to_string());
        Path::from_segments(segments)
    }

    /// The current namespace.
    fn cur_ns(&self) -> &Namespace {
        self.reg.root.module(&self.ns).expect("current namespace exists")
    }

    /// If `path` starts with an empty segment (reader-encoded `::foo` absolute
    /// path), strip it and return `(true, rest)`.  Otherwise `(false, path)`.
    fn split_abs<'a>(&self, path: &'a [String]) -> (bool, &'a [String]) {
        if path.first().map(|s| s.is_empty()).unwrap_or(false) {
            (true, &path[1..])
        } else {
            (false, path)
        }
    }

    /// Whether `item_ns` is the same module as the current namespace (i.e.
    /// same-module access; no cross-module visibility check needed).
    fn same_module(&self, item_ns: &[String]) -> bool {
        item_ns == self.ns.as_slice()
    }

    /// Locate a child-module `path`, resolving it relative to the current
    /// namespace first, then the root, then via `mod_aliases`. Handles absolute
    /// paths (leading empty segment) by going straight to root.
    /// Returns `(absolute_segments, &Namespace)`.
    fn find_module(&self, path: &[String]) -> Option<(Vec<String>, &Namespace)> {
        let (is_abs, eff) = self.split_abs(path);
        if is_abs {
            return self.reg.root.module(eff).map(|m| (eff.to_vec(), m));
        }

        // 1. Child of the current namespace.
        if let Some(m) = self.cur_ns().module(eff) {
            let mut abs = self.ns.clone();
            abs.extend_from_slice(eff);
            return Some((abs, m));
        }
        // 2. Child of root.
        if let Some(m) = self.reg.root.module(eff) {
            return Some((eff.to_vec(), m));
        }
        // 3. Module alias: expand the first segment via mod_aliases.
        if !eff.is_empty() {
            let first = &eff[0];
            let rest = &eff[1..];
            let alias = self
                .cur_ns()
                .mod_aliases
                .get(first)
                .or_else(|| self.reg.root.mod_aliases.get(first));
            if let Some(base) = alias {
                let mut expanded = base.clone();
                expanded.extend_from_slice(rest);
                if let Some(m) = self.reg.root.module(&expanded) {
                    return Some((expanded, m));
                }
            }
        }
        None
    }

    /// A `use` alias for a bare `name`, looked up current namespace then root.
    fn lookup_alias(&self, name: &str) -> Option<Vec<String>> {
        if let Some(p) = self.cur_ns().aliases.get(name) {
            return Some(p.clone());
        }
        self.reg.root.aliases.get(name).cloned()
    }

    /// Resolve a bare free-function name to its absolute [`Path`].
    fn resolve_fn(&self, name: &str) -> Option<Path> {
        if let Some(path) = self.lookup_alias(name) {
            return self.resolve_fn_path(&path);
        }
        // Same module — always accessible.
        if self.cur_ns().fns.contains_key(name) {
            return Some(self.fq(name));
        }
        // Root namespace — cross-module if we are inside a submodule.
        if let Some(sig) = self.reg.root.fns.get(name) {
            if sig.public || self.ns.is_empty() {
                return Some(Path::root(name));
            }
        }
        None
    }

    /// Resolve a qualified `module::...::fn` path to its absolute [`Path`].
    fn resolve_fn_path(&self, segs: &[String]) -> Option<Path> {
        if segs.is_empty() {
            return None;
        }
        let (mods, last) = segs.split_at(segs.len() - 1);
        let (abs, m) = self.find_module(mods)?;
        if let Some(sig) = m.fns.get(&last[0]) {
            if !sig.public && !self.same_module(&abs) {
                return None;
            }
            let mut full = abs;
            full.push(last[0].clone());
            return Some(Path::from_segments(full));
        }
        None
    }

    /// Resolve a bare macro name to its absolute [`Path`], fixed arity, and
    /// whether it's variadic (`&rest`) (current namespace then root — same
    /// priority as [`Self::resolve_fn`], but without `use`-alias support,
    /// which `defmacro` doesn't have yet).
    fn resolve_macro(&self, name: &str) -> Option<(Path, usize, bool)> {
        if let Some(def) = self.cur_ns().macros.get(name) {
            return Some((self.fq(name), def.arity, def.rest));
        }
        if let Some(def) = self.reg.root.macros.get(name) {
            if def.public || self.ns.is_empty() {
                return Some((Path::root(name), def.arity, def.rest));
            }
        }
        None
    }

    /// Resolve a bare constructor name to its `(type path, variant index)`,
    /// searching the current namespace then root.
    fn resolve_ctor(&self, name: &str) -> Option<(Path, usize)> {
        // Same module — always accessible.
        if let Some(hit) = self.cur_ns().ctors.get(name) {
            return Some(hit.clone());
        }
        // Root — check type visibility for cross-module access.
        if let Some(hit) = self.reg.root.ctors.get(name) {
            if !self.ns.is_empty() {
                let (type_path, _) = hit;
                if let Some(def) = self.reg.type_def(type_path) {
                    if !def.public {
                        return None;
                    }
                }
            }
            return Some(hit.clone());
        }
        None
    }

    /// Resolve a bare name snapshotted by `(use Type)` (see `Self::check_use`)
    /// to the `(type path, method name)` of the static associated function it
    /// names — the static-method counterpart of [`Self::resolve_ctor`],
    /// checked the same way (current namespace then root).
    fn resolve_static_use(&self, name: &str) -> Option<(Path, String)> {
        if let Some(hit) = self.cur_ns().static_uses.get(name) {
            return Some(hit.clone());
        }
        self.reg.root.static_uses.get(name).cloned()
    }

    /// Wraps an already-checked `&rest` element `e` (statically `elem_ty`,
    /// already resolved to a concrete type — see [`Self::cons_rest_list`])
    /// as a `Sexpr` value, so a sequence of them can be `cons`-ed into a
    /// plain list — see [`sexpr_ty`]'s doc comment. `elem_ty` itself needs no
    /// wrapping (already `Sexpr`); every other supported element type goes
    /// through its own `Sexpr` constructor (`sexpr_ctor_for`) exactly as if
    /// the caller had written e.g. `(Int e)` by hand — `construct_sexpr`
    /// (`crate::eval::interp`) only ever reads the field's *runtime* value
    /// (an `i32`/`i64` argument both evaluate to the same `RtValue::Int`),
    /// so this never goes through the normal field-type validation
    /// `check_construct` would otherwise apply, but is sound for the same
    /// reason. An `elem_ty` with no `Sexpr` encoding (e.g. `Option<T>`) is a
    /// `TypeError` here, not a panic — unlike most of `&rest`'s checking,
    /// this can't happen any earlier than this, since a generic `&rest`'s
    /// declared type may only resolve to something concrete at a given call
    /// site (see `Self::check_call`'s `subst_apply`).
    fn wrap_rest_elem(&self, elem_ty: &Type, e: Typed) -> Result<Typed, Error> {
        if *elem_ty == sexpr_ty() {
            return Ok(e);
        }
        let ctor = sexpr_ctor_for(elem_ty).ok_or_else(|| {
            Error::TypeError(format!(
                "&rest: element type {:?} has no Sexpr encoding (use one of i32/i64/f64/bool/char/string/Sexpr)",
                elem_ty
            ))
        })?;
        let (type_name, variant) = self.resolve_ctor(ctor).expect("sexpr constructors are always registered");
        Ok(Typed { expr: Expr::Construct { type_name, variant, args: vec![e], mutable: false }, ty: sexpr_ty() })
    }

    /// Collects already-checked `&rest` elements into one `Sexpr` list
    /// expression, back-to-front (mirroring `Interp::bind_macro_args`'s
    /// runtime construction of `defmacro`'s own `&rest` list) — `(cons
    /// (wrap e1) (cons (wrap e2) ... ()))`. The empty case is the `Nil`
    /// literal, reusing the same `Expr::Quote(QuotedSexpr::Nil)` shape `()`
    /// itself checks to.
    fn cons_rest_list(&self, elem_ty: &Type, items: Vec<Typed>) -> Result<Typed, Error> {
        let cons_path = Path::root("cons");
        items.into_iter().rev().try_fold(Typed { expr: Expr::Quote(QuotedSexpr::Nil), ty: sexpr_ty() }, |acc, item| {
            let item = self.wrap_rest_elem(elem_ty, item)?;
            Ok(Typed { expr: Expr::Call(cons_path.clone(), vec![item, acc]), ty: sexpr_ty() })
        })
    }

    /// Resolve a `module::...::Type` segment path to the type's [`Path`].
    fn resolve_type_path(&self, segs: &[String]) -> Option<Path> {
        if segs.is_empty() {
            return None;
        }
        let (mods, local) = segs.split_at(segs.len() - 1);
        let (abs, m) = self.find_module(mods)?;
        if let Some(def) = m.types.get(&local[0]) {
            if !def.public && !self.same_module(&abs) {
                return None;
            }
            return Some(def.name.clone());
        }
        None
    }

    /// Resolve a nominal type [`Path`] to its canonical (located) form if a
    /// matching type exists; otherwise (type variables, unknown names) return it
    /// unchanged.
    fn resolve_type_name(&self, path: &Path) -> Path {
        if path.is_simple() {
            let name = path.local();
            if let Some(target) = self.lookup_alias(name) {
                if let Some(tp) = self.resolve_type_path(&target) {
                    return tp;
                }
            }
            if let Some(def) = self.cur_ns().types.get(name) {
                return def.name.clone();
            }
            // Root types: check public if cross-module.
            if let Some(def) = self.reg.root.types.get(name) {
                if def.public || self.ns.is_empty() {
                    return def.name.clone();
                }
            }
            return path.clone();
        }
        self.resolve_type_path(path.segments()).unwrap_or_else(|| path.clone())
    }

    /// Resolve a bare global variable/constant name to its `(path, info)`.
    fn resolve_global(&self, name: &str) -> Option<(Path, VarInfo)> {
        if let Some(p) = self.lookup_alias(name) {
            return self.resolve_global_path(&p);
        }
        // Same module — always accessible.
        if let Some(vi) = self.cur_ns().vars.get(name) {
            return Some((self.fq(name), vi.clone()));
        }
        // Root — check public for cross-module access.
        if let Some(vi) = self.reg.root.vars.get(name) {
            if vi.public || self.ns.is_empty() {
                return Some((Path::root(name), vi.clone()));
            }
        }
        None
    }

    /// Resolve a qualified `module::...::global` path to its `(path, info)`.
    fn resolve_global_path(&self, segs: &[String]) -> Option<(Path, VarInfo)> {
        if segs.is_empty() {
            return None;
        }
        let (mods, last) = segs.split_at(segs.len() - 1);
        let (abs, m) = self.find_module(mods)?;
        if let Some(vi) = m.vars.get(&last[0]) {
            if !vi.public && !self.same_module(&abs) {
                return None;
            }
            let mut full = abs;
            full.push(last[0].clone());
            return Some((Path::from_segments(full), vi.clone()));
        }
        None
    }

    /// Reify a bare free-function name as a function value (`FnRef`), if it names
    /// one.
    fn fn_value(&self, name: &str) -> Option<Typed> {
        let fq = self.resolve_fn(name)?;
        Some(self.fn_ref_node(fq))
    }

    /// Reify a qualified free-function path as a function value (`FnRef`).
    fn fn_path_value(&self, segs: &[String]) -> Option<Typed> {
        let fq = self.resolve_fn_path(segs)?;
        Some(self.fn_ref_node(fq))
    }

    /// Build an `FnRef` node carrying the function's `(fn ...)` type.
    fn fn_ref_node(&self, fq: Path) -> Typed {
        let sig = self.reg.fn_sig(&fq).expect("resolved fn exists").clone();
        let ty = Type::Fn(sig.params, sig.rest.map(Box::new), Box::new(sig.ret));
        Typed { expr: Expr::FnRef(fq), ty }
    }

    /// Reify a bare name as an *instance method* value (`MethodRef`), e.g.
    /// `+` where a `(fn (i32 i32) i32)` is expected (see
    /// [`builtin_as_value`](../../../tests/eval_test.rs)-style usage of
    /// `registry::int_assoc`'s arithmetic). Unlike a real call site, there is
    /// no receiver expression to dispatch on, so the receiver type is read
    /// off `expected`'s first parameter instead — this only resolves
    /// non-generic instance methods (the receiver's own type fully
    /// determines the method's signature with no substitution needed), which
    /// covers `i32`/`i64`/`f64`/`string`/`char`; a generic method (e.g.
    /// `HashTable::get`) can't be reified this way since there's no call-site
    /// type argument to substitute, and simply fails to match below.
    fn method_value(&self, name: &str, expected: Option<&Type>) -> Option<Typed> {
        let Type::Fn(params, _, _) = expected? else { return None };
        let type_fq = match params.first()? {
            Type::Named(n, _) => n.clone(),
            other => prim_type_path(other)?,
        };
        let def = self.reg.type_def(&type_fq)?;
        let af = def.assoc.get(name)?;
        if !af.instance {
            return None;
        }
        let ty = Type::Fn(af.sig.params.clone(), af.sig.rest.clone().map(Box::new), Box::new(af.sig.ret.clone()));
        if &ty != expected.unwrap() {
            return None;
        }
        Some(Typed { expr: Expr::MethodRef { type_name: type_fq, method: name.to_string() }, ty })
    }

    /// `var::field`: if `segs` is `[recv, method]` and `recv` names a bound
    /// local or global whose type has an *instance* associated function
    /// called `method` (in practice always a `defstruct` field accessor —
    /// see `Checker::check_defstruct` — though this doesn't care how the
    /// method came to exist), produces the same `Expr::Assoc` node a
    /// `(method recv)` call would. `None` (not an error) for any other
    /// shape, so the caller falls back to ordinary module/type-path
    /// resolution (`Checker::check`'s `Value::Path` case).
    /// Delegates to `Checker::check_assoc_call` (instead of building the
    /// `Expr::Assoc` node directly) so a *generic* `defstruct`'s field type
    /// gets the receiver's concrete type arguments substituted in exactly
    /// the way any other instance-method call already does — building the
    /// node by hand here once produced an unsubstituted type variable as
    /// the field's type (caught by `generic_defstruct_setf_works` in
    /// `tests/struct_test.rs`, since a bare type variable can't satisfy a
    /// real expected type downstream, even though the type-erased runtime
    /// value was already correct).
    fn try_field_access(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        segs: &[String],
    ) -> Option<Result<Typed, Error>> {
        let [recv_name, method] = segs else { return None };
        let recv = if let Some(t) = env.get(recv_name) {
            Typed { expr: Expr::Var(recv_name.clone()), ty: t.clone() }
        } else {
            let (path, vi) = self.resolve_global(recv_name)?;
            Typed { expr: Expr::Global(path), ty: vi.ty }
        };
        let Type::Named(type_fq, _) = &recv.ty else { return None };
        let af = self.reg.type_def(type_fq)?.assoc.get(method)?;
        if !af.instance {
            return None;
        }
        let type_fq = type_fq.clone();
        Some(self.check_assoc_call(
            heap,
            interp,
            env,
            AssocCall { type_fq: &type_fq, method, receiver: Some(recv), expected: None },
            &[],
        ))
    }

    /// Canonicalize a parsed type: resolve every nominal name to its located
    /// [`Path`] so that types compare equal across module boundaries.
    fn canon(&self, t: &Type) -> Type {
        match t {
            Type::Named(n, args) => Type::Named(
                self.resolve_type_name(n),
                args.iter().map(|a| self.canon(a)).collect(),
            ),
            Type::Fn(ps, rest, r) => Type::Fn(
                ps.iter().map(|p| self.canon(p)).collect(),
                rest.as_ref().map(|t| Box::new(self.canon(t))),
                Box::new(self.canon(r)),
            ),
            other => other.clone(),
        }
    }

    /// The lowercase segments of a module/use path (a symbol or a `Value::Path`).
    fn path_to_segs(&self, heap: &Heap, v: Value) -> Result<Vec<String>, Error> {
        match v {
            Value::Symbol(id) => Ok(vec![heap.symbol_name(id).to_string()]),
            Value::Path(id) => Ok(heap
                .path_segments(id)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect()),
            _ => Err(Error::TypeError("expected a name or `::` path".into())),
        }
    }

    // ---- defun ------------------------------------------------------------

    fn check_defun(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        public: bool,
    ) -> Result<TopLevel, Error> {
        if parts.len() < 3 {
            return Err(Error::TypeError("defun: (defun name (params) ret body...)".into()));
        }
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;
        let (params, rest) = self.parse_params_rest(heap, parts[1])?;
        let ret = self.canon(&parse_type(heap, parts[2])?);
        let fq_name = self.fq(&name);

        // Register the signature in the current namespace before checking the
        // body so self-recursion works.
        self.check_redef("function", &name, self.cur_ns().fns.get(&name))?;
        let sig = FnSig {
            type_params: type_params.clone(),
            params: params.iter().map(|(_, t)| t.clone()).collect(),
            ret: ret.clone(),
            public,
            rest: rest.as_ref().map(|(_, t)| t.clone()),
            builtin: false,
        };
        self.reg.root.module_mut(&self.ns).fns.insert(name.clone(), sig);

        // The body additionally sees the `&rest` parameter (if any) bound to
        // a plain `Sexpr` list collecting every variadic argument (see
        // `sexpr_ty`'s doc comment) — the call-site desugaring
        // (`Self::check_call`) always supplies exactly one such list as the
        // last actual argument, so the stored `params` below (consumed only
        // for its *names* by `Interp::exec`) already lines up 1:1 with
        // arguments at runtime; mirrors `TopLevel::Defmacro::params`
        // including its `&rest` name last.
        let mut params = params;
        if let Some((rname, _)) = &rest {
            params.push((rname.clone(), sexpr_ty()));
        }
        let env = Env::new().extended(params.clone());
        let (body, _) = self.check_seq(heap, interp, &env, &parts[3..], Some(&ret))?;
        Ok(TopLevel::Defun { name: fq_name, type_params, params, ret, body })
    }

    /// `parts[0]` of a `defun` form: a bare symbol names an ordinary
    /// function; `(name T1 T2...)` additionally declares generic type
    /// parameter names — mirroring `defmethod`'s `(self Type)` receiver-list
    /// syntax. Type parameter names are just ordinary (lowercase, after
    /// reader case-folding) identifiers; `Checker::check_call` resolves them
    /// against actual argument/expected types with the same `unify`/
    /// `subst_apply` machinery `check_construct` uses for an ADT's `params`.
    fn parse_defun_name(&self, heap: &Heap, v: Value) -> Result<(String, Vec<String>), Error> {
        match v {
            Value::Symbol(id) => Ok((heap.symbol_name(id).to_string(), Vec::new())),
            Value::Cons(_) => {
                let elems = heap.list_to_vec(v)?;
                let name = match elems.first() {
                    Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
                    _ => return Err(Error::TypeError("defun: name must be a symbol".into())),
                };
                let mut type_params = Vec::new();
                for tp in &elems[1..] {
                    match tp {
                        Value::Symbol(id) => type_params.push(heap.symbol_name(*id).to_string()),
                        _ => return Err(Error::TypeError("defun: type parameter must be a symbol".into())),
                    }
                }
                Ok((name, type_params))
            }
            _ => Err(Error::TypeError(
                "defun: name must be a symbol or (name type-params...)".into(),
            )),
        }
    }

    /// `(defmacro name (p1 p2 ...) body...)`: a compile-time code
    /// transformer. Every parameter and the implicit return are `Sexpr` — so,
    /// unlike `defun`, the parameter list is bare names with no type
    /// annotations (they'd always just say `Sexpr`). Registers the signature
    /// (arity only) before checking the body, so self-recursion works,
    /// matching [`Self::check_defun`]. The body is checked exactly like a
    /// `defun`'s; calling it later at macro-expansion time reuses the same
    /// machinery as calling an ordinary function (see [`MacroExpander`]) —
    /// `Interp::exec` stores a `Defmacro` in the very same function table.
    fn check_defmacro(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        public: bool,
    ) -> Result<TopLevel, Error> {
        if parts.len() < 2 {
            return Err(Error::TypeError("defmacro: (defmacro name (params) body...)".into()));
        }
        let name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("defmacro: name must be a symbol".into())),
        };
        let param_vals = heap.list_to_vec(parts[1])?;
        let mut names = Vec::new();
        for p in &param_vals {
            match p {
                Value::Symbol(id) => names.push(heap.symbol_name(*id).to_string()),
                _ => return Err(Error::TypeError("defmacro: parameter must be a name".into())),
            }
        }
        // `&rest name` collects every argument from that point on into a
        // single `Sexpr` list bound to `name`; it must be the last two
        // lambda-list items (CL-style, but `&rest` only — no `&optional`/`&key`).
        let (params, rest) = match names.iter().position(|n| n == "&rest") {
            Some(i) => {
                if i + 2 != names.len() {
                    return Err(Error::TypeError(
                        "defmacro: &rest must be followed by exactly one parameter name, as the last item in the parameter list".into(),
                    ));
                }
                names.remove(i); // drop the `&rest` marker, keeping the rest-param name in place
                (names, true)
            }
            None => (names, false),
        };
        let arity = if rest { params.len() - 1 } else { params.len() };
        let fq_name = self.fq(&name);

        self.check_redef("macro", &name, self.cur_ns().macros.get(&name))?;
        self.reg
            .root
            .module_mut(&self.ns)
            .macros
            .insert(name, MacroDef { arity, rest, public, builtin: false });

        let sexpr_ty = Type::Named(Path::root("sexpr"), vec![]);
        let env = Env::new().extended(params.iter().map(|p| (p.clone(), sexpr_ty.clone())).collect());
        let (body, _) = self.check_seq(heap, interp, &env, &parts[2..], Some(&sexpr_ty))?;
        Ok(TopLevel::Defmacro { name: fq_name, params, body, rest })
    }

    /// Parse a `((name type)...)` parameter list (types canonicalized to FQ).
    fn parse_params(&self, heap: &Heap, v: Value) -> Result<Vec<(String, Type)>, Error> {
        let elems = heap.list_to_vec(v)?;
        self.parse_param_pairs(heap, &elems)
    }

    /// Parse a `((name type)... [&rest (name elem-type)])` parameter list for
    /// `defun`/`lambda`: an optional trailing `&rest (name elem-type)` marks
    /// the function variadic — every call-site argument from that position on
    /// must have type `elem-type`, individually checked, then collected into
    /// a single `Sexpr` list bound to `name` inside the body (see
    /// `Self::check_defun`/`Self::check_lambda`) — like every Lisp's `&rest`
    /// parameter, an ordinary list, never a homogeneous array type; unlike
    /// `Self::check_defmacro`'s bare `&rest name` (always untyped `Sexpr`),
    /// `defun`/`lambda` additionally check each element against `elem-type`
    /// at every call site, since `defun`/`lambda` parameters always carry an
    /// explicit type.
    fn parse_params_rest(&self, heap: &Heap, v: Value) -> Result<ParamsAndRest, Error> {
        let elems = heap.list_to_vec(v)?;
        let rest_marker = elems
            .iter()
            .position(|p| matches!(p, Value::Symbol(id) if heap.symbol_name(*id) == "&rest"));
        match rest_marker {
            Some(i) => {
                if i + 2 != elems.len() {
                    return Err(Error::TypeError(
                        "&rest must be followed by exactly one parameter, as the last item in the parameter list".into(),
                    ));
                }
                let params = self.parse_param_pairs(heap, &elems[..i])?;
                let mut rest = self.parse_param_pairs(heap, &elems[i + 1..])?;
                // `&rest`'s declared element type isn't validated against
                // `sexpr_ctor_for` here: a generic `defun` (e.g. `(defun
                // (f T) ((a T) &rest (xs T)) ...)`) may declare it as a bare
                // type parameter, only resolved to something concrete once a
                // call site's actual arguments are checked — see
                // `Checker::wrap_rest_elem`, which validates the *resolved*
                // type instead, every time the list is actually collected.
                Ok((params, Some(rest.remove(0))))
            }
            None => Ok((self.parse_param_pairs(heap, &elems)?, None)),
        }
    }

    /// Parse a slice of `(name type)` binding forms.
    fn parse_param_pairs(&self, heap: &Heap, pairs: &[Value]) -> Result<Vec<(String, Type)>, Error> {
        let mut out = Vec::new();
        for binding in pairs {
            let pair = heap.list_to_vec(*binding)?;
            if pair.len() != 2 {
                return Err(Error::TypeError("parameter must be (name type)".into()));
            }
            let name = match pair[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("parameter name must be a symbol".into())),
            };
            out.push((name, self.canon(&parse_type(heap, pair[1])?)));
        }
        Ok(out)
    }

    // ---- module / defmethod / use -----------------------------------------

    fn check_module(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("module: (module path body...)".into()));
        }
        let segs = self.path_to_segs(heap, parts[0])?;
        let depth = segs.len();
        for s in &segs {
            self.ns.push(s.clone());
        }
        let path = Path::from_segments(self.ns.clone());
        // Ensure the (possibly empty) module namespace exists.
        self.reg.root.module_mut(&self.ns);

        let mut body = Vec::new();
        let mut result = Ok(());
        for form in &parts[1..] {
            match self.check_form(heap, interp, *form) {
                Ok(tl) => body.push(tl),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        for _ in 0..depth {
            self.ns.pop();
        }
        result?;
        Ok(TopLevel::Module { path, body })
    }

    fn check_defmethod(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        public: bool,
    ) -> Result<TopLevel, Error> {
        if parts.len() < 3 {
            return Err(Error::TypeError(
                "defmethod: (defmethod name (receiver params...) ret body...)".into(),
            ));
        }
        let method = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("defmethod: name must be a symbol".into())),
        };
        let sig_list = heap.list_to_vec(parts[1])?;
        if sig_list.is_empty() {
            return Err(Error::TypeError("defmethod: needs a receiver".into()));
        }
        // `(self T)` -> instance method; a bare type name -> static method.
        let (instance, self_name, type_expr) = match sig_list[0] {
            Value::Cons(_) => {
                let recv = heap.list_to_vec(sig_list[0])?;
                if recv.len() != 2 {
                    return Err(Error::TypeError("defmethod: receiver must be (self Type)".into()));
                }
                let sname = match recv[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("defmethod: receiver name must be a symbol".into())),
                };
                (true, Some(sname), recv[1])
            }
            Value::Symbol(_) | Value::Path(_) => (false, None, sig_list[0]),
            _ => return Err(Error::TypeError("defmethod: receiver must be (self Type) or a type name".into())),
        };
        let recv_ty = self.canon(&parse_type(heap, type_expr)?);
        let type_fq = match &recv_ty {
            Type::Named(n, _) => n.clone(),
            other => match prim_type_path(other) {
                Some(p) => p,
                None => return Err(Error::TypeError("defmethod: receiver must be a data type".into())),
            },
        };
        if self.reg.type_def(&type_fq).is_none() {
            return Err(Error::TypeError(format!("defmethod: unknown type `{}`", type_fq)));
        }
        let params = self.parse_param_pairs(heap, &sig_list[1..])?;
        let ret = self.canon(&parse_type(heap, parts[2])?);

        // Register the signature before checking the body (self-recursion).
        let mut sig_params: Vec<Type> = Vec::new();
        if instance {
            sig_params.push(recv_ty.clone());
        }
        sig_params.extend(params.iter().map(|(_, t)| t.clone()));
        let sig = FnSig { type_params: vec![], params: sig_params, ret: ret.clone(), public, rest: None, builtin: false };
        self.check_redef("method", &method, self.reg.type_def(&type_fq).and_then(|d| d.assoc.get(&method)))?;
        if let Some(def) = self.reg.type_def_mut(&type_fq) {
            def.assoc.insert(method.clone(), AssocFn { sig, instance, builtin: false });
        }

        let mut binds: Vec<(String, Type)> = Vec::new();
        if let Some(s) = &self_name {
            binds.push((s.clone(), recv_ty.clone()));
        }
        binds.extend(params.clone());
        let env = Env::new().extended(binds);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[3..], Some(&ret))?;
        Ok(TopLevel::Defmethod { type_name: type_fq, method, instance, self_name, params, ret, body })
    }

    /// `(defstruct Name (field Type)...)` — or, generically,
    /// `(defstruct (Name T1 T2...) (field Type)...)`, the name position
    /// parsed exactly like `(defun (name T1 T2...) ...)`'s
    /// (`Checker::parse_defun_name`, reused as-is) — a mutable product type:
    /// a single constructor, deliberately named `"new"` rather than `Name`
    /// itself, so `Type::new` resolves through the existing `Type::ctor`
    /// branch of `check_path_call` with no new constructor-resolution logic,
    /// and never occupies a bare/flat name (the "don't pollute the
    /// namespace" design goal this whole redesign was scoped around).
    /// `check_construct` builds instances (reading `AdtKind::Struct` to
    /// choose `RtValue::Struct` over `Data`) — generic substitution there
    /// (and in `match`'s `check_ctor_pattern`) is already `AdtDef.params`-
    /// generic, shared with `Option`/`Result`/`HashTable`, so a generic
    /// `defstruct` needs no changes to either.
    ///
    /// Beyond registering the type, this synthesizes one getter and one
    /// setter `defmethod` per field: `f` reads `fields[i]` (`Expr::FieldGet`)
    /// and `set-f` writes it (`Expr::FieldSet`, `set-car`/`set-cdr`'s prefix
    /// convention, no `!`), so `(f instance)`/`(set-f instance v)` — and,
    /// through `Checker::check`'s `Value::Path` sugar and `Checker::check_setf`,
    /// `instance::f`/`(setf instance::f v)` — work immediately; no separate
    /// accessor-declaration step exists. `check_form`'s signature is
    /// `Result<TopLevel, Error>` (one node per top-level form), so the type
    /// registration and every accessor `TopLevel::Defmethod` are bundled into
    /// one `TopLevel::Module` — purely as a grouping device: `Interp::exec`'s
    /// `Module` arm just runs `body` in order and never reads `path`, so this
    /// carries none of an actual `(module ...)`'s namespace-nesting semantics.
    fn check_defstruct(&mut self, heap: &Heap, parts: &[Value], public: bool) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("defstruct: (defstruct name (field type)...)".into()));
        }
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;
        let fields = self.parse_param_pairs(heap, &parts[1..])?;
        if fields.is_empty() {
            return Err(Error::TypeError("defstruct: needs at least one field".into()));
        }
        let field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
        let field_types: Vec<Type> = fields.iter().map(|(_, t)| t.clone()).collect();
        for (i, n) in field_names.iter().enumerate() {
            if field_names[..i].contains(n) {
                return Err(Error::TypeError(format!("defstruct: duplicate field `{}`", n)));
            }
        }

        self.check_redef("type", &name, self.cur_ns().types.get(&name))?;
        let type_fq = self.fq(&name);
        let recv_targs: Vec<Type> = type_params.iter().map(|p| Type::Named(Path::root(p), Vec::new())).collect();
        let recv_ty = Type::Named(type_fq.clone(), recv_targs);

        let mut assoc = HashMap::new();
        let mut accessors = Vec::with_capacity(fields.len() * 2);
        for (i, (field_name, field_ty)) in fields.iter().enumerate() {
            let getter_sig = FnSig {
                type_params: vec![],
                params: vec![recv_ty.clone()],
                ret: field_ty.clone(),
                public,
                rest: None,
                builtin: false,
            };
            assoc.insert(field_name.clone(), AssocFn { sig: getter_sig, instance: true, builtin: false });
            let self_var = Typed { expr: Expr::Var("self".to_string()), ty: recv_ty.clone() };
            accessors.push(TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: field_name.clone(),
                instance: true,
                self_name: Some("self".to_string()),
                params: Vec::new(),
                ret: field_ty.clone(),
                body: vec![Typed { expr: Expr::FieldGet(Box::new(self_var), i), ty: field_ty.clone() }],
            });

            // Setter (`set-car`/`set-cdr`'s `set-` prefix, no `!` — see
            // `Checker::check_setf`, which calls this via `(setf p::x v)`).
            let setter_name = format!("set-{}", field_name);
            let setter_sig = FnSig {
                type_params: vec![],
                params: vec![recv_ty.clone(), field_ty.clone()],
                ret: Type::Unit,
                public,
                rest: None,
                builtin: false,
            };
            assoc.insert(setter_name.clone(), AssocFn { sig: setter_sig, instance: true, builtin: false });
            let self_var = Typed { expr: Expr::Var("self".to_string()), ty: recv_ty.clone() };
            let value_var = Typed { expr: Expr::Var("value".to_string()), ty: field_ty.clone() };
            accessors.push(TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: setter_name,
                instance: true,
                self_name: Some("self".to_string()),
                params: vec![("value".to_string(), field_ty.clone())],
                ret: Type::Unit,
                body: vec![Typed {
                    expr: Expr::FieldSet(Box::new(self_var), i, Box::new(value_var)),
                    ty: Type::Unit,
                }],
            });
        }

        let def = AdtDef {
            name: type_fq.clone(),
            params: type_params,
            variants: vec![Variant { name: "new".to_string(), fields: field_types }],
            assoc,
            public,
            builtin: false,
            kind: AdtKind::Struct,
            field_names,
        };
        self.reg.root.module_mut(&self.ns).add_type(def);

        let mut body = vec![TopLevel::Defstruct { name: type_fq.clone() }];
        body.extend(accessors);
        Ok(TopLevel::Module { path: type_fq, body })
    }

    fn check_use(&mut self, heap: &Heap, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.len() != 1 {
            return Err(Error::TypeError("use: (use path)".into()));
        }
        let segs = self.path_to_segs(heap, parts[0])?;
        let bare = segs.last().cloned().unwrap();

        // Try: free function.
        if let Some(target) = self.resolve_fn_path(&segs) {
            self.reg.root.module_mut(&self.ns).aliases.insert(bare.clone(), segs);
            return Ok(TopLevel::Use { alias: self.fq(&bare), target });
        }
        // Try: type. Beyond the usual alias (so the bare name also resolves
        // in a type-annotation position, e.g. `(x Option)` — see
        // `resolve_type_name`), this snapshots the type's *current*
        // constructors and public static methods as bare names in the
        // current namespace — e.g. `(use option)` makes `some`/`none`
        // callable bare, exactly like `option::some`/`option::none` (see
        // `check_path_call`, which already treats a variant and a
        // non-instance `assoc` entry as the same kind of "static member").
        if let Some(target) = self.resolve_type_path(&segs) {
            self.reg.root.module_mut(&self.ns).aliases.insert(bare.clone(), segs);
            let def = self.reg.type_def(&target).expect("resolved type exists").clone();
            for (i, v) in def.variants.iter().enumerate() {
                let is_builtin = self
                    .cur_ns()
                    .ctors
                    .get(&v.name)
                    .and_then(|(owner, _)| self.reg.type_def(owner))
                    .map(|d| d.builtin)
                    .unwrap_or(false);
                if self.cur_ns().ctors.contains_key(&v.name) {
                    self.check_redef_outcome("constructor", &v.name, is_builtin)?;
                }
                self.reg.root.module_mut(&self.ns).ctors.insert(v.name.clone(), (target.clone(), i));
            }
            for (name, af) in def.assoc.iter() {
                if af.instance || !af.sig.public {
                    continue;
                }
                let is_builtin = self
                    .cur_ns()
                    .static_uses
                    .get(name)
                    .and_then(|(owner, m)| self.reg.type_def(owner).and_then(|d| d.assoc.get(m)))
                    .map(|af| af.builtin)
                    .unwrap_or(false);
                if self.cur_ns().static_uses.contains_key(name) {
                    self.check_redef_outcome("static method", name, is_builtin)?;
                }
                self.reg
                    .root
                    .module_mut(&self.ns)
                    .static_uses
                    .insert(name.clone(), (target.clone(), name.clone()));
            }
            return Ok(TopLevel::Use { alias: self.fq(&bare), target });
        }
        // Try: module alias — `(use std::math)` makes `math` a short name for `std::math`.
        if let Some((abs, _)) = self.find_module(&segs) {
            let target_path = Path::from_segments(abs.clone());
            self.reg.root.module_mut(&self.ns).mod_aliases.insert(bare.clone(), abs);
            return Ok(TopLevel::Use { alias: self.fq(&bare), target: target_path });
        }
        Err(Error::TypeError(format!("use: unresolved `{}`", segs.join("::"))))
    }

    // ---- expressions ------------------------------------------------------

    /// Check `v` as an expression, optionally against an `expected` type.
    fn check(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let typed = match v {
            Value::Int(n) => Typed { expr: Expr::Int(n), ty: int_lit_ty(expected) },
            Value::Float(f) => Typed { expr: Expr::Float(f), ty: float_lit_ty(expected) },
            Value::Bool(b) => Typed { expr: Expr::Bool(b), ty: Type::Bool },
            Value::Char(c) => Typed { expr: Expr::Char(c), ty: Type::Char },
            Value::Str(s) => Typed { expr: Expr::Str(heap.string(s).to_string()), ty: Type::Str },
            Value::Empty => {
                // The empty list `()` is the `None` value of `Option<T>` when an
                // option type is expected, the `Nil` value of `Sexpr` when a
                // `Sexpr` is expected, and otherwise the unit value.
                if let Some(Type::Named(n, _)) = expected {
                    if *n == Path::root("option") {
                        // `Option`'s constructors moved to `Option::`/`use
                        // option` (no more bare `none`), but `()` denoting
                        // `None` is a language-core inference rule, not a
                        // name-visibility one — it must keep working
                        // regardless of `use` status, so this bypasses
                        // `resolve_ctor` and goes straight to the known
                        // builtin path/variant index.
                        return self.check_construct(heap, interp, env, (&Path::root("option"), 1), &[], expected);
                    }
                    // `nil` stays a bare-resolvable name (`Sexpr` is exempt
                    // from the use-gated constructor visibility rule), so
                    // this one keeps going through `resolve_ctor` unchanged.
                    if let Some((adt, idx)) = self.resolve_ctor("nil") {
                        if *n == adt {
                            return self.check_construct(heap, interp, env, (&adt, idx), &[], expected);
                        }
                    }
                }
                Typed { expr: Expr::Unit, ty: Type::Unit }
            }
            Value::Symbol(id) => {
                let name = heap.symbol_name(id);
                if let Some(t) = env.get(name) {
                    Typed { expr: Expr::Var(name.to_string()), ty: t.clone() }
                } else if let Some((path, vi)) = self.resolve_global(name) {
                    Typed { expr: Expr::Global(path), ty: vi.ty }
                } else if let Some(t) = self.fn_value(name) {
                    t
                } else if let Some(t) = self.method_value(name, expected) {
                    t
                } else {
                    return Err(Error::TypeError(format!("unbound variable: {}", name)));
                }
            }
            Value::Cons(_) => self.check_list(heap, interp, env, v, expected)?,
            Value::Path(pid) => {
                // A bare `::` path as an expression: a qualified global or a
                // qualified function used as a value.
                let segs: Vec<String> = heap
                    .path_segments(pid)
                    .iter()
                    .map(|s| heap.symbol_name(*s).to_string())
                    .collect();
                // `var::field` — sugar for `(field var)`, a field accessor's
                // ordinary instance-method call (`Checker::check_defstruct`
                // synthesizes one getter `defmethod` per field). Tried first:
                // `segs[0]` here names a *value* (a bound local/global), not
                // a module/type segment the way `resolve_global_path`/
                // `fn_path_value` below expect.
                if let Some(result) = self.try_field_access(heap, interp, env, &segs) {
                    result?
                } else if let Some((path, vi)) = self.resolve_global_path(&segs) {
                    Typed { expr: Expr::Global(path), ty: vi.ty }
                } else if let Some(t) = self.fn_path_value(&segs) {
                    t
                } else {
                    return Err(Error::TypeError(format!("unresolved path: {}", segs.join("::"))));
                }
            }
        };
        // Reconcile synthesized types against the expectation. Literal and
        // constructor nodes already adopted `expected`, so this only fires on a
        // genuine mismatch. `Never` (a diverging expression) satisfies any
        // expected type.
        if let Some(e) = expected {
            if typed.ty != Type::Never && &typed.ty != e {
                return Err(Error::TypeError(format!(
                    "type mismatch: expected {:?}, found {:?}",
                    e, typed.ty
                )));
            }
        }
        Ok(typed)
    }

    /// Dispatch a compound form on its head symbol.
    fn check_list(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let elems = heap.list_to_vec(v)?;
        let args = &elems[1..];

        // A `::`-path head is a module-qualified function or a `Type::method`
        // static associated function.
        if let Value::Path(pid) = elems[0] {
            let segs: Vec<String> = heap
                .path_segments(pid)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect();
            return self.check_path_call(heap, interp, env, &segs, args, expected);
        }

        // A non-symbol head (e.g. a `lambda` literal or any expression) is
        // evaluated and applied as a function value.
        let head = match elems[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => {
                let callee = self.check(heap, interp, env, elems[0], None)?;
                return self.check_apply(heap, interp, env, callee, args);
            }
        };
        match head.as_str() {
            "if" => return self.check_if(heap, interp, env, args, expected),
            "let" => return self.check_let(heap, interp, env, args, expected),
            "let*" => return self.check_let_star(heap, interp, env, args, expected),
            "progn" => {
                let (body, ty) = self.check_seq(heap, interp, env, args, expected)?;
                // Represent progn as a let with no bindings.
                return Ok(Typed { expr: Expr::Let(Vec::new(), body), ty });
            }
            "setf" => return self.check_setf(heap, interp, env, args),
            "loop" => return self.check_loop(heap, interp, env, args),
            "break" => return self.check_break(args),
            "return" => return self.check_return(heap, interp, env, args),
            "list" => return self.check_list_lit(heap, interp, env, args),
            "lambda" => return self.check_lambda(heap, interp, env, args),
            "labels" => return self.check_labels(heap, interp, env, args, expected),
            "apply" => return self.check_apply_form(heap, interp, env, args),
            "match" => return self.check_match(heap, interp, env, args, expected),
            "panic" => return self.check_panic(heap, interp, env, args),
            "quote" => return self.check_quote(heap, args),
            "quasiquote" => return self.check_quasiquote(heap, interp, env, args),
            _ => {}
        }
        // A local variable holding a function value is applied directly (locals
        // shadow free functions).
        if let Some(t) = env.get(&head) {
            let callee = Typed { expr: Expr::Var(head.clone()), ty: t.clone() };
            return self.check_apply(heap, interp, env, callee, args);
        }
        // A macro call: expand (against the *unevaluated* argument forms,
        // exactly as written — see `MacroExpander`) and recursively check the
        // expansion in place, in the use site's lexical `env`. This is what
        // makes macros unhygienic/CL-style. Checked after special forms and
        // local-variable-as-callee (matching how a constructor/free-function
        // call is resolved below), since a macro is a purely compile-time
        // name with no runtime value to shadow or be shadowed by.
        if let Some((macro_path, arity, rest)) = self.resolve_macro(&head) {
            if rest {
                if args.len() < arity {
                    return Err(Error::TypeError(format!(
                        "macro `{}` expects at least {} argument(s), got {}",
                        head,
                        arity,
                        args.len()
                    )));
                }
            } else if args.len() != arity {
                return Err(Error::TypeError(format!(
                    "macro `{}` expects {} argument(s), got {}",
                    head,
                    arity,
                    args.len()
                )));
            }
            let expanded = interp
                .expand_macro(heap, &macro_path, args.to_vec())
                .map_err(|e| Error::TypeError(format!("macro `{}`: {}", head, e)))?;
            heap.push_root(expanded);
            let result = self.check(heap, interp, env, expanded, expected);
            heap.pop_root();
            return result;
        }
        // Constructor, then a receiver-typed instance method on the first
        // argument's type, then the free function, then a global function
        // value, then instance-method dispatch again (for the error message
        // when nothing at all matches). The instance method is tried *before*
        // the free function — CLOS precedent: a type-specific method takes
        // precedence over a same-named ordinary function, which only serves
        // as the generic function's implicit default when no type-specific
        // method matches the call's argument type (see `try_instance_method`'s
        // doc comment). This lets e.g. `HashTable<K,V>`'s `remove`/`count` and
        // the `Sexpr`-list prelude's free-function `remove`/`count` share a
        // name without either having to be renamed.
        if let Some((adt, idx)) = self.resolve_ctor(&head) {
            self.check_construct(heap, interp, env, (&adt, idx), args, expected)
        } else if let Some((type_fq, method)) = self.resolve_static_use(&head) {
            self.check_assoc_call(
                heap,
                interp,
                env,
                AssocCall { type_fq: &type_fq, method: &method, receiver: None, expected },
                args,
            )
        } else if let Some(result) = self.try_instance_method(heap, interp, env, &head, args) {
            result
        } else if let Some(fq) = self.resolve_fn(&head) {
            self.check_call(heap, interp, env, &fq, args)
        } else if let Some((path, vi)) = self.resolve_global(&head) {
            let callee = Typed { expr: Expr::Global(path), ty: vi.ty };
            self.check_apply(heap, interp, env, callee, args)
        } else {
            self.check_instance_method(heap, interp, env, &head, args)
        }
    }

    /// Type-check a `lambda`: `(lambda (params) ret body...)`. The body sees the
    /// enclosing locals (a closure) plus the parameters.
    fn check_lambda(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if args.len() < 2 {
            return Err(Error::TypeError("lambda: (lambda (params) ret body...)".into()));
        }
        let (params, rest) = self.parse_params_rest(heap, args[0])?;
        let ret = self.canon(&parse_type(heap, args[1])?);
        let fn_ty = Type::Fn(
            params.iter().map(|(_, t)| t.clone()).collect(),
            rest.as_ref().map(|(_, t)| Box::new(t.clone())),
            Box::new(ret.clone()),
        );
        // The body additionally sees the `&rest` parameter (if any) bound to
        // a plain `Sexpr` list — see `Self::check_defun`'s identical treatment.
        let mut params = params;
        if let Some((rname, _)) = rest {
            params.push((rname, sexpr_ty()));
        }
        let child = env.extended(params.clone());
        // A lambda is a new function boundary: `break`/`return` cannot reach an
        // outer loop through it, so it checks its body against an empty loop
        // stack (restored afterwards, even on error).
        let saved = self.loop_stack.replace(Vec::new());
        let result = self.check_seq(heap, interp, &child, &args[2..], Some(&ret));
        self.loop_stack.replace(saved);
        let (body, _) = result?;
        Ok(Typed { expr: Expr::Lambda { params, body }, ty: fn_ty })
    }

    /// `(labels ((name (params) ret body...)...) body...)`: like several
    /// `lambda`s, except each one — and the trailing `body` — can call any
    /// of them by name, including itself (CL's `labels`; addresses the
    /// catalog's "no self-referencing local function" gap, since a bare
    /// `lambda` only sees its *enclosing* scope, not a `let`-bound name for
    /// itself). Every function's signature is registered as a local
    /// variable of `Type::Fn` *before* any body is checked (mirroring
    /// `check_defun`'s self-recursion registration), so all of them —
    /// including the trailing `body` — see each other regardless of
    /// definition order.
    fn check_labels(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError(
                "labels: (labels ((name (params) ret body...)...) body...)".into(),
            ));
        }
        let specs = heap.list_to_vec(args[0])?;
        struct Spec {
            name: String,
            params: Vec<(String, Type)>,
            ret: Type,
            raw_body: Vec<Value>,
        }
        let mut parsed = Vec::new();
        let mut sigs: Vec<(String, Type)> = Vec::new();
        for spec in specs {
            let parts = heap.list_to_vec(spec)?;
            if parts.len() < 3 {
                return Err(Error::TypeError(
                    "labels: binding must be (name (params) ret body...)".into(),
                ));
            }
            let name = match parts[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("labels: name must be a symbol".into())),
            };
            let params = self.parse_params(heap, parts[1])?;
            let ret = self.canon(&parse_type(heap, parts[2])?);
            let fn_ty = Type::Fn(params.iter().map(|(_, t)| t.clone()).collect(), None, Box::new(ret.clone()));
            sigs.push((name.clone(), fn_ty));
            parsed.push(Spec { name, params, ret, raw_body: parts[3..].to_vec() });
        }
        // Every function's name is visible to every body (including its
        // own) and to the trailing `body` — registered up front, like
        // `check_defun`'s pre-body signature insert.
        let labels_env = env.extended(sigs);

        let mut defs = Vec::new();
        for Spec { name, params, ret, raw_body } in parsed {
            let fn_env = labels_env.extended(params.clone());
            // A new function boundary, same as `lambda`: `break`/`return`
            // can't reach an outer loop through it.
            let saved = self.loop_stack.replace(Vec::new());
            let result = self.check_seq(heap, interp, &fn_env, &raw_body, Some(&ret));
            self.loop_stack.replace(saved);
            let (body, _) = result?;
            defs.push((name, params, body));
        }
        let (body, ty) = self.check_seq(heap, interp, &labels_env, &args[1..], expected)?;
        Ok(Typed { expr: Expr::Labels { defs, body }, ty })
    }

    /// Type-check applying a function *value* `callee` to `args`. If
    /// `callee`'s type is variadic (`Type::Fn`'s `rest` is `Some`), every
    /// argument past the fixed parameters is checked against the rest
    /// element type and collected into a single `Sexpr` list actual
    /// argument — the value-level counterpart of `Self::check_call`'s
    /// desugaring for a named function.
    fn check_apply(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        callee: Typed,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let (params, rest, ret) = match &callee.ty {
            Type::Fn(p, r, ret) => (p.clone(), r.clone(), (**ret).clone()),
            other => return Err(Error::TypeError(format!("value is not callable: {:?}", other))),
        };
        let fixed = params.len();
        match &rest {
            None if args.len() != fixed => {
                return Err(Error::TypeError(format!(
                    "function expects {} argument(s), got {}",
                    fixed,
                    args.len()
                )));
            }
            Some(_) if args.len() < fixed => {
                return Err(Error::TypeError(format!(
                    "function expects at least {} argument(s), got {}",
                    fixed,
                    args.len()
                )));
            }
            _ => {}
        }
        let mut typed = Vec::new();
        for (arg, pty) in args[..fixed].iter().zip(params.iter()) {
            typed.push(self.check(heap, interp, env, *arg, Some(pty))?);
        }
        if let Some(elem_ty) = &rest {
            let mut rest_typed = Vec::new();
            for arg in &args[fixed..] {
                rest_typed.push(self.check(heap, interp, env, *arg, Some(elem_ty))?);
            }
            typed.push(self.cons_rest_list(elem_ty, rest_typed)?);
        }
        Ok(Typed { expr: Expr::Apply(Box::new(callee), typed), ty: ret })
    }

    /// `(apply f arg1 ... argN rest-list)`: call the *variadic* function
    /// value `f` — its type must be `(fn (T1..Tn) &rest Te) R)` — with
    /// `arg1..argN` bound to its fixed parameters and `rest-list` (already a
    /// plain `Sexpr` list, e.g. a `quote`d list or one built by `cons`)
    /// passed straight through as its `&rest` argument, instead of
    /// `Self::check_apply`'s usual one-by-one collection from individually-
    /// typed trailing expressions. Note `rest-list`'s elements are *not*
    /// checked against `Te` here — same as CL's `apply`, which never checks a
    /// list's contents either; the callee's own body is responsible for
    /// whatever it does with each element. Desugars to the very same
    /// `Expr::Apply` shape `check_apply` produces for `(f arg1 .. argN e1 e2
    /// e3)` if `rest-list` were three literal elements `e1 e2 e3` instead of
    /// one dynamic list — so the interpreter needs no `apply`-specific
    /// evaluation at all.
    fn check_apply_form(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if args.len() < 2 {
            return Err(Error::TypeError("apply: (apply function arg... rest-list)".into()));
        }
        let callee = self.check(heap, interp, env, args[0], None)?;
        let (params, ret) = match &callee.ty {
            Type::Fn(p, Some(_), ret) => (p.clone(), (**ret).clone()),
            Type::Fn(_, None, _) => {
                return Err(Error::TypeError(
                    "apply: function has no `&rest` parameter to apply a list to".into(),
                ))
            }
            other => return Err(Error::TypeError(format!("apply: value is not callable: {:?}", other))),
        };
        let (fixed_args, list_arg) = args[1..].split_at(args.len() - 2);
        if fixed_args.len() != params.len() {
            return Err(Error::TypeError(format!(
                "apply: expected {} fixed argument(s) before the rest list, got {}",
                params.len(),
                fixed_args.len()
            )));
        }
        let mut typed = Vec::new();
        for (arg, pty) in fixed_args.iter().zip(params.iter()) {
            typed.push(self.check(heap, interp, env, *arg, Some(pty))?);
        }
        typed.push(self.check(heap, interp, env, list_arg[0], Some(&sexpr_ty()))?);
        Ok(Typed { expr: Expr::Apply(Box::new(callee), typed), ty: ret })
    }

    /// A `::`-qualified call: a module-qualified free function, or a
    /// `Type::method` static associated function.
    fn check_path_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        segs: &[String],
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        // A module-qualified free function, e.g. `math::id`.
        if let Some(fq) = self.resolve_fn_path(segs) {
            return self.check_call(heap, interp, env, &fq, args);
        }
        // Otherwise `Type::member`: split the last segment as the member and
        // resolve the prefix as a type. The member is a constructor or a static
        // associated function of that type.
        if segs.len() >= 2 {
            let (type_segs, member) = segs.split_at(segs.len() - 1);
            let member = &member[0];
            if let Some(type_fq) = self.resolve_type_path(type_segs) {
                let def = self.reg.type_def(&type_fq).expect("resolved type exists");
                if let Some(variant) = def.variants.iter().position(|v| &v.name == member) {
                    return self.check_construct(heap, interp, env, (&type_fq, variant), args, expected);
                }
                if let Some(af) = def.assoc.get(member) {
                    if af.instance {
                        return Err(Error::TypeError(format!(
                            "`{}::{}` is an instance method; call it as ({} obj ...)",
                            type_fq, member, member
                        )));
                    }
                    return self.check_assoc_call(
                        heap,
                        interp,
                        env,
                        AssocCall { type_fq: &type_fq, method: member.as_str(), receiver: None, expected },
                        args,
                    );
                }
            }
        }
        Err(Error::TypeError(format!("unresolved path: {}", segs.join("::"))))
    }

    /// Dispatch a bare call `(m recv args...)` as an instance method on the
    /// static type of its first argument.
    /// Try resolving `(method args...)` as a call to an instance method on
    /// the first argument's type. Returns `None` — not an error — when no
    /// such method exists for that type, so `check_list` can fall back to a
    /// free function of the same name: this mirrors CLOS, where a
    /// type-specific method takes precedence over an ordinary function of
    /// the same name, and that ordinary function only acts as the generic
    /// function's default when no type-specific method matches the call's
    /// argument type (an existing `defun` becomes the default method when a
    /// same-named `defgeneric`/`defmethod` is introduced later).
    ///
    /// The first argument is checked once here with `expected: None`, purely
    /// to learn its type — if that doesn't resolve to a method, `check_list`'s
    /// free-function fallback re-checks it against the free function's real
    /// parameter type, so e.g. an integer literal still gets that parameter's
    /// exact width rather than the throwaway default this `check` would have
    /// given it. Any error checking the first argument (e.g. an unbound
    /// variable) also yields `None`: it isn't a real "no such method"
    /// failure, but the free-function fallback (or, if none exists either,
    /// the final `check_instance_method` call in `check_list`) re-checks the
    /// same argument and surfaces the real error there instead.
    fn try_instance_method(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        method: &str,
        args: &[Value],
    ) -> Option<Result<Typed, Error>> {
        let recv = self.check(heap, interp, env, *args.first()?, None).ok()?;
        let type_fq = match &recv.ty {
            Type::Named(n, _) => Some(n.clone()),
            other => prim_type_path(other),
        }?;
        let def = self.reg.type_def(&type_fq)?;
        if def.assoc.get(method).map(|a| a.instance) != Some(true) {
            return None;
        }
        Some(self.check_assoc_call(
            heap,
            interp,
            env,
            AssocCall { type_fq: &type_fq, method, receiver: Some(recv), expected: None },
            &args[1..],
        ))
    }

    /// The final fallback when nothing else matched `(method args...)` in
    /// `check_list`: re-derives the same instance-method lookup
    /// `try_instance_method` does, but — since there's no free function left
    /// to defer to — propagates a real error from checking the first
    /// argument instead of swallowing it, and reports `NoSuchFunction` if no
    /// matching method exists either.
    fn check_instance_method(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        method: &str,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if !args.is_empty() {
            let recv = self.check(heap, interp, env, args[0], None)?;
            let type_fq = match &recv.ty {
                Type::Named(n, _) => Some(n.clone()),
                other => prim_type_path(other),
            };
            if let Some(type_fq) = type_fq {
                if let Some(def) = self.reg.type_def(&type_fq) {
                    if def.assoc.get(method).map(|a| a.instance) == Some(true) {
                        return self.check_assoc_call(
                            heap,
                            interp,
                            env,
                            AssocCall { type_fq: &type_fq, method, receiver: Some(recv), expected: None },
                            &args[1..],
                        );
                    }
                }
            }
        }
        Err(Error::NoSuchFunction(method.to_string()))
    }

    /// Check a call to a type-associated function. For an instance method the
    /// already-checked `receiver` fills the first parameter slot. `call` bundles
    /// `(type path, method name)`, the receiver, and the call site's expected
    /// type into one parameter to keep the arity down.
    ///
    /// Generic owners (e.g. the built-in `HashTable<K,V>`): the method's
    /// signature template mentions `def.params` (`k`/`v`) as type variables,
    /// the same way a constructor's field templates do (see
    /// [`Self::check_construct`]). The concrete bindings come from wherever
    /// they're available — an instance call already knows them from the
    /// receiver's resolved type, `Type::Named(type_fq, concrete_args)`; a
    /// static call with no receiver (e.g. `HashTable::new`, which has no
    /// argument to infer `K`/`V` from either) instead seeds them from
    /// `expected`, exactly like a field-less constructor such as `None`
    /// learns its type argument from `expected`. If a parameter is bound by
    /// neither, it's left as a free type variable and surfaces as a normal
    /// "cannot infer" error, matching `check_construct`.
    fn check_assoc_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        call: AssocCall,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let AssocCall { type_fq, method, receiver, expected } = call;
        let instance = receiver.is_some();
        let def = self.reg.type_def(type_fq).expect("assoc type exists").clone();
        let af = def.assoc[method].clone();

        let mut subst: HashMap<String, Type> = HashMap::new();
        let concrete_args: Option<&[Type]> = match (&receiver, expected) {
            (Some(r), _) => match &r.ty {
                Type::Named(n, args) if n == type_fq => Some(args.as_slice()),
                _ => None,
            },
            (None, Some(Type::Named(n, args))) if n == type_fq => Some(args.as_slice()),
            (None, _) => None,
        };
        if let Some(args) = concrete_args {
            if args.len() == def.params.len() {
                for (p, a) in def.params.iter().zip(args.iter()) {
                    subst.insert(p.clone(), a.clone());
                }
            }
        }

        let offset = if instance { 1 } else { 0 };
        let expected_params: Vec<Type> =
            af.sig.params[offset..].iter().map(|t| subst_apply(t, &subst)).collect();
        if args.len() != expected_params.len() {
            return Err(Error::TypeError(format!(
                "{}::{}: expected {} argument(s), got {}",
                type_fq,
                method,
                expected_params.len(),
                args.len()
            )));
        }
        let mut typed = Vec::new();
        if let Some(r) = receiver {
            typed.push(r);
        }
        for (arg, pty) in args.iter().zip(expected_params.iter()) {
            typed.push(self.check(heap, interp, env, *arg, Some(pty))?);
        }
        for p in &def.params {
            if !subst.contains_key(p) {
                return Err(Error::TypeError(format!(
                    "cannot infer type argument `{}` for `{}::{}`",
                    p, type_fq, method
                )));
            }
        }
        Ok(Typed {
            expr: Expr::Assoc {
                type_name: type_fq.clone(),
                method: method.to_string(),
                instance,
                args: typed,
            },
            ty: subst_apply(&af.sig.ret, &subst),
        })
    }

    fn check_if(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.len() != 3 {
            return Err(Error::TypeError("if: (if cond then else)".into()));
        }
        let cond = self.check(heap, interp, env, args[0], Some(&Type::Bool))?;
        let then = self.check(heap, interp, env, args[1], expected)?;
        // A diverging (`Never`) then branch must not constrain the else branch.
        let else_expected = non_never(&then.ty).or(expected);
        let els = self.check(heap, interp, env, args[2], else_expected)?;
        let ty = join_types(&then.ty, &els.ty)?;
        Ok(Typed { expr: Expr::If(Box::new(cond), Box::new(then), Box::new(els)), ty })
    }

    fn check_panic(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("panic: (panic message)".into()));
        }
        let msg = self.check(heap, interp, env, args[0], Some(&Type::Str))?;
        Ok(Typed { expr: Expr::Panic(Box::new(msg)), ty: Type::Never })
    }

    /// `(quote datum)`: `datum` as a literal `Sexpr` value, unevaluated. See
    /// [`Expr::Quote`] for why this converts to an owned [`QuotedSexpr`]
    /// rather than keeping the raw read `Value`.
    fn check_quote(&self, heap: &Heap, args: &[Value]) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("quote: (quote datum)".into()));
        }
        let qs = value_to_quoted(heap, args[0])?;
        Ok(Typed { expr: Expr::Quote(qs), ty: Type::Named(Path::root("sexpr"), vec![]) })
    }

    /// `(quasiquote template)`: like `quote`, but `(unquote x)` sub-forms are
    /// evaluated (checked as an ordinary expression in the surrounding
    /// lexical scope, so they may reference locals) and spliced in directly.
    /// `x` must itself be `Sexpr`-typed — no implicit coercion from `i32`/
    /// `bool`/etc, matching the rest of the language's strict-type-matching
    /// rules. This is the natural fit for macro-writing (a `defmacro`
    /// parameter, or any other already-`Sexpr` sub-expression, splices in
    /// as-is); embedding a non-`Sexpr` runtime value as quoted data needs an
    /// explicit conversion (not provided yet — there is no `int->sexpr` etc.).
    /// A pure syntax-to-`Expr` desugaring — same idea as `list` building
    /// nested `Expr::Construct{Cons,..}` (`check_list_lit`) — so it needs no
    /// new runtime machinery; `(unquote-splicing x)` (`,@x`) is the one
    /// exception, desugaring to a call to the prelude's `append` (see
    /// `check_qq_template`'s doc comment) since the spliced list's length
    /// isn't known until runtime. A quasiquote nested inside another is
    /// treated as ordinary literal data (no depth tracking).
    fn check_quasiquote(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("quasiquote: (quasiquote template)".into()));
        }
        self.check_qq_template(heap, interp, env, args[0])
    }

    /// Recursively desugar one quasiquote template node. See [`Self::check_quasiquote`].
    ///
    /// `(unquote-splicing x)` (`,@x`) as a list *element* (i.e. `car` of the
    /// cons being processed) splices `x`'s elements into the result in place,
    /// rather than nesting `x` itself as one element — CL/Scheme's usual
    /// `,@` semantics, used so a macro's `&rest body` (already one `Sexpr`
    /// list) can be spliced into a template as multiple sibling forms
    /// (e.g. `` `(progn ,@body) `` for a `body` of three forms expands to a
    /// 3-element `progn`, not a `progn` wrapping one 3-element list). Since
    /// `x`'s length isn't known until runtime, this can't be expressed as a
    /// static `cons` nest like the non-splicing case — it desugars to a call
    /// to the prelude's `append` (`Checker::resolve_fn`, the same lookup
    /// `check_call`'s caller uses), joining `x` with the recursively
    /// desugared rest of the list. This means `,@` requires the prelude to
    /// be loaded (`append` registered) — acceptable since the macros that
    /// actually need `,@` (`until`/`while-let`/`case`/`do`, roadmap step 8)
    /// live in the prelude themselves.
    fn check_qq_template(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
    ) -> Result<Typed, Error> {
        let sexpr_ty = Type::Named(Path::root("sexpr"), vec![]);
        if let Value::Cons(_) = v {
            let car = heap.car(v)?;
            let cdr = heap.cdr(v)?;
            if is_symbol(heap, car, "unquote") {
                if let Value::Cons(_) = cdr {
                    let x = heap.car(cdr)?;
                    if heap.cdr(cdr)?.is_empty() {
                        return self.check(heap, interp, env, x, Some(&sexpr_ty));
                    }
                }
                return Err(Error::TypeError("unquote: (unquote datum)".into()));
            }
            if let Value::Cons(_) = car {
                let car_car = heap.car(car)?;
                if is_symbol(heap, car_car, "unquote-splicing") {
                    let car_cdr = heap.cdr(car)?;
                    if let Value::Cons(_) = car_cdr {
                        let x = heap.car(car_cdr)?;
                        if heap.cdr(car_cdr)?.is_empty() {
                            let spliced = self.check(heap, interp, env, x, Some(&sexpr_ty))?;
                            let rest = self.check_qq_template(heap, interp, env, cdr)?;
                            let append_fq = self.resolve_fn("append").ok_or_else(|| {
                                Error::TypeError(
                                    "unquote-splicing (,@) requires the prelude's `append` to be loaded"
                                        .into(),
                                )
                            })?;
                            return Ok(Typed {
                                expr: Expr::Call(append_fq, vec![spliced, rest]),
                                ty: sexpr_ty,
                            });
                        }
                    }
                    return Err(Error::TypeError("unquote-splicing: (unquote-splicing datum)".into()));
                }
            }
            let car_t = self.check_qq_template(heap, interp, env, car)?;
            let cdr_t = self.check_qq_template(heap, interp, env, cdr)?;
            let (adt, cons_idx) = self.resolve_ctor("cons").expect("sexpr::cons is built in");
            return Ok(Typed {
                expr: Expr::Construct { type_name: adt, variant: cons_idx, args: vec![car_t, cdr_t], mutable: false },
                ty: sexpr_ty,
            });
        }
        let qs = value_to_quoted(heap, v)?;
        Ok(Typed { expr: Expr::Quote(qs), ty: sexpr_ty })
    }

    fn check_let(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("let: (let ((name val)...) body...)".into()));
        }
        let mut binds = Vec::new();
        for binding in heap.list_to_vec(args[0])? {
            let pair = heap.list_to_vec(binding)?;
            if pair.len() != 2 {
                return Err(Error::TypeError("let: binding must be (name val)".into()));
            }
            let name = match pair[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("let: binding name must be a symbol".into())),
            };
            // Binding values are checked in the *outer* environment (CL `let`).
            let val = self.check(heap, interp, env, pair[1], None)?;
            binds.push((name, val));
        }
        let child = env.extended(binds.iter().map(|(n, t)| (n.clone(), t.ty.clone())).collect());
        let (body, ty) = self.check_seq(heap, interp, &child, &args[1..], expected)?;
        Ok(Typed { expr: Expr::Let(binds, body), ty })
    }

    /// `let*`: like `let` but each binding sees the earlier ones. Desugars to
    /// nested single-binding `let`s.
    fn check_let_star(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("let*: (let* ((name val)...) body...)".into()));
        }
        let binds = heap.list_to_vec(args[0])?;
        self.let_star_rec(heap, interp, env, &binds, &args[1..], expected)
    }

    fn let_star_rec(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        binds: &[Value],
        body: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if binds.is_empty() {
            let (body, ty) = self.check_seq(heap, interp, env, body, expected)?;
            return Ok(Typed { expr: Expr::Let(Vec::new(), body), ty });
        }
        let pair = heap.list_to_vec(binds[0])?;
        if pair.len() != 2 {
            return Err(Error::TypeError("let*: binding must be (name val)".into()));
        }
        let name = match pair[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("let*: binding name must be a symbol".into())),
        };
        let val = self.check(heap, interp, env, pair[1], None)?;
        let child = env.extended(vec![(name.clone(), val.ty.clone())]);
        let inner = self.let_star_rec(heap, interp, &child, &binds[1..], body, expected)?;
        let ty = inner.ty.clone();
        Ok(Typed { expr: Expr::Let(vec![(name, val)], vec![inner]), ty })
    }

    /// `(setf var value)`: assign to a bound variable. The value must match the
    /// variable's type; the expression evaluates to that value.
    fn check_setf(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("setf: (setf var value)".into()));
        }
        if let Value::Path(pid) = args[0] {
            let segs: Vec<String> = heap
                .path_segments(pid)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect();
            return self.check_field_set(heap, interp, env, &segs, args[1]);
        }
        let name = match args[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("setf: target must be a variable".into())),
        };
        if let Some(ty) = env.get(&name).cloned() {
            let value = self.check(heap, interp, env, args[1], Some(&ty))?;
            return Ok(Typed { expr: Expr::Set(name, Box::new(value)), ty });
        }
        if let Some((path, vi)) = self.resolve_global(&name) {
            if !vi.mutable {
                return Err(Error::TypeError(format!("setf: cannot assign to constant `{}`", name)));
            }
            let value = self.check(heap, interp, env, args[1], Some(&vi.ty))?;
            return Ok(Typed { expr: Expr::SetGlobal(path, Box::new(value)), ty: vi.ty });
        }
        Err(Error::TypeError(format!("setf: unbound variable: {}", name)))
    }

    /// `(setf var::field value)`: the write counterpart of
    /// `Checker::try_field_access` — resolves `var`'s type's `set-field`
    /// setter (synthesized by `Checker::check_defstruct` alongside the
    /// getter) and delegates to `Checker::check_assoc_call` exactly like
    /// `try_field_access` does, so a generic `defstruct`'s field type gets
    /// the receiver's concrete type arguments substituted (see that
    /// function's doc comment for why building the `Expr::Assoc` node by
    /// hand here once got this wrong).
    fn check_field_set(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        segs: &[String],
        value: Value,
    ) -> Result<Typed, Error> {
        let [recv_name, field] = segs else {
            return Err(Error::TypeError(format!("setf: unresolved path: {}", segs.join("::"))));
        };
        let recv = if let Some(t) = env.get(recv_name) {
            Typed { expr: Expr::Var(recv_name.clone()), ty: t.clone() }
        } else if let Some((path, vi)) = self.resolve_global(recv_name) {
            Typed { expr: Expr::Global(path), ty: vi.ty }
        } else {
            return Err(Error::TypeError(format!("setf: unbound variable: {}", recv_name)));
        };
        let Type::Named(type_fq, _) = &recv.ty else {
            return Err(Error::TypeError(format!("setf: `{}` has no field `{}`", recv_name, field)));
        };
        let setter = format!("set-{}", field);
        let is_field_setter = self
            .reg
            .type_def(type_fq)
            .and_then(|d| d.assoc.get(&setter))
            .is_some_and(|af| af.instance);
        if !is_field_setter {
            return Err(Error::TypeError(format!("setf: `{}` has no field `{}`", type_fq, field)));
        }
        let type_fq = type_fq.clone();
        self.check_assoc_call(
            heap,
            interp,
            env,
            AssocCall { type_fq: &type_fq, method: &setter, receiver: Some(recv), expected: None },
            std::slice::from_ref(&value),
        )
    }

    /// `(defvar name value)` / `(defconstant name value)`, with an optional
    /// `(name Type)` annotation. Registers a global in the current namespace.
    fn check_defvar(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        mutable: bool,
        public: bool,
    ) -> Result<TopLevel, Error> {
        if parts.len() != 2 {
            return Err(Error::TypeError("defvar/defconstant: (defvar name value)".into()));
        }
        let (name, ann) = match parts[0] {
            Value::Symbol(id) => (heap.symbol_name(id).to_string(), None),
            Value::Cons(_) => {
                let pair = heap.list_to_vec(parts[0])?;
                if pair.len() != 2 {
                    return Err(Error::TypeError("defvar: typed name must be (name type)".into()));
                }
                let name = match pair[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("defvar: name must be a symbol".into())),
                };
                (name, Some(self.canon(&parse_type(heap, pair[1])?)))
            }
            _ => return Err(Error::TypeError("defvar: name must be a symbol or (name type)".into())),
        };
        // The value is checked at the top level (no locals), but globals/fns are
        // visible via the registry.
        let value = self.check(heap, interp, &Env::new(), parts[1], ann.as_ref())?;
        let ty = ann.unwrap_or_else(|| value.ty.clone());
        self.check_redef("variable", &name, self.cur_ns().vars.get(&name))?;
        self.reg
            .root
            .module_mut(&self.ns)
            .vars
            .insert(name.clone(), VarInfo { ty: ty.clone(), mutable, public, builtin: false });
        Ok(TopLevel::Defvar { name: self.fq(&name), ty, value, mutable })
    }

    /// `(loop body...)`: an infinite loop, exited via `break`/`return`. Its
    /// type is the join of every `break`/`return` reached directly inside it
    /// (not crossing a nested loop/lambda); `Never` if it never exits.
    fn check_loop(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let (body, ty) = self.check_loop_body(heap, interp, env, args, Type::Never)?;
        Ok(Typed { expr: Expr::Loop(body), ty })
    }

    /// Check a loop body sequence with a fresh loop-stack frame seeded at
    /// `seed` (the loop's known result type if fixed, e.g. `Unit` for
    /// `while`/`dotimes`/`dolist`; `Never` for `loop`, refined by any
    /// `break`/`return` found). Returns the checked body and the frame's final
    /// type. The frame is popped even if checking the body fails.
    fn check_loop_body(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        body: &[Value],
        seed: Type,
    ) -> Result<(Vec<Typed>, Type), Error> {
        self.loop_stack.borrow_mut().push(seed);
        let result = self.check_seq(heap, interp, env, body, None);
        let ty = self.loop_stack.borrow_mut().pop().expect("pushed above");
        let (body, _) = result?;
        Ok((body, ty))
    }

    /// `(break)`: exit the nearest enclosing loop with no value (`Unit`). Type
    /// `Never` (diverges; satisfies any expectation).
    fn check_break(&self, args: &[Value]) -> Result<Typed, Error> {
        if !args.is_empty() {
            return Err(Error::TypeError("break: (break), takes no arguments".into()));
        }
        self.contribute_loop_exit(Type::Unit)?;
        Ok(Typed { expr: Expr::Break, ty: Type::Never })
    }

    /// `(return)` / `(return value)`: exit the nearest enclosing loop,
    /// optionally with a value (`Unit` if omitted). Type `Never`.
    fn check_return(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if args.len() > 1 {
            return Err(Error::TypeError("return: (return) or (return value)".into()));
        }
        let top = self
            .loop_stack
            .borrow()
            .last()
            .cloned()
            .ok_or_else(|| Error::TypeError("return: not inside a loop".into()))?;
        let expected = non_never(&top).cloned();
        let value = match args.first() {
            Some(v) => Some(self.check(heap, interp, env, *v, expected.as_ref())?),
            None => None,
        };
        let ty = value.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Unit);
        self.contribute_loop_exit(ty)?;
        Ok(Typed { expr: Expr::Return(value.map(Box::new)), ty: Type::Never })
    }

    /// Unify a `break`/`return` value's type into the nearest enclosing loop's
    /// accumulated exit type, erroring if there is no enclosing loop or the
    /// types disagree (mirrors how `match`/`cond` arms must agree).
    fn contribute_loop_exit(&self, ty: Type) -> Result<(), Error> {
        let mut stack = self.loop_stack.borrow_mut();
        let top = stack
            .last_mut()
            .ok_or_else(|| Error::TypeError("break/return: not inside a loop".into()))?;
        *top = join_types(&*top, &ty)?;
        Ok(())
    }

    /// `(list e1 e2 ... en)`: build a `Sexpr` cons-list from `Sexpr`-typed
    /// elements (each checked against `Sexpr`, so `()` adopts `Nil`).
    /// Desugars to nested `(Cons e1 (Cons e2 (... (Nil))))`; `(list)` is `(Nil)`.
    fn check_list_lit(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let sexpr_ty = Type::Named(Path::root("sexpr"), vec![]);
        let (adt, cons_idx) = self.resolve_ctor("cons").expect("sexpr::cons is built in");
        let (_, nil_idx) = self.resolve_ctor("nil").expect("sexpr::nil is built in");
        let mut acc = Typed {
            expr: Expr::Construct { type_name: adt.clone(), variant: nil_idx, args: Vec::new(), mutable: false },
            ty: sexpr_ty.clone(),
        };
        for &elem in args.iter().rev() {
            let e = self.check(heap, interp, env, elem, Some(&sexpr_ty))?;
            acc = Typed {
                expr: Expr::Construct { type_name: adt.clone(), variant: cons_idx, args: vec![e, acc], mutable: false },
                ty: sexpr_ty.clone(),
            };
        }
        Ok(acc)
    }

    fn check_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        name: &Path,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let sig = self.reg.fn_sig(name).expect("caller checked presence").clone();
        let fixed = sig.params.len();
        match &sig.rest {
            None if args.len() != fixed => {
                return Err(Error::TypeError(format!(
                    "{}: expected {} argument(s), got {}",
                    name,
                    fixed,
                    args.len()
                )));
            }
            Some(_) if args.len() < fixed => {
                return Err(Error::TypeError(format!(
                    "{}: expected at least {} argument(s), got {}",
                    name,
                    fixed,
                    args.len()
                )));
            }
            _ => {}
        }
        // Non-generic functions (the overwhelming majority) take the fast
        // path: `params`/`subst` stay empty, so `subst_apply`/`type_has_param`
        // below are no-ops and behavior is identical to before generics were
        // added. Generic functions resolve `sig.type_params` against the
        // actual argument types with the same `unify`/`subst_apply` accumulation
        // `check_construct` uses for an ADT's `params`.
        let params: HashSet<String> = sig.type_params.iter().cloned().collect();
        let mut subst: HashMap<String, Type> = HashMap::new();
        let mut typed = Vec::new();
        for (arg, pty) in args[..fixed].iter().zip(sig.params.iter()) {
            let st = subst_apply(pty, &subst);
            let exp = if type_has_param(&st, &params) { None } else { Some(st) };
            let ta = self.check(heap, interp, env, *arg, exp.as_ref())?;
            unify(&params, pty, &ta.ty, &mut subst)?;
            typed.push(ta);
        }
        // A variadic function: every remaining argument is individually
        // checked against the (possibly still-generic) rest element type,
        // then collected into a single `Sexpr` list actual argument — see
        // `sexpr_ty`'s doc comment and `Self::check_apply`'s value-level
        // counterpart.
        if let Some(elem_ty) = &sig.rest {
            let mut rest_typed = Vec::new();
            for arg in &args[fixed..] {
                let st = subst_apply(elem_ty, &subst);
                let exp = if type_has_param(&st, &params) { None } else { Some(st) };
                let ta = self.check(heap, interp, env, *arg, exp.as_ref())?;
                unify(&params, elem_ty, &ta.ty, &mut subst)?;
                rest_typed.push(ta);
            }
            let resolved = subst_apply(elem_ty, &subst);
            typed.push(self.cons_rest_list(&resolved, rest_typed)?);
        }
        for p in &sig.type_params {
            if !subst.contains_key(p) {
                return Err(Error::TypeError(format!(
                    "cannot infer type parameter `{}` for `{}`",
                    p, name
                )));
            }
        }
        Ok(Typed { expr: Expr::Call(name.clone(), typed), ty: subst_apply(&sig.ret, &subst) })
    }

    /// `ctor` is `(type path, variant index)` — the same pair [`Self::resolve_ctor`]
    /// returns, bundled into one parameter to keep the arity down.
    fn check_construct(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        ctor: (&Path, usize),
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let (adt_name, variant) = ctor;
        let def = self.reg.type_def(adt_name).expect("indexed adt exists").clone();
        let fields = &def.variants[variant].fields;
        if args.len() != fields.len() {
            return Err(Error::TypeError(format!(
                "{}: expected {} field(s), got {}",
                def.variants[variant].name,
                fields.len(),
                args.len()
            )));
        }
        let params: HashSet<String> = def.params.iter().cloned().collect();
        let mut subst: HashMap<String, Type> = HashMap::new();

        // Seed the substitution from an expected `Named(adt, args)` type, so a
        // field-less constructor such as `None` can learn its type argument.
        if let Some(Type::Named(n, eargs)) = expected {
            if n == adt_name && eargs.len() == def.params.len() {
                for (p, a) in def.params.iter().zip(eargs.iter()) {
                    subst.insert(p.clone(), a.clone());
                }
            }
        }

        let mut typed_args = Vec::new();
        for (arg, field) in args.iter().zip(fields.iter()) {
            let st = subst_apply(field, &subst);
            let exp = if type_has_param(&st, &params) { None } else { Some(st) };
            let ta = self.check(heap, interp, env, *arg, exp.as_ref())?;
            unify(&params, field, &ta.ty, &mut subst)?;
            typed_args.push(ta);
        }

        let mut result_args = Vec::new();
        for p in &def.params {
            match subst.get(p) {
                Some(t) => result_args.push(t.clone()),
                None => {
                    return Err(Error::TypeError(format!(
                        "cannot infer type argument `{}` for `{}`",
                        p, adt_name
                    )))
                }
            }
        }
        Ok(Typed {
            expr: Expr::Construct {
                type_name: adt_name.clone(),
                variant,
                args: typed_args,
                mutable: def.kind == AdtKind::Struct,
            },
            ty: Type::Named(adt_name.clone(), result_args),
        })
    }

    // ---- match / if-let ---------------------------------------------------

    fn check_match(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("match: (match expr arms...)".into()));
        }
        let scrut = self.check(heap, interp, env, args[0], None)?;
        let (adt_name, _) = self.expect_adt(&scrut.ty)?;
        let total_variants = self.reg.type_def(&adt_name).expect("adt exists").variants.len();

        let mut arms = Vec::new();
        let mut covered: HashSet<usize> = HashSet::new();
        let mut catchall = false;
        let mut result_ty: Option<Type> = expected.cloned();

        for arm_val in &args[1..] {
            let parts = heap.list_to_vec(*arm_val)?;
            if parts.is_empty() {
                return Err(Error::TypeError("match: arm must be (pattern body...)".into()));
            }
            let (pat, binds) = self.check_pattern(heap, &scrut.ty, parts[0])?;
            match &pat {
                Pattern::Ctor { variant, .. } => {
                    covered.insert(*variant);
                }
                Pattern::Wildcard | Pattern::Bind(_) => catchall = true,
                _ => {}
            }
            let arm_env = env.extended(binds);
            // Diverging arms don't constrain the result type; concrete arms must
            // all agree (Never joins with anything).
            let arm_expected = result_ty.as_ref().and_then(non_never);
            let (body, body_ty) = self.check_seq(heap, interp, &arm_env, &parts[1..], arm_expected)?;
            result_ty = Some(match result_ty {
                None => body_ty,
                Some(r) => join_types(&r, &body_ty)?,
            });
            arms.push(Arm { pat, body });
        }

        if !catchall && covered.len() != total_variants {
            return Err(Error::TypeError(format!(
                "non-exhaustive match on `{}`: {}/{} variants covered",
                adt_name,
                covered.len(),
                total_variants
            )));
        }
        let ty = result_ty.ok_or_else(|| Error::TypeError("match: no arms".into()))?;
        Ok(Typed { expr: Expr::Match(Box::new(scrut), arms), ty })
    }

    /// Check a pattern against the type of the value it matches, returning the
    /// pattern and the variable bindings it introduces.
    fn check_pattern(
        &self,
        heap: &Heap,
        expected: &Type,
        v: Value,
    ) -> Result<(Pattern, Vec<(String, Type)>), Error> {
        match v {
            Value::Symbol(id) => {
                let name = heap.symbol_name(id);
                if name == "_" {
                    Ok((Pattern::Wildcard, Vec::new()))
                } else {
                    Ok((Pattern::Bind(name.to_string()), vec![(name.to_string(), expected.clone())]))
                }
            }
            Value::Int(n) => {
                if is_integer_type(expected) {
                    Ok((Pattern::Int(n), Vec::new()))
                } else {
                    Err(Error::TypeError(format!(
                        "integer pattern does not match type {:?}",
                        expected
                    )))
                }
            }
            Value::Bool(b) if *expected == Type::Bool => Ok((Pattern::Bool(b), Vec::new())),
            Value::Char(c) if *expected == Type::Char => Ok((Pattern::Char(c), Vec::new())),
            Value::Cons(_) => self.check_ctor_pattern(heap, expected, v),
            other => Err(Error::TypeError(format!("pattern does not match {:?}: {:?}", expected, other))),
        }
    }

    fn check_ctor_pattern(
        &self,
        heap: &Heap,
        expected: &Type,
        v: Value,
    ) -> Result<(Pattern, Vec<(String, Type)>), Error> {
        let parts = heap.list_to_vec(v)?;
        let ctor = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("pattern: constructor must be a symbol".into())),
        };
        // The constructor is resolved against the scrutinee's type (its
        // variants), not the namespace — the type context disambiguates it.
        let (adt_name, targs) = self.expect_adt(expected)?;
        let def = self.reg.type_def(&adt_name).expect("adt exists").clone();
        let variant = def.variants.iter().position(|vr| vr.name == ctor).ok_or_else(|| {
            Error::TypeError(format!("`{}` is not a constructor of `{}`", ctor, adt_name))
        })?;
        let subst: HashMap<String, Type> =
            def.params.iter().cloned().zip(targs.iter().cloned()).collect();

        let fields = &def.variants[variant].fields;
        if parts.len() - 1 != fields.len() {
            return Err(Error::TypeError(format!(
                "pattern `{}`: expected {} field(s), got {}",
                ctor,
                fields.len(),
                parts.len() - 1
            )));
        }
        let mut sub_pats = Vec::new();
        let mut binds = Vec::new();
        for (sub, field) in parts[1..].iter().zip(fields.iter()) {
            let field_ty = subst_apply(field, &subst);
            let (p, b) = self.check_pattern(heap, &field_ty, *sub)?;
            sub_pats.push(p);
            binds.extend(b);
        }
        Ok((Pattern::Ctor { type_name: adt_name, variant, args: sub_pats }, binds))
    }

    // ---- helpers ----------------------------------------------------------

    /// Check a body sequence (`expr...`), returning the typed exprs and the type
    /// of the last (the sequence's type; `Unit` if empty). Only the last element
    /// is checked against `expected`.
    fn check_seq(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        body: &[Value],
        expected: Option<&Type>,
    ) -> Result<(Vec<Typed>, Type), Error> {
        if body.is_empty() {
            if let Some(e) = expected {
                if *e != Type::Unit {
                    return Err(Error::TypeError(format!(
                        "empty body has type Unit, expected {:?}",
                        e
                    )));
                }
            }
            return Ok((Vec::new(), Type::Unit));
        }
        let mut out = Vec::new();
        let last = body.len() - 1;
        for (i, expr) in body.iter().enumerate() {
            let exp = if i == last { expected } else { None };
            out.push(self.check(heap, interp, env, *expr, exp)?);
        }
        let ty = out[last].ty.clone();
        Ok((out, ty))
    }

    /// Require `ty` to be a registered data type, returning its path and args.
    fn expect_adt(&self, ty: &Type) -> Result<(Path, Vec<Type>), Error> {
        match ty {
            Type::Named(n, args) if self.reg.type_def(n).is_some() => Ok((n.clone(), args.clone())),
            other => Err(Error::TypeError(format!(
                "expected a data type, found {:?}",
                other
            ))),
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Checker::new()
    }
}

// ---- free helpers ---------------------------------------------------------

/// Recursively convert a raw read `Value` into an owned [`QuotedSexpr`] (see
/// [`Expr::Quote`] for why `quote` can't just keep the heap pointer). A
/// `Value::Path` (a `::`-qualified token, e.g. `a::b`, appearing inside quoted
/// data) has no `Sexpr` counterpart yet (the built-in `Sexpr` ADT doesn't have
/// a `Path` variant — see `docs/language-design.md` §2.1) so it is rejected.
fn value_to_quoted(heap: &Heap, v: Value) -> Result<QuotedSexpr, Error> {
    Ok(match v {
        Value::Empty => QuotedSexpr::Nil,
        Value::Int(n) => QuotedSexpr::Int(n),
        Value::Float(f) => QuotedSexpr::Float(f),
        Value::Char(c) => QuotedSexpr::Char(c),
        Value::Bool(b) => QuotedSexpr::Bool(b),
        Value::Symbol(id) => QuotedSexpr::Sym(heap.symbol_name(id).to_string()),
        Value::Str(id) => QuotedSexpr::Str(heap.string(id).to_string()),
        Value::Cons(_) => {
            let car = value_to_quoted(heap, heap.car(v)?)?;
            let cdr = value_to_quoted(heap, heap.cdr(v)?)?;
            QuotedSexpr::Cons(Box::new(car), Box::new(cdr))
        }
        Value::Path(_) => {
            return Err(Error::TypeError(
                "quote: `::`-paths inside quoted data are not yet supported".into(),
            ))
        }
    })
}

/// Whether `v` is the symbol named `name`.
fn is_symbol(heap: &Heap, v: Value, name: &str) -> bool {
    matches!(v, Value::Symbol(id) if heap.symbol_name(id) == name)
}

/// `Some(t)` unless `t` is `Never` (which never constrains an expectation).
fn non_never(t: &Type) -> Option<&Type> {
    if *t == Type::Never {
        None
    } else {
        Some(t)
    }
}

/// Join two branch types: `Never` is absorbed by the other; otherwise the two
/// must be equal.
fn join_types(a: &Type, b: &Type) -> Result<Type, Error> {
    if *a == Type::Never {
        return Ok(b.clone());
    }
    if *b == Type::Never {
        return Ok(a.clone());
    }
    if a == b {
        return Ok(a.clone());
    }
    Err(Error::TypeError(format!(
        "branches have incompatible types: {:?} vs {:?}",
        a, b
    )))
}

/// `Sexpr` — the type a `&rest` parameter's collected arguments are bound as
/// inside a `defun`/`lambda` body, and the type `apply`'s trailing list
/// argument must have (see `Checker::parse_params_rest`/`check_call`/
/// `check_apply_form`). Every Lisp's `&rest` parameter is an ordinary list
/// (built from cons cells), never a homogeneous array type — `defmacro`'s own
/// `&rest` already works this way unconditionally (`Interp::bind_macro_args`);
/// `defun`/`lambda`'s `&rest` now matches it, just with an added call-site
/// check (below) that every individual variadic argument satisfies the
/// declared element type before it's wrapped into the list.
fn sexpr_ty() -> Type {
    Type::Named(Path::root("sexpr"), vec![])
}

/// The `Sexpr` constructor name that exactly represents a value of `elem_ty`
/// (`Sexpr` itself needs no wrapping — see [`Checker::wrap_rest_elem`]).
/// `&rest`'s declared element type must be one of these (or `Sexpr`) since
/// nothing else has a lossless `Sexpr` encoding to collect into a list with.
fn sexpr_ctor_for(elem_ty: &Type) -> Option<&'static str> {
    match elem_ty {
        Type::I64 | Type::I32 => Some("int"),
        Type::F64 => Some("float"),
        Type::Char => Some("char"),
        Type::Bool => Some("bool"),
        Type::Str => Some("str"),
        _ => None,
    }
}

fn is_integer_type(t: &Type) -> bool {
    matches!(
        t,
        Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Usize
    )
}

/// The type of an integer literal: the expected integer type if any, else `i32`.
fn int_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t) if is_integer_type(t) => t.clone(),
        _ => Type::I32,
    }
}

/// The type of a float literal: the expected float type if any, else `f64`.
fn float_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t @ Type::F32) | Some(t @ Type::F64) => t.clone(),
        _ => Type::F64,
    }
}

/// Whether `path` is a type variable in `params` (a single-segment name).
fn is_param(path: &crate::Path, params: &HashSet<String>) -> bool {
    path.is_simple() && params.contains(path.local())
}

/// Whether `t` mentions any type parameter in `params`.
fn type_has_param(t: &Type, params: &HashSet<String>) -> bool {
    match t {
        Type::Named(n, args) => {
            (args.is_empty() && is_param(n, params)) || args.iter().any(|a| type_has_param(a, params))
        }
        Type::Fn(ps, rest, r) => {
            ps.iter().any(|p| type_has_param(p, params))
                || matches!(rest, Some(t) if type_has_param(t, params))
                || type_has_param(r, params)
        }
        _ => false,
    }
}

/// Replace type parameters in `t` with their bindings from `subst`.
fn subst_apply(t: &Type, subst: &HashMap<String, Type>) -> Type {
    match t {
        Type::Named(n, args) if args.is_empty() && n.is_simple() => match subst.get(n.local()) {
            Some(bound) => bound.clone(),
            None => t.clone(),
        },
        Type::Named(n, args) => {
            Type::Named(n.clone(), args.iter().map(|a| subst_apply(a, subst)).collect())
        }
        Type::Fn(ps, rest, r) => Type::Fn(
            ps.iter().map(|p| subst_apply(p, subst)).collect(),
            rest.as_ref().map(|t| Box::new(subst_apply(t, subst))),
            Box::new(subst_apply(r, subst)),
        ),
        other => other.clone(),
    }
}

/// Unify a field template (which may mention `params`) against a concrete
/// `actual` type, accumulating parameter bindings in `subst`.
fn unify(
    params: &HashSet<String>,
    tmpl: &Type,
    actual: &Type,
    subst: &mut HashMap<String, Type>,
) -> Result<(), Error> {
    // A diverging value (`Never`) unifies with any template without binding.
    if *actual == Type::Never {
        return Ok(());
    }
    if let Type::Named(n, args) = tmpl {
        if args.is_empty() && is_param(n, params) {
            let key = n.local().to_string();
            return match subst.get(&key) {
                Some(bound) if bound == actual => Ok(()),
                Some(bound) => Err(Error::TypeError(format!(
                    "conflicting types for `{}`: {:?} vs {:?}",
                    key, bound, actual
                ))),
                None => {
                    subst.insert(key, actual.clone());
                    Ok(())
                }
            };
        }
    }
    match (tmpl, actual) {
        (Type::Named(n1, a1), Type::Named(n2, a2)) if n1 == n2 && a1.len() == a2.len() => {
            for (t, a) in a1.iter().zip(a2.iter()) {
                unify(params, t, a, subst)?;
            }
            Ok(())
        }
        (Type::Fn(p1, rest1, r1), Type::Fn(p2, rest2, r2)) if p1.len() == p2.len() => {
            for (t, a) in p1.iter().zip(p2.iter()) {
                unify(params, t, a, subst)?;
            }
            match (rest1, rest2) {
                (Some(t1), Some(t2)) => unify(params, t1, t2, subst)?,
                (None, None) => {}
                _ => {
                    return Err(Error::TypeError(format!(
                        "type mismatch: expected {:?}, found {:?}",
                        tmpl, actual
                    )))
                }
            }
            unify(params, r1, r2, subst)
        }
        _ if tmpl == actual => Ok(()),
        _ => Err(Error::TypeError(format!(
            "type mismatch: expected {:?}, found {:?}",
            tmpl, actual
        ))),
    }
}
