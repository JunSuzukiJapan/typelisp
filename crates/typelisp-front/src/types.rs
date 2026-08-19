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
use std::rc::Rc;

use typelisp_read::name_lexer::{NameLexer, NameTok};
use crate::{Error, Heap, Loc, PathId, SymId, Value};

/// A structured, fully-qualified path identifying a type, free function, or
/// module — a sequence of lowercase segments (e.g. `geo::point` is
/// `["geo", "point"]`). Used instead of a joined `"a::b"` string so identity is
/// never re-parsed; the only place `::` strings are split is the reader/parser
/// (surface syntax). A single-segment path is also how type *variables* and
/// root-level names are represented.
/// `Ord` so a `Path`-keyed table can be a `BTreeMap`: several of the
/// registry's are, because a dump hashes what a table *would serialize to* to
/// decide what changed, and a `HashMap` serializes in an order that differs
/// run to run (`std`'s `RandomState`) — which would make every entry look
/// modified.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, serde::Serialize, serde::Deserialize)]
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

    /// The final segment on its own — **not** the type's identity.
    ///
    /// A type is identified by its *whole* path: `m::vector` and the built-in
    /// `vector` are different types that share a last segment. Two bugs came
    /// from forgetting that (see [`path_is_builtin`] and
    /// `crate::type_key`), so this is deliberately named after what it
    /// returns rather than "the local name". Legitimate uses are name-table
    /// lookups paired with [`Path::parent`] (a module's tables are keyed by
    /// last segment), type-*variable* names (single-segment by construction),
    /// and display/derived names.
    ///
    /// To ask "is this the built-in `X`?" use [`path_is_builtin`]; to spell a
    /// type's runtime identity use `crate::type_key::type_key_of`.
    pub fn last_segment(&self) -> &str {
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

/// The `::`-joined form — **the** spelling of a type's identity. Everything
/// that stores or compares a type by name (a heap value's type name, a
/// registry key, an error message) uses this, never [`Path::last_segment`].
impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.join("::"))
    }
}

/// `p` as an interned heap [`PathId`] — the segments interned as symbols,
/// then the sequence interned as a path. Both tables are permanent, so the id
/// stays valid for the heap's lifetime and equal paths always share one id.
///
/// This is the *structural* Path↔heap correspondence (the one the reader
/// itself produces for a `::`-qualified token), deliberately not the
/// `type_key.rs` string form: it addresses a name, not a heap value's type
/// identity, so nothing here may be compared against a stored type key.
pub fn intern_path_id(heap: &mut Heap, p: &Path) -> PathId {
    let segs: Vec<SymId> = p
        .segments()
        .iter()
        .map(|s| match heap.intern_symbol(s) {
            Value::Symbol(id) => id,
            _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
        })
        .collect();
    match heap.intern_path(&segs) {
        Value::Path(id) => id,
        _ => unreachable!("Heap::intern_path always returns Value::Path"),
    }
}

/// The [`Path`] an interned heap path spells — the inverse of
/// [`intern_path_id`].
pub fn path_from_id(heap: &Heap, id: PathId) -> Path {
    Path::from_segments(
        heap.path_segments(id).iter().map(|s| heap.symbol_name(*s).to_string()).collect(),
    )
}

/// Whether `p` is the built-in type `name`.
///
/// Every built-in (`vector`/`hashtable`/`option`/`result`/`llvm-*`/`scope`/the
/// primitives) is registered at the *root* namespace
/// (`Registry::with_builtins`), so a qualified path can never be one however
/// its last segment reads — which is the whole point of the `is_simple` half.
/// Redefining a built-in type name is rejected at the root
/// (`Checker::check_redef`), but nothing stops `(module m (defstruct vector
/// ...))`, and compiled code that recognized the built-in by last segment
/// alone lowered *that* type's methods to `vector-op`, reading a user struct
/// as if it were a `Vector` — silently wrong for `vector`, a `BoxId does not
/// hold a HashTable` abort for `hashtable` (fixed 2026-08-04).
pub fn path_is_builtin(p: &Path, name: &str) -> bool {
    p.is_simple() && p.last_segment() == name
}

/// [`path_is_builtin`] against a set of names — for the "one of the natively
/// lowered built-ins" tests, whose name lists live as consts below so every
/// site reads the same list instead of spelling its own.
pub fn path_is_builtin_any(p: &Path, names: &[&str]) -> bool {
    p.is_simple() && names.contains(&p.last_segment())
}

