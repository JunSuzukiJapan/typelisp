//! Trait objects: `:dyn Trait<pins...>`, a value of any type implementing
//! the trait, whose methods are called through a table built where the
//! value was boxed.
//!
//! The table's slots are the trait's methods in its vtable order: every
//! method inherited from a supertrait first (the supertraits in written
//! order, a method reached twice through a diamond keeping one slot), then
//! the trait's own. The current checker lays it out the same way, and the
//! code it compiled for the prelude reads the slots by these numbers.
//!
//! A value is boxed where a trait object is expected of it: a value of a
//! type that implements the trait, with a heap representation, its `impl`
//! binding the associated types as the pins say; or a trait object of a
//! trait that inherits this one, with pins that agree.

use crate::check::core::NotDyn;
use crate::check::not_dyn;
use crate::check::registry::{AdtKind, TraitBound};
use crate::types::{nominal_path, path_is_builtin, Path, Type};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::bounds::{supertrait_at, trait_def, Meets};
use super::expr::{check_expr, Cx};
use super::generic::method_specialization;
use super::locals::Locals;
use super::lower::Lowering;
use super::registry::{Registry, TraitDef};
use super::subst::Subst;
use super::tree::{Node, Typed};

/// The trait `name`'s vtable: each slot's method and the trait that
/// declares it. `supertraits` and `own` are the trait's; `path` is the
/// trait itself. A method two supertraits declare, or one the trait
/// declares again, is refused: a slot has one name.
pub(crate) fn linearize(reg: &Registry, name: &str, path: Path, supertraits: &[TraitBound], own: &[SymRef]) -> Result<Vec<(SymRef, Path)>, Error> {
    let mut order: Vec<(SymRef, Path)> = Vec::new();
    for sup in supertraits {
        for (m, decl) in vtable(reg, &sup.trait_path)? {
            match order.iter().find(|(x, _)| *x == m) {
                // A diamond: the method is declared by the same trait both
                // times.
                Some((_, d)) if *d == decl => {}
                Some((_, d)) => {
                    return Err(Error::TypeError(format!(
                        "deftrait {}: method `{}` is inherited from both `{}` and `{}` — a vtable has one slot per name",
                        name, m, d, decl
                    )))
                }
                None => order.push((m, decl)),
            }
        }
    }
    for m in own {
        if let Some((_, d)) = order.iter().find(|(x, _)| x == m) {
            return Err(Error::TypeError(format!(
                "deftrait {}: method `{}` is already inherited from `{}` — redeclaring it would need a way to say which one a call means, and there is none",
                name, m, d
            )));
        }
        order.push((*m, path));
    }
    Ok(order)
}

/// The vtable of the trait `path`, which its `deftrait` laid out.
pub(crate) fn vtable(reg: &Registry, path: &Path) -> Result<Vec<(SymRef, Path)>, Error> {
    let Some(tdef) = trait_def(reg, path) else {
        return Err(Error::TypeError(format!("dyn: unknown trait `{}`", path)));
    };
    let own: Vec<SymRef> = tdef.methods.iter().map(|(m, _)| *m).collect();
    linearize(reg, &path.to_string(), *path, &tdef.supertraits, &own)
}

/// The trait `path`, when it can be a trait object: each of its slots has
/// one entry point every implementation fits — a `self` receiver, no other
/// mention of `Self`, no type parameters of its own and no `&rest` — and it
/// has one at all. Its definition and its vtable.
fn object_safe<'r>(reg: &'r Registry, path: &Path) -> Result<(&'r TraitDef, Vec<(SymRef, Path)>), Error> {
    let Some(tdef) = trait_def(reg, path) else {
        return Err(Error::TypeError(format!("dyn: unknown trait `{}`", path)));
    };
    let slots = vtable(reg, path)?;
    if slots.is_empty() {
        return Err(Error::TypeError(format!("dyn {}: the trait declares no methods, so a trait object of it could not be called", path)));
    }
    for (m, decl) in &slots {
        let ddef = trait_def(reg, decl).expect("a supertrait is defined");
        let sig = ddef.method(*m).expect("the declaring trait has the method");
        let whose = if decl == path { String::new() } else { format!(" (inherited from `{}`)", decl) };
        let this = Type::Param(ddef.params[0]);
        if sig.params.first() != Some(&this) {
            return Err(Error::TypeError(format!(
                "dyn {}: method `{}`{} has no `self` receiver — a static associated function has no value to dispatch on",
                path, m, whose
            )));
        }
        let mentions_self = |t: &Type| {
            let mut found = false;
            super::unify::walk(t, &mut |u| found |= *u == this);
            found
        };
        if sig.params[1..].iter().any(mentions_self) || mentions_self(&sig.ret) {
            return Err(Error::TypeError(format!(
                "dyn {}: method `{}`{} mentions `Self` outside the receiver position, so its signature differs per implementation and cannot share one vtable slot",
                path, m, whose
            )));
        }
        if sig.type_params.len() != ddef.params.len() {
            return Err(Error::TypeError(format!(
                "dyn {}: method `{}`{} is generic — a vtable slot holds one compiled entry point, so there is nothing to specialize at the call site",
                path, m, whose
            )));
        }
        if sig.rest.is_some() {
            return Err(Error::TypeError(format!("dyn {}: method `{}`{} is variadic, which has no single vtable entry point", path, m, whose)));
        }
    }
    Ok((tdef, slots))
}

