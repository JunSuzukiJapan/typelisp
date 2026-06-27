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
use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue, Value};

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
    run_with_compiler_and_capacity(src, 1 << 16)
}

/// Like [`run_with_compiler`], but with a caller-chosen `Heap` capacity —
/// see [`run_with_compiler_and_prelude_and_capacity`]'s doc comment for why.
fn run_with_compiler_and_capacity(src: &str, capacity: usize) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(capacity);
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

/// Like [`run_with_compiler`], but with the prelude (`prelude::SOURCE` —
/// `while`/`dotimes`/`dolist`/...) loaded too, for tests that exercise a
/// macro built on `loop`/`break`/`return`/`setf` rather than those
/// primitives directly.
fn run_with_compiler_and_prelude(src: &str) -> Result<RtValue, EvalError> {
    run_with_compiler_and_prelude_and_capacity(src, 1 << 16)
}

/// Like [`run_with_compiler_and_prelude`], but with a caller-chosen `Heap`
/// capacity — for a test (Stage 6 of the Sexpr-representation plan, the
/// "Sexprルート挿入パス") that needs a *small* heap to force a real `gc()`
/// partway through a compiled function's execution, the same reason
/// `typelisp-rt`'s own `rt_push_sexpr_root` tests use a tiny capacity rather
/// than the generous default every other test here gets.
fn run_with_compiler_and_prelude_and_capacity(src: &str, capacity: usize) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(capacity);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
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
    // Every tag `ast_bridge` can actually produce now has a real
    // translation (`construct`/`field-get`/`field-set` joined the list in
    // Stage 6, `docs/TODO.md` — `if`/`let`/`bool`/`match` stood in for
    // "not yet supported" here in earlier stages, until each in turn got a
    // real translation) — this test now uses a tag name `compile-value`
    // could never legitimately see, purely to exercise its own fallback
    // `panic`.
    let err = run_with_compiler(r#"(compile-function (llvm-module::create "mod") "answer" '() '(not-a-real-tag))"#)
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
        r#"(compile-function (llvm-module::create "mod") "add2" '((a . 0) (b . 0)) '(assoc "i64" "+" true (0 var "a" false) (0 var "b" false)))"#,
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
        r#"(compile-function (llvm-module::create "mod") "outer" '() '(labels () (("f" ((x . 0)) (apply "g" (0 var "x" false))) ("g" ((n . 0)) (var "n" false))) (apply "f" (0 int 5))))"#,
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
        r#"(compile-function (llvm-module::create "mod") "outer" '((offset . 0) (n . 0)) '(labels ((offset . 0)) (("go" ((k . 0)) (assoc "i64" "+" true (0 var "k" false) (0 var "offset" false)))) (apply "go" (0 var "n" false))))"#,
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

/// `build-make-closure`/`build-closure-apply`: a `ClosureBox` wrapping a
/// capturing function (`add_offset`, declared via `add-function-with-env`,
/// adding its one logical argument to a captured value), called *indirectly*
/// through the closure value rather than `build-call-with-env`'s direct,
/// statically-known-target path (labels/closures Stage 4) — no
/// `ast_bridge`/`compiler.rs` involvement yet, raw builtins only, the same
/// way Phase 0/1's and Stage 1's own raw-builtin tests preceded their
/// self-hosted-compiler-body counterparts. Also the first thing in this
/// codebase to exercise `Builder::build_array_malloc`'s legacy
/// `malloc`-declaring IR helper end to end: this test JIT-executing
/// successfully is the empirical proof that `malloc` resolves under MCJIT
/// with no extra `add_global_mapping` wiring (see
/// `registry::llvm_builder_def`'s doc comment on `build-make-closure` for
/// why that's expected, not a leap of faith).
#[test]
fn a_closure_made_from_a_capturing_function_can_be_called_indirectly() {
    let src = r#"
        (defun build-and-run-closure-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((add-offset-fn (add-function-with-env m "add_offset")))
              (let ((caller-fn (add-function m "caller")))
                (let ((b1 (append-block add-offset-fn "entry")))
                  (let ((builder1 (llvm-builder::create)))
                    (position-at-end builder1 b1)
                    (let ((x (load-arg builder1 add-offset-fn 0)))
                      (let ((offset (load-env builder1 add-offset-fn 0)))
                        (build-ret builder1 (build-add builder1 x offset))))))
                (let ((b2 (append-block caller-fn "entry")))
                  (let ((builder2 (llvm-builder::create)))
                    (position-at-end builder2 b2)
                    (let ((env-arr (alloca-args builder2 1)))
                      (store-arg builder2 env-arr 0 (const-i64 builder2 100))
                      (let ((closure (build-make-closure builder2 add-offset-fn env-arr 1 0)))
                        (let ((args-arr (alloca-args builder2 1)))
                          (store-arg builder2 args-arr 0 (const-i64 builder2 5))
                          (build-ret builder2 (build-closure-apply builder2 closure args-arr 1)))))))
                m))))
        (build-and-run-closure-module)
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
    let caller = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("caller")
            .expect("failed to look up the compiled `caller` function")
    };
    assert_eq!(unsafe { caller.call(std::ptr::null(), 0) }, 105);
}

/// `build-closure-retain`/`build-closure-release`: a crash-free regression
/// test, not an exact-refcount one — the refcount slot itself isn't exposed
/// by any builtin, so the only thing worth asserting is what the plan calls
/// for (labels/closures Stage 4 plan, §6 Stage 4 step 5): retaining once
/// then releasing once leaves the closure still safely callable (refcount
/// back to its original 1, not 0), and `build-closure-retain` really does
/// return the closure value unchanged (so call sites can chain it without a
/// separate `let`).
#[test]
fn closure_retain_then_release_leaves_it_still_callable() {
    let src = r#"
        (defun build-and-run-closure-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((add-offset-fn (add-function-with-env m "add_offset")))
              (let ((caller-fn (add-function m "caller")))
                (let ((b1 (append-block add-offset-fn "entry")))
                  (let ((builder1 (llvm-builder::create)))
                    (position-at-end builder1 b1)
                    (let ((x (load-arg builder1 add-offset-fn 0)))
                      (let ((offset (load-env builder1 add-offset-fn 0)))
                        (build-ret builder1 (build-add builder1 x offset))))))
                (let ((b2 (append-block caller-fn "entry")))
                  (let ((builder2 (llvm-builder::create)))
                    (position-at-end builder2 b2)
                    (let ((env-arr (alloca-args builder2 1)))
                      (store-arg builder2 env-arr 0 (const-i64 builder2 100))
                      (let ((closure (build-make-closure builder2 add-offset-fn env-arr 1 0)))
                        (let ((retained (build-closure-retain builder2 closure)))
                          (build-closure-release builder2 m retained)
                          (let ((args-arr (alloca-args builder2 1)))
                            (store-arg builder2 args-arr 0 (const-i64 builder2 5))
                            (build-ret builder2 (build-closure-apply builder2 closure args-arr 1))))))))
                m))))
        (build-and-run-closure-module)
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
    let caller = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("caller")
            .expect("failed to look up the compiled `caller` function")
    };
    assert_eq!(unsafe { caller.call(std::ptr::null(), 0) }, 105);
}

/// The end-to-end Phase 1 slice: `(compile name)` from typelisp source
/// itself (not by hand-feeding `compile-function` a pre-built `Sexpr`, like
/// the compiler-body tests above), then a later `Expr::Call` of that same
/// function transparently dispatching to the JIT-compiled native code
/// (`Interp::compiled`) instead of tree-walking it.
#[test]
fn compile_dispatches_a_defun_call_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun add2 ((a i64) (b i64)) i64 (+ a b))
        (compile add2)
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
    let v = eval_ok_with_compiler(r#"(defun answer () i64 42) (compile answer)"#);
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
/// `(compile sum-of-squares)` runs the *whole* pipeline (`ast_bridge`'s
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
        (compile sum-of-squares)
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
/// `(compile add-offset)` must build `go` under the extended ABI and wire
/// the trailing body's call to pass `offset` through.
#[test]
fn compile_dispatches_a_defun_with_a_capturing_labels_body_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun add-offset ((offset i64) (n i64)) i64
          (labels ((go ((k i64)) i64 (+ k offset)))
            (go n)))
        (compile add-offset)
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
        (compile choose)
        (choose 10 5)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `get-function`/`compile-call`: the compiler body (not the raw builtins
/// directly, unlike `a_function_can_directly_call_another_function_in_the_same_module`)
/// compiling `(call name arg...)` nodes — `ast_bridge::translate_call`'s
/// exact tagged shape (labels/closures Stage 3) — into direct calls to a
/// *different*, already-`compile-function`-compiled function sharing the
/// same module: the same shared-module setup `compile::aot::compile_file`
/// uses in practice. `quadruple`'s body even nests one `call` inside
/// another's argument, exercising `compile-call-args`' own recursion back
/// into `compile-value`.
#[test]
fn the_compiler_body_compiles_a_call_to_another_compiled_function() {
    let module = match eval_ok_with_compiler(
        r#"
        (let ((m (llvm-module::create "mod")))
          (compile-function m "double" '((x . 0)) '(assoc "i64" "+" true (0 var "x" false) (0 var "x" false)))
          (compile-function m "quadruple" '((n . 0)) '(call "double" (0 call "double" (0 var "n" false)))))
        "#,
    ) {
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

/// labels/closures Stage 3, self-recursion: `compile-function`'s own first
/// step (`add-function`) already declares `f` in its module *before*
/// compiling its body, so a `(call "f" ...)` referring to the very function
/// being compiled resolves through the same `get-function` `compile-call`
/// uses for any other target — no special-casing needed. Only checks the
/// resulting IR (deliberately never run!): the body trivially infinite-loops
/// if executed (no `if`/comparison support yet to give it a base case — out
/// of this stage's scope, see `compiler.rs`'s `compile-call` doc comment).
#[test]
fn the_compiler_body_compiles_a_self_referencing_call() {
    let ir = expect_str(eval_ok_with_compiler(
        r#"(to-string (compile-function (llvm-module::create "mod") "f" '((n . 0)) '(call "f" (0 var "n" false))))"#,
    ));
    assert!(ir.contains("define i64 @f("), "IR was:\n{}", ir);
    assert!(ir.contains("call i64 @f("), "IR was:\n{}", ir);
}

/// The end-to-end Stage 3 slice (top-level `Expr::Call`, non-recursive),
/// from real typelisp source (not a hand-fed `Sexpr`, unlike the
/// compiler-body tests above): `sum-of-squares` calls the *separately*
/// `compile`d `square` twice. `Interp::compile_function`'s own module for
/// `sum-of-squares` never contains `square`'s body, only a no-body forward
/// declaration of it — proving the `add_global_mapping` wiring really does
/// reach the other, already-running JIT code, not just a coincidentally
/// correct IR shape.
#[test]
fn compile_dispatches_a_defun_that_calls_another_compiled_function() {
    let v = eval_ok_with_compiler(
        r#"
        (defun square ((x i64)) i64 (* x x))
        (defun sum-of-squares ((a i64) (b i64)) i64 (+ (square a) (square b)))
        (compile square)
        (compile sum-of-squares)
        (sum-of-squares 3 4)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 25),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `Interp::compile_function`'s up-front check (labels/closures Stage 3):
/// `(compile sum-of-squares)` calls `square`, but nothing has `compile`d
/// `square` yet — a clear `Panic` naming both functions, not a confusing one
/// from deep inside the compiler body's `get-function`.
#[test]
fn compile_errors_clearly_when_a_called_function_is_not_yet_compiled() {
    let err = run_with_compiler(
        r#"
        (defun square ((x i64)) i64 (* x x))
        (defun sum-of-squares ((a i64) (b i64)) i64 (+ (square a) (square b)))
        (compile sum-of-squares)
        "#,
    )
    .expect_err("expected a clear must-compile-first error");
    match err {
        EvalError::Panic(msg) => {
            assert!(msg.contains("square"), "message was: {}", msg);
            assert!(msg.contains("sum-of-squares"), "message was: {}", msg);
        }
        other => panic!("expected a Panic, got {:?}", other),
    }
}

/// The end-to-end self-recursion counterpart of
/// `the_compiler_body_compiles_a_self_referencing_call`, through the real
/// `(compile name)` JIT path rather than a hand-fed `Sexpr`: proves
/// `Interp::compile_function`'s call-target collection correctly excludes
/// self (no "must be compiled first" error, no forward declaration/
/// `add_global_mapping` wiring attempted for its own name). Deliberately
/// never *called* — see that test's doc comment for why.
#[test]
fn compile_succeeds_for_a_self_recursive_defun_without_being_run() {
    let v = eval_ok_with_compiler(
        r#"
        (defun loop-forever ((n i64)) i64 (loop-forever n))
        (compile loop-forever)
        "#,
    );
    assert!(expect_bool(v));
}

/// The end-to-end Stage 4 slice (immediate-call, non-capturing): an IIFE
/// (`((lambda (params) body) args...)`) translates to a single-def `labels`
/// block (`ast_bridge::translate_immediate_lambda_call`), not a boxed
/// `ClosureBox` at all — proves that delegation actually produces working,
/// callable code end to end, from real source through `(compile name)`.
#[test]
fn compile_dispatches_a_defun_with_an_immediately_invoked_lambda_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun calls-immediately ((n i64)) i64 ((lambda ((x i64)) i64 (+ x 1)) n))
        (compile calls-immediately)
        (calls-immediately 9)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 10),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Same IIFE shape, but the lambda's body captures its enclosing `defun`'s
/// own parameter — exercising `translate_immediate_lambda_call`'s reuse of
/// `labels_free_vars`/the captures ABI through the synthetic single-def
/// block, not just the no-capture case above.
#[test]
fn compile_dispatches_a_defun_with_a_capturing_immediately_invoked_lambda_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun adds-offset ((offset i64) (n i64)) i64 ((lambda ((y i64)) i64 (+ y offset)) n))
        (compile adds-offset)
        (adds-offset 100 5)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 105),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The end-to-end Stage 4 slice (escaping + capturing `lambda`): `adder`
