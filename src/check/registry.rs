//! Registries of data types (ADTs) and function signatures used by the checker.
//!
//! Type and variant names are stored lowercase because the reader case-folds all
//! symbols. The built-in types `Option<T>` and `Sexpr` are pre-registered.

use std::collections::HashMap;

use crate::{Path, Type};

/// One constructor of a data type: a name and its field types. Field types may
/// reference the enclosing type's parameters as `Type::Named(param, [])`.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Type>,
}

/// A function's parameter and return types.
#[derive(Clone, Debug)]
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
}

/// A type-associated function or method (Rust-style; types are *not*
/// namespaces). `instance` is true when the first parameter is the receiver.
#[derive(Clone, Debug)]
pub struct AssocFn {
    pub sig: FnSig,
    pub instance: bool,
    /// See [`FnSig::builtin`].
    pub builtin: bool,
}

/// A `defmacro`'s signature: an arity and whether it's variadic (every
/// parameter and the implicit return are always `Sexpr`, so there is no
/// per-parameter type to record — see `Checker::check_defmacro`).
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
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
/// constructs a mutable [`crate::eval::RtValue::Struct`] instead of an
/// immutable [`crate::eval::RtValue::Data`] (`Checker::check_construct`
/// decides which by this field) — see [`AdtDef::field_names`] for the other
/// `Struct`-only piece of metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdtKind {
    Sum,
    Struct,
}

/// A built-in data-type definition (a sum type, e.g. `Option`/`Result`/
/// `Sexpr`/`HashTable`). `name` is the fully-qualified (module-prefixed) type
/// [`Path`] used as the type's identity.
#[derive(Clone, Debug)]
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

/// The checker's symbol table: a tree of namespaces rooted at [`Registry::root`].
pub struct Registry {
    pub root: Namespace,
}

impl Registry {
    /// A registry whose root namespace holds the built-in `Option`/`Result`/
    /// `Error`/`Sexpr` types and the i32 arithmetic/comparison operators.
    pub fn with_builtins() -> Registry {
        let mut root = Namespace::default();
        root.add_type(option_def());
        root.add_type(result_def());
        root.add_type(error_def());
        // `Sexpr` is the one type whose constructors (`nil`/`cons`/...) stay
        // reachable as bare names without a `use` — Lisp's list operations
        // (`car`/`cdr`/`cons`/list literals) would be unworkably verbose
        // otherwise. `Option`/`Result`/`Error` constructors are `Type::ctor`
        // (or `use`d) only — see `Checker::check_use`.
        let def = sexpr_def();
        root.register_ctors(&def.name, &def.variants);
        root.add_type(def);
        root.add_type(hashtable_def());
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
                Type::Bool => bool_assoc(),
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
                field_names: Vec::new(),
            });
        }
        // `random`: the only numeric builtin with no natural receiver to
        // dispatch on (like `gensym`), so it stays a free function.
        root.fns.insert("random".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::I32], ret: Type::I32, public: true, builtin: true });
        // `not`: a plain unary function (no short-circuiting needed, unlike
        // `and`/`or`), so — unlike those two — it doesn't need special-form
        // treatment.
        root.fns.insert("not".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Bool], ret: Type::Bool, public: true, builtin: true });
        // `cons`/`car`/`cdr` operate on `Sexpr` (the cons/nil duality at the
        // type level). `cons` is also reachable as the `Cons` constructor;
        // registering it as a function too lets it be used as a value
        // (e.g. passed to a higher-order function).
        root.fns.insert("cons".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: sexpr(), public: true, builtin: true });
        root.fns.insert("car".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: sexpr(), public: true, builtin: true });
        root.fns.insert("cdr".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: sexpr(), public: true, builtin: true });
        // `set-car`/`set-cdr` (CL `rplaca`/`rplacd`): in-place mutation of an
        // existing cons cell, backed by `mem::Heap::set_car`/`set_cdr` (already
        // implemented at the heap layer, just not wired up to a language-level
        // name until now). Panics on a non-`Cons` `Sexpr`, matching `car`/`cdr`.
        // This is the prerequisite `nconc`/`nreverse` (the prelude's destructive
        // list operations) build on.
        root.fns.insert("set-car".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Unit, public: true, builtin: true });
        root.fns.insert("set-cdr".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Unit, public: true, builtin: true });
        // `gensym`: a fresh `Sexpr::Sym` on every call, for macro hygiene
        // workarounds (see `Interp`'s `gensym_counter` for the caveat that
        // these are collision-*resistant*, not truly unforgeable — typelisp
        // symbols are always interned/permanent, there is no uninterned-symbol
        // concept to give a CL-style absolute guarantee).
        root.fns.insert("gensym".to_string(), FnSig { type_params: vec![], rest: None, params: vec![], ret: sexpr(), public: true, builtin: true });
        // `compile`: JIT-compiles a previously-defined `defun` (see
        // `Interp::compile_function`) so later calls dispatch to native
        // code. No natural receiver (it operates on something named by a
        // string, not a typed value), so a free function like `gensym`/
        // `random` above.
        root.fns.insert("compile".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Str], ret: Type::Bool, public: true, builtin: true });
        // `compile-file`: AOT-compiles an independent source file to a
        // native executable (see `Interp::eval_builtin`'s `"compile-file"`
        // arm / `compile::aot::compile_file`). Same free-function shape as
        // `compile` above, just two string arguments (source path, output
        // path) instead of one.
        root.fns.insert(
            "compile-file".to_string(),
            FnSig { type_params: vec![], rest: None, params: vec![Type::Str, Type::Str], ret: Type::Bool, public: true, builtin: true },
        );
        Registry { root }
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
        field_names: Vec::new(),
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
        field_names: Vec::new(),
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
        field_names: Vec::new(),
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
            Variant { name: "sym".to_string(), fields: vec![Type::Str] },
            Variant { name: "str".to_string(), fields: vec![Type::Str] },
            Variant { name: "cons".to_string(), fields: vec![sexpr(), sexpr()] },
        ],
        assoc: sexpr_assoc(),
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(),
    }
}

