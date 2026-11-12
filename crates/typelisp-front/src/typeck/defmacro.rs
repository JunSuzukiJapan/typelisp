//! `defmacro`: `(defmacro name (params) body...)`, CL's.
//!
//! A macro is a function from the forms of its call, as they are written,
//! to the form the call is: every parameter and the result are S-expression
//! data, `Option<Sexpr>`. The parameter list is CL's: required names, then
//! `&optional`, `&rest` and `&key` ones, an optional or key one with a
//! default form, which sees the parameters before it. The body is checked
//! here; the evaluator holds it, and expands each call
//! ([`crate::typeck::checker::MacroExpander`]).

use crate::wk;
use crate::typeck::sexpr_value::option_of_sexpr;
use crate::check::bound_name;
use crate::typeck::registry::MacroDef;
use crate::types::{Path, Type};
use crate::typeck::checker::MacroExpander;
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::define::{Definer, Kind};
use super::defun::split_doc;
use super::expr::{check_body, check_expr, Cx};
use super::generic::Requests;
use super::locals::Locals;
use super::lower::{lower_rooted, Lowering};
use super::registry::{Definition, Registry, ValueDef};
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
    let name = d.ownable(heap, name, "defmacro")?;
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
    d.record(reg, Definition::Macro(path), doc);
    let site = MacroSite { reg, expander: d.expander, ns: d.site.ns, notes: d.notes };
    macro_form(&site, heap, requests, &path, (required, optionals, rest, keys), body)
}

/// Where a macro's body is checked: what is defined there, what expands
/// the macro calls it makes, and its module.
pub(crate) struct MacroSite<'a> {
    pub(crate) reg: &'a Registry,
    pub(crate) expander: &'a dyn MacroExpander,
    pub(crate) ns: NsId,
    pub(crate) notes: &'a super::expr::Notes,
}

/// A macro's parameter list as `parse_macro_lambda_list` reads it.
pub(crate) type MacroParams = (Vec<SymRef>, Vec<(SymRef, Option<Value>)>, Option<SymRef>, Vec<(SymRef, Option<Value>)>);

/// The macro `path` with the parameters `params` and the body `body`,
/// checked at `site`: its definition's core form, unrooted.
pub(crate) fn macro_form(
    site: &MacroSite,
    heap: &mut Heap,
    requests: &mut Requests,
    path: &Path,
    (required, optionals, rest, keys): MacroParams,
    body: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    // Each default sees the parameters before it.
    let sexpr = option_of_sexpr();
    let mut bound: Vec<SymRef> = required.clone();
    let mut opt_defaults = Vec::with_capacity(optionals.len());
    for (n, default) in &optionals {
        opt_defaults.push(check_default(site, heap, requests, &bound, *default)?);
        bound.push(*n);
    }
    bound.extend(rest);
    let mut key_defaults = Vec::with_capacity(keys.len());
    for (n, default) in &keys {
        key_defaults.push((n.name().to_string(), check_default(site, heap, requests, &bound, *default)?));
        bound.push(*n);
    }
    let reg = site.reg;
    let scope = TypeScope::empty();
    let mut checked = {
        let cx = Cx::new(reg, site.expander, site.ns, &scope, None, site.notes);
        let root = Locals::empty();
        let locals = root.bind(bound.iter().map(|n| (*n, sexpr.clone(), None)).collect())?;
        let (mut checked, _) = check_body(&cx, heap, &locals, body, Some(&sexpr))?;
        cx.settle(heap, &mut checked)?;
        checked
    };
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let lowered = checked.iter().map(|t| lower_rooted(heap, &mut lw, t)).collect::<Result<Vec<_>, _>>()?;
    checked.clear();
    let lambda = MacroLambda { required: required.len(), optionals: opt_defaults, keys: key_defaults };
    crate::check::forms::defmacro_form(heap, path, &bound, rest.is_some(), &lambda, &lowered)
}

