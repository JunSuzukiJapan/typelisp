//! Exhaustive tests for the managed cons-cell heap.
//!
//! The heap uses raw pointers internally, so these tests pin down the
//! properties that guarantee it never leaks and never corrupts memory:
//!
//!   * the accounting invariant `free + live == capacity` holds after every op;
//!   * every unreachable cell is reclaimed (including cycles — which reference
//!     counting would leak);
//!   * the arena never grows (exhaustion is an error, not a silent realloc);
//!   * freed cells are reused;
//!   * GC handles deep/long structures without a native-stack overflow.
//!
//! For UB / leak detection of the unsafe internals, also run under Miri:
//!   cargo +nightly miri test --test mem_test

extern crate typelisp;
use typelisp::{BoxId, Error, Heap, Value};

// ---- helpers ------------------------------------------------------------

/// `free + live == capacity` must hold at all times.
fn assert_accounting(h: &Heap) {
    assert_eq!(
        h.free_count() + h.live_count(),
        h.capacity(),
        "accounting invariant violated: free={} live={} cap={}",
        h.free_count(),
        h.live_count(),
        h.capacity()
    );
}

/// Build a proper list of the given ints; returns its head. The caller must
/// ensure capacity exceeds the length (no GC fires mid-build, since nothing
/// is rooted yet).
fn list_of(h: &mut Heap, items: &[i64]) -> Value {
    let mut acc = Value::Empty;
    for &x in items.iter().rev() {
        acc = h.cons(Value::Int(x), acc).unwrap();
    }
    acc
}

/// Walk a proper list of ints back into a Vec.
fn to_vec(h: &Heap, mut v: Value) -> Vec<i64> {
    let mut out = Vec::new();
    while v.is_cons() {
        if let Value::Int(n) = h.car(v).unwrap() {
            out.push(n);
        }
        v = h.cdr(v).unwrap();
    }
    out
}

// ---- basic accounting ---------------------------------------------------

#[test]
fn fresh_heap_is_all_free() {
    let h = Heap::with_capacity(16);
    assert_eq!(h.capacity(), 16);
    assert_eq!(h.free_count(), 16);
    assert_eq!(h.live_count(), 0);
    assert_accounting(&h);
}

#[test]
fn cons_consumes_one_cell_and_reads_back() {
    let mut h = Heap::with_capacity(16);
    let c = h.cons(Value::Int(1), Value::Int(2)).unwrap();
    assert_eq!(h.free_count(), 15);
    assert_eq!(h.live_count(), 1);
    assert_eq!(h.car(c).unwrap(), Value::Int(1));
    assert_eq!(h.cdr(c).unwrap(), Value::Int(2));
    assert_accounting(&h);
}

#[test]
fn accounting_holds_through_a_sequence() {
    let mut h = Heap::with_capacity(32);
    for i in 0..10 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap();
        assert_accounting(&h);
    }
    h.gc();
    assert_accounting(&h);
}

// ---- reclamation --------------------------------------------------------

#[test]
fn gc_reclaims_unrooted_cells() {
    let mut h = Heap::with_capacity(16);
    for i in 0..10 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap();
    }
    assert_eq!(h.live_count(), 10);
    let reclaimed = h.gc();
    assert_eq!(reclaimed, 10);
    assert_eq!(h.live_count(), 0);
    assert_eq!(h.free_count(), 16);
    assert_accounting(&h);
}

#[test]
fn rooted_structure_survives_gc() {
    let mut h = Heap::with_capacity(64);
    let list = list_of(&mut h, &[10, 20, 30]);
    h.push_root(list);
    // allocate garbage that should be collected
    for i in 0..20 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap();
    }
    h.gc();
    // the 3-cell rooted list survives, garbage is gone
    assert_eq!(h.live_count(), 3);
    assert_eq!(to_vec(&h, list), vec![10, 20, 30]);
    assert_accounting(&h);
}

#[test]
fn dropping_the_root_lets_it_be_collected() {
    let mut h = Heap::with_capacity(16);
    let list = list_of(&mut h, &[1, 2, 3]);
    h.push_root(list);
    h.gc();
    assert_eq!(h.live_count(), 3);
    h.pop_root();
    h.gc();
    assert_eq!(h.live_count(), 0);
    assert_eq!(h.free_count(), 16);
    assert_accounting(&h);
}

// ---- cycles (the reason we need a GC and not Rc) ------------------------

#[test]
fn two_cell_cycle_is_collected() {
    let mut h = Heap::with_capacity(16);
    let a = h.cons(Value::Int(1), Value::Empty).unwrap();
    let b = h.cons(Value::Int(2), a).unwrap();
    h.set_cdr(a, b).unwrap(); // a -> b -> a
    assert_eq!(h.live_count(), 2);
    // not rooted: a full collection must reclaim both despite the cycle
    let reclaimed = h.gc();
    assert_eq!(reclaimed, 2);
    assert_eq!(h.live_count(), 0);
    assert_accounting(&h);
}

#[test]
fn self_cycle_is_collected() {
    let mut h = Heap::with_capacity(8);
    let c = h.cons(Value::Empty, Value::Empty).unwrap();
    h.set_car(c, c).unwrap();
    h.set_cdr(c, c).unwrap(); // points to itself both ways
    h.gc();
    assert_eq!(h.live_count(), 0);
    assert_accounting(&h);
}

#[test]
fn rooted_cycle_survives_then_dies() {
    let mut h = Heap::with_capacity(16);
    let a = h.cons(Value::Int(1), Value::Empty).unwrap();
    let b = h.cons(Value::Int(2), a).unwrap();
    h.set_cdr(a, b).unwrap();
    h.push_root(a);
    h.gc();
    assert_eq!(h.live_count(), 2); // cycle kept alive via root
    h.pop_root();
    h.gc();
    assert_eq!(h.live_count(), 0); // now reclaimed
    assert_accounting(&h);
}

