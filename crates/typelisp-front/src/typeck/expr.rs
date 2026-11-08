//! Checking an expression: which kind of form it is, and the module that
//! checks that kind.

use std::cell::RefCell;

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;

use super::literal::literal;
use super::locals::Locals;
use super::registry::Registry;
use super::tree::{Node, Typed};
use super::unify::Holes;
use super::{binding, call, control};

/// What every expression of one top-level form is checked against: the
/// definitions, the module the form is in, and the form's inference holes.
pub(crate) struct Cx<'a> {
    pub(crate) reg: &'a Registry,
    pub(crate) ns: NsId,
    pub(crate) holes: RefCell<Holes>,
}

impl<'a> Cx<'a> {
    pub(crate) fn new(reg: &'a Registry, ns: NsId) -> Cx<'a> {
        Cx { reg, ns, holes: RefCell::new(Holes::new()) }
    }

    /// `ty` with every hole solved so far filled in.
    pub(crate) fn resolve(&self, ty: &Type) -> Type {
        self.holes.borrow().resolve(ty)
    }
}

/// `v` checked where `expected` is expected (`None`: anything), with `locals`
/// bound. `loc` is where `v` was written, for an atom, which carries no
/// position of its own.
///
/// A value this allocates is rooted on `heap`'s root stack; the top-level
/// form's checking releases it after the tree is lowered.
pub(crate) fn check_expr(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    v: Value,
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let loc = heap.cons_loc(v).or(loc);
    let typed = check_form(cx, heap, locals, v, expected, loc.clone()).and_then(|t| {
        agree(cx, &t.ty, expected)?;
        Ok(t)
    });
    typed.map_err(|e| match &loc {
        Some(l) => e.at(l.clone()),
        None => e,
    })
}

fn check_form(cx: &Cx, heap: &mut Heap, locals: &Locals, v: Value, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    // A literal's type depends on what is expected, which may be a hole an
    // earlier argument solved.
    let solved = expected.map(|e| cx.resolve(e));
    if let Some(lit) = literal(heap, v, solved.as_ref())? {
        if let Some(value) = lit.value {
            heap.push_root(value);
        }
        return Ok(Typed { node: Node::Literal { tag: lit.tag, value: lit.value }, ty: lit.ty, loc });
    }
    match v {
        Value::Symbol(id) => match locals.lookup(id) {
            Some(ty) => Ok(Typed { node: Node::Local { name: id }, ty: ty.clone(), loc }),
            None => unimplemented!("typeck: the value of `{}` (only local variables are read so far)", heap.symbol_spelling(id)),
        },
        Value::Cons(_) => check_list(cx, heap, locals, v, expected, loc),
        other => unimplemented!("typeck: checking {:?} (only literals, locals and lists are checked so far)", other),
    }
}

/// A list form: a special form, or a call.
fn check_list(cx: &Cx, heap: &mut Heap, locals: &Locals, v: Value, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    let Value::Symbol(head) = elems[0].0 else {
        unimplemented!("typeck: a list whose head is not a symbol (only named calls are checked so far)")
    };
    let args = &elems[1..];
    match head.well_known() {
        wk::LET => return binding::check_let(cx, heap, locals, args, expected, loc),
        wk::PROGN => return binding::check_progn(cx, heap, locals, args, expected, loc),
        wk::IF => return control::check_if(cx, heap, locals, args, expected, loc),
        _ => {}
    }
    if locals.lookup(head).is_some() {
        unimplemented!("typeck: calling the local `{}` (only module functions are called so far)", heap.symbol_spelling(head))
    }
    let path = Path::of_symbol(cx.ns, head);
    match cx.reg.fn_def(&path) {
        Some(def) => call::check_call(cx, heap, locals, path, def, args, expected, loc),
        None => unimplemented!("typeck: calling `{}` (only functions this checker defined are called so far)", heap.symbol_spelling(head)),
    }
}

/// Whether a value of type `found` may be where `expected` is expected,
/// solving holes to make it so. `!` is every type.
pub(crate) fn agree(cx: &Cx, found: &Type, expected: Option<&Type>) -> Result<(), Error> {
    let Some(e) = expected else { return Ok(()) };
    cx.holes.borrow_mut().unify(e, found).map_err(|m| {
        Error::TypeError(format!("type mismatch: expected `{}`, found `{}`", m.left, m.right))
    })
}

/// `forms` checked in order, the last where `expected` is expected: the
/// body of a `let`, a `progn` or a function. Its type is the last form's, or
/// `()` when there is none.
pub(crate) fn check_body(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    forms: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
) -> Result<(Vec<Typed>, Type), Error> {
    let mut out = Vec::with_capacity(forms.len());
    for (i, (v, loc)) in forms.iter().enumerate() {
        let last = i + 1 == forms.len();
        out.push(check_expr(cx, heap, locals, *v, if last { expected } else { None }, loc.clone())?);
    }
    let ty = match out.last() {
        Some(t) => t.ty.clone(),
        None => {
            agree(cx, &Type::Unit, expected)?;
            Type::Unit
        }
    };
    Ok((out, ty))
}
