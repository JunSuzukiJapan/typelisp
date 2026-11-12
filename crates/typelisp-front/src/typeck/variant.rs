//! A sum type's value: `(Name::Variant value...)`, or the bare variant name
//! a `use` of the type brought in.


use crate::types::{Path, Type};
use crate::{Error, Heap, Loc, Value};

use super::expr::{check_expr, Cx};
use super::widen::expect;
use super::locals::Locals;
use super::resolve::sees;
use super::subst::Subst;
use super::tree::{Node, Typed};

/// The variant `variant` of the sum type `owner`, its fields' values `args`.
///
/// The type's parameters are fresh holes, solved by what is expected first,
/// then by the values, each checked where its field's type is expected, and
/// at the latest by the rest of the form.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_variant(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    owner: Path,
    variant: usize,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    // `Sexpr`'s retired variants keep their places, so that the others keep
    // their numbers.
    if is_retired_nil(&owner, variant) {
        return Err(Error::TypeError(NIL_IS_GONE.to_string()));
    }
    if is_retired_bignum(&owner, variant) {
        return Err(Error::TypeError(BIGNUM_IS_GONE.to_string()));
    }
    let def = cx.reg.type_def(&owner).expect("a variant's type is defined");
    if !def.public && !sees(cx.ns, owner.ns()) {
        return Err(Error::TypeError(format!("`{}` is not `pub`, so it is not visible outside its module", owner)));
    }
    let v = &def.variants[variant];
    if args.len() != v.fields.len() {
        return Err(Error::TypeError(format!("{}::{}: expected {} field(s), got {}", owner, v.name, v.fields.len(), args.len())));
    }
    let targs: Vec<Type> = def.params.iter().map(|_| cx.holes.borrow_mut().fresh()).collect();
    let ty = Type::Named(owner, targs.clone());
    expect(cx, &ty, expected)?;
    let subst = Subst::positional(&def.params, &targs);
    let field_tys: Vec<Type> = v.fields.iter().map(|f| subst.apply(f)).collect();
    let mut checked = Vec::with_capacity(args.len());
    for ((arg, arg_loc), field_ty) in args.iter().zip(&field_tys) {
        checked.push(check_expr(cx, heap, locals, *arg, Some(field_ty), arg_loc.clone())?);
    }
    for (p, t) in def.params.iter().zip(&targs) {
        let what = format!("cannot infer type argument `{}` for `{}`", p.name.name(), owner);
        cx.must_decide(t.clone(), what, loc.clone());
    }
    Ok(Typed { node: Node::Construct { owner, targs, variant, mutable: false, field_tys, args: checked }, ty, loc })
}

/// `Sexpr`'s `nil` variant — the old empty list.
///
/// The variant is still *in* `sexpr_def`, and deliberately so: its number is
/// burned into the island's IR and into compiled code (`crate::sexpr_variant`
/// says why numbers are only ever appended). Removing the entry would
/// renumber every variant after it.
///
/// What is gone is its *surface*: `(nil)` can no longer be written as a
/// constructor or a pattern, because a writable empty-list `Sexpr` would
/// make `Option<Sexpr>`'s niche unsound — `(Option::some (nil))` and
/// `(Option::none)` would be the same word. The empty list is reached as
/// `Option<Sexpr>`'s `none`, and the core IR still spells it `(construct
/// sexpr 0 ..)`, which the checker emits itself.
pub(crate) const SEXPR_RESERVED_VARIANT: usize = crate::sexpr_variant::NIL;

/// `Sexpr`'s old `bignum` variant, retired with the type: a bignum box is an
/// `int` now. Same treatment as [`SEXPR_RESERVED_VARIANT`]:
/// the slot stays so nothing renumbers, and its surface is refused.
const SEXPR_RETIRED_BIGNUM: usize = crate::sexpr_variant::RETIRED_BIGNUM;

/// Message for a `(bignum ..)` constructor or pattern.
pub(crate) const BIGNUM_IS_GONE: &str = "`bignum` is no longer a `Sexpr` constructor: the `bignum` type was folded \
     into `int`, so a big integer is `(int n)` like any other.";

/// Message for a `(nil)` that survived in source somewhere.
pub(crate) const NIL_IS_GONE: &str = "`nil` is no longer a `Sexpr` constructor: the empty list is \
     `Option<Sexpr>`'s `none`. Write `()` where an `Option<Sexpr>` is expected, or \
     `(Option::none)`; match it with `((none) ...)`.";

/// Is `(adt, variant)` the retired `Sexpr::nil`?
pub(crate) fn is_retired_nil(adt: &Path, variant: usize) -> bool {
    variant == SEXPR_RESERVED_VARIANT && crate::types::path_is_builtin(adt, typelisp_mem::wk::SEXPR)
}

/// Is `(adt, variant)` the retired `Sexpr::bignum`?
pub(crate) fn is_retired_bignum(adt: &Path, variant: usize) -> bool {
    variant == SEXPR_RETIRED_BIGNUM && crate::types::path_is_builtin(adt, typelisp_mem::wk::SEXPR)
}
