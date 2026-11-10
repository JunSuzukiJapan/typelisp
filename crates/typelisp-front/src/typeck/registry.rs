//! The rebuilt checker's registry: what the program has defined
//! (`docs/dev/checker-rebuild-plan.md` 8).
//!
//! Definitions are keyed by their [`Path`], which already says which module
//! and which name. Every table keeps what each write replaced, so the writes
//! of a top-level form whose checking fails can be undone
//! ([`Registry::rollback`]) and those of one that succeeds kept
//! ([`Registry::commit`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use crate::check::analyze::MacroShape;
use crate::check::lower::CoreForms;
use std::collections::BTreeMap;

use crate::check::registry::{AdtKind, OptKeyParam, TraitBound, Variant};
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

/// A type: its kind, its type parameters, and a struct's fields or a sum
/// type's variants.
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
    /// A sum type's variants, in order: a variant is its index here. Their
    /// field types are written in this type's own parameters.
    pub(crate) variants: Vec<Variant>,
}

impl TypeDef {
    /// The index of the variant `name` (a member symbol), if it is one.
    pub(crate) fn variant_index(&self, name: SymRef) -> Option<usize> {
        self.variants.iter().position(|v| v.name == name)
    }
}

/// A struct's field: its member symbol, its type, whether it is `pub`, and
/// whether it has a default.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Field {
    pub(crate) name: SymRef,
    pub(crate) ty: Type,
    pub(crate) public: bool,
    /// Written with a default, which `make-instance` gives the slot when
    /// it is not given ([`super::defstruct::default_method`]).
    pub(crate) default: bool,
}

/// What a module-level name in the type name space is: a type, a type
/// alias, a trait or a C struct, never two of them.
#[derive(Clone, Debug)]
pub(crate) enum TypeEntry {
    Type(TypeDef),
    /// A `deftype`: the type `body` stands for, written in its `params`.
    Alias { public: bool, params: Vec<TypeParam>, body: Type },
    Trait(TraitDef),
    /// A `def-c-struct`: its fields laid out as C lays them out.
    CStruct(crate::check::registry::CStructDef),
}

/// A trait: its type parameters, `Self` first and then its associated
/// types (`docs/dev/checker-rebuild-plan.md` 7.4), and its own methods'
/// signatures, written in them.
#[derive(Clone, Debug)]
pub(crate) struct TraitDef {
    pub(crate) params: Vec<TypeParam>,
    /// The trait's own methods, in the order the `deftrait` wrote them. A
    /// method's type parameters are the trait's.
    pub(crate) methods: Vec<(SymRef, FnDef)>,
    /// The methods the `deftrait` gave a body, which an `impl` may leave out.
    pub(crate) defaults: Vec<SymRef>,
    /// The traits a type implementing this one implements too, written in
    /// this one's parameters.
    pub(crate) supertraits: Vec<TraitBound>,
    pub(crate) public: bool,
}

impl TraitDef {
    /// The signature of the trait's own method `name`.
    pub(crate) fn method(&self, name: SymRef) -> Option<&FnDef> {
        self.methods.iter().find(|(n, _)| *n == name).map(|(_, d)| d)
    }
}

/// An `impl` of a trait for a type: for every instantiation of it, or for
/// the one `args` names. The associated types it binds, by name, and the
/// bounds its `(:where ...)` puts on the type's parameters are written in
/// the type's parameters.
#[derive(Clone, Debug)]
pub(crate) struct TraitImpl {
    pub(crate) args: Option<Vec<Type>>,
    pub(crate) assoc: BTreeMap<String, Type>,
    pub(crate) bounds: Vec<(TypeParam, Vec<TraitBound>)>,
}

/// What a module-level name in the value name space is: one of three, never
/// two of them (`docs/dev/name-resolution.md` 1). Of a macro, whether it is
/// `pub` and how many arguments a call of it may have; its body is the
/// evaluator's, which expands it.
#[derive(Clone, Debug)]
pub(crate) enum ValueDef {
    Fn(FnDef),
    Var(VarDef),
    Macro { public: bool, shape: MacroShape },
}

impl ValueDef {
    /// Whether it is visible outside its module.
    pub(crate) fn public(&self) -> bool {
        match self {
            ValueDef::Fn(def) => def.public,
            ValueDef::Var(def) => def.public,
            ValueDef::Macro { public, .. } => *public,
        }
    }
}

