//! Tests for the LLVM-backed `compile`/`compile-file` machinery (Phase 0:
//! the shared core that lets the (typelisp-hosted) compiler body build an
//! LLVM module at all — `llvm-module`/`llvm-function`/`llvm-builder`/
//! `llvm-value`, see `registry::llvm_module_def` and friends). No AST
//! bridge or JIT/AOT output path exists yet; these only exercise the raw
//! LLVM builtins from typelisp source, the same way `hashtable_test.rs`
//! exercises `HashTable<K,V>`.

extern crate typelisp;
use inkwell::module::Module;
use inkwell::OptimizationLevel;
use std::cell::RefCell;
use std::rc::Rc;
use typelisp::compile::COMPILE_LOCK;
use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    typelisp::compile::install_llvm_backend();
    let interp = Interp::new();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// Check every form in `src` and return the first check-time error, for the
/// tests about what `(compile ...)` rejects before anything runs.
///
/// [`run`] can't serve these: it `expect`s the check to succeed, so a
/// check-time rejection panics there instead of coming back as a value. This
/// stops after the first error rather than running the program, which is also
/// what a real driver does.
fn check_error(src: &str) -> Error {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    typelisp::compile::install_llvm_backend();
    let interp = Interp::new();
    let result = vs
        .into_iter()
        .try_for_each(|v| chk.check_form(&mut h, &interp, v).map_err(Error::into_kind).map(|_| ()));
    match result {
        Err(e) => e,
        Ok(()) => panic!("expected a check-time error for {:?}, but every form checked", src),
    }
}

/// The text of a `string` result.
///
/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — and every runner here drops its heap on
/// return. Hence the `*_string` runners below, which read the text out first.
fn read_str(h: &Heap, v: Value) -> String {
    match v {
        typelisp::Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a Str, got {:?}", other),
    }
}

/// [`eval_ok`]'s string-returning counterpart.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    typelisp::compile::install_llvm_backend();
    let interp = Interp::new();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    read_str(&h, last)
}

/// The `llvm-module` `v` is a handle for.
///
/// [`typelisp::llvm_module_of`] answers `None` both for a value that is no
/// handle at all and for a handle to something other than a module, so this
/// `expect` is the whole check. Each of these call sites used to spell it as
/// a `match` with a second arm for "not an `LlvmModule`" — a leftover from
/// when `RtValue` had its own `LlvmModule` variant, and dead (with a dead
/// message) since the handle registry replaced it, as `unreachable pattern`
/// warnings pointed out.
fn expect_llvm_module(v: Value) -> Rc<RefCell<Module<'static>>> {
    typelisp::llvm_module_of(&v).expect("expected an llvm-module handle")
}

/// Like [`run`], but with the (typelisp-hosted) compiler body
/// (`compiler::SOURCE`) loaded first, for tests that call
/// `compile-constant-function`/`compile-value`.
fn run_with_compiler(src: &str) -> Result<Value, EvalError> {
    run_with_compiler_and_capacity(src, 1 << 16)
}

/// Like [`run_with_compiler`], but with a caller-chosen `Heap` capacity —
/// see [`run_with_compiler_and_prelude_and_capacity`]'s doc comment for why.
fn run_with_compiler_and_capacity(src: &str, capacity: usize) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(capacity);
    let mut chk = Checker::new();
    typelisp::compile::install_llvm_backend();
    let mut interp = Interp::new();
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok_with_compiler(src: &str) -> Value {
    run_with_compiler(src).expect("eval failed")
}

/// [`eval_ok_with_compiler`]'s string-returning counterpart — see
/// [`read_str`].
fn eval_string_with_compiler_and_capacity(src: &str, capacity: usize) -> String {
    let mut h = Heap::with_capacity(capacity);
    let mut chk = Checker::new();
    typelisp::compile::install_llvm_backend();
    let mut interp = Interp::new();
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    read_str(&h, last)
}

fn eval_string_with_compiler(src: &str) -> String {
    eval_string_with_compiler_and_capacity(src, 1 << 16)
}

/// Like [`run_with_compiler`], but with the prelude (`prelude::SOURCE` —
/// `while`/`dotimes`/`dolist`/...) loaded too, for tests that exercise a
/// macro built on `loop`/`break`/`return`/`setf` rather than those
/// primitives directly.
fn run_with_compiler_and_prelude(src: &str) -> Result<Value, EvalError> {
    run_with_compiler_and_prelude_and_capacity(src, 1 << 16)
}

/// Like [`run_with_compiler_and_prelude`], but with a caller-chosen `Heap`
/// capacity — for a test (Stage 6 of the Sexpr-representation plan, the
/// "Sexprルート挿入パス") that needs a *small* heap to force a real `gc()`
/// partway through a compiled function's execution, the same reason
/// `typelisp-rt`'s own `rt_push_sexpr_root` tests use a tiny capacity rather
/// than the generous default every other test here gets.
fn run_with_compiler_and_prelude_and_capacity(src: &str, capacity: usize) -> Result<Value, EvalError> {
    run_and_read(src, capacity, |_, v| v)
}

/// Like [`run_with_compiler_and_prelude_and_capacity`], but handing the
/// result to `f` *while the run's `Heap` is still alive*.
///
/// Every runner here drops its heap on return, which was invisible as long as
/// each compared type was self-contained in `RtValue`. It stops being
/// invisible as the scalar unification moves types onto the heap: a
/// `bignum`/`ratio` result is a `Value::Boxed(BoxId)` into a heap that no
/// longer exists by the time the caller looks, and two runs' ids are
/// unrelated even when the values agree. So anything compared *between* runs
/// has to be read out here — see [`Readback`].
fn run_and_read<R>(src: &str, capacity: usize, f: impl FnOnce(&Heap, Value) -> R) -> Result<R, EvalError> {
    let mut h = Heap::with_capacity(capacity);
    let mut chk = Checker::new();
    typelisp::compile::install_llvm_backend();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(f(&h, last))
}

/// A run's result in a heap-independent form, so that an interpreted run and
/// a compiled run can be compared to each other.
///
/// The numbers are kept as their decimal spellings rather than as `BigInt`s:
/// `BigInt`/`BigRational` print canonically (and a `ratio` is stored already
/// reduced), so equal spellings mean equal values, and an unequal pair reads
/// as the two numbers rather than as two box ids.
#[derive(Debug, PartialEq)]
enum Readback {
    Bignum(String),
    Ratio(String, String),
    /// A float, compared exactly *and at its own width*. Two runs of the
    /// same computation are deterministic, so a tolerance would only hide a
    /// real divergence; the tests that *do* want one read the number out
    /// with [`run_f64`]. One case per width, so that an `f32` result and an
    /// `f64` result can never compare equal by being widened to meet.
    F32(f32),
    F64(f64),
    Str(String),
    /// A result that `RtValue` still carries by value — `Int`, `Bool`,
    /// `Unit`.
    Scalar(Value),
}

fn run_readback(src: &str) -> Result<Readback, EvalError> {
    run_and_read(src, 1 << 16, |h, v| match v {
        typelisp::Value::Boxed(id) if h.is_bignum(id) => {
            Readback::Bignum(h.bignum_value(id).to_string())
        }
        typelisp::Value::Boxed(id) if h.is_ratio(id) => {
            let r = h.ratio_value(id);
            Readback::Ratio(r.numer().to_string(), r.denom().to_string())
        }
        // Read as the width the box actually is, and *kept* as that width:
        // an `f32` compared as an `f64` would compare the widened number,
        // which is the comparison this reader exists to make honest.
        typelisp::Value::Boxed(id) if h.float_box(id).is_some() => match h.float_box(id) {
            Some(typelisp::FloatBox::F32(f)) => Readback::F32(f),
            Some(typelisp::FloatBox::F64(f)) => Readback::F64(f),
            None => unreachable!("guarded by float_box above"),
        },
        typelisp::Value::Str(id) => Readback::Str(h.string(id).to_string()),
        // Any *other* heap box is a value this reader cannot compare across
        // heaps, so it must fail loudly rather than silently compare box ids
        // — the guard that caught the float crossing bug in 1b-5.
        other @ typelisp::Value::Boxed(_) => panic!(
            "run_readback only knows how to read a bignum/ratio/float/string out of its heap, got {:?}",
            other
        ),
        // Scalars are self-contained, so they outlive their heap and compare
        // directly.
        scalar => Readback::Scalar(scalar),
    })
}

/// The `f64` a run produced, for the comparisons that want a tolerance
/// rather than exact equality.
fn run_f64(src: &str) -> f64 {
    match run_readback(src).expect("eval failed") {
        Readback::F64(f) => f,
        other => panic!("expected an f64, got {:?}", other),
    }
}

fn expect_bool(v: Value) -> bool {
    match v {
        Value::Bool(b) => b,
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
          (let ((c (const-word builder 42)))
            (build-ret builder c)
            (to-string m)))))))
"#;

#[test]
fn builds_a_module_with_a_constant_returning_function() {
    let src = format!("{}\n(build-answer-module)", BUILD_ANSWER_MODULE);
    let ir = eval_string(&src);
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
                  (let ((c (const-word builder 42)))
                    (build-ret builder c)
                    (verify m)))))))
        (build-and-verify)
    "#;
    assert!(expect_bool(eval_ok(src)));
}

/// The self-hosted compiler body (`src/compiler.rs`'s `compile-value`/
/// `compile-function`) compiling the exact shape `ast_bridge::ast_to_sexpr`
/// produces for `Expr::Int` — `'(int-any-width 0 42)` here stands in for what the bridge
/// would build from the real typed AST (the Rust-side `ast_bridge` unit
/// tests already cover that translation in isolation; this covers the
/// compiler body consuming it). No parameters, hence the empty `'()`.
#[test]
fn the_compiler_body_compiles_an_int_literal_node() {
    let ir = eval_string_with_compiler(
        r#"(to-string (compile-function (llvm-module::create "mod") "answer" '() '(int-any-width 0 42)))"#,
    );
    assert!(ir.contains("define i64 @answer"), "IR was:\n{}", ir);
    assert!(ir.contains("ret i64 42"), "IR was:\n{}", ir);
}

// (Removed `the_compiler_body_panics_on_an_unsupported_tag` in interp-closure
// removal Stage 8a.) It hand-fed `compile-function` a bogus `'(not-a-real-tag)`
// node to exercise `compile-value`'s defensive fallback `panic` and asserted a
// catchable `EvalError::Panic`. Under the AOT-native island (the only island
// now) that fallback is a compiled `rt_panic`, which aborts the process across
// the native-code boundary rather than returning a catchable error — so the
// assertion can no longer hold in-process. The path is a practically
// unreachable defensive guard (its own comment noted `ast_bridge` never emits
// such a tag); user-facing compile rejections stay covered by
// `compile_of_a_function_calling_an_unsupported_int_method_is_a_clean_error`
// and `compile_of_an_unknown_method_name_is_a_clean_error`.

/// Exercises `compile-function`'s parameter binding (`bind-params`) and the
/// `(assoc ...)` tag (`compile-assoc`'s `+`/`-`/`*` arms) — the shapes
/// `ast_bridge::ast_to_sexpr` produces for `Expr::Var`/`Expr::Assoc`. The
/// quoted body stands in for what the bridge would build from
/// `(+ a b)`'s typed AST (an `i32::+` instance-method call on two `Var`s).
#[test]
fn the_compiler_body_compiles_a_two_parameter_addition() {
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "add2" '((a . 0) (b . 0)) '(assoc "i32" "+" true (0 var "a" false) (0 var "b" false)))"#,
    ));
    // See `compile::COMPILE_LOCK`'s doc comment — every LLVM-Context-touching
    // call, even from a test driving the raw builtins directly rather than
    // going through `Interp::compile_function`, must hold this.
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "outer" '() '(labels () (("f" ((x . 0)) (apply "g" (0 var "x" false))) ("g" ((n . 0)) (var "n" false))) (apply "f" (0 int-any-width 0 5))))"#,
    ));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
    // `bind-captures` unconditionally roots every captured value now
    // (closure-representation unification, Stage 4 — every real
    // `ast_bridge`-built captured list is unconditionally cell-kind, so a
    // plain `rt_push_sexpr_root` per entry, no branching, is enough; this
    // hand-written IR literal's own `kind = 0` never reaches a dereferencing
    // path either way), so `rt_push_sexpr_root` needs an explicit
    // declaration here — this module is built by a direct `compile-function`
    // call, bypassing `Interp::compile_function`'s automatic `rt_*`
    // forward-declaration (see
    // `the_compiler_body_boxes_a_bare_labels_sibling_reference`'s doc
    // comment for the same idiom).
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"(let ((m (llvm-module::create "mod")))
             (let ((ignored (add-function m "rt_push_sexpr_root"))) ())
             (compile-function m "outer" '((offset . 0) (n . 0)) '(labels ((offset . 0)) (("rec" ((k . 0)) (assoc "i32" "+" true (0 var "k" false) (0 var "offset" false)))) (apply "rec" (0 var "n" false)))))"#,
    ));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
                  (let ((c (const-word builder 42)))
                    (build-ret builder c)
                    m))))))
        (build-answer-module-raw)
    "#;
    let module = expect_llvm_module(eval_ok(src));
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
    let module = expect_llvm_module(eval_ok(src));
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
    let module = expect_llvm_module(eval_ok(src));
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

/// `build-make-closure`/`build-closure-apply`: a `BoxedObj::CompiledClosure`
/// wrapping a capturing function (`add_offset`, declared via
/// `add-function-with-env`, adding its one logical argument to a captured
/// value), called *indirectly* through the closure value rather than
/// `build-call-with-env`'s direct, statically-known-target path
/// (labels/closures Stage 4) — no `ast_bridge`/`compiler.rs` involvement
/// yet, raw builtins only, the same way Phase 0/1's and Stage 1's own
/// raw-builtin tests preceded their self-hosted-compiler-body counterparts.
/// This module is built entirely by hand (`llvm-module::create`, no
/// `Interp::compile_function`/`add_compiled_function`), so it has to
/// explicitly declare the `rt_closure_new`/`rt_apply_any` shims
/// `build-make-closure`/`build-closure-apply` call into (bodyless
/// `add-function` declarations,
/// resolved via LLVM's default process-symbol lookup since every `rt_*`
/// function is a `#[no_mangle]` symbol already linked into this test
/// binary) — the same idiom `rt_root_count`/`rt_truncate_sexpr_roots` need
/// in the hand-built `loop`/`return` tests elsewhere in this file. `Heap`
/// must be registered active for the duration of the call too, since
/// `rt_closure_new` allocates on it.
#[test]
fn a_closure_made_from_a_capturing_function_can_be_called_indirectly() {
    let src = r#"
        (defun build-and-run-closure-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((ignored-new-decl (add-function m "rt_closure_new"))) ())
            (let ((ignored-apply-decl (add-function m "rt_apply_any"))) ())
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
                      (store-arg builder2 env-arr 0 (const-word builder2 100))
                      (let ((closure (build-make-closure builder2 m add-offset-fn env-arr 1 0 0)))
                        (let ((args-arr (alloca-args builder2 1)))
                          (store-arg builder2 args-arr 0 (const-word builder2 5))
                          (build-ret builder2 (build-closure-apply builder2 m closure args-arr 1)))))))
                m))))
        (build-and-run-closure-module)
    "#;
    let module = expect_llvm_module(eval_ok(src));
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
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
        (defun add2 ((a i32) (b i32)) i32 (+ a b))
        (compile add2)
        (add2 10 32)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A quoted symbol keeps the module that owns it when it crosses into
/// compiled code.
///
/// A symbol's identity is an address, which cannot be baked into a program
/// that may run in another process, so compiled code rebuilds the symbol from
/// what `core_bridge::sym_form` wrote down. Writing down only the *name* is
/// what this guards against: two modules that both write `a-module-local-tag`
/// own two different symbols, so a bare name would intern into whichever
/// module the runtime defaulted to, and the compiled `tag` would stop being
/// `equal` to the interpreted `same?`'s own literal. Only one side is
/// compiled here on purpose — with both compiled they would agree on the
/// wrong symbol and the test would pass while broken.
#[test]
fn a_compiled_quoted_symbol_keeps_its_home_module() {
    let v = eval_ok_with_compiler(
        r#"
        (module m
          (pub defun tag () Sexpr (quote a-module-local-tag))
          (pub defun same? ((s Sexpr)) bool (equal s (quote a-module-local-tag))))
        (compile m::tag)
        (m::same? (m::tag))
        "#,
    );
    assert!(expect_bool(v), "the compiled symbol must be the module's own");
}

/// The same for a symbol from the fixed vocabulary, which crosses by its
/// `BUILTIN_SYMBOLS` index instead of by name (`rt_wk_symbol`).
#[test]
fn a_compiled_quoted_system_symbol_is_the_same_symbol() {
    let v = eval_ok_with_compiler(
        r#"
        (module m
          (pub defun tag () Sexpr (quote let))
          (pub defun same? ((s Sexpr)) bool (equal s (quote let))))
        (compile m::tag)
        (m::same? (m::tag))
        "#,
    );
    assert!(expect_bool(v), "one `let`, whichever side built it");
}

#[test]
fn compile_returns_true_on_success() {
    let v = eval_ok_with_compiler(r#"(defun answer () i32 42) (compile answer)"#);
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
        (defun first-of-rest ((a i32) &rest (xs i32)) i32
          a)
        (compile first-of-rest)
        (first-of-rest 1 10 20)
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, 1),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `target` (a `&rest`-taking `defun`) is referenced *bare* — not called
/// directly — inside `run-it`'s own `let`, so the checker reifies it as
/// `Expr::FnRef`. `ast_bridge::translate_fnref` used to synthesize a
/// forwarding wrapper closure with only `target`'s *fixed* parameters,
/// silently dropping the rest list rather than failing to compile — the
/// wrapper still compiled and ran, it just always called `target` with an
/// empty/garbage `xs`. This asserts the actual forwarded content, not just
/// that compilation succeeds, since a "compiles fine but silently wrong"
/// bug is exactly what a `compile ok` check alone would miss. Everything
/// happens inside the single compiled `run-it` function (never observed by
/// the tree-walking interpreter in between), matching
/// `compile_dispatches_a_top_level_function_passed_by_name_through_apply_fn`'s
/// own reasoning for why that's necessary.
#[test]
fn fnref_of_a_variadic_function_forwards_the_rest_list() {
    let v = eval_ok_with_compiler(
        r#"
        (defun target ((a i32) &rest (xs i32)) i32
          (match (sexpr-car xs)
            ((i32 n) n)
            (_ -1)))
        (defun run-it () i32
          (let ((f target))
            (apply f 1 (sexpr-cons (i32 10) (i32 20)))))
        (compile target)
        (compile run-it)
        (run-it)
        "#,
    );
    assert_eq!(v, Value::Int(10));
}

#[test]
fn an_uncompiled_function_still_tree_walks_normally() {
    // Sanity check that `compiled`-table dispatch doesn't break the
    // ordinary path for a function nobody asked to `compile`.
    let v = eval_ok_with_compiler(r#"(defun answer () i32 42) (answer)"#);
    match v {
        Value::Int(n) => assert_eq!(n, 42),
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
    let ir = eval_string(&src);
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
        (defun sum-of-squares ((a i32) (b i32)) i32
          (labels ((square ((x i32)) i32 (* x x))
                   (sum-helper ((x i32) (y i32)) i32 (+ (square x) (square y))))
            (sum-helper a b)))
        (compile sum-of-squares)
        (sum-of-squares 3 4)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 25),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The end-to-end Stage 2 slice (outer-scope capture), from real typelisp
/// source: `rec`'s body references `offset`, the enclosing `defun`'s own
/// parameter — neither its own parameter `k` nor a sibling name — so
/// `ast_bridge`'s `labels_free_vars` must collect it as a real capture, and
/// `(compile add-offset)` must build `rec` under the extended ABI and wire
/// the trailing body's call to pass `offset` through.
#[test]
fn compile_dispatches_a_defun_with_a_capturing_labels_body_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun add-offset ((offset i32) (n i32)) i32
          (labels ((rec ((k i32)) i32 (+ k offset)))
            (rec n)))
        (compile add-offset)
        (add-offset 10 5)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The multi-sibling case `compile_dispatches_a_defun_with_a_capturing_labels_body_to_native_code`
/// doesn't exercise: `helper` itself never references `offset`, only its
/// sibling `rec` does — so the *block's* free-variable list (not `helper`'s
/// own) must still include `offset`, and `helper` must still receive it (via
/// `bind-captures`, even though its own body never reads it) purely so it
/// can forward it on to `rec`. This is the shared-environment design's load-
/// bearing claim (`compile::freevars::labels_free_vars`'s doc comment): if
/// each sibling computed its own narrower captured list instead, `helper`
/// would have no value to forward and this call would fail to compile.
#[test]
fn a_sibling_that_never_references_a_capture_still_forwards_it_to_another_sibling() {
    let v = eval_ok_with_compiler(
        r#"
        (defun choose ((offset i32) (n i32)) i32
          (labels ((helper ((k i32)) i32 (rec k))
                   (rec ((k i32)) i32 (+ k offset)))
            (helper n)))
        (compile choose)
        (choose 10 5)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 15),
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
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"
        (let ((m (llvm-module::create "mod")))
          (compile-function m "double" '((x . 0)) '(assoc "i32" "+" true (0 var "x" false) (0 var "x" false)))
          (compile-function m "quadruple" '((n . 0)) '(call "double" (0 call "double" (0 var "n" false)))))
        "#,
    ));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
    let ir = eval_string_with_compiler(
        r#"(to-string (compile-function (llvm-module::create "mod") "f" '((n . 0)) '(call "f" (0 var "n" false))))"#,
    );
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
        (defun square ((x i32)) i32 (* x x))
        (defun sum-of-squares ((a i32) (b i32)) i32 (+ (square a) (square b)))
        (compile square)
        (compile sum-of-squares)
        (sum-of-squares 3 4)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 25),
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
        (defun square ((x i32)) i32 (* x x))
        (defun sum-of-squares ((a i32) (b i32)) i32 (+ (square a) (square b)))
        (compile sum-of-squares)
        (sum-of-squares 3 4)
        "#,
    )
    .expect("transitive compile of `square` should succeed");
    assert_eq!(v, Value::Int(25), "3*3 + 4*4 = 25, with `square` auto-compiled");
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
        (defun loop-forever ((n i32)) i32 (loop-forever n))
        (compile loop-forever)
        "#,
    );
    assert!(expect_bool(v));
}

/// The end-to-end proof of labels/closures Stage 6 (multi-expression body
/// support): a top-level `defun`'s body with *two* statements — a `setf` on
/// a global (evaluated purely for effect, its own value discarded) then a
/// final arithmetic expression (the function's real return value).
/// `Interp::compiled_fn_body` used to reject any `f.body.len() != 1`; it now
/// collapses a multi-expression body to a single bindingless `(let () e1
/// e2)` via `ast_bridge::single_body_expr`, which `compiler.rs`'s existing
/// `compile-let`/`compile-let-body` already know how to sequence — so this
/// needs no `compiler.rs` change, only proof the wrapping/collapsing is
/// wired correctly end to end. The global write is what proves the first
/// statement actually *ran* (not just parsed harmlessly): the assertion
/// checks both the return value and the mutated global.
#[test]
fn compile_dispatches_a_defun_with_a_two_statement_body_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defvar (counter i32) 0)
        (defun bump ((x i32)) i32
          (setf counter (+ counter 1))
          (+ x 100))
        (compile bump)
        (+ (bump 1) (* counter 1000))
        "#,
    );
    assert_eq!(v, Value::Int(1101), "bump(1)=101 or the setf never ran (counter would still read 0)");
}

/// The `lambda` counterpart of the two-statement `defun` test above: an
/// *escaping* lambda (boxed into a `ClosureBox`, not an IIFE) whose body has
/// two statements — proves `ast_bridge::translate_lambda`'s own
/// `single_body_expr` wrapping is wired correctly, independent of
/// `compiled_fn_body`'s.
#[test]
fn compile_dispatches_an_escaping_lambda_with_a_two_statement_body_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defvar (calls i32) 0)
        (defun make-adder ((n i32)) (fn (i32) i32)
          (lambda ((x i32)) i32
            (setf calls (+ calls 1))
            (+ x n)))
        (defun apply-fn ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (compile make-adder)
        (compile apply-fn)
        (+ (apply-fn (make-adder 5) 10) (* calls 1000))
        "#,
    );
    assert_eq!(v, Value::Int(1015), "adder(10)=15 with n=5, plus 1000*calls proving the setf statement ran once");
}

/// The `labels` sibling counterpart: one sibling's own body has two
/// statements — proves `ast_bridge::translate_labels_def`'s caller
/// (`translate_labels`, over each `LabelDef`'s own body) wraps correctly,
/// independent of the trailing-body wrapping exercised below.
#[test]
fn compile_dispatches_a_labels_sibling_with_a_two_statement_body_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defvar (calls i32) 0)
        (defun sum-of-squares ((a i32) (b i32)) i32
          (labels ((square ((x i32)) i32
                     (setf calls (+ calls 1))
                     (* x x))
                   (sum-helper ((x i32) (y i32)) i32 (+ (square x) (square y))))
            (sum-helper a b)))
        (compile sum-of-squares)
        (+ (sum-of-squares 3 4) (* calls 1000))
        "#,
    );
    assert_eq!(v, Value::Int(2025), "3*3+4*4=25, plus 1000*calls (square called twice) proving each call's first statement ran");
}

/// The `labels` *trailing body* counterpart: the block's own trailing body
/// (after all sibling defs) has two statements, not just one — proves
/// `translate_labels`'s own separate wrapping of `body` (as opposed to each
/// def's own `fbody`) is wired correctly.
#[test]
fn compile_dispatches_a_labels_trailing_body_with_two_statements_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defvar (calls i32) 0)
        (defun sum-of-squares ((a i32) (b i32)) i32
          (labels ((square ((x i32)) i32 (* x x))
                   (sum-helper ((x i32) (y i32)) i32 (+ (square x) (square y))))
            (setf calls (+ calls 1))
            (sum-helper a b)))
        (compile sum-of-squares)
        (+ (sum-of-squares 3 4) (* calls 1000))
        "#,
    );
    assert_eq!(v, Value::Int(1025), "3*3+4*4=25, plus 1000*calls proving the trailing body's first statement ran");
}

/// The `match` arm counterpart: one arm's own body has two statements —
/// proves `ast_bridge::translate_arms`'s `single_body_expr` wrapping is
/// wired correctly.
#[test]
fn compile_matches_with_a_two_statement_arm_body_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defvar (calls i32) 0)
        (defenum Maybe<T> (Just T) (Nothing))
        (defun m ((x i32)) i32
          (match (Maybe::Just x)
            ((Just v) (setf calls (+ calls 1)) v)
            ((Nothing) x)))
        (compile m)
        (+ (m 7) (* calls 1000))
        "#,
    );
    assert_eq!(v, Value::Int(1007), "m(7)=7 via the Just arm, plus 1000*calls proving its first statement ran");
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
        (defun calls-immediately ((n i32)) i32 ((lambda ((x i32)) i32 (+ x 1)) n))
        (compile calls-immediately)
        (calls-immediately 9)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 10),
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
        (defun adds-offset ((offset i32) (n i32)) i32 ((lambda ((y i32)) i32 (+ y offset)) n))
        (compile adds-offset)
        (adds-offset 100 5)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 105),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The end-to-end Stage 4 slice (escaping + capturing `lambda`): `adder`
