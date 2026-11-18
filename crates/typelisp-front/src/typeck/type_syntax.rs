//! Reading a written type.

use crate::typeck::registry::AdtKind;
use crate::check::semantic::{TypeKind, TypeUse};
use crate::types::{is_dyn_form, is_type_application, last_segment_span, scalar_of_symbol, scalar_type, tuple_path, written_path, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
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
    /// Where the type names read are written down, for an editor.
    pub(crate) notes: &'a super::expr::Notes,
}

/// The type `v` writes.
///
/// A bare name is first a declared type parameter, then `Self`, then a
/// built-in scalar type, then a type the module sees. Writing a name never declares it
/// (`docs/dev/checker-rebuild-plan.md` 7.5). `Name<A, B>` reads as
/// `(%type-apply Name A B)`. A `::` path names a type in a module.
/// `(fn (A B &rest C) R)` is a function's type, `#{A B}` a tuple's, and any
/// other list a list type ([`list_type`]). A C word (`ptr`, `c-long`, `c-ulong`) is the
/// whole type or nothing: every slot inside another type tags what it holds,
/// which would drop the word's top three bits. A C pointer `(ptr T)` is not
/// read so far.
///
/// `at` is where `v` is written, when it is: each type it names is noted
/// there for an editor.
pub(crate) fn parse_type(heap: &Heap, cx: &TypeCx, v: Value, at: Option<&Loc>) -> Result<Type, Error> {
    let ty = parse_raw(heap, cx, v, at)?;
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
pub(crate) fn parse_raw(heap: &Heap, cx: &TypeCx, v: Value, at: Option<&Loc>) -> Result<Type, Error> {
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
                None => named(heap, cx, id, Vec::new(), at),
            }
        }
        Value::Path(_) => {
            let path = written_path(heap, cx.ns, v)?;
            match scalar_type(&path) {
                Some(t) => Ok(t),
                None => at_path(cx, path, Vec::new(), at),
            }
        }
        Value::Cons(_) if !is_list_typed_form(heap, v) => list_type(heap, cx, v),
        Value::Cons(_) if is_dyn_form(heap, v) => dyn_type(heap, cx, v),
        // `(ptr T)`: a typed pointer.
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::PTR)) => {
            let [_, (pointee, pointee_at)] = &heap.list_to_vec_locs(v)?[..] else {
                return Err(Error::TypeError("a typed pointer is written (ptr T)".into()));
            };
            Ok(Type::PtrTo(Box::new(super::c_memory::pointee(heap, cx, *pointee, pointee_at.as_ref())?)))
        }
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::FN)) => fn_type(heap, cx, v),
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::TYPE_MEMBER)) => {
            Err(Error::TypeError("`Name<Args>::member` names a member of a type, not a type".to_string()))
        }
        // The last of [`LIST_WRITTEN_TYPE_HEADS`]: `(%type-apply Name A B)`,
        // what `Name<A, B>` reads as.
        Value::Cons(_) => {
            let parts = heap.list_to_vec_locs(v)?;
            let [_, (head, head_at), args @ ..] = &parts[..] else {
                return Err(Error::TypeError("a generic type is written Name<Arg,...>, with at least one argument".to_string()));
            };
            if args.is_empty() {
                return Err(Error::TypeError("a generic type is written Name<Arg,...>, with at least one argument".to_string()));
            }
            let args = args.iter().map(|(a, a_at)| parse_raw(heap, cx, *a, a_at.as_ref())).collect::<Result<Vec<_>, _>>()?;
            let head_at = head_at.as_ref();
            match *head {
                Value::Symbol(name) => named(heap, cx, name, args, head_at),
                Value::Path(_) => at_path(cx, written_path(heap, cx.ns, *head)?, args, head_at),
                other => Err(Error::TypeError(format!("a type application's head must be a plain type name, found `{:?}`", other))),
            }
        }
        // `#{A B}`, written the way its values are.
        Value::Boxed(id) if heap.sexpr_tuple_arity(id).is_some() => {
            let n = heap.struct_field_count(id);
            // The elements of a `#{..}` datum carry no place of their own.
            let args = (0..n).map(|i| parse_raw(heap, cx, heap.struct_field(id, i), None)).collect::<Result<Vec<_>, _>>()?;
            Ok(Type::Named(tuple_path(n), args))
        }
        other => Err(Error::TypeError(format!("not a type expression: {:?}", other))),
    }
}

