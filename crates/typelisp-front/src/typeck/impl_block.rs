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

use crate::types::{ParamOwner, Path};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::defmethod::{define_method, receiver_type_path, Method};
use super::defun::{parse_header, parse_params};
use super::generic::Requests;
use super::define::Definer;
use super::registry::Registry;
use super::scope::TypeScope;
use super::type_syntax::{parse_type, TypeCx};

const ITEM_SHAPE: &str = "impl: an item is (pub? defmethod name<T...>? (params...) ReturnType body...)";

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
    let Some((target, _)) = parts.first() else {
        return Err(Error::TypeError("impl: (impl<T...>? Type (defmethod ...)...)".into()));
    };
    // `(impl Trait Type ...)` names a type second; an inherent `impl` has an
    // item, a `(:where ...)` or nothing there.
    if let Some((second, _)) = parts.get(1) {
        if !matches!(second, Value::Cons(_)) || crate::types::is_type_application(heap, *second) {
            unimplemented!("typeck: a trait's `impl`")
        }
        if matches!(heap.car(*second), Ok(Value::Symbol(k)) if k.is_keyword()) {
            unimplemented!("typeck: an `impl` with a `(:where ...)` clause")
        }
    }
    let owner = receiver_type_path(heap, reg, ns, *target)?;
    if reg.type_def(&owner).is_some_and(|d| d.stdlib) {
        return Err(Error::TypeError(format!(
            "impl: `{}` is a standard library type; methods can be added to a type only where it is defined. \
             Define a trait and `(impl Trait {} ...)` to give it new behavior",
            owner, owner
        )));
    }
    let mut forms = Vec::new();
    for (item, _) in &parts[1..] {
        if let Some(form) = check_item(reg, d, heap, requests, owner, type_names, *target, *item)? {
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
    target: Value,
    item: Value,
) -> Result<Option<Value>, Error> {
    let elems = heap.list_to_vec_locs(item).map_err(|_| Error::TypeError(ITEM_SHAPE.into()))?;
    let (public, elems) = match elems.split_first() {
        Some(((Value::Symbol(p), _), rest)) if p.is(wk::PUB) => (true, rest),
        _ => (false, &elems[..]),
    };
    let [(Value::Symbol(head), _), (header, _), (params, _), (ret, _), body @ ..] = elems else {
        return Err(Error::TypeError(ITEM_SHAPE.into()));
    };
    if !head.is(wk::DEFMETHOD) {
        return Err(Error::TypeError(
            "impl: a method is written (defmethod name (params...) ReturnType body...), or (pub defmethod ...) to make it public".into(),
        ));
    }
    let (name, own_names) = parse_header(heap, *header)?;
    let ns = d.site.ns;
    if matches!(body, [(Value::Str(_), _), _, ..]) {
        unimplemented!("typeck: a method with a documentation string")
    }
    let method = member_symbol(name.name());
    let names: Vec<SymRef> = impl_names.iter().chain(&own_names).copied().collect();
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Method(owner, method), &names).map_err(|twice| {
        Error::TypeError(format!("method {}::{}: type parameter `{}` is declared twice", owner, method.name(), twice.0.name()))
    })?;
    let target_ty = parse_type(heap, &TypeCx { reg, ns, scope: &scope, self_type: None }, target)?;
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: Some(&target_ty) };
    // `self` alone is the receiver; the rest are ordinary parameters.
    let written = heap.list_to_vec(*params)?;
    let receiver = match written.first() {
        Some(Value::Symbol(s)) if s.is(wk::SELF) => Some(*s),
        _ => None,
    };
    for p in &written {
        if matches!(heap.car(*p), Ok(Value::Symbol(s)) if s.is(wk::SELF)) {
            return Err(Error::TypeError(format!(
                "method `{}`: the receiver is written `self` alone, as the first parameter — `(self Type)` is not accepted",
                name.name()
            )));
        }
    }
    let rest = if receiver.is_some() { heap.cdr(*params)? } else { *params };
    let mut all = Vec::new();
    if let Some(s) = receiver {
        all.push((s, target_ty.clone()));
    }
    all.extend(parse_params(heap, &tcx, rest)?);
    let ret = parse_type(heap, &tcx, *ret)?;
    let def = Method { owner, method, instance: receiver.is_some(), public, home: None, params: all, ret };
    define_method(reg, d, heap, requests, &scope, Some(&target_ty), def, body)
}