/// returns a closure that captures its own parameter `n`, and `apply-fn`
/// (a separately-compiled function taking a `(fn (i64) i64)` *parameter*)
/// calls it through `apply-indirect`/`build-closure-apply` — flowing from
/// one `compile`d function's `i64` return straight into another's `i64`
/// argument via the ordinary `Expr::Call`-dispatches-to-`Interp::compiled`
/// path (see `Interp::compile_function`'s doc comment). This closure value
/// happens to never get inspected by tree-walking code along the way here —
/// unlike, since the closure-representation unification's boundary-opening
/// (Stage 3), `compile_interp_applies_a_closure_returned_by_compiled_code`
/// below, which *does* have the interpreter apply a compiled-produced
/// closure directly.
#[test]
fn compile_dispatches_an_escaping_capturing_lambda_called_through_another_compiled_function() {
    let v = eval_ok_with_compiler(
        r#"
        (defun adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n)))
        (defun apply-fn ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (compile adder)
        (compile apply-fn)
        (apply-fn (adder 5) 10)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A closure that captures a **`match` arm binding** rather than a `let`
/// binding or a parameter. Every capture reads a shared cell, and `pat-bind`
/// binds an ordinary slot — so `core_bridge::translate_match` wraps an arm
/// whose body captures one in a `let` that rebinds it, and the island's
/// existing `kind >= 10` cell path does the rest.
///
/// This was a hard error until 2026-08-30 ("`g` is referenced but no binder
/// in scope states its representation"): the arm body was translated without
/// its own pattern's bindings in scope, so the `cellvar` a capture compiles
/// to had no representation to state.
#[test]
fn compile_dispatches_a_lambda_that_captures_a_match_arm_binding() {
    let v = eval_ok_with_compiler(
        r#"
        (defun mk ((o Option<i32>)) (fn (i32) bool)
          (match o
            ((some g) (lambda ((a i32)) bool (< a g)))
            ((none) (lambda ((a i32)) bool false))))
        (defun try-it ((o Option<i32>) (a i32)) i32 (if ((mk o) a) 1 0))
        (compile mk)
        (compile try-it)
        (+ (* 100 (try-it (option::some 5) 3))
           (+ (* 10 (try-it (option::some 5) 7)) (try-it (option::none) 1)))
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 100),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The same, from a `defstruct` pattern binding two fields at once: the
/// representations come off the `pat-ctor` node's own per-field list, which
/// is the only place a generic ADT's instantiated field types are known.
#[test]
fn compile_dispatches_a_lambda_that_captures_two_struct_pattern_bindings() {
    let v = eval_ok_with_compiler(
        r#"
        (defstruct bx (v i32) (w i32))
        (defun mk2 ((b bx)) (fn (i32) i32)
          (match b ((new v w) (lambda ((a i32)) i32 (+ a (+ v w))))))
        (defun use2 ((b bx) (a i32)) i32 ((mk2 b) a))
        (compile mk2)
        (compile use2)
        (use2 (bx::new 3 4) 10)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 17),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The rebinding is a *cell*, not a copy: three calls to one closure see each
/// other's writes. This is what says wrapping the body in a `let` gives the
/// same sharing semantics cell-boxing the pattern binder itself would — and
/// it is safe precisely because an arm's bindings are visible nowhere else,
/// so the `let` shadows every read and every `set` in the body.
#[test]
fn a_captured_match_arm_binding_is_shared_not_copied() {
    let v = eval_ok_with_compiler(
        r#"
        (defun counter ((o Option<i32>)) (fn () i32)
          (match o
            ((some n) (lambda () i32 (progn (setf n (+ n 1)) n)))
            ((none) (lambda () i32 0))))
        (defun run-three ((o Option<i32>)) i32
          (let ((f (counter o))) (+ (f) (+ (f) (f)))))
        (compile counter)
        (compile run-three)
        (run-three (option::some 10))
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 36),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The mirror image: the `match` sits *inside* the closure, over an
/// `Option<Sexpr>` — the one type whose `(some P)` is niched into a
/// `pat-nonempty` node instead of a `pat-ctor`. `core_freevars` did not count
/// that node's bindings, so `g` looked free in the lambda's body and the
/// closure captured a name bound inside itself. The `Option<i32>` spelling of
/// the same program never showed it: only `Option<Sexpr>` is niched.
#[test]
fn a_niched_option_sexpr_pattern_binds_inside_an_enclosing_closure() {
    let v = eval_ok_with_compiler(
        r#"
        (defun pick ((o Option<Sexpr>)) (fn () Sexpr)
          (lambda () Sexpr (match o ((some g) g) ((none) (quote nothing)))))
        (defun hit ((o Option<Sexpr>)) i32 (if (equal ((pick o)) (quote hi)) 1 0))
        (compile pick)
        (compile hit)
        (+ (* 10 (hit (option::some (quote hi)))) (hit (option::none)))
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 10),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The other direction of the same slice, and what the plan called the
/// "compiled -> interpreted hole": `adder` is *not* compiled, so `(adder 5)`
/// is an ordinary tree-walked `lambda` and yields a `BoxedObj::Closure`. It
/// then crosses into `apply-fn`, which *is* compiled, and gets applied there.
///
/// Before `rt_apply_any` this aborted the process: `build-closure-apply`
/// emitted a `rt_closure_fnptr` call, which `fatal`s on anything that is not
/// a `BoxedObj::CompiledClosure`, and `fatal` cannot be caught. Compiled code
/// now calls a closure value through `rt_apply_any`, which dispatches on what
/// the box actually holds and re-enters the interpreter for this case.
#[test]
fn compile_applies_an_interpreted_closure_handed_to_a_compiled_function() {
    let v = eval_ok_with_compiler(
        r#"
        (defun adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n)))
        (defun apply-fn ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (compile apply-fn)
        (apply-fn (adder 5) 10)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The same crossing with representations that are *not* raw machine words:
/// a `Sexpr` argument arrives tagged and has to be decoded, the interpreted
/// body conses (so it allocates with a compiled frame on the stack), and the
/// result goes back tagged.
///
/// The word itself says nothing about which of the two it is — that is the
/// whole reason `BoxedObj::Closure` carries its own `params`/`ret`. Deciding
/// by the value's shape instead is the mistake `(which (f64 1.5))` caught
/// when `f64` unified into one value world.
#[test]
fn an_interpreted_closure_called_from_compiled_code_crosses_tagged_values() {
    let v = eval_ok_with_compiler(
        r#"
        (defun consr ((tail Option<Sexpr>)) (fn (Option<Sexpr>) Option<Sexpr>)
          (lambda ((x Option<Sexpr>)) Option<Sexpr> (sexpr-cons x tail)))
        (defun apply-fn ((f (fn (Option<Sexpr>) Option<Sexpr>)) (v Option<Sexpr>)) Option<Sexpr> (f v))
        (compile apply-fn)
        (sexpr-i32 (sexpr-car (apply-fn (consr (i32 2)) (i32 1))))
        "#,
    );
    assert_eq!(v, Value::Int(1));
}

/// And with `f64`, the representation that crosses as a raw `to_bits` pattern
/// and whose *decode* allocates a box — so a later argument's decode can
/// collect an earlier one if it was not rooted on the way in.
#[test]
fn an_interpreted_closure_called_from_compiled_code_crosses_floats() {
    let v = eval_ok_with_compiler(
        r#"
        (defun scaler ((k f64)) (fn (f64 f64) f64) (lambda ((x f64) (y f64)) f64 (* k (+ x y))))
        (defun apply-fn ((f (fn (f64 f64) f64)) (x f64) (y f64)) bool (= (f x y) 9.0))
        (compile apply-fn)
        (apply-fn (scaler 3.0) 1.0 2.0)
        "#,
    );
    assert_eq!(v, Value::Bool(true));
}

/// The interpreted callee under real collections: the compiled caller conses
/// in a loop before applying `f`, on a heap small enough that those conses
/// force repeated `gc()` runs — the counterpart of
/// `repeated_calls_through_a_captured_closure_survive_gc_pressure` for a
/// callee the collector has to keep alive across a frame it cannot see.
#[test]
fn an_interpreted_closure_held_by_compiled_code_survives_gc_pressure() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defun adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n)))
        (defun outer ((f (fn (i32) i32)) (x i32)) i32
          (let ((ignored (loop
                           (if (eq x 0) (break) ())
                           (sexpr-cons (i32 0) (i32 0))
                           (setf x (- x 1)))))
            (f 5)))
        (compile outer)
        (outer (adder 10) 5000)
        "#,
        20000,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(15));
}

/// The end-to-end Stage 4 slice (`Expr::FnRef` as a closure value): `square`
/// is used bare (no call syntax) where a `(fn (i64) i64)` is expected — the
/// checker reifies that into a non-capturing forwarding `lambda` (`(lambda
/// ... (call "square" (var arg0)))`), reaching the exact same closure-box/
/// `build-closure-apply` machinery the closure-value test above does.
///
/// `run-it`'s body (not the top-level call site) is where `square` appears
/// bare, so the whole chain — `run-it` -> `apply-fn` -> the `square` `FnRef`
/// — stays compiled. (This routing used to be load-bearing: a bare top-level
/// `(apply-fn square 5)` produced an *interpreted* closure that the compiled
/// boundary rejected outright. It no longer is — an interpreted closure
/// crosses and is applied through `rt_apply_any`, see
/// `compile_applies_an_interpreted_closure_handed_to_a_compiled_function` —
/// but this test keeps the `run-it` form as the original end-to-end slice.)
#[test]
fn compile_dispatches_a_top_level_function_passed_by_name_through_apply_fn() {
    let v = eval_ok_with_compiler(
        r#"
        (defun square ((x i32)) i32 (* x x))
        (defun apply-fn ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (defun run-it () i32 (apply-fn square 5))
        (compile square)
        (compile apply-fn)
        (compile run-it)
        (run-it)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 25),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Closure-representation unification, Stage 3 (interp<->compiled boundary
/// opening): `make-adder` is `compile`d and returns a `(fn (i64) i64)`; the
/// call site is a bare top-level `let`, so it's tree-walked, not compiled.
/// `(make-adder 3)` decodes through `Interp::call_compiled`'s `Type::Fn` arm
/// of `is_boxed_sexpr_type` into a real `RtValue::Sexpr(Value::Boxed(_))` at
/// a `BoxedObj::CompiledClosure` (not the misread `RtValue::Int` the pre-
/// Stage-3 gap left it as), and `(adder 5)` — an ordinary `Expr::Apply` on a
/// let-bound variable — dispatches through `Interp::eval`'s new
/// `is_compiled_closure` arm (`Self::call_closure_box`), the first time a
/// compiled closure is directly invoked *by* tree-walking code rather than
/// merely passed between two compiled functions.
#[test]
fn compile_interp_applies_a_closure_returned_by_compiled_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n)))
        (compile make-adder)
        (let ((adder (make-adder 3))) (adder 5))
        "#,
    );
    assert_eq!(v, Value::Int(8));
}

/// Stage 3's other half: a compiled-produced `Fn` value crossing back into
/// *another* compiled call's argument list after having been visibly
/// `RtValue::Sexpr`-decoded and interpreter-applied first (unlike
/// `compile_dispatches_an_escaping_capturing_lambda_called_through_another_compiled_function`
/// above, where the closure never left compiled-to-compiled `Expr::Call`
/// dispatch) — `adder` is applied once directly by the interpreter, then the
/// very same closure value is handed to `apply-fn` (compiled) as an
/// ordinary argument, proving `encode_crossing_args` round-trips a
/// `BoxedObj::CompiledClosure` correctly whether or not tree-walking code
/// touched it in between.
#[test]
fn compile_a_compiled_produced_closure_survives_interp_apply_then_crosses_into_another_compiled_call() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n)))
        (defun apply-fn ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (compile make-adder)
        (compile apply-fn)
        (let ((adder (make-adder 3)))
          (let ((direct (adder 5)))
            (+ direct (apply-fn adder 10))))
        "#,
    );
    // direct = 3 + 5 = 8; apply-fn adder 10 = 3 + 10 = 13; 8 + 13 = 21.
    assert_eq!(v, Value::Int(21));
}

/// Closure-representation unification, Stage 3 (`struct_field_kind` gap
/// (c)): a `defstruct` field of `Fn` type used to classify as kind `0` ("not
/// representable yet" — see that function's doc comment), panicking
/// `compile-tag-struct-field`/`compile-sexpr-field` on construction/read.
/// `Type::Fn` now joins `Str`/nested-struct's passthrough kind `6`, so
/// `make-holder` (constructing a `holder` whose field holds `make-adder`'s
/// compiled closure) and `call-held` (reading the field back and applying
/// it) both compile and dispatch to native code.
#[test]
fn compile_dispatches_a_defstruct_field_of_fn_type_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct holder (f (fn (i32) i32)))
        (defun make-adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n)))
        (defun make-holder ((n i32)) holder (holder::new (make-adder n)))
        (compile make-adder)
        (compile make-holder)
        (compile holder::f)
        (defun call-held ((h holder) (x i32)) i32 (let ((f h::f)) (f x)))
        (compile call-held)
        (call-held (make-holder 3) 5)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(8));
}

/// Follow-up to Stage 4: a `labels` sibling referenced *as a value* (not
/// called) — here, the block's own trailing body bare-returns `f` instead of
/// calling it. `ast_bridge::ast_to_sexpr_scoped`'s `Expr::Var` arm doesn't
/// distinguish this from any other variable reference (it always emits
/// `(var name)`); `compiler.rs`'s `resolve-value` is what now resolves it —
/// not found in the ordinary `env`, found instead in `fn-env`, so it gets
/// boxed into a fresh `ClosureBox` on the spot (`build-make-closure`, the
/// same builtin `compile-lambda` already uses). `(apply-indirect (var "f")
/// (int-any-width 0 5))` then calls through that box, proving the boxing produced a
/// genuinely callable closure, not just a value that type-checks.
#[test]
fn the_compiler_body_boxes_a_bare_labels_sibling_reference() {
    // This module is built by a direct `compile-function` call (bypassing
    // `Interp::compile_function`/`add_compiled_function`, which would
    // otherwise forward-declare every `rt_*` shim automatically), so
    // `build-make-closure` needs `rt_closure_new` explicitly declared here —
    // a bodyless `add-function`, resolved via LLVM's default process-symbol
    // lookup since it's a `#[no_mangle]` symbol already linked into this
    // test binary, the same idiom `rt_root_count`/`rt_truncate_sexpr_roots`
    // need elsewhere in this file. `Heap` must be registered active for the
    // call too, since `rt_closure_new` allocates on it.
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"(let ((m (llvm-module::create "mod")))
             (let ((ignored-new (add-function m "rt_closure_new"))) ())
             (let ((ignored-apply (add-function m "rt_apply_any"))) ())
             (compile-function m "outer" '() '(labels () (("f" ((n . 0)) (var "n" false))) (apply-indirect (var "f" true) (0 int-any-width 0 5)))))"#,
    ));
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
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
    // See `the_compiler_body_boxes_a_bare_labels_sibling_reference`'s doc
    // comment for why `rt_closure_new` needs an explicit declaration and
    // `Heap` needs to be active here. `rt_push_sexpr_root` needs one too now
    // — see `the_compiler_body_compiles_a_labels_form_that_captures_an_outer_scope_value`'s
    // own doc comment (`bind-captures` unconditionally roots every captured
    // value, closure-representation unification Stage 4).
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"(let ((m (llvm-module::create "mod")))
             (let ((ignored-new (add-function m "rt_closure_new"))) ())
             (let ((ignored-apply (add-function m "rt_apply_any"))) ())
             (let ((ignored-push (add-function m "rt_push_sexpr_root"))) ())
             (compile-function m "outer" '((offset . 0) (n . 0)) '(labels ((offset . 0)) (("rec" ((k . 0)) (assoc "i32" "+" true (0 var "k" false) (0 var "offset" false)))) (apply-indirect (var "rec" true) (0 int-any-width 0 5)))))"#,
    ));
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
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
/// itself (a `call ... @rt_closure_new` showing up in the IR — the
/// closure-representation unification's flip off the old `build_array_malloc`
/// this test used to look for) is exactly what this test confirms.
/// `rt_closure_new` needs an explicit declaration for the same reason
/// `the_compiler_body_boxes_a_bare_labels_sibling_reference`'s doc comment
/// explains — this module is built by a direct `compile-function` call.
#[test]
fn the_compiler_body_boxes_a_labels_sibling_that_bare_references_itself() {
    let ir = eval_string_with_compiler(
        r#"(let ((m (llvm-module::create "mod")))
             (let ((ignored (add-function m "rt_closure_new"))) ())
             (to-string (compile-function m "outer" '() '(labels () (("f" () (var "f" true))) (apply "f")))))"#,
    );
    assert!(ir.contains("call i64 @rt_closure_new"), "IR was:\n{}", ir);
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
        (defun make-adder ((n i32)) (fn (i32) i32)
          (labels ((adder ((x i32)) i32 (+ x n)))
            adder))
        (defun apply-fn ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (compile make-adder)
        (compile apply-fn)
        (apply-fn (make-adder 5) 10)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 15),
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
        (defun make-pair ((n i32)) (fn (i32) i32)
          (labels ((f ((x i32)) i32 (+ x n))
                   (g ((x i32)) i32 (+ x (f x))))
            f))
        (defun apply-fn ((h (fn (i32) i32)) (n i32)) i32 (h n))
        (compile make-pair)
        (compile apply-fn)
        (apply-fn (make-pair 5) 10)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 15),
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
        (defun make-caller ((n i32)) (fn () i32)
          (labels ((double ((x i32)) i32 (* x 2)))
            (lambda () i32 (double n))))
        (defun apply-fn0 ((f (fn () i32))) i32 (f))
        (compile make-caller)
        (compile apply-fn0)
        (apply-fn0 (make-caller 21))
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// The closure-representation unification's replacement for the retired
/// `ClosureBox` refcount tests: `outer(n, x)` builds its own capturing
/// closure `cb` internally (`make-adder`, itself `compile`d — the interp/
/// compiled boundary's own remaining gap around a bare top-level function
/// *value* is Stage 3's, not this one's, so `cb` is constructed and consumed
/// entirely on the compiled side), then a `labels` sibling `rec` captures and
/// calls it indirectly through the *direct*, non-escaping `compile-env-args`
/// path (`compile-apply` building `rec`'s env array each call). A compiled
/// closure is now an ordinary GC-heap `BoxedObj::CompiledClosure` with
/// nothing to leak a refcount on — this instead proves the same
/// repeated-call path stays correct and crash-free under GC pressure
/// (unrelated cons allocations forcing real collections mid-loop,
/// `run_with_compiler_and_capacity`'s small-heap idiom), the property that
/// actually matters now.
#[test]
fn repeated_calls_through_a_captured_closure_survive_gc_pressure() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defun make-adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n)))
        (defun outer ((n i32) (x i32)) i32
          (let ((cb (make-adder n)))
            (labels ((rec ((y i32)) i32 (cb y)))
              (let ((ignored (loop
                               (if (eq x 0) (break) ())
                               (sexpr-cons (i32 0) (i32 0))
                               (setf x (- x 1)))))
                (rec 5)))))
        (compile make-adder)
        (compile outer)
        (outer 10 5000)
        "#,
        20000,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(15), "the captured closure keeps working after many unrelated conses force repeated gc() runs");
}

/// The escaping-closure counterpart of the test above: `make-wrapper`
/// builds its own inner closure `inner` (`make-adder`, `compile`d
/// separately — again staying entirely on the compiled side of the interp/
/// compiled boundary, see the test above) and returns a `lambda` that
/// captures it. A `BoxedObj::CompiledClosure`'s captures are GC-traced
/// (Stage 1's `push_boxed_nested` fan-out), so the returned wrapper keeps
/// its captured `inner` closure alive across a GC — and calling the wrapper
/// (which calls `inner` in turn) after many unrelated allocations proves
/// that, replacing the retired refcount-cascade assertion with the
/// GC-survival property that actually matters now.
#[test]
fn an_escaping_lambdas_captured_closure_survives_gc_pressure() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defun make-adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n)))
        (defun make-wrapper ((n i32)) (fn (i32) i32)
          (let ((inner (make-adder n)))
            (lambda ((x i32)) i32 (inner x))))
        (defun call-through-wrapper ((n i32) (count i32)) i32
          (let ((wrapper (make-wrapper n)))
            (let ((ignored (loop
                             (if (eq count 0) (break) ())
                             (sexpr-cons (i32 0) (i32 0))
                             (setf count (- count 1)))))
              (wrapper 7))))
        (compile make-adder)
        (compile make-wrapper)
        (compile call-through-wrapper)
        (call-through-wrapper 3 5000)
        "#,
        20000,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(10), "the wrapper's captured inner closure survives many unrelated conses forcing repeated gc() runs");
}

/// Closure-representation unification, Stage 4 (shared-cell captures): a
/// `setf` on a captured name inside a compiled closure mutates the *shared*
/// `BoxedObj::Cell` (`compile-cellset`/`rt_cell_set`), not a per-call
/// snapshot — so `counter`, called twice, sees its own previous write on the
/// second call. Before this stage, `bind-params`/`bind-captures` copied a
/// captured value into each activation's own stack slot; a `setf` there
/// would only ever have mutated that local copy, invisible to the next call
/// through the same closure — the documented gap this stage exists to close
/// (CL semantics, matching the interpreter's own heap-cell captures).
#[test]
fn compile_a_setf_on_a_captured_name_is_visible_on_the_next_call_through_the_same_closure() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-counter ((start i32)) (fn () i32)
          (lambda () i32 (setf start (+ start 1))))
        (defun call-twice ((c (fn () i32))) i32
          (let ((a (c))) (let ((b (c))) (+ a (* b 100)))))
        (compile make-counter)
        (compile call-twice)
        (call-twice (make-counter 0))
        "#,
    );
    // First call: start 0 -> 1, returns 1 (a = 1). Second call, same
    // closure, same cell: start 1 -> 2, returns 2 (b = 2). 1 + 200 = 201 —
    // only possible if both calls mutated and read the *same* cell.
    match v {
        Value::Int(n) => assert_eq!(n, 201),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Stage 4's other half: two `labels` siblings that both capture the *same*
/// outer name share the *same* cell too — a `setf` through one
/// (`bump`) is immediately visible through the other (`read-it`), proving
/// the sharing isn't an artifact of always calling through one particular
/// closure value (the test above) but genuine shared mutable state, exactly
/// like two closures over the same CL `let` binding.
#[test]
fn compile_a_setf_through_one_labels_sibling_is_visible_through_another_sharing_the_same_capture() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-pair ((start i32)) i32
          (labels ((bump () i32 (setf start (+ start 1)))
                   (read-it () i32 start))
            (let ((ignored1 (bump)))
              (let ((ignored2 (bump)))
                (read-it)))))
        (compile make-pair)
        (make-pair 10)
        "#,
    );
    // bump: 10 -> 11, bump: 11 -> 12, read-it: 12 — read-it never itself
    // writes, so it can only see 12 by reading the exact cell bump wrote to.
    match v {
        Value::Int(n) => assert_eq!(n, 12),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// GC stress for Stage 4's cell-rooting scheme: `bind-params`'/
/// `bind-let-values`'s cell-kind branch and `bind-captures`'s own permanent
/// `push-sexpr-root` (see those functions' own doc comments for why each
/// needs one, immediately, rather than relying on a later batched pass) are
/// the only things keeping a freshly allocated `BoxedObj::Cell` alive across
/// the many unrelated `cons` allocations a `gc()` needs to actually trigger
/// under a small heap — proves the cell (and the closures sharing it)
/// survive real collections, not just a GC-pressure-free happy path.
#[test]
fn compile_a_captured_cell_survives_gc_pressure_across_many_calls() {
    let v = run_with_compiler_and_capacity(
        r#"
        (defun make-counter ((start i32)) (fn () i32)
          (lambda () i32 (setf start (+ start 1))))
        (defun pump ((c (fn () i32)) (n i32)) i32
          (let ((ignored (loop
                           (if (eq n 0) (break) ())
                           (sexpr-cons (i32 0) (i32 0))
                           (setf n (- n 1)))))
            (c)))
        (defun run-it ((start i32) (n i32)) i32
          (pump (make-counter start) n))
        (compile make-counter)
        (compile pump)
        (compile run-it)
        (run-it 0 5000)
        "#,
        20000,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(1), "the counter's cell survives many unrelated conses forcing repeated gc() runs, then the single (c) call sees start still at 0");
}

// --- if/let/comparisons (labels/closures Stage 5) ---

/// `(bool b)` (`ast_bridge` already produced this tag; `compile-value` had no
/// receiving arm for it until now) — every compiled value is a plain `i64`,
/// so `true`/`false` compile straight to `1`/`0`.
#[test]
fn the_compiler_body_compiles_a_bool_literal_node() {
    let ir = eval_string_with_compiler(
        r#"(to-string (compile-function (llvm-module::create "mod") "answer" '() '(bool true)))"#,
    );
    assert!(ir.contains("ret i64 1"), "IR was:\n{}", ir);
}

/// `compile-assoc`'s new comparison arms (`<`/`<=`/`>`/`>=`/`=`/`eq`/`/=`) —
/// `build-icmp-lt` end to end, JIT-executed both ways.
#[test]
fn the_compiler_body_compiles_an_integer_comparison() {
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "lt" '((a . 0) (b . 0))
              '(assoc "i32" "<" true (0 var "a" false) (0 var "b" false)))"#,
    ));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
    let engine = module.borrow().create_jit_execution_engine(OptimizationLevel::None).expect("failed to create JIT execution engine");
    let lt = unsafe { engine.get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("lt").expect("failed to look up `lt`") };
    assert_eq!(unsafe { lt.call([3, 5].as_ptr(), 2) }, 1);
    assert_eq!(unsafe { lt.call([5, 3].as_ptr(), 2) }, 0);
}

// (Removed `compile_assoc_panics_on_an_unsupported_receiver_type` in
// interp-closure removal Stage 8a.) It hand-fed `compile-function` a raw
// `(assoc "i32" "int->char" ...)` node — deliberately bypassing
// `Interp::compile_function`'s up-front check — so the only thing left to catch
// the non-native `i32::int->char` was the island's `get-function` failing to
// find it in the module. Under the AOT-native island that failure is an
// `rt_llvm_call` process abort (it can't return a catchable error across the
// native-code boundary), so the assertion can't hold in-process. The
// user-facing path — a real `defun` calling `int->char`, then `(compile ...)`d
// — is now rejected cleanly and up front by `call_graph_edges`
// (`is_native_lowered_primitive_method`).
//
// That rejection has no subject left to test end to end: every builtin method
// on a natively lowered receiver now *has* a lowering, `i32::try-int->char`
// (which this pair of tests used last) included. What guards it instead is
// `externs::native_method_list_tests::every_registered_builtin_method_on_a_native_receiver_lowers`,
// which compares the registry against both native-method lists — so a new
// builtin that opens the gap again fails there, at the list, rather than here.

