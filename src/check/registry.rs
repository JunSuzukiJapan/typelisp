//! Registries of data types (ADTs) and function signatures used by the checker.
//!
//! Type and variant names are stored lowercase because the reader case-folds all
//! symbols. The built-in types `Option<T>` and `Sexpr` are pre-registered.

use std::collections::HashMap;

use crate::Type;

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
    pub params: Vec<Type>,
    pub ret: Type,
}

/// A type-associated function or method (Rust-style; types are *not*
/// namespaces). `instance` is true when the first parameter is the receiver.
#[derive(Clone, Debug)]
pub struct AssocFn {
    pub sig: FnSig,
    pub instance: bool,
}

/// A data-type definition (a sum type / `defstruct`-style ADT). `name` is the
/// fully-qualified (module-prefixed) lowercase type name.
#[derive(Clone, Debug)]
pub struct AdtDef {
    pub name: String,
    /// Type-parameter names (lowercase), e.g. `["t"]` for `Option<T>`.
    pub params: Vec<String>,
    pub variants: Vec<Variant>,
    /// Associated functions / methods, keyed by (unqualified) name.
    pub assoc: HashMap<String, AssocFn>,
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
    /// Types defined directly here, keyed by unqualified name. `AdtDef::name`
    /// holds the fully-qualified name used as the type's identity.
    pub types: HashMap<String, AdtDef>,
    /// Constructor (unqualified) name -> (FQ owning type, variant index).
    pub ctors: HashMap<String, (String, usize)>,
    /// `use` aliases: unqualified name -> absolute path (from the root).
    pub aliases: HashMap<String, Vec<String>>,
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
        let local = def.name.rsplit("::").next().unwrap().to_string();
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
        // Built-in i32 operators (MVP: i32 only; per-type/generic numeric ops
        // come later).
        let int_binop = || FnSig { params: vec![Type::I32, Type::I32], ret: Type::I32 };
        let int_cmp = || FnSig { params: vec![Type::I32, Type::I32], ret: Type::Bool };
        for op in ["+", "-", "*", "/", "mod"] {
            root.fns.insert(op.to_string(), int_binop());
        }
        for op in ["<", "<=", ">", ">=", "=", "/="] {
            root.fns.insert(op.to_string(), int_cmp());
        }
        Registry { root }
    }

    /// Look up a type by its fully-qualified name (e.g. `geo::point`).
    pub fn type_def(&self, fq: &str) -> Option<&AdtDef> {
        let (path, local) = split_fq(fq);
        self.root.module(&path)?.types.get(local)
    }

    /// Mutable lookup of a type by its fully-qualified name.
    pub fn type_def_mut(&mut self, fq: &str) -> Option<&mut AdtDef> {
        let (path, local) = split_fq(fq);
        self.root.module_mut(&path).types.get_mut(local)
    }

    /// Look up a free function by its fully-qualified name.
    pub fn fn_sig(&self, fq: &str) -> Option<&FnSig> {
        let (path, local) = split_fq(fq);
        self.root.module(&path)?.fns.get(local)
    }
}

/// Split a fully-qualified name into its module path and unqualified tail, e.g.
/// `"geo::point"` -> `(["geo"], "point")`, `"option"` -> `([], "option")`.
fn split_fq(fq: &str) -> (Vec<String>, &str) {
    let mut segs: Vec<&str> = fq.split("::").collect();
    let local = segs.pop().unwrap();
    (segs.into_iter().map(|s| s.to_string()).collect(), local)
}

/// `Option<T> = Some(T) | None`.
fn option_def() -> AdtDef {
    AdtDef {
        name: "option".to_string(),
        params: vec!["t".to_string()],
        variants: vec![
            Variant { name: "some".to_string(), fields: vec![tvar("t")] },
            Variant { name: "none".to_string(), fields: vec![] },
        ],
        assoc: HashMap::new(),
    }
}

/// `Result<T, E> = Ok(T) | Err(E)`.
fn result_def() -> AdtDef {
    AdtDef {
        name: "result".to_string(),
        params: vec!["t".to_string(), "e".to_string()],
        variants: vec![
            Variant { name: "ok".to_string(), fields: vec![tvar("t")] },
            Variant { name: "err".to_string(), fields: vec![tvar("e")] },
        ],
        assoc: HashMap::new(),
    }
}

/// The built-in generic error type carrying a message: `Error(String)`. This is
/// the default `E` for fallible built-ins; user-defined error types come later.
fn error_def() -> AdtDef {
    AdtDef {
        name: "error".to_string(),
        params: vec![],
        variants: vec![Variant { name: "error".to_string(), fields: vec![Type::Str] }],
        assoc: HashMap::new(),
    }
}

/// The built-in `Sexpr` sum type (the result type of `read`).
///
/// `Cons` holds two `Option<Sexpr>` fields, since `()` (the empty list, encoded
/// as `None`) can appear as either car or cdr.
fn sexpr_def() -> AdtDef {
    let opt_sexpr = Type::Named("option".to_string(), vec![sexpr()]);
    AdtDef {
        name: "sexpr".to_string(),
        params: vec![],
        variants: vec![
            Variant { name: "int".to_string(), fields: vec![Type::I64] },
            Variant { name: "float".to_string(), fields: vec![Type::F64] },
            Variant { name: "char".to_string(), fields: vec![Type::Char] },
            Variant { name: "bool".to_string(), fields: vec![Type::Bool] },
            Variant { name: "sym".to_string(), fields: vec![Type::Str] },
            Variant { name: "str".to_string(), fields: vec![Type::Str] },
            Variant {
                name: "cons".to_string(),
                fields: vec![opt_sexpr.clone(), opt_sexpr],
            },
        ],
        assoc: HashMap::new(),
    }
}

fn sexpr() -> Type {
    Type::Named("sexpr".to_string(), vec![])
}

/// A type-parameter reference, e.g. `t` in `Option<T>`'s field list.
fn tvar(name: &str) -> Type {
    Type::Named(name.to_string(), vec![])
}