/// returns a closure that captures its own parameter `n`, and `apply-fn`
/// (a separately-compiled function taking a `(fn (i64) i64)` *parameter*)
/// calls it through `apply-indirect`/`build-closure-apply` — the closure
/// value never gets inspected by tree-walking code along the way (it flows
/// from one `compile`d function's `i64` return straight into another's
/// `i64` argument via the ordinary `Expr::Call`-dispatches-to-`Interp::compiled`
/// path — see `Interp::compile_function`'s doc comment), which is exactly
/// what keeps this in scope (a compiled closure observed *by tree-walking
/// code* — e.g. printed, `eq`-compared, stored in a `HashTable` — is the
/// one documented exclusion, not this).
#[test]
fn compile_dispatches_an_escaping_capturing_lambda_called_through_another_compiled_function() {
    let v = eval_ok_with_compiler(
        r#"
        (defun adder ((n i64)) (fn (i64) i64) (lambda ((x i64)) i64 (+ x n)))
        (defun apply-fn ((f (fn (i64) i64)) (n i64)) i64 (f n))
        (compile adder)
        (compile apply-fn)
        (apply-fn (adder 5) 10)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The end-to-end Stage 4 slice (`Expr::FnRef` as a closure value): `square`
/// is used bare (no call syntax) where a `(fn (i64) i64)` is expected — the
/// checker reifies that into `Expr::FnRef`, which `ast_bridge::translate_fnref`
/// turns into a non-capturing forwarding `lambda` (`(lambda ... (call
/// "square" (var arg0)))`), reaching the exact same `ClosureBox`/
/// `build-closure-apply` machinery the closure-value test above does.
///
/// `run-it`'s body (not the top-level call site) is where `square` appears
/// bare — deliberately, since the *tree-walking* interpreter's own
/// `Expr::FnRef` evaluation produces an `RtValue::Closure`, not a plain
/// `i64` (see `Interp::eval`'s `Expr::FnRef` arm) — feeding that straight
/// into a *compiled* function's fixed `i64` ABI from the top level would be
/// exactly the "compiled closure observed by tree-walking code" case
/// `compile-lambda`'s doc comment explicitly excludes. Routing the `FnRef`
/// through another `compile`d function instead (`run-it`, itself dispatched
/// via `Expr::Call` -> `Interp.compiled` like any Stage 3 call) keeps the
/// `ClosureBox` entirely on the compiled side throughout.
#[test]
fn compile_dispatches_a_top_level_function_passed_by_name_through_apply_fn() {
    let v = eval_ok_with_compiler(
        r#"
        (defun square ((x i64)) i64 (* x x))
        (defun apply-fn ((f (fn (i64) i64)) (n i64)) i64 (f n))
        (defun run-it () i64 (apply-fn square 5))
        (compile square)
        (compile apply-fn)
        (compile run-it)
        (run-it)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 25),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Follow-up to Stage 4: a `labels` sibling referenced *as a value* (not
/// called) — here, the block's own trailing body bare-returns `f` instead of
/// calling it. `ast_bridge::ast_to_sexpr_scoped`'s `Expr::Var` arm doesn't
/// distinguish this from any other variable reference (it always emits
/// `(var name)`); `compiler.rs`'s `resolve-value` is what now resolves it —
/// not found in the ordinary `env`, found instead in `fn-env`, so it gets
/// boxed into a fresh `ClosureBox` on the spot (`build-make-closure`, the
/// same builtin `compile-lambda` already uses). `(apply-indirect (var "f")
/// (int 5))` then calls through that box, proving the boxing produced a
/// genuinely callable closure, not just a value that type-checks.
#[test]
fn the_compiler_body_boxes_a_bare_labels_sibling_reference() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "outer" '() '(labels () (("f" ((n . 0)) (var "n" false))) (apply-indirect (var "f" true) (0 int 5))))"#,
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

/// Same as above, but the sibling being boxed itself captures an outer-scope
/// value (`offset`) — exercises `compile-env-args`'s extended signature: the
/// `names` it's walking (`captured`, building the *block's* own env array
/// from the caller's `outer` scope) and the `captured` it holds constant for
/// `resolve-value`'s own fallback happen to be the *same* list here (the
/// common case — see `resolve-value`'s doc comment for the one place,
/// `compile-lambda`, where they legitimately differ).
#[test]
fn the_compiler_body_boxes_a_bare_labels_sibling_reference_that_captures_an_outer_value() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "outer" '((offset . 0) (n . 0)) '(labels ((offset . 0)) (("go" ((k . 0)) (assoc "i64" "+" true (0 var "k" false) (0 var "offset" false)))) (apply-indirect (var "go" true) (0 int 5))))"#,
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
    let argv: [i64; 2] = [10, 5];
    assert_eq!(unsafe { outer.call(argv.as_ptr(), argv.len() as u32) }, 15);
}

/// A `labels` sibling whose own body bare-references *itself* — `declare-labels-siblings`
/// already registers `f` into `inner-fn-env` before any sibling's body is
/// compiled (including `f`'s own), so `resolve-value`'s fallback finds it
/// immediately. Only checks the IR (never run!), the same way
/// `the_compiler_body_compiles_a_self_referencing_call` deliberately never
/// JIT-runs a self-referencing top-level `Expr::Call`: a closure that boxes
/// itself and is then called would just box itself again, forever, with no
/// base case — out of scope here (no `if`/comparison yet), but the boxing
/// itself (a `build-array_malloc` call showing up in the IR) is exactly what
/// this test confirms.
#[test]
fn the_compiler_body_boxes_a_labels_sibling_that_bare_references_itself() {
    let ir = expect_str(eval_ok_with_compiler(
        r#"(to-string (compile-function (llvm-module::create "mod") "outer" '() '(labels () (("f" () (var "f" true))) (apply "f"))))"#,
    ));
    assert!(ir.contains("malloc"), "IR was:\n{}", ir);
}

/// The end-to-end follow-up to Stage 4: `make-adder` returns one of its own
/// `labels` siblings bare (no `lambda` literal involved at all this time —
/// the originally-skipped Stage 4 sub-step 5 scenario, restricted to the
/// non-cyclic single-sibling case this fix actually supports — see
/// `resolve-value`'s doc comment for why a true reference cycle between two
/// boxed siblings isn't constructible under this design). `apply-fn` is the
/// same helper already used by the `lambda`-literal version of this test
/// (`compile_dispatches_an_escaping_capturing_lambda_called_through_another_compiled_function`).
#[test]
fn compile_dispatches_an_escaping_labels_sibling_returned_bare() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-adder ((n i64)) (fn (i64) i64)
          (labels ((adder ((x i64)) i64 (+ x n)))
            adder))
        (defun apply-fn ((f (fn (i64) i64)) (n i64)) i64 (f n))
        (compile make-adder)
        (compile apply-fn)
        (apply-fn (make-adder 5) 10)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A `labels` block with *two* siblings, where the trailing body bare-returns
/// only one of them (`f`, not `g`) — `g` exists purely to catch a plausible
/// implementation bug: if `resolve-value`'s `fn-env` lookup or
/// `declare-labels-siblings`' mangled-name bookkeeping ever got the wrong
/// sibling, this would box/call `g` instead of `f` and produce a different
/// (or panicking) result. `g` calling `f` directly (an ordinary `apply`,
/// unaffected by this fix) also confirms the fix doesn't disturb sibling-to-
/// sibling direct calls.
#[test]
fn compile_dispatches_an_escaping_labels_sibling_chosen_correctly_among_several() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-pair ((n i64)) (fn (i64) i64)
          (labels ((f ((x i64)) i64 (+ x n))
                   (g ((x i64)) i64 (+ x (f x))))
            f))
        (defun apply-fn ((h (fn (i64) i64)) (n i64)) i64 (h n))
        (compile make-pair)
        (compile apply-fn)
        (apply-fn (make-pair 5) 10)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A nested, escaping `lambda` whose body indirectly calls a `labels`
