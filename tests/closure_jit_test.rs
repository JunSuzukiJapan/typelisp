//! Closure unification Stage 7 (definition-time JIT) tests: every closure
//! value here (`lambda`/`labels` sibling/reified named function) is
//! evaluated by *pure interpretation* — no `(compile ...)` call anywhere in
//! any of these programs — so a passing test exercises
//! `Interp::jit_define_closure`'s attempt-first-fall-back-second path, not
//! the pre-existing `(compile fn)`-triggered pipeline `compile_test.rs`
//! covers.
//!
//! Every assertion here is correct whether the JIT attempt actually
//! succeeds or silently falls back to `Interp::make_closure`
//! (`TYPELISP_CLOSURE_JIT` defaults to `prefer`) — that's the point: Stage 7
//! must never change a program's observable behavior. To confirm the JIT
//! path is the one that actually ran (rather than a silent fallback masking
//! a bug that would otherwise show up once Stage 9 makes JIT mandatory),
//! rerun this file with:
//!
//!   TYPELISP_CLOSURE_JIT=require scripts/with-llvm-env.sh cargo test --test closure_jit_test
//!
//! which turns every fallback into a hard `EvalError::Panic` instead.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, RtValue};

/// The compiler body must be loaded (`Interp::jit_define_closure` looks up
/// `compile-function` in `self.fns`) even though nothing here ever calls
/// `(compile ...)` — a definition-time JIT attempt drives the exact same
/// self-hosted `compile-function` entry point `(compile fn)` does, just for
/// a synthetic constructor rather than a user-named `defun`. The prelude is
/// loaded too since some of these programs use `dotimes`/`while`.
fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

#[test]
fn fnref_of_a_plain_function_is_callable() {
    let src = "(defun inc ((x i32)) i32 (+ x 1)) \
               (defun call2 ((f (fn (i32) i32))) i32 (f (f 0))) \
               (call2 inc)";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn methodref_of_a_defstruct_field_accessor_is_callable() {
    let src = "(defstruct point (x i64) (y i64)) \
               (defun make-point ((a i64) (b i64)) point (point::new a b)) \
               (defun call-getter ((f (fn (point) i64)) (p point)) i64 (f p)) \
               (call-getter x (make-point 7 1))";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn lambda_with_no_captures_is_callable() {
    let src = "(defun apply-twice ((f (fn (i32) i32)) (x i32)) i32 (f (f x))) \
               (apply-twice (lambda ((n i32)) i32 (+ n 1)) 5)";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn lambda_captures_an_outer_variable() {
    let src = "(defun adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n))) \
               (let ((add5 (adder 5))) (add5 10))";
    assert_eq!(eval_ok(src), RtValue::Int(15));
}

/// The `make-counter` idiom: a shared mutable capture, `setf` through the
/// closure visible on the *next* call through that same closure —
/// `compile_test.rs`'s `compile_a_setf_on_a_captured_name_...` precedent
/// (Stage 4), but reached through plain interpretation instead of an
/// explicit `(compile ...)` call.
#[test]
fn make_counter_shares_a_mutable_capture_across_calls() {
    let src = "(defun make-counter () (fn () i32) \
                 (let ((c 0)) (lambda () i32 (setf c (+ c 1))))) \
               (let ((next (make-counter))) (next) (next))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

/// Two closures from two separate `make-counter` calls must *not* share a
/// cell — each `(let ((c 0)) ...)` binds its own.
#[test]
fn two_counters_from_the_same_maker_have_independent_cells() {
    let src = "(defun make-counter () (fn () i32) \
                 (let ((c 0)) (lambda () i32 (setf c (+ c 1))))) \
               (let ((a (make-counter)) (b (make-counter))) \
                 (a) (a) (b) \
                 (+ (a) (b)))";
    assert_eq!(eval_ok(src), RtValue::Int(5));
}

/// `labels` mutual recursion, JIT'd one sibling at a time — each sibling
/// calling the other resolves through the shared placeholder cell
/// (`apply-indirect`), not a direct LLVM call, since Stage 7 never batches
/// siblings into one module the way `compiler.rs`'s own
/// `compile-labels-bodies` does.
#[test]
fn labels_mutual_recursion_via_independent_sibling_jit() {
    let src = "(labels ((is-even ((n i32)) bool (if (= n 0) true (is-odd (- n 1))))
                        (is-odd ((n i32)) bool (if (= n 0) false (is-even (- n 1)))))
                 (is-even 10))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

/// A capture shared between two `labels` siblings (not just a sibling and
/// its own recursive self) stays the *same* cell — `bump` mutates it,
/// `read-it` (which never itself writes) observes the mutation.
#[test]
fn setf_through_one_labels_sibling_is_visible_through_another_sharing_the_same_capture() {
    let src = "(defun make-pair ((start i64)) i64
                 (labels ((bump () i64 (setf start (+ start 1)))
                          (read-it () i64 start))
                   (let ((ignored1 (bump)))
                     (let ((ignored2 (bump)))
                       (read-it)))))
               (make-pair 10)";
    assert_eq!(eval_ok(src), RtValue::Int(12));
}

/// Regression for the macro-expansion JIT-suppression flag
/// (`Interp::jit_suppressed`): `dotimes`'s own expansion
/// (`prelude::SOURCE`) runs through `Interp::expand_macro` at *check* time,
/// and its use here also builds/calls a real closure at *run* time — this
/// must still produce the right answer (and, under
/// `TYPELISP_CLOSURE_JIT=require`, must not turn a suppressed
/// check-time JIT attempt into a spurious hard failure).
#[test]
fn a_macro_expansion_and_a_real_closure_coexist() {
    let src = "(defun make-counter () (fn () i32) \
                 (let ((c 0)) (lambda () i32 (setf c (+ c 1))))) \
               (let ((next (make-counter)) (total 0)) \
                 (dotimes (i 3) (setf total (+ total (next)))) \
                 total)";
    assert_eq!(eval_ok(src), RtValue::Int(6));
}
