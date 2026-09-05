//! The symbol table: process-global, permanent, and **one table per module**.
//!
//! A symbol's identity is the **address of its [`Symbol`] header** — the same
//! thing CL means by comparing symbols with `eq`, and the same thing
//! [`super::value::ConsRef`] already is for a cons cell. Asking "is this
//! `&rest`?" is a pointer comparison; nothing reads the name back out to
//! answer it.
//!
//! Four properties hold, and each one is load-bearing:
//!
//! 1. **Global.** The table is not part of any `Heap`. It is one process-wide
//!    structure, so a symbol is meaningful across heaps — which matters
//!    because compiled code reaches symbols through a raw tagged word, with no
//!    heap handle in sight.
//! 2. **Permanent.** Nothing here is ever freed. Headers are `Box::leak`ed, so
//!    the `'static` lifetime [`SymRef::name`] hands out is real rather than
//!    asserted, and the address can never dangle. That is what makes the
//!    `unsafe` below simple enough not to need Miri watching it: there is no
//!    deallocation to get wrong.
//! 3. **One table per module.** A module owns the symbols written inside it.
//!    Two modules that both write `foo` get **two different symbols** — the
//!    module a name is written in is part of what the name means.
//! 4. **Sharing is by pointer, never by name.** A module that must see another
//!    module's symbol holds *that symbol* in its own table ([`import`]): one
//!    header, two names pointing at it, so `==` on the addresses answers
//!    "the same symbol" with nothing to fall back on. There is no search up
//!    the module tree — a lookup reads exactly one table.
//!
//! ## The system module
//!
//! [`NsId::SYSTEM`] holds the fixed vocabulary ([`BUILTIN_SYMBOLS`]) and
//! nothing else. It is created once; every module created afterwards
//! *imports* those symbols by pointer, which is why `defun` written inside
//! `(module m ...)` is the very same symbol as `defun` written at the top
//! level — and why the checker can keep recognizing a definition by identity.
//! Without that import a module's `defun` would be `m`'s own new symbol, and
//! every definition inside a module would silently stop being a definition.
//!
//! [`NsId::ROOT`] is an ordinary module that happens to be first: the prelude,
//! the core macro layer and the island are read into it.
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
    // **This table is positional.** `sym_index` is the offset into it, and
    // compiled island code carries those offsets, so an entry may never be
    // inserted or removed in the middle — and a rename in place silently
    // changes what an already-compiled index resolves to. Replacements go at
    // the *end* of the table (see the last section).
    //
    // `int`/`float` were the core-IR tags before the width work; they are
    // retired rather than deleted, because deleting them would renumber
    // everything after and invalidate every committed artifact. Nothing reads
    // them now — the live spellings are `INT_ANY_WIDTH`/`FLOAT_ANY_WIDTH`.
    RETIRED_INT => "int"
    RETIRED_FLOAT => "float"
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

    "Names appended after the width work (2026-09-02). Appended, never inserted — see the positional note on `RETIRED_INT`."
    // The core-IR operator tags. `-any-width`, not `int`/`float`: one node
    // serves every width of its family, and the name has to say so. The
    // literal's payload is already the number the type names — a narrow
    // integer normalized, a binary32 float rounded and widened — so the
    // constant it compiles to is correct whichever width the type was, and no
    // width is discarded by leaving it out of the node. The width that *does*
    // matter downstream is the one the value is boxed at, and that reaches
    // the island by a different route: `Repr::field_kind`.
    INT_ANY_WIDTH => "int-any-width"
    FLOAT_ANY_WIDTH => "float-any-width"
    // `Sexpr`'s numeric constructors, spelled by width because a `Sexpr` node
    // keeps the width of the value put into it. These used to share `INT`/
    // `FLOAT` with the core-IR tags — one spelling standing for two different
    // things, which is exactly what folding the widths away allowed.
    I32 => "i32"
    F64 => "f64"
    F32 => "f32"
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

    "Appended after the narrow-integer `Sexpr` variants (2026-09-02), at the very end of the table for the reason `RETIRED_INT` records."
    // `Sexpr`'s constructors for the five integer widths that are not `i32`
    // — peers of `I32`/`F64`/`F32` above, listed here rather than beside
    // them because this table is positional and appending is the only edit
    // that moves nothing.
    I8 => "i8"
    I16 => "i16"
    U8 => "u8"
    U16 => "u16"
    U32 => "u32"

    "Appended for `return-from` (2026-09-04), at the very end for the reason `RETIRED_INT` records."
    // `block`'s own entry is already above, among the forms the pretty printer
    // lays out code-shaped; it is now a core-IR tag as well, which changes
    // nothing about the table. Both are a source special form and a core-IR
    // tag, the way `loop`/`break`/`return` are. `block` is the *lexical* named
    // escape — `catch`/`throw` remain the dynamic one — so the name it carries
    // is resolved where it is written and never travels at run time the way a
    // `throw`'s tag does.
    RETURN_FROM => "return-from"

    "Appended for Stage 9a (2026-09-04), again at the very end. `in-module` is
     the flat form of `(module ...)`; `import` is the CL-compatible spelling of
     `use`, and `shadowing-import` the one that means to take a bare name
     another binding already holds. CL's `shadow` and `unuse-package` have no
     entries because they are not adopted — see cl-parity-plan.md Stage 9a."
    IN_MODULE => "in-module"
    IMPORT => "import"
    SHADOWING_IMPORT => "shadowing-import"

    "Appended for Stage 9b (2026-09-04). A *source* form only — it lowers to the
     same `(defvar ...)` node with a trailing `true`, so the core vocabulary is
     unchanged (see `Checker::defvar_form`)."
    DEFPARAMETER => "defparameter"

    "Appended for Stage 4a (2026-09-05): the extended `loop`'s `:named`, held
     back until `block`/`return-from` existed to give it a meaning. A clause
     word like its neighbours above, listed here rather than beside them
     because this table is positional — see the module comment."
    KW_NAMED => ":named"
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
    /// The module that owns this symbol — the one it was created in. A module
    /// that merely [`import`]s it is not its home.
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

    /// The module that owns this symbol. Not "a module it is visible from":
    /// an imported symbol keeps the home it was created with.
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

