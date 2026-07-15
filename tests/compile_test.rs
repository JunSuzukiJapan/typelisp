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
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
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
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
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
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn expect_str(v: RtValue) -> String {
    match v {
        RtValue::Str(s) => s.to_string(),
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

/// A `&rest` parameter is bound to a plain `Sexpr` inside the body (see
/// `Checker::check_defun`'s desugaring), and `Sexpr` already has full
/// `compile` support — so a variadic function compiles with no special-
/// casing at all, as long as its body sticks to already-compilable
/// operations on the rest list (e.g. `sexpr-car`/`sexpr-cdr`, which
/// `is_rt_builtin_name` recognizes as `rt_*` shims rather than something
/// `compile_function_rec` needs to recursively compile).
#[test]
fn a_variadic_function_compiles_and_dispatches_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun first-of-rest ((a i64) &rest (xs i64)) i64
          a)
        (compile first-of-rest)
        (first-of-rest 1 10 20)
        "#,
    )
    .expect("eval failed");
    match v {
        RtValue::Int(n) => assert_eq!(n, 1),
        other => panic!("expected an Int, got {:?}", other),
    }
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

/// `(compile sum-of-squares)` calls `square`, which nothing has `compile`d
/// yet — so `Interp::compile_function_rec` (the Iter-compile plan's Stage B
/// transitive driver) compiles `square` first, then `sum-of-squares`, rather
/// than erroring. This is what lets a caller pull in the (whitespace-mangled,
/// un-nameable) monomorphized method instantiations an `Iter` combinator
/// bottoms out in; a plain named callee like `square` is the simplest case
/// of the same recursion.
#[test]
fn compile_transitively_compiles_a_called_function() {
    let v = run_with_compiler(
        r#"
        (defun square ((x i64)) i64 (* x x))
        (defun sum-of-squares ((a i64) (b i64)) i64 (+ (square a) (square b)))
        (compile sum-of-squares)
        (sum-of-squares 3 4)
        "#,
    )
    .expect("transitive compile of `square` should succeed");
    assert_eq!(v, RtValue::Int(25), "3*3 + 4*4 = 25, with `square` auto-compiled");
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

/// `compile-assoc`'s receiver-type guard for a method it still can't lower:
/// `i64::int->char` is a *non-native* `i64` method (`registry::int_assoc`'s
/// `int->char`/`try-int->char` conversions have no compiled primitive behind
/// them, unlike the arithmetic/comparison methods `int-native-method?`
/// covers). It must panic clearly rather than silently misinterpret
/// anything. Reached via the generic user-method branch
/// (`compile-assoc-user`): this hand-fed Sexpr bypasses
/// `Interp::compile_function`'s own up-front check (the normal rejection
/// path, with a clearer message — see
/// `compile_of_a_function_calling_an_unsupported_i64_method_is_a_clean_error`
/// below), so the only thing left to catch it is `get-function` failing to
/// find `"i64::int->char"` in this throwaway module.
#[test]
fn compile_assoc_panics_on_an_unsupported_receiver_type() {
    let err = run_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "tochar" '((a . 0))
              '(assoc "i64" "int->char" true (0 var "a" false)))"#,
    )
    .expect_err("expected a panic for a non-native i64 method");
    match err {
        EvalError::Panic(msg) => assert!(msg.contains("no function named") && msg.contains("i64::int->char"), "message was: {}", msg),
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
///
/// This module is built entirely by hand (`llvm-module::create`, no
/// `Interp::compile_function`/`add_compiled_function` in the loop) and its
/// JIT engine is created with no `externals`/`add_global_mapping` step at
/// all — every extern call it makes (`__typelisp_closure_release`, via
/// `build-closure-release`) resolves purely through LLVM's default
/// process-symbol lookup, since every `rt_*`/`__typelisp_*` function is a
/// `#[no_mangle]` symbol already linked into this very test binary. Once
/// `compile-loop`/`compile-return` started unconditionally reading and
/// truncating the GC root stack (`rt_root_count`/`rt_truncate_sexpr_roots` —
/// see `compiler.rs`'s `compile-loop`/`compile-break`/`compile-return` doc
/// comments), this module needs *some* declaration of those two names for
/// `get-function` to find, even though `pick_via_loop`'s own body never
/// touches a `Sexpr`-typed value — the two `add-function` calls below add
/// exactly that (a body-less declaration, the same shape
/// `Interp::compile_function`'s `declare_external_function` builds), relying
/// on the same automatic symbol resolution `release_box` above already does.
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
            (let ((ignored-rc-decl (add-function m "rt_root_count"))) ())
            (let ((ignored-trunc-decl (add-function m "rt_truncate_sexpr_roots"))) ())
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

    // `pick_via_loop`'s body never touches a `Sexpr`/cons-heap value, but
    // `compile-loop`/`compile-return` now unconditionally call
    // `rt_root_count`/`rt_truncate_sexpr_roots` regardless (see this test's
    // own doc comment) — both dereference the active `Heap` (`typelisp_rt`'s
    // `active_heap()`), which every *real* caller already has registered by
    // the time compiled code runs (`Interp::eval`'s compiled-call dispatch).
    // This test calls the JIT'd functions directly, bypassing `Interp`
    // entirely, so it has to register one itself — the same
    // `set_active_heap` call that dispatch path makes.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);

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

/// `str::append`/`str::length` (`compile-assoc`'s `str` branch): `append`
/// builds a brand-new string at run time (not something `compile-str`'s
/// compile-time character embedding could produce) and `length`s the result,
/// exercising `rt_str_append`'s own fresh allocation. (The former
/// `(match (Str "hi") ((Str content) (eq content "hi")))` string-content-eq
/// sub-cases were dropped when Symbol/Sexpr redesign Phase 5 fenced `match`
/// off `Sexpr`; the fence has since been lifted — see the "compiled `match`
/// on a `Sexpr` scrutinee" section at the end of this file.)
#[test]
fn compile_dispatches_a_function_that_appends_strings_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun str-append-len () i32
          (length (append "foo" "bar")))
        (compile str-append-len)
        (str-append-len)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(6), "\"foo\" ++ \"bar\" has 6 characters");
}

/// A `let`-bound `Type::Str` local survives many unrelated allocations —
/// `ast_bridge::binding_kind`'s Stage 7 extension (`Type::Str` now shares
/// `KIND_SEXPR` with `Sexpr`) is what gives it the same `rt_push_sexpr_root`/
/// `rt_pop_sexpr_root` protection a `let`-bound `Sexpr` local already gets
/// (see `compile_dispatches_a_function_that_keeps_a_let_bound_sexpr_local_rooted_across_many_allocations`,
/// the direct precedent this test mirrors) — without it, `s`'s underlying
/// `StrId` slot could be reclaimed by the `gc()` a later, unrelated `cons`
/// triggers before `length` ever reads it back. Forces that with a tiny
/// heap capacity, the same technique that test and `typelisp-rt`'s own
/// GC-root tests already use.
#[test]
fn compile_dispatches_a_function_that_keeps_a_let_bound_str_local_rooted_across_many_allocations() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defun str-survives-gc ((n i64)) i32
          (let ((s (append "hello" " world")))
            (let ((ignored (loop
                             (if (eq n 0) (break) ())
                             (sexpr-cons (Int 0) (Int 0))
                             (setf n (- n 1)))))
              (length s))))
        (compile str-survives-gc)
        (str-survives-gc 5000)
        "#,
        13000,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(11), "\"hello world\" has 11 characters, even after many unrelated conses force a gc()");
}

/// A general ADT (`Option<i64>`'s `Some`, `AdtKind::Sum`) constructs via
/// `compile-construct-box`'s `malloc`'d-box path instead —
/// `[0, field0]`: slot `0` the variant tag, slot `1` the lone field.
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
        (*p.add(0), *p.add(1))
    };
    assert_eq!(variant, 0, "Some is option_def's variant 0");
    assert_eq!(field0, 42);
}

/// A `mutable` `AdtKind::Struct` (`defstruct`) instance, unlike `Some`'s
/// `malloc`'d-box representation above — `point::new`'s `Expr::Construct`
/// dispatches to `compile-construct-boxed-struct` instead (Stage 3 of the
/// Sexpr/RtValue unification plan, `docs/implementation-log.md`: the
/// `mutable` flag `Checker::check_construct` sets for an `AdtKind::Struct`
/// def), building a `BoxedObj::Struct` via `rt_struct_new` — the exact same
/// tagged `Value::Boxed` representation the interpreter's own
/// `Expr::Construct` `mutable` arm already builds (`heap.alloc_struct`), so
/// `Interp::call_compiled` decodes `make-point`'s raw compiled return value
/// back into `RtValue::Sexpr(Value::Boxed(_))` (`Self::is_boxed_sexpr_type`,
/// `point` recorded in `Self::struct_types` by its own `TopLevel::Defstruct`)
/// rather than treating it as a raw pointer `RtValue::Int`, unlike the
/// pre-Stage-3 general-ADT box case.
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
    match v {
        RtValue::Sexpr(Value::Boxed(_)) => {}
        other => panic!("expected a boxed struct Sexpr (compile-construct-boxed-struct), got {:?}", other),
    }
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

/// The method-call counterpart of `compile_transitively_compiles_a_called_function`
/// (Stage B): `sum-coords` calls the auto-generated `point::x`/`point::y`
/// accessors, neither `compile`d in advance — `compile_function_rec` compiles
/// each on demand (their bodies are the `Expr::FieldGet` accessors registered
/// in `self.methods`) rather than erroring.
#[test]
fn compile_transitively_compiles_a_called_user_method() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct point (x i64) (y i64))
        (defun sum-coords ((p point)) i64 (+ p::x p::y))
        (compile sum-coords)
        (sum-coords (point::new 3 4))
        "#,
    )
    .expect("transitive compile of the `point` accessors should succeed");
    assert_eq!(v, RtValue::Int(7), "3 + 4 = 7, with `point::x`/`point::y` auto-compiled");
}

