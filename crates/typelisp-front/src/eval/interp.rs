//! A tree-walking interpreter over the checker's typed AST.
//!
//! Top-level definitions (`defun`/`defmethod`) are registered at [`Interp::root`],
//! a runtime mirror of the checker's own module tree (see `eval::scope`'s module
//! doc comment) — each at the node for its own defining module, under its own
//! unqualified name, never in one flat table spanning the whole program.
//! Expressions are evaluated against that tree plus a lexical environment.
//! Functions are not closures — a body sees only its parameters and the global
//! definitions, matching top-level `defun`/`defmethod` semantics.
//!
//! Values live in the GC-managed [`Heap`] shared with the reader, so `eval`
//! threads a `&mut Heap` throughout. Every mutable [`Slot`] *is* a heap cell,
//! which the collector finds through its own `cell_registry` on every
//! collection — so the interpreter pushes no roots for bindings at all. What
//! it does still have to root by hand is anything held only on the Rust stack
//! across an allocation: partially-built structures inside a builder, and
//! arguments staged for a crossing into compiled code.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::rc::Rc;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, ToPrimitive, Zero};

use crate::check::registry::EVAL_ERROR;
use crate::type_key::{alloc_typed_enum, heap_type_path};
use typelisp_mem::RootScope;

use crate::check::core;
use crate::check::repr::Repr;
use crate::{BoxId, Heap, MacroExpander, MacroLambda, Path, Ref, SymId, Value};

use super::scope;
pub use super::value::EvalError;
use super::value::Slot;

/// The evaluator over core forms — the cons-cell program representation that
/// replaces the `Typed` tree. A child module rather than a sibling of
/// `interp`, so it can reach this module's private items (the builtin
/// dispatch, the module tree) while it is being built alongside the evaluator
/// it will replace.
mod core_eval;

/// A registered function or method body with its parameter names. Lives at
/// exactly one [`scope::ModuleScope`] tree node — its own defining module —
/// rather than in a flat program-wide table; see that module's doc comment.
pub struct FnDef {
    /// Parameter names, including the receiver name first for instance methods.
    pub params: Vec<String>,
    /// The body's core forms, in order.
    ///
    /// These are heap cells, which the old `Vec<Typed>` was not — so the
    /// module tree is now a *source of GC roots*. `Interp::exec` permanently
    /// roots the whole top-level form it registers (one root per definition,
    /// covering the body, the parameter list and every representation
    /// reachable from it); see `Heap::push_permanent_root`, which is
    /// LIFO-independent for exactly this.
    pub body: Vec<Value>,
    /// Only ever set for a `defmacro` with a trailing `&rest` parameter (see
    /// [`Interp::expand_macro`]); always `false` for `defun`/`defmethod`,
    /// which `apply` calls 1:1 regardless.
    pub rest: bool,
    /// The `&optional`/`&key` structure of a `defmacro`'s lambda list —
    /// `Some` only for a macro (its default-value bodies and keyword names,
    /// consumed by [`Interp::bind_macro_args`]), `None` for `defun`/
    /// `defmethod`, which bind their arguments 1:1 with no defaults. `params`
    /// still lists every binding name in order; this only adds how the
    /// non-required regions are filled.
    pub lambda: Option<MacroLambda>,
    /// `(parameter representations, return representation)`, parallel to
    /// `params` — `None` for a `defmacro` (every parameter and the implicit
    /// return are always `Sexpr`, see `check::registry::MacroDef`'s doc
    /// comment) since a macro is never a `compile` target.
    ///
    /// Representations, not `Type`s: the type is gone from the IR by the time
    /// a definition is registered, and what the two consumers actually need is
    /// exactly what a `Repr` says. `Interp::compile_function` needs an LLVM
    /// signature, and `encode_crossing_args` must be driven by the *declared*
    /// shape rather than the runtime value's — see
    /// `docs/dev/` and the crossing rule: whether a word is raw or tagged is
    /// in the declaration only.
    pub sig: Option<(Vec<Repr>, Repr)>,
    /// Whether this definition is `pub` — the runtime twin of
    /// `FnSig`/`VarInfo`/`AssocFn.sig`'s own `public` bit
    /// (`check::registry`), baked onto the `TopLevel` node by the checker
    /// (which computed it once already) so `scope::ModuleScope`'s qualified-
    /// path resolution can enforce `Checker::resolve_fn_path`'s
    /// `public || in_scope` gate without needing the checker's `Registry` at
    /// runtime.
    pub public: bool,
    /// This definition's JIT/AOT-compiled form, if `(compile name)` (or an
    /// AOT pass) has ever produced one — the direct replacement for the old
    /// standalone `Interp::compiled`/`compiled_methods` side tables (`Path`-
    /// keyed, "must never drift" from `fns`/`methods` by discipline alone).
    /// Living on the same node `scope::ModuleScope::resolve_fn`/
    /// `resolve_method` already found eliminates that duplication
    /// structurally: the compiled-fast-path check in `Call`/`Assoc` eval is
    /// just "does the `FnDef` I already have in hand carry one?". Purely a
    /// machine-code-linkage implementation detail — never consulted by this
    /// module's own visibility/resolution logic.
    ///
    /// Its own `RefCell`, separate from the rest of this struct (the tree
    /// stores `Rc<FnDef>`, not `Rc<RefCell<FnDef>>`): `Self::apply` holds a
    /// plain `&FnDef` borrowed out of that `Rc` for the *entire* recursive
    /// evaluation of `body` (needed since the body's form list is too large
    /// to clone per call) — if `compiled` needed the same outer `RefCell`,
    /// an inner `(compile name)` call reached from within that body (e.g. a
    /// self-referential or mutually-recursive JIT request) that tried to
    /// populate this very same `FnDef`'s `compiled` slot would panic against
    /// `apply`'s already-live borrow. Scoping the `RefCell` down to just this
    /// field keeps that borrow-and-mutate pair independent, exactly like the
    /// old design's two separate top-level fields (`fns`/`compiled`) did.
    pub compiled: RefCell<Option<Rc<dyn CompiledBody>>>,
}

/// What the interpreter needs from a compiled function body: where it is.
///
/// A trait rather than the backend's own `crate::compile::CompiledFn`, so
/// nothing here names a type that owns an LLVM `ExecutionEngine`. That struct
/// is two fields — the engine, held only to keep the code alive, and this
/// address — and every use of it from the evaluator is `.address()`. Naming
/// only the half the evaluator actually reads is what lets the front end stop
/// depending on the backend.
///
/// The implementor's `Drop` is what retires the engine, so holding
/// `Rc<dyn CompiledBody>` here keeps the machine code alive exactly as
/// holding the concrete type did.
pub trait CompiledBody {
    /// The entry address of this body, under the shared compiled-function ABI
    /// (`crate::compile::CompiledSignature`).
    fn address(&self) -> usize;

    /// Calls it with `args` already in the compiled representation.
    ///
    /// Provided, not required: the ABI is one signature and the address is the
    /// only thing that varies, so an implementor supplies the address and
    /// nothing else. `extern "C-unwind"`, not `extern "C"`, because a
    /// `(panic ...)` in the body unwinds out through here — see
    /// `crate::compile::CompiledSignature`.
    fn call(&self, args: &[i64]) -> i64 {
        // SAFETY: `address` comes from a symbol the backend resolved out of a
        // module that defines it, so it points at a function built under the
        // shared ABI. `CompiledFn::new`/`new_multi` are the only producers.
        let f: unsafe extern "C-unwind" fn(*const i64, u32) -> i64 =
            unsafe { std::mem::transmute::<usize, unsafe extern "C-unwind" fn(*const i64, u32) -> i64>(self.address()) };
        unsafe { f(args.as_ptr(), args.len() as u32) }
    }
}

/// A user `defenum`'s variants (each one's declared field types), as
/// `TopLevel::Defenum` baked them in at check time — the entry
/// [`Interp::enum_defs`] keeps per enum type; see that field's doc comment for
/// why the interpreter needs this at all (compiled-global box decoding) and
/// why `Option`/`Result` are not represented this way.
///
/// The type *parameters* used to sit here too, read only by
/// `enum_fields_representable` to substitute `args` into the field types
/// before asking whether each was storable. Deleting `RtValue::Data` made that
/// question moot — every enum value is a heap box — and the parameters went
/// unread with it.
#[derive(Clone)]
pub struct EnumDef {
    /// Each variant's name and its fields' representations, in declaration
    /// order — everything the two consumers need: the printer wants the name
    /// (`eval::format`), and the compiled-global boundary wants each field's
    /// representation to decode a box back into an enum value. The core
    /// `(defenum PATH (SYM...) ((REPR...)...))` form carries exactly this.
    pub(crate) variants: Vec<(String, Vec<Repr>)>,
}

/// The interpreter state: a runtime mirror of the checker's module tree
/// ([`scope::ModuleScope`], rooted here), holding every free function/
/// macro/method/global/struct/enum by its own defining module and
/// unqualified name — never by a single flat [`Path`]-keyed table spanning
/// the whole program. See `eval::scope`'s module doc comment for why, and
/// for how a reference re-resolves against this tree instead of trusting a
/// checker-baked [`Path`] as a lookup key.
pub struct Interp {
    /// The runtime module tree. Behind a `RefCell` so the `eval` builtin
    /// (`Self::eval_form`, reached through the `&self` evaluation path) can
    /// register a definition it just evaluated — `(eval '(defun ...))` — into
    /// the same tree that ordinary top-level `exec` writes to, giving
    /// CL-faithful "definitions become visible immediately" semantics without
    /// a `&mut self` the deep `&self` eval chain cannot produce. Every read
    /// (`resolve_fn`/`get_fn`/`find_type`/...) borrows shared; the handful of
    /// registration sites in [`Self::exec`] borrow mutably.
    pub root: RefCell<scope::ModuleScope>,
    /// A shared handle to the live `Checker`, set by [`Self::set_checker`] on
    /// the drivers that support runtime `eval` (the CLI's `run_file`/`repl`/
    /// `compile_module` and the LSP). `None` in throwaway/AOT/bootstrap/test
    /// contexts, where `eval` returns an error rather than type-checking at
    /// runtime. The `Rc<RefCell<..>>` is the same cell the driver checks
    /// top-level forms through; the borrow discipline (never hold a checker
    /// borrow across an `exec` that could `eval`) keeps the two from
    /// double-borrowing — see `Self::eval_form` and the drivers' own comments.
    pub(crate) checker: Option<Rc<RefCell<crate::check::Checker>>>,
    /// Every `defvar`/`defconstant` global some compiled function has
    /// referenced, promoted to a compiled-global slot (a permanent GC root
    /// — `typelisp_rt::global_new`) and mapped to the id that slot got.
    /// Populated lazily by [`Self::add_compiled_function`], the single
    /// choke point both the JIT (`Self::compile_function`) and AOT
    /// (`crate::compile::aot::compile_file`) paths compile a `defun`
    /// through — so this table, and the promotion it drives, is shared by
    /// both with no separate AOT-specific logic. A global *never*
    /// referenced from compiled code has no entry here and keeps using its
    /// ordinary [`Slot`] in [`Self::globals`] untouched — see that
    /// method's doc comment for why only a promoted global's storage moves.
    pub compiled_globals: RefCell<HashMap<Path, usize>>,
    /// The values currently being rendered by their own `print-object`
    /// method, innermost last — [`Self::print_object`]'s re-entry guard.
    pub(crate) printing: RefCell<Vec<Value>>,
    /// The dumps whose units this environment was built from, in the order
    /// they were applied — the bytes `(dump ...)` re-emits ahead of the
    /// session's own unit, so that what it writes is self-contained.
    ///
    /// `Cow` rather than `Vec<u8>`: the prelude's and island's are
    /// `include_bytes!` statics (4MB between them, borrowed for free), while a
    /// `typl --image` load owns what it read.
    dump_sources: RefCell<Vec<std::borrow::Cow<'static, [u8]>>>,
    /// What this session has defined since [`Self::start_recording`], and the
    /// checker signature it started from — the two halves `(dump ...)` needs
    /// to write the session's own unit.
    ///
    /// `None` until a driver turns it on, which is what keeps the definitions
    /// arriving from the loaded units themselves out of it.
    recording: RefCell<Option<Recording>>,
    /// Trait-object vtables, interpreter tier: `vtable_id` -> the call
    /// targets for the trait's methods, in slot order (`dyn-new`'s
    /// `slots`, which the checker laid out from `TraitDef::method_order`).
    /// Entries are `(owning type path, method name)` — resolved through the
    /// scope tree at call time, so a method that gets `compile`d later is
    /// picked up automatically, exactly as for a static `assoc`.
    ///
    /// The compiled tier keeps a parallel table of raw function pointers in
    /// `typelisp_rt`, indexed by the same ids (which [`Self::vtable_id_for`]
    /// is the sole source of). Neither table holds heap values, so a vtable
    /// needs no GC root — `BoxedObj::Dyn`'s own `value` is all the collector
    /// ever has to trace.
    pub(crate) vtables: RefCell<Vec<Vec<(Path, String)>>>,
    /// `(concrete type key, trait path)` -> `vtable_id`, so the same pair
    /// interns to one id however many times it is boxed. The key's first
    /// half is `mangle_type`'s rendering (the `dyn-new` node's own concrete
    /// key), since `Type` has no `Hash`.
    pub vtable_ids: RefCell<HashMap<(String, Path), u32>>,
    /// `(vtable id, trait id)` -> the vtable id the *same concrete type* uses
    /// for that trait: the supertrait upcast table `dyn-upcast` reads.
    ///
    /// Keyed by the runtime vtable id because that is all an upcast site has
    /// — the concrete type, the other half of a vtable's identity, is gone by
    /// then. Filled in at every boxing site from `dyn-new`'s `SUPERS`
    /// ([`Self::register_dyn_box`]), which is where both halves are still in
    /// hand. The compiled tier keeps the same map under the same ids in
    /// `typelisp_rt` (`upcast_define`).
    pub(crate) dyn_upcasts: RefCell<HashMap<(u32, u32), u32>>,
    /// Trait path -> a small integer, so an upcast site in *compiled* code
    /// can name its target trait with a baked-in constant the way a boxing
    /// site names its vtable ([`Self::vtable_id_for`]). Interpreted upcasts
    /// go through the same ids, so both tiers agree by construction.
    pub trait_ids: RefCell<HashMap<Path, u32>>,
    /// Traits some *compiled* body dispatches on (an `dyn-call` in a
    /// function that has been through `translate_and_compile`).
    ///
    /// A compiled `:dyn` call site reads a raw function pointer out of the
    /// vtable, so every implementation it could reach has to be compiled
    /// too. `dyn-call`'s `IMPL_TARGETS` covers the ones that existed when
    /// the call site was checked; this set covers the rest, by telling
    /// `dyn-new`'s evaluation that boxing for *this* trait now has a
    /// native consumer, so a slot method still lacking a compiled form must
    /// be compiled before the box escapes. Empty for any program that never
    /// compiles a dynamic dispatch — which is why boxing does not simply
    /// compile its slots unconditionally.
    pub dyn_dispatch_compiled: RefCell<HashSet<Path>>,
}

/// One outgoing edge of the top-level compile call graph
/// [`Interp::compute_sccs`] walks: either a plain `call` target (a
/// `defun`) or an `assoc` target (a `defmethod`, keyed the same way
/// [`Interp::method_key`] keys the scope tree's `methods`/`compiled`).
/// Carries the full typed key (not just its string name) so
/// [`Interp::compile_scc`] can reuse it directly to declare/wire the target
/// without re-deriving it from the name.
pub enum CallEdge {
    Fn(Path),
    Method(Path, String),
}

/// Why [`Interp::precheck_compilable`] says no.
///
/// The split is between a cause worth *recording* and one worth *fixing*: a
/// missing target names a specific gap in what the compile path lowers, stays
/// stable while that gap does, and is the same for every definition that
/// reaches it; anything else is a one-off and gets no such treatment.
#[derive(Debug)]
pub enum Uncompilable {
    /// A call target with no compilable body — a Rust builtin free function or
    /// builtin method the compile path has no lowering for. Carries that
    /// target's name, not the caller's.
    MissingTarget(String),
    /// Anything else the compile path rejected.
    Other(EvalError),
}

impl CallEdge {
    /// The node identity [`Interp::compute_sccs`]'s graph traversal keys on
    /// — a bare `defun` name, or `"type-path::method"` (the type's full
    /// `::`-joined path, never just its local segment — see
    /// [`Interp::method_key`]'s doc comment for why) for a `defmethod`,
    /// exactly the string shape [`Interp::compiled_fn_body`]/
    /// [`Interp::method_key`] already resolve back.
    pub fn node_name(&self) -> String {
        match self {
            CallEdge::Fn(p) => p.to_string(),
            CallEdge::Method(p, m) => method_link_name(p, m),
        }
    }
}

/// Parse a [`CallEdge::Fn`] node name back to its `Path` — `"m::inc"` ->
/// `Path[m, inc]`, `"inc"` -> `Path::root("inc")`. Only ever applied to a name
/// [`Interp::method_key`] has already ruled out as a `type::method`, so a
/// `"::"` here is unambiguously a module separator.
pub fn fn_path_from_node_name(name: &str) -> Path {
    if name.contains("::") {
        Path::from_segments(name.split("::").map(|s| s.to_string()).collect())
    } else {
        Path::root(name)
    }
}

impl Interp {
    pub fn new() -> Interp {
        // A fresh `Interp` always pairs with a fresh `Heap` (every caller in
        // this codebase constructs them together) — clear the global-id
        // table typelisp-rt keeps for compiled global-variable access
        // (`compiled_globals`'s eventual runtime counterpart) so a stale
        // entry from an earlier `Interp`/`Heap` pair that happened to share
        // this OS thread (`cargo test`'s worker pool) can't resolve to a
        // permanent-root position in a `Heap` that no longer exists. See
        // `typelisp_rt::reset_global_table`'s doc comment.
        typelisp_rt::reset_global_table();
        // Same reasoning for the compiled tier's vtable table: ids are
        // per-`Interp`, so a stale entry from an earlier pair on this thread
        // must not survive into this one.
        typelisp_rt::reset_vtable_table();
        // `vector` is registered directly in `Registry::with_builtins`
        // (`registry::vector_def`) with `AdtKind::Struct`, so its
        // `construct` sites already get `mutable = true`
        // (`Checker::check_construct`) — but it never executes a
        // `TopLevel::Defstruct`, the only other place a struct `TypeEntry`
        // gets populated, so it's seeded here by hand, directly at the root
        // node (it has no defining module of its own). `hashtable`/`scope`
        // don't need this: both are `AdtKind::Sum` (`registry::hashtable_def`/
        // `scope_def`) with no `variants`/`field_names` of their own — user
        // code only ever builds one through its `::new()` assoc fn, never
        // `construct`, so there is no construct site to affect.
        let mut root = scope::ModuleScope::default();
        // No field representations: `Vector<T>`'s storage is internal
        // (`rt_struct_*`), it has no constructor-pattern shape, and nothing
        // ever names one of its fields — so there is nothing for a `construct`
        // or a pattern to read here.
        root.types.insert("vector".to_string(), scope::TypeEntry::Struct(Vec::new()));
        // `Option`/`Result`/the four concrete error types are `AdtKind::Sum`
        // registrations in `Registry::with_builtins`
        // (`registry::builtin_sum_defs`, the single source both sides read)
        // but — like `vector` above — never execute a `TopLevel::Defenum`, so
        // nothing would otherwise put their variant names in this tree. That
        // used to mean a printed `Option` came out as `(<unknown-variant> 1)`
        // (`Self::render_ctx`'s `enums` table had no entry for it); seeding
        // through the same `register_enum` a real `defenum` uses closes that
        // gap by construction rather than papering over it at the print site.
        for def in crate::check::registry::builtin_sum_defs() {
            let name = def.name;
            // The field types are the builtins' own declarations, so their
            // representations come from the same classification a `defenum`'s
            // do — `Repr::of_by` with no ADT lookup, since none of these
            // mention a user type (`Option<T>`/`Result<T,E>`'s fields are type
            // variables, an error type's are scalars).
            let variants = def
                .variants
                .into_iter()
                .map(|v| {
                    let reprs = v.fields.iter().map(|t| Repr::of_by(t, &|_| None)).collect();
                    (v.name, reprs)
                })
                .collect();
            root.register_enum(&name, EnumDef { variants });
        }
        Interp {
            root: RefCell::new(root),
            checker: None,
            compiled_globals: RefCell::new(HashMap::new()),
            vtables: RefCell::new(Vec::new()),
            vtable_ids: RefCell::new(HashMap::new()),
            dyn_upcasts: RefCell::new(HashMap::new()),
            trait_ids: RefCell::new(HashMap::new()),
            dyn_dispatch_compiled: RefCell::new(HashSet::new()),
            printing: RefCell::new(Vec::new()),
            dump_sources: RefCell::new(Vec::new()),
            recording: RefCell::new(None),
        }
    }

    /// A shared handle to the live `Checker`, for the paths that need to check
    /// or capture against this environment (`(dump ...)`). `None` where no
    /// driver wired one — see the `checker` field.
    pub fn checker_handle(&self) -> Option<Rc<RefCell<crate::check::Checker>>> {
        self.checker.clone()
    }

    /// Records that this environment came from `bytes` — a dump whose units
    /// have just been applied.
    pub fn push_dump_source(&self, bytes: std::borrow::Cow<'static, [u8]>) {
        self.dump_sources.borrow_mut().push(bytes);
    }

