//! Tests for parsing type expressions from read `Sexpr` values.

extern crate typelisp;
use typelisp::{parse_type, Heap, Reader, Type};

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
}

#[test]
fn unit_type() {
    assert_eq!(parse("()"), Type::Unit);
}

#[test]
fn generic_option_and_vec() {
    assert_eq!(parse("Option<i32>"), Type::Named("option".into(), vec![Type::I32]));
    assert_eq!(parse("Vec<String>"), Type::Named("vec".into(), vec![Type::Str]));
}

#[test]
fn nested_generics() {
    assert_eq!(
        parse("Vec<Option<i32>>"),
        Type::Named("vec".into(), vec![Type::Named("option".into(), vec![Type::I32])])
    );
}

#[test]
fn multi_param_generic() {
    assert_eq!(
        parse("Pair<K,V>"),
        Type::Named(
            "pair".into(),
            vec![Type::Named("k".into(), vec![]), Type::Named("v".into(), vec![])]
        )
    );
}

#[test]
fn sexpr_and_user_types_are_named() {
    assert_eq!(parse("Sexpr"), Type::Named("sexpr".into(), vec![]));
    assert_eq!(parse("Point"), Type::Named("point".into(), vec![]));
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
        Type::Fn(vec![Type::Named("option".into(), vec![Type::I32])], Box::new(Type::I32))
    );
}