/// `eq`: identity/structural-scalar equality on `Sexpr`, matching CL's `eq`
/// for the cases this representation can express cheaply — comparing the
/// underlying `mem::Value` directly (`crate::eval::interp`'s
/// `eval_builtin_method`) means two `Cons` cells are equal only if they're
/// the *same* heap cell (true `eq` identity), while two scalar `Sexpr`s
/// (`Int`/`Char`/`Sym`/...) compare by value — `Sym` is still correct under
/// `eq` since symbols are always interned (same name -> same id). The one
/// case this can't get right structurally is two separately-built `Str`
/// `Sexpr`s with equal content (different heap allocations, not `eq` in
/// CL either) — `equal` (the prelude's recursive structural comparison)
/// special-cases `Str` to compare content instead.
fn sexpr_assoc() -> HashMap<String, AssocFn> {
    let mut m = HashMap::new();
    m.insert(
        "eq".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Bool, public: true, builtin: true }, instance: true, builtin: true },
    );
    m
}

fn bool_assoc() -> HashMap<String, AssocFn> {
    let mut m = HashMap::new();
    m.insert(
        "eq".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bool, Type::Bool], ret: Type::Bool, public: true, builtin: true }, instance: true, builtin: true },
    );
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
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![], ret: hashtable_ty(), public: true, builtin: true }, instance: false, builtin: true },
    );
    assoc.insert(
        "get".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k")], ret: option_of(tvar("v")), public: true, builtin: true },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "set".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k"), tvar("v")], ret: Type::Unit, public: true, builtin: true },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "remove".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k")], ret: option_of(tvar("v")), public: true, builtin: true },
            instance: true,
            builtin: true,
        },
    );
    assoc.insert(
        "count".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::I32, public: true, builtin: true }, instance: true, builtin: true },
    );
    assoc.insert(
        "clear".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::Unit, public: true, builtin: true }, instance: true, builtin: true },
    );
    AdtDef {
        name: Path::root("hashtable"),
        params: vec!["k".to_string(), "v".to_string()],
        variants: vec![],
        assoc,
        public: true,
        builtin: true,
        kind: AdtKind::Sum,
        field_names: Vec::new(),
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
    AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true }, instance, builtin: true }
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
    AdtDef { name: Path::root("llvm-module"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new() }
}

