//! Macro calls: binding a macro's lambda list, `macroexpand`/`macroexpand-1`,
//! and the `MacroExpander` the checker expands through.

use std::collections::HashMap;
use std::rc::Rc;
use crate::{wk, Heap, MacroExpander, Path, TypeId, Value};
use crate::eval::value::EvalError;
use super::{result_err, result_ok, FnDef, Interp, MACROEXPAND_RESULT_TYPE};

impl Interp {
    /// Bind a macro call's raw (unevaluated) argument forms to its parameters,
    /// producing one value per parameter in `f.params` order —
    /// required, then `&optional`, then `&rest`, then `&key` — ready for
    /// [`Self::enter`]. Omitted `&optional`/`&key` arguments evaluate their
    /// default-value body (in an environment holding the params already bound,
    /// CL-style); an empty body binds the empty list.
    ///
    /// GC safety: every value built here (a `&rest` list, an evaluated
    /// default) is stashed in a live heap cell as it is produced, and
    /// each cell is an implicit heap root (`Heap::alloc_cell`) for as long as
    /// the returned `env` lives. Those cells also form the environment the
    /// next default is evaluated in, so an earlier bound param is protected
    /// across a later default's allocations. The caller drops `env` only after
    /// `apply` has consumed `argv` (and `apply`'s own cell allocations can
    /// never trigger a collection — see `Self::slot`), so the raw `argv`
    /// pointers stay valid in the gap between.
    /// `environment` is what an `&environment` parameter is bound to, the
    /// last parameter; rooted by the caller.
    fn bind_macro_args(&self, heap: &mut Heap, f: &FnDef, raw_args: &[Value], environment: Option<Value>) -> Result<Vec<Value>, EvalError> {
        let lambda = f
            .lambda
            .as_ref()
            .ok_or_else(|| EvalError::Internal("bind_macro_args on a non-macro FnDef".into()))?;
        let n_req = lambda.required;
        let n_opt = lambda.optionals.len();
        if raw_args.len() < n_req {
            return Err(EvalError::Panic(format!("expected at least {} argument(s), got {}", n_req, raw_args.len())));
        }

        // Fast path — the overwhelmingly common macro shape: only required
        // params, optionally a trailing `&rest`, with no `&optional`/`&key`
        // defaults to evaluate. No default eval means no allocation that could
        // trigger a GC after the (already-rooted) raw args are placed, so the
        // protective per-param Heap-cell environment the general path builds is
        // unnecessary here (`&rest` list aside, which `build_sexpr_list` roots
        // internally). This keeps `while`/`dotimes`/`cond`/... — expanded en
        // masse during checking — allocation-free, exactly as before this
        // feature.
        if n_opt == 0 && lambda.keys.is_empty() {
            let mut argv: Vec<Value> = raw_args[..n_req].to_vec();
            if f.rest {
                let list = self.build_sexpr_list(heap, &raw_args[n_req..])?;
                argv.push(list);
            } else if raw_args.len() > n_req {
                return Err(EvalError::Panic(format!("expected {} argument(s), got {}", n_req, raw_args.len())));
            }
            argv.extend(environment);
            return Ok(argv);
        }

        // The positional region is `required + optional`; `&rest`/`&key`
        // arguments (if any) begin right after however many of those slots the
        // call actually filled.
        let n_pos = n_req + n_opt;
        let pos_end = raw_args.len().min(n_pos);

        let mut argv: Vec<Value> = Vec::with_capacity(f.params.len());
        // The environment the `&optional`/`&key` defaults are evaluated in: the
        // ordinary heap chain `eval_core` walks, one frame per binding, so a
        // default sees exactly the parameters bound before it and nothing after
        // — `let*`, which is what CL specifies here.
        //
        // This is also what keeps the bound values alive: a frame is heap data
        // the collector traces, so the protective per-parameter cells an earlier
        // version built purely to anchor them are unnecessary. One mechanism,
        // both jobs.
        let roots_base = heap.root_count();
        let mut env = Value::Empty;
        let mut pi = 0usize; // index into `f.params`
        // A closure would borrow `self`/`heap` mutably twice, so bind inline.
        macro_rules! bind {
            ($v:expr) => {{
                let v = $v;
                heap.push_root(v);
                env = super::core_eval::extend_env(heap, &[(f.params[pi], v)], env)?;
                heap.push_root(env);
                argv.push(v);
                pi += 1;
            }};
        }

        // required
        for &a in &raw_args[..n_req] {
            bind!(a);
        }
        // &optional — filled positionally, else its default (or the empty list)
        for (oi, default) in lambda.optionals.iter().enumerate() {
            let arg_idx = n_req + oi;
            let v = if arg_idx < pos_end {
                raw_args[arg_idx]
            } else {
                self.eval_macro_default(heap, default, env)?
            };
            bind!(v);
        }
        // The trailing region past the positional args: a `&rest` list and/or
        // a `&key` plist both read from it (CL binds `&rest` to the whole tail
        // even when `&key` is also present).
        let tail = &raw_args[pos_end..];
        if f.rest {
            let list = self.build_sexpr_list(heap, tail)?;
            bind!(list);
        }
        if !lambda.keys.is_empty() {
            let supplied = self.parse_keyword_args(heap, tail, &lambda.keys)?;
            for (kname, default) in &lambda.keys {
                let v = match supplied.get(kname) {
                    Some(val) => *val,
                    None => self.eval_macro_default(heap, default, env)?,
                };
                bind!(v);
            }
        } else if !f.rest && !tail.is_empty() {
            heap.truncate_roots(roots_base);
            return Err(EvalError::Panic(format!("expected at most {} argument(s), got {}", n_pos, raw_args.len())));
        }

        // The bound values are about to be re-bound by `apply` from `argv`, so
        // the frames built here have done their job. `argv` itself is rooted by
        // the caller (`expand_macro` roots the raw args, and every value in
        // `argv` is either one of those or a default the caller roots next).
        heap.truncate_roots(roots_base);
        argv.extend(environment);
        Ok(argv)
    }