    /// Runs `f` over the dumps this environment was built from.
    pub fn with_dump_sources<R>(&self, f: impl FnOnce(&[std::borrow::Cow<'static, [u8]>]) -> R) -> R {
        f(&self.dump_sources.borrow())
    }

    /// Starts recording what this session defines, from a checker signature
    /// taken now.
    ///
    /// Called by a driver once its environment is fully loaded and before it
    /// runs a line of user code: everything after this point is the session's,
    /// and everything before it belongs to a unit that is already written down.
    pub fn start_recording(&self, baseline: crate::dump::RegistrySignature) {
        *self.recording.borrow_mut() =
            Some(Recording { baseline, forms: Vec::new(), globals_before: self.compiled_globals.borrow().len() });
    }

    /// Whether [`Self::start_recording`] has been called.
    pub fn is_recording(&self) -> bool {
        self.recording.borrow().is_some()
    }

    /// Runs `f` over this session's recording, or fails if there is none.
    pub fn with_recording<R>(
        &self,
        f: impl FnOnce(&crate::dump::RegistrySignature, &[Value], usize) -> R,
    ) -> Option<R> {
        let r = self.recording.borrow();
        let r = r.as_ref()?;
        Some(f(&r.baseline, &r.forms, r.globals_before))
    }

    /// Adds `tl`'s definitions to the recording, if there is one.
    ///
    /// Called by [`Self::exec`] after a successful top-level form. Each
    /// recorded node is rooted for the rest of the session: `exec` permanently
    /// roots a definition's *body*, not the top-level node, and `(dump ...)`
    /// reads these nodes long afterwards.
    pub(crate) fn note_definitions(&self, heap: &mut Heap, tl: Value) {
        let mut recording = self.recording.borrow_mut();
        let Some(recording) = recording.as_mut() else { return };
        let mut found = Vec::new();
        crate::dump::record_definitions(heap, tl, &mut found);
        for v in found {
            heap.push_root(v);
            recording.forms.push(v);
        }
    }

    /// Wire this interpreter to the live `Checker` so a runtime `eval` can
    /// type-check the form it is handed against the program's current global
    /// environment. Called by the `eval`-supporting drivers (the CLI and LSP)
    /// right after construction; contexts that never run user `eval` (AOT
    /// compilation, the compiler-island bootstrap, most tests) skip it and
    /// leave the handle `None`. See the `checker` field's doc comment.
    pub fn set_checker(&mut self, checker: Rc<RefCell<crate::check::Checker>>) {
        self.checker = Some(checker);
    }

    /// Wrap `v` in a fresh GC heap cell. Every binding site (`let`,
    /// parameters, `match` bindings, `labels` placeholders, globals,
    /// `eval_args`'s anchors) goes through here, and they all get the same
    /// thing — there is no longer a kind to choose.
    ///
    /// That uniformity is the point, and it is a correctness property rather
    /// than a simplification. A slot used to come in three kinds, routed by a
    /// declared type through `is_heap_repr_ty` and its checker-side twin
    /// `Checker::is_heap_repr`, because the interpreter had a second value
    /// world (`RtValue`) whose variants were not heap values. But a heap cell
    /// is rooted by the collector's own `cell_registry` walk on *every*
    /// collection, whereas a native slot was only rooted when `sync_roots`
    /// next ran — so any type whose values were collectible heap values *had*
    /// to be classified heap-repr, and misclassifying one meant the value
    /// silently vanished mid-collection. Five types were found that way, each
    /// after it went missing under `gc_stress`: `random-state`, `bignum`,
    /// `ratio`, `f64`, and `string` (the last as an outright dangling
    /// `StrId`; see `tests/random_state_time_test.rs`,
    /// `tests/bignum_ratio_gc_test.rs`, `tests/float_gc_test.rs`,
    /// `tests/string_gc_test.rs`). With one value world every value is a heap
    /// value, so the requirement is satisfied by construction and the two
    /// predicates that had to agree with each other are gone.
    ///
    /// (`Heap::alloc_cell` cannot trigger a collection — only `Heap::cons`
    /// can — so `v`'s payload needs no rooting across this call.)
    fn slot(&self, heap: &mut Heap, v: Value) -> Slot {
        Slot::new(heap.alloc_cell(v))
    }


    /// Publish every interned vtable to the compiled tier, resolving each
    /// slot to its method's current native entry point. Called after each
    /// SCC finishes so a table whose targets have just been compiled becomes
    /// callable; re-publishing tables that were already complete is
    /// harmless, and is what keeps a vtable correct when one of its methods
    /// is compiled *later* than the function that boxes for it.
    ///
    /// A slot whose method has no compiled form yet is published as 0, which
    /// `rt_dyn_call` reads as "this one is interpreted — ask the interpreter
    /// for its closure". That is not an error: a trait object's concrete type
    /// can be a user `defstruct` whose methods nobody ever compiled, while the
    /// code dispatching on it (a prelude stream method, say) is native.
    pub fn publish_vtables(&self) {
        for id in 0..self.vtables.borrow().len() {
            self.publish_vtable(id as u32);
        }
    }

    /// [`Self::publish_vtables`] for one table.
    fn publish_vtable(&self, id: u32) {
        let slots = match self.vtables.borrow().get(id as usize) {
            Some(s) => s.clone(),
            None => return,
        };
        let addrs: Vec<usize> = slots
            .iter()
            .map(|(type_name, method)| {
                self.root
                    .borrow()
                    .get_method(type_name, method)
                    .and_then(|f| f.compiled.borrow().as_ref().map(|c| c.address()))
                    .unwrap_or(0)
            })
            .collect();
        typelisp_rt::vtable_define(id, addrs);
    }

    /// Every vtable interned so far, as `(id, slots)` — for a code generator
    /// that has to emit the table itself rather than patch it in from JIT
    /// addresses (`compile::aot::build_main_wrapper`).
    /// Every enum in the program as `(type key, variant index, variant
    /// name)` — what an AOT executable's startup registers so its printer can
    /// name a variant (`typelisp_print::aot`), since the box carries only the
    /// index.
    ///
    /// Keyed by `type_key::type_key_of`, not by the `Path`'s `Display`: the
    /// key is what the box actually stores, and the two are only the same for
    /// a root-module type (see `src/type_key.rs`'s module doc comment).
    pub fn enum_variant_descriptors(&self) -> Vec<(String, usize, String)> {
        let (_, enums) = self.root.borrow().collect_struct_and_enum_types();
        let mut out = Vec::new();
        for (path, def) in &enums {
            let key = crate::type_key::type_key_of(path);
            for (i, (name, _)) in def.variants.iter().enumerate() {
                out.push((key.clone(), i, name.clone()));
            }
        }
        // `collect_struct_and_enum_types` returns a `HashMap`, so sort for a
        // deterministic startup sequence — two runs of `compile-file` on the
        // same source must produce the same executable.
        out.sort();
        out
    }

    pub fn vtable_descriptors(&self) -> Vec<(u32, Vec<(Path, String)>)> {
        self.vtables.borrow().iter().cloned().enumerate().map(|(i, s)| (i as u32, s)).collect()
    }

    /// The vtable id for boxing a `concrete_key`-typed value as
    /// `trait_path` — interning the (type, trait) pair so repeated boxing
    /// reuses one table. The sole source of vtable ids; the compiled tier's
    /// parallel table (`typelisp_rt`) is keyed by the same numbers.
    ///
    /// `slots` comes from the checker (`dyn-new`), already in
    /// `TraitDef::method_order` order and already monomorphized, so this only
    /// records it.
    pub(crate) fn vtable_id_for(&self, concrete_key: &str, trait_path: &Path, slots: &[(Path, String)]) -> u32 {
        let key = (concrete_key.to_string(), trait_path.clone());
        if let Some(&id) = self.vtable_ids.borrow().get(&key) {
            return id;
        }
        let id = {
            let mut tables = self.vtables.borrow_mut();
            let id = tables.len() as u32;
            tables.push(slots.to_vec());
            id
        };
        self.vtable_ids.borrow_mut().insert(key, id);
        // Publish immediately, not only after the next `compile_scc`: with a
        // precompiled prelude the targets may already have native bodies
        // while nothing in this session ever compiles anything, and then the
        // compiled tier would never learn the table exists. A slot whose
        // method is *not* compiled publishes as 0, which `rt_dyn_call` reads
        // as "ask the interpreter".
        self.publish_vtable(id);
        id
    }

    /// The id interning `trait_path` for upcast lookups — the trait half of
    /// [`Self::dyn_upcasts`]'s key, and the constant a compiled
    /// `dyn-upcast` bakes in. Ids are dense and per-`Interp`, like
    /// vtable ids; nothing outside this pair of tables reads them.
    pub fn trait_id_for(&self, trait_path: &Path) -> u32 {
        if let Some(&id) = self.trait_ids.borrow().get(trait_path) {
            return id;
        }
        let mut ids = self.trait_ids.borrow_mut();
        let id = ids.len() as u32;
        ids.insert(trait_path.clone(), id);
        id
    }

    /// Intern a boxing site's vtable *and* every supertrait table it carries
    /// (`dyn-new`'s `SUPERS`), recording how to get from any one of them to
    /// any other. Returns the ids, the boxed trait's own first.
    ///
    /// Every pair is registered, not just (sub -> super): the entries only
    /// ever say "the vtable *this same concrete type* uses for that trait",
    /// which is true regardless of the direction the two traits are related
    /// in, and a chain of upcasts can arrive at any of these tables before
    /// asking for the next. Admissibility is the checker's business — an
    /// entry nothing is allowed to ask for is simply never read.
    pub fn register_dyn_box(
        &self,
        concrete_key: &str,
        trait_path: &Path,
        slots: &[(Path, String)],
        supers: &[(Path, Vec<(Path, String)>)],
    ) -> Vec<u32> {
        let id = self.vtable_id_for(concrete_key, trait_path, slots);
        if supers.is_empty() {
            return vec![id];
        }
        let mut tables: Vec<(u32, u32)> = vec![(id, self.trait_id_for(trait_path))];
        for (sup, sup_slots) in supers {
            tables.push((self.vtable_id_for(concrete_key, sup, sup_slots), self.trait_id_for(sup)));
        }
        for &(from, _) in &tables {
            for &(to, to_trait) in &tables {
                self.dyn_upcasts.borrow_mut().insert((from, to_trait), to);
                typelisp_rt::upcast_define(from, to_trait, to);
            }
        }
        tables.into_iter().map(|(id, _)| id).collect()
    }

    /// Every upcast registered so far, as `(source vtable, trait id, target
    /// vtable)` — [`Self::vtable_descriptors`]'s counterpart for the code
    /// generator that has to emit the table itself instead of filling it in
    /// from a running interpreter (`compile::aot::build_main_wrapper`).
    /// Sorted so a rebuild of the same program emits the same startup code.
    pub fn upcast_descriptors(&self) -> Vec<(u32, u32, u32)> {
        let mut out: Vec<(u32, u32, u32)> =
            self.dyn_upcasts.borrow().iter().map(|(&(from, t), &to)| (from, t, to)).collect();
        out.sort_unstable();
        out
    }








    /// Execute a checked top-level form. Definitions register and return `None`;
    /// a bare expression returns `Some(value)`.
    /// Every currently-registered top-level function's *local* name, in no
    /// particular order — for tooling and tests that need to enumerate what
    /// a given load produced (e.g. deriving the compiler island's own
    /// `defun` set by diffing a prelude-only interpreter against one that
    /// also ran `load_compiler`; interp-closure removal Stage 2 onward).
    pub fn function_names(&self) -> Vec<String> {
        self.root.borrow().all_fn_names()
    }




    /// Resolves a `Call`/`FnRef` [`Ref`]: tries the independent
    /// re-derivation first ([`scope::ModuleScope::resolve_fn`]), falling
    /// back to direct descent via `r.resolved` only when that search comes
    /// up empty. The one legitimate reason that happens: `r.written` named a
    /// `use` alias (a bare-name import, or a module alias like `use
    /// geo::point` making `point::f` resolve to `geo::point::f`) —
    /// `Checker::lookup_alias`/`find_module`'s alias branches are check-time
    /// -only machinery (the alias table itself is never mirrored into this
    /// tree, see `eval::scope`'s module doc comment), so the literal
    /// written text has no ancestor-chain-searchable meaning at runtime.
    /// Falling back to `resolved` — a well-typed program's checker-verified
    /// answer — is still a direct tree descent, not a flat-table trust
    /// fallback: only the *search strategy* differs from the ordinary case,
    /// not the mechanism.
    pub fn resolve_fn_ref(&self, r: &Ref) -> Option<Rc<FnDef>> {
        self.root.borrow().resolve_fn(&r.home, &r.written).or_else(|| self.root.borrow().get_fn(&r.resolved))
    }

    /// Runs already-evaluated `argv` (`param_tys`-typed, `ret_ty`-returning)
    /// through `compiled` instead of tree-walking — shared by `call`
    /// and `assoc`, the two places eval can reach a `(compile
    /// "name")`d body from. Every parameter/return type is either `Sexpr`
    /// (`compile::runtime::encode`/`decode`, Stage 5 of the
    /// Sexpr-representation plan) or assumed already representable as a
    /// plain `i64` at the compiled ABI level — true for an `i64`/`i32`
    /// itself, and for a general-ADT pointer (`Option`/`defstruct`, Stage 6)
    /// that *already* crossed this same boundary once (so it's sitting in
    /// `argv` as a `Value::Int`, not decoded into a boxed struct/enum/etc.
    /// — general-ADT bridging at this boundary isn't implemented, a
    /// pre-existing, separate gap). Calling a compiled method on a receiver
    /// built by *pure* interpretation (a real boxed struct,
    /// never touched by compiled code) hits the same wall an analogous top-level
    /// `call` already would for a general-ADT parameter — a clear
    /// internal error here, not a silent misread of unrelated bits.
    pub fn call_compiled(
        &self,
        heap: &mut Heap,
        compiled: &dyn CompiledBody,
        argv: &[Value],
        param_reprs: &[Repr],
        ret: &Repr,
    ) -> Result<Value, EvalError> {
        // Recorded before anything is pushed, so the unwind path below can
        // discard the crossing roots and whatever compiled code pushed on top
        // of them in one shot — an unwinding `(panic ...)` runs none of the
        // pops that normally balance either.
        let roots_on_entry = heap.root_count();
        // A `defun`/`defmethod`'s `sig` lists only its fixed parameters; a
        // `&rest` one that reached compilation would land in the "no declared
        // representation" error above rather than be guessed at.
        let (int_args, crossing_roots) = self.encode_crossing_args(heap, argv, param_reprs, false)?;
        self.enter_compiled(heap);
        let raw = match crate::eval::crossing::catch_compiled_panic(|| compiled.call(&int_args)) {
            Ok(raw) => raw,
            Err(e) => {
                heap.truncate_roots(roots_on_entry);
                return Err(e);
            }
        };
        for _ in 0..crossing_roots {
            heap.pop_root();
        }
        self.decode_compiled_return(heap, raw, ret)
    }

    /// [`Self::call_compiled`]'s argument-marshaling half, factored out so
    /// [`Self::eval`]'s `apply` arm can reuse it when the callee is a
    /// `BoxedObj::CompiledClosure` rather than a top-level `(compile ...)`d
    /// function — both cross the exact same interpreter -> compiled-ABI
    /// `i64` boundary. Returns the encoded `int_args` plus how many roots it
    /// pushed (over `argv`'s heap-backed elements only); the caller must pop
    /// exactly that many once the call this feeds into has returned. See
    /// `call_compiled`'s (pre-refactor) doc comment for why encoding reads
    /// `argv`'s own runtime shape rather than each parameter's static type,
    /// and why every heap-backed argument is rooted for the marshaling+call
    /// window.
    /// `param_reprs` are the callee's *declared* parameter representations, and
    /// they are required rather than advisory: since the scalar unification an
    /// `f64` and a `Sexpr` holding a float are the same runtime shape
    /// (`Value::Boxed(float)`), and they cross the boundary differently — an
    /// `f64` parameter takes raw `f64::to_bits`, a `Sexpr` parameter takes the
    /// tagged box pointer. The value cannot say which; only the declaration
    /// can. Reading it off the value instead made `(which (Float 1.5))` miss
    /// its `(float _)` arm in compiled code while the interpreted call still
    /// matched.
    ///
    /// A `Repr` rather than a `Type`, since Stage C: the type is gone from the
    /// IR by the time a definition is registered, and every distinction this
    /// function draws is one a `Repr` already makes. That is what a `Repr` is
    /// — the residue of the type system the boundary needs.
    fn encode_crossing_args(
        &self,
        heap: &mut Heap,
        argv: &[Value],
        param_reprs: &[Repr],
        rest: bool,
    ) -> Result<(Vec<i64>, usize), EvalError> {
        let mut crossing_roots = 0usize;
        let mut int_args: Vec<i64> = Vec::with_capacity(argv.len());
        // Root every argument up front, before the encode loop below
        // allocates anything. (Rooting a scalar is a no-op for the collector,
        // so the pass does not bother telling them apart.) A later argument's
        // encoding can allocate — a `Str`/`Bignum`/`Ratio` argument copies
        // itself onto the GC heap — and that allocation can trigger a GC;
        // without this pre-pass, an *earlier-in-`argv`* alloc (e.g. a
        // leading `Str` argument) would collect a not-yet-rooted `Sexpr`
        // argument sitting later in `argv`, whose freed string/cons slots
        // then get recycled under it (observed compiling the self-hosted
        // island: a `defun`'s `name` string argument's `alloc_string`
        // reclaimed its own body AST, so a `(var "x")` node read back as the
        // `name`). Rooting order doesn't matter for protection — only that
        // every heap arg is rooted before the first allocation — so this
        // separate pass is the whole fix.
        for v in argv {
            heap.push_root(*v);
            crossing_roots += 1;
        }
        for (i, v) in argv.iter().enumerate() {
            // Past the fixed parameters there is exactly one more argument:
            // the `&rest` list. The checker packs every surplus argument into
            // that single `Sexpr` at *check* time
            // (`wrap_rest_elem`/`cons_rest_list`), so what crosses here is the
            // whole list, and it crosses tagged like any other `Sexpr`.
            //
            // `Repr::Sexpr`, not the `&rest` *element*'s representation:
            // using the element here asked a list to be an `i32` ("expected
            // an integer argument, got Cons"). The two are only
            // distinguishable by the declaration, since a packed list and an
            // ordinary `Sexpr` argument are the same kind of value.
            let param_repr = match param_reprs.get(i) {
                Some(r) => Some(r),
                None if rest => Some(&Repr::Sexpr),
                None => None,
            };
            let encoded = match param_repr {
                Some(r) => self.encode_crossing_value(heap, v, r),
                None => Err(EvalError::Internal(format!(
                    "compiled call: no declared representation for argument {} — cannot tell a raw scalar from a tagged heap word",
                    i
                ))),
            };
            match encoded {
                Ok(n) => int_args.push(n),
                Err(e) => {
                    for _ in 0..crossing_roots {
                        heap.pop_root();
                    }
                    return Err(e);
                }
            }
        }
        Ok((int_args, crossing_roots))
    }

    /// One value's worth of [`Self::encode_crossing_args`] — the exact
    /// inverse of [`Self::decode_compiled_return`], and shared with it by
    /// [`Self::apply_interpreted`], which crosses the same boundary in the
    /// other direction (compiled code's argument words in, the interpreted
    /// result's word out).
    ///
    /// The dispatch is on the *declared* representation, never on the value's
    /// shape. It used to read the value: an `i32` argument was `RtValue::Int`
    /// and crossed as a raw machine word, while a `Sexpr` argument holding an
    /// integer was `RtValue::Sexpr(Value::Int)` and crossed as a tagged word,
    /// and the two variants told them apart. With one value universe both are
    /// `Value::Int(n)` and the value says nothing — exactly the ambiguity
    /// that made `(which (Float 1.5))` miss its `(float _)` arm when `f64`
    /// unified (`compile_match_distinguishes_float_bignum_and_ratio_boxes`).
    /// A caller with no declared representation to consult must raise an
    /// error rather than guess: either encoding would corrupt the call
    /// silently.
    ///
    /// Encoding allocates nothing, so no rooting happens (or is needed) here.
    fn encode_crossing_value(&self, heap: &Heap, v: &Value, repr: &Repr) -> Result<i64, EvalError> {
        // A built-in used as a function value is a box like any other, so it
        // would encode as an ordinary tagged word — and then `rt_apply_any`
        // would find a callee whose arguments it has no way to decode: a
        // built-in carries only its name, and the apply site does not carry
        // the representations at runtime (an interpreted closure does, which
        // is why *it* crosses fine). Rejected here, where it is still an
        // ordinary catchable error rather than an abort from inside a
        // compiled frame.
        if let Value::Boxed(id) = v {
            if heap.is_builtin_fn(*id) {
                return Err(EvalError::Internal(format!(
                    "compiled call: the built-in \"{}\" cannot be passed as a function value to compiled code",
                    heap.builtin_fn_name(*id)
                )));
            }
        }
        match repr {
            // Raw machine words, by the encodings compiled code uses
            // internally.
            Repr::Int => match v {
                Value::Int(n) => Ok(*n),
                other => Err(EvalError::Internal(format!("compiled call: expected an integer argument, got {:?}", other))),
            },
            Repr::Bool => match v {
                Value::Bool(b) => Ok(i64::from(*b)),
                other => Err(EvalError::Internal(format!("compiled call: expected a bool argument, got {:?}", other))),
            },
            Repr::Char => match v {
                Value::Char(c) => Ok(*c as i64),
                other => Err(EvalError::Internal(format!("compiled call: expected a char argument, got {:?}", other))),
            },
            // `()` crosses as the plain `0` `compile-unit` compiles a
            // `Unit`-typed body tail to. A unit value carries no information,
            // so the word is a placeholder the callee never reads; it just has
            // to be the one both sides agree on.
            Repr::Unit => Ok(0),
            // An `f64` is its raw `f64::to_bits` pattern in an `i64`
            // (`compile-float`/`llvm_builder_build_float_op`), the inverse of
            // `decode_compiled_return`'s `Repr::Float` arm.
            Repr::Float => match v {
                Value::Boxed(id) if heap.is_float(*id) => Ok(heap.float_value(*id).to_bits() as i64),
                other => Err(EvalError::Internal(format!("compiled call: expected an f64 argument, got {:?}", other))),
            },
            // An LLVM handle crosses as the *raw* registry index — never
            // tagged. Without its own arm it would fall into the tagged
            // catch-all below and the callee would read `handle << 3` as a
            // handle: passing the island its own module as handle 1 made it
            // look up handle 8 and abort with "dangling llvm handle 8". Must
            // stay ahead of the catch-all, exactly as the matching check in
            // `decode_compiled_return` does.
            Repr::Handle => match v {
                Value::Int(h) => Ok(*h),
                other => Err(EvalError::Internal(format!("compiled call: expected an llvm handle argument, got {:?}", other))),
            },
            // Every remaining representation — `Sexpr`, `string`,
            // `bignum`/`ratio`, structs, enums, closures, trait objects, and a
            // `Scope<V>` (one heap object since Phase 1a) — is already a heap
            // `Value`, and crosses as the tagged `i64` the `rt_*` shims read.
            _ => Ok(typelisp_rt::encode(*v)),
        }
    }

    /// [`Self::call_compiled`]'s return-value half, factored out for the same
    /// reason as [`Self::encode_crossing_args`] — a direct `apply` on a
    /// `BoxedObj::CompiledClosure` decodes its raw `i64` result exactly like a
    /// top-level compiled call's, by the callee's declared return
    /// representation.
    ///
    /// A `Repr` rather than a `Type` since Stage C, which collapsed the old
    /// `is_boxed_sexpr_type` predicate into the classification itself: which
    /// `Type::Named`s cross as a tagged word (a `defstruct`, an enum, a
    /// `Scope<V>`, a trait object, a closure) was a rule spelled out at that
    /// predicate; a `Repr` already *is* that rule's answer.
    fn decode_compiled_return(&self, heap: &mut Heap, raw: i64, ret: &Repr) -> Result<Value, EvalError> {
        Ok(match ret {
            // A raw registry index, never a heap pointer. For an LLVM object
            // that is already the interpreter's own representation, so it just
            // *is* the result.
            Repr::Handle => match backend("decoding an llvm-handle return")? {
                b if (b.handle_is_live)(raw) => Value::Int(raw),
                _ => {
                    return Err(EvalError::Internal(format!(
                        "compiled call returned dangling llvm handle {}",
                        raw
                    )))
                }
            },
            // Raw machine words on the way out, mirroring the argument encode.
            Repr::Int => Value::Int(raw),
            // A `Unit`-typed body compiles to a plain `0` (`compile-unit`) —
            // decode it back to the real unit value rather than surfacing the
            // raw word as a bogus `Int(0)`, so a `Unit`-returning compiled
            // function interoperates with interpreted code exactly like an
            // interpreted one.
            Repr::Unit => Value::Empty,
            // Compiled code represents a `bool` as a raw 0/1 `i64` (LLVM
            // `icmp` results, zero-extended).
            Repr::Bool => Value::Bool(raw != 0),
            // A compiled `char` is a raw `i64` Unicode scalar value — the
            // exact inverse of the `*c as i64` a `char` argument crosses as. A
            // compiled `char` only ever holds a value that was a valid `char`
            // on the way in, so a decode failure is an internal-invariant
            // break, not a user-reachable error.
            Repr::Char => match char::from_u32(raw as u32) {
                Some(c) => Value::Char(c),
                None => {
                    return Err(EvalError::Internal(format!(
                        "compiled call returned {} for a `char` result, which is not a valid Unicode scalar value",
                        raw
                    )))
                }
            },
            // Raw `f64::to_bits` in the return register
            // (`llvm_builder_build_float_op`'s final `bitcast`).
            Repr::Float => float_rt(heap, f64::from_bits(raw as u64)),
            // A tagged word whose decode must land on the shape the
            // declaration promised — a mismatch here means the compiled side
            // and this side disagree about the ABI, which is an internal
            // error rather than anything a program can cause.
            Repr::Str => match typelisp_rt::decode(raw) {
                v @ Value::Str(_) => v,
                other => return Err(crossing_mismatch("a string", other)),
            },
            Repr::Sym => match typelisp_rt::decode(raw) {
                v @ Value::Symbol(_) => v,
                other => return Err(crossing_mismatch("a Symbol", other)),
            },
            Repr::Bignum | Repr::Ratio => match typelisp_rt::decode(raw) {
                v @ Value::Boxed(id) if heap.is_bignum(id) || heap.is_ratio(id) => v,
                other => return Err(crossing_mismatch("a bignum/ratio", other)),
            },
            Repr::RandomState => match typelisp_rt::decode(raw) {
                v @ Value::Boxed(id) if heap.is_random_state(id) => v,
                other => return Err(crossing_mismatch("a random-state", other)),
            },
            // Every remaining representation is a heap value crossing as the
            // tagged `i64` the `rt_*` shims read.
            Repr::Sexpr
            | Repr::Struct
            | Repr::Enum
            | Repr::Dyn
            | Repr::Fn
            | Repr::Scope(_)
            | Repr::Vector(_)
            | Repr::HashTable(..) => typelisp_rt::decode(raw),
            // A still-generic type variable. Nothing compiled can return one:
            // a generic body is monomorphized before it is compiled, so
            // reaching here is a bug in whatever produced the signature.
            Repr::None => {
                return Err(EvalError::Internal(
                    "compiled call has no declared return representation — a generic body was compiled unmonomorphized"
                        .to_string(),
                ))
            }
        })
    }

    /// Invokes a `BoxedObj::CompiledClosure` directly from interp Rust code
    /// — the `apply` counterpart of a top-level `(compile ...)`d
    /// function call, for a callee produced by compiled code (returned
    /// across the boundary, or built and threaded through a chain of
    /// `apply`s the interpreter is itself driving). `args` are
    /// already-encoded raw i64s ([`Self::encode_crossing_args`]'s output);
    /// the caller must already have called `compile::runtime::set_active_heap`
    /// (the closure's own body may call back into the `rt_*` runtime,
    /// directly or transitively through another compiled function).
    /// Marshals the closure's captured environment into the fixed
    /// `compiled_fn_type_with_env` ABI (`args_ptr, argc, env_ptr, env_len`)
    /// every closure-boxed compiled function shares — the exact inverse of
    /// `rt_closure_env_get`'s per-slot re-encode, done here in one pass
    /// since the whole env crosses at once rather than one slot per call.
    fn call_closure_box(heap: &Heap, id: BoxId, args: &[i64]) -> i64 {
        let env_len = heap.compiled_closure_env_len(id);
        let mask = heap.compiled_closure_mask(id);
        let env: Vec<i64> = (0..env_len)
            .map(|i| {
                let v = heap.compiled_closure_env_get(id, i);
                if mask & (1 << i) != 0 {
                    typelisp_rt::encode(v)
                } else {
                    match v {
                        Value::Int(raw) => raw,
                        other => unreachable!("compiled closure env slot {} holds a non-raw value {:?} for an unmasked slot", i, other),
                    }
                }
            })
            .collect();
        let fn_ptr = heap.compiled_closure_fnptr(id);
        // SAFETY: every `BoxedObj::CompiledClosure` in the heap was built by
        // `rt_closure_new` from a real LLVM function pointer compiled under
        // `compiled_fn_type_with_env`'s exact signature (`build-make-closure`
        // in `compiler.rs`'s `compile-lambda`/`resolve-value` is its only
        // producer) — there is no other way to construct one, so `fn_ptr`
        // always points at a function with this signature.
        // `extern "C-unwind"`, for the same reason `compile::CompiledSignature`
        // is: a `(panic ...)` in this closure's body unwinds out through here,
        // and an `extern "C"` pointer would promise Rust that it cannot.
        let f: unsafe extern "C-unwind" fn(*const i64, u32, *const i64, u32) -> i64 = unsafe { std::mem::transmute(fn_ptr) };
        unsafe { f(args.as_ptr(), args.len() as u32, env.as_ptr(), env.len() as u32) }
    }

    /// Registers this thread's `Heap` and `Interp` for the compiled code
    /// about to run, and wires the interpreter re-entry hook
    /// `typelisp_rt::rt_apply_any` calls when it finds an interpreted callee.
    ///
    /// Called immediately before *every* crossing into compiled code rather
    /// than once per session: it is three pointer stores, and there is no
    /// cheaper place to decide ahead of time whether a given callee might
    /// transitively touch the heap or apply a function value. The heap
    /// registration is what makes `rt-cons`/`rt-car`/... resolve against the
    /// right heap — see `compile::runtime::set_active_heap`'s doc comment.
    ///
    /// AOT has no counterpart: an AOT-compiled executable is its own process
    /// with no interpreter in it, so `rt_heap_init` registers the heap alone
    /// and an interpreted callee there is an invariant break (see
    /// `rt_apply_any`).
    fn enter_compiled(&self, heap: &mut Heap) {
        typelisp_rt::set_active_heap(heap as *mut Heap);
        self.install_print_hooks();
        typelisp_rt::set_apply_interpreted(Some(rt_apply_interpreted));
        typelisp_rt::set_dyn_slot_closure(Some(rt_dyn_slot_closure));
    }

    /// Registers this `Interp` as the environment the printer asks its two
    /// program questions of, and its control variables of — see
    /// [`INTERP_PRINT_HOOKS`].
    ///
    /// Called at the point of use rather than once per session, for the same
    /// reason [`Self::enter_compiled`] re-registers the heap on every
    /// crossing: it is two stores, and a nested `Interp` (`compile-file`
    /// builds one) would otherwise leave the slot naming an `Interp` that has
    /// since returned.
    ///
    /// `pub` for the one caller that is *not* at a point of use:
    /// `crate::shim::rt_eval_init` calls it on the immortal `Interp` it has
    /// just leaked. Building that environment runs this from inside, against
    /// an `Interp` still on `rt_eval_init`'s stack, and moving it into the
    /// leak leaves the slot dangling — which the rest of the process then
    /// follows, since an AOT program has no later crossing to refresh it.
    /// Re-registering the moved address is what closes that.
    pub fn install_print_hooks(&self) {
        ACTIVE_INTERP.with(|cell| cell.set(self as *const Interp));
        typelisp_print::runtime::set_print_hooks(Some(INTERP_PRINT_HOOKS));
    }

    /// The closure an unfilled vtable slot's method reifies to —
    /// [`rt_dyn_slot_closure`]'s body, and the `:dyn` counterpart of
    /// [`Self::apply_interpreted`]'s closure argument.
    ///
    /// The slot names a `(type path, method)` pair (`Self::vtables`, filled
    /// by the boxing site from the checker's own `TraitDef::method_order`),
    /// and reifying it produces exactly the closure an interpreted
    /// `(method-ref ...)` would — carrying the parameter and return
    /// representations the caller decodes by.
    fn dyn_slot_closure(&self, heap: &mut Heap, vtable: u32, slot: u32) -> Result<i64, EvalError> {
        let target = self
            .vtables
            .borrow()
            .get(vtable as usize)
            .and_then(|slots| slots.get(slot as usize).cloned())
            .ok_or_else(|| EvalError::Internal(format!("vtable {} has no slot {}", vtable, slot)))?;
        let (type_path, method) = target;
        let f = self
            .root
            .borrow()
            .get_method(&type_path, &method)
            .ok_or_else(|| EvalError::Internal(format!("vtable slot names `{}::{}`, which is not registered", type_path, method)))?;
        let v = self.reify(heap, &f)?;
        Ok(typelisp_rt::encode(v))
    }

    /// Applies an *interpreted* closure that compiled code is calling —
    /// [`rt_apply_interpreted`]'s body, and the compiled -> interpreted
    /// direction of the boundary [`Self::call_compiled`] crosses the other
    /// way. `closure` and `argv` are raw compiled-side words.
    ///
    /// The representations to decode and encode by come from the closure box
    /// itself ([`typelisp_mem::BoxedObj::Closure`]'s `params`/`ret`), because
    /// nothing else at this point has them: the apply site knows the callee's
    /// `Fn` type statically but carries none of it into the emitted call, and
    /// the words are exactly as ambiguous here as they are anywhere else on
    /// this boundary (a raw `f64` bit pattern and a tagged box pointer are
    /// both just `i64`s). A built-in used as a function value has no
    /// signature to read, which is why it is rejected before it can ever
    /// cross — see [`Self::encode_crossing_value`].
    fn apply_interpreted(&self, heap: &mut Heap, closure: i64, argv: &[i64]) -> Result<i64, EvalError> {
        let f = typelisp_rt::decode(closure);
        let id = match f {
            Value::Boxed(id) if heap.is_closure(id) => id,
            Value::Boxed(id) if heap.is_builtin_fn(id) => {
                return Err(EvalError::Internal(format!(
                    "the built-in \"{}\" cannot be applied from compiled code: its argument representations are not carried at the apply site",
                    heap.builtin_fn_name(id)
                )))
            }
            other => return Err(EvalError::Internal(format!("the callee is not an interpreted closure: {:?}", other))),
        };
        let (params, _, _) = heap.closure_parts(id);
        let param_reprs = core_eval::param_reprs(heap, params)?;
        let ret = Repr::read(heap, heap.closure_ret(id))
            .ok_or_else(|| EvalError::Internal("the closure has no declared return representation".to_string()))?;
        if argv.len() != param_reprs.len() {
            return Err(EvalError::Internal(format!(
                "arity mismatch: the closure takes {} argument(s), given {}",
                param_reprs.len(),
                argv.len()
            )));
        }

        let mut s = RootScope::new(heap);
        // The closure box is reachable from the caller's compiled frame,
        // which the collector cannot see; the arguments the caller passed are
        // rooted on its side, but their *decoded* forms (a `Repr::Float`
        // argument allocates a box) are new objects reachable from nothing.
        s.push_root(f);
        let mut args = Vec::with_capacity(argv.len());
        for (raw, r) in argv.iter().zip(&param_reprs) {
            let v = self.decode_compiled_return(&mut s, *raw, r)?;
            s.push_root(v);
            args.push(v);
        }
        let v = self.call_interpreted_closure(&mut s, id, args)?;
        s.push_root(v);
        self.encode_crossing_value(&s, &v, &ret)
    }

    /// Finds the registered `(Path, String)` key for a `"type-path::method"`
    /// name — `None` for a plain name (no `"::"`) or a method that isn't
    /// registered. `name`'s *last* `"::"`-separated segment is always the
    /// method; everything before it is the type's own full `::`-joined
    /// path (`rsplit_once`, not `split_once` — a module-qualified type has
    /// more than one segment of its own before the method). Resolved by
    /// direct descent ([`scope::ModuleScope::get_method`]) against that
    /// full path, not a whole-tree scan by local type name alone — two
    /// different types in different modules sharing both a local name and
    /// a method name must never collide on one key (this is also why
    /// `core_bridge.rs`'s `assoc`/`methodref` translation and
    /// `user_method_symbol_name`'s mangled LLVM symbol both embed the type's
    /// full path too, not just its local segment). Shared by
    /// [`Self::resolve_fn_def`] (the lookup) and [`Self::compile_function`]
    /// (which `(compile name)` for a method) — both need the exact same
    /// `(Path, String)` key.
    pub fn method_key(&self, name: &str) -> Option<(Path, String)> {
        let (type_part, method) = name.rsplit_once("::")?;
        let type_path = Path::from_segments(type_part.split("::").map(|s| s.to_string()).collect());
        self.root.borrow().get_method(&type_path, method)?;
        Some((type_path, method.to_string()))
    }

    /// Resolves a `(compile name)` argument against either the scope tree's
    /// free functions (a plain name, a top-level `defun`) or its methods (a
    /// `"type::method"` name, an instance/static `defmethod` — including a `defstruct`'s
    /// auto-generated field accessor/setter, whose body is the `field-get`/
    /// `FieldSet` `compile-field-get`/`compile-field-set` exist to compile in
    /// the first place). See [`Self::method_key`] for how `name` decides
    /// which of the two this is.
    pub(crate) fn resolve_fn_def(&self, name: &str) -> Result<Rc<FnDef>, EvalError> {
        // A `"::"` name is a `type::method` (a `defmethod`) *or* a
        // module-qualified `defun` (`m::inc`) — both share the separator.
        // `method_key` matches only a genuinely registered method, so try it
        // first; a miss falls through to a direct-descent lookup by the full
        // parsed path (interp-closure removal: module-qualified closure
        // targets).
        if name.contains("::") {
            if let Some((type_path, method)) = self.method_key(name) {
                return self.root.borrow().get_method(&type_path, &method).ok_or_else(|| EvalError::NoSuchFunction(name.to_string()));
            }
            return self
                .root
                .borrow()
                .get_fn(&fn_path_from_node_name(name))
                .ok_or_else(|| EvalError::NoSuchFunction(name.to_string()));
        }
        self.root.borrow().get_fn(&Path::root(name)).ok_or_else(|| EvalError::NoSuchFunction(name.to_string()))
    }

    /// Whether `name` — a `defun` name, or `type-path::method` — currently has
    /// a compiled body installed, so calls to it run native rather than being
    /// tree-walked.
    ///
    /// Public because "did the precompiled prelude actually take effect?" is
    /// otherwise unanswerable from outside the crate: every observable of a
    /// compiled call is identical to the interpreted one *except* how fast it
    /// is, which is exactly the wrong thing to assert on. `false` for a name
    /// that resolves to nothing, since an absent definition has no compiled
    /// body either.
    pub fn is_compiled(&self, name: &str) -> bool {
        self.resolve_fn_def(name).is_ok_and(|f| f.compiled.borrow().is_some())
    }

    /// Looks up `name`'s registered `defun`/`defmethod` body (see
    /// [`Self::resolve_fn_def`]), enforcing the one constraint every entry
    /// point into the compiler shares: a real type signature (so an LLVM
    /// function type can be built — never set for a `defmacro`). A multi-
    /// expression body is collapsed to one form via
    /// [`crate::compile::symbols::single_body_expr`] (labels/closures
    /// Stage 6 — see that function's doc comment for why this needs no
    /// `compiler.rs` change). For an instance method, `params`/`sig.0`
    /// already carry the receiver as element `0` (`Checker::check_defmethod`
    /// pushes the receiver's own type onto `sig_params` before the method's
    /// declared parameters) — so it flows through exactly like any other
    /// parameter here, no special-casing needed. Shared by
    /// [`Self::add_compiled_function`] (the actual AST-bridge step) and
    /// [`Self::compile_function`]/[`Self::call_graph_edges`] (which need the
    /// body slightly earlier — to collect `call` targets — before
    /// `add_compiled_function` ever runs).
    pub fn compiled_fn_body(&self, name: &str) -> Result<(Vec<(String, Repr)>, Vec<Value>), EvalError> {
        let f = self.resolve_fn_def(name)?;
        let sig = f
            .sig
            .as_ref()
            .ok_or_else(|| EvalError::Panic(format!("compile: \"{}\" has no signature (is it a defmacro?)", name)))?;
        let params = f.params.iter().cloned().zip(sig.0.iter().cloned()).collect();
        // The body's forms, not one spliced expression: `core_bridge` takes a
        // sequence, so there is nothing for `single_body_expr`'s old
        // wrap-in-a-`progn` to do.
        Ok((params, f.body.clone()))
    }

    /// Ensures `path` (a global some compiled function's body references)
    /// has a compiled-global slot, promoting it from its ordinary
    /// interpreter [`Slot`] on first reference; returns the slot's id
    /// either way. See [`Self::compiled_globals`]'s doc comment for why
    /// promotion — not moving every global's storage — is the shape this
    /// takes: reads the global's *current* value (whatever the interpreter
    /// last set it to) via the existing `Slot` and hands it to
    /// `typelisp_rt::global_new`, which roots it permanently and returns its
    /// id.
    ///
    /// Nothing is converted on the way. There used to be a crossing here — a
    /// global's storage has the shape of a single always-live boxed-struct
    /// field, so it went through the same `RtValue` -> `mem::Value` mapping a
    /// field write did, with a raw-box/shift-tagged special case for an enum
    /// global on top. The value worlds were unified and that mapping became the
    /// identity, then went; the enum special case had already been retired by
    /// the enum-representation unification's compiler flip.
    ///
    /// `pub(crate)`: `compile::aot::compile_file` calls this directly too,
    /// once per `defvar`, *before* compiling any `defun` — eagerly, in file
    /// declaration order, rather than waiting for some `defun` body to
    /// reference it. That ordering matters only for AOT: the ids this
    /// assigns are baked into compiled IR at this (the *compiling*)
    /// process's `Heap`, but AOT's actual runtime storage is established by
    /// a *different* `Heap` — the standalone executable's own, via a
    /// generated startup sequence (`Self::add_compiled_global_init`) that
    /// must call `rt_global_new` in this exact same order for the two
    /// numberings to agree. Running that sequence in file-declaration order
    /// is what keeps it correct even when one `defvar`'s initializer
    /// references an earlier one — the checker's forward-reference
    /// restriction guarantees "earlier in the file" for any such reference,
    /// so eager, in-order promotion here guarantees "already promoted,
    /// lower id" for it too. The JIT path (`Self::add_compiled_function`)
    /// has no such concern — compiling and running happen in the same
    /// `Heap` there, so lazy, reference-driven promotion (its own call
    /// here) is simpler and just as correct.
    /// A global's current value, wherever it actually lives.
    ///
    /// A *promoted* global (one compiled code can reach) is stored in a
    /// permanent GC root, and `set-global` writes only there — its
    /// interpreter-side cell keeps whatever it was initialized with. So Rust
    /// code that reads a prelude global by name must ask here rather than
    /// through `root.get_global(..).get(heap)`, which is the cell.
    ///
    /// The `eval_core` reader (`global_core`) has always made this
    /// distinction; the printer-settings readers did not, and once the
    /// precompiled prelude started promoting *every* prelude `defvar` eagerly,
    /// `(setf *print-pretty* true)` stopped reaching [`Self::pretty_opts`] —
    /// pretty printing silently never happened.
    pub(crate) fn global_value(&self, heap: &Heap, path: &Path) -> Option<Value> {
        if let Some(&id) = self.compiled_globals.borrow().get(path) {
            // See `global_core`: the id is not the permanent-root position,
            // so it is resolved the same way `rt_global_get` resolves it.
            return typelisp_rt::global_perm_idx(id).map(|i| heap.permanent_root(i));
        }
        self.root.borrow().get_global(path).map(|slot| slot.get(heap))
    }

    /// Record that `path`'s storage is the compiled-global slot `id`, without
    /// creating one — [`Self::promote_global`]'s half for an environment that
    /// is *joining* a running compiled program rather than preparing one.
    ///
    /// The AOT `eval` environment is the only caller: a standalone executable
    /// created its globals at startup with ids `compile-file` baked into its
    /// machine code, and the interpreter the executable builds for `eval` has
    /// to address those same slots rather than allocate a second set nobody
    /// else can see. `global_core`/`set_global_core` consult this map before
    /// the module tree, so binding it is the whole of sharing the storage.
    pub fn bind_compiled_global(&self, path: Path, id: usize) {
        self.compiled_globals.borrow_mut().insert(path, id);
    }

    /// Whether `path` already has a compiled-global slot — "is this variable
    /// already initialized by somebody else?", asked by the AOT `eval`
    /// environment before it replays a `defvar` (see
    /// [`Self::bind_compiled_global`]).
    pub fn has_compiled_global(&self, path: &Path) -> bool {
        self.compiled_globals.borrow().contains_key(path)
    }

    /// The compiled-global slot id [`Self::promote_global`] assigned `path`,
    /// if it has one.
    pub fn compiled_global_id(&self, path: &Path) -> Option<usize> {
        self.compiled_globals.borrow().get(path).copied()
    }

    pub fn promote_global(&self, heap: &mut Heap, path: &Path) -> Result<usize, EvalError> {
        if let Some(&id) = self.compiled_globals.borrow().get(path) {
            return Ok(id);
        }
        let slot = self
            .root
            .borrow()
            .get_global(path)
            .ok_or_else(|| EvalError::Internal(format!("compile: global \"{}\" is not defined", path)))?;
        let v = slot.get(heap);
        let id = typelisp_rt::global_new(heap, v);
        self.compiled_globals.borrow_mut().insert(path.clone(), id);
        Ok(id)
    }


    /// Build the argument vector for a macro call: the first `fixed` raw
    /// forms map 1:1 to parameters; if `f.rest`, every remaining raw
    /// form is collected into a single heap-allocated `Sexpr` list (built
    /// back-to-front, like `Self::alloc_quoted`'s `Cons` case) bound to the
    /// last parameter. Each element is already rooted by the caller (it's in
    /// `raw_args`, individually pushed in `Self::expand_macro`); only the
    /// growing `list` accumulator needs protecting around each `cons` call.
    /// Bind a macro call's raw (unevaluated) argument forms to its parameters,
    /// producing one value per parameter in `f.params` order —
    /// required, then `&optional`, then `&rest`, then `&key` — ready for
    /// [`Self::apply`]. Omitted `&optional`/`&key` arguments evaluate their
    /// default-value body (in an environment holding the params already bound,
    /// CL-style); an empty body binds `nil` (`Sexpr::Nil`).
    ///
    /// GC safety: every value built here (a `&rest` list, an evaluated
    /// default) is stashed in a live heap cell as it is produced, and
    /// each cell is an implicit heap root (`Heap::alloc_cell`) for as long as
    /// the returned `env` lives. Those cells also form the environment the
    /// next default is evaluated in, so an earlier bound param is protected
    /// across a later default's allocations. The caller drops `env` only after
    /// `apply` has consumed `argv` (and `apply`'s own cell allocations can
    /// never trigger a collection — see `Self::slot`), so the raw `argv`
    /// pointers stay valid in the gap between.
    fn bind_macro_args(&self, heap: &mut Heap, f: &FnDef, raw_args: &[Value]) -> Result<Vec<Value>, EvalError> {
        let lambda = f
            .lambda
            .as_ref()
            .ok_or_else(|| EvalError::Internal("bind_macro_args on a non-macro FnDef".into()))?;
        let n_req = lambda.required;
        let n_opt = lambda.optionals.len();
        if raw_args.len() < n_req {
            return Err(EvalError::Panic(format!("expected at least {} argument(s), got {}", n_req, raw_args.len())));
        }

        // Fast path — the overwhelmingly common macro shape: only required
        // params, optionally a trailing `&rest`, with no `&optional`/`&key`
        // defaults to evaluate. No default eval means no allocation that could
        // trigger a GC after the (already-rooted) raw args are placed, so the
        // protective per-param Heap-cell environment the general path builds is
        // unnecessary here (`&rest` list aside, which `build_sexpr_list` roots
        // internally). This keeps `while`/`dotimes`/`cond`/... — expanded en
        // masse during checking — allocation-free, exactly as before this
        // feature.
        if n_opt == 0 && lambda.keys.is_empty() {
            let mut argv: Vec<Value> = raw_args[..n_req].iter().map(|v| *v).collect();
            if f.rest {
                let list = self.build_sexpr_list(heap, &raw_args[n_req..])?;
                argv.push(list);
            } else if raw_args.len() > n_req {
                return Err(EvalError::Panic(format!("expected {} argument(s), got {}", n_req, raw_args.len())));
            }
            return Ok(argv);
        }

        // The positional region is `required + optional`; `&rest`/`&key`
        // arguments (if any) begin right after however many of those slots the
        // call actually filled.
        let n_pos = n_req + n_opt;
        let pos_end = raw_args.len().min(n_pos);

        let mut argv: Vec<Value> = Vec::with_capacity(f.params.len());
        // The environment the `&optional`/`&key` defaults are evaluated in: the
        // ordinary heap chain `eval_core` walks, one frame per binding, so a
        // default sees exactly the parameters bound before it and nothing after
        // — `let*`, which is what CL specifies here.
        //
        // This is also what keeps the bound values alive: a frame is heap data
        // the collector traces, so the protective per-parameter cells an earlier
        // version built purely to anchor them are unnecessary. One mechanism,
        // both jobs.
        let roots_base = heap.root_count();
        let mut env = Value::Empty;
        let mut pi = 0usize; // index into `f.params`
        // A closure would borrow `self`/`heap` mutably twice, so bind inline.
        macro_rules! bind {
            ($v:expr) => {{
                let v = $v;
                heap.push_root(v);
                let sym = match heap.intern_symbol(&f.params[pi]) {
                    Value::Symbol(id) => id,
                    other => unreachable!("intern_symbol returned {:?}", other),
                };
                env = self::core_eval::extend_env(heap, &[(sym, v)], env)?;
                heap.push_root(env);
                argv.push(v);
                pi += 1;
            }};
        }

        // required
        for &a in &raw_args[..n_req] {
            bind!(a);
        }
        // &optional — filled positionally, else its default (or nil)
        for (oi, default) in lambda.optionals.iter().enumerate() {
            let arg_idx = n_req + oi;
            let v = if arg_idx < pos_end {
                raw_args[arg_idx]
            } else {
                self.eval_macro_default(heap, default, env)?
            };
            bind!(v);
        }
        // The trailing region past the positional args: a `&rest` list and/or
        // a `&key` plist both read from it (CL binds `&rest` to the whole tail
        // even when `&key` is also present).
        let tail = &raw_args[pos_end..];
        if f.rest {
            let list = self.build_sexpr_list(heap, tail)?;
            bind!(list);
        }
        if !lambda.keys.is_empty() {
            let supplied = self.parse_keyword_args(heap, tail, &lambda.keys)?;
            for (kname, default) in &lambda.keys {
                let v = match supplied.get(kname) {
                    Some(val) => *val,
                    None => self.eval_macro_default(heap, default, env)?,
                };
                bind!(v);
            }
        } else if !f.rest && !tail.is_empty() {
            heap.truncate_roots(roots_base);
            return Err(EvalError::Panic(format!("expected at most {} argument(s), got {}", n_pos, raw_args.len())));
        }

        // The bound values are about to be re-bound by `apply` from `argv`, so
        // the frames built here have done their job. `argv` itself is rooted by
        // the caller (`expand_macro` roots the raw args, and every value in
        // `argv` is either one of those or a default the caller roots next).
        heap.truncate_roots(roots_base);
        Ok(argv)
    }

    /// Evaluate an `&optional`/`&key` default-value body to the `Sexpr` value
    /// bound when the argument is omitted — an empty body means `nil`
    /// (`Sexpr::Nil`).
    ///
    /// `env` holds the parameters bound before this one, which a default may
    /// reference: `(defmacro dup (x &optional (y x)) ...)` copies `x` when `y`
    /// is omitted. That is CL's rule, and `Checker::check_defmacro` checks each
    /// default in exactly this environment — so dropping `env` here made the
    /// checker accept a form the expander could not run (`macro `dup`: unbound
    /// variable: x`).
    fn eval_macro_default(&self, heap: &mut Heap, default: &[Value], env: Value) -> Result<Value, EvalError> {
        // The empty body is "no default", which binds `nil` at expansion time.
        let mut result = Value::Empty;
        for form in default {
            result = self.eval_core(heap, *form, env)?;
        }
        Ok(result)
    }

    /// Build a fresh `Sexpr` list from `items`, rooting the growing tail
    /// across each `cons` (which may collect). The shared tail-builder for a
    /// macro's `&rest` parameter.
    fn build_sexpr_list(&self, heap: &mut Heap, items: &[Value]) -> Result<Value, EvalError> {
        let mut list = Value::Empty;
        for v in items.iter().rev() {
            heap.push_root(list);
            // core-build-ok: a `Sexpr` list of user data for a macro's `&rest`,
            // not a core form. The growing tail is rooted on the line above and
            // popped on the line below.
            let consed = heap.cons(*v, list);
            heap.pop_root();
            list = consed.map_err(|e| EvalError::Panic(e.to_string()))?;
        }
        Ok(list)
    }

    /// Parse a macro call's trailing arguments as a `&key` plist — pairs of
    /// `(:name, value)` — returning each supplied param name mapped to its
    /// value form. A keyword must be a symbol `:name` matching one of the
    /// declared keys; the plist must have even length; a duplicate keeps the
    /// first value (CL). Unknown keywords and odd length are errors.
    fn parse_keyword_args(
        &self,
        heap: &Heap,
        tail: &[Value],
        keys: &[(String, Vec<Value>)],
    ) -> Result<HashMap<String, Value>, EvalError> {
        if tail.len() % 2 != 0 {
            return Err(EvalError::Panic("odd number of &key arguments (each key needs a value)".into()));
        }
        let mut map: HashMap<String, Value> = HashMap::new();
        let mut i = 0;
        while i < tail.len() {
            let name = match tail[i] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(EvalError::Panic("&key argument name must be a keyword symbol like :name".into())),
            };
            let stripped = name
                .strip_prefix(':')
                .ok_or_else(|| EvalError::Panic(format!("expected a keyword like :name, got `{}`", name)))?;
            if !keys.iter().any(|(k, _)| k == stripped) {
                return Err(EvalError::Panic(format!("unknown &key argument `:{}`", stripped)));
            }
            map.entry(stripped.to_string()).or_insert(tail[i + 1]);
            i += 2;
        }
        Ok(map)
    }



    /// Evaluate a built-in operator. Returns `None` if `name` is not a
    /// builtin, so the caller can fall through to a "no such function" error.
    /// (Arithmetic/comparison operators are *instance* methods, not free
    /// functions — see `eval_builtin_method` — so they don't appear here.
    /// `cons`/`car`/`cdr` operate on `Sexpr`; `car`/`cdr` of a non-`Cons`
    /// `Sexpr` — including `Nil` — panics. `gensym` returns a fresh
    /// `Sexpr::Sym` each call. `make-random-state-fresh`/`random-state-copy`/
    /// `random-state-next` have no natural receiver to dispatch on, so they
    /// stay free functions too — `random`/`make-random-state`/
    /// `random-state-p` are ordinary prelude `defun`s built on top of them.)
    fn eval_builtin(&self, heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, EvalError>> {
        // The printing builtins, ahead of the match so there is one list of
        // their names rather than two. Most are what a special form lowered
        // to — `format`/`print`/`println`/`pprint` and `pprint-logical-block`
        // are checked forms, and what survives checking is a call to one of
        // the `*-rt` names; the four `pprint-*` operators are ordinary
        // builtins. All of them run the same
        // `typelisp_print::runtime::print_builtin` that compiled code reaches
        // through `rt_format`/`rt_print`/..., so a `println` inside a
        // `pprint-logical-block` lands in the same buffer whichever side of
        // the compile boundary each half was on.
        self.install_print_hooks();
        if let Some(r) = typelisp_print::runtime::print_builtin(heap, name, args) {
            use typelisp_print::runtime::PrintError;
            return Some(r.map_err(|e| match e {
                // A `~` directive the engine doesn't know, or a
                // `print-object` method that failed: catchable, like the
                // `(panic ...)` it would be if a program wrote it.
                PrintError::Raise(msg) => EvalError::Panic(msg),
                // An argument the checker cannot have produced.
                PrintError::Shape(msg) => EvalError::Internal(msg),
            }));
        }
        match name {
            // `(compile-file "source.typl" "output")`: AOT-compiles an
            // independent source file straight to a native executable —
            // see `compile::aot::compile_file`'s doc comment for why this
            // runs against a *fresh* `Heap`/`Checker`/`Interp` rather than
            // the caller's (`self`'s), unlike `compile` above.
            "compile-file" => {
                let source_path = match expect_str(heap, &args[0]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let output_path = match expect_str(heap, &args[1]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                Some(
                    (match backend("compile-file") {
                        Ok(b) => b,
                        Err(e) => return Some(Err(e)),
                    }
                    .compile_file)(&source_path, &output_path)
                        .map(|()| Value::Bool(true))
                        .map_err(|e| EvalError::Panic(format!("compile-file: {}", e))),
                )
            }
            // `(dump "path")`: this session's environment, written where
            // `typl --image` can pick it up — see `compile::dump::dump_image`.
            "dump" => {
                let path = match expect_str(heap, &args[0]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let backend = match backend("dump") {
                    Ok(b) => b,
                    Err(e) => return Some(Err(e)),
                };
                Some(
                    (backend.dump_image)(self, heap, &path)
                        .map(|()| Value::Bool(true))
                        .map_err(EvalError::Panic),
                )
            }
            name if name.starts_with("stream-") || name.starts_with("file-") => {
                self.eval_stream_builtin(heap, name, args)
            }
            "make-random-state-fresh" => Some(eval_make_random_state_fresh(heap, args)),
            "random-state-copy" => Some(eval_random_state_copy(heap, args)),
            "random-state-next" => Some(eval_random_state_next(heap, args)),
            "get-universal-time" => Some(eval_get_universal_time(args)),
            "get-internal-real-time" => Some(eval_get_internal_real_time(args)),
            // The environment and the running implementation. Each is one
            // call into `typelisp_rt::sys_builtin`, the same implementation
            // the compiled shims wrap — the `file-*` half of Phase 9c needs
            // no arm here at all, since the `starts_with("file-")` guard
            // above already routes it to `eval_stream_builtin`.
            "command-line-args" => Some(Ok(typelisp_rt::sys_builtin::command_line_args(heap))),
            "getenv" => Some(match &args[0] {
                Value::Str(id) => {
                    let name = heap.string(*id).to_string();
                    Ok(typelisp_rt::sys_builtin::getenv(heap, &name))
                }
                other => Err(EvalError::Panic(format!("getenv: argument is not a string, got {:?}", other))),
            }),
            "home-directory" => Some(Ok(typelisp_rt::sys_builtin::home_directory(heap))),
            "lisp-implementation-version" => Some(Ok(typelisp_rt::sys_builtin::lisp_implementation_version(heap))),
            "machine-type" => Some(Ok(typelisp_rt::sys_builtin::machine_type(heap))),
            "software-type" => Some(Ok(typelisp_rt::sys_builtin::software_type(heap))),
            "parse-int" => Some(eval_parse_int(heap, args)),
            "parse-float" => Some(eval_parse_float(heap, args)),
            "read" => Some(eval_read(heap, args)),
            "eval" => Some(self.eval_form(heap, &args[0])),
            // `equal`/`equalp` on `Sexpr`: structural equality builtins (the
            // free-function `Sexpr` overloads; the per-scalar-type `equal`
            // *methods* — `string`/`char`/`int`/... — are dispatched separately
            // in `eval_builtin_method`). Rust builtins since Phase 5 fenced
            // `match` off `Sexpr`; see `sexpr_equal`/`sexpr_equalp`.
            "equal" => Some(sexpr_equal(heap, args)),
            "equalp" => Some(sexpr_equalp(heap, args)),
            // `exit`: terminates the process immediately via the OS, never
            // returning — `rt_i64` truncates to `i32` the same way every
            // other `i32`-typed builtin extracts its argument.
            "exit" => match rt_i64(&args[0]) {
                Ok(code) => std::process::exit(code as i32),
                Err(e) => Some(Err(e)),
            },
            "gensym" => {
                // Both the fresh-name scheme (leading-space, unforgeable) and
                // the monotonic counter live on the `Heap` now
                // (`Heap::gensym`), so this interpreted path and the compiled
                // `rt_gensym` shim share one sequence — see that method's doc
                // comment for why they must.
                Some(Ok(heap.gensym()))
            }
            // `symbol->string`/`string->symbol`: the `Symbol`<->`Str` bridges.
            // A `Symbol` value shares the `Value::Symbol(id)` carrier of a
            // `Sexpr::Sym` (a `Value::Symbol(id)`), so
            // `symbol->string` reads its interned name and `string->symbol`
            // interns a fresh one — the same intern table `gensym`/`read` use.
            "symbol->string" => Some(match args[0] {
                Value::Symbol(id) => {
                    let name = heap.symbol_name(id).to_string();
                    Ok(str_rt(heap, name))
                }
                _ => Err(EvalError::Panic("symbol->string: not a symbol".into())),
            }),
            "string->symbol" => Some(match rt_str(heap, &args[0]) {
                Ok(s) => Ok(heap.intern_symbol(&s)),
                Err(e) => Err(e),
            }),
            // `cons`/`car`/`cdr`/`set-car`/`set-cdr` are no longer `Sexpr`
            // builtins (Symbol/Sexpr redesign Phase 4b): `cons`/`car`/`cdr` are
            // the `cons<T,U>` pair (a prelude `defun` + `cons-cell` accessor
            // methods, run as ordinary user code), and `set-car`/`set-cdr` were
            // dropped. `Sexpr` cons/nil operations live in the `sexpr-*` layer
            // below.
            // Internal `Sexpr` navigation layer (Symbol/Sexpr redesign Phase 1):
            // `sexpr-cons`/`sexpr-car`/`sexpr-cdr` are the island's own aliases
            // for the identical heap operations `cons`/`car`/`cdr` perform, kept
            // separate so Phase 4 can repurpose the user-facing names to a
            // generic `cons<T,U>` pair. `sexpr-consp`/`sexpr-null`/`sexpr-atom`
            // read the runtime tag directly (no `match`), so they survive
            // Phase 5's `match`-to-enum fence.
            "sexpr-cons" => match (args.first(), args.get(1)) {
                (Some(a), Some(b)) => {
                    // core-build-ok: this *is* `cons`, over user data; both
                    // arguments are already rooted as evaluated call arguments.
                    Some(heap.cons(*a, *b).map_err(|e| EvalError::Panic(e.to_string())))
                }
                _ => Some(Err(EvalError::Internal("sexpr-cons: expected two Sexpr arguments".into()))),
            },
            "sexpr-car" => match args.first() {
                Some(v) => {
                    Some(heap.car(*v).map_err(|_| EvalError::Panic("sexpr-car: not a cons".into())))
                }
                None => Some(Err(EvalError::Internal("sexpr-car: expected one argument".into()))),
            },
            "sexpr-cdr" => match args.first() {
                Some(v) => {
                    Some(heap.cdr(*v).map_err(|_| EvalError::Panic("sexpr-cdr: not a cons".into())))
                }
                None => Some(Err(EvalError::Internal("sexpr-cdr: expected one argument".into()))),
            },
            "sexpr-consp" => match args.first() {
                Some(v) => Some(Ok(Value::Bool(v.is_cons()))),
                None => Some(Err(EvalError::Internal("sexpr-consp: expected one argument".into()))),
            },
            "sexpr-null" => match args.first() {
                Some(v) => Some(Ok(Value::Bool(v.is_empty()))),
                None => Some(Err(EvalError::Internal("sexpr-null: expected one argument".into()))),
            },
            "sexpr-atom" => match args.first() {
                Some(v) => Some(Ok(Value::Bool(!v.is_cons()))),
                None => Some(Err(EvalError::Internal("sexpr-atom: expected one argument".into()))),
            },
            // Internal `Sexpr` payload extractors (Symbol/Sexpr redesign Phase 2):
            // the island's own typed field readers, moved out of `compiler.rs`'s
            // `match`-based typelisp defuns so the island stops depending on the
            // user-facing `match` (Phase 5 fences `match` to enum-only). Each
            // reads the runtime `Value` payload directly, mirroring exactly what
            // `match_sexpr_ctor`'s corresponding arm binds — panicking (not
            // `None`-matching) on a tag mismatch, the same contract the old
            // `(_ (panic ...))` catch-all arms had.
            "sexpr-int" => match args.first() {
                Some(Value::Int(n)) => Some(Ok(Value::Int(*n))),
                Some(_) => Some(Err(EvalError::Panic("sexpr-int: expected an Int Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-int: expected a Sexpr argument".into()))),
            },
            "sexpr-bool" => match args.first() {
                Some(Value::Bool(b)) => Some(Ok(Value::Bool(*b))),
                Some(_) => Some(Err(EvalError::Panic("sexpr-bool: expected a Bool Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-bool: expected a Sexpr argument".into()))),
            },
            // `sexpr-char`: peer of `sexpr-int`/`sexpr-bool` for a `Char` node
            // (`Value::Char`, an ordinary immediate, unlike `Float`'s boxed
            // payload) — added alongside `compile-char`/the `char` dispatch tag
            // (compiled code previously had no way to build/read a bare `char`
            // literal at all, an oversight discovered while implementing
            // `quote`, whose `Char` leaf needs exactly this).
            "sexpr-char" => match args.first() {
                Some(Value::Char(c)) => Some(Ok(Value::Char(*c))),
                Some(_) => Some(Err(EvalError::Panic("sexpr-char: expected a Char Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-char: expected a Sexpr argument".into()))),
            },
            // `sexpr-float`: peer of `sexpr-int` for a `Float` node (heap-boxed,
            // `Value::Boxed` — see `BoxedObj`). Added with the Phase 5 `match`
            // fence so a `Sexpr::Float` payload can still be read out without a
            // `(match s ((Float f) f) ..)`.
            "sexpr-float" => match args.first() {
                // The node *is* the float box since the scalar
                // unification, so reading the payload out is the identity.
                Some(v @ Value::Boxed(id)) if heap.is_float(*id) => Some(Ok(v.clone())),
                Some(_) => Some(Err(EvalError::Panic("sexpr-float: expected a Float Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-float: expected a Sexpr argument".into()))),
            },
            "sexpr-str" => match args.first() {
                // The node *is* the heap string since the scalar
                // unification, so reading the payload out is the identity.
                Some(v @ Value::Str(_)) => Some(Ok(v.clone())),
                Some(_) => Some(Err(EvalError::Panic("sexpr-str: expected a Str Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-str: expected a Sexpr argument".into()))),
            },
            // `(Sym v)` binds `v : Symbol`, then `symbol->string` reads its name;
            // this fuses the two, matching the old defun `(symbol->string v)`.
            "sexpr-sym-name" => match args.first() {
                Some(Value::Symbol(id)) => {
                    let name = heap.symbol_name(*id).to_string();
                    Some(Ok(str_rt(heap, name)))
                }
                Some(_) => Some(Err(EvalError::Panic("sexpr-sym-name: expected a Sym Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-sym-name: expected a Sexpr argument".into()))),
            },
            // `sexpr-symp`: the tag predicate a `match (car x) ((Sym s) ...) (_ ...))`
            // with a *non-panic* fallback rewrites to (`form-is-borrowed?` in
            // `compiler.rs`) — a peer of `sexpr-consp`/`sexpr-null`/`sexpr-atom`,
            // reading the tag directly.
            "sexpr-symp" => match args.first() {
                Some(v) => Some(Ok(Value::Bool(matches!(v, Value::Symbol(_))))),
                None => Some(Err(EvalError::Internal("sexpr-symp: expected one argument".into()))),
            },
            _ => None,
        }
    }

    /// The `print-object` dispatch behind every `~a`/`~s`/`~w`/`pprint`
    /// rendering (`crate::eval::format::render_value` calls this first).
    /// `Ok(None)` means "no method applies, render `v` the built-in way".
    ///
    /// The *registration* is entirely static: writing `(impl print-object
    /// point ...)` registers an ordinary method on `point`, so the lookup
    /// here is just the module tree — there is no separate dispatch table to
    /// keep in sync, and no way to register a printer under a type it isn't
    /// for (the `impl` is type-checked like any other). What cannot be static
    /// is the *selection*: which directive consumes which argument depends on
    /// the control string's runtime contents, so only here — with `escape`
    /// in hand (CL's `*print-escape*`: `~s` true, `~a` false) — is it known
    /// what to ask the method for. That is the same split CLOS makes, where
    /// `print-object` methods are defined per class but selected at print
    /// time.
    ///
    /// The signature check is a safety net: it keeps an unrelated method that
    /// merely happens to be named `print-object` (a plain `defmethod`, no
    /// trait involved) from being mistaken for an implementation of it.
    ///
    /// GC: this runs user code, which conses, which can collect — but the
    /// value being printed needs no rooting *here*. Every builtin's arguments
    /// are already anchored for the whole call by `Self::eval_args`, which
    /// binds each one into a heap cell the collector traces on its own. Since
    /// collection is a non-moving mark-sweep that follows `car`/`cdr` and
    /// boxed nested values, that one anchor on the outermost value covers
    /// every part of it, including the elements the renderers hold in
    /// intermediate `Vec<Value>`s.
    pub(crate) fn print_object(&self, heap: &mut Heap, v: Value, escape: bool) -> Result<Option<String>, String> {
        let Value::Boxed(id) = v else { return Ok(None) };
        let Some(name) = heap_type_path(heap, id).map(|p| p.to_string()) else {
            return Ok(None);
        };
        // A value already being printed by its own method is rendered the
        // built-in way instead, so `(impl print-object point (... (format
        // false "~a" self)))` degrades to `#<point 1 2>` rather than
        // recursing forever. Keyed on the value, not a depth limit, so a
        // genuinely nested self-referential structure still prints in full.
        if self.printing.borrow().contains(&v) {
            return Ok(None);
        }
        let type_path = Path::from_segments(name.split("::").map(|s| s.to_string()).collect());
        let Some(f) = self.root.borrow().get_method(&type_path, "print-object") else {
            return Ok(None);
        };
        match f.sig.as_ref() {
            Some((params, ret))
                if params.len() == 2 && params[1] == Repr::Bool && *ret == Repr::Str => {}
            _ => return Ok(None),
        }
        self.printing.borrow_mut().push(v);
        let result = self.apply(heap, &f, vec![v, Value::Bool(escape)]);
        self.printing.borrow_mut().pop();
        match result.map_err(|e| e.to_string())? {
            Value::Str(id) => Ok(Some(heap.string(id).to_string())),
            other => Err(format!("print-object on `{}` returned {:?}, not a string", type_path, other)),
        }
    }

    /// Reads the three "what to print" globals — `*print-circle*`,
    /// `*print-level*`, `*print-length*` (CLHS 22.1.1) — the same way
    /// [`Self::pretty_opts`] reads the three "how to lay it out" ones: fresh
    /// on every printing operation, since typelisp has no dynamic binding.
    ///
    /// CL writes "no limit" as `nil`; the prelude's globals are `i64`s where 0
    /// or less means unlimited, matching `*print-right-margin*`/
    /// `*print-miser-width*`. A missing global (no prelude — some unit tests
    /// build a bare `Interp`) means every limit is off, which is also CL's
    /// initial state for all three.
    /// The name of `variant` of the enum whose type key is `type_key` — the
    /// printer's question, answered from the scope tree.
    ///
    /// The tree holds every `TypeEntry::Enum`: a user `defenum`'s own exec,
    /// *and* the built-in sum types (`Option`/`Result`/the error types),
    /// which `Interp::new` seeds from `registry::builtin_sum_defs` up front
    /// precisely so this lookup never needs a second table to fall back to.
    /// Coming up empty means the lookup itself is broken (a stale key, an
    /// enum the tree was never told about), not that the name lives
    /// somewhere else — which is why it prints as the loud
    /// `<unknown-variant>` rather than anything plausible.
    fn enum_variant_name(&self, type_key: &str, variant: usize) -> Option<String> {
        let path = Path::from_segments(type_key.split("::").map(str::to_string).collect());
        self.root.borrow().enum_variant_name(&path, variant)
    }

    pub(crate) fn print_limits(&self, heap: &Heap) -> crate::eval::format::Limits {
        let read_limit = |name: &str| -> Option<usize> {
            match self.global_value(heap, &crate::Path::root(name)) {
                Some(Value::Int(n)) if n > 0 => Some(n as usize),
                _ => None,
            }
        };
        crate::eval::format::Limits {
            circle: matches!(
                self.global_value(heap, &crate::Path::root("*print-circle*")),
                Some(Value::Bool(true))
            ),
            level: read_limit("*print-level*"),
            length: read_limit("*print-length*"),
        }
    }

    /// Closes and writes out a pretty-printing session left open by a
    /// non-local exit (see [`Self::exec`]). A no-op in the normal case, where
    /// the matching `pprint-block-end-rt` already flushed it.
    fn flush_pretty(&self) -> Result<(), EvalError> {
        typelisp_print::runtime::flush().map_err(EvalError::Panic)
    }

    /// Reads the three pretty-printing globals the prelude defines —
    /// `*print-pretty*`, `*print-right-margin*`, `*print-miser-width*` — into
    /// the snapshot [`crate::eval::pprint`] works from.
    ///
    /// typelisp has no dynamic (`let`-rebindable) special variables, so these
    /// are ordinary assignable globals read fresh on every printing operation:
    /// `(setf *print-pretty* true)` takes effect from the next `print` on, and
    /// stays in effect, which is the closest analogue of CL's
    /// `(setf (symbol-value '*print-pretty*) t)` at top level. A margin of 0
    /// or less means "no margin" (never break); a miser width of 0 or less
    /// means miser style is off, standing in for CL's `nil`.
    ///
    /// A missing global (the prelude was not loaded — some unit tests build a
    /// bare `Interp`) falls back to the defaults, i.e. pretty printing off.
    pub(crate) fn pretty_opts(&self, heap: &Heap) -> crate::eval::pprint::Opts {
        let read_int = |name: &str, default: i64| -> i64 {
            match self.global_value(heap, &crate::Path::root(name)) {
                Some(Value::Int(n)) => n,
                _ => default,
            }
        };
        // Inside a `pprint-logical-block` the "stream" *is* a pretty stream,
        // so everything printed into it pretty-prints regardless of the
        // global — the same thing CL's stream-type dispatch achieves.
        let pretty = typelisp_print::runtime::session_open()
            || matches!(
                self.global_value(heap, &crate::Path::root("*print-pretty*")),
                Some(Value::Bool(true))
            );
        let margin = read_int("*print-right-margin*", crate::eval::pprint::DEFAULT_MARGIN as i64);
        let miser = read_int("*print-miser-width*", 0);
        crate::eval::pprint::Opts {
            pretty,
            margin: margin.max(0) as usize,
            miser: (miser > 0).then_some(miser as usize),
        }
    }

    /// The `eval` builtin: type-check `arg` (a runtime `Sexpr`) against the
    /// program's *current* global environment and run it, CL-style —
    /// `(eval form) => Result<Sexpr, Error>`. Sees every global definition
    /// (functions, variables, types, macros), including ones added earlier at
    /// runtime; does *not* see the caller's lexical locals (the empty `Env`
    /// below is CL's "null lexical environment"). A form that *defines*
    /// something (`(eval '(defun foo () 42))`) registers it immediately and
    /// permanently, exactly as if typed at top level — that is why `root` is a
    /// `RefCell` (this `&self` path must write it) and why the checker handle
    /// is `&mut`-borrowed for `check_form_at`.
    ///
    /// Return value follows CL: an expression yields its value (as a `Sexpr`);
    /// a definition yields the defined name as a symbol. A malformed or
    /// ill-typed form is a recoverable `Err(error ...)`, not a panic — `eval`
    /// is handed runtime data the program doesn't control. (A *runtime* panic
    /// inside the evaluated code, e.g. division by zero, still propagates like
    /// any other, matching code written directly.)
    ///
    /// GC-root discipline mirrors `crate::main`'s `try_run_pending`: the raw
    /// argument `Value` rides on the Rust stack, where the collector cannot
    /// see it, so it must be `push_root`ed across `check_form_at` — which
    /// conses during macro expansion and can trigger a GC — and popped back
    /// to the entry mark before `exec`. The result-building afterward
    /// only allocates through the growable box store (`alloc_enum`/
    /// `intern_symbol`/scalar boxing), never `cons`, so it needs no rooting.
    pub fn eval_form(&self, heap: &mut Heap, arg: &Value) -> Result<Value, EvalError> {
        let checker = match &self.checker {
            Some(c) => Rc::clone(c),
            None => return Ok(result_err(heap, EVAL_ERROR, "eval: unavailable in this context (no checker handle)".to_string())),
        };
        let form = *arg;
        // Root the form across type-checking (which allocates), then pop back
        // to the entry mark before exec — never hold a manual root across the
        // exec below.
        let mark = heap.root_count();
        heap.push_root(form);
        let checked = checker.borrow_mut().check_form_at(heap, self, form, None);
        while heap.root_count() > mark {
            heap.pop_root();
        }
        let tl = match checked {
            Ok(tl) => tl,
            Err(e) => return Ok(result_err(heap, EVAL_ERROR, e.to_string())),
        };
        // Decide the CL-style return before `exec` consumes `tl`: a definition
        // returns its own name symbol; an expression returns its value below.
        let def_name: Option<String> = match core::op(heap, tl) {
            // Every definition form but `defmethod` names itself in field 0;
            // `defmethod` names its owner there and the method in field 1.
            Some("defun") | Some("defvar") | Some("defmacro") | Some("defstruct") | Some("defenum") => {
                match core::field(heap, tl, 0) {
                    Some(Value::Path(id)) => {
                        Some(crate::types::path_from_id(heap, id).last_segment().to_string())
                    }
                    Some(Value::Symbol(id)) => Some(heap.symbol_name(id).to_string()),
                    _ => None,
                }
            }
            Some("defmethod") => match core::field(heap, tl, 1) {
                Some(Value::Symbol(id)) => Some(heap.symbol_name(id).to_string()),
                _ => None,
            },
            _ => None,
        };
        let out = self.exec(heap, tl)?;
        let result: Value = match def_name {
            Some(n) => heap.intern_symbol(&n),
            None => match out {
                Some(rt) => rt,
                // `use`/`module`, or an empty body — nothing to hand back but `()`.
                None => Value::Empty,
            },
        };
        Ok(result_ok(heap, result))
    }
}

/// Intern a name set into the `SymId`s the core bridge compares by.
pub fn intern_names(heap: &mut Heap, names: &HashSet<String>) -> HashSet<SymId> {
    names
        .iter()
        .map(|n| match heap.intern_symbol(n) {
            Value::Symbol(id) => id,
            _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
        })
        .collect()
}

/// The same for a parameter list, keeping each name's representation.
pub fn intern_params(heap: &mut Heap, params: &[(String, Repr)]) -> Vec<(SymId, Repr)> {
    params
        .iter()
        .map(|(n, r)| {
            let id = match heap.intern_symbol(n) {
                Value::Symbol(id) => id,
                _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
            };
            (id, r.clone())
        })
        .collect()
}

/// A compiled call handed back a word that does not decode to the shape its
/// declared representation promised — the two sides disagree about the ABI, so
/// this is an internal error rather than anything a program can cause.
fn crossing_mismatch(what: &str, got: Value) -> EvalError {
    EvalError::Internal(format!("compiled call returned {:?} for {} result", got, what))
}

impl MacroExpander for Interp {
    /// Expand one macro call: look `path` up in `fns` (a `defmacro` is stored
    /// there exactly like a `defun` — see [`Interp::exec`]'s `Defmacro` arm),
    /// wrap each raw (unevaluated) argument form as a datum with no
    /// conversion (this *is* the implicit quoting that makes macro arguments
    /// unevaluated data), and run it like any other call.
    ///
    /// GC-root discipline: `raw_args` are raw `Value`s on the Rust stack,
    /// invisible to the collector until `bind_macro_args` binds them into
    /// cells, so they are `push_root`ed across the whole expansion and popped
    /// on the way out. Plain LIFO — `apply` leaves the root stack exactly as
    /// it found it, because bindings are cells and need no roots pushed for
    /// them.
    fn expand_macro(&self, heap: &mut Heap, path: &Path, raw_args: Vec<Value>) -> Result<Value, String> {
        let f = self.root.borrow().get_fn(path).ok_or_else(|| format!("no such macro: {}", path))?;
        for v in &raw_args {
            heap.push_root(*v);
        }
        // A closure built while a macro body executes (check-time expansion)
        // is JIT-compiled like any other (interp-closure removal Stage 6,
        // retiring Stage 7's `jit_suppressed` guard): the island is always
        // loaded (Stage 5), so `jit_define_closure` succeeds; in an embedder
        // that skipped island loading it declines *Benign* ("island not
        // loaded") and falls back exactly as before. A macro that builds a
        // closure over a non-tier type is the only behavior change — it now
        // JITs (or hard-declines *Gap*) instead of silently interpreting —
        // and no such macro exists (macro bodies close over ordinary types).
        //
        // `bind_macro_args` validates arity and binds every parameter
        // (defaulting omitted `&optional`/`&key` args); the call-site checker
        // has already range-checked the raw count (`Checker::check_macro_arity`),
        // so any error here is a keyword/plist detail it deliberately deferred.
        let result = match self.bind_macro_args(heap, &f, &raw_args) {
            Ok(argv) => self.apply(heap, &f, argv),
            Err(e) => Err(e),
        };
        for _ in &raw_args {
            heap.pop_root();
        }
        match result {
            Ok(v) => Ok(v),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Default for Interp {
    fn default() -> Self {
        Interp::new()
    }
}

/// Whether `type_name` is the built-in `Sexpr` type, whose values are
/// represented as a heap box rather than a Rust-side value.
fn is_sexpr_type(type_name: &Path) -> bool {
    *type_name == Path::root("sexpr")
}

pub(crate) fn rt_i64(v: &Value) -> Result<i64, EvalError> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(EvalError::Internal("sexpr: expected an i64 field".into())),
    }
}

pub(crate) fn rt_f64(heap: &Heap, v: &Value) -> Result<f64, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_float(*id) => Ok(heap.float_value(*id)),
        _ => Err(EvalError::Internal("sexpr: expected an f64 field".into())),
    }
}

fn rt_char(v: &Value) -> Result<char, EvalError> {
    match v {
        Value::Char(c) => Ok(*c),
        _ => Err(EvalError::Internal("sexpr: expected a char field".into())),
    }
}

fn rt_bool(v: &Value) -> Result<bool, EvalError> {
    match v {
        Value::Bool(b) => Ok(*b),
        _ => Err(EvalError::Internal("sexpr: expected a bool field".into())),
    }
}

fn rt_str(heap: &Heap, v: &Value) -> Result<String, EvalError> {
    match v {
        Value::Str(id) => Ok(heap.string(*id).to_string()),
        _ => Err(EvalError::Internal("sexpr: expected a str field".into())),
    }
}

fn rt_bignum(heap: &Heap, v: &Value) -> Result<BigInt, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_bignum(*id) => Ok(heap.bignum_value(*id).clone()),
        _ => Err(EvalError::Internal("sexpr: expected a bignum field".into())),
    }
}

fn rt_ratio(heap: &Heap, v: &Value) -> Result<BigRational, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_ratio(*id) => Ok(heap.ratio_value(*id).clone()),
        _ => Err(EvalError::Internal("sexpr: expected a ratio field".into())),
    }
}

