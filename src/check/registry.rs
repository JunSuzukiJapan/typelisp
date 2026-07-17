//! Registries of data types (ADTs) and function signatures used by the checker.
//!
//! Type and variant names are stored lowercase because the reader case-folds all
//! symbols. The built-in types `Option<T>` and `Sexpr` are pre-registered.

use std::collections::HashMap;

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
    pub assoc: HashMap<String, Type>,
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
    pub bounds: HashMap<String, Vec<TraitBound>>,
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
    /// The number of fixed (non-`&rest`) parameters.
    pub arity: usize,
    /// Whether the parameter list ends in `&rest name` — a call then needs
    /// only *at least* `arity` arguments, with the trailing ones collected
    /// into a single `Sexpr` list bound to the last parameter.
    pub rest: bool,
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
/// constructs a mutable boxed struct (a `BoxedObj::Struct`, wrapped in
/// [`crate::eval::RtValue::Sexpr`]) instead of an immutable
/// [`crate::eval::RtValue::Data`] (`Checker::check_construct` decides which
/// by this field) — see [`AdtDef::field_names`] for the other `Struct`-only
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
    pub assoc: HashMap<String, AssocFn>,
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
    pub trait_assoc: HashMap<Path, HashMap<String, Type>>,
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
    pub methods: HashMap<String, FnSig>,
    /// Visible outside its defining module.
    pub public: bool,
    pub builtin: bool,
}

/// A namespace (module): a container of free functions, types, constructors,
/// child modules, and `use` aliases. `Foo::Bar` is resolved by descending into
/// the child module `Foo` and looking up `Bar` there. (Types are *not*
/// namespaces — they own associated items in [`AdtDef::assoc`].)
#[derive(Default)]
pub struct Namespace {
    /// Child modules, keyed by their (unqualified) name.
    pub modules: HashMap<String, Namespace>,
    /// Free functions defined directly here, keyed by unqualified name.
    pub fns: HashMap<String, FnSig>,
    /// `deftrait`s defined directly here, keyed by unqualified name.
    pub traits: HashMap<String, TraitDef>,
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
        let local = def.name.local().to_string();
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
    /// `check::locate::definition_target`'s `Expr::Var` arm.
    pub local_refs: HashMap<(u32, u32), Loc>,
}

/// The checker's symbol table: a tree of namespaces rooted at [`Registry::root`].
pub struct Registry {
    pub root: Namespace,
    pub def_locs: DefLocs,
}

