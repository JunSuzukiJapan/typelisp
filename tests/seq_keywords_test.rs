//! CL's sequence keyword arguments — `:key` / `:test` / `:test-not` /
//! `:start` / `:end` / `:from-end` / `:count` — added by
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 3e.
//!
//! Same harness as `seq_catalog_test`: every one of these is a prelude
//! `defun` over the `Iter` trait, so the prelude has to be loaded, and the
//! results are read through their `~a` rendering.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

fn show(expr: &str) -> String {
    let src = format!("(format false \"~a\" {})", expr);
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, &src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// `expr` with `a` = `#<vector<i32> 1 2 2 3>`, `b` = `#<vector<i32> 2 3 4>`,
/// `neg` = `#<vector<i32> -1 -2 -3>` and `al` = an alist of `1->"one"`, `2->"two"`.
fn with_fixtures(expr: &str) -> String {
    show(&format!(
        "(let ((a (the Vector<i32> (Vector::new)))
               (b (the Vector<i32> (Vector::new)))
               (neg (the Vector<i32> (Vector::new)))
               (al (the Vector<cons-cell<i32,string>> (Vector::new))))
           (progn
             (push a 1) (push a 2) (push a 2) (push a 3)
             (push b 2) (push b 3) (push b 4)
             (push neg -1) (push neg -2) (push neg -3)
             (push al (cons 1 \"one\")) (push al (cons 2 \"two\"))
             {}))",
        expr
    ))
}

/// `(lambda ((x i32)) i32 (abs x))` — the `:key` used throughout.
const ABS: &str = "(lambda ((x i32)) i32 (abs x))";

// ------------------------------------------------------------------ `:key`

#[test]
fn key_projects_the_element_and_the_element_still_comes_back() {
    // CL's rule for this family: `:key` is applied to the sequence element
    // before the comparison, never to the item searched for — and what
    // `find` answers is the element itself, not its projection.
    assert_eq!(with_fixtures(&format!("(find 2 (iter neg) :key {})", ABS)), "(some -2)");
    assert_eq!(with_fixtures(&format!("(position 3 (iter neg) :key {})", ABS)), "(some 2)");
    assert_eq!(with_fixtures(&format!("(count 2 (iter neg) :key {})", ABS)), "1");
    assert_eq!(with_fixtures(&format!("(member 3 (iter neg) :key {})", ABS)), "true");
}

#[test]
fn key_reaches_the_predicate_variants_and_the_alist_searches() {
    assert_eq!(
        with_fixtures(&format!(
            "(find-if (iter neg) (lambda ((x i32)) bool (> x 2)) :key {})",
            ABS
        )),
        "(some -3)"
    );
    // `assoc`'s `:key` projects the *car*; `rassoc`'s projects the *cdr*.
    // Looking for 1 among cars projected by `(- k 1)` finds the pair whose
    // car is 2 — and the whole pair comes back, not the projection.
    assert_eq!(
        with_fixtures("(assoc 1 (iter al) :key (lambda ((k i32)) i32 (- k 1)))"),
        "(some #<cons-cell<i32,string> 2 two>)"
    );
    assert_eq!(
        with_fixtures("(rassoc \"ONE\" (iter al) :key (lambda ((s string)) string (upcase s)))"),
        "(some #<cons-cell<i32,string> 1 one>)"
    );
}

#[test]
fn sort_and_merge_compare_the_projections() {
    assert_eq!(
        with_fixtures(&format!(
            "(sort (iter neg) (lambda ((p i32) (q i32)) bool (< p q)) :key {})",
            ABS
        )),
        "#<vector<i32> -1 -2 -3>"
    );
    assert_eq!(
        with_fixtures(&format!(
            "(merge (iter neg) (iter a) (lambda ((p i32) (q i32)) bool (< p q)) :key {})",
            ABS
        )),
        "#<vector<i32> -1 1 -2 2 2 -3 3>"
    );
}

// --------------------------------------------------- `:test` / `:test-not`

#[test]
fn test_replaces_the_eq_bounds_equals() {
    // `(< item element)` — the item is the first argument, as in CL.
    assert_eq!(
        with_fixtures("(find 2 (iter a) :test (lambda ((p i32) (q i32)) bool (< p q)))"),
        "(some 3)"
    );
    assert_eq!(
        with_fixtures("(count 1 (iter a) :test (lambda ((p i32) (q i32)) bool (< p q)))"),
        "3"
    );
}

#[test]
fn test_not_is_the_negation_of_test() {
    assert_eq!(
        with_fixtures("(find 2 (iter a) :test-not (lambda ((p i32) (q i32)) bool (< p q)))"),
        "(some 1)"
    );
    assert_eq!(
        with_fixtures("(remove 2 (iter a) :test-not (lambda ((p i32) (q i32)) bool (= p q)))"),
        "#<vector<i32> 2 2>"
    );
}

#[test]
fn the_set_operations_project_both_sides() {
    // `:key` on a set operation applies to elements of *both* sequences —
    // both are sequence elements, unlike the item searches.
    assert_eq!(
        with_fixtures(&format!("(intersection (iter neg) (iter a) :key {})", ABS)),
        "#<vector<i32> -1 -2 -3>"
    );
    assert_eq!(
        with_fixtures(&format!("(set-difference (iter neg) (iter b) :key {})", ABS)),
        "#<vector<i32> -1>"
    );
    assert_eq!(with_fixtures(&format!("(subsetp (iter neg) (iter a) :key {})", ABS)), "true");
    assert_eq!(
        with_fixtures(&format!("(adjoin -2 (iter a) :key {})", ABS)),
        "#<vector<i32> 1 2 2 3>"
    );
}

// ------------------------------------------------------- `:start` / `:end`

#[test]
fn the_window_bounds_which_elements_a_search_considers() {
    assert_eq!(with_fixtures("(position 2 (iter a) :start 2)"), "(some 2)");
    assert_eq!(with_fixtures("(find 3 (iter a) :end 3)"), "none");
    assert_eq!(with_fixtures("(count 2 (iter a) :start 2)"), "1");
}

#[test]
fn a_position_is_an_index_into_the_whole_sequence_not_the_window() {
    // CL's rule: `:start` moves where the search begins, not where counting
    // does.
    assert_eq!(with_fixtures("(position 3 (iter a) :start 1)"), "(some 3)");
}

#[test]
fn an_element_outside_the_window_is_kept_rather_than_examined() {
    // The modify family's rule, and the reason this is not a filter over the
    // window: `remove` still returns everything the window did not reach.
    assert_eq!(with_fixtures("(remove 2 (iter a) :start 2)"), "#<vector<i32> 1 2 3>");
    assert_eq!(with_fixtures("(substitute 9 2 (iter a) :end 2)"), "#<vector<i32> 1 9 2 3>");
}

// --------------------------------------------------------------- `:from-end`

#[test]
fn from_end_makes_a_search_answer_with_the_last_match() {
    assert_eq!(with_fixtures("(position 2 (iter a))"), "(some 1)");
    assert_eq!(with_fixtures("(position 2 (iter a) :from-end true)"), "(some 2)");
}

// ------------------------------------------------------------------ `:count`

#[test]
fn count_limits_how_many_matches_are_affected_and_from_end_picks_which() {
    assert_eq!(with_fixtures("(remove 2 (iter a) :count 1)"), "#<vector<i32> 1 2 3>");
    assert_eq!(with_fixtures("(substitute 9 2 (iter a) :count 1)"), "#<vector<i32> 1 9 2 3>");
    // The same limit, taken from the other end.
    assert_eq!(
        with_fixtures("(substitute 9 2 (iter a) :count 1 :from-end true)"),
        "#<vector<i32> 1 2 9 3>"
    );
}

// ------------------------------------------------------- `remove-duplicates`

#[test]
fn remove_duplicates_keeps_the_last_of_each_group_as_cl_does() {
    // This is a *behaviour change*: before Phase 3e this function
    // unconditionally kept the first occurrence. CL keeps the last unless
    // `:from-end` says otherwise.
    assert_eq!(with_fixtures("(remove-duplicates (iter a))"), "#<vector<i32> 1 2 3>");
    assert_eq!(with_fixtures("(remove-duplicates (iter b))"), "#<vector<i32> 2 3 4>");
    let mixed = "(let ((v (the Vector<i32> (Vector::new))))
                   (progn (push v 1) (push v 2) (push v 1) (push v 3) {}))";
    assert_eq!(show(&mixed.replace("{}", "(remove-duplicates (iter v))")), "#<vector<i32> 2 1 3>");
    assert_eq!(
        show(&mixed.replace("{}", "(remove-duplicates (iter v) :from-end true)")),
        "#<vector<i32> 1 2 3>"
    );
}

