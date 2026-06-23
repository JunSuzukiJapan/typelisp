//! Tests for `compile` (Phase 1 of [docs/TODO.md](../docs/TODO.md)「ステップ5」:
//! a function-level compiler **written in typelisp**, restricted to all-`i64`
//! scalar functions). The smoke test below just confirms inkwell/llvm-sys link
//! correctly; `compiles_and_calls_an_all_i64_function` is the real Phase 1
//! vertical slice — compile `max2` and confirm calling it through the
//! ordinary interpreter dispatch (`Expr::Call`) now runs the JIT'd native
//! code instead of tree-walking, transparently to the caller.

#![cfg(feature = "compile")]

use typelisp::{compiler_source, load_prelude, Checker, Heap, Interp, Reader, RtValue, Value};

#[test]
fn llvm_toolchain_links_and_runs() {
    assert_eq!(typelisp::llvm_smoke_test(), "typelisp_smoke");
}

fn run(src: &str) -> RtValue {
    run_with_heap(Heap::with_capacity(1 << 16), src)
}

/// Like [`run`], but with a caller-supplied heap — Phase 3's rooting tests
/// use a tiny cons arena so `Heap::cons` is forced to run a real GC
/// mid-compiled-function-call, exercising `compile`'s
/// `push-root`/`pop-root` discipline rather than just its happy path (where
/// the arena never fills up at all).
fn run_with_heap(mut h: Heap, src: &str) -> RtValue {
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

/// Phase 3: `car`/`cdr` never allocate, so this exercises the simplest
/// `Sexpr` round-trip (`load-arg-sexpr`/`build-cdr`/`build-car`/
/// `build-ret-sexpr`) with no rooting concerns at all — every element of
/// `s` is `Nil`/`Cons`, the only `Value` shapes Phase 3 bridges.
#[test]
fn compiles_a_function_using_car_and_cdr() {
    let src = r#"
        (defun second ((s Sexpr)) Sexpr (car (cdr s)))
        (compile "second")
        (null (second (cons () (cons () ()))))
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
}

/// `cons` (Phase 3) actually allocates — `build-cons` must root its operands
/// around the underlying `Heap::cons` call so a GC mid-call can't reclaim
/// either one.
#[test]
fn compiles_a_function_that_builds_a_cons_cell() {
    let src = r#"
        (defun pair ((a Sexpr) (b Sexpr)) Sexpr (cons a b))
        (compile "pair")
        (consp (pair () ()))
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
}

/// `null` (Phase 3) bridges directly onto the `Nil`-pointer-check primitive
/// (`build-nullp`) even though it's an ordinary `match`-based typelisp
/// `defun` in `prelude.rs`, not a Rust builtin.
#[test]
fn compiles_a_function_checking_for_nil() {
    let src = r#"
        (defun is-empty ((s Sexpr)) bool (null s))
        (compile "is-empty")
        (is-empty ())
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
    let src2 = r#"
        (defun is-empty ((s Sexpr)) bool (null s))
        (compile "is-empty")
        (is-empty (cons (Int 1) (Int 2)))
    "#;
    assert_eq!(run(src2), RtValue::Bool(false));
}

/// A bare `Nil` literal (`()`) as the function's entire body —
/// `AstExpr::ANil`/`llvm-const-nil`.
#[test]
fn compiles_a_function_returning_nil() {
    let src = r#"
        (defun make-nil () Sexpr ())
        (compile "make-nil")
        (null (make-nil))
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
}

/// The rooting discipline's actual reason to exist: two nested `let`s each
/// `cons`-ing their own pair, then `cons`-ing those two pairs together. If
/// `x` (the first `let`'s result) weren't rooted for the duration of the
/// *second* `let`'s `cons` call, a GC triggered by that second call could
/// reclaim it before the final `(cons x y)` ever runs.
#[test]
fn compiles_a_function_with_nested_lets_each_consing() {
    let src = r#"
        (defun build ((a Sexpr) (b Sexpr) (c Sexpr) (d Sexpr)) Sexpr
          (let ((x (cons a b)))
            (let ((y (cons c d)))
              (cons x y))))
        (compile "build")
        (let* ((r (build () () () ())))
          (consp r))
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
}

/// The same nested-`let`-and-`cons` shape as
/// `compiles_a_function_with_nested_lets_each_consing`, but run against a
/// cons arena so small that `Heap::cons` is forced to actually run a
/// collection partway through the compiled function's execution — the only
/// way to exercise the rooting discipline's reason to exist rather than
/// just its happy path. Calls the compiled function repeatedly (each call
/// allocates 3 fresh cells) so the arena fills up and a real GC fires
/// mid-call; if `push-root`/`pop-root` were missing or miscounted, the
/// `x`/`y` intermediates would be vulnerable to reclamation and this would
/// either panic (`HeapExhausted`/`NotACons`) or abort the process.
#[test]
fn compiled_cons_chain_survives_a_gc_mid_call() {
    let src = r#"
        (defun build ((a Sexpr) (b Sexpr) (c Sexpr) (d Sexpr)) Sexpr
          (let ((x (cons a b)))
            (let ((y (cons c d)))
              (cons x y))))
        (compile "build")
        (let ((result (Nil)))
          (dotimes (i 500) (setf result (build () () () ())))
          (consp result))
    "#;
    // `dotimes` runs in the tree-walking interpreter as a plain Rust `loop`
    // (not recursion — see `Interp`'s `While`/`Loop` evaluation), calling
    // the compiled `build` 500 times — far more cons cells than this small
    // arena can hold without collecting, several times over (most of the
    // arena's cells go to loading `prelude.rs`/`compiler_source.rs`
    // themselves; the loop's 1500 cells then force several real
    // collections).
    assert_eq!(run_with_heap(Heap::with_capacity(8192), src), RtValue::Bool(true));
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

/// Phase 2g (self-recursion): a self-recursive call now bridges into an
/// `ACall` whose callee is the very function `compile` is building —
/// `add-function` already added it to `module` *before* the body is walked,
/// so the call resolves to an ordinary intra-module `call`, ahead of `name`
/// ever being registered in `Interp.compiled` (`compile`'s singleton
/// `group`, see `ast_bridge::typed_to_ast`'s doc comment). Asserts `compile`
/// itself returns `Ok` (not just that the *answer* is right) — a function
/// `compile` refuses still runs correctly via the tree-walking fallback, so
/// only the answer would *not* catch a regression back to "self-recursion
/// is refused".
#[test]
fn compiles_a_self_recursive_function() {
    let src = r#"
        (defun fact ((n i64)) i64 (if (<= n 1) 1 (* n (fact (- n 1)))))
        (compile "fact")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let src2 = r#"
        (defun fact ((n i64)) i64 (if (<= n 1) 1 (* n (fact (- n 1)))))
        (compile "fact")
        (fact 6)
    "#;
    assert_eq!(run(src2), RtValue::Int(720));
}

/// The same self-recursion bridging, but with the recursive call in **tail**
/// position — exercised separately since `compile-tail`'s catch-all `_` arm
/// (which calls `compile-value` then `build-ret`) is a different code path
/// from `compile-value`'s own dedicated `ACall` arm
/// (`compiles_a_self_recursive_function` above only exercises the latter,
/// since `(fact (- n 1))` sits inside `(* n ...)`, a value position). 1000
/// frames of genuine native recursion (no tail-call optimization at
/// `OptimizationLevel::None`) — far more than would survive through the
/// tree-walking interpreter's own call stack for a recursive *caller*, so a
/// silent fallback to tree-walking would be expected to behave differently,
/// not just compute a wrong answer.
#[test]
fn compiles_a_tail_self_recursive_function() {
    let src = r#"
        (defun count-down ((n i64)) i64 (if (<= n 0) n (count-down (- n 1))))
        (compile "count-down")
        (count-down 1000)
    "#;
    assert_eq!(run(src), RtValue::Int(0));
}

/// Phase 2g (mutual-recursion infrastructure): `compile-group` batches
/// several `defun`s into *one* shared `LlvmModule`/`ExecutionEngine` — every
/// member's bare declaration is added to it before any body is built, so a
/// call to a sibling resolves to an ordinary intra-module `call` even though
/// *neither* function is registered in `Interp.compiled` until the whole
/// group finishes (unlike two separate `(compile ...)` calls, where the
/// callee must finish compiling *first*, Phase 2f). `helper` is
/// self-recursive (the same `group`-membership trick `compile`'s own
/// singleton group uses) and `caller` calls it.
///
/// Note genuine A-calls-B-calls-A recursion between two distinct *top-level*
/// `defun`s isn't expressible in this language at all: a `defun` may only
/// call itself or an already-*defined* `defun` (`crate::compile::ast_bridge`
/// /`Checker::check_defun`), and top-level forms are read/checked/executed
/// one at a time with no forward declarations, so `helper` could never
/// reference a `caller` defined after it. The shape this test exercises — a
/// forward-referencing caller plus a self-recursive callee, compiled
/// *together* in one pass instead of two separate `compile` calls — is the
/// realistic scope `compile-group` actually covers today.
#[test]
fn compile_group_compiles_several_functions_into_one_module() {
    let src = r#"
        (defun helper ((n i64)) i64 (if (<= n 0) 0 (+ n (helper (- n 1)))))
        (defun caller ((n i64)) i64 (helper n))
        (defun two-names ((a string) (b string) (out Vector<string>)) Vector<string>
          (progn (push out a) (push out b) out))
        (compile-group (two-names "helper" "caller" (Vector::new 0 "")))
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let src2 = r#"
        (defun helper ((n i64)) i64 (if (<= n 0) 0 (+ n (helper (- n 1)))))
        (defun caller ((n i64)) i64 (helper n))
        (defun two-names ((a string) (b string) (out Vector<string>)) Vector<string>
          (progn (push out a) (push out b) out))
        (compile-group (two-names "helper" "caller" (Vector::new 0 "")))
        (caller 5)
    "#;
    assert_eq!(run(src2), RtValue::Int(15));
}

/// A group member calling a function neither in the group nor already
/// compiled is refused outright, the same way Phase 2f refuses a plain
/// `compile` of such a function — `compile-group`'s `group` widens what
/// `ast_bridge::typed_to_ast` accepts, it doesn't remove the check entirely.
#[test]
fn compile_group_refuses_a_call_outside_the_group() {
    let src = r#"
        (defun foo ((x i64)) i64 (+ x 1))
        (defun bar ((x i64)) i64 (foo x))
        (defun one-name ((a string) (out Vector<string>)) Vector<string>
          (progn (push out a) out))
        (compile-group (one-name "bar" (Vector::new 0 "")))
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 1, "expected Err, not Ok"),
        other => panic!("expected a Result value, got {:?}", other),
    }
}

