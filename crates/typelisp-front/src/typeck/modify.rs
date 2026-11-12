//! The forms that read a place and write it back: `(incf place delta)`,
//! `(decf place delta)`, `(rotatef place...)` and `(shiftf place...
//! newvalue)`.
//!
//! Each is written as a `setf` form, which is checked in its place. A call
//! place's arguments are first bound to temporaries, so each is evaluated
//! once although the place is both read and written; a symbol macro's place
//! is its form's.

use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::well_known_symbol;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::Typed;

/// `(incf ...)` or `(decf ...)` — `args` without the head.
pub(crate) fn check_incf_decf(cx: &Cx, heap: &mut Heap, locals: &Locals, head: u32, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let (name, op) = if head == wk::INCF { ("incf", "+") } else { ("decf", "-") };
    if args.is_empty() || args.len() > 2 {
        return Err(Error::TypeError(format!("{}: ({} place) or ({} place delta)", name, name, name)));
    }
    let delta = args.get(1).map_or(Value::Int(1), |(v, _)| *v);
    let mut bindings = Vec::new();
    let place = fresh_place(cx, heap, locals, args[0].0, &mut bindings)?;
    let op = heap.intern_symbol(op);
    let value = form(heap, &[op, place, delta])?;
    let set = form(heap, &[Value::Symbol(well_known_symbol(wk::SETF)), place, value])?;
    let written = let_star(heap, &bindings, set)?;
    check_expr(cx, heap, locals, written, None, loc)
}

/// `(rotatef ...)` or `(shiftf ...)` — `args` without the head. Every place
/// is read before any is written; each then takes the next one's value, the
/// last the first's (`rotatef`) or `newvalue` (`shiftf`, which is the first
/// place's old value).
pub(crate) fn check_rotatef_shiftf(cx: &Cx, heap: &mut Heap, locals: &Locals, head: u32, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let shift = head == wk::SHIFTF;
    let (places, newvalue) = if shift {
        match args.split_last() {
            Some(((last, _), rest)) if !rest.is_empty() => (rest, Some(*last)),
            _ => return Err(Error::TypeError("shiftf: (shiftf place... newvalue)".into())),
        }
    } else {
        (args, None)
    };
    let mut bindings = Vec::new();
    let mut fresh = Vec::with_capacity(places.len());
    for (place, _) in places {
        fresh.push(fresh_place(cx, heap, locals, *place, &mut bindings)?);
    }
    let mut olds = Vec::with_capacity(fresh.len());
    for place in &fresh {
        let old = Value::Symbol(cx.temp());
        bindings.push((old, *place));
        olds.push(old);
    }
    let last = match newvalue {
        Some(v) => {
            let t = Value::Symbol(cx.temp());
            bindings.push((t, v));
            t
        }
        None => olds.first().copied().unwrap_or(Value::Empty),
    };
    let mut body = vec![Value::Symbol(well_known_symbol(wk::PROGN))];
    for (i, place) in fresh.iter().enumerate() {
        let from = olds.get(i + 1).copied().unwrap_or(last);
        body.push(form(heap, &[Value::Symbol(well_known_symbol(wk::SETF)), *place, from])?);
    }
    body.push(if shift { olds[0] } else { Value::Empty });
    let body = form(heap, &body)?;
    let written = let_star(heap, &bindings, body)?;
    check_expr(cx, heap, locals, written, None, loc)
}

/// `place` with each argument of a call place replaced by a temporary,
/// whose binding is pushed onto `bindings`. A symbol macro's place is its
/// form's.
fn fresh_place(cx: &Cx, heap: &mut Heap, locals: &Locals, place: Value, bindings: &mut Vec<(Value, Value)>) -> Result<Value, Error> {
    match place {
        Value::Symbol(name) => match locals.symbol_macro(name) {
            Some(expansion) => fresh_place(cx, heap, locals, expansion, bindings),
            None => Ok(place),
        },
        Value::Cons(_) if matches!(heap.car(place), Ok(Value::Symbol(head)) if head.is(wk::VALUE_MEMBER)) => Ok(place),
        Value::Cons(_) => {
            let items = heap.list_to_vec(place).map_err(|_| Error::TypeError("place: must be a proper list".into()))?;
            let Some((head, args)) = items.split_first() else {
                return Err(Error::TypeError("place: empty call form".into()));
            };
            let mut fresh = vec![*head];
            for arg in args {
                let t = Value::Symbol(cx.temp());
                bindings.push((t, *arg));
                fresh.push(t);
            }
            form(heap, &fresh)
        }
        _ => Err(Error::TypeError("place: target must be a variable, `var::field`, or a `(get recv key...)` call form".into())),
    }
}

/// The list of `items`, rooted until the top-level form is checked.
fn form(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let with_locs: Vec<(Value, Option<Loc>)> = items.iter().map(|v| (*v, None)).collect();
    let v = crate::check::forms::list_from_vec_locs(heap, &with_locs)?;
    heap.push_root(v);
    Ok(v)
}

/// `(let* ((name value)...) body)`, or `body` when nothing is bound.
fn let_star(heap: &mut Heap, bindings: &[(Value, Value)], body: Value) -> Result<Value, Error> {
    if bindings.is_empty() {
        return Ok(body);
    }
    let mut pairs = Vec::with_capacity(bindings.len());
    for (name, value) in bindings {
        pairs.push(form(heap, &[*name, *value])?);
    }
    let pairs = form(heap, &pairs)?;
    form(heap, &[Value::Symbol(well_known_symbol(wk::LET_STAR)), pairs, body])
}