/// Walks a proper `Sexpr` list of `sym`s into a `Vec<SymId>` —
/// `construct_sexpr`'s `SEXPR_PATH` arm's own reverse of
/// `match_sexpr_ctor`'s path-segments-to-list direction. Read-only (no
/// allocation): every `car` must already be a `Value::Symbol` and the `cdr`
/// chain must terminate in `Value::Empty`, or the path being constructed
/// isn't well-formed.
fn sexpr_list_to_symbols(heap: &Heap, mut v: Value) -> Result<Vec<SymId>, EvalError> {
    let mut ids = Vec::new();
    loop {
        match v {
            Value::Empty => return Ok(ids),
            Value::Cons(_) => {
                match heap.car(v) {
                    Ok(Value::Symbol(id)) => ids.push(id),
                    _ => return Err(EvalError::Internal("sexpr: path segment is not a sym".into())),
                }
                v = heap.cdr(v).map_err(|e| EvalError::Internal(e.to_string()))?;
            }
            _ => return Err(EvalError::Internal("sexpr: path segments are not a proper list".into())),
        }
    }
}

/// Variant indices of `Sexpr`'s constructors (see `check::registry::sexpr_def`).
const SEXPR_NIL: usize = 0;
const SEXPR_INT: usize = 1;
const SEXPR_FLOAT: usize = 2;
const SEXPR_CHAR: usize = 3;
const SEXPR_BOOL: usize = 4;
const SEXPR_SYM: usize = 5;
const SEXPR_STR: usize = 6;
const SEXPR_CONS: usize = 7;
const SEXPR_BIGNUM: usize = 8;
const SEXPR_RATIO: usize = 9;
const SEXPR_PATH: usize = 10;

