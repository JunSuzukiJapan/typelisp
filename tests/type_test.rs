//! Tests for parsing type expressions from read `Sexpr` values.

extern crate typelisp;
use typelisp::{parse_type, Heap, Path, Reader, Type};

fn parse(src: &str) -> Type {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let v = r.read(&mut h, src).expect("read failed");
    parse_type(&h, v).expect("parse_type failed")
}

#[test]
fn primitives() {
    assert_eq!(parse("i32"), Type::I32);
    assert_eq!(parse("i64"), Type::I64);
    assert_eq!(parse("u8"), Type::U8);
    assert_eq!(parse("usize"), Type::Usize);
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
fn generic_option_and_vec() {
    assert_eq!(parse("Option<i32>"), Type::Named(Path::root("option"), vec![Type::I32]));
    assert_eq!(parse("Vec<String>"), Type::Named(Path::root("vec"), vec![Type::Str]));
}

#[test]
fn nested_generics() {
    assert_eq!(
        parse("Vec<Option<i32>>"),
        Type::Named(Path::root("vec"), vec![Type::Named(Path::root("option"), vec![Type::I32])])
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
        parse("(fn (i32 i32) i32)"),
        Type::Fn(vec![Type::I32, Type::I32], None, Box::new(Type::I32))
    );
    assert_eq!(parse("(fn () bool)"), Type::Fn(vec![], None, Box::new(Type::Bool)));
    assert_eq!(
        parse("(fn (Option<i32>) i32)"),
        Type::Fn(vec![Type::Named(Path::root("option"), vec![Type::I32])], None, Box::new(Type::I32))
    );
}

#[test]
fn variadic_function_types() {
    assert_eq!(
        parse("(fn (i32 &rest i32) i32)"),
        Type::Fn(vec![Type::I32], Some(Box::new(Type::I32)), Box::new(Type::I32))
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
    let v = r.read(&mut h, "(fn (&rest i32 i32) i32)").expect("read failed");
    assert!(parse_type(&h, v).is_err());
}

#[test]
fn rest_with_no_element_type_in_a_function_type_is_an_error() {
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let v = r.read(&mut h, "(fn (&rest) i32)").expect("read failed");
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
    assert_eq!(parse(":dyn iter<i32>"), Type::Dyn(Path::root("iter"), vec![Type::I32]));
    assert_eq!(
        parse(":dyn pairwise<i32,string>"),
        Type::Dyn(Path::root("pairwise"), vec![Type::I32, Type::Str])
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
        parse("Vector<:dyn iter<i32>>"),
        Type::Named(Path::root("vector"), vec![Type::Dyn(Path::root("iter"), vec![Type::I32])])
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
    assert!(parse_type(&h, v).is_err(), "`:dyn i32` must not parse as a trait object");
}
