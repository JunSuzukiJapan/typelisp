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
//! `Sexpr` values (see [`RtValue::Sexpr`]) live in the GC-managed cons [`Heap`]
//! shared with the reader, so `eval` threads a `&mut Heap` throughout. Since
//! cons cells held by the interpreter (in locals, globals, closures) are
//! otherwise invisible to [`Heap::gc`], every mutable [`Slot`] is registered
//! (weakly) in [`Interp::slots`]; [`Interp::sync_roots`] rebuilds the heap's
//! root set from whatever is still live there right before any allocation
//! that could trigger a collection.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::rc::{Rc, Weak};

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, ToPrimitive, Zero};

use crate::{BoxId, CompileTarget, Expr, Heap, Loc, MacroExpander, Path, Pattern, QuotedSexpr, Ref, SymId, TopLevel, Type, Typed, Value};

use super::scope;
use super::value::{EvalError, NativeScope, RtValue, Slot, SlotKind};

/// A registered function or method body with its parameter names. Lives at
/// exactly one [`scope::ModuleScope`] tree node — its own defining module —
/// rather than in a flat program-wide table; see that module's doc comment.
pub(crate) struct FnDef {
    /// Parameter names, including the receiver name first for instance methods.
    pub(crate) params: Vec<String>,
    /// Each parameter's binding-slot routing, parallel to `params` — derived
    /// once at registration from the declared parameter types (`sig`; a
    /// `defmacro`'s parameters are all `Sexpr`, hence all `Heap`), so
    /// `Interp::apply` needs no type information per call.
    pub(crate) kinds: Vec<SlotKind>,
    pub(crate) body: Vec<Typed>,
    /// Only ever set for a `defmacro` with a trailing `&rest` parameter (see
    /// [`Interp::expand_macro`]); always `false` for `defun`/`defmethod`,
    /// which `apply` calls 1:1 regardless.
    pub(crate) rest: bool,
    /// `(parameter types, return type)`, parallel to `params` — `None` for a
    /// `defmacro` (every parameter and the implicit return are always
    /// `Sexpr`, see `check::registry::MacroDef`'s doc comment) since a macro
    /// is never a `compile` target. The tree-walking evaluator itself never
    /// needs this (it's already erased everywhere else, see
    /// `Checker::check_call`'s doc comment) — it exists solely so
    /// `Interp::compile_function` can build an LLVM function signature
    /// without the checker's `Registry` (which `Interp` otherwise has no
    /// access to).
    pub(crate) sig: Option<(Vec<Type>, Type)>,
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
    /// evaluation of `body` (needed since `body`'s `Vec<Typed>` is too large
    /// to clone per call) — if `compiled` needed the same outer `RefCell`,
    /// an inner `(compile name)` call reached from within that body (e.g. a
    /// self-referential or mutually-recursive JIT request) that tried to
    /// populate this very same `FnDef`'s `compiled` slot would panic against
    /// `apply`'s already-live borrow. Scoping the `RefCell` down to just this
    /// field keeps that borrow-and-mutate pair independent, exactly like the
    /// old design's two separate top-level fields (`fns`/`compiled`) did.
    pub(crate) compiled: RefCell<Option<Rc<crate::compile::CompiledFn>>>,
}

/// A user `defenum`'s type parameters and variants (each one's declared
/// field types), as `TopLevel::Defenum` baked them in at check time — the
/// entry [`Interp::enum_defs`] keeps per enum type; see that field's doc
/// comment for why the interpreter needs this at all (compiled-global box
/// decoding) and why `Option`/`Result` are not represented this way.
#[derive(Clone)]
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

/// The interpreter state: a runtime mirror of the checker's module tree
/// ([`scope::ModuleScope`], rooted here), holding every free function/
/// macro/method/global/struct/enum by its own defining module and
/// unqualified name — never by a single flat [`Path`]-keyed table spanning
/// the whole program. See `eval::scope`'s module doc comment for why, and
/// for how a reference re-resolves against this tree instead of trusting a
/// checker-baked [`Path`] as a lookup key.
pub struct Interp {
    root: scope::ModuleScope,
    /// Every `Native` slot ever created, held weakly. A slot stays discoverable
    /// here for exactly as long as it's reachable some other way (an env frame
    /// on the call stack, `globals`, or a closure's captured environment) —
    /// once that owner drops the `Rc`, the entry quietly goes dead and is
    /// pruned on the next [`Self::sync_roots`].
    slots: RefCell<Vec<Weak<RefCell<RtValue>>>>,
    /// How many roots `sync_roots` last pushed onto the heap, so it knows how
    /// many to pop before recomputing the set from scratch.
    rooted: Cell<usize>,
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
    /// Every closure "constructor" [`Self::jit_define_closure`] has ever
    /// JIT-compiled, held forever so the `ExecutionEngine`/code backing each
    /// one's `fn_ptr` is never dropped — the "engine graveyard" the
    /// closure-unification plan calls for: a `BoxedObj::CompiledClosure`
    /// only stores a bare `fn_ptr: usize` (see that variant's doc comment),
    /// nothing else keeps the JIT engine that produced it alive once the
    /// constructor call returns.
    jit_graveyard: RefCell<Vec<Rc<crate::compile::CompiledFn>>>,
    /// Definition-site cache for [`Self::jit_define_closure`]: `(Loc, debug
    /// repr of the closure's `Type::Fn`)` -> the already-JIT'd constructor,
    /// so evaluating the same `lambda` literal repeatedly (a loop body, a
    /// hot function) JITs its constructor once. `Type` has no `Eq`/`Hash`
    /// (`src/types.rs`), so the signature half of the key is its `Debug`
    /// string rather than the `Type` itself — adequate for a cache (a false
    /// miss only costs a redundant JIT, never incorrect reuse, since
    /// `Debug` is structural). A node with no `Loc` (macro-synthesized, a
    /// `labels` sibling — [`Self::jit_define_closure`] doesn't try to
    /// synthesize one) is never cached, only ever freshly compiled.
    jit_ctor_cache: RefCell<HashMap<(Loc, String), Rc<crate::compile::CompiledFn>>>,
    /// Monotonic source for [`Self::jit_define_closure`]'s synthetic
    /// constructor function names (`"closure_ctor$0"`, `"closure_ctor$1"`,
    /// ...) — never reused, so two constructors can never collide even
    /// across separate throwaway modules.
    jit_ctor_counter: Cell<u64>,
}

/// One outgoing edge of the top-level compile call graph
/// [`Interp::compute_sccs`] walks: either a plain `Expr::Call` target (a
/// `defun`) or an `Expr::Assoc` target (a `defmethod`, keyed the same way
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

/// Why a [`Interp::jit_define_closure`] attempt didn't produce a compiled
/// closure — the classification that (from closure-unification Stage 9
/// onward) is unconditional policy: it must escalate exactly the failures
/// that represent actionable holes in compiled coverage, and nothing else.
enum JitDecline {
    /// A decline that is not itself a *coverage* gap — a setup/environment
    /// condition rather than a hole in what the compiler can lower. Two cases
    /// remain: a native-tier type (the self-hosted compiler island's own
    /// `llvm-*`/`Scope<llvm-value>` closures — see [`Interp::is_jit_tier_ty`];
    /// kept distinct so the reason string names the real cause), and the
    /// compiler island not being loaded at all. Both are unreachable in
    /// normal operation — since Stage 5 every `typl` entry point installs the
    /// native island ([`crate::compiler::load_aot`]), which runs *compiled*,
    /// so its own bodies never reach definition-time JIT, and no user type is
    /// native-tier; "island not loaded" survives only for an embedder that
    /// builds an `Interp` and skips island loading. Interp-closure removal
    /// Stage 8c deleted the interpreted-closure fallback these used to select,
    /// so [`Interp::jit_closure_from_result`] now escalates a `Benign` decline
    /// to the same hard [`EvalError::Panic`] a `Gap` gets — there is no longer
    /// a distinct behavior, only a distinct message. (`TYPELISP_CLOSURE_JIT=off`
    /// and heap exhaustion mid-JIT were `Benign` too until Stage 7 retired
    /// both; macro-expansion suppression until Stage 6.)
    Benign(String),
    /// A real, actionable hole in compiled coverage (an uncompiled call
    /// target, a constructor-JIT failure, a JIT-time heap exhaustion, ...) —
    /// always escalated to a hard [`EvalError::Panic`] (Stage 9 made this
    /// unconditional; interp-closure removal Stage 7 folded heap exhaustion
    /// mid-JIT in here too).
    Gap(String),
}

impl JitDecline {
    /// The human-readable reason, for `TYPELISP_CLOSURE_JIT_LOG` and the
    /// `Gap` panic message.
    fn reason(&self) -> &str {
        match self {
            JitDecline::Benign(r) | JitDecline::Gap(r) => r,
        }
    }

