//! The symbol table: process-global, permanent, and shaped like the module
//! tree.
//!
//! A symbol's identity is the **address of its [`Symbol`] header** — the same
//! thing CL means by comparing symbols with `eq`, and the same thing
//! [`super::value::ConsRef`] already is for a cons cell. Asking "is this
//! `&rest`?" is a pointer comparison; nothing reads the name back out to
//! answer it.
//!
//! Three properties hold, and each one is load-bearing:
//!
//! 1. **Global.** The table is not part of any `Heap`. `intern` returns the
//!    same address for the same name for the life of the process, so a symbol
//!    is meaningful across heaps — which matters because compiled code reaches
//!    symbols through a raw tagged word, with no heap handle in sight.
//! 2. **Permanent.** Nothing here is ever freed. Headers are `Box::leak`ed, so
//!    the `'static` lifetime [`SymRef::name`] hands out is real rather than
//!    asserted, and the address can never dangle. That is what makes the
//!    `unsafe` below simple enough not to need Miri watching it: there is no
//!    deallocation to get wrong.
//! 3. **Shaped like the module tree.** Each module owns a table of the symbols
//!    written inside it, so `m::foo` and `n::foo` are different symbols. See
//!    [`intern_in`] for the lookup rule that keeps `(module m (defun ...))`
//!    working anyway.
//!
//! ## Why an address and not an index
//!
//! An index needs a reverse table (`id -> name`) to print with, and is only
//! meaningful relative to the table that issued it. An address *is* the
//! header, so the name is one dereference away and no reverse table exists at
//! all. The tagged compiled-code representation carries the address directly
//! (`typelisp-abi`'s `encode`), exactly as it already does for `Cons`.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

// ---- the well-known vocabulary ------------------------------------------

/// Compile-time string equality, for [`sym_index`].
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Where `name` sits in [`BUILTIN_SYMBOLS`]. A name that is not in the table
/// is a compile error.
const fn sym_index(name: &str) -> u32 {
    let mut i = 0;
    while i < BUILTIN_SYMBOLS.len() {
        if str_eq(BUILTIN_SYMBOLS[i], name) {
            return i as u32;
        }
        i += 1;
    }
    panic!("a `wk` constant names a symbol that is not in BUILTIN_SYMBOLS")
}

/// Declares the compiler's fixed vocabulary, and one [`wk`] constant per entry.
///
/// One list, two outputs: the constant's value is *computed from* the table by
/// [`sym_index`] at compile time, so a constant and its name cannot drift
/// apart — a name that moves takes its constant with it, and a constant whose
/// name is not in the table fails to compile.
macro_rules! well_known_symbols {
    ($($section:literal $($konst:ident => $name:literal)+)+) => {
        /// The names the compiler compares symbols against — the head of a
        /// definition form, a lambda-list marker, a core-IR operator tag, a
        /// `loop` clause word. All interned into the root module when the
        /// table is created.
        ///
        /// This list lives in the memory crate rather than with the front end
        /// that gives each name meaning, because the table itself does. The
        /// crate does not interpret the names; it interns them, the way it
        /// interns any other string.
        ///
        /// Names must already be lowercase: interning folds case, so a name
        /// spelled otherwise would intern under a different one.
        pub const BUILTIN_SYMBOLS: &[&str] = &[$($($name,)+)+];

        /// One constant per [`BUILTIN_SYMBOLS`] entry: the value
        /// [`SymRef::well_known`] returns for that symbol.
        ///
        /// **These are not identities.** Two symbols are the same when their
        /// addresses are equal; a `wk` constant only answers "is this
        /// particular member of the fixed vocabulary?", so that a dispatch
        /// over that vocabulary can stay a `match` (an address cannot be a
        /// pattern). Every symbol outside the vocabulary reports
        /// [`NOT_WELL_KNOWN`], so `well_known()` must never be used to compare
        /// two symbols to each other — use `==` on the [`SymRef`]s.
        pub mod wk {
            $($(
                #[doc = $section]
                pub const $konst: u32 = super::sym_index($name);
            )+)+
        }
    };
}

/// What [`SymRef::well_known`] reports for a symbol outside the fixed
/// vocabulary — every user-written name, and everything `gensym` makes.
pub const NOT_WELL_KNOWN: u32 = u32::MAX;

