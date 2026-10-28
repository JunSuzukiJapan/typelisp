//! A runtime mirror of the checker's modules
//! ([`crate::check::registry::Namespace`]), holding every top-level
//! `defun`/`defmethod`/`defvar`/`defstruct`/`defenum`. Each definition is
//! registered with its own defining module, under the symbol its name reads
//! as there — so no single table spans the whole program.
//!
//! Nothing here resolves a name. A reference arrives already resolved — the
//! checker wrote the `Path` it found into the node — and is looked up by that
//! path's module and symbol
//! ([`ModuleScope::get_fn`]/[`get_global`]/[`get_method`]). Visibility was the
//! checker's question too, so there is no `pub` check either.

use std::collections::HashMap;
use std::rc::Rc;

use typelisp_mem::NsId;

use crate::{Path, SymRef, TypeId};

use super::interp::{EnumDef, FnDef};
use super::value::Slot;

/// A user `defstruct`/`defenum`'s runtime classification, keyed by its own
/// unqualified name at the tree node for its defining module — the
/// distributed replacement for the old `Interp::struct_types`/`enum_defs`
/// flat `HashSet<Path>`/`HashMap<Path, EnumDef>`. Membership here is a
/// structural fact about the type's own representation, not a visibility
/// gate (the checker already validated that any reference to the type
/// itself was in scope when it type-checked the reference).
pub(crate) enum TypeEntry {
    /// A `defstruct`, with its fields' representations — what
    /// `(defstruct PATH (REPR...) (TEMPLATE...))` publishes, and what the
    /// compile bridge reads a `construct`'s and a pattern's field kinds from
    /// — and its fields' type templates, the printer's question. A built-in
    /// that reuses the struct representation but whose fields are internal
    /// (`Vector<T>`) records no representations: nothing ever names those
    /// fields. Its *elements* still print, so it records a uniform template.
    ///
    /// `names` are the fields' names, which the printer writes before each
    /// value (`#<point x: 1 y: 2>`). Empty for a type whose fields are its
    /// elements, which print positionally (`#(1 2 3)`).
    Struct { reprs: Vec<crate::check::repr::Repr>, templates: FieldTemplates, names: Vec<String> },
    /// A `defenum` — carries the same per-variant field-type data
    /// `enum_defs` used to.
    Enum(EnumDef),
}

/// A struct's fields' types as templates (`type_key::field_template`): one
/// per declared field, or one for every element of a type whose fields are
/// its elements (`Vector<T>`, whose count only the value knows).
#[derive(Clone)]
pub(crate) enum FieldTemplates {
    Positional(Vec<TypeId>),
    Uniform(TypeId),
}

impl FieldTemplates {
    pub(crate) fn get(&self, index: usize) -> Option<TypeId> {
        match self {
            FieldTemplates::Positional(v) => v.get(index).copied(),
            FieldTemplates::Uniform(t) => Some(*t),
        }
    }
}

/// One module's runtime definitions, keyed by the symbols their names read
/// as there — the symbol a [`Path`] to the definition holds.
#[derive(Default)]
pub(crate) struct ModuleTables {
    /// `defun`/`defmacro`, by name — defined directly in this module.
    pub(crate) fns: HashMap<SymRef, Rc<FnDef>>,
    /// `defmethod`, by (the owning type's name, the method's member symbol)
    /// — stored with the *type's* defining module, not the method's own
    /// definition site (a `defmethod` always lives alongside its type for
    /// this purpose, matching how the checker's `AdtDef::assoc` is itself
    /// part of the type's own definition).
    pub(crate) methods: HashMap<(SymRef, SymRef), Rc<FnDef>>,
    /// `defvar`/`defconstant`, by name.
    pub(crate) globals: HashMap<SymRef, Slot>,
    /// `defstruct`/`defenum`, by name.
    pub(crate) types: HashMap<SymRef, TypeEntry>,
}

