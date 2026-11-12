//! A value where a type other than its own is expected: the same type, or a
//! widening the expectation calls for.
//!
//! Where a `Sexpr` is expected, any value with a `Sexpr` encoding may be: a
//! `Symbol` or a `Sexpr` is one as it is, and any other is held as a `Sexpr`
//! by its type ([`super::sexpr_value`]). An `Option<Sexpr>` is not, where a
//! `Sexpr` is expected: that would claim it is not the empty list, which a
//! `match` says. Where a `rational` is expected, an `int` or a `ratio` may
//! be. There is no other widening: the checker has no subtyping.

use crate::typeck::sexpr_value::{is_sexpr_expectation, option_of_sexpr, sexpr_ty};
use crate::types::{Path, Type};
use crate::{wk, Error};
use typelisp_mem::symbols::well_known_symbol;

use super::expr::Cx;
use super::sexpr_value::encoding;
use super::tree::{Node, Typed};

/// `t` where `expected` is expected: `t` itself, its holes solved to agree,
/// or `t` widened to `expected`. `!` is every type.
pub(crate) fn conform(cx: &Cx, t: Typed, expected: Option<&Type>) -> Result<Typed, Error> {
    let Some(e) = expected else { return Ok(t) };
    let unified = cx.holes.borrow_mut().unify(e, &t.ty);
    let Err(m) = unified else { return Ok(t) };
    if let Type::Dyn(to, pins) = cx.resolve(e) {
        return super::dyn_trait::coerce(cx, t, &to, &pins);
    }
    match widen(cx, t, &cx.resolve(e)) {
        Ok(widened) => Ok(widened),
        Err(_) => Err(Error::TypeError(format!("type mismatch: expected `{}`, found `{}`", m.left, m.right))),
    }
}

/// Make `ty`, the type a form will have, agree with `expected` before the
/// form's parts are checked, so `expected` can decide what they are. Where
/// `expected` is a type a value may be widened to, a `ty` that disagrees is
/// not yet an error: the form's value is widened, or refused, when it is
/// checked ([`conform`]).
pub(crate) fn expect(cx: &Cx, ty: &Type, expected: Option<&Type>) -> Result<(), Error> {
    let Some(e) = expected else { return Ok(()) };
    let wanted = cx.resolve(e);
    if !widens_to(&wanted) {
        return super::expr::agree(cx, ty, expected);
    }
    match cx.holes.borrow_mut().unify(e, ty) {
        Ok(()) => Ok(()),
        // Decided when the value is checked, which may widen it.
        Err(_) => Ok(()),
    }
}

/// Whether a value of another type may be widened to `ty`.
fn widens_to(ty: &Type) -> bool {
    is_sexpr_expectation(ty) || matches!(ty, Type::Rational | Type::Dyn(..))
}

/// `t`, of a type other than `wanted`, widened to `wanted`; `t` back when
/// it cannot be.
fn widen(cx: &Cx, t: Typed, wanted: &Type) -> Result<Typed, Typed> {
    // A number literal no type decided yet is read as its default.
    let found = cx.with_default(&t.ty);
    if *wanted == Type::Rational {
        return match found {
            Type::Int => {
                cx.decide(&t.ty);
                Ok(retyped(t, Type::Rational))
            }
            // Normalized on the way in, so that a `rational` that is an
            // integer is always the `int`.
            Type::Ratio => {
                let method = well_known_symbol(wk::M_RATIO_TO_RATIONAL);
                let loc = t.loc.clone();
                let node = Node::MethodCall { owner: Path::primitive("ratio"), method, instance: true, targs: Vec::new(), args: vec![t] };
                Ok(Typed { node, ty: Type::Rational, loc })
            }
            _ => Err(t),
        };
    }
    if !is_sexpr_expectation(wanted) {
        return Err(t);
    }
    // A trait object goes in as the value it holds.
    if let Type::Dyn(..) = found {
        let loc = t.loc.clone();
        return Ok(Typed { node: Node::DynValue { value: Box::new(t) }, ty: wanted.clone(), loc });
    }
    // Narrowing, not widening: an `Option<Sexpr>` may be the empty list.
    if found == option_of_sexpr() {
        return Err(t);
    }
    // Its word is the datum already.
    if found == sexpr_ty() || found == Type::Symbol {
        return Ok(retyped(t, wanted.clone()));
    }
    if encoding(cx.reg, &found).is_none() {
        return Err(t);
    }
    let loc = t.loc.clone();
    Ok(Typed { node: Node::AsSexpr { value: Box::new(t) }, ty: wanted.clone(), loc })
}

/// `t` as a value of `ty`, whose word it already is.
fn retyped(mut t: Typed, ty: Type) -> Typed {
    t.ty = ty;
    t
}