/// Evaluate a built-in `i32`/`i64` arithmetic/comparison instance method
/// (`registry::int_assoc`) — shared by both widths since `Value::Int`
/// represents every integer type uniformly as `i64`.
fn eval_int_builtin(name: &str, args: &[Value]) -> Option<Result<Value, EvalError>> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(Value::Int(a)), Some(Value::Int(b))) => (*a, *b),
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two integers", name)))),
    };
    let v = match name {
        "+" => Value::Int(a + b),
        "-" => Value::Int(a - b),
        "*" => Value::Int(a * b),
        "/" => {
            if b == 0 {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            match a.checked_div(b) {
                Some(q) => Value::Int(q),
                // `i64::MIN / -1`, whose quotient is one past `i64::MAX`. CL
                // would widen to a bignum here, but the *declared* type of
                // this expression is `i64` and a checked program cannot be
                // handed a wider result than it asked for — so it fails, with
                // `rt_i64_div`'s wording. Plain `a / b` would have trapped as
                // a Rust overflow panic instead, killing the process where
                // compiled code reported an ordinary error.
                None => {
                    return Some(Err(EvalError::Panic(format!("arithmetic overflow: {} / {}", a, b))))
                }
            }
        }
        "mod" => {
            if b == 0 {
                return Some(Err(EvalError::Panic("mod by zero".into())));
            }
            // CL `mod`: floored remainder, result takes the sign of the
            // divisor (`-7 mod 3 = 2`). `i64::MIN % -1` is mathematically 0
            // (`checked_rem` returns `None` on that overflow case).
            let r = a.checked_rem(b).unwrap_or(0);
            Value::Int(if r != 0 && (r < 0) != (b < 0) { r + b } else { r })
        }
        "<" => Value::Bool(a < b),
        "<=" => Value::Bool(a <= b),
        ">" => Value::Bool(a > b),
        ">=" => Value::Bool(a >= b),
        "=" => Value::Bool(a == b),
        "/=" => Value::Bool(a != b),
        "max" => Value::Int(a.max(b)),
        "min" => Value::Int(a.min(b)),
        "logand" => Value::Int(a & b),
        "logior" => Value::Int(a | b),
        "logxor" => Value::Int(a ^ b),
        // `(ash integer count)`: positive `count` shifts left, negative
        // shifts right (arithmetic — sign-extending), matching CL §12.10.
        // Shifts by 64+ places are clamped rather than handed to Rust's `<<`/
        // `>>` (which panic once the shift amount reaches the operand's bit
        // width): the result at that point is just `0` (left) or the sign
        // bit smeared across every bit (right).
        "ash" => Value::Int(if b >= 0 {
            if b >= 64 { 0 } else { a.wrapping_shl(b as u32) }
        } else if -b >= 64 {
            if a < 0 { -1 } else { 0 }
        } else {
            a >> (-b)
        }),
        "logbitp" => Value::Bool(if a >= 64 { b < 0 } else { (b >> a) & 1 == 1 }),
        "logtest" => Value::Bool((a & b) != 0),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

/// A unary `i32`/`i64` builtin (`lognot`/`logcount`/`integer-length`,
/// `registry::int_assoc`) — the unary counterpart of [`eval_int_builtin`]'s
/// binary ops, mirroring [`float_unary`] below.
fn int_unary(args: &[Value], f: fn(i64) -> i64) -> Result<Value, EvalError> {
    Ok(Value::Int(f(rt_i64(&args[0])?)))
}

fn expect_float(heap: &Heap, v: &Value) -> Result<f64, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_float(*id) => Ok(heap.float_value(*id)),
        other => Err(EvalError::Internal(format!("expected a Float, got {:?}", other))),
    }
}

