//! Tests for the Phase 6.5 sequence library (`docs/dev/symbol-sexpr-redesign.md`)
//! — the typed rebuild of the `Sexpr` list operations removed in Phase 5, as
//! generic `Iter` `defun`s (`length`/`append`/`nth`/`elt`/`take`/`subseq`/
//! `last`/`butlast`/`every`/`any`) plus the `Eq`-bounded pair
//! (`member`: `(where (Eq A))`, `assoc`: `(where (Iter I (Item
//! cons-cell<K,V>)) (Eq K))`) and `sort`, which — matching CL's own
//! `(sort sequence predicate)` — takes an explicit comparator instead of an
//! `Ord` bound. Also covers the `Eq`/`Ord` traits themselves: scalar impls,
//! user-type impls, and the rejection of unimplemented element types at
//! check time.

extern crate typelisp;

mod common;
use common::{eval_string_compiled as eval_string, Load, Session};
use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, TopLevelForm, Value};

fn check(src: &str) -> Result<TopLevelForm, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

fn run(src: &str) -> Result<Value, EvalError> {
    Session::new(Load::Compiler).eval(src)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}


/// A `(1 2 3)`-valued `Vector<i32>` builder (no variadic `vector-of` builder
/// exists — the language has no value-level `&rest`).
const V123: &str = "(defun make-v () Vector<i32> (Vector::new))
                    (defvar (v Vector<i32>) (make-v))
                    (push v 1) (push v 2) (push v 3)";

/// An empty `Vector<i32>`.
const VEMPTY: &str = "(defun make-v () Vector<i32> (Vector::new))
                      (defvar (v Vector<i32>) (make-v))";

// ---- length -----------------------------------------------------------------

#[test]
fn length_counts_the_elements() {
    let src = format!("{V123} (length (iter v))");
    assert_eq!(eval_ok(&src), Value::Int(3));
}

#[test]
fn length_of_an_empty_iterator_is_zero() {
    let src = format!("{VEMPTY} (length (iter v))");
    assert_eq!(eval_ok(&src), Value::Int(0));
}

#[test]
fn length_on_a_string_still_resolves_to_the_builtin_method() {
    // Instance-method dispatch by receiver type must win over the new free
    // generic `length` — the self-hosted compiler island depends on it.
    assert_eq!(eval_ok("(length \"abc\")"), Value::Int(3));
}

// ---- append -----------------------------------------------------------------
// (Vector-over-Vector concatenation and non-mutation are covered in
// `vector_ops_test.rs`; here: mixed sources and the string-method regression.)

#[test]
fn append_rejects_mismatched_element_types() {
    let src = "(defun make-i () Vector<i32> (Vector::new))
               (defun make-s () Vector<string> (Vector::new))
               (append (iter (make-i)) (iter (make-s)))";
    assert!(check(src).is_err());
}

#[test]
fn append_on_strings_still_resolves_to_the_builtin_method() {
    let got = eval_string("(append \"ab\" \"cd\")");
    assert_eq!(got, "abcd");
}

// ---- nth / elt --------------------------------------------------------------

#[test]
fn nth_returns_the_element_at_the_index() {
    // CL argument order: index first.
    let src = format!("{V123} (unwrap (nth 1 (iter v)))");
    assert_eq!(eval_ok(&src), Value::Int(2));
}

#[test]
fn nth_out_of_range_and_negative_are_none() {
    let src = format!("{V123} (+ (unwrap-or (nth 9 (iter v)) -1) (unwrap-or (nth -1 (iter v)) -10))");
    assert_eq!(eval_ok(&src), Value::Int(-11));
}

#[test]
fn elt_takes_the_sequence_first() {
    // CL argument order: sequence first (the mirror of `nth`).
    let src = format!("{V123} (unwrap (elt (iter v) 2))");
    assert_eq!(eval_ok(&src), Value::Int(3));
}

// ---- take / subseq ----------------------------------------------------------

#[test]
fn take_returns_the_first_n_elements() {
    let src = format!("{V123} (let ((out (take (iter v) 2))) (+ (* (len out) 100) (+ (get out 0) (get out 1))))");
    // len 2 -> 200, 1+2 = 3 -> 203
    assert_eq!(eval_ok(&src), Value::Int(203));
}

