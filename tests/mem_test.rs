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
use typelisp::{Error, Heap, Value};

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
    h.set_cdr(c, Value::Float(3.5)).unwrap();
    assert_eq!(h.car(c).unwrap(), Value::Char('x'));
    assert_eq!(h.cdr(c).unwrap(), Value::Float(3.5));
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