/// The LLVM handle *types* (`crate::compile::Repr::Handle`):
/// values compiled code passes around as raw handles rather than heap boxes.
/// A superset of [`LLVM_METHOD_RECEIVER_TYPES`] — `llvm-basic-block` and
/// `llvm-value` are handles that carry no methods of their own.
pub const LLVM_HANDLE_TYPES: [&str; 5] =
    ["llvm-module", "llvm-function", "llvm-builder", "llvm-basic-block", "llvm-value"];

/// The LLVM handle types that *have* methods, i.e. the receivers whose
/// `assoc` lowers to an `llvm-op` node (`llvm_assoc_key`) and is
/// therefore never a real call target (`Interp::compile_scc`'s filters).
/// Deliberately not [`LLVM_HANDLE_TYPES`]: the two lists answer different
/// questions and used to differ only by accident.
pub const LLVM_METHOD_RECEIVER_TYPES: [&str; 4] =
    ["llvm-module", "llvm-function", "llvm-builder", "scope"];

/// The primitive receivers whose built-in methods `compile-assoc` turns into
/// LLVM instructions or `rt_*` calls rather than function calls
/// (`Interp::is_native_lowered_primitive_method` says *which* methods; this
/// says which receivers can have them).
pub const NATIVE_LOWERED_PRIMITIVES: [&str; 10] =
    ["i64", "i32", "char", "string", "f64", "bignum", "ratio", "sexpr", "bool", "symbol"];

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
    /// The `random-state` type (CL's `random-state`): an opaque, mutable PRNG
    /// stream. Has no reader syntax and no arithmetic — only ever produced by
    /// `make-random-state-fresh`/`random-state-copy` and consumed by
    /// `random-state-next` (`BoxedObj::RandomState` holds the actual seed).
    RandomState,
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
    /// type" (`Checker::int_lit_ty`/pattern checking, `core_bridge`'s
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
        Type::F32, Type::F64, Type::Bignum, Type::Ratio, Type::RandomState,
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
        Type::RandomState => "random-state",
        Type::Bool => "bool",
        Type::Char => "char",
        Type::Str => "string",
        Type::Symbol => "symbol",
        Type::Unit | Type::Never | Type::Named(..) | Type::Dyn(..) | Type::Fn(..) => return None,
    };
    Some(Path::root(name))
}

/// One written type/trait *name* occurrence inside a type expression, with
/// the exact source span of its final segment (the name proper — the module
/// prefix of a qualified spelling and any `<...>` argument punctuation are
/// not part of it). Produced by [`parse_type_spanned`]; consumed by the
/// checker, which resolves each written path against the registry and — when
/// it names a user-defined type or trait — records the span for the LSP's
/// semantic tokens. This is what makes type highlighting *resolution*-driven:
/// a function that shares a type's name can never be mistaken for it, because
/// only positions the type grammar actually parsed are ever recorded.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeNameSpan {
    /// The name as written (still relative — not resolved).
    pub path: Path,
    /// True when the name is a `:dyn` head, i.e. names a *trait*.
    pub dyn_head: bool,
    /// Span of the final segment.
    pub loc: Loc,
}

/// Parse a type expression (a read `Value`) into a [`Type`].
pub fn parse_type(heap: &Heap, v: Value) -> Result<Type, Error> {
    parse_type_spanned(heap, v, None, &mut Vec::new())
}

