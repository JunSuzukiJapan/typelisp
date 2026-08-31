//! A type's *runtime identity*: the token that says which type a heap value
//! is, and the only functions allowed to mint, write, compare, or read it
//! back.
//!
//! A `BoxedObj::Struct`/`Enum` carries its type's identity as an interned
//! [`TypeKeyId`] (`Heap::alloc_struct`/`alloc_enum`). That identity is not
//! decoration — it is consulted by `match_pattern`'s enum/struct arms,
//! `rt_sexpr_instance_test` (compiled code's downcast), `equalp`, the
//! printer's variant-name lookup, and `print-object` dispatch. Everyone who
//! writes it and everyone who reads it must agree, and what they agree on is
//! the *name the id was interned under*: **the whole path** ([`Path`]'s
//! `Display`), never its last segment — `m::vector` and the built-in `vector`
//! are different types.
//!
//! That agreement was broken twice on 2026-08-04 — the interpreter wrote
//! `Path::to_string` while `core_bridge` wrote `Path::last_segment`, so a value
//! built by compiled code was unmatchable by interpreted code (and vice
//! versa), printed as `<unknown-variant>`, and compared unequal to its own
//! twin. Neither side was *wrong on its own*; they simply spelled the same
//! concept two ways. Hence this module: one entry point per direction, and
//! `tests/type_identity_guard_test.rs` keeps the raw heap accessors from being
//! called anywhere else.
//!
//! The identity used to *be* a `String`, copied onto every instance and
//! compared character by character. Interning moved the spelling into the
//! heap's type-key table and left an integer on the value, so the comparisons
//! below are `==` on a [`TypeKeyId`]; a name is built only where one has to
//! cross into the table ([`type_key_of`]), and read back only for printing.
//!
//! The `crates/typelisp-rt` shims (`rt_data_new`, `rt_sexpr_instance_test`)
//! are deliberately outside this rule: they receive the key as a string from
//! compiled code, having no `Path` to work from, and intern it themselves.
//! Their end of the agreement is held up by [`type_key_of`] being what
//! `core_bridge` compiles into the literal they are handed.

use std::borrow::Cow;

use crate::mem::BoxId;
use crate::{Heap, Path, Type, TypeKeyId};

/// How `p` is spelled when interned as (or looked up among) type identities.
/// The one place a type identity *name* is produced.
///
/// Borrowed for a single-segment path — the common case, and the one where
/// joining would allocate a copy of a string that already exists.
pub fn type_key_of(p: &Path) -> Cow<'_, str> {
    match p.segments() {
        [only] => Cow::Borrowed(only.as_str()),
        segs => Cow::Owned(segs.join("::")),
    }
}

/// The interned identity of the type `p` names, minting it if this heap has
/// not seen the type before. The one place a type identity is created.
pub fn type_key_id(heap: &mut Heap, p: &Path) -> TypeKeyId {
    heap.intern_type_key(&type_key_of(p))
}

/// The interned identity of the type `p` names, or `None` if no value in this
/// heap has ever been of that type — which answers "is `id` one of those?"
/// on its own, without minting anything.
fn existing_type_key_id(heap: &Heap, p: &Path) -> Option<TypeKeyId> {
    heap.type_key_id(&type_key_of(p))
}

/// Allocate a boxed struct of the type `p` names — `Heap::alloc_struct` with
/// the identity resolved for you. The one place a struct's key is written.
pub fn alloc_typed_struct(heap: &mut Heap, p: &Path, fields: Vec<crate::Value>) -> crate::Value {
    let key = type_key_id(heap, p);
    heap.alloc_struct(key, fields)
}

/// Allocate a boxed enum value of the type `p` names — the enum counterpart of
/// [`alloc_typed_struct`]. The one place an enum's key is written.
pub fn alloc_typed_enum(
    heap: &mut Heap,
    p: &Path,
    variant: usize,
    fields: Vec<crate::Value>,
) -> crate::Value {
    let key = type_key_id(heap, p);
    heap.alloc_enum(key, variant, fields)
}

