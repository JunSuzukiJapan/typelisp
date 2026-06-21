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

/// Phase 2c: `f64` arithmetic, mixed in with `i64`-style scalar functions.
/// Exercises `+`/`*` (`build_float_add`/`build_float_mul`) and confirms the
/// JIT'd float result round-trips correctly through `TlValue`'s raw-bit-cast
/// boundary (`load-arg-f64`/`build-ret-f64`).
#[test]
fn compiles_an_f64_arithmetic_function() {
    let src = r#"
        (defun quad ((a f64) (b f64)) f64 (* (+ a b) 2.0))
        (compile "quad")
        (quad 3.0 4.0)
    "#;
    assert_eq!(run(src), RtValue::Float(14.0));
}

/// `f64` comparison (`fcmp`) still produces a plain `bool`/`i1` result, not a
/// `FloatValue` — mixing an `f64`-typed parameter with a `bool` return.
#[test]
fn compiles_an_f64_predicate_function() {
    let src = r#"
        (defun flt ((a f64) (b f64)) bool (< a b))
        (compile "flt")
        (flt 1.5 2.5)
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
    let src2 = r#"
        (defun flt ((a f64) (b f64)) bool (< a b))
        (compile "flt")
        (flt 2.5 1.5)
    "#;
    assert_eq!(run(src2), RtValue::Bool(false));
}

/// A bare `f64` literal as the function's entire body (return value),
/// exercising `AstExpr::AFloat`/`llvm-const-f64` directly.
#[test]
fn compiles_a_function_returning_an_f64_literal() {
    let src = r#"
        (defun pi-ish () f64 3.5)
        (compile "pi-ish")
        (pi-ish)
    "#;
    assert_eq!(run(src), RtValue::Float(3.5));
}

/// `i64`/`f64` mixed within one signature, with the `i64` parameter driving
/// an `i64`-only comparison (`typelisp` has no implicit numeric coercion, so
/// `i64` and `f64` values are never combined in one `Assoc` call — each
/// stays within its own type's binops) whose `bool` result picks between two
/// `f64` computations — the fully general case Phase 2's per-parameter
/// `param-is-bool`/`param-is-f64` dispatch (rather than one signature-wide
/// flag) is designed to support.
#[test]
fn compiles_a_function_mixing_i64_and_f64() {
    let src = r#"
        (defun pick ((a i64) (b f64)) f64 (if (> a 5) b (+ b 1.0)))
        (compile "pick")
        (pick 10 2.5)
    "#;
    assert_eq!(run(src), RtValue::Float(2.5));
    let src2 = r#"
        (defun pick ((a i64) (b f64)) f64 (if (> a 5) b (+ b 1.0)))
        (compile "pick")
        (pick 1 2.5)
    "#;
    assert_eq!(run(src2), RtValue::Float(3.5));
}

/// Phase 2e: a `let`-bound local as the function's entire body.
#[test]
fn compiles_a_function_with_a_single_let_binding() {
    let src = r#"
        (defun double ((x i64)) i64 (let ((y (* x 2))) y))
        (compile "double")
        (double 21)
    "#;
    assert_eq!(run(src), RtValue::Int(42));
}

/// Multiple bindings in one `let`, plus extra body forms before the last
/// (their values are computed — emitted as IR — but discarded; only the
/// last form's value is the `let`'s own value).
#[test]
fn compiles_a_function_with_multiple_let_bindings_and_extra_body_forms() {
    let src = r#"
        (defun combo ((a i64) (b i64)) i64
          (let ((x (* a 2)) (y (* b 3)))
            (+ x 0)
            (+ x y)))
        (compile "combo")
        (combo 5 4)
    "#;
    assert_eq!(run(src), RtValue::Int(22));
}

/// A `let` nested inside another `let`'s body.
#[test]
fn compiles_a_function_with_nested_let() {
    let src = r#"
        (defun nested ((a i64)) i64
          (let ((b (+ a 1)))
            (let ((c (+ b 1)))
              (+ a (+ b c)))))
        (compile "nested")
        (nested 10)
    "#;
    assert_eq!(run(src), RtValue::Int(33));
}

