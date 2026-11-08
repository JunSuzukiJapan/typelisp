//! Reading a written type.

use crate::types::{scalar_of_symbol, Type};
use crate::{Error, Heap, Value};

use super::scope::TypeScope;

/// The type `v` writes, where `scope`'s type parameters are declared.
///
/// A bare name is first a declared type parameter, then a built-in scalar
/// type. Writing a name never declares it (`docs/dev/checker-rebuild-plan.md`
/// 7.5). So far only these and `()` are read; a named type, a function type
/// and the C words are read when the checker can check what they are used
/// for.
pub(crate) fn parse_type(heap: &Heap, scope: &TypeScope, v: Value) -> Result<Type, Error> {
    match v {
        Value::Empty => Ok(Type::Unit),
        Value::Symbol(id) => {
            if let Some(p) = scope.lookup(id) {
                return Ok(Type::Param(p));
            }
            match scalar_of_symbol(id) {
                Some(Type::Ptr | Type::CLong | Type::CULong) => {
                    unimplemented!("typeck: the C word type `{}`", heap.symbol_spelling(id))
                }
                Some(t) => Ok(t),
                None => unimplemented!(
                    "typeck: the type `{}` (only type parameters and built-in scalar types are read so far)",
                    heap.symbol_spelling(id)
                ),
            }
        }
        _ => unimplemented!("typeck: a compound type (only type parameters and built-in scalar types are read so far)"),
    }
}
