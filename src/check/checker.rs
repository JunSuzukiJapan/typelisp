//! The type checker: lowers read `Sexpr` values into a typed AST while checking.
//!
//! Checking is bidirectional in a small way: literal and constructor nodes may
//! be checked *against* an expected type (so integer literals adopt the expected
//! integer type and a nullary `None` learns its type argument), while everything
//! else synthesizes its own type and is reconciled against the expectation.

use std::collections::{HashMap, HashSet};

use crate::{parse_type, Error, Heap, Type, Value};

use super::ast::{Arm, Expr, Pattern, Typed};
use super::registry::{FnSig, Registry};

/// A checked top-level form.
#[derive(Clone, Debug, PartialEq)]
pub enum TopLevel {
    /// A `defun`: name, typed parameters, return type, and checked body.
    Defun {
        name: String,
        params: Vec<(String, Type)>,
        ret: Type,
        body: Vec<Typed>,
    },
    /// A bare top-level expression.
    Expr(Typed),
}

/// A lexical environment mapping variable names to their types.
#[derive(Clone)]
struct Env {
    vars: Vec<(String, Type)>,
}

impl Env {
    fn new() -> Env {
        Env { vars: Vec::new() }
    }

    fn get(&self, name: &str) -> Option<&Type> {
        self.vars.iter().rev().find(|(n, _)| n == name).map(|(_, t)| t)
    }

    /// A child environment with `binds` added (later bindings shadow earlier).
    fn extended(&self, binds: Vec<(String, Type)>) -> Env {
        let mut vars = self.vars.clone();
        vars.extend(binds);
        Env { vars }
    }
}

/// The type checker, holding the data-type and function registries.
pub struct Checker {
    reg: Registry,
}

impl Checker {
    pub fn new() -> Checker {
        Checker { reg: Registry::with_builtins() }
    }

    /// Check one top-level form. A `(defun ...)` registers its signature and
    /// checks its body; anything else is checked as an expression.
    pub fn check_form(&mut self, heap: &Heap, v: Value) -> Result<TopLevel, Error> {
        if let Value::Cons(_) = v {
            let elems = heap.list_to_vec(v)?;
            if let Some(Value::Symbol(id)) = elems.first() {
                if heap.symbol_name(*id) == "defun" {
                    return self.check_defun(heap, &elems[1..]);
                }
            }
        }
        let env = Env::new();
        let t = self.check(heap, &env, v, None)?;
        Ok(TopLevel::Expr(t))
    }

    // ---- defun ------------------------------------------------------------

