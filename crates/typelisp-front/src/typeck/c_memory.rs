//! `def-c-struct` and typed pointers: memory laid out as C lays it out,
//! allocated inside `(unsafe ...)` and freed when that form is left.
//!
//! The outermost `(unsafe ...)` of a function body (or of a top-level form)
//! owns every `c-alloc` inside it, and is wrapped in an arena opened before
//! it and closed after it however it is left. A `lambda` or a `labels`
//! function inside starts over. The value of an `unsafe` may not be a typed
//! pointer, which would outlive the memory. `(c-alloc T n)`, `(c-ref p i)`,
//! `(c-deref p)`, `p::field` and their `setf`s are calls of the internal
//! builtins `crate::c_struct` names, and a typed pointer that came from C (a
//! `defffi` result, a pointer field read back) is checked against the live
//! allocations at run time. The rules and the messages are the old
//! checker's (`check::analyze::c_struct`).

use crate::c_struct as b;
use crate::check::registry::{CField, CStructDef};
use crate::types::{written_path, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;
use typelisp_rt::c_mem;

use super::define::{Definer, Kind};
use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::registry::{Definition, Registry, TypeEntry};
use super::resolve::{find_named, sees};
use super::tree::{Node, Typed};
use super::type_syntax::{parse_raw, TypeCx};

/// What a typed pointer may point at, and a `def-c-struct` field may be,
/// spelled out for an error message.
const C_TYPES: &str = "i8, i16, i32, u8, u16, u32, c-long, c-ulong, f32, f64, bool, ptr, \
                       a typed pointer (ptr T), or a def-c-struct";

/// The load/store kind of a C scalar type, `ptr` or a typed pointer.
fn c_kind(ty: &Type) -> Option<i64> {
    Some(match ty {
        Type::I8 => c_mem::KIND_I8,
        Type::I16 => c_mem::KIND_I16,
        Type::I32 => c_mem::KIND_I32,
        Type::U8 => c_mem::KIND_U8,
        Type::U16 => c_mem::KIND_U16,
        Type::U32 => c_mem::KIND_U32,
        Type::CLong => c_mem::KIND_C_LONG,
        Type::CULong => c_mem::KIND_C_ULONG,
        Type::F32 => c_mem::KIND_F32,
        Type::F64 => c_mem::KIND_F64,
        Type::Bool => c_mem::KIND_BOOL,
        Type::Ptr | Type::PtrTo(_) => c_mem::KIND_PTR,
        _ => return None,
    })
}

/// Whether `ty` is, or holds, a typed pointer.
pub(crate) fn mentions_typed_ptr(ty: &Type) -> bool {
    let mut found = false;
    super::unify::walk(ty, &mut |t| found |= matches!(t, Type::PtrTo(_)));
    found
}

// ---- types -------------------------------------------------------------------

/// The `def-c-struct` the name `v` (a symbol or a `::` path) names here.
fn c_struct_named<'r>(heap: &Heap, reg: &'r Registry, ns: typelisp_mem::NsId, v: Value) -> Option<&'r CStructDef> {
    let entry = match v {
        Value::Symbol(id) => find_named(reg, ns, id, |n, key| reg.types.get(&Path::of_symbol(n, key))).map(|(_, e)| e),
        Value::Path(_) => written_path(heap, ns, v).ok().and_then(|p| reg.types.get(&p)),
        _ => None,
    };
    match entry {
        Some(TypeEntry::CStruct(def)) if def.public || sees(ns, def.name.ns()) => Some(def),
        _ => None,
    }
}

/// `(ptr T)`'s `T`: a `def-c-struct` by name, or a C type.
pub(crate) fn pointee(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    if let Some(def) = c_struct_named(heap, cx.reg, cx.ns, v) {
        return Ok(Type::CStruct(def.name));
    }
    let ty = parse_raw(heap, cx, v)?;
    match &ty {
        t if c_kind(t).is_some() => Ok(ty),
        t => Err(Error::TypeError(format!("`(ptr {})`: a typed pointer points at {} — `{}` is none of these", t, C_TYPES, t))),
    }
}

/// The definition behind a [`Type::CStruct`].
fn c_struct_def<'r>(reg: &'r Registry, path: &Path) -> Result<&'r CStructDef, Error> {
    match reg.types.get(path) {
        Some(TypeEntry::CStruct(def)) => Ok(def),
        _ => Err(Error::TypeError(format!("internal error: C struct `{}` is not registered", path))),
    }
}

