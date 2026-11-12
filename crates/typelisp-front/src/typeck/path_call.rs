//! A call whose head is a `::` path: `(Type::Variant value...)`,
//! `(Type::method arg...)` or `(module::function arg...)`.
//!
//! The path is followed as it was read (`types::path_target`): to a member
//! of a type, or to a name in a module.

use crate::typeck::registry::AdtKind;
use crate::types::{path_spelling, path_target, Path, PathTarget, Type};
use crate::{Error, Heap, Loc, PathId, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::call::check_call;
use super::expr::{check_expr, Cx};
use super::macros::expand;
use super::locals::Locals;
use super::method::check_method_call;
use super::registry::{TypeEntry, ValueDef};
use super::resolve::{reexported, sees};
use super::tree::{Node, Typed};
use super::variant::check_variant;
use super::widen::expect;

/// `(path arg...)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_path_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    id: PathId,
    at: Option<&Loc>,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    match path_target(heap, cx.ns, id)? {
        PathTarget::Member(owner, member) => {
            let spelling = path_spelling(heap, id);
            if let Some(entry) = cx.reg.types.get(&owner) {
                super::type_syntax::note_path_head(cx.notes(), owner, entry, &spelling, at);
            }
            check_member_call(cx, heap, locals, owner, None, member, &spelling, args, expected, loc)
        }
        // A definition another module does not make `pub` is not there for
        // this one.
        PathTarget::In(path) => {
            let path = reexported(cx.reg, path);
            match cx.reg.values.get(&path).filter(|def| def.public() || sees(cx.ns, path.ns())) {
                Some(ValueDef::Fn(def)) => check_call(cx, heap, locals, path, def, None, args, expected, loc),
                // A global holding a function: its value, called.
                Some(ValueDef::Var(var)) => {
                    let callee = Typed { node: Node::Global { path }, ty: var.ty.clone(), loc: at.cloned() };
                    super::lambda::check_apply(cx, heap, locals, callee, None, args, expected, loc)
                }
                Some(ValueDef::Macro { shape, .. }) => {
                    let spelling = path_spelling(heap, id);
                    let expanded = expand(cx.expander, heap, &path, *shape, &spelling, args)?;
                    check_expr(cx, heap, locals, expanded, expected, loc)
                }
                None => Err(Error::TypeError(format!("unresolved path: {}", path_spelling(heap, id)))),
            }
        }
    }
}

/// `(Type::member arg...)`, `spelling` how the path is written: a variant of
/// the type, or its static method. `written` are the type's arguments when
/// the path wrote them (`(Type<args>::member arg...)`): the value is of that
/// instantiation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_member_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    owner: Path,
    written: Option<&[Type]>,
    member: SymRef,
    spelling: &str,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let def = match cx.reg.types.get(&owner) {
        Some(TypeEntry::Type(def)) => def,
        // Which type's implementation is meant, the path does not say.
        Some(TypeEntry::Trait(_)) => {
            return Err(Error::TypeError(format!(
                "`{}` names a method of the trait `{}`, not of a type implementing it — call it through the type, as `(Type::{} ...)`",
                spelling,
                owner,
                member.name()
            )))
        }
        _ => return Err(Error::TypeError(format!("`{}`: `{}` is not a type with methods", spelling, owner))),
    };
    let name = member_symbol(member.name());
    if let Some(variant) = def.variant_index(name) {
        let Some(w) = written else {
            return check_variant(cx, heap, locals, owner, variant, args, expected, loc);
        };
        let value = check_variant(cx, heap, locals, owner, variant, args, Some(&Type::Named(owner, w.to_vec())), loc)?;
        expect(cx, &value.ty, expected)?;
        return Ok(value);
    }
    let method = match cx.reg.method(&owner, name) {
        Some(m) => Some(m),
        None if cx.reg.has_inst_method(&owner, name) => Some(match written {
            Some(w) => written_inst(cx, owner, name, spelling, w)?,
            None => inst_static(cx, owner, name, spelling, expected)?,
        }),
        None => None,
    };
    let Some(method) = method else {
        if def.kind == AdtKind::Struct && !def.builtin {
            return Err(Error::TypeError(format!(
                "`{}` has no static method `{}`; a struct's methods are written in its `impl`, \
                 e.g. (impl {} (pub {} (...) {} (make-instance '{} :slot value...)))",
                owner,
                member.name(),
                owner,
                member.name(),
                owner,
                owner
            )));
        }
        return Err(Error::TypeError(format!("unresolved path: {}", spelling)));
    };
    if method.instance {
        return Err(Error::TypeError(format!(
            "`{}::{}` is an instance method; call it as ({} obj ...)",
            owner,
            member.name(),
            member.name()
        )));
    }
    let home = method.home.map_or(owner.ns(), |h| h.ns());
    if !method.sig.public && !sees(cx.ns, home) {
        return Err(Error::TypeError(format!(
            "`{}` is not `pub`, so it is not visible outside its module",
            spelling
        )));
    }
    check_method_call(cx, heap, locals, owner, name, method, None, written.unwrap_or(&[]), args, expected, loc)
}

