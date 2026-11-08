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
use typelisp_mem::SymRef;

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
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: None };
    let params = parse_params(heap, &tcx, *params)?;
    let ret = parse_type(heap, &tcx, *ret)?;
    let def = Method { owner, method, instance: true, public: false, home: Some(Path::of_symbol(ns, name)), params, ret };
    let form = define_method(reg, ns, heap, requests, &scope, None, def, body)?;
    let forms: Vec<Value> = form.into_iter().collect();
    crate::check::forms::module_form(heap, &owner, &forms)
}

/// A method about to be defined: whose it is, its parameters (the receiver
/// first, for an instance method) and its result type.
pub(crate) struct Method {
    pub(crate) owner: Path,
    pub(crate) method: SymRef,
    pub(crate) instance: bool,
    pub(crate) public: bool,
    /// The module a `defmethod` outside an `impl` was written in.
    pub(crate) home: Option<Path>,
    pub(crate) params: Vec<(SymRef, Type)>,
    pub(crate) ret: Type,
}

/// Register `def` with the type parameters `scope` declares, check `body`,
/// and return the method's code, rooted — or `None` when it is generic and
/// kept to be specialized where it is called. `self_type` is what `Self`
/// stands for in the body.
///
/// The method is registered before its body is checked, so the body can
/// call it. The caller undoes the definition if checking fails.
#[allow(clippy::too_many_arguments)]
pub(crate) fn define_method(
    reg: &mut Registry,
    ns: NsId,
    heap: &mut Heap,
    requests: &mut Requests,
    scope: &TypeScope,
    self_type: Option<&Type>,
    def: Method,
    body: &[(Value, Option<Loc>)],
) -> Result<Option<Value>, Error> {
    let Method { owner, method, instance, public, home, params, ret } = def;
    let type_params = scope.params().to_vec();
    // A receiver that names every type argument outright would make the
    // method that instantiation's own.
    if instance && type_params.is_empty() && matches!(&params[0].1, Type::Named(_, args) if !args.is_empty()) {
        unimplemented!("typeck: a method written for one instantiation of a generic type")
    }
    let sig = FnDef { public, ..FnDef::plain(type_params.clone(), params.iter().map(|(_, t)| t.clone()).collect(), ret.clone()) };
    reg.define_method(owner, method, MethodDef { sig, instance, home, inst_args: None });

    let mut body = {
        let cx = Cx::new(reg, ns, scope, self_type);
        let locals_root = Locals::empty();
        let locals = locals_root.bind(params.clone())?;
        let (mut body, _) = check_body(&cx, heap, &locals, body, Some(&ret))?;
        cx.settle(heap, &mut body)?;
        body
    };
    if !type_params.is_empty() {
        for t in &body {
            t.each_value(&mut |v| heap.push_permanent_root(v));
        }
        reg.set_template(Base::Method(owner, method), Some(Template { params, ret, body: std::mem::take(&mut body) }));
        return Ok(None);
    }
    reg.set_template(Base::Method(owner, method), None);
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let forms = lower_all(heap, &mut lw, &body)?;
    let form = reg.defmethod_form(heap, &owner, method, instance, &params, &ret, &forms)?;
    heap.push_root(form);
    Ok(Some(form))
}

/// The type a receiver's written type belongs to: its head names it.
pub(crate) fn receiver_type_path(heap: &Heap, reg: &Registry, ns: NsId, written: Value) -> Result<Path, Error> {
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
