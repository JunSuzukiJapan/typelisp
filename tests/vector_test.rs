//! Tests for the built-in `Vector<T>` (CRUD via instance/static methods),
//! reusing the generic-receiver assoc-call dispatch built for `HashTable<K,V>`
//! (see `Checker::check_assoc_call`'s `AssocCall`/`subst` handling, and
//! `tests/hashtable_test.rs` for the sibling test suite this one mirrors).

extern crate typelisp;
use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, RtValue, Value};

/// Read, type-check, and evaluate a program; return the last expression's
/// value alongside the heap (so `Sexpr` results can be inspected).
fn run(src: &str) -> Result<(RtValue, Heap), EvalError> {
    run_with_capacity(src, 1 << 16)
}

fn run_with_capacity(src: &str, capacity: usize) -> Result<(RtValue, Heap), EvalError> {
    let mut h = Heap::with_capacity(capacity);
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
    Ok((last, h))
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed").0
}

/// Render a heap-backed `Sexpr` value in reader syntax, for exact-content
/// assertions (see `sexpr_values_survive_gc_pressure`).
fn sexpr_to_string(heap: &Heap, v: Value) -> String {
    match v {
        Value::Empty => "()".to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => format!("#\\{}", c),
        Value::Symbol(id) => heap.symbol_name(id).to_string(),
        Value::Str(id) => format!("{:?}", heap.string(id)),
        Value::Path(_) => "<path>".to_string(),
        Value::Cons(_) => {
            let mut parts = Vec::new();
            let mut cur = v;
            loop {
                match cur {
                    Value::Cons(_) => {
                        parts.push(sexpr_to_string(heap, heap.car(cur).unwrap()));
                        cur = heap.cdr(cur).unwrap();
                    }
                    Value::Empty => return format!("({})", parts.join(" ")),
                    other => return format!("({} . {})", parts.join(" "), sexpr_to_string(heap, other)),
                }
            }
        }
    }
}

fn type_error(src: &str) {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut result: Result<_, Error> = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    assert!(result.is_err(), "expected a type error");
}

// ---- construction ------------------------------------------------------------

#[test]
fn new_fills_with_the_given_initial_value() {
    let src = "(defun make-v () Vector<i32> (Vector::new 3 0))
               (defun f () i32
                 (let ((v (make-v)))
                   (+ (get v 0) (+ (get v 1) (get v 2)))))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(0));
}

// ---- CRUD ---------------------------------------------------------------------

#[test]
fn get_returns_the_value_at_an_index() {
    let src = "(defun make-v () Vector<i32> (Vector::new 3 7))
               (get (make-v) 1)";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn set_overwrites_the_value_at_an_index() {
    let src = "(defun make-v () Vector<i32> (Vector::new 3 0))
               (defun f () i32
                 (let ((v (make-v)))
                   (set v 1 42)
                   (get v 1)))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

#[test]
fn length_tracks_the_element_count() {
    let src = "(defun make-v () Vector<i32> (Vector::new 5 0))
               (length (make-v))";
    assert_eq!(eval_ok(src), RtValue::Int(5));
}

#[test]
fn push_appends_and_increases_length() {
    let src = "(defun make-v () Vector<i32> (Vector::new 0 0))
               (defun f () i32
                 (let ((v (make-v)))
                   (push v 1)
                   (push v 2)
                   (length v)))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn push_then_get_sees_the_pushed_value() {
    let src = "(defun make-v () Vector<i32> (Vector::new 0 0))
               (defun f () i32
                 (let ((v (make-v)))
                   (push v 9)
                   (get v 0)))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(9));
}

#[test]
fn pop_returns_the_last_value_and_shrinks() {
    let src = "(defun make-v () Vector<i32> (Vector::new 0 0))
               (defun f () i32
                 (let ((v (make-v)))
                   (push v 1)
                   (push v 2)
                   (let ((popped (match (pop v) ((Some x) x) ((None) -1))))
                     (+ popped (* 10 (length v))))))
               (f)";
    // popped = 2, length after pop = 1 -> 2 + 10*1 = 12
    assert_eq!(eval_ok(src), RtValue::Int(12));
}

#[test]
fn pop_on_an_empty_vector_returns_none() {
    let src = "(defun make-v () Vector<i32> (Vector::new 0 0))
               (defun f () i32
                 (let ((v (make-v)))
                   (match (pop v) ((Some x) x) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(-1));
}

// ---- out-of-range access (runtime panic) ---------------------------------------

#[test]
fn get_out_of_range_panics() {
    let src = "(defun make-v () Vector<i32> (Vector::new 2 0))
               (get (make-v) 5)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn set_out_of_range_panics() {
    let src = "(defun make-v () Vector<i32> (Vector::new 2 0))
               (defun f () () (set (make-v) 5 1))
               (f)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn get_negative_index_panics() {
    let src = "(defun make-v () Vector<i32> (Vector::new 2 0))
               (get (make-v) -1)";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

// ---- element types ----------------------------------------------------------

#[test]
fn string_elements_work() {
    let src = "(defun make-v () Vector<string> (Vector::new 1 \"a\"))
               (defun f () string
                 (let ((v (make-v)))
                   (set v 0 \"hi\")
                   (get v 0)))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Str("hi".into()));
}

// ---- static typing --------------------------------------------------------

#[test]
fn wrong_element_type_is_a_type_error() {
    let src = "(defun make-v () Vector<i32> (Vector::new 2 0))
               (defun f () () (set (make-v) 0 true))";
    type_error(src);
}

#[test]
fn instance_method_dispatch_substitutes_t_independently() {
    // Two differently-instantiated Vectors must not cross-contaminate each
    // other's inferred `T` (regression for `check_assoc_call`'s `subst` being
    // built fresh per call, from the receiver's own concrete type args).
    let src = "(defun make-ints () Vector<i32> (Vector::new 1 0))
               (defun make-strs () Vector<string> (Vector::new 1 \"\"))
               (defun f () i32
                 (let ((a (make-ints)) (b (make-strs)))
                   (set a 0 100)
                   (set b 0 \"k\")
                   (get a 0)))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(100));
}

// ---- GC pressure ------------------------------------------------------------

#[test]
fn sexpr_values_survive_gc_pressure() {
    // A `Vector<Sexpr>` holds `RtValue::Sexpr` values — these are cons-heap
    // pointers that must stay rooted via `collect_sexpr_roots`'s
    // `RtValue::Vector` case (see `Interp::sync_roots`), or a GC triggered
    // while building a later quoted list (via `Heap::cons`) could reclaim
    // them out from under the vector. Index 0 is set once, then index 1 is
    // overwritten 500 times purely to churn the tiny heap and force repeated
    // GCs; if index 0's cells aren't rooted they can be swept and recycled
    // into one of those later allocations, corrupting the read back at the
    // end.
    let src = "(defun make-v () Vector<Sexpr> (Vector::new 2 (quote ())))
               (defun f () Sexpr
                 (let ((v (make-v)))
                   (set v 0 (quote (a b c d e)))
                   (dotimes (i 500)
                     (set v 1 (quote (x y z))))
                   (get v 0)))
               (f)";
    let (v, h) = run_with_capacity(src, 96).expect("eval failed");
    match v {
        RtValue::Sexpr(sv) => assert_eq!(sexpr_to_string(&h, sv), "(a b c d e)"),
        other => panic!("expected a Sexpr value, got {:?}", other),
    }
}