/// The other half of `Interp::compile_function`'s `Expr::Assoc`-target check:
/// a *non-native* builtin method on an otherwise-native receiver — here
/// `i64::int->char` (`registry::int_assoc`'s conversion, no compiled
/// primitive backing it, unlike the arithmetic/comparison methods
/// `compile-assoc`'s i64 branch lowers) — is rejected with its own clear
/// "no function named" message from `compile-assoc-user`'s `get-function`,
/// rather than silently misbehaving. (`i64`/`i32` are in the native-receiver
/// exclusion list, so this reaches the compiler rather than the up-front
/// method-target check — the same treatment `compile_assoc_panics_on_an_unsupported_receiver_type`
/// exercises directly against a hand-fed Sexpr.)
#[test]
fn compile_of_a_function_calling_an_unsupported_i64_method_is_a_clean_error() {
    let err = run_with_compiler_and_prelude(
        r#"
        (defun root ((a i64)) char (int->char a))
        (compile root)
        "#,
    )
    .expect_err("expected compiling a caller of the non-native i64 `int->char` to fail");
    match err {
        EvalError::Panic(msg) => {
            assert!(msg.contains("i64::int->char"), "message was: {}", msg);
            assert!(msg.contains("no function named"), "message was: {}", msg);
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
        let result = vs
            .into_iter()
            .try_for_each(|v| chk.check_form(&mut h, &interp, v).map_err(Error::into_kind).map(|_| ()));
        match result {
            Err(Error::TypeError(_)) => {}
            other => panic!("expected a TypeError for {:?}, got {:?}", src, other),
        }
    }
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

// ---- where-bounded generics × compiled methods (monomorphization) ----------
//
// Before monomorphization these two tests exercised `(compile describe)` —
// compiling the *generic* `describe` itself, whose body checked as
// `Expr::TraitCall` and lowered to `compile-trait-dispatch`'s runtime
// type-id chain. Monomorphization made that whole configuration
// unrepresentable: a generic `defun`'s erased body is never registered in
// `Interp::fns` (see `Interp::exec`'s `Defun` arm), so it cannot be
// `compile`d — each call site instead instantiates a fully concrete
// specialization whose `(count it)` resolves statically to `Expr::Assoc`.
// What's still worth proving here is the boundary the old tests also
// covered: a specialized generic body dispatching into already-`compile`d
// methods on a receiver built by already-`compile`d code.

/// `describe`'s call site instantiates `describe <box-a>`, whose re-checked
/// body resolves `(count it)` to a plain `Expr::Assoc` on `box-a` — which
/// `Interp`'s `Assoc` arm routes to the *compiled* `box-a::count`
/// (`compiled_methods` is consulted first), with the receiver itself built
/// by the compiled `make-box-a`.
#[test]
fn a_where_bounded_generic_specializes_and_dispatches_into_compiled_methods() {
    let v = run_with_compiler(
        r#"
        (deftrait Counted (count ((self Self)) i32))
        (defstruct box-a (n i32))
        (impl Counted box-a (count ((self Self)) i32 self::n))
        (defun describe<T> ((it T)) i32 (where (Counted T)) (count it))
        (defun make-box-a ((n i32)) box-a (box-a::new n))
        (compile box-a::n)
        (compile box-a::count)
        (compile make-box-a)
        (describe (make-box-a 7))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(7));
}

/// The multi-impl counterpart: two call sites instantiate `describe <box-a>`
/// and `describe <box-b>` independently, and each specialization's static
/// `Assoc` reaches the *right* compiled method — mirrors
/// `two_types_implementing_the_same_trait_dispatch_independently`
/// (`tests/trait_test.rs`), with the method bodies native.
#[test]
fn a_where_bounded_generic_specializes_per_impl_and_dispatches_into_compiled_methods() {
    let v = run_with_compiler(
        r#"
        (deftrait Counted (count ((self Self)) i32))
        (defstruct box-a (n i32))
        (defstruct box-b (n i32))
        (impl Counted box-a (count ((self Self)) i32 self::n))
        (impl Counted box-b (count ((self Self)) i32 (* 2 self::n)))
        (defun describe<T> ((it T)) i32 (where (Counted T)) (count it))
        (defun make-box-a ((n i32)) box-a (box-a::new n))
        (defun make-box-b ((n i32)) box-b (box-b::new n))
        (compile box-a::n)
        (compile box-b::n)
        (compile box-a::count)
        (compile box-b::count)
        (compile make-box-a)
        (compile make-box-b)
        (+ (describe (make-box-a 3)) (describe (make-box-b 3)))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(3 + 6));
}


/// Demonstrates the invalid-IR risk `compile-let`'s `unroot-let-sexpr-values`
/// doc comment describes: a `let` binding a `Sexpr`-typed value (`s`, kind
/// `2`) whose body exits early via a bare `(return 42)` inside a `loop` has
/// already closed the current block with `compile-return`'s own `build-br`
/// to the loop's exit block by the time `compile-let` would otherwise
/// unconditionally call `unroot-let-sexpr-values` — which itself builds a
/// `call rt_pop_sexpr_root` instruction. Without the `block-terminated?`
/// guard around that call, this lands *after* the block's terminator: not
/// merely a leak but a malformed basic block (LLVM requires exactly one
/// terminator, as the very last instruction). Reverting that guard reliably
/// reproduces the failure this test guards against — `Interp::compile_function`'s
/// own `module.verify()` call (added alongside this fix specifically because
/// this bug could otherwise reach the JIT engine as undefined behavior
/// instead of a catchable error) rejects the module with "Terminator found
/// in the middle of a basic block!", so `(compile make-thing)` itself fails
/// rather than `(make-thing)` returning a corrupted value.
#[test]
fn compile_let_does_not_emit_instructions_after_an_early_return_from_its_body() {
    let v = run_with_compiler(
        r#"
        (defun make-thing () i32
          (loop
            (let ((s (sexpr-cons (Int 1) (Int 2))))
              (return 42))))
        (compile make-thing)
        (make-thing)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(42));
}

/// The general fix `compile_let_does_not_emit_instructions_after_an_early_return_from_its_body`'s
/// own doc comment flags as a follow-up: `block-terminated?` there stops
/// `compile-let` from generating invalid IR, but the `Sexpr`-typed binding's
/// GC root still went unpopped on that path — a leak that grows by one every
/// time such a function is *called*, not just once, since `Heap` (and its
/// root stack) persists across every compiled call made through the same
/// `Interp`. `compile-loop` now reads the root stack's depth once on entry
/// (`rt_root_count`) and `compile-break`/`compile-return` unconditionally
/// reset it back there (`rt_truncate_sexpr_roots`) right before jumping out,
/// regardless of how many `let`/`match` scopes are open above them — see
/// `compiler.rs`'s `compile-loop`/`compile-break`/`compile-return` doc
/// comments.
///
/// `Heap::root_count()` isn't purely a compiled-code metric, though — the
/// tree-walking interpreter's own `Interp::sync_roots` also grows the same
/// stack by a small, unrelated amount on every top-level call (its own
/// GC-safety bookkeeping, nothing to do with this fix), so a bare "must stay
/// at exactly its post-setup value" assertion would be comparing against
/// noise. Instead this measures `leaky-inner`'s (a `let` binding a fresh
/// `Sexpr` immediately followed by an unconditional `return`, inside a
/// `loop`) net root growth over many calls against `trivial`'s (a
/// same-shape, no-`Sexpr`, non-looping compiled function) over the same
/// number of calls: if `compile-return`'s truncation is working, `leaky-inner`
/// should leak *nothing beyond* whatever background growth `trivial` already
/// has, so the two deltas should come out equal. Reverting the
/// `compile-break`/`compile-return` truncation (while keeping the
/// `block-terminated?` guard, so the IR stays valid) reliably reproduces
/// `leaky-inner` growing by one root more than `trivial` per call, the
/// failure this test guards against.
#[test]
fn compile_return_truncates_a_sexpr_lets_gc_root_on_every_call_not_just_the_first() {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();

    let setup = r#"
        (defun trivial () i32 7)
        (compile trivial)
        (defun leaky-inner () i32
          (loop
            (let ((s (sexpr-cons (Int 1) (Int 2))))
              (return 7))))
        (compile leaky-inner)
        "#;
    for v in r.read_all(&mut h, setup).expect("read failed") {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        interp.exec(&mut h, tl).expect("exec failed");
    }

    const CALLS: usize = 500;

    let before_trivial = h.root_count();
    for _ in 0..CALLS {
        for v in r.read_all(&mut h, "(trivial)").expect("read failed") {
            let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
            let result = interp.exec(&mut h, tl).expect("exec failed").expect("trivial should produce a value");
            assert_eq!(result, RtValue::Int(7));
        }
    }
    let trivial_growth = h.root_count() - before_trivial;

    let before_leaky = h.root_count();
    for _ in 0..CALLS {
        for v in r.read_all(&mut h, "(leaky-inner)").expect("read failed") {
            let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
            let result = interp.exec(&mut h, tl).expect("exec failed").expect("leaky-inner should produce a value");
            assert_eq!(result, RtValue::Int(7));
        }
    }
    let leaky_growth = h.root_count() - before_leaky;

    assert_eq!(
        leaky_growth, trivial_growth,
        "leaky-inner's own let-bound Sexpr root must not leak beyond whatever background growth an equivalent non-Sexpr, non-looping compiled function already has"
    );
}

// ---- nested boxed-struct fields ---------------------------------------------
//
// A `defstruct` field whose declared type is *itself* a `defstruct` (an
// `AdtKind::Struct` `Type::Named`) compiles as `struct_field_kind` `6` — the
// value is already a properly tagged boxed-struct `Sexpr`, so both
// `compile-tag-struct-field` (encode) and `compile-sexpr-field` (decode)
// pass it through unchanged, exactly like a `Str`/`Sexpr` field. Before
// `ast_bridge` was handed the checker-resolved struct-type set
// (`Interp::struct_types`), such a field fell into the `0` "not
// representable" kind — the compile below panicked — for a *wrong* reason:
// the classification (`AdtDef::kind`) was fully known at check time, the
// bridge just had no channel for it.

/// Construction with a struct-typed field (`outer::new`'s `inner` argument,
/// `compile-construct-boxed-struct-fields`' kind-6 passthrough) plus a read
/// back through both compiled accessors (`outer::child`'s own body is an
/// `Expr::FieldGet` whose type is `inner` — kind 6 on the decode side).
#[test]
fn compile_dispatches_a_function_that_constructs_a_nested_defstruct_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct inner (v i64))
        (defstruct outer (child inner))
        (defun make-outer ((n i64)) outer (outer::new (inner::new n)))
        (compile make-outer)
        (compile inner::v)
        (compile outer::child)
        (defun read-nested ((o outer)) i64 (v o::child))
        (compile read-nested)
        (read-nested (make-outer 41))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(41));
}