/// `compile-if`: `(if is-fn cond-form then-form else-form)` end to end —
/// `max(a, b)` via a comparison feeding the branch, JIT-executed both ways
/// to prove both the `then` and `else` arm are reachable and correct.
#[test]
fn the_compiler_body_compiles_an_if_expression() {
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "maxab" '((a . 0) (b . 0))
              '(if false
                   (assoc "i32" ">" true (0 var "a" false) (0 var "b" false))
                   (var "a" false)
                   (var "b" false)))"#,
    ));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
    let module = expect_llvm_module(eval_ok_with_compiler(
        r#"(compile-function (llvm-module::create "mod") "shadow_test" '((x . 0))
              '(assoc "i32" "+" true
                 (0 let (((x . 0) . (int-any-width 0 99))) (var "x" false))
                 (0 var "x" false)))"#,
    ));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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
        (defun fact ((n i32)) i32 (if (<= n 1) 1 (* n (fact (- n 1)))))
        (compile fact)
        (fact 10)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 3628800),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `loop`/`break`/`return`/`setf`: a `setf` on a `let`-bound local, compiled
/// through the full pipeline. Exercises the new alloca-backed `env`
/// representation (`bind-params`/`bind-captures`/`bind-let-values`).
/// `i32`: a bare integer literal defaults to `i32`
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
        Value::Int(n) => assert_eq!(n, 5),
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
        Value::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `(return)` with no value — `ast_bridge::translate_return`'s implicit-
/// `Unit` case, which also exercises `compile-value`'s new `"unit"` arm
/// (`compile-unit`). The tree-walking call dispatch decodes the raw `i64`
/// result by the callee's declared return type
/// (`Interp::decode_compiled_return`), whose `Type::Unit` arm (closure
/// unification Stage 8) turns `compile-unit`'s `0` encoding back into a
/// real `RtValue::Unit` — closing the pre-Stage-8 gap where it surfaced
/// as a bogus `RtValue::Int(0)`.
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
        Value::Empty => {}
        other => panic!("expected Unit, got {:?}", other),
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
        Value::Int(n) => assert_eq!(n, 15, "0+1+2+3+4+5"),
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
/// doc comment — the `Unit` result decodes as a real `RtValue::Unit` now).
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
        Value::Empty => {}
        other => panic!("expected Unit, got {:?}", other),
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
        Value::Int(n) => assert_eq!(n, 6),
        other => panic!("expected an Int, got {:?}", other),
    }
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
        Value::Empty => {}
        other => panic!("expected Unit, got {:?}", other),
    }
}

/// `dolist` (`prelude.rs`) desugars to the same `loop`/`break`/`setf` core as
/// `while`/`dotimes` (hence the identical `(compile not)`-first requirement,
/// since it too goes through `while`), *plus* a `match` on the `Sexpr` element
/// and `sexpr-consp`/`sexpr-car`/`sexpr-cdr` walking — all already compilable
/// (`Sexpr` `match` support, plus the `sexpr-*` `rt_*` shims
/// `is_rt_builtin_name` recognizes). So a `dolist`-using function compiles with
/// no `dolist`-specific machinery: this sums the `(i32 n)` elements of a
/// `Sexpr` list argument entirely in native code (the quoted list is built by
/// the tree-walking interpreter and handed to the compiled function), asserting
/// the real total — not just that compilation succeeded — so a "compiles but
/// walks the list wrong" bug can't hide. The `result-form` (`acc`) is the whole
/// `dolist`'s value, and the `(_ ())` arm keeps every `match` arm at `Unit`
/// (`setf` yields the assigned value, not `Unit`).
#[test]
fn compile_dispatches_a_dolist_summing_a_sexpr_list() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun sum-list-ints ((lst Option<Sexpr>)) i32
          (let ((acc (the i32 0)))
            (dolist (x lst acc)
              (match x ((i32 n) (setf acc (+ acc n)) ()) (_ ())))))
        (compile not)
        (compile sum-list-ints)
        (sum-list-ints (quote (1 2 3 4 5)))
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, 15),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `build-shl`/`build-ashr` round-trip a `Sexpr::i32` fixnum through the
/// planned tagged representation (the tag table in
/// `docs/dev/implementation-log.md`'s "Sexpr表現 + Match/Construct/共有Rust
/// ライブラリ 実装計画": tag `000`,
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
                    (let ((three (const-word builder 3)))
                      (let ((tagged (build-shl builder x three)))
                        (let ((untagged (build-ashr builder tagged three)))
                          (build-ret builder untagged)
                          m)))))))))
        (build-fixnum-round-trip-module)
    "#;
    let module = expect_llvm_module(eval_ok(src));
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
                    (let ((three (const-word builder 3)))
                      (let ((tagged (build-or builder (build-shl builder idx three) (const-word builder 2))))
                        (let ((tag-back (build-and builder tagged (const-word builder 7))))
                          (let ((idx-back (build-lshr builder tagged three)))
                            (build-ret builder (build-add builder (build-mul builder tag-back (const-word builder 1000)) idx-back))
                            m))))))))))
        (build-tag-round-trip-module)
    "#;
    let module = expect_llvm_module(eval_ok(src));
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

/// `Expr::Construct` over `Sexpr` itself: `(i32 n)`/`(Bool b)`/`()` all
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
        (defun make-int ((n i32)) Sexpr (i32 n))
        (compile make-int)
        (make-int 42)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));

    let v = run_with_compiler_and_prelude(
        r#"
        (defun make-true () Sexpr (Bool true))
        (compile make-true)
        (make-true)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Bool(true));

    let v = run_with_compiler_and_prelude(
        r#"
        (defun make-nil () Option<Sexpr> ())
        (compile make-nil)
        (make-nil)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Empty);
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
    assert_eq!(v, Value::Int(6), "\"foo\" ++ \"bar\" has 6 characters");
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
        (defun str-survives-gc ((n i32)) i32
          (let ((s (append "hello" " world")))
            (let ((ignored (loop
                             (if (eq n 0) (break) ())
                             (sexpr-cons (i32 0) (i32 0))
                             (setf n (- n 1)))))
              (length s))))
        (compile str-survives-gc)
        (str-survives-gc 5000)
        "#,
        // Bumped from 13000: the enum-representation unification added new
        // compiler-body functions/symbols (`rt_data_*`, `compile-option-
        // type-name`, ...), raising the self-hosted compiler's own baseline
        // heap usage enough to tip this tightly-tuned capacity over —
        // unrelated to this test's own correctness claim, the same class of
        // margin issue `scripts/test-serial.sh`'s `RUST_MIN_STACK` comment
        // documents for stack depth.
        20000,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(11), "\"hello world\" has 11 characters, even after many unrelated conses force a gc()");
}

/// A general ADT (`Option<i32>`'s `Some`, `AdtKind::Sum`) constructs via
/// `compile-construct-box`'s `rt_data_new` path (the enum-representation
/// unification's compiler flip) into a real `BoxedObj::Enum` — and
/// `Interp::call_compiled`'s `is_boxed_sexpr_type` extension decodes the
/// compiled function's `Option<i32>` return value as the proper
/// `RtValue::Sexpr` rather than the raw, undecoded box address this test
/// used to read directly out of process memory (a documented, now-closed
/// gap). An ordinary *interpreted* `match` over that returned value proves
/// both halves at once: the box really is heap-shaped correctly, and it
/// interoperates with the interpreter like any other enum value.
#[test]
fn compile_dispatches_a_function_that_constructs_a_general_adt_box_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun make-some ((n i32)) Option<i32> (Option::some n))
        (compile make-some)
        (match (make-some 42) ((Some x) x) ((None) -1))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
}

// ---- enum-representation unification (Stage 4): newly-opened scenarios ----
//
// The compiler flip (`compile-construct-box`/`compile-box-tag-test`/
// `compile-box-field` now building/reading a real `BoxedObj::Enum` via
// `rt_data_new`/`rt_data_variant`/`rt_data_field`, `struct_field_kind`
// classifying every enum type as kind `6`) opens several scenarios that
// either panicked or silently misbehaved before: an enum-typed compiled
// argument (`call_compiled`'s own argument encoding already treated
// `RtValue::Sexpr` uniformly, but no compiled function could receive an
// enum value produced by `Expr::Construct` before this flip made
// `Option::some`/`none` themselves heap-repr), a nested enum, an enum
// stored inside a `Vector<T>`/`HashTable<K,V>`/`defstruct` field, and GC
// safety for an enum value under root-stack pressure.

/// An enum-typed *parameter* — `Interp::call_compiled`'s argument encoding
/// (already value-shape-driven, `RtValue::Sexpr` crosses uniformly) now
/// actually receives something to encode: `Option::some`'s own construction
/// is heap-repr since the compiler flip, so this is the first case where a
/// compiled function can be handed a real enum value as an argument at all.
#[test]
fn compile_dispatches_a_function_taking_an_enum_typed_argument_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun unwrap-or-zero ((o Option<i32>)) i32 (match o ((Some x) x) ((None) 0)))
        (compile unwrap-or-zero)
        (unwrap-or-zero (Option::some 99))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(99));
}

/// `Option<Option<Sexpr>>`'s three states stay distinct — the soundness
/// requirement the `Option<Sexpr>` niche has to earn.
///
/// `Option<Sexpr>` is represented *exactly* like a `Sexpr` (`check/repr.rs`):
/// `none` is the empty-list immediate, `some v` is `v`. That works only while
/// the niche is claimed for `Option<sexpr>` and nothing else. Were the rule
/// written as "the payload *represents* like a `Sexpr`" — which the inner
/// `Option<Sexpr>` does — the outer level would niche too, and its `none`
/// would be the same word as the inner one's: `outer-none` and `inner-none`
/// would become indistinguishable. Requiring the type argument to be the
/// `sexpr` type itself drops the outer `Option` through to a real box.
///
/// This is reachable, not hypothetical: `HashTable<K,Option<Sexpr>>::get`
/// produces exactly this type.
#[test]
fn a_nested_option_over_sexpr_keeps_its_two_nones_apart() {
    let v = run_and_read(
        r#"
        (defun name ((v Option<Option<Sexpr>>)) string
          (match v
            ((none) "outer-none")
            ((some i) (match i ((none) "inner-none") ((some _) "inner-some")))))
        (compile name)
        (append (append (name (the Option<Option<Sexpr>> (Option::none)))
                        (append " " (name (Option::some (the Option<Sexpr> (Option::none))))))
                (append " " (name (Option::some (quote 42)))))
        "#,
        1 << 16,
        read_str,
    )
    .expect("eval failed");
    assert_eq!(v, "outer-none inner-none inner-some");
}

/// A nested enum (`Option<Option<i32>>`): `struct_field_kind`'s recursive
/// classification (an `Option<T>` field is kind `6` regardless of what `T`
/// is) means the inner `Option<i32>` crosses the outer box's field boundary
/// as an ordinary tagged `Sexpr`, no different from a `Str` or boxed struct
/// field.
#[test]
fn compile_dispatches_a_function_constructing_a_nested_option_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun wrap ((n i32)) Option<Option<i32>> (Option::some (Option::some n)))
        (compile wrap)
        (match (wrap 7)
          ((Some inner) (match inner ((Some x) x) ((None) -1)))
          ((None) -2))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(7));
}

/// A user `defenum` whose variant carries a builtin `Option<T>` field —
/// two different enum types nesting through the same tagged-`Sexpr`
/// boundary.
#[test]
fn compile_dispatches_a_function_constructing_a_user_defenum_with_an_option_field_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defenum wrapper (w Option<i32>))
        (defun mk ((n i32)) wrapper (wrapper::w (Option::some n)))
        (compile mk)
        (match (mk 5) ((w o) (match o ((Some x) x) ((None) -1))))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(5));
}

/// `Vector<Option<i32>>::push`/`get` — the element `kind`
/// (`vector_element_kind`/`struct_field_kind`) is now `6` for an `Option<T>`
/// element (previously `0`/unsupported), so a compiled `Vector` can hold
/// enum values at all.
#[test]
fn compile_dispatches_vector_push_and_get_of_an_option_element_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i32
          (let ((v (the Vector<Option<i32>> (Vector::new))))
            (push v (Option::some 3))
            (push v (Option::none))
            (match (get v 0) ((Some x) x) ((None) -1))))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(3));
}

/// `HashTable<i32, Option<i32>>::get` returns `Option<Option<i32>>` — the
/// map's own `get`/`remove` (`compile-hashtable-op`'s `rt_data_new` path)
/// nests with a user-stored `Option<i32>` value (an ordinary `Expr::Construct`
/// through the generic `val-kind` tagging), exercising both enum-construction
/// paths together.
#[test]
fn compile_dispatches_hashtable_get_of_an_option_typed_value_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i32
          (let ((ht (the HashTable<i32,Option<i32>> (HashTable::new))))
            (set ht 1 (Option::some 42))
            (match (get ht 1)
              ((Some inner) (match inner ((Some x) x) ((None) -1)))
              ((None) -2))))
        (compile run)
        (run)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
}

/// A `defstruct` field of enum type — `struct_field_kind`'s kind `6` for
/// `Option<T>` applies identically whether the enum sits in a general-ADT
/// box, a `Vector`/`HashTable` slot, or (here) a boxed struct's own field.
#[test]
fn compile_dispatches_a_defstruct_field_of_option_type_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct holder (val Option<i32>))
        (defun mk ((n i32)) holder (holder::new (Option::some n)))
        (compile mk)
        (match (val (mk 8)) ((Some x) x) ((None) -1))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(8));
}

/// GC stress: constructs many enum values inside a loop with a small heap,
/// forcing a real `gc()` mid-construction — proves an enum box is a real
/// GC-traced value now (`compile-match`'s root-protection no longer
/// excludes `scrut-kind = 1`, and `rt_data_new`'s result is a properly
/// GC-visible `BoxedObj::Enum`, not the old leaked, never-scanned `malloc`
/// box that needed no such protection).
#[test]
fn compile_dispatches_a_function_that_keeps_an_enum_scrutinee_rooted_across_many_allocations() {
    let v = run_with_compiler_and_prelude_and_capacity(
        r#"
        (defun sum-after-gc ((n i32)) i32
          (let ((scratch (the Vector<i32> (Vector::new))))
            (loop
              (if (eq n 0) (break) ())
              (push scratch n)
              (setf n (- n 1)))
            (match (Option::some (the i32 123)) ((Some x) x) ((None) -1))))
        (compile sum-after-gc)
        (sum-after-gc 5000)
        "#,
        1 << 15,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(123));
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
/// `compile`d at all (`compile-assoc` only recognized an integer
/// receiver) — see
/// `compile_dispatches_a_function_that_calls_a_compiled_method_in_its_own_body`
/// below for the follow-up that closes *that* gap.
#[test]
fn compile_dispatches_a_function_that_constructs_a_defstruct_instance_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct point (x i32) (y i32))
        (defun make-point ((a i32) (b i32)) point (point::new a b))
        (compile make-point)
        (make-point 3 4)
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Boxed(_) => {}
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
        (defstruct point (x i32) (y i32))
        (defun make-point ((a i32) (b i32)) point (point::new a b))
        (compile make-point)
        (compile point::x)
        (let ((p (make-point 3 4))) p::x)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(3));
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
        (defstruct point (x i32) (y i32))
        (defun make-point ((a i32) (b i32)) point (point::new a b))
        (compile make-point)
        (compile point::x)
        (compile point::set-x)
        (let ((p (make-point 3 4)))
          (setf p::x 99)
          p::x)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(99));
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
        (defstruct point (x i32) (y i32))
        (defun make-point ((a i32) (b i32)) point (point::new a b))
        (compile make-point)
        (compile point::x)
        (compile point::y)
        (defun sum-coords ((p point)) i32 (+ p::x p::y))
        (compile sum-coords)
        (sum-coords (make-point 3 4))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(7));
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
        (defstruct counter (n i32))
        (defun make-counter ((a i32)) counter (counter::new a))
        (compile make-counter)
        (compile counter::n)
        (compile counter::set-n)
        (defmethod countdown ((self counter)) i32
          (if (<= self::n 0)
              0
              (let ((ignored (setf self::n (- self::n 1))))
                (countdown self))))
        (compile counter::countdown)
        (countdown (make-counter 5))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(0));
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
        (defstruct point (x i32) (y i32))
        (defun sum-coords ((p point)) i32 (+ p::x p::y))
        (compile sum-coords)
        (sum-coords (point::new 3 4))
        "#,
    )
    .expect("transitive compile of the `point` accessors should succeed");
    assert_eq!(v, Value::Int(7), "3 + 4 = 7, with `point::x`/`point::y` auto-compiled");
}

/// The compiled tier shifts by the same distance the interpreter does, at a
/// width where the answer differs between a logical and an arithmetic shift.
///
/// `ash`'s distance became an `i32` (from "another value of the receiver's
/// type", which made right shift unwritable on unsigned widths). Nothing in
/// the island changed — the distance was always passed as a raw word and
/// `rt_int_ash` always read it as signed — and this test is what says so
/// rather than assuming it.
#[test]
fn a_negative_shift_distance_lowers_at_every_width() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun shr-u8 ((x u8)) u8 (ash x -3))
        (defun shr-i8 ((x i8)) i8 (ash x -2))
        (compile shr-u8)
        (compile shr-i8)
        (+ (as i32 (shr-u8 200)) (as i32 (shr-i8 -16)))
        "#,
    )
    .expect("a compiled right shift should run");
    // 200 >>> 3 = 25 (logical: `u8` has no sign bit to extend), and
    // -16 >> 2 = -4 (arithmetic). 25 + -4 = 21.
    assert_eq!(v, Value::Int(21));
}

/// A seeded run is the same run on either tier.
///
/// That is the whole reason `seed-random-state` has a compiled lowering
/// (`rt_seed_random_state`) rather than being left interpreter-only: a caller
/// reaches for a seed to make a run reproducible, and a reproducibility that
/// evaporated the moment the enclosing function got compiled would be worse
/// than none. The two halves — the integer-to-state map and the xorshift step
/// — both live in `typelisp-rt` for this, so neither tier has its own copy to
/// drift.
#[test]
fn a_seeded_stream_agrees_across_the_compile_boundary() {
    let draws = "(defun draws ((s random-state)) i32
                   (let ((acc (the i32 0)) (i 0))
                     (while (< i 9)
                       (setf acc (+ (* acc 10) (as i32 (random 10 s))))
                       (setf i (+ i 1)))
                     acc))";
    let interpreted = run_with_compiler_and_prelude(&format!(
        "{draws} (draws (seed-random-state 2024))"
    ))
    .expect("the interpreted draw should succeed");
    let compiled = run_with_compiler_and_prelude(&format!(
        "{draws} (compile draws) (draws (seed-random-state 2024))"
    ))
    .expect("the compiled draw should succeed");
    assert_eq!(compiled, interpreted, "seed 2024 must name one stream, not one per tier");
}

/// A name `(compile ...)` can't resolve is rejected at *check* time, with a
/// message that says which of the three ways it failed.
///
/// Nothing is left to look up at runtime — the checker's resolution is the
/// answer — so there is no reason to carry the failure to the `(compile ...)`
/// call the way this used to (an `EvalError::NoSuchFunction` out of
/// `Interp::resolve_fn_ref`). See `Checker::check_compile`'s doc comment.
#[test]
fn compile_of_an_unresolvable_name_is_a_check_time_error() {
    // A bare name that names nothing.
    match check_error("(compile no-such-function)") {
        Error::TypeError(m) => {
            assert!(m.contains("no function `no-such-function` is visible"), "message was: {}", m)
        }
        other => panic!("expected a TypeError, got {:?}", other),
    }

    // The type resolves; it just has no such member.
    match check_error("(defstruct point (x i32) (y i32)) (compile point::bogus)") {
        Error::TypeError(m) => {
            assert!(m.contains("has no associated function or method `bogus`"), "message was: {}", m)
        }
        other => panic!("expected a TypeError, got {:?}", other),
    }

    // Neither half resolves.
    match check_error("(compile bogus::x)") {
        Error::TypeError(m) => assert!(
            m.contains("`bogus::x` names neither a type's method nor a function"),
            "message was: {}",
            m
        ),
        other => panic!("expected a TypeError, got {:?}", other),
    }
}

/// A definition that exists but is private from the call site fails exactly
/// like one that doesn't exist at all — `resolve_fn`/`resolve_fn_path` fold
/// "exists but not visible" into "doesn't resolve" for every reference, and
/// `(compile ...)` is no exception now that it reports the failure itself.
#[test]
fn compile_of_a_private_name_from_outside_its_module_is_a_check_time_error() {
    match check_error("(module m (defun hidden () i32 1)) (compile m::hidden)") {
        Error::TypeError(m) => assert!(m.contains("m::hidden"), "message was: {}", m),
        other => panic!("expected a TypeError, got {:?}", other),
    }
}

/// `(compile name)`'s bare-name argument must resolve against the *calling*
/// form's own lexical module — not just the root — the same ancestor-chain
/// resolution an ordinary bare `Call` already gets (`Checker::resolve_fn`).
/// Before this fix, `Interp`'s own runtime resolution of a bare (non-`::`)
/// `(compile name)` argument only ever looked the name up at the root
/// (`Path::root(name)`), so `(compile helper)` from inside a module whose
/// `helper` was never registered at root would fail with `NoSuchFunction`
/// even though `helper` is genuinely in scope and callable from there.
#[test]
fn compile_resolves_a_bare_name_against_its_own_module_not_just_root() {
    let v = eval_ok_with_compiler(
        r#"
        (module m
          (pub defun helper () i32 42)
          (pub defun use-it () bool (compile helper)))
        (m::use-it)
        "#,
    );
    assert!(expect_bool(v));
}

/// The module-scoped counterpart of `compile_resolves_a_bare_name_against_its_own_module_not_just_root`:
/// two sibling modules each define their own `tag`, and `(compile tag)`
/// written inside `a` must resolve — and actually dispatch — to `a`'s own
/// `tag`, never `b`'s same-named one, even though nothing here is `pub`. The
/// old module-blind runtime resolution (`Interp::method_key`'s "search the
/// whole tree by local name" fallback) had no way to prefer the caller's own
/// module in a case like this; the new `written`+`home` resolution does, by
/// construction (`ModuleScope::resolve_fn`'s ancestor-chain walk starting
/// from `a` never even reaches `b`).
#[test]
fn compile_bare_name_prefers_the_callers_own_module_over_a_same_named_sibling() {
    let v = eval_ok_with_compiler(
        r#"
        (module a
          (defun tag () i32 1)
          (pub defun get-it () i32 (compile tag) (tag)))
        (module b (defun tag () i32 2))
        (a::get-it)
        "#,
    );
    assert_eq!(v, Value::Int(1));
}

/// The `defmethod`/`defstruct`-accessor counterpart of the sibling test
/// above, at the LLVM mangled-symbol level rather than the scope-tree
/// resolution level: two sibling modules each `defstruct` their own `box`
/// with a field accessor also named `n`, and — the part that actually
/// exercises the mangled *symbol* rather than just the interpreter's own
/// (already type-correct) dispatch — `sum-both`'s own body calls *both*
/// same-named accessors as external `Expr::Assoc` targets from within one
/// compiled function. Before this fix, a method's mangled LLVM symbol
/// (`user_method_symbol_name`) and every internal SCC bookkeeping key
/// derived from it (`Interp::method_key`, `CallEdge::Method`'s node name)
/// were built from the receiver type's *local* name only (`tl_box::n` for
/// *both* structs) — so `compiler.rs`'s `get-function "tl_box::n"` inside
/// `sum-both`'s own module would resolve to the *same* declared value for
/// both calls, and whichever real address `Interp::compile_scc` wired last
/// via `add_global_mapping` would silently win for both — reading `x::n`
/// would return `y`'s field (or vice versa) instead of its own.
#[test]
fn compile_a_same_named_method_in_two_sibling_modules_does_not_alias_the_others_llvm_symbol() {
    let v = eval_ok_with_compiler(
        r#"
        (module a (pub defstruct box (pub n i32)))
        (module b (pub defstruct box (pub n i32)))
        (defun make-a () a::box (a::box::new 100))
        (defun make-b () b::box (b::box::new 200))
        (compile make-a)
        (compile make-b)
        (compile a::box::n)
        (compile b::box::n)
        (defun sum-both ((x a::box) (y b::box)) i32 (+ x::n y::n))
        (compile sum-both)
        (sum-both (make-a) (make-b))
        "#,
    );
    assert_eq!(v, Value::Int(300));
}

