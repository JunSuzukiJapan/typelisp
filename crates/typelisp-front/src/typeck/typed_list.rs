//! A list built where a list type is expected: `(list a b)`, `'(1 "a")` or
//! `` `(,a ,b) `` where `(T U)` — `cons-cell<T,cons-cell<U,()>>` — is what
//! is wanted builds those cells, each element checked against its own type.
//! Anywhere else the same forms build S-expression data (`list_form`,
//! `quote`).
//!
//! The expected type is walked one cell at a time, so its length is the
//! list's: an element where it has none, or an end where it has one, is an
//! error. A cell whose cdr is S-expression data (`(int . Option<Sexpr>)`)
//! takes the rest of a template as data.

use crate::check::forms::list_from_vec_locs;
use crate::types::{cons_cell_parts, cons_cell_path, Type};
use crate::{wk, Error, Heap, Loc, SymRef, Value};

use super::collection_literal::literal_element_form;
use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::sexpr_value::is_sexpr_expectation;
use super::tree::{Node, Typed};

/// The car and cdr types of the cell `expected` asks for, when it asks for
/// one: what decides that a list form builds cells.
pub(crate) fn expected_cell(cx: &Cx, expected: Option<&Type>) -> Option<(Type, Type)> {
    let ty = cx.resolve(expected?);
    cons_cell_parts(&ty).map(|(car, cdr)| (car.clone(), cdr.clone()))
}

/// `(list item...)` where `expected`, a list type, is expected.
pub(crate) fn check_list(cx: &Cx, heap: &mut Heap, locals: &Locals, items: &[(Value, Option<Loc>)], expected: &Type, loc: Option<Loc>) -> Result<Typed, Error> {
    let mut cells = Cells::new(expected);
    for (item, item_loc) in items {
        let (car, _) = cells.next_cell(cx)?;
        let elem = check_expr(cx, heap, locals, *item, Some(&car), item_loc.clone())?;
        cells.push(Step::Elem(elem));
    }
    let tail = cells.end(cx, heap, locals)?;
    cells.finish(cx, tail, loc)
}

/// The quoted list `datum` (`quasi` false) or the quasiquoted template
/// `datum` (`quasi` true) where `expected`, a list type, is expected.
pub(crate) fn check_template(cx: &Cx, heap: &mut Heap, locals: &Locals, datum: Value, quasi: bool, expected: &Type, loc: Option<Loc>) -> Result<Typed, Error> {
    let mut cells = Cells::new(expected);
    let mut cur = datum;
    let tail = loop {
        let Value::Cons(cell) = cur else {
            break match cur {
                Value::Empty => cells.end(cx, heap, locals)?,
                // `(a . b)`: what `b` is, quoted.
                atom => {
                    let form = literal_element_form(heap, atom, false)?;
                    heap.push_root(form);
                    check_expr(cx, heap, locals, form, Some(&cells.rest), None)?
                }
            };
        };
        let (car, cdr) = (heap.car(cur)?, heap.cdr(cur)?);
        // `(a . ,b)` reads as `(a unquote b)`: the rest is `b`'s value.
        if quasi && matches!(car, Value::Symbol(s) if s.is(wk::UNQUOTE)) {
            let [x] = heap.list_to_vec(cdr)?[..] else {
                return Err(Error::TypeError("unquote: (unquote datum)".into()));
            };
            break check_expr(cx, heap, locals, x, Some(&cells.rest), None)?;
        }
        if expected_cell(cx, Some(&cells.rest)).is_none() && is_sexpr_expectation(&cx.resolve(&cells.rest)) {
            let form = wrapped(heap, quasi, cur)?;
            break check_expr(cx, heap, locals, form, Some(&cells.rest), None)?;
        }
        let at = heap.elem_loc(cell);
        if let Some(spliced) = quasi.then(|| unquoted(heap, car, wk::UNQUOTE_SPLICING)).transpose()?.flatten() {
            // `,@x` last: the list goes on as `x`, as CL's shares the last
            // list spliced.
            if cdr == Value::Empty {
                break check_expr(cx, heap, locals, spliced, Some(&cells.rest), at)?;
            }
            cells.splice(cx, heap, locals, spliced, at)?;
            cur = cdr;
            continue;
        }
        let (car_ty, _) = cells.next_cell(cx)?;
        let elem = match quasi.then(|| unquoted(heap, car, wk::UNQUOTE)).transpose()?.flatten() {
            Some(x) => check_expr(cx, heap, locals, x, Some(&car_ty), at)?,
            // A list in the template is a template of its own, of the element's type.
            None if matches!(car, Value::Cons(_)) => {
                let form = wrapped(heap, quasi, car)?;
                check_expr(cx, heap, locals, form, Some(&car_ty), at)?
            }
            None => {
                let form = literal_element_form(heap, car, false)?;
                heap.push_root(form);
                check_expr(cx, heap, locals, form, Some(&car_ty), at)?
            }
        };
        cells.push(Step::Elem(elem));
        cur = cdr;
    };
    cells.finish(cx, tail, loc)
}

