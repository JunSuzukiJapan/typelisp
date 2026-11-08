//! The rebuilt checker's registry: what the program has defined
//! (`docs/dev/checker-rebuild-plan.md` 8).
//!
//! Definitions are keyed by their [`Path`], which already says which module
//! and which name. Every table keeps what each write replaced, so the writes
//! of a top-level form whose checking fails can be undone
//! ([`Registry::rollback`]) and those of one that succeeds kept
//! ([`Registry::commit`]).

use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use crate::check::lower::CoreForms;
use crate::check::registry::{AdtKind, OptKeyParam, TraitBound};
use crate::types::{Path, Type, TypeParam};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::generic::Base;
use super::tree::Typed;

/// A function: its type parameters, its parameter types and its result
/// type. The types may mention the type parameters.
#[derive(Clone, Debug)]
pub(crate) struct FnDef {
    pub(crate) type_params: Vec<TypeParam>,
    pub(crate) params: Vec<Type>,
    pub(crate) ret: Type,
    pub(crate) public: bool,
    /// Implemented in Rust, with no typelisp body.
    pub(crate) builtin: bool,
    /// A C function (`defffi`).
    pub(crate) ffi: bool,
    /// The element type of a trailing `&rest` parameter.
    pub(crate) rest: Option<Type>,
    pub(crate) optionals: Vec<OptKeyParam>,
    pub(crate) keys: Vec<OptKeyParam>,
    /// The `:where` bounds, by the type parameter they bound.
    pub(crate) bounds: Vec<(TypeParam, Vec<TraitBound>)>,
}

impl FnDef {
    /// A function the program defined with required parameters only.
    pub(crate) fn plain(type_params: Vec<TypeParam>, params: Vec<Type>, ret: Type) -> FnDef {
        FnDef {
            type_params,
            params,
            ret,
            public: false,
            builtin: false,
            ffi: false,
            rest: None,
            optionals: Vec::new(),
            keys: Vec::new(),
            bounds: Vec::new(),
        }
    }
}

/// A method of a type. Its type parameters are the type's, in order, then
/// the method's own (`docs/dev/checker-rebuild-plan.md` 7.3).
#[derive(Clone, Debug)]
pub(crate) struct MethodDef {
    pub(crate) sig: FnDef,
    /// The first parameter is the receiver.
    pub(crate) instance: bool,
    /// The module a `defmethod` outside an `impl` was written in, where a
    /// method without `pub` is visible; `None` for one an `impl` gave.
    pub(crate) home: Option<Path>,
    /// For a method written for one instantiation of a generic type, that
    /// instantiation's type arguments: its code is named for them.
    pub(crate) inst_args: Option<Vec<Type>>,
}

/// A type: its kind, its type parameters and, for a struct, its fields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TypeDef {
    pub(crate) kind: AdtKind,
    pub(crate) params: Vec<TypeParam>,
    pub(crate) public: bool,
    /// Implemented in Rust (`Vector`, `HashTable`, ...): no `make-instance`.
    pub(crate) builtin: bool,
    /// Defined by the standard library, so only the standard library may
    /// write an inherent `impl` of it.
    pub(crate) stdlib: bool,
    /// A struct's fields, in order. Their types are written in this type's
    /// own parameters.
    pub(crate) fields: Vec<Field>,
    /// Some field has a default (`(name Type default)`).
    pub(crate) has_defaults: bool,
}

/// A struct's field: its member symbol, its type, and whether it is `pub`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Field {
    pub(crate) name: SymRef,
    pub(crate) ty: Type,
    pub(crate) public: bool,
}

/// What a module-level name in the type name space is: a type, a type
/// alias, a trait or a C struct, never two of them. An alias, a trait and a
/// C struct are only known to be one so far.
#[derive(Clone, Debug)]
pub(crate) enum TypeEntry {
    Type(TypeDef),
    Alias,
    Trait,
    CStruct,
}

/// What a module-level name in the value name space is: one of three, never
/// two of them (`docs/dev/name-resolution.md` 1). A global variable and a
/// macro are only known to be one so far; what they are is recorded when the
/// checker checks their uses.
#[derive(Clone, Debug)]
pub(crate) enum ValueDef {
    Fn(FnDef),
    Var,
    Macro,
}

/// A generic function's or method's checked body, kept to be specialized:
/// its parameters' names and types (a method's receiver first), its result
/// type, and the body, every type in them settled except for the
/// definition's own type parameters.
#[derive(Debug)]
pub(crate) struct Template {
    pub(crate) params: Vec<(SymRef, Type)>,
    pub(crate) ret: Type,
    pub(crate) body: Vec<Typed>,
}

/// One table of the registry, keeping what each write replaced since the
/// last commit.
#[derive(Debug)]
pub(crate) struct Table<K, V> {
    map: HashMap<K, V>,
    undo: Vec<(K, Option<V>)>,
}

impl<K, V> Default for Table<K, V> {
    fn default() -> Self {
        Table { map: HashMap::new(), undo: Vec::new() }
    }
}

impl<K: Hash + Eq + Clone, V> Table<K, V> {
    pub(crate) fn get(&self, k: &K) -> Option<&V> {
        self.map.get(k)
    }

    pub(crate) fn insert(&mut self, k: K, v: V) {
        let replaced = self.map.insert(k.clone(), v);
        self.undo.push((k, replaced));
    }

    pub(crate) fn remove(&mut self, k: &K) {
        let replaced = self.map.remove(k);
        self.undo.push((k.clone(), replaced));
    }

    fn commit(&mut self) {
        self.undo.clear();
    }

