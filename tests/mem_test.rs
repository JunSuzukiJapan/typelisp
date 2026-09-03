//! Exhaustive tests for the managed cons-cell heap.
//!
//! The heap uses raw pointers internally, so these tests pin down the
//! properties that guarantee it never leaks and never corrupts memory:
//!
//!   * the accounting invariant `free + live == capacity` holds after every op;
//!   * every unreachable cell is reclaimed (including cycles — which reference
//!     counting would leak);
//!   * the arena grows only up to a bounded ceiling (past it, exhaustion is an
//!     error rather than a silent climb toward an OOM kill), growth is refusable
//!     outright, and a growth appends a chunk without invalidating a single
//!     existing cons pointer;
//!   * freed cells are reused;
//!   * GC handles deep/long structures without a native-stack overflow.
//!
//! For UB / leak detection of the unsafe internals, also run under Miri:
//!   cargo +nightly miri test --test mem_test

extern crate typelisp;
use typelisp::{BoxId, Error, Heap, Value};

/// `Heap::alloc_struct` with the type identity interned for you.
///
/// The heap stores an interned `TypeKeyId`, not a name, so a struct cannot be
/// allocated without its identity already existing (that is the point — it is
/// what makes "same type?" an integer comparison). These tests care about the
/// *name* they wrote, so they mint it here.
fn alloc_named_struct(h: &mut Heap, name: &str, fields: Vec<Value>) -> Value {
    let key = h.intern_type_key(name);
    h.alloc_struct(key, fields)
}

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
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(1), Value::Int(2)]);
    let list = h.cons(s, Value::Empty).unwrap();
    h.push_root(list);
    // unrelated garbage, both cons cells and boxes
    for i in 0..20 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap();
    }
    let _ = alloc_named_struct(&mut h, "garbage", vec![Value::Int(0)]);
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
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(1), Value::Int(2)]);
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

// ---- exhaustion ----------------------------------------------------------

/// A heap told not to grow errors rather than growing.
///
/// `set_growth_limit(0)` is explicit here because growth is the *default* now
/// (`Heap::GROWTH_FACTOR`): once the checker lowers code into cons cells, the
/// program itself lives in the heap, so no caller can pick an initial capacity
/// that is known to suffice — it depends on the size of a program not yet read.
/// Initial capacity means "allocate this much up front" and the ceiling means
/// "past here, call it a leak". A test that wants a fixed arena says so.
#[test]
fn exhaustion_errors_when_growth_is_refused() {
    let mut h = Heap::with_capacity(3);
    h.set_growth_limit(0);
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

/// Growth is a last resort, never a first one: a collection runs first, and if
/// it frees anything the arena stays the size it was.
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

// ---- bounded growth ------------------------------------------------------

/// The default ceiling: a fresh heap may grow, up to `GROWTH_FACTOR` times the
/// capacity it was asked for, and `0` turns growth off entirely.
#[test]
fn growth_is_permitted_by_default_and_bounded() {
    let h = Heap::with_capacity(1000);
    assert_eq!(h.growth_limit(), 1000 * typelisp_mem::heap::GROWTH_FACTOR, "growth must be on by default");

    let mut off = Heap::with_capacity(2);
    off.set_growth_limit(0);
    for i in 0..2 {
        let c = off.cons(Value::Int(i), Value::Empty).unwrap();
        off.push_root(c);
    }
    assert!(matches!(off.cons(Value::Int(9), Value::Empty), Err(Error::HeapExhausted)));
    assert_eq!(off.capacity(), 2);
    assert_accounting(&off);
}

/// A runaway still stops, loudly, at the default ceiling — which is what makes
/// the ceiling a leak detector rather than decoration. Without it a rooting bug
/// would grow the arena until the OS killed the process, and an OOM kill says
/// nothing about which allocation was at fault.
#[test]
fn a_runaway_stops_at_the_default_ceiling() {
    let mut h = Heap::with_capacity(4);
    let ceiling = h.growth_limit();
    // Every cell rooted, so no collection can ever reclaim one.
    let mut n = 0u64;
    loop {
        match h.cons(Value::Int(0), Value::Empty) {
            Ok(c) => {
                h.push_root(c);
                n += 1;
                assert!(n <= ceiling as u64 + 1, "grew past the ceiling without erroring");
            }
            Err(Error::HeapExhausted) => break,
            other => panic!("expected HeapExhausted at the ceiling, got {:?}", other),
        }
    }
    assert!(h.capacity() <= ceiling, "capacity {} exceeded the ceiling {}", h.capacity(), ceiling);
    assert_accounting(&h);
}

#[test]
fn growth_appends_a_chunk_once_permitted() {
    let mut h = Heap::with_capacity(2);
    h.set_growth_limit(4096);
    for i in 0..2 {
        let c = h.cons(Value::Int(i), Value::Empty).unwrap();
        h.push_root(c);
    }
    assert_eq!(h.free_count(), 0);
    // Nothing is collectible, so this must grow rather than error.
    let c = h.cons(Value::Int(99), Value::Empty).unwrap();
    h.push_root(c);
    assert!(h.capacity() > 2, "arena should have grown, cap={}", h.capacity());
    assert!(h.capacity() <= 4096);
    assert_eq!(h.car(c).unwrap(), Value::Int(99));
    assert_accounting(&h);
}

/// The whole point of chunking: growth must never move an existing cell, since
/// every `Value::Cons` is a raw pointer into the arena.
#[test]
fn existing_cons_pointers_stay_valid_across_growth() {
    let mut h = Heap::with_capacity(64);
    h.set_growth_limit(1 << 16);

    let items: Vec<i64> = (0..40).collect();
    let head = list_of(&mut h, &items);
    h.push_root(head);
    let cap_before = h.capacity();

    // Allocate hard enough to force at least one new chunk, keeping every
    // result rooted so GC cannot satisfy the demand instead.
    let mut roots = 0;
    while h.capacity() == cap_before {
        let c = h.cons(Value::Int(7), Value::Empty).unwrap();
        h.push_root(c);
        roots += 1;
        assert!(roots < 1 << 16, "arena never grew");
    }

    // The pre-growth list must still read back intact.
    assert_eq!(to_vec(&h, head), items);
    assert_accounting(&h);
}

#[test]
fn growth_collects_before_it_grows() {
    let mut h = Heap::with_capacity(8);
    h.set_growth_limit(4096);
    for i in 0..8 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap(); // unrooted garbage
    }
    assert_eq!(h.free_count(), 0);
    // Reclaimable garbage exists, so the arena must reuse it rather than grow.
    let _ = h.cons(Value::Int(42), Value::Empty).unwrap();
    assert_eq!(h.capacity(), 8, "grew instead of collecting");
    assert_accounting(&h);
}

