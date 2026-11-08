//! `defstruct`: a mutable product type, and the methods that read and write
//! its fields.
//!
//! `(defstruct Name<T...> (field Type) (pub field Type)...)`. Each field
//! `f` gets a method `f` that reads it and a method `set-f` that writes it.
//! Their type parameters are the struct's, owned by each method
//! (`docs/dev/checker-rebuild-plan.md` 7.3); for a generic struct they are
//! kept as checked bodies and specialized where they are called, like any
//! generic method.

use crate::check::lower::CoreForms;
use crate::check::registry::AdtKind;
use crate::types::{ParamOwner, Path, Type, TypeParam};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::{intern, member_symbol, NsId};
use typelisp_mem::SymRef;

use super::defun::{lower_all, parse_header};
use super::generic::{Base, Requests};
use super::lower::Lowering;
use super::registry::{Field, FnDef, MethodDef, Registry, Template, TypeDef, TypeEntry};
use super::scope::TypeScope;
use super::subst::Subst;
use super::tree::{Node, Typed};
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "defstruct: (defstruct Name<T...>? (field Type)...)";

/// `(defstruct ...)` — `parts` without the `defstruct` — and its core form,
/// unrooted: the type's definition, then its accessors.
pub(crate) fn check_defstruct(
    reg: &mut Registry,
    ns: NsId,
    heap: &mut Heap,
    requests: &mut Requests,
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let [(header, _), fields @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    if matches!(header, Value::Cons(_)) && !crate::types::is_type_application(heap, *header) {
        unimplemented!("typeck: `defstruct` options (`(defstruct (Name option...) ...)`)")
    }
    if matches!(fields.first(), Some((Value::Str(_), _))) {
        unimplemented!("typeck: a `defstruct` with a documentation string")
    }
    let (name, type_names) = parse_header(heap, *header)?;
    let path = Path::of_symbol(ns, name);
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Def(path), &type_names).map_err(|twice| {
        Error::TypeError(format!("defstruct {}: type parameter `{}` is declared twice", path, twice.0.name()))
    })?;
    let params = scope.params().to_vec();
    // Registered before the fields are read, so a field can name the type
    // it belongs to.
    let stub = TypeDef { kind: AdtKind::Struct, params: params.clone(), public: false, builtin: false, fields: Vec::new(), has_defaults: false };
    reg.types.insert(path, TypeEntry::Type(stub.clone()));
    let fields = parse_fields(heap, &TypeCx { reg, ns, scope: &scope }, fields)?;
    if fields.is_empty() {
        return Err(Error::TypeError("defstruct: needs at least one field".into()));
    }
    reg.types.insert(path, TypeEntry::Type(TypeDef { fields: fields.clone(), ..stub }));

    let mut forms = Vec::new();
    let names: Vec<String> = fields.iter().map(|f| f.name.name().to_string()).collect();
    let field_tys: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
    let def_form = reg.defstruct_form(heap, &path, &[], &field_tys, &names)?;
    heap.push_root(def_form);
    forms.push(def_form);
    for (i, field) in fields.iter().enumerate() {
        let getter = member_symbol(field.name.name());
        let setter = member_symbol(&format!("set-{}", field.name.name()));
        for (method, accessor) in [(getter, Accessor::Get), (setter, Accessor::Set)] {
            if let Some(form) = define_accessor(reg, heap, requests, path, &params, i, field, method, accessor)? {
                heap.push_root(form);
                forms.push(form);
            }
        }
    }
    crate::check::forms::module_form(heap, &path, &forms)
}

#[derive(Clone, Copy)]
enum Accessor {
    Get,
    Set,
}

/// Register the accessor `method` of the field `index` of `path` and return
/// its code, or `None` when the struct is generic and the accessor is kept
/// to be specialized.
#[allow(clippy::too_many_arguments)]
fn define_accessor(
    reg: &mut Registry,
    heap: &mut Heap,
    requests: &mut Requests,
    path: Path,
    struct_params: &[TypeParam],
    index: usize,
    field: &Field,
    method: SymRef,
    accessor: Accessor,
) -> Result<Option<Value>, Error> {
    // The struct's parameters, owned by this method.
    let owner = ParamOwner::Method(path, method);
    let own: Vec<TypeParam> = struct_params.iter().map(|p| TypeParam { owner, ..*p }).collect();
    let reown = Subst::positional(struct_params, &own.iter().map(|p| Type::Param(*p)).collect::<Vec<_>>());
    let recv_ty = Type::Named(path, own.iter().map(|p| Type::Param(*p)).collect());
    let field_ty = reown.apply(&field.ty);
    let (self_name, value_name) = (intern("self"), intern("value"));
    let local = |name: SymRef, ty: &Type| Typed { node: Node::Local { name }, ty: ty.clone(), loc: None };
    let (params, ret, body) = match accessor {
        Accessor::Get => {
            let get = Node::FieldGet { obj: Box::new(local(self_name, &recv_ty)), index };
            (vec![(self_name, recv_ty.clone())], field_ty.clone(), Typed { node: get, ty: field_ty.clone(), loc: None })
        }
        Accessor::Set => {
            let set = Node::FieldSet {
                obj: Box::new(local(self_name, &recv_ty)),
                index,
                field_ty: field_ty.clone(),
                value: Box::new(local(value_name, &field_ty)),
            };
            (vec![(self_name, recv_ty.clone()), (value_name, field_ty.clone())], Type::Unit, Typed { node: set, ty: Type::Unit, loc: None })
        }
    };
    let sig = FnDef { public: field.public, ..FnDef::plain(own.clone(), params.iter().map(|(_, t)| t.clone()).collect(), ret.clone()) };
    reg.methods.insert((path, method), MethodDef { sig, instance: true, home: None, inst_args: None });
    if !own.is_empty() {
        reg.set_template(Base::Method(path, method), Some(Template { params, ret, body: vec![body] }));
        return Ok(None);
    }
    reg.set_template(Base::Method(path, method), None);
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let body = lower_all(heap, &mut lw, &[body])?;
    Ok(Some(reg.defmethod_form(heap, &path, method, true, &params, &ret, &body)?))
}

/// The field list: `(name Type)` or `(pub name Type)`.
fn parse_fields(heap: &Heap, cx: &TypeCx, items: &[(Value, Option<Loc>)]) -> Result<Vec<Field>, Error> {
    let mut out: Vec<Field> = Vec::new();
    for (item, _) in items {
        let elems = heap.list_to_vec(*item).map_err(|_| Error::TypeError(SHAPE.into()))?;
        let (public, rest) = match elems.split_first() {
            Some((Value::Symbol(id), rest)) if id.is(crate::wk::PUB) => (true, rest),
            _ => (false, &elems[..]),
        };
        let (name, ty) = match rest {
            [Value::Symbol(name), ty] => (*name, *ty),
            [Value::Symbol(_), _, _] => unimplemented!("typeck: a `defstruct` field with a default"),
            _ => {
                return Err(Error::TypeError(
                    "defstruct: field must be (name type), (pub name type), or either with a default".into(),
                ))
            }
        };
        let name = member_symbol(name.name());
        if out.iter().any(|f| f.name == name) {
            return Err(Error::TypeError(format!("defstruct: duplicate field `{}`", name)));
        }
        out.push(Field { name, ty: parse_type(heap, cx, ty)?, public });
    }
    Ok(out)
}
