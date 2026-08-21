//! Dumps: what a compilation produced, written down so it never has to be
//! derived again.
//!
//! A *unit* is one compilation's output — the prelude, the compiler island, a
//! source file, a REPL session — and it always has two halves: the native
//! bodies (LLVM bitcode, held by the backend) and the checked state that says
//! what those bodies are (this module). A *dump* is an ordered sequence of
//! units, applied front to back.
//!
//! **Why not a memory image.** SBCL's `save-lisp-and-die` writes the heap out
//! verbatim, which works because in SBCL compiled code, the environment and
//! every value are all objects in that one heap. Here only the third is: the
//! native bodies live in LLVM's JIT memory as bare addresses, and the checker's
//! tables are Rust `HashMap`s outside the cons arena. So the code has to be
//! written as bitcode (not addresses) and the environment as serialized state
//! — the same position ECL is in, and for the same reason.
//!
//! **A unit carries what it added, not everything in scope.** The island unit
//! holds the island's 121 definitions, not the prelude's as well. Cumulative
//! units would mean replacing the checker wholesale on each load, which would
//! re-root every generic template the previous unit permanently rooted (61 in
//! the prelude alone) with no way to drop the old ones.
//!
//! **What a dump does not save.** Values: `defvar` initializers are re-run when
//! the unit loads, so a global comes back at its initial value rather than
//! whatever the session last stored. That is the one place this deliberately
//! differs from SBCL, and it is what makes stream handles, closures and foreign
//! pointers non-problems — none of them are ever written down. Non-definition
//! top-level forms are not saved either (a dump that re-prints your session's
//! output on load is nobody's idea of a dump), nor is `Registry::def_locs`,
//! which only `typl-lsp` reads.

use std::collections::hash_map::DefaultHasher;
use std::convert::TryInto;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::check::registry::{AdtDef, BlanketImpl, Docs, FnSig, MacroDef, Namespace, TraitDef, VarInfo};
use crate::owned_form::OwnedForm;
use crate::types::Path;
use crate::Type;

/// Bumped whenever the wire shape changes. Producer and consumer ship
/// together, so this catches a stale artifact rather than a version skew
/// anyone has to migrate — the same stance SBCL takes with its core files
/// ("there is absolutely no binary compatibility of core images between
/// different runtime support programs").
pub const FORMAT_VERSION: u32 = 2;

/// Which table an entry came out of. Part of its identity: `foo` the function
/// and `foo` the macro are different entries in the same namespace.
pub mod cat {
    pub const MODULE: u8 = 0;
    pub const FN: u8 = 1;
    pub const TYPE: u8 = 2;
    pub const TRAIT: u8 = 3;
    pub const MACRO: u8 = 4;
    pub const CTOR: u8 = 5;
    pub const VAR: u8 = 6;
    pub const ALIAS: u8 = 7;
    pub const MOD_ALIAS: u8 = 8;
    pub const STATIC_USE: u8 = 9;
    pub const BLANKET: u8 = 10;
    pub const DOC_FN: u8 = 11;
    pub const DOC_METHOD: u8 = 12;
    pub const DOC_TYPE: u8 = 13;
    pub const DOC_VAR: u8 = 14;
    pub const DOC_TRAIT: u8 = 15;
    pub const DOC_MACRO: u8 = 16;
    pub const DOC_TRAIT_METHOD: u8 = 17;
    pub const THROW_TAG: u8 = 18;
    pub const PREDECLARED: u8 = 19;
    pub const FN_TEMPLATE: u8 = 20;
    pub const METHOD_TEMPLATE: u8 = 21;
}

/// One registry entry, with its value already serialized.
///
/// The payload is bincode rather than a typed enum with one variant per table
/// because the walk that produces these has to serialize each value anyway (to
/// hash it for [`RegistrySignature`]), and a `Vec<u8>` it already has costs
/// nothing to keep. `apply` reads `cat` to know what to deserialize it back
/// into.
#[derive(Serialize, Deserialize)]
pub struct Entry {
    cat: u8,
    /// The module path the entry lives in — `[]` for the root, and for the
    /// tables (docs, throw tags) that are keyed absolutely rather than by
    /// namespace.
    ns: Vec<String>,
    /// The entry's name within its table, or the whole key for an absolutely
    /// keyed table.
    name: String,
    payload: Vec<u8>,
}

