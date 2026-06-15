//! A tree-walking interpreter over the checker's typed AST.
//!
//! Top-level definitions (`defun`/`defmethod`) are registered by fully-qualified
//! name; expressions are evaluated against those registries plus a lexical
//! environment. Functions are not closures — a body sees only its parameters and
//! the global definitions, matching top-level `defun`/`defmethod` semantics.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::{Expr, Path, Pattern, TopLevel, Typed};

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

fn slot(v: RtValue) -> Slot {
    Rc::new(RefCell::new(v))
}

/// The interpreter state: free functions (by [`Path`]), type-associated methods
/// (by type [`Path`] and method name), and global variables (by [`Path`]).
pub struct Interp {
    fns: HashMap<Path, FnDef>,
    methods: HashMap<(Path, String), FnDef>,
    globals: HashMap<Path, Slot>,
}

impl Interp {
    pub fn new() -> Interp {
        Interp { fns: HashMap::new(), methods: HashMap::new(), globals: HashMap::new() }
    }

    /// Execute a checked top-level form. Definitions register and return `None`;
    /// a bare expression returns `Some(value)`.
    pub fn exec(&mut self, tl: TopLevel) -> Result<Option<RtValue>, EvalError> {
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
                let v = self.eval(&value, &Env::new())?;
                self.globals.insert(name, slot(v));
                Ok(None)
            }
            TopLevel::Module { body, .. } => {
                let mut last = None;
                for t in body {
                    last = self.exec(t)?;
                }
                Ok(last)
            }
            TopLevel::Expr(t) => Ok(Some(self.eval(&t, &Env::new())?)),
        }
    }

    fn eval(&self, t: &Typed, env: &Env) -> Result<RtValue, EvalError> {
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
            Expr::If(c, then, els) => match self.eval(c, env)? {
                RtValue::Bool(true) => self.eval(then, env),
                RtValue::Bool(false) => self.eval(els, env),
                _ => Err(EvalError::Internal("if condition is not a bool".into())),
            },
            Expr::Let(binds, body) => {
                // CL `let`: binding values are evaluated in the outer environment.
                let mut child = env.clone();
                for (name, val) in binds {
                    let v = self.eval(val, env)?;
                    child.push((name.clone(), slot(v)));
                }
                self.eval_seq(body, &child)
            }
            Expr::Call(name, args) => {
                let argv = self.eval_args(args, env)?;
                if let Some(f) = self.fns.get(name) {
                    self.apply(&f.params, &f.body, argv)
                } else if let Some(result) = name.is_simple().then(|| eval_builtin(name.local(), &argv)).flatten() {
                    result
                } else {
                    Err(EvalError::NoSuchFunction(name.to_string()))
                }
            }
            Expr::Assoc { type_name, method, args, .. } => {
                let argv = self.eval_args(args, env)?;
                let m = self
                    .methods
                    .get(&(type_name.clone(), method.clone()))
                    .ok_or_else(|| EvalError::NoSuchFunction(format!("{}::{}", type_name, method)))?;
                self.apply(&m.params, &m.body, argv)
            }
            Expr::Construct { type_name, variant, args } => {
                let fields = self.eval_args(args, env)?;
                Ok(RtValue::Data { type_name: type_name.clone(), variant: *variant, fields })
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
                let f = self.eval(callee, env)?;
                let argv = self.eval_args(args, env)?;
                match f {
                    RtValue::Closure(c) => {
                        if c.params.len() != argv.len() {
                            return Err(EvalError::Internal("closure arity mismatch".into()));
                        }
                        let mut cenv = c.env.clone();
                        for (n, v) in c.params.iter().zip(argv) {
                            cenv.push((n.clone(), slot(v)));
                        }
                        self.eval_seq(&c.body, &cenv)
                    }
                    RtValue::Builtin(name) => match eval_builtin(&name, &argv) {
                        Some(r) => r,
                        None => Err(EvalError::NoSuchFunction(name)),
                    },
                    _ => Err(EvalError::Internal("apply of a non-function value".into())),
                }
            }
            Expr::Match(scrut, arms) => {
                let v = self.eval(scrut, env)?;
                for arm in arms {
                    if let Some(binds) = match_pattern(&arm.pat, &v) {
                        let mut child = env.clone();
                        child.extend(binds.into_iter().map(|(n, v)| (n, slot(v))));
                        return self.eval_seq(&arm.body, &child);
                    }
                }
                Err(EvalError::Internal("no matching match arm".into()))
            }
            Expr::Set(name, value) => {
                let v = self.eval(value, env)?;
                let cell = env_get(env, name).ok_or_else(|| EvalError::Unbound(name.clone()))?;
                *cell.borrow_mut() = v.clone();
                Ok(v)
            }
            Expr::SetGlobal(path, value) => {
                let v = self.eval(value, env)?;
                let cell = self
                    .globals
                    .get(path)
                    .ok_or_else(|| EvalError::Unbound(path.to_string()))?;
                *cell.borrow_mut() = v.clone();
                Ok(v)
            }
            Expr::While(cond, body) => {
                while matches!(self.eval(cond, env)?, RtValue::Bool(true)) {
                    for e in body {
                        self.eval(e, env)?;
                    }
                }
                Ok(RtValue::Unit)
            }
            Expr::Panic(msg) => match self.eval(msg, env)? {
                RtValue::Str(s) => Err(EvalError::Panic(s)),
                _ => Err(EvalError::Panic(String::new())),
            },
        }
    }

    /// Apply a function/method body: bind `params` to `args` and run the body.
    fn apply(&self, params: &[String], body: &[Typed], args: Vec<RtValue>) -> Result<RtValue, EvalError> {
        if params.len() != args.len() {
            return Err(EvalError::Internal("arity mismatch".into()));
        }
        let env: Env = params.iter().cloned().zip(args.into_iter().map(slot)).collect();
        self.eval_seq(body, &env)
    }

    fn eval_args(&self, args: &[Typed], env: &Env) -> Result<Vec<RtValue>, EvalError> {
        args.iter().map(|a| self.eval(a, env)).collect()
    }

    /// Evaluate a body sequence, returning the last value (`Unit` if empty).
    fn eval_seq(&self, body: &[Typed], env: &Env) -> Result<RtValue, EvalError> {
        let mut result = RtValue::Unit;
        for e in body {
            result = self.eval(e, env)?;
        }
        Ok(result)
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

/// Variant index of `Sexpr::Cons` (see `check::registry::sexpr_def`): a
/// 2-field constructor holding `(car, cdr)`, both `Sexpr`.
const SEXPR_CONS: usize = 7;

/// Evaluate a built-in operator. Returns `None` if `name` is not a builtin, so
/// the caller can fall through to a "no such function" error. (MVP: i32
/// arithmetic/comparison only; integer divide/mod by zero `panic!`s, matching
/// Rust. `cons`/`car`/`cdr` operate on `Sexpr`; `car`/`cdr` of a non-`Cons`
/// `Sexpr` — including `Nil` — `panic!`s.)
fn eval_builtin(name: &str, args: &[RtValue]) -> Option<Result<RtValue, EvalError>> {
    match name {
        "+" | "-" | "*" | "/" | "mod" | "<" | "<=" | ">" | ">=" | "=" | "/=" => {
            eval_int_builtin(name, args)
        }
        "cons" => match (args.first(), args.get(1)) {
            (Some(a), Some(b)) => Some(Ok(RtValue::Data {
                type_name: Path::root("sexpr"),
                variant: SEXPR_CONS,
                fields: vec![a.clone(), b.clone()],
            })),
            _ => Some(Err(EvalError::Internal("cons: expected two arguments".into()))),
        },
        "car" => match args.first() {
            Some(RtValue::Data { variant: SEXPR_CONS, fields, .. }) => Some(Ok(fields[0].clone())),
            Some(_) => Some(Err(EvalError::Panic("car: not a cons".into()))),
            None => Some(Err(EvalError::Internal("car: expected one argument".into()))),
        },
        "cdr" => match args.first() {
            Some(RtValue::Data { variant: SEXPR_CONS, fields, .. }) => Some(Ok(fields[1].clone())),
            Some(_) => Some(Err(EvalError::Panic("cdr: not a cons".into()))),
            None => Some(Err(EvalError::Internal("cdr: expected one argument".into()))),
        },
        _ => None,
    }
}

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
fn match_pattern(pat: &Pattern, v: &RtValue) -> Option<Vec<(String, RtValue)>> {
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
                    binds.extend(match_pattern(p, f)?);
                }
                Some(binds)
            }
            _ => None,
        },
    }
}
