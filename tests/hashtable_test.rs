//! Tests for the built-in `HashTable<K,V>` (CRUD via instance/static methods)
//! and the generic-receiver `defmethod`/assoc-call dispatch it depends on
//! (see `Checker::check_assoc_call`'s `AssocCall`/`subst` handling).

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

/// Read, type-check, and evaluate a program; return the last expression's
/// value alongside the heap (so `Sexpr` results can be inspected).
fn run(src: &str) -> Result<(Value, Heap), EvalError> {
    run_with_capacity(src, 1 << 16)
}

fn run_with_capacity(src: &str, capacity: usize) -> Result<(Value, Heap), EvalError> {
    let mut h = Heap::with_capacity(capacity);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
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

/// Like [`run_with_capacity`], but checks against a separate, generously
/// sized heap with the prelude loaded — needed for `dotimes` (a `defmacro`
/// in `src/prelude.rs`, expanded during checking, not a checker-native
/// special form) — while still *executing* against a heap of exactly
/// `capacity` cells, so a small `capacity` still stresses the GC the way
/// `run_with_capacity` alone would. The execution phase reuses `check_interp`
/// itself (its `Interp.fns` already has the prelude's `defun`s registered,
/// `not` included — `not` moved from a Rust builtin to a plain prelude
/// `defun` in the `loop`/`break`/`return`/`setf` stage, so unlike
/// `loop`/`if`/`break`/`setf`/`set`/`quote`/... a fresh, prelude-less
/// `Interp` can no longer evaluate `while`'s expansion) rather than a
/// second, fresh `Interp::new()` — `Interp` carries no heap state of its
/// own (every `exec` call takes one as an explicit argument), so this
/// doesn't tie the GC-pressure heap to `check_heap`'s large one (see
/// `eval_test.rs`'s `runtime_cons_cells_survive_gc_when_rooted` for the
/// same fix).
///
/// All but the *last* form are also `exec`'d (cloned first) against
/// `check_interp`/`check_heap` as they're checked, not just collected —
/// `src` may define its own helper `defun`s/`defmacro`s used by a later
/// form, and checking that later form needs the earlier one already
/// registered, the same per-form check-then-exec interleaving
/// `crate::prelude::load` itself uses.
fn run_with_capacity_and_prelude(src: &str, capacity: usize) -> Result<(Value, Heap), EvalError> {
    let mut check_heap = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut check_interp = Interp::new();
    load_prelude(&mut check_heap, &mut chk, &mut check_interp);
    let r = Reader::new();
    let vs = r.read_all(&mut check_heap, src).expect("read failed");
    let n = vs.len();
    let mut tls = Vec::with_capacity(n);
    for (i, v) in vs.into_iter().enumerate() {
        let tl = chk.check_form(&mut check_heap, &check_interp, v).expect("check failed");
        if i + 1 < n {
            check_interp.exec(&mut check_heap, tl.clone()).expect("eval failed");
        }
        tls.push(tl);
    }

    let mut h = Heap::with_capacity(capacity);
    let mut last = Value::Empty;
    for tl in tls {
        if let Some(val) = check_interp.exec(&mut h, tl)? {
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
    let (v, h) = run_with_capacity_and_prelude(src, 96).expect("eval failed");
    match v {
        sv => assert_eq!(sexpr_to_string(&h, sv), "(a b c d e)"),
    }
}

