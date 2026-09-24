//! A runtime mirror of the checker's module tree
//! ([`crate::check::registry::Namespace`]), replacing the flat `Path`-keyed
//! tables [`Interp`](super::interp::Interp) used to hold for every top-level
//! `defun`/`defmethod`/`defvar`/`defstruct`/`defenum`. Each definition is
//! registered at the tree node for its own defining module, under its own
//! unqualified name — so no single table spans the whole program. Resolving
//! a reference re-walks this tree from the reference's own lexical module
//! (`check::resolved::Ref::home`), independently reproducing
//! `Checker::resolve_fn`/`resolve_fn_path`/`resolve_global`/`resolve_global_path`
//! (`check/checker.rs`) rather than trusting the checker's already-resolved
//! `Path` as a lookup key — the point of this module existing at all.
//!
//! Two access patterns only, matching the checker's own two resolution
//! shapes:
//! - **Direct qualified descent** ([`ModuleScope::find`]/[`get_or_create`]):
//!   walk straight down by an absolute `Path`'s segments. Used for
//!   registration (a `TopLevel` node's own `Path` is never a visibility
//!   decision — the checker already placed it uniquely) and for anything
//!   whose target module is already known outright rather than searched —
//!   an instance/static method's receiver type, a `defstruct`/`defenum`'s own
//!   type identity, or a macro-expansion lookup driven by the checker's own
//!   already-resolved macro path.
//! - **Ancestor-chain resolution** ([`ModuleScope::resolve_fn`]/
//!   [`resolve_global`]): given a lexical `home` module and the name as
//!   actually written at the reference site, walk from `home` up to the
//!   root (mirroring `Checker::ns_ancestors`, checker.rs:958-966) for a bare
//!   name, or descend to an explicitly qualified module and apply the same
//!   `pub`-or-`in_scope` gate `Checker::resolve_fn_path` does
//!   (checker.rs:1078-1093). Used by `call`/`Global`/`FnRef`/
//!   `SetGlobal` eval, and by `(compile name)` (see `Interp::method_key`'s
//!   replacement).

use std::collections::HashMap;
use std::rc::Rc;

use crate::Path;

use super::interp::{EnumDef, FnDef};
use super::value::Slot;

/// A user `defstruct`/`defenum`'s runtime classification, keyed by its own
/// unqualified name at the tree node for its defining module — the
/// distributed replacement for the old `Interp::struct_types`/`enum_defs`
/// flat `HashSet<Path>`/`HashMap<Path, EnumDef>`. Membership here is a
/// structural fact about the type's own representation, not a visibility
/// gate (the checker already validated that any reference to the type
/// itself was in scope when it type-checked the reference) — so lookup
/// never needs a `pub`/`in_scope` check the way [`ModuleScope::resolve_fn`]
/// does.
pub(crate) enum TypeEntry {
    /// A `defstruct`, with its fields' representations — what
    /// `(defstruct PATH (REPR...) (TEMPLATE...))` publishes, and what the
    /// compile bridge reads a `construct`'s and a pattern's field kinds from
    /// — and its fields' key templates, the printer's question. A built-in
    /// that reuses the struct representation but whose fields are internal
    /// (`Vector<T>`) records no representations: nothing ever names those
    /// fields. Its *elements* still print, so it records a uniform template.
    Struct { reprs: Vec<crate::check::repr::Repr>, templates: FieldTemplates },
    /// A `defenum` — carries the same per-variant field-type data
    /// `enum_defs` used to.
    Enum(EnumDef),
}

/// A struct's fields' types as key templates (`type_key::field_key_template`):
/// one per declared field, or one for every element of a type whose fields
/// are its elements (`Vector<T>`, whose count only the value knows).
#[derive(Clone)]
pub(crate) enum FieldTemplates {
    Positional(Vec<String>),
    Uniform(String),
}

impl FieldTemplates {
    pub(crate) fn get(&self, index: usize) -> Option<&str> {
        match self {
            FieldTemplates::Positional(v) => v.get(index).map(String::as_str),
            FieldTemplates::Uniform(t) => Some(t),
        }
    }
}

/// A registered `defvar`/`defconstant`'s slot plus its own `pub` bit — the
/// runtime twin of `check::registry::VarInfo::public`, needed so a
/// qualified-path `Global`/`SetGlobal` reference can enforce
/// `Checker::resolve_global_path`'s `public || in_scope` gate without the
/// checker's `Registry` at runtime, exactly like [`super::interp::FnDef::public`]
/// does for a qualified function reference.
pub(crate) struct GlobalDef {
    pub(crate) slot: Slot,
    pub(crate) public: bool,
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
    pub(crate) globals: HashMap<String, GlobalDef>,
    /// `defstruct`/`defenum`, by unqualified name.
    pub(crate) types: HashMap<String, TypeEntry>,
    /// Child modules, by unqualified name.
    pub(crate) children: HashMap<String, ModuleScope>,
}

