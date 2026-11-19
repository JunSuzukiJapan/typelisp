//! Tests for the built-in `HashTable<K,V>` (CRUD via instance/static methods)
//! and the generic-receiver `defmethod`/assoc-call dispatch it depends on
//! (see `typeck::method`'s `AssocCall`/`subst` handling).

extern crate typelisp;
mod common;
use common::{run_with_prelude_under_gc_stress, sexpr_to_string, type_error};
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

/// Read, type-check, and evaluate a program; return the last expression's
/// value alongside the heap (so `Sexpr` results can be inspected).
///
/// Loads the prelude, because `get`/`set`/`remove` **are** prelude
/// `defmethod`s: looking a key up means hashing it and comparing it, and both
/// are the key type's own `sxhash`/`equals`, so no builtin could do it. Their
/// `(:where (Hash K))` bound is what turned "unsupported key type" from a
/// runtime panic into a type error — and then into no restriction at all, a
/// `defstruct` included. Every real program loads the prelude anyway; the one
/// thing that deliberately does not is the compiler island, which never holds
/// a `HashTable` value of its own (it only *emits* the `rt_hashtable_*`
/// calls).
fn run(src: &str) -> Result<(Value, Heap), EvalError> {
    run_with_capacity(src, 1 << 16)
}

fn run_with_capacity(src: &str, capacity: usize) -> Result<(Value, Heap), EvalError> {
    let mut h = Heap::with_capacity(capacity);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok((last, h))
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed").0
}

