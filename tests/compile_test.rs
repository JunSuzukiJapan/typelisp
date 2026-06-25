//! Tests for the LLVM-backed `compile`/`compile-file` machinery (Phase 0:
//! the shared core that lets the (typelisp-hosted) compiler body build an
//! LLVM module at all — `llvm-module`/`llvm-function`/`llvm-builder`/
//! `llvm-value`, see `registry::llvm_module_def` and friends). No AST
//! bridge or JIT/AOT output path exists yet; these only exercise the raw
//! LLVM builtins from typelisp source, the same way `hashtable_test.rs`
//! exercises `HashTable<K,V>`.

extern crate typelisp;
use inkwell::OptimizationLevel;
use typelisp::compile::COMPILE_LOCK;
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

/// The Phase 0 vertical slice: build an `i64`-returning LLVM function (under
/// the fixed `i64 name(i64* args, i32 argc)` ABI every compiled function
/// uses, see `registry::llvm_module_def`'s doc comment) by calling the raw
/// LLVM builtins directly from typelisp source (no AST bridge yet — that's
/// a later phase), and check the resulting IR.
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
    assert!(ir.contains("define i64 @answer("), "IR was:\n{}", ir);
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
/// `compile-function`) compiling the exact shape `ast_bridge::ast_to_sexpr`
/// produces for `Expr::Int` — `'(int 42)` here stands in for what the bridge
/// would build from the real typed AST (the Rust-side `ast_bridge` unit
/// tests already cover that translation in isolation; this covers the
/// compiler body consuming it). No parameters, hence the empty `'()`.
#[test]
fn the_compiler_body_compiles_an_int_literal_node() {
    let ir = expect_str(eval_ok_with_compiler(
        r#"(to-string (compile-function (llvm-module::create "mod") "answer" '() '(int 42)))"#,
    ));
    assert!(ir.contains("define i64 @answer"), "IR was:\n{}", ir);
    assert!(ir.contains("ret i64 42"), "IR was:\n{}", ir);
}

#[test]
fn the_compiler_body_panics_on_an_unsupported_tag() {
    let err = run_with_compiler(r#"(compile-function (llvm-module::create "mod") "answer" '() '(if true))"#)
        .expect_err("expected an unsupported-tag panic");
    match err {
        EvalError::Panic(msg) => assert!(msg.contains("unsupported tag"), "message was: {}", msg),
        other => panic!("expected a Panic, got {:?}", other),
    }
}

/// Exercises `compile-function`'s parameter binding (`bind-params`) and the
/// `(assoc ...)` tag (`compile-assoc`'s `+`/`-`/`*` arms) — the shapes
/// `ast_bridge::ast_to_sexpr` produces for `Expr::Var`/`Expr::Assoc`. The
/// quoted body stands in for what the bridge would build from
/// `(+ a b)`'s typed AST (an `i64::+` instance-method call on two `Var`s).
#[test]
fn the_compiler_body_compiles_a_two_parameter_addition() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "add2" '(a b) '(assoc "i64" "+" true (var "a") (var "b")))"#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    // See `compile::COMPILE_LOCK`'s doc comment — every LLVM-Context-touching
    // call, even from a test driving the raw builtins directly rather than
    // going through `Interp::compile_function`, must hold this.
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let add2 = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("add2")
            .expect("failed to look up the compiled `add2` function")
    };
    let argv: [i64; 2] = [10, 32];
    assert_eq!(unsafe { add2.call(argv.as_ptr(), argv.len() as u32) }, 42);
}

/// labels compilation work (Stage 1): the compiler body compiling
/// `ast_bridge::translate_labels`'s exact tagged shape — two non-recursive
/// `labels` siblings (`f` calls `g` directly, `g` just returns its
/// argument), then the trailing body calling `f` directly too. Both calls
/// go through `compile-apply`'s `(get fn-env ...)` lookup + `build-call`,
/// not nested LLVM arithmetic — proving siblings really do become separate
/// LLVM functions linked by direct calls, the core claim this phase set
/// out to verify.
#[test]
fn the_compiler_body_compiles_a_labels_form_with_a_sibling_call() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "outer" '() '(labels () (("f" (x) (apply "g" (var "x"))) ("g" (n) (var "n"))) (apply "f" (int 5))))"#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let outer = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("outer")
            .expect("failed to look up the compiled `outer` function")
    };
    assert_eq!(unsafe { outer.call(std::ptr::null(), 0) }, 5);
}

