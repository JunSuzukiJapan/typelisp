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

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, ToPrimitive, Zero};

use crate::check::registry::{EVAL_ERROR, PARSE_FLOAT_ERROR, PARSE_INT_ERROR, READ_ERROR};
use crate::type_key::{alloc_typed_enum, heap_type_path};
use crate::types::{
    path_is_builtin, path_is_builtin_any, LLVM_METHOD_RECEIVER_TYPES, NATIVE_LOWERED_PRIMITIVES,
};
use typelisp_mem::RootScope;

use crate::check::core;
use crate::check::repr::Repr;
use crate::{BoxId, CompileTarget, Heap, MacroExpander, MacroLambda, Path, Ref, SymId, Type, Value};

use super::pprint;
use super::scope;
use super::value::{EvalError, Slot};

/// The evaluator over core forms — the cons-cell program representation that
/// replaces the `Typed` tree. A child module rather than a sibling of
/// `interp`, so it can reach this module's private items (the builtin
/// dispatch, the module tree) while it is being built alongside the evaluator
/// it will replace.
mod core_eval;

/// A registered function or method body with its parameter names. Lives at
/// exactly one [`scope::ModuleScope`] tree node — its own defining module —
/// rather than in a flat program-wide table; see that module's doc comment.
pub(crate) struct FnDef {
    /// Parameter names, including the receiver name first for instance methods.
    pub(crate) params: Vec<String>,
    /// The body's core forms, in order.
    ///
    /// These are heap cells, which the old `Vec<Typed>` was not — so the
    /// module tree is now a *source of GC roots*. `Interp::exec` permanently
    /// roots the whole top-level form it registers (one root per definition,
    /// covering the body, the parameter list and every representation
    /// reachable from it); see `Heap::push_permanent_root`, which is
    /// LIFO-independent for exactly this.
    pub(crate) body: Vec<Value>,
    /// Only ever set for a `defmacro` with a trailing `&rest` parameter (see
    /// [`Interp::expand_macro`]); always `false` for `defun`/`defmethod`,
    /// which `apply` calls 1:1 regardless.
    pub(crate) rest: bool,
    /// The `&optional`/`&key` structure of a `defmacro`'s lambda list —
    /// `Some` only for a macro (its default-value bodies and keyword names,
    /// consumed by [`Interp::bind_macro_args`]), `None` for `defun`/
    /// `defmethod`, which bind their arguments 1:1 with no defaults. `params`
    /// still lists every binding name in order; this only adds how the
    /// non-required regions are filled.
    pub(crate) lambda: Option<MacroLambda>,
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
    pub(crate) sig: Option<(Vec<Repr>, Repr)>,
    /// Whether this definition is `pub` — the runtime twin of
    /// `FnSig`/`VarInfo`/`AssocFn.sig`'s own `public` bit
    /// (`check::registry`), baked onto the `TopLevel` node by the checker
    /// (which computed it once already) so `scope::ModuleScope`'s qualified-
    /// path resolution can enforce `Checker::resolve_fn_path`'s
    /// `public || in_scope` gate without needing the checker's `Registry` at
    /// runtime.
    pub(crate) public: bool,
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
    pub(crate) compiled: RefCell<Option<Rc<crate::compile::CompiledFn>>>,
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
pub(crate) struct EnumDef {
    /// Each variant's name and its fields' representations, in declaration
    /// order — everything the two consumers need: the printer wants the name
    /// (`eval::format`), and the compiled-global boundary wants each field's
    /// representation to decode a box back into an enum value. The core
    /// `(defenum PATH (SYM...) ((REPR...)...))` form carries exactly this.
    pub(crate) variants: Vec<(String, Vec<Repr>)>,
}

/// An in-progress pretty-printing session: what CL would call "the output
/// stream is a pretty stream right now".
///
/// typelisp has no first-class streams, so instead of handing the user a
/// pretty-stream *value* to thread through every call (a whole new mutable
/// value type the language does not otherwise have — see `docs/dev/TODO.md`'s
/// T5 notes on why that was the blocker), the session is implicit interpreter
/// state, in exactly the way the GC heap already is. It is opened by the
/// outermost `pprint-logical-block` and flushed to stdout when that block
/// closes. While it is open, *every* printing operation — `print`, `println`,
/// `(format true …)`, `pprint` — appends into it instead of going straight to
/// stdout, so ordinary printing calls supply the block's content and the
/// `pprint-newline`/`pprint-indent`/`pprint-tab` builtins supply its layout,
/// which is exactly how the same code reads in CL.
struct PrettySession {
    /// The text and pretty-printer ops accumulated so far.
    out: crate::eval::pprint::Out,
    /// One entry per open logical block: a heap cell holding the still
    /// unconsumed tail of the list that block was given, which `pprint-pop`
    /// walks. A *cell* rather than a bare `Value` because a cell is a GC root
    /// for as long as its `Rc` is alive (`Heap::alloc_cell`'s registry), so the
    /// list survives whatever the block's body allocates — and is released
    /// automatically when the block closes, unlike a permanent root.
    lists: Vec<Rc<crate::mem::BoxId>>,
    /// The layout parameters this session was opened with, read once so a
    /// `setf` of `*print-right-margin*` inside a block cannot change the
    /// margin halfway through laying one document out.
    opts: crate::eval::pprint::Opts,
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
    root: RefCell<scope::ModuleScope>,
    /// A shared handle to the live `Checker`, set by [`Self::set_checker`] on
    /// the drivers that support runtime `eval` (the CLI's `run_file`/`repl`/
    /// `compile_module` and the LSP). `None` in throwaway/AOT/bootstrap/test
    /// contexts, where `eval` returns an error rather than type-checking at
    /// runtime. The `Rc<RefCell<..>>` is the same cell the driver checks
    /// top-level forms through; the borrow discipline (never hold a checker
    /// borrow across an `exec` that could `eval`) keeps the two from
    /// double-borrowing — see `Self::eval_form` and the drivers' own comments.
    checker: Option<Rc<RefCell<crate::check::Checker>>>,
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
    compiled_globals: RefCell<HashMap<Path, usize>>,
    /// The open pretty-printing session, if any — see [`PrettySession`].
    pretty: RefCell<Option<PrettySession>>,
    /// The values currently being rendered by their own `print-object`
    /// method, innermost last — [`Self::print_object`]'s re-entry guard.
    printing: RefCell<Vec<Value>>,
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
    /// Open streams, addressed by the opaque `i64` handle a stream
    /// `defstruct` holds. See `eval::stream` for why the OS resource lives
    /// here rather than inside the value.
    streams: RefCell<crate::eval::stream::StreamTable>,
    vtables: RefCell<Vec<Vec<(Path, String)>>>,
    /// `(concrete type key, trait path)` -> `vtable_id`, so the same pair
    /// interns to one id however many times it is boxed. The key's first
    /// half is `mangle_type`'s rendering (the `dyn-new` node's own concrete
    /// key), since `Type` has no `Hash`.
    vtable_ids: RefCell<HashMap<(String, Path), u32>>,
    /// `(vtable id, trait id)` -> the vtable id the *same concrete type* uses
    /// for that trait: the supertrait upcast table `dyn-upcast` reads.
    ///
    /// Keyed by the runtime vtable id because that is all an upcast site has
    /// — the concrete type, the other half of a vtable's identity, is gone by
    /// then. Filled in at every boxing site from `dyn-new`'s `SUPERS`
    /// ([`Self::register_dyn_box`]), which is where both halves are still in
    /// hand. The compiled tier keeps the same map under the same ids in
    /// `typelisp_rt` (`upcast_define`).
    dyn_upcasts: RefCell<HashMap<(u32, u32), u32>>,
    /// Trait path -> a small integer, so an upcast site in *compiled* code
    /// can name its target trait with a baked-in constant the way a boxing
    /// site names its vtable ([`Self::vtable_id_for`]). Interpreted upcasts
    /// go through the same ids, so both tiers agree by construction.
    trait_ids: RefCell<HashMap<Path, u32>>,
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
    dyn_dispatch_compiled: RefCell<HashSet<Path>>,
}

/// One outgoing edge of the top-level compile call graph
/// [`Interp::compute_sccs`] walks: either a plain `call` target (a
/// `defun`) or an `assoc` target (a `defmethod`, keyed the same way
/// [`Interp::method_key`] keys the scope tree's `methods`/`compiled`).
/// Carries the full typed key (not just its string name) so
/// [`Interp::compile_scc`] can reuse it directly to declare/wire the target
/// without re-deriving it from the name.
enum CallEdge {
    Fn(Path),
    Method(Path, String),
}

impl CallEdge {
    /// The node identity [`Interp::compute_sccs`]'s graph traversal keys on
    /// — a bare `defun` name, or `"type-path::method"` (the type's full
    /// `::`-joined path, never just its local segment — see
    /// [`Interp::method_key`]'s doc comment for why) for a `defmethod`,
    /// exactly the string shape [`Interp::compiled_fn_body`]/
    /// [`Interp::method_key`] already resolve back.
    fn node_name(&self) -> String {
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
fn fn_path_from_node_name(name: &str) -> Path {
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
        crate::compile::runtime::reset_global_table();
        // Same reasoning for the compiled tier's vtable table: ids are
        // per-`Interp`, so a stale entry from an earlier pair on this thread
        // must not survive into this one.
        crate::compile::runtime::reset_vtable_table();
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
            streams: RefCell::new(Default::default()),
            vtables: RefCell::new(Vec::new()),
            vtable_ids: RefCell::new(HashMap::new()),
            dyn_upcasts: RefCell::new(HashMap::new()),
            trait_ids: RefCell::new(HashMap::new()),
            dyn_dispatch_compiled: RefCell::new(HashSet::new()),
            pretty: RefCell::new(None),
            printing: RefCell::new(Vec::new()),
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
    /// `rt_vtable_slot` refuses to call. Compiled code can only reach a slot
    /// through a boxing site, and `core_bridge::collect_targets` puts every one
    /// of a boxing site's slots into the call graph — so a live 0 would mean
    /// that contract was broken, not that user code did something unusual.
    fn publish_vtables(&self) {
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
        crate::compile::runtime::vtable_define(id, addrs);
    }

    /// Every vtable interned so far, as `(id, slots)` — for a code generator
    /// that has to emit the table itself rather than patch it in from JIT
    /// addresses (`compile::aot::build_main_wrapper`).
    pub(crate) fn vtable_descriptors(&self) -> Vec<(u32, Vec<(Path, String)>)> {
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
        let mut tables = self.vtables.borrow_mut();
        let id = tables.len() as u32;
        tables.push(slots.to_vec());
        self.vtable_ids.borrow_mut().insert(key, id);
        id
    }

    /// The id interning `trait_path` for upcast lookups — the trait half of
    /// [`Self::dyn_upcasts`]'s key, and the constant a compiled
    /// `dyn-upcast` bakes in. Ids are dense and per-`Interp`, like
    /// vtable ids; nothing outside this pair of tables reads them.
    pub(crate) fn trait_id_for(&self, trait_path: &Path) -> u32 {
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
    fn register_dyn_box(
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
                crate::compile::runtime::upcast_define(from, to_trait, to);
            }
        }
        tables.into_iter().map(|(id, _)| id).collect()
    }

    /// Every upcast registered so far, as `(source vtable, trait id, target
    /// vtable)` — [`Self::vtable_descriptors`]'s counterpart for the code
    /// generator that has to emit the table itself instead of filling it in
    /// from a running interpreter (`compile::aot::build_main_wrapper`).
    /// Sorted so a rebuild of the same program emits the same startup code.
    pub(crate) fn upcast_descriptors(&self) -> Vec<(u32, u32, u32)> {
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

    /// Installs the precompiled compiler island (interp-closure removal
    /// Stage 4): parses the committed bitcode ([`crate::compile::bootstrap`])
    /// and registers every island top-level `defun` named in `island_defuns`
    /// into [`Self::compiled`], so a later call to `compile-function` (and
    /// the whole island it drives) runs as native code instead of being
    /// tree-walked. [`crate::compiler::load_aot`] calls this after re-checking
    /// and `exec`ing the island `SOURCE` (which registers the interpreted
    /// `FnDef`s + checker state the compiled bodies still need for
    /// signatures/fallback), passing the `defun` names it collected there.
    ///
    /// The island bitcode references no external symbols other than the
    /// `rt_*` runtime shims (verified: it is one self-contained module whose
    /// functions call each other directly and lower every builtin to an
    /// `rt_*`/`rt_llvm_call`), so `externals` is exactly
    /// [`rt_extern_functions`] — the same set `compile_scc` supplies for a
    /// JIT'd SCC.
    ///
    /// `check_hash` selects the two callers' differing staleness needs. The
    /// runtime loader ([`crate::compiler::load_aot`], `check_hash = true`)
    /// compares the bitcode's embedded source hash against the live `SOURCE`
    /// and hard-errors on a mismatch: a stale committed `.bc` (someone edited
    /// `compiler.rs` without running `scripts/regen-compiler-island.sh`) must
    /// never be silently loaded as wrong-version native bodies. The bootstrap
    /// regenerator ([`crate::compile::bootstrap::build_island_bitcode`],
    /// `check_hash = false`) *deliberately* loads the committed — necessarily
    /// older — `.bc` to compile a possibly-changed `SOURCE` with it (the
    /// snapshot chain that lets interpreted closures be removed: the *previous*
    /// native island recompiles the next one), so a mismatch is expected, not
    /// an error. In that mode a `defun` present in `island_defuns` but absent
    /// from the older `.bc` (a newly added island function) is skipped here and
    /// gets freshly compiled by the just-installed native `compile-function`
    /// like any other new body, rather than failing the whole install.
    pub(crate) fn install_island_bitcode(&self, bitcode: &[u8], island_defuns: &[String], check_hash: bool) -> Result<(), String> {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let buffer = MemoryBuffer::create_from_memory_range_copy(bitcode, "compiler_island");
        let module = Module::parse_bitcode_from_buffer(&buffer, crate::compile::llvm_context())
            .map_err(|e| format!("compiler island bitcode failed to parse: {}", e))?;

        if check_hash {
            let embedded = crate::compile::bootstrap::read_embedded_source_hash(&module)
                .ok_or_else(|| "compiler island bitcode has no embedded source hash".to_string())?;
            if embedded != crate::compile::bootstrap::island_source_hash(crate::compiler::SOURCE)? {
                return Err(
                    "compiler island bitcode is stale relative to compiler.rs's SOURCE — run scripts/regen-compiler-island.sh"
                        .to_string(),
                );
            }
        }

        // In bootstrap mode, keep only names the (older) module actually
        // defines; a `check_hash` load has a fresh `.bc` so all are present.
        let names: Vec<String> = island_defuns
            .iter()
            .filter(|n| check_hash || module.get_function(&crate::compile::symbols::user_symbol_name(n)).is_some())
            .cloned()
            .collect();
        let internal_names: Vec<String> =
            names.iter().map(|n| crate::compile::symbols::user_symbol_name(n)).collect();
        // Wire only the `rt_*` shims the module actually forward-declares.
        // Skipping the rest is not a leniency: a module can only *call* what
        // it declares, so a shim with no declaration here has no call site to
        // resolve, whereas passing it to `new_multi` would fail that
        // function's "no forward declaration" guard.
        //
        // Both loads need this, not just the bootstrap one. The `check_hash`
        // load verifies the `.bc` against `compiler.rs`'s `SOURCE` — and the
        // shim list is *Rust*, so adding one (`rt_gensym`, made to compile
        // `gensym` in macro-expansion lambdas; `rt_apply_any`, Stage D)
        // leaves the hash matching while the committed bitcode still declares
        // the older set. Gating on `check_hash` made every such addition fail
        // the install until the island was regenerated, for a mapping the
        // island had no use for.
        let externals: Vec<(String, usize)> = rt_extern_functions()
            .iter()
            .filter(|(n, _)| module.get_function(n).is_some())
            .map(|(n, addr)| (n.to_string(), *addr))
            .collect();
        let compiled_fns = crate::compile::CompiledFn::new_multi(&module, &internal_names, &externals)
            .map_err(|e| format!("compiler island JIT install failed: {}", e))?;

        for (name, cf) in names.iter().zip(compiled_fns) {
            let f = self.root.borrow().get_fn(&Path::root(name)).expect("island defun already registered by exec'ing SOURCE");
            *f.compiled.borrow_mut() = Some(Rc::new(cf));
        }
        Ok(())
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
    fn resolve_fn_ref(&self, r: &Ref) -> Option<Rc<FnDef>> {
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
    /// `argv` as `RtValue::Int`, not decoded into a boxed struct/`Data`/etc.
    /// — general-ADT bridging at this boundary isn't implemented, a
    /// pre-existing, separate gap). Calling a compiled method on a receiver
    /// built by *pure* interpretation (a real boxed-struct `RtValue::Sexpr`,
    /// never touched by compiled code) hits the same wall an analogous top-level
    /// `call` already would for a general-ADT parameter — a clear
    /// internal error here, not a silent misread of unrelated bits.
    fn call_compiled(
        &self,
        heap: &mut Heap,
        compiled: &crate::compile::CompiledFn,
        argv: &[Value],
        param_reprs: &[Repr],
        ret: &Repr,
    ) -> Result<Value, EvalError> {
        // A `defun`/`defmethod`'s `sig` lists only its fixed parameters; a
        // `&rest` one that reached compilation would land in the "no declared
        // representation" error above rather than be guessed at.
        let (int_args, crossing_roots) = self.encode_crossing_args(heap, argv, param_reprs, false)?;
        self.enter_compiled(heap);
        let raw = compiled.call(&int_args);
        for _ in 0..crossing_roots {
            heap.pop_root();
        }
        self.decode_compiled_return(heap, raw, ret)
    }

    /// [`Self::call_compiled`]'s argument-marshaling half, factored out so
    /// [`Self::eval`]'s `apply` arm can reuse it when the callee is a
    /// `BoxedObj::CompiledClosure` rather than a top-level `(compile ...)`d
    /// function — both cross the exact same interp-`RtValue` -> compiled-ABI
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
            _ => Ok(crate::compile::runtime::encode(*v)),
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
            Repr::Handle => match llvm_handle_get(raw) {
                Some(_) => Value::Int(raw),
                None => {
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
            Repr::Str => match crate::compile::runtime::decode(raw) {
                v @ Value::Str(_) => v,
                other => return Err(crossing_mismatch("a string", other)),
            },
            Repr::Sym => match crate::compile::runtime::decode(raw) {
                v @ Value::Symbol(_) => v,
                other => return Err(crossing_mismatch("a Symbol", other)),
            },
            Repr::Bignum | Repr::Ratio => match crate::compile::runtime::decode(raw) {
                v @ Value::Boxed(id) if heap.is_bignum(id) || heap.is_ratio(id) => v,
                other => return Err(crossing_mismatch("a bignum/ratio", other)),
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
            | Repr::HashTable(..) => crate::compile::runtime::decode(raw),
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
                    crate::compile::runtime::encode(v)
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
        let f: unsafe extern "C" fn(*const i64, u32, *const i64, u32) -> i64 = unsafe { std::mem::transmute(fn_ptr) };
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
        crate::compile::runtime::set_active_heap(heap as *mut Heap);
        ACTIVE_INTERP.with(|cell| cell.set(self as *const Interp));
        crate::compile::runtime::set_apply_interpreted(Some(rt_apply_interpreted));
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
        let f = crate::compile::runtime::decode(closure);
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
    fn method_key(&self, name: &str) -> Option<(Path, String)> {
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
    fn resolve_fn_def(&self, name: &str) -> Result<Rc<FnDef>, EvalError> {
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
    fn compiled_fn_body(&self, name: &str) -> Result<(Vec<(String, Repr)>, Vec<Value>), EvalError> {
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
    /// last set it to) via the existing `Slot`, converts it with
    /// `rtvalue_to_struct_field` (the same `RtValue` -> `mem::Value`
    /// boundary crossing a boxed-struct field write already uses — a
    /// global's storage is exactly that shape, a single always-live cell),
    /// and hands the result to `typelisp_rt::global_new`, which roots it
    /// permanently and returns its id. An enum global
    /// (`Option`/`Result`/a user `defenum`) crosses through
    /// `rtvalue_to_struct_field`'s ordinary `RtValue::Sexpr` arm like any
    /// other boxed value now — the enum-representation unification's
    /// compiler flip retired the raw-box/shift-tagged special case this
    /// used to need (a `defvar`'s declared type is always one ordinary user
    /// source can write, and every such enum instantiation is heap-repr by
    /// construction — see `Repr::field_kind`'s doc comment —
    /// so the native `RtValue::Data` fallback never actually reaches here).
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
    pub(crate) fn promote_global(&self, heap: &mut Heap, path: &Path) -> Result<usize, EvalError> {
        if let Some(&id) = self.compiled_globals.borrow().get(path) {
            return Ok(id);
        }
        let slot = self
            .root
            .borrow()
            .get_global(path)
            .ok_or_else(|| EvalError::Internal(format!("compile: global \"{}\" is not defined", path)))?;
        let v = slot.get(heap);
        let value = rtvalue_to_struct_field(&v);
        let id = crate::compile::runtime::global_new(heap, value);
        self.compiled_globals.borrow_mut().insert(path.clone(), id);
        Ok(id)
    }

    /// Compiles the `defun` named `name` (looked up in the scope tree) into one
    /// LLVM function — named `internal_name` — added to `module`. Shared by
    /// [`Self::compile_function`] (JIT, Phase 1) — which always passes a
    /// throwaway, single-use module and the same name twice — and
    /// `compile::aot::compile_file` (AOT, Phase 2) — which passes the same
    /// shared, file-wide module across every `defun` in the source file,
    /// asking for a different `internal_name` only for `main` (so it
    /// doesn't collide with the real C `main` the AOT path synthesizes
    /// separately — see that module's doc comment).
    ///
    /// Takes `module` instead of creating/returning one, on purpose: see
    /// `compiler.rs`'s doc comment for why `compile-function` (the
    /// typelisp-hosted half of this) can never hand back sole ownership of
    /// an `llvm-module` value once `labels`' mutual-recursion closures have
    /// captured it.
    ///
    /// Phase 1/2 scope: a non-generic `defun` whose single-expression body
    /// only uses node shapes `compile::core_bridge::to_island` has a real
    /// translation for (`i64` literals/vars/`+`/`-`/`*`) — anything else
    /// surfaces as a `Panic` from the compiler body's own `"unsupported"`
    /// handling (`compiler.rs`'s `compile-value`), not a separate check
    /// here; there's exactly one place that needs to know the supported
    /// shape.
    pub(crate) fn add_compiled_function(
        &self,
        heap: &mut Heap,
        module: Rc<RefCell<Module<'static>>>,
        name: &str,
        internal_name: &str,
    ) -> Result<(), EvalError> {
        let (params, body) = self.compiled_fn_body(name)?;
        self.translate_and_compile(heap, module, &params, &body, internal_name, &HashSet::new())
    }

    /// The AST-bridge-and-emit half of [`Self::add_compiled_function`],
    /// factored out (no behavior change for that caller) so closure
    /// unification Stage 7's [`Self::jit_define_closure`] can drive the same
    /// translate-then-`compile-function` pipeline for a *synthetic*
    /// top-level function — a "closure constructor" whose own `params` are
    /// **not** a real `defun`'s declared parameters but a captured-cell
    /// reference per free variable — rather than one looked up by name via
    /// [`Self::compiled_fn_body`].
    ///
    /// `extra_exclude_from_cell_names` is empty for every ordinary caller
    /// (`add_compiled_function`'s own behavior, unchanged); Stage 7's ctor
    /// passes its own synthetic parameter names there, because
    /// `names_captured_by_nested`'s free-variable walk of `body` (which
    /// literally *is* `(lambda ...)`, wrapping the real closure being
    /// JIT'd) would otherwise "discover" that the ctor's own params are
    /// captured by the nested `lambda` it wraps and — wrongly — cell-box
    /// them a second time (`tagged_sym_list`'s `kind + 10`): the ctor's own
    /// params are declared `Sexpr` specifically so `bind-params` passes
    /// each cell reference through unchanged (kind `6`, the same tagged-
    /// pointer passthrough any other boxed value gets), for the *inner*
    /// `lambda`'s own (correctly, separately, computed) `lcaptured` list to
    /// pick up as-is.
    fn translate_and_compile(
        &self,
        heap: &mut Heap,
        module: Rc<RefCell<Module<'static>>>,
        params: &[(String, Repr)],
        body: &[Value],
        internal_name: &str,
        extra_exclude_from_cell_names: &HashSet<String>,
    ) -> Result<(), EvalError> {
        // Every global this body reads/assigns must have a compiled-global
        // slot before translation starts — `ast_to_sexpr` looks each one up
        // by id, not by name (see `Ctx::globals`'s doc comment), so there is
        // nothing to resolve lazily mid-translation the way `compile-call`'s
        // `get-function` can for an ordinary function name.
        let targets = match crate::compile::core_bridge::collect_targets(heap, body) {
            Ok(t) => t,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        for target in &targets.globals {
            self.promote_global(heap, target)?;
        }
        // Every trait object this body boxes needs its vtable id before
        // translation starts, for the same reason a global needs its slot:
        // `ast_to_sexpr` bakes the id into the emitted IR as a constant.
        for site in &targets.dyn_boxes {
            self.register_dyn_box(&site.concrete_key, &site.trait_path, &site.slots, &site.supers);
        }
        for to_trait in &targets.dyn_upcasts {
            self.trait_id_for(to_trait);
        }
        for trait_path in &targets.dyn_traits {
            self.dyn_dispatch_compiled.borrow_mut().insert(trait_path.clone());
        }
        // `core_bridge` is deliberately `Registry`-free, so hand it the type
        // definitions as a plain flattened snapshot of the scope tree —
        // exactly what `exec` recorded there from each `defstruct`/`defenum`
        // form. Collected fresh per compilation; compiling is rare enough that
        // keeping a second always-current copy isn't worth it.
        let defs = self.compile_definitions();
        let compiled_globals = self.compiled_globals.borrow();
        let vtable_ids = self.vtable_ids.borrow();
        let trait_ids = self.trait_ids.borrow();
        let dyn_tables = crate::compile::symbols::DynTables { vtables: &vtable_ids, trait_ids: &trait_ids };
        let cx = crate::compile::core_bridge::Ctx::with_dyn_tables(&defs, &compiled_globals, dyn_tables);
        let excluded = intern_names(heap, extra_exclude_from_cell_names);
        let params = intern_params(heap, params);
        let (param_list, body_sexpr) =
            match crate::compile::core_bridge::function_parts(heap, &params, body, cx, &excluded) {
                Ok(v) => v,
                Err(e) => return Err(EvalError::Panic(e.to_string())),
            };
        drop(compiled_globals);
        drop(vtable_ids);
        drop(trait_ids);

        self.run_compile_function(heap, module, internal_name, param_list, body_sexpr)
    }

    /// The scope tree's type definitions, in the shape the compile bridge
    /// takes. See [`Self::translate_and_compile`] for why it is a snapshot.
    fn compile_definitions(&self) -> crate::compile::core_bridge::Definitions {
        let (structs, enums) = self.root.borrow().collect_struct_and_enum_types();
        let mut defs = crate::compile::core_bridge::Definitions::new();
        for (path, _fields) in structs {
            defs.record_struct(path);
        }
        for (path, _def) in enums {
            defs.record_enum(path);
        }
        defs
    }

    /// Drives the island's `compile-function` over one already-translated
    /// function `(param_list, body_sexpr)` into `module` — the shared tail of
    /// [`Self::translate_and_compile`] and [`Self::add_compiled_global_init`].
    ///
    /// Dispatches to the *compiled* island `compile-function` whenever it is
    /// installed in [`Self::compiled`] (interp-closure removal Stage 4: after
    /// [`crate::compiler::load_aot`], so the island runs natively and its own
    /// `labels`/`lambda` bodies are never built as interpreted closures),
    /// falling back to the interpreted `FnDef` otherwise — a plain
    /// `load_compiler` environment, or the bootstrap
    /// ([`crate::compile::bootstrap`]) that produces the island bitcode in
    /// the first place, where the compiled island doesn't exist yet.
    ///
    /// The compiled `compile-function` returns the very `llvm-module` it was
    /// handed (mutated in place); both callers care only about that side
    /// effect and ignore the return, but it is still decoded so
    /// [`Self::call_compiled`]'s LLVM-handle bookkeeping stays balanced. A
    /// [`llvm_handles_mark`]/[`llvm_handles_release`] pair brackets the call
    /// so the transient handles the native compiler registers while walking
    /// the AST don't accumulate across many compiles.
    fn run_compile_function(
        &self,
        heap: &mut Heap,
        module: Rc<RefCell<Module<'static>>>,
        internal_name: &str,
        param_list: Value,
        body_sexpr: Value,
    ) -> Result<(), EvalError> {
        let compiler_path = Path::root("compile-function");
        let argv = vec![
            llvm_module_value_rc(module),
            str_rt(heap, internal_name),
            param_list,
            body_sexpr,
        ];
        let compiler_def = self.root.borrow().get_fn(&compiler_path).ok_or_else(|| {
            EvalError::Internal("compile: compiler body not loaded — call load_compiler first".into())
        })?;
        let compiled = compiler_def.compiled.borrow().clone();
        if let Some(cf) = compiled {
            // The island's own entry point hands back an LLVM module, which
            // crosses as a raw registry index.
            let ret = Repr::Handle;
            let mark = llvm_handles_mark();
            let scope_mark = heap.session_root_count();
            let param_reprs =
                &compiler_def.sig.as_ref().expect("the compiler body always has a signature").0;
            let r = self.call_compiled(heap, &cf, &argv, param_reprs, &ret);
            llvm_handles_release(mark);
            // Scope boxes the island created during this compile are session
            // roots (see `LlvmRetK::Scope`); release them with the handles.
            heap.truncate_session_roots(scope_mark);
            r?;
            return Ok(());
        }
        self.apply(heap, &compiler_def, argv)?;
        Ok(())
    }

    /// AOT-only counterpart of [`Self::add_compiled_function`]: compiles a
    /// `defvar`'s initializer expression `value` into a zero-argument LLVM
    /// function `internal_name` in `module` that, when called, evaluates it
    /// and calls `rt_global_new` to establish that global's *runtime*
    /// storage — one entry in the startup sequence `compile::aot::
    /// compile_file` generates and wires into `main` (via
    /// `compile::aot::build_main_wrapper`) so a standalone executable
    /// allocates each of its own promoted globals before `tl_main` (the
    /// file's own `main` defun) ever runs. See [`Self::promote_global`]'s
    /// doc comment for why `compile::aot::compile_file` must call these, in
    /// the same order it called `Self::promote_global` for each `defvar`.
    ///
    /// Mirrors `add_compiled_function`'s own translate-then-`compile-
    /// function` shape almost exactly, just with no parameters and a body
    /// wrapped as `(global-init kind value-form)`
    /// ([`crate::compile::symbols::ast_to_sexpr_for_global_init`],
    /// `compiler.rs`'s `compile-global-init`) instead of an ordinary
    /// translated function body — `value` may itself reference other
    /// globals (an earlier `defvar`'s value), so the same promotion pass
    /// applies here too.
    pub(crate) fn add_compiled_global_init(
        &self,
        heap: &mut Heap,
        module: Rc<RefCell<Module<'static>>>,
        internal_name: &str,
        form: Value,
    ) -> Result<(), EvalError> {
        // The whole `(defvar ...)` form, not just its initializer:
        // `core_bridge::global_init` reads the declared representation off the
        // form to know how the global's storage is tagged, which the
        // initializer alone does not say.
        let body = std::slice::from_ref(&form);
        let targets = match crate::compile::core_bridge::collect_targets(heap, body) {
            Ok(t) => t,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        // The initializer may reference *other* globals (an earlier `defvar`'s
        // value), so the same promotion pass an ordinary body gets applies.
        for target in &targets.globals {
            self.promote_global(heap, target)?;
        }
        for site in &targets.dyn_boxes {
            self.register_dyn_box(&site.concrete_key, &site.trait_path, &site.slots, &site.supers);
        }
        for to_trait in &targets.dyn_upcasts {
            self.trait_id_for(to_trait);
        }
        let defs = self.compile_definitions();
        let compiled_globals = self.compiled_globals.borrow();
        let vtable_ids = self.vtable_ids.borrow();
        let trait_ids = self.trait_ids.borrow();
        let dyn_tables = crate::compile::symbols::DynTables { vtables: &vtable_ids, trait_ids: &trait_ids };
        let cx = crate::compile::core_bridge::Ctx::with_dyn_tables(&defs, &compiled_globals, dyn_tables);
        let body_sexpr = match crate::compile::core_bridge::global_init(heap, form, cx) {
            Ok(Some(v)) => v,
            Ok(None) => {
                return Err(EvalError::Internal(
                    "compile: a global initializer was built from something that is not a `defvar`".into(),
                ))
            }
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        drop(compiled_globals);
        drop(vtable_ids);
        drop(trait_ids);
        let mut s = RootScope::new(heap);
        s.push_root(body_sexpr);
        // A global initializer takes no parameters.
        let param_list = Value::Empty;
        self.run_compile_function(&mut s, module, internal_name, param_list, body_sexpr)
    }

    /// `(compile fn-name)` (or `(compile type::method)`): JIT-compiles a
    /// previously-defined `defun`/`defmethod` and marks the target `FnDef`
    /// node compiled (see `FnDef::compiled`) so `call`/`assoc`
    /// dispatches to native code instead of tree-walking it from then on.
    /// `target` is already fully resolved by `Checker::check_compile` (a
    /// `Ref` re-verified here via `Self::resolve_fn_ref`, or a `type_name`+
    /// `method` re-verified via `ModuleScope::resolve_method` — the same
    /// independent re-resolution every other reference gets, not a bare
    /// name to search the whole tree for by local name alone). The
    /// qualified string this derives from that resolved identity only feeds
    /// [`Self::compute_sccs`]'s *internal* graph bookkeeping — unchanged
    /// from before, and still keyed by local type name for a method
    /// ([`Self::method_key`]), since transitively-discovered call targets
    /// already reach that machinery the same way. See
    /// [`Self::add_compiled_function`] for the supported-shape scope.
    ///
    /// labels/closures Stage 3 (single-function shape) / Stage 5 (SCC
    /// generalization): unlike `compile::aot::compile_file` (one shared
    /// module built up over every `defun` in file order, so a callee is
    /// always already fully defined in that same module by the time its
    /// caller is compiled — see that module's doc comment), `name`'s own
    /// strongly connected component of the top-level call graph —
    /// [`Self::compute_sccs`], usually just `{name}` itself, but a genuine
    /// group for mutual recursion across *separate* top-level functions —
    /// gets one throwaway module/engine per SCC ([`Self::compile_scc`]),
    /// processed leaf-SCC-first. Every target *outside* the current SCC
    /// (`crate::compile::symbols::collect_call_targets`/
    /// `collect_assoc_targets`, gathered via [`Self::call_graph_edges`]) is
    /// handled by hand, in three steps: (1) it must already be `compile`d by
    /// the time its SCC is processed — [`Self::compute_sccs`]'s finish-order
    /// contract guarantees this, so a violation is an internal-invariant
    /// `.expect()`, not a user-facing error; (2) forward-declared — no body
    /// — in this SCC's module *before* the compiler body runs for any of its
    /// members (`compile-call`'s `get-function` needs to find *something* by
    /// that name); (3) wired to the real, already-running JIT code's address
    /// via `add_global_mapping` *after* (`crate::compile::CompiledFn::
    /// new_multi`'s `externals` parameter) — can't happen any earlier, since
    /// the engine that will actually run this SCC's code doesn't exist until
    /// then. Self-recursion, and recursion among an SCC's own members, needs
    /// none of this: every member of the SCC is forward-declared under its
    /// own name in the *same* module before any of their bodies are
    /// translated, so `compile-function`'s own `(add-function m name)`
    /// (reusing that declaration — see [`llvm_module_add_function`]'s doc
    /// comment) already gives every sibling something to call before any
    /// body exists.
    ///
    /// A user-defined method this body calls (`assoc`,
    /// `crate::compile::symbols::collect_assoc_targets`) goes through the
    /// exact same three steps, *keyed and named differently*: looked up in
    /// [`Self::methods`]/[`Self::compiled_methods`] instead of
    /// [`Self::fns`]/[`Self::compiled`], and forward-declared/wired under the
    /// mangled name [`method_link_name`] builds — the same literal string
    /// this very method itself uses as `internal_name` when `name` is a
    /// method (see [`Self::add_compiled_function`]'s call below), which is
    /// also exactly what `compiler.rs`'s `compile-assoc` mangles a callee's
    /// `(type-name, method)` back into before its own `get-function` lookup
    /// — so the three names (this method's own `internal_name`, this
    /// method's entry in `externals`, and a *caller's* `compile-assoc`
    /// lookup) can never drift apart. A primitive-receiver target *not*
    /// registered in the scope tree's `methods` (`i64`/`i32`'s own built-in
    /// arithmetic; `string`'s, since Stage 7 of the Sexpr-representation plan —
    /// `docs/implementation-log.md`) needs none of this: those compile
    /// natively with no external call (`compile-assoc`'s own dispatch, which
    /// panics clearly on its own for any one of *their* methods it doesn't
    /// actually implement, e.g. `string::upcase`). But a *user-defined*
    /// method on a primitive receiver (e.g. the prelude's `impl Eq i32` →
    /// `i32::equals`) is registered like any other `defstruct` method and
    /// takes the normal three steps — `compile-assoc`'s dispatch falls
    /// through to the same mangled-name call for it. Anything else
    /// (`f64`/`char` builtins — still out of scope) panics clearly right
    /// here rather than deep inside `compile-assoc`'s own `get-function`.
    fn compile_function(&self, heap: &mut Heap, target: &CompileTarget) -> Result<Value, EvalError> {
        let (name, already_compiled) = match target {
            CompileTarget::Fn(r) => {
                self.resolve_fn_ref(r).ok_or_else(|| EvalError::NoSuchFunction(r.written.join("::")))?;
                (r.resolved.to_string(), self.root.borrow().fn_compiled(&r.resolved))
            }
            CompileTarget::Method { type_name, method, home } => {
                self.root
                    .borrow()
                    .resolve_method(home, type_name, method)
                    .ok_or_else(|| EvalError::NoSuchFunction(method_link_name(type_name, method)))?;
                (method_link_name(type_name, method), self.root.borrow().method_compiled(type_name, method))
            }
        };
        if already_compiled {
            return Ok(Value::Bool(true));
        }
        for scc in self.compute_sccs(heap, &name)? {
            self.compile_scc(heap, &scc)?;
        }
        Ok(Value::Bool(true))
    }

    /// `name`'s own outgoing edges in the top-level compile call graph —
    /// every concrete function/method *instantiation* `name`'s body calls,
    /// filtered exactly the way `compile_function_rec` always has: self-
    /// recursion and `rt_*`/native builtins excluded from
    /// [`CallEdge::Fn`]; `Vector`/`HashTable`'s op-node-lowered builtin
    /// methods and natively-lowered primitive-receiver builtins excluded
    /// from [`CallEdge::Method`]. A method target with no registered
    /// implementation at all (a builtin `compile-assoc` doesn't lower
    /// natively, e.g. `f64::sqrt`) is a compile-time error here, same as
    /// before Stage 5 — this is the one path that produces a real user-
    /// facing error out of graph construction, everything else just shapes
    /// the graph [`Self::compute_sccs`] walks. Shared by that graph walk and
    /// [`Self::compile_scc`] (which needs the same edges again, in typed
    /// form, to know what to forward-declare/wire as `externals`).
    fn call_graph_edges(&self, heap: &Heap, name: &str) -> Result<Vec<CallEdge>, EvalError> {
        let path = fn_path_from_node_name(name);
        let method_key = self.method_key(name);
        let (_, body) = self.compiled_fn_body(name)?;
        let targets = match crate::compile::core_bridge::collect_targets(heap, &body) {
            Ok(t) => t,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        let mut edges = Vec::new();

        edges.extend(
            targets
                .calls
                .into_iter()
                .filter(|p| *p != path && !is_rt_builtin_name(p.last_segment()))
                .map(CallEdge::Fn),
        );

        let method_targets: Vec<(Path, String)> = targets
            .methods
            .into_iter()
            .filter(|key| method_key.as_ref() != Some(key))
            .filter(|key| {
                // `Vector<T>`'s field-backed builtin methods (`new`/`get`/
                // `set`/`len`/`push`/`pop`) are lowered to a `vector-op` node
                // (`core_bridge::translate_vector_method` -> `rt_struct_*`),
                // not a method call, so — like the native primitive methods
                // below — they are never a real call target. `vector::iter`
                // is deliberately excluded from this list: it is a genuine
                // prelude `defmethod` (`vector-iter::new`) and must be
                // `compile`d like any other method.
                if path_is_builtin(&key.0, "vector") && matches!(key.1.as_str(), "new" | "get" | "set" | "len" | "push" | "pop") {
                    return false;
                }
                // `HashTable<K,V>`'s builtin methods lowered to a `hashtable-op`
                // node (`core_bridge::translate_hashtable_method`) are likewise
                // never a real call target. `iter` (a real `defmethod`) is
                // deliberately absent so it's validated/transitively compiled
                // normally.
                if path_is_builtin(&key.0, "hashtable")
                    && matches!(key.1.as_str(), "new" | "set" | "get" | "remove" | "count" | "clear" | "keys" | "values" | "entries")
                {
                    return false;
                }
                // `llvm-*`/`scope` builtin methods are natively lowered to
                // the `rt_llvm_call` dispatch shim (an `llvm-op` node,
                // interp-closure removal Stage 1) — like `vector-op`/
                // `hashtable-op` above, never a real call target. A
                // heap-repr `Scope<V>` method has no compiled lowering and
                // panics inside `compile-assoc-user`'s `get-function`
                // instead, per the convention in the next comment.
                if path_is_builtin_any(&key.0, &LLVM_METHOD_RECEIVER_TYPES) {
                    return false;
                }
                // A user-registered method is a real call target even on a
                // primitive receiver (`i32::equals`); only the natively
                // lowered `i64`/`i32`/`char`/`string`/`f64`/`bignum`/`ratio`
                // builtins (`+`, `<`, `=`, `lt`, `length`, `sqrt`, `fadd`,
                // `rt_bignum_add`, ...) are excluded — those become LLVM
                // instructions / `rt_str_*`/`rt_bignum_*`/`rt_ratio_*` calls
                // in `compile-assoc`, not function calls. A builtin on these
                // receivers that `compile-assoc` does *not* lower natively
                // (`i64::int->char`, `char::equalp`, ...) is kept as a target
                // so the `!self.methods.contains_key` check below rejects it
                // with a clean up-front error — otherwise it reaches the
                // island's `get-function` guard, an unrecoverable
                // `rt_llvm_call` abort under the AOT-native island
                // (interp-closure removal Stage 8a). `is_native_lowered_primitive_method`
                // is the Rust twin of the island's `*-native-method?` list.
                self.root.borrow().has_method(&key.0, &key.1)
                    || !path_is_builtin_any(&key.0, &NATIVE_LOWERED_PRIMITIVES)
                    || !is_native_lowered_primitive_method(key.0.last_segment(), &key.1)
            })
            .collect();
        for (type_name, method) in &method_targets {
            if !self.root.borrow().has_method(type_name, method) {
                return Err(EvalError::Panic(format!(
                    "compile: \"{}\" calls \"{}\", a builtin method with no compiled implementation",
                    name,
                    method_link_name(type_name, method)
                )));
            }
        }
        edges.extend(method_targets.into_iter().map(|(p, m)| CallEdge::Method(p, m)));
        Ok(edges)
    }

    /// Tarjan's algorithm over the top-level compile call graph, rooted at
    /// `name`, restricted to the induced subgraph of not-yet-`compile`d
    /// nodes (labels/closures Stage 5) — an edge into an already-compiled
    /// target is a leaf for this traversal's purposes, since its address is
    /// already known and needs no further graph treatment. Returns every
    /// strongly connected component this traversal reaches, **in the order
    /// Tarjan completes them**: a classic property of the algorithm is that
    /// this finish order is a reverse topological order of the SCC
    /// condensation — if `A` calls something in a different SCC `B`, `B`
    /// finishes (and is pushed onto the result) before `A` does. That is
    /// exactly the order [`Self::compile_function`] needs to hand to
    /// [`Self::compile_scc`]: every SCC's external dependencies are already
    /// compiled by the time it's processed. A single-member SCC with no
    /// self-loop is the common case (an ordinary, non-recursive-with-others
    /// function); a multi-member SCC is genuine mutual recursion across
    /// separate top-level functions, unsupported before this stage.
    fn compute_sccs(&self, heap: &Heap, name: &str) -> Result<Vec<Vec<String>>, EvalError> {
        let mut counter = 0usize;
        let mut indices: HashMap<String, usize> = HashMap::new();
        let mut lowlink: HashMap<String, usize> = HashMap::new();
        let mut on_stack: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();
        let mut sccs: Vec<Vec<String>> = Vec::new();
        self.scc_strongconnect(heap, name, &mut counter, &mut indices, &mut lowlink, &mut on_stack, &mut stack, &mut sccs)?;
        Ok(sccs)
    }

    /// One node's worth of Tarjan's `strongconnect` — see
    /// [`Self::compute_sccs`]'s doc comment for the algorithm-level
    /// contract. Recursive over [`Self::call_graph_edges`]; an edge whose
    /// target is already compiled is skipped outright (never entered into
    /// `indices` at all), so it never contributes a spurious singleton SCC.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn scc_strongconnect(
        &self,
        heap: &Heap,
        node: &str,
        counter: &mut usize,
        indices: &mut HashMap<String, usize>,
        lowlink: &mut HashMap<String, usize>,
        on_stack: &mut HashSet<String>,
        stack: &mut Vec<String>,
        sccs: &mut Vec<Vec<String>>,
    ) -> Result<(), EvalError> {
        indices.insert(node.to_string(), *counter);
        lowlink.insert(node.to_string(), *counter);
        *counter += 1;
        stack.push(node.to_string());
        on_stack.insert(node.to_string());

        for edge in self.call_graph_edges(heap, node)? {
            let already_compiled = match &edge {
                CallEdge::Fn(p) => self.root.borrow().fn_compiled(p),
                CallEdge::Method(p, m) => self.root.borrow().method_compiled(p, m),
            };
            if already_compiled {
                continue;
            }
            let target = edge.node_name();
            if !indices.contains_key(&target) {
                self.scc_strongconnect(heap, &target, counter, indices, lowlink, on_stack, stack, sccs)?;
                let merged = lowlink[node].min(lowlink[&target]);
                lowlink.insert(node.to_string(), merged);
            } else if on_stack.contains(&target) {
                let merged = lowlink[node].min(indices[&target]);
                lowlink.insert(node.to_string(), merged);
            }
        }

        if lowlink[node] == indices[node] {
            let mut component = Vec::new();
            loop {
                let w = stack.pop().expect("node's own strongconnect frame pushed it onto the stack");
                on_stack.remove(&w);
                let is_root = w == node;
                component.push(w);
                if is_root {
                    break;
                }
            }
            sccs.push(component);
        }
        Ok(())
    }

    /// Compiles one strongly connected component of the top-level call graph
    /// (labels/closures Stage 5) — a single function/method, or a set of
    /// separate top-level `defun`/`defmethod`s mutually recursive with each
    /// other — into one shared, throwaway LLVM module, replacing the single-
    /// function-per-module shape every `compile_function_rec` call used
    /// before this stage. Every `members` name is forward-declared under its
    /// real internal name (`core_bridge::user_symbol_name`) *before* any of
    /// their bodies are translated, exactly like an external call target
    /// always was — so a call from one member to a sibling still without a
    /// body yet resolves to that same declaration by name
    /// (`compile-call`'s `get-function`), and [`Self::add_compiled_function`]
    /// (via `compiler.rs`'s `compile-function`, whose own `(add-function m
    /// name)` now reuses an existing declaration instead of minting a second,
    /// disjoint one — see [`llvm_module_add_function`]'s doc comment) attaches
    /// that member's real body to it in place. Targets *outside* `members`
    /// are handled exactly like [`Self::call_graph_edges`]'s callers always
    /// have: forward-declared, then wired post-hoc via `add_global_mapping`
    /// (`externals`) to their already-compiled address — guaranteed to exist
    /// by [`Self::compute_sccs`]'s finish-order contract. The whole module is
    /// JIT'd exactly once via [`crate::compile::CompiledFn::new_multi`], so
    /// every member shares one execution engine (mutual calls within the SCC
    /// need no `add_global_mapping` entry at all — LLVM resolves them
    /// directly against the sibling's own definition in this same module).
    fn compile_scc(&self, heap: &mut Heap, members: &[String]) -> Result<(), EvalError> {
        let member_set: HashSet<&str> = members.iter().map(|s| s.as_str()).collect();

        let mut call_targets: Vec<Path> = Vec::new();
        let mut method_targets: Vec<(Path, String)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for member in members {
            for edge in self.call_graph_edges(heap, member)? {
                let target_name = edge.node_name();
                // A sibling within this same SCC resolves through the SCC's
                // own internal forward declarations below, not `externals`.
                if member_set.contains(target_name.as_str()) || !seen.insert(target_name) {
                    continue;
                }
                match edge {
                    CallEdge::Fn(p) => call_targets.push(p),
                    CallEdge::Method(p, m) => method_targets.push((p, m)),
                }
            }
        }

        let module = {
            let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
            let module = Rc::new(RefCell::new(crate::compile::llvm_context().create_module("compiled")));
            for member in members {
                declare_external_function(&module, &crate::compile::symbols::user_symbol_name(member));
            }
            for target in &call_targets {
                declare_external_function(&module, &crate::compile::symbols::user_symbol_name(&target.to_string()));
            }
            for (type_name, method) in &method_targets {
                declare_external_function(&module, &crate::compile::symbols::user_method_symbol_name(type_name, method));
            }
            for (rt_name, _) in rt_extern_functions() {
                declare_external_function(&module, rt_name);
            }
            module
        };

        for member in members {
            self.add_compiled_function(heap, module.clone(), member, &crate::compile::symbols::user_symbol_name(member))?;
        }

        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let mut externals: Vec<(String, usize)> = call_targets
            .iter()
            .map(|p| {
                let f = self.root.borrow().get_fn(p).expect("Self::compute_sccs's finish order guarantees this is already compiled");
                let addr = f.compiled.borrow().as_ref().expect("Self::compute_sccs's finish order guarantees this is already compiled").address();
                (crate::compile::symbols::user_symbol_name(&p.to_string()), addr)
            })
            .collect();
        externals.extend(method_targets.iter().map(|(type_name, method)| {
            let f = self.root.borrow().get_method(type_name, method).expect("Self::compute_sccs's finish order guarantees this is already compiled");
            let addr = f.compiled.borrow().as_ref().expect("Self::compute_sccs's finish order guarantees this is already compiled").address();
            (crate::compile::symbols::user_method_symbol_name(type_name, method), addr)
        }));
        externals.extend(rt_extern_functions().iter().map(|(n, addr)| (n.to_string(), *addr)));
        // Mirrors `compile::aot::compile_file`'s own `verify()` call in the
        // same position, before handing the module to LLVM for real: a
        // typelisp-hosted `compiler.rs` bug that emits
        // instructions after a block's terminator (the `compile-let`
        // GC-root-leak fix's own doc comment names this exact risk) would
        // otherwise reach `CompiledFn::new_multi`'s `create_jit_execution_engine`
        // as malformed IR — undefined behavior in LLVM itself, not a
        // catchable Rust error. Verifying first turns that into a clean
        // `Panic` instead.
        module.borrow().verify().map_err(|e| EvalError::Panic(format!("compile: module failed verification: {}", e)))?;
        let internal_names: Vec<String> = members.iter().map(|m| crate::compile::symbols::user_symbol_name(m)).collect();
        let compiled_fns = crate::compile::CompiledFn::new_multi(&module.borrow(), &internal_names, &externals)
            .map_err(|e| EvalError::Panic(format!("compile: JIT failed: {}", e)))?;
        for (member, compiled) in members.iter().zip(compiled_fns) {
            match self.method_key(member) {
                Some((type_path, method)) => {
                    let f = self.root.borrow().get_method(&type_path, &method).expect("member is a registered method");
                    *f.compiled.borrow_mut() = Some(Rc::new(compiled));
                }
                None => {
                    let f = self.root.borrow().get_fn(&fn_path_from_node_name(member)).expect("member is a registered function");
                    *f.compiled.borrow_mut() = Some(Rc::new(compiled));
                }
            }
        }
        // After the addresses above are in place, never before: a method
        // that lands in some vtable's slot may well be a member of *this*
        // SCC, so its entry point only exists as of the loop just above.
        self.publish_vtables();
        Ok(())
    }

    /// Build the argument vector for a macro call: the first `fixed` raw
    /// forms map 1:1 to `RtValue::Sexpr`; if `f.rest`, every remaining raw
    /// form is collected into a single heap-allocated `Sexpr` list (built
    /// back-to-front, like `Self::alloc_quoted`'s `Cons` case) bound to the
    /// last parameter. Each element is already rooted by the caller (it's in
    /// `raw_args`, individually pushed in `Self::expand_macro`); only the
    /// growing `list` accumulator needs protecting around each `cons` call.
    /// Bind a macro call's raw (unevaluated) argument forms to its parameters,
    /// producing one `RtValue::Sexpr` per parameter in `f.params` order —
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
                    crate::compile::aot::compile_file(&source_path, &output_path)
                        .map(|()| Value::Bool(true))
                        .map_err(|e| EvalError::Panic(format!("compile-file: {}", e))),
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
            "parse-int" => Some(eval_parse_int(heap, args)),
            "parse-float" => Some(eval_parse_float(heap, args)),
            "read" => Some(eval_read(heap, args)),
            "eval" => Some(self.eval_form(heap, &args[0])),
            // The runtime side of the `format`/`print`/`println` special forms
            // (`Checker::check_format`/`check_print_like`): each lowers to a
            // synthetic call to one of these three names, with `args[0]`
            // (for `format-rt` a `bool` destination, otherwise the control
            // string) and a trailing `Sexpr` list of the already-`Sexpr`-wrapped
            // directive arguments. The CL-style directive engine itself is
            // `Self::run_format`. `format-rt` returns the built string (having
            // also written it to stdout when the destination is `true`, CL's
            // `t`); `print-rt`/`println-rt` return `unit` after writing (the
            // latter with a trailing newline).
            "format-rt" => {
                let dest = match expect_bool(&args[0]) {
                    Ok(b) => b,
                    Err(e) => return Some(Err(e)),
                };
                let control = match expect_str(heap, &args[1]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let list = args[2];
                Some((|| {
                    let out = self.build_format(heap, &control, list)?;
                    // The string `format` returns is always the laid-out text.
                    // Writing it to stdout, though, goes through `emit`, which
                    // merges it into an open `pprint-logical-block` instead of
                    // jumping the queue past that block's buffered output.
                    let text = crate::eval::format::finish(out.clone(), &self.pretty_opts(heap));
                    if dest {
                        self.emit(heap, out, false)?;
                    }
                    Ok(str_rt(heap, text))
                })())
            }
            "print-rt" | "println-rt" => {
                let control = match expect_str(heap, &args[0]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let list = args[1];
                let newline = name == "println-rt";
                Some((|| {
                    let out = self.build_format(heap, &control, list)?;
                    self.emit(heap, out, newline)
                })())
            }
            // The runtime side of the `pprint`/`pprint-fill`/`pprint-linear`/
            // `pprint-tabular` special forms (`Checker::check_pprint`), which
            // pass the form's own name so one builtin serves all four:
            // `args[0]` names the layout, `args[1]` is the `Sexpr`-wrapped
            // object and `args[2]` is `pprint-tabular`'s column width.
            //
            // These pretty-print unconditionally (CL defines `pprint` as
            // printing "as if `*print-pretty*` were true"), but still honor
            // `*print-right-margin*`/`*print-miser-width*`. Following CLHS,
            // `pprint` emits a newline *before* the object and none after,
            // while the three layout-specific ones emit no newline at all.
            "pprint-rt" => {
                let form = match expect_str(heap, &args[0]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let value = args[1];
                let colinc = match rt_i64(&args[2]) {
                    Ok(n) => n,
                    Err(e) => return Some(Err(e)),
                };
                use crate::eval::pprint::Style;
                let style = match form.as_str() {
                    "pprint-fill" => Style::Fill,
                    "pprint-linear" => Style::Linear,
                    // CL's `pprint-tabular` defaults its column width to 16;
                    // `check_pprint` passes 0 when the caller omitted it.
                    "pprint-tabular" => Style::Tabular(if colinc <= 0 { 16 } else { colinc }),
                    _ => Style::Default,
                };
                Some((|| {
                    let (_, enums) = self.root.borrow().collect_struct_and_enum_types();
                    let ctx = self.render_ctx(heap, &enums);
                    let mut out = crate::eval::pprint::Out::new();
                    if form == "pprint" {
                        out.push('\n');
                    }
                    // Unconditionally pretty: `render` always records the
                    // layout ops, and `emit`'s layout pass runs whenever any
                    // op is present — `*print-pretty*` only gates
                    // `~a`/`~s`/`~w`.
                    crate::eval::pprint::render(heap, ctx, value, true, style, &mut out)
                        .map_err(EvalError::Panic)?;
                    self.emit(heap, out, false)
                })())
            }
            // The user-callable pretty-printer API (CLHS 22.2.1's `pprint-*`
            // operators). `pprint-block-start-rt`/`pprint-block-end-rt` are
            // what the `pprint-logical-block` special form lowers to; the rest
            // are ordinary builtins taking CL's keyword arguments as the
            // self-evaluating symbols typelisp already has. Each is a no-op
            // outside a logical block, as CL's are on a non-pretty stream.
            "pprint-block-start-rt" => {
                let obj = args[0];
                let prefix = match expect_str(heap, &args[1]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let per_line = match expect_bool(&args[2]) {
                    Ok(b) => b,
                    Err(e) => return Some(Err(e)),
                };
                let suffix = match expect_str(heap, &args[3]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                self.pprint_block_start(heap, obj, &prefix, per_line, &suffix);
                Some(Ok(Value::Empty))
            }
            "pprint-block-end-rt" => Some(self.pprint_block_end()),
            "pprint-newline" => {
                let kind = match pprint_keyword(heap, &args[0]) {
                    Ok(k) => k,
                    Err(e) => return Some(Err(e)),
                };
                use crate::eval::pprint::NewlineKind;
                let kind = match kind {
                    ":linear" => NewlineKind::Linear,
                    ":fill" => NewlineKind::Fill,
                    ":miser" => NewlineKind::Miser,
                    ":mandatory" => NewlineKind::Mandatory,
                    other => {
                        return Some(Err(EvalError::Panic(format!(
                            "pprint-newline: expected :linear, :fill, :miser or :mandatory, got {}",
                            other
                        ))))
                    }
                };
                self.pprint_op(crate::eval::pprint::Op::Newline(kind));
                Some(Ok(Value::Empty))
            }
            "pprint-indent" => {
                let kind = match pprint_keyword(heap, &args[0]) {
                    Ok(k) => k,
                    Err(e) => return Some(Err(e)),
                };
                let n = match rt_i64(&args[1]) {
                    Ok(n) => n,
                    Err(e) => return Some(Err(e)),
                };
                use crate::eval::pprint::IndentKind;
                let kind = match kind {
                    ":block" => IndentKind::Block,
                    ":current" => IndentKind::Current,
                    other => {
                        return Some(Err(EvalError::Panic(format!(
                            "pprint-indent: expected :block or :current, got {}",
                            other
                        ))))
                    }
                };
                self.pprint_op(crate::eval::pprint::Op::Indent(kind, n));
                Some(Ok(Value::Empty))
            }
            "pprint-tab" => {
                let kind = match pprint_keyword(heap, &args[0]) {
                    Ok(k) => k,
                    Err(e) => return Some(Err(e)),
                };
                let colnum = match rt_i64(&args[1]) {
                    Ok(n) => n,
                    Err(e) => return Some(Err(e)),
                };
                let colinc = match rt_i64(&args[2]) {
                    Ok(n) => n,
                    Err(e) => return Some(Err(e)),
                };
                use crate::eval::pprint::TabKind;
                let kind = match kind {
                    ":line" => TabKind::Line,
                    ":section" => TabKind::Section,
                    ":line-relative" => TabKind::LineRelative,
                    ":section-relative" => TabKind::SectionRelative,
                    other => {
                        return Some(Err(EvalError::Panic(format!(
                            "pprint-tab: expected :line, :section, :line-relative or :section-relative, got {}",
                            other
                        ))))
                    }
                };
                self.pprint_op(crate::eval::pprint::Op::Tab { kind, colnum, colinc });
                Some(Ok(Value::Empty))
            }
            "pprint-pop" => Some(Ok(self.pprint_pop(heap))),
            "pprint-list-exhausted" => Some(Ok(Value::Bool(self.pprint_list_exhausted(heap)))),
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
            // `Sexpr::Sym` (`RtValue::Sexpr(Value::Symbol(id))`), so
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

    /// The [`RenderCtx`](crate::eval::format::RenderCtx) a printing operation
    /// runs under: the enum-variant name table plus this interpreter, so a
    /// value's own `print-object` method can be dispatched to, plus the
    /// `*print-circle*`/`*print-level*`/`*print-length*` snapshot
    /// ([`Self::print_limits`]).
    fn render_ctx<'a>(
        &'a self,
        heap: &Heap,
        enums: &'a HashMap<Path, EnumDef>,
    ) -> crate::eval::format::RenderCtx<'a> {
        crate::eval::format::RenderCtx { enums, interp: Some(self), limits: self.print_limits(heap) }
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
    pub(crate) fn print_limits(&self, heap: &Heap) -> crate::eval::format::Limits {
        let root = self.root.borrow();
        let read_limit = |name: &str| -> Option<usize> {
            match root.get_global(&crate::Path::root(name)).map(|s| s.get(heap)) {
                Some(Value::Int(n)) if n > 0 => Some(n as usize),
                _ => None,
            }
        };
        crate::eval::format::Limits {
            circle: matches!(
                root.get_global(&crate::Path::root("*print-circle*")).map(|s| s.get(heap)),
                Some(Value::Bool(true))
            ),
            level: read_limit("*print-level*"),
            length: read_limit("*print-length*"),
        }
    }

    /// Builds `control`'s output without committing it: the shared half of
    /// `format-rt`/`print-rt`/`println-rt`. Kept separate from [`Self::emit`]
    /// because a buffer that still carries pretty-printer ops must be merged
    /// into an open [`PrettySession`] *un*-laid-out — laying it out early
    /// would freeze line breaks chosen against the wrong starting column and
    /// without the enclosing block's indentation.
    fn build_format(&self, heap: &mut Heap, control: &str, args: Value) -> Result<pprint::Out, EvalError> {
        let (_, enums) = self.root.borrow().collect_struct_and_enum_types();
        let opts = self.pretty_opts(heap);
        let ctx = self.render_ctx(heap, &enums);
        crate::eval::format::build(heap, ctx, control, args, &opts).map_err(EvalError::Panic)
    }

    /// Commits printed output: into the open [`PrettySession`] if there is
    /// one, otherwise laid out and written straight to stdout.
    fn emit(&self, heap: &Heap, mut out: pprint::Out, newline: bool) -> Result<Value, EvalError> {
        if newline {
            out.push('\n');
        }
        // `pretty_opts` itself reads `self.pretty`, so it must not run while
        // this borrow is held.
        let buffered = match self.pretty.borrow_mut().as_mut() {
            Some(s) => {
                s.out.append(out);
                None
            }
            None => Some(out),
        };
        match buffered {
            Some(out) => {
                let opts = self.pretty_opts(heap);
                write_stdout(&crate::eval::format::finish(out, &opts), false)
            }
            None => Ok(Value::Empty),
        }
    }

    /// Opens a logical block, starting a [`PrettySession`] if this is the
    /// outermost one. `obj` is the list `pprint-pop` walks (`()` when the
    /// block iterates nothing).
    pub(crate) fn pprint_block_start(
        &self,
        heap: &mut Heap,
        obj: Value,
        prefix: &str,
        per_line: bool,
        suffix: &str,
    ) {
        let opts = pprint::Opts { pretty: true, ..self.pretty_opts(heap) };
        let cell = heap.alloc_cell(obj);
        let mut session = self.pretty.borrow_mut();
        let s = session.get_or_insert_with(|| PrettySession { out: pprint::Out::new(), lists: Vec::new(), opts });
        s.out.op(pprint::Op::BlockStart {
            prefix: prefix.to_string(),
            per_line,
            suffix: suffix.to_string(),
        });
        s.lists.push(cell);
    }

    /// Closes a logical block; closing the outermost one lays the whole
    /// session out and writes it to stdout.
    pub(crate) fn pprint_block_end(&self) -> Result<Value, EvalError> {
        let finished = {
            let mut session = self.pretty.borrow_mut();
            let Some(s) = session.as_mut() else {
                return Ok(Value::Empty);
            };
            s.out.op(pprint::Op::BlockEnd);
            s.lists.pop();
            if s.lists.is_empty() {
                session.take()
            } else {
                None
            }
        };
        match finished {
            // `s.opts` was snapshotted with `pretty` forced on at block start:
            // an explicit `pprint-logical-block` is a request to pretty-print,
            // exactly as `pprint` is.
            Some(s) => write_stdout(&crate::eval::format::finish(s.out, &s.opts), false),
            None => Ok(Value::Empty),
        }
    }

    /// Closes and writes out a pretty-printing session left open by a
    /// non-local exit (see [`Self::exec`]). A no-op in the normal case, where
    /// the matching `pprint-block-end-rt` already flushed it.
    fn flush_pretty(&self) -> Result<(), EvalError> {
        let Some(s) = self.pretty.borrow_mut().take() else {
            return Ok(());
        };
        let opts = pprint::Opts { pretty: true, ..s.opts };
        // `layout` closes whatever blocks are still open, emitting their
        // suffixes, so the partial output is still well-formed.
        write_stdout(&crate::eval::format::finish(s.out, &opts), false).map(|_| ())
    }

    /// Records a pretty-printer op on the open session. A no-op with no
    /// session open, matching CL, where `pprint-newline` and friends do
    /// nothing unless the stream really is a pretty stream.
    fn pprint_op(&self, op: pprint::Op) {
        if let Some(s) = self.pretty.borrow_mut().as_mut() {
            s.out.op(op);
        }
    }

    /// `pprint-pop`: the next element of the innermost open block's list, or
    /// `()` when it is exhausted (`pprint-list-exhausted` is the predicate to
    /// check first). Advances the stored tail in place.
    fn pprint_pop(&self, heap: &mut Heap) -> Value {
        let cell = match self.pretty.borrow().as_ref().and_then(|s| s.lists.last().cloned()) {
            Some(c) => c,
            None => return Value::Empty,
        };
        let rest = heap.cell_get(*cell);
        let Ok(head) = heap.car(rest) else { return Value::Empty };
        let tail = heap.cdr(rest).unwrap_or(Value::Empty);
        heap.cell_set(*cell, tail);
        head
    }

    /// `pprint-list-exhausted`: whether the innermost open block's list has
    /// nothing left (also true when there is no open block at all).
    fn pprint_list_exhausted(&self, heap: &Heap) -> bool {
        match self.pretty.borrow().as_ref().and_then(|s| s.lists.last().cloned()) {
            Some(cell) => !heap.cell_get(*cell).is_cons(),
            None => true,
        }
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
        let root = self.root.borrow();
        let read_int = |name: &str, default: i64| -> i64 {
            match root.get_global(&crate::Path::root(name)).map(|s| s.get(heap)) {
                Some(Value::Int(n)) => n,
                _ => default,
            }
        };
        // Inside a `pprint-logical-block` the "stream" *is* a pretty stream,
        // so everything printed into it pretty-prints regardless of the
        // global — the same thing CL's stream-type dispatch achieves.
        let pretty = self.pretty.borrow().is_some()
            || matches!(
                root.get_global(&crate::Path::root("*print-pretty*")).map(|s| s.get(heap)),
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
    fn eval_form(&self, heap: &mut Heap, arg: &Value) -> Result<Value, EvalError> {
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
                Some(rt) => rtvalue_to_sexpr(&rt),
                // `use`/`module`, or an empty body — nothing to hand back but `()`.
                None => Value::Empty,
            },
        };
        Ok(result_ok(heap, result))
    }
}

/// Intern a name set into the `SymId`s the core bridge compares by.
fn intern_names(heap: &mut Heap, names: &HashSet<String>) -> HashSet<SymId> {
    names
        .iter()
        .map(|n| match heap.intern_symbol(n) {
            Value::Symbol(id) => id,
            _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
        })
        .collect()
}

/// The same for a parameter list, keeping each name's representation.
fn intern_params(heap: &mut Heap, params: &[(String, Repr)]) -> Vec<(SymId, Repr)> {
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
    /// wrap each raw (unevaluated) argument form as `RtValue::Sexpr` with no
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
/// represented by [`RtValue::Sexpr`] (heap-backed) rather than
/// [`RtValue::Data`].
fn is_sexpr_type(type_name: &Path) -> bool {
    *type_name == Path::root("sexpr")
}

fn rt_i64(v: &Value) -> Result<i64, EvalError> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(EvalError::Internal("sexpr: expected an i64 field".into())),
    }
}

fn rt_f64(heap: &Heap, v: &Value) -> Result<f64, EvalError> {
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
/// (`registry::int_assoc`) — shared by both widths since `RtValue::Int`
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
            Value::Int(a / b)
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
/// widths share `RtValue::Int(i64)` at runtime (see `eval_int_builtin`'s doc
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
fn str_rt(heap: &mut Heap, s: impl Into<String>) -> Value {
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

/// One step of a 64-bit xorshift generator (period `2^64 - 1` over the
/// nonzero states — these exact shift/xor constants are a full-cycle
/// permutation of them, so a nonzero seed can never reach `0`) — the
/// bit-twiddling behind every `random-state` draw. Not cryptographically
/// secure and not expressible in typelisp itself (no bitwise operators),
/// matching `gensym`'s "collision-resistant, not unforgeable" precedent for
/// what a builtin without a real entropy/hygiene API can promise.
fn xorshift64_step(x: u64) -> u64 {
    let mut x = x;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// A fresh entropy seed for `make-random-state-fresh` — `| 1` guarantees
/// non-zero (the one fixed point `xorshift64_step` can't escape).
fn fresh_random_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        | 1
}

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
    const UNIX_TO_CL_EPOCH_SECS: i64 = 2_208_988_800;
    let unix_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(Value::Int(unix_secs + UNIX_TO_CL_EPOCH_SECS))
}

/// `get-internal-real-time` (CLHS 25.1): elapsed `internal-time-units-per-
/// second` (the prelude's `defvar`, 1_000_000 — i.e. microseconds) since an
/// arbitrary reference point fixed at first call — a monotonic
/// `std::time::Instant`, not wall-clock time, so `time`'s elapsed-time
/// measurement can't go backwards under a clock adjustment.
fn eval_get_internal_real_time(_args: &[Value]) -> Result<Value, EvalError> {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    Ok(Value::Int(start.elapsed().as_micros() as i64))
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

/// Reads a `pprint-*` builtin's keyword argument (`:linear`, `:block`, …).
/// Keywords are ordinary interned symbols whose name keeps the leading colon
/// (`sym`), so this is just "the symbol's name".
fn pprint_keyword<'a>(heap: &'a Heap, v: &Value) -> Result<&'a str, EvalError> {
    match v {
        Value::Symbol(id) => Ok(heap.symbol_name(*id)),
        _ => Err(EvalError::Panic("expected a keyword argument such as :linear".into())),
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
    let s = expect_str(heap, &args[0])?;
    match s.parse::<i32>() {
        Ok(n) => Ok(result_ok(heap, Value::Int(n as i64))),
        Err(_) => Ok(result_err(heap, PARSE_INT_ERROR, format!("parse-int: invalid integer literal: {:?}", s))),
    }
}

/// `parse-float` (`registry.rs`'s free-function entry): an `f64` literal via
/// `str::parse` (accepts everything Rust's own `FromStr for f64` does,
/// including `inf`/`nan`), `Err` on anything else.
fn eval_parse_float(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let s = expect_str(heap, &args[0])?;
    match s.parse::<f64>() {
        Ok(f) => {
            let v = float_rt(heap, f);
            Ok(result_ok(heap, v))
        }
        Err(_) => Ok(result_err(heap, PARSE_FLOAT_ERROR, format!("parse-float: invalid float literal: {:?}", s))),
    }
}

/// `read` (`registry.rs`'s free-function entry): parses exactly one `Sexpr`
/// form from `s` via the ordinary reader (`crate::read::Reader::read`) —
/// the same pipeline `typl`/the REPL use for source text, just callable at
/// runtime on a string value instead of a file/stdin. `Err` (not a panic)
/// on malformed input, e.g. an unterminated list or string — this reads
/// data the running program doesn't control.
fn eval_read(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let s = expect_str(heap, &args[0])?.to_string();
    let reader = crate::read::Reader::new();
    match reader.read(heap, &s) {
        Ok(v) => Ok(result_ok(heap, v)),
        Err(e) => Ok(result_err(heap, READ_ERROR, format!("read: {}", e))),
    }
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
            // `eq`/`eql`: true identity (`Rc::ptr_eq` — see `RtValue::Str`'s
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
    if *type_name == Path::root("i32") || *type_name == Path::root("i64") {
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
    if *type_name == Path::root("f64") {
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

/// The (typelisp-hosted) compiler's view of LLVM — `llvm-module`/
/// `llvm-function`/`llvm-builder`/`llvm-value` instance and static methods.
/// Same metadata-only pattern as the rest of `eval_builtin_method` (the
/// `AdtDef`s in `registry::llvm_module_def` etc. carry no `defmethod` body).
/// Every arm holds [`crate::compile::COMPILE_LOCK`] for its duration — see
/// that constant's doc comment for why concurrent access to the one
/// process-wide LLVM `Context` must never happen.
fn eval_llvm_builtin_method(heap: &mut Heap, type_name: &Path, method: &str, args: &[Value]) -> Option<Result<Value, EvalError>> {
    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    if *type_name == Path::root("llvm-module") {
        return match method {
            "create" => Some(llvm_module_create(heap, args)),
            "add-function" => Some(llvm_module_add_function(heap, args)),
            "verify" => Some(llvm_module_verify(args)),
            "to-string" => Some(llvm_module_to_string(heap, args)),
            "get-function" => Some(llvm_module_get_function(heap, args)),
            "add-function-with-env" => Some(llvm_module_add_function_with_env(heap, args)),
            _ => None,
        };
    }
    if *type_name == Path::root("llvm-function") {
        return match method {
            "append-block" => Some(llvm_function_append_block(heap, args)),
            _ => None,
        };
    }
    if *type_name == Path::root("llvm-builder") {
        return match method {
            "create" => Some(llvm_builder_create()),
            "position-at-end" => Some(llvm_builder_position_at_end(args)),
            "const-i64" => Some(llvm_builder_const_i64(args)),
            "build-ret" => Some(llvm_builder_build_ret(args)),
            "load-arg" => Some(llvm_builder_load_arg(args)),
            "build-add" => Some(llvm_builder_build_int_op(args, "add", Builder::build_int_add)),
            "build-sub" => Some(llvm_builder_build_int_op(args, "sub", Builder::build_int_sub)),
            "build-mul" => Some(llvm_builder_build_int_op(args, "mul", Builder::build_int_mul)),
            "build-and" => Some(llvm_builder_build_int_op(args, "and", Builder::build_and)),
            "build-or" => Some(llvm_builder_build_int_op(args, "or", Builder::build_or)),
            "build-xor" => Some(llvm_builder_build_int_op(args, "xor", Builder::build_xor)),
            "build-select" => Some(llvm_builder_build_select(args)),
            "build-shl" => Some(llvm_builder_build_int_op(args, "shl", Builder::build_left_shift)),
            "build-lshr" => Some(llvm_builder_build_int_op(args, "lshr", |b, lhs, rhs, name| b.build_right_shift(lhs, rhs, false, name))),
            "build-ashr" => Some(llvm_builder_build_int_op(args, "ashr", |b, lhs, rhs, name| b.build_right_shift(lhs, rhs, true, name))),
            "build-fadd" => Some(llvm_builder_build_float_op(args, "fadd", Builder::build_float_add)),
            "build-fsub" => Some(llvm_builder_build_float_op(args, "fsub", Builder::build_float_sub)),
            "build-fmul" => Some(llvm_builder_build_float_op(args, "fmul", Builder::build_float_mul)),
            "build-fdiv" => Some(llvm_builder_build_float_op(args, "fdiv", Builder::build_float_div)),
            "build-frem" => Some(llvm_builder_build_float_op(args, "frem", Builder::build_float_rem)),
            "build-fcmp-lt" => Some(llvm_builder_build_fcmp(args, "fcmp_lt", inkwell::FloatPredicate::OLT)),
            "build-fcmp-le" => Some(llvm_builder_build_fcmp(args, "fcmp_le", inkwell::FloatPredicate::OLE)),
            "build-fcmp-gt" => Some(llvm_builder_build_fcmp(args, "fcmp_gt", inkwell::FloatPredicate::OGT)),
            "build-fcmp-ge" => Some(llvm_builder_build_fcmp(args, "fcmp_ge", inkwell::FloatPredicate::OGE)),
            "build-fcmp-eq" => Some(llvm_builder_build_fcmp(args, "fcmp_eq", inkwell::FloatPredicate::OEQ)),
            "build-fcmp-ne" => Some(llvm_builder_build_fcmp(args, "fcmp_ne", inkwell::FloatPredicate::UNE)),
            "build-fsqrt" => Some(llvm_builder_build_float_unary_intrinsic(args, "fsqrt", "llvm.sqrt.f64")),
            "build-ffloor" => Some(llvm_builder_build_float_unary_intrinsic(args, "ffloor", "llvm.floor.f64")),
            "build-fceil" => Some(llvm_builder_build_float_unary_intrinsic(args, "fceil", "llvm.ceil.f64")),
            "build-fround" => Some(llvm_builder_build_float_unary_intrinsic(args, "fround", "llvm.round.f64")),
            "build-ftrunc" => Some(llvm_builder_build_float_unary_intrinsic(args, "ftrunc", "llvm.trunc.f64")),
            "build-fpow" => Some(llvm_builder_build_fpow(args)),
            "build-fmaxnum" => Some(llvm_builder_build_float_binary_intrinsic(args, "fmaxnum", "llvm.maxnum.f64")),
            "build-fminnum" => Some(llvm_builder_build_float_binary_intrinsic(args, "fminnum", "llvm.minnum.f64")),
            "build-fsin" => Some(llvm_builder_build_float_unary_intrinsic(args, "fsin", "llvm.sin.f64")),
            "build-fcos" => Some(llvm_builder_build_float_unary_intrinsic(args, "fcos", "llvm.cos.f64")),
            "build-fexp" => Some(llvm_builder_build_float_unary_intrinsic(args, "fexp", "llvm.exp.f64")),
            "build-flog" => Some(llvm_builder_build_float_unary_intrinsic(args, "flog", "llvm.log.f64")),
            "build-fptosi" => Some(llvm_builder_build_fptosi(args)),
            "alloca-args" => Some(llvm_builder_alloca_args(args)),
            "store-arg" => Some(llvm_builder_store_arg(args)),
            "build-call" => Some(llvm_builder_build_call(args)),
            "load-env" => Some(llvm_builder_load_env(args)),
            "build-call-with-env" => Some(llvm_builder_build_call_with_env(args)),
            "build-make-closure" => Some(llvm_builder_build_make_closure(args)),
            "build-closure-apply" => Some(llvm_builder_build_closure_apply(args)),
            "build-dyn-call" => Some(llvm_builder_build_dyn_call(args)),
            "load-raw" => Some(llvm_builder_load_raw(args)),
            "build-icmp-lt" => Some(llvm_builder_build_icmp(args, "icmp_lt", inkwell::IntPredicate::SLT)),
            "build-icmp-le" => Some(llvm_builder_build_icmp(args, "icmp_le", inkwell::IntPredicate::SLE)),
            "build-icmp-gt" => Some(llvm_builder_build_icmp(args, "icmp_gt", inkwell::IntPredicate::SGT)),
            "build-icmp-ge" => Some(llvm_builder_build_icmp(args, "icmp_ge", inkwell::IntPredicate::SGE)),
            "build-icmp-eq" => Some(llvm_builder_build_icmp(args, "icmp_eq", inkwell::IntPredicate::EQ)),
            "build-icmp-ne" => Some(llvm_builder_build_icmp(args, "icmp_ne", inkwell::IntPredicate::NE)),
            "build-cond-br" => Some(llvm_builder_build_cond_br(args)),
            "build-br" => Some(llvm_builder_build_br(args)),
            "block-terminated?" => Some(llvm_builder_block_terminated(args)),
            "build-malloc" => Some(llvm_builder_build_malloc(args)),
            "build-free" => Some(llvm_builder_build_free(args)),
            "build-int-to-ptr" => Some(llvm_builder_build_int_to_ptr(args)),
            "build-ptr-to-int" => Some(llvm_builder_build_ptr_to_int(args)),
            _ => None,
        };
    }
    None
}

// The five accessors below resolve a handle integer back to its native
// object. They return owned values (an `Rc` clone for the two shared ones)
// rather than references, because the object lives in a thread-local registry
// that cannot lend out a borrow.

fn expect_llvm_module(v: &Value) -> Result<Rc<RefCell<Module<'static>>>, EvalError> {
    match expect_handle(v, "LlvmModule")? {
        NativeHandle::Module(m) => Ok(m),
        _ => Err(EvalError::Internal(format!("expected an LlvmModule, got {:?}", v))),
    }
}

fn expect_llvm_function(v: &Value) -> Result<FunctionValue<'static>, EvalError> {
    match expect_handle(v, "LlvmFunction")? {
        NativeHandle::Function(f) => Ok(f),
        _ => Err(EvalError::Internal(format!("expected an LlvmFunction, got {:?}", v))),
    }
}

fn expect_llvm_builder(v: &Value) -> Result<Rc<RefCell<Builder<'static>>>, EvalError> {
    match expect_handle(v, "LlvmBuilder")? {
        NativeHandle::Builder(b) => Ok(b),
        _ => Err(EvalError::Internal(format!("expected an LlvmBuilder, got {:?}", v))),
    }
}

fn expect_llvm_basic_block(v: &Value) -> Result<BasicBlock<'static>, EvalError> {
    match expect_handle(v, "LlvmBasicBlock")? {
        NativeHandle::BasicBlock(b) => Ok(b),
        _ => Err(EvalError::Internal(format!("expected an LlvmBasicBlock, got {:?}", v))),
    }
}

fn expect_llvm_value(v: &Value) -> Result<BasicValueEnum<'static>, EvalError> {
    match expect_handle(v, "LlvmValue")? {
        NativeHandle::Value(x) => Ok(x),
        _ => Err(EvalError::Internal(format!("expected an LlvmValue, got {:?}", v))),
    }
}

fn llvm_module_create(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let name = expect_str(heap, &args[0])?;
    let module = crate::compile::llvm_context().create_module(name);
    Ok(llvm_module_value(module))
}

/// Every compiled function gets the same fixed C ABI — `i64 name(i64* args,
/// i32 argc)` — regardless of its typelisp-level arity (see
/// `registry::llvm_module_def`'s doc comment for why); `llvm-builder::load-arg`
/// reads a logical parameter back out of `args`. LLVM 17 defaults to opaque
/// pointers (inkwell's `llvm17-0` feature doesn't pull in its
/// `typed-pointers` feature — confirmed against inkwell's own `Cargo.toml`),
/// so the parameter type is `Context::ptr_type`, not `IntType::ptr_type`.
/// Shared by [`llvm_module_add_function`] and [`declare_external_function`]
/// (labels/closures Stage 3's JIT-only forward declarations) — both declare
/// a function under this exact same signature, just with or without a body.
fn compiled_fn_type() -> inkwell::types::FunctionType<'static> {
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false)
}

/// Get-or-create: reuses an existing declaration under `name` (a bodyless
/// forward declaration — [`declare_external_function`] or an earlier call to
/// this very builtin) instead of always minting a fresh one. LLVM's own
/// `LLVMAddFunction` does *not* do this — a second call with a colliding
/// name silently gets uniquified (`"name.1"`), never merged with the first —
/// so this reuse has to happen here. Needed since labels/closures Stage 5:
/// [`Interp::compile_scc`] forward-declares every member of a mutually
/// recursive group under its real internal name *before* any member's body
/// is translated, and `compiler.rs`'s `compile-function` (the sole caller of
/// this builtin) must attach that member's own body to that exact same
/// declaration, not a second, disconnected one — otherwise a sibling's call
/// to it (resolved by name against the module) would find only the empty
/// declaration. A no-op generalization for every call site that predates
/// Stage 5: none of them ever collided with a pre-existing declaration under
/// the same name.
fn llvm_module_add_function(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let existing = module.borrow().get_function(name);
    let function = existing.unwrap_or_else(|| module.borrow_mut().add_function(name, compiled_fn_type(), None));
    Ok(llvm_function_value(function))
}

/// Forward-declares `name` in `module` with the standard compiled-function
/// ABI but **no body** — [`Interp::compile_function`]'s JIT-only step
/// (labels/closures Stage 3, `call` to a different top-level function)
/// that lets `compiler.rs`'s `compile-call` find an already-`compile`d
/// function via `get-function` before the real call target is wired in via
/// `add_global_mapping` once the engine running this declaration's own
/// module exists (see that method's doc comment). Not exposed as an
/// `llvm-*` builtin — unlike [`llvm_module_add_function`], the typelisp
/// compiler body itself never needs to call this; only the Rust-side JIT
/// orchestration above does. Must be called with
/// [`crate::compile::COMPILE_LOCK`] held.
fn declare_external_function(module: &Rc<RefCell<Module<'static>>>, name: &str) {
    module.borrow_mut().add_function(name, compiled_fn_type(), None);
}

/// The single canonical `"type-path::method"` string for a method — `Path`'s
/// own `Display` for `type_name` (its full `::`-joined path, never just its
/// local segment — see [`Interp::method_key`]'s doc comment for why), `"::"`,
/// then `method`. This is the LLVM-visible name a method's own compiled
/// function is declared/looked-up under — exactly the literal string a
/// standalone `(compile "type-path::method")` call uses as its
/// `internal_name` ([`Interp::compile_function`]'s own
/// `self.add_compiled_function(heap, module.clone(), name, name)` call,
/// where `name` is that literal user-typed string) — but also every other
/// place this crate needs the same "which method" identity as plain text: a
/// `NoSuchFunction` error, [`CallEdge::Method`]'s own SCC graph node name.
/// One shared helper keeps all of them in lockstep rather than each
/// re-deriving the same format independently. Also relied on by
/// `compiler.rs`'s `compile-assoc` (looking the same name back up via
/// `get-function` — see that function's doc comment).
fn method_link_name(type_name: &Path, method: &str) -> String {
    format!("{}::{}", type_name, method)
}

/// True for the handful of builtins `compiler.rs`'s `compile-call` rewrites to
/// a `crate::compile::runtime` shim by name (`sexpr-car` -> `rt_car`, etc. —
/// see that function's `raw-nm`/`nm` rename) rather than requiring `(compile
/// ...)` first: these can never be `compile`d themselves (no typelisp AST body
/// — direct cons-heap access, Rust-only), so [`Interp::compile_function`]
/// excludes them from its normal "every call target must already be compiled"
/// check and instead always wires them via [`rt_extern_functions`]. These are
/// the `sexpr-*` island layer (Symbol/Sexpr redesign Phase 4b) — the whole
/// family since closure unification Stage 8 (tag predicates and typed
/// payload extractors included, not just `car`/`cdr`/`cons`). The free
/// `car`/`cdr`/`cons` names are the `cons<T,U>` pair (an ordinary
/// `defstruct` method / `defun`, compiled the normal way), not `rt_*` shims.
pub(crate) fn is_rt_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "sexpr-car"
            | "sexpr-cdr"
            | "sexpr-cons"
            | "sexpr-consp"
            | "sexpr-null"
            | "sexpr-atom"
            | "sexpr-symp"
            | "sexpr-int"
            | "sexpr-bool"
            | "sexpr-char"
            | "sexpr-float"
            | "sexpr-str"
            | "sexpr-sym-name"
            // `gensym`: a free builtin (not a `sexpr-*` accessor) with no
            // typelisp body — `compile-call` rewrites it to the `rt_gensym`
            // shim, the same wiring as the accessors above. It must be here so
            // a macro-expansion lambda that calls it (e.g. `do`'s per-binding
            // temporaries) doesn't send `call_graph_edges` looking for a
            // (nonexistent) `gensym` function to transitively compile.
            | "gensym"
    )
}

/// Whether a builtin method on a primitive receiver (`i64`/`i32`/`char`/
/// `string`/`f64`/`bignum`/`ratio`) is one `compiler.rs`'s `compile-assoc`
/// lowers *natively* — to an inline LLVM instruction or an `rt_*` call —
/// rather than to an ordinary function call that would need the method
/// `compile`d as its own function first. The Rust-side twin of the island's
/// own `int-native-method?`/`string-native-method?`/`char-native-method?`/
/// `float-native-method?`/`bignum-native-method?`/`ratio-native-method?`
/// predicates (`compiler.rs`'s `SOURCE`); the lists must stay in lockstep,
/// the same way [`Interp::heap_repr_kind`] mirrors the checker's
/// `is_heap_repr`.
///
/// [`Interp::call_graph_edges`] needs this so it can reject — cleanly, up
/// front — a compile whose body calls a *non*-native primitive builtin
/// (`i64::int->char`, `char::equalp`, ...): those have no compiled lowering
/// *and* no function to link, so left to reach the island they hit its
/// `get-function` guard, which under the AOT-native island is a hard
/// `rt_llvm_call` process abort rather than a catchable error. Catching them
/// here keeps `(compile bad-fn)` a clean `EvalError` — the behavior the
/// interpreted island used to give from `get-function` directly.
pub(crate) fn is_native_lowered_primitive_method(type_local: &str, method: &str) -> bool {
    match type_local {
        "i64" | "i32" => matches!(
            method,
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "eq" | "/=" | "int->bignum" | "int->ratio"
                | "max" | "min" | "logand" | "logior" | "logxor" | "logtest" | "lognot" | "logcount" | "integer-length"
                | "ash" | "logbitp"
        ),
        "string" => matches!(
            method,
            "length" | "ref" | "eq" | "equal" | "equalp" | "lt" | "<" | "<=" | ">" | ">=" | "append"
        ),
        "char" => matches!(
            method,
            "eq" | "eql" | "equal" | "equalp" | "lt" | "<" | "<=" | ">" | ">=" | "char->int"
                | "char->string"
        ),
        "f64" => matches!(
            method,
            "+" | "-" | "*" | "/" | "expt" | "sqrt" | "floor" | "ceiling" | "round" | "truncate"
                | "float->int" | "float->bignum" | "float->ratio"
                | "<" | "<=" | ">" | ">=" | "=" | "/=" | "eq" | "eql" | "equal" | "equalp"
                | "max" | "min" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh"
                | "asinh" | "acosh" | "atanh" | "exp" | "log"
        ),
        "bignum" => matches!(
            method,
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" | "eq" | "eql" | "equal" | "equalp"
                | "bignum->int" | "try-bignum->int" | "bignum->float" | "bignum->ratio" | "max" | "min"
        ),
        "ratio" => matches!(
            method,
            "+" | "-" | "*" | "/" | "<" | "<=" | ">" | ">=" | "=" | "/=" | "eq" | "eql" | "equal" | "equalp"
                | "ratio->bignum" | "ratio->float" | "numerator" | "denominator" | "max" | "min"
        ),
        // `Sexpr` values are raw tagged `i64` handles in compiled code, and
        // interned symbols/`nil`/small atoms are handle-identical, so `eq`
        // (CL identity) lowers to a plain `icmp eq` on the two handles —
        // `compile-assoc`'s `sexpr` arm, mirroring the `char` branch. Only
        // `eq` is native; structural `equal` stays an ordinary prelude
        // `defun` compiled the normal way.
        "sexpr" => matches!(method, "eq"),
        _ => false,
    }
}

/// The fixed set of `crate::compile::runtime` shims every compiled function
/// gets forward-declared and (JIT only — AOT resolves them as ordinary
/// linker symbols against `typelisp-rt`'s `staticlib`, see
/// `compile::aot::compile_file`) `add_global_mapping`-wired to, regardless
/// of whether its own body actually calls any of them. Cheap enough (9
/// extra declarations/mappings) to always include rather than checking
/// which ones a given body's call targets actually need. `pub(crate)`:
/// `compile::aot::compile_file` declares the same names (no JIT mapping
/// needed there — ordinary linker symbol resolution against `typelisp-rt`'s
/// `staticlib` instead) from this one source of truth.
///
/// `rt_push_sexpr_root`/`rt_pop_sexpr_root` (Stage 6 of the
/// Sexpr-representation plan, `docs/implementation-log.md` — the "Sexprルート挿入パス"):
/// unlike `rt_car`/.../`rt_match_fail`, nothing here rewrites a *user-visible*
/// call name to reach these (`is_rt_builtin_name`'s list is unchanged) —
/// `compiler.rs`'s `retain-bindings`/`release-bindings`/`bind-let-values`
/// call them directly via `get-function`/`build-call`, the same way
/// `build-make-closure`/`build-closure-apply` call `rt_closure_*` directly
/// rather than through the `(call name args)` tag. They still need
/// the same forward-declaration/global-mapping treatment as every other
/// `rt_*` shim, so they belong in this one shared list regardless.
///
/// `rt_push_permanent_sexpr_root` (general-ADT box field GC root
/// protection): `compiler.rs`'s `compile-construct-box-fields` calls this
/// directly for the same reason, one level down from a box's own
/// never-`build-free`'d field storage rather than a call-stack scope — see
/// `typelisp_rt::rt_push_permanent_sexpr_root`'s doc comment for why it has
/// no `rt_pop_permanent_sexpr_root` counterpart.
///
/// `rt_root_count`/`rt_set_sexpr_root` (the `setf`-reassignment GC-root fix):
/// `compiler.rs`'s `retain-bindings`/`bind-let-values` call `rt_root_count`
/// right before their own `rt_push_sexpr_root` call for a `kind = 2` binding,
/// to record the exact root-stack index that push lands at; `compile-set`
/// later hands that same index to `rt_set_sexpr_root` so a `setf` updates the
/// binding's *existing* root in place instead of leaving a freshly assigned
/// value with no root at all — see `typelisp_rt::rt_set_sexpr_root`'s doc
/// comment for the corruption this closes.
///
/// `rt_float_new`/`rt_float_value` (Sexpr/RtValue unification, Stage 0):
/// `compiler.rs`'s `compile-construct-sexpr`/`compile-sexpr-field` variant-2
/// arms call these to box/unbox a `Sexpr::Float` (`Value::Boxed`, see
/// `BoxedObj`) — the first `rt_*` pair for the new boxed-object store, same
/// declare-into-every-module mechanism every other `rt_*` function here
/// already uses.
///
/// `rt_struct_new`/`rt_struct_field_get`/`rt_struct_field_set`
/// (Sexpr/RtValue unification, Stage 3): `compiler.rs`'s
/// `compile-construct-boxed-struct`/`compile-field-get`/`compile-field-set`
/// call these to build/read/write a `BoxedObj::Struct` — the same
/// `BoxedObj::Struct` mem/rt-layer plumbing Stage 1 already exercised in
/// isolation, wired to the compiler for the first time here.
///
/// `rt_closure_*`/`rt_cell_*` (closure unification, Stage 1): the GC-heap
/// `BoxedObj::CompiledClosure` that replaces the raw `malloc`'d
/// reference-counted `ClosureBox`, plus the shared binding cells
/// (`BoxedObj::Cell`) captured names live in so compiled and interpreted
/// `setf` mutate the very same object.
pub(crate) fn rt_extern_functions() -> [(&'static str, usize); 119] {
    use crate::compile::runtime::{
        rt_atom, rt_bignum_add, rt_bignum_cmp, rt_bignum_div, rt_bignum_fits_i32, rt_bignum_mod, rt_bignum_mul, rt_bignum_new,
        rt_bignum_sub, rt_bignum_to_float, rt_bignum_to_int, rt_bignum_to_int_raw, rt_bignum_to_ratio, rt_box_kind, rt_car, rt_cdr,
        rt_apply_any, rt_cell_get, rt_cell_new, rt_cell_set, rt_char_equalp, rt_closure_env_get, rt_closure_env_len,
        rt_closure_fnptr, rt_closure_new, rt_cons, rt_consp, rt_data_field, rt_data_new, rt_data_variant, rt_float_new, rt_float_to_bignum,
        rt_float_to_ratio, rt_float_value, rt_gensym, rt_global_get, rt_global_new, rt_global_set, rt_i64_div, rt_i64_mod,
        rt_i64_ash, rt_i64_logbitp, rt_i64_logcount, rt_i64_integer_length,
        rt_f64_tan, rt_f64_asin, rt_f64_acos, rt_f64_atan, rt_f64_sinh, rt_f64_cosh, rt_f64_tanh, rt_f64_asinh, rt_f64_acosh,
        rt_f64_atanh,
        rt_hashtable_clear, rt_hashtable_contains, rt_hashtable_count, rt_hashtable_entries, rt_hashtable_get_raw, rt_hashtable_keys,
        rt_hashtable_new, rt_hashtable_remove_raw, rt_hashtable_set, rt_hashtable_values, rt_int_to_bignum, rt_int_to_ratio,
        rt_intern_path, rt_intern_symbol, rt_list_to_path, rt_match_fail, rt_null, rt_panic, rt_path_to_list, rt_pop_sexpr_root, rt_push_permanent_sexpr_root,
        rt_push_sexpr_root, rt_ratio_add, rt_ratio_cmp, rt_ratio_denominator, rt_ratio_div, rt_ratio_from_bignums, rt_ratio_mul,
        rt_ratio_numerator, rt_ratio_sub, rt_ratio_to_bignum, rt_ratio_to_float, rt_root_count, rt_set_car, rt_set_cdr,
        rt_set_sexpr_root, rt_sexpr_bool, rt_sexpr_char, rt_sexpr_instance_test, rt_sexpr_int, rt_sexpr_str, rt_str_append, rt_str_eq, rt_str_equalp,
        rt_str_length, rt_str_lt, rt_str_new, rt_str_ref, rt_struct_field_count, rt_struct_field_get, rt_struct_field_set,
        rt_struct_new, rt_struct_pop_field, rt_struct_push_field, rt_sym_name, rt_symp, rt_truncate_sexpr_roots,
        rt_dyn_new, rt_dyn_upcast, rt_dyn_value, rt_dyn_vtable, rt_upcast_set, rt_vtable_set, rt_vtable_slot,
    };
    [
        // The one main-crate entry: the generic `llvm-*`/native-scope
        // builtin dispatch shim (interp-closure removal Stage 1) — it can't
        // live in `typelisp-rt` because it calls into the `inkwell`-backed
        // [`eval_llvm_builtin_method`]. Never referenced by AOT-linked user
        // executables (LLVM handle types are unreachable from user code, so
        // `compile-file` output never emits a call to it — an unreferenced
        // declaration emits no symbol for the linker to miss).
        ("rt_llvm_call", rt_llvm_call as usize),
        // Trait objects and vtables (TODO T4): `rt_dyn_new` boxes,
        // `rt_dyn_vtable`/`rt_dyn_value` decode, and `rt_vtable_slot` reads
        // the native entry point a `:dyn` call site then calls indirectly
        // (`compiler.rs`'s `compile-dyn-*`). `rt_dyn_upcast` swaps a box's
        // table for a supertrait's when the two layouts share no prefix.
        // `rt_vtable_set`/`rt_upcast_set` fill the two tables from AOT
        // startup; the JIT fills them Rust-side instead
        // (`Interp::publish_vtables`/`register_dyn_box`), so nothing emits a
        // call to either there.
        ("rt_dyn_new", rt_dyn_new as usize),
        ("rt_dyn_vtable", rt_dyn_vtable as usize),
        ("rt_dyn_value", rt_dyn_value as usize),
        ("rt_dyn_upcast", rt_dyn_upcast as usize),
        ("rt_vtable_slot", rt_vtable_slot as usize),
        ("rt_vtable_set", rt_vtable_set as usize),
        ("rt_upcast_set", rt_upcast_set as usize),
        ("rt_car", rt_car as usize),
        ("rt_cdr", rt_cdr as usize),
        ("rt_cons", rt_cons as usize),
        ("rt_consp", rt_consp as usize),
        ("rt_null", rt_null as usize),
        ("rt_atom", rt_atom as usize),
        ("rt_symp", rt_symp as usize),
        ("rt_sexpr_int", rt_sexpr_int as usize),
        ("rt_sexpr_bool", rt_sexpr_bool as usize),
        ("rt_sexpr_char", rt_sexpr_char as usize),
        ("rt_sexpr_str", rt_sexpr_str as usize),
        ("rt_sym_name", rt_sym_name as usize),
        ("rt_set_car", rt_set_car as usize),
        ("rt_set_cdr", rt_set_cdr as usize),
        ("rt_match_fail", rt_match_fail as usize),
        ("rt_panic", rt_panic as usize),
        ("rt_push_sexpr_root", rt_push_sexpr_root as usize),
        ("rt_pop_sexpr_root", rt_pop_sexpr_root as usize),
        ("rt_push_permanent_sexpr_root", rt_push_permanent_sexpr_root as usize),
        ("rt_root_count", rt_root_count as usize),
        ("rt_set_sexpr_root", rt_set_sexpr_root as usize),
        ("rt_truncate_sexpr_roots", rt_truncate_sexpr_roots as usize),
        ("rt_str_new", rt_str_new as usize),
        ("rt_str_length", rt_str_length as usize),
        ("rt_str_ref", rt_str_ref as usize),
        ("rt_str_eq", rt_str_eq as usize),
        ("rt_str_equalp", rt_str_equalp as usize),
        ("rt_char_equalp", rt_char_equalp as usize),
        ("rt_str_lt", rt_str_lt as usize),
        ("rt_str_append", rt_str_append as usize),
        ("rt_float_new", rt_float_new as usize),
        ("rt_float_value", rt_float_value as usize),
        ("rt_box_kind", rt_box_kind as usize),
        ("rt_struct_new", rt_struct_new as usize),
        ("rt_struct_field_get", rt_struct_field_get as usize),
        ("rt_struct_field_set", rt_struct_field_set as usize),
        ("rt_struct_field_count", rt_struct_field_count as usize),
        ("rt_struct_push_field", rt_struct_push_field as usize),
        ("rt_struct_pop_field", rt_struct_pop_field as usize),
        ("rt_closure_new", rt_closure_new as usize),
        ("rt_closure_fnptr", rt_closure_fnptr as usize),
        ("rt_closure_env_len", rt_closure_env_len as usize),
        ("rt_closure_env_get", rt_closure_env_get as usize),
        ("rt_apply_any", rt_apply_any as usize),
        ("rt_cell_new", rt_cell_new as usize),
        ("rt_cell_get", rt_cell_get as usize),
        ("rt_cell_set", rt_cell_set as usize),
        ("rt_data_new", rt_data_new as usize),
        ("rt_data_variant", rt_data_variant as usize),
        ("rt_data_field", rt_data_field as usize),
        ("rt_sexpr_instance_test", rt_sexpr_instance_test as usize),
        ("rt_hashtable_new", rt_hashtable_new as usize),
        ("rt_hashtable_set", rt_hashtable_set as usize),
        ("rt_hashtable_count", rt_hashtable_count as usize),
        ("rt_hashtable_clear", rt_hashtable_clear as usize),
        ("rt_hashtable_keys", rt_hashtable_keys as usize),
        ("rt_hashtable_values", rt_hashtable_values as usize),
        ("rt_hashtable_entries", rt_hashtable_entries as usize),
        ("rt_hashtable_contains", rt_hashtable_contains as usize),
        ("rt_hashtable_get_raw", rt_hashtable_get_raw as usize),
        ("rt_hashtable_remove_raw", rt_hashtable_remove_raw as usize),
        ("rt_global_new", rt_global_new as usize),
        ("rt_global_get", rt_global_get as usize),
        ("rt_global_set", rt_global_set as usize),
        ("rt_bignum_new", rt_bignum_new as usize),
        ("rt_ratio_from_bignums", rt_ratio_from_bignums as usize),
        ("rt_bignum_add", rt_bignum_add as usize),
        ("rt_bignum_sub", rt_bignum_sub as usize),
        ("rt_bignum_mul", rt_bignum_mul as usize),
        ("rt_bignum_div", rt_bignum_div as usize),
        ("rt_bignum_mod", rt_bignum_mod as usize),
        ("rt_bignum_cmp", rt_bignum_cmp as usize),
        ("rt_bignum_to_int", rt_bignum_to_int as usize),
        ("rt_bignum_fits_i32", rt_bignum_fits_i32 as usize),
        ("rt_bignum_to_int_raw", rt_bignum_to_int_raw as usize),
        ("rt_bignum_to_float", rt_bignum_to_float as usize),
        ("rt_bignum_to_ratio", rt_bignum_to_ratio as usize),
        ("rt_int_to_bignum", rt_int_to_bignum as usize),
        ("rt_int_to_ratio", rt_int_to_ratio as usize),
        ("rt_float_to_bignum", rt_float_to_bignum as usize),
        ("rt_float_to_ratio", rt_float_to_ratio as usize),
        ("rt_ratio_add", rt_ratio_add as usize),
        ("rt_ratio_sub", rt_ratio_sub as usize),
        ("rt_ratio_mul", rt_ratio_mul as usize),
        ("rt_ratio_div", rt_ratio_div as usize),
        ("rt_ratio_cmp", rt_ratio_cmp as usize),
        ("rt_ratio_to_bignum", rt_ratio_to_bignum as usize),
        ("rt_ratio_to_float", rt_ratio_to_float as usize),
        ("rt_ratio_numerator", rt_ratio_numerator as usize),
        ("rt_ratio_denominator", rt_ratio_denominator as usize),
        ("rt_intern_symbol", rt_intern_symbol as usize),
        ("rt_intern_path", rt_intern_path as usize),
        ("rt_path_to_list", rt_path_to_list as usize),
        ("rt_list_to_path", rt_list_to_path as usize),
        ("rt_gensym", rt_gensym as usize),
        ("rt_i64_div", rt_i64_div as usize),
        ("rt_i64_mod", rt_i64_mod as usize),
        ("rt_i64_ash", rt_i64_ash as usize),
        ("rt_i64_logbitp", rt_i64_logbitp as usize),
        ("rt_i64_logcount", rt_i64_logcount as usize),
        ("rt_i64_integer_length", rt_i64_integer_length as usize),
        ("rt_f64_tan", rt_f64_tan as usize),
        ("rt_f64_asin", rt_f64_asin as usize),
        ("rt_f64_acos", rt_f64_acos as usize),
        ("rt_f64_atan", rt_f64_atan as usize),
        ("rt_f64_sinh", rt_f64_sinh as usize),
        ("rt_f64_cosh", rt_f64_cosh as usize),
        ("rt_f64_tanh", rt_f64_tanh as usize),
        ("rt_f64_asinh", rt_f64_asinh as usize),
        ("rt_f64_acosh", rt_f64_acosh as usize),
        ("rt_f64_atanh", rt_f64_atanh as usize),
    ]
}

/// The captures counterpart of [`compiled_fn_type`]: `i64 name(i64* args,
/// i32 argc, i64* env, i32 env_len)` — used for a `labels` sibling whenever
/// its block's shared captured-name list
/// (`compile::core_freevars::free_vars`) is non-empty, and (labels/closures
/// Stage 4) for *every* function ever wrapped into a `ClosureBox` via
/// `build-make-closure`, capturing or not — see that builtin's doc comment
/// (`registry::llvm_builder_def`) for why unifying on one ABI regardless of
/// whether a given closure actually captures anything is what lets
/// `build-closure-apply` call through it without first checking which case
/// it's in.
fn compiled_fn_type_with_env() -> inkwell::types::FunctionType<'static> {
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into(), ptr_ty.into(), ctx.i32_type().into()], false)
}

/// `llvm-builder::load-env` reads a logical captured slot back out of `env`,
/// the same way `load-arg` reads a logical parameter out of `args`.
fn llvm_module_add_function_with_env(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let function = module.borrow_mut().add_function(name, compiled_fn_type_with_env(), None);
    Ok(llvm_function_value(function))
}

fn llvm_module_verify(args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let ok = module.borrow().verify().is_ok();
    Ok(Value::Bool(ok))
}

fn llvm_module_to_string(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let text = module.borrow().print_to_string().to_string();
    Ok(str_rt(heap, text))
}

/// Looks up an already-`add-function`-declared `llvm-function` by name —
/// see `registry::llvm_module_def`'s doc comment on `get-function` for why
/// this is the core lookup every direct call (self-recursion, `labels`
/// siblings, top-level `defun`-to-`defun` calls) is built on.
fn llvm_module_get_function(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let found = module.borrow().get_function(name);
    found
        .map(llvm_function_value)
        .ok_or_else(|| EvalError::Panic(format!("get-function: no function named \"{}\" in this module", name)))
}

fn llvm_function_append_block(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let function = expect_llvm_function(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let block = crate::compile::llvm_context().append_basic_block(function, name);
    Ok(llvm_block_value(block))
}

fn llvm_builder_create() -> Result<Value, EvalError> {
    let builder = crate::compile::llvm_context().create_builder();
    Ok(llvm_builder_value(builder))
}

fn llvm_builder_position_at_end(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let block = expect_llvm_basic_block(&args[1])?;
    builder.borrow().position_at_end(block);
    Ok(Value::Empty)
}

fn llvm_builder_const_i64(args: &[Value]) -> Result<Value, EvalError> {
    let _builder = expect_llvm_builder(&args[0])?;
    let n = match &args[1] {
        Value::Int(n) => *n,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let value = crate::compile::llvm_context().i64_type().const_int(n as u64, false);
    Ok(llvm_value_value(value.into()))
}

fn llvm_builder_build_ret(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let value = expect_llvm_value(&args[1])?;
    builder
        .borrow()
        .build_return(Some(&value))
        .map_err(|e| EvalError::Internal(format!("build-ret: {}", e)))?;
    Ok(Value::Empty)
}

/// Reads logical parameter `index` out of `function`'s fixed-ABI argument
/// array (its sole real LLVM parameter — see `llvm_module_add_function`'s
/// doc comment) via a GEP + load. `i64` only for now, matching every other
/// `llvm-builder` arithmetic builtin.
fn llvm_builder_load_arg(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let function = expect_llvm_function(&args[1])?;
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let args_ptr = function
        .get_nth_param(0)
        .ok_or_else(|| EvalError::Internal("load-arg: function has no args parameter".into()))?
        .into_pointer_value();
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), args_ptr, &[idx_val], "arg_ptr")
            .map_err(|e| EvalError::Internal(format!("load-arg: {}", e)))?
    };
    let loaded =
        b.build_load(ctx.i64_type(), elem_ptr, "arg_val").map_err(|e| EvalError::Internal(format!("load-arg: {}", e)))?;
    Ok(llvm_value_value(loaded))
}

/// Reads logical captured slot `index` out of `function`'s env array
/// (its 3rd real LLVM parameter, `get_nth_param(2)` — see
/// `llvm_module_add_function_with_env`'s doc comment) — the same GEP+load
/// pattern `load_arg` uses against the args array (parameter 0), just
/// against the env one instead.
fn llvm_builder_load_env(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let function = expect_llvm_function(&args[1])?;
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let env_ptr = function
        .get_nth_param(2)
        .ok_or_else(|| EvalError::Internal("load-env: function has no env parameter".into()))?
        .into_pointer_value();
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), env_ptr, &[idx_val], "env_ptr")
            .map_err(|e| EvalError::Internal(format!("load-env: {}", e)))?
    };
    let loaded =
        b.build_load(ctx.i64_type(), elem_ptr, "env_val").map_err(|e| EvalError::Internal(format!("load-env: {}", e)))?;
    Ok(llvm_value_value(loaded))
}

/// Shared by `build-add`/`build-sub`/`build-mul`: unwrap both `llvm-value`
/// operands to `IntValue`s, apply `op` (one of `Builder::build_int_add`/
/// `_sub`/`_mul`), and re-wrap the result.
fn llvm_builder_build_int_op(
    args: &[Value],
    name: &str,
    op: impl FnOnce(&Builder<'static>, inkwell::values::IntValue<'static>, inkwell::values::IntValue<'static>, &str) -> Result<inkwell::values::IntValue<'static>, inkwell::builder::BuilderError>,
) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a = expect_llvm_value(&args[1])?.into_int_value();
    let b = expect_llvm_value(&args[2])?.into_int_value();
    let result = op(&builder.borrow(), a, b, name).map_err(|e| EvalError::Internal(format!("build-{}: {}", name, e)))?;
    Ok(llvm_value_value(result.into()))
}

/// Shared by `build-fadd`/`build-fsub`/`build-fmul`/`build-fdiv`/`build-frem`
/// (compiled `f64` arithmetic). A compiled `f64` value is its raw `f64::to_bits`
/// pattern carried in an `i64` register (`compile-float`'s convention — "every
/// compiled value is a plain `i64`"), so each operand is `bitcast`ed `i64` ->
/// `double` here, the `op` applied, and the `double` result `bitcast`ed back to
/// `i64` — the whole float-ness stays contained in this one instruction from
/// the surrounding IR's point of view, exactly the way a `char`'s code point
/// stays a plain `i64` everywhere but the `char->int`/`int->char` edges.
fn llvm_builder_build_float_op(
    args: &[Value],
    name: &str,
    op: impl FnOnce(&Builder<'static>, inkwell::values::FloatValue<'static>, inkwell::values::FloatValue<'static>, &str) -> Result<inkwell::values::FloatValue<'static>, inkwell::builder::BuilderError>,
) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a_bits = expect_llvm_value(&args[1])?.into_int_value();
    let b_bits = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("build-{}: {}", name, e));
    let a = bld.build_bit_cast(a_bits, f64_ty, "a_f").map_err(err)?.into_float_value();
    let b = bld.build_bit_cast(b_bits, f64_ty, "b_f").map_err(err)?.into_float_value();
    let result = op(&bld, a, b, name).map_err(err)?;
    let bits = bld.build_bit_cast(result, ctx.i64_type(), name).map_err(err)?;
    Ok(llvm_value_value(bits))
}

/// The `f64` comparison counterpart of [`llvm_builder_build_int_op`]/
/// [`llvm_builder_build_icmp`] combined: `bitcast` both `i64`-carried operands
/// to `double`, `fcmp` with `predicate`, then zero-extend the `i1` result to
/// the `i64` every compiled value is (`build-icmp`'s own widening step).
/// `<`/`<=`/`>`/`>=` use the *ordered* predicates (`OLT`/... — false if either
/// operand is NaN, matching Rust's `<`/... the interpreter's `eval_float_builtin`
/// uses); `=`/`eq`/`eql`/`equal`/`equalp` use `OEQ` (NaN never equals NaN) and
/// `/=` uses `UNE` (Rust's `!=` is `!(a == b)`, true when either is NaN).
fn llvm_builder_build_fcmp(args: &[Value], name: &str, predicate: inkwell::FloatPredicate) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a_bits = expect_llvm_value(&args[1])?.into_int_value();
    let b_bits = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("{}: {}", name, e));
    let a = bld.build_bit_cast(a_bits, f64_ty, "a_f").map_err(err)?.into_float_value();
    let b = bld.build_bit_cast(b_bits, f64_ty, "b_f").map_err(err)?.into_float_value();
    let cmp = bld.build_float_compare(predicate, a, b, name).map_err(err)?;
    let widened = bld.build_int_z_extend(cmp, ctx.i64_type(), name).map_err(err)?;
    Ok(llvm_value_value(widened.into()))
}

/// The unary transcendental/rounding counterpart of
/// [`llvm_builder_build_float_op`]: `bitcast` the single `i64`-carried
/// operand to `double`, call the named LLVM intrinsic (`llvm.sqrt.f64`/
/// `llvm.floor.f64`/`llvm.ceil.f64`/`llvm.round.f64`/`llvm.trunc.f64`), then
/// `bitcast` the `double` result back. Unlike `build-fadd`/... (plain LLVM
/// instructions), these have no dedicated IR opcode, so they go through
/// `module`'s intrinsic declaration (`Intrinsic::get_declaration`, itself
/// idempotent — safe to call again for a later use of the same op in the
/// same module, same as `get-function` finding an already-declared `rt_*`
/// shim) rather than `get-function`'s fixed-ABI `rt_*` lookup. `round`
/// matches Rust's `f64::round` (`float-native-method?`'s uncompiled
/// fallback, kept in sync by `tests/compile_test.rs`): both round halfway
/// cases away from zero, not to even. `sqrt`/`floor`/`ceil`/`trunc` are
/// exact IEEE-754 operations with no rounding-mode ambiguity to begin with.
fn llvm_builder_build_float_unary_intrinsic(args: &[Value], name: &str, intrinsic_name: &str) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let x_bits = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: String| EvalError::Internal(format!("build-{}: {}", name, e));
    let x = bld.build_bit_cast(x_bits, f64_ty, "x_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name)
        .ok_or_else(|| err(format!("no such LLVM intrinsic {}", intrinsic_name)))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[f64_ty.into()])
        .ok_or_else(|| err(format!("failed to declare {}", intrinsic_name)))?;
    let call = bld.build_call(decl, &[x.into()], name).map_err(|e| err(e.to_string()))?;
    let result = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_float_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err(err(format!("{} produced no value", intrinsic_name))),
    };
    let bits = bld.build_bit_cast(result, ctx.i64_type(), name).map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(bits))
}

