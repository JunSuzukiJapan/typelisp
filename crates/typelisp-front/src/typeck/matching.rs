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
use super::pattern::{check_arm_pattern, Binds};
use super::tree::{Node, Pat, Typed};

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
    // A trait object is matched as the value it holds, by the `Sexpr`
    // downcast patterns: its implementing types are open, so a catch-all
    // arm is needed, as for a `Sexpr`.
    let scrut = match cx.resolve(&scrut.ty) {
        Type::Dyn(..) => {
            let loc = scrut.loc.clone();
            Typed { node: Node::DynValue { value: Box::new(scrut) }, ty: crate::check::analyze::sexpr_ty(), loc }
        }
        _ => scrut,
    };
    let scrut_ty = cx.resolve(&scrut.ty);
    let result = match expected {
        Some(e) => e.clone(),
        None => cx.holes.borrow_mut().fresh(),
    };
    let mut checked = Vec::with_capacity(arms.len());
    let mut returns = false;
    // In recover mode an arm that fails is left out, and with it the check
    // that the arms cover every value, which would only repeat the error.
    let mut skipped = false;
    for (arm, arm_loc) in arms {
        match check_arm(cx, heap, locals, &scrut_ty, &result, *arm) {
            Ok((pat, body, ty)) => {
                returns |= cx.resolve(&ty) != Type::Never;
                checked.push((pat, body));
            }
            Err(e) => {
                cx.recover(e, heap.cons_loc(*arm).or(arm_loc.clone()))?;
                skipped = true;
            }
        }
    }
    let pats: Vec<_> = checked.iter().map(|(p, _)| p).collect();
    if let Some(missed) = uncovered(cx.reg, &scrut_ty, &pats).filter(|_| !skipped) {
        let e = Error::TypeError(format!(
            "non-exhaustive match on `{}`: `{}` is not covered — write an arm for it or a `_` arm",
            scrut_ty, missed
        ));
        cx.recover(e, loc.clone())?;
    }
    // Arms that all never return leave nothing to decide the type by.
    let ty = if returns { cx.resolve(&result) } else { Type::Never };
    Ok(Typed { node: Node::Match { scrut: Box::new(scrut), arms: checked }, ty, loc })
}

/// The arm `arm` of a `match` on a `scrut_ty`, whose body is to have the
/// type `result`: its pattern, its body and the body's type.
fn check_arm(cx: &Cx, heap: &mut Heap, locals: &Locals, scrut_ty: &Type, result: &Type, arm: Value) -> Result<(Pat, Vec<Typed>, Type), Error> {
    let parts = heap.list_to_vec_locs(arm).ok().filter(|p| !p.is_empty());
    let Some(parts) = parts else {
        return Err(Error::TypeError("match: arm must be (pattern body...)".into()));
    };
    // `(pattern :when test body...)`: the condition sits between the
    // pattern and the body.
    let guard = match parts.get(1) {
        Some((Value::Symbol(k), _)) if k.is(wk::KW_WHEN) => match parts.get(2) {
            Some(test) => Some(test.clone()),
            None => return Err(Error::TypeError("match: `:when` needs a condition: (pattern :when test body...)".into())),
        },
        _ => None,
    };
    let body_start = if guard.is_some() { 3 } else { 1 };
    let (pat, pat_loc) = &parts[0];
    let where_ = heap.cons_loc(*pat).or(pat_loc.clone());
    let at = |e: Error| match &where_ {
        Some(l) => e.at(l.clone()),
        None => e,
    };
    // `(:or p1 p2 ...)`, anywhere in the pattern, is the whole patterns
    // it stands for, which one body reads the names of.
    let alternatives = crate::check::analyze::expand_or_patterns(heap, *pat).map_err(at)?;
    let mut pats = Vec::with_capacity(alternatives.len());
    let mut first: Option<Binds> = None;
    for alt in alternatives {
        let mut binds = Vec::new();
        pats.push(check_arm_pattern(cx, heap, locals, scrut_ty, alt, &mut binds).map_err(at)?);
        match &first {
            None => first = Some(binds),
            Some(first) => same_binds(cx, first, &binds).map_err(at)?,
        }
    }
    let arm_locals = locals.bind(first.expect("a pattern has an alternative"))?;
    let pat = if pats.len() == 1 { pats.remove(0) } else { Pat::Or(pats) };
    let pat = match guard {
        Some((test, test_loc)) => {
            let test = check_expr(cx, heap, &arm_locals, test, Some(&Type::Bool), test_loc)?;
            Pat::When { pat: Box::new(pat), test: Box::new(test) }
        }
        None => pat,
    };
    let (body, ty) = check_body(cx, heap, &arm_locals, &parts[body_start..], Some(result))?;
    Ok((pat, body, ty))
}

/// That two `:or` alternatives bind the same names at the same types.
fn same_binds(cx: &Cx, first: &Binds, other: &Binds) -> Result<(), Error> {
    let key = |b: &Binds| {
        let mut v: Vec<(String, Type)> = b.iter().map(|(n, t)| (n.name().to_string(), cx.resolve(t))).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };
    let (a, b) = (key(first), key(other));
    if a == b {
        return Ok(());
    }
    let show = |v: &[(String, Type)]| {
        if v.is_empty() {
            "nothing".to_string()
        } else {
            v.iter().map(|(n, t)| format!("`{}: {}`", n, t)).collect::<Vec<_>>().join(", ")
        }
    };
    Err(Error::TypeError(format!(
        "pattern: the alternatives of `(:or ...)` must bind the same variables at the same types, but one binds {} and another {}",
        show(&a),
        show(&b)
    )))
}
