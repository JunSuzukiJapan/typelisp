//! Tests for the typelisp-defined prelude
//! ([cl-equivalence-catalog.md](../docs/cl-equivalence-catalog.md) §2.1,
//! roadmap step 7a): `not`/`consp`/`null`/`atom`/`equal` (`src/prelude.rs`),
//! plus a Rust-side foundation added alongside them — the `eq` coverage
//! gaps it closes (`sexpr`/`bool`/`i32`/`i64`/`f64`). `not` itself moved
//! from a Rust builtin to a plain `defun` here later (`loop`/`break`/
//! `return`/`setf` stage — see `src/prelude.rs`'s own comment on it).

extern crate typelisp;
use std::cell::RefCell;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue};

// Every test below shares one (Heap, Checker, Interp) per thread instead of
// reloading the prelude from scratch each time — loading it ~100+ times is
// negligible under a normal `cargo test` but dominates runtime under `cargo
// miri test` (where each instruction is far more expensive). Reusing state
// is safe here because `defun`/`defmethod` registration is a plain
// `HashMap::insert` (last definition wins) and every test that needs a
// helper function defines it itself in the same source string before using
// it, so cross-test name reuse (e.g. `f`, `is-a`) never observes a stale
// definition from another test.
thread_local! {
    static CTX: RefCell<Option<(Heap, Checker, Interp)>> = const { RefCell::new(None) };
}

fn with_ctx<R>(f: impl FnOnce(&mut Heap, &mut Checker, &mut Interp) -> R) -> R {
    CTX.with(|cell| {
        let mut opt = cell.borrow_mut();
        let (h, chk, interp) = opt.get_or_insert_with(|| {
            let mut h = Heap::with_capacity(1 << 16);
            let mut chk = Checker::new();
            let mut interp = Interp::new();
            load_prelude(&mut h, &mut chk, &mut interp);
            (h, chk, interp)
        });
        f(h, chk, interp)
    })
}

fn run(src: &str) -> Result<RtValue, EvalError> {
    with_ctx(|h, chk, interp| {
        let r = Reader::new();
        let vs = r.read_all(h, src).expect("read failed");
        let mut last = RtValue::Unit;
        for v in vs {
            let tl = chk.check_form(h, &*interp, v).expect("check failed");
            if let Some(val) = interp.exec(h, tl)? {
                last = val;
            }
        }
        Ok(last)
    })
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

fn type_error(src: &str) {
    with_ctx(|h, chk, interp| {
        let r = Reader::new();
        let vs = r.read_all(h, src).expect("read failed");
        let mut result: Result<_, Error> = Ok(());
        for v in vs {
            if let Err(e) = chk.check_form(h, &*interp, v) {
                result = Err(e);
                break;
            }
        }
        assert!(result.is_err(), "expected a type error");
    })
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
fn any_is_true_if_any_element_matches() {
    let src = "(defun is-b ((x Sexpr)) bool (eq x (quote b))) (any is-b (quote (a b c)))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
    let src2 = "(defun is-z ((x Sexpr)) bool (eq x (quote z))) (any is-z (quote (a b c)))";
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

// ---- set-car / set-cdr / nconc / nreverse (destructive operations) -------------

#[test]
fn set_car_overwrites_in_place() {
    // Mutating through a second reference (`tail`, an alias to the same cons
    // cell `lst`'s cdr points at) must be visible through `lst` too — proof
    // this is a true in-place mutation, not a copy.
    let src = "(defun f () bool
                 (let ((lst (quote (a b c))))
                   (let ((tail (cdr lst)))
                     (progn
                       (set-car tail (quote z))
                       (equal lst (quote (a z c)))))))
               (f)";
    eval_true(src);
}

#[test]
fn set_cdr_overwrites_in_place() {
    let src = "(defun f () bool
                 (let ((lst (quote (a b c))))
                   (progn
                     (set-cdr lst (quote (z)))
                     (equal lst (quote (a z))))))
               (f)";
    eval_true(src);
}

#[test]
fn set_car_panics_on_a_non_cons() {
    assert!(matches!(run("(set-car (quote a) (quote z))"), Err(EvalError::Panic(_))));
}

#[test]
fn set_cdr_panics_on_a_non_cons() {
    assert!(matches!(run("(set-cdr (quote a) (quote z))"), Err(EvalError::Panic(_))));
}

#[test]
fn nconc_concatenates_and_mutates_the_first_list_in_place() {
    let src = "(defun f () bool
                 (let ((a (quote (1 2))))
                   (let ((result (nconc a (quote (3 4)))))
                     (and (equal result (quote (1 2 3 4)))
                          (equal a (quote (1 2 3 4)))))))
               (f)";
    eval_true(src);
}

