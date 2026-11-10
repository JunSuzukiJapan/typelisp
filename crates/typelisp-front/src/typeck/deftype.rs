//! `(deftype Name Type)` and `(deftype Name<T...> Type)`: a spelling of a
//! type, not a type of its own. A use of the name is the type it stands for,
//! its parameters replaced by the use's arguments, so nothing after the type
//! is read knows there was a name.

use crate::types::{ParamOwner, Path, Type};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::define::{Definer, Kind};
use super::defun::parse_header;
use super::registry::{Doc, Registry, TypeEntry};
use super::scope::TypeScope;
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "deftype: (deftype Name Type)";

/// `(deftype ...)` — `parts` without the `deftype` — and its core form,
/// which runs nothing.
pub(crate) fn check_deftype(reg: &mut Registry, d: &mut Definer, heap: &mut Heap, parts: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    let Some(((header, _), rest)) = parts.split_first() else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let (name, type_names) = parse_header(heap, *header)?;
    // A leading docstring: a type is never written as a string.
    let (doc, rest) = match rest.split_first() {
        Some(((Value::Str(id), _), body)) => (Some(heap.string(*id).to_string()), body),
        _ => (None, rest),
    };
    let [(body, _)] = rest else {
        return Err(Error::TypeError("deftype: (deftype Name Type) — exactly one type follows the name".into()));
    };
    d.ownable(heap, name, "deftype")?;
    d.type_name(reg, Kind::Alias, name)?;
    let ns = d.site.ns;
    let path = Path::of_symbol(ns, name);
    let root = TypeScope::empty();
    let scope = root
        .declare(ParamOwner::Def(path), &type_names)
        .map_err(|twice| Error::TypeError(format!("deftype: duplicate type parameter `{}`", twice.0.name())))?;
    // The name is not defined while its type is read, so it would read as no
    // type at all.
    if mentions(heap, *body, name) {
        return Err(Error::TypeError(format!(
            "deftype {}: an alias may not mention itself — it is a spelling, expanded where it is written, so there is nothing to recurse into",
            name.name()
        )));
    }
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: None };
    let ty: Type = parse_type(heap, &tcx, *body)?;
    let params = scope.params().to_vec();
    reg.types.insert(path, TypeEntry::Alias { public: d.public, params, body: ty });
    if let Some(doc) = doc {
        reg.docs.insert(Doc::Type(path), doc);
    }
    crate::check::forms::module_form(heap, &path, &[])
}

/// Whether the type written `v` names `name` anywhere in it.
fn mentions(heap: &Heap, v: Value, name: SymRef) -> bool {
    match v {
        Value::Symbol(id) => id == name,
        Value::Cons(_) => heap.list_to_vec(v).is_ok_and(|items| items.iter().any(|i| mentions(heap, *i, name))),
        Value::Boxed(id) if heap.sexpr_tuple_arity(id).is_some() => (0..heap.struct_field_count(id)).any(|i| mentions(heap, heap.struct_field(id, i), name)),
        _ => false,
    }
}
