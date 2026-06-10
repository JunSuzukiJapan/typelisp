//! Builds the typed AST (`Form` / `TypedExpr`) from reader `Object`s.
//!
//! Dispatch is by the head symbol of each list. Type-mandatory positions
//! (`defun`/`lambda`/`defmethod` params, struct fields) and type-optional
//! positions (`let`/`defvar` bindings) are parsed by *different* helpers, so
//! the two never cross (grammar §5).

use crate::{
    parse_type, BinOp, CondClause, DefMethod, DefStruct, DefVar, Defun, Error, ExprKind, Form,
    LetBinding, MatchArm, NumSuffix, Object, Pattern, Place, Receiver, Type, TypedExpr, UnOp,
    CallTarget,
};

pub struct ASTConstructor;

impl ASTConstructor {
    pub fn new() -> ASTConstructor {
        ASTConstructor {}
    }

    // ------------------------------------------------------------------
    // Top-level forms
    // ------------------------------------------------------------------

    pub fn make_form(&self, obj: &Object) -> Result<Form, Error> {
        let elems = match obj {
            Object::List(_) => list_elems(obj),
            _ => return Ok(Form::Expr(self.make_expr(obj)?)),
        };
        if elems.is_empty() {
            return Ok(Form::Expr(self.make_expr(obj)?));
        }

        // Optional leading `pub`.
        let (pub_, head_idx) = match elems[0] {
            Object::Symbol(s) if s == "pub" => (true, 1),
            _ => (false, 0),
        };
        let head = match elems.get(head_idx) {
            Some(Object::Symbol(s)) => s.as_str(),
            _ => return Ok(Form::Expr(self.make_expr(obj)?)),
        };
        let rest = &elems[head_idx + 1..];

        match head {
            "defun" => Ok(Form::Defun(self.build_defun(pub_, rest)?)),
            "defstruct" => Ok(Form::DefStruct(self.build_defstruct(pub_, rest)?)),
            "defmethod" => Ok(Form::DefMethod(self.build_defmethod(pub_, rest)?)),
            "defvar" => Ok(Form::DefVar(self.build_defvar(rest, true)?)),
            "defconstant" => Ok(Form::DefConstant(self.build_defvar(rest, false)?)),
            "use" => {
                let path = as_symbol(rest.first().ok_or_else(|| malformed("use"))?)?;
                Ok(Form::Use(path.split("::").map(|s| s.to_string()).collect()))
            }
            "module" => {
                let name = as_symbol(rest.first().ok_or_else(|| malformed("module"))?)?.to_string();
                let mut items = Vec::new();
                for item in &rest[1..] {
                    items.push(self.make_form(item)?);
                }
                Ok(Form::Module { pub_, name, items })
            }
            _ => Ok(Form::Expr(self.make_expr(obj)?)),
        }
    }

