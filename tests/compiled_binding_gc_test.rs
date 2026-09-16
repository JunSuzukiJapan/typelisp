//! Compiled locals that hold heap values must survive collection.
//!
//! `compiler.rs`'s `retain-bindings`/`bind-let-values` push a GC root for a
//! binding only when `Repr::binding_kind` gave it `KIND_SEXPR` (or the
//! cell-boxed `kind >= 10`). That classifier used to be *coarser* than the
//! field one, which called the same types kind `6` — a tagged heap pointer —
//! so a `defstruct`/`Vector`-typed binding and a `:dyn T` reached it as
//! `KIND_PLAIN` and got no root. Both now derive from one `Repr::class`, and
//! every reclaimable representation gets a root.
//!
//! **These tests never could have failed, and the reason is worth knowing.**
//! They were written to exploit that asymmetry and never managed to. It is
//! `compile-construct-boxed-struct` (`src/compiler.rs`): every box compiled
//! code builds — struct, enum, `Vector`, `HashTable`, `:dyn`, `Option` — is
//! `push-permanent-sexpr-root`ed the moment it is created, and a permanent
//! root is never popped (`rt_push_permanent_sexpr_root`; there is no
//! `rt_pop_permanent_sexpr_root`, and `Heap::gc`'s mark phase walks
//! `permanent_roots` in full). So a compiled struct local is immortal no
//! matter what its binding kind says, and no arrangement of allocations can
//! reclaim it.
//!
//! **The island says that coupling was on purpose**, not an oversight —
//! `compile-construct-boxed-struct`'s own doc comment argues the classifier
//! "deliberately doesn't classify a `mutable` struct type as `KIND_SEXPR`:
//! this permanent root already keeps every boxed struct alive from birth, so
//! per-binding push/pop would only add redundant bookkeeping". So this was
//! never a latent crash; it was correctness resting on a leak. The permanent
//! rooting *is* a leak by design (`rt_push_permanent_sexpr_root`'s doc: "this
//! leaks one root slot per call, forever", matching a box that is never
//! `build-free`'d). Deriving both kinds from one `Repr::class` costs redundant
//! push/pop and buys back independence: whoever eventually gives compiled
//! boxes real lifetimes no longer removes the only thing standing between an
//! unrooted binding and a collection.
//!
//! What these tests still hold is the composite property — a compiled local
//! stays valid across collections under `gc_stress` — whichever mechanism
//! provides it.

use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn stressed() -> (Heap, Checker, Interp) {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    h.set_gc_stress(true);
    (h, chk, interp)
}

fn eval_in(h: &mut Heap, chk: &mut Checker, interp: &mut Interp, src: &str) -> Result<Value, EvalError> {
    let r = Reader::new();
    let vs = r.read_all(h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(h, interp, v).expect("check failed");
        if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

/// A struct-typed local held across collections. Its field is a `string`, so a
/// recycled box or `StrId` reads back as visibly wrong text rather than as a
/// plausible integer.
#[test]
fn a_compiled_struct_local_survives_collection() {
    let (mut h, mut chk, mut interp) = stressed();
    let src = r#"(defstruct named (tag string) (n int))
                 (defun probe ((n int)) string
                   (let ((p (named::new "keepme" n)))
                     (let ((a (sexpr-cons (quote x) (quote ()))))
                       (let ((b (sexpr-cons (quote y) a)))
                         (let ((c (sexpr-cons (quote z) b)))
                           p::tag)))))
                 (compile probe)
                 (probe 7)"#;
    match eval_in(&mut h, &mut chk, &mut interp, src).expect("compile+run failed under gc stress") {
        Value::Str(id) => assert_eq!(h.string(id), "keepme"),
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
    let (mut h, mut chk, mut interp) = stressed();
    let src = r#"(defstruct tagged (tag string))
                 (defun probe () string
                   (let ((p (tagged::new "keepme")))
                     (let ((a (sexpr-cons (quote x) (quote ()))))
                       (let ((q (tagged::new "clobber")))
                         (let ((b (sexpr-cons (quote y) a)))
                           p::tag)))))
                 (compile probe)
                 (probe)"#;
    match eval_in(&mut h, &mut chk, &mut interp, src).expect("compile+run failed under gc stress") {
        Value::Str(id) => assert_eq!(
            h.string(id),
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
    let (mut h, mut chk, mut interp) = stressed();
    let src = r#"(defun probe ((n int)) int
                   (let ((v (the Vector<int> (Vector::new))))
                     (push v n)
                     (push v 20)
                     (let ((a (sexpr-cons (quote x) (quote ()))))
                       (let ((b (sexpr-cons (quote y) a)))
                         (+ (get v 0) (get v 1))))))
                 (compile probe)
                 (probe 22)"#;
    let got = eval_in(&mut h, &mut chk, &mut interp, src).expect("compile+run failed under gc stress");
    assert_eq!(got, Value::Int(42));
}
