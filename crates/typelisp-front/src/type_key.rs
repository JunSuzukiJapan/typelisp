//! A type's *runtime identity*: the token that says which type a heap value
//! is, and the only functions allowed to mint, write, compare, or read it
//! back.
//!
//! A `BoxedObj::Struct`/`Enum` carries its type's identity as an interned
//! [`TypeId`] (`Heap::alloc_struct`/`alloc_enum`). That identity is not
//! decoration — it is consulted by `match_pattern`'s enum/struct arms,
//! `rt_sexpr_instance_test` (compiled code's downcast), `equalp`, the
//! printer's variant-name lookup, and `print-object` dispatch. Everyone who
//! writes it and everyone who reads it must agree, and what they agree on is
//! the *name the id was interned under*: **the whole path** ([`Path`]'s
//! `Display`), never its last segment — `m::vector` and the built-in `vector`
//! are different types.
//!
//! That agreement was broken twice on 2026-08-04 — the interpreter wrote
//! `Path::to_string` while `core_bridge` wrote `Path::last_segment`, so a value
//! built by compiled code was unmatchable by interpreted code (and vice
//! versa), printed as `<unknown-variant>`, and compared unequal to its own
//! twin. Neither side was *wrong on its own*; they simply spelled the same
//! concept two ways. Hence this module: one entry point per direction, and
//! `tests/type_identity_guard_test.rs` keeps the raw heap accessors from being
//! called anywhere else.
//!
//! The identity used to *be* a `String`, copied onto every instance and
//! compared character by character. Interning moved the spelling into the
//! heap's type-key table and left an integer on the value, so the comparisons
//! below are `==` on a [`TypeId`]; a name is built only where one has to
//! cross into the table ([`type_key_of`]), and read back only for printing.
//!
//! The `crates/typelisp-rt` shims (`rt_data_new`, `rt_sexpr_instance_test`)
//! are deliberately outside this rule: they receive the type as a type form
//! from compiled code, having no `Type` to work from, and intern it
//! themselves (`typelisp_mem::type_of_form`). Their end of the agreement is
//! held up by the form being the one the checker wrote (`core::type_form`).

use std::borrow::Cow;

use crate::mem::BoxId;
use crate::{Heap, Path, Type, TypeId};
use typelisp_mem::{ConcreteNames, NsId, TypeKey, TypeShape};

/// How `p` is spelled when interned as (or looked up among) type identities.
/// The one place a type identity *name* is produced.
///
/// Borrowed for a root-level name — the common case, and the one where
/// joining would allocate a copy of a string that already exists.
pub fn type_key_of(p: &Path) -> Cow<'static, str> {
    if p.is_simple() {
        Cow::Borrowed(p.last_segment())
    } else {
        Cow::Owned(p.segments().join("::"))
    }
}

/// The interned identity of the type `p` names at the type arguments
/// `args`, minting it if it has not seen the type before.
pub fn type_key_id(p: &Path, args: Vec<TypeId>) -> TypeId {
    typelisp_mem::intern_type(TypeShape::Named(p.ns(), p.name_symbol(), args))
}

/// The interned identity of the type `p` names, or `None` if no value has
/// ever been of that type — which answers "is `id` one of those?"
/// on its own, without minting anything.
fn existing_type_key_id(p: &Path) -> Option<TypeId> {
    typelisp_mem::find_type(&TypeShape::Named(p.ns(), p.name_symbol(), Vec::new()))
}

/// The interned identity of the type `ty`, minting it if it has not been seen
/// before. The one place a checker type becomes a runtime one; the shape is
/// the one `core::type_form` writes for the same type.
pub fn type_id(ty: &Type) -> TypeId {
    type_id_in(ty, &[])
}

/// A field's declared type as a *template*: its type with each of the owning
/// type's parameters (`params`, in declaration order) as a
/// [`TypeShape::Param`] — `Option<T>` in a `gen<T>` is `option<$0>`.
/// [`typelisp_mem::instantiate`] gives back the type a particular
/// instantiation's field has, from the value's own type; that is how the
/// printer, which sees a struct only through its box, learns that a field is
/// a niche-represented `Option` it must render as one.
pub fn field_template(params: &[String], field: &Type) -> TypeId {
    type_id_in(field, params)
}

