//! An inherent `impl`: methods of one type.
//!
//! `(impl<T, U> Pair<T, U> (defmethod name<V>? (self param...) Ret body...)...)`.
//! The `<T, U>` after `impl` declares the type parameters the target and the
//! items use; a name written in the target declares nothing
//! (`docs/dev/checker-rebuild-plan.md` 7.3). Each method's type parameters
//! are the `impl`'s followed by its own, owned by the method. A method whose
//! first parameter is `self` is an instance method, `self` having the
//! target's type; any other is a static method. `Self` in a type is the
//! target.

use crate::types::{ParamOwner, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::defmethod::{define_method, receiver_type_path, Method};
use super::bounds::parse_where;
use super::defun::{parse_header, parse_param, split_doc, split_where};
use super::lambda_list::{name_loc, parse_lambda_list, Written};
use super::locals::Bound;
use super::generic::Requests;
use super::define::Definer;
use super::registry::{Definition, Registry};
use super::scope::TypeScope;
use super::type_syntax::{parse_type, TypeCx};

const ITEM_SHAPE: &str = "impl: an item is (pub? defmethod name (params...) ReturnType body...), the name `name<T...>` when the method has type parameters of its own";

/// `(impl ...)` — `parts` without the head — and its core form, unrooted.
/// `type_names` are what `impl<...>` declares. The caller undoes the
/// definitions if checking fails.
pub(crate) fn check_impl(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    type_names: &[SymRef],
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let ns = d.site.ns;
    let Some((target, target_at)) = parts.first() else {
        return Err(Error::TypeError("impl: (impl<T...>? Type (defmethod ...)...)".into()));
    };
    // `(impl Trait Type ...)` names a type second; an inherent `impl` has an
    // item, a `(:where ...)` or nothing there.
    if let Some((second, _)) = parts.get(1) {
        if !matches!(second, Value::Cons(_)) || crate::types::is_type_application(heap, *second) {
            return super::trait_impl::check_trait_impl(reg, d, heap, requests, type_names, parts);
        }
    }
    // The bounds an `impl`'s `(:where ...)` writes are on every method's
    // type parameters.
    let (clause, items) = match parts.get(1) {
        Some((second, _)) if crate::check::analyze::is_where_clause(heap, *second)? => (Some(*second), &parts[2..]),
        _ => (None, &parts[1..]),
    };
    let owner = receiver_type_path(heap, reg, ns, *target)?;
    // The standard library may add methods to its own types in any of its
    // modules.
    if reg.type_def(&owner).is_some_and(|d| d.stdlib) && !crate::types::in_std(ns) {
        return Err(Error::TypeError(format!(
            "impl: `{}` is a standard library type; methods can be added to a type only where it is defined. \
             Define a trait and `(impl Trait {} ...)` to give it new behavior",
            owner, owner
        )));
    }
    let mut forms = Vec::new();
    for (item, _) in items {
        if let Some(form) = check_item(reg, d, heap, requests, owner, type_names, clause, *target, target_at.as_ref(), *item)? {
            forms.push(form);
        }
    }
    crate::check::forms::module_form(heap, &owner, &forms)
}

/// One `(pub? defmethod ...)` item of an `impl` of `owner`, its target
/// written `target`: its code, rooted, or `None` when it is generic.
#[allow(clippy::too_many_arguments)]
fn check_item(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    owner: Path,
    impl_names: &[SymRef],
    impl_where: Option<Value>,
    target: Value,
    target_at: Option<&Loc>,
    item: Value,
) -> Result<Option<Value>, Error> {
    let elems = heap.list_to_vec_locs(item).map_err(|_| Error::TypeError(ITEM_SHAPE.into()))?;
    let (public, elems) = match elems.split_first() {
        Some(((Value::Symbol(p), _), rest)) if p.is(wk::PUB) => (true, rest),
        _ => (false, &elems[..]),
    };
    let [(Value::Symbol(head), _), (header, _), (params, _), (ret, ret_at), body @ ..] = elems else {
        return Err(Error::TypeError(ITEM_SHAPE.into()));
    };
    if !head.is(wk::DEFMETHOD) {
        return Err(Error::TypeError(
            "impl: a method is written (defmethod name (params...) ReturnType body...), or (pub defmethod ...) to make it public".into(),
        ));
    }
    let (name, own_names) = parse_header(heap, *header)?;
    let ns = d.site.ns;
    let (own_where, body) = split_where(heap, body)?;
    let (doc, body) = split_doc(heap, body);
    let method = member_symbol(name.name());
    let names: Vec<SymRef> = impl_names.iter().chain(&own_names).copied().collect();
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Method(owner, method), &names).map_err(|twice| {
        Error::TypeError(format!("method {}::{}: type parameter `{}` is declared twice", owner, method.name(), twice.0.name()))
    })?;
    let target_ty = parse_type(heap, &TypeCx { reg, ns, scope: &scope, self_type: None, notes: d.notes }, target, target_at)?;
    if !all_or_none(&target_ty, &scope.params()[..impl_names.len()]) {
        return Err(not_all_or_none(&format!("method {}::{}", owner, method), &owner, &target_ty));
    }
    let (receiver, written) = split_receiver(heap, name, *params)?;
    let what = format!("impl: method `{}`", method);
    let named = format!("method {}::{}", owner, method);
    let at = Written { what: &what, named: &named, owner: ParamOwner::Method(owner, method) };
    let list = parse_lambda_list(reg, d, heap, requests, &scope, Some(&target_ty), &at, &written)?;
    let instance = receiver.is_some();
    let list = match receiver {
        Some((s, at)) => list.after((s, target_ty.clone()), at),
        None => list,
    };
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: Some(&target_ty), notes: d.notes };
    let ret = parse_type(heap, &tcx, *ret, ret_at.as_ref())?;
    let mut bounds = Vec::new();
    for clause in impl_where.into_iter().chain(own_where) {
        bounds.extend(parse_where(heap, &tcx, clause)?);
    }
    // A target that names every type argument outright makes the method
    // that instantiation's own.
    let inst = match &target_ty {
        Type::Named(_, args) if impl_names.is_empty() && !args.is_empty() => Some(args.clone()),
        _ => None,
    };
    // One instantiation's method is documented under its code's name.
    let key = inst.as_ref().map_or(method, |args| super::generic::method_specialization(&owner, method, args));
    d.record(reg, Definition::Method(owner, key), doc);
    let def = Method { owner, method, name, instance, public, home: None, params: list, ret, bounds, inst };
    define_method(reg, d, heap, requests, &scope, Some(&target_ty), def, body)
}