/// The runtime definitions of every module, rooted at `Interp`'s own `root`:
/// one [`ModuleTables`] per module that defines something, found by the
/// module's [`NsId`] — the module a [`Path`] names.
#[derive(Default)]
pub struct ModuleScope {
    modules: HashMap<NsId, ModuleTables>,
}

impl ModuleScope {
    /// The tables of module `ns`, if it defines anything.
    fn module(&self, ns: NsId) -> Option<&ModuleTables> {
        self.modules.get(&ns)
    }

    /// The tables of module `ns`, made empty if it has none yet. Never a
    /// visibility decision: the checker placed the definition at this exact
    /// path, so there is nothing to gate here.
    pub(crate) fn module_mut(&mut self, ns: NsId) -> &mut ModuleTables {
        self.modules.entry(ns).or_default()
    }

    /// Registers the function or macro `def` at `path`, replacing whatever
    /// was there.
    pub(crate) fn insert_fn(&mut self, path: &Path, def: Rc<FnDef>) {
        self.module_mut(path.ns()).fns.insert(path.name_symbol(), def);
    }

    /// Registers the method `method` of the type at `type_name`.
    pub(crate) fn insert_method(&mut self, type_name: &Path, method: SymRef, def: Rc<FnDef>) {
        self.module_mut(type_name.ns()).methods.insert((type_name.name_symbol(), method), def);
    }

    /// Registers the global variable at `path`.
    pub(crate) fn insert_global(&mut self, path: &Path, slot: Slot) {
        self.module_mut(path.ns()).globals.insert(path.name_symbol(), slot);
    }

    /// Registers the type at `path`.
    pub(crate) fn insert_type(&mut self, path: &Path, entry: TypeEntry) {
        self.module_mut(path.ns()).types.insert(path.name_symbol(), entry);
    }

    /// THE registration point for a `TypeEntry::Enum` — the only place that
    /// inserts one, so an enum's variant names live in exactly one table no
    /// matter where the definition came from. Both a user `defenum`'s exec
    /// (`Interp::exec`'s `TopLevel::Defenum` arm) and `Interp::new`'s seeding
    /// of the built-in sum types (`crate::check::registry::builtin_sum_defs`
    /// — `Option`/`Result`/the error types, which are registered with the
    /// checker but never `exec`'d) call this rather than inserting a
    /// `TypeEntry::Enum` themselves.
    pub(crate) fn register_enum(&mut self, name: &Path, def: EnumDef) {
        self.insert_type(name, TypeEntry::Enum(def));
    }

    /// The type registered at `name` — the printer asks once per value it
    /// renders.
    fn type_entry(&self, name: &Path) -> Result<&TypeEntry, String> {
        self.module(name.ns())
            .and_then(|m| m.types.get(&name.name_symbol()))
            .ok_or_else(|| format!("internal error: no type `{}` is registered", name))
    }

    /// The name of `variant` of the enum registered at `name` — the printer's
    /// question ([`typelisp_print::PrintEnv::enum_variant_name`]): an enum box
    /// stores its type key and variant *index*, never the name.
    pub(crate) fn enum_variant_name(&self, name: &Path, variant: usize) -> Result<String, String> {
        match self.type_entry(name)? {
            TypeEntry::Enum(def) => def
                .variants
                .get(variant)
                .map(|(n, _)| n.clone())
                .ok_or_else(|| format!("internal error: enum `{}` has no variant {}", name, variant)),
            TypeEntry::Struct { .. } => Err(format!("internal error: `{}` is a struct, not an enum", name)),
        }
    }