#[test]
fn growth_stops_at_the_ceiling() {
    let mut h = Heap::with_capacity(4);
    h.set_growth_limit(2048);
    // Root everything so nothing is ever collectible: the only way forward is
    // growth, and it must stop dead at the ceiling.
    loop {
        match h.cons(Value::Int(1), Value::Empty) {
            Ok(c) => h.push_root(c),
            Err(Error::HeapExhausted) => break,
            other => panic!("unexpected {:?}", other),
        }
        assert!(h.capacity() <= 2048, "capacity {} passed the ceiling", h.capacity());
    }
    assert_eq!(h.capacity(), 2048);
    assert_accounting(&h);
}

// ---- gc stress mode ------------------------------------------------------

#[test]
fn gc_stress_reclaims_an_unrooted_value_at_the_very_next_cons() {
    let mut h = Heap::with_capacity(64);
    h.set_gc_stress(true);
    assert!(h.gc_stress());

    let orphan = h.cons(Value::Int(1), Value::Empty).unwrap(); // never rooted
    assert_eq!(h.live_count(), 1);
    // Under stress the next allocation collects first, so the orphan dies now
    // rather than whenever the free list happens to run dry.
    let kept = h.cons(Value::Int(2), Value::Empty).unwrap();
    h.push_root(kept);
    assert_eq!(h.live_count(), 1, "the orphan should have been reclaimed");
    let _ = orphan; // deliberately not dereferenced: its cell is now free
    assert_accounting(&h);
}

#[test]
fn gc_stress_keeps_properly_rooted_intermediates_alive() {
    let mut h = Heap::with_capacity(64);
    h.set_gc_stress(true);

    // The push/pop discipline every cons-building helper must follow.
    let mut acc = Value::Empty;
    for x in (0..20i64).rev() {
        h.push_root(acc);
        let next = h.cons(Value::Int(x), acc).unwrap();
        h.pop_root();
        acc = next;
        h.push_root(acc);
    }
    assert_eq!(to_vec(&h, acc), (0..20).collect::<Vec<i64>>());
    assert_accounting(&h);
}

// ---- session roots -------------------------------------------------------

#[test]
fn session_roots_keep_a_value_alive_until_the_bracket_ends() {
    let mut h = Heap::with_capacity(64);
    let mark = h.session_root_count();

    let v = h.cons(Value::Int(1), Value::Empty).unwrap();
    h.push_session_root(v); // the only thing keeping it alive
    h.gc();
    assert_eq!(h.car(v).unwrap(), Value::Int(1), "session root did not survive");

    h.truncate_session_roots(mark);
    h.gc();
    assert_eq!(h.live_count(), 0, "released session root was not reclaimed");
    assert_accounting(&h);
}

/// The reason this is not just `push_root`: compiled code unwinds the LIFO
/// stack to a recorded base (`rt_truncate_sexpr_roots`) on every `break`/
/// `return`, which would silently discard a root pushed underneath it by a
/// runtime shim.
#[test]
fn session_roots_survive_a_lifo_unwind() {
    let mut h = Heap::with_capacity(64);

    let base = h.root_count();
    let lifo = h.cons(Value::Int(1), Value::Empty).unwrap();
    h.push_root(lifo);

    let shim = h.cons(Value::Int(2), Value::Empty).unwrap();
    h.push_session_root(shim);

    // What a compiled `break` does: cut the LIFO stack back to its base.
    h.truncate_roots(base);
    h.gc();

    assert_eq!(h.car(shim).unwrap(), Value::Int(2), "the unwind took the session root with it");
    assert_eq!(h.live_count(), 1, "only the session-rooted value should survive");
    assert_accounting(&h);
}