/// [`parse_type`], additionally recording a [`TypeNameSpan`] for every
/// nominal name it parses. `loc` is the span of `v` *itself* and is only
/// needed when `v` is a bare atom (a symbol/path token has no heap identity,
/// so its span must travel alongside it — the same rule as
/// `Checker::check_form_at`'s `loc_hint`); a list form's elements carry their
/// own recorded spans (`Heap::list_to_vec_locs`). With no span available the
/// parse still succeeds — the names just go unrecorded.
pub fn parse_type_spanned(
    heap: &Heap,
    v: Value,
    loc: Option<&Loc>,
    out: &mut Vec<TypeNameSpan>,
) -> Result<Type, Error> {
    match v {
        Value::Empty => Ok(Type::Unit),
        Value::Symbol(id) => {
            let name = heap.symbol_name(id);
            parse_type_name_rec(name, loc, out)
        }
        Value::Path(id) => {
            // A qualified type name like `geometry::Point`. Reconstruct the
            // surface token so the name parser can split off any generics on
            // the last segment; it yields a structured `Path`. The rebuilt
            // string is the source token again (the segments joined by the
            // same `::` the reader split them on), which is what lets an
            // offset within it name a source column — see `SpanRec::record`,
            // which verifies that alignment before recording anything.
            let name = heap
                .path_segments(id)
                .iter()
                .map(|s| heap.symbol_name(*s))
                .collect::<Vec<_>>()
                .join("::");
            parse_type_name_rec(&name, loc, out)
        }
        // `(:dyn Trait)` — the reader's joined form of the two-word `:dyn
        // Trait` spelling (`read::reader::read_datum`). Checked before
        // the `(fn ...)` case since both are lists.
        Value::Cons(_) if is_dyn_form(heap, v) => parse_dyn_type(heap, v, out),
        Value::Cons(_) => parse_fn_type(heap, v, out),
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
fn parse_dyn_type(heap: &Heap, v: Value, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    if elems.len() != 2 {
        return Err(Error::TypeError("`:dyn` must be followed by exactly one trait name".to_string()));
    }
    // Parse into a scratch vector first: the recorder writes the trait head
    // *before* any associated-type pins (see `parse_qualified_generic`), so
    // its first entry — recorded as an ordinary `Named` head — is exactly the
    // name the `:dyn` reinterprets as a trait.
    let mut tmp = Vec::new();
    match parse_type_spanned(heap, elems[1].0, elems[1].1.as_ref(), &mut tmp)? {
        Type::Named(trait_path, pins) => {
            if let Some(head) = tmp.first_mut() {
                head.dyn_head = true;
            }
            out.extend(tmp);
            Ok(Type::Dyn(trait_path, pins))
        }
        other => Err(Error::TypeError(format!("`:dyn` must be followed by a trait name, found `{:?}`", other))),
    }
}

/// Parse a `(fn (param-types...) ret-type)` list. The parameter-type list may
/// end in `&rest elem-type` to write a variadic function type, mirroring
/// `Checker::parse_params_rest`'s `defun`/`lambda` parameter syntax.
fn parse_fn_type(heap: &Heap, v: Value, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    if elems.len() != 3 {
        return Err(Error::TypeError("fn type must be (fn (params) ret)".to_string()));
    }
    match elems[0].0 {
        Value::Symbol(id) if heap.symbol_name(id) == "fn" => {}
        _ => return Err(Error::TypeError("expected fn type".to_string())),
    }
    let (params, rest) = match elems[1].0 {
        Value::Empty => (Vec::new(), None),
        Value::Cons(_) => parse_fn_params(heap, &heap.list_to_vec_locs(elems[1].0)?, out)?,
        _ => return Err(Error::TypeError("fn parameter list must be a list".to_string())),
    };
    let ret = parse_type_spanned(heap, elems[2].0, elems[2].1.as_ref(), out)?;
    Ok(Type::Fn(params, rest, Box::new(ret)))
}

/// Split a `(fn ...)` type's parameter-type list into fixed types and an
/// optional trailing `&rest elem-type` (the type of each variadic argument) —
/// the type-expression counterpart of `Checker::parse_params_rest`.
fn parse_fn_params(
    heap: &Heap,
    ps: &[(Value, Option<Loc>)],
    out: &mut Vec<TypeNameSpan>,
) -> Result<(Vec<Type>, Option<Box<Type>>), Error> {
    let rest_marker = ps
        .iter()
        .position(|(p, _)| matches!(p, Value::Symbol(id) if heap.symbol_name(*id) == "&rest"));
    match rest_marker {
        Some(i) => {
            if i + 2 != ps.len() {
                return Err(Error::TypeError(
                    "fn type: &rest must be followed by exactly one type, as the last item in the parameter list".to_string(),
                ));
            }
            let params = ps[..i]
                .iter()
                .map(|(p, l)| parse_type_spanned(heap, *p, l.as_ref(), out))
                .collect::<Result<_, _>>()?;
            let rest = parse_type_spanned(heap, ps[i + 1].0, ps[i + 1].1.as_ref(), out)?;
            Ok((params, Some(Box::new(rest))))
        }
        None => {
            let params = ps
                .iter()
                .map(|(p, l)| parse_type_spanned(heap, *p, l.as_ref(), out))
                .collect::<Result<_, _>>()?;
            Ok((params, None))
        }
    }
}

/// The span recorder threaded through the token-level parse: `base` is where
/// the token begins in the source, `name` the token text being lexed (so byte
/// offsets from the lexer convert to char columns). Present only when the
/// caller had a span for the token — parsing works identically without it.
struct SpanRec<'a> {
    name: &'a str,
    base: &'a Loc,
    out: &'a mut Vec<TypeNameSpan>,
}

impl SpanRec<'_> {
    /// Record `segs` (a written path) whose final segment occupies bytes
    /// `b0..b1` of the token. A token never spans lines
    /// (`read::reader::extend_angle_token` refuses newlines), so the span
    /// stays on `base`'s line and only the columns shift.
    ///
    /// `name` is the *interned* token, which `Heap::intern_symbol` has
    /// case-folded; the offsets computed from it only name the right source
    /// columns if folding preserved the character count. That holds for every
    /// ASCII identifier, but not universally (`İ` lowercases to two
    /// characters), so the two lengths are compared and a token where they
    /// disagree records nothing rather than painting the wrong range.
    fn record(&mut self, segs: &[String], dyn_head: bool, b0: usize, b1: usize) {
        if self.name.chars().count() as u32 != self.base.end_col.saturating_sub(self.base.col) {
            return;
        }
        let start = self.base.col + self.name[..b0].chars().count() as u32;
        let len = self.name[b0..b1].chars().count() as u32;
        self.out.push(TypeNameSpan {
            path: Path::from_segments(segs.to_vec()),
            dyn_head,
            loc: Loc::new(Rc::clone(&self.base.file), self.base.line, start)
                .with_end(self.base.line, start + len),
        });
    }
}

