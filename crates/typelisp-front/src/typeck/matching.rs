//! `match`: `(match expr (pattern body...)...)`.
//!
//! The arms are tried in order and the first whose pattern matches is run,
//! with the names its pattern binds. The arms' bodies have one type; one
//! that never returns (`!`) leaves it to the others. The arms must cover
//! every value of the scrutinee's type.

use crate::types::Type;
use crate::{wk, Error, Heap, Loc, Value};

use super::exhaustive::uncovered;
use super::expr::{check_body, check_expr, Cx};
use super::locals::Locals;
use super::pattern::check_arm_pattern;
use super::tree::{Node, Typed};

/// `(match ...)` — `args` without the `match`.
pub(crate) fn check_match(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let [(scrut, scrut_loc), arms @ ..] = args else {
        return Err(Error::TypeError("match: (match expr arms...)".into()));
    };
    if arms.is_empty() {
        return Err(Error::TypeError("match: no arms".into()));
    }
    let scrut = check_expr(cx, heap, locals, *scrut, None, scrut_loc.clone())?;
    let scrut_ty = cx.resolve(&scrut.ty);
    let result = match expected {
        Some(e) => e.clone(),
        None => cx.holes.borrow_mut().fresh(),
    };
    let mut checked = Vec::with_capacity(arms.len());
    let mut returns = false;
    for (arm, _) in arms {
        let parts = heap.list_to_vec_locs(*arm).ok().filter(|p| !p.is_empty());
        let Some(parts) = parts else {
            return Err(Error::TypeError("match: arm must be (pattern body...)".into()));
        };
        if matches!(parts.get(1), Some((Value::Symbol(k), _)) if k.is(wk::KW_WHEN)) {
            unimplemented!("typeck: a `match` arm with a `:when` guard")
        }
        let (pat, pat_loc) = &parts[0];
        let mut binds = Vec::new();
        let pat = check_arm_pattern(cx, heap, locals, &scrut_ty, *pat, &mut binds).map_err(|e| match heap.cons_loc(*pat).or(pat_loc.clone()) {
            Some(l) => e.at(l),
            None => e,
        })?;
        let arm_locals = locals.bind(binds)?;
        let (body, ty) = check_body(cx, heap, &arm_locals, &parts[1..], Some(&result))?;
        returns |= cx.resolve(&ty) != Type::Never;
        checked.push((pat, body));
    }
    let pats: Vec<_> = checked.iter().map(|(p, _)| p).collect();
    if let Some(missed) = uncovered(cx.reg, &scrut_ty, &pats) {
        return Err(Error::TypeError(format!(
            "non-exhaustive match on `{}`: `{}` is not covered — write an arm for it or a `_` arm",
            scrut_ty, missed
        )));
    }
    // Arms that all never return leave nothing to decide the type by.
    let ty = if returns { cx.resolve(&result) } else { Type::Never };
    Ok(Typed { node: Node::Match { scrut: Box::new(scrut), arms: checked }, ty, loc })
}
