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
