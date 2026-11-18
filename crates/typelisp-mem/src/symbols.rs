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
//! ## Keywords
//!
//! [`NsId::KEYWORD`] is CL's `KEYWORD` package: the module `keyword` at the
//! root, which every keyword belongs to whichever module wrote it. The reader
//! turns `:foo` into the symbol *named* `foo` in that module ([`intern_keyword`]);
//! the colon is how a keyword is spelled, not part of its name. Being a
//! keyword is being owned by that module ([`SymRef::is_keyword`]), and nothing
//! about a name says so: `(intern ":foo")` is an ordinary symbol whose name
//! starts with a colon. The keyword module imports nothing, and no other
//! module imports a keyword, so `foo` and `:foo` never meet in one table.
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

mod well_known;
mod vocabulary;
mod session;
pub use well_known::*;
pub use vocabulary::*;
pub use session::*;

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
    /// The spelling: the name, with a colon in front
    /// for a keyword. One allocation serves [`SymRef::name`] and
    /// [`SymRef::spelling`], which differ only by that colon.
    spelling: Box<str>,
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

/// How the symbol is written ([`SymRef::spelling`]) — what a message naming
/// it shows.
impl std::fmt::Display for SymRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.spelling())
    }
}

impl SymRef {
    fn header(&self) -> &'static Symbol {
        // SAFETY: the only way to obtain a `SymRef` is from the table, which
        // leaks every header it builds — so this address is always a live,
        // immutable `Symbol` (see the module doc comment's property 2).
        unsafe { &*self.0 }
    }

    /// The symbol's name — CL's `symbol-name`, so a
    /// keyword's has no colon: `:foo`'s name is `foo`.
    ///
    /// `'static` for real, not asserted: headers are leaked. Reading a name is
    /// one dereference — there is no reverse table, which is the whole point
    /// of the identity being an address.
    pub fn name(&self) -> &'static str {
        let spelling: &'static str = &self.header().spelling;
        if self.is_keyword() {
            &spelling[1..]
        } else {
            spelling
        }
    }

    /// How the symbol is written: its name, with a keyword's colon in front.
    /// What the compiler keys a symbol by when it needs a string (an
    /// environment, a diagnostic), so a keyword never stands where the plain
    /// symbol of the same name does. Not an identity: two symbols are the
    /// same when their addresses are.
    pub fn spelling(&self) -> &'static str {
        &self.header().spelling
    }

    /// The module that owns this symbol. Not "a module it is visible from":
    /// an imported symbol keeps the home it was created with.
    pub fn home(&self) -> NsId {
        self.header().home
    }

    /// Whether this is a keyword — owned by [`NsId::KEYWORD`]. The only test
    /// there is: a keyword's name is an ordinary name, colon-less.
    pub fn is_keyword(&self) -> bool {
        self.home() == NsId::KEYWORD
    }

    /// Which member of the fixed vocabulary this is, or [`NOT_WELL_KNOWN`].
    ///
    /// For dispatching over that vocabulary with a `match` — see [`wk`] for
    /// why this exists and why it is never an identity test.
    pub fn well_known(&self) -> u32 {
        self.header().well_known
    }

    /// Whether this is a member of the vocabulary every module holds alike —
    /// a reserved word, a keyword, or a name of `std`'s or the core IR's —
    /// so that where it was made says nothing about which definition it
    /// names. A member [`HOMED_VOCABULARY`] places in a module is that
    /// module's, like any symbol made there, even when the module is `std`
    /// (`dump`).
    pub fn is_shared_vocabulary(&self) -> bool {
        let k = self.well_known();
        k != NOT_WELL_KNOWN && !HOMED.get().expect("a symbol exists only once the table is built")[k as usize]
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

    /// Where every keyword lives — see the module doc comment. It is the
    /// root's child `keyword`, so its path is `["keyword"]` and re-interning a
    /// keyword by path lands back here.
    pub const KEYWORD: NsId = NsId(2);

    /// The standard library's module, the root's child `std`. What it
    /// exports is visible in every module without being imported: reading a
    /// name a module does not hold yields `std`'s symbol of that name when
    /// `std` exports one ([`intern_in`]).
    pub const STD: NsId = NsId(3);

    /// The core IR's module, the root's child `%core`: the IR's tags and the
    /// names the compiler island is written with. No other module imports
    /// these, so a user's `var` or `call` is the user's own symbol.
    pub const CORE: NsId = NsId(4);
}

