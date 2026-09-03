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
    // This used to be a runtime panic out of the mem layer ("HashTable:
    // unsupported key type"), whose comment said the checker could not
    // express a hashable bound. It can, and this is the result — and `point`
    // *can* be a key, once it implements `Hash` (see the tests at the end).
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

// ---- a user-defined type as a key (cl-parity-plan.md Phase 6a's remainder) --
//
// What `Hash` was put in for. The table is stored as hash -> bucket, and
// neither hashing a key nor comparing two is something the layer below can
// do: both are the key type's own methods. So `get`/`set`/`remove` are
// prelude `defmethod`s carrying `(where (Hash K))`, written on five bucket
// primitives — and a `defstruct` that implements `Hash` is a key like any
// other.

/// The `point` from `a_key_type_the_table_cannot_hold_is_a_type_error`, with
/// the two impls that make it a key.
const POINT: &str = r#"
    (defstruct point (x i32) (y i32))
    (impl Eq point
      (equals ((self Self) (other Self)) bool
        (if (= self::x other::x) (= self::y other::y) false)))
    (impl Hash point
      (sxhash ((self Self)) i32 (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))
"#;

#[test]
fn a_user_type_that_implements_hash_is_a_key() {
    let src = format!(
        "{}
         (defvar (h HashTable<point,string>) (HashTable::new))
         (progn
           (set h (point::new 1 2) \"a\")
           (set h (point::new 3 4) \"b\")
           (match (get h (point::new 1 2)) ((some v) v) ((none) \"missing\")))",
        POINT
    );
    match eval_ok(&src) {
        Value::Str(_) => {}
        other => panic!("expected a string, got {:?}", other),
    }
    // And a key that is `equals` to a stored one — a *different* object with
    // the same fields — finds it, which is the whole point of hashing by the
    // type's own methods rather than by identity.
    let src = format!(
        "{}
         (defvar (h HashTable<point,i32>) (HashTable::new))
         (progn (set h (point::new 1 2) 7)
                (match (get h (point::new 1 2)) ((some v) v) ((none) -1)))",
        POINT
    );
    assert_eq!(eval_ok(&src), Value::Int(7));
}

#[test]
fn a_user_key_overwrites_removes_and_counts_like_a_scalar_one() {
    let src = format!(
        "{}
         (defvar (h HashTable<point,i32>) (HashTable::new))
         (progn
           (set h (point::new 1 2) 1)
           (set h (point::new 3 4) 2)
           (set h (point::new 1 2) 9)          ; overwrite, not a second entry
           (let ((after-overwrite (count h))
                 (removed (match (remove h (point::new 1 2)) ((some v) v) ((none) -1)))
                 (after-remove (count h))
                 (gone (match (remove h (point::new 1 2)) ((some v) v) ((none) -1))))
             (+ (* 1000 after-overwrite) (+ (* 100 removed) (+ (* 10 after-remove) (+ 1 gone))))))",
        POINT
    );
    // 2 entries, removed 9, 1 left, second remove answers none (-1 -> 0).
    assert_eq!(eval_ok(&src), Value::Int(2000 + 900 + 10));
}

/// Two keys that collide — the same `sxhash`, different `equals` — stay two
/// entries. The bucket is where that is decided, and a hash function is
/// allowed to collide; `Hash`'s contract only runs the other way.
#[test]
fn colliding_keys_stay_separate_entries() {
    let src = r#"
        (defstruct k (n i32))
        (impl Eq k (equals ((self Self) (other Self)) bool (= self::n other::n)))
        ;; Every key hashes to 0.
        (impl Hash k (sxhash ((self Self)) i32 0))
        (defvar (h HashTable<k,i32>) (HashTable::new))
        (progn
          (set h (k::new 1) 10)
          (set h (k::new 2) 20)
          (+ (* 100 (count h))
             (+ (match (get h (k::new 1)) ((some v) v) ((none) 0))
                (match (get h (k::new 2)) ((some v) v) ((none) 0)))))
    "#;
    assert_eq!(eval_ok(src), Value::Int(230));
}

/// A `symbol` key works too. It has always had a `Hash` impl and always
/// type-checked, and it always panicked at run time: the layer below knew
/// four scalar shapes and `Symbol` was not one of them. Nothing about symbols
/// was fixed — the layer stopped needing to know.
#[test]
fn a_symbol_key_works() {
    let src = "
        (defvar (h HashTable<symbol,i32>) (HashTable::new))
        (progn (set h 'a 1) (set h 'b 2)
               (+ (match (get h 'a) ((some v) v) ((none) 0))
                  (match (get h 'b) ((some v) v) ((none) 0))))
    ";
    assert_eq!(eval_ok(src), Value::Int(3));
}

/// The keys the table hands back are the keys that went in — `keys`/`entries`
/// snapshot the buckets, which hold the stored `Value`s themselves.
#[test]
fn keys_and_entries_see_user_keys() {
    let src = format!(
        "{}
         (defvar (h HashTable<point,i32>) (HashTable::new))
         (progn (set h (point::new 1 2) 5) (set h (point::new 3 4) 6)
                (+ (* 10 (len (keys h)))
                   (foldl (iter (values h)) (lambda ((a i32) (b i32)) i32 (+ a b)) 0)))",
        POINT
    );
    assert_eq!(eval_ok(&src), Value::Int(31));
}
