//! Tests for the `Hash` trait, `sxhash`, and the static hashability of a
//! `HashTable<K,V>`'s key type.
//!
//! CL states `sxhash`'s contract as an implication: `(equal x y)` implies
//! `(= (sxhash x) (sxhash y))`. Here it is a trait with `Eq` as its
//! supertrait, which says the same thing in the language's own terms — and
//! lets `HashTable`'s key methods carry `(where (Hash K))`, so a key type the
//! table cannot hold is a *type error* instead of the runtime panic it used
//! to be.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
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

fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut result = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    match result.map_err(Error::into_kind) {
        Err(Error::TypeError(msg)) => msg,
        other => panic!("expected a TypeError, got {:?}", other),
    }
}

// ---- sxhash -----------------------------------------------------------------

#[test]
fn an_integers_hash_is_itself_when_non_negative() {
    assert_eq!(eval_ok("(sxhash 42)"), Value::Int(42));
}

#[test]
fn a_negative_integers_hash_is_non_negative() {
    // CL requires a non-negative fixnum; the impls mask off the sign bit.
    assert_eq!(eval_ok("(>= (sxhash -1) 0)"), Value::Bool(true));
}

#[test]
fn a_strings_hash_is_fnv_1a_over_its_code_points() {
    // The 32-bit FNV-1a of "hello" (1335831723), narrowed to the 30-bit
    // fixnum range every `sxhash` here lands in. Pinned so a change to the
    // function is a deliberate one, not a silent drift.
    assert_eq!(eval_ok("(sxhash \"hello\")"), Value::Int(262089899));
}

#[test]
fn equal_values_hash_equal() {
    // The contract, for each scalar the prelude implements.
    assert_eq!(eval_ok("(= (sxhash \"hi\") (sxhash \"hi\"))"), Value::Bool(true));
    assert_eq!(eval_ok("(= (sxhash #\\a) (sxhash #\\a))"), Value::Bool(true));
    assert_eq!(eval_ok("(= (sxhash true) (sxhash true))"), Value::Bool(true));
    assert_eq!(eval_ok("(= (sxhash (string->symbol \"s\")) (sxhash (string->symbol \"s\")))"), Value::Bool(true));
}

#[test]
fn different_strings_usually_hash_differently() {
    assert_eq!(eval_ok("(= (sxhash \"hi\") (sxhash \"ho\"))"), Value::Bool(false));
}

#[test]
fn a_chars_hash_is_its_code_point() {
    assert_eq!(eval_ok("(sxhash #\\a)"), Value::Int(97));
}

#[test]
fn a_user_type_can_implement_hash() {
    let src = "
        (defstruct point (x i32) (y i32))
        (impl Eq point (equals ((self Self) (other Self)) bool
          (and (= self::x other::x) (= self::y other::y))))
        (impl Hash point (sxhash ((self Self)) i32
          (logand (+ (* (as i32 self::x) 31) (as i32 self::y)) *sxhash-mask*)))
        (sxhash (point::new 1 2))
    ";
    assert_eq!(eval_ok(src), Value::Int(33));
}

#[test]
fn a_generic_function_can_require_hash() {
    let src = "
        (defun both<T> ((a T) (b T)) i32 (where (Hash T)) (+ (sxhash a) (sxhash b)))
        (both 3 4)
    ";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn hash_inherits_eq() {
    // `Hash`'s supertrait is `Eq`, so a `(where (Hash T))` bound already
    // obliges `T` to be comparable — no second bound needed.
    let src = "
        (defun same<T> ((a T) (b T)) bool (where (Hash T)) (equals a b))
        (same 3 3)
    ";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

// ---- static hashability ------------------------------------------------------

#[test]
fn a_scalar_key_is_accepted() {
    let src = "
        (defvar (h HashTable<string,i32>) (HashTable::new))
        (progn (set h \"a\" 1) (match (get h \"a\") ((some v) v) ((none) 0)))
    ";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn a_key_type_the_table_cannot_hold_is_a_type_error() {
    // This used to be a runtime panic out of `Heap::lookup_hash_key`
    // ("HashTable: unsupported key type"), whose comment said the checker
    // could not express a hashable bound. It can, and this is the result.
    let src = "
        (defstruct point (x i32) (y i32))
        (defvar (h HashTable<point,i32>) (HashTable::new))
        (set h (point::new 1 2) 3)
    ";
    let msg = check_err(src);
    assert!(msg.contains("does not implement trait"), "unexpected message: {}", msg);
}

#[test]
fn f64_is_not_a_key_type() {
    // `f64` has `Eq`, but no `Hash`: `NaN` makes it unusable as a key, which
    // is exactly what the missing impl says.
    let src = "
        (defvar (h HashTable<f64,i32>) (HashTable::new))
        (set h 1.0 3)
    ";
    let msg = check_err(src);
    assert!(msg.contains("does not implement trait"), "unexpected message: {}", msg);
}

// ---- maphash / size ------------------------------------------------------------

#[test]
fn maphash_visits_every_pair() {
    let src = "
        (defvar (h HashTable<string,i32>) (HashTable::new))
        (defvar (total i32) 0)
        (progn
          (set h \"a\" 1)
          (set h \"b\" 2)
          (maphash h (lambda ((k string) (v i32)) () (progn (setf total (+ total v)) ())))
          total)
    ";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn size_reports_the_occupancy() {
    let src = "
        (defvar (h HashTable<string,i32>) (HashTable::new))
        (progn (set h \"a\" 1) (set h \"b\" 2) (size h))
    ";
    assert_eq!(eval_ok(src), Value::Int(2));
}
