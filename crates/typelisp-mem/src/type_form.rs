//! How the core IR writes a type, read back as the type's runtime-identity
//! spelling.
//!
//! A type form is one of
//!
//! ```text
//! (type-named PATH ARG...)       a struct, an enum, a built-in type
//! (type-fn (PARAM...) REST RET)  REST is () or the &rest parameter's type
//! (type-dyn PATH PIN...)         a trait object
//! (type-ptr T)
//! (type-cstruct PATH)
//! (type-param N)                 the owning type's Nth parameter, in a field's template
//! (type-var "NAME")              a type parameter, in a generic definition's body
//! (type-unit)
//! (type-never)
//! ```
//!
//! where `PATH` is the definition's path from the root, as the reader reads
//! `::a::b::name`. The interpreter and the compiled code's runtime both read a
//! type off the IR through [`type_key_of_form`], so they cannot spell the same
//! type two ways.

use crate::names::ConcreteNames;
use crate::symbols::{child_ns_named, intern_exact, wk, NsId, SymRef};
use crate::{Heap, TypeId, TypeShape, Value};

/// The interned identity of the type `form` writes, minting it if it has not
/// been seen before — or `None` when `form` is not a type form (a `type-var`
/// names no type by itself). A `type-param` reads as [`TypeShape::Param`],
/// which a field's template holds.
pub fn type_of_form(heap: &Heap, form: Value) -> Option<TypeId> {
    match read_type(heap, form, true) {
        Read::Found(id) => Some(id),
        Read::Unseen => unreachable!("a minting read always finds the type"),
        Read::NotAType => None,
    }
}

/// [`type_of_form`] without the minting: `Ok(None)` when no value has ever
/// had the type, which answers "is this value one of those?"
/// on its own. `Err(())` when `form` is not a type form.
pub fn existing_type_of_form(heap: &Heap, form: Value) -> Result<Option<TypeId>, ()> {
    match read_type(heap, form, false) {
        Read::Found(id) => Ok(Some(id)),
        Read::Unseen => Ok(None),
        Read::NotAType => Err(()),
    }
}

enum Read {
    Found(TypeId),
    Unseen,
    NotAType,
}

fn read_type(heap: &Heap, form: Value, mint: bool) -> Read {
    let Some(shape) = read_shape(heap, form, mint) else { return Read::NotAType };
    let Some(shape) = shape else { return Read::Unseen };
    if mint {
        Read::Found(crate::intern_type(shape))
    } else {
        crate::find_type(&shape).map_or(Read::Unseen, Read::Found)
    }
}

/// The shape `form` writes: `None` when it is not a type form, `Some(None)`
/// when one of its arguments was never interned and `mint` is off.
fn read_shape(heap: &Heap, form: Value, mint: bool) -> Option<Option<TypeShape>> {
    let Value::Cons(_) = form else { return None };
    let Value::Symbol(tag) = heap.car(form).ok()? else { return None };
    let fields = heap.list_to_vec(heap.cdr(form).ok()?).ok()?;
    let arg = |f: Value| match read_type(heap, f, mint) {
        Read::Found(id) => Some(Some(id)),
        Read::Unseen => Some(None),
        Read::NotAType => None,
    };
    let args = |forms: &[Value]| -> Option<Option<Vec<TypeId>>> { forms.iter().map(|f| arg(*f)).collect::<Option<Vec<_>>>().map(|v| v.into_iter().collect()) };
    let shape = match (tag.well_known(), fields.as_slice()) {
        (wk::TYPE_NAMED, [path, rest @ ..]) => {
            let (ns, sym) = definition(heap, *path)?;
            args(rest)?.map(|a| TypeShape::Named(ns, sym, a))
        }
        (wk::TYPE_DYN, [path, pins @ ..]) => {
            let (ns, sym) = definition(heap, *path)?;
            args(pins)?.map(|a| TypeShape::Dyn(ns, sym, a))
        }
        (wk::TYPE_FN, [params, rest, ret]) => {
            let params = args(&heap.list_to_vec(*params).ok()?)?;
            let rest = match rest {
                Value::Empty => Some(None),
                r => arg(*r)?.map(Some),
            };
            let ret = arg(*ret)?;
            match (params, rest, ret) {
                (Some(p), Some(r), Some(t)) => Some(TypeShape::Fn(p, r, t)),
                _ => None,
            }
        }
        (wk::TYPE_PTR, [pointee]) => arg(*pointee)?.map(TypeShape::Ptr),
        (wk::TYPE_CSTRUCT, [path]) => {
            let (ns, sym) = definition(heap, *path)?;
            Some(TypeShape::CStruct(ns, sym))
        }
        (wk::TYPE_UNIT, []) => Some(TypeShape::Unit),
        (wk::TYPE_NEVER, []) => Some(TypeShape::Never),
        (wk::TYPE_PARAM, [Value::Int(n)]) => Some(TypeShape::Param(u32::try_from(*n).ok()?)),
        _ => return None,
    };
    Some(shape)
}

