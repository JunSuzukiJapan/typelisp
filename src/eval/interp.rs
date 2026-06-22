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

use crate::{Expr, Heap, MacroExpander, Path, Pattern, QuotedSexpr, TopLevel, Type, Typed, Value};
#[cfg(feature = "compile")]
use crate::ConsRef;

use super::value::{Closure, EvalError, HashKey, RtValue};
#[cfg(feature = "compile")]
use super::value::LlvmBuilderHandle;

/// A registered function or method body with its parameter names.
struct FnDef {
    /// Parameter names, including the receiver name first for instance methods.
    params: Vec<String>,
    body: Vec<Typed>,
    /// Only ever set for a `defmacro` with a trailing `&rest` parameter (see
    /// [`Interp::expand_macro`]); always `false` for `defun`/`defmethod`,
    /// which `apply` calls 1:1 regardless.
    rest: bool,
    /// Parameter/return types, for a `defun` only (`None` for `defmethod`
    /// — its receiver type isn't included in `params` — and `defmacro`,
    /// whose parameters are always `Sexpr` with no per-parameter type
    /// recorded). Lets [`Interp::fn_signature`] hand the compiler's
    /// typed-AST bridge (`crate::compile::ast_bridge`) a function's
    /// signature without `Interp` needing a reference to the checker's
    /// `Registry`.
    sig: Option<(Vec<Type>, Type)>,
}

/// A mutable variable slot (shared so `setf` mutations are visible to every
/// holder of the binding, e.g. across `while` iterations).
type Slot = Rc<RefCell<RtValue>>;

/// A lexical environment: name -> slot, searched from the back (innermost).
type Env = Vec<(String, Slot)>;

