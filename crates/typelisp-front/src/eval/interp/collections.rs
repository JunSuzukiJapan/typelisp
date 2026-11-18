//! The `HashTable`, `Vector` and heap `Scope` built-in methods.

use typelisp_mem::RootScope;
use crate::{BoxId, Heap, TypeId, Value};
use crate::eval::value::EvalError;
use super::expect_str;

/// The bucket primitives, the interpreted half of `rt_hashtable_bucket_*`.
/// The hash and the index are ordinary `i32`s the prelude computed; this
/// layer neither hashes nor compares, because it cannot — see
/// `HashTable`'s definition (`typeck::builtins`).
fn bucket_args(args: &[Value], who: &str) -> Result<(BoxId, i64, usize), EvalError> {
    let id = expect_struct_box(&args[0])?;
    let Value::Int(hash) = args[1] else {
        return Err(EvalError::Internal(format!("{}: hash is not an integer", who)));
    };
    let i = match args.get(2) {
        Some(Value::Int(n)) => usize::try_from(*n)
            .map_err(|_| EvalError::Panic(format!("{}: negative bucket index {}", who, n)))?,
        Some(_) => return Err(EvalError::Internal(format!("{}: index is not an integer", who))),
        None => 0,
    };
    Ok((id, hash, i))
}

pub(super) fn hashtable_bucket_count(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let (id, hash, _) = bucket_args(args, "bucket-count")?;
    Ok(Value::Int(heap.hashtable_bucket_count(id, hash) as i64))
}

pub(super) fn hashtable_bucket_key(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let (id, hash, i) = bucket_args(args, "bucket-key")?;
    Ok(heap.hashtable_bucket_key(id, hash, i))
}

pub(super) fn hashtable_bucket_value(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let (id, hash, i) = bucket_args(args, "bucket-value")?;
    Ok(heap.hashtable_bucket_value(id, hash, i))
}

pub(super) fn hashtable_bucket_put(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let (id, hash, i) = bucket_args(args, "bucket-put")?;
    heap.hashtable_bucket_put(id, hash, i, args[3], args[4]);
    Ok(Value::Empty)
}

pub(super) fn hashtable_bucket_delete(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let (id, hash, i) = bucket_args(args, "bucket-delete")?;
    heap.hashtable_bucket_delete(id, hash, i);
    Ok(Value::Empty)
}

pub(super) fn hashtable_count(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(Value::Int(heap.hashtable_count(id) as i64))
}

pub(super) fn hashtable_clear(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.hashtable_clear(id);
    Ok(Value::Empty)
}

pub(super) fn hashtable_keys(heap: &mut Heap, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let fields = heap.hashtable_pairs(id).into_iter().map(|(k, _)| k).collect();
    Ok(crate::type_key::alloc_struct_of(heap, ret_type, fields))
}

pub(super) fn hashtable_values(heap: &mut Heap, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let fields = heap.hashtable_pairs(id).into_iter().map(|(_, v)| v).collect();
    Ok(crate::type_key::alloc_struct_of(heap, ret_type, fields))
}

/// Each entry becomes a `cons-cell<K,V>` (`prelude.rs`'s generic `car`/`cdr`
/// `defstruct`, the boxed-struct `"cons-cell"` representation, matching how
/// `typeck::variant` would build one from typelisp source; built
/// directly here since a defstruct instance is just tagged field data, not
/// something only the checker/prelude can construct) wrapping the pair's
/// already-encoded key/value `mem::Value`s straight from
/// `Heap::hashtable_pairs`.
pub(super) fn hashtable_entries(heap: &mut Heap, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    // Two identities here, and only the outer one was handed in: the result is
    // a `Vector<cons-cell<K,V>>`, so each cell's own type is the vector's one
    // argument.
    let cell_type = match typelisp_mem::type_shape(ret_type) {
        typelisp_mem::TypeShape::Named(_, _, args) if args.len() == 1 => args[0],
        _ => panic!("HashTable::entries: `{}` is not a one-argument container type", &*typelisp_mem::type_name(ret_type)),
    };
    // Each cell is a new box that nothing points at until the vector is built,
    // so each is rooted as it is made: building the next one can collect.
    let mut s = RootScope::new(heap);
    let mut fields = Vec::new();
    for (k, v) in s.hashtable_pairs(id) {
        let cell = crate::type_key::alloc_struct_of(&mut s, cell_type, vec![k, v]);
        s.push_root(cell);
        fields.push(cell);
    }
    Ok(crate::type_key::alloc_struct_of(&mut s, ret_type, fields))
}

/// The `BoxId` behind a `defstruct`/`Vector<T>`/`cons-cell<K,V>` instance —
/// they are ordinary boxed structs rather than a representation of their own.
/// Doesn't itself check `Heap::is_struct` (a
/// mismatched-kind `BoxId` — e.g. a boxed float reaching here — is caught by
/// the panic in whichever `Heap` struct accessor the caller goes on to call,
/// the same internal-invariant-trap convention `Heap::struct_field` etc.
/// already use).
pub fn expect_struct_box(v: &Value) -> Result<BoxId, EvalError> {
    match v {
        Value::Boxed(id) => Ok(*id),
        other => Err(EvalError::Internal(format!("expected a boxed struct, got {:?}", other))),
    }
}

