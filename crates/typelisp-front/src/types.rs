//! typelisp's type representation and parsing of type expressions.
//!
//! Types appear in source as `Sexpr`: a symbol such as `i32`, `bool`, `String`,
//! or a generic written without spaces like `Option<i32>` / `Vec<String>` /
//! `Pair<K,V>` (read as a *single* symbol token, so generics are split out of
//! the symbol's name here), the empty list `()` for the unit type, a list
//! `(fn (param-types...) ret-type)` for a function type, or the *applied*
//! spelling of a generic, `(pair k v)` — see [`parse_applied_type`] for why
//! that second spelling exists.
//!
//! Symbols are case-folded by the reader, so all type names are lowercase here.

use std::fmt;

use typelisp_read::name_lexer::{NameLexer, NameTok};
use crate::{wk, Error, Heap, Loc, PathId, SymRef, Value};

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
    let segs: Vec<SymRef> = p
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
pub const NATIVE_LOWERED_PRIMITIVES: [&str; 17] = [
    "i32", "i8", "i16", "u8", "u16", "u32", "int",
    "char", "string", "f64", "f32", "ratio", "sexpr", "bool", "symbol",
    // The two C-boundary words are integer receivers to `compile-assoc`
    // (`int-receiver-type?`), so their conversions are lowered the same way.
    // Listing the *methods* is not enough: a receiver missing from here makes
    // every one of its calls a real graph edge, and a builtin with no user
    // body then fails the `has_method` check with "no compiled
    // implementation" — which is exactly what a compiled `(as int ...)` on
    // a `c-ulong` did.
    "c-long", "c-ulong",
];

#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Type {
    I8,
    I16,
    I32,
    U8,
    U16,
    U32,
    F32,
    F64,
    /// The `int` type: CL's `integer` — an arbitrary-precision integer whose
    /// arithmetic promotes from the 63-bit fixnum to a heap bignum when a
    /// result outgrows it, and demotes back when it fits. Unlike the six
    /// fixed widths above it never wraps and never normalizes. A value of it
    /// is a box only when it has to be (`Heap::canonical_int`) — the old
    /// `bignum` type, always boxed, was folded into this one.
    ///
    /// Its runtime carrier is the tagged word itself (`Repr::Int`), so a
    /// fixnum and a bignum of this type are told apart by the word's tag
    /// and nothing else.
    Int,
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
    /// Its runtime value is a `Value::Symbol(SymRef)` (the same carrier a
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
    /// (`docs/dev/language-design.md` §5.2). The [`Path`] is the *trait*'s
    /// fully-qualified path (traits
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
    /// An opaque C pointer — `void *`, `FILE *`, whatever the declaration
    /// meant. Only ever produced or consumed by a `defffi` declaration, and
    /// only inside `(unsafe ...)`.
    ///
    /// **A raw machine word, not a tagged one.** Every other heap-bearing
    /// value in this language crosses the compiled boundary with a 3-bit tag
    /// in its low bits; a pointer cannot, because it needs all 64. That is
    /// what confines it: it may not be a `defstruct` field, a `defvar`, or a
    /// type argument (`Vector<ptr>`), because each of those tags what it
    /// stores. See [`crate::check::repr::Repr::RawWord`].
    ///
    /// The collector does not trace it, which is right — it points outside the
    /// heap entirely — and is the other half of why it is confined.
    Ptr,
    /// C's `long` / `int64_t` / `intptr_t`, and [`Self::CULong`] its unsigned
    /// twin (`unsigned long`, `size_t`, `uint64_t`).
    ///
    /// Named for C rather than spelled `i64`/`u64` on purpose. This language
    /// deliberately has no 64-bit integer type — a tagged immediate has 61
    /// bits (see [`Self::is_integer`]) — and the name says which of the two
    /// things it is: a word on the way to or from C, not an integer of this
    /// language. It is deliberately *not* in `INT_TYPE_NAMES`, so it carries
    /// no arithmetic: to compute with one, convert (`as i32`, `as bignum`).
    ///
    /// LP64 is assumed, which every Unix this workspace supports uses (the
    /// `#[cfg(unix)]` split in `typelisp_rt::os` is the same assumption).
    CLong,
    CULong,
}

impl Type {
    /// The six built-in integer types (`i8`/`i16`/`i32`, `u8`/`u16`/`u32`).
    /// The one authoritative list, so callers that need to single out "an
    /// integer type" (`Checker::int_lit_ty`/pattern checking, `core_bridge`'s
    /// struct-field classifier) share it instead of each re-enumerating all
    /// six variants.
    ///
    /// There is deliberately no 64-bit-wide integer type: a runtime value is
    /// a tagged word whose low 3 bits are the tag, so an immediate integer
    /// has 61 bits — a type that claims 64 would have to lose the top 3
    /// somewhere. Wider-than-`i32` arithmetic is `bignum`'s job.
    /// The two C-boundary word types. Not integers of this language (see
    /// [`Self::is_integer`]) — they carry no arithmetic — but convertible to
    /// and from one, which is the only way to compute with what C handed over.
    pub fn is_c_word(&self) -> bool {
        matches!(self, Type::CLong | Type::CULong)
    }