well_known_symbols! {
    "Syntax the checker reads."
    QUOTE => "quote"
    UNQUOTE => "unquote"
    UNQUOTE_SPLICING => "unquote-splicing"
    THE => "the"
    FN => "fn"
    PUB => "pub"
    WHERE => "where"
    RETURN => "return"
    REST => "&rest"
    OPTIONAL => "&optional"
    KEY => "&key"
    DYN => ":dyn"
    EQUALS => "="
    COLON_EQUALS => ":="

    "Core-IR operator tags shared with the island (`compile-value`'s dispatch)."
    INT => "int"
    FLOAT => "float"
    BIGNUM => "bignum"
    RATIO => "ratio"
    CHAR => "char"
    BOOL => "bool"
    STR => "str"
    UNIT => "unit"
    VAR => "var"
    SET => "set"
    GLOBAL => "global"
    SET_GLOBAL => "set-global"
    LET => "let"
    LAMBDA => "lambda"
    LABELS => "labels"
    CALL => "call"
    ASSOC => "assoc"
    APPLY => "apply"
    IF => "if"
    LOOP => "loop"
    BREAK => "break"
    PANIC => "panic"
    MATCH => "match"
    CONSTRUCT => "construct"
    FIELD_GET => "field-get"
    FIELD_SET => "field-set"
    DYN_NEW => "dyn-new"
    DYN_UPCAST => "dyn-upcast"
    DYN_CALL => "dyn-call"
    DYN_VALUE => "dyn-value"
    CATCH => "catch"
    THROW => "throw"
    UNWIND_PROTECT => "unwind-protect"

    "Core-IR tags with no island counterpart: the bridge turns each into something else."
    SYM => "sym"
    FNREF => "fnref"
    METHODREF => "methodref"
    COMPILE_FN => "compile-fn"
    METHOD => "method"
    PAT_WILD => "pat-wild"
    PAT_EMPTY => "pat-empty"
    PAT_NONEMPTY => "pat-nonempty"
    PAT_BIND => "pat-bind"
    PAT_LIT => "pat-lit"
    PAT_GUARD => "pat-guard"
    PAT_CTOR => "pat-ctor"
    PAT_TYPETEST => "pat-typetest"

    "Core-IR top-level tags, and the definition-form heads they are lowered from."
    DEFUN => "defun"
    DEFMETHOD => "defmethod"
    DEFMACRO => "defmacro"
    DEFVAR => "defvar"
    DEFSTRUCT => "defstruct"
    DEFENUM => "defenum"
    MODULE => "module"
    USE => "use"
    LOAD => "load"
    EXPR => "expr"
    DEFSIGNATURE => "defsignature"
    DEFCONSTANT => "defconstant"
    DEFTRAIT => "deftrait"
    DEFTYPE => "deftype"
    IMPL => "impl"

    "`Sexpr`'s own constructors, as a downcast pattern's head can spell them."
    NIL => "nil"
    CONS => "cons"
    PATH => "path"

    "Forms the pretty printer lays out code-shaped, beyond those already above."
    PROGN => "progn"
    COND => "cond"
    AND => "and"
    OR => "or"
    LIST => "list"
    BLOCK => "block"
    WHEN => "when"
    UNLESS => "unless"
    WHILE => "while"
    LET_STAR => "let*"
    CASE => "case"
    SETF => "setf"
    AS => "as"
    DOLIST => "dolist"
    DOTIMES => "dotimes"
    DOITER => "doiter"
    UNTIL => "until"
    DO => "do"

    "Values a well-known global may hold, compared against as symbols."
    UPCASE => ":upcase"
    CAPITALIZE => ":capitalize"

    "The synthetic module `check_program` collects monomorphized specializations into."
    MONO_BUNDLE => "<monomorph specializations>"

    "`loop`'s clause words. Keywords, so distinct from the bare symbols above."
    KW_WITH => ":with"
    KW_FOR => ":for"
    KW_AS => ":as"
    KW_REPEAT => ":repeat"
    KW_INITIALLY => ":initially"
    KW_FINALLY => ":finally"
    KW_IN => ":in"
    KW_ACROSS => ":across"
    KW_ON => ":on"
    KW_FROM => ":from"
    KW_DOWNFROM => ":downfrom"
    KW_UPFROM => ":upfrom"
    KW_TO => ":to"
    KW_BELOW => ":below"
    KW_DOWNTO => ":downto"
    KW_ABOVE => ":above"
    KW_BY => ":by"
    KW_THEN => ":then"
    KW_DO => ":do"
    KW_DOING => ":doing"
    KW_COLLECT => ":collect"
    KW_COLLECTING => ":collecting"
    KW_APPEND => ":append"
    KW_APPENDING => ":appending"
    KW_SUM => ":sum"
    KW_SUMMING => ":summing"
    KW_COUNT => ":count"
    KW_COUNTING => ":counting"
    KW_MAXIMIZE => ":maximize"
    KW_MAXIMIZING => ":maximizing"
    KW_MINIMIZE => ":minimize"
    KW_MINIMIZING => ":minimizing"
    KW_ALWAYS => ":always"
    KW_NEVER => ":never"
    KW_THEREIS => ":thereis"
    KW_WHILE => ":while"
    KW_UNTIL => ":until"
    KW_RETURN => ":return"
    KW_WHEN => ":when"
    KW_IF => ":if"
    KW_UNLESS => ":unless"
    KW_ELSE => ":else"
    KW_INTO => ":into"
}