/// [`type_id`], with the type variables `params` names as parameters.
fn type_id_in(ty: &Type, params: &[String]) -> TypeId {
    let ids = |tys: &[Type]| tys.iter().map(|t| type_id_in(t, params)).collect::<Vec<_>>();
    let shape = match ty {
        Type::Named(p, args) => TypeShape::Named(p.ns(), p.name_symbol(), ids(args)),
        Type::Dyn(p, pins) => TypeShape::Dyn(p.ns(), p.name_symbol(), ids(pins)),
        Type::Fn(ps, rest, r) => TypeShape::Fn(ids(ps), rest.as_deref().map(|t| type_id_in(t, params)), type_id_in(r, params)),
        Type::PtrTo(t) => TypeShape::Ptr(type_id_in(t, params)),
        Type::CStruct(p) => TypeShape::CStruct(p.ns(), p.name_symbol()),
        Type::Unit => TypeShape::Unit,
        Type::Never => TypeShape::Never,
        Type::Var(name) => match params.iter().position(|p| p == name) {
            Some(i) => TypeShape::Param(i as u32),
            None => panic!("the type variable `{}` has no runtime identity (internal error)", name),
        },
        Type::Ptr
        | Type::CLong
        | Type::CULong
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::F32
        | Type::F64
        | Type::Int
        | Type::Ratio
        | Type::Rational
        | Type::RandomState
        | Type::Bool
        | Type::Char
        | Type::Str
        | Type::Symbol => TypeShape::named(NsId::ROOT, &ty.type_key(), &[]),
    };
    typelisp_mem::intern_type(shape)
}

/// Allocate a boxed struct of the type `p` names — `Heap::alloc_struct` with
/// the identity resolved for you. The one place a struct's key is written.
pub fn alloc_typed_struct(heap: &mut Heap, p: &Path, fields: Vec<crate::Value>) -> crate::Value {
    let key = type_key_id(p, Vec::new());
    heap.alloc_struct(key, fields)
}

/// Allocate a boxed enum value of the type `p` names — the enum counterpart of
/// [`alloc_typed_struct`]. The one place an enum's key is written.
pub fn alloc_typed_enum(
    heap: &mut Heap,
    p: &Path,
    variant: usize,
    fields: Vec<crate::Value>,
) -> crate::Value {
    let key = type_key_id(p, Vec::new());
    heap.alloc_enum(key, variant, fields)
}

/// Allocate a boxed struct whose runtime identity is `ty` — the type the
/// construction site wrote, carried through the core IR's `construct` node.
///
/// The [`Path`]-taking [`alloc_typed_struct`] answers the same question for a
/// caller that has no instantiation to speak of (a non-generic type).
pub fn alloc_struct_of(heap: &mut Heap, ty: TypeId, fields: Vec<crate::Value>) -> crate::Value {
    heap.alloc_struct(ty, fields)
}

/// A fresh `Vector<Option<Sexpr>>` of `elems` — the `Sexpr` `vector`
/// variant's box, and the payload of its `tuple` variant.
pub fn alloc_sexpr_vector(heap: &mut Heap, elems: Vec<crate::Value>) -> crate::Value {
    heap.alloc_struct(crate::TypeId::SEXPR_VECTOR, elems)
}

/// The tuple of `Option<Sexpr>` elements `elems` — what `#{..}` reads as.
/// Its type is `typelisp_mem::sexpr_tuple_type`'s, which
/// `tests/type_identity_guard_test.rs` holds equal to the checker's.
pub fn alloc_sexpr_tuple(heap: &mut Heap, elems: Vec<crate::Value>) -> crate::Value {
    let ty = typelisp_mem::sexpr_tuple_type(elems.len());
    heap.alloc_struct(ty, elems)
}

/// A fresh `HashTable<K,V>` of the type `ty` — the map-payload counterpart of
/// [`alloc_struct_of`], here for the same reason: writing a type identity is
/// this module's job.
pub fn alloc_hashtable_of(heap: &mut Heap, ty: TypeId) -> crate::Value {
    heap.alloc_hashtable(ty)
}

/// [`alloc_struct_of`]'s enum counterpart.
pub fn alloc_enum_of(heap: &mut Heap, ty: TypeId, variant: usize, fields: Vec<crate::Value>) -> crate::Value {
    heap.alloc_enum(ty, variant, fields)
}

