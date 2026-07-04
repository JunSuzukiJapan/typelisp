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
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};
use inkwell::AddressSpace;

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
}

impl Interp {
    pub fn new() -> Interp {
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
        match ty {
            Type::Named(p, _)
                if is_sexpr_type(p) || *p == Path::root("hashtable") || self.struct_types.contains(p) =>
            {
                SlotKind::Heap
            }
            Type::Named(p, args) if *p == Path::root("scope") && args.len() == 1 => self.heap_repr_kind(&args[0]),
            _ => SlotKind::Native,
        }
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
        }
    }

    fn eval(&self, heap: &mut Heap, t: &Typed, env: &Env) -> Result<RtValue, EvalError> {
        match &t.expr {
            Expr::Int(n) => Ok(RtValue::Int(*n)),
            Expr::Float(f) => Ok(RtValue::Float(*f)),
            Expr::Bool(b) => Ok(RtValue::Bool(*b)),
            Expr::Char(c) => Ok(RtValue::Char(*c)),
            Expr::Str(s) => Ok(RtValue::Str(s.as_str().into())),
            Expr::Unit => Ok(RtValue::Unit),
            Expr::Var(n) => env_get(env, n)
                .map(|s| s.get(heap))
                .ok_or_else(|| EvalError::Unbound(n.clone())),
            Expr::Global(path) => self
                .globals
                .get(path)
                .map(|s| s.get(heap))
                .ok_or_else(|| EvalError::Unbound(path.to_string())),
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
                    let (fields, _slots) = self.eval_args(heap, args, env)?;
                    Ok(RtValue::Data { type_name: type_name.clone(), variant: *variant, fields })
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
            Expr::SetGlobal(path, value) => {
                let v = self.eval(heap, value, env)?;
                let cell = self
                    .globals
                    .get(path)
                    .ok_or_else(|| EvalError::Unbound(path.to_string()))?
                    .clone();
                cell.set(heap, v.clone())?;
                Ok(v)
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
        // Encoded off `argv`'s own runtime shape, not each parameter's
        // *static* type (unlike `ret_ty`'s decode below) — a generic
        // parameter (`(defun (describe T) ((it T)) ...)`) has no concrete
        // `Type` to classify at all here, only whatever `T` happened to be
        // substituted with at this call site, and `Self::struct_types`
        // (keyed by concrete type `Path`, never a type variable) can't
        // answer that. `RtValue::Sexpr`/`RtValue::Int` are themselves
        // unambiguous — a well-typed argument's `RtValue` variant is already
        // exactly the one `Self::is_boxed_sexpr_type` would have derived
        // from its (possibly-generic) static type anyway, so no information
        // is lost by reading it directly off the value instead.
        let int_args = argv
            .iter()
            .map(|v| match v {
                // An *interpreted* closure box must not silently cross this
                // boundary: compiled code represents function values as its
                // own `ClosureBox` (a malloc'd i64 pointer), and a tagged
                // interp closure handed over as a plain `Sexpr` would be
                // dereferenced as one — same "clear internal error, not a
                // silent misread" stance the pre-6b `RtValue::Closure`
                // rejection took.
                RtValue::Sexpr(Value::Boxed(id)) if heap.is_closure(*id) => Err(EvalError::Internal(
                    "compiled call: an interpreted closure cannot be passed to compiled code".into(),
                )),
                RtValue::Sexpr(sv) => Ok(crate::compile::runtime::encode(*sv)),
                RtValue::Int(n) => Ok(*n),
                other => Err(EvalError::Internal(format!("compiled call: unsupported argument {:?}", other))),
            })
            .collect::<Result<Vec<i64>, EvalError>>()?;
        // Registers `heap` as this thread's active `Heap` (see
        // `compile::runtime::set_active_heap`'s doc comment) so any
        // `rt-cons`/`rt-car`/... call the compiled code makes — directly or
        // transitively through another compiled function — resolves against
        // the right heap. Done on every call rather than once, since it's
        // one pointer store and there's no cheaper place to detect "this
        // callee might transitively touch the heap" ahead of time.
        crate::compile::runtime::set_active_heap(heap as *mut Heap);
        let raw = compiled.call(&int_args);
        Ok(if self.is_boxed_sexpr_type(ret_ty) {
            RtValue::Sexpr(crate::compile::runtime::decode(raw))
        } else {
            RtValue::Int(raw)
        })
    }

    /// Whether `ty`'s compiled representation crosses the typelisp-call-
    /// syntax/compiled-code boundary as a tagged `Sexpr`
    /// (`compile::runtime::encode`/`decode`, [`Self::call_compiled`]) rather
    /// than a plain `i64` — `Sexpr` itself, or any `Type::Named` this
    /// `Interp` has recorded in [`Self::struct_types`] (Stage 3 of the
    /// Sexpr/RtValue unification plan, `docs/implementation-log.md`).
    fn is_boxed_sexpr_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Named(p, _) if is_sexpr_type(p) || self.struct_types.contains(p))
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
        let param_list = match crate::compile::ast_bridge::tagged_sym_list(heap, &params) {
            Ok(v) => v,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
        heap.push_root(param_list);
        let body_sexpr = match crate::compile::ast_bridge::ast_to_sexpr(heap, &body, &self.struct_types) {
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
    /// lookup) can never drift apart. A target whose `type_name` isn't
    /// registered in `self.methods` at all (`i64`/`i32`'s own built-in
    /// arithmetic; `string`'s, since Stage 7 of the Sexpr-representation
    /// plan — `docs/implementation-log.md`; or — still out of scope —
    /// `f64`/`char`'s) needs none of this: each of `i64`/`i32`/`string`
    /// compiles natively with no external call (`compile-assoc`'s own
    /// dispatch, which panics clearly on its own for any one of *their*
    /// methods it doesn't actually implement, e.g. `string::upcase`), and
    /// anything else panics clearly right here rather than deep inside
    /// `compile-assoc`'s own `get-function`.
    fn compile_function(&self, heap: &mut Heap, name: &str) -> Result<RtValue, EvalError> {
        let path = Path::root(name);
        let method_key = self.method_key(name);
        let (_, body) = self.compiled_fn_body(name)?;
        let call_targets: Vec<Path> = crate::compile::ast_bridge::collect_call_targets(&body)
            .into_iter()
            .filter(|p| *p != path && !is_rt_builtin_name(p.local()))
            .collect();
        for target in &call_targets {
            if !self.compiled.borrow().contains_key(target) {
                return Err(EvalError::Panic(format!(
                    "compile: \"{}\" calls \"{}\", which must be `compile`d first",
                    name,
                    target.local()
                )));
            }
        }

        let method_targets: Vec<(Path, String)> = crate::compile::ast_bridge::collect_assoc_targets(&body)
            .into_iter()
            .filter(|key| method_key.as_ref() != Some(key))
            .filter(|(type_name, _)| !matches!(type_name.local(), "i64" | "i32" | "string"))
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
                return Err(EvalError::Panic(format!(
                    "compile: \"{}\" calls \"{}\", which must be `compile`d first",
                    name,
                    method_link_name(type_name, method)
                )));
            }
        }

        let module = {
            let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
            let module = Rc::new(RefCell::new(crate::compile::llvm_context().create_module("compiled")));
            for target in &call_targets {
                declare_external_function(&module, target.local());
            }
            for (type_name, method) in &method_targets {
                declare_external_function(&module, &method_link_name(type_name, method));
            }
            for (rt_name, _) in rt_extern_functions() {
                declare_external_function(&module, rt_name);
            }
            module
        };
        self.add_compiled_function(heap, module.clone(), name, name)?;

        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let mut externals: Vec<(String, usize)> = {
            let compiled = self.compiled.borrow();
            call_targets
                .iter()
                .map(|p| (p.local().to_string(), compiled.get(p).expect("checked compiled above").address()))
                .collect()
        };
        {
            let compiled_methods = self.compiled_methods.borrow();
            externals.extend(method_targets.iter().map(|(type_name, method)| {
                let key = (type_name.clone(), method.clone());
                (method_link_name(type_name, method), compiled_methods.get(&key).expect("checked compiled_methods above").address())
            }));
        }
        externals.extend(rt_extern_functions().iter().map(|(n, addr)| (n.to_string(), *addr)));
        // A no-op on everything this typelisp-hosted compiler body emits
        // today (see that pass's own doc comment) — run unconditionally
        // anyway, the same "cheap, provably-safe cleanup pass" spirit as
        // running an optimizer at `OptimizationLevel::None` costs nothing
        // when it finds nothing to do.
        crate::compile::arc_opt::eliminate_redundant_retain_release_pairs(&module.borrow());
        // Mirrors `compile::aot::compile_file`'s own `verify()` call in the
        // same position (after `arc_opt`, before handing the module to LLVM
        // for real): a typelisp-hosted `compiler.rs` bug that emits
        // instructions after a block's terminator (the `compile-let`
        // GC-root-leak fix's own doc comment names this exact risk) would
        // otherwise reach `CompiledFn::new`'s `create_jit_execution_engine`
        // as malformed IR — undefined behavior in LLVM itself, not a
        // catchable Rust error. Verifying first turns that into a clean
        // `Panic` instead.
        module.borrow().verify().map_err(|e| EvalError::Panic(format!("compile: module failed verification: {}", e)))?;
        let compiled = crate::compile::CompiledFn::new(&module.borrow(), name, &externals)
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
            "cons" => match (args.first(), args.get(1)) {
                (Some(RtValue::Sexpr(a)), Some(RtValue::Sexpr(b))) => {
                    self.sync_roots(heap);
                    Some(heap.cons(*a, *b).map(RtValue::Sexpr).map_err(|e| EvalError::Panic(e.to_string())))
                }
                _ => Some(Err(EvalError::Internal("cons: expected two Sexpr arguments".into()))),
            },
            "car" => match args.first() {
                Some(RtValue::Sexpr(v)) => {
                    Some(heap.car(*v).map(RtValue::Sexpr).map_err(|_| EvalError::Panic("car: not a cons".into())))
                }
                Some(_) => Some(Err(EvalError::Internal("car: expected a Sexpr argument".into()))),
                None => Some(Err(EvalError::Internal("car: expected one argument".into()))),
            },
            "cdr" => match args.first() {
                Some(RtValue::Sexpr(v)) => {
                    Some(heap.cdr(*v).map(RtValue::Sexpr).map_err(|_| EvalError::Panic("cdr: not a cons".into())))
                }
                Some(_) => Some(Err(EvalError::Internal("cdr: expected a Sexpr argument".into()))),
                None => Some(Err(EvalError::Internal("cdr: expected one argument".into()))),
            },
            "set-car" => match (args.first(), args.get(1)) {
                (Some(RtValue::Sexpr(c)), Some(RtValue::Sexpr(v))) => Some(
                    heap.set_car(*c, *v)
                        .map(|_| RtValue::Unit)
                        .map_err(|_| EvalError::Panic("set-car: not a cons".into())),
                ),
                _ => Some(Err(EvalError::Internal("set-car: expected two Sexpr arguments".into()))),
            },
            "set-cdr" => match (args.first(), args.get(1)) {
                (Some(RtValue::Sexpr(c)), Some(RtValue::Sexpr(v))) => Some(
                    heap.set_cdr(*c, *v)
                        .map(|_| RtValue::Unit)
                        .map_err(|_| EvalError::Panic("set-cdr: not a cons".into())),
                ),
                _ => Some(Err(EvalError::Internal("set-cdr: expected two Sexpr arguments".into()))),
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
        QuotedSexpr::Char(c) => Ok(Value::Char(*c)),
        QuotedSexpr::Bool(b) => Ok(Value::Bool(*b)),
        QuotedSexpr::Sym(s) => Ok(heap.intern_symbol(s)),
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

/// Recursively gather every `Sexpr` value reachable from `v` through nested
/// `Data` fields or `Scope` frames. (A closure needs no arm of its own:
/// since Stage 6b it *is* an `RtValue::Sexpr` closure box — pushed by the
/// plain `Sexpr` arm, with the GC tracing its heap-cell captures from
/// there — while its `Native` captures are each already registered in
/// [`Interp::slots`].) A boxed struct (`RtValue::Sexpr(Value::Boxed(_))`,
/// since the `Sexpr`/`RtValue` unification's Stage 2 — `HashTable<K,V>`
/// included, since Stage 5) needs no separate arm here — the plain `Sexpr`
/// one already pushes its `Value::Boxed` root, and `Heap::gc`'s mark phase
/// traces *into* a `BoxedObj::Struct`'s own fields (or, for a `HashTable`,
/// its `StructPayload::Map` keys/values) from there (see
/// `Heap::push_boxed_nested`), the same way it already does for a `Cons`
/// cell's `car`/`cdr`.
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
                "get" => Some(scope_get(args)),
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
            "alloca-args" => Some(llvm_builder_alloca_args(args)),
            "store-arg" => Some(llvm_builder_store_arg(args)),
            "build-call" => Some(llvm_builder_build_call(args)),
            "load-env" => Some(llvm_builder_load_env(args)),
            "build-call-with-env" => Some(llvm_builder_build_call_with_env(args)),
            "build-make-closure" => Some(llvm_builder_build_make_closure(args)),
            "build-closure-env-get" => Some(llvm_builder_build_closure_env_get(args)),
            "build-closure-apply" => Some(llvm_builder_build_closure_apply(args)),
            "build-closure-retain" => Some(llvm_builder_build_closure_retain(args)),
            "build-closure-release" => Some(llvm_builder_build_closure_release(args)),
            "debug-closure-refcount" => Some(llvm_builder_debug_closure_refcount(args)),
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

/// True for the handful of free functions `compiler.rs`'s `compile-call`
/// rewrites to a `crate::compile::runtime` shim by name (`car` -> `rt_car`,
/// etc. — see that function's `raw-nm`/`nm` rename) rather than requiring
/// `(compile car)` first: these can never be `compile`d themselves (no
/// typelisp AST body — direct cons-heap access, Rust-only), so
/// [`Interp::compile_function`] excludes them from its normal "every call
/// target must already be compiled" check and instead always wires them via
/// [`rt_extern_functions`].
fn is_rt_builtin_name(name: &str) -> bool {
    matches!(name, "car" | "cdr" | "cons" | "set-car" | "set-cdr")
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
/// `compiler.rs`'s `retain-bindings`/`release-bindings`/`bind-let-values`/
/// `restore-let-values` call them directly via `get-function`/`build-call`,
/// the same way `release-pending-args` calls `build-closure-release`
/// directly rather than through the `(call name args)` tag. They still need
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
pub(crate) fn rt_extern_functions() -> [(&'static str, usize); 22] {
    use crate::compile::runtime::{
        rt_car, rt_cdr, rt_cons, rt_float_new, rt_float_value, rt_match_fail, rt_pop_sexpr_root, rt_push_permanent_sexpr_root, rt_push_sexpr_root,
        rt_root_count, rt_set_car, rt_set_cdr, rt_set_sexpr_root, rt_str_append, rt_str_eq, rt_str_length, rt_str_new, rt_str_ref,
        rt_struct_field_get, rt_struct_field_set, rt_struct_new, rt_truncate_sexpr_roots,
    };
    [
        ("rt_car", rt_car as usize),
        ("rt_cdr", rt_cdr as usize),
        ("rt_cons", rt_cons as usize),
        ("rt_set_car", rt_set_car as usize),
        ("rt_set_cdr", rt_set_cdr as usize),
        ("rt_match_fail", rt_match_fail as usize),
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
        ("rt_str_append", rt_str_append as usize),
        ("rt_float_new", rt_float_new as usize),
        ("rt_float_value", rt_float_value as usize),
        ("rt_struct_new", rt_struct_new as usize),
        ("rt_struct_field_get", rt_struct_field_get as usize),
        ("rt_struct_field_set", rt_struct_field_set as usize),
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
/// same shape [`get_or_define_closure_release_fn`] already uses internally
/// for its own null/refcount checks.
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

/// A `ClosureBox`'s fixed heap layout (labels/closures Stage 4): 4 header
/// slots, then `env_len` captured `i64` values inline right after — see
/// `registry::llvm_builder_def`'s doc comment on `build-make-closure` for
/// why this exists at all (a `lambda` value that *escapes* its defining
/// function, rather than being called directly while still statically
/// resolvable — `compile-apply`/`compile-call`'s direct-call scope).
const CLOSURE_HEADER_SLOTS: u64 = 4;
const CLOSURE_FN_PTR_SLOT: u64 = 0;
const CLOSURE_ENV_LEN_SLOT: u64 = 1;
const CLOSURE_REFCOUNT_SLOT: u64 = 2;
/// A bitmask, one bit per captured slot (so up to 64 captures), marking
/// which of a `ClosureBox`'s captured values are themselves `Fn`-typed —
/// the automatic retain/release insertion work's piece of per-box metadata,
/// computed entirely at compile time (`compiler.rs`'s `compute-fn-mask`)
/// from the static types already known for every captured name. Needed so
/// [`get_or_define_closure_release_fn`]'s cascading release (a box being
/// freed must also release any closure-typed values it captured — the
/// `Drop`/`deinit`-cascade counterpart of `compile-lambda`'s/`resolve-value`'s
/// retain at capture time) knows, at *runtime*, which slots to recurse into
/// without corrupting an ordinary `i64` by treating it as a pointer.
const CLOSURE_FN_MASK_SLOT: u64 = 3;

/// GEPs to slot `slot` of a `ClosureBox` whose base address is `base` — the
/// same flat-`i64`-array GEP pattern [`llvm_builder_load_arg`]/
/// [`llvm_builder_store_arg`] already use for the args/env arrays, just
/// against heap-allocated closure storage instead of a stack one.
fn closure_slot_ptr(b: &Builder<'static>, base: PointerValue<'static>, slot: u64) -> Result<PointerValue<'static>, EvalError> {
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(slot, false);
    unsafe {
        b.build_gep(ctx.i64_type(), base, &[idx_val], "closure_slot_ptr")
            .map_err(|e| EvalError::Internal(format!("closure: {}", e)))
    }
}

/// The dynamic-index counterpart of [`closure_slot_ptr`] — `idx` is a
/// runtime `i64` value rather than a compile-time constant, for
/// [`get_or_define_closure_release_fn`]'s captured-slot scan (the number of
/// captures, and which slot index is being visited, are only known once the
/// loop is actually running).
fn closure_slot_ptr_dyn(
    b: &Builder<'static>,
    base: PointerValue<'static>,
    idx: inkwell::values::IntValue<'static>,
) -> Result<PointerValue<'static>, EvalError> {
    let ctx = crate::compile::llvm_context();
    unsafe {
        b.build_gep(ctx.i64_type(), base, &[idx], "closure_slot_ptr_dyn")
            .map_err(|e| EvalError::Internal(format!("closure: {}", e)))
    }
}

fn store_closure_slot(
    b: &Builder<'static>,
    base: PointerValue<'static>,
    slot: u64,
    value: BasicValueEnum<'static>,
    who: &str,
) -> Result<(), EvalError> {
    let ptr = closure_slot_ptr(b, base, slot)?;
    b.build_store(ptr, value).map_err(|e| EvalError::Internal(format!("{}: {}", who, e)))?;
    Ok(())
}

fn load_closure_slot(b: &Builder<'static>, base: PointerValue<'static>, slot: u64, who: &str) -> Result<inkwell::values::IntValue<'static>, EvalError> {
    let ptr = closure_slot_ptr(b, base, slot)?;
    let ctx = crate::compile::llvm_context();
    b.build_load(ctx.i64_type(), ptr, "closure_slot_val")
        .map_err(|e| EvalError::Internal(format!("{}: {}", who, e)))
        .map(|v| v.into_int_value())
}

/// Reinterprets a closure `i64` value back into the `ClosureBox` pointer it
/// actually is — the inverse of `build-make-closure`'s final `ptrtoint`.
fn closure_box_ptr(b: &Builder<'static>, closure: BasicValueEnum<'static>, who: &str) -> Result<PointerValue<'static>, EvalError> {
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    b.build_int_to_ptr(closure.into_int_value(), ptr_ty, "closure_box_ptr")
        .map_err(|e| EvalError::Internal(format!("{}: {}", who, e)))
}

/// Heap-allocates a `ClosureBox` wrapping `target` (a function declared via
/// `add-function-with-env` — see `compiled_fn_type_with_env`'s doc comment
/// for why *every* closure-boxed function uses that ABI) and a copy of
/// `env`'s `env_len` values (an already-built `alloca-args`/`store-arg`
/// array — the same shape a direct capturing call already builds, see
/// `compiler.rs`'s `compile-env-args`). Uses `Builder::build_array_malloc`
/// (LLVM's legacy `malloc`-declaring IR helper, not a Rust-side runtime
/// shim — see `registry::llvm_builder_def`'s doc comment for that decision):
/// `malloc`/`free` are ordinary, already-linked C library symbols on both
/// backends (AOT's `cc` step links libc by default; MCJIT resolves
/// unmapped externals against the host process's own symbol table by
/// default, and this process is itself linked against libc) — unlike
/// Stage 3's cross-`compile`-call case, no `add_global_mapping` wiring is
/// needed here at all. `fn_mask` (automatic retain/release insertion
/// follow-up) is stored verbatim into `CLOSURE_FN_MASK_SLOT` — a bitmask,
/// computed entirely at compile time by `compiler.rs`'s `compute-fn-mask`,
/// of which captured slots are themselves `Fn`-typed, so
/// [`get_or_define_closure_release_fn`]'s cascading release knows which
/// slots to recurse into when this box is finally freed.
fn llvm_builder_build_make_closure(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let env_ptr = expect_llvm_value(&args[2])?.into_pointer_value();
    let env_len = match &args[3] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let fn_mask = match &args[4] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let total_slots = ctx.i64_type().const_int(CLOSURE_HEADER_SLOTS + env_len, false);
    let box_ptr = b
        .build_array_malloc(ctx.i64_type(), total_slots, "closure_box")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;

    let fn_ptr_int = b
        .build_ptr_to_int(target.as_global_value().as_pointer_value(), ctx.i64_type(), "closure_fn_ptr")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
    store_closure_slot(&b, box_ptr, CLOSURE_FN_PTR_SLOT, fn_ptr_int.into(), "build-make-closure")?;
    store_closure_slot(&b, box_ptr, CLOSURE_ENV_LEN_SLOT, ctx.i64_type().const_int(env_len, false).into(), "build-make-closure")?;
    store_closure_slot(&b, box_ptr, CLOSURE_REFCOUNT_SLOT, ctx.i64_type().const_int(1, false).into(), "build-make-closure")?;
    store_closure_slot(&b, box_ptr, CLOSURE_FN_MASK_SLOT, ctx.i64_type().const_int(fn_mask, false).into(), "build-make-closure")?;

    for i in 0..env_len {
        let idx_val = ctx.i64_type().const_int(i, false);
        let src_ptr = unsafe {
            b.build_gep(ctx.i64_type(), env_ptr, &[idx_val], "closure_env_src")
                .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?
        };
        let v = b
            .build_load(ctx.i64_type(), src_ptr, "closure_env_val")
            .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
        store_closure_slot(&b, box_ptr, CLOSURE_HEADER_SLOTS + i, v, "build-make-closure")?;
    }

    let closure_val = b
        .build_ptr_to_int(box_ptr, ctx.i64_type(), "closure_value")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
    Ok(RtValue::LlvmValue(closure_val.into()))
}

/// `load-env`'s closure-value counterpart: reads captured slot `index` back
/// out of a `ClosureBox` value directly, for code *holding the closure
/// value* (e.g. about to forward its captures elsewhere) — the closure-boxed
/// function's own body still reads its captures via `load-env` against its
/// own env parameter, never this one.
fn llvm_builder_build_closure_env_get(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let closure = expect_llvm_value(&args[1])?;
    let index = match &args[2] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let b = builder.borrow();
    let box_ptr = closure_box_ptr(&b, closure, "build-closure-env-get")?;
    let v = load_closure_slot(&b, box_ptr, CLOSURE_HEADER_SLOTS + index, "build-closure-env-get")?;
    Ok(RtValue::LlvmValue(v.into()))
}

/// `build-call-with-env`'s indirect counterpart: the callee isn't a
/// statically-known `llvm-function` here, only an `i64` closure value, so
/// this loads `fn_ptr`/`env_len`/the env array straight out of the box at
/// runtime and calls through `Builder::build_indirect_call` against the one
/// fixed `compiled_fn_type_with_env` signature every closure-boxed function
/// shares (see that function's doc comment for why no ABI branch is needed
/// here).
fn llvm_builder_build_closure_apply(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let closure = expect_llvm_value(&args[1])?;
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = match &args[3] {
        RtValue::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let box_ptr = closure_box_ptr(&b, closure, "build-closure-apply")?;
    let fn_ptr_int = load_closure_slot(&b, box_ptr, CLOSURE_FN_PTR_SLOT, "build-closure-apply")?;
    let env_len_int = load_closure_slot(&b, box_ptr, CLOSURE_ENV_LEN_SLOT, "build-closure-apply")?;
    let env_len_i32 = b
        .build_int_truncate(env_len_int, ctx.i32_type(), "closure_env_len_i32")
        .map_err(|e| EvalError::Internal(format!("build-closure-apply: {}", e)))?;
    let env_ptr = closure_slot_ptr(&b, box_ptr, CLOSURE_HEADER_SLOTS)?;
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let fn_ptr = b
        .build_int_to_ptr(fn_ptr_int, ptr_ty, "closure_fn_ptr_val")
        .map_err(|e| EvalError::Internal(format!("build-closure-apply: {}", e)))?;
    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = b
        .build_indirect_call(
            compiled_fn_type_with_env(),
            fn_ptr,
            &[args_ptr.into(), argc_val.into(), env_ptr.into(), env_len_i32.into()],
            "closure_apply_result",
        )
        .map_err(|e| EvalError::Internal(format!("build-closure-apply: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(RtValue::LlvmValue(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-closure-apply: callee produced no value".into())),
    }
}

/// Increments a `ClosureBox`'s refcount and returns the closure value
/// itself unchanged, so calls can chain (`(build-closure-retain b (compile-value ...))`)
/// without a separate `let` to hold onto the original value.
fn llvm_builder_build_closure_retain(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let closure = expect_llvm_value(&args[1])?;
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let box_ptr = closure_box_ptr(&b, closure, "build-closure-retain")?;
    let rc = load_closure_slot(&b, box_ptr, CLOSURE_REFCOUNT_SLOT, "build-closure-retain")?;
    let one = ctx.i64_type().const_int(1, false);
    let rc2 = b.build_int_add(rc, one, "closure_rc_inc").map_err(|e| EvalError::Internal(format!("build-closure-retain: {}", e)))?;
    store_closure_slot(&b, box_ptr, CLOSURE_REFCOUNT_SLOT, rc2.into(), "build-closure-retain")?;
    Ok(RtValue::LlvmValue(closure))
}

/// Builds (once per `module`) the shared LLVM function backing
/// `build-closure-release`: `void __typelisp_closure_release(i64 closure)`.
/// Decrements `closure`'s refcount; once that reaches zero, walks its
/// captured slots (`CLOSURE_FN_MASK_SLOT` says which ones are themselves
/// `Fn`-typed) recursively releasing each one *before* freeing the box
/// itself — the `Drop`/`deinit`-cascade counterpart of `compile-lambda`'s/
/// `resolve-value`'s retain at capture time (automatic retain/release
/// insertion, a follow-up to labels/closures Stage 4).
///
/// This has to be a real, runtime-self-recursive *LLVM function* — not a
/// Rust-side recursive call that statically unrolls the IR — because a
/// captured slot's own `env_len`/`fn_mask` live *inside* the box it points
/// to, which this code generator has no way to know at the point it's
/// generating IR for the *outer* box: "release whatever this slot turns out
/// to point to, recursively" can only be resolved once that pointer's value
/// actually exists, at runtime. `Module::get_function` makes building this
/// idempotent: every `build-closure-release` call (and the function's own
/// body, for its self-call) shares the one definition already in `module`,
/// built lazily the first time any `ClosureBox` work happens in it.
///
/// Recursion only ever terminates because a true reference cycle between
/// two `ClosureBox`es isn't constructible under this design (see
/// `compiler.rs`'s `resolve-value` doc comment) — nothing here detects or
/// guards against one; a cyclic structure, were one ever constructed some
/// other way, would recurse until the native stack overflows.
fn get_or_define_closure_release_fn(
    module: &Rc<RefCell<Module<'static>>>,
    ctx: &'static Context,
) -> Result<FunctionValue<'static>, EvalError> {
    const RELEASE_FN_NAME: &str = "__typelisp_closure_release";
    if let Some(f) = module.borrow().get_function(RELEASE_FN_NAME) {
        return Ok(f);
    }

    let i64_ty = ctx.i64_type();
    let fn_ty = ctx.void_type().fn_type(&[i64_ty.into()], false);
    let function = module.borrow_mut().add_function(RELEASE_FN_NAME, fn_ty, None);

    let entry = ctx.append_basic_block(function, "entry");
    let not_null = ctx.append_basic_block(function, "not_null");
    let free_block = ctx.append_basic_block(function, "free");
    let loop_header = ctx.append_basic_block(function, "loop_header");
    let loop_check_fn = ctx.append_basic_block(function, "loop_check_fn");
    let loop_release = ctx.append_basic_block(function, "loop_release");
    let loop_inc = ctx.append_basic_block(function, "loop_inc");
    let loop_exit = ctx.append_basic_block(function, "loop_exit");
    let cont_block = ctx.append_basic_block(function, "cont");

    let b = ctx.create_builder();
    let err = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("closure_release: {}", e));
    let one = i64_ty.const_int(1, false);
    let zero = i64_ty.const_int(0, false);

    b.position_at_end(entry);
    let closure = function
        .get_nth_param(0)
        .ok_or_else(|| EvalError::Internal("closure_release: missing param".into()))?
        .into_int_value();
    // `release-pending-args` (automatic retain/release insertion) calls
    // this unconditionally on every slot of a "pending release" array, even
    // ones it marked with the `0` sentinel for "nothing to release here" —
    // `malloc` never returns a null pointer for a real allocation, so `0`
    // can never collide with an actual `ClosureBox`. Tolerating it here
    // keeps that caller from needing a runtime branch of its own (`compiler.rs`
    // has no general `if` to build one with).
    let is_null = b.build_int_compare(inkwell::IntPredicate::EQ, closure, zero, "closure_release_is_null").map_err(err)?;
    b.build_conditional_branch(is_null, cont_block, not_null).map_err(err)?;

    b.position_at_end(not_null);
    let box_ptr = closure_box_ptr(&b, closure.into(), "closure_release")?;
    let rc = load_closure_slot(&b, box_ptr, CLOSURE_REFCOUNT_SLOT, "closure_release")?;
    let rc2 = b.build_int_sub(rc, one, "closure_rc_dec").map_err(err)?;
    store_closure_slot(&b, box_ptr, CLOSURE_REFCOUNT_SLOT, rc2.into(), "closure_release")?;
    let is_zero = b.build_int_compare(inkwell::IntPredicate::EQ, rc2, zero, "closure_rc_is_zero").map_err(err)?;
    b.build_conditional_branch(is_zero, free_block, cont_block).map_err(err)?;

    // free: scan the captured slots, recursively releasing the `Fn`-typed
    // ones, before freeing the box itself.
    b.position_at_end(free_block);
    let env_len = load_closure_slot(&b, box_ptr, CLOSURE_ENV_LEN_SLOT, "closure_release")?;
    let fn_mask = load_closure_slot(&b, box_ptr, CLOSURE_FN_MASK_SLOT, "closure_release")?;
    let idx_alloca = b.build_alloca(i64_ty, "release_idx").map_err(err)?;
    b.build_store(idx_alloca, zero).map_err(err)?;
    b.build_unconditional_branch(loop_header).map_err(err)?;

    b.position_at_end(loop_header);
    let i_val = b.build_load(i64_ty, idx_alloca, "release_i").map_err(err)?.into_int_value();
    let in_range = b.build_int_compare(inkwell::IntPredicate::ULT, i_val, env_len, "release_i_lt_len").map_err(err)?;
    b.build_conditional_branch(in_range, loop_check_fn, loop_exit).map_err(err)?;

    b.position_at_end(loop_check_fn);
    let shifted = b.build_right_shift(fn_mask, i_val, false, "release_mask_shifted").map_err(err)?;
    let bit = b.build_and(shifted, one, "release_mask_bit").map_err(err)?;
    let is_fn = b.build_int_compare(inkwell::IntPredicate::EQ, bit, one, "release_is_fn").map_err(err)?;
    b.build_conditional_branch(is_fn, loop_release, loop_inc).map_err(err)?;

    b.position_at_end(loop_release);
    let slot_idx = b.build_int_add(i64_ty.const_int(CLOSURE_HEADER_SLOTS, false), i_val, "release_slot_idx").map_err(err)?;
    let slot_ptr = closure_slot_ptr_dyn(&b, box_ptr, slot_idx)?;
    let captured_val = b.build_load(i64_ty, slot_ptr, "release_captured_val").map_err(err)?;
    b.build_call(function, &[captured_val.into()], "").map_err(err)?;
    b.build_unconditional_branch(loop_inc).map_err(err)?;

    b.position_at_end(loop_inc);
    let i_next = b.build_int_add(i_val, one, "release_i_next").map_err(err)?;
    b.build_store(idx_alloca, i_next).map_err(err)?;
    b.build_unconditional_branch(loop_header).map_err(err)?;

    b.position_at_end(loop_exit);
    b.build_free(box_ptr).map_err(err)?;
    b.build_unconditional_branch(cont_block).map_err(err)?;

    b.position_at_end(cont_block);
    b.build_return(None).map_err(err)?;

    Ok(function)
}

/// `build-closure-release`: a thin wrapper emitting one
/// `call void @__typelisp_closure_release(i64 closure)` against the shared,
/// self-recursive function [`get_or_define_closure_release_fn`] builds (and
/// memoizes) in `module` — see that function's doc comment for why the
/// actual decrement/cascade logic has to live there rather than here.
fn llvm_builder_build_closure_release(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let closure = expect_llvm_value(&args[2])?;
    let ctx = crate::compile::llvm_context();
    let release_fn = get_or_define_closure_release_fn(module, ctx)?;
    let b = builder.borrow();
    b.build_call(release_fn, &[closure.into()], "")
        .map_err(|e| EvalError::Internal(format!("build-closure-release: {}", e)))?;
    Ok(RtValue::Unit)
}

/// Test-only observation hook: reads a `ClosureBox`'s refcount slot
/// straight out, as an ordinary `llvm-value` the caller can `build-ret`/
/// inspect — never called from `compiler.rs` itself, only from
/// `tests/compile_test.rs` to verify the automatic retain/release insertion
/// work actually keeps a box's refcount at the expected value at a given
/// point, rather than only checking "didn't crash" (this builtin's whole
/// reason for existing — there's no other way to observe a refcount that
/// isn't itself a behavior change to the compiled code under test).
fn llvm_builder_debug_closure_refcount(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let closure = expect_llvm_value(&args[1])?;
    let b = builder.borrow();
    let box_ptr = closure_box_ptr(&b, closure, "debug-closure-refcount")?;
    let rc = load_closure_slot(&b, box_ptr, CLOSURE_REFCOUNT_SLOT, "debug-closure-refcount")?;
    Ok(RtValue::LlvmValue(rc.into()))
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
/// `llvm-value`) can dereference it. The generic, builtin-exposed
/// counterpart of [`closure_box_ptr`] — `compiler.rs`'s `compile-field-get`/
/// `compile-field-set` need it directly, unlike `ClosureBox`'s own
/// fixed-shape accessors.
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

/// `Some(v)`/`None` as an `RtValue::Data`, matching `option_def`'s variant
/// order (`some` = 0, `none` = 1).
fn option_value(v: Option<RtValue>) -> RtValue {
    match v {
        Some(x) => RtValue::Data { type_name: Path::root("option"), variant: 0, fields: vec![x] },
        None => RtValue::Data { type_name: Path::root("option"), variant: 1, fields: vec![] },
    }
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
    Ok(option_value(heap.hashtable_get(id, key).map(|v| decode_field_typed(heap, v, &val_ty))))
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
    Ok(option_value(heap.hashtable_remove(id, key).map(|v| decode_field_typed(heap, v, &val_ty))))
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
        RtValue::Sexpr(v) => Ok(*v),
        other => Err(EvalError::Internal(format!(
            "struct field: {:?} is not yet representable in the boxed struct representation",
            other
        ))),
    }
}

/// Decodes a `mem::Value` read out of a `BoxedObj::Struct` field (or
/// `Vector<T>` element, `HashTable<K,V>` value) as the slot's *declared*
/// type directs — `rtvalue_to_struct_field`'s inverse, and the type-driven
/// replacement for the value-shape heuristic that existed under type-erased
/// generic evaluation. Post-monomorphization every reader has the concrete
/// declared type in hand (an accessor's `FieldGet` node type, a builtin
/// call site's checked return type), so the one genuine ambiguity — a
/// `Sexpr`-declared slot holding a scalar datum, where a stored quoted `42`
/// is a `Value::Int` that must come back as the `Sexpr` it is, not as
/// `RtValue::Int` — is decided statically here, never guessed from the
/// value.
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
fn vector_get(heap: &Heap, args: &[RtValue], ret_ty: &Type) -> Result<RtValue, EvalError> {
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

fn scope_get(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let scope = expect_scope(&args[0])?;
    let name = expect_str(&args[1])?;
    Ok(option_value(scope.get(name)))
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

fn scope_get_heap(heap: &Heap, args: &[RtValue]) -> Result<RtValue, EvalError> {
    let id = expect_struct_box(&args[0])?;
    let name = expect_str(&args[1])?;
    Ok(option_value(heap.scope_get(id, name).map(RtValue::Sexpr)))
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

fn char_eq(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_char(&args[0])? == expect_char(&args[1])?))
}

fn char_lt(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_char(&args[0])? < expect_char(&args[1])?))
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
    if let (Value::Boxed(ia), Value::Boxed(ib)) = (a, b) {
        if heap.is_float(ia) && heap.is_float(ib) {
            return Ok(RtValue::Bool(heap.float_value(ia) == heap.float_value(ib)));
        }
    }
    Ok(RtValue::Bool(a == b))
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
            RtValue::Data { variant: vv, fields, .. } if vv == variant && fields.len() == args.len() => {
                let mut binds = Vec::new();
                for (p, f) in args.iter().zip(fields.iter()) {
                    binds.extend(match_pattern(heap, p, f)?);
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