/// `int->float` (`registry::int_assoc`): widen an `i32`/`i64` to `f64`. Both
/// widths share `Value::Int(i64)` at runtime (see `eval_int_builtin`'s doc
/// comment), so one implementation covers both.
fn int_to_float(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    match args.first() {
        Some(Value::Int(n)) => Ok(float_rt(heap, *n as f64)),
        other => Err(EvalError::Internal(format!("int->float: expected an integer, got {:?}", other))),
    }
}

/// `float->int` (`registry::float_assoc`): narrow an `f64` to an integer,
/// truncating toward zero (Rust's `as i64`, same rounding direction as CL's
/// `truncate`).
fn float_to_int(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    match args.first() {
        Some(Value::Boxed(id)) if heap.is_float(*id) => Ok(Value::Int(heap.float_value(*id) as i64)),
        other => Err(EvalError::Internal(format!("float->int: expected a float, got {:?}", other))),
    }
}

/// `int->char` (`registry::int_assoc`): a Unicode scalar value back to
/// `char`. Panics (same precedent as `car`/`cdr` on a non-`Cons` `Sexpr`) if
/// the value is outside the valid range — a surrogate code point or past
/// `U+10FFFF` — since the type system can't express "valid scalar value".
fn int_to_char(args: &[Value]) -> Result<Value, EvalError> {
    match args.first() {
        Some(Value::Int(n)) => {
            let in_u32_range = *n >= 0 && *n <= i64::from(u32::MAX);
            in_u32_range
                .then_some(*n as u32)
                .and_then(char::from_u32)
                .map(Value::Char)
                .ok_or_else(|| EvalError::Panic(format!("int->char: {} is not a valid Unicode scalar value", n)))
        }
        other => Err(EvalError::Internal(format!("int->char: expected an integer, got {:?}", other))),
    }
}