/// ループ構文: the canonical `while`+`setf` accumulator loop — two `let`-
/// bound loop-carried variables (`acc`/`i`), mutated via `setf` each
/// iteration, with the final answer (`acc`) read *after* the loop exits.
/// Reading it correctly depends on `AWhile`'s header phi nodes (not the
/// loop body's raw, dominance-unsafe SSA values) being what post-loop code
/// actually sees — see `compiler_source`'s doc comment.
#[test]
fn compiles_a_function_with_a_while_loop_summing_to_n() {
    let src = r#"
        (defun sum-to ((n i64)) i64
          (let ((acc (- n n)) (i (- n n)))
            (while (< i n)
              (setf acc (+ acc i))
              (setf i (+ i 1)))
            acc))
        (compile "sum-to")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let src2 = r#"
        (defun sum-to ((n i64)) i64
          (let ((acc (- n n)) (i (- n n)))
            (while (< i n)
              (setf acc (+ acc i))
              (setf i (+ i 1)))
            acc))
        (compile "sum-to")
        (sum-to 5)
    "#;
    assert_eq!(run(src2), RtValue::Int(10));
}

/// The loop body never runs at all (`n <= 0`) — the zero-iteration path
/// through the header straight to `exit`, never visiting `body-block`. This
/// is exactly the path that would be broken by a naive "just mutate `vals`
/// in place, no phi" implementation: `exit`'s predecessor is `header`, not
/// `body-block`, so any post-loop read must come from the header's phi
/// (valid on *every* path into `exit`), not a value the loop body computed
/// (which, on this path, never even ran).
#[test]
fn compiles_a_while_loop_that_never_runs_its_body() {
    let src = r#"
        (defun sum-to ((n i64)) i64
          (let ((acc (- n n)) (i (- n n)))
            (while (< i n)
              (setf acc (+ acc i))
              (setf i (+ i 1)))
            acc))
        (compile "sum-to")
        (sum-to 0)
    "#;
    assert_eq!(run(src), RtValue::Int(0));
}

