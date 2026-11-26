//! What the REPL's `:describe` says about a definition: what kind it is, its
//! type or shape, where it was written and its documentation.
//!
//! The name is resolved as `(documentation name)` resolves it
//! ([`super::documentation::definition`]), so the two cannot disagree on
//! which definition a name names.

use crate::check::CompileTarget;
use crate::types::{Path, Type, TypeParam};
use crate::{Error, Heap, Loc, MacroExpander, Value};
use typelisp_mem::symbols::NsId;

use super::expr::{Cx, Notes};
use super::registry::{AdtKind, Definition, FnDef, Registry, TraitBound, TypeEntry, ValueDef};
use super::scope::TypeScope;

/// A definition as `:describe` shows it.
#[derive(Clone, Debug)]
pub struct Description {
    /// The definition's path and what it is: `user::inner is a function`.
    pub title: String,
    /// The lines under the title: its type, fields, methods and so on.
    pub lines: Vec<String>,
    /// Its documentation string, if it was written with one.
    pub doc: Option<String>,
    /// Where it was written; `None` for one the runtime provides.
    pub at: Option<Loc>,
    /// The definition's name, without its module.
    pub name: String,
    /// The function or method, for what asks about calls to it.
    pub target: Option<CompileTarget>,
}

/// Describes the definition the name `v` (a symbol or a `::` path) names,
/// read in the module `ns`.
pub(crate) fn describe(reg: &Registry, expander: &dyn MacroExpander, notes: &Notes, heap: &Heap, ns: NsId, v: Value) -> Result<Description, Error> {
    let scope = TypeScope::empty();
    let cx = Cx::new(reg, expander, ns, &scope, None, notes);
    let def = super::documentation::definition(&cx, heap, v, ":describe", MESSAGE)?;
    let (title, lines, name, target) = match def {
        Definition::Fn(path) => {
            let f = reg.fn_def(&path).expect("a function's definition is in the registry");
            let what = if f.ffi { "a C function" } else if f.builtin { "a builtin function" } else { "a function" };
            (format!("{} is {}", path, what), fn_lines(f), path.name_symbol().name().to_string(), Some(CompileTarget::Fn(path)))
        }
        Definition::Var(path) => {
            let Some(ValueDef::Var(var)) = reg.values.get(&path) else { unreachable!("a variable's definition is in the registry") };
            let what = if var.mutable { "a global variable" } else { "a constant" };
            (format!("{} is {}", path, what), vec![format!("type: {}", var.ty)], path.name_symbol().name().to_string(), None)
        }
        Definition::Macro(path) => {
            let what = match reg.values.get(&path) {
                Some(ValueDef::SymbolMacro { .. }) => "a symbol macro",
                _ => "a macro",
            };
            (format!("{} is {}", path, what), Vec::new(), path.name_symbol().name().to_string(), None)
        }
        Definition::Type(path) => {
            let entry = reg.types.get(&path).expect("a type's definition is in the registry");
            let (what, lines) = type_lines(reg, path, entry);
            (format!("{} is {}", path, what), lines, path.name_symbol().name().to_string(), None)
        }
        Definition::Trait(path) => {
            let Some(TypeEntry::Trait(t)) = reg.types.get(&path) else { unreachable!("a trait's definition is in the registry") };
            let mut lines = Vec::new();
            if !t.params.is_empty() {
                lines.push(format!("type parameters: {}", param_names(&t.params)));
            }
            if !t.supertraits.is_empty() {
                lines.push(format!("supertraits: {}", bound_names(&t.supertraits)));
            }
            lines.push("methods:".to_string());
            for (m, f) in &t.methods {
                let default = if t.defaults.contains(m) { " (has a default body)" } else { "" };
                lines.push(format!("  {}: {}{}", m.name(), fn_type(f), default));
            }
            let mut types: Vec<String> = reg.trait_impls.iter().filter(|((tr, _), _)| *tr == path).map(|((_, ty), _)| ty.to_string()).collect();
            if let Some(b) = reg.blanket_impls.get(&path) {
                types.push(format!("every type that is {}", bound_names(&b.bounds)));
            }
            if !types.is_empty() {
                types.sort();
                lines.push(format!("implemented by: {}", types.join(", ")));
            }
            (format!("{} is a trait", path), lines, path.name_symbol().name().to_string(), None)
        }
        Definition::Method(owner, m) => {
            let method = reg.method(&owner, m).expect("a method's definition is in the registry");
            let what = if method.instance { "a method" } else { "an associated function" };
            (format!("{}::{} is {} of {}", owner, m.name(), what, owner), fn_lines(&method.sig), m.name().to_string(), Some(CompileTarget::Method(owner, m)))
        }
        Definition::TraitMethod(owner, m) => {
            let Some(TypeEntry::Trait(t)) = reg.types.get(&owner) else { unreachable!("a trait's definition is in the registry") };
            let f = t.method(m).expect("a trait method's definition is in its trait");
            (format!("{}::{} is a method of the trait {}", owner, m.name(), owner), fn_lines(f), m.name().to_string(), None)
        }
    };
    Ok(Description { title, lines, doc: reg.docs.get(&def).cloned(), at: reg.locs.get(&def).cloned(), name, target })
}

const MESSAGE: &str = ":describe: expected a symbol or path naming a definition, e.g. :describe foo or :describe point::x";

