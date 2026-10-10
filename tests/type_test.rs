//! Tests for parsing type expressions from read `Sexpr` values.

extern crate typelisp;
use typelisp::{parse_type, Heap, Path, Reader, Type};

fn parse(src: &str) -> Type {
    parse_result(src).expect("parse_type failed")
}

fn parse_result(src: &str) -> Result<Type, typelisp::Error> {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let v = r.read(&mut h, src).expect("read failed");
    parse_type(&h, v)
}

#[test]
fn primitives() {
    assert_eq!(parse("i32"), Type::I32);
    assert_eq!(parse("int"), Type::Int);
    assert_eq!(parse("i16"), Type::I16);
    assert_eq!(parse("u8"), Type::U8);
    assert_eq!(parse("u32"), Type::U32);
    assert_eq!(parse("f64"), Type::F64);
    assert_eq!(parse("bool"), Type::Bool);
    assert_eq!(parse("char"), Type::Char);
    assert_eq!(parse("String"), Type::Str); // case-folded to "string"
    assert_eq!(parse("Symbol"), Type::Symbol); // case-folded to "symbol"
}

#[test]
fn unit_type() {
    assert_eq!(parse("()"), Type::Unit);
}

#[test]
fn unit_as_a_generic_argument() {
    // `(`/`)` are reader delimiters, so getting `()` *inside* a type token
    // takes two halves: the reader's type-argument grammar taking the pair as
    // a type, and `NameTok::Unit` picking it back out. Before that this
    // spelling tripped an assertion in `Path::from_segments`.
    let unit_string = vec![Type::Unit, Type::Str];
    assert_eq!(parse("Result<(), String>"), Type::Named(Path::root("result"), unit_string.clone()));
    // Without the space, and with the unit in trailing position too.
    assert_eq!(parse("Result<(),String>"), Type::Named(Path::root("result"), unit_string));
    assert_eq!(parse("Result<(),()>"), Type::Named(Path::root("result"), vec![Type::Unit, Type::Unit]));
}

#[test]
fn unit_nested_inside_another_generic_argument() {
    assert_eq!(
        parse("Option<Result<(), String>>"),
        Type::Named(
            Path::root("option"),
            vec![Type::Named(Path::root("result"), vec![Type::Unit, Type::Str])]
        )
    );
}

#[test]
fn a_unit_argument_records_no_name() {
    // `()` is not a nominal name, so — like a primitive — it contributes
    // nothing to the semantic-token spans; the names around it still line up
    // with their real columns (`file-error` starts past `result<(), `).
    assert_eq!(
        spans("result<(), file-error>"),
        vec![("result".to_string(), 1), ("file-error".to_string(), 12)]
    );
}

#[test]
fn a_malformed_generic_is_an_error_not_a_panic() {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    // An unterminated `<` — the reader's speculative extension rewinds and
    // leaves the bare token `result<`, whose argument list has no name in it.
    // This used to reach `Path::from_segments` with zero segments.
    let v = r.read(&mut h, "Result<").expect("read failed");
    assert!(parse_type(&h, v).is_err(), "`Result<` must be a reported type error");
}

#[test]
fn generic_option_and_vec() {
    assert_eq!(parse("Option<int>"), Type::Named(Path::root("option"), vec![Type::Int]));
    assert_eq!(parse("Vec<String>"), Type::Named(Path::root("vec"), vec![Type::Str]));
}

#[test]
fn nested_generics() {
    assert_eq!(
        parse("Vec<Option<int>>"),
        Type::Named(Path::root("vec"), vec![Type::Named(Path::root("option"), vec![Type::Int])])
    );
}

#[test]
fn multi_param_generic() {
    assert_eq!(
        parse("Pair<K,V>"),
        Type::Named(
            Path::root("pair"),
            vec![Type::Named(Path::root("k"), vec![]), Type::Named(Path::root("v"), vec![])]
        )
    );
}

#[test]
fn sexpr_and_user_types_are_named() {
    assert_eq!(parse("Sexpr"), Type::Named(Path::root("sexpr"), vec![]));
    assert_eq!(parse("Point"), Type::Named(Path::root("point"), vec![]));
}

