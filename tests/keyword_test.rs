//! Tests for CL-style keywords (`:name`): self-evaluating interned symbols.
//!
//! A keyword is a `Value::Symbol` whose interned name *includes* the leading
//! colon — there is no separate keyword package or table, so "same name ->
//! same object" comes straight from `Heap::intern_symbol` (which also
//! case-folds, hence `:foo` and `:FOO` are one object). The checker turns one
//! into `Expr::SymLit` before any binding lookup (`Checker::check_inner`'s
//! `Value::Symbol` case), and the reader rejects malformed spellings
//! (`read::reader::validate_keyword`).

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Error, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| format!("{:?}", e))?;
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| format!("{:?}", e))?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| format!("{:?}", e))? {
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

fn read_err(src: &str) -> String {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    // Read errors carry a source location (`Error::At`), so unwrap that first.
    match r.read_all(&mut h, src) {
        Err(Error::At(_, inner)) => match *inner {
            Error::ReadError(m) => m,
            other => panic!("expected a ReadError for {:?}, got {:?}", src, other),
        },
        Err(Error::ReadError(m)) => m,
        other => panic!("expected a ReadError for {:?}, got {:?}", src, other),
    }
}

// ---- self-evaluation ----------------------------------------------------

#[test]
fn a_keyword_evaluates_to_itself_without_being_bound() {
    // The point of a keyword: no `defvar`, no `let`, yet it is not an
    // "unbound variable" error. Its static type is the ordinary `symbol`.
    // The interned name keeps the leading colon: typelisp has no package
    // system, so the colon *is* the whole of what makes a keyword a keyword
    // (unlike CL, where `symbol-name` drops the package-marker colon).
    assert_eq!(eval_string("(symbol->string :foo)"), ":foo");
}

#[test]
fn a_keyword_is_typed_as_symbol_and_flows_where_a_symbol_is_expected() {
    let out = eval_string("(defun name-of ((s symbol)) string (symbol->string s)) (name-of :hello)");
    assert_eq!(out, ":hello");
}

#[test]
fn the_same_keyword_read_twice_is_the_same_object() {
    // Interning, not structural equality: `eq` is identity.
    assert_eq!(eval_ok("(eq :foo :foo)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(eq :foo :bar)"), RtValue::Bool(false));
}

#[test]
fn keywords_are_case_folded_like_every_other_symbol() {
    assert_eq!(eval_ok("(eq :foo :FOO)"), RtValue::Bool(true));
}

#[test]
fn a_keyword_can_be_stored_in_a_sexpr_datum() {
    // The `Symbol -> Sexpr` transparent retype already in `check_inner`
    // covers this — a keyword needs no wrapping constructor.
    let out = eval_string("(match (sexpr-car (list :a :b)) ((sym s) (symbol->string s)) (_ \"not a symbol\"))");
    assert_eq!(out, ":a");
}

// ---- keywordp -----------------------------------------------------------

#[test]
fn keywordp_distinguishes_keywords_from_ordinary_symbols() {
    assert_eq!(eval_ok("(keywordp :foo)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(keywordp (string->symbol \"foo\"))"), RtValue::Bool(false));
    assert_eq!(eval_ok("(keywordp (string->symbol \"\"))"), RtValue::Bool(false));
}

// ---- malformed keywords are read errors ---------------------------------

#[test]
fn a_lone_colon_is_not_a_keyword() {
    assert!(read_err("(f :)").contains("alone is not a keyword"), "{}", read_err("(f :)"));
}

#[test]
fn a_keyword_may_not_contain_further_colons() {
    for src in ["(f :foo:bar)", "(f :foo::bar)"] {
        let m = read_err(src);
        assert!(m.contains("may not appear inside a keyword"), "{}: {}", src, m);
    }
}

// ---- the absolute-path syntax `::foo` is not a keyword ------------------

#[test]
fn a_leading_double_colon_is_still_the_absolute_path_syntax() {
    // `::foo` must not be mistaken for a malformed keyword — it reads as a
    // `Value::Path` whose first segment is empty ("from root").
    let out = eval_ok("(defun f () i32 7) (module m (pub defun g () i32 (::f))) (m::g)");
    assert_eq!(out, RtValue::Int(7));
}

// ---- the existing `&key` macro arguments still work ---------------------

#[test]
fn defmacro_key_arguments_still_parse_as_keywords() {
    let out = eval_ok("(defmacro pick (&key (a 1) (b 2)) `(- ,a ,b)) (pick :b 10 :a 30)");
    assert_eq!(out, RtValue::Int(20));
}