/// The `FieldSet` counterpart: a compiled setter whose value operand is a
/// struct-typed field (`compile-field-set`'s `compile-tag-struct-field`
/// kind-6 passthrough), replacing the nested instance in place; the
/// replacement is constructed by the *interpreter* (`heap.alloc_struct`) and
/// crosses into compiled code through `Interp::call_compiled`'s ordinary
/// `RtValue::Sexpr` encoding — pinning that the two sides still agree on
/// the representation for nested structs.
#[test]
fn compile_dispatches_a_nested_defstruct_field_setter_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct inner (v i64))
        (defstruct outer (child inner))
        (defun make-outer ((n i64)) outer (outer::new (inner::new n)))
        (compile make-outer)
        (compile inner::v)
        (compile outer::child)
        (compile outer::set-child)
        (let ((o (make-outer 1)))
          (setf o::child (inner::new 9))
          (v o::child))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(9));
}

/// A user-defined method on a *primitive* receiver — the prelude's `impl Eq
/// i32` registers `i32::equals` as an ordinary user method whose body
/// `(= self other)` lowers natively. Two former gates blocked this path:
/// `Interp::compile_function` excluded every `i64`/`i32`/`string` assoc
/// target from forward-declaration wholesale, and `compile-assoc`'s int
/// branch dead-ended any method outside its fixed operator list into a
/// panic instead of falling through to the mangled-name user-method call
/// (`compile-assoc-user`).
#[test]
fn compile_dispatches_a_user_method_on_a_primitive_receiver() {
    let v = run_with_compiler_and_prelude(
        r#"
        (compile i32::equals)
        (defun both-equal ((a i32) (b i32) (c i32)) bool
          (if (equals a b) (equals b c) false))
        (compile both-equal)
        (if (both-equal 3 3 3) (if (both-equal 3 3 4) false true) false)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Bool(true));
}

/// The `Ord` counterpart of the previous test: `i32::less` (body
/// `(< self other)`) compiles natively and a compiled caller reaches it
/// through `compile-assoc-user`'s mangled-name call.
#[test]
fn compile_dispatches_less_on_a_primitive_receiver() {
    let v = run_with_compiler_and_prelude(
        r#"
        (compile i32::less)
        (defun strictly-between ((lo i32) (x i32) (hi i32)) bool
          (if (less lo x) (less x hi) false))
        (compile strictly-between)
        (if (strictly-between 1 5 9) (if (strictly-between 1 9 5) false true) false)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Bool(true));
}

/// A user method on a *primitive* receiver (the prelude's `impl Eq i32` ->
/// `i32::equals`) is transitively compiled too (Stage B): `(compile run)`
/// pulls in `i32::equals` on demand. `i32::equals`'s own body delegates to
/// the native `=` (lowered in-place by `compile-assoc`, no further call), so
/// the recursion bottoms out immediately. (A *builtin* method with no body —
/// e.g. an `f64`/`char` builtin — still errors clearly; only methods with a
/// real registered body are auto-compiled.)
#[test]
fn compile_transitively_compiles_a_called_primitive_receiver_method() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun eq2 ((a i32) (b i32)) bool (equals a b))
        (defun run () i64 (if (eq2 5 5) (if (eq2 5 6) 0 1) 0))
        (compile run)
        (run)
        "#,
    )
    .expect("transitive compile of `i32::equals` should succeed");
    assert_eq!(v, RtValue::Int(1), "eq2(5,5) true and eq2(5,6) false, with `i32::equals` auto-compiled");
}

/// The `string` counterpart: `string::equals`'s body is `(equal self
/// other)` (content comparison, `rt_str_eq`) and `string::less`'s is
/// `(lt self other)` (lexicographic, the new `rt_str_lt` shim) — both now
/// on `string-native-method?`'s list, so the impl bodies themselves compile
/// and a compiled caller reaches them through `compile-assoc-user`.
#[test]
fn compile_dispatches_string_equals_and_less() {
    let v = run_with_compiler_and_prelude(
        r#"
        (compile string::equals)
        (compile string::less)
        (defun str-cmp ((a string) (b string)) i64
          (if (equals a b) 0 (if (less a b) -1 1)))
        (compile str-cmp)
        (+ (str-cmp (append "ab" "c") "abc")
           (+ (* (str-cmp "abc" "abd") 10)
              (* (str-cmp "b" "a") 100)))
        "#,
    )
    .expect("eval failed");
    // equal content (built separately) -> 0, "abc" < "abd" -> -10, "b" > "a" -> 100
    assert_eq!(v, RtValue::Int(90));
}

/// And `char`: a compiled `char` is a raw `i64` code point, so
/// `char::equals` (body `(equal self other)`) and `char::less` (body
/// `(lt self other)`) lower to plain integer `icmp`s via the char-native
/// branch.
#[test]
fn compile_dispatches_char_equals_and_less() {
    let v = run_with_compiler_and_prelude(
        r#"
        (compile char::equals)
        (compile char::less)
        (defun char-cmp ((a char) (b char)) i64
          (if (equals a b) 0 (if (less a b) -1 1)))
        (compile char-cmp)
        (+ (char-cmp (ref "xa" 1) (ref "ya" 1))
           (+ (* (char-cmp (ref "a" 0) (ref "b" 0)) 10)
              (* (char-cmp (ref "b" 0) (ref "a" 0)) 100)))
        "#,
    )
    .expect("eval failed");
    // 'a'=='a' -> 0, 'a'<'b' -> -10, 'b'>'a' -> 100
    assert_eq!(v, RtValue::Int(90));
}

// ---- `< <= > >=` comparison operators on char/string, compiled --------------
//
// The operators are overloaded builtin methods on char/string (like the numeric
// ones): char lowers to integer `icmp`s (raw code point), string derives all
// four from `rt_str_lt` (`compiler.rs`'s `str-lt-call`). No prelude needed —
// they're native builtins, reached through `compile-assoc`'s char/string arms.

#[test]
fn compile_dispatches_char_comparison_operators() {
    let v = run_with_compiler(
        r#"
        (defun ccmp ((a char) (b char)) i64
          (if (< a b) 1 (if (<= a b) 2 (if (> a b) 3 4))))
        (compile ccmp)
        (+ (ccmp (ref "ab" 0) (ref "ab" 1))
           (+ (* (ccmp (ref "ba" 0) (ref "ba" 1)) 10)
              (* (ccmp (ref "aa" 0) (ref "aa" 1)) 100)))
        "#,
    )
    .expect("eval failed");
    // 'a'<'b' -> 1 ; 'b' vs 'a': >  -> 3 -> 30 ; 'a' vs 'a': <= -> 2 -> 200
    assert_eq!(v, RtValue::Int(231));
}

#[test]
fn compile_dispatches_string_comparison_operators() {
    let v = run_with_compiler(
        r#"
        (defun scmp ((a string) (b string)) i64
          (if (< a b) 1 (if (<= a b) 2 (if (> a b) 3 4))))
        (compile scmp)
        (+ (scmp "a" "b")
           (+ (* (scmp "b" "a") 10)
              (* (scmp "x" "x") 100)))
        "#,
    )
    .expect("eval failed");
    // "a"<"b" -> 1 ; "b" vs "a": > -> 3 -> 30 ; "x" vs "x": <= -> 2 -> 200
    assert_eq!(v, RtValue::Int(231));
}

#[test]
fn compile_string_ge_and_le_derive_from_rt_str_lt() {
    // Exercises `>=`/`<=` specifically (the `not rt_str_lt(...)` derivations).
    let v = run_with_compiler(
        r#"
        (defun sle ((a string) (b string)) i64 (if (<= a b) 1 0))
        (defun sge ((a string) (b string)) i64 (if (>= a b) 1 0))
        (compile sle)
        (compile sge)
        (+ (sle "a" "a")
           (+ (* (sle "b" "a") 10)
              (+ (* (sge "a" "a") 100)
                 (* (sge "a" "b") 1000))))
        "#,
    )
    .expect("eval failed");
    // sle("a","a")=1 ; sle("b","a")=0 ; sge("a","a")=1 ->100 ; sge("a","b")=0
    assert_eq!(v, RtValue::Int(101));
}

