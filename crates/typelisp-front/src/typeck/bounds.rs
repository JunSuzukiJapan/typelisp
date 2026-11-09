//! `:where` bounds (`docs/dev/checker-rebuild-plan.md` 7.6): reading them,
//! the methods a bounded type parameter has, and whether a type meets one.
//!
//! A bound `(Trait T (Assoc Type)...)` says the type parameter `T`
//! implements `Trait`, and may pin the trait's associated types. A type
//! implementing a trait implements its supertraits, so a bound on a trait
//! is a bound on each of its supertraits too.

use std::collections::BTreeMap;

use crate::check::core::Unmet;
use crate::check::registry::TraitBound;
use crate::types::{prim_type_path, Path, Type, TypeParam};
use crate::{Error, Heap, Value};

use super::registry::{FnDef, Registry, TraitDef, TypeEntry};
use super::resolve::trait_named;
use super::scope::TypeScope;
use super::subst::Subst;
use super::type_syntax::{parse_type, TypeCx};

/// The clause `(:where bound...)`, written where `tcx` holds the bounded
/// definition's type parameters: each bound, by the parameter it bounds.
pub(crate) fn parse_where(heap: &Heap, tcx: &TypeCx, clause: Value) -> Result<Vec<(TypeParam, TraitBound)>, Error> {
    let elems = heap.list_to_vec(clause)?;
    let mut out = Vec::new();
    for bound in &elems[1..] {
        let parts = heap.list_to_vec(*bound)?;
        let [head, param, pins @ ..] = parts.as_slice() else {
            return Err(Error::TypeError(":where: each bound must be (Trait type-param (AssocName Type)...)".into()));
        };
        let Value::Symbol(name) = *head else {
            return Err(Error::TypeError(":where: trait name must be a symbol".into()));
        };
        let Some((trait_path, tdef)) = trait_named(tcx.reg, tcx.ns, name) else {
            return Err(Error::TypeError(format!("impl: unknown trait `{}`", heap.symbol_spelling(name))));
        };
        let Value::Symbol(param_name) = *param else {
            return Err(Error::TypeError(":where: type parameter must be a symbol".into()));
        };
        let Some(Type::Param(p)) = tcx.scope.lookup_type(param_name) else {
            return Err(Error::TypeError(format!(
                ":where: `{}` is not a type parameter this definition declares",
                heap.symbol_spelling(param_name)
            )));
        };
        let mut assoc = BTreeMap::new();
        for pin in pins {
            let pair = heap.list_to_vec(*pin)?;
            let [Value::Symbol(assoc_name), ty] = pair.as_slice() else {
                return Err(Error::TypeError(":where: associated-type pin must be (AssocName Type)".into()));
            };
            if !tdef.params[1..].iter().any(|a| a.name == *assoc_name) {
                return Err(Error::TypeError(format!(
                    ":where: trait `{}` has no associated type `{}`",
                    trait_path,
                    heap.symbol_spelling(*assoc_name)
                )));
            }
            assoc.insert(assoc_name.name().to_string(), parse_type(heap, tcx, *ty)?);
        }
        out.push((p, TraitBound { trait_path, assoc }));
    }
    Ok(out)
}

/// A method of a trait bounding a type parameter: which trait, its
/// signature, and what the trait's parameters are at the bound.
pub(crate) struct BoundMethod<'r> {
    pub(crate) trait_path: Path,
    pub(crate) sig: &'r FnDef,
    /// The trait's `Self` and associated types at the bound; `None` for an
    /// associated type the bound does not pin.
    pub(crate) at: Vec<Option<Type>>,
}

/// The method `name` the type parameter `p` has by its bounds in `scope`.
pub(crate) fn bound_method<'r>(reg: &'r Registry, scope: &TypeScope, p: TypeParam, name: typelisp_mem::SymRef) -> Option<BoundMethod<'r>> {
    for b in scope.bounds_of(p) {
        let at = bound_at(reg, Type::Param(p), b)?;
        if let Some(found) = find_method(reg, b.trait_path, at, name, 0) {
            return Some(found);
        }
    }
    None
}

/// The trait `trait_path`'s parameters, `Self` being `self_ty`, at the
/// bound `b`'s pins.
fn bound_at(reg: &Registry, self_ty: Type, b: &TraitBound) -> Option<Vec<Option<Type>>> {
    let tdef = trait_def(reg, &b.trait_path)?;
    Some(std::iter::once(Some(self_ty)).chain(tdef.params[1..].iter().map(|a| b.assoc.get(a.name.name()).cloned())).collect())
}

