//! Inference holes and the unification that solves them
//! (`docs/dev/checker-rebuild-plan.md` 7.2).
//!
//! A call to a generic definition replaces the definition's type parameters
//! with fresh holes ([`Holes::instantiate`]) before its arguments are
//! matched against its parameters. The caller's own type parameters are
//! fixed types there, like `int`: only a hole is solved. So a recursive call
//! (`f<T>` calling `f`) cannot confuse the caller's `T` with the callee's.

use crate::types::{Type, TypeParam};

use super::subst::Subst;

/// Two types that cannot be made the same, with every hole solved so far
/// filled in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mismatch {
    pub(crate) left: Type,
    pub(crate) right: Type,
}

/// The holes of one inference and what each is solved to.
#[derive(Debug, Default)]
pub(crate) struct Holes {
    solved: Vec<Option<Type>>,
}

impl Holes {
    pub(crate) fn new() -> Holes {
        Holes::default()
    }

    /// A new, unsolved hole.
    pub(crate) fn fresh(&mut self) -> Type {
        self.solved.push(None);
        Type::Infer(self.solved.len() as u32 - 1)
    }

    /// A substitution replacing each of `params` with a fresh hole.
    pub(crate) fn instantiate(&mut self, params: &[TypeParam]) -> Subst {
        let holes: Vec<Type> = params.iter().map(|_| self.fresh()).collect();
        Subst::positional(params, &holes)
    }

    /// `ty` with every solved hole replaced by its solution, all the way down.
    pub(crate) fn resolve(&self, ty: &Type) -> Type {
        let each = |ts: &[Type]| ts.iter().map(|t| self.resolve(t)).collect::<Vec<_>>();
        match ty {
            Type::Infer(n) => match &self.solved[*n as usize] {
                Some(t) => self.resolve(t),
                None => ty.clone(),
            },
            Type::Named(path, args) => Type::Named(*path, each(args)),
            Type::Dyn(path, pins) => Type::Dyn(*path, each(pins)),
            Type::Fn(params, rest, ret) => {
                Type::Fn(each(params), rest.as_ref().map(|t| Box::new(self.resolve(t))), Box::new(self.resolve(ret)))
            }
            Type::PtrTo(t) => Type::PtrTo(Box::new(self.resolve(t))),
            other => other.clone(),
        }
    }

    /// The first hole `ty` still has after [`Self::resolve`], if any.
    pub(crate) fn unsolved(&self, ty: &Type) -> Option<u32> {
        let mut found = None;
        walk(&self.resolve(ty), &mut |t| {
            if let (None, Type::Infer(n)) = (found, t) {
                found = Some(*n);
            }
        });
        found
    }

    /// Make `left` and `right` the same type by solving holes.
    ///
    /// `!` (the type of a form that never returns) is the same as any type
    /// and solves nothing. On a mismatch, the holes solved before it stay
    /// solved; a caller that goes on after one discards these holes.
    pub(crate) fn unify(&mut self, left: &Type, right: &Type) -> Result<(), Mismatch> {
        let (l, r) = (self.resolve(left), self.resolve(right));
        let mismatch = |h: &Holes| Err(Mismatch { left: h.resolve(left), right: h.resolve(right) });
        match (&l, &r) {
            (Type::Never, _) | (_, Type::Never) => Ok(()),
            (Type::Infer(a), Type::Infer(b)) if a == b => Ok(()),
            (Type::Infer(n), other) | (other, Type::Infer(n)) => {
                if mentions_hole(other, *n) {
                    return mismatch(self);
                }
                self.solved[*n as usize] = Some(other.clone());
                Ok(())
            }
            (Type::Named(p, xs), Type::Named(q, ys)) | (Type::Dyn(p, xs), Type::Dyn(q, ys)) => {
                if p != q || xs.len() != ys.len() || std::mem::discriminant(&l) != std::mem::discriminant(&r) {
                    return mismatch(self);
                }
                for (x, y) in xs.iter().zip(ys) {
                    if self.unify(x, y).is_err() {
                        return mismatch(self);
                    }
                }
                Ok(())
            }
            (Type::Fn(ps, prest, pret), Type::Fn(qs, qrest, qret)) => {
                if ps.len() != qs.len() || prest.is_some() != qrest.is_some() {
                    return mismatch(self);
                }
                let mut pairs: Vec<(&Type, &Type)> = ps.iter().zip(qs).collect();
                if let (Some(a), Some(b)) = (prest, qrest) {
                    pairs.push((a, b));
                }
                pairs.push((pret, qret));
                for (x, y) in pairs {
                    if self.unify(x, y).is_err() {
                        return mismatch(self);
                    }
                }
                Ok(())
            }
            (Type::PtrTo(a), Type::PtrTo(b)) => {
                if self.unify(a, b).is_err() {
                    return mismatch(self);
                }
                Ok(())
            }
            _ if l == r => Ok(()),
            _ => mismatch(self),
        }
    }
}

