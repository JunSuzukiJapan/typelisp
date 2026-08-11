//! Tests for the built-in `Scope<V>` (a stack of `String`-keyed frames —
//! the (typelisp-hosted) compiler body's replacement for a bare `HashTable`
//! as `env`/`fn-env`, see `src/compiler.rs`'s module doc comment for the
//! "list of scopes" model this implements).
//!
//! Two representations back the one method surface (unification Stage 8),
//! dispatched statically on the element type `V` (`Interp::scope_is_heap`):
//! the `Scope<i32>` tests below run the Rust-native `RtValue::Scope` path
//! (the same one `compiler.rs`'s `Scope<llvm-value>`/`Scope<llvm-function>`
//! use), the `Scope<Sexpr>`/`Scope<Vector<i32>>` tests the GC-heap
//! `StructPayload::Frames` path. Both families assert the same observable
//! semantics — that equivalence is the point.

extern crate typelisp;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// Runs `src` with the prelude loaded and **a collection before every
/// allocation** (`gc_stress`), returning the heap alongside the value so a
/// `Sexpr` result can be read out of it.
///
/// There is no `capacity` parameter any more, and that absence is the point.
/// These tests used to check against a roomy heap and then execute
/// against a 96-cell one so the churn forced repeated collections; that is not
/// expressible any more. Since the checker lowers code into cons cells, the
/// checked program *is* cells in the heap it was checked against, and handing it
/// to a second heap reads those cells through the wrong symbol/string tables
/// (`sym_names` is empty there). A 96-cell heap could not hold the prelude, let
/// alone the program. Stressing one heap is both the honest shape and strictly
/// stronger pressure — a collection before *every* allocation, not only before
/// the ones a small arena happens to block on. See `eval_test.rs`'s
/// `eval_under_gc_pressure`, which this mirrors.
///
/// Stress goes on *after* the prelude loads: it is large, and collecting through
/// it would dominate the runtime without testing anything these cases are about.
fn run_with_prelude_under_gc_stress(src: &str) -> Result<(Value, Heap), EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    h.set_gc_stress(true);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok((last, h))
}

/// Render a list-of-symbols `Sexpr` for assertions (a cut-down
/// `hashtable_test.rs::sexpr_to_string` — the GC-pressure test's kept value
/// only ever holds symbols and conses).
fn sexpr_to_string(heap: &Heap, v: Value) -> String {
    match v {
        Value::Symbol(id) => heap.symbol_name(id).to_string(),
        Value::Cons(_) => {
            let mut parts = Vec::new();
            let mut cur = v;
            while let Value::Cons(_) = cur {
                parts.push(sexpr_to_string(heap, heap.car(cur).unwrap()));
                cur = heap.cdr(cur).unwrap();
            }
            format!("({})", parts.join(" "))
        }
        other => format!("{:?}", other),
    }
}