/// A function's type, and what its type does not show: its type
/// parameters and their bounds, and its `&optional` and `&key` parameters.
fn fn_lines(f: &FnDef) -> Vec<String> {
    let mut lines = vec![format!("type: {}", fn_type(f))];
    if !f.type_params.is_empty() {
        lines.push(format!("type parameters: {}", param_names(&f.type_params)));
    }
    for (param, bounds) in &f.bounds {
        lines.push(format!("where {}: {}", param.name.name(), bound_names(bounds)));
    }
    for p in &f.optionals {
        lines.push(format!("&optional {}: {}", p.name.name(), p.decl_ty));
    }
    for p in &f.keys {
        lines.push(format!("&key :{}: {}", p.name.name(), p.decl_ty));
    }
    lines
}

/// The function type `f` has, as `:type` writes it.
pub(crate) fn fn_type(f: &FnDef) -> Type {
    Type::Fn(f.params.clone(), f.rest.clone().map(Box::new), Box::new(f.ret.clone()))
}

/// What a type is, and its fields or variants, methods and traits.
fn type_lines(reg: &Registry, path: Path, entry: &TypeEntry) -> (&'static str, Vec<String>) {
    let mut lines = Vec::new();
    let what = match entry {
        TypeEntry::Type(def) => {
            if !def.params.is_empty() {
                lines.push(format!("type parameters: {}", param_names(&def.params)));
            }
            match def.kind {
                AdtKind::Struct if !def.fields.is_empty() => {
                    lines.push("fields:".to_string());
                    for f in &def.fields {
                        lines.push(format!("  {}: {}", f.name.name(), f.ty));
                    }
                }
                AdtKind::Sum if !def.variants.is_empty() => {
                    lines.push("variants:".to_string());
                    for v in &def.variants {
                        let fields: Vec<String> = v.fields.iter().map(|t| format!(" {}", t)).collect();
                        lines.push(format!("  ({}{})", v.name.name(), fields.concat()));
                    }
                }
                AdtKind::Struct | AdtKind::Sum => {}
            }
            match (def.builtin, def.kind) {
                (true, _) => "a builtin type",
                (false, AdtKind::Struct) => "a struct",
                (false, AdtKind::Sum) => "an enum",
            }
        }
        TypeEntry::Alias { params, body, .. } => {
            if !params.is_empty() {
                lines.push(format!("type parameters: {}", param_names(params)));
            }
            lines.push(format!("= {}", body));
            "a type alias"
        }
        TypeEntry::CStruct(def) => {
            lines.push(format!("size: {} bytes", def.size));
            "a C struct"
        }
        TypeEntry::Trait(_) => unreachable!("a trait is described as one"),
    };
    let mut methods: Vec<String> = reg.methods_of(path).map(|(m, _)| m.name().to_string()).collect();
    methods.sort();
    methods.dedup();
    if !methods.is_empty() {
        lines.push(format!("methods: {}", methods.join(" ")));
    }
    let mut traits: Vec<String> = reg.trait_impls.iter().filter(|((_, ty), _)| *ty == path).map(|((tr, _), _)| tr.to_string()).collect();
    if !traits.is_empty() {
        traits.sort();
        lines.push(format!("implements: {}", traits.join(", ")));
    }
    (what, lines)
}

fn param_names(params: &[TypeParam]) -> String {
    params.iter().map(|p| p.name.name().to_string()).collect::<Vec<_>>().join(", ")
}

fn bound_names(bounds: &[TraitBound]) -> String {
    bounds.iter().map(|b| b.trait_path.to_string()).collect::<Vec<_>>().join(" + ")
}

/// What a value of a type is made of, as the REPL's `:inspect` opens it.
#[derive(Clone, Debug)]
pub enum Parts {
    /// A struct: each field's name and type.
    Fields(Vec<(String, Type)>),
    /// A sum type: each variant's name and its fields' types, by the
    /// variant's index.
    Variants(Vec<(String, Vec<Type>)>),
    /// A `Vector<T>`: the elements' type.
    Elements(Type),
    /// A `HashTable<K, V>`: the keys' and the values' types.
    Entries(Type, Type),
    /// Nothing to open.
    Whole,
}

/// What a value of `ty` is made of.
pub(crate) fn parts_of(reg: &Registry, ty: &Type) -> Parts {
    let Type::Named(path, args) = ty else { return Parts::Whole };
    let arg = |i: usize| args.get(i).cloned().expect("a builtin collection type is applied to its parameters");
    if crate::types::path_is_builtin(path, typelisp_mem::wk::VECTOR) {
        return Parts::Elements(arg(0));
    }
    if crate::types::path_is_builtin(path, typelisp_mem::wk::HASHTABLE) {
        return Parts::Entries(arg(0), arg(1));
    }
    let Some(TypeEntry::Type(def)) = reg.types.get(path) else { return Parts::Whole };
    let subst = super::subst::Subst::positional(&def.params, args);
    match def.kind {
        AdtKind::Struct if !def.fields.is_empty() => {
            Parts::Fields(def.fields.iter().map(|f| (f.name.name().to_string(), subst.apply(&f.ty))).collect())
        }
        AdtKind::Sum if !def.variants.is_empty() => Parts::Variants(
            def.variants.iter().map(|v| (v.name.name().to_string(), v.fields.iter().map(|t| subst.apply(t)).collect())).collect(),
        ),
        AdtKind::Struct | AdtKind::Sum => Parts::Whole,
    }
}