/// `expt` (`f64,f64->f64`): the binary counterpart of
/// [`llvm_builder_build_float_unary_intrinsic`], `llvm.pow.f64` — matches
/// the interpreter's `f64::powf` (`float_expt`), both ultimately the
/// platform libm `pow` either way.
/// Shared by `build-fpow`/`build-fmaxnum`/`build-fminnum` — every binary
/// `f64` LLVM intrinsic this project uses. Each `i64`-carried operand is
/// `bitcast`ed to `double`, the intrinsic (overloaded on its `f64` operand
/// type, hence the `module` parameter — same reason
/// [`llvm_builder_build_float_unary_intrinsic`] takes one) applied, and the
/// `double` result `bitcast`ed back to `i64`.
fn llvm_builder_build_float_binary_intrinsic(args: &[Value], name: &str, intrinsic_name: &str) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let a_bits = expect_llvm_value(&args[2])?.into_int_value();
    let b_bits = expect_llvm_value(&args[3])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: String| EvalError::Internal(format!("build-{}: {}", name, e));
    let a = bld.build_bit_cast(a_bits, f64_ty, "a_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let b = bld.build_bit_cast(b_bits, f64_ty, "b_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name).ok_or_else(|| err(format!("no such LLVM intrinsic {}", intrinsic_name)))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[f64_ty.into()])
        .ok_or_else(|| err(format!("failed to declare {}", intrinsic_name)))?;
    let call = bld.build_call(decl, &[a.into(), b.into()], name).map_err(|e| err(e.to_string()))?;
    let result = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_float_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err(err(format!("{} produced no value", intrinsic_name))),
    };
    let bits = bld.build_bit_cast(result, ctx.i64_type(), name).map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(bits))
}

