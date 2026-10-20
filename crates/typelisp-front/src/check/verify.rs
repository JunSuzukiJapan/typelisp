//! Checking: the agreements the analysis recorded on the analyzed tree and
//! did not stop at (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).
//!
//! The analysis takes the type that was settled first and goes on; what it
//! could not reconcile stays in the tree as a node of its own. This reads
//! them back, after the expressions they sit in are analyzed.

use typelisp_mem::{Error, Loc};

use super::core::{Checked, Disagreement, Node};

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
        for child in c.node.children() {
            walk(child, loc, out);
        }
        if let Node::Disagree { what, value } = &c.node {
            let e = Error::TypeError(match what {
                Disagreement::Expected(expected) => format!("type mismatch: expected `{}`, found `{}`", expected, value.ty),
                Disagreement::Branches(a, b) => format!("branches have incompatible types: `{}` vs `{}`", a, b),
            });
            out.push(match loc {
                Some(l) => e.at(l.clone()),
                None => e,
            });
        }
    })
}
