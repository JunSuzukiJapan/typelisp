//! The tuple types — the prelude's `std::%internal::tupleN<T0,...>` structs
//! (see the front end's `types::tuple_path`) — and the ones whose elements are
//! all `Option<Sexpr>`: what the reader builds for a `#{..}` datum, and what a
//! `Sexpr`'s `tuple` variant is. They are built here because the reader and
//! the runtime build and test such boxes with no checker in hand;
//! `tests/type_identity_guard_test.rs` holds them equal to the checker's.

/// The most elements a tuple can have; the prelude defines one tuple type per
/// arity up to this.
pub const TUPLE_MAX_ARITY: usize = 12;

use crate::symbols::{child_ns, intern_exact, NsId};
use crate::{TypeId, TypeShape};

/// The definition of the tuple type of `arity` elements: its module and its
/// symbol there.
fn tuple_definition(arity: usize) -> (NsId, crate::SymRef) {
    let internal = child_ns(NsId::STD, "%internal");
    (internal, intern_exact(internal, &format!("tuple{}", arity)))
}

/// The type of the `#{..}` datum of `arity` elements.
pub fn sexpr_tuple_type(arity: usize) -> TypeId {
    debug_assert!((1..=TUPLE_MAX_ARITY).contains(&arity), "no tuple type of arity {}", arity);
    let (ns, sym) = tuple_definition(arity);
    crate::intern_type(TypeShape::Named(ns, sym, vec![TypeId::OPTION_SEXPR; arity]))
}

/// The arity of the tuple type `ty`, of any element types, or `None` when it
/// is not a tuple type.
pub fn tuple_type_arity(ty: TypeId) -> Option<usize> {
    let TypeShape::Named(ns, sym, args) = crate::type_shape(ty) else { return None };
    let arity = args.len();
    ((1..=TUPLE_MAX_ARITY).contains(&arity) && (ns, sym) == tuple_definition(arity)).then_some(arity)
}

/// The arity of the `#{..}` datum type `ty`, or `None` when it is not one —
/// including a tuple whose elements are of other types.
pub fn sexpr_tuple_arity(ty: TypeId) -> Option<usize> {
    tuple_type_arity(ty).filter(|n| ty == sexpr_tuple_type(*n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_datum_tuple_type_reads_back_as_its_arity() {
        for n in 1..=TUPLE_MAX_ARITY {
            assert_eq!(sexpr_tuple_arity(sexpr_tuple_type(n)), Some(n));
            assert_eq!(tuple_type_arity(sexpr_tuple_type(n)), Some(n));
        }
    }

    #[test]
    fn other_types_are_not_data_tuples() {
        let (ns, sym) = tuple_definition(2);
        let mixed = crate::intern_type(TypeShape::Named(ns, sym, vec![TypeId::SEXPR, TypeId::OPTION_SEXPR]));
        assert_eq!(tuple_type_arity(mixed), Some(2));
        assert_eq!(sexpr_tuple_arity(mixed), None);
        let (ns, sym) = tuple_definition(1);
        let wrong_arity = crate::intern_type(TypeShape::Named(ns, sym, vec![TypeId::OPTION_SEXPR; 2]));
        assert_eq!(tuple_type_arity(wrong_arity), None);
        assert_eq!(sexpr_tuple_arity(TypeId::SEXPR_VECTOR), None);
    }
}