// ---- shared structure ---------------------------------------------------

#[test]
fn shared_substructure_lives_until_all_owners_gone() {
    let mut h = Heap::with_capacity(64);
    let shared = list_of(&mut h, &[7, 8]); // 2 cells
    let p1 = h.cons(Value::Int(1), shared).unwrap();
    let p2 = h.cons(Value::Int(2), shared).unwrap();
    h.push_root(p1);
    h.push_root(p2);
    h.gc();
    assert_eq!(h.live_count(), 4); // p1, p2, and 2 shared cells

    // drop p2 (LIFO pop): the shared tail is still reachable via p1
    h.pop_root();
    h.gc();
    assert_eq!(h.live_count(), 3); // p1 + 2 shared cells; p2 collected
    assert_eq!(to_vec(&h, h.cdr(p1).unwrap()), vec![7, 8]);
    assert_accounting(&h);
}

// ---- a boxed struct reachable only through a cons cell -------------------
//
// Sexpr-user-ADT design plan, Stage 4: the same GC path `Value::Boxed`
// float/bignum already exercised in a cons's car/cdr (no new tracing code —
// `gc`'s mark phase already recurses into any `Value::Boxed` it finds
// there), now exercised with a `BoxedObj::Struct` specifically, since a
// `defstruct` instance can now sit inside an ordinary `Sexpr` list.

#[test]
fn a_struct_reachable_only_through_a_cons_car_survives_gc() {
    let mut h = Heap::with_capacity(64);
    let s = h.alloc_struct("point".to_string(), vec![Value::Int(1), Value::Int(2)]);
    let list = h.cons(s, Value::Empty).unwrap();
    h.push_root(list);
    // unrelated garbage, both cons cells and boxes
    for i in 0..20 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap();
    }
    let _ = h.alloc_struct("garbage".to_string(), vec![Value::Int(0)]);
    assert_eq!(h.box_count(), 2, "the rooted struct plus the unrooted garbage struct");
    h.gc();
    assert_eq!(h.box_count(), 1, "only the struct reachable through the rooted cons survives");
    let Value::Boxed(id) = h.car(list).unwrap() else { panic!("expected the struct back") };
    assert_eq!(h.struct_type_name(id), "point");
    assert_eq!(h.struct_field(id, 0), Value::Int(1));
    assert_eq!(h.struct_field(id, 1), Value::Int(2));
    assert_accounting(&h);
}

#[test]
fn a_struct_reachable_only_through_a_cons_car_is_reclaimed_once_unrooted() {
    let mut h = Heap::with_capacity(64);
    let s = h.alloc_struct("point".to_string(), vec![Value::Int(1), Value::Int(2)]);
    let list = h.cons(s, Value::Empty).unwrap();
    h.push_root(list);
    h.gc();
    assert_eq!(h.box_count(), 1);
    h.pop_root();
    h.gc();
    assert_eq!(h.box_count(), 0, "the struct dies once its only cons-cell owner is unrooted and collected");
    assert_accounting(&h);
}

// ---- deep structures ----------------------------------------------------

#[test]
#[cfg_attr(miri, ignore)] // too slow under Miri's interpreter
fn deep_list_marks_without_stack_overflow() {
    let n = 100_000;
    let mut h = Heap::with_capacity(n + 1024);
    let items: Vec<i64> = (0..n as i64).collect();
    let list = list_of(&mut h, &items);
    h.push_root(list);
    h.gc(); // iterative mark must not overflow the native stack
    assert_eq!(h.live_count(), n);
    h.pop_root();
    h.gc();
    assert_eq!(h.live_count(), 0);
    assert_eq!(h.free_count(), h.capacity());
    assert_accounting(&h);
}

// ---- exhaustion (never grows) -------------------------------------------

#[test]
fn exhaustion_errors_when_everything_is_rooted() {
    let mut h = Heap::with_capacity(3);
    for i in 0..3 {
        let c = h.cons(Value::Int(i), Value::Empty).unwrap();
        h.push_root(c);
    }
    assert_eq!(h.free_count(), 0);
    // GC can free nothing (all rooted) -> allocation must error, not grow
    match h.cons(Value::Int(99), Value::Empty) {
        Err(Error::HeapExhausted) => {}
        other => panic!("expected HeapExhausted, got {:?}", other),
    }
    assert_eq!(h.capacity(), 3); // unchanged
    assert_accounting(&h);
}

#[test]
fn exhaustion_triggers_gc_then_succeeds_when_garbage_exists() {
    let mut h = Heap::with_capacity(3);
    for i in 0..3 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap(); // unrooted garbage
    }
    assert_eq!(h.free_count(), 0);
    // the next cons should GC, reclaim the garbage, and succeed
    let c = h.cons(Value::Int(42), Value::Empty).unwrap();
    assert_eq!(h.car(c).unwrap(), Value::Int(42));
    assert_eq!(h.capacity(), 3); // still no growth
    assert_eq!(h.live_count(), 1);
    assert_accounting(&h);
}

// ---- reuse / no growth --------------------------------------------------

#[test]
fn freed_cells_are_reused_without_growing() {
    let cap = 8;
    let mut h = Heap::with_capacity(cap);
    for i in 0..cap as i64 {
        let c = h.cons(Value::Int(i), Value::Empty).unwrap();
        h.push_root(c);
    }
    assert_eq!(h.free_count(), 0);
    assert_eq!(h.gc(), 0); // all rooted, nothing to reclaim
    for _ in 0..cap {
        h.pop_root();
    }
    h.gc();
    assert_eq!(h.free_count(), cap);
    // allocate again: must reuse, never exceed capacity
    for i in 0..cap as i64 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap();
    }
    assert_eq!(h.capacity(), cap);
    assert_accounting(&h);
}