/// A [`NameLexer`] with one-token lookahead that also reports each token's
/// byte range — [`Peekable`] would hide the underlying `pos()`, and the span
/// recorder needs to know *where* an identifier sat inside the token.
struct Toks<'a> {
    /// The whole token being parsed, kept only so a parse error can quote the
    /// spelling the user actually wrote.
    src: &'a str,
    lex: NameLexer<'a>,
    peeked: Option<(NameTok<'a>, usize, usize)>,
}

impl<'a> Toks<'a> {
    fn new(src: &'a str) -> Toks<'a> {
        Toks { src, lex: NameLexer::new(src), peeked: None }
    }
    /// `(token, start_byte, end_byte)`. `NameLexer::pos()` is "just past the
    /// most recent token", so reading it before and after `next()` brackets
    /// the token exactly (the lexer skips nothing — even spaces are tokens).
    fn next(&mut self) -> Option<(NameTok<'a>, usize, usize)> {
        if let Some(t) = self.peeked.take() {
            return Some(t);
        }
        let start = self.lex.pos();
        let t = self.lex.next()?;
        Some((t, start, self.lex.pos()))
    }
    fn peek(&mut self) -> Option<&NameTok<'a>> {
        if self.peeked.is_none() {
            self.peeked = self.next();
        }
        self.peeked.as_ref().map(|(t, _, _)| t)
    }
}

/// Parse a type from a (case-folded) token, splitting generic arguments and
/// `::` path segments into a structured [`Type`]/[`Path`]. This — and the
/// reader — are the only places `::` strings are decoded. With `loc` present,
/// every nominal name parsed out of the token is recorded into `out`, and a
/// parse error is pinned to the token itself rather than to whatever enclosing
/// form the caller would otherwise blame.
fn parse_type_name_rec(name: &str, loc: Option<&Loc>, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
    let mut toks = Toks::new(name);
    let mut rec = loc.map(|base| SpanRec { name, base, out });
    let parsed = parse_qualified_generic(&mut toks, &mut rec, false).map(|(segs, args)| named_or_primitive(segs, args));
    match (parsed, loc) {
        (Err(e), Some(l)) => Err(e.at(l.clone())),
        (other, _) => other,
    }
}

