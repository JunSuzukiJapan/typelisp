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
    /// collected into a single `Vector<elem>` actual argument — see
    /// `Checker::check_call`.
    pub rest: Option<Type>,
}

/// A type-associated function or method (Rust-style; types are *not*
/// namespaces). `instance` is true when the first parameter is the receiver.
#[derive(Clone, Debug)]
pub struct AssocFn {
    pub sig: FnSig,
    pub instance: bool,
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
}

/// A global variable/constant: its type and whether it is assignable.
#[derive(Clone, Debug)]
pub struct VarInfo {
    pub ty: Type,
    pub mutable: bool,
    /// Visible outside its defining module.
    pub public: bool,
}

/// A data-type definition (a sum type / `defstruct`-style ADT). `name` is the
/// fully-qualified (module-prefixed) type [`Path`] used as the type's identity.
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

    /// Register a type here and index its constructors (bare ctor name — first
    /// definer wins so built-in constructors stay reachable).
    pub fn add_type(&mut self, def: AdtDef) {
        for (i, v) in def.variants.iter().enumerate() {
            self.ctors.entry(v.name.clone()).or_insert((def.name.clone(), i));
        }
        let local = def.name.local().to_string();
        self.types.insert(local, def);
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
        root.add_type(sexpr_def());
        root.add_type(hashtable_def());
        root.add_type(vector_def());
        // Primitive value types (i8..usize/f32/f64/bool/char/string) get a
        // method table too, so `defmethod` can target them (see
        // `check_defmethod`/`check_instance_method`, which map a primitive
        // `Type` to its registry `Path` via `prim_type_path`). `string`/`char`/
        // `i32`/`i64`/`f64` additionally get a built-in method set (no
        // `defmethod` body to check; the runtime implementation lives in
        // `eval_builtin_method` in `crate::eval::interp`, the same
        // metadata-only pattern as `hashtable_def`/`vector_def`) — every
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
            root.add_type(AdtDef { name, params: Vec::new(), variants: Vec::new(), assoc, public: true });
        }
        // `random`: the only numeric builtin with no natural receiver to
        // dispatch on (like `gensym`), so it stays a free function.
        root.fns.insert("random".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::I32], ret: Type::I32, public: true });
        // `not`: a plain unary function (no short-circuiting needed, unlike
        // `and`/`or`), so — unlike those two — it doesn't need special-form
        // treatment.
        root.fns.insert("not".to_string(), FnSig { type_params: vec![], rest: None, params: vec![Type::Bool], ret: Type::Bool, public: true });
        // `cons`/`car`/`cdr` operate on `Sexpr` (the cons/nil duality at the
        // type level). `cons` is also reachable as the `Cons` constructor;
        // registering it as a function too lets it be used as a value
        // (e.g. passed to a higher-order function).
        root.fns.insert("cons".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: sexpr(), public: true });
        root.fns.insert("car".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: sexpr(), public: true });
        root.fns.insert("cdr".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr()], ret: sexpr(), public: true });
        // `set-car`/`set-cdr` (CL `rplaca`/`rplacd`): in-place mutation of an
        // existing cons cell, backed by `mem::Heap::set_car`/`set_cdr` (already
        // implemented at the heap layer, just not wired up to a language-level
        // name until now). Panics on a non-`Cons` `Sexpr`, matching `car`/`cdr`.
        // This is the prerequisite `nconc`/`nreverse` (the prelude's destructive
        // list operations) build on.
        root.fns.insert("set-car".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Unit, public: true });
        root.fns.insert("set-cdr".to_string(), FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Unit, public: true });
        // `gensym`: a fresh `Sexpr::Sym` on every call, for macro hygiene
        // workarounds (see `Interp`'s `gensym_counter` for the caveat that
        // these are collision-*resistant*, not truly unforgeable — typelisp
        // symbols are always interned/permanent, there is no uninterned-symbol
        // concept to give a CL-style absolute guarantee).
        root.fns.insert("gensym".to_string(), FnSig { type_params: vec![], rest: None, params: vec![], ret: sexpr(), public: true });
        #[cfg(feature = "compile")]
        register_compile_builtins(&mut root);
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
    m.insert("eq".to_string(), AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![sexpr(), sexpr()], ret: Type::Bool, public: true }, instance: true });
    m
}