/// Two `while` loops, one nested inside the other's body, sharing every
/// loop-carried variable from one `let` (no intervening `let` between
/// either loop and the other's `vals` — see `compile-value`'s `ALet` arm's
/// doc comment on why a `setf` from inside a *nested let* needs its own
/// `copy-into` to be visible further out; this test deliberately avoids
/// that by declaring `j` up front instead). Exercises `current-block`
/// (rather than an assumed `body-block`) for the *outer* loop's own
/// preheader/latch: by the time the outer loop's body finishes (having run
/// the whole inner loop), `b` is positioned at the inner loop's `exit`
/// block, not the outer loop's own `body-block`.
#[test]
fn compiles_a_function_with_a_nested_while_loop() {
    let src = r#"
        (defun nested-sum ((n i64)) i64
          (let ((total (- n n)) (i (- n n)) (j (- n n)))
            (while (< i n)
              (setf j 0)
              (while (< j n)
                (setf total (+ total 1))
                (setf j (+ j 1)))
              (setf i (+ i 1)))
            total))
        (compile "nested-sum")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let src2 = r#"
        (defun nested-sum ((n i64)) i64
          (let ((total (- n n)) (i (- n n)) (j (- n n)))
            (while (< i n)
              (setf j 0)
              (while (< j n)
                (setf total (+ total 1))
                (setf j (+ j 1)))
              (setf i (+ i 1)))
            total))
        (compile "nested-sum")
        (nested-sum 3)
    "#;
    assert_eq!(run(src2), RtValue::Int(9));
}