/// `(T U)`: the list of a `T` and a `U`, `cons-cell<T,cons-cell<U,()>>`.
/// A dotted tail is the last cell's cdr, `(T . U)` being `cons-cell<T,U>`;
/// with none it is `()`, the empty list. A tail that is a type written as a
/// list — `(T . Option<U>)`, `(T . (fn (A) R))` — reads as that list's
/// elements, `(T %type-apply Option U)`: its head, which no element type
/// can be, is where the tail begins.
///
/// The cells are the standard library's `cons-cell`, which a module sees
/// wherever the library is loaded.
fn list_type(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    let cons = crate::types::cons_cell_path();
    if cx.reg.types.get(&cons).is_none() {
        return Err(Error::TypeError(format!("the list type `{}` is made of `cons-cell`s, and the standard library that defines them is not loaded", crate::check::core::print(heap, v))));
    }
    let mut elems = Vec::new();
    let mut cur = v;
    while let Value::Cons(cell) = cur {
        if is_list_typed_form(heap, cur) {
            break;
        }
        let elem = parse_raw(heap, cx, heap.car(cur)?, heap.elem_loc(cell).as_ref());
        // `(Vector int)` was once the generic type `Vector<int>`.
        let elem = match elem {
            Err(Error::TypeError(m)) if elems.is_empty() && matches!(heap.car(cur)?, Value::Symbol(_) | Value::Path(_)) && heap.cdr(cur)? != Value::Empty => {
                let head = crate::check::core::print(heap, heap.car(cur)?);
                Err(Error::TypeError(format!(
                    "{} — `{}` is a list type, each element a type of its own; a generic type takes its arguments in angle brackets, `{}<...>`",
                    m,
                    crate::check::core::print(heap, v),
                    head
                )))
            }
            other => other,
        };
        elems.push(elem?);
        cur = heap.cdr(cur)?;
    }
    // The reader keeps no place for a dotted tail.
    let mut ty = parse_raw(heap, cx, cur, None)?;
    for elem in elems.into_iter().rev() {
        ty = Type::Named(cons.clone(), vec![elem, ty]);
    }
    Ok(ty)
}

/// The heads of the types written as a list that are no list type:
/// `Name<A>`, `Name<A>::member`, `(fn ...)`, `(ptr T)` and `:dyn T`. Every
/// other list is a list type ([`list_type`]).
const LIST_WRITTEN_TYPE_HEADS: [u32; 5] = [wk::TYPE_APPLY, wk::TYPE_MEMBER, wk::FN, wk::PTR, wk::DYN];

/// Whether `v` is a type that is written as a list but is no list type.
fn is_list_typed_form(heap: &Heap, v: Value) -> bool {
    matches!(heap.car(v), Ok(Value::Symbol(id)) if LIST_WRITTEN_TYPE_HEADS.contains(&id.well_known()))
}

/// `(fn (A B &rest C) R)`: a function's type.
fn fn_type(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    let [_, (params, _), (ret, ret_at)] = elems.as_slice() else {
        return Err(Error::TypeError("fn type must be (fn (params) ret)".to_string()));
    };
    let ps = match params {
        Value::Empty => Vec::new(),
        Value::Cons(_) => heap.list_to_vec_locs(*params)?,
        _ => return Err(Error::TypeError("fn parameter list must be a list".to_string())),
    };
    let (fixed, rest) = match ps.iter().position(|(p, _)| matches!(p, Value::Symbol(id) if id.is(wk::REST))) {
        Some(i) if i + 2 == ps.len() => (&ps[..i], Some(Box::new(parse_raw(heap, cx, ps[i + 1].0, ps[i + 1].1.as_ref())?))),
        Some(_) => {
            return Err(Error::TypeError(
                "fn type: &rest must be followed by exactly one type, as the last item in the parameter list".to_string(),
            ))
        }
        None => (&ps[..], None),
    };
    let fixed = fixed.iter().map(|(p, at)| parse_raw(heap, cx, *p, at.as_ref())).collect::<Result<Vec<_>, _>>()?;
    Ok(Type::Fn(fixed, rest, Box::new(parse_raw(heap, cx, *ret, ret_at.as_ref())?)))
}

/// The type the path `path` names, applied to `args`. One its module does
/// not make `pub` is not seen from another.
fn at_path(cx: &TypeCx, path: Path, args: Vec<Type>, at: Option<&Loc>) -> Result<Type, Error> {
    let path = super::resolve::reexported(cx.reg, path);
    let visible = |public: bool| public || sees(cx.ns, path.ns());
    let entry = match cx.reg.types.get(&path) {
        Some(entry @ TypeEntry::Type(TypeDef { public, .. })) if visible(*public) => entry,
        Some(entry @ TypeEntry::Alias { public, .. }) if visible(*public) => entry,
        Some(entry @ TypeEntry::Trait(TraitDef { public, .. })) if visible(*public) => entry,
        Some(entry @ TypeEntry::CStruct(_)) => entry,
        _ => return Err(unknown_type(&path.to_string())),
    };
    note_use(cx, path, entry, at);
    entry_type(path, entry, args)
}

/// That the name written at `at`, which ends with `path`'s last segment,
/// names `path`, defined as `entry`: an editor shows a type the program
/// defines there, and leaves a built-in one to its own grammar.
pub(crate) fn note_use(cx: &TypeCx, path: Path, entry: &TypeEntry, at: Option<&Loc>) {
    note(cx.notes, path, entry, at);
}