    fn check_defun(&mut self, heap: &Heap, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.len() < 3 {
            return Err(Error::TypeError("defun: (defun name (params) ret body...)".into()));
        }
        let name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("defun: name must be a symbol".into())),
        };
        let params = self.parse_params(heap, parts[1])?;
        let ret = parse_type(heap, parts[2])?;

        // Register the signature before checking the body so self-recursion works.
        let sig = FnSig { params: params.iter().map(|(_, t)| t.clone()).collect(), ret: ret.clone() };
        self.reg.fns.insert(name.clone(), sig);

        let env = Env::new().extended(params.clone());
        let (body, _) = self.check_seq(heap, &env, &parts[3..], Some(&ret))?;
        Ok(TopLevel::Defun { name, params, ret, body })
    }

    /// Parse a `((name type)...)` parameter list.
    fn parse_params(&self, heap: &Heap, v: Value) -> Result<Vec<(String, Type)>, Error> {
        let mut out = Vec::new();
        for binding in heap.list_to_vec(v)? {
            let pair = heap.list_to_vec(binding)?;
            if pair.len() != 2 {
                return Err(Error::TypeError("defun: parameter must be (name type)".into()));
            }
            let name = match pair[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("defun: parameter name must be a symbol".into())),
            };
            out.push((name, parse_type(heap, pair[1])?));
        }
        Ok(out)
    }

    // ---- expressions ------------------------------------------------------

    /// Check `v` as an expression, optionally against an `expected` type.
    fn check(
        &self,
        heap: &Heap,
        env: &Env,
        v: Value,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let typed = match v {
            Value::Int(n) => Typed { expr: Expr::Int(n), ty: int_lit_ty(expected) },
            Value::Float(f) => Typed { expr: Expr::Float(f), ty: float_lit_ty(expected) },
            Value::Bool(b) => Typed { expr: Expr::Bool(b), ty: Type::Bool },
            Value::Char(c) => Typed { expr: Expr::Char(c), ty: Type::Char },
            Value::Str(s) => Typed { expr: Expr::Str(heap.string(s).to_string()), ty: Type::Str },
            Value::Empty => {
                // The empty list `()` is the `None` value of `Option<T>` when an
                // option type is expected; otherwise it is the unit value.
                if let Some(Type::Named(n, _)) = expected {
                    if let Some((adt, idx)) = self.reg.variant_index.get("none").cloned() {
                        if *n == adt {
                            return self.check_construct(heap, env, &adt, idx, &[], expected);
                        }
                    }
                }
                Typed { expr: Expr::Unit, ty: Type::Unit }
            }
            Value::Symbol(id) => {
                let name = heap.symbol_name(id);
                match env.get(name) {
                    Some(t) => Typed { expr: Expr::Var(name.to_string()), ty: t.clone() },
                    None => return Err(Error::TypeError(format!("unbound variable: {}", name))),
                }
            }
            Value::Cons(_) => self.check_list(heap, env, v, expected)?,
        };
        // Reconcile synthesized types against the expectation. Literal and
        // constructor nodes already adopted `expected`, so this only fires on a
        // genuine mismatch.
        if let Some(e) = expected {
            if &typed.ty != e {
                return Err(Error::TypeError(format!(
                    "type mismatch: expected {:?}, found {:?}",
                    e, typed.ty
                )));
            }
        }
        Ok(typed)
    }

    /// Dispatch a compound form on its head symbol.
    fn check_list(
        &self,
        heap: &Heap,
        env: &Env,
        v: Value,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let elems = heap.list_to_vec(v)?;
        let head = match elems[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("call head must be a symbol".into())),
        };
        let args = &elems[1..];
        match head.as_str() {
            "if" => self.check_if(heap, env, args, expected),
            "let" => self.check_let(heap, env, args, expected),
            "progn" => {
                let (body, ty) = self.check_seq(heap, env, args, expected)?;
                // Represent progn as a let with no bindings.
                Ok(Typed { expr: Expr::Let(Vec::new(), body), ty })
            }
            "match" => self.check_match(heap, env, args, expected),
            "if-let" => self.check_if_let(heap, env, args, expected),
            _ => {
                if let Some((adt, idx)) = self.reg.variant_index.get(&head).cloned() {
                    self.check_construct(heap, env, &adt, idx, args, expected)
                } else if self.reg.fns.contains_key(&head) {
                    self.check_call(heap, env, &head, args)
                } else {
                    Err(Error::NoSuchFunction(head))
                }
            }
        }
    }

    fn check_if(
        &self,
        heap: &Heap,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.len() != 3 {
            return Err(Error::TypeError("if: (if cond then else)".into()));
        }
        let cond = self.check(heap, env, args[0], Some(&Type::Bool))?;
        let then = self.check(heap, env, args[1], expected)?;
        let then_ty = then.ty.clone();
        let els = self.check(heap, env, args[2], Some(&then_ty))?;
        Ok(Typed {
            expr: Expr::If(Box::new(cond), Box::new(then), Box::new(els)),
            ty: then_ty,
        })
    }

    fn check_let(
        &self,
        heap: &Heap,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("let: (let ((name val)...) body...)".into()));
        }
        let mut binds = Vec::new();
        for binding in heap.list_to_vec(args[0])? {
            let pair = heap.list_to_vec(binding)?;
            if pair.len() != 2 {
                return Err(Error::TypeError("let: binding must be (name val)".into()));
            }
            let name = match pair[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("let: binding name must be a symbol".into())),
            };
            // Binding values are checked in the *outer* environment (CL `let`).
            let val = self.check(heap, env, pair[1], None)?;
            binds.push((name, val));
        }
        let child = env.extended(binds.iter().map(|(n, t)| (n.clone(), t.ty.clone())).collect());
        let (body, ty) = self.check_seq(heap, &child, &args[1..], expected)?;
        Ok(Typed { expr: Expr::Let(binds, body), ty })
    }

    fn check_call(
        &self,
        heap: &Heap,
        env: &Env,
        name: &str,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let sig = self.reg.fns.get(name).expect("caller checked presence").clone();
        if args.len() != sig.params.len() {
            return Err(Error::TypeError(format!(
                "{}: expected {} argument(s), got {}",
                name,
                sig.params.len(),
                args.len()
            )));
        }
        let mut typed = Vec::new();
        for (arg, pty) in args.iter().zip(sig.params.iter()) {
            typed.push(self.check(heap, env, *arg, Some(pty))?);
        }
        Ok(Typed { expr: Expr::Call(name.to_string(), typed), ty: sig.ret })
    }

    fn check_construct(
        &self,
        heap: &Heap,
        env: &Env,
        adt_name: &str,
        variant: usize,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let def = self.reg.adts.get(adt_name).expect("indexed adt exists").clone();
        let fields = &def.variants[variant].fields;
        if args.len() != fields.len() {
            return Err(Error::TypeError(format!(
                "{}: expected {} field(s), got {}",
                def.variants[variant].name,
                fields.len(),
                args.len()
            )));
        }
        let params: HashSet<String> = def.params.iter().cloned().collect();
        let mut subst: HashMap<String, Type> = HashMap::new();

        // Seed the substitution from an expected `Named(adt, args)` type, so a
        // field-less constructor such as `None` can learn its type argument.
        if let Some(Type::Named(n, eargs)) = expected {
            if n == adt_name && eargs.len() == def.params.len() {
                for (p, a) in def.params.iter().zip(eargs.iter()) {
                    subst.insert(p.clone(), a.clone());
                }
            }
        }

        let mut typed_args = Vec::new();
        for (arg, field) in args.iter().zip(fields.iter()) {
            let st = subst_apply(field, &subst);
            let exp = if type_has_param(&st, &params) { None } else { Some(st) };
            let ta = self.check(heap, env, *arg, exp.as_ref())?;
            unify(&params, field, &ta.ty, &mut subst)?;
            typed_args.push(ta);
        }

        let mut result_args = Vec::new();
        for p in &def.params {
            match subst.get(p) {
                Some(t) => result_args.push(t.clone()),
                None => {
                    return Err(Error::TypeError(format!(
                        "cannot infer type argument `{}` for `{}`",
                        p, adt_name
                    )))
                }
            }
        }
        Ok(Typed {
            expr: Expr::Construct {
                type_name: adt_name.to_string(),
                variant,
                args: typed_args,
            },
            ty: Type::Named(adt_name.to_string(), result_args),
        })
    }

    // ---- match / if-let ---------------------------------------------------

    fn check_match(
        &self,
        heap: &Heap,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("match: (match expr arms...)".into()));
        }
        let scrut = self.check(heap, env, args[0], None)?;
        let (adt_name, _) = self.expect_adt(&scrut.ty)?;
        let total_variants = self.reg.adts[&adt_name].variants.len();

        let mut arms = Vec::new();
        let mut covered: HashSet<usize> = HashSet::new();
        let mut catchall = false;
        let mut result_ty: Option<Type> = expected.cloned();

        for arm_val in &args[1..] {
            let parts = heap.list_to_vec(*arm_val)?;
            if parts.is_empty() {
                return Err(Error::TypeError("match: arm must be (pattern body...)".into()));
            }
            let (pat, binds) = self.check_pattern(heap, &scrut.ty, parts[0])?;
            match &pat {
                Pattern::Ctor { variant, .. } => {
                    covered.insert(*variant);
                }
                Pattern::Wildcard | Pattern::Bind(_) => catchall = true,
                _ => {}
            }
            let arm_env = env.extended(binds);
            let (body, body_ty) = self.check_seq(heap, &arm_env, &parts[1..], result_ty.as_ref())?;
            if result_ty.is_none() {
                result_ty = Some(body_ty);
            }
            arms.push(Arm { pat, body });
        }

        if !catchall && covered.len() != total_variants {
            return Err(Error::TypeError(format!(
                "non-exhaustive match on `{}`: {}/{} variants covered",
                adt_name,
                covered.len(),
                total_variants
            )));
        }
        let ty = result_ty.ok_or_else(|| Error::TypeError("match: no arms".into()))?;
        Ok(Typed { expr: Expr::Match(Box::new(scrut), arms), ty })
    }

    fn check_if_let(
        &self,
        heap: &Heap,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.len() != 3 {
            return Err(Error::TypeError("if-let: (if-let (pattern val) then else)".into()));
        }
        let binding = heap.list_to_vec(args[0])?;
        if binding.len() != 2 {
            return Err(Error::TypeError("if-let: binding must be (pattern val)".into()));
        }
        let scrut = self.check(heap, env, binding[1], None)?;
        self.expect_adt(&scrut.ty)?;
        let (pat, binds) = self.check_pattern(heap, &scrut.ty, binding[0])?;

        let then_env = env.extended(binds);
        let then = self.check(heap, &then_env, args[1], expected)?;
        let then_ty = then.ty.clone();
        let els = self.check(heap, env, args[2], Some(&then_ty))?;

        // Desugar to a two-arm match; the wildcard arm makes it exhaustive.
        let arms = vec![
            Arm { pat, body: vec![then] },
            Arm { pat: Pattern::Wildcard, body: vec![els] },
        ];
        Ok(Typed { expr: Expr::Match(Box::new(scrut), arms), ty: then_ty })
    }

    /// Check a pattern against the type of the value it matches, returning the
    /// pattern and the variable bindings it introduces.
    fn check_pattern(
        &self,
        heap: &Heap,
        expected: &Type,
        v: Value,
    ) -> Result<(Pattern, Vec<(String, Type)>), Error> {
        match v {
            Value::Symbol(id) => {
                let name = heap.symbol_name(id);
                if name == "_" {
                    Ok((Pattern::Wildcard, Vec::new()))
                } else {
                    Ok((Pattern::Bind(name.to_string()), vec![(name.to_string(), expected.clone())]))
                }
            }
            Value::Int(n) => {
                if is_integer_type(expected) {
                    Ok((Pattern::Int(n), Vec::new()))
                } else {
                    Err(Error::TypeError(format!(
                        "integer pattern does not match type {:?}",
                        expected
                    )))
                }
            }
            Value::Bool(b) if *expected == Type::Bool => Ok((Pattern::Bool(b), Vec::new())),
            Value::Char(c) if *expected == Type::Char => Ok((Pattern::Char(c), Vec::new())),
            Value::Cons(_) => self.check_ctor_pattern(heap, expected, v),
            other => Err(Error::TypeError(format!("pattern does not match {:?}: {:?}", expected, other))),
        }
    }

    fn check_ctor_pattern(
        &self,
        heap: &Heap,
        expected: &Type,
        v: Value,
    ) -> Result<(Pattern, Vec<(String, Type)>), Error> {
        let parts = heap.list_to_vec(v)?;
        let ctor = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("pattern: constructor must be a symbol".into())),
        };
        let (adt_name, variant) = self
            .reg
            .variant_index
            .get(&ctor)
            .cloned()
            .ok_or_else(|| Error::TypeError(format!("unknown constructor in pattern: {}", ctor)))?;

        let (exp_adt, targs) = self.expect_adt(expected)?;
        if exp_adt != adt_name {
            return Err(Error::TypeError(format!(
                "constructor `{}` belongs to `{}`, but matched value has type `{}`",
                ctor, adt_name, exp_adt
            )));
        }
        let def = self.reg.adts[&adt_name].clone();
        let subst: HashMap<String, Type> =
            def.params.iter().cloned().zip(targs.iter().cloned()).collect();

        let fields = &def.variants[variant].fields;
        if parts.len() - 1 != fields.len() {
            return Err(Error::TypeError(format!(
                "pattern `{}`: expected {} field(s), got {}",
                ctor,
                fields.len(),
                parts.len() - 1
            )));
        }
        let mut sub_pats = Vec::new();
        let mut binds = Vec::new();
        for (sub, field) in parts[1..].iter().zip(fields.iter()) {
            let field_ty = subst_apply(field, &subst);
            let (p, b) = self.check_pattern(heap, &field_ty, *sub)?;
            sub_pats.push(p);
            binds.extend(b);
        }
        Ok((Pattern::Ctor { type_name: adt_name, variant, args: sub_pats }, binds))
    }

    // ---- helpers ----------------------------------------------------------

    /// Check a body sequence (`expr...`), returning the typed exprs and the type
    /// of the last (the sequence's type; `Unit` if empty). Only the last element
    /// is checked against `expected`.
    fn check_seq(
        &self,
        heap: &Heap,
        env: &Env,
        body: &[Value],
        expected: Option<&Type>,
    ) -> Result<(Vec<Typed>, Type), Error> {
        if body.is_empty() {
            if let Some(e) = expected {
                if *e != Type::Unit {
                    return Err(Error::TypeError(format!(
                        "empty body has type Unit, expected {:?}",
                        e
                    )));
                }
            }
            return Ok((Vec::new(), Type::Unit));
        }
        let mut out = Vec::new();
        let last = body.len() - 1;
        for (i, expr) in body.iter().enumerate() {
            let exp = if i == last { expected } else { None };
            out.push(self.check(heap, env, *expr, exp)?);
        }
        let ty = out[last].ty.clone();
        Ok((out, ty))
    }

    /// Require `ty` to be a registered data type, returning its name and args.
    fn expect_adt(&self, ty: &Type) -> Result<(String, Vec<Type>), Error> {
        match ty {
            Type::Named(n, args) if self.reg.adts.contains_key(n) => Ok((n.clone(), args.clone())),
            other => Err(Error::TypeError(format!(
                "expected a data type, found {:?}",
                other
            ))),
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Checker::new()
    }
}

