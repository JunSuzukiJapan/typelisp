//! interp-closure removal Stage 1: LLVM handle types (`llvm-builder`,
//! `llvm-value`, native `Scope<V>`, ...) have a compiled representation —
//! an `i64` registry handle — and their builtin methods compile to the
//! generic `rt_llvm_call` dispatch shim. These tests compile small
//! functions whose parameters/returns/bodies use those types (exactly what
//! the self-hosted compiler island's own `defun`s do), call them with real
//! Rust-native LLVM values built by interpreted code, and check the results
//! round-trip — the boundary the island's own AOT compilation (later
//! stages) rests on.

use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
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
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// The `i64` constant an `llvm-value` handle holds, for asserting a
/// compiled `b::const-word` round-tripped through the handle registry.
fn const_int_of(v: &Value) -> i64 {
    match v {
        ref v if typelisp::llvm_value_of(v).is_some() => typelisp::llvm_value_of(v).unwrap()
            .into_int_value()
            .get_sign_extended_constant()
            .expect("expected a constant llvm-value"),
        other => panic!("expected an llvm-value handle, got {:?}", other),
    }
}

#[test]
fn a_compiled_function_with_llvm_typed_params_builds_a_constant() {
    // `mk-const` takes an `llvm-builder` (crossing in as a handle) and
    // returns an `llvm-value` (crossing back out) — compiled, its body is a
    // single `rt_llvm_call` into `llvm-builder::const-word`.
    let src = "(defun mk-const ((b llvm-builder)) llvm-value (const-word b 42)) \
               (compile mk-const) \
               (let ((b (llvm-builder::create))) (mk-const b))";
    assert_eq!(const_int_of(&eval_ok(src)), 42);
}

#[test]
fn compiled_and_interpreted_results_agree_for_an_llvm_method_body() {
    let defs = "(defun mk-const ((b llvm-builder)) llvm-value (const-word b 7))";
    let interp_src = format!("{} (let ((b (llvm-builder::create))) (mk-const b))", defs);
    let compiled_src = format!("{} (compile mk-const) (let ((b (llvm-builder::create))) (mk-const b))", defs);
    assert_eq!(const_int_of(&eval_ok(&interp_src)), const_int_of(&eval_ok(&compiled_src)));
}

#[test]
fn a_compiled_function_drives_a_native_scope_and_matches_its_option() {
    // The native `Scope<llvm-value>` surface the island leans on hardest:
    // `set` stores a handle, `get` returns `Option<llvm-value>` as a heap
    // enum box whose payload decodes through the integer field kind, and a
    // compiled `match` takes it apart.
    let src = "(defun keep ((s Scope<llvm-value>) (v llvm-value)) llvm-value \
                 (set s \"x\" v) \
                 (match (get s \"x\") \
                   ((Some w) w) \
                   (None (panic \"missing\")))) \
               (compile keep) \
               (defun new-scope () Scope<llvm-value> (Scope::new)) \
               (let ((b (llvm-builder::create))) \
                 (keep (new-scope) (const-word b 9)))";
    assert_eq!(const_int_of(&eval_ok(src)), 9);
}

#[test]
fn a_compiled_function_sees_none_for_a_missing_scope_name() {
    let src = "(defun probe ((s Scope<llvm-value>)) i64 \
                 (match (get s \"absent\") \
                   ((Some w) 1) \
                   (None 0))) \
               (compile probe) \
               (defun new-scope () Scope<llvm-value> (Scope::new)) \
               (probe (new-scope))";
    assert_eq!(eval_ok(src), Value::Int(0));
}
