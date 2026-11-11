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
use crate::check::registry::TraitBound;
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use crate::CompileTarget;
use typelisp_print::format::{call_sites, ArgKind, ArgType, CallArg, CallSite};

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
        return to_stream(cx, heap, locals, dest, &args[1..], loc);
    }
    let (control, list) = check_control(cx, heap, locals, "format", (*control, control_loc.clone()), items, loc.clone())?;
    let path = Path::internal_builtin(wk::F_FORMAT_RT);
    Ok(Typed { node: Node::Call { path, targs: Vec::new(), args: vec![dest, control, list] }, ty: Type::Str, loc })
}

/// `(format stream control arg...)`, `dest` the stream checked: the string
/// written to it with its `CharOutput` method `write-string`, whose value
/// `()` is the form's. The stream is bound to a temporary, so that
/// `(write-string tmp (format false control arg...))` is checked with it in
/// hand, whether its type is a stream type, a trait object or a type
/// parameter bounded by `CharOutput`.
fn to_stream(cx: &Cx, heap: &mut Heap, locals: &Locals, dest: Typed, rest: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    // A number literal is its default type here.
    cx.decide(&dest.ty);
    let ty = cx.resolve(&dest.ty);
    let concrete = matches!(&ty, Type::Named(p, _) if cx.reg.type_def(p).is_some()) || crate::types::prim_type_path(&ty).is_some();
    let char_output = TraitBound { trait_path: Path::in_ns(crate::types::std_module("io"), "CharOutput"), assoc: Default::default() };
    if concrete && matches!(super::bounds::meets(cx, &ty, &char_output), super::bounds::Meets::No(_)) {
        return Err(Error::TypeError(format!(
            "format: the destination must be `true` (stdout), `false` (build the string only), or a stream implementing `CharOutput` — found `{}`",
            ty
        )));
    }
    let tmp = cx.temp();
    let format = Value::Symbol(heap.names().read(typelisp_mem::NsId::STD, "format"));
    let mut inner = vec![(format, None), (Value::Bool(false), None)];
    inner.extend(rest.iter().cloned());
    let inner = crate::check::forms::list_from_vec_locs(heap, &inner)?;
    heap.push_root(inner);
    let write_string = Value::Symbol(heap.names().read(typelisp_mem::NsId::STD, "write-string"));
    let form = crate::check::forms::list_from_vec_locs(heap, &[(write_string, None), (Value::Symbol(tmp), None), (inner, None)]);
    heap.pop_root();
    let form = form?;
    heap.push_root(form);
    let written = locals.bind(vec![(tmp, dest.ty.clone(), None)]).and_then(|l| check_expr(cx, heap, &l, form, None, loc.clone()));
    heap.pop_root();
    let written = written?;
    let ty = written.ty.clone();
    Ok(Typed { node: Node::Let { binds: vec![(tmp, dest)], body: vec![written] }, ty, loc })
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

/// `(eprint control arg...)` or `(eprintln control arg...)`, `head` saying
/// which — `args` without the head: `(format *error-output* control
/// arg...)`, `eprintln`'s control ending in `~%`. A release build checks it
/// and answers `()` instead, so a debugging line costs nothing there.
pub(crate) fn check_eprint(cx: &Cx, heap: &mut Heap, locals: &Locals, head: u32, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let what = if head == wk::EPRINTLN { "eprintln" } else { "eprint" };
    let [(control, control_loc), items @ ..] = args else {
        return Err(Error::TypeError(format!("{}: expected at least a control string", what)));
    };
    let mut text = control_text(heap, what, *control)?;
    if head == wk::EPRINTLN {
        text.push_str("~%");
    }
    let control = heap.alloc_string(text);
    heap.push_root(control);
    let (format, error_output) = {
        let names = heap.names();
        (names.read(typelisp_mem::NsId::STD, "format"), names.read(typelisp_mem::NsId::STD, "*error-output*"))
    };
    let mut written = vec![(Value::Symbol(format), None), (Value::Symbol(error_output), None), (control, control_loc.clone())];
    written.extend(items.iter().cloned());
    let form = crate::check::forms::list_from_vec_locs(heap, &written)?;
    heap.push_root(form);
    let checked = check_expr(cx, heap, locals, form, None, loc.clone())?;
    if crate::release::is_release() {
        return check_expr(cx, heap, locals, Value::Empty, Some(&Type::Unit), loc);
    }
    Ok(checked)
}

/// The text of `control`, the control string the form `what` is given,
/// which must be a literal.
fn control_text(heap: &Heap, what: &str, control: Value) -> Result<String, Error> {
    match control {
        Value::Str(id) => Ok(heap.string(id).to_string()),
        _ => Err(Error::TypeError(format!(
            "{}: the control string must be a literal — the directives in it decide what \
             `~/name/` calls and which arguments they take, and a string built at run time \
             cannot be read at compile time. Build the text with `format` and print that \
             instead.",
            what
        ))),
    }
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
    let text = control_text(heap, what, control)?;
    let mut checked = Vec::with_capacity(items.len());
    for (i, (item, item_loc)) in items.iter().enumerate() {
        let item = check_expr(cx, heap, locals, *item, None, item_loc.clone())?;
        cx.must_decide(item.ty.clone(), format!("{}: the type of argument {} is not decided", what, i + 1), item.loc.clone());
        checked.push(item);
    }
    let unknown: Vec<ArgType> = items.iter().map(|_| ArgType { kind: ArgKind::Unknown, shown: String::new() }).collect();
    let control = if call_sites(&text, &unknown).map_err(Error::TypeError)?.is_empty() {
        check_expr(cx, heap, locals, control, Some(&Type::Str), control_loc)?
    } else {
        resolve_calls(cx, heap, locals, what, &text, &checked, control_loc)?
    };
    cx.check_directives_later(Directives { control: text, args: checked.iter().map(|t| t.ty.clone()).collect(), loc });
    Ok((control, sexpr_list(checked)))
}

/// The control string `text`, which has `~/name/` directives, with each name
/// replaced by what it calls given the arguments `items` it can be handed
/// ([`call_target`]): the engine fetches that definition rather than look a
/// name up. The arguments' types are needed here, so a number literal no
/// type has decided is its default.
fn resolve_calls(cx: &Cx, heap: &mut Heap, locals: &Locals, what: &str, text: &str, items: &[Typed], loc: Option<Loc>) -> Result<Typed, Error> {
    let mut types = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        cx.decide_all(&item.ty);
        let ty = cx.resolve(&item.ty);
        if cx.holes.borrow().unsolved(&ty).is_some() {
            return Err(Error::TypeError(format!("{}: the type of argument {} is not decided, and a `~/name/` directive calls what its type says", what, i + 1)));
        }
        types.push(ty);
    }
    let args: Vec<ArgType> = types.iter().map(|ty| ArgType { kind: arg_kind(ty), shown: ty.to_string() }).collect();
    let sites = call_sites(text, &args).map_err(Error::TypeError)?;
    let mut names = Vec::with_capacity(sites.len());
    let mut targets: Vec<CompileTarget> = Vec::new();
    for site in &sites {
        if site.args.contains(&CallArg::Element) {
            return Err(Error::TypeError(format!(
                "{}: ~/{}/ can be handed an element of a list argument, whose type is only `Sexpr` — \
                 ~/name/ calls what `({} arg colon at)` would call here, which needs the argument's type",
                what, site.name, site.name
            )));
        }
        let mut found: Option<(CompileTarget, &Type)> = None;
        for arg in &site.args {
            let CallArg::Arg(i) = *arg else { unreachable!("elements were refused above") };
            let target = call_target(cx, heap, locals, what, site, &types[i])?;
            match &found {
                None => found = Some((target, &types[i])),
                Some((first, first_ty)) if *first != target => {
                    return Err(Error::TypeError(format!(
                        "{}: ~/{}/ can be handed a `{}` or a `{}` here, which call different definitions \
                         (`{}` and `{}`) — a directive calls one",
                        what,
                        site.name,
                        first_ty,
                        types[i],
                        first.written_name(),
                        target.written_name()
                    )));
                }
                Some(_) => {}
            }
        }
        names.push(match found {
            Some((target, _)) => {
                let name = target.link_name();
                if !targets.contains(&target) {
                    targets.push(target);
                }
                name
            }
            // No path through the control string reaches it.
            None => site.name.clone(),
        });
    }
    let renamed = typelisp_print::format::rename_calls(text, &names).map_err(Error::TypeError)?;
    let renamed = heap.alloc_string(renamed);
    heap.push_root(renamed);
    let control = check_expr(cx, heap, locals, renamed, Some(&Type::Str), loc.clone())?;
    Ok(Typed { node: Node::FormatCalls { control: Box::new(control), targets }, ty: Type::Str, loc })
}