// ---- `defenum` / sum-ADT `match` in compiled code (box scrutinee) ----------
//
// Construction already lowered to a `compile-construct-box` box (slot 0 = tag,
// slot 1+ = fields); these exercise the box-scrutinee `match` path added
// alongside `defenum` — `ast_bridge::translate_match`'s `is-box` branch plus
// `compiler.rs`'s `compile-box-tag-test`/`compile-box-field`. Closes the
// `docs/dev/TODO.md` gap where `Match` on a non-`Sexpr` scrutinee was
// `unsupported`.

/// A compiled `match` on a payload variant tests the box's tag slot and
/// extracts the field from slot 1.
#[test]
fn compile_matches_a_payload_variant_and_extracts_its_field() {
    // The payload comes from an i64 parameter so the field type is unambiguous
    // (a bare literal `7` would default to i32 and clash with the i64 return).
    let v = eval_ok_with_compiler(
        r#"
        (defenum Maybe<T> (Just T) (Nothing))
        (defun m ((x i64)) i64 (match (Maybe::Just x) ((Just v) v) ((Nothing) x)))
        (compile m)
        (m 7)
        "#,
    );
    assert_eq!(v, RtValue::Int(7));
}

/// A compiled `match` discriminates across three nullary variants by the
/// box's variant-tag slot, selecting the correct arm.
#[test]
fn compile_matches_discriminates_among_nullary_variants() {
    let v = eval_ok_with_compiler(
        r#"
        (defenum Sign (Neg) (Zero) (Pos))
        (defun classify () i64 (match (Sign::Pos) ((Neg) 10) ((Zero) 20) ((Pos) 30)))
        (compile classify)
        (classify)
        "#,
    );
    assert_eq!(v, RtValue::Int(30));
}

/// A runtime-chosen variant (both arms reachable): the `if` merges two boxes,
/// and the compiled `match` picks the arm by the tag slot — the payload arm
/// extracts, the nullary arm doesn't.
#[test]
fn compile_matches_a_runtime_chosen_variant() {
    // The payload `Just` branch is the `if`'s `then` so its `T=i64` is inferred
    // before the payload-less `Nothing` `else` (which can't infer `T` alone —
    // an inference ordering property, the same in the interpreter).
    let src = r#"
        (defenum Maybe<T> (Just T) (Nothing))
        (defun pick ((n i64)) i64
          (match (if (eq n 0) (Maybe::Just n) (Maybe::Nothing))
            ((Just v) v)
            ((Nothing) 99)))
        (compile pick)
        (pick %ARG%)
    "#;
    assert_eq!(eval_ok_with_compiler(&src.replace("%ARG%", "0")), RtValue::Int(0));
    assert_eq!(eval_ok_with_compiler(&src.replace("%ARG%", "5")), RtValue::Int(99));
}

/// The compiled result must agree with the tree-walking interpreter for the
/// same `defenum` `match` — the JIT/interp-parity check the plan calls for.
#[test]
fn compile_and_interpret_agree_on_a_defenum_match() {
    let prog = r#"
        (defenum Maybe<T> (Just T) (Nothing))
        (defun m ((x i64)) i64 (match (Maybe::Just x) ((Just v) (+ v 1)) ((Nothing) x)))
    "#;
    let compiled = eval_ok_with_compiler(&format!("{}\n(compile m)\n(m 41)", prog));
    let interpreted = eval_ok(&format!("{}\n(m 41)", prog));
    assert_eq!(compiled, interpreted);
    assert_eq!(compiled, RtValue::Int(42));
}

/// Built-in `Option` is itself a sum-ADT box, so a compiled `match` on it now
/// works too (previously `unsupported`), for free from the same generalization.
/// The `Option` is constructed *inside* compiled code (passing an already-built
/// `RtValue::Data` across the interp→compiled boundary is a separate, unrelated
/// argument-marshalling gap).
#[test]
fn compile_matches_a_builtin_option() {
    let v = eval_ok_with_compiler(
        r#"
        (defun u ((n i64)) i64 (match (option::some n) ((Some v) v) ((None) n)))
        (compile u)
        (u 5)
        "#,
    );
    assert_eq!(v, RtValue::Int(5));
}

// ---- boxed-struct (`defstruct`) `match` in compiled code -------------------
//
// A `defstruct`/`Vector` scrutinee is a properly tagged `Sexpr`
// (`BoxedObj::Struct`) like the plain-`Sexpr` case, but has exactly one
// variant (its own `new` constructor), so no tag test is ever emitted for it
// — only per-field extraction via `rt_struct_field_get`/`compile-sexpr-field`
// (`ast_bridge::pattern_to_sexpr`'s `MATCH_KIND_STRUCT` branch,
// `compiler.rs`'s `compile-struct-field`). Closes the last remaining
// `docs/dev/TODO.md` gap for compiled `Match`.

/// A compiled `match` destructures a `defstruct` instance's fields by
/// position, the same as the interpreter's own
/// `match_destructures_a_struct_instance` (`tests/struct_test.rs`).
#[test]
fn compile_matches_and_destructures_a_defstruct_instance() {
    let v = eval_ok_with_compiler(
        r#"
        (defstruct point (x i64) (y i64))
        (defun sum ((p point)) i64 (match p ((new a b) (+ a b))))
        (compile sum)
        (sum (point::new 3 4))
        "#,
    );
    assert_eq!(v, RtValue::Int(7));
}

/// A wildcard sub-pattern skips field extraction entirely, and binding order
/// follows field declaration order, not name — mirrors the interpreter's own
/// `match_on_a_struct_binds_fields_by_position`.
#[test]
fn compile_match_on_a_defstruct_binds_fields_by_position_and_skips_wildcards() {
    let v = eval_ok_with_compiler(
        r#"
        (defstruct point (x i64) (y i64) (z i64))
        (defun diff ((p point)) i64 (match p ((new a _ c) (- a c))))
        (compile diff)
        (diff (point::new 9 100 3))
        "#,
    );
    assert_eq!(v, RtValue::Int(6));
}

/// Compiled and interpreted `match` agree over the same `defstruct` instance
/// (the JIT/interp-parity check every other scrutinee kind above has).
#[test]
fn compile_and_interpret_agree_on_a_defstruct_match() {
    let prog = r#"
        (defstruct point (x i64) (y i64))
        (defun sum ((p point)) i64 (match p ((new a b) (+ a b))))
    "#;
    let compiled = eval_ok_with_compiler(&format!("{}\n(compile sum)\n(sum (point::new 5 6))", prog));
    let interpreted = eval_ok(&format!("{}\n(sum (point::new 5 6))", prog));
    assert_eq!(compiled, interpreted);
    assert_eq!(compiled, RtValue::Int(11));
}

// ---- compiled `match` on a `Sexpr` scrutinee ---------------------------------
//
// Re-enabled after Symbol/Sexpr redesign Phase 5's checker fence was lifted
// (in preparation for a user-facing `(read)` — see `tests/match_sexpr_test.rs`'s
// module doc for the interp side). The Sexpr-scrutinee machinery below
// (`compile-sexpr-tag-test`/`compile-sexpr-field`) predates the fence; what's
// new on this side is `rt_box_kind`: `float`/`bignum`/`ratio` all share
// `TAG_BOXED`, so their tag tests can't tell them apart by the 3-bit tag alone.

/// A compiled `match` over a `Sexpr` scrutinee dispatches on the tagged-i64
/// representation and extracts `int`/`cons` payloads, nested pattern included.
#[test]
fn compile_matches_a_sexpr_scrutinee_and_extracts_payloads() {
    let v = eval_ok_with_compiler(
        r#"
        (defun f ((s Sexpr)) i64
          (match s
            ((int n) n)
            ((cons (int a) _) a)
            (_ 0)))
        (compile f)
        (+ (f (Int 40)) (f (sexpr-cons (Int 2) (Str "tail"))))
        "#,
    );
    assert_eq!(v, RtValue::Int(42));
}

/// The three numeric boxed variants share `TAG_BOXED`, so their arms dispatch
/// through `rt_box_kind` — a bignum/ratio scrutinee must *not* take a
/// preceding `(float _)` arm even though it carries the same 3-bit tag.
#[test]
fn compile_match_distinguishes_float_bignum_and_ratio_boxes() {
    let v = eval_ok_with_compiler(
        r#"
        (defun which ((s Sexpr)) i64
          (match s
            ((float _) 1)
            ((bignum _) 2)
            ((ratio _) 3)
            (_ 0)))
        (compile which)
        (+ (+ (which (Float 1.5))
              (* (the i64 10) (which (Bignum 99999999999999999999999999))))
           (* (the i64 100) (which (Ratio 2/3))))
        "#,
    );
    assert_eq!(v, RtValue::Int(321));
}

/// Tag-only dispatch covers every variant, including `sym`, whose *payload*
/// still has no compiled representation (binding it is a clear
/// `compile-sexpr-field` panic; `(sym _)` never extracts, so it compiles).
#[test]
fn compile_match_dispatches_nil_sym_str_and_bool_by_tag() {
    let v = eval_ok_with_compiler(
        r#"
        (defun tag ((s Sexpr)) i64
          (match s
            ((nil) 1) ((sym _) 2) ((str _) 3) ((bool _) 4)
            (_ 0)))
        (compile tag)
        (+ (+ (tag (Nil)) (* (the i64 10) (tag (quote foo))))
           (+ (* (the i64 100) (tag (Str "s"))) (* (the i64 1000) (tag (Bool false)))))
        "#,
    );
    assert_eq!(v, RtValue::Int(4321));
}