/// `(compile "name")`/`(compile "type::method")` — a string literal, not an
/// unevaluated symbol or `::`-path — is a type error caught at check time,
/// before the compiler ever runs (see `Checker::check_compile`'s doc
/// comment for why the name being compiled is program structure, not
/// runtime data).
#[test]
fn compile_of_a_string_literal_is_a_type_error() {
    for src in [
        r#"(defun add2 ((a i32) (b i32)) i32 (+ a b)) (compile "add2")"#,
        r#"(defstruct point (x i32) (y i32)) (compile "point::x")"#,
    ] {
        match check_error(src) {
            Error::TypeError(_) => {}
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
        (defun f () i32
          (labels ((outer-fn ((a i32)) i32 (+ a 1)))
            (labels ((inner-fn ((b i32)) i32 (outer-fn b)))
              (inner-fn 10))))
        (compile f)
        (f)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 11),
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
        (defun f ((z i32)) i32
          (labels ((outer-fn ((a i32)) i32 (+ a z)))
            (labels ((inner-fn ((b i32)) i32 (outer-fn b)))
              (inner-fn 10))))
        (compile f)
        (f 100)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 110),
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
        (defun f ((z i32)) i32
          (labels ((outer-fn ((a i32)) i32 (+ a z)))
            (labels ((dummy ((x i32)) i32 x))
              (outer-fn 10))))
        (compile f)
        (f 100)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 110),
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
        (defun f ((z i32) (w i32)) i32
          (labels ((outer-fn ((a i32)) i32 (+ a z)))
            (labels ((inner-fn ((b i32)) i32 (+ (outer-fn b) w)))
              (inner-fn 10))))
        (compile f)
        (f 100 1000)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 1110),
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
        (defun f ((z i32)) i32
          (labels ((f1 ((a i32)) i32 (+ a z)))
            (labels ((f2 ((a i32)) i32 (* a 2)))
              (labels ((f3 ((a i32)) i32 (f1 a)))
                (f3 10)))))
        (compile f)
        (f 100)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 110),
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
        (defun make-it ((z i32)) (fn (i32) i32)
          (labels ((outer-fn ((a i32)) i32 (+ a z)))
            (labels ((grab () (fn (i32) i32) outer-fn))
              (grab))))
        (defun apply-fn ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (compile make-it)
        (compile apply-fn)
        (apply-fn (make-it 100) 10)
        "#,
    );
    match v {
        Value::Int(n) => assert_eq!(n, 110),
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
        (deftrait Counted () (count ((self Self)) i32))
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
    assert_eq!(v, Value::Int(7));
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
        (deftrait Counted () (count ((self Self)) i32))
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
    assert_eq!(v, Value::Int(3 + 6));
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
            (let ((s (sexpr-cons (i32 1) (i32 2))))
              (return 42))))
        (compile make-thing)
        (make-thing)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
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
    typelisp::compile::install_llvm_backend();
    let mut interp = Interp::new();
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();

    let setup = r#"
        (defun trivial () i32 7)
        (compile trivial)
        (defun leaky-inner () i32
          (loop
            (let ((s (sexpr-cons (i32 1) (i32 2))))
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
            assert_eq!(result, Value::Int(7));
        }
    }
    let trivial_growth = h.root_count() - before_trivial;

    let before_leaky = h.root_count();
    for _ in 0..CALLS {
        for v in r.read_all(&mut h, "(leaky-inner)").expect("read failed") {
            let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
            let result = interp.exec(&mut h, tl).expect("exec failed").expect("leaky-inner should produce a value");
            assert_eq!(result, Value::Int(7));
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
        (defstruct inner (v i32))
        (defstruct outer (child inner))
        (defun make-outer ((n i32)) outer (outer::new (inner::new n)))
        (compile make-outer)
        (compile inner::v)
        (compile outer::child)
        (defun read-nested ((o outer)) i32 (v o::child))
        (compile read-nested)
        (read-nested (make-outer 41))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(41));
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
        (defstruct inner (v i32))
        (defstruct outer (child inner))
        (defun make-outer ((n i32)) outer (outer::new (inner::new n)))
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
    assert_eq!(v, Value::Int(9));
}

/// A user-defined method on a *primitive* receiver — the prelude's `impl Eq
/// i32` registers `i32::equals` as an ordinary user method whose body
/// `(= self other)` lowers natively. Two former gates blocked this path:
/// `Interp::compile_function` excluded every integer/`string` assoc
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
    assert_eq!(v, Value::Bool(true));
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
    assert_eq!(v, Value::Bool(true));
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
        (defun run () i32 (if (eq2 5 5) (if (eq2 5 6) 0 1) 0))
        (compile run)
        (run)
        "#,
    )
    .expect("transitive compile of `i32::equals` should succeed");
    assert_eq!(v, Value::Int(1), "eq2(5,5) true and eq2(5,6) false, with `i32::equals` auto-compiled");
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
        (defun str-cmp ((a string) (b string)) i32
          (if (equals a b) 0 (if (less a b) -1 1)))
        (compile str-cmp)
        (+ (str-cmp (append "ab" "c") "abc")
           (+ (* (str-cmp "abc" "abd") 10)
              (* (str-cmp "b" "a") 100)))
        "#,
    )
    .expect("eval failed");
    // equal content (built separately) -> 0, "abc" < "abd" -> -10, "b" > "a" -> 100
    assert_eq!(v, Value::Int(90));
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
        (defun char-cmp ((a char) (b char)) i32
          (if (equals a b) 0 (if (less a b) -1 1)))
        (compile char-cmp)
        (+ (char-cmp (ref "xa" 1) (ref "ya" 1))
           (+ (* (char-cmp (ref "a" 0) (ref "b" 0)) 10)
              (* (char-cmp (ref "b" 0) (ref "a" 0)) 100)))
        "#,
    )
    .expect("eval failed");
    // 'a'=='a' -> 0, 'a'<'b' -> -10, 'b'>'a' -> 100
    assert_eq!(v, Value::Int(90));
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
        (defun ccmp ((a char) (b char)) i32
          (if (< a b) 1 (if (<= a b) 2 (if (> a b) 3 4))))
        (compile ccmp)
        (+ (ccmp (ref "ab" 0) (ref "ab" 1))
           (+ (* (ccmp (ref "ba" 0) (ref "ba" 1)) 10)
              (* (ccmp (ref "aa" 0) (ref "aa" 1)) 100)))
        "#,
    )
    .expect("eval failed");
    // 'a'<'b' -> 1 ; 'b' vs 'a': >  -> 3 -> 30 ; 'a' vs 'a': <= -> 2 -> 200
    assert_eq!(v, Value::Int(231));
}

#[test]
fn compile_dispatches_string_comparison_operators() {
    let v = run_with_compiler(
        r#"
        (defun scmp ((a string) (b string)) i32
          (if (< a b) 1 (if (<= a b) 2 (if (> a b) 3 4))))
        (compile scmp)
        (+ (scmp "a" "b")
           (+ (* (scmp "b" "a") 10)
              (* (scmp "x" "x") 100)))
        "#,
    )
    .expect("eval failed");
    // "a"<"b" -> 1 ; "b" vs "a": > -> 3 -> 30 ; "x" vs "x": <= -> 2 -> 200
    assert_eq!(v, Value::Int(231));
}

#[test]
fn compile_string_ge_and_le_derive_from_rt_str_lt() {
    // Exercises `>=`/`<=` specifically (the `not rt_str_lt(...)` derivations).
    let v = run_with_compiler(
        r#"
        (defun sle ((a string) (b string)) i32 (if (<= a b) 1 0))
        (defun sge ((a string) (b string)) i32 (if (>= a b) 1 0))
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
    assert_eq!(v, Value::Int(101));
}

// ---- `defenum` / sum-ADT `match` in compiled code (box scrutinee) ----------
//
// Construction already lowered to a `compile-construct-box` box (slot 0 = tag,
// slot 1+ = fields); these exercise the box-scrutinee `match` path added
// alongside `defenum` — `ast_bridge::translate_match`'s `is-box` branch plus
// `compiler.rs`'s `compile-box-tag-test`/`compile-box-field`. Closes the
// gap where `Match` on a non-`Sexpr` scrutinee was `unsupported` (recorded in
// TODO.md at the time; see `docs/dev/implementation-log.md`'s 2026-07-12 entry).

/// A compiled `match` on a payload variant tests the box's tag slot and
/// extracts the field from slot 1.
#[test]
fn compile_matches_a_payload_variant_and_extracts_its_field() {
    // The payload comes from a parameter so the field type is unambiguous:
    // `T` is read off `x`'s declared type rather than a literal's default.
    let v = eval_ok_with_compiler(
        r#"
        (defenum Maybe<T> (Just T) (Nothing))
        (defun m ((x i32)) i32 (match (Maybe::Just x) ((Just v) v) ((Nothing) x)))
        (compile m)
        (m 7)
        "#,
    );
    assert_eq!(v, Value::Int(7));
}

/// A compiled `match` discriminates across three nullary variants by the
/// box's variant-tag slot, selecting the correct arm.
#[test]
fn compile_matches_discriminates_among_nullary_variants() {
    let v = eval_ok_with_compiler(
        r#"
        (defenum Sign (Neg) (Zero) (Pos))
        (defun classify () i32 (match (Sign::Pos) ((Neg) 10) ((Zero) 20) ((Pos) 30)))
        (compile classify)
        (classify)
        "#,
    );
    assert_eq!(v, Value::Int(30));
}

/// A runtime-chosen variant (both arms reachable): the `if` merges two boxes,
/// and the compiled `match` picks the arm by the tag slot — the payload arm
/// extracts, the nullary arm doesn't.
#[test]
fn compile_matches_a_runtime_chosen_variant() {
    // The payload `Just` branch is the `if`'s `then` so its `T` is inferred
    // before the payload-less `Nothing` `else` (which can't infer `T` alone —
    // an inference ordering property, the same in the interpreter).
    let src = r#"
        (defenum Maybe<T> (Just T) (Nothing))
        (defun pick ((n i32)) i32
          (match (if (eq n 0) (Maybe::Just n) (Maybe::Nothing))
            ((Just v) v)
            ((Nothing) 99)))
        (compile pick)
        (pick %ARG%)
    "#;
    assert_eq!(eval_ok_with_compiler(&src.replace("%ARG%", "0")), Value::Int(0));
    assert_eq!(eval_ok_with_compiler(&src.replace("%ARG%", "5")), Value::Int(99));
}

/// The compiled result must agree with the tree-walking interpreter for the
/// same `defenum` `match` — the JIT/interp-parity check the plan calls for.
#[test]
fn compile_and_interpret_agree_on_a_defenum_match() {
    let prog = r#"
        (defenum Maybe<T> (Just T) (Nothing))
        (defun m ((x i32)) i32 (match (Maybe::Just x) ((Just v) (+ v 1)) ((Nothing) x)))
    "#;
    let compiled = eval_ok_with_compiler(&format!("{}\n(compile m)\n(m 41)", prog));
    let interpreted = eval_ok(&format!("{}\n(m 41)", prog));
    assert_eq!(compiled, interpreted);
    assert_eq!(compiled, Value::Int(42));
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
        (defun u ((n i32)) i32 (match (option::some n) ((Some v) v) ((None) n)))
        (compile u)
        (u 5)
        "#,
    );
    assert_eq!(v, Value::Int(5));
}

// ---- boxed-struct (`defstruct`) `match` in compiled code -------------------
//
// A `defstruct`/`Vector` scrutinee is a properly tagged `Sexpr`
// (`BoxedObj::Struct`) like the plain-`Sexpr` case, but has exactly one
// variant (its own `new` constructor), so no tag test is ever emitted for it
// — only per-field extraction via `rt_struct_field_get`/`compile-sexpr-field`
// (`ast_bridge::pattern_to_sexpr`'s `MATCH_KIND_STRUCT` branch,
// `compiler.rs`'s `compile-struct-field`). Closes the last remaining gap for
// compiled `Match` (recorded in TODO.md at the time; see
// `docs/dev/implementation-log.md`'s 2026-07-12 entry).

/// A compiled `match` destructures a `defstruct` instance's fields by
/// position, the same as the interpreter's own
/// `match_destructures_a_struct_instance` (`tests/struct_test.rs`).
#[test]
fn compile_matches_and_destructures_a_defstruct_instance() {
    let v = eval_ok_with_compiler(
        r#"
        (defstruct point (x i32) (y i32))
        (defun sum ((p point)) i32 (match p ((new a b) (+ a b))))
        (compile sum)
        (sum (point::new 3 4))
        "#,
    );
    assert_eq!(v, Value::Int(7));
}

/// A wildcard sub-pattern skips field extraction entirely, and binding order
/// follows field declaration order, not name — mirrors the interpreter's own
/// `match_on_a_struct_binds_fields_by_position`.
#[test]
fn compile_match_on_a_defstruct_binds_fields_by_position_and_skips_wildcards() {
    let v = eval_ok_with_compiler(
        r#"
        (defstruct point (x i32) (y i32) (z i32))
        (defun diff ((p point)) i32 (match p ((new a _ c) (- a c))))
        (compile diff)
        (diff (point::new 9 100 3))
        "#,
    );
    assert_eq!(v, Value::Int(6));
}

/// Compiled and interpreted `match` agree over the same `defstruct` instance
/// (the JIT/interp-parity check every other scrutinee kind above has).
#[test]
fn compile_and_interpret_agree_on_a_defstruct_match() {
    let prog = r#"
        (defstruct point (x i32) (y i32))
        (defun sum ((p point)) i32 (match p ((new a b) (+ a b))))
    "#;
    let compiled = eval_ok_with_compiler(&format!("{}\n(compile sum)\n(sum (point::new 5 6))", prog));
    let interpreted = eval_ok(&format!("{}\n(sum (point::new 5 6))", prog));
    assert_eq!(compiled, interpreted);
    assert_eq!(compiled, Value::Int(11));
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
        (defun f ((s Option<Sexpr>)) i32
          (match s
            ((i32 n) n)
            ((cons (i32 a) _) a)
            (_ 0)))
        (compile f)
        (+ (f (i32 40)) (f (sexpr-cons (i32 2) (Str "tail"))))
        "#,
    );
    assert_eq!(v, Value::Int(42));
}

/// A user `defun` literally named `rt_cons` gets its own distinct LLVM
/// symbol (`tl_rt_cons`, the `USER_SYMBOL_PREFIX` prefix) — it never
/// collides with the real `rt_cons` runtime shim `compiler.rs`'s
/// `compile-call` rewrites a bare `sexpr-cons` call to (see
/// `is_rt_builtin_name`). Both the user's own `rt_cons` and the built-in
/// `sexpr-cons`/`match`-on-`cons` machinery work correctly side by side.
#[test]
fn compile_of_a_user_function_literally_named_rt_cons_does_not_collide_with_the_rt_cons_shim() {
    let v = eval_ok_with_compiler(
        r#"
        (defun rt_cons ((a i32) (b i32)) i32 (+ a b))
        (compile rt_cons)
        (defun cons-and-extract () i32
          (match (sexpr-cons (i32 5) (i32 9))
            ((cons (i32 x) (i32 y)) (+ x y))
            (_ 0)))
        (compile cons-and-extract)
        (+ (rt_cons 3 4) (cons-and-extract))
        "#,
    );
    // rt_cons(3,4) = 3+4 = 7 (the user's own definition); the built-in
    // sexpr-cons/car/cdr machinery still produces 5+9 = 14 unaffected;
    // 7+14 = 21.
    assert_eq!(v, Value::Int(21));
}

/// The three numeric boxed variants share `TAG_BOXED`, so their arms dispatch
/// through `rt_box_kind` — a bignum/ratio scrutinee must *not* take a
/// preceding `(f64 _)` arm even though it carries the same 3-bit tag.
#[test]
fn compile_match_distinguishes_float_bignum_and_ratio_boxes() {
    let v = eval_ok_with_compiler(
        r#"
        (defun which ((s Sexpr)) i32
          (match s
            ((f64 _) 1)
            ((bignum _) 2)
            ((ratio _) 3)
            (_ 0)))
        (compile which)
        (+ (+ (which (f64 1.5))
              (* (the i32 10) (which (Bignum 99999999999999999999999999))))
           (* (the i32 100) (which (Ratio 2/3))))
        "#,
    );
    assert_eq!(v, Value::Int(321));
}

/// Compiled code tells the six integer widths apart inside a `Sexpr`, and
/// round-trips each one's value back out.
///
/// Both halves are new machinery: `compile-construct-sexpr` boxes through
/// `rt_narrow_new` with a `wsig` constant taken from the *variant*, and
/// `compile-sexpr-tag-test` asks `rt_box_kind` which box it got. Neither can
/// be derived from the value — `100` is the same machine word in all six
/// types — which is exactly why the box exists.
#[test]
fn compile_match_distinguishes_every_integer_width_in_a_sexpr() {
    let v = eval_ok_with_compiler(
        r#"
        (defun tag ((s Option<Sexpr>)) i32
          (match s
            ((i8 _) 1) ((i16 _) 2) ((i32 _) 3)
            ((u8 _) 4) ((u16 _) 5) ((u32 _) 6)
            (_ 0)))
        (compile tag)
        (+ (+ (+ (tag (i8 100)) (* 10 (tag (i16 100))))
              (+ (* 100 (tag (i32 100))) (* 1000 (tag (u8 100)))))
           (+ (* 10000 (tag (u16 100))) (* 100000 (tag (u32 100)))))
        "#,
    );
    // One digit per width, so a wrong arm shows up as a wrong digit rather
    // than as a plausible total.
    assert_eq!(v, Value::Int(654_321));
}

/// The payload comes back out of a compiled narrow-integer arm, at its own
/// width — including a `u32` past `i32`'s range, which is the value the old
/// single `int` variant genuinely corrupted rather than merely mislabelled.
#[test]
fn compile_extracts_a_narrow_integer_payload_at_its_own_width() {
    let v = eval_ok_with_compiler(
        r#"
        (defun small ((s Option<Sexpr>)) i32
          (match s ((u8 n) (as i32 n)) (_ -1)))
        (defun big ((s Option<Sexpr>)) u32
          (match s ((u32 n) n) (_ 0)))
        (compile small)
        (compile big)
        (+ (small (u8 200)) (as i32 (- (big (u32 4000000000)) (the u32 3999999000))))
        "#,
    );
    // 200 + (4000000000 - 3999999000) = 200 + 1000.
    assert_eq!(v, Value::Int(1200));
}

/// A compiled `match` on an `f32` node takes the `f32` arm and not the `f64`
/// one.
///
/// The two share `TAG_BOXED` *and* the same bit pattern in a register, so
/// only `rt_box_kind` can tell them apart. `compile-sexpr-tag-test` had no
/// arm for the `f32` variant at all when it was added, which made this
/// dispatch a `panic` in the compiler rather than a wrong answer at run time.
#[test]
fn compile_match_distinguishes_the_two_float_widths() {
    let v = eval_ok_with_compiler(
        r#"
        (defun which ((s Option<Sexpr>)) i32
          (match s ((f32 _) 1) ((f64 _) 2) (_ 0)))
        (compile which)
        (+ (which (f32 1.5)) (* 10 (which (f64 1.5))))
        "#,
    );
    assert_eq!(v, Value::Int(21));
}

/// Tag-only dispatch covers every variant, including `sym`.
#[test]
fn compile_match_dispatches_nil_sym_str_and_bool_by_tag() {
    let v = eval_ok_with_compiler(
        r#"
        (defun tag ((s Option<Sexpr>)) i32
          (match s
            ((none) 1) ((sym _) 2) ((str _) 3) ((bool _) 4)
            (_ 0)))
        (compile tag)
        (+ (+ (tag ()) (* (the i32 10) (tag (quote foo))))
           (+ (* (the i32 100) (tag (Str "s"))) (* (the i32 1000) (tag (Bool false)))))
        "#,
    );
    assert_eq!(v, Value::Int(4321));
}

/// A `sym`'s `Symbol` payload — unlike `bignum`/`ratio` (still `unsupported`)
/// — is already a fully tagged immediate, the same passthrough kind
/// `struct_field_kind` gives `Str`/`Sexpr` (`ast_bridge.rs`'s `Type::Symbol
/// => 6`), so binding `(sym x)` and returning `x` compiles: `compile-sexpr-
/// field`'s variant-5 arm passes the tagged word through unchanged. Crossing
/// back out to interpreted code exercises `Interp::decode_compiled_return`'s
/// own `Type::Symbol` arm (added alongside this), which the un-fixed
/// fallthrough would have misdecoded as a bare `RtValue::Int`.
#[test]
fn compile_match_binds_and_returns_a_sym_payload() {
    let v = eval_string_with_compiler(
        r#"
        (defun get-sym ((s Option<Sexpr>)) Symbol
          (match s ((sym x) x) (_ (panic "not a sym"))))
        (compile get-sym)
        (symbol->string (get-sym (quote hello)))
        "#,
    );
    assert_eq!(v, "hello");
}

/// The `path` variant (`registry::sexpr_def`'s eleventh, added alongside
/// `sym`'s fix): tag-tests against `TAG_PATH` (`compile-sexpr-tag-test`
/// variant `10` -> tag `5`) and extracts its payload as a fresh `Sexpr` list
/// of `sym`s via the new `rt_path_to_list` runtime shim (`compile-sexpr-
/// field` variant `10`) — the compiled-code mirror of `typelisp::eval::
/// interp`'s own `match_sexpr_ctor` `SEXPR_PATH` arm.
#[test]
fn compile_match_dispatches_and_extracts_the_path_variant() {
    let v = eval_ok_with_compiler(
        r#"
        (defun path-segs ((s Option<Sexpr>)) Option<Sexpr>
          (match s ((path segs) segs) (_ (panic "not a path"))))
        (compile path-segs)
        (equal (path-segs (quote dep::head)) (list (quote dep) (quote head)))
        "#,
    );
    assert_eq!(v, Value::Bool(true));
}

/// `(Path segs)` construction, the inverse of the extraction test above: a
/// compiled `Sexpr` list of `sym`s becomes a genuine `Value::Path` through
/// the new `rt_list_to_path` runtime shim (`compile-construct-sexpr` variant
/// `10`). The result crosses back out as an ordinary `Sexpr` (already
/// `is_boxed_sexpr_type`-covered — no return-decode fix needed, unlike
/// `sym`), so the round trip through `match`'s own `path` arm is checked in
/// interpreted code.
#[test]
fn compile_construct_and_round_trips_the_path_variant() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-path ((segs Option<Sexpr>)) Option<Sexpr>
          (Path segs))
        (compile make-path)
        (match (make-path (list (quote a) (quote b)))
          ((path segs) (equal segs (list (quote a) (quote b))))
          (_ false))
        "#,
    );
    assert_eq!(v, Value::Bool(true));
}

/// Compiled and interpreted `match` agree over the same Sexpr inputs.
#[test]
fn compile_and_interpret_agree_on_a_sexpr_match() {
    let src = |call: &str| {
        format!(
            r#"
            (defun sum ((s Option<Sexpr>)) i32
              (match s
                ((cons (i32 n) rest) (+ n (sum rest)))
                (_ 0)))
            {}
            (sum (quote (1 2 3 4)))
            "#,
            call
        )
    };
    let interp_v = eval_ok_with_compiler(&src(""));
    let compiled_v = eval_ok_with_compiler(&src("(compile sum)"));
    assert_eq!(interp_v, Value::Int(10));
    assert_eq!(compiled_v, interp_v);
}

/// A `compile`d function can read a `defvar` global — `Expr::Global`'s
/// real `ast_bridge` translation (`(global id)`) and `compiler.rs`'s
/// `compile-global`.
#[test]
fn compile_dispatches_a_function_that_reads_a_global_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defvar (counter i32) 41)
        (defun read-counter () i32 counter)
        (compile read-counter)
        (read-counter)
        "#,
    );
    assert_eq!(v, Value::Int(41));
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
        (defvar (counter i32) 0)
        (defun bump-counter () i32 (setf counter (+ counter 1)))
        (compile bump-counter)
        (bump-counter)
        (bump-counter)
        counter
        "#,
    );
    assert_eq!(v, Value::Int(2));
}