/// The outcome of evaluating one step inside a loop body: either a plain
/// value, or a `break`/`return` already resolved to the value the loop should
/// exit with (see [`Interp::eval_loop_step`]).
enum Step {
    Value(RtValue),
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
    /// Functions JIT-compiled by `compile` (`crate::compile::compiler_source`),
    /// by their fully-qualified [`Path`] — checked first in `Expr::Call`'s
    /// dispatch (see [`Self::call_compiled`]) so a compiled function is
    /// called exactly like any other, transparently to its callers.
    #[cfg(feature = "compile")]
    compiled: RefCell<HashMap<Path, CompiledFn>>,
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
            #[cfg(feature = "compile")]
            compiled: RefCell::new(HashMap::new()),
        }
    }

    /// The checked body of a `defun` (`None` for a `defmethod`/`defmacro` of
    /// the same name, or if no such function exists) — used by the
    /// compiler's typed-AST bridge (`crate::compile::ast_bridge`) to walk a
    /// function without `Interp` exposing its internal `FnDef` representation.
    pub fn fn_body(&self, path: &Path) -> Option<&[Typed]> {
        self.fns.get(path).map(|f| f.body.as_slice())
    }

    /// A `defun`'s parameter names alongside its parameter/return types
    /// (`None` for a `defmethod`/`defmacro` of the same name, or if no such
    /// function exists — see [`FnDef::sig`]).
    pub fn fn_signature(&self, path: &Path) -> Option<(&[String], &[Type], &Type)> {
        let f = self.fns.get(path)?;
        let (params, ret) = f.sig.as_ref()?;
        Some((&f.params, params, ret))
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
                let sig = Some((params.iter().map(|(_, t)| t.clone()).collect(), ret));
                let params = params.into_iter().map(|(n, _)| n).collect();
                self.fns.insert(name, FnDef { params, body, rest: false, sig });
                Ok(None)
            }
            TopLevel::Defmethod { type_name, method, self_name, params, body, .. } => {
                let mut names: Vec<String> = Vec::new();
                if let Some(s) = self_name {
                    names.push(s);
                }
                names.extend(params.into_iter().map(|(n, _)| n));
                self.methods.insert((type_name, method), FnDef { params: names, body, rest: false, sig: None });
                Ok(None)
            }
            TopLevel::Defmacro { name, params, body, rest } => {
                // A macro's body is callable exactly like a `defun`'s — see
                // `MacroExpander`/`Self::expand_macro` — so it's stored in
                // the very same `fns` table; no separate macro table exists.
                self.fns.insert(name, FnDef { params, body, rest, sig: None });
                Ok(None)
            }
            TopLevel::Defstruct { .. } | TopLevel::Use { .. } => Ok(None),
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
                #[cfg(feature = "compile")]
                if let Some(result) = self.call_compiled(heap, name, &argv) {
                    return result;
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
                    let result = eval_builtin_method(type_name, method, &argv)
                        .or_else(|| eval_llvm_builtin_method_fallback(type_name, method, &argv));
                    match result {
                        Some(result) => result,
                        None => Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, method))),
                    }
                }
            }
            Expr::Construct { type_name, variant, args } => {
                if is_sexpr_type(type_name) {
                    self.construct_sexpr(heap, *variant, args, env)
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
            Expr::Vector(elems) => {
                let (vs, _slots) = self.eval_args(heap, elems, env)?;
                Ok(RtValue::Vector(Rc::new(RefCell::new(vs))))
            }
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
            Expr::While(cond, body) => loop {
                match self.eval_loop_step(heap, cond, env)? {
                    Step::Exit(v) => return Ok(v),
                    Step::Value(RtValue::Bool(true)) => {}
                    Step::Value(RtValue::Bool(false)) => return Ok(RtValue::Unit),
                    Step::Value(_) => {
                        return Err(EvalError::Internal("while condition is not a bool".into()))
                    }
                }
                if let Some(v) = self.eval_loop_body(heap, body, env)? {
                    return Ok(v);
                }
            },
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

    /// The outcome of evaluating one step (the condition or a body
    /// expression) of a `while`/`loop`: either an ordinary value, or a
    /// `break`/`return` signal already resolved to the loop's exit value.
    fn eval_loop_step(&self, heap: &mut Heap, t: &Typed, env: &Env) -> Result<Step, EvalError> {
        match self.eval(heap, t, env) {
            Ok(v) => Ok(Step::Value(v)),
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
            #[cfg(feature = "compile")]
            other => self.eval_compile_builtin(other, args),
            #[cfg(not(feature = "compile"))]
            _ => None,
        }
    }
}

/// `compile`'s builtins (`crate::compile::compiler_source`'s LLVM-builder
/// bindings and typed-AST bridge), kept in their own `impl` block since every
/// item here is `#[cfg(feature = "compile")]` — see
/// [docs/TODO.md](../../docs/TODO.md)「ステップ5」for the overall design and
/// [`Self::call_compiled`]/[`Self::eval_compile_builtin`]'s doc comments for
/// how this plugs into the ordinary tree-walking dispatch.
#[cfg(feature = "compile")]
impl Interp {
    /// Free functions for `compile` not already handled by [`Self::eval_builtin`]
    /// (this is that method's fallback arm, only reached for an unrecognized
    /// name). `ast-params`/`ast-body` need `&self` (to read `self.fns`);
    /// `llvm-finish-compile` needs it too (to write `self.compiled`) — the
    /// pure LLVM-IR-construction free functions (`llvm-new-module` etc.) are
    /// plain functions below that happen not to need it.
    fn eval_compile_builtin(&self, name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
        match name {
            "ast-params" => Some(self.builtin_ast_params(args)),
            "ast-body" => Some(self.builtin_ast_body(args)),
            "param-is-bool" => Some(self.builtin_param_is_bool(args)),
            "param-is-f64" => Some(self.builtin_param_is_f64(args)),
            "param-is-char" => Some(self.builtin_param_is_char(args)),
            "param-is-sexpr" => Some(self.builtin_param_is_sexpr(args)),
            "ret-is-bool" => Some(self.builtin_ret_is_bool(args)),
            "ret-is-f64" => Some(self.builtin_ret_is_f64(args)),
            "ret-is-char" => Some(self.builtin_ret_is_char(args)),
            "ret-is-sexpr" => Some(self.builtin_ret_is_sexpr(args)),
            "llvm-new-module" => Some(llvm_module_new(args)),
            "llvm-new-builder" => Some(llvm_builder_new(args)),
            "llvm-const-i64" => Some(llvm_const_i64(args)),
            "llvm-const-bool" => Some(llvm_const_bool(args)),
            "llvm-const-f64" => Some(llvm_const_f64(args)),
            "llvm-const-char" => Some(llvm_const_char(args)),
            "llvm-const-nil" => Some(llvm_const_nil(args)),
            "is-sexpr-value" => Some(llvm_value_is_sexpr(args)),
            "llvm-finish-compile" => Some(self.builtin_llvm_finish_compile(args)),
            "llvm-finish-compile-group" => Some(self.builtin_llvm_finish_compile_group(args)),
            _ => None,
        }
    }

    /// `(ast-params name) -> Option<Vector<string>>`: a `defun`'s parameter
    /// names, or `None` if `name` isn't a `defun` or its parameters/return
    /// aren't all `i64`/`bool` (Phase 2b's restricted scope — see
    /// `crate::compile::ast_bridge`).
    fn builtin_ast_params(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        let name = expect_str(&args[0])?;
        match self.fn_signature(&Path::root(name)) {
            Some((names, types, ret)) if is_supported_scalar_signature(types, ret) => {
                let v: Vec<RtValue> = names.iter().map(|n| RtValue::Str(n.clone())).collect();
                Ok(option_value(Some(RtValue::Vector(Rc::new(RefCell::new(v))))))
            }
            _ => Ok(option_value(None)),
        }
    }

    /// `(ast-body name group) -> Option<AstExpr>`: the bridged form of
    /// `name`'s (single-expression — Phase 1/2 don't support multi-form
    /// bodies, i.e. `let`-bound locals, yet) checked body, or `None` if
    /// `name` isn't a `defun`, isn't all `i64`/`bool`, has other than one
    /// body expression, or its body uses an `Expr` form
    /// `crate::compile::ast_bridge::typed_to_ast` doesn't (yet) bridge.
    /// `group` (Phase 2g, self/mutual recursion) is the list of function
    /// names being compiled together in the same module as `name` — see
    /// `typed_to_ast`'s doc comment. `compile`'s singleton call site passes
    /// `[name]` so an ordinary single-function compile gets self-recursion
    /// bridging for free; `compile-group` passes the whole group.
    fn builtin_ast_body(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        let name = expect_str(&args[0])?;
        let path = Path::root(name);
        let supported = matches!(self.fn_signature(&path), Some((_, types, ret)) if is_supported_scalar_signature(types, ret));
        if !supported {
            return Ok(option_value(None));
        }
        let group_vec = expect_vector(&args[1])?;
        let group = group_vec
            .borrow()
            .iter()
            .map(|v| expect_str(v).map(Path::root))
            .collect::<Result<Vec<_>, _>>()?;
        match self.fn_body(&path) {
            Some([only]) => Ok(option_value(crate::compile::ast_bridge::typed_to_ast(only, self, &group))),
            _ => Ok(option_value(None)),
        }
    }

    /// Whether `name` is already JIT-compiled (`self.compiled`) — checked by
    /// `crate::compile::ast_bridge::typed_to_ast` so a call to an
    /// as-yet-uncompiled function fails the AST bridge outright (Phase 2f)
    /// rather than reaching `compile-value` with no way to build IR for it.
    pub(crate) fn is_compiled(&self, name: &Path) -> bool {
        self.compiled.borrow().contains_key(name)
    }

    /// Shared by `builtin_param_is_bool`/`builtin_param_is_f64`: whether
    /// `name`'s `i`-th parameter is exactly `want` — lets the
    /// typelisp-written `compile` pick `load-arg`/`load-arg-bool`/
    /// `load-arg-f64` per parameter without `AstExpr` needing to carry
    /// per-node type tags (Phase 2b/2c, mixed `i64`/`bool`/`f64` signatures).
    fn param_has_type(&self, args: &[RtValue], want: &Type) -> Result<RtValue, EvalError> {
        let name = expect_str(&args[0])?;
        let i = rt_i64(&args[1])?;
        let matches = match self.fn_signature(&Path::root(name)) {
            Some((_, types, _)) => i >= 0 && types.get(i as usize) == Some(want),
            None => false,
        };
        Ok(RtValue::Bool(matches))
    }

    /// `(param-is-bool name i) -> bool`: see [`Self::param_has_type`].
    fn builtin_param_is_bool(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        self.param_has_type(args, &Type::Bool)
    }

    /// `(param-is-f64 name i) -> bool`: see [`Self::param_has_type`].
    fn builtin_param_is_f64(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        self.param_has_type(args, &Type::F64)
    }

    /// `(param-is-char name i) -> bool`: see [`Self::param_has_type`].
    fn builtin_param_is_char(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        self.param_has_type(args, &Type::Char)
    }

    /// `(param-is-sexpr name i) -> bool`: see [`Self::param_has_type`].
    fn builtin_param_is_sexpr(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        self.param_has_type(args, &Type::Named(Path::root("sexpr"), vec![]))
    }

    /// Shared by `builtin_ret_is_bool`/`builtin_ret_is_f64`: whether `name`'s
    /// return type is exactly `want` — same role as [`Self::param_has_type`]
    /// but for the function's own return, letting `compile` pick
    /// `build-ret`/`build-ret-bool`/`build-ret-f64` once per function.
    fn ret_has_type(&self, args: &[RtValue], want: &Type) -> Result<RtValue, EvalError> {
        let name = expect_str(&args[0])?;
        let matches = matches!(self.fn_signature(&Path::root(name)), Some((_, _, ret)) if ret == want);
        Ok(RtValue::Bool(matches))
    }

    /// `(ret-is-bool name) -> bool`: see [`Self::ret_has_type`].
    fn builtin_ret_is_bool(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        self.ret_has_type(args, &Type::Bool)
    }

    /// `(ret-is-f64 name) -> bool`: see [`Self::ret_has_type`].
    fn builtin_ret_is_f64(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        self.ret_has_type(args, &Type::F64)
    }

    /// `(ret-is-char name) -> bool`: see [`Self::ret_has_type`].
    fn builtin_ret_is_char(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        self.ret_has_type(args, &Type::Char)
    }

    /// `(ret-is-sexpr name) -> bool`: see [`Self::ret_has_type`].
    fn builtin_ret_is_sexpr(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        self.ret_has_type(args, &Type::Named(Path::root("sexpr"), vec![]))
    }

    /// Patches every bare (no-body) function declaration in `module` to the
    /// real address of whatever it refers to — another already-compiled
    /// typelisp function (`self.compiled`) or a fixed Rust runtime shim
    /// (`runtime_shim_address`) — by registering it with `engine`. Shared by
    /// [`Self::builtin_llvm_finish_compile`] and
    /// [`Self::builtin_llvm_finish_compile_group`] (Phase 2g): every callee
    /// `LlvmModule::get-or-declare-function`/`get-or-declare-runtime-fn`
    /// added for a direct `build-call` or a `Sexpr` runtime shim
    /// (`build-cons`/`build-car`/`build-cdr`/`push-root`/`pop-root`) is a
    /// bare declaration (no basic blocks) in *this* module/engine, distinct
    /// from wherever its body actually lives — another `compile` call's
    /// separate `ExecutionEngine` for a typelisp callee, or a fixed Rust
    /// function for a runtime shim. A declaration neither resolves (Phase
    /// 2g: another member of the same self/mutual-recursion group, not
    /// registered into `self.compiled` yet) is simply left unmapped here —
    /// [`Self::builtin_llvm_finish_compile_group`] patches every group
    /// member's own address in immediately afterward, before this module's
    /// code is ever actually executed.
    fn resolve_external_declarations(
        &self,
        module: &inkwell::module::Module<'static>,
        engine: &inkwell::execution_engine::ExecutionEngine<'static>,
    ) -> Result<(), EvalError> {
        for f in module.get_functions() {
            if f.count_basic_blocks() == 0 {
                let callee_name = f.get_name().to_str().map_err(|e| EvalError::Internal(e.to_string()))?;
                if let Some(cf) = self.compiled.borrow().get(&Path::root(callee_name)) {
                    engine.add_global_mapping(&f, cf.f as usize);
                } else if let Some(addr) = runtime_shim_address(callee_name) {
                    engine.add_global_mapping(&f, addr);
                }
            }
        }
        Ok(())
    }

    /// `(llvm-finish-compile module name arity) -> Result<bool, Error>`:
    /// JITs `module` (already built and verified by the typelisp-written
    /// `compile`) and registers `name` as a compiled, `arity`-ary function —
    /// see [`Self::call_compiled`]. The `ExecutionEngine` is kept alive for as
    /// long as `name` stays registered (its `Rc` is cloned into
    /// [`CompiledFn::_engine`]), since the JIT'd machine code is only valid
    /// while it lives.
    fn builtin_llvm_finish_compile(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        let _guard = compile_lock().lock().unwrap();
        let module = expect_llvm_module(&args[0])?;
        let name = expect_str(&args[1])?;
        let arity = rt_i64(&args[2])?;
        let engine = module
            .borrow()
            .create_jit_execution_engine(inkwell::OptimizationLevel::None)
            .map_err(|e| EvalError::Panic(format!("compile: {}", e)))?;
        self.resolve_external_declarations(&module.borrow(), &engine)?;
        let addr = engine
            .get_function_address(name)
            .map_err(|e| EvalError::Panic(format!("compile: {}", e)))?;
        let engine = Rc::new(engine);
        // SAFETY: `addr` is the address LLVM's JIT compiled `name`'s body to,
        // declared (by `add-function`, `check::registry::llvm_module_def`)
        // with the one unified `TlValue` ABI signature every compiled
        // function shares regardless of its logical arity —
        // `extern "C" fn(*const TlValue, u32, *mut TlValue, *mut Heap) -> i32`
        // (Phase 3 added the trailing `heap` parameter so compiled code can
        // call `cons`/etc through the runtime shims — see
        // [`llvm_module_get_or_declare_runtime_fn`]) — so transmuting it to
        // that single function-pointer type is always correct (Phase 2/3,
        // [docs/TODO.md](../../docs/TODO.md)「ステップ5」).
        let f = unsafe {
            std::mem::transmute::<usize, extern "C" fn(*const TlValue, u32, *mut TlValue, *mut Heap) -> i32>(addr)
        };
        let cf = CompiledFn { f, arity: arity as usize, _engine: engine };
        self.compiled.borrow_mut().insert(Path::root(name), cf);
        Ok(ok_bool(true))
    }

    /// `(llvm-finish-compile-group module names) -> Result<bool, Error>`
    /// (Phase 2g, self/mutual recursion): the `compile-group` counterpart to
    /// [`Self::builtin_llvm_finish_compile`]. A `Module` can only ever be
    /// handed to *one* `ExecutionEngine`, so a whole self/mutual-recursion
    /// group sharing one module (`compile-group` builds every member's body
    /// into it before this runs) must be JIT'd and registered together in a
    /// single call, rather than once per member — unlike the single-function
    /// path, `arity` isn't passed in from typelisp for each name; it's read
    /// back from `self.fn_signature`, since every name here is already a
    /// registered `defun`.
    fn builtin_llvm_finish_compile_group(&self, args: &[RtValue]) -> Result<RtValue, EvalError> {
        let _guard = compile_lock().lock().unwrap();
        let module = expect_llvm_module(&args[0])?;
        let names_vec = expect_vector(&args[1])?;
        let names = names_vec.borrow().iter().map(|v| expect_str(v).map(|s| s.to_string())).collect::<Result<Vec<_>, _>>()?;
        let engine = module
            .borrow()
            .create_jit_execution_engine(inkwell::OptimizationLevel::None)
            .map_err(|e| EvalError::Panic(format!("compile-group: {}", e)))?;
        self.resolve_external_declarations(&module.borrow(), &engine)?;
        let engine = Rc::new(engine);
        for name in &names {
            let arity = self
                .fn_signature(&Path::root(name))
                .map(|(_, types, _)| types.len())
                .ok_or_else(|| EvalError::Internal(format!("compile-group: unknown function {}", name)))?;
            let addr = engine
                .get_function_address(name)
                .map_err(|e| EvalError::Panic(format!("compile-group: {}", e)))?;
            // SAFETY: see `builtin_llvm_finish_compile`.
            let f = unsafe {
                std::mem::transmute::<usize, extern "C" fn(*const TlValue, u32, *mut TlValue, *mut Heap) -> i32>(addr)
            };
            let cf = CompiledFn { f, arity, _engine: engine.clone() };
            self.compiled.borrow_mut().insert(Path::root(name), cf);
        }
        Ok(ok_bool(true))
    }

    /// Checked first in `Expr::Call`'s dispatch: `None` if `name` isn't a
    /// compiled function (the ordinary `self.fns`/`eval_builtin` dispatch
    /// then runs exactly as before — compiling a function is meant to be
    /// completely transparent to its callers). Marshals `args` into a
    /// `TlValue` array and back per the unified ABI (see [`TlValue`]) — Phase
    /// 2's calling convention generalization, replacing Phase 1's
    /// arity-specific native `extern "C" fn` pointers. `heap` (Phase 3) is
    /// passed straight through as the JIT'd code's fourth parameter so it can
    /// call the `cons`/`car`/`cdr`/rooting runtime shims
    /// ([`llvm_module_get_or_declare_runtime_fn`]) against the *same* heap
    /// the rest of this `Interp`'s evaluation is using.
    fn call_compiled(&self, heap: &mut Heap, name: &Path, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
        let compiled = self.compiled.borrow();
        let cf = compiled.get(name)?;
        if args.len() != cf.arity {
            return Some(Err(EvalError::Internal("compiled function: arity mismatch".into())));
        }
        // The JIT'd code reads each argument's payload as a plain `i64`
        // (`load-arg`) or truncates it to `i1` itself (`load-arg-bool`) based
        // on what `compile` baked into the IR for that parameter position —
        // so the *tag* written into each outgoing `TlValue` here is never
        // read back by the callee, only the payload bits matter.
        let tl_args: Vec<TlValue> = match args.iter().map(to_tlvalue_arg).collect() {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        let mut out = TlValue::zeroed();
        // SAFETY: `cf.f` was JIT-compiled (by `builtin_llvm_finish_compile`)
        // from a function declared with exactly this `(*const TlValue, u32,
        // *mut TlValue, *mut Heap) -> i32` signature, `tl_args` has length
        // `cf.arity` (checked above) matching the array `cf.f` expects, `out`
        // is a single live `TlValue` for it to write its result into, and
        // `heap` is the same heap this whole `Interp` evaluation is using
        // (so any `cons` the JIT'd code performs through the runtime shims
        // allocates from/roots against the right arena).
        let status = (cf.f)(tl_args.as_ptr(), tl_args.len() as u32, &mut out, heap);
        if status != 0 {
            return Some(Err(EvalError::Panic(format!("compiled function: returned status {}", status))));
        }
        // `build-ret`/`build-ret-bool`/`build-ret-f64`/`build-ret-char`/
        // `build-ret-sexpr` tag the result correctly, so unlike the
        // arguments above, `out.tag` is meaningful here and decides how to
        // decode `out.payload` back into an `RtValue`.
        let result = if out.tag == TlTag::Bool as u8 {
            RtValue::Bool(unsafe { out.payload.i64_ } != 0)
        } else if out.tag == TlTag::F64 as u8 {
            RtValue::Float(unsafe { out.payload.f64_ })
        } else if out.tag == TlTag::Char as u8 {
            let code = unsafe { out.payload.i64_ } as u32;
            match char::from_u32(code) {
                Some(c) => RtValue::Char(c),
                None => return Some(Err(EvalError::Internal(format!("compiled function: invalid char code {}", code)))),
            }
        } else if out.tag == TlTag::Ptr as u8 {
            RtValue::Sexpr(ptr_to_value(unsafe { out.payload.ptr }))
        } else {
            RtValue::Int(unsafe { out.payload.i64_ })
        };
        Some(Ok(result))
    }
}

/// Converts one argument `RtValue` into the [`TlValue`] `call_compiled` packs
/// into the outgoing array — `i64`/`bool`/`f64`/`char`/`Sexpr` only (Phase
/// 2b/2c/2d/3's scope).
#[cfg(feature = "compile")]
fn to_tlvalue_arg(v: &RtValue) -> Result<TlValue, EvalError> {
    match v {
        RtValue::Int(n) => Ok(TlValue::from_i64(*n)),
        RtValue::Bool(b) => Ok(TlValue::from_i64(*b as i64)),
        RtValue::Float(n) => Ok(TlValue::from_f64(*n)),
        RtValue::Sexpr(v) => Ok(TlValue::from_ptr(value_to_ptr(*v)?)),
        RtValue::Char(c) => Ok(TlValue::from_i64(*c as i64)),
        other => Err(EvalError::Internal(format!("compiled function: unsupported argument {:?}", other))),
    }
}

/// `Sexpr`'s `TlValue` encoding (Phase 3): null for `Value::Empty` (`Nil`),
/// otherwise `Value::Cons`'s raw cell pointer. Only these two variants are
/// bridged ([`crate::compile::ast_bridge`]'s doc comment) — `compile` refuses
/// any function whose body could produce another `Value` variant before this
/// is ever called from JIT'd code, but `call_compiled` also reaches it for
/// values supplied straight from the interpreter (a `Sexpr` argument to a
/// compiled function isn't bridge-checked the way a function *body* is), so
/// it still reports an error rather than panicking on a stray
/// `Int`/`Float`/`Char`/`Bool`/`Symbol`/`Str`/`Path` value.
#[cfg(feature = "compile")]
fn value_to_ptr(v: Value) -> Result<*mut u8, EvalError> {
    match v {
        Value::Empty => Ok(std::ptr::null_mut()),
        Value::Cons(c) => Ok(c.0 as *mut u8),
        other => Err(EvalError::Internal(format!("compiled function: unsupported Sexpr value {:?}", other))),
    }
}

/// The reverse of [`value_to_ptr`]: null decodes back to `Value::Empty`,
/// any other pointer to the `Value::Cons` it was built from.
#[cfg(feature = "compile")]
fn ptr_to_value(p: *mut u8) -> Value {
    if p.is_null() {
        Value::Empty
    } else {
        Value::Cons(ConsRef(p as *mut crate::mem::value::Cell))
    }
}

/// The unified calling-convention value (Phase 2,
/// [docs/TODO.md](../../docs/TODO.md)「ステップ5」): every compiled function
/// shares the one signature `extern "C" fn(*const TlValue, u32, *mut TlValue)
/// -> i32` (args array, arg count, out slot, status) regardless of its
/// logical arity or argument types, replacing Phase 1's per-arity native
/// `extern "C" fn(i64, ...) -> i64` pointers. 16 bytes, laid out so the LLVM
/// side can define the identical shape as the literal struct type `{i64,
/// i64}` (`crate::eval::interp::tlvalue_llvm_type`): `tag` occupies the low
/// byte of the first 8-byte slot (the rest is alignment padding, written as
/// zero from the LLVM side), `payload` is the second 8-byte slot reinterpreted
/// per `tag`.
#[cfg(feature = "compile")]
#[repr(C)]
#[derive(Clone, Copy)]
struct TlValue {
    tag: u8,
    payload: TlPayload,
}

#[cfg(feature = "compile")]
impl TlValue {
    fn from_i64(n: i64) -> TlValue {
        TlValue { tag: TlTag::I64 as u8, payload: TlPayload { i64_: n } }
    }

    /// Sets the union's `f64_` field directly (not a cast of some `i64`) so
    /// the payload's raw bit pattern is the IEEE-754 encoding of `n` — what
    /// the JIT'd side's `bitcast i64 to double` expects to find when it later
    /// reads this slot as `f64` (see `load_arg_f64_payload`/`build-ret-f64`).
    fn from_f64(n: f64) -> TlValue {
        TlValue { tag: TlTag::F64 as u8, payload: TlPayload { f64_: n } }
    }

    /// `Sexpr` (Phase 3): `p` is `Value::Empty`'s/`Value::Cons`'s
    /// representation per [`value_to_ptr`] — null for `Nil`, a `ConsRef`'s
    /// raw cell pointer otherwise. Other `Value` variants (`Int`/`Float`/
    /// `Char`/`Bool`/`Symbol`/`Str`/`Path`) aren't bridged yet (see
    /// `crate::compile::ast_bridge`'s doc comment).
    fn from_ptr(p: *mut u8) -> TlValue {
        TlValue { tag: TlTag::Ptr as u8, payload: TlPayload { ptr: p } }
    }

    fn zeroed() -> TlValue {
        TlValue { tag: 0, payload: TlPayload { i64_: 0 } }
    }
}

/// [`TlValue`]'s payload — `i64_`/`f64_`/`bool_`/`char_` cover `i64`/`f64`/
/// `bool`/`char` (Phase 2), `ptr` covers `Sexpr` (Phase 3, [`TlValue::from_ptr`]).
#[cfg(feature = "compile")]
#[repr(C)]
#[derive(Clone, Copy)]
union TlPayload {
    i64_: i64,
    f64_: f64,
    bool_: bool,
    char_: u32,
    ptr: *mut u8,
}

/// [`TlValue::tag`]'s possible values.
#[cfg(feature = "compile")]
#[repr(u8)]
#[allow(dead_code)]
enum TlTag {
    I64 = 0,
    F64 = 1,
    Bool = 2,
    Char = 3,
    Ptr = 4,
}

/// A function JIT-compiled by `compile`, called through the unified
/// [`TlValue`] ABI — see [`Interp::call_compiled`]. The `Rc<ExecutionEngine>`
/// keeps the JIT'd machine code alive — `call_compiled` only ever reads `f`,
/// never `_engine` itself, so rustc considers that field dead code; it is not.
#[cfg(feature = "compile")]
#[allow(dead_code)]
struct CompiledFn {
    f: extern "C" fn(*const TlValue, u32, *mut TlValue, *mut Heap) -> i32,
    arity: usize,
    _engine: Rc<inkwell::execution_engine::ExecutionEngine<'static>>,
}

/// Whether a `defun`'s parameter/return types are all `i64`/`bool`/`f64`/
/// `char`/`Sexpr` — Phase 3's restricted scope
/// (`crate::compile::ast_bridge`'s doc comment). Phase 1/2a required every
/// one of them to be `i64`; mixing in `bool` (Phase 2b) is what makes
/// predicate functions like `(a i64) (b i64) -> bool` compilable at all,
/// `f64`/`char` (Phase 2c/2d) floating-point/character ones, and `Sexpr`
/// (Phase 3) cons-cell ones — though only `Nil`/`Cons` *values* of `Sexpr`
/// are actually supported at runtime (see [`value_to_ptr`]'s doc comment),
/// a restriction this purely-type-level check can't itself express.
#[cfg(feature = "compile")]
fn is_supported_scalar_type(t: &Type) -> bool {
    matches!(t, Type::I64 | Type::Bool | Type::F64 | Type::Char) || is_compiled_sexpr_type(t)
}

/// Whether `t` is the `Sexpr` type itself (not [`super::is_sexpr_type`],
/// which checks a `Data` constructor's *type name* `Path` rather than a
/// [`Type`] value).
#[cfg(feature = "compile")]
fn is_compiled_sexpr_type(t: &Type) -> bool {
    matches!(t, Type::Named(p, params) if params.is_empty() && *p == Path::root("sexpr"))
}

#[cfg(feature = "compile")]
fn is_supported_scalar_signature(param_types: &[Type], ret: &Type) -> bool {
    is_supported_scalar_type(ret) && param_types.iter().all(is_supported_scalar_type)
}

/// The process-wide LLVM `Context`, leaked once for the life of the process
/// so every `Module`/`Builder`/value built from it can be `'static` —
/// otherwise inkwell's `'ctx` lifetime parameter would have to appear on
/// `RtValue` itself, which (unlike every other variant) is meant to be
/// freely cloned/stored without any borrow tracking. A single shared context
/// (rather than one per `compile` call) also lets JIT'd functions from
/// different `compile` calls in the same session call each other (LLVM does
/// not allow mixing types from different `Context`s in one module).
/// `inkwell::context::Context` is `!Sync` (it wraps a raw LLVM pointer), but a
/// `static` must be `Sync`. This wrapper asserts it anyway: every LLVM-IR-
/// construction entry point ([`compile_lock`]) takes a process-wide `Mutex`
/// before touching the context, so two `compile`-using tests running on
/// different threads (as `cargo test`'s default runner does) never mutate it
/// concurrently — only raw machine-code calls through an already-JIT'd
/// [`CompiledFn`] (no LLVM API involved) run outside that lock.
#[cfg(feature = "compile")]
struct LlvmContextHandle(*const inkwell::context::Context);
#[cfg(feature = "compile")]
unsafe impl Sync for LlvmContextHandle {}
#[cfg(feature = "compile")]
unsafe impl Send for LlvmContextHandle {}

#[cfg(feature = "compile")]
fn llvm_context() -> &'static inkwell::context::Context {
    static CONTEXT: once_cell::sync::OnceCell<LlvmContextHandle> = once_cell::sync::OnceCell::new();
    let handle = CONTEXT.get_or_init(|| {
        let ctx: &'static inkwell::context::Context = Box::leak(Box::new(inkwell::context::Context::create()));
        LlvmContextHandle(ctx)
    });
    unsafe { &*handle.0 }
}

/// Guards every operation that touches the shared [`llvm_context`] (type/IR
/// construction, JIT compilation) — LLVM's C API is not safe to call
/// concurrently from multiple threads against one `Context` without external
/// synchronization. Acquired by each LLVM-touching builtin individually
/// (rather than once for a whole `compile` call) since `compile` itself is
/// ordinary typelisp making many separate builtin calls, not one Rust call —
/// serializing each individual LLVM C API call is what actually prevents
/// concurrent mutation of the context's internal state (e.g. type-uniquing
/// tables), regardless of how two different threads' `compile` calls
/// interleave between those individual calls.
#[cfg(feature = "compile")]
fn compile_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: once_cell::sync::OnceCell<std::sync::Mutex<()>> = once_cell::sync::OnceCell::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// The LLVM-side mirror of [`TlValue`]'s layout: a literal (structurally
/// uniqued, so safe to build fresh on every call) `{i64, i64}` struct type —
/// field 0 is the tag (stored as a full `i64`, even though only its low byte
/// is meaningful from the Rust side), field 1 is the raw payload bits.
#[cfg(feature = "compile")]
fn tlvalue_llvm_type() -> inkwell::types::StructType<'static> {
    let i64_ty = llvm_context().i64_type();
    llvm_context().struct_type(&[i64_ty.into(), i64_ty.into()], false)
}

