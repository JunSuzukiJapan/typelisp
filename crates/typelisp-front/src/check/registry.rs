//! Registries of data types (ADTs) and function signatures used by the checker.
//!
//! Type and variant names are stored lowercase because the reader case-folds all
//! symbols. The built-in types `Option<T>` and `Sexpr` are pre-registered.

use std::collections::{BTreeMap, HashMap};

use crate::{Loc, Path, Type};

/// One constructor of a data type: a name and its field types. Field types may
/// reference the enclosing type's parameters as `Type::Named(param, [])`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Type>,
}

/// One `where`-clause bound: a trait a type parameter must implement, plus
/// (optionally) concrete pins for that trait's associated types — e.g.
/// `(Iter T (Item i32))` pins `T`'s `Item` to `i32`. `assoc` is empty for a
/// bound with no pins (`(Iter T)`), which behaves exactly as before pins
/// existed — see `Checker::parse_where_clause`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TraitBound {
    pub trait_path: Path,
    /// Associated-type name (lowercase) -> the concrete `Type` this bound
    /// pins it to. Only entries explicitly written in the `where` clause are
    /// present; an un-pinned associated type stays absent (looked up, not
    /// substituted) wherever it's consulted.
    pub assoc: BTreeMap<String, Type>,
}

/// A function's parameter and return types.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FnSig {
    /// Type parameter names declared by `(defun (name T1 T2...) ...)`. Empty
    /// for an ordinary (non-generic) function — callers resolve these against
    /// the actual argument/expected types the same way `check_construct`
    /// resolves an ADT's `params` (see `Checker::check_call`).
    pub type_params: Vec<String>,
    pub params: Vec<Type>,
    pub ret: Type,
    /// Visible outside its defining module.
    pub public: bool,
    /// The element type of a trailing `&rest` parameter, if this function is
    /// variadic (`None` for the overwhelming majority of functions). Every
    /// call-site argument past `params.len()` must have this type, and is
    /// collected into a single `Sexpr` list actual argument (an ordinary
    /// Lisp list, like every Lisp's `&rest` parameter) — see
    /// `Checker::check_call`.
    pub rest: Option<Type>,
    /// Registered by `Registry::with_builtins` (Rust-implemented), as opposed
    /// to a user (or prelude) `defun` — see [`Definable`]/`Checker::check_redef`.
    /// Redefining a builtin is always an error; redefining anything else
    /// follows the configurable [`crate::check::checker::RedefPolicy`].
    pub builtin: bool,
    /// Trait bounds declared by this function's own `(where (Trait T
    /// (Assoc Concrete)...)...)` clause (`Checker::check_defun`), keyed by
    /// type-parameter name — empty for a non-generic function or a generic
    /// one with no `where` clause. Stored on the registered signature (not
    /// just the body-checking `Env`) so `Checker::check_call` can validate a
    /// call site's actual argument types against it.
    pub bounds: BTreeMap<String, Vec<TraitBound>>,
    /// `&optional` parameters (`defun` only — never populated by `lambda`
    /// or a builtin), in declared order, past `params`/`rest`. Empty for the
    /// overwhelming majority of functions. Mutually exclusive with `keys`
    /// being non-empty (`Checker::parse_defun_params_full` rejects a
    /// parameter list combining `&optional`/`&rest` with `&key` — CL's own
    /// ambiguity between a positionally-filled `&optional` and a
    /// label-matched `&key` argument when both sections are present). See
    /// [`OptKeyParam`] and `Checker::check_call_opt_key`.
    pub optionals: Vec<OptKeyParam>,
    /// `&key` parameters (`defun` only), matched at the call site by the
    /// keyword symbol `:name` — order doesn't matter there, but is
    /// preserved here as declared for the runtime parameter list
    /// (`TopLevel::Defun::params`) callers must fill positionally once
    /// resolved. Empty for the overwhelming majority of functions; mutually
    /// exclusive with `optionals`/`rest` — see that field's doc comment.
    pub keys: Vec<OptKeyParam>,
}

/// One `&optional`/`&key` parameter of a `defun`, resolved at check time —
/// see `Checker::check_defun_opt_key`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OptKeyParam {
    pub name: String,
    /// The declared type as written — always the plain (non-`Option`-
    /// wrapped) type, even when `default` is absent. See
    /// [`Self::effective_ty`] for the type actually bound in the callee's
    /// body and expected of an omitted call-site argument.
    pub decl_ty: Type,
    /// The checked default-value expression, when the parameter declared
    /// one (`(name type default-form)`) — checked once, at `defun`-check
    /// time, in an environment with *no* other parameters bound (a default
    /// may not yet reference an earlier parameter — a real, documented
    /// restriction, not every CL implementation's full generality). Spliced
    /// into the call site's argument list by `Checker::check_call_opt_key`
    /// whenever the call omits this argument.
    /// `None` means "no default": the parameter's *effective* type (both in
    /// the callee's body and for a call site that omits it) is
    /// `Option<decl_ty>` instead — a caller who does supply this argument
    /// still writes a plain `decl_ty`-typed value, auto-wrapped into `Some`
    /// (never a literal `Option::some` call) by the same call-site logic.
    ///
    /// Held as an [`OwnedForm`](crate::owned_form::OwnedForm), not as the lowered
    /// core form itself: a `Value` is an index into *a* heap, and this
    /// signature outlives the checker's per-form root truncation and is
    /// serialized into a fasl. Rebuilding it per call site with
    /// `owned_form::owned_to_value` is also the right splice semantics — each site
    /// needs its own cells, not a shared subtree. `TraitDefault` makes the
    /// same call for the same reason. The source position survives the round
    /// trip because `OwnedForm::Cons` carries both of a cell's location slots.
    pub default: Option<crate::owned_form::OwnedForm>,
}

impl OptKeyParam {
    /// The type this parameter is bound at in the callee's body, and that a
    /// call-site argument (when supplied and there's no default to compare
    /// against instead) is ultimately packaged as — see [`Self::default`]'s
    /// doc comment.
    pub fn effective_ty(&self) -> Type {
        opt_key_effective_ty(&self.decl_ty, self.default.is_some())
    }
}

/// [`OptKeyParam::effective_ty`] before the default has been checked into an
/// [`OwnedForm`](crate::owned_form::OwnedForm) — all it ever depended on was
/// *whether* there is one. `Checker::parse_defmethod_sig_inner` needs the
/// effective types while still holding the defaults as raw source forms.
pub fn opt_key_effective_ty(decl_ty: &Type, has_default: bool) -> Type {
    if has_default {
        decl_ty.clone()
    } else {
        Type::Named(Path::root("option"), vec![decl_ty.clone()])
    }
}

/// A type-associated function or method (Rust-style; types are *not*
/// namespaces). `instance` is true when the first parameter is the receiver.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AssocFn {
    pub sig: FnSig,
    pub instance: bool,
    /// See [`FnSig::builtin`].
    pub builtin: bool,
}

/// A `defmacro`'s signature: an arity and whether it's variadic (every
/// parameter and the implicit return are always `Sexpr`, so there is no
/// per-parameter type to record — see `Checker::check_defmacro`).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MacroDef {
    /// The number of leading required parameters — the call's minimum argument
    /// count. (`&optional`/`&rest`/`&key` params past this prefix are all
    /// omittable; see [`crate::MacroLambda`].)
    pub required: usize,
    /// The number of `&optional` parameters (each omittable, filled
    /// positionally after the required ones). Together with `required` this
    /// bounds a plain call's arg count (`required..=required+optional`) when
    /// there is neither `&rest` nor `&key`.
    pub optional: usize,
    /// Whether the parameter list ends in `&rest name` — the trailing
    /// positional arguments (past `required + optional`) are collected into a
    /// single `Sexpr` list bound to that parameter.
    pub rest: bool,
    /// The `&key` parameter names (bare, no leading colon), empty when the
    /// lambda list has no `&key` section. A call supplies one as `:name value`
    /// in its trailing arguments.
    pub keys: Vec<String>,
    /// Visible outside its defining module.
    pub public: bool,
    /// See [`FnSig::builtin`]. Always `false` today — `with_builtins`
    /// registers no macros — kept for uniformity with the other four
    /// `Definable` impls so `Checker::check_redef` has one shape to call.
    pub builtin: bool,
}

/// A global variable/constant: its type and whether it is assignable.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VarInfo {
    pub ty: Type,
    pub mutable: bool,
    /// Visible outside its defining module.
    pub public: bool,
    /// See [`FnSig::builtin`]. Always `false` today — `with_builtins`
    /// registers no global variables.
    pub builtin: bool,
}

/// Distinguishes a multi-variant sum type (`Option`/`Result`/`Sexpr`/a future
/// user ADT) from a `defstruct` product type. The two are otherwise the same
/// `AdtDef`/`Variant` machinery (a struct registers exactly one variant,
/// named `"new"` — see `Checker::check_defstruct`), but only a `Struct`
/// constructs a *mutable* boxed struct (`BoxedObj::Struct`) rather than an
/// immutable enum box (`Checker::check_construct` decides which by this field) — see [`AdtDef::field_names`] for the other `Struct`-only
/// piece of metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AdtKind {
    Sum,
    Struct,
}

/// A built-in data-type definition (a sum type, e.g. `Option`/`Result`/
/// `Sexpr`/`HashTable`). `name` is the fully-qualified (module-prefixed) type
/// [`Path`] used as the type's identity.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AdtDef {
    pub name: Path,
    /// Type-parameter names (lowercase), e.g. `["t"]` for `Option<T>`.
    pub params: Vec<String>,
    pub variants: Vec<Variant>,
    /// Associated functions / methods, keyed by (unqualified) name.
    pub assoc: BTreeMap<String, AssocFn>,
    /// Visible outside its defining module.
    pub public: bool,
    /// See [`FnSig::builtin`].
    pub builtin: bool,
    /// See [`AdtKind`]. `Sum` for every built-in ADT; only `defstruct`
    /// produces `Struct`.
    pub kind: AdtKind,
    /// Field names, in declaration order — empty for every `Sum`-kind type
    /// (`Variant::fields` there is positional only, e.g. `Some(T)`'s field
    /// has no name). Non-empty only for a `Struct`, where it backs field
    /// accessor/setter generation (`Checker::check_defstruct`) — the
    /// `Variant`'s own `fields: Vec<Type>` still holds the parallel types.
    pub field_names: Vec<String>,
    /// Traits this type has an `impl` for (`Checker::check_impl`), by the
    /// trait's [`Path`]. Each `impl`ed method is inserted into `assoc` like
    /// an ordinary `defmethod` — so an instance call on a *concrete* receiver
    /// type resolves through the same `assoc` lookup `check_instance_method`
    /// already does, untouched. Also consulted by `Checker::check_call` to
    /// validate a where-bounded call site's actual argument types against
    /// the callee's declared bounds.
    pub impls: Vec<Path>,
    /// Per-implemented-trait associated-type bindings, parsed
    /// (`Checker::check_impl`): trait `Path` -> {associated-type name -> the
    /// `Type` this `impl` binds it to}. For a generic `impl` (e.g. `(impl
    /// Iter (vector-iter T) (type Item T) ...)`), the stored `Type` may
    /// itself still mention this ADT's own `params` (here, `T`) rather than
    /// being fully concrete — see `resolve_trait_assoc_type`, which
    /// substitutes a *specific* instantiation's `params` (e.g.
    /// `vector-iter<i32>`) in before use.
    pub trait_assoc: BTreeMap<Path, BTreeMap<String, Type>>,
}

/// A `deftrait` definition (`Checker::check_deftrait`): a trait name, its
/// associated type names, and method signature *templates* — mentioning
/// `Self` and the trait's own associated types (e.g. `Item`) as type
/// variables, the same way an [`AdtDef`]'s `variants` field types mention
/// `AdtDef::params`. Carries no method bodies: those live on the
/// *implementing* type's own [`AdtDef::assoc`], inserted by `Checker::check_impl`
/// as ordinary `defmethod`s with `Self`/the associated types already
/// substituted for the `impl`'s concrete target type — see that function's
/// doc comment. A `TraitDef` itself is consulted only when a method is
/// called on a still-generic type-variable receiver (no concrete `AdtDef` to
/// look the method up on yet) — `Checker::check_instance_method`'s
/// type-variable branch.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TraitDef {
    pub name: Path,
    /// Associated type names (lowercase), e.g. `["item"]` for `Iter`.
    pub assoc_types: Vec<String>,
    /// Method signature templates, keyed by (unqualified) method name. Each
    /// signature's `params`/`ret` may mention `Self` and `assoc_types` as
    /// type variables (`Type::Named(Path::root("self"), [])` etc.).
    pub methods: BTreeMap<String, FnSig>,
    /// This trait's *own* method names in `deftrait` source order. Always the
    /// same length as `methods`. Not the vtable layout — see
    /// [`Self::vtable_order`], which prepends the inherited methods.
    pub method_order: Vec<String>,
    /// Visible outside its defining module.
    pub public: bool,
    pub builtin: bool,
    /// Direct supertraits, in written order — the mandatory list right after
    /// the trait name (`(deftrait Ord (Eq) ...)`), empty for `()`. Reuses
    /// [`TraitBound`] verbatim: a supertrait is Rust's `trait Ord where Self:
    /// Eq`, i.e. a bound whose type variable is always `Self`, so only the
    /// trait path and the associated-type pins need storing. A supertrait's
    /// associated types must be *fully* pinned here (`Checker::check_deftrait`
    /// enforces it), which is what keeps [`Self::assoc_types`] meaning "this
    /// trait's own" and leaves `:dyn Sub<...>` pin arity unchanged.
    pub supertraits: Vec<TraitBound>,
    /// The linearized vtable layout: every transitively inherited method
    /// first (supertraits in written order, first occurrence wins), then
    /// [`Self::method_order`] — **the sole authority for vtable slot
    /// numbering** (`Checker::dyn_vtable_slots`, `Checker::check_dyn_call`).
    /// `methods` is keyed by name, so its iteration order is alphabetical
    /// rather than the declared one — it must never be used to lay out a table
    /// that a compiled call site indexes by a baked-in constant.
    ///
    /// Inherited methods coming *first* is load-bearing, not cosmetic: it
    /// makes `X.vtable_order` a prefix of `Y.vtable_order` for every `X` on
    /// `Y`'s leftmost supertrait spine, so upcasting a `:dyn Y` to a `:dyn X`
    /// along that spine reuses the very same vtable and slot numbers — a
    /// purely type-level operation (`Checker::coerce_to_dyn`).
    pub vtable_order: Vec<String>,
    /// Parallel to [`Self::vtable_order`]: the trait that *declares* each
    /// entry (this trait's own path for its own methods). Lets a lookup find
    /// the declaring `TraitDef` — and hence the `FnSig` and the pin
    /// substitution to compose — without re-walking the supertrait graph.
    pub vtable_owner: Vec<Path>,
    /// Default method bodies, keyed by method name — a subset of
    /// [`Self::method_order`]. `Checker::check_impl` synthesizes an ordinary
    /// `defmethod` from one whenever an `impl` omits that method, so nothing
    /// downstream of the checker ever learns a body was defaulted.
    pub defaults: BTreeMap<String, TraitDefault>,
}