/// A `let` used in **value position** — nested inside an arithmetic
/// expression rather than as the function's whole body or a tail-`if` branch.
#[test]
fn compiles_a_function_using_let_in_value_position() {
    let src = r#"
        (defun let-in-value ((a i64)) i64 (+ (let ((x (* a 2))) x) 1))
        (compile "let-in-value")
        (let-in-value 5)
    "#;
    assert_eq!(run(src), RtValue::Int(11));
}

/// A `let` as one tail-`if` branch (`compile-tail`'s `ALet` arm, not
/// `compile-value`'s).
#[test]
fn compiles_a_function_using_let_in_an_if_branch() {
    let src = r#"
        (defun let-in-if ((a i64)) i64 (if (> a 0) (let ((x (* a 2))) x) 0))
        (compile "let-in-if")
        (let-in-if 5)
    "#;
    assert_eq!(run(src), RtValue::Int(10));
    let src2 = r#"
        (defun let-in-if ((a i64)) i64 (if (> a 0) (let ((x (* a 2))) x) 0))
        (compile "let-in-if")
        (let-in-if -5)
    "#;
    assert_eq!(run(src2), RtValue::Int(0));
}

/// A `let`-bound name shadowing the function's own parameter: looking it up
/// from the `let`'s body must find the *inner* binding, not the outer
/// parameter — exercises `param-index`'s backward (innermost-first) search,
/// since `extend-strs` appends the shadowing name *after* the parameter's
/// own entry.
#[test]
fn compiles_a_function_with_a_shadowing_let_binding() {
    let src = r#"
        (defun shadow ((a i64)) i64 (let ((a (+ a 1))) a))
        (compile "shadow")
        (shadow 5)
    "#;
    assert_eq!(run(src), RtValue::Int(6));
}

/// CL `let`'s parallel-binding semantics: every binding's value expression
/// sees only the *outer* scope, never a sibling binding — even one that
/// shadows the same name. `b`'s value here is the parameter `a` (5),
/// unaffected by the sibling binding that shadows `a` to `a + 1`; a
/// (incorrect) sequential/`let*`-like evaluation would instead see `b` as 6.
#[test]
fn compiles_a_function_whose_let_binding_value_sees_the_outer_scope() {
    let src = r#"
        (defun parallel ((a i64)) i64 (let ((a (+ a 1)) (b a)) b))
        (compile "parallel")
        (parallel 5)
    "#;
    assert_eq!(run(src), RtValue::Int(5));
}

/// Phase 2f: a compiled function directly calling another already-compiled
/// function — `square` must be compiled *before* `sum-of-squares` is, since
/// `ast_bridge::typed_to_ast` only bridges an `Expr::Call` whose callee is
/// already in `Interp.compiled`.
#[test]
fn compiles_a_function_calling_another_compiled_function() {
    let src = r#"
        (defun square ((x i64)) i64 (* x x))
        (compile "square")
        (defun sum-of-squares ((a i64) (b i64)) i64 (+ (square a) (square b)))
        (compile "sum-of-squares")
        (sum-of-squares 3 4)
    "#;
    assert_eq!(run(src), RtValue::Int(25));
}

/// The same callee invoked **twice** from one caller — exercises
/// `get-or-declare-function`'s reuse path (a second `add-function` call
/// with a colliding name would otherwise get silently renamed by LLVM,
/// leaving the second `call` site pointing at an undeclared/unmapped
/// function).
#[test]
fn compiles_a_function_calling_the_same_compiled_function_twice() {
    let src = r#"
        (defun half ((x f64)) f64 (/ x 2.0))
        (compile "half")
        (defun quarter ((x f64)) f64 (half (half x)))
        (compile "quarter")
        (quarter 8.0)
    "#;
    assert_eq!(run(src), RtValue::Float(2.0));
}

