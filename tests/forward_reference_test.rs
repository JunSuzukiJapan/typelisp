//! Tests for top-level forward references — `Checker::predeclare_program`.
//!
//! Every driver that checks a whole program (the prelude loader, the file
//! loaders in `project.rs`, `compile-module`, AOT `compile-file`, the compiler
//! island's own bootstrap) runs a pre-pass that registers each top-level
//! `defun`'s *signature* before any body is checked. `check_defun` has always
//! registered a function's own signature before its body, so self-recursion
//! worked; what did not was recursion *between* forms, because the drivers
//! check and execute one form at a time. These tests pin the resulting
//! behaviour — both what now works and what deliberately still does not.
//!
//! `run`/`check_program` mirror the real drivers: read the whole program,
//! pre-declare, then check/exec form by form. A test that skipped the
//! pre-pass would just be testing the old behaviour.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Error, Heap, Interp, Reader, Value};

/// Read, pre-declare, then check+exec every form — the shape every real
/// driver has. `with_island` additionally loads the prelude and the compiler
/// island, which anything building a closure or calling `(compile ...)` needs
/// (a closure value JIT-compiles when it can, and compiling needs the
/// island).
fn run_inner(src: &str, with_island: bool) -> Result<Value, Error> {
    let mut h = Heap::with_capacity(1 << 18);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    if with_island {
        load_prelude(&mut h, &mut chk, &mut interp);
        load_compiler(&mut h, &mut chk, &mut interp);
    }
    let vs = r.read_all(&mut h, src).expect("read failed");
    chk.predeclare_program(&mut h, &vs);
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?;
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    Ok(last)
}

fn run(src: &str) -> Result<Value, Error> {
    run_inner(src, false)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

fn eval_ok_with_island(src: &str) -> Value {
    run_inner(src, true).expect("eval failed")
}

/// The text of a `string` result. A `string` is a heap `Value::Str` since the
/// scalar unification, so reading one needs the heap it lives in — and
/// `run_inner` drops its heap on return, hence this parallel runner.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 18);
    let r = Reader::new();
    let mut chk = Checker::new();
    let interp = Interp::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    chk.predeclare_program(&mut h, &vs);
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        typelisp::Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

fn expect_err(src: &str) -> String {
    match run(src) {
        Ok(v) => panic!("expected an error, got {:?}", v),
        Err(e) => e.to_string(),
    }
}

/// Collects the warnings a program produces (rather than its value) — for
/// asserting that a genuine redefinition still reports even though the
/// pre-pass registered the name first.
fn warnings_of(src: &str) -> Vec<String> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    chk.predeclare_program(&mut h, &vs);
    let mut out = Vec::new();
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        out.extend(chk.take_warnings());
        interp.exec(&mut h, tl).expect("eval failed");
    }
    out
}

// ---- what now works -----------------------------------------------------

#[test]
fn a_defun_can_call_one_defined_below_it() {
    assert_eq!(eval_ok("(defun a () i64 (b)) (defun b () i64 7) (a)"), Value::Int(7));
}

#[test]
fn two_top_level_defuns_can_be_mutually_recursive() {
    // The case `Interp::compute_sccs`'s own test could only reach by
    // hand-building `FnDef`s and bypassing the checker entirely (see
    // `scc_tests` in `eval/interp.rs`): surface syntax can now express it.
    let src = "
        (defun even2? ((n i64)) bool (if (= n 0) true  (odd2? (- n 1))))
        (defun odd2?  ((n i64)) bool (if (= n 0) false (even2? (- n 1))))
        (even2? 10)";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn mutual_recursion_still_holds_after_compiling_one_of_the_pair() {
    // Exercises the JIT's transitive/SCC path over a genuine cycle that came
    // from source rather than from a hand-built `FnDef`.
    let src = "
        (defun even2? ((n i64)) bool (if (= n 0) true  (odd2? (- n 1))))
        (defun odd2?  ((n i64)) bool (if (= n 0) false (even2? (- n 1))))
        (compile even2?)
        (even2? 11)";
    assert_eq!(eval_ok_with_island(src), Value::Bool(false));
}

