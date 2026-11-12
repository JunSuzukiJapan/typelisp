//! A call to a method: `(name receiver arg...)`, the instance method found
//! on the receiver's type, or `(Type::name arg...)`, a static method.

use crate::typeck::sexpr_value::is_option_of_sexpr;
use crate::types::{path_is_builtin, Path, Type, TypeParam};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::args::{check_tail, has_tail};
use super::bounds::{in_params, BoundMethod};
use super::expr::{agree, check_expr, conform_at, Cx};
use super::widen::expect;
use super::locals::Locals;
use super::registry::MethodDef;
use super::tree::{Node, Typed};

/// A call to the method `def` of the type `owner`: an instance method's with
/// its `receiver` already checked, a static method's with none. Each other
/// argument in `rest` is checked where its parameter's type is expected.
/// `written` are the type's arguments the call's path wrote
/// (`(Type<args>::name ...)`), none when it wrote none.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_method_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    owner: Path,
    method: SymRef,
    def: &MethodDef,
    receiver: Option<Typed>,
    written: &[Type],
    rest: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let sig = &def.sig;
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
    // The type's parameters come first; one instantiation's method has none
    // of them, and its instantiation was chosen by what was written.
    if def.inst_args.is_none() {
        for (p, t) in sig.type_params.iter().zip(written) {
            agree(cx, &subst.apply(&Type::Param(*p)), Some(t))?;
        }
    }
    let instance = receiver.is_some();
    let mut args = Vec::with_capacity(sig.params.len());
    let mut rational_op = false;
    if let Some(receiver) = receiver {
        // An `Option<Sexpr>` is the type an S-expression has: it is the
        // receiver of `Sexpr`'s methods as it is (`resolve::receiver_method`).
        let sexpr_receiver = path_is_builtin(&owner, wk::SEXPR) && is_option_of_sexpr(&cx.resolve(&receiver.ty));
        if !sexpr_receiver {
            agree(cx, &receiver.ty, Some(&subst.apply(&sig.params[0])))?;
        }
        rational_op = is_rational_operator(cx, method, &receiver, rest);
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
        if matches!(seen, Type::Int | Type::Ratio | Type::Rational) && seen != cx.with_default(&args[0].ty) {
            // An `int` and a `ratio`: `rational`'s operator, both widened.
            let receiver = args.pop().expect("the receiver was checked first");
            return rational_call(cx, method, receiver, other, expected, loc);
        }
        args.push(conform_at(cx, other, &param)?);
    }
    args.extend(check_tail(cx, heap, locals, &what, sig, &subst, None, tail, given)?);
    let ret = subst.apply(&sig.ret);
    expect(cx, &ret, expected)?;
    let targs: Vec<Type> = sig.type_params.iter().map(|p| subst.apply(&Type::Param(*p))).collect();
    // The type's parameters come first, then the method's own.
    // One instantiation's method has its own alone.
    let owner_params = match def.inst_args {
        Some(_) => 0,
        None => cx.reg.type_def(&owner).map_or(0, |d| d.params.len()),
    };
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
    for (p, bounds) in &sig.bounds {
        for b in bounds {
            cx.require_bound(subst.apply(&Type::Param(*p)), b.at(&subst), format!("{}::{}", owner, method.name()), p.name.name().to_string(), loc.clone())?;
        }
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

/// A call of the method `method` of a trait bounding the type parameter
/// that is `receiver`'s type, `bound` saying which trait and at what.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_trait_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    bound: BoundMethod,
    method: SymRef,
    receiver: Typed,
    rest: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let sig = bound.sig;
    let what = format!("{}::{}", bound.trait_path, method.name());
    // An associated type the bound does not pin is the one of the bounded
    // type, which nothing here knows more of.
    let self_ty = bound.at[0].clone().expect("a bound says what its `Self` is");
    let filled: Vec<Option<Type>> = bound
        .at
        .iter()
        .zip(&sig.type_params)
        .map(|(t, p)| Some(t.clone().unwrap_or_else(|| Type::Proj(bound.trait_path, Box::new(self_ty.clone()), p.name.name().to_string()))))
        .collect();
    // The method's own type parameters, after the trait's, are holes the
    // call solves.
    let own: Vec<Type> = sig.type_params[filled.len()..].iter().map(|_| cx.holes.borrow_mut().fresh()).collect();
    for (p, t) in sig.type_params[filled.len()..].iter().zip(&own) {
        cx.must_decide(t.clone(), format!("cannot infer type parameter `{}` for `{}`", p.name.name(), what), loc.clone());
    }
    let filled: Vec<Option<Type>> = filled.into_iter().chain(own.iter().cloned().map(Some)).collect();
    let at = |ty: &Type| in_params(&sig.type_params, &filled, ty).expect("every parameter of the trait is filled");
    // The method's own type parameters meet the bounds the trait declares.
    for (p, bounds) in &sig.bounds {
        for b in bounds {
            let b = crate::typeck::registry::TraitBound { trait_path: b.trait_path, assoc: b.assoc.iter().map(|(k, t)| (k.clone(), at(t))).collect() };
            cx.require_bound(at(&Type::Param(*p)), b, what.clone(), p.name.name().to_string(), loc.clone())?;
        }
    }
    assert!(!super::args::has_tail(sig), "a trait's method declares no &optional, &rest or &key parameter (`impl_block::method_params`)");
    let given = rest.len() + 1;
    if given != sig.params.len() {
        return Err(Error::TypeError(format!("{}: expected {} argument(s), got {}", what, sig.params.len(), given)));
    }
    agree(cx, &receiver.ty, Some(&at(&sig.params[0])))?;
    let mut args = vec![receiver];
    for ((arg, arg_loc), param) in rest.iter().zip(&sig.params[1..]) {
        args.push(check_expr(cx, heap, locals, *arg, Some(&at(param)), arg_loc.clone())?);
    }
    let ret = at(&sig.ret);
    expect(cx, &ret, expected)?;
    Ok(Typed { node: Node::TraitCall { trait_path: bound.trait_path, method, own, args }, ty: ret, loc })
}

