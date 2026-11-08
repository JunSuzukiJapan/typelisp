//! A sum type's value: `(Name::Variant value...)`, or the bare variant name
//! a `use` of the type brought in.

use crate::types::{path_is_builtin, Path, Type};
use crate::{Error, Heap, Loc, Value};

use super::expr::{agree, check_expr, Cx};
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
    if path_is_builtin(&owner, typelisp_mem::wk::SEXPR) {
        unimplemented!("typeck: a `Sexpr` value made from its variant")
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
    agree(cx, &ty, expected)?;
    let subst = Subst::positional(&def.params, &targs);
    let field_tys: Vec<Type> = v.fields.iter().map(|f| subst.apply(f)).collect();
    let mut checked = Vec::with_capacity(args.len());
    for ((arg, arg_loc), field_ty) in args.iter().zip(&field_tys) {
        checked.push(check_expr(cx, heap, locals, *arg, Some(field_ty), arg_loc.clone())?);
    }
    for (p, t) in def.params.iter().zip(&targs) {
        let what = format!("{}::{}: cannot infer type argument `{}` — no value or expected type decides it", owner, v.name, p.name.name());
        cx.must_decide(t.clone(), what, loc.clone());
    }
    Ok(Typed { node: Node::Construct { owner, targs, variant, mutable: false, field_tys, args: checked }, ty, loc })
}
