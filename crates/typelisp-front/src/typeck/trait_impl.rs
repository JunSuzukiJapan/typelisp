//! An `impl` of a trait for a type: the trait's methods, given bodies.
//!
//! `(impl<T...>? Trait Type (:type Assoc Type)... (defmethod name (self param...) Ret body...)...)`.
//! Each method is a method of `Type`, public wherever the value is, and its
//! signature is the trait's with `Self` replaced by `Type` and each
//! associated type by the type the `impl` binds it to, by position
//! (`docs/dev/checker-rebuild-plan.md` 7.4). In a method's signature, an
//! associated type's name stands for that type.
//!
//! The `<T...>` after `impl` declares the type parameters `Type` is written
//! in. So far they are the type's own, in order: `(impl<T> Show Box<T> ...)`.
//! With none, `Type` may name every type argument outright, `(impl Show
//! Box<int> ...)`: the `impl` is that instantiation's, and so are its
//! methods, their code named for it.

use crate::types::{ParamOwner, Path, Type, TypeParam};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::define::Definer;
use super::defmethod::{define_method, receiver_type_path, Method};
use super::defun::parse_header;
use super::generic::Requests;
use super::bounds::parse_where;
use super::defun::by_param;
use super::impl_block::method_params;
use super::lambda_list::LambdaList;
use super::registry::{Definition, FnDef, MethodDef, Registry, Template, TraitBound, TraitDef, TraitImpl, TypeEntry};
use super::defun::{lower_template, split_doc, split_where};
use super::generic::Base;
use super::lower::Lowering;
use crate::check::lower::CoreForms;
use std::rc::Rc;
use super::resolve::trait_named;
use super::scope::TypeScope;
use super::subst::Subst;
use super::type_syntax::{parse_type, TypeCx};