    fn rollback(&mut self) {
        while let Some((k, replaced)) = self.undo.pop() {
            match replaced {
                Some(v) => {
                    self.map.insert(k, v);
                }
                None => {
                    self.map.remove(&k);
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Registry {
    /// Functions, global variables and macros: one name space.
    pub(crate) values: Table<Path, ValueDef>,
    pub(crate) types: Table<Path, TypeEntry>,
    /// Methods, by the type and the method's member symbol.
    pub(crate) methods: Table<(Path, SymRef), MethodDef>,
    /// Methods written for one instantiation of a generic type, by the type,
    /// the member symbol and the instantiation's type arguments.
    pub(crate) inst_methods: Table<(Path, SymRef), Vec<(Vec<Type>, MethodDef)>>,
    /// What a `use` brought into a module: the module and the name it is
    /// brought in under, and the definition.
    pub(crate) imports: Table<(NsId, SymRef), Path>,
    /// The imports a `pub use` brought in, which a path through the module
    /// reaches.
    pub(crate) reexports: Table<(NsId, SymRef), ()>,
    /// The variants a `use` of a type brought in bare: the type and the
    /// variant's index.
    pub(crate) ctors: Table<(NsId, SymRef), (Path, usize)>,
    /// The static methods a `use` of a type brought in bare: the type and the
    /// method's member symbol.
    pub(crate) static_uses: Table<(NsId, SymRef), (Path, SymRef)>,
    templates: Table<Base, Rc<Template>>,
}

impl Registry {
    pub(crate) fn type_def(&self, path: &Path) -> Option<&TypeDef> {
        match self.types.get(path) {
            Some(TypeEntry::Type(def)) => Some(def),
            _ => None,
        }
    }

    pub(crate) fn fn_def(&self, path: &Path) -> Option<&FnDef> {
        match self.values.get(path) {
            Some(ValueDef::Fn(def)) => Some(def),
            _ => None,
        }
    }

    /// Define (or redefine) the function `path`. Seen at once, so a body can
    /// call the function it belongs to.
    pub(crate) fn define_fn(&mut self, path: Path, def: FnDef) {
        self.values.insert(path, ValueDef::Fn(def));
    }

    /// The generic function's or method's body, to specialize.
    pub(crate) fn template(&self, base: &Base) -> Option<Rc<Template>> {
        self.templates.get(base).cloned()
    }

    /// Keep the generic function's or method's body, or forget the one an
    /// earlier definition kept (`None`: it is not generic now).
    pub(crate) fn set_template(&mut self, base: Base, template: Option<Template>) {
        match template {
            Some(t) => self.templates.insert(base, Rc::new(t)),
            None => self.templates.remove(&base),
        }
    }

    /// The method `name` of the type `owner` for a value whose type arguments
    /// are `args`: the instantiation's own, or the one for every
    /// instantiation.
    pub(crate) fn method_at(&self, owner: &Path, name: SymRef, args: &[Type]) -> Option<&MethodDef> {
        let key = (*owner, name);
        let own = self.inst_methods.get(&key).and_then(|insts| insts.iter().find(|(a, _)| a == args)).map(|(_, m)| m);
        own.or_else(|| self.methods.get(&key))
    }

    /// Keep every write since the last commit.
    pub(crate) fn commit(&mut self) {
        self.values.commit();
        self.types.commit();
        self.methods.commit();
        self.inst_methods.commit();
        self.imports.commit();
        self.reexports.commit();
        self.ctors.commit();
        self.static_uses.commit();
        self.templates.commit();
    }

    /// Undo every write since the last commit.
    pub(crate) fn rollback(&mut self) {
        self.values.rollback();
        self.types.rollback();
        self.methods.rollback();
        self.inst_methods.rollback();
        self.imports.rollback();
        self.reexports.rollback();
        self.ctors.rollback();
        self.static_uses.rollback();
        self.templates.rollback();
    }
}

/// What building core IR asks of the types this registry knows.
impl CoreForms for Registry {
    fn type_kind(&self, p: &Path) -> Option<AdtKind> {
        self.type_def(p).map(|d| d.kind)
    }

    fn builtin_fn_sig(&self, p: &Path) -> Option<(Vec<Type>, Type)> {
        self.fn_def(p).filter(|d| d.builtin && !d.ffi).map(|d| (d.params.clone(), d.ret.clone()))
    }

    fn builtin_method_sig(&self, owner: &Path, method: SymRef) -> Option<(Vec<Type>, Type)> {
        self.methods.get(&(*owner, method)).filter(|m| m.sig.builtin).map(|m| (m.sig.params.clone(), m.sig.ret.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str) -> Path {
        Path::in_ns(NsId::ROOT, name)
    }

    fn def(ret: Type) -> FnDef {
        FnDef::plain(Vec::new(), Vec::new(), ret)
    }

    #[test]
    fn a_rollback_removes_what_was_defined() {
        let mut r = Registry::default();
        r.define_fn(f("a"), def(Type::Int));
        r.rollback();
        assert!(r.fn_def(&f("a")).is_none());
    }

    #[test]
    fn a_rollback_restores_what_was_redefined() {
        let mut r = Registry::default();
        r.define_fn(f("a"), def(Type::Int));
        r.commit();
        r.define_fn(f("a"), def(Type::Bool));
        r.define_fn(f("a"), def(Type::Str));
        r.rollback();
        assert_eq!(r.fn_def(&f("a")).map(|d| &d.ret), Some(&Type::Int));
    }

    #[test]
    fn a_commit_keeps_what_was_written() {
        let mut r = Registry::default();
        r.define_fn(f("a"), def(Type::Int));
        r.commit();
        r.rollback();
        assert_eq!(r.fn_def(&f("a")).map(|d| &d.ret), Some(&Type::Int));
    }
}