/// Whether `ty` mentions the hole `n` (`ty` already resolved).
fn mentions_hole(ty: &Type, n: u32) -> bool {
    let mut found = false;
    walk(ty, &mut |t| found |= matches!(t, Type::Infer(m) if *m == n));
    found
}

/// Call `f` on `ty` and every type inside it.
fn walk(ty: &Type, f: &mut impl FnMut(&Type)) {
    f(ty);
    match ty {
        Type::Named(_, ts) | Type::Dyn(_, ts) => ts.iter().for_each(|t| walk(t, f)),
        Type::Fn(ps, rest, ret) => {
            ps.iter().for_each(|t| walk(t, f));
            if let Some(t) = rest {
                walk(t, f);
            }
            walk(ret, f);
        }
        Type::PtrTo(t) => walk(t, f),
        _ => {}
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

    fn func(params: Vec<Type>, ret: Type) -> Type {
        Type::Fn(params, None, Box::new(ret))
    }

    #[test]
    fn solves_a_hole_inside_a_type() {
        let mut h = Holes::new();
        let x = h.fresh();
        h.unify(&named("Option", vec![x.clone()]), &named("Option", vec![Type::Int])).unwrap();
        assert_eq!(h.resolve(&x), Type::Int);
        assert_eq!(h.unsolved(&x), None);
    }

    #[test]
    fn a_hole_solved_twice_must_agree() {
        let mut h = Holes::new();
        let x = h.fresh();
        h.unify(&x, &Type::Int).unwrap();
        let err = h.unify(&x, &Type::Str).unwrap_err();
        assert_eq!(err, Mismatch { left: Type::Int, right: Type::Str });
    }

    /// `f<T>` calling itself: the callee's `T` becomes a hole, the caller's
    /// stays a fixed type, and the hole is solved to it.
    #[test]
    fn a_recursive_call_keeps_the_callers_parameter_apart() {
        let t = param("f", 0, "T");
        let mut h = Holes::new();
        let callee = h.instantiate(&[t]);
        let callee_param = callee.apply(&named("Option", vec![Type::Param(t)]));
        let caller_arg = named("Option", vec![Type::Param(t)]);
        h.unify(&callee_param, &caller_arg).unwrap();
        assert_eq!(h.resolve(&callee.apply(&Type::Param(t))), Type::Param(t));
    }

    /// The caller's parameter is a fixed type: it is not solved to anything.
    #[test]
    fn a_parameter_is_not_a_hole() {
        let mut h = Holes::new();
        let err = h.unify(&Type::Param(param("f", 0, "T")), &Type::Int);
        assert!(err.is_err());
    }

    #[test]
    fn a_hole_cannot_contain_itself() {
        let mut h = Holes::new();
        let x = h.fresh();
        assert!(h.unify(&x, &named("Option", vec![x.clone()])).is_err());
    }

    /// A lambda's expected type may carry holes; checking its body solves them.
    #[test]
    fn holes_on_both_sides() {
        let mut h = Holes::new();
        let (a, b) = (h.fresh(), h.fresh());
        h.unify(&func(vec![a.clone()], b.clone()), &func(vec![Type::Int], a.clone())).unwrap();
        assert_eq!(h.resolve(&b), Type::Int);
    }

    #[test]
    fn never_matches_anything_and_solves_nothing() {
        let mut h = Holes::new();
        let x = h.fresh();
        h.unify(&x, &Type::Never).unwrap();
        assert_eq!(h.unsolved(&x), Some(0));
    }

    #[test]
    fn different_definitions_do_not_match() {
        let mut h = Holes::new();
        assert!(h.unify(&named("Option", vec![Type::Int]), &named("Vector", vec![Type::Int])).is_err());
        assert!(h.unify(&func(vec![Type::Int], Type::Int), &func(vec![], Type::Int)).is_err());
    }
}