/// A module's table, as an index into the one `Vec` behind the lock.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NsId(u32);

impl NsId {
    /// The system module: the fixed vocabulary ([`BUILTIN_SYMBOLS`]) and
    /// nothing else. Every other module imports these by pointer when it is
    /// created — see the module doc comment.
    pub const SYSTEM: NsId = NsId(0);

    /// The top-level module: the prelude, the core macro layer and the island
    /// are read into it, as is a source file that declares no module of its
    /// own. Ordinary in every respect but being created first.
    pub const ROOT: NsId = NsId(1);
}

/// One module's table.
///
/// `syms` holds both the symbols this module created and the ones it imported
/// from elsewhere; the two are not distinguished here because a lookup does
/// not care. `SymRef::home` is what tells them apart.
struct NsNode {
    syms: HashMap<Box<str>, SymRef>,
    /// Child modules, by segment — the same shape as the checker's
    /// `Namespace::modules`. Structure only: a lookup never follows it.
    children: HashMap<Box<str>, NsId>,
    /// The module this one is written inside, for [`SymTable::path`]. Never
    /// consulted when resolving a name.
    parent: Option<NsId>,
    /// The segment this module was reached by, for diagnostics.
    segment: Box<str>,
}

struct SymTable {
    nodes: Vec<NsNode>,
    /// How many headers have been created. Summing `syms.len()` would count
    /// an imported symbol once per module that can see it.
    created: usize,
}

impl SymTable {
    fn new() -> SymTable {
        let mut t = SymTable { nodes: Vec::new(), created: 0 };
        // The system module owns the vocabulary...
        t.push_node(None, "");
        let vocabulary: Vec<SymRef> = BUILTIN_SYMBOLS
            .iter()
            .enumerate()
            .map(|(i, name)| t.create(NsId::SYSTEM, name, i as u32))
            .collect();
        WELL_KNOWN
            .set(vocabulary.into_boxed_slice())
            .unwrap_or_else(|_| unreachable!("the table is built exactly once"));
        // ...and the root module is the first to import it.
        t.push_node(None, "");
        debug_assert_eq!(t.nodes.len(), 2);
        t
    }

    /// Appends a node, giving it the system vocabulary. `parent` is `None` for
    /// the two modules that are not written inside another one.
    fn push_node(&mut self, parent: Option<NsId>, segment: &str) -> NsId {
        let id = NsId(self.nodes.len() as u32);
        let syms = if id == NsId::SYSTEM {
            HashMap::new()
        } else {
            // By pointer, not by name: this is property 4 in the module doc
            // comment, and the reason `defun` means the same thing in every
            // module.
            self.nodes[NsId::SYSTEM.0 as usize].syms.clone()
        };
        self.nodes.push(NsNode {
            syms,
            children: HashMap::new(),
            parent,
            segment: segment.into(),
        });
        id
    }

