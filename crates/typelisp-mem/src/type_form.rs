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
use crate::symbols::{intern_exact, wk, NsId};
use crate::{Heap, Value};

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
