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
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;

use crate::{Expr, Heap, MacroExpander, Path, Pattern, QuotedSexpr, TopLevel, Type, Typed, Value};

use super::value::{Closure, EvalError, HashKey, RtValue, StructData};

/// A registered function or method body with its parameter names.
struct FnDef {
    /// Parameter names, including the receiver name first for instance methods.
    params: Vec<String>,
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

/// A mutable variable slot (shared so `setf` mutations are visible to every
/// holder of the binding, e.g. across `loop` iterations).
type Slot = Rc<RefCell<RtValue>>;

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
    /// Every mutable slot ever created, held weakly. A slot stays discoverable
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
    /// Functions JIT-compiled by `(compile "name")` (see
    /// [`Self::compile_function`]), by their fully-qualified `Path`.
    /// `RefCell` because `compile` is itself an ordinary builtin reached
    /// through `eval`'s `&self` — the same internal-mutability pattern
    /// `slots` above already uses. `Expr::Call`'s eval arm checks here
    /// first, before falling back to the tree-walking `fns` entry.
    compiled: RefCell<HashMap<Path, crate::compile::CompiledFn>>,
}

impl Interp {
    pub fn new() -> Interp {
        Interp {
            fns: HashMap::new(),
            methods: HashMap::new(),
            globals: HashMap::new(),
            slots: RefCell::new(Vec::new()),
            rooted: Cell::new(0),
            gensym_counter: Cell::new(0),
            compiled: RefCell::new(HashMap::new()),
        }
    }

    /// Wrap `v` in a fresh mutable slot and register it (weakly) for GC
    /// rooting purposes. Every binding site (`let`, parameters, closure
    /// capture, `match` bindings, globals) goes through here.
    fn slot(&self, v: RtValue) -> Slot {
        let s = Rc::new(RefCell::new(v));
        self.slots.borrow_mut().push(Rc::downgrade(&s));
        s
    }

