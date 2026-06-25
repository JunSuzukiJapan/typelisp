//! Tests for `compile-file` (the AOT exit, Phase 2): an independent
//! source file compiled straight to a native executable via
//! `typelisp::compile::aot::compile_file`, run as a subprocess and checked
//! by its exit code (since Phase 2's fixed-ABI entry point only carries an
//! `i64` exit code out, not text output — see `compile::aot`'s doc
//! comment).
//!
//! Every test writes its source/output under `target/aot-test-tmp/<unique
//! name>` rather than a shared path, both so parallel `cargo test` threads
//! never collide and so `compile::aot::write_executable`'s `<output>.o`
//! naming scheme (which relies on distinct `output_path`s for the same
//! reason) stays collision-free too.

use std::path::PathBuf;
use std::process::Command;

fn tmp_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp");
    std::fs::create_dir_all(&dir).expect("failed to create the AOT test scratch dir");
    dir
}

/// Writes `source` to `<scratch>/<name>.typl`, AOT-compiles it to
/// `<scratch>/<name>`, and returns the exit code of running that
/// executable with no arguments.
fn compile_and_run(name: &str, source: &str) -> i32 {
    let dir = tmp_dir();
    let src_path = dir.join(format!("{}.typl", name));
    let out_path = dir.join(name);
    std::fs::write(&src_path, source).expect("failed to write test source file");

    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");

    let status = Command::new(&out_path).status().expect("failed to run the compiled executable");
    status.code().expect("process did not exit normally")
}

#[test]
fn compiles_and_runs_a_constant_main() {
    assert_eq!(compile_and_run("answer", "(defun main () i64 42)"), 42);
}

#[test]
fn compiles_and_runs_arithmetic_in_main() {
    // `i32`, not `i64`: a bare integer literal with no surrounding type
    // context (no typed parameter or `let` — `main` can't have either,
    // see this module's doc comment) always defaults to `i32` (the
    // checker resolves `+`/`-`/`*`'s receiver type from its *first*
    // argument checked with `expected: None`, by design — see
    // `Checker::try_instance_method`'s doc comment — so the outer
    // `expected: Some(I64)` from `main`'s declared return type never
    // reaches it). That's purely a type-checking-time distinction, not a
    // codegen one: every integer is `RtValue::Int(i64)`/an LLVM `i64` IR
    // value regardless of typelisp-level width (`compile::ast_bridge`
    // always emits `(int n)`, `compiler.rs`'s `compile-int` always builds
    // an `i64` constant), so declaring `main`'s return type `i32` here
    // changes nothing about what actually runs.
    assert_eq!(compile_and_run("arith", "(defun main () i32 (- (* 6 8) (+ 4 2)))"), 42);
}

#[test]
fn compiles_a_file_with_a_non_main_helper_function_too() {
    // `add2` isn't called from `main` (cross-function calls aren't
    // supported until a later phase — see `compile::aot`'s doc comment),
    // but it still has to compile and link into the same executable
    // without colliding with anything.
    let src = r#"
        (defun add2 ((a i64) (b i64)) i64 (+ a b))
        (defun main () i64 7)
    "#;
    assert_eq!(compile_and_run("with_helper", src), 7);
}

/// labels/closures Stage 2 (outer-scope capture): a non-`main` helper
/// `defun` whose body is a *capturing* `labels` form (`go` references
/// `add-offset`'s own parameter `offset`) compiles and links into the same
/// shared module as `main` without error — proving the AOT path handles the
/// `add-function-with-env` ABI variant correctly alongside `main`'s own
/// (always non-capturing) function in one module. `main` can't actually
/// call `add-offset` here (no top-level `Expr::Call` support yet, and
/// `main` itself can't have parameters to capture from — see this module's
/// doc comment), the same limitation `compiles_a_file_with_a_non_main_helper_function_too`
/// already works around for an ordinary (non-capturing) helper.
#[test]
fn compiles_a_file_with_a_capturing_labels_helper_function_too() {
    let src = r#"
        (defun add-offset ((offset i64) (n i64)) i64
          (labels ((go ((k i64)) i64 (+ k offset)))
            (go n)))
        (defun main () i64 7)
    "#;
    assert_eq!(compile_and_run("with_capturing_helper", src), 7);
}