/// A `bool`-returning callee, called from an `if` condition in the caller —
/// exercises `build-call-bool`'s narrowing path.
#[test]
fn compiles_a_function_calling_a_bool_returning_compiled_function() {
    let src = r#"
        (defun is-positive ((x i64)) bool (> x 0))
        (compile "is-positive")
        (defun classify ((x i64)) i64 (if (is-positive x) 1 0))
        (compile "classify")
        (classify 5)
    "#;
    assert_eq!(run(src), RtValue::Int(1));
    let src2 = r#"
        (defun is-positive ((x i64)) bool (> x 0))
        (compile "is-positive")
        (defun classify ((x i64)) i64 (if (is-positive x) 1 0))
        (compile "classify")
        (classify -5)
    "#;
    assert_eq!(run(src2), RtValue::Int(0));
}

/// A call inside a `let` binding's value, and the `let`-bound result fed
/// into a second call — exercises `compile-value`'s `ACall`/`ALet`
/// interaction (both share the same `eval-args` `labels` helper).
#[test]
fn compiles_a_function_calling_a_compiled_function_inside_let() {
    let src = r#"
        (defun inc ((x i64)) i64 (+ x 1))
        (compile "inc")
        (defun twice-inc ((x i64)) i64 (let ((y (inc x))) (inc y)))
        (compile "twice-inc")
        (twice-inc 5)
    "#;
    assert_eq!(run(src), RtValue::Int(7));
}

/// Calling a function that hasn't been `compile`d yet is refused outright
/// (`ast-body` fails to bridge the call, so `compile` itself returns `Err`)
/// — the function is *not* JIT'd by silently falling back to the tree-walked
/// callee, which the unified `TlValue` ABI's compiled-call path has no
/// mechanism for at all.
#[test]
fn refuses_to_compile_a_function_calling_an_uncompiled_function() {
    let src = r#"
        (defun foo ((x i64)) i64 (+ x 1))
        (defun bar ((x i64)) i64 (foo x))
        (compile "bar")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 1, "expected Err, not Ok"),
        other => panic!("expected a Result value, got {:?}", other),
    }
}

/// Phase 2d: a bare `char` literal as the function's entire body, and as an
/// `if`'s tail condition (`AstExpr::AChar`/`llvm-const-char`).
#[test]
fn compiles_a_function_returning_a_char_literal() {
    let src = r#"
        (defun letter-a () char #\a)
        (compile "letter-a")
        (letter-a)
    "#;
    assert_eq!(run(src), RtValue::Char('a'));
}

/// A `char` parameter compared with `char::eq` (bridged onto the same `=`
/// `build-op` already implements for `i64`/`f64` — see `ast_bridge`).
#[test]
fn compiles_a_function_comparing_char_parameters_with_eq() {
    let src = r#"
        (defun same-letter ((a char) (b char)) bool (eq a b))
        (compile "same-letter")
        (same-letter #\x #\x)
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
    let src2 = r#"
        (defun same-letter ((a char) (b char)) bool (eq a b))
        (compile "same-letter")
        (same-letter #\x #\y)
    "#;
    assert_eq!(run(src2), RtValue::Bool(false));
}

/// `char::lt` bridged onto `<`, and a `char` parameter returned directly
/// from an `if`'s tail branches — exercises `load-arg-char`/`build-ret-char`'s
/// narrow/widen round-trip through the `TlValue` boundary.
#[test]
fn compiles_a_function_picking_the_smaller_of_two_chars() {
    let src = r#"
        (defun min-char ((a char) (b char)) char (if (lt a b) a b))
        (compile "min-char")
        (min-char #\z #\a)
    "#;
    assert_eq!(run(src), RtValue::Char('a'));
}

/// A compiled function calling another compiled function through a `char`
/// parameter/return — exercises `build-call-char`/`ret-is-char`.
#[test]
fn compiles_a_function_calling_a_char_returning_compiled_function() {
    let src = r#"
        (defun upcase-a ((c char)) char (upcase c))
        (compile "upcase-a")
    "#;
    // `upcase` isn't bridged (Phase 2d only covers `eq`/`lt`), so this must
    // be refused rather than miscompiled.
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 1, "expected Err, not Ok"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let src2 = r#"
        (defun first-char ((a char) (b char)) char a)
        (compile "first-char")
        (defun pick-first ((a char) (b char)) char (first-char a b))
        (compile "pick-first")
        (pick-first #\p #\q)
    "#;
    assert_eq!(run(src2), RtValue::Char('p'));
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