/// Compiled and interpreted `match` agree over the same Sexpr inputs.
#[test]
fn compile_and_interpret_agree_on_a_sexpr_match() {
    let src = |call: &str| {
        format!(
            r#"
            (defun sum ((s Sexpr)) i64
              (match s
                ((cons (int n) rest) (+ n (sum rest)))
                (_ 0)))
            {}
            (sum (quote (1 2 3 4)))
            "#,
            call
        )
    };
    let interp_v = eval_ok_with_compiler(&src(""));
    let compiled_v = eval_ok_with_compiler(&src("(compile sum)"));
    assert_eq!(interp_v, RtValue::Int(10));
    assert_eq!(compiled_v, interp_v);
}

/// A `compile`d function can read a `defvar` global — `Expr::Global`'s
/// real `ast_bridge` translation (`(global id)`) and `compiler.rs`'s
/// `compile-global`.
#[test]
fn compile_dispatches_a_function_that_reads_a_global_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defvar (counter i64) 41)
        (defun read-counter () i64 counter)
        (compile read-counter)
        (read-counter)
        "#,
    );
    assert_eq!(v, RtValue::Int(41));
}

/// A `compile`d function can assign a `defvar` global (`Expr::SetGlobal`'s
/// real translation, `(set-global id is-fn value-form)` /
/// `compiler.rs`'s `compile-set-global`) — and an *interpreted* read of the
/// same global afterward sees the compiled write, not a stale value: once a
/// global is promoted to a compiled-global slot (a permanent GC root),
/// `Interp`'s own `Expr::Global`/`Expr::SetGlobal` evaluation reads/writes
/// that same slot too (see `Interp::promote_global`'s doc comment) rather
/// than the plain `Slot` it used before promotion.
#[test]
fn compile_dispatches_a_function_that_writes_a_global_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defvar (counter i64) 0)
        (defun bump-counter () i64 (setf counter (+ counter 1)))
        (compile bump-counter)
        (bump-counter)
        (bump-counter)
        counter
        "#,
    );
    assert_eq!(v, RtValue::Int(2));
}

/// A global whose declared type can't cross into compiled code
/// (`Option<T>`/`Result<T,E>`/a user `defenum` — `RtValue::Data`,
/// `rtvalue_to_struct_field`'s own documented gap) surfaces as a clean
/// compile-time error rather than a panic deep inside the compiler body —
/// see `Interp::promote_global`'s doc comment.
#[test]
fn compile_of_a_function_referencing_an_option_typed_global_is_a_clean_error() {
    let err = run_with_compiler(
        r#"
        (defvar (maybe Option<i64>) (Option::none))
        (defun read-maybe () Option<i64> maybe)
        (compile read-maybe)
        "#,
    )
    .unwrap_err();
    assert!(matches!(err, EvalError::Panic(_)), "expected a Panic, got {:?}", err);
}

// ---- `Expr::Panic`/`Expr::MethodRef`/`Expr::Quote` in compiled code --------
//
// Closes the three remaining `docs/dev/TODO.md` `unsupported` gaps.

/// A `(panic msg)` branch compiles cleanly (`ast_bridge::translate_panic`/
/// `compiler.rs`'s `compile-panic`), and the *non*-panicking path through the
/// same function still runs correctly — actually triggering the panic would
/// abort the whole test process (`rt_panic`'s documented "abort, don't
/// unwind" contract), so this only exercises the branch never taken.
#[test]
fn compile_dispatches_a_function_with_an_untaken_panic_branch_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun safe-add ((a i64) (b i64)) i64 (if (eq b 0) (panic "b is zero") (+ a b)))
        (compile safe-add)
        (safe-add 10 2)
        "#,
    );
    assert_eq!(v, RtValue::Int(12));
}

/// A bare `char` literal now has a real compiled representation
/// (`compile-char`/`sexpr-char`) — an oversight discovered while
/// implementing `Expr::Quote`'s `Char` leaf (a real `Sexpr::Char`
/// construction, e.g. `(Char c)`, already routed through `compile-value` on
/// its own literal-`char` argument form, which previously had no dispatch
/// tag at all). This keeps the literal entirely inside the compiled
/// function, comparing it via the native `char` `equal` (`compile-assoc`'s
/// char-native branch) — only the `Bool` result crosses the boundary. (A
/// `char` *return* crossing the boundary is now supported too — see
/// `compile_returns_a_char_across_the_jit_boundary`.)
#[test]
fn compile_dispatches_a_function_containing_a_bare_char_literal_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun is-a ((c char)) bool (equal c #\A))
        (compile is-a)
        (if (is-a #\A) 1 (if (is-a #\B) 2 0))
        "#,
    );
    assert_eq!(v, RtValue::Int(1));
}

/// A `char`-returning compiled function's result crosses the JIT-call
/// boundary as a real `RtValue::Char`, not a bare `RtValue::Int` of its code
/// point (`Interp::call_compiled`'s `Type::Char` return decode, the inverse
/// of the `*c as i64` a `char` argument crosses as). The function passes a
/// `char` straight through — the raw `i64` code point that arrives as an
/// argument is the same one handed back — so the decode is exercised on its
/// own, independent of any in-function `char` construction.
#[test]
fn compile_returns_a_char_across_the_jit_boundary() {
    let v = eval_ok_with_compiler(
        r#"
        (defun echo-char ((c char)) char c)
        (compile echo-char)
        (echo-char #\Z)
        "#,
    );
    assert_eq!(v, RtValue::Char('Z'));
}

/// The decode also runs on a `char` a compiled function *selects* rather than
/// receives — here a bare `char` literal returned from one branch of an
/// `if` — confirming the boundary decode doesn't depend on the value having
/// arrived as an argument.
#[test]
fn compile_returns_a_selected_char_literal_across_the_jit_boundary() {
    let v = eval_ok_with_compiler(
        r#"
        (defun grade ((pass bool)) char (if pass #\P #\F))
        (compile grade)
        (grade true)
        "#,
    );
    assert_eq!(v, RtValue::Char('P'));
}

/// `+` reified as a value (`Expr::MethodRef`, `Checker::method_value`) and
/// passed through a `Fn`-typed parameter — `translate_methodref`'s
/// forwarding-`lambda`-to-`(assoc ...)` wrapper reaches the exact same
/// `compile-assoc` native-arithmetic dispatch an ordinary `(+ a b)` call site
/// already goes through. Both the receiver function (`apply2`) and the
/// caller that reifies `+` (`use-plus`) are compiled.
#[test]
fn compile_dispatches_a_builtin_operator_reified_as_a_value_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun apply2 ((f (fn (i64 i64) i64)) (a i64) (b i64)) i64 (f a b))
        (compile apply2)
        (defun use-plus ((a i64) (b i64)) i64 (apply2 + a b))
        (compile use-plus)
        (use-plus 3 4)
        "#,
    );
    assert_eq!(v, RtValue::Int(7));
}

/// A user-defined instance method (not a native-arithmetic builtin) reified
/// as a value dispatches through `compile-assoc-user` instead — the other
/// half of `compile-assoc`'s two-way split, reached the same way an ordinary
/// `p::double` call site would.
#[test]
fn compile_dispatches_a_user_defined_method_reified_as_a_value_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defstruct point (x i64))
        (defmethod double ((self point)) i64 (* self::x 2))
        (compile point::x)
        (compile point::double)
        (defun apply1 ((f (fn (point) i64)) (p point)) i64 (f p))
        (compile apply1)
        (defun use-double ((p point)) i64 (apply1 double p))
        (compile use-double)
        (use-double (point::new 21))
        "#,
    );
    assert_eq!(v, RtValue::Int(42));
}

/// `(quote (1 2 3))` compiles to the same `compile-construct-sexpr`
/// machinery an ordinary `(Cons (Int 1) ...)` construction already uses
/// (`ast_bridge::translate_quote`) — a compiled function can build a quoted
/// literal, and an ordinary compiled `match`-based `sum` (already proven
/// correct against `Sexpr` scrutinees elsewhere in this file) can consume it.
#[test]
fn compile_dispatches_a_function_that_constructs_a_quoted_list_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-quoted () Sexpr (quote (1 2 3)))
        (defun sum ((s Sexpr)) i64 (match s ((cons (int n) rest) (+ n (sum rest))) (_ 0)))
        (compile make-quoted)
        (compile sum)
        (sum (make-quoted))
        "#,
    );
    assert_eq!(v, RtValue::Int(6));
}

/// Compiled and interpreted agree on the same quoted literal — the
/// JIT/interp-parity check every other construct in this file gets.
#[test]
fn compile_and_interpret_agree_on_a_quoted_list() {
    let prog = r#"
        (defun make-quoted () Sexpr (quote (1 2 3)))
        (defun sum ((s Sexpr)) i64 (match s ((cons (int n) rest) (+ n (sum rest))) (_ 0)))
    "#;
    let compiled = eval_ok_with_compiler(&format!("{}\n(compile make-quoted)\n(compile sum)\n(sum (make-quoted))", prog));
    let interpreted = eval_ok(&format!("{}\n(sum (make-quoted))", prog));
    assert_eq!(compiled, interpreted);
    assert_eq!(compiled, RtValue::Int(6));
}