    /// The symbol `name` names in `ns` — in that module's own table, and
    /// nowhere else. There is no walk up the tree.
    fn lookup(&self, ns: NsId, name: &str) -> Option<SymRef> {
        self.nodes[ns.0 as usize].syms.get(name).copied()
    }

    /// Build a symbol owned by `ns`. The caller has already looked.
    fn create(&mut self, ns: NsId, name: &str, well_known: u32) -> SymRef {
        // Leaked on purpose: symbols are permanent, so this is the allocation
        // that gives `SymRef::name` a genuine `'static` and makes the address
        // impossible to dangle.
        let header: &'static Symbol =
            Box::leak(Box::new(Symbol { name: name.into(), home: ns, well_known }));
        let sym = SymRef(header as *const Symbol);
        self.nodes[ns.0 as usize].syms.insert(name.into(), sym);
        self.created += 1;
        sym
    }

    fn child(&mut self, parent: NsId, segment: &str) -> NsId {
        if let Some(id) = self.nodes[parent.0 as usize].children.get(segment) {
            return *id;
        }
        let id = self.push_node(Some(parent), segment);
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
}

static SYMBOLS: OnceLock<Mutex<SymTable>> = OnceLock::new();

/// The system module's symbols in [`BUILTIN_SYMBOLS`] order, for
/// [`well_known_symbol`]. Written once, while the table is being built, and
/// read without the lock afterwards.
static WELL_KNOWN: OnceLock<Box<[SymRef]>> = OnceLock::new();

fn table() -> MutexGuard<'static, SymTable> {
    SYMBOLS
        .get_or_init(|| Mutex::new(SymTable::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Intern `name` in module `ns`.
///
/// Names are **case-insensitive**: they fold to a canonical lowercase form, so
/// `Foo`, `FOO` and `foo` all intern to one symbol.
///
/// Only `ns`'s own table is consulted. A name it does not hold — neither
/// created there nor imported into it — becomes a **new symbol owned by
/// `ns`**, which is what makes `m::foo` and `n::foo` different symbols. The
/// fixed vocabulary is the same symbol everywhere only because every module
/// imports it from [`NsId::SYSTEM`] when it is created.
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

/// Makes `sym` visible in `into` under its own name, by pointer.
///
/// The imported name is *the same symbol*, so `==` compares equal across the
/// two modules — the only way one module is ever meant to see another's. Any
/// symbol `into` already had under that name is replaced.
pub fn import(into: NsId, sym: SymRef) {
    table().nodes[into.0 as usize].syms.insert(sym.name().into(), sym);
}

/// The symbol `name` names in `ns`, without creating one.
pub fn lookup_in(ns: NsId, name: &str) -> Option<SymRef> {
    table().lookup(ns, &name.to_lowercase())
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

/// How many symbols exist — headers created, not names visible. Diagnostics
/// only: the count is process-global and other threads may be interning.
pub fn symbol_count() -> usize {
    table().created
}

/// The system module's symbol at index `i` of [`BUILTIN_SYMBOLS`] — the
/// inverse of [`SymRef::well_known`].
///
/// This is how a member of the fixed vocabulary crosses into a compiled
/// program: its *address* cannot (an ahead-of-time compiled program is another
/// process), but its index can, because every process builds the vocabulary
/// from the same list in the same order. No name is spelled out and no lookup
/// happens — the alternative, rebuilding the name and interning it, is the
/// thing this exists to avoid.
///
/// Panics on an index outside the vocabulary: the caller is a compiler that
/// read the index off a symbol, so an out-of-range one is a bug here, not bad
/// input.
pub fn well_known_symbol(i: u32) -> SymRef {
    if WELL_KNOWN.get().is_none() {
        // Building the main table is what fills this in. Deliberately not
        // `get_or_init`: the initializer would have to build that table, whose
        // own construction `set`s this very cell — re-entering a `OnceLock`
        // that is mid-initialization deadlocks.
        drop(table());
    }
    WELL_KNOWN.get().expect("building the table fills WELL_KNOWN")[i as usize]
}

/// Intern `name` in the module `segments` names, relative to the root —
/// [`ns_of`] and [`intern_in`] in one step, for callers holding a path rather
/// than an [`NsId`] (compiled code rebuilding a quoted symbol).
pub fn intern_in_path(segments: &[String], name: &str) -> SymRef {
    intern_in(ns_of(segments), name)
}