/// A `deftrait` method that was written with a body — Rust's default method
/// implementation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TraitDefault {
    /// The *whole* method item as written, `(name (params) ret [where] [doc]
    /// body...)`, so `Checker::check_impl` can push it through the very same
    /// rebuild-and-check loop a written `impl` item takes: a default is
    /// exactly "an `impl` item the user didn't type". Stored as
    /// [`crate::owned_form::OwnedForm`]s because a `Value` is a heap index, which
    /// means nothing once the heap it indexed is gone.
    pub item: Vec<crate::owned_form::OwnedForm>,
    /// The namespace the `deftrait` was checked in. The body must resolve
    /// names *there* — a default written against the trait's module-private
    /// helpers has to keep working at an `impl` in some other module.
    pub ns: Vec<String>,
}

/// A blanket `impl` — one written over a bare type variable rather than a
/// named type, so it covers *every* type satisfying its bounds:
/// `(impl<T> Clamp T (where (Ord T)) ...)`, Rust's `impl<T: Ord> Clamp for T`.
///
/// Nothing is registered on any `AdtDef` when this is declared, and no method
/// body is checked: a blanket impl generates code only when some concrete
/// type actually reaches it, one materialization per type
/// (`Checker::materialize_blanket_impl`, driven through the same
/// `SpecRequest` queue monomorphization already uses). An `impl` written over
/// a *constructor* (`impl Iter vector-iter<T>`) is not a blanket impl — it
/// has a single owning `AdtDef` and takes the ordinary path.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlanketImpl {
    pub trait_path: Path,
    /// The type-variable name the impl abstracts over, as written (`t`).
    pub target_var: String,
    /// The impl-level `(where ...)`, keyed by type-parameter name — the
    /// condition a candidate type must satisfy to be covered.
    pub bounds: BTreeMap<String, Vec<TraitBound>>,
    /// `(type AssocName Type)` items, as written; the type may mention
    /// `target_var`, so it can only be parsed once the target is known.
    pub assoc: Vec<(String, crate::owned_form::OwnedForm)>,
    /// Method items, as written — replayed per materialization.
    pub methods: Vec<Vec<crate::owned_form::OwnedForm>>,
    /// The namespace the `impl` was written in, which its method bodies and
    /// type annotations must be re-checked under.
    pub ns: Vec<String>,
}

/// A namespace (module): a container of free functions, types, constructors,
/// child modules, and `use` aliases. `Foo::Bar` is resolved by descending into
/// the child module `Foo` and looking up `Bar` there. (Types are *not*
/// namespaces — they own associated items in [`AdtDef::assoc`].)
/// A `deftype` alias: `(deftype name<T...> Body)`. `body` is stored already
/// canonicalized and already alias-expanded, which is what makes expansion at
/// a use site a single non-recursive substitution and makes an alias cycle
/// unconstructible: by the time `B`'s body is stored, any `A` in it has
/// already become `A`'s own body. The cost is the usual textual-precedence
/// rule this language applies everywhere (see `check_supertrait_impls`) —
/// redefining `A` afterwards does not reach back into `B`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TypeAlias {
    /// The alias's fully-qualified name, its identity for diagnostics.
    pub name: Path,
    /// The type parameters written in the header (`(deftype pair<T> ...)`),
    /// in order. A use site must supply exactly this many arguments.
    pub params: Vec<String>,
    /// What the alias stands for, with `params` left as bare type variables.
    pub body: Type,
    /// See [`FnSig::public`].
    pub public: bool,
}

#[derive(Default, serde::Serialize, serde::Deserialize, Clone)]
pub struct Namespace {
    /// Child modules, keyed by their (unqualified) name.
    pub modules: HashMap<String, Namespace>,
    /// Free functions defined directly here, keyed by unqualified name.
    pub fns: HashMap<String, FnSig>,
    /// `deftrait`s defined directly here, keyed by unqualified name.
    pub traits: HashMap<String, TraitDef>,
    /// Blanket `impl`s declared directly here (`(impl<T> Clamp T (where (Ord
    /// T)) ...)`) — kept in declaration order, which is not consulted for
    /// resolution (at most one may cover any trait) but keeps diagnostics
    /// and fasl output deterministic.
    pub blanket_impls: Vec<BlanketImpl>,
    /// `defmacro`s defined directly here, keyed by unqualified name. Kept
    /// separate from `fns` so the checker's head-symbol dispatch can tell a
    /// macro call (expand, then re-check) from an ordinary function call.
    pub macros: HashMap<String, MacroDef>,
    /// Types defined directly here, keyed by unqualified name. `AdtDef::name`
    /// holds the fully-qualified name used as the type's identity.
    pub types: HashMap<String, AdtDef>,
    /// Constructor (unqualified) name -> (owning type path, variant index).
    pub ctors: HashMap<String, (Path, usize)>,
    /// Global variables/constants defined directly here, by unqualified name.
    pub vars: HashMap<String, VarInfo>,
    /// `use` aliases for items: unqualified name -> absolute path (from the root).
    pub aliases: HashMap<String, Vec<String>>,
    /// `use` aliases for modules: short name -> absolute module path.
    pub mod_aliases: HashMap<String, Vec<String>>,
    /// `deftype` aliases defined directly here, keyed by unqualified name.
    /// Kept apart from `types`: an alias is not a type, it is a *spelling*
    /// that `Checker::canon` rewrites away, so nothing past the type parser
    /// ever sees one (`mangle_type`, the fasl, the compile pipeline, an error
    /// message — all show the expansion).
    pub type_aliases: HashMap<String, TypeAlias>,
    /// Bare names snapshotted from a `(use Type)` of a type's *static*
    /// associated functions (`instance: false`, e.g. `HashTable::new`) —
    /// unqualified name -> (owning type path, method name). Mirrors `ctors`
    /// (which plays the same role for a type's *constructors*) but kept
    /// separate since a constructor and a static method are looked up
    /// through different machinery (`check_construct` vs `check_assoc_call`)
    /// — see `Checker::check_use`/`Checker::check_list`.
    pub static_uses: HashMap<String, (Path, String)>,
}

impl Namespace {
    /// Descend through child modules along `path`; `None` if any segment is not
    /// a module.
    pub fn module(&self, path: &[String]) -> Option<&Namespace> {
        let mut ns = self;
        for seg in path {
            ns = ns.modules.get(seg)?;
        }
        Some(ns)
    }

    /// Descend through child modules along `path`, creating empty modules as
    /// needed.
    pub fn module_mut(&mut self, path: &[String]) -> &mut Namespace {
        let mut ns = self;
        for seg in path {
            ns = ns.modules.entry(seg.clone()).or_default();
        }
        ns
    }

    /// Register a type here. Does *not* index its constructors as bare names
    /// — a type's constructors are only reachable as `Type::ctor` (or after
    /// `use Type`, see `Checker::check_use`) unless the caller separately
    /// calls [`Self::register_ctors`] (only done for `Sexpr`, the one
    /// exception — see `Registry::with_builtins`).
    pub fn add_type(&mut self, def: AdtDef) {
        let local = def.name.last_segment().to_string();
        self.types.insert(local, def);
    }

    /// Indexes `variants` as bare constructor names (first definer wins, so
    /// builtins stay reachable even if a later `use` snapshots the same
    /// name). Called explicitly — unlike the old `add_type`, this is no
    /// longer automatic — by `with_builtins` (for `Sexpr` only) and by
    /// `Checker::check_use` (to snapshot a `use`d type's constructors into
    /// the current namespace).
    pub fn register_ctors(&mut self, type_path: &Path, variants: &[Variant]) {
        for (i, v) in variants.iter().enumerate() {
            self.ctors.entry(v.name.clone()).or_insert((type_path.clone(), i));
        }
    }
}

/// Definition-site source locations, keyed by the same fully-qualified
/// identity each table's `Registry`/`Namespace` lookup uses. Kept separate
/// from `FnSig`/`AssocFn`/`AdtDef`/`VarInfo`/`TraitDef`/`MacroDef` themselves
/// rather than adding a field to each — those are constructed at ~150 call
/// sites in [`Registry::with_builtins`] alone, none of which have (or need) a
/// source location to record, so a field there would mean touching every one
/// of them for no benefit. Instead, only the checker's user-facing
/// registration points (`Checker::check_defun`/`check_defmethod`/
/// `check_defstruct`/`check_defenum`/`check_defvar`/`check_deftrait`/
/// `check_defmacro`) insert an entry here. Consulted by the LSP's
/// goto-definition (`check::locate::definition_target`) — a lookup miss
/// (a builtin, or anything not yet registered) is not an error, just "no
/// definition to jump to".
#[derive(Clone, Default)]
pub struct DefLocs {
    pub fns: HashMap<Path, Loc>,
    pub methods: HashMap<(Path, String), Loc>,
    pub types: HashMap<Path, Loc>,
    pub vars: HashMap<Path, Loc>,
    pub traits: HashMap<Path, Loc>,
    pub macros: HashMap<Path, Loc>,
    /// A local variable reference's own source position (`line`, `col`) ->
    /// the `Loc` of the binding site it resolves to (a `let`/`let*` binding
    /// name, a `lambda`/`labels`/`defun`/`defmethod` parameter or receiver
    /// name, a `labels` function name, or a `match`-pattern binding).
    /// Unlike the other tables above
    /// (keyed by a fully-qualified [`Path`], since a global definition has
    /// exactly one), a local binding has no such stable identity — the same
    /// name can be bound many times in one file — so this is keyed by the
    /// *reference's* own position instead, resolved once at check time
    /// (`Checker::check_at`, where both the reference's position and its
    /// binding's recorded position — `Env`'s third tuple element — are
    /// available together) rather than searched at query time. Consulted by
    /// `check::locate::definition_target`'s `var` arm.
    pub local_refs: HashMap<(u32, u32), Loc>,
    /// The type the checker proved for the node at each source position — the
    /// LSP's hover text.
    ///
    /// A side table because the type genuinely stops at the checker (see
    /// `check::core::Checked`): it is not written into the lowered form and
    /// nothing downstream can ask a node for it. Hover is the one consumer that
    /// still wants it, so it is recorded where it is known — `Checker::check_at`
    /// has the position and the type in hand at the same moment, exactly as it
    /// does for `local_refs` — rather than by keeping types in the IR for one
    /// query's sake.
    pub node_types: HashMap<(u32, u32), crate::Type>,
}

/// Docstrings, keyed the same way [`DefLocs`] keys source locations — kept
/// separate from `FnSig`/`AssocFn`/`AdtDef`/`VarInfo`/`TraitDef`/`MacroDef`
/// for the identical reason `DefLocs` is (see its doc comment): those are
/// constructed at ~150 call sites in [`Registry::with_builtins`] alone, none
/// of which have (or need) a docstring. Only the checker's user-facing
/// registration points (the same set that populates `DefLocs`) insert an
/// entry here. Consulted by the `documentation` special form
/// (`Checker::check_documentation`) and the LSP's hover
/// (`check::locate::doc_for`).
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Docs {
    pub fns: HashMap<Path, String>,
    pub methods: HashMap<(Path, String), String>,
    pub types: HashMap<Path, String>,
    pub vars: HashMap<Path, String>,
    pub traits: HashMap<Path, String>,
    pub macros: HashMap<Path, String>,
    /// Docstrings on a `deftrait`'s *individual* methods, keyed by (trait
    /// path, method name). Only a method with a default body can carry one:
    /// in a bodyless signature a trailing string is the return value, with
    /// nothing to tell the two apart.
    pub trait_methods: HashMap<(Path, String), String>,
}

/// The checker's symbol table: a tree of namespaces rooted at [`Registry::root`].
pub struct Registry {
    pub root: Namespace,
    pub def_locs: DefLocs,
    pub docs: Docs,
    /// Per-`defstruct` slot default forms, keyed by the type's path and
    /// parallel to [`AdtDef::field_names`] — `None` where a slot declared no
    /// default. Sparse: only a `defstruct` that actually wrote one appears.
    ///
    /// A side table rather than an [`AdtDef`] field, for the same reason
    /// [`Docs`] is one: every built-in type constructs an `AdtDef` literally
    /// (thirty sites) and none of them has a slot default to record. What
    /// needs it is `Checker::check_defstruct`'s generated constructors, and —
    /// across a module or a dump boundary — a child `defstruct` that
    /// `:include`s this one.
    ///
    /// Held as [`OwnedForm`](crate::owned_form::OwnedForm)s, and *unchecked*:
    /// these are the default expressions as written, spliced into a
    /// generated constructor's source before that constructor is checked.
    pub struct_defaults: BTreeMap<Path, Vec<Option<crate::owned_form::OwnedForm>>>,
}