    /// Recompute the cons heap's root set from every `Sexpr` value reachable
    /// through a currently-live slot. Must be called right before any
    /// operation that might allocate a cons cell (i.e. [`Heap::cons`]), since
    /// otherwise a GC during evaluation could reclaim a cons cell still
    /// referenced from a local, global, or closure.
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
    pub fn exec(&mut self, heap: &mut Heap, tl: TopLevel) -> Result<Option<RtValue>, EvalError> {
        match tl {
            TopLevel::Defun { name, params, ret, body, .. } => {
                let (names, types): (Vec<String>, Vec<Type>) = params.into_iter().unzip();
                self.fns.insert(name, FnDef { params: names, body, rest: false, sig: Some((types, ret)) });
                Ok(None)
            }
            TopLevel::Defmethod { type_name, method, self_name, params, ret, body, .. } => {
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
                self.methods.insert((type_name, method), FnDef { params: names, body, rest: false, sig: Some((types, ret)) });
                Ok(None)
            }
            TopLevel::Defmacro { name, params, body, rest } => {
                // A macro's body is callable exactly like a `defun`'s — see
                // `MacroExpander`/`Self::expand_macro` — so it's stored in
                // the very same `fns` table; no separate macro table exists.
                self.fns.insert(name, FnDef { params, body, rest, sig: None });
                Ok(None)
            }
            TopLevel::Use { .. } => Ok(None),
            // The type was already registered in the checker's `Registry` at
            // check time; there's nothing for the interpreter to do, the
            // same as `Option`/`Result` needing no runtime registration of
            // their own — see `TopLevel::Defstruct`'s doc comment.
            TopLevel::Defstruct { .. } => Ok(None),
            TopLevel::Defvar { name, value, .. } => {
                let v = self.eval(heap, &value, &Env::new())?;
                self.globals.insert(name, self.slot(v));
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
            Expr::Str(s) => Ok(RtValue::Str(s.clone())),
            Expr::Unit => Ok(RtValue::Unit),
            Expr::Var(n) => env_get(env, n)
                .map(|s| s.borrow().clone())
                .ok_or_else(|| EvalError::Unbound(n.clone())),
            Expr::Global(path) => self
                .globals
                .get(path)
                .map(|s| s.borrow().clone())
                .ok_or_else(|| EvalError::Unbound(path.to_string())),
            Expr::FnRef(path) => Ok(match self.fns.get(path) {
                // Reify a user function as a closure with no captured environment.
                Some(f) => RtValue::Closure(Rc::new(Closure {
                    params: f.params.clone(),
                    body: f.body.clone(),
                    env: Vec::new(),
                })),
                // Otherwise a built-in operator (lives at the root, simple path).
                None => RtValue::Builtin(path.local().to_string()),
            }),
            Expr::MethodRef { type_name, method } => {
                Ok(match self.methods.get(&(type_name.clone(), method.clone())) {
                    Some(m) => RtValue::Closure(Rc::new(Closure {
                        params: m.params.clone(),
                        body: m.body.clone(),
                        env: Vec::new(),
                    })),
                    None => RtValue::BuiltinMethod(type_name.clone(), method.clone()),
                })
            }
            Expr::If(c, then, els) => match self.eval(heap, c, env)? {
                RtValue::Bool(true) => self.eval(heap, then, env),
                RtValue::Bool(false) => self.eval(heap, els, env),
                _ => Err(EvalError::Internal("if condition is not a bool".into())),
            },
            Expr::Let(binds, body) => {
                // CL `let`: binding values are evaluated in the outer environment.
                let mut child = env.clone();
                for (name, val) in binds {
                    let v = self.eval(heap, val, env)?;
                    child.push((name.clone(), self.slot(v)));
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
                let mut child = env.clone();
                let slots: Vec<Slot> = defs.iter().map(|_| self.slot(RtValue::Unit)).collect();
                for ((name, _, _), slot) in defs.iter().zip(&slots) {
                    child.push((name.clone(), slot.clone()));
                }
                for ((_, params, fbody), slot) in defs.iter().zip(&slots) {
                    let names = params.iter().map(|(n, _)| n.clone()).collect();
                    *slot.borrow_mut() = RtValue::Closure(Rc::new(Closure {
                        params: names,
                        body: fbody.clone(),
                        env: child.clone(),
                    }));
                }
                self.eval_seq(heap, body, &child)
            }
            Expr::Call(name, args) => {
                let (argv, _slots) = self.eval_args(heap, args, env)?;
                // A `(compile "name")`d function dispatches to native code
                // first — checked ahead of `fns` so a later recompile (not
                // possible yet, but the ordering is the cheap-to-get-right
                // choice) would naturally take precedence over the
                // tree-walked body.
                if let Some(compiled) = self.compiled.borrow().get(name) {
                    let int_args = argv
                        .iter()
                        .map(|v| match v {
                            RtValue::Int(n) => Ok(*n),
                            other => {
                                Err(EvalError::Internal(format!("compiled call: expected an Int argument, got {:?}", other)))
                            }
                        })
                        .collect::<Result<Vec<i64>, EvalError>>()?;
                    return Ok(RtValue::Int(compiled.call(&int_args)));
                }
                if let Some(f) = self.fns.get(name) {
                    self.apply(heap, &f.params, &f.body, argv)
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
                if let Some(m) = self.methods.get(&(type_name.clone(), method.clone())) {
                    self.apply(heap, &m.params, &m.body, argv)
                } else {
                    match eval_builtin_method(type_name, method, &argv) {
                        Some(result) => result,
                        None => Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, method))),
                    }
                }
            }
            Expr::Construct { type_name, variant, args, mutable } => {
                if is_sexpr_type(type_name) {
                    self.construct_sexpr(heap, *variant, args, env)
                } else if *mutable {
                    let (fields, _slots) = self.eval_args(heap, args, env)?;
                    Ok(RtValue::Struct(Rc::new(RefCell::new(StructData {
                        type_name: type_name.to_string(),
                        fields,
                    }))))
                } else {
                    let (fields, _slots) = self.eval_args(heap, args, env)?;
                    Ok(RtValue::Data { type_name: type_name.clone(), variant: *variant, fields })
                }
            }
            Expr::Lambda { params, body } => {
                // Capture the current environment (shared slots) for the closure.
                let names = params.iter().map(|(n, _)| n.clone()).collect();
                Ok(RtValue::Closure(Rc::new(Closure {
                    params: names,
                    body: body.clone(),
                    env: env.clone(),
                })))
            }
            Expr::Apply(callee, args) => {
                let f = self.eval(heap, callee, env)?;
                let (argv, _slots) = self.eval_args(heap, args, env)?;
                match f {
                    RtValue::Closure(c) => {
                        if c.params.len() != argv.len() {
                            return Err(EvalError::Internal("closure arity mismatch".into()));
                        }
                        let mut cenv = c.env.clone();
                        for (n, v) in c.params.iter().zip(argv) {
                            cenv.push((n.clone(), self.slot(v)));
                        }
                        self.eval_seq(heap, &c.body, &cenv)
                    }
                    RtValue::Builtin(name) => match self.eval_builtin(heap, &name, &argv) {
                        Some(r) => r,
                        None => Err(EvalError::NoSuchFunction(name)),
                    },
                    RtValue::BuiltinMethod(type_name, method) => {
                        match eval_builtin_method(&type_name, &method, &argv) {
                            Some(r) => r,
                            None => Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, method))),
                        }
                    }
                    _ => Err(EvalError::Internal("apply of a non-function value".into())),
                }
            }
            Expr::FieldGet(obj, idx) => match self.eval(heap, obj, env)? {
                RtValue::Struct(s) => Ok(s.borrow().fields[*idx].clone()),
                other => Err(EvalError::Internal(format!("FieldGet on a non-Struct value: {:?}", other))),
            },
            Expr::FieldSet(obj, idx, value) => match self.eval(heap, obj, env)? {
                RtValue::Struct(s) => {
                    let v = self.eval(heap, value, env)?;
                    s.borrow_mut().fields[*idx] = v;
                    Ok(RtValue::Unit)
                }
                other => Err(EvalError::Internal(format!("FieldSet on a non-Struct value: {:?}", other))),
            },
            Expr::Match(scrut, arms) => {
                let v = self.eval(heap, scrut, env)?;
                for arm in arms {
                    if let Some(binds) = match_pattern(heap, &arm.pat, &v) {
                        let mut child = env.clone();
                        child.extend(binds.into_iter().map(|(n, v)| (n, self.slot(v))));
                        return self.eval_seq(heap, &arm.body, &child);
                    }
                }
                Err(EvalError::Internal("no matching match arm".into()))
            }
            Expr::Set(name, value) => {
                let v = self.eval(heap, value, env)?;
                let cell = env_get(env, name).ok_or_else(|| EvalError::Unbound(name.clone()))?;
                *cell.borrow_mut() = v.clone();
                Ok(v)
            }
            Expr::SetGlobal(path, value) => {
                let v = self.eval(heap, value, env)?;
                let cell = self
                    .globals
                    .get(path)
                    .ok_or_else(|| EvalError::Unbound(path.to_string()))?;
                *cell.borrow_mut() = v.clone();
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
                RtValue::Str(s) => Err(EvalError::Panic(s)),
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
            SEXPR_FLOAT => Value::Float(rt_f64(&vs[0])?),
            SEXPR_CHAR => Value::Char(rt_char(&vs[0])?),
            SEXPR_BOOL => Value::Bool(rt_bool(&vs[0])?),
            SEXPR_SYM => heap.intern_symbol(&rt_str(vs[0].clone())?),
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

    /// Apply a function/method body: bind `params` to `args` and run the body.
    fn apply(
        &self,
        heap: &mut Heap,
        params: &[String],
        body: &[Typed],
        args: Vec<RtValue>,
    ) -> Result<RtValue, EvalError> {
        if params.len() != args.len() {
            return Err(EvalError::Internal("arity mismatch".into()));
        }
        let env: Env = params.iter().cloned().zip(args.into_iter().map(|v| self.slot(v))).collect();
        self.eval_seq(heap, body, &env)
    }

    /// Looks up `name`'s registered `defun`/`defmethod` body, enforcing the
    /// two constraints every entry point into the compiler shares: a real
    /// type signature (so an LLVM function type can be built — never set for
    /// a `defmacro`) and a single-expression body (`compile`'s long-standing
    /// scope, unchanged by labels/closures Stage 3). Shared by
    /// [`Self::add_compiled_function`] (the actual AST-bridge step) and
    /// [`Self::compile_function`] (which needs the body slightly earlier —
    /// to collect `Expr::Call` targets, see that method's doc comment —
    /// before `add_compiled_function` ever runs).
    fn compiled_fn_body(&self, name: &str) -> Result<(Vec<String>, Typed), EvalError> {
        let path = Path::root(name);
        let f = self.fns.get(&path).ok_or_else(|| EvalError::NoSuchFunction(name.to_string()))?;
        if f.sig.is_none() {
            return Err(EvalError::Panic(format!("compile: \"{}\" has no type signature (is it a defmacro?)", name)));
        }
        if f.body.len() != 1 {
            return Err(EvalError::Panic(format!(
                "compile: \"{}\" has a multi-expression body, not yet supported",
                name
            )));
        }
        Ok((f.params.clone(), f.body[0].clone()))
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

        // Builds `(a b ...)`, the `Sexpr` symbol list `compiler.rs`'s
        // `bind-params` walks to know which logical argument-array slot
        // binds to which name.
        let mut param_list = Value::Empty;
        for n in params.iter().rev() {
            let sym = heap.intern_symbol(n);
            heap.push_root(sym);
            heap.push_root(param_list);
            let next = heap.cons(sym, param_list);
            heap.pop_root();
            heap.pop_root();
            param_list = next.map_err(|e| EvalError::Panic(e.to_string()))?;
        }
        heap.push_root(param_list);
        let body_sexpr = match crate::compile::ast_bridge::ast_to_sexpr(heap, &body) {
            Ok(v) => v,
            Err(e) => {
                heap.pop_root(); // param_list
                return Err(EvalError::Panic(e.to_string()));
            }
        };
        heap.pop_root(); // param_list

        let compiler_path = Path::root("compile-function");
        let (compiler_params, compiler_body) = {
            let f = self.fns.get(&compiler_path).ok_or_else(|| {
                EvalError::Internal("compile: compiler body not loaded — call load_compiler first".into())
            })?;
            (f.params.clone(), f.body.clone())
        };
        self.apply(
            heap,
            &compiler_params,
            &compiler_body,
            vec![
                RtValue::LlvmModule(module),
                RtValue::Str(internal_name.to_string()),
                RtValue::Sexpr(param_list),
                RtValue::Sexpr(body_sexpr),
            ],
        )?;
        Ok(())
    }

    /// `(compile "fn-name")`: JIT-compiles a previously-defined `defun` and
    /// registers the result in [`Self::compiled`] so `Expr::Call` dispatches
    /// to native code instead of tree-walking it from then on. See
    /// [`Self::add_compiled_function`] for the supported-shape scope.
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
    fn compile_function(&self, heap: &mut Heap, name: &str) -> Result<RtValue, EvalError> {
        let path = Path::root(name);
        let (_, body) = self.compiled_fn_body(name)?;
        let call_targets: Vec<Path> =
            crate::compile::ast_bridge::collect_call_targets(&body).into_iter().filter(|p| *p != path).collect();
        for target in &call_targets {
            if !self.compiled.borrow().contains_key(target) {
                return Err(EvalError::Panic(format!(
                    "compile: \"{}\" calls \"{}\", which must be `compile`d first",
                    name,
                    target.local()
                )));
            }
        }

        let module = {
            let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
            let module = Rc::new(RefCell::new(crate::compile::llvm_context().create_module("compiled")));
            for target in &call_targets {
                declare_external_function(&module, target.local());
            }
            module
        };
        self.add_compiled_function(heap, module.clone(), name, name)?;

        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let externals: Vec<(String, usize)> = {
            let compiled = self.compiled.borrow();
            call_targets
                .iter()
                .map(|p| (p.local().to_string(), compiled.get(p).expect("checked compiled above").address()))
                .collect()
        };
        let compiled = crate::compile::CompiledFn::new(&module.borrow(), name, &externals)
            .map_err(|e| EvalError::Panic(format!("compile: JIT failed: {}", e)))?;
        self.compiled.borrow_mut().insert(path, compiled);
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
            slots.push(self.slot(v.clone()));
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
            "not" => Some(expect_bool(&args[0]).map(|b| RtValue::Bool(!b))),
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
            Ok(argv) => self.apply(heap, &f.params, &f.body, argv),
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
        QuotedSexpr::Float(f) => Ok(Value::Float(*f)),
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

/// Recursively gather every `Sexpr` value reachable from `v` through nested
/// `Data` fields or `HashTable` values (a `Closure`'s captured environment is
/// covered separately, since each of its slots is already registered in
/// [`Interp::slots`]). `HashTable` keys never need walking — [`HashKey`] is
/// restricted to scalar variants that can't carry a `Sexpr`.
fn collect_sexpr_roots(v: &RtValue, out: &mut Vec<Value>) {
    match v {
        RtValue::Sexpr(val) => out.push(*val),
        RtValue::Data { fields, .. } => {
            for f in fields {
                collect_sexpr_roots(f, out);
            }
        }
        RtValue::HashTable(map) => {
            for f in map.borrow().values() {
                collect_sexpr_roots(f, out);
            }
        }
        RtValue::Struct(s) => {
            for f in &s.borrow().fields {
                collect_sexpr_roots(f, out);
            }
        }
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
        RtValue::Str(s) => Ok(s),
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
/// rather than re-checking shape.
fn eval_builtin_method(type_name: &Path, method: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    if *type_name == Path::root("hashtable") {
        return match method {
            "new" => Some(Ok(RtValue::HashTable(Rc::new(RefCell::new(HashMap::new()))))),
            "get" => Some(hashtable_get(args)),
            "set" => Some(hashtable_set(args)),
            "remove" => Some(hashtable_remove(args)),
            "count" => Some(hashtable_count(args)),
            "clear" => Some(hashtable_clear(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("string") {
        return match method {
            "upcase" => Some(expect_str(&args[0]).map(|s| RtValue::Str(s.to_ascii_uppercase()))),
            "downcase" => Some(expect_str(&args[0]).map(|s| RtValue::Str(s.to_ascii_lowercase()))),
            "length" => Some(string_length(args)),
            "ref" => Some(string_ref(args)),
            "substring" => Some(string_substring(args)),
            "append" => Some(string_append(args)),
            "eq" => Some(string_eq(args)),
            "lt" => Some(string_lt(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("char") {
        return match method {
            "upcase" => Some(expect_char(&args[0]).map(|c| RtValue::Char(c.to_ascii_uppercase()))),
            "downcase" => Some(expect_char(&args[0]).map(|c| RtValue::Char(c.to_ascii_lowercase()))),
            "eq" => Some(char_eq(args)),
            "lt" => Some(char_lt(args)),
            "alphap" => Some(expect_char(&args[0]).map(|c| RtValue::Bool(c.is_ascii_alphabetic()))),
            "digitp" => Some(expect_char(&args[0]).map(|c| RtValue::Bool(c.is_ascii_digit()))),
            _ => None,
        };
    }
    if *type_name == Path::root("i32") || *type_name == Path::root("i64") {
        return match method {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                eval_int_builtin(method, args)
            }
            // `eq` is registered as an alias for `=` (see `registry::int_assoc`).
            "eq" => eval_int_builtin("=", args),
            _ => None,
        };
    }
    if *type_name == Path::root("f64") {
        return match method {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                eval_float_builtin(method, args)
            }
            "eq" => eval_float_builtin("=", args),
            "expt" => Some(float_expt(args)),
            "sqrt" => Some(float_unary(args, f64::sqrt)),
            "floor" => Some(float_unary(args, f64::floor)),
            "ceiling" => Some(float_unary(args, f64::ceil)),
            "round" => Some(float_unary(args, f64::round)),
            "truncate" => Some(float_unary(args, f64::trunc)),
            _ => None,
        };
    }
    if *type_name == Path::root("bool") {
        return match method {
            "eq" => Some(bool_eq(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("sexpr") {
        return match method {
            "eq" => Some(sexpr_eq(args)),
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
            "alloca-args" => Some(llvm_builder_alloca_args(args)),
            "store-arg" => Some(llvm_builder_store_arg(args)),
            "build-call" => Some(llvm_builder_build_call(args)),
            "load-env" => Some(llvm_builder_load_env(args)),
            "build-call-with-env" => Some(llvm_builder_build_call_with_env(args)),
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

/// The captures counterpart of [`llvm_module_add_function`]: `i64
/// name(i64* args, i32 argc, i64* env, i32 env_len)` — used for a `labels`
/// sibling whenever its block's shared captured-name list
/// (`compile::freevars::labels_free_vars`) is non-empty. `llvm-builder::load-env`
/// reads a logical captured slot back out of `env`, the same way `load-arg`
/// reads a logical parameter out of `args`.
fn llvm_module_add_function_with_env(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(&args[1])?;
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let fn_type =
        ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into(), ptr_ty.into(), ctx.i32_type().into()], false);
    let function = module.borrow_mut().add_function(name, fn_type, None);
    Ok(RtValue::LlvmFunction(function))
}

fn llvm_module_verify(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    Ok(RtValue::Bool(module.borrow().verify().is_ok()))
}

fn llvm_module_to_string(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    Ok(RtValue::Str(module.borrow().print_to_string().to_string()))
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

fn expect_hashtable(v: &RtValue) -> Result<&Rc<RefCell<HashMap<HashKey, RtValue>>>, EvalError> {
    match v {
        RtValue::HashTable(m) => Ok(m),
        other => Err(EvalError::Internal(format!("expected a HashTable, got {:?}", other))),
    }
}

/// `Some(v)`/`None` as an `RtValue::Data`, matching `option_def`'s variant
/// order (`some` = 0, `none` = 1).
fn option_value(v: Option<RtValue>) -> RtValue {
    match v {
        Some(x) => RtValue::Data { type_name: Path::root("option"), variant: 0, fields: vec![x] },
        None => RtValue::Data { type_name: Path::root("option"), variant: 1, fields: vec![] },
    }
}

fn hashtable_get(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let map = expect_hashtable(&args[0])?;
    let key = HashKey::from_rtvalue(&args[1])?;
    Ok(option_value(map.borrow().get(&key).cloned()))
}

fn hashtable_set(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let map = expect_hashtable(&args[0])?;
    let key = HashKey::from_rtvalue(&args[1])?;
    map.borrow_mut().insert(key, args[2].clone());
    Ok(RtValue::Unit)
}

fn hashtable_remove(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let map = expect_hashtable(&args[0])?;
    let key = HashKey::from_rtvalue(&args[1])?;
    Ok(option_value(map.borrow_mut().remove(&key)))
}

fn hashtable_count(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let map = expect_hashtable(&args[0])?;
    Ok(RtValue::Int(map.borrow().len() as i64))
}

fn hashtable_clear(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let map = expect_hashtable(&args[0])?;
    map.borrow_mut().clear();
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
        RtValue::Str(s) => Ok(s.as_str()),
        other => Err(EvalError::Internal(format!("expected a Str, got {:?}", other))),
    }
}

fn expect_char(v: &RtValue) -> Result<char, EvalError> {
    match v {
        RtValue::Char(c) => Ok(*c),
        other => Err(EvalError::Internal(format!("expected a Char, got {:?}", other))),
    }
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
    Ok(RtValue::Str(chars[start as usize..end as usize].iter().collect()))
}

fn string_append(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Str(format!("{}{}", expect_str(&args[0])?, expect_str(&args[1])?)))
}

fn string_eq(args: &[RtValue]) -> Result<RtValue, EvalError> {
    Ok(RtValue::Bool(expect_str(&args[0])? == expect_str(&args[1])?))
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

/// Try to match a pattern against a value, returning the bindings on success.
/// `heap` is needed to destructure `Sexpr` values (`RtValue::Sexpr`), which
/// hold their `Cons`/`Str`/`Sym` payloads in the cons heap.
fn match_pattern(heap: &Heap, pat: &Pattern, v: &RtValue) -> Option<Vec<(String, RtValue)>> {
    match pat {
        Pattern::Wildcard => Some(Vec::new()),
        Pattern::Bind(n) => Some(vec![(n.clone(), v.clone())]),
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
        Pattern::Ctor { variant, args, .. } => match v {
            RtValue::Data { variant: vv, fields, .. } if vv == variant && fields.len() == args.len() => {
                let mut binds = Vec::new();
                for (p, f) in args.iter().zip(fields.iter()) {
                    binds.extend(match_pattern(heap, p, f)?);
                }
                Some(binds)
            }
            // A `defstruct` has exactly one variant (`"new"`, index 0), so
            // `variant` always matches here — only the field count/pattern
            // shape can fail. Reads a clone of each field out of the
            // `RefCell`, same as `Expr::FieldGet`.
            RtValue::Struct(s) if *variant == 0 && s.borrow().fields.len() == args.len() => {
                let mut binds = Vec::new();
                for (p, f) in args.iter().zip(s.borrow().fields.iter()) {
                    binds.extend(match_pattern(heap, p, f)?);
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
fn match_sexpr_ctor(heap: &Heap, variant: usize, args: &[Pattern], v: Value) -> Option<Vec<(String, RtValue)>> {
    match (variant, v) {
        (SEXPR_NIL, Value::Empty) => Some(Vec::new()),
        (SEXPR_INT, Value::Int(n)) => match_pattern(heap, &args[0], &RtValue::Int(n)),
        (SEXPR_FLOAT, Value::Float(f)) => match_pattern(heap, &args[0], &RtValue::Float(f)),
        (SEXPR_CHAR, Value::Char(c)) => match_pattern(heap, &args[0], &RtValue::Char(c)),
        (SEXPR_BOOL, Value::Bool(b)) => match_pattern(heap, &args[0], &RtValue::Bool(b)),
        (SEXPR_SYM, Value::Symbol(id)) => {
            match_pattern(heap, &args[0], &RtValue::Str(heap.symbol_name(id).to_string()))
        }
        (SEXPR_STR, Value::Str(id)) => {
            match_pattern(heap, &args[0], &RtValue::Str(heap.string(id).to_string()))
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
