//! The type checker: lowers read `Sexpr` values into a typed AST while checking.
//!
//! Checking is bidirectional in a small way: literal and constructor nodes may
//! be checked *against* an expected type (so integer literals adopt the expected
//! integer type and a nullary `None` learns its type argument), while everything
//! else synthesizes its own type and is reconciled against the expectation.

use std::collections::{HashMap, HashSet};

use crate::{parse_type, Error, Heap, Path, Type, Value};

use super::ast::{Arm, Expr, Pattern, Typed};
use super::registry::{AdtDef, AssocFn, FnSig, Namespace, Registry, Variant};

/// A checked top-level form.
#[derive(Clone, Debug, PartialEq)]
pub enum TopLevel {
    /// A `defun`: fully-qualified [`Path`], typed parameters, return type, body.
    Defun {
        name: Path,
        params: Vec<(String, Type)>,
        ret: Type,
        body: Vec<Typed>,
    },
    /// A `defmethod`: instance or static associated function of a type.
    Defmethod {
        type_name: Path,
        method: String,
        instance: bool,
        /// The receiver's variable name (e.g. `self`) for instance methods.
        self_name: Option<String>,
        params: Vec<(String, Type)>,
        ret: Type,
        body: Vec<Typed>,
    },
    /// A `defstruct`: a user data type.
    Defstruct { name: Path, params: Vec<String>, variants: Vec<Variant> },
    /// A `module`: a namespace and its checked body forms.
    Module { path: Path, body: Vec<TopLevel> },
    /// A `use`: a name brought into the current scope (alias -> target path).
    Use { alias: Path, target: Path },
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

/// The type checker, holding the data-type and function registries plus the
/// current namespace (module) path.
pub struct Checker {
    reg: Registry,
    ns: Vec<String>,
}

impl Checker {
    pub fn new() -> Checker {
        Checker { reg: Registry::with_builtins(), ns: Vec::new() }
    }

    /// Check one top-level form. Definition forms (`defun`/`defstruct`/`module`/
    /// `defmethod`/`use`) register into the current namespace; anything else is
    /// checked as an expression.
    pub fn check_form(&mut self, heap: &Heap, v: Value) -> Result<TopLevel, Error> {
        if let Value::Cons(_) = v {
            let elems = heap.list_to_vec(v)?;
            if let Some(Value::Symbol(id)) = elems.first() {
                match heap.symbol_name(*id) {
                    "defun" => return self.check_defun(heap, &elems[1..]),
                    "defstruct" => return self.check_defstruct(heap, &elems[1..]),
                    "module" => return self.check_module(heap, &elems[1..]),
                    "defmethod" => return self.check_defmethod(heap, &elems[1..]),
                    "use" => return self.check_use(heap, &elems[1..]),
                    _ => {}
                }
            }
        }
        let env = Env::new();
        let t = self.check(heap, &env, v, None)?;
        Ok(TopLevel::Expr(t))
    }

    // ---- namespace navigation & resolution --------------------------------
    //
    // Identities are structured `Path`s, never joined `"a::b"` strings: the only
    // `::`-string decoding lives in the reader/parser. Resolution navigates the
    // namespace tree, current namespace first then root.

    /// Qualify a bare name with the current namespace path.
    fn fq(&self, name: &str) -> Path {
        let mut segments = self.ns.clone();
        segments.push(name.to_string());
        Path::from_segments(segments)
    }

    /// The current namespace.
    fn cur_ns(&self) -> &Namespace {
        self.reg.root.module(&self.ns).expect("current namespace exists")
    }

    /// Locate a child-module `path`, resolving it relative to the current
    /// namespace first, then the root. Returns its absolute segments and module.
    fn find_module(&self, path: &[String]) -> Option<(Vec<String>, &Namespace)> {
        if let Some(m) = self.cur_ns().module(path) {
            let mut abs = self.ns.clone();
            abs.extend_from_slice(path);
            return Some((abs, m));
        }
        self.reg.root.module(path).map(|m| (path.to_vec(), m))
    }

