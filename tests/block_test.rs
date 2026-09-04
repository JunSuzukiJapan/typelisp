//! `block` / `return-from` — CL's *lexical* named escape
//! ([cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 4a's main item).
//!
//! The distinction that shapes every test here: a `block` name is resolved
//! **where it is written**. The checker matches each `return-from` to an
//! enclosing frame and joins its value's type into it, so a `return-from` with
//! no matching block, or with a value of the wrong type, is a *type* error and
//! not a runtime one. `catch`/`throw` are the dynamic counterpart and keep
//! their tag as a value precisely because they cannot do this.
//!
//! The other half is CL's implicit block: every `defun`, `defmethod` and
//! `labels` function establishes one named after itself, which is what makes
//! `(return-from name value)` an early return.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<(Heap, Value), EvalError> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok((h, last))
}

/// The `i32` a program answers with.
fn int(src: &str) -> i64 {
    match run(src).expect("eval failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

/// The string a program answers with.
fn text(src: &str) -> String {
    let (h, v) = run(src).expect("eval failed");
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// The same program with the compiler loaded, so a `(compile f)` in the source
/// really compiles `f` — the island's own lowering of `block`/`return-from`
/// rather than the evaluator's.
fn run_compiled(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
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

fn int_compiled(src: &str) -> i64 {
    match run_compiled(src).expect("eval failed") {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

/// The checker's complaint about a program that must not type-check.
fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 18);
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
    panic!("expected a type error, but the program checked");
}

// ---------------------------------------------------------------- the form

#[test]
fn a_block_answers_with_its_last_form_when_nothing_returns() {
    assert_eq!(int("(block b 1 2 3)"), 3);
}

#[test]
fn return_from_leaves_the_block_with_its_value() {
    assert_eq!(int("(block b 1 (return-from b 2) 3)"), 2);
}

#[test]
fn return_from_skips_the_rest_of_the_body() {
    // The `setf` after the escape must not run, so the counter stays at 1.
    assert_eq!(
        int(
            "(defvar (n i32) 0)
             (block b (setf n (+ n 1)) (return-from b 0) (setf n (+ n 100)))
             n"
        ),
        1
    );
}

#[test]
fn a_block_that_nobody_returns_from_still_scopes_its_name() {
    // Two sibling blocks may share a name: neither is inside the other.
    assert_eq!(int("(+ (block b (return-from b 1)) (block b (return-from b 2)))"), 3);
}

#[test]
fn the_innermost_block_of_a_name_wins() {
    // CL's shadowing rule. The inner `b` catches, so the outer one falls
    // through to its own last form.
    assert_eq!(int("(block b (+ (block b (return-from b 1)) 10))"), 11);
}

#[test]
fn return_from_can_leave_an_outer_block_from_inside_an_inner_one() {
    assert_eq!(int("(block outer (+ (block inner (return-from outer 7)) 10))"), 7);
}

// ------------------------------------------------- the implicit block

#[test]
fn defun_establishes_a_block_named_after_itself() {
    assert_eq!(
        int(
            "(defun first-even ((a i32) (b i32)) i32
               (if (= (mod a 2) 0) (return-from first-even a) ())
               (if (= (mod b 2) 0) (return-from first-even b) ())
               -1)
             ;; 8 from the second argument, -1 when neither is even.
             (+ (first-even 3 8) (first-even 3 5))"
        ),
        7
    );
}

#[test]
fn a_defun_that_never_names_itself_is_unchanged() {
    assert_eq!(int("(defun twice ((n i32)) i32 (* n 2)) (twice 21)"), 42);
}

#[test]
fn return_from_with_no_value_leaves_a_unit_function() {
    assert_eq!(
        int(
            "(defvar (n i32) 0)
             (defun bump ((stop bool)) ()
               (if stop (return-from bump) ())
               (setf n 5)
               ())
             (bump true)
             (bump false)
             n"
        ),
        5
    );
}

#[test]
fn labels_functions_get_their_own_implicit_block() {
    assert_eq!(
        int(
            "(defun outer () i32
               (labels ((inner ((n i32)) i32 (if (> n 0) (return-from inner 1) ()) 0))
                 (+ (inner 5) (inner -5))))
             (outer)"
        ),
        1
    );
}

#[test]
fn a_method_body_can_return_from_itself() {
    assert_eq!(
        text(
            "(defstruct point (x i32) (y i32))
             (defmethod describe ((self point)) string
               (if (= self::x 0) (return-from describe \"on the axis\") ())
               \"off the axis\")
             (format false \"~a/~a\" (describe (point::new 0 3)) (describe (point::new 1 3)))"
        ),
        "on the axis/off the axis"
    );
}

// --------------------------------------------- against the other escapes

#[test]
fn return_from_leaves_the_block_not_the_enclosing_loop() {
    // The `block` is inside the loop, so the escape lands in the loop body and
    // the loop keeps going — the counter reaches 3.
    assert_eq!(
        int(
            "(defvar (n i32) 0)
             (dotimes (i 3)
               (block b (return-from b ()))
               (setf n (+ n 1)))
             n"
        ),
        3
    );
}

#[test]
fn break_still_leaves_the_loop_from_inside_a_block() {
    assert_eq!(
        int(
            "(defvar (n i32) 0)
             (dotimes (i 5)
               (setf n (+ n 1))
               (if (= n 2) (break) ()))
             n"
        ),
        2
    );
}

#[test]
fn an_unwind_protect_cleanup_runs_when_a_return_from_passes_through() {
    assert_eq!(
        text(
            "(defvar (trail string) \"\")
             (defun escape () i32
               (unwind-protect (return-from escape 1)
                 (setf trail (append trail \"cleaned\"))))
             (format false \"~a:~a\" (escape) trail)"
        ),
        "1:cleaned"
    );
}

// ---------------------------------------------------------- what it rejects

#[test]
fn return_from_an_unknown_block_is_a_type_error() {
    let msg = check_err("(block b (return-from nowhere 1))");
    assert!(msg.contains("no enclosing block named `nowhere`"), "{}", msg);
}

#[test]
fn return_from_disagreeing_with_the_blocks_other_exits_is_a_type_error() {
    let msg = check_err("(block b (if true (return-from b 1) (return-from b \"two\")))");
    assert!(msg.to_lowercase().contains("expected") || msg.contains("type"), "{}", msg);
}

#[test]
fn return_from_cannot_cross_a_lambda_boundary() {
    // A `lambda` may outlive the block, so the escape target may be gone by
    // the time it runs. CL leaves that undefined; here it does not check.
    let msg = check_err(
        "(defun run-it ((f (fn () i32))) i32 (f))
         (defun outer () i32 (block b (run-it (lambda () i32 (return-from b 1)))))",
    );
    assert!(msg.contains("no enclosing block named `b`"), "{}", msg);
}

#[test]
fn a_block_name_must_be_a_bare_symbol() {
    let msg = check_err("(block \"b\" 1)");
    assert!(msg.contains("must be a bare symbol"), "{}", msg);
}

#[test]
fn a_defuns_implicit_block_does_not_escape_into_its_caller() {
    let msg = check_err(
        "(defun callee () i32 1)
         (defun caller () i32 (return-from callee 2))",
    );
    assert!(msg.contains("no enclosing block named `callee`"), "{}", msg);
}


// ------------------------------------------------------ the compiled path
//
// Each of these is a program the interpreter above already answers, run again
// with the island doing the lowering. `block` is a *static* escape there — a
// branch to a basic block known at compile time, the same machinery `break`
// uses — never `catch`/`throw`, which is the dynamic one
// (language-design.md §7.5: the two are not mixed).

const EARLY_RETURN: &str = r#"
    (defun first-even ((a i32) (b i32)) i32
      (if (= (mod a 2) 0) (return-from first-even a) ())
      (if (= (mod b 2) 0) (return-from first-even b) ())
      -1)
    "#;

#[test]
fn a_compiled_defun_returns_from_itself() {
    assert_eq!(
        int_compiled(&format!("{EARLY_RETURN} (compile first-even) (+ (first-even 3 8) (first-even 3 5))")),
        7
    );
}

const NESTED_BLOCKS: &str = r#"
    (defun pick () i32 (block outer (+ (block inner (return-from outer 7)) 10)))
    "#;

#[test]
fn a_compiled_return_from_leaves_the_outer_block() {
    assert_eq!(int_compiled(&format!("{NESTED_BLOCKS} (compile pick) (pick)")), 7);
}

#[test]
fn a_compiled_block_answers_with_its_last_form() {
    assert_eq!(
        int_compiled("(defun plain () i32 (block b 1 2 3)) (compile plain) (plain)"),
        3
    );
}

const BLOCK_IN_LOOP: &str = r#"
    (defvar (n i32) 0)
    (defun count-up () i32
      (dotimes (i 3)
        (block b (return-from b ()))
        (setf n (+ n 1)))
      n)
    "#;

/// The escape lands in the loop body, so the loop keeps going — the two
/// mechanisms do not intercept each other's exits.
#[test]
fn a_compiled_return_from_does_not_leave_the_enclosing_loop() {
    assert_eq!(int_compiled(&format!("{BLOCK_IN_LOOP} (compile count-up) (count-up)")), 3);
}

const RETURN_FROM_THROUGH_CLEANUP: &str = r#"
    (defvar (ran i32) 0)
    (defun escape () i32
      (unwind-protect (return-from escape 1) (setf ran 2))
      0)
    (defun f () i32 (progn (escape) ran))
    "#;

/// The one that decides the design: a `return-from` that leaves an
/// `unwind-protect` has to run the cleanup, exactly as `break`/`return` do
/// (`catch_throw_test`'s `a_break_runs_an_enclosing_compiled_cleanup`). The
/// island gets there by giving each enclosing block its own copy of the
/// cleanup block, since each resumes somewhere different.
#[test]
fn a_return_from_runs_an_enclosing_cleanup() {
    assert_eq!(int(&format!("{RETURN_FROM_THROUGH_CLEANUP} (f)")), 2);
}

#[test]
fn a_return_from_runs_an_enclosing_compiled_cleanup() {
    assert_eq!(
        int_compiled(&format!("{RETURN_FROM_THROUGH_CLEANUP} (compile escape) (compile f) (f)")),
        2
    );
}


/// A heap value carried out of a `return-from` that crosses a cleanup, with
/// the collector running at every allocation.
///
/// The value sits in the block's merge slot — an `alloca`, not a GC root —
/// while the cleanup runs, and the cleanup allocates. `break`/`return` have the
/// same shape through `xexit`, so this is as much a check of the existing
/// machinery as of the new lowering; it is here because this lowering is what
/// made the question worth asking.
#[test]
fn a_return_from_carries_a_heap_value_past_an_allocating_cleanup() {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    h.set_gc_stress(true);
    let src = r#"
        (defvar (trail string) "")
        (defun escape () string
          (unwind-protect (return-from escape (append "a" "b"))
            (setf trail (append trail "c"))))
        (compile escape)
        (escape)
        "#;
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        Value::Str(id) => assert_eq!(h.string(id), "ab"),
        other => panic!("expected a string, got {:?}", other),
    }
}
