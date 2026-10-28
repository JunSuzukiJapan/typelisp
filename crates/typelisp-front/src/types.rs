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

use crate::{wk, Error, Heap, Loc, PathId, SymRef, Value};
use typelisp_mem::symbols::{self, NsId};

/// The standard library's module, the root's child `std`: the prelude, the
/// natives and the core macro layer are defined in it.
pub const STD_MODULE: &str = typelisp_mem::symbols::STD_MODULE;

/// The module the REPL's top level is written in — CL's `CL-USER`.
pub const USER_MODULE: &str = "user";

/// The child of [`STD_MODULE`] the standard library keeps its internal
/// helpers in: the prelude's own `defun`s a user never writes, the natives
/// under them, and what a library macro's expansion calls. The `%` keeps the
/// name out of the way of a user's own modules.
pub const INTERNAL_MODULE: &str = "%internal";

/// The child of [`STD_MODULE`] whose re-exports every module reads without a
/// `use`.
pub const PRELUDE_MODULE: &str = "prelude";

/// The name of a definition: the module it is in and the symbol its name
/// reads as there.
///
/// Compared by pointer: two paths are the same when their modules are the
/// same node of the module tree ([`NsId`]) and their names the same symbol.
/// A module's symbol table never changes what a name already reads as, so a
/// path built twice from the same segments is the same path. The segments
/// (`geo::point` is `["geo", "point"]`) are not stored; they are read back
/// from the tree, for display and for writing a path out of the process.
///
/// `Ord` compares the segments, so a `Path`-keyed table can be a `BTreeMap`
/// whose order is the same in every run: several of the registry's are,
/// because a dump hashes what a table *would serialize to* to decide what
/// changed, and an order that differs run to run (a `HashMap`'s, or the
/// symbols' addresses) would make every entry look modified.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Path {
    ns: NsId,
    name: SymRef,
}

impl Path {
    /// `name` in the module `ns`.
    pub fn in_ns(ns: NsId, name: &str) -> Path {
        Path { ns, name: symbols::intern_in(ns, name) }
    }

    /// The definition `name` names in the module `ns`: `name` is a symbol
    /// that reads as itself there — a key of one of `ns`'s tables.
    pub fn of_symbol(ns: NsId, name: SymRef) -> Path {
        Path { ns, name }
    }

    /// The module `ns`, named as a definition of the module it is a child of.
    pub fn of_module(ns: NsId) -> Path {
        let parent = symbols::ns_parent(ns).expect("the root and the system module are not named");
        Path { ns: parent, name: symbols::ns_name(ns).expect("a module with a parent has a name") }
    }

    /// Build a path from explicit segments.
    pub fn of(segments: &[&str]) -> Path {
        let (name, modules) = segments.split_last().expect("a path must have at least one segment");
        Path::in_ns(module_ns(modules.iter().copied()), name)
    }

    /// A root-level name.
    pub fn root(name: &str) -> Path {
        Path::in_ns(NsId::ROOT, name)
    }

    /// `name` inside the library's internal module (`std::%internal`).
    pub fn internal(name: &str) -> Path {
        Path::in_ns(internal_ns(), name)
    }

    /// The internal builtin whose name is the vocabulary symbol `k`
    /// (`wk::F_C_ALLOC` ...), in the library's internal module.
    pub fn internal_builtin(k: u32) -> Path {
        Path::of_symbol(internal_ns(), symbols::well_known_symbol(k))
    }

    /// The built-in type whose name is the vocabulary symbol `ty`
    /// (`wk::OPTION` ...), in the module that defines it: `%core` for the
    /// LLVM handle types, which only the compiler island works with, and
    /// otherwise the module of the standard library
    /// `StdModule::of_builtin_type` answers.
    pub fn builtin(ty: u32) -> Path {
        let ns = if LLVM_HANDLE_TYPES.contains(&ty) { NsId::CORE } else { typelisp_mem::StdModule::of_builtin_type(ty).ns() };
        Path { ns, name: symbols::well_known_symbol(ty) }
    }

    /// The primitive type `name` (`int`, `i32`, `string` ...), which
    /// `std::primitive` defines.
    pub fn primitive(name: &str) -> Path {
        Path::in_ns(typelisp_mem::StdModule::PRIMITIVE.ns(), name)
    }

    /// `name` inside the standard library's module, `std::name`.
    pub fn std(name: &str) -> Path {
        Path::in_ns(NsId::STD, name)
    }

    /// The symbol of the free builtin this path names, if it names one where
    /// builtins are registered: `std` or a module inside it. The same last
    /// segment anywhere else is somebody's own definition.
    pub fn std_builtin(&self) -> Option<SymRef> {
        in_std(self.ns).then_some(self.name)
    }

