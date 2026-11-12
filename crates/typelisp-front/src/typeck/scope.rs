//! Which type parameters a name means where a type is written
//! (`docs/dev/checker-rebuild-plan.md` 7.5).
//!
//! A type parameter exists only where a declaration made it: the `<...>` of
//! a definition's name or of `impl`, and a trait's `Self` and `(:type ...)`.
//! Writing a name inside a type never declares it. In an `impl` of a trait,
//! a name the trait declares an associated type stands for the type the
//! `impl` binds it to.
//!
//! A `:where` clause bounds the parameters of the frame it is written for,
//! in a frame inside that one ([`TypeScope::bound`]).
//!
//! The frames form a chain in which each frame refers to its parent and
//! neither is copied or changed.

use crate::typeck::registry::TraitBound;
use crate::types::{ParamOwner, Type, TypeParam};
use typelisp_mem::SymRef;

/// A type parameter declared twice in one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeclaredTwice(pub(crate) SymRef);

/// One frame of declared type parameters, and the frames around it.
#[derive(Debug)]
pub(crate) struct TypeScope<'p> {
    parent: Option<&'p TypeScope<'p>>,
    params: Vec<TypeParam>,
    /// Names that stand for types.
    named: Vec<(SymRef, Type)>,
    /// The bounds on parameters of the frames outside this one.
    bounds: Vec<(TypeParam, TraitBound)>,
}

impl TypeScope<'static> {
    /// Where no type parameter is declared.
    pub(crate) fn empty() -> TypeScope<'static> {
        TypeScope { parent: None, params: Vec::new(), named: Vec::new(), bounds: Vec::new() }
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
        Ok(TypeScope { parent: Some(self), params, named: Vec::new(), bounds: Vec::new() })
    }

    /// A frame inside `self` in which each name of `named` stands for its
    /// type.
    pub(crate) fn name_types<'s>(&'s self, named: Vec<(SymRef, Type)>) -> TypeScope<'s> {
        TypeScope { parent: Some(self), params: Vec::new(), named, bounds: Vec::new() }
    }

    /// A frame inside `self` in which each parameter of `bounds` is bounded
    /// by its trait.
    pub(crate) fn bound<'s>(&'s self, bounds: Vec<(TypeParam, TraitBound)>) -> TypeScope<'s> {
        TypeScope { parent: Some(self), params: Vec::new(), named: Vec::new(), bounds }
    }

    /// The bounds on the parameter `p` here.
    pub(crate) fn bounds_of(&self, p: TypeParam) -> Vec<&TraitBound> {
        let mut out: Vec<&TraitBound> = self.bounds.iter().filter(|(q, _)| *q == p).map(|(_, b)| b).collect();
        if let Some(parent) = self.parent {
            out.extend(parent.bounds_of(p));
        }
        out
    }

    /// The type `name` stands for here: the type parameter or the named type
    /// of the innermost frame that has it.
    pub(crate) fn lookup_type(&self, name: SymRef) -> Option<Type> {
        if let Some(p) = self.params.iter().find(|p| p.name == name) {
            return Some(Type::Param(*p));
        }
        // An associated type's name comes from the trait, which may have been
        // read in another module than the `impl`: the spelling is the name.
        if let Some((_, t)) = self.named.iter().find(|(n, _)| n.name() == name.name()) {
            return Some(t.clone());
        }
        self.parent.and_then(|s| s.lookup_type(name))
    }

    /// This frame's parameters, in declaration order.
    pub(crate) fn params(&self) -> &[TypeParam] {
        &self.params
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Path;
    use typelisp_mem::symbols::{intern, NsId};

    /// The type parameter `name` means in `s`, if it is one.
    fn param(s: &TypeScope, name: &str) -> Option<TypeParam> {
        match s.lookup_type(intern(name)) {
            Some(Type::Param(p)) => Some(p),
            _ => None,
        }
    }

    fn owner(name: &str) -> ParamOwner {
        ParamOwner::Def(Path::in_ns(NsId::ROOT, name))
    }

    #[test]
    fn a_declared_name_is_its_position() {
        let root = TypeScope::empty();
        let s = root.declare(owner("pair"), &[intern("T"), intern("U")]).unwrap();
        let u = param(&s, "U").unwrap();
        assert_eq!((u.owner, u.index), (owner("pair"), 1));
    }

    /// Nothing declares a name that is only written.
    #[test]
    fn an_undeclared_name_is_no_parameter() {
        let root = TypeScope::empty();
        let s = root.declare(owner("f"), &[intern("T")]).unwrap();
        assert_eq!(param(&s, "U"), None);
        assert_eq!(param(&root, "T"), None);
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
        assert_eq!(param(&s, "V").map(|p| p.index), Some(2));
        assert!(root.declare(method, &[intern("T"), intern("T")]).is_err());
    }

    #[test]
    fn an_inner_frame_is_searched_first() {
        let root = TypeScope::empty();
        let outer = root.declare(owner("outer"), &[intern("T")]).unwrap();
        let inner = outer.declare(owner("inner"), &[intern("T")]).unwrap();
        assert_eq!(param(&inner, "T").map(|p| p.owner), Some(owner("inner")));
        assert_eq!(param(&outer, "T").map(|p| p.owner), Some(owner("outer")));
    }
}