    /// Evaluate an `&optional`/`&key` default-value body to the `Sexpr` value
    /// bound when the argument is omitted — an empty body means the empty
    /// list (`Option<Sexpr>`'s `None`).
    ///
    /// `env` holds the parameters bound before this one, which a default may
    /// reference: `(defmacro dup (x &optional (y x)) ...)` copies `x` when `y`
    /// is omitted. That is CL's rule, and `typeck::defmacro` checks each
    /// default in exactly this environment — so dropping `env` here made the
    /// checker accept a form the expander could not run (`macro `dup`: unbound
    /// variable: x`).
    fn eval_macro_default(&self, heap: &mut Heap, default: &[Value], env: Value) -> Result<Value, EvalError> {
        // The empty body is "no default", which binds the empty list at
        // expansion time.
        let mut result = Value::Empty;
        for form in default {
            result = self.eval_core(heap, *form, env)?;
        }
        Ok(result)
    }

    /// Build a fresh `Sexpr` list from `items`, rooting the growing tail
    /// across each `cons` (which may collect). The shared tail-builder for a
    /// macro's `&rest` parameter.
    fn build_sexpr_list(&self, heap: &mut Heap, items: &[Value]) -> Result<Value, EvalError> {
        let mut list = Value::Empty;
        for v in items.iter().rev() {
            heap.push_root(list);
            // core-build-ok: a `Sexpr` list of user data for a macro's `&rest`,
            // not a core form. The growing tail is rooted on the line above and
            // popped on the line below.
            let consed = heap.cons(*v, list);
            heap.pop_root();
            list = consed.map_err(|e| EvalError::Panic(e.to_string()))?;
        }
        Ok(list)
    }

