//! A value of its own type held as a `Sexpr`: how each type is encoded.
//!
//! `format`'s arguments keep their own types and are handed to the engine
//! as one `Sexpr` list, so each is encoded by its type. The type may be one
//! the rest of the form decides, or a type parameter of a generic body, so
//! the encoding is chosen when the tree is lowered, from the settled type.


use crate::types::Path;
use crate::check::lower::CoreForms;
use crate::typeck::registry::AdtKind;
use crate::types::{path_is_builtin, Type};
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
    sexpr_variant_for(ty).map(Encoding::Variant)
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
pub(crate) fn heap_value(reg: &Registry, ty: &Type) -> bool {
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

/// `Sexpr` — the type a `&rest` parameter's collected arguments are bound as
/// inside a `defun`/`lambda` body, and the type `apply`'s trailing list
/// argument must have (see `typeck::lambda_list`/`check_call`/
/// `check_apply_form`). Every Lisp's `&rest` parameter is an ordinary list
/// (built from cons cells), never a homogeneous array type — `defmacro`'s own
/// `&rest` already works this way unconditionally (`Interp::bind_macro_args`);
/// `defun`/`lambda`'s `&rest` now matches it, just with an added call-site
/// check (below) that every individual variadic argument satisfies the
/// declared element type before it's wrapped into the list.
pub(crate) fn sexpr_ty() -> Type {
    Type::Named(Path::builtin(typelisp_mem::wk::SEXPR), vec![])
}

/// `Option<Sexpr>` — the type an S-expression datum has once the empty list
/// is `None` (see [`is_option_of_sexpr`]).
pub(crate) fn option_of_sexpr() -> Type {
    Type::Named(Path::builtin(typelisp_mem::wk::OPTION), vec![sexpr_ty()])
}

/// Does an expected type ask for an S-expression?
///
/// Both spellings answer yes. `Option<Sexpr>` is what a datum has, and it is
/// what the implicit widenings should fire on — a `Symbol`, a heap value or a
/// `:dyn` reaching "an S-expression is wanted here" is the same rule
/// whichever of the two the site declared. Bare `Sexpr` still means
/// *non-empty*, and a widening into it is just as sound: nothing these rules
/// produce is the empty list.
pub(crate) fn is_sexpr_expectation(ty: &Type) -> bool {
    *ty == sexpr_ty() || is_option_of_sexpr(ty)
}

/// Is `ty` exactly `Option<Sexpr>` — the type the empty list's niche belongs
/// to (`check/repr.rs`)?
///
/// One predicate because three decisions have to agree on the same boundary:
/// how the value is represented, how its constructors lower, and which
/// patterns a `match` on it accepts. Were any of them drawn wider — around
/// "an `Option` whose payload *represents* like a `Sexpr`", which
/// `Option<Option<Sexpr>>` does — the outer level would claim the same
/// empty-list word as the inner one and the two `None`s would collide.
pub(crate) fn is_option_of_sexpr(ty: &Type) -> bool {
    matches!(ty, Type::Named(p, args)
        if crate::types::path_is_builtin(p, typelisp_mem::wk::OPTION)
            && args.len() == 1
            && matches!(&args[0], Type::Named(a, _) if crate::types::path_is_builtin(a, typelisp_mem::wk::SEXPR)))
}

/// The `Sexpr` variant that exactly represents a value of `elem_ty`
/// (`Sexpr` itself needs no wrapping — see `typeck::sexpr_value`).
/// `&rest`'s declared element type must be one of these (or `Sexpr`) since
/// nothing else has a lossless `Sexpr` encoding to collect into a list with.
pub(crate) fn sexpr_variant_for(elem_ty: &Type) -> Option<usize> {
    use crate::sexpr_variant as sv;
    match elem_ty {
        // One variant per width. There is no folding left to do: `Sexpr` has
        // an `I8` and a `U32` because the types do, and a value that arrives
        // here as a `u32` comes back out of the `Sexpr` as a `u32`.
        Type::I8 => Some(sv::I8),
        Type::I16 => Some(sv::I16),
        Type::I32 => Some(sv::I32),
        Type::U8 => Some(sv::U8),
        Type::U16 => Some(sv::U16),
        Type::U32 => Some(sv::U32),
        Type::F32 => Some(sv::F32),
        Type::F64 => Some(sv::F64),
        Type::Int => Some(sv::INT),
        Type::Ratio => Some(sv::RATIO),
        Type::Char => Some(sv::CHAR),
        Type::Bool => Some(sv::BOOL),
        Type::Str => Some(sv::STR),
        Type::Symbol => Some(sv::SYM),
        _ => None,
    }
}