// ------------------------------------------------------- omitting them all

#[test]
fn every_function_still_answers_with_no_keywords_at_all() {
    // The defaultless-`&key` path: each omitted keyword arrives as
    // `Option::none` and the body falls back to the `Eq` bound / the whole
    // sequence / the first match.
    assert_eq!(with_fixtures("(find 2 (iter a))"), "(some 2)");
    assert_eq!(with_fixtures("(position 2 (iter a))"), "(some 1)");
    assert_eq!(with_fixtures("(count 2 (iter a))"), "2");
    assert_eq!(with_fixtures("(member 4 (iter a))"), "false");
    assert_eq!(with_fixtures("(remove 2 (iter a))"), "#<vector<i32> 1 3>");
    assert_eq!(with_fixtures("(substitute 9 2 (iter a))"), "#<vector<i32> 1 9 9 3>");
    assert_eq!(with_fixtures("(union (iter a) (iter b))"), "#<vector<i32> 1 2 3 4>");
    assert_eq!(with_fixtures("(assoc 2 (iter al))"), "(some #<cons-cell<i32,string> 2 two>)");
    assert_eq!(
        with_fixtures("(sort (iter b) (lambda ((p i32) (q i32)) bool (> p q)))"),
        "#<vector<i32> 4 3 2>"
    );
}