/// A global variable: its type, and whether it is `pub`.
#[derive(Clone, Debug)]
pub(crate) struct VarDef {
    pub(crate) ty: Type,
    pub(crate) public: bool,
    /// Whether `setf` may assign it: not a `defconstant`.
    pub(crate) mutable: bool,
}

/// A generic function's or method's checked body, kept to be specialized:
/// its parameters' names and types (a method's receiver first), its result
/// type, and the body, every type in them settled except for the
/// definition's own type parameters.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Template {
    #[serde(with = "super::wire::sym_pairs")]
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

    /// Every key and value.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter()
    }

    /// The keys written since the last commit, each once for each write.
    pub(crate) fn written(&self) -> impl Iterator<Item = &K> {
        self.undo.iter().map(|(k, _)| k)
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
    methods: Table<(Path, SymRef), MethodDef>,
    /// Methods written for one instantiation of a generic type, by the type,
    /// the member symbol and the instantiation's type arguments.
    inst_methods: Table<(Path, SymRef), Vec<(Vec<Type>, MethodDef)>>,
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
    /// The modules a `use` brought in under a short name (`(use std::math)`
    /// makes `math` one).
    pub(crate) module_aliases: Table<(NsId, SymRef), NsId>,
    templates: Table<Base, Rc<Template>>,
    /// The `impl`s of a trait for a type, by the trait and the type.
    pub(crate) trait_impls: Table<(Path, Path), Vec<TraitImpl>>,
    /// The traits some blanket `impl` (one for a type variable) implements.
    pub(crate) blanket_impls: Table<Path, ()>,
    /// The methods an `impl` left out and took from the trait's default
    /// body, by the type and the member symbol: the trait, and its
    /// parameters there, written in the method's type parameters.
    defaulted: Table<(Path, SymRef), (Path, Vec<Type>)>,
    /// The documentation strings definitions wrote.
    pub(crate) docs: Table<Doc, String>,
    /// The functions a `defsignature` declared whose `defun` has not come
    /// yet.
    pub(crate) predeclared: Table<Path, ()>,
    /// The type each `catch` and `throw` tag carries, by the tag's symbol.
    /// Written when a form is settled (`Cx::settle`), which holds the
    /// registry to read only.
    throw_tags: RefCell<Table<SymRef, Type>>,
    /// The functions lifted for C callbacks so far, to name the next one.
    callbacks_lifted: std::cell::Cell<u64>,
}

