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

use super::expr::{agree, check_body, check_expr, Cx};
use super::format::sexpr_list;
use super::block::check_named_body;
use super::locals::{LabelsCallback, Locals};
use super::registry::{FnDef, MethodDef};
use super::tree::{Node, Typed};
use super::type_syntax::parse_type;

/// What a function value with `&optional` or `&key` parameters lacks.
pub(crate) const OPT_KEY_NEEDS_A_NAME: &str = "{}: &optional/&key need a named callee (a `defun`, or a method in an `impl`) — a \
     function value is described by its `Type::Fn` alone, which has no place to carry a default \
     expression for a call site to fill in. `&rest` does work here.";

/// `(lambda (params) Ret body...)` — `args` without the `lambda`.
pub(crate) fn check_lambda(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    lambda(cx, heap, locals, args, loc, false)
}

/// [`check_lambda`] of a `lambda` handed to C, which can give it nothing
/// but its arguments: its body may name no local around it.
pub(crate) fn check_callback_lambda(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    lambda(cx, heap, locals, args, loc, true)
}

fn lambda(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>, callback: bool) -> Result<Typed, Error> {
    let [(params, _), (ret, _), body @ ..] = args else {
        return Err(Error::TypeError("lambda: (lambda (params) ret body...)".into()));
    };
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
    let checked = cx.own_arena(|| locals.bind_function(params.clone(), "a `lambda`".into(), callback).and_then(|inner| check_body(cx, heap, &inner, body, Some(&ret))));
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

/// The method `method` of `owner`, defined as `def`, taken as a value: what
/// a bare name is where a function is expected, `expected`, whose first
/// parameter, the receiver, has the method. Its type arguments are what the
/// expected function's type says.
pub(crate) fn method_value(cx: &Cx, owner: Path, method: SymRef, def: &MethodDef, expected: &Type, loc: Option<Loc>) -> Result<Typed, Error> {
    let sig = &def.sig;
    if !sig.optionals.is_empty() || !sig.keys.is_empty() {
        unimplemented!("typeck: the method `{}::{}`, which takes &optional or &key parameters, as a value", owner, method.name())
    }
    let subst = cx.holes.borrow_mut().instantiate(&sig.type_params);
    let params: Vec<Type> = sig.params.iter().map(|p| subst.apply(p)).collect();
    let rest = sig.rest.as_ref().map(|r| subst.apply(r));
    let ret = subst.apply(&sig.ret);
    let ty = Type::Fn(params.clone(), rest.clone().map(Box::new), Box::new(ret.clone()));
    agree(cx, &ty, Some(expected))?;
    let what = format!("{}::{}", owner, method.name());
    for (p, bounds) in &sig.bounds {
        for b in bounds {
            cx.require_bound(subst.apply(&Type::Param(*p)), b.at(&subst), what.clone(), p.name.name().to_string(), loc.clone())?;
        }
    }
    // A Rust builtin has one entry point for every instantiation.
    let targs: Vec<Type> = match sig.builtin {
        true => Vec::new(),
        false => sig.type_params.iter().map(|p| subst.apply(&Type::Param(*p))).collect(),
    };
    for (p, t) in sig.type_params.iter().zip(&targs) {
        cx.must_decide(t.clone(), format!("generic method `{}` used as a value needs its type parameter `{}` decided", what, p.name.name()), loc.clone());
    }
    // A method written for one instantiation has code under its name.
    let code = match &def.inst_args {
        Some(inst) => super::generic::method_specialization(&owner, method, inst),
        None => method,
    };
    let mut taken = params;
    taken.extend(rest.map(|_| option_of_sexpr()));
    Ok(Typed { node: Node::MethodRef { owner, method: code, targs, params: taken, ret }, ty, loc })
}

/// A `labels` function passed to C, and the top-level function `path` C is
/// handed: a copy of the block, `kept` being the functions it reaches,
/// that calls it.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct LiftedLabel {
    #[serde(with = "crate::owned_form::sym_serde")]
    pub(crate) name: SymRef,
    pub(crate) path: Path,
    #[serde(with = "super::wire::syms")]
    pub(crate) kept: Vec<SymRef>,
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
    let scope = locals.bind_labels(names)?;
    let mut defs = Vec::with_capacity(parsed.len());
    let mut captured = Vec::with_capacity(parsed.len());
    for (name, params, ret, def_body) in parsed {
        // Each function is a function of its own, as a `lambda` is, with a
        // block named after itself.
        let inner = scope.bind_function(params.clone(), format!("the `labels` function `{}`", name.name()), false)?;
        let outer = cx.loops.replace(Vec::new());
        let outer_blocks = cx.blocks.replace(Vec::new());
        let checked = cx.own_arena(|| check_named_body(cx, heap, &inner, name, &def_body, &ret));
        cx.loops.replace(outer);
        cx.blocks.replace(outer_blocks);
        let body = checked?;
        captured.push(inner.captured());
        defs.push(Label { name, params, ret, body });
    }
    let (body, ty) = check_body(cx, heap, &scope, body, expected)?;
    let lifted = scope
        .take_labels_callbacks()
        .into_iter()
        .map(|c| {
            let loc = c.loc.clone();
            lift_label(&defs, &captured, c).map_err(|e| match loc {
                Some(l) => e.at(l),
                None => e,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Typed { node: Node::Labels { defs, body, lifted }, ty, loc })
}

/// The `labels` function `c.name` passed to C, lifted with every sibling it
/// reaches: siblings share one environment, so a sibling's capture is the
/// function's too, and C can give it none. `captured` is what each of
/// `defs` names outside its own parameters.
fn lift_label(defs: &[Label], captured: &[Vec<SymRef>], c: LabelsCallback) -> Result<LiftedLabel, Error> {
    let siblings: Vec<SymRef> = defs.iter().map(|d| d.name).collect();
    let at = |name: SymRef| siblings.iter().position(|n| *n == name).expect("a sibling is the block's");
    let mut reached = vec![c.name];
    let mut i = 0;
    while i < reached.len() {
        for n in &captured[at(reached[i])] {
            if siblings.contains(n) && !reached.contains(n) {
                reached.push(*n);
            }
        }
        i += 1;
    }
    let capturing: Vec<String> = reached
        .iter()
        .filter_map(|n| {
            let outside: Vec<String> = captured[at(*n)].iter().filter(|c| !siblings.contains(c)).map(|c| format!("`{}`", c.name())).collect();
            (!outside.is_empty()).then(|| format!("`{}` refers to {}", n.name(), outside.join(", ")))
        })
        .collect();
    if !capturing.is_empty() {
        return Err(Error::TypeError(format!(
            "the local function `{}` cannot be passed to C as a callback: C calls it with \
             nothing but its arguments, and it needs local variables of the code around it \
             ({}{})",
            c.name.name(),
            capturing.join("; "),
            if reached.len() > 1 { " — the local functions it calls count too" } else { "" }
        )));
    }
    let kept = siblings.into_iter().filter(|n| reached.contains(n)).collect();
    Ok(LiftedLabel { name: c.name, path: c.lifted, kept })
}
