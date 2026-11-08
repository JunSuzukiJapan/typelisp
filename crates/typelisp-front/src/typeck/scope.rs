//! Which type parameters a name means where a type is written
//! (`docs/dev/checker-rebuild-plan.md` 7.5).
//!
//! A type parameter exists only where a declaration made it: the `<...>` of
//! a definition's name or of `impl`, and a trait's `Self` and `(:type ...)`.
//! Writing a name inside a type never declares it.
//!
//! The frames form a chain in which each frame refers to its parent and
//! neither is copied or changed.

use crate::types::{ParamOwner, TypeParam};
use typelisp_mem::SymRef;

/// A type parameter declared twice in one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeclaredTwice(pub(crate) SymRef);

/// One frame of declared type parameters, and the frames around it.
#[derive(Debug)]
pub(crate) struct TypeScope<'p> {
    parent: Option<&'p TypeScope<'p>>,
    params: Vec<TypeParam>,
}

impl TypeScope<'static> {
    /// Where no type parameter is declared.
    pub(crate) fn empty() -> TypeScope<'static> {
        TypeScope { parent: None, params: Vec::new() }
    }
}

impl<'p> TypeScope<'p> {
    /// A frame inside `self` declaring `names`, in order, as `owner`'s
    /// parameters `0, 1, ...`.
    pub(crate) fn declare<'s>(&'s self, owner: ParamOwner, names: &[SymRef]) -> Result<TypeScope<'s>, DeclaredTwice> {
        for (i, n) in names.iter().enumerate() {
            if names[..i].contains(n) {
                return Err(DeclaredTwice(*n));
            }
        }
        let params = names.iter().enumerate().map(|(i, &name)| TypeParam { owner, index: i as u32, name }).collect();
        Ok(TypeScope { parent: Some(self), params })
    }

    /// This frame's parameters, in declaration order.
    pub(crate) fn params(&self) -> &[TypeParam] {
        &self.params
    }

    /// The parameter `name` means here: the innermost frame's that declares it.
    pub(crate) fn lookup(&self, name: SymRef) -> Option<TypeParam> {
        match self.params.iter().find(|p| p.name == name) {
            Some(p) => Some(*p),
            None => self.parent.and_then(|s| s.lookup(name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Path;
    use typelisp_mem::symbols::{intern, NsId};

    fn owner(name: &str) -> ParamOwner {
        ParamOwner::Def(Path::in_ns(NsId::ROOT, name))
    }

    #[test]
    fn a_declared_name_is_its_position() {
        let root = TypeScope::empty();
        let s = root.declare(owner("pair"), &[intern("T"), intern("U")]).unwrap();
        let u = s.lookup(intern("U")).unwrap();
        assert_eq!((u.owner, u.index), (owner("pair"), 1));
    }

    /// Nothing declares a name that is only written.
    #[test]
    fn an_undeclared_name_is_no_parameter() {
        let root = TypeScope::empty();
        let s = root.declare(owner("f"), &[intern("T")]).unwrap();
        assert_eq!(s.lookup(intern("U")), None);
        assert_eq!(root.lookup(intern("T")), None);
    }

    #[test]
    fn declared_twice() {
        let root = TypeScope::empty();
        let err = root.declare(owner("f"), &[intern("T"), intern("T")]).unwrap_err();
        assert_eq!(err, DeclaredTwice(intern("T")));
    }

    /// The `impl<T,U>` and the method's own parameters are one frame, owned
    /// by the method, so `impl<T> ... (defmethod m<T> ...)` declares `T` twice.
    #[test]
    fn an_impls_and_a_methods_parameters_are_one_frame() {
        let root = TypeScope::empty();
        let method = ParamOwner::Method(Path::in_ns(NsId::ROOT, "pair"), intern("m"));
        let s = root.declare(method, &[intern("T"), intern("U"), intern("V")]).unwrap();
        assert_eq!(s.lookup(intern("V")).map(|p| p.index), Some(2));
        assert!(root.declare(method, &[intern("T"), intern("T")]).is_err());
    }

    #[test]
    fn an_inner_frame_is_searched_first() {
        let root = TypeScope::empty();
        let outer = root.declare(owner("outer"), &[intern("T")]).unwrap();
        let inner = outer.declare(owner("inner"), &[intern("T")]).unwrap();
        assert_eq!(inner.lookup(intern("T")).map(|p| p.owner), Some(owner("inner")));
        assert_eq!(outer.lookup(intern("T")).map(|p| p.owner), Some(owner("outer")));
    }
}