impl Registry {
    /// A registry whose root namespace holds the built-in `Option`/`Result`/
    /// the concrete error types/`Sexpr` and the i32 arithmetic/comparison
    /// operators.
    pub fn with_builtins() -> Registry {
        let mut root = Namespace::default();
        for def in builtin_sum_defs() {
            root.add_type(def);
        }
        // `Sexpr` is the one type whose data constructors (`nil`/`int`/`str`/
        // ...) stay reachable as bare names without a `use` — so `(Int 5)`/
        // `(Nil)` datum literals stay writable. `Option`/`Result`/the error
        // types' constructors are `Type::ctor` (or `use`d) only — see
        // `Checker::check_use`. The one exception is the `cons` variant: its
        // bare name is removed here so `(cons a b)` resolves to the free
        // `cons<T,U>` pair function (Symbol/Sexpr redesign Phase 4b), not the
        // `Sexpr` cons cell. Internal `Sexpr` cons construction (`list`/
        // quasiquote/`&rest`) goes through `Checker::sexpr_cons_ctor`/
        // `sexpr-cons` directly instead of this bare name.
        let def = sexpr_def();
        root.register_ctors(&def.name, &def.variants);
        root.ctors.remove("cons");
        root.add_type(def);
        root.add_type(hashtable_def());
        root.add_type(vector_def());
        root.add_type(scope_def());
        // The (typelisp-hosted) `compile`/`compile-file` compiler's view of
        // LLVM: four more builtin types, metadata-only like `hashtable_def`
        // (the runtime implementation lives in `eval_llvm_builtin_method` in
        // `crate::eval::interp`). `llvm-basic-block` has no methods of its
        // own yet — it's only ever produced by `add-block` and consumed by
        // `position-at-end` — but still needs registering as a type so it
        // has a `Path` to appear as a parameter/return type.
        root.add_type(llvm_module_def());
        root.add_type(llvm_function_def());
        root.add_type(llvm_builder_def());
        root.add_type(llvm_basic_block_def());
        root.add_type(llvm_value_def());
        // Primitive value types (i8..usize/f32/f64/bool/char/string) get a
        // method table too, so `defmethod` can target them (see
        // `check_defmethod`/`check_instance_method`, which map a primitive
        // `Type` to its registry `Path` via `prim_type_path`). `string`/`char`/
        // The integer widths and `f64` additionally get a built-in method set (no
        // `defmethod` body to check; the runtime implementation lives in
        // `eval_builtin_method` in `crate::eval::interp`, the same
        // metadata-only pattern as `hashtable_def`) — every
        // other primitive's table starts empty. Arithmetic/comparison
        // operators are *instance* methods (not free functions) precisely so
        // the same symbol (`+`, `<`, ...) can be overloaded per receiver
        // type — every integer width shares one `+` symbol, and `f64` another,
        // resolved by `check_instance_method` on the first argument's static
        // type exactly like `(get h k)` resolves to `HashTable`'s `get`.
        for ty in crate::types::primitive_types() {
            let name = crate::types::prim_type_path(&ty).expect("primitive_types() are all prim_type_path-mappable");
            let assoc = match ty {
                Type::Str => string_assoc(),
                Type::Char => char_assoc(),
                Type::Bignum => bignum_assoc(),
                Type::Ratio => ratio_assoc(),
                Type::Bool => bool_assoc(),
                Type::Symbol => symbol_assoc(),
                // Every integer width and both float widths get the same
                // catalog as `i32`/`f64` did alone. The catalog is shared but
                // the *arithmetic* is not: a type name means its width and
                // its signedness (`types::int_width_signed`), so `u8`
                // addition wraps at 8 bits and `f32` division rounds to
                // binary32 — `eval_int_builtin`/`float_at` cut every result
                // back at the receiver's own width. What this entry removes
                // is the state these types were in before: registered,
                // nameable, and with no `+` at all.
                ref t if t.is_integer() => int_assoc(t.clone()),
                ref t if t.is_float() => float_assoc(t.clone()),
                _ => BTreeMap::new(),
            };
            root.add_type(AdtDef {
                name,
                params: Vec::new(),
                variants: Vec::new(),
                assoc,
                public: true,
                builtin: true,
                kind: AdtKind::Sum,
                field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new(),
            });
        }
        // `random-state` (CL's `random-state`): a mutable PRNG stream, its
        // actual bit-twiddling done in Rust (`interp::eval_random_state_next`
        // — the fixed-width xorshift step isn't expressible in typelisp,
        // which has no bitwise operators). None of these has a natural
        // receiver to dispatch on, so all four stay free functions. `random`/`make-random-state`/`random-state-p`
        // are ordinary `defun`s in the prelude built on top of these — a
        // `random-state` has nowhere else to hang an `&optional` parameter
        // off of, since `check_call_opt_key` only resolves `&optional`/`&key`
        // for a `defun`'s own `FnSig`, not an ad hoc native one.
        root.fns.insert("make-random-state-fresh".to_string(), FnSig { type_params: vec![], rest: None, params: vec![], ret: Type::RandomState, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("random-state-copy".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::RandomState], ret: Type::RandomState, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("random-state-next".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::RandomState, Type::I32], ret: Type::I32, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `seed-random-state` (SBCL's `sb-ext:seed-random-state`, not in the
        // standard): the stream a given integer names, for a caller who wants
        // a run to be reproducible. CL itself has no portable way to seed —
        // `make-random-state` takes `nil`/`t`/a state and nothing else — so
        // this deliberately does *not* borrow a standard name for a
        // non-standard operation. The integer-to-state map lives in
        // `typelisp_rt::seeded_random_state`, which is where the reason it
        // isn't the identity is written down.
        root.fns.insert("seed-random-state".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::I32], ret: Type::RandomState, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `get-universal-time`/`get-internal-real-time` (CLHS 25.1): wall-clock
        // and monotonic-ish timers, respectively. `get-universal-time` counts
        // seconds since 1900-01-01 UTC (CL's epoch, 2208988800s before the
        // Unix epoch); `get-internal-real-time` counts
        // `internal-time-units-per-second` (a prelude `defvar`, 1_000_000 —
        // i.e. microseconds) since an arbitrary process-start reference point,
        // matching CL's own "units are implementation-defined, only the ratio
        // between two calls means anything" contract.
        //
        // Both return a *struct*, not a number: neither count fits an `i32`
        // (1900-epoch seconds passed 2^31 in 1968; microseconds overflow in
        // about 35 minutes) and this language has no 64-bit-wide integer type
        // — see `Type::is_integer`. Splitting the count the way the domain
        // already splits it costs nothing: `decode-universal-time`'s first
        // step is exactly this division, and `time`'s subtraction is two
        // field subtractions.
        root.fns.insert("get-universal-time".to_string(), FnSig { type_params: vec![], rest: None, params: vec![], ret: universal_time(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("get-internal-real-time".to_string(), FnSig { type_params: vec![], rest: None, params: vec![], ret: internal_time(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `parse-int`/`parse-float`: untrusted-text numeric parsing
        // (`docs/language-design.md` §4.1's planned conversion catalog) —
        // `Result`, not a panic, since the input is runtime text the caller
        // doesn't control (unlike a source literal, which the reader/checker
        // already validate before this code ever runs).
        root.fns.insert("parse-int".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Str], ret: result_of(Type::I32, error_ty(PARSE_INT_ERROR)), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("parse-float".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Str], ret: result_of(Type::F64, error_ty(PARSE_FLOAT_ERROR)), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `read`: parses one `Sexpr` form out of a string with the same
        // reader `typl`/the REPL use for source text
        // (`crate::read::Reader::read`) — CL's `read-from-string`.
        register_stream_builtins(&mut root);
        register_system_builtins(&mut root);
        register_readtable_builtins(&mut root);
        root.fns.insert("read".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Str], ret: result_of(option_of(sexpr()), error_ty(READ_ERROR)), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `read-datum-at`: one datum from a string starting at a character
        // index, paired with where reading stopped — CL's `read-from-string`
        // and its second return value, which this language has no multiple
        // values to carry. The prelude's `read-from-string` /
        // `read-from-string-preserving-whitespace` are this with the two
        // `preserve` settings and a default `start`; the primitive keeps a
        // name of its own because the CL one is the prelude's.
        root.fns.insert(
            "read-datum-at".to_string(),
            FnSig {
                type_params: vec![],
                rest: None,
                params: vec![Type::Str, Type::I32, Type::Bool],
                ret: result_of(
                    Type::Named(Path::root("cons-cell"), vec![option_of(sexpr()), Type::I32]),
                    error_ty(READ_ERROR),
                ),
                public: true,
                builtin: true,
                bounds: BTreeMap::new(),
                optionals: Vec::new(),
                keys: Vec::new(),
            },
        );
        // `eval`: type-checks and runs a runtime `Sexpr` against the current
        // global environment, CL-style (`Interp::eval_form`). Sees all globals
        // but not the caller's lexical locals; a definition form registers
        // immediately. Result is a `Sexpr` (the value, or a definition's name
        // symbol); malformed/ill-typed input is `Err`, not a panic.
        root.fns.insert("eval".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: result_of(option_of(sexpr()), error_ty(EVAL_ERROR)), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `macroexpand-1`/`macroexpand`: what the checker does to a macro
        // call, made available to a program (CL's own, and the only way to
        // see a `defmacro`'s output without reading the checker's mind).
        //
        // CL returns a second value saying whether anything expanded; there
        // are no multiple values here, so `macroexpand-1` answers
        // `Option<Sexpr>` — `none` *is* "not a macro call", and it carries
        // strictly more than CL's boolean, since the caller cannot mistake a
        // macro that expands to itself for a non-macro. `macroexpand` repeats
        // until `none` and answers the final form, as CL's does.
        root.fns.insert("macroexpand-1".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: result_of(option_of(sexpr()), error_ty(EVAL_ERROR)), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("macroexpand".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: result_of(option_of(sexpr()), error_ty(EVAL_ERROR)), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // The pretty printer's user-callable layout operators (CLHS 22.2.1),
        // minus the stream argument typelisp has no streams for. They act on
        // the logical block the `pprint-logical-block` special form opened
        // (`Interp`'s `PrettySession`) and are no-ops outside one, exactly as
        // CL's are when the stream is not a pretty stream — which is why they
        // are plain functions here and need no special-form treatment: their
        // arguments are CL's own keywords, i.e. ordinary `Symbol` values.
        //
        // `pprint-pop` returns `()` once the block's list is exhausted;
        // `pprint-list-exhausted` is the predicate to test first (the prelude
        // macro `pprint-exit-if-list-exhausted` is the CL-spelled wrapper).
        root.fns.insert("pprint-newline".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Symbol], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("pprint-indent".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Symbol, Type::I32], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("pprint-tab".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Symbol, Type::I32, Type::I32], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("pprint-pop".to_string(), FnSig { type_params: vec![], rest: None, params: vec![], ret: option_of(sexpr()), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("pprint-list-exhausted".to_string(), FnSig { type_params: vec![], rest: None, params: vec![], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `cons`/`car`/`cdr`/`set-car`/`set-cdr` are no longer `Sexpr` builtins:
        // the Symbol/Sexpr redesign (Phase 4b) repurposes `cons`/`car`/`cdr` to
        // the generic `cons<T,U>` pair (`prelude.rs`'s free `cons` +
        // `cons-cell<A,B>`'s `car`/`cdr` field-accessor methods). The `Sexpr`
        // cons/nil duality is served by the `sexpr-*` island layer below
        // (`sexpr-cons`/`sexpr-car`/`sexpr-cdr`), and the destructive
        // `set-car`/`set-cdr` are dropped entirely (their only callers —
        // `nconc`/`nreverse` — were removed in Phase 5; a `cons<T,U>` field is
        // mutated with `(setf p::car v)` instead).
        // Internal `Sexpr` navigation layer (Symbol/Sexpr redesign Phase 1,
        // `docs/dev/symbol-sexpr-redesign.md`): `sexpr-cons`/`sexpr-car`/
        // `sexpr-cdr`/`sexpr-consp`/`sexpr-null`/`sexpr-atom`. These are exact
        // duplicates of the free `cons`/`car`/`cdr` and the prelude
        // `consp`/`null`/`atom` today, but under a dedicated `sexpr-` island
        // namespace so the self-hosting compiler (`compiler.rs`) and prelude
        // macros can navigate `Sexpr` structure *without* going through the
        // user-facing `car`/`cdr` (Phase 4 repurposes those to a generic
        // `cons<T,U>` pair) or `match` (Phase 5 fences `match` to enums). The
        // predicates read the runtime tag directly (`Value::is_cons`/
        // `is_empty`) rather than pattern-matching, so they survive that
        // fence. Same heap operations as `cons`/`car`/`cdr` (see
        // `Interp::eval_builtin`), so no new runtime machinery is needed.
        root.fns.insert("sexpr-cons".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr()), option_of(sexpr())], ret: option_of(sexpr()), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-car".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: option_of(sexpr()), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-cdr".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: option_of(sexpr()), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-consp".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-null".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-atom".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // Internal `Sexpr` payload extractors (Symbol/Sexpr redesign Phase 2):
        // typed field readers that used to be `match`-based typelisp defuns in
        // `compiler.rs` (`(match s ((Int n) n) (_ (panic ...)))`), moved to Rust
        // builtins (`Interp::eval_builtin`) so the island no longer needs the
        // user-facing `match` at all. Each panics on a tag mismatch, preserving
        // the old defuns' `(_ (panic ...))` contract. `sexpr-symp` is the tag
        // predicate its non-panic-fallback caller (`form-is-borrowed?`) needs
        // to branch on a `Sym` node — a peer of
        // `sexpr-consp`/`sexpr-null`/`sexpr-atom`.
        root.fns.insert("sexpr-i32".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::I32, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-f64".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::F64, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-f32".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::F32, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // One extractor per integer type, not one per machine word: the five
        // narrow widths are separate `Sexpr` variants carrying separate boxes
        // (`BoxedObj::Narrow`), and each of these returns exactly its own.
        for (name, ty) in [
            ("sexpr-i8", Type::I8),
            ("sexpr-i16", Type::I16),
            ("sexpr-u8", Type::U8),
            ("sexpr-u16", Type::U16),
            ("sexpr-u32", Type::U32),
        ] {
            root.fns.insert(name.to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: ty, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        }
        root.fns.insert("sexpr-bool".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-char".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::Char, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-str".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::Str, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-sym-name".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::Str, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("sexpr-symp".to_string(), FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr())], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `equal`/`equalp`: structural equality (CL `equal`/`equalp`), Rust
        // builtins (`Interp::eval_builtin`'s `sexpr_equal`/`sexpr_equalp`)
        // since the Symbol/Sexpr redesign fenced `match` off `Sexpr` (Phase 5).
        // They used to be prelude `defun`s (`(match a ((Cons ..) ..) ..)`); the
        // per-scalar `equal`/`equalp` *methods* (string/char/int/...) are
        // separate (`registry::string_assoc` etc.) and resolved first when the
        // receiver is one of those types — these free ones are the fallback
        // for everything else (`case` expands to `(equal ..)`).
        //
        // **Generic, not `Sexpr`-typed**, and that is the whole point. With
        // concrete `Option<Sexpr>` parameters both arguments widened into
        // S-expression data independently, so *any two values of any two
        // types* compared — `(equalp (pa::new 1) (pb::new "z"))` type-checked
        // and answered `false`. That silently contradicted the rule the
        // language already enforces everywhere else: `(equalp 1 "a")` is a
        // type error, because `i32` has its own `equal` method and that
        // method demands both sides be `i32`. The escape hatch opened only
        // for types with *no* such method — `Option<T>`, `Result<T,E>`, user
        // ADTs — which is exactly where a mistake is hardest to see.
        //
        // One type parameter used twice forces the two arguments to agree.
        // No template exists for a builtin, so `check_call` leaves the call
        // runtime-dispatched (see its "builtin generic free functions"
        // comment) — the runtime already compares `Value`s structurally
        // whatever they hold, so nothing downstream changes.
        let eq_generic = || FnSig {
            type_params: vec!["t".to_string()],
            rest: None,
            params: vec![tvar("t"), tvar("t")],
            ret: Type::Bool,
            public: true,
            builtin: true,
            bounds: BTreeMap::new(),
            optionals: Vec::new(),
            keys: Vec::new(),
        };
        root.fns.insert("equal".to_string(), eq_generic());
        root.fns.insert("equalp".to_string(), eq_generic());
        // `gensym` is *not* here: it moved into the prelude in Phase 4c, so
        // that CL's `*gensym-counter*` could be a real variable a program can
        // read and set. A builtin's counter lived on the `Heap` and nothing
        // could name it.
        // `symbol->string`/`string->symbol`: the only bridges between the
        // interned `Symbol` handle and its textual name. They are Rust builtins
        // (`Interp::eval_builtin`) rather than typelisp because they touch the
        // symbol intern table directly; `Sexpr::Sym` now wraps `Symbol` (not
        // `Str`), so the old prelude `(match s (Sym name) name)` /
        // `(Sym s)` definitions no longer type-check.
        root.fns.insert("symbol->string".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Symbol], ret: Type::Str, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        root.fns.insert("string->symbol".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Str], ret: Type::Symbol, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `exit`: process termination (cl-equivalence-catalog.md §1.2). Unlike
        // `panic`/`unreachable`/`todo` (which unwind through `EvalError::Panic`,
        // a typelisp-level signal), this needs an actual OS call
        // (`std::process::exit`, in `Interp::eval_builtin`'s `"exit"` arm), so
        // it stays an ordinary `Rust` builtin rather than a `defmacro`. `Never`
        // return type, same as `panic`, so it satisfies any expected type.
        root.fns.insert("exit".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::I32], ret: Type::Never, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() });
        // `compile` is a genuine special form (`Checker::check_compile`,
        // dispatched by name in `Checker::check_list` alongside `quote`/
        // `panic`/etc. — never an ordinary call), so unlike `compile-file`
        // below it has no `root.fns` entry: its argument is an unevaluated
        // symbol or `::`-path, resolved directly against the current
        // namespace into an `compile-fn`(CompileTarget)` node.
        // `compile-file`: AOT-compiles an independent source file to a
        // native executable (see `Interp::eval_builtin`'s `"compile-file"`
        // arm / `compile::aot::compile_file`). Same free-function shape as
        // `compile` above, just two string arguments (source path, output
        // path) instead of one.
        root.fns.insert(
            "compile-file".to_string(),
            FnSig { type_params: vec![], rest: None, params: vec![Type::Str, Type::Str], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() },
        );
        // `dump`: writes this session's whole environment — the units it was
        // loaded from, plus one for what it has defined since — to a file
        // `typl --image` can start from. Same free-function shape as
        // `compile-file`, one path argument (see `compile::dump::dump_image`).
        root.fns.insert(
            "dump".to_string(),
            FnSig { type_params: vec![], rest: None, params: vec![Type::Str], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() },
        );
        Registry { root, def_locs: DefLocs::default(), docs: Docs::default(), struct_defaults: BTreeMap::new() }
    }

    /// Look up a type by its fully-qualified [`Path`] (e.g. `geo::point`).
    pub fn type_def(&self, path: &Path) -> Option<&AdtDef> {
        self.root.module(path.parent())?.types.get(path.last_segment())
    }

    /// Mutable lookup of a type by its fully-qualified [`Path`].
    pub fn type_def_mut(&mut self, path: &Path) -> Option<&mut AdtDef> {
        self.root.module_mut(path.parent()).types.get_mut(path.last_segment())
    }

    /// Look up a free function by its fully-qualified [`Path`].
    pub fn fn_sig(&self, path: &Path) -> Option<&FnSig> {
        self.root.module(path.parent())?.fns.get(path.last_segment())
    }

    /// Look up a `deftrait` by its fully-qualified [`Path`].
    pub fn trait_def(&self, path: &Path) -> Option<&TraitDef> {
        self.root.module(path.parent())?.traits.get(path.last_segment())
    }

    /// Resolve `method` against `tdef` *including inherited methods*,
    /// returning the trait that declares it and its signature template.
    ///
    /// `TraitDef::methods` holds a trait's own methods only, so a bare
    /// `tdef.methods.get(m)` silently misses everything a supertrait
    /// contributes. Every consult point wants the inherited view, so they all
    /// go through here: the answer is read off [`TraitDef::vtable_owner`],
    /// which `Checker::check_deftrait` already computed, rather than
    /// re-walking the supertrait graph.
    pub fn trait_method<'a>(
        &'a self,
        tdef: &'a TraitDef,
        method: &str,
    ) -> Option<(&'a TraitDef, &'a FnSig)> {
        let slot = tdef.vtable_order.iter().position(|m| m == method)?;
        let owner = &tdef.vtable_owner[slot];
        // Own methods dominate: skip the second lookup (and work even while
        // `check_deftrait`'s self-referential stub is the registered entry).
        if owner == &tdef.name {
            return Some((tdef, tdef.methods.get(method)?));
        }
        let odef = self.trait_def(owner)?;
        Some((odef, odef.methods.get(method)?))
    }

    /// The blanket `impl` covering `trait_path`, if one is declared anywhere
    /// in the loaded tree. At most one may exist per trait
    /// (`Checker::check_impl` rejects a second), which is what makes this
    /// answer independent of module walk order.
    pub fn blanket_impl(&self, trait_path: &Path) -> Option<&BlanketImpl> {
        fn walk<'a>(ns: &'a Namespace, t: &Path) -> Option<&'a BlanketImpl> {
            ns.blanket_impls
                .iter()
                .find(|b| b.trait_path == *t)
                .or_else(|| ns.modules.values().find_map(|c| walk(c, t)))
        }
        walk(&self.root, trait_path)
    }

    /// Every type that has an `impl` for `trait_path` — the reverse of
    /// [`AdtDef::impls`], which only answers the forward question ("does
    /// *this* type implement it?"). Needed by trait objects (`Type::Dyn`):
    /// building a vtable per (concrete type, trait) pair means enumerating
    /// the pairs, and compiling a `:dyn` call site means knowing every
    /// method body that could end up in one (`core_bridge::collect_targets`).
    ///
    /// Sorted, because the namespace tree is walked through `HashMap`s whose
    /// iteration order varies between runs — an unsorted result would make
    /// diagnostics and generated-code ordering nondeterministic.
    pub fn trait_impls(&self, trait_path: &Path) -> Vec<Path> {
        let mut out = Vec::new();
        collect_trait_impls(&self.root, trait_path, &mut out);
        out.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        out
    }
}

/// Depth-first walk of the module tree collecting types whose `impls` list
/// contains `trait_path` — [`Registry::trait_impls`]'s recursion.
fn collect_trait_impls(ns: &Namespace, trait_path: &Path, out: &mut Vec<Path>) {
    for def in ns.types.values() {
        if def.impls.contains(trait_path) {
            out.push(def.name.clone());
        }
    }
    for child in ns.modules.values() {
        collect_trait_impls(child, trait_path, out);
    }
}

/// `Option<T> = Some(T) | None`.
fn option_def() -> AdtDef {
    AdtDef {
        name: Path::root("option"),
        params: vec!["t".to_string()],
        variants: vec![
            Variant { name: "some".to_string(), fields: vec![tvar("t")] },
            Variant { name: "none".to_string(), fields: vec![] },
        ],
        assoc: BTreeMap::new(),
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new(),
    }
}

/// `Result<T, E> = Ok(T) | Err(E)`.
fn result_def() -> AdtDef {
    AdtDef {
        name: Path::root("result"),
        params: vec!["t".to_string(), "e".to_string()],
        variants: vec![
            Variant { name: "ok".to_string(), fields: vec![tvar("t")] },
            Variant { name: "err".to_string(), fields: vec![tvar("e")] },
        ],
        assoc: BTreeMap::new(),
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new(),
    }
}

/// THE built-in sum types whose values exist at runtime as `BoxedObj::Enum`:
/// `Option`, `Result`, and the four concrete error types. This is the single
/// source both [`Registry::with_builtins`] (the checker side) and
/// `Interp::new` (which seeds its own runtime scope tree so a value's variant
/// name is available to the printer — see that function's doc comment) read;
/// neither defines its own copy.
///
/// `Sexpr` is deliberately absent: its values are never `BoxedObj::Enum` —
/// they use the dedicated `Value` variants (`Value::Cons`, `Value::Symbol`,
/// ...) instead (`Interp::construct_sexpr`), so it needs no entry here for
/// the printer or anything else that walks this list.
pub(crate) fn builtin_sum_defs() -> Vec<AdtDef> {
    let mut defs = vec![option_def(), result_def()];
    defs.extend(builtin_error_defs());
    defs
}

/// The four concrete built-in error types, by (case-folded) type name — the
/// one list every layer reads: [`builtin_error_defs`] registers them, the
/// interpreter builds their values (`crate::eval::interp`'s `result_err`),
/// and both the interpreter and `core_bridge` classify a type as an enum by
/// consulting [`is_builtin_error_type`]. Like `Option`/`Result` they must be
/// recognizable by name outside the registry, since neither of those two
/// layers holds a `Registry` to look a definition up in.
pub const PARSE_INT_ERROR: &str = "parseinterror";
pub const PARSE_FLOAT_ERROR: &str = "parsefloaterror";
pub const READ_ERROR: &str = "readerror";
pub const EVAL_ERROR: &str = "evalerror";
/// CLHS's `file-error`, covering every failure the stream/file layer can
/// report: `open` on a missing file, `delete-file` on a read-only directory,
/// and the wrong-direction/closed-stream program errors that reach a
/// `Result`-typed entry point. CL splits these across
/// `file-error`/`stream-error`; one concrete type is enough here because the
/// *message* is what a program can act on — dispatching on a condition
/// *class* would need a condition system, which this language deliberately
/// does not have.
pub const FILE_ERROR: &str = "fileerror";
pub const BUILTIN_ERROR_TYPES: [&str; 5] =
    [PARSE_INT_ERROR, PARSE_FLOAT_ERROR, READ_ERROR, EVAL_ERROR, FILE_ERROR];

/// The `stream-*` / `file-*` primitives (`eval::interp::Interp::
/// eval_stream_builtin`). Deliberately minimal and untyped-looking: a stream
/// is an opaque `i32` handle here, and the whole CL-shaped surface — the
/// `Stream`/`InputStream`/`OutputStream` traits, the concrete stream types
/// that wrap a handle in a `defstruct`, every composite stream, and the
/// `with-...` macros — is written in `prelude.rs` on top of these.
///
/// Anything that can fail returns `Result<_, FileError>`; a missing file or a
/// closed stream is something programs handle, not a bug.
fn register_stream_builtins(root: &mut Namespace) {
    let file_err = error_ty(FILE_ERROR);
    let mut native = |name: &str, params: Vec<Type>, ret: Type| {
        root.fns.insert(
            name.to_string(),
            FnSig {
                type_params: vec![],
                params,
                ret,
                public: true,
                rest: None,
                builtin: true,
                bounds: BTreeMap::new(),
                optionals: Vec::new(),
                keys: Vec::new(),
            },
        );
    };
    let h = Type::I32;
    let unit_or_err = result_of(Type::Unit, file_err.clone());

    // Constructors. The three standard streams hand out a fresh handle per
    // call; the prelude opens each exactly once, into a global.
    native("stream-stdin", vec![], h.clone());
    native("stream-stdout", vec![], h.clone());
    native("stream-stderr", vec![], h.clone());
    native("stream-string-input", vec![Type::Str], h.clone());
    native("stream-string-output", vec![], h.clone());
    // mode: 0 input, 1 output (truncate), 2 output (append).
    native("stream-open-file", vec![Type::Str, h.clone()], result_of(h.clone(), file_err.clone()));

    // Lifetime and interrogation.
    native("stream-close", vec![h.clone()], unit_or_err.clone());
    native("stream-open-p", vec![h.clone()], Type::Bool);
    native("stream-input-p", vec![h.clone()], result_of(Type::Bool, file_err.clone()));
    native("stream-output-p", vec![h.clone()], result_of(Type::Bool, file_err.clone()));

    // Input. `stream-read-char` returns `Ok(none)` at end of input and `Err`
    // only for a real failure, so end-of-input never has to be a panic.
    native(
        "stream-read-char",
        vec![h.clone()],
        result_of(option_of(Type::Char), file_err.clone()),
    );
    native("stream-unread-char", vec![h.clone(), Type::Char], unit_or_err.clone());
    // Byte I/O, the same shape one level down: `Ok(none)` at end of file.
    // Only a file stream (and stdin/stdout/stderr) answers these — a string
    // stream is a sequence of characters, and CL calls `read-byte` on a
    // character stream an error rather than handing back a UTF-8 encoding
    // nobody wrote.
    native("stream-read-byte", vec![h.clone()], result_of(option_of(Type::I32), file_err.clone()));
    native("stream-write-byte", vec![h.clone(), Type::I32], unit_or_err.clone());
    native("stream-listen", vec![h.clone()], result_of(Type::Bool, file_err.clone()));
    // How many characters have been read out of a *string* input stream.
    // Narrow on purpose: this is what tells a reader macro's caller how much
    // of the text the macro consumed, and no other backend has a position in
    // the same unit. Not CL's `file-position` — that one also seeks.
    native("stream-position", vec![h.clone()], result_of(Type::I32, file_err.clone()));

    // Output. Only whole strings cross this boundary — a per-character
    // built-in call would dominate the cost of writing anything.
    native("stream-write-string", vec![h.clone(), Type::Str], unit_or_err.clone());
    native("stream-at-line-start", vec![h.clone()], result_of(Type::Bool, file_err.clone()));
    native("stream-finish-output", vec![h.clone()], unit_or_err.clone());
    native("stream-take-output-string", vec![h.clone()], result_of(Type::Str, file_err.clone()));

    // Filesystem operations that need no open stream. Every one of these is
    // a *primitive* under a `Pathish` prelude wrapper that carries the CL
    // name (`probe-file`, `delete-file`, `rename-file`, `truename`,
    // `file-write-date`, `directory-p`, `directory`,
    // `ensure-directories-exist`) — which is also why the primitive's name
    // never equals the CL one: both would live in this same root namespace.
    //
    // The `file-` prefix is load-bearing, not decorative: `Interp::
    // eval_builtin` routes every `stream-`/`file-` name to
    // `stream_builtin::stream_builtin`, so a filesystem builtin spelled
    // otherwise would be unreachable interpreted.
    native("file-exists-p", vec![Type::Str], Type::Bool);
    native("file-delete", vec![Type::Str], unit_or_err.clone());
    native("file-rename", vec![Type::Str, Type::Str], unit_or_err.clone());
    native("file-truename", vec![Type::Str], result_of(Type::Str, file_err.clone()));
    // A universal time, on `get-universal-time`'s 1900-epoch scale, so the
    // two are comparable and either decodes with the same prelude function.
    native("file-modified-date", vec![Type::Str], result_of(universal_time(), file_err.clone()));
    native("file-directory-p", vec![Type::Str], Type::Bool);
    native(
        "file-list-directory",
        vec![Type::Str],
        result_of(Type::Named(Path::root("vector"), vec![Type::Str]), file_err.clone()),
    );
    native("file-create-directories", vec![Type::Str], unit_or_err);
}

/// The environment the program is running in (CLHS 25.1) — plus the two
/// things CL has no equivalent of at all and a script cannot do without,
/// `command-line-args` and `getenv`.
///
/// All free functions: none has a receiver to dispatch on, the same reason
/// the `random-state` primitives above are free functions.
/// `lisp-implementation-type` is deliberately *not* here — it is a constant
/// string, so the prelude defines it in typelisp rather than spending a
/// builtin on it.
fn register_system_builtins(root: &mut Namespace) {
    let mut native = |name: &str, params: Vec<Type>, ret: Type| {
        root.fns.insert(
            name.to_string(),
            FnSig {
                type_params: vec![],
                params,
                ret,
                public: true,
                rest: None,
                builtin: true,
                bounds: BTreeMap::new(),
                optionals: Vec::new(),
                keys: Vec::new(),
            },
        );
    };
    // Element 0 names the program in both worlds — the script path under
    // `typl`, the executable under AOT — so one source file can be run either
    // way and index its arguments identically. See
    // `sys_builtin::COMMAND_LINE_ARGS` for how the two are made to agree.
    native("command-line-args", vec![], Type::Named(Path::root("vector"), vec![Type::Str]));
    // `none` covers both "unset" and "not valid Unicode": a `string` here is
    // Rust's, so bytes that aren't UTF-8 have no value to hand back.
    native("getenv", vec![Type::Str], option_of(Type::Str));
    // `$HOME`, or `none` — the primitive under `user-homedir-pathname`, which
    // CL explicitly allows to answer `NIL`.
    native("home-directory", vec![], option_of(Type::Str));
    native("lisp-implementation-version", vec![], Type::Str);
    native("machine-type", vec![], Type::Str);
    native("software-type", vec![], Type::Str);
}

/// The type of a reader macro: what `set-macro-character` stores and what the
/// reader calls when the character turns up.
///
/// The stream is a `string-input-stream` and not a `:dyn PeekInput`, because
/// that is honestly what it is. The reader hands its unread text to the
/// evaluator and gets back a character count (`Interp::call_reader_macro_fn`),
/// so the stream a macro sees is *always* one made over that text — a trait
/// object would be a generality with exactly one inhabitant. Nothing is lost
/// with it: every reader operation a macro wants (`read-sexpr`, `read-char`,
/// `peek-char`, `unread-char`) is generic over `(where (PeekInput S))` and
/// takes the concrete type directly.
///
/// The character comes second, as in CL, and is the *sub*-character for a
/// two-character sequence — `#\{` for `(set-dispatch-macro-character #\# #\{ ...)`
/// — so one function can serve several of them by looking at it.
///
/// `Option<Sexpr>` rather than `Sexpr` because `()` is `none`: a macro that
/// reads an empty list has to be able to say so. A macro that wants to
/// produce *nothing at all* (CL's zero values, what a comment reader macro
/// returns) has no way to say that and is not supported; there is one datum
/// per call.
pub fn reader_macro_fn_type() -> Type {
    Type::Fn(
        vec![Type::Named(Path::root("string-input-stream"), vec![]), Type::Char],
        None,
        Box::new(option_of(sexpr())),
    )
}

/// The readtable (CLHS 23.1): the character-to-function table the reader
/// consults before it does anything else with a character.
///
/// Four functions, not CL's eight. There is no `*readtable*` and no
/// `copy-readtable`, because there is no readtable *object* — a first-class
/// one would have to be a value the reader can be handed, and the reader
/// here is called from Rust drivers that have nowhere to take it from.
/// `make-dispatch-macro-character` is absent for a different reason:
/// `set-dispatch-macro-character` makes the character dispatching by itself,
/// which leaves the separate step nothing to do.
fn register_readtable_builtins(root: &mut Namespace) {
    let mut native = |name: &str, params: Vec<Type>, ret: Type| {
        root.fns.insert(
            name.to_string(),
            FnSig {
                type_params: vec![],
                params,
                ret,
                public: true,
                rest: None,
                builtin: true,
                bounds: BTreeMap::new(),
                optionals: Vec::new(),
                keys: Vec::new(),
            },
        );
    };
    let f = reader_macro_fn_type();
    native("set-macro-character", vec![Type::Char, f.clone()], Type::Unit);
    native("get-macro-character", vec![Type::Char], option_of(f.clone()));
    native("set-dispatch-macro-character", vec![Type::Char, Type::Char, f.clone()], Type::Unit);
    native("get-dispatch-macro-character", vec![Type::Char, Type::Char], option_of(f));
}

/// Whether `p` names one of [`BUILTIN_ERROR_TYPES`].
pub fn is_builtin_error_type(p: &Path) -> bool {
    p.is_simple() && BUILTIN_ERROR_TYPES.contains(&p.last_segment())
}

/// The concrete error type of every fallible built-in, one per failure
/// source: `ParseIntError` (`parse-int`), `ParseFloatError` (`parse-float`),
/// `ReadError` (`read`), `EvalError` (`eval`), `FileError` (every stream and
/// file operation) — modeled on Rust's std, where
/// `Error` is a *trait* and each operation returns its own concrete error
/// (`ParseIntError`, `io::Error`, ...). `Error` is accordingly not a type in
/// this language at all: it is the prelude trait these four implement
/// (`src/prelude.rs`), so code that wants to hold any of them uniformly says
/// `Result<T, :dyn Error>` — the counterpart of Rust's `Box<dyn Error>`.
///
/// Each is a single-variant sum carrying the message string, the same shape
/// (and therefore the same heap representation, `match` destructuring, and
/// trait-object boxing) a user's own `(defenum MyError (my-error string))`
/// would have: nothing about a built-in error type is privileged.
///
/// Type name and variant name coincide, so `(match e ((ParseIntError m) m))`
/// reads as one name — the reader case-folds both to `parseinterror`.
/// [`crate::eval::interp`]'s `result_err` builds these values at run time and
/// must agree with the variant order (a single variant, index 0).
fn builtin_error_defs() -> Vec<AdtDef> {
    BUILTIN_ERROR_TYPES
        .iter()
        .map(|&name| AdtDef {
            name: Path::root(name),
            params: vec![],
            variants: vec![Variant { name: name.to_string(), fields: vec![Type::Str] }],
            assoc: BTreeMap::new(),
            public: true,
            builtin: true,
            kind: AdtKind::Sum,
            field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new(),
        })
        .collect()
}

/// The built-in `Sexpr` sum type (the result type of `read`).
///
/// `Sexpr = i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | Char | Bool | Sym |
/// Str | Cons(Option<Sexpr>, Option<Sexpr>) | Bignum | Ratio | Path`: every
/// S-expression **but** the
/// empty list, which is `Option<Sexpr>`'s `none` since the null-elimination
/// work (`docs/dev/null-elimination-plan.md`). So the type S-expression data
/// is passed around as is `Option<Sexpr>`, not `Sexpr` — that is the point of
/// the exercise, and `car`/`cdr` return it.
///
/// Both `cons` fields are `Option<Sexpr>`, the car as much as the cdr: an
/// element position has to hold the empty list too, or `'(a () b)` could not
/// be written (plan §2.1).
///
/// The `nil` variant below is a **placeholder, not a constructor**. Writing
/// it is refused with a message pointing at `none`; the slot survives only so
/// that the `SEXPR_*` variant indices (`crate::eval::interp`) — which are
/// burned into the island's IR and into compiled code — keep their values.
/// Renumbering them buys nothing and would invalidate every artifact.
fn sexpr_def() -> AdtDef {
    AdtDef {
        name: Path::root("sexpr"),
        params: vec![],
        variants: vec![
            // Index 0, and never constructible — see this function's doc
            // comment. `Checker` rejects it by name.
            Variant { name: "nil".to_string(), fields: vec![] },
            Variant { name: "i32".to_string(), fields: vec![Type::I32] },
            Variant { name: "f64".to_string(), fields: vec![Type::F64] },
            Variant { name: "char".to_string(), fields: vec![Type::Char] },
            Variant { name: "bool".to_string(), fields: vec![Type::Bool] },
            Variant { name: "sym".to_string(), fields: vec![Type::Symbol] },
            Variant { name: "str".to_string(), fields: vec![Type::Str] },
            Variant { name: "cons".to_string(), fields: vec![option_of(sexpr()), option_of(sexpr())] },
            // Appended after `cons` (not inserted alongside `int`/`float`)
            // so the existing `SEXPR_*` variant-index constants
            // (`crate::eval::interp`) stay valid.
            Variant { name: "bignum".to_string(), fields: vec![Type::Bignum] },
            Variant { name: "ratio".to_string(), fields: vec![Type::Ratio] },
            // A `::`-qualified path (e.g. `dep::head`), the reader's
            // `Value::Path` (`crate::mem::Value`) made matchable. Its single
            // field is a proper `Sexpr` list of `sym`s (its segments, in
            // written order) — the same shape a quoted `'(dep head)` list
            // already has — not a re-stringified `"dep::head"`: a segment
            // once split out of the reader's token stays a real `Symbol`,
            // never gets flattened back into text, and the list's own
            // length/`car`/`cdr` give the segment count and per-segment
            // access for free through the ordinary list-processing
            // machinery, instead of needing a second parse. Building this
            // list means allocating fresh `Cons` cells during pattern
            // matching itself, which none of the other variants'
            // `match_sexpr_ctor` arms need to (they only ever read
            // already-heap-resident data) — see that function's own doc
            // comment for the GC-rooting this requires.
            Variant { name: "path".to_string(), fields: vec![option_of(sexpr())] },
            // Index 11 onwards: the widths that used to be folded into `i32`
            // and `f64` above. Appended rather than inserted, because the
            // variant numbers are burned into the island's IR and into
            // compiled code, and because `Repr::field_kind` reads off the
            // same numbering (`Unit` was moved out to 100 to make room).
            //
            // Eight numeric variants where there were two. `Sexpr` is the
            // one place a value's type is not written down anywhere else, so
            // it is the one place every width has to be its own variant —
            // folding `u8` and `i32` into one `int` did not merely lose the
            // name, it let `(the u32 4000000000)` come back out as an `i32`
            // holding a number no `i32` can hold.
            Variant { name: "f32".to_string(), fields: vec![Type::F32] },
            Variant { name: "i8".to_string(), fields: vec![Type::I8] },
            Variant { name: "i16".to_string(), fields: vec![Type::I16] },
            Variant { name: "u8".to_string(), fields: vec![Type::U8] },
            Variant { name: "u16".to_string(), fields: vec![Type::U16] },
            Variant { name: "u32".to_string(), fields: vec![Type::U32] },
        ],
        assoc: sexpr_assoc(),
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new(),
    }
}

/// `eq`/`eql`: true CL identity on `Sexpr` — comparing the underlying
/// `mem::Value` directly (`crate::eval::interp`'s `eval_builtin_method`)
/// means two `Cons` cells or two `Str`s are equal only if they're the *same*
/// heap cell/slot (real identity), while immediate scalars (`Int`/`Char`/
/// `Bool`/`Sym`/...) compare by value — correct under `eq` for these since
/// there is no separate boxed representation to diverge from (`Sym` in
/// particular is correct since symbols are always interned: same name ->
/// same id). `eql` is registered as a plain alias of `eq` here: CL's `eql`
/// only adds same-type/value number and character comparisons beyond `eq`'s
/// identity, and every immediate scalar in this representation already
/// satisfies that trivially — the two predicates only diverge once a boxed
/// numeric representation exists (not yet the case here). Two separately
/// built `Str` `Sexpr`s with equal content are correctly *not* `eq`/`eql` —
/// see [`string_assoc`]'s own doc comment and `docs/cl-equivalence-catalog.md`'s
/// eq/eql/equal/equalp section — `equal`/`equalp` (the prelude's recursive
/// structural comparisons) special-case `Str` to compare content instead.
fn sexpr_assoc() -> BTreeMap<String, AssocFn> {
    let mut m = BTreeMap::new();
    let eq_fn = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![option_of(sexpr()), option_of(sexpr())], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    m.insert("eq".to_string(), eq_fn());
    m.insert("eql".to_string(), eq_fn());
    m
}

fn bool_assoc() -> BTreeMap<String, AssocFn> {
    let mut m = BTreeMap::new();
    // `bool` has exactly two values, both immediate — `eq`/`eql`/`equal`/
    // `equalp` can never diverge for it (no case-folding, no cross-type
    // comparison, no structure to recurse into), so all four alias the same
    // value comparison. See `docs/cl-equivalence-catalog.md`'s eq/eql/equal/
    // equalp section for why every one of these four is still registered
    // explicitly rather than leaving `eql`/`equal`/`equalp` undefined.
    let eq_fn = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bool, Type::Bool], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), eq_fn());
    }
    m.insert("print".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bool], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("println".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bool], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m
}

/// `symbol`'s method table: `eq`/`eql` only. Symbols are always interned, so
/// two `Symbol`s are `eq` iff they are the same interned id (same name) — the
/// runtime routes both to `sexpr_eq`/`sexpr_eql` since a `Symbol` value shares
/// the `Value::Symbol(id)` carrier of a `Sexpr::Sym`. `equal`/`equalp` are not
/// registered (there is no structure to recurse into beyond `eq`'s identity).
fn symbol_assoc() -> BTreeMap<String, AssocFn> {
    let mut m = BTreeMap::new();
    let eq_fn = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Symbol, Type::Symbol], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    m.insert("eq".to_string(), eq_fn());
    m.insert("eql".to_string(), eq_fn());
    m
}

fn sexpr() -> Type {
    Type::Named(Path::root("sexpr"), vec![])
}

/// `universal-time` (a prelude `defstruct`): a CL universal time split into
/// whole days since 1900-01-01 and seconds within that day. The return type
/// of `get-universal-time`/`file-modified-date` — see the comment at
/// `get-universal-time`'s registration for why a struct and not a number.
pub fn universal_time() -> Type {
    Type::Named(Path::root("universal-time"), vec![])
}

/// `internal-time` (a prelude `defstruct`): `get-internal-real-time`'s
/// reading, as whole seconds plus microseconds within that second.
/// [`universal_time`]'s monotonic counterpart.
pub fn internal_time() -> Type {
    Type::Named(Path::root("internal-time"), vec![])
}

fn hashtable_ty() -> Type {
    Type::Named(Path::root("hashtable"), vec![tvar("k"), tvar("v")])
}

fn option_of(t: Type) -> Type {
    Type::Named(Path::root("option"), vec![t])
}

/// One of [`builtin_error_defs`]' concrete error types, as a `Type` — the
/// `E` in the `Result<T, E>` returned by the fallible Rust builtin that
/// raises it (`parse-int`/`parse-float`/`read`/`eval`).
fn error_ty(name: &str) -> Type {
    Type::Named(Path::root(name), vec![])
}

fn result_of(t: Type, e: Type) -> Type {
    Type::Named(Path::root("result"), vec![t, e])
}

/// `HashTable<K, V>`: a builtin (Rust-implemented) mutable hash map, with no
/// constructors of its own (built via the static `new`, not pattern-matched).
/// All methods here are metadata only — there is no `defmethod` body to
/// check; the runtime implementation lives in `eval_builtin_method` in
/// `crate::eval::interp`.
fn hashtable_def() -> AdtDef {
    let mut assoc = BTreeMap::new();
    // The key methods require `K: Hash` — and they are in the *prelude*
    // now, where the bound is written as an ordinary `(where (Hash K))`.
    // That is what turned "unsupported key type" from a runtime panic into a
    // type error, and then into no restriction at all: the bound gives the
    // prelude's `get`/`set`/`remove` the key type's own `sxhash` and
    // `equals`, so any type that implements `Hash` can be a key.
    assoc.insert(
        "new".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![], ret: hashtable_ty(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: false, builtin: true },
    );
    // `get`/`set`/`remove` are **not here**. They are prelude `defmethod`s
    // (`(where (Hash K))`), written on the five bucket primitives below —
    // because looking a key up means hashing it and comparing it, and both of
    // those are the key type's own methods, written in typelisp. A builtin
    // that did the whole lookup would have to call back into the interpreter
    // to ask; a prelude method already is the interpreter, and compiles like
    // any other definition.
    //
    // The bucket primitives take the hash the caller computed. `i` is an
    // index the caller got from `bucket-count`; `bucket-put` also accepts the
    // count itself, which appends.
    // Not `public`: these are the layer under `get`/`set`/`remove`, not API.
    // The prelude reaches them because it is the root module, where the table
    // itself lives; nothing outside can, and nothing outside should.
    let bucket = |params: Vec<Type>, ret: Type| AssocFn {
        sig: FnSig { type_params: vec![], rest: None, params, ret, public: false, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() },
        instance: true,
        builtin: true,
    };
    assoc.insert("bucket-count".to_string(), bucket(vec![hashtable_ty(), Type::I32], Type::I32));
    assoc.insert("bucket-key".to_string(), bucket(vec![hashtable_ty(), Type::I32, Type::I32], tvar("k")));
    assoc.insert("bucket-value".to_string(), bucket(vec![hashtable_ty(), Type::I32, Type::I32], tvar("v")));
    assoc.insert(
        "bucket-put".to_string(),
        bucket(vec![hashtable_ty(), Type::I32, Type::I32, tvar("k"), tvar("v")], Type::Unit),
    );
    assoc.insert("bucket-delete".to_string(), bucket(vec![hashtable_ty(), Type::I32, Type::I32], Type::Unit));
    assoc.insert(
        "count".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::I32, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "clear".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    // `keys`/`values`/`entries`: a Rust `HashMap` has no stable, resumable cursor the way `Vector<T>`'s
    // own index-based iterator does, so each call snapshots the table's
    // current contents into a fresh `Vector` rather than exposing a live
    // cursor — `eval_builtin_method`'s `"hashtable"` arm builds these
    // (`hashtable_keys`/`hashtable_values`/`hashtable_entries`). `entries`'
    // element type is `cons-cell<K,V>` (`prelude.rs`'s generic `car`/`cdr`
    // product — there's no built-in tuple syntax, so this mirrors Lisp's
    // own two-value-storage convention rather than an arbitrary
    // `first`/`second` struct), which `hashtable-iter<K,V>` (also
    // `prelude.rs`) walks to give `HashTable<K,V>` an `Iter` impl on top of
    // this snapshot, the same way `vector-iter<T>` walks `Vector<T>`.
    assoc.insert(
        "keys".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::Named(Path::root("vector"), vec![tvar("k")]), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "values".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::Named(Path::root("vector"), vec![tvar("v")]), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "entries".to_string(),
        AssocFn {
            sig: FnSig {
                type_params: vec![],
                rest: None,
                params: vec![hashtable_ty()],
                ret: Type::Named(Path::root("vector"), vec![Type::Named(Path::root("cons-cell"), vec![tvar("k"), tvar("v")])]),
                public: true,
                builtin: true,
                bounds: BTreeMap::new(),
                optionals: Vec::new(),
                keys: Vec::new(),
            },
            instance: true,
            builtin: true,
        },
    );
    AdtDef {
        name: Path::root("hashtable"),
        params: vec!["k".to_string(), "v".to_string()],
        variants: vec![],
        assoc,
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new(),
    }
}

fn vector_ty() -> Type {
    Type::Named(Path::root("vector"), vec![tvar("t")])
}

/// `Vector<T>`: a builtin growable sequence. Reuses the same boxed-struct
/// representation `defstruct` instances get (`BoxedObj::Struct`) rather than
/// a dedicated one of its own — a `Vector<T>`'s fields are simply treated as
/// variable-length instead of the fixed, name-indexed layout a `defstruct`'s
/// fields have (see `eval_builtin_method`'s `"vector"` arm).
/// All methods here are metadata only — there is no `defmethod` body to
/// check; the runtime implementation lives in `eval_builtin_method` in
/// `crate::eval::interp`.
fn vector_def() -> AdtDef {
    let mut assoc = BTreeMap::new();
    assoc.insert(
        "new".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![], ret: vector_ty(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: false, builtin: true },
    );
    assoc.insert(
        "push".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), tvar("t")], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "get".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), Type::I32], ret: tvar("t"), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "set".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), Type::I32, tvar("t")], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "len".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty()], ret: Type::I32, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "pop".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty()], ret: option_of(tvar("t")), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    AdtDef {
        name: Path::root("vector"),
        params: vec!["t".to_string()],
        variants: vec![],
        assoc,
        public: true,
        builtin: true,
        kind: AdtKind::Struct,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new(),
    }
}

fn scope_ty() -> Type {
    Type::Named(Path::root("scope"), vec![tvar("v")])
}

/// `Scope<V>`: a builtin (Rust-implemented) stack of `String`-keyed frames —
/// the (typelisp-hosted) compiler body's (`src/compiler.rs`) replacement for
/// a bare `HashTable` as `env`/`fn-env`, modeling the "list of scopes" name
/// resolution `labels`/`let` need (see that module's doc comment). No
/// constructors of its own (built via the static `new`); all methods here
/// are metadata only — the runtime implementation lives in
/// `eval_builtin_method` in `crate::eval::interp`.
fn scope_def() -> AdtDef {
    let mut assoc = BTreeMap::new();
    assoc.insert(
        "new".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![], ret: scope_ty(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: false, builtin: true },
    );
    assoc.insert(
        // Shares every existing frame (by reference, not by copying their
        // contents) plus pushes one fresh empty frame on top — the single
        // operation a `labels` def's own new lexical scope needs to start
        // from every enclosing scope's frames (see `compile-labels`'s doc
        // comment for why a *fresh* top frame, not the shared ones
        // themselves, must receive that def's own parameter bindings).
        "clone-frames".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty()], ret: scope_ty(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "push-frame".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty()], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "pop-frame".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty()], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "get".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty(), Type::Str], ret: option_of(tvar("v")), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "set".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty(), Type::Str, tvar("v")], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() },
            instance: true,
            builtin: true,
        },
    );
    AdtDef {
        name: Path::root("scope"),
        params: vec!["v".to_string()],
        variants: vec![],
        assoc,
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new(),
    }
}