/// `(impl Trait Type ...)` — `parts` without the head — and its core form,
/// unrooted. `type_names` are what `impl<...>` declares. The caller undoes
/// the definitions if checking fails.
pub(crate) fn check_trait_impl(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    type_names: &[SymRef],
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let ns = d.site.ns;
    let [(trait_v, trait_at), (target, target_at), items @ ..] = parts else {
        return Err(Error::TypeError("impl: (impl TraitName TargetType (:type AssocName Type)... (method ...)...)".into()));
    };
    let (trait_path, tdef) = match *trait_v {
        Value::Symbol(sym) => match trait_named(reg, ns, sym) {
            Some((p, def)) => (p, def.clone()),
            None => return Err(Error::TypeError(format!("impl: unknown trait `{}`", heap.symbol_spelling(sym)))),
        },
        Value::Path(id) => {
            let path = crate::types::written_path(heap, ns, *trait_v)?;
            match reg.types.get(&path) {
                Some(TypeEntry::Trait(def)) if def.public || super::resolve::sees(ns, path.ns()) => (path, def.clone()),
                _ => return Err(Error::TypeError(format!("impl: unknown trait `{}`", crate::types::path_spelling(heap, id)))),
            }
        }
        _ => return Err(Error::TypeError("impl: trait name must be a name or `::` path".into())),
    };
    super::type_syntax::note(d.notes, trait_path, &TypeEntry::Trait(tdef.clone()), trait_at.as_ref());
    // `(:where ...)` bounds the type's parameters: the type implements the
    // trait when its arguments meet them, and every method may use them.
    let (clause, items) = match items.split_first() {
        Some(((first, _), rest)) if crate::typeck::bounds::is_where_clause(heap, *first)? => (Some(*first), rest),
        _ => (None, items),
    };
    // `(impl<T> Trait T ...)`: every type that meets the bounds.
    if let Value::Symbol(var) = *target {
        if type_names.contains(&var) {
            return check_blanket_impl(reg, d, heap, requests, type_names, trait_path, &tdef, clause, var, items);
        }
    }
    let owner = receiver_type_path(heap, reg, ns, *target).map_err(|_| Error::TypeError("impl: target must be a data type".into()))?;
    // The target written in the type's own parameters, which `impl<...>`
    // declares in order.
    let root = TypeScope::empty();
    let head = root.declare(ParamOwner::Def(owner), type_names).map_err(|twice| {
        Error::TypeError(format!("impl {} {}: type parameter `{}` is declared twice", trait_path, owner, twice.0.name()))
    })?;
    let target_ty = parse_type(heap, &TypeCx { reg, ns, scope: &head, self_type: None, notes: d.notes }, *target, target_at.as_ref())?;
    if !super::impl_block::all_or_none(&target_ty, head.params()) {
        return Err(super::impl_block::not_all_or_none(&format!("impl {} {}", trait_path, owner), &owner, &target_ty));
    }
    super::impl_block::every_parameter_named(&format!("impl {} {}", trait_path, owner), &target_ty, head.params())?;
    let own: Vec<Type> = head.params().iter().map(|p| Type::Param(*p)).collect();
    let inst = match &target_ty {
        Type::Named(_, args) if *args == own => None,
        // Every type argument written outright: one instantiation's.
        Type::Named(_, args) if own.is_empty() => Some(args.clone()),
        // `impl<...>` declares the type's parameters in its order, and the
        // target names every one (`impl_block::in_target_order`,
        // `every_parameter_named`).
        Type::Named(..) => unreachable!("the target `{}` names the declared parameters in order", target_ty),
        _ if own.is_empty() => None,
        _ => unreachable!("the target `{}`, with no type arguments, names no declared parameter", target_ty),
    };
    // A trait is implemented either for every instantiation of a type or for
    // each one it applies to.
    let all = reg.trait_impls.get(&(trait_path, owner)).map_or(&[][..], Vec::as_slice);
    match &inst {
        Some(_) if all.iter().any(|i| i.args.is_none()) => {
            return Err(Error::TypeError(format!(
                "impl {} {}: `{}` implements `{}` for every instantiation already, so `{}` cannot have \
                 its own `impl` (a trait is implemented either for every instantiation of a type or for \
                 each one it applies to)",
                trait_path, target_ty, owner, trait_path, target_ty
            )));
        }
        None if all.iter().any(|i| i.args.is_some()) => {
            let mut written_for: Vec<String> =
                all.iter().filter_map(|i| i.args.as_ref()).map(|args| format!("`{}`", Type::Named(owner, args.clone()))).collect();
            written_for.sort();
            return Err(Error::TypeError(format!(
                "impl {} {}: `{}` is implemented for {} already, so it cannot also be implemented for \
                 every instantiation of `{}` (a trait is implemented either for every instantiation of \
                 a type or for each one it applies to)",
                trait_path,
                target_ty,
                trait_path,
                written_for.join(", "),
                owner
            )));
        }
        _ => {}
    }
    let shown = format!("impl {} {}", trait_path, owner);
    let impl_bounds = match clause {
        Some(c) => parse_where(heap, &TypeCx { reg, ns, scope: &head, self_type: None, notes: d.notes }, c)?,
        None => Vec::new(),
    };
    let (assoc, methods) = items_of(heap, reg, ns, d.notes, &head, &target_ty, &tdef, &shown, items)?;
    for (written, _) in &methods {
        if tdef.method(*written).is_none() {
            return Err(not_a_method(reg, &shown, *written, &trait_path));
        }
    }
    let mut left = Vec::new();
    for (m, _) in &tdef.methods {
        if !methods.iter().any(|(w, _)| w == m) {
            if tdef.defaults.contains(m) {
                left.push(*m);
                continue;
            }
            // A method the type already has is the trait's, when it has the
            // signature the trait gives it.
            let existing = match &inst {
                Some(args) => reg.method_at(&owner, *m, args),
                None => reg.method(&owner, *m),
            };
            let Some(existing) = existing else {
                return Err(Error::TypeError(format!("{}: missing method `{}` (`{}` declares no default body for it)", shown, m, trait_path)));
            };
            adopt(&tdef, existing, &target_ty, &assoc, head.params(), &shown, &trait_path, *m)?;
        }
    }
    for sup in &tdef.supertraits {
        // The associated types of the supertrait's `impl`, or of the blanket
        // `impl` that gives it the supertrait.
        let own_impl = implements(reg, &sup.trait_path, &owner, inst.as_deref());
        let unpinned = crate::typeck::registry::TraitBound { trait_path: sup.trait_path, assoc: Default::default() };
        if !own_impl && !super::bounds::blanket_covers(reg, &target_ty, &unpinned) {
            return Err(Error::TypeError(format!(
                "{}: `{}` requires `{}`, but `{}` has no `impl {}` — write it before this one",
                shown, trait_path, sup.trait_path, owner, sup.trait_path
            )));
        }
        let sup_assoc = |name: &str| match own_impl {
            true => reg.trait_impls.get(&(sup.trait_path, owner)).and_then(|all| impl_at(all, inst.as_deref())).expect("checked above").assoc.get(name).cloned(),
            false => reg.blanket_impls.get(&sup.trait_path).expect("a blanket `impl` covers it").assoc_at(name, &target_ty),
        };
        // What the supertrait entry pins, in this trait's parameters, is
        // what the supertrait's `impl` binds. A type with a parameter left in
        // it is not checked.
        let to_impl = Subst::positional(&tdef.params, &std::iter::once(target_ty.clone()).chain(assoc.iter().cloned()).collect::<Vec<_>>());
        let open = |t: &Type| {
            let mut found = false;
            super::unify::walk(t, &mut |u| found |= matches!(u, Type::Param(_)));
            found
        };
        for (name, pinned) in &sup.assoc {
            let want = to_impl.apply(pinned);
            match sup_assoc(name) {
                Some(actual) if actual == want || open(&want) || open(&actual) => {}
                Some(actual) => {
                    return Err(Error::TypeError(format!(
                        "{}: `{}` requires `{}`'s `{} = {}`, but `{}`'s `impl {}` binds it to `{}`",
                        shown, trait_path, sup.trait_path, name, want, owner, sup.trait_path, actual
                    )))
                }
                None => {
                    return Err(Error::TypeError(format!(
                        "{}: `{}`'s `impl {}` does not bind the associated type `{}`",
                        shown, owner, sup.trait_path, name
                    )))
                }
            }
        }
    }
    let of = Of { trait_path, owner, type_names, head: head.params(), target: &target_ty, assoc: &assoc, clause, inst: inst.as_deref(), blanket: false };
    // The methods left out are defined first, so a written one can call
    // them, and lowered last, once every method they may call is defined.
    let mut defaulted = Vec::new();
    for method in left {
        defaulted.push((method, define_defaulted(reg, d, heap, &tdef, &of, method)?));
    }
    let mut forms = Vec::new();
    let mut impl_bounds = impl_bounds;
    for (method, item) in methods {
        let want = tdef.method(method).expect("checked above").clone();
        let (form, own) = check_method(reg, d, heap, requests, &tdef, &of, method, &want, &shown, item)?;
        forms.extend(form);
        impl_bounds.extend(own);
    }
    for (method, at) in defaulted {
        // A generic one is specialized where it is called.
        let Some(at) = at else { continue };
        let (template, subst) = default_body(reg, &trait_path, method, &at);
        let mut lw = Lowering { reg, subst: &subst, requests };
        let (params, ret, body) = lower_template(heap, &mut lw, &template)?;
        let (instance, code_name) = match &inst {
            Some(args) => (reg.method_at(&owner, method, args).expect("defined above").instance, super::generic::method_specialization(&owner, method, args)),
            None => (reg.method(&owner, method).expect("defined above").instance, method),
        };
        let form = reg.defmethod_form(heap, &owner, code_name, instance, &params, &ret, &body)?;
        heap.push_root(form);
        forms.push(form);
    }
    let mut all = reg.trait_impls.get(&(trait_path, owner)).cloned().unwrap_or_default();
    all.retain(|i| i.args != inst);
    let assoc = tdef.params[1..].iter().map(|a| a.name.name().to_string()).zip(assoc).collect();
    all.push(TraitImpl { args: inst, assoc, bounds: by_param(impl_bounds) });
    reg.trait_impls.insert((trait_path, owner), all);
    crate::check::forms::module_form(heap, &owner, &forms)
}