/// The trait `path`'s parameters for a `:dyn path<pins>` value of type
/// `ty`: `Self` is the trait object, the associated types are the pins,
/// which must be one for each.
fn dyn_at(tdef: &TraitDef, path: &Path, ty: &Type, pins: &[Type]) -> Result<Vec<Option<Type>>, Error> {
    let assoc: Vec<&str> = tdef.params[1..].iter().map(|a| a.name.name()).collect();
    if pins.len() != assoc.len() {
        return Err(Error::TypeError(format!(
            "dyn {}: expected {} associated-type argument(s) ({}), got {} — write `:dyn {}<{}>`",
            path,
            assoc.len(),
            assoc.join(", "),
            pins.len(),
            path,
            assoc.join(",")
        )));
    }
    Ok(std::iter::once(Some(ty.clone())).chain(pins.iter().cloned().map(Some)).collect())
}

/// The parameters of `target`, a supertrait of `path` or `path` itself,
/// when `path`'s are `at` — `None` when `target` is not in `path`'s
/// supertrait chain.
fn at_in(reg: &Registry, path: &Path, at: &[Option<Type>], target: &Path) -> Option<Vec<Option<Type>>> {
    if path == target {
        return Some(at.to_vec());
    }
    let tdef = trait_def(reg, path)?;
    tdef.supertraits.iter().find_map(|sup| at_in(reg, &sup.trait_path, &supertrait_at(reg, tdef, at, sup)?, target))
}

/// `t` where the trait object `:dyn to<pins>` is expected of it.
pub(crate) fn coerce(cx: &Cx, t: Typed, to: &Path, pins: &[Type]) -> Result<Typed, Error> {
    let to_ty = Type::Dyn(*to, pins.to_vec());
    let found = cx.with_default(&t.ty);
    // Said of the type as checking it decided it.
    let refused = |why: NotDyn| Error::TypeError(not_dyn(&cx.holes.borrow().shown(&found), to, pins, &why));
    match &found {
        Type::Dyn(from, from_pins) => {
            let (fdef, from_slots) = object_safe(cx.reg, from)?;
            let (tdef, to_slots) = object_safe(cx.reg, to)?;
            let Some(implied) = at_in(cx.reg, from, &dyn_at(fdef, from, &found, from_pins)?, to) else {
                return Err(refused(NotDyn::NotInherited { from: *from }));
            };
            for (a, want) in tdef.params[1..].iter().zip(pins) {
                let i = tdef.params.iter().position(|p| p == a).expect("one of the trait's");
                match &implied[i] {
                    Some(actual) if cx.holes.borrow_mut().unify(want, actual).is_ok() => {}
                    Some(actual) => return Err(refused(NotDyn::PinDiffers { from: *from, name: a.name.name().to_string(), actual: actual.clone(), want: want.clone() })),
                    None => return Err(refused(NotDyn::Unpinned { from: *from, name: a.name.name().to_string() })),
                }
            }
            // Where the target's slots begin the source's, the box already is
            // one of the target; otherwise its table is swapped for the
            // target's.
            let loc = t.loc.clone();
            if from_slots.starts_with(&to_slots) {
                let mut t = t;
                t.ty = to_ty;
                return Ok(t);
            }
            Ok(Typed { node: Node::DynUpcast { to: *to, value: Box::new(t) }, ty: to_ty, loc })
        }
        // A type parameter has the trait by its bounds; its table is laid out
        // in each specialization.
        Type::Param(p) => {
            object_safe(cx.reg, to)?;
            if !cx.scope.bounds_of(*p).iter().any(|b| b.trait_path == *to) {
                return Err(refused(NotDyn::NotNominal));
            }
            Ok(boxed(t, *to, to_ty))
        }
        Type::Infer(_) => unimplemented!("typeck: a value whose type is not decided yet, boxed as `{}`", to_ty),
        _ => {
            let (tdef, _) = object_safe(cx.reg, to)?;
            dyn_at(tdef, to, &found, pins)?;
            admit(cx, &found, to, tdef, pins).map_err(refused)?;
            cx.decide(&t.ty);
            Ok(boxed(t, *to, to_ty))
        }
    }
}

