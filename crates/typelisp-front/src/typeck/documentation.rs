//! `(documentation name)`: the documentation string the definition `name`
//! was written with, `(some text)`, or `(none)` when it has none.
//!
//! The name is not evaluated: it is a symbol or a `::` path naming a
//! function, a macro, a global variable, a type or a trait, or a type's or a
//! trait's method. Which definition it names is decided here, so a name that
//! names none is an error before the program runs.

use crate::types::{path_spelling, path_target, PathTarget, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::registry::{Definition, TypeEntry, ValueDef};
use super::resolve::{ctor_named, find_named, sees, static_use_named, value_named};
use super::tree::Typed;

/// `(documentation name)` — `args` without the `documentation`.
pub(crate) fn check_documentation(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(name, _)] = args else {
        return Err(Error::TypeError("documentation: (documentation name) — expected exactly 1 argument".into()));
    };
    let doc = definition(cx, heap, *name, "documentation", MESSAGE)?;
    match cx.reg.docs.get(&doc).cloned() {
        Some(text) => {
            let text = heap.alloc_string(text);
            heap.push_root(text);
            let text = check_expr(cx, heap, locals, text, Some(&Type::Str), None)?;
            Ok(Typed::some(text, loc))
        }
        None => {
            let mut none = Typed::none(Type::Str);
            none.loc = loc;
            Ok(none)
        }
    }
}

/// The definition the name `v`, written for the form `who`, names;
/// `message` is the error for a `v` that is no name.
pub(crate) fn definition(cx: &Cx, heap: &Heap, v: Value, who: &str, message: &str) -> Result<Definition, Error> {
    let constructor = |name: &str| Error::TypeError(format!("{}: `{}` is a constructor, which has no definition of its own — name its type", who, name));
    match v {
        Value::Symbol(sym) => {
            let name = heap.symbol_spelling(sym).to_string();
            if ctor_named(cx.reg, cx.ns, sym).is_some() {
                return Err(constructor(&name));
            }
            if let Some((owner, method)) = static_use_named(cx.reg, cx.ns, sym) {
                return Ok(Definition::Method(owner, method));
            }
            if let Some((path, def)) = value_named(cx.reg, cx.ns, sym) {
                return Ok(value_doc(path, def));
            }
            // Types and traits are a name space of their own.
            match find_named(cx.reg, cx.ns, sym, |ns, key| cx.reg.types.get(&crate::types::Path::of_symbol(ns, key))) {
                Some((path, TypeEntry::Trait(_))) => Ok(Definition::Trait(path)),
                Some((path, _)) => Ok(Definition::Type(path)),
                None => Err(Error::TypeError(format!("{}: no definition named `{}`", who, name))),
            }
        }
        Value::Path(id) => {
            let name = path_spelling(heap, id);
            match path_target(heap, cx.ns, id)? {
                PathTarget::In(path) => {
                    if let Some(def) = cx.reg.values.get(&path).filter(|def| def.public() || sees(cx.ns, path.ns())) {
                        return Ok(value_doc(path, def));
                    }
                    match cx.reg.types.get(&path) {
                        Some(TypeEntry::Trait(_)) => Ok(Definition::Trait(path)),
                        Some(_) => Ok(Definition::Type(path)),
                        None => Err(Error::TypeError(format!("{}: no definition named `{}`", who, name))),
                    }
                }
                PathTarget::Member(owner, member) => {
                    let method = member_symbol(member.name());
                    match cx.reg.types.get(&owner) {
                        Some(TypeEntry::Type(def)) if def.variant_index(method).is_some() => Err(constructor(&name)),
                        Some(TypeEntry::Type(_)) => match super::tools::member_code(cx, owner, method, &name, who)? {
                            Some((code, _)) => Ok(Definition::Method(owner, code)),
                            None => Err(Error::TypeError(format!("{}: type `{}` has no associated function or method `{}`", who, owner, member.name()))),
                        },
                        Some(TypeEntry::Trait(def)) => match def.method(method) {
                            Some(_) => Ok(Definition::TraitMethod(owner, method)),
                            None => Err(Error::TypeError(format!("{}: trait `{}` has no method `{}`", who, owner, member.name()))),
                        },
                        _ => Err(Error::TypeError(format!("{}: `{}` is not a type with methods", who, owner))),
                    }
                }
            }
        }
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::TYPE_MEMBER)) => {
            let (owner, code, _) = super::tools::instantiation_member(cx, heap, v, who)?;
            Ok(Definition::Method(owner, code))
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

const MESSAGE: &str = "documentation: expected a symbol or path naming a definition, e.g. (documentation foo) or (documentation point::x) — not a string";

/// The module-level definition `def` at `path`.
fn value_doc(path: crate::types::Path, def: &ValueDef) -> Definition {
    match def {
        ValueDef::Fn(_) => Definition::Fn(path),
        ValueDef::Var(_) => Definition::Var(path),
        ValueDef::Macro { .. } => Definition::Macro(path),
    }
}
