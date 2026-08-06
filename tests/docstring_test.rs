//! Tests for the CL-equivalent docstring mechanism: a docstring attached to
//! `defun`/`defmethod`/`defmacro`/`defvar`/`defconstant`/`defstruct`/
//! `defenum`/`deftrait`, and the `documentation` special form that reads it
//! back — resolved entirely at check time (`Checker::check_documentation`)
//! into a constant `Option<Str>`, so these tests exercise it the same way
//! any other expression is evaluated.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue};

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
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

/// The text of a `string` result.
///
/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — and `run` above drops its heap on return.
/// Hence this parallel runner, which reads the text out first.
fn eval_string(src: &str) -> String {
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
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        RtValue::Sexpr(typelisp::Value::Str(id)) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// Type-checks `src` (prelude loaded, so `documentation`/`unwrap-or`/etc. are
/// in scope) and returns the `TypeError` message from the first form that
/// fails to check.
fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut result = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    match result.map_err(Error::into_kind) {
        Err(Error::TypeError(msg)) => msg,
        other => panic!("expected a TypeError, got {:?}", other),
    }
}

// ---- defun ------------------------------------------------------------------

#[test]
fn defun_docstring_is_returned_by_documentation() {
    let src = r#"
        (defun add ((x i32) (y i32)) i32
          "Adds two integers."
          (+ x y))
        (unwrap-or (documentation add) "none")
    "#;
    assert_eq!(eval_string(src), "Adds two integers.");
}

#[test]
fn defun_without_docstring_has_no_documentation() {
    let src = r#"
        (defun add ((x i32) (y i32)) i32 (+ x y))
        (is-none (documentation add))
    "#;
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn defun_single_string_body_is_the_return_value_not_a_docstring() {
    // A lone string with nothing after it is CL's ambiguous case: it must
    // stay the return value, not be swallowed as documentation.
    let src = r#"(defun greeting () string "just a value") (greeting)"#;
    assert_eq!(eval_string(src), "just a value");
    let src2 = r#"
        (defun greeting () string "just a value")
        (is-none (documentation greeting))
    "#;
    assert_eq!(eval_ok(src2), RtValue::Bool(true));
}

#[test]
fn defun_opt_key_docstring_is_returned_by_documentation() {
    let src = r#"
        (defun greet ((name string) &key (loud bool false)) string
          "Greets someone, optionally loudly."
          name)
        (unwrap-or (documentation greet) "none")
    "#;
    assert_eq!(eval_string(src), "Greets someone, optionally loudly.");
}

// ---- defmacro -----------------------------------------------------------------

#[test]
fn defmacro_docstring_is_returned_by_documentation() {
    let src = r#"
        (defmacro my-when (test then)
          "A minimal `when`."
          (list 'if test then '()))
        (unwrap-or (documentation my-when) "none")
    "#;
    assert_eq!(eval_string(src), "A minimal `when`.");
}

// ---- defvar / defconstant (trailing docstring) -------------------------------

#[test]
fn defvar_trailing_docstring_is_returned_by_documentation() {
    let src = r#"
        (defvar (limit i32) 100 "The maximum allowed count.")
        (unwrap-or (documentation limit) "none")
    "#;
    assert_eq!(eval_string(src), "The maximum allowed count.");
}

#[test]
fn defconstant_trailing_docstring_is_returned_by_documentation() {
    let src = r#"
        (defconstant (pi-ish f64) 3.14 "An approximation of pi.")
        (unwrap-or (documentation pi-ish) "none")
    "#;
    assert_eq!(eval_string(src), "An approximation of pi.");
}

#[test]
fn defvar_without_docstring_still_checks_normally() {
    let src = r#"
        (defvar (limit i32) 100)
        limit
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(100));
}

#[test]
fn defvar_third_argument_must_be_a_string() {
    let msg = check_err("(defvar (limit i32) 100 42)");
    assert!(msg.contains("docstring"), "unexpected message: {}", msg);
}

// ---- defstruct / defenum (leading docstring) ---------------------------------

#[test]
fn defstruct_docstring_is_returned_by_documentation() {
    let src = r#"
        (defstruct point
          "A 2D point."
          (x i32)
          (y i32))
        (unwrap-or (documentation point) "none")
    "#;
    assert_eq!(eval_string(src), "A 2D point.");
}

#[test]
fn defenum_docstring_is_returned_by_documentation() {
    let src = r#"
        (defenum color
          "Primary colors."
          (red)
          (green)
          (blue))
        (unwrap-or (documentation color) "none")
    "#;
    assert_eq!(eval_string(src), "Primary colors.");
}

#[test]
fn defstruct_without_docstring_has_no_documentation() {
    let src = r#"
        (defstruct point (x i32) (y i32))
        (is-none (documentation point))
    "#;
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

// ---- deftrait / defmethod (impl) ---------------------------------------------

#[test]
fn deftrait_docstring_is_returned_by_documentation() {
    let src = r#"
        (deftrait describable ()
          "Types that can describe themselves."
          (describe ((self Self)) string))
        (unwrap-or (documentation describable) "none")
    "#;
    assert_eq!(eval_string(src), "Types that can describe themselves.");
}

#[test]
fn defmethod_docstring_via_impl_is_returned_by_documentation() {
    let src = r#"
        (defstruct point (x i32) (y i32))
        (deftrait describable ()
          (describe ((self Self)) string))
        (impl describable point
          (describe ((self point)) string
            "Describes this point as a string."
            "a point"))
        (unwrap-or (documentation point::describe) "none")
    "#;
    assert_eq!(eval_string(src), "Describes this point as a string.");
}

#[test]
fn defmethod_docstring_direct_is_returned_by_documentation() {
    let src = r#"
        (defstruct point (x i32) (y i32))
        (defmethod magnitude (point) i32
          "Returns a magnitude-ish value."
          0)
        (unwrap-or (documentation point::magnitude) "none")
    "#;
    assert_eq!(eval_string(src), "Returns a magnitude-ish value.");
}

// ---- documentation error cases ------------------------------------------------

#[test]
fn documentation_on_unbound_name_is_a_check_error() {
    let msg = check_err("(documentation totally-undefined-name)");
    assert!(msg.contains("documentation"), "unexpected message: {}", msg);
}
