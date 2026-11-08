//! `defun`: defining a function.

use crate::check::lower::CoreForms;
use crate::types::{Path, Type};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::expr::{check_body, Cx};
use super::locals::Locals;
use super::lower::lower;
use super::registry::{FnDef, Registry};
use super::type_syntax::parse_type;

const SHAPE: &str = "defun: (defun name ((param Type)...) ReturnType body...)";

/// `(defun name ((param Type)...) Ret body...)` — `parts` without the
/// `defun` — and its core form, unrooted.
///
/// The function is defined before its body is checked, so the body can call
/// it. The caller undoes the definition if checking fails.
pub(crate) fn check_defun(reg: &mut Registry, ns: NsId, heap: &mut Heap, parts: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    let [(name, _), (params, _), (ret, _), body @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let Value::Symbol(name) = *name else {
        unimplemented!("typeck: a generic `defun` (only plain names are defined so far)")
    };
    if matches!(body, [(Value::Str(_), _), _, ..]) {
        unimplemented!("typeck: a `defun` with a documentation string")
    }
    let params = parse_params(heap, *params)?;
    let ret = parse_type(heap, *ret)?;
    let path = Path::of_symbol(ns, name);
    reg.define_fn(path, FnDef { params: params.iter().map(|(_, t)| t.clone()).collect(), ret: ret.clone() });
    let cx = Cx { reg, ns };
    let root = Locals::empty();
    let locals = root.bind(params.clone())?;
    let (body, _) = check_body(&cx, heap, &locals, body, Some(&ret))?;
    let mut forms = Vec::with_capacity(body.len());
    for t in &body {
        let form = lower(heap, reg, t)?;
        heap.push_root(form);
        forms.push(form);
    }
    reg.defun_form(heap, &path, &params, &ret, &forms)
}

/// `((param Type)...)`.
fn parse_params(heap: &Heap, v: Value) -> Result<Vec<(SymRef, Type)>, Error> {
    let mut out = Vec::new();
    for (p, _) in heap.list_to_vec_locs(v)? {
        match p {
            Value::Symbol(id) if id.name().starts_with('&') => {
                unimplemented!("typeck: the `{}` parameter marker", id.name())
            }
            _ => {}
        }
        let pair = heap.list_to_vec(p).map_err(|_| Error::TypeError(SHAPE.into()))?;
        let [Value::Symbol(name), ty] = pair.as_slice() else {
            return Err(Error::TypeError(format!("defun: a parameter is (name Type): {}", SHAPE)));
        };
        out.push((*name, parse_type(heap, *ty)?));
    }
    Ok(out)
}
