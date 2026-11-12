//! Macro calls. A macro is expanded with its arguments as they are written,
//! and the expansion is checked where the call is, as CL does: what it
//! expands to sees the call's local variables.
//!
//! The body of a macro is the evaluator's ([`MacroExpander`]); the checker
//! knows how many arguments a call of it may have.

use crate::typeck::defmacro::MacroShape;
use crate::typeck::expr::TOPLEVEL_FORM_HEADS;
use crate::typeck::checker::MacroExpander;
use crate::typeck::expr::SPECIAL_FORM_HEADS;
use crate::types::{path_target, Path, PathTarget};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;

use super::registry::{Registry, ValueDef};
use super::resolve::{sees, value_named};

/// The call of the macro `path`, written `display`, with the arguments
/// `args` expanded once: the expansion, rooted on `heap`'s root stack.
pub(crate) fn expand(
    expander: &dyn MacroExpander,
    heap: &mut Heap,
    path: &Path,
    shape: MacroShape,
    display: &str,
    args: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    shape.check_arity(display, args.len())?;
    let raw = args.iter().map(|(v, _)| *v).collect();
    let expanded = expander.expand_macro(heap, path, raw).map_err(|e| Error::TypeError(format!("macro `{}`: {}", display, e)))?;
    heap.push_root(expanded);
    Ok(expanded)
}

/// When the top-level form `v`, written in the module `ns`, is a macro call:
/// its expansion once, rooted. A top-level form is expanded before it is
/// read as a definition, since a macro may expand to one.
pub(crate) fn expand_toplevel(reg: &Registry, expander: &dyn MacroExpander, heap: &mut Heap, ns: NsId, v: Value) -> Result<Option<Value>, Error> {
    if !matches!(v, Value::Cons(_)) {
        return Ok(None);
    }
    let elems = heap.list_to_vec_locs(v)?;
    let (path, shape, display) = match elems[0].0 {
        // A built-in form is never a macro call, whatever a module defines.
        Value::Symbol(head) if SPECIAL_FORM_HEADS.contains(&head.well_known()) || TOPLEVEL_FORM_HEADS.contains(&head.well_known()) => {
            return Ok(None)
        }
        Value::Symbol(head) => match value_named(reg, ns, head) {
            Some((path, ValueDef::Macro { shape, .. })) => (path, *shape, heap.symbol_spelling(head).to_string()),
            _ => return Ok(None),
        },
        // The reader's head for `#[ATTR...] FORM`, which names nothing.
        Value::Path(_) if super::attribute::is_attribute(heap, elems[0].0) => return Ok(None),
        Value::Path(id) => match path_target(heap, ns, id)? {
            PathTarget::In(path) => {
                let path = super::resolve::reexported(reg, path);
                match reg.values.get(&path) {
                    Some(ValueDef::Macro { public, shape }) if *public || sees(ns, path.ns()) => {
                        (path, *shape, crate::types::path_spelling(heap, id))
                    }
                    _ => return Ok(None),
                }
            }
            PathTarget::Member(..) => return Ok(None),
        },
        _ => return Ok(None),
    };
    expand(expander, heap, &path, shape, &display, &elems[1..]).map(Some)
}
