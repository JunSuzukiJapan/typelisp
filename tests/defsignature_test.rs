//! Tests for `defsignature` — explicit forward declaration of a top-level
//! `defun`.
//!
//! Top-level definitions are checked and executed one form at a time, in
//! source order, so a body can only call a name the checker has already seen.
//! Mutual recursion therefore has to be *announced*, and `defsignature` is
//! how. It replaces the implicit pre-pass this checker used to run over a
//! whole file before checking any of it (`predeclare_program`): that pass
//! required reading every form before checking the first, which is exactly
//! what a reader macro cannot allow.
//!
//! Two things follow from the declaration being written rather than derived,
//! and both are tested here: it can *disagree* with the definition, and it
//! can go unfulfilled. Neither was possible before, because the old pass read
//! the signature off the definition itself.
//!
//! `run`/`eval_string` mirror the real drivers: read the whole program, then
//! check/exec form by form, in order, with nothing looking ahead.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Error, Heap, Interp, Reader, Value};

/// Read, then check+exec every form in order — the shape every real driver
/// has. `with_island` additionally loads the prelude and the compiler island,
/// which anything building a closure or calling `(compile ...)` needs (a
/// closure value JIT-compiles when it can, and compiling needs the island).
fn run_inner(src: &str, with_island: bool) -> Result<Value, Error> {
    let mut h = Heap::with_capacity(1 << 18);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    if with_island {
        load_prelude(&mut h, &mut chk, &mut interp);
        load_compiler(&mut h, &mut chk, &mut interp);
    }
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?;
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    chk.finish_unit().map_err(Error::into_kind)?;
    Ok(last)
}

fn run(src: &str) -> Result<Value, Error> {
    run_inner(src, false)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

fn eval_ok_with_island(src: &str) -> Value {
    run_inner(src, true).expect("eval failed")
}

/// The text of a `string` result. A `string` is a heap `Value::Str` since the
/// scalar unification, so reading one needs the heap it lives in — and
/// `run_inner` drops its heap on return, hence this parallel runner.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 18);
    let r = Reader::new();
    let mut chk = Checker::new();
    let interp = Interp::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        typelisp::Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

fn expect_err(src: &str) -> String {
    match run(src) {
        Ok(v) => panic!("expected an error, got {:?}", v),
        Err(e) => e.to_string(),
    }
}

/// Collects the warnings a program produces (rather than its value) — for
/// asserting that a genuine redefinition still reports.
fn warnings_of(src: &str) -> Vec<String> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut out = Vec::new();
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        out.extend(chk.take_warnings());
        interp.exec(&mut h, tl).expect("eval failed");
    }
    out
}

// ---- what a declaration buys ---------------------------------------------

