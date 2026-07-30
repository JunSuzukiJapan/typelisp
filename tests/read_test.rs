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

// ---- error source locations ---------------------------------------------

#[test]
fn read_error_carries_location() {
    let mut h = Heap::with_capacity(64);
    let r = Reader::new();
    // Unmatched `)` on the second line, third column.
    let err = r.read_all_in(&mut h, "foo.typl", "(+ 1 2)\n  )\n").unwrap_err();
    let loc = err.loc().expect("error should carry a location");
    assert_eq!(&*loc.file, "foo.typl");
    assert_eq!(loc.line, 2);
    // The message is prefixed with `file:line:col: `.
    let msg = err.to_string();
    assert!(msg.starts_with("foo.typl:2:"), "unexpected message: {}", msg);
}

#[test]
fn default_file_name_is_input_placeholder() {
    let mut h = Heap::with_capacity(64);
    let r = Reader::new();
    let err = r.read(&mut h, ")").unwrap_err();
    assert_eq!(&*err.loc().expect("location").file, "<input>");
}

// ---- source spans ---------------------------------------------------------

use typelisp::Loc;

/// The four span coordinates as a tuple, for compact assertions.
fn span(loc: &Loc) -> (u32, u32, u32, u32) {
    (loc.line, loc.col, loc.end_line, loc.end_col)
}

/// Read all of `src` and return each top-level datum with its span.
fn read_spanned(src: &str) -> (Heap, Vec<(Value, Loc)>) {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let out = r.read_all_in_spanned(&mut h, "t.typl", src).expect("read failed");
    (h, out)
}

/// The element spans of a list, via the heap's `elem_locs` table.
fn elem_spans(h: &Heap, v: Value) -> Vec<(u32, u32, u32, u32)> {
    h.list_to_vec_locs(v)
        .expect("proper list")
        .iter()
        .map(|(_, loc)| span(loc.as_ref().expect("element should have a location")))
        .collect()
}

#[test]
fn toplevel_bare_atom_has_a_span() {
    let (_h, out) = read_spanned("42");
    assert_eq!(out.len(), 1);
    assert_eq!(span(&out[0].1), (1, 1, 1, 3));
}

#[test]
fn toplevel_spans_skip_leading_whitespace_and_comments() {
    let (_h, out) = read_spanned("  ; comment\n  foo  (bar)\n");
    assert_eq!(out.len(), 2);
    assert_eq!(span(&out[0].1), (2, 3, 2, 6)); // foo
    assert_eq!(span(&out[1].1), (2, 8, 2, 13)); // (bar)
}

#[test]
fn list_cons_loc_spans_open_through_close_paren() {
    let (h, out) = read_spanned("(foo bar)");
    let loc = h.cons_loc(out[0].0).expect("list should have a cons_loc");
    assert_eq!(span(&loc), (1, 1, 1, 10));
}

#[test]
fn list_element_spans_cover_each_element() {
    let (h, out) = read_spanned("(foo bar)");
    assert_eq!(elem_spans(&h, out[0].0), vec![(1, 2, 1, 5), (1, 6, 1, 9)]);
}

#[test]
fn nested_list_records_its_own_span() {
    let (h, out) = read_spanned("(a (b c) d)");
    let outer = out[0].0;
    assert_eq!(span(&h.cons_loc(outer).unwrap()), (1, 1, 1, 12));
    let inner = h.list_to_vec_locs(outer).unwrap()[1].0;
    assert_eq!(span(&h.cons_loc(inner).unwrap()), (1, 4, 1, 9));
    // The nested list's elem_loc (on the outer spine) matches its cons_loc.
    assert_eq!(elem_spans(&h, outer)[1], (1, 4, 1, 9));
}

#[test]
fn multiline_form_span_ends_on_the_closing_line() {
    let (h, out) = read_spanned("(foo\n  bar)");
    assert_eq!(span(&h.cons_loc(out[0].0).unwrap()), (1, 1, 2, 7));
    assert_eq!(elem_spans(&h, out[0].0), vec![(1, 2, 1, 5), (2, 3, 2, 6)]);
}