/// A declared LLVM function (a `Module::add-function` result).
fn llvm_function_def() -> AdtDef {
    let mut assoc = HashMap::new();
    assoc.insert("append-block".to_string(), assoc_fn(vec![llvm_function_ty(), Type::Str], llvm_basic_block_ty(), true));
    AdtDef { name: Path::root("llvm-function"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new() }
}

/// An LLVM basic block. No methods of its own yet (Phase 0) — produced by
/// `llvm-function::append-block`, consumed by `llvm-builder::position-at-end`.
fn llvm_basic_block_def() -> AdtDef {
    AdtDef { name: Path::root("llvm-basic-block"), params: vec![], variants: vec![], assoc: HashMap::new(), public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new() }
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
    // `build-make-closure`/`build-closure-env-get`/`build-closure-apply`/
    // `build-closure-retain`/`build-closure-release`: a `ClosureBox` — the
    // runtime representation of a `lambda` value that *escapes* its defining
    // function (rather than being called directly while statically known,
    // like every `apply`/`call` site above) — labels/closures Stage 4.
    // Heap-allocated (`malloc`/`free`, not a Rust-side runtime shim — see
    // `compile::CompiledFn`'s module doc comment for the staticlib
    // alternative this replaces) with a fixed `i64` layout: `fn_ptr`,
    // `env_len`, `refcount`, then `env_len` captured values inline. The
    // resulting `i64` *is* the closure value, exactly like every other
    // compiled value — see `registry::llvm_module_def`'s doc comment for why
    // every compiled value is a plain, untagged `i64`.
    //
    // `build-make-closure` takes the already-compiled `target` (declared
    // under `add-function-with-env`'s ABI — *every* closure-boxed function
    // uses that ABI, capturing or not, so `build-closure-apply` never has to
    // decide which ABI to call through) and an env array built the same way
    // a direct capturing call already builds one (`alloca-args`/`store-arg`/
    // `compile-env-args`), copying it into the new heap box rather than
    // passing it straight through (the stack array doesn't outlive this
    // call). `build-closure-env-get` is `load-env`'s closure-value
    // counterpart (reads a captured slot back out of the box itself, for the
    // *nested* function's own body — the closure-boxed function still reads
    // its captures via `load-env`/its own env parameter, never this one;
    // this one is for code holding the closure *value* from the outside).
    // `build-closure-apply` is `build-call-with-env`'s indirect counterpart:
    // the callee isn't a statically-known `llvm-function` here, just an
    // `i64` value, so it loads `fn_ptr`/`env_len`/the env pointer out of the
    // box at runtime and calls through `build_indirect_call` instead.
    // `build-closure-retain` increments the refcount and returns the closure
    // itself (chainable); `build-closure-release` decrements it and, once it
    // reaches zero, recursively releases any captured slot `fn_mask` marks
    // as itself `Fn`-typed before freeing the box (automatic retain/release
    // insertion, a follow-up to labels/closures Stage 4 — see
    // `compiler.rs`'s `resolve-value`/`compile-lambda` for where retains are
    // inserted on the way in). `build-make-closure`'s `fn_mask` (a bitmask,
    // one bit per captured slot, computed entirely at compile time by
    // `compiler.rs`'s `compute-fn-mask`) is what makes that cascade
    // possible — without it, `build-closure-release` would have no way to
    // tell a captured closure pointer apart from an ordinary `i64` at
    // runtime. A true reference cycle between two `ClosureBox`es still isn't
    // constructible under this design (see `resolve-value`'s doc comment),
    // so the cascade's recursion is always finite in practice even though
    // nothing here guards against one.
    assoc.insert(
        "build-make-closure".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_function_ty(), llvm_value_ty(), Type::I32, Type::I64], llvm_value_ty(), true),
    );
    assoc.insert(
        "build-closure-env-get".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), Type::I32], llvm_value_ty(), true),
    );
    assoc.insert(
        "build-closure-apply".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_value_ty(), llvm_value_ty(), Type::I32], llvm_value_ty(), true),
    );
    assoc.insert("build-closure-retain".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty()], llvm_value_ty(), true));
    assoc.insert(
        "build-closure-release".to_string(),
        assoc_fn(vec![llvm_builder_ty(), llvm_module_ty(), llvm_value_ty()], Type::Unit, true),
    );
    // Test-only: reads a `ClosureBox`'s refcount slot directly, as an
    // ordinary `llvm-value` — never called from `compiler.rs` itself, only
    // from `tests/compile_test.rs` to verify the automatic retain/release
    // insertion work, see `interp::llvm_builder_debug_closure_refcount`'s
    // doc comment.
    assoc.insert("debug-closure-refcount".to_string(), assoc_fn(vec![llvm_builder_ty(), llvm_value_ty()], llvm_value_ty(), true));
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
    AdtDef { name: Path::root("llvm-builder"), params: vec![], variants: vec![], assoc, public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new() }
}