#[test]
fn never_type() {
    assert_eq!(parse("!"), Type::Never);
}

#[test]
fn qualified_path_types() {
    // `geometry::Point` reads as a Path; its raw name keeps the `::`.
    assert_eq!(parse("geometry::Point"), Type::Named(Path::of(&["geometry", "point"]), vec![]));
    // generics stay on the last segment
    assert_eq!(
        parse("geometry::Vec<String>"),
        Type::Named(Path::of(&["geometry", "vec"]), vec![Type::Str])
    );
}

#[test]
fn function_types() {
    assert_eq!(
        parse("(fn (int int) int)"),
        Type::Fn(vec![Type::Int, Type::Int], None, Box::new(Type::Int))
    );
    assert_eq!(parse("(fn () bool)"), Type::Fn(vec![], None, Box::new(Type::Bool)));
    assert_eq!(
        parse("(fn (Option<int>) int)"),
        Type::Fn(vec![Type::Named(Path::root("option"), vec![Type::Int])], None, Box::new(Type::Int))
    );
}

#[test]
fn variadic_function_types() {
    assert_eq!(
        parse("(fn (int &rest int) int)"),
        Type::Fn(vec![Type::Int], Some(Box::new(Type::Int)), Box::new(Type::Int))
    );
    // `&rest` with no fixed parameters before it.
    assert_eq!(
        parse("(fn (&rest bool) bool)"),
        Type::Fn(vec![], Some(Box::new(Type::Bool)), Box::new(Type::Bool))
    );
}

#[test]
fn rest_not_last_in_a_function_type_is_an_error() {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let v = r.read(&mut h, "(fn (&rest int int) int)").expect("read failed");
    assert!(parse_type(&h, v).is_err());
}

#[test]
fn rest_with_no_element_type_in_a_function_type_is_an_error() {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let v = r.read(&mut h, "(fn (&rest) int)").expect("read failed");
    assert!(parse_type(&h, v).is_err());
}

// ---- trait objects (`:dyn Trait`) ---------------------------------------

/// The reader joins the two-word `:dyn Trait` spelling into `(:dyn Trait)`,
/// which `parse_type` recognizes. The head is a *trait* path, so it never
/// becomes a `Type::Named` (traits live in their own registry table).
#[test]
fn dyn_parses_into_a_trait_object_type() {
    assert_eq!(parse(":dyn drawable"), Type::Dyn(Path::root("drawable"), vec![]));
    assert_eq!(parse(":dyn shapes::drawable"), Type::Dyn(Path::of(&["shapes", "drawable"]), vec![]));
}

/// A trait's associated types are pinned positionally, in declaration order —
/// written with the generic-argument syntax on the trait name.
#[test]
fn dyn_pins_associated_types_positionally() {
    assert_eq!(parse(":dyn iter<int>"), Type::Dyn(Path::root("iter"), vec![Type::Int]));
    assert_eq!(
        parse(":dyn pairwise<int,string>"),
        Type::Dyn(Path::root("pairwise"), vec![Type::Int, Type::Str])
    );
}

/// The motivating case: a heterogeneous collection. Inside a generic argument
/// there is no datum boundary, so this rides on the reader's angle-bracket
/// token extension plus the type lexer's whitespace token.
#[test]
fn dyn_can_appear_as_a_generic_argument() {
    assert_eq!(
        parse("Vector<:dyn drawable>"),
        Type::Named(Path::root("vector"), vec![Type::Dyn(Path::root("drawable"), vec![])])
    );
    assert_eq!(
        parse("HashTable<string, :dyn drawable>"),
        Type::Named(
            Path::root("hashtable"),
            vec![Type::Str, Type::Dyn(Path::root("drawable"), vec![])]
        )
    );
    assert_eq!(
        parse("Vector<:dyn iter<int>>"),
        Type::Named(Path::root("vector"), vec![Type::Dyn(Path::root("iter"), vec![Type::Int])])
    );
}

