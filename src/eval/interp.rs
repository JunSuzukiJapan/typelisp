//! A tree-walking interpreter over the checker's typed AST.
//!
//! Top-level definitions (`defun`/`defmethod`) are registered by fully-qualified
//! name; expressions are evaluated against those registries plus a lexical
//! environment. Functions are not closures — a body sees only its parameters and
//! the global definitions, matching top-level `defun`/`defmethod` semantics.
//!
//! `Sexpr` values (see [`RtValue::Sexpr`]) live in the GC-managed cons [`Heap`]
//! shared with the reader, so `eval` threads a `&mut Heap` throughout. Since
//! cons cells held by the interpreter (in locals, globals, closures) are
//! otherwise invisible to [`Heap::gc`], every mutable [`Slot`] is registered
//! (weakly) in [`Interp::slots`]; [`Interp::sync_roots`] rebuilds the heap's
//! root set from whatever is still live there right before any allocation
//! that could trigger a collection.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, ToPrimitive, Zero};

use crate::{BoxId, Expr, Heap, MacroExpander, Path, Pattern, QuotedSexpr, TopLevel, Type, Typed, Value};

use super::value::{Capture, ClosureBody, EvalError, NativeScope, RtValue, Slot, SlotKind};

/// A registered function or method body with its parameter names.
struct FnDef {
    /// Parameter names, including the receiver name first for instance methods.
    params: Vec<String>,
    /// Each parameter's binding-slot routing, parallel to `params` — derived
    /// once at registration from the declared parameter types (`sig`; a
    /// `defmacro`'s parameters are all `Sexpr`, hence all `Heap`), so
    /// `Interp::apply` needs no type information per call.
    kinds: Vec<SlotKind>,
    body: Vec<Typed>,
    /// Only ever set for a `defmacro` with a trailing `&rest` parameter (see
    /// [`Interp::expand_macro`]); always `false` for `defun`/`defmethod`,
    /// which `apply` calls 1:1 regardless.
    rest: bool,
    /// `(parameter types, return type)`, parallel to `params` — `None` for a
    /// `defmacro` (every parameter and the implicit return are always
    /// `Sexpr`, see `check::registry::MacroDef`'s doc comment) since a macro
    /// is never a `compile` target. The tree-walking evaluator itself never
    /// needs this (it's already erased everywhere else, see
    /// `Checker::check_call`'s doc comment) — it exists solely so
    /// `Interp::compile_function` can build an LLVM function signature
    /// without the checker's `Registry` (which `Interp` otherwise has no
    /// access to).
    sig: Option<(Vec<Type>, Type)>,
}

/// A user `defenum`'s type parameters and variants (each one's declared
/// field types), as `TopLevel::Defenum` baked them in at check time — the
/// entry [`Interp::enum_defs`] keeps per enum type; see that field's doc
/// comment for why the interpreter needs this at all (compiled-global box
/// decoding) and why `Option`/`Result` are not represented this way.
pub(crate) struct EnumDef {
    pub(crate) params: Vec<String>,
    pub(crate) variants: Vec<crate::check::registry::Variant>,
}

/// A lexical environment: name -> slot, searched from the back (innermost).
type Env = Vec<(String, Slot)>;

/// The outcome of evaluating one step inside a loop body: either an
/// ordinary value (discarded — only whether a step exited matters, not what
/// it returned along the way), or a `break`/`return` already resolved to the
/// value the loop should exit with (see [`Interp::eval_loop_step`]).
enum Step {
    Continue,
    Exit(RtValue),
}