// ---- mutation -----------------------------------------------------------

#[test]
fn set_car_and_set_cdr_mutate() {
    let mut h = Heap::with_capacity(8);
    let c = h.cons(Value::Int(1), Value::Int(2)).unwrap();
    h.set_car(c, Value::Char('x')).unwrap();
    let fv = h.alloc_float(3.5);
    h.set_cdr(c, fv).unwrap();
    assert_eq!(h.car(c).unwrap(), Value::Char('x'));
    match h.cdr(c).unwrap() {
        Value::Boxed(id) => assert_eq!(h.float_value(id), 3.5),
        other => panic!("expected a boxed float, got {:?}", other),
    }
}

#[test]
fn dotted_pair_holds_two_arbitrary_values() {
    let mut h = Heap::with_capacity(8);
    let p = h.cons(Value::Int(1), Value::Int(2)).unwrap();
    assert!(p.is_cons());
    assert_eq!(h.car(p).unwrap(), Value::Int(1));
    assert_eq!(h.cdr(p).unwrap(), Value::Int(2));
}

// ---- nil / non-cons semantics -------------------------------------------

#[test]
fn car_cdr_of_non_cons_errors() {
    let mut h = Heap::with_capacity(4);
    // the empty list is not a cons: car/cdr error
    assert!(matches!(h.car(Value::Empty), Err(Error::NotACons)));
    assert!(matches!(h.cdr(Value::Empty), Err(Error::NotACons)));
    assert!(matches!(h.car(Value::Int(5)), Err(Error::NotACons)));
    assert!(matches!(h.cdr(Value::Char('a')), Err(Error::NotACons)));
    assert!(matches!(h.set_car(Value::Int(5), Value::Empty), Err(Error::NotACons)));
    assert!(matches!(h.set_cdr(Value::Empty, Value::Empty), Err(Error::NotACons)));
}

// ---- symbols & strings --------------------------------------------------

#[test]
fn symbols_intern_by_name() {
    let mut h = Heap::with_capacity(8);
    let a = h.intern_symbol("foo");
    let b = h.intern_symbol("foo");
    let c = h.intern_symbol("bar");
    assert_eq!(a, b, "equal names must intern to the same symbol");
    assert_ne!(a, c);
    assert_eq!(h.symbol_count(), 2);
    if let Value::Symbol(id) = a {
        assert_eq!(h.symbol_name(id), "foo");
    } else {
        panic!("expected a symbol");
    }
}

#[test]
fn symbols_are_case_insensitive() {
    let mut h = Heap::with_capacity(8);
    let lower = h.intern_symbol("foo");
    let upper = h.intern_symbol("FOO");
    let mixed = h.intern_symbol("Foo");
    assert_eq!(lower, upper);
    assert_eq!(lower, mixed);
    assert_eq!(h.symbol_count(), 1);
    if let Value::Symbol(id) = mixed {
        assert_eq!(h.symbol_name(id), "foo"); // canonical lowercase
    } else {
        panic!("expected a symbol");
    }
}

#[test]
fn strings_store_and_read_back() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_string("hello".to_string());
    match s {
        Value::Str(id) => assert_eq!(h.string(id), "hello"),
        _ => panic!("expected a string"),
    }
    assert_eq!(h.string_count(), 1);
}

#[test]
fn unreachable_strings_are_collected() {
    let mut h = Heap::with_capacity(8);
    let _ = h.alloc_string("garbage1".to_string());
    let _ = h.alloc_string("garbage2".to_string());
    assert_eq!(h.string_count(), 2);
    h.gc(); // not rooted, not in any cell -> reclaimed
    assert_eq!(h.string_count(), 0);
}

#[test]
fn string_reachable_via_rooted_cons_survives() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_string("keep".to_string());
    let cell = h.cons(s, Value::Empty).unwrap();
    h.push_root(cell);
    let _ = h.alloc_string("drop".to_string()); // unrooted garbage
    h.gc();
    assert_eq!(h.string_count(), 1); // only "keep" survives
    // content still intact and reachable through the rooted cons
    match h.car(cell).unwrap() {
        Value::Str(id) => assert_eq!(h.string(id), "keep"),
        _ => panic!("expected a string in car"),
    }
}

#[test]
fn directly_rooted_string_survives() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_string("rooted".to_string());
    h.push_root(s);
    h.gc();
    assert_eq!(h.string_count(), 1);
}

#[test]
fn string_slots_are_recycled() {
    let mut h = Heap::with_capacity(8);
    for i in 0..100 {
        let _ = h.alloc_string(format!("s{}", i)); // all garbage
        h.gc();
    }
    // churn of 100 strings must not accumulate: at most a couple of live slots
    assert!(h.string_count() <= 1);
}

// ---- boxed structs (Sexpr/RtValue unification, Stage 1) -----------------
//
// `alloc_struct`/`struct_type_name`/`struct_field`/`struct_set_field` are
// the mem-layer representation `defstruct`/`Vector<T>`/`cons-cell<K,V>` are
// all meant to share once the interpreter/compiler are wired up to them
// (Stage 2/3) — see `docs/TODO.md`'s Sexpr/RtValue unification plan. These
// tests exercise the representation itself, independent of that wiring.

#[test]
fn structs_store_and_read_back_fields() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_struct("point".to_string(), vec![Value::Int(1), Value::Int(2)]);
    match s {
        Value::Boxed(id) => {
            assert_eq!(h.struct_type_name(id), "point");
            assert_eq!(h.struct_field_count(id), 2);
            assert_eq!(h.struct_field(id, 0), Value::Int(1));
            assert_eq!(h.struct_field(id, 1), Value::Int(2));
        }
        other => panic!("expected a boxed struct, got {:?}", other),
    }
    assert_eq!(h.box_count(), 1);
}

