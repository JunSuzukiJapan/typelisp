//! Tests for error-handling primitives: `Result<T,E>`, the built-in concrete
//! error types (`ParseIntError`/`ParseFloatError`/`ReadError`/`EvalError`)
//! and the prelude `Error` trait they implement, user-defined error types,
//! the `Never` (`!`) type, and the `panic` special form.

extern crate typelisp;
use std::cell::RefCell;
use typelisp::{load_prelude, Checker, Error, Heap, Interp, Path, Reader, RtValue, TopLevel, Type};

fn program(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = None;
    for v in vs {
        // Strip any source-location wrapper so kind-based assertions
        // (`Err(Error::TypeError(_))`) still match.
        last = Some(chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?);
    }
    Ok(last.expect("no forms"))
}

fn ty(src: &str) -> Type {
    match program(src).expect("check failed") {
        TopLevel::Expr(t) => t.ty,
        other => panic!("expected expression, got {:?}", other),
    }
}

fn assert_type_error(src: &str) {
    match program(src) {
        Err(Error::TypeError(_)) => {}
        other => panic!("expected TypeError, got {:?}", other),
    }
}

// ---- Never / panic ---------------------------------------------------------

#[test]
fn panic_has_never_type() {
    assert_eq!(ty("(panic \"boom\")"), Type::Never);
}

#[test]
fn panic_message_must_be_string() {
    assert_type_error("(panic 42)");
}

#[test]
fn if_branch_may_panic() {
    // The else branch diverges; the if still has the then branch's type.
    assert_eq!(ty("(if true 1 (panic \"x\"))"), Type::I32);
    assert_eq!(ty("(if true (panic \"x\") 2)"), Type::I32);
}

#[test]
fn defun_branch_may_panic() {
    let src = "(defun f ((b bool) (x i32)) i32 (if b (panic \"neg\") x))";
    assert!(matches!(program(src), Ok(TopLevel::Defun { .. })));
}

#[test]
fn parses_never_type_annotation() {
    // A function whose body always diverges has return type `!`.
    let src = "(defun boom () ! (panic \"always\"))";
    match program(src).unwrap() {
        TopLevel::Defun { ret, .. } => assert_eq!(ret, Type::Never),
        other => panic!("expected defun, got {:?}", other),
    }
}

// ---- Result / Error ---------------------------------------------------------

#[test]
fn result_ok_infers_from_return_type() {
    let src = "(defun mk () Result<i32,ParseIntError> (result::ok 1))";
    match program(src).unwrap() {
        TopLevel::Defun { ret, .. } => {
            assert_eq!(ret, Type::Named(Path::root("result"), vec![Type::I32, error_ty()]));
        }
        other => panic!("expected defun, got {:?}", other),
    }
}

#[test]
fn result_err_takes_error_value() {
    let src = "(defun bad () Result<i32,ParseIntError> (result::err (ParseIntError::ParseIntError \"boom\")))";
    assert!(matches!(program(src), Ok(TopLevel::Defun { .. })));
}

#[test]
fn match_result_exhaustive_with_panic_arm() {
    // Unwrapping a Result: the Err arm diverges, so the match has type i32.
    let src = "(defun unwrap-i ((r Result<i32,ParseIntError>)) i32 \
                 (match r ((Ok v) v) ((Err e) (panic \"unwrap on Err\"))))";
    match program(src).unwrap() {
        TopLevel::Defun { ret, .. } => assert_eq!(ret, Type::I32),
        other => panic!("expected defun, got {:?}", other),
    }
}