/// The error for the `impl` `shown` writing `written`, which is not a method
/// of `trait_path`'s own: it may be one a supertrait declares.
fn not_a_method(reg: &Registry, shown: &str, written: SymRef, trait_path: &Path) -> Error {
    let inherited = super::dyn_trait::vtable(reg, trait_path).is_ok_and(|slots| slots.iter().any(|(m, _)| *m == written));
    let hint = if inherited { " — it is inherited, so it belongs to the supertrait's own `impl`" } else { "" };
    Error::TypeError(format!("{}: `{}` is not a method of `{}`{}", shown, written, trait_path, hint))
}

/// Whether `ty` implements `trait_path` at the instantiation `inst`, or for
/// every instantiation when `inst` is `None`.
fn implements(reg: &Registry, trait_path: &Path, ty: &Path, inst: Option<&[Type]>) -> bool {
    reg.trait_impls.get(&(*trait_path, *ty)).is_some_and(|all| impl_at(all, inst).is_some())
}

/// Of the `impl`s `all` of one trait for one type, the one that applies at
/// the instantiation `inst` (the instantiation's own, or the one for every
/// instantiation), or the one for every instantiation when `inst` is `None`.
fn impl_at<'a>(all: &'a [TraitImpl], inst: Option<&[Type]>) -> Option<&'a TraitImpl> {
    let every = all.iter().find(|i| i.args.is_none());
    match inst {
        Some(args) => all.iter().find(|i| i.args.as_deref() == Some(args)).or(every),
        None => every,
    }
}