/// A quoted symbol has no compiled representation (a `Sym`'s payload is a
/// `SymId`, same gap `compile-sexpr-field`'s own doc comment already
/// documents) — `translate_quote` bails out to a clean bridge-level
/// `unsupported`, even though the *surrounding* function is otherwise
/// perfectly compilable, rather than reaching a confusing low-level panic
/// deep inside the interpreted compiler body.
#[test]
fn compile_of_a_function_quoting_a_symbol_is_a_clean_error() {
    let err = run_with_compiler(
        r#"
        (defun q () Sexpr (quote foo))
        (compile q)
        "#,
    )
    .unwrap_err();
    assert!(matches!(err, EvalError::Panic(_)), "expected a Panic, got {:?}", err);
}

// ---- Stage A: `Vector<T>` builtin method compile (Iter-compile plan) --------
//
// `push`/`get`/`set`/`len` on a `Vector<T>` have no compiled `defmethod`
// body; `ast_bridge::translate_vector_method` lowers them to a `vector-op`
// node that `compiler.rs`'s `compile-vector-op` turns into
// `rt_struct_push_field`/`rt_struct_field_get`/`rt_struct_field_set`/
// `rt_struct_field_count`, tagging/untagging each element by its
// `struct_field_kind` (`compile-tag-struct-field`/`compile-sexpr-field`) —
// the same boxed-struct-boundary crossing `field-get`/`field-set` do, but
// with a *runtime* index. These are the primitive layer every `Iter`
// combinator over a `Vector` bottoms out in.

/// `push` (grow) then `get` (read back) an `i64` element — kind `1`, so the
/// element is `shl 3`-tagged on the way in and `ashr 3`-untagged on the way
/// out.
#[test]
fn compile_dispatches_vector_push_and_get_of_an_i64_element_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i64
          (let ((v (the Vector<i64> (Vector::new))))
            (push v 10)
            (push v 20)
            (push v 30)
            (get v 1)))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(20), "the element pushed at index 1");
}

/// `set` overwrites an element in place (`rt_struct_field_set` with a
/// tagged element), observable through a later `get`.
#[test]
fn compile_dispatches_vector_set_in_place_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i64
          (let ((v (the Vector<i64> (Vector::new))))
            (push v 10)
            (push v 20)
            (set v 0 99)
            (get v 0)))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(99), "index 0 was overwritten from 10 to 99");
}

/// `len` (`rt_struct_field_count`, one of the two new primitives) returns the
/// element count as a raw `i64` — `push` grew the vector from 0 to 3.
#[test]
fn compile_dispatches_vector_len_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i64
          (let ((v (the Vector<i64> (Vector::new))))
            (push v 10)
            (push v 20)
            (push v 30)
            (as i64 (len v))))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(3), "three pushes -> length 3");
}

/// A passthrough (kind `6`) element type: a `Vector<string>` stores each
/// element as an already-tagged `Sexpr`, so `push`/`get` neither tag nor
/// untag (`compile-tag-struct-field`/`compile-sexpr-field`'s kind-`6`
/// identity arm) — `get` hands back a real `Str` the compiled `length`
/// (`rt_str_length`) can measure.
#[test]
fn compile_dispatches_vector_get_of_a_passthrough_string_element_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i64
          (let ((v (the Vector<string> (Vector::new))))
            (push v "hi")
            (push v "world")
            (as i64 (length (get v 1)))))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, RtValue::Int(5), "\"world\" has 5 characters");
}

/// The JIT result of a function summing a `Vector<i64>` by index must match
/// the interpreter's result for the same source — the end-to-end agreement
/// check every other compile test pairs with its representation proof.
#[test]
fn compile_of_a_vector_summing_function_agrees_with_the_interpreter() {
    let src = r#"
        (defun build-and-sum () i64
          (let ((v (the Vector<i64> (Vector::new))))
            (push v 3)
            (push v 4)
            (push v 5)
            (+ (+ (get v 0) (get v 1)) (get v 2))))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(build-and-sum)"))
        .expect("interpreted eval failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile build-and-sum)\n(build-and-sum)"))
        .expect("compiled eval failed");
    assert_eq!(interpreted, RtValue::Int(12), "3 + 4 + 5 = 12, interpreted");
    assert_eq!(compiled, interpreted, "JIT result matches the interpreter");
}

// ---- Stage B: `Iter` combinators over `Vector<T>` compile end-to-end -------
//
// `(compile fn)` now transitively compiles the monomorphized method/function
// instantiations `fn` calls (`Interp::compile_function_rec`), so a caller that
// iterates a `Vector` — `doiter`, or a prelude combinator like `map`/`member`
// — pulls in `vector::iter <T>` / `vector-iter::next <T>` (and any `Eq`
// instance the combinator needs) automatically, even though those
// whitespace-mangled names can't be `(compile ...)`d by hand. This is the
// real proof that "an `Iter` over a `Vector` compiles": closures + `Option`
// construct/match + user-method dispatch + the Stage A `vector-op` primitives
// all composed together.

/// The minimal `Iter` end-to-end case: a `doiter` loop (expands to
/// `while-let` + `next` on a `vector-iter<i64>`) summing a `Vector<i64>`,
/// compiled and run natively — its result must match the interpreter's.
#[test]
fn compile_transitively_compiles_a_doiter_loop_over_a_vector() {
    let src = r#"
        (defun sum-vec ((v Vector<i64>)) i64
          (let ((total (the i64 0)))
            (doiter (x (iter v))
              (setf total (+ total x)))
            total))
        (defun run () i64
          (let ((v (the Vector<i64> (Vector::new))))
            (push v 3) (push v 4) (push v 5)
            (sum-vec v)))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(run)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile sum-vec)\n(compile run)\n(run)")).expect("compiled failed");
    assert_eq!(interpreted, RtValue::Int(12), "3 + 4 + 5 = 12 (interpreted)");
    assert_eq!(compiled, interpreted, "the transitively-compiled doiter loop agrees with the interpreter");
}

/// The `member` combinator (`where (Iter I (Item A)) (Eq A)`) over a
/// `Vector<i32>`: transitively compiling it drags in `vector-iter::next`
/// *and* the element type's `Eq` instance (`i32::equals`). Returns the
/// membership result via an `if` so the JIT boundary sees a plain `i64`.
#[test]
fn compile_transitively_compiles_the_member_combinator_over_a_vector() {
    let src = r#"
        (defun has-it ((v Vector<i32>) (needle i32)) i64
          (if (member needle (iter v)) 1 0))
        (defun run () i64
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 10) (push v 20) (push v 30)
            (+ (* (the i64 10) (has-it v 20)) (has-it v 99))))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(run)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile has-it)\n(compile run)\n(run)")).expect("compiled failed");
    assert_eq!(interpreted, RtValue::Int(10), "20 is a member (10), 99 is not (0) -> 10 (interpreted)");
    assert_eq!(compiled, interpreted, "the transitively-compiled member combinator agrees with the interpreter");
}

/// The `map` combinator (builds a fresh `Vector<U>`, taking a closure `f`):
/// transitively compiling `double-all` drags in `vector-iter::next` and
/// exercises `map`'s own `push`/`Vector::new` (`vector-op`) plus a compiled
/// closure argument. The result vector is read back by index.
#[test]
fn compile_transitively_compiles_the_map_combinator_over_a_vector() {
    let src = r#"
        (defun double-all ((v Vector<i64>)) i64
          (let ((out (map (iter v) (lambda ((x i64)) i64 (* x 2)))))
            (+ (get out 0) (get out 2))))
        (defun run () i64
          (let ((v (the Vector<i64> (Vector::new))))
            (push v 1) (push v 2) (push v 3)
            (double-all v)))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(run)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile double-all)\n(compile run)\n(run)")).expect("compiled failed");
    assert_eq!(interpreted, RtValue::Int(8), "doubled [2,4,6], out[0]+out[2] = 2+6 = 8 (interpreted)");
    assert_eq!(compiled, interpreted, "the transitively-compiled map combinator agrees with the interpreter");
}

// ---- Stage D: `Iter` over a `HashTable<K,V>` compiles end-to-end -----------
//
// `HashTable<K,V>` iteration bottoms out in Stage A's `Vector` primitives:
// `hashtable-iter<K,V>` walks a `Vector<cons-cell<K,V>>` *snapshot* that
// `HashTable::iter` builds via `entries`. With Stage C's `rt_hashtable_*`
// primitives (`new`/`set`/`entries`/...) and Stage B's transitive driver,
// `(compile fn)` for a function that builds, populates, and iterates a map
// compiles the whole chain (`hashtable::iter`, `hashtable-iter::next`,
// `cons-cell` accessors) automatically.

/// Build + populate + iterate a `HashTable<i64,i64>`, summing its values —
/// the full HashTable-iteration chain, compiled and agreeing with the
/// interpreter. Exercises `new`/`set`/`entries` (`hashtable-op`), the
/// transitively-compiled `iter`/`next` methods, and `cons-cell`'s `cdr`.
#[test]
fn compile_transitively_compiles_iteration_over_a_hashtable() {
    let src = r#"
        (defun sum-values ((ht HashTable<i64,i64>)) i64
          (let ((total (the i64 0)))
            (doiter (e (iter ht))
              (setf total (+ total (cdr e))))
            total))
        (defun run () i64
          (let ((ht (the HashTable<i64,i64> (HashTable::new))))
            (set ht 1 10)
            (set ht 2 20)
            (set ht 3 30)
            (sum-values ht)))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(run)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile sum-values)\n(compile run)\n(run)")).expect("compiled failed");
    assert_eq!(interpreted, RtValue::Int(60), "10 + 20 + 30 = 60 (interpreted)");
    assert_eq!(compiled, interpreted, "the transitively-compiled HashTable iteration agrees with the interpreter");
}

