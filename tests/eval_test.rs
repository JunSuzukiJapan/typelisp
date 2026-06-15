//! Tests for the tree-walking interpreter over the typed AST (step 4a).

extern crate typelisp;
use typelisp::{Checker, EvalError, Heap, Interp, Path, Reader, RtValue};

/// Read, type-check, and evaluate a program; return the last expression's value.
fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&h, v).expect("check failed");
        if let Some(val) = interp.exec(tl)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
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
               (unwrap-or (Some 5) 0)";
    assert_eq!(eval_ok(src), RtValue::Int(5));
}

#[test]
fn unwrap_or_none() {
    let src = "(defun unwrap-or ((o Option<i32>) (d i32)) i32 \
                 (match o ((Some v) v) ((None) d))) \
               (unwrap-or (None) 9)";
    assert_eq!(eval_ok(src), RtValue::Int(9));
}

#[test]
fn result_match() {
    let prog = "(defun unwrap-i ((r Result<i32,Error>)) i32 \
                  (match r ((Ok v) v) ((Err e) (panic! \"err\")))) ";
    assert_eq!(eval_ok(&format!("{} (unwrap-i (Ok 9))", prog)), RtValue::Int(9));
    assert_eq!(
        run(&format!("{} (unwrap-i (Err (Error \"boom\")))", prog)),
        Err(EvalError::Panic("err".into()))
    );
}

// ---- methods ----------------------------------------------------------------