/// Every registry entry's identity and content-hash, as of some point in time.
///
/// Taken before a unit is loaded and handed to [`crate::check::Checker::capture_delta`]
/// afterwards; the delta is every entry whose hash is new or different.
/// Content-hashing rather than key-comparison because entries get *modified*,
/// not just added — an `impl` block grows an existing `AdtDef`'s `assoc` table,
/// and a redefinition replaces a `FnSig` in place.
#[derive(Default)]
pub struct RegistrySignature(BTreeMap<(u8, String), u64>);

impl RegistrySignature {
    pub(crate) fn record(&mut self, cat: u8, key: String, hash: u64) {
        self.0.insert((cat, key), hash);
    }

    pub(crate) fn unchanged(&self, cat: u8, key: &str, hash: u64) -> bool {
        self.0.get(&(cat, key.to_string())) == Some(&hash)
    }

    /// How many entries the registry held — reported by the artifact
    /// generators so a delta that suddenly swallows the world is visible.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every entry that differs between two signatures, as readable lines.
    ///
    /// The failure message of the round-trip guard test: "the two registries
    /// are not equal" is useless, "`prelude::length` is missing on the restored
    /// side" is the bug.
    pub fn difference(&self, other: &RegistrySignature) -> Vec<String> {
        let name = |cat: u8| match cat {
            cat::MODULE => "module",
            cat::FN => "fn",
            cat::TYPE => "type",
            cat::TRAIT => "trait",
            cat::MACRO => "macro",
            cat::CTOR => "ctor",
            cat::VAR => "var",
            cat::ALIAS => "alias",
            cat::MOD_ALIAS => "mod-alias",
            cat::STATIC_USE => "static-use",
            cat::BLANKET => "blanket-impl",
            cat::THROW_TAG => "throw-tag",
            cat::PREDECLARED => "predeclared",
            cat::FN_TEMPLATE => "fn-template",
            cat::METHOD_TEMPLATE => "method-template",
            _ => "doc",
        };
        let mut out = Vec::new();
        for (key, hash) in &self.0 {
            match other.0.get(key) {
                None => out.push(format!("only on the left: {} {}", name(key.0), key.1)),
                Some(h) if h != hash => out.push(format!("differs: {} {}", name(key.0), key.1)),
                Some(_) => {}
            }
        }
        for key in other.0.keys() {
            if !self.0.contains_key(key) {
                out.push(format!("only on the right: {} {}", name(key.0), key.1));
            }
        }
        out
    }
}

/// Identifies the file and, with [`FORMAT_VERSION`], what is in it.
pub const MAGIC: &[u8; 6] = b"TYPLD\0";

/// The container's own version, distinct from [`FORMAT_VERSION`] (the unit
/// payload's): a change to the directory layout and a change to what a unit
/// records are different events, and either one alone should be able to reject
/// an old file.
pub const CONTAINER_VERSION: u32 = 1;

const HEADER: usize = 6 + 4 + 4;
const DIRECTORY_ENTRY: usize = 8 * 4;

/// One unit as it sits in a dump: the bytes of its checked state, and the
/// bitcode holding the bodies that state describes.
///
/// Borrowed, not owned: the prelude's and island's dumps are `include_bytes!`
/// statics, and the bitcode is handed straight to LLVM, which copies it into
/// its own `MemoryBuffer` anyway.
pub struct UnitRef<'a> {
    pub types: &'a [u8],
    pub bitcode: &'a [u8],
}

/// Lays out `units` as a dump.
pub fn write(units: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    out.extend_from_slice(&(units.len() as u32).to_le_bytes());

    let mut offset = (HEADER + DIRECTORY_ENTRY * units.len()) as u64;
    for (types, bitcode) in units {
        for section in [types, bitcode] {
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&(section.len() as u64).to_le_bytes());
            offset += section.len() as u64;
        }
    }
    for (types, bitcode) in units {
        out.extend_from_slice(types);
        out.extend_from_slice(bitcode);
    }
    out
}