    /// The type template of field `index` of the type registered at `name` —
    /// of `variant` for an enum, of the struct otherwise. The printer's other
    /// question ([`typelisp_print::PrintEnv::field_is_niched_option`]).
    pub(crate) fn field_template(&self, name: &Path, variant: Option<usize>, index: usize) -> Result<TypeId, String> {
        let template = match (self.type_entry(name)?, variant) {
            (TypeEntry::Enum(def), Some(v)) => def.templates.get(v).and_then(|ts| ts.get(index)).copied(),
            (TypeEntry::Struct { templates, .. }, None) => templates.get(index),
            (TypeEntry::Enum(_), None) => return Err(format!("internal error: enum `{}` asked for as a struct", name)),
            (TypeEntry::Struct { .. }, Some(_)) => return Err(format!("internal error: struct `{}` asked for as an enum", name)),
        };
        template.ok_or_else(|| format!("internal error: `{}` has no field {} (variant {:?})", name, index, variant))
    }

    /// The name of field `index` of the struct registered at `name` —
    /// `Ok(None)` when its fields have no names (a `Vector<T>`'s elements).
    /// The printer's third question ([`typelisp_print::PrintEnv::field_name`]).
    pub(crate) fn field_name(&self, name: &Path, index: usize) -> Result<Option<&str>, String> {
        match self.type_entry(name)? {
            TypeEntry::Struct { names, .. } if names.is_empty() => Ok(None),
            TypeEntry::Struct { names, .. } => names
                .get(index)
                .map(|n| Some(n.as_str()))
                .ok_or_else(|| format!("internal error: struct `{}` has no field {}", name, index)),
            TypeEntry::Enum(_) => Err(format!("internal error: `{}` is an enum, not a struct", name)),
        }
    }

    /// Every registered type, with its path.
    fn all_types(&self) -> impl Iterator<Item = (Path, &TypeEntry)> {
        self.modules.iter().flat_map(|(ns, m)| m.types.iter().map(move |(name, entry)| (Path::of_symbol(*ns, *name), entry)))
    }

    /// Every registered struct's field names, as `(path, index, name)` — for
    /// an AOT executable's startup registration
    /// (`typelisp_print::aot::rt_print_field_name`) and a worker thread's
    /// printer snapshot, the same two readers [`Self::collect_field_templates`]
    /// has.
    pub(crate) fn collect_field_names(&self, out: &mut Vec<(Path, usize, String)>) {
        for (path, entry) in self.all_types() {
            if let TypeEntry::Struct { names, .. } = entry {
                for (i, n) in names.iter().enumerate() {
                    out.push((path, i, n.clone()));
                }
            }
        }
    }

    /// A free function/macro's own `FnDef`, found by its already-resolved
    /// `Path` — no search, no visibility check (registration is never a
    /// visibility decision, see [`Self::module_mut`]). Used by every consumer
    /// that already has a resolved target in hand rather than a bare name to
    /// search for: `MacroExpander::expand_macro`'s checker-driven lookup,
    /// and the JIT/SCC machinery's "is this already compiled" checks.
    pub fn get_fn(&self, path: &Path) -> Option<Rc<FnDef>> {
        self.module(path.ns())?.fns.get(&path.name_symbol()).cloned()
    }

    /// A global's own `Slot`, found by its already-resolved `Path` (used by
    /// `Interp::promote_global`, always called with a `collect_global_targets`
    /// result, never a bare name to search for).
    pub(crate) fn get_global(&self, path: &Path) -> Option<Slot> {
        self.module(path.ns())?.globals.get(&path.name_symbol()).cloned()
    }

    /// [`Self::get_fn`]'s twin for a `(type, method)` pair.
    pub fn get_method(&self, type_name: &Path, method: SymRef) -> Option<Rc<FnDef>> {
        self.module(type_name.ns())?.methods.get(&(type_name.name_symbol(), method)).cloned()
    }

    /// Whether `path`'s `FnDef` already has a JIT/AOT-compiled form — the
    /// direct replacement for the old `Interp::compiled: HashMap<Path, _>`'s
    /// `contains_key` check (see [`super::interp::FnDef::compiled`]'s doc
    /// comment for why this now lives on the definition itself).
    pub fn fn_compiled(&self, path: &Path) -> bool {
        self.get_fn(path).is_some_and(|f| f.compiled.borrow().is_some())
    }