// ---- free helpers ---------------------------------------------------------

fn is_integer_type(t: &Type) -> bool {
    matches!(
        t,
        Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Usize
    )
}

/// The type of an integer literal: the expected integer type if any, else `i32`.
fn int_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t) if is_integer_type(t) => t.clone(),
        _ => Type::I32,
    }
}

/// The type of a float literal: the expected float type if any, else `f64`.
fn float_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t @ Type::F32) | Some(t @ Type::F64) => t.clone(),
        _ => Type::F64,
    }
}

/// Whether `t` mentions any type parameter in `params` (as `Named(p, [])`).
fn type_has_param(t: &Type, params: &HashSet<String>) -> bool {
    match t {
        Type::Named(n, args) => {
            (args.is_empty() && params.contains(n)) || args.iter().any(|a| type_has_param(a, params))
        }
        Type::Fn(ps, r) => ps.iter().any(|p| type_has_param(p, params)) || type_has_param(r, params),
        _ => false,
    }
}

/// Replace type parameters in `t` with their bindings from `subst`.
fn subst_apply(t: &Type, subst: &HashMap<String, Type>) -> Type {
    match t {
        Type::Named(n, args) if args.is_empty() => match subst.get(n) {
            Some(bound) => bound.clone(),
            None => t.clone(),
        },
        Type::Named(n, args) => {
            Type::Named(n.clone(), args.iter().map(|a| subst_apply(a, subst)).collect())
        }
        Type::Fn(ps, r) => Type::Fn(
            ps.iter().map(|p| subst_apply(p, subst)).collect(),
            Box::new(subst_apply(r, subst)),
        ),
        other => other.clone(),
    }
}