fn llvm_module_ty() -> Type {
    Type::Named(Path::root("llvm-module"), vec![])
}

fn llvm_function_ty() -> Type {
    Type::Named(Path::root("llvm-function"), vec![])
}

fn llvm_basic_block_ty() -> Type {
    Type::Named(Path::root("llvm-basic-block"), vec![])
}

fn llvm_builder_ty() -> Type {
    Type::Named(Path::root("llvm-builder"), vec![])
}

fn llvm_value_ty() -> Type {
    Type::Named(Path::root("llvm-value"), vec![])
}

fn assoc_fn(params: Vec<Type>, ret: Type, instance: bool) -> AssocFn {
    AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance, builtin: true }
}

/// An in-progress LLVM module. `add-function` always declares a function
/// under one fixed C ABI — `i64 name(i64* args, i32 argc)` — regardless of
/// the typelisp-level function's actual arity; `llvm-builder::load-arg`
/// reads a logical parameter back out of that array. Fixing the ABI this
/// way (rather than generating a distinct LLVM signature per arity) is what
/// lets `compile::CompiledFn` call *any* compiled function through one Rust
/// function-pointer type — other return types are a later phase's concern.
pub fn llvm_module_def() -> AdtDef {
    let mut assoc = BTreeMap::new();
    assoc.insert("create".to_string(), assoc_fn(vec![Type::Str], llvm_module_ty(), false));
    assoc.insert("add-function".to_string(), assoc_fn(vec![llvm_module_ty(), Type::Str], llvm_function_ty(), true));
    assoc.insert("verify".to_string(), assoc_fn(vec![llvm_module_ty()], Type::Bool, true));
    assoc.insert("to-string".to_string(), assoc_fn(vec![llvm_module_ty()], Type::Str, true));
    // `get-function`: look up an already-declared `llvm-function` by name in
    // this module — the core primitive direct calls (self-recursion,
    // `labels`-sibling calls, top-level `defun`-to-`defun` calls) are built
    // on, since every callee in those cases is something this same module
    // already added via `add-function` before the caller's body is compiled.
    assoc.insert("get-function".to_string(), assoc_fn(vec![llvm_module_ty(), Type::Str], llvm_function_ty(), true));
    // `add-function-with-env`: declares a function under the *extended*
    // ABI a `labels` block with outer-scope captures needs — `i64 name(i64*
    // args, i32 argc, i64* env, i32 env_len)` — used instead of
    // `add-function` exactly when that block's shared captured-name list
    // (`compile::core_freevars::free_vars`) is non-empty. Every sibling in
    // such a block shares this one extended signature, even ones whose own
    // body doesn't reference every captured name (see `compiler.rs`'s
    // `compile-labels` doc comment for why captures aren't computed
    // per-sibling).
    assoc.insert("add-function-with-env".to_string(), assoc_fn(vec![llvm_module_ty(), Type::Str], llvm_function_ty(), true));
    AdtDef { name: Path::root("llvm-module"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new() }
}

/// A declared LLVM function (a `Module::add-function` result).
pub fn llvm_function_def() -> AdtDef {
    let mut assoc = BTreeMap::new();
    assoc.insert("append-block".to_string(), assoc_fn(vec![llvm_function_ty(), Type::Str], llvm_basic_block_ty(), true));
    AdtDef { name: Path::root("llvm-function"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new() }
}

/// An LLVM basic block. No methods of its own yet (Phase 0) — produced by
/// `llvm-function::append-block`, consumed by `llvm-builder::position-at-end`.
fn llvm_basic_block_def() -> AdtDef {
    AdtDef { name: Path::root("llvm-basic-block"), params: vec![], variants: vec![], assoc: BTreeMap::new(), public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new() }
}

/// An IR builder. `load-arg` reads logical parameter `index` out of a
/// function's fixed-ABI argument array (see `llvm_module_def`'s doc
/// comment); `build-add`/`build-sub`/`build-mul` cover Phase 1's scalar
/// arithmetic (comparisons, which return `bool` rather than `i64`, are a
/// later phase's concern — keeping every builtin here `i64`-in-`i64`-out
/// for now).
pub fn llvm_builder_def() -> AdtDef {
    let mut assoc = BTreeMap::new();
    assoc.insert("create".to_string(), assoc_fn(vec![], llvm_builder_ty(), false));
    assoc.insert("position-at-end".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_basic_block_ty()], Type::Unit, true));
    // `const-word`: an LLVM 64-bit constant. The *generated* code's word is 64
    // bits whatever the source language's integer types are, so this builtin
    // outlives `i64` (the name it used to carry, which claimed a source type
    // that no longer exists). Its argument is an `i32` because every caller
    // passes something small — a tag mask, a slot index, `0`/`1`/`-1`; the two
    // places that need a full-width constant (`compile-int`/`compile-float`)
    // assemble it from two 32-bit halves in LLVM instead.
    assoc.insert("const-word".to_string(), assoc_fn(vec![llvm_builder_ty(), Type::I32], llvm_value_ty(), true));
    assoc.insert("build-ret".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty()], Type::Unit, true));
    assoc.insert("load-arg".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_function_ty(), Type::I32], llvm_value_ty(), true));
    assoc.insert("build-add".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    assoc.insert("build-sub".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    assoc.insert("build-mul".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // Stage 2 of the Sexpr-representation plan (`docs/implementation-log.md`): bitwise
    // primitives for packing/unpacking the tagged `i64` representation of a
    // compiled `Sexpr` value (3 low tag bits + payload — see that doc's
    // tag table). Deliberately generic, type-agnostic `i64`-in-`i64`-out
    // ops, the same as `build-add`/`build-sub`/`build-mul` — the actual
    // tagging/untagging logic is plain typelisp in `compiler.rs`, built out
    // of these, the same way `compile-if`/`compile-loop` are built out of
    // `build-icmp-*`/`build-cond-br`.
    assoc.insert("build-and".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    assoc.insert("build-or".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // `xor` (CL `logxor`, plus `lognot`'s `(xor n -1)` — a bare LLVM
    // instruction like `build-and`/`build-or`).
    assoc.insert("build-xor".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // `(select cond then else)`: `max`/`min` (integers, `bignum`, `ratio`) all
    // lower to this — see `llvm_builder_build_select`'s doc comment.
    assoc.insert(
        "build-select".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true),
    );
    assoc.insert("build-shl".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // Logical (unsigned) vs. arithmetic (sign-extending) right shift: the
    // tagged representation's payload bits must never be sign-extended back
    // in when shifting a Cons/Symbol/Str/Path pointer or index out from
    // under its tag (`build-lshr`), but a genuine signed `Sexpr::i32`
    // fixnum's sign bit must survive untagging (`build-ashr`) or negative
    // integers would come back corrupted.
    assoc.insert("build-lshr".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    assoc.insert("build-ashr".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // `f64` arithmetic/comparison (`eval_llvm_builtin_method`'s
    // `llvm_builder_build_float_op`/`_fcmp`): both operands and the arithmetic
    // result are the *same* `i64`-carried representation every other
    // `llvm-value` uses (a compiled `f64`'s raw `f64::to_bits` pattern) — the
    // `i64`<->`double` `bitcast`ing is entirely internal to each op, so these
    // are ordinary `i64`-in/`i64`-out builtins like `build-add`, and
    // `compile-assoc`'s f64 branch composes them exactly like the int one
    // composes `build-add`/`build-icmp-*`. A comparison yields a `0`/`1`
    // `i64` (the `bool` representation), matching `build-icmp-*`.
    for name in ["build-fadd", "build-fsub", "build-fmul", "build-fdiv", "build-frem"] {
        assoc.insert(name.to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    }
    for name in ["build-fcmp-lt", "build-fcmp-le", "build-fcmp-gt", "build-fcmp-ge", "build-fcmp-eq", "build-fcmp-ne"] {
        assoc.insert(name.to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    }
    // `f64` transcendentals (`float-native-method?`'s `sqrt`/`floor`/`ceiling`/
    // `round`/`truncate`): each lowers to the matching LLVM intrinsic
    // (`llvm.sqrt.f64`/...) declared on demand in `module` — unlike
    // `build-fadd`/..., these need the module to look the intrinsic
    // declaration up in (`eval_llvm_builtin_method`'s
    // `llvm_builder_build_float_unary_intrinsic`), so they take `llvm-module`
    // as a second argument the same way `build-make-closure`/
    // `build-closure-apply` do.
    for name in ["build-fsqrt", "build-ffloor", "build-fceil", "build-fround", "build-ftrunc", "build-fsin", "build-fcos", "build-fexp", "build-flog"] {
        assoc.insert(name.to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_module_ty(), llvm_value_ty()], llvm_value_ty(), true));
    }
    // `expt` (`f64,f64->f64`): the binary counterpart of the unary
    // transcendentals above, `llvm.pow.f64`. `max`/`min` are the same shape
    // (`llvm.maxnum.f64`/`llvm.minnum.f64`).
    for name in ["build-fpow", "build-fmaxnum", "build-fminnum"] {
        assoc.insert(name.to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_module_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    }
    // `float->int` (`f64->i32`, narrowing, truncating toward zero): a single
    // `fptosi` instruction, no heap allocation and no module lookup needed —
    // unlike `float->bignum`/`float->ratio`, which stay non-native (see
    // `float-native-method?`'s doc comment).
    assoc.insert("build-fptosi".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_module_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // `int->float` (widening, always exact for the widths this language has):
    // `sitofp` to `double`, then `bitcast` back to the raw word a compiled
    // `f64` is carried in. No module parameter — unlike `fptosi`'s saturating
    // form, `sitofp` is a plain instruction with no intrinsic to overload and
    // no input it is undefined on.
    assoc.insert("build-sitofp".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // `float->f32` (and every `f32` arithmetic result): round the operand to
    // binary32 and widen it straight back. Two instructions, no intrinsic and
    // no module lookup — `fptrunc` then `fpext`, which is exactly "the
    // nearest binary32 value" written in IR.
    assoc.insert("build-fround32".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // `alloca-args`/`store-arg`/`build-call`: building a direct call to an
    // already-declared function (`get-function`'s result). `alloca-args`
    // stack-allocates a fresh `[count x i64]` array (mirroring the fixed-ABI
    // argument array every compiled function already expects) and returns
    // its decayed element pointer; `store-arg` fills in one slot at a time
    // (the same loop shape `compiler.rs`'s `bind-params` already uses for
    // reading arguments, just writing instead); `build-call` then calls the
    // target with that pointer plus a literal arg count, exactly matching
    // `CompiledSignature`'s `i64 fn(i64* args, i32 argc)` shape — so calling
    // a compiled function looks the same whether the call originates from
    // Rust (`compile::CompiledFn::call`) or from another compiled function.
    assoc.insert("alloca-args".to_string(), assoc_fn(vec![llvm_builder_ty(), Type::I32], llvm_value_ty(), true));
    assoc.insert("store-arg".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), Type::I32, llvm_value_ty()], Type::Unit, true));
    assoc.insert("build-call".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_function_ty(), llvm_value_ty(), Type::I32], llvm_value_ty(), true));
    // `load-env`/`build-call-with-env`: the captures counterpart of
    // `load-arg`/`build-call`, for functions declared via
    // `add-function-with-env`. `load-env` reads logical captured slot
    // `index` out of the function's env array (`load-arg`'s GEP pattern,
    // against the env parameter instead of the args one); `build-call-with-env`
    // calls a target under the extended ABI, passing both the args array
    // (as `build-call` already does) and an env array built the same way
    // (`alloca-args`/`store-arg`, just filled with captured values instead
    // of call arguments).
    assoc.insert("load-env".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_function_ty(), Type::I32], llvm_value_ty(), true));
    assoc.insert(
        "build-call-with-env".to_string(),
        assoc_fn(
            vec![llvm_builder_ty(), llvm_function_ty(), llvm_value_ty(), Type::I32, llvm_value_ty(), Type::I32],
            llvm_value_ty(),
            true,
        ),
    );
    // `build-make-closure`/`build-closure-apply`: `BoxedObj::CompiledClosure`
    // (`typelisp-mem`) — the GC-heap runtime representation of a `lambda`
    // value that *escapes* its defining function (rather than being called
    // directly while statically known, like every `apply`/`call` site
    // above) — labels/closures Stage 4, flipped off the raw `malloc`'d,
    // reference-counted `ClosureBox` it used to be at the closure-
    // representation unification (see that variant's own doc comment). The
    // resulting value is a tagged `Sexpr` reference, exactly like every
    // other `BoxedObj` constructor's result (`rt_struct_new`, `rt_data_new`,
    // ...) — no longer a bare untagged `i64` the way the old `ClosureBox`
    // pointer was.
    //
    // `build-make-closure` takes the already-compiled `target` (declared
    // under `add-function-with-env`'s ABI — *every* closure-boxed function
    // uses that ABI, capturing or not, so `build-closure-apply` never has to
    // decide which ABI to call through), an env array built the same way a
    // direct capturing call already builds one (`alloca-args`/`store-arg`/
    // `compile-env-args`), and `sexpr_mask` (a bitmask, one bit per captured
    // slot, computed entirely at compile time by `compiler.rs`'s
    // `compute-sexpr-mask`) marking which captured slots are tagged `Sexpr`
    // values for the GC mark phase to trace — see
    // `interp::llvm_builder_build_make_closure`'s doc comment for how it
    // gets copied into the new box via `rt_closure_new`.
    // `build-closure-apply` is `build-call-with-env`'s indirect counterpart:
    // the callee isn't a statically-known `llvm-function` here, just a
    // tagged closure reference, so it reads `fn_ptr`/`env_len`/each captured
    // slot back out via `rt_closure_fnptr`/`rt_closure_env_len`/
    // `rt_closure_env_get` at runtime and calls through
    // `build_indirect_call` instead — see that builtin's own doc comment.
    // Neither builtin needs a retain/release counterpart anymore: the box's
    // lifetime is the GC's business now, including a `labels` cycle between
    // two mutually-capturing siblings, which mark-sweep reclaims like any
    // other unreachable structure.
    assoc.insert(
        "build-make-closure".to_string(),
        assoc_fn(
            vec![llvm_builder_ty(), llvm_module_ty(), llvm_function_ty(), llvm_value_ty(), Type::I32, Type::I32, Type::I32],
            llvm_value_ty(),
            true,
        ),
    );
    assoc.insert(
        "build-closure-apply".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_module_ty(), llvm_value_ty(), llvm_value_ty(), Type::I32], llvm_value_ty(), true),
    );
    // The generic-pointer read counterpart of `store-arg` — see
    // `interp::llvm_builder_load_raw`'s doc comment.
    assoc.insert(
        "load-raw".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), Type::I32], llvm_value_ty(), true),
    );
    // `build-icmp-lt`/`-le`/`-gt`/`-ge`/`-eq`/`-ne`: `i64` comparisons —
    // `compiler.rs`'s `compile-assoc` dispatches `<`/`<=`/`>`/`>=`/(`=`,`eq`)/`/=`
    // to these (`if`/comparisons work, labels/closures Stage 5). Each widens
    // the underlying `icmp` instruction's `i1` result back to `i64` (0/1) via
    // `build_int_z_extend`, matching every other builtin's "every compiled
    // value is a plain i64" convention (see `llvm_module_def`'s doc comment)
    // — keeping a `bool` result indistinguishable in representation from any
    // other `i64` is what lets `compile-if`'s `build-cond-br` (just below)
    // accept either one uniformly.
    for name in ["build-icmp-lt", "build-icmp-le", "build-icmp-gt", "build-icmp-ge", "build-icmp-eq", "build-icmp-ne"] {
        assoc.insert(name.to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    }
    // `build-cond-br`: branches to `then`/`else` depending on whether `cond`
    // (an ordinary `i64`-valued `llvm-value`, typically a `build-icmp-*`
    // result or a `bool` literal) is zero — `compile-if`'s primitive.
    assoc.insert(
        "build-cond-br".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_basic_block_ty(), llvm_basic_block_ty()], Type::Unit, true),
    );
    // `build-br`: an unconditional branch — `compile-if`'s then/else arms use
    // this to join back at the merge block.
    assoc.insert("build-br".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_basic_block_ty()], Type::Unit, true));
    // `block-terminated?` (`loop`/`break`/`return`): whether the builder's
    // *current* insertion block already ends in a terminator instruction —
    // a host-level query (returns a real `Bool`, not a compiled `i64` the
    // way every `build-*` primitive above does), the same flavor as
    // `llvm-module::verify`. `compile-if`'s branches and `compile-loop-body`'s
    // statement sequencing both need this: a `break`/`return` reached
    // directly (or via a taken `if` branch) already ends the current block
    // with its own unconditional branch to the loop's exit block, and LLVM
    // allows only one terminator per block — anything that would otherwise
    // unconditionally append more instructions after compiling a sub-form
    // (`compile-if`'s store-then-branch-to-merge, `compile-loop-body`'s next
    // statement) must check this first and skip emitting if it's already
    // true, or the resulting IR is malformed.
    assoc.insert("block-terminated?".to_string(), assoc_fn(vec![llvm_builder_ty()], Type::Bool, true));
    // Stage 6 of the Sexpr-representation plan (`docs/implementation-log.md`): generic
    // malloc/free + pointer conversion — the `ClosureBox` generalization a
    // general ADT box (`Option`/`Result`/`defstruct`) needs, without baking
    // in `ClosureBox`'s own fixed header layout. `build-malloc`/`build-free`
    // mirror `alloca-args`'s stack-allocation shape but on the heap;
    // `build-int-to-ptr`/`build-ptr-to-int` let `compiler.rs` cross between
    // "every compiled value is a plain i64" (`build-ret`/`store-arg`'s value
    // operand/`build-icmp-*`/...) and the pointer `load-raw`/`store-arg`'s
    // *array* operand already expects, exactly the conversion
    // `build-make-closure`/`build-closure-apply` already do internally for
    // `ClosureBox`, just exposed generically here.
    assoc.insert("build-malloc".to_string(), assoc_fn(vec![llvm_builder_ty(), Type::I32], llvm_value_ty(), true));
    assoc.insert("build-free".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty()], Type::Unit, true));
    assoc.insert("build-int-to-ptr".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty()], llvm_value_ty(), true));
    assoc.insert("build-ptr-to-int".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // `build-fn-address`: a declared function's address as a plain `i64` —
    // `build-ptr-to-int` applied to an `llvm-function` instead of to a
    // `build-malloc`'d block. `compile-call` inside a `catch`/`unwind-protect`
    // region needs it: there the call goes through `rt_protected_call`, which
    // takes its target as an ordinary argument rather than being the callee of
    // a `call` instruction (see `typelisp-rt`'s catch/throw section for why
    // the Rust frame that catches the unwind has to sit at the call).
    assoc.insert("build-fn-address".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_function_ty()], llvm_value_ty(), true));
    AdtDef { name: Path::root("llvm-builder"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new() }
}

