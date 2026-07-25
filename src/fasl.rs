//! The fasl ("fast-load") compiled-module format: a serialization of a
//! file's *checked* state, so `(load)`-ing it skips reading, macro
//! expansion, and type checking entirely.
//!
//! A fasl is **not** native code — the LLVM `(compile ...)`/`compile-file`
//! pipeline is an orthogonal runtime feature (and `compile-file` emits a
//! self-contained *executable*, a different artifact entirely). A fasl is
//! the CL-fasl-shaped snapshot of what checking a file produced:
//!
//! - [`RegistryDelta`] — the checker-side definitions the file added
//!   (function signatures, types, macros, traits, aliases, `DefLocs`), as a
//!   name-diff against the environment the file was checked in.
//! - The generic templates (`FnTemplate`/`MethodTemplate::Form`) the file
//!   retained, with their raw read forms converted to the heap-independent
//!   [`OwnedForm`] — the *only* place a checked file's state references the
//!   GC heap (see `Checker`'s template fields).
//! - The checked [`TopLevel`] sequence, re-`exec`d on load so the
//!   interpreter side (function bodies, macro bodies, `defvar` initializers,
//!   `struct_types`) is rebuilt through the exact same code path a source
//!   load uses — no second registration mechanism to drift.
//!
//! Loading never copies raw memory: every heap-resident value is
//! **re-allocated through the target heap's own allocation APIs**
//! ([`owned_to_value`] — `intern_symbol`/`alloc_string`/`heap.cons`/...), so
//! the result is indistinguishable from values the reader would have built.

use std::collections::{HashMap, HashSet};

use num_bigint::BigInt;
use num_rational::BigRational;
use serde::{Deserialize, Serialize};

use crate::check::registry::{AdtDef, FnSig, MacroDef, Namespace, TraitDef, VarInfo};
use crate::{Checker, Error, Heap, Interp, Loc, Path, TopLevel, Value};

/// An owned, GC-heap-independent mirror of a *read form* (`Value`) — the
/// serializable carrier for generic-template `parts`. The same idea as
/// `QuotedSexpr` (see `check::ast`), plus the [`OwnedForm::Path`] variant
/// `QuotedSexpr` deliberately lacks (quote strips `::`-paths during
/// checking; raw template forms still contain them).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OwnedForm {
    /// `Value::Empty` — the empty list `()`.
    Empty,
    Int(i64),
    Char(char),
    Bool(bool),
    /// An interned symbol, by name (re-interned on load).
    Sym(String),
    /// A string, by content (re-allocated on load).
    Str(String),
    /// A `::`-path, by segment names (re-interned on load).
    Path(Vec<String>),
    Float(f64),
    Bignum(BigInt),
    Ratio(BigRational),
    Cons(Box<OwnedForm>, Box<OwnedForm>),
}

/// Converts a heap `Value` into its owned mirror. Read-only on `heap`.
///
/// Handles exactly the shapes the reader produces (which is all a generic
/// template's `parts` can contain — they are raw, unchecked read forms):
/// atoms, interned symbols/paths, strings, cons cells, and the three boxed
/// numeric literals. Any other boxed object (a struct, closure, hashtable —
/// never reader output) is an internal error, reported rather than silently
/// mis-serialized.
pub fn value_to_owned(heap: &Heap, v: Value) -> Result<OwnedForm, Error> {
    Ok(match v {
        Value::Empty => OwnedForm::Empty,
        Value::Int(n) => OwnedForm::Int(n),
        Value::Char(c) => OwnedForm::Char(c),
        Value::Bool(b) => OwnedForm::Bool(b),
        Value::Symbol(id) => OwnedForm::Sym(heap.symbol_name(id).to_string()),
        Value::Str(id) => OwnedForm::Str(heap.string(id).to_string()),
        Value::Path(id) => OwnedForm::Path(
            heap.path_segments(id).iter().map(|s| heap.symbol_name(*s).to_string()).collect(),
        ),
        Value::Cons(_) => {
            let car = value_to_owned(heap, heap.car(v)?)?;
            let cdr = value_to_owned(heap, heap.cdr(v)?)?;
            OwnedForm::Cons(Box::new(car), Box::new(cdr))
        }
        Value::Boxed(id) if heap.is_float(id) => OwnedForm::Float(heap.float_value(id)),
        Value::Boxed(id) if heap.is_bignum(id) => OwnedForm::Bignum(heap.bignum_value(id).clone()),
        Value::Boxed(id) if heap.is_ratio(id) => OwnedForm::Ratio(heap.ratio_value(id).clone()),
        Value::Boxed(_) => {
            return Err(Error::TypeError(
                "fasl: a non-reader boxed value (struct/closure/hashtable) cannot be serialized".into(),
            ))
        }
    })
}

