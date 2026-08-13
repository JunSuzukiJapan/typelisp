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

use crate::check::registry::{AdtDef, BlanketImpl, FnSig, MacroDef, Namespace, TraitDef, VarInfo};
use crate::{Checker, Error, Heap, Interp, Loc, Path, TopLevelForm, Value};

/// An owned, GC-heap-independent mirror of a *read form* (`Value`) — the
/// serializable carrier for generic-template `parts`. The same idea as
/// the deleted `QuotedSexpr`, plus the [`OwnedForm::Path`] variant that one
/// deliberately lacked (quote strips `::`-paths during checking; raw template
/// forms still contain them).
///
/// A `Cons` carries the two source spans its cell holds, because a location is
/// a property of the datum and this is that datum written down: dropping them
/// here would mean a form that came back from a fasl could not say where it was
/// written, and every runtime error in it would lose its `file:line:col`
/// (`tests/fasl_test.rs`'s `a_fasl_loaded_function_reports_its_own_source_position`
/// is the guard). The rebuild re-records them, which is possible precisely
/// because the location travels *with* the cell rather than in a table keyed by
/// an address that no longer exists.
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
    /// A cons chain, spine flattened: one entry per cell, then whatever the
    /// last cell's `cdr` holds (`Empty` for a proper list, an atom for a dotted
    /// one like `(a . b)`).
    ///
    /// Flat rather than the right-nested pair the cells actually are, because
    /// the nesting is what a *list* costs in a depth-limited format: this
    /// serializes as JSON, whose parser refuses past 128 levels of nesting, and
    /// a right-nested pair makes a list of N elements N levels deep. A checked
    /// body is a list now, and the prelude's own bodies exceeded the limit — no
    /// fasl could be read back at all (`fasl parse: recursion limit exceeded`).
    /// Depth here is structural nesting only. Raising the parser's limit instead
    /// would have traded a clean error for a stack overflow, since the limit is
    /// what stands between deep input and one; flattening removes the growth.
    ///
    /// Both walks iterate the spine for the same reason one level down: a
    /// pair-at-a-time recursion spends one Rust stack frame per list element.
    Cons { cells: Vec<OwnedCell>, tail: Box<OwnedForm> },
}

