//! `defstruct`: a mutable product type, and the methods that read and write
//! its fields.
//!
//! `(defstruct Name<T...> (field Type) (pub field Type)...)`, or
//! `(defstruct (Name<T...> option...) ...)`. Each field `f` gets a method
//! `f` that reads it and a method `set-f` that writes it. Their type
//! parameters are the struct's, owned by each method
//! (`docs/dev/checker-rebuild-plan.md` 7.3); for a generic struct they are
//! kept as checked bodies and specialized where they are called, like any
//! generic method.
//!
//! A field may be written with a default, `(name Type default)`: the value
//! `make-instance` gives a slot it is not given, evaluated each time. The
//! default is checked once, here, as the body of a static method no source
//! can name, which `make-instance` calls.
//!
//! The options are `(:copier name)`, an instance method returning a new
//! value with the same slot values, and `(:include Parent)`, the parent's
//! fields, defaults too, before the struct's own. A struct that includes
//! another is not a subtype of it: the field list is all it takes.

use crate::check::lower::CoreForms;
use crate::check::registry::AdtKind;
use crate::types::{ParamOwner, Path, Type, TypeParam};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::{intern, member_symbol};
use typelisp_mem::SymRef;

use super::defun::{lower_all, parse_header, split_doc};
use super::generic::{Base, Requests};
use super::lower::Lowering;
use super::define::{Definer, Kind};
use super::registry::{Doc, Field, FnDef, MethodDef, Registry, Template, TypeDef, TypeEntry};
use super::scope::TypeScope;
use super::subst::Subst;
use super::tree::{Node, Typed};
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "defstruct: (defstruct Name<T...>? (field Type)...)";

