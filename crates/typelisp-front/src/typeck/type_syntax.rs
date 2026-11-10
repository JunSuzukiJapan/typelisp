//! Reading a written type.

use crate::types::{is_dyn_form, is_type_application, scalar_of_symbol, scalar_type, tuple_path, written_path, Path, Type};
use crate::{wk, Error, Heap, Value};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::registry::{Registry, TraitDef, TypeDef, TypeEntry};
use super::resolve::{find_named, sees};
use super::scope::TypeScope;
use super::subst::Subst;

/// Where a type is read: the definitions, the module, the type parameters
/// declared there, and inside an `impl`, the type `Self` stands for.
pub(crate) struct TypeCx<'a> {
    pub(crate) reg: &'a Registry,
    pub(crate) ns: NsId,
    pub(crate) scope: &'a TypeScope<'a>,
    pub(crate) self_type: Option<&'a Type>,
}

/// The type `v` writes.
///
/// A bare name is first a declared type parameter, then `Self`, then a
/// built-in scalar type, then a type the module sees. Writing a name never declares it
/// (`docs/dev/checker-rebuild-plan.md` 7.5). `Name<A, B>` reads as
/// `(%type-apply Name A B)`, and `(Name A B)` is the same type. A `::` path
/// names a type in a module. `(fn (A B &rest C) R)` is a function's type
/// and `#{A B}` a tuple's. A C word (`ptr`, `c-long`, `c-ulong`) is the
/// whole type or nothing: every slot inside another type tags what it holds,
/// which would drop the word's top three bits. A C pointer `(ptr T)` is not
/// read so far.
pub(crate) fn parse_type(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    let ty = parse_raw(heap, cx, v)?;
    no_nested_raw_word(&ty)?;
    Ok(ty)
}

/// Refuse a C word inside `ty` (it may be `ty` itself).
pub(crate) fn no_nested_raw_word(ty: &Type) -> Result<(), Error> {
    let nested = match ty {
        Type::Named(_, args) | Type::Dyn(_, args) => args.iter().any(mentions_raw_word),
        Type::Fn(ps, rest, ret) => ps.iter().chain(rest.as_deref()).any(mentions_raw_word) || mentions_raw_word(ret),
        _ => false,
    };
    if nested {
        return Err(Error::TypeError(format!(
            "`{}` puts a raw C word inside another type, and every such slot tags what it \
             holds — which would drop the word's top three bits. A `ptr`/`c-long`/`c-ulong` \
             can be a parameter or a return type, and nothing else.",
            ty
        )));
    }
    Ok(())
}

/// Whether `ty` is, or holds, a C word or a C pointer.
pub(crate) fn mentions_raw_word(ty: &Type) -> bool {
    match ty {
        Type::Ptr | Type::CLong | Type::CULong | Type::PtrTo(_) => true,
        Type::Named(_, args) | Type::Dyn(_, args) => args.iter().any(mentions_raw_word),
        Type::Fn(ps, rest, ret) => ps.iter().chain(rest.as_deref()).any(mentions_raw_word) || mentions_raw_word(ret),
        _ => false,
    }
}

