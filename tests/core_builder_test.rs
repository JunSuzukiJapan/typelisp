//! Tests for the core-form builders (`src/check/core.rs`).
//!
//! The property that matters is survival, not shape: `Heap::cons` collects
//! whenever the free list is empty, so a node's finished fields have to stay
//! rooted while its remaining fields are built. Shape bugs show up immediately;
//! a missing root shows up as heap corruption, later, only under the right
//! allocation pattern.
//!
//! So every builder test here runs under `set_gc_stress(true)`, where each
//! allocation collects and an unrooted intermediate therefore dies at the very
//! next one — turning "fails one run in a hundred" into "fails every run".

extern crate typelisp;

use typelisp::check::core;
use typelisp::{Heap, Loc, Value};

/// A heap in stress mode: every `cons` collects first.
fn stress_heap() -> Heap {
    let mut h = Heap::with_capacity(1 << 12);
    h.set_gc_stress(true);
    h
}

// ---- shape ---------------------------------------------------------------

#[test]
fn tagged_builds_a_tagged_proper_list() {
    let mut h = stress_heap();
    let node = core::tagged(&mut h, "int-any-width", &[Value::Int(42)]).unwrap();
    h.push_root(node);

    assert_eq!(core::op(&h, node), Some("int-any-width"));
    assert_eq!(core::fields(&h, node).unwrap(), vec![Value::Int(42)]);
    assert_eq!(core::field(&h, node, 0), Some(Value::Int(42)));
    assert_eq!(core::field(&h, node, 1), None);
    assert_eq!(core::print(&h, node), "(int-any-width 42)");
}

#[test]
fn a_tag_with_no_fields_is_still_a_list() {
    let mut h = stress_heap();
    let node = core::tagged(&mut h, "break", &[]).unwrap();
    h.push_root(node);

    assert_eq!(core::op(&h, node), Some("break"));
    assert!(core::fields(&h, node).unwrap().is_empty());
    assert_eq!(core::print(&h, node), "(break)");
}

#[test]
fn op_rejects_things_that_are_not_nodes() {
    let mut h = stress_heap();
    assert_eq!(core::op(&h, Value::Int(1)), None);
    assert_eq!(core::op(&h, Value::Empty), None);
    // A list whose head is not a symbol is data, not a node.
    let data = core::list(&mut h, &[Value::Int(1), Value::Int(2)]).unwrap();
    h.push_root(data);
    assert_eq!(core::op(&h, data), None);
}

#[test]
fn list_preserves_order() {
    let mut h = stress_heap();
    let l = core::list(&mut h, &[Value::Int(1), Value::Int(2), Value::Int(3)]).unwrap();
    h.push_root(l);
    assert_eq!(core::print(&h, l), "(1 2 3)");
}

// ---- survival ------------------------------------------------------------

/// The failure `Items` exists to prevent: three sibling sub-nodes, each built
/// by an allocation that would collect the ones before it.
#[test]
fn items_keeps_every_sibling_alive_while_the_rest_are_built() {
    let mut h = stress_heap();

    let node = {
        let mut f = core::Items::new(&mut h);
        let cond = core::tagged(f.heap(), "bool", &[Value::Bool(true)]).unwrap();
        f.push(cond);
        let then = core::tagged(f.heap(), "int-any-width", &[Value::Int(1)]).unwrap();
        f.push(then);
        let els = core::tagged(f.heap(), "int-any-width", &[Value::Int(2)]).unwrap();
        f.push(els);
        f.finish("if").unwrap()
    };
    h.push_root(node);

    assert_eq!(core::print(&h, node), "(if (bool true) (int-any-width 1) (int-any-width 2))");
}

#[test]
fn deeply_nested_nodes_survive_construction() {
    let mut h = stress_heap();

    // Left-nested 30 deep: every level allocates while the previous level's
    // whole subtree is only reachable through the field being collected.
    let mut acc = core::tagged(&mut h, "int-any-width", &[Value::Int(0)]).unwrap();
    for i in 1..30i64 {
        h.push_root(acc);
        let mut f = core::Items::new(&mut h);
        f.push(acc);
        let leaf = core::tagged(f.heap(), "int-any-width", &[Value::Int(i)]).unwrap();
        f.push(leaf);
        acc = f.finish("call").unwrap();
        h.pop_root();
    }
    h.push_root(acc);

    let printed = core::print(&h, acc);
    assert!(printed.starts_with("(call (call "), "{}", printed);
    assert!(printed.ends_with("(int-any-width 29))"), "{}", printed);
    assert_eq!(printed.matches("(call ").count(), 29);
}

#[test]
fn a_wide_node_survives_construction() {
    let mut h = stress_heap();

    let node = {
        let mut f = core::Items::new(&mut h);
        for i in 0..64i64 {
            let arg = core::tagged(f.heap(), "int-any-width", &[Value::Int(i)]).unwrap();
            f.push(arg);
        }
        assert_eq!(f.as_slice().len(), 64);
        f.finish("call").unwrap()
    };
    h.push_root(node);

    let got = core::fields(&h, node).unwrap();
    assert_eq!(got.len(), 64);
    for (i, arg) in got.iter().enumerate() {
        assert_eq!(core::op(&h, *arg), Some("int-any-width"));
        assert_eq!(core::field(&h, *arg, 0), Some(Value::Int(i as i64)));
    }
}

#[test]
fn finish_list_builds_an_untagged_sublist() {
    let mut h = stress_heap();

    let binds = {
        let mut f = core::Items::new(&mut h);
        for name in ["x", "y"] {
            let sym = f.heap().intern_symbol(name);
            f.push(sym);
        }
        f.finish_list().unwrap()
    };
    h.push_root(binds);
    assert_eq!(core::print(&h, binds), "(x y)");
}

