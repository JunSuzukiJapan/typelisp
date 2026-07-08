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
use std::iter::Peekable;

use crate::name_lexer::{NameLexer, NameTok};
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
    /// The `Symbol` type: an interned symbol handle, distinct from `Str`.
    /// Its runtime value is a `Value::Symbol(SymId)` (the same carrier a
    /// `Sexpr::Sym` holds), but statically it is its own primitive type so
    /// `gensym`/`string->symbol` can be typed precisely instead of as the
    /// heterogeneous `Sexpr`. `symbol->string`/`string->symbol` are the only
    /// bridges to/from `Str`.
    Symbol,
    /// The unit type `()`.
    Unit,
    /// The never / bottom type `!` (the type of `panic` and other diverging
    /// forms). It is compatible with — and absorbed by — any expected type.
    Never,
    /// A nominal type with type arguments: `Option<T>`, `Vec<T>`, `Sexpr`,
    /// user structs, and (during checking) generic type variables (a
    /// single-segment [`Path`]).
    Named(Path, Vec<Type>),
    /// A function type `(fn (params...) ret)`. Always fixed-arity — the
    /// language has no variadic value-level functions (`&rest` exists only in
    /// `defmacro`'s macro lambda list, never in `defun`/`lambda`/`fn` types).
    Fn(Vec<Type>, Box<Type>),
}

impl Type {
    /// The ten built-in integer types (`i8`..`isize`, `u8`..`usize`). The one
    /// authoritative list, so callers that need to single out "an integer
    /// type" (`Checker::int_lit_ty`/pattern checking, `ast_bridge`'s
    /// struct-field classifier) share it instead of each re-enumerating all
    /// ten variants.
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Isize | Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Usize
        )
    }

    /// The two built-in floating-point types (`f32`/`f64`) — the float
    /// counterpart of [`Type::is_integer`].
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }
}

/// All primitive value types that can be a `defmethod` receiver (every
/// variant [`prim_type_path`] maps to a `Path`). Used to pre-register each
/// one's (empty) method table in [`crate::Registry::with_builtins`].
pub fn primitive_types() -> Vec<Type> {
    vec![
        Type::I8, Type::I16, Type::I32, Type::I64, Type::Isize,
        Type::U8, Type::U16, Type::U32, Type::U64, Type::Usize,
        Type::F32, Type::F64, Type::Bool, Type::Char, Type::Str, Type::Symbol,
    ]
}

/// The registry [`Path`] a primitive `Type` is addressed by when used as a
/// `defmethod` receiver (e.g. `Type::I32` -> `i32`). Mirrors the surface
/// keywords [`parse_type_name`] recognizes, so `(defmethod m ((self i32)) ...)`
/// and this lookup agree on the same name. `None` for `Unit`/`Never`/`Named`/
/// `Fn`, which are not primitive value types with their own method table here.
pub fn prim_type_path(ty: &Type) -> Option<Path> {
    let name = match ty {
        Type::I8 => "i8",
        Type::I16 => "i16",
        Type::I32 => "i32",
        Type::I64 => "i64",
        Type::Isize => "isize",
        Type::U8 => "u8",
        Type::U16 => "u16",
        Type::U32 => "u32",
        Type::U64 => "u64",
        Type::Usize => "usize",
        Type::F32 => "f32",
        Type::F64 => "f64",
        Type::Bool => "bool",
        Type::Char => "char",
        Type::Str => "string",
        Type::Symbol => "symbol",
        Type::Unit | Type::Never | Type::Named(..) | Type::Fn(..) => return None,
    };
    Some(Path::root(name))
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

/// Parse a `(fn (param-types...) ret-type)` list. Function types are always
/// fixed-arity — `&rest` is a `defmacro`-only lambda-list marker and is
/// rejected in a `fn` type.
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
        Value::Cons(_) => parse_fn_params(heap, &heap.list_to_vec(elems[1])?)?,
        _ => return Err(Error::TypeError("fn parameter list must be a list".to_string())),
    };
    let ret = parse_type(heap, elems[2])?;
    Ok(Type::Fn(params, Box::new(ret)))
}

/// Parse a `(fn ...)` type's parameter-type list into fixed types. `&rest` is
/// not a valid `fn`-type parameter (no variadic value-level functions).
fn parse_fn_params(heap: &Heap, ps: &[Value]) -> Result<Vec<Type>, Error> {
    if ps.iter().any(|p| matches!(p, Value::Symbol(id) if heap.symbol_name(*id) == "&rest")) {
        return Err(Error::TypeError(
            "fn type: &rest is not allowed (function types are always fixed-arity)".to_string(),
        ));
    }
    ps.iter().map(|p| parse_type(heap, *p)).collect()
}

/// Parse a type from a (case-folded) token, splitting generic arguments and
/// `::` path segments into a structured [`Type`]/[`Path`]. This — and the
/// reader — are the only places `::` strings are decoded.
fn parse_type_name(name: &str) -> Type {
    let mut toks = NameLexer::new(name).peekable();
    let (segs, args) = parse_qualified_generic(&mut toks);
    named_or_primitive(segs, args)
}

/// Recursive-descent parse of `ident (:: ident)* (< args >)?` — the grammar
/// behind a generic type token like `geometry::Pair<K,V>` — into path
/// segments and (if a `<...>` suffix was present) the generic arguments of
/// the final segment. `Vec<a::b>`'s inner `::` is never mistaken for a path
/// separator because it is consumed while parsing the `<...>` argument, one
/// recursive level down from the `::` chain that builds `segs` here.
fn parse_qualified_generic<'a>(toks: &mut Peekable<NameLexer<'a>>) -> (Vec<String>, Vec<Type>) {
    let mut segs = Vec::new();
    loop {
        if let Some(NameTok::Ident(s)) = toks.next() {
            segs.push(s.to_string());
        }
        if matches!(toks.peek(), Some(NameTok::ColonColon)) {
            toks.next();
        } else {
            break;
        }
    }

    let mut args = Vec::new();
    if matches!(toks.peek(), Some(NameTok::Lt)) {
        toks.next(); // '<'
        if matches!(toks.peek(), Some(NameTok::Gt)) {
            toks.next(); // empty argument list, e.g. `Foo<>`
        } else {
            loop {
                let (a_segs, a_args) = parse_qualified_generic(toks);
                args.push(named_or_primitive(a_segs, a_args));
                match toks.next() {
                    Some(NameTok::Comma) => continue,
                    _ => break, // '>' (or a malformed, premature end) closes the list
                }
            }
        }
    }
    (segs, args)
}

/// A non-generic, single-segment name names a primitive; anything else is a
/// nominal [`Type::Named`].
fn named_or_primitive(segs: Vec<String>, args: Vec<Type>) -> Type {
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
            "symbol" => return Type::Symbol,
            "!" => return Type::Never,
            _ => {}
        }
    }
    Type::Named(Path::from_segments(segs), args)
}
