//! Whether a `match`'s arms cover every value of the scrutinee's type, and
//! if not, a value they miss.
//!
//! The patterns are read as a matrix, one row per arm and one column per
//! value being matched, and a value no row matches is looked for column by
//! column (Maranget, "Warnings for pattern matching", 2007). A nested
//! pattern counts: `((node (leaf) r) ..)` does not cover every `node`.

use crate::check::analyze::{is_option_of_sexpr, OPTION_NONE};
use crate::check::registry::AdtKind;
use crate::sexpr_variant;
use crate::types::{path_is_builtin, Path, Type};
use crate::wk;

use super::registry::Registry;
use super::subst::Subst;
use super::tree::Pat;

/// A value of type `ty` that none of `arms` matches, written as a pattern,
/// or `None` when they cover every value.
pub(crate) fn uncovered(reg: &Registry, ty: &Type, arms: &[&Pat]) -> Option<String> {
    let wild = Pat::Wild;
    let rows: Vec<Vec<&Pat>> = arms.iter().map(|p| vec![*p]).collect();
    missing(reg, &wild, &rows, std::slice::from_ref(ty)).map(|mut w| w.remove(0))
}

/// A row of values, of the types `tys`, that no row of `rows` matches:
/// one pattern per column. `wild` is a `_`, for the columns a row fills in.
fn missing<'p>(reg: &Registry, wild: &'p Pat, rows: &[Vec<&'p Pat>], tys: &[Type]) -> Option<Vec<String>> {
    let Some((ty, rest_tys)) = tys.split_first() else {
        return if rows.is_empty() { Some(Vec::new()) } else { None };
    };
    let variants = sum_variants(reg, ty);
    let all = variants.clone().unwrap_or_default();
    let heads: Vec<usize> = rows.iter().filter_map(|r| cases(r[0], wild, &all)).flatten().map(|(i, _)| i).collect();
    match variants {
        // Every variant is written in the column: a value is missed when
        // the rows for one variant miss one of its field values.
        Some(vs) if vs.iter().all(|(i, _, _)| heads.contains(i)) => vs.into_iter().find_map(|(i, name, fields)| {
            let arity = fields.len();
            let rows = specialize(wild, rows, i, arity, &all);
            let tys: Vec<Type> = fields.into_iter().chain(rest_tys.iter().cloned()).collect();
            let mut w = missing(reg, wild, &rows, &tys)?;
            let rest = w.split_off(arity);
            let head = if *ty == Type::Bool { name } else if w.is_empty() { format!("({})", name) } else { format!("({} {})", name, w.join(" ")) };
            Some(std::iter::once(head).chain(rest).collect())
        }),
        // Some variant, or every value of a type without variants, is
        // matched only by the rows that match anything in this column.
        _ => {
            let rows: Vec<Vec<&Pat>> = rows.iter().filter(|r| cases(r[0], wild, &all).is_none()).map(|r| r[1..].to_vec()).collect();
            let w = missing(reg, wild, &rows, rest_tys)?;
            let head = match sum_variants(reg, ty).and_then(|vs| vs.into_iter().find(|(i, _, _)| !heads.contains(i))) {
                Some((_, name, _)) if *ty == Type::Bool => name,
                Some((_, name, fields)) if fields.is_empty() => format!("({})", name),
                Some((_, name, fields)) => format!("({} {})", name, vec!["_"; fields.len()].join(" ")),
                None => "_".to_string(),
            };
            Some(std::iter::once(head).chain(w).collect())
        }
    }
}

