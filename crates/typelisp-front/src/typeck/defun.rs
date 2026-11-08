//! `defun`: defining a function.
//!
//! A generic function's body is checked once, with its type parameters
//! abstract (`docs/dev/checker-rebuild-plan.md` 5.4), and kept to be
//! specialized for each instantiation a call asks for ([`super::generic`]).
//! The generic function itself has no code: its definition lowers to an
//! empty `(module PATH)`, the form that defines a name and runs nothing.

use crate::check::lower::CoreForms;
use crate::types::{ParamOwner, Path, Type};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::expr::{check_body, Cx};
use super::generic::Requests;
use super::locals::Locals;
use super::lower::{lower_rooted, Lowering};
use super::registry::{FnDef, Registry, Template};
use super::scope::TypeScope;
use super::subst::Subst;
use super::tree::Typed;
use super::type_syntax::parse_type;

const SHAPE: &str = "defun: (defun name<T...>? ((param Type)...) ReturnType body...)";

/// `(defun name ((param Type)...) Ret body...)` — `parts` without the
/// `defun` — and its core form, unrooted.
///
/// The function is defined before its body is checked, so the body can call
/// it. The caller undoes the definition if checking fails.
pub(crate) fn check_defun(
    reg: &mut Registry,
    ns: NsId,
    heap: &mut Heap,
    requests: &mut Requests,
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let [(header, _), (params, _), (ret, _), body @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let (name, type_names) = parse_header(heap, *header)?;
    if matches!(body, [(Value::Str(_), _), _, ..]) {
        unimplemented!("typeck: a `defun` with a documentation string")
    }
    let path = Path::of_symbol(ns, name);
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Def(path), &type_names).map_err(|twice| {
        Error::TypeError(format!("defun {}: type parameter `{}` is declared twice", path, twice.0.name()))
    })?;
    let params = parse_params(heap, &scope, *params)?;
    let ret = parse_type(heap, &scope, *ret)?;
    let type_params = scope.params().to_vec();
    reg.define_fn(path, FnDef { type_params: type_params.clone(), params: params.iter().map(|(_, t)| t.clone()).collect(), ret: ret.clone() });

    let mut body = {
        let cx = Cx::new(reg, ns);
        let locals_root = Locals::empty();
        let locals = locals_root.bind(params.clone())?;
        let (mut body, _) = check_body(&cx, heap, &locals, body, Some(&ret))?;
        for t in body.iter_mut() {
            t.map_types(&|ty| cx.resolve(ty));
        }
        body
    };

    if !type_params.is_empty() {
        // The body is kept past this form, so the values it holds are too.
        for t in &body {
            t.each_value(&mut |v| heap.push_permanent_root(v));
        }
        reg.set_template(path, Some(Template { params, ret, body: std::mem::take(&mut body) }));
        return crate::check::forms::module_form(heap, &path, &[]);
    }
    reg.set_template(path, None);
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let forms = lower_all(heap, &mut lw, &body)?;
    reg.defun_form(heap, &path, &params, &ret, &forms)
}

/// `body` lowered in order, each form rooted.
pub(crate) fn lower_all(heap: &mut Heap, lw: &mut Lowering, body: &[Typed]) -> Result<Vec<Value>, Error> {
    body.iter().map(|t| lower_rooted(heap, lw, t)).collect()
}

/// `name` or `name<T...>` (read as `(%type-apply name T...)`): the name and
/// the type parameters it declares.
fn parse_header(heap: &Heap, v: Value) -> Result<(SymRef, Vec<SymRef>), Error> {
    match v {
        Value::Symbol(id) => Ok((id, Vec::new())),
        Value::Cons(_) if crate::types::is_type_application(heap, v) => {
            let parts = heap.list_to_vec(v)?;
            let names: Option<Vec<SymRef>> = parts[1..].iter().map(|p| match p {
                Value::Symbol(id) => Some(*id),
                _ => None,
            }).collect();
            match names.as_deref() {
                Some([name, params @ ..]) => Ok((*name, params.to_vec())),
                _ => Err(Error::TypeError("defun: a function's name is a plain name, with its type parameters after it: name<T, U>".into())),
            }
        }
        _ => Err(Error::TypeError(SHAPE.into())),
    }
}

/// `((param Type)...)`.
fn parse_params(heap: &Heap, scope: &TypeScope, v: Value) -> Result<Vec<(SymRef, Type)>, Error> {
    let mut out = Vec::new();
    for (p, _) in heap.list_to_vec_locs(v)? {
        if let Value::Symbol(id) = p {
            if id.name().starts_with('&') {
                unimplemented!("typeck: the `{}` parameter marker", id.name())
            }
        }
        let pair = heap.list_to_vec(p).map_err(|_| Error::TypeError(SHAPE.into()))?;
        let [Value::Symbol(name), ty] = pair.as_slice() else {
            return Err(Error::TypeError(format!("defun: a parameter is (name Type): {}", SHAPE)));
        };
        out.push((*name, parse_type(heap, scope, *ty)?));
    }
    Ok(out)
}
