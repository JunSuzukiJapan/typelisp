//! CL's chapter-5 control forms that need no new machinery —
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 4a's
//! macro-expressible half: `prog1`/`prog2`, `do*`, `ecase`/`ccase`,
//! `setq`/`psetq`/`psetf`, and `pushnew`.
//!
//! `destructuring-bind` is here too (2026-09-05), which turned out to be the
//! same half after all: every variable it binds is an `Option<Sexpr>`, so it
//! is a prelude macro over nested `let*`s and needs no checker support.
//!
//! Elsewhere: `block`/`return-from` in `block_test.rs`, `loop :named` in
//! `loop_dsl_test.rs`, `sleep` in `environment_catalog_test.rs`. Not
//! anywhere, deliberately: `prog`/`prog*` (without `tagbody` it is `let`
//! inside `block`, and the block would need the name `nil`, which this
//! language does not have) and `remf` (property lists are out of scope —
//! the plan's Phase 3c decision).

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
        "#<vector<i32> 1 2 3>"
    );
}

// ----------------------------------------------------------------------
// `destructuring-bind`
// ----------------------------------------------------------------------

/// `expr`'s rendering, or the message that stopped it — which for this form
/// is usually a *check* error, because a lambda list the expander refuses is
/// refused while the macro runs.
fn try_show(expr: &str) -> Result<String, String> {
    let src = format!("(format false \"~a\" {})", expr);
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

fn dbind_fails(expr: &str) -> String {
    try_show(expr).expect_err("expected this destructuring-bind to be rejected")
}

#[test]
fn destructuring_bind_takes_a_list_apart_positionally() {
    assert_eq!(show("(destructuring-bind (a b c) (quote (1 2 3)) (format false \"~a~a~a\" a b c))"), "123");
}

#[test]
fn every_variable_it_binds_is_an_option_sexpr() {
    // Which is the whole answer to "what type do the variables get": an
    // S-expression list is the only kind of list there is, so its elements
    // have no other type they could be. A scalar is reached with `match`.
    assert_eq!(
        show(
            "(destructuring-bind (a b) (quote (1 2)) \
               (+ (match a ((i32 n) n) (_ 0)) (match b ((i32 n) n) (_ 0))))"
        ),
        "3"
    );
}

#[test]
fn a_short_list_is_an_error_not_a_row_of_empties() {
    // `sexpr-car` is lenient — `(sexpr-car ())` is `()` — so a chain of them
    // would bind a short list silently. This is why the expansion checks.
    let msg = dbind_fails("(destructuring-bind (a b) (quote (1)) a)");
    assert!(msg.contains("too few elements"), "{}", msg);
    assert!(msg.contains("(a b)"), "the message names the lambda list: {}", msg);
}

#[test]
fn a_long_list_is_an_error_too() {
    let msg = dbind_fails("(destructuring-bind (a) (quote (1 2)) a)");
    assert!(msg.contains("too many elements"), "{}", msg);
}

#[test]
fn rest_takes_whatever_is_left_including_nothing() {
    assert_eq!(show("(destructuring-bind (a &rest r) (quote (1 2 3)) (format false \"~a|~a\" a r))"), "1|(2 3)");
    assert_eq!(show("(destructuring-bind (a &rest r) (quote (1)) (format false \"~a|~a\" a r))"), "1|()");
    assert_eq!(show("(destructuring-bind (&rest r) (quote (1 2)) r)"), "(1 2)");
    // `&body`, CL's synonym.
    assert_eq!(show("(destructuring-bind (a &body r) (quote (1 2)) r)"), "(2)");
}

#[test]
fn optional_falls_back_to_its_default() {
    assert_eq!(show("(destructuring-bind (a &optional b) (quote (1)) (format false \"~a|~a\" a b))"), "1|()");
    assert_eq!(show("(destructuring-bind (a &optional (b 9)) (quote (1)) (format false \"~a|~a\" a b))"), "1|9");
    assert_eq!(show("(destructuring-bind (a &optional (b 9)) (quote (1 5)) (format false \"~a|~a\" a b))"), "1|5");
}

#[test]
fn a_default_is_not_evaluated_when_the_element_was_there() {
    // CL evaluates an `&optional` default only when it is used. The
    // expansion puts it in the arm that needs it, so this comes out for free
    // — but it is the kind of thing that stops being true by accident.
    assert_eq!(
        show(
            "(let ((n 0)) \
               (progn (destructuring-bind (a &optional (b (progn (setf n 1) 0))) (quote (1 2)) b) n))"
        ),
        "0"
    );
    assert_eq!(
        show(
            "(let ((n 0)) \
               (progn (destructuring-bind (a &optional (b (progn (setf n 1) 0))) (quote (1)) b) n))"
        ),
        "1"
    );
}

#[test]
fn key_parameters_read_a_plist() {
    assert_eq!(show("(destructuring-bind (&key x y) (quote (:x 1 :y 2)) (format false \"~a~a\" x y))"), "12");
    // Order does not matter, and an absent one takes its default.
    assert_eq!(show("(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false \"~a~a\" x y))"), "07");
    // `&rest` and `&key` see the same tail, as they do in CL.
    assert_eq!(
        show("(destructuring-bind (a &rest r &key (k 3)) (quote (1 :k 9)) (format false \"~a|~a|~a\" a r k))"),
        "1|(:k 9)|9"
    );
}

#[test]
fn an_unknown_keyword_is_an_error() {
    let msg = dbind_fails("(destructuring-bind (&key x) (quote (:z 1)) x)");
    assert!(msg.contains(":z is not a keyword"), "{}", msg);
}

#[test]
fn an_odd_key_list_is_an_error() {
    let msg = dbind_fails("(destructuring-bind (&key x) (quote (:x)) x)");
    assert!(msg.contains("odd number"), "{}", msg);
}

#[test]
fn a_nested_lambda_list_is_refused_rather_than_misread() {
    // `defmacro` does not take nested lambda lists either, and the two share
    // one rule. Refused loudly because the quiet reading is available and
    // wrong: `(b c)` would otherwise parse as "b, defaulting to c" and bind
    // the whole sublist to `b`.
    let msg = dbind_fails("(destructuring-bind (a (b c)) (quote (1 (2 3))) b)");
    assert!(msg.contains("nested lambda list"), "{}", msg);
    let msg = dbind_fails("(destructuring-bind (a &rest (b c)) (quote (1 2)) a)");
    assert!(msg.contains("nested lambda list"), "{}", msg);
}

#[test]
fn a_parameter_after_rest_is_refused() {
    let msg = dbind_fails("(destructuring-bind (a &rest r b) (quote (1 2)) a)");
    assert!(msg.contains("after &rest"), "{}", msg);
}

#[test]
fn an_empty_lambda_list_demands_an_empty_list() {
    assert_eq!(show("(destructuring-bind () (quote ()) 5)"), "5");
    let msg = dbind_fails("(destructuring-bind () (quote (1)) 5)");
    assert!(msg.contains("too many elements"), "{}", msg);
}

#[test]
fn it_nests_inside_itself() {
    assert_eq!(
        show(
            "(destructuring-bind (op args) (quote (f (1 2))) \
               (destructuring-bind (x y) args (format false \"~a ~a ~a\" op x y)))"
        ),
        "f 1 2"
    );
}
