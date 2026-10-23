//! A runtime mirror of the checker's module tree
//! ([`crate::check::registry::Namespace`]), replacing the flat `Path`-keyed
//! tables [`Interp`](super::interp::Interp) used to hold for every top-level
//! `defun`/`defmethod`/`defvar`/`defstruct`/`defenum`. Each definition is
//! registered at the tree node for its own defining module, under its own
//! unqualified name — so no single table spans the whole program.
//!
//! Nothing here resolves a name. A reference arrives already resolved — the
//! checker wrote the absolute `Path` it found into the node — and is looked
//! up by walking straight down by that path's segments
//! ([`ModuleScope::get_fn`]/[`get_global`]/[`get_method`]). Visibility was the
//! checker's question too, so there is no `pub` check either.

use std::collections::HashMap;
use std::rc::Rc;

use crate::{Path, TypeId};

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

/// One node of the runtime module tree, rooted at `Interp`'s own `root`.
#[derive(Default)]
pub struct ModuleScope {
    /// `defun`/`defmacro`, by unqualified name — defined directly in this
    /// module (not a descendant).
    pub(crate) fns: HashMap<String, Rc<FnDef>>,
    /// `defmethod`, by (the owning type's unqualified name, method name) —
    /// stored at the tree node for the *type's* defining module, not the
    /// method's own definition site (a `defmethod` always lives alongside
    /// its type for this purpose, matching how the checker's `AdtDef::assoc`
    /// is itself part of the type's own definition).
    pub(crate) methods: HashMap<(String, String), Rc<FnDef>>,
    /// `defvar`/`defconstant`, by unqualified name.
    pub(crate) globals: HashMap<String, Slot>,
    /// `defstruct`/`defenum`, by unqualified name.
    pub(crate) types: HashMap<String, TypeEntry>,
    /// Child modules, by unqualified name.
    pub(crate) children: HashMap<String, ModuleScope>,
}

impl ModuleScope {
    /// Descend through child modules along `segs`; `None` if any segment is
    /// missing.
    pub(crate) fn find(&self, segs: &[String]) -> Option<&ModuleScope> {
        let mut ns = self;
        for seg in segs {
            ns = ns.children.get(seg)?;
        }
        Some(ns)
    }

    /// Descend through child modules along `segs`, creating any missing
    /// intermediate node. The tree's only mutating-descent entry point —
    /// every registration (`Interp::exec`'s `Defun`/`Defmethod`/`Defvar`/
    /// `Defmacro`/`Defstruct`/`Defenum`/`Module` arms) reaches its target
    /// node this way, from the absolute `Path` the checker already baked
    /// onto the `TopLevel` node. This is never itself a visibility decision
    /// — the checker placed the definition at this exact path uniquely, so
    /// there is nothing to gate here.
    pub(crate) fn get_or_create(&mut self, segs: &[String]) -> &mut ModuleScope {
        let mut ns = self;
        for seg in segs {
            ns = ns.children.entry(seg.clone()).or_default();
        }
        ns
    }

    /// THE registration point for a `TypeEntry::Enum` in this tree — the only
    /// place that inserts one, so an enum's variant names live in exactly one
    /// table no matter where the definition came from. Both a user
    /// `defenum`'s exec (`Interp::exec`'s `TopLevel::Defenum` arm) and
    /// `Interp::new`'s seeding of the built-in sum types
    /// (`crate::check::registry::builtin_sum_defs` — `Option`/`Result`/the
    /// error types, which are registered with the checker but never `exec`'d)
    /// call this rather than inserting into `types` themselves.
    pub(crate) fn register_enum(&mut self, name: &Path, def: EnumDef) {
        self.get_or_create(&name.parent()).types.insert(name.last_segment().to_string(), TypeEntry::Enum(def));
    }