fn llvm_builder_build_fpow(args: &[Value]) -> Result<Value, EvalError> {
    llvm_builder_build_float_binary_intrinsic(args, "fpow", "llvm.pow.f64")
}

/// `(select cond then else)`: CL's `max`/`min` (and `bignum`/`ratio`'s, via
/// their own three-way `rt_*_cmp` comparator) all lower to this — an
/// `icmp`+`select` pair, branch-free, rather than real control flow. `cond`
/// is an ordinary `i64`-valued `llvm-value` (nonzero = true, the same
/// convention [`llvm_builder_build_cond_br`]'s `cond` uses), narrowed to `i1`
/// with an `icmp ne cond, 0` before `select` — which, unlike `br`, requires a
/// genuine `i1` operand, not a widened `i64`.
fn llvm_builder_build_select(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let cond = expect_llvm_value(&args[1])?.into_int_value();
    let then_v = expect_llvm_value(&args[2])?.into_int_value();
    let else_v = expect_llvm_value(&args[3])?.into_int_value();
    let bld = builder.borrow();
    let err = |e: String| EvalError::Internal(format!("build-select: {}", e));
    let ctx = crate::compile::llvm_context();
    let zero = ctx.i64_type().const_zero();
    let is_nonzero = bld.build_int_compare(inkwell::IntPredicate::NE, cond, zero, "select_cond").map_err(|e| err(e.to_string()))?;
    let result = bld.build_select(is_nonzero, then_v, else_v, "select").map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(result.into_int_value().into()))
}

