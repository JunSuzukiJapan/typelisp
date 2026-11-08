//! `defmethod` outside an `impl`: a method of its first parameter's type.
//!
//! `(defmethod name<T...>? ((recv Type) param...) Ret body...)`. The type
//! parameters are those the name declares, and nothing else: the receiver's
//! type uses them like any other parameter's type
//! (`docs/dev/checker-rebuild-plan.md` 7.3). `(defmethod pair-first<T,U> ((p
//! Pair<T,U>)) T ...)` declares `T` and `U`; `Pair` declared its own, which
//! are other parameters, whatever they are called.

use crate::check::lower::CoreForms;
use crate::types::{is_type_application, prim_type_path, scalar_of_symbol, ParamOwner, Path, Type};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::{member_symbol, NsId};

use super::defun::{lower_all, parse_header, parse_params};
use super::expr::{check_body, Cx};
use super::generic::{Base, Requests};
use super::locals::Locals;
use super::lower::Lowering;
use super::registry::{FnDef, MethodDef, Registry, Template, TypeEntry};
use super::resolve::find_named;
use super::scope::TypeScope;
use super::subst::Subst;
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "defmethod: (defmethod name<T...>? ((x Type) params...) ReturnType body...)";

/// `(defmethod ...)` — `parts` without the `defmethod` — and its core form,
/// unrooted. The caller undoes the definition if checking fails.
pub(crate) fn check_defmethod(
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
        unimplemented!("typeck: a `defmethod` with a documentation string")
    }
    let written: Vec<Value> = heap.list_to_vec(*params)?;
    let receiver = written.first().and_then(|r| heap.list_to_vec(*r).ok()).filter(|r| r.len() == 2 && matches!(r[0], Value::Symbol(_)));
    let Some(receiver) = receiver.filter(|_| !is_type_application(heap, written[0])) else {
        return Err(Error::TypeError(format!(
            "defmethod `{}`: the first parameter is the receiver, written (name Type): {}",
            name.name(),
            SHAPE
        )));
    };
    if matches!(receiver[0], Value::Symbol(id) if id.is(crate::wk::SELF)) {
        return Err(Error::TypeError(format!(
            "defmethod `{}`: `self` is written only inside an `impl`; name the receiver otherwise: ((x Type) ...)",
            name.name()
        )));
    }
    let owner = receiver_type_path(heap, reg, ns, receiver[1])?;
    let method = member_symbol(name.name());
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Method(owner, method), &type_names).map_err(|twice| {
        Error::TypeError(format!("defmethod {}::{}: type parameter `{}` is declared twice", owner, method.name(), twice.0.name()))
    })?;
    let tcx = TypeCx { reg, ns, scope: &scope };
    let params = parse_params(heap, &tcx, *params)?;
    let ret = parse_type(heap, &tcx, *ret)?;
    let type_params = scope.params().to_vec();
    // A receiver that names every type argument outright makes the method
    // that instantiation's own.
    let inst_args = match &params[0].1 {
        Type::Named(_, args) if !args.is_empty() && type_params.is_empty() => Some(args.clone()),
        _ => None,
    };
    if inst_args.is_some() {
        unimplemented!("typeck: a method written for one instantiation of a generic type")
    }
    let sig = FnDef::plain(type_params.clone(), params.iter().map(|(_, t)| t.clone()).collect(), ret.clone());
    reg.methods.insert((owner, method), MethodDef { sig, instance: true, home: Some(Path::of_symbol(ns, name)), inst_args: None });

    let mut body = {
        let cx = Cx::new(reg, ns, &scope);
        let locals_root = Locals::empty();
        let locals = locals_root.bind(params.clone())?;
        let (mut body, _) = check_body(&cx, heap, &locals, body, Some(&ret))?;
        for t in body.iter_mut() {
            t.map_types(&|ty| cx.resolve(ty));
        }
        body
    };
    if !type_params.is_empty() {
        for t in &body {
            t.each_value(&mut |v| heap.push_permanent_root(v));
        }
        reg.set_template(Base::Method(owner, method), Some(Template { params, ret, body: std::mem::take(&mut body) }));
        return crate::check::forms::module_form(heap, &owner, &[]);
    }
    reg.set_template(Base::Method(owner, method), None);
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let forms = lower_all(heap, &mut lw, &body)?;
    let form = reg.defmethod_form(heap, &owner, method, true, &params, &ret, &forms)?;
    heap.push_root(form);
    crate::check::forms::module_form(heap, &owner, &[form])
}

/// The type a receiver's written type belongs to: its head names it.
fn receiver_type_path(heap: &Heap, reg: &Registry, ns: NsId, written: Value) -> Result<Path, Error> {
    let head = match written {
        Value::Cons(_) if is_type_application(heap, written) => heap.list_to_vec(written)?.get(1).copied(),
        other => Some(other),
    };
    let Some(Value::Symbol(id)) = head else {
        unimplemented!("typeck: a `defmethod` receiver whose type is not a name")
    };
    if let Some(p) = scalar_of_symbol(id).as_ref().and_then(prim_type_path) {
        return Ok(p);
    }
    match find_named(reg, ns, id, |n, key| reg.types.get(&Path::of_symbol(n, key)).cloned()) {
        Some((path, TypeEntry::Type(_))) => Ok(path),
        _ => Err(Error::TypeError(format!("defmethod: `{}` is not a type with methods", heap.symbol_spelling(id)))),
    }
}
