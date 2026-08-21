//! Tests for the built-in `HashTable<K,V>` (CRUD via instance/static methods)
//! and the generic-receiver `defmethod`/assoc-call dispatch it depends on
//! (see `Checker::check_assoc_call`'s `AssocCall`/`subst` handling).

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

/// Read, type-check, and evaluate a program; return the last expression's
/// value alongside the heap (so `Sexpr` results can be inspected).
///
/// Loads the prelude, which the key methods now require: `get`/`set`/`remove`
/// carry a `(where (Hash K))` bound (`registry::hashtable_def`), and `Hash` is
/// a prelude trait. That bound is what turned "unsupported key type" from a
/// runtime panic into a type error, so depending on the prelude for it is the
/// trade — and every real program loads the prelude anyway. The one thing that
/// deliberately does not is the compiler island, which never holds a
/// `HashTable` value of its own (it only *emits* the `rt_hashtable_*` calls).
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

/// Runs `src` with the prelude loaded and **a collection before every
/// allocation** (`gc_stress`), returning the heap alongside the value so a
/// `Sexpr` result can be read out of it.
///
/// There is no `capacity` parameter any more, and that absence is the point.
/// These tests used to check against a roomy heap and then execute
/// against a 96-cell one so the churn forced repeated collections; that is not
/// expressible any more. Since the checker lowers code into cons cells, the
/// checked program *is* cells in the heap it was checked against, and handing it
/// to a second heap reads those cells through the wrong symbol/string tables
/// (`sym_names` is empty there). A 96-cell heap could not hold the prelude, let
/// alone the program. Stressing one heap is both the honest shape and strictly
/// stronger pressure — a collection before *every* allocation, not only before
/// the ones a small arena happens to block on. See `eval_test.rs`'s
/// `eval_under_gc_pressure`, which this mirrors.
///
/// Stress goes on *after* the prelude loads: it is large, and collecting through
/// it would dominate the runtime without testing anything these cases are about.
fn run_with_prelude_under_gc_stress(src: &str) -> Result<(Value, Heap), EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    h.set_gc_stress(true);
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

/// Render a heap-backed `Sexpr` value in reader syntax, for exact-content
/// assertions (see `sexpr_values_survive_gc_pressure`).
fn sexpr_to_string(heap: &Heap, v: Value) -> String {
    match v {
        Value::Empty => "()".to_string(),
        Value::Int(i) => i.to_string(),
        Value::Boxed(id) => heap.float_value(id).to_string(),
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

// ---- construction / generic static dispatch ---------------------------------

#[test]
fn new_infers_type_args_from_return_type() {
    // `HashTable::new` has no argument to infer `K`/`V` from — it must come
    // from the call site's expected type (here, `make_h`'s declared return
    // type), the same way a field-less `None` learns its type argument.
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (count (make-h))";
    assert_eq!(eval_ok(src), Value::Int(0));
}

// ---- CRUD ---------------------------------------------------------------------

#[test]
fn set_then_get_returns_some() {
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (match (get h 1) ((Some v) v) ((None) \"missing\"))))
               (f)";
    assert_eq!(eval_string(src), "a");
}

#[test]
fn get_missing_key_returns_none() {
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (match (get h 1) ((Some v) v) ((None) \"missing\"))))
               (f)";
    assert_eq!(eval_string(src), "missing");
}

#[test]
fn set_overwrites_existing_key() {
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
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
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () i32
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
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
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
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () string
                 (let ((h (make-h)))
                   (match (remove h 1) ((Some v) v) ((None) \"missing\"))))
               (f)";
    assert_eq!(eval_string(src), "missing");
}

#[test]
fn clear_empties_the_table() {
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () i32
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
    let src = "(defun make-h () HashTable<string,i32> (HashTable::new))
               (defun f () i32
                 (let ((h (make-h)))
                   (set h \"x\" 42)
                   (match (get h \"x\") ((Some v) v) ((None) 0))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

// ---- static typing --------------------------------------------------------

#[test]
fn wrong_key_type_is_a_type_error() {
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
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
    let src = "(defun make-ints () HashTable<i32,i32> (HashTable::new))
               (defun make-strs () HashTable<string,string> (HashTable::new))
               (defun f () i32
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
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () i32
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (set h 2 \"b\")
                   (len (keys h))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn values_returns_a_vector_with_one_entry_per_distinct_key() {
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () i32
                 (let ((h (make-h)))
                   (set h 1 \"a\")
                   (set h 2 \"b\")
                   (len (values h))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn entries_returns_a_vector_with_one_entry_per_distinct_key() {
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () i32
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
    let src = "(defun make-h () HashTable<i32,string> (HashTable::new))
               (defun f () i32
                 (let ((h (make-h)))
                   (+ (len (keys h)) (+ (len (values h)) (len (entries h))))))
               (f)";
    assert_eq!(eval_ok(src), Value::Int(0));
}

// ---- GC pressure ------------------------------------------------------------

#[test]
fn sexpr_values_survive_gc_pressure() {
    // A `HashTable<i32,Sexpr>` holds `RtValue::Sexpr` values — these are
    // cons-heap pointers that must stay rooted via `collect_sexpr_roots`'s new
    // `RtValue::HashTable` case (see `Interp::sync_roots`), or a GC triggered
    // while building a later quoted list (via `Heap::cons`, the same allocator
    // the cons heap and `HashTable`'s own storage are unrelated to) could
    // reclaim them out from under the table. Key 0 is set once, then key 1 is
    // overwritten 500 times purely to churn the tiny heap and force repeated
    // GCs; if key 0's cells aren't rooted they can be swept and recycled into
    // one of those later allocations, corrupting (not just losing) the read
    // back at the end.
    let src = "(defun make-h () HashTable<i32,Sexpr> (HashTable::new))
               (defun f () Sexpr
                 (let ((h (make-h)))
                   (set h 0 (quote (a b c d e)))
                   (dotimes (i 500)
                     (set h 1 (quote (x y z))))
                   (match (get h 0) ((Some v) v) ((None) (quote boom)))))
               (f)";
    let (v, h) = run_with_prelude_under_gc_stress(src).expect("eval failed");
    match v {
        sv => assert_eq!(sexpr_to_string(&h, sv), "(a b c d e)"),
    }
}