/// `f64`/`bool` loop-carried variables go through the very same phi
/// mechanism as `i64` (`build-phi` is seeded from whatever LLVM kind the
/// variable's current value happens to be) — this exercises an `f64`
/// accumulator and a `bool` flag both threaded through one loop's header.
#[test]
fn compiles_a_while_loop_with_an_f64_accumulator_and_a_bool_flag() {
    let src = r#"
        (defun sum-to-f64 ((n i64)) f64
          (let ((acc 0.0) (i (- n n)) (seen-any false))
            (while (< i n)
              (setf acc (+ acc 1.0))
              (setf seen-any true)
              (setf i (+ i 1)))
            (if seen-any acc 0.0)))
        (compile "sum-to-f64")
        (sum-to-f64 4)
    "#;
    assert_eq!(run(src), RtValue::Float(4.0));
}

/// A `Sexpr`-typed *parameter* may flow unmodified through a `while` loop's
/// phi nodes (read-only — `s` itself is never `setf`'d, only an unrelated
/// `i64` counter is) without compromising GC safety: the parameter's
/// initial pointer is already rooted for the function's whole lifetime
/// (Phase 3's `count-and-push-sexpr` at `entry`), and `s`'s phi (built
/// unconditionally for *every* in-scope variable — see `make-phis`'s doc
/// comment) merely creates an additional register that aliases that same
/// already-rooted pointer on every iteration. See
/// `ast_bridge::typed_to_ast`'s `Expr::Set` arm doc comment on why actually
/// *reassigning* a `Sexpr`-typed variable (as opposed to just reading one
/// unchanged through the loop, like this test does) is the part that isn't
/// safe yet, and so isn't bridged.
#[test]
fn compiles_a_while_loop_reading_a_sexpr_parameter_without_mutating_it() {
    let src = r#"
        (defun spin-then-return ((s Sexpr) (n i64)) Sexpr
          (let ((i (- n n)))
            (while (< i n)
              (setf i (+ i 1)))
            s))
        (compile "spin-then-return")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let src2 = r#"
        (defun spin-then-return ((s Sexpr) (n i64)) Sexpr
          (let ((i (- n n)))
            (while (< i n)
              (setf i (+ i 1)))
            s))
        (compile "spin-then-return")
        (car (spin-then-return (cons (Int 7) ()) 3))
    "#;
    assert_eq!(run(src2), RtValue::Sexpr(Value::Int(7)));
}

/// `break`/`return`/`loop` aren't bridged yet (no `ABreak`/`AReturn`/`ALoop`
/// `AstExpr` variant) — a `while` body using `break` fails the whole AST
/// bridge, refusing `compile` outright rather than miscompiling it, the
/// same "refuse, don't guess" discipline every other unsupported construct
/// gets.
#[test]
fn compiles_a_while_loop_using_break() {
    // `break` is bridged now (ループ・分岐構文の整理: `AstExpr::abreak`,
    // `compile-value`'s `ABreak` arm) — `while`'s own macro expansion over
    // `loop` (`src/prelude.rs`) means this `break` is really inside an
    // `Expr::Loop`, and the `if` guarding it is a genuine statement-position
    // `if` (one branch diverges via `break`, the other falls through via
    // `setf`) — exactly the case the value-position `if` redesign exists
    // for. `i` is read *after* the loop, via the loop's per-variable exit
    // phi (`copy-into`d back into the outer `vals`).
    let src = r#"
        (defun first-ge ((n i64)) i64
          (let ((i (- n n)))
            (while (< i n)
              (if (= i 3) (break) (setf i (+ i 1))))
            i))
        (compile "first-ge")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let src2 = r#"
        (defun first-ge ((n i64)) i64
          (let ((i (- n n)))
            (while (< i n)
              (if (= i 3) (break) (setf i (+ i 1))))
            i))
        (compile "first-ge")
        (first-ge 10)
    "#;
    assert_eq!(run(src2), RtValue::Int(3));
    let src3 = r#"
        (defun first-ge ((n i64)) i64
          (let ((i (- n n)))
            (while (< i n)
              (if (= i 3) (break) (setf i (+ i 1))))
            i))
        (compile "first-ge")
        (first-ge 2)
    "#;
    assert_eq!(run(src3), RtValue::Int(2));
}

