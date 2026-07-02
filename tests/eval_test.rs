//! Tests for the tree-walking interpreter over the typed AST (step 4a).

extern crate typelisp;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, RtValue, Value};

/// Read, type-check, and evaluate a program; return the last expression's value.
fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(8192);
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

/// Like [`run`], but with the prelude loaded first — needed for `while`/
/// `dotimes`/`dolist`/`when`/`unless`/`and`/`or`/`cond`/`if-let`, which are
/// `defmacro`s in `src/prelude.rs` rather than checker-native special forms
/// (see that file's "loop/branch primitive reduction" comment).
fn run_with_prelude(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
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

fn eval_ok_with_prelude(src: &str) -> RtValue {
    run_with_prelude(src).expect("eval failed")
}

// ---- literals / control -----------------------------------------------------

#[test]
fn literals() {
    assert_eq!(eval_ok("42"), RtValue::Int(42));
    assert_eq!(eval_ok("true"), RtValue::Bool(true));
    assert_eq!(eval_ok("#\\a"), RtValue::Char('a'));
    assert_eq!(eval_ok("\"hi\""), RtValue::Str("hi".into()));
    assert_eq!(eval_ok("()"), RtValue::Unit);
}

#[test]
fn if_and_let() {
    assert_eq!(eval_ok("(if true 1 2)"), RtValue::Int(1));
    assert_eq!(eval_ok("(if false 1 2)"), RtValue::Int(2));
    assert_eq!(eval_ok("(let ((x 5) (y 7)) y)"), RtValue::Int(7));
}

// ---- functions --------------------------------------------------------------

#[test]
fn defun_and_call() {
    assert_eq!(eval_ok("(defun id ((x i32)) i32 x) (id 7)"), RtValue::Int(7));
}

// ---- constructors / match ---------------------------------------------------

#[test]
fn unwrap_or_some() {
    let src = "(defun unwrap-or ((o Option<i32>) (d i32)) i32 \
                 (match o ((Some v) v) ((None) d))) \
               (unwrap-or (option::some 5) 0)";
    assert_eq!(eval_ok(src), RtValue::Int(5));
}

#[test]
fn unwrap_or_none() {
    let src = "(defun unwrap-or ((o Option<i32>) (d i32)) i32 \
                 (match o ((Some v) v) ((None) d))) \
               (unwrap-or (option::none) 9)";
    assert_eq!(eval_ok(src), RtValue::Int(9));
}

#[test]
fn result_match() {
    let prog = "(defun unwrap-i ((r Result<i32,Error>)) i32 \
                  (match r ((Ok v) v) ((Err e) (panic \"err\")))) ";
    assert_eq!(eval_ok(&format!("{} (unwrap-i (result::ok 9))", prog)), RtValue::Int(9));
    assert_eq!(
        run(&format!("{} (unwrap-i (result::err (error::error \"boom\")))", prog)),
        Err(EvalError::Panic("err".into()))
    );
}

// ---- panic ------------------------------------------------------------------

#[test]
fn panic_propagates() {
    let src = "(defun boom () i32 (panic \"kaboom\")) (boom)";
    assert_eq!(run(src), Err(EvalError::Panic("kaboom".into())));
}

#[test]
fn module_function_runs() {
    let src = "(module m (pub defun id ((x i32)) i32 x)) (m::id 42)";
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

// ---- builtin arithmetic / comparison (i32) ---------------------------------

#[test]
fn arithmetic() {
    assert_eq!(eval_ok("(+ 2 3)"), RtValue::Int(5));
    assert_eq!(eval_ok("(- 10 4)"), RtValue::Int(6));
    assert_eq!(eval_ok("(* 6 7)"), RtValue::Int(42));
    assert_eq!(eval_ok("(/ 20 5)"), RtValue::Int(4));
    assert_eq!(eval_ok("(mod 17 5)"), RtValue::Int(2));
}

#[test]
fn comparison() {
    assert_eq!(eval_ok("(< 1 2)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(<= 2 2)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(> 1 2)"), RtValue::Bool(false));
    assert_eq!(eval_ok("(= 3 3)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(/= 3 4)"), RtValue::Bool(true));
}

#[test]
fn recursive_factorial() {
    let src = "(defun fact ((n i32)) i32 (if (<= n 1) 1 (* n (fact (- n 1))))) (fact 5)";
    assert_eq!(eval_ok(src), RtValue::Int(120));
}

#[test]
fn recursive_fibonacci() {
    let src = "(defun fib ((n i32)) i32 \
                 (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) \
               (fib 10)";
    assert_eq!(eval_ok(src), RtValue::Int(55));
}

#[test]
fn divide_by_zero_panics() {
    let src = "(defun d ((a i32) (b i32)) i32 (/ a b)) (d 6 0)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

// ---- derived control special forms -----------------------------------------

#[test]
fn when_and_unless_are_unit() {
    assert_eq!(eval_ok_with_prelude("(when (< 1 2) 5)"), RtValue::Unit);
    assert_eq!(eval_ok_with_prelude("(unless (< 2 1) 5)"), RtValue::Unit);
}

#[test]
fn and_short_circuits_to_bool() {
    assert_eq!(eval_ok_with_prelude("(and (< 1 2) (< 2 3))"), RtValue::Bool(true));
    assert_eq!(eval_ok_with_prelude("(and (< 1 2) (< 3 2))"), RtValue::Bool(false));
    assert_eq!(eval_ok_with_prelude("(and)"), RtValue::Bool(true));
}

#[test]
fn or_short_circuits_to_bool() {
    assert_eq!(eval_ok_with_prelude("(or (< 3 2) (< 1 2))"), RtValue::Bool(true));
    assert_eq!(eval_ok_with_prelude("(or (< 3 2) (< 4 2))"), RtValue::Bool(false));
    assert_eq!(eval_ok_with_prelude("(or)"), RtValue::Bool(false));
}

#[test]
fn cond_selects_first_true_clause() {
    assert_eq!(eval_ok_with_prelude("(cond ((< 1 2) 10) (else 20))"), RtValue::Int(10));
    assert_eq!(
        eval_ok_with_prelude("(cond ((< 3 2) 10) ((< 1 2) 20) (else 30))"),
        RtValue::Int(20)
    );
    assert_eq!(eval_ok_with_prelude("(cond ((< 3 2) 10) (else 30))"), RtValue::Int(30));
}

#[test]
fn let_star_binds_sequentially() {
    assert_eq!(eval_ok("(let* ((x 1) (y (+ x 1))) y)"), RtValue::Int(2));
    assert_eq!(eval_ok("(let* ((x 2) (y (* x x)) (z (+ y 1))) z)"), RtValue::Int(5));
}

// ---- mutable variables: setf + while ---------------------------------------

#[test]
fn while_loop_with_setf() {
    let src = "(defun sum-to ((n i32)) i32 \
                 (let ((sum 0) (i 0)) \
                   (while (< i n) (setf sum (+ sum i)) (setf i (+ i 1))) \
                   sum)) \
               (sum-to 5)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(10)); // 0+1+2+3+4
}

#[test]
fn setf_returns_assigned_value() {
    assert_eq!(eval_ok("(let ((x 0)) (setf x 7))"), RtValue::Int(7));
}

#[test]
fn while_is_unit() {
    assert_eq!(eval_ok_with_prelude("(let ((i 0)) (while (< i 0) (setf i 1)))"), RtValue::Unit);
}

#[test]
fn factorial_via_loop() {
    let src = "(defun fact ((n i32)) i32 \
                 (let ((acc 1) (i 1)) \
                   (while (<= i n) (setf acc (* acc i)) (setf i (+ i 1))) \
                   acc)) \
               (fact 5)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(120));
}

// ---- lambda / closures ------------------------------------------------------

#[test]
fn lambda_called_immediately() {
    assert_eq!(eval_ok("((lambda ((x i32)) i32 (+ x 1)) 10)"), RtValue::Int(11));
}

#[test]
fn lambda_bound_in_let() {
    assert_eq!(
        eval_ok("(let ((f (lambda ((x i32)) i32 (* x x)))) (f 5))"),
        RtValue::Int(25)
    );
}

#[test]
fn higher_order_function() {
    let src = "(defun apply-twice ((f (fn (i32) i32)) (x i32)) i32 (f (f x))) \
               (apply-twice (lambda ((n i32)) i32 (+ n 1)) 5)";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn closure_captures_variable() {
    let src = "(defun adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n))) \
               (let ((add5 (adder 5))) (add5 10))";
    assert_eq!(eval_ok(src), RtValue::Int(15));
}

#[test]
fn closure_captures_mutable_state() {
    // The returned closure shares the captured `c` slot across calls.
    let src = "(defun make-counter () (fn () i32) \
                 (let ((c 0)) (lambda () i32 (setf c (+ c 1))))) \
               (let ((next (make-counter))) (next) (next))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

// ---- named functions as values ---------------------------------------------

#[test]
fn named_function_as_value() {
    let src = "(defun inc ((x i32)) i32 (+ x 1)) \
               (defun call2 ((f (fn (i32) i32))) i32 (f (f 0))) \
               (call2 inc)";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn builtin_as_value() {
    let src = "(defun apply2 ((f (fn (i32 i32) i32)) (a i32) (b i32)) i32 (f a b)) \
               (apply2 + 3 4)";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn module_function_as_value() {
    let src = "(module m (pub defun inc ((x i32)) i32 (+ x 1))) \
               (defun c ((f (fn (i32) i32))) i32 (f 9)) \
               (c m::inc)";
    assert_eq!(eval_ok(src), RtValue::Int(10));
}

// ---- dotimes ----------------------------------------------------------------

#[test]
fn dotimes_accumulates() {
    assert_eq!(
        eval_ok_with_prelude("(let ((sum 0)) (dotimes (i 5) (setf sum (+ sum i))) sum)"),
        RtValue::Int(10) // 0+1+2+3+4
    );
}

#[test]
fn dotimes_factorial() {
    let src = "(defun fact ((n i32)) i32 \
                 (let ((acc 1)) (dotimes (i n) (setf acc (* acc (+ i 1)))) acc)) \
               (fact 5)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(120));
}

#[test]
fn dotimes_is_unit() {
    assert_eq!(eval_ok_with_prelude("(dotimes (i 3) ())"), RtValue::Unit);
}

// ---- loop / break / return ---------------------------------------------------

#[test]
fn loop_break_with_no_value_is_unit() {
    assert_eq!(eval_ok("(loop (break))"), RtValue::Unit);
}

#[test]
fn loop_return_yields_its_value() {
    assert_eq!(eval_ok("(loop (return 42))"), RtValue::Int(42));
}

#[test]
fn loop_runs_until_break_with_accumulated_state() {
    // Sum 0..4 by hand-rolled loop+break (vs. while_loop_with_setf's while).
    let src = "(let ((sum 0) (i 0)) \
                 (loop (if (>= i 5) (break) ()) (setf sum (+ sum i)) (setf i (+ i 1))) \
                 sum)";
    assert_eq!(eval_ok(src), RtValue::Int(10));
}

#[test]
fn loop_return_short_circuits_the_body() {
    // `return` exits immediately, skipping the rest of the body and any
    // further iterations.
    let src = "(let ((i 0)) (loop (setf i (+ i 1)) (return i) (setf i 999)))";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn break_exits_while_early() {
    let src = "(let ((i 0)) \
                 (while true (if (>= i 3) (break) ()) (setf i (+ i 1))) \
                 i)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(3));
}

#[test]
fn return_exits_dotimes_early() {
    let src = "(let ((i 0)) \
                 (dotimes (n 100) (setf i n) (if (= n 2) (return) ())) \
                 i)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(2));
}

#[test]
fn break_in_inner_loop_does_not_exit_outer() {
    let src = "(let ((outer 0)) \
                 (dotimes (i 3) (loop (break)) (setf outer (+ outer 1))) \
                 outer)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(3));
}

#[test]
fn nested_loop_factorial_via_return() {
    let src = "(defun fact ((n i32)) i32 \
                 (let ((acc 1) (i 1)) \
                   (loop (if (> i n) (return acc) ()) \
                         (setf acc (* acc i)) \
                         (setf i (+ i 1))))) \
               (fact 5)";
    assert_eq!(eval_ok(src), RtValue::Int(120));
}

// ---- cons / car / cdr / list / dolist ---------------------------------------

/// Read, check, and evaluate a program whose final value is a `Sexpr`. Returns
/// the heap it was built in too — kept alive because the `Value` (e.g. a
/// `Cons`) is a pointer into that heap's arena, dangling once it's dropped.
fn eval_sexpr(src: &str) -> (Heap, Value) {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        RtValue::Sexpr(v) => (h, v),
        other => panic!("expected a Sexpr value, got {:?}", other),
    }
}

/// Structural equality for `Sexpr` values: `Value::Cons`'s derived
/// `PartialEq` is pointer identity, so cons-shaped results need this instead.
/// `Value::Boxed` (a `Sexpr::Float` today, see `BoxedObj`) is identity-based
/// for the same reason, so it needs the same treatment.
fn sexpr_eq(h: &Heap, a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::Empty, Value::Empty) => true,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Boxed(x), Value::Boxed(y)) => h.float_value(x) == h.float_value(y),
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Symbol(x), Value::Symbol(y)) => h.symbol_name(x) == h.symbol_name(y),
        (Value::Str(x), Value::Str(y)) => h.string(x) == h.string(y),
        (Value::Cons(_), Value::Cons(_)) => {
            sexpr_eq(h, h.car(a).unwrap(), h.car(b).unwrap())
                && sexpr_eq(h, h.cdr(a).unwrap(), h.cdr(b).unwrap())
        }
        _ => false,
    }
}

/// Assert that evaluating `src` yields a `Sexpr` structurally equal to the
/// value `expected` builds (in the same heap `src` ran in).
fn assert_sexpr_eq(src: &str, expected: impl FnOnce(&mut Heap) -> Value) {
    let (mut h, actual) = eval_sexpr(src);
    let want = expected(&mut h);
    assert!(sexpr_eq(&h, actual, want), "expected {:?}, got {:?}", want, actual);
}

#[test]
fn cons_car_cdr() {
    assert_sexpr_eq("(cons (Int 1) (Nil))", |h| h.cons(Value::Int(1), Value::Empty).unwrap());
    assert_sexpr_eq("(car (cons (Int 1) (Nil)))", |_| Value::Int(1));
    assert_sexpr_eq("(cdr (cons (Int 1) (Nil)))", |_| Value::Empty);
}

#[test]
fn float_sexpr_constructs_and_extracts_through_a_heap_boxed_value() {
    // `Sexpr::Float` is heap-boxed (`Value::Boxed`, see `BoxedObj`) — this
    // exercises `Heap::alloc_float`/`Interp::construct_sexpr`'s `SEXPR_FLOAT`
    // arm end to end, not just the mem-layer plumbing `mem_test.rs` covers.
    assert_sexpr_eq("(Float 3.5)", |h| h.alloc_float(3.5));
    assert_sexpr_eq("(cons (Float 1.5) (Nil))", |h| {
        let f = h.alloc_float(1.5);
        h.cons(f, Value::Empty).unwrap()
    });
    assert_sexpr_eq(
        "(match (Float 2.0) ((Float f) (Float f)) (_ (Nil)))",
        |h| h.alloc_float(2.0),
    );
}

#[test]
fn car_and_cdr_of_non_cons_panic() {
    assert_eq!(run("(car (Nil))"), Err(EvalError::Panic("car: not a cons".into())));
    assert_eq!(run("(cdr (Int 5))"), Err(EvalError::Panic("cdr: not a cons".into())));
}

#[test]
fn list_builds_cons_chain() {
    assert_sexpr_eq("(list (Int 1) (Int 2))", |h| {
        let tail = h.cons(Value::Int(2), Value::Empty).unwrap();
        h.cons(Value::Int(1), tail).unwrap()
    });
    assert_sexpr_eq("(list)", |_| Value::Empty);
}

#[test]
fn dolist_is_unit() {
    assert_eq!(eval_ok_with_prelude("(dolist (x (list (Int 1))) x)"), RtValue::Unit);
}

#[test]
fn dolist_iterates_over_each_element() {
    let src = "(let ((count 0)) \
                 (dolist (x (list (Int 1) (Int 2) (Int 3))) (setf count (+ count 1))) \
                 count)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(3));
}

#[test]
fn dolist_over_empty_list_does_nothing() {
    let src = "(let ((count 0)) \
                 (dolist (x (list)) (setf count (+ count 1))) \
                 count)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(0));
}

#[test]
fn cons_as_value() {
    let src = "(defun apply2 ((f (fn (Sexpr Sexpr) Sexpr)) (a Sexpr) (b Sexpr)) Sexpr (f a b)) \
               (apply2 cons (Int 1) (Nil))";
    assert_sexpr_eq(src, |h| h.cons(Value::Int(1), Value::Empty).unwrap());
}

// ---- runtime Sexpr values share the GC-managed cons heap --------------------

#[test]
fn runtime_cons_cells_survive_gc_when_rooted() {
    // Read/check against a generously-sized heap, then evaluate against a
    // tiny 2-cell one: one cell permanently held by `kept`, the other cycling
    // through garbage the loop body builds and immediately discards each
    // iteration. With only 2 cells, every other iteration must run a GC to
    // free the previous iteration's garbage before it can allocate again. If
    // `kept` weren't tracked as a GC root, one of those collections would
    // eventually reclaim and corrupt it instead of the garbage.
    let mut src_heap = Heap::with_capacity(4096);
    let r = Reader::new();
    let src = "(let ((kept (cons (Int 1) (Nil)))) \
                 (dotimes (i 50) (cons (Int 2) (Nil))) \
                 (car kept))";
    let vs = r.read_all(&mut src_heap, src).expect("read failed");
    let mut chk = Checker::new();
    let mut check_interp = Interp::new();
    // `dotimes` is a `defmacro` (see `src/prelude.rs`), expanded during
    // checking — `check_interp` needs the prelude actually `exec`'d (not
    // just checked) for `MacroExpander::expand_macro` to find it. The
    // expansion bottoms out in `loop`/`if`/`break`/`setf`/`cons`/`<`/`+`
    // (true Rust builtins/native `Expr` variants) plus a call to `not` —
    // `not` is a plain prelude `defun` now (`loop`/`break`/`return`/`setf`
    // stage), not a builtin, so unlike the others its *body* must actually
    // be registered in whichever `Interp` runs the expanded `Expr::Call`,
    // not just present at check time — this is why the loop below reuses
    // `check_interp` itself (already holding the loaded prelude's `Interp.fns`)
    // rather than a second, fresh `Interp::new()`. `Interp` carries no heap
    // state of its own (every `exec`/`add_compiled_function` call takes one
    // as an explicit argument), so reusing it here doesn't tie this test's
    // GC-pressure heap to the large one `load_prelude` ran against above.
    load_prelude(&mut src_heap, &mut chk, &mut check_interp);
    let tls: Vec<_> = vs
        .into_iter()
        .map(|v| chk.check_form(&mut src_heap, &check_interp, v).expect("check failed"))
        .collect();

    let mut rt_heap = Heap::with_capacity(2);
    let mut last = RtValue::Unit;
    for tl in tls {
        if let Some(val) = check_interp.exec(&mut rt_heap, tl).expect("eval failed") {
            last = val;
        }
    }
    assert_eq!(last, RtValue::Sexpr(Value::Int(1)));
    // GC is lazy (it only runs when an allocation needs space), so the final
    // iteration's garbage cell is still live; one more collection reclaims it.
    // `kept`'s cell — still rooted — is the only thing left afterwards.
    rt_heap.gc();
    assert_eq!(rt_heap.live_count(), 1);
}

// ---- global definitions: defvar / defconstant ------------------------------

#[test]
fn defvar_global_read() {
    assert_eq!(eval_ok("(defvar g 42) g"), RtValue::Int(42));
}

#[test]
fn defvar_visible_in_function() {
    assert_eq!(eval_ok("(defvar g 10) (defun f () i32 g) (f)"), RtValue::Int(10));
}

#[test]
fn defvar_is_mutable() {
    let src = "(defvar c 0) \
               (defun bump () i32 (setf c (+ c 1))) \
               (bump) (bump) c";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn defconstant_read() {
    assert_eq!(eval_ok("(defconstant k 5) k"), RtValue::Int(5));
}

#[test]
fn typed_defvar() {
    assert_eq!(eval_ok("(defvar (g i32) 7) g"), RtValue::Int(7));
}

#[test]
fn module_global_via_path() {
    assert_eq!(eval_ok("(module m (pub defvar g 7)) m::g"), RtValue::Int(7));
}

#[test]
fn cond_with_classify() {
    let src = "(defun classify ((n i32)) i32 \
                 (cond ((< n 0) (- 0 1)) ((= n 0) 0) (else 1))) \
               (classify 7)";
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(1));
}

// ---- labels (roadmap step 9) -------------------------------------------------

#[test]
fn labels_self_recursion() {
    let src = "(labels ((fact ((n i32)) i32 (if (= n 0) 1 (* n (fact (- n 1)))))) (fact 5))";
    assert_eq!(eval_ok(src), RtValue::Int(120));
}

#[test]
fn labels_mutual_recursion() {
    let src = "(labels ((is-even ((n i32)) bool (if (= n 0) true (is-odd (- n 1))))
                        (is-odd ((n i32)) bool (if (= n 0) false (is-even (- n 1)))))
                 (is-even 10))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn labels_trailing_body_can_call_multiple_functions() {
    let src = "(labels ((double ((n i32)) i32 (* n 2))) (+ (double 3) (double 4)))";
    assert_eq!(eval_ok(src), RtValue::Int(14));
}

#[test]
fn labels_nested_inside_a_defun_still_self_recurses() {
    let src = "(defun run-it () i32
                  (labels ((sum-to ((n i32)) i32 (if (= n 0) 0 (+ n (sum-to (- n 1))))))
                    (sum-to 4)))
                (run-it)";
    assert_eq!(eval_ok(src), RtValue::Int(10));
}

#[test]
fn labels_function_can_be_bound_to_a_variable_like_any_other() {
    let src = "(labels ((add1 ((n i32)) i32 (+ n 1)))
                  (let ((f add1)) (f 10)))";
    assert_eq!(eval_ok(src), RtValue::Int(11));
}