#[cfg(feature = "compile")]
fn llvm_module_new(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let name = expect_str(&args[0])?;
    Ok(RtValue::LlvmModule(Rc::new(RefCell::new(llvm_context().create_module(name)))))
}

#[cfg(feature = "compile")]
fn llvm_builder_new(_args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    Ok(RtValue::LlvmBuilder(LlvmBuilderHandle(Rc::new(RefCell::new(llvm_context().create_builder())))))
}

#[cfg(feature = "compile")]
fn llvm_const_i64(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let n = rt_i64(&args[0])?;
    Ok(RtValue::LlvmValue(llvm_context().i64_type().const_int(n as u64, true).into()))
}

/// `(llvm-const-bool b) -> LlvmValue`: an `i1` constant — Phase 2b's `bool`
/// literal support (`AstExpr::ABool`/`compile-value`).
#[cfg(feature = "compile")]
fn llvm_const_bool(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = rt_bool(&args[0])?;
    Ok(RtValue::LlvmValue(llvm_context().bool_type().const_int(b as u64, false).into()))
}

/// `(llvm-const-f64 n) -> LlvmValue`: an `f64` constant — Phase 2c's `f64`
/// literal support (`AstExpr::AFloat`/`compile-value`).
#[cfg(feature = "compile")]
fn llvm_const_f64(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let n = rt_f64(&args[0])?;
    Ok(RtValue::LlvmValue(llvm_context().f64_type().const_float(n).into()))
}

/// `(llvm-const-char c) -> LlvmValue`: an `i32` constant holding `c`'s
/// Unicode scalar value — Phase 2d's `char` literal support
/// (`AstExpr::AChar`/`compile-value`). `char` is stored as a plain `i32`
/// throughout IR construction (never bit-cast like `f64`), narrowed/widened
/// only at the `TlValue` boundary (`load-arg-char`/`build-ret-char`).
#[cfg(feature = "compile")]
fn llvm_const_char(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let c = rt_char(&args[0])?;
    Ok(RtValue::LlvmValue(llvm_context().i32_type().const_int(c as u64, false).into()))
}

/// `(llvm-const-nil) -> LlvmValue`: a null pointer constant — `Sexpr`'s
/// `Nil` literal (Phase 3, `AstExpr::ANil`/`compile-value`). [`value_to_ptr`]
/// encodes `Value::Empty` as null, so this is the one `Sexpr` literal
/// `compile-value` ever needs to construct directly (`Value::Cons` only ever
/// comes from `cons`/parameters/`car`/`cdr` — see `crate::compile::ast_bridge`).
#[cfg(feature = "compile")]
fn llvm_const_nil(_args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let ptr_ty = llvm_context().ptr_type(inkwell::AddressSpace::default());
    Ok(RtValue::LlvmValue(ptr_ty.const_null().into()))
}