#[test]
fn struct_with_no_fields_has_zero_field_count() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_struct("unit-struct".to_string(), vec![]);
    match s {
        Value::Boxed(id) => assert_eq!(h.struct_field_count(id), 0),
        other => panic!("expected a boxed struct, got {:?}", other),
    }
}

#[test]
fn struct_set_field_mutates_in_place() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_struct("point".to_string(), vec![Value::Int(1), Value::Int(2)]);
    let id = match s {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed struct, got {:?}", other),
    };
    h.struct_set_field(id, 0, Value::Int(99));
    assert_eq!(h.struct_field(id, 0), Value::Int(99));
    assert_eq!(h.struct_field(id, 1), Value::Int(2), "the other field is untouched");
}

#[test]
#[should_panic(expected = "out of range")]
fn struct_field_out_of_range_panics() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_struct("point".to_string(), vec![Value::Int(1)]);
    let id = match s {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed struct, got {:?}", other),
    };
    h.struct_field(id, 1);
}

#[test]
#[should_panic(expected = "does not hold a Struct")]
fn struct_accessor_on_a_boxed_float_panics() {
    let mut h = Heap::with_capacity(8);
    let f = h.alloc_float(1.5);
    let id = match f {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed float, got {:?}", other),
    };
    h.struct_type_name(id);
}

#[test]
fn unreachable_structs_are_collected() {
    let mut h = Heap::with_capacity(8);
    let _ = h.alloc_struct("garbage".to_string(), vec![Value::Int(1)]);
    assert_eq!(h.box_count(), 1);
    h.gc(); // not rooted, not in any cell -> reclaimed
    assert_eq!(h.box_count(), 0);
}

#[test]
fn struct_reachable_via_rooted_cons_survives() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_struct("keep".to_string(), vec![Value::Int(1)]);
    let cell = h.cons(s, Value::Empty).unwrap();
    h.push_root(cell);
    let _ = h.alloc_struct("drop".to_string(), vec![Value::Int(2)]); // unrooted garbage
    h.gc();
    assert_eq!(h.box_count(), 1); // only "keep" survives
    match h.car(cell).unwrap() {
        Value::Boxed(id) => assert_eq!(h.struct_type_name(id), "keep"),
        other => panic!("expected a boxed struct in car, got {:?}", other),
    }
}

/// A struct field that itself holds an unrelated cons must survive a GC
/// through that field alone — the direct test that `gc`'s mark phase traces
/// *into* `BoxedObj::Struct`'s fields (not just marks the struct's own box
/// slot and stops).
#[test]
fn gc_traces_into_a_rooted_structs_fields() {
    let mut h = Heap::with_capacity(64);
    let inner = list_of(&mut h, &[10, 20, 30]);
    let s = h.alloc_struct("wrapper".to_string(), vec![inner]);
    h.push_root(s);
    for i in 0..20 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap(); // unrelated garbage
    }
    h.gc();
    let id = match s {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed struct, got {:?}", other),
    };
    assert_eq!(to_vec(&h, h.struct_field(id, 0)), vec![10, 20, 30]);
    assert_accounting(&h);
}

// ---- boxed structs (Sexpr/RtValue unification, Stage 2 additions) -------
//
// `struct_push_field`/`is_struct` were added alongside the interpreter
// wiring (Stage 2) — `Vector<T>::push` needs a field count that can grow
// after `alloc_struct`, and decoding a struct field/`match` scrutinee back
// into an interpreter value needs to tell a boxed struct apart from a boxed
// float without risking `float_value`'s panic.

#[test]
fn struct_push_field_grows_field_count() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_struct("vector".to_string(), vec![]);
    let id = match s {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed struct, got {:?}", other),
    };
    assert_eq!(h.struct_field_count(id), 0);
    h.struct_push_field(id, Value::Int(1));
    h.struct_push_field(id, Value::Int(2));
    assert_eq!(h.struct_field_count(id), 2);
    assert_eq!(h.struct_field(id, 0), Value::Int(1));
    assert_eq!(h.struct_field(id, 1), Value::Int(2));
}

#[test]
#[should_panic(expected = "does not hold a Struct")]
fn struct_push_field_on_a_boxed_float_panics() {
    let mut h = Heap::with_capacity(8);
    let f = h.alloc_float(1.5);
    let id = match f {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed float, got {:?}", other),
    };
    h.struct_push_field(id, Value::Int(1));
}

#[test]
fn is_struct_distinguishes_struct_from_float() {
    let mut h = Heap::with_capacity(8);
    let s = h.alloc_struct("point".to_string(), vec![Value::Int(1)]);
    let f = h.alloc_float(1.5);
    let (sid, fid) = match (s, f) {
        (Value::Boxed(sid), Value::Boxed(fid)) => (sid, fid),
        other => panic!("expected two boxed values, got {:?}", other),
    };
    assert!(h.is_struct(sid));
    assert!(!h.is_struct(fid));
}

// ---- boxed hash tables (Sexpr/RtValue unification, Stage 4) -------------
//
// `alloc_hashtable`/`hashtable_get`/`hashtable_set`/`hashtable_remove`/
// `hashtable_count`/`hashtable_clear` are the mem-layer representation
// `HashTable<K,V>` is meant to share once the interpreter is wired up to it
// (Stage 5) — see `docs/TODO.md`'s Sexpr/RtValue unification plan. These
// tests exercise the representation itself, independent of that wiring.

fn as_boxed(v: Value) -> BoxId {
    match v {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed value, got {:?}", other),
    }
}

