//! A call to a function the program defined.

use crate::types::Path;
use crate::{Error, Heap, Loc, Value};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::registry::FnDef;
use super::tree::{Node, Typed};

/// `(f arg...)`, `f` being the function `path` defines as `def`. Each
/// argument is checked where its parameter's type is expected.
pub(crate) fn check_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    path: Path,
    def: &FnDef,
    args: &[(Value, Option<Loc>)],
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
    let mut checked = Vec::with_capacity(args.len());
    for ((arg, arg_loc), param) in args.iter().zip(&def.params) {
        checked.push(check_expr(cx, heap, locals, *arg, Some(param), arg_loc.clone())?);
    }
    Ok(Typed { node: Node::Call { path, args: checked }, ty: def.ret.clone(), loc })
}