    /// [`Self::fn_compiled`]'s twin for a `(type, method)` pair.
    pub fn method_compiled(&self, type_name: &Path, method: SymRef) -> bool {
        self.get_method(type_name, method).is_some_and(|f| f.compiled.borrow().is_some())
    }

    /// Register `def` at `path`, replacing whatever was there. The raw door:
    /// the interpreter's own `defun`/`defmethod` execution goes through
    /// `Interp::register_fn`, which builds the `FnDef` out of a checked core
    /// form. This is for a caller that already holds one — the backend's SCC
    /// tests build two mutually recursive `FnDef`s directly, because the
    /// checker's forward-reference rules will not let that call graph be
    /// written as source.
    pub fn define_fn(&mut self, path: &Path, def: Rc<FnDef>) {
        self.insert_fn(path, def);
    }

    /// Whether a `(type, method)` pair is registered at all — the direct
    /// replacement for the old `Interp::methods.contains_key` check.
    pub fn has_method(&self, type_name: &Path, method: SymRef) -> bool {
        self.get_method(type_name, method).is_some()
    }

    /// Every registered free function/macro's own unqualified name, across
    /// every module — for `Interp::function_names`, a tooling/test-only
    /// "enumerate everything regardless of module" consumer.
    pub(crate) fn all_fn_names(&self) -> Vec<String> {
        self.modules.values().flat_map(|m| m.fns.keys().map(|name| name.name().to_string())).collect()
    }

    /// Flatten every `TypeEntry` into the `(HashMap<Path, Vec<Repr>>,
    /// HashMap<Path, EnumDef>)` shape `compile::core_bridge` (deliberately
    /// `Registry`-free, see that module's own doc comment) expects as a plain
    /// snapshot — the same "flatten once for the compile boundary" treatment
    /// `compiled_globals` already gets when cloned into
    /// `core_bridge::Ctx::globals`. Called once per JIT/AOT compile, not on
    /// any interpreted hot path.
    pub fn collect_struct_and_enum_types(
        &self,
    ) -> (HashMap<Path, Vec<crate::check::repr::Repr>>, HashMap<Path, EnumDef>) {
        let mut structs = HashMap::new();
        let mut enums = HashMap::new();
        for (path, entry) in self.all_types() {
            match entry {
                TypeEntry::Struct { reprs, .. } => {
                    structs.insert(path, reprs.clone());
                }
                TypeEntry::Enum(def) => {
                    enums.insert(path, def.clone());
                }
            }
        }
        (structs, enums)
    }

    /// Every function and method registered anywhere — what a worker
    /// thread's printer is given a snapshot of (`worker_print`), and where a
    /// `~/name/` directive's definition is found.
    pub(crate) fn collect_defs(&self, out: &mut Vec<Rc<FnDef>>) {
        for m in self.modules.values() {
            out.extend(m.fns.values().cloned());
            out.extend(m.methods.values().cloned());
        }
    }

    /// Every registered type's field type templates, for an AOT executable's
    /// startup registration (`typelisp_print::aot::rt_print_field_template`):
    /// `(path, variant or None, field index or None for every field,
    /// template)`.
    pub(crate) fn collect_field_templates(&self, out: &mut Vec<(Path, Option<usize>, Option<usize>, TypeId)>) {
        for (path, entry) in self.all_types() {
            match entry {
                TypeEntry::Struct { templates: FieldTemplates::Positional(ts), .. } => {
                    for (i, t) in ts.iter().enumerate() {
                        out.push((path, None, Some(i), *t));
                    }
                }
                TypeEntry::Struct { templates: FieldTemplates::Uniform(t), .. } => {
                    out.push((path, None, None, *t));
                }
                TypeEntry::Enum(def) => {
                    for (v, ts) in def.templates.iter().enumerate() {
                        for (i, t) in ts.iter().enumerate() {
                            out.push((path, Some(v), Some(i), *t));
                        }
                    }
                }
            }
        }
    }
}