#[test]
fn hashtable_starts_empty() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    assert_eq!(h.hashtable_count(id), 0);
    assert_eq!(h.hashtable_get(id, Value::Int(1)), None);
}

#[test]
fn hashtable_stores_and_reads_back_values() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    assert_eq!(h.hashtable_set(id, Value::Int(1), Value::Char('a')), None);
    assert_eq!(h.hashtable_set(id, Value::Bool(true), Value::Char('b')), None);
    assert_eq!(h.hashtable_get(id, Value::Int(1)), Some(Value::Char('a')));
    assert_eq!(h.hashtable_get(id, Value::Bool(true)), Some(Value::Char('b')));
    assert_eq!(h.hashtable_count(id), 2);
}

#[test]
fn hashtable_set_on_existing_key_returns_previous_value() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    assert_eq!(h.hashtable_set(id, Value::Int(1), Value::Int(10)), None);
    assert_eq!(h.hashtable_set(id, Value::Int(1), Value::Int(20)), Some(Value::Int(10)));
    assert_eq!(h.hashtable_get(id, Value::Int(1)), Some(Value::Int(20)));
    assert_eq!(h.hashtable_count(id), 1, "overwrite must not grow the entry count");
}

#[test]
fn hashtable_remove_deletes_the_entry() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    h.hashtable_set(id, Value::Int(1), Value::Int(10));
    assert_eq!(h.hashtable_remove(id, Value::Int(1)), Some(Value::Int(10)));
    assert_eq!(h.hashtable_get(id, Value::Int(1)), None);
    assert_eq!(h.hashtable_count(id), 0);
}

#[test]
fn hashtable_remove_of_missing_key_is_none() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    assert_eq!(h.hashtable_remove(id, Value::Int(1)), None);
}

#[test]
fn hashtable_clear_empties_the_map() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    h.hashtable_set(id, Value::Int(1), Value::Int(10));
    h.hashtable_set(id, Value::Int(2), Value::Int(20));
    h.hashtable_clear(id);
    assert_eq!(h.hashtable_count(id), 0);
    assert_eq!(h.hashtable_get(id, Value::Int(1)), None);
}

/// Two distinct `alloc_string` calls with identical content are two
/// distinct, non-`eq` `Value::Str`s — but as `HashTable` keys they must
/// still compare equal ("equal, not eq" — see `MemHashKey`'s doc comment).
#[test]
fn string_keys_hash_by_content_not_by_allocation_identity() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    let key_in = h.alloc_string("x".to_string());
    h.hashtable_set(id, key_in, Value::Int(42));
    let key_out = h.alloc_string("x".to_string()); // separate allocation, same content
    assert_ne!(key_in, key_out, "sanity: alloc_string does not itself dedupe");
    assert_eq!(h.hashtable_get(id, key_out), Some(Value::Int(42)));
}

#[test]
fn string_keys_with_different_content_do_not_collide() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    let x = h.alloc_string("x".to_string());
    let y = h.alloc_string("y".to_string());
    h.hashtable_set(id, x, Value::Int(1));
    h.hashtable_set(id, y, Value::Int(2));
    assert_eq!(h.hashtable_get(id, x), Some(Value::Int(1)));
    assert_eq!(h.hashtable_get(id, y), Some(Value::Int(2)));
}

/// `MemHashKey`'s variants must not collide across key *types* that happen
/// to encode to the same underlying integer bit pattern.
#[test]
fn keys_of_different_types_do_not_collide() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    h.hashtable_set(id, Value::Int(0), Value::Char('i'));
    h.hashtable_set(id, Value::Bool(false), Value::Char('b'));
    assert_eq!(h.hashtable_get(id, Value::Int(0)), Some(Value::Char('i')));
    assert_eq!(h.hashtable_get(id, Value::Bool(false)), Some(Value::Char('b')));
    assert_eq!(h.hashtable_count(id), 2);
}

#[test]
#[should_panic(expected = "does not hold a HashTable")]
fn hashtable_accessor_on_a_boxed_float_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_float(1.5));
    h.hashtable_count(id);
}

#[test]
#[should_panic(expected = "does not hold a HashTable")]
fn hashtable_accessor_on_a_boxed_struct_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_struct("point".to_string(), vec![Value::Int(1)]));
    h.hashtable_count(id);
}

#[test]
#[should_panic(expected = "unsupported key type")]
fn unsupported_key_type_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    let bad_key = h.cons(Value::Int(1), Value::Empty).unwrap();
    h.hashtable_set(id, bad_key, Value::Int(1));
}

#[test]
fn is_hashtable_distinguishes_hashtable_from_struct_and_float() {
    let mut h = Heap::with_capacity(8);
    let map = as_boxed(h.alloc_hashtable());
    let s = as_boxed(h.alloc_struct("point".to_string(), vec![Value::Int(1)]));
    let f = as_boxed(h.alloc_float(1.5));
    assert!(h.is_hashtable(map));
    assert!(!h.is_hashtable(s));
    assert!(!h.is_hashtable(f));
    assert!(h.is_struct(s));
    assert!(!h.is_struct(map));
}

#[test]
fn unreachable_hashtables_are_collected() {
    let mut h = Heap::with_capacity(8);
    let _ = h.alloc_hashtable();
    assert_eq!(h.box_count(), 1);
    h.gc(); // not rooted, not in any cell -> reclaimed
    assert_eq!(h.box_count(), 0);
}

#[test]
fn hashtable_reachable_via_rooted_cons_survives() {
    let mut h = Heap::with_capacity(8);
    let map = h.alloc_hashtable();
    let id = as_boxed(map);
    h.hashtable_set(id, Value::Int(1), Value::Int(99));
    let cell = h.cons(map, Value::Empty).unwrap();
    h.push_root(cell);
    let _ = h.alloc_hashtable(); // unrooted garbage
    h.gc();
    assert_eq!(h.box_count(), 1); // only the rooted map survives
    assert_eq!(h.hashtable_get(id, Value::Int(1)), Some(Value::Int(99)));
}

