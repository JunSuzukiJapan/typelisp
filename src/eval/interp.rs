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

use crate::{Expr, Heap, MacroExpander, Path, Pattern, QuotedSexpr, TopLevel, Typed, Value};

use super::value::{Closure, EvalError, HashKey, RtValue};

/// A registered function or method body with its parameter names.
struct FnDef {
    /// Parameter names, including the receiver name first for instance methods.
    params: Vec<String>,
    body: Vec<Typed>,
    /// Only ever set for a `defmacro` with a trailing `&rest` parameter (see
    /// [`Interp::expand_macro`]); always `false` for `defun`/`defmethod`,
    /// which `apply` calls 1:1 regardless.
    rest: bool,
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
            TopLevel::Defun { name, params, body, .. } => {
                let params = params.into_iter().map(|(n, _)| n).collect();
                self.fns.insert(name, FnDef { params, body, rest: false });
                Ok(None)
            }
            TopLevel::Defmethod { type_name, method, self_name, params, body, .. } => {
                let mut names: Vec<String> = Vec::new();
                if let Some(s) = self_name {
                    names.push(s);
                }
                names.extend(params.into_iter().map(|(n, _)| n));
                self.methods.insert((type_name, method), FnDef { params: names, body, rest: false });
                Ok(None)
            }
            TopLevel::Defmacro { name, params, body, rest } => {
                // A macro's body is callable exactly like a `defun`'s — see
                // `MacroExpander`/`Self::expand_macro` — so it's stored in
                // the very same `fns` table; no separate macro table exists.
                self.fns.insert(name, FnDef { params, body, rest });
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
            Expr::Call(name, args) => {
                let (argv, _slots) = self.eval_args(heap, args, env)?;
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
                    _ => Err(EvalError::Internal("apply of a non-function value".into())),
                }
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
    /// (MVP: i32 arithmetic/comparison only; integer divide/mod by zero
    /// panics, matching Rust. `cons`/`car`/`cdr` operate on `Sexpr`;
    /// `car`/`cdr` of a non-`Cons` `Sexpr` — including `Nil` — panics.
    /// `gensym` returns a fresh `Sexpr::Sym` each call.)
    fn eval_builtin(&self, heap: &mut Heap, name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
        match name {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                eval_int_builtin(name, args)
            }
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

/// Evaluate a built-in i32 arithmetic/comparison operator.
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
