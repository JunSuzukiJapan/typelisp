//! Replacing declared type parameters with types
//! (`docs/dev/checker-rebuild-plan.md` 7.1).
//!
//! A substitution is applied once, to every parameter at the same time: what
//! a parameter is replaced with is never itself replaced again. So replacing
//! `T` with `U` and `U` with `T` swaps them, and a parameter may be replaced
//! with a type that mentions it.

use std::collections::HashMap;

use crate::types::{Type, TypeParam};

/// A map from declared type parameters to the types that replace them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Subst {
    map: HashMap<TypeParam, Type>,
}

impl Subst {
    /// The `i`-th of `params` replaced with the `i`-th of `args` — how a
    /// definition's parameters are bound to the arguments a use of it writes.
    pub(crate) fn positional(params: &[TypeParam], args: &[Type]) -> Subst {
        assert_eq!(params.len(), args.len(), "a definition's type parameters and the arguments bound to them differ in number");
        Subst { map: params.iter().copied().zip(args.iter().cloned()).collect() }
    }

    /// Each of `params` whose entry in `args` is a type replaced with it;
    /// one whose entry is `None` is left as it is.
    pub(crate) fn some_of(params: &[TypeParam], args: &[Option<Type>]) -> Subst {
        assert_eq!(params.len(), args.len(), "a definition's type parameters and the arguments bound to them differ in number");
        Subst { map: params.iter().zip(args).filter_map(|(p, a)| a.clone().map(|a| (*p, a))).collect() }
    }

    /// `ty` with every parameter this substitution binds replaced.
    pub(crate) fn apply(&self, ty: &Type) -> Type {
        let each = |ts: &[Type]| ts.iter().map(|t| self.apply(t)).collect::<Vec<_>>();
        match ty {
            Type::Param(p) => self.map.get(p).cloned().unwrap_or(Type::Param(*p)),
            Type::Named(path, args) => Type::Named(*path, each(args)),
            Type::Dyn(path, pins) => Type::Dyn(*path, each(pins)),
            Type::Fn(params, rest, ret) => {
                Type::Fn(each(params), rest.as_ref().map(|t| Box::new(self.apply(t))), Box::new(self.apply(ret)))
            }
            Type::PtrTo(t) => Type::PtrTo(Box::new(self.apply(t))),
            other => other.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ParamOwner, Path};
    use typelisp_mem::symbols::{intern, NsId};

    fn param(owner: &str, index: u32, name: &str) -> TypeParam {
        TypeParam { owner: ParamOwner::Def(Path::in_ns(NsId::ROOT, owner)), index, name: intern(name) }
    }

    fn named(name: &str, args: Vec<Type>) -> Type {
        Type::Named(Path::in_ns(NsId::ROOT, name), args)
    }

    #[test]
    fn binds_by_position() {
        let (t, u) = (param("pair", 0, "T"), param("pair", 1, "U"));
        let s = Subst::positional(&[t, u], &[Type::Int, Type::Str]);
        let field = named("Option", vec![Type::Param(u)]);
        assert_eq!(s.apply(&field), named("Option", vec![Type::Str]));
    }

    /// The name a declaration wrote is not what identifies the parameter.
    #[test]
    fn the_name_does_not_matter() {
        let declared = param("pair", 0, "T");
        let spelled_otherwise = param("pair", 0, "A");
        let s = Subst::positional(&[declared], &[Type::Int]);
        assert_eq!(s.apply(&Type::Param(spelled_otherwise)), Type::Int);
    }

    /// Two definitions' parameters never meet, whatever they are called.
    #[test]
    fn another_definitions_parameter_is_left_alone() {
        let pairs_t = param("pair", 0, "T");
        let methods_t = TypeParam { owner: ParamOwner::Method(Path::in_ns(NsId::ROOT, "pair"), intern("mk")), index: 0, name: intern("T") };
        let s = Subst::positional(&[pairs_t], &[Type::Int]);
        assert_eq!(s.apply(&Type::Param(methods_t)), Type::Param(methods_t));
    }

    /// Applied once: a replacement is not replaced again.
    #[test]
    fn swapping_two_parameters() {
        let (t, u) = (param("f", 0, "T"), param("f", 1, "U"));
        let s = Subst::positional(&[t, u], &[Type::Param(u), Type::Param(t)]);
        let ty = Type::Fn(vec![Type::Param(t)], None, Box::new(Type::Param(u)));
        assert_eq!(s.apply(&ty), Type::Fn(vec![Type::Param(u)], None, Box::new(Type::Param(t))));
    }

    #[test]
    fn a_replacement_may_mention_the_parameter() {
        let t = param("f", 0, "T");
        let s = Subst::positional(&[t], &[named("Option", vec![Type::Param(t)])]);
        assert_eq!(s.apply(&Type::Param(t)), named("Option", vec![Type::Param(t)]));
    }
}