/// A hash-table value that itself holds an unrelated cons must survive a GC
/// through that value alone — the direct test that `gc`'s mark phase traces
/// *into* `BoxedObj::Struct`'s `Map` payload values, not just marks the
/// map's own box slot and stops.
#[test]
fn gc_traces_into_a_rooted_hashtables_values() {
    let mut h = Heap::with_capacity(64);
    let inner = list_of(&mut h, &[10, 20, 30]);
    let map = h.alloc_hashtable();
    let id = as_boxed(map);
    h.hashtable_set(id, Value::Int(1), inner);
    h.push_root(map);
    for i in 0..20 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap(); // unrelated garbage
    }
    h.gc();
    assert_eq!(to_vec(&h, h.hashtable_get(id, Value::Int(1)).unwrap()), vec![10, 20, 30]);
    assert_accounting(&h);
}

/// A string used as a hash-table key must survive a GC through the map
/// alone (the key, not just the value, must be traced) — otherwise a
/// second, unrelated GC pass could recycle its `StrId` slot for something
/// else while the map still holds it.
#[test]
fn gc_keeps_a_rooted_hashtables_string_keys_alive() {
    let mut h = Heap::with_capacity(64);
    let map = h.alloc_hashtable();
    let id = as_boxed(map);
    let key = h.alloc_string("k".to_string());
    h.hashtable_set(id, key, Value::Int(1));
    h.push_root(map);
    for i in 0..20 {
        let _ = h.alloc_string(format!("garbage{}", i)); // unrooted garbage strings
    }
    h.gc();
    // the key string must still resolve to the stored value after GC
    let key2 = h.alloc_string("k".to_string());
    assert_eq!(h.hashtable_get(id, key2), Some(Value::Int(1)));
}

// ---- roots bookkeeping --------------------------------------------------

#[test]
fn root_count_tracks_push_and_pop() {
    let mut h = Heap::with_capacity(8);
    assert_eq!(h.root_count(), 0);
    let c = h.cons(Value::Empty, Value::Empty).unwrap();
    h.push_root(c);
    h.push_root(c);
    assert_eq!(h.root_count(), 2);
    assert_eq!(h.pop_root(), Some(c));
    assert_eq!(h.root_count(), 1);
}

/// A permanent root keeps its value alive across a `gc()` exactly like an
/// ordinary root does, with no matching pop — proving `gc()`'s mark phase
/// genuinely walks `permanent_roots`, not just `roots`.
#[test]
fn permanent_root_survives_gc_with_no_matching_pop() {
    let mut h = Heap::with_capacity(64);
    let list = list_of(&mut h, &[10, 20, 30]);
    h.push_permanent_root(list);
    assert_eq!(h.permanent_root_count(), 1);
    for i in 0..20 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap();
    }
    h.gc();
    assert_eq!(h.live_count(), 3);
    assert_eq!(to_vec(&h, list), vec![10, 20, 30]);
    assert_eq!(h.permanent_root_count(), 1, "never popped");
    assert_accounting(&h);
}

/// A permanent root pushed *between* an ordinary root's push and pop must
/// not disturb that ordinary root's own LIFO pairing — the entire reason
/// `permanent_roots` is a separate `Vec` rather than appended to `roots`
/// (see `Heap::push_permanent_root`'s doc comment).
#[test]
fn permanent_root_does_not_desync_the_ordinary_root_stack() {
    let mut h = Heap::with_capacity(16);
    let a = h.cons(Value::Int(1), Value::Empty).unwrap();
    h.push_root(a);
    let b = h.cons(Value::Int(2), Value::Empty).unwrap();
    h.push_permanent_root(b);
    assert_eq!(h.root_count(), 1, "the permanent push left the ordinary stack untouched");
    assert_eq!(h.pop_root(), Some(a), "still pops exactly what was pushed");
    assert_eq!(h.root_count(), 0);
}

#[test]
fn gc_is_idempotent() {
    let mut h = Heap::with_capacity(16);
    let list = list_of(&mut h, &[1, 2, 3]);
    h.push_root(list);
    let first = h.gc();
    let live_after_first = h.live_count();
    let second = h.gc();
    assert_eq!(second, 0, "second GC should reclaim nothing");
    assert_eq!(h.live_count(), live_after_first);
    assert_accounting(&h);
    let _ = first;
}

// ---- stress: churn must not leak ---------------------------------------

#[test]
#[cfg_attr(miri, ignore)] // 200k iterations: too slow under Miri's interpreter
fn churn_never_leaks_or_overflows_capacity() {
    let cap = 64;
    let mut h = Heap::with_capacity(cap);
    // Allocate far more cells than capacity, all immediately garbage. The
    // auto-GC on exhaustion must keep this going without ever erroring.
    for i in 0..200_000i64 {
        let v = h.cons(Value::Int(i), Value::Empty).unwrap();
        assert!(h.live_count() <= cap);
        let _ = v; // dropped -> garbage
    }
    // a final collection with no roots returns the heap to all-free
    h.gc();
    assert_eq!(h.free_count(), cap);
    assert_eq!(h.live_count(), 0);
    assert_accounting(&h);
}

// ---- Drop: no arena leak (validated under Miri) -------------------------

#[test]
fn many_heaps_create_and_drop_cleanly() {
    for _ in 0..100 {
        let mut h = Heap::with_capacity(256);
        let list = list_of(&mut h, &[1, 2, 3, 4, 5]);
        h.push_root(list);
        h.gc();
        // dropped here: arena must be freed (Miri checks for leaks)
    }
}

// ---- binding cells (`BoxedObj::Cell`, Sexpr/RtValue unification Stage 6a) ----

