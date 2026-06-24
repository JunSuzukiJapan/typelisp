//! Tests for the LLVM-backed `compile`/`compile-file` machinery (Phase 0:
//! the shared core that lets the (typelisp-hosted) compiler body build an
//! LLVM module at all — `llvm-module`/`llvm-function`/`llvm-builder`/
//! `llvm-value`, see `registry::llvm_module_def` and friends). No AST
//! bridge or JIT/AOT output path exists yet; these only exercise the raw
//! LLVM builtins from typelisp source, the same way `hashtable_test.rs`
//! exercises `HashTable<K,V>`.

extern crate typelisp;
use typelisp::{load_compiler, Checker, EvalError, Heap, Interp, Reader, RtValue};

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

/// Like [`run`], but with the (typelisp-hosted) compiler body
/// (`compiler::SOURCE`) loaded first, for tests that call
/// `compile-constant-function`/`compile-value`.
fn run_with_compiler(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok_with_compiler(src: &str) -> RtValue {
    run_with_compiler(src).expect("eval failed")
}

fn expect_str(v: RtValue) -> String {
    match v {
        RtValue::Str(s) => s,
        other => panic!("expected a Str, got {:?}", other),
    }
}

fn expect_bool(v: RtValue) -> bool {
    match v {
        RtValue::Bool(b) => b,
        other => panic!("expected a Bool, got {:?}", other),
    }
}

/// The Phase 0 vertical slice: build a zero-parameter, `i64`-returning LLVM
/// function by calling the raw LLVM builtins directly from typelisp source
/// (no AST bridge yet — that's a later phase), and check the resulting IR.
const BUILD_ANSWER_MODULE: &str = r#"
(defun build-answer-module () string
  (let ((m (llvm-module::create "mymod")))
    (let ((f (add-function m "answer")))
      (let ((b (append-block f "entry")))
        (let ((builder (llvm-builder::create)))
          (position-at-end builder b)
          (let ((c (const-i64 builder 42)))
            (build-ret builder c)
            (to-string m)))))))
"#;

#[test]
fn builds_a_module_with_a_constant_returning_function() {
    let src = format!("{}\n(build-answer-module)", BUILD_ANSWER_MODULE);
    let ir = expect_str(eval_ok(&src));
    assert!(ir.contains("define i64 @answer()"), "IR was:\n{}", ir);
    assert!(ir.contains("ret i64 42"), "IR was:\n{}", ir);
}

#[test]
fn a_freshly_built_module_verifies_successfully() {
    let src = r#"
        (defun build-and-verify () bool
          (let ((m (llvm-module::create "mymod")))
            (let ((f (add-function m "answer")))
              (let ((b (append-block f "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (let ((c (const-i64 builder 42)))
                    (build-ret builder c)
                    (verify m)))))))
        (build-and-verify)
    "#;
    assert!(expect_bool(eval_ok(src)));
}

/// The self-hosted compiler body (`src/compiler.rs`'s `compile-value`/
/// `compile-constant-function`) compiling the exact shape
/// `ast_bridge::ast_to_sexpr` produces for `Expr::Int` — `'(int 42)` here
/// stands in for what the bridge would build from the real typed AST (the
/// Rust-side `ast_bridge` unit tests already cover that translation in
/// isolation; this covers the compiler body consuming it).
#[test]
fn the_compiler_body_compiles_an_int_literal_node() {
    let ir = expect_str(eval_ok_with_compiler(
        r#"(compile-constant-function "answer" '(int 42))"#,
    ));
    assert!(ir.contains("define i64 @answer()"), "IR was:\n{}", ir);
    assert!(ir.contains("ret i64 42"), "IR was:\n{}", ir);
}

#[test]
fn the_compiler_body_panics_on_an_unsupported_tag() {
    let err = run_with_compiler(r#"(compile-constant-function "answer" '(if true))"#)
        .expect_err("expected an unsupported-tag panic");
    match err {
        EvalError::Panic(msg) => assert!(msg.contains("unsupported tag"), "message was: {}", msg),
        other => panic!("expected a Panic, got {:?}", other),
    }
}

#[test]
fn two_modules_built_back_to_back_do_not_interfere() {
    let src = format!(
        "{}\n(build-answer-module)\n(build-answer-module)",
        BUILD_ANSWER_MODULE
    );
    // Each call creates its own fresh module/function/builder; running it
    // twice in the same process is the cheapest possible check that the
    // shared process-wide `Context` (see `compile::llvm_context`) tolerates
    // repeated use rather than e.g. colliding on the function name.
    let ir = expect_str(eval_ok(&src));
    assert!(ir.contains("ret i64 42"), "IR was:\n{}", ir);
}