/// Rebuilds an [`OwnedForm`] as a live `Value` in `heap` — the load-time
/// half, allocating **through the heap's own APIs** (`intern_symbol`/
/// `alloc_string`/`intern_path`/`alloc_float`/`alloc_bignum`/`alloc_ratio`/
/// `cons`), never copying raw memory.
///
/// The returned value is *unrooted* — like a reader result, the caller must
/// root it (or store it somewhere the GC traces) before the next allocation.
/// Within a single call this function protects its own intermediates: while
/// a `Cons`'s cdr is being built, the already-built car is pushed as a GC
/// root (the `cdr`'s own allocations may trigger a collection), popped
/// LIFO before consing — the same discipline the reader itself follows.
pub fn owned_to_value(heap: &mut Heap, f: &OwnedForm) -> Result<Value, Error> {
    Ok(match f {
        OwnedForm::Empty => Value::Empty,
        OwnedForm::Int(n) => Value::Int(*n),
        OwnedForm::Char(c) => Value::Char(*c),
        OwnedForm::Bool(b) => Value::Bool(*b),
        OwnedForm::Sym(name) => heap.intern_symbol(name),
        OwnedForm::Str(s) => heap.alloc_string(s.clone()),
        OwnedForm::Path(segs) => {
            // Symbols are permanent (never collected), so the intermediate
            // `SymId`s need no rooting while later segments intern.
            let ids: Vec<_> = segs
                .iter()
                .map(|s| match heap.intern_symbol(s) {
                    Value::Symbol(id) => id,
                    other => unreachable!("intern_symbol returned {:?}", other),
                })
                .collect();
            heap.intern_path(&ids)
        }
        OwnedForm::Float(x) => heap.alloc_float(*x),
        OwnedForm::Bignum(n) => heap.alloc_bignum(n.clone()),
        OwnedForm::Ratio(r) => heap.alloc_ratio(r.clone()),
        OwnedForm::Cons(car, cdr) => {
            let car_v = owned_to_value(heap, car)?;
            heap.push_root(car_v);
            let cdr_v = match owned_to_value(heap, cdr) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root();
                    return Err(e);
                }
            };
            heap.push_root(cdr_v);
            let cell = heap.cons(car_v, cdr_v);
            heap.pop_root();
            heap.pop_root();
            cell?
        }
    })
}

// ---- Generic-template representations --------------------------------------

/// The serializable mirror of the checker's private `FnTemplate` (a generic
/// `defun` retained for per-instantiation re-checking) — `parts` are the raw
/// read forms as [`OwnedForm`]s. Converted by `Checker::export_templates` /
/// `install_templates`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FnTemplateRepr {
    pub parts: Vec<OwnedForm>,
    pub ns: Vec<String>,
    pub type_params: Vec<String>,
}

/// The serializable mirror of the checker's private `MethodTemplate` — see
/// [`FnTemplateRepr`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MethodTemplateRepr {
    Form { parts: Vec<OwnedForm>, ns: Vec<String>, written_vars: Vec<String> },
    Getter { index: usize },
    Setter { index: usize },
}

// ---- Registry delta ---------------------------------------------------------

/// A snapshot of which names each namespace already held at capture-start —
/// [`registry_mark`] takes it before a file is loaded, [`capture`] diffs
/// against it after, so the fasl carries exactly the entries the file added
/// (builtins and any pre-loaded context stay out).
pub struct RegistryMark {
    /// Namespace path (root = `[]`) -> the key sets of its definition maps.
    namespaces: HashMap<Vec<String>, NamespaceKeys>,
}

