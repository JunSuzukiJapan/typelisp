//! A type's *runtime identity*: the one string that says which type a heap
//! value is, and the only functions allowed to write, compare, or read it
//! back.
//!
//! A `BoxedObj::Struct`/`Enum` carries its type's name as a string
//! (`Heap::alloc_struct`/`alloc_enum`). That string is not decoration — it is
//! the value's identity, consulted by `match_pattern`'s enum/struct arms,
//! `rt_sexpr_instance_test` (compiled code's downcast), `equalp`, the
//! printer's variant-name lookup, and `print-object` dispatch. Everyone who
//! writes it and everyone who reads it must agree on the spelling, and the
//! spelling is **the whole path** ([`Path`]'s `Display`), never its last
//! segment: `m::vector` and the built-in `vector` are different types.
//!
//! That agreement was broken twice on 2026-08-04 — the interpreter wrote
//! `Path::to_string` while `core_bridge` wrote `Path::last_segment`, so a value
//! built by compiled code was unmatchable by interpreted code (and vice
//! versa), printed as `<unknown-variant>`, and compared unequal to its own
//! twin. Neither side was *wrong on its own*; they simply spelled the same
//! concept two ways, and both spellings are `String`. Hence this module: one
//! entry point per direction, and `tests/type_identity_guard_test.rs` keeps
//! the raw heap accessors from being called anywhere else.
//!
//! The `crates/typelisp-rt` shims (`rt_data_new`, `rt_sexpr_instance_test`)
//! are deliberately outside this rule: they receive the key as a string from
//! compiled code, having no `Path` to work from. Their end of the agreement is
//! held up by [`type_key_of`] being what `core_bridge` compiles into the
//! literal they are handed.

use crate::mem::BoxId;
use crate::{Heap, Path};

/// How `p` is spelled when stored in (or compared against) a heap value.
/// The one place a type identity string is produced.
pub fn type_key_of(p: &Path) -> String {
    p.to_string()
}

/// Allocate a boxed struct of the type `p` names — `Heap::alloc_struct` with
/// the identity spelled for you. The one place a struct's key is written.
pub fn alloc_typed_struct(heap: &mut Heap, p: &Path, fields: Vec<crate::Value>) -> crate::Value {
    heap.alloc_struct(type_key_of(p), fields)
}

/// Allocate a boxed enum value of the type `p` names — the enum counterpart of
/// [`alloc_typed_struct`]. The one place an enum's key is written.
pub fn alloc_typed_enum(
    heap: &mut Heap,
    p: &Path,
    variant: usize,
    fields: Vec<crate::Value>,
) -> crate::Value {
    heap.alloc_enum(type_key_of(p), variant, fields)
}

/// The type key stored in boxed value `id`, or `None` when the box is neither
/// a struct nor an enum (a closure, a cell, a string builder, ...).
fn stored_key(heap: &Heap, id: BoxId) -> Option<&str> {
    if heap.is_struct(id) {
        Some(heap.struct_type_name(id)) // type-identity-ok: this is the reader
    } else if heap.is_enum(id) {
        Some(heap.enum_type_name(id)) // type-identity-ok: this is the reader
    } else {
        None
    }
}

/// Whether boxed value `id` is an instance of the type `p` names. The one
/// place a type identity is compared.
///
/// Deliberately indifferent to struct-vs-enum: a path names one or the other,
/// never both, so the key comparison alone settles it. Callers that go on to
/// *decode fields* still test `is_struct`/`is_enum` for their own reasons.
pub fn heap_type_is(heap: &Heap, id: BoxId, p: &Path) -> bool {
    stored_key(heap, id) == Some(type_key_of(p).as_str())
}

/// The type boxed value `id` belongs to, or `None` when the box carries no
/// type name. The one place a stored key is parsed back into a [`Path`] —
/// lossless because a path segment can never contain `::`.
pub fn heap_type_path(heap: &Heap, id: BoxId) -> Option<Path> {
    stored_key(heap, id).map(|k| Path::from_segments(k.split("::").map(str::to_string).collect()))
}
