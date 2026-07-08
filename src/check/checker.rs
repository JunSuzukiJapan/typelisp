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
use super::registry::{AdtDef, AdtKind, AssocFn, FnSig, MacroDef, Namespace, Registry, TraitBound, TraitDef, VarInfo, Variant};

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
    /// `params` is the function's fixed parameter list — `defun`/`lambda`
    /// have no `&rest` (variadic value-level functions were removed); only
    /// `defmacro` still has a `&rest` lambda-list marker. See
    /// `Checker::check_defun`/`parse_params`.
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
        /// The owner type's parameters as the receiver spelled them
        /// (`(self Option<U>)` -> `["u"]`) when this is a *generic-owner*
        /// method — the erased, diagnostics-only artifact `Interp::exec`
        /// must skip, exactly like `Defun::type_params`; empty for a
        /// concrete method or a generated specialization.
        type_params: Vec<String>,
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

/// A lexical environment mapping variable names to their types.
#[derive(Clone)]
struct Env {
    vars: Vec<(String, Type)>,
    /// Trait bounds on this function's own generic type parameters, declared
    /// by a `(where (Trait T (Assoc Concrete)...)...)` clause
    /// (`Checker::check_defun`) — e.g. `{"t": [TraitBound { trait_path:
    /// Path::root("iter"), assoc: {} }]}`. Fixed for the lifetime of one
    /// function body (a nested `let`/`lambda` never introduces new type
    /// parameters), so `extended` just carries the same `Rc` forward instead
    /// of letting it vary per-binding the way `vars` does. Consulted only by
    /// `Checker::check_instance_method`'s type-variable-receiver branch — see
    /// its doc comment. The same bounds are also stored on the registered
    /// `FnSig` (`Checker::check_defun`), for `Checker::check_call`'s
    /// call-site validation — this copy exists only because body-checking
    /// needs it readily available in `Env`, not threaded through `Registry`
    /// lookups on every method call.
    bounds: std::rc::Rc<HashMap<String, Vec<TraitBound>>>,
}

impl Env {
    fn new() -> Env {
        Env { vars: Vec::new(), bounds: std::rc::Rc::new(HashMap::new()) }
    }

    fn get(&self, name: &str) -> Option<&Type> {
        self.vars.iter().rev().find(|(n, _)| n == name).map(|(_, t)| t)
    }

    /// A child environment with `binds` added (later bindings shadow earlier).
    fn extended(&self, binds: Vec<(String, Type)>) -> Env {
        let mut vars = self.vars.clone();
        vars.extend(binds);
        Env { vars, bounds: self.bounds.clone() }
    }