fn boxed(t: Typed, trait_path: Path, ty: Type) -> Typed {
    let loc = t.loc.clone();
    Typed { node: Node::DynNew { trait_path, value: Box::new(t) }, ty, loc }
}

/// Whether a value of `ty`, a type that is not a type parameter, can be
/// boxed as `:dyn path<pins>`.
fn admit(cx: &Cx, ty: &Type, path: &Path, tdef: &TraitDef, pins: &[Type]) -> Result<(), NotDyn> {
    let Some(owner) = nominal_path(ty) else { return Err(NotDyn::NotNominal) };
    match super::bounds::meets(cx, ty, &TraitBound { trait_path: *path, assoc: Default::default() }) {
        Meets::Yes => {}
        Meets::No(_) => return Err(NotDyn::Unimplemented),
        Meets::NotYet => unimplemented!("typeck: whether `{}`, not decided yet, implements `{}`", ty, path),
    }
    storable(cx.reg, ty)?;
    // As `meets` decided it: an `impl` may be one instantiation's.
    let ty = &cx.resolve(ty);
    let args: &[Type] = match ty {
        Type::Named(_, args) => args,
        _ => &[],
    };
    let imp = cx
        .reg
        .trait_impls
        .get(&(*path, owner))
        .and_then(|all| all.iter().find(|i| i.args.as_deref().is_none_or(|a| a == args)))
        .expect("it meets the bound, so it has an `impl`");
    let type_params = cx.reg.type_def(&owner).map(|d| d.params.clone()).unwrap_or_default();
    let at = Subst::positional(&type_params, args);
    for (a, pin) in tdef.params[1..].iter().zip(pins) {
        let actual = at.apply(&imp.assoc[a.name.name()]);
        if cx.holes.borrow_mut().unify(pin, &actual).is_err() {
            return Err(NotDyn::Assoc { owner, name: a.name.name().to_string(), actual, pin: cx.resolve(pin) });
        }
    }
    Ok(())
}

/// Whether the box can hold a value of `ty`: one `Value` with a type key,
/// so a value with a heap representation that is not a niched `Option`.
fn storable(reg: &Registry, ty: &Type) -> Result<(), NotDyn> {
    let heap_repr = match ty {
        Type::Named(p, args) if path_is_builtin(p, typelisp_mem::wk::SCOPE) && args.len() == 1 => true,
        Type::Named(p, _) if path_is_builtin(p, typelisp_mem::wk::SEXPR) || path_is_builtin(p, typelisp_mem::wk::HASHTABLE) => true,
        Type::Named(p, _) => match reg.type_def(p) {
            Some(d) if d.kind == AdtKind::Struct => true,
            Some(d) if d.kind == AdtKind::Sum && !d.variants.is_empty() => true,
            _ => false,
        },
        Type::Dyn(..) => true,
        Type::RandomState | Type::Int | Type::Ratio | Type::Rational | Type::F64 | Type::Str => true,
        _ => false,
    };
    if !heap_repr {
        return Err(NotDyn::NoHeapRepr);
    }
    use crate::check::lower::CoreForms;
    if reg.repr(ty).niche_payload().is_some() {
        return Err(NotDyn::NichedOption);
    }
    Ok(())
}

/// Whether a `:dyn trait_path` value has the method `method`, its trait's
/// own or inherited.
pub(crate) fn has_method(reg: &Registry, trait_path: &Path, method: SymRef) -> bool {
    vtable(reg, trait_path).is_ok_and(|slots| slots.iter().any(|(m, _)| *m == method))
}

/// `(method receiver arg...)` on `receiver`, a `:dyn trait_path<pins>`:
/// the call through its table.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_dyn_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    receiver: Typed,
    trait_path: &Path,
    pins: &[Type],
    method: SymRef,
    args: &[(Value, Option<Loc>)],
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let (tdef, slots) = object_safe(cx.reg, trait_path)?;
    let at = dyn_at(tdef, trait_path, &receiver.ty, pins)?;
    let Some(slot) = slots.iter().position(|(m, _)| *m == method) else {
        return Err(Error::TypeError(format!(
            "`{}` is not a method of `{}` — a `:dyn {}` value can only call the trait's own or inherited methods",
            method, trait_path, trait_path
        )));
    };
    let decl = slots[slot].1;
    let ddef = trait_def(cx.reg, &decl).expect("a supertrait is defined");
    let sig = ddef.method(method).expect("the declaring trait has the method");
    let at = at_in(cx.reg, trait_path, &at, &decl).expect("the declaring trait is in the chain");
    let subst = Subst::some_of(&ddef.params, &at);
    let want = sig.params.len() - 1;
    if args.len() != want {
        return Err(Error::TypeError(format!("{}::{}: expected {} argument(s), got {}", trait_path, method, want, args.len())));
    }
    let mut typed = vec![receiver];
    for ((arg, arg_loc), pty) in args.iter().zip(&sig.params[1..]) {
        typed.push(check_expr(cx, heap, locals, *arg, Some(&subst.apply(pty)), arg_loc.clone())?);
    }
    let ret = subst.apply(&sig.ret);
    Ok(Typed { node: Node::DynCall { trait_path: *trait_path, method, slot, args: typed }, ty: ret, loc })
}