/// `(sizeof, _Alignof)` of a C type on LP64.
fn c_layout(reg: &Registry, ty: &Type) -> Result<(u64, u64), Error> {
    Ok(match ty {
        Type::I8 | Type::U8 | Type::Bool => (1, 1),
        Type::I16 | Type::U16 => (2, 2),
        Type::I32 | Type::U32 | Type::F32 => (4, 4),
        Type::CLong | Type::CULong | Type::F64 | Type::Ptr | Type::PtrTo(_) => (8, 8),
        Type::CStruct(p) => {
            let d = c_struct_def(reg, p)?;
            (d.size, d.align)
        }
        other => return Err(Error::TypeError(format!("`{}` is not a C type — it must be one of {}", other, C_TYPES))),
    })
}

/// Every `(offset, type)` a C object of type `ty` placed at `base` starts
/// at: itself, and, for a struct, each field, recursively.
fn c_positions(reg: &Registry, ty: &Type, base: u64, out: &mut Vec<(u64, Type)>) -> Result<(), Error> {
    out.push((base, ty.clone()));
    if let Type::CStruct(p) = ty {
        for f in &c_struct_def(reg, p)?.fields {
            c_positions(reg, &f.ty, base + f.offset, out)?;
        }
    }
    Ok(())
}

// ---- declarations -----------------------------------------------------------

/// Whether the top-level `(unsafe ...)` of `forms` declares C structs: every
/// form a `def-c-struct` (possibly `pub`). A mixture is refused, as the
/// declarations and an expression would be checked by two different rules.
pub(crate) fn declares_c_structs(heap: &Heap, forms: &[(Value, Option<Loc>)]) -> Result<bool, Error> {
    let raw: Vec<Value> = forms.iter().map(|(v, _)| *v).collect();
    crate::check::Checker::unsafe_declares_c_structs(heap, &raw)
}

/// `(unsafe (def-c-struct ...)...)` at top level: each struct registered.
/// Nothing runs, so the form is an empty `module` named after the last.
pub(crate) fn check_c_struct_block(reg: &mut Registry, d: &mut Definer, heap: &mut Heap, forms: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    let mut last = None;
    for (form, _) in forms {
        let items = heap.list_to_vec_locs(*form)?;
        let public = matches!(items.first(), Some((Value::Symbol(id), _)) if id.is(wk::PUB));
        let parts = &items[if public { 2 } else { 1 }..];
        let outer = d.public;
        d.public = public;
        let path = check_def_c_struct(reg, d, heap, parts);
        d.public = outer;
        last = Some(path?);
    }
    let last = last.expect("a block that declares C structs has one");
    crate::check::forms::module_form(heap, &last, &[])
}

/// `(def-c-struct name "doc"? (field type)...)` — `parts` without the head.
fn check_def_c_struct(reg: &mut Registry, d: &mut Definer, heap: &mut Heap, parts: &[(Value, Option<Loc>)]) -> Result<Path, Error> {
    let usage = "def-c-struct: (def-c-struct name (field type)...)";
    let Some(((Value::Symbol(name), _), rest)) = parts.split_first() else {
        return Err(Error::TypeError(usage.into()));
    };
    let name = *name;
    let name = d.ownable(heap, name, "def-c-struct")?;
    let (doc, specs) = match rest.split_first() {
        Some(((Value::Str(id), _), specs)) => (Some(heap.string(*id).to_string()), specs),
        _ => (None, rest),
    };
    if specs.is_empty() {
        return Err(Error::TypeError(format!("def-c-struct {}: a C struct needs at least one field", name.name())));
    }
    d.type_name(reg, Kind::CStruct, name)?;
    let path = Path::of_symbol(d.site.ns, name);
    // Registered with no fields while its fields are read, so that a field
    // can point at the struct being defined (`(next (ptr node))`).
    let public = d.public;
    reg.types.insert(path, TypeEntry::CStruct(CStructDef { name: path, fields: Vec::new(), size: 0, align: 1, public }));
    let (fields, size, align) = c_struct_fields(reg, heap, d.site.ns, name, &path, specs)?;
    reg.types.insert(path, TypeEntry::CStruct(CStructDef { name: path, fields, size, align, public }));
    d.record(reg, Definition::Type(path), doc);
    Ok(path)
}