/// An LLVM SSA value (e.g. a constant). No methods of its own yet — produced
/// by `llvm-builder::const-word`, consumed by `llvm-builder::build-ret`.
fn llvm_value_def() -> AdtDef {
    AdtDef { name: Path::root("llvm-value"), params: vec![], variants: vec![], assoc: BTreeMap::new(), public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: BTreeMap::new() }
}

/// Built-in `String` instance methods ([cl-equivalence-catalog.md](../../../docs/dev/cl-equivalence-catalog.md)
/// §2.2 d). All char/index arguments and `length` count Unicode scalar values
/// (`char`s), not bytes. `ref`/`substring` panic on an out-of-range index —
/// the type system can't express the bound, the same precedent as
/// `car`/`cdr` on a non-`Cons` `Sexpr`. `upcase`/`downcase` are ASCII-only
/// (`str::to_ascii_uppercase`/`lowercase`), avoiding Unicode case mappings
/// that can change a string's length (e.g. German `ß` -> `SS`).
///
/// `eq`/`eql`/`equal`/`equalp` (see `docs/cl-equivalence-catalog.md`'s
/// eq/eql/equal/equalp section for the full rationale): `eq`/`eql` are true
/// CL identity (`Rc::ptr_eq` on a string's underlying `Rc<str>` —
/// see `Heap`'s string table for why `Rc`, not a plain `String`, is
/// what makes identity meaningful here at all); `eql` doesn't add anything
/// beyond `eq` for strings in real CL either (it only extends numbers/
/// characters), so it's a plain alias. Content comparison — what a naive
/// reading of "string equality" usually means — is `equal` (case-sensitive)
/// and `equalp` (case-insensitive, ASCII-only for the same reason
/// `upcase`/`downcase` are) instead; two separately-built equal-content
/// `Str`s are correctly *not* `eq`/`eql`.
fn string_assoc() -> BTreeMap<String, AssocFn> {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let mut m = BTreeMap::new();
    m.insert("upcase".to_string(), method(vec![Type::Str], Type::Str));
    m.insert("downcase".to_string(), method(vec![Type::Str], Type::Str));
    m.insert("length".to_string(), method(vec![Type::Str], Type::I32));
    m.insert("ref".to_string(), method(vec![Type::Str, Type::I32], Type::Char));
    m.insert("substring".to_string(), method(vec![Type::Str, Type::I32, Type::I32], Type::Str));
    m.insert("append".to_string(), method(vec![Type::Str, Type::Str], Type::Str));
    m.insert("lt".to_string(), method(vec![Type::Str, Type::Str], Type::Bool));
    // Lexicographic comparison operators, overloaded on `string` the same way
    // `int_assoc` overloads them on numbers (dispatched by receiver type — see
    // the prelude's `impl Ord string`). `lt` is kept as the older CL-catalog
    // primitive `<` now supersedes.
    for name in ["<", "<=", ">", ">="] {
        m.insert(name.to_string(), method(vec![Type::Str, Type::Str], Type::Bool));
    }
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), method(vec![Type::Str, Type::Str], Type::Bool));
    }
    // `print`/`println`: write the string as-is to stdout, with/without a
    // trailing newline (`crate::eval::interp::eval_builtin_method`'s
    // `"string"` arm). No quoting — unlike `format_value`'s reader-syntax
    // output for the REPL, this is for a program's own user-facing text.
    m.insert("print".to_string(), method(vec![Type::Str], Type::Unit));
    m.insert("println".to_string(), method(vec![Type::Str], Type::Unit));
    m
}