/// `setf` of a `Sexpr`-typed variable isn't bridged (see
/// `ast_bridge::typed_to_ast`'s `Expr::Set` arm doc comment) — `compile`
/// refuses outright rather than risk an unrooted pointer.
#[test]
fn refuses_to_compile_a_setf_of_a_sexpr_variable() {
    let src = r#"
        (defun loses-the-front ((s Sexpr) (n i64)) Sexpr
          (let ((i (- n n)) (cur s))
            (while (< i n)
              (setf cur (cdr cur))
              (setf i (+ i 1)))
            cur))
        (compile "loses-the-front")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 1, "expected Err, not Ok"),
        other => panic!("expected a Result value, got {:?}", other),
    }
}

// ---- ループ・分岐構文の整理: value-position `if` ------------------------------

/// The simplest value-position `if`: its result feeds a binary operator,
/// not a `build-ret*` — exercises `compile-value`'s new `AIf` arm
/// (continuation block + phi) rather than `compile-tail`'s old
/// tail-only one.
#[test]
fn compiles_a_function_using_if_in_value_position() {
    let src = r#"
        (defun f ((a i64) (b i64)) i64 (+ (if (< a b) a b) 1))
        (compile "f")
        (f 3 7)
    "#;
    assert_eq!(run(src), RtValue::Int(4));
}

/// An `if` used as a `let` binding's initializer — exercises the merge
/// phi's result flowing through `eval-args`/`ALet`'s `bound-vals`.
#[test]
fn compiles_a_function_using_if_as_a_let_binding_value() {
    let src = r#"
        (defun f ((a i64) (b i64)) i64 (let ((m (if (< a b) a b))) (* m 10)))
        (compile "f")
        (f 3 7)
    "#;
    assert_eq!(run(src), RtValue::Int(30));
}

/// An `if` nested inside another `if`'s branch, in value position —
/// exercises `current-block` for the outer `if`'s `then-end`/`els-end`
/// (the inner `if` leaves `b` positioned at *its own* continuation block,
/// not the block the outer `if` itself `append-block`-ed).
#[test]
fn compiles_a_function_with_nested_if_in_value_position() {
    // Returns one of `a`/`b`/`c` (always `i64`, sidestepping bare integer
    // literals defaulting to `i32` deep inside a binop's first-argument
    // position — see the other tests in this file for that convention).
    let src = r#"
        (defun classify ((a i64) (b i64) (c i64)) i64
          (+ (if (< a b) (if (< b c) a b) (if (< a c) a c)) 100))
        (compile "classify")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let cases = [((1, 2, 3), 101), ((1, 2, 0), 102), ((5, 1, 9), 105), ((5, 1, 0), 100)];
    for ((a, b, c), expected) in cases {
        let src2 = format!(
            r#"
            (defun classify ((a i64) (b i64) (c i64)) i64
              (+ (if (< a b) (if (< b c) a b) (if (< a c) a c)) 100))
            (compile "classify")
            (classify {a} {b} {c})
        "#
        );
        assert_eq!(run(&src2), RtValue::Int(expected), "a={a} b={b} c={c}");
    }
}

/// A regression test for the exact bug found while designing this: each
/// `if` branch must compile against its *own copy* of `vals`, not the
/// shared one directly — otherwise `then`'s `setf` would leak into `els`'s
/// *compiled code* even though only one of them ever actually runs.
/// `acc` is read identically in both branches, so a leak would make this
/// return the wrong (always-mutated) value regardless of which branch the
/// condition actually selects.
#[test]
fn compiles_a_function_where_setf_in_one_if_branch_does_not_leak_into_the_other() {
    let src = r#"
        (defun f ((flag bool) (start i64)) i64
          (let ((acc start))
            (if flag (setf acc (+ acc 100)) acc)
            acc))
        (compile "f")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    let src_true = r#"
        (defun f ((flag bool) (start i64)) i64
          (let ((acc start))
            (if flag (setf acc (+ acc 100)) acc)
            acc))
        (compile "f")
        (f true 5)
    "#;
    assert_eq!(run(src_true), RtValue::Int(105));
    let src_false = r#"
        (defun f ((flag bool) (start i64)) i64
          (let ((acc start))
            (if flag (setf acc (+ acc 100)) acc)
            acc))
        (compile "f")
        (f false 5)
    "#;
    assert_eq!(run(src_false), RtValue::Int(5));
}

