//! Checking: the agreements the analysis recorded on the analyzed tree and
//! did not stop at (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).
//!
//! The analysis takes the type that was settled first and goes on; what it
//! could not reconcile stays in the tree as a node of its own. This reads
//! them back, after the expressions they sit in are analyzed.
//!
//! A definition and what declared it ahead are checked here too, once the
//! definition is analyzed.

use typelisp_mem::{ConcreteNames, Error, Loc};

use crate::types::Type;

use super::core::{Checked, Disagreement, Node, Uncovered, Unmet, Unsafe};
use super::registry::FnSig;

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
                Disagreement::Uncovered(Uncovered::Variants { adt, covered, total }) => {
                    format!("non-exhaustive match on `{}`: {}/{} variants covered", adt, covered, total)
                }
                Disagreement::Uncovered(Uncovered::Bool { missing }) => {
                    format!("non-exhaustive match on Bool: `{}` is not covered — write that arm or a `_` arm", missing)
                }
                Disagreement::Uncovered(Uncovered::ByValue { ty }) => format!(
                    "non-exhaustive match on `{}`: a type with no variants can only be matched \
                     by value, so a `_` arm is required",
                    ty
                ),
                Disagreement::HiddenSlot { ty, slot, module } => format!(
                    "make-instance {}: slot `{}` is not `pub`, so it can be given only inside module `{}`",
                    ty, slot, module
                ),
                Disagreement::Unsafe(Unsafe::CCall { name }) => format!(
                    "`{}` is a C function declared by `defffi`, so calling it needs `(unsafe ...)`. \
                     Whether its declared signature is the one the C function really has is not \
                     something this compiler can check — `unsafe` is where that is taken on. Wrap the \
                     call, or wrap it once inside a `defun` that offers a checked signature.",
                    name
                ),
                Disagreement::Unsafe(Unsafe::RawWord { ty }) => format!(
                    "a `{}` is a raw machine word from the C FFI and can only be a value \
                     inside `(unsafe ...)` — nothing here can check that it points at anything",
                    ConcreteNames::type_key(ty)
                ),
                Disagreement::Unsafe(Unsafe::TypedPtrAnswer { ty }) => format!(
                    "an `(unsafe ...)` cannot answer `{}`: the memory a typed pointer points at is freed \
                     when the `unsafe` that allocated it is left. Copy what is needed into ordinary \
                     values (a `defstruct`, numbers) inside the `unsafe`, and answer those.",
                    ty
                ),
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

/// Rejects a `defun` that disagrees with the `defsignature` that promised it.
///
/// A declaration is only worth writing if it is binding: a call checked
/// against the declaration and then dispatched to a definition of another
/// shape is exactly the kind of quiet mismatch static types exist to stop.
/// The old implicit pre-pass could not do this — it *derived* the signature
/// from the definition, so the two agreed by construction.
///
/// Docstrings, parameter *names* and the body play no part: a signature is
/// the types, `pub`, and the `&rest` tail.
pub(super) fn signature_agrees(name: &str, declared: &FnSig, defined: &FnSig) -> Result<(), Error> {
    let show_rest = |r: &Option<Type>| match r {
        Some(t) => format!("`&rest {}`", t),
        None => "none".to_string(),
    };
    let mismatch = |what: &str, decl: String, def: String| {
        Err(Error::TypeError(format!(
            "`{}` does not match its `defsignature`: {} declared as {}, defined as {}",
            name, what, decl, def
        )))
    };
    if declared.params.len() != defined.params.len() {
        return mismatch(
            "parameter count",
            declared.params.len().to_string(),
            defined.params.len().to_string(),
        );
    }
    for (i, (d, f)) in declared.params.iter().zip(&defined.params).enumerate() {
        if d != f {
            return mismatch(&format!("parameter {}", i + 1), format!("`{}`", d), format!("`{}`", f));
        }
    }
    if declared.ret != defined.ret {
        return mismatch("return type", format!("`{}`", declared.ret), format!("`{}`", defined.ret));
    }
    if declared.rest != defined.rest {
        return mismatch("`&rest`", show_rest(&declared.rest), show_rest(&defined.rest));
    }
    if !defined.type_params.is_empty() {
        return mismatch("type parameters", "none".to_string(), format!("{:?}", defined.type_params));
    }
    if !defined.optionals.is_empty() || !defined.keys.is_empty() {
        return mismatch(
            "parameter kinds",
            "required only".to_string(),
            "`&optional`/`&key`".to_string(),
        );
    }
    if declared.public != defined.public {
        let vis = |p: bool| if p { "`pub`".to_string() } else { "private".to_string() };
        return mismatch("visibility", vis(declared.public), vis(defined.public));
    }
    Ok(())
}
