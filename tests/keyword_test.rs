//! Tests for CL-style keywords (`:name`): self-evaluating symbols of the
//! keyword namespace.
//!
//! A keyword is a symbol owned by the root's module `keyword` (CL's
//! `KEYWORD` package), whichever module wrote it, so "same name -> same
//! object" holds across modules too. Its name has no colon — the colon spells
//! it — and being a keyword is being owned by that module, which `keywordp`
//! asks. The checker turns one into a symbol literal before any binding
//! lookup, refuses one wherever a name would be bound or defined, and the
//! reader rejects malformed spellings.

extern crate typelisp;

mod common;
use common::{eval_string_compiled as eval_string};
use typelisp::{load_compiler, load_prelude, Checker, Error, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| format!("{:?}", e))?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| format!("{:?}", e))?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| format!("{:?}", e))? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
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
    // As in CL, the colon spells the keyword and is no part of its name.
    assert_eq!(eval_string("(symbol->string :foo)"), "foo");
}

#[test]
fn a_keyword_is_typed_as_symbol_and_flows_where_a_symbol_is_expected() {
    let out = eval_string("(defun name-of ((s symbol)) string (symbol->string s)) (name-of :hello)");
    assert_eq!(out, "hello");
}

#[test]
fn the_same_keyword_read_twice_is_the_same_object() {
    // Interning, not structural equality: `eq` is identity.
    assert_eq!(eval_ok("(eq :foo :foo)"), Value::Bool(true));
    assert_eq!(eval_ok("(eq :foo :bar)"), Value::Bool(false));
}

#[test]
fn keywords_are_case_sensitive_like_every_other_symbol() {
    assert_eq!(eval_ok("(eq :foo :FOO)"), Value::Bool(false));
}

#[test]
fn a_keyword_can_be_stored_in_a_sexpr_datum() {
    // The `Symbol -> Sexpr` transparent retype already in `check_inner`
    // covers this — a keyword needs no wrapping constructor.
    let out = eval_string("(match (sexpr-car (list :a :b)) ((sym s) (symbol->string s)) (_ \"not a symbol\"))");
    assert_eq!(out, "a");
}

// ---- keywordp -----------------------------------------------------------

#[test]
fn keywordp_distinguishes_keywords_from_ordinary_symbols() {
    assert_eq!(eval_ok("(keywordp :foo)"), Value::Bool(true));
    assert_eq!(eval_ok("(keywordp (intern \"foo\"))"), Value::Bool(false));
    assert_eq!(eval_ok("(keywordp (intern \"\"))"), Value::Bool(false));
}

// ---- one namespace, CL's names ------------------------------------------

#[test]
fn a_keyword_written_in_a_module_is_the_roots_keyword() {
    let out = eval_ok("(module m (pub defun k () symbol :zork)) (eq (m::k) :zork)");
    assert_eq!(out, Value::Bool(true));
}

#[test]
fn intern_into_the_keyword_namespace_makes_the_keyword() {
    assert_eq!(eval_ok("(eq (intern \"foo\" \"keyword\") :foo)"), Value::Bool(true));
    assert_eq!(eval_ok("(keywordp (intern \"foo\" \"keyword\"))"), Value::Bool(true));
}

#[test]
fn a_name_starting_with_a_colon_does_not_make_a_keyword() {
    // CL's `(intern ":foo")`: an ordinary symbol whose name has a colon.
    assert_eq!(eval_ok("(keywordp (intern \":foo\"))"), Value::Bool(false));
    assert_eq!(eval_ok("(eq (intern \":foo\") :foo)"), Value::Bool(false));
    assert_eq!(eval_ok("(eq (quote |:foo|) (intern \":foo\"))"), Value::Bool(true));
}

#[test]
fn intern_without_a_namespace_interns_where_the_call_is_written() {
    let out = eval_ok("(module m (pub defun same () bool (eq (intern \"zork\") (quote zork)))) (m::same)");
    assert_eq!(out, Value::Bool(true));
    let out = eval_ok("(module m (pub defun q () symbol (quote zork))) (eq (m::q) (intern \"zork\" \"m\"))");
    assert_eq!(out, Value::Bool(true));
}

#[test]
fn a_keyword_prints_with_its_colon_only_when_escaped() {
    assert_eq!(eval_string("(format false \"~a ~s\" :foo :foo)"), "foo :foo");
    assert_eq!(eval_string("(format false \"~s\" (intern \":foo\"))"), "|:foo|");
}

#[test]
fn a_keyword_pattern_matches_only_that_keyword() {
    assert_eq!(eval_ok("(match :y (:x 1) (_ 0))"), Value::Int(0));
    assert_eq!(eval_ok("(match :x (:x 1) (_ 0))"), Value::Int(1));
}

// ---- a keyword is a constant --------------------------------------------

#[test]
fn a_keyword_cannot_be_bound_assigned_or_defined() {
    for src in [
        "(let ((:foo 1)) 1)",
        "(defun f ((:x int)) int 1)",
        "(defvar (:bar int) 1)",
        "(defun :baz () int 1)",
        "(defmacro :m () 1)",
    ] {
        let e = run(src).expect_err(src);
        assert!(e.contains("is a keyword"), "{}: {}", src, e);
    }
}

#[test]
fn no_module_takes_the_keyword_namespaces_name() {
    let e = run("(module keyword (defun f () int 1))").expect_err("module keyword");
    assert!(e.contains("keyword namespace"), "{}", e);
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
    let out = eval_ok("(pub defun f () int 7) (module m (pub defun g () int (::f))) (m::g)");
    assert_eq!(out, Value::Int(7));
}

// ---- the existing `&key` macro arguments still work ---------------------

#[test]
fn defmacro_key_arguments_still_parse_as_keywords() {
    let out = eval_ok("(defmacro pick (&key (a 1) (b 2)) `(- ,a ,b)) (pick :b 10 :a 30)");
    assert_eq!(out, Value::Int(20));
}
