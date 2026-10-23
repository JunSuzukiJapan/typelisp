//! The `Option<T>` niche: which instantiations have no box, decided from a
//! type key.
//!
//! An `Option<T>` is represented as `T`'s own tagged word for `some v` and
//! the empty-list immediate ([`crate::NIL_WORD`], `Value::Empty`) for
//! `none` whenever no `T` value can *be* that immediate — which is every
//! type except the ones listed in [`option_payload_niches`]. The checker
//! decides the same thing from a `Type` (`Repr::of_by` in `typelisp-front`,
//! kept in agreement by a test there); this is the decision for the code
//! that only has a *key* in hand — the runtime producers of `Option`
//! values (`Heap::alloc_option`) and the printer, which sees a struct field's
//! type only as the key its template instantiates to.
//!
//! Lives in this crate, below both tiers, for the same reason
//! [`crate::ConcreteNames`] does: the interpreter and an AOT executable's
//! printer must answer identically, and the AOT tier cannot see
//! `typelisp-front`.

use crate::{ConcreteNames, Heap, TypeId, Value};

/// Whether `Option<P>` is niche-represented, given `P`'s type key.
///
/// The payload must never be the empty-list word. A nested `Option` may be
/// (its own `none`), `()` *is* it, and a raw C word (`ptr`/`c-long`/
/// `c-ulong`) has no tag to keep it apart; `!` has no values and no
/// representation at all. Everything else — fixnums, boxes, strings,
/// symbols, characters, booleans, cons cells, and `sexpr` itself, whose
/// empty list left it for `Option<sexpr>` — is a word the immediate is not.
pub fn option_payload_niches(payload_key: &str) -> bool {
    !(ConcreteNames::base_type(payload_key) == "option" || matches!(payload_key, "()" | "!" | "ptr" | "c-long" | "c-ulong"))
}

/// Whether the type `key` names is a niche-represented `Option`: an
/// `option<P>` whose `P` passes [`option_payload_niches`].
pub fn is_niched_option_key(key: &str) -> bool {
    ConcreteNames::base_type(key) == "option" && ConcreteNames::single_type_arg(key).is_some_and(option_payload_niches)
}

/// Whether a *printer* should write a value of the type `key` names as
/// `(some ...)`/`none` around the word it holds: a niche-represented
/// `Option` ([`is_niched_option_key`]) other than `Option<sexpr>`.
/// `Option<sexpr>` is S-expression data — the S-expression *is* the value
/// and the empty list is its `none` — and prints as the datum it is, `()`
/// included, wherever it sits (`docs/ja/reference/functions/printing.md` §1). Every other niche has no
/// such reading, so the wrapper the word cannot carry is written back.
pub fn option_prints_wrapped(key: &str) -> bool {
    is_niched_option_key(key) && ConcreteNames::single_type_arg(key) != Some("sexpr")
}

impl Heap {
    /// An `Option` value of the type `ty` (`option<P>`): the niche when `P`
    /// allows one — `v` itself for `some`, [`Value::Empty`] for `none` — and a
    /// `BoxedObj::Enum` box (`some` = variant 0, `none` = variant 1,
    /// `option_def`'s order) otherwise. **The one producer** for Rust code
    /// that builds an `Option` from a type; the checker's `some-of`/`none`
    /// forms are its typed twin.
    pub fn alloc_option(&mut self, ty: TypeId, v: Option<Value>) -> Value {
        let key = self.type_key_name(ty).to_string();
        assert!(
            ConcreteNames::base_type(&key) == "option" && ConcreteNames::single_type_arg(&key).is_some(),
            "alloc_option: `{}` is not an `option<P>` type",
            key
        );
        if is_niched_option_key(&key) {
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

    #[test]
    fn a_payload_niches_unless_it_can_be_the_empty_word() {
        for k in ["int", "i32", "u8", "f64", "f32", "char", "bool", "string", "symbol", "ratio", "sexpr", "point", "m::point", "vector<int>", "hashtable<string,int>", "result<int,parseinterror>", "dyn shape", "(fn (int) int)", "(fn (int,string) int)", "llvm-value"] {
            assert!(option_payload_niches(k), "{}", k);
            assert!(is_niched_option_key(&format!("option<{}>", k)), "option<{}>", k);
        }
        for k in ["option<int>", "option<sexpr>", "option<option<int>>", "()", "!", "ptr", "c-long", "c-ulong"] {
            assert!(!option_payload_niches(k), "{}", k);
            assert!(!is_niched_option_key(&format!("option<{}>", k)), "option<{}>", k);
        }
        assert!(!is_niched_option_key("int"));
        assert!(!is_niched_option_key("vector<option<int>>"));
        assert!(option_prints_wrapped("option<int>"));
        assert!(!option_prints_wrapped("option<sexpr>"));
        assert!(!option_prints_wrapped("option<option<int>>"));
    }

    #[test]
    fn alloc_option_niches_or_boxes_by_the_type() {
        use crate::{NsId, TypeShape};
        let mut h = Heap::with_capacity(64);
        let int = h.intern_type(TypeShape::named(NsId::ROOT, "int", &[]));
        let unit = h.intern_type(TypeShape::Unit);
        let option_int = h.intern_type(TypeShape::named(NsId::ROOT, "option", &[int]));
        let option_option_int = h.intern_type(TypeShape::named(NsId::ROOT, "option", &[option_int]));
        let option_unit = h.intern_type(TypeShape::named(NsId::ROOT, "option", &[unit]));
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