#[test]
fn match_result_non_exhaustive_rejected() {
    let src = "(defun f ((r Result<i32,ParseIntError>)) i32 (match r ((Ok v) v)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

#[test]
fn match_result_arms_must_agree() {
    let src = "(defun f ((r Result<i32,ParseIntError>)) i32 \
                 (match r ((Ok v) v) ((Err e) true)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

fn error_ty() -> Type {
    Type::Named(Path::root("parseinterror"), vec![])
}

// ---- the `Error` trait / user-defined error types (TODO T3) -----------------
//
// These need the prelude (that is where the `Error` trait, the built-in
// types' impls, and `as-dyn-error` live), so they run against a shared
// prelude-loaded context rather than `program`'s bare `Checker`, the same
// arrangement `prelude_test.rs` uses and for the same reason (loading the
// prelude per test dominates `miri` runtime).

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

/// Check and run `src` against the prelude-loaded context, returning the last
/// value produced.
fn run(src: &str) -> Result<RtValue, Error> {
    with_ctx(|h, chk, interp| {
        let r = Reader::new();
        let vs = r.read_all(h, src).expect("read failed");
        let mut last = RtValue::Unit;
        for v in vs {
            let tl = chk.check_form(h, &*interp, v).map_err(Error::into_kind)?;
            if let Some(val) = interp.exec(h, tl).map_err(|e| Error::TypeError(e.to_string()))? {
                last = val;
            }
        }
        Ok(last)
    })
}

fn run_str(src: &str) -> String {
    match run(src).expect("eval failed") {
        RtValue::Str(s) => s.to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

fn assert_prelude_type_error(src: &str) {
    match run(src) {
        Err(Error::TypeError(_)) => {}
        other => panic!("expected TypeError, got {:?}", other),
    }
}

#[test]
fn builtin_errors_implement_the_error_trait() {
    // Each fallible builtin returns its *own* concrete error type, and each
    // one implements `Error` — so the message is read the same way for all.
    let src = r#"
        (defun int-msg ((r Result<i32,ParseIntError>)) string
          (match r ((ok _) "?") ((err e) (message e))))
        (defun read-msg ((r Result<Sexpr,ReadError>)) string
          (match r ((ok _) "?") ((err e) (message e))))
        (list (int-msg (parse-int "zz")) (read-msg (read "(")))
        (int-msg (parse-int "zz"))
    "#;
    assert!(run_str(src).starts_with("parse-int: invalid integer literal"));
}

#[test]
fn a_builtin_error_boxes_into_dyn_error() {
    let src = r#"
        (defun describe ((e :dyn Error)) string (message e))
        (describe (ParseFloatError::ParseFloatError "bad float"))
    "#;
    assert_eq!(run_str(src), "bad float");
}

#[test]
fn a_builtin_error_has_no_source() {
    // Nothing wraps a built-in error, so it is always the root cause.
    let src = r#"
        (defun has-source ((e :dyn Error)) string
          (match (source e) ((some _) "some") ((none) "none")))
        (has-source (EvalError::EvalError "boom"))
    "#;
    assert_eq!(run_str(src), "none");
}

#[test]
fn a_defstruct_error_type_rides_in_result() {
    // A user type as `E` needs nothing from the language beyond `Result`'s
    // own generic parameter — no `Error` impl required for this much.
    let src = r#"
        (defstruct IoErr (path string))
        (defun open-it ((p string)) Result<i32,IoErr>
          (if (equal p "") (result::err (IoErr::new "<empty>")) (result::ok 3)))
        (match (open-it "") ((ok _) "?") ((err e) e::path))
    "#;
    assert_eq!(run_str(src), "<empty>");
}

#[test]
fn a_defenum_error_type_implements_the_error_trait() {
    let src = r#"
        (defenum ConfigErr (missing-key string) (bad-value string))
        (impl Error ConfigErr
          (message ((self Self)) string
            (match self ((missing-key k) (append "missing key: " k))
                        ((bad-value v) (append "bad value: " v))))
          (source ((self Self)) Option<:dyn Error> (option::none)))
        (defun describe ((e :dyn Error)) string (message e))
        (describe (ConfigErr::missing-key "port"))
    "#;
    assert_eq!(run_str(src), "missing key: port");
}

#[test]
fn source_returns_the_wrapped_error() {
    // Rust's `Error::source`: the chain is walked through trait objects, so
    // the wrapper needs no static knowledge of what it wraps.
    let src = r#"
        (defstruct LowErr (detail string))
        (impl Error LowErr
          (message ((self Self)) string self::detail)
          (source ((self Self)) Option<:dyn Error> (option::none)))
        (defstruct HighErr (cause LowErr))
        (impl Error HighErr
          (message ((self Self)) string "high-level failure")
          (source ((self Self)) Option<:dyn Error> (option::some self::cause)))
        (defun root-message ((e :dyn Error)) string
          (match (source e) ((some inner) (message inner)) ((none) (message e))))
        (root-message (HighErr::new (LowErr::new "disk offline")))
    "#;
    assert_eq!(run_str(src), "disk offline");
}

#[test]
fn as_dyn_error_unifies_builtin_and_user_error_types() {
    // The prelude's `as-dyn-error` widens any `Result<T,E>` whose `E`
    // implements `Error` — the point being that both results below end up in
    // the *same* `Result<i32, :dyn Error>` type.
    let src = r#"
        (defstruct AppErr (why string))
        (impl Error AppErr
          (message ((self Self)) string self::why)
          (source ((self Self)) Option<:dyn Error> (option::none)))
        (defun fail-app () Result<i32,AppErr> (result::err (AppErr::new "app said no")))
        (defun describe ((r Result<i32, :dyn Error>)) string
          (match r ((ok _) "?") ((err e) (message e))))
        (append (describe (as-dyn-error (fail-app)))
                (if (is-err (as-dyn-error (parse-int "zz"))) " / builtin too" ""))
    "#;
    assert_eq!(run_str(src), "app said no / builtin too");
}

#[test]
fn a_trait_written_in_type_position_is_rejected() {
    // `Error` is a trait, never a type: the old `Result<T, Error>` spelling
    // must say so rather than silently passing as an unresolved name.
    assert_prelude_type_error("(defun f () Result<i32,Error> (result::ok 1))");
    assert_prelude_type_error("(defun g ((e Error)) i32 1)");
}

#[test]
fn a_type_may_not_take_a_traits_name() {
    // Types and traits share one name space per module (Rust's rule), which
    // is why the built-in error types could not also be called `Error`.
    assert_prelude_type_error("(defstruct Error (msg string))");
    assert_prelude_type_error("(defenum Error (boom string))");
}

#[test]
fn a_trait_may_not_take_a_types_name() {
    // The other direction, on a bare checker so the names are free to start
    // with: whichever is defined second is the one rejected.
    assert_type_error("(defstruct Thing (x i32)) (deftrait Thing () (m ((self Self)) i32))");
    assert_type_error("(deftrait Gadget () (m ((self Self)) i32)) (defstruct Gadget (x i32))");
    assert_type_error("(deftrait Widget () (m ((self Self)) i32)) (defenum Widget (a))");
}

#[test]
fn a_type_and_a_trait_of_the_same_name_may_live_in_different_modules() {
    // The rule is per-namespace, like every other name in this language.
    let src = "(module a (deftrait Same () (m ((self Self)) i32))) \
               (module b (defstruct Same (x i32)))";
    assert!(program(src).is_ok());
}