/// The `impl`'s items: the types it binds the trait's associated types to,
/// in the trait's order and written in the type's parameters (`head`), and
/// its methods, each with its item.
#[allow(clippy::too_many_arguments)]
fn items_of(
    heap: &Heap,
    reg: &Registry,
    ns: typelisp_mem::symbols::NsId,
    notes: &super::expr::Notes,
    head: &TypeScope,
    target_ty: &Type,
    tdef: &TraitDef,
    shown: &str,
    items: &[(Value, Option<Loc>)],
) -> Result<(Vec<Type>, Vec<(SymRef, Value)>), Error> {
    let assoc_names: Vec<SymRef> = tdef.params[1..].iter().map(|p| p.name).collect();
    let mut bound: Vec<Option<Type>> = vec![None; assoc_names.len()];
    let mut methods: Vec<(SymRef, Value)> = Vec::new();
    for (item, _) in items {
        let written = heap.list_to_vec_locs(*item).map_err(|_| Error::TypeError("impl: item must start with a symbol".into()))?;
        let elems: Vec<Value> = written.iter().map(|(v, _)| *v).collect();
        match elems.as_slice() {
            [Value::Symbol(k), rest @ ..] if k.is(wk::KW_TYPE) => {
                let ty_at = written.get(2).and_then(|(_, l)| l.clone());
                let [name, ty] = rest else {
                    return Err(Error::TypeError("impl: (:type AssocName Type)".into()));
                };
                let Value::Symbol(name) = name else {
                    return Err(Error::TypeError("impl: associated type name must be a symbol".into()));
                };
                let Some(i) = assoc_names.iter().position(|a| a.name() == name.name()) else {
                    return Err(Error::TypeError(format!("{}: `{}` is not an associated type of the trait", shown, name.name())));
                };
                bound[i] = Some(parse_type(heap, &TypeCx { reg, ns, scope: head, self_type: Some(target_ty), notes }, *ty, ty_at.as_ref())?);
            }
            [Value::Symbol(d), rest @ ..] | [Value::Symbol(_), Value::Symbol(d), rest @ ..] if d.is(wk::DEFMETHOD) => {
                let Some(header) = rest.first() else {
                    return Err(Error::TypeError("impl: method must be (name (recv params...) ret body...)".into()));
                };
                let (name, own) = parse_header(heap, *header)?;
                let m = member_symbol(name.name());
                let declared = tdef.method(m).map_or(0, |want| want.type_params.len() - tdef.params.len());
                if tdef.method(m).is_some() && own.len() != declared {
                    return Err(Error::TypeError(format!(
                        "{}: method `{}` declares {} type parameter(s) of its own, and the trait declares {}",
                        shown,
                        m,
                        own.len(),
                        declared
                    )));
                }
                if methods.iter().any(|(w, _)| *w == m) {
                    return Err(Error::TypeError(format!("{}: method `{}` is given twice", shown, m)));
                }
                methods.push((m, *item));
            }
            _ => {
                return Err(Error::TypeError(
                    "impl: a method is written (defmethod name (params...) ReturnType body...), or (pub defmethod ...) to make it public".into(),
                ))
            }
        }
    }
    let assoc = bound
        .into_iter()
        .zip(&assoc_names)
        .map(|(t, a)| {
            t.ok_or_else(|| {
                Error::TypeError(format!("{}: associated type `{}` is not bound — write `(:type {} <type>)`", shown, a.name(), a.name()))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((assoc, methods))
}

/// What an `impl` is of and for: the trait, the type, the type parameters
/// `impl<...>` declares, the target and the associated types (both written
/// in the type's parameters).
struct Of<'a> {
    trait_path: Path,
    owner: Path,
    type_names: &'a [SymRef],
    /// The parameters `impl<...>` declares, which the target and the
    /// associated types are written in.
    head: &'a [TypeParam],
    target: &'a Type,
    assoc: &'a [Type],
    /// The `impl`'s `(:where ...)`, which bounds each method's parameters.
    clause: Option<Value>,
    /// The instantiation the `impl` is for, when it is for one.
    inst: Option<&'a [Type]>,
    /// A blanket `impl`: its type parameter is bounded by the trait too, so
    /// each method may call the trait's others.
    blanket: bool,
}

/// Whether `existing`, the method `method` the type already has, is the
/// trait's: an instance method or not as the trait declares it, with no
/// `&optional`, `&key` or `&rest`, and the trait's signature with `Self` and
/// the associated types where the `impl` puts them (`target` and `assoc`,
/// written in the type's parameters `own`).
#[allow(clippy::too_many_arguments)]
fn adopt(tdef: &TraitDef, existing: &MethodDef, target: &Type, assoc: &[Type], own: &[TypeParam], shown: &str, trait_path: &Path, method: SymRef) -> Result<(), Error> {
    let want = tdef.method(method).expect("a method of the trait");
    let to_impl = Subst::positional(&tdef.params, &std::iter::once(target.clone()).chain(assoc.iter().cloned()).collect::<Vec<_>>());
    let want_params: Vec<Type> = want.params.iter().map(|t| to_impl.apply(t)).collect();
    let want_ret = to_impl.apply(&want.ret);
    let want_instance = want.params.first() == Some(&Type::Param(tdef.params[0]));
    let kind = |i: bool| if i { "an instance method" } else { "a static function" };
    if existing.instance != want_instance {
        return Err(Error::TypeError(format!(
            "{}: method `{}` is {}, but `{}` declares it {}",
            shown,
            method,
            kind(existing.instance),
            trait_path,
            kind(want_instance)
        )));
    }
    let sig = &existing.sig;
    if !sig.optionals.is_empty() || !sig.keys.is_empty() || sig.rest.is_some() {
        return Err(Error::TypeError(format!(
            "{}: method `{}` may not declare &optional/&key/&rest — a trait method's arity is fixed by its vtable slot, and `{}` declares none",
            shown, method, trait_path
        )));
    }
    // The method's own type parameters, after the type's, are the trait
    // method's, by position.
    let want_own = &want.type_params[tdef.params.len()..];
    if sig.type_params.len() != own.len() + want_own.len() {
        return Err(Error::TypeError(format!(
            "{}: method `{}` declares {} type parameter(s) of its own, and `{}` declares {}",
            shown,
            method,
            sig.type_params.len().saturating_sub(own.len()),
            trait_path,
            want_own.len()
        )));
    }
    // The method's types are written in its own parameters, the type's.
    let reown = Subst::positional(&sig.type_params, &own.iter().chain(want_own).map(|p| Type::Param(*p)).collect::<Vec<_>>());
    let params: Vec<Type> = sig.params.iter().map(|t| reown.apply(t)).collect();
    let ret = reown.apply(&sig.ret);
    if params != want_params || ret != want_ret {
        let describe = |params: &[Type], ret: &Type| format!("({}) {}", params.iter().map(Type::to_string).collect::<Vec<_>>().join(" "), ret);
        return Err(Error::TypeError(format!(
            "{}: method `{}` is `{}`, but `{}` declares `{}`",
            shown,
            method,
            describe(&params, &ret),
            trait_path,
            describe(&want_params, &want_ret)
        )));
    }
    // Its own type parameters have the bounds the trait gives them.
    let mut have: Vec<(TypeParam, TraitBound)> = Vec::new();
    for (p, bounds) in &sig.bounds {
        if let Type::Param(q) = reown.apply(&Type::Param(*p)) {
            if want_own.contains(&q) {
                have.extend(bounds.iter().map(|b| (q, b.at(&reown))));
            }
        }
    }
    let wanted: Vec<(TypeParam, TraitBound)> = want.bounds.iter().flat_map(|(p, bs)| bs.iter().map(move |b| (*p, b.clone()))).collect();
    if have.len() != wanted.len() || !wanted.iter().all(|w| have.contains(w)) {
        return Err(Error::TypeError(format!(
            "{}: method `{}` bounds its own type parameters otherwise than `{}` declares",
            shown, method, trait_path
        )));
    }
    Ok(())
}

/// Define the method `method`, which the `impl` leaves out, as the trait's
/// default body: the trait's parameters where the `impl` puts them, written
/// in the method's type parameters. Those are returned when the method has
/// none, to lower its body at.
fn define_defaulted(reg: &mut Registry, d: &mut Definer, heap: &Heap, tdef: &TraitDef, of: &Of, method: SymRef) -> Result<Option<Vec<Type>>, Error> {
    let (ns, owner) = (d.site.ns, of.owner);
    let want = tdef.method(method).expect("a default is of the trait's own method");
    // The method's own type parameters are the trait method's, after the
    // `impl`'s.
    let names: Vec<SymRef> = of.type_names.iter().copied().chain(want.type_params[tdef.params.len()..].iter().map(|p| p.name)).collect();
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Method(owner, method), &names).map_err(|twice| {
        Error::TypeError(format!("method {}::{}: type parameter `{}` is declared twice", owner, method.name(), twice.0.name()))
    })?;
    let reown = reown(&scope, of);
    let at: Vec<Type> = std::iter::once(reown.apply(of.target)).chain(of.assoc.iter().map(|t| reown.apply(t))).collect();
    let to_impl = to_impl(tdef, want, at.clone(), &scope);
    let instance = want.params.first() == Some(&Type::Param(tdef.params[0]));
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: Some(&at[0]), notes: d.notes };
    let mut bounds = method_bounds(heap, &tcx, &scope, of)?;
    bounds.extend(trait_bounds(want, &to_impl));
    let sig = FnDef {
        public: true,
        bounds: by_param(bounds),
        ..FnDef::plain(scope.params().to_vec(), want.params.iter().map(|t| to_impl.apply(t)).collect(), to_impl.apply(&want.ret))
    };
    let generic = !scope.params().is_empty();
    match of.inst {
        // Lowered at once, the instantiation's own: no body to specialize.
        Some(args) => {
            d.inst_method(reg, &owner, method, args)?;
            reg.define_inst_method(owner, method, args.to_vec(), MethodDef { sig, instance, home: None, inst_args: Some(args.to_vec()) });
        }
        None => {
            d.method(reg, &owner, method)?;
            reg.define_defaulted_method(owner, method, MethodDef { sig, instance, home: None, inst_args: None }, of.trait_path, at.clone());
        }
    }
    Ok((!generic).then_some(at))
}

/// What puts the `impl`'s types, written in the parameters `impl<...>`
/// declares, in a method's, which `scope` declares.
fn reown(scope: &TypeScope, of: &Of) -> Subst {
    let own: Vec<Type> = scope.params()[..of.head.len()].iter().map(|p| Type::Param(*p)).collect();
    Subst::positional(of.head, &own)
}

/// The bounds the trait declares on `want`'s own type parameters, put where
/// `to_impl` puts them.
fn trait_bounds(want: &super::registry::FnDef, to_impl: &Subst) -> Vec<(TypeParam, crate::typeck::registry::TraitBound)> {
    let mut out = Vec::new();
    for (p, bounds) in &want.bounds {
        let Type::Param(q) = to_impl.apply(&Type::Param(*p)) else { unreachable!("a method's own type parameter is put at one of the method's") };
        out.extend(bounds.iter().map(|b| (q, b.at(to_impl))));
    }
    out
}

/// What puts a trait method's signature where the `impl` puts the trait's
/// parameters (`at`, `Self` and the associated types), its own type
/// parameters the method's after the `impl`'s in `scope`.
fn to_impl(tdef: &TraitDef, want: &super::registry::FnDef, at: Vec<Type>, scope: &TypeScope) -> Subst {
    let own = &scope.params()[scope.params().len() - (want.type_params.len() - tdef.params.len())..];
    Subst::positional(&want.type_params, &at.into_iter().chain(own.iter().map(|p| Type::Param(*p))).collect::<Vec<_>>())
}

/// The bounds a method of the `impl` `of` has: its `(:where ...)`, read
/// where `tcx` says, and for a blanket `impl` the trait itself on its type
/// parameter.
fn method_bounds(heap: &Heap, tcx: &TypeCx, scope: &TypeScope, of: &Of) -> Result<Vec<(TypeParam, crate::typeck::registry::TraitBound)>, Error> {
    let mut bounds = of.clause.map(|c| parse_where(heap, tcx, c)).transpose()?.unwrap_or_default();
    if of.blanket {
        bounds.push((scope.params()[0], crate::typeck::registry::TraitBound { trait_path: of.trait_path, assoc: Default::default() }));
    }
    Ok(bounds)
}

/// `(impl<T> Trait T (:where ...) items...)` — what follows the trait's
/// name being `var`, the one type parameter `impl<...>` declares: the trait
/// for every type that meets the bounds. Its methods are checked once here,
/// with `T` a parameter, and kept to be specialized at each type a call
/// reaches them at, as that type's methods.
#[allow(clippy::too_many_arguments)]
fn check_blanket_impl(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    type_names: &[SymRef],
    trait_path: Path,
    tdef: &TraitDef,
    clause: Option<Value>,
    var: SymRef,
    items: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let ns = d.site.ns;
    let names = type_names.iter().map(|n| n.name()).collect::<Vec<_>>().join(",");
    if let Some(other) = type_names.iter().find(|n| **n != var) {
        return Err(Error::TypeError(format!(
            "impl<{}> {} {}: type parameter `{}` is declared, and the target `{}` does not name it — nothing would decide it",
            names,
            trait_path,
            var.name(),
            other.name(),
            var.name()
        )));
    }
    // Which of two would apply to a type could depend on bounds met later.
    if reg.blanket_impls.get(&trait_path).is_some() {
        return Err(Error::TypeError(format!(
            "impl<{}> {} {}: `{}` already has a blanket impl — a trait may have at most one, \
             since which of two applies to a given type could depend on bounds established later",
            names,
            trait_path,
            var.name(),
            trait_path
        )));
    }
    if let Some(c) = clause {
        for bound in &heap.list_to_vec(c)?[1..] {
            if let Some(Value::Symbol(p)) = heap.list_to_vec(*bound)?.get(1) {
                if !type_names.contains(p) {
                    return Err(Error::TypeError(format!(
                        "impl<{}> {}: `:where` bounds `{}`, which is not one of the impl's type parameters",
                        names,
                        trait_path,
                        p.name()
                    )));
                }
            }
        }
    }
    // A leading space is unwritable in source.
    let owner = Path::in_ns(trait_path.ns(), &format!(" blanket {}", trait_path.name_symbol().name()));
    let root = TypeScope::empty();
    let head = root.declare(ParamOwner::Def(owner), type_names).expect("one parameter is declared once");
    let param = head.params()[0];
    let target_ty = Type::Param(param);
    let shown = format!("impl<{}> {} {}", names, trait_path, var.name());
    let impl_bounds = match clause {
        Some(c) => parse_where(heap, &TypeCx { reg, ns, scope: &head, self_type: None, notes: d.notes }, c)?,
        None => Vec::new(),
    };
    // A type the `impl` covers has the trait's supertraits: the bounds
    // have to say it does.
    for sup in &tdef.supertraits {
        let implied = impl_bounds.iter().any(|(_, b)| super::bounds::trait_implies(reg, &b.trait_path, &sup.trait_path));
        if !implied {
            return Err(Error::TypeError(format!(
                "{}: `{}` requires `{}`, but the `:where` clause does not bound `{}` by it",
                shown,
                trait_path,
                sup.trait_path,
                var.name()
            )));
        }
    }
    let (assoc, methods) = items_of(heap, reg, ns, d.notes, &head, &target_ty, tdef, &shown, items)?;
    for (written, _) in &methods {
        if tdef.method(*written).is_none() {
            return Err(not_a_method(reg, &shown, *written, &trait_path));
        }
    }
    let mut left = Vec::new();
    for (m, _) in &tdef.methods {
        if !methods.iter().any(|(w, _)| w == m) {
            if !tdef.defaults.contains(m) {
                return Err(Error::TypeError(format!("{}: missing method `{}` (`{}` declares no default body for it)", shown, m, trait_path)));
            }
            left.push(*m);
        }
    }
    let of = Of { trait_path, owner, type_names, head: head.params(), target: &target_ty, assoc: &assoc, clause, inst: None, blanket: true };
    for method in left {
        define_defaulted(reg, d, heap, tdef, &of, method)?;
    }
    let mut impl_bounds = impl_bounds;
    for (method, item) in methods {
        let want = tdef.method(method).expect("checked above").clone();
        let (_, own) = check_method(reg, d, heap, requests, tdef, &of, method, &want, &shown, item)?;
        impl_bounds.extend(own);
    }
    let bounds = impl_bounds.into_iter().map(|(_, b)| b).collect();
    let assoc = tdef.params[1..].iter().map(|a| a.name.name().to_string()).zip(assoc).collect();
    reg.define_blanket(trait_path, super::registry::BlanketImpl { owner, param, bounds, assoc });
    crate::check::forms::module_form(heap, &Path::in_ns(ns, "nothing"), &[])
}

/// How many type parameters the trait `trait_path`'s method `method`
/// declares of its own.
pub(crate) fn own_type_params(reg: &Registry, trait_path: &Path, method: SymRef) -> usize {
    let Some(TypeEntry::Trait(tdef)) = reg.types.get(trait_path) else {
        unreachable!("a method is defaulted only from a trait")
    };
    tdef.method(method).expect("a method of the trait").type_params.len() - tdef.params.len()
}

/// The trait `trait_path`'s default body of `method`, and the substitution
/// that puts the method's type parameters, the trait's and then its own, at
/// `at`.
pub(crate) fn default_body(reg: &Registry, trait_path: &Path, method: SymRef, at: &[Type]) -> (Rc<Template>, Subst) {
    let Some(TypeEntry::Trait(tdef)) = reg.types.get(trait_path) else {
        unreachable!("a method is defaulted only from a trait")
    };
    let want = tdef.method(method).expect("a default is of the trait's own method");
    let template = reg.template(&Base::Method(*trait_path, method)).expect("a default body is checked with its trait");
    (template, Subst::positional(&want.type_params, at))
}

/// One method of the `impl`, `want` its signature in the trait: checked to
/// be that signature, and defined. Its code, and the bounds its own
/// `(:where ...)` writes, on the `impl`'s parameters: the `impl` applies
/// only where they hold, since a call through the trait cannot see them.
#[allow(clippy::too_many_arguments)]
fn check_method(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    tdef: &TraitDef,
    of: &Of,
    method: SymRef,
    want: &super::registry::FnDef,
    shown: &str,
    item: Value,
) -> Result<(Option<Value>, Vec<(TypeParam, crate::typeck::registry::TraitBound)>), Error> {
    let elems = heap.list_to_vec_locs(item)?;
    let rest = match elems.split_first() {
        Some(((Value::Symbol(p), _), rest)) if p.is(wk::PUB) => &rest[1..],
        Some((_, rest)) => rest,
        None => unreachable!("an item was read above"),
    };
    let [(header, _), (params, _), (ret, ret_at), body @ ..] = rest else {
        return Err(Error::TypeError("impl: method must be (name (recv params...) ret body...)".into()));
    };
    let (doc, body) = split_doc(heap, body);
    let (own_where, body) = split_where(heap, body)?;
    // One instantiation's method is documented under its code's name.
    let key = of.inst.as_ref().map_or(method, |args| super::generic::method_specialization(&of.owner, method, args));
    d.record(reg, Definition::Method(of.owner, key), doc);
    let (name, own_names) = parse_header(heap, *header)?;
    let (ns, owner) = (d.site.ns, of.owner);
    // The method owns the `impl`'s parameters, then its own, as an inherent
    // `impl`'s does.
    let names: Vec<SymRef> = of.type_names.iter().chain(&own_names).copied().collect();
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Method(owner, method), &names).map_err(|twice| {
        Error::TypeError(format!("method {}::{}: type parameter `{}` is declared twice", owner, method.name(), twice.0.name()))
    })?;
    let reown = reown(&scope, of);
    let target_ty = reown.apply(of.target);
    let assoc: Vec<Type> = of.assoc.iter().map(|t| reown.apply(t)).collect();
    let named: Vec<(SymRef, Type)> = tdef.params[1..].iter().map(|p| p.name).zip(assoc.iter().cloned()).collect();
    let inner = scope.name_types(named);
    let tcx = TypeCx { reg, ns, scope: &inner, self_type: Some(&target_ty), notes: d.notes };
    // A trait method's arity is fixed by its vtable slot: a call through a
    // trait object fills its arguments in from the trait's declaration, a
    // call on the type from this method's, and `deftrait` has no sections.
    if heap.list_to_vec(*params)?.iter().any(|p| matches!(p, Value::Symbol(s) if s.name().starts_with('&'))) {
        return Err(Error::TypeError(format!(
            "{}: method `{}` may not declare &optional/&key/&rest — a trait method's arity is fixed by its vtable slot, and `{}` declares none",
            shown, method, of.trait_path
        )));
    }
    let (instance, ps) = method_params(heap, &tcx, name, *params, &target_ty)?;
    let ret = parse_type(heap, &tcx, *ret, ret_at.as_ref())?;
    let to_impl = to_impl(tdef, want, std::iter::once(target_ty.clone()).chain(assoc).collect(), &scope);
    let want_params: Vec<Type> = want.params.iter().map(|t| to_impl.apply(t)).collect();
    let want_ret = to_impl.apply(&want.ret);
    let want_instance = want.params.first() == Some(&Type::Param(tdef.params[0]));
    let kind = |i: bool| if i { "an instance method" } else { "a static function" };
    if instance != want_instance {
        return Err(Error::TypeError(format!(
            "{}: method `{}` is {}, but `{}` declares it {}",
            shown,
            method,
            kind(instance),
            of.trait_path,
            kind(want_instance)
        )));
    }
    let written: Vec<Type> = ps.iter().map(|(_, t, _)| t.clone()).collect();
    if written != want_params || ret != want_ret {
        let describe = |params: &[Type], ret: &Type| format!("({}) {}", params.iter().map(Type::to_string).collect::<Vec<_>>().join(" "), ret);
        return Err(Error::TypeError(format!(
            "{}: method `{}` is `{}`, but `{}` declares `{}`",
            shown,
            method,
            describe(&written, &ret),
            of.trait_path,
            describe(&want_params, &want_ret)
        )));
    }
    let mut bounds = method_bounds(heap, &tcx, &scope, of)?;
    let own = own_where.map(|c| parse_where(heap, &tcx, c)).transpose()?.unwrap_or_default();
    // The method's first parameters are the `impl`'s, by position; its own
    // have the bounds the trait gives them, and no more.
    let impl_part = &scope.params()[..of.head.len()];
    if let Some((p, _)) = own.iter().find(|(p, _)| !impl_part.contains(p)) {
        return Err(Error::TypeError(format!(
            "{}: method `{}` bounds its own type parameter `{}`; the bounds on it are the ones `{}` declares",
            shown,
            method,
            p.name.name(),
            of.trait_path
        )));
    }
    bounds.extend(own.iter().cloned());
    bounds.extend(trait_bounds(want, &to_impl));
    let to_head = Subst::positional(impl_part, &of.head.iter().map(|p| Type::Param(*p)).collect::<Vec<_>>());
    let own = own
        .into_iter()
        .map(|(p, b)| {
            let i = impl_part.iter().position(|q| *q == p).expect("a bound on the `impl`'s parameter");
            (of.head[i], b.at(&to_head))
        })
        .collect();
    let def = Method { owner, method, name, instance, public: true, home: None, params: LambdaList::plain(ps), ret, bounds, inst: of.inst.map(<[Type]>::to_vec) };
    Ok((define_method(reg, d, heap, requests, &scope, Some(&target_ty), def, body)?, own))
}
