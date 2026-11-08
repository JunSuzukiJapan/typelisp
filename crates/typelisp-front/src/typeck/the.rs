//! `the`: `(the Type expr)`, `expr` checked where a `Type` is expected.

use crate::{Error, Heap, Loc, Value};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::Typed;
use super::type_syntax::parse_type;

/// `(the Type expr)` — `args` without the `the`. The value is `expr`'s; what
/// the form around it expects is agreed with by the caller, as for any
/// expression.
pub(crate) fn check_the(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
) -> Result<Typed, Error> {
    let [(ty, _), (expr, expr_loc)] = args else {
        return Err(Error::TypeError("the: (the Type expr)".into()));
    };
    let ty = parse_type(heap, &cx.types(), *ty)?;
    check_expr(cx, heap, locals, *expr, Some(&ty), expr_loc.clone())
}