/// An LLVM SSA value (e.g. a constant). No methods of its own yet — produced
/// by `llvm-builder::const-i64`, consumed by `llvm-builder::build-ret`.
fn llvm_value_def() -> AdtDef {
    AdtDef { name: Path::root("llvm-value"), params: vec![], variants: vec![], assoc: HashMap::new(), public: true, builtin: true, kind: AdtKind::Sum, field_names: Vec::new() }
}

/// Built-in `String` instance methods ([cl-equivalence-catalog.md](../../../docs/cl-equivalence-catalog.md)
/// §2.2 d). All char/index arguments and `length` count Unicode scalar values
/// (`char`s), not bytes. `ref`/`substring` panic on an out-of-range index —
/// the type system can't express the bound, the same precedent as
/// `car`/`cdr` on a non-`Cons` `Sexpr`. `upcase`/`downcase` are ASCII-only
/// (`str::to_ascii_uppercase`/`lowercase`), avoiding Unicode case mappings
/// that can change a string's length (e.g. German `ß` -> `SS`).
fn string_assoc() -> HashMap<String, AssocFn> {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true }, instance: true, builtin: true };
    let mut m = HashMap::new();
    m.insert("upcase".to_string(), method(vec![Type::Str], Type::Str));
    m.insert("downcase".to_string(), method(vec![Type::Str], Type::Str));
    m.insert("length".to_string(), method(vec![Type::Str], Type::I32));
    m.insert("ref".to_string(), method(vec![Type::Str, Type::I32], Type::Char));
    m.insert("substring".to_string(), method(vec![Type::Str, Type::I32, Type::I32], Type::Str));
    m.insert("append".to_string(), method(vec![Type::Str, Type::Str], Type::Str));
    m.insert("eq".to_string(), method(vec![Type::Str, Type::Str], Type::Bool));
    m.insert("lt".to_string(), method(vec![Type::Str, Type::Str], Type::Bool));
    m
}

/// Built-in `char` instance methods (same catalog section as
/// [`string_assoc`]). `upcase`/`downcase` are ASCII-only, for the same
/// reason as `string_assoc`'s (a non-ASCII char's case mapping isn't
/// necessarily a single char). `alphap`/`digitp` classify ASCII letters/
/// digits only (CL's `alpha-char-p`/`digit-char-p` without a radix).
fn char_assoc() -> HashMap<String, AssocFn> {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true, builtin: true }, instance: true, builtin: true };
    let mut m = HashMap::new();
    m.insert("upcase".to_string(), method(vec![Type::Char], Type::Char));
    m.insert("downcase".to_string(), method(vec![Type::Char], Type::Char));
    m.insert("eq".to_string(), method(vec![Type::Char, Type::Char], Type::Bool));
    m.insert("lt".to_string(), method(vec![Type::Char, Type::Char], Type::Bool));
    m.insert("alphap".to_string(), method(vec![Type::Char], Type::Bool));
    m.insert("digitp".to_string(), method(vec![Type::Char], Type::Bool));
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
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: ty.clone(), public: true, builtin: true }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: Type::Bool, public: true, builtin: true }, instance: true, builtin: true };
    let mut m = HashMap::new();
    for op in ["+", "-", "*", "/", "mod"] {
        m.insert(op.to_string(), binop());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        m.insert(op.to_string(), cmp());
    }
    // `eq` is an alias for `=` here (no identity/value distinction for a
    // scalar) — registered separately so `case`/`equal` can call `eq`
    // uniformly across every type (see `cl-equivalence-catalog.md` §2.1).
    m.insert("eq".to_string(), cmp());
    m
}

/// Built-in arithmetic/comparison/transcendental instance methods for `f64`:
/// the same binary operator set as [`int_assoc`] (unlike integer division,
/// `/`/`mod` follow IEEE-754 — a zero divisor yields `inf`/`NaN`, no panic),
/// plus `expt`(binary, `f64::powf`) and the unary rounding/root family
/// `sqrt`/`floor`/`ceiling`/`round`/`truncate`.
fn float_assoc() -> HashMap<String, AssocFn> {
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64, Type::F64], ret: Type::F64, public: true, builtin: true }, instance: true, builtin: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64, Type::F64], ret: Type::Bool, public: true, builtin: true }, instance: true, builtin: true };
    let unary = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64], ret: Type::F64, public: true, builtin: true }, instance: true, builtin: true };
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
    // See `int_assoc`'s `eq` comment — same alias-for-`=` rationale.
    m.insert("eq".to_string(), cmp());
    m
}

/// A type-parameter reference, e.g. `t` in `Option<T>`'s field list.
fn tvar(name: &str) -> Type {
    Type::Named(Path::root(name), vec![])
}
