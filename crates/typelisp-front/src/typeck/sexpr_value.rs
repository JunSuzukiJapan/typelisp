//! A value of its own type held as a `Sexpr`: how each type is encoded.
//!
//! `format`'s arguments keep their own types and are handed to the engine
//! as one `Sexpr` list, so each is encoded by its type. The type may be one
//! the rest of the form decides, or a type parameter of a generic body, so
//! the encoding is chosen when the tree is lowered, from the settled type.

use crate::check::analyze::{is_sexpr_expectation, sexpr_ctor_for};
use crate::check::lower::CoreForms;
use crate::check::registry::AdtKind;
use crate::types::{path_is_builtin, Type};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::wk;

use super::registry::Registry;

/// How a value of one type becomes a `Sexpr`.
pub(crate) enum Encoding {
    /// Its word is a `Sexpr` already.
    Itself,
    /// A niched `Option`, which has no box of its own: one is made.
    BoxOption,
    /// Through the `Sexpr` variant of this index, whose field is the value.
    Variant(usize),
}

/// How a value of the settled type `ty` becomes a `Sexpr`, or `None` when
/// it has no encoding.
pub(crate) fn encoding(reg: &Registry, ty: &Type) -> Option<Encoding> {
    if is_sexpr_expectation(ty) {
        return Some(Encoding::Itself);
    }
    // A float is a heap value to the interpreter only: compiled, it is the
    // raw bits of the number, so it goes through its variant, which boxes it.
    if heap_value(reg, ty) && !ty.is_float() {
        return Some(match reg.repr(ty).niche_payload() {
            Some(_) => Encoding::BoxOption,
            None => Encoding::Itself,
        });
    }
    let name = sexpr_ctor_for(ty)?;
    let sexpr = reg.type_def(&crate::types::Path::builtin(wk::SEXPR)).expect("`Sexpr` is built in");
    Some(Encoding::Variant(sexpr.variant_index(member_symbol(name)).expect("`Sexpr` has a variant for each type it encodes")))
}

/// Whether a value of type `ty`, a type parameter of a generic body
/// included, can be held as a `Sexpr`. A type parameter's encoding is its
/// specialization's.
pub(crate) fn encodable(reg: &Registry, ty: &Type) -> bool {
    matches!(ty, Type::Param(..)) || encoding(reg, ty).is_some()
}

/// The error for a value of type `ty`, which has no `Sexpr` encoding.
pub(crate) fn no_encoding(ty: &Type) -> crate::Error {
    crate::Error::TypeError(format!(
        "&rest: element type `{}` has no Sexpr encoding (use a number, bool/char/string/Sexpr)",
        ty
    ))
}

/// Whether a value of type `ty` is a reference to the heap whose word a
/// `Sexpr` holds as it is.
fn heap_value(reg: &Registry, ty: &Type) -> bool {
    match ty {
        Type::Named(p, _) if path_is_builtin(p, wk::SCOPE) || path_is_builtin(p, wk::SEXPR) || path_is_builtin(p, wk::HASHTABLE) => true,
        Type::Named(p, _) => match reg.type_def(p) {
            Some(d) => d.kind == AdtKind::Struct || (d.kind == AdtKind::Sum && !d.variants.is_empty()),
            None => false,
        },
        Type::Dyn(..) => true,
        Type::RandomState | Type::Int | Type::Ratio | Type::Rational | Type::F64 | Type::Str => true,
        _ => false,
    }
}
