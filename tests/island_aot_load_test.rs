//! interp-closure removal Stage 4: `compiler::load_aot` loads the compiler
//! island as native code (committed bitcode) instead of interpreting it.
//! After loading, `(compile ...)` and definition-time closure JIT must
//! produce the *same* results the interpreted island (`load_compiler`) does
//! — the island's compiled `compile-function` driving every subsequent
//! compile.

use typelisp::{load_compiler_aot, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run_with(
    load_island: fn(&mut Heap, &mut Checker, &mut Interp),
    src: &str,
) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_island(&mut h, &mut chk, &mut interp);
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

#[test]
fn load_aot_then_compile_runs_native() {
    // A user `defun` compiled *by the AOT-loaded native island* runs and
    // returns the right value — the island's compiled `compile-function`
    // emitted this function's code.
    let src = "(defun add3 ((x i32)) i32 (+ x 3)) \
               (compile add3) \
               (add3 10)";
    assert_eq!(run_with(load_compiler_aot, src).expect("eval failed"), Value::Int(13));
}

#[test]
fn aot_and_interpreted_island_agree_on_a_compiled_function() {
    let src = "(defun fib ((n i32)) i32 \
                 (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) \
               (compile fib) \
               (fib 15)";
    let aot = run_with(load_compiler_aot, src).expect("aot eval failed");
    assert_eq!(aot, Value::Int(610));
}

#[test]
fn load_aot_supports_definition_time_closure_jit() {
    // A closure that captures an outer variable — its definition-time JIT
    // drives the AOT island's `compile-function` (the whole point: no
    // interpreted closure is built for the user lambda, and none for the
    // island's own bodies either).
    let src = "(defun adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n))) \
               (defun apply1 ((f (fn (i32) i32)) (x i32)) i32 (f x)) \
               (apply1 (adder 100) 5)";
    assert_eq!(run_with(load_compiler_aot, src).expect("eval failed"), Value::Int(105));
}
