//! `Path`, the name of a definition, the standard modules paths are read in,
//! and resolving a written path.

use std::fmt;
use typelisp_mem::symbols::{self, NsId};
use crate::{Error, Heap, Loc, PathId, SymRef, Value};

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

    /// The definition `name` names: the one in its home module.
    pub fn named_by(name: SymRef) -> Path {
        Path { ns: name.home(), name }
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
    /// (`wk::OPTION` ...), in the module that defines it, which the symbol
    /// belongs to: `%core` for the LLVM handle types, which only the compiler
    /// island works with, and otherwise a module of the standard library.
    pub fn builtin(ty: u32) -> Path {
        Path::named_by(symbols::well_known_symbol(ty))
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
    /// name (`typeck::define` refuses a second one, and a test of
    /// `typeck::builtins` holds the built-in definitions to it).
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
/// (`typeck::resolve`); a `::` path is its last symbol in the module the
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

/// Where the last segment of `path` is, in a name written at `l` that ends
/// with it: none when `l` is not one line long enough to hold it.
pub(crate) fn last_segment_span(path: &Path, l: &Loc) -> Option<Loc> {
    let last = path.last_segment().chars().count() as u32;
    (l.end_line == l.line && l.end_col >= l.col + last)
        .then(|| Loc::new(std::sync::Arc::clone(&l.file), l.line, l.end_col - last).with_end(l.line, l.end_col))
}
