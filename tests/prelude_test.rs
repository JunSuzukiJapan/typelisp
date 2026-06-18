//! Tests for the typelisp-defined prelude
//! ([cl-equivalence-catalog.md](../docs/cl-equivalence-catalog.md) §2.1,
//! roadmap step 7a): `consp`/`null`/`atom`/`equal` (`src/prelude.rs`), plus
//! their Rust-side foundations added alongside them — `not`, and the `eq`
//! coverage gaps it closes (`sexpr`/`bool`/`i32`/`i64`/`f64`).

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

fn type_error(src: &str) {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut result: Result<_, Error> = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    assert!(result.is_err(), "expected a type error");
}

// ---- not --------------------------------------------------------------------

#[test]
fn not_negates() {
    assert_eq!(eval_ok("(not true)"), RtValue::Bool(false));
    assert_eq!(eval_ok("(not false)"), RtValue::Bool(true));
}

// ---- eq, across every type that has it -----------------------------------------

#[test]
fn eq_on_bool() {
    assert_eq!(eval_ok("(eq true true)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(eq true false)"), RtValue::Bool(false));
}

#[test]
fn eq_on_i32() {
    assert_eq!(eval_ok("(eq 1 1)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(eq 1 2)"), RtValue::Bool(false));
}

#[test]
fn eq_on_i64() {
    let src = "(defun f ((a i64) (b i64)) bool (eq a b)) (f 1 1)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn eq_on_f64() {
    let src = "(defun f ((a f64) (b f64)) bool (eq a b)) (f 1.5 1.5)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn eq_on_char() {
    assert_eq!(eval_ok(r"(eq #\a #\a)"), RtValue::Bool(true));
}