#[test]
fn position_if_not_exists_now_completing_the_if_pairs() {
    // The one gap Phase 3b left in "the `-if`/`-if-not` pairs CL has for
    // every search".
    assert_eq!(
        with_fixtures("(position-if-not (iter a) (lambda ((x i32)) bool (< x 2)))"),
        "(some 1)"
    );
}

// ---------------- the `defmethod` side (the Phase 3e remainder) --------------
//
// Phase 3e left two groups without keywords because `defmethod` took neither
// `&optional` nor `&key`: the destructive `Vector<T>` operations and
// `string`'s `search`/`mismatch`. Phase 5b put that machinery in; these are
// the rewritten declarations.
//
// Each destructive operation unpacks its own keywords rather than handing
// them to the non-destructive twin: a keyword parameter arrives as an
// `Option` and the callee's keyword wants the bare value, so what the two
// share is the loop core underneath, not a forwarded argument list. These
// tests are therefore against the *pairs* — a destructive call must answer
// what its non-destructive twin answers, and mutate the receiver to match.

/// A fresh `#<vector<i32> 1 2 2 3>` per expression, since these mutate it.
fn with_vec(expr: &str) -> String {
    show(&format!(
        "(let ((v (the Vector<i32> (Vector::new))))
           (progn (push v 1) (push v 2) (push v 2) (push v 3) {}))",
        expr
    ))
}