impl Registry {
    /// A registry whose root namespace holds the built-in `Option`/`Result`/
    /// `Error`/`Sexpr` types and the i32 arithmetic/comparison operators.
    pub fn with_builtins() -> Registry {
        let mut root = Namespace::default();
        root.add_type(option_def());
        root.add_type(result_def());
        root.add_type(error_def());
        // `Sexpr` is the one type whose data constructors (`nil`/`int`/`str`/
        // ...) stay reachable as bare names without a `use` — so `(Int 5)`/
        // `(Nil)` datum literals stay writable. `Option`/`Result`/`Error`
        // constructors are `Type::ctor` (or `use`d) only — see
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
        // `i32`/`i64`/`f64` additionally get a built-in method set (no
        // `defmethod` body to check; the runtime implementation lives in
        // `eval_builtin_method` in `crate::eval::interp`, the same
        // metadata-only pattern as `hashtable_def`) — every
        // other primitive's table starts empty. Arithmetic/comparison
        // operators are *instance* methods (not free functions) precisely so
        // the same symbol (`+`, `<`, ...) can be overloaded per receiver
        // type — `i32`/`i64` share one `+` symbol, and `f64` another,
        // resolved by `check_instance_method` on the first argument's static
        // type exactly like `(get h k)` resolves to `HashTable`'s `get`.
        for ty in crate::types::primitive_types() {
            let name = crate::types::prim_type_path(&ty).expect("primitive_types() are all prim_type_path-mappable");
            let assoc = match ty {
                Type::Str => string_assoc(),
                Type::Char => char_assoc(),
                Type::I32 => int_assoc(Type::I32),
                Type::I64 => int_assoc(Type::I64),
                Type::F64 => float_assoc(),
                Type::Bignum => bignum_assoc(),
                Type::Ratio => ratio_assoc(),
                Type::Bool => bool_assoc(),
                Type::Symbol => symbol_assoc(),
                _ => HashMap::new(),
            };
            root.add_type(AdtDef {
                name,
                params: Vec::new(),
                variants: Vec::new(),
                assoc,
                public: true,
                builtin: true,
                kind: AdtKind::Sum,
                field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new(),
            });
        }
        // `random`: the only numeric builtin with no natural receiver to
        // dispatch on (like `gensym`), so it stays a free function.
        root.fns.insert("random".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::I32], ret: Type::I32, public: true, builtin: true, bounds: HashMap::new() });
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
        root.fns.insert("sexpr-cons".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: sexpr(), public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-car".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: sexpr(), public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-cdr".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: sexpr(), public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-consp".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-null".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-atom".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() });
        // Internal `Sexpr` payload extractors (Symbol/Sexpr redesign Phase 2):
        // typed field readers that used to be `match`-based typelisp defuns in
        // `compiler.rs` (`(match s ((Int n) n) (_ (panic ...)))`), moved to Rust
        // builtins (`Interp::eval_builtin`) so the island no longer needs the
        // user-facing `match` at all. Each panics on a tag mismatch, preserving
        // the old defuns' `(_ (panic ...))` contract. `sexpr-symp` is the tag
        // predicate its non-panic-fallback caller (`form-is-borrowed?`) needs
        // to branch on a `Sym` node — a peer of
        // `sexpr-consp`/`sexpr-null`/`sexpr-atom`.
        root.fns.insert("sexpr-int".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::I64, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-float".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::F64, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-bool".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-char".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::Char, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-str".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::Str, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-sym-name".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::Str, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("sexpr-symp".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() });
        // `equal`/`equalp` on `Sexpr`: structural equality (CL `equal`/`equalp`),
        // Rust builtins (`Interp::eval_builtin`'s `sexpr_equal`/`sexpr_equalp`)
        // since the Symbol/Sexpr redesign fenced `match` off `Sexpr` (Phase 5).
        // They used to be prelude `defun`s (`(match a ((Cons ..) ..) ..)`); the
        // per-scalar `equal`/`equalp` *methods* (string/char/int/...) are
        // separate (`registry::string_assoc` etc.) and resolved first when the
        // receiver is one of those types — these free `Sexpr` overloads are the
        // fallback for actual `Sexpr` data (`case` expands to `(equal ..)`).
        root.fns.insert("equal".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("equalp".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() });
        // `gensym`: a fresh `Sexpr::Sym` on every call, for macro hygiene
        // workarounds (see `Interp`'s `gensym_counter` for the caveat that
        // these are collision-*resistant*, not truly unforgeable — typelisp
        // symbols are always interned/permanent, there is no uninterned-symbol
        // concept to give a CL-style absolute guarantee).
        root.fns.insert("gensym".to_string(), FnSig { type_params: vec![], rest: None, params: vec![], ret: Type::Symbol, public: true, builtin: true, bounds: HashMap::new() });
        // `symbol->string`/`string->symbol`: the only bridges between the
        // interned `Symbol` handle and its textual name. They are Rust builtins
        // (`Interp::eval_builtin`) rather than typelisp because they touch the
        // symbol intern table directly; `Sexpr::Sym` now wraps `Symbol` (not
        // `Str`), so the old prelude `(match s (Sym name) name)` /
        // `(Sym s)` definitions no longer type-check.
        root.fns.insert("symbol->string".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Symbol], ret: Type::Str, public: true, builtin: true, bounds: HashMap::new() });
        root.fns.insert("string->symbol".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Str], ret: Type::Symbol, public: true, builtin: true, bounds: HashMap::new() });
        // `exit`: process termination (cl-equivalence-catalog.md §1.2). Unlike
        // `panic`/`unreachable`/`todo` (which unwind through `EvalError::Panic`,
        // a typelisp-level signal), this needs an actual OS call
        // (`std::process::exit`, in `Interp::eval_builtin`'s `"exit"` arm), so
        // it stays an ordinary `Rust` builtin rather than a `defmacro`. `Never`
        // return type, same as `panic`, so it satisfies any expected type.
        root.fns.insert("exit".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::I32], ret: Type::Never, public: true, builtin: true, bounds: HashMap::new() });
        // `compile`: JIT-compiles a previously-defined `defun` (see
        // `Interp::compile_function`) so later calls dispatch to native
        // code. This `Type::Str` signature is the internal shape only —
        // surface syntax takes an unevaluated symbol or `::`-path
        // (`(compile foo)`, `(compile point::x)`), special-cased in
        // `Checker::check_compile` to convert that name to the string this
        // entry expects before an ordinary call is built; a string literal
        // there (`(compile "foo")`) is a type error.
        root.fns.insert("compile".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Str], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() });
        // `compile-file`: AOT-compiles an independent source file to a
        // native executable (see `Interp::eval_builtin`'s `"compile-file"`
        // arm / `compile::aot::compile_file`). Same free-function shape as
        // `compile` above, just two string arguments (source path, output
        // path) instead of one.
        root.fns.insert(
            "compile-file".to_string(),
            FnSig { type_params: vec![], rest: None, params: vec![Type::Str, Type::Str], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() },
        );
        Registry { root, def_locs: DefLocs::default() }
    }

    /// Look up a type by its fully-qualified [`Path`] (e.g. `geo::point`).
    pub fn type_def(&self, path: &Path) -> Option<&AdtDef> {
        self.root.module(path.parent())?.types.get(path.local())
    }

    /// Mutable lookup of a type by its fully-qualified [`Path`].
    pub fn type_def_mut(&mut self, path: &Path) -> Option<&mut AdtDef> {
        self.root.module_mut(path.parent()).types.get_mut(path.local())
    }

    /// Look up a free function by its fully-qualified [`Path`].
    pub fn fn_sig(&self, path: &Path) -> Option<&FnSig> {
        self.root.module(path.parent())?.fns.get(path.local())
    }

    /// Look up a `deftrait` by its fully-qualified [`Path`].
    pub fn trait_def(&self, path: &Path) -> Option<&TraitDef> {
        self.root.module(path.parent())?.traits.get(path.local())
    }

    /// Mutable lookup of a `deftrait` by its fully-qualified [`Path`].
    pub fn trait_def_mut(&mut self, path: &Path) -> Option<&mut TraitDef> {
        self.root.module_mut(path.parent()).traits.get_mut(path.local())
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
        assoc: HashMap::new(),
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new(),
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
        assoc: HashMap::new(),
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new(),
    }
}

