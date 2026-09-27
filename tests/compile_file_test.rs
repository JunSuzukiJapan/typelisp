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

/// `~/name/` in a standalone executable.
///
/// The directive names its method in a string, and an AOT program has no
/// interpreter to look a name up in — so it used to be refused outright.
/// `format`/`print`/`println` now take a *literal* control string, the
/// checker scans it (`Checker::format_call_methods`), and `compile_file`
/// emits one `rt_format_call_method` registration per method it named,
/// beside the `print-object` ones. Exit code 0 means the directive produced
/// exactly what the interpreter produces for the same program.
#[test]
fn a_call_directive_dispatches_in_a_compiled_executable() {
    assert_eq!(
        compile_and_run(
            "format_call",
            r#"(defstruct money (yen int))
               (defmethod jp ((self money) (colon bool) (at bool)) string
                 (format false "~a yen" (yen self)))
               (defun main () int
                 (if (equal (format false "~/jp/" (money::new 300)) "300 yen") 0 1))"#,
        ),
        0
    );
}

/// The same variant-name lookup, in a standalone executable.
///
/// The AOT tier has its own table (`rt_print_enum_variant` fills it at
/// startup, one entry per enum *type*), and it was looked up by the *value's*
/// key — which now names an instantiation. The interpreted half of this bug
/// was caught by 28 existing tests; this half was caught by nothing, so it
/// gets a test of its own.
#[test]
fn an_enum_variant_name_survives_an_instantiated_key_in_an_executable() {
    assert_eq!(
        compile_and_run(
            "enum_variant_key",
            r#"(defun main () int
                 (if (equal (format false "~a" (option::some 1)) "(some 1)") 0 1))"#,
        ),
        0
    );
}

#[test]
fn compiles_and_runs_a_call_to_a_declared_c_function() {
    // The AOT half of the FFI. The thunk goes into the executable's own
    // module; the C function it calls is left to the linker, which finds
    // `abs` in the libc `cc` already links.
    assert_eq!(
        compile_and_run(
            "ffi_abs",
            r#"
            (defffi (c-abs "abs") (i32) i32)
            (defun main () int (as int (unsafe (c-abs -7))))
            "#
        ),
        7
    );
}

#[test]
fn compiles_and_runs_a_c_call_through_a_helper() {
    // The call site is a compiled body, not the entry point — which is what
    // exercises the naming: the island emits an ordinary call to `tl_c-abs`
    // and resolves it against this module like any other.
    assert_eq!(
        compile_and_run(
            "ffi_helper",
            r#"
            (defffi (c-abs "abs") (i32) i32)
            (defun magnitude ((n i32)) int (as int (unsafe (c-abs n))))
            (defun main () int (+ (magnitude -3) (magnitude 4)))
            "#
        ),
        7
    );
}

#[test]
fn compiles_and_runs_a_c_call_taking_a_string() {
    // The string conversions are `rt_*` shims, so this also checks that the
    // thunk's calls to them resolve against the static library.
    assert_eq!(
        compile_and_run(
            "ffi_strlen",
            r#"
            (defffi (c-strlen "strlen") (string) c-ulong)
            (defun main () int (as int (unsafe (c-strlen "hello"))))
            "#
        ),
        5
    );
}

#[test]
fn compiles_and_runs_a_constant_main() {
    assert_eq!(compile_and_run("answer", "(defun main () int 42)"), 42);
}

#[test]
fn compiles_and_runs_arithmetic_in_main() {
    // `int`: a bare integer literal with no surrounding type
    // context (no typed parameter or `let` — `main` can't have either,
    // see this module's doc comment) always defaults to `int` (the
    // checker resolves `+`/`-`/`*`'s receiver type from its *first*
    // argument checked with `expected: None`, by design — see
    // `Checker::try_instance_method`'s doc comment — so the outer
    // expectation from `main`'s declared return type never reaches it).
    // That's a type-checking-time distinction, not a codegen one: the core
    // IR's literal node is `int-any-width` and the island's
    // `compile-int-any-width` builds an `i64` constant whatever the
    // declared width, because a narrow integer *is* its number in a wider
    // carrier. The width still exists — it decides what the arithmetic
    // normalizes to and which box the value gets on its way into a `Sexpr`
    // — it just isn't what a literal's own IR looks like.
    assert_eq!(compile_and_run("arith", "(defun main () int (- (* 6 8) (+ 4 2)))"), 42);
}

#[test]
fn compiles_a_file_with_a_non_main_helper_function_too() {
    // `add2` specifically isn't called from `main` here (see
    // `compiles_and_runs_main_calling_a_helper_function` for a helper `main`
    // actually calls, labels/closures Stage 3) — this just confirms an
    // uncalled helper still compiles and links into the same executable
    // without colliding with anything.
    let src = r#"
        (defun add2 ((a int) (b int)) int (+ a b))
        (defun main () int 7)
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
        (defun square ((x int)) int (* x x))
        (defun main () int (+ (square 3) (square 4)))
    "#;
    assert_eq!(compile_and_run("calls_helper", src), 25);
}

/// The end-to-end Stage 4 slice (escaping + capturing `lambda`, indirect
/// dispatch through a parameter), AOT side: `main` calls `apply-fn` with
/// `(adder 5)` directly — `main` still can't have parameters/`let`s of its
/// own, but every value here comes from ordinary calls with literal
/// arguments, which `main`'s body can already express (see this module's
/// doc comment). Proves the closure box (`build-make-closure`/
/// `coroutine-apply`, a `BoxedObj::CompiledClosure` on the GC heap) links and
/// runs correctly from a real `cc`-built executable, not just under JIT.
#[test]
fn compiles_and_runs_an_escaping_capturing_lambda_through_a_helper() {
    let src = r#"
        (defun adder ((n int)) (fn (int) int) (lambda ((x int)) int (+ x n)))
        (defun apply-fn ((f (fn (int) int)) (n int)) int (f n))
        (defun main () int (apply-fn (adder 5) 10))
    "#;
    assert_eq!(compile_and_run("escaping_lambda", src), 15);
}