/// labels/closures Stage 2 (outer-scope capture): the compiler body
/// compiling `ast_bridge::translate_labels`'s extended shape — a non-empty
/// captured list (`"offset"`, `outer`'s own first parameter) — through
/// `declare-labels-siblings`'s `add-function-with-env` branch,
/// `compile-labels-bodies`'s `bind-captures`, and `compile-apply`'s
/// `build-call-with-env` branch, none of which the Stage 1 sibling-call
/// test above exercises (its captured list is always empty).
#[test]
fn the_compiler_body_compiles_a_labels_form_that_captures_an_outer_scope_value() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "outer" '(offset n) '(labels (offset) (("go" (k) (assoc "i64" "+" true (var "k") (var "offset")))) (apply "go" (var "n"))))"#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let outer = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("outer")
            .expect("failed to look up the compiled `outer` function")
    };
    let argv: [i64; 2] = [3, 4];
    assert_eq!(unsafe { outer.call(argv.as_ptr(), argv.len() as u32) }, 7);
}

/// Hands a real, JIT-executable `llvm-module` back to Rust (rather than its
/// `to-string` dump, like `BUILD_ANSWER_MODULE`) so this test can prove
/// `inkwell::Module::create_jit_execution_engine` actually runs code the
/// typelisp-built IR describes — not just that the IR text looks right.
#[test]
fn the_built_module_actually_jit_executes_to_42() {
    let src = r#"
        (defun build-answer-module-raw () llvm-module
          (let ((m (llvm-module::create "mymod")))
            (let ((f (add-function m "answer")))
              (let ((b (append-block f "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (let ((c (const-i64 builder 42)))
                    (build-ret builder c)
                    m))))))
        (build-answer-module-raw)
    "#;
    let module = match eval_ok(src) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let answer = unsafe {
        engine
            .get_function::<unsafe extern "C" fn() -> i64>("answer")
            .expect("failed to look up the compiled `answer` function")
    };
    assert_eq!(unsafe { answer.call() }, 42);
}

/// Exercises the fixed-ABI parameter convention (`registry::llvm_module_def`'s
/// doc comment: every compiled function is `i64 name(i64* args, i32 argc)`)
/// end to end: `load-arg` pulls two logical parameters out of that array,
/// `build-add` combines them, and the JIT-executed result is checked against
/// the same convention `CompiledFn`/`compile::CompiledFn::call` will use —
/// an `i64` array in, one `i64` out.
#[test]
fn a_function_using_load_arg_and_build_add_computes_correctly() {
    let src = r#"
        (defun build-add-fn-raw () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((f (add-function m "add2")))
              (let ((b (append-block f "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (let ((a (load-arg builder f 0)))
                    (let ((bb (load-arg builder f 1)))
                      (let ((sum (build-add builder a bb)))
                        (build-ret builder sum)
                        m))))))))
        (build-add-fn-raw)
    "#;
    let module = match eval_ok(src) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let add2 = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("add2")
            .expect("failed to look up the compiled `add2` function")
    };
    let argv: [i64; 2] = [3, 4];
    assert_eq!(unsafe { add2.call(argv.as_ptr(), argv.len() as u32) }, 7);
}

/// `get-function`/`alloca-args`/`store-arg`/`build-call`: a direct call from
/// one compiled function to another *already declared in the same module*
/// — the primitive every statically-resolvable call (self-recursion,
/// `labels` siblings, top-level `defun`-to-`defun`) is built on (labels
/// compilation work, Stage 1). Builds `double` (doubles its one argument)
/// and `quadruple` (calls `double` twice via direct calls, not nested LLVM
/// arithmetic), all with the raw builtins — no `ast_bridge`/`compiler.rs`
/// involvement yet, the same way Phase 0/1's raw-builtin tests preceded the
/// self-hosted compiler body.
#[test]
fn a_function_can_directly_call_another_function_in_the_same_module() {
    let src = r#"
        (defun build-quadruple-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((double-fn (add-function m "double")))
              (let ((quad-fn (add-function m "quadruple")))
                (let ((b1 (append-block double-fn "entry")))
                  (let ((builder1 (llvm-builder::create)))
                    (position-at-end builder1 b1)
                    (let ((x (load-arg builder1 double-fn 0)))
                      (build-ret builder1 (build-add builder1 x x)))))
                (let ((b2 (append-block quad-fn "entry")))
                  (let ((builder2 (llvm-builder::create)))
                    (position-at-end builder2 b2)
                    (let ((n (load-arg builder2 quad-fn 0)))
                      (let ((args1 (alloca-args builder2 1)))
                        (store-arg builder2 args1 0 n)
                        (let ((doubled (build-call builder2 (get-function m "double") args1 1)))
                          (let ((args2 (alloca-args builder2 1)))
                            (store-arg builder2 args2 0 doubled)
                            (build-ret builder2 (build-call builder2 (get-function m "double") args2 1))))))))
                m))))
        (build-quadruple-module)
    "#;
    let module = match eval_ok(src) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let quadruple = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("quadruple")
            .expect("failed to look up the compiled `quadruple` function")
    };
    let argv: [i64; 1] = [5];
    assert_eq!(unsafe { quadruple.call(argv.as_ptr(), argv.len() as u32) }, 20);
}