#[test]
fn dyn_in_a_function_type_parses() {
    assert_eq!(
        parse("(fn (:dyn drawable) string)"),
        Type::Fn(vec![Type::Dyn(Path::root("drawable"), vec![])], None, Box::new(Type::Str))
    );
}

#[test]
fn dyn_must_be_followed_by_a_trait_name() {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    // A primitive is not a trait name.
    let v = r.read(&mut h, ":dyn i32").expect("read failed");
    assert!(parse_type(&h, v).is_err(), "`:dyn int` must not parse as a trait object");
}

// ---- recorded name spans (semantic highlighting) ------------------------
//
// `parse_type_spanned` reports where each written type/trait name sits, which
// is what makes the LSP's type highlighting resolution-driven rather than a
// text search (`src/check/semantic.rs`). A generic type is read as *one*
// symbol token, so every name inside one is located by arithmetic within that
// token -- the part worth pinning down here.

/// Read `src` as a single datum with its span (the way the checker gets one
/// for a type annotation) and return the names `parse_type_spanned` recorded,
/// each as `(text at that span, 1-based start column)`.
fn spans(src: &str) -> Vec<(String, u32)> {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let read = r.read_all_in_spanned(&mut h, "t.typl", src).expect("read failed");
    let (v, loc) = read[0].clone();
    let mut out = Vec::new();
    typelisp::parse_type_spanned(&h, v, Some(&loc), &mut out).expect("parse failed");
    let chars: Vec<char> = src.chars().collect();
    out.iter()
        .map(|s| {
            let text: String =
                chars[(s.loc.col - 1) as usize..(s.loc.end_col - 1) as usize].iter().collect();
            (text, s.loc.col)
        })
        .collect()
}

#[test]
fn a_plain_name_is_located_at_the_whole_token() {
    assert_eq!(spans("rect"), vec![("rect".to_string(), 1)]);
}

#[test]
fn a_primitive_records_no_name() {
    // Primitives are not nominal types; both editors' grammars already colour
    // them, so recording them would override a scope that was already right.
    assert!(spans("i32").is_empty());
    assert!(spans("(fn (int) bool)").is_empty());
}

#[test]
fn a_qualified_name_is_located_at_its_last_segment() {
    // `geometry::point` -- the type is `point`, at column 11, not the whole
    // token and not the module prefix.
    assert_eq!(spans("geometry::point"), vec![("point".to_string(), 11)]);
}

#[test]
fn a_generic_argument_is_located_inside_the_token() {
    // One symbol token, two names: the head at column 1 and the argument at
    // column 12, past `hashtable<int,`.
    assert_eq!(
        spans("hashtable<int,todo-item>"),
        vec![("hashtable".to_string(), 1), ("todo-item".to_string(), 15)]
    );
}

#[test]
fn the_head_is_recorded_before_its_arguments() {
    // The order matters: `parse_dyn_type` reinterprets the *first* recorded
    // name as the trait, so an outer name must always precede what nests in it.
    let found = spans("vector<pair<a-type,b-type>>");
    let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["vector", "pair", "a-type", "b-type"]);
}

#[test]
fn a_dyn_head_is_flagged_as_a_trait() {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let read = r.read_all_in_spanned(&mut h, "t.typl", ":dyn shape").expect("read failed");
    let (v, loc) = read[0].clone();
    let mut out = Vec::new();
    typelisp::parse_type_spanned(&h, v, Some(&loc), &mut out).expect("parse failed");
    assert_eq!(out.len(), 1);
    assert!(out[0].dyn_head, "the `:dyn` head names a trait, not a type");
    // Column 6: past `:dyn `, on the trait name itself.
    assert_eq!(out[0].loc.col, 6);
    assert_eq!(out[0].loc.end_col, 11);
}