/// A value-position `if` with one branch diverging via `panic`
/// (`then_diverges`/`els_diverges`, computed from the branch's own checked
/// type being `Never`) — the merge at `cont-block` must skip the
/// diverging branch entirely rather than trying to treat its absent value
/// as a real one. Only the non-panicking path is actually exercised at
/// runtime (calling this with a negative `a` would abort the process).
#[test]
fn compiles_a_function_with_a_divergent_if_branch() {
    let src = r#"
        (defun f ((a i64)) i64 (+ (if (< a 0) (panic "negative") a) 1))
        (compile "f")
        (f 9)
    "#;
    assert_eq!(run(src), RtValue::Int(10));
}

/// `cond`/`when`/`unless`/`and`/`or` are `defmacro`s over `if` now (see
/// `src/prelude.rs`'s "loop/branch primitive reduction" comment) — by the
/// time `ast_bridge::typed_to_ast` ever sees these functions' bodies,
/// they've already been expanded into plain `if`/`Unit` forms, so this is
/// really exercising the same `AIf`/`AUnit` machinery, just confirming the
/// macro layer composes with it correctly end to end.
#[test]
fn compiles_a_function_using_cond_when_unless_and_and_or() {
    let src = r#"
        (defun classify ((n i64)) i64
          (cond ((< n 0) (- (- n n) 1)) ((= n 0) (- n n)) (else (+ (- n n) 1))))
        (defun side-effect ((flag bool) (start i64)) i64
          (let ((acc start))
            (when flag (setf acc (+ acc 1)))
            (unless flag (setf acc (+ acc 100)))
            acc))
        (defun combine ((a bool) (b bool)) bool (and a (or b false)))
        (compile "classify")
        (compile "side-effect")
        (compile "combine")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    assert_eq!(run(&format!("{src}\n(classify -5)")), RtValue::Int(-1));
    assert_eq!(run(&format!("{src}\n(classify 0)")), RtValue::Int(0));
    assert_eq!(run(&format!("{src}\n(classify 5)")), RtValue::Int(1));
    assert_eq!(run(&format!("{src}\n(side-effect true 0)")), RtValue::Int(1));
    assert_eq!(run(&format!("{src}\n(side-effect false 0)")), RtValue::Int(100));
    assert_eq!(run(&format!("{src}\n(combine true true)")), RtValue::Bool(true));
    assert_eq!(run(&format!("{src}\n(combine true false)")), RtValue::Bool(false));
    assert_eq!(run(&format!("{src}\n(combine false true)")), RtValue::Bool(false));
}

// ---- ループ・分岐構文の整理: `loop`/`break`/`return` ---------------------------

/// The simplest `loop`+`return`: a single exit site carrying a value —
/// exercises `ALoop`'s eagerly-built `exit-result-phi` with exactly one
/// incoming edge.
#[test]
fn compiles_a_function_with_a_loop_returning_a_value() {
    let src = r#"
        (defun f ((n i64)) i64
          (let ((i (- n n)))
            (loop
              (if (>= i n) (return i) (setf i (+ i 1))))))
        (compile "f")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    assert_eq!(run(&format!("{src}\n(f 7)")), RtValue::Int(7));
    assert_eq!(run(&format!("{src}\n(f 0)")), RtValue::Int(0));
}

/// A `loop` with *two* distinct `return` sites, each contributing its own
/// incoming edge to the same `exit-result-phi` — the part `while`'s
/// single-predecessor `exit` never needed to handle.
#[test]
fn compiles_a_function_with_multiple_return_sites() {
    let src = r#"
        (defun classify ((n i64)) i64
          (let ((i (- n n)))
            (loop
              (if (= i n) (return (- (- n n) 1)) ())
              (if (= i 3) (return 99) ())
              (setf i (+ i 1)))))
        (compile "classify")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    // Hits the `i == 3` return site first.
    assert_eq!(run(&format!("{src}\n(classify 10)")), RtValue::Int(99));
    // Never reaches `i == 3`; falls out via the `i == n` return site.
    assert_eq!(run(&format!("{src}\n(classify 2)")), RtValue::Int(-1));
}

