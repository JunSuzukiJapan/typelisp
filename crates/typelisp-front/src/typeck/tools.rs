//! The forms that name one function's body for a tool: `(compile name)`,
//! `(trace name...)`, `(untrace name...)` and `(disassemble name)`.
//!
//! The name is not evaluated: it is a symbol or a `::` path naming a
//! function, a macro or a type's method. A generic definition has no single
//! body, its instantiations being made where it is called, so none of
//! these forms takes one.

use crate::check::resolved::CompileTarget;
use crate::check::analyze::sexpr_ty;
use crate::types::{path_spelling, path_target, Path, PathTarget, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::expr::Cx;
use super::registry::{TypeEntry, ValueDef};
use super::resolve::{ctor_named, sees, static_use_named, value_named};
use super::tree::{Node, Typed};

/// `(compile name)` — `args` without the `compile`.
pub(crate) fn check_compile(cx: &Cx, heap: &Heap, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(name, _)] = args else {
        return Err(Error::TypeError("compile: (compile name) — expected exactly 1 argument".into()));
    };
    let message = "compile: expected a symbol or path naming a function, e.g. (compile foo) or (compile point::x) — not a string";
    let target = target(cx, heap, *name, "compile", message)?;
    Ok(Typed { node: Node::Tool { tag: wk::COMPILE, targets: vec![target], llvm_ir: false }, ty: Type::Bool, loc })
}

/// `(trace name...)` or `(untrace name...)`, `head` saying which — `args`
/// without the head. Its value is the names traced afterwards.
pub(crate) fn check_trace(cx: &Cx, heap: &Heap, head: SymRef, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let who = head.name();
    let message = format!("{}: expected symbols or `::`-paths naming functions, e.g. ({} foo point::x) — not a string", who, who);
    let targets = args.iter().map(|(a, _)| target(cx, heap, *a, who, &message)).collect::<Result<Vec<_>, _>>()?;
    Ok(Typed { node: Node::Tool { tag: head.well_known(), targets, llvm_ir: false }, ty: sexpr_ty(), loc })
}

/// `(disassemble name)` or `(disassemble name llvm)`, `llvm` a literal
/// saying whether to show LLVM IR rather than the machine's assembly —
/// `args` without the `disassemble`.
pub(crate) fn check_disassemble(cx: &Cx, heap: &Heap, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let (name, llvm_ir) = match args {
        [(one, _)] => (*one, false),
        [(one, _), (Value::Bool(b), _)] => (*one, *b),
        [_, _] => {
            return Err(Error::TypeError("disassemble: the second argument selects LLVM IR and must be a literal `true` or `false`".into()));
        }
        _ => return Err(Error::TypeError("disassemble: (disassemble name [llvm]) — expected 1 or 2 arguments".into())),
    };
    let message = "disassemble: expected a symbol or path naming a function, e.g. (disassemble foo) or (disassemble point::x) — not a string";
    let target = target(cx, heap, name, "disassemble", message)?;
    Ok(Typed { node: Node::Tool { tag: wk::DISASSEMBLE, targets: vec![target], llvm_ir }, ty: Type::Unit, loc })
}

/// The body the name `v`, written for the tool `who`, names; `message` is
/// the error for a `v` that is no name.
fn target(cx: &Cx, heap: &Heap, v: Value, who: &str, message: &str) -> Result<CompileTarget, Error> {
    let generic = |name: &str| {
        Error::TypeError(format!(
            "{}: `{}` is generic — monomorphization runs per use, so there is no single body \
             to name; call it at concrete types and name those uses' enclosing functions instead",
            who, name
        ))
    };
    match v {
        Value::Symbol(sym) => {
            let name = heap.symbol_spelling(sym).to_string();
            if ctor_named(cx.reg, cx.ns, sym).is_some() {
                return Err(Error::TypeError(format!("{}: `{}` is a constructor, not a function", who, name)));
            }
            if static_use_named(cx.reg, cx.ns, sym).is_some() {
                unimplemented!("typeck: `{}` of a static method a `use` brought in", who)
            }
            match value_named(cx.reg, cx.ns, sym) {
                Some((path, def)) => value_target(path, def, who, &name).map_err(|e| e.unwrap_or_else(|| generic(&name))),
                None => Err(Error::TypeError(format!("{}: no function `{}` is visible from here", who, name))),
            }
        }
        Value::Path(id) => {
            let name = path_spelling(heap, id);
            match path_target(heap, cx.ns, id)? {
                PathTarget::In(path) => match cx.reg.values.get(&path).filter(|def| def.public() || sees(cx.ns, path.ns())) {
                    Some(def) => value_target(path, def, who, &name).map_err(|e| e.unwrap_or_else(|| generic(&name))),
                    None => Err(Error::TypeError(format!("{}: no function `{}` is visible from here", who, name))),
                },
                PathTarget::Member(owner, member) => {
                    let Some(TypeEntry::Type(def)) = cx.reg.types.get(&owner) else {
                        unimplemented!("typeck: `{}` of the path `{}`, through a type this checker does not know", who, name)
                    };
                    let method = member_symbol(member.name());
                    if def.variant_index(method).is_some() {
                        return Err(Error::TypeError(format!("{}: `{}` is a constructor, not a function", who, name)));
                    }
                    match cx.reg.method(&owner, method) {
                        Some(m) if !m.sig.builtin && !m.sig.type_params.is_empty() => Err(generic(&name)),
                        Some(_) => Ok(CompileTarget::Method(owner, method)),
                        None if cx.reg.has_inst_method(&owner, method) => {
                            unimplemented!("typeck: `{}` of `{}`, a method written for one instantiation", who, name)
                        }
                        None => Err(Error::TypeError(format!("{}: type `{}` has no associated function or method `{}`", who, owner, member.name()))),
                    }
                }
            }
        }
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::TYPE_MEMBER)) => {
            unimplemented!("typeck: `{}` of a method of one instantiation (`Type<args>::member`)", who)
        }
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::VALUE_MEMBER)) => {
            let parts = heap.list_to_vec(v)?;
            let [_, Value::Symbol(recv), Value::Symbol(member)] = parts.as_slice() else {
                return Err(Error::TypeError(message.into()));
            };
            Err(Error::TypeError(format!(
                "{}: `{}::{}` names neither a type's method nor a function (`{}` is no module or type visible here)",
                who,
                recv.name(),
                member.name(),
                recv.name()
            )))
        }
        _ => Err(Error::TypeError(message.into())),
    }
}

/// The body the module-level `def`, at `path` and written `name`, is:
/// `Err(None)` for a generic function, which has none.
fn value_target(path: Path, def: &ValueDef, who: &str, name: &str) -> Result<CompileTarget, Option<Error>> {
    match def {
        ValueDef::Fn(f) if !f.builtin && !f.type_params.is_empty() => Err(None),
        ValueDef::Fn(_) | ValueDef::Macro { .. } => Ok(CompileTarget::Fn(path)),
        ValueDef::Var(_) => Err(Some(Error::TypeError(format!("{}: `{}` is a variable, not a function", who, name)))),
    }
}