/// The built-in generic error type carrying a message: `Error(String)`. This is
/// the default `E` for fallible built-ins; user-defined error types come later.
fn error_def() -> AdtDef {
    AdtDef {
        name: Path::root("error"),
        params: vec![],
        variants: vec![Variant { name: "error".to_string(), fields: vec![Type::Str] }],
        assoc: HashMap::new(),
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new(),
    }
}

/// The built-in `Sexpr` sum type (the result type of `read`).
///
/// `Sexpr = Nil | Int | Float | Char | Bool | Sym | Str | Cons(Sexpr, Sexpr)`:
/// the empty list `()` is the nullary `Nil` constructor (a `Sexpr` value in its
/// own right, on par with `cons` cells), and `cons` holds two `Sexpr` fields —
/// matching Lisp's `list = cons | nil` duality so `car`/`cdr` and list
/// operations (`cons`/`car`/`cdr`/`list`/`dolist`) stay in `Sexpr` throughout.
fn sexpr_def() -> AdtDef {
    AdtDef {
        name: Path::root("sexpr"),
        params: vec![],
        variants: vec![
            Variant { name: "nil".to_string(), fields: vec![] },
            Variant { name: "int".to_string(), fields: vec![Type::I64] },
            Variant { name: "float".to_string(), fields: vec![Type::F64] },
            Variant { name: "char".to_string(), fields: vec![Type::Char] },
            Variant { name: "bool".to_string(), fields: vec![Type::Bool] },
            Variant { name: "sym".to_string(), fields: vec![Type::Symbol] },
            Variant { name: "str".to_string(), fields: vec![Type::Str] },
            Variant { name: "cons".to_string(), fields: vec![sexpr(), sexpr()] },
            // Appended after `cons` (not inserted alongside `int`/`float`)
            // so the existing `SEXPR_*` variant-index constants
            // (`crate::eval::interp`) stay valid.
            Variant { name: "bignum".to_string(), fields: vec![Type::Bignum] },
            Variant { name: "ratio".to_string(), fields: vec![Type::Ratio] },
        ],
        assoc: sexpr_assoc(),
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new(),
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
fn sexpr_assoc() -> HashMap<String, AssocFn> {
    let mut m = HashMap::new();
    let eq_fn = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    m.insert("eq".to_string(), eq_fn());
    m.insert("eql".to_string(), eq_fn());
    m
}

fn bool_assoc() -> HashMap<String, AssocFn> {
    let mut m = HashMap::new();
    // `bool` has exactly two values, both immediate — `eq`/`eql`/`equal`/
    // `equalp` can never diverge for it (no case-folding, no cross-type
    // comparison, no structure to recurse into), so all four alias the same
    // value comparison. See `docs/cl-equivalence-catalog.md`'s eq/eql/equal/
    // equalp section for why every one of these four is still registered
    // explicitly rather than leaving `eql`/`equal`/`equalp` undefined.
    let eq_fn = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bool, Type::Bool], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), eq_fn());
    }
    m
}