    /// Parse a macro call's trailing arguments as a `&key` plist — pairs of
    /// `(:name, value)` — returning each supplied param name mapped to its
    /// value form. A keyword must be a symbol `:name` matching one of the
    /// declared keys; the plist must have even length; a duplicate keeps the
    /// first value (CL). Unknown keywords and odd length are errors.
    fn parse_keyword_args(
        &self,
        heap: &Heap,
        tail: &[Value],
        keys: &[(String, Vec<Value>)],
    ) -> Result<HashMap<String, Value>, EvalError> {
        if !tail.len().is_multiple_of(2) {
            return Err(EvalError::Panic("odd number of &key arguments (each key needs a value)".into()));
        }
        let mut map: HashMap<String, Value> = HashMap::new();
        let mut i = 0;
        while i < tail.len() {
            let stripped = match tail[i] {
                Value::Symbol(id) if id.is_keyword() => id.name(),
                Value::Symbol(id) => {
                    return Err(EvalError::Panic(format!("expected a keyword like :name, got `{}`", heap.symbol_spelling(id))))
                }
                _ => return Err(EvalError::Panic("&key argument name must be a keyword symbol like :name".into())),
            };
            if !keys.iter().any(|(k, _)| k == stripped) {
                return Err(EvalError::Panic(format!("unknown &key argument `:{}`", stripped)));
            }
            map.entry(stripped.to_string()).or_insert(tail[i + 1]);
            i += 2;
        }
        Ok(map)
    }
}

impl Interp {
    /// The `eval` builtin: type-check `arg` (a runtime `Sexpr`) against the
    /// program's *current* global environment and run it, CL-style —
    /// `(eval form) => Result<Sexpr, Error>`. Sees every global definition
    /// (functions, variables, types, macros), including ones added earlier at
    /// runtime; does *not* see the caller's lexical locals (the empty `Env`
    /// below is CL's "null lexical environment"). A form that *defines*
    /// something (`(eval '(defun foo () 42))`) registers it immediately and
    /// permanently, exactly as if typed at top level — that is why `root` is a
    /// `RefCell` (this `&self` path must write it) and why the checker handle
    /// is `&mut`-borrowed for `check_form_at`.
    ///
    /// Return value follows CL: an expression yields its value (as a `Sexpr`);
    /// a definition yields the defined name as a symbol. A malformed or
    /// ill-typed form is a recoverable `Err(error ...)`, not a panic — `eval`
    /// is handed runtime data the program doesn't control. (A *runtime* panic
    /// inside the evaluated code, e.g. division by zero, still propagates like
    /// any other, matching code written directly.)
    ///
    /// GC-root discipline mirrors `crate::main`'s `try_run_pending`: the raw
    /// argument `Value` rides on the Rust stack, where the collector cannot
    /// see it, so it must be `push_root`ed across `check_form_at` — which
    /// conses during macro expansion and can trigger a GC — and popped back
    /// to the entry mark before `exec`. The result-building afterward
    /// only allocates through the growable box store (`alloc_enum`/
    /// `intern_symbol`/scalar boxing), never `cons`, so it needs no rooting.
    /// CL's `macroexpand-1`: one expansion step on a runtime `Sexpr`.
    ///
    /// The step is the checker's own (`Checker::try_expand_toplevel_macro`),
    /// so what a program sees here is exactly what checking would have seen —
    /// there is no second expander to drift from it.
    ///
    /// `Ok(None)` means "not a macro call", which is what CL's second return
    /// value says. Carrying it in the `Option` rather than beside the form is
    /// both what this language has (no multiple values) and strictly more
    /// informative: a macro whose expansion is its own call form is
    /// distinguishable from a non-macro, which CL's boolean cannot do.
    ///
    /// `env` is the `Option<Environment>` the call was given: `Value::Empty`
    /// for none.
    pub fn macroexpand_1_form(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        match self.expand_step(heap, form, env) {
            Ok(Some(v)) => {
                heap.push_root(v);
                let some = heap.alloc_option(TypeId::OPTION_SEXPR, Some(v));
                heap.pop_root();
                heap.push_root(some);
                let out = result_ok(heap, &MACROEXPAND_RESULT_TYPE, some);
                heap.pop_root();
                Ok(out)
            }
            Ok(None) => {
                let none = heap.alloc_option(TypeId::OPTION_SEXPR, None);
                heap.push_root(none);
                let out = result_ok(heap, &MACROEXPAND_RESULT_TYPE, none);
                heap.pop_root();
                Ok(out)
            }
            Err(msg) => Ok(result_err(heap, &MACROEXPAND_RESULT_TYPE, wk::EVAL_ERROR, msg)),
        }
    }

