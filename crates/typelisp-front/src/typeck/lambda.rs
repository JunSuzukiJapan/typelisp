//! Function values: `lambda`, `labels`, a named function taken as a value,
//! and a call of a value of a function type.
//!
//! A `lambda` is a function of its own: a `break` or a `return` in its body
//! cannot leave a loop around it. It takes required parameters and a
//! `&rest` one, which its type `(fn (A...) &rest R Ret)` says. `&optional`
//! and `&key` need a named callee, whose definition has the defaults: a
//! function type has no place for them.

use crate::check::analyze::option_of_sexpr;
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::{check_body, check_expr, Cx};
use super::format::sexpr_list;
use super::block::check_named_body;
use super::locals::Locals;
use super::registry::FnDef;
use super::tree::{Node, Typed};
use super::type_syntax::parse_type;

/// What a function value with `&optional` or `&key` parameters lacks.
pub(crate) const OPT_KEY_NEEDS_A_NAME: &str = "{}: &optional/&key need a named callee (a `defun`, or a method in an `impl`) — a \
     function value is described by its `Type::Fn` alone, which has no place to carry a default \
     expression for a call site to fill in. `&rest` does work here.";

/// `(lambda (params) Ret body...)` — `args` without the `lambda`.
pub(crate) fn check_lambda(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(params, _), (ret, _), body @ ..] = args else {
        return Err(Error::TypeError("lambda: (lambda (params) ret body...)".into()));
    };
    if locals.any(|t| mentions_typed_ptr(t)) {
        unimplemented!("typeck: a `lambda` where a typed pointer is in scope, which it must not capture")
    }
    let written = heap.list_to_vec(*params)?;
    if written.iter().any(|p| matches!(p, Value::Symbol(s) if s.is(wk::OPTIONAL) || s.is(wk::KEY))) {
        return Err(Error::TypeError(OPT_KEY_NEEDS_A_NAME.replace("{}", "lambda")));
    }
    let tcx = cx.types();
    let (required, rest) = match written.iter().position(|p| matches!(p, Value::Symbol(s) if s.is(wk::REST))) {
        Some(i) if i + 2 != written.len() => {
            return Err(Error::TypeError("&rest must be followed by exactly one parameter, as the last item in the parameter list".into()));
        }
        Some(i) => (&written[..i], Some(param(heap, &tcx, written[i + 1])?)),
        None => (&written[..], None),
    };
    let required = required.iter().map(|p| param(heap, &tcx, *p)).collect::<Result<Vec<_>, _>>()?;
    let ret = parse_type(heap, &tcx, *ret)?;
    let ty = Type::Fn(required.iter().map(|(_, t)| t.clone()).collect(), rest.as_ref().map(|(_, t)| Box::new(t.clone())), Box::new(ret.clone()));
    let mut params = required;
    params.extend(rest.map(|(n, _)| (n, option_of_sexpr())));
    // A loop or a block around the `lambda` is not one its body can leave.
    let outer = cx.loops.replace(Vec::new());
    let outer_blocks = cx.blocks.replace(Vec::new());
    let checked = locals.bind(params.clone()).and_then(|inner| check_body(cx, heap, &inner, body, Some(&ret)));
    cx.loops.replace(outer);
    cx.blocks.replace(outer_blocks);
    let (body, _) = checked?;
    Ok(Typed { node: Node::Lambda { params, ret, body }, ty, loc })
}

/// A `lambda`'s parameter `(name Type)`.
fn param(heap: &Heap, tcx: &super::type_syntax::TypeCx, p: Value) -> Result<(SymRef, Type), Error> {
    let pair = heap.list_to_vec(p).map_err(|_| Error::TypeError("parameter must be (name type)".into()))?;
    let [Value::Symbol(name), ty] = pair.as_slice() else {
        return Err(Error::TypeError("parameter must be (name type)".into()));
    };
    Ok((*name, parse_type(heap, tcx, *ty)?))
}

