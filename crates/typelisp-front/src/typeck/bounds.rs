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

use super::expr::Cx;
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
        let assoc = parse_pins(heap, tcx, ":where", trait_path, tdef, pins)?;
        out.push((p, TraitBound { trait_path, assoc }));
    }
    Ok(out)
}

/// The pins `(Assoc Type)...` a bound on the trait `trait_path` writes,
/// read where `tcx` says; `what` names the form in the messages.
pub(crate) fn parse_pins(heap: &Heap, tcx: &TypeCx, what: &str, trait_path: Path, tdef: &TraitDef, pins: &[Value]) -> Result<BTreeMap<String, Type>, Error> {
    let mut assoc = BTreeMap::new();
    for pin in pins {
        let pair = heap.list_to_vec(*pin)?;
        let [Value::Symbol(assoc_name), ty] = pair.as_slice() else {
            return Err(Error::TypeError(format!("{}: associated-type pin must be (AssocName Type)", what)));
        };
        if !tdef.params[1..].iter().any(|a| a.name.name() == assoc_name.name()) {
            return Err(Error::TypeError(format!("{}: trait `{}` has no associated type `{}`", what, trait_path, heap.symbol_spelling(*assoc_name))));
        }
        assoc.insert(assoc_name.name().to_string(), parse_type(heap, tcx, *ty)?);
    }
    Ok(assoc)
}

impl TraitBound {
    /// This bound with `s` applied to the types it pins.
    pub(crate) fn at(&self, s: &Subst) -> TraitBound {
        TraitBound { trait_path: self.trait_path, assoc: self.assoc.iter().map(|(k, v)| (k.clone(), s.apply(v))).collect() }
    }
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
pub(crate) fn supertrait_at(reg: &Registry, tdef: &TraitDef, at: &[Option<Type>], sup: &TraitBound) -> Option<Vec<Option<Type>>> {
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
pub(crate) fn trait_def<'r>(reg: &'r Registry, path: &Path) -> Option<&'r TraitDef> {
    match reg.types.get(path) {
        Some(TypeEntry::Trait(def)) => Some(def),
        _ => None,
    }
}

/// Whether a type meets a bound.
pub(crate) enum Meets {
    Yes,
    No(Unmet),
    /// The type is a hole nothing has decided yet.
    NotYet,
}

/// Whether `ty` meets the bound `b`, whose pins are written in the types
/// of this form. A pin is made to agree with what the `impl` binds the
/// associated type to, which may decide the pin's holes. A type parameter
/// meets a bound by a bound of its own in `cx`'s scope.
pub(crate) fn meets(cx: &Cx, ty: &Type, b: &TraitBound) -> Meets {
    // Which `impl` applies may be picked by the type's arguments, so a
    // number literal's hole in them is decided first.
    if let Some(path) = crate::types::nominal_path(&cx.resolve(ty)) {
        if cx.reg.trait_impls.get(&(b.trait_path, path)).is_some_and(|all| all.iter().any(|i| i.args.is_some())) {
            cx.decide_all(ty);
        }
    }
    meets_at(Asker::Checking(cx), &cx.resolve(ty), b, 0)
}

/// [`meets`] of a settled type, which has no hole and no type parameter:
/// what lowering asks.
pub(crate) fn settled_meets(reg: &Registry, ty: &Type, b: &TraitBound) -> bool {
    matches!(meets_at(Asker::Settled(reg), ty, b, 0), Meets::Yes)
}

/// Who asks whether a type meets a bound: a form being checked, whose
/// types may hold holes and its scope's type parameters, or lowering, whose
/// types are settled.
#[derive(Clone, Copy)]
enum Asker<'c, 'a> {
    Checking(&'c Cx<'a>),
    Settled(&'c Registry),
}

impl Asker<'_, '_> {
    fn reg(&self) -> &Registry {
        match self {
            Asker::Checking(cx) => cx.reg,
            Asker::Settled(reg) => reg,
        }
    }

    fn resolve(&self, ty: &Type) -> Type {
        match self {
            Asker::Checking(cx) => cx.resolve(ty),
            Asker::Settled(_) => ty.clone(),
        }
    }