    /// The type registered at `name`, walking down the module chain directly
    /// rather than through [`Self::collect_struct_and_enum_types`]: the
    /// printer asks once per value it renders, and that one builds a map of
    /// every type in the program.
    fn type_entry(&self, name: &Path) -> Result<&TypeEntry, String> {
        let unknown = || format!("internal error: no type `{}` is registered", name);
        let mut ns = self;
        for seg in name.parent() {
            ns = ns.children.get(&seg).ok_or_else(unknown)?;
        }
        ns.types.get(name.last_segment()).ok_or_else(unknown)
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

    /// Every registered struct's field names, as `(path, index, name)` in tree
    /// order — for an AOT executable's startup registration
    /// (`typelisp_print::aot::rt_print_field_name`) and a worker thread's
    /// printer snapshot, the same two readers [`Self::collect_field_templates`]
    /// has.
    pub(crate) fn collect_field_names(&self, prefix: &[String], out: &mut Vec<(Path, usize, String)>) {
        for (name, entry) in &self.types {
            if let TypeEntry::Struct { names, .. } = entry {
                let mut segs = prefix.to_vec();
                segs.push(name.clone());
                let path = Path::from_segments(segs);
                for (i, n) in names.iter().enumerate() {
                    out.push((path.clone(), i, n.clone()));
                }
            }
        }
        for (name, child) in &self.children {
            let mut segs = prefix.to_vec();
            segs.push(name.clone());
            child.collect_field_names(&segs, out);
        }
    }

    /// A free function/macro's own `FnDef`, found by direct descent to its
    /// already-fully-qualified `Path` — no ancestor-chain search, no
    /// visibility check (registration is never a visibility decision, see
    /// [`Self::get_or_create`]'s doc comment). Used by every consumer that
    /// already has a resolved target in hand rather than a bare name to
    /// search for: `MacroExpander::expand_macro`'s checker-driven lookup,
    /// and the JIT/SCC machinery's "is this already compiled" checks.
    pub fn get_fn(&self, path: &Path) -> Option<Rc<FnDef>> {
        self.find(&path.parent())?.fns.get(path.last_segment()).cloned()
    }

    /// A global's own `Slot`, found by direct descent to its already-fully-
    /// qualified `Path` — no ancestor-chain search (used by
    /// `Interp::promote_global`, always called with a `collect_global_targets`
    /// result, i.e. an already-resolved absolute path, never a bare name to
    /// search for).
    pub(crate) fn get_global(&self, path: &Path) -> Option<Slot> {
        self.find(&path.parent())?.globals.get(path.last_segment()).cloned()
    }

    /// [`Self::get_fn`]'s twin for a `(type, method)` pair.
    pub fn get_method(&self, type_name: &Path, method: &str) -> Option<Rc<FnDef>> {
        self.find(&type_name.parent())?.methods.get(&(type_name.last_segment().to_string(), method.to_string())).cloned()
    }

    /// Whether `path`'s `FnDef` already has a JIT/AOT-compiled form — the
    /// direct replacement for the old `Interp::compiled: HashMap<Path, _>`'s
    /// `contains_key` check (see [`super::interp::FnDef::compiled`]'s doc
    /// comment for why this now lives on the node itself).
    pub fn fn_compiled(&self, path: &Path) -> bool {
        self.get_fn(path).is_some_and(|f| f.compiled.borrow().is_some())
    }

    /// [`Self::fn_compiled`]'s twin for a `(type, method)` pair.
    pub fn method_compiled(&self, type_name: &Path, method: &str) -> bool {
        self.get_method(type_name, method).is_some_and(|f| f.compiled.borrow().is_some())
    }

    /// Register `def` under `name` in this module, replacing whatever was
    /// there. The raw door: the interpreter's own `defun`/`defmethod`
    /// execution goes through `Interp::register_fn`, which builds the `FnDef`
    /// out of a checked core form. This is for a caller that already holds
    /// one — the backend's SCC tests build two mutually recursive `FnDef`s
    /// directly, because the checker's forward-reference rules will not let
    /// that call graph be written as source.
    pub fn define_fn(&mut self, name: String, def: Rc<FnDef>) {
        self.fns.insert(name, def);
    }

    /// Whether a `(type, method)` pair is registered at all — the direct
    /// replacement for the old `Interp::methods.contains_key` check.
    pub fn has_method(&self, type_name: &Path, method: &str) -> bool {
        self.get_method(type_name, method).is_some()
    }

    /// Every registered free function/macro's own unqualified name, across
    /// every module in the tree — the direct replacement for the old
    /// `Interp::fns.keys()` full-table scan (`Interp::function_names`, a
    /// tooling/test-only "enumerate everything regardless of module"
    /// consumer).
    pub(crate) fn all_fn_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_fn_names(&mut out);
        out
    }

