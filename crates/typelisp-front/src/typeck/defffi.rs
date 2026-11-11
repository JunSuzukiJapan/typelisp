//! `(defffi name (Type...) Ret [:library "name"])`: a C function, declared.
//!
//! Like a `defsignature`, a name, the types it takes and the type it
//! answers, and no body; but the body is somebody else's, already compiled,
//! reached through a thunk the backend makes, so nothing will define it.
//! `(defffi (c-strlen "strlen") ...)` names the C symbol apart from the
//! function. Calling one needs `(unsafe ...)`. Whether a declared type can be
//! spelled in C is the backend's to say, once.

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::define::{Definer, Kind};
use super::defun::parse_header;
use super::registry::{FnDef, Registry, ValueDef};
use super::scope::TypeScope;
use super::type_syntax::{no_nested_raw_word, parse_raw, parse_type, TypeCx};

const SHAPE: &str = "defffi: (defffi name (param-type...) return-type [:library \"name\"])";

/// `(defffi ...)` — `parts` without the `defffi` — and its core form.
pub(crate) fn check_defffi(reg: &mut Registry, d: &mut Definer, heap: &mut Heap, parts: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    let [(header, _), (params, _), (ret, ret_at), options @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let (name, c_symbol) = names(heap, *header)?;
    let name = d.ownable(heap, name, "defffi")?;
    let root = TypeScope::empty();
    let tcx = TypeCx { reg, ns: d.site.ns, scope: &root, self_type: None, notes: d.notes };
    let written = heap.list_to_vec(*params)?;
    if written.iter().any(|p| matches!(p, Value::Symbol(s) if s.is(wk::REST))) {
        return Err(Error::TypeError(
            "defffi: `&rest` cannot be declared — a variadic C function passes its variadic \
             arguments under different rules than its fixed ones (on the stack, on AArch64 \
             Darwin), which a thunk built from a fixed signature does not follow. Declare each \
             arity you call as its own name."
                .into(),
        ));
    }
    let params = heap.list_to_vec_locs(*params)?.iter().map(|(p, at)| param_type(heap, &tcx, *p, at.as_ref())).collect::<Result<Vec<_>, _>>()?;
    let ret = parse_type(heap, &tcx, *ret, ret_at.as_ref())?;
    let library = library(heap, options)?;
    let path = Path::of_symbol(d.site.ns, name);
    match reg.values.get(&path) {
        Some(ValueDef::Fn(def)) if !def.builtin => return Err(Error::TypeError(format!("defffi: `{}` is already defined", name.name()))),
        _ => d.value(reg, Kind::Function, name)?,
    }
    let def = FnDef { public: d.public, ffi: true, ..FnDef::plain(Vec::new(), params.clone(), ret.clone()) };
    reg.define_fn(path, def);
    use crate::check::lower::CoreForms;
    reg.defffi_form(heap, &path, &c_symbol, library.as_deref(), &params, &ret)
}

/// The function's name, and the C symbol's: `name` for both, or `(name
/// "c_symbol")`. A C function has no type parameters to take.
fn names(heap: &Heap, v: Value) -> Result<(SymRef, String), Error> {
    let plain = |n: Value| -> Result<SymRef, Error> {
        let (name, type_names) = parse_header(heap, n)?;
        if !type_names.is_empty() {
            return Err(Error::TypeError(format!("defffi: `{}` takes type parameters, and a C function has none to take", name.name())));
        }
        Ok(name)
    };
    let bad = || Error::TypeError("defffi: a name is either `name` or `(name \"c_symbol\")`".into());
    match v {
        Value::Symbol(_) => plain(v).map(|n| (n, n.name().to_string())),
        Value::Cons(_) if crate::types::is_type_application(heap, v) => plain(v).map(|n| (n, n.name().to_string())),
        Value::Cons(_) => {
            let [name, Value::Str(id)] = heap.list_to_vec(v)?[..] else { return Err(bad()) };
            let name = match name {
                Value::Symbol(_) => plain(name)?,
                Value::Cons(_) if crate::types::is_type_application(heap, name) => plain(name)?,
                _ => return Err(bad()),
            };
            let c_symbol = heap.string(id).to_string();
            if c_symbol.is_empty() {
                return Err(Error::TypeError("defffi: the C symbol name is empty".into()));
            }
            Ok((name, c_symbol))
        }
        _ => Err(bad()),
    }
}

/// A parameter's type. A function type is a callback C calls, which crosses
/// as an entry's address, so it may take and answer C words.
fn param_type(heap: &Heap, tcx: &TypeCx, v: Value, at: Option<&Loc>) -> Result<Type, Error> {
    let ty = parse_raw(heap, tcx, v, at)?;
    let Type::Fn(params, rest, ret) = &ty else {
        no_nested_raw_word(&ty)?;
        return Ok(ty);
    };
    if rest.is_some() {
        return Err(Error::TypeError("defffi: a callback cannot be variadic — C calls it with a fixed signature".into()));
    }
    for t in params.iter().chain(std::iter::once(&**ret)) {
        if matches!(t, Type::Fn(..)) {
            return Err(Error::TypeError(format!(
                "defffi: `{}` is a callback taking or returning a function, which C \
                 could only do through another callback — declare that one as `ptr`",
                ty
            )));
        }
        no_nested_raw_word(t)?;
    }
    Ok(ty)
}

/// The options after the types: only `:library "name"`, where the symbol
/// is looked for. Without it, it is looked for in the running process.
fn library(heap: &Heap, options: &[(Value, Option<Loc>)]) -> Result<Option<String>, Error> {
    let mut library = None;
    let mut rest = options;
    while let Some(((key, _), after)) = rest.split_first() {
        let Value::Symbol(key) = key else {
            return Err(Error::TypeError("defffi: expected an option keyword".into()));
        };
        if !key.is(wk::KW_LIBRARY) {
            return Err(Error::TypeError(format!("defffi: `{}` is not an option here (only `:library` is)", heap.symbol_spelling(*key))));
        }
        let Some(((Value::Str(id), _), after)) = after.split_first() else {
            return Err(Error::TypeError("defffi: `:library` needs a name, as a string".into()));
        };
        library = Some(heap.string(*id).to_string());
        rest = after;
    }
    Ok(library)
}
