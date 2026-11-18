//! Macro calls and symbol macros. A macro is expanded with its arguments as
//! they are written, and the expansion is checked where the call is, as CL
//! does: what it expands to sees the call's local variables.
//!
//! Which forms are macro uses is answered here once ([`macro_use`]), for
//! checking and for `macroexpand-1` alike, so the two cannot disagree.
//!
//! The body of a macro is the evaluator's ([`MacroExpander`]); the checker
//! knows how many arguments a call of it may have.

use crate::typeck::checker::{ExpansionEnv, MacroExpander};
use crate::typeck::defmacro::MacroShape;
use crate::typeck::expr::{Cx, SPECIAL_FORM_HEADS, TOPLEVEL_FORM_HEADS};
use crate::types::{path_target, Path, PathTarget};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::locals::Locals;
use super::registry::{Registry, ValueDef};
use super::resolve::{reexported, sees, value_named};

/// What a form is a macro use of.
pub(crate) enum MacroUse {
    /// A call of the macro at the path, which takes arguments as the shape
    /// says; the head written as the string.
    Call(Path, MacroShape, String),
    /// A symbol `symbol-macrolet` bound to the form.
    LocalSymbol(Value),
    /// A symbol `define-symbol-macro` defined at the path.
    GlobalSymbol(Path),
}

/// A place macros are expanded at: the definitions, the module, and the
/// locals around it, none for a top-level form.
pub(crate) struct Site<'a> {
    reg: &'a Registry,
    ns: NsId,
    locals: Option<&'a Locals<'a>>,
}

