//! Checking: the agreements the analysis recorded on the analyzed tree and
//! did not stop at (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).
//!
//! The analysis takes the type that was settled first and goes on; what it
//! could not reconcile stays in the tree as a node of its own. This reads
//! them back, after the expressions they sit in are analyzed.

use typelisp_mem::{Error, Loc};

use super::core::{Checked, Disagreement, Node, Unmet};

/// The disagreements in `c`, each at the innermost position written for it,
/// in the order the analysis met them: a subexpression's before the
/// expression around it, and left to right among siblings.
pub(super) fn disagreements(c: &Checked) -> Vec<Error> {
    let mut out = Vec::new();
    walk(c, None, &mut out);
    out
}

fn walk(c: &Checked, outer: Option<&Loc>, out: &mut Vec<Error>) {
    typelisp_mem::grow_stack(|| {
        let loc = c.loc.as_ref().or(outer);
        let before = out.len();
        for child in c.node.children() {
            walk(child, loc, out);
        }
        if let Node::Disagree { what, value } = &c.node {
            let e = Error::TypeError(match what {
                Disagreement::Expected(expected) => format!("type mismatch: expected `{}`, found `{}`", expected, value.ty),
                Disagreement::Branches(a, b) => format!("branches have incompatible types: `{}` vs `{}`", a, b),
                Disagreement::Mismatch(expected, found) => format!("type mismatch: expected `{}`, found `{}`", expected, found),
                Disagreement::Conflict(param, a, b) => format!("conflicting types for `{}`: `{}` vs `{}`", param, a, b),
                // An argument that disagrees decides nothing; that is what
                // there is to report.
                Disagreement::Uninferred { .. } if out.len() > before => return,
                Disagreement::Uninferred { noun, param, of } => format!("cannot infer {} `{}` for `{}`", noun, param, of),
                Disagreement::Bound { of, param, unmet } => match unmet {
                    Unmet::Undeclared { var, trait_path } => format!(
                        "{}: type parameter `{}` is only known here as the enclosing function's own `{}`, \
                         which has no `where` bound declaring it implements `{}` — the enclosing function's \
                         signature needs a matching `:where` clause on `{}`",
                        of, param, var, trait_path, var
                    ),
                    Unmet::Unimplemented { ty, trait_path } => format!(
                        "{}: type `{}` does not implement trait `{}` required by `:where` clause on type parameter `{}`",
                        of, ty, trait_path, param
                    ),
                    Unmet::NoImpls { ty } => format!(
                        "{}: type `{}` does not implement a trait required by `:where` clause on type parameter `{}`",
                        of, ty, param
                    ),
                    Unmet::Assoc { ty, assoc, actual, required } => format!(
                        "{}: type `{}`'s associated type `{}` is `{}`, but `:where` clause requires `{}`",
                        of,
                        ty,
                        assoc,
                        actual.as_ref().map(|t| t.to_string()).unwrap_or_else(|| "undefined".to_string()),
                        required
                    ),
                },
            });
            out.push(match loc {
                Some(l) => e.at(l.clone()),
                None => e,
            });
        }
    })
}