/// The fields of the C struct `name` (at `path`), laid out by C's rules:
/// each field at the next multiple of its alignment, the struct as aligned
/// as its most aligned field, its size rounded up to that.
fn c_struct_fields(reg: &Registry, heap: &Heap, ns: typelisp_mem::NsId, name: SymRef, path: &Path, specs: &[(Value, Option<Loc>)]) -> Result<(Vec<CField>, u64, u64), Error> {
    let mut fields: Vec<CField> = Vec::new();
    let (mut offset, mut align) = (0u64, 1u64);
    let root = super::scope::TypeScope::empty();
    let tcx = TypeCx { reg, ns, scope: &root, self_type: None };
    let shape = || Error::TypeError(format!("def-c-struct {}: each field is (name type)", name.name()));
    for (spec, _) in specs {
        let items = heap.list_to_vec(*spec).map_err(|_| shape())?;
        let [Value::Symbol(fid), tv] = items[..] else { return Err(shape()) };
        let fname = member_symbol(fid.name());
        if fields.iter().any(|f| f.name == fname) {
            return Err(Error::TypeError(format!("def-c-struct {}: field `{}` is declared twice", name.name(), fname)));
        }
        let ty = match c_struct_named(heap, reg, ns, tv) {
            Some(def) => Type::CStruct(def.name),
            None => {
                let ty = parse_raw(heap, &tcx, tv)?;
                if c_kind(&ty).is_none() {
                    return Err(Error::TypeError(format!("a C struct field cannot be `{}` — it must be one of {}", ty, C_TYPES)));
                }
                ty
            }
        };
        if ty == Type::CStruct(*path) {
            return Err(Error::TypeError(format!(
                "def-c-struct {}: field `{}` holds the struct itself, which would make it infinitely \
                 large — a field can point at it instead: (ptr {})",
                name.name(),
                fname,
                name.name()
            )));
        }
        let (fsize, falign) = c_layout(reg, &ty)?;
        offset = offset.div_ceil(falign) * falign;
        fields.push(CField { name: fname, ty, offset });
        offset += fsize;
        align = align.max(falign);
    }
    Ok((fields, offset.div_ceil(align) * align, align))
}

// ---- ownership --------------------------------------------------------------

/// `(unsafe body...)`'s checked body `body` (of type `ty`), around which
/// an arena is opened and closed when this `unsafe` owns allocations.
/// `owner` is whether it is the outermost of its function, and `allocates`
/// whether a `c-alloc` in it was met.
pub(crate) fn owned(cx: &Cx, body: Typed, ty: &Type, owner: bool, allocates: bool) -> Result<Typed, Error> {
    if mentions_typed_ptr(&cx.resolve(ty)) {
        return Err(Error::TypeError(format!(
            "an `(unsafe ...)` cannot answer `{}`: the memory a typed pointer points at is freed \
             when the `unsafe` that allocated it is left. Copy what is needed into ordinary \
             values (a `defstruct`, numbers) inside the `unsafe`, and answer those.",
            cx.resolve(ty)
        )));
    }
    if !(owner && allocates) {
        return Ok(body);
    }
    let arena = arena_local();
    let open = builtin(b::ARENA_OPEN, Vec::new(), Type::CULong);
    let close = builtin(b::ARENA_CLOSE, vec![arena_var()], Type::Unit);
    let protected = Typed { node: Node::UnwindProtect { protected: Box::new(body), cleanup: Box::new(close) }, ty: ty.clone(), loc: None };
    Ok(Typed { node: Node::Let { binds: vec![(arena, open)], body: vec![protected] }, ty: ty.clone(), loc: None })
}

/// The local the owning `unsafe` binds its arena to.
fn arena_local() -> SymRef {
    typelisp_mem::symbols::intern(b::ARENA_VAR)
}

/// The owning `unsafe`'s arena.
fn arena_var() -> Typed {
    Typed { node: Node::Local { name: arena_local() }, ty: Type::CULong, loc: None }
}

// ---- expressions ------------------------------------------------------------

/// A call of the internal builtin `name`, which answers a `ret`.
fn builtin(name: u32, args: Vec<Typed>, ret: Type) -> Typed {
    Typed { node: Node::Call { path: Path::internal_builtin(name), targs: Vec::new(), args }, ty: ret, loc: None }
}

