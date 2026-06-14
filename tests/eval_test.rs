//! Tests for the tree-walking interpreter over the typed AST (step 4a).

extern crate typelisp;
use typelisp::{Checker, EvalError, Heap, Interp, Reader, RtValue};

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

#[test]
fn cond_with_classify() {
    let src = "(defun classify ((n i32)) i32 \
                 (cond ((< n 0) (- 0 1)) ((= n 0) 0) (else 1))) \
               (classify 7)";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}