    /// Whether this path is inside the standard library — `std` or a module
    /// in it — where the built-in types are defined. A type's name there is
    /// its identity: no two of the library's modules define a type of one
    /// name ([`crate::check::Registry::reexport_std_in_prelude`] asserts it).
    pub fn is_std(&self) -> bool {
        in_std(self.ns)
    }

    /// Whether this path is directly inside the library's internal module.
    pub fn is_internal(&self) -> bool {
        self.ns == internal_ns()
    }

    /// Build a path from owned segments (must be non-empty).
    pub fn from_segments(segments: Vec<String>) -> Path {
        let (name, modules) = segments.split_last().expect("a path must have at least one segment");
        Path::in_ns(module_ns(modules.iter().map(String::as_str)), name)
    }

    /// The segments from the root: the module's, then the name.
    pub fn segments(&self) -> Vec<String> {
        let mut segs = symbols::ns_path(self.ns);
        segs.push(self.last_segment().to_string());
        segs
    }

    /// The module the definition is in.
    pub fn ns(&self) -> NsId {
        self.ns
    }

    /// The symbol the name reads as in [`Self::ns`].
    pub fn name_symbol(&self) -> SymRef {
        self.name
    }

    /// The final segment on its own — **not** the type's identity.
    ///
    /// A type is identified by its *whole* path: `m::vector` and the built-in
    /// `vector` are different types that share a last segment. Two bugs came
    /// from forgetting that (see [`path_is_builtin`] and
    /// `crate::type_key`), so this is deliberately named after what it
    /// returns rather than "the local name". Legitimate uses are name-table
    /// lookups paired with [`Path::parent`] (a module's tables are keyed by
    /// last segment), and display/derived names.
    ///
    /// To ask "is this the built-in `X`?" use [`path_is_builtin`]; to spell a
    /// type's runtime identity use `crate::type_key::type_key_of`.
    pub fn last_segment(&self) -> &'static str {
        self.name.name()
    }

    /// The module's segments (everything but the final segment).
    pub fn parent(&self) -> Vec<String> {
        symbols::ns_path(self.ns)
    }

    /// True for a root-level name.
    pub fn is_simple(&self) -> bool {
        self.ns == NsId::ROOT
    }
}

/// A module field of a serialized struct, written as the module's segments
/// from the root — the one form that means the same thing in another
/// process, as with [`Path`]. For `#[serde(with = "ns_as_segments")]`.
pub mod ns_as_segments {
    use typelisp_mem::symbols::{self, NsId};

    pub fn serialize<S: serde::Serializer>(ns: &NsId, s: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&symbols::ns_path(*ns), s)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<NsId, D::Error> {
        let segments: Vec<String> = serde::Deserialize::deserialize(d)?;
        Ok(symbols::ns_of(&segments))
    }
}

/// The module `segments` names from the root, each segment as written: a
/// module's name keeps its case, a file's module included (`Shapes.typl` is
/// the module `Shapes`).
fn module_ns<'a>(segments: impl Iterator<Item = &'a str>) -> NsId {
    let mut ns = NsId::ROOT;
    for seg in segments {
        ns = symbols::child_ns(ns, seg);
    }
    ns
}

/// The library's internal module, `std::%internal`.
pub fn internal_ns() -> NsId {
    static NS: std::sync::OnceLock<NsId> = std::sync::OnceLock::new();
    *NS.get_or_init(|| symbols::child_ns(NsId::STD, INTERNAL_MODULE))
}

/// Whether `ns` is the standard library's module or one inside it.
pub fn in_std(ns: NsId) -> bool {
    std::iter::successors(Some(ns), |m| symbols::ns_parent(*m)).any(|m| m == NsId::STD)
}

/// The standard library's child module `segment`, `std::segment`.
pub fn std_module(segment: &str) -> NsId {
    symbols::child_ns(NsId::STD, segment)
}

/// The printer control variable `name` (`*print-base*` ...), which the
/// prelude defines in `std::fmt`.
pub fn printer_global(name: &str) -> Path {
    Path::in_ns(std_module("fmt"), name)
}

/// The module every module reads its re-exports from, `std::prelude`.
pub fn prelude_ns() -> NsId {
    static NS: std::sync::OnceLock<NsId> = std::sync::OnceLock::new();
    *NS.get_or_init(|| symbols::child_ns(NsId::STD, PRELUDE_MODULE))
}

impl PartialOrd for Path {
    fn partial_cmp(&self, other: &Path) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Path {
    fn cmp(&self, other: &Path) -> std::cmp::Ordering {
        if self == other {
            return std::cmp::Ordering::Equal;
        }
        self.segments().cmp(&other.segments())
    }
}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Path").field(&self.segments()).finish()
    }
}

