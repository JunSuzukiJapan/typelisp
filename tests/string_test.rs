//! Tests for the built-in `String`/`char` instance methods
//! ([cl-equivalence-catalog.md](../docs/dev/cl-equivalence-catalog.md) §2.2 d),
//! reusing the primitive-type `defmethod` receiver support from step 1
//! (`check_instance_method`'s `prim_type_path` dispatch) — these are
//! metadata-only `AssocFn`s (`registry::string_assoc`/`char_assoc`) with no
//! `defmethod` body, falling back to `eval_builtin_method` at runtime, the
//! same pattern as `HashTable`.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

/// Like [`run`], but with the prelude loaded first — needed for `and` (a
/// `defmacro` in `src/prelude.rs`, not a checker-native special form).
fn run_with_prelude(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok_with_prelude(src: &str) -> RtValue {
    run_with_prelude(src).expect("eval failed")
}

/// The text of a `string` result. A `string` is a heap `Value::Str` since the
/// scalar unification, so reading one needs the heap it lives in — and [`run`]
/// drops its heap on return, hence this parallel runner.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        RtValue::Sexpr(typelisp::Value::Str(id)) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

fn type_error(src: &str) {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut result: Result<_, Error> = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    assert!(result.is_err(), "expected a type error");
}

// ---- String -----------------------------------------------------------------

#[test]
fn string_upcase() {
    assert_eq!(eval_string(r#"(upcase "abc")"#), "ABC");
}

#[test]
fn string_downcase() {
    assert_eq!(eval_string(r#"(downcase "ABC")"#), "abc");
}

#[test]
fn string_length() {
    assert_eq!(eval_ok(r#"(length "hello")"#), RtValue::Int(5));
}

#[test]
fn string_ref_returns_the_char_at_an_index() {
    assert_eq!(eval_ok(r#"(ref "hello" 1)"#), RtValue::Char('e'));
}

#[test]
fn string_ref_out_of_range_panics() {
    assert!(matches!(run(r#"(ref "hi" 5)"#), Err(EvalError::Panic(_))));
}

#[test]
fn string_ref_negative_index_panics() {
    assert!(matches!(run(r#"(ref "hi" -1)"#), Err(EvalError::Panic(_))));
}

#[test]
fn string_substring_returns_the_given_range() {
    assert_eq!(eval_string(r#"(substring "hello" 1 4)"#), "ell");
}

#[test]
fn string_substring_empty_range_is_the_empty_string() {
    assert_eq!(eval_string(r#"(substring "hello" 2 2)"#), "");
}

#[test]
fn string_substring_end_past_length_panics() {
    assert!(matches!(run(r#"(substring "hi" 0 5)"#), Err(EvalError::Panic(_))));
}

#[test]
fn string_substring_start_after_end_panics() {
    assert!(matches!(run(r#"(substring "hello" 3 1)"#), Err(EvalError::Panic(_))));
}

#[test]
fn string_append_concatenates() {
    assert_eq!(eval_string(r#"(append "foo" "bar")"#), "foobar");
}

#[test]
fn string_eq_is_identity_not_value_equality() {
    // `eq` is real CL identity (see `string_identity_eq` and
    // `docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp section) — two
    // separately-evaluated literals with equal content are not `eq`.
    // Content comparison is `equal`/`equalp` instead (below).
    assert_eq!(eval_ok(r#"(eq "abc" "abc")"#), RtValue::Bool(false));
    // Nor are two separately-built strings with equal content.
    assert_eq!(eval_ok(r#"(eq (append "a" "b") (append "a" "b"))"#), RtValue::Bool(false));
}

/// The positive half of [`string_eq_is_identity_not_value_equality`]: a
/// string threaded through a binding, a call, a global, a `cons` cell, or a
/// struct field is still the *same* string.
///
/// These are the cases the scalar unification had to preserve when `string`
/// moved from a Rust-side `Rc<str>` to a heap `Value::Str`, and the last two
/// are cases it *fixed*: the old encoding copied the text onto the heap when
/// building a `Sexpr::Str` node or storing a struct field, so `eq` came back
/// false where CL says a cons cell and a slot store the object itself. Only
/// the first three were previously true.
#[test]
fn string_eq_survives_being_stored_and_read_back() {
    assert_eq!(eval_ok(r#"(let ((s "abc")) (eq s s))"#), RtValue::Bool(true));
    assert_eq!(
        eval_ok(r#"(defun id ((s string)) string s) (let ((s "abc")) (eq s (id s)))"#),
        RtValue::Bool(true),
        "through a call"
    );
    assert_eq!(eval_ok(r#"(defvar (*s* string) "abc") (eq *s* *s*)"#), RtValue::Bool(true), "through a global");
    assert_eq!(
        eval_ok(
            r#"(let ((s "abc")) (let ((c (sexpr-cons (Str s) (quote ())))) (eq s (sexpr-str (sexpr-car c)))))"#
        ),
        RtValue::Bool(true),
        "through a cons cell"
    );
    assert_eq!(
        eval_ok(r#"(defstruct bx (s string)) (let ((s "abc")) (let ((b (bx::new s))) (eq s b::s)))"#),
        RtValue::Bool(true),
        "through a struct field"
    );
}

#[test]
fn string_equal_compares_value_equality() {
    assert_eq!(eval_ok(r#"(equal "abc" "abc")"#), RtValue::Bool(true));
    assert_eq!(eval_ok(r#"(equal "abc" "abd")"#), RtValue::Bool(false));
}

#[test]
fn string_equalp_ignores_ascii_case() {
    assert_eq!(eval_ok(r#"(equalp "ABC" "abc")"#), RtValue::Bool(true));
    assert_eq!(eval_ok(r#"(equalp "abc" "abd")"#), RtValue::Bool(false));
}

#[test]
fn string_lt_compares_lexicographically() {
    assert_eq!(eval_ok(r#"(lt "abc" "abd")"#), RtValue::Bool(true));
    assert_eq!(eval_ok(r#"(lt "abd" "abc")"#), RtValue::Bool(false));
}

#[test]
fn string_method_on_wrong_type_is_a_type_error() {
    type_error("(upcase 5)");
}

// ---- char ---------------------------------------------------------------------

#[test]
fn char_upcase() {
    assert_eq!(eval_ok(r"(upcase #\a)"), RtValue::Char('A'));
}

#[test]
fn char_downcase() {
    assert_eq!(eval_ok(r"(downcase #\A)"), RtValue::Char('a'));
}

#[test]
fn char_eq_compares_value_equality() {
    assert_eq!(eval_ok(r"(eq #\a #\a)"), RtValue::Bool(true));
    assert_eq!(eval_ok(r"(eq #\a #\b)"), RtValue::Bool(false));
}

#[test]
fn char_lt_compares_by_code_point() {
    assert_eq!(eval_ok(r"(lt #\a #\b)"), RtValue::Bool(true));
    assert_eq!(eval_ok(r"(lt #\b #\a)"), RtValue::Bool(false));
}

#[test]
fn char_alpha_is_true_for_letters() {
    assert_eq!(eval_ok(r"(alphap #\a)"), RtValue::Bool(true));
}

#[test]
fn char_alpha_is_false_for_digits() {
    assert_eq!(eval_ok(r"(alphap #\5)"), RtValue::Bool(false));
}

#[test]
fn char_digit_is_true_for_digits() {
    assert_eq!(eval_ok(r"(digitp #\5)"), RtValue::Bool(true));
}

#[test]
fn char_digit_is_false_for_letters() {
    assert_eq!(eval_ok(r"(digitp #\a)"), RtValue::Bool(false));
}

#[test]
fn char_only_method_on_a_string_is_a_type_error() {
    // `alphap` exists only in `char_assoc`, not `string_assoc` — unlike
    // `upcase`/`downcase`/`eq`/`lt`, which both tables define, so a `Str`
    // receiver can't fall back to it.
    type_error(r#"(alphap "a")"#);
}

// ---- dispatch does not cross-contaminate between string and char --------------

#[test]
fn eq_dispatches_separately_per_receiver_type() {
    // `string`'s and `char`'s comparison methods live in separate per-type
    // `assoc` tables (see `check_instance_method`'s dispatch on the
    // receiver's static type) — calling both in the same program is a
    // regression check that they don't collide. Uses `equal` for the
    // string half (`eq` on `Str` is identity, not content — see
    // `string_eq_is_identity_not_value_equality`).
    let src = r#"(defun f () bool (and (equal "x" "x") (eq #\x #\x))) (f)"#;
    assert_eq!(eval_ok_with_prelude(src), RtValue::Bool(true));
}