/// `float->int` (`f64->i32`, narrowing, truncating toward zero): `bitcast`
/// the `i64`-carried operand to `double`, then the `llvm.fptosi.sat`
/// intrinsic (overloaded on both its `i64` result and `f64` operand type,
/// hence the `module` parameter every other overloaded-intrinsic builtin
/// here already takes — see [`llvm_builder_build_float_unary_intrinsic`]/
/// [`llvm_builder_build_fpow`]) straight to `i64` (every compiled integer,
/// `i32` included, is carried in a full `i64` register — see
/// `int-native-method?`'s doc comment). Unlike a plain `fptosi`
/// instruction (poison on NaN/out-of-range input), `.sat` clamps: NaN -> 0,
/// `+inf`/an overflowing magnitude -> `i64::MAX`, `-inf`/an underflowing
/// magnitude -> `i64::MIN` — exactly Rust's `as` cast semantics, matching
/// the interpreter's `float_to_int` (`*f as i64`) bit for bit.
fn llvm_builder_build_fptosi(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let x_bits = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let i64_ty = ctx.i64_type();
    let err = |e: String| EvalError::Internal(format!("build-fptosi: {}", e));
    let x = bld.build_bit_cast(x_bits, f64_ty, "x_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let intrinsic =
        inkwell::intrinsics::Intrinsic::find("llvm.fptosi.sat").ok_or_else(|| err("no such LLVM intrinsic llvm.fptosi.sat".into()))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[i64_ty.into(), f64_ty.into()])
        .ok_or_else(|| err("failed to declare llvm.fptosi.sat".into()))?;
    let call = bld.build_call(decl, &[x.into()], "fptosi_sat").map_err(|e| err(e.to_string()))?;
    let result = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err(err("llvm.fptosi.sat produced no value".into())),
    };
    Ok(llvm_value_value(result.into()))
}

