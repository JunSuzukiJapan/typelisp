//! Whether a `match`'s arms cover every value of the scrutinee's type, and
//! if not, a value they miss.
//!
//! The patterns are read as a matrix, one row per arm and one column per
//! value being matched, and a value no row matches is looked for column by
//! column (Maranget, "Warnings for pattern matching", 2007). A nested
//! pattern counts: `((node (leaf) r) ..)` does not cover every `node`.

use crate::check::registry::AdtKind;
use crate::types::Type;

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
    let heads: Vec<usize> = rows
        .iter()
        .filter_map(|r| match r[0] {
            Pat::Ctor { variant, .. } => Some(*variant),
            Pat::Wild | Pat::Bind(_) => None,
        })
        .collect();
    match variants {
        // Every variant is written in the column: a value is missed when
        // the rows for one variant miss one of its field values.
        Some(vs) if vs.iter().all(|(i, _, _)| heads.contains(i)) => vs.into_iter().find_map(|(i, name, fields)| {
            let arity = fields.len();
            let rows = specialize(wild, rows, i, arity);
            let tys: Vec<Type> = fields.into_iter().chain(rest_tys.iter().cloned()).collect();
            let mut w = missing(reg, wild, &rows, &tys)?;
            let rest = w.split_off(arity);
            let head = if w.is_empty() { format!("({})", name) } else { format!("({} {})", name, w.join(" ")) };
            Some(std::iter::once(head).chain(rest).collect())
        }),
        // Some variant, or every value of a type without variants, is
        // matched only by the rows that match anything in this column.
        _ => {
            let rows: Vec<Vec<&Pat>> = rows.iter().filter(|r| matches!(r[0], Pat::Wild | Pat::Bind(_))).map(|r| r[1..].to_vec()).collect();
            let w = missing(reg, wild, &rows, rest_tys)?;
            let head = match sum_variants(reg, ty).and_then(|vs| vs.into_iter().find(|(i, _, _)| !heads.contains(i))) {
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
fn specialize<'p>(wild: &'p Pat, rows: &[Vec<&'p Pat>], variant: usize, arity: usize) -> Vec<Vec<&'p Pat>> {
    rows.iter()
        .filter_map(|r| {
            let fields: Vec<&Pat> = match r[0] {
                Pat::Ctor { variant: v, subs, .. } if *v == variant => subs.iter().collect(),
                Pat::Ctor { .. } => return None,
                Pat::Wild | Pat::Bind(_) => vec![wild; arity],
            };
            Some(fields.into_iter().chain(r[1..].iter().copied()).collect())
        })
        .collect()
}

/// A sum type's variants: each one's index, name, and field types at `ty`'s
/// type arguments. `None` for any other type.
fn sum_variants(reg: &Registry, ty: &Type) -> Option<Vec<(usize, String, Vec<Type>)>> {
    let Type::Named(p, args) = ty else { return None };
    let def = reg.type_def(p).filter(|d| d.kind == AdtKind::Sum)?;
    let subst = Subst::positional(&def.params, args);
    Some(def.variants.iter().enumerate().map(|(i, v)| (i, v.name.name().to_string(), v.fields.iter().map(|f| subst.apply(f)).collect())).collect())
}