/// The key one level inside `key` — see [`typelisp_mem::inner_type_key`],
/// which lives a crate down because the runtime shims spell keys too
/// (`typelisp_rt`'s `rt_hashtable_entries` builds the cells of a
/// `Vector<cons-cell<K,V>>` and is handed only the vector's key).
pub use typelisp_mem::inner_type_key as inner_key;

/// Allocate a boxed struct whose runtime identity is `key` — the spelling
/// [`type_key_of_type`] produced at the construction site, carried through the
/// core IR's `construct` node.
///
/// The [`Path`]-taking [`alloc_typed_struct`] answers the same question for a
/// caller that has no instantiation to speak of (a non-generic type, where the
/// two spellings are identical anyway).
pub fn alloc_struct_keyed(heap: &mut Heap, key: &str, fields: Vec<crate::Value>) -> crate::Value {
    let key = heap.intern_type_key(key);
    heap.alloc_struct(key, fields)
}

/// A fresh `HashTable<K,V>` under the identity `key` spells — the map-payload
/// counterpart of [`alloc_struct_keyed`], here for the same reason: writing a
/// type identity is this module's job.
pub fn alloc_hashtable_keyed(heap: &mut Heap, key: &str) -> crate::Value {
    let key = heap.intern_type_key(key);
    heap.alloc_hashtable(key)
}

/// [`alloc_struct_keyed`]'s enum counterpart.
pub fn alloc_enum_keyed(heap: &mut Heap, key: &str, variant: usize, fields: Vec<crate::Value>) -> crate::Value {
    let key = heap.intern_type_key(key);
    heap.alloc_enum(key, variant, fields)
}

