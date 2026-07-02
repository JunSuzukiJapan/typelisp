//! Tests for the Common Lisp-style reader producing `Sexpr` values.

extern crate typelisp;
use typelisp::{Heap, Reader, Value};

// Render a read value back to text, so assertions read naturally.
fn show(h: &Heap, v: Value) -> String {
    match v {
        Value::Empty => "()".to_string(),
        Value::Int(n) => n.to_string(),
        Value::Boxed(id) => h.float_value(id).to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => format!("#\\{}", c),
        Value::Symbol(id) => h.symbol_name(id).to_string(),
        Value::Str(id) => format!("\"{}\"", h.string(id)),
        Value::Path(id) => h
            .path_segments(id)
            .iter()
            .map(|s| h.symbol_name(*s))
            .collect::<Vec<_>>()
            .join("::"),
        Value::Cons(_) => {
            let mut s = String::from("(");
            let mut cur = v;
            let mut first = true;
            loop {
                match cur {
                    Value::Cons(_) => {
                        if !first {
                            s.push(' ');
                        }
                        first = false;
                        s.push_str(&show(h, h.car(cur).unwrap()));
                        cur = h.cdr(cur).unwrap();
                    }
                    Value::Empty => break,
                    other => {
                        s.push_str(" . ");
                        s.push_str(&show(h, other));
                        break;
                    }
                }
            }
            s.push(')');
            s
        }
    }
}

fn read1(src: &str) -> (Heap, Value) {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let v = r.read(&mut h, src).expect("read failed");
    (h, v)
}

fn roundtrip(src: &str, expected: &str) {
    let (h, v) = read1(src);
    assert_eq!(show(&h, v), expected, "reading {:?}", src);
}

// ---- atoms --------------------------------------------------------------

#[test]
fn integers() {
    roundtrip("42", "42");
    roundtrip("-7", "-7");
    roundtrip("+9", "9");
    roundtrip("0x20", "32");
    roundtrip("-0xff", "-255");
}

#[test]
fn floats() {
    roundtrip("3.14", "3.14");
    roundtrip("-0.5", "-0.5");
    roundtrip("1e3", "1000");
    roundtrip(".5", "0.5");
}