/// `symbol`'s method table: `eq`/`eql` only. Symbols are always interned, so
/// two `Symbol`s are `eq` iff they are the same interned id (same name) — the
/// runtime routes both to `sexpr_eq`/`sexpr_eql` since a `Symbol` value shares
/// the `Value::Symbol(id)` carrier of a `Sexpr::Sym`. `equal`/`equalp` are not
/// registered (there is no structure to recurse into beyond `eq`'s identity).
fn symbol_assoc() -> HashMap<String, AssocFn> {
    let mut m = HashMap::new();
    let eq_fn = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Symbol, Type::Symbol], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    m.insert("eq".to_string(), eq_fn());
    m.insert("eql".to_string(), eq_fn());
    m
}

fn sexpr() -> Type {
    Type::Named(Path::root("sexpr"), vec![])
}

fn hashtable_ty() -> Type {
    Type::Named(Path::root("hashtable"), vec![tvar("k"), tvar("v")])
}

fn option_of(t: Type) -> Type {
    Type::Named(Path::root("option"), vec![t])
}

/// `HashTable<K, V>`: a builtin (Rust-implemented) mutable hash map, with no
/// constructors of its own (built via the static `new`, not pattern-matched).
/// All methods here are metadata only — there is no `defmethod` body to
/// check; the runtime implementation lives in `eval_builtin_method` in
/// `crate::eval::interp`.
fn hashtable_def() -> AdtDef {
    let mut assoc = HashMap::new();
    assoc.insert(
        "new".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![], ret: hashtable_ty(), public: true, builtin: true, bounds: HashMap::new() }, instance: false, builtin: true },
    );
    assoc.insert(
        "get".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k")], ret: option_of(tvar("v")), public: true, builtin: true, bounds: HashMap::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "set".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k"), tvar("v")], ret: Type::Unit, public: true, builtin: true, bounds: HashMap::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "remove".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k")], ret: option_of(tvar("v")), public: true, builtin: true, bounds: HashMap::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "count".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::I32, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "clear".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::Unit, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
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
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::Named(Path::root("vector"), vec![tvar("k")]), public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "values".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::Named(Path::root("vector"), vec![tvar("v")]), public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
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
                bounds: HashMap::new(),
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
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new(),
    }
}