/// labels compilation work (Stage 1): `main`'s body is a `labels` form with
/// two non-recursive siblings, one calling the other directly — proving
/// the AOT path compiles `labels`-sibling calls the same way JIT does (see
/// `tests/compile_test.rs`'s `compile_dispatches_a_defun_with_a_labels_body_to_native_code`
/// for the JIT-side version of this exact source).
#[test]
fn compiles_and_runs_a_labels_body_in_main() {
    let src = r#"
        (defun main () i64
          (labels ((square ((x i64)) i64 (* x x))
                   (sum-helper ((x i64) (y i64)) i64 (+ (square x) (square y))))
            (sum-helper 3 4)))
    "#;
    assert_eq!(compile_and_run("labels_in_main", src), 25);
}

#[test]
fn exit_code_is_truncated_to_32_bits() {
    // Process exit codes are a byte on POSIX (`status.code()` already
    // reports the OS's truncated view) — 256 wraps to 0, matching `i64`
    // wrapping down through the wrapper's `i32` truncation.
    assert_eq!(compile_and_run("wraps", "(defun main () i64 256)"), 0);
}

#[test]
fn errors_without_a_zero_argument_main() {
    let dir = tmp_dir();
    let src_path = dir.join("no_main.typl");
    let out_path = dir.join("no_main");
    std::fs::write(&src_path, "(defun answer () i64 42)").unwrap();

    let err = typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect_err("expected a missing-entry-point error");
    assert!(err.contains("main"), "error was: {}", err);
}

#[test]
fn errors_on_a_non_defun_top_level_form() {
    let dir = tmp_dir();
    let src_path = dir.join("bad_top_level.typl");
    let out_path = dir.join("bad_top_level");
    std::fs::write(&src_path, "(defun main () i64 42)\n(+ 1 2)").unwrap();

    let err = typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect_err("expected an unsupported-top-level-form error");
    assert!(err.contains("defun"), "error was: {}", err);
}

/// The Phase 2 core claim: the same typelisp source, compiled through the
/// JIT path (`compile`) and the AOT path (`compile-file`), agrees — proving
/// the per-function compile step (`Interp::add_compiled_function`) really
/// is shared, not two parallel implementations that happen to look similar.
#[test]
fn jit_and_aot_agree_on_the_same_source() {
    use typelisp::{Checker, Heap, Interp, Reader, RtValue};

    // `i32`, not `i64` — see `compiles_and_runs_arithmetic_in_main`'s doc
    // comment.
    let src = "(defun main () i32 (- (* 6 8) (+ 4 2)))";

    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    // The JIT path has no notion of an entry point (that's AOT-only), so
    // explicitly `compile` then call `main` after defining it.
    let vs = r.read_all(&mut h, &format!("{}\n(compile \"main\")\n(main)", src)).expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(RtValue::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}

/// Same claim as `jit_and_aot_agree_on_the_same_source`, but for a `main`
/// whose body is a `labels` form with a non-recursive sibling call (Stage 1
/// scope) rather than bare arithmetic.
#[test]
fn jit_and_aot_agree_on_a_labels_body() {
    use typelisp::{Checker, Heap, Interp, Reader, RtValue};

    let src = r#"
        (defun main () i64
          (labels ((square ((x i64)) i64 (* x x))
                   (sum-helper ((x i64) (y i64)) i64 (+ (square x) (square y))))
            (sum-helper 3 4)))
    "#;

    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, &format!("{}\n(compile \"main\")\n(main)", src)).expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(RtValue::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_labels_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}