#[derive(Default)]
struct NamespaceKeys {
    fns: HashSet<String>,
    traits: HashSet<String>,
    macros: HashSet<String>,
    /// Keyed by type name -> the shape it had at mark time. A type is
    /// captured if it is new *or* its shape changed — the prelude adds
    /// `defmethod`s/`impl`s to *built-in* types (e.g. `iter` onto `vector`,
    /// `equals`/`less` onto `i32`), mutating an existing `AdtDef`'s `assoc`/
    /// `impls`/`trait_assoc` rather than adding a `types` entry, so a mere
    /// name diff would miss them.
    types: HashMap<String, TypeShape>,
    ctors: HashSet<String>,
    vars: HashSet<String>,
    aliases: HashSet<String>,
    mod_aliases: HashSet<String>,
    static_uses: HashSet<String>,
}

/// The parts of an `AdtDef` the prelude can grow after the type is first
/// registered — compared to decide whether a built-in type needs
/// re-capturing (see [`NamespaceKeys::types`]).
#[derive(Default, PartialEq)]
struct TypeShape {
    assoc: HashSet<String>,
    impls: usize,
    trait_assoc: HashSet<String>,
}

fn type_shape(def: &AdtDef) -> TypeShape {
    TypeShape {
        assoc: def.assoc.keys().cloned().collect(),
        impls: def.impls.len(),
        trait_assoc: def.trait_assoc.keys().map(|p| p.segments().join("::")).collect(),
    }
}

/// Takes a [`RegistryMark`] of `checker`'s current registry — call before
/// loading the file whose definitions the fasl should capture.
pub fn registry_mark(checker: &Checker) -> RegistryMark {
    let mut namespaces = HashMap::new();
    fn walk(ns: &Namespace, path: &mut Vec<String>, out: &mut HashMap<Vec<String>, NamespaceKeys>) {
        out.insert(
            path.clone(),
            NamespaceKeys {
                fns: ns.fns.keys().cloned().collect(),
                traits: ns.traits.keys().cloned().collect(),
                macros: ns.macros.keys().cloned().collect(),
                types: ns.types.iter().map(|(k, d)| (k.clone(), type_shape(d))).collect(),
                ctors: ns.ctors.keys().cloned().collect(),
                vars: ns.vars.keys().cloned().collect(),
                aliases: ns.aliases.keys().cloned().collect(),
                mod_aliases: ns.mod_aliases.keys().cloned().collect(),
                static_uses: ns.static_uses.keys().cloned().collect(),
            },
        );
        for (name, child) in &ns.modules {
            path.push(name.clone());
            walk(child, path, out);
            path.pop();
        }
    }
    walk(&checker.registry().root, &mut Vec::new(), &mut namespaces);
    RegistryMark { namespaces }
}

/// The definitions one namespace gained since the mark. Maps are stored as
/// `Vec` pairs (JSON object keys must be strings; `Path`-keyed maps are not).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamespaceDelta {
    pub path: Vec<String>,
    pub fns: Vec<(String, FnSig)>,
    pub traits: Vec<(String, TraitDef)>,
    pub macros: Vec<(String, MacroDef)>,
    pub types: Vec<(String, AdtDef)>,
    pub ctors: Vec<(String, (Path, usize))>,
    pub vars: Vec<(String, VarInfo)>,
    pub aliases: Vec<(String, Vec<String>)>,
    pub mod_aliases: Vec<(String, Vec<String>)>,
    pub static_uses: Vec<(String, (Path, String))>,
}

/// `DefLocs` as `Vec` pairs — see [`NamespaceDelta`] on why not the maps
/// themselves.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DefLocsRepr {
    pub fns: Vec<(Path, Loc)>,
    pub methods: Vec<((Path, String), Loc)>,
    pub types: Vec<(Path, Loc)>,
    pub vars: Vec<(Path, Loc)>,
    pub traits: Vec<(Path, Loc)>,
    pub macros: Vec<(Path, Loc)>,
}

/// Everything the checker gained from loading one file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegistryDelta {
    pub namespaces: Vec<NamespaceDelta>,
    pub def_locs: DefLocsRepr,
}