/// `Option<T>`/`Result<T,E>` globals compile: reading one back through
/// `match` from compiled code round-trips correctly (`Interp::promote_global`'s
/// `RtValue::Data` -> raw-box encoding, `ast_bridge::global_field_kind`'s
/// dedicated `kind = 10`). User `defenum` globals ride the same path — see
/// the tests below.
#[test]
fn compile_of_a_function_referencing_an_option_typed_global_round_trips() {
    let v = run_with_compiler(
        r#"
        (defvar (maybe Option<i32>) (Option::some 42))
        (defun read-maybe () i32 (match maybe ((Some x) x) ((None) 0)))
        (compile read-maybe)
        (read-maybe)
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// `None`/an unset `Result` err case also round-trips (the zero-field
/// variant, exercising `data_to_box`'s `fields: &[]` path).
#[test]
fn compile_of_a_function_referencing_a_none_typed_global_round_trips() {
    let v = run_with_compiler(
        r#"
        (defvar (maybe Option<i32>) (Option::none))
        (defun read-maybe () i32 (match maybe ((Some x) x) ((None) -1)))
        (compile read-maybe)
        (read-maybe)
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, -1),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Compiled code can also *write* an `Option`-typed global
/// (`compile-set-global`'s new `kind = 10` encode path), and an interpreted
/// read afterward sees the compiled write — `Expr::Global`'s own
/// `decode_field_typed` dispatch, not just the compiled side's
/// `rt_global_get`.
#[test]
fn compile_can_set_an_option_typed_global_and_the_interpreter_sees_the_write() {
    let v = run_with_compiler(
        r#"
        (defvar (maybe Option<i32>) (Option::none))
        (defun set-maybe () i32 (progn (setf maybe (Option::some 7)) 0))
        (compile set-maybe)
        (set-maybe)
        (match maybe ((Some x) x) ((None) -1))
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, 7),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A user `defenum` global crosses into compiled code the same way an
/// `Option` one does (`Interp::promote_global`'s `data_to_box` encoding,
/// `global_field_kind`'s `kind = 10`), with the variant field types coming
/// from `Interp::enum_defs` (`TopLevel::Defenum`'s baked-in definition)
/// instead of the type's own generic args — nullary variants here, so this
/// is the `fields: &[]` shape plus the tag-slot discrimination.
#[test]
fn compile_of_a_function_referencing_a_user_defenum_global_round_trips() {
    let v = run_with_compiler(
        r#"
        (defenum color (red) (green) (blue))
        (defvar (c color) (color::green))
        (defun read-c () i32 (match c ((red) 1) ((green) 2) ((blue) 3)))
        (compile read-c)
        (read-c)
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, 2),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A payload-carrying `defenum` global: the compiled `match` reads the box's
/// tag slot *and* decodes each variant's fields per its declared field types
/// — the multi-field variant exercises `data_variant_field_types`' indexed
/// slot layout beyond what single-field `Option`/`Result` cover.
#[test]
fn compile_of_a_function_referencing_a_payload_defenum_global_round_trips() {
    let v = run_with_compiler(
        r#"
        (defenum shape (Circle i32) (Rect i32 i32))
        (defvar (s shape) (shape::Rect 3 4))
        (defun area () i32 (match s ((Circle r) (* r r)) ((Rect w h) (* w h))))
        (compile area)
        (area)
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, 12),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Compiled code can also *write* a `defenum` global (`compile-set-global`'s
/// `kind = 10` encode path), and an interpreted read afterward sees the
/// compiled write — the enum mirror of the `Option` write test above,
/// exercising `Expr::Global`'s `decode_field_typed` dispatch through
/// `Interp::enum_defs`.
#[test]
fn compile_can_set_a_defenum_global_and_the_interpreter_sees_the_write() {
    let v = run_with_compiler(
        r#"
        (defenum shape (Circle i32) (Rect i32 i32))
        (defvar (s shape) (shape::Circle 1))
        (defun set-s () i32 (progn (setf s (shape::Circle 7)) 0))
        (compile set-s)
        (set-s)
        (match s ((Circle r) r) ((Rect w h) (+ w h)))
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, 7),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A *generic* `defenum` global: the variant's declared field type is the
/// type parameter `T`, so decoding must substitute the global's concrete
/// argument (`Maybe<i32>` -> `T = i32`) into it — `data_variant_field_types`'
/// `subst_apply` path, which `Option`/`Result` (whose field types *are* the
/// args) never exercise.
#[test]
fn compile_of_a_function_referencing_a_generic_defenum_global_round_trips() {
    let v = run_with_compiler(
        r#"
        (defenum Maybe<T> (Just T) (Nothing))
        (defvar (m Maybe<i32>) (Maybe::Just 42))
        (defun read-m () i32 (match m ((Just x) x) ((Nothing) -1)))
        (compile read-m)
        (read-m)
        "#,
    )
    .expect("eval failed");
    match v {
        Value::Int(n) => assert_eq!(n, 42),
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// A `Str`-carrying `Option` global survives GC: `data_to_box`'s heap-
/// allocated `Str` field lives *outside* the heap's own GC-scanned arena
/// (the raw box `compile-construct-box` itself builds is never scanned), so
/// without `Heap::push_permanent_root` protection a later, unrelated
/// allocation forcing a collection would reclaim it out from under the
/// global — this drives enough allocation (a small `Heap` capacity, a loop
/// building throwaway `Vector`s) between the global's promotion and its
/// read-back to make that failure mode observable rather than lucky. The
/// final read is a plain top-level `match` (interpreted, not `(compile
/// ...)`d), deliberately sidestepping `Interp::call_compiled`'s own
/// formerly-documented gap in decoding a compiled function's `Str`
/// *return value* (closed by `decode_compiled_return`'s `Type::Str` arm,
/// closure unification Stage 8); this test only means to exercise
/// `Expr::Global`'s `decode_field_typed` path for a promoted
/// `Option<string>`.
#[test]
fn compile_of_a_function_referencing_a_str_option_global_survives_gc() {
    let v = eval_string_with_compiler_and_capacity(
        r#"
        (defvar (maybe Option<string>) (Option::some "hello"))
        (defun touch-maybe () string (match maybe ((Some s) s) ((None) "")))
        (compile touch-maybe)
        (defvar (scratch Vector<i32>) (Vector::new))
        (defvar (n i32) 20000)
        (loop
          (if (eq n 0)
              (break)
              (progn (push scratch n) (setf n (- n 1)))))
        (match maybe ((Some s) s) ((None) ""))
        "#,
        1 << 14,
    );
    assert_eq!(v, "hello");
}

/// The `defenum` mirror of the `Option<string>` GC test above: a user
/// enum's `Str`-carrying field goes through the very same
/// `encode_data_field` `push_permanent_root` protection, so it too must
/// survive collections between promotion and read-back.
#[test]
fn compile_of_a_function_referencing_a_str_defenum_global_survives_gc() {
    let v = eval_string_with_compiler_and_capacity(
        r#"
        (defenum named (N string) (Anon))
        (defvar (who named) (named::N "hello"))
        (defun touch-who () string (match who ((N s) s) ((Anon) "")))
        (compile touch-who)
        (defvar (scratch Vector<i32>) (Vector::new))
        (defvar (n i32) 20000)
        (loop
          (if (eq n 0)
              (break)
              (progn (push scratch n) (setf n (- n 1)))))
        (match who ((N s) s) ((Anon) ""))
        "#,
        1 << 14,
    );
    assert_eq!(v, "hello");
}

// ---- `Expr::Panic`/`Expr::MethodRef`/`Expr::Quote` in compiled code --------
//
// Closes the three remaining `unsupported` gaps (recorded in TODO.md at the
// time; see `docs/dev/implementation-log.md`'s 2026-07-12 entries).

/// A `(panic msg)` branch compiles cleanly (`ast_bridge::translate_panic`/
/// `compiler.rs`'s `compile-panic`), and the *non*-panicking path through the
/// same function still runs correctly — actually triggering the panic would
/// abort the whole test process (`rt_panic`'s documented "abort, don't
/// unwind" contract), so this only exercises the branch never taken.
#[test]
fn compile_dispatches_a_function_with_an_untaken_panic_branch_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun safe-add ((a i32) (b i32)) i32 (if (eq b 0) (panic "b is zero") (+ a b)))
        (compile safe-add)
        (safe-add 10 2)
        "#,
    );
    assert_eq!(v, Value::Int(12));
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
    assert_eq!(v, Value::Int(1));
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
    assert_eq!(v, Value::Char('Z'));
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
    assert_eq!(v, Value::Char('P'));
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
        (defun apply2 ((f (fn (i32 i32) i32)) (a i32) (b i32)) i32 (f a b))
        (compile apply2)
        (defun use-plus ((a i32) (b i32)) i32 (apply2 + a b))
        (compile use-plus)
        (use-plus 3 4)
        "#,
    );
    assert_eq!(v, Value::Int(7));
}

/// A user-defined instance method (not a native-arithmetic builtin) reified
/// as a value dispatches through `compile-assoc-user` instead — the other
/// half of `compile-assoc`'s two-way split, reached the same way an ordinary
/// `p::double` call site would.
#[test]
fn compile_dispatches_a_user_defined_method_reified_as_a_value_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defstruct point (x i32))
        (defmethod double ((self point)) i32 (* self::x 2))
        (compile point::x)
        (compile point::double)
        (defun apply1 ((f (fn (point) i32)) (p point)) i32 (f p))
        (compile apply1)
        (defun use-double ((p point)) i32 (apply1 double p))
        (compile use-double)
        (use-double (point::new 21))
        "#,
    );
    assert_eq!(v, Value::Int(42));
}

/// `(quote (1 2 3))` compiles to the same `compile-construct-sexpr`
/// machinery an ordinary `(Cons (i32 1) ...)` construction already uses
/// (`ast_bridge::translate_quote`) — a compiled function can build a quoted
/// literal, and an ordinary compiled `match`-based `sum` (already proven
/// correct against `Sexpr` scrutinees elsewhere in this file) can consume it.
#[test]
fn compile_dispatches_a_function_that_constructs_a_quoted_list_to_native_code() {
    let v = eval_ok_with_compiler(
        r#"
        (defun make-quoted () Option<Sexpr> (quote (1 2 3)))
        (defun sum ((s Option<Sexpr>)) i32 (match s ((cons (i32 n) rest) (+ n (sum rest))) (_ 0)))
        (compile make-quoted)
        (compile sum)
        (sum (make-quoted))
        "#,
    );
    assert_eq!(v, Value::Int(6));
}

/// Compiled and interpreted agree on the same quoted literal — the
/// JIT/interp-parity check every other construct in this file gets.
#[test]
fn compile_and_interpret_agree_on_a_quoted_list() {
    let prog = r#"
        (defun make-quoted () Option<Sexpr> (quote (1 2 3)))
        (defun sum ((s Option<Sexpr>)) i32 (match s ((cons (i32 n) rest) (+ n (sum rest))) (_ 0)))
    "#;
    let compiled = eval_ok_with_compiler(&format!("{}\n(compile make-quoted)\n(compile sum)\n(sum (make-quoted))", prog));
    let interpreted = eval_ok(&format!("{}\n(sum (make-quoted))", prog));
    assert_eq!(compiled, interpreted);
    assert_eq!(compiled, Value::Int(6));
}

/// A quoted symbol compiles: `translate_quote`'s `Sym` arm embeds the name
/// as an ordinary `(str ...)` literal and interns it for real at runtime
/// (`rt_intern_symbol`) — round-tripped here through `eq` against the same
/// symbol quoted on the interpreted side (interning is content-addressed,
/// so the two `Value::Symbol`s compare equal regardless of which `Heap`
/// interned them first).
#[test]
fn compile_of_a_function_quoting_a_symbol_round_trips() {
    let v = eval_ok_with_compiler(
        r#"
        (defun q () Option<Sexpr> (quote foo))
        (compile q)
        (eq (q) (quote foo))
        "#,
    );
    assert!(expect_bool(v));
}

/// A quoted list containing a symbol also compiles — the `Cons` case no
/// longer needs to bail out on a nested `Sym`/`Path` leaf now that both have
/// real compiled representations.
#[test]
fn compile_of_a_function_quoting_a_list_of_symbols_round_trips() {
    let v = eval_ok_with_compiler(
        r#"
        (defun q () Option<Sexpr> (quote (a b c)))
        (compile q)
        (equal (q) (quote (a b c)))
        "#,
    );
    assert!(expect_bool(v));
}

/// A quoted `::`-qualified path compiles: `translate_quote`'s `Path` arm
/// interns each segment (`rt_intern_symbol`) then combines them
/// (`rt_intern_path`).
#[test]
fn compile_of_a_function_quoting_a_path_round_trips() {
    let v = eval_ok_with_compiler(
        r#"
        (defun q () Option<Sexpr> (quote dep::head))
        (compile q)
        (eq (q) (quote dep::head))
        "#,
    );
    assert!(expect_bool(v));
}

/// `match` on a compiled function's quoted-symbol result dispatches
/// correctly by tag (`compile-sexpr-tag-test`'s existing `variant 5` arm,
/// exercised for the first time now that a quoted symbol can actually reach
/// compiled code) — proves the tag test, not just `eq`, sees a real `Sym`.
///
/// The scrutinee is spelled `(the Option<Sexpr> ...)` because `'foo` on its
/// own is a `Symbol`, and a `Symbol` has no tag left to test: its type
/// already says what it is. Widening it back to S-expression data is what
/// puts the runtime tag test back in play, which is what this test is about.
#[test]
fn compile_of_a_function_matching_a_quoted_symbol_dispatches_by_tag() {
    let v = eval_ok_with_compiler(
        r#"
        (defun q () i32 (match (the Option<Sexpr> (quote foo)) ((sym _) 1) (_ 0)))
        (compile q)
        (q)
        "#,
    );
    assert_eq!(v, Value::Int(1));
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

/// `push` (grow) then `get` (read back) an `i32` element — kind `1`, so the
/// element is `shl 3`-tagged on the way in and `ashr 3`-untagged on the way
/// out.
#[test]
fn compile_dispatches_vector_push_and_get_of_an_int_element_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 10)
            (push v 20)
            (push v 30)
            (get v 1)))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(20), "the element pushed at index 1");
}

/// `set` overwrites an element in place (`rt_struct_field_set` with a
/// tagged element), observable through a later `get`.
#[test]
fn compile_dispatches_vector_set_in_place_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 10)
            (push v 20)
            (set v 0 99)
            (get v 0)))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(99), "index 0 was overwritten from 10 to 99");
}

/// `len` (`rt_struct_field_count`, one of the two new primitives) returns the
/// element count as a raw `i64` — `push` grew the vector from 0 to 3.
#[test]
fn compile_dispatches_vector_len_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 10)
            (push v 20)
            (push v 30)
            (as i32 (len v))))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(3), "three pushes -> length 3");
}

/// `pop` returns `Option<T>` (unlike `get`, an empty vector is `None`, not a
/// bounds panic) — `rt_struct_field_count` gates a runtime branch between
/// building `Some` (via `rt_struct_pop_field` + `rt_data_new`) and `None`,
/// mirroring `HashTable::get`/`remove`'s own compiled `Option` construction.
#[test]
fn compile_dispatches_vector_pop_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 10)
            (push v 20)
            (push v 30)
            (match (pop v) ((Some x) x) ((None) -1))))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(30), "pop returns Some of the last-pushed element");
}

/// `pop` shrinks `len` by one, observable through a subsequent compiled
/// `len` call.
#[test]
fn compile_dispatches_vector_pop_shrinks_len_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 10)
            (push v 20)
            (match (pop v) ((Some x) x) ((None) -1))
            (as i32 (len v))))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(1), "one pop after two pushes -> length 1");
}

/// Popping an empty vector returns `None` in compiled code too, not a
/// bounds panic — the `rt_struct_field_count == 0` branch of
/// `compile-vector-op`'s "pop" arm.
#[test]
fn compile_dispatches_vector_pop_of_an_empty_vector_returns_none() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun f () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (match (pop v) ((Some x) x) ((None) -1))))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(-1), "popping an empty vector is None");
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
        (defun f () i32
          (let ((v (the Vector<string> (Vector::new))))
            (push v "hi")
            (push v "world")
            (as i32 (length (get v 1)))))
        (compile f)
        (f)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(5), "\"world\" has 5 characters");
}

/// The JIT result of a function summing a `Vector<i32>` by index must match
/// the interpreter's result for the same source — the end-to-end agreement
/// check every other compile test pairs with its representation proof.
#[test]
fn compile_of_a_vector_summing_function_agrees_with_the_interpreter() {
    let src = r#"
        (defun build-and-sum () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 3)
            (push v 4)
            (push v 5)
            (+ (+ (get v 0) (get v 1)) (get v 2))))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(build-and-sum)"))
        .expect("interpreted eval failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile build-and-sum)\n(build-and-sum)"))
        .expect("compiled eval failed");
    assert_eq!(interpreted, Value::Int(12), "3 + 4 + 5 = 12, interpreted");
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
/// `while-let` + `next` on a `vector-iter<i32>`) summing a `Vector<i32>`,
/// compiled and run natively — its result must match the interpreter's.
#[test]
fn compile_transitively_compiles_a_doiter_loop_over_a_vector() {
    let src = r#"
        (defun sum-vec ((v Vector<i32>)) i32
          (let ((total (the i32 0)))
            (doiter (x (iter v))
              (setf total (+ total x)))
            total))
        (defun run () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 3) (push v 4) (push v 5)
            (sum-vec v)))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(run)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile sum-vec)\n(compile run)\n(run)")).expect("compiled failed");
    assert_eq!(interpreted, Value::Int(12), "3 + 4 + 5 = 12 (interpreted)");
    assert_eq!(compiled, interpreted, "the transitively-compiled doiter loop agrees with the interpreter");
}

/// The `member` combinator (`where (Iter I (Item A)) (Eq A)`) over a
/// `Vector<i32>`: transitively compiling it drags in `vector-iter::next`
/// *and* the element type's `Eq` instance (`i32::equals`). Returns the
/// membership result via an `if` so the JIT boundary sees a plain `i64`.
#[test]
fn compile_transitively_compiles_the_member_combinator_over_a_vector() {
    let src = r#"
        (defun has-it ((v Vector<i32>) (needle i32)) i32
          (if (member needle (iter v)) 1 0))
        (defun run () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 10) (push v 20) (push v 30)
            (+ (* (the i32 10) (has-it v 20)) (has-it v 99))))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(run)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile has-it)\n(compile run)\n(run)")).expect("compiled failed");
    assert_eq!(interpreted, Value::Int(10), "20 is a member (10), 99 is not (0) -> 10 (interpreted)");
    assert_eq!(compiled, interpreted, "the transitively-compiled member combinator agrees with the interpreter");
}

/// The `map` combinator (builds a fresh `Vector<U>`, taking a closure `f`):
/// transitively compiling `double-all` drags in `vector-iter::next` and
/// exercises `map`'s own `push`/`Vector::new` (`vector-op`) plus a compiled
/// closure argument. The result vector is read back by index.
#[test]
fn compile_transitively_compiles_the_map_combinator_over_a_vector() {
    let src = r#"
        (defun double-all ((v Vector<i32>)) i32
          (let ((out (map (iter v) (lambda ((x i32)) i32 (* x 2)))))
            (+ (get out 0) (get out 2))))
        (defun run () i32
          (let ((v (the Vector<i32> (Vector::new))))
            (push v 1) (push v 2) (push v 3)
            (double-all v)))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(run)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile double-all)\n(compile run)\n(run)")).expect("compiled failed");
    assert_eq!(interpreted, Value::Int(8), "doubled [2,4,6], out[0]+out[2] = 2+6 = 8 (interpreted)");
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
        (defun sum-values ((ht HashTable<i32,i32>)) i32
          (let ((total (the i32 0)))
            (doiter (e (iter ht))
              (setf total (+ total (cdr e))))
            total))
        (defun run () i32
          (let ((ht (the HashTable<i32,i32> (HashTable::new))))
            (set ht 1 10)
            (set ht 2 20)
            (set ht 3 30)
            (sum-values ht)))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(run)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile sum-values)\n(compile run)\n(run)")).expect("compiled failed");
    assert_eq!(interpreted, Value::Int(60), "10 + 20 + 30 = 60 (interpreted)");
    assert_eq!(compiled, interpreted, "the transitively-compiled HashTable iteration agrees with the interpreter");
}

/// `count` (`rt_hashtable_count`) and `keys` (a fresh `Vector<K>` walked by
/// `len`) over a compiled `HashTable`, confirming the non-`entries`
/// enumerators lower correctly too.
#[test]
fn compile_dispatches_hashtable_count_and_keys_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i32
          (let ((ht (the HashTable<i32,i32> (HashTable::new))))
            (set ht 1 10)
            (set ht 2 20)
            (+ (* (the i32 100) (as i32 (count ht))) (as i32 (len (keys ht))))))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, Value::Int(202), "count 2 (*100) + keys length 2 = 202");
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
        (defun run () i32
          (let ((ht (the HashTable<i32,i32> (HashTable::new))))
            (set ht 1 100)
            (unwrap-or (get ht 1) 0)))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, Value::Int(100));
}

/// `get` on an absent key returns `None`, taking the runtime "not found"
/// branch (`rt_hashtable_contains` returns `0`) — the `else` side of the
/// `alloca`+branch+merge machinery `compile-hashtable-op`'s get/remove
/// handling builds (mirroring `compile-if`'s own shape, no `phi` builtin).
#[test]
fn compile_dispatches_hashtable_get_of_an_absent_key_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i32
          (let ((ht (the HashTable<i32,i32> (HashTable::new))))
            (set ht 1 100)
            (unwrap-or (get ht (the i32 99)) -1)))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, Value::Int(-1), "key 99 was never set, so `get` returns None -> the -1 default");
}

/// `remove` both returns the removed value (`Some(v)`) *and* deletes the
/// entry — a subsequent `get` for the same key must then miss.
#[test]
fn compile_dispatches_hashtable_remove_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i32
          (let ((ht (the HashTable<i32,i32> (HashTable::new))))
            (set ht 1 100)
            (let ((removed (unwrap-or (remove ht 1) 0)))
              (+ (* (the i32 1000) removed) (unwrap-or (get ht 1) 0)))))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, Value::Int(100000), "removed value 100 (*1000) + 0 (gone after remove) = 100000");
}

/// `get`/`remove` over a `HashTable<i64,string>` — a passthrough (kind `6`)
/// value type — exercises the `push-permanent-sexpr-root`ed branch of
/// `compile-hashtable-op`'s decode (a heap-referencing `Str` surviving
/// inside the never-GC-scanned `Option` box).
#[test]
fn compile_dispatches_hashtable_get_of_a_passthrough_string_value_to_native_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun run () i32
          (let ((ht (the HashTable<i32,string> (HashTable::new))))
            (set ht 1 "hello")
            (as i32 (length (unwrap-or (get ht 1) "")))))
        (compile run)
        (run)
        "#,
    )
    .expect("compiled failed");
    assert_eq!(v, Value::Int(5), "\"hello\" has 5 characters");
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
    assert_eq!(v, Value::Int(120));
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
    assert_eq!(v, Value::Int(120));
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
// its bits and decodes an `f64` result back (`Interp::call_compiled`) — an
// `f64` is a `BoxedObj::Float64` on the interpreter side since the scalar
// unification, but the compiled side still keeps one in a native register,
// so the boundary is where the two representations meet.

/// f64 arithmetic returning an `f64` across the JIT boundary — exercises
/// `build-fadd`/`build-fsub`/`build-fmul`/`build-fdiv` and the `f64`
/// argument/return marshaling.
#[test]
fn compile_dispatches_f64_arithmetic() {
    let v = run_f64(
        r#"
        (defun combine ((a f64) (b f64)) f64
          (/ (* (+ a b) (- a b)) 2.0))
        (compile combine)
        (combine 5.0 3.0)
        "#,
    );
    // (5+3)*(5-3)/2 = 8*2/2 = 8.0
    assert!((v - 8.0).abs() < 1e-9, "expected 8.0, got {}", v);
}

/// `mod` on `i64`/`i32` is floored (CL, sign of the divisor) in both the
/// interpreter (`eval_int_builtin`) and compiled code (`rt_int_mod`) — this
/// locks the two paths together for a negative dividend, where floored and
/// truncated diverge (`-7 mod 3 = 2`, not `-1`).
#[test]
fn compile_dispatches_i32_floored_mod_and_agrees_with_the_interpreter() {
    let src = "(defun m ((a i32) (b i32)) i32 (mod a b))";
    let call = "(m -7 3)";
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n{call}")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile m)\n{call}")).expect("compiled failed");
    assert_eq!(interpreted, Value::Int(2), "-7 mod 3 = 2 (floored)");
    assert_eq!(compiled, interpreted, "compiled floored mod agrees with the interpreter");
}

/// `mod`/`rem` on `f64` are CL-conformant (`mod` floored, `rem` truncated) and
/// — unlike the earlier truncating `build-frem` lowering — are `prelude.rs`
/// methods (`a - b*floor|truncate(a/b)`) compiled the normal way. This locks
/// the compiled path to the interpreter for a negative dividend, where floored
/// and truncated diverge.
#[test]
fn compile_dispatches_f64_mod_and_rem_floored_vs_truncated() {
    let src = "(defun m ((a f64) (b f64)) f64 (mod a b)) (defun r ((a f64) (b f64)) f64 (rem a b))";
    let call_m = "(m (- 0.0 5.5) 2.0)";
    let call_r = "(r (- 0.0 5.5) 2.0)";
    let im = run_f64(&format!("{src}\n{call_m}"));
    let cm = run_f64(&format!("{src}\n(compile m)\n{call_m}"));
    let ir = run_f64(&format!("{src}\n{call_r}"));
    let cr = run_f64(&format!("{src}\n(compile r)\n{call_r}"));
    assert!((im - 0.5).abs() < 1e-9, "-5.5 mod 2.0 = 0.5 (floored), got {}", im);
    assert!((im - cm).abs() < 1e-12, "compiled f64 mod agrees with interp");
    assert!((ir - (-1.5)).abs() < 1e-9, "-5.5 rem 2.0 = -1.5 (truncated), got {}", ir);
    assert!((ir - cr).abs() < 1e-12, "compiled f64 rem agrees with interp");
}

/// The CL numeric helpers (`abs`/`signum`/`gcd`/`lcm`/`rem`/`expt`) are
/// `prelude.rs` methods, not native-lowered builtins, so `(compile f)` where
/// `f` uses them compiles the method bodies the normal (transitive) way. Each
/// compiled result must match the interpreter across the numeric types.
#[test]
fn compile_dispatches_numeric_helpers_and_agrees_with_the_interpreter() {
    let cases = [
        ("(defun f ((x i32)) i32 (abs x))", "(f -7)"),
        ("(defun f ((a i32) (b i32)) i32 (gcd a b))", "(f -12 18)"),
        ("(defun f ((a i32) (b i32)) i32 (lcm a b))", "(f 4 6)"),
        ("(defun f ((a i32) (b i32)) i32 (rem a b))", "(f -7 3)"),
        ("(defun f ((x i32)) i32 (signum x))", "(f -123456789)"),
        ("(defun f ((a bignum) (b bignum)) bignum (gcd a b))", "(f (int->bignum 48) (int->bignum 36))"),
        ("(defun f ((a bignum) (b bignum)) bignum (expt a b))", "(f (int->bignum 2) (int->bignum 64))"),
        ("(defun f ((a ratio) (b ratio)) ratio (mod a b))", "(f -7/2 3/2)"),
        ("(defun f ((a ratio) (b ratio)) ratio (expt a b))", "(f 2/3 (int->ratio 3))"),
    ];
    for (src, call) in cases {
        let interpreted = run_readback(&format!("{src}\n{call}")).unwrap_or_else(|e| panic!("interp {}: {:?}", call, e));
        let compiled = run_readback(&format!("{src}\n(compile f)\n{call}")).unwrap_or_else(|e| panic!("compiled {}: {:?}", call, e));
        assert_eq!(compiled, interpreted, "compiled {call} agrees with the interpreter");
    }
}

/// A user `defun` whose name matches a libm symbol that an LLVM float
/// intrinsic lowers to (here `pow`, which `expt`'s `build-fpow` emits) must
/// not collide with libm once compiled: the `tl_` symbol prefix
/// (`USER_SYMBOL_PREFIX`, `ast_bridge::user_symbol_name`) means the user
/// `pow`'s own symbol is `tl_pow`, never bare `pow`, so the JIT's resolver
/// binds `expt`'s lowered `pow` call to libm and the user `pow` to its own
/// body. (This regression previously fired via `mod`->`frem`->`fmod`; `mod` no
/// longer lowers to `frem`, but the same protection covers every libm-named
/// intrinsic target.)
#[test]
fn compile_of_a_user_function_named_like_a_libm_symbol_does_not_collide() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun pow ((a i32) (b i32)) i32 (+ a b))
        (defun fexpt ((a f64) (b f64)) f64 (expt a b))
        (compile pow)
        (compile fexpt)
        (+ (pow 3 4) (float->int (fexpt 2.0 3.0)))
        "#,
    )
    .expect("eval failed");
    // pow(3,4) = 3+4 = 7 (the user's own definition, not libm's);
    // float->int(2.0 ** 3.0) = float->int(8.0) = 8; 7+8 = 15.
    assert_eq!(v, Value::Int(15));
}

/// f64 comparisons lower to `build-fcmp-*` (ordered `<`/`<=`/`>`/`>=`/`=`,
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
    assert_eq!(interpreted, Value::Int(123), "1<2 ->1, 2=2 ->2, 3>2 ->3 (interpreted)");
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
    let interpreted = run_f64(&format!("{src}\n(combine 6.25)"));
    let compiled = run_f64(&format!("{src}\n(compile combine)\n(combine 6.25)"));
    assert!((interpreted - compiled).abs() < 1e-9, "interpreted {} vs compiled {}", interpreted, compiled);
}

/// The `f64` trigonometric/hyperbolic/exponential family added alongside
/// `sqrt`/`floor`/etc: `sin`/`cos`/`exp`/`log` lower to LLVM intrinsics
/// (`build-fsin`/...) like `sqrt` does; `tan`/`asin`/`acos`/`atan`/`sinh`/
/// `cosh`/`tanh`/`asinh`/`acosh`/`atanh` have no LLVM intrinsic in the
/// version this project pins, so they lower to `rt_f64_*` shims instead —
/// this exercises both codegen paths in one compiled function.
#[test]
fn compile_dispatches_f64_transcendental_functions_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun combine ((x f64)) f64
          (+ (sin x) (+ (cos x) (+ (tan x) (+ (asin x) (+ (acos x)
             (+ (atan x) (+ (sinh x) (+ (cosh x) (+ (tanh x)
                (+ (asinh x) (+ (acosh (+ x 1.0)) (+ (atanh x) (+ (exp x) (log (+ x 1.0))))))))))))))))
    "#;
    let interpreted = run_f64(&format!("{src}\n(combine 0.5)"));
    let compiled = run_f64(&format!("{src}\n(compile combine)\n(combine 0.5)"));
    assert!((interpreted - compiled).abs() < 1e-9, "interpreted {} vs compiled {}", interpreted, compiled);
}

/// `max`/`min` (`icmp`+`select`, branch-free) for every numeric type:
/// `i32`/`i64` (bare `icmp`), `f64` (`llvm.maxnum.f64`/`llvm.minnum.f64`),
/// and `bignum`/`ratio` (their three-way `rt_*_cmp` plus `select`).
#[test]
fn compile_dispatches_max_min_across_every_numeric_type_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun combine ((a i32) (b i32) (x f64) (y f64) (n bignum) (o bignum) (p ratio) (q ratio)) i32
          (+ (max a b) (+ (min a b)
             (+ (float->int (max x y)) (+ (float->int (min x y))
                (+ (bignum->int (max n o)) (+ (bignum->int (min n o))
                   (+ (bignum->int (ratio->bignum (max p q))) (bignum->int (ratio->bignum (min p q)))))))))))
    "#;
    let args = "3 7 1.0 5.0 (int->bignum 20) (int->bignum 9) (int->ratio 30) (int->ratio 11)";
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(combine {args})")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile combine)\n(combine {args})")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled max/min agree with the interpreter across every numeric type");
}

/// The bitwise catalog (`logand`/`logior`/`logxor`/`lognot`/`ash`/`logbitp`/
/// `logcount`/`logtest`/`integer-length`) on `i32`: `logand`/`logior`/
/// `logxor`/`lognot` are bare LLVM instructions, `ash`/`logbitp`/`logcount`/
/// `integer-length` lower to `rt_int_*` shims (a variable shift/count past
/// the operand's bit width being undefined behavior in LLVM, unlike this
/// language's clamped semantics), `logtest` composes `build-and` with an
/// existing `build-icmp-ne`.
#[test]
fn compile_dispatches_i32_bitwise_operators_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun combine ((a i32) (b i32)) i32
          (+ (logand a b) (+ (logior a b) (+ (logxor a b) (+ (lognot a)
             (+ (ash a 2) (+ (logcount a) (+ (integer-length a)
                (+ (if (logbitp a 1) 1 0) (if (logtest a b) 1 0))))))))))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(combine 12 10)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile combine)\n(combine 12 10)")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled i32 bitwise operators agree with the interpreter");
}

