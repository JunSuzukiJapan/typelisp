//! A call to a method: `(name receiver arg...)`, the instance method found
//! on the receiver's type, or `(Type::name arg...)`, a static method.

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::{agree, check_expr, Cx};
use super::locals::Locals;
use super::registry::MethodDef;
use super::tree::{Node, Typed};

/// A call to the method `def` of the type `owner`: an instance method's with
/// its `receiver` already checked, a static method's with none. Each other
/// argument in `rest` is checked where its parameter's type is expected.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_method_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    owner: Path,
    method: SymRef,
    def: &MethodDef,
    receiver: Option<Typed>,
    rest: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let sig = &def.sig;
    if sig.rest.is_some() || !sig.optionals.is_empty() || !sig.keys.is_empty() || !sig.bounds.is_empty() {
        unimplemented!("typeck: calling {}::{}, which takes &rest, &optional or &key parameters or has :where bounds", owner, method.name())
    }
    if !sig.type_params.is_empty() && sig.builtin {
        unimplemented!("typeck: calling the built-in generic method {}::{} (built-in generic methods are not specialized yet)", owner, method.name())
    }
    let given = rest.len() + usize::from(receiver.is_some());
    if given != sig.params.len() {
        return Err(Error::TypeError(format!(
            "{}::{}: expected {} argument(s), got {}",
            owner,
            method.name(),
            sig.params.len(),
            given
        )));
    }
    let subst = cx.holes.borrow_mut().instantiate(&sig.type_params);
    let instance = receiver.is_some();
    let mut args = Vec::with_capacity(sig.params.len());
    if let Some(receiver) = receiver {
        agree(cx, &receiver.ty, Some(&subst.apply(&sig.params[0])))?;
        if mixes_rationals(cx, heap, locals, method, &receiver, rest) {
            unimplemented!("typeck: an operator mixing an `int` and a `ratio` (CL's contagion)")
        }
        args.push(receiver);
    }
    let params = &sig.params[args.len()..];
    for ((arg, arg_loc), param) in rest.iter().zip(params) {
        args.push(check_expr(cx, heap, locals, *arg, Some(&subst.apply(param)), arg_loc.clone())?);
    }
    let ret = subst.apply(&sig.ret);
    agree(cx, &ret, expected)?;
    let targs: Vec<Type> = sig.type_params.iter().map(|p| subst.apply(&Type::Param(*p))).collect();
    for (p, t) in sig.type_params.iter().zip(&targs) {
        let what = format!(
            "{}::{}: cannot infer type parameter `{}` — no argument or expected type decides it",
            owner,
            method.name(),
            p.name.name()
        );
        cx.must_decide(t.clone(), what, loc.clone());
    }
    // A method written for one instantiation has code under that
    // instantiation's name.
    let method = match &def.inst_args {
        Some(inst) => super::generic::method_specialization(&owner, method, inst),
        None => method,
    };
    Ok(Typed { node: Node::MethodCall { owner, method, instance, targs, args }, ty: ret, loc })
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
    let saved = cx.snapshot();
    let other = check_expr(cx, heap, locals, *other, None, other_loc.clone());
    cx.restore(saved);
    match other {
        Ok(t) => matches!(t.ty, Type::Int | Type::Ratio | Type::Rational) && t.ty != receiver.ty,
        Err(_) => false,
    }
}
