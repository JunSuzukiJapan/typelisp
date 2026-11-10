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
use super::registry::{Definition, FnDef, MethodDef, Registry, Template, TraitDef, TraitImpl, TypeEntry};
use super::defun::{lower_template, split_doc};
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
    let [(trait_v, _), (target, _), items @ ..] = parts else {
        return Err(Error::TypeError("impl: (impl TraitName TargetType (:type AssocName Type)... (method ...)...)".into()));
    };
    let (trait_path, tdef) = match *trait_v {
        Value::Symbol(sym) => match trait_named(reg, ns, sym) {
            Some((p, def)) => (p, def.clone()),
            None => return Err(Error::TypeError(format!("impl: unknown trait `{}`", heap.symbol_spelling(sym)))),
        },
        Value::Path(_) => unimplemented!("typeck: an `impl` of a trait named by a path"),
        _ => return Err(Error::TypeError("impl: trait name must be a name or `::` path".into())),
    };
    // `(:where ...)` bounds the type's parameters: the type implements the
    // trait when its arguments meet them, and every method may use them.
    let (clause, items) = match items.split_first() {
        Some(((first, _), rest)) if crate::check::analyze::is_where_clause(heap, *first)? => (Some(*first), rest),
        _ => (None, items),
    };
    let owner = receiver_type_path(heap, reg, ns, *target).map_err(|_| Error::TypeError("impl: target must be a data type".into()))?;
    // The target written in the type's own parameters, which `impl<...>`
    // declares in order.
    let root = TypeScope::empty();
    let head = root.declare(ParamOwner::Def(owner), type_names).map_err(|twice| {
        Error::TypeError(format!("impl {} {}: type parameter `{}` is declared twice", trait_path, owner, twice.0.name()))
    })?;
    let target_ty = parse_type(heap, &TypeCx { reg, ns, scope: &head, self_type: None }, *target)?;
    let own: Vec<Type> = head.params().iter().map(|p| Type::Param(*p)).collect();
    let inst = match &target_ty {
        Type::Named(_, args) if *args == own => None,
        // Every type argument written outright: one instantiation's.
        Type::Named(_, args) if own.is_empty() => Some(args.clone()),
        Type::Named(..) => unimplemented!("typeck: an `impl` of a trait whose type parameters are not the type's in order"),
        _ if own.is_empty() => None,
        _ => unimplemented!("typeck: an `impl<...>` of a trait for a type without parameters"),
    };
    let shown = format!("impl {} {}", trait_path, owner);
    let impl_bounds = match clause {
        Some(c) => parse_where(heap, &TypeCx { reg, ns, scope: &head, self_type: None }, c)?,
        None => Vec::new(),
    };
    let (assoc, methods) = items_of(heap, reg, ns, &head, &target_ty, &tdef, &shown, items)?;
    for (written, _) in &methods {
        if tdef.method(*written).is_none() {
            return Err(Error::TypeError(format!("{}: `{}` is not a method of `{}`", shown, written, trait_path)));
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
        if !implements(reg, &sup.trait_path, &owner, inst.as_deref()) {
            return Err(Error::TypeError(format!(
                "{}: `{}` requires `{}`, but `{}` has no `impl {}` — write it before this one",
                shown, trait_path, sup.trait_path, owner, sup.trait_path
            )));
        }
        // What the supertrait entry pins, in this trait's parameters, is
        // what the supertrait's `impl` binds. A type with a parameter left in
        // it is the current checker's to leave unchecked, and is left so.
        let to_impl = Subst::positional(&tdef.params, &std::iter::once(target_ty.clone()).chain(assoc.iter().cloned()).collect::<Vec<_>>());
        let sup_impl = reg.trait_impls.get(&(sup.trait_path, owner)).and_then(|all| impl_at(all, inst.as_deref())).expect("checked above");
        let open = |t: &Type| {
            let mut found = false;
            super::unify::walk(t, &mut |u| found |= matches!(u, Type::Param(_)));
            found
        };
        for (name, pinned) in &sup.assoc {
            let want = to_impl.apply(pinned);
            match sup_impl.assoc.get(name) {
                Some(actual) if *actual == want || open(&want) || open(actual) => {}
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
    let of = Of { trait_path, owner, type_names, target: &target_ty, assoc: &assoc, clause, inst: inst.as_deref() };
    // The methods left out are defined first, so a written one can call
    // them, and lowered last, once every method they may call is defined.
    let mut defaulted = Vec::new();
    for method in left {
        defaulted.push((method, define_defaulted(reg, d, heap, &tdef, &of, method)?));
    }
    let mut forms = Vec::new();
    for (method, item) in methods {
        let want = tdef.method(method).expect("checked above").clone();
        if let Some(form) = check_method(reg, d, heap, requests, &tdef, &of, method, &want, &shown, item)? {
            forms.push(form);
        }
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
        let elems = heap.list_to_vec(*item).map_err(|_| Error::TypeError("impl: item must start with a symbol".into()))?;
        match elems.as_slice() {
            [Value::Symbol(k), rest @ ..] if k.is(wk::KW_TYPE) => {
                let [name, ty] = rest else {
                    return Err(Error::TypeError("impl: (:type AssocName Type)".into()));
                };
                let Value::Symbol(name) = name else {
                    return Err(Error::TypeError("impl: associated type name must be a symbol".into()));
                };
                let Some(i) = assoc_names.iter().position(|a| a.name() == name.name()) else {
                    return Err(Error::TypeError(format!("{}: `{}` is not an associated type of the trait", shown, name.name())));
                };
                bound[i] = Some(parse_type(heap, &TypeCx { reg, ns, scope: head, self_type: Some(target_ty) }, *ty)?);
            }
            [Value::Symbol(d), rest @ ..] | [Value::Symbol(_), Value::Symbol(d), rest @ ..] if d.is(wk::DEFMETHOD) => {
                let Some(header) = rest.first() else {
                    return Err(Error::TypeError("impl: method must be (name (recv params...) ret body...)".into()));
                };
                let (name, own) = parse_header(heap, *header)?;
                if !own.is_empty() {
                    return Err(Error::TypeError(format!(
                        "{}: `{}<{}>` declares type parameters of its own, and a trait's method has the signature the trait gives it",
                        shown,
                        name.name(),
                        own.iter().map(|o| o.name()).collect::<Vec<_>>().join(",")
                    )));
                }
                let m = member_symbol(name.name());
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
    target: &'a Type,
    assoc: &'a [Type],
    /// The `impl`'s `(:where ...)`, which bounds each method's parameters.
    clause: Option<Value>,
    /// The instantiation the `impl` is for, when it is for one.
    inst: Option<&'a [Type]>,
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
    if sig.type_params.len() != own.len() {
        unimplemented!("typeck: adopting `{}`, a method with type parameters of its own, as a trait's", method)
    }
    // The method's types are written in its own parameters, the type's.
    let reown = Subst::positional(&sig.type_params, &own.iter().map(|p| Type::Param(*p)).collect::<Vec<_>>());
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
    Ok(())
}

/// Define the method `method`, which the `impl` leaves out, as the trait's
/// default body: the trait's parameters where the `impl` puts them, written
/// in the method's type parameters. Those are returned when the method has
/// none, to lower its body at.
fn define_defaulted(reg: &mut Registry, d: &mut Definer, heap: &Heap, tdef: &TraitDef, of: &Of, method: SymRef) -> Result<Option<Vec<Type>>, Error> {
    let (ns, owner) = (d.site.ns, of.owner);
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Method(owner, method), of.type_names).map_err(|twice| {
        Error::TypeError(format!("method {}::{}: type parameter `{}` is declared twice", owner, method.name(), twice.0.name()))
    })?;
    let reown = reown(reg, &owner, &scope, of);
    let at: Vec<Type> = std::iter::once(reown.apply(of.target)).chain(of.assoc.iter().map(|t| reown.apply(t))).collect();
    let to_impl = Subst::positional(&tdef.params, &at);
    let want = tdef.method(method).expect("a default is of the trait's own method");
    let instance = want.params.first() == Some(&Type::Param(tdef.params[0]));
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: Some(&at[0]) };
    let bounds = of.clause.map(|c| parse_where(heap, &tcx, c)).transpose()?.unwrap_or_default();
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

/// What puts the `impl`'s types, written in `owner`'s parameters, in a
/// method's, which `scope` declares: nothing, for an `impl` of one
/// instantiation, whose types have none.
fn reown(reg: &Registry, owner: &Path, scope: &TypeScope, of: &Of) -> Subst {
    if of.inst.is_some() {
        return Subst::default();
    }
    let type_params = reg.type_def(owner).map(|t| t.params.clone()).unwrap_or_default();
    let own: Vec<Type> = scope.params().iter().map(|p| Type::Param(*p)).collect();
    Subst::positional(&type_params, &own)
}

/// The trait `trait_path`'s default body of `method`, and the substitution
/// that puts the trait's parameters at `at`.
pub(crate) fn default_body(reg: &Registry, trait_path: &Path, method: SymRef, at: &[Type]) -> (Rc<Template>, Subst) {
    let Some(TypeEntry::Trait(tdef)) = reg.types.get(trait_path) else {
        unreachable!("a method is defaulted only from a trait")
    };
    // The prelude's traits were checked by the current checker, which keeps
    // their source, not a checked tree.
    let Some(template) = reg.template(&Base::Method(*trait_path, method)) else {
        unimplemented!("typeck: the default body of `{}::{}`, which this checker has no checked body of", trait_path, method)
    };
    (template, Subst::positional(&tdef.params, at))
}

/// One method of the `impl`, `want` its signature in the trait: checked to
/// be that signature, and defined.
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
) -> Result<Option<Value>, Error> {
    let elems = heap.list_to_vec_locs(item)?;
    let rest = match elems.split_first() {
        Some(((Value::Symbol(p), _), rest)) if p.is(wk::PUB) => &rest[1..],
        Some((_, rest)) => rest,
        None => unreachable!("an item was read above"),
    };
    let [(header, _), (params, _), (ret, _), body @ ..] = rest else {
        return Err(Error::TypeError("impl: method must be (name (recv params...) ret body...)".into()));
    };
    let (doc, body) = split_doc(heap, body);
    // One instantiation's method is documented under its code's name.
    let key = of.inst.as_ref().map_or(method, |args| super::generic::method_specialization(&of.owner, method, args));
    d.record(reg, Definition::Method(of.owner, key), doc);
    if let Some((first, _)) = body.first() {
        if crate::check::analyze::is_where_clause(heap, *first)? {
            unimplemented!("typeck: a method of a trait's `impl` with a `(:where ...)` clause of its own")
        }
    }
    let (name, _) = parse_header(heap, *header)?;
    let (ns, owner) = (d.site.ns, of.owner);
    // The method owns the `impl`'s parameters, as an inherent `impl`'s does.
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Method(owner, method), of.type_names).map_err(|twice| {
        Error::TypeError(format!("method {}::{}: type parameter `{}` is declared twice", owner, method.name(), twice.0.name()))
    })?;
    let reown = reown(reg, &owner, &scope, of);
    let target_ty = reown.apply(of.target);
    let assoc: Vec<Type> = of.assoc.iter().map(|t| reown.apply(t)).collect();
    let named: Vec<(SymRef, Type)> = tdef.params[1..].iter().map(|p| p.name).zip(assoc.iter().cloned()).collect();
    let inner = scope.name_types(named);
    let tcx = TypeCx { reg, ns, scope: &inner, self_type: Some(&target_ty) };
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
    let ret = parse_type(heap, &tcx, *ret)?;
    let to_impl = Subst::positional(&tdef.params, &std::iter::once(target_ty.clone()).chain(assoc).collect::<Vec<_>>());
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
    let written: Vec<Type> = ps.iter().map(|(_, t)| t.clone()).collect();
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
    let bounds = of.clause.map(|c| parse_where(heap, &tcx, c)).transpose()?.unwrap_or_default();
    let def = Method { owner, method, name, instance, public: true, home: None, params: LambdaList::plain(ps), ret, bounds, inst: of.inst.map(<[Type]>::to_vec) };
    define_method(reg, d, heap, requests, &scope, Some(&target_ty), def, body)
}