/// `count` (`rt_hashtable_count`) and `keys` (a fresh `Vector<K>` walked by
/// `len`) over a compiled `HashTable`, confirming the non-`entries`
/// enumerators lower correctly too.
#[test]
fn compile_dispatches_hashtable_count_and_keys_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i64
          (let ((ht (the HashTable<i64,i64> (HashTable::new))))
            (set ht 1 10)
            (set ht 2 20)
            (+ (* (the i64 100) (as i64 (count ht))) (as i64 (len (keys ht))))))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, RtValue::Int(202), "count 2 (*100) + keys length 2 = 202");
}

// ---- HashTable::get/remove compile (Option-returning) ----------------------
//
// Unlike an ordinary `Option::some`/`none` source call (a compile-time-known
// variant, `compile-construct-box`), `HashTable<K,V>::get`/`remove` decide
// which variant to build from a *runtime* lookup outcome — real control flow
// (`rt_hashtable_contains` + a branch), not a static dispatch. See
// `compile-hashtable-op`'s doc comment in `compiler.rs`.

/// `get` on a present key returns `Some(v)`; unwrapped via `unwrap-or` to a
/// plain `i64` result so the JIT boundary sees a scalar, not a raw `Option`
/// box pointer (`Interp::call_compiled`'s own decode is exercised by
/// `compile_dispatches_a_function_that_constructs_a_general_adt_box_to_native_code`
/// already; this test focuses on the runtime found/not-found branch itself).
#[test]
fn compile_dispatches_hashtable_get_of_a_present_key_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i64
          (let ((ht (the HashTable<i64,i64> (HashTable::new))))
            (set ht 1 100)
            (unwrap-or (get ht 1) 0)))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, RtValue::Int(100));
}

/// `get` on an absent key returns `None`, taking the runtime "not found"
/// branch (`rt_hashtable_contains` returns `0`) — the `else` side of the
/// `alloca`+branch+merge machinery `compile-hashtable-op`'s get/remove
/// handling builds (mirroring `compile-if`'s own shape, no `phi` builtin).
#[test]
fn compile_dispatches_hashtable_get_of_an_absent_key_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i64
          (let ((ht (the HashTable<i64,i64> (HashTable::new))))
            (set ht 1 100)
            (unwrap-or (get ht (the i64 99)) -1)))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, RtValue::Int(-1), "key 99 was never set, so `get` returns None -> the -1 default");
}

/// `remove` both returns the removed value (`Some(v)`) *and* deletes the
/// entry — a subsequent `get` for the same key must then miss.
#[test]
fn compile_dispatches_hashtable_remove_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i64
          (let ((ht (the HashTable<i64,i64> (HashTable::new))))
            (set ht 1 100)
            (let ((removed (unwrap-or (remove ht 1) 0)))
              (+ (* (the i64 1000) removed) (unwrap-or (get ht 1) 0)))))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, RtValue::Int(100000), "removed value 100 (*1000) + 0 (gone after remove) = 100000");
}

/// `get`/`remove` over a `HashTable<i64,string>` — a passthrough (kind `6`)
/// value type — exercises the `push-permanent-sexpr-root`ed branch of
/// `compile-hashtable-op`'s decode (a heap-referencing `Str` surviving
/// inside the never-GC-scanned `Option` box).
#[test]
fn compile_dispatches_hashtable_get_of_a_passthrough_string_value_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i64
          (let ((ht (the HashTable<i64,string> (HashTable::new))))
            (set ht 1 "hello")
            (as i64 (length (unwrap-or (get ht 1) "")))))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, RtValue::Int(5), "\"hello\" has 5 characters");
}

// ---- `equalp` (ASCII case-insensitive) on char/string, compiled ------------
//
// Unlike `eq`/`equal` (a compiled `char`'s raw code-point `icmp`, a compiled
// string's `rt_str_eq` content compare), `equalp` folds case and so has no
// in-place lowering — `compile-assoc`'s char/string branches call the
// dedicated `rt_char_equalp`/`rt_str_equalp` runtime helpers.

/// `char::equalp` — `#\A` and `#\a` are `equalp` but not `equal`, over
/// compiled code. A `bool` result crosses the JIT boundary.
#[test]
fn compile_dispatches_char_equalp() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun same-letter ((a char) (b char)) i32
          (if (equalp a b) (if (equal a b) 2 1) 0))
        (compile same-letter)
        (+ (* 100 (same-letter #\A #\a))
           (+ (* 10 (same-letter #\A #\A))
              (same-letter #\A #\b)))
        "#,
    )
    .expect("eval failed");
    // A vs a: equalp but not equal -> 1 (*100); A vs A: equal -> 2 (*10); A vs b: neither -> 0
    assert_eq!(v, RtValue::Int(120));
}

/// `string::equalp` — `"ABC"`/`"abc"` are `equalp` but not `equal`, over
/// compiled code (`rt_str_equalp`).
#[test]
fn compile_dispatches_string_equalp() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun same-word ((a string) (b string)) i32
          (if (equalp a b) (if (equal a b) 2 1) 0))
        (compile same-word)
        (+ (* 100 (same-word "ABC" "abc"))
           (+ (* 10 (same-word "abc" "abc"))
              (same-word "abc" "abd")))
        "#,
    )
    .expect("eval failed");
    // ABC vs abc: equalp not equal -> 1 (*100); abc vs abc: equal -> 2 (*10); abc vs abd: neither -> 0
    assert_eq!(v, RtValue::Int(120));
}

// ---- `f64` arithmetic/comparison methods, compiled -------------------------
//
// A compiled `f64` is its raw `f64::to_bits` pattern carried in an `i64`
// register (`compile-float`); `compile-assoc`'s f64 branch lowers arithmetic
// to `build-fadd`/... (each bitcasts to `double` and back), comparisons to
// `build-fcmp-*`, the transcendental/rounding family (`sqrt`/`floor`/
// `ceiling`/`round`/`truncate`/`expt`) to the matching LLVM intrinsic
// (`build-fsqrt`/.../`build-fpow`), and `float->int` to a single `fptosi`
// instruction (`build-fptosi`). The JIT boundary encodes an `f64` argument as
// its bits and decodes an `f64` result back (`Interp::call_compiled`).
// `float->bignum`/`float->ratio` alone stay out of scope (`bignum`/`ratio`
// have no compiled representation at all yet), falling through to
// `compile-assoc-user`.

/// Float arithmetic returning an `f64` across the JIT boundary — exercises
/// `build-fadd`/`build-fsub`/`build-fmul`/`build-fdiv` and the `f64`
/// argument/return marshaling.
#[test]
fn compile_dispatches_f64_arithmetic() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun combine ((a f64) (b f64)) f64
          (/ (* (+ a b) (- a b)) 2.0))
        (compile combine)
        (combine 5.0 3.0)
        "#,
    )
    .expect("eval failed");
    // (5+3)*(5-3)/2 = 8*2/2 = 8.0
    match v {
        RtValue::Float(f) => assert!((f - 8.0).abs() < 1e-9, "expected 8.0, got {}", f),
        other => panic!("expected an f64, got {:?}", other),
    }
}

/// `mod` on `f64` lowers to `build-frem` (`a % b`, matching the interpreter's
/// `eval_float_builtin`). The function is deliberately *not* named `fmod`:
/// LLVM lowers an `frem` instruction to a call to the C `fmod` symbol, and
/// the JIT's symbol resolver would bind that call to a same-named compiled
/// typelisp function instead of libm — an infinite `frem`->`fmod`->`frem`
/// recursion (a real footgun for a user who compiles a function literally
/// named `fmod`, but an unusual name to pick).
#[test]
fn compile_dispatches_f64_mod() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun float-rem ((a f64) (b f64)) f64 (mod a b))
        (compile float-rem)
        (float-rem 7.5 2.0)
        "#,
    )
    .expect("eval failed");
    match v {
        RtValue::Float(f) => assert!((f - 1.5).abs() < 1e-9, "7.5 mod 2.0 = 1.5, got {}", f),
        other => panic!("expected an f64, got {:?}", other),
    }
}

/// Float comparisons lower to `build-fcmp-*` (ordered `<`/`<=`/`>`/`>=`/`=`,
/// unordered `/=`) — a `bool` result crosses the boundary. Agrees with the
/// interpreter for the same source.
#[test]
fn compile_dispatches_f64_comparisons_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun classify ((a f64) (b f64)) i32
          (if (< a b) 1
          (if (= a b) 2
          (if (> a b) 3 4))))
    "#;
    let interpreted =
        run_with_compiler_and_prelude(&format!("{src}\n(+ (* 100 (classify 1.0 2.0)) (+ (* 10 (classify 2.0 2.0)) (classify 3.0 2.0)))"))
            .expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!(
        "{src}\n(compile classify)\n(+ (* 100 (classify 1.0 2.0)) (+ (* 10 (classify 2.0 2.0)) (classify 3.0 2.0)))"
    ))
    .expect("compiled failed");
    assert_eq!(interpreted, RtValue::Int(123), "1<2 ->1, 2=2 ->2, 3>2 ->3 (interpreted)");
    assert_eq!(compiled, interpreted, "compiled f64 comparisons agree with the interpreter");
}

/// The unary transcendental/rounding family (`sqrt`/`floor`/`ceiling`/
/// `round`/`truncate`) each lower to their own LLVM intrinsic
/// (`build-fsqrt`/...) — this exercises all five in one compiled function
/// and checks the JIT result against the interpreter's.
#[test]
fn compile_dispatches_f64_transcendentals_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun combine ((x f64)) f64
          (+ (sqrt x)
             (+ (floor x)
                (+ (ceiling x)
                   (+ (round x) (truncate x))))))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(combine 6.25)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile combine)\n(combine 6.25)")).expect("compiled failed");
    match (interpreted, compiled) {
        (RtValue::Float(i), RtValue::Float(c)) => assert!((i - c).abs() < 1e-9, "interpreted {} vs compiled {}", i, c),
        other => panic!("expected two f64s, got {:?}", other),
    }
}

