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
use super::registry::{Doc, TypeEntry, ValueDef};
use super::resolve::{ctor_named, find_named, sees, static_use_named, value_named};
use super::tree::Typed;

/// `(documentation name)` — `args` without the `documentation`.
pub(crate) fn check_documentation(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(name, _)] = args else {
        return Err(Error::TypeError("documentation: (documentation name) — expected exactly 1 argument".into()));
    };
    let doc = definition(cx, heap, *name)?;
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

/// The definition the name `v` names, as the table of documentation
/// strings keys it.
fn definition(cx: &Cx, heap: &Heap, v: Value) -> Result<Doc, Error> {
    let constructor = |name: &str| Error::TypeError(format!("documentation: `{}` is a constructor, which has no definition of its own — name its type", name));
    match v {
        Value::Symbol(sym) => {
            let name = heap.symbol_spelling(sym).to_string();
            if ctor_named(cx.reg, cx.ns, sym).is_some() {
                return Err(constructor(&name));
            }
            if let Some((owner, method)) = static_use_named(cx.reg, cx.ns, sym) {
                return Ok(Doc::Method(owner, method));
            }
            if let Some((path, def)) = value_named(cx.reg, cx.ns, sym) {
                return Ok(value_doc(path, def));
            }
            // Types and traits are a name space of their own.
            match find_named(cx.reg, cx.ns, sym, |ns, key| cx.reg.types.get(&crate::types::Path::of_symbol(ns, key))) {
                Some((path, TypeEntry::Trait(_))) => Ok(Doc::Trait(path)),
                Some((path, _)) => Ok(Doc::Type(path)),
                None => Err(Error::TypeError(format!("documentation: no definition named `{}`", name))),
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
                        Some(TypeEntry::Trait(_)) => Ok(Doc::Trait(path)),
                        Some(_) => Ok(Doc::Type(path)),
                        None => Err(Error::TypeError(format!("documentation: no definition named `{}`", name))),
                    }
                }
                PathTarget::Member(owner, member) => {
                    let method = member_symbol(member.name());
                    match cx.reg.types.get(&owner) {
                        Some(TypeEntry::Type(def)) if def.variant_index(method).is_some() => Err(constructor(&name)),
                        Some(TypeEntry::Type(_)) => match super::tools::member_code(cx, owner, method, &name, "documentation")? {
                            Some((code, _)) => Ok(Doc::Method(owner, code)),
                            None => Err(Error::TypeError(format!("documentation: type `{}` has no associated function or method `{}`", owner, member.name()))),
                        },
                        Some(TypeEntry::Trait(def)) => match def.method(method) {
                            Some(_) => Ok(Doc::TraitMethod(owner, method)),
                            None => Err(Error::TypeError(format!("documentation: trait `{}` has no method `{}`", owner, member.name()))),
                        },
                        _ => unimplemented!("typeck: `documentation` of `{}`, a member of what is not a type or a trait", name),
                    }
                }
            }
        }
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::TYPE_MEMBER)) => {
            let (owner, code, _) = super::tools::instantiation_member(cx, heap, v, "documentation")?;
            Ok(Doc::Method(owner, code))
        }
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::VALUE_MEMBER)) => {
            let parts = heap.list_to_vec(v)?;
            let [_, Value::Symbol(recv), Value::Symbol(member)] = parts.as_slice() else {
                return Err(Error::TypeError(MESSAGE.into()));
            };
            Err(Error::TypeError(format!(
                "documentation: `{}::{}` names neither a type's method nor a function (`{}` is no module or type visible here)",
                recv.name(),
                member.name(),
                recv.name()
            )))
        }
        _ => Err(Error::TypeError(MESSAGE.into())),
    }
}

const MESSAGE: &str = "documentation: expected a symbol or path naming a definition, e.g. (documentation foo) or (documentation point::x) — not a string";

/// The module-level definition `def` at `path`.
fn value_doc(path: crate::types::Path, def: &ValueDef) -> Doc {
    match def {
        ValueDef::Fn(_) => Doc::Fn(path),
        ValueDef::Var(_) => Doc::Var(path),
        ValueDef::Macro { .. } => Doc::Macro(path),
    }
}