#[test]
fn booleans() {
    let (_h, v) = read1("true");
    assert_eq!(v, Value::Bool(true));
    let (_h, v) = read1("FALSE");
    assert_eq!(v, Value::Bool(false));
    let (_h, v) = read1("True");
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn strings_with_escapes() {
    roundtrip("\"hello\"", "\"hello\"");
    let (h, v) = read1("\"a\\nb\\t\\\"c\"");
    match v {
        Value::Str(id) => assert_eq!(h.string(id), "a\nb\t\"c"),
        _ => panic!("expected string"),
    }
}

#[test]
fn characters() {
    let (_h, v) = read1("#\\a");
    assert_eq!(v, Value::Char('a'));
    let (_h, v) = read1("#\\Space");
    assert_eq!(v, Value::Char(' '));
    let (_h, v) = read1("#\\Newline");
    assert_eq!(v, Value::Char('\n'));
    let (_h, v) = read1("#\\(");
    assert_eq!(v, Value::Char('('));
}

#[test]
fn symbols_are_case_insensitive() {
    let (h1, a) = read1("Foo");
    let (h2, b) = read1("FOO");
    assert_eq!(show(&h1, a), "foo");
    assert_eq!(show(&h2, b), "foo");
}

#[test]
fn operators_are_symbols() {
    roundtrip("+", "+");
    roundtrip("<=", "<=");
    roundtrip("1+", "1+");
    roundtrip("Vec<String>", "vec<string>"); // single token, case-folded
}

#[test]
fn double_colon_tokens_become_paths() {
    let (h, v) = read1("std::process::exit");
    match v {
        Value::Path(id) => {
            let segs: Vec<&str> =
                h.path_segments(id).iter().map(|s| h.symbol_name(*s)).collect();
            assert_eq!(segs, ["std", "process", "exit"]);
        }
        _ => panic!("expected Path, got {:?}", v),
    }
    // round-trips to the same text; segments are case-folded
    roundtrip("std::process::exit", "std::process::exit");
    roundtrip("Point::new", "point::new");
    // `::` splits at top level only, so a generic arg stays in the last segment
    roundtrip("geometry::Vec<String>", "geometry::vec<string>");
}

#[test]
fn malformed_paths_are_errors() {
    let mut h = Heap::with_capacity(64);
    let r = Reader::new();
    assert!(r.read(&mut h, "foo::").is_err());
    // leading `::` is now valid (absolute path), so `::bar` must succeed
    assert!(r.read(&mut h, "::bar").is_ok());
    assert!(r.read(&mut h, "a::::b").is_err());
}

// ---- lists --------------------------------------------------------------

#[test]
fn empty_list_is_empty() {
    let (_h, v) = read1("()");
    assert_eq!(v, Value::Empty);
}

#[test]
fn flat_list() {
    roundtrip("(+ 1 2)", "(+ 1 2)");
    roundtrip("(1 2 3 4 5)", "(1 2 3 4 5)");
}

#[test]
fn nested_list() {
    roundtrip(
        "(defun factorial ((n i32)) i32 (if (<= n 1) 1 (* n (factorial (- n 1)))))",
        "(defun factorial ((n i32)) i32 (if (<= n 1) 1 (* n (factorial (- n 1)))))",
    );
}

#[test]
fn empty_list_as_element() {
    // the empty list appears as an element (e.g. a () parameter list)
    roundtrip("(defun f () ())", "(defun f () ())");
    roundtrip("(())", "(())");
}

#[test]
fn dotted_pair() {
    roundtrip("(1 . 2)", "(1 . 2)");
    roundtrip("(1 2 . 3)", "(1 2 . 3)");
}

#[test]
fn dot_inside_token_is_not_a_dotted_pair() {
    roundtrip("(.5)", "(0.5)"); // .5 is a float, not a consing dot
    roundtrip("(a .b)", "(a .b)"); // .b is a symbol
}

#[test]
fn quote_expands_to_quote_form() {
    roundtrip("'x", "(quote x)");
    roundtrip("'(1 2)", "(quote (1 2))");
}

#[test]
fn quasiquote_and_unquote_expand_to_their_forms() {
    roundtrip("`x", "(quasiquote x)");
    roundtrip(",x", "(unquote x)");
    roundtrip("`(a ,b)", "(quasiquote (a (unquote b)))");
}

#[test]
fn unquote_splicing_expands_to_its_form() {
    roundtrip(",@xs", "(unquote-splicing xs)");
    roundtrip("`(a ,@xs)", "(quasiquote (a (unquote-splicing xs)))");
    roundtrip(",@(quote (1 2))", "(unquote-splicing (quote (1 2)))");
}

// ---- comments & whitespace ----------------------------------------------

#[test]
fn line_and_block_comments_are_skipped() {
    roundtrip("(a ; comment\n b)", "(a b)");
    roundtrip("(a #| block |# b)", "(a b)");
    roundtrip("(a #| nested #| inner |# still |# b)", "(a b)");
}

// ---- read_all -----------------------------------------------------------

#[test]
fn read_all_reads_multiple_forms() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let forms = r.read_all(&mut h, "1 (+ 2 3) \"x\"").unwrap();
    assert_eq!(forms.len(), 3);
    assert_eq!(show(&h, forms[0]), "1");
    assert_eq!(show(&h, forms[1]), "(+ 2 3)");
    assert_eq!(show(&h, forms[2]), "\"x\"");
}

#[test]
fn read_all_empty_input_is_empty() {
    let mut h = Heap::with_capacity(16);
    let r = Reader::new();
    let forms = r.read_all(&mut h, "   ; just a comment\n  ").unwrap();
    assert!(forms.is_empty());
}

// ---- reader / GC interaction --------------------------------------------

#[test]
fn reading_survives_gc_triggered_mid_build() {
    // Fill the heap with garbage so a GC fires while the list is being built.
    // Correct rooting in the reader must keep the partial structure alive.
    let cap = 16;
    let mut h = Heap::with_capacity(cap);
    for _ in 0..(cap - 1) {
        let _ = h.cons(Value::Empty, Value::Empty).unwrap(); // unrooted garbage
    }
    assert_eq!(h.free_count(), 1);

    let r = Reader::new();
    // needs more than 1 free cell -> GC must reclaim the garbage mid-read
    let v = r.read(&mut h, "(1 (2 3) \"x\")").unwrap();
    h.push_root(v);
    assert_eq!(show(&h, v), "(1 (2 3) \"x\")");

    // a full GC with the result rooted must preserve it intact
    h.gc();
    assert_eq!(show(&h, v), "(1 (2 3) \"x\")");
}

#[test]
fn moderately_nested_reads_correctly() {
    // The reader is recursive descent, so nesting depth is bounded by the
    // native stack; a few hundred levels is well within range.
    let depth = 500;
    let src = format!("{}{}", "(".repeat(depth), ")".repeat(depth));
    let mut h = Heap::with_capacity(depth + 16);
    let r = Reader::new();
    let v = r.read(&mut h, &src).unwrap();
    h.push_root(v);
    // depth nested lists; the innermost () is Empty, so depth-1 cons cells.
    assert_eq!(h.live_count(), depth - 1);
    h.gc();
    assert_eq!(h.live_count(), depth - 1);
}
