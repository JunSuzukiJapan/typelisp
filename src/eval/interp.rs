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

use crate::{Expr, Heap, Path, Pattern, TopLevel, Typed, Value};

use super::value::{Closure, EvalError, RtValue};

/// A registered function or method body with its parameter names.
struct FnDef {
    /// Parameter names, including the receiver name first for instance methods.
    params: Vec<String>,
    body: Vec<Typed>,
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
}

impl Interp {
    pub fn new() -> Interp {
        Interp {
            fns: HashMap::new(),
            methods: HashMap::new(),
            globals: HashMap::new(),
            slots: RefCell::new(Vec::new()),
            rooted: Cell::new(0),
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
                self.fns.insert(name, FnDef { params, body });
                Ok(None)
            }
            TopLevel::Defmethod { type_name, method, self_name, params, body, .. } => {
                let mut names: Vec<String> = Vec::new();
                if let Some(s) = self_name {
                    names.push(s);
                }
                names.extend(params.into_iter().map(|(n, _)| n));
                self.methods.insert((type_name, method), FnDef { params: names, body });
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
                let m = self
                    .methods
                    .get(&(type_name.clone(), method.clone()))
                    .ok_or_else(|| EvalError::NoSuchFunction(format!("{}::{}", type_name, method)))?;
                self.apply(heap, &m.params, &m.body, argv)
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
    /// `panic!`s, matching Rust. `cons`/`car`/`cdr` operate on `Sexpr`;
    /// `car`/`cdr` of a non-`Cons` `Sexpr` — including `Nil` — `panic!`s.)
    fn eval_builtin(&self, heap: &mut Heap, name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
        match name {
            "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                eval_int_builtin(name, args)
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

impl Default for Interp {
    fn default() -> Self {
        Interp::new()
    }
}

fn env_get<'a>(env: &'a Env, name: &str) -> Option<&'a Slot> {
    env.iter().rev().find(|(n, _)| n == name).map(|(_, s)| s)
}

/// Whether `type_name` is the built-in `Sexpr` type, whose values are
/// represented by [`RtValue::Sexpr`] (heap-backed) rather than
/// [`RtValue::Data`].
fn is_sexpr_type(type_name: &Path) -> bool {
    *type_name == Path::root("sexpr")
}

/// Recursively gather every `Sexpr` value reachable from `v` through nested
/// `Data` fields (a `Closure`'s captured environment is covered separately,
/// since each of its slots is already registered in [`Interp::slots`]).
fn collect_sexpr_roots(v: &RtValue, out: &mut Vec<Value>) {
    match v {
        RtValue::Sexpr(val) => out.push(*val),
        RtValue::Data { fields, .. } => {
            for f in fields {
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
