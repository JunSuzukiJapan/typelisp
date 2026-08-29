//! The symbol table's three promises, checked by behaviour.
//!
//! A symbol's identity is the address of its header, and the table that hands
//! those addresses out is process-global, permanent, and shaped like the
//! module tree. All three are load-bearing and none of them is visible in a
//! type, so this file is what holds them:
//!
//! 1. **global** — the same name is the same symbol in every heap, which is
//!    what lets compiled code reach one with no heap in hand;
//! 2. **permanent** — nothing is ever freed, which is what makes the `unsafe`
//!    in `typelisp_mem::symbols` simple enough not to need Miri watching it
//!    (there is no deallocation to get wrong);
//! 3. **per-module** — `m::foo` and `n::foo` are different symbols, while the
//!    root's names are inherited by every module.
//!
//! The tagged-word property (the low three bits are the tag's) is here too:
//! it is an invariant of the *allocator*, so nothing in the type system
//! enforces it, and breaking it would corrupt every symbol crossing into
//! compiled code rather than failing loudly.

extern crate typelisp;

use typelisp::compile::runtime::{decode, encode};
use typelisp::mem::symbols::{self, NsId};
use typelisp::{Checker, Error, Heap, Interp, Reader, Value};

fn sym(v: Value) -> typelisp::SymRef {
    match v {
        Value::Symbol(s) => s,
        other => panic!("expected a symbol, got {:?}", other),
    }
}

// ---- 1. global -----------------------------------------------------------

#[test]
fn one_name_is_one_symbol_in_every_heap() {
    let mut a = Heap::with_capacity(64);
    let mut b = Heap::with_capacity(64);
    assert_eq!(
        a.intern_symbol("a-shared-name"),
        b.intern_symbol("a-shared-name"),
        "the table is global, so two heaps must agree on a symbol"
    );
}

#[test]
fn interning_is_case_insensitive() {
    let mut h = Heap::with_capacity(64);
    let lower = h.intern_symbol("foldcase");
    assert_eq!(lower, h.intern_symbol("FOLDCASE"));
    assert_eq!(lower, h.intern_symbol("FoldCase"));
    assert_eq!(sym(lower).name(), "foldcase", "the canonical form is the lowercase one");
}

#[test]
fn different_names_are_different_symbols() {
    let mut h = Heap::with_capacity(64);
    assert_ne!(h.intern_symbol("left"), h.intern_symbol("right"));
}

#[test]
fn concurrent_interning_of_one_name_converges() {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel();
    let mut threads = Vec::new();
    for _ in 0..8 {
        let tx = tx.clone();
        threads.push(std::thread::spawn(move || {
            // Each thread has its own heap, as every `Interp` does; the table
            // they intern through is the one shared thing.
            let mut h = Heap::with_capacity(64);
            // The address, not the `Value`: a `Value` is `!Send` (a `Cons`
            // holds a pointer into one heap's arena). A symbol's address is
            // the part that is meaningful across threads, and it is exactly
            // what is being compared.
            tx.send(sym(h.intern_symbol("raced-name")).addr()).expect("receiver lives");
        }));
    }
    drop(tx);
    for t in threads {
        t.join().expect("thread panicked");
    }
    let first = rx.recv().expect("at least one result");
    for v in rx {
        assert_eq!(v, first, "interning the same name concurrently must yield one symbol");
    }
}

// ---- 2. permanent --------------------------------------------------------

#[test]
fn a_symbol_outlives_the_heap_it_was_interned_through() {
    let s = {
        let mut h = Heap::with_capacity(64);
        sym(h.intern_symbol("outlives-its-heap"))
    };
    // The heap is gone; the symbol is not. Reading the name here is the whole
    // point — it would be a use-after-free if symbols lived in the heap.
    assert_eq!(s.name(), "outlives-its-heap");
}

#[test]
fn the_garbage_collector_does_not_reclaim_symbols() {
    let mut h = Heap::with_capacity(64);
    let s = sym(h.intern_symbol("survives-collection"));
    for _ in 0..3 {
        h.gc();
    }
    assert_eq!(s.name(), "survives-collection");
    assert_eq!(h.intern_symbol("survives-collection"), Value::Symbol(s));
}