    /// A `use` alias for a bare `name`, looked up current namespace then root.
    fn lookup_alias(&self, name: &str) -> Option<Vec<String>> {
        if let Some(p) = self.cur_ns().aliases.get(name) {
            return Some(p.clone());
        }
        self.reg.root.aliases.get(name).cloned()
    }

    /// Resolve a bare free-function name to its absolute [`Path`].
    fn resolve_fn(&self, name: &str) -> Option<Path> {
        if let Some(path) = self.lookup_alias(name) {
            return self.resolve_fn_path(&path);
        }
        if self.cur_ns().fns.contains_key(name) {
            return Some(self.fq(name));
        }
        if self.reg.root.fns.contains_key(name) {
            return Some(Path::root(name));
        }
        None
    }

    /// Resolve a qualified `module::...::fn` path to its absolute [`Path`].
    fn resolve_fn_path(&self, segs: &[String]) -> Option<Path> {
        let (mods, last) = segs.split_at(segs.len() - 1);
        let (abs, m) = self.find_module(mods)?;
        if m.fns.contains_key(&last[0]) {
            let mut full = abs;
            full.push(last[0].clone());
            return Some(Path::from_segments(full));
        }
        None
    }

    /// Resolve a bare constructor name to its `(type path, variant index)`,
    /// searching the current namespace then root.
    fn resolve_ctor(&self, name: &str) -> Option<(Path, usize)> {
        if let Some(hit) = self.cur_ns().ctors.get(name) {
            return Some(hit.clone());
        }
        self.reg.root.ctors.get(name).cloned()
    }

    /// Resolve a `module::...::Type` segment path to the type's [`Path`].
    fn resolve_type_path(&self, segs: &[String]) -> Option<Path> {
        let (mods, local) = segs.split_at(segs.len() - 1);
        let (_abs, m) = self.find_module(mods)?;
        m.types.get(&local[0]).map(|d| d.name.clone())
    }

    /// Resolve a nominal type [`Path`] to its canonical (located) form if a
    /// matching type exists; otherwise (type variables, unknown names) return it
    /// unchanged.
    fn resolve_type_name(&self, path: &Path) -> Path {
        if path.is_simple() {
            let name = path.local();
            if let Some(target) = self.lookup_alias(name) {
                if let Some(tp) = self.resolve_type_path(&target) {
                    return tp;
                }
            }
            if let Some(def) = self.cur_ns().types.get(name) {
                return def.name.clone();
            }
            if let Some(def) = self.reg.root.types.get(name) {
                return def.name.clone();
            }
            return path.clone();
        }
        self.resolve_type_path(path.segments()).unwrap_or_else(|| path.clone())
    }

    /// Canonicalize a parsed type: resolve every nominal name to its located
    /// [`Path`] so that types compare equal across module boundaries.
    fn canon(&self, t: &Type) -> Type {
        match t {
            Type::Named(n, args) => Type::Named(
                self.resolve_type_name(n),
                args.iter().map(|a| self.canon(a)).collect(),
            ),
            Type::Fn(ps, r) => Type::Fn(
                ps.iter().map(|p| self.canon(p)).collect(),
                Box::new(self.canon(r)),
            ),
            other => other.clone(),
        }
    }

    /// The lowercase segments of a module/use path (a symbol or a `Value::Path`).
    fn path_to_segs(&self, heap: &Heap, v: Value) -> Result<Vec<String>, Error> {
        match v {
            Value::Symbol(id) => Ok(vec![heap.symbol_name(id).to_string()]),
            Value::Path(id) => Ok(heap
                .path_segments(id)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect()),
            _ => Err(Error::TypeError("expected a name or `::` path".into())),
        }
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
        let ret = self.canon(&parse_type(heap, parts[2])?);
        let fq_name = self.fq(&name);

        // Register the signature in the current namespace before checking the
        // body so self-recursion works.
        let sig = FnSig { params: params.iter().map(|(_, t)| t.clone()).collect(), ret: ret.clone() };
        self.reg.root.module_mut(&self.ns).fns.insert(name.clone(), sig);

        let env = Env::new().extended(params.clone());
        let (body, _) = self.check_seq(heap, &env, &parts[3..], Some(&ret))?;
        Ok(TopLevel::Defun { name: fq_name, params, ret, body })
    }