/// [`note_use`], into `notes`.
pub(crate) fn note(notes: &super::expr::Notes, path: Path, entry: &TypeEntry, at: Option<&Loc>) {
    if let Some(loc) = at.and_then(|l| last_segment_span(&path, l)) {
        note_exactly(notes, path, entry, loc);
    }
}

/// That the header written at `at`, which starts with `path`'s last
/// segment (`name<T>` goes on with the type parameters), defines `path`.
pub(crate) fn note_definition(notes: &super::expr::Notes, path: Path, entry: &TypeEntry, at: Option<&Loc>) {
    let Some(at) = at else { return };
    let len = path.last_segment().chars().count() as u32;
    if at.end_line == at.line && at.col + len <= at.end_col {
        note_exactly(notes, path, entry, Loc::new(std::sync::Arc::clone(&at.file), at.line, at.col).with_end(at.line, at.col + len));
    }
}

/// That the path written `spelling` at `at`, `...::type::member`, names a
/// member of the type `path`, its second segment from the end.
pub(crate) fn note_path_head(notes: &super::expr::Notes, path: Path, entry: &TypeEntry, spelling: &str, at: Option<&Loc>) {
    let Some(at) = at else { return };
    let segs: Vec<&str> = spelling.split("::").collect();
    let width = |s: &str| s.chars().count() as u32;
    // Only a token that is the path and nothing else is measured.
    if segs.len() < 2 || at.end_line != at.line || width(spelling) != at.end_col.saturating_sub(at.col) {
        return;
    }
    let col = at.col + segs[..segs.len() - 2].iter().map(|s| width(s) + 2).sum::<u32>();
    let end = col + width(segs[segs.len() - 2]);
    note_exactly(notes, path, entry, Loc::new(std::sync::Arc::clone(&at.file), at.line, col).with_end(at.line, end));
}

/// That the name at exactly `loc` names `path`, defined as `entry`.
fn note_exactly(notes: &super::expr::Notes, path: Path, entry: &TypeEntry, loc: Loc) {
    let kind = match entry {
        TypeEntry::Type(def) if !def.builtin => match def.kind {
            AdtKind::Struct => TypeKind::Struct,
            AdtKind::Sum => TypeKind::Enum,
        },
        TypeEntry::Trait(_) => TypeKind::Trait,
        _ => return,
    };
    notes.type_uses.borrow_mut().push(TypeUse { path, kind, loc });
}

/// `(:dyn Trait)` or `(:dyn Trait<pins...>)`: a trait object of the trait,
/// its associated types pinned.
fn dyn_type(heap: &Heap, cx: &TypeCx, v: Value) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    let [_, (inner, inner_at)] = elems.as_slice() else {
        return Err(Error::TypeError("`:dyn` must be followed by exactly one trait name".to_string()));
    };
    let (head, head_at, pins) = match inner {
        Value::Cons(_) if is_type_application(heap, *inner) => {
            let parts = heap.list_to_vec_locs(*inner)?;
            (parts[1].0, parts[1].1.clone(), parts[2..].to_vec())
        }
        _ => (*inner, inner_at.clone(), Vec::new()),
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
        if found.is_none() {
            return Err(Error::TypeError(format!(
                "unknown trait `{}`: no trait of that name is visible here (a `:dyn` names a trait \
                 defined before it, in this module, or brought in by a `use`)",
                crate::check::core::print(heap, head)
            )));
        }
        return Err(Error::TypeError(format!("`:dyn` must be followed by a trait name, found `{}`", crate::check::core::print(heap, *inner))));
    };
    if !public && !sees(cx.ns, path.ns()) {
        return Err(unknown_type(&path.to_string()));
    }
    if let Some(entry) = cx.reg.types.get(&path) {
        note_use(cx, path, entry, head_at.as_ref());
    }
    let pins = pins.iter().map(|(p, p_at)| parse_raw(heap, cx, *p, p_at.as_ref())).collect::<Result<Vec<_>, _>>()?;
    Ok(Type::Dyn(path, pins))
}

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

/// The type the name `id` stands for, applied to `args`.
fn named(heap: &Heap, cx: &TypeCx, id: SymRef, args: Vec<Type>, at: Option<&Loc>) -> Result<Type, Error> {
    match find_named(cx.reg, cx.ns, id, |ns, key| cx.reg.types.get(&Path::of_symbol(ns, key))) {
        Some((path, entry)) => {
            note_use(cx, path, entry, at);
            entry_type(path, entry, args)
        }
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
        // A trait object is written `(:dyn Trait)`.
        TypeEntry::Trait(_) => Err(Error::TypeError(format!(
            "`{}` is a trait, not a type — write `:dyn {}` for a trait object",
            path.last_segment(),
            path.last_segment()
        ))),
        // A C struct is no value's type: only a typed pointer points at one.
        TypeEntry::CStruct(_) => Err(unknown_type(path.last_segment())),
    }
}
