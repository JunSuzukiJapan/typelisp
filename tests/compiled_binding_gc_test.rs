//! Compiled locals that hold heap values must survive collection.
//!
//! A binding's slot in its frame is a root when `Repr::binding_kind` says so,
//! and the slot is all that keeps a box compiled code made alive. These tests
//! run under `gc_stress`, where every safepoint collects.

mod common;
use common::{Load, Session, eval_in};
use typelisp::{Checker, Heap, Interp, Value};

fn stressed() -> (Heap, Checker, Interp) {
    Session::with_capacity(Load::Compiler, 1 << 18).stressed().into_parts()
}

/// A struct-typed local held across collections. Its field is a `string`, so a
/// recycled box or `StrId` reads back as visibly wrong text rather than as a
/// plausible integer.
#[test]
fn a_compiled_struct_local_survives_collection() {
    let (mut h, mut chk, interp) = stressed();
    let src = r#"(defstruct named (tag string) (n int))
                 (defun probe ((n int)) string
                   (let ((p (make-instance 'named :tag "keepme" :n n)))
                     (let ((a (sexpr-cons (quote x) (quote ()))))
                       (let ((b (sexpr-cons (quote y) a)))
                         (let ((c (sexpr-cons (quote z) b)))
                           p::tag)))))
                 (compile probe)
                 (probe 7)"#;
    match eval_in(&mut h, &mut chk, &interp, src).expect("compile+run failed under gc stress") {
        Value::Str(id) => assert_eq!(&*h.string(id), "keepme"),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// The two tests above allocate only *cons cells* after the local they are
/// protecting, and that is why they could never fail: a collection under
/// `gc_stress` does reclaim the struct's box, but `Heap::alloc_boxed` only
/// recycles a freed slot when something asks for a **box**
/// (`box_free.pop()`), and a `sexpr-cons` asks for a cell. The slot keeps
/// holding the reclaimed struct's own bytes, so reading the field back finds
/// exactly what was written there.
///
/// This one closes that: it interleaves the two allocations, so the collected
/// slot is handed to a *different* struct before the original is read back.
///
/// - `p` is built, then a `sexpr-cons` forces the collection that reclaims it
/// - `q` is then built, taking `p`'s just-freed box slot
/// - `p::tag` is read afterwards
///
/// With `Struct` classified `KIND_PLAIN`, `p` has no GC root anywhere: it
/// lives in a compiled `alloca` the collector cannot see, and the island's
/// `retain-bindings`/`bind-let-values` only root a `kind = 2` binding. So the
/// read comes back as `q`'s tag. Nothing about this can be explained by an
/// interpreter-side root — `p` never leaves the compiled function, is not its
/// return value, and is not passed to anything.
#[test]
fn a_compiled_struct_local_is_not_clobbered_by_a_later_box_allocation() {
    let (mut h, mut chk, interp) = stressed();
    let src = r#"(defstruct tagged (tag string))
                 (defun probe () string
                   (let ((p (make-instance 'tagged :tag "keepme")))
                     (let ((a (sexpr-cons (quote x) (quote ()))))
                       (let ((q (make-instance 'tagged :tag "clobber")))
                         (let ((b (sexpr-cons (quote y) a)))
                           p::tag)))))
                 (compile probe)
                 (probe)"#;
    match eval_in(&mut h, &mut chk, &interp, src).expect("compile+run failed under gc stress") {
        Value::Str(id) => assert_eq!(
            &*h.string(id),
            "keepme",
            "the compiled local was reclaimed and its box slot handed to a later allocation"
        ),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// The same for a `Vector<T>`, which is a boxed struct too and reaches
/// `binding_kind` the same way — but whose contents grow, so a reclaimed box
/// would also lose elements rather than only its identity.
#[test]
fn a_compiled_vector_local_survives_collection() {
    let (mut h, mut chk, interp) = stressed();
    let src = r#"(defun probe ((n int)) int
                   (let ((v (the Vector<int> (Vector::new))))
                     (push v n)
                     (push v 20)
                     (let ((a (sexpr-cons (quote x) (quote ()))))
                       (let ((b (sexpr-cons (quote y) a)))
                         (+ (get v 0) (get v 1))))))
                 (compile probe)
                 (probe 22)"#;
    let got = eval_in(&mut h, &mut chk, &interp, src).expect("compile+run failed under gc stress");
    assert_eq!(got, Value::Int(42));
}