/// The default form of an `&optional` or `&key` parameter, checked where the
/// parameters `bound` are: its core form, or none when it has no default.
fn check_default(
    site: &MacroSite,
    heap: &mut Heap,
    requests: &mut Requests,
    bound: &[SymRef],
    default: Option<Value>,
) -> Result<Vec<Value>, Error> {
    let Some(form) = default else { return Ok(Vec::new()) };
    let sexpr: Type = option_of_sexpr();
    let reg = site.reg;
    let scope = TypeScope::empty();
    let mut tree = {
        let cx = Cx::new(reg, site.expander, site.ns, &scope, None, site.notes);
        let root = Locals::empty();
        let locals = root.bind(bound.iter().map(|n| (*n, sexpr.clone(), None)).collect())?;
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

/// The `&optional`/`&key` structure of a `defmacro` lambda list, beyond its
/// leading required params (and any single trailing `&rest`, whose presence is
/// tracked separately by the `rest` flag on the `defmacro` form).
///
/// The checker's own working shape while parsing a macro's lambda list; it is
/// written into the `(defmacro ...)` form's own lambda field (see
/// [`Checker::defmacro_form`]) and read back from there at expansion time, so
/// this type does not outlive checking and needs no serialized form of its own.
/// A default-value body is a list of core forms, evaluated when the argument is
/// omitted in an environment where the earlier params are already bound (an
/// empty body binds `nil`). A plain fixed-or-`&rest` macro carries a `required`
/// equal to its fixed-param count with both `optionals` and `keys` empty (the
/// common case).
#[derive(Clone, Debug, PartialEq)]
pub struct MacroLambda {
    /// Number of leading required (positional, non-defaulted) params — the
    /// call's minimum argument count.
    pub required: usize,
    /// Default-value body for each `&optional` param, in order.
    pub optionals: Vec<Vec<Value>>,
    /// Each `&key` param's `(name, default-value body)`, in order — matched at
    /// the call site by the keyword symbol `:name`.
    pub keys: Vec<(String, Vec<Value>)>,
}

/// The call-site arity shape of a macro, derived from its [`MacroDef`] by
/// [`Info::resolve_macro`]/[`Info::macro_at`]
/// and consumed by [`MacroShape::check_arity`]. Carries only what a raw
/// argument *count* can be judged against — the `&key` names themselves aren't
/// needed here (keyword validation happens in `Interp::expand_macro`).
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct MacroShape {
    required: usize,
    optional: usize,
    rest: bool,
    has_keys: bool,
}

impl MacroShape {
    pub(crate) fn of(def: &MacroDef) -> MacroShape {
        MacroShape { required: def.required, optional: def.optional, rest: def.rest, has_keys: !def.keys.is_empty() }
    }

    /// The arity check shared by every macro-expansion site. Only the bounds a
    /// call site can judge from raw argument *count* are checked here; the
    /// finer validation that needs to parse the argument forms (an unknown
    /// `&key` keyword, an odd `&key` plist) is left to `Interp::expand_macro`,
    /// whose error is surfaced the same way (`macro `name`: ...`).
    ///
    /// - A minimum of `required` args always applies.
    /// - With neither `&rest` nor `&key`, the maximum is `required + optional`
    ///   (exact when there are no `&optional` params — the plain-macro case,
    ///   preserving the original exact-arity message).
    /// - With `&rest` or `&key`, there is no call-site upper bound (the
    ///   trailing args are a `&rest` list and/or a keyword plist).
    pub(crate) fn check_arity(self, head: &str, got: usize) -> Result<(), Error> {
        let MacroShape { required, optional, rest, has_keys } = self;
        if got < required {
            return Err(Error::TypeError(format!(
                "macro `{}` expects at least {} argument(s), got {}",
                head, required, got
            )));
        }
        if !rest && !has_keys {
            let max = required + optional;
            if got > max {
                return Err(Error::TypeError(if optional == 0 {
                    format!("macro `{}` expects {} argument(s), got {}", head, required, got)
                } else {
                    format!("macro `{}` expects {} to {} argument(s), got {}", head, required, max, got)
                }));
            }
        }
        Ok(())
    }
}

/// What a `defmacro` lambda list parses into: the required parameter names,
/// the `&optional` parameters (each with its default form, if it declared
/// one), the `&rest` name, and the `&key` parameters in the same shape as the
/// optionals.
pub(crate) type MacroLambdaList = (Vec<SymRef>, Vec<(SymRef, Option<Value>)>, Option<SymRef>, Vec<(SymRef, Option<Value>)>);

pub(crate) fn parse_macro_lambda_list(heap: &Heap, param_vals: &[Value]) -> Result<MacroLambdaList, Error> {
    // Section rank: required=0, &optional=1, &rest=2, &key=3. A marker may
    // only advance the rank forward, so each appears at most once and in
    // order.
    let mut rank = 0u8;
    let mut required_names: Vec<SymRef> = Vec::new();
    let mut optionals: Vec<(SymRef, Option<Value>)> = Vec::new();
    let mut rest_name: Option<SymRef> = None;
    let mut keys: Vec<(SymRef, Option<Value>)> = Vec::new();

    for p in param_vals {
        if let Value::Symbol(id) = p {
            match id.well_known() {
                wk::OPTIONAL => {
                    if rank >= 1 {
                        return Err(Error::TypeError("defmacro: &optional must precede &rest and &key, and appear once".into()));
                    }
                    rank = 1;
                    continue;
                }
                wk::REST => {
                    if rank >= 2 {
                        return Err(Error::TypeError("defmacro: &rest must precede &key, and appear once".into()));
                    }
                    rank = 2;
                    continue;
                }
                wk::KEY => {
                    if rank >= 3 {
                        return Err(Error::TypeError("defmacro: &key may appear only once".into()));
                    }
                    rank = 3;
                    continue;
                }
                _ => {}
            }
        }
        match rank {
            0 => required_names.push(macro_param_name(*p, "required")?),
            1 => optionals.push(macro_opt_key_spec(heap, *p, "&optional")?),
            2 => {
                if rest_name.is_some() {
                    return Err(Error::TypeError("defmacro: &rest takes exactly one parameter name".into()));
                }
                rest_name = Some(macro_param_name(*p, "&rest")?);
            }
            _ => keys.push(macro_opt_key_spec(heap, *p, "&key")?),
        }
    }
    if rank == 2 && rest_name.is_none() {
        return Err(Error::TypeError("defmacro: &rest must be followed by exactly one parameter name".into()));
    }
    Ok((required_names, optionals, rest_name, keys))
}

/// A bare-symbol lambda-list parameter name (`required`/`&rest`).
fn macro_param_name(v: Value, section: &str) -> Result<SymRef, Error> {
    match v {
        Value::Symbol(id) => Ok(id),
        _ => Err(Error::TypeError(format!("defmacro: {} parameter must be a name", section))),
    }
}

/// An `&optional`/`&key` item: either a bare `name` (no default) or a
/// `(name default-form)` pair. Returns `(name, Some(form)|None)`.
fn macro_opt_key_spec(heap: &Heap, v: Value, section: &str) -> Result<(SymRef, Option<Value>), Error> {
    match v {
        Value::Symbol(id) => Ok((id, None)),
        Value::Cons(_) => {
            let items = heap.list_to_vec(v)?;
            if items.is_empty() || items.len() > 2 {
                return Err(Error::TypeError(format!(
                    "defmacro: {} parameter must be `name` or `(name default)`",
                    section
                )));
            }
            let name = macro_param_name(items[0], section)?;
            Ok((name, items.get(1).copied()))
        }
        _ => Err(Error::TypeError(format!(
            "defmacro: {} parameter must be `name` or `(name default)`",
            section
        ))),
    }
}