/// The interpreter state: free functions (by [`Path`]), type-associated methods
/// (by type [`Path`] and method name), and global variables (by [`Path`]).
pub struct Interp {
    fns: HashMap<Path, FnDef>,
    methods: HashMap<(Path, String), FnDef>,
    globals: HashMap<Path, Slot>,
    /// Every `Native` slot ever created, held weakly. A slot stays discoverable
    /// here for exactly as long as it's reachable some other way (an env frame
    /// on the call stack, `globals`, or a closure's captured environment) —
    /// once that owner drops the `Rc`, the entry quietly goes dead and is
    /// pruned on the next [`Self::sync_roots`].
    slots: RefCell<Vec<Weak<RefCell<RtValue>>>>,
    /// How many roots `sync_roots` last pushed onto the heap, so it knows how
    /// many to pop before recomputing the set from scratch.
    rooted: Cell<usize>,
    /// Monotonic counter backing `gensym`. typelisp symbols are always
    /// interned and permanent (no uninterned-symbol concept), so `gensym`
    /// can only offer collision-*resistant* fresh names, not CL's
    /// unforgeable ones — see [`Self::eval_builtin`]'s `"gensym"` arm.
    gensym_counter: Cell<u64>,
    /// Functions JIT-compiled by `(compile name)` (see
    /// [`Self::compile_function`]), by their fully-qualified `Path`.
    /// `RefCell` because `compile` is itself an ordinary builtin reached
    /// through `eval`'s `&self` — the same internal-mutability pattern
    /// `slots` above already uses. `Expr::Call`'s eval arm checks here
    /// first, before falling back to the tree-walking `fns` entry.
    compiled: RefCell<HashMap<Path, crate::compile::CompiledFn>>,
    /// `compiled`'s counterpart for an instance/static method JIT-compiled by
    /// `(compile type::method)` — keyed the same way [`Self::methods`]
    /// already is, so the two can never drift. `Expr::Assoc`'s eval arm
    /// checks here first, mirroring `Expr::Call`'s own `compiled` check.
    compiled_methods: RefCell<HashMap<(Path, String), crate::compile::CompiledFn>>,
    /// The interpreter-side half of every live closure (body + `Native`
    /// captures), keyed by the token its heap box carries — see
    /// [`ClosureBody`]/`BoxedObj::Closure`. Entries are inserted by
    /// [`Self::make_closure`] and removed by [`Self::sync_roots`] as the GC
    /// reports their boxes swept (`Heap::take_dead_closure_tokens`), so the
    /// table tracks heap liveness with at most one allocation of lag.
    /// `Rc<ClosureBody>` so an in-flight `Expr::Apply` keeps the body alive
    /// even if the box (and hence this entry) dies mid-call. Assumes the
    /// one-`Heap`-per-`Interp` usage every caller already follows — tokens
    /// from another heap would be meaningless here.
    closure_bodies: RefCell<HashMap<u32, Rc<ClosureBody>>>,
    /// Monotonic token source for `closure_bodies` — never reused, so a
    /// swept box's token can't be mistaken for a newer closure's (no ABA).
    closure_tokens: Cell<u32>,
    /// Every `Type::Named` path whose compiled representation is a tagged
    /// `Sexpr` `Value::Boxed` struct (Stage 3 of the Sexpr/RtValue
    /// unification plan, `docs/implementation-log.md`) — a user `defstruct`
    /// (inserted on its own `TopLevel::Defstruct` exec, below) or `vector`
    /// (`registry::vector_def`'s `AdtKind::Struct`, the one builtin type
    /// that shares this same shape without ever producing a
    /// `TopLevel::Defstruct` of its own, seeded in [`Self::new`] — see that
    /// registration's own comment for why `hashtable`/`scope` don't need the
    /// same treatment). Needed at the compiled/interpreted call boundary
    /// ([`Self::call_compiled`], via [`Self::is_boxed_sexpr_type`]) to
    /// decode/encode such a value the same way a `Sexpr`-typed one already
    /// is — `crate::compile::ast_bridge` can't answer this itself
    /// (deliberately `Registry`-free, see that module's `is_sexpr_type` doc
    /// comment), so `Interp` tracks it independently.
    struct_types: HashSet<Path>,
    /// Every user `defenum`'s declared type parameters and variant field
    /// types, recorded on its `TopLevel::Defenum` exec (the checker bakes
    /// them into that node — its `Registry` no longer exists at runtime).
    /// The same "track it independently, keep `ast_bridge` `Registry`-free"
    /// pattern as [`Self::struct_types`], but carrying full [`EnumDef`]s
    /// rather than bare membership: the compiled-global boundary's
    /// box -> `RtValue::Data` decode ([`decode_data_value`], via
    /// [`data_variant_field_types`]) needs each variant's field types to
    /// know how to read the box's untyped `i64` slots back. `Option`/
    /// `Result` are *not* seeded here — their field types read straight off
    /// `Type::Named`'s args, so `data_variant_field_types` keeps its
    /// dedicated arms for them.
    enum_defs: HashMap<Path, EnumDef>,
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
        Interp {
            fns: HashMap::new(),
            methods: HashMap::new(),
            globals: HashMap::new(),
            slots: RefCell::new(Vec::new()),
            closure_bodies: RefCell::new(HashMap::new()),
            closure_tokens: Cell::new(0),
            rooted: Cell::new(0),
            gensym_counter: Cell::new(0),
            compiled: RefCell::new(HashMap::new()),
            compiled_methods: RefCell::new(HashMap::new()),
            // `vector` is registered directly in `Registry::with_builtins`
            // (`registry::vector_def`) with `AdtKind::Struct`, so its
            // `Expr::Construct` sites already get `mutable = true`
            // (`Checker::check_construct`) — but it never executes a
            // `TopLevel::Defstruct`, the only other place `struct_types`
            // gets populated, so it's seeded here by hand. `hashtable`/
            // `scope` don't need this: both are `AdtKind::Sum`
            // (`registry::hashtable_def`/`scope_def`) with no `variants`/
            // `field_names` of their own — user code only ever builds one
            // through its `::new()` assoc fn, never `Expr::Construct`, so
            // there is no construct site for `struct_types` to affect.
            struct_types: HashSet::from([Path::root("vector")]),
            enum_defs: HashMap::new(),
            compiled_globals: RefCell::new(HashMap::new()),
        }
    }

    /// Wrap `v` in a fresh mutable slot of the statically-determined `kind`
    /// and register it (weakly) for GC rooting purposes. Every binding site
    /// (`let`, parameters, `match` bindings, globals) goes through here.
    /// A `Heap`-kind binding must hold an `RtValue::Sexpr` — anything else
    /// is a checker/interpreter invariant violation, never a user error.
    /// (`Heap::alloc_cell` cannot trigger a collection — the box store is
    /// growable — so `v`'s payload needs no rooting across this call.)
    fn slot(&self, heap: &mut Heap, kind: SlotKind, v: RtValue) -> Result<Slot, EvalError> {
        match kind {
            SlotKind::Heap => match v {
                RtValue::Sexpr(val) => Ok(Slot::Heap(heap.alloc_cell(val))),
                other => Err(EvalError::Internal(format!(
                    "heap-cell binding initialized with a non-Sexpr value: {:?}",
                    other
                ))),
            },
            SlotKind::Native => Ok(self.native_slot(v)),
        }
    }

    /// A bare `Native` slot — for binding sites that are `Native` by
    /// construction (`labels` placeholders, `eval_args`'s GC-protection
    /// anchors) rather than by a declared type's [`SlotKind`].
    fn native_slot(&self, v: RtValue) -> Slot {
        let s = Rc::new(RefCell::new(v));
        self.slots.borrow_mut().push(Rc::downgrade(&s));
        Slot::Native(s)
    }

    /// The number of live entries in the closure side table — exposed for
    /// tests proving the table shrinks in step with the GC (see
    /// `closure_bodies`); not meaningful to ordinary callers.
    pub fn closure_body_count(&self) -> usize {
        self.closure_bodies.borrow().len()
    }

    /// Builds a closure value: splits the captured environment by slot tier
    /// — heap cells into the closure box's GC-traced `env`, `Native` slots
    /// into the side-table [`ClosureBody`] — and returns the box as the
    /// `RtValue::Sexpr` every function value now is. (`Heap::alloc_closure`
    /// never itself collects, so the env walk needs no rooting.)
    fn make_closure(
        &self,
        heap: &mut Heap,
        params: Vec<(String, SlotKind)>,
        body: Vec<Typed>,
        env: &Env,
    ) -> RtValue {
        let mut heap_env: Vec<Value> = Vec::new();
        let mut layout: Vec<(String, Capture)> = Vec::with_capacity(env.len());
        for (name, slot) in env {
            let cap = match slot {
                Slot::Heap(id) => {
                    heap_env.push(Value::Boxed(**id));
                    Capture::Heap(heap_env.len() - 1)
                }
                Slot::Native(rc) => Capture::Native(rc.clone()),
            };
            layout.push((name.clone(), cap));
        }
        let token = self.closure_tokens.get();
        self.closure_tokens.set(token + 1);
        self.closure_bodies
            .borrow_mut()
            .insert(token, Rc::new(ClosureBody { params, body, layout }));
        RtValue::Sexpr(heap.alloc_closure(token, heap_env))
    }

    /// Whether a declared type's runtime representation is always
    /// `RtValue::Sexpr` — the interpreter-side twin of the checker's
    /// `Checker::is_heap_repr` (which bakes the same bit into
    /// `Pattern::Bind`), deciding [`SlotKind`] at binding sites whose AST
    /// carries a `Type` (`let`'s bound `Typed`, parameter lists, `defvar`).
    ///
    /// `Scope<V>` recurses on `V` (unification Stage 8): a scope whose
    /// element representation is a heap `Value` is itself heap-resident
    /// (`StructPayload::Frames`), while a scope of anything else — LLVM
    /// handles above all — stays the Rust-native [`RtValue::Scope`], so an
    /// LLVM handle can no more reach the GC heap through a scope than
    /// through a binding cell. Both twins must agree, and the checker's
    /// carries the matching arm.
    fn heap_repr_kind(&self, ty: &Type) -> SlotKind {
        if self.is_heap_repr_ty(ty, &mut HashSet::new()) { SlotKind::Heap } else { SlotKind::Native }
    }

    /// The recursive core of [`Self::heap_repr_kind`] — see that method's
    /// doc comment. `seen` is threaded through unchanged from
    /// [`Self::enum_fields_representable`]'s own doc comment (the
    /// self-/mutually-referential `defenum` guard).
    fn is_heap_repr_ty(&self, ty: &Type, seen: &mut HashSet<Path>) -> bool {
        match ty {
            Type::Named(p, _) if is_sexpr_type(p) || *p == Path::root("hashtable") || self.struct_types.contains(p) => true,
            Type::Named(p, args) if *p == Path::root("scope") && args.len() == 1 => self.is_heap_repr_ty(&args[0], seen),
            Type::Named(p, args) if self.is_enum_path(p) => self.enum_fields_representable(p, args, seen),
            _ => false,
        }
    }

    /// Whether `p` names an enum type — one whose runtime value is
    /// *potentially* a boxed `BoxedObj::Enum`: the built-in
    /// `Option`/`Result`/`Error`, or a user `defenum` recorded in
    /// [`Self::enum_defs`]. Whether a *given instantiation* actually is
    /// heap-repr (as opposed to falling back to native `RtValue::Data`) is
    /// [`Self::enum_fields_representable`]'s job, not this one — this just
    /// identifies the type family. The interpreter-side twin of the
    /// checker's `Registry`-driven `AdtKind::Sum && !variants.is_empty()`
    /// test in `Checker::is_heap_repr_seen`.
    fn is_enum_path(&self, p: &Path) -> bool {
        *p == option_path() || *p == result_path() || *p == Path::root("error") || self.enum_defs.contains_key(p)
    }

    /// Whether every field of every variant of enum type `name` —
    /// instantiated with `args` — is itself representable (a plain scalar
    /// that boxes trivially, or recursively heap-repr) — see
    /// `Checker::enum_fields_representable`'s doc comment for the full
    /// rationale (`Option<llvm-value>` and friends must classify `false`
    /// here). `option`/`result`'s field types are read straight off `args`
    /// (mirroring `data_variant_field_types`'s own dedicated arms, since
    /// `Interp` holds no `Registry` to look them up in); `error`'s one
    /// field is always `Str`, hence always representable; a user `defenum`
    /// looks up [`Self::enum_defs`] and substitutes `args` for its params,
    /// exactly as `Checker::enum_fields_representable` does with the
    /// checker's own registry-backed copy.
    fn enum_fields_representable(&self, name: &Path, args: &[Type], seen: &mut HashSet<Path>) -> bool {
        if *name == Path::root("error") {
            return true;
        }
        if !seen.insert(name.clone()) {
            return true;
        }
        let field_types: Vec<Type> = if *name == option_path() && args.len() == 1 {
            vec![args[0].clone()]
        } else if *name == result_path() && args.len() == 2 {
            args.to_vec()
        } else if let Some(def) = self.enum_defs.get(name) {
            let subst: HashMap<String, Type> = def.params.iter().cloned().zip(args.iter().cloned()).collect();
            def.variants
                .iter()
                .flat_map(|v| v.fields.iter().map(|f| crate::check::checker::subst_apply(f, &subst)))
                .collect()
        } else {
            Vec::new()
        };
        let ok = field_types.iter().all(|fty| crate::check::checker::is_boxable_scalar(fty) || self.is_heap_repr_ty(fty, seen));
        seen.remove(name);
        ok
    }

    /// Whether a `Scope<V>` type's runtime representation is the heap
    /// (`StructPayload::Frames`) one — true exactly when `V`'s own runtime
    /// representation is a heap `Value` ([`Interp::heap_repr_kind`], whose
    /// matching recursive arm routes scope-typed *bindings* to the same
    /// tier as the scope *values* this classifies). Monomorphization
    /// guarantees the `V` seen here is concrete; a non-`Scope` type
    /// reaching this is a checker/interpreter divergence, trapped loudly
    /// rather than guessed around.
    fn scope_is_heap(&self, scope_ty: &Type) -> Result<bool, EvalError> {
        match scope_ty {
            Type::Named(p, targs) if *p == Path::root("scope") && targs.len() == 1 => {
                Ok(matches!(self.heap_repr_kind(&targs[0]), SlotKind::Heap))
            }
            other => {
                Err(EvalError::Internal(format!("expected a Scope<V> type at a scope method call, got {:?}", other)))
            }
        }
    }

    /// Recompute the cons heap's root set from every `Sexpr` value reachable
    /// through a currently-live `Native` slot (a `Data`/`Scope` may hold
    /// `Sexpr`s inside). Must be called right before any operation that
    /// might allocate a cons cell (i.e. [`Heap::cons`]), since otherwise a
    /// GC during evaluation could reclaim a cons cell still referenced from
    /// a local, global, or closure. `Heap`-cell slots need no handling here
    /// at all: a live cell is an implicit root of the heap's own
    /// (`Heap::alloc_cell`'s `cell_registry`), visible to a collection
    /// triggered from *anywhere* — including compiled code, which never
    /// re-syncs the interpreter's roots.
    fn sync_roots(&self, heap: &mut Heap) {
        // Release the interpreter-side half of every closure the GC swept
        // since the last sync — see `closure_bodies`. Dropping an entry also
        // drops its `Capture::Native` `Rc`s (LLVM handles included).
        for t in heap.take_dead_closure_tokens() {
            self.closure_bodies.borrow_mut().remove(&t);
        }
        for _ in 0..self.rooted.replace(0) {
            heap.pop_root();
        }
        let mut slots = self.slots.borrow_mut();
        slots.retain(|w| w.upgrade().is_some());
        let mut roots = Vec::new();
        for w in slots.iter() {
            if let Some(s) = w.upgrade() {
                collect_sexpr_roots(&s.borrow(), &mut roots);
            }
        }
        self.rooted.set(roots.len());
        for v in roots {
            heap.push_root(v);
        }
    }

    /// Execute a checked top-level form. Definitions register and return `None`;
    /// a bare expression returns `Some(value)`.
    pub fn exec(&mut self, heap: &mut Heap, tl: TopLevel) -> Result<Option<RtValue>, EvalError> {
        match tl {
            TopLevel::Defun { name, type_params, params, ret, body } => {
                // A generic defun's own body was checked with its type
                // variables still abstract — a type-erased artifact kept only
                // for definition-time diagnostics. It must never run: every
                // call site was rewritten by the checker to a monomorphized
                // specialization (`Checker::request_fn_specialization`), and
                // registering the erased body here would leave a silently
                // callable stale twin behind.
                if !type_params.is_empty() {
                    return Ok(None);
                }
                let (names, types): (Vec<String>, Vec<Type>) = params.into_iter().unzip();
                let kinds = types.iter().map(|ty| self.heap_repr_kind(ty)).collect();
                self.fns.insert(name, FnDef { params: names, kinds, body, rest: false, sig: Some((types, ret)) });
                Ok(None)
            }
            TopLevel::Defmethod { type_name, method, self_name, params, ret, body, type_params, .. } => {
                // A generic-owner method's erased body is diagnostics-only,
                // exactly like a generic `Defun`'s above — every call site
                // was rewritten to a monomorphized specialization.
                if !type_params.is_empty() {
                    return Ok(None);
                }
                let mut names: Vec<String> = Vec::new();
                let mut types: Vec<Type> = Vec::new();
                if let Some(s) = self_name {
                    names.push(s);
                    types.push(Type::Named(type_name.clone(), vec![]));
                }
                for (n, t) in params {
                    names.push(n);
                    types.push(t);
                }
                let kinds = types.iter().map(|ty| self.heap_repr_kind(ty)).collect();
                self.methods.insert((type_name, method), FnDef { params: names, kinds, body, rest: false, sig: Some((types, ret)) });
                Ok(None)
            }
            TopLevel::Defmacro { name, params, body, rest } => {
                // A macro's body is callable exactly like a `defun`'s — see
                // `MacroExpander`/`Self::expand_macro` — so it's stored in
                // the very same `fns` table; no separate macro table exists.
                // Every macro parameter is `Sexpr` by definition, hence all
                // `Heap` slots.
                let kinds = vec![SlotKind::Heap; params.len()];
                self.fns.insert(name, FnDef { params, kinds, body, rest, sig: None });
                Ok(None)
            }
            TopLevel::Use { .. } => Ok(None),
            // The type itself was already registered in the checker's
            // `Registry` at check time — there's nothing else for the
            // interpreter to do, the same as `Option`/`Result` needing no
            // runtime registration of their own. Recording `name` in
            // `struct_types` is the one exception (Stage 3 of the
            // Sexpr/RtValue unification plan, `docs/implementation-log.md`
            // — see that field's doc comment).
            TopLevel::Defstruct { name } => {
                self.struct_types.insert(name);
                Ok(None)
            }
            // A `defenum` sum type is a check-time registration, like
            // `Option`/`Result`. Unlike `Defstruct` it is *not* recorded in
            // `struct_types` (an enum instance is an immutable
            // `RtValue::Data`, never a boxed struct) — but its variants'
            // field types *are* recorded, in `enum_defs`: the compiled-global
            // boundary needs them to decode a box back into a `RtValue::Data`
            // (see that field's doc comment). The same one-exception pattern
            // `Defstruct`/`struct_types` follows.
            TopLevel::Defenum { name, params, variants } => {
                self.enum_defs.insert(name, EnumDef { params, variants });
                Ok(None)
            }
            TopLevel::Defvar { name, ty, value, .. } => {
                let v = self.eval(heap, &value, &Env::new())?;
                let kind = self.heap_repr_kind(&ty);
                let s = self.slot(heap, kind, v)?;
                self.globals.insert(name, s);
                Ok(None)
            }
            TopLevel::Module { body, .. } => {
                let mut last = None;
                for t in body {
                    last = self.exec(heap, t)?;
                }
                Ok(last)
            }
            TopLevel::Expr(t) => Ok(Some(self.eval(heap, &t, &Env::new())?)),
            // `(load ...)` is resolved and applied by the *driver* at check
            // time (`project::load_file_flat`), never reaching the
            // interpreter's exec phase — the driver consumes a `Load` inline
            // rather than queuing it. Reaching here would be a driver bug.
            TopLevel::Load { .. } => Err(EvalError::Internal(
                "TopLevel::Load must be handled by the driver, not exec'd".into(),
            )),
        }
    }

    /// Evaluate `t`, tagging any real error with `t`'s source location.
    ///
    /// Thin wrapper over [`Self::eval_inner`]. Like the checker's `check`,
    /// evaluation recurses into sub-expressions through here, so the *deepest*
    /// failing node tags first and — since [`EvalError::at`] keeps the
    /// innermost location — that precise spot is what the message reports. The
    /// `Break`/`Return` control-flow signals pass through untagged (see
    /// `EvalError::at`), so the loop that catches them still matches the bare
    /// variant.
    fn eval(&self, heap: &mut Heap, t: &Typed, env: &Env) -> Result<RtValue, EvalError> {
        match self.eval_inner(heap, t, env) {
            Ok(v) => Ok(v),
            Err(e) => match &t.loc {
                Some(loc) => Err(e.at(loc.clone())),
                None => Err(e),
            },
        }
    }

    fn eval_inner(&self, heap: &mut Heap, t: &Typed, env: &Env) -> Result<RtValue, EvalError> {
        match &t.expr {
            Expr::Int(n) => Ok(RtValue::Int(*n)),
            Expr::Float(f) => Ok(RtValue::Float(*f)),
            Expr::Bignum(n) => Ok(RtValue::Bignum(Rc::new(n.clone()))),
            Expr::Ratio(r) => Ok(RtValue::Ratio(Rc::new(r.clone()))),
            Expr::Bool(b) => Ok(RtValue::Bool(*b)),
            Expr::Char(c) => Ok(RtValue::Char(*c)),
            Expr::Str(s) => Ok(RtValue::Str(s.as_str().into())),
            Expr::Unit => Ok(RtValue::Unit),
            Expr::Var(n) => env_get(env, n)
                .map(|s| s.get(heap))
                .ok_or_else(|| EvalError::Unbound(n.clone())),
            // A promoted global (`Self::compiled_globals` — some `compile`d
            // function reads/writes it via a permanent GC root, see
            // `Self::promote_global`) must be read from that same storage
            // here too, not the plain `Slot` below — otherwise an
            // interpreted read could see a stale value a compiled write
            // already updated, even though both sides name the same
            // `defvar`.
            Expr::Global(path) => {
                if let Some(&id) = self.compiled_globals.borrow().get(path) {
                    // `id` (`Self::promote_global`'s return value) is *not*
                    // the `Heap::permanent_root` position — see
                    // `typelisp_rt::global_new`'s doc comment (an
                    // `Option`/`Result` global's own heap-referencing field
                    // pushes its own extra permanent root during encoding,
                    // desyncing the two) — `global_perm_idx` resolves it the
                    // same way `rt_global_get` does.
                    let perm_idx = crate::compile::runtime::global_perm_idx(id)
                        .ok_or_else(|| EvalError::Internal(format!("global \"{}\": unknown compiled id {}", path, id)))?;
                    Ok(decode_field_typed(heap, heap.permanent_root(perm_idx), &t.ty))
                } else {
                    self.globals
                        .get(path)
                        .map(|s| s.get(heap))
                        .ok_or_else(|| EvalError::Unbound(path.to_string()))
                }
            }
            Expr::FnRef(path) => Ok(match self.fns.get(path) {
                // Reify a user function as a closure with no captured environment.
                Some(f) => {
                    let params = f.params.iter().cloned().zip(f.kinds.iter().copied()).collect();
                    let body = f.body.clone();
                    self.make_closure(heap, params, body, &Env::new())
                }
                // Otherwise a built-in operator (lives at the root, simple path).
                None => RtValue::Builtin(path.local().to_string()),
            }),
            Expr::MethodRef { type_name, method } => {
                Ok(match self.methods.get(&(type_name.clone(), method.clone())) {
                    Some(m) => {
                        let params = m.params.iter().cloned().zip(m.kinds.iter().copied()).collect();
                        let body = m.body.clone();
                        self.make_closure(heap, params, body, &Env::new())
                    }
                    None => RtValue::BuiltinMethod(type_name.clone(), method.clone()),
                })
            }
            Expr::If(..) => {
                // Walks a right-leaning `if`/`else-if` chain (`(if c1 b1 (if
                // c2 b2 (if c3 b3 ...)))`, exactly what `cond`'s expansion —
                // and `compiler.rs`'s own tag-dispatch `compile-value`,
                // `compile-construct-sexpr`'s variant-`eq` chain, etc. —
                // produce) iteratively instead of recursing once per link.
                // Recursing (`self.eval(heap, els, env)` on an `els` that's
                // itself another `If`) would re-enter this whole match via a
                // *new* Rust call frame per chain link — with debug builds'
                // large, uninlined `eval` frames, a `compile-value`-sized
                // chain (~20 tags) nested a few `Construct`/`Match` levels
                // deep was enough to blow even a worker thread's default
                // stack (discovered compiling a `dolist`-based function,
                // Stage 8 of the Sexpr-representation plan, `docs/implementation-log.md`).
                // Only the condition's own (shallow) evaluation and whichever
                // single leaf branch is ultimately taken still recurse.
                let mut cur = t;
                loop {
                    let (c, then, els) = match &cur.expr {
                        Expr::If(c, then, els) => (c, then, els),
                        _ => unreachable!("loop only ever advances `cur` to another Expr::If"),
                    };
                    match self.eval(heap, c, env)? {
                        RtValue::Bool(true) => break self.eval(heap, then, env),
                        RtValue::Bool(false) => {
                            if matches!(els.expr, Expr::If(..)) {
                                cur = els;
                            } else {
                                break self.eval(heap, els, env);
                            }
                        }
                        _ => break Err(EvalError::Internal("if condition is not a bool".into())),
                    }
                }
            }
            Expr::Let(binds, body) => {
                // CL `let`: binding values are evaluated in the outer environment.
                // Slot routing comes from each binding's checked type
                // (`val.ty`) — static information carried by the AST, never
                // the evaluated value's shape.
                let mut child = env.clone();
                for (name, val) in binds {
                    let v = self.eval(heap, val, env)?;
                    let kind = self.heap_repr_kind(&val.ty);
                    let s = self.slot(heap, kind, v)?;
                    child.push((name.clone(), s));
                }
                self.eval_seq(heap, body, &child)
            }
            Expr::Labels { defs, body } => {
                // Give every function a placeholder slot *before* building any
                // closure, so each closure's captured environment (`child`)
                // already contains all of them — including its own slot, the
                // self-reference `lambda` has no way to express. Only once
                // `child` is complete does each slot get overwritten with the
                // real closure that captured it.
                //
                // The placeholders are *heap cells* (unlike other
                // function-typed bindings, which stay `Native` because a
                // function value may also be a heap-less `RtValue::Builtin`):
                // a labels sibling is always a closure, and a heap cell makes
                // the whole mutual-recursion knot — cell -> closure box ->
                // sibling cell -> ... — a plain heap cycle mark-sweep
                // reclaims once the labels scope dies. (The old `Rc`-based
                // representation leaked exactly this cycle by design.)
                let mut child = env.clone();
                let mut slots: Vec<Slot> = Vec::with_capacity(defs.len());
                for (name, _, _) in defs {
                    let s = self.slot(heap, SlotKind::Heap, RtValue::Sexpr(Value::Empty))?;
                    child.push((name.clone(), s.clone()));
                    slots.push(s);
                }
                for ((_, params, fbody), slot) in defs.iter().zip(&slots) {
                    let kinds: Vec<(String, SlotKind)> = params
                        .iter()
                        .map(|(n, ty)| (n.clone(), self.heap_repr_kind(ty)))
                        .collect();
                    let closure = self.make_closure(heap, kinds, fbody.clone(), &child);
                    slot.set(heap, closure)?;
                }
                self.eval_seq(heap, body, &child)
            }
            Expr::Call(name, args) => {
                let (argv, _slots) = self.eval_args(heap, args, env)?;
                // A `(compile name)`d function dispatches to native code
                // first — checked ahead of `fns` so a later recompile (not
                // possible yet, but the ordering is the cheap-to-get-right
                // choice) would naturally take precedence over the
                // tree-walked body.
                if let Some(compiled) = self.compiled.borrow().get(name) {
                    // `compile_function` never registers a `compiled` entry
                    // without first going through `compiled_fn_body`, which
                    // requires `fns[name].sig` to be `Some` — so this is an
                    // internal invariant, not a user-reachable error.
                    let (_, ret_ty) = self
                        .fns
                        .get(name)
                        .and_then(|f| f.sig.as_ref())
                        .expect("a compiled function always has a type signature");
                    return self.call_compiled(heap, compiled, &argv, ret_ty);
                }
                if let Some(f) = self.fns.get(name) {
                    self.apply(heap, f, argv)
                } else if name.is_simple() {
                    match self.eval_builtin(heap, name.local(), &argv) {
                        Some(result) => result,
                        None => Err(EvalError::NoSuchFunction(name.to_string())),
                    }
                } else {
                    Err(EvalError::NoSuchFunction(name.to_string()))
                }
            }
            Expr::Assoc { type_name, method, args, .. } => {
                let (argv, _slots) = self.eval_args(heap, args, env)?;
                let key = (type_name.clone(), method.clone());
                // `compiled_methods`'s counterpart of `Expr::Call`'s own
                // `compiled` check above — see [`Self::call_compiled`]'s doc
                // comment for the one extra risk a method's receiver carries
                // that a plain function's parameters never do.
                if let Some(compiled) = self.compiled_methods.borrow().get(&key) {
                    let (_, ret_ty) = self
                        .methods
                        .get(&key)
                        .and_then(|f| f.sig.as_ref())
                        .expect("a compiled method always has a type signature");
                    return self.call_compiled(heap, compiled, &argv, ret_ty);
                }
                if let Some(m) = self.methods.get(&key) {
                    self.apply(heap, m, argv)
                } else {
                    let recv_ty = args.first().map(|a| &a.ty);
                    match eval_builtin_method(self, heap, type_name, method, recv_ty, &argv, &t.ty) {
                        Some(result) => result,
                        None => Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, method))),
                    }
                }
            }
            Expr::TraitCall { method, .. } => {
                // The checker only produces this node inside a where-bounded
                // *generic* body (`Checker::check_instance_method`'s
                // type-variable branch) — which, post-monomorphization, is a
                // diagnostics-only artifact that never executes: each
                // specialization re-checks the same call with the receiver
                // type concrete and resolves it statically to `Expr::Assoc`.
                // The old runtime dispatch (reading the receiver value's own
                // type tag) was the last value-shape fallback in the
                // evaluator; reaching here now is a checker/interpreter bug.
                Err(EvalError::Internal(format!(
                    "TraitCall `{}` reached the evaluator — an erased generic body executed",
                    method
                )))
            }
            Expr::Construct { type_name, variant, args, mutable } => {
                if is_sexpr_type(type_name) {
                    self.construct_sexpr(heap, *variant, args, env)
                } else if *mutable {
                    let (fields, _slots) = self.eval_args(heap, args, env)?;
                    let mem_fields = fields
                        .iter()
                        .map(|f| rtvalue_to_struct_field(heap, f))
                        .collect::<Result<Vec<Value>, EvalError>>()?;
                    Ok(RtValue::Sexpr(heap.alloc_struct(type_name.to_string(), mem_fields)))
                } else {
                    // An enum value (`Option`/`Result`/user `defenum`) — see
                    // `build_enum_value`'s doc comment for the heap/native
                    // duality this goes through (the same one `Scope<V>`
                    // already has, since `Option<llvm-value>` and friends
                    // are exactly the (typelisp-hosted) compiler's own
                    // native-only instantiation).
                    let (fields, _slots) = self.eval_args(heap, args, env)?;
                    Ok(build_enum_value(heap, type_name.clone(), *variant, fields))
                }
            }
            Expr::Lambda { params, body } => {
                // Capture the current environment (shared slots) for the
                // closure, recording each parameter's slot routing from its
                // declared type.
                let kinds = params.iter().map(|(n, ty)| (n.clone(), self.heap_repr_kind(ty))).collect();
                Ok(self.make_closure(heap, kinds, body.clone(), env))
            }
            Expr::Apply(callee, args) => {
                let f = self.eval(heap, callee, env)?;
                // The callee is a GC-heap value now (a closure box) — anchor
                // it before evaluating the arguments, whose own allocations
                // may collect. Without this, an unnamed callee (e.g.
                // `((make-adder 1) ...)`) has no binding keeping its box
                // alive across the argument churn.
                let _f_anchor = self.native_slot(f.clone());
                let (argv, _slots) = self.eval_args(heap, args, env)?;
                match f {
                    RtValue::Sexpr(Value::Boxed(id)) if heap.is_closure(id) => {
                        let token = heap.closure_token(id);
                        // Cloned out so the body survives even if the box —
                        // and with it this table entry — is swept mid-call.
                        let cb = self
                            .closure_bodies
                            .borrow()
                            .get(&token)
                            .cloned()
                            .ok_or_else(|| EvalError::Internal("closure body missing from side table".into()))?;
                        if cb.params.len() != argv.len() {
                            return Err(EvalError::Internal("closure arity mismatch".into()));
                        }
                        // Rebuild the captured environment in layout order.
                        // Heap captures are re-owned (`Heap::adopt_cell`) so
                        // each frame binding keeps its cell alive on its
                        // own, independent of the closure box.
                        let heap_env: Vec<Value> = heap.closure_env(id).to_vec();
                        let mut cenv: Env = Vec::with_capacity(cb.layout.len() + argv.len());
                        for (name, cap) in &cb.layout {
                            let slot = match cap {
                                Capture::Heap(i) => match heap_env[*i] {
                                    Value::Boxed(cell) => Slot::Heap(heap.adopt_cell(cell)),
                                    other => {
                                        return Err(EvalError::Internal(format!(
                                            "closure env slot {} is not a cell: {:?}",
                                            i, other
                                        )))
                                    }
                                },
                                Capture::Native(rc) => Slot::Native(rc.clone()),
                            };
                            cenv.push((name.clone(), slot));
                        }
                        for ((n, kind), v) in cb.params.iter().zip(argv) {
                            let s = self.slot(heap, *kind, v)?;
                            cenv.push((n.clone(), s));
                        }
                        self.eval_seq(heap, &cb.body, &cenv)
                    }
                    // The compiled peer of the interp-closure arm above —
                    // reached whenever this `Apply`'s callee was itself
                    // produced by compiled code: a compiled function
                    // returning a `Fn` (decoded via `is_boxed_sexpr_type`'s
                    // `Type::Fn` arm), or a compiled closure the interpreter
                    // is merely threading through a chain of `Expr::Apply`s
                    // it's driving (e.g. `((make-adder n) x)` where
                    // `make-adder` is `compile`d). Marshals `argv`/decodes
                    // the result exactly like a top-level `call_compiled`
                    // call, via the same two halves that split out of it.
                    RtValue::Sexpr(Value::Boxed(id)) if heap.is_compiled_closure(id) => {
                        let (int_args, crossing_roots) = self.encode_crossing_args(heap, &argv)?;
                        crate::compile::runtime::set_active_heap(heap as *mut Heap);
                        let raw = Self::call_closure_box(heap, id, &int_args);
                        for _ in 0..crossing_roots {
                            heap.pop_root();
                        }
                        self.decode_compiled_return(heap, raw, &t.ty)
                    }
                    RtValue::Builtin(name) => match self.eval_builtin(heap, &name, &argv) {
                        Some(r) => r,
                        None => Err(EvalError::NoSuchFunction(name)),
                    },
                    RtValue::BuiltinMethod(type_name, method) => {
                        let recv_ty = args.first().map(|a| &a.ty);
                        match eval_builtin_method(self, heap, &type_name, &method, recv_ty, &argv, &t.ty) {
                            Some(r) => r,
                            None => Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, method))),
                        }
                    }
                    _ => Err(EvalError::Internal("apply of a non-function value".into())),
                }
            }
            Expr::FieldGet(obj, idx) => {
                let id = expect_struct_box(&self.eval(heap, obj, env)?)?;
                let raw = heap.struct_field(id, *idx);
                // A `FieldGet`'s checked type *is* the field's declared type
                // (`Checker::check_defstruct`'s accessor synthesis) — always
                // concrete now that generic accessors are monomorphized —
                // so the decode is fully type-directed; see
                // `decode_field_typed`.
                Ok(decode_field_typed(heap, raw, &t.ty))
            }
            Expr::FieldSet(obj, idx, value) => {
                let id = expect_struct_box(&self.eval(heap, obj, env)?)?;
                let v = self.eval(heap, value, env)?;
                let mv = rtvalue_to_struct_field(heap, &v)?;
                heap.struct_set_field(id, *idx, mv);
                Ok(RtValue::Unit)
            }
            Expr::Match(scrut, arms) => {
                let v = self.eval(heap, scrut, env)?;
                for arm in arms {
                    if let Some(binds) = match_pattern(heap, &arm.pat, &v) {
                        // Each binding's slot routing was baked into the
                        // pattern at check time (`Pattern::Bind`'s bool) —
                        // the one binding site whose type the evaluator
                        // can't read off its own AST node.
                        let mut child = env.clone();
                        for (n, kind, bv) in binds {
                            let s = self.slot(heap, kind, bv)?;
                            child.push((n, s));
                        }
                        return self.eval_seq(heap, &arm.body, &child);
                    }
                }
                Err(EvalError::Internal("no matching match arm".into()))
            }
            Expr::Set(name, value) => {
                let v = self.eval(heap, value, env)?;
                let cell = env_get(env, name).ok_or_else(|| EvalError::Unbound(name.clone()))?.clone();
                cell.set(heap, v.clone())?;
                Ok(v)
            }
            // See `Expr::Global`'s arm above for why a promoted global must
            // be written through the same permanent-root storage a
            // compiled write would use, not the plain `Slot` below.
            Expr::SetGlobal(path, value) => {
                let v = self.eval(heap, value, env)?;
                if let Some(&id) = self.compiled_globals.borrow().get(path) {
                    // See `Expr::Global`'s arm for why `id` needs resolving
                    // through `global_perm_idx` rather than being used as
                    // the `Heap::permanent_root` position directly.
                    let perm_idx = crate::compile::runtime::global_perm_idx(id)
                        .ok_or_else(|| EvalError::Internal(format!("global \"{}\": unknown compiled id {}", path, id)))?;
                    let mv = rtvalue_to_struct_field(heap, &v)?;
                    heap.set_permanent_root(perm_idx, mv);
                    Ok(v)
                } else {
                    let cell = self
                        .globals
                        .get(path)
                        .ok_or_else(|| EvalError::Unbound(path.to_string()))?
                        .clone();
                    cell.set(heap, v.clone())?;
                    Ok(v)
                }
            }
            Expr::Loop(body) => loop {
                if let Some(v) = self.eval_loop_body(heap, body, env)? {
                    return Ok(v);
                }
            },
            Expr::Break => Err(EvalError::Break),
            Expr::Return(value) => {
                let v = match value {
                    Some(e) => self.eval(heap, e, env)?,
                    None => RtValue::Unit,
                };
                Err(EvalError::Return(Box::new(v)))
            }
            Expr::Panic(msg) => match self.eval(heap, msg, env)? {
                RtValue::Str(s) => Err(EvalError::Panic(s.to_string())),
                _ => Err(EvalError::Panic(String::new())),
            },
            Expr::Quote(qs) => {
                // One `sync_roots` call up front (not nested inside
                // `alloc_quoted`'s recursion — see its doc comment for why)
                // covers every *other* live slot for the whole build.
                self.sync_roots(heap);
                let v = alloc_quoted(heap, qs)?;
                Ok(RtValue::Sexpr(v))
            }
        }
    }

    /// Construct a `Sexpr` value (see `check::registry::sexpr_def` for the
    /// variant layout this mirrors), allocating into the GC-managed cons heap
    /// rather than `RtValue::Data`.
    fn construct_sexpr(
        &self,
        heap: &mut Heap,
        variant: usize,
        args: &[Typed],
        env: &Env,
    ) -> Result<RtValue, EvalError> {
        let (vs, _slots) = self.eval_args(heap, args, env)?;
        let v = match variant {
            SEXPR_NIL => Value::Empty,
            SEXPR_INT => Value::Int(rt_i64(&vs[0])?),
            SEXPR_FLOAT => heap.alloc_float(rt_f64(&vs[0])?),
            SEXPR_CHAR => Value::Char(rt_char(&vs[0])?),
            SEXPR_BOOL => Value::Bool(rt_bool(&vs[0])?),
            // `(Sym x)` where `x : Symbol`. A `Symbol` value is already carried
            // as `RtValue::Sexpr(Value::Symbol(id))`, so the field value *is*
            // the resulting `Sexpr::Sym` — extract its `Value::Symbol` directly
            // (no re-interning through a string).
            SEXPR_SYM => rt_sexpr(&vs[0])?,
            SEXPR_STR => heap.alloc_string(rt_str(vs[0].clone())?),
            SEXPR_CONS => {
                let car = rt_sexpr(&vs[0])?;
                let cdr = rt_sexpr(&vs[1])?;
                // `_slots` keeps `car`/`cdr` rooted (via the registry) through
                // this allocation, which may trigger a GC.
                self.sync_roots(heap);
                heap.cons(car, cdr).map_err(|e| EvalError::Panic(e.to_string()))?
            }
            SEXPR_BIGNUM => heap.alloc_bignum((*rt_bignum(&vs[0])?).clone()),
            SEXPR_RATIO => heap.alloc_ratio((*rt_ratio(&vs[0])?).clone()),
            _ => return Err(EvalError::Internal("sexpr: unknown variant".into())),
        };
        Ok(RtValue::Sexpr(v))
    }

    /// The outcome of evaluating one step (a body expression) of a `loop`:
    /// either an ordinary value, or a `break`/`return` signal already
    /// resolved to the loop's exit value.
    fn eval_loop_step(&self, heap: &mut Heap, t: &Typed, env: &Env) -> Result<Step, EvalError> {
        match self.eval(heap, t, env) {
            Ok(_) => Ok(Step::Continue),
            Err(EvalError::Break) => Ok(Step::Exit(RtValue::Unit)),
            Err(EvalError::Return(v)) => Ok(Step::Exit(*v)),
            Err(e) => Err(e),
        }
    }

    /// Run one pass over a loop's body expressions. Returns `Some(exit_value)`
    /// if a `break`/`return` ended the loop partway through, `None` to
    /// continue iterating.
    fn eval_loop_body(&self, heap: &mut Heap, body: &[Typed], env: &Env) -> Result<Option<RtValue>, EvalError> {
        for e in body {
            if let Step::Exit(v) = self.eval_loop_step(heap, e, env)? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    /// Apply a function/method body: bind its parameters to `args` (each
    /// slot routed by the `FnDef`'s registration-time `kinds`) and run the
    /// body.
    fn apply(&self, heap: &mut Heap, def: &FnDef, args: Vec<RtValue>) -> Result<RtValue, EvalError> {
        if def.params.len() != args.len() {
            return Err(EvalError::Internal("arity mismatch".into()));
        }
        let mut env: Env = Vec::with_capacity(args.len());
        for ((name, kind), v) in def.params.iter().zip(def.kinds.iter()).zip(args) {
            let s = self.slot(heap, *kind, v)?;
            env.push((name.clone(), s));
        }
        self.eval_seq(heap, &def.body, &env)
    }

    /// Runs already-evaluated `argv` (`param_tys`-typed, `ret_ty`-returning)
    /// through `compiled` instead of tree-walking — shared by `Expr::Call`
    /// and `Expr::Assoc`, the two places eval can reach a `(compile
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
    /// `Expr::Call` already would for a general-ADT parameter — a clear
    /// internal error here, not a silent misread of unrelated bits.
    fn call_compiled(&self, heap: &mut Heap, compiled: &crate::compile::CompiledFn, argv: &[RtValue], ret_ty: &Type) -> Result<RtValue, EvalError> {
        let (int_args, crossing_roots) = self.encode_crossing_args(heap, argv)?;
        // Registers `heap` as this thread's active `Heap` (see
        // `compile::runtime::set_active_heap`'s doc comment) so any
        // `rt-cons`/`rt-car`/... call the compiled code makes — directly or
        // transitively through another compiled function — resolves against
        // the right heap. Done on every call rather than once, since it's
        // one pointer store and there's no cheaper place to detect "this
        // callee might transitively touch the heap" ahead of time.
        crate::compile::runtime::set_active_heap(heap as *mut Heap);
        let raw = compiled.call(&int_args);
        for _ in 0..crossing_roots {
            heap.pop_root();
        }
        self.decode_compiled_return(heap, raw, ret_ty)
    }

    /// [`Self::call_compiled`]'s argument-marshaling half, factored out so
    /// [`Self::eval`]'s `Expr::Apply` arm can reuse it when the callee is a
    /// `BoxedObj::CompiledClosure` rather than a top-level `(compile ...)`d
    /// function — both cross the exact same interp-`RtValue` -> compiled-ABI
    /// `i64` boundary. Returns the encoded `int_args` plus how many roots it
    /// pushed (over `argv`'s heap-backed elements only); the caller must pop
    /// exactly that many once the call this feeds into has returned. See
    /// `call_compiled`'s (pre-refactor) doc comment for why encoding reads
    /// `argv`'s own runtime shape rather than each parameter's static type,
    /// and why every heap-backed argument is rooted for the marshaling+call
    /// window.
    fn encode_crossing_args(&self, heap: &mut Heap, argv: &[RtValue]) -> Result<(Vec<i64>, usize), EvalError> {
        let mut crossing_roots = 0usize;
        let mut int_args: Vec<i64> = Vec::with_capacity(argv.len());
        for v in argv {
            let encoded = match v {
                // An *interpreted* closure box must not silently cross this
                // boundary: compiled code represents function values as its
                // own `BoxedObj::CompiledClosure`, and a tagged interp
                // closure handed over as a plain `Sexpr` would be
                // dereferenced as one — same "clear internal error, not a
                // silent misread" stance the pre-6b `RtValue::Closure`
                // rejection took. A *compiled* closure (also a tagged
                // `Sexpr`) needs no such rejection — it falls through to the
                // ordinary `RtValue::Sexpr` arm just below like any other
                // boxed value, since compiled code on both sides of this
                // call already agrees on `BoxedObj::CompiledClosure`'s shape.
                RtValue::Sexpr(Value::Boxed(id)) if heap.is_closure(*id) => Err(EvalError::Internal(
                    "compiled call: an interpreted closure cannot be passed to compiled code".into(),
                )),
                RtValue::Sexpr(sv) => {
                    heap.push_root(*sv);
                    crossing_roots += 1;
                    Ok(crate::compile::runtime::encode(*sv))
                }
                RtValue::Int(n) => Ok(*n),
                // The remaining scalar crossings, by the same encodings
                // compiled code uses internally: `bool` and `char` are raw
                // `i64`s (0/1 / code point); a string becomes a tagged heap
                // `Value::Str` (what `rt_str_*` expect).
                RtValue::Bool(b) => Ok(i64::from(*b)),
                RtValue::Char(c) => Ok(*c as i64),
                // A compiled `f64` is its raw `f64::to_bits` pattern carried in
                // an `i64` (`compile-float`/`llvm_builder_build_float_op`'s
                // convention) — the exact inverse of the `Type::F64` return
                // decode below.
                RtValue::Float(f) => Ok(f.to_bits() as i64),
                RtValue::Str(s) => {
                    let sv = heap.alloc_string(s.to_string());
                    heap.push_root(sv);
                    crossing_roots += 1;
                    Ok(crate::compile::runtime::encode(sv))
                }
                // `bignum`/`ratio` cross exactly like `Str` above: an
                // interpreted `RtValue::Bignum`/`Ratio` is an `Rc`-managed
                // value with no GC-heap presence of its own, so it's cloned
                // onto the GC heap's `BoxedObj::Bignum`/`Ratio` store fresh
                // for this call, rooted, and encoded — the tagged `i64`
                // `rt_bignum_*`/`rt_ratio_*` (`typelisp-rt`) expect.
                RtValue::Bignum(n) => {
                    let sv = heap.alloc_bignum(n.as_ref().clone());
                    heap.push_root(sv);
                    crossing_roots += 1;
                    Ok(crate::compile::runtime::encode(sv))
                }
                RtValue::Ratio(r) => {
                    let sv = heap.alloc_ratio(r.as_ref().clone());
                    heap.push_root(sv);
                    crossing_roots += 1;
                    Ok(crate::compile::runtime::encode(sv))
                }
                other => Err(EvalError::Internal(format!("compiled call: unsupported argument {:?}", other))),
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

    /// [`Self::call_compiled`]'s return-value half, factored out for the
    /// same reason as [`Self::encode_crossing_args`] — a direct
    /// `Expr::Apply` on a `BoxedObj::CompiledClosure` decodes its raw `i64`
    /// result exactly like a top-level compiled call's, by the callee's
    /// declared (here: the closure's `Type::Fn` return) type.
    fn decode_compiled_return(&self, heap: &mut Heap, raw: i64, ret_ty: &Type) -> Result<RtValue, EvalError> {
        Ok(if self.is_boxed_sexpr_type(ret_ty) {
            RtValue::Sexpr(crate::compile::runtime::decode(raw))
        } else if matches!(ret_ty, Type::Bool) {
            // Compiled code represents a `bool` as a raw 0/1 `i64` (LLVM
            // `icmp` results, zero-extended); decode it by the declared
            // return type so an interpreted `if` over a compiled predicate
            // (`i32::equals`, ...) sees a real `RtValue::Bool`.
            RtValue::Bool(raw != 0)
        } else if matches!(ret_ty, Type::Char) {
            // A compiled `char` is a raw `i64` Unicode scalar value (the
            // widened `char->int` payload `compile-char`/`compile-sexpr-field`
            // produce — the exact inverse of the `*c as i64` a `char`
            // *argument* crosses as, above). Decode it back to a real
            // `RtValue::Char` so a `char`-returning compiled function
            // (`(defun first-char (...) char ...)`) interoperates with the
            // interpreter, rather than surfacing its code point as a bare
            // `RtValue::Int`. A compiled `char` only ever holds a value that
            // was a valid `char` on the way in, so a decode failure here is
            // an internal-invariant break, not a user-reachable error.
            match char::from_u32(raw as u32) {
                Some(c) => RtValue::Char(c),
                None => {
                    return Err(EvalError::Internal(format!(
                        "compiled call returned {} for a `char` result, which is not a valid Unicode scalar value",
                        raw
                    )))
                }
            }
        } else if matches!(ret_ty, Type::F64) {
            // A compiled `f64` result is its raw bit pattern in the `i64`
            // return register (`llvm_builder_build_float_op`'s final
            // `bitcast`); reinterpret it back to an `f64`, the inverse of the
            // `RtValue::Float` argument encode above.
            RtValue::Float(f64::from_bits(raw as u64))
        } else if matches!(ret_ty, Type::Bignum | Type::Ratio) {
            // `bignum`/`ratio` aren't `Type::Named` (unlike a `defstruct`/
            // `Vector<T>`), so `is_boxed_sexpr_type` above never catches
            // them — without this arm, `raw` (a tagged `TAG_BOXED` pointer)
            // would silently fall through to the plain `RtValue::Int(raw)`
            // case below and be misread as an ordinary integer. `raw`
            // decodes to a `Value::Boxed` id (`rt_bignum_new`/`rt_ratio_from_bignums`
            // and every `rt_bignum_*`/`rt_ratio_*` arithmetic/conversion
            // primitive already return one, the same tagged representation
            // a `Str` argument crosses as above), so re-box its `BigInt`/
            // `BigRational` into a fresh interpreter-side `Rc`, mirroring
            // `RtValue::Bignum`/`Ratio`'s own "Rc, no GC-heap presence"
            // shape.
            match crate::compile::runtime::decode(raw) {
                Value::Boxed(id) if matches!(ret_ty, Type::Bignum) => RtValue::Bignum(Rc::new(heap.bignum_value(id).clone())),
                Value::Boxed(id) => RtValue::Ratio(Rc::new(heap.ratio_value(id).clone())),
                other => {
                    return Err(EvalError::Internal(format!(
                        "compiled call returned {:?} for a bignum/ratio result, which is not a boxed Sexpr",
                        other
                    )))
                }
            }
        } else {
            RtValue::Int(raw)
        })
    }

    /// Invokes a `BoxedObj::CompiledClosure` directly from interp Rust code
    /// — the `Expr::Apply` counterpart of a top-level `(compile ...)`d
    /// function call, for a callee produced by compiled code (returned
    /// across the boundary, or built and threaded through a chain of
    /// `Expr::Apply`s the interpreter is itself driving). `args` are
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

    /// Whether `ty`'s compiled representation crosses the typelisp-call-
    /// syntax/compiled-code boundary as a tagged `Sexpr`
    /// (`compile::runtime::encode`/`decode`, [`Self::call_compiled`]) rather
    /// than a plain `i64` — `Sexpr` itself, any `Type::Named` this `Interp`
    /// has recorded in [`Self::struct_types`] (Stage 3 of the Sexpr/RtValue
    /// unification plan, `docs/implementation-log.md`), an enum type
    /// (`Option`/`Result`/`Error`/user `defenum`) — since the enum-
    /// representation unification's compiler flip, a compiled function
    /// returning e.g. `Option<i64>` really does hand back a tagged
    /// `Value::Boxed` at a `BoxedObj::Enum`, not the raw box address the
    /// pre-flip design left undecoded here (this is where that gap used to
    /// surface a bare `RtValue::Int` for an enum-typed return) — or a
    /// `Type::Fn`: since the closure-representation unification's compiled
    /// flip (Stage 2), a compiled function returning a closure hands back a
    /// tagged `Value::Boxed` at a `BoxedObj::CompiledClosure` exactly the
    /// same way, so without this arm the same kind of gap would surface
    /// (a bare `RtValue::Int` for a `Fn`-typed return). Every enum type
    /// compiled code can ever mention is heap-repr by construction (see
    /// `ast_bridge::struct_field_kind`'s doc comment), so `is_enum_path`
    /// alone is enough — no need to also check field representability the
    /// way `Interp::enum_fields_representable` does for a binding.
    fn is_boxed_sexpr_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Fn(..)) || matches!(ty, Type::Named(p, _) if is_sexpr_type(p) || self.struct_types.contains(p) || self.is_enum_path(p))
    }

    /// Finds the registered `(Path, String)` key for a `"type::method"`
    /// name — `None` for a plain name (no `"::"`) or a method that isn't
    /// registered. The match is purely textual on `name`'s `"::"` split,
    /// then by `Path::local()` alone (discarding any module qualification) —
    /// matching `compile-call`'s own "discard qualification, look up by
    /// local name" treatment of a *caller's* method/function references. A
    /// real receiver type is never module-qualified in this language
    /// (`defmethod` always registers under the type's own resolved `Path`,
    /// and `Checker::check_defmethod` requires that type to already be
    /// registered), so matching by local name alone can't collide across
    /// two different types sharing a method name. Shared by
    /// [`Self::resolve_fn_def`] (the lookup) and [`Self::compile_function`]
    /// (which `(compile name)` for a method) — both need the exact same
    /// `(Path, String)` key.
    fn method_key(&self, name: &str) -> Option<(Path, String)> {
        let (type_name, method) = name.split_once("::")?;
        self.methods.keys().find(|(p, m)| p.local() == type_name && m == method).cloned()
    }

    /// Resolves a `(compile name)` argument against either `self.fns` (a
    /// plain name, a top-level `defun`) or `self.methods` (a `"type::method"`
    /// name, an instance/static `defmethod` — including a `defstruct`'s
    /// auto-generated field accessor/setter, whose body is the `Expr::FieldGet`/
    /// `FieldSet` `compile-field-get`/`compile-field-set` exist to compile in
    /// the first place). See [`Self::method_key`] for how `name` decides
    /// which of the two this is.
    fn resolve_fn_def(&self, name: &str) -> Result<&FnDef, EvalError> {
        if name.contains("::") {
            let key = self.method_key(name).ok_or_else(|| EvalError::NoSuchFunction(name.to_string()))?;
            self.methods.get(&key).ok_or_else(|| EvalError::NoSuchFunction(name.to_string()))
        } else {
            self.fns.get(&Path::root(name)).ok_or_else(|| EvalError::NoSuchFunction(name.to_string()))
        }
    }

    /// Looks up `name`'s registered `defun`/`defmethod` body (see
    /// [`Self::resolve_fn_def`]), enforcing the two constraints every entry
    /// point into the compiler shares: a real type signature (so an LLVM
    /// function type can be built — never set for a `defmacro`) and a
    /// single-expression body (`compile`'s long-standing scope, unchanged by
    /// labels/closures Stage 3). For an instance method, `params`/`sig.0`
    /// already carry the receiver as element `0` (`Checker::check_defmethod`
    /// pushes the receiver's own type onto `sig_params` before the method's
    /// declared parameters) — so it flows through exactly like any other
    /// parameter here, no special-casing needed. Shared by
    /// [`Self::add_compiled_function`] (the actual AST-bridge step) and
    /// [`Self::compile_function`] (which needs the body slightly earlier —
    /// to collect `Expr::Call` targets, see that method's doc comment —
    /// before `add_compiled_function` ever runs).
    fn compiled_fn_body(&self, name: &str) -> Result<(Vec<(String, Type)>, Typed), EvalError> {
        let f = self.resolve_fn_def(name)?;
        let sig = f
            .sig
            .as_ref()
            .ok_or_else(|| EvalError::Panic(format!("compile: \"{}\" has no type signature (is it a defmacro?)", name)))?;
        if f.body.len() != 1 {
            return Err(EvalError::Panic(format!(
                "compile: \"{}\" has a multi-expression body, not yet supported",
                name
            )));
        }
        let params = f.params.iter().cloned().zip(sig.0.iter().cloned()).collect();
        Ok((params, f.body[0].clone()))
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
    /// construction — see `ast_bridge::struct_field_kind`'s doc comment —
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
            .globals
            .get(path)
            .ok_or_else(|| EvalError::Internal(format!("compile: global \"{}\" is not defined", path)))?;
        let v = slot.get(heap);
        let value = rtvalue_to_struct_field(heap, &v).map_err(|_| {
            EvalError::Panic(format!("compile: global \"{}\" has a type not yet supported for compiled access", path))
        })?;
        let id = crate::compile::runtime::global_new(heap, value);
        self.compiled_globals.borrow_mut().insert(path.clone(), id);
        Ok(id)
    }

    /// Compiles the `defun` named `name` (looked up in `self.fns`) into one
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
    /// only uses node shapes `compile::ast_bridge::ast_to_sexpr` has a real
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

        // Every global this body reads/assigns must have a compiled-global
        // slot before translation starts — `ast_to_sexpr` looks each one up
        // by id, not by name (see `Ctx::globals`'s doc comment), so there is
        // nothing to resolve lazily mid-translation the way `compile-call`'s
        // `get-function` can for an ordinary function name.
        for target in crate::compile::ast_bridge::collect_global_targets(&body) {
            self.promote_global(heap, &target)?;
        }
        let compiled_globals = self.compiled_globals.borrow();
        // `ast_bridge` is deliberately `Registry`-free, so hand it the enum
        // membership (`global_field_kind`'s kind-`10` classification) as
        // plain data, the same way `struct_types` already crosses. Collected
        // fresh per compilation — compiling is rare enough that keeping a
        // second always-current set alongside `enum_defs` isn't worth it.
        let enum_types: HashSet<Path> = self.enum_defs.keys().cloned().collect();

        // A top-level `defun` has no enclosing lexical scope to capture
        // *from*, so its own `cell_names` (closure-representation
        // unification, Stage 4) is exactly its own params/`let`-bindings
        // that some nested `lambda`/`labels` in `body` captures — see
        // `ast_bridge::names_captured_by_nested`'s doc comment.
        let cell_names = crate::compile::freevars::names_captured_by_nested(std::slice::from_ref(&body));

        // Builds `((a . kind) (b . kind) ...)`, the `Sexpr` list of typed
        // name pairs `compiler.rs`'s `bind-params` walks to know which
        // logical argument-array slot binds to which name — and, for the
        // automatic `ClosureBox` retain/release insertion work (`kind = 1`)
        // and the Sexpr GC-root insertion work (`kind = 2`, Stage 6 of the
        // Sexpr-representation plan, `docs/implementation-log.md`), what kind of binding
        // it is at all. Shares `ast_bridge::tagged_sym_list`'s exact
        // construction (a `labels`/`lambda` parameter or captured-name list
        // needs the identical shape) rather than re-deriving it here, so
        // the two can never desync.
        let param_list = match crate::compile::ast_bridge::tagged_sym_list(heap, &params, &self.struct_types, &enum_types, &cell_names) {
            Ok(v) => v,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        heap.push_root(param_list);
        let body_sexpr =
            match crate::compile::ast_bridge::ast_to_sexpr(heap, &body, &self.struct_types, &enum_types, &compiled_globals, &cell_names) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root(); // param_list
                    return Err(EvalError::Panic(e.to_string()));
                }
            };
        heap.pop_root(); // param_list

        let compiler_path = Path::root("compile-function");
        let compiler_def = self.fns.get(&compiler_path).ok_or_else(|| {
            EvalError::Internal("compile: compiler body not loaded — call load_compiler first".into())
        })?;
        self.apply(
            heap,
            compiler_def,
            vec![
                RtValue::LlvmModule(module),
                RtValue::Str(internal_name.into()),
                RtValue::Sexpr(param_list),
                RtValue::Sexpr(body_sexpr),
            ],
        )?;
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
    /// ([`crate::compile::ast_bridge::ast_to_sexpr_for_global_init`],
    /// `compiler.rs`'s `compile-global-init`) instead of an ordinary
    /// translated function body — `value` may itself reference other
    /// globals (an earlier `defvar`'s value), so the same promotion pass
    /// applies here too.
    pub(crate) fn add_compiled_global_init(
        &self,
        heap: &mut Heap,
        module: Rc<RefCell<Module<'static>>>,
        internal_name: &str,
        value: &Typed,
    ) -> Result<(), EvalError> {
        for target in crate::compile::ast_bridge::collect_global_targets(value) {
            self.promote_global(heap, &target)?;
        }
        let compiled_globals = self.compiled_globals.borrow();
        // See `add_compiled_function`'s own copy of this for why the enum
        // membership crosses as a per-compilation set.
        let enum_types: HashSet<Path> = self.enum_defs.keys().cloned().collect();

        let param_list = match crate::compile::ast_bridge::tagged_sym_list(heap, &[], &self.struct_types, &enum_types, &HashSet::new()) {
            Ok(v) => v,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        heap.push_root(param_list);
        let body_sexpr =
            match crate::compile::ast_bridge::ast_to_sexpr_for_global_init(heap, value, &self.struct_types, &enum_types, &compiled_globals) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root(); // param_list
                    return Err(EvalError::Panic(e.to_string()));
                }
            };
        heap.pop_root(); // param_list

        let compiler_path = Path::root("compile-function");
        let compiler_def = self.fns.get(&compiler_path).ok_or_else(|| {
            EvalError::Internal("compile: compiler body not loaded — call load_compiler first".into())
        })?;
        self.apply(
            heap,
            compiler_def,
            vec![
                RtValue::LlvmModule(module),
                RtValue::Str(internal_name.into()),
                RtValue::Sexpr(param_list),
                RtValue::Sexpr(body_sexpr),
            ],
        )?;
        Ok(())
    }

    /// `(compile fn-name)` (or `(compile type::method)`): JIT-compiles a
    /// previously-defined `defun`/`defmethod` and registers the result in
    /// [`Self::compiled`]/[`Self::compiled_methods`] (see [`Self::method_key`])
    /// so `Expr::Call`/`Expr::Assoc` dispatches to native code instead of
    /// tree-walking it from then on. See [`Self::add_compiled_function`] for
    /// the supported-shape scope.
    ///
    /// labels/closures Stage 3: unlike `compile::aot::compile_file` (one
    /// shared module built up over every `defun` in file order, so a callee
    /// is always already fully defined in that same module by the time its
    /// caller is compiled — see that module's doc comment), every `compile`
    /// call gets its own throwaway module/engine (this method's
    /// long-standing design, unchanged). So a *different* top-level function
    /// this body's `Expr::Call`s reach
    /// (`crate::compile::ast_bridge::collect_call_targets`) has to be
    /// handled by hand, in three steps: (1) it must already be `compile`d
    /// (checked against [`Self::compiled`] up front, so a missing one
    /// surfaces as a clear `Panic` here rather than a confusing one from
    /// deep inside the compiler body's `get-function`); (2) forward-declared
    /// — no body — in this throwaway module *before* the compiler body runs
    /// (`compile-call`'s `get-function` needs to find *something* by that
    /// name); (3) wired to the real, already-running JIT code's address via
    /// `add_global_mapping` *after* (`crate::compile::CompiledFn::new`'s
    /// `externals` parameter) — can't happen any earlier, since the engine
    /// that will actually run this function's code doesn't exist until then.
    /// Self-recursion needs none of this: `compile-function`'s own first
    /// step (`add-function`) already declares this very function in its own
    /// module before compiling its body, so it's excluded from every step
    /// above.
    ///
    /// A user-defined method this body calls (`Expr::Assoc`,
    /// `crate::compile::ast_bridge::collect_assoc_targets`) goes through the
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
    /// registered in `self.methods` (`i64`/`i32`'s own built-in arithmetic;
    /// `string`'s, since Stage 7 of the Sexpr-representation plan —
    /// `docs/implementation-log.md`) needs none of this: those compile
    /// natively with no external call (`compile-assoc`'s own dispatch, which
    /// panics clearly on its own for any one of *their* methods it doesn't
    /// actually implement, e.g. `string::upcase`). But a *user-defined*
    /// method on a primitive receiver (e.g. the prelude's `impl Eq i32` →
    /// `i32::equals`) is in `self.methods` like any `defstruct` method and
    /// takes the normal three steps — `compile-assoc`'s dispatch falls
    /// through to the same mangled-name call for it. Anything else
    /// (`f64`/`char` builtins — still out of scope) panics clearly right
    /// here rather than deep inside `compile-assoc`'s own `get-function`.
    fn compile_function(&self, heap: &mut Heap, name: &str) -> Result<RtValue, EvalError> {
        let mut in_progress = std::collections::HashSet::new();
        self.compile_function_rec(heap, name, &mut in_progress)
    }

    /// The transitive worker behind [`Self::compile_function`]: compiles
    /// `name`, first recursively compiling any concrete function/method
    /// *instantiation* its body calls that isn't compiled yet. This is what
    /// lets a caller `(compile sum-vec)` pull in the monomorphized
    /// `vector::iter <i64>` / `vector-iter::next <i64>` (and, transitively,
    /// `map`/`member`/... over a concrete element type) an `Iter` combinator
    /// bottoms out in — those specializations have real bodies in
    /// [`Self::fns`]/[`Self::methods`] (`Checker::specialize_defun`) but
    /// whitespace-mangled names (`"iter <i64>"`) that `(compile ...)` can't
    /// name directly. `in_progress` guards against a compile-time cycle
    /// (mutual recursion across *separate* compiled functions, which each
    /// throwaway module can't forward-declare the way self-recursion is —
    /// reported as a clear error rather than an infinite descent); a value
    /// already in [`Self::compiled`]/[`Self::compiled_methods`] (a diamond in
    /// the call graph — two callers sharing one callee) short-circuits.
    fn compile_function_rec(
        &self,
        heap: &mut Heap,
        name: &str,
        in_progress: &mut std::collections::HashSet<String>,
    ) -> Result<RtValue, EvalError> {
        let path = Path::root(name);
        let method_key = self.method_key(name);
        match &method_key {
            Some(key) if self.compiled_methods.borrow().contains_key(key) => return Ok(RtValue::Bool(true)),
            None if self.compiled.borrow().contains_key(&path) => return Ok(RtValue::Bool(true)),
            _ => {}
        }
        in_progress.insert(name.to_string());
        let (_, body) = self.compiled_fn_body(name)?;
        let call_targets: Vec<Path> = crate::compile::ast_bridge::collect_call_targets(&body)
            .into_iter()
            .filter(|p| *p != path && !is_rt_builtin_name(p.local()))
            .collect();
        for target in &call_targets {
            if !self.compiled.borrow().contains_key(target) {
                let tname = target.local().to_string();
                if in_progress.contains(&tname) {
                    return Err(EvalError::Panic(format!(
                        "compile: mutual recursion between \"{}\" and \"{}\" across separate compiled functions is not supported",
                        name, tname
                    )));
                }
                self.compile_function_rec(heap, &tname, in_progress)?;
            }
        }

        let method_targets: Vec<(Path, String)> = crate::compile::ast_bridge::collect_assoc_targets(&body)
            .into_iter()
            .filter(|key| method_key.as_ref() != Some(key))
            .filter(|key| {
                // `Vector<T>`'s field-backed builtin methods (`new`/`get`/
                // `set`/`len`/`push`) are lowered to a `vector-op` node
                // (`ast_bridge::translate_vector_method` -> `rt_struct_*`),
                // not a method call, so — like the native primitive methods
                // below — they are never a real call target. `vector::iter`
                // is deliberately excluded from this list: it is a genuine
                // prelude `defmethod` (`vector-iter::new`) and must be
                // `compile`d like any other method.
                if key.0.local() == "vector" && matches!(key.1.as_str(), "new" | "get" | "set" | "len" | "push") {
                    return false;
                }
                // `HashTable<K,V>`'s builtin methods lowered to a `hashtable-op`
                // node (`ast_bridge::translate_hashtable_method`) are likewise
                // never a real call target. `iter` (a real `defmethod`) is
                // deliberately absent so it's validated/transitively compiled
                // normally.
                if key.0.local() == "hashtable"
                    && matches!(key.1.as_str(), "new" | "set" | "get" | "remove" | "count" | "clear" | "keys" | "values" | "entries")
                {
                    return false;
                }
                // A user-registered method is a real call target even on a
                // primitive receiver (`i32::equals`); only the natively
                // lowered `i64`/`i32`/`char`/`string`/`f64`/`bignum`/`ratio`
                // builtins (`+`, `<`, `=`, `lt`, `length`, `fadd`,
                // `rt_bignum_add`, ...) are excluded — those become LLVM
                // instructions / `rt_str_*`/`rt_bignum_*`/`rt_ratio_*` calls
                // in `compile-assoc`, not function calls. (A builtin on these
                // receivers that `compile-assoc` does *not* lower natively —
                // `equalp` on `string` before it was added, `f64::sqrt`,
                // `f64::float->int`, ... — is also excluded here and panics
                // inside `compile-assoc-user`'s `get-function` instead, still
                // at compile time.)
                self.methods.contains_key(key)
                    || !matches!(key.0.local(), "i64" | "i32" | "char" | "string" | "f64" | "bignum" | "ratio")
            })
            .collect();
        for (type_name, method) in &method_targets {
            let key = (type_name.clone(), method.clone());
            if !self.methods.contains_key(&key) {
                return Err(EvalError::Panic(format!(
                    "compile: \"{}\" calls \"{}\", a builtin method with no compiled implementation",
                    name,
                    method_link_name(type_name, method)
                )));
            }
            if !self.compiled_methods.borrow().contains_key(&key) {
                // A user-defined method's own body is registered in
                // `self.methods` (a specialization included), so compile it
                // transitively — the caller can't name a whitespace-mangled
                // instantiation by hand. `type_name::method` is the exact
                // spelling `compiled_fn_body`/`method_key` resolve back.
                let mname = format!("{}::{}", type_name.local(), method);
                if in_progress.contains(&mname) {
                    return Err(EvalError::Panic(format!(
                        "compile: mutual recursion between \"{}\" and \"{}\" across separate compiled functions is not supported",
                        name, mname
                    )));
                }
                self.compile_function_rec(heap, &mname, in_progress)?;
            }
        }

        let module = {
            let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
            let module = Rc::new(RefCell::new(crate::compile::llvm_context().create_module("compiled")));
            for target in &call_targets {
                declare_external_function(&module, &crate::compile::ast_bridge::user_symbol_name(target.local()));
            }
            for (type_name, method) in &method_targets {
                declare_external_function(&module, &crate::compile::ast_bridge::user_method_symbol_name(type_name, method));
            }
            for (rt_name, _) in rt_extern_functions() {
                declare_external_function(&module, rt_name);
            }
            module
        };
        self.add_compiled_function(heap, module.clone(), name, &crate::compile::ast_bridge::user_symbol_name(name))?;

        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let mut externals: Vec<(String, usize)> = {
            let compiled = self.compiled.borrow();
            call_targets
                .iter()
                .map(|p| {
                    (
                        crate::compile::ast_bridge::user_symbol_name(p.local()),
                        compiled.get(p).expect("checked compiled above").address(),
                    )
                })
                .collect()
        };
        {
            let compiled_methods = self.compiled_methods.borrow();
            externals.extend(method_targets.iter().map(|(type_name, method)| {
                let key = (type_name.clone(), method.clone());
                (
                    crate::compile::ast_bridge::user_method_symbol_name(type_name, method),
                    compiled_methods.get(&key).expect("checked compiled_methods above").address(),
                )
            }));
        }
        externals.extend(rt_extern_functions().iter().map(|(n, addr)| (n.to_string(), *addr)));
        // Mirrors `compile::aot::compile_file`'s own `verify()` call in the
        // same position, before handing the module to LLVM for real: a
        // typelisp-hosted `compiler.rs` bug that emits
        // instructions after a block's terminator (the `compile-let`
        // GC-root-leak fix's own doc comment names this exact risk) would
        // otherwise reach `CompiledFn::new`'s `create_jit_execution_engine`
        // as malformed IR — undefined behavior in LLVM itself, not a
        // catchable Rust error. Verifying first turns that into a clean
        // `Panic` instead.
        module.borrow().verify().map_err(|e| EvalError::Panic(format!("compile: module failed verification: {}", e)))?;
        let compiled = crate::compile::CompiledFn::new(&module.borrow(), &crate::compile::ast_bridge::user_symbol_name(name), &externals)
            .map_err(|e| EvalError::Panic(format!("compile: JIT failed: {}", e)))?;
        match method_key {
            Some(key) => {
                self.compiled_methods.borrow_mut().insert(key, compiled);
            }
            None => {
                self.compiled.borrow_mut().insert(path, compiled);
            }
        }
        Ok(RtValue::Bool(true))
    }

    /// Build the argument vector for a macro call: the first `fixed` raw
    /// forms map 1:1 to `RtValue::Sexpr`; if `f.rest`, every remaining raw
    /// form is collected into a single heap-allocated `Sexpr` list (built
    /// back-to-front, like `Self::alloc_quoted`'s `Cons` case) bound to the
    /// last parameter. Each element is already rooted by the caller (it's in
    /// `raw_args`, individually pushed in `Self::expand_macro`); only the
    /// growing `list` accumulator needs protecting around each `cons` call.
    fn bind_macro_args(
        &self,
        heap: &mut Heap,
        f: &FnDef,
        raw_args: &[Value],
        fixed: usize,
    ) -> Result<Vec<RtValue>, EvalError> {
        let mut argv: Vec<RtValue> = raw_args[..fixed].iter().map(|v| RtValue::Sexpr(*v)).collect();
        if f.rest {
            let mut list = Value::Empty;
            for v in raw_args[fixed..].iter().rev() {
                heap.push_root(list);
                let consed = heap.cons(*v, list);
                heap.pop_root();
                list = consed.map_err(|e| EvalError::Panic(e.to_string()))?;
            }
            argv.push(RtValue::Sexpr(list));
        }
        Ok(argv)
    }

    /// Evaluate each argument in turn, returning the values alongside the
    /// slots they were registered in. The caller must keep the returned
    /// `Vec<Slot>` alive (even if unused) for as long as it still needs the
    /// values protected from a GC — e.g. across a subsequent allocation built
    /// from them, such as `cons`.
    fn eval_args(&self, heap: &mut Heap, args: &[Typed], env: &Env) -> Result<(Vec<RtValue>, Vec<Slot>), EvalError> {
        let mut vs = Vec::with_capacity(args.len());
        let mut slots = Vec::with_capacity(args.len());
        for a in args {
            let v = self.eval(heap, a, env)?;
            // Not a program-visible binding — a pure GC-protection anchor,
            // so `Native` unconditionally (`collect_sexpr_roots` covers it).
            slots.push(self.native_slot(v.clone()));
            vs.push(v);
        }
        Ok((vs, slots))
    }

    /// Evaluate a body sequence, returning the last value (`Unit` if empty).
    fn eval_seq(&self, heap: &mut Heap, body: &[Typed], env: &Env) -> Result<RtValue, EvalError> {
        let mut result = RtValue::Unit;
        for e in body {
            result = self.eval(heap, e, env)?;
        }
        Ok(result)
    }

    /// Evaluate a built-in operator. Returns `None` if `name` is not a
    /// builtin, so the caller can fall through to a "no such function" error.
    /// (Arithmetic/comparison operators are *instance* methods, not free
    /// functions — see `eval_builtin_method` — so they don't appear here.
    /// `cons`/`car`/`cdr` operate on `Sexpr`; `car`/`cdr` of a non-`Cons`
    /// `Sexpr` — including `Nil` — panics. `gensym` returns a fresh
    /// `Sexpr::Sym` each call. `random` has no natural receiver to dispatch
    /// on, so it stays a free function too.)
    fn eval_builtin(&self, heap: &mut Heap, name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
        match name {
            "compile" => {
                let fn_name = match expect_str(&args[0]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                Some(self.compile_function(heap, &fn_name))
            }
            // `(compile-file "source.typl" "output")`: AOT-compiles an
            // independent source file straight to a native executable —
            // see `compile::aot::compile_file`'s doc comment for why this
            // runs against a *fresh* `Heap`/`Checker`/`Interp` rather than
            // the caller's (`self`'s), unlike `compile` above.
            "compile-file" => {
                let source_path = match expect_str(&args[0]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let output_path = match expect_str(&args[1]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                Some(
                    crate::compile::aot::compile_file(&source_path, &output_path)
                        .map(|()| RtValue::Bool(true))
                        .map_err(|e| EvalError::Panic(format!("compile-file: {}", e))),
                )
            }
            "random" => Some(eval_random(args)),
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
                // A leading space mirrors the hidden-binding idiom already
                // used for `dotimes`/`dolist`'s internal variables in the
                // checker (e.g. `" dotimes-limit"`): it can never collide
                // with a name a user actually types, since the reader's
                // symbol tokenizer can't produce a space mid-token.
                let n = self.gensym_counter.get();
                self.gensym_counter.set(n + 1);
                Some(Ok(RtValue::Sexpr(heap.intern_symbol(&format!(" gensym-{}", n)))))
            }
            // `symbol->string`/`string->symbol`: the `Symbol`<->`Str` bridges.
            // A `Symbol` value shares the `Value::Symbol(id)` carrier of a
            // `Sexpr::Sym` (`RtValue::Sexpr(Value::Symbol(id))`), so
            // `symbol->string` reads its interned name and `string->symbol`
            // interns a fresh one — the same intern table `gensym`/`read` use.
            "symbol->string" => Some(match rt_sexpr(&args[0]) {
                Ok(Value::Symbol(id)) => Ok(RtValue::Str(heap.symbol_name(id).into())),
                _ => Err(EvalError::Panic("symbol->string: not a symbol".into())),
            }),
            "string->symbol" => Some(match rt_str(args[0].clone()) {
                Ok(s) => Ok(RtValue::Sexpr(heap.intern_symbol(&s))),
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
                (Some(RtValue::Sexpr(a)), Some(RtValue::Sexpr(b))) => {
                    self.sync_roots(heap);
                    Some(heap.cons(*a, *b).map(RtValue::Sexpr).map_err(|e| EvalError::Panic(e.to_string())))
                }
                _ => Some(Err(EvalError::Internal("sexpr-cons: expected two Sexpr arguments".into()))),
            },
            "sexpr-car" => match args.first() {
                Some(RtValue::Sexpr(v)) => {
                    Some(heap.car(*v).map(RtValue::Sexpr).map_err(|_| EvalError::Panic("sexpr-car: not a cons".into())))
                }
                Some(_) => Some(Err(EvalError::Internal("sexpr-car: expected a Sexpr argument".into()))),
                None => Some(Err(EvalError::Internal("sexpr-car: expected one argument".into()))),
            },
            "sexpr-cdr" => match args.first() {
                Some(RtValue::Sexpr(v)) => {
                    Some(heap.cdr(*v).map(RtValue::Sexpr).map_err(|_| EvalError::Panic("sexpr-cdr: not a cons".into())))
                }
                Some(_) => Some(Err(EvalError::Internal("sexpr-cdr: expected a Sexpr argument".into()))),
                None => Some(Err(EvalError::Internal("sexpr-cdr: expected one argument".into()))),
            },
            "sexpr-consp" => match args.first() {
                Some(RtValue::Sexpr(v)) => Some(Ok(RtValue::Bool(v.is_cons()))),
                Some(_) => Some(Err(EvalError::Internal("sexpr-consp: expected a Sexpr argument".into()))),
                None => Some(Err(EvalError::Internal("sexpr-consp: expected one argument".into()))),
            },
            "sexpr-null" => match args.first() {
                Some(RtValue::Sexpr(v)) => Some(Ok(RtValue::Bool(v.is_empty()))),
                Some(_) => Some(Err(EvalError::Internal("sexpr-null: expected a Sexpr argument".into()))),
                None => Some(Err(EvalError::Internal("sexpr-null: expected one argument".into()))),
            },
            "sexpr-atom" => match args.first() {
                Some(RtValue::Sexpr(v)) => Some(Ok(RtValue::Bool(!v.is_cons()))),
                Some(_) => Some(Err(EvalError::Internal("sexpr-atom: expected a Sexpr argument".into()))),
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
                Some(RtValue::Sexpr(Value::Int(n))) => Some(Ok(RtValue::Int(*n))),
                Some(RtValue::Sexpr(_)) => Some(Err(EvalError::Panic("sexpr-int: expected an Int Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-int: expected a Sexpr argument".into()))),
            },
            "sexpr-bool" => match args.first() {
                Some(RtValue::Sexpr(Value::Bool(b))) => Some(Ok(RtValue::Bool(*b))),
                Some(RtValue::Sexpr(_)) => Some(Err(EvalError::Panic("sexpr-bool: expected a Bool Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-bool: expected a Sexpr argument".into()))),
            },
            // `sexpr-char`: peer of `sexpr-int`/`sexpr-bool` for a `Char` node
            // (`Value::Char`, an ordinary immediate, unlike `Float`'s boxed
            // payload) — added alongside `compile-char`/the `char` dispatch tag
            // (compiled code previously had no way to build/read a bare `char`
            // literal at all, an oversight discovered while implementing
            // `Expr::Quote`, whose `Char` leaf needs exactly this).
            "sexpr-char" => match args.first() {
                Some(RtValue::Sexpr(Value::Char(c))) => Some(Ok(RtValue::Char(*c))),
                Some(RtValue::Sexpr(_)) => Some(Err(EvalError::Panic("sexpr-char: expected a Char Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-char: expected a Sexpr argument".into()))),
            },
            // `sexpr-float`: peer of `sexpr-int` for a `Float` node (heap-boxed,
            // `Value::Boxed` — see `BoxedObj`). Added with the Phase 5 `match`
            // fence so a `Sexpr::Float` payload can still be read out without a
            // `(match s ((Float f) f) ..)`.
            "sexpr-float" => match args.first() {
                Some(RtValue::Sexpr(Value::Boxed(id))) if heap.is_float(*id) => Some(Ok(RtValue::Float(heap.float_value(*id)))),
                Some(RtValue::Sexpr(_)) => Some(Err(EvalError::Panic("sexpr-float: expected a Float Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-float: expected a Sexpr argument".into()))),
            },
            "sexpr-str" => match args.first() {
                Some(RtValue::Sexpr(Value::Str(id))) => Some(Ok(RtValue::Str(heap.string(*id).into()))),
                Some(RtValue::Sexpr(_)) => Some(Err(EvalError::Panic("sexpr-str: expected a Str Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-str: expected a Sexpr argument".into()))),
            },
            // `(Sym v)` binds `v : Symbol`, then `symbol->string` reads its name;
            // this fuses the two, matching the old defun `(symbol->string v)`.
            "sexpr-sym-name" => match args.first() {
                Some(RtValue::Sexpr(Value::Symbol(id))) => Some(Ok(RtValue::Str(heap.symbol_name(*id).into()))),
                Some(RtValue::Sexpr(_)) => Some(Err(EvalError::Panic("sexpr-sym-name: expected a Sym Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-sym-name: expected a Sexpr argument".into()))),
            },
            // `sexpr-symp`: the tag predicate a `match (car x) ((Sym s) ...) (_ ...))`
            // with a *non-panic* fallback rewrites to (`form-is-borrowed?` in
            // `compiler.rs`) — a peer of `sexpr-consp`/`sexpr-null`/`sexpr-atom`,
            // reading the tag directly.
            "sexpr-symp" => match args.first() {
                Some(RtValue::Sexpr(v)) => Some(Ok(RtValue::Bool(matches!(v, Value::Symbol(_))))),
                Some(_) => Some(Err(EvalError::Internal("sexpr-symp: expected a Sexpr argument".into()))),
                None => Some(Err(EvalError::Internal("sexpr-symp: expected one argument".into()))),
            },
            _ => None,
        }
    }
}

impl MacroExpander for Interp {
    /// Expand one macro call: look `path` up in `fns` (a `defmacro` is stored
    /// there exactly like a `defun` — see [`Interp::exec`]'s `Defmacro` arm),
    /// wrap each raw (unevaluated) argument form as `RtValue::Sexpr` with no
    /// conversion (this *is* the implicit quoting that makes macro arguments
    /// unevaluated data), and run it like any other call.
    ///
    /// GC-root discipline: `apply` registers its arguments into `self.slots`
    /// and may call `sync_roots` any number of times while evaluating the
    /// macro body — each such call pushes fresh roots *without* popping them
    /// at the end (by design; see `sync_roots`'s doc comment), so some number
    /// of roots `apply` itself doesn't own may be sitting on top of the heap's
    /// root stack when it returns. This method also pushes its own roots
    /// (`raw_args`, via plain `push_root`, not `slot`) *underneath* whatever
    /// `apply` adds, to protect them across `apply`'s execution. So on the
    /// way out, the teardown order must be: pop exactly `self.rooted` entries
    /// first (deregistering `apply`'s own bookkeeping — accurate at this
    /// exact point, since nothing else touches the root stack during
    /// `apply`), *then* pop `raw_args.len()` entries (which are only now back
    /// at the top, the stack being strictly LIFO). Popping in any other order
    /// — or letting `apply`'s leftover roots survive uncounted — corrupts
    /// either this call's own protection or `sync_roots`' bookkeeping for the
    /// next caller (e.g. a later top-level form), since `sync_roots` always
    /// trusts its own `rooted` count to know how much to pop.
    fn expand_macro(&self, heap: &mut Heap, path: &Path, raw_args: Vec<Value>) -> Result<Value, String> {
        let f = self.fns.get(path).ok_or_else(|| format!("no such macro: {}", path))?;
        let fixed = if f.rest { f.params.len() - 1 } else { f.params.len() };
        if f.rest {
            if raw_args.len() < fixed {
                return Err(format!("expected at least {} argument(s), got {}", fixed, raw_args.len()));
            }
        } else if f.params.len() != raw_args.len() {
            return Err(format!(
                "expected {} argument(s), got {}",
                f.params.len(),
                raw_args.len()
            ));
        }
        for v in &raw_args {
            heap.push_root(*v);
        }
        let result = match self.bind_macro_args(heap, f, &raw_args, fixed) {
            Ok(argv) => self.apply(heap, f, argv),
            Err(e) => Err(e),
        };
        for _ in 0..self.rooted.replace(0) {
            heap.pop_root();
        }
        for _ in &raw_args {
            heap.pop_root();
        }
        match result {
            Ok(RtValue::Sexpr(v)) => Ok(v),
            Ok(_) => Err("did not expand to a Sexpr".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Default for Interp {
    fn default() -> Self {
        Interp::new()
    }
}

fn env_get<'a>(env: &'a Env, name: &str) -> Option<&'a Slot> {
    env.iter().rev().find(|(n, _)| n == name).map(|(_, s)| s)
}

/// Allocate a [`QuotedSexpr`] literal into the GC-managed cons heap, fresh on
/// every call (see [`Expr::Quote`] for why the literal is kept as an owned
/// tree rather than a live heap pointer). Every intermediate cons cell built
/// along the way is rooted via plain `push_root`/`pop_root` (not
/// `slot`/`sync_roots`) for exactly as long as it takes to link it into its
/// parent. A free function, not an `Interp` method — it never touches `self`,
/// only recurses on itself.
///
/// Deliberately does **not** call `sync_roots` itself (unlike
/// `construct_sexpr`, which only ever makes one `cons` call per invocation):
/// `sync_roots` pops exactly as many roots as *it* last pushed, assuming
/// nothing else touched the stack in between. This recursion pushes its own
/// ad-hoc roots (`cv`/`dv` below) between `cons` calls, so a `sync_roots`
/// call nested in here would pop those instead of its own bookkeeping —
/// corrupting both. The caller ([`Interp::eval`]'s `Expr::Quote` arm) calls
/// `sync_roots` exactly once, before any of this recursion starts; since no
/// slot is created or destroyed while building a literal, that one snapshot
/// stays valid (and undisturbed, since every push here is popped before
/// returning) for the whole recursive build.
fn alloc_quoted(heap: &mut Heap, qs: &QuotedSexpr) -> Result<Value, EvalError> {
    match qs {
        QuotedSexpr::Nil => Ok(Value::Empty),
        QuotedSexpr::Int(n) => Ok(Value::Int(*n)),
        QuotedSexpr::Float(f) => Ok(heap.alloc_float(*f)),
        QuotedSexpr::Bignum(n) => Ok(heap.alloc_bignum(n.clone())),
        QuotedSexpr::Ratio(r) => Ok(heap.alloc_ratio(r.clone())),
        QuotedSexpr::Char(c) => Ok(Value::Char(*c)),
        QuotedSexpr::Bool(b) => Ok(Value::Bool(*b)),
        QuotedSexpr::Sym(s) => Ok(heap.intern_symbol(s)),
        QuotedSexpr::Path(segs) => {
            let sym_ids = segs
                .iter()
                .map(|s| match heap.intern_symbol(s) {
                    Value::Symbol(id) => id,
                    _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
                })
                .collect::<Vec<_>>();
            Ok(heap.intern_path(&sym_ids))
        }
        QuotedSexpr::Str(s) => Ok(heap.alloc_string(s.clone())),
        QuotedSexpr::Cons(car, cdr) => {
            let cv = alloc_quoted(heap, car)?;
            heap.push_root(cv);
            let dv = match alloc_quoted(heap, cdr) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root();
                    return Err(e);
                }
            };
            heap.push_root(dv);
            let result = heap.cons(cv, dv).map_err(|e| EvalError::Panic(e.to_string()));
            heap.pop_root(); // dv
            heap.pop_root(); // cv
            result
        }
    }
}

/// Whether `type_name` is the built-in `Sexpr` type, whose values are
/// represented by [`RtValue::Sexpr`] (heap-backed) rather than
/// [`RtValue::Data`].
fn is_sexpr_type(type_name: &Path) -> bool {
    *type_name == Path::root("sexpr")
}

/// The `Type`-level counterpart of [`is_sexpr_type`] — whether a *declared*
/// (checker-resolved) type is the built-in `Sexpr`. The one bit
/// [`decode_field_typed`] needs: post-monomorphization every field/element
/// read site carries a concrete declared type (`Expr::FieldGet`'s own node
/// type, a builtin method's checked return type), so this fully decides the
/// decode — no value-shape guessing remains.
fn is_sexpr_ty(ty: &Type) -> bool {
    matches!(ty, Type::Named(p, _) if is_sexpr_type(p))
}

/// The `V` in an `Option<V>` return type — `HashTable<K,V>::get`/`remove`'s
/// checked return type (`hashtable_def`'s `option_of(tvar("v"))`) is always
/// `Option<V>`, never `V` directly, so the stored value's declared type for
/// [`decode_field_typed`] sits one layer down. Anything else here is a
/// checker/interpreter bug (the builtin's registered signature guarantees
/// the shape), hence `Internal` rather than a panic.
fn option_payload_ty(ret_ty: &Type) -> Result<Type, EvalError> {
    match ret_ty {
        Type::Named(p, args) if *p == Path::root("option") && args.len() == 1 => Ok(args[0].clone()),
        other => Err(EvalError::Internal(format!(
            "expected an Option<V> return type at a HashTable get/remove site, got {:?}",
            other
        ))),
    }
}

/// Recursively gather every `Sexpr` value reachable from `v` through
/// native-repr `Data` fields or `Scope` frames. (A closure needs no arm of
/// its own: since Stage 6b it *is* an `RtValue::Sexpr` closure box —
/// pushed by the plain `Sexpr` arm, with the GC tracing its heap-cell
/// captures from there — while its `Native` captures are each already
/// registered in [`Interp::slots`].) A boxed struct or enum
/// (`RtValue::Sexpr(Value::Boxed(_))`, since the `Sexpr`/`RtValue`
/// unification's Stage 2 and the enum-representation unification
/// respectively — `HashTable<K,V>` included, since Stage 5) needs no
/// separate arm here — the plain `Sexpr` one already pushes its
/// `Value::Boxed` root, and `Heap::gc`'s mark phase traces *into* a
/// `BoxedObj::Struct`/`Enum`'s own fields (or, for a `HashTable`, its
/// `StructPayload::Map` keys/values) from there (see
/// `Heap::push_boxed_nested`), the same way it already does for a `Cons`
/// cell's `car`/`cdr`. A *native-repr* `RtValue::Data`
/// (`build_enum_value`'s fallback for a field the heap cannot represent —
/// `Option<llvm-value>` and the like) is never itself heap-resident, but
/// its fields might each independently hold a `Sexpr` (e.g. an
/// `Option<Sexpr>` alongside a native one elsewhere in the same value), so
/// this still has to recurse into it, same as the pre-unification code did.
fn collect_sexpr_roots(v: &RtValue, out: &mut Vec<Value>) {
    match v {
        RtValue::Sexpr(val) => out.push(*val),
        RtValue::Data { fields, .. } => {
            for f in fields {
                collect_sexpr_roots(f, out);
            }
        }
        RtValue::Scope(scope) => scope.for_each_value(|f| collect_sexpr_roots(f, out)),
        _ => {}
    }
}

fn rt_i64(v: &RtValue) -> Result<i64, EvalError> {
    match v {
        RtValue::Int(n) => Ok(*n),
        _ => Err(EvalError::Internal("sexpr: expected an i64 field".into())),
    }
}

fn rt_f64(v: &RtValue) -> Result<f64, EvalError> {
    match v {
        RtValue::Float(n) => Ok(*n),
        _ => Err(EvalError::Internal("sexpr: expected an f64 field".into())),
    }
}

fn rt_char(v: &RtValue) -> Result<char, EvalError> {
    match v {
        RtValue::Char(c) => Ok(*c),
        _ => Err(EvalError::Internal("sexpr: expected a char field".into())),
    }
}

fn rt_bool(v: &RtValue) -> Result<bool, EvalError> {
    match v {
        RtValue::Bool(b) => Ok(*b),
        _ => Err(EvalError::Internal("sexpr: expected a bool field".into())),
    }
}

fn rt_str(v: RtValue) -> Result<String, EvalError> {
    match v {
        RtValue::Str(s) => Ok(s.to_string()),
        _ => Err(EvalError::Internal("sexpr: expected a str field".into())),
    }
}

fn rt_bignum(v: &RtValue) -> Result<Rc<BigInt>, EvalError> {
    match v {
        RtValue::Bignum(n) => Ok(n.clone()),
        _ => Err(EvalError::Internal("sexpr: expected a bignum field".into())),
    }
}

fn rt_ratio(v: &RtValue) -> Result<Rc<BigRational>, EvalError> {
    match v {
        RtValue::Ratio(r) => Ok(r.clone()),
        _ => Err(EvalError::Internal("sexpr: expected a ratio field".into())),
    }
}

fn rt_sexpr(v: &RtValue) -> Result<Value, EvalError> {
    match v {
        RtValue::Sexpr(val) => Ok(*val),
        _ => Err(EvalError::Internal("sexpr: expected a Sexpr field".into())),
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

/// Evaluate a built-in `i32`/`i64` arithmetic/comparison instance method
/// (`registry::int_assoc`) — shared by both widths since `RtValue::Int`
/// represents every integer type uniformly as `i64`.
fn eval_int_builtin(name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(RtValue::Int(a)), Some(RtValue::Int(b))) => (*a, *b),
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two integers", name)))),
    };
    let v = match name {
        "+" => RtValue::Int(a + b),
        "-" => RtValue::Int(a - b),
        "*" => RtValue::Int(a * b),
        "/" => {
            if b == 0 {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            RtValue::Int(a / b)
        }
        "mod" => {
            if b == 0 {
                return Some(Err(EvalError::Panic("mod by zero".into())));
            }
            RtValue::Int(a % b)
        }
        "<" => RtValue::Bool(a < b),
        "<=" => RtValue::Bool(a <= b),
        ">" => RtValue::Bool(a > b),
        ">=" => RtValue::Bool(a >= b),
        "=" => RtValue::Bool(a == b),
        "/=" => RtValue::Bool(a != b),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

fn expect_float(v: &RtValue) -> Result<f64, EvalError> {
    match v {
        RtValue::Float(f) => Ok(*f),
        other => Err(EvalError::Internal(format!("expected a Float, got {:?}", other))),
    }
}

/// `int->float` (`registry::int_assoc`): widen an `i32`/`i64` to `f64`. Both
/// widths share `RtValue::Int(i64)` at runtime (see `eval_int_builtin`'s doc
/// comment), so one implementation covers both.
fn int_to_float(args: &[RtValue]) -> Result<RtValue, EvalError> {
    match args.first() {
        Some(RtValue::Int(n)) => Ok(RtValue::Float(*n as f64)),
        other => Err(EvalError::Internal(format!("int->float: expected an integer, got {:?}", other))),
    }
}

/// `float->int` (`registry::float_assoc`): narrow an `f64` to an integer,
/// truncating toward zero (Rust's `as i64`, same rounding direction as CL's
/// `truncate`).
fn float_to_int(args: &[RtValue]) -> Result<RtValue, EvalError> {
    match args.first() {
        Some(RtValue::Float(f)) => Ok(RtValue::Int(*f as i64)),
        other => Err(EvalError::Internal(format!("float->int: expected a float, got {:?}", other))),
    }
}

/// `int->char` (`registry::int_assoc`): a Unicode scalar value back to
/// `char`. Panics (same precedent as `car`/`cdr` on a non-`Cons` `Sexpr`) if
/// the value is outside the valid range — a surrogate code point or past
/// `U+10FFFF` — since the type system can't express "valid scalar value".
fn int_to_char(args: &[RtValue]) -> Result<RtValue, EvalError> {
    match args.first() {
        Some(RtValue::Int(n)) => {
            let in_u32_range = *n >= 0 && *n <= i64::from(u32::MAX);
            in_u32_range
                .then_some(*n as u32)
                .and_then(char::from_u32)
                .map(RtValue::Char)
                .ok_or_else(|| EvalError::Panic(format!("int->char: {} is not a valid Unicode scalar value", n)))
        }
        other => Err(EvalError::Internal(format!("int->char: expected an integer, got {:?}", other))),
    }
}

/// `try-int->char` (`registry::int_assoc`): the `Option`-returning
/// counterpart of [`int_to_char`], for `Checker::check_as`'s `try-as` —
/// same Unicode-scalar-value validity check, `None` instead of a panic.
fn try_int_to_char(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    match args.first() {
        Some(RtValue::Int(n)) => {
            let in_u32_range = *n >= 0 && *n <= i64::from(u32::MAX);
            let c = in_u32_range.then_some(*n as u32).and_then(char::from_u32);
            Ok(option_value(heap, c.map(RtValue::Char)))
        }
        other => Err(EvalError::Internal(format!("try-int->char: expected an integer, got {:?}", other))),
    }
}

/// Evaluate a built-in `f64` arithmetic/comparison instance method
/// (`registry::float_assoc`). Unlike [`eval_int_builtin`], `/`/`mod` never
/// panic on a zero divisor — IEEE-754 division yields `inf`/`NaN` instead,
/// the natural float semantics (no "can't express nonzero" gap to plug).
fn eval_float_builtin(name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(RtValue::Float(a)), Some(RtValue::Float(b))) => (*a, *b),
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two floats", name)))),
    };
    let v = match name {
        "+" => RtValue::Float(a + b),
        "-" => RtValue::Float(a - b),
        "*" => RtValue::Float(a * b),
        "/" => RtValue::Float(a / b),
        "mod" => RtValue::Float(a % b),
        "<" => RtValue::Bool(a < b),
        "<=" => RtValue::Bool(a <= b),
        ">" => RtValue::Bool(a > b),
        ">=" => RtValue::Bool(a >= b),
        "=" => RtValue::Bool(a == b),
        "/=" => RtValue::Bool(a != b),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

fn float_unary(args: &[RtValue], f: fn(f64) -> f64) -> Result<RtValue, EvalError> {
    Ok(RtValue::Float(f(expect_float(&args[0])?)))
}

fn float_expt(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Float(expect_float(&args[0])?.powf(expect_float(&args[1])?)))
}

fn expect_bignum(v: &RtValue) -> Result<Rc<BigInt>, EvalError> {
    match v {
        RtValue::Bignum(n) => Ok(n.clone()),
        other => Err(EvalError::Internal(format!("expected a bignum, got {:?}", other))),
    }
}

fn expect_ratio(v: &RtValue) -> Result<Rc<BigRational>, EvalError> {
    match v {
        RtValue::Ratio(r) => Ok(r.clone()),
        other => Err(EvalError::Internal(format!("expected a ratio, got {:?}", other))),
    }
}

/// Evaluate a built-in `bignum` arithmetic/comparison instance method
/// (`registry::bignum_assoc`). Same operation set/panic policy as
/// [`eval_int_builtin`]: `/`/`mod` truncate toward zero (`BigInt`'s `Div`/
/// `Rem` impls already do, matching Rust's `i64` semantics) and panic on a
/// zero divisor — the type system can't express "nonzero", the same
/// precedent as `car`/`cdr` on a non-`Cons` `Sexpr`.
fn eval_bignum_builtin(name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => match (expect_bignum(a), expect_bignum(b)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Some(Err(e)),
        },
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two bignums", name)))),
    };
    let v = match name {
        "+" => RtValue::Bignum(Rc::new(&*a + &*b)),
        "-" => RtValue::Bignum(Rc::new(&*a - &*b)),
        "*" => RtValue::Bignum(Rc::new(&*a * &*b)),
        "/" => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            RtValue::Bignum(Rc::new(&*a / &*b))
        }
        "mod" => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("mod by zero".into())));
            }
            RtValue::Bignum(Rc::new(&*a % &*b))
        }
        "<" => RtValue::Bool(*a < *b),
        "<=" => RtValue::Bool(*a <= *b),
        ">" => RtValue::Bool(*a > *b),
        ">=" => RtValue::Bool(*a >= *b),
        "=" => RtValue::Bool(*a == *b),
        "/=" => RtValue::Bool(*a != *b),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

/// Evaluate a built-in `ratio` arithmetic/comparison instance method
/// (`registry::ratio_assoc`). No `mod` — CL doesn't define a rational
/// remainder either. `/` panics on a zero divisor, same precedent as every
/// other numeric type here.
fn eval_ratio_builtin(name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => match (expect_ratio(a), expect_ratio(b)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Some(Err(e)),
        },
        _ => return Some(Err(EvalError::Internal(format!("{}: expected two ratios", name)))),
    };
    let v = match name {
        "+" => RtValue::Ratio(Rc::new(&*a + &*b)),
        "-" => RtValue::Ratio(Rc::new(&*a - &*b)),
        "*" => RtValue::Ratio(Rc::new(&*a * &*b)),
        "/" => {
            if b.is_zero() {
                return Some(Err(EvalError::Panic("divide by zero".into())));
            }
            RtValue::Ratio(Rc::new(&*a / &*b))
        }
        "<" => RtValue::Bool(*a < *b),
        "<=" => RtValue::Bool(*a <= *b),
        ">" => RtValue::Bool(*a > *b),
        ">=" => RtValue::Bool(*a >= *b),
        "=" => RtValue::Bool(*a == *b),
        "/=" => RtValue::Bool(*a != *b),
        _ => unreachable!(),
    };
    Some(Ok(v))
}