/// Written out as its module's segments and its name's symbol
/// ([`crate::owned_form::OwnedSym`]), the one form that means the same thing
/// in another process.
impl serde::Serialize for Path {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_newtype_struct("Path", &(symbols::ns_path(self.ns), crate::owned_form::OwnedSym::of(self.name)))
    }
}

impl<'de> serde::Deserialize<'de> for Path {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Path, D::Error> {
        struct Written;
        impl<'de> serde::de::Visitor<'de> for Written {
            type Value = Path;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a path's module and name")
            }
            fn visit_newtype_struct<D: serde::Deserializer<'de>>(self, d: D) -> Result<Path, D::Error> {
                let (ns, name): (Vec<String>, crate::owned_form::OwnedSym) = serde::Deserialize::deserialize(d)?;
                Ok(Path::of_symbol(symbols::ns_of(&ns), name.symbol()))
            }
        }
        d.deserialize_newtype_struct("Path", Written)
    }
}

/// The `::`-joined form — **the** spelling of a type's identity. Everything
/// that stores or compares a type by name (a heap value's type name, a
/// registry key, an error message) uses this, never [`Path::last_segment`].
impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.segments().join("::"))
    }
}

/// Whether `p` is the built-in type whose name is the vocabulary symbol `ty`
/// (`wk::VECTOR`, `wk::INT` ...): compared by symbol, never by spelling.
///
/// Every built-in (`Vector`/`HashTable`/`Option`/`Result`/`llvm-*`/`Scope`/the
/// primitives) is registered in the standard library
/// (`Registry::with_builtins`), so a path anywhere else can never be one
/// however its name reads — which is the whole point of the `is_std` half: a
/// `(shadow Vector)` module's own `Vector` is another symbol, and compiled
/// code that took it for the built-in would lower its methods to
/// `vector-op`, reading a user struct as if it were a `Vector`.
pub fn path_is_builtin(p: &Path, ty: u32) -> bool {
    builtin_type(p) == Some(ty)
}

/// The vocabulary symbol (`wk::VECTOR`, `wk::INT` ...) of the built-in type
/// `p` names, or `None` when `p` names none of them. What a table of
/// per-type behaviour is keyed by, so that no type is told by its spelling.
pub fn builtin_type(p: &Path) -> Option<u32> {
    let ty = p.name_symbol().well_known();
    let home = p.is_std() || (p.ns() == NsId::CORE && LLVM_HANDLE_TYPES.contains(&ty));
    (home && ty != symbols::NOT_WELL_KNOWN).then_some(ty)
}

/// [`path_is_builtin`] against a set of types — for the "one of the natively
/// lowered built-ins" tests, whose lists live as consts below so every site
/// reads the same list instead of spelling its own.
pub fn path_is_builtin_any(p: &Path, tys: &[u32]) -> bool {
    builtin_type(p).is_some_and(|ty| tys.contains(&ty))
}

/// The LLVM handle *types* (`crate::compile::Repr::Handle`):
/// values compiled code passes around as raw handles rather than heap boxes.
/// A superset of [`LLVM_METHOD_RECEIVER_TYPES`] — `llvm-basic-block` and
/// `llvm-value` are handles that carry no methods of their own.
pub const LLVM_HANDLE_TYPES: [u32; 5] =
    [wk::LLVM_MODULE, wk::LLVM_FUNCTION, wk::LLVM_BUILDER, wk::LLVM_BASIC_BLOCK, wk::LLVM_VALUE];

/// The LLVM handle types that *have* methods, i.e. the receivers whose
/// `assoc` lowers to an `llvm-op` node (`llvm_assoc_key`) and is
/// therefore never a real call target (`Interp::compile_scc`'s filters).
/// Deliberately not [`LLVM_HANDLE_TYPES`]: the two lists answer different
/// questions and used to differ only by accident.
pub const LLVM_METHOD_RECEIVER_TYPES: [u32; 4] = [wk::LLVM_MODULE, wk::LLVM_FUNCTION, wk::LLVM_BUILDER, wk::SCOPE];