/// The raw word `n`, an argument of a builtin.
fn word(heap: &mut Heap, n: u64) -> Result<Typed, Error> {
    let lit = super::literal::literal(heap, Value::Int(n as i64), Some(&Type::I32))?.expect("a number is a literal");
    Ok(Typed { node: Node::Literal { tag: lit.tag, value: lit.value }, ty: lit.ty, loc: None })
}

/// The type `ty` as a builtin's argument.
fn type_value(ty: &Type) -> Typed {
    Typed { node: Node::TypeValue { ty: ty.clone() }, ty: Type::Int, loc: None }
}

/// `value`, a typed pointer to `pointee` that came from C, once it is known
/// to point into live memory.
pub(crate) fn checked_from_c(mut value: Typed, pointee: &Type) -> Typed {
    let ty = std::mem::replace(&mut value.ty, Type::Ptr);
    let loc = value.loc.clone();
    let mut checked = builtin(b::PTR_CHECK, vec![value, type_value(pointee)], ty);
    checked.loc = loc;
    checked
}

/// An `int` argument, as the raw word a builtin reads.
fn int_arg(cx: &Cx, heap: &mut Heap, locals: &Locals, (v, loc): &(Value, Option<Loc>)) -> Result<Typed, Error> {
    let n = check_expr(cx, heap, locals, *v, Some(&Type::Int), loc.clone())?;
    Ok(Typed { node: Node::UntagInt { value: Box::new(n) }, ty: Type::I32, loc: None })
}

/// A typed pointer argument, and what it points at.
fn ptr_arg(cx: &Cx, heap: &mut Heap, locals: &Locals, (v, loc): &(Value, Option<Loc>), who: &str) -> Result<(Typed, Type), Error> {
    let p = check_expr(cx, heap, locals, *v, None, loc.clone())?;
    match cx.resolve(&p.ty) {
        Type::PtrTo(inner) => Ok((p, *inner)),
        other => Err(Error::TypeError(format!("{}: expected a typed pointer (ptr T), got `{}`", who, other))),
    }
}

/// `(c-alloc T)` or `(c-alloc T n)` — `args` without the head: `n` zeroed
/// `T`s (one when not given), owned by the `unsafe` around it.
pub(crate) fn check_c_alloc(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    if args.is_empty() || args.len() > 2 {
        return Err(Error::TypeError("c-alloc: (c-alloc Type) or (c-alloc Type count)".into()));
    }
    if !cx.c_arena_open() {
        return Err(Error::TypeError(
            "c-alloc: the memory belongs to the `(unsafe ...)` it is allocated in, and is freed when \
             that is left — write it inside one, in the same function"
                .into(),
        ));
    }
    let elem = pointee(heap, &cx.types(), args[0].0)?;
    let count = match args.get(1) {
        Some(n) => int_arg(cx, heap, locals, n)?,
        None => word(heap, 1)?,
    };
    let (size, align) = c_layout(cx.reg, &elem)?;
    let mut positions = Vec::new();
    c_positions(cx.reg, &elem, 0, &mut positions)?;
    let mut all = vec![arena_var(), count, word(heap, size)?, word(heap, align)?];
    for (off, t) in positions {
        all.push(word(heap, off)?);
        all.push(type_value(&t));
    }
    cx.c_arena_used();
    let mut t = builtin(b::ALLOC, all, Type::PtrTo(Box::new(elem)));
    t.loc = loc;
    Ok(t)
}

/// `(c-ref p i)`: a pointer to the `i`th element from `p`, which must stay
/// inside `p`'s allocation.
pub(crate) fn check_c_ref(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [p, i] = args else {
        return Err(Error::TypeError("c-ref: (c-ref pointer index)".into()));
    };
    let (p, elem) = ptr_arg(cx, heap, locals, p, "c-ref")?;
    let ty = cx.resolve(&p.ty);
    let i = int_arg(cx, heap, locals, i)?;
    let (size, _) = c_layout(cx.reg, &elem)?;
    let mut t = builtin(b::INDEX, vec![p, i, word(heap, size)?, type_value(&elem)], ty);
    t.loc = loc;
    Ok(t)
}

/// `(c-deref p)`: the scalar `p` points at.
pub(crate) fn check_c_deref(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [p] = args else {
        return Err(Error::TypeError("c-deref: (c-deref pointer)".into()));
    };
    let (p, pointee) = ptr_arg(cx, heap, locals, p, "c-deref")?;
    if let Type::CStruct(s) = &pointee {
        return Err(Error::TypeError(format!("c-deref: `{}` is a C struct, which is not a value — read its fields: p::field", s)));
    }
    let mut t = load(heap, p, 0, &pointee)?;
    t.loc = loc;
    Ok(t)
}