#[test]
fn cell_round_trips_get_and_set() {
    let mut h = Heap::with_capacity(4);
    let cell = h.alloc_cell(Value::Int(1));
    assert!(h.is_cell(*cell));
    assert_eq!(h.cell_get(*cell), Value::Int(1));
    h.cell_set(*cell, Value::Bool(true));
    assert_eq!(h.cell_get(*cell), Value::Bool(true));
    assert_accounting(&h);
}

#[test]
fn a_live_cell_keeps_its_cons_contents_across_gc() {
    // The cell is never on the root stack at all — its liveness comes purely
    // from the `Rc<BoxId>` handle (`Heap::alloc_cell`'s registry), which is
    // exactly what protects a binding against a collection triggered from
    // anywhere (including compiled code that never re-syncs interp roots).
    let mut h = Heap::with_capacity(4);
    let kept = h.cons(Value::Int(7), Value::Empty).unwrap();
    let cell = h.alloc_cell(kept);
    h.gc();
    let held = h.cell_get(*cell);
    assert_eq!(h.car(held).unwrap(), Value::Int(7));
    assert_eq!(h.live_count(), 1);
    assert_accounting(&h);
}

#[test]
fn dropping_the_cell_handle_makes_cell_and_contents_collectable() {
    let mut h = Heap::with_capacity(4);
    let kept = h.cons(Value::Int(7), Value::Empty).unwrap();
    let cell = h.alloc_cell(kept);
    assert_eq!(h.box_count(), 1);
    drop(cell);
    h.gc();
    assert_eq!(h.box_count(), 0, "the dead binding cell itself is swept");
    assert_eq!(h.live_count(), 0, "and nothing keeps its old contents alive");
}

#[test]
fn cell_set_releases_the_old_value_and_protects_the_new() {
    let mut h = Heap::with_capacity(2);
    let first = h.cons(Value::Int(1), Value::Empty).unwrap();
    let cell = h.alloc_cell(first);
    // Overwrite the binding; the old cons becomes garbage, so the next
    // allocation can reclaim it even on this 2-cell heap.
    h.cell_set(*cell, Value::Int(0));
    let second = h.cons(Value::Int(2), Value::Empty).unwrap();
    h.cell_set(*cell, second);
    let third = h.cons(Value::Int(3), Value::Empty).unwrap(); // forces a GC over `first`
    let held = h.cell_get(*cell);
    assert_eq!(h.car(held).unwrap(), Value::Int(2));
    assert_eq!(h.car(third).unwrap(), Value::Int(3));
    assert_accounting(&h);
}

#[test]
fn gc_traces_through_cell_then_struct_then_cons() {
    let mut h = Heap::with_capacity(4);
    let leaf = h.cons(Value::Int(9), Value::Empty).unwrap();
    let boxed = h.alloc_struct("wrapper".to_string(), vec![leaf]);
    let cell = h.alloc_cell(boxed);
    h.gc();
    let id = match h.cell_get(*cell) {
        Value::Boxed(id) => id,
        other => panic!("expected the struct back, got {:?}", other),
    };
    assert_eq!(h.car(h.struct_field(id, 0)).unwrap(), Value::Int(9));
    assert_eq!(h.live_count(), 1);
}

// (The `BoxedObj::Closure` heap tests — a rooted closure keeping its
// captured cells alive, a swept closure reporting its side-table token, and a
// cell↔closure `labels` cycle collected as a unit — were removed in
// interp-closure removal Stage 8c along with `BoxedObj::Closure` itself.
// `BoxedObj::CompiledClosure` is the only closure box now; its own env-tracing
// and sweep behavior are covered in `crates/typelisp-rt`'s closure tests.)

// ---- boxed scopes (`StructPayload::Frames`, Sexpr/RtValue unification Stage 7) ----
//
// `alloc_scope`/`scope_clone_frames`/`scope_push_frame`/`scope_pop_frame`/
// `scope_get`/`scope_set` are the mem-layer representation `Scope<V>` is
// meant to share once the interpreter is wired up to it (Stage 8) — see
// `docs/TODO.md`'s Sexpr/RtValue unification plan. Each frame is a box of
// its own referenced by `BoxId` (not stored inline in the scope) so that
// `clone-frames` shares frames *by reference*, exactly like the
// pre-unification `Rc<ScopeFrame>` representation — these tests pin that
// sharing semantics down alongside the basic get/set/push/pop behavior.

#[test]
fn a_fresh_scope_starts_with_one_empty_frame() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    assert_eq!(h.scope_get(id, "x"), None);
    h.scope_set(id, "x", Value::Int(1));
    assert_eq!(h.scope_get(id, "x"), Some(Value::Int(1)));
    assert_eq!(h.box_count(), 2, "the scope box plus its first frame box");
}

#[test]
fn scope_set_overwrites_within_the_same_frame() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    h.scope_set(id, "x", Value::Int(1));
    h.scope_set(id, "x", Value::Int(2));
    assert_eq!(h.scope_get(id, "x"), Some(Value::Int(2)));
}

#[test]
fn scope_get_searches_newest_frame_first_and_pop_unshadows() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    h.scope_set(id, "x", Value::Int(1));
    h.scope_push_frame(id);
    h.scope_set(id, "x", Value::Int(2));
    assert_eq!(h.scope_get(id, "x"), Some(Value::Int(2)), "the newest frame shadows");
    h.scope_pop_frame(id);
    assert_eq!(h.scope_get(id, "x"), Some(Value::Int(1)), "popping restores the outer binding");
}