/// The primitive receivers whose built-in methods `compile-assoc` turns into
/// LLVM instructions or `rt_*` calls rather than function calls
/// (`Interp::is_native_lowered_primitive_method` says *which* methods; this
/// says which receivers can have them).
pub const NATIVE_LOWERED_PRIMITIVES: [u32; 18] = [
    wk::I32, wk::I8, wk::I16, wk::U8, wk::U16, wk::U32, wk::INT,
    wk::CHAR, wk::STRING, wk::F64, wk::F32, wk::RATIO, wk::REPR_RATIONAL, wk::SEXPR, wk::BOOL, wk::SYMBOL,
    // The two C-boundary words are integer receivers to `compile-assoc`
    // (`int-receiver-type?`), so their conversions are lowered the same way.
    // Listing the *methods* is not enough: a receiver missing from here makes
    // every one of its calls a real graph edge, and a builtin with no user
    // body then fails the `has_method` check with "no compiled
    // implementation" — which is exactly what a compiled `(as int ...)` on
    // a `c-ulong` did.
    wk::C_LONG, wk::C_ULONG,
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
    /// The `rational` type: CL's `rational` — an `int` or a `ratio`. Its
    /// values are those two types' own values, kept normalized as CL's
    /// arithmetic leaves them (an integer-valued result is the `int`), so an
    /// `int` reaches a `rational` position as it stands and a `ratio` through
    /// `ratio->rational`, which normalizes (`Checker::reconcile_expected`).
    /// It is what `int`'s `/` answers, and what arithmetic mixing an `int`
    /// with a `ratio` answers.
    Rational,
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
    /// `gensym`/`intern` can be typed precisely instead of as the
    /// heterogeneous `Sexpr`. `symbol->string`/`intern` are the only
    /// bridges to/from `Str`.
    Symbol,
    /// The unit type `()`.
    Unit,
    /// The never / bottom type `!` (the type of `panic` and other diverging
    /// forms). It is compatible with — and absorbed by — any expected type.
    Never,
    /// A nominal type with type arguments: `Option<T>`, `Vec<T>`, `Sexpr`,
    /// user structs.
    Named(Path, Vec<Type>),
    /// A type variable, by the name its declaration wrote: a definition's
    /// `<T>`, a `defmethod` receiver's or an `impl` target's variables, a
    /// `deftrait`'s `Self` and `(:type Item)`. Never a definition, so never
    /// a [`Path`].
    Var(String),
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
    /// A typed C pointer, `(ptr T)`: the address of a `T` laid out as C lays
    /// it out, where `T` is one of the C scalar types, `ptr`, another typed
    /// pointer, or a `def-c-struct` ([`Self::CStruct`]).
    ///
    /// A raw machine word like [`Self::Ptr`], confined by the same rules and
    /// by one more: the memory it points at belongs to the `(unsafe ...)`
    /// that allocated it and is freed when that form is left, so a typed
    /// pointer can never be the value of an `unsafe` form, be captured by a
    /// closure, go to a task, or be thrown. One that comes from C — a
    /// callback's argument, a `defffi` result, a pointer field read back —
    /// must point into memory such an `unsafe` allocated, or it is an error.
    PtrTo(Box<Type>),
    /// A `def-c-struct`, by its fully-qualified path. Never the type of a
    /// value: it only ever appears as the pointee of a [`Self::PtrTo`].
    CStruct(Path),
}

/// The type as a program would write it — `Vector<int>`, `Option<Sexpr>`,
/// `:dyn Error`, `(fn (int) bool)` — for error messages and the LSP's hover.
///
/// `Debug` is the representation (`Named(Path(["vector"]), [Int])`), which
/// reads as an internal detail in front of a user; the registry key
/// ([`typelisp_mem::ConcreteNames::type_key`]) is the runtime's spelling and
/// writes a trait object as `dyn Trait`, which is not what the reader
/// accepts. The reader lowercases every name, so the built-in generic types
/// are given back the capitals the documentation spells them with. A path is
/// written the way the module being read can write it
/// ([`typelisp_mem::NameView`]), so this is never a type's identity.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn args(f: &mut fmt::Formatter, ts: &[Type]) -> fmt::Result {
            write!(f, "<")?;
            for (i, t) in ts.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "{}", t)?;
            }
            write!(f, ">")
        }
        // Written for the module the reader is in (`typelisp_mem::NameView`).
        let view = typelisp_print::shared::name_view();
        match self {
            // A tuple is written the way its values are, `#{int string}`.
            Type::Named(p, ts) if tuple_arity(p).is_some() => {
                write!(f, "#{{")?;
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, "}}")
            }
            Type::Named(p, ts) => {
                write!(f, "{}", view.definition(p.ns(), p.name_symbol()))?;
                if ts.is_empty() { Ok(()) } else { args(f, ts) }
            }
            Type::Var(name) => write!(f, "{}", name),
            Type::Dyn(p, pins) => {
                write!(f, ":dyn {}", view.definition(p.ns(), p.name_symbol()))?;
                if pins.is_empty() { Ok(()) } else { args(f, pins) }
            }
            Type::Fn(ps, rest, r) => {
                write!(f, "(fn (")?;
                for (i, t) in ps.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", t)?;
                }
                if let Some(t) = rest {
                    if !ps.is_empty() {
                        write!(f, " ")?;
                    }
                    write!(f, "&rest {}", t)?;
                }
                write!(f, ") {})", r)
            }
            Type::PtrTo(t) => write!(f, "{}", typelisp_mem::ConcreteNames::ptr_type(&t.to_string())),
            Type::CStruct(p) => write!(f, "{}", view.definition(p.ns(), p.name_symbol())),
            Type::Unit => write!(f, "()"),
            Type::Never => write!(f, "!"),
            // The scalars, each written as its name alone wherever the reader is.
            scalar => write!(f, "{}", scalar.scalar_name().expect("every other type is written above")),
        }
    }
}

