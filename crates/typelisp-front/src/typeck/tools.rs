//! The forms that name one function's body for a tool: `(compile name)`,
//! `(trace name...)`, `(untrace name...)` and `(disassemble name)`; and
//! `(ed name)`, which names the place a definition was written.
//!
//! The name is not evaluated: it is a symbol or a `::` path naming a
//! function, a macro or a type's method. A generic definition has no single
//! body, its instantiations being made where it is called, so none of
//! these forms takes one.

use crate::check::resolved::CompileTarget;
use crate::typeck::sexpr_value::sexpr_ty;
use crate::types::{path_spelling, path_target, Path, PathTarget, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::generic::method_specialization;
use super::registry::{MethodDef, TypeEntry, ValueDef};
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

/// `(ed)`, `(ed "file")` or `(ed name)` — `args` without the `ed`: the
/// editor opened on nothing, on the file, or on the file and the line the
/// definition `name` was written at. Which place is decided here; what runs
/// is a call of the `ed-open` builtin with the two.
pub(crate) fn check_ed(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let (file, line) = match args {
        [] => (String::new(), 0),
        [(Value::Str(id), _)] => (heap.string(*id).to_string(), 0),
        [(name, _)] => {
            let message = "ed: expected a symbol or path naming a definition, or a string naming a file, e.g. (ed foo) or (ed \"x.typl\")";
            let def = super::documentation::definition(cx, heap, *name, "ed", message)?;
            match cx.reg.locs.get(&def) {
                Some(at) => (at.file.to_string(), at.line as i64),
                None => {
                    let written = match *name {
                        Value::Symbol(sym) => sym.name().to_string(),
                        Value::Path(id) => path_spelling(heap, id),
                        // `Type<args>::member`, which `definition` read.
                        member => match heap.list_to_vec(member)?.as_slice() {
                            // Read once more for the message, after `definition` noted it.
                            [_, ty, Value::Symbol(m)] => format!("{}::{}", super::type_syntax::parse_type(heap, &cx.types(), *ty, None)?, m.name()),
                            _ => unreachable!("`definition` names a definition only for a name"),
                        },
                    };
                    return Err(Error::TypeError(format!("ed: `{}` has no source location — it is built in, so there is no file to open", written)));
                }
            }
        }
        _ => return Err(Error::TypeError("ed: (ed [name-or-path]) — expected at most 1 argument".into())),
    };
    let path = Path::internal_builtin(wk::F_ED_OPEN);
    let Some(ValueDef::Fn(open)) = cx.reg.values.get(&path) else {
        return Err(Error::TypeError("ed: the `ed-open` builtin is not registered".into()));
    };
    let ret = open.ret.clone();
    let file = heap.alloc_string(file);
    heap.push_root(file);
    let file = check_expr(cx, heap, locals, file, Some(&Type::Str), None)?;
    let line = check_expr(cx, heap, locals, Value::Int(line), Some(&Type::Int), None)?;
    Ok(Typed { node: Node::Call { path, targs: Vec::new(), args: vec![file, line] }, ty: ret, loc })
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
            if let Some((owner, member)) = static_use_named(cx.reg, cx.ns, sym) {
                return member_target(cx, owner, member, &name, who, generic);
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
                PathTarget::Member(owner, member) => member_target(cx, owner, member, &name, who, generic),
            }
        }
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::TYPE_MEMBER)) => {
            let (owner, code, m) = instantiation_member(cx, heap, v, who)?;
            if !m.sig.builtin && !m.sig.type_params.is_empty() {
                return Err(generic(&format!("{}::{}", owner, code.name())));
            }
            Ok(CompileTarget::Method(owner, code))
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

/// The member `member` of `owner`, written `name`, as what `who` acts on:
/// a method, or a static one; `generic` is the error for a generic one.
fn member_target(cx: &Cx, owner: Path, member: SymRef, name: &str, who: &str, generic: impl Fn(&str) -> Error) -> Result<CompileTarget, Error> {
    let def = match cx.reg.types.get(&owner) {
        Some(TypeEntry::Type(def)) => def,
        Some(TypeEntry::Trait(_)) => {
            return Err(Error::TypeError(format!(
                "{}: `{}` names a method of the trait `{}`, which each type implementing it has its own of — name the type's, as `Type::{}`",
                who,
                name,
                typelisp_print::shared::name_view().definition(owner.ns(), owner.name_symbol()),
                member.name()
            )))
        }
        _ => return Err(Error::TypeError(format!("{}: `{}` is not a type with methods", who, owner))),
    };
    let method = member_symbol(member.name());
    if def.variant_index(method).is_some() {
        return Err(Error::TypeError(format!("{}: `{}` is a constructor, not a function", who, name)));
    }
    match member_code(cx, owner, method, name, who)? {
        Some((_, m)) if !m.sig.builtin && !m.sig.type_params.is_empty() => Err(generic(name)),
        Some((code, _)) => Ok(CompileTarget::Method(owner, code)),
        None => Err(Error::TypeError(format!("{}: type `{}` has no associated function or method `{}`", who, owner, member.name()))),
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

/// The method `method` of `owner`, which the path `spelling` names for the
/// form `who`, with the name its code has: the one for every instantiation,
/// or the one instantiation's when only one has it. `None` when `owner` has
/// no such method.
pub(crate) fn member_code<'r>(cx: &Cx<'r>, owner: Path, method: SymRef, spelling: &str, who: &str) -> Result<Option<(SymRef, &'r MethodDef)>, Error> {
    if let Some(m) = cx.reg.method(&owner, method) {
        return Ok(Some((method, m)));
    }
    match cx.reg.inst_methods(&owner, method) {
        [] => Ok(None),
        [(args, m)] => Ok(Some((method_specialization(&owner, method, args), m))),
        many => {
            let names: Vec<String> = many.iter().map(|(a, _)| format!("`{}::{}`", Type::Named(owner, a.clone()), method.name())).collect();
            Err(Error::TypeError(format!("{}: `{}` is written for more than one instantiation — name one: {}", who, spelling, names.join(", "))))
        }
    }
}

/// The method `Type<args>::member` (`v`, read as `(%type-member Type
/// member)`) names for the form `who`: its type, the name its code has, and
/// its definition — the instantiation's own, or the one for every
/// instantiation.
pub(crate) fn instantiation_member<'r>(cx: &Cx<'r>, heap: &Heap, v: Value, who: &str) -> Result<(Path, SymRef, &'r MethodDef), Error> {
    let parts = heap.list_to_vec_locs(v)?;
    let [_, (ty, ty_at), (Value::Symbol(member), _)] = parts.as_slice() else {
        return Err(Error::TypeError(format!("{}: expected `Type<args>::member`", who)));
    };
    let ty = super::type_syntax::parse_type(heap, &cx.types(), *ty, ty_at.as_ref())?;
    let Type::Named(owner, args) = &ty else {
        return Err(Error::TypeError(format!("{}: `{}` has no methods of its own", who, ty)));
    };
    let method = member_symbol(member.name());
    match cx.reg.method_at(owner, method, args) {
        Some(m) => {
            let code = m.inst_args.as_ref().map_or(method, |a| method_specialization(owner, method, a));
            Ok((*owner, code, m))
        }
        None => Err(Error::TypeError(format!("{}: `{}` has no method `{}` of its own", who, ty, member.name()))),
    }
}
