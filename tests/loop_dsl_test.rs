//! CL's extended `loop` — [cl-parity-plan.md](../docs/dev/cl-parity-plan.md)
//! Stage 4b.
//!
//! The DSL desugars to ordinary source (`let*`/`loop`/`if`/`setf`/`push`), so
//! these run it for real and read the answer through its `~a` rendering. The
//! prelude is loaded because the sequence clauses reach `copy-seq`/`get`/
//! `push`, exactly as `doiter` does.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<String, String> {
    let src = format!("(format false \"~a\" {})", src);
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, &src).map_err(|e| e.to_string())?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| e.to_string())?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| e.to_string())? {
            last = val;
        }
    }
    match last {
        Value::Str(id) => Ok(h.string(id).to_string()),
        other => Err(format!("expected a string, got {:?}", other)),
    }
}

fn show(src: &str) -> String {
    run(src).unwrap_or_else(|e| panic!("{}\n  in: {}", e, src))
}

/// The error message from a `loop` that should not check.
fn fails(src: &str) -> String {
    run(src).expect_err("expected this loop to be rejected")
}

/// `expr` with `v` = `#<vector<i32> 0 1 2 3>` in scope.
fn with_v(expr: &str) -> String {
    show(&format!(
        "(let ((v (the Vector<i32> (Vector::new))))
           (progn (push v 0) (push v 1) (push v 2) (push v 3) {}))",
        expr
    ))
}

// --------------------------------------------------------------- stepping

#[test]
fn a_numeric_range_steps_up_down_and_by() {
    assert_eq!(show("(loop :for i :from 1 :to 3 :collect i)"), "#<vector<i32> 1 2 3>");
    assert_eq!(show("(loop :for i :from 0 :below 3 :collect i)"), "#<vector<i32> 0 1 2>");
    assert_eq!(show("(loop :for i :from 0 :to 10 :by 3 :collect i)"), "#<vector<i32> 0 3 6 9>");
    assert_eq!(show("(loop :for i :from 3 :downto 1 :collect i)"), "#<vector<i32> 3 2 1>");
    assert_eq!(show("(loop :for i :from 3 :above 1 :collect i)"), "#<vector<i32> 3 2>");
}

#[test]
fn a_sequence_clause_walks_any_iter_and_on_gives_the_suffixes() {
    assert_eq!(with_v("(loop :for x :in (iter v) :collect (* x x))"), "#<vector<i32> 0 1 4 9>");
    // `:across` is the same clause under CL's other spelling — this language
    // has one sequence protocol, not a list/vector split.
    assert_eq!(with_v("(loop :for x :across (iter v) :collect x)"), "#<vector<i32> 0 1 2 3>");
    // `:on` yields fresh `Vector`s, not shared tail conses.
    assert_eq!(with_v("(loop :for s :on (iter v) :collect (len s))"), "#<vector<i32> 4 3 2 1>");
}

#[test]
fn repeat_counts_passes_and_equals_then_walks_its_own_recurrence() {
    assert_eq!(show("(loop :repeat 3 :collect 7)"), "#<vector<i32> 7 7 7>");
    assert_eq!(show("(loop :repeat 4 :for x = 1 :then (* x 2) :collect x)"), "#<vector<i32> 1 2 4 8>");
}

#[test]
fn parallel_for_clauses_stop_with_the_shortest() {
    assert_eq!(with_v("(loop :for i :from 10 :to 99 :for x :in (iter v) :collect (+ i x))"),
               "#<vector<i32> 10 12 14 16>");
}

#[test]
fn with_binds_once_and_can_read_an_earlier_clause() {
    assert_eq!(show("(loop :for i :from 1 :to 3 :with k = 10 :collect (+ i k))"),
               "#<vector<i32> 11 12 13>");
}

// ----------------------------------------------------------- accumulating

#[test]
fn every_accumulation_clause_answers() {
    assert_eq!(show("(loop :for i :from 1 :to 4 :sum i)"), "10");
    assert_eq!(show("(loop :for i :from 1 :to 5 :count (evenp i))"), "2");
    assert_eq!(with_v("(loop :for x :in (iter v) :maximize x)"), "(some 3)");
    assert_eq!(with_v("(loop :for x :in (iter v) :minimize x)"), "(some 0)");
    assert_eq!(with_v("(loop :for i :from 1 :to 2 :append (iter v))"),
               "#<vector<i32> 0 1 2 3 0 1 2 3>");
}