#[cfg(feature = "compile")]
fn expect_llvm_module(v: &RtValue) -> Result<&Rc<RefCell<inkwell::module::Module<'static>>>, EvalError> {
    match v {
        RtValue::LlvmModule(m) => Ok(m),
        other => Err(EvalError::Internal(format!("expected a LlvmModule, got {:?}", other))),
    }
}

#[cfg(feature = "compile")]
fn expect_llvm_builder(v: &RtValue) -> Result<&LlvmBuilderHandle, EvalError> {
    match v {
        RtValue::LlvmBuilder(b) => Ok(b),
        other => Err(EvalError::Internal(format!("expected a LlvmBuilder, got {:?}", other))),
    }
}

#[cfg(feature = "compile")]
fn expect_llvm_function(v: &RtValue) -> Result<&inkwell::values::FunctionValue<'static>, EvalError> {
    match v {
        RtValue::LlvmFunction(f) => Ok(f),
        other => Err(EvalError::Internal(format!("expected a LlvmFunction, got {:?}", other))),
    }
}

#[cfg(feature = "compile")]
fn expect_llvm_basic_block(v: &RtValue) -> Result<&inkwell::basic_block::BasicBlock<'static>, EvalError> {
    match v {
        RtValue::LlvmBasicBlock(b) => Ok(b),
        other => Err(EvalError::Internal(format!("expected a LlvmBasicBlock, got {:?}", other))),
    }
}

#[cfg(feature = "compile")]
fn expect_llvm_basic_value(v: &RtValue) -> Result<&inkwell::values::BasicValueEnum<'static>, EvalError> {
    match v {
        RtValue::LlvmValue(v) => Ok(v),
        other => Err(EvalError::Internal(format!("expected a LlvmValue, got {:?}", other))),
    }
}

/// Narrows an `LlvmValue` to its `IntValue` (`i64`/`i1`) case — used wherever
/// a builtin only ever expects an integer-kind operand (e.g. `load-arg`'s
/// payload, `build-cond-br`'s condition). Returns an `EvalError::Internal`
/// rather than the raw inkwell panic `BasicValueEnum::into_int_value` would
/// give on a kind mismatch.
#[cfg(feature = "compile")]
fn expect_llvm_int_value(v: &RtValue) -> Result<inkwell::values::IntValue<'static>, EvalError> {
    match expect_llvm_basic_value(v)? {
        inkwell::values::BasicValueEnum::IntValue(v) => Ok(*v),
        other => Err(EvalError::Internal(format!("expected an int LlvmValue, got {:?}", other))),
    }
}

/// Narrows an `LlvmValue` to its `FloatValue` (`f64`) case — Phase 2c's `f64`
/// counterpart to [`expect_llvm_int_value`].
#[cfg(feature = "compile")]
fn expect_llvm_float_value(v: &RtValue) -> Result<inkwell::values::FloatValue<'static>, EvalError> {
    match expect_llvm_basic_value(v)? {
        inkwell::values::BasicValueEnum::FloatValue(v) => Ok(*v),
        other => Err(EvalError::Internal(format!("expected a float LlvmValue, got {:?}", other))),
    }
}

/// Narrows an `LlvmValue` to its `PointerValue` case — Phase 3's `Sexpr`
/// counterpart to [`expect_llvm_int_value`]/[`expect_llvm_float_value`]
/// (`Sexpr` is represented as a plain pointer throughout IR construction —
/// null for `Nil`, a cons cell address otherwise — never bit-cast or
/// truncated like `f64`/`bool`/`char`).
#[cfg(feature = "compile")]
fn expect_llvm_pointer_value(v: &RtValue) -> Result<inkwell::values::PointerValue<'static>, EvalError> {
    match expect_llvm_basic_value(v)? {
        inkwell::values::BasicValueEnum::PointerValue(v) => Ok(*v),
        other => Err(EvalError::Internal(format!("expected a pointer LlvmValue, got {:?}", other))),
    }
}

/// `Ok(b)`/`Err(Error(msg))` as an `RtValue::Data`, matching `result_def`'s
/// variant order (`ok` = 0, `err` = 1) — used by `compile`'s builtins, which
/// return `Result<bool, Error>` (not `Result<Unit, Error>`: `()` cannot be
/// written inside a generic's `<...>` in source text, since it's a pair of
/// list-delimiter characters, not token characters — see
/// `check::registry::register_compile_builtins`'s doc comment on
/// `llvm-finish-compile`).
#[cfg(feature = "compile")]
fn ok_bool(b: bool) -> RtValue {
    RtValue::Data { type_name: Path::root("result"), variant: 0, fields: vec![RtValue::Bool(b)] }
}

#[cfg(feature = "compile")]
fn err_error(msg: String) -> RtValue {
    let err_val = RtValue::Data { type_name: Path::root("error"), variant: 0, fields: vec![RtValue::Str(msg)] };
    RtValue::Data { type_name: Path::root("result"), variant: 1, fields: vec![err_val] }
}

/// Built-in instance methods for `LlvmModule`/`LlvmFunction`/`LlvmBuilder`
/// (`check::registry::llvm_module_def`/`llvm_function_def`/`llvm_builder_def`)
/// — `compile`'s LLVM-IR-construction primitives, called from
/// `crate::compile::compiler_source`. Mirrors [`eval_builtin_method`]'s
/// dispatch shape exactly (outer match on the type, inner on the method
/// name); `Expr::Assoc`'s eval arm falls back here only after both
/// `Interp::methods` (user `defmethod`s) and [`eval_builtin_method`] miss.
#[cfg(feature = "compile")]
fn eval_llvm_builtin_method(type_name: &Path, method: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    if *type_name == Path::root("llvmmodule") {
        return match method {
            "add-function" => Some(llvm_module_add_function(args)),
            "get-or-declare-function" => Some(llvm_module_get_or_declare_function(args)),
            "verify" => Some(llvm_module_verify(args)),
            "dump" => Some(llvm_module_dump(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("llvmfunction") {
        return match method {
            "append-block" => Some(llvm_function_append_block(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("llvmbuilder") {
        return match method {
            "position-at-end" => Some(llvm_builder_position_at_end(args)),
            "load-arg" => Some(llvm_builder_load_arg(args)),
            "load-arg-bool" => Some(llvm_builder_load_arg_bool(args)),
            "load-arg-f64" => Some(llvm_builder_load_arg_f64(args)),
            "load-arg-char" => Some(llvm_builder_load_arg_char(args)),
            "load-arg-sexpr" => Some(llvm_builder_load_arg_sexpr(args)),
            "build-op" => Some(llvm_builder_build_op(args)),
            "build-cond-br" => Some(llvm_builder_build_cond_br(args)),
            "build-br" => Some(llvm_builder_build_br(args)),
            "build-ret" => Some(llvm_builder_build_ret(args)),
            "build-ret-bool" => Some(llvm_builder_build_ret_bool(args)),
            "build-ret-f64" => Some(llvm_builder_build_ret_f64(args)),
            "build-ret-char" => Some(llvm_builder_build_ret_char(args)),
            "build-ret-sexpr" => Some(llvm_builder_build_ret_sexpr(args)),
            "build-call" => Some(llvm_builder_build_call(args)),
            "build-call-bool" => Some(llvm_builder_build_call_bool(args)),
            "build-call-f64" => Some(llvm_builder_build_call_f64(args)),
            "build-call-char" => Some(llvm_builder_build_call_char(args)),
            "build-call-sexpr" => Some(llvm_builder_build_call_sexpr(args)),
            "build-cons" => Some(llvm_builder_build_cons(args)),
            "build-car" => Some(llvm_builder_build_car(args)),
            "build-cdr" => Some(llvm_builder_build_cdr(args)),
            "build-nullp" => Some(llvm_builder_build_nullp(args)),
            "push-root" => Some(llvm_builder_build_push_root(args)),
            "pop-root" => Some(llvm_builder_build_pop_root(args)),
            "current-block" => Some(llvm_builder_current_block(args)),
            "build-phi" => Some(llvm_builder_build_phi(args)),
            "add-incoming" => Some(llvm_builder_add_incoming(args)),
            _ => None,
        };
    }
    None
}

/// The one LLVM-level function type every compiled function shares under the
/// unified `TlValue` ABI (Phase 2/3): `i32 (ptr args, i32 argc, ptr out, ptr
/// heap)` — shared by [`llvm_module_add_function`] (defining a function's
/// own body) and [`llvm_module_get_or_declare_function`] (Phase 2f,
/// declaring a *callee* with no body so a direct `call` to it can be
/// emitted). The trailing `heap` parameter (Phase 3) is a `*mut Heap`,
/// threaded through every compiled function regardless of whether its own
/// body touches `Sexpr` at all, so a function that doesn't use it can still
/// directly `call` one that does (and vice versa) without a signature
/// mismatch — see [`llvm_module_get_or_declare_runtime_fn`] for the
/// `cons`/`car`/`cdr`/rooting shims that actually read it.
#[cfg(feature = "compile")]
fn tlvalue_fn_type() -> inkwell::types::FunctionType<'static> {
    let ctx = llvm_context();
    let ptr_ty = ctx.ptr_type(inkwell::AddressSpace::default());
    let i32_ty = ctx.i32_type();
    i32_ty.fn_type(&[ptr_ty.into(), i32_ty.into(), ptr_ty.into(), ptr_ty.into()], false)
}

/// Declares `name` with the one LLVM-level signature every compiled function
/// shares under the unified `TlValue` ABI (Phase 2): `i32 (ptr args, i32
/// argc, ptr out)`. Unlike Phase 1's per-arity `i64 (i64, i64, ...)`, this
/// type doesn't depend on `name`'s logical arity at all, so `add-function`
/// no longer takes one (see `check::registry::llvm_module_def`).
#[cfg(feature = "compile")]
fn llvm_module_add_function(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(&args[1])?;
    let function = module.borrow().add_function(name, tlvalue_fn_type(), None);
    Ok(RtValue::LlvmFunction(function))
}

/// Runtime shims (Phase 3) compiled code calls directly for `Sexpr`
/// operations it can't safely build as inline IR: `cons` may trigger a GC
/// (`Heap::cons` runs one whenever the free list is empty), so the shim
/// roots `car`/`cdr` around the call exactly like the interpreter's own
/// `"cons"` builtin does (`self.sync_roots`-then-`heap.cons` in
/// [`Interp::eval_builtin`]) before handing the values to [`Heap::cons`].
/// `car`/`cdr` never allocate, so they need no rooting, but go through the
/// same shim layer anyway so JIT'd IR never has to know `Cell`'s layout.
/// [`runtime_shim_address`] resolves each by name to its function-item
/// address (a plain `as usize` cast — no `#[no_mangle]`/dynamic symbol
/// lookup needed, since the cast happens in the same Rust binary) for
/// `Interp::builtin_llvm_finish_compile`'s `add_global_mapping` pass — the
/// same mechanism that resolves a direct call to another *compiled typelisp
/// function* (Phase 2f), just pointing at a fixed Rust function instead of
/// one a `compile` call JIT'd.
///
/// Every Phase 3 `Value` not bridged (`Int`/`Float`/`Char`/`Bool`/`Symbol`/
/// `Str`/`Path` — see `crate::compile::ast_bridge`'s doc comment) makes
/// `value_to_ptr` fail; rather than smuggle that error back through a
/// `*mut u8` return slot (and force every IR call site to branch on it),
/// these shims abort the process outright — the same "this should be
/// statically impossible by construction" stance `compile-value`'s fallback
/// `panic` takes for unsupported `AstExpr` shapes, just enforced at the
/// runtime-value level instead of the AST level since a `Sexpr` carries no
/// static guarantee its *contents* stay within the bridged subset.
#[cfg(feature = "compile")]
extern "C" fn tl_sexpr_cons(heap: *mut Heap, car: *mut u8, cdr: *mut u8) -> *mut u8 {
    // SAFETY: `heap` is `Interp::call_compiled`'s fourth ABI parameter,
    // passed straight from a live `&mut Heap` for the duration of this call.
    let heap = unsafe { &mut *heap };
    let car_v = ptr_to_value(car);
    let cdr_v = ptr_to_value(cdr);
    heap.push_root(car_v);
    heap.push_root(cdr_v);
    let result = heap.cons(car_v, cdr_v);
    heap.pop_root();
    heap.pop_root();
    match result {
        Ok(v) => value_to_ptr_or_abort(v),
        Err(_) => std::process::abort(),
    }
}

#[cfg(feature = "compile")]
extern "C" fn tl_sexpr_car(heap: *mut Heap, v: *mut u8) -> *mut u8 {
    // SAFETY: see [`tl_sexpr_cons`].
    let heap = unsafe { &*heap };
    match heap.car(ptr_to_value(v)) {
        Ok(r) => value_to_ptr_or_abort(r),
        Err(_) => std::process::abort(),
    }
}

#[cfg(feature = "compile")]
extern "C" fn tl_sexpr_cdr(heap: *mut Heap, v: *mut u8) -> *mut u8 {
    // SAFETY: see [`tl_sexpr_cons`].
    let heap = unsafe { &*heap };
    match heap.cdr(ptr_to_value(v)) {
        Ok(r) => value_to_ptr_or_abort(r),
        Err(_) => std::process::abort(),
    }
}

/// `(push-root b f v) -> Unit`'s shim: registers `v` as a GC root for the
/// duration of the enclosing compiled function's stack frame — see
/// `crate::compile::compiler_source`'s rooting discipline (every `Sexpr`
/// parameter/`let` binding is pushed on entry to its scope and popped before
/// every exit, so any `cons` call anywhere in that scope sees it).
#[cfg(feature = "compile")]
extern "C" fn tl_heap_push_root(heap: *mut Heap, v: *mut u8) {
    // SAFETY: see [`tl_sexpr_cons`].
    let heap = unsafe { &mut *heap };
    heap.push_root(ptr_to_value(v));
}

#[cfg(feature = "compile")]
extern "C" fn tl_heap_pop_root(heap: *mut Heap) {
    // SAFETY: see [`tl_sexpr_cons`].
    let heap = unsafe { &mut *heap };
    heap.pop_root();
}

/// [`value_to_ptr`], aborting instead of returning an error — used by the
/// runtime shims above, which have no `Result`-shaped slot to report a
/// bridge-scope violation through (see [`tl_sexpr_cons`]'s doc comment).
#[cfg(feature = "compile")]
fn value_to_ptr_or_abort(v: Value) -> *mut u8 {
    value_to_ptr(v).unwrap_or_else(|_| std::process::abort())
}

/// Resolves a [`tl_sexpr_cons`]-style runtime shim's address by name, for
/// `Interp::builtin_llvm_finish_compile`'s `add_global_mapping` pass — the
/// fallback checked after `self.compiled` (a direct call to another
/// compiled *typelisp* function) comes up empty, so a bare declaration left
/// in the module by `build-cons`/`build-car`/`build-cdr`/`push-root`/
/// `pop-root` resolves to one of these fixed Rust functions instead.
#[cfg(feature = "compile")]
fn runtime_shim_address(name: &str) -> Option<usize> {
    match name {
        "tl_sexpr_cons" => Some(tl_sexpr_cons as usize),
        "tl_sexpr_car" => Some(tl_sexpr_car as usize),
        "tl_sexpr_cdr" => Some(tl_sexpr_cdr as usize),
        "tl_heap_push_root" => Some(tl_heap_push_root as usize),
        "tl_heap_pop_root" => Some(tl_heap_pop_root as usize),
        _ => None,
    }
}

/// `(get-or-declare-function module name) -> LlvmFunction` (Phase 2f, direct
/// calls between compiled functions): returns `name`'s existing declaration
/// in `module` if `build-call` already added one for an earlier call to the
/// same callee (LLVM would otherwise rename a second `add_function` call
/// with a colliding name instead of returning the first one), otherwise
/// declares it fresh — a bare external declaration with no basic blocks,
/// later resolved to the callee's real JIT'd address by
/// `Interp::builtin_llvm_finish_compile`'s `add_global_mapping` pass.
#[cfg(feature = "compile")]
fn llvm_module_get_or_declare_function(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(&args[1])?;
    let m = module.borrow();
    if let Some(f) = m.get_function(name) {
        return Ok(RtValue::LlvmFunction(f));
    }
    let function = m.add_function(name, tlvalue_fn_type(), None);
    Ok(RtValue::LlvmFunction(function))
}

/// `ptr (ptr heap, ptr a, ptr b)` — [`tl_sexpr_cons`]'s LLVM-level type.
#[cfg(feature = "compile")]
fn runtime_binary_ptr_fn_type() -> inkwell::types::FunctionType<'static> {
    let ptr_ty = llvm_context().ptr_type(inkwell::AddressSpace::default());
    ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false)
}

/// `ptr (ptr heap, ptr v)` — [`tl_sexpr_car`]/[`tl_sexpr_cdr`]'s LLVM-level
/// type, and (modulo the `void` return) [`tl_heap_push_root`]'s.
#[cfg(feature = "compile")]
fn runtime_unary_ptr_fn_type() -> inkwell::types::FunctionType<'static> {
    let ptr_ty = llvm_context().ptr_type(inkwell::AddressSpace::default());
    ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false)
}

/// `void (ptr heap, ptr v)` — [`tl_heap_push_root`]'s LLVM-level type.
#[cfg(feature = "compile")]
fn runtime_push_root_fn_type() -> inkwell::types::FunctionType<'static> {
    let ptr_ty = llvm_context().ptr_type(inkwell::AddressSpace::default());
    llvm_context().void_type().fn_type(&[ptr_ty.into(), ptr_ty.into()], false)
}

/// `void (ptr heap)` — [`tl_heap_pop_root`]'s LLVM-level type.
#[cfg(feature = "compile")]
fn runtime_pop_root_fn_type() -> inkwell::types::FunctionType<'static> {
    let ptr_ty = llvm_context().ptr_type(inkwell::AddressSpace::default());
    llvm_context().void_type().fn_type(&[ptr_ty.into()], false)
}

/// Declares one of the fixed-name Phase 3 runtime shims
/// (`tl_sexpr_cons`/`tl_sexpr_car`/`tl_sexpr_cdr`/`tl_heap_push_root`/
/// `tl_heap_pop_root`) in `module` if not already present — the same
/// "reuse an existing declaration, else add one" shape as
/// [`llvm_module_get_or_declare_function`], just for a Rust function
/// (resolved later by [`runtime_shim_address`]) instead of another
/// compiled typelisp function.
#[cfg(feature = "compile")]
fn get_or_declare_runtime_fn(
    module: &inkwell::module::Module<'static>,
    name: &str,
    fn_type: inkwell::types::FunctionType<'static>,
) -> inkwell::values::FunctionValue<'static> {
    if let Some(f) = module.get_function(name) {
        return f;
    }
    module.add_function(name, fn_type, None)
}

