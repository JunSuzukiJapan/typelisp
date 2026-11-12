//! `:where` bounds (`docs/dev/checker-rebuild-plan.md` 7.6): reading them,
//! the methods a bounded type parameter has, and whether a type meets one.
//!
//! A bound `(Trait T (Assoc Type)...)` says the type parameter `T`
//! implements `Trait`, and may pin the trait's associated types. A type
//! implementing a trait implements its supertraits, so a bound on a trait
//! is a bound on each of its supertraits too.

use typelisp_mem::SymRef;
use crate::wk;
use super::matching::is_symbol;
use std::collections::BTreeMap;

use crate::check::core::Unmet;
use crate::typeck::registry::TraitBound;
use crate::types::{prim_type_path, Path, Type, TypeParam};
use crate::{Error, Heap, Value};

use super::expr::Cx;
use super::registry::{FnDef, Registry, TraitDef, TraitImpl, TypeEntry};
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
        let written = heap.list_to_vec_locs(*bound)?;
        let head_at = written.first().and_then(|(_, l)| l.clone());
        let parts: Vec<Value> = written.iter().map(|(p, _)| *p).collect();
        let [head, param, pins @ ..] = parts.as_slice() else {
            return Err(Error::TypeError(":where: each bound must be (Trait type-param (AssocName Type)...)".into()));
        };
        let Value::Symbol(name) = *head else {
            return Err(Error::TypeError(":where: trait name must be a symbol".into()));
        };
        let Some((trait_path, tdef)) = trait_named(tcx.reg, tcx.ns, name) else {
            return Err(Error::TypeError(format!("impl: unknown trait `{}`", heap.symbol_spelling(name))));
        };
        if let Some(entry) = tcx.reg.types.get(&trait_path) {
            super::type_syntax::note_use(tcx, trait_path, entry, head_at.as_ref());
        }
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
        let pair = heap.list_to_vec_locs(*pin)?;
        let [(Value::Symbol(assoc_name), _), (ty, ty_at)] = pair.as_slice() else {
            return Err(Error::TypeError(format!("{}: associated-type pin must be (AssocName Type)", what)));
        };
        if !tdef.params[1..].iter().any(|a| a.name.name() == assoc_name.name()) {
            return Err(Error::TypeError(format!("{}: trait `{}` has no associated type `{}`", what, trait_path, heap.symbol_spelling(*assoc_name))));
        }
        assoc.insert(assoc_name.name().to_string(), parse_type(heap, tcx, *ty, ty_at.as_ref())?);
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
    /// The trait's name of the method: an operator's is the trait method it
    /// spells (`/` is `Div`'s `div`).
    pub(crate) method: typelisp_mem::SymRef,
    pub(crate) sig: &'r FnDef,
    /// The trait's `Self` and associated types at the bound; `None` for an
    /// associated type the bound does not pin.
    pub(crate) at: Vec<Option<Type>>,
}

/// The method `name` the type parameter `p` has by its bounds in `scope`.
pub(crate) fn bound_method<'r>(reg: &'r Registry, scope: &TypeScope, p: TypeParam, name: typelisp_mem::SymRef) -> Option<BoundMethod<'r>> {
    let find = |name| {
        for b in scope.bounds_of(p) {
            let at = bound_at(reg, Type::Param(p), b)?;
            if let Some(found) = find_method(reg, b.trait_path, at, name, 0) {
                return Some(found);
            }
        }
        None
    };
    // An operator no bound declares is the trait method it spells.
    find(name).or_else(|| crate::typeck::bounds::trait_operator_method(name).and_then(find))
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
        return Some(BoundMethod { trait_path, method: name, sig, at });
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
    meets_at(Asker::Checking(cx), &cx.resolve(ty), b, &[])
}

/// [`meets`] of a settled type, which has no hole and no type parameter:
/// what lowering asks.
pub(crate) fn settled_meets(reg: &Registry, ty: &Type, b: &TraitBound) -> bool {
    matches!(meets_at(Asker::Settled(reg), ty, b, &[]), Meets::Yes)
}