    /// A child environment that additionally declares `bounds` (a function's
    /// own `where`-clause trait bounds) — see the field's doc comment.
    fn with_bounds(&self, bounds: HashMap<String, Vec<TraitBound>>) -> Env {
        Env { vars: self.vars.clone(), bounds: std::rc::Rc::new(bounds) }
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
impl Definable for TraitDef {
    fn builtin(&self) -> bool {
        self.builtin
    }
}

/// A generic `defun`'s retained raw source form, for re-checking at each
/// concrete instantiation — see [`Checker::specialize_defun`]. The checked
/// `Typed` body can't serve this purpose: it was lowered once with the type
/// variables still abstract, and everything the checker *derives* from types
/// (`Expr::Construct::mutable`, `Pattern::Ctor::sexpr_fields`, method
/// resolution, `Expr::TraitCall` vs `Expr::Assoc`) would have to be re-derived
/// anyway — re-running the checker on the source with the type variables
/// bound (see `Checker::type_var_bindings`) gets all of that for free.
#[derive(Clone)]
struct FnTemplate {
    /// The `defun` form's parts (everything after the `defun` symbol),
    /// exactly as `check_defun` received them. Each `Value` is permanently
    /// rooted in the heap (`Heap::push_permanent_root`) — a template must
    /// outlive any number of GC cycles between its definition and its last
    /// instantiation.
    parts: Vec<Value>,
    /// The namespace the `defun` was defined in — specialization re-checks
    /// the body under the *defining* module's name resolution, not the
    /// caller's.
    ns: Vec<String>,
    type_params: Vec<String>,
}

/// One queued "generate this instantiation" work item — see
/// [`Checker::request_fn_specialization`]/[`Checker::request_method_specialization`]
/// (producers, called from the `&self` expression-checking context) and
/// [`Checker::drain_specializations`] (consumer, run by `check_form` with
/// `&mut self`). The two-phase split exists because expression checking
/// (`check_call`/`check_assoc_call`) cannot re-enter top-level definition
/// checking itself.
enum SpecRequest {
    /// A generic free function (`FnTemplate`) at concrete `args`.
    Fn { base: Path, args: Vec<Type>, mangled: Path },
    /// A generic-owner associated method (`MethodTemplate`, keyed
    /// `(type_fq, base)`) at the owner's concrete type arguments `args`.
    Method { type_fq: Path, base: String, args: Vec<Type>, mangled: String },
}

/// A generic *type*'s associated method, retained for per-instantiation
/// re-generation — the method-side counterpart of [`FnTemplate`]. A method
/// is generic through its *owner* (`Option<T>`'s `unwrap`, a generic
/// `defstruct`'s accessors), never through parameters of its own, so the
/// instantiation arguments are always the owner's type arguments.
#[derive(Clone)]
enum MethodTemplate {
    /// A written `defmethod` whose receiver names the owner's type
    /// parameters (directly, or synthesized by `check_impl`'s
    /// `Self`-substitution). `written_vars` are the type-variable names *as
    /// the receiver spelled them* (`(self Option<U>)` -> `["u"]`), zipped
    /// positionally with the owner's concrete type arguments at
    /// specialization time — they need not match the `AdtDef::params`
    /// names the call site's substitution is keyed by.
    Form { parts: Vec<Value>, ns: Vec<String>, written_vars: Vec<String> },
    /// A generic `defstruct` field getter — there is no raw form to
    /// re-check (`check_defstruct` synthesizes accessor ASTs directly), so
    /// specialization re-synthesizes the `FieldGet` with the field type
    /// substituted from the owner's `AdtDef`.
    Getter { index: usize },
    /// The matching `set-<field>` setter (`FieldSet`).
    Setter { index: usize },
}

/// [`Checker::parse_defmethod_sig`]'s output — a `defmethod` form's parsed
/// header, shared between definition checking and template specialization.
struct MethodSig {
    method: String,
    instance: bool,
    self_name: Option<String>,
    recv_ty: Type,
    type_fq: Path,
    params: Vec<(String, Type)>,
    ret: Type,
    /// `(where (Trait TypeVar ...))` bounds after the return type — same
    /// syntax and parse as a free `defun`'s (`Checker::parse_defun_sig`).
    /// Lets an `impl` method on a generic owner require trait bounds on the
    /// owner's own type variables (e.g. `cons-cell<A,B>`'s `equals` needing
    /// `(where (Eq A) (Eq B))` for its recursive field comparisons).
    bounds: HashMap<String, Vec<TraitBound>>,
    /// Index of the first body form in the `defmethod` parts: 4 when a
    /// `where` clause is present, 3 otherwise.
    body_start: usize,
}

/// The synthetic `TopLevel::Module` path under which `check_form` bundles
/// monomorphized specializations together with the form that requested them.
/// Contains a space, which [`crate::read::Reader`] treats as a delimiter — so
/// no user-written `module` form can ever collide with it (see
/// `mangled_fn_path` for the same trick on function names).
pub const MONO_BUNDLE_MODULE: &str = "<monomorph specializations>";

/// Upper bound on specializations generated while draining one top-level
/// form's requests — the convergence guard for polymorphic recursion (a
/// generic function calling itself at an ever-growing type, e.g. `(f T)`
/// recursing as `(f Option<T>)`), which memoization alone cannot stop since
/// every instantiation is new. Ordinary programs instantiate a handful of
/// generics per form; hitting this is a `TypeError`, not a hang.
const SPECIALIZATION_BUDGET: usize = 512;

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
    /// Every generic `defun`'s retained source form, keyed by its
    /// fully-qualified path — see [`FnTemplate`].
    generic_fn_templates: HashMap<Path, FnTemplate>,
    /// Every generic-owner method's retained template, keyed by
    /// `(owner type path, method name)` — see [`MethodTemplate`].
    generic_method_templates: HashMap<(Path, String), MethodTemplate>,
    /// Instantiations already requested *during the current top-level form* —
    /// a free function as `(mangled path, None)`, a method as
    /// `(owner type path, Some(mangled method))` — so one form's many calls
    /// to the same instantiation — including a specialization's own
    /// recursive call to itself — produce exactly one definition.
    /// Deliberately cleared at the end of every `check_form` rather than
    /// kept for the checker's lifetime: each form's output bundle is then
    /// self-contained, so a caller that checks several forms but discards
    /// some without executing them (the REPL does exactly this when a later
    /// form in the same paste fails to check) can never leave a later form
    /// referring to a specialization whose defining bundle was thrown away.
    /// The cost — a later form re-generating an instantiation an earlier
    /// form already produced, and `Interp::exec` silently overwriting the
    /// identical earlier registration — is compile-time-only.
    spec_memo: RefCell<HashSet<(Path, Option<String>)>>,
    /// Instantiations requested but not yet generated — see [`SpecRequest`].
    /// Always empty outside `check_form`.
    spec_pending: RefCell<Vec<SpecRequest>>,
    /// The current specialization's type-variable bindings (`T` -> concrete
    /// type), consulted by [`Self::canon`] — the one chokepoint every parsed
    /// type annotation passes through, which is what makes "re-check the
    /// template with its type variables bound" work without touching the
    /// type parser: `Option<T>`'s `T` arrives here already split out as a
    /// structured `Type::Named` argument. Non-empty only while
    /// [`Self::specialize_defun`] runs.
    type_var_bindings: HashMap<String, Type>,
}

impl Checker {
    pub fn new() -> Checker {
        Checker {
            reg: Registry::with_builtins(),
            ns: Vec::new(),
            loop_stack: RefCell::new(Vec::new()),
            redef_policy: RedefPolicy::default(),
            warnings: RefCell::new(Vec::new()),
            generic_fn_templates: HashMap::new(),
            generic_method_templates: HashMap::new(),
            spec_memo: RefCell::new(HashSet::new()),
            spec_pending: RefCell::new(Vec::new()),
            type_var_bindings: HashMap::new(),
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
    ///
    /// If checking the form instantiated any generic function at concrete
    /// types (see [`Self::request_fn_specialization`]), the generated
    /// specializations are bundled *with* the form into a single synthetic
    /// `TopLevel::Module` (path [`MONO_BUNDLE_MODULE`], specializations
    /// first) — `Interp::exec`'s `Module` arm runs the body in order, so the
    /// caller needs no new handling, and the bundle is self-contained: a
    /// form's specializations can never be separated from the form that
    /// needs them (see `spec_memo`'s doc comment for why that matters).
    pub fn check_form(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value) -> Result<TopLevel, Error> {
        debug_assert!(
            self.spec_pending.borrow().is_empty(),
            "specialization requests must never leak across check_form calls"
        );
        let primary = self.check_form_dispatch(heap, interp, v);
        let bundled = primary.and_then(|tl| Ok((self.drain_specializations(heap, interp)?, tl)));
        // Both maps reset per form regardless of outcome — see `spec_memo`'s
        // doc comment for why the memo must not outlive the form.
        self.spec_memo.borrow_mut().clear();
        match bundled {
            Ok((specs, tl)) if specs.is_empty() => Ok(tl),
            Ok((mut specs, tl)) => {
                specs.push(tl);
                Ok(TopLevel::Module { path: Path::root(MONO_BUNDLE_MODULE), body: specs })
            }
            Err(e) => {
                self.spec_pending.borrow_mut().clear();
                Err(e)
            }
        }
    }

    /// [`Self::check_form`]'s dispatch body (the pre-monomorphization
    /// `check_form`, unchanged) — split out so the public entry point can
    /// wrap it with specialization draining.
    fn check_form_dispatch(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value) -> Result<TopLevel, Error> {
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
                    "deftrait" => return self.check_deftrait(heap, &elems[1..], false),
                    "impl" => return self.check_impl(heap, interp, &elems[1..], false),
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

    /// Whether an associated function/method `af` of `type_fq` is reachable
    /// from the current namespace: public, or declared in the same module —
    /// mirrors `resolve_fn_path`'s cross-module `public` check. Used at every
    /// call site that looks up `def.assoc` directly (`try_field_access`,
    /// `check_field_set`, `check_path_call`'s static-member branch,
    /// `try_instance_method`, `check_instance_method`) so a private member —
    /// including a `defstruct` field whose accessor wasn't declared
    /// `pub` — is treated as absent rather than merely forbidden, matching
    /// how a private free function or constructor "doesn't resolve" instead
    /// of erroring with a privacy-specific message.
    fn assoc_visible(&self, type_fq: &Path, af: &AssocFn) -> bool {
        af.sig.public || self.same_module(type_fq.parent())
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

    /// The `Sexpr` `cons` variant's `(type path, variant index)`, read from the
    /// type definition directly rather than the bare-name `ctors` map — that
    /// bare `cons` name was removed (Symbol/Sexpr redesign Phase 4b) so `(cons
    /// a b)` resolves to the free `cons<T,U>` pair function. Internal `Sexpr`
    /// cons construction (`list` literals, quasiquote) still needs the variant,
    /// and reaches it through here.
    fn sexpr_cons_ctor(&self) -> (Path, usize) {
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
    fn resolve_static_use(&self, name: &str) -> Option<(Path, String)> {
        if let Some(hit) = self.cur_ns().static_uses.get(name) {
            return Some(hit.clone());
        }
        self.reg.root.static_uses.get(name).cloned()
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
    /// one. `Some(Err(..))` when the name resolves but is a generic function
    /// that can't be instantiated here — see [`Self::fn_ref_node`].
    fn fn_value(&self, name: &str, expected: Option<&Type>) -> Option<Result<Typed, Error>> {
        let fq = self.resolve_fn(name)?;
        Some(self.fn_ref_node(fq, expected))
    }

    /// Reify a qualified free-function path as a function value (`FnRef`).
    fn fn_path_value(&self, segs: &[String], expected: Option<&Type>) -> Option<Result<Typed, Error>> {
        let fq = self.resolve_fn_path(segs)?;
        Some(self.fn_ref_node(fq, expected))
    }

    /// Build an `FnRef` node carrying the function's `(fn ...)` type.
    ///
    /// A *generic* function has no single value to reference — post-
    /// monomorphization, only concrete specializations exist at runtime — so
    /// its type parameters are resolved against the `expected` function type
    /// (the only call-site information a bare name carries, e.g. a typed
    /// `let` binding or the parameter an argument is checked against) and
    /// the reference is rewritten to the specialization, requested exactly
    /// like a direct call's. With no expectation to resolve from this is a
    /// check-time error (previously it produced a type-variable-ridden
    /// `FnRef` that could never be applied anyway).
    fn fn_ref_node(&self, fq: Path, expected: Option<&Type>) -> Result<Typed, Error> {
        let sig = self.reg.fn_sig(&fq).expect("resolved fn exists").clone();
        let tmpl_ty = Type::Fn(sig.params.clone(), Box::new(sig.ret.clone()));
        if sig.type_params.is_empty() {
            return Ok(Typed { expr: Expr::FnRef(fq), ty: tmpl_ty });
        }
        if let Some(exp) = expected {
            let params: HashSet<String> = sig.type_params.iter().cloned().collect();
            let mut subst: HashMap<String, Type> = HashMap::new();
            if unify(&params, &tmpl_ty, exp, &mut subst).is_ok()
                && sig.type_params.iter().all(|p| subst.contains_key(p))
            {
                let targs: Vec<Type> = sig.type_params.iter().map(|p| subst[p.as_str()].clone()).collect();
                let resolved_ty = subst_apply(&tmpl_ty, &subst);
                if !targs.iter().any(|t| self.type_is_open(t)) && self.generic_fn_templates.contains_key(&fq) {
                    let mangled = self.request_fn_specialization(&fq, targs);
                    return Ok(Typed { expr: Expr::FnRef(mangled), ty: resolved_ty });
                }
                // Open type arguments: we're inside another generic
                // function's diagnostics-only body check — the node is never
                // executed, and that function's own specialization will
                // re-check this reference with the types concrete.
                return Ok(Typed { expr: Expr::FnRef(fq), ty: resolved_ty });
            }
            // Shape mismatch: hand back the generic node so `check`'s
            // ordinary expected-vs-actual reconciliation reports it.
            return Ok(Typed { expr: Expr::FnRef(fq), ty: tmpl_ty });
        }
        Err(Error::TypeError(format!(
            "generic function `{}` used as a value needs a concrete function-type context \
             (e.g. a typed let binding, parameter position, or `the`)",
            fq
        )))
    }

    /// Reify a bare name as an *instance method* value (`MethodRef`), e.g.
    /// `+` where a `(fn (i32 i32) i32)` is expected (see
    /// [`builtin_as_value`](../../../tests/eval_test.rs)-style usage of
    /// `registry::int_assoc`'s arithmetic). Unlike a real call site, there is
    /// no receiver expression to dispatch on, so the receiver type is read
    /// off `expected`'s first parameter instead.
    ///
    /// A generic-*owner* method with a retained [`MethodTemplate`] resolves
    /// the owner's type arguments from `expected` (its first parameter is
    /// the receiver's concrete type) and is rewritten to the specialization,
    /// mirroring [`Self::fn_ref_node`]. A generic method *without* a
    /// template (a Rust builtin like `HashTable::get`) still can't be
    /// reified and fails to match, as before.
    fn method_value(&self, name: &str, expected: Option<&Type>) -> Option<Typed> {
        let Type::Fn(params, _) = expected? else { return None };
        let type_fq = match params.first()? {
            Type::Named(n, _) => n.clone(),
            other => prim_type_path(other)?,
        };
        let def = self.reg.type_def(&type_fq)?;
        let af = def.assoc.get(name)?;
        if !af.instance {
            return None;
        }
        let ty = Type::Fn(af.sig.params.clone(), Box::new(af.sig.ret.clone()));
        if !def.params.is_empty() {
            let tparams: HashSet<String> = def.params.iter().cloned().collect();
            let mut subst: HashMap<String, Type> = HashMap::new();
            if unify(&tparams, &ty, expected.unwrap(), &mut subst).is_ok()
                && def.params.iter().all(|p| subst.contains_key(p))
            {
                let targs: Vec<Type> = def.params.iter().map(|p| subst[p.as_str()].clone()).collect();
                if !targs.iter().any(|t| self.type_is_open(t))
                    && self.generic_method_templates.contains_key(&(type_fq.clone(), name.to_string()))
                {
                    let mangled = self.request_method_specialization(&type_fq, name, targs);
                    return Some(Typed {
                        expr: Expr::MethodRef { type_name: type_fq, method: mangled },
                        ty: subst_apply(&ty, &subst),
                    });
                }
            }
            return None;
        }
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
        if !af.instance || !self.assoc_visible(type_fq, af) {
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
    ///
    /// This is also monomorphization's substitution point: while a generic
    /// template is being re-checked ([`Self::specialize_defun`]), a
    /// single-segment name bound in `type_var_bindings` resolves to its
    /// concrete type instead — checked *before* `resolve_type_name` so a
    /// user-defined type that happens to share a type parameter's name can
    /// never shadow the binding.
    fn canon(&self, t: &Type) -> Type {
        match t {
            Type::Named(n, args) => {
                if args.is_empty() && n.is_simple() {
                    if let Some(bound) = self.type_var_bindings.get(n.local()) {
                        return bound.clone();
                    }
                }
                Type::Named(
                    self.resolve_type_name(n),
                    args.iter().map(|a| self.canon(a)).collect(),
                )
            }
            Type::Fn(ps, r) => Type::Fn(
                ps.iter().map(|p| self.canon(p)).collect(),
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
        let (params, ret, bounds, body_start) = self.parse_defun_sig(heap, parts)?;
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;
        let fq_name = self.fq(&name);

        // Register the signature in the current namespace before checking the
        // body so self-recursion works.
        self.check_redef("function", &name, self.cur_ns().fns.get(&name))?;

        // A generic defun additionally retains its raw source form for
        // per-instantiation re-checking — see `FnTemplate`. Retained *before*
        // the body check below: the body may itself contain a call that
        // instantiates this very function at concrete types (polymorphic
        // recursion — rejected later by `SPECIALIZATION_BUDGET`, but the
        // request still consults the template map first).
        if !type_params.is_empty() {
            for &p in parts {
                heap.push_permanent_root(p);
            }
            self.generic_fn_templates.insert(
                fq_name.clone(),
                FnTemplate { parts: parts.to_vec(), ns: self.ns.clone(), type_params: type_params.clone() },
            );
        }

        let sig = FnSig {
            type_params: type_params.clone(),
            params: params.iter().map(|(_, t)| t.clone()).collect(),
            ret: ret.clone(),
            public,
            builtin: false,
            bounds: bounds.clone(),
        };
        self.reg.root.module_mut(&self.ns).fns.insert(name.clone(), sig);

        let env = Env::new().with_bounds(bounds).extended(params.clone());
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], Some(&ret))?;
        Ok(TopLevel::Defun { name: fq_name, type_params, params, ret, body })
    }

    /// Parses a `defun` form's parameter list, return type, and optional
    /// `(where ...)` clause (`parts` = everything after the `defun` keyword),
    /// returning them plus the body's starting index in `parts`. Shared by
    /// [`Self::check_defun`] and [`Self::specialize_defun`] — the latter
    /// re-parses a retained generic template with `type_var_bindings` in
    /// effect, so the very same source annotations come back concrete.
    ///
    /// The `where` clause is parsed *before* the `FnSig` is registered (only
    /// `parts`/`heap` are needed, nothing computed later) so the bounds can
    /// be stored on the signature itself — `Checker::check_call` consults
    /// them for call-site validation, not just body-checking's `Env`.
    #[allow(clippy::type_complexity)]
    fn parse_defun_sig(
        &self,
        heap: &Heap,
        parts: &[Value],
    ) -> Result<(Vec<(String, Type)>, Type, HashMap<String, Vec<TraitBound>>, usize), Error>
    {
        if parts.len() < 3 {
            return Err(Error::TypeError("defun: (defun name (params) ret body...)".into()));
        }
        let params = self.parse_params(heap, parts[1])?;
        let ret = self.canon(&parse_type(heap, parts[2])?);
        let mut body_start = 3;
        let mut bounds: HashMap<String, Vec<TraitBound>> = HashMap::new();
        if let Some(form) = parts.get(3) {
            if self.is_where_clause(heap, *form)? {
                bounds = self.parse_where_clause(heap, *form)?;
                body_start = 4;
            }
        }
        Ok((params, ret, bounds, body_start))
    }

    // ---- monomorphization ---------------------------------------------------

    /// Records that the current form needs `base` (a generic `defun` with a
    /// retained [`FnTemplate`]) instantiated at the concrete `args`, and
    /// returns the specialized function's mangled [`Path`] for the call site
    /// to reference instead of `base`. Idempotent within one top-level form
    /// (`spec_memo`); the actual definition is generated later by
    /// [`Self::drain_specializations`], because this runs inside expression
    /// checking (`&self`) which cannot re-enter definition checking.
    fn request_fn_specialization(&self, base: &Path, args: Vec<Type>) -> Path {
        let mangled = mangled_fn_path(base, &args);
        let mut memo = self.spec_memo.borrow_mut();
        if memo.insert((mangled.clone(), None)) {
            self.spec_pending.borrow_mut().push(SpecRequest::Fn {
                base: base.clone(),
                args,
                mangled: mangled.clone(),
            });
        }
        mangled
    }

    /// [`Self::request_fn_specialization`]'s method-side counterpart: records
    /// that `type_fq`'s method `base` needs generating at the owner's
    /// concrete type arguments `args`, returning the mangled method name for
    /// the `Expr::Assoc` node to reference. Only ever called when a
    /// [`MethodTemplate`] is retained for the pair.
    fn request_method_specialization(&self, type_fq: &Path, base: &str, args: Vec<Type>) -> String {
        let mangled = mangled_method_name(base, &args);
        let mut memo = self.spec_memo.borrow_mut();
        if memo.insert((type_fq.clone(), Some(mangled.clone()))) {
            self.spec_pending.borrow_mut().push(SpecRequest::Method {
                type_fq: type_fq.clone(),
                base: base.to_string(),
                args,
                mangled: mangled.clone(),
            });
        }
        mangled
    }

    /// Generates every specialization the current form requested (and any a
    /// specialization's own body requests in turn — the worklist converges
    /// for ordinary mutual recursion because `spec_memo` dedupes by mangled
    /// name, and is cut off by [`SPECIALIZATION_BUDGET`] for polymorphic
    /// recursion, which produces a genuinely new instantiation every step).
    fn drain_specializations(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
    ) -> Result<Vec<TopLevel>, Error> {
        let mut out = Vec::new();
        loop {
            let req = self.spec_pending.borrow_mut().pop();
            let Some(req) = req else { break };
            if out.len() >= SPECIALIZATION_BUDGET {
                let last = match &req {
                    SpecRequest::Fn { mangled, .. } => mangled.to_string(),
                    SpecRequest::Method { type_fq, mangled, .. } => format!("{}::{}", type_fq, mangled),
                };
                return Err(Error::TypeError(format!(
                    "monomorphization did not converge after {} instantiations (a polymorphically \
                     recursive generic function — one that calls itself at an ever-growing type — \
                     cannot be compiled; last requested: {})",
                    SPECIALIZATION_BUDGET, last
                )));
            }
            out.push(match req {
                SpecRequest::Fn { .. } => self.specialize_defun(heap, interp, &req)?,
                SpecRequest::Method { .. } => self.specialize_method(heap, interp, &req)?,
            });
        }
        Ok(out)
    }

    /// Re-checks `req.base`'s retained template with its type variables bound
    /// to `req.args`, producing a fully concrete `TopLevel::Defun` named
    /// `req.mangled`. The re-check runs under the template's *defining*
    /// namespace and a fresh loop stack (the same isolation `check_lambda`
    /// applies to a nested function body — a specialization requested from
    /// inside somebody's `loop` must not see that loop as its own).
    fn specialize_defun(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        req: &SpecRequest,
    ) -> Result<TopLevel, Error> {
        let SpecRequest::Fn { base, args, mangled } = req else {
            unreachable!("drain routes Fn requests here")
        };
        let tmpl = self
            .generic_fn_templates
            .get(base)
            .expect("a specialization is only ever requested for a retained template")
            .clone();
        let bindings: HashMap<String, Type> =
            tmpl.type_params.iter().cloned().zip(args.iter().cloned()).collect();
        let (saved_ns, saved_loops, saved_bindings) = self.enter_specialization(tmpl.ns.clone(), bindings);
        let result = self.specialize_defun_body(heap, interp, &tmpl, mangled);
        self.exit_specialization(saved_ns, saved_loops, saved_bindings);
        result
    }

    /// Swaps in a specialization re-check's context — the template's defining
    /// namespace, a fresh loop stack (the same isolation `check_lambda`
    /// applies to a nested function body: a specialization requested from
    /// inside somebody's `loop` must not see that loop as its own), and the
    /// type-variable bindings `canon` substitutes — returning the saved state
    /// for [`Self::exit_specialization`] to restore.
    fn enter_specialization(
        &mut self,
        ns: Vec<String>,
        bindings: HashMap<String, Type>,
    ) -> (Vec<String>, Vec<Type>, HashMap<String, Type>) {
        let saved_ns = std::mem::replace(&mut self.ns, ns);
        let saved_loops = std::mem::take(&mut *self.loop_stack.borrow_mut());
        let saved_bindings = std::mem::replace(&mut self.type_var_bindings, bindings);
        (saved_ns, saved_loops, saved_bindings)
    }

    fn exit_specialization(
        &mut self,
        saved_ns: Vec<String>,
        saved_loops: Vec<Type>,
        saved_bindings: HashMap<String, Type>,
    ) {
        self.type_var_bindings = saved_bindings;
        *self.loop_stack.borrow_mut() = saved_loops;
        self.ns = saved_ns;
    }

    /// Re-checks or re-synthesizes a generic-owner method at the owner's
    /// concrete type arguments — [`Self::specialize_defun`]'s method-side
    /// counterpart. A written method ([`MethodTemplate::Form`]) is re-checked
    /// from its retained source with the *receiver-written* type-variable
    /// names bound; a generic `defstruct` accessor (`Getter`/`Setter`) has no
    /// source to re-check and is re-synthesized from the `AdtDef`'s field
    /// types instead (the same ASTs `check_defstruct` builds, with the field
    /// type substituted).
    fn specialize_method(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        req: &SpecRequest,
    ) -> Result<TopLevel, Error> {
        let SpecRequest::Method { type_fq, base, args, mangled } = req else {
            unreachable!("drain routes Method requests here")
        };
        let tmpl = self
            .generic_method_templates
            .get(&(type_fq.clone(), base.clone()))
            .expect("a method specialization is only ever requested for a retained template")
            .clone();
        match tmpl {
            MethodTemplate::Form { parts, ns, written_vars } => {
                let bindings: HashMap<String, Type> =
                    written_vars.into_iter().zip(args.iter().cloned()).collect();
                let (saved_ns, saved_loops, saved_bindings) = self.enter_specialization(ns, bindings);
                let result = self.specialize_method_form(heap, interp, &parts, mangled);
                self.exit_specialization(saved_ns, saved_loops, saved_bindings);
                result
            }
            MethodTemplate::Getter { index } => Ok(self.synthesize_accessor(type_fq, args, index, mangled, true)),
            MethodTemplate::Setter { index } => Ok(self.synthesize_accessor(type_fq, args, index, mangled, false)),
        }
    }

    fn specialize_method_form(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        mangled: &str,
    ) -> Result<TopLevel, Error> {
        let MethodSig { instance, self_name, recv_ty, type_fq, params, ret, body_start, .. } =
            self.parse_defmethod_sig(heap, parts)?;
        let mut binds: Vec<(String, Type)> = Vec::new();
        if let Some(s) = &self_name {
            binds.push((s.clone(), recv_ty));
        }
        binds.extend(params.clone());
        // Bounds are deliberately dropped here, mirroring
        // `specialize_defun_body`: with the owner's type variables concrete,
        // every bounded method call resolves against the real receiver type
        // (and was already validated at the call site).
        let env = Env::new().extended(binds);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], Some(&ret))?;
        Ok(TopLevel::Defmethod {
            type_name: type_fq,
            method: mangled.to_string(),
            instance,
            self_name,
            params,
            ret,
            body,
            type_params: Vec::new(),
        })
    }

    /// Builds a concrete accessor `TopLevel::Defmethod` for a generic
    /// `defstruct`'s field `index`, with the field type substituted at the
    /// owner's concrete `args` — mirrors the ASTs `check_defstruct`
    /// synthesizes, minus any registry mutation.
    fn synthesize_accessor(
        &self,
        type_fq: &Path,
        args: &[Type],
        index: usize,
        mangled: &str,
        getter: bool,
    ) -> TopLevel {
        let def = self.reg.type_def(type_fq).expect("an accessor template implies the type exists");
        let subst: HashMap<String, Type> =
            def.params.iter().cloned().zip(args.iter().cloned()).collect();
        let field_ty = subst_apply(&def.variants[0].fields[index], &subst);
        let recv_ty = Type::Named(type_fq.clone(), args.to_vec());
        let self_var = Typed { expr: Expr::Var("self".to_string()), ty: recv_ty.clone() };
        if getter {
            TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: mangled.to_string(),
                instance: true,
                self_name: Some("self".to_string()),
                params: Vec::new(),
                ret: field_ty.clone(),
                body: vec![Typed { expr: Expr::FieldGet(Box::new(self_var), index), ty: field_ty }],
                type_params: Vec::new(),
            }
        } else {
            let value_var = Typed { expr: Expr::Var("value".to_string()), ty: field_ty.clone() };
            TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: mangled.to_string(),
                instance: true,
                self_name: Some("self".to_string()),
                params: vec![("value".to_string(), field_ty)],
                ret: Type::Unit,
                body: vec![Typed {
                    expr: Expr::FieldSet(Box::new(self_var), index, Box::new(value_var)),
                    ty: Type::Unit,
                }],
                type_params: Vec::new(),
            }
        }
    }

    fn specialize_defun_body(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        tmpl: &FnTemplate,
        mangled: &Path,
    ) -> Result<TopLevel, Error> {
        // `bounds` deliberately dropped: the type variables are concrete
        // here, so method calls on them resolve directly against the real
        // receiver type (`check_instance_method`'s ordinary branch, never
        // the bounds branch), and the bounds' own validity was already
        // verified at the call site that requested this instantiation
        // (`check_call`'s where-clause validation).
        let (params, ret, _bounds, body_start) = self.parse_defun_sig(heap, &tmpl.parts)?;
        let env = Env::new().extended(params.clone());
        let (body, _) = self.check_seq(heap, interp, &env, &tmpl.parts[body_start..], Some(&ret))?;
        Ok(TopLevel::Defun { name: mangled.clone(), type_params: Vec::new(), params, ret, body })
    }

    /// Whether a declared type's *runtime representation* is a heap value
    /// (`RtValue::Sexpr` wrapping a `mem::Value`) — the built-in `Sexpr`
    /// itself, any `AdtKind::Struct` type (`defstruct`/`Vector<T>`/
    /// `cons-cell<K,V>`), `HashTable<K,V>` (boxed since the unification's
    /// Stage 5, though its `AdtDef` still says `Sum` — a recorded historical
    /// asymmetry), or a `Scope<V>` whose `V` is itself heap-repr (boxed
    /// since the unification's Stage 8; a scope of anything else — LLVM
    /// handles above all — stays Rust-native). This is the checker-side
    /// twin of the interpreter's `Interp::heap_repr_kind` (which carries
    /// the matching `Scope<V>` recursion), used to bake binding-slot
    /// routing into `Pattern::Bind` (the one binding site whose type the
    /// evaluator can't read off its own AST node).
    fn is_heap_repr(&self, ty: &Type) -> bool {
        match ty {
            Type::Named(p, args) if *p == Path::root("scope") && args.len() == 1 => self.is_heap_repr(&args[0]),
            Type::Named(p, _) => {
                *p == Path::root("sexpr")
                    || *p == Path::root("hashtable")
                    || self.reg.type_def(p).map(|d| d.kind == AdtKind::Struct).unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Whether `t` still mentions an unresolved type variable — a
    /// single-segment `Named` with no registered type definition, the same
    /// convention `check_call`'s where-clause validation already reads
    /// (post-`canon`, every *real* nominal type resolved to a located path
    /// with a `type_def`). True means the surrounding call is being checked
    /// inside another generic function's own (diagnostic) body, where the
    /// type only becomes concrete once *that* function is specialized — so
    /// no instantiation can be generated yet.
    fn type_is_open(&self, t: &Type) -> bool {
        match t {
            Type::Named(n, args) => {
                (args.is_empty() && n.is_simple() && self.reg.type_def(n).is_none())
                    || args.iter().any(|a| self.type_is_open(a))
            }
            Type::Fn(ps, r) => {
                ps.iter().any(|p| self.type_is_open(p)) || self.type_is_open(r)
            }
            _ => false,
        }
    }

    /// Whether `v` is a `(where ...)` clause (vs. an ordinary body form) —
    /// `Checker::check_defun` peeks at this to decide whether to consume it.
    fn is_where_clause(&self, heap: &Heap, v: Value) -> Result<bool, Error> {
        Ok(matches!(v, Value::Cons(_))
            && matches!(heap.list_to_vec(v)?.first(), Some(Value::Symbol(id)) if heap.symbol_name(*id) == "where"))
    }

    /// `(where (Trait1 T1 (Assoc1 Concrete1)...) (Trait2 T2)...)`: trait
    /// bounds on a generic `defun`'s type parameters, keyed by parameter
    /// name, each optionally pinning one or more of that trait's associated
    /// types to a concrete type — see `TraitBound`'s doc comment for how
    /// pins are used. Trait names are resolved as bare root-module paths
    /// (`Path::root`, not `Checker::resolve_trait_name`), mirroring
    /// `Checker::check_instance_method`'s existing assumption that every
    /// trait lives at the root module — a pre-existing limitation, not new
    /// here.
    fn parse_where_clause(&self, heap: &Heap, v: Value) -> Result<HashMap<String, Vec<TraitBound>>, Error> {
        let elems = heap.list_to_vec(v)?;
        let mut bounds: HashMap<String, Vec<TraitBound>> = HashMap::new();
        for clause in &elems[1..] {
            let parts = heap.list_to_vec(*clause)?;
            if parts.len() < 2 {
                return Err(Error::TypeError(
                    "where: each bound must be (Trait type-param (AssocName Type)...)".into(),
                ));
            }
            let trait_name = match parts[0] {
                Value::Symbol(id) => Path::root(heap.symbol_name(id)),
                _ => return Err(Error::TypeError("where: trait name must be a symbol".into())),
            };
            let tparam = match parts[1] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("where: type parameter must be a symbol".into())),
            };
            let mut assoc: HashMap<String, Type> = HashMap::new();
            for pin in &parts[2..] {
                let pin_parts = heap.list_to_vec(*pin)?;
                if pin_parts.len() != 2 {
                    return Err(Error::TypeError("where: associated-type pin must be (AssocName Type)".into()));
                }
                let aname = match pin_parts[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("where: associated type name must be a symbol".into())),
                };
                let aty = self.canon(&parse_type(heap, pin_parts[1])?);
                assoc.insert(aname, aty);
            }
            bounds.entry(tparam).or_default().push(TraitBound { trait_path: trait_name, assoc });
        }
        Ok(bounds)
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

    /// Parse a `((name type)...)` parameter list (types canonicalized to FQ)
    /// for `defun`/`lambda`/`labels`. These are always fixed-arity: `&rest` is
    /// rejected here (only `defmacro` has a `&rest` lambda-list marker — see
    /// `Self::check_defmacro`).
    fn parse_params(&self, heap: &Heap, v: Value) -> Result<Vec<(String, Type)>, Error> {
        let elems = heap.list_to_vec(v)?;
        if elems.iter().any(|p| matches!(p, Value::Symbol(id) if heap.symbol_name(*id) == "&rest")) {
            return Err(Error::TypeError(
                "&rest is not allowed in a defun/lambda parameter list (only defmacro is variadic)".into(),
            ));
        }
        self.parse_param_pairs(heap, &elems)
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

    /// Parse `defstruct` field bindings: `(name type)` or `(pub name type)`.
    /// Field visibility is independent of the struct's own `pub` — like every
    /// other `pub` in this language it's opt-in per item, never inherited
    /// from a container — so a field defaults to private (its getter/setter
    /// only reachable from the struct's own module) even on a `pub`
    /// `defstruct`, and `pub` on a field of a non-`pub` struct is legal (the
    /// field is reachable cross-module on any value of that type the current
    /// module's own `pub` API happens to hand out, even though outside code
    /// can't name or construct the type itself).
    fn parse_struct_fields(&self, heap: &Heap, pairs: &[Value]) -> Result<Vec<(String, Type, bool)>, Error> {
        let mut out = Vec::new();
        for binding in pairs {
            let elems = heap.list_to_vec(*binding)?;
            let (public, rest) = match elems.first() {
                Some(Value::Symbol(id)) if heap.symbol_name(*id) == "pub" => (true, &elems[1..]),
                _ => (false, &elems[..]),
            };
            if rest.len() != 2 {
                return Err(Error::TypeError("defstruct: field must be (name type) or (pub name type)".into()));
            }
            let name = match rest[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("defstruct: field name must be a symbol".into())),
            };
            out.push((name, self.canon(&parse_type(heap, rest[1])?), public));
        }
        Ok(out)
    }

    // ---- deftrait / impl ---------------------------------------------------

    /// `(deftrait Name (type AssocName)... (method-name ((self Self) params...) Ret)...)`:
    /// declares a trait as a set of method signature *templates* (no
    /// bodies) — `Self` and any declared associated type name are usable as
    /// ordinary type variables in a signature, exactly like a generic
    /// `defstruct`'s own type parameters (`Checker::parse_struct_fields`).
    /// Registers a [`TraitDef`]; `Checker::check_impl` later supplies bodies
    /// for some concrete implementing type. No type-checking happens here
    /// beyond parsing — a signature template's `Self`/associated-type
    /// variables aren't real types, so there's nothing to check yet.
    fn check_deftrait(&mut self, heap: &Heap, parts: &[Value], public: bool) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError(
                "deftrait: (deftrait Name (type AssocName)... (method (params...) ret)...)".into(),
            ));
        }
        let name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("deftrait: name must be a symbol".into())),
        };
        let mut assoc_types = Vec::new();
        let mut methods = HashMap::new();
        for item in &parts[1..] {
            let elems = heap.list_to_vec(*item)?;
            let head = match elems.first() {
                Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
                _ => return Err(Error::TypeError("deftrait: item must start with a symbol".into())),
            };
            if head == "type" {
                if elems.len() != 2 {
                    return Err(Error::TypeError("deftrait: (type AssocName)".into()));
                }
                let aname = match elems[1] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("deftrait: associated type name must be a symbol".into())),
                };
                assoc_types.push(aname);
                continue;
            }
            if elems.len() != 3 {
                return Err(Error::TypeError("deftrait: method signature must be (name (params...) ret)".into()));
            }
            let params = self.parse_param_pairs(heap, &heap.list_to_vec(elems[1])?)?;
            let ret = self.canon(&parse_type(heap, elems[2])?);
            let sig = FnSig {
                type_params: vec![],
                params: params.iter().map(|(_, t)| t.clone()).collect(),
                ret,
                public: true,
                builtin: false,
                bounds: HashMap::new(),
            };
            methods.insert(head, sig);
        }
        let fq_name = self.fq(&name);
        self.check_redef("trait", &name, self.cur_ns().traits.get(&name))?;
        self.reg
            .root
            .module_mut(&self.ns)
            .traits
            .insert(name, TraitDef { name: fq_name.clone(), assoc_types, methods, public, builtin: false });
        Ok(TopLevel::Module { path: fq_name, body: vec![] })
    }

    /// `(impl TraitName TargetType (type AssocName ConcreteType)... (method-name (recv params...) Ret body...)...)`:
    /// implements `TraitName` for `TargetType`. Each method is checked and
    /// registered exactly like an ordinary `(defmethod method-name ((self
    /// TargetType) params...) Ret body...)` — `Self` (and any of the
    /// trait's associated type names) occurring in a method's *written*
    /// parameter/return type syntax is textually substituted with
    /// `TargetType`/the `(type ...)` binding's concrete type *before*
    /// parsing, by rewriting the read `Value` tree (`Checker::subst_value`),
    /// so the rest of `check_defmethod` never has to know `Self`/associated
    /// types exist — by the time it parses the receiver/parameter/return
    /// types, they're already concrete. This is also why a method needs no
    /// separate "does this satisfy the trait's signature template" check:
    /// it's checked as a perfectly ordinary `defmethod`, just like any other.
    ///
    /// `TargetType`'s own `AdtDef.impls` gets `TraitName`'s path appended —
    /// the one piece of metadata `Checker::check_instance_method`'s
    /// type-variable branch needs later, since by then the method itself is
    /// just one more entry in `TargetType`'s ordinary `assoc` table.
    fn check_impl(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value], public: bool) -> Result<TopLevel, Error> {
        if parts.len() < 2 {
            return Err(Error::TypeError("impl: (impl TraitName TargetType (type AssocName Type)... (method ...)...)".into()));
        }
        let trait_name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("impl: trait name must be a symbol".into())),
        };
        let trait_fq = self.resolve_trait_name(&trait_name)?;
        let target_ty = self.canon(&parse_type(heap, parts[1])?);
        let target_fq = match &target_ty {
            Type::Named(n, _) => n.clone(),
            other => match prim_type_path(other) {
                Some(p) => p,
                None => return Err(Error::TypeError("impl: target must be a data type".into())),
            },
        };
        if self.reg.type_def(&target_fq).is_none() {
            return Err(Error::TypeError(format!("impl: unknown type `{}`", target_fq)));
        }

        // `Self` -> the target type's *written* form (`parts[1]`, unparsed —
        // substitution happens before `parse_type` runs on each method, so
        // a generic target like `(VectorIter T)` carries its own type
        // variable `T` through untouched). Each `(type AssocName Type)`
        // binding adds one more substitution, keyed by the trait's
        // associated type name.
        let mut subst: HashMap<String, Value> = HashMap::new();
        subst.insert("self".to_string(), parts[1]);
        // Parsed (not just textual) parallel of `subst`'s `(type AssocName
        // Type)` bindings, for `AdtDef::trait_assoc` — lets
        // `Checker::check_call`/`resolve_trait_assoc_type` later recover
        // what this `impl` concretely binds each associated type to, without
        // re-parsing `subst`'s raw `Value`s.
        let mut assoc_concrete: HashMap<String, Type> = HashMap::new();

        let mut method_forms: Vec<Value> = Vec::new();
        for item in &parts[2..] {
            let elems = heap.list_to_vec(*item)?;
            let head = match elems.first() {
                Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
                _ => return Err(Error::TypeError("impl: item must start with a symbol".into())),
            };
            if head == "type" {
                if elems.len() != 3 {
                    return Err(Error::TypeError("impl: (type AssocName Type)".into()));
                }
                let aname = match elems[1] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("impl: associated type name must be a symbol".into())),
                };
                assoc_concrete.insert(aname.clone(), self.canon(&parse_type(heap, elems[2])?));
                subst.insert(aname, elems[2]);
                continue;
            }
            method_forms.push(*item);
        }

        let mut body = Vec::new();
        for m in &method_forms {
            // `(method-name (recv-list) ret body...)`. Substitution applies
            // only to *type* positions — each receiver/parameter's type
            // (never its bound *name*, which would collide with `subst`'s
            // `"self"` key: the receiver is conventionally also named
            // `self`, a plain variable, completely unrelated to the `Self`
            // *type* keyword even though both case-fold to the same
            // string) — and the return type. The body is left untouched:
            // it's executable code, not type syntax, so any `Self`/`Item`
            // appearing there is an ordinary (if confusingly named)
            // variable/function reference, not something to substitute.
            let elems = heap.list_to_vec(*m)?;
            if elems.len() < 3 {
                return Err(Error::TypeError("impl: method must be (name (recv params...) ret body...)".into()));
            }
            let recv_pairs = heap.list_to_vec(elems[1])?;
            let mut new_recv_pairs = Vec::new();
            for pair in &recv_pairs {
                let p = heap.list_to_vec(*pair)?;
                if p.len() != 2 {
                    return Err(Error::TypeError("impl: receiver/parameter must be (name type)".into()));
                }
                let new_ty = Self::subst_value(heap, p[1], &subst)?;
                new_recv_pairs.push(self.list_from_vec(heap, &[p[0], new_ty])?);
            }
            let new_recv_list = self.list_from_vec(heap, &new_recv_pairs)?;
            let new_ret = Self::subst_value(heap, elems[2], &subst)?;
            let mut new_elems = vec![elems[0], new_recv_list, new_ret];
            new_elems.extend_from_slice(&elems[3..]);
            let tl = self.check_defmethod(heap, interp, &new_elems, public)?;
            body.push(tl);
        }
        if let Some(def) = self.reg.type_def_mut(&target_fq) {
            def.impls.push(trait_fq.clone());
            def.trait_assoc.insert(trait_fq, assoc_concrete);
        }
        Ok(TopLevel::Module { path: target_fq, body })
    }

    /// Resolve a `deftrait`-defined trait's bare name to its fully-qualified
    /// [`Path`] — mirrors `Checker::resolve_type_name`, but for the separate
    /// `traits` table (traits aren't types and don't share its namespace).
    fn resolve_trait_name(&self, name: &str) -> Result<Path, Error> {
        let mut ns = self.ns.clone();
        loop {
            if let Some(td) = self.reg.root.module(&ns).and_then(|m| m.traits.get(name)) {
                return Ok(td.name.clone());
            }
            if ns.is_empty() {
                break;
            }
            ns.pop();
        }
        Err(Error::TypeError(format!("impl: unknown trait `{}`", name)))
    }

    /// Build a proper list `Value` from `items`, in order — the inverse of
    /// `heap.list_to_vec`, used by `Checker::check_impl` to reassemble a
    /// receiver/parameter form after substituting just its type position.
    fn list_from_vec(&self, heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
        let mut out = Value::Empty;
        for item in items.iter().rev() {
            out = heap.cons(*item, out)?;
        }
        Ok(out)
    }

    /// Rewrite a read `Value` tree, replacing every bare symbol whose
    /// (case-folded) name is a key of `subst` with the corresponding
    /// replacement `Value` — `Checker::check_impl`'s `Self`/associated-type
    /// substitution. Leaves every other node (including non-symbol atoms and
    /// the list spine itself) alone; only used on already-read syntax, never
    /// on data, so there's no quoting concern.
    fn subst_value(heap: &mut Heap, v: Value, subst: &HashMap<String, Value>) -> Result<Value, Error> {
        match v {
            Value::Symbol(id) => {
                let name = heap.symbol_name(id).to_string();
                Ok(subst.get(&name).copied().unwrap_or(v))
            }
            Value::Cons(_) => {
                let car = heap.car(v)?;
                let cdr = heap.cdr(v)?;
                let new_car = Self::subst_value(heap, car, subst)?;
                let new_cdr = Self::subst_value(heap, cdr, subst)?;
                heap.cons(new_car, new_cdr)
            }
            other => Ok(other),
        }
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
        let MethodSig { method, instance, self_name, recv_ty, type_fq, params, ret, bounds, body_start } =
            self.parse_defmethod_sig(heap, parts)?;

        // A method on a *generic* type whose receiver spells the owner's
        // type parameters out as bare type variables (`(self Option<U>)`) is
        // generic through its owner — retain a re-checkable template (see
        // `MethodTemplate::Form`) and mark the erased `TopLevel` so
        // `Interp::exec` skips it, exactly like a generic `defun`. Any other
        // receiver shape (concrete arguments, no arguments on a static
        // method) declares a method whose signature can't mention the
        // owner's parameters, so it stays on the ordinary path.
        let written_vars: Vec<String> = match &recv_ty {
            Type::Named(_, targs) if !targs.is_empty() => targs
                .iter()
                .filter_map(|t| match t {
                    Type::Named(n, a) if a.is_empty() && n.is_simple() && self.reg.type_def(n).is_none() => {
                        Some(n.local().to_string())
                    }
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let owner_params = self.reg.type_def(&type_fq).map(|d| d.params.len()).unwrap_or(0);
        let is_generic_template = !written_vars.is_empty()
            && written_vars.len() == owner_params
            && matches!(&recv_ty, Type::Named(_, targs) if targs.len() == written_vars.len())
            && {
                let mut distinct = written_vars.clone();
                distinct.sort();
                distinct.dedup();
                distinct.len() == written_vars.len()
            };
        if is_generic_template {
            for &p in parts {
                heap.push_permanent_root(p);
            }
            self.generic_method_templates.insert(
                (type_fq.clone(), method.clone()),
                MethodTemplate::Form { parts: parts.to_vec(), ns: self.ns.clone(), written_vars: written_vars.clone() },
            );
        }

        // Register the signature before checking the body (self-recursion).
        let mut sig_params: Vec<Type> = Vec::new();
        if instance {
            sig_params.push(recv_ty.clone());
        }
        sig_params.extend(params.iter().map(|(_, t)| t.clone()));
        let sig = FnSig { type_params: vec![], params: sig_params, ret: ret.clone(), public, builtin: false, bounds: bounds.clone() };
        self.check_redef("method", &method, self.reg.type_def(&type_fq).and_then(|d| d.assoc.get(&method)))?;
        if let Some(def) = self.reg.type_def_mut(&type_fq) {
            def.assoc.insert(method.clone(), AssocFn { sig, instance, builtin: false });
        }

        let mut binds: Vec<(String, Type)> = Vec::new();
        if let Some(s) = &self_name {
            binds.push((s.clone(), recv_ty.clone()));
        }
        binds.extend(params.clone());
        // `with_bounds` mirrors `check_defun`'s body env: inside the body a
        // method call on a `where`-bounded type variable (e.g. `(equals
        // self::car other::car)` with `self::car : A` under `(where (Eq A))`)
        // resolves through `check_instance_method`'s bounds branch to a
        // diagnostics-only `Expr::TraitCall`, exactly as in generic `defun`
        // bodies.
        let env = Env::new().with_bounds(bounds).extended(binds);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], Some(&ret))?;
        let type_params = if is_generic_template { written_vars } else { Vec::new() };
        Ok(TopLevel::Defmethod { type_name: type_fq, method, instance, self_name, params, ret, body, type_params })
    }

    /// Parses a `defmethod` form's name, receiver, parameters, and return
    /// type (`parts` = everything after the `defmethod` keyword) — shared by
    /// [`Self::check_defmethod`] and [`Self::specialize_method`], the latter
    /// re-parsing a retained [`MethodTemplate::Form`] with
    /// `type_var_bindings` in effect so the receiver/parameter/return
    /// annotations come back concrete.
    fn parse_defmethod_sig(&self, heap: &mut Heap, parts: &[Value]) -> Result<MethodSig, Error> {
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
        // Optional `(where ...)` clause after the return type — identical
        // peek to `parse_defun_sig`'s.
        let mut body_start = 3;
        let mut bounds: HashMap<String, Vec<TraitBound>> = HashMap::new();
        if let Some(form) = parts.get(3) {
            if self.is_where_clause(heap, *form)? {
                bounds = self.parse_where_clause(heap, *form)?;
                body_start = 4;
            }
        }
        Ok(MethodSig { method, instance, self_name, recv_ty, type_fq, params, ret, bounds, body_start })
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
        let fields = self.parse_struct_fields(heap, &parts[1..])?;
        if fields.is_empty() {
            return Err(Error::TypeError("defstruct: needs at least one field".into()));
        }
        let field_names: Vec<String> = fields.iter().map(|(n, _, _)| n.clone()).collect();
        let field_types: Vec<Type> = fields.iter().map(|(_, t, _)| t.clone()).collect();
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
        for (i, (field_name, field_ty, field_public)) in fields.iter().enumerate() {
            let field_public = *field_public;
            let getter_sig = FnSig {
                type_params: vec![],
                params: vec![recv_ty.clone()],
                ret: field_ty.clone(),
                public: field_public,
                builtin: false,
                bounds: HashMap::new(),
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
                type_params: type_params.clone(),
            });

            // Setter (`set-car`/`set-cdr`'s `set-` prefix, no `!` — see
            // `Checker::check_setf`, which calls this via `(setf p::x v)`).
            let setter_name = format!("set-{}", field_name);
            let setter_sig = FnSig {
                type_params: vec![],
                params: vec![recv_ty.clone(), field_ty.clone()],
                ret: Type::Unit,
                public: field_public,
                builtin: false,
                bounds: HashMap::new(),
            };
            assoc.insert(setter_name.clone(), AssocFn { sig: setter_sig, instance: true, builtin: false });
            let self_var = Typed { expr: Expr::Var("self".to_string()), ty: recv_ty.clone() };
            let value_var = Typed { expr: Expr::Var("value".to_string()), ty: field_ty.clone() };
            accessors.push(TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: setter_name.clone(),
                instance: true,
                self_name: Some("self".to_string()),
                params: vec![("value".to_string(), field_ty.clone())],
                ret: Type::Unit,
                body: vec![Typed {
                    expr: Expr::FieldSet(Box::new(self_var), i, Box::new(value_var)),
                    ty: Type::Unit,
                }],
                type_params: type_params.clone(),
            });

            // A generic defstruct's accessors mention the type parameters
            // through the field types, so — like every generic-owner method —
            // the erased pair above is diagnostics-only (`type_params`
            // non-empty, skipped by `Interp::exec`) and each concrete
            // instantiation is generated on demand from these templates.
            // There is no raw source form to re-check (the accessor ASTs are
            // synthesized right here), hence the dedicated `Getter`/`Setter`
            // template kinds — see `Checker::synthesize_accessor`.
            if !type_params.is_empty() {
                self.generic_method_templates
                    .insert((type_fq.clone(), field_name.clone()), MethodTemplate::Getter { index: i });
                self.generic_method_templates
                    .insert((type_fq.clone(), setter_name), MethodTemplate::Setter { index: i });
            }
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
            impls: Vec::new(),
            trait_assoc: HashMap::new(),
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
            // `Sexpr::Float` is heap-boxed (`Value::Boxed`, see `BoxedObj`)
            // — today the only thing a `Value::Boxed` read-literal can be.
            Value::Boxed(id) => Typed { expr: Expr::Float(heap.float_value(id)), ty: float_lit_ty(expected) },
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
                } else if let Some(t) = self.fn_value(name, expected) {
                    t?
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
                } else if let Some(t) = self.fn_path_value(&segs, expected) {
                    t?
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
                // A `Symbol` is a valid `Sexpr` datum, so it is wrapped into
                // `Sexpr::Sym` wherever a `Sexpr` is expected (a `gensym`'d
                // temp flowing into a `list`/`cons`/quasiquote code position).
                // The runtime bits are identical, so `construct_sexpr`'s
                // `SEXPR_SYM` arm is an effective no-op.
                if *e == sexpr_ty() && typed.ty == Type::Symbol {
                    let ctor = sexpr_ctor_for(&Type::Symbol).expect("Symbol has a Sexpr encoding");
                    let (type_name, variant) =
                        self.resolve_ctor(ctor).expect("sexpr constructors are always registered");
                    return Ok(Typed {
                        expr: Expr::Construct { type_name, variant, args: vec![typed], mutable: false },
                        ty: sexpr_ty(),
                    });
                }
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
            "match" => return self.check_match(heap, interp, env, args, expected),
            "panic" => return self.check_panic(heap, interp, env, args),
            "the" => return self.check_the(heap, interp, env, args),
            "compile" => return self.check_compile(heap, args),
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
        let params = self.parse_params(heap, args[0])?;
        let ret = self.canon(&parse_type(heap, args[1])?);
        let fn_ty = Type::Fn(
            params.iter().map(|(_, t)| t.clone()).collect(),
            Box::new(ret.clone()),
        );
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
            let fn_ty = Type::Fn(params.iter().map(|(_, t)| t.clone()).collect(), Box::new(ret.clone()));
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

    /// Type-check applying a function *value* `callee` to `args`. Functions
    /// are fixed-arity, so `args` must match the callee's parameter count
    /// exactly (each checked against the corresponding parameter type).
    fn check_apply(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        callee: Typed,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let (params, ret) = match &callee.ty {
            Type::Fn(p, ret) => (p.clone(), (**ret).clone()),
            other => return Err(Error::TypeError(format!("value is not callable: {:?}", other))),
        };
        if args.len() != params.len() {
            return Err(Error::TypeError(format!(
                "function expects {} argument(s), got {}",
                params.len(),
                args.len()
            )));
        }
        let mut typed = Vec::new();
        for (arg, pty) in args.iter().zip(params.iter()) {
            typed.push(self.check(heap, interp, env, *arg, Some(pty))?);
        }
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
                    if self.assoc_visible(&type_fq, af) {
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
        match def.assoc.get(method) {
            Some(af) if af.instance && self.assoc_visible(&type_fq, af) => {}
            _ => return None,
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
            if let Some(type_fq) = &type_fq {
                if let Some(def) = self.reg.type_def(type_fq) {
                    let visible = def
                        .assoc
                        .get(method)
                        .is_some_and(|af| af.instance && self.assoc_visible(type_fq, af));
                    if visible {
                        return self.check_assoc_call(
                            heap,
                            interp,
                            env,
                            AssocCall { type_fq, method, receiver: Some(recv), expected: None },
                            &args[1..],
                        );
                    }
                }
                // No concrete `AdtDef` named `type_fq` — it may be one of
                // this function's own `where`-bounded type parameters
                // (`Env::bounds`, keyed by the lowercase type-variable name,
                // exactly what `type_fq.local()` is for a bare `Type::Named`
                // type variable like `t`). Search every trait it's bound to
                // for a matching method; see `Expr::TraitCall`'s doc comment
                // for why the implementing type is resolved at runtime
                // instead of here.
                if let Some(bound_traits) = env.bounds.get(type_fq.local()) {
                    for tb in bound_traits {
                        let Some(tdef) = self.reg.trait_def(&tb.trait_path) else { continue };
                        let Some(sig) = tdef.methods.get(method) else { continue };
                        let want = sig.params.len() - 1;
                        if args.len() - 1 != want {
                            return Err(Error::TypeError(format!(
                                "{}: expected {} argument(s), got {}",
                                method,
                                want,
                                args.len() - 1
                            )));
                        }
                        let mut typed_args = vec![recv];
                        for a in &args[1..] {
                            typed_args.push(self.check(heap, interp, env, *a, None)?);
                        }
                        // This bound's `where`-clause associated-type pins
                        // (e.g. `(Item i32)`) resolve the trait method
                        // template's otherwise-opaque associated-type
                        // variables (e.g. `Option<Item>`) wherever pinned —
                        // a no-op (returns `sig.ret` unchanged) when
                        // `tb.assoc` is empty, exactly matching pre-pin
                        // behavior.
                        let ret_ty = subst_apply(&sig.ret, &tb.assoc);
                        return Ok(Typed {
                            expr: Expr::TraitCall { method: method.to_string(), args: typed_args },
                            ty: ret_ty,
                        });
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
        // Method-side `where`-bound validation — the mirror of
        // `check_call`'s. A bounded method's bounds are keyed by the owner's
        // own type parameters (that's all a method signature can mention),
        // which `subst` above already resolved from the receiver — so e.g.
        // `(equals p q)` on a `cons-cell<point,i32>` whose `point` lacks an
        // `Eq` impl is rejected here with a real "does not implement trait"
        // error instead of an opaque `NoSuchFunction` from deep inside the
        // specialization drain.
        if !af.sig.bounds.is_empty() {
            self.validate_where_bounds(&format!("{}::{}", type_fq, method), &af.sig.bounds, &subst)?;
        }
        // Monomorphization, method side: a call on a generic owner whose
        // type arguments are fully concrete is rewritten to the specialized
        // method (generated by `check_form`'s drain) — the mirror of
        // `check_call`'s free-function rewrite. Only methods with a retained
        // template qualify: builtin generic methods (`Vector<T>::get`, ...)
        // are Rust implementations dispatched by their plain name, and stay
        // untouched.
        let mut method_name = method.to_string();
        if !def.params.is_empty()
            && self.generic_method_templates.contains_key(&(type_fq.clone(), method.to_string()))
        {
            let targs: Vec<Type> = def.params.iter().map(|p| subst[p.as_str()].clone()).collect();
            if !targs.iter().any(|t| self.type_is_open(t)) {
                method_name = self.request_method_specialization(type_fq, method, targs);
            }
        }
        Ok(Typed {
            expr: Expr::Assoc {
                type_name: type_fq.clone(),
                method: method_name,
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

    /// `(the Type expr)`: a type annotation, e.g. `(the i64 5)` to make an
    /// integer literal default to `i64` instead of `i32`, or to pin down a
    /// generic call's type argument the way an `expected` type elsewhere
    /// would. Purely a checking-time hint with no runtime behavior of its
    /// own — `Type` simply becomes `expr`'s `expected` (the same role it
    /// plays for a `defun` parameter or `let` binding annotation), and the
    /// returned `Typed` is exactly `expr`'s own (no new `Expr` variant; `the`
    /// vanishes after checking).
    fn check_the(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("the: (the Type expr)".into()));
        }
        let ty = self.canon(&parse_type(heap, args[0])?);
        self.check(heap, interp, env, args[1], Some(&ty))
    }

    /// `(compile name)` / `(compile type::method)`: the name being compiled
    /// is program structure, not runtime data, so — unlike an ordinary call —
    /// its argument is special-cased here to read as an unevaluated symbol or
    /// `::`-path rather than a checked expression. A string (`(compile
    /// "name")`) is a type error: it would let the same name be spelled two
    /// incompatible ways for no benefit. Converts the symbol/path straight to
    /// the plain `&str` `Interp::eval_builtin`'s `"compile"` arm and
    /// `Interp::method_key` already expect (`"name"` or `"type::method"`),
    /// then re-wraps it as an ordinary call to the registered `compile`
    /// builtin (`Registry::with_builtins`, still `Type::Str` -> `Type::Bool`)
    /// so every other part of the pipeline is untouched.
    fn check_compile(&self, heap: &Heap, args: &[Value]) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("compile: (compile name) — expected exactly 1 argument".into()));
        }
        let name = match args[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            Value::Path(pid) => heap
                .path_segments(pid)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect::<Vec<_>>()
                .join("::"),
            _ => {
                return Err(Error::TypeError(
                    "compile: expected a symbol or path naming a function, e.g. (compile foo) or (compile point::x) — not a string".into(),
                ))
            }
        };
        // A generic target has no erased runtime body to compile — post-
        // monomorphization only concrete specializations exist, generated
        // per call site. Rejected here with a real explanation instead of
        // the bare runtime `NoSuchFunction` the missing registration would
        // otherwise produce.
        let is_generic = match name.rsplit_once("::") {
            None => self
                .resolve_fn(&name)
                .map(|fq| self.generic_fn_templates.contains_key(&fq))
                .unwrap_or(false),
            Some((type_part, method)) => {
                let segs: Vec<String> = type_part.split("::").map(|s| s.to_string()).collect();
                let type_fq = self.resolve_type_name(&Path::from_segments(segs));
                self.generic_method_templates.contains_key(&(type_fq, method.to_string()))
            }
        };
        if is_generic {
            return Err(Error::TypeError(format!(
                "compile: `{}` is generic — a generic function has no single compiled body; \
                 call it at concrete types and compile those uses' enclosing functions instead",
                name
            )));
        }
        let sig = self.reg.fn_sig(&Path::root("compile")).expect("compile is always registered");
        Ok(Typed {
            expr: Expr::Call(Path::root("compile"), vec![Typed { expr: Expr::Str(name), ty: Type::Str }]),
            ty: sig.ret.clone(),
        })
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
    /// exception, desugaring to a call to the prelude's `sexpr-append` (see
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
    /// to the prelude's `sexpr-append` (`Checker::resolve_fn`, the same lookup
    /// `check_call`'s caller uses), joining `x` with the recursively
    /// desugared rest of the list. This means `,@` requires the prelude to
    /// be loaded (`sexpr-append` registered) — acceptable since the macros that
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
                        // `check`'s reconciliation wraps a `Symbol` (e.g. a
                        // `gensym`'d temp) into `Sexpr::Sym` here automatically;
                        // any other non-`Sexpr` value is still rejected.
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
                            let append_fq = self.resolve_fn("sexpr-append").ok_or_else(|| {
                                Error::TypeError(
                                    "unquote-splicing (,@) requires the prelude's `sexpr-append` to be loaded"
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
            let (adt, cons_idx) = self.sexpr_cons_ctor();
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
            .is_some_and(|af| af.instance && self.assoc_visible(type_fq, af));
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

    /// `(defvar (name Type) value)` / `(defconstant (name Type) value)`.
    /// Registers a global in the current namespace. The declared type is
    /// *mandatory* — a global's type is part of the program's public
    /// surface, so it is never inferred from the initializer (unlike a
    /// `let` binding, whose whole scope is in view; the untyped
    /// `(defvar name value)` form was removed 2026-07-03). The annotation
    /// also gives the initializer its expected type, which
    /// post-monomorphization is what lets e.g. a generic function value
    /// (`(defvar (f (fn (i32) i32)) identity)`) resolve its type arguments
    /// at all.
    fn check_defvar(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        mutable: bool,
        public: bool,
    ) -> Result<TopLevel, Error> {
        if parts.len() != 2 {
            return Err(Error::TypeError("defvar/defconstant: (defvar (name Type) value)".into()));
        }
        let (name, ann) = match parts[0] {
            Value::Cons(_) => {
                let pair = heap.list_to_vec(parts[0])?;
                if pair.len() != 2 {
                    return Err(Error::TypeError("defvar: name must be (name Type)".into()));
                }
                let name = match pair[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("defvar: name must be a symbol".into())),
                };
                (name, self.canon(&parse_type(heap, pair[1])?))
            }
            Value::Symbol(id) => {
                return Err(Error::TypeError(format!(
                    "defvar: a global needs a declared type — write (defvar ({} Type) value)",
                    heap.symbol_name(id)
                )))
            }
            _ => return Err(Error::TypeError("defvar: name must be (name Type)".into())),
        };
        // The value is checked at the top level (no locals), but globals/fns are
        // visible via the registry.
        let value = self.check(heap, interp, &Env::new(), parts[1], Some(&ann))?;
        let ty = ann;
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
        let (adt, cons_idx) = self.sexpr_cons_ctor();
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
        if args.len() != fixed {
            return Err(Error::TypeError(format!(
                "{}: expected {} argument(s), got {}",
                name,
                fixed,
                args.len()
            )));
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
        for (arg, pty) in args.iter().zip(sig.params.iter()) {
            let st = subst_apply(pty, &subst);
            let exp = if type_has_param(&st, &params) { None } else { Some(st) };
            let ta = self.check(heap, interp, env, *arg, exp.as_ref())?;
            unify(&params, pty, &ta.ty, &mut subst)?;
            typed.push(ta);
        }
        // Associated-type pins participate in *inference*, not just
        // verification: when a `where` bound pins an associated type to one of
        // `name`'s own type parameters (e.g. `reverse`'s `(where (Iter I (Item
        // A)))`, where `A` appears in no ordinary argument and so is otherwise
        // uninferrable), resolve the concrete type's real binding for that
        // associated type and unify it into `subst`. Runs before the
        // "cannot infer" check below so the pinned variable counts as
        // resolved; the strict validation loop further down then re-checks the
        // (now-inferred) pin for consistency. A `unify` failure here is
        // ignored — the validation loop reports pin mismatches with a precise
        // message.
        for (tparam, trait_bounds) in &sig.bounds {
            let Some(concrete) = subst.get(tparam).cloned() else { continue };
            let type_fq = match &concrete {
                Type::Named(n, _) if n.is_simple() && self.reg.type_def(n).is_none() => None,
                Type::Named(n, _) => Some(n.clone()),
                other => prim_type_path(other),
            };
            let Some(type_fq) = type_fq else { continue };
            let Some(def) = self.reg.type_def(&type_fq) else { continue };
            let concrete_args: &[Type] = match &concrete {
                Type::Named(_, args) => args.as_slice(),
                _ => &[],
            };
            for tb in trait_bounds {
                for (assoc_name, declared_ty) in &tb.assoc {
                    if let Some(actual) =
                        resolve_trait_assoc_type(def, &tb.trait_path, assoc_name, concrete_args)
                    {
                        let _ = unify(&params, declared_ty, &actual, &mut subst);
                    }
                }
            }
        }
        for p in &sig.type_params {
            if !subst.contains_key(p) {
                return Err(Error::TypeError(format!(
                    "cannot infer type parameter `{}` for `{}`",
                    p, name
                )));
            }
        }
        // Call-site `where`-bound validation: for each of `name`'s own type
        // parameters that carries trait bounds, check the concrete type
        // `subst` resolved it to actually implements every required trait
        // (and, if the bound pins an associated type, that the concrete
        // type's *own* binding for that associated type matches what the
        // `where` clause declared). Skipped when the resolved type is
        // itself still a *bare* unresolved type variable (this call is
        // nested inside another generic function and the type only becomes
        // concrete further up the call chain) — propagating the *caller's
        // own* bounds through to verify an equivalent bound is already
        // declared on the outer function is real but currently unexercised
        // by any code in this repo (no `where`-bounded function forwards its
        // own type parameter into another `where`-bounded call), so it's
        // left to the existing runtime `Expr::TraitCall` fallback, same as
        // before this validation existed. Note the skip only catches a bare
        // type variable (`T` itself) — a type variable *wrapped* in a
        // concrete type (e.g. `Vector<U>` for an outer `U`) is still
        // validated strictly below, which is correct for plain
        // trait-membership checks (`Vector<U>` never implements `Iter`
        // regardless of `U`) but could in principle reject an associated-type
        // pin that would actually hold once `U` resolves further up the call
        // chain — also unexercised today.
        self.validate_where_bounds(&name.to_string(), &sig.bounds, &subst)?;
        // Monomorphization: a call that instantiates a generic function at
        // fully concrete types is rewritten to reference the specialized
        // definition (generated by `check_form`'s drain). If any type
        // argument is still open we are inside another generic function's
        // diagnostic body-check — keep the original (never-executed) call;
        // the enclosing function's own specialization will re-check this
        // very call with the types concrete. Builtin generic free functions
        // (no template) keep their runtime-dispatched call as-is.
        let mut call_path = name.clone();
        if !sig.type_params.is_empty() && self.generic_fn_templates.contains_key(name) {
            let targs: Vec<Type> = sig.type_params.iter().map(|p| subst[p.as_str()].clone()).collect();
            if !targs.iter().any(|t| self.type_is_open(t)) {
                call_path = self.request_fn_specialization(name, targs);
            }
        }
        Ok(Typed { expr: Expr::Call(call_path, typed), ty: subst_apply(&sig.ret, &subst) })
    }

    /// Call-site `where`-bound validation shared by [`Self::check_call`]
    /// (free functions) and [`Self::check_assoc_call`] (methods): for each
    /// bounded type parameter, check the concrete type `subst` resolved it
    /// to actually implements every required trait (and that any pinned
    /// associated type matches). A parameter whose resolved type is still a
    /// bare unresolved type variable is skipped — see the caller-bound
    /// propagation discussion at [`Self::check_call`]'s call site. `name` is
    /// only for error messages (`fn-name` / `type::method`).
    fn validate_where_bounds(
        &self,
        name: &str,
        bounds: &HashMap<String, Vec<TraitBound>>,
        subst: &HashMap<String, Type>,
    ) -> Result<(), Error> {
        for (tparam, trait_bounds) in bounds {
            let Some(concrete) = subst.get(tparam) else { continue };
            let type_fq = match concrete {
                Type::Named(n, _) if n.is_simple() && self.reg.type_def(n).is_none() => None,
                Type::Named(n, _) => Some(n.clone()),
                other => prim_type_path(other),
            };
            let Some(type_fq) = type_fq else { continue };
            let def = self.reg.type_def(&type_fq).ok_or_else(|| {
                Error::TypeError(format!(
                    "{}: type {:?} does not implement a trait required by `where` clause on type parameter `{}`",
                    name, concrete, tparam
                ))
            })?;
            for tb in trait_bounds {
                if !def.impls.contains(&tb.trait_path) {
                    return Err(Error::TypeError(format!(
                        "{}: type {:?} does not implement trait {:?} required by `where` clause on type parameter `{}`",
                        name, concrete, tb.trait_path, tparam
                    )));
                }
                let concrete_args: &[Type] = match concrete {
                    Type::Named(_, args) => args.as_slice(),
                    _ => &[],
                };
                for (assoc_name, declared_ty) in &tb.assoc {
                    // The pin may itself be one of `name`'s own type parameters
                    // (e.g. `(Item A)` on a generic `map`), inferred from the
                    // other arguments into `subst` above — resolve it before
                    // comparing, so a *polymorphic* pin is verified against the
                    // concrete type `A` actually unified to, not the bare
                    // variable. A concrete pin (`(Item i32)`) is unaffected
                    // (`subst_apply` is a no-op on a type with no variables).
                    let declared_ty = subst_apply(declared_ty, subst);
                    let actual = resolve_trait_assoc_type(def, &tb.trait_path, assoc_name, concrete_args);
                    match actual {
                        Some(actual) if actual == declared_ty => {}
                        actual => {
                            return Err(Error::TypeError(format!(
                                "{}: type {:?}'s associated type `{}` is {:?}, but `where` clause requires {:?}",
                                name, concrete, assoc_name, actual, declared_ty
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
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
        // `match` is fenced to enum (sum-type) dispatch — `Option`/`Result`/
        // `error`/user `defstruct`s. `Sexpr` is the internal island
        // representation (read/eval/print/`defmacro`/self-hosting `compiler.rs`),
        // no longer a user-matchable datum: its structure is navigated with the
        // `sexpr-*` accessor layer instead (Symbol/Sexpr redesign Phase 5,
        // `docs/dev/symbol-sexpr-redesign.md`). The island already uses
        // `sexpr-consp`/`sexpr-car`/... rather than `match`, so this fence has
        // no effect on it — it only rejects a user (or leftover prelude)
        // `(match sexpr-value ...)`.
        if adt_name == Path::root("sexpr") {
            return Err(Error::TypeError(
                "match on a Sexpr value is not supported: Sexpr is an internal type; navigate it with the sexpr-* accessors (sexpr-consp/sexpr-car/sexpr-cdr/...) instead"
                    .into(),
            ));
        }
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
                Pattern::Wildcard | Pattern::Bind(..) => catchall = true,
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
                    Ok((
                        Pattern::Bind(name.to_string(), self.is_heap_repr(expected)),
                        vec![(name.to_string(), expected.clone())],
                    ))
                }
            }
            Value::Int(n) => {
                if expected.is_integer() {
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
        let mut sexpr_fields = Vec::new();
        for (sub, field) in parts[1..].iter().zip(fields.iter()) {
            let field_ty = subst_apply(field, &subst);
            // Baked into the pattern here (where the instantiated field
            // type is in hand) so the type-erased interpreter can decode a
            // boxed struct's `Sexpr`-declared field faithfully — see
            // `Pattern::Ctor::sexpr_fields`'s doc comment.
            sexpr_fields.push(matches!(&field_ty, Type::Named(p, _) if *p == Path::root("sexpr")));
            let (p, b) = self.check_pattern(heap, &field_ty, *sub)?;
            sub_pats.push(p);
            binds.extend(b);
        }
        Ok((Pattern::Ctor { type_name: adt_name, variant, args: sub_pats, sexpr_fields }, binds))
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
        // `Sexpr::Float` is heap-boxed (`Value::Boxed`, see `BoxedObj`).
        Value::Boxed(id) => QuotedSexpr::Float(heap.float_value(id)),
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

/// The built-in `Sexpr` type (`read`/`eval`/`print`/`defmacro` island).
fn sexpr_ty() -> Type {
    Type::Named(Path::root("sexpr"), vec![])
}

/// The `Sexpr` constructor name that exactly represents a value of `elem_ty`
/// (`Sexpr` itself needs no wrapping). Used to coerce a `Symbol` into its
/// `Sexpr::Sym` datum where a `Sexpr` is expected (see `Checker::check`).
fn sexpr_ctor_for(elem_ty: &Type) -> Option<&'static str> {
    match elem_ty {
        Type::I64 | Type::I32 => Some("int"),
        Type::F64 => Some("float"),
        Type::Char => Some("char"),
        Type::Bool => Some("bool"),
        Type::Str => Some("str"),
        Type::Symbol => Some("sym"),
        _ => None,
    }
}

/// The type of an integer literal: the expected integer type if any, else `i32`.
fn int_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t) if t.is_integer() => t.clone(),
        _ => Type::I32,
    }
}

/// The type of a float literal: the expected float type if any, else `f64`.
fn float_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t) if t.is_float() => t.clone(),
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
        Type::Fn(ps, r) => {
            ps.iter().any(|p| type_has_param(p, params)) || type_has_param(r, params)
        }
        _ => false,
    }
}

/// Renders a canonicalized (fully module-qualified) type as the string used
/// inside a specialization's mangled name — a faithful, unambiguous
/// serialization, so two distinct canonical types can never render equal
/// (which is what lets `mangled_fn_path` double as the instantiation's
/// identity). Nested generics render Rust-style: `vector<cons-cell<string,i32>>`.
fn mangle_type(t: &Type) -> String {
    match t {
        Type::I8 => "i8".into(),
        Type::I16 => "i16".into(),
        Type::I32 => "i32".into(),
        Type::I64 => "i64".into(),
        Type::Isize => "isize".into(),
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::Usize => "usize".into(),
        Type::F32 => "f32".into(),
        Type::F64 => "f64".into(),
        Type::Bool => "bool".into(),
        Type::Char => "char".into(),
        Type::Str => "string".into(),
        Type::Symbol => "symbol".into(),
        Type::Unit => "()".into(),
        Type::Never => "!".into(),
        Type::Named(p, args) if args.is_empty() => p.to_string(),
        Type::Named(p, args) => {
            format!("{}<{}>", p, args.iter().map(mangle_type).collect::<Vec<_>>().join(","))
        }
        Type::Fn(ps, r) => {
            let inner: Vec<String> = ps.iter().map(mangle_type).collect();
            format!("(fn ({}) {})", inner.join(","), mangle_type(r))
        }
    }
}

/// The specialized function's [`Path`] for `base` instantiated at `args`:
/// the base path with its final segment rewritten to e.g. `"identity <i32>"`.
/// The space is load-bearing: the reader treats whitespace as a delimiter,
/// so no source-written symbol can ever spell this name — a user `(defun
/// identity<i32> ...)` (a perfectly legal token) can therefore never collide
/// with a generated specialization in `Interp`'s function table.
fn mangled_fn_path(base: &Path, args: &[Type]) -> Path {
    let mut segs = base.parent().to_vec();
    segs.push(mangled_method_name(base.local(), args));
    Path::from_segments(segs)
}

/// The mangled *local* name shared by function and method specializations —
/// e.g. `"unwrap <i32>"`. See [`mangled_fn_path`]'s doc comment for why the
/// space makes collisions with source-written names impossible.
fn mangled_method_name(base: &str, args: &[Type]) -> String {
    format!("{} <{}>", base, args.iter().map(mangle_type).collect::<Vec<_>>().join(","))
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
        Type::Fn(ps, r) => Type::Fn(
            ps.iter().map(|p| subst_apply(p, subst)).collect(),
            Box::new(subst_apply(r, subst)),
        ),
        other => other.clone(),
    }
}

/// Resolve what a concrete instantiation of `def` (e.g. `vector-iter<i32>`,
/// `concrete_args = [i32]`) actually binds trait `trait_path`'s associated
/// type `assoc_name` to — reading `AdtDef::trait_assoc`'s raw (possibly
/// still `def.params`-generic) binding and substituting `concrete_args` in,
/// the same `params`-zip `Checker::check_assoc_call` already does for method
/// signatures. `None` if `def` has no recorded binding for that trait/name
/// pair.
fn resolve_trait_assoc_type(def: &AdtDef, trait_path: &Path, assoc_name: &str, concrete_args: &[Type]) -> Option<Type> {
    let raw = def.trait_assoc.get(trait_path)?.get(assoc_name)?;
    if concrete_args.len() == def.params.len() {
        let subst: HashMap<String, Type> = def.params.iter().cloned().zip(concrete_args.iter().cloned()).collect();
        Some(subst_apply(raw, &subst))
    } else {
        Some(raw.clone())
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
        (Type::Fn(p1, r1), Type::Fn(p2, r2)) if p1.len() == p2.len() => {
            for (t, a) in p1.iter().zip(p2.iter()) {
                unify(params, t, a, subst)?;
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