#[test]
fn a_forward_call_infers_the_return_type_from_the_predeclared_signature() {
    // The caller is checked before the callee's body ever is, so the only
    // thing that can give `(b)` its type is the pre-declared signature.
    assert_eq!(eval_string("(defun a () string (append (b) \"!\")) (defun b () string \"hi\") (a)"), "hi!");
}

#[test]
fn a_forward_call_to_a_generic_defun_specializes() {
    // A generic needs more than its signature: `request_fn_specialization`
    // consults the retained `FnTemplate`, so the pre-pass has to register
    // that too.
    assert_eq!(eval_ok("(defun use-it ((n i64)) i64 (idg n)) (defun idg<T> ((x T)) T x) (use-it 7)"), Value::Int(7));
}

#[test]
fn a_forward_call_passes_a_rest_argument() {
    let src = "
        (defun a () i64 (total 1 2 3))
        (defun total (&rest (xs i64)) i64 (sexpr-list-length-i64 xs))
        (a)";
    assert_eq!(eval_ok_with_island(src), Value::Int(3));
}

#[test]
fn a_wrong_argument_type_to_a_forward_call_is_still_a_type_error() {
    // The pre-declared signature must be the real one, not a permissive
    // placeholder — otherwise forward calls would silently skip checking.
    let msg = expect_err("(defun a () i64 (b \"nope\")) (defun b ((n i64)) i64 n) (a)");
    assert!(msg.contains("type"), "expected a type error, got: {}", msg);
}

#[test]
fn forward_references_work_inside_a_nested_module() {
    let src = "
        (module m
          (pub defun a () i64 (b))
          (pub defun b () i64 42))
        (m::a)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn a_lambda_body_can_call_a_defun_defined_below() {
    let src = "
        (defun a () i64 (let ((f (lambda () i64 (b)))) (f)))
        (defun b () i64 5)
        (a)";
    assert_eq!(eval_ok_with_island(src), Value::Int(5));
}

// ---- what deliberately still does not ------------------------------------

#[test]
fn a_genuine_redefinition_is_still_reported() {
    // The pre-pass registers `dup` before either definition is checked, so
    // the first real check finds an entry already there. It must recognise
    // that entry as its own pre-declaration (and stay quiet) while the
    // *second* definition still reports.
    let w = warnings_of("(defun dup () i64 1) (defun dup () i64 2)");
    assert_eq!(w.len(), 1, "expected exactly one redefinition warning, got {:?}", w);
    assert!(w[0].contains("dup"), "warning should name the function: {}", w[0]);
}

#[test]
fn a_single_definition_produces_no_redefinition_warning() {
    assert!(warnings_of("(defun once () i64 1) (once)").is_empty());
}

#[test]
fn a_macro_must_still_be_defined_before_use() {
    // Macros are deliberately not pre-declared: expanding one needs its body
    // to have been `exec`'d, which a signature registration cannot arrange.
    let msg = expect_err("(defun a () i64 (twice 5)) (defmacro twice (x) `(* 2 ,x)) (a)");
    assert!(msg.contains("twice"), "error should name the macro: {}", msg);
}

#[test]
fn a_defun_whose_signature_names_a_type_defined_below_still_fails() {
    // Types are not pre-declared, so this `defun` never gets pre-declared
    // either — its caller keeps the old behaviour. Pinned so that adding
    // forward *types* later is a deliberate, visible change.
    let src = "
        (defun drive () i64 (make-it))
        (defun make-it () pt (pt::new 1))
        (defstruct pt (x i64))
        (drive)";
    let msg = expect_err(src);
    assert!(msg.contains("make-it") || msg.contains("pt"), "unexpected error: {}", msg);
}

#[test]
fn an_undefined_function_is_still_an_error() {
    let msg = expect_err("(defun a () i64 (nope)) (a)");
    assert!(msg.contains("nope"), "error should name the missing function: {}", msg);
}
