//! A call to a function the program defined.
//!
//! A generic function's type parameters are replaced with fresh holes for
//! each call (`docs/dev/checker-rebuild-plan.md` 7.2), solved by the
//! arguments and by what the call's result is expected to be. A hole still
//! unsolved after that is an error: nothing decides that type argument.

use crate::types::{Path, Type};
use crate::{Error, Heap, Loc, Value};

use super::expr::{agree, check_expr, Cx};
use super::locals::Locals;
use super::registry::FnDef;
use super::tree::{Node, Typed};

/// `(f arg...)`, `f` being the function `path` defines as `def`. Each
/// argument is checked where its parameter's type is expected.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    path: Path,
    def: &FnDef,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    if args.len() != def.params.len() {
        return Err(Error::TypeError(format!(
            "{}: expected {} argument(s), got {}",
            path,
            def.params.len(),
            args.len()
        )));
    }
    let subst = cx.holes.borrow_mut().instantiate(&def.type_params);
    let mut checked = Vec::with_capacity(args.len());
    for ((arg, arg_loc), param) in args.iter().zip(&def.params) {
        let param = subst.apply(param);
        checked.push(check_expr(cx, heap, locals, *arg, Some(&param), arg_loc.clone())?);
    }
    let ret = subst.apply(&def.ret);
    agree(cx, &ret, expected)?;
    let targs: Vec<Type> = def.type_params.iter().map(|p| subst.apply(&Type::Param(*p))).collect();
    for (p, t) in def.type_params.iter().zip(&targs) {
        if cx.holes.borrow().unsolved(t).is_some() {
            return Err(Error::TypeError(format!(
                "{}: cannot infer type parameter `{}` — no argument or expected type decides it",
                path,
                p.name.name()
            )));
        }
    }
    Ok(Typed { node: Node::Call { path, targs, args: checked }, ty: ret, loc })
}