/// Whether the type `ty` mentions the type parameter `p`.
fn mentions(ty: &Type, p: &TypeParam) -> bool {
    let mut found = false;
    super::unify::walk(ty, &mut |t| found |= matches!(t, Type::Param(q) if q == p));
    found
}

/// Whether `(op receiver other)` is an operator of `int` or `ratio`, whose
/// other operand may be the other kind of rational.
/// `(op receiver other)`, an operator on an `int` and a `ratio` (CL's
/// contagion): `rational`'s `op`, each operand widened to a `rational`.
/// `(+ 1/2 1)` is `3/2`.
fn rational_call(cx: &Cx, method: SymRef, receiver: Typed, other: Typed, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let owner = crate::types::prim_type_path(&Type::Rational).expect("rational is a primitive type");
    let def = cx.reg.method(&owner, method).expect("`rational` has every operator an `int` and a `ratio` have");
    let args = vec![conform_at(cx, receiver, &Type::Rational)?, conform_at(cx, other, &Type::Rational)?];
    let ret = def.sig.ret.clone();
    expect(cx, &ret, expected)?;
    Ok(Typed { node: Node::MethodCall { owner, method, instance: true, targs: Vec::new(), args }, ty: ret, loc })
}

fn is_rational_operator(cx: &Cx, op: SymRef, receiver: &Typed, rest: &[(Value, Option<Loc>)]) -> bool {
    const OPERATORS: [u32; 12] = [
        wk::M_OP_ADD, wk::M_OP_SUB, wk::M_OP_MUL, wk::M_OP_DIV, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE,
        wk::RETIRED_EQUALS, wk::M_OP_NE, wk::M_MAX, wk::M_MIN,
    ];
    rest.len() == 1 && OPERATORS.contains(&op.well_known()) && matches!(cx.with_default(&receiver.ty), Type::Int | Type::Ratio)
}
