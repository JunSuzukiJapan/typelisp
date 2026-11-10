//! `defenum`: a sum type.
//!
//! `(defenum Name<T...>? (Variant Type...) Variant...)`. A variant is a list
//! of its name and its fields' types, or its name alone when it has no
//! fields. The field types use the type parameters the name declares
//! (`docs/dev/checker-rebuild-plan.md` 7.3). A value is made with
//! `(Name::Variant value...)`, or with the bare variant name after a `use` of
//! the type, and taken apart with `match`.

use crate::check::lower::CoreForms;
use crate::check::registry::{AdtKind, Variant};
use crate::types::{ParamOwner, Path};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;

use super::defun::{parse_header, split_doc};
use super::define::{Definer, Kind};
use super::registry::{Doc, Registry, TypeDef, TypeEntry};
use super::scope::TypeScope;
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "defenum: (defenum Name<T...>? (Variant Type...) Variant...)";

/// `(defenum ...)` — `parts` without the `defenum` — and its core form,
/// unrooted. The caller undoes the definition if checking fails.
pub(crate) fn check_defenum(reg: &mut Registry, d: &mut Definer, heap: &mut Heap, parts: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    let [(header, _), items @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let (doc, items) = split_doc(heap, items);
    let (name, type_names) = parse_header(heap, *header)?;
    let ns = d.site.ns;
    d.ownable(heap, name, "defenum")?;
    d.type_name(reg, Kind::Type, name)?;
    let path = Path::of_symbol(ns, name);
    if let Some(doc) = doc {
        reg.docs.insert(Doc::Type(path), doc);
    }
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Def(path), &type_names).map_err(|twice| {
        Error::TypeError(format!("defenum {}: type parameter `{}` is declared twice", path, twice.0.name()))
    })?;
    // Registered before the variants are read, so a field can name the type
    // it belongs to (a tree's node holds trees).
    let stub = TypeDef {
        kind: AdtKind::Sum,
        params: scope.params().to_vec(),
        public: d.public,
        builtin: false,
        stdlib: crate::types::in_std(ns),
        fields: Vec::new(),
        variants: Vec::new(),
    };
    reg.types.insert(path, TypeEntry::Type(stub.clone()));
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: None };
    let mut variants: Vec<Variant> = Vec::new();
    for (item, _) in items {
        let variant = parse_variant(heap, &tcx, *item)?;
        if variants.iter().any(|v| v.name == variant.name) {
            return Err(Error::TypeError(format!("defenum: duplicate variant `{}`", variant.name)));
        }
        variants.push(variant);
    }
    if variants.is_empty() {
        return Err(Error::TypeError("defenum: needs at least one variant".into()));
    }
    reg.types.insert(path, TypeEntry::Type(TypeDef { variants: variants.clone(), ..stub }));
    reg.defenum_form(heap, &path, &[], &variants)
}

/// `(Variant Type...)` or a bare `Variant`.
fn parse_variant(heap: &Heap, cx: &TypeCx, item: Value) -> Result<Variant, Error> {
    let (name, fields) = match item {
        Value::Symbol(id) => (id, Vec::new()),
        Value::Cons(_) => match heap.list_to_vec(item)?.split_first() {
            Some((Value::Symbol(id), fields)) => (*id, fields.to_vec()),
            _ => return Err(Error::TypeError("defenum: variant name must be a symbol".into())),
        },
        _ => return Err(Error::TypeError("defenum: variant must be (Name Type...) or a bare Name".into())),
    };
    let fields = fields.iter().map(|t| parse_type(heap, cx, *t)).collect::<Result<Vec<_>, _>>()?;
    for f in &fields {
        crate::check::Checker::reject_raw_word_storage(f, "the variant", name.name())?;
    }
    Ok(Variant { name: member_symbol(name.name()), fields })
}