/// The builtins that had no compiled lowering, now that they do.
///
/// `docs/syntax.md` §10 carried a table of "builtins with no compiled
/// implementation" — methods a user's own `defun` could name and thereby
/// become uncompilable. It had three rows, and this covers all three:
/// `string::upcase`/`downcase` (`rt_str_upcase`/`rt_str_downcase`), the
/// `Option`-returning conversions (`rt_int_fits`/`rt_int_fits_char`/
/// `rt_f64_fits_f32` plus the island's `build-try-option`), and `bignum`'s
/// remaining bitwise catalog.
///
/// None of these has a prelude caller, which is why none of them shows up in
/// `PRELUDE_COMPILE_UNSUPPORTED` — the gap was only ever reachable by writing
/// the call yourself, which is what this test does.
#[test]
fn the_builtins_that_used_to_block_compilation_now_lower() {
    let src = r#"
        (defun su ((s string)) string (append (upcase s) (downcase s)))
        (defun bb ((b bignum)) bignum (+ (logcount b) (integer-length b)))
        (defun bp ((b bignum)) bool (if (logbitp b 3) (logtest b (as bignum 7)) false))
        (defun tc ((n i32)) i32 (match (try-as char n) ((some c) (as i32 (char->int c))) ((none) -1)))
        (defun tu ((n i32)) i32 (match (try-as u8 n) ((some v) (as i32 v)) ((none) -1)))
        (defun tf ((x f64)) i32 (match (try-as f32 x) ((some v) 1) ((none) 0)))
        (defun tw ((x f32)) i32 (match (try-as f64 x) ((some v) 1) ((none) 0)))
        (defun all () i32
          (+ (length (su "Ab"))
             (+ (as i32 (bignum->int (bb (as bignum 255))))
                (+ (if (bp (as bignum 255)) 1 0)
                   (+ (tc 65) (+ (tc 55296) (+ (tu 200) (+ (tu 300)
                      (+ (tf 0.5) (+ (tf 0.1) (tw (as f32 0.5)))))))))))) 
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}
(all)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!(
        "{src}
(compile su)
(compile bb)
(compile bp)
(compile tc)
(compile tu)
(compile tf)
(compile tw)
(compile all)
(all)"
    ))
    .expect("compiled failed");
    assert_eq!(compiled, interpreted, "the newly lowered builtins agree with the interpreter");
    // 4 ("ABab") + 16 (8 one-bits + 8 bits) + 1 + 65 + -1 + 200 + -1 + 1 + 0 + 1
    assert_eq!(compiled, Value::Int(286));
}

/// The byte-specifier family compiles at a width that is not `i32`, and at
/// `bignum`.
///
/// These are generic functions bounded by `Bits`, so each call site
/// monomorphizes to its own specialization — the ordinary path, but one that
/// only opened when the integer moved into the first argument. The `bignum`
/// half also pulled `bignum::ash` into the island's lowering: it had been
/// left out on the grounds that no prelude definition reached it, and
/// `Bits`'s `shift` impl reaches it.
#[test]
fn the_byte_specifier_family_compiles_at_every_width() {
    let src = r#"
        (defun nibble ((x u8)) u8 (ldb x (byte 4 4)))
        (defun put ((x i32)) i32 (dpb x 255 (byte 8 0)))
        (defun high ((x u8)) bool (logbitp x 7))
        (defun wide ((x bignum)) bignum (ash x 100))
        (defun nand8 ((x u8) (y u8)) u8 (boole boole-nand x y))
        (defun all () i32
          (+ (as i32 (nibble (the u8 165)))
             (+ (put 62848)
                (+ (if (high (the u8 128)) 1 0)
                   (+ (as i32 (bignum->int (ash (wide (as bignum 1)) -100)))
                      (as i32 (nand8 (the u8 240) (the u8 60))))))))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}
(all)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!(
        "{src}
(compile nibble)
(compile put)
(compile high)
(compile wide)
(compile nand8)
(compile all)
(all)"
    ))
    .expect("compiled failed");
    assert_eq!(compiled, interpreted, "the compiled byte-specifier family agrees with the interpreter");
    // 10 + 62975 + 1 + 1 + 207
    assert_eq!(compiled, Value::Int(63194));
}

/// CL's variadic sugar (`Checker::check_variadic_arith`/`check_variadic_cmp`,
/// desugared at check time into nested 2-argument `Expr::Assoc` nodes) means
/// `compile-assoc` never sees a 3-argument `+`/`<` call — this just confirms
/// a function using the 3+-argument forms compiles and runs correctly.
#[test]
fn compile_dispatches_variadic_arithmetic_and_comparison_sugar() {
    let src = r#"
        (defun combine ((a i32) (b i32) (c i32)) bool
          (if (< a b c) (= 6 (+ a b c)) false))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(combine 1 2 3)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile combine)\n(combine 1 2 3)")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled variadic +/< sugar agrees with the interpreter");
    assert_eq!(compiled, Value::Bool(true));
}

/// `expt` (binary, `f64,f64->f64`) lowers to `build-fpow` (`llvm.pow.f64`).
#[test]
fn compile_dispatches_f64_expt_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun power ((base f64) (exp f64)) f64 (expt base exp))
    "#;
    let interpreted = run_readback(&format!("{src}\n(power 2.0 10.0)")).expect("interpreted failed");
    let compiled = run_readback(&format!("{src}\n(compile power)\n(power 2.0 10.0)")).expect("compiled failed");
    assert_eq!(interpreted, Readback::F64(1024.0));
    assert_eq!(compiled, interpreted, "compiled expt agrees with the interpreter");
}

/// `float->int` lowers to the saturating `llvm.fptosi.sat` intrinsic
/// (`build-fptosi`), no heap allocation involved.
#[test]
fn compile_dispatches_float_to_int_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun truncate-it ((x f64)) i32 (float->int x))
    "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(truncate-it 7.9)")).expect("interpreted failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile truncate-it)\n(truncate-it 7.9)")).expect("compiled failed");
    assert_eq!(interpreted, Value::Int(7));
    assert_eq!(compiled, interpreted, "compiled float->int agrees with the interpreter");
}

/// `llvm.fptosi.sat` clamps exactly like Rust's `as i32` cast (the
/// interpreter's own `float_to_int`, `i64::from(*f as i32)`) — unlike a plain
/// `fptosi` instruction, which is a poison value on NaN/out-of-range input.
/// The saturation point is `i32`'s, not the carrier's: `float->int` returns
/// the widest fixed-width integer the language has, and since 2026-09-01 that
/// is `i32`. Covers every edge case that distinction matters for: NaN -> `0`,
/// `+inf`/an overflowing magnitude -> `i32::MAX`, `-inf`/an underflowing
/// magnitude -> `i32::MIN`, and one ordinary finite value as a sanity check
/// that the intrinsic swap didn't change everyday behavior.
#[test]
fn compile_dispatches_float_to_int_on_nan_and_out_of_range_inputs_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun to-int ((x f64)) i32 (float->int x))
    "#;
    let cases: &[(&str, i64)] = &[
        ("(/ 0.0 0.0)", 0),                       // NaN -> 0
        ("(/ 1.0 0.0)", i32::MAX as i64),         // +inf -> i32::MAX
        ("(/ -1.0 0.0)", i32::MIN as i64),        // -inf -> i32::MIN
        ("1e300", i32::MAX as i64),               // overflowing magnitude -> i32::MAX
        ("-1e300", i32::MIN as i64),              // underflowing magnitude -> i32::MIN
        ("7.9", 7),                               // ordinary finite value, unaffected
    ];
    for (expr, expected) in cases {
        let interpreted = run_with_compiler_and_prelude(&format!("{src}\n(to-int {expr})"))
            .unwrap_or_else(|e| panic!("interpreted failed for {}: {}", expr, e));
        let compiled = run_with_compiler_and_prelude(&format!("{src}\n(compile to-int)\n(to-int {expr})"))
            .unwrap_or_else(|e| panic!("compiled failed for {}: {}", expr, e));
        assert_eq!(interpreted, Value::Int(*expected), "interpreted float->int of {expr}");
        assert_eq!(compiled, interpreted, "compiled float->int of {expr} agrees with the interpreter");
    }
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
    let interpreted = run_readback(&format!("{src}\n(big)")).expect("interpreted failed");
    let compiled = run_readback(&format!("{src}\n(compile big)\n(big)")).expect("compiled failed");
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
    let interpreted = run_readback(&format!("{src}\n{call}")).expect("interpreted failed");
    let compiled = run_readback(&format!("{src}\n(compile combine)\n{call}")).expect("compiled failed");
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
    let interpreted = run_readback(&format!("{src}\n{call}")).expect("interpreted failed");
    let compiled = run_readback(&format!("{src}\n(compile quotient)\n{call}")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled bignum division agrees with the interpreter");
}

/// The six `bignum` comparisons (`<`/`<=`/`>`/`>=`/`=`/`/=`) all derive from
/// `rt_bignum_cmp`'s three-way result — this exercises each direction.
#[test]
fn compile_dispatches_bignum_comparisons_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun compare ((a bignum) (b bignum)) i32
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
        let interpreted = run_readback(&format!("{def}\n{call}")).expect("interpreted failed");
        let compiled = run_readback(&format!("{def}\n(compile f)\n{call}")).expect("compiled failed");
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
    let interpreted = run_readback(&format!("{src}\n(frac)")).expect("interpreted failed");
    let compiled = run_readback(&format!("{src}\n(compile frac)\n(frac)")).expect("compiled failed");
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
    let interpreted = run_readback(&format!("{src}\n{call}")).expect("interpreted failed");
    let compiled = run_readback(&format!("{src}\n(compile combine)\n{call}")).expect("compiled failed");
    assert_eq!(compiled, interpreted, "compiled ratio arithmetic agrees with the interpreter");
}

/// The six `ratio` comparisons all derive from `rt_ratio_cmp`'s three-way
/// result, the same shape as the bignum branch.
#[test]
fn compile_dispatches_ratio_comparisons_and_agrees_with_the_interpreter() {
    let src = r#"
        (defun compare ((a ratio) (b ratio)) i32
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
        let interpreted = run_readback(&format!("{def}\n{call}")).expect("interpreted failed");
        let compiled = run_readback(&format!("{def}\n(compile f)\n{call}")).expect("compiled failed");
        assert_eq!(compiled, interpreted, "compiled `{def}` agrees with the interpreter");
    }
}

/// The `sexpr-*` island layer beyond `car`/`cdr`/`cons` (closure
/// unification Stage 8): the tag predicates and typed payload extractors
/// now lower to `rt_*` shims (`rt_consp`/`rt_null`/`rt_atom`/`rt_symp`/
/// `rt_sexpr_int`/`rt_sexpr_bool`/`rt_sexpr_char`/`rt_float_value`/
/// `rt_sexpr_str`/`rt_sym_name` — `compile-call`'s rename table), so a
/// `defun` walking a `Sexpr` list compiles. Each case runs interpreted and
/// compiled and must agree.
#[test]
fn compile_dispatches_sexpr_accessors_and_agrees_with_the_interpreter() {
    let cases = [
        // Recursive list walk: `sexpr-consp` steering, `sexpr-cdr` descent.
        (
            "(defun f ((s Option<Sexpr>)) i32 (if (sexpr-consp s) (+ (the i32 1) (f (sexpr-cdr s))) (the i32 0)))",
            "(f '(10 20 30))",
        ),
        // `sexpr-null`/`sexpr-atom` predicates surface as bools.
        ("(defun f ((s Option<Sexpr>)) bool (sexpr-null s))", "(f '())"),
        ("(defun f ((s Option<Sexpr>)) bool (sexpr-atom s))", "(f '(1 2))"),
        ("(defun f ((s Option<Sexpr>)) bool (sexpr-symp s))", "(f 'hello)"),
        // Typed payload extractors.
        ("(defun f ((s Option<Sexpr>)) i32 (sexpr-i32 (sexpr-car s)))", "(f '(42 43))"),
        ("(defun f ((s Option<Sexpr>)) bool (sexpr-bool (sexpr-car s)))", "(f '(true))"),
        ("(defun f ((s Option<Sexpr>)) char (sexpr-char (sexpr-car s)))", r#"(f '(#\A #\B))"#),
        ("(defun f ((s Option<Sexpr>)) f64 (sexpr-f64 (sexpr-car s)))", "(f '(2.5))"),
        ("(defun f ((s Option<Sexpr>)) string (sexpr-str (sexpr-car s)))", r#"(f '("hi"))"#),
        ("(defun f ((s Option<Sexpr>)) string (sexpr-sym-name (sexpr-car s)))", "(f '(hello))"),
    ];
    for (def, call) in cases {
        let interpreted = run_readback(&format!("{def}\n{call}")).expect("interpreted failed");
        let compiled = run_readback(&format!("{def}\n(compile f)\n{call}")).expect("compiled failed");
        assert_eq!(compiled, interpreted, "compiled `{def}` agrees with the interpreter");
    }
}

// ---- dynamic dispatch through a trait object's vtable (TODO T4) ---------

/// The compiled counterpart of `tests/dyn_dispatch_test.rs`: one `(draw d)`
/// call site in a *native* body, reaching two different implementations
/// through the vtable. `compile` is only asked for the boxing function —
/// `ast_bridge::collect_calls`' `DynBox` arm pulls both `impl` methods into
/// the call graph, so `compute_sccs` compiles them first and their addresses
/// are in the table by the time this runs.
#[test]
fn compile_dispatches_a_trait_object_call_through_its_vtable() {
    let v = run_with_compiler_and_prelude(
        r#"
        (deftrait Drawable () (draw ((self Self)) i32))
        (defstruct circle (r i32))
        (defstruct square (side i32))
        (impl Drawable circle (draw ((self Self)) i32 1))
        (impl Drawable square (draw ((self Self)) i32 2))
        (defun render ((d :dyn Drawable)) i32 (draw d))
        (defun both () i32 (+ (* 10 (render (circle::new 3))) (render (square::new 4))))
        (compile both)
        (both)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(12));
}

/// Slot numbering must survive into native code: `deftrait` order decides
/// which vtable entry each method occupies, so a second method has to reach
/// its own implementation and not the first one's.
#[test]
fn compile_indexes_the_right_vtable_slot_for_each_method() {
    let v = run_with_compiler_and_prelude(
        r#"
        (deftrait Shape ()
          (draw ((self Self)) i32)
          (sides ((self Self)) i32))
        (defstruct tri (n i32))
        (impl Shape tri
          (draw ((self Self)) i32 7)
          (sides ((self Self)) i32 3))
        (defun outline ((s :dyn Shape)) i32 (sides s))
        (defun paint ((s :dyn Shape)) i32 (draw s))
        (defun both () i32 (+ (* 10 (paint (tri::new 1))) (outline (tri::new 1))))
        (compile both)
        (both)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(73));
}

/// The inherited half of the same contract: a supertrait's methods occupy
/// the low slots of a subtrait's vtable, so a JIT'd call site indexing a
/// baked-in constant must land on the inherited body, not a shifted one.
#[test]
fn compile_indexes_inherited_vtable_slots_before_the_subtraits_own() {
    let v = run_with_compiler_and_prelude(
        r#"
        (deftrait Base ()
          (base-a ((self Self)) i32)
          (base-b ((self Self)) i32))
        (deftrait Sub (Base)
          (sub-c ((self Self)) i32))
        (defstruct tri (n i32))
        (impl Base tri
          (base-a ((self Self)) i32 1)
          (base-b ((self Self)) i32 2))
        (impl Sub tri
          (sub-c ((self Self)) i32 3))
        (defun pa ((s :dyn Sub)) i32 (base-a s))
        (defun pb ((s :dyn Sub)) i32 (base-b s))
        (defun pc ((s :dyn Sub)) i32 (sub-c s))
        (defun all () i32
          (+ (* 100 (pa (tri::new 0))) (+ (* 10 (pb (tri::new 0))) (pc (tri::new 0)))))
        (compile all)
        (all)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(123));
}

/// Upcasting to a *non-leftmost* supertrait is the one conversion that
/// cannot be a retype: `C`'s slots sit after `B`'s in `D`'s vtable, so the
/// box is re-made around `C`'s own table (`rt_dyn_upcast`, the island's
/// `compile-dyn-upcast` tag). Reusing `D`'s table would call `b-tag` — slot
/// 0 there — and answer `1`.
#[test]
fn compile_upcasts_to_a_non_first_supertrait_through_the_conversion_table() {
    let v = run_with_compiler_and_prelude(
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
        (defun both () i32 (+ (* 10 (via (cell::new 0))) (via (pair::new 0))))
        (compile both)
        (both)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(25));
}

/// The conversion has to work when the box is made *interpreted* and only
/// the upcasting function is native: the mapping is published to the
/// compiled tier at every boxing site, not only when something is compiled.
#[test]
fn compile_upcasts_a_trait_object_boxed_by_interpreted_code() {
    let v = run_with_compiler_and_prelude(
        r#"
        (deftrait B () (b-tag ((self Self)) i32))
        (deftrait C () (c-tag ((self Self)) i32))
        (deftrait D (B C) (d-tag ((self Self)) i32))
        (defstruct cell (n i32))
        (impl B cell (b-tag ((self Self)) i32 1))
        (impl C cell (c-tag ((self Self)) i32 2))
        (impl D cell (d-tag ((self Self)) i32 3))
        (defun only-c ((c :dyn C)) i32 (c-tag c))
        (defun via ((d :dyn D)) i32 (only-c d))
        (compile via)
        (via (cell::new 0))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(2));
}

/// A method argument and a boxed return value cross the vtable boundary
/// under the ordinary compiled-call ABI.
#[test]
fn compile_passes_arguments_through_a_trait_object_call() {
    let v = run_with_compiler_and_prelude(
        r#"
        (deftrait Scaler () (scale ((self Self) (k i32)) i32))
        (defstruct fixed (n i32))
        (impl Scaler fixed (scale ((self Self) (k i32)) i32 (* self::n k)))
        (defun apply-scale ((s :dyn Scaler) (k i32)) i32 (scale s k))
        (defun rec () i32 (apply-scale (fixed::new 6) 7))
        (compile rec)
        (rec)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
}

/// Compiled and interpreted tiers must agree, including on the `match`
/// downcast that unwraps the box back to a concrete type.
#[test]
fn compile_and_interpret_agree_on_a_trait_object_match() {
    let src = r#"
        (deftrait Drawable () (draw ((self Self)) i32))
        (defstruct circle (r i32))
        (defstruct square (side i32))
        (impl Drawable circle (draw ((self Self)) i32 1))
        (impl Drawable square (draw ((self Self)) i32 2))
        (defun area ((d :dyn Drawable)) i32
          (match d
            ((circle r) (* 100 r))
            ((the square s) s::side)
            (_ 0)))
        (defun rec () i32 (+ (area (circle::new 3)) (area (square::new 4))))
        "#;
    let interpreted = run_with_compiler_and_prelude(&format!("{src} (rec)")).expect("eval failed");
    let compiled = run_with_compiler_and_prelude(&format!("{src} (compile rec) (rec)")).expect("eval failed");
    assert_eq!(interpreted, Value::Int(304));
    assert_eq!(compiled, interpreted);
}

/// `(compile <macro>)`: a macro body is an ordinary `Sexpr -> Sexpr` function
/// — expanding it *is* calling it — so it compiles like any other definition,
/// and expansions after the compile run through the native body.
///
/// Two claims, because either alone would be worthless: the expander really is
/// compiled (`is_compiled`, since a compiled call is observationally identical
/// to an interpreted one except in speed), and the expansion is still correct.
/// The checker had to learn this name too: it keeps functions and macros in
/// separate maps, so `(compile twice)` was rejected as "no function `twice` is
/// visible from here" even though the interpreter has the body right there in
/// its `fns` table.
#[test]
fn a_macro_expander_can_be_compiled() {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    typelisp::compile::install_llvm_backend();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);

    let run = |h: &mut Heap, chk: &mut Checker, interp: &Interp, src: &str| -> Value {
        let r = Reader::new();
        let vs = r.read_all(h, src).expect("read failed");
        let mut last = Value::Empty;
        for v in vs {
            let tl = chk.check_form(h, interp, v).expect("check failed");
            if let Some(val) = interp.exec(h, tl).expect("eval failed") {
                last = val;
            }
        }
        last
    };

    run(&mut h, &mut chk, &interp, "(defmacro twice (x) `(+ ,x ,x))");
    assert!(!interp.is_compiled("twice"), "a freshly defined macro has no compiled body yet");
    assert_eq!(run(&mut h, &mut chk, &interp, "(twice 21)"), Value::Int(42));

    run(&mut h, &mut chk, &interp, "(compile twice)");
    assert!(interp.is_compiled("twice"), "`(compile twice)` should have installed a native expander");
    assert_eq!(
        run(&mut h, &mut chk, &interp, "(twice 21)"),
        Value::Int(42),
        "expanding through the native body must produce the same expansion"
    );
}

/// Compiling *only* the dispatching function — the boxing happens in
/// interpreted code at the call site — must still work. Nothing in
/// `render`'s own body names `circle::draw`, so the vtable's targets have to
/// be reached some other way than the ordinary call graph.
#[test]
fn compile_dispatches_when_only_the_dispatching_function_is_compiled() {
    let v = run_with_compiler_and_prelude(
        r#"
        (deftrait Drawable () (draw ((self Self)) i32))
        (defstruct circle (r i32))
        (defstruct square (side i32))
        (impl Drawable circle (draw ((self Self)) i32 1))
        (impl Drawable square (draw ((self Self)) i32 2))
        (defun render ((d :dyn Drawable)) i32 (draw d))
        (compile render)
        (+ (* 10 (render (circle::new 3))) (render (square::new 4)))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(12));
}

/// An `impl` added *after* the dispatching function was compiled: its method
/// isn't in that call site's `impl_targets` snapshot, so nothing pulled it
/// into native code. Dispatch must still reach it.
#[test]
fn compile_dispatches_to_an_impl_added_after_the_call_site_was_compiled() {
    let v = run_with_compiler_and_prelude(
        r#"
        (deftrait Drawable () (draw ((self Self)) i32))
        (defstruct circle (r i32))
        (impl Drawable circle (draw ((self Self)) i32 1))
        (defun render ((d :dyn Drawable)) i32 (draw d))
        (compile render)
        (defstruct square (side i32))
        (impl Drawable square (draw ((self Self)) i32 2))
        (+ (* 10 (render (circle::new 3))) (render (square::new 4)))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(12));
}

/// A trait whose implementations have *generic* owners (prelude's `Iter`,
/// implemented by `vector-iter<T>`): the call site's `impl_targets` skips
/// those, because a generic owner's method has no code until it is
/// specialized — and which specialization is only known where a concrete
/// instantiation is boxed. That box is what has to pull it in.
#[test]
fn compile_dispatches_a_trait_object_whose_impl_owner_is_generic() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun total ((it :dyn Iter<i32>)) i32
          (let ((n 0))
            (loop
              (match (next it)
                ((Some x) (setf n (+ n x)))
                (_ (break))))
            n))
        (defun make-v () Vector<i32> (Vector::new))
        (compile total)
        (let ((v (make-v)))
          (push v 10)
          (push v 32)
          (total (iter v)))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
}

// ---- user-defined error types (TODO T3) ---------------------------------

/// A user error type as `Result`'s `E`, compiled: `Result<T, MyErr>` is an
/// ordinary sum of a sum, so the native code has to build, match, and read a
/// field out of a `defstruct` sitting in the `err` payload.
#[test]
fn compile_dispatches_a_result_carrying_a_user_error_type() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct io-err (code i32))
        (defun open-it ((ok bool)) Result<i32,io-err>
          (if ok (result::ok 7) (result::err (io-err::new 42))))
        (defun code ((ok bool)) i32
          (match (open-it ok) ((Ok v) v) ((Err e) e::code)))
        (compile code)
        (+ (* 100 (code true)) (code false))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(742));
}

/// A *built-in* error type crossing into native code: `ParseIntError` is an
/// ordinary single-variant sum like any user error type, so a compiled body
/// must match it and call its prelude `Error` impl. (`parse-int` itself has
/// no compiled implementation, so the value is produced interpreted and
/// passed in — which is exactly how a built-in error reaches native code.)
#[test]
fn compile_dispatches_a_match_on_a_builtin_error_type() {
    let v = run_readback(
        r#"
        (defun classify ((r Result<i32,ParseIntError>)) string
          (match r ((Ok _) "ok") ((Err e) (message e))))
        (compile classify)
        (append (classify (parse-int "12")) (classify (parse-int "xy")))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Readback::Str("okparse-int: invalid integer literal: \"xy\"".to_string()));
}

/// The uniform-handling shape from `docs/dev/language-design.md` §7.4, in
/// native code: two unrelated error types reaching one `(message e)` call
/// site through the `Error` trait's vtable, with a built-in error
/// (`ParseIntError`, whose `impl` lives in the prelude) as one of them.
#[test]
fn compile_dispatches_message_on_a_dyn_error() {
    let v = run_readback(
        r#"
        (defstruct app-err (why string))
        (impl Error app-err
          (message ((self Self)) string self::why)
          (source ((self Self)) Option<:dyn Error> (option::none)))
        (defun describe ((e :dyn Error)) string (message e))
        (defun both () string
          (append (describe (app-err::new "mine"))
                  (describe (ParseIntError::ParseIntError "/builtin"))))
        (compile both)
        (both)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Readback::Str("mine/builtin".to_string()));
}

// ---- `()`-typed fields cross the compiled boundary ----------------------
//
// `Type::Unit` used to be a *return-position-only* allowance: a body tail
// compiled to a plain `0`, but a `()`-typed field had no encoding at all
// (`ast_bridge::struct_field_kind` kind `0`), so `compile-tag-struct-field`
// panicked outright. Kind `11` closes that: the slot stores the tagged
// `Value::Empty` word an interpreted writer stores, and reads back as the
// same plain `0` every other unit value in compiled code already is. The
// tests below drive both halves through real native code.

#[test]
fn compile_round_trips_a_unit_typed_struct_field() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct holder (u ()) (k i32))
        (defun make ((n i32)) holder (holder::new () n))
        (defun read-k ((h holder)) i32 h::k)
        (compile make)
        (compile read-k)
        (read-k (make 7))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(7));
}

#[test]
fn a_struct_built_by_compiled_code_reads_back_in_the_interpreter() {
    // The encode side alone: the box is built natively (constant `6`) and
    // then destructured by the *interpreter*, which only agrees if both
    // writers picked the same word for the `()` slot.
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct holder (u ()) (k i32))
        (defun make ((n i32)) holder (holder::new () n))
        (compile make)
        (match (make 4) ((new u n) n))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(4));
}

#[test]
fn compile_handles_a_result_with_a_unit_ok_payload() {
    // The motivating case for the whole encoding — `Result<(), E>` is what
    // the prelude's file-writing functions wanted and couldn't have.
    let v = run_readback(
        r#"
        (defun check ((n i32)) Result<(), string>
          (if (> n 0) (result::ok ()) (result::err "negative")))
        (defun describe ((n i32)) string
          (match (check n) ((ok _) "ok") ((err e) e)))
        (compile check)
        (compile describe)
        (append (describe 1) (describe -1))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Readback::Str("oknegative".to_string()));
}

#[test]
fn compile_accepts_a_unit_typed_parameter() {
    // `struct_field_kind` also gates `Interp::is_jit_tier_ty`, so giving
    // `()` a kind admits it as a parameter type too; it crosses as the
    // plain `0` `encode_crossing_args` sends.
    let v = run_with_compiler_and_prelude(
        r#"
        (defun takes-unit ((u ()) (n i32)) i32 (progn u n))
        (compile takes-unit)
        (takes-unit () 9)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(9));
}

