//! The rebuilt checker's registry: what the program has defined
//! (`docs/dev/checker-rebuild-plan.md` 8).
//!
//! Definitions are keyed by their [`Path`], which already says which module
//! and which name. Every write keeps what it replaced, so the writes of a
//! top-level form whose checking fails can be undone ([`Registry::rollback`])
//! and those of one that succeeds kept ([`Registry::commit`]).

use std::collections::HashMap;

use crate::check::lower::CoreForms;
use crate::check::registry::AdtKind;
use crate::types::{Path, Type};
use typelisp_mem::SymRef;

/// A function: its parameter types and its result type.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FnDef {
    pub(crate) params: Vec<Type>,
    pub(crate) ret: Type,
}

/// A write, and what it replaced.
#[derive(Debug)]
enum Undo {
    Fn(Path, Option<FnDef>),
}

#[derive(Debug, Default)]
pub(crate) struct Registry {
    fns: HashMap<Path, FnDef>,
    /// The writes since the last [`Self::commit`], oldest first.
    undo: Vec<Undo>,
}

impl Registry {
    pub(crate) fn fn_def(&self, path: &Path) -> Option<&FnDef> {
        self.fns.get(path)
    }

    /// Define (or redefine) the function `path`. Seen at once, so a body can
    /// call the function it belongs to.
    pub(crate) fn define_fn(&mut self, path: Path, def: FnDef) {
        let replaced = self.fns.insert(path, def);
        self.undo.push(Undo::Fn(path, replaced));
    }

    /// Keep every write since the last commit.
    pub(crate) fn commit(&mut self) {
        self.undo.clear();
    }

    /// Undo every write since the last commit, newest first.
    pub(crate) fn rollback(&mut self) {
        while let Some(u) = self.undo.pop() {
            match u {
                Undo::Fn(path, Some(def)) => {
                    self.fns.insert(path, def);
                }
                Undo::Fn(path, None) => {
                    self.fns.remove(&path);
                }
            }
        }
    }
}

/// What building core IR asks of the types this registry knows. It defines
/// no types and no builtins yet, so it knows none.
impl CoreForms for Registry {
    fn type_kind(&self, _p: &Path) -> Option<AdtKind> {
        None
    }

    fn builtin_fn_sig(&self, _p: &Path) -> Option<(Vec<Type>, Type)> {
        None
    }

    fn builtin_method_sig(&self, _owner: &Path, _method: SymRef) -> Option<(Vec<Type>, Type)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typelisp_mem::symbols::NsId;

    fn f(name: &str) -> Path {
        Path::in_ns(NsId::ROOT, name)
    }

    fn def(ret: Type) -> FnDef {
        FnDef { params: Vec::new(), ret }
    }

    #[test]
    fn a_rollback_removes_what_was_defined() {
        let mut r = Registry::default();
        r.define_fn(f("a"), def(Type::Int));
        r.rollback();
        assert_eq!(r.fn_def(&f("a")), None);
    }

    #[test]
    fn a_rollback_restores_what_was_redefined() {
        let mut r = Registry::default();
        r.define_fn(f("a"), def(Type::Int));
        r.commit();
        r.define_fn(f("a"), def(Type::Bool));
        r.define_fn(f("a"), def(Type::Str));
        r.rollback();
        assert_eq!(r.fn_def(&f("a")), Some(&def(Type::Int)));
    }

    #[test]
    fn a_commit_keeps_what_was_written() {
        let mut r = Registry::default();
        r.define_fn(f("a"), def(Type::Int));
        r.commit();
        r.rollback();
        assert_eq!(r.fn_def(&f("a")), Some(&def(Type::Int)));
    }
}
