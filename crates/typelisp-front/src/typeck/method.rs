//! A call to an instance method: `(name receiver arg...)`, the method found
//! on the receiver's type.

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::{agree, check_expr, Cx};
use super::locals::Locals;
use super::registry::MethodDef;
use super::tree::{Node, Typed};

/// `(name receiver arg...)` where `receiver` (already checked) has the
/// method `def` of the type `owner`. Each other argument is checked where
/// its parameter's type is expected.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_method_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    owner: Path,
    method: SymRef,
    def: &MethodDef,
    receiver: Typed,
    rest: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let sig = &def.sig;
    if sig.rest.is_some() || !sig.optionals.is_empty() || !sig.keys.is_empty() || !sig.bounds.is_empty() {
        unimplemented!("typeck: calling {}::{}, which takes &rest, &optional or &key parameters or has :where bounds", owner, method.name())
    }
    if !sig.type_params.is_empty() {
        unimplemented!("typeck: calling the generic method {}::{} (generic methods are not specialized yet)", owner, method.name())
    }
    if rest.len() + 1 != sig.params.len() {
        return Err(Error::TypeError(format!(
            "{}::{}: expected {} argument(s), got {}",
            owner,
            method.name(),
            sig.params.len(),
            rest.len() + 1
        )));
    }
    agree(cx, &receiver.ty, Some(&sig.params[0]))?;
    if mixes_rationals(cx, heap, locals, method, &receiver, rest) {
        unimplemented!("typeck: an operator mixing an `int` and a `ratio` (CL's contagion)")
    }
    let mut args = vec![receiver];
    for ((arg, arg_loc), param) in rest.iter().zip(&sig.params[1..]) {
        args.push(check_expr(cx, heap, locals, *arg, Some(param), arg_loc.clone())?);
    }
    let ret = sig.ret.clone();
    agree(cx, &ret, expected)?;
    Ok(Typed { node: Node::MethodCall { owner, method, args }, ty: ret, loc })
}

/// Whether `(op receiver other)` mixes the two kinds of rational, which CL's
/// contagion makes `rational`'s operator: `(+ 1/2 1)` is `3/2`. Deciding it
/// checks `other` with nothing expected, and forgets what that solved.
fn mixes_rationals(cx: &Cx, heap: &mut Heap, locals: &Locals, op: SymRef, receiver: &Typed, rest: &[(Value, Option<Loc>)]) -> bool {
    const OPERATORS: [u32; 12] = [
        wk::M_OP_ADD, wk::M_OP_SUB, wk::M_OP_MUL, wk::M_OP_DIV, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE,
        wk::RETIRED_EQUALS, wk::M_OP_NE, wk::M_MAX, wk::M_MIN,
    ];
    let [(other, other_loc)] = rest else { return false };
    if !OPERATORS.contains(&op.well_known()) || !matches!(receiver.ty, Type::Int | Type::Ratio) {
        return false;
    }
    let saved = cx.holes.borrow().clone();
    let other = check_expr(cx, heap, locals, *other, None, other_loc.clone());
    *cx.holes.borrow_mut() = saved;
    match other {
        Ok(t) => matches!(t.ty, Type::Int | Type::Ratio | Type::Rational) && t.ty != receiver.ty,
        Err(_) => false,
    }
}