/// `int->bignum` (`registry::int_assoc`): always-exact widening.
fn int_to_bignum(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bignum(Rc::new(BigInt::from(rt_i64(&args[0])?))))
}

/// `int->ratio` (`registry::int_assoc`): always-exact widening.
fn int_to_ratio(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Ratio(Rc::new(BigRational::from_integer(BigInt::from(rt_i64(&args[0])?)))))
}

/// `bignum->int` (`registry::bignum_assoc`): narrowing, panics if the value
/// doesn't fit in an `i64` — the type system can't express "in range", same
/// precedent as `int->char`'s Unicode-scalar-value check.
fn bignum_to_int(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let n = expect_bignum(&args[0])?;
    n.to_i64()
        .map(RtValue::Int)
        .ok_or_else(|| EvalError::Panic(format!("bignum->int: {} does not fit in an i64", n)))
}

/// `try-bignum->int` (`registry::bignum_assoc`): the `Option`-returning
/// counterpart of [`bignum_to_int`], for `Checker::check_as`'s `try-as` —
/// same "fits in an `i64`" check, `None` instead of a panic on overflow.
fn try_bignum_to_int(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let n = expect_bignum(&args[0])?;
    let int = n.to_i64().map(RtValue::Int);
    Ok(option_value(heap, int))
}