#[test]
fn maximize_over_nothing_is_none_rather_than_a_least_element() {
    // CL answers nil; `Option` is what that is here, and it is also the only
    // honest answer — an arbitrary `Ord` type has no bottom to start from.
    assert_eq!(
        show("(loop :for x :in (iter (the Vector<i32> (Vector::new))) :maximize x)"),
        "none"
    );
}

#[test]
fn the_accumulator_element_type_is_whatever_was_collected() {
    // The one thing the desugaring cannot infer for itself: `Vector::new`'s
    // type argument comes from the expected type, so the checker works it out
    // and writes it in. A non-numeric element proves it is not hardcoded.
    assert_eq!(show("(loop :for i :from 1 :to 3 :collect (format false \"n~a\" i))"),
               "#<vector<string> n1 n2 n3>");
    assert_eq!(show("(loop :for i :from 1 :to 3 :collect (evenp i))"),
               "#<vector<bool> false true false>");
}

#[test]
fn into_names_an_accumulator_that_finally_can_read() {
    assert_eq!(show("(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2)))"), "20");
}

// ------------------------------------------------------------- conditions

#[test]
fn when_unless_and_if_else_guard_one_clause() {
    assert_eq!(show("(loop :for i :from 1 :to 6 :when (evenp i) :collect i)"), "#<vector<i32> 2 4 6>");
    assert_eq!(show("(loop :for i :from 1 :to 6 :unless (evenp i) :collect i)"), "#<vector<i32> 1 3 5>");
    assert_eq!(show("(loop :for i :from 1 :to 4 :if (evenp i) :collect i :else :collect 0)"),
               "#<vector<i32> 0 2 0 4>");
}

#[test]
fn while_and_until_end_the_loop_normally_so_finally_still_runs() {
    assert_eq!(show("(loop :for i :from 1 :to 100 :while (< i 4) :collect i)"), "#<vector<i32> 1 2 3>");
    assert_eq!(show("(loop :for i :from 1 :to 100 :until (> i 3) :collect i)"), "#<vector<i32> 1 2 3>");
    assert_eq!(show("(loop :for i :from 1 :to 100 :until (> i 3) :sum i :into s \
                      :finally (return (- s 1)))"), "5");
}

// --------------------------------------------------------------- verdicts

#[test]
fn always_never_and_thereis_leave_early_with_their_verdict() {
    assert_eq!(show("(loop :for i :from 1 :to 3 :always (> i 0))"), "true");
    assert_eq!(show("(loop :for i :from 1 :to 3 :always (> i 1))"), "false");
    assert_eq!(show("(loop :for i :from 1 :to 3 :never (> i 5))"), "true");
    assert_eq!(show("(loop :for i :from 1 :to 3 :never (> i 2))"), "false");
    assert_eq!(
        show("(loop :for i :from 1 :to 5 :thereis (if (> i 3) (Option::some i) (Option::none)))"),
        "(some 4)"
    );
    assert_eq!(
        show("(loop :for i :from 1 :to 2 :thereis (if (> i 3) (Option::some i) (Option::none)))"),
        "none"
    );
}

// ------------------------------------------------------- order and nesting

#[test]
fn initially_runs_before_the_loop_and_finally_after_it() {
    assert_eq!(
        show("(loop :with acc = 0 :for i :from 1 :to 3 :initially (setf acc 100) \
               :do (setf acc (+ acc i)) :finally (return acc))"),
        "106"
    );
}

#[test]
fn one_loop_nests_inside_another() {
    // Both loops bind state under the same generated names; the inner `let*`
    // shadows the outer, and the outer's stepping sits outside it.
    assert_eq!(
        show("(loop :for i :from 1 :to 3 :collect (loop :for j :from 1 :to i :sum j))"),
        "#<vector<i32> 1 3 6>"
    );
}

#[test]
fn a_loop_whose_first_form_is_not_a_keyword_is_still_the_simple_loop() {
    // CL's own rule, and what keeps every `loop` already written reading the
    // same way.
    assert_eq!(
        show("(let ((n 0)) (progn (loop (setf n (+ n 1)) (if (> n 2) (break) ())) n))"),
        "3"
    );
}

