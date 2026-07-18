//! Closure unification Stage 7 (definition-time JIT) tests: every closure
//! value here (`lambda`/`labels` sibling/reified named function) is
//! evaluated by *pure interpretation* — no `(compile ...)` call anywhere in
//! any of these programs — so a passing test exercises
//! `Interp::jit_define_closure`'s attempt-first-fall-back-second path, not
//! the pre-existing `(compile fn)`-triggered pipeline `compile_test.rs`
//! covers.
//!
//! Every assertion here is correct whether the JIT attempt actually
//! succeeds or falls back to `Interp::make_closure` — that's the point:
//! definition-time JIT must never change a program's observable behavior.
//! Since Stage 9 ("JIT必須化"), a *coverage-gap* fallback (`JitDecline::Gap`)
//! is always a hard `EvalError::Panic` — and interp-closure removal Stage 7
//! narrowed the permanent `JitDecline::Benign` set to just native-tier types
//! and "compiler island not loaded" (both only reachable via the interpreted
//! island `load_compiler` these tests use, or an island-less embedder;
//! `TYPELISP_CLOSURE_JIT=off`, macro-expansion suppression, and JIT-time
//! heap exhaustion were all retired in Stages 6–7). So a passing test here
//! already confirms the JIT path is either the one that ran or one of those
//! two permanently-allowed exceptions.

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

/// A `labels` sibling whose own parameter *shadows* a name the block
/// captures must read the parameter, not the capture (interp-closure removal
/// Stage 4). `use-cap` genuinely captures the enclosing `n`; `shadow-it`'s
/// own `n` parameter collides with that captured name, and lexical scoping
/// requires the parameter to win. This is the exact collision the
/// self-hosted island hit compiling itself (`resolve-value`'s `name`
/// parameter vs. the captured `name` `declare-labels-siblings` needs), which
/// surfaced that `bind-captures` was overwriting `bind-params` in a
/// sibling's `env`; the fix binds captures first so parameters shadow them.
#[test]
fn a_labels_param_shadows_a_captured_name_of_the_same_spelling() {
    let src = "(defun outer ((n i64)) i64
                 (labels ((use-cap ((x i64)) i64 (+ x n))
                          (shadow-it ((n i64)) i64 n))
                   (+ (use-cap 1) (shadow-it 100))))
               (outer 5)";
    // use-cap: 1 + captured n(5) = 6; shadow-it: param n(100) = 100; sum 106.
    // (Before the fix shadow-it read the captured n(5), giving 11.)
    assert_eq!(eval_ok(src), RtValue::Int(106));
}

/// A `Unit`-returning closure JITs too (closure unification Stage 8):
/// `compile-unit` encodes the body's `Unit` tail as a plain `0`, and
/// `Interp::decode_compiled_return`'s `Type::Unit` arm decodes it back to a
/// real `RtValue::Unit` — the observable effect (the `setf` through the
/// capture) plus the `Unit` result must both come through the compiled
/// boundary intact.
#[test]
fn a_unit_returning_closure_performs_its_effect_and_returns_unit() {
    let src = "(defun run-thunk ((f (fn () ()))) () (f)) \
               (let ((hits 0)) \
                 (run-thunk (lambda () () (setf hits (+ hits 1)) ())) \
                 (run-thunk (lambda () () (setf hits (+ hits 1)) ())) \
                 hits)";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

/// A variadic (`&rest`) lambda JITs like any other (closure unification
/// Stage 8, retiring Stage 7's blanket `&rest` rejection): the checker
/// folded `xs` into the lambda's params as an ordinary `Sexpr`, and this
/// call site packed `2 3 4` into one list at check time — so the compiled
/// closure sees a fixed 2-argument call.
#[test]
fn a_variadic_lambda_jits_and_collects_its_rest_list() {
    let src = "(defun sexpr-len ((s Sexpr)) i64 (if (sexpr-consp s) (+ (the i64 1) (sexpr-len (sexpr-cdr s))) (the i64 0))) \
               ((lambda ((a i64) &rest (xs i64)) i64 (+ a (sexpr-len xs))) 1 2 3)";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

/// `FnRef` of a variadic named function — the reified forwarding closure
/// (`translate_fnref`) must declare and forward the rest parameter too
/// (the 2026-07-16 `fnref_of_a_variadic_function_forwards_the_rest_list`
/// fix), now reached through definition-time JIT instead of `(compile ...)`.
#[test]
fn fnref_of_a_variadic_function_jits_and_forwards_the_rest_list() {
    let src = "(defun sexpr-len ((s Sexpr)) i64 (if (sexpr-consp s) (+ (the i64 1) (sexpr-len (sexpr-cdr s))) (the i64 0))) \
               (defun count-extra ((base i64) &rest (xs i64)) i64 (+ base (sexpr-len xs))) \
               (defun use-it ((f (fn (i64 &rest i64) i64))) i64 (f 10 1 2 3)) \
               (use-it count-extra)";
    assert_eq!(eval_ok(src), RtValue::Int(13));
}

/// Regression for the macro-expansion JIT-suppression flag
/// (`Interp::jit_suppressed`): `dotimes`'s own expansion
/// (`prelude::SOURCE`) runs through `Interp::expand_macro` at *check* time,
/// and its use here also builds/calls a real closure at *run* time — this
/// must still produce the right answer, and (since Stage 9 makes a
/// coverage-gap decline a hard failure unconditionally) must not turn a
/// suppressed check-time JIT attempt into a spurious one.
#[test]
fn a_macro_expansion_and_a_real_closure_coexist() {
    let src = "(defun make-counter () (fn () i32) \
                 (let ((c 0)) (lambda () i32 (setf c (+ c 1))))) \
               (let ((next (make-counter)) (total 0)) \
                 (dotimes (i 3) (setf total (+ total (next)))) \
                 total)";
    assert_eq!(eval_ok(src), RtValue::Int(6));
}

/// interp-closure removal Stage 6 (retiring `jit_suppressed`): a closure the
/// macro *body itself* builds and calls *while it expands* — at check time —
/// now goes through definition-time JIT like any other, instead of the
/// blanket `make_closure` fallback the suppression guard used to force. `mk`
/// is a genuine capturing closure (it closes over `op`) constructed inside
/// `expand_macro`; the macro calls it (`(mk (quote (1 2)))` builds the sexpr
/// `(+ 1 2)`) and returns that, so `(plus-list)` expands to `(+ 1 2)` = 3. A
/// passing result confirms the macro-body closure JIT'd (the island is loaded)
/// without a spurious coverage-gap panic now that suppression is gone.
#[test]
fn a_closure_built_inside_a_macro_body_jits_during_expansion() {
    let src = "(defmacro plus-list () \
                 (let ((op (quote +))) \
                   (let ((mk (lambda ((s Sexpr)) Sexpr (sexpr-cons op s)))) \
                     (mk (quote (1 2)))))) \
               (plus-list)";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}