/// [`parse_type`] without its rule for the C words: a `defffi` callback's
/// type may take and answer them.
pub(crate) fn parse_raw(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    match v {
        Value::Empty => Ok(Type::Unit),
        Value::Symbol(id) => {
            if let Some(t) = cx.scope.lookup_type(id) {
                return Ok(t);
            }
            if id.is(wk::SELF_TYPE) {
                return match cx.self_type {
                    Some(t) => Ok(t.clone()),
                    None => Err(Error::TypeError("`Self` is the type an `impl` or a `deftrait` is for, and is written only inside one".into())),
                };
            }
            match scalar_of_symbol(id) {
                Some(t) => Ok(t),
                None => named(heap, cx, id, Vec::new()),
            }
        }
        Value::Path(_) => {
            let path = written_path(heap, cx.ns, v)?;
            match scalar_type(&path) {
                Some(t) => Ok(t),
                None => at_path(cx, path, Vec::new()),
            }
        }
        Value::Cons(_) if is_dyn_form(heap, v) => dyn_type(heap, cx, v),
        // `(ptr T)`: a typed pointer.
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::PTR)) => {
            let [_, pointee] = heap.list_to_vec(v)?[..] else {
                return Err(Error::TypeError("a typed pointer is written (ptr T)".into()));
            };
            Ok(Type::PtrTo(Box::new(super::c_memory::pointee(heap, cx, pointee)?)))
        }
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::FN)) => fn_type(heap, cx, v),
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::TYPE_MEMBER)) => {
            Err(Error::TypeError("`Name<Args>::member` names a member of a type, not a type".to_string()))
        }
        Value::Cons(_) => {
            // `(%type-apply Name A B)`, or the list spelling `(Name A B)`.
            let parts = heap.list_to_vec(v)?;
            let (head, args) = if is_type_application(heap, v) { (parts[1], &parts[2..]) } else { (parts[0], &parts[1..]) };
            if args.is_empty() {
                return Err(Error::TypeError("a type application must be (Name Arg...), with at least one argument".to_string()));
            }
            let args = args.iter().map(|a| parse_raw(heap, cx, *a)).collect::<Result<Vec<_>, _>>()?;
            match head {
                Value::Symbol(name) => named(heap, cx, name, args),
                Value::Path(_) => at_path(cx, written_path(heap, cx.ns, head)?, args),
                other => Err(Error::TypeError(format!("a type application's head must be a plain type name, found `{:?}`", other))),
            }
        }
        // `#{A B}`, written the way its values are.
        Value::Boxed(id) if heap.sexpr_tuple_arity(id).is_some() => {
            let n = heap.struct_field_count(id);
            let args = (0..n).map(|i| parse_raw(heap, cx, heap.struct_field(id, i))).collect::<Result<Vec<_>, _>>()?;
            Ok(Type::Named(tuple_path(n), args))
        }
        other => Err(Error::TypeError(format!("not a type expression: {:?}", other))),
    }
}

/// `(fn (A B &rest C) R)`: a function's type.
fn fn_type(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    let elems = heap.list_to_vec(v)?;
    let [_, params, ret] = elems.as_slice() else {
        return Err(Error::TypeError("fn type must be (fn (params) ret)".to_string()));
    };
    let ps = match params {
        Value::Empty => Vec::new(),
        Value::Cons(_) => heap.list_to_vec(*params)?,
        _ => return Err(Error::TypeError("fn parameter list must be a list".to_string())),
    };
    let (fixed, rest) = match ps.iter().position(|p| matches!(p, Value::Symbol(id) if id.is(wk::REST))) {
        Some(i) if i + 2 == ps.len() => (&ps[..i], Some(Box::new(parse_raw(heap, cx, ps[i + 1])?))),
        Some(_) => {
            return Err(Error::TypeError(
                "fn type: &rest must be followed by exactly one type, as the last item in the parameter list".to_string(),
            ))
        }
        None => (&ps[..], None),
    };
    let fixed = fixed.iter().map(|p| parse_raw(heap, cx, *p)).collect::<Result<Vec<_>, _>>()?;
    Ok(Type::Fn(fixed, rest, Box::new(parse_raw(heap, cx, *ret)?)))
}

/// The type the path `path` names, applied to `args`. One its module does
/// not make `pub` is not seen from another.
fn at_path(cx: &TypeCx, path: Path, args: Vec<Type>) -> Result<Type, Error> {
    let visible = |public: bool| public || sees(cx.ns, path.ns());
    match cx.reg.types.get(&path) {
        Some(entry @ TypeEntry::Type(TypeDef { public, .. })) if visible(*public) => entry_type(path, entry, args),
        Some(entry @ TypeEntry::Alias { public, .. }) if visible(*public) => entry_type(path, entry, args),
        Some(entry @ TypeEntry::Trait(TraitDef { public, .. })) if visible(*public) => entry_type(path, entry, args),
        Some(entry @ TypeEntry::CStruct(_)) => entry_type(path, entry, args),
        _ => Err(unknown_type(&path.to_string())),
    }
}