/// Nested compile sessions: the inner bracket must release only its own.
#[test]
fn session_root_brackets_nest() {
    let mut h = Heap::with_capacity(64);

    let outer_mark = h.session_root_count();
    let outer = h.cons(Value::Int(1), Value::Empty).unwrap();
    h.push_session_root(outer);

    let inner_mark = h.session_root_count();
    let inner = h.cons(Value::Int(2), Value::Empty).unwrap();
    h.push_session_root(inner);

    h.truncate_session_roots(inner_mark);
    h.gc();
    assert_eq!(h.car(outer).unwrap(), Value::Int(1), "inner bracket released the outer's root");
    assert_eq!(h.live_count(), 1);

    h.truncate_session_roots(outer_mark);
    h.gc();
    assert_eq!(h.live_count(), 0);
    assert_accounting(&h);
}

/// A session-rooted scope box must keep its frames — and their contents —
/// reachable, which is the whole point for the compiler island.
#[test]
fn session_rooted_scopes_keep_their_frames_alive() {
    let mut h = Heap::with_capacity(256);
    let scope = h.alloc_scope();
    h.push_session_root(scope);
    let id = match scope {
        Value::Boxed(id) => id,
        _ => unreachable!(),
    };
    h.scope_push_frame(id);
    let stored = h.cons(Value::Int(7), Value::Empty).unwrap();
    h.scope_set(id, "x", stored);

    h.gc();

    assert_eq!(h.scope_get(id, "x"), Some(stored));
    assert_eq!(h.car(stored).unwrap(), Value::Int(7), "frame contents were swept");
    assert_accounting(&h);
}

// ---- RootScope -----------------------------------------------------------

#[test]
fn root_scope_truncates_back_to_its_base() {
    use typelisp::RootScope;

    let mut h = Heap::with_capacity(64);
    let outer = h.cons(Value::Int(0), Value::Empty).unwrap();
    h.push_root(outer);
    let before = h.root_count();

    {
        let mut s = RootScope::new(&mut h);
        assert_eq!(s.base(), before);
        for i in 0..5 {
            let c = s.cons(Value::Int(i), Value::Empty).unwrap();
            s.push_root(c); // deliberately never popped by hand
        }
        assert_eq!(s.root_count(), before + 5);
    }

    assert_eq!(h.root_count(), before, "scope did not unwind its own roots");
    // The caller's own root is untouched — a scope never pops past its base.
    assert_eq!(h.car(outer).unwrap(), Value::Int(0));
}

/// The failure mode hand-balanced `pop_root` pairs actually have: an early
/// `?` return skips the pops and leaves the stack permanently unbalanced.
#[test]
fn root_scope_unwinds_on_an_early_return() {
    use typelisp::RootScope;

    fn build(h: &mut Heap, fail: bool) -> Result<Value, Error> {
        let mut s = RootScope::new(h);
        let a = s.cons(Value::Int(1), Value::Empty)?;
        s.push_root(a);
        if fail {
            return Err(Error::HeapExhausted); // no manual cleanup anywhere
        }
        s.cons(Value::Int(2), a)
    }

    let mut h = Heap::with_capacity(64);
    let before = h.root_count();

    assert!(build(&mut h, true).is_err());
    assert_eq!(h.root_count(), before, "early return leaked roots");

    let ok = build(&mut h, false).unwrap();
    assert_eq!(h.root_count(), before);
    h.push_root(ok); // the caller roots the result, per the scope's contract
    assert_eq!(to_vec(&h, ok), vec![2, 1]);
    assert_accounting(&h);
}

/// A scope must actually keep its intermediates alive — the whole reason it
/// exists. Under `gc_stress` every `cons` collects, so an unrooted intermediate
/// would be gone before the next one is built.
#[test]
fn root_scope_keeps_intermediates_alive_under_gc_stress() {
    use typelisp::RootScope;

    let mut h = Heap::with_capacity(64);
    h.set_gc_stress(true);

    let node = {
        let mut s = RootScope::new(&mut h);
        let mut acc = Value::Empty;
        for x in (0..12i64).rev() {
            s.push_root(acc);
            acc = s.cons(Value::Int(x), acc).unwrap();
        }
        acc
    };
    h.push_root(node);

    assert_eq!(to_vec(&h, node), (0..12).collect::<Vec<i64>>());
    assert_accounting(&h);
}

#[test]
fn root_scopes_nest() {
    use typelisp::RootScope;

    let mut h = Heap::with_capacity(64);
    let before = h.root_count();
    {
        let mut outer = RootScope::new(&mut h);
        let a = outer.cons(Value::Int(1), Value::Empty).unwrap();
        outer.push_root(a);
        {
            let mut inner = RootScope::new(&mut outer);
            let b = inner.cons(Value::Int(2), Value::Empty).unwrap();
            inner.push_root(b);
            assert_eq!(inner.root_count(), before + 2);
        }
        // The inner scope unwound only its own root, not the outer's.
        assert_eq!(outer.root_count(), before + 1);
    }
    assert_eq!(h.root_count(), before);
}