/// Unify a field template (which may mention `params`) against a concrete
/// `actual` type, accumulating parameter bindings in `subst`.
fn unify(
    params: &HashSet<String>,
    tmpl: &Type,
    actual: &Type,
    subst: &mut HashMap<String, Type>,
) -> Result<(), Error> {
    if let Type::Named(n, args) = tmpl {
        if args.is_empty() && params.contains(n) {
            return match subst.get(n) {
                Some(bound) if bound == actual => Ok(()),
                Some(bound) => Err(Error::TypeError(format!(
                    "conflicting types for `{}`: {:?} vs {:?}",
                    n, bound, actual
                ))),
                None => {
                    subst.insert(n.clone(), actual.clone());
                    Ok(())
                }
            };
        }
    }
    match (tmpl, actual) {
        (Type::Named(n1, a1), Type::Named(n2, a2)) if n1 == n2 && a1.len() == a2.len() => {
            for (t, a) in a1.iter().zip(a2.iter()) {
                unify(params, t, a, subst)?;
            }
            Ok(())
        }
        (Type::Fn(p1, r1), Type::Fn(p2, r2)) if p1.len() == p2.len() => {
            for (t, a) in p1.iter().zip(p2.iter()) {
                unify(params, t, a, subst)?;
            }
            unify(params, r1, r2, subst)
        }
        _ if tmpl == actual => Ok(()),
        _ => Err(Error::TypeError(format!(
            "type mismatch: expected {:?}, found {:?}",
            tmpl, actual
        ))),
    }
}