#[test]
fn take_zero_is_empty_and_over_length_takes_everything() {
    let src = format!("{V123} (+ (* (len (take (iter v) 0)) 10) (len (take (iter v) 9)))");
    assert_eq!(eval_ok(&src), Value::Int(3));
}

#[test]
fn subseq_extracts_the_half_open_range() {
    let src = format!("{V123} (let ((out (subseq (iter v) 1 3))) (+ (* (len out) 100) (+ (get out 0) (get out 1))))");
    // [1,3) of (1 2 3) -> (2 3): len 2 -> 200, 2+3 = 5 -> 205
    assert_eq!(eval_ok(&src), Value::Int(205));
}

#[test]
fn subseq_start_past_the_end_is_empty_and_end_is_clamped() {
    // `end` past the input's length is clamped (more lenient than CL, which
    // signals an error).
    let src = format!("{V123} (+ (* (len (subseq (iter v) 5 8)) 10) (len (subseq (iter v) 1 99)))");
    assert_eq!(eval_ok(&src), Value::Int(2));
}

// ---- last / butlast ---------------------------------------------------------

#[test]
fn last_returns_the_final_element() {
    // The final *element*, not CL's final cons.
    let src = format!("{V123} (unwrap (last (iter v)))");
    assert_eq!(eval_ok(&src), Value::Int(3));
}

#[test]
fn last_of_an_empty_iterator_is_none() {
    let src = format!("{VEMPTY} (unwrap-or (last (iter v)) -1)");
    assert_eq!(eval_ok(&src), Value::Int(-1));
}

#[test]
fn butlast_drops_only_the_final_element() {
    let src = format!("{V123} (let ((out (butlast (iter v)))) (+ (* (len out) 100) (+ (get out 0) (get out 1))))");
    // (1 2): len 2 -> 200, 1+2 = 3 -> 203
    assert_eq!(eval_ok(&src), Value::Int(203));
}

#[test]
fn butlast_of_empty_and_singleton_is_empty() {
    let src = format!(
        "{VEMPTY} (push v 7)
         (+ (len (butlast (iter v)))
            (len (butlast (iter (the Vector<i32> (Vector::new))))))"
    );
    assert_eq!(eval_ok(&src), Value::Int(0));
}

// ---- member (Eq) ------------------------------------------------------------

#[test]
fn member_finds_and_misses_by_equality() {
    let src = format!("{V123} (if (member 2 (iter v)) (if (member 9 (iter v)) 0 1) 0)");
    assert_eq!(eval_ok(&src), Value::Int(1));
}

#[test]
fn member_on_strings_compares_content_not_identity() {
    // The needle is built at runtime (`(append "a" "b")`), so it can never be
    // `eq` (Rc-identical) to the stored "ab" — `Eq string` must delegate to
    // `equal` for this to hold.
    let src = "(defun make-v () Vector<string> (Vector::new))
               (let ((v (make-v)))
                 (push v \"ab\")
                 (member (append \"a\" \"b\") (iter v)))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn member_over_an_element_type_without_eq_is_a_type_error() {
    let src = "(defstruct point (x i32))
               (defun make-v () Vector<point> (Vector::new))
               (member (point::new 1) (iter (make-v)))";
    assert!(check(src).is_err());
}

// ---- every / any ------------------------------------------------------------

#[test]
fn every_is_true_on_empty_and_any_is_false_on_empty() {
    let src = format!(
        "{VEMPTY}
         (if (every (iter v) (lambda ((x i32)) bool false))
             (if (any (iter v) (lambda ((x i32)) bool true)) 0 1)
             0)"
    );
    assert_eq!(eval_ok(&src), Value::Int(1));
}

#[test]
fn every_and_any_report_over_real_elements() {
    let src = format!(
        "{V123}
         (+ (if (every (iter v) (lambda ((x i32)) bool (> x 0))) 10 0)
            (+ (if (every (iter v) (lambda ((x i32)) bool (> x 1))) 100 0)
               (+ (if (any (iter v) (lambda ((x i32)) bool (= x 3))) 1000 0)
                  (if (any (iter v) (lambda ((x i32)) bool (= x 9))) 10000 0))))"
    );
    assert_eq!(eval_ok(&src), Value::Int(1010));
}

// ---- sort (explicit comparator, CL's own `(sort sequence predicate)`) --------

#[test]
fn sort_orders_i32_ascending_without_mutating_the_input() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 3) (push v 1) (push v 2)
                 (let ((out (sort (iter v) (lambda ((a i32) (b i32)) bool (< a b)))))
                   (+ (* (get v 0) 1000)
                      (+ (* (get out 0) 100) (+ (* (get out 1) 10) (get out 2))))))";
    // input head still 3 -> 3000; sorted (1 2 3) -> 123
    assert_eq!(eval_ok(src), Value::Int(3123));
}

#[test]
fn sort_orders_i32_descending_given_a_flipped_comparator() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 3) (push v 1) (push v 2)
                 (let ((out (sort (iter v) (lambda ((a i32) (b i32)) bool (> a b)))))
                   (+ (* (get out 0) 100) (+ (* (get out 1) 10) (get out 2)))))";
    // descending (3 2 1) -> 321
    assert_eq!(eval_ok(src), Value::Int(321));
}