/// Reads a dump's directory, without touching the payloads.
pub fn parse<'a>(bytes: &'a [u8], label: &str) -> Result<Vec<UnitRef<'a>>, String> {
    if bytes.len() < HEADER || &bytes[..6] != MAGIC {
        return Err(format!("{}: not a typelisp dump", label));
    }
    let version = u32::from_le_bytes(bytes[6..10].try_into().expect("4 bytes"));
    if version != CONTAINER_VERSION {
        return Err(format!(
            "{}: dump container version {}, expected {} — the file and this build are from \
             different sources",
            label, version, CONTAINER_VERSION
        ));
    }
    let count = u32::from_le_bytes(bytes[10..14].try_into().expect("4 bytes")) as usize;
    let directory_end = HEADER + DIRECTORY_ENTRY * count;
    if bytes.len() < directory_end {
        return Err(format!("{}: dump claims {} units but ends inside its directory", label, count));
    }

    let word = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 bytes")) as usize;
    let mut units = Vec::with_capacity(count);
    for i in 0..count {
        let base = HEADER + DIRECTORY_ENTRY * i;
        let section = |half: usize| -> Result<&[u8], String> {
            let (off, len) = (word(base + half * 16), word(base + half * 16 + 8));
            bytes
                .get(off..off + len)
                .ok_or_else(|| format!("{}: dump unit {} points past the end of the file", label, i))
        };
        units.push(UnitRef { types: section(0)?, bitcode: section(1)? });
    }
    Ok(units)
}

/// A generic `defun`'s retained source form, heap-independent.
///
/// The live `FnTemplate` holds `Vec<Value>` — raw read forms, permanently
/// rooted. 61 of the prelude's 86 `defun`s are generic, so this is the bulk of
/// its unit: an eval'd or newly-instantiated call at a fresh type argument
/// re-checks the template.
#[derive(Serialize, Deserialize)]
pub struct WireFnTemplate {
    pub parts: Vec<OwnedForm>,
    pub ns: Vec<String>,
    pub type_params: Vec<String>,
}

/// The `MethodTemplate` counterpart. `Getter`/`Setter` carry no form at all —
/// `check_defstruct` synthesizes those bodies, so specialization re-synthesizes
/// them from the owner's `AdtDef`.
#[derive(Serialize, Deserialize)]
pub enum WireMethodTemplate {
    Form { parts: Vec<OwnedForm>, ns: Vec<String>, written_vars: Vec<String> },
    Getter { index: usize },
    Setter { index: usize },
}

/// What one unit added to the checker.
///
/// Entries are in walk order (a module before anything inside it), which is
/// the order [`CheckerDelta::apply`] replays them in.
#[derive(Default, Serialize, Deserialize)]
pub struct CheckerDelta {
    pub entries: Vec<Entry>,
    /// Kept typed rather than in `entries` because applying one has to rebuild
    /// its forms in the live heap and permanently root them.
    pub fn_templates: Vec<(Path, WireFnTemplate)>,
    pub method_templates: Vec<((Path, String), WireMethodTemplate)>,
}

/// Serializes `value` and returns the bytes together with their hash.
///
/// The hash is over the bytes rather than a `Hash` impl so that no registry
/// type has to implement `Hash`, and so that "what changed" is exactly "what
/// would be written down differently".
pub(crate) fn wire<T: Serialize>(value: &T) -> Result<(Vec<u8>, u64), String> {
    let bytes = bincode::serialize(value).map_err(|e| format!("dump: serializing failed: {}", e))?;
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    Ok((bytes, h.finish()))
}

fn key_of(ns: &[String], name: &str) -> String {
    if ns.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", ns.join("::"), name)
    }
}