    fn collect_fn_names(&self, out: &mut Vec<String>) {
        out.extend(self.fns.keys().cloned());
        for child in self.children.values() {
            child.collect_fn_names(out);
        }
    }

    /// Flatten the whole tree's `TypeEntry`s back into the
    /// `(HashSet<Path>, HashMap<Path, EnumDef>)` shape `compile::core_bridge`
    /// (deliberately `Registry`-free, see that module's own doc comment)
    /// expects as a plain snapshot — the same "flatten once for the compile
    /// boundary" treatment `compiled_globals` already gets when cloned into
    /// `core_bridge::Ctx::globals`. Called once per JIT/AOT compile, not on
    /// any interpreted hot path.
    pub fn collect_struct_and_enum_types(
        &self,
    ) -> (HashMap<Path, Vec<crate::check::repr::Repr>>, HashMap<Path, EnumDef>) {
        let mut structs = HashMap::new();
        let mut enums = HashMap::new();
        self.collect_types_into(&[], &mut structs, &mut enums);
        (structs, enums)
    }

    /// Every registered type's field type templates, for an AOT executable's
    /// startup registration (`typelisp_print::aot::rt_print_field_template`):
    /// `(path, variant or None, field index or None for every field,
    /// template)`, in tree order.
    /// Every function and method registered anywhere under this node — what
    /// a worker thread's printer is given a snapshot of (`worker_print`), and
    /// where a `~/name/` directive's definition is found.
    pub(crate) fn collect_defs(&self, out: &mut Vec<Rc<FnDef>>) {
        out.extend(self.fns.values().cloned());
        out.extend(self.methods.values().cloned());
        for child in self.children.values() {
            child.collect_defs(out);
        }
    }

    pub(crate) fn collect_field_templates(&self, prefix: &[String], out: &mut Vec<(Path, Option<usize>, Option<usize>, TypeId)>) {
        for (name, entry) in &self.types {
            let mut segs = prefix.to_vec();
            segs.push(name.clone());
            let path = Path::from_segments(segs);
            match entry {
                TypeEntry::Struct { templates: FieldTemplates::Positional(ts), .. } => {
                    for (i, t) in ts.iter().enumerate() {
                        out.push((path.clone(), None, Some(i), *t));
                    }
                }
                TypeEntry::Struct { templates: FieldTemplates::Uniform(t), .. } => {
                    out.push((path.clone(), None, None, *t));
                }
                TypeEntry::Enum(def) => {
                    for (v, ts) in def.templates.iter().enumerate() {
                        for (i, t) in ts.iter().enumerate() {
                            out.push((path.clone(), Some(v), Some(i), *t));
                        }
                    }
                }
            }
        }
        for (name, child) in &self.children {
            let mut segs = prefix.to_vec();
            segs.push(name.clone());
            child.collect_field_templates(&segs, out);
        }
    }

    fn collect_types_into(
        &self,
        prefix: &[String],
        structs: &mut HashMap<Path, Vec<crate::check::repr::Repr>>,
        enums: &mut HashMap<Path, EnumDef>,
    ) {
        for (name, entry) in &self.types {
            let mut segs = prefix.to_vec();
            segs.push(name.clone());
            let path = Path::from_segments(segs);
            match entry {
                TypeEntry::Struct { reprs, .. } => {
                    structs.insert(path, reprs.clone());
                }
                TypeEntry::Enum(def) => {
                    enums.insert(path, def.clone());
                }
            }
        }
        for (name, child) in &self.children {
            let mut segs = prefix.to_vec();
            segs.push(name.clone());
            child.collect_types_into(&segs, structs, enums);
        }
    }
}
