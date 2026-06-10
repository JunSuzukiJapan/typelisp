//! Bidirectional type checker.
//!
//! Full Hindley–Milner inference is unnecessary: every function parameter,
//! struct field, method receiver and lambda parameter is type-annotated, and
//! function/method return types are declared. The only inference is (a) the
//! type of an untyped `let`/`defvar` binding (= its initializer's type) and
//! (b) numeric-literal defaulting with expected-type override (grammar §7).

use std::collections::HashMap;

use crate::{
    BinOp, CallTarget, DefMethod, DefVar, Error, ExprKind, Form, Pattern, Place, Receiver, Type,
    TypedExpr, UnOp,
};

/// A function/method signature.
#[derive(Clone, Debug)]
pub struct FnSig {
    pub params: Vec<Type>,
    pub ret: Type,
}

/// A registered struct definition.
#[derive(Clone, Debug)]
pub struct StructDef {
    pub generics: Vec<String>,
    /// All fields in declaration order (pub group then private group).
    pub fields: Vec<(String, Type)>,
}

/// The checked program plus the registries code generation needs.
pub struct CheckedProgram {
    pub forms: Vec<Form>,
    pub structs: HashMap<String, StructDef>,
    pub fns: HashMap<String, FnSig>,
}

pub struct Checker {
    scopes: Vec<HashMap<String, (Type, bool)>>, // name -> (type, mutable)
    fns: HashMap<String, FnSig>,
    structs: HashMap<String, StructDef>,
    instance_methods: HashMap<(String, String), (Type, FnSig)>, // (recv key, name) -> (recv, sig)
    static_methods: HashMap<String, Vec<(Type, FnSig)>>, // name -> [(recv, sig)]
}

impl Checker {
    pub fn new() -> Checker {
        Checker {
            scopes: vec![HashMap::new()],
            fns: HashMap::new(),
            structs: HashMap::new(),
            instance_methods: HashMap::new(),
            static_methods: HashMap::new(),
        }
    }

    /// Type-check a whole program, annotating expression nodes in place.
    pub fn check_program(mut self, mut forms: Vec<Form>) -> Result<CheckedProgram, Error> {
        // Pass 0: collect signatures so forward references and recursion work.
        for form in &forms {
            self.collect(form)?;
        }
        // Pass 1: check bodies.
        for form in &mut forms {
            self.check_form(form)?;
        }
        Ok(CheckedProgram { forms, structs: self.structs, fns: self.fns })
    }

    // ------------------------------------------------------------------
    // Pass 0: collection
    // ------------------------------------------------------------------

