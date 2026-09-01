//! Tests for the CL-parity additions: `random-state` (`make-random-state`/
//! `random-state-p`/`random`'s optional state argument) and time (`time`/
//! `get-universal-time`/`get-internal-real-time`).

extern crate typelisp;
use typelisp::{load_prelude, load_compiler, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl)? {
            last = v;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
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
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn make_random_state_returns_a_random_state_p_value() {
    let src = "(random-state-p (make-random-state))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn make_random_state_fresh_states_are_independent_objects() {
    // Two independently-seeded states drawing the same, sufficiently long
    // sequence should disagree somewhere — this would only spuriously fail
    // with astronomically low probability.
    let src = "(defun draws ((s random-state)) i32
                 (let ((acc (the i32 0)) (i 0))
                   (while (< i 15)
                     (setf acc (+ (* acc 10) (as i32 (random 10 s))))
                     (setf i (+ i 1)))
                   acc))
               (let ((a (make-random-state)) (b (make-random-state)))
                 (= (draws a) (draws b)))";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

#[test]
fn random_state_copy_replays_the_same_sequence() {
    // A copy starts at the same point as the original, so drawing the same
    // number of values from each must agree exactly.
    let src = "(defun draws ((s random-state)) i32
                 (let ((acc (the i32 0)) (i 0))
                   (while (< i 15)
                     (setf acc (+ (* acc 10) (as i32 (random 10 s))))
                     (setf i (+ i 1)))
                   acc))
               (let ((a (make-random-state)))
                 (let ((b (make-random-state a)))
                   (= (draws a) (draws b))))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

/// The other half of `random_state_copy_replays_the_same_sequence`: drawing
/// twice from *one* state must not replay, because both draws advance the same
/// stream. This is what makes a `random-state` an identity rather than a value
/// — the property that used to come from sharing one `Rc<Cell<u64>>` and now
/// comes from sharing one `BoxedObj::RandomState`. Without it, "a copy replays"
/// would also pass for an implementation that copied on every read.
#[test]
fn two_draws_from_one_state_advance_the_same_stream() {
    let src = "(defun draws ((s random-state)) i32
                 (let ((acc (the i32 0)) (i 0))
                   (while (< i 15)
                     (setf acc (+ (* acc 10) (as i32 (random 10 s))))
                     (setf i (+ i 1)))
                   acc))
               (let ((a (make-random-state)))
                 (= (draws a) (draws a)))";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

/// A `random-state` is a collectible heap box now, not a Rust-side `Rc`, so it
/// has to stay reachable from a root for as long as the binding holding it
/// does. `gc_stress` collects on every allocation, so a missed root fails every
/// run rather than once the free list happens to run dry.
///
/// The global is the case that matters: a `defvar`'s slot is what the prelude's
/// own `*random-state*` lives in, and it outlives every form that draws from it.
#[test]
fn a_random_state_global_survives_constant_collection() {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();

    let eval = |h: &mut Heap, chk: &mut Checker, interp: &mut Interp, src: &str| -> Value {
        let vs = r.read_all(h, src).expect("read failed");
        let mut last = Value::Empty;
        for v in vs {
            let tl = chk.check_form(h, interp, v).expect("check failed");
            if let Some(v) = interp.exec(h, tl).expect("eval failed") {
                last = v;
            }
        }
        last
    };

    eval(&mut h, &mut chk, &mut interp, "(defvar (*rs* random-state) (make-random-state))");
    h.set_gc_stress(true);
    // Draw across several separate top-level forms, so the global is re-read
    // from its slot after any number of intervening collections.
    for _ in 0..3 {
        let v = eval(&mut h, &mut chk, &mut interp, "(random 10 *rs*)");
        match v {
            Value::Int(n) => assert!((0..10).contains(&n), "draw {} out of bounds", n),
            other => panic!("expected an integer draw, got {:?}", other),
        }
    }
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
    assert_eq!(eval_ok(src), Value::Bool(true));
}

// ---- time --------------------------------------------------------------------

#[test]
fn get_universal_time_is_a_plausible_unix_era_value() {
    // Seconds since 1900-01-01 UTC; 2026 is comfortably past 3.9e9 and well
    // under 4.2e9 (which would be the year ~2033 in this epoch).
    let src = "(get-universal-time)";
    match eval_ok(src) {
        Value::Int(secs) => assert!(secs > 3_900_000_000 && secs < 4_200_000_000, "got {}", secs),
        other => panic!("expected an Int, got {:?}", other),
    }
}

#[test]
fn get_internal_real_time_is_monotonic() {
    let src = "(let ((a (get-internal-real-time)))
                 (let ((b (get-internal-real-time)))
                   (<= a b)))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn time_returns_the_forms_own_value_unchanged() {
    let src = "(time (+ 1 2))";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn time_evaluates_the_form_exactly_once() {
    let src = "(defvar (calls i32) 0)
               (defun bump () i32 (setf calls (+ calls 1)) calls)
               (time (bump))
               calls";
    assert_eq!(eval_ok(src), Value::Int(1));
}