/// The unified ABI's fourth parameter (`*mut Heap`) of the function `b` is
/// currently building IR for — every `Sexpr` runtime-shim call
/// (`build-cons`/`build-car`/`build-cdr`/`push-root`/`pop-root`) needs it,
/// since it's the one piece of state those shims can't get from their other
/// arguments alone.
#[cfg(feature = "compile")]
fn heap_param_of(f: &inkwell::values::FunctionValue<'static>) -> Result<inkwell::values::PointerValue<'static>, EvalError> {
    Ok(f.get_nth_param(3)
        .ok_or_else(|| EvalError::Internal("function has no heap parameter".into()))?
        .into_pointer_value())
}

/// `(build-cons b module f car cdr) -> LlvmValue`: emits a direct `call` to
/// the [`tl_sexpr_cons`] runtime shim (`module`/`f` are needed to declare
/// the shim and read the enclosing function's `heap` parameter — see
/// [`get_or_declare_runtime_fn`]/[`heap_param_of`]). The shim itself handles
/// rooting `car`/`cdr` around the underlying `Heap::cons` call (which may
/// trigger a GC) — `build-cons` itself doesn't need to.
#[cfg(feature = "compile")]
fn llvm_builder_build_cons(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let f = expect_llvm_function(&args[2])?;
    let car = expect_llvm_pointer_value(&args[3])?;
    let cdr = expect_llvm_pointer_value(&args[4])?;
    let builder = b.0.borrow();
    let heap_param = heap_param_of(f)?;
    let shim = get_or_declare_runtime_fn(&module.borrow(), "tl_sexpr_cons", runtime_binary_ptr_fn_type());
    let call = builder
        .build_call(shim, &[heap_param.into(), car.into(), cdr.into()], "cons")
        .map_err(|e| EvalError::Panic(format!("build-cons: {}", e)))?;
    let result = call
        .try_as_basic_value()
        .basic()
        .ok_or_else(|| EvalError::Internal("build-cons: shim returned no value".into()))?;
    Ok(RtValue::LlvmValue(result))
}

/// Shared by [`llvm_builder_build_car`]/[`llvm_builder_build_cdr`]: emits a
/// direct `call` to `shim_name` (`tl_sexpr_car`/`tl_sexpr_cdr`), which never
/// triggers a GC (`Heap::car`/`Heap::cdr` only read), so no rooting is
/// needed around the call.
#[cfg(feature = "compile")]
fn llvm_builder_build_unary_sexpr_op(args: &[RtValue], shim_name: &str) -> Result<RtValue, EvalError> {
    let b = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let f = expect_llvm_function(&args[2])?;
    let v = expect_llvm_pointer_value(&args[3])?;
    let builder = b.0.borrow();
    let heap_param = heap_param_of(f)?;
    let shim = get_or_declare_runtime_fn(&module.borrow(), shim_name, runtime_unary_ptr_fn_type());
    let call = builder
        .build_call(shim, &[heap_param.into(), v.into()], shim_name)
        .map_err(|e| EvalError::Panic(format!("{}: {}", shim_name, e)))?;
    let result = call
        .try_as_basic_value()
        .basic()
        .ok_or_else(|| EvalError::Internal(format!("{}: shim returned no value", shim_name)))?;
    Ok(RtValue::LlvmValue(result))
}

/// `(build-car b module f v) -> LlvmValue`: see [`llvm_builder_build_unary_sexpr_op`].
#[cfg(feature = "compile")]
fn llvm_builder_build_car(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    llvm_builder_build_unary_sexpr_op(args, "tl_sexpr_car")
}

/// `(build-cdr b module f v) -> LlvmValue`: see [`llvm_builder_build_unary_sexpr_op`].
#[cfg(feature = "compile")]
fn llvm_builder_build_cdr(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    llvm_builder_build_unary_sexpr_op(args, "tl_sexpr_cdr")
}

/// `(build-nullp b v) -> LlvmValue`: whether `v` (a `Sexpr`) is `Nil` —
/// a plain pointer-null comparison (`build_is_null`), no runtime shim or
/// `heap`/`module` needed since it never touches the heap at all.
#[cfg(feature = "compile")]
fn llvm_builder_build_nullp(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let v = expect_llvm_pointer_value(&args[1])?;
    let builder = b.0.borrow();
    let result = builder
        .build_is_null(v, "nullp")
        .map_err(|e| EvalError::Panic(format!("build-nullp: {}", e)))?;
    Ok(RtValue::LlvmValue(result.into()))
}

/// `(is-sexpr-value v) -> bool`: whether `v`'s actual LLVM kind is a pointer
/// — `Sexpr` values are the only `AstExpr` shape `compile-value` ever
/// produces as a `PointerValue` (Phase 3), so this lets the typelisp-written
/// `compile`'s rooting discipline (`crate::compile::compiler_source`'s doc
/// comment) tell which entries of a `Vector<LlvmValue>` need
/// `push-root`/`pop-root` without `AstExpr` itself carrying per-node type
/// tags — the same "ask the value its own kind" approach `build-op` uses for
/// int-vs-float dispatch.
#[cfg(feature = "compile")]
fn llvm_value_is_sexpr(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let v = expect_llvm_basic_value(&args[0])?;
    Ok(RtValue::Bool(matches!(v, inkwell::values::BasicValueEnum::PointerValue(_))))
}

/// `(push-root b module f v) -> Unit`: emits a direct `call` to the
/// [`tl_heap_push_root`] runtime shim — see
/// `crate::compile::compiler_source`'s rooting discipline for when/why
/// `compile-value`/`compile-tail` emit this around every `Sexpr`
/// parameter/`let` binding's scope.
#[cfg(feature = "compile")]
fn llvm_builder_build_push_root(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let f = expect_llvm_function(&args[2])?;
    let v = expect_llvm_pointer_value(&args[3])?;
    let builder = b.0.borrow();
    let heap_param = heap_param_of(f)?;
    let shim = get_or_declare_runtime_fn(&module.borrow(), "tl_heap_push_root", runtime_push_root_fn_type());
    builder
        .build_call(shim, &[heap_param.into(), v.into()], "pushroot")
        .map_err(|e| EvalError::Panic(format!("push-root: {}", e)))?;
    Ok(RtValue::Unit)
}

/// `(pop-root b module f) -> Unit`: see [`llvm_builder_build_push_root`].
#[cfg(feature = "compile")]
fn llvm_builder_build_pop_root(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let f = expect_llvm_function(&args[2])?;
    let builder = b.0.borrow();
    let heap_param = heap_param_of(f)?;
    let shim = get_or_declare_runtime_fn(&module.borrow(), "tl_heap_pop_root", runtime_pop_root_fn_type());
    builder
        .build_call(shim, &[heap_param.into()], "poproot")
        .map_err(|e| EvalError::Panic(format!("pop-root: {}", e)))?;
    Ok(RtValue::Unit)
}

#[cfg(feature = "compile")]
fn llvm_module_verify(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let module = expect_llvm_module(&args[0])?;
    match module.borrow().verify() {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(err_error(e.to_string())),
    }
}

#[cfg(feature = "compile")]
fn llvm_module_dump(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let module = expect_llvm_module(&args[0])?;
    eprintln!("{}", module.borrow().print_to_string().to_string());
    Ok(RtValue::Unit)
}

#[cfg(feature = "compile")]
fn llvm_function_append_block(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let f = expect_llvm_function(&args[0])?;
    let name = expect_str(&args[1])?;
    Ok(RtValue::LlvmBasicBlock(llvm_context().append_basic_block(*f, name)))
}

#[cfg(feature = "compile")]
fn llvm_builder_position_at_end(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let block = expect_llvm_basic_block(&args[1])?;
    b.0.borrow().position_at_end(*block);
    Ok(RtValue::Unit)
}

/// Shared by [`llvm_builder_load_arg`]/[`llvm_builder_load_arg_bool`]: reads
/// logical argument `i` out of `f`'s `args` parameter (the unified ABI's
/// first LLVM-level parameter, a pointer to a contiguous array of
/// `TlValue`s — see [`TlValue`]/[`tlvalue_llvm_type`]) — indexes to the
/// `i`-th element, loads the whole struct, and extracts its raw `i64`
/// payload field (always `i64`-width regardless of the argument's logical
/// type — `bool` is narrowed by the caller, see [`llvm_builder_load_arg_bool`]).
/// Does not itself take [`compile_lock`] — callers must already hold it.
#[cfg(feature = "compile")]
fn load_arg_payload(
    builder: &inkwell::builder::Builder<'static>,
    f: &inkwell::values::FunctionValue<'static>,
    i: i64,
) -> Result<inkwell::values::IntValue<'static>, EvalError> {
    let tlvalue_ty = tlvalue_llvm_type();
    let args_ptr = f
        .get_nth_param(0)
        .ok_or_else(|| EvalError::Internal("load-arg: function has no args parameter".into()))?
        .into_pointer_value();
    let idx = llvm_context().i64_type().const_int(i as u64, false);
    // SAFETY: `args_ptr` points to a contiguous array of (at least) `arity`
    // `TlValue`s, built by `Interp::call_compiled` right before invoking this
    // function; `i` is checked by the typelisp-written `compile` against the
    // function's own declared arity before it's ever used as an index here.
    let elem_ptr = unsafe {
        builder
            .build_gep(tlvalue_ty, args_ptr, &[idx], "argptr")
            .map_err(|e| EvalError::Panic(format!("load-arg: {}", e)))?
    };
    let loaded = builder
        .build_load(tlvalue_ty, elem_ptr, "argstruct")
        .map_err(|e| EvalError::Panic(format!("load-arg: {}", e)))?
        .into_struct_value();
    builder
        .build_extract_value(loaded, 1, "payload")
        .map_err(|e| EvalError::Panic(format!("load-arg: {}", e)))
        .map(|v| v.into_int_value())
}

