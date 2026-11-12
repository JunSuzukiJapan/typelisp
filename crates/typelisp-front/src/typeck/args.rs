//! The arguments of a call after its required ones: `&optional`, `&key` and
//! `&rest` parameters.
//!
//! A call is made a call with one argument for each parameter, so what runs
//! it never sees these markers. An `&optional` argument is given by
//! position, a `&key` one as `:name value`, and the arguments left after the
//! optional ones go to `&rest`, as one `Sexpr` list. A parameter the call
//! leaves out is given its default, the core form its definition checked,
//! or `none` when it has no default; one with no default that the call
//! gives is given as `(some value)`. A function has `&key` parameters or
//! `&optional` and `&rest` ones, never both.

use crate::typeck::registry::OptKeyParam;
use crate::types::Type;
use crate::{Error, Heap, Loc, Value};

use super::expr::{check_expr, conform_at, Cx};
use super::format::sexpr_list;
use super::locals::Locals;
use super::registry::FnDef;
use super::subst::Subst;
use super::tree::{Node, Typed};

/// A call's first argument, checked before the callee was known: what it
/// is, and how it was written.
pub(crate) struct First {
    pub(crate) checked: Typed,
    pub(crate) written: (Value, Option<Loc>),
}

/// An argument of a call: as written, or already checked.
enum Arg<'a> {
    Written(&'a (Value, Option<Loc>)),
    Checked(Typed),
}

/// The arguments `tail` that follow the required ones of a call of `sig`,
/// which `what` names in a message, checked and given one for each
/// `&optional`, `&key` and `&rest` parameter, in order. `pre` is the first
/// of `tail` when it is checked already. `given` is how many arguments the
/// call has in all.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_tail(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    what: &str,
    sig: &FnDef,
    subst: &Subst,
    mut pre: Option<Typed>,
    tail: &[(Value, Option<Loc>)],
    given: usize,
) -> Result<Vec<Typed>, Error> {
    if !sig.keys.is_empty() {
        // The first of the tail is a keyword, a parameter's name: its value
        // is not an argument.
        drop(pre);
        return check_keys(cx, heap, locals, what, sig, subst, tail);
    }
    let mut arg = |i: usize| match pre.take_if(|_| i == 0) {
        Some(t) => Arg::Checked(t),
        None => Arg::Written(&tail[i]),
    };
    let supplied = tail.len().min(sig.optionals.len());
    let mut out = Vec::with_capacity(sig.optionals.len() + 1);
    for (i, opt) in sig.optionals.iter().enumerate() {
        let given = (i < supplied).then(|| arg(i));
        out.push(given_or_default(cx, heap, locals, opt, subst, given)?);
    }
    match &sig.rest {
        Some(elem) => {
            let elem = subst.apply(elem);
            let mut items = Vec::with_capacity(tail.len() - supplied);
            for i in supplied..tail.len() {
                items.push(check_arg(cx, heap, locals, arg(i), &elem)?);
            }
            out.push(sexpr_list(items));
        }
        None if tail.len() > supplied => {
            return Err(Error::TypeError(format!(
                "{}: expected at most {} argument(s), got {}",
                what,
                sig.params.len() + sig.optionals.len(),
                given
            )));
        }
        None => {}
    }
    Ok(out)
}

/// `arg` where `ty` is expected.
fn check_arg(cx: &Cx, heap: &mut Heap, locals: &Locals, arg: Arg, ty: &Type) -> Result<Typed, Error> {
    match arg {
        Arg::Written((v, loc)) => check_expr(cx, heap, locals, *v, Some(ty), loc.clone()),
        Arg::Checked(t) => conform_at(cx, t, ty),
    }
}
/// `tail` as `:name value` pairs, one for each of `sig`'s `&key`
/// parameters, in the order they are declared.
fn check_keys(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    what: &str,
    sig: &FnDef,
    subst: &Subst,
    tail: &[(Value, Option<Loc>)],
) -> Result<Vec<Typed>, Error> {
    if !tail.len().is_multiple_of(2) {
        return Err(Error::TypeError(format!("{}: keyword arguments must be given as `:name value` pairs", what)));
    }
    let mut given: Vec<(usize, &(Value, Option<Loc>))> = Vec::new();
    for pair in tail.chunks(2) {
        let name = match pair[0].0 {
            Value::Symbol(id) if id.is_keyword() => id,
            Value::Symbol(id) => {
                return Err(Error::TypeError(format!("{}: expected a `:name` keyword, got `{}`", what, heap.symbol_spelling(id))));
            }
            _ => return Err(Error::TypeError(format!("{}: expected a `:name` keyword, got a non-symbol", what))),
        };
        let Some(index) = sig.keys.iter().position(|k| k.name == name) else {
            return Err(Error::TypeError(format!("{}: unknown keyword argument :{}", what, name.name())));
        };
        if given.iter().any(|(i, _)| *i == index) {
            return Err(Error::TypeError(format!("{}: duplicate keyword argument :{}", what, name.name())));
        }
        given.push((index, &pair[1]));
    }
    // Checked in the order the call writes them, which is the order they run.
    let mut checked: Vec<Option<Typed>> = sig.keys.iter().map(|_| None).collect();
    for (index, arg) in given {
        checked[index] = Some(given_or_default(cx, heap, locals, &sig.keys[index], subst, Some(Arg::Written(arg)))?);
    }
    sig.keys
        .iter()
        .zip(checked)
        .map(|(key, c)| match c {
            Some(c) => Ok(c),
            None => given_or_default(cx, heap, locals, key, subst, None),
        })
        .collect()
}

/// The argument for the `&optional` or `&key` parameter `p`: `arg` when the
/// call gives it, `(some arg)` when `p` has no default; `p`'s default, or
/// `none`, when the call leaves it out.
fn given_or_default(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    p: &OptKeyParam,
    subst: &Subst,
    arg: Option<Arg>,
) -> Result<Typed, Error> {
    let ty = subst.apply(&p.decl_ty);
    match (arg, &p.default) {
        (Some(arg), default) => {
            let value = check_arg(cx, heap, locals, arg, &ty)?;
            Ok(if default.is_some() { value } else { Typed::some(value, None) })
        }
        (None, Some(form)) => Ok(Typed { node: Node::Default { form: form.clone() }, ty, loc: None }),
        (None, None) => Ok(Typed::none(ty)),
    }
}

/// Whether `sig` has parameters after its required ones.
pub(crate) fn has_tail(sig: &FnDef) -> bool {
    sig.rest.is_some() || !sig.optionals.is_empty() || !sig.keys.is_empty()
}
