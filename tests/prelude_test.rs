//! Tests for the typelisp-defined prelude
//! ([cl-equivalence-catalog.md](../docs/dev/cl-equivalence-catalog.md) §2.1,
//! roadmap step 7a): `not`/`consp`/`null`/`atom`/`equal` (`src/prelude.rs`),
//! plus a Rust-side foundation added alongside them — the `eq` coverage
//! gaps it closes (`sexpr`/`bool`/`i32`/`i64`/`f64`). `not` itself moved
//! from a Rust builtin to a plain `defun` here later (`loop`/`break`/
//! `return`/`setf` stage — see `src/prelude.rs`'s own comment on it).

extern crate typelisp;
use std::cell::RefCell;
use typelisp::{load_prelude, load_compiler, Checker, Error, EvalError, Heap, Interp, Reader, Value};

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
            load_compiler(&mut h, &mut chk, &mut interp);
            (h, chk, interp)
        });
        f(h, chk, interp)
    })
}

/// The checker's rendered error for `src`, for the cases that fail during the
/// check rather than the run — a macro that refuses its input, among them.
/// [`run`] cannot be used for those: it `expect`s the check.
fn check_err(src: &str) -> String {
    with_ctx(|h, chk, interp| {
        let r = Reader::new();
        let vs = r.read_all(h, src).expect("read failed");
        let mut err = None;
        for v in vs {
            match chk.check_form(h, &*interp, v) {
                Ok(tl) => {
                    interp.exec(h, tl).expect("eval failed");
                }
                Err(e) => {
                    err = Some(e.to_string());
                    break;
                }
            }
        }
        err.expect("expected the check to fail")
    })
}

fn run(src: &str) -> Result<Value, EvalError> {
    with_ctx(|h, chk, interp| {
        let r = Reader::new();
        let vs = r.read_all(h, src).expect("read failed");
        let mut last = Value::Empty;
        for v in vs {
            let tl = chk.check_form(h, &*interp, v).expect("check failed");
            if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
                last = val;
            }
        }
        Ok(last)
    })
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// The `f64` a run produced. An `f64` is a `BoxedObj::Float` since the scalar
/// unification, so reading one needs the heap it lives in — the same
/// thread-local `CTX` the evaluation used.
fn eval_f64(src: &str) -> f64 {
    let v = eval_ok(src);
    CTX.with(|cell| {
        let opt = cell.borrow();
        let (h, _, _) = opt.as_ref().expect("no evaluation has run yet");
        match v {
            typelisp::Value::Boxed(id) if h.is_float(id) => h.float_value(id),
            other => panic!("expected an f64, got {:?}", other),
        }
    })
}

/// The text of a `string` result — the [`eval_f64`] counterpart, reading from
/// the same thread-local `CTX` heap the evaluation used.
fn eval_string(src: &str) -> String {
    let v = eval_ok(src);
    CTX.with(|cell| {
        let opt = cell.borrow();
        let (h, _, _) = opt.as_ref().expect("no evaluation has run yet");
        match v {
            typelisp::Value::Str(id) => h.string(id).to_string(),
            other => panic!("expected a string, got {:?}", other),
        }
    })
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
    assert_eq!(eval_ok("(not true)"), Value::Bool(false));
    assert_eq!(eval_ok("(not false)"), Value::Bool(true));
}

// ---- eq, across every type that has it -----------------------------------------

#[test]
fn eq_on_bool() {
    assert_eq!(eval_ok("(eq true true)"), Value::Bool(true));
    assert_eq!(eval_ok("(eq true false)"), Value::Bool(false));
}

#[test]
fn eq_on_i32() {
    assert_eq!(eval_ok("(eq 1 1)"), Value::Bool(true));
    assert_eq!(eval_ok("(eq 1 2)"), Value::Bool(false));
}