/// labels/closures Stage 2 (outer-scope capture) + Stage 3 (top-level
/// `Expr::Call`) together: a non-`main` helper `defun` whose body is a
/// *capturing* `labels` form (`rec` references `add-offset`'s own parameter
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
        (defun add-offset ((offset int) (n int)) int
          (labels ((rec ((k int)) int (+ k offset)))
            (rec n)))
        (defun main () int (add-offset 10 5))
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
        (defun main () int
          (labels ((square ((x int)) int (* x x))
                   (sum-helper ((x int) (y int)) int (+ (square x) (square y))))
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
        (defun fact ((n int)) int (if (<= n 1) 1 (* n (fact (- n 1)))))
        (defun main () int (fact 5))
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
        (defun sum-to ((n int)) int
          (let ((i 0) (acc 0))
            (loop
              (if (> i n) (return acc) ())
              (setf acc (+ acc i))
              (setf i (+ i 1)))))
        (defun main () int (sum-to 5))
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
        (defun nested-loop-test () int
          (let ((outer 0) (total 0))
            (loop
              (if (eq outer 3) (return total) ())
              (let ((inner 0))
                (loop
                  (if (eq inner 2) (break) ())
                  (setf total (+ total 1))
                  (setf inner (+ inner 1))))
              (setf outer (+ outer 1)))))
        (defun main () int (nested-loop-test))
    "#;
    assert_eq!(compile_and_run("nested_loop_helper", src), 6);
}

#[test]
fn exit_code_is_truncated_to_32_bits() {
    // Process exit codes are a byte on POSIX (`status.code()` already
    // reports the OS's truncated view) — 256 wraps to 0, matching the
    // wrapping down through the wrapper's `int` truncation.
    assert_eq!(compile_and_run("wraps", "(defun main () int 256)"), 0);
}

#[test]
fn errors_without_a_zero_argument_main() {
    let dir = tmp_dir();
    let src_path = dir.join("no_main.typl");
    let out_path = dir.join("no_main");
    std::fs::write(&src_path, "(defun answer () int 42)").unwrap();

    let err = typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect_err("expected a missing-entry-point error");
    assert!(err.contains("main"), "error was: {}", err);
}

#[test]
fn errors_on_a_non_defun_top_level_form() {
    let dir = tmp_dir();
    let src_path = dir.join("bad_top_level.typl");
    let out_path = dir.join("bad_top_level");
    std::fs::write(&src_path, "(defun main () int 42)\n(+ 1 2)").unwrap();

    let err = typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect_err("expected an unsupported-top-level-form error");
    assert!(err.contains("defun"), "error was: {}", err);
}

/// A trailing `(main)` — the line that starts the program under `typl
/// file.typl` — is accepted and dropped, so one source file runs either way.
/// Only that: `(main 1)` and any other expression are still refused.
#[test]
fn a_top_level_entry_call_is_the_one_expression_allowed() {
    assert_eq!(compile_and_run("entry_call_ok", "(defun main () int 7)\n(main)"), 7);

    let dir = tmp_dir();
    let src_path = dir.join("entry_call_with_args.typl");
    let out_path = dir.join("entry_call_with_args");
    std::fs::write(&src_path, "(defun main () int 7)\n(defun f ((n int)) int n)\n(f 1)").unwrap();
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

    // `int` — see `compiles_and_runs_arithmetic_in_main`'s doc
    // comment.
    let src = "(defun main () int (- (* 6 8) (+ 4 2)))";

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
        (defun main () int
          (labels ((square ((x int)) int (* x x))
                   (sum-helper ((x int) (y int)) int (+ (square x) (square y))))
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
        (defun square ((x int)) int (* x x))
        (defun main () int (+ (square 3) (square 4)))
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
        (defun adder ((n int)) (fn (int) int) (lambda ((x int)) int (+ x n)))
        (defun apply-fn ((f (fn (int) int)) (n int)) int (f n))
        (defun main () int (apply-fn (adder 5) 10))
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
        (defun fact ((n int)) int (if (<= n 1) 1 (* n (fact (- n 1)))))
        (defun main () int (fact 5))
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
        (defun sum-to ((n int)) int
          (let ((i 0) (acc 0))
            (loop
              (if (> i n) (return acc) ())
              (setf acc (+ acc i))
              (setf i (+ i 1)))))
        (defun main () int (sum-to 5))
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
        (defvar (answer int) 42)
        (defun main () int answer)
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
        (defvar (counter int) 0)
        (defun bump () int (setf counter (+ counter 1)))
        (defun main () int (let ((ignored (bump))) (bump)))
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
        (defvar (counter int) 0)
        (defun bump () int (setf counter (+ counter 1)))
        (defun main () int (let ((ignored (bump))) (bump)))
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
        (defenum shape (Circle int) (Rect int int))
        (defvar (s shape) (shape::Rect 6 7))
        (defun main () int (match s ((Circle r) (* r r)) ((Rect w h) (* w h))))
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
        (defenum counter (At int))
        (defvar (c counter) (counter::At 0))
        (defun bump () int
          (match c ((At n) (progn (setf c (counter::At (+ n 1))) (+ n 1)))))
        (defun main () int (let ((ignored (bump))) (bump)))
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
            (defstruct point (x int) (y int))
            (defmethod norm ((self point)) int (+ (* self::x self::x) (* self::y self::y)))
            (defun main () int (as int (norm (point::new 3 4))))
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
            (deftrait Counted () (count ((self Self)) int))
            (defstruct box-a (n int))
            (impl Counted box-a (count ((self Self)) int self::n))
            (defun main () int (as int (count (box-a::new 9))))
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
            (deftrait Drawable () (draw ((self Self)) int))
            (defstruct circle (r int))
            (defstruct square (side int))
            (impl Drawable circle (draw ((self Self)) int 1))
            (impl Drawable square (draw ((self Self)) int 2))
            (defun render ((d :dyn Drawable)) int (draw d))
            (defun main () int
              (as int (+ (* 10 (render (circle::new 3))) (render (square::new 4)))))
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
            (deftrait B () (b-tag ((self Self)) int))
            (deftrait C () (c-tag ((self Self)) int))
            (deftrait D (B C) (d-tag ((self Self)) int))
            (defstruct cell (n int))
            (defstruct pair (n int))
            (impl B cell (b-tag ((self Self)) int 1))
            (impl C cell (c-tag ((self Self)) int 2))
            (impl D cell (d-tag ((self Self)) int 3))
            (impl B pair (b-tag ((self Self)) int 4))
            (impl C pair (c-tag ((self Self)) int 5))
            (impl D pair (d-tag ((self Self)) int 6))
            (defun only-c ((c :dyn C)) int (c-tag c))
            (defun via ((d :dyn D)) int (only-c d))
            (defun main () int (as int (+ (* 10 (via (cell::new 0))) (via (pair::new 0)))))
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
              (draw ((self Self)) int)
              (sides ((self Self)) int))
            (defstruct tri (n int))
            (impl Shape tri
              (draw ((self Self)) int 7)
              (sides ((self Self)) int 3))
            (defun paint ((s :dyn Shape)) int (draw s))
            (defun outline ((s :dyn Shape)) int (sides s))
            (defun main () int (as int (+ (* 10 (paint (tri::new 1))) (outline (tri::new 1)))))
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
    let (code, stderr) = compile_and_capture("panics", r#"(defun main () int (panic "from aot"))"#);
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
        (defun inner ((n int)) int (if (< n 0) (panic "negative") n))
        (defun outer ((n int)) int (inner n))
        (defun main () int (as int (outer -1)))
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
            (defun deep ((n int)) int (throw 'done (* n 2)))
            (defun middle ((n int)) int (+ 1 (deep n)))
            (defun main () int (as int (catch 'done (middle 21))))
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
            (defvar (ran int) 0)
            (defun body () int (unwind-protect (throw 'done 40) (setf ran 2)))
            (defun main () int (as int (+ (catch 'done (body)) ran)))
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
        (defun thrower () int (throw 'nobody 1))
        (defun main () int (as int (thrower)))
        "#,
    );
    assert_eq!(code, 1, "stderr was: {}", stderr);
    assert!(stderr.contains("no enclosing (catch 'nobody)"), "stderr was: {}", stderr);
}

/// A runtime failure the *runtime* raises — here a zero divisor inside
/// `rt_int_div` — ends an AOT executable the same way an explicit
/// `(panic ...)` does: message on stderr, exit code 1.
///
/// It used to abort with 134 instead, because `rt_int_div` reached
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
        (defun zero () int 0)
        (defun main () int (as int (/ 5 (zero))))
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
        (defvar (ran int) 0)
        (defun zero () int 0)
        (defun body () int (unwind-protect (/ 5 (zero)) (setf ran 1)))
        (defun main () int (as int (+ (body) ran)))
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
            (defvar (ran int) 0)
            (defun body () int (progn (loop (unwind-protect (break) (setf ran 40))) ran))
            (defun main () int (as int (+ (body) 2)))
            "#
        ),
        42
    );
}

// ---- `eval` in a standalone executable ---------------------------------
//
// The last builtin that used to make a caller uncompilable. Unlike the other
// ten shims defined outside `typelisp-rt`, `eval` needs a whole checker and
// interpreter, which `typelisp_front::shim::rt_eval_init` builds at startup
// out of the source text `compile-file` embeds. What these tests are really
// about is that the environment it builds is *the running program's* — same
// heap, same global storage, same definitions — and not a second one beside
// it.

/// The base case: an eval'd expression that mentions nothing of the program.
#[test]
fn an_aot_executable_evaluates_a_form_at_runtime() {
    assert_eq!(
        compile_and_run(
            "aot_eval_expression",
            r#"
            (defun main () int
              (match (eval (quote (+ 40 2)))
                ((ok v) (as int (sexpr-int v)))
                ((err _) -1)))
            "#
        ),
        42
    );
}

/// The eval'd form calls one of the program's own functions — so the startup
/// replay has to have registered it, which is the whole reason the source is
/// embedded rather than just the prelude being loaded.
#[test]
fn an_aot_executable_evaluates_a_call_to_its_own_function() {
    assert_eq!(
        compile_and_run(
            "aot_eval_own_function",
            r#"
            (defun double ((n int)) int (* n 2))
            (defun main () int
              (match (eval (quote (double 21)))
                ((ok v) (as int (sexpr-int v)))
                ((err _) -1)))
            "#
        ),
        42
    );
}

/// The eval'd form reads a global the *compiled* code wrote. Storage is
/// shared through `Interp::bind_compiled_global`, not copied: without it the
/// replay's own `defvar` cell would answer, and the answer would be the
/// initializer rather than what `main` has since stored — silently.
#[test]
fn an_aot_executable_evaluates_a_read_of_a_global_the_program_wrote() {
    assert_eq!(
        compile_and_run(
            "aot_eval_global_read",
            r#"
            (defvar (counter int) 1)
            (defun main () int
              (progn
                (setf counter 42)
                (match (eval (quote counter))
                  ((ok v) (as int (sexpr-int v)))
                  ((err _) -1))))
            "#
        ),
        42
    );
}

/// And the other direction: an eval'd write lands where the compiled code
/// reads. Same slot or the test cannot pass.
#[test]
fn an_aot_executable_sees_a_global_an_evaluated_form_wrote() {
    assert_eq!(
        compile_and_run(
            "aot_eval_global_write",
            r#"
            (defvar (counter int) 1)
            (defun main () int
              (progn
                (eval (quote (setf counter 42)))
                (as int counter)))
            "#
        ),
        42
    );
}

/// A `defvar` initializer must run exactly once. The program's compiled
/// global-init sequence is what runs it; the startup replay skips any
/// `defvar` whose global already has a compiled slot. Counted rather than
/// asserted about directly: the initializer bumps a second global, so a
/// second run shows up as 2.
#[test]
fn an_aot_executable_runs_each_defvar_initializer_once() {
    assert_eq!(
        compile_and_run(
            "aot_eval_single_init",
            r#"
            (defvar (times int) 0)
            (defun bump () int (progn (setf times (+ times 1)) 7))
            (defvar (v int) (bump))
            (defun main () int
              (match (eval (quote times))
                ((ok r) (as int (sexpr-int r)))
                ((err _) -1)))
            "#
        ),
        1
    );
}

/// The eval'd form defines something, and a later eval uses it — the
/// environment is durable across calls, not rebuilt per call.
#[test]
fn an_aot_executable_keeps_definitions_made_by_an_evaluated_form() {
    assert_eq!(
        compile_and_run(
            "aot_eval_definition",
            r#"
            (defun main () int
              (progn
                (eval (quote (defun tripled ((n int)) int (* n 3))))
                (match (eval (quote (tripled 14)))
                  ((ok v) (as int (sexpr-int v)))
                  ((err _) -1))))
            "#
        ),
        42
    );
}

/// `(task ...)` inside an eval'd form is admitted to the *same* scheduler
/// `main` runs under — one `Interp`, not two schedulers that never see each
/// other. `main` asks `eval` to spawn `mark` and then `sleep`s; if the
/// program still ran two schedulers, the task `task` admitted would sit in the
/// `Interp`'s own queue until some *later* `rt_eval` call serviced it (the
/// AOT-scheduler work's stated limitation), so `trail` would still be empty
/// when `main` wakes. It is not: `main`'s own `sleep` blocks its task on the
/// one shared scheduler, which is exactly the opportunity `mark` needs to
/// run.
#[test]
fn an_evaluated_task_runs_while_the_program_waits() {
    assert_eq!(
        compile_and_run(
            "aot_eval_task_runs_concurrently",
            r#"
            (defvar (trail string) "")
            (defun mark () int (progn (setf trail (append trail "e")) 0))
            (defun main () int
              (progn
                (match (eval (quote (task (mark))))
                  ((ok _) ())
                  ((err _) ()))
                (sleep 0.05)
                (if (equal trail "e") 0 1)))
            "#
        ),
        0
    );
}

// ---- modules and `use` -----------------------------------------------------
//
// `compile-file` used to reject `use` outright and mangle a `defun` inside a
// `(module ...)` to its bare last segment (`m::f` compiled to a symbol
// `resolve_fn_def` could only find by looking up `f` at the root — "no such
// function: f"). It now reads through the same `Loader` `typl file.typl`
// does for its dependencies (the entry file's own items stay at the root
// namespace, unlike `typl`'s), so a module-qualified name compiles to its
// full path and a `use`d file is pulled in and compiled alongside.

/// A `defun` inside a `(module ...)` block, called through its full path —
/// the module needs no separate file, just the checker's own handling of a
/// literal `(module ...)` form.
#[test]
fn a_module_qualified_function_runs_in_a_standalone_executable() {
    assert_eq!(
        compile_and_run(
            "module_qualified_fn_aot",
            r#"
            (module m (pub defun f ((x int)) int (* x 2)))
            (defun main () int (m::f 21))
            "#
        ),
        42
    );
}

/// A `(use ...)` naming a *sibling file* under the same source root: the
/// dependency is loaded, checked and compiled into the same executable, and
/// a call through its module-qualified name resolves and runs.
#[test]
fn a_used_sibling_file_is_compiled_into_the_executable() {
    let dir = tmp_dir();
    let lib_path = dir.join("used_sibling_lib.typl");
    std::fs::write(
        &lib_path,
        r#"(pub defun helper ((x int)) int (+ x 100))"#,
    )
    .expect("failed to write the dependency source file");
    let out_path = dir.join("used_sibling_aot");
    let src_path = dir.join("used_sibling_aot.typl");
    std::fs::write(
        &src_path,
        r#"
        (use used_sibling_lib)
        (defun main () int (used_sibling_lib::helper 5))
        "#,
    )
    .expect("failed to write test source file");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    let out = Command::new(&out_path).output().expect("failed to run the compiled executable");
    assert_eq!(out.status.code(), Some(105), "stderr was: {}", String::from_utf8_lossy(&out.stderr));
}

/// A `defun main` nested in a `(module ...)` in the entry file can never be
/// reached as the entry point — `compile_file`'s entry symbol is always the
/// root `main` (see `compile::aot`'s module doc comment, "## use and
/// modules") — so it is rejected at compile time rather than silently
/// compiled as unreachable code under a confusing name.
#[test]
fn a_main_nested_in_a_module_in_the_entry_file_is_rejected() {
    let dir = tmp_dir();
    let src_path = dir.join("nested_main_in_entry_aot.typl");
    std::fs::write(
        &src_path,
        r#"
        (module m (defun main () int 0))
        (defun main () int 0)
        "#,
    )
    .expect("failed to write test source file");
    let out_path = dir.join("nested_main_in_entry_aot");
    let err = typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect_err("a nested `main` must be rejected");
    assert!(err.contains("main"), "error was: {err}");
}

/// The same rejection reaches a `use`d dependency file's own `main` too —
/// `collect_aot_item` runs the same check for a dependency's items as for
/// the entry file's own, so there is no separate case to add for it.
#[test]
fn a_main_in_a_used_dependency_file_is_rejected() {
    let dir = tmp_dir();
    let lib_path = dir.join("dep_with_main_lib.typl");
    std::fs::write(&lib_path, r#"(defun main () int 0)"#).expect("failed to write the dependency source file");
    let src_path = dir.join("dep_with_main_aot.typl");
    std::fs::write(
        &src_path,
        r#"
        (use dep_with_main_lib)
        (defun main () int 0)
        "#,
    )
    .expect("failed to write test source file");
    let out_path = dir.join("dep_with_main_aot");
    let err = typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect_err("a used dependency's own `main` must be rejected");
    assert!(err.contains("main"), "error was: {err}");
}

/// The combination Phase 3 and Phase 4 of the AOT-scheduler work land
/// together for: an `eval`'d form referring to a `use`d dependency's
/// function. The eval environment is captured at compile time by an
/// independent second read of the same source (`compile::dump::
/// capture_program_dump`) — proving that read also resolves `use` against
/// the same source root, not just the one `compile_file`'s own checker did.
#[test]
fn an_evaluated_form_resolves_a_used_module() {
    let dir = tmp_dir();
    let lib_path = dir.join("eval_used_lib.typl");
    std::fs::write(&lib_path, r#"(pub defun helper ((x int)) int (+ x 100))"#)
        .expect("failed to write the dependency source file");
    let out_path = dir.join("eval_used_aot");
    let src_path = dir.join("eval_used_aot.typl");
    std::fs::write(
        &src_path,
        r#"
        (use eval_used_lib)
        (defun main () int
          (match (eval (quote (eval_used_lib::helper 5)))
            ((ok v) (as int (sexpr-int v)))
            ((err _) -1)))
        "#,
    )
    .expect("failed to write test source file");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    let out = Command::new(&out_path).output().expect("failed to run the compiled executable");
    assert_eq!(out.status.code(), Some(105), "stderr was: {}", String::from_utf8_lossy(&out.stderr));
}

/// `examples/projects/http`, AOT-compiled — the example limitation #4 named
/// directly: two files (`main.typl` `use`s `http.typl`), judged from the
/// outside with a real HTTP client, the same shape
/// `an_echo_server_runs_in_a_standalone_executable` uses for the socket
/// layer.
#[test]
fn the_http_example_project_compiles_and_serves_requests() {
    use std::io::Read;
    use std::net::TcpStream;

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_path = manifest_dir.join("examples/projects/http/src/main.typl");
    let out_path = tmp_dir().join("http_example_aot");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to reserve a port");
    let port = listener.local_addr().expect("failed to read the reserved port").port();
    drop(listener);

    let mut child = Command::new(&out_path)
        .args(["serve", &port.to_string()])
        .spawn()
        .expect("failed to start the compiled executable");

    let mut response = String::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let stream = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => break s,
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                panic!("could not connect to the compiled http server: {}", e);
            }
        }
    };
    let mut stream = stream;
    use std::io::Write;
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").expect("failed to send the request");
    stream.read_to_string(&mut response).expect("failed to read the response");
    let _ = child.kill();
    let _ = child.wait();

    assert!(response.starts_with("HTTP/1.1 200"), "response was: {}", response);
    assert!(response.contains("hello from typelisp"), "response was: {}", response);
}

/// A program that never calls `eval` must not pay for it. The checker and the
/// interpreter reach an executable only by being referenced, so the guard is
/// the absence of any `typelisp_front` symbol — the same measurement the
/// printer's registration block is guarded by.
#[test]
fn an_executable_that_never_evaluates_carries_no_interpreter() {
    let dir = tmp_dir();
    let src_path = dir.join("no_eval.typl");
    let out_path = dir.join("no_eval");
    std::fs::write(&src_path, "(defun main () int 42)").expect("failed to write test source file");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");

    let nm = Command::new("nm").arg(&out_path).output().expect("failed to run nm");
    let symbols = String::from_utf8_lossy(&nm.stdout);
    let front: Vec<&str> = symbols.lines().filter(|l| l.contains("typelisp_front")).collect();
    assert!(
        front.is_empty(),
        "an executable with no `eval` in it linked {} front-end symbol(s): {:?}",
        front.len(),
        &front[..front.len().min(10)]
    );
}

/// A *generic* prelude function, instantiated for the first time by an eval'd
/// form. Nothing in the program calls `pathname-name`, so the executable
/// carries no instantiation of it — only the retained template, restored from
/// the snapshot (`Checker::from_state`), can answer this.
///
/// The sharpest test of the snapshot there is: 61 of the prelude's 86 `defun`s
/// are generic, the live templates hold raw heap `Value`s, and getting them
/// back means rebuilding every one through the heap's own allocation APIs.
#[test]
fn an_aot_executable_instantiates_a_generic_from_a_restored_template() {
    assert_eq!(
        compile_and_run(
            "aot_eval_generic_template",
            r#"
            (defun main () int
              (match (eval (quote (if (is-some (pathname-name "/a/b.txt")) 42 0)))
                ((ok v) (as int (sexpr-int v)))
                ((err _) -1)))
            "#
        ),
        42
    );
}

/// A prelude *macro* expanded by an eval'd form. A macro is two halves — the
/// `MacroDef` the checker dispatches on and the body the interpreter expands
/// with — and they are restored by two different halves of the snapshot (the
/// registry, and re-executing the checked `defmacro` form). Either one missing
/// and this fails.
#[test]
fn an_aot_executable_expands_a_prelude_macro_from_a_snapshot() {
    assert_eq!(
        compile_and_run(
            "aot_eval_prelude_macro",
            r#"
            (defun main () int
              (match (eval (quote (length (with-output-to-string (s) (write-string "hello" s)))))
                ((ok v) (as int (sexpr-int v)))
                ((err _) -1)))
            "#
        ),
        5
    );
}

// ---- the prelude in an AOT executable ------------------------------------

/// A compiled executable can call the prelude, not just the builtins and its
/// own definitions.
///
/// `compile-file` used to load the island and nothing else, so `abs`, `gcd`,
/// `identity`, `to-string` and every other prelude definition answered "no
/// such function" at compile time — while the *JIT* (`(compile name)`) had
/// them all, because that runs inside a process where the prelude is loaded.
/// The executable now carries the prelude's compiled bodies itself.
#[test]
fn an_aot_executable_calls_prelude_functions() {
    let src = r#"
        (defun main () int
          (+ (abs -30) (gcd 8 12) (if (zerop 0) 8 0)))
    "#;
    assert_eq!(compile_and_run("aot_prelude_call", src), 42);
}

/// A prelude *generic* has no compiled body to carry — the checker
/// monomorphizes it per use site — so this goes the other route: the
/// instantiation is checked into this file and compiled with the file's own
/// definitions.
#[test]
fn an_aot_executable_calls_a_generic_prelude_function() {
    let src = r#"
        (defun main () int
          (let ((v (the Vector<int> (Vector::new))))
            (push v 20)
            (push v 22)
            (+ (length (iter v)) 40)))
    "#;
    assert_eq!(compile_and_run("aot_prelude_generic", src), 42);
}

/// A value pattern — a `string` literal in a `match` — inside an AOT
/// executable.
///
/// It is the pattern whose test is a *call*: `(equals $match-scrut "add")`,
/// resolved to the prelude's `impl Eq string`. So this is the case where a
/// pattern drags a prelude method into the executable, and it says the
/// dependency is followed from inside a pattern the same way it is from a
/// body (`core_bridge::collect_targets` walks a pattern's fields like any
/// other node's).
#[test]
fn an_aot_executable_matches_on_a_string_literal() {
    let src = r#"
        (defun op ((s string)) int
          (match s ("add" 40) ("sub" 1) (_ 0)))
        (defun main () int
          (+ (op "add") (+ (op "sub") (op "nope"))))
    "#;
    assert_eq!(compile_and_run("aot_match_string", src), 41);
}

/// The prelude's globals get their storage at the executable's own startup,
/// in the order their compiled slot ids were assigned — a prelude body reading
/// `*print-right-margin*` reads the value the prelude's own `defvar` gave it,
/// not slot 0 of somebody else's numbering.
#[test]
fn an_aot_executable_initializes_the_preludes_globals() {
    let src = r#"
        (defun main () int
          (+ (as int *print-right-margin*) (if *print-pretty* 1 -38)))
    "#;
    assert_eq!(compile_and_run("aot_prelude_globals", src), 42);
}

/// The operating-system builtins taken with `libc` (2026-09-05) reach a
/// standalone executable.
///
/// An AOT binary has no interpreter and no extern *address* table: the shims
/// are ordinary linker symbols the island declared, so a missing entry shows
/// up only here. The exit code carries the answers — a hostname, a CPU time,
/// a local zone and a file owner all had to come back for it to be 0.
#[test]
fn the_system_information_builtins_run_in_an_executable() {
    let src = r#"
(defun main () int
  (let ((host (is-some (machine-instance)))
        (rel (is-some (software-version)))
        (cpu (> (internal-time-seconds (get-internal-run-time)) 0.0))
        (zone (match (timezone-offset-seconds 46000 0) ((some _) true) ((none) false)))
        (dst (match (timezone-daylight-p 46000 0) ((some _) true) ((none) false)))
        (owner (is-ok (file-author "Cargo.toml"))))
    (if (and host (and rel (and cpu (and zone (and dst owner))))) 0 1)))
"#;
    assert_eq!(compile_and_run("sysinfo_aot", src), 0);
}

/// `decode-universal-time`'s no-zone default is CL's local time, in a
/// standalone executable too — the path that calls `localtime_r` through the
/// prelude rather than through the interpreter.
#[test]
fn local_time_decoding_runs_in_an_executable() {
    let src = r#"
(defun main () int
  (let ((now (get-decoded-time)))
    (if (and (> now::year 2020)
             (and (>= now::hour 0) (< now::hour 24)))
        0
        1)))
"#;
    assert_eq!(compile_and_run("localtime_aot", src), 0);
}

// ---- tasks in a standalone executable -------------------------------------
//
// An AOT executable used to have no scheduler: its `main` was one
// `FrameStack::run`, so the first thing that put a task down — `sleep`,
// `wait`, a channel, a socket that said "not yet" — was an abort, and `task`
// refused outright ("needs an interpreter to run the task in"). The runtime
// now carries the scheduler itself (`typelisp_rt::sched`), and these are the
// same programs `tests/concurrency_test.rs` and friends run under the
// interpreter, compiled to an executable and judged by their exit code.

/// [`compile_and_capture`] for a program that may not exit normally, run
/// with `args`: the exit code (`None` for a signal), stdout and stderr.
fn compile_and_capture_with(name: &str, source: &str, args: &[&str]) -> (Option<i32>, String, String) {
    let out_path = compile_only(name, source);
    let out = Command::new(&out_path).args(args).output().expect("failed to run the compiled executable");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Writes and compiles `source` under `name`, returning the executable's path.
fn compile_only(name: &str, source: &str) -> PathBuf {
    let dir = tmp_dir();
    let src_path = dir.join(format!("{}.typl", name));
    let out_path = dir.join(name);
    std::fs::write(&src_path, source).expect("failed to write test source file");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    out_path
}

/// `(task ...)` in a standalone executable starts a task, and `wait` gets its
/// answer — for an `i32` and for a `string`, because the answer crosses as
/// one word and a raw-word delivery would pass exactly one of the two.
#[test]
fn task_and_wait_run_in_a_standalone_executable() {
    let (code, err) = compile_and_capture(
        "task_wait_aot",
        r#"(defun work ((n int)) int (* n 2))
           (defun greet ((name string)) string (append "hi " name))
           (defun main () int
             (let ((a (task (work 21))) (b (task (greet "ada"))))
               (if (and (= (wait a) 42) (equal (wait b) "hi ada")) 0 1)))"#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// `sleep` stops the task, not the thread: `a` is spawned first and would
/// finish first if the whole process slept, so `ba` is the whole assertion
/// (the same proof `concurrency_test` makes under the interpreter).
#[test]
fn sleep_in_a_standalone_executable_stops_only_its_task() {
    let (code, err) = compile_and_capture(
        "sleep_aot",
        r#"(defvar (trail string) "")
           (defun slow () int (progn (sleep 0.08) (setf trail (append trail "a")) 0))
           (defun fast () int (progn (sleep 0.02) (setf trail (append trail "b")) 0))
           (defun main () int
             (let ((a (task (slow))) (b (task (fast))))
               (progn (wait a) (wait b) (if (equal trail "ba") 0 1))))"#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// A rendezvous on an unbuffered channel, a buffered send, and a `select`,
/// all in an executable: one scheduler answers the compiled channel
/// operations exactly as it answers the interpreted ones.
#[test]
fn channels_and_select_run_in_a_standalone_executable() {
    let (code, err) = compile_and_capture(
        "chan_aot",
        r#"(defun producer ((ch Chan<int>)) ()
             (send ch 1) (send ch 2) (close ch) ())
           (defun drain ((ch Chan<int>)) int
             (let ((acc 0))
               (loop (match (recv ch)
                       ((none) (return acc))
                       ((some v) (setf acc (+ acc v)))))))
           (defun main () int
             (let ((r (the Chan<int> (Chan::new 0)))
                   (b (the Chan<string> (Chan::new 1))))
               (task (producer r))
               (send b "x")
               (let ((sum (drain r))
                     (picked (select ((v (recv r)) "r") ((v (recv b)) (format false "b=~a" v)))))
                 (if (and (= sum 3) (equal picked "b=(some x)")) 0 1))))"#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// A compiled loop's back edge polls a safepoint every 256 iterations
/// (Phase C7), and the offer used to reach the AOT entry as a suspension it
/// could not honour — `main` ran the chain with `FrameStack::run`, which has
/// no arm for it — so any loop past that count aborted the process. The
/// scheduler answers the offer like a `yield`.
#[test]
fn a_long_compiled_loop_runs_in_a_standalone_executable() {
    let (code, err) = compile_and_capture(
        "long_loop_aot",
        r#"(defun loop-sum ((n int)) int
             (let ((i 0) (acc 0))
               (loop (if (>= i n) (break) ())
                     (setf acc (+ acc i))
                     (setf i (+ i 1)))
               acc))
           (defun main () int (if (= (loop-sum 100000) 4999950000) 0 1))"#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// A `defvar` initialiser is an ordinary task under the program's
/// scheduler now, not a call on a machine frame with nowhere to put a
/// suspension down — so it may make a channel, `task`, or `wait` exactly like
/// any other code. Used to abort with "a compiled callee suspended under a
/// call that has to return" the moment any initialiser did.
#[test]
fn a_defvar_initialiser_may_make_a_channel_in_a_standalone_executable() {
    let (code, err) = compile_and_capture(
        "defvar_chan_aot",
        r#"(defvar (ch Chan<int>) (Chan::new 2))
           (defun main () int
             (progn (send ch 5)
                    (match (recv ch)
                      ((some v) (if (= v 5) 0 1))
                      ((none) 2))))"#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// The same, for an initialiser that actually waits — `task` then `wait`,
/// which parks the initialiser's task until the one it started finishes.
#[test]
fn a_defvar_initialiser_may_wait_in_a_standalone_executable() {
    let (code, err) = compile_and_capture(
        "defvar_wait_aot",
        r#"(defun answer () int 42)
           (defvar (x int) (wait (task (answer))))
           (defun main () int (if (= x 42) 0 1))"#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// `print-object`, reached from Rust (the printer's door,
/// `typelisp_print::aot::aot_print_object`) rather than from another
/// compiled or interpreted call, may still use a channel operation that
/// needs no waiting — `send`/`recv` on a channel it just made and filled
/// itself. Used to abort with "a compiled callee suspended under a call
/// that has to return".
#[test]
fn a_print_object_method_may_use_a_channel_in_a_standalone_executable() {
    let (code, out, err) = compile_and_capture_with(
        "po_chan_aot",
        r##"(defstruct point (x int) (y int))
           (defmethod print-object ((self point) (escape bool)) string
             (let ((ch (the Chan<int> (Chan::new 1))))
               (send ch 7)
               (format false "#<p ~a ~a>" self::x (recv ch))))
           (defun main () int (progn (println "~a" (point::new 1 2)) 0))"##,
        &[],
    );
    assert_eq!(code, Some(0), "stderr was: {}", err);
    assert_eq!(out.trim(), "#<p 1 (some 7)>");
}

/// The same door, but the operation genuinely would have to wait (an empty,
/// unfilled channel): refused as a language-level panic with the same
/// wording the interpreter's `run_to_completion` uses — not the process
/// abort the machine-frame driver used to answer with.
#[test]
fn a_print_object_method_that_would_block_is_refused_not_aborted() {
    let (code, _, err) = compile_and_capture_with(
        "po_chan_block_aot",
        r#"(defstruct probe (ch Chan<int>))
           (defmethod print-object ((self probe) (escape bool)) string
             (format false "<~a>" (recv self::ch)))
           (defun main () int
             (let ((ch (the Chan<int> (Chan::new 1))))
               (progn (println "~a" (probe::new ch)) 0)))"#,
        &[],
    );
    assert_eq!(code, Some(1), "stderr was: {}", err);
    assert!(err.contains("`recv` cannot block"), "stderr was: {}", err);
}

/// Every task blocked on something that will never happen stops the
/// program with the scheduler's own message, not a hang and not an abort.
#[test]
fn a_deadlocked_standalone_executable_says_so() {
    let (code, _, err) = compile_and_capture_with(
        "deadlock_aot",
        r#"(defun main () int
             (let ((ch (the Chan<int> (Chan::new 0))))
               (progn (recv ch) 0)))"#,
        &[],
    );
    assert!(code.is_some(), "the program was killed by a signal: {}", err);
    assert_ne!(code, Some(0), "expected the program to stop");
    assert!(err.contains("every task is blocked"), "stderr was: {}", err);
}

/// An echo server compiled to an executable: `accept` parks the main task on
/// the listener, the connection is served by a task `task` started, and the
/// socket reads inside it park that task — the shape `examples/projects/
/// echo-server` has, judged from the outside with a real TCP client.
#[test]
fn an_echo_server_runs_in_a_standalone_executable() {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};

    let exe = compile_only(
        "echo_aot",
        r#"(defun serve ((c socket-stream)) int
             (loop
               (match (read-line c)
                 ((none) (break))
                 ((some line) (write-line c line))))
             (close c)
             0)
           (defun main () int
             (let ((args (command-line-args)))
               (let ((port (unwrap-or (parse-int (get args 1)) 0)))
                 (match (tcp-listen "127.0.0.1" port)
                   ((err e) 2)
                   ((ok l)
                    (match (accept l)
                      ((err e) 3)
                      ((ok c) (wait (task (serve c))))))))))"#,
    );
    // A free port, found the usual racy way: bind to 0, read it back, let go.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(&exe)
        .arg(port.to_string())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start the echo server");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let stream = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => break s,
            Err(_) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                panic!("the server never listened: {}", e);
            }
        }
    };
    let mut writer = stream.try_clone().unwrap();
    writer.write_all(b"hello
").unwrap();
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).unwrap();
    assert_eq!(line, "hello
");
    drop(writer);
    drop(stream);

    let out = child.wait_with_output().expect("the server did not exit");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr was: {}", err);
}

/// An initialiser that allocates enough to collect at *build* time
/// (`compile_file` runs each one in the interpreter) must not take the
/// forms after it with it: they are checked and waiting in a Rust `Vec`
/// until their own turn. The second `defvar` used to come back as garbage —
/// "found ``" — because nothing rooted it.
#[test]
fn a_collecting_initialiser_leaves_the_later_forms_intact() {
    let (code, err) = compile_and_capture(
        "collecting_init",
        r#"(defun f ((n int)) int (let ((acc 0)) (dotimes (i n) (setf acc (+ acc i))) acc))
           (defvar (x int) (f 100000))
           (defvar (y int) (f 3))
           (defun main () int (if (and (= x 4999950000) (= y 3)) 0 1))"#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// An `eval`-carrying program's trailing `(main)` is dropped the way any
/// other program's is. Building the `eval` environment replays the source, and
/// that replay used to run `(main)` at compile time and record it, so the
/// executable ran the program once more at startup before its real `main`.
/// `main` here leaves a mark per run in a file: none after the build, one
/// after a run.
#[test]
fn an_eval_carrying_program_runs_main_once_and_never_at_compile_time() {
    let dir = tmp_dir();
    let marks = dir.join("aot_eval_trailing_main.marks");
    let _ = std::fs::remove_file(&marks);
    let marks_s = marks.to_string_lossy().replace('\\', "/");
    let src_path = dir.join("aot_eval_trailing_main.typl");
    let out_path = dir.join("aot_eval_trailing_main");
    let source = format!(
        r#"
        (defun main () ()
          (match (with-open-file (o "{}" direction-append) (write-string o "x"))
            ((ok _) ())
            ((err e) (panic (message e))))
          (match (eval (quote (+ 1 2)))
            ((ok _) ())
            ((err e) (panic (message e)))))
        (main)
        "#,
        marks_s
    );
    std::fs::write(&src_path, source).expect("failed to write test source file");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    assert!(!marks.exists(), "compile-file ran `main`");

    let out = Command::new(&out_path).output().expect("failed to run the compiled executable");
    assert_eq!(out.status.code(), Some(0), "stderr was: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(&marks).expect("main left no mark"), "x");
}

/// A `()`-returning `main` that returns normally exits 0 under either entry
/// shim. The `eval`-carrying one used to exit with the word `()` is encoded as.
#[test]
fn an_eval_carrying_unit_main_exits_zero() {
    let (code, err) = compile_and_capture(
        "aot_eval_unit_main",
        r#"
        (defun main () ()
          (match (eval (quote (+ 1 2)))
            ((ok _) ())
            ((err e) (panic (message e)))))
        "#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// A struct evaluates to itself (CL's self-evaluating objects): `eval` hands
/// back the very object, so a write through the result is seen through the
/// original.
#[test]
fn an_aot_executable_evaluates_a_struct_to_itself() {
    let (code, err) = compile_and_capture(
        "aot_eval_struct_self",
        r#"
        (defstruct q (x i32))
        (defun main () int
          (let ((v (q::new 7)))
            (match (eval v)
              ((ok s) (match s
                        ((the q w) (progn (setf w::x 8) (as int v::x)))
                        (_ -1)))
              ((err _) -2))))
        "#,
    );
    assert_eq!(code, 8, "stderr was: {}", err);
}

/// A prelude macro that touches a prelude global while it expands
/// (`with-output-to-string` calls `gensym`, which bumps `*gensym-counter*`)
/// in an `eval`-carrying program. The `eval` environment is checked at
/// compile time, and it used to name the prelude's globals without making
/// their storage, so the expansion failed with "unknown compiled id".
#[test]
fn an_eval_carrying_program_can_use_a_macro_that_calls_gensym() {
    let (code, err) = compile_and_capture(
        "aot_eval_gensym_macro",
        r#"
        (defun main () int
          (let ((s (with-output-to-string (o) (write-string o "ab"))))
            (match (eval (quote (+ 1 2)))
              ((ok _) (length s))
              ((err _) -1))))
        "#,
    );
    assert_eq!(code, 2, "stderr was: {}", err);
}

// ---- C callbacks ---------------------------------------------------------

const QSORT_AOT: &str = r#"
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (c-strcmp "strcmp") (string string) i32)
(defffi (text-of "strstr") (ptr string) string)
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())
(defun desc ((a ptr) (b ptr)) i32 (unsafe (c-strncmp b a 1)))
(defun sorted-with ((s string) (how int)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (if (= how 0)
          (c-qsort buf 4 1 desc)
          (if (= how 1)
              (c-qsort buf 4 1 (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
              (labels ((cmp ((a ptr) (b ptr)) i32 (flip (c-strncmp a b 1)))
                       (flip ((n i32)) i32 (- (the i32 0) n)))
                (c-qsort buf 4 1 cmp))))
      (let ((r (text-of buf ""))) (c-free buf) r))))
"#;

/// A top-level function, a `lambda` and a local function, each handed to
/// `qsort` from an executable: the entries were emitted into it and
/// registered by its `main`.
#[test]
fn an_aot_executable_passes_callbacks_to_c() {
    let (code, err) = compile_and_capture(
        "aot_ffi_callbacks",
        &format!(
            "{}(defun main () int
                 (if (and (= (unsafe (c-strcmp (sorted-with \"cadb\" 0) \"dcba\")) 0)
                          (= (unsafe (c-strcmp (sorted-with \"cadb\" 1) \"abcd\")) 0)
                          (= (unsafe (c-strcmp (sorted-with \"cadb\" 2) \"dcba\")) 0))
                     0 1))",
            QSORT_AOT
        ),
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

/// A `panic` in a callback is raised once `qsort` returns, and ends the
/// executable the way any other `panic` does.
#[test]
fn a_panic_in_a_callback_ends_an_aot_executable_after_the_c_call() {
    let (code, err) = compile_and_capture(
        "aot_ffi_callback_panic",
        &format!(
            "{}(defun main () int
                 (unsafe (c-qsort (c-strdup \"dcba\") 4 1 (lambda ((a ptr) (b ptr)) i32 (panic \"boom\"))))
                 0)",
            QSORT_AOT
        ),
    );
    assert_eq!(code, 1, "stderr was: {}", err);
    assert!(err.contains("boom"), "stderr was: {}", err);
}

/// `def-c-struct` in an executable: an array of C structs allocated inside
/// `unsafe`, sorted by `qsort` with a callback that reads their fields, and
/// a pointer C allocated refused when it is read as a typed pointer.
#[test]
fn an_aot_executable_sorts_c_structs() {
    let (code, err) = compile_and_capture(
        "aot_c_structs",
        r#"
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))
(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1)) (* 10 (key-at xs 2)) (key-at xs 3)))))

(defun main () int (if (= (sorted-keys) 1234) 0 1))
"#,
    );
    assert_eq!(code, 0, "stderr was: {}", err);
}

#[test]
fn an_aot_executable_refuses_a_typed_pointer_into_c_memory() {
    let (code, err) = compile_and_capture(
        "aot_c_struct_c_memory",
        r#"
(defffi (c-malloc "malloc") (c-ulong) (ptr i32))
(defun main () int (unsafe (as int (c-deref (c-malloc 4)))))
"#,
    );
    assert_eq!(code, 1, "stderr was: {}", err);
    assert!(err.contains("does not point into memory an `unsafe` allocated"), "stderr was: {}", err);
}