fn bool_assoc() -> HashMap<String, AssocFn> {
    let mut m = HashMap::new();
    m.insert(
        "eq".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::Bool, Type::Bool], ret: Type::Bool, public: true }, instance: true },
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
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![], ret: hashtable_ty(), public: true }, instance: false },
    );
    assoc.insert(
        "get".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k")], ret: option_of(tvar("v")), public: true },
            instance: true,
        },
    );
    assoc.insert(
        "set".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k"), tvar("v")], ret: Type::Unit, public: true },
            instance: true,
        },
    );
    assoc.insert(
        "remove".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty(), tvar("k")], ret: option_of(tvar("v")), public: true },
            instance: true,
        },
    );
    assoc.insert(
        "count".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::I32, public: true }, instance: true },
    );
    assoc.insert(
        "clear".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![hashtable_ty()], ret: Type::Unit, public: true }, instance: true },
    );
    AdtDef {
        name: Path::root("hashtable"),
        params: vec!["k".to_string(), "v".to_string()],
        variants: vec![],
        assoc,
        public: true,
    }
}

fn vector_ty() -> Type {
    Type::Named(Path::root("vector"), vec![tvar("t")])
}

/// `Vector<T>`: a builtin (Rust-implemented) growable, indexable array, with no
/// constructors of its own (built via the static `new`, not pattern-matched).
/// All methods here are metadata only — there is no `defmethod` body to
/// check; the runtime implementation lives in `eval_builtin_method` in
/// `crate::eval::interp`. `get`/`set` return/take `T` directly rather than
/// `Option<T>` — an out-of-range index is a runtime panic (see
/// `cl-equivalence-catalog.md` §2.2 b), matching `car`/`cdr`'s "type system
/// can't express the bound, so it's a panic" precedent.
fn vector_def() -> AdtDef {
    let mut assoc = HashMap::new();
    assoc.insert(
        "new".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![Type::I32, tvar("t")], ret: vector_ty(), public: true },
            instance: false,
        },
    );
    assoc.insert(
        "get".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), Type::I32], ret: tvar("t"), public: true },
            instance: true,
        },
    );
    assoc.insert(
        "set".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), Type::I32, tvar("t")], ret: Type::Unit, public: true },
            instance: true,
        },
    );
    assoc.insert(
        "length".to_string(),
        AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty()], ret: Type::I32, public: true }, instance: true },
    );
    assoc.insert(
        "push".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty(), tvar("t")], ret: Type::Unit, public: true },
            instance: true,
        },
    );
    assoc.insert(
        "pop".to_string(),
        AssocFn {
            sig: FnSig { type_params: vec![], rest: None, params: vec![vector_ty()], ret: option_of(tvar("t")), public: true },
            instance: true,
        },
    );
    AdtDef { name: Path::root("vector"), params: vec!["t".to_string()], variants: vec![], assoc, public: true }
}

/// Built-in `String` instance methods ([cl-equivalence-catalog.md](../../../docs/cl-equivalence-catalog.md)
/// §2.2 d). All char/index arguments and `length` count Unicode scalar values
/// (`char`s), not bytes. `ref`/`substring` panic on an out-of-range index —
/// the type system can't express the bound, the same precedent as
/// `car`/`cdr` on a non-`Cons` `Sexpr`. `upcase`/`downcase` are ASCII-only
/// (`str::to_ascii_uppercase`/`lowercase`), avoiding Unicode case mappings
/// that can change a string's length (e.g. German `ß` -> `SS`).
fn string_assoc() -> HashMap<String, AssocFn> {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true }, instance: true };
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
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true }, instance: true };
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
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: ty.clone(), public: true }, instance: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![ty.clone(), ty.clone()], ret: Type::Bool, public: true }, instance: true };
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
    let binop = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64, Type::F64], ret: Type::F64, public: true }, instance: true };
    let cmp = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64, Type::F64], ret: Type::Bool, public: true }, instance: true };
    let unary = || AssocFn { sig: FnSig { type_params: vec![], rest: None, params: vec![Type::F64], ret: Type::F64, public: true }, instance: true };
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

#[cfg(feature = "compile")]
fn result_of(t: Type, e: Type) -> Type {
    Type::Named(Path::root("result"), vec![t, e])
}