#[test]
fn scope_set_writes_only_into_the_newest_frame() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    h.scope_set(id, "x", Value::Int(1));
    h.scope_push_frame(id);
    h.scope_set(id, "y", Value::Int(2));
    h.scope_pop_frame(id);
    assert_eq!(h.scope_get(id, "y"), None, "the popped frame took its binding with it");
    assert_eq!(h.scope_get(id, "x"), Some(Value::Int(1)));
}

/// The semantics the frame-as-its-own-box representation exists for: after
/// `clone-frames`, a write into a frame both scopes stack (here, the
/// original's newest frame) is visible through both — a pointer copy per
/// frame, not a copy of its entries.
#[test]
fn clone_frames_shares_existing_frames_by_reference() {
    let mut h = Heap::with_capacity(8);
    let a = as_boxed(h.alloc_scope());
    h.scope_set(a, "x", Value::Int(1));
    let b = as_boxed(h.scope_clone_frames(a));
    assert_eq!(h.scope_get(b, "x"), Some(Value::Int(1)), "existing bindings come along");
    h.scope_set(a, "y", Value::Int(2)); // into a's newest frame — which b shares
    assert_eq!(h.scope_get(b, "y"), Some(Value::Int(2)), "a later write into the shared frame is visible through the clone");
}

#[test]
fn frames_pushed_after_clone_frames_are_not_shared() {
    let mut h = Heap::with_capacity(8);
    let a = as_boxed(h.alloc_scope());
    let b = as_boxed(h.scope_clone_frames(a));
    h.scope_push_frame(b);
    h.scope_set(b, "z", Value::Int(3));
    assert_eq!(h.scope_get(a, "z"), None, "b's own new frame is invisible to a");
    h.scope_push_frame(a);
    h.scope_set(a, "w", Value::Int(4));
    assert_eq!(h.scope_get(b, "w"), None, "and vice versa");
}

#[test]
fn scope_pop_frame_on_an_empty_stack_is_a_noop() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    h.scope_pop_frame(id); // pops the initial frame
    h.scope_pop_frame(id); // stack already empty: no panic, nothing to do
    assert_eq!(h.scope_get(id, "x"), None);
}

#[test]
#[should_panic(expected = "no frame to write into")]
fn scope_set_with_every_frame_popped_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    h.scope_pop_frame(id);
    h.scope_set(id, "x", Value::Int(1));
}

#[test]
fn scope_frame_count_tracks_push_and_pop() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    assert_eq!(h.scope_frame_count(id), 1, "a fresh scope has its first frame pushed");
    h.scope_push_frame(id);
    assert_eq!(h.scope_frame_count(id), 2);
    h.scope_pop_frame(id);
    h.scope_pop_frame(id);
    assert_eq!(h.scope_frame_count(id), 0, "the interpreter's guard for scope_set's panic");
}

#[test]
#[should_panic(expected = "does not hold a Scope")]
fn scope_accessor_on_a_hashtable_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_hashtable());
    h.scope_get(id, "x");
}

#[test]
#[should_panic(expected = "does not hold a HashTable")]
fn hashtable_accessor_on_a_scope_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    h.hashtable_count(id);
}

#[test]
fn is_scope_distinguishes_scopes_from_other_boxes() {
    let mut h = Heap::with_capacity(8);
    let scope = as_boxed(h.alloc_scope());
    let table = as_boxed(h.alloc_hashtable());
    let strukt = as_boxed(h.alloc_struct("point".to_string(), vec![Value::Int(1)]));
    let float = as_boxed(h.alloc_float(1.5));
    assert!(h.is_scope(scope));
    assert!(!h.is_scope(table));
    assert!(!h.is_scope(strukt));
    assert!(!h.is_scope(float));
    assert!(!h.is_struct(scope));
    assert!(!h.is_hashtable(scope));
    assert!(!h.is_float(scope));
}

#[test]
fn unreachable_scopes_are_collected_with_their_frames() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_scope());
    h.scope_push_frame(id);
    assert_eq!(h.box_count(), 3); // scope + 2 frames
    h.gc(); // not rooted, not in any cell -> all reclaimed
    assert_eq!(h.box_count(), 0);
}

/// A value bound in a scope frame must survive a GC through the scope root
/// alone — the direct test that the mark phase traces scope -> frame ->
/// bound values (two hops through `Value::Boxed`), not just marks the
/// scope's own box slot and stops.
#[test]
fn gc_traces_through_a_rooted_scopes_frames_into_bound_values() {
    let mut h = Heap::with_capacity(64);
    let inner = list_of(&mut h, &[10, 20, 30]);
    let scope = h.alloc_scope();
    let id = as_boxed(scope);
    h.scope_set(id, "kept", inner);
    h.push_root(scope);
    for i in 0..20 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap(); // unrelated garbage
    }
    h.gc();
    assert_eq!(to_vec(&h, h.scope_get(id, "kept").unwrap()), vec![10, 20, 30]);
    assert_accounting(&h);
}

/// A frame shared via `clone-frames` survives as long as *either* scope
/// does: collecting the (unrooted) clone must not take the shared frame —
/// or the bindings the clone wrote into it — down with it.
#[test]
fn a_shared_frame_survives_the_death_of_one_of_its_scopes() {
    let mut h = Heap::with_capacity(8);
    let a = h.alloc_scope();
    let aid = as_boxed(a);
    h.push_root(a);
    let bid = as_boxed(h.scope_clone_frames(aid));
    h.scope_set(bid, "via-b", Value::Int(7)); // into the frame b shares with a
    h.scope_push_frame(bid); // b's own private frame, dies with b
    assert_eq!(h.box_count(), 4); // a + shared frame + b + b's private frame
    h.gc(); // b is unrooted
    assert_eq!(h.box_count(), 2, "b and its private frame go; a and the shared frame stay");
    assert_eq!(h.scope_get(aid, "via-b"), Some(Value::Int(7)), "the shared frame kept the clone's write");
}