    /// Parse a `((name type)...)` parameter list (types canonicalized to FQ).
    fn parse_params(&self, heap: &Heap, v: Value) -> Result<Vec<(String, Type)>, Error> {
        let elems = heap.list_to_vec(v)?;
        self.parse_param_pairs(heap, &elems)
    }

    /// Parse a slice of `(name type)` binding forms.
    fn parse_param_pairs(&self, heap: &Heap, pairs: &[Value]) -> Result<Vec<(String, Type)>, Error> {
        let mut out = Vec::new();
        for binding in pairs {
            let pair = heap.list_to_vec(*binding)?;
            if pair.len() != 2 {
                return Err(Error::TypeError("parameter must be (name type)".into()));
            }
            let name = match pair[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("parameter name must be a symbol".into())),
            };
            out.push((name, self.canon(&parse_type(heap, pair[1])?)));
        }
        Ok(out)
    }

    // ---- defstruct / module / defmethod / use -----------------------------

    fn check_defstruct(&mut self, heap: &Heap, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("defstruct: (defstruct Name (Ctor (field Type)...)...)".into()));
        }
        let name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("defstruct: name must be a symbol".into())),
        };
        let fq = self.fq(&name);
        let mut variants = Vec::new();
        for vform in &parts[1..] {
            let velems = heap.list_to_vec(*vform)?;
            if velems.is_empty() {
                return Err(Error::TypeError("defstruct: variant must be (Ctor (field Type)...)".into()));
            }
            let cname = match velems[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("defstruct: constructor must be a symbol".into())),
            };
            let mut fields = Vec::new();
            for fform in &velems[1..] {
                let fpair = heap.list_to_vec(*fform)?;
                if fpair.len() != 2 {
                    return Err(Error::TypeError("defstruct: field must be (name type)".into()));
                }
                fields.push(self.canon(&parse_type(heap, fpair[1])?));
            }
            variants.push(Variant { name: cname, fields });
        }
        let def = AdtDef {
            name: fq.clone(),
            params: Vec::new(),
            variants: variants.clone(),
            assoc: HashMap::new(),
        };
        self.reg.root.module_mut(&self.ns).add_type(def);
        Ok(TopLevel::Defstruct { name: fq, params: Vec::new(), variants })
    }

    fn check_module(&mut self, heap: &Heap, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("module: (module path body...)".into()));
        }
        let segs = self.path_to_segs(heap, parts[0])?;
        let depth = segs.len();
        for s in &segs {
            self.ns.push(s.clone());
        }
        let path = Path::from_segments(self.ns.clone());
        // Ensure the (possibly empty) module namespace exists.
        self.reg.root.module_mut(&self.ns);

        let mut body = Vec::new();
        let mut result = Ok(());
        for form in &parts[1..] {
            match self.check_form(heap, *form) {
                Ok(tl) => body.push(tl),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        for _ in 0..depth {
            self.ns.pop();
        }
        result?;
        Ok(TopLevel::Module { path, body })
    }

    fn check_defmethod(&mut self, heap: &Heap, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.len() < 3 {
            return Err(Error::TypeError(
                "defmethod: (defmethod name (receiver params...) ret body...)".into(),
            ));
        }
        let method = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("defmethod: name must be a symbol".into())),
        };
        let sig_list = heap.list_to_vec(parts[1])?;
        if sig_list.is_empty() {
            return Err(Error::TypeError("defmethod: needs a receiver".into()));
        }
        // `(self T)` -> instance method; a bare type name -> static method.
        let (instance, self_name, type_expr) = match sig_list[0] {
            Value::Cons(_) => {
                let recv = heap.list_to_vec(sig_list[0])?;
                if recv.len() != 2 {
                    return Err(Error::TypeError("defmethod: receiver must be (self Type)".into()));
                }
                let sname = match recv[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("defmethod: receiver name must be a symbol".into())),
                };
                (true, Some(sname), recv[1])
            }
            Value::Symbol(_) | Value::Path(_) => (false, None, sig_list[0]),
            _ => return Err(Error::TypeError("defmethod: receiver must be (self Type) or a type name".into())),
        };
        let recv_ty = self.canon(&parse_type(heap, type_expr)?);
        let type_fq = match &recv_ty {
            Type::Named(n, _) => n.clone(),
            _ => return Err(Error::TypeError("defmethod: receiver must be a data type".into())),
        };
        if self.reg.type_def(&type_fq).is_none() {
            return Err(Error::TypeError(format!("defmethod: unknown type `{}`", type_fq)));
        }
        let params = self.parse_param_pairs(heap, &sig_list[1..])?;
        let ret = self.canon(&parse_type(heap, parts[2])?);

        // Register the signature before checking the body (self-recursion).
        let mut sig_params: Vec<Type> = Vec::new();
        if instance {
            sig_params.push(recv_ty.clone());
        }
        sig_params.extend(params.iter().map(|(_, t)| t.clone()));
        let sig = FnSig { params: sig_params, ret: ret.clone() };
        if let Some(def) = self.reg.type_def_mut(&type_fq) {
            def.assoc.insert(method.clone(), AssocFn { sig, instance });
        }

        let mut binds: Vec<(String, Type)> = Vec::new();
        if let Some(s) = &self_name {
            binds.push((s.clone(), recv_ty.clone()));
        }
        binds.extend(params.clone());
        let env = Env::new().extended(binds);
        let (body, _) = self.check_seq(heap, &env, &parts[3..], Some(&ret))?;
        Ok(TopLevel::Defmethod { type_name: type_fq, method, instance, self_name, params, ret, body })
    }

    fn check_use(&mut self, heap: &Heap, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.len() != 1 {
            return Err(Error::TypeError("use: (use path)".into()));
        }
        let segs = self.path_to_segs(heap, parts[0])?;
        let bare = segs.last().cloned().unwrap();
        // The target must resolve to a free function or a type.
        let target = self
            .resolve_fn_path(&segs)
            .or_else(|| self.resolve_type_path(&segs))
            .ok_or_else(|| Error::TypeError(format!("use: unresolved `{}`", segs.join("::"))))?;
        // Alias the bare name to the absolute target path in the current module.
        self.reg.root.module_mut(&self.ns).aliases.insert(bare.clone(), segs);
        Ok(TopLevel::Use { alias: self.fq(&bare), target })
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
                    if let Some((adt, idx)) = self.resolve_ctor("none") {
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
            Value::Path(_) => {
                // A bare `::` path as an expression (e.g. a qualified constant).
                // Namespace/type path resolution lands in a later phase.
                return Err(Error::TypeError("`::` paths are not yet supported here".into()));
            }
        };
        // Reconcile synthesized types against the expectation. Literal and
        // constructor nodes already adopted `expected`, so this only fires on a
        // genuine mismatch. `Never` (a diverging expression) satisfies any
        // expected type.
        if let Some(e) = expected {
            if typed.ty != Type::Never && &typed.ty != e {
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
        let args = &elems[1..];

        // A `::`-path head is a module-qualified function or a `Type::method`
        // static associated function.
        if let Value::Path(pid) = elems[0] {
            let segs: Vec<String> = heap
                .path_segments(pid)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect();
            return self.check_path_call(heap, env, &segs, args, expected);
        }

        let head = match elems[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("call head must be a symbol or path".into())),
        };
        match head.as_str() {
            "if" => return self.check_if(heap, env, args, expected),
            "let" => return self.check_let(heap, env, args, expected),
            "let*" => return self.check_let_star(heap, env, args, expected),
            "progn" => {
                let (body, ty) = self.check_seq(heap, env, args, expected)?;
                // Represent progn as a let with no bindings.
                return Ok(Typed { expr: Expr::Let(Vec::new(), body), ty });
            }
            "when" => return self.check_when(heap, env, args, false),
            "unless" => return self.check_when(heap, env, args, true),
            "and" => return self.check_and_or(heap, env, args, true),
            "or" => return self.check_and_or(heap, env, args, false),
            "cond" => return self.check_cond(heap, env, args, expected),
            "setf" => return self.check_setf(heap, env, args),
            "while" => return self.check_while(heap, env, args),
            "match" => return self.check_match(heap, env, args, expected),
            "if-let" => return self.check_if_let(heap, env, args, expected),
            "panic!" => return self.check_panic(heap, env, args),
            _ => {}
        }
        // Constructor, then free function, then instance-method dispatch.
        if let Some((adt, idx)) = self.resolve_ctor(&head) {
            self.check_construct(heap, env, &adt, idx, args, expected)
        } else if let Some(fq) = self.resolve_fn(&head) {
            self.check_call(heap, env, &fq, args)
        } else {
            self.check_instance_method(heap, env, &head, args)
        }
    }

    /// A `::`-qualified call: a module-qualified free function, or a
    /// `Type::method` static associated function.
    fn check_path_call(
        &self,
        heap: &Heap,
        env: &Env,
        segs: &[String],
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        // A module-qualified free function, e.g. `math::id`.
        if let Some(fq) = self.resolve_fn_path(segs) {
            return self.check_call(heap, env, &fq, args);
        }
        // Otherwise `Type::member`: split the last segment as the member and
        // resolve the prefix as a type. The member is a constructor or a static
        // associated function of that type.
        if segs.len() >= 2 {
            let (type_segs, member) = segs.split_at(segs.len() - 1);
            let member = &member[0];
            if let Some(type_fq) = self.resolve_type_path(type_segs) {
                let def = self.reg.type_def(&type_fq).expect("resolved type exists");
                if let Some(variant) = def.variants.iter().position(|v| &v.name == member) {
                    return self.check_construct(heap, env, &type_fq, variant, args, expected);
                }
                if let Some(af) = def.assoc.get(member) {
                    if af.instance {
                        return Err(Error::TypeError(format!(
                            "`{}::{}` is an instance method; call it as ({} obj ...)",
                            type_fq, member, member
                        )));
                    }
                    return self.check_assoc_call(heap, env, &type_fq, member, None, args);
                }
            }
        }
        Err(Error::TypeError(format!("unresolved path: {}", segs.join("::"))))
    }

    /// Dispatch a bare call `(m recv args...)` as an instance method on the
    /// static type of its first argument.
    fn check_instance_method(
        &self,
        heap: &Heap,
        env: &Env,
        method: &str,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if !args.is_empty() {
            let recv = self.check(heap, env, args[0], None)?;
            let type_fq = match &recv.ty {
                Type::Named(n, _) => Some(n.clone()),
                _ => None,
            };
            if let Some(type_fq) = type_fq {
                if let Some(def) = self.reg.type_def(&type_fq) {
                    if def.assoc.get(method).map(|a| a.instance) == Some(true) {
                        return self
                            .check_assoc_call(heap, env, &type_fq, method, Some(recv), &args[1..]);
                    }
                }
            }
        }
        Err(Error::NoSuchFunction(method.to_string()))
    }

    /// Check a call to a type-associated function. For an instance method the
    /// already-checked `receiver` fills the first parameter slot.
    fn check_assoc_call(
        &self,
        heap: &Heap,
        env: &Env,
        type_fq: &Path,
        method: &str,
        receiver: Option<Typed>,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let instance = receiver.is_some();
        let af = self.reg.type_def(type_fq).expect("assoc type exists").assoc[method].clone();
        let offset = if instance { 1 } else { 0 };
        let expected_params = &af.sig.params[offset..];
        if args.len() != expected_params.len() {
            return Err(Error::TypeError(format!(
                "{}::{}: expected {} argument(s), got {}",
                type_fq,
                method,
                expected_params.len(),
                args.len()
            )));
        }
        let mut typed = Vec::new();
        if let Some(r) = receiver {
            typed.push(r);
        }
        for (arg, pty) in args.iter().zip(expected_params.iter()) {
            typed.push(self.check(heap, env, *arg, Some(pty))?);
        }
        Ok(Typed {
            expr: Expr::Assoc {
                type_name: type_fq.clone(),
                method: method.to_string(),
                instance,
                args: typed,
            },
            ty: af.sig.ret,
        })
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
        // A diverging (`Never`) then branch must not constrain the else branch.
        let else_expected = non_never(&then.ty).or(expected);
        let els = self.check(heap, env, args[2], else_expected)?;
        let ty = join_types(&then.ty, &els.ty)?;
        Ok(Typed { expr: Expr::If(Box::new(cond), Box::new(then), Box::new(els)), ty })
    }

    fn check_panic(&self, heap: &Heap, env: &Env, args: &[Value]) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("panic!: (panic! message)".into()));
        }
        let msg = self.check(heap, env, args[0], Some(&Type::Str))?;
        Ok(Typed { expr: Expr::Panic(Box::new(msg)), ty: Type::Never })
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

    /// `let*`: like `let` but each binding sees the earlier ones. Desugars to
    /// nested single-binding `let`s.
    fn check_let_star(
        &self,
        heap: &Heap,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("let*: (let* ((name val)...) body...)".into()));
        }
        let binds = heap.list_to_vec(args[0])?;
        self.let_star_rec(heap, env, &binds, &args[1..], expected)
    }

    fn let_star_rec(
        &self,
        heap: &Heap,
        env: &Env,
        binds: &[Value],
        body: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if binds.is_empty() {
            let (body, ty) = self.check_seq(heap, env, body, expected)?;
            return Ok(Typed { expr: Expr::Let(Vec::new(), body), ty });
        }
        let pair = heap.list_to_vec(binds[0])?;
        if pair.len() != 2 {
            return Err(Error::TypeError("let*: binding must be (name val)".into()));
        }
        let name = match pair[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("let*: binding name must be a symbol".into())),
        };
        let val = self.check(heap, env, pair[1], None)?;
        let child = env.extended(vec![(name.clone(), val.ty.clone())]);
        let inner = self.let_star_rec(heap, &child, &binds[1..], body, expected)?;
        let ty = inner.ty.clone();
        Ok(Typed { expr: Expr::Let(vec![(name, val)], vec![inner]), ty })
    }

    /// `(setf var value)`: assign to a bound variable. The value must match the
    /// variable's type; the expression evaluates to that value.
    fn check_setf(&self, heap: &Heap, env: &Env, args: &[Value]) -> Result<Typed, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("setf: (setf var value)".into()));
        }
        let name = match args[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("setf: target must be a variable".into())),
        };
        let ty = env
            .get(&name)
            .cloned()
            .ok_or_else(|| Error::TypeError(format!("setf: unbound variable: {}", name)))?;
        let value = self.check(heap, env, args[1], Some(&ty))?;
        Ok(Typed { expr: Expr::Set(name, Box::new(value)), ty })
    }

    /// `(while cond body...)`: loop while `cond` (a `bool`) holds; the body is
    /// evaluated for effect. The result is `Unit`.
    fn check_while(&self, heap: &Heap, env: &Env, args: &[Value]) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("while: (while cond body...)".into()));
        }
        let cond = self.check(heap, env, args[0], Some(&Type::Bool))?;
        let (body, _) = self.check_seq(heap, env, &args[1..], None)?;
        Ok(Typed { expr: Expr::While(Box::new(cond), body), ty: Type::Unit })
    }

    /// `when`/`unless`: evaluate the body for effect when the condition holds
    /// (`unless` negates). The result is `Unit`; the body's value is discarded.
    fn check_when(
        &self,
        heap: &Heap,
        env: &Env,
        args: &[Value],
        negate: bool,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("when/unless: (when cond body...)".into()));
        }
        let cond = self.check(heap, env, args[0], Some(&Type::Bool))?;
        let (mut body, _) = self.check_seq(heap, env, &args[1..], None)?;
        body.push(unit_node()); // discard the body's value -> Unit
        let guarded = Typed { expr: Expr::Let(Vec::new(), body), ty: Type::Unit };
        let (then, els) = if negate {
            (unit_node(), guarded)
        } else {
            (guarded, unit_node())
        };
        Ok(Typed {
            expr: Expr::If(Box::new(cond), Box::new(then), Box::new(els)),
            ty: Type::Unit,
        })
    }

    /// `and`/`or`: short-circuiting boolean operators desugared to nested `if`s.
    fn check_and_or(
        &self,
        heap: &Heap,
        env: &Env,
        args: &[Value],
        is_and: bool,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Ok(bool_node(is_and)); // (and) = true, (or) = false
        }
        let mut acc = self.check(heap, env, args[args.len() - 1], Some(&Type::Bool))?;
        for &a in args[..args.len() - 1].iter().rev() {
            let cond = self.check(heap, env, a, Some(&Type::Bool))?;
            let (then, els) = if is_and {
                (acc, bool_node(false)) // a && rest = if a then rest else false
            } else {
                (bool_node(true), acc) // a || rest = if a then true else rest
            };
            acc = Typed {
                expr: Expr::If(Box::new(cond), Box::new(then), Box::new(els)),
                ty: Type::Bool,
            };
        }
        Ok(acc)
    }

    /// `cond`: a chain of `(test body...)` clauses with an optional final
    /// `(else body...)`. Desugars to nested `if`s; all clause bodies must share
    /// a type (the missing-else fall-through is `Unit`).
    fn check_cond(
        &self,
        heap: &Heap,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("cond: (cond (test body...)...)".into()));
        }
        let clauses: Vec<Vec<Value>> =
            args.iter().map(|c| heap.list_to_vec(*c)).collect::<Result<_, _>>()?;

        // A trailing `(else body...)` provides the fall-through value.
        let last_is_else = clauses
            .last()
            .and_then(|c| c.first())
            .map(|v| is_symbol(heap, *v, "else"))
            == Some(true);

        let (mut acc, rest) = if last_is_else {
            let els = clauses.last().unwrap();
            let exp = expected.and_then(non_never);
            let (body, ty) = self.check_seq(heap, env, &els[1..], exp)?;
            (Typed { expr: Expr::Let(Vec::new(), body), ty }, &clauses[..clauses.len() - 1])
        } else {
            (unit_node(), &clauses[..])
        };

        for clause in rest.iter().rev() {
            if clause.is_empty() {
                return Err(Error::TypeError("cond: clause must be (test body...)".into()));
            }
            if is_symbol(heap, clause[0], "else") {
                return Err(Error::TypeError("cond: `else` must be the last clause".into()));
            }
            let cond = self.check(heap, env, clause[0], Some(&Type::Bool))?;
            let exp = non_never(&acc.ty);
            let (body, bty) = self.check_seq(heap, env, &clause[1..], exp)?;
            let ty = join_types(&bty, &acc.ty)?;
            let then = Typed { expr: Expr::Let(Vec::new(), body), ty: bty };
            acc = Typed {
                expr: Expr::If(Box::new(cond), Box::new(then), Box::new(acc)),
                ty,
            };
        }
        Ok(acc)
    }

    fn check_call(
        &self,
        heap: &Heap,
        env: &Env,
        name: &Path,
        args: &[Value],
    ) -> Result<Typed, Error> {
        let sig = self.reg.fn_sig(name).expect("caller checked presence").clone();
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
        Ok(Typed { expr: Expr::Call(name.clone(), typed), ty: sig.ret })
    }

    fn check_construct(
        &self,
        heap: &Heap,
        env: &Env,
        adt_name: &Path,
        variant: usize,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let def = self.reg.type_def(adt_name).expect("indexed adt exists").clone();
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
                type_name: adt_name.clone(),
                variant,
                args: typed_args,
            },
            ty: Type::Named(adt_name.clone(), result_args),
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
        let total_variants = self.reg.type_def(&adt_name).expect("adt exists").variants.len();

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
            // Diverging arms don't constrain the result type; concrete arms must
            // all agree (Never joins with anything).
            let arm_expected = result_ty.as_ref().and_then(non_never);
            let (body, body_ty) = self.check_seq(heap, &arm_env, &parts[1..], arm_expected)?;
            result_ty = Some(match result_ty {
                None => body_ty,
                Some(r) => join_types(&r, &body_ty)?,
            });
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
        // The constructor is resolved against the scrutinee's type (its
        // variants), not the namespace — the type context disambiguates it.
        let (adt_name, targs) = self.expect_adt(expected)?;
        let def = self.reg.type_def(&adt_name).expect("adt exists").clone();
        let variant = def.variants.iter().position(|vr| vr.name == ctor).ok_or_else(|| {
            Error::TypeError(format!("`{}` is not a constructor of `{}`", ctor, adt_name))
        })?;
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

    /// Require `ty` to be a registered data type, returning its path and args.
    fn expect_adt(&self, ty: &Type) -> Result<(Path, Vec<Type>), Error> {
        match ty {
            Type::Named(n, args) if self.reg.type_def(n).is_some() => Ok((n.clone(), args.clone())),
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

/// A typed `Unit` literal node.
fn unit_node() -> Typed {
    Typed { expr: Expr::Unit, ty: Type::Unit }
}

/// A typed boolean literal node.
fn bool_node(b: bool) -> Typed {
    Typed { expr: Expr::Bool(b), ty: Type::Bool }
}

/// Whether `v` is the symbol named `name`.
fn is_symbol(heap: &Heap, v: Value, name: &str) -> bool {
    matches!(v, Value::Symbol(id) if heap.symbol_name(id) == name)
}

/// `Some(t)` unless `t` is `Never` (which never constrains an expectation).
fn non_never(t: &Type) -> Option<&Type> {
    if *t == Type::Never {
        None
    } else {
        Some(t)
    }
}

/// Join two branch types: `Never` is absorbed by the other; otherwise the two
/// must be equal.
fn join_types(a: &Type, b: &Type) -> Result<Type, Error> {
    if *a == Type::Never {
        return Ok(b.clone());
    }
    if *b == Type::Never {
        return Ok(a.clone());
    }
    if a == b {
        return Ok(a.clone());
    }
    Err(Error::TypeError(format!(
        "branches have incompatible types: {:?} vs {:?}",
        a, b
    )))
}

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

/// Whether `path` is a type variable in `params` (a single-segment name).
fn is_param(path: &crate::Path, params: &HashSet<String>) -> bool {
    path.is_simple() && params.contains(path.local())
}

/// Whether `t` mentions any type parameter in `params`.
fn type_has_param(t: &Type, params: &HashSet<String>) -> bool {
    match t {
        Type::Named(n, args) => {
            (args.is_empty() && is_param(n, params)) || args.iter().any(|a| type_has_param(a, params))
        }
        Type::Fn(ps, r) => ps.iter().any(|p| type_has_param(p, params)) || type_has_param(r, params),
        _ => false,
    }
}

/// Replace type parameters in `t` with their bindings from `subst`.
fn subst_apply(t: &Type, subst: &HashMap<String, Type>) -> Type {
    match t {
        Type::Named(n, args) if args.is_empty() && n.is_simple() => match subst.get(n.local()) {
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
    // A diverging value (`Never`) unifies with any template without binding.
    if *actual == Type::Never {
        return Ok(());
    }
    if let Type::Named(n, args) = tmpl {
        if args.is_empty() && is_param(n, params) {
            let key = n.local().to_string();
            return match subst.get(&key) {
                Some(bound) if bound == actual => Ok(()),
                Some(bound) => Err(Error::TypeError(format!(
                    "conflicting types for `{}`: {:?} vs {:?}",
                    key, bound, actual
                ))),
                None => {
                    subst.insert(key, actual.clone());
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
