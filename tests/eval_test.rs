//! Tests for the tree-walking interpreter over the typed AST (step 4a).

extern crate typelisp;
use typelisp::{load_prelude, load_compiler, Checker, EvalError, Heap, Interp, Reader, Value};

/// Read, type-check, and evaluate a program; return the last expression's value.
///
/// Loads the prelude and the native compiler island first: a
/// `lambda`/`labels`/`FnRef` value JIT-compiles through the island when the
/// island is there, which is the configuration the many closure tests here
/// are meant to exercise, so `eval_ok` loads it. Loading before
/// the read also keeps the program's GC roots off the stack while the island
/// loads; the heap is sized for prelude + island accordingly.
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

/// The text of a `string` result.
///
/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — and `run` above drops its heap on return.
/// Hence this parallel runner, which reads the text out first.
fn eval_string(src: &str) -> String {
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
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        typelisp::Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// Like [`run`], but with the prelude loaded first — needed for `while`/
/// `dotimes`/`dolist`/`when`/`unless`/`and`/`or`/`cond`/`if-let`, which are
/// `defmacro`s in `src/prelude.rs` rather than checker-native special forms
/// (see that file's "loop/branch primitive reduction" comment).
fn run_with_prelude(src: &str) -> Result<Value, EvalError> {
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

fn eval_ok_with_prelude(src: &str) -> Value {
    run_with_prelude(src).expect("eval failed")
}

// ---- literals / control -----------------------------------------------------

#[test]
fn literals() {
    assert_eq!(eval_ok("42"), Value::Int(42));
    assert_eq!(eval_ok("true"), Value::Bool(true));
    assert_eq!(eval_ok("#\\a"), Value::Char('a'));
    assert_eq!(eval_string("\"hi\""), "hi");
    assert_eq!(eval_ok("()"), Value::Empty);
}

#[test]
fn if_and_let() {
    assert_eq!(eval_ok("(if true 1 2)"), Value::Int(1));
    assert_eq!(eval_ok("(if false 1 2)"), Value::Int(2));
    assert_eq!(eval_ok("(let ((x 5) (y 7)) y)"), Value::Int(7));
}

// ---- functions --------------------------------------------------------------

#[test]
fn defun_and_call() {
    assert_eq!(eval_ok("(defun id ((x i32)) i32 x) (id 7)"), Value::Int(7));
}

// ---- constructors / match ---------------------------------------------------

#[test]
fn unwrap_or_some() {
    let src = "(defun unwrap-or ((o Option<i32>) (d i32)) i32 \
                 (match o ((Some v) v) ((None) d))) \
               (unwrap-or (option::some 5) 0)";
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn unwrap_or_none() {
    let src = "(defun unwrap-or ((o Option<i32>) (d i32)) i32 \
                 (match o ((Some v) v) ((None) d))) \
               (unwrap-or (option::none) 9)";
    assert_eq!(eval_ok(src), Value::Int(9));
}

#[test]
fn result_match() {
    let prog = "(defun unwrap-i ((r Result<i32,ParseIntError>)) i32 \
                  (match r ((Ok v) v) ((Err e) (panic \"err\")))) ";
    assert_eq!(eval_ok(&format!("{} (unwrap-i (result::ok 9))", prog)), Value::Int(9));
    assert_eq!(
        run(&format!("{} (unwrap-i (result::err (ParseIntError::ParseIntError \"boom\")))", prog)),
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
    assert_eq!(eval_ok(src), Value::Int(42));
}

// ---- builtin arithmetic / comparison (i32) ---------------------------------

#[test]
fn arithmetic() {
    assert_eq!(eval_ok("(+ 2 3)"), Value::Int(5));
    assert_eq!(eval_ok("(- 10 4)"), Value::Int(6));
    assert_eq!(eval_ok("(* 6 7)"), Value::Int(42));
    assert_eq!(eval_ok("(/ 20 5)"), Value::Int(4));
    assert_eq!(eval_ok("(mod 17 5)"), Value::Int(2));
}

/// CL `mod` is floored — the result takes the sign of the divisor. (`mod` is a
/// registry builtin, available without the prelude; `rem` — a `prelude.rs`
/// method — is covered in `numeric_test`/`bignum_ratio_test`, which load it.)
#[test]
fn mod_is_floored() {
    assert_eq!(eval_ok("(mod -7 3)"), Value::Int(2));
    assert_eq!(eval_ok("(mod 7 -3)"), Value::Int(-2));
    assert_eq!(eval_ok("(mod -7 -3)"), Value::Int(-1));
    assert_eq!(eval_ok("(mod 7 3)"), Value::Int(1));
}

#[test]
fn comparison() {
    assert_eq!(eval_ok("(< 1 2)"), Value::Bool(true));
    assert_eq!(eval_ok("(<= 2 2)"), Value::Bool(true));
    assert_eq!(eval_ok("(> 1 2)"), Value::Bool(false));
    assert_eq!(eval_ok("(= 3 3)"), Value::Bool(true));
    assert_eq!(eval_ok("(/= 3 4)"), Value::Bool(true));
}

#[test]
fn recursive_factorial() {
    let src = "(defun fact ((n i32)) i32 (if (<= n 1) 1 (* n (fact (- n 1))))) (fact 5)";
    assert_eq!(eval_ok(src), Value::Int(120));
}

#[test]
fn recursive_fibonacci() {
    let src = "(defun fib ((n i32)) i32 \
                 (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) \
               (fib 10)";
    assert_eq!(eval_ok(src), Value::Int(55));
}

#[test]
fn divide_by_zero_panics() {
    let src = "(defun d ((a i32) (b i32)) i32 (/ a b)) (d 6 0)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

// ---- derived control special forms -----------------------------------------

#[test]
fn when_and_unless_are_unit() {
    assert_eq!(eval_ok_with_prelude("(when (< 1 2) 5)"), Value::Empty);
    assert_eq!(eval_ok_with_prelude("(unless (< 2 1) 5)"), Value::Empty);
}

#[test]
fn and_short_circuits_to_bool() {
    assert_eq!(eval_ok_with_prelude("(and (< 1 2) (< 2 3))"), Value::Bool(true));
    assert_eq!(eval_ok_with_prelude("(and (< 1 2) (< 3 2))"), Value::Bool(false));
    assert_eq!(eval_ok_with_prelude("(and)"), Value::Bool(true));
}

#[test]
fn or_short_circuits_to_bool() {
    assert_eq!(eval_ok_with_prelude("(or (< 3 2) (< 1 2))"), Value::Bool(true));
    assert_eq!(eval_ok_with_prelude("(or (< 3 2) (< 4 2))"), Value::Bool(false));
    assert_eq!(eval_ok_with_prelude("(or)"), Value::Bool(false));
}

#[test]
fn cond_selects_first_true_clause() {
    assert_eq!(eval_ok_with_prelude("(cond ((< 1 2) 10) (else 20))"), Value::Int(10));
    assert_eq!(
        eval_ok_with_prelude("(cond ((< 3 2) 10) ((< 1 2) 20) (else 30))"),
        Value::Int(20)
    );
    assert_eq!(eval_ok_with_prelude("(cond ((< 3 2) 10) (else 30))"), Value::Int(30));
}

#[test]
fn let_star_binds_sequentially() {
    assert_eq!(eval_ok("(let* ((x 1) (y (+ x 1))) y)"), Value::Int(2));
    assert_eq!(eval_ok("(let* ((x 2) (y (* x x)) (z (+ y 1))) z)"), Value::Int(5));
}

// ---- mutable variables: setf + while ---------------------------------------

#[test]
fn while_loop_with_setf() {
    let src = "(defun sum-to ((n i32)) i32 \
                 (let ((sum 0) (i 0)) \
                   (while (< i n) (setf sum (+ sum i)) (setf i (+ i 1))) \
                   sum)) \
               (sum-to 5)";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(10)); // 0+1+2+3+4
}

#[test]
fn setf_returns_assigned_value() {
    assert_eq!(eval_ok("(let ((x 0)) (setf x 7))"), Value::Int(7));
}

#[test]
fn while_is_unit() {
    assert_eq!(eval_ok_with_prelude("(let ((i 0)) (while (< i 0) (setf i 1)))"), Value::Empty);
}

#[test]
fn factorial_via_loop() {
    let src = "(defun fact ((n i32)) i32 \
                 (let ((acc 1) (i 1)) \
                   (while (<= i n) (setf acc (* acc i)) (setf i (+ i 1))) \
                   acc)) \
               (fact 5)";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(120));
}

// ---- lambda / closures ------------------------------------------------------

#[test]
fn lambda_called_immediately() {
    assert_eq!(eval_ok("((lambda ((x i32)) i32 (+ x 1)) 10)"), Value::Int(11));
}

#[test]
fn lambda_bound_in_let() {
    assert_eq!(
        eval_ok("(let ((f (lambda ((x i32)) i32 (* x x)))) (f 5))"),
        Value::Int(25)
    );
}

#[test]
fn higher_order_function() {
    let src = "(defun apply-twice ((f (fn (i32) i32)) (x i32)) i32 (f (f x))) \
               (apply-twice (lambda ((n i32)) i32 (+ n 1)) 5)";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn closure_captures_variable() {
    let src = "(defun adder ((n i32)) (fn (i32) i32) (lambda ((x i32)) i32 (+ x n))) \
               (let ((add5 (adder 5))) (add5 10))";
    assert_eq!(eval_ok(src), Value::Int(15));
}

#[test]
fn closure_captures_mutable_state() {
    // The returned closure shares the captured `c` slot across calls.
    let src = "(defun make-counter () (fn () i32) \
                 (let ((c 0)) (lambda () i32 (setf c (+ c 1))))) \
               (let ((next (make-counter))) (next) (next))";
    assert_eq!(eval_ok(src), Value::Int(2));
}

// ---- named functions as values ---------------------------------------------

#[test]
fn named_function_as_value() {
    let src = "(defun inc ((x i32)) i32 (+ x 1)) \
               (defun call2 ((f (fn (i32) i32))) i32 (f (f 0))) \
               (call2 inc)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn builtin_as_value() {
    let src = "(defun apply2 ((f (fn (i32 i32) i32)) (a i32) (b i32)) i32 (f a b)) \
               (apply2 + 3 4)";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn module_function_as_value() {
    let src = "(module m (pub defun inc ((x i32)) i32 (+ x 1))) \
               (defun c ((f (fn (i32) i32))) i32 (f 9)) \
               (c m::inc)";
    assert_eq!(eval_ok(src), Value::Int(10));
}

// ---- dotimes ----------------------------------------------------------------

#[test]
fn dotimes_accumulates() {
    assert_eq!(
        eval_ok_with_prelude("(let ((sum 0)) (dotimes (i 5) (setf sum (+ sum i))) sum)"),
        Value::Int(10) // 0+1+2+3+4
    );
}

#[test]
fn dotimes_factorial() {
    let src = "(defun fact ((n i32)) i32 \
                 (let ((acc 1)) (dotimes (i n) (setf acc (* acc (+ i 1)))) acc)) \
               (fact 5)";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(120));
}

#[test]
fn dotimes_is_unit() {
    assert_eq!(eval_ok_with_prelude("(dotimes (i 3) ())"), Value::Empty);
}

// ---- loop / break / return ---------------------------------------------------

#[test]
fn loop_break_with_no_value_is_unit() {
    assert_eq!(eval_ok("(loop (break))"), Value::Empty);
}

#[test]
fn loop_return_yields_its_value() {
    assert_eq!(eval_ok("(loop (return 42))"), Value::Int(42));
}

#[test]
fn loop_runs_until_break_with_accumulated_state() {
    // Sum 0..4 by hand-rolled loop+break (vs. while_loop_with_setf's while).
    let src = "(let ((sum 0) (i 0)) \
                 (loop (if (>= i 5) (break) ()) (setf sum (+ sum i)) (setf i (+ i 1))) \
                 sum)";
    assert_eq!(eval_ok(src), Value::Int(10));
}

#[test]
fn loop_return_short_circuits_the_body() {
    // `return` exits immediately, skipping the rest of the body and any
    // further iterations.
    let src = "(let ((i 0)) (loop (setf i (+ i 1)) (return i) (setf i 999)))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn break_exits_while_early() {
    let src = "(let ((i 0)) \
                 (while true (if (>= i 3) (break) ()) (setf i (+ i 1))) \
                 i)";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(3));
}

#[test]
fn return_exits_dotimes_early() {
    let src = "(let ((i 0)) \
                 (dotimes (n 100) (setf i n) (if (= n 2) (return) ())) \
                 i)";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(2));
}

#[test]
fn break_in_inner_loop_does_not_exit_outer() {
    let src = "(let ((outer 0)) \
                 (dotimes (i 3) (loop (break)) (setf outer (+ outer 1))) \
                 outer)";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(3));
}

#[test]
fn nested_loop_factorial_via_return() {
    let src = "(defun fact ((n i32)) i32 \
                 (let ((acc 1) (i 1)) \
                   (loop (if (> i n) (return acc) ()) \
                         (setf acc (* acc i)) \
                         (setf i (+ i 1))))) \
               (fact 5)";
    assert_eq!(eval_ok(src), Value::Int(120));
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
    let interp = Interp::new();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        v => (h, v),
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

// Free `cons`/`car`/`cdr` are the generic `cons<T,U>` pair now (Symbol/Sexpr
// redesign Phase 4b — see the pair tests in `prelude_test.rs`), not `Sexpr`
// cons operations. `Sexpr` cons/nil is built and walked through the `sexpr-*`
// layer (`sexpr_cons_car_cdr_mirror...` below).

#[test]
fn float_sexpr_constructs_and_extracts_through_a_heap_boxed_value() {
    // `Sexpr::Float` is heap-boxed (`Value::Boxed`, see `BoxedObj`) — this
    // exercises `Heap::alloc_float`/`Interp::construct_sexpr`'s `SEXPR_FLOAT`
    // arm end to end, not just the mem-layer plumbing `mem_test.rs` covers.
    assert_sexpr_eq("(Float 3.5)", |h| h.alloc_float(3.5));
    assert_sexpr_eq("(sexpr-cons (Float 1.5) ())", |h| {
        let f = h.alloc_float(1.5);
        h.cons(f, Value::Empty).unwrap()
    });
    // (The former `(match (Float 2.0) ((Float f) ...))` extraction assertion
    // was dropped: `match` on a `Sexpr` value is fenced off — Symbol/Sexpr
    // redesign Phase 5.)
}

// ---- internal Sexpr navigation layer (Symbol/Sexpr redesign Phase 1) --------

#[test]
fn sexpr_cons_car_cdr_mirror_the_user_facing_ops() {
    // `sexpr-cons`/`sexpr-car`/`sexpr-cdr` are the island's `Sexpr` cons/nil
    // operations — the free `cons`/`car`/`cdr` names are the generic `cons<T,U>`
    // pair now (Phase 4b), so a `Sexpr` list is built and walked here.
    assert_sexpr_eq("(sexpr-cons (Int 1) ())", |h| h.cons(Value::Int(1), Value::Empty).unwrap());
    assert_sexpr_eq("(sexpr-car (sexpr-cons (Int 1) ()))", |_| Value::Int(1));
    assert_sexpr_eq("(sexpr-cdr (sexpr-cons (Int 1) ()))", |_| Value::Empty);
}

#[test]
fn sexpr_car_and_cdr_of_a_non_cons_atom_panic() {
    // An *atom* is a type confusion, so it still panics — CL rejects
    // `(car 5)` too.
    assert_eq!(run("(sexpr-car (Int 5))"), Err(EvalError::Panic("sexpr-car: not a cons".into())));
    assert_eq!(run("(sexpr-cdr (Int 5))"), Err(EvalError::Panic("sexpr-cdr: not a cons".into())));
    // The empty list is not a confusion, it is the end of a list — and both
    // tiers answer with the empty list rather than panicking (CL's
    // `(car nil) => nil`). `tests/match_sexpr_test.rs` asserts the value; the
    // point here is only that it is no longer an error.
    assert!(run("(sexpr-car (the Option<Sexpr> ()))").is_ok());
    assert!(run("(sexpr-cdr (the Option<Sexpr> ()))").is_ok());
}

#[test]
fn sexpr_tag_predicates_read_the_runtime_tag() {
    // The `sexpr-consp`/`sexpr-null`/`sexpr-atom` predicates inspect the runtime
    // tag directly (no `match`), so they survive Phase 5's `match`-to-enum fence.
    assert_eq!(eval_ok("(sexpr-consp (sexpr-cons (Int 1) ()))"), Value::Bool(true));
    assert_eq!(eval_ok("(sexpr-consp ())"), Value::Bool(false));
    assert_eq!(eval_ok("(sexpr-consp (Int 3))"), Value::Bool(false));

    assert_eq!(eval_ok("(sexpr-null ())"), Value::Bool(true));
    assert_eq!(eval_ok("(sexpr-null (sexpr-cons (Int 1) ()))"), Value::Bool(false));
    assert_eq!(eval_ok("(sexpr-null (Int 3))"), Value::Bool(false));

    assert_eq!(eval_ok("(sexpr-atom ())"), Value::Bool(true));
    assert_eq!(eval_ok("(sexpr-atom (Int 3))"), Value::Bool(true));
    assert_eq!(eval_ok("(sexpr-atom (sexpr-cons (Int 1) ()))"), Value::Bool(false));
}

#[test]
fn list_builds_cons_chain() {
    assert_sexpr_eq("(list (Int 1) (Int 2))", |h| {
        let tail = h.cons(Value::Int(2), Value::Empty).unwrap();
        h.cons(Value::Int(1), tail).unwrap()
    });
    assert_sexpr_eq("(list)", |_| Value::Empty);
}

// `dolist` (iterating a `Sexpr` list) was removed with the rest of the
// user-facing `Sexpr` list surface — Symbol/Sexpr redesign Phase 5.

#[test]
fn sexpr_cons_as_value() {
    // `sexpr-cons` passed as a higher-order function value (the free `cons` is
    // the generic `cons<T,U>` pair now — Symbol/Sexpr redesign Phase 4b).
    let src = "(defun apply2 ((f (fn (Option<Sexpr> Option<Sexpr>) Option<Sexpr>)) \
                              (a Option<Sexpr>) (b Option<Sexpr>)) Option<Sexpr> (f a b)) \
               (apply2 sexpr-cons (Int 1) ())";
    assert_sexpr_eq(src, |h| h.cons(Value::Int(1), Value::Empty).unwrap());
}

// ---- runtime Sexpr values share the GC-managed cons heap --------------------

#[test]
fn runtime_cons_cells_survive_gc_when_rooted() {
    // One heap, with a collection before every allocation
    // (`eval_under_gc_pressure`'s doc comment explains why the old two-heap,
    // two-cell arrangement is no longer expressible). If `kept` weren't tracked
    // as a GC root, one of those collections would reclaim and corrupt it
    // instead of the loop's garbage.
    let mut src_heap = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let src = "(let ((kept (cons (Int 1) (the Option<Sexpr> ())))) \
                 (dotimes (i 50) (cons (Int 2) (the Option<Sexpr> ()))) \
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
    src_heap.set_gc_stress(true);

    let mut last = Value::Empty;
    for tl in tls {
        if let Some(val) = check_interp.exec(&mut src_heap, tl).expect("eval failed") {
            last = val;
        }
    }
    assert_eq!(last, Value::Int(1));
    // Nothing about the *program* is reclaimable — its cells are permanently
    // rooted by `exec` — so a post-run `live_count() == 0` is no longer the
    // right claim. What this test guards is `kept`'s survival *during* the
    // loop, which the `(car kept)` assertion above already proves under a
    // collection at every allocation.
}

// ---- global definitions: defvar / defconstant ------------------------------

#[test]
fn defvar_global_read() {
    assert_eq!(eval_ok("(defvar (g i32) 42) g"), Value::Int(42));
}

#[test]
fn defvar_visible_in_function() {
    assert_eq!(eval_ok("(defvar (g i32) 10) (defun f () i32 g) (f)"), Value::Int(10));
}

#[test]
fn defvar_is_mutable() {
    let src = "(defvar (c i32) 0) \
               (defun bump () i32 (setf c (+ c 1))) \
               (bump) (bump) c";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn defconstant_read() {
    assert_eq!(eval_ok("(defconstant (k i32) 5) k"), Value::Int(5));
}

#[test]
fn typed_defvar() {
    assert_eq!(eval_ok("(defvar (g i32) 7) g"), Value::Int(7));
}

#[test]
fn module_global_via_path() {
    assert_eq!(eval_ok("(module m (pub defvar (g i32) 7)) m::g"), Value::Int(7));
}

#[test]
fn cond_with_classify() {
    let src = "(defun classify ((n i32)) i32 \
                 (cond ((< n 0) (- 0 1)) ((= n 0) 0) (else 1))) \
               (classify 7)";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(1));
}

// ---- labels (roadmap step 9) -------------------------------------------------

#[test]
fn labels_self_recursion() {
    let src = "(labels ((fact ((n i32)) i32 (if (= n 0) 1 (* n (fact (- n 1)))))) (fact 5))";
    assert_eq!(eval_ok(src), Value::Int(120));
}

#[test]
fn labels_mutual_recursion() {
    let src = "(labels ((is-even ((n i32)) bool (if (= n 0) true (is-odd (- n 1))))
                        (is-odd ((n i32)) bool (if (= n 0) false (is-even (- n 1)))))
                 (is-even 10))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn labels_trailing_body_can_call_multiple_functions() {
    let src = "(labels ((double ((n i32)) i32 (* n 2))) (+ (double 3) (double 4)))";
    assert_eq!(eval_ok(src), Value::Int(14));
}

#[test]
fn labels_nested_inside_a_defun_still_self_recurses() {
    let src = "(defun run-it () i32
                  (labels ((sum-to ((n i32)) i32 (if (= n 0) 0 (+ n (sum-to (- n 1))))))
                    (sum-to 4)))
                (run-it)";
    assert_eq!(eval_ok(src), Value::Int(10));
}

