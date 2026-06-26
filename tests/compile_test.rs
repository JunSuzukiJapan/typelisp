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
    // `match` has no real translation yet (see `ast_bridge`'s module doc
    // comment) — `if`/`let`/`bool` used to stand in for "not yet supported"
    // here too, until if/let/comparisons (labels/closures Stage 5) gave them
    // real translations.
    let err = run_with_compiler(r#"(compile-function (llvm-module::create "mod") "answer" '() '(match))"#)
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
        r#"(compile-function (llvm-module::create "mod") "add2" '((a . false) (b . false)) '(assoc "i64" "+" true (var "a" false) (var "b" false)))"#,
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
        r#"(compile-function (llvm-module::create "mod") "outer" '() '(labels () (("f" ((x . false)) (apply "g" (false var "x" false))) ("g" ((n . false)) (var "n" false))) (apply "f" (false int 5))))"#,
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
        r#"(compile-function (llvm-module::create "mod") "outer" '((offset . false) (n . false)) '(labels ((offset . false)) (("go" ((k . false)) (assoc "i64" "+" true (var "k" false) (var "offset" false)))) (apply "go" (false var "n" false))))"#,
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
          (compile-function m "double" '((x . false)) '(assoc "i64" "+" true (var "x" false) (var "x" false)))
          (compile-function m "quadruple" '((n . false)) '(call "double" (false call "double" (false var "n" false)))))
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
        r#"(to-string (compile-function (llvm-module::create "mod") "f" '((n . false)) '(call "f" (false var "n" false))))"#,
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
        (compile "square")
        (compile "sum-of-squares")
        (sum-of-squares 3 4)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 25),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `Interp::compile_function`'s up-front check (labels/closures Stage 3):
/// `(compile "sum-of-squares")` calls `square`, but nothing has `compile`d
/// `square` yet — a clear `Panic` naming both functions, not a confusing one
/// from deep inside the compiler body's `get-function`.
#[test]
fn compile_errors_clearly_when_a_called_function_is_not_yet_compiled() {
    let err = run_with_compiler(
        r#"
        (defun square ((x i64)) i64 (* x x))
        (defun sum-of-squares ((a i64) (b i64)) i64 (+ (square a) (square b)))
        (compile "sum-of-squares")
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
/// `(compile "name")` JIT path rather than a hand-fed `Sexpr`: proves
/// `Interp::compile_function`'s call-target collection correctly excludes
/// self (no "must be compiled first" error, no forward declaration/
/// `add_global_mapping` wiring attempted for its own name). Deliberately
/// never *called* — see that test's doc comment for why.
#[test]
fn compile_succeeds_for_a_self_recursive_defun_without_being_run() {
    let v = eval_ok_with_compiler(
        r#"
        (defun loop-forever ((n i64)) i64 (loop-forever n))
        (compile "loop-forever")
        "#,
    );
    assert!(expect_bool(v));
}

/// The end-to-end Stage 4 slice (immediate-call, non-capturing): an IIFE
/// (`((lambda (params) body) args...)`) translates to a single-def `labels`
/// block (`ast_bridge::translate_immediate_lambda_call`), not a boxed
/// `ClosureBox` at all — proves that delegation actually produces working,
/// callable code end to end, from real source through `(compile "name")`.
#[test]
fn compile_dispatches_a_defun_with_an_immediately_invoked_lambda_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun calls-immediately ((n i64)) i64 ((lambda ((x i64)) i64 (+ x 1)) n))
        (compile "calls-immediately")
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
        (compile "adds-offset")
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
        (compile "adder")
        (compile "apply-fn")
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
        (compile "square")
        (compile "apply-fn")
        (compile "run-it")
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
        r#"(compile-function (llvm-module::create "mod") "outer" '() '(labels () (("f" ((n . false)) (var "n" false))) (apply-indirect (var "f" true) (false int 5))))"#,
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
        r#"(compile-function (llvm-module::create "mod") "outer" '((offset . false) (n . false)) '(labels ((offset . false)) (("go" ((k . false)) (assoc "i64" "+" true (var "k" false) (var "offset" false)))) (apply-indirect (var "go" true) (false int 5))))"#,
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
        (compile "make-adder")
        (compile "apply-fn")
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
        (compile "make-pair")
        (compile "apply-fn")
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
        (compile "make-caller")
        (compile "apply-fn0")
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
            (compile-function m "outer" '((cb . true) (x . false))
              '(labels ((cb . true)) (("go" ((x . false)) (apply-indirect (var "cb" true) (false var "x" false)))) (apply "go" (false var "x" false))))
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
            (compile-function m "make-wrapper" '((inner . true))
              '(lambda "wrapper$0" ((inner . true)) () (var "inner" true)))
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
            (compile-function m "f" '((cb . true))
              '(lambda "f$0" ((cb . true)) () (var "cb" true)))
            (compile-function m "g" '((cb . true))
              '(lambda "g$0" ((cb . true)) () (var "cb" true)))
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
        r#"(to-string (compile-function (llvm-module::create "mod") "outer" '((x . false))
              '(labels () (("helper" () (int 7))
                           ("go" ((f . true) (y . false)) (apply-indirect (var "f" true) (false var "y" false))))
                 (apply "go" (false var "helper" true) (false var "x" false)))))"#,
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
        r#"(compile-function (llvm-module::create "mod") "lt" '((a . false) (b . false))
              '(assoc "i64" "<" true (var "a" false) (var "b" false)))"#,
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

/// `compile-assoc`'s new receiver-type guard: an `f64` receiver (which
/// defines the very same method names under `registry::float_assoc`) must
/// panic clearly rather than silently misinterpreting its bit pattern as an
/// `i64` — see `compile-assoc`'s doc comment.
#[test]
fn compile_assoc_panics_on_an_unsupported_receiver_type() {
    let err = run_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "addf" '((a . false) (b . false))
              '(assoc "f64" "+" true (var "a" false) (var "b" false)))"#,
    )
    .expect_err("expected a panic for a non-i64/i32 receiver");
    match err {
        EvalError::Panic(msg) => assert!(msg.contains("unsupported receiver type"), "message was: {}", msg),
        other => panic!("expected a Panic, got {:?}", other),
    }
}

/// `compile-if`: `(if is-fn cond-form then-form else-form)` end to end —
/// `max(a, b)` via a comparison feeding the branch, JIT-executed both ways
/// to prove both the `then` and `else` arm are reachable and correct.
#[test]
fn the_compiler_body_compiles_an_if_expression() {
    let module = match eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "maxab" '((a . false) (b . false))
              '(if false
                   (assoc "i64" ">" true (var "a" false) (var "b" false))
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
        r#"(compile-function (llvm-module::create "mod") "shadow_test" '((x . false))
              '(assoc "i64" "+" true
                 (let ((x . (int 99))) (var "x" false))
                 (var "x" false)))"#,
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
            (compile-function m "pick" '((f . true) (g . true))
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
        (compile "fact")
        (fact 10)
        "#,
    );
    match v {
        RtValue::Int(n) => assert_eq!(n, 3628800),
        other => panic!("expected an Int, got {:?}", other),
    }
}