    /// The six fixed-width integer types — the ones with a width to
    /// normalize to. `int` is deliberately not one of them: it has no width,
    /// and everything this predicate gates (range checks, `normalize_int`,
    /// the `wsig` operand) is about a width.
    pub fn is_integer(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::U8 | Type::U16 | Type::U32)
    }

    /// The fixed widths and `int` together — every type whose values are
    /// integers of this language (the C words are still not: they carry no
    /// arithmetic).
    pub fn is_int_family(&self) -> bool {
        self.is_integer() || matches!(self, Type::Int)
    }

    /// The two built-in floating-point types (`f32`/`f64`) — the float
    /// counterpart of [`Type::is_integer`].
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }
}

/// Every integer type's registry name, in the one order the whole codebase
/// spells them. The list exists because "which types share the integer
/// catalog" is asked in four places that must not drift apart: the registry
/// (which methods a receiver has), the interpreter (which arm runs them), the
/// compile bridge's native-method table, and the island's own dispatch
/// predicate.
pub const INT_TYPE_NAMES: [&str; 6] = ["i8", "i16", "i32", "u8", "u16", "u32"];

/// The two C-boundary word types, which are 64 bits wide and deliberately
/// *not* in [`INT_TYPE_NAMES`]: they have no arithmetic, only conversions.
/// Kept as their own list so the places that mean "an integer of this
/// language" and the places that mean "a machine word of a known width" stay
/// distinguishable.
pub const C_WORD_TYPE_NAMES: [&str; 2] = ["c-long", "c-ulong"];

/// The bit width and signedness `name` claims, for the integer type names in
/// [`INT_TYPE_NAMES`].
///
/// A type name says exactly two things and nothing else — how many bits, and
/// whether the top one is a sign — and this is where both are read off. Both
/// execution engines carry every integer in a 64-bit word, so this pair is
/// what tells them where to cut it back after an operation: the value in the
/// word is always the one the type claims, sign-extended (signed) or
/// zero-extended (unsigned) into the rest. Keeping that invariant is what
/// lets a comparison, a division and a remainder stay ordinary *signed*
/// 64-bit instructions for the unsigned types too — every width here is at
/// most 32, so a normalized unsigned value is a non-negative `i64`.
pub fn int_width_signed(name: &str) -> Option<(u32, bool)> {
    // The C words first: 64 bits under LP64, which is every platform this
    // workspace builds for. They are not in `INT_TYPE_NAMES`, so the callers
    // that ask "is this an integer of this language" still say no — but the
    // ones that ask "how wide is this word" need an answer, because the width
    // casts and the FFI thunk are both driven by it.
    match name {
        "c-long" => return Some((64, true)),
        "c-ulong" => return Some((64, false)),
        _ => {}
    }
    Some(match name {
        "i8" => (8, true),
        "i16" => (16, true),
        "i32" => (32, true),
        "u8" => (8, false),
        "u16" => (16, false),
        "u32" => (32, false),
        _ => return None,
    })
}

/// `v` cut back to `width` bits and re-extended into the 64-bit word both
/// engines carry integers in. See [`int_width_signed`] for why this is the
/// one invariant.
///
/// Defined in `typelisp-mem` rather than here, because the heap enforces it
/// too when it boxes a narrow integer (`Heap::alloc_narrow`) and an
/// invariant with two statements of it has two chances to drift.
pub use typelisp_mem::normalize_int;

/// [`INT_TYPE_NAMES`]'s float counterpart.
pub const FLOAT_TYPE_NAMES: [&str; 2] = ["f32", "f64"];

/// All primitive value types that can be a `defmethod` receiver (every
/// variant [`prim_type_path`] maps to a `Path`). Used to pre-register each
/// one's (empty) method table in [`crate::Registry::with_builtins`].
pub fn primitive_types() -> Vec<Type> {
    vec![
        Type::I8, Type::I16, Type::I32,
        Type::U8, Type::U16, Type::U32,
        Type::Int,
        Type::F32, Type::F64, Type::Ratio, Type::RandomState,
        Type::Bool, Type::Char, Type::Str, Type::Symbol,
        // Registered so their conversions have somewhere to live. What they
        // get is `c_word_assoc`, not `int_assoc` — see that function.
        Type::CLong, Type::CULong,
    ]
}

/// The registry [`Path`] a primitive `Type` is addressed by when used as a
/// `defmethod` receiver (e.g. `Type::I32` -> `i32`). Mirrors the surface
/// keywords [`parse_type_name`] recognizes, so `(defmethod m ((self i32)) ...)`
/// and this lookup agree on the same name. `None` for `Unit`/`Never`/`Named`/
/// `Fn`, which are not primitive value types with their own method table here.
pub fn prim_type_path(ty: &Type) -> Option<Path> {
    let name = match ty {
        // `ptr` is genuinely opaque — nothing to call on it. The two C words
        // are addressable, because their conversions hang here; what they do
        // *not* get is arithmetic (see `registry::c_word_assoc`).
        Type::Ptr => return None,
        Type::CLong => "c-long",
        Type::CULong => "c-ulong",
        Type::I8 => "i8",
        Type::I16 => "i16",
        Type::I32 => "i32",
        Type::U8 => "u8",
        Type::U16 => "u16",
        Type::U32 => "u32",
        Type::F32 => "f32",
        Type::F64 => "f64",
        Type::Int => "int",
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
        Value::Cons(_) if is_fn_form(heap, v) => parse_fn_type(heap, v, out),
        Value::Cons(_) => parse_applied_type(heap, v, out),
        other => Err(Error::TypeError(format!("not a type expression: {:?}", other))),
    }
}

