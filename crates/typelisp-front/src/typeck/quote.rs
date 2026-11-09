//! `quote`: `(quote datum)`, or `'datum`, the datum as it was read; and
//! `quasiquote`, a template whose `,x` and `,@x` are filled in.
//!
//! A quoted symbol is a `Symbol`: the quote is its only literal syntax, as
//! `42` is an integer's. Any other datum is S-expression data,
//! `Option<Sexpr>`, since `'()` is one and the empty list is `none`. A
//! quasiquote's value is always S-expression data.

use crate::check::analyze::option_of_sexpr;
use crate::sexpr_variant::CONS;
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::registry::ValueDef;
use super::resolve::value_named;
use super::tree::{Node, Typed};

/// `(quote datum)` — `args` without the `quote`.
pub(crate) fn check_quote(heap: &mut Heap, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(datum, _)] = args else {
        return Err(Error::TypeError("quote: (quote datum)".into()));
    };
    let ty = match datum {
        Value::Symbol(_) => Type::Symbol,
        _ => option_of_sexpr(),
    };
    heap.push_root(*datum);
    Ok(Typed { node: Node::Quote { datum: *datum }, ty, loc })
}

/// `(quasiquote template)` — `args` without the `quasiquote`.
pub(crate) fn check_quasiquote(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(template, _)] = args else {
        return Err(Error::TypeError("quasiquote: (quasiquote template)".into()));
    };
    let mut t = template_of(cx, heap, locals, *template)?;
    t.loc = loc;
    Ok(t)
}

/// The template `v`: a cons of the templates of its car and cdr, `x` for
/// `,x`, `x`'s elements spliced in for a `,@x` element, the datum itself
/// for anything else.
fn template_of(cx: &Cx, heap: &mut Heap, locals: &Locals, v: Value) -> Result<Typed, Error> {
    let sexpr = option_of_sexpr();
    let Value::Cons(_) = v else {
        heap.push_root(v);
        return Ok(Typed { node: Node::Quote { datum: v }, ty: sexpr, loc: None });
    };
    let (car, cdr) = (heap.car(v)?, heap.cdr(v)?);
    if matches!(car, Value::Symbol(s) if s.is(wk::UNQUOTE)) {
        return match heap.list_to_vec(cdr).as_deref() {
            Ok([x]) => check_expr(cx, heap, locals, *x, Some(&sexpr), None),
            _ => Err(Error::TypeError("unquote: (unquote datum)".into())),
        };
    }
    if let Value::Cons(_) = car {
        if matches!(heap.car(car)?, Value::Symbol(s) if s.is(wk::UNQUOTE_SPLICING)) {
            let spliced = match heap.list_to_vec(heap.cdr(car)?).as_deref() {
                Ok([x]) => *x,
                _ => return Err(Error::TypeError("unquote-splicing: (unquote-splicing datum)".into())),
            };
            let spliced = check_expr(cx, heap, locals, spliced, Some(&sexpr), None)?;
            let rest = template_of(cx, heap, locals, cdr)?;
            let append = heap.names().read(NsId::STD, "sexpr-append");
            let Some((path, ValueDef::Fn(_))) = value_named(cx.reg, NsId::STD, append) else {
                return Err(Error::TypeError("unquote-splicing (,@) requires the prelude's `sexpr-append` to be loaded".into()));
            };
            return Ok(Typed { node: Node::Call { path, targs: Vec::new(), args: vec![spliced, rest] }, ty: sexpr, loc: None });
        }
    }
    let car = template_of(cx, heap, locals, car)?;
    let cdr = template_of(cx, heap, locals, cdr)?;
    let owner = Path::builtin(typelisp_mem::wk::SEXPR);
    let field_tys = cx.reg.type_def(&owner).expect("`Sexpr` is defined").variants[CONS].fields.clone();
    let node = Node::Construct { owner, targs: Vec::new(), variant: CONS, mutable: false, field_tys, args: vec![car, cdr] };
    Ok(Typed { node, ty: sexpr, loc: None })
}
