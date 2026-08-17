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
    let (code, _) = compile_and_capture(name, source);
    code
}

/// [`compile_and_run`] plus the executable's stderr — for the cases where
/// *what it said* on the way out is the point, not just the code.
fn compile_and_capture(name: &str, source: &str) -> (i32, String) {
    let dir = tmp_dir();
    let src_path = dir.join(format!("{}.typl", name));
    let out_path = dir.join(name);
    std::fs::write(&src_path, source).expect("failed to write test source file");

    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");

    let out = Command::new(&out_path).output().expect("failed to run the compiled executable");
    let code = out.status.code().expect("process did not exit normally");
    (code, String::from_utf8_lossy(&out.stderr).into_owned())
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
    // `add2` specifically isn't called from `main` here (see
    // `compiles_and_runs_main_calling_a_helper_function` for a helper `main`
    // actually calls, labels/closures Stage 3) — this just confirms an
    // uncalled helper still compiles and links into the same executable
    // without colliding with anything.
    let src = r#"
        (defun add2 ((a i64) (b i64)) i64 (+ a b))
        (defun main () i64 7)
    "#;
    assert_eq!(compile_and_run("with_helper", src), 7);
}

/// The end-to-end Stage 3 slice (top-level `Expr::Call`, non-recursive),
/// AOT side: `main` calls the earlier-defined `square` twice — proving
/// `compile::aot::compile_file`'s simple one-pass loop needs no special
/// handling for cross-function calls (see that module's doc comment for
/// why), unlike `Interp::compile_function`'s JIT path.
#[test]
fn compiles_and_runs_main_calling_a_helper_function() {
    let src = r#"
        (defun square ((x i64)) i64 (* x x))
        (defun main () i64 (+ (square 3) (square 4)))
    "#;
    assert_eq!(compile_and_run("calls_helper", src), 25);
}

/// The end-to-end Stage 4 slice (escaping + capturing `lambda`, indirect
/// dispatch through a parameter), AOT side: `main` calls `apply-fn` with
/// `(adder 5)` directly — `main` still can't have parameters/`let`s of its
/// own, but every value here comes from ordinary calls with literal
/// arguments, which `main`'s body can already express (see this module's
/// doc comment). Proves the `ClosureBox` (`build-make-closure`/
/// `build-closure-apply`, `malloc`/`free`-based — see
/// `registry::llvm_builder_def`'s doc comment) links and runs correctly
/// from a real `cc`-built executable, not just under JIT.
#[test]
fn compiles_and_runs_an_escaping_capturing_lambda_through_a_helper() {
    let src = r#"
        (defun adder ((n i64)) (fn (i64) i64) (lambda ((x i64)) i64 (+ x n)))
        (defun apply-fn ((f (fn (i64) i64)) (n i64)) i64 (f n))
        (defun main () i64 (apply-fn (adder 5) 10))
    "#;
    assert_eq!(compile_and_run("escaping_lambda", src), 15);
}