/// A module of the standard library named from Rust, as its segments under
/// `std`: `StdModule(&[])` is `std` itself and `StdModule(&["time"])` is
/// `std::time`. What the runtime's constant type tables write a module as,
/// since only `std`'s own `NsId` is fixed.
#[derive(Clone, Copy, Debug)]
pub struct StdModule(pub &'static [&'static str]);

impl StdModule {
    pub const STD: StdModule = StdModule(&[]);
    pub const TIME: StdModule = StdModule(&["time"]);
    pub const REPL: StdModule = StdModule(&["repl"]);
    pub const THREAD: StdModule = StdModule(&["thread"]);
    pub const IO: StdModule = StdModule(&["io"]);
    pub const ARRAY: StdModule = StdModule(&["array"]);
    pub const OPTION: StdModule = StdModule(&["option"]);
    pub const RESULT: StdModule = StdModule(&["result"]);
    pub const VEC: StdModule = StdModule(&["vec"]);
    pub const SEXPR: StdModule = StdModule(&["sexpr"]);
    pub const COLLECTIONS: StdModule = StdModule(&["collections"]);
    pub const SYNC: StdModule = StdModule(&["sync"]);
    pub const READER: StdModule = StdModule(&["reader"]);
    pub const EVAL: StdModule = StdModule(&["eval"]);
    pub const NUM: StdModule = StdModule(&["num"]);
    pub const FS: StdModule = StdModule(&["fs"]);
    pub const NET: StdModule = StdModule(&["net"]);
    pub const PRIMITIVE: StdModule = StdModule(&["primitive"]);

    /// The module, found one segment at a time from `std`.
    pub fn ns(self) -> NsId {
        self.0.iter().fold(NsId::STD, |ns, segment| child_ns(ns, segment))
    }
}


/// One module's table.
///
/// `syms` holds both the symbols this module created and the ones it imported
/// from elsewhere; the two are not distinguished here because a lookup does
/// not care. `SymRef::home` is what tells them apart.
struct NsNode {
    syms: HashMap<Box<str>, SymRef>,
    /// Child modules, by the symbol that names each: the one its name reads
    /// as in this module, so `foo::bar` is `foo`'s child under `foo`'s `bar`.
    children: HashMap<SymRef, NsId>,
    /// The module this one is written inside, for [`SymTable::path`]. Never
    /// consulted when resolving a name.
    parent: Option<NsId>,
    /// The symbol this module is named by in its parent; `None` for the
    /// system module and the root, which have no parent.
    name: Option<SymRef>,
}

/// What a module's symbol stands for after a `use` brought something in
/// under it — see [`SessionNames::import_name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Import {
    Module(NsId),
    /// A type, by its own name's symbol.
    Type(SymRef),
    /// A function, global or macro, by its own name's symbol.
    Item(SymRef),
    /// A constructor or a static method of a type, which `(use Type)` brings
    /// in bare, by the type's name's symbol.
    Member(SymRef),
}

/// Where a path's segment led: into a module, or to a type, whose members
/// the next segment names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathStep {
    Module(NsId),
    Type(SymRef),
}

struct SymTable {
    nodes: Vec<NsNode>,
    /// What [`NsId::STD`] exports, by name: the symbols every module sees
    /// without importing them. Consulted by [`intern_in`] after the module's
    /// own table.
    std_exports: HashMap<Box<str>, SymRef>,
    /// How many headers have been created. Summing `syms.len()` would count
    /// an imported symbol once per module that can see it.
    created: usize,
}