/// Built-in `char` instance methods (same catalog section as
/// [`string_assoc`]). `upcase`/`downcase` are ASCII-only, for the same
/// reason as `string_assoc`'s (a non-ASCII char's case mapping isn't
/// necessarily a single char). `alphap`/`digitp` classify ASCII letters/
/// digits only (CL's `alpha-char-p`/`digit-char-p` without a radix).
///
/// `eq`/`eql`/`equal` all coincide (plain value comparison) — a `char` is an
/// immediate scalar here, so there's no separate identity to diverge from,
/// and CL's `equal` on characters is defined to be the same as `eql` anyway.
/// `equalp` is the one that differs for real: CL requires case-insensitive
/// comparison there (`(equalp #\A #\a)` is true).
fn char_assoc() -> BTreeMap<String, AssocFn> {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let mut m = BTreeMap::new();
    m.insert("upcase".to_string(), method(vec![Type::Char], Type::Char));
    m.insert("downcase".to_string(), method(vec![Type::Char], Type::Char));
    m.insert("lt".to_string(), method(vec![Type::Char, Type::Char], Type::Bool));
    // Code-point comparison operators, overloaded on `char` like the numeric
    // ones (a compiled `char` is a raw `i64` code point, so these lower to the
    // same integer `icmp`s — see `compiler.rs`'s `char-native-method?`). `lt`
    // is kept as the older CL-catalog primitive `<` now supersedes.
    for name in ["<", "<=", ">", ">="] {
        m.insert(name.to_string(), method(vec![Type::Char, Type::Char], Type::Bool));
    }
    m.insert("alphap".to_string(), method(vec![Type::Char], Type::Bool));
    m.insert("digitp".to_string(), method(vec![Type::Char], Type::Bool));
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), method(vec![Type::Char, Type::Char], Type::Bool));
    }
    // `char->int`: a `char`'s Unicode scalar value as `i32` (`docs/
    // language-design.md` §4.1's planned conversion catalog) — the other
    // half is `int_assoc`'s `int->char`.
    // `char->string`: the one-character string. CL reaches this through
    // `string`, which is a designator-taking function this language has no
    // room for; the explicit name says which direction the conversion goes.
    m.insert("char->string".to_string(), method(vec![Type::Char], Type::Str));
    m.insert("char->int".to_string(), method(vec![Type::Char], Type::I32));
    m.insert("print".to_string(), method(vec![Type::Char], Type::Unit));
    m.insert("println".to_string(), method(vec![Type::Char], Type::Unit));
    m
}