#[test]
fn delete_takes_removes_whole_keyword_set() {
    assert_eq!(with_vec("(delete v 2)"), "#<vector<i32> 1 3>");
    assert_eq!(with_vec("(delete v 2 :count 1)"), "#<vector<i32> 1 2 3>");
    assert_eq!(with_vec("(delete v 2 :count 1 :from-end true)"), "#<vector<i32> 1 2 3>");
    assert_eq!(with_vec("(delete v 2 :start 2)"), "#<vector<i32> 1 2 3>");
    assert_eq!(with_vec("(delete v 2 :end 2)"), "#<vector<i32> 1 2 3>");
    assert_eq!(
        with_vec("(delete v 2 :test (lambda ((p i32) (q i32)) bool (/= p q)))"),
        "#<vector<i32> 2 2>"
    );
    assert_eq!(
        with_vec("(delete v 4 :key (lambda ((x i32)) i32 (* x 2)))"),
        "#<vector<i32> 1 3>"
    );
}

/// The receiver itself is left holding the answer — that is what makes these
/// the destructive versions rather than a second spelling of `remove`.
#[test]
fn a_destructive_call_leaves_the_answer_in_the_receiver() {
    assert_eq!(
        with_vec("(progn (delete v 2 :count 1) (format false \"~a\" v))"),
        "#<vector<i32> 1 2 3>"
    );
    assert_eq!(
        with_vec("(progn (fill v 7 :start 1 :end 3) (format false \"~a\" v))"),
        "#<vector<i32> 1 7 7 3>"
    );
}

#[test]
fn the_predicate_and_substitute_forms_take_theirs() {
    let gt1 = "(lambda ((x i32)) bool (> x 1))";
    assert_eq!(with_vec(&format!("(delete-if v {})", gt1)), "#<vector<i32> 1>");
    assert_eq!(with_vec(&format!("(delete-if v {} :count 1)", gt1)), "#<vector<i32> 1 2 3>");
    assert_eq!(with_vec(&format!("(delete-if-not v {})", gt1)), "#<vector<i32> 2 2 3>");
    assert_eq!(with_vec(&format!("(delete-if-not v {} :count 1)", gt1)), "#<vector<i32> 2 2 3>");
    assert_eq!(with_vec("(nsubstitute v 9 2)"), "#<vector<i32> 1 9 9 3>");
    assert_eq!(with_vec("(nsubstitute v 9 2 :count 1)"), "#<vector<i32> 1 9 2 3>");
    assert_eq!(
        with_vec("(nsubstitute v 9 2 :count 1 :from-end true)"),
        "#<vector<i32> 1 2 9 3>"
    );
    assert_eq!(with_vec(&format!("(nsubstitute-if v 0 {})", gt1)), "#<vector<i32> 1 0 0 0>");
    assert_eq!(
        with_vec(&format!("(nsubstitute-if v 0 {} :start 2)", gt1)),
        "#<vector<i32> 1 2 0 0>"
    );
}

/// `delete-duplicates` keeps CL's default — the *last* of each group — and
/// `:from-end` keeps the first, exactly as `remove-duplicates` does.
#[test]
fn delete_duplicates_follows_remove_duplicates() {
    assert_eq!(with_vec("(delete-duplicates v)"), "#<vector<i32> 1 2 3>");
    assert_eq!(with_vec("(delete-duplicates v :from-end true)"), "#<vector<i32> 1 2 3>");
    assert_eq!(
        with_vec("(delete-duplicates v :key (lambda ((x i32)) i32 (mod x 2)))"),
        "#<vector<i32> 2 3>"
    );
    assert_eq!(
        with_vec("(delete-duplicates v :key (lambda ((x i32)) i32 (mod x 2)) :from-end true)"),
        "#<vector<i32> 1 2>"
    );
}