#[test]
fn instance_method() {
    let src = "(defstruct point (mk (x i32) (y i32))) \
               (defmethod getx ((self point)) i32 (match self ((mk a b) a))) \
               (getx (mk 7 9))";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn static_method() {
    let src = "(defstruct pair (mk (a i32) (b i32))) \
               (defmethod swap (pair (a i32) (b i32)) pair (mk b a)) \
               (match (pair::swap 1 2) ((mk a b) a))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

// ---- panic ------------------------------------------------------------------

#[test]
fn panic_propagates() {
    let src = "(defun boom () i32 (panic! \"kaboom\")) (boom)";
    assert_eq!(run(src), Err(EvalError::Panic("kaboom".into())));
}

#[test]
fn module_function_runs() {
    let src = "(module m (defun id ((x i32)) i32 x)) (m::id 42)";
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
    assert_eq!(eval_ok("(when (< 1 2) 5)"), RtValue::Unit);
    assert_eq!(eval_ok("(unless (< 2 1) 5)"), RtValue::Unit);
}

#[test]
fn and_short_circuits_to_bool() {
    assert_eq!(eval_ok("(and (< 1 2) (< 2 3))"), RtValue::Bool(true));
    assert_eq!(eval_ok("(and (< 1 2) (< 3 2))"), RtValue::Bool(false));
    assert_eq!(eval_ok("(and)"), RtValue::Bool(true));
}

#[test]
fn or_short_circuits_to_bool() {
    assert_eq!(eval_ok("(or (< 3 2) (< 1 2))"), RtValue::Bool(true));
    assert_eq!(eval_ok("(or (< 3 2) (< 4 2))"), RtValue::Bool(false));
    assert_eq!(eval_ok("(or)"), RtValue::Bool(false));
}

#[test]
fn cond_selects_first_true_clause() {
    assert_eq!(eval_ok("(cond ((< 1 2) 10) (else 20))"), RtValue::Int(10));
    assert_eq!(
        eval_ok("(cond ((< 3 2) 10) ((< 1 2) 20) (else 30))"),
        RtValue::Int(20)
    );
    assert_eq!(eval_ok("(cond ((< 3 2) 10) (else 30))"), RtValue::Int(30));
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
    assert_eq!(eval_ok(src), RtValue::Int(10)); // 0+1+2+3+4
}

#[test]
fn setf_returns_assigned_value() {
    assert_eq!(eval_ok("(let ((x 0)) (setf x 7))"), RtValue::Int(7));
}

#[test]
fn while_is_unit() {
    assert_eq!(eval_ok("(let ((i 0)) (while (< i 0) (setf i 1)))"), RtValue::Unit);
}

#[test]
fn factorial_via_loop() {
    let src = "(defun fact ((n i32)) i32 \
                 (let ((acc 1) (i 1)) \
                   (while (<= i n) (setf acc (* acc i)) (setf i (+ i 1))) \
                   acc)) \
               (fact 5)";
    assert_eq!(eval_ok(src), RtValue::Int(120));
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
    let src = "(module m (defun inc ((x i32)) i32 (+ x 1))) \
               (defun c ((f (fn (i32) i32))) i32 (f 9)) \
               (c m::inc)";
    assert_eq!(eval_ok(src), RtValue::Int(10));
}

// ---- dotimes ----------------------------------------------------------------

#[test]
fn dotimes_accumulates() {
    assert_eq!(
        eval_ok("(let ((sum 0)) (dotimes (i 5) (setf sum (+ sum i))) sum)"),
        RtValue::Int(10) // 0+1+2+3+4
    );
}

#[test]
fn dotimes_factorial() {
    let src = "(defun fact ((n i32)) i32 \
                 (let ((acc 1)) (dotimes (i n) (setf acc (* acc (+ i 1)))) acc)) \
               (fact 5)";
    assert_eq!(eval_ok(src), RtValue::Int(120));
}

#[test]
fn dotimes_is_unit() {
    assert_eq!(eval_ok("(dotimes (i 3) ())"), RtValue::Unit);
}

// ---- cons / car / cdr / list / dolist ---------------------------------------

fn sexpr_nil() -> RtValue {
    RtValue::Data { type_name: Path::root("sexpr"), variant: 0, fields: vec![] }
}

fn sexpr_int(n: i64) -> RtValue {
    RtValue::Data { type_name: Path::root("sexpr"), variant: 1, fields: vec![RtValue::Int(n)] }
}

fn sexpr_cons(car: RtValue, cdr: RtValue) -> RtValue {
    RtValue::Data { type_name: Path::root("sexpr"), variant: 7, fields: vec![car, cdr] }
}

#[test]
fn cons_car_cdr() {
    assert_eq!(eval_ok("(cons (Int 1) (Nil))"), sexpr_cons(sexpr_int(1), sexpr_nil()));
    assert_eq!(eval_ok("(car (cons (Int 1) (Nil)))"), sexpr_int(1));
    assert_eq!(eval_ok("(cdr (cons (Int 1) (Nil)))"), sexpr_nil());
}

#[test]
fn car_and_cdr_of_non_cons_panic() {
    assert_eq!(run("(car (Nil))"), Err(EvalError::Panic("car: not a cons".into())));
    assert_eq!(run("(cdr (Int 5))"), Err(EvalError::Panic("cdr: not a cons".into())));
}

#[test]
fn list_builds_cons_chain() {
    assert_eq!(
        eval_ok("(list (Int 1) (Int 2))"),
        sexpr_cons(sexpr_int(1), sexpr_cons(sexpr_int(2), sexpr_nil()))
    );
    assert_eq!(eval_ok("(list)"), sexpr_nil());
}

#[test]
fn dolist_is_unit() {
    assert_eq!(eval_ok("(dolist (x (list (Int 1))) x)"), RtValue::Unit);
}

#[test]
fn dolist_iterates_over_each_element() {
    let src = "(let ((count 0)) \
                 (dolist (x (list (Int 1) (Int 2) (Int 3))) (setf count (+ count 1))) \
                 count)";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

#[test]
fn dolist_over_empty_list_does_nothing() {
    let src = "(let ((count 0)) \
                 (dolist (x (list)) (setf count (+ count 1))) \
                 count)";
    assert_eq!(eval_ok(src), RtValue::Int(0));
}

#[test]
fn cons_as_value() {
    let src = "(defun apply2 ((f (fn (Sexpr Sexpr) Sexpr)) (a Sexpr) (b Sexpr)) Sexpr (f a b)) \
               (apply2 cons (Int 1) (Nil))";
    assert_eq!(eval_ok(src), sexpr_cons(sexpr_int(1), sexpr_nil()));
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
    assert_eq!(eval_ok("(module m (defvar g 7)) m::g"), RtValue::Int(7));
}

#[test]
fn cond_with_classify() {
    let src = "(defun classify ((n i32)) i32 \
                 (cond ((< n 0) (- 0 1)) ((= n 0) 0) (else 1))) \
               (classify 7)";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}