/// Whether `ty` is or holds a typed pointer.
fn mentions_typed_ptr(ty: &Type) -> bool {
    let mut found = false;
    super::unify::walk(ty, &mut |t| found |= matches!(t, Type::PtrTo(_)));
    found
}

/// `callee`, a value, called with `args`.
pub(crate) fn check_apply(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    callee: Typed,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let (params, rest, ret) = match cx.with_default(&callee.ty) {
        Type::Fn(p, r, ret) => (p, r, *ret),
        Type::Infer(_) => unimplemented!("typeck: calling a value whose type is not decided yet"),
        other => return Err(Error::TypeError(format!("value is not callable: `{}`", other))),
    };
    let fixed = params.len();
    match &rest {
        None if args.len() != fixed => {
            return Err(Error::TypeError(format!("function expects {} argument(s), got {}", fixed, args.len())));
        }
        Some(_) if args.len() < fixed => {
            return Err(Error::TypeError(format!("function expects at least {} argument(s), got {}", fixed, args.len())));
        }
        _ => {}
    }
    super::widen::expect(cx, &ret, expected)?;
    let mut checked = Vec::with_capacity(fixed + 1);
    for ((arg, arg_loc), param) in args.iter().zip(&params) {
        checked.push(check_expr(cx, heap, locals, *arg, Some(param), arg_loc.clone())?);
    }
    if let Some(elem) = &rest {
        let mut items = Vec::with_capacity(args.len() - fixed);
        for (arg, arg_loc) in &args[fixed..] {
            items.push(check_expr(cx, heap, locals, *arg, Some(elem), arg_loc.clone())?);
        }
        checked.push(sexpr_list(items));
    }
    Ok(Typed { node: Node::Apply { callee: Box::new(callee), args: checked }, ty: ret, loc })
}

/// `(apply f arg... list)`: `f`, a function value with a `&rest`
/// parameter, called with `arg...` for its fixed parameters and `list` as
/// its `&rest` one — `args` without the `apply`.
pub(crate) fn check_apply_form(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(f, f_loc), fixed @ .., (list, list_loc)] = args else {
        return Err(Error::TypeError("apply: (apply function arg... rest-list)".into()));
    };
    let callee = check_expr(cx, heap, locals, *f, None, f_loc.clone())?;
    let (params, ret) = match cx.with_default(&callee.ty) {
        Type::Fn(p, Some(_), ret) => (p, *ret),
        Type::Fn(_, None, _) => return Err(Error::TypeError("apply: function has no `&rest` parameter to apply a list to".into())),
        Type::Infer(_) => unimplemented!("typeck: `apply` of a value whose type is not decided yet"),
        other => return Err(Error::TypeError(format!("apply: value is not callable: `{}`", other))),
    };
    if fixed.len() != params.len() {
        return Err(Error::TypeError(format!(
            "apply: expected {} fixed argument(s) before the rest list, got {}",
            params.len(),
            fixed.len()
        )));
    }
    let mut checked = Vec::with_capacity(fixed.len() + 1);
    for ((arg, arg_loc), param) in fixed.iter().zip(&params) {
        checked.push(check_expr(cx, heap, locals, *arg, Some(param), arg_loc.clone())?);
    }
    checked.push(check_expr(cx, heap, locals, *list, Some(&option_of_sexpr()), list_loc.clone())?);
    Ok(Typed { node: Node::Apply { callee: Box::new(callee), args: checked }, ty: ret, loc })
}

