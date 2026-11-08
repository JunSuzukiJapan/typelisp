//! Reading a written type.

use crate::types::{scalar_of_symbol, Type};
use crate::{Error, Heap, Value};

/// The type `v` writes.
///
/// So far only the built-in scalar types and `()` are read; a type
/// variable, a named type, a function type and the C words are read when the
/// checker can check what they are used for.
pub(crate) fn parse_type(heap: &Heap, v: Value) -> Result<Type, Error> {
    match v {
        Value::Empty => Ok(Type::Unit),
        Value::Symbol(id) => match scalar_of_symbol(id) {
            Some(Type::Ptr | Type::CLong | Type::CULong) => {
                unimplemented!("typeck: the C word type `{}`", heap.symbol_spelling(id))
            }
            Some(t) => Ok(t),
            None => unimplemented!("typeck: the type `{}` (only built-in scalar types are read so far)", heap.symbol_spelling(id)),
        },
        _ => unimplemented!("typeck: a compound type (only built-in scalar types are read so far)"),
    }
}