/// The definition a type form's path names: the module, walked down the
/// tree from the root one segment at a time, and the name's symbol there.
fn definition(heap: &Heap, path: Value) -> Option<(NsId, SymRef)> {
    let Value::Path(id) = path else { return None };
    let segments = heap.path_segments(id);
    let (root, segs) = segments.split_first()?;
    let (name, modules) = segs.split_last()?;
    if *root != intern_exact(NsId::ROOT, "") {
        return None;
    }
    let mut ns = NsId::ROOT;
    for m in modules {
        ns = child_ns_named(ns, *m)?;
    }
    Some((ns, *name))
}

/// The runtime-identity spelling of the type `form` writes — the key
/// [`ConcreteNames`] spells for the same type — or `None` when `form` is not
/// a type form.
pub fn type_key_of_form(heap: &Heap, form: Value) -> Option<String> {
    let Value::Cons(_) = form else { return None };
    let Value::Symbol(tag) = heap.car(form).ok()? else { return None };
    let fields = heap.list_to_vec(heap.cdr(form).ok()?).ok()?;
    let keys = |forms: &[Value]| forms.iter().map(|f| type_key_of_form(heap, *f)).collect::<Option<Vec<String>>>();
    match (tag.well_known(), fields.as_slice()) {
        (wk::TYPE_NAMED, [path, args @ ..]) => Some(ConcreteNames::instantiated_type(&path_spelling(heap, *path)?, &keys(args)?)),
        (wk::TYPE_DYN, [path, pins @ ..]) => Some(ConcreteNames::dyn_type(&path_spelling(heap, *path)?, &keys(pins)?)),
        (wk::TYPE_FN, [params, rest, ret]) => {
            let params = keys(&heap.list_to_vec(*params).ok()?)?;
            let rest = match rest {
                Value::Empty => None,
                r => Some(type_key_of_form(heap, *r)?),
            };
            Some(ConcreteNames::fn_type(&params, rest.as_ref(), &type_key_of_form(heap, *ret)?))
        }
        (wk::TYPE_PTR, [pointee]) => Some(ConcreteNames::ptr_type(&type_key_of_form(heap, *pointee)?)),
        (wk::TYPE_CSTRUCT, [path]) => path_spelling(heap, *path),
        (wk::TYPE_PARAM, [Value::Int(n)]) => Some(ConcreteNames::template_param(usize::try_from(*n).ok()?)),
        (wk::TYPE_VAR, [Value::Str(name)]) => Some(heap.string(*name).to_string()),
        (wk::TYPE_UNIT, []) => Some("()".to_string()),
        (wk::TYPE_NEVER, []) => Some("!".to_string()),
        _ => None,
    }
}

/// A definition's path from the root, spelled as a type key spells it: the
/// segments after the root, joined by `::`.
fn path_spelling(heap: &Heap, path: Value) -> Option<String> {
    let Value::Path(id) = path else { return None };
    let segments = heap.path_segments(id);
    let (root, segs) = segments.split_first()?;
    if *root != intern_exact(NsId::ROOT, "") || segs.is_empty() {
        return None;
    }
    Some(segs.iter().map(|s| s.name()).collect::<Vec<_>>().join("::"))
}