#[test]
fn string_and_char_atoms_have_full_spans() {
    let (_h, out) = read_spanned("\"hi\" #\\Space");
    assert_eq!(span(&out[0].1), (1, 1, 1, 5));
    assert_eq!(span(&out[1].1), (1, 6, 1, 13));
}

#[test]
fn dotted_pair_span_includes_the_cdr() {
    let (h, out) = read_spanned("(a . b)");
    assert_eq!(span(&h.cons_loc(out[0].0).unwrap()), (1, 1, 1, 8));
}

#[test]
fn quote_synthesized_list_gets_spans() {
    let (h, out) = read_spanned("'x");
    // The whole (quote x) form spans the ' through the datum.
    assert_eq!(span(&out[0].1), (1, 1, 1, 3));
    assert_eq!(span(&h.cons_loc(out[0].0).unwrap()), (1, 1, 1, 3));
    // Elements: `quote` covers the prefix char, `x` its own character.
    assert_eq!(elem_spans(&h, out[0].0), vec![(1, 1, 1, 2), (1, 2, 1, 3)]);
}

#[test]
fn unquote_splicing_spans_both_prefix_chars() {
    let (h, out) = read_spanned("`(,@xs)");
    // (quasiquote ((unquote-splicing xs)))
    let quasi = out[0].0;
    assert_eq!(span(&h.cons_loc(quasi).unwrap()), (1, 1, 1, 8));
    let inner_list = h.list_to_vec_locs(quasi).unwrap()[1].0;
    let splice = h.list_to_vec_locs(inner_list).unwrap()[0].0;
    assert_eq!(span(&h.cons_loc(splice).unwrap()), (1, 3, 1, 7));
    // `unquote-splicing` covers the two prefix chars `,@`.
    assert_eq!(elem_spans(&h, splice), vec![(1, 3, 1, 5), (1, 5, 1, 7)]);
}

// ---- angle-bracket token extension (`:dyn` inside a generic argument) ----

/// A generic argument may itself be a two-word `:dyn Trait` type, so
/// `read_atom` keeps reading past whitespace while `<>` are unbalanced. The
/// scan is speculative: anything that doesn't close on the same line, or that
/// runs into a hard delimiter, rewinds to the ordinary short token.
#[test]
fn a_generic_type_token_may_contain_a_spaced_dyn_argument() {
    roundtrip("vector<:dyn drawable>", "vector<:dyn drawable>");
    roundtrip("hashtable<string, :dyn drawable>", "hashtable<string, :dyn drawable>");
    roundtrip("vector<:dyn iter<i32>>", "vector<:dyn iter<i32>>");
}

#[test]
fn a_leading_angle_bracket_is_still_the_comparison_operator() {
    // `<` starts the token, so it never opens a bracket — otherwise `(< a b)`
    // would swallow the rest of the form.
    roundtrip("(< a b)", "(< a b)");
    roundtrip("(<= a b)", "(<= a b)");
    roundtrip("(> a b)", "(> a b)");
}

#[test]
fn an_operator_name_ending_in_an_angle_bracket_still_reads_as_three_data() {
    // `string<` opens a bracket that never closes; hitting `)` rewinds the
    // speculative scan, leaving today's exact tokenization.
    roundtrip("(string< a b)", "(string< a b)");
    roundtrip("(string<= a b)", "(string<= a b)");
    roundtrip("(a<b c)", "(a<b c)");
}

#[test]
fn an_unbalanced_angle_bracket_rewinds_rather_than_swallowing_the_input() {
    // No closing `>` before end of input / a newline: the token ends at the
    // whitespace, exactly as before.
    roundtrip("(f vector<a)", "(f vector<a)");
    roundtrip("(f vector<\n i32>)", "(f vector< i32>)");
}

// ---- `:dyn Trait` joins into one datum ----------------------------------

#[test]
fn dyn_and_the_following_datum_read_as_one_two_element_list() {
    // So every type position keeps its "a type is one `Value`" assumption:
    // `(x :dyn drawable)` is still a 2-element parameter pair.
    roundtrip(":dyn drawable", "(:dyn drawable)");
    roundtrip("(x :dyn drawable)", "(x (:dyn drawable))");
    roundtrip(":dyn iter<i32>", "(:dyn iter<i32>)");
}