    /// Whether the type a bound pins, `required`, agrees with the one the
    /// `impl` binds, `actual`, deciding `required`'s holes.
    fn agrees(&self, required: &Type, actual: &Type) -> bool {
        match self {
            Asker::Checking(cx) => cx.holes.borrow_mut().unify(required, actual).is_ok(),
            Asker::Settled(_) => required == actual,
        }
    }
}

fn meets_at(q: Asker, ty: &Type, b: &TraitBound, depth: usize) -> Meets {
    assert!(depth < 64, "the bounds of `impl`s of `{}` do not end", b.trait_path);
    let path = match ty {
        Type::Infer(_) => return Meets::NotYet,
        // It has no values: nothing can be asked of one.
        Type::Never => return Meets::Yes,
        Type::Param(p) => match q {
            Asker::Checking(cx) => return param_meets(cx, *p, b),
            Asker::Settled(_) => unreachable!("a settled type has no type parameter, but `{}` is one", p.name.name()),
        },
        Type::Dyn(trait_path, pins) => return dyn_meets(q.reg(), ty, *trait_path, pins, b),
        Type::Named(path, _) => *path,
        other => match prim_type_path(other) {
            Some(path) => path,
            // A type with no name, as `()`, has no `impl`s.
            None => return Meets::No(Unmet::Unimplemented { ty: ty.clone(), trait_path: b.trait_path, via: None }),
        },
    };
    let args: &[Type] = match ty {
        Type::Named(_, args) => args,
        _ => &[],
    };
    let reg = q.reg();
    let found = reg.trait_impls.get(&(b.trait_path, path)).and_then(|all| all.iter().find(|i| i.args.as_deref().is_none_or(|a| a == args)));
    let Some(imp) = found else {
        if reg.blanket_impls.get(&b.trait_path).is_some() {
            unimplemented!("typeck: whether a blanket `impl` of `{}` covers `{}`", b.trait_path, ty)
        }
        return Meets::No(Unmet::Unimplemented { ty: ty.clone(), trait_path: b.trait_path, via: None });
    };
    // The `impl`'s types are written in the type's parameters.
    let type_params = reg.type_def(&path).map(|d| d.params.clone()).unwrap_or_default();
    // One written for one instantiation binds concrete types.
    let at = if imp.args.is_none() { Subst::positional(&type_params, args) } else { Subst::default() };
    for (p, bounds) in &imp.bounds {
        let arg = at.apply(&Type::Param(*p));
        for inner in bounds {
            match meets_at(q, &q.resolve(&arg), &inner.at(&at), depth + 1) {
                Meets::Yes => {}
                Meets::No(_) => return Meets::No(Unmet::Unimplemented { ty: ty.clone(), trait_path: b.trait_path, via: None }),
                Meets::NotYet => return Meets::NotYet,
            }
        }
    }
    for (name, required) in &b.assoc {
        let actual = imp.assoc.get(name).map(|t| at.apply(t));
        if !actual.as_ref().is_some_and(|actual| q.agrees(required, actual)) {
            return Meets::No(Unmet::Assoc { ty: ty.clone(), assoc: name.clone(), actual, required: q.resolve(required) });
        }
    }
    Meets::Yes
}

/// Whether the type parameter `p` meets `b` by its own bounds: one on `b`'s
/// trait or on a trait that has it among its supertraits, pinning what `b`
/// pins to the same types.
fn param_meets(cx: &Cx, p: TypeParam, b: &TraitBound) -> Meets {
    let undeclared = || Meets::No(Unmet::Undeclared { var: p.name.name().to_string(), trait_path: b.trait_path });
    let Some(tdef) = trait_def(cx.reg, &b.trait_path) else { return undeclared() };
    for own in cx.scope.bounds_of(p) {
        let Some(own_at) = bound_at(cx.reg, Type::Param(p), own) else { continue };
        let Some(at) = trait_at(cx.reg, own.trait_path, own_at, &b.trait_path, 0) else { continue };
        let pinned = b.assoc.iter().all(|(name, required)| {
            let i = tdef.params.iter().position(|a| a.name.name() == name).expect("a bound pins an associated type of its trait");
            at[i].as_ref().is_some_and(|t| cx.holes.borrow_mut().unify(required, t).is_ok())
        });
        if pinned {
            return Meets::Yes;
        }
    }
    undeclared()
}

/// Whether the trait object `ty`, of the trait `trait_path` pinned to
/// `pins`, meets `b`: its trait is `b`'s or has it among its supertraits,
/// pinning what `b` pins to the same types. Nothing else is known of the
/// value inside.
fn dyn_meets(reg: &Registry, ty: &Type, trait_path: Path, pins: &[Type], b: &TraitBound) -> Meets {
    let unimplemented = || Meets::No(Unmet::Unimplemented { ty: ty.clone(), trait_path: b.trait_path, via: None });
    let Some(tdef) = trait_def(reg, &b.trait_path) else { return unimplemented() };
    let at = pins.iter().cloned().map(Some).collect();
    let Some(at) = trait_at(reg, trait_path, at, &b.trait_path, 0) else { return unimplemented() };
    for (name, required) in &b.assoc {
        let i = tdef.params.iter().position(|a| a.name.name() == name).expect("a bound pins an associated type of its trait");
        if at[i].as_ref() != Some(required) {
            return Meets::No(Unmet::Assoc { ty: ty.clone(), assoc: name.clone(), actual: at[i].clone(), required: required.clone() });
        }
    }
    Meets::Yes
}

/// The parameters of the trait `want` at the trait `from`'s parameters
/// `at`, when `want` is `from` or one of its supertraits.
fn trait_at(reg: &Registry, from: Path, at: Vec<Option<Type>>, want: &Path, depth: usize) -> Option<Vec<Option<Type>>> {
    assert!(depth < 64, "the supertraits of `{}` do not end", from);
    if from == *want {
        return Some(at);
    }
    let tdef = trait_def(reg, &from)?;
    tdef.supertraits.iter().find_map(|sup| {
        let sup_at = supertrait_at(reg, tdef, &at, sup)?;
        trait_at(reg, sup.trait_path, sup_at, want, depth + 1)
    })
}
