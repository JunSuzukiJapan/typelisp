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
    let vs = r.read_all(&mut h, &format!("{}\n(compile main)\n(main)", src)).expect("read failed");
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
    let vs = r.read_all(&mut h, &format!("{}\n(compile main)\n(main)", src)).expect("read failed");
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
    use typelisp::{Checker, Heap, Interp, Reader, RtValue};

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
        Some(RtValue::Int(n)) => n,
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
    use typelisp::{Checker, Heap, Interp, Reader, RtValue};

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
        Some(RtValue::Int(n)) => n,
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
    use typelisp::{Checker, Heap, Interp, Reader, RtValue};

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
        Some(RtValue::Int(n)) => n,
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
    use typelisp::{Checker, Heap, Interp, Reader, RtValue};

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
        Some(RtValue::Int(n)) => n,
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
    use typelisp::{Checker, Heap, Interp, Reader, RtValue};

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
        Some(RtValue::Int(n)) => n,
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
    use typelisp::{Checker, Heap, Interp, Reader, RtValue};

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
        Some(RtValue::Int(n)) => n,
        other => panic!("expected an Int from the JIT path, got {:?}", other),
    };

    let aot_exit_code = compile_and_run("jit_aot_defenum_global_pair", src) as i64;

    assert_eq!(jit_value, aot_exit_code);
}
