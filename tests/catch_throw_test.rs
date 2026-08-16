//! `catch`/`throw`/`unwind-protect` — the non-local exits.
//!
//! These are the *dynamic* exits: where a `break`/`return` goes is settled by
//! the checker (the nearest enclosing `loop`, never across a function
//! boundary), while a throw finds its catch at run time, by tag, however many
//! frames up. The two mechanisms are deliberately kept apart — see the
//! `compile-catch` family in `src/compiler.rs` and the catch/throw section of
//! `typelisp-rt` — and a `break` leaving a `catch` body, which mixes them, has
//! its own test below for exactly that reason.

extern crate typelisp;

use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

/// Runs `src` and returns the last value, or the error that stopped it.
fn run(src: &str) -> Result<Value, EvalError> {
    run_with(src, false)
}

/// [`run`], with the compiler loaded so `(compile f)` in the source really
/// compiles — the same program, taken down the other path.
fn run_compiled(src: &str) -> Result<Value, EvalError> {
    run_with(src, true)
}

fn run_with(src: &str, compiler: bool) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    if compiler {
        load_compiler(&mut h, &mut chk, &mut interp);
    }
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

/// The message from checking `src`, which is expected to fail.
fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                interp.exec(&mut h, tl).expect("eval failed");
            }
            Err(e) => return e.to_string(),
        }
    }
    panic!("expected a check failure, but everything checked")
}

fn int(src: &str) -> i64 {
    match run(src).expect("eval failed") {
        Value::Int(n) => n,
        other => panic!("expected an Int, got {:?}", other),
    }
}