/// sibling — `freevars::lambda_free_vars` already treated a sibling
/// referenced from inside a nested `lambda` as an ordinary free variable
/// (this predates this fix, written back in Stage 2 before `lambda`s even
/// existed), so the sibling ends up in the `lambda`'s own captured list;
/// when `compile-lambda` builds that `lambda`'s `ClosureBox` env array (in
/// the *outer*, still-`fn-env`-having scope, via `compile-env-args`), it now
/// resolves the sibling through `resolve-value`'s fallback exactly like the
/// bare-reference tests above, and the boxed sibling flows into the nested
/// `lambda`'s own `env` via the ordinary `bind-captures` path — no change to
/// `compile-lambda` itself was needed for this case to start working.
#[test]
fn compile_dispatches_an_escaping_lambda_that_indirectly_captures_a_labels_sibling() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-caller ((n i64)) (fn () i64)
          (labels ((double ((x i64)) i64 (* x 2)))
            (lambda () i64 (double n))))
        (defun apply-fn0 ((f (fn () i64))) i64 (f))
        (compile make-caller)
        (compile apply-fn0)
        (apply-fn0 (make-caller 21))
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Automatic `ClosureBox` retain/release insertion (a follow-up to
/// labels/closures Stage 4): `outer(cb, x)` is a `labels` block with one
/// sibling, `go`, that captures `cb` (a borrowed, `Fn`-typed parameter of
/// `outer` itself) and calls it indirectly. Every call to `outer` exercises
/// the *direct*, non-escaping `compile-env-args` path (`compile-apply`
/// building `go`'s env array each time) plus both `go`'s and `outer`'s own
/// R1 entry-retain/R2 exit-release — if either leaked or double-released,
/// `cb`'s refcount (read back via the test-only `debug-closure-refcount`
/// builtin, through a hand-built `read_rc` function sharing the same
/// module) would drift after repeated calls instead of returning to
/// exactly `1` every time.
#[test]
fn repeated_calls_through_a_captured_closure_do_not_leak_its_refcount() {
    let module = match eval_ok_with_compiler(
        r#"
        (defun build-test-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((identity-fn (add-function-with-env m "identity")))
              (let ((b (append-block identity-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (load-arg builder identity-fn 0)))))
            (let ((rc-fn (add-function m "read_rc")))
              (let ((b (append-block rc-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (debug-closure-refcount builder (load-arg builder rc-fn 0))))))
            (let ((mkbox-fn (add-function m "make_box")))
              (let ((b (append-block mkbox-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (build-make-closure builder (get-function m "identity") (alloca-args builder 0) 0 0)))))
            (compile-function m "outer" '((cb . 1) (x . 0))
              '(labels ((cb . 1)) (("go" ((x . 0)) (apply-indirect (var "cb" true) (0 var "x" false)))) (apply "go" (0 var "x" false))))
            m))
        (build-test-module)
        "#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let make_box = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("make_box")
            .expect("failed to look up `make_box`")
    };
    let read_rc = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("read_rc")
            .expect("failed to look up `read_rc`")
    };
    let outer = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("outer")
            .expect("failed to look up the compiled `outer` function")
    };

    let cb = unsafe { make_box.call(std::ptr::null(), 0) };
    let rc_argv = [cb];
    assert_eq!(unsafe { read_rc.call(rc_argv.as_ptr(), 1) }, 1, "a freshly made closure starts at refcount 1");

    for _ in 0..1000 {
        let outer_argv = [cb, 5];
        assert_eq!(unsafe { outer.call(outer_argv.as_ptr(), 2) }, 5);
        assert_eq!(unsafe { read_rc.call(rc_argv.as_ptr(), 1) }, 1, "refcount must return to 1 after every call, never drift");
    }
}

/// The escaping-`ClosureBox` counterpart of the test above: `make-wrapper`
/// (real typelisp source, going through `ast_bridge`/`(compile ...)`, not a
/// hand-fed `Sexpr`) returns a `lambda` that captures a borrowed `Fn`-typed
/// parameter of its own — `compile-lambda`'s `compile-escaping-env-args`
/// call must retain that borrowed value at construction time (R4), and the
/// resulting outer box's `fn_mask` must mark that captured slot so
/// releasing the outer box (without ever calling it) recursively releases
/// the inner one too — read back via `debug-closure-refcount` afterward.
#[test]
fn releasing_an_escaping_lambda_recursively_releases_a_captured_closure() {
    let module = match eval_ok_with_compiler(
        r#"
        (defun build-test-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((identity-fn (add-function-with-env m "identity")))
              (let ((b (append-block identity-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (load-arg builder identity-fn 0)))))
            (let ((rc-fn (add-function m "read_rc")))
              (let ((b (append-block rc-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (debug-closure-refcount builder (load-arg builder rc-fn 0))))))
            (let ((mkbox-fn (add-function m "make_box")))
              (let ((b (append-block mkbox-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (build-make-closure builder (get-function m "identity") (alloca-args builder 0) 0 0)))))
            (let ((release-fn (add-function m "release_box")))
              (let ((b (append-block release-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-closure-release builder m (load-arg builder release-fn 0))
                  (build-ret builder (const-i64 builder 0)))))
            (compile-function m "make-wrapper" '((inner . 1))
              '(lambda "wrapper$0" ((inner . 1)) () (var "inner" true)))
            m))
        (build-test-module)
        "#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let make_box = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("make_box")
            .expect("failed to look up `make_box`")
    };
    let read_rc = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("read_rc")
            .expect("failed to look up `read_rc`")
    };
    let release_box = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("release_box")
            .expect("failed to look up `release_box`")
    };
    let make_wrapper = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("make-wrapper")
            .expect("failed to look up the compiled `make-wrapper` function")
    };

    let inner = unsafe { make_box.call(std::ptr::null(), 0) };
    let inner_argv = [inner];
    assert_eq!(unsafe { read_rc.call(inner_argv.as_ptr(), 1) }, 1);

    let wrapper_argv = [inner];
    let outer = unsafe { make_wrapper.call(wrapper_argv.as_ptr(), 1) };
    // The escaping wrapper's own construction retained `inner` (it's
    // *borrowed* from `make-wrapper`'s own parameter) — `inner`'s refcount
    // should now be 2: `make-wrapper`'s own activation's R1 copy is already
    // gone (released at `make-wrapper`'s R2 exit, since `inner` isn't what
    // it bare-returns — the *wrapper* is) plus the wrapper's own captured
    // copy.
    assert_eq!(unsafe { read_rc.call(inner_argv.as_ptr(), 1) }, 2, "the escaping wrapper's own retain of its borrowed capture");

    let release_argv = [outer];
    unsafe { release_box.call(release_argv.as_ptr(), 1) };
    assert_eq!(
        unsafe { read_rc.call(inner_argv.as_ptr(), 1) },
        1,
        "releasing the outer box must cascade-release its captured inner closure"
    );
}

/// `get_or_define_closure_release_fn` builds the shared
/// `__typelisp_closure_release` LLVM function lazily, the first time any
/// `ClosureBox` work happens in a module, and memoizes it via
/// `Module::get_function` — confirms that memoization actually holds even
/// with *two* separately-escaping closures in the same module (IR-only,
/// never executed).
#[test]
fn the_shared_closure_release_function_is_defined_once_per_module() {
    let ir = expect_str(eval_ok_with_compiler(
        r#"
        (to-string
          (let ((m (llvm-module::create "mod")))
            (compile-function m "f" '((cb . 1))
              '(lambda "f$0" ((cb . 1)) () (var "cb" true)))
            (compile-function m "g" '((cb . 1))
              '(lambda "g$0" ((cb . 1)) () (var "cb" true)))
            m))
        "#,
    ));
    let occurrences = ir.matches("define void @__typelisp_closure_release").count();
    assert_eq!(occurrences, 1, "expected exactly one shared definition, IR was:\n{}", ir);
}

/// `compile-call-args`' fresh-value pending-release path (R4: a call
/// argument that's `Fn`-typed *and* fresh — here, `helper` referenced bare
/// and passed straight to `go` as a call argument, resolved via
/// `resolve-value`'s fallback — needs releasing once `go`'s call returns,
/// since nothing else owns that one-off box). IR-only: the ephemeral box
/// never escapes anywhere this test could read its refcount back out of,
/// so this checks the *generated IR* has a release call after the call
/// instead (`compile_apply`'s `release-pending-args` call site).
#[test]
fn compile_apply_releases_a_fresh_sibling_passed_as_a_call_argument_after_the_call() {
    let ir = expect_str(eval_ok_with_compiler(
        r#"(to-string (compile-function (llvm-module::create "mod") "outer" '((x . 0))
              '(labels () (("helper" () (int 7))
                           ("go" ((f . 1) (y . 0)) (apply-indirect (var "f" true) (0 var "y" false))))
                 (apply "go" (1 var "helper" true) (0 var "x" false)))))"#,
    ));
    let call_pos = ir.find("call i64").expect("expected a direct call to go in the IR");
    let release_pos = ir.find("call void @__typelisp_closure_release").expect("expected a release call in the IR");
    assert!(release_pos > call_pos, "release must come after the call, IR was:\n{}", ir);
}

// --- if/let/comparisons (labels/closures Stage 5) ---

/// `(bool b)` (`ast_bridge` already produced this tag; `compile-value` had no
/// receiving arm for it until now) — every compiled value is a plain `i64`,
/// so `true`/`false` compile straight to `1`/`0`.
#[test]
fn the_compiler_body_compiles_a_bool_literal_node() {
    let ir = expect_str(eval_ok_with_compiler(
        r#"(to-string (compile-function (llvm-module::create "mod") "answer" '() '(bool true)))"#,
    ));
    assert!(ir.contains("ret i64 1"), "IR was:\n{}", ir);
}

/// `compile-assoc`'s new comparison arms (`<`/`<=`/`>`/`>=`/`=`/`eq`/`/=`) —
/// `build-icmp-lt` end to end, JIT-executed both ways.
#[test]
fn the_compiler_body_compiles_an_i64_comparison() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "lt" '((a . 0) (b . 0))
              '(assoc "i64" "<" true (0 var "a" false) (0 var "b" false)))"#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module.borrow().create_jit_execution_engine(OptimizationLevel::None).expect("failed to create JIT execution engine");
    let lt = unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("lt").expect("failed to look up `lt`") };
    assert_eq!(unsafe { lt.call([3, 5].as_ptr(), 2) }, 1);
    assert_eq!(unsafe { lt.call([5, 3].as_ptr(), 2) }, 0);
}

/// `compile-assoc`'s receiver-type guard: an `f64` receiver (which defines
/// the very same method names under `registry::float_assoc`, still out of
/// scope — see `compile-assoc`'s doc comment) must panic clearly rather than
/// silently misinterpreting its bit pattern as an `i64`. Now reached via the
/// generic method-call branch instead of a dedicated `i64`/`i32`-only guard:
/// this hand-fed Sexpr bypasses `Interp::compile_function`'s own up-front
/// check (the normal way such a call is rejected, with a clearer message —
/// see `compile_of_a_function_calling_an_uncompiled_builtin_method_is_a_clean_error`
/// below), so the only thing left to catch it is `get-function` itself
/// failing to find `"f64::+"` in this throwaway module.
#[test]
fn compile_assoc_panics_on_an_unsupported_receiver_type() {
    let err = run_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "addf" '((a . 0) (b . 0))
              '(assoc "f64" "+" true (0 var "a" false) (0 var "b" false)))"#,
    )
    .expect_err("expected a panic for a non-i64/i32 receiver");
    match err {
        EvalError::Panic(msg) => assert!(msg.contains("no function named") && msg.contains("f64::+"), "message was: {}", msg),
        other => panic!("expected a Panic, got {:?}", other),
    }
}

