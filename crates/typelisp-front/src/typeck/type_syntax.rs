//! Reading a written type.

use crate::types::{is_type_application, scalar_of_symbol, Path, Type};
use crate::{Error, Heap, Value};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::registry::{Registry, TypeEntry};
use super::resolve::find_named;
use super::scope::TypeScope;

/// Where a type is read: the definitions, the module, and the type
/// parameters declared there.
pub(crate) struct TypeCx<'a> {
    pub(crate) reg: &'a Registry,
    pub(crate) ns: NsId,
    pub(crate) scope: &'a TypeScope<'a>,
}

/// The type `v` writes.
///
/// A bare name is first a declared type parameter, then a built-in scalar
/// type, then a type the module sees. Writing a name never declares it
/// (`docs/dev/checker-rebuild-plan.md` 7.5). `Name<A, B>` reads as
/// `(%type-apply Name A B)`. So far a type alias, a function type, a path
/// and the C words are not read: they are read when the checker can check
/// what they are used for.
pub(crate) fn parse_type(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    match v {
        Value::Empty => Ok(Type::Unit),
        Value::Symbol(id) => {
            if let Some(p) = cx.scope.lookup(id) {
                return Ok(Type::Param(p));
            }
            match scalar_of_symbol(id) {
                Some(Type::Ptr | Type::CLong | Type::CULong) => {
                    unimplemented!("typeck: the C word type `{}`", heap.symbol_spelling(id))
                }
                Some(t) => Ok(t),
                None => named(heap, cx, id, Vec::new()),
            }
        }
        Value::Cons(_) if is_type_application(heap, v) => {
            let parts = heap.list_to_vec(v)?;
            let Some(Value::Symbol(name)) = parts.get(1) else {
                unimplemented!("typeck: a type application whose head is a path")
            };
            let args = parts[2..].iter().map(|a| parse_type(heap, cx, *a)).collect::<Result<Vec<_>, _>>()?;
            named(heap, cx, *name, args)
        }
        _ => unimplemented!("typeck: a function type or a path in a type"),
    }
}

/// The type the name `id` stands for, applied to `args`.
fn named(heap: &Heap, cx: &TypeCx, id: SymRef, args: Vec<Type>) -> Result<Type, Error> {
    let spelling = heap.symbol_spelling(id);
    let Some((path, entry)) = find_named(cx.reg, cx.ns, id, |ns, key| cx.reg.types.get(&Path::of_symbol(ns, key))) else {
        return Err(Error::TypeError(format!(
            "unknown type `{}`: no type of that name is visible here. A type has to be defined before \
             the first form that names it — types have no forward declaration (`defsignature` declares \
             functions only). If `{}` is meant to be a type variable, declare it: in the name of a \
             `defun`/`defmethod`/`defstruct`/`defenum`/`deftype` (`name<{}>`), in `impl<{}>`, or as a \
             `deftrait`'s `(:type {})`.",
            spelling, spelling, spelling, spelling, spelling
        )));
    };
    match entry {
        TypeEntry::Type(def) => {
            if def.params.len() != args.len() {
                return Err(Error::TypeError(format!(
                    "`{}` takes {} type argument(s), got {}",
                    path,
                    def.params.len(),
                    args.len()
                )));
            }
            Ok(Type::Named(path, args))
        }
        TypeEntry::Alias => unimplemented!("typeck: the type alias `{}`", path),
        TypeEntry::Trait => unimplemented!("typeck: the trait `{}` written where a type is", path),
        TypeEntry::CStruct => unimplemented!("typeck: the C struct `{}` written where a type is", path),
    }
}