#[test]
fn eq_on_i64() {
    let src = "(defun f ((a i32) (b i32)) bool (eq a b)) (f 1 1)";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn eq_on_f64() {
    let src = "(defun f ((a f64) (b f64)) bool (eq a b)) (f 1.5 1.5)";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn eq_on_char() {
    assert_eq!(eval_ok(r"(eq #\a #\a)"), Value::Bool(true));
}

#[test]
fn eq_on_string_is_identity_not_content() {
    // Two separately-evaluated string literals with equal content are *not*
    // `eq` — real CL never does structural string comparison under `eq`
    // (or `eql` — see the `eql`/`equal` sections below). `RtValue::Str`'s
    // `Rc<str>` is what makes this distinction meaningful at all (a plain
    // owned `String`, re-cloned on every read, would have no stable
    // identity to test).
    assert_eq!(eval_ok(r#"(eq "a" "a")"#), Value::Bool(false));
}

#[test]
fn eq_on_string_self_is_true() {
    // The same binding read twice stays the same object — reading a
    // variable clones the `Rc` pointer, not the string buffer.
    assert_eq!(eval_ok(r#"(let ((s "a")) (eq s s))"#), Value::Bool(true));
}

// ---- eql: identity, plus same-type/value numbers and characters ---------------
// (see `docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp section) — every
// type here is an immediate scalar except `Str`/`Sexpr`, so `eql` coincides
// with `eq` throughout; it's still tested explicitly per type so a future
// divergence (e.g. a boxed numeric representation) has a clear regression
// signal.

#[test]
fn eql_on_bool() {
    assert_eq!(eval_ok("(eql true true)"), Value::Bool(true));
    assert_eq!(eval_ok("(eql true false)"), Value::Bool(false));
}

#[test]
fn eql_on_i32() {
    assert_eq!(eval_ok("(eql 1 1)"), Value::Bool(true));
    assert_eq!(eval_ok("(eql 1 2)"), Value::Bool(false));
}

#[test]
fn eql_on_char() {
    assert_eq!(eval_ok(r"(eql #\a #\a)"), Value::Bool(true));
}

#[test]
fn eql_on_string_is_identity_not_content() {
    // `eql` does *not* add structural string comparison beyond `eq` in CL —
    // only `equal`/`equalp` do.
    assert_eq!(eval_ok(r#"(eql "a" "a")"#), Value::Bool(false));
}

#[test]
fn eql_on_sexpr_atoms_compares_by_value() {
    assert_eq!(eval_ok("(eql (quote a) (quote a))"), Value::Bool(true));
}

#[test]
fn eq_on_sexpr_atoms_compares_by_value() {
    let src = "(eq (quote a) (quote a))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn eq_on_sexpr_cons_is_identity_not_structural() {
    // Two separately-built (quote (a)) cons cells have equal content but are
    // not the *same* cell — `eq` says false, `equal` says true (see below).
    let src = "(eq (quote (a)) (quote (a)))";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

#[test]
fn eq_on_sexpr_float_is_identity_not_value() {
    // `Sexpr::Float` is heap-boxed (`Value::Boxed`, see `BoxedObj`), so two
    // separately-quoted equal floats are `Cons`/`Str`-like: not the same
    // box, hence not `eq` — see `eql_on_sexpr_float_compares_by_value` for
    // the predicate that *does* treat them as equivalent.
    assert_eq!(eval_ok("(eq (quote 1.5) (quote 1.5))"), Value::Bool(false));
}

#[test]
fn eql_on_sexpr_float_compares_by_value() {
    // CL's `eql`: two numbers of the same type and value are `eql` even when
    // they aren't the same object — the one case `eql` actually diverges
    // from `eq` in this representation (`sexpr_eql`'s doc comment).
    assert_eq!(eval_ok("(eql (quote 1.5) (quote 1.5))"), Value::Bool(true));
}

#[test]
fn eql_on_sexpr_float_is_false_for_different_values() {
    assert_eq!(eval_ok("(eql (quote 1.5) (quote 2.5))"), Value::Bool(false));
}

#[test]
fn equal_on_sexpr_float_compares_by_value() {
    // `equal` delegates to `eql` for non-`Cons`/`Str` atoms (CL's own
    // definition) — regression check for the catch-all arm switching from
    // `eq` to `eql` when `Sexpr::Float` became heap-boxed.
    assert_eq!(eval_ok("(equal (quote 1.5) (quote 1.5))"), Value::Bool(true));
}

#[test]
fn equal_on_sexpr_float_nested_in_a_cons_compares_by_value() {
    assert_eq!(
        eval_ok("(equal (quote (1.5 2.5)) (quote (1.5 2.5)))"),
        Value::Bool(true)
    );
}

#[test]
fn equalp_on_sexpr_float_compares_by_value() {
    assert_eq!(eval_ok("(equalp (quote 1.5) (quote 1.5))"), Value::Bool(true));
}

// ---- consp / null / atom ----------------------------------------------------

#[test]
fn equal_is_true_for_separately_built_equal_lists() {
    let src = "(equal (quote (a b c)) (quote (a b c)))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn equal_is_false_for_different_lists() {
    let src = "(equal (quote (a b c)) (quote (a b d)))";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

#[test]
fn equal_is_false_for_different_lengths() {
    let src = "(equal (quote (a b)) (quote (a b c)))";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

#[test]
fn equal_compares_string_content_not_identity() {
    // Two separately-built `Sexpr::Str`s with the same text: `eq` would say
    // false (different heap allocations), `equal` must say true.
    let src = r#"(equal (quote ("hi")) (quote ("hi")))"#;
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn equal_on_atoms_matches_eq() {
    assert_eq!(eval_ok("(equal (quote a) (quote a))"), Value::Bool(true));
    assert_eq!(eval_ok("(equal (quote a) (quote b))"), Value::Bool(false));
}

#[test]
fn equal_on_i32_compares_by_value() {
    // `equal` is no longer `Sexpr`-only: it's registered as an instance
    // method on every scalar type too (`registry::int_assoc` etc.),
    // specifically so `case` can dispatch uniformly across types including
    // `string` — see `docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp
    // section.
    assert_eq!(eval_ok("(equal 1 1)"), Value::Bool(true));
    assert_eq!(eval_ok("(equal 1 2)"), Value::Bool(false));
}

#[test]
fn equal_rejects_mismatched_types() {
    // Each type's `equal` still requires both operands to be that same
    // type — there is no cross-type overload (that's `equalp`'s job, and
    // even `equalp` only crosses `Sexpr`'s own dynamic tags, not bare
    // statically-typed `i32`/`Str`).
    type_error(r#"(equal 1 "a")"#);
}

// ---- equalp: like equal, but case-insensitive strings/chars -------------------

#[test]
fn equalp_on_string_ignores_ascii_case() {
    assert_eq!(eval_ok(r#"(equalp "ABC" "abc")"#), Value::Bool(true));
    assert_eq!(eval_ok(r#"(equalp "abc" "abd")"#), Value::Bool(false));
}

#[test]
fn equalp_on_char_ignores_ascii_case() {
    assert_eq!(eval_ok(r"(equalp #\A #\a)"), Value::Bool(true));
}

#[test]
fn equalp_on_i32_compares_by_value() {
    assert_eq!(eval_ok("(equalp 1 1)"), Value::Bool(true));
    assert_eq!(eval_ok("(equalp 1 2)"), Value::Bool(false));
}

#[test]
fn equalp_on_sexpr_recurses_with_case_insensitive_strings() {
    let src = r#"(equalp (quote ("HI")) (quote ("hi")))"#;
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn equalp_is_false_for_different_sexpr_lists() {
    assert_eq!(eval_ok("(equalp (quote (a b c)) (quote (a b d)))"), Value::Bool(false));
}

// ---- equalp: Int<->Float cross-type numeric comparison, via `int->float` --------
// (`docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp section: this used to
// fall back to `eql`, which is false across types even at matching value.)

#[test]
fn equalp_on_sexpr_crosses_int_and_float() {
    assert_eq!(eval_ok("(equalp (quote 1) (quote 1.0))"), Value::Bool(true));
    assert_eq!(eval_ok("(equalp (quote 1.0) (quote 1))"), Value::Bool(true));
    assert_eq!(eval_ok("(equalp (quote 1) (quote 2.0))"), Value::Bool(false));
}

#[test]
fn equalp_on_sexpr_still_rejects_other_mismatched_types() {
    assert_eq!(eval_ok(r#"(equalp (quote 1) (quote "1"))"#), Value::Bool(false));
}

// ---- int->float / float->int: numeric conversion primitives --------------------

#[test]
fn int_to_float_converts_i32() {
    assert_eq!(eval_f64("(int->float 3)"), 3.0);
}

#[test]
fn int_to_float_converts_i64() {
    assert_eq!(eval_f64("(int->float (the i32 3))"), 3.0);
}

#[test]
fn float_to_int_truncates_toward_zero() {
    assert_eq!(eval_ok("(float->int 3.9)"), Value::Int(3));
    assert_eq!(eval_ok("(float->int -3.9)"), Value::Int(-3));
}

// ---- char->int / int->char: Unicode scalar value conversion --------------------

#[test]
fn char_to_int_returns_the_scalar_value() {
    assert_eq!(eval_ok(r"(char->int #\A)"), Value::Int(65));
}

#[test]
fn int_to_char_round_trips_char_to_int() {
    assert_eq!(eval_ok("(int->char 65)"), Value::Char('A'));
}

#[test]
fn int_to_char_panics_on_a_surrogate_code_point() {
    assert!(matches!(run("(int->char 55296)"), Err(EvalError::Panic(_))));
}

#[test]
fn int_to_char_panics_past_the_max_scalar_value() {
    assert!(matches!(run("(int->char 1114112)"), Err(EvalError::Panic(_))));
}

// ---- symbol->string / string->symbol --------------------------------------------

#[test]
fn symbol_to_string_extracts_the_name() {
    // `symbol->string : Symbol -> string`. A `Symbol` is produced by
    // `string->symbol` (or `gensym`), not by a quoted-datum `Sexpr`.
    assert_eq!(eval_string(r#"(symbol->string (string->symbol "foo"))"#), "foo");
}

#[test]
fn symbol_to_string_rejects_a_non_symbol_at_check_time() {
    // `(quote 1)` is a `Sexpr` (an int datum), not a `Symbol`, so passing it to
    // `symbol->string` is now a static type error rather than a runtime panic.
    type_error("(symbol->string (quote 1))");
}

#[test]
fn string_to_symbol_round_trips_symbol_to_string() {
    // Stated at `string`, where the two sides are the same type outright.
    assert_eq!(eval_string(r#"(symbol->string (string->symbol "foo"))"#), "foo");
    // And against the quoted symbol, in either order: `'foo` is a `Symbol`
    // too, so both sides of the generic `equal<T>(T, T)` already agree.
    assert_eq!(eval_ok(r#"(equal (string->symbol "foo") (quote foo))"#), Value::Bool(true));
    assert_eq!(eval_ok(r#"(equal (quote foo) (string->symbol "foo"))"#), Value::Bool(true));
}

// `set-car`/`set-cdr` (destructive `Sexpr` cons mutation) were removed with the
// rest of the `Sexpr` list surface — Symbol/Sexpr redesign Phase 5/4b.

// ---- the generic `cons<T,U>` pair (Symbol/Sexpr redesign Phase 4b) ----------
//
// `cons`/`car`/`cdr` are the heterogeneously-typed 2-element pair now
// (`cons-cell<A,B>`), not `Sexpr` cons operations: `car`/`cdr` project the
// *statically typed* element out, and the two elements can be different types.

#[test]
fn cons_builds_a_pair_and_car_cdr_project_it() {
    assert_eq!(eval_ok("(car (cons 1 2))"), Value::Int(1));
    assert_eq!(eval_ok("(cdr (cons 1 2))"), Value::Int(2));
}

#[test]
fn cons_pair_elements_can_have_different_types() {
    // `car` is an `i32`, `cdr` is a `string` — a genuinely heterogeneous pair,
    // unlike a homogeneous `Vector<T>`.
    assert_eq!(eval_ok(r#"(car (cons 7 "x"))"#), Value::Int(7));
    assert_eq!(eval_string(r#"(cdr (cons 7 "x"))"#), "x");
}

#[test]
fn cons_pair_car_is_mutable_via_setf() {
    let src = "(let ((p (cons 1 2))) (setf p::car 9) (car p))";
    assert_eq!(eval_ok(src), Value::Int(9));
}

#[test]
fn car_of_a_non_pair_is_a_type_error() {
    // `car` is `cons-cell`'s field accessor now, not a `Sexpr` operation, so an
    // `i32` receiver has no such method.
    type_error("(car 1)");
}

fn eval_true(src: &str) {
    assert_eq!(eval_ok(src), Value::Bool(true), "expected true: {}", src);
}

#[test]
fn unwrap_returns_the_some_payload() {
    assert_eq!(eval_ok("(unwrap (option::some 5))"), Value::Int(5));
}

#[test]
fn unwrap_panics_on_none() {
    let src = "(defun get-opt () Option<i32> (option::none)) (unwrap (get-opt))";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn unwrap_or_returns_the_payload_when_some() {
    assert_eq!(eval_ok("(unwrap-or (option::some 5) 9)"), Value::Int(5));
}

#[test]
fn unwrap_or_returns_the_default_when_none() {
    let src = "(defun get-opt () Option<i32> (option::none)) (unwrap-or (get-opt) 9)";
    assert_eq!(eval_ok(src), Value::Int(9));
}

#[test]
fn is_some_distinguishes_some_from_none() {
    eval_true("(is-some (option::some 1))");
    let src = "(defun get-opt () Option<i32> (option::none)) (is-some (get-opt))";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

#[test]
fn is_none_distinguishes_none_from_some() {
    let src = "(defun get-opt () Option<i32> (option::none)) (is-none (get-opt))";
    eval_true(src);
    assert_eq!(eval_ok("(is-none (option::some 1))"), Value::Bool(false));
}

#[test]
fn result_unwrap_returns_the_ok_payload() {
    let src = "(defun get-r () Result<i32,ParseIntError> (result::ok 7)) (unwrap (get-r))";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn result_unwrap_panics_on_err() {
    let src = r#"(defun get-r () Result<i32,ParseIntError> (result::err (ParseIntError::ParseIntError "boom"))) (unwrap (get-r))"#;
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn result_unwrap_or_returns_the_default_on_err() {
    let src = r#"(defun get-r () Result<i32,ParseIntError> (result::err (ParseIntError::ParseIntError "boom"))) (unwrap-or (get-r) 99)"#;
    assert_eq!(eval_ok(src), Value::Int(99));
}

#[test]
fn result_is_ok_and_is_err() {
    let ok_src = "(defun get-r () Result<i32,ParseIntError> (result::ok 7)) (is-ok (get-r))";
    let err_src = r#"(defun get-r () Result<i32,ParseIntError> (result::err (ParseIntError::ParseIntError "x"))) (is-err (get-r))"#;
    eval_true(ok_src);
    eval_true(err_src);
}

#[test]
fn unwrap_resolves_to_the_correct_method_per_receiver_type() {
    // `unwrap` is defined on both `Option<T>` and `Result<T,E>` — same name,
    // disambiguated by `Checker::check_instance_method` from each call's
    // receiver type, like any other type's instance methods.
    let src = r#"
        (defun get-r () Result<i32,ParseIntError> (result::ok 3))
        (+ (unwrap (option::some 4)) (unwrap (get-r)))
    "#;
    assert_eq!(eval_ok(src), Value::Int(7));
}

// ---- higher-order helpers (step 7c) -----------------------------------------

#[test]
fn identity_returns_its_argument_at_any_type() {
    assert_eq!(eval_ok("(identity 42)"), Value::Int(42));
    eval_true("(identity true)");
}

#[test]
fn const_ignores_its_second_argument() {
    assert_eq!(eval_ok("(const 7 true)"), Value::Int(7));
}

#[test]
fn compose_applies_g_then_f() {
    let src = r#"
        (defun add1 ((n i32)) i32 (+ n 1))
        (defun double ((n i32)) i32 (* n 2))
        ((compose double add1) 5)
    "#;
    assert_eq!(eval_ok(src), Value::Int(12)); // double(add1(5)) = double(6) = 12
}

#[test]
fn flip_swaps_the_argument_order() {
    let src = r#"
        (defun sub ((a i32) (b i32)) i32 (- a b))
        ((flip sub) 3 10)
    "#;
    assert_eq!(eval_ok(src), Value::Int(7)); // sub(10, 3) = 7
}

// ---- remaining numeric/list helpers (step 7c) -------------------------------

#[test]
fn abs_negates_only_negative_numbers() {
    assert_eq!(eval_ok("(abs (- 0 5))"), Value::Int(5));
    assert_eq!(eval_ok("(abs 5)"), Value::Int(5));
    assert_eq!(eval_ok("(abs 0)"), Value::Int(0));
}

#[test]
fn gcd_of_coprime_numbers_is_one() {
    assert_eq!(eval_ok("(gcd 12 18)"), Value::Int(6));
    assert_eq!(eval_ok("(gcd 7 13)"), Value::Int(1));
}

#[test]
fn gcd_ignores_argument_sign() {
    assert_eq!(eval_ok("(gcd -12 18)"), Value::Int(6));
    assert_eq!(eval_ok("(gcd 12 -18)"), Value::Int(6));
}

#[test]
fn gcd_with_zero_is_the_other_argument() {
    assert_eq!(eval_ok("(gcd 0 5)"), Value::Int(5));
}

#[test]
fn lcm_of_four_and_six_is_twelve() {
    assert_eq!(eval_ok("(lcm 4 6)"), Value::Int(12));
}

#[test]
fn lcm_with_zero_is_zero() {
    assert_eq!(eval_ok("(lcm 0 5)"), Value::Int(0));
}

#[test]
fn signum_classifies_positive_negative_and_zero() {
    assert_eq!(eval_ok("(signum 5)"), Value::Int(1));
    assert_eq!(eval_ok("(signum -5)"), Value::Int(-1));
    assert_eq!(eval_ok("(signum 0)"), Value::Int(0));
}

#[test]
fn until_runs_the_body_while_the_test_is_false() {
    assert_eq!(
        eval_ok("(let ((i 0)) (until (= i 5) (setf i (+ i 1))) i)"),
        Value::Int(5)
    );
}

#[test]
fn until_does_not_run_the_body_when_the_test_is_already_true() {
    assert_eq!(
        eval_ok("(let ((i 9)) (until (= i 9) (setf i 0)) i)"),
        Value::Int(9)
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
    assert_eq!(eval_ok(src), Value::Int(15));
}

#[test]
fn while_let_does_not_run_the_body_when_the_pattern_never_matches() {
    let src = r#"
        (defun get-opt () Option<i32> (option::none))
        (let ((ran false))
          (while-let ((some x) (get-opt)) (setf ran true))
          ran)
    "#;
    assert_eq!(eval_ok(src), Value::Bool(false));
}

// ---- case (step 8c) ----------------------------------------------------------

#[test]
fn case_matches_the_first_equal_clause() {
    assert_eq!(eval_ok("(case 1 (1 100) (2 200) (else 999))"), Value::Int(100));
}

#[test]
fn case_matches_a_later_clause() {
    assert_eq!(eval_ok("(case 2 (1 100) (2 200) (else 999))"), Value::Int(200));
}

#[test]
fn case_falls_through_to_else_when_nothing_matches() {
    assert_eq!(eval_ok("(case 3 (1 100) (2 200) (else 999))"), Value::Int(999));
}

#[test]
fn case_matches_bare_symbol_keys() {
    assert_eq!(eval_ok("(case (quote b) (a 1) (b 2) (else 0))"), Value::Int(2));
}

/// A key list, CL's `((k1 k2) form)` — the thing keys-as-expressions could not
/// express: the list read as a call, and this failed to check.
#[test]
fn case_matches_any_key_in_a_key_list() {
    assert_eq!(eval_ok("(case 2 ((1 2 3) 100) ((8 9) 200) (else 0))"), Value::Int(100));
    assert_eq!(eval_ok("(case 9 ((1 2 3) 100) ((8 9) 200) (else 0))"), Value::Int(200));
    assert_eq!(eval_ok("(case 5 ((1 2 3) 100) ((8 9) 200) (else 0))"), Value::Int(0));
}

/// `'sym` in key position is refused, not read CL's way (the two-key list
/// `{quote, sym}`, which would type-check and never match). Refusing names the
/// fix, which matters here more than in CL: `'sym` was this language's
/// *documented* spelling for a symbol key until keys became literals.
#[test]
fn case_refuses_a_quoted_key() {
    let msg = check_err("(case (quote b) ('a 1) (else 0))");
    assert!(msg.contains("quoted key"), "unexpected error: {}", msg);
}

#[test]
fn case_matches_string_keys_by_content() {
    // A real capability, not just documented intent (`docs/cl-equivalence-catalog.md`'s
    // `case` design goal predates this test) — `case`'s expansion uses
    // `equal`, not `eq`/`eql` (neither does string content comparison in
    // real CL either — see the eq/eql/equal/equalp section), so a string
    // key matches by content even though the scrutinee and the key literal
    // are always separately allocated `Str`s.
    let src = r#"(case "b" ("a" 1) ("b" 2) (else 0))"#;
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn case_evaluates_its_expr_exactly_once() {
    let src = r#"
        (defvar (calls i32) 0)
        (defun next-call () i32 (progn (setf calls (+ calls 1)) calls))
        (progn (case (next-call) (1 100) (else 0)) calls)
    "#;
    assert_eq!(eval_ok(src), Value::Int(1));
}

// ---- do (step 8d) -------------------------------------------------------------

#[test]
fn do_runs_its_body_and_steps_a_single_binding() {
    let src = "(let ((acc 0)) (do ((i 0 (+ i 1))) ((= i 5) acc) (setf acc (+ acc i))) acc)";
    // 0 + 1 + 2 + 3 + 4 = 10
    assert_eq!(eval_ok(src), Value::Int(10));
}

#[test]
fn do_returns_the_result_forms_value() {
    assert_eq!(eval_ok("(do ((i 0 (+ i 1))) ((= i 3) (* i 100)) ())"), Value::Int(300));
}

#[test]
fn do_does_not_run_the_body_when_the_test_is_already_true() {
    let src = "(let ((ran false)) (do ((i 0 (+ i 1))) ((= i 0) 0) (setf ran true)) ran)";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

#[test]
fn do_steps_multiple_bindings_in_parallel() {
    // Fibonacci via two parallel bindings: each step must see the *old*
    // values of both `a` and `b`, not a half-updated state — `b`'s step
    // (`(+ a b)`) needs `a`'s pre-step value even though `a` is listed
    // first and could otherwise have already been reassigned.
    let src = "(do ((a 0 b) (b 1 (+ a b)) (n 0 (+ n 1))) ((= n 6) a) ())";
    assert_eq!(eval_ok(src), Value::Int(8));
}