#[cfg(feature = "compile")]
fn vector_of(t: Type) -> Type {
    Type::Named(Path::root("vector"), vec![t])
}

#[cfg(feature = "compile")]
fn error_ty() -> Type {
    Type::Named(Path::root("error"), vec![])
}

/// `AstExpr`: a typelisp-inspectable mirror of (a Phase-1/2 subset of)
/// `check::ast::Expr`, used so the typelisp-written compiler
/// (`crate::compile::compiler_source`) can `match` over a function's checked
/// body — see `crate::compile::ast_bridge::typed_to_ast`, whose variant
/// indices must match this definition's `variants` order exactly. Phase 1/2e
/// ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) only bridges integer/
/// bool/float literals, parameter references, `if`, `let`, and `i64`/`f64`'s
/// binary arithmetic/comparison instance methods — just enough to compile a
/// function like `(defun max2 ((a i64) (b i64)) i64 (if (< a b) b a))`.
/// `afloat`/`alet`/`acall`/`achar`/`anil`/`acons`/`acar`/`acdr`/`anullp` are
/// appended last (Phase 2c/2e/2f/2d/3) rather than inserted in
/// literal-grouping order with `aint`/`abool`, since these indices are a
/// stable wire format between this definition and `ast_bridge`'s constants
/// — reordering existing ones would silently break already-working variants.
/// `alet`'s three `Vector` fields are parallel: `names[i]`'s value is
/// `values[i]` (all checked against the **outer** scope, CL `let`
/// semantics — `crate::compile::ast_bridge`'s doc comment), then `body`
/// (one or more forms, only the last one's value escapes the `let`) is
/// checked with all of `names` newly in scope.
/// `acall`'s callee is always a top-level `defun` already registered in
/// `Interp.compiled` by the time `ast_bridge::typed_to_ast` bridges it
/// (Phase 2f) — see that function's doc comment on why an uncompiled callee
/// makes the whole bridge fail (`None`) rather than reaching `compile-value`
/// at all. `anil`/`acons`/`acar`/`acdr`/`anullp` (Phase 3) are `Sexpr`'s
/// `Nil` literal and `cons`/`car`/`cdr`/`null?` — the only `Value` shapes
/// `compile` supports (see `crate::eval::interp::value_to_ptr`'s doc comment).
#[cfg(feature = "compile")]
fn ast_expr_def() -> AdtDef {
    let t = ast_expr_ty();
    AdtDef {
        name: Path::root("astexpr"),
        params: vec![],
        variants: vec![
            Variant { name: "aint".to_string(), fields: vec![Type::I64] },
            Variant { name: "abool".to_string(), fields: vec![Type::Bool] },
            Variant { name: "avar".to_string(), fields: vec![Type::Str] },
            Variant { name: "aif".to_string(), fields: vec![t.clone(), t.clone(), t.clone()] },
            Variant { name: "abinop".to_string(), fields: vec![Type::Str, t.clone(), t.clone()] },
            Variant { name: "afloat".to_string(), fields: vec![Type::F64] },
            Variant {
                name: "alet".to_string(),
                fields: vec![vector_of(Type::Str), vector_of(t.clone()), vector_of(t.clone())],
            },
            Variant { name: "acall".to_string(), fields: vec![Type::Str, vector_of(t.clone())] },
            Variant { name: "achar".to_string(), fields: vec![Type::Char] },
            Variant { name: "anil".to_string(), fields: vec![] },
            Variant { name: "acons".to_string(), fields: vec![t.clone(), t.clone()] },
            Variant { name: "acar".to_string(), fields: vec![t.clone()] },
            Variant { name: "acdr".to_string(), fields: vec![t.clone()] },
            Variant { name: "anullp".to_string(), fields: vec![t] },
        ],
        assoc: HashMap::new(),
        public: true,
    }
}

#[cfg(feature = "compile")]
fn ast_expr_ty() -> Type {
    Type::Named(Path::root("astexpr"), vec![])
}

#[cfg(feature = "compile")]
fn llvm_module_ty() -> Type {
    Type::Named(Path::root("llvmmodule"), vec![])
}

#[cfg(feature = "compile")]
fn llvm_builder_ty() -> Type {
    Type::Named(Path::root("llvmbuilder"), vec![])
}

#[cfg(feature = "compile")]
fn llvm_function_ty() -> Type {
    Type::Named(Path::root("llvmfunction"), vec![])
}

