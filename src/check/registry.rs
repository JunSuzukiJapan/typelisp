//! Registries of data types (ADTs) and function signatures used by the checker.
//!
//! Type and variant names are stored lowercase because the reader case-folds all
//! symbols. The built-in types `Option<T>` and `Sexpr` are pre-registered.

use std::collections::{HashMap, HashSet};

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

/// The checker's symbol tables. Modules are namespaces; types are not (they own
/// associated items in [`AdtDef::assoc`]). All keys are fully-qualified
/// lowercase `::`-joined names.
pub struct Registry {
    pub adts: HashMap<String, AdtDef>,
    /// Constructor name -> (owning FQ type name, variant index). Indexed under
    /// both the bare name (first definer wins) and `fqtype::ctor`.
    pub variant_index: HashMap<String, (String, usize)>,
    pub fns: HashMap<String, FnSig>,
    /// Known fully-qualified module paths.
    pub modules: HashSet<String>,
    /// `use` injections: a current-namespace-qualified name -> FQ target.
    pub aliases: HashMap<String, String>,
}

impl Registry {
    /// A registry pre-loaded with the built-in `Option`/`Result`/`Error`/`Sexpr`
    /// types (all at the root namespace).
    pub fn with_builtins() -> Registry {
        let mut reg = Registry {
            adts: HashMap::new(),
            variant_index: HashMap::new(),
            fns: HashMap::new(),
            modules: HashSet::new(),
            aliases: HashMap::new(),
        };
        reg.add_adt(option_def());
        reg.add_adt(result_def());
        reg.add_adt(error_def());
        reg.add_adt(sexpr_def());
        reg.add_builtin_fns();
        reg
    }

    /// Register the built-in i32 arithmetic/comparison operators. (MVP: i32
    /// only; per-type operators / generic numeric methods come later.)
    fn add_builtin_fns(&mut self) {
        let int_binop = || FnSig { params: vec![Type::I32, Type::I32], ret: Type::I32 };
        let int_cmp = || FnSig { params: vec![Type::I32, Type::I32], ret: Type::Bool };
        for op in ["+", "-", "*", "/", "mod"] {
            self.fns.insert(op.to_string(), int_binop());
        }
        for op in ["<", "<=", ">", ">=", "=", "/="] {
            self.fns.insert(op.to_string(), int_cmp());
        }
    }

    /// Register a data type and index its constructors (bare name — first
    /// definer wins, keeping built-in constructors reachable — plus the
    /// fully-qualified `fqtype::ctor` key).
    pub fn add_adt(&mut self, def: AdtDef) {
        for (i, v) in def.variants.iter().enumerate() {
            self.variant_index
                .entry(v.name.clone())
                .or_insert((def.name.clone(), i));
            self.variant_index
                .insert(format!("{}::{}", def.name, v.name), (def.name.clone(), i));
        }
        self.adts.insert(def.name.clone(), def);
    }
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
