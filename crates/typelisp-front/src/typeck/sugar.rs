//! Call sugar: calls whose meaning depends on how many arguments they have,
//! which no single definition could say ([`call_sugar`]).
//!
//! Each is written as another form, which is checked in its place: `(+ a b
//! c)` is `(+ (+ a b) c)`, `(< a b c)` is `(and (< a b) (< b c))` with each
//! operand evaluated once, `(- x)` is `x`'s negation, and so on. The forms
//! are the old checker's.

use crate::check::analyze::{call_sugar, CallSugar};
use crate::check::forms::{list_from_vec_locs, numeric_identity_literal, wrap_let_star};
use crate::types::Type;
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::{well_known_symbol, NsId};
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// The call `(head arg...)` when it is call sugar: the form it is, checked.
/// `head_is_fn` says whether `head` names a function here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_sugar(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    head: SymRef,
    head_is_fn: bool,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Option<Result<Typed, Error>> {
    let sugar = call_sugar(head.name(), args.len(), || head_is_fn)?;
    let op = heap.symbol_spelling(head).to_string();
    let raw: Vec<Value> = args.iter().map(|(v, _)| *v).collect();
    let written = match sugar {
        CallSugar::NullaryOrUnary => nullary_or_unary(cx, heap, head, &op, &raw, expected),
        CallSugar::VariadicArith => fold(heap, Value::Symbol(head), &raw),
        CallSugar::VariadicCmp => adjacent(cx, heap, head, &raw),
        CallSugar::VariadicAppend => return Some(variadic_append(cx, heap, locals, args, expected, loc)),
        CallSugar::Concatenate => concatenate(heap, &raw),
        CallSugar::Aref => return Some(super::aref::check_aref(cx, heap, locals, &args[0], &args[1..], None, expected, loc)),
        CallSugar::LogWithBase => log_with_base(heap, &raw),
        CallSugar::Atan2 => {
            let atan2 = std_symbol(heap, "atan2");
            form(heap, &[atan2, raw[0], raw[1]])
        }
    };
    Some(written.and_then(|v| check_expr(cx, heap, locals, v, expected, loc)))
}

/// The list of `items`, rooted until the top-level form is checked.
fn form(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let with_locs: Vec<(Value, Option<Loc>)> = items.iter().map(|v| (*v, None)).collect();
    let v = list_from_vec_locs(heap, &with_locs)?;
    heap.push_root(v);
    Ok(v)
}

/// The standard library's `name`.
fn std_symbol(heap: &Heap, name: &str) -> Value {
    Value::Symbol(heap.names().read(NsId::STD, name))
}

/// `(op a b c ...)` as `(op (op a b) c) ...`.
fn fold(heap: &mut Heap, op: Value, args: &[Value]) -> Result<Value, Error> {
    let mut acc = args[0];
    for next in &args[1..] {
        acc = form(heap, &[op, acc, *next])?;
    }
    Ok(acc)
}

/// `(op a b c ...)`, a comparison, as every adjacent pair comparing true,
/// each operand bound to a temporary first so it is evaluated once, in
/// order.
fn adjacent(cx: &Cx, heap: &mut Heap, op: SymRef, args: &[Value]) -> Result<Value, Error> {
    let names: Vec<Value> = args.iter().map(|_| Value::Symbol(cx.temp())).collect();
    let mut tests = vec![heap.intern_symbol("and")];
    for w in names.windows(2) {
        tests.push(form(heap, &[Value::Symbol(op), w[0], w[1]])?);
    }
    let and = form(heap, &tests)?;
    let bound = wrap_let_star(heap, names.into_iter().zip(args.iter().copied()).collect(), and)?;
    heap.push_root(bound);
    Ok(bound)
}

/// `(op)` and `(op x)`: CL's identity, negation, reciprocal and one-operand
/// cases of its arithmetic and comparison operators.
fn nullary_or_unary(cx: &Cx, heap: &mut Heap, op: SymRef, spelling: &str, args: &[Value], expected: Option<&Type>) -> Result<Value, Error> {
    let Some(&x) = args.first() else {
        let value = match spelling {
            // `(gcd)` is 0, as every integer divides 0, and `(lcm)` is 1.
            "+" | "logior" | "logxor" | "gcd" => 0,
            "*" | "lcm" => 1,
            "logand" => -1,
            _ => return Err(Error::TypeError(format!("{}: requires at least 1 argument", spelling))),
        };
        let expected = expected.map(|e| cx.resolve(e));
        return Ok(numeric_identity_literal(heap, expected.as_ref(), value));
    };
    match spelling {
        "+" | "*" | "max" | "min" | "logand" | "logior" | "logxor" => Ok(x),
        // `x`'s own zero or one, `(- x x)` or `(/ x x)`, takes the type of
        // `x`, which a literal could not.
        "-" | "/" => {
            let t = Value::Symbol(cx.temp());
            let op = Value::Symbol(op);
            let same = form(heap, &[op, t, t])?;
            let outer = form(heap, &[op, same, t])?;
            let bound = wrap_let_star(heap, vec![(t, x)], outer)?;
            heap.push_root(bound);
            Ok(bound)
        }
        // CL's one-operand `gcd` and `lcm` are the absolute value.
        "gcd" | "lcm" => {
            let abs = std_symbol(heap, "abs");
            form(heap, &[abs, x])
        }
        // Nothing to compare against: true, the operand evaluated once.
        "<" | "<=" | ">" | ">=" | "=" | "/=" => {
            let progn = Value::Symbol(typelisp_mem::symbols::well_known_symbol(wk::PROGN));
            form(heap, &[progn, x, Value::Bool(true)])
        }
        _ => unreachable!("`call_sugar` names no other operator"),
    }
}

