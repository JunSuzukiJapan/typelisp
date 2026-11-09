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

use crate::types::{ParamOwner, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::define::Definer;
use super::defmethod::{define_method, receiver_type_path, Method};
use super::defun::parse_header;
use super::generic::Requests;
use super::impl_block::method_params;
use super::lambda_list::LambdaList;
use super::registry::{Registry, TraitDef, TraitImpl};
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
    if matches!(items.first(), Some((first, _)) if matches!(heap.car(*first), Ok(Value::Symbol(k)) if k.is(wk::KW_WHERE))) {
        unimplemented!("typeck: an `impl` of a trait with a `(:where ...)` clause")
    }
    let owner = receiver_type_path(heap, reg, ns, *target).map_err(|_| Error::TypeError("impl: target must be a data type".into()))?;
    // The target written in the type's own parameters, which `impl<...>`
    // declares in order.
    let root = TypeScope::empty();
    let head = root.declare(ParamOwner::Def(owner), type_names).map_err(|twice| {
        Error::TypeError(format!("impl {} {}: type parameter `{}` is declared twice", trait_path, owner, twice.0.name()))
    })?;
    let target_ty = parse_type(heap, &TypeCx { reg, ns, scope: &head, self_type: None }, *target)?;
    let own: Vec<Type> = head.params().iter().map(|p| Type::Param(*p)).collect();
    match &target_ty {
        Type::Named(_, args) if *args == own => {}
        Type::Named(..) => unimplemented!("typeck: an `impl` of a trait for one instantiation, or whose type parameters are not the type's in order"),
        _ if own.is_empty() => {}
        _ => unimplemented!("typeck: an `impl<...>` of a trait for a type without parameters"),
    }
    let shown = format!("impl {} {}", trait_path, owner);
    let (assoc, methods) = items_of(heap, reg, ns, &head, &target_ty, &tdef, &shown, items)?;
    for (written, _) in &methods {
        if tdef.method(*written).is_none() {
            return Err(Error::TypeError(format!("{}: `{}` is not a method of `{}`", shown, written, trait_path)));
        }
    }
    for (m, _) in &tdef.methods {
        if !methods.iter().any(|(w, _)| w == m) {
            if reg.method(&owner, *m).is_some() {
                unimplemented!("typeck: an `impl` that adopts `{}::{}`, written before it, as the trait's method", owner, m)
            }
            if tdef.defaults.contains(m) {
                unimplemented!("typeck: an `impl` that leaves the trait's default method `{}` out", m)
            }
            return Err(Error::TypeError(format!("{}: missing method `{}` (`{}` declares no default body for it)", shown, m, trait_path)));
        }
    }
    for sup in &tdef.supertraits {
        if !implements_every(reg, &sup.trait_path, &owner) {
            return Err(Error::TypeError(format!(
                "{}: `{}` requires `{}`, but `{}` has no `impl {}` — write it before this one",
                shown, trait_path, sup.trait_path, owner, sup.trait_path
            )));
        }
    }
    let mut forms = Vec::new();
    for (method, item) in methods {
        let want = tdef.method(method).expect("checked above").clone();
        let of = Of { trait_path, owner, type_names, target: &target_ty, assoc: &assoc };
        if let Some(form) = check_method(reg, d, heap, requests, &tdef, &of, method, &want, &shown, item)? {
            forms.push(form);
        }
    }
    let mut all = reg.trait_impls.get(&(trait_path, owner)).cloned().unwrap_or_default();
    all.retain(|i| i.args.is_some());
    let assoc = tdef.params[1..].iter().map(|a| a.name.name().to_string()).zip(assoc).collect();
    all.push(TraitImpl { args: None, assoc, bounds: Vec::new() });
    reg.trait_impls.insert((trait_path, owner), all);
    crate::check::forms::module_form(heap, &owner, &forms)
}

/// Whether `ty` implements `trait_path` for every instantiation.
fn implements_every(reg: &Registry, trait_path: &Path, ty: &Path) -> bool {
    reg.trait_impls.get(&(*trait_path, *ty)).is_some_and(|all| all.iter().any(|i| i.args.is_none()))
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
                let Some(i) = assoc_names.iter().position(|a| a == name) else {
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
    if matches!(body, [(Value::Str(_), _), _, ..]) {
        unimplemented!("typeck: a method with a documentation string")
    }
    let (name, _) = parse_header(heap, *header)?;
    let (ns, owner) = (d.site.ns, of.owner);
    // The method owns the `impl`'s parameters, as an inherent `impl`'s does.
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Method(owner, method), of.type_names).map_err(|twice| {
        Error::TypeError(format!("method {}::{}: type parameter `{}` is declared twice", owner, method.name(), twice.0.name()))
    })?;
    let type_params = reg.type_def(&owner).map(|t| t.params.clone()).unwrap_or_default();
    let own: Vec<Type> = scope.params().iter().map(|p| Type::Param(*p)).collect();
    let reown = Subst::positional(&type_params, &own);
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
    let def = Method { owner, method, instance, public: true, home: None, params: LambdaList::plain(ps), ret };
    define_method(reg, d, heap, requests, &scope, Some(&target_ty), def, body)
}