#[cfg(feature = "compile")]
fn llvm_basic_block_ty() -> Type {
    Type::Named(Path::root("llvmbasicblock"), vec![])
}

#[cfg(feature = "compile")]
fn llvm_value_ty() -> Type {
    Type::Named(Path::root("llvmvalue"), vec![])
}

/// `LlvmModule`: a builtin (Rust-implemented, inkwell-backed) opaque handle.
/// All methods are metadata only — the runtime implementation lives in
/// `eval_llvm_builtin_method` in `crate::eval::interp`, the same
/// metadata-only pattern as [`hashtable_def`]/[`vector_def`].
///
/// `add-function` no longer takes an arity: since Phase 2
/// ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) every compiled function
/// shares one LLVM-level signature regardless of its logical arity — the
/// unified `TlValue` ABI `i32 (ptr args, i32 argc, ptr out)` — so there is no
/// per-arity type to build (see `crate::eval::interp::llvm_module_add_function`).
///
/// `get-or-declare-function` (Phase 2f, direct calls between compiled
/// functions): returns the existing declaration if `name` was already
/// added to this module (so calling the same callee twice from one compiled
/// function reuses one declaration instead of LLVM renaming a second one),
/// otherwise declares it fresh with the same unified signature `add-function`
/// uses, but with no entry block — a bare external declaration `compile`
/// later resolves to the callee's real JIT'd address (see
/// `Interp::builtin_llvm_finish_compile`'s `add_global_mapping` pass).
#[cfg(feature = "compile")]
fn llvm_module_def() -> AdtDef {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true }, instance: true };
    let mut assoc = HashMap::new();
    assoc.insert("add-function".to_string(), method(vec![llvm_module_ty(), Type::Str], llvm_function_ty()));
    assoc.insert("get-or-declare-function".to_string(), method(vec![llvm_module_ty(), Type::Str], llvm_function_ty()));
    assoc.insert("verify".to_string(), method(vec![llvm_module_ty()], result_of(Type::Unit, error_ty())));
    assoc.insert("dump".to_string(), method(vec![llvm_module_ty()], Type::Unit));
    AdtDef { name: Path::root("llvmmodule"), params: vec![], variants: vec![], assoc, public: true }
}

/// `LlvmFunction`: a builtin opaque handle to a declared/defined LLVM function.
/// Same metadata-only pattern as [`llvm_module_def`]. No more `get-param` —
/// under the `TlValue` ABI a function's logical arguments aren't separate
/// LLVM-level parameters anymore, so reading one is an IR-building operation
/// (`LlvmBuilder::load-arg`), not a pure metadata lookup.
#[cfg(feature = "compile")]
fn llvm_function_def() -> AdtDef {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true }, instance: true };
    let mut assoc = HashMap::new();
    assoc.insert("append-block".to_string(), method(vec![llvm_function_ty(), Type::Str], llvm_basic_block_ty()));
    AdtDef { name: Path::root("llvmfunction"), params: vec![], variants: vec![], assoc, public: true }
}