// ---- source locations ---------------------------------------------------

/// Both slots round trip, and only a cons has them.
///
/// One slot per fact: the span of the form the cell heads (`set_cons_loc`,
/// which the reader uses for a list and the checker for a lowered node), and
/// the span of the element in its `car` (`set_elem_loc`, the only way an atom
/// gets a position — see `Cell`'s doc comment).
#[test]
fn a_cell_carries_its_own_and_its_car_s_span() {
    use std::rc::Rc;
    use typelisp::Loc;

    let mut h = Heap::with_capacity(64);
    let node = h.cons(Value::Int(1), Value::Empty).unwrap();
    h.push_root(node);
    let cr = match node {
        Value::Cons(cr) => cr,
        _ => unreachable!(),
    };
    let form = Loc::new(Rc::from("f.typl"), 3, 5).with_end(3, 9);
    let elem = Loc::new(Rc::from("f.typl"), 3, 6).with_end(3, 7);
    h.set_cons_loc(cr, form.clone());
    h.set_elem_loc(cr, elem.clone());

    assert_eq!(h.cons_loc(node), Some(form));
    assert_eq!(h.list_to_vec_locs(node).unwrap(), vec![(Value::Int(1), Some(elem))]);

    // Only a cons can carry one.
    assert_eq!(h.cons_loc(Value::Int(1)), None);
}

/// A recycled cell comes back blank.
///
/// This is what replaced two separate upkeep duties the address-keyed side
/// tables needed — a bulk clear per read batch, and a removal per reclaimed
/// cell in the sweep. With the location in the cell, `cons` blanking both slots
/// is the whole of it: there is no entry anywhere else to go stale.
#[test]
fn a_reclaimed_cell_does_not_hand_its_span_to_the_next_form() {
    use std::rc::Rc;
    use typelisp::Loc;

    let mut h = Heap::with_capacity(64);
    let orphan = h.cons(Value::Int(1), Value::Empty).unwrap(); // never rooted
    let cr = match orphan {
        Value::Cons(cr) => cr,
        _ => unreachable!(),
    };
    h.set_cons_loc(cr, Loc::new(Rc::from("f.typl"), 1, 1).with_end(1, 2));
    h.set_elem_loc(cr, Loc::new(Rc::from("f.typl"), 1, 2).with_end(1, 3));

    h.gc();

    // The recycled address must come back blank, not wearing the old spans.
    let reused = h.cons(Value::Int(2), Value::Empty).unwrap();
    h.push_root(reused);
    assert_eq!(h.cons_loc(reused), None, "stale form span on a recycled cell");
    assert_eq!(
        h.list_to_vec_locs(reused).unwrap(),
        vec![(Value::Int(2), None)],
        "stale element span on a recycled cell"
    );
    assert_accounting(&h);
}

#[test]
fn a_live_cell_keeps_its_span_across_a_collection() {
    use std::rc::Rc;
    use typelisp::Loc;

    let mut h = Heap::with_capacity(64);
    let node = h.cons(Value::Int(1), Value::Empty).unwrap();
    h.push_root(node);
    let cr = match node {
        Value::Cons(cr) => cr,
        _ => unreachable!(),
    };
    let loc = Loc::new(Rc::from("f.typl"), 7, 2).with_end(7, 8);
    h.set_cons_loc(cr, loc.clone());

    let _ = h.cons(Value::Int(9), Value::Empty).unwrap(); // garbage to collect
    h.gc();

    assert_eq!(h.cons_loc(node), Some(loc));
}

/// Spans are interned, so the many cells read from one line share one entry.
#[test]
fn identical_spans_intern_to_one_entry() {
    use std::rc::Rc;
    use typelisp::Loc;

    let mut h = Heap::with_capacity(64);
    let loc = Loc::new(Rc::from("f.typl"), 1, 1).with_end(1, 4);
    let other = Loc::new(Rc::from("f.typl"), 2, 1).with_end(2, 4);
    for i in 0..8 {
        let c = h.cons(Value::Int(i), Value::Empty).unwrap();
        h.push_root(c);
        let cr = match c {
            Value::Cons(cr) => cr,
            _ => unreachable!(),
        };
        h.set_cons_loc(cr, loc.clone());
        h.set_elem_loc(cr, other.clone());
    }
    assert_eq!(h.loc_count(), 2, "one entry per distinct span, not per cell");
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
    let fv = h.alloc_f64(3.5);
    h.set_cdr(c, fv).unwrap();
    assert_eq!(h.car(c).unwrap(), Value::Char('x'));
    match h.cdr(c).unwrap() {
        Value::Boxed(id) => assert_eq!(h.f64_value(id), 3.5),
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
    // Identity, not a count: the table is process-global and permanent, so
    // another test in this binary may have interned these names already. What
    // is being asserted is what a count was ever standing in for — one symbol
    // per name.
    let mut h = Heap::with_capacity(8);
    let a = h.intern_symbol("foo");
    let b = h.intern_symbol("foo");
    let c = h.intern_symbol("bar");
    assert_eq!(a, b, "equal names must intern to the same symbol");
    assert_ne!(a, c);
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
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(1), Value::Int(2)]);
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
    let s = alloc_named_struct(&mut h, "unit-struct", vec![]);
    match s {
        Value::Boxed(id) => assert_eq!(h.struct_field_count(id), 0),
        other => panic!("expected a boxed struct, got {:?}", other),
    }
}