/// `expt` (binary, `f64,f64->f64`) lowers to `build-fpow` (`llvm.pow.f64`).
#[test]
fn compile_dispatches_f64_expt_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun power ((base f64) (exp f64)) f64 (expt base exp))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(power 2.0 10.0)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile power)\n(power 2.0 10.0)")).expect("compiled failed");
    assert_eq!(interpreted, RtValue::Float(1024.0));
    assert_eq!(compiled, interpreted, "compiled expt agrees with the interpreter");
}

/// `float->int` lowers to a single `fptosi` instruction (`build-fptosi`), no
/// heap allocation involved.
#[test]
fn compile_dispatches_float_to_int_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun truncate-it ((x f64)) i32 (float->int x))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(truncate-it 7.9)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile truncate-it)\n(truncate-it 7.9)")).expect("compiled failed");
    assert_eq!(interpreted, RtValue::Int(7));
    assert_eq!(compiled, interpreted, "compiled float->int agrees with the interpreter");
}

// ---- bignum/ratio compiled representation -------------------------------
//
// `bignum`/`ratio` are always a tagged `TAG_BOXED` pointer (no native
// register form the way `f64` has — see `crates/typelisp-rt/src/lib.rs`'s
// "bignum/ratio compiled representation" section), so a `bignum`/`ratio`
// literal, arithmetic/comparison, and every conversion method now has a real
// compiled form (`rt_bignum_*`/`rt_ratio_*`). Each test mirrors an existing
// interpreter-only case from `tests/bignum_ratio_test.rs`, comparing the
// compiled result against the interpreted one.

/// A `bignum` literal too large for `i64` — `compile-bignum-literal`'s
/// `rt_bignum_new` construction from the literal's embedded sign+digits.
#[test]
fn compile_constructs_a_bignum_literal_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun big () bignum 123456789012345678901234567890)
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(big)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile big)\n(big)")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled bignum literal agrees with the interpreter");
}

/// `bignum` arithmetic (`+`/`-`/`*`/`/`/`mod`), each a single `rt_bignum_*`
/// call.
#[test]
fn compile_dispatches_bignum_arithmetic_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun combine ((a bignum) (b bignum)) bignum
          (+ (* a b) (mod (- a b) b)))
    "#;
    let call = "(combine 123456789012345678901234567890 98765432109876543210)";
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n{call}")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile combine)\n{call}")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled bignum arithmetic agrees with the interpreter");
}

/// `bignum` division by zero is a fatal, explicitly-checked runtime error
/// (`rt_bignum_div`'s zero-divisor guard, `fatal`) rather than an unwinding
/// Rust `panic!` crossing the `extern "C"` boundary — this only exercises
/// the ordinary non-zero path (a deliberate process abort isn't something
/// this test suite's harness verifies, matching how `rt_hashtable_get_raw`'s
/// own "caller already checked" contract isn't abort-tested either).
#[test]
fn compile_dispatches_bignum_division_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun quotient ((a bignum) (b bignum)) bignum (/ a b))
    "#;
    let call = "(quotient 100000000000000000000 (int->bignum 3))";
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n{call}")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile quotient)\n{call}")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled bignum division agrees with the interpreter");
}

/// The six `bignum` comparisons (`<`/`<=`/`>`/`>=`/`=`/`/=`) all derive from
/// `rt_bignum_cmp`'s three-way result — this exercises each direction.
#[test]
fn compile_dispatches_bignum_comparisons_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun compare ((a bignum) (b bignum)) i64
          (if (< a b) 1
          (if (<= a b) 2
          (if (> a b) 3
          (if (>= a b) 4
          (if (= a b) 5
          (if (/= a b) 6 0)))))))
    "#;
    for call in [
        "(compare 100000000000000000000 200000000000000000000)",
        "(compare 200000000000000000000 100000000000000000000)",
        "(compare 100000000000000000000 100000000000000000000)",
    ] {
        let interpreted = run_with_compiler_and_prelude(&format!("{src}\n{call}")).expect("interpreted failed");
        let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile compare)\n{call}")).expect("compiled failed");
        assert_eq!(compiled, interpreted, "compiled bignum comparison ({call}) agrees with the interpreter");
    }
}

/// `int->bignum`/`bignum->int`/`bignum->float`/`float->bignum`/
/// `bignum->ratio` each lower to a single `rt_bignum_*`/`rt_int_to_bignum`/
/// `rt_float_to_bignum` call.
#[test]
fn compile_dispatches_bignum_conversions_and_agrees_with_the_interpreter() {
    let cases = [
        ("(defun f ((a i32)) bignum (int->bignum a))", "(f 42)"),
        ("(defun f ((a bignum)) i32 (bignum->int a))", "(f (int->bignum 42))"),
        ("(defun f ((a bignum)) f64 (bignum->float a))", "(f (int->bignum 2))"),
        ("(defun f ((a f64)) bignum (float->bignum a))", "(f 2.0)"),
        ("(defun f ((a bignum)) ratio (bignum->ratio a))", "(f (int->bignum 5))"),
    ];
    for (def, call) in cases {
        let interpreted = run_with_compiler_and_prelude(&format!("{def}\n{call}")).expect("interpreted failed");
        let compiled = run_with_compiler_and_prelude(&format!("{def}\n(compile f)\n{call}")).expect("compiled failed");
        assert_eq!(compiled, interpreted, "compiled `{def}` agrees with the interpreter");
    }
}

/// `try-bignum->int` (`(try-as i32 n)`) — the `Option<i32>`-returning
/// counterpart of `bignum->int`, compiled as a two-call
/// `rt_bignum_fits_i32`/`rt_bignum_to_int_raw` sequence (the same
/// alloca+branch+merge shape `compile-hashtable-op`'s `get`/`remove` already
/// use) building a real `Some`/`None` box — exercises both outcomes.
#[test]
fn compile_dispatches_try_bignum_to_int_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun try-it ((n bignum)) bool (is-some (try-as i32 n)))
    "#;
    for call in ["(try-it (int->bignum 42))", "(try-it 123456789012345678901234567890)"] {
        let interpreted = run_with_compiler_and_prelude(&format!("{src}\n{call}")).expect("interpreted failed");
        let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile try-it)\n{call}")).expect("compiled failed");
        assert_eq!(compiled, interpreted, "compiled try-bignum->int ({call}) agrees with the interpreter");
    }
}

/// A `ratio` literal — `compile-ratio-literal`'s two nested
/// `compile-bignum-literal` calls + `rt_ratio_from_bignums`.
#[test]
fn compile_constructs_a_ratio_literal_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun frac () ratio 4/6)
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(frac)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile frac)\n(frac)")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled ratio literal agrees with the interpreter (reduced to 2/3)");
}

/// `ratio` arithmetic (`+`/`-`/`*`/`/`), each a single `rt_ratio_*` call.
#[test]
fn compile_dispatches_ratio_arithmetic_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun combine ((a ratio) (b ratio)) ratio
          (/ (+ a b) (* a b)))
    "#;
    let call = "(combine 1/2 2/3)";
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n{call}")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile combine)\n{call}")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled ratio arithmetic agrees with the interpreter");
}

/// The six `ratio` comparisons all derive from `rt_ratio_cmp`'s three-way
/// result, the same shape as the bignum branch.
#[test]
fn compile_dispatches_ratio_comparisons_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun compare ((a ratio) (b ratio)) i64
          (if (< a b) 1
          (if (<= a b) 2
          (if (> a b) 3
          (if (>= a b) 4
          (if (= a b) 5
          (if (/= a b) 6 0)))))))
    "#;
    for call in ["(compare 1/2 2/3)", "(compare 2/3 1/2)", "(compare 1/2 2/4)"] {
        let interpreted = run_with_compiler_and_prelude(&format!("{src}\n{call}")).expect("interpreted failed");
        let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile compare)\n{call}")).expect("compiled failed");
        assert_eq!(compiled, interpreted, "compiled ratio comparison ({call}) agrees with the interpreter");
    }
}

/// `int->ratio`/`ratio->bignum`/`ratio->float`/`float->ratio`/`numerator`/
/// `denominator`, each a single `rt_ratio_*`/`rt_int_to_ratio` call.
#[test]
fn compile_dispatches_ratio_conversions_and_agrees_with_the_interpreter() {
    let cases = [
        ("(defun f ((a i32)) ratio (int->ratio a))", "(f 7)"),
        ("(defun f ((r ratio)) bignum (ratio->bignum r))", "(f 7/2)"),
        ("(defun f ((r ratio)) f64 (ratio->float r))", "(f 1/2)"),
        ("(defun f ((a f64)) ratio (float->ratio a))", "(f 0.5)"),
        ("(defun f ((r ratio)) bignum (numerator r))", "(f 4/6)"),
        ("(defun f ((r ratio)) bignum (denominator r))", "(f 4/6)"),
    ];
    for (def, call) in cases {
        let interpreted = run_with_compiler_and_prelude(&format!("{def}\n{call}")).expect("interpreted failed");
        let compiled = run_with_compiler_and_prelude(&format!("{def}\n(compile f)\n{call}")).expect("compiled failed");
        assert_eq!(compiled, interpreted, "compiled `{def}` agrees with the interpreter");
    }
}