/// `break` with no value, alongside `setf`-mutated loop-carried state read
/// after the loop exits — `exit-vals-phis` must reflect the value as of
/// the `break` site, not the loop header's stale pre-iteration one.
#[test]
fn compiles_a_function_with_loop_and_break_accumulating_state() {
    let src = r#"
        (defun f ((n i64)) i64
          (let ((i (- n n)) (acc (- n n)))
            (loop
              (if (>= i n) (break) ())
              (setf acc (+ acc i))
              (setf i (+ i 1)))
            acc))
        (compile "f")
        (f 5)
    "#;
    assert_eq!(run(src), RtValue::Int(10));
}

/// `loop` nested inside another `loop`'s body, each with its own `break` —
/// `ABreak`/`AReturn` must target the *nearest* enclosing loop's exit (the
/// inner one), never the outer one, and `current-block` must correctly
/// identify the inner loop's `exit` block as the outer loop's actual
/// latch predecessor (the same nested-control-flow case `AWhile` handled,
/// now for two genuine `loop`s instead of one `while`).
#[test]
fn compiles_a_function_with_nested_loops_and_breaks() {
    let src = r#"
        (defun f ((n i64)) i64
          (let ((total (- n n)) (i (- n n)))
            (loop
              (if (>= i n) (break) ())
              (let ((j (- n n)))
                (loop
                  (if (>= j n) (break) ())
                  (setf total (+ total 1))
                  (setf j (+ j 1))))
              (setf i (+ i 1)))
            total))
        (compile "f")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    assert_eq!(run(&format!("{src}\n(f 3)")), RtValue::Int(9));
    assert_eq!(run(&format!("{src}\n(f 0)")), RtValue::Int(0));
}

/// `return` from inside a `loop` nested inside a `while` (itself now a
/// `defmacro` over `loop`) — the inner `return` must target the inner
/// `loop`'s exit, not bubble out through the outer `while`'s own `loop`.
/// Each outer iteration re-binds a fresh `j` and runs the inner `loop` to
/// completion (`j` counting 0..3, `return`ing once it hits 3), accumulating
/// into `total` — `f(n)` is `3 * n`, simple enough to verify by hand while
/// still genuinely exercising both the inner loop's own back edge *and*
/// the outer `while`'s, with one nested inside the other's body.
#[test]
fn compiles_a_function_with_return_inside_a_loop_nested_in_a_while() {
    let src = r#"
        (defun f ((n i64)) i64
          (let ((k (- n n)) (total (- n n)))
            (while (< k n)
              (let ((j (- n n)))
                (setf total
                      (+ total
                         (loop
                           (if (>= j 3) (return j) ())
                           (setf j (+ j 1))))))
              (setf k (+ k 1)))
            total))
        (compile "f")
    "#;
    match run(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 0, "expected Ok, not Err"),
        other => panic!("expected a Result value, got {:?}", other),
    }
    assert_eq!(run(&format!("{src}\n(f 2)")), RtValue::Int(6));
    assert_eq!(run(&format!("{src}\n(f 0)")), RtValue::Int(0));
}

// ---- Phase 5: full `Sexpr` variant coverage ------------------------------
//
// Phase 3 only ever bridged `Sexpr`'s `Nil`/`Cons` values (`value_to_ptr`
// aborted on anything else reaching JIT'd code). Phase 5 widened every
// in-flight `Sexpr` value from a bare pointer to a `{tag, payload}` struct
// (the same shape as the ABI's own `TlValue`) so `Int`/`Float`/`Char`/`Bool`/
// `Symbol`/`Str` (every variant reachable from ordinary typelisp source —
// `Path` has no `Sexpr` constructor at all, see `Checker::value_to_quoted`,
// so it stays untested here even though `value_to_tag_payload` handles it)
// can flow through `cons`/`car`/`cdr`/parameters/returns/calls too.

/// An `Int` extracted from a cons chain built *outside* the compiled
/// function and read back via `car`/`cdr` — the most basic non-`Nil`/`Cons`
/// payload round trip.
#[test]
fn compiles_a_function_extracting_a_non_nil_int_via_car_and_cdr() {
    let src = r#"
        (defun second ((s Sexpr)) Sexpr (car (cdr s)))
        (compile "second")
        (second (cons (Int 1) (cons (Int 42) ())))
    "#;
    assert_eq!(run(src), RtValue::Sexpr(Value::Int(42)));
}