fn vector_ty() -> Type {
    Type::Named(Path::root("vector"), vec![tvar("t")])
}

/// `Vector<T>`: a builtin growable sequence. Reuses the same boxed-struct
/// representation `defstruct` instances get (a `BoxedObj::Struct`, wrapped
/// in [`crate::eval::RtValue::Sexpr`]) rather than a dedicated `RtValue`
/// variant — a `Vector<T>` instance's fields are simply treated as
/// variable-length instead of the fixed, name-indexed layout a `defstruct`'s
/// fields have (see `eval_builtin_method`'s `"vector"` arm).
/// All methods here are metadata only — there is no `defmethod` body to
/// check; the runtime implementation lives in `eval_builtin_method` in
/// `crate::eval::interp`.
fn vector_def() -> AdtDef {
    let mut assoc = HashMap::new();
    assoc.insert(
        "new".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![], ret: vector_ty(), public: true, builtin: true, bounds: HashMap::new() }, instance: false, builtin: true },
    );
    assoc.insert(
        "push".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), tvar("t")], ret: Type::Unit, public: true, builtin: true, bounds: HashMap::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "get".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), Type::I32], ret: tvar("t"), public: true, builtin: true, bounds: HashMap::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "set".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), Type::I32, tvar("t")], ret: Type::Unit, public: true, builtin: true, bounds: HashMap::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "len".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty()], ret: Type::I32, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
    );
    AdtDef {
        name: Path::root("vector"),
        params: vec!["t".to_string()],
        variants: vec![],
        assoc,
        public: true,
        builtin: true,
        kind: AdtKind::Struct,
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new(),
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
    let mut assoc = HashMap::new();
    assoc.insert(
        "new".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![], ret: scope_ty(), public: true, builtin: true, bounds: HashMap::new() }, instance: false, builtin: true },
    );
    assoc.insert(
        // Shares every existing frame (by reference, not by copying their
        // contents) plus pushes one fresh empty frame on top — the single
        // operation a `labels` def's own new lexical scope needs to start
        // from every enclosing scope's frames (see `compile-labels`'s doc
        // comment for why a *fresh* top frame, not the shared ones
        // themselves, must receive that def's own parameter bindings).
        "clone-frames".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty()], ret: scope_ty(), public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "push-frame".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty()], ret: Type::Unit, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "pop-frame".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty()], ret: Type::Unit, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
    );
    assoc.insert(
        "get".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty(), Type::Str], ret: option_of(tvar("v")), public: true, builtin: true, bounds: HashMap::new() },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "set".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![scope_ty(), Type::Str, tvar("v")], ret: Type::Unit, public: true, builtin: true, bounds: HashMap::new() },
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
        field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new(),
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
    AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true, bounds: HashMap::new() }, instance, builtin: true }
}

