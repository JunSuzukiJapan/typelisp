//! `defmacro`: `(defmacro name (params) body...)`, CL's.
//!
//! A macro is a function from the forms of its call, as they are written,
//! to the form the call is: every parameter and the result are S-expression
//! data, `Option<Sexpr>`. The parameter list is CL's: required names, then
//! `&optional`, `&rest` and `&key` ones, an optional or key one with a
//! default form, which sees the parameters before it. The body is checked
//! here; the evaluator holds it, and expands each call
//! ([`crate::check::MacroExpander`]).

use crate::check::analyze::{option_of_sexpr, parse_macro_lambda_list, MacroLambda, MacroShape};
use crate::check::bound_name;
use crate::check::registry::MacroDef;
use crate::types::{Path, Type};
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::define::{Definer, Kind};
use super::defun::split_doc;
use super::expr::{check_body, check_expr, Cx};
use super::generic::Requests;
use super::locals::Locals;
use super::lower::{lower_rooted, Lowering};
use super::registry::{Doc, Registry, ValueDef};
use super::scope::TypeScope;
use super::subst::Subst;

/// `(defmacro name (params) body...)` — `parts` without the `defmacro` —
/// and its core form, unrooted. The caller undoes the definition if checking
/// fails.
pub(crate) fn check_defmacro(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let [(name, _), (params, _), body @ ..] = parts else {
        return Err(Error::TypeError("defmacro: (defmacro name (params) body...)".into()));
    };
    let Value::Symbol(name) = *name else {
        return Err(Error::TypeError("defmacro: name must be a symbol".into()));
    };
    d.ownable(heap, name, "defmacro")?;
    bound_name(name, "defmacro")?;
    d.value(reg, Kind::Macro, name)?;
    let (doc, body) = split_doc(heap, body);
    let (required, optionals, rest, keys) = parse_macro_lambda_list(heap, &heap.list_to_vec(*params)?)?;
    let def = MacroDef {
        required: required.len(),
        optional: optionals.len(),
        rest: rest.is_some(),
        keys: keys.iter().map(|(n, _)| n.name().to_string()).collect(),
        public: d.public,
        builtin: false,
    };
    let path = Path::of_symbol(d.site.ns, name);
    reg.values.insert(path, ValueDef::Macro { public: d.public, shape: MacroShape::of(&def) });
    if let Some(doc) = doc {
        reg.docs.insert(Doc::Macro(path), doc);
    }

    // Each default sees the parameters before it.
    let sexpr = option_of_sexpr();
    let mut bound: Vec<SymRef> = required.clone();
    let mut opt_defaults = Vec::with_capacity(optionals.len());
    for (n, default) in &optionals {
        opt_defaults.push(check_default(reg, d, heap, requests, &bound, *default)?);
        bound.push(*n);
    }
    bound.extend(rest);
    let mut key_defaults = Vec::with_capacity(keys.len());
    for (n, default) in &keys {
        key_defaults.push((n.name().to_string(), check_default(reg, d, heap, requests, &bound, *default)?));
        bound.push(*n);
    }
    let scope = TypeScope::empty();
    let mut checked = {
        let cx = Cx::new(reg, d.expander, d.site.ns, &scope, None);
        let root = Locals::empty();
        let locals = root.bind(bound.iter().map(|n| (*n, sexpr.clone())).collect())?;
        let (mut checked, _) = check_body(&cx, heap, &locals, body, Some(&sexpr))?;
        cx.settle(heap, &mut checked)?;
        checked
    };
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let lowered = checked.iter().map(|t| lower_rooted(heap, &mut lw, t)).collect::<Result<Vec<_>, _>>()?;
    checked.clear();
    let lambda = MacroLambda { required: required.len(), optionals: opt_defaults, keys: key_defaults };
    crate::check::forms::defmacro_form(heap, &path, &bound, rest.is_some(), &lambda, &lowered)
}

/// The default form of an `&optional` or `&key` parameter, checked where the
/// parameters `bound` are: its core form, or none when it has no default.
fn check_default(
    reg: &Registry,
    d: &Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    bound: &[SymRef],
    default: Option<Value>,
) -> Result<Vec<Value>, Error> {
    let Some(form) = default else { return Ok(Vec::new()) };
    let sexpr: Type = option_of_sexpr();
    let scope = TypeScope::empty();
    let mut tree = {
        let cx = Cx::new(reg, d.expander, d.site.ns, &scope, None);
        let root = Locals::empty();
        let locals = root.bind(bound.iter().map(|n| (*n, sexpr.clone())).collect())?;
        let t = check_expr(&cx, heap, &locals, form, Some(&sexpr), None)?;
        let mut tree = vec![t];
        cx.settle(heap, &mut tree)?;
        tree
    };
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let lowered = lower_rooted(heap, &mut lw, &tree[0])?;
    tree.clear();
    Ok(vec![lowered])
}
