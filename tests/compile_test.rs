//! Tests for `compile` (Phase 1 of [docs/TODO.md](../docs/TODO.md)「ステップ5」:
//! a function-level compiler **written in typelisp**, restricted to all-`i64`
//! scalar functions). The smoke test below just confirms inkwell/llvm-sys link
//! correctly; `compiles_and_calls_an_all_i64_function` is the real Phase 1
//! vertical slice — compile `max2` and confirm calling it through the
//! ordinary interpreter dispatch (`Expr::Call`) now runs the JIT'd native
//! code instead of tree-walking, transparently to the caller.

#![cfg(feature = "compile")]

use typelisp::{compiler_source, load_prelude, Checker, Heap, Interp, Reader, RtValue};

#[test]
fn llvm_toolchain_links_and_runs() {
    assert_eq!(typelisp::llvm_smoke_test(), "typelisp_smoke");
}

fn run(src: &str) -> RtValue {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    compiler_source::load(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    last
}

#[test]
fn compiles_and_calls_an_all_i64_function() {
    let src = r#"
        (defun max2 ((a i64) (b i64)) i64 (if (< a b) b a))
        (compile "max2")
        (max2 3 7)
    "#;
    assert_eq!(run(src), RtValue::Int(7));
}

#[test]
fn compiled_function_still_works_the_other_way_round() {
    let src = r#"
        (defun max2 ((a i64) (b i64)) i64 (if (< a b) b a))
        (compile "max2")
        (max2 9 2)
    "#;
    assert_eq!(run(src), RtValue::Int(9));
}

/// Phase 1's `CompiledFn` was a native `extern "C" fn(i64, ...) -> i64`
/// pointer with one variant per arity, hand-capped at 3 parameters. Phase 2's
/// unified `TlValue` ABI (`extern "C" fn(*const TlValue, u32, *mut TlValue)
/// -> i32`) passes arguments through an array instead of fixed positional
/// registers, so arity is no longer bounded by how many `CompiledFn` variants
/// exist — this exercises a 5-argument function to demonstrate that.
#[test]
fn compiles_a_function_with_more_than_three_arguments() {
    let src = r#"
        (defun sum5 ((a i64) (b i64) (c i64) (d i64) (e i64)) i64 (+ a (+ b (+ c (+ d e)))))
        (compile "sum5")
        (sum5 1 2 3 4 5)
    "#;
    assert_eq!(run(src), RtValue::Int(15));
}

/// Phase 2b: `i64`/`bool` can mix freely within one signature, the realistic
/// case Phase 1/2a's all-`i64` restriction ruled out entirely (a predicate
/// like this — `i64` params, `bool` return — is the most common shape that
/// restriction blocked).
#[test]
fn compiles_a_predicate_function_returning_bool() {
    let src = r#"
        (defun gt ((a i64) (b i64)) bool (> a b))
        (compile "gt")
        (gt 7 3)
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
}

#[test]
fn compiled_predicate_function_returns_false_too() {
    let src = r#"
        (defun gt ((a i64) (b i64)) bool (> a b))
        (compile "gt")
        (gt 2 9)
    "#;
    assert_eq!(run(src), RtValue::Bool(false));
}

/// A `bool` parameter used directly as an `if` condition — `load-arg-bool`
/// truncates the `i64`-wide `TlValue` payload down to `i1` so it can feed
/// `build-cond-br` without further conversion.
#[test]
fn compiles_a_function_taking_a_bool_parameter() {
    let src = r#"
        (defun choose ((c bool) (a i64) (b i64)) i64 (if c a b))
        (compile "choose")
        (choose true 11 22)
    "#;
    assert_eq!(run(src), RtValue::Int(11));
    let src2 = r#"
        (defun choose ((c bool) (a i64) (b i64)) i64 (if c a b))
        (compile "choose")
        (choose false 11 22)
    "#;
    assert_eq!(run(src2), RtValue::Int(22));
}

/// A bare `bool` literal as the `if` condition (`AstExpr::ABool` /
/// `llvm-const-bool`), in a 0-argument function.
#[test]
fn compiles_a_function_with_a_bool_literal_condition() {
    let src = r#"
        (defun always-one () i64 (if true 1 2))
        (compile "always-one")
        (always-one)
    "#;
    assert_eq!(run(src), RtValue::Int(1));
}

/// A bare `bool` literal as the function's entire body (return value).
#[test]
fn compiles_a_function_returning_a_bool_literal() {
    let src = r#"
        (defun yes () bool true)
        (compile "yes")
        (yes)
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
}

#[test]
fn refuses_to_compile_a_non_i64_function() {
    let src = r#"
        (defun add1 ((x i32)) i32 (+ x 1))
        (compile "add1")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 1, "expected Err, not Ok"),
        other => panic!("expected a Result value, got {:?}", other),
    }
}