#[test]
fn struct_set_field_mutates_in_place() {
    let mut h = Heap::with_capacity(8);
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(1), Value::Int(2)]);
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
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(1)]);
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
    let f = h.alloc_f64(1.5);
    let id = match f {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed float, got {:?}", other),
    };
    h.struct_type_name(id);
}

#[test]
fn unreachable_structs_are_collected() {
    let mut h = Heap::with_capacity(8);
    let _ = alloc_named_struct(&mut h, "garbage", vec![Value::Int(1)]);
    assert_eq!(h.box_count(), 1);
    h.gc(); // not rooted, not in any cell -> reclaimed
    assert_eq!(h.box_count(), 0);
}

#[test]
fn struct_reachable_via_rooted_cons_survives() {
    let mut h = Heap::with_capacity(8);
    let s = alloc_named_struct(&mut h, "keep", vec![Value::Int(1)]);
    let cell = h.cons(s, Value::Empty).unwrap();
    h.push_root(cell);
    let _ = alloc_named_struct(&mut h, "drop", vec![Value::Int(2)]); // unrooted garbage
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
    let s = alloc_named_struct(&mut h, "wrapper", vec![inner]);
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
    let s = alloc_named_struct(&mut h, "vector", vec![]);
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
    let f = h.alloc_f64(1.5);
    let id = match f {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed float, got {:?}", other),
    };
    h.struct_push_field(id, Value::Int(1));
}

#[test]
fn is_struct_distinguishes_struct_from_float() {
    let mut h = Heap::with_capacity(8);
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(1)]);
    let f = h.alloc_f64(1.5);
    let (sid, fid) = match (s, f) {
        (Value::Boxed(sid), Value::Boxed(fid)) => (sid, fid),
        other => panic!("expected two boxed values, got {:?}", other),
    };
    assert!(h.is_struct(sid));
    assert!(!h.is_struct(fid));
}

// ---- boxed hash tables ---------------------------------------------------
//
// The mem-layer representation of `HashTable<K,V>`: **hash -> bucket**, and
// nothing more. This layer neither hashes a key nor compares two, because it
// cannot — both are the key type's own `sxhash`/`equals`, written in
// typelisp, so the prelude's `get`/`set`/`remove` ask them and hand the
// answers down as a number and an index. That is what lets a `defstruct` be a
// key; the closed set of scalar shapes a `MemHashKey` used to enumerate never
// could. These tests exercise the representation itself.

fn as_boxed(v: Value) -> BoxId {
    match v {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed value, got {:?}", other),
    }
}

/// An empty table, with a type key.
///
/// `Heap::alloc_hashtable` takes the value's runtime identity because
/// `HashTable<string,i32>` and `HashTable<string,string>` are different types
/// and an empty table says nothing about which it is. At *this* layer the key
/// is only a string nobody reads back, so one spelling serves every test here.
fn new_hashtable(h: &mut Heap) -> Value {
    let key = h.intern_type_key("hashtable<i32,i32>");
    h.alloc_hashtable(key)
}

#[test]
fn hashtable_starts_empty() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    assert_eq!(h.hashtable_count(id), 0);
    assert_eq!(h.hashtable_bucket_count(id, 1), 0, "no bucket at any hash");
}

#[test]
fn a_bucket_stores_and_reads_back_both_halves() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    h.hashtable_bucket_put(id, 3, 0, Value::Int(1), Value::Char('a'));
    assert_eq!(h.hashtable_bucket_count(id, 3), 1);
    assert_eq!(h.hashtable_bucket_key(id, 3, 0), Value::Int(1));
    assert_eq!(h.hashtable_bucket_value(id, 3, 0), Value::Char('a'));
    assert_eq!(h.hashtable_count(id), 1);
}

/// Writing at an index that exists overwrites; writing at the bucket's length
/// appends. The caller decides which by looking first, which is the same look
/// it needed anyway to answer "was this key already here".
#[test]
fn a_bucket_put_overwrites_in_place_or_appends_at_the_end() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    h.hashtable_bucket_put(id, 3, 0, Value::Int(1), Value::Int(10));
    h.hashtable_bucket_put(id, 3, 0, Value::Int(1), Value::Int(20));
    assert_eq!(h.hashtable_bucket_count(id, 3), 1, "overwrite must not grow the bucket");
    assert_eq!(h.hashtable_bucket_value(id, 3, 0), Value::Int(20));
    h.hashtable_bucket_put(id, 3, 1, Value::Int(2), Value::Int(30));
    assert_eq!(h.hashtable_bucket_count(id, 3), 2);
    assert_eq!(h.hashtable_count(id), 2);
}