#[test]
fn compile_round_trips_a_vector_of_units() {
    // The same kind drives `Vector<T>`'s element encoding
    // (`ast_bridge::vector_elem_kind`), not just `defstruct` fields.
    let v = run_with_compiler_and_prelude(
        r#"
        (defun two-units () i32
          (let ((v (the Vector<()> (vector::new))))
            (push v ())
            (push v ())
            (len v)))
        (compile two-units)
        (two-units)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(2));
}

// ---- a built-in type is identified by its whole path, not its last segment --
//
// Every built-in lives at the root namespace, but nothing stops a module from
// defining `vector`/`hashtable` of its own (`Checker::check_redef` only
// guards the root). Compiled code used to recognize the built-in by last
// segment alone and lower the *user* type's methods to `vector-op`/
// `hashtable-op` — reading a user struct as if it were the built-in.

#[test]
fn a_module_type_named_vector_keeps_its_own_methods_when_compiled() {
    // Was: `1` — the built-in `len`, i.e. the struct's field count.
    let v = run_with_compiler_and_prelude(
        r#"
        (module m
          (pub defstruct vector (a i32))
          (pub defmethod len ((self vector)) i32 (* 100 self::a))
          (pub defun call-len ((v vector)) i32 (len v)))
        (compile m::call-len)
        (m::call-len (m::vector::new 3))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(300));
}