/// The text of a `string` result. A `string` is a heap `Value::Str` since the
/// scalar unification, so reading one needs the heap — which [`run`] already
/// hands back.
fn eval_string(src: &str) -> String {
    let (v, h) = run(src).expect("eval failed");
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

// ---- construction / generic static dispatch ---------------------------------

#[test]
fn new_infers_type_args_from_return_type() {
    // `HashTable::new` has no argument to infer `K`/`V` from — it must come
    // from the call site's expected type (here, `make_h`'s declared return
    // type), the same way a field-less `None` learns its type argument.
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (count (make-h))";
    assert_eq!(eval_ok(src), Value::Int(0));
}

// ---- CRUD ---------------------------------------------------------------------

#[test]
fn set_then_get_returns_some() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (match (get h 1) ((Some v) v) ((None) \"missing\"))))
               (f)";
    assert_eq!(eval_string(src), "a");
}

#[test]
fn get_missing_key_returns_none() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (match (get h 1) ((Some v) v) ((None) \"missing\"))))
               (f)";
    assert_eq!(eval_string(src), "missing");
}

#[test]
fn set_overwrites_existing_key() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (set h 1 \"b\")
                   (match (get h 1) ((Some v) v) ((None) \"missing\"))))
               (f)";
    assert_eq!(eval_string(src), "b");
}

#[test]
fn count_tracks_distinct_keys() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () int
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (set h 2 \"b\")
                   (set h 1 \"c\")
                   (count h)))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn remove_returns_removed_value_and_drops_key() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (let ((removed (match (remove h 1) ((Some v) v) ((None) \"missing\"))))
                     (match (get h 1)
                       ((Some v) v)
                       ((None) removed)))))
               (f)";
    // After removing key 1, `get h 1` is None, so the result is whatever
    // `remove` returned (proving it returned the removed value, "a", not
    // just dropping the key).
    assert_eq!(eval_string(src), "a");
}

#[test]
fn remove_missing_key_returns_none() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (match (remove h 1) ((Some v) v) ((None) \"missing\"))))
               (f)";
    assert_eq!(eval_string(src), "missing");
}

#[test]
fn clear_empties_the_table() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () int
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (set h 2 \"b\")
                   (clear h)
                   (count h)))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(0));
}

// ---- key types ------------------------------------------------------------

#[test]
fn string_keys_work() {
    let src = "(defun make-h () HashTable<string,int> (HashTable::new :test 'equal))
               (defun f () int
                 (let ((h (make-h)))
                   (set h \"x\" 42)
                   (match (get h \"x\") ((Some v) v) ((None) 0))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

// ---- static typing --------------------------------------------------------

#[test]
fn wrong_key_type_is_a_type_error() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (set h true \"a\")
                   \"x\"))";
    type_error(src);
}

#[test]
fn instance_method_dispatch_substitutes_k_and_v_independently() {
    // Two differently-instantiated HashTables must not cross-contaminate each
    // other's inferred `K`/`V` (regression for `check_assoc_call`'s `subst`
    // being built fresh per call, from the receiver's own concrete type args).
    let src = "(defun make-ints () HashTable<int,int> (HashTable::new))
               (defun make-strs () HashTable<string,string> (HashTable::new :test 'equal))
               (defun f () int
                 (let ((a (make-ints)) (b (make-strs)))
                   (set a 1 100)
                   (set b \"k\" \"v\")
                   (match (get a 1) ((Some v) v) ((None) 0))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(100));
}

// ---- traversal (keys/values/entries) ---------------------------------------

#[test]
fn keys_returns_a_vector_with_one_entry_per_distinct_key() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () int
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (set h 2 \"b\")
                   (len (keys h))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn values_returns_a_vector_with_one_entry_per_distinct_key() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () int
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (set h 2 \"b\")
                   (len (values h))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn entries_returns_a_vector_with_one_entry_per_distinct_key() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () int
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (set h 2 \"b\")
                   (set h 1 \"c\")
                   (len (entries h))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn keys_values_entries_on_an_empty_table_are_empty() {
    let src = "(defun make-h () HashTable<int,string> (HashTable::new))
               (defun f () int
                 (let ((h (make-h)))
                   (+ (len (keys h)) (+ (len (values h)) (len (entries h))))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(0));
}

// ---- GC pressure ------------------------------------------------------------

#[test]
fn sexpr_values_survive_gc_pressure() {
    // A `HashTable<int,Sexpr>` holds `RtValue::Sexpr` values — these are
    // cons-heap pointers that must stay rooted via `collect_sexpr_roots`'s new
    // `RtValue::HashTable` case (see `Interp::sync_roots`), or a GC triggered
    // while building a later quoted list (via `Heap::cons`, the same allocator
    // the cons heap and `HashTable`'s own storage are unrelated to) could
    // reclaim them out from under the table. Key 0 is set once, then key 1 is
    // overwritten 500 times purely to churn the tiny heap and force repeated
    // GCs; if key 0's cells aren't rooted they can be swept and recycled into
    // one of those later allocations, corrupting (not just losing) the read
    // back at the end.
    // The value type is `Option<Sexpr>` (S-expression data), which makes
    // `(get h 0)` an `Option<Option<Sexpr>>` — the very nesting the empty
    // list's niche has to keep distinct, reached here for real rather than
    // constructed for a test (`tests/compile_test.rs`'s
    // `a_nested_option_over_sexpr_keeps_its_two_nones_apart`).
    let src = "(defun make-h () HashTable<int,Option<Sexpr>> (HashTable::new))
               (defun f () Option<Sexpr>
                 (let ((h (make-h)))
                   (set h 0 (quote (a b c d e)))
                   (dotimes (i 500)
                     (set h 1 (quote (x y z))))
                   (match (get h 0) ((Some v) v) ((None) (quote boom)))))
               (f)";
    let (v, h) = run_with_prelude_under_gc_stress(src).expect("eval failed");
    assert_eq!(sexpr_to_string(&h, v), "(a b c d e)");
}

// ---- :test ------------------------------------------------------------------

/// CL's `make-hash-table`: `eql` without `:test`, so two strings built apart
/// are two keys; `equal` compares the contents, `equalp` without case, and
/// `eq` compares a struct by identity.
#[test]
fn test_chooses_how_keys_compare() {
    let src = "(defstruct pt (x int))
        (let ((a (the HashTable<string,int> (HashTable::new)))
              (b (the HashTable<string,int> (HashTable::new :test 'equal)))
              (c (the HashTable<string,int> (HashTable::new :test equalp)))
              (d (the HashTable<pt,int> (HashTable::new :test 'eq)))
              (p (make-instance 'pt :x 1)))
          (set a \"k\" 1) (set b \"k\" 2) (set c \"K\" 3) (set d p 4)
          (format false \"~a ~a ~a ~a ~a\"
            (get a (append \"k\" \"\")) (get b (append \"k\" \"\")) (get c \"k\") (get d p)
            (get d (make-instance 'pt :x 1))))";
    assert_eq!(eval_string(src), "None (Some 2) (Some 3) (Some 4) None");
}

/// A set takes the same `:test`.
#[test]
fn a_set_takes_the_same_test() {
    let src = "(use std::collections::HashSet)
        (let ((s (the HashSet<string> (HashSet::new :test 'equal))) (e (the HashSet<string> (HashSet::new))))
          (insert s \"x\") (insert e \"x\")
          (format false \"~a ~a\" (contains s (append \"x\" \"\")) (contains e (append \"x\" \"\"))))";
    assert_eq!(eval_string(src), "true false");
}

/// `:test` names one of CL's four, and is the only argument. (An `equal`
/// table's key type without `Hash` is `hash_trait_test`'s.)
#[test]
fn test_names_one_of_the_four() {
    type_error("(the HashTable<int,int> (HashTable::new :test 'foo))");
    type_error("(the HashTable<int,int> (HashTable::new 1))");
}
