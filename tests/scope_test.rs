//! Tests for the built-in `Scope<V>` (a stack of `String`-keyed frames —
//! the (typelisp-hosted) compiler body's replacement for a bare `HashTable`
//! as `env`/`fn-env`, see `src/compiler.rs`'s module doc comment for the
//! "list of scopes" model this implements).

extern crate typelisp;
use typelisp::{Checker, EvalError, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

#[test]
fn new_scope_supports_set_then_get() {
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (set s \"x\" 42)
                   (match (get s \"x\") ((Some v) v) ((None) 0))))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

#[test]
fn get_missing_key_returns_none() {
    let src = "(defun make-s () Scope<i32> (Scope::new))
               (defun f () i32
                 (let ((s (make-s)))
                   (match (get s \"missing\") ((Some v) v) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(-1));
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
    assert_eq!(eval_ok(src), RtValue::Int(2));
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
    assert_eq!(eval_ok(src), RtValue::Int(1));
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
    assert_eq!(eval_ok(src), RtValue::Int(-1));
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
    assert_eq!(eval_ok(src), RtValue::Int(3));
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
    assert_eq!(eval_ok(src), RtValue::Int(7));
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
    assert_eq!(eval_ok(src), RtValue::Int(2));
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
    assert_eq!(eval_ok(src), RtValue::Int(-1));
}