#[test]
fn eq_on_string() {
    assert_eq!(eval_ok(r#"(eq "a" "a")"#), RtValue::Bool(true));
}

#[test]
fn eq_on_sexpr_atoms_compares_by_value() {
    let src = "(eq (quote a) (quote a))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn eq_on_sexpr_cons_is_identity_not_structural() {
    // Two separately-built (quote (a)) cons cells have equal content but are
    // not the *same* cell — `eq` says false, `equal` says true (see below).
    let src = "(eq (quote (a)) (quote (a)))";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

// ---- consp / null / atom ----------------------------------------------------

#[test]
fn consp_is_true_for_a_cons() {
    assert_eq!(eval_ok("(consp (quote (a b)))"), RtValue::Bool(true));
}

#[test]
fn consp_is_false_for_nil() {
    assert_eq!(eval_ok("(consp (quote ()))"), RtValue::Bool(false));
}

#[test]
fn consp_is_false_for_an_atom() {
    assert_eq!(eval_ok("(consp (quote a))"), RtValue::Bool(false));
}

#[test]
fn null_is_true_for_nil() {
    assert_eq!(eval_ok("(null (quote ()))"), RtValue::Bool(true));
}

#[test]
fn null_is_false_for_a_cons() {
    assert_eq!(eval_ok("(null (quote (a)))"), RtValue::Bool(false));
}

#[test]
fn atom_is_true_for_nil_and_scalars() {
    assert_eq!(eval_ok("(atom (quote ()))"), RtValue::Bool(true));
    assert_eq!(eval_ok("(atom (quote a))"), RtValue::Bool(true));
}

#[test]
fn atom_is_false_for_a_cons() {
    assert_eq!(eval_ok("(atom (quote (a)))"), RtValue::Bool(false));
}

// ---- equal --------------------------------------------------------------------

#[test]
fn equal_is_true_for_separately_built_equal_lists() {
    let src = "(equal (quote (a b c)) (quote (a b c)))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn equal_is_false_for_different_lists() {
    let src = "(equal (quote (a b c)) (quote (a b d)))";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

#[test]
fn equal_is_false_for_different_lengths() {
    let src = "(equal (quote (a b)) (quote (a b c)))";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

#[test]
fn equal_compares_string_content_not_identity() {
    // Two separately-built `Sexpr::Str`s with the same text: `eq` would say
    // false (different heap allocations), `equal` must say true.
    let src = r#"(equal (quote ("hi")) (quote ("hi")))"#;
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn equal_on_atoms_matches_eq() {
    assert_eq!(eval_ok("(equal (quote a) (quote a))"), RtValue::Bool(true));
    assert_eq!(eval_ok("(equal (quote a) (quote b))"), RtValue::Bool(false));
}

#[test]
fn equal_requires_matching_sexpr_type() {
    type_error("(equal 1 2)");
}

// ---- step 7b: Sexpr-list library ------------------------------------------------

fn eval_true(src: &str) {
    assert_eq!(eval_ok(src), RtValue::Bool(true), "expected true: {}", src);
}

#[test]
fn length_of_a_list() {
    assert_eq!(eval_ok("(length (quote (a b c)))"), RtValue::Int(3));
}

#[test]
fn length_of_nil_is_zero() {
    assert_eq!(eval_ok("(length (quote ()))"), RtValue::Int(0));
}

#[test]
fn length_panics_on_a_non_list() {
    assert!(matches!(run("(length (quote a))"), Err(EvalError::Panic(_))));
}

#[test]
fn append_concatenates_two_lists() {
    eval_true("(equal (append (quote (a b)) (quote (c d))) (quote (a b c d)))");
}

#[test]
fn append_with_nil_is_identity() {
    eval_true("(equal (append (quote ()) (quote (a))) (quote (a)))");
    eval_true("(equal (append (quote (a)) (quote ())) (quote (a)))");
}

#[test]
fn reverse_a_list() {
    eval_true("(equal (reverse (quote (a b c))) (quote (c b a)))");
}

#[test]
fn nthcdr_skips_n_elements() {
    eval_true("(equal (nthcdr 2 (quote (a b c d))) (quote (c d)))");
}

#[test]
fn nth_returns_the_element_at_an_index() {
    eval_true("(equal (nth 1 (quote (a b c))) (quote b))");
}

#[test]
fn nth_out_of_range_is_nil() {
    eval_true("(null (nth 10 (quote (a b c))))");
}

#[test]
fn elt_is_nth_with_reversed_argument_order() {
    eval_true("(equal (elt (quote (a b c)) 2) (quote c))");
}

#[test]
fn last_returns_the_final_cons_cell() {
    eval_true("(equal (last (quote (a b c))) (quote (c)))");
}

#[test]
fn butlast_drops_the_final_element() {
    eval_true("(equal (butlast (quote (a b c))) (quote (a b)))");
}

#[test]
fn take_takes_the_first_n_elements() {
    eval_true("(equal (take 2 (quote (a b c d))) (quote (a b)))");
}

#[test]
fn subseq_extracts_a_range() {
    eval_true("(equal (subseq (quote (a b c d e)) 1 3) (quote (b c)))");
}

#[test]
fn copy_list_produces_an_equal_but_separate_list() {
    eval_true("(equal (copy-list (quote (a b c))) (quote (a b c)))");
}

#[test]
fn member_finds_the_sublist_starting_at_the_item() {
    eval_true("(equal (member (quote b) (quote (a b c))) (quote (b c)))");
}

#[test]
fn member_returns_nil_when_not_found() {
    eval_true("(null (member (quote z) (quote (a b c))))");
}

#[test]
fn find_if_returns_the_first_matching_element() {
    let src = "(defun is-b ((x Sexpr)) bool (eq x (quote b))) \
               (equal (find-if is-b (quote (a b c))) (quote b))";
    eval_true(src);
}

#[test]
fn every_is_true_only_if_all_elements_match() {
    let src = "(defun truthy ((x Sexpr)) bool true) (every truthy (quote (a b c)))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
    let src2 = "(defun is-a ((x Sexpr)) bool (eq x (quote a))) (every is-a (quote (a b c)))";
    assert_eq!(eval_ok(src2), RtValue::Bool(false));
}

#[test]
fn some_question_mark_is_true_if_any_element_matches() {
    let src = "(defun is-b ((x Sexpr)) bool (eq x (quote b))) (some? is-b (quote (a b c)))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
    let src2 = "(defun is-z ((x Sexpr)) bool (eq x (quote z))) (some? is-z (quote (a b c)))";
    assert_eq!(eval_ok(src2), RtValue::Bool(false));
}

#[test]
fn count_if_counts_matching_elements() {
    let src = "(defun is-a ((x Sexpr)) bool (eq x (quote a))) (count-if is-a (quote (a b a c a)))";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

#[test]
fn count_counts_occurrences_of_an_item() {
    assert_eq!(eval_ok("(count (quote a) (quote (a b a c a)))"), RtValue::Int(3));
}

#[test]
fn position_returns_some_index_when_found() {
    let src = "(match (position (quote b) (quote (a b c))) ((Some i) i) ((None) -1))";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn position_returns_none_when_not_found() {
    let src = "(match (position (quote z) (quote (a b c))) ((Some i) i) ((None) -1))";
    assert_eq!(eval_ok(src), RtValue::Int(-1));
}

#[test]
fn remove_drops_matching_items() {
    eval_true("(equal (remove (quote a) (quote (a b a c))) (quote (b c)))");
}

#[test]
fn remove_if_drops_elements_matching_a_predicate() {
    let src = "(defun is-a ((x Sexpr)) bool (eq x (quote a))) \
               (equal (remove-if is-a (quote (a b a c))) (quote (b c)))";
    eval_true(src);
}

#[test]
fn remove_if_not_keeps_only_elements_matching_a_predicate() {
    let src = "(defun is-a ((x Sexpr)) bool (eq x (quote a))) \
               (equal (remove-if-not is-a (quote (a b a c))) (quote (a a)))";
    eval_true(src);
}

#[test]
fn map_applies_a_function_to_every_element() {
    let src = "(defun wrap ((x Sexpr)) Sexpr (cons x (quote ()))) \
               (equal (map wrap (quote (a b))) (quote ((a) (b))))";
    eval_true(src);
}

#[test]
fn filter_keeps_only_matching_elements() {
    let src = "(defun is-a ((x Sexpr)) bool (eq x (quote a))) \
               (equal (filter is-a (quote (a b a c))) (quote (a a)))";
    eval_true(src);
}

#[test]
fn foldl_accumulates_left_to_right() {
    // (((() . a) . b) . c) -- builds up a cons chain left-to-right
    let src = "(defun step ((acc Sexpr) (x Sexpr)) Sexpr (cons acc x)) \
               (equal (foldl step (quote ()) (quote (a b c))) (quote (((() . a) . b) . c)))";
    eval_true(src);
}

#[test]
fn foldr_accumulates_right_to_left() {
    let src = "(defun step ((x Sexpr) (acc Sexpr)) Sexpr (cons x acc)) \
               (equal (foldr step (quote ()) (quote (a b c))) (quote (a b c)))";
    eval_true(src);
}