/// The static method `name` of `owner` written for the instantiation
/// `written`, which the call's path names (`spelling`).
fn written_inst<'r>(cx: &Cx<'r>, owner: Path, name: SymRef, spelling: &str, written: &[Type]) -> Result<&'r super::registry::MethodDef, Error> {
    let insts = cx.reg.inst_methods(&owner, name);
    if let Some((_, def)) = insts.iter().find(|(a, _)| a == written) {
        return Ok(def);
    }
    let have: Vec<String> = insts.iter().map(|(a, _)| format!("`{}`", Type::Named(owner, a.clone()))).collect();
    Err(Error::TypeError(format!("`{}` is written for {}, not for `{}`", spelling, have.join(", "), Type::Named(owner, written.to_vec()))))
}

/// The static method `name` of `owner`, written for one instantiation each
/// (`spelling` how its path is written): the one instantiation's, when
/// there is one, or the one of the instantiation the call is `expected` to
/// give.
fn inst_static<'r>(cx: &Cx<'r>, owner: Path, name: SymRef, spelling: &str, expected: Option<&Type>) -> Result<&'r super::registry::MethodDef, Error> {
    let insts = cx.reg.inst_methods(&owner, name);
    if let [(_, def)] = insts {
        return Ok(def);
    }
    if let Some(Type::Named(o, args)) = expected.map(|e| cx.resolve(e)) {
        if o == owner {
            if let Some((_, def)) = insts.iter().find(|(a, _)| *a == args) {
                return Ok(def);
            }
        }
    }
    let written: Vec<String> = insts.iter().map(|(a, _)| format!("`{}`", Type::Named(owner, a.clone()))).collect();
    Err(Error::TypeError(format!(
        "`{}` is written for {}, one instantiation each, and which one is meant is the type the call is expected to give — say it, e.g. `(the {} ({} ...))`",
        spelling,
        written.join(", "),
        Type::Named(owner, insts[0].0.clone()),
        spelling
    )))
}

/// The value the path `id` names: a global variable of a module.
pub(crate) fn check_path_value(cx: &Cx, heap: &Heap, id: PathId, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    match path_target(heap, cx.ns, id)? {
        PathTarget::In(path) => {
            let path = reexported(cx.reg, path);
            match cx.reg.values.get(&path).filter(|def| def.public() || sees(cx.ns, path.ns())) {
                Some(ValueDef::Var(var)) => Ok(Typed { node: Node::Global { path }, ty: var.ty.clone(), loc }),
                Some(ValueDef::Fn(def)) => super::lambda::fn_value(cx, path, def, expected, loc),
                Some(ValueDef::Macro { .. }) => Err(Error::TypeError(format!("`{}` is a macro, which has no value", path_spelling(heap, id)))),
                None => Err(Error::TypeError(format!("unresolved path: {}", path_spelling(heap, id)))),
            }
        }
        PathTarget::Member(owner, member) => match cx.reg.method(&owner, member_symbol(member.name())) {
            Some(def) if def.instance => Err(Error::TypeError(format!(
                "`{}` is an instance method, which a path does not name — where a function taking a `{}` first is expected, write its name, `{}`",
                path_spelling(heap, id),
                typelisp_print::shared::name_view().definition(owner.ns(), owner.name_symbol()),
                member.name()
            ))),
            _ => Err(Error::TypeError(format!(
                "`{}` is a constructor or a static method, which is called rather than taken as a value",
                path_spelling(heap, id)
            ))),
        },
    }
}