#[test]
fn sort_orders_strings_lexicographically() {
    let src = "(defun make-v () Vector<string> (Vector::new))
               (let ((v (make-v)))
                 (push v \"pear\") (push v \"apple\") (push v \"fig\")
                 (get (sort (iter v) (lambda ((a string) (b string)) bool (< a b))) 0))";
    assert_eq!(eval_string(src), "apple");
}

#[test]
fn sort_of_an_empty_iterator_is_empty() {
    let src = format!("{VEMPTY} (len (sort (iter v) (lambda ((a i32) (b i32)) bool (< a b))))");
    assert_eq!(eval_ok(&src), Value::Int(0));
}

#[test]
fn sort_works_over_a_type_with_no_ord_impl_given_an_explicit_comparator() {
    // `sort` takes the comparator directly now (CL's own required
    // `predicate` argument), so a type needs no `Ord` impl at all — `bool`
    // gets `Eq` but deliberately no `Ord`.
    let src = "(defun make-v () Vector<bool> (Vector::new))
               (let ((v (make-v)))
                 (push v true) (push v false)
                 (get (sort (iter v) (lambda ((a bool) (b bool)) bool (if a false b))) 0))";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

#[test]
fn sort_is_stable_for_equal_keys() {
    // `rec`s ordered by `k` only; the two `k`=1 records must keep their input
    // order (`tag` 10 before 20) because the insertion shift uses a strict
    // comparator.
    let src = "(defstruct rec (k i32) (tag i32))
               (defun make-v () Vector<rec> (Vector::new))
               (let ((v (make-v)))
                 (push v (rec::new 2 99))
                 (push v (rec::new 1 10))
                 (push v (rec::new 1 20))
                 (let ((out (sort (iter v) (lambda ((a rec) (b rec)) bool (< a::k b::k)))))
                   (let ((first (get out 0)) (second (get out 1)))
                     (+ (* first::tag 100) second::tag))))";
    assert_eq!(eval_ok(src), Value::Int(1020));
}

// ---- user-type Eq -----------------------------------------------------------

#[test]
fn member_works_over_a_user_type_with_an_eq_impl() {
    let src = "(defstruct point (x i32) (y i32))
               (impl Eq point
                 (equals ((self Self) (other Self)) bool
                   (if (= self::x other::x) (= self::y other::y) false)))
               (defun make-v () Vector<point> (Vector::new))
               (let ((v (make-v)))
                 (push v (point::new 1 2))
                 (push v (point::new 3 4))
                 (if (member (point::new 3 4) (iter v))
                     (if (member (point::new 3 5) (iter v)) 0 1)
                     0))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

// ---- assoc ------------------------------------------------------------------

#[test]
fn assoc_finds_the_pair_in_an_alist_and_projects_with_cdr() {
    let src = "(defun make-alist () Vector<cons-cell<string,i32>> (Vector::new))
               (let ((al (make-alist)))
                 (push al (cons \"one\" 1))
                 (push al (cons \"two\" 2))
                 (cdr (unwrap (assoc \"two\" (iter al)))))";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn assoc_misses_with_none() {
    let src = "(defun make-alist () Vector<cons-cell<string,i32>> (Vector::new))
               (let ((al (make-alist)))
                 (push al (cons \"one\" 1))
                 (is-none (assoc \"nope\" (iter al))))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn assoc_works_over_a_hashtable_iterator() {
    // `HashTable<K,V>`'s iterator `Item` is exactly `cons-cell<K,V>`, so the
    // same `assoc` serves it — the composite associated-type pin
    // `(Item cons-cell<K,V>)` in action.
    let src = "(defun make-h () HashTable<string,i32> (HashTable::new))
               (let ((h (make-h)))
                 (set h \"a\" 10)
                 (set h \"b\" 20)
                 (cdr (unwrap (assoc \"b\" (iter h)))))";
    assert_eq!(eval_ok(src), Value::Int(20));
}

// ---- cons-cell Eq/Ord (recursive impls with where clauses) -------------------

#[test]
fn pairs_compare_structurally_with_equals() {
    let src = "(+ (if (equals (cons 1 2) (cons 1 2)) 1 0)
                  (if (equals (cons 1 2) (cons 1 3)) 10 0))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn mixed_type_pairs_compare_fieldwise() {
    let src = "(if (equals (cons \"a\" 1) (cons \"a\" 1))
                   (if (equals (cons \"a\" 1) (cons \"b\" 1)) 0 1)
                   0)";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn nested_pairs_recurse_through_the_impl() {
    let src = "(+ (if (equals (cons (cons 1 2) 3) (cons (cons 1 2) 3)) 1 0)
                  (if (equals (cons (cons 1 2) 3) (cons (cons 1 9) 3)) 10 0))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn pairs_order_lexicographically_with_less() {
    // car decides first (1 < 2 even though 9 > 0); equal cars fall through
    // to cdr.
    let src = "(+ (if (less (cons 1 9) (cons 2 0)) 1 0)
                  (+ (if (less (cons 1 2) (cons 1 3)) 10 0)
                     (if (less (cons 1 3) (cons 1 3)) 100 0)))";
    assert_eq!(eval_ok(src), Value::Int(11));
}

#[test]
fn member_and_sort_work_over_a_vector_of_pairs() {
    let src = "(defun make-v () Vector<cons-cell<i32,i32>> (Vector::new))
               (let ((v (make-v)))
                 (push v (cons 2 0))
                 (push v (cons 1 5))
                 (push v (cons 1 3))
                 (let ((sorted (sort (iter v) (lambda ((a cons-cell<i32,i32>) (b cons-cell<i32,i32>)) bool (less a b)))))
                   (let ((first (get sorted 0)))
                     (+ (if (member (cons 1 5) (iter v)) 1000 0)
                        (+ (if (member (cons 9 9) (iter v)) 100 0)
                           (+ (* first::car 10) first::cdr))))))";
    // member hit -> 1000, miss -> 0, sorted head (1 . 3) -> 13
    assert_eq!(eval_ok(src), Value::Int(1013));
}

#[test]
fn pair_equals_rejects_an_element_type_without_eq() {
    // `point` has no `Eq` impl, so `cons-cell<point,i32>`'s bounded `equals`
    // must fail at the call site with a real trait error.
    let src = "(defstruct point (x i32))
               (equals (cons (point::new 1) 2) (cons (point::new 1) 2))";
    let err = check(src).expect_err("must fail to check");
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("does not implement trait"),
        "expected a trait-bound error, got: {}",
        msg
    );
}

// ---- monomorphization -------------------------------------------------------

#[test]
fn member_specializes_at_two_element_types_in_one_program() {
    let src = "(defun make-i () Vector<i32> (Vector::new))
               (defun make-s () Vector<string> (Vector::new))
               (let ((vi (make-i)) (vs (make-s)))
                 (push vi 7)
                 (push vs \"x\")
                 (if (member 7 (iter vi)) (if (member \"x\" (iter vs)) 1 0) 0))";
    assert_eq!(eval_ok(src), Value::Int(1));
}