/// `x` when `v` is `(marker x)`.
fn unquoted(heap: &Heap, v: Value, marker: u32) -> Result<Option<Value>, Error> {
    let Value::Cons(_) = v else { return Ok(None) };
    if !matches!(heap.car(v)?, Value::Symbol(s) if s.is(marker)) {
        return Ok(None);
    }
    match heap.list_to_vec(heap.cdr(v)?)?[..] {
        [x] => Ok(Some(x)),
        _ => {
            let name = if marker == wk::UNQUOTE { "unquote" } else { "unquote-splicing" };
            Err(Error::TypeError(format!("{}: ({} datum)", name, name)))
        }
    }
}

/// `(quasiquote v)` or `(quote v)`, rooted until the top-level form is checked.
fn wrapped(heap: &mut Heap, quasi: bool, v: Value) -> Result<Value, Error> {
    let marker = Value::Symbol(typelisp_mem::symbols::well_known_symbol(if quasi { wk::QUASIQUOTE } else { wk::QUOTE }));
    let form = list_from_vec_locs(heap, &[(marker, None), (v, None)])?;
    heap.push_root(form);
    Ok(form)
}

/// One step of a list, in the order it is evaluated.
enum Step {
    /// An element.
    Elem(Typed),
    /// A list spliced in, held in `temp`: its elements, read out of it.
    Splice { temp: SymRef, list: Typed, elems: Vec<Typed> },
}

/// The cells of a list being built: the steps so far, the type of each cell
/// they fill, and the type of the rest.
struct Cells {
    whole: Type,
    steps: Vec<Step>,
    /// Each element's cell: its car's type and its cdr's.
    cells: Vec<(Type, Type)>,
    /// The type of what comes after the cells so far.
    rest: Type,
}

impl Cells {
    fn new(expected: &Type) -> Cells {
        Cells { whole: expected.clone(), steps: Vec::new(), cells: Vec::new(), rest: expected.clone() }
    }

    /// The cell the next element fills.
    fn next_cell(&mut self, cx: &Cx) -> Result<(Type, Type), Error> {
        let Some((car, cdr)) = expected_cell(cx, Some(&self.rest)) else {
            return Err(Error::TypeError(format!(
                "this list has more elements than its type `{}` holds",
                cx.resolve(&self.whole)
            )));
        };
        self.cells.push((car.clone(), cdr.clone()));
        self.rest = cdr.clone();
        Ok((car, cdr))
    }

    fn push(&mut self, step: Step) {
        self.steps.push(step);
    }

