//! A call to a function the program defined.
//!
//! A generic function's type parameters are replaced with fresh holes for
//! each call (`docs/dev/checker-rebuild-plan.md` 7.2), solved by the
//! arguments and by what the call's result is expected to be. A hole still
//! unsolved when the form is checked is an error: nothing decides that type
//! argument.

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};

use super::args::{check_tail, has_tail, First};
use super::expr::{check_expr, conform_at, Cx};
use super::widen::expect;
use super::locals::Locals;
use super::registry::FnDef;
use super::tree::{Node, Typed};

/// `(f arg...)`, `f` being the function `path` defines as `def`: its first
/// argument `first` when the caller has already checked it, and the rest.
/// Each argument is checked where its parameter's type is expected; an
/// argument already checked is made to agree with it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    path: Path,
    def: &FnDef,
    first: Option<First>,
    rest: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let given = rest.len() + usize::from(first.is_some());
    // `(intern NAME)`: CL's current package is the module the call is
    // written in, which only the checker knows, so it becomes the second
    // argument — the module's path as `intern` reads one.
    if def.builtin && given == 1 && path.std_builtin().is_some_and(|s| s.is(wk::INTERN)) {
        let module = heap.alloc_string(typelisp_mem::symbols::ns_path(cx.ns).join("::"));
        heap.push_root(module);
        let rest: Vec<(Value, Option<Loc>)> = rest.iter().cloned().chain([(module, None)]).collect();
        return check_call(cx, heap, locals, path, def, first, &rest, expected, loc);
    }
    if def.ffi {
        unimplemented!("typeck: calling the C function {} (unsafe is not checked yet)", path)
    }
    let required = def.params.len();
    if has_tail(def) && given < required {
        return Err(Error::TypeError(format!("{}: expected at least {} argument(s), got {}", path, required, given)));
    }
    if !has_tail(def) && given != required {
        return Err(Error::TypeError(format!("{}: expected {} argument(s), got {}", path, required, given)));
    }
    let subst = cx.holes.borrow_mut().instantiate(&def.type_params);
    let mut checked = Vec::with_capacity(given);
    // With no required parameter, a first argument already checked is the
    // first of the tail.
    let (pre, tail): (Option<Typed>, Vec<(Value, Option<Loc>)>) = match first {
        Some(first) if required == 0 => (Some(first.checked), std::iter::once(first.written).chain(rest.iter().cloned()).collect()),
        Some(first) => {
            checked.push(conform_at(cx, first.checked, &subst.apply(&def.params[0]))?);
            (None, rest[required - 1..].to_vec())
        }
        None => (None, rest[required..].to_vec()),
    };
    let fixed = &rest[..required - checked.len()];
    for ((arg, arg_loc), param) in fixed.iter().zip(&def.params[checked.len()..]) {
        let param = subst.apply(param);
        checked.push(check_expr(cx, heap, locals, *arg, Some(&param), arg_loc.clone())?);
    }
    checked.extend(check_tail(cx, heap, locals, &path.to_string(), def, &subst, pre, &tail, given)?);
    let ret = subst.apply(&def.ret);
    expect(cx, &ret, expected)?;
    let targs: Vec<Type> = def.type_params.iter().map(|p| subst.apply(&Type::Param(*p))).collect();
    // A Rust builtin has no body to specialize: one entry point serves every
    // instantiation, as in the current checker.
    let call_targs = if def.builtin { Vec::new() } else { targs.clone() };
    for (p, t) in def.type_params.iter().zip(&targs) {
        let what = format!("cannot infer type parameter `{}` for `{}`", p.name.name(), path);
        cx.must_decide(t.clone(), what, loc.clone());
    }
    for (p, bounds) in &def.bounds {
        for b in bounds {
            cx.require_bound(subst.apply(&Type::Param(*p)), b.at(&subst), path.to_string(), p.name.name().to_string(), loc.clone())?;
        }
    }
    Ok(Typed { node: Node::Call { path, targs: call_targs, args: checked }, ty: ret, loc })
}