#[test]
fn nconc_with_an_empty_first_list_returns_the_second_unchanged() {
    eval_true("(equal (nconc (quote ()) (quote (1 2))) (quote (1 2)))");
}

#[test]
fn nreverse_reverses_a_list() {
    eval_true("(equal (nreverse (quote (1 2 3))) (quote (3 2 1)))");
}

#[test]
fn nreverse_of_nil_is_nil() {
    eval_true("(null (nreverse (quote ())))");
}

#[test]
fn nreverse_of_a_single_element_list_is_unchanged() {
    eval_true("(equal (nreverse (quote (1))) (quote (1)))");
}

// ---- Option<T>/Result<T,E> accessors (step 7c) ------------------------------
//
// `(none)`/`(ok ..)`/`(err ..)` constructed with no surrounding type context
// can't infer every type parameter on their own (e.g. `(none)` alone can't
// learn its `T`) — this is `check_construct`'s ordinary inference limit, not
// specific to these methods, so every case below goes through a `defun` with
// an explicit return type to give the constructor an `expected` type to seed
// from (matching `check_call`'s "non-generic param implies a concrete
// `expected`" path).

#[test]
fn unwrap_returns_the_some_payload() {
    assert_eq!(eval_ok("(unwrap (option::some 5))"), RtValue::Int(5));
}