// ---- the tagged-word invariant -------------------------------------------

#[test]
fn a_symbols_address_leaves_the_low_three_bits_free() {
    let mut h = Heap::with_capacity(64);
    // Several, including one interned late: alignment must hold for every
    // header, not just the ones the table happened to build first.
    for name in ["aligned-a", "aligned-b", "quote", "&rest", "a-much-longer-symbol-name"] {
        let s = sym(h.intern_symbol(name));
        assert_eq!(
            s.addr() & 0b111,
            0,
            "`{}`'s header is not 8-byte aligned — the 3-bit tag would collide with it",
            name
        );
    }
}

#[test]
fn a_symbol_survives_the_tagged_round_trip() {
    let mut h = Heap::with_capacity(64);
    let s = sym(h.intern_symbol("round-trips"));
    assert_eq!(decode(encode(Value::Symbol(s))), Value::Symbol(s));
}

// ---- 3. per-module -------------------------------------------------------

#[test]
fn the_same_bare_name_in_two_modules_is_two_symbols() {
    let m = symbols::ns_of(&["mod-one".to_string()]);
    let n = symbols::ns_of(&["mod-two".to_string()]);
    let in_m = symbols::intern_in(m, "a-module-local-name");
    let in_n = symbols::intern_in(n, "a-module-local-name");
    assert_ne!(in_m, in_n, "each module owns its own symbol of that name");
    assert_eq!(in_m, symbols::intern_in(m, "a-module-local-name"), "and keeps owning it");
}

#[test]
fn a_module_inherits_the_roots_names() {
    let m = symbols::ns_of(&["inheriting-module".to_string()]);
    let at_root = symbols::intern(&"already-at-root");
    assert_eq!(
        symbols::intern_in(m, "already-at-root"),
        at_root,
        "a name the root already holds is inherited, not shadowed"
    );
}

#[test]
fn a_nested_module_is_reached_through_its_parent() {
    let outer = symbols::ns_of(&["outer-mod".to_string()]);
    let inner = symbols::ns_of(&["outer-mod".to_string(), "inner-mod".to_string()]);
    assert_ne!(outer, inner);
    let at_outer = symbols::intern_in(outer, "shared-down-the-chain");
    assert_eq!(
        symbols::intern_in(inner, "shared-down-the-chain"),
        at_outer,
        "the walk goes up the whole chain, not just to the root"
    );
    assert_eq!(symbols::ns_path(inner), vec!["outer-mod".to_string(), "inner-mod".to_string()]);
}

// ---- what the inheritance rule is actually protecting ---------------------

fn check_all(src: &str) -> Result<(), Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let interp = Interp::new();
    let r = Reader::new();
    for v in r.read_all(&mut h, src)? {
        chk.check_form(&mut h, &interp, v)?;
    }
    Ok(())
}

#[test]
fn a_definition_inside_a_module_is_still_a_definition() {
    // The reader interns `defun` inside `m`, and the checker recognizes a
    // definition form by comparing that symbol against the root's `defun`. If
    // interning did not walk up to the root, this would read as a call to an
    // unbound `m::defun` instead — silently, and for every definition in every
    // module.
    check_all("(module m (defun f ((x i32)) i32 x))").expect("a defun inside a module still checks");
}

#[test]
fn a_module_body_reads_its_own_bare_names() {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let forms = r
        .read_all_in_spanned_within(&mut h, "<test>", "(module reading-mod (its-own-name))", NsId::ROOT)
        .expect("read failed");
    let module_form = forms[0].0;
    // (module reading-mod ((its-own-name)))
    let body = h.list_to_vec(module_form).expect("a proper list");
    let inner = h.list_to_vec(body[2]).expect("a proper list");
    let read_sym = sym(inner[0]);

    let ns = symbols::ns_of(&["reading-mod".to_string()]);
    assert_eq!(read_sym, symbols::intern_in(ns, "its-own-name"), "read into the module's own table");
    assert_ne!(read_sym, symbols::intern("its-own-name"), "and not into the root's");
}