/// The type of boxed value `id`, or `None` when the box is neither a struct
/// nor an enum (a closure, a cell, a string builder, ...).
pub fn heap_type_id(heap: &Heap, id: BoxId) -> Option<TypeId> {
    if heap.is_struct(id) {
        Some(heap.struct_type_key(id)) // type-identity-ok: this is the reader
    } else if heap.is_enum(id) {
        Some(heap.enum_type_key(id)) // type-identity-ok: this is the reader
    } else {
        None
    }
}

/// Whether boxed value `id` is an instance of the type `p` names. The one
/// place a type identity is compared.
///
/// Deliberately indifferent to struct-vs-enum: a path names one or the other,
/// never both, so the key comparison alone settles it. Callers that go on to
/// *decode fields* still test `is_struct`/`is_enum` for their own reasons.
pub fn heap_type_is(heap: &Heap, id: BoxId, p: &Path) -> bool {
    match heap_type_id(heap, id) {
        Some(k) => existing_type_key_id(p) == Some(k),
        None => false,
    }
}

/// Whether heap value `id` has the pre-interned identity `key` — one of the
/// `TypeId` constants, for a type the runtime builds without a checker
/// (`#(..)`'s box, say). The id is already the identity, so there is no name
/// to look up.
pub fn heap_type_is_id(heap: &Heap, id: BoxId, key: TypeId) -> bool {
    heap_type_id(heap, id) == Some(key)
}

/// The raw key stored in boxed value `id` — the spelling, instantiation and
/// all. [`heap_type_path`] is the same thing reduced to a [`Path`], for
/// callers that want the *type* rather than the instantiation.
///
/// Owned rather than borrowed: `Heap::type_key_name` hands back a
/// read-locked guard tied to its own lifetime, not `heap`'s
/// (`docs/dev/os-threads-design.md` §3), and every caller here needs the
/// spelling past that guard's scope (stashed in a `Path`, compared against
/// another owned key, or just printed) — a clone of an interned type name is
/// cheap next to that.
pub fn heap_type_key(heap: &Heap, id: BoxId) -> Option<String> {
    heap_type_id(heap, id).map(|k| typelisp_mem::type_name(k).to_string())
    // type-identity-ok: reading the spelling back out is this function's job
}

/// The type boxed value `id` belongs to — the definition its type
/// instantiates — or `None` when the box carries no type identity.
pub fn heap_type_path(heap: &Heap, id: BoxId) -> Option<Path> {
    heap_type_id(heap, id).map(|ty| definition_path(ty).expect("a box's type is a named one"))
}

/// The definition the named type `ty` instantiates, or `None` when `ty` is
/// not a named type.
pub fn definition_path(ty: TypeId) -> Option<Path> {
    let (ns, sym) = typelisp_mem::type_shape(ty).named_definition()?;
    Some(Path::of_symbol(ns, sym))
}

/// How a type is spelled as text: the whole path, plus its type arguments
/// when it has any (`gen<i32>`, `hashtable<string,i32>`). For a type with no
/// arguments this is [`type_key_of`] of its path, character for character.
impl TypeKey for Type {
    fn type_key(&self) -> String {
        match self {
            Type::Ptr => "ptr".into(),
            Type::CLong => "c-long".into(),
            Type::CULong => "c-ulong".into(),
            Type::I8 => "i8".into(),
            Type::I16 => "i16".into(),
            Type::I32 => "i32".into(),
            Type::U8 => "u8".into(),
            Type::U16 => "u16".into(),
            Type::U32 => "u32".into(),
            Type::F32 => "f32".into(),
            Type::F64 => "f64".into(),
            Type::Int => "int".into(),
            Type::Ratio => "ratio".into(),
            Type::Rational => "rational".into(),
            Type::RandomState => "random-state".into(),
            Type::Bool => "bool".into(),
            Type::Char => "char".into(),
            Type::Str => "string".into(),
            Type::Symbol => "symbol".into(),
            Type::Unit => "()".into(),
            Type::Never => "!".into(),
            Type::PtrTo(t) => ConcreteNames::ptr_type(&**t),
            Type::CStruct(p) => p.to_string(),
            Type::Named(p, args) => ConcreteNames::instantiated_type(&p.to_string(), args),
            Type::Var(name) => name.clone(),
            Type::Dyn(p, pins) => ConcreteNames::dyn_type(&p.to_string(), pins),
            Type::Fn(ps, rest, r) => ConcreteNames::fn_type(ps, rest.as_deref(), &**r),
        }
    }
}
