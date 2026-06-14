//! typelisp's type representation and parsing of type expressions.
//!
//! Types appear in source as `Sexpr`: a symbol such as `i32`, `bool`, `String`,
//! or a generic written without spaces like `Option<i32>` / `Vec<String>` /
//! `Pair<K,V>` (read as a *single* symbol token, so generics are split out of
//! the symbol's name here), the empty list `()` for the unit type, or a list
//! `(fn (param-types...) ret-type)` for a function type.
//!
//! Symbols are case-folded by the reader, so all type names are lowercase here.

use std::fmt;

use crate::{Error, Heap, Value};

/// A structured, fully-qualified path identifying a type, free function, or
/// module — a sequence of lowercase segments (e.g. `geo::point` is
/// `["geo", "point"]`). Used instead of a joined `"a::b"` string so identity is
/// never re-parsed; the only place `::` strings are split is the reader/parser
/// (surface syntax). A single-segment path is also how type *variables* and
/// root-level names are represented.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Path(Vec<String>);

impl Path {
    /// Build a path from explicit segments.
    pub fn of(segments: &[&str]) -> Path {
        Path(segments.iter().map(|s| s.to_string()).collect())
    }

    /// A single-segment path (a root-level name or a type variable).
    pub fn root(name: &str) -> Path {
        Path(vec![name.to_string()])
    }

    /// Build a path from owned segments (must be non-empty).
    pub fn from_segments(segments: Vec<String>) -> Path {
        debug_assert!(!segments.is_empty(), "a path must have at least one segment");
        Path(segments)
    }

    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// The final (unqualified) segment.
    pub fn local(&self) -> &str {
        self.0.last().expect("a path has at least one segment")
    }

    /// The module prefix (everything but the final segment).
    pub fn parent(&self) -> &[String] {
        &self.0[..self.0.len() - 1]
    }

    /// Extend the path with another segment.
    pub fn child(&self, name: &str) -> Path {
        let mut segments = self.0.clone();
        segments.push(name.to_string());
        Path(segments)
    }

    /// True for a single-segment path (a root name or type variable).
    pub fn is_simple(&self) -> bool {
        self.0.len() == 1
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.join("::"))
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    Isize,
    U8,
    U16,
    U32,
    U64,
    Usize,
    F32,
    F64,
    Bool,
    Char,
    /// The `String` type.
    Str,
    /// The unit type `()`.
    Unit,
    /// The never / bottom type `!` (the type of `panic!` and other diverging
    /// forms). It is compatible with — and absorbed by — any expected type.
    Never,
    /// A nominal type with type arguments: `Option<T>`, `Vec<T>`, `Sexpr`,
    /// user structs, and (during checking) generic type variables (a
    /// single-segment [`Path`]).
    Named(Path, Vec<Type>),
    /// A function type `(fn (params...) ret)`.
    Fn(Vec<Type>, Box<Type>),
}

/// Parse a type expression (a read `Value`) into a [`Type`].
pub fn parse_type(heap: &Heap, v: Value) -> Result<Type, Error> {
    match v {
        Value::Empty => Ok(Type::Unit),
        Value::Symbol(id) => Ok(parse_type_name(heap.symbol_name(id))),
        Value::Path(id) => {
            // A qualified type name like `geometry::Point`. Reconstruct the
            // surface token so `parse_type_name` can split off any generics on
            // the last segment; it yields a structured `Path`.
            let name = heap
                .path_segments(id)
                .iter()
                .map(|s| heap.symbol_name(*s))
                .collect::<Vec<_>>()
                .join("::");
            Ok(parse_type_name(&name))
        }
        Value::Cons(_) => parse_fn_type(heap, v),
        other => Err(Error::TypeError(format!("not a type expression: {:?}", other))),
    }
}

/// Parse a `(fn (param-types...) ret-type)` list.
fn parse_fn_type(heap: &Heap, v: Value) -> Result<Type, Error> {
    let elems = heap.list_to_vec(v)?;
    if elems.len() != 3 {
        return Err(Error::TypeError("fn type must be (fn (params) ret)".to_string()));
    }
    match elems[0] {
        Value::Symbol(id) if heap.symbol_name(id) == "fn" => {}
        _ => return Err(Error::TypeError("expected fn type".to_string())),
    }
    let params = match elems[1] {
        Value::Empty => Vec::new(),
        Value::Cons(_) => {
            let ps = heap.list_to_vec(elems[1])?;
            ps.into_iter().map(|p| parse_type(heap, p)).collect::<Result<_, _>>()?
        }
        _ => return Err(Error::TypeError("fn parameter list must be a list".to_string())),
    };
    let ret = parse_type(heap, elems[2])?;
    Ok(Type::Fn(params, Box::new(ret)))
}

/// Parse a type from a (case-folded) token, splitting generic arguments and
/// `::` path segments into a structured [`Type`]/[`Path`]. This — and the
/// reader — are the only places `::` strings are decoded.
fn parse_type_name(name: &str) -> Type {
    let (head, args) = split_generics(name);
    let segs: Vec<String> = head.split("::").map(|s| s.to_string()).collect();
    if args.is_empty() && segs.len() == 1 {
        match segs[0].as_str() {
            "i8" => return Type::I8,
            "i16" => return Type::I16,
            "i32" => return Type::I32,
            "i64" => return Type::I64,
            "isize" => return Type::Isize,
            "u8" => return Type::U8,
            "u16" => return Type::U16,
            "u32" => return Type::U32,
            "u64" => return Type::U64,
            "usize" => return Type::Usize,
            "f32" => return Type::F32,
            "f64" => return Type::F64,
            "bool" => return Type::Bool,
            "char" => return Type::Char,
            "string" => return Type::Str,
            "!" => return Type::Never,
            _ => {}
        }
    }
    Type::Named(
        Path::from_segments(segs),
        args.iter().map(|a| parse_type_name(a)).collect(),
    )
}

/// Split `"option<i32>"` into `("option", ["i32"])`. Non-generic names yield
/// no args. Commas separate arguments at the top `<>` nesting level.
fn split_generics(name: &str) -> (&str, Vec<String>) {
    if let Some(lt) = name.find('<') {
        if name.ends_with('>') {
            let head = &name[..lt];
            let inner = &name[lt + 1..name.len() - 1];
            return (head, split_top_level_commas(inner));
        }
    }
    (name, Vec::new())
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out
}