/// The documented spelling of a built-in type whose name the reader has
/// lowercased, or `None` for every other name.
/// The type behind the tuple type `#{T0 T1 ...}` of `arity` elements (at most
/// [`typelisp_mem::TUPLE_MAX_ARITY`]): the
/// prelude's `std::%internal::tupleN<T0,...>`, a `defstruct` whose fields are
/// [`tuple_field`]`(0)`, `(1)`, .... Built on a struct so that everything a
/// generic struct already has — monomorphization, representations, `setf`,
/// trait impls — is a tuple's too.
pub fn tuple_path(arity: usize) -> Path {
    debug_assert!((1..=typelisp_mem::TUPLE_MAX_ARITY).contains(&arity), "no tuple type of arity {}", arity);
    Path::internal(&format!("tuple{}", arity))
}

/// The arity of the tuple type `path` names, or `None` when it names some
/// other type.
pub fn tuple_arity(path: &Path) -> Option<usize> {
    if !path.is_internal() {
        return None;
    }
    let name = path.last_segment();
    let n: usize = name.strip_prefix("tuple")?.parse().ok()?;
    (1..=typelisp_mem::TUPLE_MAX_ARITY).contains(&n).then_some(n)
}

/// The struct field holding a tuple's element `index`, which a program
/// reaches as `t::0`. The field cannot be named `0` itself: the reader reads
/// that as an integer.
pub fn tuple_field(index: usize) -> String {
    format!("%{}", index)
}

impl Type {
    /// The name a scalar type is defined under in the standard library's
    /// module (`int`, `string`, `ptr`), or `None` when `self` is not a
    /// scalar. Its key, its path and its written form are all this name.
    pub fn scalar_name(&self) -> Option<&'static str> {
        Some(match self {
            Type::Ptr => "ptr",
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
            Type::Rational => "rational",
            Type::RandomState => "random-state",
            Type::Bool => "bool",
            Type::Char => "char",
            Type::Str => "string",
            Type::Symbol => "symbol",
            Type::Unit | Type::Never | Type::Named(..) | Type::Var(_) | Type::Dyn(..) | Type::Fn(..) | Type::PtrTo(_) | Type::CStruct(_) => {
                return None
            }
        })
    }

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

/// Every integer type, in the one order the whole codebase lists them. The
/// list exists because "which types share the integer catalog" is asked in
/// places that must not drift apart: the registry (which methods a receiver
/// has), the interpreter (which arm runs them), the compile bridge's
/// native-method table, and the island's own dispatch predicate.
pub const INT_TYPES: [Type; 6] = [Type::I8, Type::I16, Type::I32, Type::U8, Type::U16, Type::U32];

/// The two C-boundary word types, which are 64 bits wide and deliberately
/// *not* in [`INT_TYPES`]: they have no arithmetic, only conversions.
/// Kept as their own list so the places that mean "an integer of this
/// language" and the places that mean "a machine word of a known width" stay
/// distinguishable.
pub const C_WORD_TYPES: [Type; 2] = [Type::CLong, Type::CULong];