/// Two different hashes are two different buckets, and neither knows about
/// the other.
#[test]
fn buckets_at_different_hashes_are_independent() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    h.hashtable_bucket_put(id, 1, 0, Value::Int(1), Value::Char('a'));
    h.hashtable_bucket_put(id, 2, 0, Value::Bool(true), Value::Char('b'));
    assert_eq!(h.hashtable_bucket_value(id, 1, 0), Value::Char('a'));
    assert_eq!(h.hashtable_bucket_value(id, 2, 0), Value::Char('b'));
    assert_eq!(h.hashtable_count(id), 2);
}

#[test]
fn deleting_from_a_bucket_keeps_the_rest_in_order() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    h.hashtable_bucket_put(id, 7, 0, Value::Int(1), Value::Int(10));
    h.hashtable_bucket_put(id, 7, 1, Value::Int(2), Value::Int(20));
    h.hashtable_bucket_put(id, 7, 2, Value::Int(3), Value::Int(30));
    h.hashtable_bucket_delete(id, 7, 1);
    assert_eq!(h.hashtable_bucket_count(id, 7), 2);
    assert_eq!(h.hashtable_bucket_key(id, 7, 0), Value::Int(1));
    assert_eq!(h.hashtable_bucket_key(id, 7, 1), Value::Int(3), "the tail moves up");
}

/// An emptied bucket is dropped, so a hash nobody uses costs nothing.
#[test]
fn an_emptied_bucket_disappears() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    h.hashtable_bucket_put(id, 7, 0, Value::Int(1), Value::Int(10));
    h.hashtable_bucket_delete(id, 7, 0);
    assert_eq!(h.hashtable_bucket_count(id, 7), 0);
    assert_eq!(h.hashtable_count(id), 0);
}

#[test]
fn hashtable_clear_empties_the_map() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    h.hashtable_bucket_put(id, 1, 0, Value::Int(1), Value::Int(10));
    h.hashtable_bucket_put(id, 2, 0, Value::Int(2), Value::Int(20));
    h.hashtable_clear(id);
    assert_eq!(h.hashtable_count(id), 0);
    assert_eq!(h.hashtable_bucket_count(id, 1), 0);
}

/// `hashtable_pairs` flattens every bucket — the snapshot
/// `keys`/`values`/`entries` are built from.
#[test]
fn hashtable_pairs_sees_every_bucket() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    h.hashtable_bucket_put(id, 1, 0, Value::Int(1), Value::Int(10));
    h.hashtable_bucket_put(id, 1, 1, Value::Int(2), Value::Int(20));
    h.hashtable_bucket_put(id, 9, 0, Value::Int(3), Value::Int(30));
    let mut pairs = h.hashtable_pairs(id);
    pairs.sort_by_key(|(k, _)| match k {
        Value::Int(n) => *n,
        other => panic!("expected an int key, got {:?}", other),
    });
    assert_eq!(pairs, vec![
        (Value::Int(1), Value::Int(10)),
        (Value::Int(2), Value::Int(20)),
        (Value::Int(3), Value::Int(30)),
    ]);
}

#[test]
#[should_panic(expected = "bucket index 2 out of range")]
fn a_bucket_put_past_the_end_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(new_hashtable(&mut h));
    h.hashtable_bucket_put(id, 1, 2, Value::Int(1), Value::Int(10));
}

#[test]
#[should_panic(expected = "does not hold a HashTable")]
fn hashtable_accessor_on_a_boxed_float_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(h.alloc_f64(1.5));
    h.hashtable_count(id);
}

#[test]
#[should_panic(expected = "does not hold a HashTable")]
fn hashtable_accessor_on_a_boxed_struct_panics() {
    let mut h = Heap::with_capacity(8);
    let id = as_boxed(alloc_named_struct(&mut h, "point", vec![Value::Int(1)]));
    h.hashtable_count(id);
}

#[test]
fn is_hashtable_distinguishes_hashtable_from_struct_and_float() {
    let mut h = Heap::with_capacity(8);
    let map = as_boxed(new_hashtable(&mut h));
    let s = as_boxed(alloc_named_struct(&mut h, "point", vec![Value::Int(1)]));
    let f = as_boxed(h.alloc_f64(1.5));
    assert!(h.is_hashtable(map));
    assert!(!h.is_hashtable(s));
    assert!(!h.is_hashtable(f));
    assert!(h.is_struct(s));
    assert!(!h.is_struct(map));
}

#[test]
fn unreachable_hashtables_are_collected() {
    let mut h = Heap::with_capacity(8);
    let _ = new_hashtable(&mut h);
    assert_eq!(h.box_count(), 1);
    h.gc(); // not rooted, not in any cell -> reclaimed
    assert_eq!(h.box_count(), 0);
}

