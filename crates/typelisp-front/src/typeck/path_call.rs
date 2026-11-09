//! A call whose head is a `::` path: `(Type::Variant value...)`,
//! `(Type::method arg...)` or `(module::function arg...)`.
//!
//! The path is followed as it was read (`types::path_target`): to a member
//! of a type, or to a name in a module.

use crate::check::registry::AdtKind;
use crate::types::{path_spelling, path_target, PathTarget, Type};
use crate::{Error, Heap, Loc, PathId, Value};
use typelisp_mem::symbols::member_symbol;

use super::call::check_call;
use super::expr::{check_expr, Cx};
use super::macros::expand;
use super::locals::Locals;
use super::method::check_method_call;
use super::registry::{TypeEntry, ValueDef};
use super::resolve::sees;
use super::tree::{Node, Typed};
use super::variant::check_variant;

/// `(path arg...)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_path_call(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    id: PathId,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    match path_target(heap, cx.ns, id)? {
        PathTarget::Member(owner, member) => {
            let def = match cx.reg.types.get(&owner) {
                Some(TypeEntry::Type(def)) => def,
                Some(TypeEntry::Trait(_)) => unimplemented!("typeck: calling a trait's method through its path (`{}`)", path_spelling(heap, id)),
                _ => unimplemented!("typeck: the path `{}`, through a type this checker does not know", path_spelling(heap, id)),
            };
            let name = member_symbol(member.name());
            if let Some(variant) = def.variant_index(name) {
                return check_variant(cx, heap, locals, owner, variant, args, expected, loc);
            }
            let Some(method) = cx.reg.method(&owner, name) else {
                if cx.reg.has_inst_method(&owner, name) {
                    unimplemented!("typeck: `{}`, a method written for one instantiation, called through its type", path_spelling(heap, id))
                }
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
                return Err(Error::TypeError(format!("unresolved path: {}", path_spelling(heap, id))));
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
                    path_spelling(heap, id)
                )));
            }
            check_method_call(cx, heap, locals, owner, name, method, None, args, expected, loc)
        }
        // A definition another module does not make `pub` is not there for
        // this one.
        PathTarget::In(path) => match cx.reg.values.get(&path).filter(|def| def.public() || sees(cx.ns, path.ns())) {
            Some(ValueDef::Fn(def)) => check_call(cx, heap, locals, path, def, None, args, expected, loc),
            Some(ValueDef::Var(_)) => unimplemented!("typeck: calling the function the global `{}` holds", path_spelling(heap, id)),
            Some(ValueDef::Macro { shape, .. }) => {
                let spelling = path_spelling(heap, id);
                let expanded = expand(cx.expander, heap, &path, *shape, &spelling, args)?;
                check_expr(cx, heap, locals, expanded, expected, loc)
            }
            None => Err(Error::TypeError(format!("unresolved path: {}", path_spelling(heap, id)))),
        },
    }
}

/// The value the path `id` names: a global variable of a module.
pub(crate) fn check_path_value(cx: &Cx, heap: &Heap, id: PathId, loc: Option<Loc>) -> Result<Typed, Error> {
    match path_target(heap, cx.ns, id)? {
        PathTarget::In(path) => match cx.reg.values.get(&path).filter(|def| def.public() || sees(cx.ns, path.ns())) {
            Some(ValueDef::Var(var)) => Ok(Typed { node: Node::Global { path }, ty: var.ty.clone(), loc }),
            Some(ValueDef::Fn(def)) => super::lambda::fn_value(path, def, loc),
            Some(ValueDef::Macro { .. }) => unimplemented!("typeck: the macro `{}` as a value", path_spelling(heap, id)),
            None => Err(Error::TypeError(format!("unresolved path: {}", path_spelling(heap, id)))),
        },
        PathTarget::Member(..) => unimplemented!("typeck: the member `{}` of a type as a value", path_spelling(heap, id)),
    }
}