/// One cell of a flattened cons spine ([`OwnedForm::Cons`]): the element in the
/// cell's `car`, plus the two spans the cell carries — its `car`'s own extent
/// and the extent of the form this cell heads. `None` for a cell with none (a
/// synthesized or macro-expanded list).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OwnedCell {
    pub form: OwnedForm,
    pub car_loc: Option<Loc>,
    pub self_loc: Option<Loc>,
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
            // The spine iteratively; only the elements recurse.
            let mut cells = Vec::new();
            let mut cur = v;
            let tail = loop {
                match cur {
                    Value::Cons(cr) => {
                        cells.push(OwnedCell {
                            form: value_to_owned(heap, heap.car(cur)?)?,
                            car_loc: heap.elem_loc(cr),
                            self_loc: heap.cons_loc(cur),
                        });
                        cur = heap.cdr(cur)?;
                    }
                    other => break value_to_owned(heap, other)?,
                }
            };
            OwnedForm::Cons { cells, tail: Box::new(tail) }
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
        OwnedForm::Cons { cells, tail } => {
            // Back to front, so each `cons` already has its cdr in hand — which
            // is also what keeps the rooting to one intermediate: the growing
            // tail is the only thing that has to survive the next allocation.
            // Rooted for the whole arm and released on the way out, so an error
            // partway through leaves no residue.
            let roots_base = heap.root_count();
            let built = (|| -> Result<Value, Error> {
                let mut acc = owned_to_value(heap, tail)?;
                heap.push_root(acc);
                for cell in cells.iter().rev() {
                    let car_v = owned_to_value(heap, &cell.form)?;
                    heap.push_root(car_v);
                    // core-build-ok: rebuilding a flattened spine read back
                    // from disk, where the elements arrive one at a time. Both
                    // halves are rooted above, and the explicit `roots_base`
                    // unwind below covers the error paths a `RootScope` would.
                    let consed = heap.cons(car_v, acc)?;
                    heap.push_root(consed);
                    // The spans go back into the rebuilt cell, not into a table
                    // keyed by the old one's address — which is what makes them
                    // survive at all, since that address belongs to a heap that
                    // is gone.
                    if let Value::Cons(cr) = consed {
                        if let Some(loc) = &cell.car_loc {
                            heap.set_elem_loc(cr, loc.clone());
                        }
                        if let Some(loc) = &cell.self_loc {
                            heap.set_cons_loc(cr, loc.clone());
                        }
                    }
                    acc = consed;
                }
                Ok(acc)
            })();
            heap.truncate_roots(roots_base);
            built?
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
    /// Keyed by trait name -> the shape it had at mark time, for the same
    /// reason [`Self::types`] is: a `deftrait` edited in place keeps its name,
    /// so a name-only diff would leave the fasl serving the *old* trait —
    /// stale supertraits, stale vtable layout, stale default bodies.
    traits: HashMap<String, TraitShape>,
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
    /// How many blanket impls the namespace already held — see
    /// [`NamespaceDelta::blanket_impls`].
    blanket_impls: usize,
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

/// The parts of a `TraitDef` an edit can change without changing its name —
/// see [`NamespaceKeys::traits`]. Deliberately *not* `PartialEq` on `TraitDef`
/// itself: that would cascade through `FnSig::optionals` ->
/// `OptKeyParam::default` hold a checked form and demand `PartialEq` across the
/// whole checked AST.
#[derive(Default, PartialEq)]
struct TraitShape {
    supertraits: Vec<String>,
    assoc_types: Vec<String>,
    method_order: Vec<String>,
    vtable_order: Vec<String>,
}

fn trait_shape(def: &TraitDef) -> TraitShape {
    TraitShape {
        supertraits: def.supertraits.iter().map(|b| b.trait_path.to_string()).collect(),
        assoc_types: def.assoc_types.clone(),
        method_order: def.method_order.clone(),
        vtable_order: def.vtable_order.clone(),
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
                traits: ns.traits.iter().map(|(k, d)| (k.clone(), trait_shape(d))).collect(),
                macros: ns.macros.keys().cloned().collect(),
                types: ns.types.iter().map(|(k, d)| (k.clone(), type_shape(d))).collect(),
                ctors: ns.ctors.keys().cloned().collect(),
                vars: ns.vars.keys().cloned().collect(),
                aliases: ns.aliases.keys().cloned().collect(),
                mod_aliases: ns.mod_aliases.keys().cloned().collect(),
                static_uses: ns.static_uses.keys().cloned().collect(),
                blanket_impls: ns.blanket_impls.len(),
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
    /// Blanket `impl`s the file declared. Unlike every other entry here these
    /// have no name to diff on, so the count at mark time stands in: a file
    /// only ever *appends* to this list.
    pub blanket_impls: Vec<BlanketImpl>,
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

/// `Docs` as `Vec` pairs — see [`NamespaceDelta`] on why not the maps
/// themselves.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DocsRepr {
    pub fns: Vec<(Path, String)>,
    pub methods: Vec<((Path, String), String)>,
    pub types: Vec<(Path, String)>,
    pub vars: Vec<(Path, String)>,
    pub traits: Vec<(Path, String)>,
    pub macros: Vec<(Path, String)>,
    pub trait_methods: Vec<((Path, String), String)>,
}

/// Everything the checker gained from loading one file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegistryDelta {
    pub namespaces: Vec<NamespaceDelta>,
    pub def_locs: DefLocsRepr,
    pub docs: DocsRepr,
}

fn diff_namespace(ns: &Namespace, path: &mut Vec<String>, mark: &RegistryMark, out: &mut Vec<NamespaceDelta>) {
    static EMPTY: once_cell::sync::Lazy<NamespaceKeys> = once_cell::sync::Lazy::new(NamespaceKeys::default);
    let keys = mark.namespaces.get(path.as_slice()).unwrap_or(&EMPTY);
    let delta = NamespaceDelta {
        path: path.clone(),
        fns: ns.fns.iter().filter(|(k, _)| !keys.fns.contains(*k)).map(|(k, v)| (k.clone(), v.clone())).collect(),
        traits: ns
            .traits
            .iter()
            .filter(|(k, v)| keys.traits.get(*k).map(|old| *old != trait_shape(v)).unwrap_or(true))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
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
        blanket_impls: ns.blanket_impls[keys.blanket_impls.min(ns.blanket_impls.len())..].to_vec(),
    };
    let nonempty = !(delta.fns.is_empty()
        && delta.traits.is_empty()
        && delta.macros.is_empty()
        && delta.types.is_empty()
        && delta.ctors.is_empty()
        && delta.vars.is_empty()
        && delta.aliases.is_empty()
        && delta.mod_aliases.is_empty()
        && delta.static_uses.is_empty()
        && delta.blanket_impls.is_empty());
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
/// `quote`/`Typed` — 2026-07-15.
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
/// lookup key. `call`/`Global`/`FnRef`/`SetGlobal` now carry a `Ref`
/// (`written`/`home`/`resolved`) instead of a bare `Path`, and
/// `assoc`/`MethodRef` gained a `home` field; `TopLevel::Defun`/
/// `Defmethod`/`Defvar`/`Defmacro` gained a `public` field. A stale cache
/// serialized under the old `Expr`/`TopLevel` shapes would fail to
/// deserialize — 2026-07-21.
///
/// 9: `(compile name)`/`(compile type::method)` stopped being a disguised
/// `call` to a registered `compile` builtin taking a string argument —
/// `Checker::check_compile` now builds a dedicated `compile-fn` node directly from its own `written`+`home`/`type_name`
/// resolution, the same independent re-resolution every other reference
/// gets, instead of handing `Interp` a bare name string to re-resolve with
/// an unqualified, module-blind search. A stale cache holding the old
/// `call`-shaped node would fail to deserialize (or, worse, silently
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
///
/// 14: `defun` `&optional`/`&key` parameters. `FnSig` gained `optionals`/
/// `keys: Vec<OptKeyParam>` (each parameter's declared type and checked
/// default expression, if any). A stale cache would hand back a signature
/// missing both fields, so every call to a function declaring either would
/// wrongly be checked as ordinary fixed arity — 2026-07-29.
///
/// 15: docstrings (CL-equivalent `documentation`). `RegistryDelta` gained a
/// `docs: DocsRepr` table alongside `def_locs`, populated by every
/// user-facing definition form that accepts a docstring (`defun`/
/// `defmethod`/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
/// `deftrait`). Unlike most bumps here, a stale cache wouldn't actually fail
/// to *deserialize* (the new field would just be missing from the old JSON,
/// and `serde` would reject it as a missing struct field the same way any
/// other shape mismatch is rejected) — but without the bump, `(documentation
/// ...)` would silently return `none` for every definition a stale-cached
/// dependency provides, since `documentation` resolves entirely at check
/// time from whatever `Docs` table the dependency's fasl handed back — no
/// runtime fallback exists to catch a missing docstring later — 2026-07-30.
///
/// Bumped to 16 for streams and files (CLHS 19/20/21): `Type` gained a
/// `Stream` variant and the built-in error types gained `FileError`. Neither
/// changes how an *existing* fasl deserializes (this format is JSON, so
/// variants travel by name), but both change the registry a cached module is
/// reconstructed against — the trap `docs/dev/development.md` records for
/// registry changes generally — so the bump invalidates every stale cache in
/// one move rather than leaving it to a `rm` nobody remembers — 2026-08-01.
///
/// 17: the trait system reaching Rust parity. `deftrait` gained a mandatory
/// supertrait list right after the name, and `TraitDef` gained
/// `supertraits`/`vtable_order`/`vtable_owner`/`defaults`. `vtable_order` is
/// the trait-object slot layout, which compiled call sites index by a
/// baked-in constant, so a stale cache would hand back a `TraitDef` with no
/// layout at all and leave every `:dyn` call on it unable to find a slot;
/// `defaults` carries method bodies that `check_impl` synthesizes from, so
/// without them an `impl` relying on one would look incomplete. `Namespace`
/// additionally gained `blanket_impls` and `Docs` gained `trait_methods`.
/// This version also fixes the diff that decides whether a trait is
/// re-captured at all: it compared *names* only (see
/// [`NamespaceKeys::traits`]), so an edited trait kept serving its old
/// definition out of cache — 2026-08-01.
/// 18: non-leftmost supertrait `:dyn` upcasting. `Expr` gained `DynUpcast`
/// and `dyn-new` gained `supers` (the supertrait vtables a boxing site
/// lays out, which is what makes the conversion possible at all — see that
/// field's doc comment). A cached `DynBox` from before this carries no
/// `supers`, so an upcast of it would find no table registered and abort at
/// the conversion — 2026-08-04.
/// 19: source locations moved into the cons cell (from three side tables keyed
/// by cell address), so [`OwnedForm::Cons`] gained the two spans a cell carries
/// and every serialized cons is one JSON element longer. A stale cache would
/// fail to deserialize the new shape — and, if it somehow did not, would hand
/// back forms whose positions were never written, silently costing every
/// runtime error in a cached module its `file:line:col` — 2026-08-09.
/// 20: [`OwnedForm::Cons`] flattened its spine (`{ cells, tail }`, one entry
/// per cell) from a right-nested pair. Not a change of *what* is recorded — the
/// same cells and the same two spans each — but of how deeply it nests. Version
/// 19 made a checked body a list of cons cells, and a right-nested pair makes a
/// list of N elements N levels deep in JSON, so the prelude's own bodies blew
/// past serde_json's 128-level parse limit and no fasl could be read back at
/// all. A stale cache would fail to deserialize the new shape — 2026-08-11.
pub const FASL_FORMAT_VERSION: u32 = 20;

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
    /// type entries) rebuilds through the exact same path a source load uses.
    ///
    /// [`OwnedForm`]s, not the forms themselves: a checked top-level form is
    /// cons cells since Stage C, and a `Value` is an index into *a* heap.
    /// Rebuilding through the allocation API is also the only correct way back
    /// — see [`owned_to_value`]. The source positions survive because
    /// `OwnedForm::Cons` carries both of a cell's location slots.
    pub top_levels: Vec<OwnedForm>,
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
        top_levels: Vec<TopLevelForm>,
        source_hash: u64,
    ) -> Result<Fasl, Error> {
        let top_levels = top_levels
            .into_iter()
            .map(|tl| value_to_owned(heap, tl))
            .collect::<Result<Vec<_>, _>>()?;
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
        let dc = &checker.registry().docs;
        let docs = DocsRepr {
            fns: dc.fns.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            methods: dc.methods.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            types: dc.types.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            vars: dc.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            traits: dc.traits.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            macros: dc.macros.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            trait_methods: dc.trait_methods.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        };
        let (fn_templates, method_templates) = checker.export_templates(heap)?;
        Ok(Fasl {
            format_version: FASL_FORMAT_VERSION,
            source_hash,
            delta: RegistryDelta { namespaces, def_locs, docs },
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
            ns.blanket_impls.extend(nd.blanket_impls.iter().cloned());
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
        {
            let dc = &mut checker.registry_mut().docs;
            dc.fns.extend(self.delta.docs.fns.iter().cloned());
            dc.methods.extend(self.delta.docs.methods.iter().cloned());
            dc.types.extend(self.delta.docs.types.iter().cloned());
            dc.vars.extend(self.delta.docs.vars.iter().cloned());
            dc.traits.extend(self.delta.docs.traits.iter().cloned());
            dc.macros.extend(self.delta.docs.macros.iter().cloned());
            dc.trait_methods.extend(self.delta.docs.trait_methods.iter().cloned());
        }
        checker.install_templates(heap, &self.fn_templates, &self.method_templates)?;
        for tl in &self.top_levels {
            // Rebuilt in *this* heap's cells, then permanently rooted by
            // `exec` exactly as a freshly-checked form is.
            let form = owned_to_value(heap, tl)?;
            interp
                .exec(heap, form)
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