/// `(load-arg b f i) -> LlvmValue`: logical argument `i`, as a plain `i64`.
/// `bool`/`f64`/`char` arguments need [`llvm_builder_load_arg_bool`]/
/// [`llvm_builder_load_arg_f64`]/[`llvm_builder_load_arg_char`] instead
/// (Phase 2b/2c/2d).
#[cfg(feature = "compile")]
fn llvm_builder_load_arg(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let i = rt_i64(&args[2])?;
    let payload = load_arg_payload(&b.0.borrow(), f, i)?;
    Ok(RtValue::LlvmValue(payload.into()))
}

/// `(load-arg-bool b f i) -> LlvmValue`: logical argument `i`, narrowed
/// (`build_int_truncate`) from the raw `i64` payload down to `i1` — Phase
/// 2b's `bool`-parameter support, the counterpart to [`llvm_builder_load_arg`].
#[cfg(feature = "compile")]
fn llvm_builder_load_arg_bool(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let i = rt_i64(&args[2])?;
    let builder = b.0.borrow();
    let payload = load_arg_payload(&builder, f, i)?;
    let narrowed = builder
        .build_int_truncate(payload, llvm_context().bool_type(), "argbool")
        .map_err(|e| EvalError::Panic(format!("load-arg-bool: {}", e)))?;
    Ok(RtValue::LlvmValue(narrowed.into()))
}

/// `(load-arg-char b f i) -> LlvmValue`: logical argument `i`, narrowed
/// (`build_int_truncate`) from the raw `i64` payload down to `i32` — Phase
/// 2d's `char`-parameter support, the counterpart to [`llvm_builder_load_arg`]
/// (the same truncate shape [`llvm_builder_load_arg_bool`] uses, just to
/// `i32` instead of `i1`).
#[cfg(feature = "compile")]
fn llvm_builder_load_arg_char(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let i = rt_i64(&args[2])?;
    let builder = b.0.borrow();
    let payload = load_arg_payload(&builder, f, i)?;
    let narrowed = builder
        .build_int_truncate(payload, llvm_context().i32_type(), "argchar")
        .map_err(|e| EvalError::Panic(format!("load-arg-char: {}", e)))?;
    Ok(RtValue::LlvmValue(narrowed.into()))
}

/// `(load-arg-sexpr b f i) -> LlvmValue`: logical argument `i`, reinterpreted
/// (`build_int_to_ptr`) from the raw `i64` payload to a pointer — Phase 3's
/// `Sexpr`-parameter support, the counterpart to [`llvm_builder_load_arg`].
/// Like `f64`'s bit-cast, this is a reinterpretation of the same bits
/// (`TlValue::from_ptr`/[`value_to_ptr`] write the pointer's raw bytes into
/// the `i64_` slot), not a numeric conversion.
#[cfg(feature = "compile")]
fn llvm_builder_load_arg_sexpr(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let i = rt_i64(&args[2])?;
    let builder = b.0.borrow();
    let payload = load_arg_payload(&builder, f, i)?;
    let ptr_ty = llvm_context().ptr_type(inkwell::AddressSpace::default());
    let ptr = builder
        .build_int_to_ptr(payload, ptr_ty, "argsexpr")
        .map_err(|e| EvalError::Panic(format!("load-arg-sexpr: {}", e)))?;
    Ok(RtValue::LlvmValue(ptr.into()))
}

/// `(load-arg-f64 b f i) -> LlvmValue`: logical argument `i`, reinterpreted
/// (`build_bit_cast`, not a numeric conversion) from the raw `i64` payload's
/// bits to `f64` — Phase 2c's `f64`-parameter support, the counterpart to
/// [`llvm_builder_load_arg`]. A bit-cast (not truncate/zext) is correct here
/// because [`TlValue::from_f64`] writes `n`'s IEEE-754 bit pattern straight
/// into the same 8-byte slot `i64_` occupies, the same relationship
/// `build-ret-f64` uses in reverse.
#[cfg(feature = "compile")]
fn llvm_builder_load_arg_f64(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let i = rt_i64(&args[2])?;
    let builder = b.0.borrow();
    let payload = load_arg_payload(&builder, f, i)?;
    let reinterpreted = builder
        .build_bit_cast(payload, llvm_context().f64_type(), "argf64")
        .map_err(|e| EvalError::Panic(format!("load-arg-f64: {}", e)))?;
    Ok(RtValue::LlvmValue(reinterpreted))
}

/// `op` is one of `+ - * / mod < <= > >= = /=` — the same set `i64`'s
/// `int_assoc` registers and `crate::compile::ast_bridge::typed_to_ast`
/// bridges into an `abinop` node, forwarded here verbatim by
/// `crate::compile::compiler_source`'s `compile-value` so neither side needs
/// its own operator-name dispatch table. `lhs`/`rhs`'s actual `LlvmValue` kind
/// (set by which `AstExpr` leaf produced them — `f64` only ever flows from
/// `f64`'s own `Assoc` methods, per `ast_bridge`) decides int vs. float ops;
/// a kind mismatch between them would be a checker/bridge bug, not a
/// user-reachable error.
#[cfg(feature = "compile")]
fn llvm_builder_build_op(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let op = expect_str(&args[1])?;
    let builder = b.0.borrow();
    match (expect_llvm_basic_value(&args[2])?, expect_llvm_basic_value(&args[3])?) {
        (inkwell::values::BasicValueEnum::IntValue(lhs), inkwell::values::BasicValueEnum::IntValue(rhs)) => {
            llvm_builder_build_int_op(&builder, op, *lhs, *rhs)
        }
        (inkwell::values::BasicValueEnum::FloatValue(lhs), inkwell::values::BasicValueEnum::FloatValue(rhs)) => {
            llvm_builder_build_float_op(&builder, op, *lhs, *rhs)
        }
        (lhs, rhs) => Err(EvalError::Internal(format!("build-op: mismatched operand kinds {:?}/{:?}", lhs, rhs))),
    }
}

#[cfg(feature = "compile")]
fn llvm_builder_build_int_op(
    builder: &inkwell::builder::Builder<'static>,
    op: &str,
    lhs: inkwell::values::IntValue<'static>,
    rhs: inkwell::values::IntValue<'static>,
) -> Result<RtValue, EvalError> {
    let result = match op {
        "+" => builder.build_int_add(lhs, rhs, "addtmp"),
        "-" => builder.build_int_sub(lhs, rhs, "subtmp"),
        "*" => builder.build_int_mul(lhs, rhs, "multmp"),
        "/" => builder.build_int_signed_div(lhs, rhs, "divtmp"),
        "mod" => builder.build_int_signed_rem(lhs, rhs, "modtmp"),
        "<" => return llvm_icmp(builder, inkwell::IntPredicate::SLT, lhs, rhs),
        "<=" => return llvm_icmp(builder, inkwell::IntPredicate::SLE, lhs, rhs),
        ">" => return llvm_icmp(builder, inkwell::IntPredicate::SGT, lhs, rhs),
        ">=" => return llvm_icmp(builder, inkwell::IntPredicate::SGE, lhs, rhs),
        "=" => return llvm_icmp(builder, inkwell::IntPredicate::EQ, lhs, rhs),
        "/=" => return llvm_icmp(builder, inkwell::IntPredicate::NE, lhs, rhs),
        _ => return Err(EvalError::Internal(format!("build-op: unknown operator {}", op))),
    };
    result.map(|v| RtValue::LlvmValue(v.into())).map_err(|e| EvalError::Panic(format!("build-op: {}", e)))
}

#[cfg(feature = "compile")]
fn llvm_icmp(
    builder: &inkwell::builder::Builder<'static>,
    pred: inkwell::IntPredicate,
    lhs: inkwell::values::IntValue<'static>,
    rhs: inkwell::values::IntValue<'static>,
) -> Result<RtValue, EvalError> {
    builder
        .build_int_compare(pred, lhs, rhs, "cmptmp")
        .map(|v| RtValue::LlvmValue(v.into()))
        .map_err(|e| EvalError::Panic(format!("build-op: {}", e)))
}