/// An in-progress LLVM module. `add-function` always declares a function
/// under one fixed C ABI — `i64 name(i64* args, i32 argc)` — regardless of
/// the typelisp-level function's actual arity; `llvm-builder::load-arg`
/// reads a logical parameter back out of that array. Fixing the ABI this
/// way (rather than generating a distinct LLVM signature per arity) is what
/// lets `compile::CompiledFn` call *any* compiled function through one Rust
/// function-pointer type — other return types are a later phase's concern.
fn llvm_module_def() -> AdtDef {
    let mut assoc = HashMap::new();
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
    // (`compile::freevars::labels_free_vars`) is non-empty. Every sibling in
    // such a block shares this one extended signature, even ones whose own
    // body doesn't reference every captured name (see `compiler.rs`'s
    // `compile-labels` doc comment for why captures aren't computed
    // per-sibling).
    assoc.insert("add-function-with-env".to_string(), assoc_fn(vec![llvm_module_ty(), Type::Str], llvm_function_ty(), true));
    AdtDef { name: Path::root("llvm-module"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new() }
}

/// A declared LLVM function (a `Module::add-function` result).
fn llvm_function_def() -> AdtDef {
    let mut assoc = HashMap::new();
    assoc.insert("append-block".to_string(), assoc_fn(vec![llvm_function_ty(), Type::Str], llvm_basic_block_ty(), true));
    AdtDef { name: Path::root("llvm-function"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new() }
}

/// An LLVM basic block. No methods of its own yet (Phase 0) — produced by
/// `llvm-function::append-block`, consumed by `llvm-builder::position-at-end`.
fn llvm_basic_block_def() -> AdtDef {
    AdtDef { name: Path::root("llvm-basic-block"), params: vec![], variants: vec![], assoc: HashMap::new(), public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new() }
}

/// An IR builder. `load-arg` reads logical parameter `index` out of a
/// function's fixed-ABI argument array (see `llvm_module_def`'s doc
/// comment); `build-add`/`build-sub`/`build-mul` cover Phase 1's scalar
/// arithmetic (comparisons, which return `bool` rather than `i64`, are a
/// later phase's concern — keeping every builtin here `i64`-in-`i64`-out
/// for now).
fn llvm_builder_def() -> AdtDef {
    let mut assoc = HashMap::new();
    assoc.insert("create".to_string(), assoc_fn(vec![], llvm_builder_ty(), false));
    assoc.insert("position-at-end".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_basic_block_ty()], Type::Unit, true));
    assoc.insert("const-i64".to_string(), assoc_fn(vec![llvm_builder_ty(), Type::I64], llvm_value_ty(), true));
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
    assoc.insert("build-shl".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true));
    // Logical (unsigned) vs. arithmetic (sign-extending) right shift: the
    // tagged representation's payload bits must never be sign-extended back
    // in when shifting a Cons/Symbol/Str/Path pointer or index out from
    // under its tag (`build-lshr`), but a genuine signed `Sexpr::Int`
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
    for name in ["build-fsqrt", "build-ffloor", "build-fceil", "build-fround", "build-ftrunc"] {
        assoc.insert(name.to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_module_ty(), llvm_value_ty()], llvm_value_ty(), true));
    }
    // `expt` (`f64,f64->f64`): the binary counterpart of the unary
    // transcendentals above, `llvm.pow.f64`.
    assoc.insert(
        "build-fpow".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_module_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty(), true),
    );
    // `float->int` (`f64->i32`, narrowing, truncating toward zero): a single
    // `fptosi` instruction, no heap allocation and no module lookup needed —
    // unlike `float->bignum`/`float->ratio`, which stay non-native (see
    // `float-native-method?`'s doc comment).
    assoc.insert("build-fptosi".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_module_ty(), llvm_value_ty()], llvm_value_ty(), true));
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
            vec![llvm_builder_ty(), llvm_module_ty(), llvm_function_ty(), llvm_value_ty(), Type::I32, Type::I64],
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
    AdtDef { name: Path::root("llvm-builder"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new() }
}