/// `compile-if`: `(if is-fn cond-form then-form else-form)` end to end —
/// `max(a, b)` via a comparison feeding the branch, JIT-executed both ways
/// to prove both the `then` and `else` arm are reachable and correct.
#[test]
fn the_compiler_body_compiles_an_if_expression() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "maxab" '((a . 0) (b . 0))
              '(if false
                   (assoc "i64" ">" true (0 var "a" false) (0 var "b" false))
                   (var "a" false)
                   (var "b" false)))"#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module.borrow().create_jit_execution_engine(OptimizationLevel::None).expect("failed to create JIT execution engine");
    let maxab =
        unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("maxab").expect("failed to look up `maxab`") };
    assert_eq!(unsafe { maxab.call([10, 32].as_ptr(), 2) }, 32);
    assert_eq!(unsafe { maxab.call([50, 3].as_ptr(), 2) }, 50);
}

/// `compile-let`'s shadow/restore discipline (`bind-let-values`/
/// `restore-let-values`): a `let` that shadows the enclosing function's own
/// parameter must not leak its shadowed value into a sibling expression that
/// references the same name *after* the `let` ends — `(+ (let ((x 99)) x)
/// x)`'s second `x` must read the real parameter, not 99.
#[test]
fn let_shadowing_is_correctly_restored_after_the_let_ends() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "shadow_test" '((x . 0))
              '(assoc "i64" "+" true
                 (0 let (((x . 0) . (int 99))) (var "x" false))
                 (0 var "x" false)))"#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module.borrow().create_jit_execution_engine(OptimizationLevel::None).expect("failed to create JIT execution engine");
    let f = unsafe {
        engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("shadow_test").expect("failed to look up `shadow_test`")
    };
    assert_eq!(
        unsafe { f.call([5].as_ptr(), 1) },
        104,
        "the let must shadow x only within its own body, restoring the outer parameter afterward"
    );
}

/// `compile-if`'s retain-before-merge fix (this module's doc comment,
/// `compile-if-branch`): an `if` whose branches are `Fn`-typed must retain
/// whichever branch is *borrowed* before it escapes, since `if` introduces
/// no function boundary to freshen it the way a call/apply/labels/lambda
/// result already does. `pick` bare-passes through one of two borrowed
/// `Fn`-typed parameters depending on a (constant-`false`) condition,
/// picking `g`. Without the fix, the untouched branch's refcount would
/// still be correct (it's never touched), but the *picked* branch would
/// escape with no extra retain — releasing it once (simulating the caller
/// treating `pick`'s non-`var`-shaped result as fresh, per `form-is-borrowed?`'s
/// existing convention for every other tag) would then under-count `g`'s
/// refcount by one relative to before the call, instead of exactly
/// restoring it.
#[test]
fn compile_if_retains_a_borrowed_branch_value_before_it_escapes() {
    let module = match eval_ok_with_compiler(
        r#"
        (defun build-test-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((identity-fn (add-function-with-env m "identity")))
              (let ((b (append-block identity-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (load-arg builder identity-fn 0)))))
            (let ((rc-fn (add-function m "read_rc")))
              (let ((b (append-block rc-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (debug-closure-refcount builder (load-arg builder rc-fn 0))))))
            (let ((mkbox-fn (add-function m "make_box")))
              (let ((b (append-block mkbox-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (build-make-closure builder (get-function m "identity") (alloca-args builder 0) 0 0)))))
            (let ((release-fn (add-function m "release_box")))
              (let ((b (append-block release-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-closure-release builder m (load-arg builder release-fn 0))
                  (build-ret builder (const-i64 builder 0)))))
            (compile-function m "pick" '((f . 1) (g . 1))
              '(if true (bool false) (var "f" true) (var "g" true)))
            m))
        (build-test-module)
        "#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module.borrow().create_jit_execution_engine(OptimizationLevel::None).expect("failed to create JIT execution engine");
    let make_box = unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("make_box").expect("failed to look up `make_box`") };
    let read_rc = unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("read_rc").expect("failed to look up `read_rc`") };
    let release_box =
        unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("release_box").expect("failed to look up `release_box`") };
    let pick = unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("pick").expect("failed to look up `pick`") };

    let f_box = unsafe { make_box.call(std::ptr::null(), 0) };
    let g_box = unsafe { make_box.call(std::ptr::null(), 0) };
    let f_rc_before = unsafe { read_rc.call([f_box].as_ptr(), 1) };
    let g_rc_before = unsafe { read_rc.call([g_box].as_ptr(), 1) };

    let picked = unsafe { pick.call([f_box, g_box].as_ptr(), 2) };
    assert_eq!(picked, g_box, "expected the else branch (g) to be chosen");
    assert_eq!(
        unsafe { read_rc.call([f_box].as_ptr(), 1) },
        f_rc_before,
        "the untouched branch's refcount must be unchanged"
    );

    unsafe { release_box.call([picked].as_ptr(), 1) };
    assert_eq!(
        unsafe { read_rc.call([g_box].as_ptr(), 1) },
        g_rc_before,
        "releasing the if's returned value should exactly undo compile-if's retain of the borrowed branch"
    );
}

/// The capstone of if/let/comparisons (labels/closures Stage 5): a *real*,
/// terminating self-recursive function with a base case, compiled through
/// the full `ast_bridge`/`compiler.rs` pipeline from ordinary typelisp
/// source (not a hand-fed `Sexpr`) and JIT-dispatched via `(compile ...)`.
/// Every earlier Stage 1-4 self-recursion test had to leave the base case
/// out (no `if` existed to write one) — this is the first one that actually
/// runs.
#[test]
fn compile_dispatches_a_self_recursive_function_with_a_base_case_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun fact ((n i64)) i64 (if (<= n 1) 1 (* n (fact (- n 1)))))
        (compile fact)
        (fact 10)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 3628800),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `loop`/`break`/`return`/`setf`: a `setf` on a `let`-bound local, compiled
/// through the full pipeline. Exercises the new alloca-backed `env`
/// representation (`bind-params`/`bind-captures`/`bind-let-values`).
/// `i32`, not `i64`: a bare integer literal defaults to `i32`
/// (`Checker::check_let` always checks a binding's value with `expected:
/// None`), and nothing here forces otherwise. The `setf`/read-back happen
/// inside a `loop` (whose body, unlike `let`'s, may have any number of
/// statements — see `ast_bridge::translate_let`'s single-expression-body
/// restriction) rather than directly in the `let`'s own body, purely to fit
/// that restriction; `compile_dispatches_a_counting_loop_with_setf_and_conditional_return_to_native_code`
/// below is the test that actually exercises `setf` for something a `loop`
/// needs it for.
#[test]
fn compile_dispatches_a_setf_on_a_let_bound_local_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun setf-test () i32 (let ((x 1)) (loop (setf x 5) (return x))))
        (compile setf-test)
        (setf-test)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 5),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A bare `(return value)` inside a `loop` exits immediately with that
/// value — the simplest possible `loop`/`return` round trip, no `break`/
/// `setf` involved. `i32`: `(loop ...)` is seeded `Never` and refined purely
/// from the `return`s found inside it (`Checker::check_loop`/`check_return`)
/// — a `defun`'s own declared return type never propagates down into that
/// seed, so the bare literal `42` still defaults to `i32` regardless of
/// what `loop-return-test` itself declares.
#[test]
fn compile_dispatches_a_bare_return_inside_a_loop_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun loop-return-test () i32 (loop (return 42)))
        (compile loop-return-test)
        (loop-return-test)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `(return)` with no value — `ast_bridge::translate_return`'s implicit-
/// `Unit` case, which also exercises `compile-value`'s new `"unit"` arm
/// (`compile-unit`). The tree-walking call dispatch
/// (`Expr::Call`'s `compiled.borrow().get(name)` branch in `interp.rs`)
/// always wraps a compiled call's raw `i64` result as `RtValue::Int`
/// regardless of the callee's declared return type — a pre-existing gap,
/// not something this stage introduces or fixes — so a `unit`-returning
/// compiled function surfaces here as `RtValue::Int(0)` (`compile-unit`'s
/// `0` encoding), not `RtValue::Unit`; what matters for this test is that
/// it compiles and runs at all.
#[test]
fn compile_dispatches_a_value_less_return_from_a_loop_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun void-return-test () () (loop (return)))
        (compile void-return-test)
        (void-return-test)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 0),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The realistic shape `while`/`dotimes`/`dolist` all reduce to: a `loop`
/// whose body conditionally exits via `return` (delivering the
/// accumulator) and otherwise mutates loop-carried locals via `setf` —
/// computes `0+1+...+n` without recursion. `i`/`acc` default to `i32`
/// (bare integer literals with no `expected` type — `Checker::check_let`
/// always checks a binding's value with `expected: None`), so `n` is `i32`
/// too, to keep every arithmetic operand the same type.
#[test]
fn compile_dispatches_a_counting_loop_with_setf_and_conditional_return_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun sum-to ((n i32)) i32
          (let ((i 0) (acc 0))
            (loop
              (if (> i n) (return acc) ())
              (setf acc (+ acc i))
              (setf i (+ i 1)))))
        (compile sum-to)
        (sum-to 5)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 15, "0+1+2+3+4+5"),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A `loop` exited via a bare `(break)` (no `return` anywhere) is
/// necessarily `Unit`-typed (`Checker::check_break` always contributes
/// `Unit` to the enclosing loop's type) — this is the `compile-break`
/// counterpart of the `return`-based tests above, and the only way to
/// observe it actually terminating (rather than looping forever, or
/// crashing on malformed IR from a missed `block-terminated?` guard) is to
/// run it: a hung or crashed test is the failure signature here, not a
/// wrong return value (see `compile_dispatches_a_value_less_return_from_a_loop_to_native_code`'s
/// doc comment for why the result is `RtValue::Int(0)` regardless).
#[test]
fn compile_dispatches_a_loop_exited_via_a_bare_break_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun count-to-five () ()
          (let ((i 0))
            (loop
              (if (>= i 5) (break) ())
              (setf i (+ i 1)))))
        (compile count-to-five)
        (count-to-five)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 0),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Nested `loop`s: an inner `loop`'s bare `(break)` must exit only the
