//! The `Option<T>` niche: which instantiations have no box, decided from a
//! type's id.
//!
//! An `Option<T>` is represented as `T`'s own tagged word for `some v` and
//! the empty-list immediate ([`crate::NIL_WORD`], `Value::Empty`) for
//! `none` whenever no `T` value can *be* that immediate — which is every
//! type except the ones listed in [`option_payload_niches`]. The checker
//! decides the same thing from a `Type` (`Repr::of_by` in `typelisp-front`,
//! kept in agreement by a test there); this is the decision for the code
//! that only has a [`TypeId`] in hand — the runtime producers of `Option`
//! values (`Heap::alloc_option`) and the printer, which sees a struct field's
//! type only as the id its template instantiates to.
//!
//! Lives in this crate, below both tiers, for the same reason
//! [`crate::ConcreteNames`] does: the interpreter and an AOT executable's
//! printer must answer identically, and the AOT tier cannot see
//! `typelisp-front`.

use crate::{Heap, TypeId, TypeShape, Value};

/// The payload `P` of `ty` when `ty` is an `Option<P>`.
fn option_payload(ty: TypeId) -> Option<TypeId> {
    let TypeShape::Named(ns, sym, args) = crate::type_shape(ty) else { return None };
    let TypeShape::Named(option_ns, option_sym, _) = crate::type_shape(TypeId::OPTION) else {
        unreachable!("`option` is a named type")
    };
    match args.as_slice() {
        [payload] if (ns, sym) == (option_ns, option_sym) => Some(*payload),
        _ => None,
    }
}

/// Whether `Option<P>` is niche-represented, given `P`.
///
/// The payload must never be the empty-list word. A nested `Option` may be
/// (its own `none`), `()` *is* it, and a raw C word (`ptr`/`c-long`/
/// `c-ulong`) has no tag to keep it apart; `!` has no values and no
/// representation at all. Everything else — fixnums, boxes, strings,
/// symbols, characters, booleans, cons cells, and `sexpr` itself, whose
/// empty list left it for `Option<sexpr>` — is a word the immediate is not.
pub fn option_payload_niches(payload: TypeId) -> bool {
    match crate::type_shape(payload) {
        TypeShape::Unit | TypeShape::Never => false,
        shape @ TypeShape::Named(..) => {
            let raw_word = ["ptr", "c-long", "c-ulong"].iter().any(|n| shape == TypeShape::named(crate::StdModule::PRIMITIVE.ns(), n, &[]));
            !(raw_word || option_payload(payload).is_some())
        }
        TypeShape::Fn(..) | TypeShape::Ptr(_) | TypeShape::Dyn(..) | TypeShape::CStruct(..) => true,
        TypeShape::Param(n) => unreachable!("the template parameter ${} is no value's type", n),
    }
}

/// Whether `ty` is a niche-represented `Option`: an `Option<P>` whose `P`
/// passes [`option_payload_niches`].
pub fn is_niched_option(ty: TypeId) -> bool {
    option_payload(ty).is_some_and(option_payload_niches)
}

/// Whether a *printer* should write a value of the type `ty` as
/// `(some ...)`/`none` around the word it holds: a niche-represented
/// `Option` ([`is_niched_option`]) other than `Option<sexpr>`.
/// `Option<sexpr>` is S-expression data — the S-expression *is* the value
/// and the empty list is its `none` — and prints as the datum it is, `()`
/// included, wherever it sits (`docs/ja/reference/functions/printing.md` §1). Every other niche has no
/// such reading, so the wrapper the word cannot carry is written back.
pub fn option_prints_wrapped(ty: TypeId) -> bool {
    is_niched_option(ty) && ty != TypeId::OPTION_SEXPR
}

impl Heap {
    /// An `Option` value of the type `ty` (`Option<P>`): the niche when `P`
    /// allows one — `v` itself for `some`, [`Value::Empty`] for `none` — and a
    /// `BoxedObj::Enum` box (`some` = variant 0, `none` = variant 1,
    /// `option_def`'s order) otherwise. **The one producer** for Rust code
    /// that builds an `Option` from a type; the checker's `some-of`/`none`
    /// forms are its typed twin.
    pub fn alloc_option(&mut self, ty: TypeId, v: Option<Value>) -> Value {
        let Some(payload) = option_payload(ty) else {
            panic!("alloc_option: `{}` is not an `Option<P>` type", &*crate::type_name(ty))
        };
        if option_payload_niches(payload) {
            return v.unwrap_or(Value::Empty);
        }
        match v {
            Some(x) => self.alloc_enum(ty, 0, vec![x]),
            None => self.alloc_enum(ty, 1, vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern_type;

    fn named(name: &str, args: &[TypeId]) -> TypeId {
        let ns = match name {
            "Option" => crate::StdModule::OPTION.ns(),
            "Vector" => crate::StdModule::VEC.ns(),
            _ => crate::StdModule::PRIMITIVE.ns(),
        };
        intern_type(TypeShape::named(ns, name, args))
    }

    #[test]
    fn a_payload_niches_unless_it_can_be_the_empty_word() {
        let int = named("int", &[]);
        let niching = [
            int,
            named("f64", &[]),
            named("string", &[]),
            TypeId::SEXPR,
            named("Vector", &[int]),
            intern_type(TypeShape::Fn(vec![int], None, int)),
            intern_type(TypeShape::Ptr(int)),
        ];
        for p in niching {
            assert!(option_payload_niches(p), "{}", &*crate::type_name(p));
            assert!(is_niched_option(named("Option", &[p])), "Option<{}>", &*crate::type_name(p));
        }
        let not_niching = [named("Option", &[int]), TypeId::OPTION_SEXPR, intern_type(TypeShape::Unit), intern_type(TypeShape::Never), named("ptr", &[]), named("c-long", &[]), named("c-ulong", &[])];
        for p in not_niching {
            assert!(!option_payload_niches(p), "{}", &*crate::type_name(p));
            assert!(!is_niched_option(named("Option", &[p])), "Option<{}>", &*crate::type_name(p));
        }
        assert!(!is_niched_option(int));
        assert!(!is_niched_option(named("Vector", &[named("Option", &[int])])));
        assert!(option_prints_wrapped(named("Option", &[int])));
        assert!(!option_prints_wrapped(TypeId::OPTION_SEXPR));
        assert!(!option_prints_wrapped(named("Option", &[named("Option", &[int])])));
    }

    #[test]
    fn alloc_option_niches_or_boxes_by_the_type() {
        let mut h = Heap::with_capacity(64);
        let option_int = named("Option", &[named("int", &[])]);
        let option_option_int = named("Option", &[option_int]);
        let option_unit = named("Option", &[intern_type(TypeShape::Unit)]);
        assert_eq!(h.alloc_option(option_int, Some(Value::Int(5))), Value::Int(5));
        assert_eq!(h.alloc_option(option_int, None), Value::Empty);
        assert_eq!(h.box_count(), 0);
        let Value::Boxed(some) = h.alloc_option(option_option_int, Some(Value::Int(5))) else { panic!("boxed") };
        assert!(h.is_enum(some));
        assert_eq!(h.enum_variant(some), 0);
        let Value::Boxed(none) = h.alloc_option(option_unit, None) else { panic!("boxed") };
        assert_eq!(h.enum_variant(none), 1);
    }
}