/// Stack-allocates a `[count x i64]` array and returns its base pointer, to
/// be filled in by `store-arg` and passed to `build-call` — the compiled-IR
/// equivalent of building the `i64* args` array every compiled function's
/// fixed ABI expects (see `llvm_module_add_function`'s doc comment). Opaque
/// pointers (LLVM 17's default) carry no element-type info of their own, so
/// this pointer is usable as a flat `i64*` exactly the way `load_arg`'s own
/// `args_ptr` parameter already is — every GEP against it supplies
/// `ctx.i64_type()` itself, regardless of the alloca's nominal array type.
fn llvm_builder_alloca_args(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let count = match &args[1] {
        Value::Int(n) => *n as u32,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let array_ty = ctx.i64_type().array_type(count);
    let ptr = builder.borrow().build_alloca(array_ty, "call_args").map_err(|e| EvalError::Internal(format!("alloca-args: {}", e)))?;
    Ok(llvm_value_value(ptr.into()))
}

/// Writes `value` into slot `index` of an `alloca-args` array — the same
/// GEP pattern `load_arg` uses to *read* a logical argument, just paired
/// with a store instead of a load.
fn llvm_builder_store_arg(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let value = expect_llvm_value(&args[3])?.into_int_value();
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), array_ptr, &[idx_val], "store_arg_ptr").map_err(|e| EvalError::Internal(format!("store-arg: {}", e)))?
    };
    b.build_store(elem_ptr, value).map_err(|e| EvalError::Internal(format!("store-arg: {}", e)))?;
    Ok(Value::Empty)
}