/// `try-int->char` (`registry::int_assoc`): the `Option`-returning
/// counterpart of [`int_to_char`], for `Checker::check_as`'s `try-as` —
/// same Unicode-scalar-value validity check, `None` instead of a panic.
fn try_int_to_char(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    match args.first() {
        Some(Value::Int(n)) => {
            let in_u32_range = *n >= 0 && *n <= i64::from(u32::MAX);
            let c = in_u32_range.then_some(*n as u32).and_then(char::from_u32);
            Ok(option_value(heap, c.map(Value::Char)))
        }
        other => Err(EvalError::Internal(format!("try-int->char: expected an integer, got {:?}", other))),
    }
}

/// Evaluate a built-in `f64` arithmetic/comparison instance method
/// (`registry::float_assoc`). Unlike [`eval_int_builtin`], `/` never panics on
/// a zero divisor — IEEE-754 division yields `inf`/`NaN` instead, the natural
/// float semantics (no "can't express nonzero" gap to plug). `mod`/`rem` are
/// defined in `prelude.rs` as typelisp methods (`a - b*floor|truncate(a/b)`).
fn eval_float_builtin(heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, EvalError>> {
    let (a, b) = match (args.first().map(|v| expect_float(heap, v)), args.get(1).map(|v| expect_float(heap, v))) {
        (Some(Ok(a)), Some(Ok(b))) => (a, b),
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two floats", name)))),
    };
    let v = match name {
        "+" => float_rt(heap, a + b),
        "-" => float_rt(heap, a - b),
        "*" => float_rt(heap, a * b),
        "/" => float_rt(heap, a / b),
        "<" => Value::Bool(a < b),
        "<=" => Value::Bool(a <= b),
        ">" => Value::Bool(a > b),
        ">=" => Value::Bool(a >= b),
        "=" => Value::Bool(a == b),
        "/=" => Value::Bool(a != b),
        "max" => float_rt(heap, a.max(b)),
        "min" => float_rt(heap, a.min(b)),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

fn float_unary(heap: &mut Heap, args: &[Value], f: fn(f64) -> f64) -> Result<Value, EvalError> {
    let r = f(expect_float(heap, &args[0])?);
    Ok(float_rt(heap, r))
}

fn float_expt(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let r = expect_float(heap, &args[0])?.powf(expect_float(heap, &args[1])?);
    Ok(float_rt(heap, r))
}

/// The `BigInt` behind a `bignum` value, cloned out of its heap box.
///
/// Owned rather than borrowed because every caller goes on to *allocate* the
/// result — `&mut Heap` for the allocation cannot coexist with a `&BigInt`
/// borrowed from the same heap. The clone is bounded by the operand's limb
/// count, and every caller is already doing multi-precision arithmetic on it.
fn expect_bignum(heap: &Heap, v: &Value) -> Result<BigInt, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_bignum(*id) => Ok(heap.bignum_value(*id).clone()),
        other => Err(EvalError::Internal(format!("expected a bignum, got {:?}", other))),
    }
}

/// The `BigRational` behind a `ratio` value — see [`expect_bignum`] for why
/// this is owned rather than borrowed.
fn expect_ratio(heap: &Heap, v: &Value) -> Result<BigRational, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_ratio(*id) => Ok(heap.ratio_value(*id).clone()),
        other => Err(EvalError::Internal(format!("expected a ratio, got {:?}", other))),
    }
}

/// A `bignum` value: `n` boxed onto the GC heap. The one constructor, so
/// there is exactly one runtime shape for a `bignum` — this used to be a
/// Rust-side `RtValue::Bignum(Rc<BigInt>)` that had to be copied onto the heap
/// at every boundary (a struct field, a compiled call) and copied back off on
/// the way home.
fn bignum_rt(heap: &mut Heap, n: BigInt) -> Value {
    heap.alloc_bignum(n)
}

/// A `ratio` value — the [`bignum_rt`] counterpart.
fn ratio_rt(heap: &mut Heap, r: BigRational) -> Value {
    heap.alloc_ratio(r)
}

/// A `string` value: `s` allocated onto the GC heap. The one constructor.
///
/// `Heap::alloc_string` deliberately does *not* dedupe (unlike
/// `intern_string`, which the symbol/path tables use), so two separately
/// evaluated literals with equal content get distinct `StrId`s — which is
/// what keeps `eq` a genuine identity test rather than collapsing to content
/// comparison. See [`string_identity_eq`].
pub fn str_rt(heap: &mut Heap, s: impl Into<String>) -> Value {
    heap.alloc_string(s.into())
}

/// An `f64` value: `f` boxed onto the GC heap — the [`bignum_rt`] counterpart
/// for floats, and the one constructor.
///
/// A float was the last type with *two* runtime shapes: a Rust-side
/// `RtValue::Float(f64)` while the interpreter held it, and a
/// `BoxedObj::Float` once it reached a struct field, a `Sexpr`, or compiled
/// code — with a conversion at each crossing and a standing risk that a
/// reader of one shape met the other. There is only the box now.
///
/// This does *not* make float arithmetic allocate where it did not before:
/// the compiled tier keeps floats in native registers (`binding_kind`'s float
/// kind), and the interpreter was already boxing at every boundary. It is the
/// interpreter's own locals that move onto the heap.
fn float_rt(heap: &mut Heap, f: f64) -> Value {
    heap.alloc_float(f)
}

/// Evaluate a built-in `bignum` arithmetic/comparison instance method
/// (`registry::bignum_assoc`). Core integer operations: `+ - * /` (`/`
/// truncates toward zero) and `mod` (floored, CL — sign of the divisor), each
/// panicking on a zero divisor (the type system can't express "nonzero", the
/// same precedent as `car`/`cdr` on a non-`Cons` `Sexpr`). The rest of the CL
/// integer catalog (`rem`/`abs`/`signum`/`gcd`/`lcm`/`expt`) lives in
/// `prelude.rs` as typelisp methods built from these.
fn eval_bignum_builtin(heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, EvalError>> {
    use num_integer::Integer;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => match (expect_bignum(heap, a), expect_bignum(heap, b)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Some(Err(e)),
        },
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two bignums", name)))),
    };
    let v = match name {
        "+" => bignum_rt(heap, &a + &b),
        "-" => bignum_rt(heap, &a - &b),
        "*" => bignum_rt(heap, &a * &b),
        "/" => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            bignum_rt(heap, &a / &b)
        }
        "mod" => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("mod by zero".into())));
            }
            // CL `mod`: floored remainder (sign of the divisor).
            bignum_rt(heap, a.mod_floor(&b))
        }
        "<" => Value::Bool(a < b),
        "<=" => Value::Bool(a <= b),
        ">" => Value::Bool(a > b),
        ">=" => Value::Bool(a >= b),
        "=" => Value::Bool(a == b),
        "/=" => Value::Bool(a != b),
        "max" => bignum_rt(heap, if a >= b { a } else { b }),
        "min" => bignum_rt(heap, if a <= b { a } else { b }),
        "logand" => bignum_rt(heap, &a & &b),
        "logior" => bignum_rt(heap, &a | &b),
        "logxor" => bignum_rt(heap, &a ^ &b),
        // `(ash integer count)`: `a` is the integer (receiver), `b` the shift
        // count. `BigInt`'s own `Shr` is already floor-based (arithmetic,
        // sign-extending) like `i64`'s, so this mirrors `eval_int_builtin`'s
        // `ash` with no width limit to clamp against. A shift count so large
        // it doesn't fit `i64` is astronomically implausible for any bignum
        // that fits in memory, so it's treated as "shift past every bit" —
        // `0` left, sign-extended `-1`/`0` right.
        "ash" => bignum_rt(heap, match b.to_i64() {
            Some(count) if count >= 0 => &a << (count as u64),
            Some(count) => &a >> ((-count) as u64),
            None if b.sign() == num_bigint::Sign::Minus => if a.sign() == num_bigint::Sign::Minus { BigInt::from(-1) } else { BigInt::from(0) },
            None => BigInt::from(0),
        }),
        "logbitp" => Value::Bool(match a.to_u64() {
            Some(idx) => ((&b >> idx) & BigInt::from(1)) == BigInt::from(1),
            None => b.sign() == num_bigint::Sign::Minus,
        }),
        "logtest" => Value::Bool(!(&a & &b).is_zero()),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

/// Unary `bignum` builtins (`lognot`/`logcount`/`integer-length`,
/// `registry::bignum_assoc`) — CL §12.10's "infinite two's complement"
/// reading, the arbitrary-precision counterpart of [`int_unary`].
fn bignum_unary(heap: &mut Heap, args: &[Value], name: &str) -> Result<Value, EvalError> {
    let a = expect_bignum(heap, &args[0])?;
    let v = match name {
        "lognot" => bignum_rt(heap, !&a),
        // A negative bignum's 1-bits are infinite (the sign extension), so
        // CL counts its *0*-bits instead — the same identity
        // `eval_int_builtin`'s `logcount` uses: `popcount(n) = popcount(!n)`
        // for `n < 0`, and `!n` is nonnegative whenever `n` is negative.
        "logcount" => {
            let n = if a.sign() == num_bigint::Sign::Minus { !&a } else { a.clone() };
            let count: u64 = n.magnitude().to_u32_digits().iter().map(|d| d.count_ones() as u64).sum();
            bignum_rt(heap, BigInt::from(count))
        }
        // Bits needed excluding sign: `n`'s own magnitude bit-length when
        // nonnegative, else `(-n-1)`'s (CL's own negative-integer-length
        // identity — the same one `eval_int_builtin`'s `integer-length` uses).
        "integer-length" => {
            let bits = if a.sign() == num_bigint::Sign::Minus { (-(&a) - 1u32).magnitude().bits() } else { a.magnitude().bits() };
            bignum_rt(heap, BigInt::from(bits))
        }
        _ => unreachable!(),
    };
    Ok(v)
}

/// Evaluate a built-in `ratio` arithmetic/comparison instance method
/// (`registry::ratio_assoc`). Core operations: `+ - * /` (`/` panics on a zero
/// divisor). CL's `mod`/`rem`/`expt`/`abs`/`signum` on rationals live in
/// `prelude.rs` as typelisp methods built from these plus the
/// `ratio->bignum`/`bignum->ratio` truncation pair.
fn eval_ratio_builtin(heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, EvalError>> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => match (expect_ratio(heap, a), expect_ratio(heap, b)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Some(Err(e)),
        },
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two ratios", name)))),
    };
    let v = match name {
        "+" => ratio_rt(heap, &a + &b),
        "-" => ratio_rt(heap, &a - &b),
        "*" => ratio_rt(heap, &a * &b),
        "/" => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            ratio_rt(heap, &a / &b)
        }
        "<" => Value::Bool(a < b),
        "<=" => Value::Bool(a <= b),
        ">" => Value::Bool(a > b),
        ">=" => Value::Bool(a >= b),
        "=" => Value::Bool(a == b),
        "/=" => Value::Bool(a != b),
        "max" => ratio_rt(heap, if a >= b { a } else { b }),
        "min" => ratio_rt(heap, if a <= b { a } else { b }),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

/// `int->bignum` (`registry::int_assoc`): always-exact widening.
fn int_to_bignum(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(bignum_rt(heap, BigInt::from(rt_i64(&args[0])?)))
}

/// `int->ratio` (`registry::int_assoc`): always-exact widening.
fn int_to_ratio(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(ratio_rt(heap, BigRational::from_integer(BigInt::from(rt_i64(&args[0])?))))
}

/// `bignum->int` (`registry::bignum_assoc`): narrowing, panics if the value
/// doesn't fit in an `i64` — the type system can't express "in range", same
/// precedent as `int->char`'s Unicode-scalar-value check.
fn bignum_to_int(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let n = expect_bignum(heap, &args[0])?;
    n.to_i64()
        .map(Value::Int)
        .ok_or_else(|| EvalError::Panic(format!("bignum->int: {} does not fit in an i64", n)))
}

/// `try-bignum->int` (`registry::bignum_assoc`): the `Option`-returning
/// counterpart of [`bignum_to_int`], for `Checker::check_as`'s `try-as` —
/// same "fits in an `i64`" check, `None` instead of a panic on overflow.
fn try_bignum_to_int(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let n = expect_bignum(heap, &args[0])?;
    let int = n.to_i64().map(Value::Int);
    Ok(option_value(heap, int))
}

/// `bignum->float` (`registry::bignum_assoc`): widening, possibly lossy for
/// a magnitude beyond `f64`'s 53-bit mantissa (IEEE-754 rounds to the
/// nearest representable value, same as any other narrowing-precision
/// numeric conversion).
fn bignum_to_float(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let n = expect_bignum(heap, &args[0])?;
    let f = n.to_f64().unwrap_or(f64::INFINITY.copysign(if n.sign() == num_bigint::Sign::Minus { -1.0 } else { 1.0 }));
    Ok(float_rt(heap, f))
}

/// `bignum->ratio` (`registry::bignum_assoc`): always-exact widening.
fn bignum_to_ratio(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let n = expect_bignum(heap, &args[0])?;
    Ok(ratio_rt(heap, BigRational::from_integer(n)))
}

/// `float->bignum` (`registry::float_assoc`): narrowing, truncating toward
/// zero (`f64 as i64`'s multi-precision analogue). Panics on a non-finite
/// float (`NaN`/`inf`) — there is no bignum value to truncate to, the same
/// "value outside the representable range" panic precedent as
/// `int->char`/`bignum->int`.
fn float_to_bignum(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let f = expect_float(heap, &args[0])?;
    if !f.is_finite() {
        return Err(EvalError::Panic(format!("float->bignum: {} is not finite", f)));
    }
    Ok(bignum_rt(heap, BigInt::from_f64(f.trunc()).expect("a finite float truncates to a representable BigInt")))
}

/// `float->ratio` (`registry::float_assoc`): widening and *exact* — every
/// finite `f64` is itself an exact dyadic rational (CL's `rational`, not the
/// lossy-round-trip-through-decimal `rationalize`). Panics on a non-finite
/// float, same precedent as [`float_to_bignum`].
fn float_to_ratio(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let f = expect_float(heap, &args[0])?;
    BigRational::from_float(f)
        .map(|r| ratio_rt(heap, r))
        .ok_or_else(|| EvalError::Panic(format!("float->ratio: {} is not finite", f)))
}

/// `ratio->bignum` (`registry::ratio_assoc`): narrowing, truncating toward
/// zero (CL's `truncate`) — `Ratio::to_integer` already does exactly this.
fn ratio_to_bignum(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let r = expect_ratio(heap, &args[0])?;
    Ok(bignum_rt(heap, r.to_integer()))
}

/// `ratio->float` (`registry::ratio_assoc`): widening, possibly lossy
/// (IEEE-754 rounds to the nearest representable `f64`).
fn ratio_to_float(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let r = expect_ratio(heap, &args[0])?;
    let f = r.to_f64().unwrap_or(f64::NAN);
    Ok(float_rt(heap, f))
}

/// `numerator`/`denominator` (`registry::ratio_assoc`): the reduced
/// components of a `ratio` — CL's own accessors of the same names — as
/// `bignum`. The denominator of a normalized `ratio` is always positive (see
/// `BoxedObj::Ratio`'s doc comment), matching CL's guarantee.
fn ratio_numerator(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(bignum_rt(heap, expect_ratio(heap, &args[0])?.numer().clone()))
}

fn ratio_denominator(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(bignum_rt(heap, expect_ratio(heap, &args[0])?.denom().clone()))
}

/// One step of a 64-bit xorshift generator — the bit-twiddling behind every
/// `random-state` draw. Not cryptographically secure and not expressible in
/// typelisp itself (no bitwise operators), matching `gensym`'s
/// "collision-resistant, not unforgeable" precedent for what a builtin
/// without a real entropy/hygiene API can promise.
///
/// Lives in `typelisp-rt` (with the compiled lowering,
/// `rt_random_state_next`) rather than here, and is used from both sides: a
/// draw must not depend on whether the caller happened to be compiled.
use typelisp_rt::xorshift64_step;

/// A fresh entropy seed for `make-random-state-fresh`. Lives beside the
/// compiled lowering (`rt_make_random_state_fresh`), like
/// [`xorshift64_step`].
use typelisp_rt::fresh_random_seed;

/// The box behind a `random-state` value. A *positive* `is_random_state` test,
/// so a differently-shaped box (a float, a struct) is reported rather than
/// read as a seed by the accessors below, which panic on a mismatch.
fn expect_random_state(heap: &Heap, v: &Value) -> Result<BoxId, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_random_state(*id) => Ok(*id),
        other => Err(EvalError::Internal(format!("expected a random-state, got {:?}", other))),
    }
}

/// `make-random-state-fresh`: a brand new, independently-seeded stream —
/// every `(defvar *random-state* ...)` in the prelude gets one at load time,
/// and it backs CL's `(make-random-state t)` case.
fn eval_make_random_state_fresh(heap: &mut Heap, _args: &[Value]) -> Result<Value, EvalError> {
    Ok(heap.alloc_random_state(fresh_random_seed()))
}

/// `random-state-copy`: an independent stream starting from the same point
/// `state` is at right now — CL's `(make-random-state state)` case. Later
/// draws against the copy never affect `state` (or vice versa) — distinct
/// `Rc`s over distinct `Cell`s, not a second handle to the same one.
fn eval_random_state_copy(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_random_state(heap, &args[0])?;
    let seed = heap.random_state_seed(id);
    Ok(heap.alloc_random_state(seed))
}

/// `random-state-next`: advances `state` one xorshift step and returns the
/// draw reduced into `[0, bound)`. The prelude's `random` (an ordinary
/// `&optional`-taking `defun`) is the only caller — this is the one place
/// that actually touches a `random-state`'s seed.
fn eval_random_state_next(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_random_state(heap, &args[0])?;
    let n = rt_i64(&args[1])?;
    if n <= 0 {
        return Err(EvalError::Panic(format!("random: bound must be positive, got {}", n)));
    }
    let next = xorshift64_step(heap.random_state_seed(id));
    heap.set_random_state_seed(id, next);
    Ok(Value::Int((next % n as u64) as i64))
}

/// `get-universal-time` (CLHS 25.1): seconds since 1900-01-01 00:00:00 UTC
/// (CL's epoch) — the Unix epoch offset by the well-known 2208988800s
/// between the two.
fn eval_get_universal_time(_args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Int(typelisp_rt::sys_builtin::get_universal_time()))
}

/// `get-internal-real-time` (CLHS 25.1): elapsed `internal-time-units-per-
/// second` (the prelude's `defvar`, 1_000_000 — i.e. microseconds) since an
/// arbitrary reference point fixed at first call — a monotonic
/// `std::time::Instant`, not wall-clock time, so `time`'s elapsed-time
/// measurement can't go backwards under a clock adjustment.
fn eval_get_internal_real_time(_args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Int(typelisp_rt::sys_builtin::get_internal_real_time()))
}

/// Shared tail of every scalar `print`/`println` method (`registry.rs`'s
/// per-type `"print"`/`"println"` entries): writes `text` to stdout, with a
/// trailing newline iff `newline`, and flushes immediately — a script's
/// stdout isn't a terminal when piped (e.g. into a reader at the far end of a
/// pipe, or a test harness), so it isn't line-buffered there, and a prompt
/// printed via `print` (no newline) must still be visible before the process
/// blocks reading stdin.
/// `f64` display for `print`/`println` — an integral finite value prints
/// with an explicit `.0` (matching `main.rs`'s REPL-echo `format_float`), so
/// `(println 1.0)` doesn't come out indistinguishable from `(println 1)`.
fn format_float_for_print(f: f64) -> String {
    if f.is_finite() && f == f.trunc() {
        format!("{:.1}", f)
    } else {
        f.to_string()
    }
}


fn write_stdout(text: &str, newline: bool) -> Result<Value, EvalError> {
    let mut out = std::io::stdout();
    let write_result = if newline { writeln!(out, "{}", text) } else { write!(out, "{}", text) };
    write_result.and_then(|()| out.flush()).map(|()| Value::Empty).map_err(|e| EvalError::Panic(format!("print: {}", e)))
}

/// `parse-int` (`registry.rs`'s free-function entry): a decimal `i32`
/// literal (optional leading `+`/`-`, no surrounding whitespace — plain
/// `str::parse`), `Err` on anything else rather than a panic (unlike the
/// reader's own integer literals, this reads *untrusted* runtime text).
fn eval_parse_int(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let s = expect_str(heap, &args[0])?.to_string();
    Ok(typelisp_rt::sys_builtin::parse_int(heap, &s))
}

/// `parse-float` (`registry.rs`'s free-function entry): an `f64` literal via
/// `str::parse` (accepts everything Rust's own `FromStr for f64` does,
/// including `inf`/`nan`), `Err` on anything else.
fn eval_parse_float(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let s = expect_str(heap, &args[0])?.to_string();
    Ok(typelisp_rt::sys_builtin::parse_float(heap, &s))
}

/// `read` (`registry.rs`'s free-function entry): parses exactly one `Sexpr`
/// form from `s` via the ordinary reader (`crate::read::Reader::read`) —
/// the same pipeline `typl`/the REPL use for source text, just callable at
/// runtime on a string value instead of a file/stdin. `Err` (not a panic)
/// on malformed input, e.g. an unterminated list or string — this reads
/// data the running program doesn't control.
fn eval_read(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let s = expect_str(heap, &args[0])?.to_string();
    // `typelisp_read::shim`, the same implementation `rt_read` calls: one
    // reader, one `Result` shape, whichever side of the compile boundary the
    // caller is on.
    Ok(typelisp_read::shim::read_builtin(heap, &s))
}

/// Whether `type_name` is one of the integer types — every one of which shares
/// the same catalog and the same runtime representation (`Value::Int`, an
/// `i64`, whatever width the static type claims; see `registry::int_assoc`).
fn is_int_receiver(type_name: &Path) -> bool {
    crate::types::INT_TYPE_NAMES.iter().any(|n| *type_name == Path::root(n))
}

/// [`is_int_receiver`]'s float counterpart: `f32` and `f64` are one
/// `Value::Float` (an `f64`) apart from which type labels them.
fn is_float_receiver(type_name: &Path) -> bool {
    crate::types::FLOAT_TYPE_NAMES.iter().any(|n| *type_name == Path::root(n))
}