/// An LLVM SSA value (e.g. a constant). No methods of its own yet — produced
/// by `llvm-builder::const-i64`, consumed by `llvm-builder::build-ret`.
fn llvm_value_def() -> AdtDef {
    AdtDef { name: Path::root("llvm-value"), params: vec![], variants: vec![], assoc: HashMap::new(), public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new(), impls: Vec::new(), trait_assoc: HashMap::new() }
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
/// CL identity (`Rc::ptr_eq` on `RtValue::Str`'s underlying `Rc<str>` —
/// see that variant's own doc comment for why `Rc`, not a plain `String`, is
/// what makes identity meaningful here at all); `eql` doesn't add anything
/// beyond `eq` for strings in real CL either (it only extends numbers/
/// characters), so it's a plain alias. Content comparison — what a naive
/// reading of "string equality" usually means — is `equal` (case-sensitive)
/// and `equalp` (case-insensitive, ASCII-only for the same reason
/// `upcase`/`downcase` are) instead; two separately-built equal-content
/// `Str`s are correctly *not* `eq`/`eql`.
fn string_assoc() -> HashMap<String, AssocFn> {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let mut m = HashMap::new();
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
fn char_assoc() -> HashMap<String, AssocFn> {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let mut m = HashMap::new();
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
    m.insert("char->int".to_string(), method(vec![Type::Char], Type::I32));
    m
}

/// Built-in arithmetic/comparison instance methods for an integer type
/// (`i32`/`i64`): `+ - * / mod` (binary, same-type) and `< <= > >= = /=`
/// (binary, `Bool`-valued). `/`/`mod` panic on a zero divisor at runtime
/// (`crate::eval::interp::eval_int_builtin`) — the type system can't express
/// "nonzero", the same precedent as `car`/`cdr` on a non-`Cons` `Sexpr`.
/// Shared by both integer widths since the operation set and panic policy
/// are identical; only the receiver/param `Type` differs.
fn int_assoc(ty: Type) -> HashMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: ty.clone(), public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let mut m = HashMap::new();
    for op in ["+", "-", "*", "/", "mod"] {
        m.insert(op.to_string(), binop());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
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
    // §4.1's planned conversion catalog) — registered for both `i32`/`i64`
    // widths since each gets its own `int_assoc` call. This is also what
    // `equalp`'s `Sexpr` `Int`<->`Float` cross-type comparison
    // (`prelude.rs`) needed and previously lacked (see
    // `docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp section).
    m.insert("int->float".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::F64, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    // `int->char`: the other half of `char_assoc`'s `char->int` — a Unicode
    // scalar value back to `char`. Panics at runtime on a value outside the
    // valid range (surrogates, or past `U+10FFFF`) — the type system can't
    // express "valid scalar value", same precedent as `car`/`cdr` on a
    // non-`Cons` `Sexpr` or division by zero.
    m.insert("int->char".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Char, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    // `try-int->char`: the `Option`-returning counterpart of `int->char`,
    // for `(try-as char n)` (`Checker::check_as`) — same Unicode-scalar-
    // value validity check, `None` instead of a panic on failure.
    m.insert(
        "try-int->char".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: option_of(Type::Char), public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
    );
    // `int->bignum`/`int->ratio`: widening conversions into the two
    // arbitrary-precision types (`docs/cl-equivalence-catalog.md`'s planned
    // conversion catalog, extended for `bignum`/`ratio`) — always exact,
    // unlike `bignum->int`/`ratio->int`'s narrowing counterparts.
    m.insert("int->bignum".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone()], ret: Type::Bignum, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m.insert("int->ratio".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty], ret: Type::Ratio, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m
}

/// Built-in arithmetic/comparison/transcendental instance methods for `f64`:
/// the same binary operator set as [`int_assoc`] (unlike integer division,
/// `/`/`mod` follow IEEE-754 — a zero divisor yields `inf`/`NaN`, no panic),
/// plus `expt`(binary, `f64::powf`) and the unary rounding/root family
/// `sqrt`/`floor`/`ceiling`/`round`/`truncate`.
fn float_assoc() -> HashMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64, Type::F64], ret: Type::F64, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64, Type::F64], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let unary = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64], ret: Type::F64, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let mut m = HashMap::new();
    for op in ["+", "-", "*", "/", "mod"] {
        m.insert(op.to_string(), binop());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
    }
    m.insert("expt".to_string(), binop());
    for op in ["sqrt", "floor", "ceiling", "round", "truncate"] {
        m.insert(op.to_string(), unary());
    }
    // See `int_assoc`'s eq/eql/equal/equalp comment — same alias-for-`=`
    // rationale (cross-type numeric `equalp`, e.g. `f64` vs `i32`, is
    // likewise unreachable: the checker requires both operands to be `f64`).
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), cmp());
    }
    // `float->int`: narrowing numeric conversion, truncating toward zero
    // (Rust's `as i64`, same as CL's `truncate`) — the other half of
    // `int_assoc`'s `int->float`. Returns `i32` (this language's default
    // integer type, `Checker::int_lit_ty`'s fallback) even though the
    // runtime value is a uniform `RtValue::Int(i64)` either way (see
    // `eval_int_builtin`'s doc comment).
    m.insert("float->int".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64], ret: Type::I32, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    // `float->bignum`: narrowing, truncating toward zero (`f64 as i64`'s
    // multi-precision analogue — see `crate::eval::interp::float_to_bignum`).
    // `float->ratio`: widening and *exact* — every finite `f64` is itself an
    // exact dyadic rational (CL's `rational`, not the lossy-round-trip
    // `rationalize`), via `num_rational::BigRational::from_float`.
    m.insert("float->bignum".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64], ret: Type::Bignum, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m.insert("float->ratio".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64], ret: Type::Ratio, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m
}