/// The rows that match the variant `variant` in the first column, that
/// column replaced with the variant's `arity` fields (`wild` for a row that
/// matches anything there).
fn specialize<'p>(wild: &'p Pat, rows: &[Vec<&'p Pat>], variant: usize, arity: usize, all: &[Variant]) -> Vec<Vec<&'p Pat>> {
    rows.iter()
        .filter_map(|r| {
            let fields: Vec<&Pat> = match cases(r[0], wild, all) {
                Some(cs) => cs.into_iter().find(|(v, _)| *v == variant)?.1,
                None => vec![wild; arity],
            };
            Some(fields.into_iter().chain(r[1..].iter().copied()).collect())
        })
        .collect()
}

/// The variants (of the column's `all`) the pattern `p` matches, each with
/// the patterns its fields are matched by — or `None` when it matches
/// anything.
///
/// In a column of `Option<Sexpr>`, `none` and `Sexpr`'s variants are its
/// variants, `none` in the slot `Sexpr`'s retired `nil` keeps: `(some p)`
/// is what `p` matches, and `(some _)` every `Sexpr` variant.
fn cases<'p>(p: &'p Pat, wild: &'p Pat, all: &[Variant]) -> Option<Vec<(usize, Vec<&'p Pat>)>> {
    let (owner, variant, subs) = match p {
        Pat::Wild | Pat::Bind(_) => return None,
        // `true` and `false` are a `bool`'s two values.
        Pat::Lit { value: crate::Value::Bool(b), .. } => return Some(vec![(usize::from(!*b), Vec::new())]),
        // Any other value matches only some values, and covers no variant.
        Pat::Lit { .. } | Pat::Guard { .. } => return Some(Vec::new()),
        // Out of a `Sexpr`, it matches only the values of one type.
        Pat::Ctor { downcast: true, .. } => return Some(Vec::new()),
        Pat::Ctor { owner, variant, subs, .. } => (owner, variant, subs),
    };
    let flat = all.iter().any(|(i, name, _)| *i == sexpr_variant::NIL && name == "none");
    if !flat || !path_is_builtin(owner, wk::OPTION) {
        return Some(vec![(*variant, subs.iter().collect())]);
    }
    if *variant == OPTION_NONE {
        return Some(vec![(sexpr_variant::NIL, Vec::new())]);
    }
    match &subs[0] {
        Pat::Ctor { downcast: true, .. } => Some(Vec::new()),
        Pat::Ctor { variant, subs, .. } => Some(vec![(*variant, subs.iter().collect())]),
        Pat::Wild | Pat::Bind(_) => Some(all.iter().filter(|(i, _, _)| *i != sexpr_variant::NIL).map(|(i, _, f)| (*i, vec![wild; f.len()])).collect()),
        Pat::Lit { .. } | Pat::Guard { .. } => Some(Vec::new()),
    }
}

/// A variant: its index, its name and its field types.
type Variant = (usize, String, Vec<Type>);

/// A sum type's variants, or a struct's one shape: each one's index, name,
/// and field types at `ty`'s type arguments. `None` for any other type.
///
/// `Sexpr`'s are the ones a pattern can write, without the retired `nil`
/// and `bignum`; an `Option<Sexpr>`'s are those and `none` (see [`cases`]).
fn sum_variants(reg: &Registry, ty: &Type) -> Option<Vec<Variant>> {
    let sexpr = || {
        let def = reg.type_def(&Path::builtin(wk::SEXPR)).expect("`Sexpr` is defined");
        def.variants
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != sexpr_variant::NIL && *i != sexpr_variant::RETIRED_BIGNUM)
            .map(|(i, v)| (i, v.name.name().to_string(), v.fields.clone()))
    };
    if is_option_of_sexpr(ty) {
        return Some(std::iter::once((sexpr_variant::NIL, "none".to_string(), Vec::new())).chain(sexpr()).collect());
    }
    if *ty == Type::Bool {
        return Some(vec![(0, "true".to_string(), Vec::new()), (1, "false".to_string(), Vec::new())]);
    }
    let Type::Named(p, args) = ty else { return None };
    if path_is_builtin(p, wk::SEXPR) {
        return Some(sexpr().collect());
    }
    let def = reg.type_def(p)?;
    let subst = Subst::positional(&def.params, args);
    // A struct is its one shape, its fields.
    if def.kind == AdtKind::Struct && !def.builtin {
        return Some(vec![(0, p.last_segment().to_string(), def.fields.iter().map(|f| subst.apply(&f.ty)).collect())]);
    }
    if def.kind != AdtKind::Sum {
        return None;
    }
    Some(def.variants.iter().enumerate().map(|(i, v)| (i, v.name.name().to_string(), v.fields.iter().map(|f| subst.apply(f)).collect())).collect())
}