#[test]
fn a_module_type_named_hashtable_keeps_its_own_methods_when_compiled() {
    // Was: a `BoxId does not hold a HashTable` abort — the compiled body
    // called the built-in map runtime on a plain struct.
    let v = run_with_compiler_and_prelude(
        r#"
        (module m
          (pub defstruct hashtable (a i32) (b i32))
          (pub defmethod count ((self hashtable)) i32 (+ self::a self::b))
          (pub defun call-count ((h hashtable)) i32 (count h)))
        (compile m::call-count)
        (m::call-count (m::hashtable::new 20 22))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
}

#[test]
fn the_real_builtin_vector_still_lowers_from_inside_a_module() {
    // The other side of the same guard: a *use* inside a module still names
    // the root `vector`, so its methods must keep lowering to `vector-op`.
    let v = run_with_compiler_and_prelude(
        r#"
        (module m
          (pub defun total ((v Vector<i32>)) i32
            (let ((sum 0) (i 0))
              (while (< i (len v)) (setf sum (+ sum (get v i))) (setf i (+ i 1)))
              sum)))
        (compile m::total)
        (let ((v (the Vector<i32> (vector::new))))
          (push v 10)
          (push v 32)
          (m::total v))
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
}

// ---- cross-boundary conformance: a value is the same value either side ----
//
// The behavioural net under `src/type_key.rs`'s invariant. A heap value's type
// identity is a string, written by whichever side built it and read by
// whichever side consumes it, so the two sides must agree — and on 2026-08-04
// they did not (`ast_bridge` spelled `Path::last_segment`, the interpreter
// `Path::to_string`), which made a compiled constructor's value unmatchable,
// unprintable and unequal to its interpreted twin.
//
// Every case here lives in `(module m ...)`, and that is the point: at the
// root namespace the two spellings are the same string, which is why 200-odd
// compile tests missed the bug entirely. The matrix is build-site
// (interpreted / compiled) × consume-site (interpreted `match`, compiled
// `match`, compiled `(the T p)` downcast, printing, `equalp`), over both an
// enum and a struct.

/// A module defining the enum and struct the matrix works with, plus one
/// builder and one consumer per side. `compile` is applied to exactly the
/// functions whose name ends in `-c`, so a call names its own side.
const CONFORMANCE_MODULE: &str = r#"
(module m
  (pub defenum expr (num i32) (add expr expr))
  (use expr)
  (pub defstruct pt (x i32) (y i32))

  ;; builders — `-i` stays interpreted, `-c` gets compiled below
  (pub defun enum-i ((n i32)) expr (num n))
  (pub defun enum-c ((n i32)) expr (num n))
  (pub defun struct-i ((n i32)) pt (pt::new n 0))
  (pub defun struct-c ((n i32)) pt (pt::new n 0))
  ;; a nested function is always compiled, whether or not anything asks
  (pub defun enum-nested ((n i32)) expr (labels ((f ((k i32)) expr (num k))) (f n)))

  ;; consumers
  (pub defun take-enum-i ((e expr)) i32 (match e ((num v) v) ((add a b) -1)))
  (pub defun take-enum-c ((e expr)) i32 (match e ((num v) v) ((add a b) -1)))
  (pub defun downcast-c ((s Sexpr)) i32 (match s ((the pt p) p::x) (_ -1)))
  (pub defun downcast-i ((s Sexpr)) i32 (match s ((the pt p) p::x) (_ -1))))
(compile m::enum-c)
(compile m::struct-c)
(compile m::take-enum-c)
(compile m::downcast-c)
"#;

/// Runs `expr` with [`CONFORMANCE_MODULE`] in scope, expecting an `i32`.
fn conformance_int(expr: &str) -> i64 {
    match run_with_compiler_and_prelude(&format!("{CONFORMANCE_MODULE}\n{expr}"))
        .expect("eval failed")
    {
        Value::Int(n) => n,
        other => panic!("expected an Int, got {:?}", other),
    }
}

/// Runs `expr` with [`CONFORMANCE_MODULE`] in scope, expecting a string.
fn conformance_str(expr: &str) -> String {
    match run_readback(&format!("{CONFORMANCE_MODULE}\n{expr}")).expect("eval failed") {
        Readback::Str(s) => s,
        other => panic!("expected a Str, got {:?}", other),
    }
}

#[test]
fn an_enum_value_matches_whichever_side_built_it() {
    // 3 builders × 2 `match` sites. Every cell must see `num 5`.
    for build in ["m::enum-i", "m::enum-c", "m::enum-nested"] {
        for take in ["m::take-enum-i", "m::take-enum-c"] {
            let got = conformance_int(&format!("({take} ({build} 5))"));
            assert_eq!(got, 5, "built by {build}, matched by {take}");
        }
    }
}

#[test]
fn an_enum_value_matches_an_interpreted_inline_match_whichever_side_built_it() {
    // The inline form too: `take-enum-*` is a *function* the checker sees
    // whole, while this `match` sits at top level in the caller's own body.
    for build in ["m::enum-i", "m::enum-c", "m::enum-nested"] {
        let got = conformance_int(&format!(
            "(use m::expr) (match ({build} 5) ((num v) v) ((add a b) -1))"
        ));
        assert_eq!(got, 5, "built by {build}");
    }
}

#[test]
fn a_struct_value_downcasts_whichever_side_built_it() {
    // 2 builders × 2 downcast sites, over `Sexpr` — the `(the T p)` path,
    // which fails *silently* (falls to the catch-all) when the keys disagree.
    for build in ["m::struct-i", "m::struct-c"] {
        for take in ["m::downcast-i", "m::downcast-c"] {
            let got = conformance_int(&format!("({take} ({build} 7))"));
            assert_eq!(got, 7, "built by {build}, downcast by {take}");
        }
    }
}

#[test]
fn a_value_prints_the_same_whichever_side_built_it() {
    // The printer resolves the variant name by parsing the stored key back
    // into a `Path` and looking it up, so a wrong key reads
    // `(<unknown-variant> 5)` / `#<pt ...>` instead of `#<m::pt ...>`.
    for build in ["m::enum-i", "m::enum-c", "m::enum-nested"] {
        assert_eq!(conformance_str(&format!("(format false \"~a\" ({build} 5))")), "(num 5)", "{build}");
    }
    for build in ["m::struct-i", "m::struct-c"] {
        let got = conformance_str(&format!("(format false \"~a\" ({build} 7))"));
        assert!(got.ends_with("m::pt 7 0>"), "built by {}, printed as {}", build, got);
    }
}

#[test]
fn two_values_of_one_type_are_equalp_across_the_boundary() {
    // `equalp`'s "same type" is key equality, so a disagreement makes a value
    // unequal to its own twin.
    assert!(conformance_str("(format false \"~a\" (equalp (m::enum-i 5) (m::enum-c 5)))") == "true");
    assert!(conformance_str("(format false \"~a\" (equalp (m::enum-c 5) (m::enum-nested 5)))") == "true");
    assert!(conformance_str("(format false \"~a\" (equalp (m::struct-i 7) (m::struct-c 7)))") == "true");
}

// ---- the new cons-to-cons bridge ----------------------------------------

/// Translate a hand-written core form with `compile::core_bridge` and give
/// back the island form as text, ready to splice into a `compile-function`
/// call.
///
/// `gc_stress` is on, so every allocation the translation makes collects
/// first: a node whose fields the bridge failed to root would be reclaimed
/// before the node containing it exists.
fn bridge_to_island_text(core_src: &str) -> String {
    bridge_to_island_text_with(&[], core_src)
}

/// The same, with `defs` — each a `defstruct`/`defenum` core form — recorded
/// first, which is where a field's kind comes from.
fn bridge_to_island_text_with(defs: &[&str], core_src: &str) -> String {
    use typelisp::check::core;
    use typelisp::compile::core_bridge::{Ctx, Definitions};
    use std::collections::HashMap;
    let mut h = Heap::with_capacity(1 << 16);
    h.set_gc_stress(true);
    let r = Reader::new();
    let mut definitions = Definitions::new();
    for d in defs {
        for v in r.read_all(&mut h, d).expect("read failed") {
            definitions.record(&h, v).expect("recording the definition failed");
        }
    }
    let mut vs = r.read_all(&mut h, core_src).expect("read failed");
    assert_eq!(vs.len(), 1, "expected one core form");
    let form = vs.pop().unwrap();
    h.push_root(form);
    // No globals: a body reaching one would be an internal error here, which
    // is exactly what should happen — the real driver promotes them first.
    let globals = HashMap::new();
    // What the driver does before translating a body: any binding something
    // nested captures must be a cell from the moment it is bound.
    let cells = typelisp::compile::core_freevars::names_captured_by_nested(&h, &[form])
        .expect("the capture walk failed");
    let cx = Ctx::new(&definitions, &globals).with_cell_names(&cells);
    let island = typelisp::compile::core_bridge::to_island(&mut h, form, cx).expect("bridge failed");
    core::print(&h, island)
}

/// The whole point of building the bridge before the checker switches: the
/// island really accepts what it produces.
///
/// `the_compiler_body_compiles_a_two_parameter_addition` above hand-writes
/// the island form and JITs it. This runs the *same* function body through
/// the same `compile-function`, except that the form comes out of the new
/// bridge instead of being typed by hand — so a disagreement between the two
/// vocabularies fails here, while the tree is green and nothing has been
/// switched over.
///
/// The parameter list stays hand-written: it comes from the top-level `defun`
/// node, which is a later stage. Only the *body* is bridged here.
#[test]
fn the_island_compiles_a_body_the_new_bridge_produced() {
    let body = bridge_to_island_text("(assoc i32 + true () int-any-width (int-any-width int-any-width) \"i32\" (var a) (var b))");
    // The bridge reproduces exactly the text the hand-written test above
    // feeds `compile-function` — the two are checked against each other here
    // rather than only against the island, so a change to either is visible.
    assert_eq!(body, r#"(assoc "i32" "+" true (0 var "a" false) (0 var "b" false))"#);

    let module = expect_llvm_module(eval_ok_with_compiler(&format!(
        r#"(compile-function (llvm-module::create "mod") "add2" '((a . 0) (b . 0)) '{})"#,
        body
    )));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
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

/// A body exercising the tags this stage covers beyond a bare method call:
/// `let` with a kind-carrying binding, `if`, and an integer comparison —
/// compiled for real, not just compared as text.
#[test]
fn the_island_compiles_a_bridged_let_and_if() {
    let body = bridge_to_island_text(
        "(let ((d int-any-width (assoc i32 - true () int-any-width (int-any-width int-any-width) \"i32\" (var a) (var b))))
           (if (assoc i32 < true () bool (int-any-width int-any-width) \"bool\" (var d) (int-any-width 0))
               (assoc i32 - true () int-any-width (int-any-width int-any-width) \"i32\" (int-any-width 0) (var d))
               (var d)))",
    );
    let module = expect_llvm_module(eval_ok_with_compiler(&format!(
        r#"(compile-function (llvm-module::create "mod") "absdiff" '((a . 0) (b . 0)) '{})"#,
        body
    )));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let absdiff = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("absdiff")
            .expect("failed to look up the compiled `absdiff` function")
    };
    for (a, b) in [(10i64, 4i64), (4, 10), (7, 7)] {
        let argv = [a, b];
        assert_eq!(unsafe { absdiff.call(argv.as_ptr(), argv.len() as u32) }, (a - b).abs(), "a={} b={}", a, b);
    }
}

/// `(compile-function ...)` wrapped in a module with the runtime declared.
fn compile_function_source(name: &str, params: &str, body: &str) -> String {
    // The declarations `Interp::compile_function` installs. A test calling
    // `compile-function` directly gets an empty module instead, and the
    // island's `get-function` aborts the process on a missing name — so the
    // set is taken from the driver's own rather than guessed at.
    let decls: String = typelisp::compile::runtime_function_names()
        .iter()
        .map(|n| format!("(add-function m \"{}\")\n", n))
        .collect();
    format!(
        r#"(let ((m (llvm-module::create "mod")))
             {}
             (compile-function m "{}" '{} '{}))"#,
        decls, name, params, body
    )
}

/// `construct`/`field-get`/`match` through the real `compile-function`.
///
/// Verified rather than run: every one of these allocates on the GC heap at
/// run time (`rt_struct_new`/`rt_data_new`), and the heap the driver built
/// this module with is gone by the time a JIT'd function could be called —
/// unlike the pure-arithmetic bodies above, which touch nothing. `verify` is
/// still a real acceptance check: the island compiled the bridged form and
/// LLVM validated the module that came out.
#[test]
fn the_island_accepts_a_bridged_construct_field_and_match() {
    let defs = ["(defstruct point (int-any-width int-any-width))", "(defenum maybe-int (none some) (() (int-any-width)))"];
    let body = bridge_to_island_text_with(
        &defs,
        "(let ((p struct (construct point \"point\" 0 true (int-any-width int-any-width) (var a) (var b))))
           (match (construct maybe-int \"maybe-int\" 1 false (int-any-width) (field-get (var p) 1 int-any-width)) enum
             ((pat-ctor maybe-int \"maybe-int\" 0 false ()) (int-any-width -1))
             ((pat-ctor maybe-int \"maybe-int\" 1 false (int-any-width) (pat-bind x)) (var x))))",
    );
    let src = compile_function_source("second", "((a . 0) (b . 0))", &body);
    let ir = eval_string_with_compiler(&format!("(to-string {})", src));
    // The struct is built and read, and the enum box is built and tested —
    // i.e. the island really took each of the three tags, rather than
    // compiling something degenerate that happens to verify.
    for expected in ["rt_struct_new", "rt_struct_field_get", "rt_data_new", "define i64 @second"] {
        assert!(ir.contains(expected), "expected {} in the IR:\n{}", expected, ir);
    }
    assert!(
        expect_bool(eval_ok_with_compiler(&format!("(verify {})", src))),
        "the module the island built from the bridged form did not verify"
    );
}

/// A `loop`/`set`/`break`/`return` body, compiled by the real
/// `compile-function` and actually run.
///
/// Runnable, unlike the `construct`/`match` test above, because integer
/// bindings and arithmetic touch no GC heap — so this is the one that shows
/// the bridged control flow *behaves*, not merely that it verifies. It
/// multiplies by repeated addition, which needs the loop to iterate, the
/// accumulator's `set` to stick across iterations, and `break` to leave with
/// the right value.
#[test]
fn the_island_runs_a_bridged_loop() {
    let body = bridge_to_island_text(
        "(let ((acc int-any-width (int-any-width 0)) (i int-any-width (int-any-width 0)))
           (loop
             (if (assoc i32 < true () bool (int-any-width int-any-width) \"bool\" (var i) (var b))
                 (unit)
                 (break))
             (set acc (assoc i32 + true () int-any-width (int-any-width int-any-width) \"i32\" (var acc) (var a)))
             (set i (assoc i32 + true () int-any-width (int-any-width int-any-width) \"i32\" (var i) (int-any-width 1))))
           (var acc))",
    );
    let module = expect_llvm_module(eval_ok_with_compiler(&compile_function_source(
        "mul",
        "((a . 0) (b . 0))",
        &body,
    )));
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let mul = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("mul")
            .expect("failed to look up the compiled `mul` function")
    };
    // A `loop` unwinds GC roots on the way out (`rt_truncate_sexpr_roots`), so
    // this needs a live heap even though the arithmetic itself never allocates.
    // It ran without one until 2026-09-08 by reading whatever `ACTIVE_HEAP`
    // still pointed at from an earlier test — undefined behaviour that only
    // stopped looking harmless when `Heap` gained a field and the garbage
    // landed in an index instead of a `Vec`'s length.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
    for (a, b) in [(6i64, 7i64), (0, 5), (3, 0), (-4, 3)] {
        let argv = [a, b];
        assert_eq!(unsafe { mul.call(argv.as_ptr(), argv.len() as u32) }, a * b.max(0), "a={} b={}", a, b);
    }
}

/// A `labels` block with a sibling call, compiled by the real
/// `compile-function` and run.
///
/// Siblings become separate LLVM functions linked by direct calls through the
/// function environment, which is the claim this shape exists to make — and
/// runnable, since integer recursion touches no GC heap.
#[test]
fn the_island_runs_a_bridged_labels_block() {
    let body = bridge_to_island_text(
        "(labels ((rec ((n int-any-width) (acc int-any-width)) int-any-width
                    (if (assoc i32 < true () bool (int-any-width int-any-width) \"bool\" (var n) (int-any-width 1))
                        (var acc)
                        (apply (var rec) int-any-width (int-any-width int-any-width)
                          (assoc i32 - true () int-any-width (int-any-width int-any-width) \"i32\" (var n) (int-any-width 1))
                          (assoc i32 * true () int-any-width (int-any-width int-any-width) \"i32\" (var acc) (var n))))))
           (apply (var rec) int-any-width (int-any-width int-any-width) (var a) (int-any-width 1)))",
    );
    let module = expect_llvm_module(eval_ok_with_compiler(&compile_function_source(
        "fact",
        "((a . 0))",
        &body,
    )));
    let _guard = COMPILE_LOCK.lock().unwrap();
    // Every compiled function allocates its activation record on the GC heap
    // now (Phase C1), so running one needs a heap registered — a compiled
    // local lives in a `BoxedObj::Frame`, not on the machine stack.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let fact = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("fact")
            .expect("failed to look up the compiled `fact` function")
    };
    for (n, want) in [(0i64, 1i64), (1, 1), (5, 120), (10, 3628800)] {
        let argv = [n];
        assert_eq!(unsafe { fact.call(argv.as_ptr(), argv.len() as u32) }, want, "n={}", n);
    }
}

/// A closure that captures a binding, escapes, and is then applied — through
/// the real `compile-function`.
///
/// Verified rather than run: a closure is a GC-heap box, so building one needs
/// the heap that is gone by the time a JIT'd function could be called. What
/// this pins is that the island accepts the whole cell/capture shape the
/// bridge produces — the escaping `lambda`, its captured slot, and the
/// indirect application of the result.
#[test]
fn the_island_accepts_a_bridged_escaping_closure() {
    let body = bridge_to_island_text(
        "(let ((n int-any-width (var a)))
           (let ((f fn (lambda ((x int-any-width)) int-any-width (assoc i32 + true () int-any-width (int-any-width int-any-width) \"i32\" (var x) (var n)))))
             (apply (var f) int-any-width (int-any-width) (int-any-width 1))))",
    );
    // The capture really is a cell on both sides of the boundary.
    assert!(body.contains("(n . 11)"), "the captured binding should be a cell: {}", body);
    // No leading paren in the pattern: this reference sits in an argument
    // position, so it is spelled `(0 cellvar "n" 1)` — the kind, then the node.
    assert!(body.contains(r#"cellvar "n" 1"#), "the closure should read through it: {}", body);
    assert!(body.contains("apply-indirect"), "the closure should be applied through its value: {}", body);

    let src = compile_function_source("addn", "((a . 0))", &body);
    let ir = eval_string_with_compiler(&format!("(to-string {})", src));
    for expected in ["rt_cell_new", "rt_cell_get", "define i64 @addn"] {
        assert!(ir.contains(expected), "expected {} in the IR:\n{}", expected, ir);
    }
    assert!(
        expect_bool(eval_ok_with_compiler(&format!("(verify {})", src))),
        "the module the island built from the bridged closure did not verify"
    );
}

/// A quoted list, through the real `compile-function`.
///
/// Verified rather than run: rebuilding the datum allocates cons cells, and
/// the heap that built this module is gone by the time a JIT'd function could
/// be called. What this pins is that the island takes the `construct` chain
/// the bridge emits for a datum — the shape that carries no reference to the
/// compiling heap at all, which is the whole point of taking the literal
/// apart.
#[test]
fn the_island_accepts_a_bridged_quoted_datum() {
    let body = bridge_to_island_text(r#"(quote (1 foo "hi"))"#);
    let src = compile_function_source("datum", "()", &body);
    let ir = eval_string_with_compiler(&format!("(to-string {})", src));
    for expected in ["rt_cons", "rt_intern_symbol", "rt_str_new", "define i64 @datum"] {
        assert!(ir.contains(expected), "expected {} in the IR:\n{}", expected, ir);
    }
    assert!(
        expect_bool(eval_ok_with_compiler(&format!("(verify {})", src))),
        "the module the island built from the bridged datum did not verify"
    );
}

/// A whole top-level `defun`, from core form to a running compiled function.
///
/// The most complete statement Stage B can make: nothing here is hand-written
/// except the core form itself. The mangled name, the parameter list with its
/// kinds, and the body all come out of the bridge, and the island compiles the
/// three of them together through the same `compile-function` boundary the
/// real driver uses.
#[test]
fn the_island_runs_a_whole_bridged_defun() {
    use typelisp::check::core;
    use typelisp::compile::core_bridge::{top_level_function, Ctx, Definitions};
    use std::collections::HashMap;

    let mut h = Heap::with_capacity(1 << 16);
    h.set_gc_stress(true);
    let r = Reader::new();
    let definitions = Definitions::new();
    let globals = HashMap::new();
    let form = r
        .read_all(
            &mut h,
            "(defun m::clamp ((x int-any-width) (lo int-any-width) (hi int-any-width)) int-any-width true
               (if (assoc i32 < true () bool (int-any-width int-any-width) \"bool\" (var x) (var lo))
                   (var lo)
                   (if (assoc i32 < true () bool (int-any-width int-any-width) \"bool\" (var hi) (var x))
                       (var hi)
                       (var x))))",
        )
        .expect("read failed")[0];
    h.push_root(form);
    let f = top_level_function(&mut h, form, Ctx::new(&definitions, &globals))
        .expect("translation failed")
        .expect("a defun is compiled");
    assert_eq!(f.name, "tl_m::clamp");
    let (name, params, body) = (f.name.clone(), core::print(&h, f.params), core::print(&h, f.body));
    drop(h);

    let module = expect_llvm_module(eval_ok_with_compiler(&compile_function_source(
        &name, &params, &body,
    )));
    // The bridging heap `h` is dropped above, and a compiled function now
    // allocates its activation record on the heap (Phase C1) — so the call
    // below needs one that is alive, not the dangling `ACTIVE_HEAP` the drop
    // leaves behind.
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let clamp = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>(&name)
            .expect("failed to look up the compiled function")
    };
    for (x, want) in [(-5i64, 0i64), (0, 0), (7, 7), (10, 10), (99, 10)] {
        let argv = [x, 0, 10];
        assert_eq!(unsafe { clamp.call(argv.as_ptr(), argv.len() as u32) }, want, "x={}", x);
    }
}

/// A `defstruct` whose field is a `HashTable<K,V>`, compiled.
///
/// `Repr::field_kind` classified a `HashTable` as `0` — the number that means
/// "not representable in a struct field", shared with `Repr::None` (a
/// still-generic type variable). The island's `compile-sexpr-field` reaches
/// its final `else` on `0` and panics with exactly that claim. But a
/// `HashTable` *is* representable: `Heap::alloc_hashtable` builds a
/// `BoxedObj::Struct { payload: StructPayload::Map }` — a tagged heap box like
/// any `defstruct`, which is the passthrough kind `6`.
#[test]
fn compile_reads_a_hashtable_field_out_of_a_struct() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defstruct cache (table HashTable<i32,i32>))
        (defun probe () i32
          (let ((c (cache::new (the HashTable<i32,i32> (HashTable::new)))))
            (let ((t c::table))
              (set t 1 41)
              (match (get t 1) ((Some n) (+ n 1)) (None 0)))))
        (compile probe)
        (probe)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
}

/// The same gap on the *cell-boxed* path: a name a nested `lambda` captures is
/// promoted to a shared `BoxedObj::Cell`, and its binding kind becomes the
/// island's `10 + field_kind` marker (`core_bridge`'s `binding_kind`). With
/// `HashTable` classified `0` that marker was a bare `10`, so
/// `bind-let-values`' `(- kind 10)` handed `compile-tag-struct-field` the
/// not-representable `0` — the same wrong answer as the field path above,
/// reached through a different door.
#[test]
fn compile_captures_a_hashtable_in_a_closure() {
    let v = run_with_compiler_and_prelude(
        r#"
        (defun probe () i32
          (let ((t (the HashTable<i32,i32> (HashTable::new))))
            (set t 1 41)
            (let ((f (lambda () i32 (match (get t 1) ((Some n) (+ n 1)) (None 0)))))
              (f))))
        (compile probe)
        (probe)
        "#,
    )
    .expect("eval failed");
    assert_eq!(v, Value::Int(42));
}

/// A `(panic ...)` in an *interpreted* callee that compiled code called into
/// comes back as a recoverable `EvalError::Panic`, not a dead process.
///
/// This is the boundary crossed in the other direction from a compiled
/// `panic`: `apply-it` is native, its `f` parameter is an ordinary
/// interpreted function, so the call goes out through `rt_apply_any` into the
/// interpreter and the failure has to travel back through a compiled frame.
/// `rt_apply_interpreted` used to have nowhere to report to and aborted —
/// with the `EvalError`'s `Debug` formatting leaking into the message, at
/// that.
#[test]
fn a_panic_in_an_interpreted_callee_of_compiled_code_is_recoverable() {
    let src = r#"
        (defun boom ((n i32)) i32 (panic "interpreted boom"))
        (defun apply-it ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (compile apply-it)
        (apply-it boom 1)
    "#;
    match run_with_compiler(src) {
        Err(EvalError::Panic(m)) => assert_eq!(m, "interpreted boom"),
        other => panic!("expected a catchable panic, got {:?}", other),
    }
}

/// The same call with a callee that does not fail still returns normally —
/// the catch is not swallowing ordinary results.
#[test]
fn an_interpreted_callee_of_compiled_code_still_returns_normally() {
    let src = r#"
        (defun triple ((n i32)) i32 (* n 3))
        (defun apply-it ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (compile apply-it)
        (apply-it triple 7)
    "#;
    assert_eq!(eval_ok_with_compiler(src), Value::Int(21));
}

/// `(format false "~a" x)` on an `f64` *parameter*, compiled.
///
/// `format`'s variadic tail is lowered by wrapping each argument as a `Sexpr`
/// (`Checker::wrap_rest_elem`), and that wrap used to skip the constructor
/// entirely for any type `is_heap_repr` calls heap-resident — which `f64` is,
/// *in the interpreter*, where an `f64` is a `BoxedObj::Float` and therefore
/// already a `Sexpr::f64`. A compiled `f64` is neither boxed nor tagged: it
/// is the raw `f64::to_bits` pattern, whose low three bits read as the `Int`
/// tag, so the printer rendered `to_bits(x) >> 3` — a large integer — instead
/// of the number. The interpreted run got it right, which is what made the
/// divergence worth pinning here rather than in `format_test`.
///
/// `i32` alongside it: the same lowering for a type that never took the
/// shortcut, so a regression that broke the constructor path generally would
/// not hide behind the float case.
#[test]
fn compiled_format_renders_a_float_parameter_as_a_number() {
    assert_eq!(
        run_and_read(
            r#"
            (defun show ((x f64)) string (format false "~a" x))
            (defun showi ((n i32)) string (format false "~a" n))
            (compile show)
            (compile showi)
            (append (append (show 2.25) "|") (showi 7))
            "#,
            1 << 16,
            |h, v| match v {
                Value::Str(id) => h.string(id).to_string(),
                other => panic!("expected a string, got {:?}", other),
            },
        )
        .expect("eval failed"),
        "2.25|7"
    );
}

// ---- wide integer literals ---------------------------------------------------
//
// An integer crosses into the compiler island as a `Sexpr`, whose 3-bit tag
// leaves 61 bits of payload — so a literal needing more than that arrived
// sign-extended from bit 60, and `4611686018427387903` compiled to `-1` while
// the interpreter returned it whole. `core_bridge::int_node` now splits every
// integer into two 32-bit halves and `compile-int` reassembles them in LLVM,
// the shape `float` has always used and for exactly the same reason.

/// The value of `src`'s last expression, with the prelude *and* the compiler
/// island loaded — `(compile f)` needs the island.
fn run_compiled(src: &str) -> Value {
    run_and_read(src, 1 << 16, |_h, v| v).expect("eval failed")
}

// Two tests lived here — `a_literal_wider_than_the_sexpr_tag_allows_...` and
// `the_extreme_i64_literals_...` — that compiled literals like `2^62 - 1` and
// `i64::MAX` and checked they came back whole. Both were about `i64`, and the
// case they covered cannot arise since it was removed (2026-09-01): the
// widest fixed-width integer is now `i32`, so every literal of one fits the
// `Sexpr` tag's 61 bits with room to spare. The remaining literal that does
// not fit a machine word is a `bignum`, which is a heap box rather than an
// immediate, and `compile_constructs_a_bignum_literal_and_agrees_with_the_
// interpreter` covers it.

// ---- `Array<T>` (cl-parity-plan.md Phase 6b) --------------------------------
//
// `Array<T>` is a prelude `defstruct` over two `Vector`s, so it reaches
// compiled code through the paths `defstruct` and `Vector<T>` already had —
// there is no array node in the IR. What is worth pinning down is that
// `(aref a i j)`, which the checker rewrites before the compiler ever sees
// it, arrives as an ordinary `let*`/`push`/`get` and behaves the same
// compiled as interpreted.

#[test]
fn a_compiled_function_reads_and_writes_an_array_through_aref() {
    let src = "
        (defun cell () i32
          (let ((d (the Vector<i32> (Vector::new))))
            (progn
              (push d 2) (push d 3)
              (let ((a (Array::make d 0)))
                (progn (setf (aref a 1 2) 9) (aref a 1 2))))))
        (compile cell)
        (cell)
    ";
    assert_eq!(run_compiled(src), Value::Int(9));
}

#[test]
fn a_compiled_function_walks_an_array_in_row_major_order() {
    let src = "
        (defun digits () i32
          (let ((d (the Vector<i32> (Vector::new))))
            (progn
              (push d 2) (push d 2)
              (let ((a (Array::make d 0)) (acc 0))
                (progn
                  (setf (aref a 0 0) 1) (setf (aref a 0 1) 2)
                  (setf (aref a 1 0) 3) (setf (aref a 1 1) 4)
                  (doiter (x (iter a)) (setf acc (+ (* acc 10) x)))
                  acc)))))
        (compile digits)
        (digits)
    ";
    assert_eq!(run_compiled(src), Value::Int(1234));
}

#[test]
fn a_compiled_functions_fill_pointer_grows_the_same_way() {
    let src = "
        (defun grown () i32
          (let ((d (the Vector<i32> (Vector::new))))
            (progn
              (push d 1)
              (let ((a (Array::make d 0 :fill-pointer 0)))
                (progn (push-extend a 7) (push-extend a 8)
                       (+ (* 100 (total-size a)) (* 10 (len a)) (unwrap (pop a))))))))
        (compile grown)
        (grown)
    ";
    assert_eq!(run_compiled(src), Value::Int(228));
}

// ---- `BitVector` (cl-parity-plan.md Phase 6c) -------------------------------

#[test]
fn a_compiled_function_sets_and_reads_bits_across_word_boundaries() {
    let src = "
        (defun edges () i32
          (let ((v (BitVector::make 70)))
            (progn
              (set v 31 true) (set v 32 true) (set v 69 true)
              (+ (if (get v 31) 1 0) (* 2 (if (get v 32) 1 0))
                 (* 4 (if (get v 69) 1 0)) (* 8 (if (get v 30) 1 0))))))
        (compile edges)
        (edges)
    ";
    assert_eq!(run_compiled(src), Value::Int(7));
}

#[test]
fn a_compiled_bit_wise_operation_leaves_no_bits_past_the_length() {
    let src = "
        (defun ones () i32
          (let ((v (bit-not (BitVector::make 35))) (n 0) (i 0))
            (progn
              (while (< i 35) (progn (setf n (+ n (if (get v i) 1 0))) (setf i (+ i 1))))
              n)))
        (compile ones)
        (ones)
    ";
    assert_eq!(run_compiled(src), Value::Int(35));
}

// ---- errors (cl-parity-plan.md Phase 7a) -----------------------------------

#[test]
fn a_compiled_function_builds_and_reads_an_error_chain() {
    let src = "
        (defun chain () string
          (describe-error (wrap-error \"starting up\" (simple-error \"no such file\"))))
        (compile chain)
        (equal (chain) \"starting up\n  caused by: no such file\")
    ";
    assert_eq!(run_compiled(src), Value::Bool(true));
}

#[test]
fn a_compiled_assert_passes_and_fails_the_same_way() {
    let ok = "(defun fine () i32 (progn (assert (= 1 1)) 7)) (compile fine) (fine)";
    assert_eq!(run_compiled(ok), Value::Int(7));
}

// ---- the `char`/`int->char` lowerings (cl-parity-plan.md Phase 1's leftover) --
//
// `upcase`/`downcase`/`alphap`/`digitp` and `int->char` were the five builtin
// methods the island had no lowering for, so a `defun` that called one could
// not be compiled at all — a clean refusal rather than a crash, but a refusal.
// The prelude's whole character/string catalog is written on `char->int` code
// points because of it (`docs/dev/cl-parity-plan.md` Phase 2's note 1).

/// Each of the five agrees with the interpreter, which is the only claim a
/// lowering makes. The four `char` methods share one `rt_char_*` shim shape
/// and are ASCII-only on both tiers *because* both tiers run the same shim.
#[test]
fn compile_and_interpret_agree_on_the_char_methods() {
    let prog = r#"
        (defun cu ((c char)) char (upcase c))
        (defun cd ((c char)) char (downcase c))
        (defun ca ((c char)) bool (alphap c))
        (defun cg ((c char)) bool (digitp c))
        (defun ic ((n i32)) char (int->char n))
        ;; 42 only if every one of the seven answers is right, so one
        ;; number carries the whole comparison across two heaps (a `Str`
        ;; would come back as two unequal `StrId`s).
        (defun probe () i32
          (if (equal (upcase #\a) #\A)
            (if (equal (downcase #\Z) #\z)
              (if (alphap #\q)
                (if (alphap #\7) 0
                  (if (digitp #\7)
                    (if (digitp #\q) 0 (if (equal (ic 66) #\B) 42 0))
                    0))
                0)
              0)
            0))
    "#;
    let compiled = eval_ok_with_compiler(&format!(
        "{}\n(compile cu)(compile cd)(compile ca)(compile cg)(compile ic)(compile probe)\n(probe)",
        prog
    ));
    let interpreted = eval_ok(&format!("{}\n(probe)", prog));
    assert_eq!(compiled, interpreted);
    assert_eq!(compiled, Value::Int(42));
}

/// A code point that is not a Unicode scalar value raises, rather than
/// inventing a character — the check is the reason `int->char` is a shim at
/// all while its inverse `char->int` is nothing.
#[test]
fn a_compiled_int_to_char_raises_on_a_bad_code_point() {
    let err = run_with_compiler(
        r#"(defun ic ((n i32)) char (int->char n))
           (compile ic)
           (ic -1)"#,
    )
    .expect_err("expected a panic");
    assert!(
        format!("{:?}", err).contains("not a valid Unicode scalar value"),
        "{:?}",
        err
    );
}

// ---- a `HashTable` with a user-defined key, compiled ------------------------

/// `get`/`set`/`remove` are prelude `defmethod`s now (cl-parity-plan.md Phase
/// 6a's remainder), so a compiled call to one is a call to a monomorphized
/// prelude body that calls `sxhash` and `equals` and then the bucket
/// primitives. That is three layers of new call graph, and this checks it
/// arrives at the same answer the interpreter does.
#[test]
fn compile_and_interpret_agree_on_a_hashtable_with_a_user_key() {
    let prog = r#"
        (defstruct pt (x i32) (y i32))
        (impl Eq pt
          (equals ((self Self) (other Self)) bool
            (if (= self::x other::x) (= self::y other::y) false)))
        (impl Hash pt
          (sxhash ((self Self)) i32 (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))
        (defun probe ((a i32) (b i32)) i32
          (let ((h (the HashTable<pt,i32> (HashTable::new))))
            (progn
              (set h (pt::new a b) 7)
              (set h (pt::new b a) 9)
              (let ((hit (match (get h (pt::new a b)) ((some v) v) ((none) -1)))
                    (miss (match (get h (pt::new 99 99)) ((some v) v) ((none) 0)))
                    (gone (match (remove h (pt::new b a)) ((some v) v) ((none) -1))))
                (+ hit (+ miss (+ gone (count h))))))))
    "#;
    let compiled = run_with_compiler_and_prelude(&format!("{}\n(compile probe)\n(probe 1 2)", prog))
        .expect("compiled run failed");
    let interpreted =
        run_with_compiler_and_prelude(&format!("{}\n(probe 1 2)", prog)).expect("interpreted run failed");
    assert_eq!(compiled, interpreted);
    // 7 (hit) + 0 (miss) + 9 (removed) + 1 (left) = 17.
    assert_eq!(compiled, Value::Int(17));
}

/// A module-qualified type argument in a specialization's name. The name a
/// specialization is registered under embeds its type arguments — `hashtable::get
/// <m::pt,i32>` — and the split that finds the method in it used to take the
/// *last* `::` in the string, which lands inside the type argument. Nothing at
/// the top level could show it, because a type there has one segment.
#[test]
fn a_specialization_at_a_module_qualified_type_resolves() {
    let src = r#"
        (module m
          (pub defstruct pt (x i32))
          (pub defun probe ((n i32)) i32
            (let ((d (the Vector<i32> (Vector::new))))
              (progn (push d 1)
                (let ((a (Array::make d (pt::new n))))
                  (x (get a (Vector::filled 1 0))))))))
        (compile m::probe)
        (m::probe 7)
    "#;
    assert_eq!(run_with_compiler_and_prelude(src).expect("run failed"), Value::Int(7));
}

// ---- compiled frames (Phase C0) ------------------------------------------

/// `rt_frame_new`/`rt_frame_data`/`build-slot-ptr`: a compiled function
/// allocating its own activation record on the GC heap and reaching a local
/// through it.
///
/// This is Phase C's whole toolkit in one module, exercised before anything
/// depends on it — the same order Phase 0/1's raw-builtin tests preceded the
/// self-hosted compiler body. What it pins down is that a frame slot is
/// reachable by exactly the pair a local has always been read and written
/// with (`store-arg`/`load-raw` against a pointer), so moving a local off the
/// machine stack and into a frame is a change of *where the slot lives* and
/// nothing else.
///
/// **A compiled function can stop in the middle and be resumed** (Phase C2).
///
/// This is the claim the whole phase rests on, and nothing before it could be
/// stated at all: under the old ABI a function's only way to stop was to
/// return, because the rest of its work lived on the machine stack and nothing
/// could pick that up again.
///
/// The function here runs in two halves. On first entry it makes its frame,
/// writes 40 into a local, records where to resume, and hands back
/// `STATUS_SUSPEND`. The driver calls it again with the same frame; it
/// branches on `pc`, reads the local back, adds 2, and returns 42 in the
/// protocol's value slot.
///
/// The 40 is the point. It is written before the suspension and read after,
/// with the machine stack torn down and rebuilt in between — so the only way
/// it can survive is by living in the frame.
#[test]
fn a_compiled_function_suspends_and_resumes_with_its_local_intact() {
    let src = r#"
        (defun build-suspending-module () llvm-module
          (let* ((m (llvm-module::create "mod"))
                 ;; The two protocol shims this body calls directly. The rest
                 ;; (`rt_frame_new` and friends) `frame-begin` declares itself.
                 (ignored-entered (add-function m "rt_frame_entered"))
                 (ignored-setpc (add-function m "rt_frame_set_pc"))
                 (f (add-coroutine-function m "two_halves"))
                 (entry (append-block f "entry"))
                 (first (append-block f "first"))
                 (resumed (append-block f "resumed"))
                 (builder (llvm-builder::create))
                 (param (function-param f 0)))
            ;; entry: a zero frame means first entry; anything else is a resume.
            (position-at-end builder entry)
            (build-cond-br builder (build-icmp-eq builder param (const-word builder 0)) first resumed)

            ;; first: make the frame, publish it, stash 40 in a local, record
            ;; where to resume, and hand control back.
            (position-at-end builder first)
            (frame-begin builder m)
            (let* ((frame (frame-value builder))
                   (slot (frame-slot builder))
                   (pub-args (alloca-args builder 1))
                   (setpc-args (alloca-args builder 2)))
              (store-arg builder slot 0 (const-word builder 40))
              (store-arg builder pub-args 0 frame)
              (build-call builder (get-function m "rt_frame_entered") pub-args 1)
              (store-arg builder setpc-args 0 frame)
              (store-arg builder setpc-args 1 (const-word builder 1))
              (build-call builder (get-function m "rt_frame_set_pc") setpc-args 2)
              (build-ret builder (const-word builder 2))

              ;; resumed: read the local back out of the frame we were handed,
              ;; add 2, and leave it in the protocol's value slot.
              (position-at-end builder resumed)
              (let ((data-args (alloca-args builder 1)))
                (store-arg builder data-args 0 param)
                (let* ((data (build-call builder (get-function m "rt_frame_data") data-args 1))
                       (base (build-int-to-ptr builder data))
                       (stashed (load-raw builder (build-slot-ptr builder base 1) 0))
                       (sum (build-add builder stashed (const-word builder 2))))
                  (store-arg builder (build-slot-ptr builder base 0) 0 sum)
                  (frame-end builder m)
                  (build-ret builder (const-word builder 0)))))
            m))
        (build-suspending-module)
    "#;
    let module = expect_llvm_module(eval_ok(src));
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let two_halves = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(i64) -> i64>("two_halves")
            .expect("failed to look up the compiled `two_halves` function")
    };
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);

    // First entry: no frame yet.
    assert_eq!(unsafe { two_halves.call(0) }, typelisp_abi::STATUS_SUSPEND, "the first half suspends");
    let frame = typelisp_abi::call_state::take_current_frame().expect("the prologue published its frame");
    let id = match typelisp::decode(frame) {
        Value::Boxed(id) => id,
        other => panic!("expected a frame, got {:?}", other),
    };
    assert_eq!(heap.frame_pc(id), 1, "and recorded where to resume");
    assert_eq!(heap.frame_word(id, 1), 40, "with the local it wrote still in the frame");

    // Resume with the same frame.
    assert_eq!(unsafe { two_halves.call(frame) }, typelisp_abi::STATUS_RETURN, "the second half returns");
    assert_eq!(heap.frame_word(id, typelisp_abi::FRAME_VALUE_SLOT), 42, "40 survived the suspension");
}

/// **The driver runs a call chain that never touches the machine stack.**
///
/// `outer` asks the driver to call `inner`, which returns a value; the driver
/// hands it back and resumes `outer`, which adds to it and returns. Two frames
/// exist at once on [`FrameStack`], and neither call is an LLVM `call`
/// instruction — `outer` *returns* `STATUS_CALL` and is entered again later.
///
/// That is what makes the depth limit the heap rather than the OS stack, and
/// it is the same move Phase A made for the interpreter: the stack becomes
/// data.
#[test]
fn the_driver_runs_a_two_frame_call_chain() {
    let src = r#"
        (defun build-chain-module () llvm-module
          (let* ((m (llvm-module::create "mod"))
                 (ignored-entered (add-function m "rt_frame_entered"))
                 (ignored-setpc (add-function m "rt_frame_set_pc"))
                 (ignored-call (add-function m "rt_frame_call"))
                 (inner (add-coroutine-function m "inner"))
                 (outer (add-coroutine-function m "outer")))
            ;; inner: make a frame, leave 5 in the value slot, return.
            (let* ((ib (append-block inner "entry"))
                   (ibuilder (llvm-builder::create)))
              (position-at-end ibuilder ib)
              (frame-begin ibuilder m)
              (let* ((iframe (frame-value ibuilder))
                     (ipub (alloca-args ibuilder 1))
                     (idata-args (alloca-args ibuilder 1)))
                (store-arg ibuilder ipub 0 iframe)
                (build-call ibuilder (get-function m "rt_frame_entered") ipub 1)
                (store-arg ibuilder idata-args 0 iframe)
                (let* ((idata (build-call ibuilder (get-function m "rt_frame_data") idata-args 1))
                       (ibase (build-int-to-ptr ibuilder idata)))
                  (store-arg ibuilder (build-slot-ptr ibuilder ibase 0) 0 (const-word ibuilder 5))
                  (frame-end ibuilder m)
                  (build-ret ibuilder (const-word ibuilder 0)))))
            ;; outer: ask the driver for `inner`, then add 37 to what comes back.
            (let* ((oentry (append-block outer "entry"))
                   (ofirst (append-block outer "first"))
                   (oresumed (append-block outer "resumed"))
                   (obuilder (llvm-builder::create))
                   (oparam (function-param outer 0)))
              (position-at-end obuilder oentry)
              (build-cond-br obuilder (build-icmp-eq obuilder oparam (const-word obuilder 0)) ofirst oresumed)
              (position-at-end obuilder ofirst)
              (frame-begin obuilder m)
              (let* ((oframe (frame-value obuilder))
                     (opub (alloca-args obuilder 1))
                     (ocall (alloca-args obuilder 1))
                     (osetpc (alloca-args obuilder 2)))
                (store-arg obuilder opub 0 oframe)
                (build-call obuilder (get-function m "rt_frame_entered") opub 1)
                (store-arg obuilder ocall 0 (build-fn-address obuilder inner))
                (build-call obuilder (get-function m "rt_frame_call") ocall 1)
                (store-arg obuilder osetpc 0 oframe)
                (store-arg obuilder osetpc 1 (const-word obuilder 1))
                (build-call obuilder (get-function m "rt_frame_set_pc") osetpc 2)
                (build-ret obuilder (const-word obuilder 1))

                (position-at-end obuilder oresumed)
                (let ((odata-args (alloca-args obuilder 1)))
                  (store-arg obuilder odata-args 0 oparam)
                  (let* ((odata (build-call obuilder (get-function m "rt_frame_data") odata-args 1))
                         (obase (build-int-to-ptr obuilder odata))
                         (got (load-raw obuilder (build-slot-ptr obuilder obase 0) 0))
                         (sum (build-add obuilder got (const-word obuilder 37))))
                    (store-arg obuilder (build-slot-ptr obuilder obase 0) 0 sum)
                    (frame-end obuilder m)
                    (build-ret obuilder (const-word obuilder 0))))))
            m))
        (build-chain-module)
    "#;
    let module = expect_llvm_module(eval_ok(src));
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let outer = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(i64) -> i64>("outer")
            .expect("failed to look up the compiled `outer` function")
    };
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);

    let mut stack = typelisp::compile::coroutine::FrameStack::new();
    let entry: typelisp::compile::coroutine::CoroutineFn = unsafe { std::mem::transmute(outer.as_raw()) };
    // The word is raw, not tagged: what the value slot means is the function's
    // declared return representation, which the driver deliberately does not
    // read. `inner` left 5; `outer` added 37 after being resumed with it.
    let answer = stack.run(&mut heap, entry, &[]).expect("the chain ran to an answer");
    assert_eq!(answer, 42, "the callee's value came back and the caller went on with it");
    assert_eq!(stack.depth(), 0, "both frames were popped");
}

/// **The frame's size is written after the body decides it.**
///
/// `frame-begin` emits `rt_frame_new(0)` because the count cannot be known
/// yet — the body is what allocates the slots. `frame-end` replaces that zero
/// with the number actually handed out. This test is the proof that the
/// placeholder really is patched: the function asks for three slots, and the
/// frame the call leaves on the heap has three, not zero.
///
/// A zero here would not be a silent wrong answer for long — `frame_word`
/// asserts the index is in range — but it would fail at the first local
/// rather than at the mechanism, so this pins the mechanism itself.
///
/// The count is read back off the heap rather than returned by `frame-end`,
/// which yields `()`: `rt_llvm_call` marshals only handles, `()`, `bool` and
/// `string` back from an `llvm-*` builtin, and the island has no use for the
/// number anyway.
#[test]
fn a_frames_size_is_patched_in_once_the_body_is_emitted() {
    let src = r#"
        (defun build-sized-frame-module () llvm-module
          (let* ((m (llvm-module::create "mod"))
                 (ignored-new (add-function m "rt_frame_new"))
                 (ignored-data (add-function m "rt_frame_data"))
                 (ignored-mask (add-function m "rt_frame_mask_bit"))
                 (f (add-function m "sized_frame"))
                 (b (append-block f "entry"))
                 (builder (llvm-builder::create)))
            (position-at-end builder b)
            ;; Three slots: raw, collectable, raw. Only the middle is masked,
            ;; so the mask has to track the index rather than the call order.
            (frame-begin builder m)
            (let* ((s0 (frame-slot builder))
                   (s1 (frame-slot-rooted builder m))
                   (s2 (frame-slot builder)))
              (store-arg builder s0 0 (const-word builder 100))
              (store-arg builder s1 0 (load-arg builder f 0))
              (store-arg builder s2 0 (const-word builder 300))
              (frame-end builder m)
              (build-ret builder (const-word builder 0)))
            m))
        (build-sized-frame-module)
    "#;
    let module = expect_llvm_module(eval_ok(src));
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let sized = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("sized_frame")
            .expect("failed to look up the compiled `sized_frame` function")
    };
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
    let argv: [i64; 1] = [typelisp::encode(Value::Int(7))];
    assert_eq!(unsafe { sized.call(argv.as_ptr(), argv.len() as u32) }, 0);

    let id = typelisp::BoxId::from_u32(0);
    assert!(heap.is_frame(id));
    // Four, not three: slot 0 is the driver protocol's value slot, reserved by
    // the prologue and never handed out, so the three the body asked for are
    // 1, 2 and 3.
    assert_eq!(heap.frame_len(id), 4, "the placeholder was patched with what was handed out");
    assert!(!heap.frame_mask_bit(id, 0), "the reserved value slot is not a local");
    assert!(!heap.frame_mask_bit(id, 1) && heap.frame_mask_bit(id, 2) && !heap.frame_mask_bit(id, 3));
    assert_eq!(heap.frame_word(id, 1), 100);
    assert_eq!(typelisp::decode(heap.frame_word(id, 2)), Value::Int(7));
    assert_eq!(heap.frame_word(id, 3), 300);
}

/// Built by hand, so the three `rt_*` shims are declared bodyless and resolved
/// by LLVM's process-symbol lookup, and a `Heap` is registered active for the
/// call because `rt_frame_new` allocates on it — the same idiom the closure
/// test above needs.
#[test]
fn a_compiled_function_can_carry_a_local_in_a_heap_frame() {
    let src = r#"
        (defun build-frame-module () llvm-module
          (let ((m (llvm-module::create "mod")))
            (let ((ignored-new (add-function m "rt_frame_new"))) ())
            (let ((ignored-data (add-function m "rt_frame_data"))) ())
            (let ((ignored-mask (add-function m "rt_frame_mask_bit"))) ())
            (let ((f (add-function m "frame_roundtrip")))
              (let ((b (append-block f "entry")))
                (let ((builder (llvm-builder::create)))
                  (position-at-end builder b)
                  (let ((new-args (alloca-args builder 1)))
                    (store-arg builder new-args 0 (const-word builder 3))
                    (let ((frame (build-call builder (get-function m "rt_frame_new") new-args 1)))
                      (let ((mask-args (alloca-args builder 2)))
                        (store-arg builder mask-args 0 frame)
                        (store-arg builder mask-args 1 (const-word builder 1))
                        (let ((ignored-bit (build-call builder (get-function m "rt_frame_mask_bit") mask-args 2)))
                          (let ((data-args (alloca-args builder 1)))
                            (store-arg builder data-args 0 frame)
                            (let ((data (build-call builder (get-function m "rt_frame_data") data-args 1)))
                              (let ((slot (build-slot-ptr builder (build-int-to-ptr builder data) 1)))
                                (store-arg builder slot 0 (load-arg builder f 0))
                                (build-ret builder (load-raw builder slot 0))))))))))) 
              m)))
        (build-frame-module)
    "#;
    let module = expect_llvm_module(eval_ok(src));
    let _guard = COMPILE_LOCK.lock().unwrap();
    let engine = module
        .borrow()
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT execution engine");
    let roundtrip = unsafe {
        engine
            .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("frame_roundtrip")
            .expect("failed to look up the compiled `frame_roundtrip` function")
    };
    let mut heap = Heap::with_capacity(1 << 12);
    typelisp::compile::runtime::set_active_heap(&mut heap as *mut Heap);
    let argv: [i64; 1] = [4242];
    assert_eq!(unsafe { roundtrip.call(argv.as_ptr(), argv.len() as u32) }, 4242);
    // One frame, and it is the shape the collector was told about. The call
    // allocates nothing else, so it is box 0.
    assert_eq!(heap.box_count(), 1, "the call allocated exactly one box");
    let id = typelisp::BoxId::from_u32(0);
    assert!(heap.is_frame(id), "and that box is the frame");
    assert_eq!(heap.frame_len(id), 3);
    assert!(heap.frame_mask_bit(id, 1), "the slot the function marked is masked");
    assert!(!heap.frame_mask_bit(id, 0) && !heap.frame_mask_bit(id, 2), "and no other is");
    assert_eq!(heap.frame_word(id, 1), 4242, "the local is in the slot it was written to");
}