/// `(:dyn Trait)` or `(:dyn Trait<pins...>)`: a trait object of the trait,
/// its associated types pinned.
fn dyn_type(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    let elems = heap.list_to_vec(v)?;
    let [_, inner] = elems.as_slice() else {
        return Err(Error::TypeError("`:dyn` must be followed by exactly one trait name".to_string()));
    };
    let (head, pins) = match inner {
        Value::Cons(_) if is_type_application(heap, *inner) => {
            let parts = heap.list_to_vec(*inner)?;
            (parts[1], parts[2..].to_vec())
        }
        _ => (*inner, Vec::new()),
    };
    let found = match head {
        Value::Symbol(id) => find_named(cx.reg, cx.ns, id, |ns, key| cx.reg.types.get(&Path::of_symbol(ns, key))).map(|(p, e)| (p, e.clone())),
        Value::Path(_) => {
            let path = written_path(heap, cx.ns, head)?;
            cx.reg.types.get(&path).map(|e| (path, e.clone()))
        }
        _ => None,
    };
    let Some((path, TypeEntry::Trait(TraitDef { public, .. }))) = found else {
        return Err(Error::TypeError(format!("`:dyn` must be followed by a trait name, found `{}`", crate::check::core::print(heap, *inner))));
    };
    if !public && !sees(cx.ns, path.ns()) {
        return Err(unknown_type(&path.to_string()));
    }
    let pins = pins.iter().map(|p| parse_raw(heap, cx, *p)).collect::<Result<Vec<_>, _>>()?;
    Ok(Type::Dyn(path, pins))
}

/// The type the name `id` stands for, applied to `args`.
/// The path of the type the name `v` (a symbol or a `::` path) names.
pub(crate) fn type_path(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Path, Error> {
    match v {
        Value::Symbol(id) => match find_named(cx.reg, cx.ns, id, |ns, key| cx.reg.types.get(&Path::of_symbol(ns, key))) {
            Some((path, _)) => Ok(path),
            None => Err(unknown_type(&heap.symbol_spelling(id))),
        },
        _ => written_path(heap, cx.ns, v),
    }
}

fn named(heap: &Heap, cx: &TypeCx, id: SymRef, args: Vec<Type>) -> Result<Type, Error> {
    match find_named(cx.reg, cx.ns, id, |ns, key| cx.reg.types.get(&Path::of_symbol(ns, key))) {
        Some((path, entry)) => entry_type(path, entry, args),
        None => Err(unknown_type(&heap.symbol_spelling(id))),
    }
}

/// The error for a type name `spelling` that names no type here.
fn unknown_type(spelling: &str) -> Error {
    Error::TypeError(format!(
            "unknown type `{}`: no type of that name is visible here. A type has to be defined before \
             the first form that names it — types have no forward declaration (`defsignature` declares \
             functions only). If `{}` is meant to be a type variable, declare it: in the name of a \
             `defun`/`defmethod`/`defstruct`/`defenum`/`deftype` (`name<{}>`), in `impl<{}>`, or as a \
             `deftrait`'s `(:type {})`.",
        spelling, spelling, spelling, spelling, spelling
    ))
}

/// The type the definition `entry` at `path` is, applied to `args`.
fn entry_type(path: Path, entry: &TypeEntry, args: Vec<Type>) -> Result<Type, Error> {
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
        // A spelling of `body`, which is written where it is used.
        TypeEntry::Alias { params, body, .. } => {
            if params.len() != args.len() {
                return Err(Error::TypeError(format!("`{}` takes {} type argument(s), got {}", path, params.len(), args.len())));
            }
            Ok(Subst::positional(params, &args).apply(body))
        }
        TypeEntry::Trait(_) => unimplemented!("typeck: the trait `{}` written where a type is", path),
        // A C struct is no value's type: only a typed pointer points at one.
        TypeEntry::CStruct(_) => Err(unknown_type(path.last_segment())),
    }
}