    /// CL's `macroexpand`: [`Self::macroexpand_1_form`] until the form is no
    /// longer a macro call. A macro that expands to a call of itself would
    /// spin here exactly as it does in CL's expander, and in the checker.
    pub fn macroexpand_form(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let mut cur = form;
        let mark = heap.root_count();
        heap.push_root(cur);
        loop {
            match self.expand_step(heap, cur, env) {
                Ok(Some(next)) => {
                    cur = next;
                    heap.push_root(cur);
                }
                Ok(None) => break,
                Err(msg) => {
                    heap.truncate_roots(mark);
                    return Ok(result_err(heap, &MACROEXPAND_RESULT_TYPE, wk::EVAL_ERROR, msg));
                }
            }
        }
        heap.truncate_roots(mark);
        heap.push_root(cur);
        let out = result_ok(heap, &MACROEXPAND_RESULT_TYPE, cur);
        heap.pop_root();
        Ok(out)
    }

    /// One expansion step, or `None` when `form` is not a macro use, in the
    /// `Option<Environment>` `env`.
    ///
    /// With an environment, at the macro call it was made for, which must
    /// still be being expanded. Without one, CL's null lexical environment:
    /// the module's definitions — those of the check that is expanding a
    /// macro now, when one is, since that check holds the checker.
    fn expand_step(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Option<Value>, String> {
        let mark = heap.root_count();
        heap.push_root(form);
        let out = self.expand_step_in(heap, form, env);
        heap.truncate_roots(mark);
        out
    }

    fn expand_step_in(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Option<Value>, String> {
        use crate::typeck::checker::ExpansionEnv;
        let site = match env {
            Value::Empty => self.expansions.borrow().last().map(|(_, site)| (*site, false)),
            Value::Boxed(id) => {
                let Value::Int(serial) = heap.struct_field(id, 0) else {
                    unreachable!("an `Environment` holds its expansion's number")
                };
                match self.expansions.borrow().iter().find(|(n, _)| *n == serial) {
                    Some((_, site)) => Some((*site, true)),
                    None => {
                        return Err("macroexpand: the environment is of a macro expansion that has ended; it is good only while the macro that was given it runs".to_string())
                    }
                }
            }
            other => unreachable!("an `Option<Environment>` is the empty list or a box, not {:?}", other),
        };
        if let Some((site, lexical)) = site {
            // SAFETY: the entry is in `expansions`, so the `expand_macro`
            // call that pushed it, and the place it points at, are still on
            // the stack below this one.
            let site = unsafe { &*site };
            return site.expand_1(heap, self, form, lexical).map_err(|e| e.to_string());
        }
        let checker = match &self.checker {
            Some(c) => Rc::clone(c),
            None => return Err("macroexpand: unavailable in this context (no checker handle)".to_string()),
        };
        let checker = checker.borrow();
        let out = checker.toplevel_env().expand_1(heap, self, form, false);
        out.map_err(|e| e.to_string())
    }
}

impl MacroExpander for Interp {
    /// Expand one macro call: look `path` up in `fns` (a `defmacro` is stored
    /// there exactly like a `defun` — see [`Interp::exec`]'s `Defmacro` arm),
    /// wrap each raw (unevaluated) argument form as a datum with no
    /// conversion (this *is* the implicit quoting that makes macro arguments
    /// unevaluated data), and run it like any other call.
    ///
    /// GC-root discipline: `raw_args` are raw `Value`s on the Rust stack,
    /// invisible to the collector until `bind_macro_args` binds them into
    /// cells, so they are `push_root`ed across the whole expansion and popped
    /// on the way out. Plain LIFO — `apply` leaves the root stack exactly as
    /// it found it, because bindings are cells and need no roots pushed for
    /// them.
    fn expand_macro(&self, heap: &mut Heap, path: &Path, raw_args: Vec<Value>, env: &dyn crate::typeck::checker::ExpansionEnv) -> Result<Value, String> {
        let f = self.root.borrow().get_fn(path).ok_or_else(|| format!("no such macro: {}", path))?;
        let mark = heap.root_count();
        for v in &raw_args {
            heap.push_root(*v);
        }
        let serial = self.next_expansion.get();
        self.next_expansion.set(serial + 1);
        // SAFETY: only the lifetime is erased. `Expanding` takes the entry
        // out again before this call returns, by unwinding too, and `env`
        // outlives the call.
        let site: *const (dyn crate::typeck::checker::ExpansionEnv + 'static) = unsafe { std::mem::transmute(env as *const dyn crate::typeck::checker::ExpansionEnv) };
        self.expansions.borrow_mut().push((serial, site));
        let _expanding = Expanding(self, serial);
        // What an `&environment` parameter is bound to: this call's number,
        // in a box that is an `Environment`.
        let environment = f.lambda.as_ref().is_some_and(|l| l.environment).then(|| {
            let ty = crate::type_key::type_id(&crate::typeck::builtins::environment_type());
            let v = crate::type_key::alloc_struct_of(heap, ty, vec![Value::Int(serial)]);
            heap.push_root(v);
            v
        });
        // A closure built while a macro body executes (check-time expansion)
        // is JIT-compiled like any other (interp-closure removal Stage 6,
        // retiring Stage 7's `jit_suppressed` guard): the island is always
        // loaded (Stage 5), so `jit_define_closure` succeeds; in an embedder
        // that skipped island loading it declines *Benign* ("island not
        // loaded") and falls back exactly as before. A macro that builds a
        // closure over a non-tier type is the only behavior change — it now
        // JITs (or hard-declines *Gap*) instead of silently interpreting —
        // and no such macro exists (macro bodies close over ordinary types).
        //
        // `bind_macro_args` validates arity and binds every parameter
        // (defaulting omitted `&optional`/`&key` args); the call-site checker
        // has already range-checked the raw count (`typeck::macros`),
        // so any error here is a keyword/plist detail it deliberately deferred.
        //
        // `enter`, not `apply`: a macro body is an ordinary `Sexpr -> Sexpr`
        // function and is compiled like one when something has compiled it
        // (`(compile <macro>)`, or the prelude's dump generator, which now
        // collects `defmacro` alongside `defun`). `apply` always tree-walks;
        // `enter` takes the compiled body when there is one. Argument
        // *binding* stays interpreted either way — `bind_macro_args` is what
        // evaluates omitted `&optional`/`&key` defaults, and it runs before
        // this — so only the body crosses.
        let result = match self.bind_macro_args(heap, &f, &raw_args, environment) {
            Ok(argv) => self.enter(heap, &f, argv),
            Err(e) => Err(e),
        };
        heap.truncate_roots(mark);
        match result {
            Ok(v) => Ok(v),
            Err(e) => Err(e.to_string()),
        }
    }

    /// [`Self::exec`] on the form, which is the same arm the driver's own run
    /// of it between top-level forms takes. A `macrolet` binding's name is
    /// synthesized and unwritable (`typeck::macrolet`), so nothing else
    /// can collide with it and nothing needs to remove it afterwards: the
    /// *scope* is the checker's table, not this registration.
    fn install(&self, heap: &mut Heap, form: Value) -> Result<(), String> {
        self.exec(heap, form).map(|_| ()).map_err(|e| e.to_string())
    }
}

/// A macro call [`Interp::expand_macro`] is expanding: its entry in
/// `expansions`, taken out when the expansion ends.
struct Expanding<'i>(&'i Interp, i64);

impl Drop for Expanding<'_> {
    fn drop(&mut self) {
        let popped = self.0.expansions.borrow_mut().pop();
        debug_assert_eq!(popped.map(|(n, _)| n), Some(self.1), "expansions end in the order they began");
    }
}
