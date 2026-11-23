//! `HashTable<K,V>` primitives.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal};
use typelisp_mem::{BoxId, Value};
use crate::type_arg;

// ---- Iter-compile plan, Stage C: `HashTable<K,V>` primitives ------------
//
// A `HashTable<K,V>` is a `BoxedObj::Struct` with a `StructPayload::Map`
// payload (unlike a `Vector<T>`/`defstruct`'s `StructPayload::Fields`) —
// hash to bucket, with the mem layer neither hashing a key nor comparing
// two. Both are the key type's own `sxhash`/`equals`, so `get`/`set`/
// `remove` are prelude methods that ask them and then reach the bucket
// through the five shims below. These are thin adapters, the same shape as
// the `rt_struct_*` family above; every key and value crossing them is
// already tagged, exactly as a struct field is.

/// `(rt-hashtable-new type hash test)` for compiled code — an empty
/// `HashTable<K,V>` whose type is `type`, hashing its keys with the function
/// `hash` and comparing them with the function `test`.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread, `args[0]` must be a
/// type's word, and `args[1]`/`args[2]` tagged function values.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_new(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_hashtable_new: expected 3 arguments (the type, the hash function and the test function)");
    }
    // The table's own runtime identity, spelled by the checker and handed
    // down through the island (`typeck::method` -> `hashtable-op`'s name
    // slot): `HashTable<string,i32>` and `HashTable<string,string>` are
    // different types and nothing in an empty table says which this is.
    let key = type_arg(*args, "rt_hashtable_new");
    encode(active_heap().alloc_hashtable(key, decode(*args.offset(1)), decode(*args.offset(2))))
}

/// The hash function (`args[1]` 0) or the test function (1) the table
/// `args[0]` was made with, tagged.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread, `args[0]` must be a
/// tagged `HashTable` and `args[1]` a raw `0` or `1`.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_function(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_hashtable_function: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_function: first argument is not a boxed HashTable"),
    };
    encode(active_heap().hashtable_functions(id)[*args.offset(1) as usize])
}

/// The bucket family: `(rt-hashtable-bucket-* ht hash ...)` for compiled
/// code. `hash` and every index are raw `i64`s (an `i32` in the language);
/// keys and values are already-tagged `Sexpr`s, as a struct field is.
///
/// # Safety
///
/// `argc >= 2`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_count(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_hashtable_bucket_count: expected 2 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_count");
    active_heap().hashtable_bucket_count(id, *args.add(1)) as i64
}

/// The `i`th key in `hash`'s bucket, still tagged — the caller decodes it
/// per `K`'s own kind. See [`rt_hashtable_bucket_count`].
///
/// # Safety
///
/// `argc >= 3`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_key(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_hashtable_bucket_key: expected 3 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_key");
    encode(active_heap().hashtable_bucket_key(id, *args.add(1), bucket_index(*args.add(2), "rt_hashtable_bucket_key")))
}

/// The `i`th value in `hash`'s bucket. See [`rt_hashtable_bucket_key`].
///
/// # Safety
///
/// `argc >= 3`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_value(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_hashtable_bucket_value: expected 3 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_value");
    encode(active_heap().hashtable_bucket_value(id, *args.add(1), bucket_index(*args.add(2), "rt_hashtable_bucket_value")))
}

/// Writes `key -> val` at index `i` of `hash`'s bucket, appending when `i` is
/// the bucket's current length. Returns the compiled `Unit` (`0`). Stores
/// already-decoded `Value`s (no cons-heap allocation), so triggers no GC.
///
/// # Safety
///
/// `argc >= 5`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_put(args: *const i64, argc: u32) -> i64 {
    if argc < 5 {
        fatal("rt_hashtable_bucket_put: expected 5 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_put");
    let i = bucket_index(*args.add(2), "rt_hashtable_bucket_put");
    let key = decode(*args.add(3));
    let val = decode(*args.add(4));
    active_heap().hashtable_bucket_put(id, *args.add(1), i, key, val);
    0
}

/// Removes the `i`th entry of `hash`'s bucket. Returns the compiled `Unit`.
///
/// # Safety
///
/// `argc >= 3`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_delete(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_hashtable_bucket_delete: expected 3 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_delete");
    let i = bucket_index(*args.add(2), "rt_hashtable_bucket_delete");
    active_heap().hashtable_bucket_delete(id, *args.add(1), i);
    0
}

/// A bucket index. Negative is an internal-invariant break — every index a
/// caller passes came from `bucket-count` — so it traps rather than wrapping
/// into an enormous `usize`.
fn bucket_index(n: i64, who: &str) -> usize {
    match usize::try_from(n) {
        Ok(i) => i,
        Err(_) => fatal(&format!("{}: negative bucket index {}", who, n)),
    }
}