// ---- &rest / apply (roadmap step 10) -----------------------------------------
//
// `&rest`'s collected arguments are bound to a plain `Sexpr` list (an
// ordinary Lisp list of cons cells, like every Lisp's `&rest` parameter —
// never a homogeneous array type), with each element wrapped in its
// `Sexpr` constructor (`(Int n)` for an `i32`/`i64` element here) — see
// `Checker::wrap_rest_elem`'s doc comment. These tests use `run`/`eval_ok`
// (no prelude loaded), so list length/indexing is done directly via
// `match`/`car`/`cdr` rather than the prelude's `length`/`nth`.

const LEN_HELPER: &str =
    "(defun len ((s Sexpr)) i32 (match s ((Nil) 0) ((Cons _ d) (+ 1 (len d))) (_ (panic \"len: not a proper list\"))))";

#[test]
fn defun_rest_collects_extra_arguments_into_a_list() {
    let src = format!(
        "{}
         (defun count-extra ((a i32) &rest (xs i32)) i32 (len xs))
         (count-extra 1 2 3 4)",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), RtValue::Int(3));
}

#[test]
fn defun_rest_with_no_extra_arguments_is_an_empty_list() {
    let src = format!(
        "{}
         (defun count-extra ((a i32) &rest (xs i32)) i32 (len xs))
         (count-extra 1)",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), RtValue::Int(0));
}

#[test]
fn defun_rest_with_no_fixed_params_collects_every_argument() {
    let src = format!(
        "{}
         (defun count-all (&rest (xs i32)) i32 (len xs)) (count-all 1 2 3)",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), RtValue::Int(3));
}

#[test]
fn defun_rest_elements_keep_their_order_and_values() {
    // `Sexpr`'s own `Int` constructor always holds an `i64` (regardless of
    // whether the `&rest` element type was declared `i32` or `i64` — both
    // wrap into the same `Sexpr` variant, see `sexpr_ctor_for`), so
    // extracting one back out via `match` yields `i64`, not `i32`.
    let src = "(defun second-extra ((a i32) &rest (xs i32)) i64
                 (match (car (cdr xs)) ((Int n) n) (_ (panic \"not an Int\"))))
               (second-extra 1 10 20 30)";
    assert_eq!(eval_ok(src), RtValue::Int(20));
}

#[test]
fn lambda_rest_collects_extra_arguments_into_a_list() {
    let src = format!(
        "{}
         ((lambda ((a i32) &rest (xs i32)) i32 (len xs)) 1 2 3)",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), RtValue::Int(2));
}

#[test]
fn generic_rest_function_works_at_different_element_types() {
    let src = "(defun (firstn T) ((a T) &rest (xs T)) T a)
               (firstn (firstn 1 2 3) (firstn 4 5))";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn apply_calls_a_named_variadic_function_with_a_runtime_list() {
    let src = "(defun first-extra ((a i32) &rest (xs i32)) i64
                 (match (car xs) ((Int n) n) (_ (panic \"not an Int\"))))
               (apply first-extra 1 (quote (10 20)))";
    assert_eq!(eval_ok(src), RtValue::Int(10));
}

#[test]
fn apply_calls_a_variadic_lambda_value() {
    let src = format!(
        "{}
         (let ((f (lambda ((a i32) &rest (xs i32)) i32 (+ a (len xs)))))
           (apply f 10 (quote (1 2))))",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), RtValue::Int(12));
}

#[test]
fn apply_with_no_fixed_arguments_passes_the_whole_list_as_rest() {
    let src = format!(
        "{}
         (defun count-all (&rest (xs i32)) i32 (len xs))
         (apply count-all (quote (1 2 3)))",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), RtValue::Int(3));
}

// ---- the (type annotation) ---------------------------------------------------

#[test]
fn the_is_transparent_at_runtime() {
    assert_eq!(eval_ok("(the i64 5)"), RtValue::Int(5));
}

// ---- unreachable / todo -------------------------------------------------------

#[test]
fn unreachable_panics() {
    let src = "(defun boom () i32 (unreachable)) (boom)";
    assert_eq!(run_with_prelude(src), Err(EvalError::Panic("unreachable".into())));
}

#[test]
fn todo_panics() {
    let src = "(defun boom () i32 (todo)) (boom)";
    assert_eq!(run_with_prelude(src), Err(EvalError::Panic("todo".into())));
}