/// `f64`'s arithmetic (`fadd`/`fsub`/`fmul`/`fdiv`/`frem`) and *ordered*
/// comparison (`fcmp` with `O*`/`UNE` predicates — false for any operand that
/// is `NaN`, except `/=` which (matching Rust's own `f64: PartialEq`) is true
/// for `NaN` since it is never equal to anything) — Phase 2c's float
/// counterpart to [`llvm_builder_build_int_op`]. Comparisons still produce a
/// plain `i1` `IntValue` (`fcmp`'s result type, just like `icmp`'s), wrapped
/// the same way `bool` always is, not a `FloatValue`.
#[cfg(feature = "compile")]
fn llvm_builder_build_float_op(
    builder: &inkwell::builder::Builder<'static>,
    op: &str,
    lhs: inkwell::values::FloatValue<'static>,
    rhs: inkwell::values::FloatValue<'static>,
) -> Result<RtValue, EvalError> {
    let result = match op {
        "+" => builder.build_float_add(lhs, rhs, "faddtmp"),
        "-" => builder.build_float_sub(lhs, rhs, "fsubtmp"),
        "*" => builder.build_float_mul(lhs, rhs, "fmultmp"),
        "/" => builder.build_float_div(lhs, rhs, "fdivtmp"),
        "mod" => builder.build_float_rem(lhs, rhs, "fmodtmp"),
        "<" => return llvm_fcmp(builder, inkwell::FloatPredicate::OLT, lhs, rhs),
        "<=" => return llvm_fcmp(builder, inkwell::FloatPredicate::OLE, lhs, rhs),
        ">" => return llvm_fcmp(builder, inkwell::FloatPredicate::OGT, lhs, rhs),
        ">=" => return llvm_fcmp(builder, inkwell::FloatPredicate::OGE, lhs, rhs),
        "=" => return llvm_fcmp(builder, inkwell::FloatPredicate::OEQ, lhs, rhs),
        "/=" => return llvm_fcmp(builder, inkwell::FloatPredicate::UNE, lhs, rhs),
        _ => return Err(EvalError::Internal(format!("build-op: unknown operator {}", op))),
    };
    result.map(|v| RtValue::LlvmValue(v.into())).map_err(|e| EvalError::Panic(format!("build-op: {}", e)))
}

#[cfg(feature = "compile")]
fn llvm_fcmp(
    builder: &inkwell::builder::Builder<'static>,
    pred: inkwell::FloatPredicate,
    lhs: inkwell::values::FloatValue<'static>,
    rhs: inkwell::values::FloatValue<'static>,
) -> Result<RtValue, EvalError> {
    builder
        .build_float_compare(pred, lhs, rhs, "fcmptmp")
        .map(|v| RtValue::LlvmValue(v.into()))
        .map_err(|e| EvalError::Panic(format!("build-op: {}", e)))
}

#[cfg(feature = "compile")]
fn llvm_builder_build_cond_br(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let cond = expect_llvm_int_value(&args[1])?;
    let then_block = *expect_llvm_basic_block(&args[2])?;
    let else_block = *expect_llvm_basic_block(&args[3])?;
    b.0.borrow()
        .build_conditional_branch(cond, then_block, else_block)
        .map(|_| RtValue::Unit)
        .map_err(|e| EvalError::Panic(format!("build-cond-br: {}", e)))
}

#[cfg(feature = "compile")]
fn llvm_builder_build_br(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let block = expect_llvm_basic_block(&args[1])?;
    b.0.borrow()
        .build_unconditional_branch(*block)
        .map(|_| RtValue::Unit)
        .map_err(|e| EvalError::Panic(format!("build-br: {}", e)))
}

/// `(current-block b) -> LlvmBasicBlock` (ループ構文): `b`'s current
/// insertion block. `while`'s loop compilation (`compile::compiler_source`)
/// uses this rather than assuming the block it itself `append-block`-ed is
/// still where the builder ends up after compiling a loop's preheader
/// expression or body — both may contain further control flow (e.g. a
/// nested `while`) that leaves `b` positioned somewhere else by the time
/// control "falls through" to the back edge.
#[cfg(feature = "compile")]
fn llvm_builder_current_block(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let block = b
        .0
        .borrow()
        .get_insert_block()
        .ok_or_else(|| EvalError::Internal("current-block: builder has no current block".into()))?;
    Ok(RtValue::LlvmBasicBlock(block))
}

/// `(build-phi b seed) -> LlvmValue` (ループ構文): an empty phi node — no
/// incoming edges yet, see [`llvm_builder_add_incoming`] — typed to match
/// `seed`'s own LLVM type, positioned at `b`'s current block. Must be called
/// before any non-phi instruction is built in that block: LLVM requires
/// every phi in a basic block to precede all other instructions, which is
/// why `compile-value`'s `AWhile` arm builds every loop-carried variable's
/// phi immediately after `position-at-end`ing the loop header, before even
/// compiling the loop condition.
#[cfg(feature = "compile")]
fn llvm_builder_build_phi(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let seed = expect_llvm_basic_value(&args[1])?;
    let phi = b
        .0
        .borrow()
        .build_phi(seed.get_type(), "phi")
        .map_err(|e| EvalError::Panic(format!("build-phi: {}", e)))?;
    Ok(RtValue::LlvmValue(phi.as_basic_value()))
}

/// `(add-incoming b phi val block) -> Unit` (ループ構文): adds one `(val,
/// block)` incoming edge to `phi` (an [`RtValue::LlvmValue`] previously
/// returned by [`llvm_builder_build_phi`] — `phi` is reconstructed back into
/// an `inkwell::values::PhiValue` via `as_instruction_value`/`TryFrom`,
/// since `RtValue::LlvmValue` only ever stores the `BasicValueEnum` shape
/// every other LLVM value uses, not a separate phi-specific variant). `b`
/// itself isn't used (`add_incoming` is a property of the phi instruction,
/// not the builder's position) but is taken anyway for consistency with
/// every other `LlvmBuilder` method.
#[cfg(feature = "compile")]
fn llvm_builder_add_incoming(args: &[RtValue]) -> Result<RtValue, EvalError> {
    use inkwell::values::{BasicValue, PhiValue};
    use std::convert::TryFrom;

    let _guard = compile_lock().lock().unwrap();
    let phi = expect_llvm_basic_value(&args[1])?;
    let val = expect_llvm_basic_value(&args[2])?;
    let block = expect_llvm_basic_block(&args[3])?;
    let instr = phi
        .as_instruction_value()
        .ok_or_else(|| EvalError::Internal("add-incoming: phi is not an instruction".into()))?;
    let phi_value = PhiValue::try_from(instr)
        .map_err(|_| EvalError::Internal("add-incoming: value is not a phi".into()))?;
    phi_value.add_incoming(&[(val as &dyn BasicValue, *block)]);
    Ok(RtValue::Unit)
}

/// Packs `tag`/`payload` into a `TlValue` struct value — shared by
/// [`build_ret_tlvalue`] (returning it through a function's `out` parameter)
/// and [`llvm_builder_build_call_payload`] (Phase 2f, writing one into a
/// stack-allocated outgoing argument array). Does not itself take
/// [`compile_lock`] — callers must already hold it.
#[cfg(feature = "compile")]
fn pack_tlvalue(
    builder: &inkwell::builder::Builder<'static>,
    tag: TlTag,
    payload: inkwell::values::IntValue<'static>,
) -> Result<inkwell::values::StructValue<'static>, EvalError> {
    let tlvalue_ty = tlvalue_llvm_type();
    let tag_v = llvm_context().i64_type().const_int(tag as u64, false);
    let packed = tlvalue_ty.get_undef();
    let packed = builder
        .build_insert_value(packed, tag_v, 0, "tagged")
        .map_err(|e| EvalError::Panic(format!("build-tlvalue: {}", e)))?;
    let packed = builder
        .build_insert_value(packed, payload, 1, "withpayload")
        .map_err(|e| EvalError::Panic(format!("build-tlvalue: {}", e)))?;
    Ok(packed.into_struct_value())
}

/// Shared by [`llvm_builder_build_ret`]/[`llvm_builder_build_ret_bool`]: packs
/// `tag`/`payload` into a `TlValue`, stores it through `out_ptr`, and emits
/// the function's terminal `ret i32 0` (status "ok") — the one and only
/// return instruction every compiled function ends with. Does not itself
/// take [`compile_lock`] — callers must already hold it.
#[cfg(feature = "compile")]
fn build_ret_tlvalue(
    builder: &inkwell::builder::Builder<'static>,
    out_ptr: inkwell::values::PointerValue<'static>,
    tag: TlTag,
    payload: inkwell::values::IntValue<'static>,
) -> Result<RtValue, EvalError> {
    let packed = pack_tlvalue(builder, tag, payload)?;
    builder
        .build_store(out_ptr, packed)
        .map_err(|e| EvalError::Panic(format!("build-ret: {}", e)))?;
    let status = llvm_context().i32_type().const_int(0, false);
    builder
        .build_return(Some(&status))
        .map(|_| RtValue::Unit)
        .map_err(|e| EvalError::Panic(format!("build-ret: {}", e)))
}

/// Picks `(TlTag, raw i64 bits)` for one outgoing call argument from its
/// actual LLVM value kind (Phase 2f — `AstExpr` carries no per-argument type
/// tag, so, like [`llvm_builder_build_op`], this inspects the value itself):
/// an `i1` widens to `TlTag::Bool`, any other-width `IntValue` is `TlTag::I64`
/// as-is, and a `FloatValue` bit-casts to `TlTag::F64` (matching
/// [`llvm_builder_build_ret_f64`]'s reinterpret-don't-convert convention).
/// Does not itself take [`compile_lock`] — callers must already hold it.
#[cfg(feature = "compile")]
fn tlvalue_tag_and_payload(
    builder: &inkwell::builder::Builder<'static>,
    v: inkwell::values::BasicValueEnum<'static>,
) -> Result<(TlTag, inkwell::values::IntValue<'static>), EvalError> {
    match v {
        inkwell::values::BasicValueEnum::IntValue(iv) if iv.get_type().get_bit_width() == 1 => {
            let widened = builder
                .build_int_z_extend(iv, llvm_context().i64_type(), "argbool")
                .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?;
            Ok((TlTag::Bool, widened))
        }
        inkwell::values::BasicValueEnum::IntValue(iv) => Ok((TlTag::I64, iv)),
        inkwell::values::BasicValueEnum::FloatValue(fv) => {
            let bits = builder
                .build_bit_cast(fv, llvm_context().i64_type(), "argf64")
                .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?
                .into_int_value();
            Ok((TlTag::F64, bits))
        }
        inkwell::values::BasicValueEnum::PointerValue(pv) => {
            let bits = builder
                .build_ptr_to_int(pv, llvm_context().i64_type(), "argsexpr")
                .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?;
            Ok((TlTag::Ptr, bits))
        }
        other => Err(EvalError::Internal(format!("build-call: unsupported argument kind {:?}", other))),
    }
}

/// Shared by [`llvm_builder_build_call`]/[`llvm_builder_build_call_bool`]/
/// [`llvm_builder_build_call_f64`] (Phase 2f): packs `args` into a
/// stack-allocated `[argc x TlValue]` array, emits a direct `call` to
/// `callee` (the unified ABI's `i32 (ptr args, i32 argc, ptr out, ptr heap)`
/// — `f` is the function *currently being compiled* (the caller, not the
/// callee), needed solely to forward *its own* `heap` parameter
/// ([`heap_param_of`]) as the call's 4th argument, the same way
/// `build-cons`/`build-car`/`build-cdr`/`push-root`/`pop-root` already do;
/// every compiled function takes a `heap` parameter regardless of whether
/// its own body touches `Sexpr` at all, precisely so a call between two
/// arbitrary compiled functions never has a signature mismatch here) through
/// a stack-allocated `out` slot, and returns the raw `i64` bits of `out`'s
/// payload — callers narrow/reinterpret that per the callee's known return
/// type (`ret-is-bool`/`ret-is-f64`, looked up by the typelisp-written
/// `compile` since `AstExpr` itself carries no type tags).
#[cfg(feature = "compile")]
fn llvm_builder_build_call_payload(args: &[RtValue]) -> Result<inkwell::values::IntValue<'static>, EvalError> {
    let b = expect_llvm_builder(&args[0])?;
    let callee = expect_llvm_function(&args[1])?;
    let f = expect_llvm_function(&args[2])?;
    let heap_param = heap_param_of(f)?;
    let call_args = expect_vector(&args[3])?;
    let builder = b.0.borrow();
    let tlvalue_ty = tlvalue_llvm_type();
    let call_args = call_args.borrow();
    let argc = call_args.len();
    let array_ty = tlvalue_ty.array_type(argc as u32);
    let args_alloca = builder
        .build_alloca(array_ty, "callargs")
        .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?;
    let i32_ty = llvm_context().i32_type();
    let zero = i32_ty.const_int(0, false);
    for (i, v) in call_args.iter().enumerate() {
        let basic_v = *expect_llvm_basic_value(v)?;
        let (tag, payload) = tlvalue_tag_and_payload(&builder, basic_v)?;
        let packed = pack_tlvalue(&builder, tag, payload)?;
        let idx = i32_ty.const_int(i as u64, false);
        // SAFETY: `args_alloca` is a fresh `[argc x TlValue]` alloca created
        // just above, and `i < argc` (this loop is over `call_args` itself),
        // so this GEP stays in bounds.
        let elem_ptr = unsafe {
            builder
                .build_gep(array_ty, args_alloca, &[zero, idx], "argelem")
                .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?
        };
        builder
            .build_store(elem_ptr, packed)
            .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?;
    }
    drop(call_args);
    let out_alloca = builder
        .build_alloca(tlvalue_ty, "callout")
        .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?;
    // SAFETY: `args_alloca` is `[argc x TlValue]`; a `[0, 0]` GEP yields a
    // pointer to its first element — the flat `*TlValue` shape the unified
    // ABI's `args` parameter expects (matching what `Interp::call_compiled`
    // passes from the Rust side).
    let args_ptr = unsafe {
        builder
            .build_gep(array_ty, args_alloca, &[zero, zero], "argsptr")
            .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?
    };
    let argc_v = i32_ty.const_int(argc as u64, false);
    builder
        .build_call(*callee, &[args_ptr.into(), argc_v.into(), out_alloca.into(), heap_param.into()], "call")
        .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?;
    let loaded = builder
        .build_load(tlvalue_ty, out_alloca, "callresult")
        .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))?
        .into_struct_value();
    builder
        .build_extract_value(loaded, 1, "callpayload")
        .map_err(|e| EvalError::Panic(format!("build-call: {}", e)))
        .map(|v| v.into_int_value())
}