/// *inner* loop, never the outer one — the property `loop-exit`/`loop-slot`
/// being threaded (and re-installed fresh by each `compile-loop`, the same
/// way `cur-fn`/`captured` already are for `labels`/`lambda` nesting) exists
/// to guarantee. The inner loop (nested inside a fresh `let` each outer
/// iteration, so `inner` resets to `0` every time) runs exactly twice before
/// breaking, incrementing the *outer*-scoped `total` each time; after 3
/// outer iterations `total` must be exactly `3 * 2 = 6` — if the inner
/// `break` wrongly resolved to the outer loop's exit instead, this would
/// return `2` (one outer iteration's worth) or hang.
#[test]
fn compile_dispatches_nested_loops_where_an_inner_break_only_exits_the_inner_loop() {
    let v = eval_ok_with_compiler(
        r#"
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
        (compile nested-loop-test)
        (nested-loop-test)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 6),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `compile-return`'s reuse of `compile-if-branch`'s retain-before-escape
/// fix (this module's doc comment): a `loop` whose only exit is `(return
/// (var "f" true))` — a *borrowed* `Fn`-typed parameter — must retain it
/// before the value crosses out of the loop's merge slot, the same boundary
/// `compile_if_retains_a_borrowed_branch_value_before_it_escapes` already
/// verifies for `if`. Reuses that test's helper functions
/// (`identity`/`read_rc`/`make_box`/`release_box`) against a new
/// loop-returning function instead of an `if`-branching one.
#[test]
fn compile_loop_retains_a_borrowed_return_value_before_it_escapes() {
    let module = match eval_ok_with_compiler(
        r#"
        (defun build-test-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((identity-fn (add-function-with-env m "identity")))
              (let ((b (append-block identity-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (load-arg builder identity-fn 0)))))
            (let ((rc-fn (add-function m "read_rc")))
              (let ((b (append-block rc-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (debug-closure-refcount builder (load-arg builder rc-fn 0))))))
            (let ((mkbox-fn (add-function m "make_box")))
              (let ((b (append-block mkbox-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-ret builder (build-make-closure builder (get-function m "identity") (alloca-args builder 0) 0 0)))))
            (let ((release-fn (add-function m "release_box")))
              (let ((b (append-block release-fn "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (build-closure-release builder m (load-arg builder release-fn 0))
                  (build-ret builder (const-i64 builder 0)))))
            (compile-function m "pick_via_loop" '((f . 1))
              '(loop (return true (var "f" true))))
            m))
        (build-test-module)
        "#,
    ) {
        RtValue::LlvmModule(m) => m,
        other => panic!("expected an LlvmModule, got {:?}", other),
    };
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module.borrow().create_jit_execution_engine(OptimizationLevel::None).expect("failed to create JIT execution engine");
    let make_box = unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("make_box").expect("failed to look up `make_box`") };
    let read_rc = unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("read_rc").expect("failed to look up `read_rc`") };
    let release_box =
        unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("release_box").expect("failed to look up `release_box`") };
    let pick_via_loop =
        unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("pick_via_loop").expect("failed to look up `pick_via_loop`") };

    let f_box = unsafe { make_box.call(std::ptr::null(), 0) };
    let rc_before = unsafe { read_rc.call([f_box].as_ptr(), 1) };

    let picked = unsafe { pick_via_loop.call([f_box].as_ptr(), 1) };
    assert_eq!(picked, f_box);

    unsafe { release_box.call([picked].as_ptr(), 1) };
    assert_eq!(
        unsafe { read_rc.call([f_box].as_ptr(), 1) },
        rc_before,
        "releasing the loop's returned value should exactly undo compile-return's retain of the borrowed parameter"
    );
}

/// `while`/`dotimes` (`prelude.rs`) desugar to `loop`/`break`/`setf` (this
/// stage) plus a call to `not` (`(loop (if (not ,test) (break) ()) ,@body)`)
/// — `not` used to be a Rust-native free function with no typelisp AST body
/// at all, which would have made this permanently uncompilable (`compile`'s
/// call-target pre-check has no way to compile a body that doesn't exist).
/// It's a plain `defun` in `src/prelude.rs` now (`(defun not ((b bool))
/// bool (if b false true))` — no GC-heap/Rust-only dependency, so there was
/// no reason for it to stay a Rust builtin once `loop`/`if` existed to
/// write it with), so it only needs the same `(compile not)` *first* every
/// other cross-function dependency already requires (see
/// `compile_errors_clearly_when_a_called_function_is_not_yet_compiled`) —
/// proving `while`/`dotimes` themselves are now fully compilable, the
/// original motivation for this whole `loop`/`break`/`return`/`setf` stage.
#[test]
fn compile_dispatches_a_dotimes_loop_that_terminates_via_its_internal_break() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun count-via-dotimes ((n i32)) ()
          (dotimes (i n) ()))
        (compile not)
        (compile count-via-dotimes)
        (count-via-dotimes 5)
        "#,
    )
    .expect("eval failed");
    match v {
        RtValue::Int(n) => assert_eq!(n, 0),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Stage 5 (`docs/TODO.md`): `consp`/`null`/`atom` (`prelude.rs`) now
/// compile clean through to native code — `Expr::Match` over a `Sexpr`
/// scrutinee has a real translation (`ast_bridge::translate_match`) and
/// `compiler.rs` has `compile-match`/`compile-pattern-test`/
/// `compile-ctor-subpatterns`. Each is exercised (via ordinary
/// `Expr::Call` dispatch — Stage 5's other half, `Sexpr`-typed arguments
/// now marshal across that boundary too, see `Interp::eval`'s
/// compiled-call arm) on both a `Cons` and a `Nil` value, proving the
/// compiled tag test gets both arms right, not just one.
///
/// `consp`/`null`/`atom` all return `bool` — this stage's scope is the
/// `Sexpr` side of the `Expr::Call` marshaling gap only (`docs/TODO.md`),
/// not `bool`'s, so a compiled call's return value still surfaces as the
/// raw `i64` `compile-bool`'s convention uses (`1`/`0`), not a faithful
/// `RtValue::Bool` — asserted on directly here rather than glossed over.
#[test]
fn compile_dispatches_consp_null_and_atom_to_native_code_for_both_a_cons_and_a_nil_value() {
    let truthy = run_with_compiler_and_prelude(
        r#"
        (compile not)
        (compile consp)
        (consp (cons (Int 1) (Int 2)))
        "#,
    )
    .expect("eval failed");
    assert_eq!(truthy, RtValue::Int(1), "consp on a Cons should report true (1)");

    let falsy = run_with_compiler_and_prelude(
        r#"
        (compile not)
        (compile consp)
        (consp ())
        "#,
    )
    .expect("eval failed");
    assert_eq!(falsy, RtValue::Int(0), "consp on Nil should report false (0)");

    let truthy = run_with_compiler_and_prelude(
        r#"
        (compile not)
        (compile null)
        (null ())
        "#,
    )
    .expect("eval failed");
    assert_eq!(truthy, RtValue::Int(1), "null on Nil should report true (1)");

    let falsy = run_with_compiler_and_prelude(
        r#"
        (compile not)
        (compile null)
        (null (cons (Int 1) (Int 2)))
        "#,
    )
    .expect("eval failed");
    assert_eq!(falsy, RtValue::Int(0), "null on a Cons should report false (0)");

    let truthy = run_with_compiler_and_prelude(
        r#"
        (compile not)
        (compile consp)
        (compile atom)
        (atom ())
        "#,
    )
    .expect("eval failed");
    assert_eq!(truthy, RtValue::Int(1), "atom (= not . consp) on Nil should report true (1) — exercises a compiled call to another compiled function (Stage 3) alongside the new Match support");
}

/// Stage 5: a `Bind` sub-pattern over a `cons` field (`Sexpr`-typed) is
/// extracted via `rt_car`/`rt_cdr` (`compile-sexpr-field`) and the whole
/// function's `Sexpr`-typed *return* value round-trips back out through
/// `Expr::Call`'s new `decode` step — together, the full argument-in/
/// return-out `Sexpr` marshaling this stage adds, not just the `bool`-
/// returning predicates above.
#[test]
fn compile_dispatches_a_function_that_binds_and_returns_a_cons_field_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun my-car ((s Sexpr)) Sexpr (match s ((Cons h _) h) (_ s)))
        (compile my-car)
        (my-car (cons (Int 42) (Int 99)))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Sexpr(Value::Int(42)));
}

/// Stage 5: a nested `Ctor` sub-pattern (`(Cons _ (Nil))` —
/// `prelude.rs`'s `last`/`butlast` use exactly this shape) dispatches
/// correctly: `compile-ctor-subpatterns` extracts the `cdr` field and
/// recurses `compile-pattern-test` on it against the inner `(Nil)`
/// pattern, sharing this arm's one `fail-block` with the outer tag test.
#[test]
fn compile_dispatches_a_nested_ctor_pattern_to_native_code() {
    let src = r#"
        (defun second-is-nil ((s Sexpr)) bool (match s ((Cons _ (Nil)) true) (_ false)))
        (compile second-is-nil)
        "#;
    let single = run_with_compiler_and_prelude(&format!("{}\n(second-is-nil (cons (Int 1) ()))", src)).expect("eval failed");
    assert_eq!(single, RtValue::Int(1), "(1 . Nil) should match (Cons _ (Nil))");

    let two = run_with_compiler_and_prelude(&format!("{}\n(second-is-nil (cons (Int 1) (cons (Int 2) ())))", src)).expect("eval failed");
    assert_eq!(two, RtValue::Int(0), "(1 . (2 . Nil)) should not match — its cdr is a Cons, not Nil");

    let nil = run_with_compiler_and_prelude(&format!("{}\n(second-is-nil ())", src)).expect("eval failed");
    assert_eq!(nil, RtValue::Int(0), "Nil itself doesn't match (Cons _ (Nil)) at all — falls to the wildcard arm");
}

/// Stage 5: a literal `Int` sub-pattern (`pat-lit`) inside a `Cons`/`Int`
/// `Ctor` test — `compile-sexpr-field`'s `int` extraction (`build-ashr`)
/// feeding straight into `compile-pattern-guard`'s `build-icmp-eq`.
#[test]
fn compile_dispatches_a_literal_int_subpattern_to_native_code() {
    let src = r#"
        (defun is-zero ((s Sexpr)) bool (match s ((Int 0) true) (_ false)))
        (compile is-zero)
        "#;
    let zero = run_with_compiler_and_prelude(&format!("{}\n(is-zero (Int 0))", src)).expect("eval failed");
    assert_eq!(zero, RtValue::Int(1));
    let nonzero = run_with_compiler_and_prelude(&format!("{}\n(is-zero (Int 7))", src)).expect("eval failed");
    assert_eq!(nonzero, RtValue::Int(0));
    let other_variant = run_with_compiler_and_prelude(&format!("{}\n(is-zero ())", src)).expect("eval failed");
    assert_eq!(other_variant, RtValue::Int(0), "a different Sexpr variant (Nil) falls through to the wildcard arm, not a tag-test crash");
}

