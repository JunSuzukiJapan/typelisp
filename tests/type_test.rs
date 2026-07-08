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
        Type::Fn(vec![Type::I32, Type::I32], Box::new(Type::I32))
    );
    assert_eq!(parse("(fn () bool)"), Type::Fn(vec![], Box::new(Type::Bool)));
    assert_eq!(
        parse("(fn (Option<i32>) i32)"),
        Type::Fn(vec![Type::Named(Path::root("option"), vec![Type::I32])], Box::new(Type::I32))
    );
}

#[test]
fn rest_in_a_function_type_is_rejected() {
    // Function types are always fixed-arity — there are no variadic
    // value-level functions, so `&rest` is not a valid `fn`-type parameter.
    let mut h = Heap::with_capacity(256);
    let r = Reader::new();
    let v = r.read(&mut h, "(fn (i32 &rest i32) i32)").expect("read failed");
    assert!(parse_type(&h, v).is_err());
}