impl<'a> Site<'a> {
    /// Where no local is bound.
    pub(crate) fn toplevel(reg: &'a Registry, ns: NsId) -> Site<'a> {
        Site { reg, ns, locals: None }
    }

    /// Where an expression is checked, inside `locals`.
    pub(crate) fn at(cx: &'a Cx, locals: &'a Locals<'a>) -> Site<'a> {
        Site { reg: cx.reg, ns: cx.ns, locals: Some(locals) }
    }

    /// The use `u` of a macro, written as the form whose arguments are
    /// `args`, expanded once here: the expansion, rooted.
    pub(crate) fn expand(&self, expander: &dyn MacroExpander, heap: &mut Heap, u: MacroUse, args: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
        match u {
            MacroUse::Call(path, shape, display) => expand(expander, heap, &path, shape, &display, args, self),
            // The program's own form, rooted with it.
            MacroUse::LocalSymbol(form) => Ok(form),
            MacroUse::GlobalSymbol(path) => expand(expander, heap, &path, MacroShape::NONE, &path.to_string(), &[], self),
        }
    }
}

impl ExpansionEnv for Site<'_> {
    fn expand_1(&self, heap: &mut Heap, expander: &dyn MacroExpander, form: Value, lexical: bool) -> Result<Option<Value>, Error> {
        let here = Site { reg: self.reg, ns: self.ns, locals: if lexical { self.locals } else { None } };
        let empty = Locals::empty();
        let Some(u) = macro_use(here.reg, here.ns, heap, here.locals.unwrap_or(&empty), form)? else {
            return Ok(None);
        };
        let args = match u {
            MacroUse::Call(..) => heap.list_to_vec_locs(form)?.split_off(1),
            _ => Vec::new(),
        };
        here.expand(expander, heap, u, &args).map(Some)
    }
}

/// What the form `v`, written in the module `ns` inside `locals`, is a macro
/// use of, if it is one.
pub(crate) fn macro_use(reg: &Registry, ns: NsId, heap: &Heap, locals: &Locals, v: Value) -> Result<Option<MacroUse>, Error> {
    match v {
        Value::Symbol(id) => Ok(symbol_macro(reg, ns, locals, id)),
        Value::Path(id) => match path_target(heap, ns, id)? {
            PathTarget::In(path) => Ok(global_symbol_macro(reg, ns, path)),
            PathTarget::Member(..) => Ok(None),
        },
        Value::Cons(_) => Ok(macro_call(reg, ns, heap, locals, heap.car(v)?)?.map(|(path, shape, display)| MacroUse::Call(path, shape, display))),
        _ => Ok(None),
    }
}

/// The symbol macro the symbol `id` is, written in `ns` inside `locals`:
/// the nearest binding of the name decides, and a module-level one is seen
/// only where no local binds the name.
pub(crate) fn symbol_macro(reg: &Registry, ns: NsId, locals: &Locals, id: SymRef) -> Option<MacroUse> {
    if let Some(form) = locals.symbol_macro(id) {
        return Some(MacroUse::LocalSymbol(form));
    }
    if locals.binds(id) {
        return None;
    }
    match value_named(reg, ns, id) {
        Some((path, ValueDef::SymbolMacro { .. })) => Some(MacroUse::GlobalSymbol(path)),
        _ => None,
    }
}

/// The symbol macro at `path`, when `ns` sees it.
fn global_symbol_macro(reg: &Registry, ns: NsId, path: Path) -> Option<MacroUse> {
    let path = reexported(reg, path);
    match reg.values.get(&path) {
        Some(ValueDef::SymbolMacro { public }) if *public || sees(ns, path.ns()) => Some(MacroUse::GlobalSymbol(path)),
        _ => None,
    }
}

/// The macro a form whose head is `head`, written in `ns` inside `locals`,
/// calls: its path, its shape, and the head as written. A local binding of
/// the name hides a module-level macro, and a built-in form is never a
/// macro call, whatever a module defines.
pub(crate) fn macro_call(reg: &Registry, ns: NsId, heap: &Heap, locals: &Locals, head: Value) -> Result<Option<(Path, MacroShape, String)>, Error> {
    match head {
        Value::Symbol(head) if SPECIAL_FORM_HEADS.contains(&head.well_known()) || TOPLEVEL_FORM_HEADS.contains(&head.well_known()) => Ok(None),
        Value::Symbol(head) => {
            if let Some((path, shape)) = locals.local_macro(head) {
                return Ok(Some((path, shape, heap.symbol_spelling(head).to_string())));
            }
            if locals.binds(head) {
                return Ok(None);
            }
            Ok(match value_named(reg, ns, head) {
                Some((path, ValueDef::Macro { shape, .. })) => Some((path, *shape, heap.symbol_spelling(head).to_string())),
                _ => None,
            })
        }
        // The reader's head for `#[ATTR...] FORM`, which names nothing.
        Value::Path(_) if super::attribute::is_attribute(heap, head) => Ok(None),
        Value::Path(id) => match path_target(heap, ns, id)? {
            PathTarget::In(path) => {
                let path = reexported(reg, path);
                Ok(match reg.values.get(&path) {
                    Some(ValueDef::Macro { public, shape }) if *public || sees(ns, path.ns()) => {
                        Some((path, *shape, crate::types::path_spelling(heap, id)))
                    }
                    _ => None,
                })
            }
            PathTarget::Member(..) => Ok(None),
        },
        _ => Ok(None),
    }
}

/// The call of the macro `path`, written `display`, with the arguments
/// `args` expanded once at `env`: the expansion, rooted on `heap`'s root
/// stack.
pub(crate) fn expand(
    expander: &dyn MacroExpander,
    heap: &mut Heap,
    path: &Path,
    shape: MacroShape,
    display: &str,
    args: &[(Value, Option<Loc>)],
    env: &dyn ExpansionEnv,
) -> Result<Value, Error> {
    shape.check_arity(display, args.len())?;
    let raw = args.iter().map(|(v, _)| *v).collect();
    let expanded = expander.expand_macro(heap, path, raw, env).map_err(|e| Error::TypeError(format!("macro `{}`: {}", display, e)))?;
    heap.push_root(expanded);
    Ok(expanded)
}

/// When the top-level form `v`, written in the module `ns`, is a macro use:
/// its expansion once, rooted. A top-level form is expanded before it is
/// read as a definition, since a macro may expand to one.
pub(crate) fn expand_toplevel(reg: &Registry, expander: &dyn MacroExpander, heap: &mut Heap, ns: NsId, v: Value) -> Result<Option<Value>, Error> {
    Site::toplevel(reg, ns).expand_1(heap, expander, v, false)
}