/// The type key stored in boxed value `id`, or `None` when the box is neither
/// a struct nor an enum (a closure, a cell, a string builder, ...).
fn stored_key(heap: &Heap, id: BoxId) -> Option<TypeKeyId> {
    if heap.is_struct(id) {
        Some(heap.struct_type_key(id)) // type-identity-ok: this is the reader
    } else if heap.is_enum(id) {
        Some(heap.enum_type_key(id)) // type-identity-ok: this is the reader
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
    match stored_key(heap, id) {
        Some(k) => existing_type_key_id(heap, p) == Some(k),
        None => false,
    }
}

/// Whether boxed value `id` is an instance of the type spelled `key` — the
/// [`type_key_of_type`] spelling a pattern node carries.
///
/// The instantiation-aware counterpart of [`heap_type_is`]: `gen<i32>` and
/// `gen<string>` answer differently here and identically there.
pub fn heap_type_is_key(heap: &Heap, id: BoxId, key: &str) -> bool {
    match stored_key(heap, id) {
        Some(k) => heap.type_key_id(key) == Some(k),
        None => false,
    }
}

/// A key split into the type's path and its type arguments, as written:
/// `gen<i32>` -> `("gen", Some("i32"))`, `point` -> `("point", None)`.
///
/// What a *lookup* needs. A generic type's methods are registered on the base
/// type under a mangled name (`Checker::mangled_method_name`: `print-object
/// <i32>`), so a dispatcher holding a value's key needs both halves — the
/// path to find the type, the arguments to name the specialization.
pub fn split_key(key: &str) -> (&str, Option<&str>) {
    match key.find('<') {
        Some(i) if key.ends_with('>') => (&key[..i], Some(&key[i + 1..key.len() - 1])),
        _ => (key, None),
    }
}

/// The method name a *specialization* of `method` is registered under for a
/// value whose key is `key` — `("gen<i32>", "print-object")` ->
/// `"print-object <i32>"`, and plain `"print-object"` for a non-generic type.
///
/// The reader's half of `Checker::mangled_method_name`, which is what makes a
/// generic type's `print-object` findable at all: the registration names the
/// instantiation, and until values carried theirs, nothing holding one could
/// spell it.
pub fn specialized_method_name(key: &str, method: &str) -> String {
    match split_key(key).1 {
        Some(args) => format!("{} <{}>", method, args),
        None => method.to_string(),
    }
}

/// The raw key stored in boxed value `id` — the spelling, instantiation and
/// all. [`heap_type_path`] is the same thing reduced to a [`Path`], for
/// callers that want the *type* rather than the instantiation.
pub fn heap_type_key(heap: &Heap, id: BoxId) -> Option<&str> {
    stored_key(heap, id).map(|k| heap.type_key_name(k))
    // type-identity-ok: reading the spelling back out is this function's job
}

/// The type boxed value `id` belongs to, or `None` when the box carries no
/// type identity. The one place a stored key is parsed back into a [`Path`] —
/// lossless because a path segment can never contain `::`.
pub fn heap_type_path(heap: &Heap, id: BoxId) -> Option<Path> {
    heap_type_key(heap, id).map(|k| {
        // The *base*: a key carries its instantiation now (`gen<i32>`), and a
        // `Path` has no room for one. Callers that need the arguments ask
        // [`heap_type_key`]/[`split_key`] instead — the printer's dispatch is
        // the one that does.
        Path::from_segments(split_key(k).0.split("::").map(str::to_string).collect())
    })
}

/// How the type `t` is spelled as a runtime identity: the whole path, plus
/// its type arguments when it has any (`gen<i32>`, `hashtable<string,i32>`).
///
/// The one place a type identity *name* is produced from a [`Type`], and the
/// same spelling three separate mechanisms need:
///
/// * a heap value's type key — which instantiation this value is,
/// * a specialization's registered name (`Checker::mangled_method_name`),
/// * a `:dyn` box's concrete key (`Checker::check_as_dyn`).
///
/// They are one question asked in three places. `:dyn` was the only one that
/// answered it with the type arguments included, which is exactly why a
/// generic type's `print-object` could never be found: the value said `gen`
/// and the registration said `print-object <i32>`.
///
/// For a type with no arguments this is [`type_key_of`] of its path, character
/// for character — so the keys of every non-generic type are unchanged.
pub fn type_key_of_type(t: &Type) -> String {
    match t {
        Type::I8 => "i8".into(),
        Type::I16 => "i16".into(),
        Type::I32 => "i32".into(),
        Type::I64 => "i64".into(),
        Type::Isize => "isize".into(),
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::Usize => "usize".into(),
        Type::F32 => "f32".into(),
        Type::F64 => "f64".into(),
        Type::Bignum => "bignum".into(),
        Type::Ratio => "ratio".into(),
        Type::RandomState => "random-state".into(),
        Type::Bool => "bool".into(),
        Type::Char => "char".into(),
        Type::Str => "string".into(),
        Type::Symbol => "symbol".into(),
        Type::Unit => "()".into(),
        Type::Never => "!".into(),
        Type::Named(p, args) if args.is_empty() => p.to_string(),
        Type::Named(p, args) => {
            format!("{}<{}>", p, args.iter().map(type_key_of_type).collect::<Vec<_>>().join(","))
        }
        // The space is load-bearing, the same way `mangled_method_name`'s is:
        // the reader treats it as a token boundary outside `<>`, so no
        // user-written name can ever collide with a mangled one.
        Type::Dyn(p, pins) if pins.is_empty() => format!("dyn {}", p),
        Type::Dyn(p, pins) => {
            format!("dyn {}<{}>", p, pins.iter().map(type_key_of_type).collect::<Vec<_>>().join(","))
        }
        Type::Fn(ps, rest, r) => {
            let mut inner: Vec<String> = ps.iter().map(type_key_of_type).collect();
            if let Some(t) = rest {
                inner.push(format!("&rest {}", type_key_of_type(t)));
            }
            format!("(fn ({}) {})", inner.join(","), type_key_of_type(r))
        }
    }
}