/// Built-in arithmetic/comparison instance methods for `bignum` (CL's
/// bignum: an arbitrary-precision integer). Same operation set as
/// [`int_assoc`] (`/`/`mod` truncate toward zero and panic on a zero
/// divisor — CL's `truncate`/`rem`, not `floor`/`mod`), plus conversions
/// to/from `i32`/`i64` (narrowing; panics if the value doesn't fit — same
/// precedent as `int_assoc`'s `int->char`), `f64` (both directions), and
/// `ratio` (widening, exact).
fn bignum_assoc() -> HashMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum, Type::Bignum], ret: Type::Bignum, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum, Type::Bignum], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let mut m = HashMap::new();
    for op in ["+", "-", "*", "/", "mod"] {
        m.insert(op.to_string(), binop());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
    }
    // See `int_assoc`'s eq/eql/equal/equalp comment — same alias-for-`=`
    // rationale (both operands are always `bignum` here, so `equalp`'s
    // cross-type case can't be reached through this table; it's handled at
    // the `Sexpr`/dynamic layer instead — see `docs/cl-equivalence-catalog.md`).
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), cmp());
    }
    m.insert("bignum->int".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::I32, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    // `try-bignum->int`: the `Option`-returning counterpart of `bignum->int`,
    // for `(try-as i32 n)`/`(try-as i64 n)` (`Checker::check_as`) — same
    // "fits in an `i64`" check, `None` instead of a panic on overflow.
    m.insert(
        "try-bignum->int".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: option_of(Type::I32), public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true },
    );
    m.insert("bignum->float".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::F64, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m.insert("bignum->ratio".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bignum], ret: Type::Ratio, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m
}

/// Built-in arithmetic/comparison instance methods for `ratio` (CL's ratio:
/// an exact rational, always kept reduced with a positive denominator).
/// `/` panics on a zero divisor like every other numeric type here; there is
/// no `mod` (CL doesn't define a rational remainder either — `mod`/`rem`
/// only apply to integers). `numerator`/`denominator` expose the reduced
/// components as `bignum` (CL's own accessors of the same names), the only
/// way to inspect a `ratio`'s value beyond comparison/conversion.
fn ratio_assoc() -> HashMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio, Type::Ratio], ret: Type::Ratio, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio, Type::Ratio], ret: Type::Bool, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true };
    let mut m = HashMap::new();
    for op in ["+", "-", "*", "/"] {
        m.insert(op.to_string(), binop());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
    }
    for name in ["eq", "eql", "equal", "equalp"] {
        m.insert(name.to_string(), cmp());
    }
    m.insert("ratio->bignum".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::Bignum, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m.insert("ratio->float".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::F64, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m.insert("numerator".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::Bignum, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m.insert("denominator".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Ratio], ret: Type::Bignum, public: true, builtin: true, bounds: HashMap::new() }, instance: true, builtin: true });
    m
}

/// A type-parameter reference, e.g. `t` in `Option<T>`'s field list.
fn tvar(name: &str) -> Type {
    Type::Named(Path::root(name), vec![])
}