#[test]
fn items_unwinds_its_roots() {
    let mut h = stress_heap();
    let before = h.root_count();
    {
        let mut f = core::Items::new(&mut h);
        for i in 0..5i64 {
            let n = core::tagged(f.heap(), "int-any-width", &[Value::Int(i)]).unwrap();
            f.push(n);
        }
        let _ = f.finish("call").unwrap();
    }
    assert_eq!(h.root_count(), before, "builder leaked roots");
}

// ---- source locations ----------------------------------------------------

#[test]
fn tagged_at_records_a_location_the_interpreter_can_read_back() {
    use std::rc::Rc;

    let mut h = stress_heap();
    let loc = Loc::new(Rc::from("f.typl"), 4, 7).with_end(4, 15);
    let node = core::tagged_at(&mut h, "int-any-width", &[Value::Int(1)], Some(loc.clone())).unwrap();
    h.push_root(node);

    assert_eq!(h.cons_loc(node), Some(loc));

    // A node built without one simply has none — synthesized nodes are normal.
    let plain = core::tagged_at(&mut h, "int-any-width", &[Value::Int(2)], None).unwrap();
    h.push_root(plain);
    assert_eq!(h.cons_loc(plain), None);
}

/// A lowered node's location survives any number of collections, because it
/// lives in the node's own cell.
///
/// This used to be a test that the location survived the reader's per-batch
/// table clear — the hazard a lowered body outliving its read batch created
/// while locations were kept in address-keyed side tables. There is no clear
/// any more; what is worth fixing instead is that a node the collector *keeps*
/// keeps its position, under `gc_stress` where a collection runs at every
/// allocation.
#[test]
fn a_lowered_location_survives_collections() {
    use std::rc::Rc;

    let mut h = stress_heap();
    let loc = Loc::new(Rc::from("f.typl"), 1, 1).with_end(1, 6);
    let node = core::tagged_at(&mut h, "unit", &[], Some(loc.clone())).unwrap();
    h.push_root(node);

    for i in 0..16 {
        // Garbage, and a collection with it (`stress_heap` sets `gc_stress`).
        let _ = h.cons(Value::Int(i), Value::Empty).unwrap();
    }
    assert_eq!(h.cons_loc(node), Some(loc));
}

// ---- printing ------------------------------------------------------------

#[test]
fn print_renders_every_atom_kind() {
    let mut h = stress_heap();

    let s = h.alloc_string("hi".to_string());
    h.push_root(s);
    let seg_a = match h.intern_symbol("m") {
        Value::Symbol(id) => id,
        _ => unreachable!(),
    };
    let seg_b = match h.intern_symbol("f") {
        Value::Symbol(id) => id,
        _ => unreachable!(),
    };
    let path = h.intern_path(&[seg_a, seg_b]);
    h.push_root(path);

    let node = {
        let mut f = core::Items::new(&mut h);
        f.push(Value::Int(-3));
        f.push(Value::Bool(false));
        f.push(Value::Char('a'));
        f.push(s);
        f.push(path);
        f.push(Value::Empty);
        f.finish("mixed").unwrap()
    };
    h.push_root(node);

    assert_eq!(core::print(&h, node), r#"(mixed -3 false #\a "hi" m::f ())"#);
}

#[test]
fn print_shows_an_improper_tail_rather_than_hiding_it() {
    let mut h = stress_heap();
    let pair = h.cons(Value::Int(1), Value::Int(2)).unwrap();
    h.push_root(pair);
    assert_eq!(core::print(&h, pair), "(1 . 2)");
}

/// The numeric literals a core form carries are heap boxes. Printing one as an
/// opaque `#<boxed>` would make every assertion over a literal vacuous — each
/// float comparing equal to every other — so the printer has to read them.
/// This matters because `core::print` is the basis for asserting the checker's
/// lowered output as text.
#[test]
fn print_reads_the_numeric_boxes_a_literal_carries() {
    let mut h = stress_heap();

    let f = h.alloc_f64(1.5);
    h.push_root(f);
    let node = core::tagged(&mut h, "float-any-width", &[f]).unwrap();
    h.push_root(node);
    assert_eq!(core::print(&h, node), "(float-any-width 1.5)");

    // An integral float keeps its point, so it cannot be confused with `(int-any-width 1)`.
    let whole = h.alloc_f64(1.0);
    h.push_root(whole);
    let node = core::tagged(&mut h, "float-any-width", &[whole]).unwrap();
    h.push_root(node);
    assert_eq!(core::print(&h, node), "(float-any-width 1.0)");

    let b = h.alloc_bignum("123456789012345678901234567890".parse().unwrap());
    h.push_root(b);
    let node = core::tagged(&mut h, "bignum", &[b]).unwrap();
    h.push_root(node);
    assert_eq!(core::print(&h, node), "(bignum 123456789012345678901234567890)");

    let r = h.alloc_ratio(num_rational::BigRational::new(1.into(), 3.into()));
    h.push_root(r);
    let node = core::tagged(&mut h, "ratio", &[r]).unwrap();
    h.push_root(node);
    assert_eq!(core::print(&h, node), "(ratio 1/3)");

    // Two different floats must not print alike — the property that makes an
    // assertion mean anything.
    let a = h.alloc_f64(1.5);
    h.push_root(a);
    let b2 = h.alloc_f64(2.5);
    h.push_root(b2);
    assert_ne!(core::print(&h, a), core::print(&h, b2));
}