/// labels/closures Stage 2 (outer-scope capture) + Stage 3 (top-level
/// `Expr::Call`) together: a non-`main` helper `defun` whose body is a
/// *capturing* `labels` form (`go` references `add-offset`'s own parameter
/// `offset`) compiles and links into the same shared module as `main`, and
/// `main` now calls it directly with literal arguments — proving the AOT
/// path handles the `add-function-with-env` ABI variant correctly alongside
/// an ordinary top-level call into it (`main` itself still can't have
/// parameters of its own to forward — see this module's doc comment — but
/// `add-offset`'s capture of `offset` happens entirely inside `add-offset`,
/// not via `main`).
#[test]
fn compiles_a_file_with_a_capturing_labels_helper_function_too() {
    let src = r#"
        (defun add-offset ((offset i64) (n i64)) i64
          (labels ((go ((k i64)) i64 (+ k offset)))
            (go n)))
        (defun main () i64 (add-offset 10 5))
    "#;
    assert_eq!(compile_and_run("with_capturing_helper", src), 15);
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

/// if/let/comparisons (labels/closures Stage 5), AOT side: a *real*,
/// terminating self-recursive helper function with a base case (`fact`),
/// called from `main` with a literal argument (`main` itself still can't
/// have parameters/`let`s of its own — see this module's doc comment).
/// Every earlier self-recursion AOT test had no `if` to write a base case
/// with at all (see `tests/compile_test.rs`'s
/// `compile_succeeds_for_a_self_recursive_defun_without_being_run`, which
/// could only prove compilation succeeded, never actually run it).
#[test]
fn compiles_and_runs_a_self_recursive_function_with_a_base_case() {
    let src = r#"
        (defun fact ((n i64)) i64 (if (<= n 1) 1 (* n (fact (- n 1)))))
        (defun main () i64 (fact 5))
    "#;
    assert_eq!(compile_and_run("self_recursive_base_case", src), 120);
}

/// `loop`/`break`/`return`/`setf`, AOT side: a helper function computing
/// `0+1+...+n` via a `loop` that mutates loop-carried locals with `setf`
/// and delivers the result with `return` (no recursion at all) — `main`
/// itself still can't have parameters/`let`s of its own (see this module's
/// doc comment), so the actual `loop`/`setf` logic lives in `sum-to`, the
/// same shape every other AOT helper test here already uses. Mirrors
/// `tests/compile_test.rs`'s
/// `compile_dispatches_a_counting_loop_with_setf_and_conditional_return_to_native_code`.
#[test]
fn compiles_and_runs_a_loop_based_sum_through_a_helper() {
    let src = r#"
        (defun sum-to ((n i32)) i32
          (let ((i 0) (acc 0))
            (loop
              (if (> i n) (return acc) ())
              (setf acc (+ acc i))
              (setf i (+ i 1)))))
        (defun main () i32 (sum-to 5))
    "#;
    assert_eq!(compile_and_run("loop_sum_helper", src), 15);
}

/// Nested `loop`s, AOT side: an inner `loop`'s bare `(break)` must exit only
/// the inner loop — see `tests/compile_test.rs`'s
/// `compile_dispatches_nested_loops_where_an_inner_break_only_exits_the_inner_loop`
/// for the JIT-side version of this exact source (same expected result, `6`,
/// for the same reason: 3 outer iterations &times; 2 inner increments each).
#[test]
fn compiles_and_runs_nested_loops_through_a_helper() {
    let src = r#"
        (defun nested-loop-test () i32
          (let ((outer 0) (total 0))
            (loop
              (if (eq outer 3) (return total) ())
              (let ((inner 0))
                (loop
                  (if (eq inner 2) (break) ())
                  (setf total (+ total 1))
                  (setf inner (+ inner 1))))
              (setf outer (+ outer 1)))))
        (defun main () i32 (nested-loop-test))
    "#;
    assert_eq!(compile_and_run("nested_loop_helper", src), 6);
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
    use typelisp::{Checker, Heap, Interp, Reader, Value};

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
    let vs = r.read_all(&mut h, &format!("{}\n(compile main)\n(main)", src)).expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(Value::Int(n)) => n,
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
    use typelisp::{Checker, Heap, Interp, Reader, Value};

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
    let vs = r.read_all(&mut h, &format!("{}\n(compile main)\n(main)", src)).expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(Value::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_labels_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}

/// Same claim as `jit_and_aot_agree_on_the_same_source`, but for a `main`
/// that calls a separately-defined helper function directly (labels/
/// closures Stage 3, top-level `Expr::Call`) rather than bare arithmetic or
/// a `labels` form. The JIT path needs an extra `(compile square)` before
/// `(compile main)` — `Interp::compile_function`'s own forward-declare +
/// `add_global_mapping` requirement (see that method's doc comment) — that
/// `compile::aot::compile_file`'s single shared module never needs (see
/// that module's doc comment).
#[test]
fn jit_and_aot_agree_on_a_cross_function_call() {
    use typelisp::{Checker, Heap, Interp, Reader, Value};

    let src = r#"
        (defun square ((x i64)) i64 (* x x))
        (defun main () i64 (+ (square 3) (square 4)))
    "#;

    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, &format!("{}\n(compile square)\n(compile main)\n(main)", src))
        .expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(Value::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_call_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}

/// Same claim as `jit_and_aot_agree_on_the_same_source`, but for `main`'s
/// escaping-capturing-`lambda`-through-a-helper body (labels/closures
/// Stage 4 — see `compiles_and_runs_an_escaping_capturing_lambda_through_a_helper`
/// for the AOT-only version of this exact source). Both `adder` and
/// `apply-fn` need a `(compile ...)` of their own before `main`'s, same
/// reason as `jit_and_aot_agree_on_a_cross_function_call`.
#[test]
fn jit_and_aot_agree_on_an_escaping_capturing_lambda() {
    use typelisp::{Checker, Heap, Interp, Reader, Value};

    let src = r#"
        (defun adder ((n i64)) (fn (i64) i64) (lambda ((x i64)) i64 (+ x n)))
        (defun apply-fn ((f (fn (i64) i64)) (n i64)) i64 (f n))
        (defun main () i64 (apply-fn (adder 5) 10))
    "#;

    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, &format!("{}\n(compile adder)\n(compile apply-fn)\n(compile main)\n(main)", src))
        .expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(Value::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_closure_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}

/// Same claim as `jit_and_aot_agree_on_the_same_source`, but for a `main`
/// that calls a self-recursive helper with a base case (if/let/comparisons,
/// labels/closures Stage 5) rather than bare arithmetic — `fact` needs its
/// own `(compile ...)` before `main`'s, same reason as
/// `jit_and_aot_agree_on_a_cross_function_call`.
#[test]
fn jit_and_aot_agree_on_a_self_recursive_function_with_a_base_case() {
    use typelisp::{Checker, Heap, Interp, Reader, Value};

    let src = r#"
        (defun fact ((n i64)) i64 (if (<= n 1) 1 (* n (fact (- n 1)))))
        (defun main () i64 (fact 5))
    "#;

    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, &format!("{}\n(compile fact)\n(compile main)\n(main)", src))
        .expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(Value::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_self_recursive_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}

/// Same claim as `jit_and_aot_agree_on_the_same_source`, but for a `main`
/// that calls a `loop`/`break`/`return`/`setf`-based helper (this stage)
/// rather than bare arithmetic — `sum-to` needs its own `(compile ...)`
/// before `main`'s, same reason as `jit_and_aot_agree_on_a_cross_function_call`.
#[test]
fn jit_and_aot_agree_on_a_loop_based_function() {
    use typelisp::{Checker, Heap, Interp, Reader, Value};

    let src = r#"
        (defun sum-to ((n i32)) i32
          (let ((i 0) (acc 0))
            (loop
              (if (> i n) (return acc) ())
              (setf acc (+ acc i))
              (setf i (+ i 1)))))
        (defun main () i32 (sum-to 5))
    "#;

    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, &format!("{}\n(compile sum-to)\n(compile main)\n(main)", src))
        .expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(Value::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_loop_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}

/// A top-level `defvar` is accepted (no longer `errors_on_a_non_defun_top_level_form`'s
/// case) and `main` can read it — `compile::aot::compile_file`'s eager,
/// file-order `Interp::promote_global` + the generated `$global_init$0`
/// step `build_main_wrapper` calls before `tl_main`.
#[test]
fn compiles_and_runs_main_that_reads_a_global() {
    let src = r#"
        (defvar (answer i64) 42)
        (defun main () i64 answer)
    "#;
    assert_eq!(compile_and_run("global_read", src), 42);
}

/// `main` can also assign a `defvar` (`Expr::SetGlobal`,
/// `compiler.rs`'s `compile-set-global`) — and a *second* read afterward
/// sees the write, proving the standalone executable's own `rt_global_set`/
/// `rt_global_get` calls agree on the same slot (not just that each
/// compiles individually).
#[test]
fn compiles_and_runs_main_that_writes_a_global() {
    let src = r#"
        (defvar (counter i64) 0)
        (defun bump () i64 (setf counter (+ counter 1)))
        (defun main () i64 (let ((ignored (bump))) (bump)))
    "#;
    assert_eq!(compile_and_run("global_write", src), 2);
}

/// Same claim as `jit_and_aot_agree_on_the_same_source`, but for a `main`
/// that reads and assigns a `defvar` — proves JIT's lazy, reference-driven
/// global promotion (`Interp::compile_function`) and AOT's eager,
/// file-declaration-order promotion (`compile::aot::compile_file`) agree on
/// the same result despite assigning compile-time ids in different orders
/// (see `Interp::promote_global`'s doc comment).
#[test]
fn jit_and_aot_agree_on_a_global_read_and_write() {
    use typelisp::{Checker, Heap, Interp, Reader, Value};

    let src = r#"
        (defvar (counter i64) 0)
        (defun bump () i64 (setf counter (+ counter 1)))
        (defun main () i64 (let ((ignored (bump))) (bump)))
    "#;

    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, &format!("{}\n(compile bump)\n(compile main)\n(main)", src))
        .expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(Value::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_global_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}

/// A top-level `defenum` is accepted by `compile-file` (its exec records
/// the variant field types in `Interp::enum_defs`), and an enum-typed
/// `defvar` promotes and reads back in the standalone executable — the
/// startup `$global_init$N` builds the box via the ordinary
/// `compile-construct-box`, and `main`'s `match` discriminates on its tag.
#[test]
fn compiles_and_runs_main_that_reads_a_defenum_global() {
    let src = r#"
        (defenum shape (Circle i64) (Rect i64 i64))
        (defvar (s shape) (shape::Rect 6 7))
        (defun main () i64 (match s ((Circle r) (* r r)) ((Rect w h) (* w h))))
    "#;
    assert_eq!(compile_and_run("defenum_global_read", src), 42);
}

/// The `defenum`-global mirror of `jit_and_aot_agree_on_a_global_read_and_write`:
/// both pipelines must agree on a `main` that writes and then reads an
/// enum-typed global.
#[test]
fn jit_and_aot_agree_on_a_defenum_global_read_and_write() {
    use typelisp::{Checker, Heap, Interp, Reader, Value};

    let src = r#"
        (defenum counter (At i64))
        (defvar (c counter) (counter::At 0))
        (defun bump () i64
          (match c ((At n) (progn (setf c (counter::At (+ n 1))) (+ n 1)))))
        (defun main () i64 (let ((ignored (bump))) (bump)))
    "#;

    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, &format!("{}\n(compile bump)\n(compile main)\n(main)", src))
        .expect("read failed");
    let mut jit_result = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("exec failed") {
            jit_result = Some(val);
        }
    }
    let jit_value = match jit_result {
        Some(Value::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_defenum_global_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}

// ---- user types and methods (AOT prerequisite for trait objects) --------

/// AOT used to reject anything that wasn't a `defun`/`defvar`/`defenum`, so
/// a `defstruct` with methods could not appear in a compiled file at all —
/// `Checker::check_defstruct`/`check_defmethod` return `TopLevel` shapes the
/// item loop had no arm for. Both now compile like any other body.
#[test]
fn compiles_a_defstruct_with_a_method() {
    assert_eq!(
        compile_and_run(
            "struct_method",
            r#"
            (defstruct point (x i32) (y i32))
            (defmethod norm ((self point)) i32 (+ (* self::x self::x) (* self::y self::y)))
            (defun main () i64 (as i64 (norm (point::new 3 4))))
            "#
        ),
        25
    );
}

/// An `impl` block returns a `TopLevel::Module` grouping its methods, which
/// the item loop now flattens like any other module.
#[test]
fn compiles_an_impl_block() {
    assert_eq!(
        compile_and_run(
            "impl_block",
            r#"
            (deftrait Counted () (count ((self Self)) i32))
            (defstruct box-a (n i32))
            (impl Counted box-a (count ((self Self)) i32 self::n))
            (defun main () i64 (as i64 (count (box-a::new 9))))
            "#
        ),
        9
    );
}

// ---- dynamic dispatch (TODO T4) ----------------------------------------

/// The AOT tier of the same test `compile_test.rs` runs for the JIT: one
/// call site, two implementations, dispatched through the vtable. The table
/// is filled at process startup by `rt_vtable_set` calls whose function
/// pointers are `ptrtoint` constants the linker resolves.
#[test]
fn compiles_and_runs_dynamic_dispatch_through_a_trait_object() {
    assert_eq!(
        compile_and_run(
            "dyn_dispatch",
            r#"
            (deftrait Drawable () (draw ((self Self)) i32))
            (defstruct circle (r i32))
            (defstruct square (side i32))
            (impl Drawable circle (draw ((self Self)) i32 1))
            (impl Drawable square (draw ((self Self)) i32 2))
            (defun render ((d :dyn Drawable)) i32 (draw d))
            (defun main () i64
              (as i64 (+ (* 10 (render (circle::new 3))) (render (square::new 4)))))
            "#
        ),
        12
    );
}

/// The supertrait conversion table has to reach a linked executable too:
/// `rt_upcast_set` calls emitted alongside the `rt_vtable_set` ones, all
/// constants. `2`/`5` are `c-tag`'s; slot 0 of `D`'s own table is `b-tag`,
/// so a missing conversion would answer `14`.
#[test]
fn compiles_and_runs_an_upcast_to_a_non_first_supertrait() {
    assert_eq!(
        compile_and_run(
            "dyn_upcast",
            r#"
            (deftrait B () (b-tag ((self Self)) i32))
            (deftrait C () (c-tag ((self Self)) i32))
            (deftrait D (B C) (d-tag ((self Self)) i32))
            (defstruct cell (n i32))
            (defstruct pair (n i32))
            (impl B cell (b-tag ((self Self)) i32 1))
            (impl C cell (c-tag ((self Self)) i32 2))
            (impl D cell (d-tag ((self Self)) i32 3))
            (impl B pair (b-tag ((self Self)) i32 4))
            (impl C pair (c-tag ((self Self)) i32 5))
            (impl D pair (d-tag ((self Self)) i32 6))
            (defun only-c ((c :dyn C)) i32 (c-tag c))
            (defun via ((d :dyn D)) i32 (only-c d))
            (defun main () i64 (as i64 (+ (* 10 (via (cell::new 0))) (via (pair::new 0)))))
            "#
        ),
        25
    );
}

/// Slot numbering (`deftrait` order) must hold in a linked executable too.
#[test]
fn compiles_and_runs_a_multi_slot_vtable() {
    assert_eq!(
        compile_and_run(
            "dyn_slots",
            r#"
            (deftrait Shape ()
              (draw ((self Self)) i32)
              (sides ((self Self)) i32))
            (defstruct tri (n i32))
            (impl Shape tri
              (draw ((self Self)) i32 7)
              (sides ((self Self)) i32 3))
            (defun paint ((s :dyn Shape)) i32 (draw s))
            (defun outline ((s :dyn Shape)) i32 (sides s))
            (defun main () i64 (as i64 (+ (* 10 (paint (tri::new 1))) (outline (tri::new 1)))))
            "#
        ),
        73
    );
}

/// A `(panic ...)` that reaches the entry point ends the program cleanly —
/// message on stderr, exit code 1 — instead of aborting it.
///
/// `typelisp_rt::rt_panic` unwinds now, and an AOT executable's `main` is
/// generated LLVM code with no Rust frame to catch in; `rt_run_entry` is the
/// frame added for exactly this, and `compile::aot`'s entry-point builder
/// calls `tl_main` through it. Without that, the unwind would run off the end
/// of the C `main`, which is undefined.
///
/// Exit code 1 rather than the old 134 (`SIGABRT`) is the point: it matches
/// what `typl <file>` exits with when an interpreted run ends in an error, so
/// a script cannot tell the two front ends apart.
#[test]
fn a_panic_reaching_the_entry_point_exits_rather_than_aborting() {
    let (code, stderr) = compile_and_capture("panics", r#"(defun main () i64 (panic "from aot"))"#);
    assert_eq!(code, 1, "stderr was: {}", stderr);
    assert!(stderr.contains("panic: from aot"), "stderr was: {}", stderr);
}

/// The panic still has to be reachable *through* a call, not just at the top
/// of `main` — that is the frame the unwinder actually has to walk.
#[test]
fn a_panic_unwinds_through_a_compiled_call_in_an_aot_executable() {
    let (code, stderr) = compile_and_capture(
        "panics_nested",
        r#"
        (defun inner ((n i32)) i32 (if (< n 0) (panic "negative") n))
        (defun outer ((n i32)) i32 (inner n))
        (defun main () i64 (as i64 (outer -1)))
        "#,
    );
    assert_eq!(code, 1, "stderr was: {}", stderr);
    assert!(stderr.contains("panic: negative"), "stderr was: {}", stderr);
}

/// `catch`/`throw` in an AOT executable — the whole round trip with no
/// interpreter in the process at all.
///
/// The unwind is raised two compiled frames down, caught by the trampoline at
/// the protected call, recognised by tag in the region's dispatch block, and
/// its value produced in the catching function. Nothing here has a landing pad
/// or a personality routine, which is why the `__gxx_personality_v0` link
/// failure that ruled out the LLVM-EH approach cannot recur.
#[test]
fn catch_and_throw_work_in_an_aot_executable() {
    assert_eq!(
        compile_and_run(
            "aot_catch",
            r#"
            (defun deep ((n i32)) i32 (throw 'done (* n 2)))
            (defun middle ((n i32)) i32 (+ 1 (deep n)))
            (defun main () i64 (as i64 (catch 'done (middle 21))))
            "#
        ),
        42
    );
}

/// An `unwind-protect` cleanup runs while the throw passes through it, in a
/// standalone executable.
#[test]
fn an_unwind_protect_cleanup_runs_in_an_aot_executable() {
    assert_eq!(
        compile_and_run(
            "aot_unwind_protect",
            r#"
            (defvar (ran i32) 0)
            (defun body () i32 (unwind-protect (throw 'done 40) (setf ran 2)))
            (defun main () i64 (as i64 (+ (catch 'done (body)) ran)))
            "#
        ),
        42
    );
}

/// A `throw` with no enclosing `catch` anywhere ends the program the way an
/// uncaught `panic` does — message on stderr, exit code 1 — rather than
/// running off the end of the C `main`, which is undefined.
#[test]
fn an_uncaught_throw_reaching_the_entry_point_exits_rather_than_aborting() {
    let (code, stderr) = compile_and_capture(
        "aot_uncaught_throw",
        r#"
        (defun thrower () i32 (throw 'nobody 1))
        (defun main () i64 (as i64 (thrower)))
        "#,
    );
    assert_eq!(code, 1, "stderr was: {}", stderr);
    assert!(stderr.contains("no enclosing (catch 'nobody)"), "stderr was: {}", stderr);
}

/// A runtime failure the *runtime* raises — here a zero divisor inside
/// `rt_i64_div` — ends an AOT executable the same way an explicit
/// `(panic ...)` does: message on stderr, exit code 1.
///
/// It used to abort with 134 instead, because `rt_i64_div` reached
/// `typelisp_rt::fatal()`. That it now unwinds all the way to `rt_run_entry`
/// is what makes an AOT program's arithmetic failures reportable rather than
/// fatal; `tests/runtime_error_parity_test.rs` covers the same failures on the
/// interpreted and JIT paths.
///
/// The divisor comes from a call so nothing can fold it away before the
/// division is emitted.
#[test]
fn a_zero_divisor_exits_rather_than_aborting_in_an_aot_executable() {
    let (code, stderr) = compile_and_capture(
        "aot_divide_by_zero",
        r#"
        (defun zero () i32 0)
        (defun main () i64 (as i64 (/ 5 (zero))))
        "#,
    );
    assert_eq!(code, 1, "stderr was: {}", stderr);
    assert!(stderr.contains("panic: divide by zero"), "stderr was: {}", stderr);
}

/// ...and an `unwind-protect` around it runs its cleanup on the way out, in a
/// process with no interpreter in it at all.
///
/// The cleanup's effect is observable in the exit code: `main` catches nothing
/// (a panic is not a throw), so the program still dies — but `ran` was written
/// while it was dying, and the exit code carries the failure, not the value.
#[test]
fn a_cleanup_runs_when_a_zero_divisor_unwinds_in_an_aot_executable() {
    let (code, stderr) = compile_and_capture(
        "aot_divide_by_zero_cleanup",
        r#"
        (defvar (ran i32) 0)
        (defun zero () i32 0)
        (defun body () i32 (unwind-protect (/ 5 (zero)) (setf ran 1)))
        (defun main () i64 (as i64 (+ (body) ran)))
        "#,
    );
    assert_eq!(code, 1, "stderr was: {}", stderr);
    assert!(stderr.contains("panic: divide by zero"), "stderr was: {}", stderr);
}

/// A `break` leaving a protected form runs the cleanup in a standalone
/// executable too — the static exit is a plain branch, so nothing about it
/// depends on the JIT's own unwinding setup.
#[test]
fn a_break_runs_an_unwind_protect_cleanup_in_an_aot_executable() {
    assert_eq!(
        compile_and_run(
            "aot_break_cleanup",
            r#"
            (defvar (ran i32) 0)
            (defun body () i32 (progn (loop (unwind-protect (break) (setf ran 40))) ran))
            (defun main () i64 (as i64 (+ (body) 2)))
            "#
        ),
        42
    );
}