    /// `,@x` before the end of a template: `x`'s elements, each in a cell of
    /// its own. So `x`'s type has to say how many there are.
    fn splice(&mut self, cx: &Cx, heap: &mut Heap, locals: &Locals, x: Value, at: Option<Loc>) -> Result<(), Error> {
        let list = check_expr(cx, heap, locals, x, None, at)?;
        let list_ty = cx.resolve(&list.ty);
        let temp = cx.temp();
        // The types of the cells of `x`, the first one `x`'s own.
        let mut cell_tys = Vec::new();
        let mut ty = list_ty.clone();
        while let Some((_, cdr)) = cons_cell_parts(&ty).map(|(a, d)| (a.clone(), cx.resolve(d))) {
            cell_tys.push(ty);
            ty = cdr;
        }
        if ty != Type::Unit {
            return Err(Error::TypeError(format!(
                "`,@` inside a list of the type `{}` splices in a list whose type fixes its length, ending in `()`, and this one is a `{}`",
                cx.resolve(&self.whole),
                list_ty
            )));
        }
        let mut elems = Vec::new();
        for k in 0..cell_tys.len() {
            // The cell `k` of `x`: its cdr, `k` times over.
            let mut cell = Typed { node: Node::Local { name: temp }, ty: cell_tys[0].clone(), loc: None };
            for next in &cell_tys[1..=k] {
                cell = Typed { node: Node::FieldGet { obj: Box::new(cell), index: 1 }, ty: next.clone(), loc: None };
            }
            let car_ty = cons_cell_parts(&cell_tys[k]).expect("a cell's type").0.clone();
            let get = Typed { node: Node::FieldGet { obj: Box::new(cell), index: 0 }, ty: car_ty, loc: None };
            let (car, _) = self.next_cell(cx)?;
            elems.push(super::widen::conform(cx, get, Some(&car))?);
        }
        self.steps.push(Step::Splice { temp, list, elems });
        Ok(())
    }

    /// The end of the elements: `()`, where the type has one.
    fn end(&self, cx: &Cx, heap: &mut Heap, locals: &Locals) -> Result<Typed, Error> {
        if expected_cell(cx, Some(&self.rest)).is_some() {
            return Err(Error::TypeError(format!(
                "this list has fewer elements than its type `{}` holds",
                cx.resolve(&self.whole)
            )));
        }
        check_expr(cx, heap, locals, Value::Empty, Some(&self.rest), None)
    }

    /// The cells, ending in `tail`. With a list spliced in, every step is
    /// held in a temporary first, in order, so the elements are evaluated in
    /// the order written.
    fn finish(self, cx: &Cx, tail: Typed, loc: Option<Loc>) -> Result<Typed, Error> {
        let spliced = self.steps.iter().any(|s| matches!(s, Step::Splice { .. }));
        let mut binds = Vec::new();
        let mut elems = Vec::new();
        for step in self.steps {
            match step {
                Step::Elem(elem) if spliced => {
                    let temp = cx.temp();
                    elems.push(Typed { node: Node::Local { name: temp }, ty: elem.ty.clone(), loc: None });
                    binds.push((temp, elem));
                }
                Step::Elem(elem) => elems.push(elem),
                Step::Splice { temp, list, elems: from } => {
                    binds.push((temp, list));
                    elems.extend(from);
                }
            }
        }
        let cons = cons_cell_path();
        let mut acc = tail;
        for (elem, (car, cdr)) in elems.into_iter().zip(self.cells).rev() {
            let (car, cdr) = (cx.resolve(&car), cx.resolve(&cdr));
            let ty = Type::Named(cons.clone(), vec![car.clone(), cdr.clone()]);
            let node = Node::Construct { owner: cons.clone(), targs: vec![car.clone(), cdr.clone()], variant: 0, mutable: true, field_tys: vec![car, cdr], args: vec![elem, acc] };
            acc = Typed { node, ty, loc: None };
        }
        acc.loc = loc;
        if binds.is_empty() {
            return Ok(acc);
        }
        let ty = acc.ty.clone();
        Ok(Typed { node: Node::Let { binds, body: vec![acc] }, ty, loc: None })
    }
}