/// Stage 8 of the Sexpr-representation plan (`docs/TODO.md`) — the
/// motivating end goal of the whole 8-stage effort: `dolist` (`prelude.rs`)
/// actually compiles and runs correctly now. `dolist`'s expansion is `(let
/// ((,lst ,lst-expr)) (while (consp ,lst) (let ((,var (car ,lst)))
/// ,@body (setf ,lst (cdr ,lst)))))` — the inner `let`'s body is always at
/// least two statements (`body...` followed by the hidden `setf` that steps
/// the list); `ast_bridge::translate_let`/`compiler.rs`'s `compile-let` used
/// to reject any `let` body but a single expression (see
/// `translate_let`'s doc comment) — lifted for this stage, the same
/// variadic-body treatment `translate_loop`/`compile-loop-body` already had.
/// `not` (`while`'s expansion) and `consp` (`dolist`'s own expansion) are
/// ordinary `defun`s, not one of the 5 `rt_*`-backed primitives exempted
/// from the "callee must already be `compile`d" check (`Expr::Call`'s
/// pre-declaration rule, Stage 3) — both must be explicitly `compile`d
/// before `sum-via-dolist` itself.
#[test]
fn compile_dispatches_a_dolist_based_function_that_sums_a_sexpr_list_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun sum-via-dolist ((seed i64)) i64
          (let ((acc seed))
            (dolist (x (list (Int 1) (Int 2) (Int 3)))
              (match x
                ((Int n) (setf acc (+ acc n)))
                (_ acc)))
            acc))
        (compile not)
        (compile consp)
        (compile sum-via-dolist)
        (sum-via-dolist 0)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(6), "1+2+3");
}

// ---- Stage 2 of the Sexpr-representation plan: tagged-i64 bit primitives --

/// `build-shl`/`build-ashr` round-trip a `Sexpr::Int` fixnum through the
/// planned tagged representation (`docs/TODO.md`'s tag table: tag `000`,
/// payload in the upper 61 bits) — *arithmetic*, not logical, right shift,
/// so a negative payload's sign survives untagging. No `ast_bridge`/
/// `compiler.rs` involvement — these are the raw builtins Stage 5/6's real
/// `compile-match`/tagging helpers will be built out of, the same way
/// `a_function_using_load_arg_and_build_add_computes_correctly` preceded the
/// self-hosted compiler's own use of `build-add`.
#[test]
fn build_shl_and_build_ashr_round_trip_a_signed_fixnum_payload() {
    let src = r#"
        (defun build-fixnum-round-trip-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((f (add-function m "round_trip")))
              (let ((b (append-block f "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (let ((x (load-arg builder f 0)))
                    (let ((three (const-i64 builder 3)))
                      (let ((tagged (build-shl builder x three)))
                        (let ((untagged (build-ashr builder tagged three)))
                          (build-ret builder untagged)
                          m)))))))))
        (build-fixnum-round-trip-module)
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
    let round_trip = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("round_trip")
            .expect("failed to look up the compiled `round_trip` function")
    };
    for x in [0i64, 1, 42, -1, -5, i64::MIN >> 3, i64::MAX >> 3] {
        let argv: [i64; 1] = [x];
        assert_eq!(unsafe { round_trip.call(argv.as_ptr(), argv.len() as u32) }, x, "round trip failed for {}", x);
    }
}

/// `build-shl`/`build-or`/`build-and`/`build-lshr` round-trip a non-fixnum
/// tag (here `010`, the planned `Symbol` tag) and its unsigned index
/// payload — `build-or` to attach the tag, `build-and` to read it back out,
/// `build-lshr` (not `build-ashr`: an index payload is never sign-extended)
/// to recover the payload.
#[test]
fn build_or_and_build_and_pack_and_read_back_a_tag() {
    let src = r#"
        (defun build-tag-round-trip-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((f (add-function m "round_trip")))
              (let ((b (append-block f "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (let ((idx (load-arg builder f 0)))
                    (let ((three (const-i64 builder 3)))
                      (let ((tagged (build-or builder (build-shl builder idx three) (const-i64 builder 2))))
                        (let ((tag-back (build-and builder tagged (const-i64 builder 7))))
                          (let ((idx-back (build-lshr builder tagged three)))
                            (build-ret builder (build-add builder (build-mul builder tag-back (const-i64 builder 1000)) idx-back))
                            m))))))))))
        (build-tag-round-trip-module)
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
    let round_trip = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("round_trip")
            .expect("failed to look up the compiled `round_trip` function")
    };
    // `combined` packs `tag-back * 1000 + idx-back` into one return value so
    // a single call proves both the tag (must read back as `2`) and the
    // payload (must read back as the original index) survived.
    let argv: [i64; 1] = [123];
    assert_eq!(unsafe { round_trip.call(argv.as_ptr(), argv.len() as u32) }, 2123);
}

// ---- Stage 6 of the Sexpr-representation plan: Construct/FieldGet/FieldSet --

/// `Expr::Construct` over `Sexpr` itself: `(Int n)`/`(Bool b)`/`(Nil)` all
/// compile to `compile-construct-sexpr`'s pure bit-tagging path (no heap
/// allocation at all), round-tripping through `Expr::Call`'s existing
/// `Sexpr` decode step (Stage 5) with no further bridging needed. Every
/// constructor here is 0-ary or `i64`-ary on purpose: marshaling a
/// `bool`-typed *argument* across the `Expr::Call` boundary is a separate,
/// already-existing gap (`Interp::eval`'s compiled-call dispatch only
/// special-cases `Sexpr`-vs-`i64` parameters — same family as Stage 5's
/// documented `bool`-*return* gap, just for arguments instead), not
/// something this stage touches. `(Char c)` is exercised at the
/// `ast_bridge`/`compile-construct-sexpr` level only, not here — compiling
/// a bare `char` *literal* at all (`compile-value`'s dispatch has no
/// `"char"` arm, only `compile-construct-sexpr`'s own variant-3 encoding,
/// which still needs one to compile its single argument) is itself a
/// separate, pre-existing gap this stage doesn't touch.
#[test]
fn compile_dispatches_a_function_that_constructs_sexpr_immediates_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun make-int ((n i64)) Sexpr (Int n))
        (compile make-int)
        (make-int 42)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Sexpr(Value::Int(42)));

    let v = run_with_compiler_and_prelude(
        r#"
        (defun make-true () Sexpr (Bool true))
        (compile make-true)
        (make-true)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Sexpr(Value::Bool(true)));

    let v = run_with_compiler_and_prelude(
        r#"
        (defun make-nil () Sexpr (Nil))
        (compile make-nil)
        (make-nil)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Sexpr(Value::Empty));
}

/// `(Cons a-form b-form)` — `compile-construct-sexpr`'s one variant that
/// isn't pure bit math: it calls `rt_cons` (already implemented/connected
/// since Stage 3/4), exactly the inverse of `compile-sexpr-field`'s `cons`
/// extraction. Round-trips the freshly-`Construct`ed pair straight back
/// through `Match`/`compile-sexpr-field` in the same compiled function,
/// proving the two sides agree on the heap layout without any help from
/// the test itself (no raw pointer peeking needed here — see the
/// general-ADT box tests below for where that's still necessary).
#[test]
fn compile_dispatches_a_function_that_constructs_a_cons_via_construct_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun make-pair-sum ((a i64) (b i64)) i64
          (match (Cons (Int a) (Int b))
            ((Cons (Int x) (Int y)) (+ x y))
            (_ -1)))
        (compile make-pair-sum)
        (make-pair-sum 3 4)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(7));
}

/// A general ADT (`Option<i64>`'s `Some`, `AdtKind::Sum`) constructs via
/// `compile-construct-box`'s `malloc`'d-box path instead —
/// `[1, field0]`: slot `0` the variant tag, slot `1` the lone field.
/// `Expr::Call`'s existing `Sexpr`-only decode step (Stage 5) doesn't know
/// about this representation, so the box's raw address surfaces as a
/// (representationally faithful, just not yet correctly *typed*)
/// `RtValue::Int` — the same kind of documented, deferred gap Stage 5 left
/// for a compiled `bool` predicate's return value. Reading the box's own
/// memory directly (the same flavor of test `debug-closure-refcount` exists
/// for `ClosureBox`) is what actually proves `compile-construct-box` built
/// the right thing.
#[test]
fn compile_dispatches_a_function_that_constructs_a_general_adt_box_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun make-some ((n i64)) Option<i64> (Option::some n))
        (compile make-some)
        (make-some 42)
        "#,
    )
    .expect("eval failed");
    let raw = match v {
        RtValue::Int(n) => n,
        other => panic!("expected the box's raw address as an Int (see this test's doc comment), got {:?}", other),
    };
    // SAFETY: `raw` is `build-ptr-to-int`'s result over a `build-malloc`'d,
    // never-freed `[2 x i64]` array (`compile-construct-box`'s layout: slot
    // 0 = variant tag, slot 1 = the lone field) — still valid to read.
    let (variant, field0) = unsafe {
        let p = raw as *const i64;
        (*p, *p.add(1))
    };
    assert_eq!(variant, 0, "Some is option_def's variant 0");
    assert_eq!(field0, 42);
}

/// The same `malloc`'d-box representation for an `AdtKind::Struct`
/// (`defstruct`) instance — `point::new`'s `Expr::Construct` is
/// indistinguishable from `Some`'s above at this stage (both `mutable` and
/// non-`mutable` ADTs share one box layout, see `compile-construct`'s doc
/// comment), just with 2 fields instead of 1 and a single always-`0`
/// variant tag.
///
/// `compile-field-get`/`compile-field-set` themselves aren't exercised
/// end-to-end *here* — `p::x`/`(setf p::y v)` surface syntax always
/// desugars to an *instance-method call* (`Expr::Assoc`, evaluating the
/// auto-generated accessor's body — `Expr::FieldGet`/`FieldSet` — only
/// *inside that method*, never at the call site itself), and at this stage
/// `(compile name)` only ever compiled a top-level `defun` (`self.fns`),
/// never an instance method (`self.methods`). See the
/// `compile_dispatches_a_defstruct_field_accessor_method_to_native_code`/
/// `..._setter_method_to_native_code` tests below (`Interp::resolve_fn_def`/
/// `method_key` — `(compile type::method)`) for the follow-up that closes
/// this specific gap: compiling the accessor/setter *method itself* (whose
/// body is the bare `Expr::FieldGet`/`FieldSet`, no `Expr::Assoc` involved)
/// and wiring `Expr::Assoc`'s own dispatch to use it once compiled, the same
/// way `Expr::Call` already did for top-level functions. A *different* gap
/// stayed open after that, deliberately not attempted there: a top-level
/// `defun` whose *body itself* contains a `p::x`-style call still couldn't be
/// `compile`d at all (`compile-assoc` only recognized an `i64`/`i32`
/// receiver) — see
/// `compile_dispatches_a_function_that_calls_a_compiled_method_in_its_own_body`
/// below for the follow-up that closes *that* gap.
#[test]
fn compile_dispatches_a_function_that_constructs_a_defstruct_instance_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct point (x i64) (y i64))
        (defun make-point ((a i64) (b i64)) point (point::new a b))
        (compile make-point)
        (make-point 3 4)
        "#,
    )
    .expect("eval failed");
    let raw = match v {
        RtValue::Int(n) => n,
        other => panic!("expected the box's raw address as an Int, got {:?}", other),
    };
    // SAFETY: same reasoning as the `Option` box test above, just 2 fields.
    let (variant, x, y) = unsafe {
        let p = raw as *const i64;
        (*p, *p.add(1), *p.add(2))
    };
    assert_eq!(variant, 0, "a defstruct's lone variant is always index 0 (\"new\")");
    assert_eq!(x, 3);
    assert_eq!(y, 4);
}