/// `(rt-hashtable-count ht)` for compiled code — the entry count as a raw
/// `i64` (`HashTable<K,V>::count`, like `rt_struct_field_count`'s raw result).
///
/// # Safety
///
/// `argc >= 1`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_count(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_hashtable_count: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_count: first argument is not a boxed HashTable"),
    };
    active_heap().hashtable_count(id) as i64
}

/// `(rt-hashtable-clear ht)` for compiled code — empties the map in place,
/// returning `Unit` (`0`). Allocates nothing, so triggers no GC.
///
/// # Safety
///
/// `argc >= 1`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_clear(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_hashtable_clear: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_clear: first argument is not a boxed HashTable"),
    };
    active_heap().hashtable_clear(id);
    0
}

/// The `BoxId` of `args[0]` decoded as a boxed struct — shared preamble of
/// every `HashTable` shim: the bucket family above and the three
/// `Vector`-building enumerators below.
///
/// # Safety
///
/// `argc >= 1`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
unsafe fn hashtable_arg(args: *const i64, argc: u32, who: &str) -> BoxId {
    if argc < 1 {
        fatal(&format!("{who}: expected 1 argument"));
    }
    match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal(&format!("{who}: first argument is not a boxed HashTable")),
    }
}

/// `(rt-hashtable-keys ht)` — a fresh `Vector<K>` of the map's keys, in the
/// map's own iteration order. The keys are the map's already-tagged `Value`s,
/// still reachable through the (caller-rooted) map box, so building the vector
/// in a single `alloc_struct` needs no intermediate rooting.
///
/// # Safety
///
/// See [`hashtable_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_keys(args: *const i64, argc: u32) -> i64 {
    let id = hashtable_arg(args, argc, "rt_hashtable_keys");
    // `args[1]` is the result's own type — a `Vector<K>`, whose instantiation
    // no value here carries. See `rt_hashtable_new`.
    let key = type_arg(*args.add(1), "rt_hashtable_keys");
    let fields: Vec<Value> = active_heap().hashtable_pairs(id).into_iter().map(|(k, _)| k).collect();
    encode(active_heap().alloc_struct(key, fields))
}

/// `(rt-hashtable-values ht)` — a fresh `Vector<V>` of the map's values; see
/// [`rt_hashtable_keys`].
///
/// # Safety
///
/// See [`hashtable_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_values(args: *const i64, argc: u32) -> i64 {
    let id = hashtable_arg(args, argc, "rt_hashtable_values");
    let key = type_arg(*args.add(1), "rt_hashtable_values");
    let fields: Vec<Value> = active_heap().hashtable_pairs(id).into_iter().map(|(_, v)| v).collect();
    encode(active_heap().alloc_struct(key, fields))
}

/// `(rt-hashtable-entries ht)` — a fresh `Vector<cons-cell<K,V>>`, each entry
/// a `cons-cell` boxing the pair (matching `typeck::variant`'s own
/// `cons-cell` layout and the interpreter's `hashtable_entries`). Unlike
/// `keys`/`values`, each `cons-cell` is a *new* allocation, so each is rooted
/// as it is built — otherwise a later `alloc_struct`'s GC could reclaim an
/// earlier, not-yet-referenced cell. (The pair's `Value`s themselves stay
/// reachable through the map box while this runs, like `keys`/`values`.)
///
/// # Safety
///
/// See [`hashtable_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_entries(args: *const i64, argc: u32) -> i64 {
    let id = hashtable_arg(args, argc, "rt_hashtable_entries");
    // Two identities, one handed in: the result is a `Vector<cons-cell<K,V>>`,
    // so each cell's type is the vector type's one argument.
    let vec_key = type_arg(*args.add(1), "rt_hashtable_entries");
    let cell_key = match typelisp_mem::type_shape(vec_key) {
        typelisp_mem::TypeShape::Named(_, _, args) if args.len() == 1 => args[0],
        _ => fatal(&format!(
            "rt_hashtable_entries: the result type `{}` is not a one-argument container",
            &*typelisp_mem::type_name(vec_key)
        )),
    };
    let pairs = active_heap().hashtable_pairs(id);
    let mut fields = Vec::with_capacity(pairs.len());
    let mut rooted = 0usize;
    for (k, v) in pairs {
        let cell = active_heap().alloc_struct(cell_key, vec![k, v]);
        active_heap().push_root(cell);
        rooted += 1;
        fields.push(cell);
    }
    let vec = active_heap().alloc_struct(vec_key, fields);
    for _ in 0..rooted {
        active_heap().pop_root();
    }
    encode(vec)
}