/// A `Vector<T>` index, rejected the same way an index past the end is.
///
/// A negative index used to report itself in a shape of its own (`Vector:
/// invalid index Int(-1)`, a Rust `Debug` rendering leaking into a user-facing
/// message). It is an out-of-bounds access like any other, and compiled code —
/// where the index arrives as a bare `i64` with no `Value` around it — has no
/// way to say it differently anyway; `typelisp_rt::checked_field_index` is the
/// other half, and `tests/runtime_error_parity_test.rs` holds the two together.
/// A non-`Int` here is a checker failure rather than a program error.
fn expect_int_index(v: &Value) -> Result<usize, EvalError> {
    match v {
        Value::Int(n) if *n >= 0 => Ok(*n as usize),
        Value::Int(n) => Err(EvalError::Panic(format!("Vector: index {} out of bounds", n))),
        Value::Boxed(_) => Err(EvalError::Panic("Vector: index does not fit a fixnum".into())),
        other => Err(EvalError::Internal(format!("Vector: index is not an integer: {:?}", other))),
    }
}

pub(super) fn vector_push(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.struct_push_field(id, args[1]);
    Ok(Value::Empty)
}

/// `ret_ty` is the call site's checked return type — always the concrete
/// element type post-monomorphization (`Vector<Sexpr>`'s `get` returns
/// `Sexpr` there), so the element decode is fully type-directed; see
/// [`decode_field_typed`].
pub(super) fn vector_get(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let i = expect_int_index(&args[1])?;
    if i >= heap.struct_field_count(id) {
        return Err(EvalError::Panic(format!("Vector: index {} out of bounds", i)));
    }
    Ok(heap.struct_field(id, i))
}

pub(super) fn vector_set(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let i = expect_int_index(&args[1])?;
    if i >= heap.struct_field_count(id) {
        return Err(EvalError::Panic(format!("Vector: index {} out of bounds", i)));
    }
    heap.struct_set_field(id, i, args[2]);
    Ok(Value::Empty)
}

pub(super) fn vector_len(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(Value::Int(heap.struct_field_count(id) as i64))
}

/// Unlike [`vector_get`]/[`vector_set`] (which panic out of range), `pop`
/// returns `Option<T>` — an empty vector is a legitimate `None`, not a
/// bounds violation — the same shape [`hashtable_remove`] uses for its own
/// "might not be there" result. `ret_ty` is the call site's checked
/// `Option<T>` return type; [`option_payload_ty`] extracts `T` for
/// [`decode_field_typed`]'s type-directed decode of the popped element.
pub(super) fn vector_pop(heap: &mut Heap, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let popped = heap.struct_pop_field(id);
    Ok(heap.alloc_option(ret_type, popped))
}

// The `Scope<V>`-with-heap-repr-`V` counterparts of the native scope
// helpers above (unification Stage 8) — same six-method surface, backed by
// `Heap`'s `StructPayload::Frames` representation instead of `Rc` frames.
// Which family a call lands in is decided statically, from the receiver's
// checked `Scope<V>` type ([`Interp::scope_is_heap`]) — never from the
// receiver value's shape. `V` being heap-repr means every stored/returned
// element is a heap value by construction, so — unlike `hashtable_get`,
// whose `V` can be anything — no typed decode is needed on the way out and
// a non-`Sexpr` element on the way in is an internal-invariant trap.

pub fn scope_clone_frames_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(heap.scope_clone_frames(id))
}

pub fn scope_push_frame_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.scope_push_frame(id);
    Ok(Value::Empty)
}

pub fn scope_pop_frame_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.scope_pop_frame(id);
    Ok(Value::Empty)
}

pub(crate) fn scope_get_heap(heap: &mut Heap, args: &[Value], ret_type: TypeId) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    // Scoped so the read-locked guard `expect_str` returns is dropped before
    // `alloc_option` needs `heap` mutably below — unlike a plain `&str`,
    // this guard's `Drop` impl keeps NLL from ending its borrow at its last
    // use, so it has to be ended explicitly (`typelisp-mem`'s `Heap::string`
    // doc comment, `docs/dev/os-threads-design.md` §3).
    let found = {
        let name = expect_str(heap, &args[1])?;
        heap.scope_get(id, &name)
    };
    Ok(heap.alloc_option(ret_type, found))
}

/// `Heap::scope_set` panics on an empty frame stack (the mem layer's
/// internal-invariant-trap convention); every frame *is* poppable from
/// typelisp (`pop-frame`), so the guard runs here first and reports the
/// same `EvalError` the native `scope_set` does — the `expect_hashable_key`
/// precedent of keeping a user-reachable condition a catchable evaluation
/// error rather than a Rust panic.
pub(crate) fn scope_set_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    scope_store(heap, args, args[2])
}

/// `set` for a value that is already in its stored form — the `rt_llvm_call`
/// path, where the element arrives as the very word compiled code holds and
/// there is no value to encode.
pub fn scope_set_raw(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    scope_store(heap, args, args[2])
}

/// The shared tail of both `set` paths.
///
/// `Heap::scope_set` panics on an empty frame stack (the mem layer's
/// internal-invariant-trap convention); every frame *is* poppable from
/// typelisp (`pop-frame`), so the guard runs here first and reports a
/// catchable `EvalError` instead — the `expect_hashable_key` precedent of
/// keeping a user-reachable condition an evaluation error rather than a Rust
/// panic. That matters doubly on the `rt_llvm_call` path: a panic unwinding
/// out of an `extern "C"` shim aborts the process, losing even the
/// `rt_llvm_fatal` diagnostic.
fn scope_store(heap: &mut Heap, args: &[Value], v: Value) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let name = expect_str(heap, &args[1])?.to_string();
    if heap.scope_frame_count(id) == 0 {
        return Err(EvalError::Internal("Scope::set: no frame to write into".into()));
    }
    heap.scope_set(id, &name, v);
    Ok(Value::Empty)
}
