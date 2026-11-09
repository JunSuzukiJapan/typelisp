//! Checking a `match` arm's pattern against the type of the value it
//! matches.
//!
//! `_` matches anything. A name matches anything and binds it, except that
//! a name that is a variant of the type being matched is that variant. A
//! list `(variant pattern...)` is a variant of the type being matched, its
//! fields matched by the patterns; the variant is named bare, since the
//! type is the one being matched.

use crate::check::analyze::{is_option_of_sexpr, is_sexpr_expectation, BIGNUM_IS_GONE, NIL_IS_GONE};
use crate::sexpr_variant::{self, is_ctor_name};
use crate::check::registry::AdtKind;
use crate::types::{path_is_builtin, Path, Type};
use crate::{wk, Error, Heap, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::expr::Cx;
use super::registry::TypeDef;
use super::resolve::{ctor_named, find_named};
use super::subst::Subst;
use super::tree::Pat;

/// The names a pattern binds, with their types, in the order written.
pub(crate) type Binds = Vec<(SymRef, Type)>;

/// An arm's whole pattern, matching a value of type `expected`.
///
/// A bare name here is always a variant: `_` is the one pattern that
/// matches every value, so a misspelled variant is an error rather than an
/// arm that binds a variable, matches everything and leaves the arms after
/// it unreachable.
pub(crate) fn check_arm_pattern(cx: &Cx, heap: &Heap, expected: &Type, v: Value, binds: &mut Binds) -> Result<Pat, Error> {
    if let Value::Symbol(id) = v {
        if !id.is(wk::UNDERSCORE) && !id.is_keyword() {
            return match bare_variant(cx, heap, expected, id, binds)? {
                Some(pat) => Ok(pat),
                None => Err(Error::TypeError(format!(
                    "pattern: `{}` is not a variant of `{}` — a bare name in a `match` arm is a variant; only `_` matches every value",
                    heap.symbol_spelling(id),
                    cx.resolve(expected)
                ))),
            };
        }
    }
    check_pattern(cx, heap, expected, v, binds)
}

/// A pattern inside an arm's, matching a value of type `expected`.
fn check_pattern(cx: &Cx, heap: &Heap, expected: &Type, v: Value, binds: &mut Binds) -> Result<Pat, Error> {
    match v {
        Value::Symbol(id) if id.is(wk::UNDERSCORE) => Ok(Pat::Wild),
        Value::Symbol(id) if id.is_keyword() => unimplemented!("typeck: a keyword as a pattern"),
        Value::Symbol(id) => {
            if let Some(pat) = bare_variant(cx, heap, expected, id, binds)? {
                return Ok(pat);
            }
            if binds.iter().any(|(n, _)| *n == id) {
                return Err(Error::TypeError(format!("pattern: `{}` is bound twice", heap.symbol_spelling(id))));
            }
            binds.push((id, expected.clone()));
            Ok(Pat::Bind(id))
        }
        Value::Cons(_) => {
            let parts = heap.list_to_vec(v)?;
            let head = match parts[0] {
                Value::Symbol(h) if h.is_keyword() || h.is(wk::QUOTE) || h.is(wk::THE) => {
                    unimplemented!("typeck: the pattern `({} ...)`", heap.symbol_spelling(h))
                }
                Value::Symbol(h) => h,
                _ => return Err(Error::TypeError("pattern: constructor must be a symbol".into())),
            };
            let ty = cx.resolve(expected);
            if is_sexpr_expectation(&ty) && !option_variant(cx, &ty, head) {
                if !is_ctor_name(head.name()) && names_type(cx, head) {
                    unimplemented!("typeck: a downcast pattern")
                }
                // An `Option<Sexpr>` is matched by `Sexpr`'s variants as if
                // its `some` were peeled, beside `none` in the same arms.
                if is_option_of_sexpr(&ty) && is_ctor_name(head.name()) {
                    return check_pattern(cx, heap, &sexpr_ty(), v, binds);
                }
            }
            let (owner, targs, def) = sum_type(cx, expected, || format!("({} ...)", heap.symbol_spelling(head)))?;
            let Some(variant) = def.variant_index(member_symbol(head.name())) else {
                return Err(Error::TypeError(format!("`{}` is not a constructor of `{}`", heap.symbol_spelling(head), owner)));
            };
            if path_is_builtin(&owner, wk::SEXPR) {
                match variant {
                    sexpr_variant::NIL => return Err(Error::TypeError(NIL_IS_GONE.into())),
                    sexpr_variant::RETIRED_BIGNUM => return Err(Error::TypeError(BIGNUM_IS_GONE.into())),
                    _ => {}
                }
            }
            variant_pattern(cx, heap, owner, targs, def, variant, &parts[1..], binds)
        }
        Value::Int(_) | Value::Bool(_) | Value::Char(_) | Value::Str(_) | Value::Boxed(_) => {
            unimplemented!("typeck: a value pattern")
        }
        other => Err(Error::TypeError(format!("pattern does not match `{}`: `{:?}`", cx.resolve(expected), other))),
    }
}

/// The variant `id` names of the sum type `expected` is, as a pattern — or
/// `None` when `expected` is not a sum type or has no such variant.
fn bare_variant(cx: &Cx, heap: &Heap, expected: &Type, id: SymRef, binds: &mut Binds) -> Result<Option<Pat>, Error> {
    let ty = cx.resolve(expected);
    let Type::Named(owner, targs) = &ty else { return Ok(None) };
    if is_sexpr_expectation(&ty) && !option_variant(cx, &ty, id) {
        // Against a `Sexpr`, a type's or a variant's name is a downcast.
        if names_type(cx, id) {
            unimplemented!("typeck: a downcast pattern")
        }
        return Ok(None);
    }
    let Some(def) = cx.reg.type_def(owner).filter(|d| d.kind == AdtKind::Sum) else { return Ok(None) };
    let Some(variant) = def.variant_index(member_symbol(id.name())) else { return Ok(None) };
    variant_pattern(cx, heap, *owner, targs.clone(), def, variant, &[], binds).map(Some)
}

/// Whether `id` names a type, or a variant a `use` brought in: against a
/// `Sexpr`, a pattern headed by one is a downcast.
fn names_type(cx: &Cx, id: SymRef) -> bool {
    find_named(cx.reg, cx.ns, id, |ns, key| cx.reg.types.get(&Path::of_symbol(ns, key))).is_some() || ctor_named(cx.reg, cx.ns, id).is_some()
}

/// Whether `id` is a variant of `ty` when it is `Option<Sexpr>`: `some` and
/// `none` are its own, not downcasts.
fn option_variant(cx: &Cx, ty: &Type, id: SymRef) -> bool {
    is_option_of_sexpr(ty) && cx.reg.type_def(&Path::builtin(wk::OPTION)).is_some_and(|d| d.variant_index(member_symbol(id.name())).is_some())
}

/// `Sexpr`.
fn sexpr_ty() -> Type {
    Type::Named(Path::builtin(wk::SEXPR), Vec::new())
}

/// The sum type a pattern `what` (for the message) matches a value of, with
/// its type arguments and definition.
fn sum_type<'r>(cx: &Cx<'r>, expected: &Type, what: impl Fn() -> String) -> Result<(Path, Vec<Type>, &'r TypeDef), Error> {
    let ty = cx.resolve(expected);
    if let Type::Named(p, args) = &ty {
        if let Some(def) = cx.reg.type_def(p) {
            return match def.kind {
                AdtKind::Sum => Ok((*p, args.clone(), def)),
                AdtKind::Struct => unimplemented!("typeck: a struct pattern"),
            };
        }
    }
    if let Type::Dyn(..) = ty {
        unimplemented!("typeck: matching a trait object")
    }
    Err(Error::TypeError(format!("pattern `{}` does not match a value of type `{}`", what(), ty)))
}

/// The variant `variant` of `owner<targs>`, its fields matched by `subs`.
#[allow(clippy::too_many_arguments)]
fn variant_pattern(
    cx: &Cx,
    heap: &Heap,
    owner: Path,
    targs: Vec<Type>,
    def: &TypeDef,
    variant: usize,
    subs: &[Value],
    binds: &mut Binds,
) -> Result<Pat, Error> {
    let v = &def.variants[variant];
    if subs.len() != v.fields.len() {
        return Err(Error::TypeError(format!("pattern `{}`: expected {} field(s), got {}", v.name, v.fields.len(), subs.len())));
    }
    let subst = Subst::positional(&def.params, &targs);
    let field_tys: Vec<Type> = v.fields.iter().map(|f| subst.apply(f)).collect();
    let subs = subs.iter().zip(&field_tys).map(|(s, t)| check_pattern(cx, heap, t, *s, binds)).collect::<Result<Vec<_>, _>>()?;
    Ok(Pat::Ctor { owner, targs, variant, field_tys, subs })
}