#[test]
fn hashtable_reachable_via_rooted_cons_survives() {
    let mut h = Heap::with_capacity(8);
    let map = new_hashtable(&mut h);
    let id = as_boxed(map);
    h.hashtable_bucket_put(id, 1, 0, Value::Int(1), Value::Int(99));
    let cell = h.cons(map, Value::Empty).unwrap();
    h.push_root(cell);
    let _ = new_hashtable(&mut h); // unrooted garbage
    h.gc();
    assert_eq!(h.box_count(), 1); // only the rooted map survives
    assert_eq!(h.hashtable_bucket_value(id, 1, 0), Value::Int(99));
}

/// A hash-table value that itself holds an unrelated cons must survive a GC
/// through that value alone — the direct test that `gc`'s mark phase traces
/// *into* `BoxedObj::Struct`'s `Map` payload values, not just marks the
/// map's own box slot and stops.
#[test]
fn gc_traces_into_a_rooted_hashtables_values() {
    let mut h = Heap::with_capacity(64);
    let inner = list_of(&mut h, &[10, 20, 30]);
    let map = new_hashtable(&mut h);
    let id = as_boxed(map);
    h.hashtable_bucket_put(id, 1, 0, Value::Int(1), inner);
    h.push_root(map);
    for i in 0..20 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap(); // unrelated garbage
    }
    h.gc();
    assert_eq!(to_vec(&h, h.hashtable_bucket_value(id, 1, 0)), vec![10, 20, 30]);
    assert_accounting(&h);
}

/// A string used as a hash-table key must survive a GC through the map
/// alone — the key, not just the value, has to be traced. This matters more
/// than it used to: a key is an ordinary `Value` now (a `defstruct` can be
/// one), so nothing interns it and nothing else keeps it alive.
#[test]
fn gc_keeps_a_rooted_hashtables_string_keys_alive() {
    let mut h = Heap::with_capacity(64);
    let map = new_hashtable(&mut h);
    let id = as_boxed(map);
    let key = h.alloc_string("k".to_string());
    h.hashtable_bucket_put(id, 1, 0, key, Value::Int(1));
    h.push_root(map);
    for i in 0..20 {
        let _ = h.alloc_string(format!("garbage{}", i)); // unrooted garbage strings
    }
    h.gc();
    // the key string must still read back with its content intact
    match h.hashtable_bucket_key(id, 1, 0) {
        Value::Str(sid) => assert_eq!(h.string(sid), "k"),
        other => panic!("expected the string key, got {:?}", other),
    }
    assert_eq!(h.hashtable_bucket_value(id, 1, 0), Value::Int(1));
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
    let boxed = alloc_named_struct(&mut h, "wrapper", vec![leaf]);
    let cell = h.alloc_cell(boxed);
    h.gc();
    let id = match h.cell_get(*cell) {
        Value::Boxed(id) => id,
        other => panic!("expected the struct back, got {:?}", other),
    };
    assert_eq!(h.car(h.struct_field(id, 0)).unwrap(), Value::Int(9));
    assert_eq!(h.live_count(), 1);
}

// (The `BoxedObj::Closure` heap tests that once sat here — a swept closure
// reporting its side-table token, and a cell↔closure `labels` cycle collected
// as a unit — went with the side table itself: an interpreted closure now
// carries its own code as ordinary heap data, so there is no token to report
// and the cycle is collected by the same rules as any other. What the box
// keeps alive is the test just below; `BoxedObj::CompiledClosure`'s own env
// tracing is covered in `crates/typelisp-rt`'s closure tests.)

/// An interpreted closure keeps its own *code* alive, not just its captures:
/// `params`, `ret`, `body` and `env` are all ordinary heap values, and
/// nothing else necessarily still refers to a `lambda`'s body once the form
/// that built it is gone.
///
/// `ret` is the one worth naming. The interpreter never reads it — it returns
/// the `Value` it produced — so the only reader is the compiled -> interpreted
/// boundary (`typelisp_rt::rt_apply_any`, which has to encode the result by
/// its declared representation). A slot with one distant reader is exactly
/// the kind the mark phase can quietly stop tracing, and `live_count` is what
/// notices.
#[test]
fn gc_traces_an_interpreted_closures_params_ret_body_and_env() {
    let mut h = Heap::with_capacity(8);
    let params = h.cons(Value::Int(1), Value::Empty).unwrap();
    let ret = h.cons(Value::Int(2), Value::Empty).unwrap();
    let body = h.cons(Value::Int(3), Value::Empty).unwrap();
    let env = h.cons(Value::Int(4), Value::Empty).unwrap();
    let clo = h.alloc_closure(params, ret, body, env);
    h.push_root(clo);
    h.gc();
    let id = match clo {
        Value::Boxed(id) => id,
        other => panic!("expected a boxed closure, got {:?}", other),
    };
    let (p, b, e) = h.closure_parts(id);
    assert_eq!(h.car(p).unwrap(), Value::Int(1));
    assert_eq!(h.car(h.closure_ret(id)).unwrap(), Value::Int(2));
    assert_eq!(h.car(b).unwrap(), Value::Int(3));
    assert_eq!(h.car(e).unwrap(), Value::Int(4));
    assert_eq!(h.live_count(), 4, "all four of the closure's own slots stay live");
    assert_accounting(&h);
}

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
    let id = as_boxed(new_hashtable(&mut h));
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
    let table = as_boxed(new_hashtable(&mut h));
    let strukt = as_boxed(alloc_named_struct(&mut h, "point", vec![Value::Int(1)]));
    let float = as_boxed(h.alloc_f64(1.5));
    assert!(h.is_scope(scope));
    assert!(!h.is_scope(table));
    assert!(!h.is_scope(strukt));
    assert!(!h.is_scope(float));
    assert!(!h.is_struct(scope));
    assert!(!h.is_hashtable(scope));
    assert!(!h.is_f64(scope));
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