/// The generic-pointer read counterpart of [`llvm_builder_store_arg`] — same
/// GEP pattern as [`llvm_builder_load_arg`], but against an arbitrary
/// `array_ptr` rather than a function's own args parameter (automatic
/// retain/release insertion's `release-pending-args` uses this to read back
/// the parallel "which call/env-array slots need releasing" array
/// `compile-call-args`/`compile-env-args` built via `store-arg`).
fn llvm_builder_load_raw(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), array_ptr, &[idx_val], "load_raw_ptr").map_err(|e| EvalError::Internal(format!("load-raw: {}", e)))?
    };
    let loaded = b.build_load(ctx.i64_type(), elem_ptr, "load_raw_val").map_err(|e| EvalError::Internal(format!("load-raw: {}", e)))?;
    Ok(llvm_value_value(loaded))
}

/// Shared by every `build-icmp-*` builtin (if/let/comparisons, labels/closures
/// Stage 5): runs `icmp <predicate>` on two `i64` operands, then widens the
/// resulting `i1` back to `i64` (0/1) via `build_int_z_extend` — every other
/// builtin here treats a compiled value as a plain `i64` (see
/// `registry::llvm_module_def`'s doc comment), and a comparison result is no
/// exception, which is exactly what lets `compile-if`'s `build-cond-br` (and
/// ordinary arithmetic/storage) accept it without caring it came from a
/// comparison rather than `+`/a literal.
fn llvm_builder_build_icmp(args: &[Value], name: &str, predicate: inkwell::IntPredicate) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a = expect_llvm_value(&args[1])?.into_int_value();
    let b = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let cmp = bld.build_int_compare(predicate, a, b, name).map_err(|e| EvalError::Internal(format!("{}: {}", name, e)))?;
    let ctx = crate::compile::llvm_context();
    let widened = bld.build_int_z_extend(cmp, ctx.i64_type(), name).map_err(|e| EvalError::Internal(format!("{}: {}", name, e)))?;
    Ok(llvm_value_value(widened.into()))
}

/// `compile-if`'s branch primitive: branches to `then_block` when `cond`
/// (an ordinary `i64`-valued `llvm-value`) is nonzero, `else_block`
/// otherwise — built from an `icmp ne cond, 0` plus a conditional branch, the
/// same shape [`llvm_builder_build_closure_apply`] already uses internally
/// for its own env-loop bounds check.
fn llvm_builder_build_cond_br(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let cond = expect_llvm_value(&args[1])?.into_int_value();
    let then_block = expect_llvm_basic_block(&args[2])?;
    let else_block = expect_llvm_basic_block(&args[3])?;
    let b = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let zero = ctx.i64_type().const_zero();
    let is_nonzero =
        b.build_int_compare(inkwell::IntPredicate::NE, cond, zero, "if_cond_nz").map_err(|e| EvalError::Internal(format!("build-cond-br: {}", e)))?;
    b.build_conditional_branch(is_nonzero, then_block, else_block).map_err(|e| EvalError::Internal(format!("build-cond-br: {}", e)))?;
    Ok(Value::Empty)
}

/// An unconditional branch — `compile-if`'s then/else arms use this to join
/// back at the merge block after storing their value into the shared slot.
fn llvm_builder_build_br(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_basic_block(&args[1])?;
    builder.borrow().build_unconditional_branch(target).map_err(|e| EvalError::Internal(format!("build-br: {}", e)))?;
    Ok(Value::Empty)
}

/// See `registry::llvm_builder_def`'s doc comment for `block-terminated?`.
fn llvm_builder_block_terminated(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let terminated = builder.borrow().get_insert_block().and_then(|bb| bb.get_terminator()).is_some();
    Ok(Value::Bool(terminated))
}

/// A direct call to an already-declared `target` (typically `get-function`'s
/// result), passing `args_ptr`/`argc` straight through to its fixed ABI —
/// see `registry::llvm_module_def`'s doc comment for why every compiled
/// function shares that one signature regardless of arity. This is the one
/// new primitive that unlocks every statically-resolvable direct call:
/// self-recursion, `labels`-sibling calls, and top-level `defun`-to-`defun`
/// calls alike, since all three reduce to "the callee's `llvm-function`
/// already exists in this module, look it up and call it."
fn llvm_builder_build_call(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = match &args[3] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = builder
        .borrow()
        .build_call(target, &[args_ptr.into(), argc_val.into()], "call_result")
        .map_err(|e| EvalError::Internal(format!("build-call: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-call: callee produced no value".into())),
    }
}

/// Dynamic dispatch through a trait object's vtable (TODO T4): calls the
/// raw function pointer `args[1]` — which the island has just read out with
/// `rt_vtable_slot` — under the ordinary `compiled_fn_type` ABI, passing the
/// argument array exactly as [`llvm_builder_build_call`] does.
///
/// The stripped-down sibling of [`llvm_builder_build_closure_apply`]: both
/// call a callee that is only a runtime value, but a vtable slot carries no
/// captured environment, so none of that function's env-copying loop or
/// fixed 64-slot scratch buffer is needed — just an `inttoptr` and an
/// indirect call.
fn llvm_builder_build_dyn_call(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let fn_ptr_int = expect_llvm_value(&args[1])?.into_int_value();
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = match &args[3] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let fn_ptr = b
        .build_int_to_ptr(fn_ptr_int, ptr_ty, "dyn_fn_ptr")
        .map_err(|e| EvalError::Internal(format!("build-dyn-call: {}", e)))?;
    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = b
        .build_indirect_call(compiled_fn_type(), fn_ptr, &[args_ptr.into(), argc_val.into()], "dyn_call_result")
        .map_err(|e| EvalError::Internal(format!("build-dyn-call: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => {
            Err(EvalError::Internal("build-dyn-call: callee produced no value".into()))
        }
    }
}

/// The captures counterpart of [`llvm_builder_build_call`]: calls `target`
/// (declared via `add-function-with-env`) passing both the args array
/// (`args_ptr`/`argc`, exactly as `build-call` does) and an env array
/// (`env_ptr`/`env_len`) under its extended ABI.
fn llvm_builder_build_call_with_env(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = match &args[3] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let env_ptr = expect_llvm_value(&args[4])?;
    let env_len = match &args[5] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let argc_val = ctx.i32_type().const_int(argc, false);
    let env_len_val = ctx.i32_type().const_int(env_len, false);
    let call = builder
        .borrow()
        .build_call(target, &[args_ptr.into(), argc_val.into(), env_ptr.into(), env_len_val.into()], "call_result")
        .map_err(|e| EvalError::Internal(format!("build-call-with-env: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => {
            Err(EvalError::Internal("build-call-with-env: callee produced no value".into()))
        }
    }
}

/// Heap-allocates a `BoxedObj::CompiledClosure` (`typelisp-mem`) wrapping
/// `target` (a function declared via `add-function-with-env` — see
/// `compiled_fn_type_with_env`'s doc comment for why *every* closure-boxed
/// function uses that ABI) and a copy of `env`'s `env_len` values (an
/// already-built `alloca-args`/`store-arg` array — the same shape a direct
/// capturing call already builds, see `compiler.rs`'s `compile-env-args`).
/// The closure-representation unification's flip of the retired
/// `ClosureBox` (a raw `malloc`'d, reference-counted block the GC never saw)
/// to a GC-heap value: builds a scratch `rt_closure_new` argument array
/// (`target`'s raw address, `sexpr_mask`, then each captured slot copied
/// verbatim from `env_ptr`) and calls it through the ordinary `rt_*` FFI
/// convention every other `BoxedObj` constructor uses (`rt_struct_new`,
/// `rt_data_new`, ...) — `rt_closure_new` is unconditionally forward-declared
/// into every module (`rt_extern_functions`), so `target`'s own parent module
/// already has it. `sexpr_mask` (`compiler.rs`'s `compute-sexpr-mask`) is
/// passed straight through unchanged: it marks which captured slots are
/// tagged `Sexpr` values for `rt_closure_new` to `decode`, exactly the mask
/// [`BoxedObj::CompiledClosure`]'s own doc comment describes.
fn llvm_builder_build_make_closure(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let target = expect_llvm_function(&args[2])?;
    let env_ptr = expect_llvm_value(&args[3])?.into_pointer_value();
    let env_len = match &args[4] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let sexpr_mask = match &args[5] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let module = module.borrow();
    let rt_closure_new = module
        .get_function("rt_closure_new")
        .ok_or_else(|| EvalError::Internal("build-make-closure: rt_closure_new not declared in this module".into()))?;

    let i64_ty = ctx.i64_type();
    let argc = 2 + env_len;
    let ctor_args_ptr = b
        .build_alloca(i64_ty.array_type(argc as u32), "closure_ctor_args")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
    let store_slot = |idx: u64, v: BasicValueEnum<'static>| -> Result<(), EvalError> {
        let idx_val = i64_ty.const_int(idx, false);
        let p = unsafe {
            b.build_gep(i64_ty, ctor_args_ptr, &[idx_val], "closure_ctor_arg_ptr")
                .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?
        };
        b.build_store(p, v).map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
        Ok(())
    };

    let fn_ptr_int = b
        .build_ptr_to_int(target.as_global_value().as_pointer_value(), i64_ty, "closure_fn_ptr")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
    store_slot(0, fn_ptr_int.into())?;
    store_slot(1, i64_ty.const_int(sexpr_mask, false).into())?;
    for i in 0..env_len {
        let idx_val = i64_ty.const_int(i, false);
        let src_ptr = unsafe {
            b.build_gep(i64_ty, env_ptr, &[idx_val], "closure_env_src")
                .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?
        };
        let v = b
            .build_load(i64_ty, src_ptr, "closure_env_val")
            .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
        store_slot(2 + i, v)?;
    }

    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = b
        .build_call(rt_closure_new, &[ctor_args_ptr.into(), argc_val.into()], "closure_new_result")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-make-closure: rt_closure_new produced no value".into())),
    }
}

/// `build-call-with-env`'s indirect counterpart: the callee isn't a
/// statically-known `llvm-function` here, only a tagged `Sexpr` function
/// value, so the call goes out through
/// [`typelisp_rt::rt_apply_any`](crate::compile::runtime::rt_apply_any) —
/// one `rt_*` call taking the closure word, the address of the argument
/// array the caller already built, and its length.
///
/// This used to emit the dispatch inline: `rt_closure_fnptr` +
/// `rt_closure_env_len`, an `rt_closure_env_get` copy loop into a fixed
/// 64-slot scratch buffer, then a `build_indirect_call` through
/// `compiled_fn_type_with_env`. That works for a `BoxedObj::CompiledClosure`
/// and *only* for one — `rt_closure_fnptr` `fatal`s on anything else, and a
/// `Type::Fn` value can equally be an interpreted `BoxedObj::Closure` since
/// the JIT stopped being mandatory. Emitting a call to a shim that dispatches
/// on the box instead moves the whole decision to where the value is, at the
/// cost of one call for the compiled case (the loop it replaces was already
/// three `rt_*` calls plus a per-slot one).
///
/// `compiler.rs` is untouched by this — it asks for `build-closure-apply`
/// and gets whatever this emits — which is what let the change happen with
/// the committed island bitcode frozen.
fn llvm_builder_build_closure_apply(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let closure = expect_llvm_value(&args[2])?;
    let args_ptr = expect_llvm_value(&args[3])?.into_pointer_value();
    let argc = match &args[4] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let module = module.borrow();
    let err = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("build-closure-apply: {}", e));

    let rt_apply_any = module
        .get_function("rt_apply_any")
        .ok_or_else(|| EvalError::Internal("build-closure-apply: rt_apply_any not declared in this module".into()))?;

    // `rt_apply_any` shares the one uniform `(args_ptr, argc) -> i64` `rt_*`
    // ABI (`compiled_fn_type`), so its own three arguments go into an
    // `alloca`'d array first — exactly what `compiler.rs`'s
    // `alloca-args`/`store-arg`/`build-call` triple does for every other
    // `rt_*` call. The *callee's* argument array is passed by address, as an
    // `i64`; it is a live `alloca` in this frame, so no copy is needed (and
    // unlike a `Vec`'s buffer it cannot be moved by anything the shim does).
    let i64_ty = ctx.i64_type();
    let shim_args = b.build_alloca(i64_ty.array_type(3), "apply_any_args").map_err(err)?;
    let store = |i: u64, v: BasicValueEnum<'static>, name: &str| -> Result<(), EvalError> {
        let p = unsafe { b.build_gep(i64_ty, shim_args, &[i64_ty.const_int(i, false)], name).map_err(err)? };
        b.build_store(p, v).map_err(err)?;
        Ok(())
    };
    let args_ptr_int = b.build_ptr_to_int(args_ptr, i64_ty, "apply_any_callee_args_int").map_err(err)?;
    store(0, closure, "apply_any_closure_ptr")?;
    store(1, args_ptr_int.into(), "apply_any_args_ptr")?;
    store(2, i64_ty.const_int(argc, false).into(), "apply_any_argc_ptr")?;

    let call = b
        .build_call(rt_apply_any, &[shim_args.into(), ctx.i32_type().const_int(3, false).into()], "closure_apply_result")
        .map_err(err)?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-closure-apply: rt_apply_any produced no value".into())),
    }
}

// ---- Stage 6 of the Sexpr-representation plan: generic malloc/free ------
// (`docs/implementation-log.md`) — the `ClosureBox` generalization: a general ADT box
// (`Option`/`Result`/`defstruct`) needs heap storage and offset load/store
// exactly like a `ClosureBox` does, but with no fixed header shape to bake
// in (a variant tag slot, then one slot per field — `compiler.rs`'s
// `compile-construct` lays this out itself using the four primitives below
// plus the already-generic `store-arg`/`load-raw`). Unlike `ClosureBox`,
// no refcounting/cascading-release machinery exists yet for these boxes —
// `compile-construct` simply leaks them, the same accepted trade-off this
// codebase already takes for an unreferenced boxed `labels` sibling or a
// captured reference cycle (see `compiler.rs`'s module doc comment) —
// `build-free` is exposed regardless, ready for a later stage to wire up
// automatic freeing without needing a new Rust primitive then.

