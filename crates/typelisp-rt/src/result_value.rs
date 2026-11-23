//! The `Result`/`Option`/`Vector` values a builtin answers with, built under
//! the type its own table row gives them.
//!
//! A type's identity includes its instantiation, and the runtime builds these
//! boxes with no checker to ask: it sits below it. So each module of
//! builtins keeps a table — one row per builtin, the type of what it answers
//! with — and `tests/type_identity_guard_test.rs` compares every row against
//! that builtin's own registry return type. Checked rather than assumed,
//! because the failure it guards is silent: a value built under a type
//! nobody else gives it is unmatchable and fails to print.
//!
//! The types come from `typelisp_mem`'s pre-interned table rather than from
//! a `Path`, because the runtime has none to derive a name from.

use typelisp_mem::prelude::*;
use typelisp_mem::{Heap, TypeId, TypeSpec, Value};

/// A table of builtins' types: the builtin's well-known symbol, and the type.
pub type TypeTable = &'static [(u32, TypeSpec)];

/// The type `table` gives builtin `name`. A builtin that builds a box without
/// a row would give that value an identity nothing else spells, so a missing
/// row is not a recoverable condition.
pub fn type_in(table: TypeTable, name: u32) -> TypeId {
    match table.iter().find(|(n, _)| *n == name) {
        Some((_, ty)) => typelisp_mem::intern_spec(ty),
        None => crate::fatal(&format!("no type for builtin `{}` in its table", typelisp_mem::BUILTIN_SYMBOLS[name as usize])),
    }
}

/// `Ok(v)`, matching `result_def`'s variant order (`Ok` = 0, `Err` = 1).
pub fn ok_in(heap: &mut Heap, table: TypeTable, name: u32, v: Value) -> Value {
    let key = type_in(table, name);
    heap.alloc_enum(key, 0, vec![v])
}

/// `Err(<ErrType>(msg))`: the builtin's single-variant error type, wrapped in
/// `Result`'s `Err`. The error types take no type arguments, so their keys
/// are the pre-interned constants.
pub fn err_in(heap: &mut Heap, table: TypeTable, name: u32, err_type: TypeId, msg: String) -> Value {
    let msg_val = heap.alloc_string(msg);
    let err_val = heap.alloc_enum(err_type, 0, vec![msg_val]);
    let key = type_in(table, name);
    heap.alloc_enum(key, 1, vec![err_val])
}

/// `Some(v)`/`None` under the instantiation the row spells — the niche when
/// the payload allows one, else a box (`Heap::alloc_option` decides).
pub fn option_in(heap: &mut Heap, table: TypeTable, name: u32, v: Option<Value>) -> Value {
    heap.alloc_option(type_in(table, name), v)
}

/// A `Vector` of `elems` under the type the row spells. `Vector<T>`'s box is
/// a `Struct` whose fields are its elements.
///
/// The elements are allocated before the box by the caller; that is safe
/// because `alloc_string` and `alloc_struct` never collect (only
/// `Heap::cons` does). A caller that conses between them must root each.
pub fn vector_in(heap: &mut Heap, table: TypeTable, name: u32, elems: Vec<Value>) -> Value {
    let key = type_in(table, name);
    heap.alloc_struct(key, elems)
}