// ---- the symbol itself ---------------------------------------------------

/// An interned symbol's header — what a [`SymRef`] points at.
///
/// `align(8)` is not decoration: the tagged compiled-code representation packs
/// a 3-bit tag into the low bits of this address, so the header must be
/// 8-byte aligned or the tag would collide with the pointer. `Cell` relies on
/// the same property for `Cons`.
///
/// Every field is immutable once written, and the header is never freed.
#[repr(C, align(8))]
pub struct Symbol {
    /// The canonical (lowercase) name.
    name: Box<str>,
    /// The module whose table this symbol lives in.
    home: NsId,
    /// Index into [`BUILTIN_SYMBOLS`], or [`NOT_WELL_KNOWN`].
    well_known: u32,
}

/// An interned symbol: the address of its [`Symbol`] header, compared by
/// identity. The pointer counterpart of [`super::value::ConsRef`].
#[derive(Clone, Copy)]
pub struct SymRef(*const Symbol);

// SAFETY: a `Symbol` is written once, never mutated, and never freed, so the
// address is valid for the life of the process and reading through it from any
// thread races with nothing. This is what lets one global table serve every
// thread (`cargo test` runs many at once) — unlike a `Heap`, which is mutable
// and therefore stays thread-local.
unsafe impl Send for SymRef {}
unsafe impl Sync for SymRef {}

impl PartialEq for SymRef {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SymRef {}

impl std::hash::Hash for SymRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::fmt::Debug for SymRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SymRef({})", self.name())
    }
}

impl SymRef {
    fn header(&self) -> &'static Symbol {
        // SAFETY: the only way to obtain a `SymRef` is from the table, which
        // leaks every header it builds — so this address is always a live,
        // immutable `Symbol` (see the module doc comment's property 2).
        unsafe { &*self.0 }
    }

    /// The symbol's canonical (lowercase) name.
    ///
    /// `'static` for real, not asserted: headers are leaked. Reading a name is
    /// one dereference — there is no reverse table, which is the whole point
    /// of the identity being an address.
    pub fn name(&self) -> &'static str {
        &self.header().name
    }

    /// The module this symbol lives in.
    pub fn home(&self) -> NsId {
        self.header().home
    }

    /// Which member of the fixed vocabulary this is, or [`NOT_WELL_KNOWN`].
    ///
    /// For dispatching over that vocabulary with a `match` — see [`wk`] for
    /// why this exists and why it is never an identity test.
    pub fn well_known(&self) -> u32 {
        self.header().well_known
    }

    /// Whether this is the vocabulary member `konst` names.
    pub fn is(&self, konst: u32) -> bool {
        debug_assert!(konst != NOT_WELL_KNOWN, "`is` takes a `wk` constant, not NOT_WELL_KNOWN");
        self.header().well_known == konst
    }

    /// The raw address, as an opaque integer — for embedding in
    /// `typelisp-abi`'s tagged compiled-code representation, exactly as
    /// [`super::value::ConsRef::addr`] is. Never meaningful to do arithmetic
    /// on; only round-tripped through [`SymRef::from_addr`].
    pub fn addr(&self) -> usize {
        self.0 as usize
    }

    /// # Safety
    ///
    /// `addr` must have come from [`SymRef::addr`]. Since symbols are never
    /// freed, any address that ever was one still is.
    pub unsafe fn from_addr(addr: usize) -> SymRef {
        SymRef(addr as *const Symbol)
    }
}

// ---- the module tree -----------------------------------------------------

/// A module's node in the symbol table — an index, since the nodes live in one
/// `Vec` behind the lock. [`NsId::ROOT`] always exists.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NsId(u32);

impl NsId {
    /// The root module: where the prelude, the core macro layer, the island
    /// and every built-in name live, and the fallback every other module
    /// inherits from (see [`intern_in`]).
    pub const ROOT: NsId = NsId(0);
}

struct NsNode {
    /// This module's own symbols, by canonical name.
    syms: HashMap<Box<str>, SymRef>,
    /// Child modules, by segment — the same shape as the checker's
    /// `Namespace::modules`.
    children: HashMap<Box<str>, NsId>,
    parent: Option<NsId>,
    /// The segment this module was reached by, for diagnostics.
    segment: Box<str>,
}

struct SymTable {
    nodes: Vec<NsNode>,
}

