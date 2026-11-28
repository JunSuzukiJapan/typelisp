//! Proves that a compile session's `Scope<V>` boxes survive collection.
//!
//! Scopes used to be Rust-native objects held alive by a handle registry; they
//! are ordinary heap boxes now. That moves them under the collector — and the
//! committed island bitcode (`src/compiler_island.bc`, which this phase must
//! not regenerate) was compiled when a scope word was an untraced integer, so
//! it emits no GC root for the scope locals it threads through
//! `compile-function`. `Heap::push_session_root` is what closes that gap:
//! `rt_llvm_call` roots every scope it hands out for the length of the compile
//! session (`compile::driver::run_compile_function`'s bracket).
//!
//! Under `set_gc_stress` every single `cons` collects, so a scope reachable
//! only from a compiled local is freed at the very next allocation — which,
//! during compilation, is essentially immediate. These tests would therefore
//! fail deterministically if the session rooting were missing or released too
//! early, rather than depending on the free list happening to run dry.

mod common;
use common::{Load, Session, eval_in_island as eval_in};
use typelisp::{Checker, Heap, Interp, Value};

fn stressed() -> (Heap, Checker, Interp) {
    Session::with_capacity(Load::Compiler, 1 << 18).stressed().into_parts()
}

/// The island's own scope constructors, driven through the compiled
/// `compile-function`. These are the functions whose return value *is* a
/// scope, so they exercise the session-rooted `LlvmRetK::Scope` path.
#[test]
fn compiling_the_islands_scope_constructors_survives_constant_collection() {
    let (mut h, mut chk, interp) = stressed();
    for name in ["new-env", "new-fn-env", "new-acc-table"] {
        eval_in(&mut h, &mut chk, &interp, &format!("(compile {})", name))
            .unwrap_or_else(|e| panic!("(compile {}) failed under gc stress: {:?}", name, e));
    }
}

/// A function whose compilation threads scopes through `env`/`fn-env` for
/// every nested binding — the case where a scope box lives across many
/// allocations inside one compile session.
#[test]
/// Spent a long time `#[ignore]`d: it tripped `attempt to subtract with
/// overflow` in `Heap::gc`'s reclaim count, which was blamed on the scope work
/// but reproduced at `f4b2480`, before scopes moved to the heap. The real cause
/// was a missing GC root in `ast_bridge::tagged_sym_list` — see that function.
fn compiling_a_nested_binding_function_survives_constant_collection() {
    let (mut h, mut chk, interp) = stressed();
    let src = "(defun deep ((a int) (b int)) int
                 (let ((x (+ a b)))
                   (let ((y (* x 2)))
                     (let ((z (- y a)))
                       (if (< z 0) (- 0 z) z)))))
               (compile deep)
               (deep 3 4)";
    let got = eval_in(&mut h, &mut chk, &interp, src).expect("compile+run failed under gc stress");
    assert_eq!(got, Value::Int(11));
}

/// `labels` is what pushes and clones scope frames most heavily
/// (`clone-frames` per sibling), so it is the densest exercise of the scope
/// surface within a single session.
#[test]
/// Spent a long time `#[ignore]`d: it tripped `attempt to subtract with
/// overflow` in `Heap::gc`'s reclaim count, which was blamed on the scope work
/// but reproduced at `f4b2480`, before scopes moved to the heap. The real cause
/// was a missing GC root in `ast_bridge::tagged_sym_list` — see that function.
fn compiling_labels_survives_constant_collection() {
    let (mut h, mut chk, interp) = stressed();
    let src = "(defun sum-to ((n int)) int
                 (labels ((rec ((i int) (acc int)) int
                            (if (> i n) acc (rec (+ i 1) (+ acc i)))))
                   (rec 1 0)))
               (compile sum-to)
               (sum-to 5)";
    let got = eval_in(&mut h, &mut chk, &interp, src).expect("compile+run failed under gc stress");
    assert_eq!(got, Value::Int(15));
}

/// A compiled function must still agree with the interpreter when every
/// allocation during its compilation collected.
#[test]
fn a_function_compiled_under_stress_agrees_with_the_interpreter() {
    let src = "(defun f ((n int)) int
                 (let ((a (* n n)))
                   (if (> a 10) (- a 10) (+ a 1))))";

    let (mut h, mut chk, interp) = stressed();
    eval_in(&mut h, &mut chk, &interp, src).expect("define failed");
    let interpreted: Vec<Value> = (0..6)
        .map(|n| eval_in(&mut h, &mut chk, &interp, &format!("(f {})", n)).expect("interpreted call failed"))
        .collect();

    eval_in(&mut h, &mut chk, &interp, "(compile f)").expect("compile failed under gc stress");
    let compiled: Vec<Value> = (0..6)
        .map(|n| eval_in(&mut h, &mut chk, &interp, &format!("(f {})", n)).expect("compiled call failed"))
        .collect();

    assert_eq!(compiled, interpreted);
}

/// Scope values in *interpreted* code are ordinary heap boxes with no session
/// rooting at all — the binding's own slot is what keeps them alive. Under
/// stress this checks that path independently of the compile session.
#[test]
fn interpreted_scopes_survive_constant_collection() {
    let (mut h, mut chk, interp) = stressed();
    let src = "(defun make-s () Scope<Option<Sexpr>> (Scope::new))
               (defun f () int
                 (let ((s (make-s)))
                   (set s \"a\" (quote 42))
                   (push-frame s)
                   (set s \"b\" (quote 7))
                   (pop-frame s)
                   (match (get s \"a\")
                     ((Some v) (sexpr-int v))
                     ((None) -1))))
               (f)";
    let got = eval_in(&mut h, &mut chk, &interp, src).expect("interpreted scope failed under gc stress");
    assert_eq!(got, Value::Int(42));
}