/// `Float`/`Char`/`Bool` `Sexpr` parameters pass straight through an identity
/// function (no `cons`/`car` involved at all) — exercises `load-arg-sexpr`/
/// `build-ret-sexpr` directly carrying a non-`Cons` tag across the ABI
/// boundary.
#[test]
fn compiles_a_function_passing_through_a_float_sexpr_value() {
    let src = r#"
        (defun identity-sexpr ((s Sexpr)) Sexpr s)
        (compile "identity-sexpr")
        (identity-sexpr (Float 2.5))
    "#;
    assert_eq!(run(src), RtValue::Sexpr(Value::Float(2.5)));
}

#[test]
fn compiles_a_function_passing_through_a_char_sexpr_value() {
    let src = r#"
        (defun identity-sexpr ((s Sexpr)) Sexpr s)
        (compile "identity-sexpr")
        (identity-sexpr (Char #\z))
    "#;
    assert_eq!(run(src), RtValue::Sexpr(Value::Char('z')));
}

#[test]
fn compiles_a_function_passing_through_a_bool_sexpr_value() {
    let src = r#"
        (defun identity-sexpr ((s Sexpr)) Sexpr s)
        (compile "identity-sexpr")
        (identity-sexpr (Bool true))
    "#;
    assert_eq!(run(src), RtValue::Sexpr(Value::Bool(true)));
}

/// `Symbol`/`Str` are interned (their `Sexpr` payload is an id, not the
/// content itself), so identity is checked structurally (`match` + `eq`)
/// rather than by comparing the returned `RtValue` directly against a
/// hardcoded id.
#[test]
fn compiles_a_function_passing_through_a_symbol_sexpr_value() {
    let src = r#"
        (defun identity-sexpr ((s Sexpr)) Sexpr s)
        (compile "identity-sexpr")
        (match (identity-sexpr (Sym "greeting")) ((Sym name) (eq name "greeting")) (_ false))
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
}

/// `Str` is the one `Sexpr` scalar the GC actually collects (see
/// `Heap::gc`'s string sweep) — run under a tiny arena with a forced GC mid
/// loop to confirm a `Str`-tagged `Sexpr` value survives passing through
/// compiled code repeatedly without `compile`'s rooting discipline needing
/// any change for it (the same `push-root`/`pop-root` shim now just
/// reconstructs a `Value::Str` instead of refusing to).
#[test]
fn compiles_a_function_passing_through_a_string_sexpr_value_under_gc_pressure() {
    let src = r#"
        (defun identity-sexpr ((s Sexpr)) Sexpr s)
        (compile "identity-sexpr")
        (let ((result (Nil)))
          (dotimes (i 500) (setf result (identity-sexpr (Str "hello"))))
          (match result ((Str s) (eq s "hello")) (_ false)))
    "#;
    assert_eq!(run_with_heap(Heap::with_capacity(8192), src), RtValue::Bool(true));
}

/// `consp`/`atom` (Phase 5) — `prelude.rs` `defun`s built on `match`, bridged
/// directly onto a tag-equality check the same way `null` already is.
#[test]
fn compiles_a_function_using_consp_and_atom() {
    let src = r#"
        (defun classify ((s Sexpr)) bool (consp s))
        (compile "classify")
        (classify (cons (Int 1) ()))
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
    let src2 = r#"
        (defun classify ((s Sexpr)) bool (consp s))
        (compile "classify")
        (classify (Int 1))
    "#;
    assert_eq!(run(src2), RtValue::Bool(false));
    let src3 = r#"
        (defun is-atom ((s Sexpr)) bool (atom s))
        (compile "is-atom")
        (and (is-atom (Int 1)) (not (is-atom (cons (Int 1) ()))))
    "#;
    assert_eq!(run(src3), RtValue::Bool(true));
}

/// `set-car`/`set-cdr` (Phase 5) — in-place mutation of an existing cons
/// cell, visible through another reference to the same cell (the same
/// aliasing `tests/prelude_test.rs`'s `set_car_overwrites_in_place` checks
/// for the tree-walking interpreter, here exercised through compiled code).
/// The new value (`Int 99`) is a non-`Cons` `Sexpr`, exercising the same
/// widened representation as the round-trip tests above.
#[test]
fn compiles_a_function_using_set_car_and_set_cdr() {
    let src = r#"
        (defun mutate ((cell Sexpr)) Sexpr (progn (set-car cell (Int 99)) (set-cdr cell (Int 100)) cell))
        (compile "mutate")
        (let* ((pair (cons (Int 1) (Int 2)))
               (alias pair))
          (mutate pair)
          (match (car alias)
            ((Int n) (match (cdr alias) ((Int m) (and (= n 99) (= m 100))) (_ false)))
            (_ false)))
    "#;
    assert_eq!(run(src), RtValue::Bool(true));
}