fn int_compiled(src: &str) -> i64 {
    match run_compiled(src).expect("eval failed") {
        Value::Int(n) => n,
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A compiled run whose result is read back *while its heap is still alive* —
/// a `Sexpr` result is a reference into that heap and means nothing once it is
/// dropped.
fn read_compiled(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    typelisp::check::core::print(&h, last)
}

/// A `catch` whose body finishes normally is just its body.
#[test]
fn a_body_that_does_not_throw_is_the_catch_s_value() {
    assert_eq!(int("(catch 'done (+ 1 2))"), 3);
}

/// The throw takes the catch's place, discarding the rest of the body.
#[test]
fn a_throw_replaces_the_rest_of_the_body() {
    assert_eq!(int("(catch 'done (+ 1 (throw 'done 41)))"), 41);
}

/// The point of the whole mechanism: the throw is raised in a function that
/// has no idea a catch exists, and the frames in between simply vanish.
#[test]
fn a_throw_crosses_function_boundaries() {
    assert_eq!(
        int(r#"
        (defun deep ((n i32)) i32 (throw 'done (* n 2)))
        (defun middle ((n i32)) i32 (+ 1 (deep n)))
        (catch 'done (middle 21))
        "#),
        42
    );
}

/// A tag that is not this catch's belongs to an outer one. Swallowing it here
/// is the bug CL's tag comparison exists to prevent.
#[test]
fn an_inner_catch_passes_on_a_tag_that_is_not_its_own() {
    assert_eq!(int("(catch 'outer (catch 'inner (throw 'outer 7)))"), 7);
}

/// With no enclosing catch at all, the throw is an ordinary reportable error
/// rather than a crash.
#[test]
fn an_uncaught_throw_is_an_error() {
    match run("(throw 'nobody 1)") {
        Err(EvalError::Throw(tag, _)) => assert_eq!(tag, "nobody"),
        other => panic!("expected an uncaught throw, got {:?}", other),
    }
}

/// Every symbol carries one type, decided by its first use and checked against
/// every later one — the tag is the only thing a throw and its catch share, so
/// it is where the type has to live.
#[test]
fn one_tag_cannot_carry_two_types() {
    let err = check_err(r#"
        (defun a () i32 (throw 'both 1))
        (defun b () bool (throw 'both true))
        "#);
    // The second use is checked *against* what the first established, so the
    // mismatch is reported at the offending value rather than after the fact.
    assert!(err.contains("expected I32, found Bool"), "unexpected error: {}", err);
}

/// `unwind-protect` on the ordinary path: the cleanup runs after the protected
/// form, and the protected form's value is what the whole thing produces.
#[test]
fn a_cleanup_runs_on_the_normal_path() {
    assert_eq!(
        int(r#"
        (defvar (ran i32) 0)
        (let ((v (unwind-protect 7 (setf ran 1))))
          (+ v ran))
        "#),
        8
    );
}

/// The reason `unwind-protect` exists: the cleanup runs even though the
/// protected form left by a throw, and the throw still arrives.
#[test]
fn a_cleanup_runs_while_a_throw_passes_through() {
    assert_eq!(
        int(r#"
        (defvar (ran i32) 0)
        (let ((v (catch 'done (unwind-protect (throw 'done 40) (setf ran 2)))))
          (+ v ran))
        "#),
        42
    );
}

/// The cleanup runs for a `panic` too — the other dynamic exit.
#[test]
fn a_cleanup_runs_while_a_panic_passes_through() {
    match run(r#"
        (defvar (ran i32) 0)
        (unwind-protect (panic "boom") (setf ran 1))
        "#) {
        Err(EvalError::Panic(msg)) => assert_eq!(msg, "boom"),
        other => panic!("expected the panic to keep going, got {:?}", other),
    }
}

/// Cleanups nest outward, innermost first, and every one of them runs.
#[test]
fn nested_cleanups_all_run() {
    assert_eq!(
        int(r#"
        (defvar (trace i32) 0)
        (let ((v (catch 'done
                   (unwind-protect
                     (unwind-protect (throw 'done 0) (setf trace (+ (* trace 10) 1)))
                     (setf trace (+ (* trace 10) 2))))))
          (+ v trace))
        "#),
        12
    );
}

/// A `break` inside a `catch` body is a *static* exit: it belongs to the
/// enclosing `loop`, not to the catch, and passes straight through. This is
/// the case that decided how the compiled side is built — the catch body stays
/// in the same function precisely so this stays an ordinary branch there too.
#[test]
fn a_break_leaves_a_catch_body_for_its_loop() {
    assert_eq!(
        int(r#"
        (defvar (n i32) 0)
        (loop
          (setf n (+ n 1))
          (catch 'done (if (> n 3) (break) ()))
          ())
        n
        "#),
        4
    );
}

/// The same for `return`, which carries a value out of the loop.
#[test]
fn a_return_leaves_a_catch_body_with_its_value() {
    assert_eq!(int("(loop (catch 'done (return 5)))"), 5);
}

/// A `return` crossing an `unwind-protect` still runs the cleanup: the
/// protected form left, and how it left is not the cleanup's business.
#[test]
fn a_cleanup_runs_when_a_return_leaves_the_loop() {
    assert_eq!(
        int(r#"
        (defvar (ran i32) 0)
        (let ((v (loop (unwind-protect (return 40) (setf ran 2)))))
          (+ v ran))
        "#),
        42
    );
}

// ---- the compiled path ---------------------------------------------------
//
// Everything above again, but with the functions actually compiled. Each of
// these is a `(compile f)` away from its interpreted twin, and asserts the
// same answer: the whole point of the mechanism is that a throw does not care
// which side of the boundary its catch is on.

/// `catch`/`throw` inside one compiled function: the unwind never leaves the
/// frame, and the dispatch block claims it.
#[test]
fn compiled_catch_claims_a_throw_in_its_own_body() {
    assert_eq!(
        int_compiled(
            r#"
        (defun f () i32 (catch 'done (+ 1 (throw 'done 41))))
        (compile f)
        (f)
        "#
        ),
        41
    );
}

/// The compiled version of the mechanism's whole point: the throw is raised in
/// a compiled function that knows nothing about the catch, unwinds through a
/// compiled frame in between, and lands in a compiled catch.
#[test]
fn a_throw_crosses_compiled_frames() {
    assert_eq!(
        int_compiled(
            r#"
        (defun deep ((n i32)) i32 (throw 'done (* n 2)))
        (defun middle ((n i32)) i32 (+ 1 (deep n)))
        (defun outer ((n i32)) i32 (catch 'done (middle n)))
        (compile outer)
        (outer 21)
        "#
        ),
        42
    );
}

/// A tag that is not this region's is handed to the enclosing one — the
/// dispatch blocks are chained, so this is settled inside the single compiled
/// frame that holds both catches.
#[test]
fn compiled_nested_catches_chain_their_dispatch() {
    assert_eq!(
        int_compiled(
            r#"
        (defun f () i32 (catch 'outer (catch 'inner (throw 'outer 7))))
        (compile f)
        (f)
        "#
        ),
        7
    );
}

/// A throw out of a compiled function with no compiled catch above it reaches
/// the interpreter as the same `EvalError::Throw` an interpreted throw
/// produces — so an interpreted `catch` claims it.
#[test]
fn a_compiled_throw_reaches_an_interpreted_catch() {
    assert_eq!(
        match run_compiled(
            r#"
        (defun thrower ((n i32)) i32 (throw 'done (* n 2)))
        (compile thrower)
        (catch 'done (thrower 21))
        "#
        )
        .expect("eval failed")
        {
            Value::Int(n) => n,
            other => panic!("expected an Int, got {:?}", other),
        },
        42
    );
}

/// And uncaught anywhere, it is still an ordinary reportable error rather than
/// an abort.
#[test]
fn an_uncaught_compiled_throw_is_an_error() {
    match run_compiled(
        r#"
        (defun thrower () i32 (throw 'nobody 1))
        (compile thrower)
        (thrower)
        "#,
    ) {
        Err(EvalError::Throw(tag, _)) => assert_eq!(tag, "nobody"),
        other => panic!("expected an uncaught throw, got {:?}", other),
    }
}

/// A compiled `unwind-protect` runs its cleanup while a throw passes through,
/// and the throw still arrives.
#[test]
fn a_compiled_cleanup_runs_while_a_throw_passes_through() {
    assert_eq!(
        int_compiled(
            r#"
        (defvar (ran i32) 0)
        (defun f () i32 (catch 'done (unwind-protect (throw 'done 40) (setf ran 2))))
        (compile f)
        (+ (f) ran)
        "#
        ),
        42
    );
}

/// ...and on the ordinary path too, where no unwind happens at all.
#[test]
fn a_compiled_cleanup_runs_on_the_normal_path() {
    assert_eq!(
        int_compiled(
            r#"
        (defvar (ran i32) 0)
        (defun f () i32 (unwind-protect 7 (setf ran 1)))
        (compile f)
        (+ (f) ran)
        "#
        ),
        8
    );
}

/// The case that shaped the whole design: a `break` inside a compiled `catch`
/// body is a static exit and stays an ordinary branch to the loop's exit
/// block. If the catch body were outlined into its own function — the
/// alternative that was rejected — this would not compile at all.
#[test]
fn a_break_leaves_a_compiled_catch_body_for_its_loop() {
    assert_eq!(
        int_compiled(
            r#"
        (defun f () i32
          (let ((n 0))
            (loop
              (setf n (+ n 1))
              (catch 'done (if (> n 3) (break) ()))
              ())
            n))
        (compile f)
        (f)
        "#
        ),
        4
    );
}

/// The same for `return`, which carries its value out past the region.
#[test]
fn a_return_leaves_a_compiled_catch_body_with_its_value() {
    assert_eq!(
        int_compiled(
            r#"
        (defun f () i32 (loop (catch 'done (return 5))))
        (compile f)
        (f)
        "#
        ),
        5
    );
}

/// A thrown `Sexpr` — a heap value, not a raw word — survives the trip. The
/// representation baked into the node is what tells the two sides whether the
/// machine word they are passing is a tagged reference or a bare integer.
#[test]
fn a_compiled_throw_carries_a_heap_value() {
    let out = read_compiled(
        r#"
        (defun thrower () string (throw 'text (append "car" "ried")))
        (defun f () string (catch 'text (thrower)))
        (compile f)
        (f)
        "#,
    );
    assert_eq!(out, "\"carried\"");
}

/// The thrown value survives the cleanup that runs while it is in flight.
///
/// The value travels in a Rust `Box` the collector cannot see, and a cleanup —
/// unlike every other unwind in the evaluator — runs code, and that code
/// allocates. Under `gc-stress` every allocation collects, so an unrooted
/// value in flight is reclaimed here and the assertion sees garbage.
#[test]
fn a_thrown_value_survives_a_cleanup_that_allocates() {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    // Only now: checking the prelude's own quasiquote-heavy macros has a root
    // leak of its own under stress, which has nothing to do with what this
    // test is about and would fire first.
    h.set_gc_stress(true);
    let r = Reader::new();
    let src = r#"
        (defun thrower () string (throw 'text (append "in" "flight")))
        (catch 'text (unwind-protect (thrower) (cons 3 (cons 4 ()))))
        "#;
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    assert_eq!(typelisp::check::core::print(&h, last), "\"inflight\"");
}

// ------------------------------------------ static exits through a cleanup

/// Compiles `setup` with the collector quiet, then runs `expr` with
/// `gc-stress` on, so every allocation the *compiled* code makes collects.
///
/// Compiling under stress is not an option: the island is a large typelisp
/// program, and checking it trips the prelude's own quasiquote root leak
/// (`check_qq_template` → `construct_form`) long before reaching anything
/// these tests are about.
fn read_compiled_under_stress(setup: &str, expr: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    for v in r.read_all(&mut h, setup).expect("read failed") {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        interp.exec(&mut h, tl).expect("eval failed");
    }
    h.set_gc_stress(true);
    let mut last = Value::Empty;
    for v in r.read_all(&mut h, expr).expect("read failed") {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    typelisp::check::core::print(&h, last)
}

/// [`read_compiled`]'s interpreted twin, for results that are heap values.
fn read_interpreted(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let mut last = Value::Empty;
    for v in r.read_all(&mut h, src).expect("read failed") {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    typelisp::check::core::print(&h, last)
}

const RETURN_THROUGH_CLEANUP: &str = r#"
    (defvar (ran i32) 0)
    (defun body () i32 (loop (unwind-protect (return 40) (setf ran 2))))
    (defun f () i32 (+ (body) ran))
    "#;

/// A `return` leaving a protected form runs the cleanup on its way out.
#[test]
fn a_return_runs_an_enclosing_cleanup() {
    assert_eq!(int(&format!("{RETURN_THROUGH_CLEANUP} (f)")), 42);
}

#[test]
fn a_return_runs_an_enclosing_compiled_cleanup() {
    assert_eq!(int_compiled(&format!("{RETURN_THROUGH_CLEANUP} (compile f) (f)")), 42);
}

const BREAK_THROUGH_CLEANUP: &str = r#"
    (defvar (ran i32) 0)
    (defun body () i32 (progn (loop (unwind-protect (break) (setf ran 2))) ran))
    (defun f () i32 (body))
    "#;

/// Likewise for `break`, whose own value is `Unit` — the cleanup still runs.
#[test]
fn a_break_runs_an_enclosing_cleanup() {
    assert_eq!(int(&format!("{BREAK_THROUGH_CLEANUP} (f)")), 2);
}

#[test]
fn a_break_runs_an_enclosing_compiled_cleanup() {
    assert_eq!(int_compiled(&format!("{BREAK_THROUGH_CLEANUP} (compile f) (f)")), 2);
}

const NESTED_CLEANUPS_ON_RETURN: &str = r#"
    (defvar (log string) "")
    (defun body () i32
      (loop (unwind-protect
              (unwind-protect (return 7) (setf log (append log "in")))
              (setf log (append log "out")))))
    (defun f () string (progn (body) log))
    "#;

/// Nested cleanups all run, innermost first — the same order the interpreted
/// side produces, and the reason the exit walks a chain rather than jumping to
/// one block.
#[test]
fn nested_cleanups_all_run_on_a_return() {
    assert_eq!(read_interpreted(&format!("{NESTED_CLEANUPS_ON_RETURN} (f)")), "\"inout\"");
}

#[test]
fn nested_compiled_cleanups_all_run_on_a_return() {
    assert_eq!(
        read_compiled(&format!("{NESTED_CLEANUPS_ON_RETURN} (compile f) (f)")),
        "\"inout\""
    );
}

/// The returned value is a heap value that sits in the loop's result slot —
/// a plain `alloca`, not a GC root — while the cleanup runs, and the cleanup
/// allocates. Under `gc-stress` an unrooted value there is reclaimed.
#[test]
fn a_returned_value_survives_a_compiled_cleanup_that_allocates() {
    assert_eq!(
        read_compiled_under_stress(
            r#"
            (defun body () string
              (loop (unwind-protect (return (append "sur" "vives")) (cons 3 (cons 4 ())))))
            (defun f () string (body))
            (compile f)
            "#,
            "(f)"
        ),
        "\"survives\""
    );
}

/// The same question for the path that already worked: the protected form's
/// own value also waits in a slot while the cleanup allocates.
#[test]
fn a_protected_form_s_value_survives_a_compiled_cleanup_that_allocates() {
    assert_eq!(
        read_compiled_under_stress(
            r#"
            (defun body () string (unwind-protect (append "nor" "mal") (cons 3 (cons 4 ()))))
            (defun f () string (body))
            (compile f)
            "#,
            "(f)"
        ),
        "\"normal\""
    );
}

const INNER_LOOP_BREAK: &str = r#"
    (defvar (log string) "")
    (defun body () i32
      (loop
        (unwind-protect
          (progn (loop (break)) (setf log (append log "after")))
          (setf log (append log "|c")))
        (return 5)))
    (defun f () i32 (body))
    "#;

/// A `break` bound to a loop *inside* the protected form is none of the
/// enclosing `unwind-protect`'s business: it leaves that inner loop and the
/// protected form carries on. Only an exit that actually leaves the protected
/// form runs the cleanup.
///
/// This is what the reset at each `loop` boundary buys. Without it the inner
/// `break` would branch to the cleanup block and then out of the *outer* loop,
/// skipping the rest of the protected form entirely — the function would
/// answer 0 and the log would never see "after".
#[test]
fn an_inner_loops_break_does_not_run_an_enclosing_cleanup() {
    assert_eq!(int(&format!("{INNER_LOOP_BREAK} (f)")), 5);
    assert_eq!(read_interpreted(&format!("{INNER_LOOP_BREAK} (f) log")), "\"after|c\"");
}

#[test]
fn an_inner_loops_break_does_not_run_an_enclosing_compiled_cleanup() {
    assert_eq!(int_compiled(&format!("{INNER_LOOP_BREAK} (compile f) (f)")), 5);
    assert_eq!(
        read_compiled(&format!("{INNER_LOOP_BREAK} (compile f) (f) log")),
        "\"after|c\""
    );
}

const BREAK_THROUGH_CATCH_AND_CLEANUP: &str = r#"
    (defvar (ran i32) 0)
    (defun body () i32
      (progn (loop (catch 'tag (unwind-protect (break) (setf ran 2)))) ran))
    (defun f () i32 (body))
    "#;

/// A `catch` sitting between the `break` and its loop contributes nothing —
/// it has no cleanup to run — while the `unwind-protect` inside it still does.
/// The two chains are walked independently, which is the whole point of
/// keeping them apart.
#[test]
fn a_break_passes_through_a_catch_but_runs_a_cleanup() {
    assert_eq!(int(&format!("{BREAK_THROUGH_CATCH_AND_CLEANUP} (f)")), 2);
}

#[test]
fn a_compiled_break_passes_through_a_catch_but_runs_a_cleanup() {
    assert_eq!(
        int_compiled(&format!("{BREAK_THROUGH_CATCH_AND_CLEANUP} (compile f) (f)")),
        2
    );
}