/// The bit width and signedness an integer type claims ([`INT_TYPES`] and
/// [`C_WORD_TYPES`]).
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
pub fn int_width_signed(ty: &Type) -> Option<(u32, bool)> {
    Some(match ty {
        // The C words: 64 bits under LP64, which is every platform this
        // workspace builds for. They are not in `INT_TYPES`, so the callers
        // that ask "is this an integer of this language" still say no — but
        // the ones that ask "how wide is this word" need an answer, because
        // the width casts and the FFI thunk are both driven by it.
        Type::CLong => (64, true),
        Type::CULong => (64, false),
        Type::I8 => (8, true),
        Type::I16 => (16, true),
        Type::I32 => (32, true),
        Type::U8 => (8, false),
        Type::U16 => (16, false),
        Type::U32 => (32, false),
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


/// All primitive value types that can be a `defmethod` receiver (every
/// variant [`prim_type_path`] maps to a `Path`). Used to pre-register each
/// one's (empty) method table in [`crate::Registry::with_builtins`].
pub fn primitive_types() -> Vec<Type> {
    vec![
        Type::I8, Type::I16, Type::I32,
        Type::U8, Type::U16, Type::U32,
        Type::Int,
        Type::F32, Type::F64, Type::Ratio, Type::Rational, Type::RandomState,
        Type::Bool, Type::Char, Type::Str, Type::Symbol,
        // Registered so their conversions have somewhere to live. What they
        // get is `c_word_assoc`, not `int_assoc` — see that function.
        Type::CLong, Type::CULong,
    ]
}

/// The path a type's methods and `impl`s are registered under: a named
/// type's own path, a primitive's built-in name ([`prim_type_path`]). `None`
/// for a type with no name — a trait object, `()`, `!`, a function, a pointer
/// — which has no table of its own.
pub fn nominal_path(ty: &Type) -> Option<Path> {
    match ty {
        Type::Named(n, _) => Some(n.clone()),
        other => prim_type_path(other),
    }
}

/// The registry [`Path`] a primitive `Type` is addressed by when used as a
/// `defmethod` receiver (e.g. `Type::I32` -> `std::primitive::i32`). Mirrors the surface
/// names [`primitive_by_name`] recognizes, so `(defmethod m ((self i32)) ...)`
/// and this lookup agree on the same name. `None` for `ptr`, which is
/// genuinely opaque — nothing to call on it — and for every type that is not
/// a scalar. The two C words are addressable, because their conversions hang
/// here; what they do *not* get is arithmetic (see `registry::c_word_assoc`).
pub fn prim_type_path(ty: &Type) -> Option<Path> {
    match ty {
        Type::Ptr => None,
        other => other.scalar_name().map(Path::primitive),
    }
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

/// Parse a type expression (a read `Value`) into a [`Type`]. `ns` is the
/// module it is written in: a name in it is that module's ([`written_path`]).
pub fn parse_type(heap: &Heap, ns: NsId, v: Value) -> Result<Type, Error> {
    parse_type_spanned(heap, ns, v, None, &mut Vec::new())
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
    ns: NsId,
    v: Value,
    loc: Option<&Loc>,
    out: &mut Vec<TypeNameSpan>,
) -> Result<Type, Error> {
    match v {
        Value::Empty => Ok(Type::Unit),
        Value::Symbol(sym) => match scalar_of_symbol(sym) {
            Some(t) => Ok(t),
            None => Ok(Type::Named(record_name(Path::of_symbol(ns, sym), loc, false, out), Vec::new())),
        },
        Value::Path(_) => {
            let path = written_path(heap, ns, v)?;
            match scalar_type(&path) {
                Some(t) => Ok(t),
                None => Ok(Type::Named(record_name(path, loc, false, out), Vec::new())),
            }
        }
        // `(:dyn Trait)` — the reader's joined form of the two-word `:dyn
        // Trait` spelling (`read::reader::read_datum`). Checked before
        // the `(fn ...)` case since both are lists.
        Value::Cons(_) if is_dyn_form(heap, v) => parse_dyn_type(heap, ns, v, out),
        Value::Cons(_) if is_fn_form(heap, v) => parse_fn_type(heap, ns, v, out),
        Value::Cons(_) if is_ptr_form(heap, v) => parse_ptr_type(heap, ns, v, out),
        Value::Cons(_) if is_type_application(heap, v) => parse_type_application(heap, ns, v, out),
        Value::Cons(_) if matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::TYPE_MEMBER)) => {
            Err(Error::TypeError("`Name<Args>::member` names a member of a type, not a type".to_string()))
        }
        Value::Cons(_) => parse_applied_type(heap, ns, v, out),
        // `#{int string}` — the tuple type, written the way its values are.
        // The reader hands the elements over as a `#{..}` datum, whose
        // elements carry no source positions of their own.
        Value::Boxed(id) if heap.sexpr_tuple_arity(id).is_some() => {
            let n = heap.struct_field_count(id);
            let args = (0..n)
                .map(|i| parse_type_spanned(heap, ns, heap.struct_field(id, i), None, out))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Type::Named(tuple_path(n), args))
        }
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

/// Whether `v` is a `(ptr ...)` list — a typed C pointer.
fn is_ptr_form(heap: &Heap, v: Value) -> bool {
    matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::PTR))
}

/// Parse `(ptr T)` into [`Type::PtrTo`]. What `T` may be is the checker's
/// question (it needs the `def-c-struct` table); this only reads the shape.
fn parse_ptr_type(heap: &Heap, ns: NsId, v: Value, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    if elems.len() != 2 {
        return Err(Error::TypeError("a typed pointer is written (ptr T), with exactly one type".to_string()));
    }
    let pointee = parse_type_spanned(heap, ns, elems[1].0, elems[1].1.as_ref(), out)?;
    Ok(Type::PtrTo(Box::new(pointee)))
}

/// Parse the *applied* spelling of a generic type — `(vector char)`, the
/// list form of `Vector<char>`.
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
fn parse_applied_type(heap: &Heap, ns: NsId, v: Value, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    if elems.len() < 2 {
        return Err(Error::TypeError(
            "a type application must be (Name Arg...), with at least one argument".to_string(),
        ));
    }
    let head = match parse_type_spanned(heap, ns, elems[0].0, elems[0].1.as_ref(), out)? {
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
        args.push(parse_type_spanned(heap, ns, *a, l.as_ref(), out)?);
    }
    Ok(Type::Named(head, args))
}

/// Parse the reader-joined `(:dyn Trait)` / `(:dyn Trait<Pin,...>)` form. The
/// trait name is parsed with the ordinary type-name grammar, so a qualified
/// and/or generic-looking spelling works; its "generic arguments" are the
/// associated-type pins, not type arguments (see [`Type::Dyn`]).
fn parse_dyn_type(heap: &Heap, ns: NsId, v: Value, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    if elems.len() != 2 {
        return Err(Error::TypeError("`:dyn` must be followed by exactly one trait name".to_string()));
    }
    // Parse into a scratch vector first: the recorder writes the trait head
    // *before* any associated-type pins (see `parse_type_application`), so
    // its first entry — recorded as an ordinary `Named` head — is exactly the
    // name the `:dyn` reinterprets as a trait.
    let mut tmp = Vec::new();
    match parse_type_spanned(heap, ns, elems[1].0, elems[1].1.as_ref(), &mut tmp)? {
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
fn parse_fn_type(heap: &Heap, ns: NsId, v: Value, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
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
        Value::Cons(_) => parse_fn_params(heap, ns, &heap.list_to_vec_locs(elems[1].0)?, out)?,
        _ => return Err(Error::TypeError("fn parameter list must be a list".to_string())),
    };
    let ret = parse_type_spanned(heap, ns, elems[2].0, elems[2].1.as_ref(), out)?;
    Ok(Type::Fn(params, rest, Box::new(ret)))
}

/// Split a `(fn ...)` type's parameter-type list into fixed types and an
/// optional trailing `&rest elem-type` (the type of each variadic argument) —
/// the type-expression counterpart of `Checker::parse_params_rest`.
fn parse_fn_params(
    heap: &Heap,
    ns: NsId,
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
                .map(|(p, l)| parse_type_spanned(heap, ns, *p, l.as_ref(), out))
                .collect::<Result<_, _>>()?;
            let rest = parse_type_spanned(heap, ns, ps[i + 1].0, ps[i + 1].1.as_ref(), out)?;
            Ok((params, Some(Box::new(rest))))
        }
        None => {
            let params = ps
                .iter()
                .map(|(p, l)| parse_type_spanned(heap, ns, *p, l.as_ref(), out))
                .collect::<Result<_, _>>()?;
            Ok((params, None))
        }
    }
}