/// `(compile point::x)`: `Interp::method_key`/`resolve_fn_def` resolve a
/// `"type::method"` name against `self.methods` instead of `self.fns`,
/// compiling `point`'s auto-generated `x` accessor (body: bare
/// `Expr::FieldGet`, no `Expr::Assoc`) on its own — closing the gap the
/// `..._constructs_a_defstruct_instance..._` test above's doc comment
/// describes. `make-point`'s own *construction* stays exactly as before
/// (a `compile`d top-level `defun`, `Expr::Call`-dispatched); only the
/// *read* (`p::x`, an `Expr::Assoc`) is new — `Interp::call_compiled`
/// receives the receiver as `RtValue::Int` (the box's raw address, already
/// in that representation since it came straight out of a `compile`d
/// `Expr::Call` — see that method's doc comment for why a *purely
/// interpreted* `RtValue::Struct` receiver would instead hit a clear
/// internal error, a separate, pre-existing gap), encodes nothing further
/// (the receiver's static type isn't `Sexpr`), and the field itself is a
/// plain `i64`.
#[test]
fn compile_dispatches_a_defstruct_field_accessor_method_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct point (x i64) (y i64))
        (defun make-point ((a i64) (b i64)) point (point::new a b))
        (compile make-point)
        (compile point::x)
        (let ((p (make-point 3 4))) p::x)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(3));
}

/// The `FieldSet` counterpart, through `(setf p::x v)`'s own desugaring to
/// `(set-x p v)` (`Checker::check_setf`) — `compile-field-set` mutates the
/// box `make-point` returned *in place* (the same raw address, never
/// copied), so reading it back afterward (interpreted `p::x`, still
/// dispatching to the now-`compile`d `point::x` from the test above) sees
/// the write.
#[test]
fn compile_dispatches_a_defstruct_field_setter_method_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct point (x i64) (y i64))
        (defun make-point ((a i64) (b i64)) point (point::new a b))
        (compile make-point)
        (compile point::x)
        (compile point::set-x)
        (let ((p (make-point 3 4)))
          (setf p::x 99)
          p::x)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(99));
}

/// The composability gap described in the
/// `compile_dispatches_a_function_that_constructs_a_defstruct_instance_to_native_code`
/// test's doc comment, now closed: `sum-coords`'s body itself contains a
/// `p::x`/`p::y`-style call (`Expr::Assoc`), and it's an ordinary `defun`
/// (`self.fns`), not a method — `compile-assoc`'s new generic branch resolves
/// each to the already-`compile`d `point::x`/`point::y` accessor via the
/// mangled `get-function` lookup, and `Interp::compile_function` forward-
/// declares/wires both *before* `sum-coords`'s own body is compiled (see
/// `compile-assoc`'s and `Interp::compile_function`'s doc comments).
#[test]
fn compile_dispatches_a_function_that_calls_a_compiled_method_in_its_own_body() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct point (x i64) (y i64))
        (defun make-point ((a i64) (b i64)) point (point::new a b))
        (compile make-point)
        (compile point::x)
        (compile point::y)
        (defun sum-coords ((p point)) i64 (+ p::x p::y))
        (compile sum-coords)
        (sum-coords (make-point 3 4))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(7));
}

/// A self-recursive `defmethod` (CLOS-style call syntax, `(countdown self)`
/// — see `Checker::check_defmethod`'s "register the signature before
/// checking the body" comment for why self-recursion is allowed at all)
/// through `compile-assoc`'s same generic branch: the mangled lookup name
/// `counter::countdown` is exactly what `compile-function`'s own first step
/// (`add-function`) already declared in this module before this body was
/// compiled — the same self-recursion precedent `compile-call` already
/// established for a top-level `defun`, just reached through `Expr::Assoc`
/// instead of `Expr::Call`. Also exercises the generic branch's handling of
/// a zero-argument instance method (`argc` 1, the receiver alone) and a
/// `setf` through an already-`compile`d setter method (`counter::set-n`)
/// inside the same recursive call. `make-counter` must be `compile`d too
/// (not just interpreted) so the receiver `countdown` itself first sees is
/// already the raw-address `RtValue::Int` form `Interp::call_compiled`
/// expects — a purely-interpreted `RtValue::Struct` receiver hits a
/// different, pre-existing, documented gap (see `Interp::call_compiled`'s
/// doc comment), not the one this test is for.
#[test]
fn compile_dispatches_a_self_recursive_method_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct counter (n i64))
        (defun make-counter ((a i64)) counter (counter::new a))
        (compile make-counter)
        (compile counter::n)
        (compile counter::set-n)
        (defmethod countdown ((self counter)) i64
          (if (<= self::n 0)
              0
              (let ((ignored (setf self::n (- self::n 1))))
                (countdown self))))
        (compile counter::countdown)
        (countdown (make-counter 5))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(0));
}

/// `Interp::compile_function`'s up-front check for an `Expr::Assoc` target
/// (the method-call counterpart of `compile_errors_clearly_when_a_called_function_is_not_yet_compiled`):
/// `sum-coords` calls `point::x`, but only `point::y` has been `compile`d —
/// a clear `Panic` naming the exact missing method, not a confusing failure
/// from deep inside `compile-assoc`'s own `get-function`.
#[test]
fn compile_of_a_function_calling_an_uncompiled_user_method_is_a_clean_error() {
    let err = run_with_compiler_and_prelude(
        r#"
        (defstruct point (x i64) (y i64))
        (compile point::y)
        (defun sum-coords ((p point)) i64 (+ p::x p::y))
        (compile sum-coords)
        "#,
    )
    .expect_err("expected compiling a caller of an uncompiled method to fail");
    match err {
        EvalError::Panic(msg) => {
            assert!(msg.contains("point::x"), "message was: {}", msg);
            assert!(msg.contains("must be"), "message was: {}", msg);
        }
        other => panic!("expected a Panic, got {:?}", other),
    }
}

/// The other half of `Interp::compile_function`'s `Expr::Assoc`-target check:
/// a receiver type with no `self.methods` entry at all and not `i64`/`i32`
/// (here `f64`, a registry-builtin receiver with no typelisp AST body to
/// `compile` in the first place — still out of scope, see `compile-assoc`'s
/// doc comment) is rejected with its own clear message up front, rather than
/// a deep `get-function` failure from inside the generic branch.
#[test]
fn compile_of_a_function_calling_an_uncompiled_builtin_method_is_a_clean_error() {
    let err = run_with_compiler_and_prelude(
        r#"
        (defun add-floats ((a f64) (b f64)) f64 (+ a b))
        (compile add-floats)
        "#,
    )
    .expect_err("expected compiling a caller of a builtin f64 method to fail");
    match err {
        EvalError::Panic(msg) => {
            assert!(msg.contains("f64::+"), "message was: {}", msg);
            assert!(msg.contains("builtin method"), "message was: {}", msg);
        }
        other => panic!("expected a Panic, got {:?}", other),
    }
}

/// `(compile point::bogus)`/`(compile bogus::x)`: `Interp::method_key`
/// finds no match either way (a real method name on the wrong type, or any
/// method name on a type that was never `defstruct`/`defmethod`-registered)
/// — `resolve_fn_def` reports the same `NoSuchFunction` a plain unknown
/// `defun` name would, not an internal panic.
#[test]
fn compile_of_an_unknown_method_name_is_a_clean_error() {
    let err = run_with_compiler_and_prelude(
        r#"
        (defstruct point (x i64) (y i64))
        (compile point::bogus)
        "#,
    )
    .expect_err("expected compiling an unknown method to fail");
    match err {
        EvalError::NoSuchFunction(name) => assert_eq!(name, "point::bogus"),
        other => panic!("expected a NoSuchFunction, got {:?}", other),
    }

    let err2 = run_with_compiler_and_prelude(
        r#"
        (compile bogus::x)
        "#,
    )
    .expect_err("expected compiling a method on an unknown type to fail");
    match err2 {
        EvalError::NoSuchFunction(name) => assert_eq!(name, "bogus::x"),
        other => panic!("expected a NoSuchFunction, got {:?}", other),
    }
}

/// `(compile "name")`/`(compile "type::method")` — a string literal, not an
/// unevaluated symbol or `::`-path — is a type error caught at check time,
/// before the compiler ever runs (see `Checker::check_compile`'s doc
/// comment for why the name being compiled is program structure, not
/// runtime data).
#[test]
fn compile_of_a_string_literal_is_a_type_error() {
    for src in [
        r#"(defun add2 ((a i64) (b i64)) i64 (+ a b)) (compile "add2")"#,
        r#"(defstruct point (x i64) (y i64)) (compile "point::x")"#,
    ] {
        let mut h = Heap::with_capacity(1 << 16);
        let r = Reader::new();
        let vs = r.read_all(&mut h, src).expect("read failed");
        let mut chk = Checker::new();
        let interp = Interp::new();
        let result = vs.into_iter().try_for_each(|v| chk.check_form(&mut h, &interp, v).map(|_| ()));
        match result {
            Err(Error::TypeError(_)) => {}
            other => panic!("expected a TypeError for {:?}, got {:?}", src, other),
        }
    }
}

/// The "Sexprルート挿入パス" (`docs/TODO.md`): a `let`-bound `Sexpr` local
/// (`bind-let-values`'s new `rt_push_sexpr_root` push) must survive many
/// *unrelated* allocations made later in the same function activation —
/// without it, the fixed-arena `gc()` a tiny `Heap` forces here would
/// reclaim `s`'s cons cell and hand it to one of the throwaway `(Cons s
/// s)` calls instead (the exact corruption `typelisp-rt`'s own
/// `an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations`
/// test demonstrates at the raw-builtin level) — this test demonstrates the
/// *wiring* (`bind-let-values`/`restore-let-values` actually calling it)
/// through a real compiled function instead. Loading the compiler itself
/// (`run_with_compiler_and_capacity`) already leaves a bit over 6300 cells
/// live by the time `churn-and-check` is compiled (measured directly via
/// `Heap::live_count`), so a capacity of `1 << 13` (8192) leaves only a
/// couple thousand free — comfortably exhausted (forcing several real
/// `gc()` calls) by 5000 throwaway `(Cons s s)` iterations, each one
/// otherwise indistinguishable from a value that's about to be reclaimed.
#[test]
fn compile_dispatches_a_function_that_keeps_a_let_bound_sexpr_local_rooted_across_many_allocations() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defun churn-and-check ((n i64)) bool
          (let ((s (Cons (Int 111) (Int 222))))
            (let ((ignored (loop
                             (if (eq n 0) (break) ())
                             (Cons s s)
                             (setf n (- n 1)))))
              (match s
                ((Cons (Int a) (Int b)) (eq (+ a b) 333))
                (_ false)))))
        (compile churn-and-check)
        (churn-and-check 5000)
        "#,
        1 << 13,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(1), "s's contents must survive every intervening allocation");
}