#[test]
fn a_nested_dyn_is_flagged_and_located_inside_its_token() {
    // `vector<:dyn shape>` is a single token; the trait sits at column 13.
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let read = r.read_all_in_spanned(&mut h, "t.typl", "vector<:dyn shape>").expect("read failed");
    let (v, loc) = read[0].clone();
    let mut out = Vec::new();
    typelisp::parse_type_spanned(&h, v, Some(&loc), &mut out).expect("parse failed");
    let dyn_heads: Vec<&typelisp::TypeNameSpan> = out.iter().filter(|s| s.dyn_head).collect();
    assert_eq!(dyn_heads.len(), 1, "recorded: {:?}", out);
    assert_eq!(dyn_heads[0].loc.col, 13);
    assert_eq!(dyn_heads[0].loc.end_col, 18);
}

#[test]
fn without_a_span_the_parse_still_succeeds_and_records_nothing() {
    // The checker hands `None` wherever a form has no recorded position (a
    // macro-synthesized annotation); parsing must not depend on it.
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let v = r.read(&mut h, "vector<rect>").expect("read failed");
    let mut out = Vec::new();
    let ty = typelisp::parse_type_spanned(&h, v, None, &mut out).expect("parse failed");
    assert_eq!(ty, Type::Named(Path::root("vector"), vec![Type::Named(Path::root("rect"), vec![])]));
    assert!(out.is_empty());
}

/// The *applied* spelling of a generic — `(vector char)` for `Vector<char>`.
///
/// Both are one type. The name form is what programs are written in; the
/// list form exists because a generic *argument* is a whole type expression
/// and the name grammar can only spell arguments that are names, `()` or
/// `:dyn` — see the test below for the one that has no name form at all.
#[test]
fn a_generic_can_be_written_applied() {
    let vector_char = Type::Named(Path::root("vector"), vec![Type::Char]);
    assert_eq!(parse("(vector char)"), vector_char);
    assert_eq!(parse("Vector<char>"), vector_char);
    // Nested, and with a qualified head — the head is parsed as an ordinary
    // type expression, so every name spelling works there too.
    assert_eq!(
        parse("(vector (option char))"),
        Type::Named(
            Path::root("vector"),
            vec![Type::Named(Path::root("option"), vec![Type::Char])]
        )
    );
    assert_eq!(
        parse("(geo::pair int char)"),
        Type::Named(Path::from_segments(vec!["geo".into(), "pair".into()]), vec![Type::Int, Type::Char])
    );
}

/// What the applied form is for: `Vector<(fn (int) int)>` cannot be written
/// as a name, because a `(fn ...)` type is a list and the name grammar has no
/// token for it. Substituting a trait's associated type into a signature has
/// to be able to produce it anyway (`Checker::subst_inside_name`), so the
/// syntax it emits has to be able to hold one.
#[test]
fn an_applied_generic_carries_an_argument_no_name_can_spell() {
    assert_eq!(
        parse("(vector (fn (int) int))"),
        Type::Named(
            Path::root("vector"),
            vec![Type::Fn(vec![Type::Int], None, Box::new(Type::Int))]
        )
    );
    // The reader's type grammar has no parenthesized type but `()`, so this
    // spelling does not even read.
    let mut h = Heap::with_capacity(256);
    assert!(Reader::new().read(&mut h, "Vector<(fn (int) int)>").is_err());
}

#[test]
fn an_applied_generic_is_rejected_without_arguments_or_with_an_applied_head() {
    // `(vector)` supplies no arguments, so it says nothing `vector` doesn't.
    assert!(parse_result("(vector)").is_err());
    // `(vector<char> int)` gives `vector` two argument lists.
    assert!(parse_result("(vector<char> int)").is_err());
}

/// [`parse_type_name`] answers "is this *name* a type?" for symbols that may
/// be nothing of the kind (`Checker::subst_inside_name` asks it about every
/// name in a signature being substituted), so it has to consume the whole
/// string. The parse it wraps stops at the closing `>` and would otherwise
/// report the method path `vector<t>::new` as the type `vector<t>`.
#[test]
fn a_type_name_must_be_the_whole_string() {
    assert_eq!(
        typelisp::parse_type_name("vector<char>").unwrap(),
        Type::Named(Path::root("vector"), vec![Type::Char])
    );
    assert!(typelisp::parse_type_name("vector<t>::new").is_err());
    assert!(typelisp::parse_type_name("vector<char>>").is_err());
}
