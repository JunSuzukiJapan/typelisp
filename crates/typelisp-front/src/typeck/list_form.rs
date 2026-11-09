//! `(list item...)`: a list of S-expressions, itself one.
//!
//! Each item is expected to be an `Option<Sexpr>`, so any value with a
//! `Sexpr` encoding may be one ([`super::widen`]). The list is built from
//! `Sexpr`'s cons variant, ending in the empty list.

use crate::check::analyze::{option_of_sexpr, SEXPR_RESERVED_VARIANT};
use crate::sexpr_variant::CONS;
use crate::types::Path;
use crate::{Error, Heap, Loc, Value};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// `(list item...)` — `args` without the `list`.
pub(crate) fn check_list(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let list_ty = option_of_sexpr();
    let sexpr = Path::builtin(typelisp_mem::wk::SEXPR);
    let cons_tys = cx.reg.type_def(&sexpr).expect("`Sexpr` is defined").variants[CONS].fields.clone();
    let mut items = Vec::with_capacity(args.len());
    for (item, item_loc) in args {
        items.push(check_expr(cx, heap, locals, *item, Some(&list_ty), item_loc.clone())?);
    }
    let nil = Node::Construct { owner: sexpr, targs: Vec::new(), variant: SEXPR_RESERVED_VARIANT, mutable: false, field_tys: Vec::new(), args: Vec::new() };
    let mut acc = Typed { node: nil, ty: list_ty.clone(), loc: None };
    for item in items.into_iter().rev() {
        let node = Node::Construct { owner: sexpr, targs: Vec::new(), variant: CONS, mutable: false, field_tys: cons_tys.clone(), args: vec![item, acc] };
        acc = Typed { node, ty: list_ty.clone(), loc: None };
    }
    acc.loc = loc;
    Ok(acc)
}