/// `LlvmBuilder`: a builtin opaque handle wrapping an LLVM IR builder
/// positioned at some basic block. `build-op` covers both `i64` arithmetic
/// (`+ - * / mod`) and comparison (`< <= > >= = /=`) — the same operator
/// strings `crate::compile::ast_bridge`'s `abinop` carries — so the
/// typelisp-written compiler can forward an operator string straight through
/// without its own dispatch table; see `crate::eval::interp::llvm_builder_build_op`.
///
/// `load-arg`/`build-ret` both take an `LlvmFunction` alongside the builder:
/// under the unified `TlValue` ABI (Phase 2), reading argument `i` means
/// indexing the function's first (`args`) parameter, and returning means
/// writing through its third (`out`) parameter — see
/// `crate::eval::interp::{llvm_builder_load_arg,llvm_builder_build_ret}`.
/// `load-arg-bool`/`build-ret-bool` are their `bool`-narrowing counterparts
/// (Phase 2b, mixed `i64`/`bool` signatures) — see
/// `crate::eval::interp::{llvm_builder_load_arg_bool,llvm_builder_build_ret_bool}`.
/// `load-arg-f64`/`build-ret-f64` are the `f64`-reinterpreting counterparts
/// (Phase 2c) — see
/// `crate::eval::interp::{llvm_builder_load_arg_f64,llvm_builder_build_ret_f64}`.
#[cfg(feature = "compile")]
fn llvm_builder_def() -> AdtDef {
    let method = |params: Vec<Type>, ret: Type| AssocFn { sig: FnSig { type_params: vec![], rest: None, params, ret, public: true }, instance: true };
    let mut assoc = HashMap::new();
    assoc.insert("position-at-end".to_string(), method(vec![llvm_builder_ty(), llvm_basic_block_ty()], Type::Unit));
    assoc.insert("load-arg".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), Type::I32], llvm_value_ty()));
    assoc.insert("load-arg-bool".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), Type::I32], llvm_value_ty()));
    assoc.insert("load-arg-f64".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), Type::I32], llvm_value_ty()));
    // `load-arg-char`/`build-ret-char` (Phase 2d): `char`'s `i32`-narrowing
    // counterparts to `load-arg`/`build-ret`, the same truncate/zext shape
    // `-bool` uses (`bool` truncates/zext to `i1`, `char` to `i32`) — see
    // `crate::eval::interp::{llvm_builder_load_arg_char,llvm_builder_build_ret_char}`.
    assoc.insert("load-arg-char".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), Type::I32], llvm_value_ty()));
    // `load-arg-sexpr`/`build-ret-sexpr` (Phase 3): `Sexpr`'s pointer-
    // reinterpreting counterparts to `load-arg`/`build-ret`, the same
    // bit-reinterpret (not narrow/widen) shape `-f64` uses (`int_to_ptr`/
    // `ptr_to_int` instead of `bitcast`, since LLVM has no pointer<->i64
    // bitcast) — see
    // `crate::eval::interp::{llvm_builder_load_arg_sexpr,llvm_builder_build_ret_sexpr}`.
    assoc.insert("load-arg-sexpr".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), Type::I32], llvm_value_ty()));
    assoc.insert("build-op".to_string(), method(vec![llvm_builder_ty(), Type::Str, llvm_value_ty(), llvm_value_ty()], llvm_value_ty()));
    assoc.insert(
        "build-cond-br".to_string(),
        method(vec![llvm_builder_ty(), llvm_value_ty(), llvm_basic_block_ty(), llvm_basic_block_ty()], Type::Unit),
    );
    assoc.insert("build-br".to_string(), method(vec![llvm_builder_ty(), llvm_basic_block_ty()], Type::Unit));
    assoc.insert("build-ret".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), llvm_value_ty()], Type::Unit));
    assoc.insert("build-ret-bool".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), llvm_value_ty()], Type::Unit));
    assoc.insert("build-ret-f64".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), llvm_value_ty()], Type::Unit));
    assoc.insert("build-ret-char".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), llvm_value_ty()], Type::Unit));
    assoc.insert("build-ret-sexpr".to_string(), method(vec![llvm_builder_ty(), llvm_function_ty(), llvm_value_ty()], Type::Unit));
    // `build-call`/`build-call-bool`/`build-call-f64`/`build-call-char` (Phase 2f/2d): packs `args`
    // into a stack-allocated `TlValue` array and emits a direct `call` to
    // `callee` (a declaration from `LlvmModule::get-or-declare-function`)
    // under the unified ABI, then unpacks the `out` slot per the callee's
    // known return type — see `crate::eval::interp::llvm_builder_build_call`.
    assoc.insert(
        "build-call".to_string(),
        method(vec![llvm_builder_ty(), llvm_function_ty(), vector_of(llvm_value_ty())], llvm_value_ty()),
    );
    assoc.insert(
        "build-call-bool".to_string(),
        method(vec![llvm_builder_ty(), llvm_function_ty(), vector_of(llvm_value_ty())], llvm_value_ty()),
    );
    assoc.insert(
        "build-call-f64".to_string(),
        method(vec![llvm_builder_ty(), llvm_function_ty(), vector_of(llvm_value_ty())], llvm_value_ty()),
    );
    assoc.insert(
        "build-call-char".to_string(),
        method(vec![llvm_builder_ty(), llvm_function_ty(), vector_of(llvm_value_ty())], llvm_value_ty()),
    );
    assoc.insert(
        "build-call-sexpr".to_string(),
        method(vec![llvm_builder_ty(), llvm_function_ty(), vector_of(llvm_value_ty())], llvm_value_ty()),
    );
    // `build-cons`/`build-car`/`build-cdr`/`push-root`/`pop-root` (Phase 3):
    // `Sexpr`'s cons-cell operations and GC-rooting primitives, each a
    // direct `call` to a fixed Rust runtime shim rather than inline IR
    // (`cons` may trigger a GC; `car`/`cdr` never do but go through the same
    // shim layer so JIT'd IR never has to know `Cell`'s layout) — see
    // `crate::eval::interp::{llvm_builder_build_cons,llvm_builder_build_car,
    // llvm_builder_build_cdr,llvm_builder_build_push_root,llvm_builder_build_pop_root}`.
    // `module`/`f` are needed alongside the builder: `module` to declare the
    // shim (reusing an existing declaration if `compile` already added one),
    // `f` to read the enclosing function's `heap` parameter (the unified
    // ABI's fourth parameter, `crate::eval::interp::heap_param_of`).
    assoc.insert(
        "build-cons".to_string(),
        method(vec![llvm_builder_ty(), llvm_module_ty(), llvm_function_ty(), llvm_value_ty(), llvm_value_ty()], llvm_value_ty()),
    );
    assoc.insert(
        "build-car".to_string(),
        method(vec![llvm_builder_ty(), llvm_module_ty(), llvm_function_ty(), llvm_value_ty()], llvm_value_ty()),
    );
    assoc.insert(
        "build-cdr".to_string(),
        method(vec![llvm_builder_ty(), llvm_module_ty(), llvm_function_ty(), llvm_value_ty()], llvm_value_ty()),
    );
    // `build-nullp` needs neither `module` nor `f` — it's a plain
    // pointer-null comparison, never touching the heap or a runtime shim.
    assoc.insert("build-nullp".to_string(), method(vec![llvm_builder_ty(), llvm_value_ty()], llvm_value_ty()));
    assoc.insert(
        "push-root".to_string(),
        method(vec![llvm_builder_ty(), llvm_module_ty(), llvm_function_ty(), llvm_value_ty()], Type::Unit),
    );
    assoc.insert("pop-root".to_string(), method(vec![llvm_builder_ty(), llvm_module_ty(), llvm_function_ty()], Type::Unit));
    AdtDef { name: Path::root("llvmbuilder"), params: vec![], variants: vec![], assoc, public: true }
}