/// Built-in arithmetic/comparison instance methods for an integer type
/// (one call per name in `types::INT_TYPE_NAMES`): `+ - * / mod` (binary, same-type) and `< <= > >= = /=`
/// (binary, `Bool`-valued). `mod` is floored (CL, sign of the divisor);
/// `/`/`mod` panic on a zero divisor at runtime
/// (`crate::eval::interp::eval_int_builtin`) — the type system can't express
/// "nonzero", the same precedent as `car`/`cdr` on a non-`Cons` `Sexpr`.
/// Shared by both integer widths since the operation set and panic policy are
/// identical; only the receiver/param `Type` differs. The rest of CL's integer
/// catalog (`rem`/`abs`/`signum`/`gcd`/`lcm`) is defined in `prelude.rs` as
/// ordinary typelisp methods (so it compiles via the normal path); only the
/// operations needing a genuinely primitive machine op live here.
fn int_assoc(ty: Type) -> BTreeMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: ty.clone(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let unary = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: ty.clone(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let mut m = BTreeMap::new();
    // `max`/`min` (CL) and the bitwise operators (`logand`/`logior`/`logxor`)
    // are same-type binary ops like `+`/`-`/`*`. Every one of them works in
    // the receiver type's own width and signedness — the value in hand is
    // always the number its type names, sign- or zero-extended into the
    // 64-bit carrier (see `types::normalize_int` and `eval_int_builtin`'s
    // doc comment).
    for op in ["+", "-", "*", "/", "mod", "max", "min", "logand", "logior", "logxor"] {
        m.insert(op.to_string(), binop());
    }
    // `ash` (arithmetic shift, positive = left) is *not* one of them, and
    // used to be: its second operand is a shift **distance**, not another
    // value of the receiver's type, and typing it as the latter made right
    // shift unwritable on every unsigned width — `(ash (the u8 x) -3)` was
    // rejected because `-3` is out of `u8`'s range, and no other spelling of
    // "shift right" existed. The distance is an `i32` for the same reason
    // CL's `(ash integer count)` lets `count` be any integer: it measures
    // bits, so the receiver's width and signedness have nothing to say about
    // it. Nothing below the checker changes — the compiled tier passes the
    // distance as a raw word and `rt_int_ash` already read it as a signed
    // count, as did `eval_int_builtin`.
    m.insert("ash".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), Type::I32], ret: ty.clone(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
    }
    // `logtest`: a bitwise `Bool`-valued predicate on two values of the
    // receiver's type (`(logtest a b)`).
    m.insert("logtest".to_string(), cmp());
    // `logbitp` is *not* one of those, and used to be registered as if it
    // were. Its index is a bit **position**, the same kind of quantity as
    // `ash`'s distance, so it is an `i32` at every width. The argument order
    // changed with it: CL writes `(logbitp index integer)`, which put the
    // *index* in the receiver slot — and since a method is keyed by
    // `(receiver type, name)`, an `i32` index could carry only one signature,
    // so the integer could never be anything but an `i32` either. The value
    // being asked about comes first here, as it does in every other bit
    // operation this file registers (`(logand a b)`, `(ash x count)`,
    // `(lognot x)`); `ldb` and its family moved the same way and for the same
    // reason (`prelude.rs`'s byte-specifier section).
    m.insert("logbitp".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), Type::I32], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    // `lognot` (bitwise complement), `logcount` (population count of a
    // nonnegative integer, or of the zero bits of a negative one — CL
    // §12.10's "infinite precision" reading), `integer-length` (bits needed,
    // excluding sign) are unary, same-type.
    for op in ["lognot", "logcount", "integer-length"] {
        m.insert(op.to_string(), unary());
    }
    // `eq`/`eql`/`equal`/`equalp` are all aliases for `=` here — a fixnum has
    // no separate identity to diverge from value, `eql` doesn't add anything
    // for same-type numbers beyond `eq` in real CL either, `equal` on
    // numbers is defined to be the same as `eql`, and `equalp`'s one real
    // difference (cross-*type* numeric comparison, e.g. `i32` vs `f64`) can
    // never be reached here — the checker requires both operands to share
    // this exact `ty`, so there is no other type for it to differ from.
    // Registered under all four names anyway so `case`/`equal`/`equalp`
    // can call any of them uniformly across every type (see
    // `docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp section).
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), cmp());
    }
    // `int->float`: widening numeric conversion (`docs/language-design.md`
    // §4.1's planned conversion catalog) — registered for every integer
    // width, since each gets its own `int_assoc` call. This is also what
    // `equalp`'s `Sexpr` `Int`<->`Float` cross-type comparison
    // (`prelude.rs`) needed and previously lacked (see
    // `docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp section).
    m.insert("int->float".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::F64, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    // `int->char`: the other half of `char_assoc`'s `char->int` — a Unicode
    // scalar value back to `char`. Panics at runtime on a value outside the
    // valid range (surrogates, or past `U+10FFFF`) — the type system can't
    // express "valid scalar value", same precedent as `car`/`cdr` on a
    // non-`Cons` `Sexpr` or division by zero.
    m.insert("int->char".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Char, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    // `try-int->char`: the `Option`-returning counterpart of `int->char`,
    // for `(try-as char n)` (`Checker::check_as`) — same Unicode-scalar-
    // value validity check, `None` instead of a panic on failure.
    m.insert(
        "try-int->char".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: option_of(Type::Char), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    // `int->bignum`/`int->ratio`: widening conversions into the two
    // arbitrary-precision types (`docs/cl-equivalence-catalog.md`'s planned
    // conversion catalog, extended for `bignum`/`ratio`) — always exact,
    // unlike `bignum->int`/`ratio->int`'s narrowing counterparts.
    m.insert("int->bignum".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Bignum, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("int->ratio".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Ratio, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    // `int->i8`/`int->u32`/... and their `try-` counterparts: a cast between
    // two integer *widths*, which is a real conversion now that a type name
    // means its width and its signedness (`types::int_width_signed`) rather
    // than labelling a shared 64-bit one. `int->W` truncates — that is what a
    // width cast means, the same as Rust's `as` — and `try-int->W` answers
    // `none` instead when the value does not fit, which is the question
    // `try-as` asks. `Checker::check_as` picks between them.
    //
    // One pair per *target*, on every integer receiver, because the target is
    // the only thing a conversion's name can carry: there is no way to spell
    // "narrow to whatever width the context wants" as one method.
    for target in crate::types::INT_TYPE_NAMES {
        let to = crate::types::primitive_by_name(target).expect("INT_TYPE_NAMES names a primitive type");
        m.insert(
            format!("int->{}", target),
            AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: to.clone(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
        );
        m.insert(
            format!("try-int->{}", target),
            AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: option_of(to), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
        );
    }
    m.insert("print".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("println".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m
}

/// Built-in arithmetic/comparison/transcendental instance methods for `f64`:
/// `+ - * /` (IEEE-754 — a zero divisor yields `inf`/`NaN`, no panic), the
/// comparisons, `expt` (binary, `f64::powf`) and the unary rounding/root
/// family `sqrt`/`floor`/`ceiling`/`round`/`truncate`. `mod`/`rem` and
/// `abs`/`signum` are defined in `prelude.rs` as typelisp methods (built from
/// these primitives — `mod`/`rem` via `a - b*floor|truncate(a/b)`), so they
/// compile via the normal path.
fn float_assoc(ty: Type) -> BTreeMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: ty.clone(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let unary = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: ty.clone(), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let mut m = BTreeMap::new();
    for op in ["+", "-", "*", "/", "max", "min"] {
        m.insert(op.to_string(), binop());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
    }
    m.insert("expt".to_string(), binop());
    // The transcendental family (CL §12.10 — "not a single one exists" was
    // the gap): trig, their inverses and hyperbolic counterparts, natural
    // `exp`/`log`, alongside the existing root/rounding unaries.
    for op in [
        "sqrt", "floor", "ceiling", "round", "truncate", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh",
        "tanh", "asinh", "acosh", "atanh", "exp", "log",
    ] {
        m.insert(op.to_string(), unary());
    }
    // See `int_assoc`'s eq/eql/equal/equalp comment — same alias-for-`=`
    // rationale (cross-type numeric `equalp`, e.g. `f64` vs `i32`, is
    // likewise unreachable: the checker requires both operands to be `f64`).
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), cmp());
    }
    // `float->int`: narrowing numeric conversion, truncating toward zero and
    // saturating (Rust's `as i32`) — the other half of `int_assoc`'s
    // `int->float`. Returns `i32`, this language's widest fixed-width integer
    // and `Checker::int_lit_ty`'s fallback; a narrower target is reached by
    // chaining `int->W` (`Checker::check_as`).
    m.insert("float->int".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::I32, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    // `float->bignum`: narrowing, truncating toward zero (`f64 as i64`'s
    // multi-precision analogue — see `crate::eval::interp::float_to_bignum`).
    // `float->ratio`: widening and *exact* — every finite `f64` is itself an
    // exact dyadic rational (CL's `rational`, not the lossy-round-trip
    // `rationalize`), via `num_rational::BigRational::from_float`.
    m.insert("float->bignum".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Bignum, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("float->ratio".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Ratio, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    // `float->f32`/`float->f64` and `try-float->f32`: a cast between the two
    // float *widths*, a real conversion now that `f32` is binary32 rather
    // than a label on an `f64` (`docs/functions.md` §1b). `float->f32` rounds
    // to nearest, `float->f64` is exact in both directions (every binary32
    // value is a binary64 value), and `try-float->f32` answers `none` when
    // the rounding would lose something — the question `try-as` asks.
    m.insert("float->f32".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::F32, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("float->f64".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::F64, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert(
        "try-float->f32".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: option_of(Type::F32), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    // `try-float->f64` can only answer `some` — widening is exact — but it is
    // registered all the same, because `Checker::width_cast` names the method
    // from the target alone and has no "this direction cannot fail" case. The
    // integer side is the same shape: `try-int->i32` from an `i8` is always
    // `some` too.
    m.insert(
        "try-float->f64".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: option_of(Type::F64), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    m.insert("print".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("println".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m
}

/// Built-in arithmetic/comparison instance methods for `bignum` (CL's
/// bignum: an arbitrary-precision integer). Core operation set: `+ - * /`
/// (`/` truncates toward zero) and `mod` (floored, CL — sign of the divisor),
/// all panicking on a zero divisor, plus conversions to/from `i32`
/// (narrowing; panics if the value doesn't fit — same precedent as
/// `int_assoc`'s `int->char`), `f64` (both directions), and `ratio`
/// (widening, exact). The rest of CL's integer catalog
/// (`rem`/`abs`/`signum`/`gcd`/`lcm`/`expt`) is defined in `prelude.rs` as
/// typelisp methods (built from these primitives), so it compiles normally.
fn bignum_assoc() -> BTreeMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum, Type::Bignum], ret: Type::Bignum, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum, Type::Bignum], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let unary = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::Bignum, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let mut m = BTreeMap::new();
    for op in ["+", "-", "*", "/", "mod", "max", "min", "logand", "logior", "logxor"] {
        m.insert(op.to_string(), binop());
    }
    // `ash`'s distance and `logbitp`'s index are `i32` here for exactly the
    // reasons `int_assoc` gives, and one more that is specific to `bignum`:
    // a shift distance that could itself be arbitrary precision is not a
    // quantity anyone can use. `(ash big huge)` names a result with `huge`
    // more bits than `big` — no machine finishes that, so the wider type
    // buys nothing and only makes the ordinary call awkward to write.
    m.insert("ash".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum, Type::I32], ret: Type::Bignum, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("logbitp".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum, Type::I32], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
    }
    m.insert("logtest".to_string(), cmp());
    for op in ["lognot", "logcount", "integer-length"] {
        m.insert(op.to_string(), unary());
    }
    // See `int_assoc`'s eq/eql/equal/equalp comment — same alias-for-`=`
    // rationale (both operands are always `bignum` here, so `equalp`'s
    // cross-type case can't be reached through this table; it's handled at
    // the `Sexpr`/dynamic layer instead — see `docs/cl-equivalence-catalog.md`).
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), cmp());
    }
    m.insert("bignum->int".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::I32, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    // `try-bignum->int`: the `Option`-returning counterpart of `bignum->int`,
    // for `(try-as i32 n)`/`(try-as i64 n)` (`Checker::check_as`) — same
    // "fits in an `i64`" check, `None` instead of a panic on overflow.
    m.insert(
        "try-bignum->int".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: option_of(Type::I32), public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true },
    );
    m.insert("bignum->float".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::F64, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("bignum->ratio".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::Ratio, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("print".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("println".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m
}

/// Built-in arithmetic/comparison instance methods for `ratio` (CL's ratio:
/// an exact rational, always kept reduced with a positive denominator).
/// `/` panics on a zero divisor like every other numeric type here.
/// `numerator`/`denominator` expose the reduced components as `bignum` (CL's
/// own accessors of the same names). The rest of CL's rational catalog
/// (`mod`/`rem`/`expt`/`abs`/`signum`) is defined in `prelude.rs` as typelisp
/// methods (built from `/` plus `ratio->bignum`/`bignum->ratio` truncation),
/// so it compiles via the normal path.
fn ratio_assoc() -> BTreeMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio, Type::Ratio], ret: Type::Ratio, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio, Type::Ratio], ret: Type::Bool, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true };
    let mut m = BTreeMap::new();
    for op in ["+", "-", "*", "/", "max", "min"] {
        m.insert(op.to_string(), binop());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
    }
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), cmp());
    }
    m.insert("ratio->bignum".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::Bignum, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("ratio->float".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::F64, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("numerator".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::Bignum, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("denominator".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::Bignum, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("print".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m.insert("println".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::Unit, public: true, builtin: true, bounds: BTreeMap::new(), optionals: Vec::new(), keys: Vec::new() }, instance: true, builtin: true });
    m
}

/// A type-parameter reference, e.g. `t` in `Option<T>`'s field list.
fn tvar(name: &str) -> Type {
    Type::Named(Path::root(name), vec![])
}