/// Feeds every registry entry to `visit` as `(category, namespace, name,
/// serialized value, hash)`.
///
/// One walk, two readers: [`signature`] keeps the hashes and throws the bytes
/// away, [`delta`] does the reverse. Writing it once is the point — a table
/// that only one of them knew about would produce a delta that silently drops
/// definitions.
fn walk(
    root: &Namespace,
    docs: &Docs,
    throw_tags: &BTreeMap<String, Type>,
    predeclared: &[String],
    visit: &mut dyn FnMut(u8, &[String], &str, Vec<u8>, u64) -> Result<(), String>,
) -> Result<(), String> {
    walk_ns(root, &mut Vec::new(), visit)?;

    macro_rules! doc_table {
        ($cat:expr, $map:expr, $key:expr) => {
            let mut rows: Vec<(String, &String)> = $map.iter().map(|(k, v)| ($key(k), v)).collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, v) in rows {
                let (bytes, hash) = wire(v)?;
                visit($cat, &[], &key, bytes, hash)?;
            }
        };
    }
    doc_table!(cat::DOC_FN, docs.fns, |p: &Path| p.to_string());
    doc_table!(cat::DOC_TYPE, docs.types, |p: &Path| p.to_string());
    doc_table!(cat::DOC_VAR, docs.vars, |p: &Path| p.to_string());
    doc_table!(cat::DOC_TRAIT, docs.traits, |p: &Path| p.to_string());
    doc_table!(cat::DOC_MACRO, docs.macros, |p: &Path| p.to_string());
    doc_table!(cat::DOC_METHOD, docs.methods, |k: &(Path, String)| format!("{}|{}", k.0, k.1));
    doc_table!(cat::DOC_TRAIT_METHOD, docs.trait_methods, |k: &(Path, String)| format!("{}|{}", k.0, k.1));

    for (tag, ty) in throw_tags {
        let (bytes, hash) = wire(ty)?;
        visit(cat::THROW_TAG, &[], tag, bytes, hash)?;
    }
    for name in predeclared {
        let (bytes, hash) = wire(&())?;
        visit(cat::PREDECLARED, &[], name, bytes, hash)?;
    }
    Ok(())
}

fn walk_ns(
    ns: &Namespace,
    path: &mut Vec<String>,
    visit: &mut dyn FnMut(u8, &[String], &str, Vec<u8>, u64) -> Result<(), String>,
) -> Result<(), String> {
    // Sorted, not in iteration order: the namespace's own tables are
    // `HashMap`s, so walking them as they come would put a unit's entries in a
    // different order every run and make the dump's bytes differ from build to
    // build for no change in content.
    macro_rules! table {
        ($cat:expr, $map:expr) => {
            let mut names: Vec<&String> = $map.keys().collect();
            names.sort();
            for name in names {
                let (bytes, hash) = wire(&$map[name])?;
                visit($cat, path, name, bytes, hash)?;
            }
        };
    }
    table!(cat::FN, ns.fns);
    table!(cat::TYPE, ns.types);
    table!(cat::TRAIT, ns.traits);
    table!(cat::MACRO, ns.macros);
    table!(cat::CTOR, ns.ctors);
    table!(cat::VAR, ns.vars);
    table!(cat::ALIAS, ns.aliases);
    table!(cat::MOD_ALIAS, ns.mod_aliases);
    table!(cat::STATIC_USE, ns.static_uses);
    // Positional, not keyed: at most one blanket `impl` may cover any trait,
    // so the index is a stable identity for as long as the list only grows.
    for (i, imp) in ns.blanket_impls.iter().enumerate() {
        let (bytes, hash) = wire(imp)?;
        visit(cat::BLANKET, path, &i.to_string(), bytes, hash)?;
    }

    let mut module_names: Vec<&String> = ns.modules.keys().collect();
    module_names.sort();
    for name in module_names {
        let child = &ns.modules[name];
        // Emitted before its contents so `apply` creates the module first —
        // and so a module that is *only* a container still appears.
        let (bytes, hash) = wire(&())?;
        visit(cat::MODULE, path, name, bytes, hash)?;
        path.push(name.clone());
        walk_ns(child, path, visit)?;
        path.pop();
    }
    Ok(())
}

/// Every entry's content hash, as of now.
pub fn signature(
    root: &Namespace,
    docs: &Docs,
    throw_tags: &BTreeMap<String, Type>,
    predeclared: &[String],
) -> Result<RegistrySignature, String> {
    let mut sig = RegistrySignature::default();
    walk(root, docs, throw_tags, predeclared, &mut |cat, ns, name, _bytes, hash| {
        sig.record(cat, key_of(ns, name), hash);
        Ok(())
    })?;
    Ok(sig)
}

/// Every entry that is new or different since `before`.
pub fn delta(
    root: &Namespace,
    docs: &Docs,
    throw_tags: &BTreeMap<String, Type>,
    predeclared: &[String],
    before: &RegistrySignature,
) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    walk(root, docs, throw_tags, predeclared, &mut |cat, ns, name, bytes, hash| {
        if !before.unchanged(cat, &key_of(ns, name), hash) {
            entries.push(Entry { cat, ns: ns.to_vec(), name: name.to_string(), payload: bytes });
        }
        Ok(())
    })?;
    Ok(entries)
}

