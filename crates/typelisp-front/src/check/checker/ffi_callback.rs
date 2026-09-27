//! A function handed to C as a callback — a `defffi` argument whose declared
//! type is `(fn (T...) R)`.
//!
//! C holds nothing but a code address and calls it with exactly the declared
//! arguments, so the function it is given can have no environment: there is
//! nowhere for C to pass one. Three spellings of such a function are
//! accepted, all written directly at the argument:
//!
//! * the name of a top-level function (a generic one is specialized from the
//!   declared type, as any function value is),
//! * a `lambda` with no free variables,
//! * the name of a `labels` function that, together with every sibling it
//!   reaches, has no free variables — siblings share one captured
//!   environment, so a sibling's capture is this function's too.
//!
//! A variable holding a function is refused: which function it holds, and so
//! whether it has an environment, is not known until it runs.
//!
//! The last two are **lifted**: a new top-level function carries the body
//! (for `labels`, a copy of the block that calls the function), so every
//! callback reaches the backend as a named top-level function and the entry
//! C calls never needs an environment. The lifted definitions travel ahead of
//! the form being checked, in the same bundle a generic's instantiations do
//! (`Checker::check_form_at`).

use super::*;
use typelisp_mem::SymRef;
use crate::ffi_callback::{CallbackSig, ADDRESS_BUILTIN};

/// A `labels` block whose function names are in scope while its bodies and
/// its trailing body are checked.
pub(super) struct LabelsScope {
    /// Where the block's names start in the `Env` they were bound into — a
    /// reference resolves to this block's function exactly when its nearest
    /// binding sits in `start..start + names.len()`.
    start: usize,
    names: Vec<String>,
    /// References to one of these functions as a C callback, resolved once
    /// every body in the block has been checked.
    pending: Vec<PendingLabelsCallback>,
}

struct PendingLabelsCallback {
    name: String,
    lifted: Path,
    loc: Option<Loc>,
}

/// A definition made by lifting, waiting for `check_form_at` to register its
/// signature and place its form ahead of the form that asked for it.
pub(super) struct LiftedCallback {
    pub(super) path: Path,
    pub(super) sig: FnSig,
    pub(super) form: Value,
}

impl Checker {
    /// Every callback entry the checked code asks for, as its key
    /// ([`CallbackSig::key`]). Read by `compile::aot::compile_file`, which
    /// emits one entry per key and registers it at startup.
    pub fn ffi_callback_keys(&self) -> Vec<String> {
        self.callback_keys.borrow().iter().cloned().collect()
    }