#[test]
fn a_dyn_with_nothing_after_it_is_a_read_error() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    assert!(r.read_all(&mut h, "(f :dyn)").is_err(), "`:dyn` at the end of a list must not read");
}

// ---------------------------------------------------------------------
// `#+`/`#-` reader conditionals (CL-style conditional compilation)
// ---------------------------------------------------------------------

fn read_all_with(features: Vec<&str>, src: &str) -> (Heap, Vec<Value>) {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::with_features(features.into_iter().map(str::to_string));
    let vs = r.read_all(&mut h, src).expect("read failed");
    (h, vs)
}

fn shown(h: &Heap, vs: &[Value]) -> Vec<String> {
    vs.iter().map(|v| show(h, *v)).collect()
}

#[test]
fn plus_feature_conditional_keeps_the_form_when_present() {
    let (h, vs) = read_all_with(vec!["debug"], "(a) #+:debug (b) (c)");
    assert_eq!(shown(&h, &vs), vec!["(a)", "(b)", "(c)"]);
}

#[test]
fn plus_feature_conditional_drops_the_form_when_absent() {
    let (h, vs) = read_all_with(vec![], "(a) #+:debug (b) (c)");
    assert_eq!(shown(&h, &vs), vec!["(a)", "(c)"]);
}

#[test]
fn minus_feature_conditional_inverts_the_test() {
    let (h, vs) = read_all_with(vec!["debug"], "(a) #-:debug (b) (c)");
    assert_eq!(shown(&h, &vs), vec!["(a)", "(c)"]);

    let (h, vs) = read_all_with(vec![], "(a) #-:debug (b) (c)");
    assert_eq!(shown(&h, &vs), vec!["(a)", "(b)", "(c)"]);
}

#[test]
fn feature_expression_supports_and_or_not() {
    let (h, vs) = read_all_with(vec!["a", "b"], "#+(and :a :b) (yes) #+(and :a :missing) (no)");
    assert_eq!(shown(&h, &vs), vec!["(yes)"]);

    let (h, vs) = read_all_with(vec!["a"], "#+(or :missing :a) (yes)");
    assert_eq!(shown(&h, &vs), vec!["(yes)"]);

    let (h, vs) = read_all_with(vec![], "#+(not :missing) (yes)");
    assert_eq!(shown(&h, &vs), vec!["(yes)"]);
}

#[test]
fn feature_conditional_as_a_list_element() {
    let (h, vs) = read_all_with(vec![], "(a b #+:missing c d)");
    assert_eq!(shown(&h, &vs), vec!["(a b d)"]);
}

#[test]
fn feature_conditional_as_the_last_element_before_close_paren() {
    let (h, vs) = read_all_with(vec![], "(a b #+:missing c)");
    assert_eq!(shown(&h, &vs), vec!["(a b)"]);
}

#[test]
fn dropping_a_conditional_form_still_requires_it_to_read_cleanly() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::with_features(Vec::<String>::new());
    // The unbalanced paren inside the discarded form must still surface as a
    // read error — a failed `#+` doesn't degrade to "skip to end of line".
    assert!(r.read_all(&mut h, "#+:missing (a (b) (a").is_err());
}

#[test]
fn host_features_include_the_implementation_name() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r.read_all(&mut h, "#+:typelisp (yes)").unwrap();
    assert_eq!(shown(&h, &vs), vec!["(yes)"]);
}

#[test]
fn feature_names_are_extended_not_replaced() {
    // `with_features` adds to the host defaults rather than overriding them.
    let mut h = Heap::with_capacity(4096);
    let r = Reader::with_features(vec!["my-flag".to_string()]);
    let vs = r.read_all(&mut h, "#+(and :typelisp :my-flag) (yes)").unwrap();
    assert_eq!(shown(&h, &vs), vec!["(yes)"]);
}

#[test]
fn unknown_feature_operator_is_a_read_error() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    assert!(r.read_all(&mut h, "#+(xor :a :b) (form)").is_err());
}

#[test]
fn non_keyword_feature_expression_is_a_read_error() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    assert!(r.read_all(&mut h, "#+not-a-keyword (form)").is_err());
}