/// `(defstruct ...)` — `parts` without the `defstruct` — and its core form,
/// unrooted: the type's definition, then its accessors.
pub(crate) fn check_defstruct(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let [(header, _), fields @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let (header, options) = match *header {
        Value::Cons(_) if !crate::types::is_type_application(heap, *header) => {
            let items = heap.list_to_vec(*header)?;
            (items[0], parse_options(heap, &items[1..])?)
        }
        _ => (*header, Options::default()),
    };
    let (doc, fields) = split_doc(heap, fields);
    let (name, type_names) = parse_header(heap, header)?;
    let ns = d.site.ns;
    let name = d.ownable(heap, name, "defstruct")?;
    d.type_name(reg, Kind::Type, name)?;
    let path = Path::of_symbol(ns, name);
    if let Some(doc) = doc {
        reg.docs.insert(Doc::Type(path), doc);
    }
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Def(path), &type_names).map_err(|twice| {
        Error::TypeError(format!("defstruct {}: type parameter `{}` is declared twice", path, twice.0.name()))
    })?;
    let params = scope.params().to_vec();
    // Registered before the fields are read, so a field can name the type
    // it belongs to.
    let stub = TypeDef {
        kind: AdtKind::Struct,
        params: params.clone(),
        public: d.public,
        builtin: false,
        stdlib: crate::types::in_std(ns),
        fields: Vec::new(),
        variants: Vec::new(),
    };
    reg.types.insert(path, TypeEntry::Type(stub.clone()));
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: None };
    let mut all = match options.include {
        Some(parent) => included(heap, &tcx, parent)?,
        None => Vec::new(),
    };
    for (field, default) in parse_fields(heap, &tcx, fields)? {
        if all.iter().any(|(f, _)| f.name == field.name) {
            return Err(Error::TypeError(format!("defstruct: duplicate field `{}`", field.name)));
        }
        all.push((field, default.map(Default::Written)));
    }
    if all.is_empty() {
        return Err(Error::TypeError("defstruct: needs at least one field".into()));
    }
    let fields: Vec<Field> = all.iter().map(|(f, _)| f.clone()).collect();
    reg.types.insert(path, TypeEntry::Type(TypeDef { fields: fields.clone(), ..stub }));

    let mut forms = Vec::new();
    let names: Vec<String> = fields.iter().map(|f| f.name.name().to_string()).collect();
    let field_tys: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
    let def_form = reg.defstruct_form(heap, &path, &[], &field_tys, &names)?;
    heap.push_root(def_form);
    forms.push(def_form);
    let mut push = |heap: &mut Heap, form: Option<Value>| {
        if let Some(form) = form {
            heap.push_root(form);
            forms.push(form);
        }
    };
    for (i, field) in fields.iter().enumerate() {
        let getter = member_symbol(field.name.name());
        let setter = member_symbol(&format!("set-{}", field.name.name()));
        for (method, accessor) in [(getter, Accessor::Get), (setter, Accessor::Set)] {
            let form = define_accessor(reg, heap, requests, path, &params, i, field, method, accessor)?;
            push(heap, form);
        }
    }
    for (field, default) in &all {
        let Some(default) = default else { continue };
        let form = define_default(reg, d, heap, requests, path, &scope, field, default)?;
        push(heap, form);
    }
    if let Some(copier) = options.copier {
        let form = define_copier(reg, heap, requests, path, &params, &fields, member_symbol(copier.name()), d.public)?;
        push(heap, form);
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
    reg.define_method(path, method, MethodDef { sig, instance: true, home: None, inst_args: None });
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

/// The field list: `(name Type)` or `(pub name Type)`, either with a
/// default after the type, which is returned with the field.
fn parse_fields(heap: &Heap, cx: &TypeCx, items: &[(Value, Option<Loc>)]) -> Result<Vec<(Field, Option<Value>)>, Error> {
    let mut out: Vec<(Field, Option<Value>)> = Vec::new();
    for (item, _) in items {
        let elems = heap.list_to_vec(*item).map_err(|_| Error::TypeError(SHAPE.into()))?;
        let (public, rest) = match elems.split_first() {
            Some((Value::Symbol(id), rest)) if id.is(crate::wk::PUB) => (true, rest),
            _ => (false, &elems[..]),
        };
        let (name, ty, default) = match rest {
            [Value::Symbol(name), ty] => (*name, *ty, None),
            [Value::Symbol(name), ty, default] => (*name, *ty, Some(*default)),
            _ => {
                return Err(Error::TypeError(
                    "defstruct: field must be (name type), (pub name type), or either with a default".into(),
                ))
            }
        };
        let name = member_symbol(name.name());
        if out.iter().any(|(f, _)| f.name == name) {
            return Err(Error::TypeError(format!("defstruct: duplicate field `{}`", name)));
        }
        let ty = parse_type(heap, cx, ty)?;
        crate::check::Checker::reject_raw_word_storage(&ty, "the field", name.name())?;
        out.push((Field { name, ty, public, default: default.is_some() }, default));
    }
    Ok(out)
}

/// A `(defstruct (Name option...) ...)` option list, parsed.
#[derive(Default)]
struct Options {
    copier: Option<SymRef>,
    include: Option<Value>,
}

/// The options after the name in `(defstruct (Name option...) ...)`.
///
/// Some of CL's are refused, because what they are for does not exist
/// here: `:conc-name` keeps two structs' accessors apart in one flat name
/// space, and accessors here are methods of the receiver's type;
/// `:predicate` answers at run time what a type, known before the program
/// runs, already says; `:type`, `:initial-offset` and `:named` lay the
/// value out as a list or a vector, which is the compiler's to choose.
fn parse_options(heap: &Heap, opts: &[Value]) -> Result<Options, Error> {
    let mut out = Options::default();
    for opt in opts {
        if let Value::Symbol(id) = opt {
            if !id.is_keyword() {
                let name = heap.symbol_spelling(*id);
                return Err(Error::TypeError(format!(
                    "defstruct: `{}` is not an option — a definition name's type parameters go on the name itself (`Name<{}>`); the list after `defstruct` is the option list (`:copier`, `:include`)",
                    name, name
                )));
            }
        }
        let items = heap.list_to_vec(*opt).map_err(|_| Error::TypeError("defstruct: each option must be a list, e.g. (:copier copy-point)".into()))?;
        let Some(Value::Symbol(key)) = items.first() else {
            return Err(Error::TypeError("defstruct: an option starts with a keyword, e.g. (:copier copy-point)".into()));
        };
        match key.well_known() {
            wk::KW_CONSTRUCTOR => {
                return Err(Error::TypeError(
                    "defstruct: (:constructor ...) is gone — write the constructor as a static method in the type's `impl`, \
                     e.g. (impl Point (pub defmethod new ((x int) (y int)) Point (make-instance 'Point :x x :y y)))"
                        .into(),
                ))
            }
            wk::KW_COPIER => {
                let [_, Value::Symbol(name)] = items[..] else {
                    return Err(Error::TypeError("defstruct: (:copier name) takes exactly one name".into()));
                };
                if out.copier.is_some() {
                    return Err(Error::TypeError("defstruct: :copier may appear only once".into()));
                }
                out.copier = Some(name);
            }
            wk::KW_INCLUDE => {
                let [_, parent] = items[..] else {
                    return Err(Error::TypeError("defstruct: (:include Parent) takes exactly one type".into()));
                };
                if out.include.is_some() {
                    return Err(Error::TypeError("defstruct: :include may appear only once".into()));
                }
                out.include = Some(parent);
            }
            wk::KW_CONC_NAME => {
                return Err(Error::TypeError(
                    "defstruct: :conc-name has nothing to do here — accessors are methods dispatched on the receiver's type, so slot names never collide, and a prefix would break `instance::field`"
                        .into(),
                ))
            }
            wk::KW_PREDICATE => {
                return Err(Error::TypeError(
                    "defstruct: :predicate has nothing to answer — a type is a compile-time classification with no runtime witness, so the generated predicate could only ever return true"
                        .into(),
                ))
            }
            wk::KW_TYPE | wk::KW_INITIAL_OFFSET | wk::KW_NAMED => {
                return Err(Error::TypeError(format!(
                    "defstruct: `{}` re-lays the value out as a list or vector; the representation belongs to the compiler here and nothing in the language observes it",
                    heap.symbol_spelling(*key)
                )))
            }
            _ => return Err(Error::TypeError(format!("defstruct: unknown option `{}` (known: :copier, :include)", heap.symbol_spelling(*key)))),
        }
    }
    Ok(out)
}

/// Where a field's default comes from: written on the field, or the
/// default of the field of the struct `(:include Parent<args>)` names.
enum Default {
    Written(Value),
    Included { parent: Path, args: Vec<Type> },
}

/// The fields `(:include parent)` gives, in the parent's order, its type
/// arguments put in.
fn included(heap: &Heap, cx: &TypeCx, parent: Value) -> Result<Vec<(Field, Option<Default>)>, Error> {
    let ty = parse_type(heap, cx, parent)?;
    let Type::Named(parent, args) = &ty else {
        return Err(Error::TypeError(format!("defstruct: (:include `{}`) — a struct is what can be included", ty)));
    };
    let def = match cx.reg.type_def(parent) {
        Some(def) if def.kind == AdtKind::Struct && !def.builtin => def,
        _ => return Err(Error::TypeError(format!("defstruct: (:include {}) — only a `defstruct` has slots to include", parent))),
    };
    let subst = Subst::positional(&def.params, args);
    Ok(def
        .fields
        .iter()
        .map(|f| {
            let default = f.default.then(|| Default::Included { parent: *parent, args: args.clone() });
            (Field { ty: subst.apply(&f.ty), ..f.clone() }, default)
        })
        .collect())
}

/// The static method that gives the field `name`'s default, which no
/// source can name: it holds `(`, which the reader never puts in a symbol.
pub(crate) fn default_method(name: SymRef) -> SymRef {
    member_symbol(&format!("(default {})", name.name()))
}

/// Register the method giving `field`'s default and return its code, or
/// `None` when the struct is generic and the method is kept to be
/// specialized. A default written on the field is checked here, where the
/// struct's type parameters are in `scope` and no local is.
#[allow(clippy::too_many_arguments)]
fn define_default(
    reg: &mut Registry,
    d: &Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    path: Path,
    scope: &TypeScope,
    field: &Field,
    default: &Default,
) -> Result<Option<Value>, Error> {
    let method = default_method(field.name);
    let struct_params = scope.params().to_vec();
    let owner = ParamOwner::Method(path, method);
    let own: Vec<TypeParam> = struct_params.iter().map(|p| TypeParam { owner, ..*p }).collect();
    let reown = Subst::positional(&struct_params, &own.iter().map(|p| Type::Param(*p)).collect::<Vec<_>>());
    let mut body = match default {
        Default::Written(form) => {
            let cx = super::expr::Cx::new(reg, d.expander, d.site.ns, scope, None);
            let mut body = super::expr::check_expr(&cx, heap, &super::locals::Locals::empty(), *form, Some(&field.ty), heap.cons_loc(*form))
                .map_err(|e| Error::TypeError(format!("defstruct {}: the default of `{}`: {}", path, field.name, message(e))))?;
            cx.settle(heap, std::slice::from_mut(&mut body))?;
            body
        }
        Default::Included { parent, args } => {
            let node = Node::MethodCall { owner: *parent, method, instance: false, targs: args.clone(), args: Vec::new() };
            Typed { node, ty: field.ty.clone(), loc: None }
        }
    };
    body.map_types(&|t| reown.apply(t));
    let ret = reown.apply(&field.ty);
    let sig = FnDef { public: true, ..FnDef::plain(own.clone(), Vec::new(), ret.clone()) };
    reg.define_method(path, method, MethodDef { sig, instance: false, home: None, inst_args: None });
    define_body(reg, heap, requests, path, method, false, &own, Vec::new(), ret, body)
}

/// What an error says, without where: the default is the struct's.
fn message(e: Error) -> String {
    match e.kind() {
        Error::TypeError(m) => m.clone(),
        other => other.to_string(),
    }
}

/// Register the copier `method`, an instance method returning a new value
/// with the same slot values, and return its code, or `None` when the
/// struct is generic.
#[allow(clippy::too_many_arguments)]
fn define_copier(
    reg: &mut Registry,
    heap: &mut Heap,
    requests: &mut Requests,
    path: Path,
    struct_params: &[TypeParam],
    fields: &[Field],
    method: SymRef,
    public: bool,
) -> Result<Option<Value>, Error> {
    let owner = ParamOwner::Method(path, method);
    let own: Vec<TypeParam> = struct_params.iter().map(|p| TypeParam { owner, ..*p }).collect();
    let targs: Vec<Type> = own.iter().map(|p| Type::Param(*p)).collect();
    let reown = Subst::positional(struct_params, &targs);
    let recv_ty = Type::Named(path, targs.clone());
    let self_name = intern("self");
    let field_tys: Vec<Type> = fields.iter().map(|f| reown.apply(&f.ty)).collect();
    let args = field_tys
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let obj = Typed { node: Node::Local { name: self_name }, ty: recv_ty.clone(), loc: None };
            Typed { node: Node::FieldGet { obj: Box::new(obj), index }, ty: ty.clone(), loc: None }
        })
        .collect();
    let node = Node::Construct { owner: path, targs, variant: 0, mutable: true, field_tys, args };
    let body = Typed { node, ty: recv_ty.clone(), loc: None };
    let params = vec![(self_name, recv_ty.clone())];
    let sig = FnDef { public, ..FnDef::plain(own.clone(), vec![recv_ty.clone()], recv_ty.clone()) };
    reg.define_method(path, method, MethodDef { sig, instance: true, home: None, inst_args: None });
    define_body(reg, heap, requests, path, method, true, &own, params, recv_ty, body)
}

/// The checked `body` of `path`'s method `method`: kept to be specialized
/// when the method has type parameters `own`, its code otherwise.
#[allow(clippy::too_many_arguments)]
fn define_body(
    reg: &mut Registry,
    heap: &mut Heap,
    requests: &mut Requests,
    path: Path,
    method: SymRef,
    instance: bool,
    own: &[TypeParam],
    params: Vec<(SymRef, Type)>,
    ret: Type,
    body: Typed,
) -> Result<Option<Value>, Error> {
    if !own.is_empty() {
        // The body is kept past this form, so the values it holds are too.
        body.each_value(&mut |v| heap.push_permanent_root(v));
        reg.set_template(Base::Method(path, method), Some(Template { params, ret, body: vec![body] }));
        return Ok(None);
    }
    reg.set_template(Base::Method(path, method), None);
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let body = lower_all(heap, &mut lw, &[body])?;
    Ok(Some(reg.defmethod_form(heap, &path, method, instance, &params, &ret, &body)?))
}
