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

// A `Path` serializes as its `::`-joined string form rather than a segment
// array, so a `HashMap<Path, _>` survives formats (like JSON) that require
// object keys to be strings — the fasl format relies on this
// (`AdtDef::trait_assoc`, `DefLocs`'s `Path`-keyed maps). Round-trips
// exactly: a path segment can never itself contain `::` (the reader splits
// tokens on `::` to form segments), so join/split is lossless.
impl serde::Serialize for Path {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.join("::"))
    }
}

impl<'de> serde::Deserialize<'de> for Path {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Path, D::Error> {
        let joined = String::deserialize(d)?;
        Ok(Path(joined.split("::").map(str::to_string).collect()))
    }
}

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

#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
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
    /// The `bignum` type: an arbitrary-precision integer (CL's bignum). A
    /// separate static type from the fixed-width integers — this language is
    /// statically typed, so CL's transparent fixnum->bignum overflow
    /// promotion doesn't apply; conversions are explicit (`int->bignum`/
    /// `bignum->int`/...).
    Bignum,
    /// The `ratio` type: an exact rational (CL's ratio), kept reduced with a
    /// positive denominator. Like `Bignum`, its own static type with
    /// explicit conversions (`int->ratio`/`ratio->float`/...).
    Ratio,
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
    /// A trait object `:dyn Trait` / `:dyn Trait<Pin,...>` — a value whose
    /// concrete type is only known at run time, dispatched through a vtable
    /// (TODO T4). The [`Path`] is the *trait*'s fully-qualified path (traits
    /// live in `Namespace::traits`, a different table from types, so this is
    /// deliberately not a `Named`); the `Vec<Type>` pins the trait's
    /// associated types, positionally in `TraitDef::assoc_types` declaration
    /// order — `:dyn Iter<i32>` is `Iter` with `Item = i32`, the counterpart
    /// of Rust's `dyn Iterator<Item = i32>`.
    ///
    /// A `Dyn` is always heap-represented (`Checker::is_heap_repr`): its
    /// runtime value is a `BoxedObj::Dyn` fat box holding the vtable id
    /// alongside the concrete value.
    Dyn(Path, Vec<Type>),
    /// A function type `(fn (params...) ret)`, or — when the second field is
    /// `Some` — a variadic function type `(fn (params... &rest elem) ret)`:
    /// every call-site argument from that point on must have type `elem`
    /// (see `Checker::check_call`/`check_apply`/`apply`'s desugaring, and
    /// `Checker::parse_params_rest` for the parallel `defun`/`lambda`
    /// parameter-list syntax).
    Fn(Vec<Type>, Option<Box<Type>>, Box<Type>),
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
        Type::F32, Type::F64, Type::Bignum, Type::Ratio,
        Type::Bool, Type::Char, Type::Str, Type::Symbol,
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
        Type::Bignum => "bignum",
        Type::Ratio => "ratio",
        Type::Bool => "bool",
        Type::Char => "char",
        Type::Str => "string",
        Type::Symbol => "symbol",
        Type::Unit | Type::Never | Type::Named(..) | Type::Dyn(..) | Type::Fn(..) => return None,
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
        // `(:dyn Trait)` — the reader's joined form of the two-word `:dyn
        // Trait` spelling (`read::reader::read_datum`). Checked before
        // `parse_fn_type` since both are lists.
        Value::Cons(_) if is_dyn_form(heap, v) => parse_dyn_type(heap, v),
        Value::Cons(_) => parse_fn_type(heap, v),
        other => Err(Error::TypeError(format!("not a type expression: {:?}", other))),
    }
}

/// Whether `v` is a `(:dyn ...)` list.
pub fn is_dyn_form(heap: &Heap, v: Value) -> bool {
    matches!(heap.car(v), Ok(Value::Symbol(id)) if heap.symbol_name(id) == ":dyn")
}

/// Parse the reader-joined `(:dyn Trait)` / `(:dyn Trait<Pin,...>)` form. The
/// trait name is parsed with the ordinary type-name grammar, so a qualified
/// and/or generic-looking spelling works; its "generic arguments" are the
/// associated-type pins, not type arguments (see [`Type::Dyn`]).
fn parse_dyn_type(heap: &Heap, v: Value) -> Result<Type, Error> {
    let elems = heap.list_to_vec(v)?;
    if elems.len() != 2 {
        return Err(Error::TypeError("`:dyn` must be followed by exactly one trait name".to_string()));
    }
    match parse_type(heap, elems[1])? {
        Type::Named(trait_path, pins) => Ok(Type::Dyn(trait_path, pins)),
        other => Err(Error::TypeError(format!("`:dyn` must be followed by a trait name, found `{:?}`", other))),
    }
}