/// What `(name arg colon at)` calls here, `arg` being of the type `ty`:
/// checked as that very call, so the name means what it means anywhere
/// else. It has to be a function or a method written in typelisp that takes
/// `arg` as it is: the engine hands the value over and nothing else.
fn call_target(cx: &Cx, heap: &mut Heap, locals: &Locals, what: &str, site: &CallSite, ty: &Type) -> Result<CompileTarget, Error> {
    // A name no source can bind: `'` ends a symbol.
    let arg = typelisp_mem::symbols::intern("format''arg");
    let shown = format!("~/{}/", site.name);
    let as_call = format!("({} arg colon at) with arg a `{}`", site.name, ty);
    let head = if site.name.contains("::") {
        let parts: Vec<&str> = site.name.split("::").collect();
        let syms = heap.names().read_path(cx.ns, &parts).map_err(|e| Error::TypeError(format!("{}: {}: {}", what, shown, e)))?;
        heap.intern_path(&syms)
    } else {
        Value::Symbol(heap.names().read(cx.ns, &site.name))
    };
    let call = crate::check::forms::list_from_vec_locs(heap, &[(head, None), (Value::Symbol(arg), None), (Value::Bool(site.colon), None), (Value::Bool(site.at), None)])?;
    heap.push_root(call);
    let checked = locals.bind(vec![(arg, ty.clone(), None)]).and_then(|inner| check_expr(cx, heap, &inner, call, Some(&Type::Str), None));
    heap.pop_root();
    let checked = checked.map_err(|e| {
        let inner = match e.kind() {
            Error::TypeError(m) => m.clone(),
            other => other.to_string(),
        };
        Error::TypeError(format!("{}: {} calls what {} would: {}", what, shown, as_call, inner))
    })?;
    let not_a_definition = || {
        Error::TypeError(format!(
            "{}: {} must call a function or a method written in typelisp that takes the argument as it is, and {} does not",
            what, shown, as_call
        ))
    };
    let (target, first, generic) = match &checked.node {
        Node::Call { path, targs, args } => {
            if cx.reg.fn_def(path).is_some_and(|d| d.builtin) {
                return Err(not_a_definition());
            }
            (CompileTarget::Fn(*path), args.first(), !targs.is_empty())
        }
        Node::MethodCall { owner, method, targs, args, .. } => {
            if cx.reg.method(owner, *method).is_some_and(|d| d.sig.builtin) {
                return Err(not_a_definition());
            }
            (CompileTarget::Method(*owner, *method), args.first(), !targs.is_empty())
        }
        _ => return Err(not_a_definition()),
    };
    if !first.is_some_and(|a| matches!(a.node, Node::Local { name } if name == arg)) {
        return Err(not_a_definition());
    }
    if generic {
        unimplemented!("typeck: {} calling `{}`, a generic definition", shown, target.written_name())
    }
    Ok(target)
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
