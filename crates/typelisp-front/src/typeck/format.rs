//! `format`, `print` and `println`: CL's `format` and its two front ends.
//!
//! `(format dest control arg...)` builds the string the literal `control`
//! says; with `dest` `true` it is written to stdout too. `(print control
//! arg...)` and `(println control arg...)` write to stdout, `println` with a
//! newline after. The directives are run by the engine
//! (`Interp::run_format`): the call lowers to the internal builtin, handed the
//! arguments as one `Sexpr` list, each encoded by its own type.
//!
//! The control string has to be a literal, because its directives say which
//! arguments each one takes and of what type. That is checked once the form
//! is, when every argument's type is settled ([`super::expr::Cx::settle`]).

use crate::check::analyze::{option_of_sexpr, sexpr_ty};
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_print::format::{call_sites, ArgKind, ArgType};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// A control string to check against its arguments' types once they are
/// settled.
pub(crate) struct Directives {
    pub(crate) control: String,
    pub(crate) args: Vec<Type>,
    pub(crate) loc: Option<Loc>,
}

/// `(format dest control arg...)` — `args` without the `format`.
pub(crate) fn check_format(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(dest, dest_loc), (control, control_loc), items @ ..] = args else {
        return Err(Error::TypeError(
            "format: expected at least a destination (bool or a `CharOutput` stream) and a control string".into(),
        ));
    };
    // The destination's type says which `format` this is, so it is checked
    // with nothing expected of it.
    let dest = check_expr(cx, heap, locals, *dest, None, dest_loc.clone())?;
    if cx.resolve(&dest.ty) != Type::Bool {
        unimplemented!("typeck: `format` to a stream")
    }
    let (control, list) = check_control(cx, heap, locals, "format", (*control, control_loc.clone()), items, loc.clone())?;
    let path = Path::internal_builtin(wk::F_FORMAT_RT);
    Ok(Typed { node: Node::Call { path, targs: Vec::new(), args: vec![dest, control, list] }, ty: Type::Str, loc })
}

/// `(print control arg...)` or `(println control arg...)`, `head` saying
/// which — `args` without the head.
pub(crate) fn check_print(cx: &Cx, heap: &mut Heap, locals: &Locals, head: u32, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let (what, builtin) = if head == wk::PRINTLN { ("println", wk::F_PRINTLN_RT) } else { ("print", wk::F_PRINT_RT) };
    let [(control, control_loc), items @ ..] = args else {
        return Err(Error::TypeError(format!("{}: expected at least a control string", what)));
    };
    let (control, list) = check_control(cx, heap, locals, what, (*control, control_loc.clone()), items, loc.clone())?;
    let path = Path::internal_builtin(builtin);
    Ok(Typed { node: Node::Call { path, targs: Vec::new(), args: vec![control, list] }, ty: Type::Unit, loc })
}

/// The literal control string `control` and the arguments `items` it is
/// given, checked: the string, and the arguments as one `Sexpr` list.
fn check_control(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    what: &'static str,
    (control, control_loc): (Value, Option<Loc>),
    items: &[(Value, Option<Loc>)],
    loc: Option<Loc>,
) -> Result<(Typed, Typed), Error> {
    let Value::Str(id) = control else {
        return Err(Error::TypeError(format!(
            "{}: the control string must be a literal — the directives in it decide what \
             `~/name/` calls and which arguments they take, and a string built at run time \
             cannot be read at compile time. Build the text with `format` and print that \
             instead.",
            what
        )));
    };
    let text = heap.string(id).to_string();
    let unknown: Vec<ArgType> = items.iter().map(|_| ArgType { kind: ArgKind::Unknown, shown: String::new() }).collect();
    if !call_sites(&text, &unknown).map_err(Error::TypeError)?.is_empty() {
        unimplemented!("typeck: a `~/name/` directive")
    }
    let control = check_expr(cx, heap, locals, control, Some(&Type::Str), control_loc)?;
    let mut checked = Vec::with_capacity(items.len());
    for (i, (item, item_loc)) in items.iter().enumerate() {
        let item = check_expr(cx, heap, locals, *item, None, item_loc.clone())?;
        cx.must_decide(item.ty.clone(), format!("{}: the type of argument {} is not decided", what, i + 1), item.loc.clone());
        checked.push(item);
    }
    cx.check_directives_later(Directives { control: text, args: checked.iter().map(|t| t.ty.clone()).collect(), loc });
    Ok((control, sexpr_list(checked)))
}

/// `items` as one `Sexpr` list, each held as a `Sexpr` by its own type.
pub(crate) fn sexpr_list(items: Vec<Typed>) -> Typed {
    let list_ty = option_of_sexpr();
    let cons = Path::in_ns(typelisp_mem::StdModule::SEXPR.ns(), "sexpr-cons");
    let mut acc = Typed::none(sexpr_ty());
    for item in items.into_iter().rev() {
        let loc = item.loc.clone();
        let held = Typed { node: Node::AsSexpr { value: Box::new(item) }, ty: sexpr_ty(), loc };
        acc = Typed { node: Node::Call { path: cons, targs: Vec::new(), args: vec![held, acc] }, ty: list_ty.clone(), loc: None };
    }
    acc
}

/// `d`'s control string checked against its arguments' settled types,
/// `resolve` settling each.
pub(crate) fn check_directives(d: &Directives, resolve: impl Fn(&Type) -> Type) -> Result<(), Error> {
    let args: Vec<ArgType> = d
        .args
        .iter()
        .map(|t| {
            let ty = resolve(t);
            ArgType { kind: arg_kind(&ty), shown: ty.to_string() }
        })
        .collect();
    call_sites(&d.control, &args).map(|_| ()).map_err(|m| {
        let e = Error::TypeError(m);
        match &d.loc {
            Some(l) => e.at(l.clone()),
            None => e,
        }
    })
}

/// What a directive may demand of an argument of the settled type `ty`.
///
/// A type parameter of a generic body is any type, since the body is
/// checked once for all of them: only the directives that print any value
/// take it.
fn arg_kind(ty: &Type) -> ArgKind {
    match ty {
        t if t.is_int_family() => ArgKind::Integer,
        Type::Ratio | Type::Rational | Type::F32 | Type::F64 => ArgKind::Real,
        Type::Char => ArgKind::Char,
        Type::Bool => ArgKind::Bool,
        Type::Str => ArgKind::Str,
        // A diverging argument never reaches the directive.
        Type::Never => ArgKind::Unknown,
        t if *t == option_of_sexpr() => ArgKind::List,
        Type::Named(p, _) if crate::types::path_is_builtin(p, typelisp_mem::wk::SEXPR) => ArgKind::Sexpr,
        _ => ArgKind::Other,
    }
}
