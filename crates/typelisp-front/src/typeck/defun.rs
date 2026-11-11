//! `defun`: defining a function.
//!
//! A generic function's body is checked once, with its type parameters
//! abstract (`docs/dev/checker-rebuild-plan.md` 5.4), and kept to be
//! specialized for each instantiation a call asks for ([`super::generic`]).
//! The generic function itself has no code: its definition lowers to an
//! empty `(module PATH)`, the form that defines a name and runs nothing.

use crate::check::lower::CoreForms;
use crate::check::registry::TraitBound;
use crate::types::{ParamOwner, Path, Type, TypeParam};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::Cx;
use super::bounds::parse_where;
use super::generic::{Base, Requests};
use super::lambda_list::{bindings, parse_lambda_list, LambdaList, Written};
use super::block::check_named_body;
use super::locals::Locals;
use super::lower::{lower_rooted, Lowering};
use super::define::{Definer, Kind};
use super::registry::{Definition, FnDef, Registry, Template};
use super::scope::TypeScope;
use super::subst::Subst;
use super::tree::Typed;
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "defun: (defun name<T...>? ((param Type)...) ReturnType body...)";

/// `(defun name ((param Type)...) Ret body...)` — `parts` without the
/// `defun` — and its core form, unrooted.
///
/// The function is defined before its body is checked, so the body can call
/// it. The caller undoes the definition if checking fails.
pub(crate) fn check_defun(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let [(header, _), (params, _), (ret, ret_at), body @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let (name, type_names) = parse_header(heap, *header)?;
    let ns = d.site.ns;
    let name = d.ownable(heap, name, "defun")?;
    let path = Path::of_symbol(ns, name);
    // The definition a `defsignature` announced is not a redefinition.
    let declared = match reg.predeclared.get(&path) {
        Some(()) => {
            reg.predeclared.remove(&path);
            reg.fn_def(&path).cloned()
        }
        None => {
            d.value(reg, Kind::Function, name)?;
            None
        }
    };
    let (clause, body) = split_where(heap, body)?;
    let (doc, body) = split_doc(heap, body);
    d.record(reg, Definition::Fn(path), doc);
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Def(path), &type_names).map_err(|twice| {
        Error::TypeError(format!("defun {}: type parameter `{}` is declared twice", path, twice.0.name()))
    })?;
    let named = format!("defun {}", path.last_segment());
    let at = Written { what: "defun", named: &named, owner: ParamOwner::Def(path) };
    let list = parse_lambda_list(reg, d, heap, requests, &scope, None, &at, &heap.list_to_vec(*params)?)?;
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: None, notes: d.notes };
    let ret = parse_type(heap, &tcx, *ret, ret_at.as_ref())?;
    let bounds = match clause {
        Some(clause) => parse_where(heap, &tcx, clause)?,
        None => Vec::new(),
    };
    let bounded = scope.bound(bounds.clone());
    let type_params = scope.params().to_vec();
    let LambdaList { required, optionals, rest, keys, bound: params, locs } = list;
    let def = FnDef {
        public: d.public,
        rest: rest.map(|(_, t)| t),
        optionals,
        keys,
        bounds: by_param(bounds),
        ..FnDef::plain(type_params.clone(), required.into_iter().map(|(_, t)| t).collect(), ret.clone())
    };
    if let Some(declared) = &declared {
        super::defsignature::agrees(name.name(), declared, &def)?;
    }
    reg.define_fn(path, def);

    let mut body = {
        let cx = Cx::new(reg, d.expander, ns, &bounded, None, d.notes);
        let locals_root = Locals::empty();
        let locals = locals_root.bind(bindings(&params, &locs))?;
        let mut body = check_named_body(&cx, heap, &locals, name, body, &ret)?;
        cx.settle(heap, &mut body)?;
        body
    };

    if !type_params.is_empty() {
        // The body is kept past this form, so the values it holds are too.
        for t in &body {
            t.each_value(&mut |v| heap.push_permanent_root(v));
        }
        reg.set_template(Base::Fn(path), Some(Template { params, ret, body: std::mem::take(&mut body) }));
        return crate::check::forms::module_form(heap, &path, &[]);
    }
    reg.set_template(Base::Fn(path), None);
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let forms = lower_all(heap, &mut lw, &body)?;
    reg.defun_form(heap, &path, &params, &ret, &forms)
}

/// What follows a definition's result type: its `(:where ...)` clause, when
/// it has one, and the rest.
pub(crate) fn split_where<'b>(heap: &Heap, body: &'b [(Value, Option<Loc>)]) -> Result<(Option<Value>, &'b [(Value, Option<Loc>)]), Error> {
    match body.split_first() {
        Some(((first, _), rest)) if crate::check::analyze::is_where_clause(heap, *first)? => Ok((Some(*first), rest)),
        _ => Ok((None, body)),
    }
}