/// Recursive-descent parse of `ident (:: ident)* (< args >)?` — the grammar
/// behind a generic type token like `geometry::Pair<K,V>` — into path
/// segments and (if a `<...>` suffix was present) the generic arguments of
/// the final segment. `Vec<a::b>`'s inner `::` is never mistaken for a path
/// separator because it is consumed while parsing the `<...>` argument, one
/// recursive level down from the `::` chain that builds `segs` here.
///
/// The head name is recorded *before* its arguments are parsed, so a
/// recorded sequence always lists an outer name ahead of everything nested
/// in it — `parse_dyn_type` relies on that ordering. A name that will
/// resolve to a primitive (single segment, no arguments, in
/// [`primitive_by_name`]) is not a nominal type and is not recorded.
fn parse_qualified_generic<'a>(
    toks: &mut Toks<'a>,
    rec: &mut Option<SpanRec<'_>>,
    dyn_head: bool,
) -> Result<(Vec<String>, Vec<Type>), Error> {
    let mut segs = Vec::new();
    // Byte range of the last identifier consumed — the final path segment,
    // which is the part of the token a recorded span covers.
    let mut last_range = None;
    loop {
        skip_space(toks);
        if let Some((NameTok::Ident(s), b0, b1)) = toks.next() {
            segs.push(s.to_string());
            last_range = Some((b0, b1));
        }
        if matches!(toks.peek(), Some(NameTok::ColonColon)) {
            toks.next();
        } else {
            break;
        }
    }
    // Nothing name-shaped where a name was required. Reachable from ordinary
    // source — `Result<` (an unterminated generic whose speculative extension
    // the reader rewound) leaves an empty argument here — so it has to be a
    // reported error, not the `Path::from_segments` assertion it used to trip.
    if segs.is_empty() {
        return Err(Error::TypeError(format!("malformed type name: `{}`", toks.src)));
    }

    let has_args = matches!(toks.peek(), Some(NameTok::Lt));
    if let (Some(r), Some((b0, b1))) = (rec.as_mut(), last_range) {
        let primitive = !has_args && segs.len() == 1 && primitive_by_name(&segs[0]).is_some();
        if !primitive {
            r.record(&segs, dyn_head, b0, b1);
        }
    }

    let mut args = Vec::new();
    if has_args {
        toks.next(); // '<'
        skip_space(toks);
        if matches!(toks.peek(), Some(NameTok::Gt)) {
            toks.next(); // empty argument list, e.g. `Foo<>`
        } else {
            loop {
                args.push(parse_type_arg(toks, rec)?);
                skip_space(toks);
                match toks.next() {
                    Some((NameTok::Comma, _, _)) => continue,
                    _ => break, // '>' (or a malformed, premature end) closes the list
                }
            }
        }
    }
    Ok((segs, args))
}

/// Skip a run of spaces. Only a reader-extended token can contain one
/// (`NameTok::Space`), but once one does, spaces may separate any two tokens
/// in it — `hashtable<string, :dyn drawable>` has one after the comma and
/// another after `:dyn`.
fn skip_space(toks: &mut Toks<'_>) {
    while matches!(toks.peek(), Some(NameTok::Space)) {
        toks.next();
    }
}

/// One generic argument. Almost always an ordinary qualified/generic type
/// name, but two other spellings appear here: `()`, the unit type (as in
/// `Result<(), FileError>`), and `:dyn Trait` — the nested spelling of the
/// same trait-object type `parse_dyn_type` builds from the datum-level form,
/// e.g. the element type of `Vector<:dyn Drawable>`.
fn parse_type_arg<'a>(toks: &mut Toks<'a>, rec: &mut Option<SpanRec<'_>>) -> Result<Type, Error> {
    skip_space(toks);
    // `()` is the whole argument; it has no name to record and no arguments
    // of its own, so it never reaches `parse_qualified_generic`.
    if matches!(toks.peek(), Some(NameTok::Unit)) {
        toks.next();
        return Ok(Type::Unit);
    }
    if matches!(toks.peek(), Some(NameTok::Ident(s)) if *s == ":dyn") {
        toks.next(); // `:dyn`
        let (segs, pins) = parse_qualified_generic(toks, rec, true)?;
        return Ok(Type::Dyn(Path::from_segments(segs), pins));
    }
    let (segs, args) = parse_qualified_generic(toks, rec, false)?;
    Ok(named_or_primitive(segs, args))
}

/// The primitive type a bare single-segment name denotes, if any — the one
/// authoritative name table behind [`named_or_primitive`] and the span
/// recorder's "is this a nominal name at all" test.
fn primitive_by_name(name: &str) -> Option<Type> {
    Some(match name {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "isize" => Type::Isize,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "usize" => Type::Usize,
        "f32" => Type::F32,
        "f64" => Type::F64,
        "bignum" => Type::Bignum,
        "ratio" => Type::Ratio,
        "random-state" => Type::RandomState,
        "bool" => Type::Bool,
        "char" => Type::Char,
        "string" => Type::Str,
        "symbol" => Type::Symbol,
        "!" => Type::Never,
        _ => return None,
    })
}

/// A non-generic, single-segment name names a primitive; anything else is a
/// nominal [`Type::Named`].
fn named_or_primitive(segs: Vec<String>, args: Vec<Type>) -> Type {
    if args.is_empty() && segs.len() == 1 {
        if let Some(t) = primitive_by_name(&segs[0]) {
            return t;
        }
    }
    Type::Named(Path::from_segments(segs), args)
}
