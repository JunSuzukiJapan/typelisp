//! The type keys of tuples whose elements are all `Option<Sexpr>` — what the
//! reader builds for a `#{..}` datum, and what a `Sexpr`'s `tuple` variant
//! is.
//!
//! A tuple type is the prelude's `%internal::tupleN<T0,...>` struct (see the
//! front end's `types::tuple_path`), so these keys are the keys the checker
//! gives those instantiations. They are spelled here because the reader and
//! the runtime build and test such boxes with no checker in hand;
//! `tests/type_identity_guard_test.rs` holds the two spellings together.

/// The most elements a tuple can have; the prelude defines one tuple type per
/// arity up to this.
pub const TUPLE_MAX_ARITY: usize = 12;

const PREFIX: &str = "%internal::tuple";
const ELEMENT: &str = "option<sexpr>";

/// The key of the `#{..}` datum of `arity` elements.
pub fn sexpr_tuple_key(arity: usize) -> String {
    debug_assert!((1..=TUPLE_MAX_ARITY).contains(&arity), "no tuple type of arity {}", arity);
    format!("{}{}<{}>", PREFIX, arity, vec![ELEMENT; arity].join(","))
}

/// Whether `base` — a type key with its `<..>` arguments taken off
/// ([`crate::base_type_key`]) — names a tuple type, of any element types.
pub fn is_tuple_base_key(base: &str) -> bool {
    base.strip_prefix(PREFIX)
        .and_then(|n| n.parse::<usize>().ok())
        .is_some_and(|n| (1..=TUPLE_MAX_ARITY).contains(&n))
}

/// The arity of the `#{..}` datum whose key is `key`, or `None` when `key`
/// is not one — including a tuple whose elements are of other types.
pub fn sexpr_tuple_arity(key: &str) -> Option<usize> {
    let rest = key.strip_prefix(PREFIX)?;
    let digits = rest.find('<')?;
    let arity: usize = rest[..digits].parse().ok()?;
    if !(1..=TUPLE_MAX_ARITY).contains(&arity) {
        return None;
    }
    (key == sexpr_tuple_key(arity)).then_some(arity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_reads_back_as_its_arity() {
        for n in 1..=TUPLE_MAX_ARITY {
            assert_eq!(sexpr_tuple_arity(&sexpr_tuple_key(n)), Some(n));
        }
    }

    #[test]
    fn other_keys_are_not_data_tuples() {
        assert_eq!(sexpr_tuple_arity("%internal::tuple2<int,option<sexpr>>"), None);
        assert_eq!(sexpr_tuple_arity("%internal::tuple13<option<sexpr>>"), None);
        assert_eq!(sexpr_tuple_arity("vector<option<sexpr>>"), None);
    }

    #[test]
    fn every_arity_is_a_tuple_base() {
        assert!(is_tuple_base_key("%internal::tuple1"));
        assert!(is_tuple_base_key("%internal::tuple12"));
        assert!(!is_tuple_base_key("%internal::tuple13"));
        assert!(!is_tuple_base_key("%internal::tuplex"));
        assert!(!is_tuple_base_key("vector"));
    }
}