/// Heap-allocates `count` `i64` slots and returns the raw pointer — see this
/// section's own doc comment. The generalization of
/// [`llvm_builder_build_make_closure`]'s own `build_array_malloc` call,
/// without baking in `ClosureBox`'s fixed header layout.
fn llvm_builder_build_malloc(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let count = match &args[1] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let count_val = ctx.i64_type().const_int(count, false);
    let ptr = builder
        .borrow()
        .build_array_malloc(ctx.i64_type(), count_val, "box")
        .map_err(|e| EvalError::Internal(format!("build-malloc: {}", e)))?;
    Ok(llvm_value_value(ptr.into()))
}

/// Frees a pointer `build-malloc` returned (or any other `llvm-value`
/// already holding a real pointer, e.g. after `build-int-to-ptr`) — the
/// inverse of `build-malloc`. See this section's own doc comment for why
/// nothing in `compiler.rs` calls this yet.
fn llvm_builder_build_free(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    builder.borrow().build_free(ptr).map_err(|e| EvalError::Internal(format!("build-free: {}", e)))?;
    Ok(Value::Empty)
}

/// Reinterprets an `i64`-valued `llvm-value` as a pointer — every compiled
/// value is a plain `i64` (`registry::llvm_module_def`'s doc comment), so a
/// general ADT box value read back out of a slot/argument/field needs this
/// before `load-raw`/`store-arg` (which both expect an already-pointer-typed
/// `llvm-value`) can dereference it — `compiler.rs`'s `compile-field-get`/
/// `compile-field-set` need it directly.
fn llvm_builder_build_int_to_ptr(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let v = expect_llvm_value(&args[1])?.into_int_value();
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let ptr = builder
        .borrow()
        .build_int_to_ptr(v, ptr_ty, "int_to_ptr")
        .map_err(|e| EvalError::Internal(format!("build-int-to-ptr: {}", e)))?;
    Ok(llvm_value_value(ptr.into()))
}

/// The inverse of `build-int-to-ptr` — `compile-construct`'s final step,
/// turning a freshly `build-malloc`'d pointer into the plain `i64` value
/// every other compiled value already is, matching how `build-make-closure`
/// does the same `ptrtoint` for a `ClosureBox`.
fn llvm_builder_build_ptr_to_int(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let ctx = crate::compile::llvm_context();
    let v = builder
        .borrow()
        .build_ptr_to_int(ptr, ctx.i64_type(), "ptr_to_int")
        .map_err(|e| EvalError::Internal(format!("build-ptr-to-int: {}", e)))?;
    Ok(llvm_value_value(v.into()))
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
    let mem_fields = fields.iter().map(|f| rtvalue_to_struct_field(f)).collect();
    alloc_typed_enum(heap, &type_name, variant, mem_fields)
}