/// Parse a `(fn (param-types...) ret-type)` list. The parameter-type list may
/// end in `&rest elem-type` to write a variadic function type, mirroring
/// `Checker::parse_params_rest`'s `defun`/`lambda` parameter syntax.
fn parse_fn_type(heap: &Heap, v: Value) -> Result<Type, Error> {
    let elems = heap.list_to_vec(v)?;
    if elems.len() != 3 {
        return Err(Error::TypeError("fn type must be (fn (params) ret)".to_string()));
    }
    match elems[0] {
        Value::Symbol(id) if heap.symbol_name(id) == "fn" => {}
        _ => return Err(Error::TypeError("expected fn type".to_string())),
    }
    let (params, rest) = match elems[1] {
        Value::Empty => (Vec::new(), None),
        Value::Cons(_) => parse_fn_params(heap, &heap.list_to_vec(elems[1])?)?,
        _ => return Err(Error::TypeError("fn parameter list must be a list".to_string())),
    };
    let ret = parse_type(heap, elems[2])?;
    Ok(Type::Fn(params, rest, Box::new(ret)))
}

/// Split a `(fn ...)` type's parameter-type list into fixed types and an
/// optional trailing `&rest elem-type` (the type of each variadic argument) —
/// the type-expression counterpart of `Checker::parse_params_rest`.
fn parse_fn_params(heap: &Heap, ps: &[Value]) -> Result<(Vec<Type>, Option<Box<Type>>), Error> {
    let rest_marker = ps
        .iter()
        .position(|p| matches!(p, Value::Symbol(id) if heap.symbol_name(*id) == "&rest"));
    match rest_marker {
        Some(i) => {
            if i + 2 != ps.len() {
                return Err(Error::TypeError(
                    "fn type: &rest must be followed by exactly one type, as the last item in the parameter list".to_string(),
                ));
            }
            let params = ps[..i].iter().map(|p| parse_type(heap, *p)).collect::<Result<_, _>>()?;
            let rest = parse_type(heap, ps[i + 1])?;
            Ok((params, Some(Box::new(rest))))
        }
        None => {
            let params = ps.iter().map(|p| parse_type(heap, *p)).collect::<Result<_, _>>()?;
            Ok((params, None))
        }
    }
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
        skip_space(toks);
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
        skip_space(toks);
        if matches!(toks.peek(), Some(NameTok::Gt)) {
            toks.next(); // empty argument list, e.g. `Foo<>`
        } else {
            loop {
                args.push(parse_type_arg(toks));
                skip_space(toks);
                match toks.next() {
                    Some(NameTok::Comma) => continue,
                    _ => break, // '>' (or a malformed, premature end) closes the list
                }
            }
        }
    }
    (segs, args)
}

/// Skip a run of spaces. Only a reader-extended token can contain one
/// (`NameTok::Space`), but once one does, spaces may separate any two tokens
/// in it — `hashtable<string, :dyn drawable>` has one after the comma and
/// another after `:dyn`.
fn skip_space<'a>(toks: &mut Peekable<NameLexer<'a>>) {
    while matches!(toks.peek(), Some(NameTok::Space)) {
        toks.next();
    }
}

/// One generic argument. Almost always an ordinary qualified/generic type
/// name, but `:dyn Trait` may appear here too — the nested spelling of the
/// same trait-object type `parse_dyn_type` builds from the datum-level form,
/// e.g. the element type of `Vector<:dyn Drawable>`.
fn parse_type_arg<'a>(toks: &mut Peekable<NameLexer<'a>>) -> Type {
    skip_space(toks);
    if matches!(toks.peek(), Some(NameTok::Ident(s)) if *s == ":dyn") {
        toks.next(); // `:dyn`
        let (segs, pins) = parse_qualified_generic(toks);
        return Type::Dyn(Path::from_segments(segs), pins);
    }
    let (segs, args) = parse_qualified_generic(toks);
    named_or_primitive(segs, args)
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
            "bignum" => return Type::Bignum,
            "ratio" => return Type::Ratio,
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
