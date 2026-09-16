//! Tests for error-handling primitives: `Result<T,E>`, the built-in concrete
//! error types (`ParseIntError`/`ParseFloatError`/`ReadError`/`EvalError`)
//! and the prelude `Error` trait they implement, user-defined error types,
//! the `Never` (`!`) type, and the `panic` special form.

extern crate typelisp;
use std::cell::RefCell;
use typelisp::check::core;
use typelisp::{load_prelude, Checker, Error, Heap, Interp, Path, Reader, Type, Value};

/// Check a sequence of forms; return the last one's tag and, for an `expr`
/// form, the type the checker proved for it. A tag rather than the form itself:
/// a checked form is cons cells in this helper's own `Heap`, which dies with the
/// call.
fn program(src: &str) -> Result<(String, Option<Type>), Error> {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut tag = String::new();
    for v in vs {
        // Strip any source-location wrapper so kind-based assertions
        // (`Err(Error::TypeError(_))`) still match.
        let tl = chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?;
        tag = core::op(&h, tl).expect("every top-level form is a tagged list").to_string();
    }
    Ok((tag, chk.expr_type().cloned()))
}

fn ty(src: &str) -> Type {
    let (tag, ty) = program(src).expect("check failed");
    ty.unwrap_or_else(|| panic!("expected expression, got a `{}` form", tag))
}

/// The return type the checker recorded for the root-level function `name` in
/// `src`. Read from the registry, the declaration's own home — the core form
/// carries only the return's *representation* (`check::repr::Repr`), which is
/// deliberately coarser than its type.
fn ret_of(src: &str, name: &str) -> Type {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    for v in vs {
        chk.check_form(&mut h, &interp, v).expect("check failed");
    }
    chk.registry()
        .fn_sig(&Path::root(name))
        .unwrap_or_else(|| panic!("`{}` was not registered", name))
        .ret
        .clone()
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
    assert_eq!(ty("(if true 1 (panic \"x\"))"), Type::Int);
    assert_eq!(ty("(if true (panic \"x\") 2)"), Type::Int);
}

#[test]
fn defun_branch_may_panic() {
    let src = "(defun f ((b bool) (x int)) int (if b (panic \"neg\") x))";
    assert_eq!(program(src).expect("check failed").0, "defun");
}

#[test]
fn parses_never_type_annotation() {
    // A function whose body always diverges has return type `!`.
    let src = "(defun boom () ! (panic \"always\"))";
    assert_eq!(ret_of(src, "boom"), Type::Never);
}

// ---- Result / Error ---------------------------------------------------------

#[test]
fn result_ok_infers_from_return_type() {
    let src = "(defun mk () Result<int,ParseIntError> (result::ok 1))";
    assert_eq!(ret_of(src, "mk"), Type::Named(Path::root("result"), vec![Type::Int, error_ty()]));
}

#[test]
fn result_err_takes_error_value() {
    let src = "(defun bad () Result<int,ParseIntError> (result::err (ParseIntError::ParseIntError \"boom\")))";
    assert_eq!(program(src).expect("check failed").0, "defun");
}

#[test]
fn match_result_exhaustive_with_panic_arm() {
    // Unwrapping a Result: the Err arm diverges, so the match has type int.
    let src = "(defun unwrap-i ((r Result<int,ParseIntError>)) int \
                 (match r ((Ok v) v) ((Err e) (panic \"unwrap on Err\"))))";
    assert_eq!(ret_of(src, "unwrap-i"), Type::Int);
}

#[test]
fn match_result_non_exhaustive_rejected() {
    let src = "(defun f ((r Result<int,ParseIntError>)) int (match r ((Ok v) v)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

#[test]
fn match_result_arms_must_agree() {
    let src = "(defun f ((r Result<int,ParseIntError>)) int \
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
fn run(src: &str) -> Result<Value, Error> {
    with_ctx(|h, chk, interp| {
        let r = Reader::new();
        let vs = r.read_all(h, src).expect("read failed");
        let mut last = Value::Empty;
        for v in vs {
            let tl = chk.check_form(h, &*interp, v).map_err(Error::into_kind)?;
            if let Some(val) = interp.exec(h, tl).map_err(|e| Error::TypeError(e.to_string()))? {
                last = val;
            }
        }
        Ok(last)
    })
}

/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — the same thread-local `CTX` the
/// evaluation used, so call sites stay unchanged.
fn run_str(src: &str) -> String {
    let v = run(src).expect("eval failed");
    CTX.with(|cell| {
        let opt = cell.borrow();
        let (h, _, _) = opt.as_ref().expect("no evaluation has run yet");
        match v {
            typelisp::Value::Str(id) => h.string(id).to_string(),
            other => panic!("expected a string, got {:?}", other),
        }
    })
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
        (defun int-msg ((r Result<int,ParseIntError>)) string
          (match r ((ok _) "?") ((err e) (message e))))
        (defun read-msg ((r Result<Option<Sexpr>,ReadError>)) string
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
        (defun open-it ((p string)) Result<int,IoErr>
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
    // the *same* `Result<int, :dyn Error>` type.
    let src = r#"
        (defstruct AppErr (why string))
        (impl Error AppErr
          (message ((self Self)) string self::why)
          (source ((self Self)) Option<:dyn Error> (option::none)))
        (defun fail-app () Result<int,AppErr> (result::err (AppErr::new "app said no")))
        (defun describe ((r Result<int, :dyn Error>)) string
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
    assert_prelude_type_error("(defun f () Result<int,Error> (result::ok 1))");
    assert_prelude_type_error("(defun g ((e Error)) int 1)");
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
    assert_type_error("(defstruct Thing (x int)) (deftrait Thing () (m ((self Self)) int))");
    assert_type_error("(deftrait Gadget () (m ((self Self)) int)) (defstruct Gadget (x int))");
    assert_type_error("(deftrait Widget () (m ((self Self)) int)) (defenum Widget (a))");
}

#[test]
fn a_type_and_a_trait_of_the_same_name_may_live_in_different_modules() {
    // The rule is per-namespace, like every other name in this language.
    let src = "(module a (deftrait Same () (m ((self Self)) int))) \
               (module b (defstruct Same (x int)))";
    assert!(program(src).is_ok());
}