/// Whether `home` (a reference's lexical module) is `owner` (a definition's
/// module) or nested inside it — the runtime twin of `Checker::in_scope`
/// (checker.rs:974-976): Rust-style module privacy, where a private item is
/// visible to its own module and every descendant module.
fn in_scope(home: &[String], owner: &[String]) -> bool {
    home.len() >= owner.len() && home[..owner.len()] == *owner
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
        self.get_or_create(name.parent()).types.insert(name.last_segment().to_string(), TypeEntry::Enum(def));
    }

    /// The name of `variant` of the enum registered at `name`, or `None` when
    /// no enum is registered there.
    ///
    /// The printer's question ([`typelisp_print::PrintEnv::enum_variant_name`]):
    /// an enum box stores its type key and variant *index*, never the name.
    /// A direct walk down the module chain rather than
    /// [`Self::collect_struct_and_enum_types`], because this is asked once per
    /// enum *value* rendered and that one builds a map of every type in the
    /// program.
    pub(crate) fn enum_variant_name(&self, name: &Path, variant: usize) -> Option<String> {
        let mut ns = self;
        for seg in name.parent() {
            ns = ns.children.get(seg)?;
        }
        match ns.types.get(name.last_segment())? {
            TypeEntry::Enum(def) => def.variants.get(variant).map(|(n, _)| n.clone()),
            TypeEntry::Struct { .. } => None,
        }
    }

    /// The key template of field `index` of the type registered at `name` —
    /// of `variant` for an enum, of the struct otherwise — or `None` when no
    /// such type, variant or field is registered. The printer's other
    /// question ([`typelisp_print::PrintEnv::field_is_niched_option`]),
    /// walked the same way as [`Self::enum_variant_name`].
    pub(crate) fn field_template(&self, name: &Path, variant: Option<usize>, index: usize) -> Option<&str> {
        let mut ns = self;
        for seg in name.parent() {
            ns = ns.children.get(seg)?;
        }
        match (ns.types.get(name.last_segment())?, variant) {
            (TypeEntry::Enum(def), Some(v)) => def.templates.get(v)?.get(index).map(String::as_str),
            (TypeEntry::Struct { templates, .. }, None) => templates.get(index),
            _ => None,
        }
    }

    /// Resolve a `Call`/`FnRef` reference's `written` name segments from its
    /// lexical `home` module — the runtime re-derivation of
    /// `Checker::resolve_fn`/`resolve_fn_path` (checker.rs:1061-1093).
    ///
    /// A single-segment (bare) name walks `home`'s ancestor chain, nearest
    /// module first, ending at root — exactly `Checker::ns_ancestors`
    /// (checker.rs:958-966) — with **no `pub` check**: anything found on
    /// that chain is in scope without qualification, matching
    /// `resolve_fn`'s own behavior.
    ///
    /// A multi-segment (explicitly qualified) name descends straight to the
    /// named module, then requires `public || in_scope(home, that module)`
    /// — `resolve_fn_path`'s cross-module gate.
    pub(crate) fn resolve_fn(&self, home: &[String], written: &[String]) -> Option<Rc<FnDef>> {
        if written.len() <= 1 {
            let name = written.first()?;
            for k in (0..=home.len()).rev() {
                if let Some(ns) = self.find(&home[..k]) {
                    if let Some(f) = ns.fns.get(name) {
                        return Some(f.clone());
                    }
                }
            }
            None
        } else {
            let (mods, last) = written.split_at(written.len() - 1);
            let ns = self.find(mods)?;
            let f = ns.fns.get(&last[0])?;
            if !f.public && !in_scope(home, mods) {
                return None;
            }
            Some(f.clone())
        }
    }

    /// [`Self::resolve_fn`]'s twin for `Global`/`SetGlobal` — identical
    /// shape, over `globals` instead of `fns`, mirroring
    /// `Checker::resolve_global`/`resolve_global_path` (checker.rs:1374-1410).
    pub(crate) fn resolve_global(&self, home: &[String], written: &[String]) -> Option<Slot> {
        if written.len() <= 1 {
            let name = written.first()?;
            for k in (0..=home.len()).rev() {
                if let Some(ns) = self.find(&home[..k]) {
                    if let Some(g) = ns.globals.get(name) {
                        return Some(g.slot.clone());
                    }
                }
            }
            None
        } else {
            let (mods, last) = written.split_at(written.len() - 1);
            let ns = self.find(mods)?;
            let g = ns.globals.get(&last[0])?;
            if !g.public && !in_scope(home, mods) {
                return None;
            }
            Some(g.slot.clone())
        }
    }

    /// An instance/static method lookup: `type_name` is already a fully
    /// resolved type identity (from static type inference, never searched),
    /// so this is always direct descent to `type_name.parent()` followed by
    /// a `(type_name.last_segment(), method)` lookup and a `pub`-or-`in_scope`
    /// check against `home` — the runtime twin of `Checker::assoc_visible`
    /// (checker.rs:989-991).
    pub fn resolve_method(&self, home: &[String], type_name: &Path, method: &str) -> Option<Rc<FnDef>> {
        let ns = self.find(type_name.parent())?;
        let f = ns.methods.get(&(type_name.last_segment().to_string(), method.to_string()))?;
        if !f.public && !in_scope(home, type_name.parent()) {
            return None;
        }
        Some(f.clone())
    }

    /// A free function/macro's own `FnDef`, found by direct descent to its
    /// already-fully-qualified `Path` — no ancestor-chain search, no
    /// visibility check (registration is never a visibility decision, see
    /// [`Self::get_or_create`]'s doc comment). Used by every consumer that
    /// already has a resolved target in hand rather than a bare name to
    /// search for: `MacroExpander::expand_macro`'s checker-driven lookup,
    /// and the JIT/SCC machinery's "is this already compiled" checks.
    pub fn get_fn(&self, path: &Path) -> Option<Rc<FnDef>> {
        self.find(path.parent())?.fns.get(path.last_segment()).cloned()
    }

    /// A global's own `Slot`, found by direct descent to its already-fully-
    /// qualified `Path` — no ancestor-chain search (used by
    /// `Interp::promote_global`, always called with a `collect_global_targets`
    /// result, i.e. an already-resolved absolute path, never a bare name to
    /// search for).
    pub(crate) fn get_global(&self, path: &Path) -> Option<Slot> {
        self.find(path.parent())?.globals.get(path.last_segment()).map(|g| g.slot.clone())
    }

    /// [`Self::get_fn`]'s twin for a `(type, method)` pair.
    pub fn get_method(&self, type_name: &Path, method: &str) -> Option<Rc<FnDef>> {
        self.find(type_name.parent())?.methods.get(&(type_name.last_segment().to_string(), method.to_string())).cloned()
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

    /// Every registered type's field key templates, for an AOT executable's
    /// startup registration (`typelisp_print::aot::rt_print_field_template`):
    /// `(path, variant or None, field index or None for every field,
    /// template)`, in tree order.
    /// Every method registered anywhere under this node, as `(receiver type
    /// path, method name, definition)` — what a worker thread's printer is
    /// given a snapshot of (`worker_print`).
    pub(crate) fn collect_methods(&self, prefix: &[String], out: &mut Vec<(Path, String, Rc<FnDef>)>) {
        for ((type_name, method), f) in &self.methods {
            let mut segs = prefix.to_vec();
            segs.push(type_name.clone());
            out.push((Path::from_segments(segs), method.clone(), Rc::clone(f)));
        }
        for (name, child) in &self.children {
            let mut segs = prefix.to_vec();
            segs.push(name.clone());
            child.collect_methods(&segs, out);
        }
    }

    pub(crate) fn collect_field_templates(&self, prefix: &[String], out: &mut Vec<(Path, Option<usize>, Option<usize>, String)>) {
        for (name, entry) in &self.types {
            let mut segs = prefix.to_vec();
            segs.push(name.clone());
            let path = Path::from_segments(segs);
            match entry {
                TypeEntry::Struct { templates: FieldTemplates::Positional(ts), .. } => {
                    for (i, t) in ts.iter().enumerate() {
                        out.push((path.clone(), None, Some(i), t.clone()));
                    }
                }
                TypeEntry::Struct { templates: FieldTemplates::Uniform(t), .. } => {
                    out.push((path.clone(), None, None, t.clone()));
                }
                TypeEntry::Enum(def) => {
                    for (v, ts) in def.templates.iter().enumerate() {
                        for (i, t) in ts.iter().enumerate() {
                            out.push((path.clone(), Some(v), Some(i), t.clone()));
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