/// Built-in (Rust-implemented) instance/static methods for nominal types that
/// have no `defmethod` body to run — currently `HashTable<K,V>`
/// (`crate::check::registry`'s `hashtable_def`).
/// Mirrors `Interp::eval_builtin` for free functions: `assoc`'s eval arm
/// tries `Interp::methods` (user `defmethod`s) first, falling back to this.
/// Argument count/types are trusted (the checker already validated them
/// against the type's `AdtDef` signatures), so arms index `args` directly
/// rather than re-checking shape. Takes `heap` (unlike most of these arms
/// need) for `sexpr`'s `eql`, which must read a boxed `Sexpr::Float`'s
/// actual value (`Heap::float_value`) to tell it apart from `eq`'s identity
/// comparison — see `sexpr_eql`'s doc comment.
/// `ret_ty` is the call site's checked return type (`assoc`/
/// the unreachable-`panic` node a bounded generic's method call leaves behind) — [`vector_get`]
/// consumes it to decode a `Vector<Sexpr>` element by its static type (see
/// [`decode_field_typed`]), and `Scope::new` reads its `V` off it.
/// `recv_ty` is the receiver argument's checked type (`args[0]`'s
/// the call site's checked type, `None` on a receiver-less static call) —
/// what `Scope<V>`'s instance methods dispatch their representation on;
/// see the `"scope"` arm. `interp` carries the `struct_types` that
/// classification reads ([`Interp::scope_is_heap`]).
/// Dispatch a built-in type's method — a `defmethod` with no typelisp body, so
/// the implementation lives here rather than in the registry.
///
/// Takes no types at all, which is the whole point: the container methods that
/// return `Option<V>` (`HashTable::get`/`remove`, `Vector::pop`, `Scope::get`)
/// used to be handed the call site's checked return type so the stored word
/// could be decoded back into a Rust-side `RtValue` by its declared type. With
/// one value world left, that decode is the identity — a stored field *is* the
/// value — so `decode_field_typed`/`option_payload_ty`/`scope_elem_ty` are gone
/// and nothing here needs to know `V`.
fn eval_builtin_method(heap: &mut Heap, type_name: &Path, method: &str, args: &[Value]) -> Option<Result<Value, EvalError>> {
    if *type_name == Path::root("hashtable") {
        return match method {
            "new" => Some(Ok(heap.alloc_hashtable())),
            "get" => Some(hashtable_get(heap, args)),
            "set" => Some(hashtable_set(heap, args)),
            "remove" => Some(hashtable_remove(heap, args)),
            "count" => Some(hashtable_count(heap, args)),
            "clear" => Some(hashtable_clear(heap, args)),
            "keys" => Some(hashtable_keys(heap, args)),
            "values" => Some(hashtable_values(heap, args)),
            "entries" => Some(hashtable_entries(heap, args)),
            _ => None,
        };
    }
    if *type_name == Path::root("vector") {
        return match method {
            // type-identity-ok: the built-in `Vector`, a root name spelled in full
            "new" => Some(Ok(heap.alloc_struct("vector".to_string(), Vec::new()))),
            "push" => Some(vector_push(heap, args)),
            "get" => Some(vector_get(heap, args)),
            "set" => Some(vector_set(heap, args)),
            "len" => Some(vector_len(heap, args)),
            "pop" => Some(vector_pop(heap, args)),
            _ => None,
        };
    }
    if *type_name == Path::root("scope") {
        // One representation: every `Scope<V>` is a `StructPayload::Frames`
        // heap box, and the element needs no encode or decode, so `V` never
        // enters into it — see this function's doc comment.
        if method == "new" {
            return Some(Ok(heap.alloc_scope()));
        }
        return match method {
            "clone-frames" => Some(scope_clone_frames_heap(heap, args)),
            "push-frame" => Some(scope_push_frame_heap(heap, args)),
            "pop-frame" => Some(scope_pop_frame_heap(heap, args)),
            "get" => Some(scope_get_heap(heap, args)),
            "set" => Some(scope_set_heap(heap, args)),
            _ => None,
        };
    }
    if *type_name == Path::root("string") {
        return match method {
            "upcase" => Some(expect_str(heap, &args[0]).map(|s| s.to_ascii_uppercase()).map(|s| str_rt(heap, s))),
            "downcase" => Some(expect_str(heap, &args[0]).map(|s| s.to_ascii_lowercase()).map(|s| str_rt(heap, s))),
            "length" => Some(string_length(heap, args)),
            "ref" => Some(string_ref(heap, args)),
            "substring" => Some(string_substring(heap, args)),
            "append" => Some(string_append(heap, args)),
            "lt" => Some(string_lt(heap, args)),
            "<" | "<=" | ">" | ">=" => Some(string_compare(heap, method, args)),
            // `eq`/`eql`: true identity (`Rc::ptr_eq` — see the string table's
            // doc comment). `equal`/`equalp`: content comparison, the
            // (case-sensitive/-insensitive) CL predicates a naive "string
            // equality" actually means — see `registry::string_assoc`'s doc
            // comment and `docs/cl-equivalence-catalog.md`'s eq/eql/equal/
            // equalp section.
            "eq" | "eql" => Some(string_identity_eq(args)),
            "equal" => Some(string_content_eq(heap, args)),
            "equalp" => Some(string_content_eqp(heap, args)),
            "print" => Some(expect_str(heap, &args[0]).and_then(|s| write_stdout(s, false))),
            "println" => Some(expect_str(heap, &args[0]).and_then(|s| write_stdout(s, true))),
            _ => None,
        };
    }
    if *type_name == Path::root("char") {
        return match method {
            "upcase" => Some(expect_char(&args[0]).map(|c| Value::Char(c.to_ascii_uppercase()))),
            "downcase" => Some(expect_char(&args[0]).map(|c| Value::Char(c.to_ascii_lowercase()))),
            "lt" => Some(char_lt(args)),
            "<" | "<=" | ">" | ">=" => Some(char_compare(method, args)),
            "alphap" => Some(expect_char(&args[0]).map(|c| Value::Bool(c.is_ascii_alphabetic()))),
            "digitp" => Some(expect_char(&args[0]).map(|c| Value::Bool(c.is_ascii_digit()))),
            // `eq`/`eql`/`equal` all coincide (immediate scalar, and CL's
            // own `equal` on characters is defined to be `eql`); `equalp`
            // is case-insensitive (see `registry::char_assoc`'s doc comment).
            "eq" | "eql" | "equal" => Some(char_eq(args)),
            "equalp" => Some(char_eqp(args)),
            "char->string" => Some(expect_char(&args[0]).map(|c| c.to_string()).map(|s| str_rt(heap, s))),
            "char->int" => Some(char_to_int(args)),
            "print" => Some(expect_char(&args[0]).and_then(|c| write_stdout(&c.to_string(), false))),
            "println" => Some(expect_char(&args[0]).and_then(|c| write_stdout(&c.to_string(), true))),
            _ => None,
        };
    }
    if is_int_receiver(type_name) {
        return match method {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" | "max" | "min" | "logand"
            | "logior" | "logxor" | "ash" | "logbitp" | "logtest" => eval_int_builtin(method, args),
            "lognot" => Some(int_unary(args, |n| !n)),
            "logcount" => Some(int_unary(args, |n| if n >= 0 { n.count_ones() as i64 } else { (!n).count_ones() as i64 })),
            "integer-length" => Some(int_unary(args, |n| {
                if n >= 0 { (64 - n.leading_zeros()) as i64 } else { (64 - (!n).leading_zeros()) as i64 }
            })),
            // `eq`/`eql`/`equal`/`equalp` are all registered as aliases for
            // `=` (see `registry::int_assoc`'s doc comment for why every one
            // of these four is meaningful to register even though none can
            // diverge from `=` here).
            "eq" | "eql" | "equal" | "equalp" => eval_int_builtin("=", args),
            "int->float" => Some(int_to_float(heap, args)),
            "int->char" => Some(int_to_char(args)),
            "try-int->char" => Some(try_int_to_char(heap, args)),
            "int->bignum" => Some(int_to_bignum(heap, args)),
            "int->ratio" => Some(int_to_ratio(heap, args)),
            "print" => Some(rt_i64(&args[0]).and_then(|n| write_stdout(&n.to_string(), false))),
            "println" => Some(rt_i64(&args[0]).and_then(|n| write_stdout(&n.to_string(), true))),
            _ => None,
        };
    }
    if is_float_receiver(type_name) {
        return match method {
            "+" | "-" | "*" | "/" | "<" | "<=" | ">" | ">=" | "=" | "/=" | "max" | "min" => {
                eval_float_builtin(heap, method, args)
            }
            "eq" | "eql" | "equal" | "equalp" => eval_float_builtin(heap, "=", args),
            "expt" => Some(float_expt(heap, args)),
            "sqrt" => Some(float_unary(heap, args, f64::sqrt)),
            "floor" => Some(float_unary(heap, args, f64::floor)),
            "ceiling" => Some(float_unary(heap, args, f64::ceil)),
            "round" => Some(float_unary(heap, args, f64::round)),
            "truncate" => Some(float_unary(heap, args, f64::trunc)),
            "sin" => Some(float_unary(heap, args, f64::sin)),
            "cos" => Some(float_unary(heap, args, f64::cos)),
            "tan" => Some(float_unary(heap, args, f64::tan)),
            "asin" => Some(float_unary(heap, args, f64::asin)),
            "acos" => Some(float_unary(heap, args, f64::acos)),
            "atan" => Some(float_unary(heap, args, f64::atan)),
            "sinh" => Some(float_unary(heap, args, f64::sinh)),
            "cosh" => Some(float_unary(heap, args, f64::cosh)),
            "tanh" => Some(float_unary(heap, args, f64::tanh)),
            "asinh" => Some(float_unary(heap, args, f64::asinh)),
            "acosh" => Some(float_unary(heap, args, f64::acosh)),
            "atanh" => Some(float_unary(heap, args, f64::atanh)),
            "exp" => Some(float_unary(heap, args, f64::exp)),
            "log" => Some(float_unary(heap, args, f64::ln)),
            "float->int" => Some(float_to_int(heap, args)),
            "float->bignum" => Some(float_to_bignum(heap, args)),
            "float->ratio" => Some(float_to_ratio(heap, args)),
            "print" => Some(rt_f64(heap, &args[0]).and_then(|f| write_stdout(&format_float_for_print(f), false))),
            "println" => Some(rt_f64(heap, &args[0]).and_then(|f| write_stdout(&format_float_for_print(f), true))),
            _ => None,
        };
    }
    if *type_name == Path::root("bignum") {
        return match method {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" | "max" | "min" | "logand"
            | "logior" | "logxor" | "ash" | "logbitp" | "logtest" => eval_bignum_builtin(heap, method, args),
            "lognot" | "logcount" | "integer-length" => Some(bignum_unary(heap, args, method)),
            "eq" | "eql" | "equal" | "equalp" => eval_bignum_builtin(heap, "=", args),
            "bignum->int" => Some(bignum_to_int(heap, args)),
            "try-bignum->int" => Some(try_bignum_to_int(heap, args)),
            "bignum->float" => Some(bignum_to_float(heap, args)),
            "bignum->ratio" => Some(bignum_to_ratio(heap, args)),
            "print" => Some(expect_bignum(heap, &args[0]).and_then(|n| write_stdout(&n.to_string(), false))),
            "println" => Some(expect_bignum(heap, &args[0]).and_then(|n| write_stdout(&n.to_string(), true))),
            _ => None,
        };
    }
    if *type_name == Path::root("ratio") {
        return match method {
            "+" | "-" | "*" | "/" | "<" | "<=" | ">" | ">=" | "=" | "/=" | "max" | "min" => {
                eval_ratio_builtin(heap, method, args)
            }
            "eq" | "eql" | "equal" | "equalp" => eval_ratio_builtin(heap, "=", args),
            "ratio->bignum" => Some(ratio_to_bignum(heap, args)),
            "ratio->float" => Some(ratio_to_float(heap, args)),
            "numerator" => Some(ratio_numerator(heap, args)),
            "denominator" => Some(ratio_denominator(heap, args)),
            "print" => Some(expect_ratio(heap, &args[0]).and_then(|r| write_stdout(&format!("{}/{}", r.numer(), r.denom()), false))),
            "println" => Some(expect_ratio(heap, &args[0]).and_then(|r| write_stdout(&format!("{}/{}", r.numer(), r.denom()), true))),
            _ => None,
        };
    }
    if *type_name == Path::root("bool") {
        return match method {
            "eq" | "eql" | "equal" | "equalp" => Some(bool_eq(args)),
            "print" => Some(expect_bool(&args[0]).and_then(|b| write_stdout(&b.to_string(), false))),
            "println" => Some(expect_bool(&args[0]).and_then(|b| write_stdout(&b.to_string(), true))),
            _ => None,
        };
    }
    if *type_name == Path::root("symbol") {
        // A `Symbol` value shares the `Value::Symbol(id)` carrier of a
        // `Sexpr::Sym`, so `eq`/`eql` reuse the `Sexpr` comparisons (interned
        // id identity — same name => same id => `eq`).
        return match method {
            "eq" => Some(sexpr_eq(heap, args)),
            "eql" => Some(sexpr_eql(heap, args)),
            _ => None,
        };
    }
    if *type_name == Path::root("sexpr") {
        return match method {
            // `eq`/`eql` now diverge, as anticipated by this arm's own prior
            // history (see `sexpr_eql`'s doc comment): `Sexpr::Float` became
            // heap-boxed (`Value::Boxed`, see `BoxedObj`) for the `Sexpr`/
            // `RtValue` unification plan, the "boxed numeric representation"
            // this comment used to say didn't exist yet. `equal`/`equalp`
            // are `prelude.rs` free functions (structural recursion via
            // `match`), not registered here, the same as `length`/`append`
            // for `Sexpr` lists.
            "eq" => Some(sexpr_eq(heap, args)),
            "eql" => Some(sexpr_eql(heap, args)),
            _ => None,
        };
    }
    eval_llvm_builtin_method(heap, type_name, method, args)
}


/// What a session has defined since it started recording.
struct Recording {
    /// The checker signature as of the moment recording started — the "before"
    /// side of the delta `(dump ...)` writes.
    baseline: crate::dump::RegistrySignature,
    /// The definitions, in the order they were executed, each rooted.
    forms: Vec<Value>,
    /// How many globals had compiled slots before the session began; the ones
    /// past this are the session's own.
    globals_before: usize,
}

/// Everything the interpreter needs from the LLVM backend, as plain `fn`
/// pointers.
///
/// A hook rather than direct calls because all four are the *backend*
/// (`crate::compile`), and this file is the front end. Inverting the
/// reference is what lets the front end stop naming `inkwell` at all — and,
/// eventually, become a crate that does not depend on the one holding LLVM
/// (`docs/dev/TODO.md`).
///
/// Plain `fn` pointers rather than a trait object for the reason
/// [`typelisp_print::runtime::PrintHooks`] uses them: these are only ever
/// called Rust-side, never emitted as a call by the compiler, so there is no
/// ABI to pin down and `Result`/`String` cross directly.
#[derive(Clone, Copy)]
pub struct Backend {
    /// The `llvm-*`/native-scope builtins the compiler island calls to emit
    /// IR. `None` means "not one of mine".
    pub llvm_builtin: fn(&mut Heap, &Path, &str, &[Value]) -> Option<Result<Value, EvalError>>,
    /// Whether `handle` is a live entry in the backend's handle registry —
    /// the one question [`Interp::decode_compiled_return`] asks about a
    /// `Repr::Handle` result it is otherwise passing straight through.
    pub handle_is_live: fn(i64) -> bool,
    /// `(compile name)`.
    pub compile_function: fn(&Interp, &mut Heap, &crate::CompileTarget) -> Result<Value, EvalError>,
    /// `(compile-file source output)`.
    pub compile_file: fn(&str, &str) -> Result<(), String>,
    /// `(dump path)`.
    pub dump_image: fn(&Interp, &mut Heap, &str) -> Result<(), String>,
}

thread_local! {
    /// Installed by [`crate::compile::install_llvm_backend`], which
    /// `compiler::load_aot` calls — loading the island is what makes any of
    /// this reachable in the first place.
    ///
    /// Thread-local for the reason the active heap is: `cargo test` drives one
    /// compiler per thread, many per process.
    static BACKEND: std::cell::Cell<Option<Backend>> = const { std::cell::Cell::new(None) };
}

/// Registers the LLVM backend for this thread.
pub fn set_backend(backend: Backend) {
    BACKEND.with(|cell| cell.set(Some(backend)));
}

/// The registered backend, or an error naming what needed it.
///
/// No backend at all is an invariant break rather than a program error: an
/// `Interp` built without loading the island has no business reaching any of
/// this, and gets told so rather than a quietly missing result.
fn backend(who: &str) -> Result<Backend, EvalError> {
    BACKEND.with(|cell| cell.get()).ok_or_else(|| {
        EvalError::Internal(format!("{} needs the LLVM backend, and none is registered on this thread", who))
    })
}

/// The registered backend's `(compile name)`, for the evaluator's own
/// `compile` special form.
pub(crate) fn backend_compile_function(
) -> Result<fn(&Interp, &mut Heap, &crate::CompileTarget) -> Result<Value, EvalError>, EvalError> {
    Ok(backend("compile")?.compile_function)
}

/// Runs one `llvm-*`/native-scope builtin through the registered backend.
///
/// `None` from the backend means "not one of mine", and falls through to the
/// caller's own `None`.
fn eval_llvm_builtin_method(
    heap: &mut Heap,
    type_name: &Path,
    method: &str,
    args: &[Value],
) -> Option<Result<Value, EvalError>> {
    match backend(&format!("`{}::{}`", type_name, method)) {
        Ok(b) => (b.llvm_builtin)(heap, type_name, method, args),
        Err(e) => Some(Err(e)),
    }
}

/// The single canonical `"type-path::method"` string for a method — `Path`'s
/// own `Display` for `type_name` (its full `::`-joined path, never just its
/// local segment — see [`Interp::method_key`]'s doc comment for why), `"::"`,
/// then `method`. This is the LLVM-visible name a method's own compiled
/// function is declared/looked-up under — exactly the literal string a
/// standalone `(compile "type-path::method")` call uses as its
/// `internal_name` ([`Interp::compile_function`]'s own
/// `crate::compile::driver::add_compiled_function(&self, heap, module.clone(), name, name)` call,
/// where `name` is that literal user-typed string) — but also every other
/// place this crate needs the same "which method" identity as plain text: a
/// `NoSuchFunction` error, [`CallEdge::Method`]'s own SCC graph node name.
/// One shared helper keeps all of them in lockstep rather than each
/// re-deriving the same format independently. Also relied on by
/// `compiler.rs`'s `compile-assoc` (looking the same name back up via
/// `get-function` — see that function's doc comment).
pub fn method_link_name(type_name: &Path, method: &str) -> String {
    format!("{}::{}", type_name, method)
}



/// Builds an enum value (`Option`/`Result`/user `defenum`) for `type_name`'s
/// `variant`, from already-evaluated `fields` — the encode-direction
/// counterpart of `match_pattern`'s boxed-enum decode.
///
/// Every enum value is a GC-heap `BoxedObj::Enum`. There is no second
/// representation: this used to fall back to a Rust-side `RtValue::Data`
/// whenever a field held something the heap could not carry — an LLVM handle,
/// a native `Scope<V>`, a built-in function value, a `random-state` — and each
/// of those has since been given a heap form, so nothing can fail the
/// conversion any more. A failure here is therefore an interpreter bug, not a
/// tier decision, and is reported as one.
///
/// Losing that fallback is what makes an enum's representation a *static*
/// property again. While it existed, the tier was decided from the value in
/// hand, so `is_heap_repr_ty` had to predict the same answer from the type
/// alone (`enum_fields_representable`) and the two could — and did — disagree
/// without anything observable breaking, which is a bad place to be.
fn build_enum_value(heap: &mut Heap, type_name: Path, variant: usize, fields: Vec<Value>) -> Value {
    alloc_typed_enum(heap, &type_name, variant, fields)
}

