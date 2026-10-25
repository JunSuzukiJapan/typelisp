//! [`NameView`]: how a definition is written for someone reading in one
//! module — the one place that decides how much of a path to print.
//!
//! A definition is its module and its symbol there, and stays that
//! everywhere it is stored or compared. Only what is shown to a person is
//! shortened, the way the module they are in would let them write it:
//!
//! - a definition of the current module is its name alone, and one brought
//!   into it is the name it was brought in under;
//! - a definition in a module under the current one is the path from the
//!   current module;
//! - anything else is the whole path.
//!
//! The decision is made on the modules' tree; the string is only assembled at
//! the end.

use std::collections::HashMap;

use crate::symbols::{ns_parent, ns_path, NsId, SymRef};
use crate::TypeId;

/// The current module and what it has brought in, for writing definitions.
/// See the module documentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameView {
    module: NsId,
    /// A definition the current module has brought in -> the name it is
    /// brought in under.
    imports: HashMap<(NsId, SymRef), &'static str>,
}

impl Default for NameView {
    fn default() -> NameView {
        NameView { module: NsId::ROOT, imports: HashMap::new() }
    }
}

impl NameView {
    /// The view from `module` with `imports` (definition -> the name it is
    /// brought in under) brought in.
    pub fn new(module: NsId, imports: HashMap<(NsId, SymRef), &'static str>) -> NameView {
        NameView { module, imports }
    }

    /// The current module.
    pub fn module(&self) -> NsId {
        self.module
    }

    /// What the current module has brought in.
    pub fn imports(&self) -> &HashMap<(NsId, SymRef), &'static str> {
        &self.imports
    }

    /// The definition `sym` of the module `ns`, as written from the current
    /// module.
    pub fn definition(&self, ns: NsId, sym: SymRef) -> String {
        if ns == self.module {
            return sym.name().to_string();
        }
        if let Some(name) = self.imports.get(&(ns, sym)) {
            return name.to_string();
        }
        let mut segs = ns_path(ns);
        if self.is_below(ns) {
            segs.drain(..ns_path(self.module).len());
        }
        segs.push(sym.name().to_string());
        segs.join("::")
    }

    /// The type `ty` as written from the current module: every definition in
    /// it — the type's own and its arguments' — through [`Self::definition`].
    pub fn type_name(&self, ty: TypeId) -> String {
        crate::type_shape::display(&crate::type_shape(ty), |arg| self.type_name(arg), |ns, sym| self.definition(ns, sym))
    }

    /// Whether the module `ns` is under the current one.
    fn is_below(&self, ns: NsId) -> bool {
        let mut at = ns;
        while let Some(parent) = ns_parent(at) {
            if parent == self.module {
                return true;
            }
            at = parent;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{child_ns, intern_in};
    use crate::TypeShape;

    fn module(path: &[&str]) -> NsId {
        path.iter().fold(NsId::ROOT, |ns, s| child_ns(ns, s))
    }

    fn view(at: &[&str], imports: &[((NsId, SymRef), &'static str)]) -> NameView {
        NameView::new(module(at), imports.iter().copied().collect())
    }

    fn def(at: &[&str], name: &str) -> (NsId, SymRef) {
        let ns = module(at);
        (ns, intern_in(ns, name))
    }

    fn written(v: &NameView, (ns, sym): (NsId, SymRef)) -> String {
        v.definition(ns, sym)
    }

    #[test]
    fn a_definition_of_the_current_module_is_its_name() {
        assert_eq!(written(&view(&["nv-m"], &[]), def(&["nv-m"], "point")), "point");
        assert_eq!(written(&view(&[], &[]), def(&[], "point")), "point");
    }

    #[test]
    fn a_definition_brought_in_is_the_name_it_was_brought_in_under() {
        let array = def(&["nv-std"], "array");
        assert_eq!(written(&view(&["nv-m"], &[(array, "array")]), array), "array");
        assert_eq!(written(&view(&["nv-m"], &[(array, "arr")]), array), "arr");
    }

    #[test]
    fn a_definition_brought_in_does_not_shorten_another_of_the_same_name() {
        let array = def(&["nv-std"], "array");
        assert_eq!(written(&view(&["nv-m"], &[(array, "array")]), def(&["nv-n"], "array")), "nv-n::array");
    }

    #[test]
    fn a_definition_below_the_current_module_is_written_from_it() {
        assert_eq!(written(&view(&["nv-m"], &[]), def(&["nv-m", "n"], "point")), "n::point");
        assert_eq!(written(&view(&[], &[]), def(&["nv-m", "n"], "point")), "nv-m::n::point");
    }

    #[test]
    fn anything_else_is_the_whole_path() {
        assert_eq!(written(&view(&["nv-m"], &[]), def(&["nv-n"], "point")), "nv-n::point");
        assert_eq!(written(&view(&["nv-m", "k"], &[]), def(&["nv-m"], "point")), "nv-m::point");
    }

    #[test]
    fn every_definition_in_a_type_is_written_from_the_current_module() {
        let array = def(&["nv-std"], "array");
        let v = view(&["nv-m"], &[(array, "array")]);
        let named = |(ns, sym): (NsId, SymRef), args: &[TypeId]| crate::intern_type(TypeShape::Named(ns, sym, args.to_vec()));
        let point = named(def(&["nv-m"], "point"), &[]);
        let cell = named(def(&["nv-m", "n"], "cell"), &[]);
        let int = named(def(&[], "int"), &[]);
        let shape = def(&["nv-m"], "shape");
        let dyn_shape = crate::intern_type(TypeShape::Dyn(shape.0, shape.1, Vec::new()));
        assert_eq!(v.type_name(named(array, &[point])), "array<point>");
        assert_eq!(v.type_name(named(def(&[], "HashTable"), &[int, cell])), "HashTable<int,n::cell>");
        assert_eq!(v.type_name(crate::intern_type(TypeShape::Fn(vec![point, int], None, dyn_shape))), "(fn (point,int) dyn shape)");
    }
}