    fn collect(&mut self, form: &Form) -> Result<(), Error> {
        match form {
            Form::Defun(d) => {
                if self.fns.contains_key(&d.name) {
                    return Err(Error::DuplicateDefinition(d.name.clone()));
                }
                self.fns.insert(
                    d.name.clone(),
                    FnSig { params: d.params.iter().map(|(_, t)| t.clone()).collect(), ret: d.ret.clone() },
                );
            }
            Form::DefStruct(s) => {
                let mut fields = s.pub_fields.clone();
                fields.extend(s.priv_fields.clone());
                self.structs.insert(s.name.clone(), StructDef { generics: s.generics.clone(), fields });
            }
            Form::DefMethod(m) => {
                let sig = FnSig {
                    params: m.params.iter().map(|(_, t)| t.clone()).collect(),
                    ret: m.ret.clone(),
                };
                match &m.recv {
                    Receiver::Instance(t) => {
                        self.instance_methods
                            .insert((type_key(t), m.name.clone()), (t.clone(), sig));
                    }
                    Receiver::Static(t) => {
                        self.static_methods.entry(m.name.clone()).or_default().push((t.clone(), sig));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Pass 1: checking
    // ------------------------------------------------------------------

    fn check_form(&mut self, form: &mut Form) -> Result<(), Error> {
        match form {
            Form::Defun(d) => self.check_fn_body(&d.params, &d.ret, &mut d.body),
            Form::DefMethod(m) => self.check_method_body(m),
            Form::DefVar(d) | Form::DefConstant(d) => {
                self.check_defvar(d)?;
                Ok(())
            }
            Form::Expr(e) => {
                self.synth(e)?;
                Ok(())
            }
            Form::DefStruct(_) | Form::Use(_) => Ok(()),
            Form::Module { items, .. } => {
                for item in items.iter() {
                    self.collect(item)?;
                }
                for item in items.iter_mut() {
                    self.check_form(item)?;
                }
                Ok(())
            }
        }
    }

    fn check_fn_body(
        &mut self,
        params: &[(String, Type)],
        ret: &Type,
        body: &mut [TypedExpr],
    ) -> Result<(), Error> {
        self.push_scope();
        for (name, ty) in params {
            self.bind(name, ty.clone(), true);
        }
        let r = self.check_body_against_ret(body, ret);
        self.pop_scope();
        r
    }

    fn check_method_body(&mut self, m: &mut DefMethod) -> Result<(), Error> {
        self.push_scope();
        if let Receiver::Instance(t) = &m.recv {
            self.bind("self", t.clone(), true);
        }
        for (name, ty) in &m.params {
            self.bind(name, ty.clone(), true);
        }
        let r = self.check_body_against_ret(&mut m.body, &m.ret);
        self.pop_scope();
        r
    }

    /// Check a function/method body against its declared return type. A `Unit`
    /// return ignores the body's value; otherwise the body's last expression is
    /// checked against `ret` (so return-position literals adopt the right type).
    fn check_body_against_ret(&mut self, body: &mut [TypedExpr], ret: &Type) -> Result<(), Error> {
        if *ret == Type::Unit {
            self.synth_body(body)?;
            Ok(())
        } else {
            self.check_body(body, ret)
        }
    }

    /// Check a body (implicit `progn`): synth all but the last expression, then
    /// check the last against `expected`.
    fn check_body(&mut self, body: &mut [TypedExpr], expected: &Type) -> Result<(), Error> {
        if body.is_empty() {
            return self.assignable(&Type::Unit, expected);
        }
        let last = body.len() - 1;
        for e in body[..last].iter_mut() {
            self.synth(e)?;
        }
        self.check(&mut body[last], expected)
    }

    fn check_defvar(&mut self, d: &mut DefVar) -> Result<Type, Error> {
        let ty = match &d.ty {
            Some(t) => {
                self.check(&mut d.init, t)?;
                t.clone()
            }
            None => self.synth(&mut d.init)?,
        };
        self.bind(&d.name, ty.clone(), d.mutable);
        Ok(ty)
    }

    /// Type-check a body (implicit `progn`); returns the last expr's type.
    fn synth_body(&mut self, body: &mut [TypedExpr]) -> Result<Type, Error> {
        let mut ty = Type::Unit;
        for e in body.iter_mut() {
            ty = self.synth(e)?;
        }
        Ok(ty)
    }

    // ------------------------------------------------------------------
    // synth / check
    // ------------------------------------------------------------------

    /// Infer the type of `e`, annotating `e.ty`.
    fn synth(&mut self, e: &mut TypedExpr) -> Result<Type, Error> {
        let ty = self.synth_kind(e)?;
        e.ty = Some(ty.clone());
        Ok(ty)
    }

    fn synth_kind(&mut self, e: &mut TypedExpr) -> Result<Type, Error> {
        match &mut e.kind {
            ExprKind::Int(_, suffix) => Ok(suffix.clone().unwrap_or_else(Type::default_int)),
            ExprKind::Float(_, suffix) => Ok(suffix.clone().unwrap_or_else(Type::default_float)),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::Char(_) => Ok(Type::Char),
            ExprKind::Str(_) => Ok(Type::Str),
            ExprKind::Unit => Ok(Type::Unit),
            ExprKind::Break => Ok(Type::Unit),
            ExprKind::Quote(_) => Ok(Type::Named("Sexpr".to_string(), vec![])),

            ExprKind::Var(name) => self
                .lookup(name)
                .map(|(t, _)| t)
                .ok_or_else(|| Error::UndefinedVariable(name.clone())),

            ExprKind::BinOp { op, lhs, rhs } => {
                let op = *op;
                self.synth_binop(op, lhs, rhs)
            }
            ExprKind::UnOp { op, operand } => {
                let op = *op;
                match op {
                    UnOp::Not => {
                        self.check(operand, &Type::Bool)?;
                        Ok(Type::Bool)
                    }
                    UnOp::Inc1 | UnOp::Dec1 => {
                        let t = self.synth(operand)?;
                        if !t.is_integer() {
                            return Err(Error::NotNumeric(t));
                        }
                        Ok(t)
                    }
                }
            }

            ExprKind::If { cond, then, els } => {
                self.check(cond, &Type::Bool)?;
                match els {
                    Some(els) => {
                        let tt = self.synth(then)?;
                        self.check(els, &tt)
                            .map_err(|_| Error::BranchTypeMismatch("if".to_string()))?;
                        Ok(tt)
                    }
                    None => {
                        self.synth(then)?;
                        Ok(Type::Unit)
                    }
                }
            }
            ExprKind::When { cond, body } | ExprKind::Unless { cond, body } => {
                self.check(cond, &Type::Bool)?;
                self.synth_body(body)?;
                Ok(Type::Unit)
            }
            ExprKind::While { cond, body } => {
                self.check(cond, &Type::Bool)?;
                self.push_scope();
                let r = self.synth_body(body);
                self.pop_scope();
                r?;
                Ok(Type::Unit)
            }
            ExprKind::Loop { body } => {
                self.push_scope();
                let r = self.synth_body(body);
                self.pop_scope();
                r?;
                Ok(Type::Unit)
            }
            ExprKind::Dotimes { var, count, body } => {
                let ct = self.synth(count)?;
                if !ct.is_integer() {
                    return Err(Error::NotNumeric(ct));
                }
                self.push_scope();
                self.bind(var, ct, false);
                let r = self.synth_body(body);
                self.pop_scope();
                r?;
                Ok(Type::Unit)
            }
            ExprKind::Progn { body } => self.synth_body(body),

            ExprKind::Let { bindings, body } => {
                self.push_scope();
                for b in bindings.iter_mut() {
                    let t = match &b.ty {
                        Some(t) => {
                            self.check(&mut b.init, t)?;
                            t.clone()
                        }
                        None => self.synth(&mut b.init)?,
                    };
                    self.bind(&b.name, t, false);
                }
                let r = self.synth_body(body);
                self.pop_scope();
                r
            }

            ExprKind::Cond { clauses } => {
                let mut result: Option<Type> = None;
                for c in clauses.iter_mut() {
                    // `true` default clause is allowed; otherwise test is bool.
                    self.check(&mut c.test, &Type::Bool).ok();
                    let bt = self.synth_body(&mut c.body)?;
                    match &result {
                        None => result = Some(bt),
                        Some(r) if *r == bt => {}
                        Some(_) => result = Some(Type::Unit), // statement use
                    }
                }
                Ok(result.unwrap_or(Type::Unit))
            }

            ExprKind::Match { scrutinee, arms } => {
                let st = self.synth(scrutinee)?;
                let mut result: Option<Type> = None;
                for arm in arms.iter_mut() {
                    self.push_scope();
                    for p in &arm.patterns {
                        self.bind_pattern(p, &st);
                    }
                    let bt = self.synth_body(&mut arm.body);
                    self.pop_scope();
                    let bt = bt?;
                    match &result {
                        None => result = Some(bt),
                        Some(r) if *r == bt => {}
                        Some(_) => result = Some(Type::Unit),
                    }
                }
                Ok(result.unwrap_or(Type::Unit))
            }

            ExprKind::Setf { place, value } => {
                let pt = self.place_type(place, true)?;
                self.check(value, &pt)?;
                Ok(Type::Unit)
            }
            ExprKind::Incf(place) | ExprKind::Decf(place) => {
                let pt = self.place_type(place, true)?;
                if !pt.is_integer() {
                    return Err(Error::NotNumeric(pt));
                }
                Ok(Type::Unit)
            }

            ExprKind::Lambda { params, body, .. } => {
                self.push_scope();
                for (name, ty) in params.iter() {
                    self.bind(name, ty.clone(), true);
                }
                let ret = self.synth_body(body);
                self.pop_scope();
                let ret = ret?;
                let pts = params.iter().map(|(_, t)| t.clone()).collect();
                Ok(Type::Fn(pts, Box::new(ret)))
            }

            ExprKind::FieldAccess { object, field } => {
                let ot = self.synth(object)?;
                let key = type_key(&ot);
                let sd = self
                    .structs
                    .get(&key)
                    .ok_or_else(|| Error::UnknownField { struct_name: key.clone(), field: field.clone() })?;
                sd.fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, t)| t.clone())
                    .ok_or_else(|| Error::UnknownField { struct_name: key, field: field.clone() })
            }

            ExprKind::MethodCall { method, receiver, args } => {
                let rt = self.synth(receiver)?;
                let key = type_key(&rt);
                // `(.field obj)` sugar: with no args, prefer a field of the same name.
                if args.is_empty() {
                    if let Some(sd) = self.structs.get(&key) {
                        if let Some((_, ft)) = sd.fields.iter().find(|(n, _)| n == method) {
                            return Ok(ft.clone());
                        }
                    }
                }
                let (_, sig) = self
                    .instance_methods
                    .get(&(key.clone(), method.clone()))
                    .cloned()
                    .ok_or_else(|| Error::NoSuchMethod { recv: key, name: method.clone() })?;
                self.check_args(method, &sig.params, args)?;
                Ok(sig.ret)
            }

            ExprKind::DefVarLocal(d) => {
                self.check_defvar(d)?;
                Ok(Type::Unit)
            }

            ExprKind::Call { target, args } => {
                let target = target.clone();
                self.synth_call(&target, args)
            }
        }
    }

    fn synth_binop(
        &mut self,
        op: BinOp,
        lhs: &mut TypedExpr,
        rhs: &mut TypedExpr,
    ) -> Result<Type, Error> {
        let ca = self.operand_concrete(lhs)?;
        let cb = self.operand_concrete(rhs)?;
        // Determine the common operand type, honouring literal defaulting.
        let t = match (ca, cb) {
            (Some(ta), Some(tb)) => {
                if ta != tb {
                    return Err(Error::TypeMismatch { expected: ta, found: tb });
                }
                ta
            }
            (Some(ta), None) => {
                self.check(rhs, &ta)?;
                ta
            }
            (None, Some(tb)) => {
                self.check(lhs, &tb)?;
                tb
            }
            (None, None) => {
                let ta = self.synth(lhs)?;
                self.check(rhs, &ta)?;
                ta
            }
        };
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                if !t.is_numeric() {
                    return Err(Error::NotNumeric(t));
                }
                Ok(t)
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                if !t.is_numeric() {
                    return Err(Error::NotNumeric(t));
                }
                Ok(Type::Bool)
            }
            BinOp::Eq | BinOp::Ne => Ok(Type::Bool),
        }
    }

    /// The concrete type of an operand, or `None` if it is a flexible
    /// (unsuffixed) numeric literal that should adopt the other operand's type.
    fn operand_concrete(&mut self, e: &mut TypedExpr) -> Result<Option<Type>, Error> {
        let flexible = matches!(e.kind, ExprKind::Int(_, None) | ExprKind::Float(_, None));
        if flexible {
            Ok(None)
        } else {
            Ok(Some(self.synth(e)?))
        }
    }

    fn synth_call(&mut self, target: &CallTarget, args: &mut [TypedExpr]) -> Result<Type, Error> {
        match target {
            CallTarget::Lambda(boxed) => {
                let mut b = boxed.clone();
                let ft = self.synth(&mut b)?;
                if let Type::Fn(params, ret) = ft {
                    self.check_args("lambda", &params, args)?;
                    Ok(*ret)
                } else {
                    Err(Error::NotCallable(format!("{:?}", ft)))
                }
            }
            CallTarget::Path(_) => {
                // External/path calls are not resolved in v1; check args loosely.
                for a in args.iter_mut() {
                    self.synth(a)?;
                }
                Ok(Type::Unit)
            }
            CallTarget::Sym(name) => {
                // 1) ordinary function
                if let Some(sig) = self.fns.get(name).cloned() {
                    self.check_args(name, &sig.params, args)?;
                    return Ok(sig.ret);
                }
                // 2) struct constructor
                if self.structs.contains_key(name) {
                    return self.synth_struct_ctor(name, args);
                }
                // 3) static method by name (+ arg-type resolution)
                if let Some(cands) = self.static_methods.get(name).cloned() {
                    return self.resolve_static(name, &cands, args);
                }
                // 4) instance method dispatching on the first argument's type
                if !args.is_empty() {
                    // Let a flexible literal first-arg adopt a unique method receiver type.
                    let flexible = matches!(args[0].kind, ExprKind::Int(_, None) | ExprKind::Float(_, None));
                    let recv_ty = if flexible {
                        let cands: Vec<Type> = self
                            .instance_methods
                            .iter()
                            .filter(|((_, n), _)| n == name)
                            .map(|(_, (t, _))| t.clone())
                            .collect();
                        if cands.len() == 1 {
                            self.check(&mut args[0], &cands[0])?;
                            cands[0].clone()
                        } else {
                            self.synth(&mut args[0])?
                        }
                    } else {
                        self.synth(&mut args[0])?
                    };
                    let key = type_key(&recv_ty);
                    if let Some((_, sig)) = self.instance_methods.get(&(key, name.clone())).cloned() {
                        self.check_args(name, &sig.params, &mut args[1..])?;
                        return Ok(sig.ret);
                    }
                }
                Err(Error::NotCallable(name.clone()))
            }
        }
    }

    fn synth_struct_ctor(&mut self, name: &str, args: &mut [TypedExpr]) -> Result<Type, Error> {
        let sd = self.structs.get(name).cloned().unwrap();
        if args.len() != sd.fields.len() {
            return Err(Error::ArityMismatch {
                name: name.to_string(),
                expected: sd.fields.len(),
                found: args.len(),
            });
        }
        // Best-effort generic binding from field types that are bare type vars.
        let mut subst: HashMap<String, Type> = HashMap::new();
        for ((_, fty), arg) in sd.fields.iter().zip(args.iter_mut()) {
            let expected = substitute(fty, &subst);
            if let Type::Named(n, a) = &expected {
                if a.is_empty() && sd.generics.contains(n) {
                    let at = self.synth(arg)?;
                    subst.insert(n.clone(), at);
                    continue;
                }
            }
            self.check(arg, &expected)?;
        }
        let generics = sd
            .generics
            .iter()
            .map(|g| subst.get(g).cloned().unwrap_or_else(|| Type::Named(g.clone(), vec![])))
            .collect();
        Ok(Type::Named(name.to_string(), generics))
    }

    fn resolve_static(
        &mut self,
        name: &str,
        cands: &[(Type, FnSig)],
        args: &mut [TypedExpr],
    ) -> Result<Type, Error> {
        if cands.len() == 1 {
            let sig = cands[0].1.clone();
            self.check_args(name, &sig.params, args)?;
            return Ok(sig.ret);
        }
        // Multiple candidates: resolve by argument arity/types.
        let matching: Vec<&(Type, FnSig)> =
            cands.iter().filter(|(_, s)| s.params.len() == args.len()).collect();
        match matching.len() {
            0 => Err(Error::NoSuchMethod { recv: "?".to_string(), name: name.to_string() }),
            1 => {
                let sig = matching[0].1.clone();
                self.check_args(name, &sig.params, args)?;
                Ok(sig.ret)
            }
            _ => Err(Error::AmbiguousMethod(name.to_string())),
        }
    }

    fn check_args(&mut self, name: &str, params: &[Type], args: &mut [TypedExpr]) -> Result<(), Error> {
        if params.len() != args.len() {
            return Err(Error::ArityMismatch {
                name: name.to_string(),
                expected: params.len(),
                found: args.len(),
            });
        }
        for (p, a) in params.iter().zip(args.iter_mut()) {
            self.check(a, p)?;
        }
        Ok(())
    }

    /// Check `e` against an expected type, with numeric-literal defaulting and
    /// expected-type propagation into compound forms.
    fn check(&mut self, e: &mut TypedExpr, expected: &Type) -> Result<(), Error> {
        match &mut e.kind {
            ExprKind::Int(_, suffix) => {
                let t = match suffix {
                    Some(s) if s == expected => s.clone(),
                    Some(s) => return Err(Error::TypeMismatch { expected: expected.clone(), found: s.clone() }),
                    None if expected.is_integer() => expected.clone(),
                    None => {
                        let d = Type::default_int();
                        if d != *expected {
                            return Err(Error::TypeMismatch { expected: expected.clone(), found: d });
                        }
                        d
                    }
                };
                e.ty = Some(t);
                Ok(())
            }
            ExprKind::Float(_, suffix) => {
                let t = match suffix {
                    Some(s) if s == expected => s.clone(),
                    Some(s) => return Err(Error::TypeMismatch { expected: expected.clone(), found: s.clone() }),
                    None if expected.is_float() => expected.clone(),
                    None => {
                        let d = Type::default_float();
                        if d != *expected {
                            return Err(Error::TypeMismatch { expected: expected.clone(), found: d });
                        }
                        d
                    }
                };
                e.ty = Some(t);
                Ok(())
            }
            // expected-type propagation into branch/block forms
            ExprKind::If { cond, then, els } => {
                self.check(cond, &Type::Bool)?;
                match els {
                    Some(els) => {
                        self.check(then, expected)?;
                        self.check(els, expected)?;
                    }
                    None => {
                        self.check(then, &Type::Unit)?;
                        self.assignable(&Type::Unit, expected)?;
                    }
                }
                e.ty = Some(expected.clone());
                Ok(())
            }
            ExprKind::Progn { body } => {
                self.check_body(body, expected)?;
                e.ty = Some(expected.clone());
                Ok(())
            }
            ExprKind::Let { bindings, body } => {
                self.push_scope();
                let r = (|| {
                    for b in bindings.iter_mut() {
                        let t = match &b.ty {
                            Some(t) => {
                                self.check(&mut b.init, t)?;
                                t.clone()
                            }
                            None => self.synth(&mut b.init)?,
                        };
                        self.bind(&b.name, t, false);
                    }
                    self.check_body(body, expected)
                })();
                self.pop_scope();
                r?;
                e.ty = Some(expected.clone());
                Ok(())
            }
            ExprKind::Cond { clauses } => {
                for c in clauses.iter_mut() {
                    self.check(&mut c.test, &Type::Bool).ok();
                    self.check_body(&mut c.body, expected)?;
                }
                e.ty = Some(expected.clone());
                Ok(())
            }
            _ => {
                let found = self.synth(e)?;
                self.assignable(&found, expected)
            }
        }
    }

    fn assignable(&self, found: &Type, expected: &Type) -> Result<(), Error> {
        if found == expected {
            Ok(())
        } else {
            Err(Error::TypeMismatch { expected: expected.clone(), found: found.clone() })
        }
    }

    // ------------------------------------------------------------------
    // places & patterns
    // ------------------------------------------------------------------

    fn place_type(&mut self, place: &mut Place, require_mut: bool) -> Result<Type, Error> {
        match place {
            Place::Var(name) => {
                let (t, mutable) = self
                    .lookup(name)
                    .ok_or_else(|| Error::UndefinedVariable(name.clone()))?;
                if require_mut && !mutable {
                    return Err(Error::NotMutable(name.clone()));
                }
                Ok(t)
            }
            Place::Field { object, field } => {
                let ot = self.synth(object)?;
                let key = type_key(&ot);
                let sd = self
                    .structs
                    .get(&key)
                    .ok_or_else(|| Error::UnknownField { struct_name: key.clone(), field: field.clone() })?;
                sd.fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, t)| t.clone())
                    .ok_or_else(|| Error::UnknownField { struct_name: key, field: field.clone() })
            }
            Place::Index { object, index } => {
                self.synth(object)?;
                self.synth(index)?;
                // element type resolution deferred (Vec lands in M12)
                Ok(Type::Unit)
            }
        }
    }

    fn bind_pattern(&mut self, pat: &Pattern, scrutinee: &Type) {
        match pat {
            Pattern::Bind(name) => self.bind(name, scrutinee.clone(), false),
            Pattern::Tuple(ps) => {
                for p in ps {
                    self.bind_pattern(p, &Type::Unit);
                }
            }
            Pattern::Struct { fields, .. } => {
                for p in fields {
                    self.bind_pattern(p, &Type::Unit);
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // scope helpers
    // ------------------------------------------------------------------

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }
    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
    fn bind(&mut self, name: &str, ty: Type, mutable: bool) {
        self.scopes.last_mut().unwrap().insert(name.to_string(), (ty, mutable));
    }
    fn lookup(&self, name: &str) -> Option<(Type, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }
}

/// A key identifying a receiver type for method dispatch.
fn type_key(t: &Type) -> String {
    match t {
        Type::Named(n, _) => n.clone(),
        Type::Path(segs) => segs.join("::"),
        other => format!("{:?}", other),
    }
}

/// Substitute generic type variables in `ty` using `subst`.
fn substitute(ty: &Type, subst: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Named(n, args) if args.is_empty() => {
            subst.get(n).cloned().unwrap_or_else(|| ty.clone())
        }
        Type::Named(n, args) => {
            Type::Named(n.clone(), args.iter().map(|a| substitute(a, subst)).collect())
        }
        Type::Fn(ps, r) => Type::Fn(
            ps.iter().map(|p| substitute(p, subst)).collect(),
            Box::new(substitute(r, subst)),
        ),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| substitute(t, subst)).collect()),
        other => other.clone(),
    }
}