#[test]
fn new_scope_supports_set_then_get() {
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (set s \"x\" 42)
                   (match (get s \"x\") ((Some v) v) ((None) 0))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn get_missing_key_returns_none() {
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (match (get s \"missing\") ((Some v) v) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(-1));
}

#[test]
fn push_frame_shadows_the_same_name_in_the_new_top_frame() {
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (set s \"x\" 1)
                   (push-frame s)
                   (set s \"x\" 2)
                   (match (get s \"x\") ((Some v) v) ((None) 0))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn pop_frame_reveals_the_shadowed_outer_binding_again() {
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (set s \"x\" 1)
                   (push-frame s)
                   (set s \"x\" 2)
                   (pop-frame s)
                   (match (get s \"x\") ((Some v) v) ((None) 0))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn pop_frame_removes_a_name_only_visible_in_the_popped_frame() {
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (push-frame s)
                   (set s \"y\" 9)
                   (pop-frame s)
                   (match (get s \"y\") ((Some v) v) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(-1));
}

#[test]
fn set_always_writes_into_the_most_recently_pushed_frame() {
    // Two names set in two different frames must both stay visible — `set`
    // never overwrites an outer frame's own slot for a *different* name.
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (set s \"a\" 1)
                   (push-frame s)
                   (set s \"b\" 2)
                   (+ (match (get s \"a\") ((Some v) v) ((None) 0))
                      (match (get s \"b\") ((Some v) v) ((None) 0)))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn clone_frames_shares_existing_frames_without_copying_their_entries() {
    // A name set *before* `clone-frames` is visible through the clone too —
    // proving the clone shares the original's frames (by reference) rather
    // than starting empty.
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (set s \"x\" 7)
                   (let ((s2 (clone-frames s)))
                     (match (get s2 \"x\") ((Some v) v) ((None) -1)))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn clone_frames_mutation_through_the_shared_frame_is_visible_in_both() {
    // Since `clone-frames` shares (not copies) the underlying frame, a
    // mutation made through the clone — to a name that already lived in a
    // *shared* frame — is visible back through the original too.
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (set s \"x\" 1)
                   (let ((s2 (clone-frames s)))
                     (set s2 \"x\" 2)
                     (match (get s \"x\") ((Some v) v) ((None) -1)))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn pushing_a_frame_on_the_clone_does_not_affect_the_original() {
    // A new frame pushed onto the clone (and a name set into it) must not
    // become visible through the original `Scope` — `clone-frames` makes an
    // independent *list* of frames, even though the frames it starts with
    // are shared.
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (let ((s2 (clone-frames s)))
                     (push-frame s2)
                     (set s2 \"only-in-clone\" 5))
                   (match (get s \"only-in-clone\") ((Some v) v) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(-1));
}

// ---- heap-repr `V` (`Scope<Sexpr>` etc. — `StructPayload::Frames`) ---------
//
// The same method surface as above, backed by the GC-heap representation
// (unification Stage 8). Each test mirrors a native-path sibling; the
// returned `i64` is extracted from the stored `Sexpr` with `sexpr-int` (`match`
// on a `Sexpr` is fenced off — Symbol/Sexpr redesign Phase 5), so a corrupted
// round-trip fails loudly rather than comparing equal by accident.

#[test]
fn heap_scope_set_then_get_roundtrips() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun f () i64
                 (let ((s (make-s)))
                   (set s \"x\" (quote 42))
                   (match (get s \"x\")
                     ((Some v) (sexpr-int v))
                     ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn heap_scope_get_missing_key_returns_none() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (match (get s \"missing\") ((Some v) 0) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(-1));
}

#[test]
fn heap_scope_push_frame_shadows_and_pop_frame_unshadows() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun as-int ((o Option<Sexpr>)) i64
                 (match o
                   ((Some v) (sexpr-int v))
                   ((None) -1)))
               (defun f () bool
                 (let ((s (make-s)))
                   (set s \"x\" (quote 1))
                   (push-frame s)
                   (set s \"x\" (quote 2))
                   (let ((shadowed (as-int (get s \"x\"))))
                     (pop-frame s)
                     (let ((unshadowed (as-int (get s \"x\"))))
                       (if (eq shadowed 2) (eq unshadowed 1) false)))))
               (f)";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn heap_scope_pop_frame_removes_a_name_only_visible_in_the_popped_frame() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (push-frame s)
                   (set s \"y\" (quote 9))
                   (pop-frame s)
                   (match (get s \"y\") ((Some v) 0) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(-1));
}

#[test]
fn heap_scope_clone_frames_shares_existing_frames() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun f () i64
                 (let ((s (make-s)))
                   (set s \"x\" (quote 7))
                   (let ((s2 (clone-frames s)))
                     (match (get s2 \"x\")
                       ((Some v) (sexpr-int v))
                       ((None) -1)))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn heap_scope_clone_frames_mutation_through_the_shared_frame_is_visible_in_both() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun f () i64
                 (let ((s (make-s)))
                   (set s \"x\" (quote 1))
                   (let ((s2 (clone-frames s)))
                     (set s2 \"x\" (quote 2))
                     (match (get s \"x\")
                       ((Some v) (sexpr-int v))
                       ((None) -1)))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn heap_scope_pushing_a_frame_on_the_clone_does_not_affect_the_original() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (let ((s2 (clone-frames s)))
                     (push-frame s2)
                     (set s2 \"only-in-clone\" (quote 5)))
                   (match (get s \"only-in-clone\") ((Some v) 0) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(-1));
}

/// `Heap::scope_set` panics on an empty frame stack (the mem layer's
/// internal-invariant-trap convention), but every frame is poppable from
/// typelisp — the interpreter must pre-check and report a catchable
/// `EvalError` instead, matching the native path's own behavior.
#[test]
fn heap_scope_set_with_every_frame_popped_is_a_catchable_error_not_a_panic() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun f () ()
                 (let ((s (make-s)))
                   (pop-frame s)
                   (set s \"x\" (quote 1))))
               (f)";
    match run(src) {
        Err(EvalError::Internal(msg)) => assert!(msg.contains("no frame to write into"), "unexpected message: {}", msg),
        other => panic!("expected an Internal eval error, got {:?}", other),
    }
}

/// A boxed-struct element (`Vector<i32>`, heap-repr via `struct_types`)
/// keeps its reference semantics through the scope: what `get` hands back
/// is the *same* vector box that `set` stored, so a `push` through the
/// retrieved handle is visible through the original one.
#[test]
fn heap_scope_stores_a_boxed_struct_element_by_reference() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (defun make-s () Scope<Vector<i32>> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (let ((v (make-v)))
                     (push v 10)
                     (set s \"v\" v)
                     (match (get s \"v\")
                       ((Some w) (push w 20))
                       ((None) ()))
                     (len v))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

/// The heap-scope GC contract end to end: values stored in a
/// `Scope<Sexpr>`'s frames are cons-heap pointers that must survive
/// collections triggered by later allocation churn — rooted through the
/// binding's `Slot::Heap` cell (`heap_repr_kind`'s `Scope<V>` recursion) ->
/// the scope box -> `StructPayload::Frames` -> the frame's values, all
/// traced by the mark phase alone (no interpreter-side re-collection pass).
/// Mirrors `hashtable_test.rs`'s `sexpr_values_survive_gc_pressure`.
#[test]
fn heap_scope_sexpr_values_survive_gc_pressure() {
    let src = "(defun make-s () Scope<Sexpr> (Scope::new))
               (defun f () Sexpr
                 (let ((s (make-s)))
                   (set s \"keep\" (quote (a b c d e)))
                   (dotimes (i 500)
                     (set s \"churn\" (quote (x y z))))
                   (match (get s \"keep\") ((Some v) v) ((None) (quote boom)))))
               (f)";
    let (v, h) = run_with_prelude_under_gc_stress(src).expect("eval failed");
    match v {
        sv => assert_eq!(sexpr_to_string(&h, sv), "(a b c d e)"),
    }
}
