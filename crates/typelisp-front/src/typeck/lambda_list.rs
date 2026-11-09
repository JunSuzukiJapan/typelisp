//! A `defun`'s parameter list: its required parameters, then `&optional`,
//! `&rest` and `&key` ones, in that order.
//!
//! An `&optional` or `&key` parameter is `(name Type)` or `(name Type
//! default)`. One with a default is bound at `Type` in the body; one with
//! none at `Option<Type>`, `none` when the call leaves it out. The `&rest`
//! parameter is the arguments left after the optional ones, bound as one
//! `Option<Sexpr>` list. `&key` is never written with `&optional` or
//! `&rest`: which of them takes an argument would depend on its value.
//!
//! A default is checked once, here, with no parameter bound, and kept as
//! the core form a call that leaves its parameter out is given
//! ([`super::args`]). It is the same form for every call, so the type of a
//! parameter with a default may not mention the function's type
//! parameters.

use crate::check::analyze::option_of_sexpr;
use crate::check::registry::{opt_key_effective_ty, OptKeyParam};
use crate::owned_form::value_to_owned;
use crate::types::{ParamOwner, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::intern_keyword;
use typelisp_mem::SymRef;

use super::define::Definer;
use super::expr::{check_expr, Cx};
use super::generic::Requests;
use super::locals::Locals;
use super::lower::{lower_rooted, Lowering};
use super::registry::Registry;
use super::scope::TypeScope;
use super::subst::Subst;
use super::type_syntax::{parse_type, TypeCx};

/// A parameter list, read.
pub(crate) struct LambdaList {
    pub(crate) required: Vec<(SymRef, Type)>,
    pub(crate) optionals: Vec<OptKeyParam>,
    /// The `&rest` parameter's name and the type of each argument it takes.
    pub(crate) rest: Option<(SymRef, Type)>,
    pub(crate) keys: Vec<OptKeyParam>,
    /// Every parameter as the body binds it, in the order a call gives
    /// them: required, `&optional`, `&rest`, `&key`.
    pub(crate) bound: Vec<(SymRef, Type)>,
}

/// An `&optional` or `&key` parameter as written: its name, its type, and
/// its default.
type Written = (SymRef, Type, Option<(Value, Option<Loc>)>);

/// The parameter list `v` of the `defun` `path`, its defaults checked where
/// `scope` holds the function's type parameters.
pub(crate) fn parse_lambda_list(
    reg: &Registry,
    d: &Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    scope: &TypeScope,
    path: &Path,
    v: Value,
) -> Result<LambdaList, Error> {
    let [required, optionals, rest, keys] = sections(heap, v)?;
    let tcx = TypeCx { reg, ns: d.site.ns, scope, self_type: None };
    let required = required.iter().map(|p| required_param(heap, &tcx, *p)).collect::<Result<Vec<_>, _>>()?;
    let rest = match rest.as_slice() {
        [] => None,
        [p] => Some(required_param(heap, &tcx, *p)?),
        _ => unreachable!("`sections` takes one `&rest` parameter"),
    };
    let optionals = optionals.iter().map(|p| written(heap, &tcx, *p, "&optional")).collect::<Result<Vec<_>, _>>()?;
    let keys = keys.iter().map(|p| written(heap, &tcx, *p, "&key")).collect::<Result<Vec<_>, _>>()?;
    let owner = ParamOwner::Def(*path);
    let mut checked = |section: &str, params: Vec<Written>| -> Result<Vec<OptKeyParam>, Error> {
        params
            .into_iter()
            .map(|(name, decl_ty, default)| {
                let default = match default {
                    Some(_) if mentions_own_param(&decl_ty, owner) => {
                        return Err(Error::TypeError(format!(
                            "defun {}: {} parameter `{}` may not default when its type mentions the \
                             function's own type parameter — declare it with no default (`Option<{}>`) instead",
                            path.last_segment(),
                            section,
                            name.name(),
                            decl_ty
                        )));
                    }
                    Some((form, loc)) => Some(check_default(reg, d, heap, requests, scope, form, loc, &decl_ty)?),
                    None => None,
                };
                Ok(OptKeyParam { name: intern_keyword(name.name()), decl_ty, default })
            })
            .collect()
    };
    let optional_names: Vec<SymRef> = optionals.iter().map(|(n, _, _)| *n).collect();
    let key_names: Vec<SymRef> = keys.iter().map(|(n, _, _)| *n).collect();
    let optionals = checked("&optional", optionals)?;
    let keys = checked("&key", keys)?;
    let mut bound = required.clone();
    bound.extend(optional_names.iter().zip(&optionals).map(|(n, o)| (*n, effective(o))));
    bound.extend(rest.iter().map(|(n, _)| (*n, option_of_sexpr())));
    bound.extend(key_names.iter().zip(&keys).map(|(n, k)| (*n, effective(k))));
    Ok(LambdaList { required, optionals, rest, keys, bound })
}

/// The type `p` is bound at in the body.
fn effective(p: &OptKeyParam) -> Type {
    opt_key_effective_ty(&p.decl_ty, p.default.is_some())
}

/// Whether `ty` mentions a type parameter `owner` declares.
fn mentions_own_param(ty: &Type, owner: ParamOwner) -> bool {
    let mut found = false;
    super::unify::walk(ty, &mut |t| found |= matches!(t, Type::Param(p) if p.owner == owner));
    found
}

/// The parameter list `v` split at its markers: the required, `&optional`,
/// `&rest` and `&key` parameters.
fn sections(heap: &Heap, v: Value) -> Result<[Vec<Value>; 4], Error> {
    let mut out: [Vec<Value>; 4] = Default::default();
    let mut rank = 0;
    for p in heap.list_to_vec(v)? {
        if let Value::Symbol(id) = p {
            let (at, message) = match id.well_known() {
                wk::OPTIONAL => (1, "defun: &optional must precede &rest and &key, and appear once"),
                wk::REST => (2, "defun: &rest must precede &key, and appear once"),
                wk::KEY => (3, "defun: &key may appear only once"),
                _ if id.name().starts_with('&') => unimplemented!("typeck: the `{}` parameter marker", id.name()),
                _ => (0, ""),
            };
            if at > 0 {
                if rank >= at {
                    return Err(Error::TypeError(message.into()));
                }
                rank = at;
                continue;
            }
        }
        out[rank].push(p);
    }
    let [_, optionals, rest, keys] = &out;
    if rank == 2 && rest.is_empty() {
        return Err(Error::TypeError("defun: &rest must be followed by exactly one parameter".into()));
    }
    if rest.len() > 1 {
        return Err(Error::TypeError("defun: &rest takes exactly one parameter, as the last item in the parameter list".into()));
    }
    if !keys.is_empty() && (!optionals.is_empty() || !rest.is_empty()) {
        return Err(Error::TypeError("defun: &key cannot be combined with &optional/&rest in the same parameter list".into()));
    }
    Ok(out)
}

/// A required or `&rest` parameter, `(name Type)`.
fn required_param(heap: &Heap, tcx: &TypeCx, p: Value) -> Result<(SymRef, Type), Error> {
    let pair = heap.list_to_vec(p).map_err(|_| Error::TypeError(super::defun::SHAPE.into()))?;
    let [Value::Symbol(name), ty] = pair.as_slice() else {
        return Err(Error::TypeError(format!("defun: a parameter is (name Type): {}", super::defun::SHAPE)));
    };
    Ok((*name, parse_type(heap, tcx, *ty)?))
}

/// An `&optional` or `&key` parameter, `(name Type)` or `(name Type
/// default)`, of the section `section`.
fn written(heap: &Heap, tcx: &TypeCx, p: Value, section: &str) -> Result<Written, Error> {
    let items = heap.list_to_vec_locs(p)?;
    if items.len() < 2 || items.len() > 3 {
        return Err(Error::TypeError(format!("defun: {} parameter must be `(name type)` or `(name type default)`", section)));
    }
    let Value::Symbol(name) = items[0].0 else {
        return Err(Error::TypeError(format!("defun: {} parameter name must be a symbol", section)));
    };
    let ty = parse_type(heap, tcx, items[1].0)?;
    Ok((name, ty, items.get(2).cloned()))
}

/// The default `form`, checked where `decl_ty` is expected with no
/// parameter bound, as the core form a call is given.
#[allow(clippy::too_many_arguments)]
fn check_default(
    reg: &Registry,
    d: &Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    scope: &TypeScope,
    form: Value,
    loc: Option<Loc>,
    decl_ty: &Type,
) -> Result<crate::owned_form::OwnedForm, Error> {
    let mut tree = {
        let cx = Cx::new(reg, d.expander, d.site.ns, scope, None);
        let t = check_expr(&cx, heap, &Locals::empty(), form, Some(decl_ty), loc)?;
        let mut tree = vec![t];
        cx.settle(heap, &mut tree)?;
        tree
    };
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let lowered = lower_rooted(heap, &mut lw, &tree[0])?;
    tree.clear();
    value_to_owned(heap, lowered)
}