impl Interp {
    /// The `stream-*` / `file-*` built-ins: a thin adapter over
    /// [`typelisp_rt::stream_builtin::stream_builtin`], which is
    /// the implementation — and is also what the compiled `rt_stream_*` shims
    /// call, so an interpreted and a compiled call to `read-char` are the same
    /// code reading the same stream table.
    ///
    /// Every one of those is deliberately dumb. The CL-shaped surface —
    /// `open`'s `&key` arguments, the `with-...` macros, `read-line`'s
    /// end-of-input convention, and every composite stream — is written in
    /// typelisp on top of the `Stream`/`InputStream`/`OutputStream` traits,
    /// where a default method body can express it once for all
    /// implementations. Nothing down there knows those traits exist.
    ///
    /// Fallible operations return `Result<_, FileError>` values; the `Err`
    /// arm here is something else entirely — an argument of the wrong shape,
    /// which the checker rules out, so it can only mean an internal bug.
    fn eval_stream_builtin(
        &self,
        heap: &mut Heap,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, EvalError>> {
        Some(match typelisp_rt::stream_builtin::stream_builtin(heap, name, args)? {
            Ok(v) => Ok(v),
            Err(e) => Err(EvalError::Internal(e)),
        })
    }
}

/// `Some(v)`/`None`, matching `option_def`'s variant order (`some` = 0,
/// `none` = 1).
fn option_value(heap: &mut Heap, v: Option<Value>) -> Value {
    let (variant, fields) = match v {
        Some(x) => (0, vec![x]),
        None => (1, vec![]),
    };
    build_enum_value(heap, Path::root("option"), variant, fields)
}

/// `Ok(v)`, matching `result_def`'s variant order (`ok` = 0, `err` = 1).
fn result_ok(heap: &mut Heap, v: Value) -> Value {
    build_enum_value(heap, Path::root("result"), 0, vec![v])
}


/// `Err(<ErrType>(msg))` — wraps `msg` in the concrete error type of the
/// failing built-in (`registry::builtin_error_defs`: `parseinterror`,
/// `parsefloaterror`, `readerror`, `evalerror`), whose single variant carries
/// exactly that string, before wrapping *that* in `Result`'s `err` variant.
/// `err_type` must name one of those four — its variant index is 0, the only
/// one each has.
fn result_err(heap: &mut Heap, err_type: &str, msg: String) -> Value {
    let msg_val = str_rt(heap, msg);
    let err_val = build_enum_value(heap, Path::root(err_type), 0, vec![msg_val]);
    build_enum_value(heap, Path::root("result"), 1, vec![err_val])
}

/// Rejects a `HashTable<K,V>` key argument before it ever reaches
/// `Heap::hashtable_get`/`_set`/`_remove` — those panic (a hard internal-
/// invariant trap, the same convention every other `Heap` accessor uses) on
/// an unhashable `Value` shape, since the type checker can't express a
/// "hashable" bound (no traits in this language) and so can't rule out e.g.
/// `HashTable<f64,T>` at check time. Catching it here instead, as an
/// `EvalError::Panic`, keeps a user mistake (an unsupported `K`) a catchable
/// evaluation error rather than an uncatchable Rust panic unwinding out of
/// `Heap` — mirroring the pre-unification `HashKey::from_rtvalue`'s contract.
fn expect_hashable_key(v: &Value) -> Result<(), EvalError> {
    match v {
        Value::Int(_) | Value::Bool(_) | Value::Char(_) | Value::Str(_) => Ok(()),
        other => Err(EvalError::Panic(format!("HashTable: unsupported key type {:?}", other))),
    }
}

fn hashtable_get(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    expect_hashable_key(&args[1])?;
    let found = heap.hashtable_get(id, args[1]);
    Ok(option_value(heap, found))
}

fn hashtable_set(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    expect_hashable_key(&args[1])?;
    let key = args[1];
    let val = args[2];
    heap.hashtable_set(id, key, val);
    Ok(Value::Empty)
}

fn hashtable_remove(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    expect_hashable_key(&args[1])?;
    let removed = heap.hashtable_remove(id, args[1]);
    Ok(option_value(heap, removed))
}

fn hashtable_count(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(Value::Int(heap.hashtable_count(id) as i64))
}

fn hashtable_clear(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.hashtable_clear(id);
    Ok(Value::Empty)
}

/// Builds a `Vector<T>` runtime value (the boxed-struct variable-length
/// `"vector"` representation, see `vector_def`'s doc comment) directly out of
/// already-encoded `mem::Value` fields — the shared tail of `hashtable_keys`/
/// `hashtable_values`/`hashtable_entries`, whose fields come straight from
/// `Heap::hashtable_pairs` and so need no conversion on the way out.
fn vector_of_raw(heap: &mut Heap, fields: Vec<Value>) -> Value {
    // type-identity-ok: the built-in `Vector`, a root name spelled in full
    heap.alloc_struct("vector".to_string(), fields)
}

fn hashtable_keys(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let fields = heap.hashtable_pairs(id).into_iter().map(|(k, _)| k).collect();
    Ok(vector_of_raw(heap, fields))
}

fn hashtable_values(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let fields = heap.hashtable_pairs(id).into_iter().map(|(_, v)| v).collect();
    Ok(vector_of_raw(heap, fields))
}

/// Each entry becomes a `cons-cell<K,V>` (`prelude.rs`'s generic `car`/`cdr`
/// `defstruct`, the boxed-struct `"cons-cell"` representation, matching how
/// `Checker::check_construct` would build one from typelisp source; built
/// directly here since a defstruct instance is just tagged field data, not
/// something only the checker/prelude can construct) wrapping the pair's
/// already-encoded key/value `mem::Value`s straight from
/// `Heap::hashtable_pairs`.
fn hashtable_entries(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let fields = heap
        .hashtable_pairs(id)
        .into_iter()
        // type-identity-ok: the built-in `cons-cell`, a root name spelled in full
        .map(|(k, v)| heap.alloc_struct("cons-cell".to_string(), vec![k, v]))
        .collect();
    Ok(vector_of_raw(heap, fields))
}

/// The `BoxId` behind a `defstruct`/`Vector<T>`/`cons-cell<K,V>` instance —
/// they are ordinary boxed structs rather than a representation of their own.
/// Doesn't itself check `Heap::is_struct` (a
/// mismatched-kind `BoxId` — e.g. a boxed float reaching here — is caught by
/// the panic in whichever `Heap` struct accessor the caller goes on to call,
/// the same internal-invariant-trap convention `Heap::struct_field` etc.
/// already use).
pub fn expect_struct_box(v: &Value) -> Result<BoxId, EvalError> {
    match v {
        Value::Boxed(id) => Ok(*id),
        other => Err(EvalError::Internal(format!("expected a boxed struct, got {:?}", other))),
    }
}

/// A `Vector<T>` index, rejected the same way an index past the end is.
///
/// A negative index used to report itself in a shape of its own (`Vector:
/// invalid index Int(-1)`, a Rust `Debug` rendering leaking into a user-facing
/// message). It is an out-of-bounds access like any other, and compiled code —
/// where the index arrives as a bare `i64` with no `Value` around it — has no
/// way to say it differently anyway; `typelisp_rt::checked_field_index` is the
/// other half, and `tests/runtime_error_parity_test.rs` holds the two together.
/// A non-`Int` here is a checker failure rather than a program error.
fn expect_int_index(v: &Value) -> Result<usize, EvalError> {
    match v {
        Value::Int(n) if *n >= 0 => Ok(*n as usize),
        Value::Int(n) => Err(EvalError::Panic(format!("Vector: index {} out of bounds", n))),
        other => Err(EvalError::Internal(format!("Vector: index is not an integer: {:?}", other))),
    }
}

fn vector_push(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.struct_push_field(id, args[1]);
    Ok(Value::Empty)
}

/// `ret_ty` is the call site's checked return type — always the concrete
/// element type post-monomorphization (`Vector<Sexpr>`'s `get` returns
/// `Sexpr` there), so the element decode is fully type-directed; see
/// [`decode_field_typed`].
fn vector_get(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let i = expect_int_index(&args[1])?;
    if i >= heap.struct_field_count(id) {
        return Err(EvalError::Panic(format!("Vector: index {} out of bounds", i)));
    }
    Ok(heap.struct_field(id, i))
}

fn vector_set(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let i = expect_int_index(&args[1])?;
    if i >= heap.struct_field_count(id) {
        return Err(EvalError::Panic(format!("Vector: index {} out of bounds", i)));
    }
    heap.struct_set_field(id, i, args[2]);
    Ok(Value::Empty)
}

fn vector_len(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(Value::Int(heap.struct_field_count(id) as i64))
}

/// Unlike [`vector_get`]/[`vector_set`] (which panic out of range), `pop`
/// returns `Option<T>` — an empty vector is a legitimate `None`, not a
/// bounds violation — the same shape [`hashtable_remove`] uses for its own
/// "might not be there" result. `ret_ty` is the call site's checked
/// `Option<T>` return type; [`option_payload_ty`] extracts `T` for
/// [`decode_field_typed`]'s type-directed decode of the popped element.
fn vector_pop(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let popped = heap.struct_pop_field(id);
    Ok(option_value(heap, popped))
}


/// The printer's [`typelisp_print::runtime::PrintHooks`] pointing at the
/// interpreter — the two program facts and the two sets of control
/// variables that `typelisp-print` cannot read off the heap.
///
/// Plain `fn` pointers, so each one reaches the running `Interp` through
/// [`ACTIVE_INTERP`] rather than by capture. That is the same thread-local
/// [`rt_apply_interpreted`] already uses, refreshed at the point of use for
/// the same reason: a nested `Interp` (`compile-file` builds its own) leaves
/// the slot pointing at a finished one otherwise.
const INTERP_PRINT_HOOKS: typelisp_print::runtime::PrintHooks = typelisp_print::runtime::PrintHooks {
    enum_variant_name: |type_key, variant| with_active_interp(|i| i.enum_variant_name(type_key, variant))?,
    print_object: |heap, v, escape| match with_active_interp(|i| i.print_object(heap, v, escape)) {
        Some(r) => r,
        // No interpreter registered: the printer is running under a bare
        // program (a unit test), where no type can have a `print-object`
        // method because no program defined one.
        None => Ok(None),
    },
    opts: |heap| with_active_interp(|i| i.pretty_opts(heap)).unwrap_or_default(),
    limits: |heap| with_active_interp(|i| i.print_limits(heap)).unwrap_or_default(),
};

/// Runs `f` against this thread's registered `Interp`, or `None` when there
/// is none.
///
/// # Safety of the raw pointer
///
/// The slot is written immediately before every use (see
/// [`Interp::install_print_hooks`] and [`Interp::enter_compiled`]) by an
/// `&self` method, so the `Interp` it names is on the stack below this call
/// for the whole of `f`.
pub fn with_active_interp<T>(f: impl FnOnce(&Interp) -> T) -> Option<T> {
    let ptr = ACTIVE_INTERP.with(|cell| cell.get());
    if ptr.is_null() {
        return None;
    }
    Some(f(unsafe { &*ptr }))
}


thread_local! {
    /// The interpreter [`rt_apply_interpreted`] re-enters — registered by
    /// [`Interp::enter_compiled`] alongside the active heap, and thread-local
    /// for the identical reason (`cargo test` drives one `Interp` per
    /// thread, many per process).
    static ACTIVE_INTERP: std::cell::Cell<*const Interp> = const { std::cell::Cell::new(std::ptr::null()) };
}

/// `typelisp_rt::rt_apply_any`'s interpreter half: compiled code has reached
/// an apply site whose callee is *not* a compiled closure, so the call has to
/// finish in the tree-walking evaluator.
///
/// An error from the callee *unwinds* rather than returning: there is a
/// compiled frame between here and any Rust caller that could handle a
/// `Result`, and it has no way to carry one. Unwinding is how the error gets
/// past that frame to [`crate::eval::crossing::catch_compiled_panic`], which puts it
/// back together — so a `(panic ...)` in an interpreted callback reached from
/// compiled code is as recoverable as one anywhere else.
///
/// A *missing interpreter* still aborts: that is a registration bug in
/// [`Interp::enter_compiled`], not something a program can provoke.
///
/// # Safety
///
/// `args` must point to `argc` valid `i64`s, and both a `Heap`
/// ([`typelisp_rt::set_active_heap`]) and an `Interp`
/// ([`Interp::enter_compiled`]) must be registered on this thread. Every
/// frame between here and the catching boundary must tolerate being unwound
/// through, which is why this and `rt_apply_any` are `extern "C-unwind"`.
unsafe extern "C-unwind" fn rt_apply_interpreted(closure: i64, args: *const i64, argc: u32) -> i64 {
    let interp = ACTIVE_INTERP.with(|cell| cell.get());
    if interp.is_null() {
        typelisp_rt::fatal("rt_apply_any: no interpreter is registered on this thread");
    }
    let interp = &*interp;
    let heap = typelisp_rt::active_heap();
    let argv = std::slice::from_raw_parts(args, argc as usize);
    match interp.apply_interpreted(heap, closure, argv) {
        Ok(w) => w,
        Err(e) => crate::eval::crossing::unwind_interpreted_failure(e),
    }
}

/// `typelisp_rt::rt_dyn_call`'s interpreter half: a `:dyn` call site found
/// its vtable slot empty, so the implementation behind it is an ordinary
/// interpreted method. Hands back the closure that method reifies to, which
/// the caller then applies through [`rt_apply_interpreted`] — the same path
/// any other interpreted callee takes out of compiled code.
///
/// Most failures here really are compiler bugs — a slot with no
/// `(type, method)` behind it, or a method that no longer resolves — but
/// reifying the closure allocates, so heap exhaustion reaches this arm too,
/// and that a program can provoke. So errors unwind rather than abort, the
/// same way [`rt_apply_interpreted`]'s do; an internal one still surfaces
/// intact as `EvalError::Internal` rather than being flattened.
///
/// # Safety
///
/// Both a `Heap` and an `Interp` must be registered on this thread, and every
/// frame out to the catching boundary must tolerate being unwound through.
unsafe extern "C-unwind" fn rt_dyn_slot_closure(vtable: u32, slot: u32) -> i64 {
    let interp = ACTIVE_INTERP.with(|cell| cell.get());
    if interp.is_null() {
        typelisp_rt::fatal("rt_dyn_call: no interpreter is registered on this thread");
    }
    let interp = &*interp;
    let heap = typelisp_rt::active_heap();
    match interp.dyn_slot_closure(heap, vtable, slot) {
        Ok(w) => w,
        Err(e) => crate::eval::crossing::unwind_interpreted_failure(e),
    }
}


// The `Scope<V>`-with-heap-repr-`V` counterparts of the native scope
// helpers above (unification Stage 8) — same six-method surface, backed by
// `Heap`'s `StructPayload::Frames` representation instead of `Rc` frames.
// Which family a call lands in is decided statically, from the receiver's
// checked `Scope<V>` type ([`Interp::scope_is_heap`]) — never from the
// receiver value's shape. `V` being heap-repr means every stored/returned
// element is a heap value by construction, so — unlike `hashtable_get`,
// whose `V` can be anything — no typed decode is needed on the way out and
// a non-`Sexpr` element on the way in is an internal-invariant trap.

pub fn scope_clone_frames_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(heap.scope_clone_frames(id))
}

pub fn scope_push_frame_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.scope_push_frame(id);
    Ok(Value::Empty)
}

pub fn scope_pop_frame_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.scope_pop_frame(id);
    Ok(Value::Empty)
}

pub(crate) fn scope_get_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let found = heap.scope_get(id, name);
    Ok(option_value(heap, found))
}

/// `Heap::scope_set` panics on an empty frame stack (the mem layer's
/// internal-invariant-trap convention); every frame *is* poppable from
/// typelisp (`pop-frame`), so the guard runs here first and reports the
/// same `EvalError` the native `scope_set` does — the `expect_hashable_key`
/// precedent of keeping a user-reachable condition a catchable evaluation
/// error rather than a Rust panic.
pub(crate) fn scope_set_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    scope_store(heap, args, args[2])
}

/// `set` for a value that is already in its stored form — the `rt_llvm_call`
/// path, where the element arrives as the very word compiled code holds and
/// there is no value to encode.
pub fn scope_set_raw(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    scope_store(heap, args, args[2])
}

/// The shared tail of both `set` paths.
///
/// `Heap::scope_set` panics on an empty frame stack (the mem layer's
/// internal-invariant-trap convention); every frame *is* poppable from
/// typelisp (`pop-frame`), so the guard runs here first and reports a
/// catchable `EvalError` instead — the `expect_hashable_key` precedent of
/// keeping a user-reachable condition an evaluation error rather than a Rust
/// panic. That matters doubly on the `rt_llvm_call` path: a panic unwinding
/// out of an `extern "C"` shim aborts the process, losing even the
/// `rt_llvm_fatal` diagnostic.
fn scope_store(heap: &mut Heap, args: &[Value], v: Value) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let name = expect_str(heap, &args[1])?.to_string();
    if heap.scope_frame_count(id) == 0 {
        return Err(EvalError::Internal("Scope::set: no frame to write into".into()));
    }
    heap.scope_set(id, &name, v);
    Ok(Value::Empty)
}

/// Resolve an `i32` index against a sequence's current length: out of range
/// (including negative) is `None`, the caller turns that into a panic — the
/// type system can't express the bound, the same "runtime panic for what
/// types can't catch" precedent as `car`/`cdr` on a non-`Cons` `Sexpr`. Used
/// by `string::ref`'s bounds check.
fn checked_index(i: i64, len: usize) -> Option<usize> {
    if i >= 0 && (i as usize) < len { Some(i as usize) } else { None }
}

/// The `&str` behind a `string` value, borrowed from the heap it lives in.
///
/// Borrowed rather than owned (unlike [`expect_bignum`]) because most callers
/// only read it; the ones that go on to build a new string end the borrow
/// with an explicit `.to_string()` first, so the copy is visible at the site
/// that needs it rather than paid by every caller.
pub fn expect_str<'h>(heap: &'h Heap, v: &Value) -> Result<&'h str, EvalError> {
    match v {
        Value::Str(id) => Ok(heap.string(*id)),
        other => Err(EvalError::Internal(format!("expected a Str, got {:?}", other))),
    }
}

fn expect_char(v: &Value) -> Result<char, EvalError> {
    match v {
        Value::Char(c) => Ok(*c),
        other => Err(EvalError::Internal(format!("expected a Char, got {:?}", other))),
    }
}

/// `char->int` (`registry::char_assoc`): a `char`'s Unicode scalar value as
/// `i32` (uniformly `Value::Int(i64)` at runtime — see
/// `eval_int_builtin`'s doc comment). Always succeeds — every `char` is
/// already a valid scalar value, unlike `int->char`'s reverse direction.
fn char_to_int(args: &[Value]) -> Result<Value, EvalError> {
    expect_char(&args[0]).map(|c| Value::Int(c as i64))
}

fn string_length(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Int(expect_str(heap, &args[0])?.chars().count() as i64))
}

fn string_ref(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let chars: Vec<char> = expect_str(heap, &args[0])?.chars().collect();
    let i = rt_i64(&args[1])?;
    match checked_index(i, chars.len()) {
        Some(idx) => Ok(Value::Char(chars[idx])),
        None => Err(EvalError::Panic(format!("ref: index {} out of range (length {})", i, chars.len()))),
    }
}

fn string_substring(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let chars: Vec<char> = expect_str(heap, &args[0])?.chars().collect();
    let start = rt_i64(&args[1])?;
    let end = rt_i64(&args[2])?;
    let len = chars.len() as i64;
    if start < 0 || end > len || start > end {
        return Err(EvalError::Panic(format!("substring: invalid range {}..{} (length {})", start, end, len)));
    }
    let s: String = chars[start as usize..end as usize].iter().collect();
    Ok(str_rt(heap, s))
}

fn string_append(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let joined = format!("{}{}", expect_str(heap, &args[0])?, expect_str(heap, &args[1])?);
    Ok(str_rt(heap, joined))
}

/// True CL identity for `string` — the same heap string, not merely equal
/// content (that's [`string_content_eq`]/[`string_content_eqp`] instead — CL's
/// `eq`/`eql` never do structural string comparison, only `equal`/`equalp`
/// do).
///
/// `StrId` equality is what makes this meaningful, and it is meaningful only
/// because `Heap::alloc_string` does not dedupe: two separately evaluated
/// literals with equal content get distinct ids, so `(eq "abc" "abc")` stays
/// false while a value threaded through a binding, a call, a `cons` cell, or
/// a struct field stays `eq` to itself. This replaces an `Rc::ptr_eq` on a
/// Rust-side `Rc<str>`, which had the same identity semantics for the first
/// three but *lost* it through a `cons` cell or a struct field, where the
/// old encoding copied the text onto the heap.
fn string_identity_eq(args: &[Value]) -> Result<Value, EvalError> {
    match (&args[0], &args[1]) {
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a == b)),
        (other0, other1) => Err(EvalError::Internal(format!("string::eq: expected two Str arguments, got {:?}/{:?}", other0, other1))),
    }
}

/// Content equality (case-sensitive) — CL's `equal`/`string=`, *not* `eq`
/// (identity — see the string table's doc comment and `docs/cl-equivalence-catalog.md`'s
/// eq/eql/equal/equalp section). Named for what it computes, not for which
/// builtin method currently calls it.
fn string_content_eq(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_str(heap, &args[0])? == expect_str(heap, &args[1])?))
}

/// Content equality ignoring ASCII case — CL's `equalp` for strings.
fn string_content_eqp(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_str(heap, &args[0])?.eq_ignore_ascii_case(expect_str(heap, &args[1])?)))
}

fn string_lt(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_str(heap, &args[0])? < expect_str(heap, &args[1])?))
}

/// The `<`/`<=`/`>`/`>=` comparison operators on `string`, lexicographic (byte
/// order) — the counterpart of `eval_int_builtin`'s numeric comparisons, kept
/// in one function for the same reason (one match over the operator symbol).
fn string_compare(heap: &Heap, method: &str, args: &[Value]) -> Result<Value, EvalError> {
    let a = expect_str(heap, &args[0])?;
    let b = expect_str(heap, &args[1])?;
    Ok(Value::Bool(match method {
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        ">=" => a >= b,
        other => return Err(EvalError::Internal(format!("string_compare: not a comparison operator: {}", other))),
    }))
}

fn char_eq(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_char(&args[0])? == expect_char(&args[1])?))
}

fn char_lt(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_char(&args[0])? < expect_char(&args[1])?))
}

/// The `<`/`<=`/`>`/`>=` comparison operators on `char`, by Unicode scalar
/// value (a compiled `char` is a raw `i64` code point, so this matches the
/// integer `icmp`s `compiler.rs` emits for the same operators).
fn char_compare(method: &str, args: &[Value]) -> Result<Value, EvalError> {
    let a = expect_char(&args[0])?;
    let b = expect_char(&args[1])?;
    Ok(Value::Bool(match method {
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        ">=" => a >= b,
        other => return Err(EvalError::Internal(format!("char_compare: not a comparison operator: {}", other))),
    }))
}

/// CL's `equalp` for `char` — case-insensitive (`(equalp #\A #\a)` is true).
fn char_eqp(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_char(&args[0])?.eq_ignore_ascii_case(&expect_char(&args[1])?)))
}

pub(crate) fn expect_bool(v: &Value) -> Result<bool, EvalError> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(EvalError::Internal(format!("expected a Bool, got {:?}", other))),
    }
}

fn bool_eq(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_bool(&args[0])? == expect_bool(&args[1])?))
}


/// `eq` on `Sexpr`: compares the underlying `mem::Value` directly (see
/// `registry::sexpr_assoc`'s doc comment for why this matches CL's `eq`
/// semantics — cons identity, scalar/symbol value equality).
fn sexpr_eq(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(typelisp_rt::equality::eq_val(heap, args[0], args[1])))
}

/// `eql` on `Sexpr`: CL's `eql` is `eq` plus "two numbers of the same type
/// and value are equivalent even when they aren't the same object" — the
/// one case that can actually diverge from `eq`'s plain `Value` equality
/// now that `Sexpr::Float` is heap-boxed (`Value::Boxed`, see `BoxedObj`):
/// `eq`'s `==` compares two boxed floats by `BoxId` identity (correctly not
/// `eq` for separately-allocated equal floats, the same way two separately
/// built `Str`s aren't `eq` — see `registry::sexpr_assoc`'s doc comment),
/// but they must still be `eql`. Every other `Sexpr` variant is either
/// immediate (`Int`/`Char`/`Bool`/`Sym`, already value-equal under `eq`) or
/// `eq`-as-identity by design (`Cons`/`Str`) — CL's own `eql` agrees `eq` is
/// already correct for those, so this only special-cases `Boxed`.
///
/// `Value::Boxed` now holds more than floats (structs, hash tables, binding
/// cells, closures) — only a *float* box gets content comparison here; every
/// other boxed kind is an aggregate/identity object for which CL's `eql` is
/// `eq` anyway, so they fall through to the identity comparison below.
fn sexpr_eql(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(typelisp_rt::equality::eql_val(heap, args[0], args[1])))
}

/// CL's `equal` on `Sexpr`: `eql` on every atom but `Cons` (structural
/// recursion) and `Str` (case-sensitive content). The comparison itself is
/// [`typelisp_rt::equality::equal_val`], which the compiled `rt_sexpr_equal`
/// shim also calls — one implementation, so `(equal x y)` cannot answer
/// differently depending on whether the caller happened to be compiled.
fn sexpr_equal(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(typelisp_rt::equality::equal_val(heap, args[0], args[1])))
}

/// CL's `equalp` on `Sexpr` — [`sexpr_equal`]'s case-folding, cross-type-
/// numeric, struct/enum-recursive sibling. Same shared implementation.
fn sexpr_equalp(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(typelisp_rt::equality::equalp_val(heap, args[0], args[1])))
}
