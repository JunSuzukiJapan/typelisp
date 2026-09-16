//! `(unsafe body...)` — the form that grants permission.
//!
//! Two halves, and only the first exists at this stage. As a *form*, `unsafe`
//! is `progn`: it evaluates its body in order and answers with the last value,
//! establishes no scope of its own, and is not a function boundary — a `break`
//! or a `return-from` written inside one escapes straight through it. The
//! checker lowers it to the same node `progn` lowers to (a `let` with no
//! bindings), so nothing downstream of the checker learns it was ever written.
//!
//! The second half is what it is *for*: an FFI call and the raw-word types
//! (`ptr`/`c-long`/`c-ulong`) are rejected outside one. Those tests live in
//! `ffi_test.rs`, next to the declarations they need.
//!
//! Which means the whole of this file is the claim "adding permission changed
//! nothing about evaluation" — the part that would be easy to get wrong and
//! never notice, because every test of it passes whether or not `unsafe` does
//! anything at all.

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

fn int(src: &str) -> i64 {
    match run(src).expect("eval failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

fn text(src: &str) -> String {
    let (h, v) = run(src).expect("eval failed");
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// The same program with the compiler loaded, so a `(compile f)` in the source
/// really compiles `f` — the island's own lowering rather than the evaluator's.
fn int_compiled(src: &str) -> i64 {
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
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
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

// ---------------------------------------------------------------- as a progn

#[test]
fn answers_with_its_last_form() {
    assert_eq!(int("(unsafe 42)"), 42);
    assert_eq!(int("(unsafe 1 2 3)"), 3);
}

#[test]
fn evaluates_its_body_in_order() {
    // The values are discarded; the order is what is under test.
    assert_eq!(
        text(
            r#"
            (defvar (log string) "")
            (defun note ((s string)) () (setf log (append log s)) ())
            (unsafe (note "a") (note "b") (note "c"))
            log
            "#
        ),
        "abc"
    );
}

#[test]
fn an_empty_body_is_unit() {
    // `progn`'s rule, reached through the same `check_seq`.
    assert_eq!(int("(progn (unsafe) 7)"), 7);
}

#[test]
fn is_an_expression_like_any_other() {
    assert_eq!(int("(+ 1 (unsafe 2))"), 3);
    assert_eq!(int("(let ((x (unsafe 5))) (* x 2))"), 10);
}

#[test]
fn nests() {
    assert_eq!(int("(unsafe (unsafe (unsafe 9)))"), 9);
}

#[test]
fn establishes_no_scope_of_its_own() {
    // A `let` inside one does not leak, and one inside a `let` sees the binding.
    assert_eq!(int("(let ((x 4)) (unsafe (* x x)))"), 16);
}

// -------------------------------------------------- not a function boundary

#[test]
fn break_escapes_through_it() {
    // `unsafe` is not a function boundary, so the nearest enclosing loop is
    // still the one outside it.
    assert_eq!(int("(loop (unsafe (return 3)))"), 3);
}

#[test]
fn return_from_escapes_through_it() {
    assert_eq!(
        int(
            r#"
            (defun f () int (unsafe (return-from f 8)) 1)
            (f)
            "#
        ),
        8
    );
}

#[test]
fn a_lambda_written_inside_one_still_checks() {
    // The body of a `lambda` inside an `unsafe` inherits the permission
    // (nothing here needs it yet — what is under test is that the lambda
    // checks at all, since its body is checked through a different path).
    assert_eq!(
        int(
            r#"
            (defun run-it ((f (fn () int))) int (f))
            (run-it (unsafe (lambda () int 11)))
            "#
        ),
        11
    );
}

// ------------------------------------------------------------- compile path

#[test]
fn survives_compilation() {
    // The island never sees `unsafe`: it is gone by the time the core IR is
    // built, so a compiled body containing one is a compiled body containing
    // a `progn`. Compiling it is how that claim gets tested.
    assert_eq!(
        int_compiled(
            r#"
            (defun f ((n int)) int (unsafe (* n 3)))
            (compile f)
            (f 14)
            "#
        ),
        42
    );
}

#[test]
fn survives_compilation_with_a_loop_escaping_through_it() {
    assert_eq!(
        int_compiled(
            r#"
            (defun f ((n int)) int
              (loop (unsafe (return (+ n 1)))))
            (compile f)
            (f 5)
            "#
        ),
        6
    );
}

// ------------------------------------------------------------------- errors

#[test]
fn its_body_is_still_type_checked() {
    // Permission is not an escape from the type system: `unsafe` grants the
    // right to write particular operations, not the right to write nonsense.
    let e = check_err("(unsafe (+ 1 \"two\"))");
    assert!(e.contains("Str"), "unexpected error: {}", e);
}

#[test]
fn its_type_is_its_last_form_s_type() {
    let e = check_err("(defun f () string (unsafe 1))");
    assert!(e.contains("Str") && e.contains("Int"), "unexpected error: {}", e);
}