/// `replace` keeps CL's *numbers*, because the receiver is CL's own
/// `sequence-1` — the one place in this rewrite where they line up.
#[test]
fn replace_takes_cls_four_windows() {
    let src = "(let ((s (the Vector<i32> (Vector::new)))) (progn (push s 8) (push s 9) (push s 10) s))";
    assert_eq!(
        with_vec(&format!("(replace v {})", src)),
        "#<vector<i32> 8 9 10 3>"
    );
    assert_eq!(
        with_vec(&format!("(replace v {} :start1 2 :start2 1)", src)),
        "#<vector<i32> 1 2 9 10>"
    );
    assert_eq!(
        with_vec(&format!("(replace v {} :end1 1)", src)),
        "#<vector<i32> 8 2 2 3>"
    );
    assert_eq!(
        with_vec(&format!("(replace v {} :start2 2 :end2 3)", src)),
        "#<vector<i32> 10 2 2 3>"
    );
}

#[test]
fn search_takes_named_windows_and_the_test_keywords() {
    assert_eq!(show(r#"(search "xabcabc" "abc")"#), "(some 1)");
    assert_eq!(show(r#"(search "xabcabc" "abc" :from-end true)"#), "(some 4)");
    assert_eq!(show(r#"(search "xabcabc" "abc" :start 2)"#), "(some 4)");
    assert_eq!(show(r#"(search "xabcabc" "abc" :end 4)"#), "(some 1)");
    assert_eq!(show(r#"(search "xyz" "abc")"#), "none");
    // CL: an empty pattern occurs at the start of the window.
    assert_eq!(show(r#"(search "xyz" "")"#), "(some 0)");
    assert_eq!(show(r#"(search "xyz" "" :start 2)"#), "(some 2)");
    // `:sub-start`/`:sub-end` bound the pattern — CL's `:start1`/`:end1`,
    // renamed because the arguments are in the other order here.
    assert_eq!(show(r#"(search "xbcabc" "abc" :sub-start 1)"#), "(some 1)");
    assert_eq!(
        show(
            r#"(search "xABc" "abc" :test
                 (lambda ((p char) (q char)) bool (equal (downcase p) (downcase q))))"#
        ),
        "(some 1)"
    );
    assert_eq!(
        show(r#"(search "xABc" "abc" :key (lambda ((c char)) char (downcase c)))"#),
        "(some 1)"
    );
}

/// CL's own numbers for `mismatch`, whose argument order does agree, and
/// CL's `:from-end` rule: one *past* the rightmost difference.
#[test]
fn mismatch_takes_cls_numbered_windows() {
    assert_eq!(show(r#"(mismatch "abc" "abc")"#), "none");
    assert_eq!(show(r#"(mismatch "abc" "abd")"#), "(some 2)");
    assert_eq!(show(r#"(mismatch "abcd" "ab")"#), "(some 2)");
    assert_eq!(show(r#"(mismatch "abcd" "cd" :from-end true)"#), "(some 2)");
    assert_eq!(show(r#"(mismatch "abcd" "abxd" :from-end true)"#), "(some 3)");
    assert_eq!(show(r#"(mismatch "xabc" "abc" :start1 1)"#), "none");
    assert_eq!(show(r#"(mismatch "abc" "xabc" :start2 1)"#), "none");
    assert_eq!(show(r#"(mismatch "abcz" "abc" :end1 3)"#), "none");
    assert_eq!(
        show(r#"(mismatch "ABC" "abc" :key (lambda ((c char)) char (downcase c)))"#),
        "none"
    );
    assert_eq!(
        show(r#"(mismatch "ABC" "abd" :test (lambda ((p char) (q char)) bool (equal (downcase p) (downcase q))))"#),
        "(some 2)"
    );
}

/// A keyword CL spells but this language does not is a *check* error, which
/// is the whole reason `search`'s windows were renamed rather than
/// renumbered: `:start1` on `search` means the pattern in CL and would have
/// meant the sequence here.
#[test]
fn a_cl_keyword_that_does_not_apply_is_rejected() {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, r#"(search "abc" "b" :start1 0)"#).expect("read failed");
    let err = chk.check_form(&mut h, &interp, vs[0]).expect_err("expected a check error");
    assert!(format!("{:?}", err).contains("unknown keyword argument :start1"), "{:?}", err);
}