    /// The argument `arg` to a `defffi` parameter declared `fn_ty` — a
    /// function type — as the entry address C is handed.
    pub(super) fn check_callback_arg(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        arg: Value,
        loc: Option<Loc>,
        fn_ty: &Type,
        callee: &Path,
    ) -> Result<Checked, Error> {
        let checked = self.check_at(heap, interp, env, arg, Some(fn_ty), loc.clone())?;
        let target = match core::op(heap, checked.form) {
            Some("lambda") => self.lift_lambda(heap, checked.form, fn_ty)?,
            Some("fnref") => {
                let path = core::path_field(heap, checked.form, 2)
                    .ok_or_else(|| Error::TypeError("internal error: a fnref node without a path".into()))?;
                if self.reg.fn_sig(&path).is_some_and(|s| s.builtin) {
                    return Err(Error::TypeError(format!(
                        "{}: `{}` is a builtin, and a builtin has no body of its own to hand C. \
                         Pass a `lambda` that calls it instead.",
                        callee, path
                    )));
                }
                path
            }
            Some("var") => {
                let name = match core::field(heap, checked.form, 0) {
                    Some(Value::Symbol(id)) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("internal error: a var node without a name".into())),
                };
                self.pend_labels_callback(env, &name, loc.as_ref(), callee)?
            }
            _ => {
                return Err(Error::TypeError(format!(
                    "{}: a C callback must be written where it is passed — the name of a function, \
                     or a `lambda` — so that it can be checked for free variables here",
                    callee
                )))
            }
        };
        let sig = CallbackSig::from_fn_type(&target, fn_ty)
            .ok_or_else(|| Error::TypeError(format!("internal error: `{}` is not a callback type", fn_ty)))?;
        let key = sig.key();
        self.callback_keys.borrow_mut().insert(key.clone());
        let form = self.callback_address_form(heap, &key)?;
        Ok(Checked::new(form, Type::Ptr))
    }

    /// `(call (" ffi-callback-address") HOME PATH (REPR) (str KEY))` — the
    /// entry's address, as a raw word.
    fn callback_address_form(&self, heap: &mut Heap, key: &str) -> Result<Value, Error> {
        let arg = Checked::new(forms::str_lit_form(heap, key)?, Type::Str);
        let r = Ref::synthetic(Path::root(ADDRESS_BUILTIN));
        let form = self.call_form(heap, &r, &[arg])?;
        Ok(forms::rooted(heap, form))
    }

    /// A path for a lifted definition, in the current namespace, that names
    /// nothing yet — neither a function already registered nor one lifted
    /// earlier in this form.
    fn fresh_callback_path(&self) -> Path {
        loop {
            let n = self.callback_seq.get();
            self.callback_seq.set(n + 1);
            let name = format!("callback${}", n);
            if !self.cur_ns().fns.contains_key(&name) {
                return self.fq(&name);
            }
        }
    }

    fn lifted_sig(fn_ty: &Type) -> FnSig {
        let (params, ret) = match fn_ty {
            Type::Fn(ps, _, r) => (ps.clone(), (**r).clone()),
            _ => (Vec::new(), Type::Unit),
        };
        FnSig {
            ffi: false,
            type_params: Vec::new(),
            params,
            ret,
            public: false,
            rest: None,
            builtin: false,
            bounds: BTreeMap::new(),
            optionals: Vec::new(),
            keys: Vec::new(),
        }
    }

    /// A checked `(lambda ((SYM REPR)...) RET-REPR BODY...)` as a top-level
    /// `(defun PATH ((SYM REPR)...) RET-REPR false BODY...)`, or an error
    /// naming what it captures.
    fn lift_lambda(&self, heap: &mut Heap, lambda: Value, fn_ty: &Type) -> Result<Path, Error> {
        let fields = core::fields(heap, lambda)?;
        let params = fields.first().copied().unwrap_or(Value::Empty);
        let bound = param_syms(heap, params)?;
        let captured = crate::check::freevars::free_vars(heap, &fields[2..], &bound, &HashSet::new())?;
        if !captured.is_empty() {
            return Err(Error::TypeError(format!(
                "a `lambda` passed to C as a callback cannot refer to local variables of the \
                 code around it — C calls it with nothing but its arguments, so there is \
                 nowhere to keep them. This one refers to {}.",
                name_list(&captured)
            )));
        }
        let path = self.fresh_callback_path();
        let mut f = Items::new(heap);
        let p = forms::path_form(f.heap(), &path);
        f.push(p);
        f.extend(fields.iter().copied().take(2));
        f.push(Value::Bool(false));
        f.extend(fields[2..].iter().copied());
        let form = f.finish("defun")?;
        let form = forms::rooted(heap, form);
        self.lifted.borrow_mut().push(LiftedCallback { path: path.clone(), sig: Self::lifted_sig(fn_ty), form });
        Ok(path)
    }

    /// A reference to a local function, noted for [`Self::resolve_labels_callbacks`]
    /// — its body, and its siblings', are not all checked yet — and the path
    /// its lifted copy will have. Any other local is a variable, refused.
    fn pend_labels_callback(&self, env: &Env, name: &str, loc: Option<&Loc>, callee: &Path) -> Result<Path, Error> {
        let at = env.position(name);
        let mut scopes = self.labels_scopes.borrow_mut();
        let scope = scopes
            .iter_mut()
            .rev()
            .find(|s| at.is_some_and(|i| i >= s.start && i < s.start + s.names.len()));
        let Some(scope) = scope else {
            return Err(Error::TypeError(format!(
                "{}: `{}` is a variable, and which function a variable holds is not known until \
                 the program runs — so neither is whether it has local variables C could not \
                 give it. Pass the function's own name, or a `lambda`.",
                callee, name
            )));
        };
        let lifted = self.fresh_callback_path();
        scope.pending.push(PendingLabelsCallback { name: name.to_string(), lifted: lifted.clone(), loc: loc.cloned() });
        Ok(lifted)
    }

    /// Opens a `labels` block's scope: its `names` were just bound into an
    /// `Env` that held `start` bindings before them.
    pub(super) fn open_labels_scope(&self, start: usize, names: Vec<String>) {
        self.labels_scopes.borrow_mut().push(LabelsScope { start, names, pending: Vec::new() });
    }

    /// Closes the innermost `labels` scope and lifts every callback reference
    /// made to one of its functions. `defs` are the block's checked
    /// definitions; `None` when checking the block failed, which only closes
    /// the scope.
    pub(super) fn close_labels_scope(&self, heap: &mut Heap, defs: Option<&[LabelsDef]>) -> Result<(), Error> {
        let scope = self.labels_scopes.borrow_mut().pop().expect("a labels scope is open");
        let Some(defs) = defs else { return Ok(()) };
        for p in scope.pending {
            self.lift_labels_callback(heap, defs, &p).map_err(|e| match &p.loc {
                Some(l) => e.at(l.clone()),
                None => e,
            })?;
        }
        Ok(())
    }

    /// `(defun LIFTED ((" a0" T0)...) R false (labels (DEF...) (G " a0"...)))`,
    /// where `DEF...` are `g` and every sibling it reaches, or an error naming
    /// what they capture.
    fn lift_labels_callback(&self, heap: &mut Heap, defs: &[LabelsDef], p: &PendingLabelsCallback) -> Result<(), Error> {
        let names: HashSet<&str> = defs.iter().map(|(n, _, _, _)| n.as_str()).collect();
        // Each definition's references out of its own parameters, split into
        // the siblings it reaches and the locals it captures.
        let mut refs: HashMap<&str, (Vec<String>, Vec<String>)> = HashMap::new();
        for (name, params, _, body) in defs {
            let bound: HashSet<SymRef> = params.iter().map(|(n, _)| typelisp_mem::symbols::intern(n)).collect();
            let free = crate::check::freevars::free_vars(heap, body, &bound, &HashSet::new())?;
            let (sibs, caps): (Vec<String>, Vec<String>) = free
                .iter()
                .map(|s| s.name().to_string())
                .partition(|n| names.contains(n.as_str()));
            refs.insert(name.as_str(), (sibs, caps));
        }
        let mut reached: Vec<&str> = vec![p.name.as_str()];
        let mut i = 0;
        while i < reached.len() {
            let (sibs, _) = &refs[reached[i]];
            for s in sibs {
                if let Some(n) = names.get(s.as_str()) {
                    if !reached.contains(n) {
                        reached.push(n);
                    }
                }
            }
            i += 1;
        }
        let capturing: Vec<String> = reached
            .iter()
            .filter(|n| !refs[**n].1.is_empty())
            .map(|n| format!("`{}` refers to {}", n, refs[*n].1.iter().map(|c| format!("`{}`", c)).collect::<Vec<_>>().join(", ")))
            .collect();
        if !capturing.is_empty() {
            return Err(Error::TypeError(format!(
                "the local function `{}` cannot be passed to C as a callback: C calls it with \
                 nothing but its arguments, and it needs local variables of the code around it \
                 ({}{})",
                p.name,
                capturing.join("; "),
                if reached.len() > 1 { " — the local functions it calls count too" } else { "" }
            )));
        }
        let (_, g_params, g_ret, _) = defs.iter().find(|(n, _, _, _)| n == &p.name).expect("a pending name is the block's");
        let fn_ty = Type::Fn(g_params.iter().map(|(_, t)| t.clone()).collect(), None, Box::new(g_ret.clone()));
        // Parameter names no source can write, so none of them shadows — or
        // is shadowed by — a local function of the copied block.
        let params: Vec<(String, Type)> =
            g_params.iter().enumerate().map(|(i, (_, t))| (format!(" a{}", i), t.clone())).collect();
        let mut args = Vec::with_capacity(params.len());
        for (n, t) in &params {
            args.push(Checked::new(forms::var_form(heap, n)?, t.clone()));
        }
        let callee = forms::var_form(heap, &p.name)?;
        let call = self.apply_form(heap, callee, g_ret, &args)?;
        let call = forms::rooted(heap, call);
        let kept: Vec<LabelsDef> = defs.iter().filter(|(n, _, _, _)| reached.contains(&n.as_str())).cloned().collect();
        let block = self.labels_form(heap, &kept, &[call])?;
        let block = forms::rooted(heap, block);
        let form = self.defun_form(heap, &p.lifted, &params, g_ret, false, &[block])?;
        let form = forms::rooted(heap, form);
        self.lifted.borrow_mut().push(LiftedCallback { path: p.lifted.clone(), sig: Self::lifted_sig(&fn_ty), form });
        Ok(())
    }
}

/// The symbols a `((SYM REPR)...)` parameter list binds.
fn param_syms(heap: &Heap, params: Value) -> Result<HashSet<SymRef>, Error> {
    let mut out = HashSet::new();
    for p in heap.list_to_vec(params)? {
        if let Some(Value::Symbol(id)) = heap.list_to_vec(p)?.first() {
            out.insert(*id);
        }
    }
    Ok(out)
}

fn name_list(names: &[SymRef]) -> String {
    names.iter().map(|s| format!("`{}`", s.name())).collect::<Vec<_>>().join(", ")
}
