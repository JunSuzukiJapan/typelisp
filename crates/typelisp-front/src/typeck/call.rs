//! A call to a function the program defined.
//!
//! A generic function's type parameters are replaced with fresh holes for
//! each call (`docs/dev/checker-rebuild-plan.md` 7.2), solved by the
//! arguments and by what the call's result is expected to be. A hole still
//! unsolved when the form is checked is an error: nothing decides that type
//! argument.

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};

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
    first: Option<Typed>,
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
    if def.rest.is_some() || !def.optionals.is_empty() || !def.keys.is_empty() || !def.bounds.is_empty() {
        unimplemented!("typeck: calling {}, which takes &rest, &optional or &key parameters or has :where bounds", path)
    }
    if def.ffi {
        unimplemented!("typeck: calling the C function {} (unsafe is not checked yet)", path)
    }
    if given != def.params.len() {
        return Err(Error::TypeError(format!("{}: expected {} argument(s), got {}", path, def.params.len(), given)));
    }
    let subst = cx.holes.borrow_mut().instantiate(&def.type_params);
    let mut checked = Vec::with_capacity(given);
    if let Some(first) = first {
        checked.push(conform_at(cx, first, &subst.apply(&def.params[0]))?);
    }
    let params = &def.params[checked.len()..];
    for ((arg, arg_loc), param) in rest.iter().zip(params) {
        let param = subst.apply(param);
        checked.push(check_expr(cx, heap, locals, *arg, Some(&param), arg_loc.clone())?);
    }
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
    Ok(Typed { node: Node::Call { path, targs: call_targs, args: checked }, ty: ret, loc })
}