/// Where a `::` path leads: its last segment as read in the module the
/// segments before it reached, or a member of the type they reached.
#[derive(Clone, Copy, Debug)]
pub enum PathTarget {
    In(Path),
    /// The type, and the member's symbol.
    Member(Path, SymRef),
}

/// Where the `::` path `id`, written in `ns`, leads — followed by the
/// symbols it was read as (`SessionNames::reached`), never by its spelling.
pub fn path_target(heap: &Heap, ns: NsId, id: PathId) -> Result<PathTarget, Error> {
    let segs = heap.path_segments(id);
    let (last, prefix) = segs.split_last().expect("a path has a last segment");
    let names = heap.names();
    match names.reached(ns, prefix).map_err(Error::TypeError)? {
        symbols::PathStep::Module(m) => Ok(PathTarget::In(Path::of_symbol(m, *last))),
        symbols::PathStep::Type(ty) => {
            let home = names.type_home(ty).ok_or_else(|| {
                Error::TypeError(format!("`{}`: `{}` is a type no module defines", path_spelling(heap, id), ty.name()))
            })?;
            Ok(PathTarget::Member(Path::of_symbol(home, ty), *last))
        }
    }
}

/// How the path `id` is written, for messages.
pub fn path_spelling(heap: &Heap, id: PathId) -> String {
    heap.path_segments(id).iter().map(|s| heap.symbol_spelling(*s)).collect::<Vec<_>>().join("::")
}