// ----------------------------------------------------------- what it says no to

#[test]
fn an_unknown_clause_word_is_named_in_the_error() {
    assert!(fails("(loop :for i :from 1 :to 3 :frobnicate i)").contains("`:frobnicate` is not a clause"));
}

#[test]
fn a_variable_clause_after_the_body_is_refused_rather_than_reinterpreted() {
    let msg = fails("(loop :do (print \"x\") :for i :from 1 :to 3 :collect i)");
    assert!(msg.contains("must come before the body clauses"), "{}", msg);
}

#[test]
fn thereis_wants_an_option_not_a_bool() {
    let msg = fails("(loop :for i :from 1 :to 3 :thereis (> i 1))");
    assert!(msg.contains("expected an `Option<T>`"), "{}", msg);
    assert!(msg.contains(":always"), "{}", msg);
}

#[test]
fn two_accumulations_into_one_place_must_agree() {
    let msg = fails("(loop :for i :from 1 :to 3 :collect i :sum i)");
    assert!(msg.contains("accumulates two different ways"), "{}", msg);
}

#[test]
fn a_return_with_no_answer_for_exhaustion_says_so() {
    // CL answers nil when the `return` never fires; there is no nil here, so
    // the loop has to say what it leaves with.
    let msg = fails("(loop :for i :from 1 :to 9 :when (= i 4) :return i)");
    assert!(msg.contains("exhaustion"), "{}", msg);
    assert_eq!(show("(loop :for i :from 1 :to 9 :when (= i 4) :return i :finally (return 0))"), "4");
}

// ----------------------------------------------------------------------
// `:named` (Stage 4a — held back until `block`/`return-from` existed)
// ----------------------------------------------------------------------

#[test]
fn named_gives_the_loop_a_block_to_return_from() {
    assert_eq!(
        show("(loop :named o :for i :from 1 :to 9 :do (if (= i 4) (return-from o (* i 10)) ()) :finally (return 0))"),
        "40"
    );
}

#[test]
fn a_named_escape_skips_the_finally_clause() {
    // CL's rule: `return` out of a loop does not run the epilogue. The block
    // is outside `:finally`, so leaving through it jumps past.
    assert_eq!(
        show("(loop :named o :for i :from 1 :to 3 :do (return-from o 1) :finally (return 2))"),
        "1"
    );
}

#[test]
fn a_name_is_optional_and_changes_nothing_when_absent() {
    assert_eq!(show("(loop :for i :from 1 :to 3 :collect i)"), "#<vector<i32> 1 2 3>");
    assert_eq!(show("(loop :named o :for i :from 1 :to 3 :collect i)"), "#<vector<i32> 1 2 3>");
}

#[test]
fn the_name_reaches_out_of_a_nested_loop() {
    assert_eq!(
        show(
            "(loop :named outer :for i :from 1 :to 3 \
               :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ())) \
             :finally (return 0))"
        ),
        "200"
    );
}

#[test]
fn a_named_escape_must_agree_with_what_exhaustion_leaves() {
    // The same rule every `block` has: the body's value and each exit's value
    // join. Without a `:finally` the loop leaves `Unit`, and an `i32` escape
    // has nothing to join with.
    let msg = fails("(loop :named o :for i :from 1 :to 3 :do (if (= i 2) (return-from o i) ()))");
    assert!(msg.contains("incompatible"), "{}", msg);
}

#[test]
fn named_must_come_first() {
    let msg = fails("(loop :for i :from 1 :to 3 :named o :collect i)");
    assert!(!msg.is_empty(), "expected a rejection, got {}", msg);
}

#[test]
fn named_needs_a_name() {
    let msg = fails("(loop :named :for i :from 1 :to 3 :collect i)");
    assert!(msg.contains(":named"), "{}", msg);
}

#[test]
fn break_and_return_still_mean_the_nearest_loop() {
    // `:named` adds an escape; it does not take the old ones away, and a
    // plain `return` in a nested loop still leaves only that one.
    assert_eq!(
        show(
            "(loop :named outer :for i :from 1 :to 3 \
               :do (loop :for j :from 1 :to 9 :do (if (= j 2) (return ()) ())) \
             :finally (return 7))"
        ),
        "7"
    );
}