/// Whether the target `target` of an `impl` declaring `declared` writes
/// every type argument outright, or each as a distinct declared parameter.
/// Specialization binds the parameters from the type's arguments, so a
/// target with some of each would leave a parameter nothing binds.
pub(crate) fn all_or_none(target: &Type, declared: &[crate::types::TypeParam]) -> bool {
    let Type::Named(_, args) = target else {
        return true;
    };
    if declared.is_empty() {
        return true;
    }
    let mut seen = Vec::new();
    for a in args {
        match a {
            Type::Param(p) if declared.contains(p) && !seen.contains(p) => seen.push(*p),
            _ => return false,
        }
    }
    true
}

/// The error for a target [`all_or_none`] refuses, `who` naming the item.
pub(crate) fn not_all_or_none(who: &str, owner: &Path, target: &Type) -> Error {
    Error::TypeError(format!(
        "{}: an `impl` of `{}` names every type parameter of `{}` as a distinct variable, or none of them",
        who, target, owner
    ))
}

/// The parameters `params` of the method `name` of an `impl` or a
/// `deftrait`, read in `tcx`: whether it is an instance method, and each
/// parameter's name, type and where its name was written. `self` alone,
/// first, is the receiver, of type `self_ty`; the rest are ordinary
/// parameters.
pub(crate) fn method_params(heap: &Heap, tcx: &TypeCx, name: SymRef, params: Value, self_ty: &Type) -> Result<(bool, Vec<Bound>), Error> {
    let (receiver, written) = split_receiver(heap, name, params)?;
    let mut all = Vec::new();
    if let Some((s, at)) = &receiver {
        all.push((*s, self_ty.clone(), at.clone()));
    }
    for p in written {
        let (n, t) = parse_param(heap, tcx, p)?;
        all.push((n, t, name_loc(heap, p)));
    }
    Ok((receiver.is_some(), all))
}

/// The parameters `params` of the method `name`: the receiver `self`, when
/// it is written first, with where, and the others.
pub(crate) fn split_receiver(heap: &Heap, name: SymRef, params: Value) -> Result<(Option<(SymRef, Option<Loc>)>, Vec<Value>), Error> {
    let at = heap.list_to_vec_locs(params)?.into_iter().next().and_then(|(_, l)| l);
    let mut written = heap.list_to_vec(params)?;
    for p in &written {
        if matches!(heap.car(*p), Ok(Value::Symbol(s)) if s.is(wk::SELF)) {
            return Err(Error::TypeError(format!(
                "method `{}`: the receiver is written `self` alone, as the first parameter — `(self Type)` is not accepted",
                name.name()
            )));
        }
    }
    match written.first() {
        Some(Value::Symbol(s)) if s.is(wk::SELF) => {
            let s = *s;
            written.remove(0);
            Ok((Some((s, at)), written))
        }
        _ => Ok((None, written)),
    }
}