impl SymTable {
    fn new() -> SymTable {
        let mut t = SymTable {
            nodes: vec![NsNode {
                syms: HashMap::new(),
                children: HashMap::new(),
                parent: None,
                segment: "".into(),
            }],
        };
        for (i, name) in BUILTIN_SYMBOLS.iter().enumerate() {
            t.create(NsId::ROOT, name, i as u32);
        }
        t
    }

    /// The symbol `name` names in `ns` or any module above it, if it exists.
    fn lookup(&self, ns: NsId, name: &str) -> Option<SymRef> {
        let mut cur = Some(ns);
        while let Some(id) = cur {
            let node = &self.nodes[id.0 as usize];
            if let Some(s) = node.syms.get(name) {
                return Some(*s);
            }
            cur = node.parent;
        }
        None
    }

    /// Build a symbol in `ns` unconditionally. The caller has already looked.
    fn create(&mut self, ns: NsId, name: &str, well_known: u32) -> SymRef {
        // Leaked on purpose: symbols are permanent, so this is the allocation
        // that gives `SymRef::name` a genuine `'static` and makes the address
        // impossible to dangle.
        let header: &'static Symbol =
            Box::leak(Box::new(Symbol { name: name.into(), home: ns, well_known }));
        let sym = SymRef(header as *const Symbol);
        self.nodes[ns.0 as usize].syms.insert(name.into(), sym);
        sym
    }

    fn child(&mut self, parent: NsId, segment: &str) -> NsId {
        if let Some(id) = self.nodes[parent.0 as usize].children.get(segment) {
            return *id;
        }
        let id = NsId(self.nodes.len() as u32);
        self.nodes.push(NsNode {
            syms: HashMap::new(),
            children: HashMap::new(),
            parent: Some(parent),
            segment: segment.into(),
        });
        self.nodes[parent.0 as usize].children.insert(segment.into(), id);
        id
    }

    fn path(&self, ns: NsId) -> Vec<String> {
        let mut segs = Vec::new();
        let mut cur = Some(ns);
        while let Some(id) = cur {
            let node = &self.nodes[id.0 as usize];
            if node.parent.is_some() {
                segs.push(node.segment.to_string());
            }
            cur = node.parent;
        }
        segs.reverse();
        segs
    }

    fn count(&self) -> usize {
        self.nodes.iter().map(|n| n.syms.len()).sum()
    }
}

static SYMBOLS: OnceLock<Mutex<SymTable>> = OnceLock::new();

fn table() -> MutexGuard<'static, SymTable> {
    SYMBOLS
        .get_or_init(|| Mutex::new(SymTable::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Intern `name` as seen from module `ns`.
///
/// Names are **case-insensitive**: they fold to a canonical lowercase form, so
/// `Foo`, `FOO` and `foo` all intern to one symbol.
///
/// The lookup walks `ns` and then every module above it before creating
/// anything, so a name the root already holds is *inherited* rather than
/// shadowed. That rule is what keeps `(module m (defun ...))` working: `defun`
/// is recognized by identity against the root's `defun`, and interning a
/// second one in `m` would silently stop every definition inside a module from
/// being a definition. It is also the rule the language already resolves names
/// by — current namespace first, then root.
///
/// A name found nowhere up the chain is created **in `ns`**, which is what
/// makes `m::foo` and `n::foo` different symbols.
pub fn intern_in(ns: NsId, name: &str) -> SymRef {
    let key = name.to_lowercase();
    let mut t = table();
    if let Some(s) = t.lookup(ns, &key) {
        return s;
    }
    t.create(ns, &key, NOT_WELL_KNOWN)
}

/// [`intern_in`] at the root module — for every caller with no lexical module
/// to work from: the prelude and the island (which *are* root), the checker
/// building core-IR tags, and the runtime's `string->symbol`/`gensym`, whose
/// symbols are made while the program runs rather than while it is read.
pub fn intern(name: &str) -> SymRef {
    intern_in(NsId::ROOT, name)
}

/// The module reached from `parent` by `segment`, creating it if new.
pub fn child_ns(parent: NsId, segment: &str) -> NsId {
    table().child(parent, &segment.to_lowercase())
}

/// The module `segments` names, relative to the root.
pub fn ns_of(segments: &[String]) -> NsId {
    let mut ns = NsId::ROOT;
    for s in segments {
        ns = child_ns(ns, s);
    }
    ns
}

/// `ns`'s segments from the root, for diagnostics.
pub fn ns_path(ns: NsId) -> Vec<String> {
    table().path(ns)
}

/// How many symbols exist, across every module. Diagnostics only — the count
/// is process-global and other threads may be interning.
pub fn symbol_count() -> usize {
    table().count()
}