/// A gap the "Sexprルート挿入パス" left for a later stage (`docs/TODO.md`'s
/// "残る選択肢" — temporaries passing through a call-argument array are
/// unrooted): `compile-construct-sexpr`'s `Cons` variant computes its `car`
/// sub-form *first* (storing the freshly-built cons cell's tagged pointer
/// into `args-ptr[0]`), then its `cdr` sub-form — if that second step
/// allocates (here, by calling a separately-compiled `churn`, which loops
/// consing the way `churn-and-check` above does directly), `args-ptr[0]`'s
/// pointer sits in a raw stack slot with no GC root at all until `rt_cons`
/// is finally called with both slots. Same tiny-heap-forces-real-`gc()`
/// technique as the `let`-bound case above, just aimed at a fresh, never-let-
/// bound `car` sub-expression instead.
#[test]
fn compile_dispatches_a_function_that_keeps_a_fresh_cons_car_rooted_while_its_cdr_sub_expression_allocates() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defun churn ((n i64)) i64
          (let ((s (Cons (Int 9) (Int 9))))
            (loop
              (if (eq n 0) (return n) ())
              (Cons s s)
              (setf n (- n 1)))))
        (defun make-and-check ((n i64)) i64
          (match (Cons (Cons (Int 111) (Int 222)) (Int (churn n)))
            ((Cons (Cons (Int a) (Int b)) (Int c)) (+ a b))
            (_ -1)))
        (compile churn)
        (compile make-and-check)
        (make-and-check 5000)
        "#,
        1 << 13,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(333), "the car cons cell's contents must survive the cdr sub-expression's own allocations");
}

/// The call-argument-array counterpart of the `Cons`-field test above:
/// `compile-call-args` (`ast_bridge::tagged_ast_list_to_sexpr`'s `kind`
/// tag, Stage 8 of the Sexpr-representation plan) now `push-sexpr-root`s
/// every `kind = 2` argument right after computing it — without that, the
/// first of `combine`'s two `Sexpr` arguments would sit unrooted in
/// `args-ptr[0]` while the second argument's own sub-expression (a call to
/// `churn`, allocating heavily) runs, exactly the same exposure window as
/// `compile-construct-sexpr`'s `Cons` case, just reached through an ordinary
/// function call instead of a `Construct` literal.
#[test]
fn compile_dispatches_a_call_that_keeps_an_earlier_fresh_sexpr_argument_rooted_while_a_later_argument_allocates() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defun churn ((n i64)) i64
          (let ((s (Cons (Int 9) (Int 9))))
            (loop
              (if (eq n 0) (return n) ())
              (Cons s s)
              (setf n (- n 1)))))
        (defun combine ((a Sexpr) (b Sexpr)) Sexpr (Cons a b))
        (defun make-and-check ((n i64)) i64
          (match (combine (Cons (Int 111) (Int 222)) (Int (churn n)))
            ((Cons (Cons (Int a) (Int b)) (Int c)) (+ a b))
            (_ -1)))
        (compile churn)
        (compile combine)
        (compile make-and-check)
        (make-and-check 5000)
        "#,
        1 << 13,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(333), "combine's first argument must survive its second argument's own allocations");
}

/// The general-ADT-box counterpart of the `Cons`-field/call-argument GC-root
/// tests above, for a `defstruct` field instead: `compile-construct-box-fields`
/// now `push-permanent-sexpr-root`s a `kind = 2` (`Sexpr`-typed) field right
/// after storing it (`ast_bridge::translate_construct`'s new `kind` tagging
/// for a general-ADT `Construct`'s args). Without this, `holder::s`'s
/// `Cons` cell has no GC root at all the moment `make-holder` returns
/// (`release-bindings` unconditionally pops the local that built it, the
/// only thing that *did* root it) — sitting only inside the `malloc`'d box,
/// invisible to `Heap::gc`'s root walk — so `churn`'s own heavy, unrelated
/// consing under a tiny heap reclaims it before `holder::s` is ever read
/// back. Confirmed by temporarily removing the `push-permanent-sexpr-root`
/// call from `compile-construct-box-fields`: this test then fails (`a + b`
/// comes back as something other than `333`, the exact corruption pattern
/// `typelisp-rt`'s own permanent-root tests demonstrate at the raw-builtin
/// level) before restoring the fix.
#[test]
fn compile_dispatches_a_function_that_keeps_a_general_adt_box_field_rooted_across_many_unrelated_allocations() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defstruct holder (s Sexpr))
        (defun make-holder () holder (holder::new (Cons (Int 111) (Int 222))))
        (defun churn ((n i64)) i64
          (let ((s (Cons (Int 9) (Int 9))))
            (loop
              (if (eq n 0) (return n) ())
              (Cons s s)
              (setf n (- n 1)))))
        (compile make-holder)
        (compile churn)
        (compile holder::s)
        (let ((h (make-holder)))
          (let ((ignored (churn 5000)))
            (match h::s
              ((Cons (Int a) (Int b)) (+ a b))
              (_ -1))))
        "#,
        1 << 13,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(333), "holder::s's contents must survive churn's own unrelated allocations after make-holder returns");
}

/// Follow-up to the labels/closures work: the long-standing "known
/// limitation" this module's own doc comment used to describe (a nested
/// `labels`'s inner sibling couldn't direct-call an *outer* `labels`'s own
/// sibling) is now fixed — `compiler.rs`'s `fn-env` is a real `Scope` shared
/// down through `labels` nesting (`push-frame`/`clone-frames`, see that
/// module's doc comment), not a fresh empty table per block. Here `inner-fn`
/// (the sole sibling of the *inner* `labels` block) calls `outer-fn` (the
/// sole sibling of the block enclosing it) directly — neither captures
/// anything, the simplest case this fix covers.
#[test]
fn compile_dispatches_a_nested_labels_inner_sibling_calling_an_outer_sibling_directly() {
    let v = eval_ok_with_compiler(
        r#"
        (defun f () i64
          (labels ((outer-fn ((a i64)) i64 (+ a 1)))
            (labels ((inner-fn ((b i64)) i64 (outer-fn b)))
              (inner-fn 10))))
        (compile f)
        (f)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 11),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Same shape as the no-capture case above, but `outer-fn` itself captures
/// `z` from the enclosing `defun` — `outer-fn` is declared with
/// `add-function-with-env` and expects a 1-element env array.
/// `freevars::labels_free_vars`'s unconditional prefix-copy of
/// `outer_captured` makes the inner block's own captured-list `[z]` too (the
/// same list, even though `inner-fn`'s body never mentions `z` directly), so
/// `compile-apply`'s existing "build the callee's env array from the
/// caller's own shared captured list" logic hands `outer-fn` exactly the
/// `z` value it needs with no further change.
#[test]
fn compile_dispatches_a_nested_labels_inner_sibling_calling_a_capturing_outer_sibling() {
    let v = eval_ok_with_compiler(
        r#"
        (defun f ((z i64)) i64
          (labels ((outer-fn ((a i64)) i64 (+ a z)))
            (labels ((inner-fn ((b i64)) i64 (outer-fn b)))
              (inner-fn 10))))
        (compile f)
        (f 100)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 110),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The inner `labels`'s *trailing* body (not one of its own def bodies)
/// calls the outer `labels`'s sibling directly — `compile-labels` compiles
/// the trailing body with `fn-env` still holding this block's own
/// just-`push-frame`d frame on top of the outer one, so the lookup
/// succeeds the same way a def body's own would.
#[test]
fn compile_dispatches_a_nested_labels_trailing_body_calling_an_outer_sibling() {
    let v = eval_ok_with_compiler(
        r#"
        (defun f ((z i64)) i64
          (labels ((outer-fn ((a i64)) i64 (+ a z)))
            (labels ((dummy ((x i64)) i64 x))
              (outer-fn 10))))
        (compile f)
        (f 100)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 110),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `inner-fn` captures `z` (transitively, via calling `outer-fn`, which
/// itself captures `z`) *and* references its own additional `w` directly —
/// so the inner block's own captured-list `[z, w]` is strictly longer than
/// the outer block's `[z]`. `compile-apply`'s callee env-array build still
/// only reads as many slots as `outer-fn` itself declared captures for
/// (`bind-captures`/`load-env` index purely off `outer-fn`'s own captured
/// list, never off the caller's longer one or any runtime length), so the
/// caller handing over a strictly *longer* array than the callee reads is
/// safe — proving a separate "callee's own captured-list length" table
/// isn't needed, only the unconditional prefix-inheritance in
/// `labels_free_vars`.
#[test]
fn compile_dispatches_a_nested_labels_inner_sibling_with_a_capture_of_its_own_beyond_the_outer_blocks() {
    let v = eval_ok_with_compiler(
        r#"
        (defun f ((z i64) (w i64)) i64
          (labels ((outer-fn ((a i64)) i64 (+ a z)))
            (labels ((inner-fn ((b i64)) i64 (+ (outer-fn b) w)))
              (inner-fn 10))))
        (compile f)
        (f 100 1000)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 1110),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The innermost `labels` sibling, `f3`, calls `f1` — the *outermost*
/// block's sibling, two levels up — skipping `f2`'s own level entirely.
/// `f2` itself never references `f1` or `z`, so this also exercises that
/// `labels_free_vars`'s unconditional prefix-copy still carries `z` through
/// `f2`'s own (otherwise-empty) captured-list down to `f3`'s — proving the
/// fix isn't limited to one level of nesting.
#[test]
fn compile_dispatches_a_triple_nested_labels_call_skipping_the_middle_level() {
    let v = eval_ok_with_compiler(
        r#"
        (defun f ((z i64)) i64
          (labels ((f1 ((a i64)) i64 (+ a z)))
            (labels ((f2 ((a i64)) i64 (* a 2)))
              (labels ((f3 ((a i64)) i64 (f1 a)))
                (f3 10)))))
        (compile f)
        (f 100)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 110),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The inner `labels`'s sibling bare-returns the *outer* `labels`'s own
/// sibling as a value — `resolve-value`'s fallback boxes `outer-fn` into a
/// `ClosureBox` (it's found via `fn-env`, not `env`), exactly like the
/// existing single-level "boxes a bare labels sibling reference" tests,
/// just with the box built one nesting level further out. The box is then
/// called *indirectly*, through another already-compiled function, well
/// outside either `labels` block.
#[test]
fn compile_dispatches_a_nested_labels_inner_sibling_that_boxes_an_outer_sibling_bare() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-it ((z i64)) (fn (i64) i64)
          (labels ((outer-fn ((a i64)) i64 (+ a z)))
            (labels ((grab () (fn (i64) i64) outer-fn))
              (grab))))
        (defun apply-fn ((f (fn (i64) i64)) (n i64)) i64 (f n))
        (compile make-it)
        (compile apply-fn)
        (apply-fn (make-it 100) 10)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 110),
        other => panic!("expected an Int, got {:?}", other),
    }
}

