//! `(defsignature name (Type...) Ret)`: a function declared before it is
//! defined, so the forms before its `defun` can call it (forms are checked
//! one at a time, in order).
//!
//! The `defun` that follows must agree with the declaration, and one that
//! never follows is reported when the unit is finished. A declaration has no
//! body, so it declares no type parameters (a generic function is
//! specialized from its body) and no `&optional` or `&key` (their defaults
//! are checked forms the declaration has nowhere to write).

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};

use super::define::{Definer, Kind};
use super::defun::parse_header;
use super::registry::{FnDef, Registry, ValueDef};
use super::scope::TypeScope;
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "defsignature: (defsignature name (param-type...) return-type)";

/// `(defsignature ...)` — `parts` without the `defsignature` — and its core
/// form, which runs nothing.
pub(crate) fn check_defsignature(reg: &mut Registry, d: &mut Definer, heap: &mut Heap, parts: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    let [(header, _), (params, _), (ret, _)] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let (name, type_names) = parse_header(heap, *header)?;
    let name = d.ownable(heap, name, "defsignature")?;
    if !type_names.is_empty() {
        return Err(Error::TypeError(format!(
            "defsignature: `{}` takes type parameters, and a generic function cannot be forward-declared — \
             instantiating one needs its body, which a declaration does not have. Define it before its callers.",
            name.name()
        )));
    }
    let path = Path::of_symbol(d.site.ns, name);
    if reg.predeclared.get(&path).is_some() {
        return Err(Error::TypeError(format!("defsignature: `{}` is already declared", name.name())));
    }
    // A declaration after the definition could never do anything.
    match reg.values.get(&path) {
        Some(ValueDef::Fn(def)) if !def.builtin => {
            return Err(Error::TypeError(format!(
                "defsignature: `{}` is already defined — a declaration has to come before the definition it announces",
                name.name()
            )))
        }
        _ => d.value(reg, Kind::Function, name)?,
    }
    let root = TypeScope::empty();
    let tcx = TypeCx { reg, ns: d.site.ns, scope: &root, self_type: None };
    let (params, rest) = signature_params(heap, &tcx, *params)?;
    let ret = parse_type(heap, &tcx, *ret)?;
    let def = FnDef { public: d.public, rest, ..FnDef::plain(Vec::new(), params, ret) };
    reg.define_fn(path, def);
    reg.predeclared.insert(path, ());
    crate::check::forms::defsignature_form(heap)
}

/// The parameter types of a declaration, and its `&rest` one.
fn signature_params(heap: &Heap, tcx: &TypeCx, v: Value) -> Result<(Vec<Type>, Option<Type>), Error> {
    let written = heap.list_to_vec(v)?;
    if written.iter().any(|p| matches!(p, Value::Symbol(s) if s.is(wk::OPTIONAL) || s.is(wk::KEY))) {
        return Err(Error::TypeError(
            "defsignature: `&optional`/`&key` cannot be forward-declared — a declaration has nowhere to put \
             their default expressions, which call sites splice in verbatim. Define such a function before its callers."
                .into(),
        ));
    }
    let (fixed, rest) = match written.iter().position(|p| matches!(p, Value::Symbol(s) if s.is(wk::REST))) {
        Some(i) if i + 2 != written.len() => {
            return Err(Error::TypeError("&rest must be followed by exactly one type, as the last item in the parameter list".into()));
        }
        Some(i) => (&written[..i], Some(parse_type(heap, tcx, written[i + 1])?)),
        None => (&written[..], None),
    };
    let fixed = fixed.iter().map(|t| parse_type(heap, tcx, *t)).collect::<Result<Vec<_>, _>>()?;
    Ok((fixed, rest))
}

/// Whether the function `name`, defined as `defined`, is what its
/// declaration `declared` said.
pub(crate) fn agrees(name: &str, declared: &FnDef, defined: &FnDef) -> Result<(), Error> {
    let mismatch = |what: &str, decl: String, def: String| {
        Err(Error::TypeError(format!("`{}` does not match its `defsignature`: {} declared as {}, defined as {}", name, what, decl, def)))
    };
    let show_rest = |r: &Option<Type>| r.as_ref().map_or("none".to_string(), |t| format!("`&rest {}`", t));
    if declared.params.len() != defined.params.len() {
        return mismatch("parameter count", declared.params.len().to_string(), defined.params.len().to_string());
    }
    for (i, (d, f)) in declared.params.iter().zip(&defined.params).enumerate() {
        if d != f {
            return mismatch(&format!("parameter {}", i + 1), format!("`{}`", d), format!("`{}`", f));
        }
    }
    if declared.ret != defined.ret {
        return mismatch("return type", format!("`{}`", declared.ret), format!("`{}`", defined.ret));
    }
    if declared.rest != defined.rest {
        return mismatch("`&rest`", show_rest(&declared.rest), show_rest(&defined.rest));
    }
    if !defined.type_params.is_empty() {
        let names: Vec<String> = defined.type_params.iter().map(|p| p.name.name().to_string()).collect();
        return mismatch("type parameters", "none".to_string(), format!("{:?}", names));
    }
    if !defined.optionals.is_empty() || !defined.keys.is_empty() {
        return mismatch("parameter kinds", "required only".to_string(), "`&optional`/`&key`".to_string());
    }
    if declared.public != defined.public {
        let vis = |p: bool| if p { "`pub`".to_string() } else { "private".to_string() };
        return mismatch("visibility", vis(declared.public), vis(defined.public));
    }
    Ok(())
}