    /// Parse a whole program (sequence of top-level forms).
    pub fn make_program(&self, objs: &[Object]) -> Result<Vec<Form>, Error> {
        objs.iter().map(|o| self.make_form(o)).collect()
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    pub fn make_expr(&self, obj: &Object) -> Result<TypedExpr, Error> {
        let kind = match obj {
            Object::Null => ExprKind::Unit,
            Object::True => ExprKind::Bool(true),
            Object::False => ExprKind::Bool(false),
            Object::Int(x) => ExprKind::Int(*x, None),
            Object::IntWithSuffix(x, ns) => ExprKind::Int(*x, Some(suffix_type(*ns))),
            Object::Float(x) => ExprKind::Float(*x, None),
            Object::FloatWithSuffix(x, ns) => ExprKind::Float(*x, Some(suffix_type(*ns))),
            Object::Char(c) => ExprKind::Char(*c),
            Object::String(s) => ExprKind::Str(s.clone()),
            Object::Symbol(s) => ExprKind::Var(s.clone()),
            Object::List(_) => return self.make_expr_list(obj),
        };
        Ok(TypedExpr::new(kind))
    }

    fn make_expr_list(&self, obj: &Object) -> Result<TypedExpr, Error> {
        let elems = list_elems(obj);
        if elems.is_empty() {
            return Ok(TypedExpr::new(ExprKind::Unit));
        }

        // `((lambda ...) args...)` — immediately-applied / computed callee.
        let head = match elems[0] {
            Object::Symbol(s) => s.as_str(),
            Object::List(_) => {
                let target = CallTarget::Lambda(Box::new(self.make_expr(elems[0])?));
                let args = self.make_exprs(&elems[1..])?;
                return Ok(TypedExpr::new(ExprKind::Call { target, args }));
            }
            other => {
                return Err(Error::MalformedSpecialForm(format!("cannot call {:?}", other)))
            }
        };
        let rest = &elems[1..];

        let kind = match head {
            "let" => self.build_let(rest)?,
            "if" => self.build_if(rest)?,
            "when" => ExprKind::When {
                cond: Box::new(self.make_expr(first(rest, "when")?)?),
                body: self.make_exprs(&rest[1..])?,
            },
            "unless" => ExprKind::Unless {
                cond: Box::new(self.make_expr(first(rest, "unless")?)?),
                body: self.make_exprs(&rest[1..])?,
            },
            "cond" => self.build_cond(rest)?,
            "match" => self.build_match(rest)?,
            "loop" => ExprKind::Loop { body: self.make_exprs(rest)? },
            "while" => ExprKind::While {
                cond: Box::new(self.make_expr(first(rest, "while")?)?),
                body: self.make_exprs(&rest[1..])?,
            },
            "dotimes" => self.build_dotimes(rest)?,
            "progn" => ExprKind::Progn { body: self.make_exprs(rest)? },
            "break" => ExprKind::Break,
            "setf" => ExprKind::Setf {
                place: self.parse_place(first(rest, "setf")?)?,
                value: Box::new(self.make_expr(rest.get(1).ok_or_else(|| malformed("setf"))?)?),
            },
            "incf" => ExprKind::Incf(self.parse_place(first(rest, "incf")?)?),
            "decf" => ExprKind::Decf(self.parse_place(first(rest, "decf")?)?),
            "quote" => ExprKind::Quote(first(rest, "quote")?.clone()),
            "lambda" => self.build_lambda(rest)?,
            "defvar" => ExprKind::DefVarLocal(Box::new(self.build_defvar(rest, true)?)),
            "defconstant" => ExprKind::DefVarLocal(Box::new(self.build_defvar(rest, false)?)),
            // arithmetic (left-folded, n-ary)
            "+" => self.build_arith(BinOp::Add, rest)?,
            "-" => self.build_arith(BinOp::Sub, rest)?,
            "*" => self.build_arith(BinOp::Mul, rest)?,
            "/" => self.build_arith(BinOp::Div, rest)?,
            "%" => self.build_arith(BinOp::Rem, rest)?,
            // comparison (binary)
            "==" => self.build_compare(BinOp::Eq, rest)?,
            "!=" => self.build_compare(BinOp::Ne, rest)?,
            "<" => self.build_compare(BinOp::Lt, rest)?,
            ">" => self.build_compare(BinOp::Gt, rest)?,
            "<=" => self.build_compare(BinOp::Le, rest)?,
            ">=" => self.build_compare(BinOp::Ge, rest)?,
            // unary
            "1+" => ExprKind::UnOp { op: UnOp::Inc1, operand: Box::new(self.make_expr(first(rest, "1+")?)?) },
            "1-" => ExprKind::UnOp { op: UnOp::Dec1, operand: Box::new(self.make_expr(first(rest, "1-")?)?) },
            "!" => ExprKind::UnOp { op: UnOp::Not, operand: Box::new(self.make_expr(first(rest, "!")?)?) },
            // field access: (. obj field)
            "." => ExprKind::FieldAccess {
                object: Box::new(self.make_expr(first(rest, ".")?)?),
                field: as_symbol(rest.get(1).ok_or_else(|| malformed("."))?)?.to_string(),
            },
            // method call sugar: (.method obj args...)
            _ if head.starts_with('.') && head.len() > 1 => ExprKind::MethodCall {
                method: head[1..].to_string(),
                receiver: Box::new(self.make_expr(first(rest, head)?)?),
                args: self.make_exprs(&rest[1..])?,
            },
            // plain call (symbol or path target)
            _ => {
                let target = if head.contains("::") {
                    CallTarget::Path(head.split("::").map(|s| s.to_string()).collect())
                } else {
                    CallTarget::Sym(head.to_string())
                };
                ExprKind::Call { target, args: self.make_exprs(rest)? }
            }
        };
        Ok(TypedExpr::new(kind))
    }

    fn make_exprs(&self, objs: &[&Object]) -> Result<Vec<TypedExpr>, Error> {
        objs.iter().map(|o| self.make_expr(o)).collect()
    }

    // ------------------------------------------------------------------
    // Builders
    // ------------------------------------------------------------------

    fn build_arith(&self, op: BinOp, args: &[&Object]) -> Result<ExprKind, Error> {
        if args.is_empty() {
            return Err(Error::MalformedSpecialForm("arithmetic needs at least one argument".into()));
        }
        let mut acc = self.make_expr(args[0])?;
        for a in &args[1..] {
            acc = TypedExpr::new(ExprKind::BinOp {
                op,
                lhs: Box::new(acc),
                rhs: Box::new(self.make_expr(a)?),
            });
        }
        // unwrap the outermost node's kind
        Ok(acc.kind)
    }

    fn build_compare(&self, op: BinOp, args: &[&Object]) -> Result<ExprKind, Error> {
        if args.len() != 2 {
            return Err(Error::MalformedSpecialForm("comparison takes exactly two arguments".into()));
        }
        Ok(ExprKind::BinOp {
            op,
            lhs: Box::new(self.make_expr(args[0])?),
            rhs: Box::new(self.make_expr(args[1])?),
        })
    }

    fn build_if(&self, rest: &[&Object]) -> Result<ExprKind, Error> {
        if rest.len() < 2 {
            return Err(malformed("if"));
        }
        let cond = Box::new(self.make_expr(rest[0])?);
        let then = Box::new(self.make_expr(rest[1])?);
        let els = match rest.get(2) {
            Some(e) => Some(Box::new(self.make_expr(e)?)),
            None => None,
        };
        Ok(ExprKind::If { cond, then, els })
    }

    fn build_let(&self, rest: &[&Object]) -> Result<ExprKind, Error> {
        let bindings_obj = first(rest, "let")?;
        let mut bindings = Vec::new();
        for b in list_elems(bindings_obj) {
            bindings.push(self.parse_let_binding(b)?);
        }
        Ok(ExprKind::Let { bindings, body: self.make_exprs(&rest[1..])? })
    }

    fn parse_let_binding(&self, obj: &Object) -> Result<LetBinding, Error> {
        let elems = list_elems(obj);
        if elems.len() != 2 {
            return Err(Error::InvalidLetBinding(format!("{:?}", obj)));
        }
        match elems[0] {
            // (name init) — inferred
            Object::Symbol(name) => Ok(LetBinding {
                name: name.clone(),
                ty: None,
                init: self.make_expr(elems[1])?,
            }),
            // ((name type) init) — explicit
            Object::List(_) => {
                let (name, ty) = self.parse_typed_param(elems[0])?;
                Ok(LetBinding { name, ty: Some(ty), init: self.make_expr(elems[1])? })
            }
            _ => Err(Error::InvalidLetBinding(format!("{:?}", obj))),
        }
    }

    fn build_cond(&self, rest: &[&Object]) -> Result<ExprKind, Error> {
        let mut clauses = Vec::new();
        for c in rest {
            let elems = list_elems(c);
            if elems.is_empty() {
                return Err(malformed("cond clause"));
            }
            clauses.push(CondClause {
                test: self.make_expr(elems[0])?,
                body: self.make_exprs(&elems[1..])?,
            });
        }
        Ok(ExprKind::Cond { clauses })
    }

    fn build_dotimes(&self, rest: &[&Object]) -> Result<ExprKind, Error> {
        let spec = list_elems(first(rest, "dotimes")?);
        if spec.len() != 2 {
            return Err(malformed("dotimes (var count)"));
        }
        let var = as_symbol(spec[0])?.to_string();
        let count = Box::new(self.make_expr(spec[1])?);
        Ok(ExprKind::Dotimes { var, count, body: self.make_exprs(&rest[1..])? })
    }

    fn build_lambda(&self, rest: &[&Object]) -> Result<ExprKind, Error> {
        let (move_, idx) = match rest.first() {
            Some(Object::Symbol(s)) if s == "move" => (true, 1),
            _ => (false, 0),
        };
        let params_obj = rest.get(idx).ok_or_else(|| malformed("lambda"))?;
        let params = self.parse_param_list(params_obj)?;
        Ok(ExprKind::Lambda { move_, params, body: self.make_exprs(&rest[idx + 1..])? })
    }

    fn build_defun(&self, pub_: bool, rest: &[&Object]) -> Result<Defun, Error> {
        if rest.len() < 2 {
            return Err(malformed("defun"));
        }
        let name = as_symbol(rest[0])?.to_string();
        let params = self.parse_param_list(rest[1])?;
        let ret = parse_type(rest.get(2).ok_or_else(|| malformed("defun return type"))?)?;
        let body = self.make_exprs(&rest[3..])?;
        Ok(Defun { pub_, name, params, ret, body })
    }

    fn build_defmethod(&self, pub_: bool, rest: &[&Object]) -> Result<DefMethod, Error> {
        if rest.len() < 3 {
            return Err(malformed("defmethod"));
        }
        let name = as_symbol(rest[0])?.to_string();
        let param_elems = list_elems(rest[1]);
        let recv_obj = param_elems.first().ok_or_else(|| Error::InvalidReceiver("missing receiver".into()))?;
        let recv = self.parse_receiver(recv_obj)?;
        let mut params = Vec::new();
        for p in &param_elems[1..] {
            params.push(self.parse_typed_param(p)?);
        }
        let ret = parse_type(rest.get(2).ok_or_else(|| malformed("defmethod return type"))?)?;
        let body = self.make_exprs(&rest[3..])?;
        Ok(DefMethod { pub_, name, recv, params, ret, body })
    }

    fn parse_receiver(&self, obj: &Object) -> Result<Receiver, Error> {
        let inner = list_elems(obj);
        match inner.len() {
            1 => {
                if let Object::Symbol(s) = inner[0] {
                    if s == "self" {
                        return Err(Error::InvalidReceiver("`self` alone is not a type".into()));
                    }
                }
                Ok(Receiver::Static(parse_type(inner[0])?))
            }
            2 => match inner[0] {
                Object::Symbol(s) if s == "self" => Ok(Receiver::Instance(parse_type(inner[1])?)),
                _ => Err(Error::InvalidReceiver("instance receiver name must be `self`".into())),
            },
            _ => Err(Error::InvalidReceiver(format!("{:?}", obj))),
        }
    }

    fn build_defstruct(&self, pub_: bool, rest: &[&Object]) -> Result<DefStruct, Error> {
        let name_sym = as_symbol(rest.first().ok_or_else(|| malformed("defstruct"))?)?;
        let (name, generics) = split_generics(name_sym);

        let mut pub_fields = Vec::new();
        let mut priv_fields = Vec::new();
        for group in &rest[1..] {
            let g = list_elems(group);
            match g.first() {
                // (pub (fields...))
                Some(Object::Symbol(s)) if s == "pub" => {
                    for fobj in list_elems(g.get(1).ok_or_else(|| malformed("defstruct pub group"))?) {
                        pub_fields.push(self.parse_typed_param(fobj)?);
                    }
                }
                // ((fields...))
                Some(Object::List(_)) => {
                    for fobj in list_elems(g[0]) {
                        priv_fields.push(self.parse_typed_param(fobj)?);
                    }
                }
                _ => return Err(malformed("defstruct field group")),
            }
        }
        Ok(DefStruct { pub_, name, generics, pub_fields, priv_fields })
    }

    fn build_defvar(&self, rest: &[&Object], mutable: bool) -> Result<DefVar, Error> {
        if rest.len() != 2 {
            return Err(malformed("defvar/defconstant (target init)"));
        }
        let (name, ty) = match rest[0] {
            Object::Symbol(s) => (s.clone(), None),
            Object::List(_) => {
                let (n, t) = self.parse_typed_param(rest[0])?;
                (n, Some(t))
            }
            _ => return Err(malformed("defvar target")),
        };
        Ok(DefVar { name, ty, init: Box::new(self.make_expr(rest[1])?), mutable })
    }

    /// `(name type)` — type is mandatory.
    fn parse_typed_param(&self, obj: &Object) -> Result<(String, Type), Error> {
        let elems = list_elems(obj);
        if elems.len() != 2 {
            return Err(Error::ExpectedTypedParam(format!("{:?}", obj)));
        }
        let name = as_symbol(elems[0])?.to_string();
        let ty = parse_type(elems[1])?;
        Ok((name, ty))
    }

    fn parse_param_list(&self, obj: &Object) -> Result<Vec<(String, Type)>, Error> {
        match obj {
            Object::Null => Ok(Vec::new()),
            Object::List(_) => list_elems(obj).iter().map(|p| self.parse_typed_param(p)).collect(),
            _ => Err(Error::ExpectedTypedParam(format!("{:?}", obj))),
        }
    }

    fn parse_place(&self, obj: &Object) -> Result<Place, Error> {
        match obj {
            Object::Symbol(s) => Ok(Place::Var(s.clone())),
            Object::List(_) => {
                let elems = list_elems(obj);
                match elems.first() {
                    Some(Object::Symbol(s)) if s == "." => Ok(Place::Field {
                        object: Box::new(self.make_expr(elems.get(1).ok_or_else(|| malformed("place ."))?)?),
                        field: as_symbol(elems.get(2).ok_or_else(|| malformed("place ."))?)?.to_string(),
                    }),
                    Some(Object::Symbol(s)) if s == "aref" => Ok(Place::Index {
                        object: Box::new(self.make_expr(elems.get(1).ok_or_else(|| malformed("aref"))?)?),
                        index: Box::new(self.make_expr(elems.get(2).ok_or_else(|| malformed("aref"))?)?),
                    }),
                    _ => Err(malformed("place")),
                }
            }
            _ => Err(malformed("place")),
        }
    }

    // ------------------------------------------------------------------
    // match
    // ------------------------------------------------------------------

    fn build_match(&self, rest: &[&Object]) -> Result<ExprKind, Error> {
        let scrutinee = Box::new(self.make_expr(first(rest, "match")?)?);
        let mut arms = Vec::new();
        for arm in &rest[1..] {
            arms.push(self.parse_match_arm(arm)?);
        }
        Ok(ExprKind::Match { scrutinee, arms })
    }

    fn parse_match_arm(&self, obj: &Object) -> Result<MatchArm, Error> {
        let elems = list_elems(obj);
        // split at `=>`
        let arrow = elems.iter().position(|e| matches!(e, Object::Symbol(s) if s == "=>"));
        let arrow = arrow.ok_or_else(|| Error::InvalidPattern("match arm missing `=>`".into()))?;
        let (pat_part, body_part) = elems.split_at(arrow);
        let body_part = &body_part[1..]; // skip `=>`

        // patterns separated by `|`
        let mut patterns = Vec::new();
        for p in pat_part {
            if matches!(p, Object::Symbol(s) if s == "|") {
                continue;
            }
            patterns.push(self.parse_pattern(p)?);
        }
        if patterns.is_empty() {
            return Err(Error::InvalidPattern("match arm has no patterns".into()));
        }

        // body is a parenthesised group `( ... )`
        let body = match body_part.first() {
            Some(b) => self.make_exprs(&list_elems(b))?,
            None => return Err(Error::InvalidPattern("match arm missing body".into())),
        };
        Ok(MatchArm { patterns, body })
    }

    fn parse_pattern(&self, obj: &Object) -> Result<Pattern, Error> {
        match obj {
            Object::Int(x) => Ok(Pattern::Int(*x)),
            Object::IntWithSuffix(x, _) => Ok(Pattern::Int(*x)),
            Object::Float(x) => Ok(Pattern::Float(*x)),
            Object::FloatWithSuffix(x, _) => Ok(Pattern::Float(*x)),
            Object::Char(c) => Ok(Pattern::Char(*c)),
            Object::String(s) => Ok(Pattern::Str(s.clone())),
            Object::True => Ok(Pattern::Bool(true)),
            Object::False => Ok(Pattern::Bool(false)),
            Object::Symbol(s) if s == "_" => Ok(Pattern::Wildcard),
            Object::Symbol(s) if s.contains("..") => parse_range_pattern(s),
            Object::Symbol(s) => Ok(Pattern::Bind(s.clone())),
            Object::List(_) => {
                let elems = list_elems(obj);
                match elems.first() {
                    Some(Object::Symbol(s)) if s == "tuple" => {
                        let mut pats = Vec::new();
                        for p in &elems[1..] {
                            pats.push(self.parse_pattern(p)?);
                        }
                        Ok(Pattern::Tuple(pats))
                    }
                    // Name(pat...) struct pattern
                    Some(Object::Symbol(s)) => {
                        let name = s.split("::").map(|x| x.to_string()).collect();
                        let mut fields = Vec::new();
                        for p in &elems[1..] {
                            fields.push(self.parse_pattern(p)?);
                        }
                        Ok(Pattern::Struct { name, fields })
                    }
                    _ => Err(Error::InvalidPattern(format!("{:?}", obj))),
                }
            }
            Object::Null => Err(Error::InvalidPattern("null pattern".into())),
        }
    }
}

// ----------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------

fn malformed(what: &str) -> Error {
    Error::MalformedSpecialForm(what.to_string())
}

fn first<'a>(rest: &'a [&'a Object], what: &str) -> Result<&'a Object, Error> {
    rest.first().copied().ok_or_else(|| malformed(what))
}

fn as_symbol(obj: &Object) -> Result<&str, Error> {
    match obj {
        Object::Symbol(s) => Ok(s),
        _ => Err(Error::ExpectedSymbol(format!("{:?}", obj))),
    }
}

/// Collect a proper list's elements into a vector of references.
fn list_elems(obj: &Object) -> Vec<&Object> {
    match obj {
        Object::List(cons) => cons.iter().collect(),
        _ => Vec::new(),
    }
}

/// Split a struct-name token `Pair<K,V>` into `("Pair", ["K","V"])`.
fn split_generics(name: &str) -> (String, Vec<String>) {
    if let Some(open) = name.find('<') {
        if name.ends_with('>') {
            let head = name[..open].to_string();
            let inner = &name[open + 1..name.len() - 1];
            let generics = inner
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            return (head, generics);
        }
    }
    (name.to_string(), Vec::new())
}

/// Parse a range pattern from a single token like `0..5` or `0..=5`.
fn parse_range_pattern(s: &str) -> Result<Pattern, Error> {
    let (lo_str, hi_str, inclusive) = if let Some(idx) = s.find("..=") {
        (&s[..idx], &s[idx + 3..], true)
    } else if let Some(idx) = s.find("..") {
        (&s[..idx], &s[idx + 2..], false)
    } else {
        return Err(Error::InvalidPattern(s.to_string()));
    };
    let lo = lo_str.parse::<i64>().map_err(|_| Error::InvalidPattern(s.to_string()))?;
    let hi = hi_str.parse::<i64>().map_err(|_| Error::InvalidPattern(s.to_string()))?;
    Ok(Pattern::Range { lo, hi, inclusive })
}

fn suffix_type(ns: NumSuffix) -> Type {
    match ns {
        NumSuffix::I8 => Type::I8,
        NumSuffix::I16 => Type::I16,
        NumSuffix::I32 => Type::I32,
        NumSuffix::I64 => Type::I64,
        NumSuffix::Isize => Type::Isize,
        NumSuffix::U8 => Type::U8,
        NumSuffix::U16 => Type::U16,
        NumSuffix::U32 => Type::U32,
        NumSuffix::U64 => Type::U64,
        NumSuffix::Usize => Type::Usize,
        NumSuffix::F32 => Type::F32,
        NumSuffix::F64 => Type::F64,
    }
}