/// `bignum->float` (`registry::bignum_assoc`): widening, possibly lossy for
/// a magnitude beyond `f64`'s 53-bit mantissa (IEEE-754 rounds to the
/// nearest representable value, same as any other narrowing-precision
/// numeric conversion).
fn bignum_to_float(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let n = expect_bignum(&args[0])?;
    Ok(RtValue::Float(n.to_f64().unwrap_or(f64::INFINITY.copysign(if n.sign() == num_bigint::Sign::Minus { -1.0 } else { 1.0 }))))
}

/// `bignum->ratio` (`registry::bignum_assoc`): always-exact widening.
fn bignum_to_ratio(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let n = expect_bignum(&args[0])?;
    Ok(RtValue::Ratio(Rc::new(BigRational::from_integer((*n).clone()))))
}

/// `float->bignum` (`registry::float_assoc`): narrowing, truncating toward
/// zero (`f64 as i64`'s multi-precision analogue). Panics on a non-finite
/// float (`NaN`/`inf`) — there is no bignum value to truncate to, the same
/// "value outside the representable range" panic precedent as
/// `int->char`/`bignum->int`.
fn float_to_bignum(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let f = expect_float(&args[0])?;
    if !f.is_finite() {
        return Err(EvalError::Panic(format!("float->bignum: {} is not finite", f)));
    }
    Ok(RtValue::Bignum(Rc::new(BigInt::from_f64(f.trunc()).expect("a finite float truncates to a representable BigInt"))))
}