#[test]
fn a_declared_defun_can_be_called_before_it_is_defined() {
    let src = "(defsignature b () i32) (defun a () i32 (b)) (defun b () i32 7) (a)";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn two_top_level_defuns_can_be_mutually_recursive() {
    // The case `Interp::compute_sccs`'s own test could only reach by
    // hand-building `FnDef`s and bypassing the checker entirely (see
    // `scc_tests` in `eval/interp.rs`): surface syntax can express it, once
    // the ring is announced.
    let src = "
        (defsignature odd2? (i32) bool)
        (defun even2? ((n i32)) bool (if (= n 0) true  (odd2? (- n 1))))
        (defun odd2?  ((n i32)) bool (if (= n 0) false (even2? (- n 1))))
        (even2? 10)";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn mutual_recursion_still_holds_after_compiling_one_of_the_pair() {
    // Exercises the JIT's transitive/SCC path over a genuine cycle that came
    // from source rather than from a hand-built `FnDef`.
    let src = "
        (defsignature odd2? (i32) bool)
        (defun even2? ((n i32)) bool (if (= n 0) true  (odd2? (- n 1))))
        (defun odd2?  ((n i32)) bool (if (= n 0) false (even2? (- n 1))))
        (compile even2?)
        (even2? 11)";
    assert_eq!(eval_ok_with_island(src), Value::Bool(false));
}

#[test]
fn a_forward_call_takes_its_type_from_the_declaration() {
    // The caller is checked before the callee's body ever is, so the only
    // thing that can give `(b)` its type is the declaration.
    let src = "(defsignature b () string) (defun a () string (append (b) \"!\")) (defun b () string \"hi\") (a)";
    assert_eq!(eval_string(src), "hi!");
}

#[test]
fn a_declared_rest_argument_is_collected_at_the_call() {
    let src = "
        (defsignature total (&rest i32) i32)
        (defun a () i32 (total 1 2 3))
        (defun total (&rest (xs i32)) i32 (sexpr-list-length-i32 xs))
        (a)";
    assert_eq!(eval_ok_with_island(src), Value::Int(3));
}

#[test]
fn a_wrong_argument_type_to_a_forward_call_is_a_type_error() {
    // The declaration must be the real signature, not a permissive
    // placeholder — otherwise forward calls would silently skip checking.
    let msg = expect_err("(defsignature b (i32) i32) (defun a () i32 (b \"nope\")) (defun b ((n i32)) i32 n) (a)");
    assert!(msg.contains("type"), "expected a type error, got: {}", msg);
}

#[test]
fn declarations_work_inside_a_nested_module() {
    let src = "
        (module m
          (pub defsignature b () i32)
          (pub defun a () i32 (b))
          (pub defun b () i32 42))
        (m::a)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn a_lambda_body_can_call_a_declared_defun() {
    let src = "
        (defsignature b () i32)
        (defun a () i32 (let ((f (lambda () i32 (b)))) (f)))
        (defun b () i32 5)
        (a)";
    assert_eq!(eval_ok_with_island(src), Value::Int(5));
}

#[test]
fn a_declaration_can_carry_a_generic_type_as_a_parameter() {
    // The *function* may not be generic; naming an instantiated generic type
    // in its signature is ordinary.
    let src = "
        (defsignature first-of (Vector<i32>) i32)
        (defun a () i32 (first-of (the Vector<i32> (Vector::new))))
        (defun first-of ((v Vector<i32>)) i32 (if (= (len v) 0) 0 (get v 0)))
        (a)";
    assert_eq!(eval_ok(src), Value::Int(0));
}

// ---- what a declaration costs --------------------------------------------

#[test]
fn an_undeclared_forward_call_is_an_error() {
    // The whole point of the change: nothing looks ahead any more.
    let msg = expect_err("(defun a () i32 (b)) (defun b () i32 7) (a)");
    assert!(msg.contains("b"), "error should name the missing function: {}", msg);
}

#[test]
fn undeclared_mutual_recursion_is_an_error() {
    let src = "
        (defun even2? ((n i32)) bool (if (= n 0) true  (odd2? (- n 1))))
        (defun odd2?  ((n i32)) bool (if (= n 0) false (even2? (- n 1))))
        (even2? 10)";
    let msg = expect_err(src);
    assert!(msg.contains("odd2?"), "error should name the missing function: {}", msg);
}

#[test]
fn a_declaration_with_no_definition_is_an_error() {
    let msg = expect_err("(defsignature b () i32) (defun a () i32 1) (a)");
    assert!(msg.contains("b") && msg.contains("no definition"), "unexpected error: {}", msg);
}

#[test]
fn a_definition_that_disagrees_on_a_parameter_type_is_an_error() {
    let msg = expect_err("(defsignature b (i32) i32) (defun b ((n string)) i32 1) (b 1)");
    assert!(msg.contains("defsignature") && msg.contains("parameter 1"), "unexpected error: {}", msg);
}

#[test]
fn a_definition_that_disagrees_on_the_return_type_is_an_error() {
    let msg = expect_err("(defsignature b () i32) (defun b () string \"x\") (b)");
    assert!(msg.contains("defsignature") && msg.contains("return type"), "unexpected error: {}", msg);
}

#[test]
fn a_definition_that_disagrees_on_arity_is_an_error() {
    let msg = expect_err("(defsignature b (i32) i32) (defun b () i32 1) (b)");
    assert!(msg.contains("defsignature") && msg.contains("parameter count"), "unexpected error: {}", msg);
}

#[test]
fn a_definition_that_disagrees_on_visibility_is_an_error() {
    let src = "
        (module m
          (defsignature b () i32)
          (pub defun b () i32 1))
        1";
    let msg = expect_err(src);
    assert!(msg.contains("visibility"), "unexpected error: {}", msg);
}

#[test]
fn a_generic_function_cannot_be_declared() {
    // Instantiating one needs its body, which a declaration does not have.
    let msg = expect_err("(defsignature idg<T> (T) T) (defun idg<T> ((x T)) T x) (idg 1)");
    assert!(msg.contains("generic"), "unexpected error: {}", msg);
}

#[test]
fn an_optional_or_key_parameter_cannot_be_declared() {
    let msg = expect_err("(defsignature f (i32 &optional i32) i32) (defun f ((a i32)) i32 a) (f 1)");
    assert!(msg.contains("&optional"), "unexpected error: {}", msg);
}

#[test]
fn declaring_the_same_name_twice_is_an_error() {
    let msg = expect_err("(defsignature b () i32) (defsignature b () i32) (defun b () i32 1) (b)");
    assert!(msg.contains("already declared"), "unexpected error: {}", msg);
}

#[test]
fn declaring_a_name_that_is_already_defined_is_an_error() {
    // Not a redefinition warning and not a dangling declaration: a
    // declaration placed after its definition can never do anything, and the
    // message says exactly that.
    let msg = expect_err("(defun b () i32 1) (defsignature b () i32) (b)");
    assert!(msg.contains("already defined"), "unexpected error: {}", msg);
}

// ---- what is unchanged ----------------------------------------------------

#[test]
fn a_genuine_redefinition_is_still_reported() {
    let w = warnings_of("(defun dup () i32 1) (defun dup () i32 2)");
    assert_eq!(w.len(), 1, "expected exactly one redefinition warning, got {:?}", w);
    assert!(w[0].contains("dup"), "warning should name the function: {}", w[0]);
}

#[test]
fn a_single_definition_produces_no_redefinition_warning() {
    assert!(warnings_of("(defun once () i32 1) (once)").is_empty());
}

#[test]
fn a_declaration_and_its_definition_produce_no_redefinition_warning() {
    assert!(warnings_of("(defsignature once () i32) (defun once () i32 1) (once)").is_empty());
}

#[test]
fn a_macro_must_still_be_defined_before_use() {
    // Macros cannot be declared: expanding one needs its body to have been
    // `exec`'d, which a signature registration cannot arrange.
    let msg = expect_err("(defun a () i32 (twice 5)) (defmacro twice (x) `(* 2 ,x)) (a)");
    assert!(msg.contains("twice"), "error should name the macro: {}", msg);
}

#[test]
fn a_type_still_has_to_be_defined_before_it_is_named() {
    // Types cannot be declared either — a type's registration *is* what the
    // code registering it needs. Pinned so that adding forward types later is
    // a deliberate, visible change.
    let src = "
        (defsignature make-it () pt)
        (defun make-it () pt (pt::new 1))
        (defstruct pt (x i32))
        (make-it)";
    let msg = expect_err(src);
    assert!(msg.contains("pt"), "unexpected error: {}", msg);
}

#[test]
fn an_undefined_function_is_still_an_error() {
    let msg = expect_err("(defun a () i32 (nope)) (a)");
    assert!(msg.contains("nope"), "error should name the missing function: {}", msg);
}