/// `(setf (c-deref p) value)` — `place_args` the place's arguments.
pub(crate) fn set_c_deref(cx: &Cx, heap: &mut Heap, locals: &Locals, place_args: &[(Value, Option<Loc>)], (value, value_loc): (Value, Option<Loc>), loc: Option<Loc>) -> Result<Typed, Error> {
    let [p] = place_args else {
        return Err(Error::TypeError("setf: (setf (c-deref pointer) value)".into()));
    };
    let (p, pointee) = ptr_arg(cx, heap, locals, p, "c-deref")?;
    let v = check_expr(cx, heap, locals, value, Some(&pointee), value_loc)?;
    let mut t = store(cx, heap, p, 0, &pointee, v)?;
    t.loc = loc;
    Ok(t)
}

/// `recv::field` of the C struct the typed pointer `recv` points at, when
/// it points at one.
pub(crate) fn field_get(cx: &Cx, heap: &mut Heap, recv: Typed, field: SymRef) -> Option<Result<Typed, Error>> {
    let Type::PtrTo(inner) = cx.resolve(&recv.ty) else { return None };
    let Type::CStruct(s) = *inner else { return None };
    Some(c_field(cx.reg, &s, field).and_then(|f| load(heap, recv, f.offset, &f.ty)))
}

/// `(setf recv::field value)` for a typed pointer `recv` to a C struct, when
/// it is one.
pub(crate) fn field_set(cx: &Cx, heap: &mut Heap, locals: &Locals, recv: Typed, field: SymRef, (value, value_loc): (Value, Option<Loc>)) -> Option<Result<Typed, Error>> {
    let Type::PtrTo(inner) = cx.resolve(&recv.ty) else { return None };
    let Type::CStruct(s) = *inner else { return None };
    Some((|| {
        let f = c_field(cx.reg, &s, field)?;
        let v = check_expr(cx, heap, locals, value, Some(&f.ty), value_loc)?;
        store(cx, heap, recv, f.offset, &f.ty, v)
    })())
}

/// The field `field` of the C struct `s`.
fn c_field(reg: &Registry, s: &Path, field: SymRef) -> Result<CField, Error> {
    let member = member_symbol(field.name());
    c_struct_def(reg, s)?
        .fields
        .iter()
        .find(|f| f.name == member)
        .cloned()
        .ok_or_else(|| Error::TypeError(format!("C struct `{}` has no field `{}`", s, field.name())))
}

/// What a load of `ty` at `addr + offset` answers. A struct held by value
/// answers its address.
fn load(heap: &mut Heap, addr: Typed, offset: u64, ty: &Type) -> Result<Typed, Error> {
    if let Type::CStruct(_) = ty {
        return Ok(builtin(b::OFFSET, vec![addr, word(heap, offset)?], Type::PtrTo(Box::new(ty.clone()))));
    }
    let kind = c_kind(ty).ok_or_else(|| Error::TypeError(format!("internal error: no load for `{}`", ty)))?;
    let loaded = builtin(b::LOAD, vec![addr, word(heap, offset)?, word(heap, kind as u64)?], ty.clone());
    Ok(match ty {
        // A pointer read back from memory C may have written.
        Type::PtrTo(pointee) => checked_from_c(loaded, pointee),
        _ => loaded,
    })
}

/// `value` (checked against `ty`) stored at `addr + offset`, and answered:
/// `setf` answers what it stored.
fn store(cx: &Cx, heap: &mut Heap, addr: Typed, offset: u64, ty: &Type, value: Typed) -> Result<Typed, Error> {
    let Some(kind) = c_kind(ty) else {
        return Err(Error::TypeError(format!("setf: a field holding a struct by value (`{}`) cannot be assigned as a whole — assign its fields", ty)));
    };
    let temp = cx.temp();
    let local = || Typed { node: Node::Local { name: temp }, ty: ty.clone(), loc: None };
    let stored = builtin(b::STORE, vec![addr, word(heap, offset)?, word(heap, kind as u64)?, local()], Type::Unit);
    Ok(Typed { node: Node::Let { binds: vec![(temp, value)], body: vec![stored, local()] }, ty: ty.clone(), loc: None })
}