/// The function `path`, defined as `def`, taken as a value.
pub(crate) fn fn_value(path: Path, def: &FnDef, loc: Option<Loc>) -> Result<Typed, Error> {
    if def.ffi {
        return Err(Error::TypeError(format!(
            "`{0}` is a C function declared by `defffi` and cannot be used as a value — it has no body to close over, only a native entry point. Wrap it: `(unsafe (lambda (...) ... ({0} ...)))`.",
            path
        )));
    }
    if !def.type_params.is_empty() {
        unimplemented!("typeck: the generic function `{}` as a value", path)
    }
    if !def.optionals.is_empty() || !def.keys.is_empty() {
        unimplemented!("typeck: the function `{}`, which takes &optional or &key parameters, as a value", path)
    }
    let ty = Type::Fn(def.params.clone(), def.rest.clone().map(Box::new), Box::new(def.ret.clone()));
    // Every parameter the function really takes: a `&rest` one is a list.
    let mut params = def.params.clone();
    params.extend(def.rest.as_ref().map(|_| option_of_sexpr()));
    Ok(Typed { node: Node::FnRef { path, params }, ty, loc })
}

/// One function of a `labels`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Label {
    #[serde(with = "crate::owned_form::sym_serde")]
    pub(crate) name: SymRef,
    #[serde(with = "super::wire::sym_pairs")]
    pub(crate) params: Vec<(SymRef, Type)>,
    pub(crate) ret: Type,
    pub(crate) body: Vec<Typed>,
}

/// `(labels ((name (params) Ret body...)...) body...)` — `args` without the
/// `labels`. Every function's name is bound in every function's body, its
/// own too, and in `body`.
pub(crate) fn check_labels(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let Some(((specs, _), body)) = args.split_first() else {
        return Err(Error::TypeError("labels: (labels ((name (params) ret body...)...) body...)".into()));
    };
    if locals.any(|t| mentions_typed_ptr(t)) {
        unimplemented!("typeck: a `labels` where a typed pointer is in scope, which it must not capture")
    }
    let tcx = cx.types();
    let mut parsed = Vec::new();
    for spec in heap.list_to_vec(*specs)? {
        let parts = heap.list_to_vec_locs(spec)?;
        let [(name, _), (params, _), (ret, _), def_body @ ..] = parts.as_slice() else {
            return Err(Error::TypeError("labels: binding must be (name (params) ret body...)".into()));
        };
        let Value::Symbol(name) = *name else {
            return Err(Error::TypeError("labels: name must be a symbol".into()));
        };
        let name = crate::check::bound_name(name, "labels").map(|_| name)?;
        let written = heap.list_to_vec(*params)?;
        if written.iter().any(|p| matches!(p, Value::Symbol(s) if s.is(wk::OPTIONAL) || s.is(wk::KEY))) {
            return Err(Error::TypeError(OPT_KEY_NEEDS_A_NAME.replace("{}", "labels")));
        }
        let params = written.iter().map(|p| param(heap, &tcx, *p)).collect::<Result<Vec<_>, _>>()?;
        let ret = parse_type(heap, &tcx, *ret)?;
        parsed.push((name, params, ret, def_body.to_vec()));
    }
    let names: Vec<(SymRef, Type)> = parsed
        .iter()
        .map(|(name, params, ret, _)| (*name, Type::Fn(params.iter().map(|(_, t)| t.clone()).collect(), None, Box::new(ret.clone()))))
        .collect();
    let scope = locals.bind(names)?;
    let mut defs = Vec::with_capacity(parsed.len());
    for (name, params, ret, def_body) in parsed {
        // Each function is a function of its own, as a `lambda` is, with a
        // block named after itself.
        let outer = cx.loops.replace(Vec::new());
        let outer_blocks = cx.blocks.replace(Vec::new());
        let checked = scope.bind(params.clone()).and_then(|inner| check_named_body(cx, heap, &inner, name, &def_body, &ret));
        cx.loops.replace(outer);
        cx.blocks.replace(outer_blocks);
        let body = checked?;
        defs.push(Label { name, params, ret, body });
    }
    let (body, ty) = check_body(cx, heap, &scope, body, expected)?;
    Ok(Typed { node: Node::Labels { defs, body }, ty, loc })
}