/// The end-to-end Phase 1 slice: `(compile "name")` from typelisp source
/// itself (not by hand-feeding `compile-function` a pre-built `Sexpr`, like
/// the compiler-body tests above), then a later `Expr::Call` of that same
/// function transparently dispatching to the JIT-compiled native code
/// (`Interp::compiled`) instead of tree-walking it.
#[test]
fn compile_dispatches_a_defun_call_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun add2 ((a i64) (b i64)) i64 (+ a b))
        (compile "add2")
        (add2 10 32)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
    }
}

#[test]
fn compile_returns_true_on_success() {
    let v = eval_ok_with_compiler(r#"(defun answer () i64 42) (compile "answer")"#);
    assert!(expect_bool(v));
}

#[test]
fn an_uncompiled_function_still_tree_walks_normally() {
    // Sanity check that `compiled`-table dispatch doesn't break the
    // ordinary path for a function nobody asked to `compile`.
    let v = eval_ok_with_compiler(r#"(defun answer () i64 42) (answer)"#);
    match v {
        RtValue::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
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

/// The end-to-end Stage 1 slice, from real typelisp source (not a hand-fed
/// Sexpr like `the_compiler_body_compiles_a_labels_form_with_a_sibling_call`):
/// `(compile "sum-of-squares")` runs the *whole* pipeline (`ast_bridge`'s
/// `translate_labels`/`translate_apply` included) on a `defun` whose body
/// is a `labels` form with two non-recursive siblings, one calling the
/// other (`sum-of-squares` calls `square` twice; nothing calls itself).
#[test]
fn compile_dispatches_a_defun_with_a_labels_body_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun sum-of-squares ((a i64) (b i64)) i64
          (labels ((square ((x i64)) i64 (* x x))
                   (sum-helper ((x i64) (y i64)) i64 (+ (square x) (square y))))
            (sum-helper a b)))
        (compile "sum-of-squares")
        (sum-of-squares 3 4)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 25),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The end-to-end Stage 2 slice (outer-scope capture), from real typelisp
/// source: `go`'s body references `offset`, the enclosing `defun`'s own
/// parameter — neither its own parameter `k` nor a sibling name — so
/// `ast_bridge`'s `labels_free_vars` must collect it as a real capture, and
/// `(compile "add-offset")` must build `go` under the extended ABI and wire
/// the trailing body's call to pass `offset` through.
#[test]
fn compile_dispatches_a_defun_with_a_capturing_labels_body_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun add-offset ((offset i64) (n i64)) i64
          (labels ((go ((k i64)) i64 (+ k offset)))
            (go n)))
        (compile "add-offset")
        (add-offset 10 5)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The multi-sibling case `compile_dispatches_a_defun_with_a_capturing_labels_body_to_native_code`
/// doesn't exercise: `helper` itself never references `offset`, only its
/// sibling `go` does — so the *block's* free-variable list (not `helper`'s
/// own) must still include `offset`, and `helper` must still receive it (via
/// `bind-captures`, even though its own body never reads it) purely so it
/// can forward it on to `go`. This is the shared-environment design's load-
/// bearing claim (`compile::freevars::labels_free_vars`'s doc comment): if
/// each sibling computed its own narrower captured list instead, `helper`
/// would have no value to forward and this call would fail to compile.
#[test]
fn a_sibling_that_never_references_a_capture_still_forwards_it_to_another_sibling() {
    let v = eval_ok_with_compiler(
        r#"
        (defun choose ((offset i64) (n i64)) i64
          (labels ((helper ((k i64)) i64 (go k))
                   (go ((k i64)) i64 (+ k offset)))
            (helper n)))
        (compile "choose")
        (choose 10 5)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}
