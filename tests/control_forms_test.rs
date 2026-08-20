//! CL's chapter-5 control forms that need no new machinery —
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 4a's
//! macro-expressible half: `prog1`/`prog2`, `do*`, `ecase`/`ccase`,
//! `setq`/`psetq`/`psetf`, and `pushnew`.
//!
//! The half that *does* need new machinery — `block`/`return-from`,
//! `prog`/`prog*`, `destructuring-bind`, `remf`, `sleep` — is not here; see
//! the plan for what each of those needs.

extern crate typelisp;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<(Heap, Value), EvalError> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok((h, last))
}

/// `expr` rendered with `~a`, so a `Vector` result is legible.
fn show(expr: &str) -> String {
    let (h, v) = run(&format!("(format false \"~a\" {})", expr)).expect("eval failed");
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// `prog1`/`prog2` run every form and answer with the first/second one's
/// value — the point being that the later forms' side effects still happen.
#[test]
fn prog1_and_prog2_answer_with_an_earlier_forms_value() {
    assert_eq!(
        show("(let ((n 0)) (let ((r (prog1 1 (setf n 9) 3))) (format false \"~a/~a\" r n)))"),
        "1/9"
    );
    assert_eq!(
        show("(let ((n 0)) (let ((r (prog2 (setf n 1) 2 3))) (format false \"~a/~a\" r n)))"),
        "2/1"
    );
}

/// `do*` binds and steps *sequentially*, so a later step sees the earlier
/// ones already updated — the difference from `do`, which computes every step
/// against the old values first.
#[test]
fn do_star_steps_sequentially_unlike_do() {
    // `y` is stepped to `(* x 2)` *after* `x` has become its new value.
    assert_eq!(show("(do* ((x 0 (+ x 1)) (y x (* x 2))) ((> x 3) y))"), "8");
    // The parallel `do` sees the old `x` in `y`'s step, one iteration behind.
    assert_eq!(show("(do ((x 0 (+ x 1)) (y 0 (* x 2))) ((> x 3) y))"), "6");
    // `let*`-style initialization: `y`'s init may name `x`.
    assert_eq!(show("(do* ((x 5 (+ x 1)) (y (* x 2) y)) ((> x 5) y))"), "10");
}

#[test]
fn ecase_matches_like_case_and_panics_when_nothing_does() {
    assert_eq!(show("(ecase 2 (1 \"one\") (2 \"two\"))"), "two");
    // `ccase` has no restart to offer, so it is the same form.
    assert_eq!(show("(ccase 1 (1 \"one\") (2 \"two\"))"), "one");
    // `Heap` is not `Debug`, so the Ok side cannot be unwrapped by
    // `expect_err` — match instead.
    match run("(ecase 7 (1 \"one\") (2 \"two\"))") {
        Err(e) => assert!(format!("{:?}", e).contains("no clause matched"), "{:?}", e),
        Ok(_) => panic!("ecase with no matching clause should have panicked"),
    }
}

#[test]
fn setq_assigns_each_pair_in_order() {
    assert_eq!(
        show("(let ((a 1) (b 2)) (setq a 10 b 20) (format false \"~a/~a\" a b))"),
        "10/20"
    );
    // Sequential, so a later pair sees an earlier assignment.
    assert_eq!(show("(let ((a 1) (b 2)) (setq a 10 b a) b)"), "10");
}

/// `psetq`/`psetf` compute every value before assigning any, which is what
/// makes a swap work without a temporary.
#[test]
fn psetq_and_psetf_assign_in_parallel() {
    assert_eq!(
        show("(let ((a 1) (b 2)) (psetq a b b a) (format false \"~a/~a\" a b))"),
        "2/1"
    );
    assert_eq!(
        show("(let ((a 1) (b 2)) (psetf a 100 b 200) (format false \"~a/~a\" a b))"),
        "100/200"
    );
    // The contrast that names the form: `setq` here would leave both at 2.
    assert_eq!(show("(let ((a 1) (b 2)) (setq a b b a) (format false \"~a/~a\" a b))"), "2/2");
}

/// `pushnew` is a `defmethod`, not a macro: CL needs a macro because its
/// `place` is rewritten, and a `Vector<T>` mutates in place instead.
#[test]
fn pushnew_adds_only_what_is_not_already_there() {
    assert_eq!(
        show(
            "(let ((v (the Vector<i32> (Vector::new))))
               (push v 1) (push v 2) (pushnew v 2) (pushnew v 3) v)"
        ),
        "#<vector 1 2 3>"
    );
}
