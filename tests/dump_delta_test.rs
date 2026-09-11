//! The guard on `dump`'s delta: everything a load put into the checker must
//! come back out of a captured unit.
//!
//! A dump replaces read+check at startup, so anything the capture walk fails to
//! see is a definition that silently stops existing — and the failure surfaces
//! far away, as an unresolved name in a program that used to compile. These
//! tests build one environment the slow way and one from a unit, and compare
//! the checker's whole content-signature: every table, every entry, hashed by
//! what it would serialize to.

use typelisp::dump::{self, RegistrySignature};
use typelisp::{Checker, Heap, Interp, Value};

/// Loads the prelude the ordinary way, capturing every checked top-level form.
fn load_recording(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) -> Vec<Value> {
    let mut forms = Vec::new();
    typelisp::prelude::load_interpreted_with(heap, chk, interp, &mut |_, _| {}, &mut |heap, tl| {
        // Rooted for as long as the list is: `exec` permanently roots a
        // definition's *body*, not the top-level node collected here, and a
        // later form's checking allocates.
        heap.push_root(tl);
        forms.push(tl);
    });
    forms
}

fn assert_same(label: &str, a: &RegistrySignature, b: &RegistrySignature) {
    let diff = a.difference(b);
    assert!(
        diff.is_empty(),
        "{}: the restored checker differs from the one that ran the source ({} entries):\n  {}",
        label,
        diff.len(),
        diff.join("\n  ")
    );
}

#[test]
fn a_restored_prelude_unit_holds_everything_the_source_defined() {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let before = chk.signature(&heap).expect("signature of a fresh checker");
    let forms = load_recording(&mut heap, &mut chk, &mut interp);
    let delta = chk.capture_delta(&heap, &before).expect("capturing the delta");
    let unit = dump::capture_types(&heap, delta, "prelude", None, None, &forms, Vec::new())
        .expect("capturing the prelude unit");
    let expected = chk.signature(&heap).expect("signature after the slow load");

    let mut heap2 = Heap::with_capacity(1 << 18);
    let mut chk2 = Checker::new();
    let mut interp2 = Interp::new();
    dump::apply_types(&mut heap2, &mut chk2, &mut interp2, unit).expect("applying the prelude unit");
    let restored = chk2.signature(&heap2).expect("signature after applying the unit");

    assert_same("prelude", &expected, &restored);
}

#[test]
fn a_restored_prelude_unit_can_check_and_run_a_program() {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let before = chk.signature(&heap).expect("signature of a fresh checker");
    let forms = load_recording(&mut heap, &mut chk, &mut interp);
    let delta = chk.capture_delta(&heap, &before).expect("capturing the delta");
    let unit = dump::capture_types(&heap, delta, "prelude", None, None, &forms, Vec::new())
        .expect("capturing the prelude unit");

    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    dump::apply_types(&mut heap, &mut chk, &mut interp, unit).expect("applying the prelude unit");

    // `length<I,A> (where (Iter I (Item A)))` is one of the 61 generic prelude
    // `defun`s the checker retains as a *template* rather than a signature, so
    // this call can only check if the restored template came back — and with
    // it `string`'s `impl Iter`, which is what pins `A` to `char`.
    let src = r#"(length "abc")"#;
    let r = typelisp::Reader::new();
    let read = r.read_all(&mut heap, src).expect("read");
    let mut last = None;
    for v in read {
        let tl = chk.check_form(&mut heap, &interp, v).expect("check against the restored checker");
        last = interp.exec(&mut heap, tl).expect("exec");
    }
    assert_eq!(last, Some(Value::Int(3)));
}

#[test]
fn a_prelude_macro_survives_the_round_trip() {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let before = chk.signature(&heap).expect("signature of a fresh checker");
    let forms = load_recording(&mut heap, &mut chk, &mut interp);
    let delta = chk.capture_delta(&heap, &before).expect("capturing the delta");
    let unit = dump::capture_types(&heap, delta, "prelude", None, None, &forms, Vec::new())
        .expect("capturing the prelude unit");

    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    dump::apply_types(&mut heap, &mut chk, &mut interp, unit).expect("applying the prelude unit");

    let src = r#"(length (with-output-to-string (s) (write-string "hello" s)))"#;
    let r = typelisp::Reader::new();
    let read = r.read_all(&mut heap, src).expect("read");
    let mut last = None;
    for v in read {
        let tl = chk.check_form(&mut heap, &interp, v).expect("check");
        last = interp.exec(&mut heap, tl).expect("exec");
    }
    assert_eq!(last, Some(Value::Int(5)));
}