// ---- trait objects (`BoxedObj::Dyn`, TODO T4) --------------------------
//
// A trait object is a fat box pairing a vtable identifier with the concrete
// value it dispatches for. The vtable lives outside the heap and holds no
// heap values, so the collector's whole job here is the one nested `value`.

#[test]
fn a_dyn_box_keeps_the_value_it_wraps_alive() {
    let mut h = Heap::with_capacity(64);
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(1), Value::Int(2)]);
    let d = h.alloc_dyn(7, s);
    h.push_root(d);
    let _ = alloc_named_struct(&mut h, "garbage", vec![Value::Int(0)]);
    assert_eq!(h.box_count(), 3, "the struct, its dyn box, and the unrooted garbage");
    h.gc();
    assert_eq!(h.box_count(), 2, "the dyn box and the struct it wraps both survive");
    let Value::Boxed(id) = d else { panic!("expected a boxed dyn value") };
    assert!(h.is_dyn(id));
    assert!(!h.is_struct(id), "boxing must not make the wrapper look like the struct inside");
    assert_eq!(h.dyn_vtable_id(id), 7);
    let Value::Boxed(inner) = h.dyn_value(id) else { panic!("expected the struct back") };
    assert_eq!(h.struct_type_name(inner), "point");
    assert_eq!(h.struct_field(inner, 1), Value::Int(2));
    assert_accounting(&h);
}

#[test]
fn a_dyn_box_and_its_value_are_reclaimed_together_once_unrooted() {
    let mut h = Heap::with_capacity(64);
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(1), Value::Int(2)]);
    let d = h.alloc_dyn(0, s);
    h.push_root(d);
    h.gc();
    assert_eq!(h.box_count(), 2);
    h.pop_root();
    h.gc();
    assert_eq!(h.box_count(), 0);
    assert_accounting(&h);
}

#[test]
fn a_dyn_box_reachable_only_through_a_cons_car_survives_gc() {
    let mut h = Heap::with_capacity(64);
    let s = alloc_named_struct(&mut h, "point", vec![Value::Int(3), Value::Int(4)]);
    let d = h.alloc_dyn(2, s);
    let list = h.cons(d, Value::Empty).unwrap();
    h.push_root(list);
    h.gc();
    assert_eq!(h.box_count(), 2, "the dyn box and its struct survive through the cons");
    let Value::Boxed(id) = h.car(list).unwrap() else { panic!("expected the dyn box back") };
    assert_eq!(h.dyn_vtable_id(id), 2);
    assert_accounting(&h);
}

// ---- headroom ------------------------------------------------------------

/// A live set that nearly fills the arena must make the arena grow, not make
/// every allocation collect.
///
/// The old rule was "collect when the free list empties, grow only if the
/// collection freed nothing", so a single reclaimed cell was enough to keep the
/// arena at its size — and the next `cons` emptied the free list again. Every
/// allocation then paid for a full mark of the whole live set: loading the
/// prelude into a `1 << 16` heap took ~108 seconds where a `1 << 18` one took
/// 1.2. Collection count, not wall time, is what this asserts.
#[test]
fn a_live_set_near_capacity_grows_the_arena_instead_of_collecting_every_time() {
    let mut h = Heap::with_capacity(1000);
    // Root 900 of the 1000 cells, so a collection can only ever reclaim the
    // other 100 — the shape that used to thrash.
    for i in 0..900 {
        let c = h.cons(Value::Int(i), Value::Empty).unwrap();
        h.push_root(c);
    }
    let before = h.gc_count();
    for i in 0..20_000 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap(); // garbage
    }
    let collections = h.gc_count() - before;
    assert!(h.capacity() > 1000, "the arena must have grown, got {}", h.capacity());
    // 20k allocations against ~100 reclaimable cells is 200 collections under
    // the old rule. The bound is generous: what it rules out is thrashing.
    assert!(collections < 25, "expected a handful of collections, got {}", collections);
    assert_accounting(&h);
}

/// Growth is bounded by headroom, not unbounded: a workload whose garbage is
/// most of the arena keeps the arena the size it is.
#[test]
fn plentiful_garbage_does_not_grow_the_arena() {
    let mut h = Heap::with_capacity(1000);
    for i in 0..100 {
        let c = h.cons(Value::Int(i), Value::Empty).unwrap();
        h.push_root(c);
    }
    for i in 0..20_000 {
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap(); // garbage
    }
    assert_eq!(h.capacity(), 1000, "900 free cells after each collection is headroom enough");
    assert_accounting(&h);
}