#[test]
fn labels_function_can_be_bound_to_a_variable_like_any_other() {
    let src = "(labels ((add1 ((n i32)) i32 (+ n 1)))
                  (let ((f add1)) (f 10)))";
    assert_eq!(eval_ok(src), Value::Int(11));
}

// ---- &rest / apply (roadmap step 10) -----------------------------------------
//
// `&rest`'s collected arguments are bound to a plain `Sexpr` list (an
// ordinary Lisp list of cons cells, like every Lisp's `&rest` parameter —
// never a homogeneous array type), with each element wrapped in its
// `Sexpr` constructor (`(Int n)` for an `i32`/`i64` element here) — see
// `Checker::wrap_rest_elem`'s doc comment. These tests use `run`/`eval_ok`
// (no prelude loaded), so list length/indexing is done directly via the
// `sexpr-*` accessor layer (`match` on `Sexpr` is fenced off — Symbol/Sexpr
// redesign Phase 5 — and `car`/`cdr` are repurposed to a `cons<T,U>` pair).

const LEN_HELPER: &str =
    "(defun len ((s Option<Sexpr>)) i32 (if (sexpr-consp s) (+ 1 (len (sexpr-cdr s))) 0))";

#[test]
fn defun_rest_collects_extra_arguments_into_a_list() {
    let src = format!(
        "{}
         (defun count-extra ((a i32) &rest (xs i32)) i32 (len xs))
         (count-extra 1 2 3 4)",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), Value::Int(3));
}

#[test]
fn defun_rest_with_no_extra_arguments_is_an_empty_list() {
    let src = format!(
        "{}
         (defun count-extra ((a i32) &rest (xs i32)) i32 (len xs))
         (count-extra 1)",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), Value::Int(0));
}

#[test]
fn defun_rest_with_no_fixed_params_collects_every_argument() {
    let src = format!(
        "{}
         (defun count-all (&rest (xs i32)) i32 (len xs)) (count-all 1 2 3)",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), Value::Int(3));
}

#[test]
fn defun_rest_elements_keep_their_order_and_values() {
    // `Sexpr`'s own `Int` constructor always holds an `i64` (regardless of
    // whether the `&rest` element type was declared `i32` or `i64` — both
    // wrap into the same `Sexpr` variant, see `sexpr_ctor_for`), so
    // extracting one back out via `sexpr-int` yields `i64`, not `i32`.
    let src = "(defun second-extra ((a i32) &rest (xs i32)) i64
                 (sexpr-int (sexpr-car (sexpr-cdr xs))))
               (second-extra 1 10 20 30)";
    assert_eq!(eval_ok(src), Value::Int(20));
}

#[test]
fn lambda_rest_collects_extra_arguments_into_a_list() {
    let src = format!(
        "{}
         ((lambda ((a i32) &rest (xs i32)) i32 (len xs)) 1 2 3)",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), Value::Int(2));
}

#[test]
fn generic_rest_function_works_at_different_element_types() {
    let src = "(defun firstn<T> ((a T) &rest (xs T)) T a)
               (firstn (firstn 1 2 3) (firstn 4 5))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn apply_calls_a_named_variadic_function_with_a_runtime_list() {
    let src = "(defun first-extra ((a i32) &rest (xs i32)) i64
                 (sexpr-int (sexpr-car xs)))
               (apply first-extra 1 (quote (10 20)))";
    assert_eq!(eval_ok(src), Value::Int(10));
}

#[test]
fn apply_calls_a_variadic_lambda_value() {
    let src = format!(
        "{}
         (let ((f (lambda ((a i32) &rest (xs i32)) i32 (+ a (len xs)))))
           (apply f 10 (quote (1 2))))",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), Value::Int(12));
}

#[test]
fn apply_with_no_fixed_arguments_passes_the_whole_list_as_rest() {
    let src = format!(
        "{}
         (defun count-all (&rest (xs i32)) i32 (len xs))
         (apply count-all (quote (1 2 3)))",
        LEN_HELPER
    );
    assert_eq!(eval_ok(&src), Value::Int(3));
}

// ---- the (type annotation) ---------------------------------------------------

#[test]
fn the_is_transparent_at_runtime() {
    assert_eq!(eval_ok("(the i64 5)"), Value::Int(5));
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

// ---- two-tier binding slots (Sexpr/RtValue unification Stage 6a) -------------

/// Shared harness for the heap-cell binding tests below: check and run `src`
/// (with the prelude, for `dotimes`) against a collector that fires at every
/// single `cons` — any binding not protected by its heap cell
/// (`Heap::alloc_cell`'s registry) would be corrupted.
///
/// `gc_stress` rather than a tiny arena. These tests used to read and check
/// against a roomy heap and then execute against a two- or three-cell one, so
/// that the churn forced repeated collections. That is no longer expressible:
/// since the checker lowers code into cons cells, the checked program *is* cells
/// in the heap it was checked against, and handing it to a second heap reads
/// those cells through the wrong symbol/string tables. A three-cell heap could
/// not hold the program at all. Stressing one heap is both the honest shape and
/// strictly stronger pressure: a collection before *every* allocation, not just
/// before the ones a small arena happens to block on.
///
/// Stress goes on *after* the prelude and island load, which are large and would
/// otherwise dominate the runtime without testing anything this file is about.
fn eval_under_gc_pressure(src: &str) -> Value {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    h.set_gc_stress(true);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    last
}

#[test]
fn setf_on_a_sexpr_binding_survives_gc_and_releases_the_old_value() {
    // `s` is rebound mid-loop; the *new* cons must survive every later
    // collection and the old one must be reclaimable (with only 3 cells,
    // the loop can't run at all unless the old value's cell is freed).
    let src = "(let ((s (cons (Int 1) (the Option<Sexpr> ())))) \
                 (setf s (cons (Int 5) (the Option<Sexpr> ()))) \
                 (dotimes (i 40) (cons (Int 2) (the Option<Sexpr> ()))) \
                 (car s))";
    assert_eq!(eval_under_gc_pressure(src), Value::Int(5));
}

#[test]
fn a_sexpr_car_bound_sexpr_survives_gc_pressure() {
    // `match` on a `Sexpr` is fenced off (Symbol/Sexpr redesign Phase 5); the
    // extracted `car` is now bound with `sexpr-car` instead, exercising the
    // same "a let-bound `Sexpr` local stays rooted across GC" path.
    let src = "(let ((s (sexpr-cons (Int 8) ()))) \
                 (let ((h (sexpr-car s))) \
                   (dotimes (i 40) (sexpr-cons (Int 2) ())) h))";
    assert_eq!(eval_under_gc_pressure(src), Value::Int(8));
}

#[test]
fn a_defvar_sexpr_global_survives_gc_pressure_across_forms() {
    let src = "(defvar (g Option<Sexpr>) (sexpr-cons (Int 3) ())) \
               (dotimes (i 40) (sexpr-cons (Int 2) ())) \
               (sexpr-car g)";
    assert_eq!(eval_under_gc_pressure(src), Value::Int(3));
}

#[test]
fn a_lambda_captured_sexpr_binding_survives_gc_pressure() {
    // 48 cells: since interp-closure removal the `lambda` is JIT-compiled at
    // definition time, which itself allocates cons cells for the AST — the old
    // 4-cell heap can't even hold that. 48 clears the JIT floor (~20) with
    // margin; the churn count (200, far over the heap size) is what now forces
    // the repeated collections the captured `s` must survive, so the test still
    // fails if the capture isn't rooted through GC.
    let src = "(let ((s (cons (Int 6) (the Option<Sexpr> ())))) \
                 (let ((f (lambda () Sexpr (car s)))) \
                   (dotimes (i 200) (cons (Int 2) (the Option<Sexpr> ()))) \
                   (f)))";
    assert_eq!(eval_under_gc_pressure(src), Value::Int(6));
}

// ---- heap-boxed closures (Sexpr/RtValue unification Stage 6b) ----------------

#[test]
fn an_unnamed_callee_survives_argument_evaluation_under_gc_pressure() {
    // `((f ...) arg)`-shaped calls bind the callee to nothing — only
    // `Expr::Apply`'s own anchor keeps the closure box alive while the
    // argument churns allocations, which would sweep an unanchored callee. 48
    // cells clears the definition-time JIT floor (interp-closure removal — the
    // old 6-cell heap can't hold the compiled closures' AST); the 200-iteration
    // churn, far over the heap size, is what forces the collections now.
    let src = "(let ((mk (lambda ((s Option<Sexpr>)) (fn (i32) Option<Sexpr>) \
                          (lambda ((n i32)) Option<Sexpr> (sexpr-car s))))) \
                 (let ((f (mk (sexpr-cons (Int 4) ())))) \
                   (dotimes (i 200) (sexpr-cons (Int 2) ())) \
                   (f 0)))";
    assert_eq!(eval_under_gc_pressure(src), Value::Int(4));
}

#[test]
fn labels_siblings_mutually_recurse_under_gc_pressure() {
    // 64 cells clears the definition-time JIT floor for the two `labels`
    // siblings (interp-closure removal — the old 4-cell heap can't hold their
    // compiled AST, and the two-member SCC keeps both siblings' AST sexprs
    // live at once, measured floor ~51); the 200-iteration churn, far over
    // the heap size, forces the collections the mutually-recursive closures
    // must survive.
    let src = "(labels ((is-even ((n i32)) bool (if (= n 0) true (is-odd (- n 1)))) \
                        (is-odd ((n i32)) bool (if (= n 0) false (is-even (- n 1))))) \
                 (dotimes (i 200) (cons (Int 2) (the Option<Sexpr> ()))) \
                 (if (is-even 10) (Int 1) (Int 0)))";
    assert_eq!(eval_under_gc_pressure(src), Value::Int(1));
}

#[test]
fn setf_through_a_shared_capture_is_visible_to_the_sibling_closure() {
    // Two closures capture the same `Sexpr` binding; a write through one is
    // observed by the other — the shared-mutable-cell semantics the heap
    // cell representation must preserve.
    let src = "(let ((s (cons (Int 1) (the Option<Sexpr> ())))) \
                 (let ((write (lambda () () (setf s (cons (Int 9) (the Option<Sexpr> ()))) ())) \
                       (read (lambda () Sexpr (car s)))) \
                   (write) \
                   (read)))";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(9));
}

// (Removed `the_closure_side_table_shrinks_when_the_gc_sweeps_closure_boxes`
// in interp-closure removal Stage 8c.) It proved `Interp`'s interpreted-
// closure side table (`closure_bodies`, drained via
// `Heap::take_dead_closure_tokens`) shrank in step with the GC. Both the side
// table and that drain mechanism are gone: a compiled closure carries its env
// inline, and an interpreted one carries its own code as heap data, so
// neither leaves interpreter-side per-closure state to leak.

/// A runtime error (here a `panic`) carries the source location of the form it
/// came from, so messages read `file:line:col: ...` — down to the innermost
/// list form that raised it, even when raised deep inside a called function.
#[test]
fn runtime_error_carries_source_location() {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    // The `(panic ...)` is on line 2; calling `risky` on line 3 must still
    // report line 2 (where the error actually arose), not the call site.
    let src = "(defun risky ((n i32)) i32\n  (panic \"boom\"))\n(risky 5)";
    let vs = r.read_all_in(&mut h, "prog.typl", src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut err = None;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Err(e) = interp.exec(&mut h, tl) {
            err = Some(e);
            break;
        }
    }
    let err = err.expect("expected a runtime panic");
    let loc = err.loc().expect("runtime error should carry a location");
    assert_eq!(&*loc.file, "prog.typl");
    assert_eq!(loc.line, 2, "should point at the `(panic ...)` form on line 2");
    assert!(matches!(err.kind(), EvalError::Panic(_)));
}