/// A definition a documentation string is kept for: a function, a macro, a
/// global variable, a type or a trait by its path, a type's method or a
/// trait's own method by the type or the trait and the member symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Doc {
    Fn(Path),
    Macro(Path),
    Var(Path),
    Type(Path),
    Trait(Path),
    Method(Path, SymRef),
    TraitMethod(Path, SymRef),
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

    /// A number no lifted callback's name has had.
    pub(crate) fn next_callback(&self) -> u64 {
        let n = self.callbacks_lifted.get();
        self.callbacks_lifted.set(n + 1);
        n
    }

    /// The type the tag `tag` carries, once a form has said.
    pub(crate) fn throw_tag(&self, tag: SymRef) -> Option<Type> {
        self.throw_tags.borrow().get(&tag).cloned()
    }

    /// Record that the tag `tag` carries `ty`.
    pub(crate) fn record_throw_tag(&self, tag: SymRef, ty: Type) {
        self.throw_tags.borrow_mut().insert(tag, ty);
    }

    /// Every generic body kept.
    pub(crate) fn templates(&self) -> impl Iterator<Item = (&Base, &Rc<Template>)> {
        self.templates.iter()
    }

    /// Every method an `impl` took from its trait's default body, with the
    /// trait and its arguments.
    pub(crate) fn all_defaulted(&self) -> impl Iterator<Item = (&(Path, SymRef), &(Path, Vec<Type>))> {
        self.defaulted.iter()
    }

    /// Keep the generic body `template` a dump wrote, where this registry
    /// has none: what a registry copied from the current checker's lacks of
    /// the prelude (`wire::RebuiltBodies::restore`).
    pub(crate) fn adopt_template(&mut self, base: Base, template: Rc<Template>) {
        if self.templates.get(&base).is_none() {
            self.templates.insert(base, template);
        }
    }

    /// Record the method a dump wrote that an `impl` took from its trait's
    /// default body, where this registry records none.
    pub(crate) fn adopt_defaulted(&mut self, key: (Path, SymRef), defaulted: (Path, Vec<Type>)) {
        if self.defaulted.get(&key).is_none() {
            self.defaulted.insert(key, defaulted);
        }
    }

    /// The method `name` of the type `owner` written for every instantiation.
    pub(crate) fn method(&self, owner: &Path, name: SymRef) -> Option<&MethodDef> {
        self.methods.get(&(*owner, name))
    }

    /// The methods of the type `owner`: those for every instantiation, then
    /// those written for one, each by its member symbol.
    pub(crate) fn methods_of(&self, owner: Path) -> impl Iterator<Item = (SymRef, &MethodDef)> {
        let every = self.methods.iter().filter(move |((o, _), _)| *o == owner).map(|((_, n), m)| (*n, m));
        let one = self.inst_methods.iter().filter(move |((o, _), _)| *o == owner).flat_map(|((_, n), insts)| insts.iter().map(move |(_, m)| (*n, m)));
        every.chain(one)
    }

    /// Whether `owner` has a method `name` written for one instantiation.
    pub(crate) fn has_inst_method(&self, owner: &Path, name: SymRef) -> bool {
        self.inst_methods.get(&(*owner, name)).is_some()
    }

    /// The methods `name` of `owner` written for one instantiation each,
    /// with the instantiations' type arguments.
    pub(crate) fn inst_methods(&self, owner: &Path, name: SymRef) -> &[(Vec<Type>, MethodDef)] {
        self.inst_methods.get(&(*owner, name)).map_or(&[], Vec::as_slice)
    }

    /// Define (or redefine) the method `name` of `owner`.
    pub(crate) fn define_method(&mut self, owner: Path, name: SymRef, def: MethodDef) {
        if self.defaulted.get(&(owner, name)).is_some() {
            self.defaulted.remove(&(owner, name));
        }
        self.methods.insert((owner, name), def);
    }

    /// Define the method `name` of `owner` as the default body of the trait
    /// `trait_path`, whose parameters are `at` there.
    pub(crate) fn define_defaulted_method(&mut self, owner: Path, name: SymRef, def: MethodDef, trait_path: Path, at: Vec<Type>) {
        self.define_method(owner, name, def);
        self.defaulted.insert((owner, name), (trait_path, at));
    }

    /// The trait whose default body the method `name` of `owner` is, and the
    /// trait's parameters there.
    pub(crate) fn defaulted(&self, owner: &Path, name: SymRef) -> Option<&(Path, Vec<Type>)> {
        self.defaulted.get(&(*owner, name))
    }

    /// Define (or redefine) the method `name` of `owner` for the
    /// instantiation `args`.
    pub(crate) fn define_inst_method(&mut self, owner: Path, name: SymRef, args: Vec<Type>, def: MethodDef) {
        let mut insts = self.inst_methods.get(&(owner, name)).cloned().unwrap_or_default();
        insts.retain(|(a, _)| *a != args);
        insts.push((args, def));
        self.inst_methods.insert((owner, name), insts);
    }

    /// Every type with an instance method `name`, written as a message names
    /// it: the type itself for a method of every instantiation, the
    /// instantiation for one of one. Sorted, so a message is the same from
    /// run to run.
    pub(crate) fn types_with_instance_method(&self, name: SymRef) -> Vec<String> {
        let view = typelisp_print::shared::name_view();
        let mut out: Vec<String> = self
            .methods
            .iter()
            .filter(|((_, m), def)| *m == name && def.instance)
            .map(|((owner, _), _)| view.definition(owner.ns(), owner.name_symbol()))
            .collect();
        for ((owner, m), insts) in self.inst_methods.iter() {
            if *m == name {
                out.extend(insts.iter().filter(|(_, def)| def.instance).map(|(args, _)| Type::Named(*owner, args.clone()).to_string()));
            }
        }
        out.sort();
        out.dedup();
        out
    }

        /// The methods defined since the last commit.
    pub(crate) fn methods_written(&self) -> impl Iterator<Item = &(Path, SymRef)> {
        self.methods.written().chain(self.inst_methods.written())
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
        self.module_aliases.commit();
        self.templates.commit();
        self.trait_impls.commit();
        self.blanket_impls.commit();
        self.defaulted.commit();
        self.docs.commit();
        self.predeclared.commit();
        self.throw_tags.get_mut().commit();
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
        self.module_aliases.rollback();
        self.templates.rollback();
        self.trait_impls.rollback();
        self.blanket_impls.rollback();
        self.defaulted.rollback();
        self.docs.rollback();
        self.predeclared.rollback();
        self.throw_tags.get_mut().rollback();
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