fn diff_namespace(ns: &Namespace, path: &mut Vec<String>, mark: &RegistryMark, out: &mut Vec<NamespaceDelta>) {
    static EMPTY: once_cell::sync::Lazy<NamespaceKeys> = once_cell::sync::Lazy::new(NamespaceKeys::default);
    let keys = mark.namespaces.get(path.as_slice()).unwrap_or(&EMPTY);
    let delta = NamespaceDelta {
        path: path.clone(),
        fns: ns.fns.iter().filter(|(k, _)| !keys.fns.contains(*k)).map(|(k, v)| (k.clone(), v.clone())).collect(),
        traits: ns.traits.iter().filter(|(k, _)| !keys.traits.contains(*k)).map(|(k, v)| (k.clone(), v.clone())).collect(),
        macros: ns.macros.iter().filter(|(k, _)| !keys.macros.contains(*k)).map(|(k, v)| (k.clone(), v.clone())).collect(),
        types: ns
            .types
            .iter()
            .filter(|(k, v)| keys.types.get(*k).map(|old| *old != type_shape(v)).unwrap_or(true))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        ctors: ns.ctors.iter().filter(|(k, _)| !keys.ctors.contains(*k)).map(|(k, v)| (k.clone(), v.clone())).collect(),
        vars: ns.vars.iter().filter(|(k, _)| !keys.vars.contains(*k)).map(|(k, v)| (k.clone(), v.clone())).collect(),
        aliases: ns.aliases.iter().filter(|(k, _)| !keys.aliases.contains(*k)).map(|(k, v)| (k.clone(), v.clone())).collect(),
        mod_aliases: ns
            .mod_aliases
            .iter()
            .filter(|(k, _)| !keys.mod_aliases.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        static_uses: ns
            .static_uses
            .iter()
            .filter(|(k, _)| !keys.static_uses.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };
    let nonempty = !(delta.fns.is_empty()
        && delta.traits.is_empty()
        && delta.macros.is_empty()
        && delta.types.is_empty()
        && delta.ctors.is_empty()
        && delta.vars.is_empty()
        && delta.aliases.is_empty()
        && delta.mod_aliases.is_empty()
        && delta.static_uses.is_empty());
    if nonempty {
        out.push(delta);
    }
    for (name, child) in &ns.modules {
        path.push(name.clone());
        diff_namespace(child, path, mark, out);
        path.pop();
    }
}

// ---- The fasl itself --------------------------------------------------------

/// Bump on any change to the serialized shape — a loader seeing a different
/// version silently falls back to the source file.
///
/// 3: added `QuotedSexpr::Path` (a `::`-qualified path inside quoted data,
/// e.g. a `defmacro` body's `'(dep::head)`), reachable from any serialized
/// `Expr::Quote`/`Typed` — 2026-07-15.
///
/// 4: re-added value-level `&rest`/`apply` (`FnSig.rest: Option<Type>`,
/// `Type::Fn`'s second field), reachable from any serialized function
/// signature or `Typed` whose type mentions a function type — 2026-07-15.
///
/// 5: `TopLevel::Defenum` now carries its `params`/`variants` (each
/// variant's field types, baked in at check time for the compiled-global
/// box decode — `Interp::enum_defs`) — 2026-07-16.
///
/// 6: enum values became heap-boxed (`BoxedObj::Enum`, the
/// enum-representation unification), which changes two pieces of *baked*
/// data without changing any serialized shape: `Pattern::Bind`'s
/// heap-slot-routing bool is now `true` for enum-typed bindings
/// (`Checker::is_heap_repr`'s "Sum with variants" arm), and an
/// enum-typed anything now classifies heap-repr throughout — a stale
/// cache's `false` would put a GC-heap value in a Native slot the GC
/// can't see through compiled code's own collections — 2026-07-16.
///
/// 7: the `Symbol`->`Sexpr` widening (`Checker::check_inner`'s reconciliation)
/// stopped wrapping the value in a `Sexpr::Sym` *constructor* node and now
/// emits a transparent retype instead — a `gensym`'d temp captured by a
/// macro-expansion lambda is compiled since interp-closure removal, and the
/// compiler lowered that constructor as a name-interning `compile-construct-sym`
/// that aborted on an already-built `Symbol`. A stale cache carries the old
/// `Construct` node and would re-hit the abort — 2026-07-19.
///
/// 8: `Interp`'s flat `Path`-keyed `fns`/`methods`/`globals`/`compiled`/
/// `compiled_methods`/`struct_types`/`enum_defs` tables were replaced by a
/// runtime module-scope tree (`eval::scope::ModuleScope`) that re-derives
/// visibility independently instead of trusting a checker-baked `Path` as a
/// lookup key. `Expr::Call`/`Global`/`FnRef`/`SetGlobal` now carry a `Ref`
/// (`written`/`home`/`resolved`) instead of a bare `Path`, and
/// `Expr::Assoc`/`MethodRef` gained a `home` field; `TopLevel::Defun`/
/// `Defmethod`/`Defvar`/`Defmacro` gained a `public` field. A stale cache
/// serialized under the old `Expr`/`TopLevel` shapes would fail to
/// deserialize — 2026-07-21.
///
/// 9: `(compile name)`/`(compile type::method)` stopped being a disguised
/// `Expr::Call` to a registered `compile` builtin taking a string argument —
/// `Checker::check_compile` now builds a dedicated `Expr::CompileFn
/// (CompileTarget)` node directly from its own `written`+`home`/`type_name`
/// resolution, the same independent re-resolution every other reference
/// gets, instead of handing `Interp` a bare name string to re-resolve with
/// an unqualified, module-blind search. A stale cache holding the old
/// `Expr::Call`-shaped node would fail to deserialize (or, worse, silently
/// keep the old module-blind resolution) — 2026-07-21.
///
/// 10: `Pattern` gained a `TypeTest(Type, Box<Pattern>)` variant (the Sexpr-
/// user-ADT design plan's `(the Type pattern)` downcast) and `Pattern::Ctor`
/// is no longer guaranteed same-ADT as its match's scrutinee (a Sexpr-
/// downcast `Ctor`'s `type_name` now names the downcast target). A stale
/// cache serialized before either change would fail to deserialize the new
/// shape (or worse, misroute an old same-shape `Ctor` as a downcast) —
/// 2026-07-24.
///
/// 11: `defmacro` lambda lists gained `&optional`/`&key`. `MacroDef` (the
/// serialized macro-registry entry) replaced its single `arity`/`rest` with
/// `required`/`optional`/`rest`/`keys`, and `TopLevel::Defmacro` gained a
/// `MacroLambda` (the checked default-value bodies + key names). A stale cache
/// under the old shapes would fail to deserialize — 2026-07-24.
///
/// 12: dynamic dispatch (TODO T4). `Type` gained `Dyn(Path, Vec<Type>)`,
/// `Expr` gained `SymLit`/`DynBox`/`DynCall`/`DynValue`, and `TraitDef`
/// gained `method_order` (the vtable slot layout). Every one of those is
/// serialized as part of a namespace delta or a checked top-level form, so a
/// stale cache would fail to deserialize the new shapes — and a `TraitDef`
/// read back without `method_order` would have no slot numbering at all —
/// 2026-07-25.
///
/// 13: user-defined error types (TODO T3). The single built-in `Error` type
/// is gone, replaced by one concrete type per fallible built-in
/// (`ParseIntError`/`ParseFloatError`/`ReadError`/`EvalError`) plus the
/// prelude `Error` *trait*; `where`-clause bounds now store the trait's
/// fully-qualified path instead of the bare written name. A stale cache
/// would hand back signatures naming a type that no longer exists and bounds
/// that no longer compare equal to any trait — 2026-07-25.
pub const FASL_FORMAT_VERSION: u32 = 13;

/// A compiled module: the complete checked state one `.typl` file produced,
/// heap-independent and serializable. See the module doc comment.
#[derive(Serialize, Deserialize)]
pub struct Fasl {
    pub format_version: u32,
    /// Hash of the source text this was compiled from ([`source_hash`]) —
    /// a loader whose source hashes differently treats the fasl as stale.
    pub source_hash: u64,
    pub delta: RegistryDelta,
    pub fn_templates: Vec<(Path, FnTemplateRepr)>,
    pub method_templates: Vec<(Path, String, MethodTemplateRepr)>,
    /// The checked top-level forms, in file order. Loading re-`exec`s them —
    /// the interpreter side (function/macro bodies, `defvar` initializers,
    /// `struct_types`) rebuilds through the exact same path a source load
    /// uses.
    pub top_levels: Vec<TopLevel>,
}

/// The stale-detection hash for fasl files: `DefaultHasher` over the source
/// bytes. Not cryptographic — it only needs to distinguish "the source
/// changed since this fasl was produced".
pub fn source_hash(src: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

impl Fasl {
    /// Captures everything `checker` gained since `mark` (plus the given
    /// already-checked `top_levels`) as a fasl. `heap` is the heap the
    /// capture-side templates live in (read-only — their forms are converted
    /// to [`OwnedForm`]s).
    pub fn capture(
        heap: &Heap,
        checker: &Checker,
        mark: &RegistryMark,
        top_levels: Vec<TopLevel>,
        source_hash: u64,
    ) -> Result<Fasl, Error> {
        let mut namespaces = Vec::new();
        diff_namespace(&checker.registry().root, &mut Vec::new(), mark, &mut namespaces);
        let dl = &checker.registry().def_locs;
        let def_locs = DefLocsRepr {
            fns: dl.fns.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            methods: dl.methods.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            types: dl.types.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            vars: dl.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            traits: dl.traits.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            macros: dl.macros.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        };
        let (fn_templates, method_templates) = checker.export_templates(heap)?;
        Ok(Fasl {
            format_version: FASL_FORMAT_VERSION,
            source_hash,
            delta: RegistryDelta { namespaces, def_locs },
            fn_templates,
            method_templates,
            top_levels,
        })
    }

    /// Applies this fasl to a live environment: inserts the registry
    /// entries, rebuilds the generic templates in `heap` (allocation APIs +
    /// permanent roots — `Checker::install_templates`), then re-`exec`s the
    /// checked top-level forms so the interpreter registers everything the
    /// source load would have.
    pub fn load_into(&self, heap: &mut Heap, checker: &mut Checker, interp: &mut Interp) -> Result<(), Error> {
        for nd in &self.delta.namespaces {
            let ns = checker.registry_mut().root.module_mut(&nd.path);
            ns.fns.extend(nd.fns.iter().cloned());
            ns.traits.extend(nd.traits.iter().cloned());
            ns.macros.extend(nd.macros.iter().cloned());
            ns.types.extend(nd.types.iter().cloned());
            ns.ctors.extend(nd.ctors.iter().cloned());
            ns.vars.extend(nd.vars.iter().cloned());
            ns.aliases.extend(nd.aliases.iter().cloned());
            ns.mod_aliases.extend(nd.mod_aliases.iter().cloned());
            ns.static_uses.extend(nd.static_uses.iter().cloned());
        }
        {
            let dl = &mut checker.registry_mut().def_locs;
            dl.fns.extend(self.delta.def_locs.fns.iter().cloned());
            dl.methods.extend(self.delta.def_locs.methods.iter().cloned());
            dl.types.extend(self.delta.def_locs.types.iter().cloned());
            dl.vars.extend(self.delta.def_locs.vars.iter().cloned());
            dl.traits.extend(self.delta.def_locs.traits.iter().cloned());
            dl.macros.extend(self.delta.def_locs.macros.iter().cloned());
        }
        checker.install_templates(heap, &self.fn_templates, &self.method_templates)?;
        for tl in &self.top_levels {
            interp
                .exec(heap, tl.clone())
                .map_err(|e| Error::TypeError(format!("fasl load: exec failed: {}", e)))?;
        }
        Ok(())
    }

    /// Serializes to the on-disk byte form (JSON today; the format is an
    /// implementation detail behind this pair of functions).
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(self).map_err(|e| Error::TypeError(format!("fasl serialize: {}", e)))
    }

    /// Deserializes [`Self::to_bytes`]' output. A version mismatch is an
    /// `Err` like any other parse failure — callers treat every failure the
    /// same way (fall back to the source file).
    pub fn from_bytes(bytes: &[u8]) -> Result<Fasl, Error> {
        let fasl: Fasl = serde_json::from_slice(bytes).map_err(|e| Error::TypeError(format!("fasl parse: {}", e)))?;
        if fasl.format_version != FASL_FORMAT_VERSION {
            return Err(Error::TypeError(format!(
                "fasl format version {} does not match this build's {}",
                fasl.format_version, FASL_FORMAT_VERSION
            )));
        }
        Ok(fasl)
    }
}