/// Whether `v` is a `(:dyn ...)` list.
pub fn is_dyn_form(heap: &Heap, v: Value) -> bool {
    matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::DYN))
}

/// Whether `v` is a `(fn ...)` list. Only the head is examined, so a
/// malformed function type still reaches [`parse_fn_type`] and is reported as
/// one rather than as a mis-shaped type application.
fn is_fn_form(heap: &Heap, v: Value) -> bool {
    matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::FN))
}

/// Parse the *applied* spelling of a generic type — `(vector char)`, the
/// list form of `vector<char>`.
///
/// The two spell the same type and parse to the same [`Type`]. The name form
/// is what a program is normally written in; this one exists because a type
/// *argument* is a whole type expression, and the name grammar
/// ([`parse_qualified_generic`]) can only spell arguments that are themselves
/// names, `()`, or `:dyn` — never a `(fn ...)`. Substituting a trait's
/// associated type into a signature has to be able to produce whatever the
/// `impl` bound it to, so `Checker::subst_value` emits this form (see
/// `check::forms::type_to_form`) instead of splicing text into a symbol's
/// name.
///
/// The head is parsed as an ordinary type expression so a qualified spelling
/// (`geo::pair`) works and its span is recorded like any other name; it must
/// come out as a nominal type with no arguments of its own, since its
/// arguments are exactly what the list supplies.
fn parse_applied_type(heap: &Heap, v: Value, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    if elems.len() < 2 {
        return Err(Error::TypeError(
            "a type application must be (Name Arg...), with at least one argument".to_string(),
        ));
    }
    let head = match parse_type_spanned(heap, elems[0].0, elems[0].1.as_ref(), out)? {
        Type::Named(p, args) if args.is_empty() => p,
        other => {
            return Err(Error::TypeError(format!(
                "a type application's head must be a plain type name, found `{:?}`",
                other
            )))
        }
    };
    let mut args = Vec::with_capacity(elems.len() - 1);
    for (a, l) in &elems[1..] {
        args.push(parse_type_spanned(heap, *a, l.as_ref(), out)?);
    }
    Ok(Type::Named(head, args))
}

/// [`parse_type`] for a type written as a bare *name* string — the spelling
/// that reaches the checker as one symbol because the reader reads
/// `vector<char>` as a single token. Records no spans; when a span is
/// available the caller has a `Value` and wants [`parse_type_spanned`].
///
/// Stricter than the parse behind [`parse_type_spanned`] in one way: the
/// whole string has to be the type. `parse_qualified_generic` stops at the
/// `>` that closes the argument list and its caller ignores whatever
/// follows, which is harmless when a *type* is what was written — but this
/// entry point is asked "is this name a type at all?" about symbols that may
/// be nothing of the kind, and `vector<t>::new` (a method path) must not
/// come back as the type `vector<t>`.
pub fn parse_type_name(name: &str) -> Result<Type, Error> {
    let mut toks = Toks::new(name);
    let ty = parse_qualified_generic(&mut toks, &mut None, false)
        .map(|(segs, args)| named_or_primitive(segs, args))?;
    skip_space(&mut toks);
    match toks.peek() {
        None => Ok(ty),
        Some(_) => Err(Error::TypeError(format!("`{}`: unexpected tokens after the type name", name))),
    }
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
        Value::Symbol(id) if id.is(wk::FN) => {}
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
        .position(|(p, _)| matches!(p, Value::Symbol(id) if id.is(wk::REST)));
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
            loc: Loc::new(std::sync::Arc::clone(&self.base.file), self.base.line, start)
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
///
/// Public because the registry builds the width-conversion methods
/// (`int->u8` and friends) by walking [`INT_TYPE_NAMES`]: those names have to
/// become `Type`s, and this is where a name becomes a type.
pub fn primitive_by_name(name: &str) -> Option<Type> {
    Some(match name {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "f32" => Type::F32,
        "f64" => Type::F64,
        "int" => Type::Int,
        "ratio" => Type::Ratio,
        "random-state" => Type::RandomState,
        "bool" => Type::Bool,
        "char" => Type::Char,
        "string" => Type::Str,
        "symbol" => Type::Symbol,
        "!" => Type::Never,
        // The FFI's raw words. Spellable anywhere a type is, and usable only
        // inside `(unsafe ...)` — that check is the checker's, not the
        // parser's, because it is about the expression and not the spelling.
        "ptr" => Type::Ptr,
        "c-long" => Type::CLong,
        "c-ulong" => Type::CULong,
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