impl Interp {
    /// The `stream-*` / `file-*` built-ins: thin wrappers over
    /// [`crate::eval::stream::StreamTable`], which owns the OS resources.
    ///
    /// Every one of these is deliberately dumb. The CL-shaped surface —
    /// `open`'s `&key` arguments, the `with-...` macros, `read-line`'s
    /// end-of-input convention, and every composite stream — is written in
    /// typelisp on top of the `Stream`/`InputStream`/`OutputStream` traits,
    /// where a default method body can express it once for all
    /// implementations. Nothing here knows those traits exist.
    ///
    /// Fallible operations return `Result<_, FileError>` rather than
    /// panicking: a missing file or a closed stream is a condition programs
    /// routinely handle, not a bug.
    fn eval_stream_builtin(
        &self,
        heap: &mut Heap,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, EvalError>> {
        use crate::check::registry::FILE_ERROR;
        // Most arms yield a `StreamResult`; this keeps each to one line.
        macro_rules! wrap {
            ($e:expr, $ok:expr) => {
                match $e {
                    Ok(v) => Ok(result_ok(heap, $ok(v))),
                    Err(m) => Ok(result_err(heap, FILE_ERROR, m)),
                }
            };
        }
        let mut t = self.streams.borrow_mut();
        let h = |i: usize| -> Result<i64, EvalError> { rt_i64(&args[i]) };
        Some(match name {
            "stream-stdin" => Ok(Value::Int(t.stdin())),
            "stream-stdout" => Ok(Value::Int(t.stdout())),
            "stream-stderr" => Ok(Value::Int(t.stderr())),
            "stream-string-input" => match expect_str(heap, &args[0]) {
                Ok(s) => Ok(Value::Int(t.string_input(&s))),
                Err(e) => Err(e),
            },
            "stream-string-output" => Ok(Value::Int(t.string_output())),
            "stream-open-file" => match (expect_str(heap, &args[0]), h(1)) {
                (Ok(p), Ok(mode)) => wrap!(t.open_file(&p, mode), |v: i64| Value::Int(v)),
                (Err(e), _) | (_, Err(e)) => Err(e),
            },
            "stream-close" => match h(0) {
                Ok(x) => wrap!(t.close(x), |_v: ()| Value::Empty),
                Err(e) => Err(e),
            },
            "stream-open-p" => h(0).map(|x| Value::Bool(t.is_open(x))),
            "stream-input-p" => match h(0) {
                Ok(x) => wrap!(t.is_input(x), |v: bool| Value::Bool(v)),
                Err(e) => Err(e),
            },
            "stream-output-p" => match h(0) {
                Ok(x) => wrap!(t.is_output(x), |v: bool| Value::Bool(v)),
                Err(e) => Err(e),
            },
            "stream-read-char" => match h(0) {
                Ok(x) => match t.read_char(x) {
                    Ok(c) => {
                        let inner = option_value(heap, c.map(Value::Char));
                        Ok(result_ok(heap, inner))
                    }
                    Err(m) => Ok(result_err(heap, FILE_ERROR, m)),
                },
                Err(e) => Err(e),
            },
            "stream-unread-char" => match (h(0), expect_char(&args[1])) {
                (Ok(x), Ok(c)) => wrap!(t.unread_char(x, c), |_v: ()| Value::Empty),
                (Err(e), _) | (_, Err(e)) => Err(e),
            },
            "stream-listen" => match h(0) {
                Ok(x) => wrap!(t.listen(x), |v: bool| Value::Bool(v)),
                Err(e) => Err(e),
            },
            "stream-write-string" => match (h(0), expect_str(heap, &args[1])) {
                (Ok(x), Ok(s)) => wrap!(t.write_str(x, &s), |_v: ()| Value::Empty),
                (Err(e), _) | (_, Err(e)) => Err(e),
            },
            "stream-at-line-start" => match h(0) {
                Ok(x) => wrap!(t.at_line_start(x), |v: bool| Value::Bool(v)),
                Err(e) => Err(e),
            },
            "stream-finish-output" => match h(0) {
                Ok(x) => wrap!(t.finish_output(x), |_v: ()| Value::Empty),
                Err(e) => Err(e),
            },
            "stream-take-output-string" => match h(0) {
                Ok(x) => match t.take_output_string(x) {
                    Ok(s) => {
                        let sv = str_rt(heap, s);
                        Ok(result_ok(heap, sv))
                    }
                    Err(m) => Ok(result_err(heap, FILE_ERROR, m)),
                },
                Err(e) => Err(e),
            },
            "file-exists-p" => match expect_str(heap, &args[0]) {
                Ok(p) => Ok(Value::Bool(std::path::Path::new(&*p).exists())),
                Err(e) => Err(e),
            },
            "file-delete" => match expect_str(heap, &args[0]) {
                Ok(p) => match std::fs::remove_file(&*p) {
                    Ok(()) => Ok(result_ok(heap, Value::Empty)),
                    Err(e) => Ok(result_err(heap, FILE_ERROR, format!("delete-file: {}: {}", p, e))),
                },
                Err(e) => Err(e),
            },
            "file-rename" => match (expect_str(heap, &args[0]), expect_str(heap, &args[1])) {
                (Ok(a), Ok(b)) => match std::fs::rename(&*a, &*b) {
                    Ok(()) => Ok(result_ok(heap, Value::Empty)),
                    Err(e) => Ok(result_err(heap, FILE_ERROR, format!("rename-file: {}: {}", a, e))),
                },
                (Err(e), _) | (_, Err(e)) => Err(e),
            },
            _ => return None,
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
    let key = rtvalue_to_struct_field(&args[1]);
    let found = heap.hashtable_get(id, key);
    Ok(option_value(heap, found))
}

fn hashtable_set(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    expect_hashable_key(&args[1])?;
    let key = rtvalue_to_struct_field(&args[1]);
    let val = rtvalue_to_struct_field(&args[2]);
    heap.hashtable_set(id, key, val);
    Ok(Value::Empty)
}

fn hashtable_remove(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    expect_hashable_key(&args[1])?;
    let key = rtvalue_to_struct_field(&args[1]);
    let removed = heap.hashtable_remove(id, key);
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
/// `Heap::hashtable_pairs` and so need no `RtValue` round-trip.
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
/// see [`RtValue::Sexpr`]'s doc comment for why these no longer get their
/// own `RtValue` variant. Doesn't itself check `Heap::is_struct` (a
/// mismatched-kind `BoxId` — e.g. a boxed float reaching here — is caught by
/// the panic in whichever `Heap` struct accessor the caller goes on to call,
/// the same internal-invariant-trap convention `Heap::struct_field` etc.
/// already use).
fn expect_struct_box(v: &Value) -> Result<BoxId, EvalError> {
    match v {
        Value::Boxed(id) => Ok(*id),
        other => Err(EvalError::Internal(format!("expected a boxed struct, got {:?}", other))),
    }
}

/// Converts an already-evaluated `RtValue` into the `mem::Value` a
/// `BoxedObj::Struct` field stores — the encode half of the struct-field
/// boundary crossing (`decode_field_typed` is the other direction).
/// Unambiguous regardless of the field's static type: every supported
/// `RtValue` variant maps to exactly one `Value` shape. `RtValue::Sexpr`
/// covers `HashTable<K,V>` too, since the `Sexpr`/`RtValue` unification's
/// Stage 5 — a boxed `HashTable` is a `mem::Value::Boxed` like any other
/// boxed struct. The variants a `defstruct`/`Vector<T>`/`HashTable<K,V>`
/// field still can't hold — `Data` (`Option`/`Result`/a user sum type) and
/// `RtValue::Scope` (a `Scope<V>` of a *native-repr* `V`, LLVM handles
/// above all) — aren't representable in `crate::mem::Value` (that crate
/// can't depend on `RtValue`), so they're a clear internal error here
/// rather than a silent corruption. Closures stopped being on that list at
/// Stage 6b (a function value is a `Value::Boxed` closure box riding in
/// `RtValue::Sexpr`, storable like any other boxed value), and heap-repr-`V`
/// scopes at Stage 8, the same way.
///
/// `Unit` left that list when `()` gained a field representation: it stores
/// as `Value::Empty`. The slot carries no information (a unit type has one
/// value, already known statically from the declared type), so what it holds
/// only has to be a word the GC can `decode` safely — and an immediate nil
/// references nothing at all. This is the *one* encoding where the stored
/// shape alone doesn't identify the type, so [`decode_field_typed`] reads
/// `Unit` back off the declared type rather than from the value; see its
/// doc comment. Making this representable is what keeps `Result<(), E>` a
/// heap-repr enum instead of sending [`build_enum_value`] down its
/// native-`RtValue::Data` fallback — which is what let a `()` payload cross
/// into compiled code at all.
pub(super) fn rtvalue_to_struct_field(v: &Value) -> Value {
    match v {
        Value::Empty => Value::Empty,
        Value::Int(n) => Value::Int(*n),
        Value::Bool(b) => Value::Bool(*b),
        Value::Char(c) => Value::Char(*c),
        v => *v,
    }
}

/// Convert an evaluated [`RtValue`] back into a `Sexpr`-side [`Value`], or
/// `None` if it has no `Sexpr` representation — the shape `eval` needs to
/// return its result (`Self::eval_form`). Exactly
/// [`rtvalue_to_struct_field`]'s conversion: scalars box through the heap,
/// an `RtValue::Sexpr` passes through, and `Unit` becomes `Value::Empty` —
/// so a form that produces no value (a `defvar` initializer's side effect,
/// an empty `progn`) reads back as `()`/nil. `Unit` used to need its own arm
/// here because a struct field couldn't hold one; now that it can, the two
/// agree by construction rather than by two copies of the same rule.
///
/// The non-representable set has shrunk to exactly one variant,
/// [`RtValue::Data`]. Everything that used to be in it has since been given a
/// heap form: the LLVM handles and native `Scope<V>` (Phase 1a), built-in
/// function values (`BoxedObj::Builtin`), and `random-state`
/// (`BoxedObj::RandomState`). And `Data` is only ever *built* by
/// `build_enum_value`'s fallback, which fires only when a field fails this
/// very conversion — so with nothing else able to fail it, no `Data` can be
/// constructed and this returns `None` for nothing at all.
pub(super) fn rtvalue_to_sexpr(v: &Value) -> Value {
    rtvalue_to_struct_field(v)
}

fn expect_int_index(v: &Value) -> Result<usize, EvalError> {
    match v {
        Value::Int(n) if *n >= 0 => Ok(*n as usize),
        other => Err(EvalError::Panic(format!("Vector: invalid index {:?}", other))),
    }
}

fn vector_push(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let v = rtvalue_to_struct_field(&args[1]);
    heap.struct_push_field(id, v);
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
    let v = rtvalue_to_struct_field(&args[2]);
    heap.struct_set_field(id, i, v);
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

// ---- LLVM handle registry + `rt_llvm_call` (interp-closure removal ------
// ---- Stage 1) -----------------------------------------------------------
//
// Compiled code represents every Rust-native LLVM value (`RtValue::
// LlvmModule`/`LlvmBuilder`/`LlvmFunction`/`LlvmBasicBlock`/`LlvmValue`)
// and the native `RtValue::Scope` as an *LLVM handle*: an index into this
// thread-local registry, carried as a plain untraced `i64`
// (`Repr::Handle` — kind `1`, the integer kind). That is
// what makes the self-hosted compiler island's own `defun`s compilable:
// their `llvm-*` method calls lower to a single generic shim,
// [`rt_llvm_call`], which decodes handles back to `RtValue`s, dispatches
// into the very same [`eval_llvm_builtin_method`]/native-scope builtins the
// interpreter uses, and encodes the result.
//
// Registry entries are only ever *appended* during a compile session; the
// mark/release pair below lets the outermost compiled-island entry point
// drop everything it accumulated once the session ends (the `Rc`s inside
// the registered `RtValue`s keep shared structures like a module or a
// scope's frames alive exactly as long as some other owner still needs
// them). Thread-local for the same reason `set_active_heap` is: `cargo
// test` workers each drive their own independent `Heap`/LLVM session.

/// A Rust-native object that has no representation in the value heap, held
/// here so a typelisp value can refer to it by an opaque integer handle.
///
/// These are the compiler's own working objects — an LLVM module, builder,
/// function, block, value, and a `Scope` whose `V` is one of those. They
/// cannot live on the GC heap (an inkwell handle is not a `Value` and has no
/// traceable shape), and they used to be carried as `RtValue::Llvm*`/`Scope`
/// variants instead: a second, Rust-side value universe running alongside the
/// heap one, which is what forced every binding to be routed by static type
/// (`SlotKind`) rather than just being a `Value`.
///
/// Keeping the object here and handing out an `i64` collapses that: at the
/// language level an `llvm-value` is an integer like any other, and the same
/// representation already crosses to compiled code
/// (`Repr::Handle`'s integer kind), so interpreted and
/// compiled tiers now agree by construction rather than by conversion. Type
/// safety is unaffected — `LLVM_HANDLE_TYPES` keeps these distinct from `i32`
/// statically, which is where it was always enforced.
///
/// `src/eval/stream.rs` represents OS streams the same way.
#[derive(Clone)]
pub(crate) enum NativeHandle {
    Module(Rc<RefCell<Module<'static>>>),
    Builder(Rc<RefCell<Builder<'static>>>),
    Function(FunctionValue<'static>),
    BasicBlock(BasicBlock<'static>),
    Value(BasicValueEnum<'static>),
}

thread_local! {
    static LLVM_HANDLES: RefCell<Vec<NativeHandle>> = const { RefCell::new(Vec::new()) };
}

/// Registers `h` and returns its handle — the `i64` both the interpreter and
/// compiled code carry in place of the Rust-native object.
pub(crate) fn llvm_handle_register(h: NativeHandle) -> i64 {
    LLVM_HANDLES.with(|t| {
        let mut t = t.borrow_mut();
        t.push(h);
        (t.len() - 1) as i64
    })
}

/// The object behind handle `h`, or `None` for a never-issued (or already
/// released) handle.
pub(crate) fn llvm_handle_get(h: i64) -> Option<NativeHandle> {
    if h < 0 {
        return None;
    }
    LLVM_HANDLES.with(|t| t.borrow().get(h as usize).cloned())
}

/// Register `h` as the interpreter-level value standing for it.
fn handle_value(h: NativeHandle) -> Value {
    Value::Int(llvm_handle_register(h))
}

fn llvm_module_value(m: Module<'static>) -> Value {
    handle_value(NativeHandle::Module(Rc::new(RefCell::new(m))))
}

fn llvm_module_value_rc(m: Rc<RefCell<Module<'static>>>) -> Value {
    handle_value(NativeHandle::Module(m))
}

fn llvm_builder_value(b: Builder<'static>) -> Value {
    handle_value(NativeHandle::Builder(Rc::new(RefCell::new(b))))
}

fn llvm_function_value(f: FunctionValue<'static>) -> Value {
    handle_value(NativeHandle::Function(f))
}

fn llvm_block_value(b: BasicBlock<'static>) -> Value {
    handle_value(NativeHandle::BasicBlock(b))
}

fn llvm_value_value(v: BasicValueEnum<'static>) -> Value {
    handle_value(NativeHandle::Value(v))
}

/// The LLVM module `v` is a handle for, or `None` if it is not one.
///
/// An `llvm-module` is an opaque integer at the value level, so a caller
/// outside this module (a test inspecting generated IR, say) needs this to get
/// at the object behind it.
pub fn llvm_module_of(v: &Value) -> Option<Rc<RefCell<Module<'static>>>> {
    match llvm_handle_get(match v {
        Value::Int(h) => *h,
        _ => return None,
    })? {
        NativeHandle::Module(m) => Some(m),
        _ => None,
    }
}

/// The LLVM value `v` is a handle for — [`llvm_module_of`]'s counterpart.
pub fn llvm_value_of(v: &Value) -> Option<BasicValueEnum<'static>> {
    match llvm_handle_get(match v {
        Value::Int(h) => *h,
        _ => return None,
    })? {
        NativeHandle::Value(x) => Some(x),
        _ => None,
    }
}

/// The handle integer standing for `v` at the compiled boundary.
///
/// An LLVM object already *is* its handle, so this is the identity on it; only
/// a native scope still has to be registered on the way out.
fn handle_of(v: &Value) -> Option<i64> {
    match v {
        Value::Int(h) => Some(*h),
        _ => None,
    }
}

/// The interpreter value standing for handle integer `raw`, the inverse of
/// [`handle_of`].
fn value_of_handle(raw: i64) -> Option<Value> {
    llvm_handle_get(raw).map(|_| Value::Int(raw))
}

/// The handle integer `v` carries, for the `expect_llvm_*` accessors.
fn expect_handle(v: &Value, want: &str) -> Result<NativeHandle, EvalError> {
    let h = match v {
        Value::Int(n) => *n,
        other => return Err(EvalError::Internal(format!("expected an {}, got {:?}", want, other))),
    };
    llvm_handle_get(h).ok_or_else(|| EvalError::Internal(format!("expected an {}, got dangling handle {}", want, h)))
}

/// The current registry length — pass to [`llvm_handles_release`] to drop
/// every handle issued after this point. Wired to the compiled-island
/// session boundaries in a later stage of the interp-closure removal plan
/// (nothing calls a compiled function with LLVM-handle-typed values yet
/// outside tests, whose registries die with their test process).
#[allow(dead_code)]
pub(crate) fn llvm_handles_mark() -> usize {
    LLVM_HANDLES.with(|t| t.borrow().len())
}

/// Drops every handle issued since the matching [`llvm_handles_mark`].
#[allow(dead_code)]
pub(crate) fn llvm_handles_release(mark: usize) {
    LLVM_HANDLES.with(|t| t.borrow_mut().truncate(mark));
}

/// How `rt_llvm_call` decodes one raw argument word, per the op table.
#[derive(Clone, Copy, Debug)]
enum LlvmArgK {
    /// A registry handle — decode via [`llvm_handle_get`].
    Handle,
    /// A tagged heap `Value::Boxed` at a `StructPayload::Frames` scope.
    ///
    /// Compiled code treats a scope word as opaque: it only ever receives one
    /// from `rt_llvm_call` and hands it straight back, so what the word *is*
    /// is settled entirely here, and the committed island bitcode is
    /// indifferent to the change from registry handle to tagged box.
    Scope,
    /// A tagged heap `Value::Str` — decode to `RtValue::Str`.
    Str,
    /// A raw untagged integer.
    Int,
    /// A raw 0/1 word.
    Bool,
}

/// How `rt_llvm_call` encodes the builtin's `RtValue` result.
#[derive(Clone, Copy, Debug)]
enum LlvmRetK {
    /// Register the value, return its handle.
    Handle,
    /// A scope box, tagged — see [`LlvmArgK::Scope`]. Registered as a session
    /// root on the way out: the committed island binds scope words at the
    /// untraced integer kind, so nothing else keeps the box alive while the
    /// compile session runs.
    Scope,
    /// Return `0` (`compile-unit`'s convention).
    Unit,
    /// Raw 0/1.
    Bool,
    /// A freshly-allocated tagged heap `Value::Str`.
    Str,
    /// `Option<handle>` as a heap `BoxedObj::Enum` (`Some` payload =
    /// `Value::Int(handle)`, decoded through the ordinary integer field
    /// kind by a compiled `match`) — the native scope `get`'s shape.
    OptHandle,
}

/// One dispatchable builtin: `(type_key, method)` plus its marshaling
/// shape, keyed in [`llvm_op_table`] by [`compile::symbols::llvm_op_id`].
struct LlvmOp {
    type_key: &'static str,
    method: String,
    args: Vec<LlvmArgK>,
    ret: LlvmRetK,
}

fn llvm_arg_kind(ty: &Type) -> LlvmArgK {
    if crate::check::repr::is_llvm_handle_ty(ty) {
        LlvmArgK::Handle
    } else if ty.is_integer() {
        LlvmArgK::Int
    } else {
        match ty {
            Type::Str => LlvmArgK::Str,
            Type::Bool => LlvmArgK::Bool,
            other => panic!("llvm_op_table: parameter type {:?} has no rt_llvm_call marshaling", other),
        }
    }
}

fn llvm_ret_kind(ty: &Type) -> LlvmRetK {
    if crate::check::repr::is_llvm_handle_ty(ty) {
        LlvmRetK::Handle
    } else {
        match ty {
            Type::Unit => LlvmRetK::Unit,
            Type::Bool => LlvmRetK::Bool,
            Type::Str => LlvmRetK::Str,
            other => panic!("llvm_op_table: return type {:?} has no rt_llvm_call marshaling", other),
        }
    }
}

/// The `rt_llvm_call` dispatch table, keyed by [`compile::symbols::llvm_op_id`]'s
/// stable hash. The `llvm-*` entries are *derived* from the same
/// `check::registry` `AdtDef`s the checker types these methods with —
/// table and signatures cannot drift apart. The six `native-scope` entries
/// are written out by hand because `scope_def`'s signatures are generic
/// over `V` (here always an LLVM handle — see
/// `core_bridge::llvm_assoc_key`, which only routes a `Scope<V>` with an
/// LLVM-handle `V` to this table in the first place).
fn llvm_op_table() -> &'static HashMap<i64, LlvmOp> {
    static TABLE: std::sync::OnceLock<HashMap<i64, LlvmOp>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t: HashMap<i64, LlvmOp> = HashMap::new();
        let insert = |t: &mut HashMap<i64, LlvmOp>, type_key: &'static str, method: String, args: Vec<LlvmArgK>, ret: LlvmRetK| {
            let id = crate::compile::symbols::llvm_op_id(type_key, &method);
            if t.insert(id, LlvmOp { type_key, method, args, ret }).is_some() {
                panic!("llvm_op_table: op id collision on {}", id);
            }
        };
        for (type_key, def) in [
            ("llvm-module", crate::check::registry::llvm_module_def()),
            ("llvm-function", crate::check::registry::llvm_function_def()),
            ("llvm-builder", crate::check::registry::llvm_builder_def()),
        ] {
            for (method, af) in def.assoc {
                let args = af.sig.params.iter().map(llvm_arg_kind).collect();
                let ret = llvm_ret_kind(&af.sig.ret);
                insert(&mut t, type_key, method, args, ret);
            }
        }
        use LlvmArgK::{Handle as H, Str as S};
        // The scope receiver is a tagged box word now, not a registry handle
        // (`LlvmArgK::Scope`). Only the op *ids* are baked into the committed
        // island bitcode — this table, and so the marshaling shape, is Rust
        // side and free to change with it.
        const SC: LlvmArgK = LlvmArgK::Scope;
        insert(&mut t, "native-scope", "new".to_string(), vec![], LlvmRetK::Scope);
        insert(&mut t, "native-scope", "clone-frames".to_string(), vec![SC], LlvmRetK::Scope);
        insert(&mut t, "native-scope", "push-frame".to_string(), vec![SC], LlvmRetK::Unit);
        insert(&mut t, "native-scope", "pop-frame".to_string(), vec![SC], LlvmRetK::Unit);
        insert(&mut t, "native-scope", "get".to_string(), vec![SC, S], LlvmRetK::OptHandle);
        insert(&mut t, "native-scope", "set".to_string(), vec![SC, S, H], LlvmRetK::Unit);
        t
    })
}

/// The `rt_*` family's abort-on-invariant-break convention
/// (`typelisp-rt`'s `fatal`), for the main-crate shims.
fn rt_llvm_fatal(msg: &str) -> ! {
    eprintln!("typelisp runtime error: {}", msg);
    std::process::abort();
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
/// Errors abort rather than return: there is a compiled frame between here
/// and any Rust caller that could handle a `Result`, and it has no way to
/// carry one. That matches every other `rt_*` shim (`rt_panic`,
/// `rt_match_fail`, `rt_llvm_call`).
///
/// # Safety
///
/// `args` must point to `argc` valid `i64`s, and both a `Heap`
/// ([`typelisp_rt::set_active_heap`]) and an `Interp`
/// ([`Interp::enter_compiled`]) must be registered on this thread.
unsafe extern "C" fn rt_apply_interpreted(closure: i64, args: *const i64, argc: u32) -> i64 {
    let interp = ACTIVE_INTERP.with(|cell| cell.get());
    if interp.is_null() {
        rt_llvm_fatal("rt_apply_any: no interpreter is registered on this thread");
    }
    let interp = &*interp;
    let heap = crate::compile::runtime::shim_active_heap();
    let argv = std::slice::from_raw_parts(args, argc as usize);
    match interp.apply_interpreted(heap, closure, argv) {
        Ok(w) => w,
        Err(e) => rt_llvm_fatal(&format!("rt_apply_any: {:?}", e)),
    }
}

/// `(rt-llvm-call opid arg...)` for compiled code — the generic dispatch
/// shim behind every compiled `llvm-*`/native-`Scope<V>` builtin method
/// call (`compiler.rs`'s `compile-llvm-op`; interp-closure removal Stage
/// 1). `args[0]` is the [`compile::symbols::llvm_op_id`] hash embedded at
/// translate time; the rest are marshaled per the matching
/// [`llvm_op_table`] entry and dispatched into the *exact same*
/// [`eval_llvm_builtin_method`]/native-scope builtins the interpreter
/// itself uses — one implementation, two callers, no drift.
///
/// Argument values are fully materialized into Rust-side `RtValue`s
/// *before* anything here can allocate on the GC heap, so callers only
/// need their usual kind-driven rooting (a tagged `Str` argument crossing
/// in stays valid until then because nothing between the caller's own
/// allocation and this decode allocates).
///
/// # Safety
///
/// `args` must point to `argc` valid `i64`s; a `Heap` must already be
/// registered on this thread (`set_active_heap`). Errors abort via
/// [`rt_llvm_fatal`], mirroring `typelisp-rt`'s `fatal`.
pub(crate) unsafe extern "C" fn rt_llvm_call(args: *const i64, argc: u32) -> i64 {
    let argv = std::slice::from_raw_parts(args, argc as usize);
    let Some((&opid, raw_args)) = argv.split_first() else {
        rt_llvm_fatal("rt_llvm_call: missing op id");
    };
    let Some(op) = llvm_op_table().get(&opid) else {
        rt_llvm_fatal(&format!("rt_llvm_call: unknown op id {}", opid));
    };
    if raw_args.len() != op.args.len() {
        rt_llvm_fatal(&format!(
            "rt_llvm_call: {}::{} expects {} arguments, got {}",
            op.type_key,
            op.method,
            op.args.len(),
            raw_args.len()
        ));
    }
    let heap = crate::compile::runtime::shim_active_heap();
    let mut vals: Vec<Value> = Vec::with_capacity(raw_args.len());
    for (raw, k) in raw_args.iter().zip(&op.args) {
        vals.push(match k {
            LlvmArgK::Handle => match value_of_handle(*raw) {
                Some(v) => v,
                None => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: dangling llvm handle {}", op.type_key, op.method, raw)),
            },
            LlvmArgK::Str => match crate::compile::runtime::decode(*raw) {
                v @ Value::Str(_) => v,
                other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Str argument, got {:?}", op.type_key, op.method, other)),
            },
            LlvmArgK::Scope => match crate::compile::runtime::decode(*raw) {
                v @ Value::Boxed(_) => v,
                other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a scope box, got {:?}", op.type_key, op.method, other)),
            },
            LlvmArgK::Int => Value::Int(*raw),
            LlvmArgK::Bool => Value::Bool(*raw != 0),
        });
    }
    // The scope `get` returns `Option<V>` — encoded specially, so it's
    // handled before the generic single-`RtValue` result path.
    //
    // The stored element word goes into the `Some` payload verbatim: for the
    // island's `Scope<llvm-value>`/`Scope<llvm-function>` that is the same
    // `Value::Int(handle)` this produced before scopes moved to the heap, so
    // the option box the committed bitcode unwraps is bit-identical.
    if op.type_key == "native-scope" && op.method == "get" {
        let found = match (expect_struct_box(&vals[0]), expect_str(heap, &vals[1])) {
            (Ok(id), Ok(name)) => heap.scope_get(id, name),
            (Err(e), _) | (_, Err(e)) => rt_llvm_fatal(&format!("rt_llvm_call: native-scope::get: {:?}", e)),
        };
        let boxed = match found {
            // type-identity-ok: the built-in `Option`, a root name spelled in full
            Some(v) => heap.alloc_enum("option".to_string(), 0, vec![v]),
            // type-identity-ok: the built-in `Option`, a root name spelled in full
            None => heap.alloc_enum("option".to_string(), 1, vec![]),
        };
        return crate::compile::runtime::encode(boxed);
    }
    let result: Result<Value, EvalError> = if op.type_key == "native-scope" {
        match op.method.as_str() {
            "new" => Ok(heap.alloc_scope()),
            "clone-frames" => scope_clone_frames_heap(heap, &vals),
            "push-frame" => scope_push_frame_heap(heap, &vals),
            "pop-frame" => scope_pop_frame_heap(heap, &vals),
            // The element type is irrelevant here: the value arrives already
            // in its stored form, so `set` stores the word as given. (The
            // interpreter's own `set` still encodes through
            // `rtvalue_to_struct_field`, since there the value is an
            // `RtValue` that has not crossed a boundary.)
            "set" => scope_set_raw(heap, &vals),
            other => rt_llvm_fatal(&format!("rt_llvm_call: unknown native-scope method {}", other)),
        }
    } else {
        match eval_llvm_builtin_method(heap, &Path::root(op.type_key), &op.method, &vals) {
            Some(r) => r,
            None => rt_llvm_fatal(&format!("rt_llvm_call: {} has no builtin method {}", op.type_key, op.method)),
        }
    };
    let v = match result {
        Ok(v) => v,
        Err(e) => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: {:?}", op.type_key, op.method, e)),
    };
    match op.ret {
        LlvmRetK::Handle => match handle_of(&v) {
            Some(h) => h,
            None => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a handle result, got {:?}", op.type_key, op.method, v)),
        },
        LlvmRetK::Scope => {
            // Root for the compile session: the committed island holds this
            // word in an untraced local, so nothing else would keep the box
            // alive across the next collection.
            heap.push_session_root(v);
            crate::compile::runtime::encode(v)
        }
        LlvmRetK::Unit => 0,
        LlvmRetK::Bool => match v {
            Value::Bool(b) => i64::from(b),
            other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Bool result, got {:?}", op.type_key, op.method, other)),
        },
        LlvmRetK::Str => match v {
            // Already a heap string; encoding is the tagged word, no copy.
            sv @ Value::Str(_) => crate::compile::runtime::encode(sv),
            other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Str result, got {:?}", op.type_key, op.method, other)),
        },
        LlvmRetK::OptHandle => rt_llvm_fatal("rt_llvm_call: OptHandle result outside native-scope::get"),
    }
}

// The `Scope<V>`-with-heap-repr-`V` counterparts of the native scope
// helpers above (unification Stage 8) — same six-method surface, backed by
// `Heap`'s `StructPayload::Frames` representation instead of `Rc` frames.
// Which family a call lands in is decided statically, from the receiver's
// checked `Scope<V>` type ([`Interp::scope_is_heap`]) — never from the
// receiver value's shape. `V` being heap-repr means every stored/returned
// element is `RtValue::Sexpr` by construction, so — unlike `hashtable_get`,
// whose `V` can be anything — no typed decode is needed on the way out and
// a non-`Sexpr` element on the way in is an internal-invariant trap.

fn scope_clone_frames_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(heap.scope_clone_frames(id))
}

fn scope_push_frame_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.scope_push_frame(id);
    Ok(Value::Empty)
}

fn scope_pop_frame_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.scope_pop_frame(id);
    Ok(Value::Empty)
}

fn scope_get_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
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
fn scope_set_heap(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let v = rtvalue_to_struct_field(&args[2]);
    scope_store(heap, args, v)
}

/// `set` for a value that is already in its stored form — the `rt_llvm_call`
/// path, where the element arrives as the very word compiled code holds and
/// there is no `RtValue` to encode.
fn scope_set_raw(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
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
fn expect_str<'h>(heap: &'h Heap, v: &Value) -> Result<&'h str, EvalError> {
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
/// `i32` (uniformly `RtValue::Int(i64)` at runtime — see
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
/// (identity — see `RtValue::Str`'s doc comment and `docs/cl-equivalence-catalog.md`'s
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

fn expect_bool(v: &Value) -> Result<bool, EvalError> {
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
    let a = strip_dyn(heap, args[0]);
    let b = strip_dyn(heap, args[1]);
    Ok(Value::Bool(a == b))
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
    let a = args[0];
    let b = args[1];
    Ok(Value::Bool(eql_val(heap, a, b)))
}

/// The concrete value inside a trait object, or `v` unchanged. Comparison
/// (like printing) sees straight through a `BoxedObj::Dyn`: the box is a
/// dispatch mechanism, and — since it is usually created by an *implicit*
/// coercion at a `:dyn` parameter — letting it change the answer of `eq`/
/// `equal` would make an invisible conversion observable. Applied at the
/// entry of `eql_val`/`sexpr_equal_val`/`sexpr_equalp_val`, so it covers
/// nested positions through their recursion too.
fn strip_dyn(heap: &Heap, v: Value) -> Value {
    match v {
        Value::Boxed(id) if heap.is_dyn(id) => heap.dyn_value(id),
        other => other,
    }
}

/// The scalar core of `eql` on two `Sexpr` payloads: `==` (plain `Value`
/// identity/value equality) except two separately-boxed but equal `Float`s,
/// which are `eql` by value — see [`sexpr_eql`]. Shared by [`sexpr_equal`]/
/// [`sexpr_equalp`] as their atom-comparison base case.
fn eql_val(heap: &Heap, a: Value, b: Value) -> bool {
    let (a, b) = (strip_dyn(heap, a), strip_dyn(heap, b));
    if let (Value::Boxed(ia), Value::Boxed(ib)) = (a, b) {
        if heap.is_float(ia) && heap.is_float(ib) {
            return heap.float_value(ia) == heap.float_value(ib);
        }
        // Same rationale as `Float` above: two separately-allocated but
        // equal-valued `bignum`/`ratio` boxes must still be `eql`.
        if heap.is_bignum(ia) && heap.is_bignum(ib) {
            return heap.bignum_value(ia) == heap.bignum_value(ib);
        }
        if heap.is_ratio(ia) && heap.is_ratio(ib) {
            return heap.ratio_value(ia) == heap.ratio_value(ib);
        }
    }
    a == b
}

/// CL's `equal` on `Sexpr`: `eql` on every atom but `Cons` (structural
/// recursion) and `Str` (case-sensitive content). Was a prelude `defun` until
/// the Symbol/Sexpr redesign fenced `match` off `Sexpr` (Phase 5,
/// `docs/dev/symbol-sexpr-redesign.md`); reimplemented here as a Rust builtin
/// (a peer of [`sexpr_eq`]/[`sexpr_eql`]) rather than a `sexpr-*`-navigated
/// `defun`, so it needs neither `match` nor the user-facing `car`/`cdr` (which
/// Phase 4b repurposes to a generic `cons<T,U>` pair). The self-hosting
/// compiler (`compiler.rs`) still calls it from interpreted code.
fn sexpr_equal_val(heap: &Heap, a: Value, b: Value) -> bool {
    let (a, b) = (strip_dyn(heap, a), strip_dyn(heap, b));
    match (a, b) {
        (Value::Cons(_), Value::Cons(_)) => {
            let (Ok(ca), Ok(cb)) = (heap.car(a), heap.car(b)) else { return false };
            let (Ok(da), Ok(db)) = (heap.cdr(a), heap.cdr(b)) else { return false };
            sexpr_equal_val(heap, ca, cb) && sexpr_equal_val(heap, da, db)
        }
        (Value::Str(i), Value::Str(j)) => heap.string(i) == heap.string(j),
        (a, b) => eql_val(heap, a, b),
    }
}

fn sexpr_equal(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let a = args[0];
    let b = args[1];
    Ok(Value::Bool(sexpr_equal_val(heap, a, b)))
}

/// Converts any of the four numeric `Sexpr` shapes (`Int`, boxed `Float`,
/// boxed `bignum`, boxed `ratio`) into an exact `BigRational`, or `None` for
/// a non-numeric value *or* a non-finite `Float` (`NaN`/`inf` have no exact
/// rational value) — the common representation [`sexpr_equalp_val`]'s
/// cross-type numeric comparison needs, since every finite `f64` is itself
/// an exact dyadic rational (`BigRational::from_float`, same conversion
/// `float_assoc`'s `float->ratio` uses).
fn numeric_as_ratio(heap: &Heap, v: Value) -> Option<BigRational> {
    match v {
        Value::Int(n) => Some(BigRational::from_integer(BigInt::from(n))),
        Value::Boxed(id) if heap.is_float(id) => BigRational::from_float(heap.float_value(id)),
        Value::Boxed(id) if heap.is_bignum(id) => Some(BigRational::from_integer(heap.bignum_value(id).clone())),
        Value::Boxed(id) if heap.is_ratio(id) => Some(heap.ratio_value(id).clone()),
        _ => None,
    }
}

/// CL's `equalp` on `Sexpr`: like [`sexpr_equal`] but `Str`/`Char` compare
/// case-insensitively and numbers compare across type (`Int`/`Float`/
/// `bignum`/`ratio`) via [`numeric_as_ratio`] — CL's `equalp` defines two
/// numbers as equal by `=`, regardless of type, unlike `eql`/`equal`'s
/// same-type requirement. Same Phase 5 migration from a prelude
/// `match`-based `defun` to a Rust builtin.
fn sexpr_equalp_val(heap: &Heap, a: Value, b: Value) -> bool {
    let (a, b) = (strip_dyn(heap, a), strip_dyn(heap, b));
    if let (Some(x), Some(y)) = (numeric_as_ratio(heap, a), numeric_as_ratio(heap, b)) {
        return x == y;
    }
    match (a, b) {
        (Value::Cons(_), Value::Cons(_)) => {
            let (Ok(ca), Ok(cb)) = (heap.car(a), heap.car(b)) else { return false };
            let (Ok(da), Ok(db)) = (heap.cdr(a), heap.cdr(b)) else { return false };
            sexpr_equalp_val(heap, ca, cb) && sexpr_equalp_val(heap, da, db)
        }
        (Value::Str(i), Value::Str(j)) => heap.string(i).eq_ignore_ascii_case(heap.string(j)),
        (Value::Char(c), Value::Char(d)) => c.eq_ignore_ascii_case(&d),
        // CL's `equalp` on a structure: same type, and every slot `equalp`
        // (unlike `equal`, which is `eq` on structures — `eql_val`'s
        // fallback `a == b`, a pointer-identity `BoxId` compare, is exactly
        // that, so this recursive arm must come *before* it or it would
        // never run). Design plan §3.
        (Value::Boxed(ia), Value::Boxed(ib)) if heap.is_struct(ia) && heap.is_struct(ib) => {
            // type-identity-ok: two stored keys compared to each other — no
            // `Path` to spell, and `equalp`'s "same type" is exactly key equality
            heap.struct_type_name(ia) == heap.struct_type_name(ib)
                && heap.struct_field_count(ia) == heap.struct_field_count(ib)
                && (0..heap.struct_field_count(ia))
                    .all(|i| sexpr_equalp_val(heap, heap.struct_field(ia, i), heap.struct_field(ib, i)))
        }
        (Value::Boxed(ia), Value::Boxed(ib)) if heap.is_enum(ia) && heap.is_enum(ib) => {
            // type-identity-ok: two stored keys compared to each other (see the struct arm)
            heap.enum_type_name(ia) == heap.enum_type_name(ib)
                && heap.enum_variant(ia) == heap.enum_variant(ib)
                && heap.enum_field_count(ia) == heap.enum_field_count(ib)
                && (0..heap.enum_field_count(ia))
                    .all(|i| sexpr_equalp_val(heap, heap.enum_field(ia, i), heap.enum_field(ib, i)))
        }
        (a, b) => eql_val(heap, a, b),
    }
}

fn sexpr_equalp(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let a = args[0];
    let b = args[1];
    Ok(Value::Bool(sexpr_equalp_val(heap, a, b)))
}

#[cfg(test)]
mod scc_tests {
    use super::*;

    /// Surface `defun`/`defmethod` syntax can never actually exercise the
    /// mutual-recursion branch of [`Interp::compute_sccs`]/
    /// [`Interp::compile_scc`]: `Checker::check_form_at` checks one top-level
    /// form at a time, in file order, so a `defun` can only ever call a name
    /// already registered *earlier* — `docs/syntax.md`'s own description of
    /// `labels` ("相互再帰可能なローカル関数定義") confirms mutual recursion
    /// is deliberately a `labels`-only, local-scope feature, not something a
    /// pair of top-level `defun`s can express. So this bypasses the checker
    /// entirely — inserting two hand-built [`FnDef`]s that call each other
    /// straight into [`Interp::fns`], the same "same-module direct access"
    /// trick this file's own [`Interp`] fields allow — to prove the SCC
    /// machinery itself (labels/closures Stage 5) handles a genuine cycle
    /// between two *separately* JIT'd top-level functions: forward-declares
    /// both in one shared module before either body is translated, JITs the
    /// module once via `CompiledFn::new_multi`, and both end up in
    /// `Interp::compiled` with no "mutual recursion ... is not supported"
    /// `Panic` (the pre-Stage-5 behavior this replaces).
    #[test]
    fn compile_function_compiles_a_genuine_two_node_cycle_bypassing_the_checker() {
        let mut heap = Heap::with_capacity(1 << 16);
        let mut checker = crate::Checker::new();
        let mut interp = Interp::new();
        crate::load_compiler(&mut heap, &mut checker, &mut interp);

        // `(defun a () i64 (b))` / `(defun b () i64 (a))` — never checked,
        // built directly as core forms, so the checker's forward-reference
        // restriction never comes into play. Each body is a single
        // `(call () () NAME ())`: no written/home segments (the callee is
        // already fully resolved) and no arguments, hence no argument reprs.
        for (name, callee) in [("a", "b"), ("b", "a")] {
            let path = heap.intern_symbol(callee);
            heap.push_root(path);
            let call = core::tagged(&mut heap, "call", &[Value::Empty, Value::Empty, path, Value::Empty])
                .expect("building a 4-field node cannot exhaust a 1<<16 heap");
            // Registered `FnDef` bodies are reachable only through
            // `Interp::fns`, which the collector does not scan — the same
            // permanent root `Interp::exec` takes for a checked definition.
            heap.push_permanent_root(call);
            heap.pop_root();
            interp.root.borrow_mut().fns.insert(
                name.to_string(),
                Rc::new(FnDef {
                    params: vec![],
                    body: vec![call],
                    rest: false,
                    lambda: None,
                    sig: Some((vec![], Repr::Int)),
                    public: true,
                    compiled: RefCell::new(None),
                }),
            );
        }

        interp
            .compile_function(&mut heap, &CompileTarget::Fn(crate::check::resolved::Ref::synthetic(Path::root("a"))))
            .expect("mutual recursion across separate top-level functions should now compile");
        assert!(interp.root.borrow().fn_compiled(&Path::root("a")), "\"a\" should have ended up compiled");
        assert!(interp.root.borrow().fn_compiled(&Path::root("b")), "\"b\", pulled in transitively as part of the same SCC, should have ended up compiled too");
    }
}
