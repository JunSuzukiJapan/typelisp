//! Checking: the agreements the analysis recorded on the analyzed tree and
//! did not stop at (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).
//!
//! The analysis takes the type that was settled first and goes on; what it
//! could not reconcile stays in the tree as a node of its own. This reads
//! them back, after the expressions they sit in are analyzed.
//!
//! A definition and what declared it ahead are checked here too, once the
//! definition is analyzed, and so is the name a binding or a definition
//! introduces, before the analysis registers it, and so is the head of a
//! form in expression position, before the analysis reads it as a call: the
//! rules are about what is written, and nothing analyzed later bears on
//! them. A form that breaks one is not analyzed on, since what it would have
//! registered is missing and what follows would complain about that instead.

use typelisp_mem::Error;

use crate::types::Type;
use crate::wk;

use super::core::{NotDyn, Unmet};

/// What a call says when the type it decided for `of`'s type parameter
/// `param` falls short of `param`'s bound as `unmet` says.
pub(crate) fn bound_message(of: &str, param: &str, unmet: &Unmet) -> String {
    match unmet {
        Unmet::Undeclared { var, trait_path } => format!(
            "{}: type parameter `{}` is only known here as the enclosing function's own `{}`, \
             which has no `where` bound declaring it implements `{}` — the enclosing function's \
             signature needs a matching `:where` clause on `{}`",
            of, param, var, trait_path, var
        ),
        Unmet::Unimplemented { ty, trait_path, via } => {
            let mut m = format!(
                "{}: type `{}` does not implement trait `{}` required by `:where` clause on type parameter `{}`",
                of, ty, trait_path, param
            );
            if let Some(method) = via {
                m.push_str(&format!(
                    "; its method `{}` returns a value that does — pass `({} x)` instead of `x`",
                    method, method
                ));
            }
            m
        }
        Unmet::Assoc { ty, assoc, actual, required } => format!(
            "{}: type `{}`'s associated type `{}` is `{}`, but `:where` clause requires `{}`",
            of,
            ty,
            assoc,
            actual.as_ref().map(|t| t.to_string()).unwrap_or_else(|| "undefined".to_string()),
            required
        ),
    }
}

pub(crate) fn not_dyn(ty: &Type, trait_path: &crate::Path, pins: &[Type], why: &NotDyn) -> String {
    let converts = |why: String| {
        let to = Type::Dyn(trait_path.clone(), pins.to_vec());
        format!(
            "cannot convert `{}` to `{}` — {}; box the concrete value as `:dyn {}` instead",
            ty,
            to,
            why,
            trait_path
        )
    };
    match why {
        NotDyn::NotNominal => {
            format!("`{}` cannot be used as `:dyn {}`: it is not a nominal type", ty, trait_path)
        }
        NotDyn::Unimplemented => format!(
            "`{}` does not implement `{}`, so it cannot be used as `:dyn {}`",
            ty, trait_path, trait_path
        ),
        NotDyn::NoHeapRepr => format!(
            "`{}` has no heap representation (its values are not `Sexpr`-encodable), so it cannot be boxed as `:dyn {}`",
            ty,
            trait_path
        ),
        NotDyn::NichedOption => format!(
            "`{}` is a niche-represented Option with no box to carry a type key, so it cannot be boxed as `:dyn {}`",
            ty,
            trait_path
        ),
        NotDyn::Assoc { owner, name, actual, pin } => format!(
            "`{}` implements `{}` with `{} = {}`, but `:dyn {}` requires `{} = {}`",
            owner,
            trait_path,
            name,
            actual,
            trait_path,
            name,
            pin
        ),
        NotDyn::NotInherited { from } => converts(format!("`{}` does not inherit `{}`", from, trait_path)),
        NotDyn::PinDiffers { from, name, actual, want } => converts(format!(
            "`{}` pins `{}`'s `{}` to `{}`, not `{}`",
            from,
            trait_path,
            name,
            actual,
            want
        )),
        NotDyn::Unpinned { from, name } => {
            converts(format!("`{}`'s `{}` is not pinned by `{}`", trait_path, name, from))
        }
    }
}

/// The name a binding or a definition introduces with the symbol `id`. A
/// keyword cannot be one: CL makes every keyword a constant variable naming
/// itself, so it can be neither bound nor assigned, and no definition takes
/// its name either.
pub(crate) fn bound_name(id: typelisp_mem::SymRef, who: &str) -> Result<String, Error> {
    bound_symbol(id, who).map(|id| id.spelling().to_string())
}

/// [`bound_name`] for a local binding, which is the symbol itself: a
/// reference to the local reads as the same symbol.
pub(super) fn bound_symbol(id: typelisp_mem::SymRef, who: &str) -> Result<typelisp_mem::SymRef, Error> {
    if id.is_keyword() {
        return Err(Error::TypeError(format!(
            "{}: `{}` is a keyword — a constant that names itself — so it cannot be bound, assigned or defined",
            who,
            id.spelling()
        )));
    }
    Ok(id)
}

/// Refuses `head` as the head of a form in expression position when it is
/// one of `top_level`, the heads written only at top level.
pub(crate) fn expression_head(head: typelisp_mem::SymRef, top_level: &[u32]) -> Result<(), Error> {
    let k = head.well_known();
    if !top_level.contains(&k) {
        return Ok(());
    }
    let hint = match k {
        wk::DEFVAR | wk::DEFPARAMETER | wk::DEFCONSTANT => " — bind a local value with `let`",
        wk::DEFUN => " — define a local function with `labels`",
        wk::DEFMACRO => " — define a local macro with `macrolet`",
        _ => "",
    };
    Err(Error::TypeError(format!(
        "{}: only allowed at top level, not in expression position{}",
        head.spelling(),
        hint
    )))
}
