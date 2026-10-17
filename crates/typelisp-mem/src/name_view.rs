//! [`NameView`]: how a definition's path is written for someone reading in
//! one module — the one place that decides how much of a path to print.
//!
//! A path is the definition's identity and stays whole everywhere it is
//! stored or compared (a type key, a registry key). Only what is shown to a
//! person is shortened, the way the module they are in would let them write
//! it:
//!
//! - a definition of the current module, or one brought into it, is its last
//!   segment alone;
//! - a definition in a module under the current one is the path from the
//!   current module;
//! - anything else is the whole path.

use std::collections::HashMap;

/// The current module and what it has brought in, for writing paths. See the
/// module documentation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameView {
    /// The current module's path, outermost first; empty for the root.
    module: Vec<String>,
    /// Bare name -> the `::`-joined path of the definition the current module
    /// has brought in under it.
    imports: HashMap<String, String>,
}

impl NameView {
    /// The view from `module` with `imports` (bare name -> whole path) brought
    /// in.
    pub fn new(module: Vec<String>, imports: HashMap<String, String>) -> NameView {
        NameView { module, imports }
    }

    /// The current module's path.
    pub fn module(&self) -> &[String] {
        &self.module
    }

    /// What the current module has brought in: bare name -> whole path.
    pub fn imports(&self) -> &HashMap<String, String> {
        &self.imports
    }

    /// `path` (`::`-joined, from the root) as written from the current module.
    pub fn path(&self, path: &str) -> String {
        let segs: Vec<&str> = path.split("::").collect();
        let Some((last, owner)) = segs.split_last() else {
            return path.to_string();
        };
        if owner.iter().copied().eq(self.module.iter().map(String::as_str)) {
            return last.to_string();
        }
        if self.imports.get(*last).is_some_and(|p| p == path) {
            return last.to_string();
        }
        if owner.len() > self.module.len() && owner[..self.module.len()].iter().copied().eq(self.module.iter().map(String::as_str)) {
            return segs[self.module.len()..].join("::");
        }
        path.to_string()
    }

    /// The type key `key` as written from the current module: every path in
    /// it — the type's own and its arguments' — through [`Self::path`]. A
    /// key's paths are its runs of characters other than the brackets, the
    /// commas and the spaces that [`crate::ConcreteNames`] puts between them.
    pub fn type_key(&self, key: &str) -> String {
        let mut out = String::with_capacity(key.len());
        let mut run = String::new();
        for c in key.chars() {
            if matches!(c, '<' | '>' | ',' | ' ' | '(' | ')') {
                out.push_str(&self.path(&run));
                run.clear();
                out.push(c);
            } else {
                run.push(c);
            }
        }
        out.push_str(&self.path(&run));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(module: &[&str], imports: &[(&str, &str)]) -> NameView {
        NameView::new(
            module.iter().map(|s| s.to_string()).collect(),
            imports.iter().map(|(n, p)| (n.to_string(), p.to_string())).collect(),
        )
    }

    #[test]
    fn a_definition_of_the_current_module_is_its_last_segment() {
        assert_eq!(view(&["m"], &[]).path("m::point"), "point");
        assert_eq!(view(&[], &[]).path("point"), "point");
    }

    #[test]
    fn a_definition_brought_in_is_its_last_segment() {
        assert_eq!(view(&["m"], &[("array", "std::array")]).path("std::array"), "array");
    }

    #[test]
    fn a_name_brought_in_from_elsewhere_does_not_shorten_another_path() {
        assert_eq!(view(&["m"], &[("array", "std::array")]).path("n::array"), "n::array");
    }

    #[test]
    fn a_definition_below_the_current_module_is_written_from_it() {
        assert_eq!(view(&["m"], &[]).path("m::n::point"), "n::point");
        assert_eq!(view(&[], &[]).path("m::n::point"), "m::n::point");
    }

    #[test]
    fn anything_else_is_the_whole_path() {
        assert_eq!(view(&["m"], &[]).path("n::point"), "n::point");
        assert_eq!(view(&["m", "k"], &[]).path("m::point"), "m::point");
    }

    #[test]
    fn every_path_in_a_type_key_is_written_from_the_current_module() {
        let v = view(&["m"], &[("array", "std::array")]);
        assert_eq!(v.type_key("std::array<m::point>"), "array<point>");
        assert_eq!(v.type_key("hashtable<string,m::n::cell>"), "hashtable<string,n::cell>");
        assert_eq!(v.type_key("(fn (m::point,int) dyn m::shape)"), "(fn (point,int) dyn shape)");
    }
}