/// `float->ratio` (`registry::float_assoc`): widening and *exact* — every
/// finite `f64` is itself an exact dyadic rational (CL's `rational`, not the
/// lossy-round-trip-through-decimal `rationalize`). Panics on a non-finite
/// float, same precedent as [`float_to_bignum`].
fn float_to_ratio(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let f = expect_float(&args[0])?;
    BigRational::from_float(f)
        .map(|r| RtValue::Ratio(Rc::new(r)))
        .ok_or_else(|| EvalError::Panic(format!("float->ratio: {} is not finite", f)))
}

/// `ratio->bignum` (`registry::ratio_assoc`): narrowing, truncating toward
/// zero (CL's `truncate`) — `Ratio::to_integer` already does exactly this.
fn ratio_to_bignum(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let r = expect_ratio(&args[0])?;
    Ok(RtValue::Bignum(Rc::new(r.to_integer())))
}

/// `ratio->float` (`registry::ratio_assoc`): widening, possibly lossy
/// (IEEE-754 rounds to the nearest representable `f64`).
fn ratio_to_float(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let r = expect_ratio(&args[0])?;
    Ok(RtValue::Float(r.to_f64().unwrap_or(f64::NAN)))
}

/// `numerator`/`denominator` (`registry::ratio_assoc`): the reduced
/// components of a `ratio` — CL's own accessors of the same names — as
/// `bignum`. The denominator of a normalized `ratio` is always positive (see
/// `BoxedObj::Ratio`'s doc comment), matching CL's guarantee.
fn ratio_numerator(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bignum(Rc::new(expect_ratio(&args[0])?.numer().clone())))
}

fn ratio_denominator(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bignum(Rc::new(expect_ratio(&args[0])?.denom().clone())))
}

/// A small global xorshift64* generator backing `random`. Not
/// cryptographically secure and not reseedable from typelisp — sufficient
/// for an MVP `(random n)`, matching `gensym`'s "collision-resistant, not
/// unforgeable" precedent for what a builtin without a real entropy/hygiene
/// API can promise. Lazily seeded from the system clock on first use.
/// Process-global (shared by every `Interp` instance and thread, e.g.
/// parallel `cargo test` threads) rather than per-`Interp` — `Relaxed`
/// atomics keep concurrent access memory-safe, at the cost of two threads
/// occasionally racing to the same draw (no correctness issue for an MVP
/// PRNG with no uniqueness guarantee to begin with).
static RNG_STATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_random_u64() -> u64 {
    use std::sync::atomic::Ordering;
    let mut x = RNG_STATE.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
            | 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    RNG_STATE.store(x, Ordering::Relaxed);
    x
}