/// Reinstates `entries` in a live registry.
///
/// Inserts straight into each table rather than going through
/// `Namespace::add_type` and friends: the walk recorded what the tables *held*,
/// including the constructor index those helpers maintain, so replaying their
/// side effects on top would be doing the same work twice from worse
/// information.
pub fn apply_entries(
    root: &mut Namespace,
    docs: &mut Docs,
    throw_tags: &mut BTreeMap<String, Type>,
    predeclared: &mut dyn FnMut(String),
    entries: Vec<Entry>,
) -> Result<(), String> {
    for e in entries {
        match e.cat {
            cat::MODULE => {
                let mut p = e.ns.clone();
                p.push(e.name);
                root.module_mut(&p);
            }
            cat::FN => {
                let v: FnSig = bincode::deserialize(&e.payload).map_err(|err| read_failed("function signature", err))?;
                root.module_mut(&e.ns).fns.insert(e.name, v);
            }
            cat::TYPE => {
                let v: AdtDef = bincode::deserialize(&e.payload).map_err(|err| read_failed("type", err))?;
                root.module_mut(&e.ns).types.insert(e.name, v);
            }
            cat::TRAIT => {
                let v: TraitDef = bincode::deserialize(&e.payload).map_err(|err| read_failed("trait", err))?;
                root.module_mut(&e.ns).traits.insert(e.name, v);
            }
            cat::MACRO => {
                let v: MacroDef = bincode::deserialize(&e.payload).map_err(|err| read_failed("macro", err))?;
                root.module_mut(&e.ns).macros.insert(e.name, v);
            }
            cat::CTOR => {
                let v: (Path, usize) = bincode::deserialize(&e.payload).map_err(|err| read_failed("constructor", err))?;
                root.module_mut(&e.ns).ctors.insert(e.name, v);
            }
            cat::VAR => {
                let v: VarInfo = bincode::deserialize(&e.payload).map_err(|err| read_failed("global", err))?;
                root.module_mut(&e.ns).vars.insert(e.name, v);
            }
            cat::ALIAS => {
                let v: Vec<String> = bincode::deserialize(&e.payload).map_err(|err| read_failed("use alias", err))?;
                root.module_mut(&e.ns).aliases.insert(e.name, v);
            }
            cat::MOD_ALIAS => {
                let v: Vec<String> = bincode::deserialize(&e.payload).map_err(|err| read_failed("module alias", err))?;
                root.module_mut(&e.ns).mod_aliases.insert(e.name, v);
            }
            cat::STATIC_USE => {
                let v: (Path, String) = bincode::deserialize(&e.payload).map_err(|err| read_failed("static use", err))?;
                root.module_mut(&e.ns).static_uses.insert(e.name, v);
            }
            cat::BLANKET => {
                let v: BlanketImpl = bincode::deserialize(&e.payload).map_err(|err| read_failed("blanket impl", err))?;
                let index: usize =
                    e.name.parse().map_err(|_| format!("dump: blanket impl index `{}` is not a number", e.name))?;
                let list = &mut root.module_mut(&e.ns).blanket_impls;
                if index < list.len() {
                    list[index] = v;
                } else if index == list.len() {
                    list.push(v);
                } else {
                    return Err(format!(
                        "dump: blanket impl {} arrives with only {} in place — the units were applied out of order",
                        index,
                        list.len()
                    ));
                }
            }
            cat::THROW_TAG => {
                let v: Type = bincode::deserialize(&e.payload).map_err(|err| read_failed("throw tag", err))?;
                throw_tags.insert(e.name, v);
            }
            cat::PREDECLARED => predeclared(e.name),
            cat::DOC_FN => {
                docs.fns.insert(parse_path(&e.name), doc_text(&e.payload)?);
            }
            cat::DOC_TYPE => {
                docs.types.insert(parse_path(&e.name), doc_text(&e.payload)?);
            }
            cat::DOC_VAR => {
                docs.vars.insert(parse_path(&e.name), doc_text(&e.payload)?);
            }
            cat::DOC_TRAIT => {
                docs.traits.insert(parse_path(&e.name), doc_text(&e.payload)?);
            }
            cat::DOC_MACRO => {
                docs.macros.insert(parse_path(&e.name), doc_text(&e.payload)?);
            }
            cat::DOC_METHOD => {
                docs.methods.insert(parse_method_key(&e.name)?, doc_text(&e.payload)?);
            }
            cat::DOC_TRAIT_METHOD => {
                docs.trait_methods.insert(parse_method_key(&e.name)?, doc_text(&e.payload)?);
            }
            other => return Err(format!("dump: entry category {} is not one this build knows", other)),
        }
    }
    Ok(())
}