/// The method `name` of the trait `trait_path` or of one of its
/// supertraits, the trait's parameters being `at`.
fn find_method(reg: &Registry, trait_path: Path, at: Vec<Option<Type>>, name: typelisp_mem::SymRef, depth: usize) -> Option<BoundMethod<'_>> {
    // A trait cannot be its own supertrait, but a cycle would not end.
    assert!(depth < 64, "the supertraits of `{}` do not end", trait_path);
    let tdef = trait_def(reg, &trait_path)?;
    if let Some(sig) = tdef.method(name) {
        return Some(BoundMethod { trait_path, sig, at });
    }
    for sup in &tdef.supertraits {
        let sup_at = supertrait_at(reg, tdef, &at, sup)?;
        if let Some(found) = find_method(reg, sup.trait_path, sup_at, name, depth + 1) {
            return Some(found);
        }
    }
    None
}

/// The supertrait `sup` of the trait `tdef`, whose parameters are `at`:
/// its own parameters there.
fn supertrait_at(reg: &Registry, tdef: &TraitDef, at: &[Option<Type>], sup: &TraitBound) -> Option<Vec<Option<Type>>> {
    let sdef = trait_def(reg, &sup.trait_path)?;
    let mut out = vec![at[0].clone()];
    for a in &sdef.params[1..] {
        out.push(match sup.assoc.get(a.name.name()) {
            Some(pin) => in_params(&tdef.params, at, pin),
            None => None,
        });
    }
    Some(out)
}

/// `ty`, written in `params`, at `at`: `None` when it needs a parameter
/// `at` does not give.
pub(crate) fn in_params(params: &[TypeParam], at: &[Option<Type>], ty: &Type) -> Option<Type> {
    let mut missing = false;
    super::unify::walk(ty, &mut |t| {
        if let Type::Param(p) = t {
            if let Some(i) = params.iter().position(|q| q == p) {
                missing |= at[i].is_none();
            }
        }
    });
    (!missing).then(|| Subst::some_of(params, at).apply(ty))
}

/// The trait `path`'s definition.
fn trait_def<'r>(reg: &'r Registry, path: &Path) -> Option<&'r TraitDef> {
    match reg.types.get(path) {
        Some(TypeEntry::Trait(def)) => Some(def),
        _ => None,
    }
}

/// Whether the trait `sub` is `sup` or has it among its supertraits.
fn inherits(reg: &Registry, sub: &Path, sup: &Path, depth: usize) -> bool {
    assert!(depth < 64, "the supertraits of `{}` do not end", sub);
    sub == sup || trait_def(reg, sub).is_some_and(|d| d.supertraits.iter().any(|s| inherits(reg, &s.trait_path, sup, depth + 1)))
}

/// Why the settled type `ty` does not meet the bound `b`, or `None` when it
/// does. A type parameter meets it by a bound of its own, in `scope`.
pub(crate) fn unmet(reg: &Registry, scope: &TypeScope, ty: &Type, b: &TraitBound) -> Option<Unmet> {
    if !b.assoc.is_empty() {
        unimplemented!("typeck: a `:where` bound pinning `{}`'s associated types, at a call", b.trait_path)
    }
    let path = match ty {
        // It has no values: nothing can be asked of one.
        Type::Never => return None,
        Type::Param(p) => {
            let declared = scope.bounds_of(*p).iter().any(|own| inherits(reg, &own.trait_path, &b.trait_path, 0));
            return (!declared).then(|| Unmet::Undeclared { var: p.name.name().to_string(), trait_path: b.trait_path });
        }
        Type::Dyn(..) => unimplemented!("typeck: whether a trait object meets a `:where` bound"),
        Type::Infer(_) => unimplemented!("typeck: a `:where` bound on a type argument nothing decided"),
        Type::Named(path, _) => *path,
        other => match prim_type_path(other) {
            Some(path) => path,
            None => return Some(Unmet::NoImpls { ty: ty.clone() }),
        },
    };
    let args: &[Type] = match ty {
        Type::Named(_, args) => args,
        _ => &[],
    };
    let impls = reg.trait_impls.get(&(b.trait_path, path));
    if impls.is_some_and(|all| all.iter().any(|i| i.args.as_deref().is_none_or(|a| a == args))) {
        return None;
    }
    if reg.blanket_impls.get(&b.trait_path).is_some() {
        unimplemented!("typeck: whether a blanket `impl` of `{}` covers `{}`", b.trait_path, ty)
    }
    Some(Unmet::Unimplemented { ty: ty.clone(), trait_path: b.trait_path, via: None })
}