/// `(append a b c ...)`: the left fold of the two-operand `append`s. Of
/// strings, `(append (append a b) c)`; of sequences, each result passed on
/// as `(iter acc)`, the `Vector` the prelude's `append` answers being no
/// `Iter` itself. Which is the first operand's type: it is checked first
/// and bound to a temporary, which the fold starts from.
fn variadic_append(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let (first, first_loc) = &args[0];
    let first = check_expr(cx, heap, locals, *first, None, first_loc.clone())?;
    let strings = cx.with_default(&first.ty) == Type::Str;
    let t = cx.temp();
    let raw: Vec<Value> = std::iter::once(Value::Symbol(t)).chain(args[1..].iter().map(|(v, _)| *v)).collect();
    let roots = heap.root_count();
    let checked = append_fold(heap, &raw, strings).and_then(|fold| locals.bind(vec![(t, first.ty.clone())]).and_then(|l| check_expr(cx, heap, &l, fold, expected, loc.clone())));
    heap.truncate_roots(roots);
    let checked = checked?;
    let ty = checked.ty.clone();
    Ok(Typed { node: Node::Let { binds: vec![(t, first)], body: vec![checked] }, ty, loc })
}

/// The fold [`variadic_append`] says, of the operands `args`.
fn append_fold(heap: &mut Heap, args: &[Value], strings: bool) -> Result<Value, Error> {
    let append = std_symbol(heap, "append");
    let iter = std_symbol(heap, "iter");
    let mut acc = args[0];
    for (i, next) in args[1..].iter().enumerate() {
        if i > 0 && !strings {
            acc = form(heap, &[iter, acc])?;
        }
        acc = form(heap, &[append, acc, *next])?;
    }
    Ok(acc)
}

/// CL's `concatenate`: `(concatenate 'string s ...)` joins strings into a
/// string, `(concatenate 'Vector seq ...)` sequences into a `Vector`. The
/// result type is a quoted symbol, read here.
fn concatenate(heap: &mut Heap, args: &[Value]) -> Result<Value, Error> {
    let usage = "concatenate: (concatenate 'string s ...) or (concatenate 'Vector seq ...)";
    let result_type = args
        .first()
        .and_then(|&f| heap.list_to_vec(f).ok())
        .filter(|elems| elems.len() == 2 && matches!(elems[0], Value::Symbol(id) if id.is(wk::QUOTE)))
        .and_then(|elems| match elems[1] {
            Value::Symbol(id) => Some(id),
            _ => None,
        })
        .ok_or_else(|| Error::TypeError(format!("{} — the result type must be a quoted symbol", usage)))?;
    let seqs = &args[1..];
    match (result_type.well_known(), seqs.len()) {
        (wk::STRING, 0) => {
            let empty = heap.alloc_string(String::new());
            heap.push_root(empty);
            Ok(empty)
        }
        (wk::STRING, 1) => {
            let the = Value::Symbol(well_known_symbol(wk::THE));
            let string = std_symbol(heap, "string");
            form(heap, &[the, string, seqs[0]])
        }
        (wk::STRING, _) => append_fold(heap, seqs, true),
        (wk::VECTOR, 0) => Err(Error::TypeError(format!("{} — `(concatenate 'Vector)` needs at least one sequence to take the element type from", usage))),
        (wk::VECTOR, 1) => {
            let copy = std_symbol(heap, "copy-seq");
            form(heap, &[copy, seqs[0]])
        }
        (wk::VECTOR, _) => append_fold(heap, seqs, false),
        _ => Err(Error::TypeError(format!(
            "{} — unknown result type `{}` (a `Sexpr` list is joined with `sexpr-append`)",
            usage,
            heap.symbol_spelling(result_type)
        ))),
    }
}

/// `(log number base)` as `(/ (log number) (log base))`.
fn log_with_base(heap: &mut Heap, args: &[Value]) -> Result<Value, Error> {
    let log = std_symbol(heap, "log");
    let number = form(heap, &[log, args[0]])?;
    let base = form(heap, &[log, args[1]])?;
    let div = std_symbol(heap, "/");
    form(heap, &[div, number, base])
}