/// The one message shape every mis-read entry gets, so a corrupt or
/// version-skewed dump names the table it broke on.
fn read_failed(what: &str, err: bincode::Error) -> String {
    format!("dump: reading a {}: {}", what, err)
}

fn doc_text(payload: &[u8]) -> Result<String, String> {
    bincode::deserialize(payload).map_err(|e| format!("dump: reading a docstring: {}", e))
}

fn parse_method_key(key: &str) -> Result<(Path, String), String> {
    match key.split_once('|') {
        Some((path, method)) => Ok((parse_path(path), method.to_string())),
        None => Err(format!("dump: `{}` is not a method key", key)),
    }
}

/// `m::x` back into the `Path` the registry keys its tables by — the inverse of
/// the `Display` the walk wrote the key with.
pub fn parse_path(name: &str) -> Path {
    Path::from_segments(name.split("::").map(str::to_string).collect())
}

/// One definition whose native body a unit's bitcode carries.
///
/// The backend's own `CompiledItem` in wire form. Duplicated rather than
/// shared because the type info is the front end's to write down and the
/// bitcode is the backend's to install, and the front end must not depend on
/// the backend (that separation is what keeps `typelisp-front` out of an AOT
/// executable that never calls `eval`).
#[derive(Clone, Serialize, Deserialize)]
pub enum UnitItem {
    Fn(Path),
    Method(Path, String),
}

/// One compilation's output: what it added to the checker, and what its
/// bitcode holds bodies for.
///
/// Written into a dump next to the bitcode it describes. The two are produced
/// together, by the same pass, and are meaningless apart — an address with no
/// signature answers to no name, and a signature with no body type-checks a
/// call that then has nothing to run.
#[derive(Serialize, Deserialize)]
pub struct UnitState {
    /// [`FORMAT_VERSION`] as of writing. Checked by [`read_state`]: producer
    /// and consumer ship together, so a mismatch is a stale file rather than a
    /// version skew anyone has to migrate.
    pub version: u32,
    /// For diagnostics: "prelude", "island", a source path, "session".
    pub label: String,
    /// The hash of the source text this unit was built from, for the units
    /// (prelude, island) whose source is compiled into the binary and can
    /// therefore drift out from under the artifact. `None` for a unit built
    /// from source the loader will never see again.
    pub source_digest: Option<u64>,
    /// The hash of the *forms* the source read into, position-insensitive
    /// (`compile::bootstrap::hash_read_forms`). Not consulted at load time —
    /// `source_digest` is what says the dump is current. This is for the
    /// regeneration script, which reuses the committed bitcode section
    /// whenever the forms are unchanged, so editing a comment does not cost a
    /// full recompilation of the prelude or the island.
    pub forms_digest: Option<u64>,
    pub checker: CheckerDelta,
    /// Every checked top-level core form this unit defines, in declaration
    /// order. Re-executed on load, which is what fills the interpreter's
    /// function/type/macro tables.
    pub forms: Vec<OwnedForm>,
    /// Every definition the bitcode section carries a body for.
    pub items: Vec<UnitItem>,
    /// `(global path, compiled slot id)`. Written as the pairs that were
    /// actually assigned rather than as an order to re-derive: `promote_global`
    /// is called both eagerly in declaration order *and* lazily by the compile
    /// driver when it first sees a reference, so the sequence is not something
    /// a second process can reproduce by walking the source.
    pub globals: Vec<(String, usize)>,
}