/// Whether the settled type `ty` meets `b` through the blanket `impl` of
/// `b`'s trait, having no `impl` of it of its own.
pub(crate) fn blanket_covers(reg: &Registry, ty: &Type, b: &TraitBound) -> bool {
    let mut open = false;
    super::unify::walk(ty, &mut |t| open |= matches!(t, Type::Param(_) | Type::Infer(_)));
    !open && reg.blanket_impls.get(&b.trait_path).is_some() && settled_meets(reg, ty, b)
}

/// Whether a type implementing the trait `from` implements `want`: `want`
/// is `from` or one of its supertraits.
pub(crate) fn trait_implies(reg: &Registry, from: &Path, want: &Path) -> bool {
    from == want || trait_def(reg, from).is_some_and(|t| t.supertraits.iter().any(|s| trait_implies(reg, &s.trait_path, want)))
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

/// Whether `ty` meets `b`; `seen` are the bounds asked of the types on the
/// way here, which a blanket `impl`'s bounds may ask again.
fn meets_at(q: Asker, ty: &Type, b: &TraitBound, seen: &[(Path, Type)]) -> Meets {
    assert!(seen.len() < 64, "the bounds of `impl`s of `{}` do not end", b.trait_path);
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
    let mut next = seen.to_vec();
    next.push((b.trait_path, ty.clone()));
    let Some((imp, at)) = impl_for(reg, b.trait_path, path, args) else {
        let unmet = || Meets::No(Unmet::Unimplemented { ty: ty.clone(), trait_path: b.trait_path, via: None });
        let Some(blanket) = reg.blanket_impls.get(&b.trait_path) else { return unmet() };
        // Its bounds asking what is being asked already: they never end,
        // and cover nothing.
        if seen.iter().any(|(t, s)| *t == b.trait_path && s == ty) {
            return unmet();
        }
        let at = Subst::positional(&[blanket.param], std::slice::from_ref(ty));
        for inner in &blanket.bounds {
            match meets_at(q, ty, &inner.at(&at), &next) {
                Meets::Yes => {}
                Meets::No(_) => return unmet(),
                Meets::NotYet => return Meets::NotYet,
            }
        }
        if !b.assoc.is_empty() {
            unimplemented!("typeck: whether a blanket `impl` of `{}` binds the associated types a bound pins", b.trait_path)
        }
        return Meets::Yes;
    };
    for (p, bounds) in &imp.bounds {
        let arg = at.apply(&Type::Param(*p));
        for inner in bounds {
            match meets_at(q, &q.resolve(&arg), &inner.at(&at), &next) {
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

/// The `impl` of the trait `trait_path` for the type at `path` whose
/// arguments are `args`, not a blanket one, and what puts its types, which
/// are written in the type's parameters, at those arguments.
fn impl_for<'r>(reg: &'r Registry, trait_path: Path, path: Path, args: &[Type]) -> Option<(&'r TraitImpl, Subst)> {
    let imp = reg.trait_impls.get(&(trait_path, path))?.iter().find(|i| i.args.as_deref().is_none_or(|a| a == args))?;
    // One written for one instantiation binds concrete types.
    let at = match imp.args {
        Some(_) => Subst::default(),
        None => Subst::positional(&reg.type_def(&path).map(|d| d.params.clone()).unwrap_or_default(), args),
    };
    Some((imp, at))
}

/// `ty`, which a specialization settled, with each associated type of a
/// type ([`Type::Proj`]) replaced by the type that type's `impl` binds it to.
pub(crate) fn projected(reg: &Registry, ty: &Type) -> Type {
    let each = |ts: &[Type]| ts.iter().map(|t| projected(reg, t)).collect::<Vec<_>>();
    match ty {
        Type::Named(path, args) => Type::Named(*path, each(args)),
        Type::Dyn(path, pins) => Type::Dyn(*path, each(pins)),
        Type::Fn(params, rest, ret) => {
            Type::Fn(each(params), rest.as_ref().map(|t| Box::new(projected(reg, t))), Box::new(projected(reg, ret)))
        }
        Type::PtrTo(t) => Type::PtrTo(Box::new(projected(reg, t))),
        Type::Proj(trait_path, self_ty, assoc) => projected(reg, &assoc_of(reg, *trait_path, &projected(reg, self_ty), assoc)),
        other => other.clone(),
    }
}

/// The type that `ty`, a settled type implementing the trait `trait_path`,
/// binds the trait's associated type `assoc` to.
fn assoc_of(reg: &Registry, trait_path: Path, ty: &Type, assoc: &str) -> Type {
    let (path, args): (Path, &[Type]) = match ty {
        Type::Dyn(dyn_trait, pins) if *dyn_trait == trait_path => {
            let tdef = trait_def(reg, &trait_path).expect("a trait object's trait is defined");
            let i = tdef.params[1..].iter().position(|a| a.name.name() == assoc).expect("the associated type is its trait's");
            return pins[i].clone();
        }
        Type::Dyn(..) => unimplemented!("typeck: the associated type `{}` of `{}` that `{}`, a supertrait's object, binds", assoc, trait_path, ty),
        Type::Named(path, args) => (*path, args),
        other => (prim_type_path(other).unwrap_or_else(|| panic!("`{}` implements `{}`, but has no `impl`s", other, trait_path)), &[]),
    };
    match impl_for(reg, trait_path, path, args) {
        Some((imp, at)) => at.apply(imp.assoc.get(assoc).expect("an `impl` binds each associated type of its trait")),
        None if reg.blanket_impls.get(&trait_path).is_some() => {
            unimplemented!("typeck: the associated type `{}` of `{}` that a blanket `impl` gives `{}`", assoc, trait_path, ty)
        }
        None => panic!("`{}` meets the bound on `{}`, but has no `impl` of it", ty, trait_path),
    }
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

/// The type of an integer literal: the expected integer type if any, else `i32`.
/// The trait method an operator spells when its receiver is a `where`-bounded
/// type variable — `+` is `Add`'s `add`, `<` is `Ord`'s `less`.
///
/// One table, not a rule: the prelude's numeric traits pick these names
/// (`prelude.rs`'s "arithmetic traits" section says why they cannot be the
/// operators), and this is the other half of that choice. A name absent from
/// the table is passed through unchanged, and so is one a bound declares
/// itself.
pub(crate) fn trait_operator_method(op: SymRef) -> Option<SymRef> {
    const TABLE: [(&str, &str); 15] = [
        ("+", "add"),
        ("-", "sub"),
        ("*", "mul"),
        ("/", "div"),
        ("rem", "remainder"),
        ("logand", "bit-and"),
        ("logior", "bit-or"),
        ("logxor", "bit-xor"),
        ("lognot", "bit-not"),
        // `Eq`/`Ord` predate the arithmetic traits and already had these four
        // under spelled-out names; the operators reach them the same way.
        ("=", "equals"),
        ("/=", "not-equals"),
        ("<", "less"),
        ("<=", "less-equal"),
        (">", "greater"),
        (">=", "greater-equal"),
    ];
    static MEMBERS: std::sync::OnceLock<Vec<(SymRef, SymRef)>> = std::sync::OnceLock::new();
    let members = MEMBERS.get_or_init(|| {
        TABLE
            .iter()
            .map(|(o, m)| (typelisp_mem::symbols::member_symbol(o), typelisp_mem::symbols::member_symbol(m)))
            .collect()
    });
    members.iter().find(|(o, _)| *o == op).map(|(_, m)| *m)
}

/// Whether `v` is a `(:where ...)` clause (vs. an ordinary body form) —
/// `Checker::check_defun` peeks at this to decide whether to consume it.
pub(crate) fn is_where_clause(heap: &Heap, v: Value) -> Result<bool, Error> {
    Ok(matches!(v, Value::Cons(_)) && heap.list_to_vec(v)?.first().is_some_and(|h| is_symbol(*h, wk::KW_WHERE)))
}