#[test]
fn unwrap_panics_on_none() {
    let src = "(defun get-opt () Option<i32> (option::none)) (unwrap (get-opt))";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn unwrap_or_returns_the_payload_when_some() {
    assert_eq!(eval_ok("(unwrap-or (option::some 5) 9)"), RtValue::Int(5));
}

#[test]
fn unwrap_or_returns_the_default_when_none() {
    let src = "(defun get-opt () Option<i32> (option::none)) (unwrap-or (get-opt) 9)";
    assert_eq!(eval_ok(src), RtValue::Int(9));
}

#[test]
fn is_some_distinguishes_some_from_none() {
    eval_true("(is-some (option::some 1))");
    let src = "(defun get-opt () Option<i32> (option::none)) (is-some (get-opt))";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

#[test]
fn is_none_distinguishes_none_from_some() {
    let src = "(defun get-opt () Option<i32> (option::none)) (is-none (get-opt))";
    eval_true(src);
    assert_eq!(eval_ok("(is-none (option::some 1))"), RtValue::Bool(false));
}

#[test]
fn result_unwrap_returns_the_ok_payload() {
    let src = "(defun get-r () Result<i32,Error> (result::ok 7)) (unwrap (get-r))";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn result_unwrap_panics_on_err() {
    let src = r#"(defun get-r () Result<i32,Error> (result::err (error::error "boom"))) (unwrap (get-r))"#;
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn result_unwrap_or_returns_the_default_on_err() {
    let src = r#"(defun get-r () Result<i32,Error> (result::err (error::error "boom"))) (unwrap-or (get-r) 99)"#;
    assert_eq!(eval_ok(src), RtValue::Int(99));
}

#[test]
fn result_is_ok_and_is_err() {
    let ok_src = "(defun get-r () Result<i32,Error> (result::ok 7)) (is-ok (get-r))";
    let err_src = r#"(defun get-r () Result<i32,Error> (result::err (error::error "x"))) (is-err (get-r))"#;
    eval_true(ok_src);
    eval_true(err_src);
}

#[test]
fn unwrap_resolves_to_the_correct_method_per_receiver_type() {
    // `unwrap` is defined on both `Option<T>` and `Result<T,E>` — same name,
    // disambiguated by `Checker::check_instance_method` from each call's
    // receiver type, like any other type's instance methods.
    let src = r#"
        (defun get-r () Result<i32,Error> (result::ok 3))
        (+ (unwrap (option::some 4)) (unwrap (get-r)))
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

// ---- higher-order helpers (step 7c) -----------------------------------------

#[test]
fn identity_returns_its_argument_at_any_type() {
    assert_eq!(eval_ok("(identity 42)"), RtValue::Int(42));
    eval_true("(identity true)");
}

#[test]
fn const_ignores_its_second_argument() {
    assert_eq!(eval_ok("(const 7 true)"), RtValue::Int(7));
}

#[test]
fn compose_applies_g_then_f() {
    let src = r#"
        (defun add1 ((n i32)) i32 (+ n 1))
        (defun double ((n i32)) i32 (* n 2))
        ((compose double add1) 5)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(12)); // double(add1(5)) = double(6) = 12
}

#[test]
fn flip_swaps_the_argument_order() {
    let src = r#"
        (defun sub ((a i32) (b i32)) i32 (- a b))
        ((flip sub) 3 10)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(7)); // sub(10, 3) = 7
}

// ---- remaining numeric/list helpers (step 7c) -------------------------------

#[test]
fn abs_negates_only_negative_numbers() {
    assert_eq!(eval_ok("(abs (- 0 5))"), RtValue::Int(5));
    assert_eq!(eval_ok("(abs 5)"), RtValue::Int(5));
    assert_eq!(eval_ok("(abs 0)"), RtValue::Int(0));
}

#[test]
fn gcd_of_coprime_numbers_is_one() {
    assert_eq!(eval_ok("(gcd 12 18)"), RtValue::Int(6));
    assert_eq!(eval_ok("(gcd 7 13)"), RtValue::Int(1));
}

#[test]
fn gcd_ignores_argument_sign() {
    assert_eq!(eval_ok("(gcd -12 18)"), RtValue::Int(6));
    assert_eq!(eval_ok("(gcd 12 -18)"), RtValue::Int(6));
}

#[test]
fn gcd_with_zero_is_the_other_argument() {
    assert_eq!(eval_ok("(gcd 0 5)"), RtValue::Int(5));
}

#[test]
fn lcm_of_four_and_six_is_twelve() {
    assert_eq!(eval_ok("(lcm 4 6)"), RtValue::Int(12));
}

#[test]
fn lcm_with_zero_is_zero() {
    assert_eq!(eval_ok("(lcm 0 5)"), RtValue::Int(0));
}

#[test]
fn signum_classifies_positive_negative_and_zero() {
    assert_eq!(eval_ok("(signum 5)"), RtValue::Int(1));
    assert_eq!(eval_ok("(signum -5)"), RtValue::Int(-1));
    assert_eq!(eval_ok("(signum 0)"), RtValue::Int(0));
}

/// `(< x y)` over `Sexpr` integers, for use as `sort`'s comparator below —
/// `Sexpr` itself has no `<` (only `eq`), so every test supplies its own.
const SEXPR_INT_LT: &str = r#"
    (defun sexpr-lt ((a Sexpr) (b Sexpr)) bool
      (match a ((Int x) (match b ((Int y) (< x y)) (_ false))) (_ false)))
"#;

#[test]
fn sort_orders_a_list_ascending() {
    let src = format!("{} (equal (sort sexpr-lt (quote (3 1 2))) (quote (1 2 3)))", SEXPR_INT_LT);
    eval_true(&src);
}

#[test]
fn sort_of_an_empty_list_is_empty() {
    let src = format!("{} (null (sort sexpr-lt (quote ())))", SEXPR_INT_LT);
    eval_true(&src);
}

#[test]
fn sort_keeps_duplicate_elements() {
    let src = format!(
        "{} (equal (sort sexpr-lt (quote (3 1 2 1 5 1))) (quote (1 1 1 2 3 5)))",
        SEXPR_INT_LT
    );
    eval_true(&src);
}

#[test]
fn assoc_finds_the_matching_pair() {
    eval_true("(equal (assoc (quote b) (quote ((a . 1) (b . 2)))) (quote (b . 2)))");
}

#[test]
fn assoc_returns_nil_when_absent() {
    eval_true("(null (assoc (quote z) (quote ((a . 1) (b . 2)))))");
}

// ---- until / while-let (step 8b) --------------------------------------------

#[test]
fn until_runs_the_body_while_the_test_is_false() {
    assert_eq!(
        eval_ok("(let ((i 0)) (until (= i 5) (setf i (+ i 1))) i)"),
        RtValue::Int(5)
    );
}

#[test]
fn until_does_not_run_the_body_when_the_test_is_already_true() {
    assert_eq!(
        eval_ok("(let ((i 9)) (until (= i 9) (setf i 0)) i)"),
        RtValue::Int(9)
    );
}

#[test]
fn while_let_drains_an_option_producing_call_until_none() {
    let src = r#"
        (defun next ((n i32)) Option<i32> (if (> n 0) (option::some n) (option::none)))
        (let ((i 5) (sum 0))
          (while-let ((some x) (next i))
            (setf sum (+ sum x))
            (setf i (- i 1)))
          sum)
    "#;
    // 5 + 4 + 3 + 2 + 1 = 15
    assert_eq!(eval_ok(src), RtValue::Int(15));
}

#[test]
fn while_let_does_not_run_the_body_when_the_pattern_never_matches() {
    let src = r#"
        (defun get-opt () Option<i32> (option::none))
        (let ((ran false))
          (while-let ((some x) (get-opt)) (setf ran true))
          ran)
    "#;
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

// ---- case (step 8c) ----------------------------------------------------------

#[test]
fn case_matches_the_first_equal_clause() {
    assert_eq!(eval_ok("(case 1 (1 100) (2 200) (else 999))"), RtValue::Int(100));
}

#[test]
fn case_matches_a_later_clause() {
    assert_eq!(eval_ok("(case 2 (1 100) (2 200) (else 999))"), RtValue::Int(200));
}

#[test]
fn case_falls_through_to_else_when_nothing_matches() {
    assert_eq!(eval_ok("(case 3 (1 100) (2 200) (else 999))"), RtValue::Int(999));
}

#[test]
fn case_matches_explicitly_quoted_symbol_keys() {
    assert_eq!(eval_ok("(case (quote b) ('a 1) ('b 2) (else 0))"), RtValue::Int(2));
}

#[test]
fn case_evaluates_its_expr_exactly_once() {
    let src = r#"
        (defvar calls 0)
        (defun next-call () i32 (progn (setf calls (+ calls 1)) calls))
        (progn (case (next-call) (1 100) (else 0)) calls)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

// ---- do (step 8d) -------------------------------------------------------------

#[test]
fn do_runs_its_body_and_steps_a_single_binding() {
    let src = "(let ((acc 0)) (do ((i 0 (+ i 1))) ((= i 5) acc) (setf acc (+ acc i))) acc)";
    // 0 + 1 + 2 + 3 + 4 = 10
    assert_eq!(eval_ok(src), RtValue::Int(10));
}

#[test]
fn do_returns_the_result_forms_value() {
    assert_eq!(eval_ok("(do ((i 0 (+ i 1))) ((= i 3) (* i 100)) ())"), RtValue::Int(300));
}

#[test]
fn do_does_not_run_the_body_when_the_test_is_already_true() {
    let src = "(let ((ran false)) (do ((i 0 (+ i 1))) ((= i 0) 0) (setf ran true)) ran)";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

#[test]
fn do_steps_multiple_bindings_in_parallel() {
    // Fibonacci via two parallel bindings: each step must see the *old*
    // values of both `a` and `b`, not a half-updated state — `b`'s step
    // (`(+ a b)`) needs `a`'s pre-step value even though `a` is listed
    // first and could otherwise have already been reassigned.
    let src = "(do ((a 0 b) (b 1 (+ a b)) (n 0 (+ n 1))) ((= n 6) a) ())";
    assert_eq!(eval_ok(src), RtValue::Int(8));
}