/// Writes down what a unit added.
///
/// `checker` comes from [`crate::Checker::capture_delta`], taken against a
/// signature from before the unit loaded — and taken *before any later unit
/// loads*, since the checker they all share cannot say afterwards which of
/// them added what. `forms` are the unit's checked top-level forms in
/// declaration order, and must still be rooted: this reads them out of `heap`.
pub fn capture_types(
    heap: &crate::Heap,
    checker: CheckerDelta,
    label: &str,
    source_digest: Option<u64>,
    forms_digest: Option<u64>,
    forms: &[crate::Value],
    items: Vec<UnitItem>,
    globals: Vec<(String, usize)>,
) -> Result<UnitState, String> {
    Ok(UnitState {
        version: FORMAT_VERSION,
        label: label.to_string(),
        source_digest,
        forms_digest,
        checker,
        forms: forms
            .iter()
            .map(|v| crate::owned_form::value_to_owned(heap, *v).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?,
        items,
        globals,
    })
}

/// [`capture_types`]'s inverse: put a unit's checker state and definitions
/// back, in `heap`.
///
/// The forms are rebuilt and executed one at a time rather than all at once —
/// each is rooted only across its own `exec`, which is where it acquires the
/// permanent root that keeps it alive after.
///
/// This is the front end's half of loading a unit. Installing the native
/// bodies over the definitions it registers is the backend's
/// (`compile::dump::load_unit`), and a build with no LLVM backend can do this
/// half alone and run everything interpreted.
///
/// Deliberately does **not** touch the unit's globals: whether they have to be
/// *created* here or merely *bound* to storage somebody else made is the one
/// thing that differs between the two callers, and neither answer is safe to
/// guess. A JIT load owns the storage and calls `Interp::promote_global` after
/// this (`compile::dump::load_unit`); an AOT executable's compiled startup
/// creates it, so that caller calls [`bind_globals`] *before* this — which is
/// also what lets `already_initialized_global` below recognise the program's
/// own `defvar`s.
pub fn apply_types(
    heap: &mut crate::Heap,
    chk: &mut crate::Checker,
    interp: &mut crate::Interp,
    unit: UnitState,
) -> Result<(), String> {
    chk.apply_delta(unit.checker, heap)?;
    for f in &unit.forms {
        let tl = crate::owned_form::owned_to_value(heap, f).map_err(|e| e.to_string())?;
        heap.push_root(tl);
        let skip = already_initialized_global(heap, interp, tl);
        let r = if skip { Ok(None) } else { interp.exec(heap, tl) };
        heap.pop_root();
        r.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Records that these globals already have storage, made by somebody else.
///
/// The AOT half of the split described on [`apply_types`].
pub fn bind_globals(interp: &crate::Interp, globals: &[(String, usize)]) {
    for (name, id) in globals {
        interp.bind_compiled_global(parse_path(name), *id);
    }
}

/// Whether `tl` is a `defvar` for a global the compiled program has already
/// created and initialized.
///
/// Executing it would run the initializer a second time — with its side
/// effects, and overwriting whatever the program has since stored — because the
/// interpreter's `defvar` always evaluates and assigns (typelisp's `defvar` is
/// not CL's "only if unbound"). Skipping it costs nothing else: the *checker*
/// still saw the form, which is what a later `eval` needs to type-check a
/// reference, and both reads and writes consult `compiled_globals` before the
/// module tree.
///
/// Through `core::path_field`, not by matching `Value::Path`: a root-level name
/// is stored as a bare `Value::Symbol`, so reading the field by hand silently
/// misses every unqualified global — which is most of them.
pub fn already_initialized_global(heap: &crate::Heap, interp: &crate::Interp, tl: crate::Value) -> bool {
    if crate::check::core::op(heap, tl) != Some("defvar") {
        return false;
    }
    match crate::check::core::path_field(heap, tl, 0) {
        Some(path) => interp.has_compiled_global(&path),
        None => false,
    }
}

/// A unit's checked state as bytes.
///
/// The wire format is this crate's, so the encoder lives here rather than in
/// the backend that assembles the file around it — nothing outside this module
/// needs to know it is bincode.
pub fn write_state(state: &UnitState) -> Result<Vec<u8>, String> {
    bincode::serialize(state).map_err(|e| format!("{}: writing the dump failed: {}", state.label, e))
}

/// [`write_state`]'s inverse.
pub fn read_state(bytes: &[u8], label: &str) -> Result<UnitState, String> {
    let state: UnitState = bincode::deserialize(bytes)
        .map_err(|e| format!("{}: reading the dump's checked state failed: {}", label, e))?;
    if state.version != FORMAT_VERSION {
        return Err(format!(
            "{}: dump unit version {}, expected {} — the file and this build are from different \
             sources",
            label, state.version, FORMAT_VERSION
        ));
    }
    Ok(state)
}

/// The staleness key for a unit whose source ships in the same binary as the
/// dump — the prelude and the compiler island.
///
/// Over the source *bytes*, deliberately, and not over the forms the reader
/// produced the way the old `.bc` freshness check was. A unit records checked
/// forms, and a checked form carries its `Loc`: adding a comment line shifts
/// every position after it, so an artifact that survived a comment edit would
/// be reporting the wrong line for the rest of its life. Comment edits
/// therefore need a regeneration now — cheap, since the regeneration script
/// keeps the bitcode section it already has whenever the *forms* are unchanged.
pub fn source_digest(source: &str) -> u64 {
    let mut h = DefaultHasher::new();
    source.hash(&mut h);
    h.finish()
}

/// Fails unless `state` was built from `source`.
///
/// An error, never a silent rebuild: a dump that disagrees with the source
/// compiled into the same binary is a build the developer has to fix, and
/// quietly falling back to reading the source would hide exactly the case this
/// exists to catch — an edit that appears to do nothing.
pub fn verify_digest(state: &UnitState, source: &str, regen_script: &str) -> Result<(), String> {
    match state.source_digest {
        Some(d) if d == source_digest(source) => Ok(()),
        Some(_) => Err(format!(
            "{}: the dump is stale relative to its source — run {}",
            state.label, regen_script
        )),
        None => Err(format!(
            "{}: the dump carries no source digest, but is being loaded against a source that could \
             have changed — run {}",
            state.label, regen_script
        )),
    }
}

/// The label a program's own unit carries in an AOT executable's embedded dump.
pub const PROGRAM_LABEL: &str = "<program>";

/// Rebuilds an embedded environment dump in `heap` — the inverse of
/// `compile::dump::capture_program_dump`, which is what wrote it.
///
/// Applies each unit in order. No bitcode is installed — a dump embedded in an
/// executable carries none, and this crate has no installer anyway.
pub fn restore_dump(heap: &mut crate::Heap, bytes: &[u8]) -> Result<crate::Interp, String> {
    let mut chk = crate::Checker::new();
    // Before anything runs: `Interp::new` resets the runtime global table
    // (`typelisp_rt::reset_global_table`), which is why an AOT program's startup
    // calls this ahead of its own global-init sequence.
    let mut interp = crate::Interp::new();

    for unit in parse(bytes, "eval environment")? {
        let state = read_state(unit.types, "eval environment")?;
        // Every unit's globals are somebody else's storage: an AOT executable
        // carries the prelude's compiled bodies as well as its own, and those
        // bodies address their globals by baked-in slot id, so the executable's
        // startup created the storage for the prelude's `defvar`s before this
        // ran (`compile::aot::compile_file`'s global-init sequence). Binding
        // them here is what makes an eval'd `*print-pretty*` read that same
        // storage instead of a second, interpreted copy of it — and it is why
        // the `defvar`s below are skipped rather than re-assigned.
        bind_globals(&interp, &state.globals);
        apply_types(heap, &mut chk, &mut interp, state)?;
    }

    chk.take_warnings();
    interp.set_checker(std::rc::Rc::new(std::cell::RefCell::new(chk)));
    Ok(interp)
}

/// Collects the definitions in one checked top-level form, for a session that
/// is recording what it defines.
///
/// A dump records definitions, not history: re-running a session's `(println
/// ...)` on load is nobody's idea of a dump, and a file-derived `(module ...)`
/// wrapping the whole script would drag every one of them in — including the
/// `(dump ...)` call itself. So a container contributes *nothing* here: its
/// items come back through `Interp::exec` on their own and each records itself,
/// which also puts them in the order they ran. That covers a written `module`,
/// an `impl` block's grouping, a `deftrait`'s empty one, and the checker's
/// monomorphization bundle (whose specializations are real `defun`s compiled
/// code may call by their mangled names) with one rule instead of four.
pub fn record_definitions(heap: &crate::Heap, tl: crate::Value, out: &mut Vec<crate::Value>) {
    if matches!(
        crate::check::core::op(heap, tl),
        Some("defun" | "defmethod" | "defvar" | "defstruct" | "defenum" | "defmacro" | "use")
    ) {
        out.push(tl);
    }
}