/// `bounds`, each parameter's together, in the order the parameters are
/// first bounded.
pub(crate) fn by_param(bounds: Vec<(TypeParam, TraitBound)>) -> Vec<(TypeParam, Vec<TraitBound>)> {
    let mut out: Vec<(TypeParam, Vec<TraitBound>)> = Vec::new();
    for (p, b) in bounds {
        match out.iter_mut().find(|(q, _)| *q == p) {
            Some((_, bs)) => bs.push(b),
            None => out.push((p, vec![b])),
        }
    }
    out
}

/// `body` lowered in order, each form rooted.
pub(crate) fn lower_all(heap: &mut Heap, lw: &mut Lowering, body: &[Typed]) -> Result<Vec<Value>, Error> {
    body.iter().map(|t| lower_rooted(heap, lw, t)).collect()
}

/// A documentation string written first in `items` with something after
/// it, as the definition forms take one, and the items after it.
pub(crate) fn split_doc<'a>(heap: &Heap, items: &'a [(Value, Option<Loc>)]) -> (Option<String>, &'a [(Value, Option<Loc>)]) {
    match items {
        [(Value::Str(id), _), rest @ ..] if !rest.is_empty() => (Some(heap.string(*id).to_string()), rest),
        _ => (None, items),
    }
}

/// The generic body `template` at `lw`'s substitution: its parameters and
/// result type, and its body lowered.
pub(crate) fn lower_template(heap: &mut Heap, lw: &mut Lowering, template: &Template) -> Result<(Vec<(SymRef, Type)>, Type, Vec<Value>), Error> {
    let params = template.params.iter().map(|(n, t)| (*n, lw.settle(t))).collect();
    let ret = lw.settle(&template.ret);
    let body = lower_all(heap, lw, &template.body)?;
    Ok((params, ret, body))
}

/// `name` or `name<T...>` (read as `(%type-apply name T...)`): the name and
/// the type parameters it declares.
pub(crate) fn parse_header(heap: &Heap, v: Value) -> Result<(SymRef, Vec<SymRef>), Error> {
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
        _ => Err(Error::TypeError("a definition name must be a symbol (write `name<T>` for type parameters)".into())),
    }
}

/// A required parameter `(param Type)`, which no marker may be.
pub(crate) fn parse_param(heap: &Heap, cx: &TypeCx, p: Value) -> Result<(SymRef, Type), Error> {
    if let Value::Symbol(id) = p {
        if id.name().starts_with('&') {
            unimplemented!("typeck: the `{}` parameter marker", id.name())
        }
    }
    let pair = heap.list_to_vec_locs(p).map_err(|_| Error::TypeError(SHAPE.into()))?;
    let [(Value::Symbol(name), _), (ty, ty_at)] = pair.as_slice() else {
        return Err(Error::TypeError(format!("defun: a parameter is (name Type): {}", SHAPE)));
    };
    Ok((*name, parse_type(heap, cx, *ty, ty_at.as_ref())?))
}

#[cfg(test)]
mod header_tests {
    use super::parse_header;
    use crate::{Heap, Reader};

    fn header(src: &str) -> Result<(String, Vec<String>), crate::Error> {
        let mut h = Heap::with_capacity(4096);
        let v = Reader::new().read_all(&mut h, src).expect("reads")[0];
        let (name, params) = parse_header(&h, v)?;
        Ok((name.name().to_string(), params.iter().map(|p| p.name().to_string()).collect()))
    }

    #[test]
    fn a_plain_name_has_no_type_parameters() {
        assert_eq!(header("identity").unwrap(), ("identity".to_string(), vec![]));
        // Hyphens are ordinary name characters, not token boundaries.
        assert_eq!(header("cons-cell").unwrap(), ("cons-cell".to_string(), vec![]));
    }

    #[test]
    fn an_angle_bracket_header_splits_into_name_and_params() {
        assert_eq!(header("pair<a,b>").unwrap(), ("pair".to_string(), vec!["a".to_string(), "b".to_string()]));
        assert_eq!(header("identity<t>").unwrap(), ("identity".to_string(), vec!["t".to_string()]));
    }

    #[test]
    fn every_cl_comparison_operator_is_a_plain_name() {
        // `char<`/`string<=`/...: a `<` with no type after it is part of the
        // name.
        for op in ["<", ">", "<=", ">=", "char<", "char>", "char<=", "char>=", "string<", "string>", "string<=", "string>=", "->", "+"] {
            assert_eq!(header(&format!("{} ", op)).unwrap(), (op.to_string(), vec![]), "operator {:?}", op);
        }
    }

    #[test]
    fn a_type_parameter_must_be_a_plain_name() {
        assert!(header("pair<Vector<t>>").is_err());
    }
}