/// The table for boxing a value of `concrete` as `:dyn trait_path`: each
/// slot's type and the name of its method's code, and the same for each of
/// the trait's supertraits, which an upcast of the box swaps in.
pub(crate) fn tables(lw: &mut Lowering, concrete: &Type, trait_path: &Path) -> Result<(Vec<(Path, SymRef)>, Vec<(Path, Vec<(Path, SymRef)>)>), Error> {
    let to = Type::Dyn(*trait_path, Vec::new());
    storable(lw.reg, concrete).map_err(|why| Error::TypeError(not_dyn(concrete, trait_path, &[], &why)))?;
    let Some(owner) = nominal_path(concrete) else {
        return Err(Error::TypeError(not_dyn(concrete, trait_path, &[], &NotDyn::NotNominal)));
    };
    let targs: Vec<Type> = match concrete {
        Type::Named(_, args) => args.clone(),
        _ => Vec::new(),
    };
    let order = vtable(lw.reg, trait_path)?;
    let mut slots = Vec::with_capacity(order.len());
    for (m, decl) in &order {
        let name = match lw.reg.method_at(&owner, *m, &targs) {
            Some(def) if def.inst_args.is_some() => method_specialization(&owner, *m, &targs),
            Some(def) if super::toplevel::builtin_needs_specialization(lw.reg, &owner, def) => lw.requests.ask_method(&owner, *m, targs.clone()),
            Some(def) if def.sig.builtin || def.sig.type_params.is_empty() => *m,
            Some(def) if def.sig.type_params.len() == targs.len() => lw.requests.ask_method(&owner, *m, targs.clone()),
            Some(_) => unimplemented!("typeck: `{}::{}`, with type parameters of its own, in the table of `{}`", owner, m, to),
            None if lw.reg.blanket_impls.get(decl).is_some() => unimplemented!("typeck: a blanket `impl`'s method in the table of `{}`", to),
            None => return Err(Error::TypeError(format!("`{}`'s `impl {}` is missing method `{}`", owner, decl, m))),
        };
        slots.push((owner, name));
    }
    let mut supers = Vec::new();
    for sup in supertrait_closure(lw.reg, trait_path) {
        let mut entries = Vec::new();
        for (m, _) in vtable(lw.reg, &sup)? {
            let i = order.iter().position(|(x, _)| *x == m).expect("a supertrait's methods are in the trait's table");
            entries.push(slots[i]);
        }
        if !entries.is_empty() {
            supers.push((sup, entries));
        }
    }
    Ok((slots, supers))
}

/// Every supertrait of `path`, transitively, each once, in the order the
/// current checker lists them.
fn supertrait_closure(reg: &Registry, path: &Path) -> Vec<Path> {
    let mut out: Vec<Path> = Vec::new();
    let mut stack = vec![*path];
    while let Some(p) = stack.pop() {
        let Some(def) = trait_def(reg, &p) else { continue };
        for sup in &def.supertraits {
            if !out.contains(&sup.trait_path) {
                out.push(sup.trait_path);
                stack.push(sup.trait_path);
            }
        }
    }
    out
}

/// The bodies a call of `method` through a `:dyn trait_path` may run that
/// have code already: each type's whose `impl` of the trait is for every
/// instantiation of a type without parameters, or for one instantiation.
/// What the table names is what dispatches; these say which bodies have to
/// exist natively before the call can run natively.
pub(crate) fn targets(reg: &Registry, trait_path: &Path, method: SymRef) -> Vec<(Path, SymRef)> {
    let mut out = Vec::new();
    for ((t, ty), impls) in reg.trait_impls.iter() {
        if t != trait_path {
            continue;
        }
        let Some(def) = reg.type_def(ty) else { continue };
        for imp in impls {
            match &imp.args {
                None if def.params.is_empty() && reg.method(ty, method).is_some() => out.push((*ty, method)),
                Some(args) if reg.method_at(ty, method, args).is_some_and(|m| m.inst_args.is_some()) => {
                    out.push((*ty, method_specialization(ty, method, args)))
                }
                _ => {}
            }
        }
    }
    out.sort_by_key(|(ty, m)| (ty.to_string(), m.name()));
    out
}