/// `(build-call b callee f args) -> LlvmValue`: calls `callee` and returns
/// its result as a plain `i64`. `bool`/`f64`-returning callees need
/// [`llvm_builder_build_call_bool`]/[`llvm_builder_build_call_f64`] instead.
#[cfg(feature = "compile")]
fn llvm_builder_build_call(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let payload = llvm_builder_build_call_payload(args)?;
    Ok(RtValue::LlvmValue(payload.into()))
}

/// `(build-call-bool b callee f args) -> LlvmValue`: as [`llvm_builder_build_call`],
/// narrowing (`build_int_truncate`) the raw `i64` payload down to `i1`.
#[cfg(feature = "compile")]
fn llvm_builder_build_call_bool(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let payload = llvm_builder_build_call_payload(args)?;
    let b = expect_llvm_builder(&args[0])?;
    let truncated = b
        .0
        .borrow()
        .build_int_truncate(payload, llvm_context().bool_type(), "callbool")
        .map_err(|e| EvalError::Panic(format!("build-call-bool: {}", e)))?;
    Ok(RtValue::LlvmValue(truncated.into()))
}

/// `(build-call-f64 b callee f args) -> LlvmValue`: as [`llvm_builder_build_call`],
/// bit-casting (not converting) the raw `i64` payload back to `f64`.
#[cfg(feature = "compile")]
fn llvm_builder_build_call_f64(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let payload = llvm_builder_build_call_payload(args)?;
    let b = expect_llvm_builder(&args[0])?;
    let bits = b
        .0
        .borrow()
        .build_bit_cast(payload, llvm_context().f64_type(), "callf64")
        .map_err(|e| EvalError::Panic(format!("build-call-f64: {}", e)))?;
    Ok(RtValue::LlvmValue(bits))
}

/// `(build-call-char b callee f args) -> LlvmValue`: as [`llvm_builder_build_call`],
/// narrowing (`build_int_truncate`) the raw `i64` payload down to `i32`.
#[cfg(feature = "compile")]
fn llvm_builder_build_call_char(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let payload = llvm_builder_build_call_payload(args)?;
    let b = expect_llvm_builder(&args[0])?;
    let truncated = b
        .0
        .borrow()
        .build_int_truncate(payload, llvm_context().i32_type(), "callchar")
        .map_err(|e| EvalError::Panic(format!("build-call-char: {}", e)))?;
    Ok(RtValue::LlvmValue(truncated.into()))
}

/// `(build-call-sexpr b callee f args) -> LlvmValue`: as [`llvm_builder_build_call`],
/// reinterpreting (`build_int_to_ptr`) the raw `i64` payload back to a pointer.
#[cfg(feature = "compile")]
fn llvm_builder_build_call_sexpr(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let payload = llvm_builder_build_call_payload(args)?;
    let b = expect_llvm_builder(&args[0])?;
    let ptr_ty = llvm_context().ptr_type(inkwell::AddressSpace::default());
    let ptr = b
        .0
        .borrow()
        .build_int_to_ptr(payload, ptr_ty, "callsexpr")
        .map_err(|e| EvalError::Panic(format!("build-call-sexpr: {}", e)))?;
    Ok(RtValue::LlvmValue(ptr.into()))
}

#[cfg(feature = "compile")]
fn out_ptr_of(f: &inkwell::values::FunctionValue<'static>) -> Result<inkwell::values::PointerValue<'static>, EvalError> {
    Ok(f.get_nth_param(2)
        .ok_or_else(|| EvalError::Internal("build-ret: function has no out parameter".into()))?
        .into_pointer_value())
}

/// `(build-ret b f v) -> Unit`: returns `v` (a plain `i64`) as `TlTag::I64`.
/// `bool`/`f64`/`char` returns need [`llvm_builder_build_ret_bool`]/
/// [`llvm_builder_build_ret_f64`]/[`llvm_builder_build_ret_char`] instead
/// (Phase 2b/2c/2d).
#[cfg(feature = "compile")]
fn llvm_builder_build_ret(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let v = expect_llvm_int_value(&args[2])?;
    let builder = b.0.borrow();
    let out_ptr = out_ptr_of(f)?;
    build_ret_tlvalue(&builder, out_ptr, TlTag::I64, v)
}

/// `(build-ret-bool b f v) -> Unit`: widens `v` (an `i1`) to `i64`
/// (`build_int_z_extend`) and returns it as `TlTag::Bool` — Phase 2b's
/// `bool`-return support, the counterpart to [`llvm_builder_build_ret`].
#[cfg(feature = "compile")]
fn llvm_builder_build_ret_bool(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let v = expect_llvm_int_value(&args[2])?;
    let builder = b.0.borrow();
    let widened = builder
        .build_int_z_extend(v, llvm_context().i64_type(), "retbool")
        .map_err(|e| EvalError::Panic(format!("build-ret-bool: {}", e)))?;
    let out_ptr = out_ptr_of(f)?;
    build_ret_tlvalue(&builder, out_ptr, TlTag::Bool, widened)
}

/// `(build-ret-f64 b f v) -> Unit`: reinterprets `v` (`build_bit_cast`, not a
/// numeric conversion) as an `i64`'s worth of raw bits and returns it as
/// `TlTag::F64` — Phase 2c's `f64`-return support, the counterpart to
/// [`llvm_builder_build_ret`]; see [`llvm_builder_load_arg_f64`] for the
/// matching reverse direction.
#[cfg(feature = "compile")]
fn llvm_builder_build_ret_f64(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let v = expect_llvm_float_value(&args[2])?;
    let builder = b.0.borrow();
    let bits = builder
        .build_bit_cast(v, llvm_context().i64_type(), "retf64")
        .map_err(|e| EvalError::Panic(format!("build-ret-f64: {}", e)))?
        .into_int_value();
    let out_ptr = out_ptr_of(f)?;
    build_ret_tlvalue(&builder, out_ptr, TlTag::F64, bits)
}

/// `(build-ret-char b f v) -> Unit`: widens `v` (an `i32`) to `i64`
/// (`build_int_z_extend`) and returns it as `TlTag::Char` — Phase 2d's
/// `char`-return support, the counterpart to [`llvm_builder_build_ret`] (the
/// same zext shape [`llvm_builder_build_ret_bool`] uses, just from `i32`
/// instead of `i1`).
#[cfg(feature = "compile")]
fn llvm_builder_build_ret_char(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let v = expect_llvm_int_value(&args[2])?;
    let builder = b.0.borrow();
    let widened = builder
        .build_int_z_extend(v, llvm_context().i64_type(), "retchar")
        .map_err(|e| EvalError::Panic(format!("build-ret-char: {}", e)))?;
    let out_ptr = out_ptr_of(f)?;
    build_ret_tlvalue(&builder, out_ptr, TlTag::Char, widened)
}

/// `(build-ret-sexpr b f v) -> Unit`: reinterprets `v` (`build_ptr_to_int`,
/// not a numeric conversion) as an `i64`'s worth of raw bits and returns it
/// as `TlTag::Ptr` — Phase 3's `Sexpr`-return support, the counterpart to
/// [`llvm_builder_build_ret`]; see [`llvm_builder_load_arg_sexpr`] for the
/// matching reverse direction.
#[cfg(feature = "compile")]
fn llvm_builder_build_ret_sexpr(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let _guard = compile_lock().lock().unwrap();
    let b = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let v = expect_llvm_pointer_value(&args[2])?;
    let builder = b.0.borrow();
    let bits = builder
        .build_ptr_to_int(v, llvm_context().i64_type(), "retsexpr")
        .map_err(|e| EvalError::Panic(format!("build-ret-sexpr: {}", e)))?;
    let out_ptr = out_ptr_of(f)?;
    build_ret_tlvalue(&builder, out_ptr, TlTag::Ptr, bits)
}

#[cfg(feature = "compile")]
fn eval_llvm_builtin_method_fallback(type_name: &Path, method: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    eval_llvm_builtin_method(type_name, method, args)
}

#[cfg(not(feature = "compile"))]
fn eval_llvm_builtin_method_fallback(_type_name: &Path, _method: &str, _args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    None
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
/// `Data` fields, `HashTable` values, or `Vector` elements (a `Closure`'s
/// captured environment is covered separately, since each of its slots is
/// already registered in [`Interp::slots`]). `HashTable` keys never need
/// walking — [`HashKey`] is restricted to scalar variants that can't carry a
/// `Sexpr`.
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
        RtValue::Vector(vec) => {
            for f in vec.borrow().iter() {
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
/// have no `defmethod` body to run — currently `HashTable<K,V>` and
/// `Vector<T>` (`crate::check::registry`'s `hashtable_def`/`vector_def`).
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
    if *type_name == Path::root("vector") {
        return match method {
            "new" => Some(vector_new(args)),
            "get" => Some(vector_get(args)),
            "set" => Some(vector_set(args)),
            "length" => Some(vector_length(args)),
            "push" => Some(vector_push(args)),
            "pop" => Some(vector_pop(args)),
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
    None
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

fn expect_vector(v: &RtValue) -> Result<&Rc<RefCell<Vec<RtValue>>>, EvalError> {
    match v {
        RtValue::Vector(v) => Ok(v),
        other => Err(EvalError::Internal(format!("expected a Vector, got {:?}", other))),
    }
}

/// Resolve an `i32` index against a vector's current length: out of range
/// (including negative) is `None`, the caller turns that into a panic — the
/// type system can't express the bound, the same "runtime panic for what
/// types can't catch" precedent as `car`/`cdr` on a non-`Cons` `Sexpr`.
fn vector_index(i: i64, len: usize) -> Option<usize> {
    if i >= 0 && (i as usize) < len { Some(i as usize) } else { None }
}

fn vector_new(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let n = rt_i64(&args[0])?;
    if n < 0 {
        return Err(EvalError::Panic(format!("Vector::new: negative length {}", n)));
    }
    Ok(RtValue::Vector(Rc::new(RefCell::new(vec![args[1].clone(); n as usize]))))
}

fn vector_get(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let v = expect_vector(&args[0])?;
    let i = rt_i64(&args[1])?;
    let vec = v.borrow();
    match vector_index(i, vec.len()) {
        Some(idx) => Ok(vec[idx].clone()),
        None => Err(EvalError::Panic(format!("Vector::get: index {} out of range (length {})", i, vec.len()))),
    }
}

fn vector_set(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let v = expect_vector(&args[0])?;
    let i = rt_i64(&args[1])?;
    let mut vec = v.borrow_mut();
    match vector_index(i, vec.len()) {
        Some(idx) => {
            vec[idx] = args[2].clone();
            Ok(RtValue::Unit)
        }
        None => Err(EvalError::Panic(format!("Vector::set: index {} out of range (length {})", i, vec.len()))),
    }
}

fn vector_length(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let v = expect_vector(&args[0])?;
    Ok(RtValue::Int(v.borrow().len() as i64))
}

fn vector_push(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let v = expect_vector(&args[0])?;
    v.borrow_mut().push(args[1].clone());
    Ok(RtValue::Unit)
}

fn vector_pop(args: &[RtValue]) -> Result<RtValue, EvalError> {
    let v = expect_vector(&args[0])?;
    Ok(option_value(v.borrow_mut().pop()))
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
    match vector_index(i, chars.len()) {
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