    /// Classifies an error out of the constructor-JIT pipeline
    /// ([`Interp::jit_compile_closure_ctor`]'s `translate_and_compile` →
    /// `apply`(`compile-function`) → verify → JIT chain) as a real
    /// [`JitDecline::Gap`] — a hard failure. Interp-closure removal Stage 7
    /// retired the one former exception, heap exhaustion mid-JIT: it is no
    /// longer classified [`JitDecline::Benign`] (which would silently fall
    /// back to an interpreted closure), so like any other `Gap` it now
    /// escalates to a hard [`EvalError::Panic`] — treating a JIT-time heap
    /// exhaustion as the resource error it is, the same as a heap exhaustion
    /// anywhere else, rather than papering over it with a slower interpreted
    /// closure.
    fn from_ctor_error(msg: String) -> JitDecline {
        JitDecline::Gap(format!("constructor JIT failed: {}", msg))
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
        // `vector` is registered directly in `Registry::with_builtins`
        // (`registry::vector_def`) with `AdtKind::Struct`, so its
        // `Expr::Construct` sites already get `mutable = true`
        // (`Checker::check_construct`) — but it never executes a
        // `TopLevel::Defstruct`, the only other place a struct `TypeEntry`
        // gets populated, so it's seeded here by hand, directly at the root
        // node (it has no defining module of its own). `hashtable`/`scope`
        // don't need this: both are `AdtKind::Sum` (`registry::hashtable_def`/
        // `scope_def`) with no `variants`/`field_names` of their own — user
        // code only ever builds one through its `::new()` assoc fn, never
        // `Expr::Construct`, so there is no construct site to affect.
        let mut root = scope::ModuleScope::default();
        root.types.insert("vector".to_string(), scope::TypeEntry::Struct);
        Interp {
            root,
            slots: RefCell::new(Vec::new()),
            rooted: Cell::new(0),
            compiled_globals: RefCell::new(HashMap::new()),
            jit_graveyard: RefCell::new(Vec::new()),
            jit_ctor_cache: RefCell::new(HashMap::new()),
            jit_ctor_counter: Cell::new(0),
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

    /// A `Slot::TypedCell` — closure unification Stage 7's promotion of an
    /// otherwise-`Native` binding to a GC heap cell because
    /// `freevars::names_captured_by_nested` says some nested `lambda`/
    /// `labels` in the enclosing body captures it (see call sites in
    /// [`Self::apply`]/`Expr::Let`). `ty` is the binding's own declared
    /// type — `Slot::TypedCell::get`/`set` need it to decode/encode the
    /// cell's raw `mem::Value` correctly (`rtvalue_to_struct_field`'s
    /// encoding is only unambiguous with the static type in hand, exactly
    /// like a `defstruct` field read).
    fn typed_cell_slot(&self, heap: &mut Heap, ty: Type, v: RtValue) -> Result<Slot, EvalError> {
        let encoded = rtvalue_to_struct_field(heap, &v)?;
        Ok(Slot::TypedCell(heap.alloc_cell(encoded), ty))
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

    /// Closure unification Stage 7's JIT tier judgment: whether `ty` has
    /// *any* compiled representation at all — as opposed to
    /// [`Self::heap_repr_kind`], which only decides *which* representation a
    /// type that unconditionally has one gets. Every ordinary user type
    /// (scalars, `Str`, `Fn`, `Sexpr`, structs, enums, `bignum`/`ratio`) is
    /// `true`; the five LLVM FFI handle types and a `Scope<V>` of one of
    /// them — used only inside the self-hosted compiler's own source
    /// (`compiler.rs`'s embedded `SOURCE`) — are `false`. A `lambda`/
    /// `labels`/`FnRef`/`MethodRef` whose parameter, return, or any captured
    /// name's type fails this check cannot be JIT-compiled and declines
    /// [`JitDecline::Benign`] — which kept the interpreted compiler island's
    /// own `compile-lambda` (whose captures include `llvm-builder`/
    /// `Scope<llvm-value>`) from a bootstrap paradox (JIT-compiling itself to
    /// run itself), with no special-cased "is this the compiler island" check
    /// needed: the type system alone decides. Since interp-closure removal
    /// Stage 8c the island always runs *compiled*, so its own closures are
    /// built by native code and never reach this classification (see
    /// `Self::jit_define_closure`'s own note) — the check now only guards the
    /// unreachable path, still without a special case. Reuses `ast_bridge::struct_field_kind`'s own
    /// classification (kind `0` is its catch-all "not representable" arm —
    /// every representable type has an explicit kind `1`-`6`), the same
    /// table `tagged_sym_list`/`ast_to_sexpr` already commit to for any
    /// value that actually does cross the compiled boundary.
    fn is_jit_tier_ty(&self, ty: &Type) -> bool {
        // LLVM handle types gained a compiled representation (kind `1`,
        // interp-closure removal Stage 1) so the compiler island's own
        // `defun`s can be whole-function compiled — but *closure* JIT must
        // keep excluding them: an interpreted `compile-function` run would
        // otherwise try to JIT its own `labels` siblings, recursing into
        // itself. Once the island is AOT-loaded its closures are built by
        // native code and never reach this classification at all.
        if crate::compile::ast_bridge::is_llvm_handle_ty(ty) {
            return false;
        }
        let (struct_types, enum_defs) = self.root.collect_struct_and_enum_types();
        let enum_types: HashSet<Path> = enum_defs.keys().cloned().collect();
        crate::compile::ast_bridge::struct_field_kind(ty, &struct_types, &enum_types) != 0
    }

    /// Closure unification Stage 7's definition-time JIT driver. Attempts to
    /// build `t` (an `Expr::Lambda`/`Expr::FnRef`/`Expr::MethodRef` node, or
    /// a synthetic `Expr::Lambda` wrapper `Expr::Labels` builds for one
    /// sibling — see that eval arm) as a *compiled* closure value — the only
    /// kind there is since interp-closure removal Stage 8c. `captured` is
    /// `t`'s free variables (name + declared type), `[]` for
    /// `FnRef`/`MethodRef`. Never panics itself — every failure comes back as
    /// `Err(reason)` so the caller can decide what to do with it; the failure
    /// becomes a hard error only at the eval-arm call site
    /// ([`Self::jit_closure_from_result`]), not here.
    ///
    /// The trick that avoids JIT-compiling any new self-hosted Lisp at all:
    /// `t.ty` is already `Type::Fn(params, rest, ret)`, the exact shape
    /// `compiler.rs`'s `compile-lambda` (reached by wrapping `t` as a
    /// throwaway top-level `defun`'s *entire* body — a body that is
    /// literally `(lambda ...)`, exactly what `ast_bridge::translate_lambda`
    /// already emits for a real `Expr::Lambda`) already knows how to turn
    /// into a `BoxedObj::CompiledClosure`. That throwaway "constructor"
    /// function's own *parameters* are one per name in `captured`, each
    /// declared `Type::Sexpr` regardless of its real type — not because the
    /// value is a `Sexpr`, but because `Type::Sexpr`'s `struct_field_kind`
    /// is the tagged-pointer-passthrough kind `6` (`bind-params` roots the
    /// incoming word and binds it unchanged, no decode) — exactly the
    /// existing GC cell reference (`Slot::Heap`/`Slot::TypedCell`, already
    /// established at the binding site that introduced this name — see
    /// `Self::apply`/`Expr::Let`) each argument actually *is*. When
    /// `compile-lambda`'s own outer half later builds its captured-value
    /// array from the constructor's `env` for the *inner* (real) `lambda`,
    /// it reads that exact same cell pointer back out unchanged and hands
    /// it to `rt_closure_new` — so the constructor never re-boxes anything;
    /// it exists purely to give the self-hosted compiler a `builder`/`env`
    /// to compile `(lambda ...)` *in*. Calling the constructor (an ordinary
    /// 2-argument-ABI `CompiledFn`, ABI-wise indistinguishable from any
    /// other `(compile ...)`d top-level function) is then a single
    /// `CompiledFn::call`, and its `i64` result is the finished
    /// `BoxedObj::CompiledClosure`'s own tagged encoding — built once by
    /// `rt_closure_new`, never touched by Rust at all.
    fn jit_define_closure(&self, heap: &mut Heap, env: &Env, t: &Typed, captured: &[(String, Type)]) -> Result<RtValue, JitDecline> {
        // No self-hosted compiler island loaded means there is nothing to
        // JIT *with*. Since interp-closure removal Stage 5 every `typl`
        // entry point (CLI `run_file`/REPL/`compile-module`) loads the island
        // natively (`compiler::load_aot`), so this only fires for an *embedder*
        // that builds an `Interp` and execs closure-defining code without
        // loading any island — an expected environment, not a coverage gap.
        if !self.root.fns.contains_key("compile-function") {
            return Err(JitDecline::Benign("compiler island not loaded".to_string()));
        }
        let (param_tys, ret_ty) = match &t.ty {
            Type::Fn(p, _rest, ret) => (p, ret),
            other => return Err(JitDecline::Gap(format!("internal: closure value has non-Fn type {:?}", other))),
        };
        // `&rest` needs no special JIT handling (Stage 8, retiring Stage 7's
        // blanket rejection): the checker already folded the rest parameter
        // into the node's `params` as an ordinary `(name, Sexpr)` pair
        // (`Checker::check_lambda`/`check_defun`'s shared treatment), and
        // every call site packs surplus arguments into that single `Sexpr`
        // list at *check* time (`wrap_rest_elem`/`cons_rest_list`) — so by
        // the time anything is applied, arity always matches and the rest
        // list crosses the compiled boundary like any other `Sexpr`
        // argument. `t.ty`'s `rest` field still matters to those check-time
        // call sites, just not here.
        //
        // `Unit` is a *return-position-only* allowance: `compile-unit`
        // (`compiler.rs`) compiles a `Unit`-typed body tail to a plain `0`,
        // so a `Unit`-returning closure JITs fine — but a `Unit`-typed
        // parameter or capture has no crossing encoding
        // (`struct_field_kind` kind `0`, no `encode_crossing_args` arm), so
        // those checks below stay on the plain tier test.
        if !self.is_jit_tier_ty(ret_ty) && !matches!(**ret_ty, Type::Unit) {
            return Err(JitDecline::Benign(format!("return type {:?} has no compiled representation", ret_ty)));
        }
        for ty in param_tys {
            if !self.is_jit_tier_ty(ty) {
                return Err(JitDecline::Benign(format!("parameter type {:?} has no compiled representation", ty)));
            }
        }
        for (name, ty) in captured {
            if !self.is_jit_tier_ty(ty) {
                return Err(JitDecline::Benign(format!("captured name \"{}\" has type {:?} with no compiled representation", name, ty)));
            }
        }
        // Every captured name must already be bound to a GC cell — the
        // binding-time promotion (`Self::apply`/`Expr::Let`, driven by
        // `freevars::names_captured_by_nested`) is what's supposed to
        // guarantee this; a `Slot::Native` capture here means some binding
        // site doesn't cell-ize yet (a coverage gap for Stage 8, not a
        // bug) — fail cleanly rather than panic.
        let mut cell_ids: Vec<(String, Rc<BoxId>)> = Vec::with_capacity(captured.len());
        for (name, _ty) in captured {
            match env.iter().rev().find(|(n, _)| n == name) {
                Some((_, Slot::Heap(id))) | Some((_, Slot::TypedCell(id, _))) => cell_ids.push((name.clone(), id.clone())),
                Some((_, Slot::Native(_))) => return Err(JitDecline::Gap(format!("captured name \"{}\" is not yet cell-bound", name))),
                None => return Err(JitDecline::Gap(format!("internal: captured name \"{}\" not found in env", name))),
            }
        }
        // Every `Expr::Call`/`Expr::Assoc` target `t`'s body reaches must be
        // JIT'd before the constructor's module can wire its forward
        // declaration to a real address — so a not-yet-compiled target is
        // *driven through* the same Tarjan SCC machinery `(compile name)`
        // itself uses ([`Self::compile_function`], Stage 8) rather than
        // declined: this is exactly the dependency-ordering role Stage 5
        // built that machinery for ("needed once each `labels` sibling
        // becomes an independent JIT unit" — its plan note). A target the
        // machinery *can't* compile (an unsupported construct somewhere in
        // its transitive graph) comes back as a graceful `Gap`, and the
        // definition falls back to an interpreted closure as before.
        // Filtering matches `Self::call_graph_edges`'s own exactly (see that
        // method for the rationale of each exclusion): a known
        // `rt`-shimmed/`vector-op`/`hashtable-op`/native-primitive-receiver
        // target compiles with no external symbol at all, so requiring one
        // here would reject perfectly JIT-able code.
        let call_targets: Vec<Path> = crate::compile::ast_bridge::collect_call_targets(t)
            .into_iter()
            .filter(|p| !is_rt_builtin_name(p.local()))
            .collect();
        for p in &call_targets {
            if !self.root.fn_compiled(p) {
                // The SCC machinery keys on a node name that is the callee's
                // full `::`-joined path (`Path`'s own `Display`), so a
                // module-qualified `defun` (`m::inc`) is transitively compiled
                // and stored/linked under a name distinct from a same-named
                // root function — the old "module-qualified is out of reach"
                // Stage 5 limitation is gone (interp-closure removal).
                self.compile_function(heap, &CompileTarget::Fn(Ref::synthetic(p.clone())))
                    .map_err(|e| JitDecline::Gap(format!("transitive compile of call target \"{}\" failed: {}", p, e)))?;
            }
        }
        let assoc_targets: Vec<(Path, String)> = crate::compile::ast_bridge::collect_assoc_targets(t)
            .into_iter()
            .filter(|key| {
                if key.0.local() == "vector" && matches!(key.1.as_str(), "new" | "get" | "set" | "len" | "push" | "pop") {
                    return false;
                }
                if key.0.local() == "hashtable"
                    && matches!(key.1.as_str(), "new" | "set" | "get" | "remove" | "count" | "clear" | "keys" | "values" | "entries")
                {
                    return false;
                }
                // `llvm-*`/`scope` builtin methods are natively lowered to
                // the `rt_llvm_call` dispatch shim (an `llvm-op` node,
                // interp-closure removal Stage 1), never a method-call
                // target. A *heap*-repr `Scope<V>` method has no compiled
                // lowering, but excluding it here follows the existing
                // convention for non-lowered builtins (see
                // `call_graph_edges`' comment): it panics inside
                // `compile-assoc-user`'s `get-function`, still at compile
                // time.
                if matches!(key.0.local(), "llvm-module" | "llvm-function" | "llvm-builder" | "scope") {
                    return false;
                }
                self.root.has_method(&key.0, &key.1) || !matches!(key.0.local(), "i64" | "i32" | "char" | "string" | "f64" | "bignum" | "ratio" | "sexpr")
            })
            .collect();
        for (p, m) in &assoc_targets {
            if !self.root.method_compiled(p, m) {
                // Same transitive drive as the call targets above —
                // `"type::method"` is exactly the name shape a standalone
                // `(compile "type::method")` resolves via `Self::method_key`.
                let target = CompileTarget::Method { type_name: p.clone(), method: m.clone(), home: p.parent().to_vec() };
                self.compile_function(heap, &target)
                    .map_err(|e| JitDecline::Gap(format!("transitive compile of method target \"{}::{}\" failed: {}", p, m, e)))?;
            }
        }

        let cache_key = t.loc.as_ref().map(|loc| (loc.clone(), format!("{:?}", &t.ty)));
        let cached = cache_key.as_ref().and_then(|k| self.jit_ctor_cache.borrow().get(k).cloned());
        let ctor = match cached {
            Some(c) => c,
            None => {
                let ctor = self
                    .jit_compile_closure_ctor(heap, t, captured, &call_targets, &assoc_targets)
                    .map_err(JitDecline::from_ctor_error)?;
                let ctor = Rc::new(ctor);
                if let Some(k) = cache_key {
                    self.jit_ctor_cache.borrow_mut().insert(k, ctor.clone());
                }
                self.jit_graveyard.borrow_mut().push(ctor.clone());
                ctor
            }
        };

        let int_args: Vec<i64> = cell_ids.iter().map(|(_, id)| crate::compile::runtime::encode(Value::Boxed(**id))).collect();
        // Every `CompiledFn::call` must register the active `Heap` first —
        // see `Self::call_compiled`'s matching call for why (any `rt_*` the
        // constructor's body transitively reaches, `rt_closure_new` above
        // all, needs it).
        crate::compile::runtime::set_active_heap(heap as *mut Heap);
        let raw = ctor.call(&int_args);
        Ok(RtValue::Sexpr(crate::compile::runtime::decode(raw)))
    }

    /// The actual JIT-compilation half of [`Self::jit_define_closure`] — see
    /// that method's doc comment for the "wrap `t` as a throwaway top-level
    /// function whose params are all `Type::Sexpr`" trick this builds.
    /// Returns the finished, callable [`crate::compile::CompiledFn`]; never
    /// caches or graveyards it (the caller does both, only for a fresh —
    /// not cache-hit — compile).
    fn jit_compile_closure_ctor(
        &self,
        heap: &mut Heap,
        t: &Typed,
        captured: &[(String, Type)],
        call_targets: &[Path],
        assoc_targets: &[(Path, String)],
    ) -> Result<crate::compile::CompiledFn, String> {
        let ctor_name = {
            let n = self.jit_ctor_counter.get();
            self.jit_ctor_counter.set(n + 1);
            format!("closure_ctor${}", n)
        };
        let ctor_params: Vec<(String, Type)> =
            captured.iter().map(|(name, _)| (name.clone(), Type::Named(Path::root("sexpr"), Vec::new()))).collect();
        let exclude: HashSet<String> = captured.iter().map(|(name, _)| name.clone()).collect();

        let module = {
            let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
            let module = Rc::new(RefCell::new(crate::compile::llvm_context().create_module("closure_ctor")));
            for p in call_targets {
                declare_external_function(&module, &crate::compile::ast_bridge::user_symbol_name(&p.to_string()));
            }
            for (p, m) in assoc_targets {
                declare_external_function(&module, &crate::compile::ast_bridge::user_method_symbol_name(p, m));
            }
            for (rt_name, _) in rt_extern_functions() {
                declare_external_function(&module, rt_name);
            }
            module
        };

        self.translate_and_compile(heap, module.clone(), &ctor_params, t, &ctor_name, &exclude)
            .map_err(|e| e.to_string())?;

        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        module.borrow().verify().map_err(|e| format!("module failed verification: {}", e))?;
        let mut externals: Vec<(String, usize)> = Vec::new();
        for p in call_targets {
            let f = self.root.get_fn(p).expect("checked already-compiled above");
            let addr = f.compiled.borrow().as_ref().expect("checked already-compiled above").address();
            externals.push((crate::compile::ast_bridge::user_symbol_name(&p.to_string()), addr));
        }
        for (p, m) in assoc_targets {
            let f = self.root.get_method(p, m).expect("checked already-compiled above");
            let addr = f.compiled.borrow().as_ref().expect("checked already-compiled above").address();
            externals.push((crate::compile::ast_bridge::user_method_symbol_name(p, m), addr));
        }
        externals.extend(rt_extern_functions().iter().map(|(n, a)| (n.to_string(), *a)));
        let compiled_fn = crate::compile::CompiledFn::new(&module.borrow(), &ctor_name, &externals).map_err(|e| format!("JIT failed: {}", e))?;
        Ok(compiled_fn)
    }

    /// The shared policy every `Expr::Lambda`/`Labels`/`FnRef`/`MethodRef`
    /// eval arm applies around [`Self::jit_define_closure`]: on success, use
    /// the compiled closure; on *any* decline, a hard [`EvalError::Panic`].
    /// Interp-closure removal Stage 8c deleted the interpreted-closure
    /// fallback (`make_closure`) this used to take, so there is nothing to
    /// fall back *to* anymore — every closure value is a compiled one. A
    /// [`JitDecline::Gap`] was already a hard error (Stage 9); the two
    /// remaining [`JitDecline::Benign`] cases (a native-tier type, the island
    /// not loaded) are both unreachable under the always-loaded AOT-native
    /// island every entry point installs (Stage 5), so reaching one is a real
    /// error — an embedder that skipped island loading, say — not a silent
    /// downgrade. `EvalError::Panic` is a catchable interpreter error (not a
    /// process abort), so a caller like `Interp::expand_macro` still surfaces
    /// it as a diagnostic rather than crashing.
    fn jit_closure(
        &self,
        heap: &mut Heap,
        env: &Env,
        t: &Typed,
        captured: &[(String, Type)],
    ) -> Result<RtValue, EvalError> {
        let result = self.jit_define_closure(heap, env, t, captured);
        self.jit_closure_from_result(result)
    }

    /// [`Self::jit_closure`]'s policy half, taking an already-computed
    /// [`Self::jit_define_closure`] result rather than the inputs to compute
    /// one — for a caller (`Expr::Lambda`/`Expr::Labels`' eval arms) that
    /// builds the `Typed`/captured inputs itself before invoking the JIT.
    fn jit_closure_from_result(&self, result: Result<RtValue, JitDecline>) -> Result<RtValue, EvalError> {
        match result {
            Ok(v) => Ok(v),
            Err(decline) => Err(EvalError::Panic(format!("definition-time JIT failed: {}", decline.reason()))),
        }
    }

    /// The recursive core of [`Self::heap_repr_kind`] — see that method's
    /// doc comment. `seen` is threaded through unchanged from
    /// [`Self::enum_fields_representable`]'s own doc comment (the
    /// self-/mutually-referential `defenum` guard).
    fn is_heap_repr_ty(&self, ty: &Type, seen: &mut HashSet<Path>) -> bool {
        match ty {
            Type::Named(p, _) if is_sexpr_type(p) || *p == Path::root("hashtable") || matches!(self.root.find_type(p), Some(scope::TypeEntry::Struct)) => true,
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
        *p == option_path() || *p == result_path() || *p == Path::root("error") || matches!(self.root.find_type(p), Some(scope::TypeEntry::Enum(_)))
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
        } else if let Some(scope::TypeEntry::Enum(def)) = self.root.find_type(name) {
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
    /// Every currently-registered top-level function's *local* name, in no
    /// particular order — for tooling and tests that need to enumerate what
    /// a given load produced (e.g. deriving the compiler island's own
    /// `defun` set by diffing a prelude-only interpreter against one that
    /// also ran `load_compiler`; interp-closure removal Stage 2 onward).
    pub fn function_names(&self) -> Vec<String> {
        self.root.all_fn_names()
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
            if embedded != crate::fasl::source_hash(crate::compiler::SOURCE) {
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
            .filter(|n| check_hash || module.get_function(&crate::compile::ast_bridge::user_symbol_name(n)).is_some())
            .cloned()
            .collect();
        let internal_names: Vec<String> =
            names.iter().map(|n| crate::compile::ast_bridge::user_symbol_name(n)).collect();
        // Wire only the `rt_*` shims the module actually forward-declares. A
        // `check_hash` load has a fresh `.bc` that declares exactly the current
        // set, but the bootstrap snapshot chain installs the *committed* (older)
        // `.bc`: if this generation added a new shim (e.g. `rt_gensym`, made to
        // compile `gensym` in macro-expansion lambdas), that older module never
        // declared it — and never called it either, so skipping its mapping is
        // correct, whereas passing it to `new_multi` would fail its "no forward
        // declaration" guard.
        let externals: Vec<(String, usize)> = rt_extern_functions()
            .iter()
            .filter(|(n, _)| check_hash || module.get_function(n).is_some())
            .map(|(n, addr)| (n.to_string(), *addr))
            .collect();
        let compiled_fns = crate::compile::CompiledFn::new_multi(&module, &internal_names, &externals)
            .map_err(|e| format!("compiler island JIT install failed: {}", e))?;

        for (name, cf) in names.iter().zip(compiled_fns) {
            let f = self.root.get_fn(&Path::root(name)).expect("island defun already registered by exec'ing SOURCE");
            *f.compiled.borrow_mut() = Some(Rc::new(cf));
        }
        Ok(())
    }

    pub fn exec(&mut self, heap: &mut Heap, tl: TopLevel) -> Result<Option<RtValue>, EvalError> {
        match tl {
            TopLevel::Defun { name, type_params, params, ret, body, public } => {
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
                let def = FnDef { params: names, kinds, body, rest: false, sig: Some((types, ret)), public, compiled: RefCell::new(None) };
                self.root.get_or_create(name.parent()).fns.insert(name.local().to_string(), Rc::new(def));
                Ok(None)
            }
            TopLevel::Defmethod { type_name, method, self_name, params, ret, body, type_params, public, .. } => {
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
                let def = FnDef { params: names, kinds, body, rest: false, sig: Some((types, ret)), public, compiled: RefCell::new(None) };
                self.root.get_or_create(type_name.parent()).methods.insert((type_name.local().to_string(), method), Rc::new(def));
                Ok(None)
            }
            TopLevel::Defmacro { name, params, body, rest, public } => {
                // A macro's body is callable exactly like a `defun`'s — see
                // `MacroExpander`/`Self::expand_macro` — so it's stored in
                // the very same `fns` table; no separate macro table exists.
                // Every macro parameter is `Sexpr` by definition, hence all
                // `Heap` slots.
                let kinds = vec![SlotKind::Heap; params.len()];
                let def = FnDef { params, kinds, body, rest, sig: None, public, compiled: RefCell::new(None) };
                self.root.get_or_create(name.parent()).fns.insert(name.local().to_string(), Rc::new(def));
                Ok(None)
            }
            TopLevel::Use { .. } => Ok(None),
            // The type itself was already registered in the checker's
            // `Registry` at check time — there's nothing else for the
            // interpreter to do, the same as `Option`/`Result` needing no
            // runtime registration of their own. Recording `name` as a
            // `TypeEntry::Struct` is the one exception (Stage 3 of the
            // Sexpr/RtValue unification plan, `docs/implementation-log.md`
            // — see `scope::TypeEntry`'s doc comment).
            TopLevel::Defstruct { name } => {
                self.root.get_or_create(name.parent()).types.insert(name.local().to_string(), scope::TypeEntry::Struct);
                Ok(None)
            }
            // A `defenum` sum type is a check-time registration, like
            // `Option`/`Result`. Unlike `Defstruct` it is *not* recorded as a
            // `TypeEntry::Struct` (an enum instance is an immutable
            // `RtValue::Data`, never a boxed struct) — but its variants'
            // field types *are* recorded, as `TypeEntry::Enum`: the
            // compiled-global boundary needs them to decode a box back into a
            // `RtValue::Data` (see `scope::TypeEntry`'s doc comment). The same
            // one-exception pattern `Defstruct` follows.
            TopLevel::Defenum { name, params, variants } => {
                self.root.get_or_create(name.parent()).types.insert(name.local().to_string(), scope::TypeEntry::Enum(EnumDef { params, variants }));
                Ok(None)
            }
            TopLevel::Defvar { name, ty, value, public, .. } => {
                let v = self.eval(heap, &value, &Env::new())?;
                let kind = self.heap_repr_kind(&ty);
                let slot = self.slot(heap, kind, v)?;
                self.root.get_or_create(name.parent()).globals.insert(name.local().to_string(), scope::GlobalDef { slot, public });
                Ok(None)
            }
            TopLevel::Module { path, body } => {
                // Ensures the tree node for `path` exists (idempotent — a
                // "mkdir -p") before its own body registers into it; doesn't
                // need to *stay* "current" for anything else, since every
                // nested `Defun`/`Defmethod`/`Defvar`/`Defmacro`'s own `name`/
                // `type_name` is already absolute regardless of nesting (see
                // `scope`'s module doc comment) — registration is direct
                // descent from `self.root`, never relative to a live "current
                // module" cursor. This also means a `Defmacro` already
                // exec'd immediately (`project::needs_immediate_exec`, before
                // this `Module` wrapper was even built) lands at the exact
                // same node here, with no ordering dependency.
                self.root.get_or_create(path.segments());
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
        self.root.resolve_fn(&r.home, &r.written).or_else(|| self.root.get_fn(&r.resolved))
    }

    /// [`Self::resolve_fn_ref`]'s twin for `Global`/`SetGlobal`.
    fn resolve_global_ref(&self, r: &Ref) -> Option<Slot> {
        self.root.resolve_global(&r.home, &r.written).or_else(|| self.root.get_global(&r.resolved))
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
            Expr::Global(r) => {
                if let Some(&id) = self.compiled_globals.borrow().get(&r.resolved) {
                    // `id` (`Self::promote_global`'s return value) is *not*
                    // the `Heap::permanent_root` position — see
                    // `typelisp_rt::global_new`'s doc comment (an
                    // `Option`/`Result` global's own heap-referencing field
                    // pushes its own extra permanent root during encoding,
                    // desyncing the two) — `global_perm_idx` resolves it the
                    // same way `rt_global_get` does.
                    let perm_idx = crate::compile::runtime::global_perm_idx(id)
                        .ok_or_else(|| EvalError::Internal(format!("global \"{}\": unknown compiled id {}", r.resolved, id)))?;
                    Ok(decode_field_typed(heap, heap.permanent_root(perm_idx), &t.ty))
                } else {
                    self.resolve_global_ref(r)
                        .map(|s| s.get(heap))
                        .ok_or_else(|| EvalError::Unbound(r.resolved.to_string()))
                }
            }
            Expr::FnRef(r) => match self.resolve_fn_ref(r) {
                // Reify a user function as a closure with no captured
                // environment. No captures means no cell-binding prerequisite
                // at all, the simplest possible JIT (`Self::jit_closure`'s doc
                // comment covers the mandatory-JIT policy shared by all four
                // call sites since interp-closure removal Stage 8c).
                Some(_) => self.jit_closure(heap, env, t, &[]),
                // Otherwise a built-in operator (lives at the root, simple path).
                None => Ok(RtValue::Builtin(r.resolved.local().to_string())),
            },
            Expr::MethodRef { type_name, method, home, .. } => match self.root.resolve_method(home, type_name, method) {
                Some(_) => self.jit_closure(heap, env, t, &[]),
                None => Ok(RtValue::BuiltinMethod(type_name.clone(), method.clone())),
            },
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
                // Closure unification Stage 7: same capture-cell promotion
                // as `Self::apply` — see that method's matching comment,
                // including the `is_jit_tier_ty` gate (a `let`-bound
                // `llvm-*`/native-`Scope<V>` value inside the self-hosted
                // compiler's own body is just as captured-by-a-nested-
                // closure, by the same walk, as an ordinary user binding).
                let cell_names = crate::compile::freevars::names_captured_by_nested(body);
                let mut child = env.clone();
                for (name, val) in binds {
                    let v = self.eval(heap, val, env)?;
                    let kind = self.heap_repr_kind(&val.ty);
                    let s = if kind == SlotKind::Native && cell_names.contains(name) && self.is_jit_tier_ty(&val.ty) {
                        self.typed_cell_slot(heap, val.ty.clone(), v)?
                    } else {
                        self.slot(heap, kind, v)?
                    };
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
                    // Closure unification Stage 7: each sibling is JIT'd
                    // *independently* (no SCC/shared-module batching,
                    // unlike `compiler.rs`'s own `compile-labels-bodies`,
                    // which can only batch siblings compiled as part of the
                    // *same* already-compiled outer function) — a
                    // sibling-to-sibling call, including self-recursion,
                    // is just an ordinary captured-name reference (this
                    // block's own placeholder cells, above), reached
                    // through the same `apply-indirect` path any other
                    // escaping closure call uses. `lambda_free_vars` (not
                    // `labels_free_vars`) is deliberate: the latter treats
                    // sibling names as *direct calls* to exclude from the
                    // captured list — correct only when every sibling
                    // compiles into one shared module/`fn-env`, which
                    // Stage 7's one-sibling-at-a-time JIT never does — so
                    // every sibling name a body references must flow
                    // through `captured`/the cell mechanism like any other
                    // free variable, exactly what `lambda_free_vars` (whose
                    // own doc comment notes it walks with an empty
                    // siblings set) already does.
                    // Cheap (`O(params)`, no body walk) pre-check before
                    // `lambda_free_vars` — see `Expr::Lambda`'s matching
                    // comment for why this ordering matters: the self-
                    // hosted compiler's own `labels` siblings (`compile-
                    // value` above all) always declare an `llvm-builder`/
                    // `Scope<llvm-value>`/... param, so this alone already
                    // excludes them before ever recursively walking a body
                    // the size of that dispatcher.
                    let ret_ty = fbody.last().expect("a labels def's body has at least one expression").ty.clone();
                    // `Unit` return allowed for the same reason as
                    // `jit_define_closure`'s own return-type check — see
                    // the comment there.
                    let jit_worth_trying = (self.is_jit_tier_ty(&ret_ty) || matches!(ret_ty, Type::Unit))
                        && params.iter().all(|(_, ty)| self.is_jit_tier_ty(ty));
                    let jit_result = if jit_worth_trying {
                        let captured = crate::compile::freevars::lambda_free_vars(params, fbody);
                        let fn_ty = Type::Fn(params.iter().map(|(_, ty)| ty.clone()).collect(), None, Box::new(ret_ty));
                        let synthetic = Typed::new(Expr::Lambda { params: params.clone(), body: fbody.clone() }, fn_ty);
                        self.jit_define_closure(heap, &child, &synthetic, &captured)
                    } else {
                        Err(JitDecline::Benign("a declared parameter or the return type has no compiled representation".to_string()))
                    };
                    let closure = self.jit_closure_from_result(jit_result)?;
                    slot.set(heap, closure)?;
                }
                self.eval_seq(heap, body, &child)
            }
            Expr::Call(r, args) => {
                let (argv, _slots) = self.eval_args(heap, args, env)?;
                match self.resolve_fn_ref(r) {
                    Some(f) => {
                        // A `(compile name)`d function dispatches to native
                        // code first — checked ahead of the tree-walked body
                        // so a later recompile (not possible yet, but the
                        // ordering is the cheap-to-get-right choice) would
                        // naturally take precedence.
                        let compiled = f.compiled.borrow().clone();
                        if let Some(compiled) = compiled {
                            // `compile_function` never populates `compiled`
                            // without first going through `compiled_fn_body`,
                            // which requires `f.sig` to be `Some` — so this
                            // is an internal invariant, not a user-reachable
                            // error.
                            let ret_ty = &f.sig.as_ref().expect("a compiled function always has a type signature").1;
                            return self.call_compiled(heap, &compiled, &argv, ret_ty);
                        }
                        self.apply(heap, &f, argv)
                    }
                    None if r.written.len() == 1 => match self.eval_builtin(heap, &r.written[0], &argv) {
                        Some(result) => result,
                        None => Err(EvalError::NoSuchFunction(r.resolved.to_string())),
                    },
                    None => Err(EvalError::NoSuchFunction(r.resolved.to_string())),
                }
            }
            Expr::Assoc { type_name, method, args, home, .. } => {
                let (argv, _slots) = self.eval_args(heap, args, env)?;
                match self.root.resolve_method(home, type_name, method) {
                    Some(f) => {
                        // `Expr::Call`'s own `compiled` fast-path check above
                        // — see `Self::call_compiled`'s doc comment for the
                        // one extra risk a method's receiver carries that a
                        // plain function's parameters never do.
                        let compiled = f.compiled.borrow().clone();
                        if let Some(compiled) = compiled {
                            let ret_ty = &f.sig.as_ref().expect("a compiled method always has a type signature").1;
                            return self.call_compiled(heap, &compiled, &argv, ret_ty);
                        }
                        self.apply(heap, &f, argv)
                    }
                    None => {
                        let recv_ty = args.first().map(|a| &a.ty);
                        match eval_builtin_method(self, heap, type_name, method, recv_ty, &argv, &t.ty) {
                            Some(result) => result,
                            None => Err(EvalError::NoSuchFunction(method_link_name(type_name, method))),
                        }
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
                // Closure unification Stage 7: try definition-time JIT
                // first — but only bother computing `captured` (a full
                // recursive walk of `body`, `freevars::lambda_free_vars`) if
                // every *declared* param already passes the tier check —
                // this shallow, O(params) test is what actually excludes
                // the self-hosted compiler's own `lambda`/`labels` (whose
                // params always include an `llvm-builder`/`Scope<llvm-
                // value>`/... — see `Self::is_jit_tier_ty`'s doc comment)
                // *before* ever descending into a body that, for something
                // the size of `compiler.rs`'s own `compile-value`
                // dispatcher, is deep and wide enough to overflow the stack
                // on a plain recursive walk purely to compute a captured
                // list that was always going to fail tier anyway.
                let jit_worth_trying = params.iter().all(|(_, ty)| self.is_jit_tier_ty(ty));
                let jit_result = if jit_worth_trying {
                    let captured = crate::compile::freevars::lambda_free_vars(params, body);
                    self.jit_define_closure(heap, env, t, &captured)
                } else {
                    Err(JitDecline::Benign("a declared parameter has no compiled representation".to_string()))
                };
                self.jit_closure_from_result(jit_result)
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
                    // A closure callee is always a *compiled* closure now
                    // (interp-closure removal Stage 8c deleted the interpreted
                    // `BoxedObj::Closure` arm that used to precede this one):
                    // every `lambda`/`labels`/`FnRef`/`MethodRef` value is
                    // built by definition-time JIT. Reached whenever this
                    // `Apply`'s callee was produced by compiled code — a
                    // compiled function returning a `Fn` (decoded via
                    // `is_boxed_sexpr_type`'s `Type::Fn` arm), or a compiled
                    // closure the interpreter is threading through a chain of
                    // `Expr::Apply`s it's driving (e.g. `((make-adder n) x)`
                    // where `make-adder` is `compile`d). Marshals `argv`/
                    // decodes the result exactly like a top-level
                    // `call_compiled` call, via the same two halves that split
                    // out of it.
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
                            None => Err(EvalError::NoSuchFunction(method_link_name(&type_name, &method))),
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
            Expr::SetGlobal(r, value) => {
                let v = self.eval(heap, value, env)?;
                if let Some(&id) = self.compiled_globals.borrow().get(&r.resolved) {
                    // See `Expr::Global`'s arm for why `id` needs resolving
                    // through `global_perm_idx` rather than being used as
                    // the `Heap::permanent_root` position directly.
                    let perm_idx = crate::compile::runtime::global_perm_idx(id)
                        .ok_or_else(|| EvalError::Internal(format!("global \"{}\": unknown compiled id {}", r.resolved, id)))?;
                    let mv = rtvalue_to_struct_field(heap, &v)?;
                    heap.set_permanent_root(perm_idx, mv);
                    Ok(v)
                } else {
                    let cell = self.resolve_global_ref(r).ok_or_else(|| EvalError::Unbound(r.resolved.to_string()))?;
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
            Expr::CompileFn(target) => self.compile_function(heap, target),
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
            // `(Path segs)` where `segs : Sexpr` is a proper list of `sym`s —
            // walks it into `Vec<SymId>` and re-interns, the inverse of
            // `match_sexpr_ctor`'s `SEXPR_PATH` arm, which builds that same
            // list fresh from an existing `Value::Path`'s interned segments.
            SEXPR_PATH => {
                let ids = sexpr_list_to_symbols(heap, rt_sexpr(&vs[0])?)?;
                heap.intern_path(&ids)
            }
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
        // Closure unification Stage 7: a param this body's own nested
        // `lambda`/`labels` captures must be a GC cell (`Slot::TypedCell`)
        // even when its declared type would otherwise route it `Native` —
        // see `Slot::TypedCell`'s doc comment. Recomputed per call for now
        // (correctness first, matching Stage 4's own "全捕獲セル化" choice);
        // caching this per-`FnDef` is a follow-up optimization.
        //
        // Gated on `is_jit_tier_ty` too: `names_captured_by_nested` doesn't
        // know or care whether a capture is JIT-representable — it fires
        // just as readily for the self-hosted compiler's own `compile-
        // function` (`m: llvm-module`, captured by `compile-lambda` and
        // friends within its own `labels` body) as for ordinary user code.
        // `rtvalue_to_struct_field` has no encoding for an `RtValue::
        // LlvmModule`/`LlvmBuilder`/... (by design — see that function's
        // doc comment), so cell-boxing one would be an immediate internal
        // error for every single `(compile ...)` call. A capture that can
        // never cross into compiled code can also never be read by a
        // *compiled* closure, so it has no reason to be a GC cell at all —
        // `Slot::Native` (this binding's existing, correct behavior) is
        // exactly right for it, `Self::jit_define_closure`'s own tier check
        // will reject any closure trying to capture it either way.
        let cell_names = crate::compile::freevars::names_captured_by_nested(&def.body);
        let mut env: Env = Vec::with_capacity(args.len());
        for (i, ((name, kind), v)) in def.params.iter().zip(def.kinds.iter()).zip(args).enumerate() {
            let ty = def.sig.as_ref().map(|(ptys, _)| ptys[i].clone());
            let s = if *kind == SlotKind::Native && cell_names.contains(name) && ty.as_ref().is_some_and(|ty| self.is_jit_tier_ty(ty)) {
                match ty {
                    Some(ty) => self.typed_cell_slot(heap, ty, v)?,
                    None => self.slot(heap, *kind, v)?,
                }
            } else {
                self.slot(heap, *kind, v)?
            };
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
        // Root every already-heap-resident `Sexpr` argument up front, before
        // the encode loop below allocates anything. A later argument's
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
        // separate pass is the whole fix; the encode loop then just skips
        // re-rooting `Sexpr`s.
        for v in argv {
            if let RtValue::Sexpr(sv) = v {
                heap.push_root(*sv);
                crossing_roots += 1;
            }
        }
        for v in argv {
            let encoded = match v {
                // A closure crossing into compiled code is always a
                // `BoxedObj::CompiledClosure` now (interp-closure removal
                // Stage 8c deleted `BoxedObj::Closure`), a tagged `Sexpr` like
                // any other boxed value — so it just falls through to the
                // ordinary `RtValue::Sexpr` arm; compiled code on both sides
                // already agrees on its shape. Already rooted by the pre-pass
                // above, so this only encodes.
                RtValue::Sexpr(sv) => Ok(crate::compile::runtime::encode(*sv)),
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
                // A Rust-native LLVM value / native scope crosses as an
                // LLVM handle: a raw untraced registry index (interp-closure
                // removal Stage 1 — `ast_bridge::is_llvm_handle_ty`'s
                // integer-kind representation). The registry entry keeps the
                // `Rc`/`Copy` payload alive for compiled code to hand back
                // through `rt_llvm_call`/the return decode; no GC root, the
                // registry itself is the owner.
                RtValue::LlvmModule(_)
                | RtValue::LlvmBuilder(_)
                | RtValue::LlvmFunction(_)
                | RtValue::LlvmBasicBlock(_)
                | RtValue::LlvmValue(_)
                | RtValue::Scope(_) => Ok(llvm_handle_register(v.clone())),
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
        // An LLVM-handle-typed result (interp-closure removal Stage 1) is a
        // raw registry index — resolve it back to the Rust-native
        // `RtValue::Llvm*`/`Scope` the interpreter works with, the exact
        // inverse of `encode_crossing_args`' handle registration. Checked
        // before every other arm since these are `Type::Named` and would
        // otherwise be misread by `is_boxed_sexpr_type`'s catch-all or fall
        // through to the bare `RtValue::Int`.
        if crate::compile::ast_bridge::is_llvm_handle_ty(ret_ty) {
            return match llvm_handle_get(raw) {
                Some(v) => Ok(v),
                None => Err(EvalError::Internal(format!("compiled call returned dangling llvm handle {}", raw))),
            };
        }
        Ok(if self.is_boxed_sexpr_type(ret_ty) {
            RtValue::Sexpr(crate::compile::runtime::decode(raw))
        } else if matches!(ret_ty, Type::Unit) {
            // A `Unit`-typed body compiles to a plain `0` (`compile-unit`) —
            // decode it back to the real `RtValue::Unit` rather than
            // surfacing the raw word as a bogus `RtValue::Int(0)` (the
            // pre-Stage-8 fallthrough this arm replaces), so a
            // `Unit`-returning compiled function/closure interoperates with
            // interpreted code exactly like an interpreted one.
            RtValue::Unit
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
        } else if matches!(ret_ty, Type::Str) {
            // A compiled `string` value is a tagged `Value::Str` word
            // (`Type::Str`'s passthrough kind); `Type::Str` isn't
            // `Type::Named`, so `is_boxed_sexpr_type` above never catches it
            // — without this arm the tagged pointer fell through to the
            // bare `RtValue::Int` case below (the formerly-documented gap
            // `compile_of_a_function_referencing_a_str_option_global_...`'s
            // doc comment used to note). Decoded to an interp-side
            // `RtValue::Str` exactly like the `sexpr-str` builtin does.
            match crate::compile::runtime::decode(raw) {
                Value::Str(id) => RtValue::Str(heap.string(id).into()),
                other => {
                    return Err(EvalError::Internal(format!(
                        "compiled call returned {:?} for a string result, which is not a Str",
                        other
                    )))
                }
            }
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
        } else if matches!(ret_ty, Type::Symbol) {
            // `Type::Symbol` isn't `Type::Named` either, so `is_boxed_sexpr_type`
            // never catches it — without this arm a compiled `Symbol` result
            // (the tagged `Value::Symbol` word `compile-sexpr-field`'s `sym`
            // passthrough / `compile-construct-sexpr`'s `rt_intern_symbol`
            // already produce) would silently misdecode as a plain
            // `RtValue::Int` below. A `Symbol`-typed interp value is always
            // carried generically as `RtValue::Sexpr(Value::Symbol(_))` (see
            // `match_sexpr_ctor`'s own `SEXPR_SYM` arm's doc comment) — which
            // is also exactly why *argument* encoding needed no matching fix:
            // `encode_crossing_args`' `RtValue::Sexpr(sv) => encode(*sv)` arm
            // already covers a `Symbol` argument for free.
            match crate::compile::runtime::decode(raw) {
                v @ Value::Symbol(_) => RtValue::Sexpr(v),
                other => {
                    return Err(EvalError::Internal(format!(
                        "compiled call returned {:?} for a Symbol result, which is not a Symbol",
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
        matches!(ty, Type::Fn(..))
            || matches!(ty, Type::Named(p, _) if is_sexpr_type(p) || matches!(self.root.find_type(p), Some(scope::TypeEntry::Struct)) || self.is_enum_path(p))
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
    /// `ast_bridge.rs`'s `Expr::Assoc`/`Expr::MethodRef` translation and
    /// `user_method_symbol_name`'s mangled LLVM symbol both embed the type's
    /// full path too, not just its local segment). Shared by
    /// [`Self::resolve_fn_def`] (the lookup) and [`Self::compile_function`]
    /// (which `(compile name)` for a method) — both need the exact same
    /// `(Path, String)` key.
    fn method_key(&self, name: &str) -> Option<(Path, String)> {
        let (type_part, method) = name.rsplit_once("::")?;
        let type_path = Path::from_segments(type_part.split("::").map(|s| s.to_string()).collect());
        self.root.get_method(&type_path, method)?;
        Some((type_path, method.to_string()))
    }

    /// Resolves a `(compile name)` argument against either the scope tree's
    /// free functions (a plain name, a top-level `defun`) or its methods (a
    /// `"type::method"` name, an instance/static `defmethod` — including a `defstruct`'s
    /// auto-generated field accessor/setter, whose body is the `Expr::FieldGet`/
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
                return self.root.get_method(&type_path, &method).ok_or_else(|| EvalError::NoSuchFunction(name.to_string()));
            }
            return self
                .root
                .get_fn(&fn_path_from_node_name(name))
                .ok_or_else(|| EvalError::NoSuchFunction(name.to_string()));
        }
        self.root.get_fn(&Path::root(name)).ok_or_else(|| EvalError::NoSuchFunction(name.to_string()))
    }

    /// Looks up `name`'s registered `defun`/`defmethod` body (see
    /// [`Self::resolve_fn_def`]), enforcing the one constraint every entry
    /// point into the compiler shares: a real type signature (so an LLVM
    /// function type can be built — never set for a `defmacro`). A multi-
    /// expression body is collapsed to one `Typed` via
    /// [`crate::compile::ast_bridge::single_body_expr`] (labels/closures
    /// Stage 6 — see that function's doc comment for why this needs no
    /// `compiler.rs` change). For an instance method, `params`/`sig.0`
    /// already carry the receiver as element `0` (`Checker::check_defmethod`
    /// pushes the receiver's own type onto `sig_params` before the method's
    /// declared parameters) — so it flows through exactly like any other
    /// parameter here, no special-casing needed. Shared by
    /// [`Self::add_compiled_function`] (the actual AST-bridge step) and
    /// [`Self::compile_function`]/[`Self::call_graph_edges`] (which need the
    /// body slightly earlier — to collect `Expr::Call` targets — before
    /// `add_compiled_function` ever runs).
    fn compiled_fn_body(&self, name: &str) -> Result<(Vec<(String, Type)>, Typed), EvalError> {
        let f = self.resolve_fn_def(name)?;
        let sig = f
            .sig
            .as_ref()
            .ok_or_else(|| EvalError::Panic(format!("compile: \"{}\" has no type signature (is it a defmacro?)", name)))?;
        let params = f.params.iter().cloned().zip(sig.0.iter().cloned()).collect();
        Ok((params, crate::compile::ast_bridge::single_body_expr(&f.body)))
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
            .root
            .get_global(path)
            .ok_or_else(|| EvalError::Internal(format!("compile: global \"{}\" is not defined", path)))?;
        let v = slot.get(heap);
        let value = rtvalue_to_struct_field(heap, &v).map_err(|_| {
            EvalError::Panic(format!("compile: global \"{}\" has a type not yet supported for compiled access", path))
        })?;
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
        params: &[(String, Type)],
        body: &Typed,
        internal_name: &str,
        extra_exclude_from_cell_names: &HashSet<String>,
    ) -> Result<(), EvalError> {
        // Every global this body reads/assigns must have a compiled-global
        // slot before translation starts — `ast_to_sexpr` looks each one up
        // by id, not by name (see `Ctx::globals`'s doc comment), so there is
        // nothing to resolve lazily mid-translation the way `compile-call`'s
        // `get-function` can for an ordinary function name.
        for target in crate::compile::ast_bridge::collect_global_targets(body) {
            self.promote_global(heap, &target)?;
        }
        let compiled_globals = self.compiled_globals.borrow();
        // `ast_bridge` is deliberately `Registry`-free, so hand it the
        // struct/enum classification as a plain flattened snapshot of the
        // scope tree. Collected fresh per compilation — compiling is rare
        // enough that keeping a second always-current copy isn't worth it.
        let (struct_types, enum_defs) = self.root.collect_struct_and_enum_types();
        let enum_types: HashSet<Path> = enum_defs.keys().cloned().collect();

        // A top-level `defun` has no enclosing lexical scope to capture
        // *from*, so its own `cell_names` (closure-representation
        // unification, Stage 4) is exactly its own params/`let`-bindings
        // that some nested `lambda`/`labels` in `body` captures — see
        // `ast_bridge::names_captured_by_nested`'s doc comment.
        let mut cell_names = crate::compile::freevars::names_captured_by_nested(std::slice::from_ref(body));
        for n in extra_exclude_from_cell_names {
            cell_names.remove(n);
        }

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
        let param_list = match crate::compile::ast_bridge::tagged_sym_list(heap, params, &struct_types, &enum_types, &cell_names) {
            Ok(v) => v,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        heap.push_root(param_list);
        let body_sexpr =
            match crate::compile::ast_bridge::ast_to_sexpr(heap, body, &struct_types, &enum_types, &compiled_globals, &cell_names) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root(); // param_list
                    return Err(EvalError::Panic(e.to_string()));
                }
            };
        heap.pop_root(); // param_list

        self.run_compile_function(heap, module, internal_name, param_list, body_sexpr)
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
            RtValue::LlvmModule(module),
            RtValue::Str(internal_name.into()),
            RtValue::Sexpr(param_list),
            RtValue::Sexpr(body_sexpr),
        ];
        let compiler_def = self.root.get_fn(&compiler_path).ok_or_else(|| {
            EvalError::Internal("compile: compiler body not loaded — call load_compiler first".into())
        })?;
        let compiled = compiler_def.compiled.borrow().clone();
        if let Some(cf) = compiled {
            let ret_ty = Type::Named(Path::root("llvm-module"), Vec::new());
            let mark = llvm_handles_mark();
            let r = self.call_compiled(heap, &cf, &argv, &ret_ty);
            llvm_handles_release(mark);
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
        // See `add_compiled_function`'s own copy of this for why the
        // struct/enum classification crosses as a per-compilation snapshot.
        let (struct_types, enum_defs) = self.root.collect_struct_and_enum_types();
        let enum_types: HashSet<Path> = enum_defs.keys().cloned().collect();

        let param_list = match crate::compile::ast_bridge::tagged_sym_list(heap, &[], &struct_types, &enum_types, &HashSet::new()) {
            Ok(v) => v,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        heap.push_root(param_list);
        let body_sexpr =
            match crate::compile::ast_bridge::ast_to_sexpr_for_global_init(heap, value, &struct_types, &enum_types, &compiled_globals) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root(); // param_list
                    return Err(EvalError::Panic(e.to_string()));
                }
            };
        heap.pop_root(); // param_list

        self.run_compile_function(heap, module, internal_name, param_list, body_sexpr)
    }

    /// `(compile fn-name)` (or `(compile type::method)`): JIT-compiles a
    /// previously-defined `defun`/`defmethod` and marks the target `FnDef`
    /// node compiled (see `FnDef::compiled`) so `Expr::Call`/`Expr::Assoc`
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
    /// (`crate::compile::ast_bridge::collect_call_targets`/
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
    fn compile_function(&self, heap: &mut Heap, target: &CompileTarget) -> Result<RtValue, EvalError> {
        let (name, already_compiled) = match target {
            CompileTarget::Fn(r) => {
                self.resolve_fn_ref(r).ok_or_else(|| EvalError::NoSuchFunction(r.written.join("::")))?;
                (r.resolved.to_string(), self.root.fn_compiled(&r.resolved))
            }
            CompileTarget::Method { type_name, method, home } => {
                self.root
                    .resolve_method(home, type_name, method)
                    .ok_or_else(|| EvalError::NoSuchFunction(method_link_name(type_name, method)))?;
                (method_link_name(type_name, method), self.root.method_compiled(type_name, method))
            }
        };
        if already_compiled {
            return Ok(RtValue::Bool(true));
        }
        for scc in self.compute_sccs(&name)? {
            self.compile_scc(heap, &scc)?;
        }
        Ok(RtValue::Bool(true))
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
    fn call_graph_edges(&self, name: &str) -> Result<Vec<CallEdge>, EvalError> {
        let path = fn_path_from_node_name(name);
        let method_key = self.method_key(name);
        let (_, body) = self.compiled_fn_body(name)?;
        let mut edges = Vec::new();

        edges.extend(
            crate::compile::ast_bridge::collect_call_targets(&body)
                .into_iter()
                .filter(|p| *p != path && !is_rt_builtin_name(p.local()))
                .map(CallEdge::Fn),
        );

        let method_targets: Vec<(Path, String)> = crate::compile::ast_bridge::collect_assoc_targets(&body)
            .into_iter()
            .filter(|key| method_key.as_ref() != Some(key))
            .filter(|key| {
                // `Vector<T>`'s field-backed builtin methods (`new`/`get`/
                // `set`/`len`/`push`/`pop`) are lowered to a `vector-op` node
                // (`ast_bridge::translate_vector_method` -> `rt_struct_*`),
                // not a method call, so — like the native primitive methods
                // below — they are never a real call target. `vector::iter`
                // is deliberately excluded from this list: it is a genuine
                // prelude `defmethod` (`vector-iter::new`) and must be
                // `compile`d like any other method.
                if key.0.local() == "vector" && matches!(key.1.as_str(), "new" | "get" | "set" | "len" | "push" | "pop") {
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
                // `llvm-*`/`scope` builtin methods are natively lowered to
                // the `rt_llvm_call` dispatch shim (an `llvm-op` node,
                // interp-closure removal Stage 1) — like `vector-op`/
                // `hashtable-op` above, never a real call target. A
                // heap-repr `Scope<V>` method has no compiled lowering and
                // panics inside `compile-assoc-user`'s `get-function`
                // instead, per the convention in the next comment.
                if matches!(key.0.local(), "llvm-module" | "llvm-function" | "llvm-builder" | "scope") {
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
                self.root.has_method(&key.0, &key.1)
                    || !matches!(key.0.local(), "i64" | "i32" | "char" | "string" | "f64" | "bignum" | "ratio" | "sexpr")
                    || !is_native_lowered_primitive_method(key.0.local(), &key.1)
            })
            .collect();
        for (type_name, method) in &method_targets {
            if !self.root.has_method(type_name, method) {
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
    fn compute_sccs(&self, name: &str) -> Result<Vec<Vec<String>>, EvalError> {
        let mut counter = 0usize;
        let mut indices: HashMap<String, usize> = HashMap::new();
        let mut lowlink: HashMap<String, usize> = HashMap::new();
        let mut on_stack: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();
        let mut sccs: Vec<Vec<String>> = Vec::new();
        self.scc_strongconnect(name, &mut counter, &mut indices, &mut lowlink, &mut on_stack, &mut stack, &mut sccs)?;
        Ok(sccs)
    }

    /// One node's worth of Tarjan's `strongconnect` — see
    /// [`Self::compute_sccs`]'s doc comment for the algorithm-level
    /// contract. Recursive over [`Self::call_graph_edges`]; an edge whose
    /// target is already compiled is skipped outright (never entered into
    /// `indices` at all), so it never contributes a spurious singleton SCC.
    #[allow(clippy::too_many_arguments)]
    fn scc_strongconnect(
        &self,
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

        for edge in self.call_graph_edges(node)? {
            let already_compiled = match &edge {
                CallEdge::Fn(p) => self.root.fn_compiled(p),
                CallEdge::Method(p, m) => self.root.method_compiled(p, m),
            };
            if already_compiled {
                continue;
            }
            let target = edge.node_name();
            if !indices.contains_key(&target) {
                self.scc_strongconnect(&target, counter, indices, lowlink, on_stack, stack, sccs)?;
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
    /// real internal name (`ast_bridge::user_symbol_name`) *before* any of
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
            for edge in self.call_graph_edges(member)? {
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
                declare_external_function(&module, &crate::compile::ast_bridge::user_symbol_name(member));
            }
            for target in &call_targets {
                declare_external_function(&module, &crate::compile::ast_bridge::user_symbol_name(&target.to_string()));
            }
            for (type_name, method) in &method_targets {
                declare_external_function(&module, &crate::compile::ast_bridge::user_method_symbol_name(type_name, method));
            }
            for (rt_name, _) in rt_extern_functions() {
                declare_external_function(&module, rt_name);
            }
            module
        };

        for member in members {
            self.add_compiled_function(heap, module.clone(), member, &crate::compile::ast_bridge::user_symbol_name(member))?;
        }

        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let mut externals: Vec<(String, usize)> = call_targets
            .iter()
            .map(|p| {
                let f = self.root.get_fn(p).expect("Self::compute_sccs's finish order guarantees this is already compiled");
                let addr = f.compiled.borrow().as_ref().expect("Self::compute_sccs's finish order guarantees this is already compiled").address();
                (crate::compile::ast_bridge::user_symbol_name(&p.to_string()), addr)
            })
            .collect();
        externals.extend(method_targets.iter().map(|(type_name, method)| {
            let f = self.root.get_method(type_name, method).expect("Self::compute_sccs's finish order guarantees this is already compiled");
            let addr = f.compiled.borrow().as_ref().expect("Self::compute_sccs's finish order guarantees this is already compiled").address();
            (crate::compile::ast_bridge::user_method_symbol_name(type_name, method), addr)
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
        let internal_names: Vec<String> = members.iter().map(|m| crate::compile::ast_bridge::user_symbol_name(m)).collect();
        let compiled_fns = crate::compile::CompiledFn::new_multi(&module.borrow(), &internal_names, &externals)
            .map_err(|e| EvalError::Panic(format!("compile: JIT failed: {}", e)))?;
        for (member, compiled) in members.iter().zip(compiled_fns) {
            match self.method_key(member) {
                Some((type_path, method)) => {
                    let f = self.root.get_method(&type_path, &method).expect("member is a registered method");
                    *f.compiled.borrow_mut() = Some(Rc::new(compiled));
                }
                None => {
                    let f = self.root.get_fn(&fn_path_from_node_name(member)).expect("member is a registered function");
                    *f.compiled.borrow_mut() = Some(Rc::new(compiled));
                }
            }
        }
        Ok(())
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
            "read-line" => Some(eval_read_line(heap)),
            "parse-int" => Some(eval_parse_int(heap, args)),
            "parse-float" => Some(eval_parse_float(heap, args)),
            "read" => Some(eval_read(heap, args)),
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
                Some(Ok(RtValue::Sexpr(heap.gensym())))
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
        let f = self.root.get_fn(path).ok_or_else(|| format!("no such macro: {}", path))?;
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
        // A closure built while a macro body executes (check-time expansion)
        // is JIT-compiled like any other (interp-closure removal Stage 6,
        // retiring Stage 7's `jit_suppressed` guard): the island is always
        // loaded (Stage 5), so `jit_define_closure` succeeds; in an embedder
        // that skipped island loading it declines *Benign* ("island not
        // loaded") and falls back exactly as before. A macro that builds a
        // closure over a non-tier type is the only behavior change — it now
        // JITs (or hard-declines *Gap*) instead of silently interpreting —
        // and no such macro exists (macro bodies close over ordinary types).
        let result = match self.bind_macro_args(heap, &f, &raw_args, fixed) {
            Ok(argv) => self.apply(heap, &f, argv),
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
            "expected an Option<V> return type at a HashTable get/remove or Vector pop site, got {:?}",
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

/// Shared tail of every scalar `print`/`println` method (`registry.rs`'s
/// per-type `"print"`/`"println"` entries): writes `text` to stdout, with a
/// trailing newline iff `newline`, and flushes immediately — a script's
/// stdout isn't a terminal when piped (e.g. into `read-line` at the far end
/// of a pipe, or a test harness), so it isn't line-buffered there, and a
/// prompt printed via `print` (no newline) must still be visible before the
/// process blocks on `read-line`.
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

fn write_stdout(text: &str, newline: bool) -> Result<RtValue, EvalError> {
    let mut out = std::io::stdout();
    let write_result = if newline { writeln!(out, "{}", text) } else { write!(out, "{}", text) };
    write_result.and_then(|()| out.flush()).map(|()| RtValue::Unit).map_err(|e| EvalError::Panic(format!("print: {}", e)))
}

/// `read-line` (`registry.rs`'s free-function entry): one line from stdin,
/// sans the trailing newline (and a trailing `\r`, for CRLF input). `None`
/// at EOF (`read_line` returning `Ok(0)`) rather than an error — a script
/// polling stdin in a loop needs to see end-of-input as ordinary data.
fn eval_read_line(heap: &mut Heap) -> Result<RtValue, EvalError> {
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => Ok(option_value(heap, None)),
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Ok(option_value(heap, Some(RtValue::Str(line.into()))))
        }
        Err(e) => Err(EvalError::Panic(format!("read-line: {}", e))),
    }
}

/// `parse-int` (`registry.rs`'s free-function entry): a decimal `i32`
/// literal (optional leading `+`/`-`, no surrounding whitespace — plain
/// `str::parse`), `Err` on anything else rather than a panic (unlike the
/// reader's own integer literals, this reads *untrusted* runtime text).
fn eval_parse_int(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let s = expect_str(&args[0])?;
    Ok(match s.parse::<i32>() {
        Ok(n) => result_ok(heap, RtValue::Int(n as i64)),
        Err(_) => result_err(heap, format!("parse-int: invalid integer literal: {:?}", s)),
    })
}

/// `parse-float` (`registry.rs`'s free-function entry): an `f64` literal via
/// `str::parse` (accepts everything Rust's own `FromStr for f64` does,
/// including `inf`/`nan`), `Err` on anything else.
fn eval_parse_float(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let s = expect_str(&args[0])?;
    Ok(match s.parse::<f64>() {
        Ok(f) => result_ok(heap, RtValue::Float(f)),
        Err(_) => result_err(heap, format!("parse-float: invalid float literal: {:?}", s)),
    })
}

/// `read` (`registry.rs`'s free-function entry): parses exactly one `Sexpr`
/// form from `s` via the ordinary reader (`crate::read::Reader::read`) —
/// the same pipeline `typl`/the REPL use for source text, just callable at
/// runtime on a string value instead of a file/stdin. `Err` (not a panic)
/// on malformed input, e.g. an unterminated list or string — this reads
/// data the running program doesn't control.
fn eval_read(heap: &mut Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let s = expect_str(&args[0])?.to_string();
    let reader = crate::read::Reader::new();
    Ok(match reader.read(heap, &s) {
        Ok(v) => result_ok(heap, RtValue::Sexpr(v)),
        Err(e) => result_err(heap, format!("read: {}", e)),
    })
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
            "pop" => Some(vector_pop(heap, args, ret_ty)),
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
            "print" => Some(expect_str(&args[0]).and_then(|s| write_stdout(s, false))),
            "println" => Some(expect_str(&args[0]).and_then(|s| write_stdout(s, true))),
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
            "print" => Some(expect_char(&args[0]).and_then(|c| write_stdout(&c.to_string(), false))),
            "println" => Some(expect_char(&args[0]).and_then(|c| write_stdout(&c.to_string(), true))),
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
            "print" => Some(rt_i64(&args[0]).and_then(|n| write_stdout(&n.to_string(), false))),
            "println" => Some(rt_i64(&args[0]).and_then(|n| write_stdout(&n.to_string(), true))),
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
            "print" => Some(rt_f64(&args[0]).and_then(|f| write_stdout(&format_float_for_print(f), false))),
            "println" => Some(rt_f64(&args[0]).and_then(|f| write_stdout(&format_float_for_print(f), true))),
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
            "print" => Some(expect_bignum(&args[0]).and_then(|n| write_stdout(&n.to_string(), false))),
            "println" => Some(expect_bignum(&args[0]).and_then(|n| write_stdout(&n.to_string(), true))),
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
            "print" => Some(expect_ratio(&args[0]).and_then(|r| write_stdout(&format!("{}/{}", r.numer(), r.denom()), false))),
            "println" => Some(expect_ratio(&args[0]).and_then(|r| write_stdout(&format!("{}/{}", r.numer(), r.denom()), true))),
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
fn llvm_module_add_function(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(&args[1])?;
    let existing = module.borrow().get_function(name);
    let function = existing.unwrap_or_else(|| module.borrow_mut().add_function(name, compiled_fn_type(), None));
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
        ),
        "string" => matches!(
            method,
            "length" | "ref" | "eq" | "equal" | "equalp" | "lt" | "<" | "<=" | ">" | ">=" | "append"
        ),
        "char" => matches!(
            method,
            "eq" | "eql" | "equal" | "equalp" | "lt" | "<" | "<=" | ">" | ">=" | "char->int"
        ),
        "f64" => matches!(
            method,
            "+" | "-" | "*" | "/" | "mod" | "expt" | "sqrt" | "floor" | "ceiling" | "round" | "truncate"
                | "float->int" | "float->bignum" | "float->ratio"
                | "<" | "<=" | ">" | ">=" | "=" | "/=" | "eq" | "eql" | "equal" | "equalp"
        ),
        "bignum" => matches!(
            method,
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" | "eq" | "eql" | "equal" | "equalp"
                | "bignum->int" | "try-bignum->int" | "bignum->float" | "bignum->ratio"
        ),
        "ratio" => matches!(
            method,
            "+" | "-" | "*" | "/" | "<" | "<=" | ">" | ">=" | "=" | "/=" | "eq" | "eql" | "equal" | "equalp"
                | "ratio->bignum" | "ratio->float" | "numerator" | "denominator"
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
pub(crate) fn rt_extern_functions() -> [(&'static str, usize); 96] {
    use crate::compile::runtime::{
        rt_atom, rt_bignum_add, rt_bignum_cmp, rt_bignum_div, rt_bignum_fits_i32, rt_bignum_mod, rt_bignum_mul, rt_bignum_new,
        rt_bignum_sub, rt_bignum_to_float, rt_bignum_to_int, rt_bignum_to_int_raw, rt_bignum_to_ratio, rt_box_kind, rt_car, rt_cdr,
        rt_cell_get, rt_cell_new, rt_cell_set, rt_char_equalp, rt_closure_env_get, rt_closure_env_len, rt_closure_fnptr,
        rt_closure_new, rt_cons, rt_consp, rt_data_field, rt_data_new, rt_data_variant, rt_float_new, rt_float_to_bignum,
        rt_float_to_ratio, rt_float_value, rt_gensym, rt_global_get, rt_global_new, rt_global_set, rt_i64_div, rt_i64_mod,
        rt_hashtable_clear, rt_hashtable_contains, rt_hashtable_count, rt_hashtable_entries, rt_hashtable_get_raw, rt_hashtable_keys,
        rt_hashtable_new, rt_hashtable_remove_raw, rt_hashtable_set, rt_hashtable_values, rt_int_to_bignum, rt_int_to_ratio,
        rt_intern_path, rt_intern_symbol, rt_list_to_path, rt_match_fail, rt_null, rt_panic, rt_path_to_list, rt_pop_sexpr_root, rt_push_permanent_sexpr_root,
        rt_push_sexpr_root, rt_ratio_add, rt_ratio_cmp, rt_ratio_denominator, rt_ratio_div, rt_ratio_from_bignums, rt_ratio_mul,
        rt_ratio_numerator, rt_ratio_sub, rt_ratio_to_bignum, rt_ratio_to_float, rt_root_count, rt_set_car, rt_set_cdr,
        rt_set_sexpr_root, rt_sexpr_bool, rt_sexpr_char, rt_sexpr_int, rt_sexpr_str, rt_str_append, rt_str_eq, rt_str_equalp,
        rt_str_length, rt_str_lt, rt_str_new, rt_str_ref, rt_struct_field_count, rt_struct_field_get, rt_struct_field_set,
        rt_struct_new, rt_struct_pop_field, rt_struct_push_field, rt_sym_name, rt_symp, rt_truncate_sexpr_roots,
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
        ("rt_path_to_list", rt_path_to_list as usize),
        ("rt_list_to_path", rt_list_to_path as usize),
        ("rt_gensym", rt_gensym as usize),
        ("rt_i64_div", rt_i64_div as usize),
        ("rt_i64_mod", rt_i64_mod as usize),
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

/// `Ok(v)`, matching `result_def`'s variant order (`ok` = 0, `err` = 1).
fn result_ok(heap: &mut Heap, v: RtValue) -> RtValue {
    build_enum_value(heap, Path::root("result"), 0, vec![v])
}

/// `Err(error(msg))` — wraps `msg` in the built-in `error` type's own single
/// `error(string)` variant (`registry::error_def`) before wrapping *that* in
/// `Result`'s `err` variant, matching `error`'s only constructor.
fn result_err(heap: &mut Heap, msg: String) -> RtValue {
    let err_val = build_enum_value(heap, Path::root("error"), 0, vec![RtValue::Str(msg.into())]);
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
pub(super) fn rtvalue_to_struct_field(heap: &mut Heap, v: &RtValue) -> Result<Value, EvalError> {
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
pub(super) fn decode_field_typed(heap: &Heap, v: Value, ty: &Type) -> RtValue {
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

/// Unlike [`vector_get`]/[`vector_set`] (which panic out of range), `pop`
/// returns `Option<T>` — an empty vector is a legitimate `None`, not a
/// bounds violation — the same shape [`hashtable_remove`] uses for its own
/// "might not be there" result. `ret_ty` is the call site's checked
/// `Option<T>` return type; [`option_payload_ty`] extracts `T` for
/// [`decode_field_typed`]'s type-directed decode of the popped element.
fn vector_pop(heap: &mut Heap, args: &[RtValue], ret_ty: &Type) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let val_ty = option_payload_ty(ret_ty)?;
    let popped = heap.struct_pop_field(id).map(|v| decode_field_typed(heap, v, &val_ty));
    Ok(option_value(heap, popped))
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

// ---- LLVM handle registry + `rt_llvm_call` (interp-closure removal ------
// ---- Stage 1) -----------------------------------------------------------
//
// Compiled code represents every Rust-native LLVM value (`RtValue::
// LlvmModule`/`LlvmBuilder`/`LlvmFunction`/`LlvmBasicBlock`/`LlvmValue`)
// and the native `RtValue::Scope` as an *LLVM handle*: an index into this
// thread-local registry, carried as a plain untraced `i64`
// (`ast_bridge::is_llvm_handle_ty` — kind `1`, the integer kind). That is
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

thread_local! {
    static LLVM_HANDLES: RefCell<Vec<RtValue>> = const { RefCell::new(Vec::new()) };
}

/// Registers `v` and returns its handle — the raw `i64` compiled code
/// carries in place of the Rust-native value.
pub(crate) fn llvm_handle_register(v: RtValue) -> i64 {
    LLVM_HANDLES.with(|t| {
        let mut t = t.borrow_mut();
        t.push(v);
        (t.len() - 1) as i64
    })
}

/// The value behind handle `h`, or `None` for a never-issued (or already
/// released) handle.
pub(crate) fn llvm_handle_get(h: i64) -> Option<RtValue> {
    if h < 0 {
        return None;
    }
    LLVM_HANDLES.with(|t| t.borrow().get(h as usize).cloned())
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
/// shape, keyed in [`llvm_op_table`] by [`ast_bridge::llvm_op_id`].
struct LlvmOp {
    type_key: &'static str,
    method: String,
    args: Vec<LlvmArgK>,
    ret: LlvmRetK,
}

fn llvm_arg_kind(ty: &Type) -> LlvmArgK {
    if crate::compile::ast_bridge::is_llvm_handle_ty(ty) {
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
    if crate::compile::ast_bridge::is_llvm_handle_ty(ty) {
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

/// The `rt_llvm_call` dispatch table, keyed by [`ast_bridge::llvm_op_id`]'s
/// stable hash. The `llvm-*` entries are *derived* from the same
/// `check::registry` `AdtDef`s the checker types these methods with —
/// table and signatures cannot drift apart. The six `native-scope` entries
/// are written out by hand because `scope_def`'s signatures are generic
/// over `V` (here always an LLVM handle — see
/// `ast_bridge::llvm_assoc_key`, which only routes a `Scope<V>` with an
/// LLVM-handle `V` to this table in the first place).
fn llvm_op_table() -> &'static HashMap<i64, LlvmOp> {
    static TABLE: std::sync::OnceLock<HashMap<i64, LlvmOp>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t: HashMap<i64, LlvmOp> = HashMap::new();
        let insert = |t: &mut HashMap<i64, LlvmOp>, type_key: &'static str, method: String, args: Vec<LlvmArgK>, ret: LlvmRetK| {
            let id = crate::compile::ast_bridge::llvm_op_id(type_key, &method);
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
        insert(&mut t, "native-scope", "new".to_string(), vec![], LlvmRetK::Handle);
        insert(&mut t, "native-scope", "clone-frames".to_string(), vec![H], LlvmRetK::Handle);
        insert(&mut t, "native-scope", "push-frame".to_string(), vec![H], LlvmRetK::Unit);
        insert(&mut t, "native-scope", "pop-frame".to_string(), vec![H], LlvmRetK::Unit);
        insert(&mut t, "native-scope", "get".to_string(), vec![H, S], LlvmRetK::OptHandle);
        insert(&mut t, "native-scope", "set".to_string(), vec![H, S, H], LlvmRetK::Unit);
        t
    })
}

/// The `rt_*` family's abort-on-invariant-break convention
/// (`typelisp-rt`'s `fatal`), local to the one main-crate shim.
fn rt_llvm_fatal(msg: &str) -> ! {
    eprintln!("typelisp runtime error: {}", msg);
    std::process::abort();
}

/// `(rt-llvm-call opid arg...)` for compiled code — the generic dispatch
/// shim behind every compiled `llvm-*`/native-`Scope<V>` builtin method
/// call (`compiler.rs`'s `compile-llvm-op`; interp-closure removal Stage
/// 1). `args[0]` is the [`ast_bridge::llvm_op_id`] hash embedded at
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
    let mut vals: Vec<RtValue> = Vec::with_capacity(raw_args.len());
    for (raw, k) in raw_args.iter().zip(&op.args) {
        vals.push(match k {
            LlvmArgK::Handle => match llvm_handle_get(*raw) {
                Some(v) => v,
                None => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: dangling llvm handle {}", op.type_key, op.method, raw)),
            },
            LlvmArgK::Str => match crate::compile::runtime::decode(*raw) {
                Value::Str(id) => RtValue::Str(heap.string(id).into()),
                other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Str argument, got {:?}", op.type_key, op.method, other)),
            },
            LlvmArgK::Int => RtValue::Int(*raw),
            LlvmArgK::Bool => RtValue::Bool(*raw != 0),
        });
    }
    // The native scope `get` returns `Option<V>` — encoded specially, so
    // it's handled before the generic single-`RtValue` result path.
    if op.type_key == "native-scope" && op.method == "get" {
        let found = match (expect_scope(&vals[0]), expect_str(&vals[1])) {
            (Ok(scope), Ok(name)) => scope.get(name),
            (Err(e), _) | (_, Err(e)) => rt_llvm_fatal(&format!("rt_llvm_call: native-scope::get: {:?}", e)),
        };
        let boxed = match found {
            Some(v) => {
                let h = llvm_handle_register(v);
                heap.alloc_enum("option".to_string(), 0, vec![Value::Int(h)])
            }
            None => heap.alloc_enum("option".to_string(), 1, vec![]),
        };
        return crate::compile::runtime::encode(boxed);
    }
    let result: Result<RtValue, EvalError> = if op.type_key == "native-scope" {
        match op.method.as_str() {
            "new" => Ok(scope_new()),
            "clone-frames" => scope_clone_frames(&vals),
            "push-frame" => scope_push_frame(&vals),
            "pop-frame" => scope_pop_frame(&vals),
            "set" => scope_set(&vals),
            other => rt_llvm_fatal(&format!("rt_llvm_call: unknown native-scope method {}", other)),
        }
    } else {
        match eval_llvm_builtin_method(&Path::root(op.type_key), &op.method, &vals) {
            Some(r) => r,
            None => rt_llvm_fatal(&format!("rt_llvm_call: {} has no builtin method {}", op.type_key, op.method)),
        }
    };
    let v = match result {
        Ok(v) => v,
        Err(e) => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: {:?}", op.type_key, op.method, e)),
    };
    match op.ret {
        LlvmRetK::Handle => llvm_handle_register(v),
        LlvmRetK::Unit => 0,
        LlvmRetK::Bool => match v {
            RtValue::Bool(b) => i64::from(b),
            other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Bool result, got {:?}", op.type_key, op.method, other)),
        },
        LlvmRetK::Str => match v {
            RtValue::Str(s) => crate::compile::runtime::encode(heap.alloc_string(s.to_string())),
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
/// hold their `Cons`/`Str`/`Sym` payloads in the cons heap. `&mut` (not `&`)
/// since `match_sexpr_ctor`'s `path` arm allocates a fresh `Sexpr` list of
/// `sym`s while decomposing a `Value::Path` — the one arm across every
/// variant that builds heap data rather than only reading already-resident
/// data — and needs `Heap::push_root`/`cons` for that.
fn match_pattern(heap: &mut Heap, pat: &Pattern, v: &RtValue) -> Option<Vec<(String, SlotKind, RtValue)>> {
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
fn match_sexpr_ctor(heap: &mut Heap, variant: usize, args: &[Pattern], v: Value) -> Option<Vec<(String, SlotKind, RtValue)>> {
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
        // `(Path v)` binds `v : Sexpr` — a fresh proper list of the path's
        // segments as `sym`s, in written order — the inverse of
        // `construct_sexpr`'s `SEXPR_PATH` arm. The only `match_sexpr_ctor`
        // arm that allocates: `path_segments` only borrows a permanent
        // (non-GC) table, but the list linking those segments together is
        // built fresh here, so the growing accumulator needs rooting across
        // each `cons` call exactly like `alloc_quoted`'s `QuotedSexpr::Cons`
        // case does — every element is a permanent `Value::Symbol` (never
        // GC-collected), so only the `Cons` chain itself is at risk.
        (SEXPR_PATH, Value::Path(id)) => {
            let segs = heap.path_segments(id).to_vec();
            let mut acc = Value::Empty;
            for seg in segs.into_iter().rev() {
                heap.push_root(acc);
                let result = heap.cons(Value::Symbol(seg), acc);
                heap.pop_root();
                acc = result.ok()?;
            }
            match_pattern(heap, &args[0], &RtValue::Sexpr(acc))
        }
        _ => None,
    }
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
        // built directly as already-typed AST, so the checker's forward-
        // reference restriction never comes into play.
        interp.root.fns.insert(
            "a".to_string(),
            Rc::new(FnDef {
                params: vec![],
                kinds: vec![],
                body: vec![Typed::new(Expr::Call(crate::check::ast::Ref::synthetic(Path::root("b")), vec![]), Type::I64)],
                rest: false,
                sig: Some((vec![], Type::I64)),
                public: true,
                compiled: RefCell::new(None),
            }),
        );
        interp.root.fns.insert(
            "b".to_string(),
            Rc::new(FnDef {
                params: vec![],
                kinds: vec![],
                body: vec![Typed::new(Expr::Call(crate::check::ast::Ref::synthetic(Path::root("a")), vec![]), Type::I64)],
                rest: false,
                sig: Some((vec![], Type::I64)),
                public: true,
                compiled: RefCell::new(None),
            }),
        );

        interp
            .compile_function(&mut heap, &CompileTarget::Fn(crate::check::ast::Ref::synthetic(Path::root("a"))))
            .expect("mutual recursion across separate top-level functions should now compile");
        assert!(interp.root.fn_compiled(&Path::root("a")), "\"a\" should have ended up compiled");
        assert!(interp.root.fn_compiled(&Path::root("b")), "\"b\", pulled in transitively as part of the same SCC, should have ended up compiled too");
    }
}