/// The path a type name written in `ns` stands for, before the analysis
/// looks it up: a bare name is its symbol in `ns`, where the lookup starts
/// (`Info::find_written`); a `::` path is its last symbol in the module the
/// path leads to ([`path_target`]).
pub fn written_path(heap: &Heap, ns: NsId, v: Value) -> Result<Path, Error> {
    match v {
        Value::Symbol(sym) => Ok(Path::of_symbol(ns, sym)),
        Value::Path(id) => match path_target(heap, ns, id)? {
            PathTarget::In(p) => Ok(p),
            PathTarget::Member(..) => {
                Err(Error::TypeError(format!("`{}` names a member of a type, not a type", path_spelling(heap, id))))
            }
        },
        other => Err(Error::TypeError(format!("not a type name: {:?}", other))),
    }
}

/// Records where a written type name's last segment is — the name proper;
/// gives the path back. `loc` is the whole name's span; the last segment is
/// its tail on the same line.
fn record_name(path: Path, loc: Option<&Loc>, dyn_head: bool, out: &mut Vec<TypeNameSpan>) -> Path {
    if let Some(l) = loc {
        let last = path.last_segment().chars().count() as u32;
        if l.end_line == l.line && l.end_col >= l.col + last {
            out.push(TypeNameSpan {
                path,
                dyn_head,
                loc: Loc::new(std::sync::Arc::clone(&l.file), l.line, l.end_col - last).with_end(l.line, l.end_col),
            });
        }
    }
    path
}

/// Whether `v` is the reader's `(%type-apply Name Arg...)` — what
/// `Name<Arg,...>` reads as.
pub fn is_type_application(heap: &Heap, v: Value) -> bool {
    matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::TYPE_APPLY))
}

/// Parse `(%type-apply Name Arg...)` — `Name<Arg,...>` — into a generic
/// type. The name is recorded ahead of its arguments, so a recorded sequence
/// lists an outer name before everything nested in it (`parse_dyn_type`
/// relies on that).
fn parse_type_application(heap: &Heap, ns: NsId, v: Value, out: &mut Vec<TypeNameSpan>) -> Result<Type, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    if elems.len() < 3 {
        return Err(Error::TypeError("a generic type is written Name<Arg,...>, with at least one argument".to_string()));
    }
    let head = record_name(written_path(heap, ns, elems[1].0)?, elems[1].1.as_ref(), false, out);
    let mut args = Vec::with_capacity(elems.len() - 2);
    for (a, l) in &elems[2..] {
        args.push(parse_type_spanned(heap, ns, *a, l.as_ref(), out)?);
    }
    Ok(Type::Named(head, args))
}

/// The primitive type the symbol `sym` names, if any — the one authoritative
/// table behind [`parse_type`]'s "is this a nominal name at all" test, told
/// by the vocabulary symbol and never by its spelling: a module that
/// `shadow`s `int` has a symbol of its own, which names no primitive.
pub fn scalar_of_symbol(sym: SymRef) -> Option<Type> {
    Some(match sym.well_known() {
        wk::I8 => Type::I8,
        wk::I16 => Type::I16,
        wk::I32 => Type::I32,
        wk::U8 => Type::U8,
        wk::U16 => Type::U16,
        wk::U32 => Type::U32,
        wk::F32 => Type::F32,
        wk::F64 => Type::F64,
        wk::INT => Type::Int,
        wk::RATIO => Type::Ratio,
        wk::REPR_RATIONAL => Type::Rational,
        wk::REPR_RANDOM_STATE => Type::RandomState,
        wk::BOOL => Type::Bool,
        wk::CHAR => Type::Char,
        wk::STRING => Type::Str,
        wk::SYMBOL => Type::Symbol,
        wk::NEVER => Type::Never,
        // The FFI's raw words. Spellable anywhere a type is, and usable only
        // inside `(unsafe ...)` — that check is the checker's, not the
        // parser's, because it is about the expression and not the spelling.
        wk::PTR => Type::Ptr,
        wk::C_LONG => Type::CLong,
        wk::C_ULONG => Type::CULong,
        _ => return None,
    })
}

/// The primitive type `p` names, when it names one of `std::primitive`'s:
/// [`scalar_of_symbol`] for a path.
pub fn scalar_type(p: &Path) -> Option<Type> {
    if p.ns() == typelisp_mem::StdModule::PRIMITIVE.ns() {
        scalar_of_symbol(p.name_symbol())
    } else {
        None
    }
}
