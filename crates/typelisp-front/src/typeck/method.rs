//! A call to a method: `(name receiver arg...)`, the instance method found
//! on the receiver's type, or `(Type::name arg...)`, a static method.

use crate::types::{Path, Type, TypeParam};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::args::{check_tail, has_tail};
use super::expr::{agree, check_expr, conform_at, Cx};
use super::widen::expect;
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
    if !sig.bounds.is_empty() {
        unimplemented!("typeck: calling {}::{}, which has :where bounds", owner, method.name())
    }
    let given = rest.len() + usize::from(receiver.is_some());
    let required = sig.params.len();
    let what = format!("{}::{}", owner, method.name());
    if has_tail(sig) && given < required {
        return Err(Error::TypeError(format!("{}: expected at least {} argument(s), got {}", what, required, given)));
    }
    if !has_tail(sig) && given != required {
        return Err(Error::TypeError(format!("{}: expected {} argument(s), got {}", what, required, given)));
    }
    let subst = cx.holes.borrow_mut().instantiate(&sig.type_params);
    let instance = receiver.is_some();
    let mut args = Vec::with_capacity(sig.params.len());
    let mut rational_op = false;
    if let Some(receiver) = receiver {
        agree(cx, &receiver.ty, Some(&subst.apply(&sig.params[0])))?;
        rational_op = is_rational_operator(method, &receiver, rest);
        args.push(receiver);
    }
    let (fixed, tail) = rest.split_at(required - args.len());
    for ((arg, arg_loc), param) in fixed.iter().zip(&sig.params[args.len()..]) {
        let param = subst.apply(param);
        if !rational_op {
            args.push(check_expr(cx, heap, locals, *arg, Some(&param), arg_loc.clone())?);
            continue;
        }
        // The other operand of a rational's operator is checked with nothing
        // expected, to see whether it is the other kind of rational: CL's
        // contagion makes `(+ 1/2 1)` `rational`'s operator, `3/2`.
        let other = check_expr(cx, heap, locals, *arg, None, arg_loc.clone())?;
        let seen = cx.with_default(&other.ty);
        if matches!(seen, Type::Int | Type::Ratio | Type::Rational) && seen != args[0].ty {
            unimplemented!("typeck: an operator mixing an `int` and a `ratio` (CL's contagion)")
        }
        args.push(conform_at(cx, other, &param)?);
    }
    args.extend(check_tail(cx, heap, locals, &what, sig, &subst, None, tail, given)?);
    let ret = subst.apply(&sig.ret);
    expect(cx, &ret, expected)?;
    let targs: Vec<Type> = sig.type_params.iter().map(|p| subst.apply(&Type::Param(*p))).collect();
    // The type's parameters come first, then the method's own.
    let owner_params = cx.reg.type_def(&owner).map_or(0, |d| d.params.len());
    for (i, (p, t)) in sig.type_params.iter().zip(&targs).enumerate() {
        // A builtin whose signature never mentions `p` is not affected by
        // it: it has no body, and nothing it is passed or returns carries
        // `p` (`Thread::current-id`, a static function of `Thread<T>`).
        if sig.builtin && !sig.params.iter().chain([&sig.ret]).any(|ty| mentions(ty, p)) {
            continue;
        }
        let noun = if i < owner_params { "type argument" } else { "type parameter" };
        let what = format!("cannot infer {} `{}` for `{}::{}`", noun, p.name.name(), owner, method.name());
        cx.must_decide(t.clone(), what, loc.clone());
    }
    // A Rust builtin has no body to specialize: one entry point serves every
    // instantiation, and the result's type, which the lowering writes,
    // carries the instantiation (`Vector::new`'s `Vector<T>`).
    let targs = if sig.builtin { Vec::new() } else { targs };
    // A method written for one instantiation has code under that
    // instantiation's name.
    let method = match &def.inst_args {
        Some(inst) => super::generic::method_specialization(&owner, method, inst),
        None => method,
    };
    Ok(Typed { node: Node::MethodCall { owner, method, instance, targs, args }, ty: ret, loc })
}

/// Whether the type `ty` mentions the type parameter `p`.
fn mentions(ty: &Type, p: &TypeParam) -> bool {
    let mut found = false;
    super::unify::walk(ty, &mut |t| found |= matches!(t, Type::Param(q) if q == p));
    found
}

/// Whether `(op receiver other)` is an operator of `int` or `ratio`, whose
/// other operand may be the other kind of rational.
fn is_rational_operator(op: SymRef, receiver: &Typed, rest: &[(Value, Option<Loc>)]) -> bool {
    const OPERATORS: [u32; 12] = [
        wk::M_OP_ADD, wk::M_OP_SUB, wk::M_OP_MUL, wk::M_OP_DIV, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE,
        wk::RETIRED_EQUALS, wk::M_OP_NE, wk::M_MAX, wk::M_MIN,
    ];
    rest.len() == 1 && OPERATORS.contains(&op.well_known()) && matches!(receiver.ty, Type::Int | Type::Ratio)
}
