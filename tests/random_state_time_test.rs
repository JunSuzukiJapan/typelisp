//! Tests for the CL-parity additions: `random-state` (`make-random-state`/
//! `random-state-p`/`random`'s optional state argument) and time (`time`/
//! `get-universal-time`/`get-internal-real-time`).

extern crate typelisp;
use typelisp::{load_prelude, load_compiler, Checker, EvalError, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl)? {
            last = v;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

// ---- random / random-state --------------------------------------------------

#[test]
fn random_draws_stay_within_bounds() {
    let src = "(defun all-in-range () bool
                 (let ((ok true) (i 0))
                   (while (< i 200)
                     (let ((n (random 10)))
                       (if (if (>= n 0) (< n 10) false) () (progn (setf ok false) ())))
                     (setf i (+ i 1)))
                   ok))
               (all-in-range)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn make_random_state_returns_a_random_state_p_value() {
    let src = "(random-state-p (make-random-state))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn make_random_state_fresh_states_are_independent_objects() {
    // Two independently-seeded states drawing the same, sufficiently long
    // sequence should disagree somewhere — this would only spuriously fail
    // with astronomically low probability.
    let src = "(defun draws ((s random-state)) i64
                 (let ((acc (the i64 0)) (i 0))
                   (while (< i 15)
                     (setf acc (+ (* acc 10) (as i64 (random 10 s))))
                     (setf i (+ i 1)))
                   acc))
               (let ((a (make-random-state)) (b (make-random-state)))
                 (= (draws a) (draws b)))";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

#[test]
fn random_state_copy_replays_the_same_sequence() {
    // A copy starts at the same point as the original, so drawing the same
    // number of values from each must agree exactly.
    let src = "(defun draws ((s random-state)) i64
                 (let ((acc (the i64 0)) (i 0))
                   (while (< i 15)
                     (setf acc (+ (* acc 10) (as i64 (random 10 s))))
                     (setf i (+ i 1)))
                   acc))
               (let ((a (make-random-state)))
                 (let ((b (make-random-state a)))
                   (= (draws a) (draws b))))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn random_with_an_explicit_state_stays_within_bounds() {
    let src = "(let ((s (make-random-state)))
                 (let ((ok true) (i 0))
                   (while (< i 200)
                     (let ((n (random 10 s)))
                       (if (if (>= n 0) (< n 10) false) () (progn (setf ok false) ())))
                     (setf i (+ i 1)))
                   ok))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

// ---- time --------------------------------------------------------------------

#[test]
fn get_universal_time_is_a_plausible_unix_era_value() {
    // Seconds since 1900-01-01 UTC; 2026 is comfortably past 3.9e9 and well
    // under 4.2e9 (which would be the year ~2033 in this epoch).
    let src = "(get-universal-time)";
    match eval_ok(src) {
        RtValue::Int(secs) => assert!(secs > 3_900_000_000 && secs < 4_200_000_000, "got {}", secs),
        other => panic!("expected an Int, got {:?}", other),
    }
}

#[test]
fn get_internal_real_time_is_monotonic() {
    let src = "(let ((a (get-internal-real-time)))
                 (let ((b (get-internal-real-time)))
                   (<= a b)))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn time_returns_the_forms_own_value_unchanged() {
    let src = "(time (+ 1 2))";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

#[test]
fn time_evaluates_the_form_exactly_once() {
    let src = "(defvar (calls i32) 0)
               (defun bump () i32 (setf calls (+ calls 1)) calls)
               (time (bump))
               calls";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}