impl SymTable {
    fn new() -> SymTable {
        let mut t = SymTable { nodes: Vec::new(), std_exports: HashMap::new(), created: 0 };
        // The fixed nodes, named once the root can read their names (below).
        let empty = |parent: Option<NsId>| NsNode { syms: HashMap::new(), children: HashMap::new(), parent, name: None };
        t.nodes.push(empty(None));
        t.nodes.push(empty(None));
        t.nodes.push(empty(Some(NsId::ROOT)));
        t.nodes.push(empty(Some(NsId::ROOT)));
        t.nodes.push(empty(Some(NsId::ROOT)));
        // The system module owns the reserved words. A keyword belongs to the
        // keyword module under its name without the colon, a standard
        // library name to `std` (which exports it), an IR name to `%core`.
        // A member of [`HOMED_VOCABULARY`] is made below, in its module, once
        // the modules can be named.
        let mut homed = vec![false; BUILTIN_SYMBOLS.len()];
        for (_, members) in HOMED_VOCABULARY {
            for &i in *members {
                homed[i as usize] = i >= FIRST_HOMED || STD_VOCABULARY.contains(&i);
            }
        }
        let mut vocabulary: Vec<Option<SymRef>> = BUILTIN_SYMBOLS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let i = i as u32;
                if homed[i as usize] {
                    None
                } else if let Some(kw) = name.strip_prefix(':') {
                    Some(t.create(NsId::KEYWORD, kw, i))
                } else if STD_VOCABULARY.contains(&i) {
                    let sym = t.create(NsId::STD, name, i);
                    t.std_exports.insert((*name).into(), sym);
                    Some(sym)
                } else if CORE_VOCABULARY.contains(&i) {
                    Some(t.create(NsId::CORE, name, i))
                } else {
                    Some(t.create(NsId::SYSTEM, name, i))
                }
            })
            .collect();
        // Every module but the keyword one holds the reserved words.
        let reserved = t.nodes[NsId::SYSTEM.0 as usize].syms.clone();
        for ns in [NsId::ROOT, NsId::STD, NsId::CORE] {
            for (name, sym) in &reserved {
                t.nodes[ns.0 as usize].syms.insert(name.clone(), *sym);
            }
        }
        for (segment, id) in [(KEYWORD_MODULE, NsId::KEYWORD), (STD_MODULE, NsId::STD), (CORE_MODULE, NsId::CORE)] {
            let name = t.read(NsId::ROOT, segment);
            t.nodes[id.0 as usize].name = Some(name);
            t.nodes[NsId::ROOT.0 as usize].children.insert(name, id);
        }
        for (path, members) in HOMED_VOCABULARY {
            let ns = path.iter().fold(NsId::ROOT, |ns, segment| t.child(ns, segment));
            for &i in *members {
                let name = BUILTIN_SYMBOLS[i as usize];
                match vocabulary[i as usize] {
                    Some(sym) => {
                        t.nodes[ns.0 as usize].syms.insert(name.into(), sym);
                    }
                    None => {
                        let sym = t.create(ns, name, i);
                        if path.first() == Some(&STD_MODULE) && STD_VOCABULARY.contains(&i) {
                            t.std_exports.insert(name.into(), sym);
                        }
                        vocabulary[i as usize] = Some(sym);
                    }
                }
            }
        }
        let vocabulary: Vec<SymRef> = vocabulary
            .into_iter()
            .enumerate()
            .map(|(i, sym)| sym.unwrap_or_else(|| panic!("`{}` is in no group of the vocabulary", BUILTIN_SYMBOLS[i])))
            .collect();
        WELL_KNOWN
            .set(vocabulary.into_boxed_slice())
            .unwrap_or_else(|_| unreachable!("the table is built exactly once"));
        HOMED.set(homed.into_boxed_slice()).unwrap_or_else(|_| unreachable!("the table is built exactly once"));
        t
    }

    /// Appends a module node, giving it the reserved words. The fixed nodes
    /// (system, root, keyword, std, core) are built by [`SymTable::new`].
    fn push_node(&mut self, parent: NsId, name: SymRef) -> NsId {
        let id = NsId(self.nodes.len() as u32);
        // By pointer, not by name: this is property 4 in the module doc
        // comment, and the reason `defun` means the same thing in every
        // module.
        let syms = self.nodes[NsId::SYSTEM.0 as usize].syms.clone();
        self.nodes.push(NsNode { syms, children: HashMap::new(), parent: Some(parent), name: Some(name) });
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
        let spelling: Box<str> = if ns == NsId::KEYWORD { format!(":{}", name).into() } else { name.into() };
        let header: &'static Symbol = Box::leak(Box::new(Symbol { spelling, home: ns, well_known }));
        let sym = SymRef(header as *const Symbol);
        self.nodes[ns.0 as usize].syms.insert(name.into(), sym);
        self.created += 1;
        sym
    }

    /// What `name` reads as in `ns` — [`intern_in`]'s rule, with the lock
    /// already held.
    fn read(&mut self, ns: NsId, name: &str) -> SymRef {
        match self.find(ns, name) {
            Some(s) => s,
            None => self.create(ns, name, NOT_WELL_KNOWN),
        }
    }

    /// The symbol `name` would read as in `ns`, if it exists yet.
    fn find(&self, ns: NsId, name: &str) -> Option<SymRef> {
        self.lookup(ns, name).or_else(|| if ns == NsId::KEYWORD { None } else { self.std_exports.get(name).copied() })
    }

    /// `parent`'s child module named `segment`, if it exists.
    fn find_child(&self, parent: NsId, segment: &str) -> Option<NsId> {
        let name = self.find(parent, segment)?;
        self.nodes[parent.0 as usize].children.get(&name).copied()
    }

    /// `parent`'s child module named `segment`, created if new. The child is
    /// keyed by the symbol `segment` reads as in `parent`.
    fn child(&mut self, parent: NsId, segment: &str) -> NsId {
        let name = self.read(parent, segment);
        if let Some(id) = self.nodes[parent.0 as usize].children.get(&name) {
            return *id;
        }
        let id = self.push_node(parent, name);
        self.nodes[parent.0 as usize].children.insert(name, id);
        id
    }

    /// Where the segment `name`, read in the module `ns`, leads in the
    /// session `names`: a child module of `ns` the session has, or a type.
    fn step_down(&mut self, names: &SessionNames, ns: NsId, name: &str) -> Option<(SymRef, PathStep)> {
        let sym = self.read(ns, name);
        self.step_down_symbol(names, ns, sym).map(|step| (sym, step))
    }

    /// [`Self::step_down`] for the symbol the segment read as.
    fn step_down_symbol(&self, names: &SessionNames, ns: NsId, sym: SymRef) -> Option<PathStep> {
        if let Some(id) = self.nodes[ns.0 as usize].children.get(&sym) {
            if names.modules.contains(id) {
                return Some(PathStep::Module(*id));
            }
        }
        // A type is a step from the module that defines it. Its name alone
        // does not say which: a vocabulary symbol (`Vector`, `llvm-module`)
        // is the same symbol in every module.
        (names.type_home(sym) == Some(ns)).then_some(PathStep::Type(sym))
    }

    /// Where the name `name` written in `ns` leads, as a path's first step:
    /// a child module of `ns` or a type named by it, or else what a `use`
    /// brought in under it — a module's own definition wins over an import
    /// of the same name, as it does for every name.
    fn step_in(&mut self, names: &SessionNames, ns: NsId, name: &str) -> Option<(SymRef, PathStep)> {
        let sym = self.read(ns, name);
        self.step_in_symbol(names, ns, sym).map(|step| (sym, step))
    }

    /// [`Self::step_in`] for the symbol the segment read as.
    fn step_in_symbol(&self, names: &SessionNames, ns: NsId, sym: SymRef) -> Option<PathStep> {
        if let Some(own) = self.step_down_symbol(names, ns, sym) {
            return Some(own);
        }
        match names.imports.get(&(ns, sym)) {
            Some(Import::Module(id)) => Some(PathStep::Module(*id)),
            Some(Import::Type(t)) => Some(PathStep::Type(*t)),
            _ => None,
        }
    }

    /// Where the first segment `name` of a path read in `ns` leads: from
    /// `ns`, else a type the standard library offers every module under the
    /// name ([`SessionNames::offered_type`]), else from the root, else — while
    /// the standard library's names are still resolved by name — from `std`.
    fn first_step(&mut self, names: &SessionNames, ns: NsId, name: &str) -> Option<(SymRef, PathStep)> {
        let mut found = self.step_in(names, ns, name);
        if found.is_none() {
            let sym = self.read(ns, name);
            found = names.offered_type(sym).map(|step| (sym, step));
        }
        for outer in [NsId::ROOT, NsId::STD] {
            if found.is_none() && ns != outer {
                found = self.step_in(names, outer, name);
            }
        }
        found
    }

    /// Whether `ns` is `std` or a module inside it.
    fn within_std(&self, ns: NsId) -> bool {
        std::iter::successors(Some(ns), |m| self.nodes[m.0 as usize].parent).any(|m| m == NsId::STD)
    }

    fn path(&self, ns: NsId) -> Vec<String> {
        let mut segs = Vec::new();
        let mut cur = Some(ns);
        while let Some(id) = cur {
            let node = &self.nodes[id.0 as usize];
            if let Some(name) = node.name {
                segs.push(name.name().to_string());
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

/// Whether [`HOMED_VOCABULARY`] places each member of the vocabulary, in
/// [`BUILTIN_SYMBOLS`] order, for [`SymRef::is_shared_vocabulary`]. Written
/// with [`WELL_KNOWN`].
static HOMED: OnceLock<Box<[bool]>> = OnceLock::new();

fn table() -> MutexGuard<'static, SymTable> {
    SYMBOLS
        .get_or_init(|| Mutex::new(SymTable::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Intern `name` in module `ns`.
///
/// Names are case-sensitive: `Foo`, `FOO` and `foo` are three symbols.
///
/// The name means, in order: the symbol `ns`'s own table holds (one it
/// created, a reserved word every module holds, or one it declared its own
/// with `shadow`); else the symbol [`NsId::STD`] exports under that name;
/// else a **new symbol owned by `ns`**. Nothing else is looked at — not the
/// modules `ns` is written inside, not any other module — so `m::foo` and
/// `n::foo` are different symbols unless `std` exports `foo`.
///
/// `ns` may be [`NsId::KEYWORD`]; that is [`intern_keyword`], and the keyword
/// module sees nothing `std` exports. Nothing about `name` routes it anywhere
/// else — `":foo"` interned in a module is a symbol of that module whose name
/// starts with a colon, as CL's `intern` makes.
pub fn intern_in(ns: NsId, name: &str) -> SymRef {
    table().read(ns, name)
}

/// The symbol named `name` whose home is `ns`, creating it there if `ns`
/// holds none — without looking at what `std` exports.
///
/// For rebuilding a symbol whose home was recorded (a dump, compiled code
/// re-interning a quoted symbol): the home is the answer, and consulting
/// `std`'s exports could hand back a different symbol of the same name.
pub fn intern_at_home(ns: NsId, name: &str) -> SymRef {
    let mut t = table();
    if let Some(s) = t.lookup(ns, name) {
        return s;
    }
    t.create(ns, name, NOT_WELL_KNOWN)
}

/// A new symbol of `home` that no table holds: nothing reads as it, so a
/// definition it names is reached only through whoever made it — a
/// definition the source cannot write ([`crate::specialization`]). `name`
/// is what it is shown as.
pub fn unlisted(home: NsId, name: &str) -> SymRef {
    let mut t = table();
    // Leaked like every header, for the reasons `SymTable::create` gives.
    let header: &'static Symbol = Box::leak(Box::new(Symbol { spelling: name.into(), home, well_known: NOT_WELL_KNOWN }));
    t.created += 1;
    SymRef(header as *const Symbol)
}

/// [`intern_at_home`] for a home given as segments from the root.
pub fn intern_at_home_path(segments: &[String], name: &str) -> SymRef {
    intern_at_home(ns_of(segments), name)
}

/// The keyword named `name` — what the reader makes of `:name`. `name` is
/// the keyword's name, without the colon that spells it.
pub fn intern_keyword(name: &str) -> SymRef {
    intern_in(NsId::KEYWORD, name)
}

/// The module a namespace name names — the second argument of `intern` —
/// or `None` when there is no such module: CL's `intern` signals an error
/// for a package that does not exist rather than making one. A name is a
/// module path from the root with `::` between its segments
/// (`"geometry::shapes"`), `"keyword"` is the keyword module, and `""` is the
/// root.
pub fn ns_named(name: &str) -> Option<NsId> {
    let name = name.strip_prefix("::").unwrap_or(name);
    if name.is_empty() {
        return Some(NsId::ROOT);
    }
    let segments: Vec<String> = name.split("::").map(str::to_string).collect();
    find_ns(&segments)
}

/// The keyword module's segment under the root — CL's `KEYWORD` package,
/// and what `(intern NAME "keyword")` names.
pub const KEYWORD_MODULE: &str = "keyword";

/// [`intern_in`] at the root module — for every caller with no lexical module
/// to work from: the prelude and the island (which *are* root), the checker
/// building core-IR tags.
pub fn intern(name: &str) -> SymRef {
    intern_in(NsId::ROOT, name)
}

/// The symbol a `catch` or `throw` of `tag` carrying the type whose key is
/// `type_key` is matched by at run time: comparing these symbols by pointer
/// compares the pairs, and a `catch` of the same symbol carrying another type
/// is told apart by [`throw_tag_mismatch`].
///
/// Made in [`THROW_TAG_MODULE`], under a module for the type key and then
/// `tag`'s own home path, and named as `tag` is: two symbols of one name in
/// two modules stay two tags, a message spells the tag as it was written,
/// and compiled code re-interns it at that home like any quoted symbol.
pub fn throw_tag_symbol(tag: SymRef, type_key: &str) -> SymRef {
    static ROOT: OnceLock<NsId> = OnceLock::new();
    let mut ns = child_ns(*ROOT.get_or_init(|| child_ns(NsId::ROOT, THROW_TAG_MODULE)), type_key);
    for seg in ns_path(tag.home()) {
        ns = child_ns(ns, &seg);
    }
    intern_at_home(ns, tag.name())
}

/// When `thrown` and `caught` are [`throw_tag_symbol`]s of one symbol
/// carrying two types, the error a `throw` that reached a `catch` of its
/// symbol carrying another type is, with the types written as `view` writes
/// them. `None` when they are one tag, or tags of two symbols — the `catch`
/// is the throw's, or not its business.
pub fn throw_tag_mismatch(thrown: SymRef, caught: SymRef, view: &crate::NameView) -> Option<String> {
    if thrown == caught || thrown.name() != caught.name() {
        return None;
    }
    let (t, c) = (ns_path(thrown.home()), ns_path(caught.home()));
    let parts = |p: &[String]| match p {
        [module, key, home @ ..] if module == THROW_TAG_MODULE => Some((key.clone(), home.to_vec())),
        _ => None,
    };
    let ((tk, th), (ck, ch)) = (parts(&t)?, parts(&c)?);
    (th == ch).then(|| format!("throw: `{}` carries `{}`, but the nearest (catch '{}) carries `{}`", thrown.name(), view.type_key_name(&tk), caught.name(), view.type_key_name(&ck)))
}


/// The symbol a member of a type — a method — is named by: made in
/// [`MEMBER_MODULE`], where a built-in method's name is the vocabulary
/// symbol its dispatch compares against.
pub fn member_symbol(name: &str) -> SymRef {
    static MEMBER: OnceLock<NsId> = OnceLock::new();
    let ns = *MEMBER.get_or_init(|| child_ns(NsId::ROOT, MEMBER_MODULE));
    intern_in(ns, name)
}

/// Makes `sym` visible in `into` under its own name, by pointer.
///
/// The imported name is *the same symbol*, so `==` compares equal across the
/// two modules — the only way one module is ever meant to see another's. Any
/// symbol `into` already had under that name is replaced.
pub fn import(into: NsId, sym: SymRef) {
    table().nodes[into.0 as usize].syms.insert(sym.name().into(), sym);
}

/// The module reached from `parent` by `segment`, creating it if new.
pub fn child_ns(parent: NsId, segment: &str) -> NsId {
    table().child(parent, segment)
}

/// What [`intern_in`] would return, without creating a symbol when there
/// is none: for looking a written name up in a table keyed by symbols, where
/// a name nothing defined must not leave a symbol behind.
pub fn find_exact(ns: NsId, name: &str) -> Option<SymRef> {
    table().find(ns, name)
}

/// The child module of `parent` whose name in `parent` is the symbol `name`
/// — the tree's own key, as [`ns_names`] gives it — if there is one.
pub fn child_ns_named(parent: NsId, name: SymRef) -> Option<NsId> {
    table().nodes[parent.0 as usize].children.get(&name).copied()
}

/// The definition a path from the root names — the module it is in and the
/// symbol it is defined under — reading `segments` as the tree's own keys:
/// the root's empty name, each module's name in its parent, then the
/// definition's name. `None` when the path does not start at the root.
///
/// # Panics
///
/// When a module segment is not a child of the module before it: the path
/// was not written from the tree.
pub fn definition_of_path(segments: &[SymRef]) -> Option<(NsId, SymRef)> {
    let (root, segs) = segments.split_first()?;
    if *root != intern_in(NsId::ROOT, "") {
        return None;
    }
    let (name, modules) = segs.split_last()?;
    let ns = modules.iter().fold(NsId::ROOT, |ns, m| {
        child_ns_named(ns, *m)
            .unwrap_or_else(|| panic!("`{}` is not a module of `{}`: the path was not written from the tree", m.name(), ns_path(ns).join("::")))
    });
    Some((ns, *name))
}

/// The module `ns` is a child of; `None` for the root and the system module.
pub fn ns_parent(ns: NsId) -> Option<NsId> {
    table().nodes[ns.0 as usize].parent
}

/// The module `segments` names from the root, if every segment exists — the
/// lookup [`ns_of`] makes, without creating modules.
pub fn find_ns(segments: &[String]) -> Option<NsId> {
    let t = table();
    let mut ns = NsId::ROOT;
    for s in segments {
        ns = t.find_child(ns, s)?;
    }
    Some(ns)
}

/// `ns`'s child modules, by the name of each, in no particular order.
pub fn ns_children(ns: NsId) -> Vec<(&'static str, NsId)> {
    table().nodes[ns.0 as usize].children.iter().map(|(name, id)| (name.name(), *id)).collect()
}


/// The module `segments` names, relative to the root.
pub fn ns_of(segments: &[String]) -> NsId {
    let mut ns = NsId::ROOT;
    for s in segments {
        ns = child_ns(ns, s);
    }
    ns
}

/// The symbol `ns` is named by in the module it is a child of; `None` for
/// the root and the system module.
pub fn ns_name(ns: NsId) -> Option<SymRef> {
    table().nodes[ns.0 as usize].name
}

/// The names of `ns` and of the modules above it, from the root down: the
/// symbol each is a child of the one before under.
pub fn ns_names(ns: NsId) -> Vec<SymRef> {
    let t = table();
    let mut names = Vec::new();
    let mut cur = Some(ns);
    while let Some(id) = cur {
        let node = &t.nodes[id.0 as usize];
        if let Some(name) = node.name {
            names.push(name);
        }
        cur = node.parent;
    }
    names.reverse();
    names
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