/// `LlvmBasicBlock`/`LlvmValue`: opaque handles with no methods of their own
/// (Phase 1 only ever passes them as arguments to `LlvmFunction`/`LlvmBuilder`
/// methods) — registered purely so they have a type identity to check against.
#[cfg(feature = "compile")]
fn llvm_basic_block_def() -> AdtDef {
    AdtDef { name: Path::root("llvmbasicblock"), params: vec![], variants: vec![], assoc: HashMap::new(), public: true }
}

#[cfg(feature = "compile")]
fn llvm_value_def() -> AdtDef {
    AdtDef { name: Path::root("llvmvalue"), params: vec![], variants: vec![], assoc: HashMap::new(), public: true }
}

/// Registers the LLVM-compiler builtins (types and free functions) into
/// `root`. Split out of [`Registry::with_builtins`] purely for readability —
/// this is the one block of registration calls gated by the `compile`
/// feature. See `crate::compile` and [docs/TODO.md](../../docs/TODO.md)
/// 「ステップ5」for the overall design.
#[cfg(feature = "compile")]
fn register_compile_builtins(root: &mut Namespace) {
    root.add_type(ast_expr_def());
    root.add_type(llvm_module_def());
    root.add_type(llvm_function_def());
    root.add_type(llvm_builder_def());
    root.add_type(llvm_basic_block_def());
    root.add_type(llvm_value_def());

    let free = |params: Vec<Type>, ret: Type| FnSig { type_params: vec![], rest: None, params, ret, public: true };
    // `ast-params`/`ast-body`: the typed-AST bridge (`compile::ast_bridge`).
    // `None` means "not a `defun`, or not all `i64`/`bool` params/return"
    // (Phase 2b's restricted scope) — `compile` then refuses the function
    // outright.
    root.fns.insert(
        "ast-params".to_string(),
        free(vec![Type::Str], option_of(Type::Named(Path::root("vector"), vec![Type::Str]))),
    );
    root.fns.insert("ast-body".to_string(), free(vec![Type::Str], option_of(ast_expr_ty())));
    // `param-is-bool`/`ret-is-bool` (Phase 2b) and `param-is-f64`/`ret-is-f64`
    // (Phase 2c): per-parameter/return type tags (`AstExpr` itself carries no
    // type info per node), letting `compile` pick `load-arg`/`build-ret` vs
    // their `-bool`/`-f64` counterparts — see
    // `crate::eval::interp::Interp::{builtin_param_is_bool,builtin_ret_is_bool,
    // builtin_param_is_f64,builtin_ret_is_f64}`.
    root.fns.insert("param-is-bool".to_string(), free(vec![Type::Str, Type::I32], Type::Bool));
    root.fns.insert("ret-is-bool".to_string(), free(vec![Type::Str], Type::Bool));
    root.fns.insert("param-is-f64".to_string(), free(vec![Type::Str, Type::I32], Type::Bool));
    root.fns.insert("ret-is-f64".to_string(), free(vec![Type::Str], Type::Bool));
    // `param-is-char`/`ret-is-char` (Phase 2d): same role as the `-bool`/
    // `-f64` pairs above, for `char` (stored as an `i32`-wide `IntValue`,
    // narrowed/widened only at the `TlValue` boundary — see
    // `crate::eval::interp::Interp::{builtin_param_is_char,builtin_ret_is_char}`).
    root.fns.insert("param-is-char".to_string(), free(vec![Type::Str, Type::I32], Type::Bool));
    root.fns.insert("ret-is-char".to_string(), free(vec![Type::Str], Type::Bool));
    // `param-is-sexpr`/`ret-is-sexpr` (Phase 3): same role as the pairs
    // above, for `Sexpr` (stored as a plain pointer, narrowed/widened only
    // at the `TlValue` boundary — see
    // `crate::eval::interp::Interp::{builtin_param_is_sexpr,builtin_ret_is_sexpr}`).
    root.fns.insert("param-is-sexpr".to_string(), free(vec![Type::Str, Type::I32], Type::Bool));
    root.fns.insert("ret-is-sexpr".to_string(), free(vec![Type::Str], Type::Bool));
    root.fns.insert("llvm-new-module".to_string(), free(vec![Type::Str], llvm_module_ty()));
    root.fns.insert("llvm-new-builder".to_string(), free(vec![], llvm_builder_ty()));
    root.fns.insert("llvm-const-i64".to_string(), free(vec![Type::I64], llvm_value_ty()));
    root.fns.insert("llvm-const-bool".to_string(), free(vec![Type::Bool], llvm_value_ty()));
    root.fns.insert("llvm-const-f64".to_string(), free(vec![Type::F64], llvm_value_ty()));
    root.fns.insert("llvm-const-char".to_string(), free(vec![Type::Char], llvm_value_ty()));
    root.fns.insert("llvm-const-nil".to_string(), free(vec![], llvm_value_ty()));
    // `is-sexpr-value` (Phase 3): whether an `LlvmValue`'s actual LLVM kind
    // is a pointer — `compile`'s rooting discipline uses this to find which
    // entries of a `Vector<LlvmValue>` (parameters/`let` bindings) need
    // `push-root`/`pop-root` around `Sexpr`-allocating operations — see
    // `crate::eval::interp::llvm_value_is_sexpr`.
    root.fns.insert("is-sexpr-value".to_string(), free(vec![llvm_value_ty()], Type::Bool));
    // `llvm-finish-compile`: JITs `module` and registers `name` (a
    // `arity`-ary, all-`i64` function) so ordinary calls to it dispatch to
    // the compiled native code instead of tree-walking — see
    // `Interp::call_compiled`. Returns `bool` rather than `Unit` so `compile`
    // (`crate::compile::compiler_source`) can write its own return type as
    // plain text (`Result<bool,Error>` — `Result<(),Error>` cannot be
    // written in source, since `()` is a list-delimiter pair, not a token
    // character, so it can't appear inside a generic's `<...>` symbol token).
    root.fns.insert(
        "llvm-finish-compile".to_string(),
        free(vec![llvm_module_ty(), Type::Str, Type::I32], result_of(Type::Bool, error_ty())),
    );
}
