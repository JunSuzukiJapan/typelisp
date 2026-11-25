//! `size-of` and `size-of-val`: how many bytes a value of a type takes, as
//! C's `sizeof` says it.
//!
//! The answer is a constant, of the machine the code is for
//! (`typelisp_mem::target`). It is worked out when the form is lowered,
//! because that is when a type parameter's type is known.

use crate::types::Type;
use crate::{Error, Heap, Loc, Value};

use super::c_memory::{c_layout, c_struct_named};
use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::registry::Registry;
use super::tree::{Node, Typed};
use super::type_syntax::parse_raw;

/// `(size-of Type)` — `args` without the head. A type that is a value only
/// inside `unsafe` is named here anywhere: nothing is made of it.
pub(crate) fn check_size_of(cx: &Cx, heap: &Heap, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(ty, at)] = args else {
        return Err(Error::TypeError("size-of: (size-of Type)".into()));
    };
    let types = cx.types();
    let ty = match c_struct_named(heap, types.reg, types.ns, *ty) {
        Some(def) => Type::CStruct(def.name),
        None => parse_raw(heap, &types, *ty, at.as_ref())?,
    };
    Ok(Typed { node: Node::SizeOf { ty }, ty: Type::Int, loc })
}

/// `(size-of-val expr)` — `args` without the head. The expression is checked
/// for its type and not evaluated.
pub(crate) fn check_size_of_val(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(expr, at)] = args else {
        return Err(Error::TypeError("size-of-val: (size-of-val expr)".into()));
    };
    let value = check_expr(cx, heap, locals, *expr, None, at.clone())?;
    // A number literal no type decided has its default type.
    cx.decide(&value.ty);
    Ok(Typed { node: Node::SizeOf { ty: value.ty.clone() }, ty: Type::Int, loc })
}

/// How many bytes a value of `ty` takes. One that is in the heap, or may be
/// (an `int` as well), is a word that says where.
pub(crate) fn size_of(reg: &Registry, ty: &Type) -> Result<u64, Error> {
    Ok(match ty {
        Type::Never => return Err(Error::TypeError("size-of: `!` has no value".into())),
        Type::Unit => 0,
        Type::Char => 4,
        Type::I8 | Type::I16 | Type::I32 | Type::U8 | Type::U16 | Type::U32 | Type::F32 | Type::F64 | Type::Bool
        | Type::CLong | Type::CULong | Type::Ptr | Type::PtrTo(_) | Type::CStruct(_) => c_layout(reg, ty)?.0,
        _ => u64::from(typelisp_mem::target::DataModel::current().pointer_bytes),
    })
}