fn eval_random(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let n = rt_i64(&args[0])?;
    if n <= 0 {
        return Err(EvalError::Panic(format!("random: bound must be positive, got {}", n)));
    }
    Ok(RtValue::Int((next_random_u64() % n as u64) as i64))
}

/// Built-in (Rust-implemented) instance/static methods for nominal types that
/// have no `defmethod` body to run — currently `HashTable<K,V>`
/// (`crate::check::registry`'s `hashtable_def`).
/// Mirrors `Interp::eval_builtin` for free functions: `Expr::Assoc`'s eval arm
/// tries `Interp::methods` (user `defmethod`s) first, falling back to this.
/// Argument count/types are trusted (the checker already validated them
/// against the type's `AdtDef` signatures), so arms index `args` directly
/// rather than re-checking shape. Takes `heap` (unlike most of these arms
/// need) for `sexpr`'s `eql`, which must read a boxed `Sexpr::Float`'s
/// actual value (`Heap::float_value`) to tell it apart from `eq`'s identity
/// comparison — see `sexpr_eql`'s doc comment.
/// `ret_ty` is the call site's checked return type (`Expr::Assoc`/
/// `Expr::TraitCall`/`Expr::Apply`'s own node type) — [`vector_get`]
/// consumes it to decode a `Vector<Sexpr>` element by its static type (see
/// [`decode_field_typed`]), and `Scope::new` reads its `V` off it.
/// `recv_ty` is the receiver argument's checked type (`args[0]`'s
/// `Typed.ty` at the call site, `None` on a receiver-less static call) —
/// what `Scope<V>`'s instance methods dispatch their representation on;
/// see the `"scope"` arm. `interp` carries the `struct_types` that
/// classification reads ([`Interp::scope_is_heap`]).
fn eval_builtin_method(interp: &Interp, heap: &mut Heap, type_name: &Path, method: &str, recv_ty: Option<&Type>, args: &[RtValue], ret_ty: &Type) -> Option<Result<RtValue, EvalError>> {
    if *type_name == Path::root("hashtable") {
        return match method {
            "new" => Some(Ok(RtValue::Sexpr(heap.alloc_hashtable()))),
            "get" => Some(hashtable_get(heap, args, ret_ty)),
            "set" => Some(hashtable_set(heap, args)),
            "remove" => Some(hashtable_remove(heap, args, ret_ty)),
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
            "new" => Some(Ok(RtValue::Sexpr(heap.alloc_struct("vector".to_string(), Vec::new())))),
            "push" => Some(vector_push(heap, args)),
            "get" => Some(vector_get(heap, args, ret_ty)),
            "set" => Some(vector_set(heap, args)),
            "len" => Some(vector_len(heap, args)),
            _ => None,
        };
    }
    if *type_name == Path::root("scope") {
        // Two representations, dispatched on the *static* element type `V`
        // (never the receiver value's shape — [`Interp::scope_is_heap`]):
        // heap-repr `V` -> the `StructPayload::Frames` heap scope
        // (unification Stage 8), everything else (LLVM handles above all)
        // -> the Rust-native `RtValue::Scope`. `new` has no receiver, so it
        // reads `V` off its own checked return type `Scope<V>` instead.
        if method == "new" {
            return Some(
                interp
                    .scope_is_heap(ret_ty)
                    .map(|heap_repr| if heap_repr { RtValue::Sexpr(heap.alloc_scope()) } else { scope_new() }),
            );
        }
        let heap_repr = match recv_ty {
            Some(ty) => match interp.scope_is_heap(ty) {
                Ok(h) => h,
                Err(e) => return Some(Err(e)),
            },
            None => {
                return Some(Err(EvalError::Internal(format!(
                    "scope::{}: no receiver type at the call site",
                    method
                ))))
            }
        };
        return if heap_repr {
            match method {
                "clone-frames" => Some(scope_clone_frames_heap(heap, args)),
                "push-frame" => Some(scope_push_frame_heap(heap, args)),
                "pop-frame" => Some(scope_pop_frame_heap(heap, args)),
                "get" => Some(scope_get_heap(heap, args)),
                "set" => Some(scope_set_heap(heap, args)),
                _ => None,
            }
        } else {
            match method {
                "clone-frames" => Some(scope_clone_frames(args)),
                "push-frame" => Some(scope_push_frame(args)),
                "pop-frame" => Some(scope_pop_frame(args)),
                "get" => Some(scope_get(heap, args)),
                "set" => Some(scope_set(args)),
                _ => None,
            }
        };
    }
    if *type_name == Path::root("string") {
        return match method {
            "upcase" => Some(expect_str(&args[0]).map(|s| RtValue::Str(s.to_ascii_uppercase().into()))),
            "downcase" => Some(expect_str(&args[0]).map(|s| RtValue::Str(s.to_ascii_lowercase().into()))),
            "length" => Some(string_length(args)),
            "ref" => Some(string_ref(args)),
            "substring" => Some(string_substring(args)),
            "append" => Some(string_append(args)),
            "lt" => Some(string_lt(args)),
            "<" | "<=" | ">" | ">=" => Some(string_compare(method, args)),
            // `eq`/`eql`: true identity (`Rc::ptr_eq` — see `RtValue::Str`'s
            // doc comment). `equal`/`equalp`: content comparison, the
            // (case-sensitive/-insensitive) CL predicates a naive "string
            // equality" actually means — see `registry::string_assoc`'s doc
            // comment and `docs/cl-equivalence-catalog.md`'s eq/eql/equal/
            // equalp section.
            "eq" | "eql" => Some(string_identity_eq(args)),
            "equal" => Some(string_content_eq(args)),
            "equalp" => Some(string_content_eqp(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("char") {
        return match method {
            "upcase" => Some(expect_char(&args[0]).map(|c| RtValue::Char(c.to_ascii_uppercase()))),
            "downcase" => Some(expect_char(&args[0]).map(|c| RtValue::Char(c.to_ascii_lowercase()))),
            "lt" => Some(char_lt(args)),
            "<" | "<=" | ">" | ">=" => Some(char_compare(method, args)),
            "alphap" => Some(expect_char(&args[0]).map(|c| RtValue::Bool(c.is_ascii_alphabetic()))),
            "digitp" => Some(expect_char(&args[0]).map(|c| RtValue::Bool(c.is_ascii_digit()))),
            // `eq`/`eql`/`equal` all coincide (immediate scalar, and CL's
            // own `equal` on characters is defined to be `eql`); `equalp`
            // is case-insensitive (see `registry::char_assoc`'s doc comment).
            "eq" | "eql" | "equal" => Some(char_eq(args)),
            "equalp" => Some(char_eqp(args)),
            "char->int" => Some(char_to_int(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("i32") || *type_name == Path::root("i64") {
        return match method {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                eval_int_builtin(method, args)
            }
            // `eq`/`eql`/`equal`/`equalp` are all registered as aliases for
            // `=` (see `registry::int_assoc`'s doc comment for why every one
            // of these four is meaningful to register even though none can
            // diverge from `=` here).
            "eq" | "eql" | "equal" | "equalp" => eval_int_builtin("=", args),
            "int->float" => Some(int_to_float(args)),
            "int->char" => Some(int_to_char(args)),
            "try-int->char" => Some(try_int_to_char(heap, args)),
            "int->bignum" => Some(int_to_bignum(args)),
            "int->ratio" => Some(int_to_ratio(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("f64") {
        return match method {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                eval_float_builtin(method, args)
            }
            "eq" | "eql" | "equal" | "equalp" => eval_float_builtin("=", args),
            "expt" => Some(float_expt(args)),
            "sqrt" => Some(float_unary(args, f64::sqrt)),
            "floor" => Some(float_unary(args, f64::floor)),
            "ceiling" => Some(float_unary(args, f64::ceil)),
            "round" => Some(float_unary(args, f64::round)),
            "truncate" => Some(float_unary(args, f64::trunc)),
            "float->int" => Some(float_to_int(args)),
            "float->bignum" => Some(float_to_bignum(args)),
            "float->ratio" => Some(float_to_ratio(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("bignum") {
        return match method {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                eval_bignum_builtin(method, args)
            }
            "eq" | "eql" | "equal" | "equalp" => eval_bignum_builtin("=", args),
            "bignum->int" => Some(bignum_to_int(args)),
            "try-bignum->int" => Some(try_bignum_to_int(heap, args)),
            "bignum->float" => Some(bignum_to_float(args)),
            "bignum->ratio" => Some(bignum_to_ratio(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("ratio") {
        return match method {
            "+" | "-" | "*" | "/" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                eval_ratio_builtin(method, args)
            }
            "eq" | "eql" | "equal" | "equalp" => eval_ratio_builtin("=", args),
            "ratio->bignum" => Some(ratio_to_bignum(args)),
            "ratio->float" => Some(ratio_to_float(args)),
            "numerator" => Some(ratio_numerator(args)),
            "denominator" => Some(ratio_denominator(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("bool") {
        return match method {
            "eq" | "eql" | "equal" | "equalp" => Some(bool_eq(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("symbol") {
        // A `Symbol` value shares the `Value::Symbol(id)` carrier of a
        // `Sexpr::Sym`, so `eq`/`eql` reuse the `Sexpr` comparisons (interned
        // id identity — same name => same id => `eq`).
        return match method {
            "eq" => Some(sexpr_eq(args)),
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
            "eq" => Some(sexpr_eq(args)),
            "eql" => Some(sexpr_eql(heap, args)),
            _ => None,
        };
    }
    eval_llvm_builtin_method(type_name, method, args)
}

/// The (typelisp-hosted) compiler's view of LLVM — `llvm-module`/
/// `llvm-function`/`llvm-builder`/`llvm-value` instance and static methods.
/// Same metadata-only pattern as the rest of `eval_builtin_method` (the
/// `AdtDef`s in `registry::llvm_module_def` etc. carry no `defmethod` body).
/// Every arm holds [`crate::compile::COMPILE_LOCK`] for its duration — see
/// that constant's doc comment for why concurrent access to the one
/// process-wide LLVM `Context` must never happen.
fn eval_llvm_builtin_method(type_name: &Path, method: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    if *type_name == Path::root("llvm-module") {
        return match method {
            "create" => Some(llvm_module_create(args)),
            "add-function" => Some(llvm_module_add_function(args)),
            "verify" => Some(llvm_module_verify(args)),
            "to-string" => Some(llvm_module_to_string(args)),
            "get-function" => Some(llvm_module_get_function(args)),
            "add-function-with-env" => Some(llvm_module_add_function_with_env(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("llvm-function") {
        return match method {
            "append-block" => Some(llvm_function_append_block(args)),
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
            "build-fptosi" => Some(llvm_builder_build_fptosi(args)),
            "alloca-args" => Some(llvm_builder_alloca_args(args)),
            "store-arg" => Some(llvm_builder_store_arg(args)),
            "build-call" => Some(llvm_builder_build_call(args)),
            "load-env" => Some(llvm_builder_load_env(args)),
            "build-call-with-env" => Some(llvm_builder_build_call_with_env(args)),
            "build-make-closure" => Some(llvm_builder_build_make_closure(args)),
            "build-closure-apply" => Some(llvm_builder_build_closure_apply(args)),
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

fn expect_llvm_module(v: &RtValue) -> Result<&Rc<RefCell<Module<'static>>>, EvalError> {
    match v {
        RtValue::LlvmModule(m) => Ok(m),
        other => Err(EvalError::Internal(format!("expected an LlvmModule, got {:?}", other))),
    }
}

fn expect_llvm_function(v: &RtValue) -> Result<FunctionValue<'static>, EvalError> {
    match v {
        RtValue::LlvmFunction(f) => Ok(*f),
        other => Err(EvalError::Internal(format!("expected an LlvmFunction, got {:?}", other))),
    }
}

fn expect_llvm_builder(v: &RtValue) -> Result<&Rc<RefCell<Builder<'static>>>, EvalError> {
    match v {
        RtValue::LlvmBuilder(b) => Ok(b),
        other => Err(EvalError::Internal(format!("expected an LlvmBuilder, got {:?}", other))),
    }
}

fn expect_llvm_basic_block(v: &RtValue) -> Result<BasicBlock<'static>, EvalError> {
    match v {
        RtValue::LlvmBasicBlock(b) => Ok(*b),
        other => Err(EvalError::Internal(format!("expected an LlvmBasicBlock, got {:?}", other))),
    }
}

fn expect_llvm_value(v: &RtValue) -> Result<BasicValueEnum<'static>, EvalError> {
    match v {
        RtValue::LlvmValue(v) => Ok(*v),
        other => Err(EvalError::Internal(format!("expected an LlvmValue, got {:?}", other))),
    }
}

fn llvm_module_create(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let name = expect_str(&args[0])?;
    let module = crate::compile::llvm_context().create_module(name);
    Ok(RtValue::LlvmModule(Rc::new(RefCell::new(module))))
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

fn llvm_module_add_function(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(&args[1])?;
    let function = module.borrow_mut().add_function(name, compiled_fn_type(), None);
    Ok(RtValue::LlvmFunction(function))
}

/// Forward-declares `name` in `module` with the standard compiled-function
/// ABI but **no body** — [`Interp::compile_function`]'s JIT-only step
/// (labels/closures Stage 3, `Expr::Call` to a different top-level function)
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

/// The LLVM-visible name a method's own compiled function is declared/
/// looked-up under: `type_name`'s *local* segment, `"::"`, `method` —
/// exactly the literal string a standalone `(compile "type-name::method")`
/// call uses as its `internal_name` ([`Interp::compile_function`]'s own
/// `self.add_compiled_function(heap, module.clone(), name, name)` call,
/// where `name` is that literal user-typed string), so every caller of this
/// helper agrees with that name without re-deriving it. Shared by
/// [`Interp::compile_function`] (forward-declaring/wiring a callee method)
/// and `compiler.rs`'s `compile-assoc` (looking the same name back up via
/// `get-function` — see that function's doc comment).
fn method_link_name(type_name: &Path, method: &str) -> String {
    format!("{}::{}", type_name.local(), method)
}

/// True for the handful of builtins `compiler.rs`'s `compile-call` rewrites to
/// a `crate::compile::runtime` shim by name (`sexpr-car` -> `rt_car`, etc. —
/// see that function's `raw-nm`/`nm` rename) rather than requiring `(compile
/// ...)` first: these can never be `compile`d themselves (no typelisp AST body
/// — direct cons-heap access, Rust-only), so [`Interp::compile_function`]
/// excludes them from its normal "every call target must already be compiled"
/// check and instead always wires them via [`rt_extern_functions`]. These are
/// the `sexpr-*` island layer (Symbol/Sexpr redesign Phase 4b): the free
/// `car`/`cdr`/`cons` names are now the `cons<T,U>` pair (an ordinary
/// `defstruct` method / `defun`, compiled the normal way), not `rt_*` shims.
pub(crate) fn is_rt_builtin_name(name: &str) -> bool {
    matches!(name, "sexpr-car" | "sexpr-cdr" | "sexpr-cons")
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
pub(crate) fn rt_extern_functions() -> [(&'static str, usize); 80] {
    use crate::compile::runtime::{
        rt_bignum_add, rt_bignum_cmp, rt_bignum_div, rt_bignum_fits_i32, rt_bignum_mod, rt_bignum_mul, rt_bignum_new, rt_bignum_sub,
        rt_bignum_to_float, rt_bignum_to_int, rt_bignum_to_int_raw, rt_bignum_to_ratio, rt_box_kind, rt_car, rt_cdr, rt_cell_get,
        rt_cell_new, rt_cell_set, rt_char_equalp, rt_closure_env_get, rt_closure_env_len, rt_closure_fnptr, rt_closure_new,
        rt_cons, rt_data_field, rt_data_new, rt_data_variant, rt_float_new, rt_float_to_bignum, rt_float_to_ratio, rt_float_value,
        rt_global_get, rt_global_new, rt_global_set,
        rt_hashtable_clear, rt_hashtable_contains, rt_hashtable_count, rt_hashtable_entries, rt_hashtable_get_raw, rt_hashtable_keys,
        rt_hashtable_new, rt_hashtable_remove_raw, rt_hashtable_set, rt_hashtable_values, rt_int_to_bignum, rt_int_to_ratio,
        rt_intern_path, rt_intern_symbol, rt_match_fail, rt_panic, rt_pop_sexpr_root, rt_push_permanent_sexpr_root, rt_push_sexpr_root,
        rt_ratio_add, rt_ratio_cmp, rt_ratio_denominator, rt_ratio_div, rt_ratio_from_bignums, rt_ratio_mul, rt_ratio_numerator,
        rt_ratio_sub, rt_ratio_to_bignum, rt_ratio_to_float, rt_root_count, rt_set_car, rt_set_cdr, rt_set_sexpr_root, rt_str_append,
        rt_str_eq, rt_str_equalp, rt_str_length, rt_str_lt, rt_str_new, rt_str_ref, rt_struct_field_count, rt_struct_field_get,
        rt_struct_field_set, rt_struct_new, rt_struct_push_field, rt_truncate_sexpr_roots,
    };
    [
        ("rt_car", rt_car as usize),
        ("rt_cdr", rt_cdr as usize),
        ("rt_cons", rt_cons as usize),
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
        ("rt_closure_new", rt_closure_new as usize),
        ("rt_closure_fnptr", rt_closure_fnptr as usize),
        ("rt_closure_env_len", rt_closure_env_len as usize),
        ("rt_closure_env_get", rt_closure_env_get as usize),
        ("rt_cell_new", rt_cell_new as usize),
        ("rt_cell_get", rt_cell_get as usize),
        ("rt_cell_set", rt_cell_set as usize),
        ("rt_data_new", rt_data_new as usize),
        ("rt_data_variant", rt_data_variant as usize),
        ("rt_data_field", rt_data_field as usize),
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
    ]
}

/// The captures counterpart of [`compiled_fn_type`]: `i64 name(i64* args,
/// i32 argc, i64* env, i32 env_len)` — used for a `labels` sibling whenever
/// its block's shared captured-name list
/// (`compile::freevars::labels_free_vars`) is non-empty, and (labels/closures
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
fn llvm_module_add_function_with_env(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(&args[1])?;
    let function = module.borrow_mut().add_function(name, compiled_fn_type_with_env(), None);
    Ok(RtValue::LlvmFunction(function))
}

fn llvm_module_verify(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    Ok(RtValue::Bool(module.borrow().verify().is_ok()))
}

fn llvm_module_to_string(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    Ok(RtValue::Str(module.borrow().print_to_string().to_string().into()))
}

/// Looks up an already-`add-function`-declared `llvm-function` by name —
/// see `registry::llvm_module_def`'s doc comment on `get-function` for why
/// this is the core lookup every direct call (self-recursion, `labels`
/// siblings, top-level `defun`-to-`defun` calls) is built on.
fn llvm_module_get_function(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(&args[1])?;
    module
        .borrow()
        .get_function(name)
        .map(RtValue::LlvmFunction)
        .ok_or_else(|| EvalError::Panic(format!("get-function: no function named \"{}\" in this module", name)))
}

fn llvm_function_append_block(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let function = expect_llvm_function(&args[0])?;
    let name = expect_str(&args[1])?;
    let block = crate::compile::llvm_context().append_basic_block(function, name);
    Ok(RtValue::LlvmBasicBlock(block))
}

fn llvm_builder_create() -> Result<RtValue, EvalError> {
    let builder = crate::compile::llvm_context().create_builder();
    Ok(RtValue::LlvmBuilder(Rc::new(RefCell::new(builder))))
}

fn llvm_builder_position_at_end(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let block = expect_llvm_basic_block(&args[1])?;
    builder.borrow().position_at_end(block);
    Ok(RtValue::Unit)
}

fn llvm_builder_const_i64(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _builder = expect_llvm_builder(&args[0])?;
    let n = match &args[1] {
        RtValue::Int(n) => *n,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let value = crate::compile::llvm_context().i64_type().const_int(n as u64, false);
    Ok(RtValue::LlvmValue(value.into()))
}

fn llvm_builder_build_ret(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let value = expect_llvm_value(&args[1])?;
    builder
        .borrow()
        .build_return(Some(&value))
        .map_err(|e| EvalError::Internal(format!("build-ret: {}", e)))?;
    Ok(RtValue::Unit)
}

/// Reads logical parameter `index` out of `function`'s fixed-ABI argument
/// array (its sole real LLVM parameter — see `llvm_module_add_function`'s
/// doc comment) via a GEP + load. `i64` only for now, matching every other
/// `llvm-builder` arithmetic builtin.
fn llvm_builder_load_arg(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let function = expect_llvm_function(&args[1])?;
    let index = match &args[2] {
        RtValue::Int(n) => *n as u64,
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
    Ok(RtValue::LlvmValue(loaded))
}

/// Reads logical captured slot `index` out of `function`'s env array
/// (its 3rd real LLVM parameter, `get_nth_param(2)` — see
/// `llvm_module_add_function_with_env`'s doc comment) — the same GEP+load
/// pattern `load_arg` uses against the args array (parameter 0), just
/// against the env one instead.
fn llvm_builder_load_env(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let function = expect_llvm_function(&args[1])?;
    let index = match &args[2] {
        RtValue::Int(n) => *n as u64,
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
    Ok(RtValue::LlvmValue(loaded))
}

/// Shared by `build-add`/`build-sub`/`build-mul`: unwrap both `llvm-value`
/// operands to `IntValue`s, apply `op` (one of `Builder::build_int_add`/
/// `_sub`/`_mul`), and re-wrap the result.
fn llvm_builder_build_int_op(
    args: &[RtValue],
    name: &str,
    op: impl FnOnce(&Builder<'static>, inkwell::values::IntValue<'static>, inkwell::values::IntValue<'static>, &str) -> Result<inkwell::values::IntValue<'static>, inkwell::builder::BuilderError>,
) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a = expect_llvm_value(&args[1])?.into_int_value();
    let b = expect_llvm_value(&args[2])?.into_int_value();
    let result = op(&builder.borrow(), a, b, name).map_err(|e| EvalError::Internal(format!("build-{}: {}", name, e)))?;
    Ok(RtValue::LlvmValue(result.into()))
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
    args: &[RtValue],
    name: &str,
    op: impl FnOnce(&Builder<'static>, inkwell::values::FloatValue<'static>, inkwell::values::FloatValue<'static>, &str) -> Result<inkwell::values::FloatValue<'static>, inkwell::builder::BuilderError>,
) -> Result<RtValue, EvalError> {
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
    Ok(RtValue::LlvmValue(bits))
}

/// The `f64` comparison counterpart of [`llvm_builder_build_int_op`]/
/// [`llvm_builder_build_icmp`] combined: `bitcast` both `i64`-carried operands
/// to `double`, `fcmp` with `predicate`, then zero-extend the `i1` result to
/// the `i64` every compiled value is (`build-icmp`'s own widening step).
/// `<`/`<=`/`>`/`>=` use the *ordered* predicates (`OLT`/... — false if either
/// operand is NaN, matching Rust's `<`/... the interpreter's `eval_float_builtin`
/// uses); `=`/`eq`/`eql`/`equal`/`equalp` use `OEQ` (NaN never equals NaN) and
/// `/=` uses `UNE` (Rust's `!=` is `!(a == b)`, true when either is NaN).
fn llvm_builder_build_fcmp(args: &[RtValue], name: &str, predicate: inkwell::FloatPredicate) -> Result<RtValue, EvalError> {
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
    Ok(RtValue::LlvmValue(widened.into()))
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
fn llvm_builder_build_float_unary_intrinsic(args: &[RtValue], name: &str, intrinsic_name: &str) -> Result<RtValue, EvalError> {
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
    Ok(RtValue::LlvmValue(bits))
}

/// `expt` (`f64,f64->f64`): the binary counterpart of
/// [`llvm_builder_build_float_unary_intrinsic`], `llvm.pow.f64` — matches
/// the interpreter's `f64::powf` (`float_expt`), both ultimately the
/// platform libm `pow` either way.
fn llvm_builder_build_fpow(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let a_bits = expect_llvm_value(&args[2])?.into_int_value();
    let b_bits = expect_llvm_value(&args[3])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: String| EvalError::Internal(format!("build-fpow: {}", e));
    let a = bld.build_bit_cast(a_bits, f64_ty, "a_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let b = bld.build_bit_cast(b_bits, f64_ty, "b_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let intrinsic = inkwell::intrinsics::Intrinsic::find("llvm.pow.f64").ok_or_else(|| err("no such LLVM intrinsic llvm.pow.f64".into()))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[f64_ty.into()])
        .ok_or_else(|| err("failed to declare llvm.pow.f64".into()))?;
    let call = bld.build_call(decl, &[a.into(), b.into()], "fpow").map_err(|e| err(e.to_string()))?;
    let result = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_float_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err(err("llvm.pow.f64 produced no value".into())),
    };
    let bits = bld.build_bit_cast(result, ctx.i64_type(), "fpow_bits").map_err(|e| err(e.to_string()))?;
    Ok(RtValue::LlvmValue(bits))
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
fn llvm_builder_build_fptosi(args: &[RtValue]) -> Result<RtValue, EvalError> {
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
    Ok(RtValue::LlvmValue(result.into()))
}

/// Stack-allocates a `[count x i64]` array and returns its base pointer, to
/// be filled in by `store-arg` and passed to `build-call` — the compiled-IR
/// equivalent of building the `i64* args` array every compiled function's
/// fixed ABI expects (see `llvm_module_add_function`'s doc comment). Opaque
/// pointers (LLVM 17's default) carry no element-type info of their own, so
/// this pointer is usable as a flat `i64*` exactly the way `load_arg`'s own
/// `args_ptr` parameter already is — every GEP against it supplies
/// `ctx.i64_type()` itself, regardless of the alloca's nominal array type.
fn llvm_builder_alloca_args(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let count = match &args[1] {
        RtValue::Int(n) => *n as u32,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let array_ty = ctx.i64_type().array_type(count);
    let ptr = builder.borrow().build_alloca(array_ty, "call_args").map_err(|e| EvalError::Internal(format!("alloca-args: {}", e)))?;
    Ok(RtValue::LlvmValue(ptr.into()))
}

/// Writes `value` into slot `index` of an `alloca-args` array — the same
/// GEP pattern `load_arg` uses to *read* a logical argument, just paired
/// with a store instead of a load.
fn llvm_builder_store_arg(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = match &args[2] {
        RtValue::Int(n) => *n as u64,
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
    Ok(RtValue::Unit)
}

/// The generic-pointer read counterpart of [`llvm_builder_store_arg`] — same
/// GEP pattern as [`llvm_builder_load_arg`], but against an arbitrary
/// `array_ptr` rather than a function's own args parameter (automatic
/// retain/release insertion's `release-pending-args` uses this to read back
/// the parallel "which call/env-array slots need releasing" array
/// `compile-call-args`/`compile-env-args` built via `store-arg`).
fn llvm_builder_load_raw(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = match &args[2] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), array_ptr, &[idx_val], "load_raw_ptr").map_err(|e| EvalError::Internal(format!("load-raw: {}", e)))?
    };
    let loaded = b.build_load(ctx.i64_type(), elem_ptr, "load_raw_val").map_err(|e| EvalError::Internal(format!("load-raw: {}", e)))?;
    Ok(RtValue::LlvmValue(loaded))
}

/// Shared by every `build-icmp-*` builtin (if/let/comparisons, labels/closures
/// Stage 5): runs `icmp <predicate>` on two `i64` operands, then widens the
/// resulting `i1` back to `i64` (0/1) via `build_int_z_extend` — every other
/// builtin here treats a compiled value as a plain `i64` (see
/// `registry::llvm_module_def`'s doc comment), and a comparison result is no
/// exception, which is exactly what lets `compile-if`'s `build-cond-br` (and
/// ordinary arithmetic/storage) accept it without caring it came from a
/// comparison rather than `+`/a literal.
fn llvm_builder_build_icmp(args: &[RtValue], name: &str, predicate: inkwell::IntPredicate) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a = expect_llvm_value(&args[1])?.into_int_value();
    let b = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let cmp = bld.build_int_compare(predicate, a, b, name).map_err(|e| EvalError::Internal(format!("{}: {}", name, e)))?;
    let ctx = crate::compile::llvm_context();
    let widened = bld.build_int_z_extend(cmp, ctx.i64_type(), name).map_err(|e| EvalError::Internal(format!("{}: {}", name, e)))?;
    Ok(RtValue::LlvmValue(widened.into()))
}

/// `compile-if`'s branch primitive: branches to `then_block` when `cond`
/// (an ordinary `i64`-valued `llvm-value`) is nonzero, `else_block`
/// otherwise — built from an `icmp ne cond, 0` plus a conditional branch, the
/// same shape [`llvm_builder_build_closure_apply`] already uses internally
/// for its own env-loop bounds check.
fn llvm_builder_build_cond_br(args: &[RtValue]) -> Result<RtValue, EvalError> {
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
    Ok(RtValue::Unit)
}

/// An unconditional branch — `compile-if`'s then/else arms use this to join
/// back at the merge block after storing their value into the shared slot.
fn llvm_builder_build_br(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_basic_block(&args[1])?;
    builder.borrow().build_unconditional_branch(target).map_err(|e| EvalError::Internal(format!("build-br: {}", e)))?;
    Ok(RtValue::Unit)
}

/// See `registry::llvm_builder_def`'s doc comment for `block-terminated?`.
fn llvm_builder_block_terminated(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let terminated = builder.borrow().get_insert_block().and_then(|bb| bb.get_terminator()).is_some();
    Ok(RtValue::Bool(terminated))
}

/// A direct call to an already-declared `target` (typically `get-function`'s
/// result), passing `args_ptr`/`argc` straight through to its fixed ABI —
/// see `registry::llvm_module_def`'s doc comment for why every compiled
/// function shares that one signature regardless of arity. This is the one
/// new primitive that unlocks every statically-resolvable direct call:
/// self-recursion, `labels`-sibling calls, and top-level `defun`-to-`defun`
/// calls alike, since all three reduce to "the callee's `llvm-function`
/// already exists in this module, look it up and call it."
fn llvm_builder_build_call(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = match &args[3] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = builder
        .borrow()
        .build_call(target, &[args_ptr.into(), argc_val.into()], "call_result")
        .map_err(|e| EvalError::Internal(format!("build-call: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(RtValue::LlvmValue(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-call: callee produced no value".into())),
    }
}

/// The captures counterpart of [`llvm_builder_build_call`]: calls `target`
/// (declared via `add-function-with-env`) passing both the args array
/// (`args_ptr`/`argc`, exactly as `build-call` does) and an env array
/// (`env_ptr`/`env_len`) under its extended ABI.
fn llvm_builder_build_call_with_env(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = match &args[3] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let env_ptr = expect_llvm_value(&args[4])?;
    let env_len = match &args[5] {
        RtValue::Int(n) => *n as u64,
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
        inkwell::values::ValueKind::Basic(v) => Ok(RtValue::LlvmValue(v)),
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
fn llvm_builder_build_make_closure(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let target = expect_llvm_function(&args[2])?;
    let env_ptr = expect_llvm_value(&args[3])?.into_pointer_value();
    let env_len = match &args[4] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let sexpr_mask = match &args[5] {
        RtValue::Int(n) => *n as u64,
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
        inkwell::values::ValueKind::Basic(v) => Ok(RtValue::LlvmValue(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-make-closure: rt_closure_new produced no value".into())),
    }
}

/// `build-call-with-env`'s indirect counterpart: the callee isn't a
/// statically-known `llvm-function` here, only a tagged `Sexpr` closure
/// reference, so this reads `fn_ptr`/`env_len` back out via `rt_closure_fnptr`/
/// `rt_closure_env_len`, copies each captured slot into a fixed 64-slot
/// scratch buffer via a genuine runtime loop over `rt_closure_env_get`
/// (`env_len` is only known once the closure value actually exists, not at
/// IR-build time; 64 is `rt_closure_new`'s own capture-count ceiling — see
/// `BoxedObj::CompiledClosure`'s doc comment — so a fixed-capacity buffer
/// avoids a dynamic-sized `alloca`), then calls through
/// `Builder::build_indirect_call` against the one fixed
/// `compiled_fn_type_with_env` signature every closure-boxed function shares
/// (see that function's doc comment for why no ABI branch is needed here).
/// Never exposes `env`'s backing `Vec<Value>` as a raw pointer across an `rt_*`
/// call boundary (Stage 1's "don't hold an env pointer across an rt call"
/// convention — a GC triggered inside `rt_closure_env_get` could move/resize
/// that `Vec`), which is exactly why each slot is copied out one at a time
/// through the accessor rather than read directly.
fn llvm_builder_build_closure_apply(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let closure = expect_llvm_value(&args[2])?;
    let args_ptr = expect_llvm_value(&args[3])?;
    let argc = match &args[4] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let module = module.borrow();
    let err = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("build-closure-apply: {}", e));

    let function = b
        .get_insert_block()
        .and_then(|blk| blk.get_parent())
        .ok_or_else(|| EvalError::Internal("build-closure-apply: builder has no current function".into()))?;
    let rt_closure_fnptr = module
        .get_function("rt_closure_fnptr")
        .ok_or_else(|| EvalError::Internal("build-closure-apply: rt_closure_fnptr not declared in this module".into()))?;
    let rt_closure_env_len = module
        .get_function("rt_closure_env_len")
        .ok_or_else(|| EvalError::Internal("build-closure-apply: rt_closure_env_len not declared in this module".into()))?;
    let rt_closure_env_get = module
        .get_function("rt_closure_env_get")
        .ok_or_else(|| EvalError::Internal("build-closure-apply: rt_closure_env_get not declared in this module".into()))?;

    let i64_ty = ctx.i64_type();
    let closure_int = closure.into_int_value();

    // Every `rt_*` shim shares the one uniform `(args_ptr, argc) -> i64` ABI
    // (`compiled_fn_type`) — including these three — never raw scalar
    // parameters, so each call below builds its own small `alloca`'d
    // argument array first, exactly the way `compiler.rs`'s own
    // `alloca-args`/`store-arg`/`build-call` triple does for every other
    // `rt_*` call.
    let call_rt1 = |target: FunctionValue<'static>, a0: BasicValueEnum<'static>, name: &str| -> Result<inkwell::values::IntValue<'static>, EvalError> {
        let ap = b.build_alloca(i64_ty.array_type(1), "rt1_args").map_err(err)?;
        let p0 = unsafe { b.build_gep(i64_ty, ap, &[i64_ty.const_int(0, false)], "rt1_arg0_ptr").map_err(err)? };
        b.build_store(p0, a0).map_err(err)?;
        let argc = ctx.i32_type().const_int(1, false);
        let call = b.build_call(target, &[ap.into(), argc.into()], name).map_err(err)?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal(format!("build-closure-apply: {} produced no value", name))),
        }
    };
    let call_rt2 = |target: FunctionValue<'static>,
                     a0: BasicValueEnum<'static>,
                     a1: BasicValueEnum<'static>,
                     name: &str|
     -> Result<BasicValueEnum<'static>, EvalError> {
        let ap = b.build_alloca(i64_ty.array_type(2), "rt2_args").map_err(err)?;
        let p0 = unsafe { b.build_gep(i64_ty, ap, &[i64_ty.const_int(0, false)], "rt2_arg0_ptr").map_err(err)? };
        b.build_store(p0, a0).map_err(err)?;
        let p1 = unsafe { b.build_gep(i64_ty, ap, &[i64_ty.const_int(1, false)], "rt2_arg1_ptr").map_err(err)? };
        b.build_store(p1, a1).map_err(err)?;
        let argc = ctx.i32_type().const_int(2, false);
        let call = b.build_call(target, &[ap.into(), argc.into()], name).map_err(err)?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v),
            inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal(format!("build-closure-apply: {} produced no value", name))),
        }
    };

    let fn_ptr_int = call_rt1(rt_closure_fnptr, closure_int.into(), "closure_fnptr_raw")?;
    let env_len_i64 = call_rt1(rt_closure_env_len, closure_int.into(), "closure_env_len_raw")?;

    // Fixed-capacity (64) scratch buffer — the same cap `rt_closure_new`
    // enforces — filled by a genuine runtime loop since `env_len_i64` is
    // only known once this closure value actually exists.
    let scratch_ptr = b
        .build_alloca(i64_ty.array_type(64), "closure_apply_env_scratch")
        .map_err(err)?;
    let idx_alloca = b.build_alloca(i64_ty, "closure_apply_env_idx").map_err(err)?;
    b.build_store(idx_alloca, i64_ty.const_int(0, false)).map_err(err)?;

    let loop_header = ctx.append_basic_block(function, "closure_apply_env_loop_header");
    let loop_body = ctx.append_basic_block(function, "closure_apply_env_loop_body");
    let loop_exit = ctx.append_basic_block(function, "closure_apply_env_loop_exit");
    b.build_unconditional_branch(loop_header).map_err(err)?;

    b.position_at_end(loop_header);
    let i_val = b.build_load(i64_ty, idx_alloca, "closure_apply_env_i").map_err(err)?.into_int_value();
    let in_range = b.build_int_compare(inkwell::IntPredicate::ULT, i_val, env_len_i64, "closure_apply_env_in_range").map_err(err)?;
    b.build_conditional_branch(in_range, loop_body, loop_exit).map_err(err)?;

    b.position_at_end(loop_body);
    let elem_v = call_rt2(rt_closure_env_get, closure_int.into(), i_val.into(), "closure_env_elem")?;
    let dst_ptr = unsafe {
        b.build_gep(i64_ty, scratch_ptr, &[i_val], "closure_apply_env_dst").map_err(err)?
    };
    b.build_store(dst_ptr, elem_v).map_err(err)?;
    let i_next = b.build_int_add(i_val, i64_ty.const_int(1, false), "closure_apply_env_i_next").map_err(err)?;
    b.build_store(idx_alloca, i_next).map_err(err)?;
    b.build_unconditional_branch(loop_header).map_err(err)?;

    b.position_at_end(loop_exit);
    let env_len_i32 = b.build_int_truncate(env_len_i64, ctx.i32_type(), "closure_env_len_i32").map_err(err)?;
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let fn_ptr = b.build_int_to_ptr(fn_ptr_int, ptr_ty, "closure_fn_ptr_val").map_err(err)?;
    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = b
        .build_indirect_call(
            compiled_fn_type_with_env(),
            fn_ptr,
            &[args_ptr.into(), argc_val.into(), scratch_ptr.into(), env_len_i32.into()],
            "closure_apply_result",
        )
        .map_err(err)?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(RtValue::LlvmValue(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-closure-apply: callee produced no value".into())),
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
fn llvm_builder_build_malloc(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let count = match &args[1] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let count_val = ctx.i64_type().const_int(count, false);
    let ptr = builder
        .borrow()
        .build_array_malloc(ctx.i64_type(), count_val, "box")
        .map_err(|e| EvalError::Internal(format!("build-malloc: {}", e)))?;
    Ok(RtValue::LlvmValue(ptr.into()))
}

/// Frees a pointer `build-malloc` returned (or any other `llvm-value`
/// already holding a real pointer, e.g. after `build-int-to-ptr`) — the
/// inverse of `build-malloc`. See this section's own doc comment for why
/// nothing in `compiler.rs` calls this yet.
fn llvm_builder_build_free(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    builder.borrow().build_free(ptr).map_err(|e| EvalError::Internal(format!("build-free: {}", e)))?;
    Ok(RtValue::Unit)
}

/// Reinterprets an `i64`-valued `llvm-value` as a pointer — every compiled
/// value is a plain `i64` (`registry::llvm_module_def`'s doc comment), so a
/// general ADT box value read back out of a slot/argument/field needs this
/// before `load-raw`/`store-arg` (which both expect an already-pointer-typed
/// `llvm-value`) can dereference it — `compiler.rs`'s `compile-field-get`/
/// `compile-field-set` need it directly.
fn llvm_builder_build_int_to_ptr(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let v = expect_llvm_value(&args[1])?.into_int_value();
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let ptr = builder
        .borrow()
        .build_int_to_ptr(v, ptr_ty, "int_to_ptr")
        .map_err(|e| EvalError::Internal(format!("build-int-to-ptr: {}", e)))?;
    Ok(RtValue::LlvmValue(ptr.into()))
}

/// The inverse of `build-int-to-ptr` — `compile-construct`'s final step,
/// turning a freshly `build-malloc`'d pointer into the plain `i64` value
/// every other compiled value already is, matching how `build-make-closure`
/// does the same `ptrtoint` for a `ClosureBox`.
fn llvm_builder_build_ptr_to_int(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let ctx = crate::compile::llvm_context();
    let v = builder
        .borrow()
        .build_ptr_to_int(ptr, ctx.i64_type(), "ptr_to_int")
        .map_err(|e| EvalError::Internal(format!("build-ptr-to-int: {}", e)))?;
    Ok(RtValue::LlvmValue(v.into()))
}

/// Builds an enum value (`Option`/`Result`/user `defenum`) for `type_name`'s
/// `variant`, from already-evaluated `fields` — the encode-direction
/// counterpart of `match_pattern`'s dual heap-enum/native-`Data` decode.
/// Boxes onto the GC heap (`BoxedObj::Enum`) when every field converts via
/// `rtvalue_to_struct_field` (the same representable set every other
/// heap-boxed container already requires); for a field holding something
/// the heap cannot represent at all (an LLVM handle, a native-repr
/// `Scope<V>`, `Unit`, a `Builtin` function value — precisely
/// `Option<llvm-value>`/`Option<llvm-basic-block>`/... the (typelisp-hosted)
/// compiler body itself constructs throughout `compile-value` and friends),
/// falls back to the native `RtValue::Data`, mirroring `Scope<V>`'s own
/// heap-repr/native-repr duality (`Interp::heap_repr_kind`) — but decided
/// from the *value* already in hand rather than a static type, since
/// encoding (unlike decoding) is unambiguous regardless: unlike
/// `decode_field_typed`'s "quoted scalar vs plain scalar" ambiguity (a
/// decode-only concern), `rtvalue_to_struct_field` either faithfully
/// converts a field or doesn't apply to it at all. Any field converted
/// before a later one fails is harmless heap churn (the GC reclaims it),
/// not a leak.
fn build_enum_value(heap: &mut Heap, type_name: Path, variant: usize, fields: Vec<RtValue>) -> RtValue {
    let mut mem_fields = Vec::with_capacity(fields.len());
    for f in &fields {
        match rtvalue_to_struct_field(heap, f) {
            Ok(mv) => mem_fields.push(mv),
            Err(_) => return RtValue::Data { type_name, variant, fields },
        }
    }
    RtValue::Sexpr(heap.alloc_enum(type_name.to_string(), variant, mem_fields))
}

/// `Some(v)`/`None`, matching `option_def`'s variant order (`some` = 0,
/// `none` = 1) — see [`build_enum_value`] for the heap/native duality this
/// goes through.
fn option_value(heap: &mut Heap, v: Option<RtValue>) -> RtValue {
    let (variant, fields) = match v {
        Some(x) => (0, vec![x]),
        None => (1, vec![]),
    };
    build_enum_value(heap, Path::root("option"), variant, fields)
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
fn expect_hashable_key(v: &RtValue) -> Result<(), EvalError> {
    match v {
        RtValue::Int(_) | RtValue::Bool(_) | RtValue::Char(_) | RtValue::Str(_) => Ok(()),
        other => Err(EvalError::Panic(format!("HashTable: unsupported key type {:?}", other))),
    }
}

fn hashtable_get(heap: &mut Heap, args: &[RtValue], ret_ty: &Type) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    expect_hashable_key(&args[1])?;
    let key = rtvalue_to_struct_field(heap, &args[1])?;
    let val_ty = option_payload_ty(ret_ty)?;
    let found = heap.hashtable_get(id, key).map(|v| decode_field_typed(heap, v, &val_ty));
    Ok(option_value(heap, found))
}

fn hashtable_set(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    expect_hashable_key(&args[1])?;
    let key = rtvalue_to_struct_field(heap, &args[1])?;
    let val = rtvalue_to_struct_field(heap, &args[2])?;
    heap.hashtable_set(id, key, val);
    Ok(RtValue::Unit)
}

fn hashtable_remove(heap: &mut Heap, args: &[RtValue], ret_ty: &Type) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    expect_hashable_key(&args[1])?;
    let key = rtvalue_to_struct_field(heap, &args[1])?;
    let val_ty = option_payload_ty(ret_ty)?;
    let removed = heap.hashtable_remove(id, key).map(|v| decode_field_typed(heap, v, &val_ty));
    Ok(option_value(heap, removed))
}

fn hashtable_count(heap: &Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(RtValue::Int(heap.hashtable_count(id) as i64))
}

fn hashtable_clear(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.hashtable_clear(id);
    Ok(RtValue::Unit)
}

/// Builds a `Vector<T>` runtime value (the boxed-struct variable-length
/// `"vector"` representation, see `vector_def`'s doc comment) directly out of
/// already-encoded `mem::Value` fields — the shared tail of `hashtable_keys`/
/// `hashtable_values`/`hashtable_entries`, whose fields come straight from
/// `Heap::hashtable_pairs` and so need no `RtValue` round-trip.
fn vector_of_raw(heap: &mut Heap, fields: Vec<Value>) -> RtValue {
    RtValue::Sexpr(heap.alloc_struct("vector".to_string(), fields))
}

fn hashtable_keys(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let fields = heap.hashtable_pairs(id).into_iter().map(|(k, _)| k).collect();
    Ok(vector_of_raw(heap, fields))
}

fn hashtable_values(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
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
fn hashtable_entries(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let fields = heap
        .hashtable_pairs(id)
        .into_iter()
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
fn expect_struct_box(v: &RtValue) -> Result<BoxId, EvalError> {
    match v {
        RtValue::Sexpr(Value::Boxed(id)) => Ok(*id),
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
fn rtvalue_to_struct_field(heap: &mut Heap, v: &RtValue) -> Result<Value, EvalError> {
    match v {
        RtValue::Int(n) => Ok(Value::Int(*n)),
        RtValue::Bool(b) => Ok(Value::Bool(*b)),
        RtValue::Char(c) => Ok(Value::Char(*c)),
        RtValue::Str(s) => Ok(heap.alloc_string(s.to_string())),
        RtValue::Float(f) => Ok(heap.alloc_float(*f)),
        RtValue::Bignum(n) => Ok(heap.alloc_bignum((**n).clone())),
        RtValue::Ratio(r) => Ok(heap.alloc_ratio((**r).clone())),
        RtValue::Sexpr(v) => Ok(*v),
        other => Err(EvalError::Internal(format!(
            "struct field: {:?} is not yet representable in the boxed struct representation",
            other
        ))),
    }
}

/// `Option`/`Result`'s fully-qualified [`Path`]s — [`Interp::enum_fields_representable`]
/// reads these two structurally (their variants' field types are always
/// exactly the type's own generic arguments, no registry lookup needed);
/// a user `defenum` instead looks up [`Interp::enum_defs`].
fn option_path() -> Path {
    Path::root("option")
}
fn result_path() -> Path {
    Path::root("result")
}

/// Decodes a `mem::Value` read out of a `BoxedObj::Struct` field (or
/// `Vector<T>` element, `HashTable<K,V>` value, `defvar` global) as the
/// slot's *declared* type directs — `rtvalue_to_struct_field`'s inverse,
/// and the type-driven replacement for the value-shape heuristic that
/// existed under type-erased generic evaluation. Post-monomorphization
/// every reader has the concrete declared type in hand (an accessor's
/// `FieldGet` node type, a builtin call site's checked return type), so
/// the one genuine ambiguity — a `Sexpr`-declared slot holding a scalar
/// datum, where a stored quoted `42` is a `Value::Int` that must come back
/// as the `Sexpr` it is, not as `RtValue::Int` — is decided statically
/// here, never guessed from the value. An enum-typed slot needs no arm of
/// its own (unlike before the enum-representation unification's compiler
/// flip): it's a `Value::Boxed` at a `BoxedObj::Enum`, which
/// [`decode_nonsexpr_field`]'s catch-all already turns into `RtValue::Sexpr`
/// correctly, the same as any other boxed value.
fn decode_field_typed(heap: &Heap, v: Value, ty: &Type) -> RtValue {
    if is_sexpr_ty(ty) {
        RtValue::Sexpr(v)
    } else {
        decode_nonsexpr_field(heap, v)
    }
}

/// [`decode_field_typed`]'s non-`Sexpr` half: for every declared type
/// *other than* `Sexpr`, `rtvalue_to_struct_field`'s encoding is injective —
/// `Int`/`Bool`/`Char`/`Str` map straight back, a `Boxed` holding a float
/// (`Heap::is_float` — the *positive* test, since structs, `HashTable`s,
/// heap scopes, and closures are boxed too) is `RtValue::Float`, and every
/// other `Boxed` (a nested `defstruct`/`Vector<T>`/`cons-cell<K,V>`/
/// `HashTable<K,V>`/heap `Scope<V>`/closure, whose declared type is some
/// concrete named heap-repr type) becomes `RtValue::Sexpr`, the same
/// wrapper a top-level struct value itself uses — so the stored shape alone
/// determines the result with no ambiguity. Also reached directly by `match_pattern`'s
/// boxed-struct arm, whose per-field `Pattern::Ctor::sexpr_fields` (baked at
/// check time, exact post-monomorphization) is precisely the
/// "`Sexpr`-declared or not" bit `decode_field_typed` reads off a `Type`.
fn decode_nonsexpr_field(heap: &Heap, v: Value) -> RtValue {
    match v {
        Value::Int(n) => RtValue::Int(n),
        Value::Bool(b) => RtValue::Bool(b),
        Value::Char(c) => RtValue::Char(c),
        Value::Str(id) => RtValue::Str(heap.string(id).into()),
        Value::Boxed(id) if heap.is_float(id) => RtValue::Float(heap.float_value(id)),
        Value::Boxed(id) if heap.is_bignum(id) => RtValue::Bignum(Rc::new(heap.bignum_value(id).clone())),
        Value::Boxed(id) if heap.is_ratio(id) => RtValue::Ratio(Rc::new(heap.ratio_value(id).clone())),
        other => RtValue::Sexpr(other),
    }
}

fn expect_int_index(v: &RtValue) -> Result<usize, EvalError> {
    match v {
        RtValue::Int(n) if *n >= 0 => Ok(*n as usize),
        other => Err(EvalError::Panic(format!("Vector: invalid index {:?}", other))),
    }
}

fn vector_push(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let v = rtvalue_to_struct_field(heap, &args[1])?;
    heap.struct_push_field(id, v);
    Ok(RtValue::Unit)
}

/// `ret_ty` is the call site's checked return type — always the concrete
/// element type post-monomorphization (`Vector<Sexpr>`'s `get` returns
/// `Sexpr` there), so the element decode is fully type-directed; see
/// [`decode_field_typed`].
fn vector_get(heap: &mut Heap, args: &[RtValue], ret_ty: &Type) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let i = expect_int_index(&args[1])?;
    if i >= heap.struct_field_count(id) {
        return Err(EvalError::Panic(format!("Vector: index {} out of bounds", i)));
    }
    let raw = heap.struct_field(id, i);
    Ok(decode_field_typed(heap, raw, ret_ty))
}

fn vector_set(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let i = expect_int_index(&args[1])?;
    if i >= heap.struct_field_count(id) {
        return Err(EvalError::Panic(format!("Vector: index {} out of bounds", i)));
    }
    let v = rtvalue_to_struct_field(heap, &args[2])?;
    heap.struct_set_field(id, i, v);
    Ok(RtValue::Unit)
}

fn vector_len(heap: &Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(RtValue::Int(heap.struct_field_count(id) as i64))
}

// The native (`RtValue::Scope`) halves of the six scope builtin methods —
// thin adapters between the `args` slice and [`NativeScope`]'s method
// surface, which owns the frame-stack semantics (search order, top-frame
// writes, `clone-frames` sharing — see that struct's doc comments).

fn expect_scope(v: &RtValue) -> Result<&NativeScope, EvalError> {
    match v {
        RtValue::Scope(scope) => Ok(scope),
        other => Err(EvalError::Internal(format!("expected a Scope, got {:?}", other))),
    }
}

fn scope_new() -> RtValue {
    RtValue::Scope(NativeScope::new())
}

fn scope_clone_frames(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Scope(expect_scope(&args[0])?.clone_frames()))
}

fn scope_push_frame(args: &[RtValue]) -> Result<RtValue, EvalError> {
    expect_scope(&args[0])?.push_frame();
    Ok(RtValue::Unit)
}

fn scope_pop_frame(args: &[RtValue]) -> Result<RtValue, EvalError> {
    expect_scope(&args[0])?.pop_frame();
    Ok(RtValue::Unit)
}

fn scope_get(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let scope = expect_scope(&args[0])?;
    let name = expect_str(&args[1])?;
    let found = scope.get(name);
    Ok(option_value(heap, found))
}

fn scope_set(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let scope = expect_scope(&args[0])?;
    let name = expect_str(&args[1])?;
    scope.set(name, args[2].clone())?;
    Ok(RtValue::Unit)
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

fn scope_clone_frames_heap(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    Ok(RtValue::Sexpr(heap.scope_clone_frames(id)))
}

fn scope_push_frame_heap(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.scope_push_frame(id);
    Ok(RtValue::Unit)
}

fn scope_pop_frame_heap(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    heap.scope_pop_frame(id);
    Ok(RtValue::Unit)
}

fn scope_get_heap(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let name = expect_str(&args[1])?;
    let found = heap.scope_get(id, name).map(RtValue::Sexpr);
    Ok(option_value(heap, found))
}

/// `Heap::scope_set` panics on an empty frame stack (the mem layer's
/// internal-invariant-trap convention); every frame *is* poppable from
/// typelisp (`pop-frame`), so the guard runs here first and reports the
/// same `EvalError` the native `scope_set` does — the `expect_hashable_key`
/// precedent of keeping a user-reachable condition a catchable evaluation
/// error rather than a Rust panic.
fn scope_set_heap(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let name = expect_str(&args[1])?.to_string();
    let v = match &args[2] {
        RtValue::Sexpr(v) => *v,
        other => {
            return Err(EvalError::Internal(format!(
                "heap Scope::set: value is not a heap-repr element: {:?}",
                other
            )))
        }
    };
    if heap.scope_frame_count(id) == 0 {
        return Err(EvalError::Internal("Scope::set: no frame to write into".into()));
    }
    heap.scope_set(id, &name, v);
    Ok(RtValue::Unit)
}

/// Resolve an `i32` index against a sequence's current length: out of range
/// (including negative) is `None`, the caller turns that into a panic — the
/// type system can't express the bound, the same "runtime panic for what
/// types can't catch" precedent as `car`/`cdr` on a non-`Cons` `Sexpr`. Used
/// by `string::ref`'s bounds check.
fn checked_index(i: i64, len: usize) -> Option<usize> {
    if i >= 0 && (i as usize) < len { Some(i as usize) } else { None }
}

fn expect_str(v: &RtValue) -> Result<&str, EvalError> {
    match v {
        RtValue::Str(s) => Ok(s.as_ref()),
        other => Err(EvalError::Internal(format!("expected a Str, got {:?}", other))),
    }
}

fn expect_char(v: &RtValue) -> Result<char, EvalError> {
    match v {
        RtValue::Char(c) => Ok(*c),
        other => Err(EvalError::Internal(format!("expected a Char, got {:?}", other))),
    }
}

/// `char->int` (`registry::char_assoc`): a `char`'s Unicode scalar value as
/// `i32` (uniformly `RtValue::Int(i64)` at runtime — see
/// `eval_int_builtin`'s doc comment). Always succeeds — every `char` is
/// already a valid scalar value, unlike `int->char`'s reverse direction.
fn char_to_int(args: &[RtValue]) -> Result<RtValue, EvalError> {
    expect_char(&args[0]).map(|c| RtValue::Int(c as i64))
}

fn string_length(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Int(expect_str(&args[0])?.chars().count() as i64))
}

fn string_ref(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let chars: Vec<char> = expect_str(&args[0])?.chars().collect();
    let i = rt_i64(&args[1])?;
    match checked_index(i, chars.len()) {
        Some(idx) => Ok(RtValue::Char(chars[idx])),
        None => Err(EvalError::Panic(format!("ref: index {} out of range (length {})", i, chars.len()))),
    }
}

fn string_substring(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let chars: Vec<char> = expect_str(&args[0])?.chars().collect();
    let start = rt_i64(&args[1])?;
    let end = rt_i64(&args[2])?;
    let len = chars.len() as i64;
    if start < 0 || end > len || start > end {
        return Err(EvalError::Panic(format!("substring: invalid range {}..{} (length {})", start, end, len)));
    }
    let s: String = chars[start as usize..end as usize].iter().collect();
    Ok(RtValue::Str(s.into()))
}

fn string_append(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Str(format!("{}{}", expect_str(&args[0])?, expect_str(&args[1])?).into()))
}

/// True CL identity for `string` — same underlying `Rc<str>` allocation, not
/// merely equal content (that's [`string_content_eq`]/[`string_content_eqp`]
/// instead — CL's `eq`/`eql` never do structural string comparison, only
/// `equal`/`equalp` do). See `RtValue::Str`'s doc comment for why `Rc` is
/// what makes this meaningful at all (a plain owned `String`, re-cloned on
/// every variable read, would have no stable identity to compare).
fn string_identity_eq(args: &[RtValue]) -> Result<RtValue, EvalError> {
    match (&args[0], &args[1]) {
        (RtValue::Str(a), RtValue::Str(b)) => Ok(RtValue::Bool(Rc::ptr_eq(a, b))),
        (other0, other1) => Err(EvalError::Internal(format!("string::eq: expected two Str arguments, got {:?}/{:?}", other0, other1))),
    }
}

/// Content equality (case-sensitive) — CL's `equal`/`string=`, *not* `eq`
/// (identity — see `RtValue::Str`'s doc comment and `docs/cl-equivalence-catalog.md`'s
/// eq/eql/equal/equalp section). Named for what it computes, not for which
/// builtin method currently calls it.
fn string_content_eq(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_str(&args[0])? == expect_str(&args[1])?))
}

/// Content equality ignoring ASCII case — CL's `equalp` for strings.
fn string_content_eqp(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_str(&args[0])?.eq_ignore_ascii_case(expect_str(&args[1])?)))
}

fn string_lt(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_str(&args[0])? < expect_str(&args[1])?))
}

/// The `<`/`<=`/`>`/`>=` comparison operators on `string`, lexicographic (byte
/// order) — the counterpart of `eval_int_builtin`'s numeric comparisons, kept
/// in one function for the same reason (one match over the operator symbol).
fn string_compare(method: &str, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let a = expect_str(&args[0])?;
    let b = expect_str(&args[1])?;
    Ok(RtValue::Bool(match method {
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        ">=" => a >= b,
        other => return Err(EvalError::Internal(format!("string_compare: not a comparison operator: {}", other))),
    }))
}

fn char_eq(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_char(&args[0])? == expect_char(&args[1])?))
}

fn char_lt(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_char(&args[0])? < expect_char(&args[1])?))
}

/// The `<`/`<=`/`>`/`>=` comparison operators on `char`, by Unicode scalar
/// value (a compiled `char` is a raw `i64` code point, so this matches the
/// integer `icmp`s `compiler.rs` emits for the same operators).
fn char_compare(method: &str, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let a = expect_char(&args[0])?;
    let b = expect_char(&args[1])?;
    Ok(RtValue::Bool(match method {
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        ">=" => a >= b,
        other => return Err(EvalError::Internal(format!("char_compare: not a comparison operator: {}", other))),
    }))
}

/// CL's `equalp` for `char` — case-insensitive (`(equalp #\A #\a)` is true).
fn char_eqp(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_char(&args[0])?.eq_ignore_ascii_case(&expect_char(&args[1])?)))
}

fn expect_bool(v: &RtValue) -> Result<bool, EvalError> {
    match v {
        RtValue::Bool(b) => Ok(*b),
        other => Err(EvalError::Internal(format!("expected a Bool, got {:?}", other))),
    }
}

fn bool_eq(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_bool(&args[0])? == expect_bool(&args[1])?))
}

/// `eq` on `Sexpr`: compares the underlying `mem::Value` directly (see
/// `registry::sexpr_assoc`'s doc comment for why this matches CL's `eq`
/// semantics — cons identity, scalar/symbol value equality).
fn sexpr_eq(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(rt_sexpr(&args[0])? == rt_sexpr(&args[1])?))
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
fn sexpr_eql(heap: &Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let a = rt_sexpr(&args[0])?;
    let b = rt_sexpr(&args[1])?;
    Ok(RtValue::Bool(eql_val(heap, a, b)))
}

/// The scalar core of `eql` on two `Sexpr` payloads: `==` (plain `Value`
/// identity/value equality) except two separately-boxed but equal `Float`s,
/// which are `eql` by value — see [`sexpr_eql`]. Shared by [`sexpr_equal`]/
/// [`sexpr_equalp`] as their atom-comparison base case.
fn eql_val(heap: &Heap, a: Value, b: Value) -> bool {
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

fn sexpr_equal(heap: &Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let a = rt_sexpr(&args[0])?;
    let b = rt_sexpr(&args[1])?;
    Ok(RtValue::Bool(sexpr_equal_val(heap, a, b)))
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
        (a, b) => eql_val(heap, a, b),
    }
}

fn sexpr_equalp(heap: &Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let a = rt_sexpr(&args[0])?;
    let b = rt_sexpr(&args[1])?;
    Ok(RtValue::Bool(sexpr_equalp_val(heap, a, b)))
}

/// Try to match a pattern against a value, returning the bindings on success.
/// `heap` is needed to destructure `Sexpr` values (`RtValue::Sexpr`), which
/// hold their `Cons`/`Str`/`Sym` payloads in the cons heap.
fn match_pattern(heap: &Heap, pat: &Pattern, v: &RtValue) -> Option<Vec<(String, SlotKind, RtValue)>> {
    match pat {
        Pattern::Wildcard => Some(Vec::new()),
        Pattern::Bind(n, heap_bind) => {
            let kind = if *heap_bind { SlotKind::Heap } else { SlotKind::Native };
            Some(vec![(n.clone(), kind, v.clone())])
        }
        Pattern::Int(n) => match v {
            RtValue::Int(m) if m == n => Some(Vec::new()),
            _ => None,
        },
        Pattern::Bool(b) => match v {
            RtValue::Bool(m) if m == b => Some(Vec::new()),
            _ => None,
        },
        Pattern::Char(c) => match v {
            RtValue::Char(m) if m == c => Some(Vec::new()),
            _ => None,
        },
        Pattern::Ctor { variant, args, sexpr_fields, .. } => match v {
            // The native-repr fallback for an enum instantiated over a
            // type the heap cannot represent at all (`build_enum_value`'s
            // doc comment — `Option<llvm-value>` and friends). Matched by
            // recursing on each field directly, exactly like the pre-
            // unification code always did; no `sexpr_fields`/heap decode
            // needed since a native field is never `mem::Value`-encoded.
            RtValue::Data { variant: vv, fields, .. } if vv == variant && fields.len() == args.len() => {
                let mut binds = Vec::new();
                for (p, f) in args.iter().zip(fields.iter()) {
                    binds.extend(match_pattern(heap, p, f)?);
                }
                Some(binds)
            }
            // An enum value (`Option`/`Result`/user `defenum`) is a boxed
            // `BoxedObj::Enum`: tag-test its variant index, then decode each
            // field exactly the way the struct arm below does. Guarded on
            // `heap.is_enum` first (like the struct arm's own guard) so a
            // genuine `Sexpr` datum falls through to the `RtValue::Sexpr`
            // arm; a *matching-shape* enum whose variant differs also falls
            // through, where `match_sexpr_ctor`'s own guards all fail —
            // `None`, the same "try the next match arm" a wrong-variant
            // `RtValue::Data` used to produce.
            RtValue::Sexpr(Value::Boxed(id))
                if heap.is_enum(*id) && heap.enum_variant(*id) == *variant && heap.enum_field_count(*id) == args.len() =>
            {
                let mut binds = Vec::new();
                for (i, p) in args.iter().enumerate() {
                    let raw = heap.enum_field(*id, i);
                    let f = if sexpr_fields.get(i).copied().unwrap_or(false) {
                        RtValue::Sexpr(raw)
                    } else {
                        decode_nonsexpr_field(heap, raw)
                    };
                    binds.extend(match_pattern(heap, p, &f)?);
                }
                Some(binds)
            }
            // A `defstruct` has exactly one variant (`"new"`, index 0), so
            // `variant` always matches here — only the field count/pattern
            // shape can fail. Guarded on `heap.is_struct` first so a value
            // that's a genuine `Sexpr` datum (not a boxed struct) falls
            // through to the `RtValue::Sexpr` arm below instead.
            RtValue::Sexpr(Value::Boxed(id))
                if heap.is_struct(*id) && *variant == 0 && heap.struct_field_count(*id) == args.len() =>
            {
                let mut binds = Vec::new();
                for (i, p) in args.iter().enumerate() {
                    let raw = heap.struct_field(*id, i);
                    // `sexpr_fields` (baked at check time, exact now that
                    // generic patterns are checked monomorphized) is the
                    // "`Sexpr`-declared or not" bit `decode_field_typed`
                    // reads off a `Type` — same type-directed decode, with
                    // the bit precomputed per field.
                    let f = if sexpr_fields.get(i).copied().unwrap_or(false) {
                        RtValue::Sexpr(raw)
                    } else {
                        decode_nonsexpr_field(heap, raw)
                    };
                    binds.extend(match_pattern(heap, p, &f)?);
                }
                Some(binds)
            }
            RtValue::Sexpr(sv) => match_sexpr_ctor(heap, *variant, args, *sv),
            _ => None,
        },
    }
}

/// Match a `Sexpr` constructor pattern against a heap-backed `Sexpr` value,
/// destructuring through `heap` (`car`/`cdr`/`symbol_name`/`string`) rather
/// than an `RtValue::Data` shape.
fn match_sexpr_ctor(heap: &Heap, variant: usize, args: &[Pattern], v: Value) -> Option<Vec<(String, SlotKind, RtValue)>> {
    match (variant, v) {
        (SEXPR_NIL, Value::Empty) => Some(Vec::new()),
        (SEXPR_INT, Value::Int(n)) => match_pattern(heap, &args[0], &RtValue::Int(n)),
        (SEXPR_FLOAT, Value::Boxed(id)) if heap.is_float(id) => {
            match_pattern(heap, &args[0], &RtValue::Float(heap.float_value(id)))
        }
        (SEXPR_BIGNUM, Value::Boxed(id)) if heap.is_bignum(id) => {
            match_pattern(heap, &args[0], &RtValue::Bignum(Rc::new(heap.bignum_value(id).clone())))
        }
        (SEXPR_RATIO, Value::Boxed(id)) if heap.is_ratio(id) => {
            match_pattern(heap, &args[0], &RtValue::Ratio(Rc::new(heap.ratio_value(id).clone())))
        }
        (SEXPR_CHAR, Value::Char(c)) => match_pattern(heap, &args[0], &RtValue::Char(c)),
        (SEXPR_BOOL, Value::Bool(b)) => match_pattern(heap, &args[0], &RtValue::Bool(b)),
        (SEXPR_SYM, Value::Symbol(id)) => {
            // `(Sym v)` binds `v : Symbol` — the symbol value itself, carried as
            // `RtValue::Sexpr(Value::Symbol(id))`, not its textual name.
            match_pattern(heap, &args[0], &RtValue::Sexpr(Value::Symbol(id)))
        }
        (SEXPR_STR, Value::Str(id)) => {
            match_pattern(heap, &args[0], &RtValue::Str(heap.string(id).into()))
        }
        (SEXPR_CONS, Value::Cons(_)) => {
            let car = heap.car(v).ok()?;
            let cdr = heap.cdr(v).ok()?;
            let mut binds = match_pattern(heap, &args[0], &RtValue::Sexpr(car))?;
            binds.extend(match_pattern(heap, &args[1], &RtValue::Sexpr(cdr))?);
            Some(binds)
        }
        _ => None,
    }
}
