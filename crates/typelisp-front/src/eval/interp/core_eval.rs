//! The evaluator, over the cons-cell core forms (`crate::check::core`) the
//! checker produces.
//!
//! The only evaluator: the Rust `Typed` tree it replaced, and the walker over
//! it, are gone. It was built additively one tag at a time against
//! hand-written core forms read straight through the reader — which is why its
//! tests still read that way, and why they are the cheapest place to pin a
//! tag's shape.
//!
//! **The loop that drives it lives in [`core_cps`](super::core_cps)**, over a
//! continuation stack rather than the Rust stack. What stays here is the tag
//! vocabulary ([`Op`]), the leaves that reach a value without a continuation
//! ([`Interp::eval_leaf`]), the readers for a node's fields, and the top level
//! ([`Interp::exec`]).
//!
//! # Roots
//!
//! Every value here is a heap value and [`Heap::cons`] collects whenever the
//! free list is empty, so anything the evaluator is holding *in a Rust local*
//! is invisible to the collector unless it is rooted. Two things are rooted for
//! the whole of an [`eval_core`](super::Interp::eval_core) call, in two
//! dedicated slots the loop overwrites in place rather than pushing onto:
//!
//! - the **environment**, which the loop replaces as it descends;
//! - the **form**, so a body reached by a tail jump stays reachable even when
//!   it belongs to a different tree than the one the caller rooted.
//!
//! Overwriting (`set_root`) rather than pushing is what keeps a tail loop from
//! growing the root stack without bound. Continuation frames push *above*
//! those two slots, and are truncated back down to them as they resume.
//!
//! # Tail positions
//!
//! `if` branches, a `let` body's last form and a call's body are *jumps*, not
//! recursive calls: the loop moves on to the next form without pushing a
//! frame. The Rust-tree evaluator this replaced special-cased only `if`, to
//! survive long `cond` chains; here every tail position is constant-stack, so
//! mutual recursion through a tail call is too.
//!
//! # Environment
//!
//! ```text
//! env   : (frame ...)          a cons list, innermost frame first
//! frame : ((SYM . CELL) ...)   an assoc list
//! CELL  : Value::Boxed(BoxedObj::Cell)
//! ```
//!
//! Lookup compares interned [`SymRef`]s, so it is a `u32` comparison rather
//! than the `String` comparison `Env`'s `Vec<(String, Slot)>` needs. Within a
//! frame the *most recently added* binding is found first, matching
//! `env_get`'s `.rev()`.

use std::cell::RefCell;
use std::rc::Rc;

use typelisp_mem::{wk, BoxId, Heap, RootScope, SymRef, Value};

use crate::check::core;
use crate::check::repr::Repr;
use crate::eval::value::EvalError;
use crate::MacroLambda;

use super::scope;
use super::{EnumDef, FnDef, Interp};

/// A core form's operator, resolved from its tag symbol.
///
/// Every vocabulary tag lives here — `tests/core_vocabulary_test.rs` holds the
/// vocabulary closed, and `a_tag_with_no_evaluation_names_itself` holds the
/// other side: anything *off* it is deliberately absent, so reaching one is an
/// internal error naming the tag rather than a silent mis-evaluation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Op {
    Int,
    Float,
    Bignum,
    Ratio,
    Char,
    Bool,
    Str,
    Sym,
    Unit,
    Var,
    Let,
    If,
    Call,
    Panic,
    Construct,
    FieldGet,
    FieldSet,
    Match,
    Set,
    Loop,
    Break,
    Return,
    Block,
    ReturnFrom,
    Catch,
    Throw,
    UnwindProtect,
    Lambda,
    Labels,
    Apply,
    Quote,
    DynNew,
    DynUpcast,
    DynValue,
    Global,
    SetGlobal,
    Assoc,
    DynCall,
    FnRef,
    MethodRef,
    CompileFn,
    Trace,
    Untrace,
    Step,
    DisassembleFn,
    Go,
    Select,
}

impl Op {
    /// The single dispatch point from a tag to an operator.
    ///
    /// A tag is a symbol, so recognizing one is an identity test on its
    /// [`SymRef`] — the same `eq` CL compares symbols with, never a comparison
    /// of the names behind them. Every tag is in `BUILTIN_SYMBOLS`, so the
    /// constants below mean the same thing in every heap.
    ///
    /// A tag missing from that table would arrive with some id past the
    /// built-in range and match nothing, which is the internal error the
    /// caller reports; `tests/core_vocabulary_test.rs` holds the vocabulary
    /// closed from the other side.
    pub(super) fn from_sym(tag: SymRef) -> Option<Op> {
        Some(match tag.well_known() {
            wk::INT_ANY_WIDTH => Op::Int,
            wk::FLOAT_ANY_WIDTH => Op::Float,
            wk::BIGNUM => Op::Bignum,
            wk::RATIO => Op::Ratio,
            wk::CHAR => Op::Char,
            wk::BOOL => Op::Bool,
            wk::STR => Op::Str,
            wk::SYM => Op::Sym,
            wk::UNIT => Op::Unit,
            wk::VAR => Op::Var,
            wk::LET => Op::Let,
            wk::IF => Op::If,
            wk::CALL => Op::Call,
            wk::PANIC => Op::Panic,
            wk::CONSTRUCT => Op::Construct,
            wk::FIELD_GET => Op::FieldGet,
            wk::FIELD_SET => Op::FieldSet,
            wk::MATCH => Op::Match,
            wk::SET => Op::Set,
            wk::LOOP => Op::Loop,
            wk::BREAK => Op::Break,
            wk::RETURN => Op::Return,
            wk::BLOCK => Op::Block,
            wk::RETURN_FROM => Op::ReturnFrom,
            wk::CATCH => Op::Catch,
            wk::THROW => Op::Throw,
            wk::UNWIND_PROTECT => Op::UnwindProtect,
            wk::LAMBDA => Op::Lambda,
            wk::LABELS => Op::Labels,
            wk::APPLY => Op::Apply,
            wk::QUOTE => Op::Quote,
            wk::DYN_NEW => Op::DynNew,
            wk::DYN_UPCAST => Op::DynUpcast,
            wk::DYN_VALUE => Op::DynValue,
            wk::GLOBAL => Op::Global,
            wk::SET_GLOBAL => Op::SetGlobal,
            wk::ASSOC => Op::Assoc,
            wk::DYN_CALL => Op::DynCall,
            wk::FNREF => Op::FnRef,
            wk::METHODREF => Op::MethodRef,
            wk::COMPILE_FN => Op::CompileFn,
            wk::TRACE => Op::Trace,
            wk::UNTRACE => Op::Untrace,
            wk::STEP => Op::Step,
            wk::DISASSEMBLE_FN => Op::DisassembleFn,
            wk::GO => Op::Go,
            wk::SELECT => Op::Select,
            _ => return None,
        })
    }
}

/// What a person answered at a `step` prompt.
pub(super) enum StepCmd {
    /// Stop again at the next call inside this one.
    Into,
    /// Run this call to completion, then stop at the following one.
    Over,
    /// Stop asking for the rest of this `(step ...)`.
    Continue,
    /// Abandon the computation.
    Quit,
}

impl Interp {
    /// Evaluate the core form `form` in environment `env`.
    ///
    /// `env` may be [`Value::Empty`] for a form with no free variables. Both
    /// arguments are rooted here for the duration, so a caller holding them
    /// only in a Rust local is safe; a caller that *keeps* them past the call
    /// must root them itself.
    ///
    /// **The evaluation itself now happens in `core_cps`**, over a continuation
    /// stack rather than the Rust stack. This entry point is kept so the ~50
    /// call sites below (and `Interp::exec`, and the re-entry from compiled
    /// code) do not each have to change; the recursive body underneath it is
    /// dead except for the leaf tags `step_cps` still routes here, and comes
    /// out with the rest of the move.
    pub(crate) fn eval_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        self.eval_cps(heap, form, env)
    }

    /// Evaluate a *leaf* core form — one with no subexpression, so it reaches
    /// its value without a continuation.
    ///
    /// `step_cps` routes exactly these tags here; every other tag needs a
    /// frame and never leaves `core_cps`. A non-leaf tag arriving here is an
    /// internal error rather than a fallback, and cannot happen by omission:
    /// `step_cps` matches on `Op` with no catch-all, so adding a tag without
    /// giving it a frame fails to build.
    pub(super) fn eval_leaf(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let tag = match heap.car(form) {
            Ok(Value::Symbol(id)) => id,
            _ => return Err(EvalError::Internal(format!("eval: not a core form: {}", core::print(heap, form)))),
        };
        let op = match Op::from_sym(tag) {
            Some(op) => op,
            None => {
                let name = heap.symbol_name(tag).to_string();
                return Err(EvalError::Internal(format!("eval: no evaluation for core form `{}`", name)));
            }
        };

        match op {
            // ---- literals ------------------------------------------------
            //
            // The numeric and string literals carry their value as a heap box
            // built at check time, and each evaluation allocates a *fresh* one
            // rather than handing back the stored box. That is not caution: it
            // is what keeps `eq` an identity test. `Heap::alloc_string`
            // deliberately does not dedupe, so two evaluations of the same
            // literal have always produced distinct values (see `str_rt`), and
            // returning the checker's single stored box would silently make
            // them `eq`.
            Op::Int | Op::Char | Op::Bool | Op::Sym => self.literal_field(heap, form),
            Op::Unit => Ok(Value::Empty),
            // A float literal is re-boxed rather than handed straight back,
            // so two literals of the same number are not `eq`. Each width
            // re-boxes into its own kind: widening an `f32` literal here
            // would be the literal quietly changing type.
            Op::Float => {
                let v = match self.literal_field(heap, form)? {
                    Value::Boxed(id) if heap.is_f32(id) => {
                        let f = heap.f32_value(id);
                        heap.alloc_f32(f)
                    }
                    Value::Boxed(id) if heap.is_f64(id) => {
                        let f = heap.f64_value(id);
                        heap.alloc_f64(f)
                    }
                    other => return Err(EvalError::Internal(format!("eval: (float ..) holds {:?}", other))),
                };
                Ok(v)
            }
            Op::Bignum => {
                let n = match self.literal_field(heap, form)? {
                    Value::Boxed(id) if heap.is_bignum(id) => heap.bignum_value(id).clone(),
                    other => return Err(EvalError::Internal(format!("eval: (bignum ..) holds {:?}", other))),
                };
                Ok(heap.alloc_bignum(n))
            }
            Op::Ratio => {
                let r = match self.literal_field(heap, form)? {
                    Value::Boxed(id) if heap.is_ratio(id) => heap.ratio_value(id).clone(),
                    other => return Err(EvalError::Internal(format!("eval: (ratio ..) holds {:?}", other))),
                };
                Ok(heap.alloc_ratio(r))
            }
            Op::Str => {
                let s = match self.literal_field(heap, form)? {
                    Value::Str(id) => heap.string(id).to_string(),
                    other => return Err(EvalError::Internal(format!("eval: (str ..) holds {:?}", other))),
                };
                Ok(heap.alloc_string(s))
            }

            // ---- variables -----------------------------------------------
            Op::Var => {
                let sym = self.sym_field(heap, form, 0, "var")?;
                match env_lookup(heap, env, sym) {
                    Some(cell) => cell_value(heap, cell),
                    None => Err(EvalError::Unbound(heap.symbol_name(sym).to_string())),
                }
            }

            // ---- closures ------------------------------------------------
            //
            // No JIT attempt. The old evaluator compiled every `lambda` and
            // `labels` sibling at definition time and raised a hard error if
            // the compiler declined — the closure had nowhere else to live,
            // since its body was a Rust AST the box could not hold. With the
            // body a core form there is somewhere, so the JIT is an
            // optimisation the checker's own driver applies rather than a
            // requirement of the representation.
            Op::Lambda => {
                let params = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (lambda ..) has no parameter list".to_string()))?;
                // The return repr, for the bridge — and, once this closure
                // is stored in its box, for a compiled caller applying it
                // (`Interp::apply_interpreted`).
                let ret = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (lambda ..) has no return representation".to_string()))?;
                let body = tail_after(heap, form, 2)?;
                // `alloc_closure` cannot collect (only `cons` does), so the
                // four values need no rooting across it.
                Ok(heap.alloc_closure(params, ret, body, env))
            }

            // ---- quoted data ---------------------------------------------
            //
            // The datum is handed back as it stands. The old evaluator kept a
            // quoted literal as an owned Rust tree (`QuotedSexpr`) and rebuilt
            // it into the heap on *every* evaluation, because a saved function
            // body was invisible to the collector and a live heap pointer in
            // it would have dangled. A core form is itself heap data and is
            // rooted like any other, so the datum can simply be the node's
            // field.
            //
            // That also makes the same `quote` form yield the *same* object
            // each time, where rebuilding gave a fresh copy — which is what CL
            // specifies for a literal, and the reason its consequences are
            // undefined if one is destructively modified.
            Op::Quote => self.literal_field(heap, form),

            // ---- globals -------------------------------------------------
            Op::Global => self.global_core(heap, form),

            // ---- function values and the tools that name them -------------
            Op::FnRef => self.fnref_core(heap, form),
            Op::MethodRef => self.methodref_core(heap, form),
            Op::CompileFn => self.compile_fn_core(heap, form),
            Op::Trace => self.trace_core(heap, form, true),
            Op::Untrace => self.trace_core(heap, form, false),
            Op::DisassembleFn => self.disassemble_fn_core(heap, form),

            other => Err(EvalError::Internal(format!(
                "eval: {:?} is not a leaf; `step_cps` owes it a continuation frame",
                other
            ))),
        }
    }

    /// The block name a `block`/`return-from` node's first field carries.
    ///
    /// A plain `(str "name")` node, not a quoted symbol: the name is settled at
    /// check time (`Checker::block_stack`) and exists at run time only to tell
    /// nested blocks apart, so nothing here needs a symbol's identity.
    pub(super) fn block_name_of(&self, heap: &Heap, form: Value) -> Result<String, EvalError> {
        match core::field(heap, form, 0).and_then(|n| core::field(heap, n, 0)) {
            Some(Value::Str(id)) => Ok(heap.string(id).to_string()),
            other => Err(EvalError::Internal(format!("eval: block name is {:?}", other))),
        }
    }

    /// The symbol a `catch`/`throw` node's first field names.
    ///
    /// Stored as a quoted `Sexpr` symbol datum (`forms::catch_form`), so this
    /// is the same shape `Op::Quote` produces — read back by name because
    /// that is what `EvalError::Throw` carries between the throw site and its
    /// catch, which may be in a different function entirely.
    pub(super) fn throw_tag_of(&self, heap: &Heap, form: Value, who: &str) -> Result<String, EvalError> {
        let quoted = core::field(heap, form, 0)
            .and_then(|q| core::field(heap, q, 0))
            .ok_or_else(|| EvalError::Internal(format!("eval: ({} ..) has no tag", who)))?;
        match quoted {
            Value::Symbol(id) => Ok(heap.symbol_name(id).to_string()),
            other => Err(EvalError::Internal(format!("eval: ({} ..) tag is not a symbol: {:?}", who, other))),
        }
    }

    /// [`Self::dyn_new_core`] from the point its operand has a value.
    ///
    /// Split out so the continuation-stack evaluator, which evaluates the
    /// operand through a frame rather than a recursive call, reaches the same
    /// boxing here rather than a second copy of it.
    pub(super) fn dyn_new_with_value(&self, heap: &mut Heap, form: Value, v: Value) -> Result<Value, EvalError> {
        let concrete_key = match core::field(heap, form, 0) {
            Some(Value::Str(id)) => heap.string(id).to_string(),
            other => return Err(EvalError::Internal(format!("eval: (dyn-new ..) concrete key is {:?}", other))),
        };
        let trait_path = path_field(heap, form, 1, "dyn-new")?;
        let slots = method_list(heap, form, 2, "dyn-new")?;
        let supers = super_list(heap, form, 3)?;

        let mut s = RootScope::new(heap);
        // `alloc_dyn` cannot collect, but compiling a slot below does, and the
        // boxed value is reachable from nothing else meanwhile.
        s.push_root(v);
        let ids = self.register_dyn_box(&concrete_key, &trait_path, &slots, &supers);
        let id = *ids
            .first()
            .ok_or_else(|| EvalError::Internal("eval: (dyn-new ..) registered no vtable".to_string()))?;
        // If some compiled body dispatches on this trait, the box may be about
        // to reach it, and a native call site can only read a *native* entry
        // point out of the table — so any slot still interpreted has to be
        // compiled now, before `publish_vtable` reads the addresses.
        //
        // Not reachable for an ordinary program: a `dyn-call` site's own
        // `((TYPE SYM)...)` table already pulled in every implementation it
        // could dispatch to. What it does not pull in is an implementation
        // whose owner is *generic* — `vector-iter<T>`'s `Iter` methods have no
        // code until specialized, and which specialization is only known here,
        // where a concrete instantiation is boxed. Also reached by an `impl`
        // written after that call site was checked.
        //
        // The borrow is bound to a local so it is definitely released before
        // `compile_function`, which reaches `translate_and_compile` and borrows
        // this same cell mutably.
        let has_native_dispatcher = self.dyn_dispatch_compiled.borrow().contains(&trait_path);
        if has_native_dispatcher {
            for (type_name, method) in &slots {
                if !self.root.borrow().method_compiled(type_name, method) {
                    let target = crate::CompileTarget::Method {
                        type_name: type_name.clone(),
                        method: method.clone(),
                        home: type_name.parent().to_vec(),
                    };
                    (crate::eval::interp::backend_compile_function()?)(self, &mut s, &target)?;
                }
            }
        }
        // Published on every boxing, not only after a compilation: the value
        // may be about to cross into *already*-compiled code that dispatches on
        // it, and this is the only point where such a box comes into existence
        // with no compilation having just happened.
        //
        // Every table, not just this box's own: an upcast of it hands compiled
        // code one of the supertrait tables, and compiled code reads entry
        // points from the compiled tier's copy and nowhere else. (Publishing
        // only `ids[0]` left those empty, which aborted every AOT dispatch
        // through a supertrait table.)
        for id in &ids {
            self.publish_vtable(*id);
        }
        Ok(s.alloc_dyn(id, v))
    }

    /// [`Self::dyn_upcast_core`] from the point its operand has a value —
    /// split for the same reason [`Self::dyn_new_with_value`] is.
    pub(super) fn dyn_upcast_with_value(&self, heap: &mut Heap, form: Value, v: Value) -> Result<Value, EvalError> {
        let to_trait = path_field(heap, form, 0, "dyn-upcast")?;

        let id = match v {
            Value::Boxed(id) if heap.is_dyn(id) => id,
            other => {
                return Err(EvalError::Internal(format!(
                    "eval: (dyn-upcast {} ..): not a trait object: {:?}",
                    to_trait, other
                )))
            }
        };
        let from = heap.dyn_vtable_id(id);
        let trait_id = self.trait_id_for(&to_trait);
        let to = self.dyn_upcasts.borrow().get(&(from, trait_id)).copied().ok_or_else(|| {
            EvalError::Internal(format!(
                "eval: (dyn-upcast {} ..): vtable {} has no supertrait table registered",
                to_trait, from
            ))
        })?;
        // Upcasting to the trait it already is leaves the box alone, so the
        // identity of a `:dyn` value is not disturbed by a no-op view.
        if to == from {
            return Ok(v);
        }
        let inner = heap.dyn_value(id);
        self.publish_vtable(to);
        let mut s = RootScope::new(heap);
        s.push_root(inner);
        Ok(s.alloc_dyn(to, inner))
    }

    /// Bind `argv` to interpreted closure `id`'s parameters, returning the
    /// environment its body runs in and that body's forms.
    ///
    /// The call environment extends the environment the closure *captured*,
    /// not the caller's: that is what makes this lexical scope rather than
    /// dynamic. `call_env` is rooted on `heap` — the caller is inside a
    /// [`RootScope`] that will release it.
    pub(super) fn closure_frame(&self, heap: &mut Heap, id: BoxId, argv: Vec<Value>) -> Result<(Value, Vec<Value>), EvalError> {
        let (params, body, closure_env) = heap.closure_parts(id);
        let names = param_names(heap, params)?;
        if names.len() != argv.len() {
            return Err(EvalError::Internal(format!(
                "eval: arity mismatch: the closure takes {} argument(s), given {}",
                names.len(),
                argv.len()
            )));
        }
        let binds: Vec<(SymRef, Value)> = names.into_iter().zip(argv).collect();
        let call_env = extend_env(heap, &binds, closure_env)?;
        heap.push_root(call_env);
        let body = heap
            .list_to_vec(body)
            .map_err(|e| EvalError::Internal(format!("eval: closure body: {}", e)))?;
        Ok((call_env, body))
    }

    /// Applies interpreted closure `id` to values, all the way to a result.
    ///
    /// For a caller that is *not* the evaluator's own loop —
    /// `Interp::apply_interpreted`, re-entered from compiled code. There is a
    /// native frame waiting on this call, so the body's last form is evaluated
    /// here rather than continued as a tail jump: tail calls are constant-stack
    /// *within* an interpreted chain, and this is a boundary crossing.
    pub(super) fn call_interpreted_closure(&self, heap: &mut Heap, id: BoxId, argv: Vec<Value>) -> Result<Value, EvalError> {
        let mut s = RootScope::new(heap);
        let (call_env, body) = self.closure_frame(&mut s, id, argv)?;
        let mut last = Value::Empty;
        for e in &body {
            last = self.eval_core(&mut s, *e, call_env)?;
        }
        Ok(last)
    }

    /// A one-field literal node's payload.
    fn literal_field(&self, heap: &Heap, form: Value) -> Result<Value, EvalError> {
        core::field(heap, form, 0).ok_or_else(|| EvalError::Internal("eval: literal has no value".to_string()))
    }

    /// Field `i` of `form`, which must be a symbol.
    pub(super) fn sym_field(&self, heap: &Heap, form: Value, i: usize, what: &str) -> Result<SymRef, EvalError> {
        match core::field(heap, form, i) {
            Some(Value::Symbol(id)) => Ok(id),
            other => Err(EvalError::Internal(format!("eval: ({} ..) field {} is not a symbol: {:?}", what, i, other))),
        }
    }

    /// `(global (WRITTEN...) (HOME...) PATH REPR)` — read a `defvar`.
    ///
    /// A global some compiled function reads or writes lives in a permanent GC
    /// root instead of an ordinary cell (`Interp::promote_global`), and must be
    /// read from that same storage here — otherwise an interpreted read could
    /// see a stale value a compiled write already updated, even though both
    /// sides name the same `defvar`.
    fn global_core(&self, heap: &mut Heap, form: Value) -> Result<Value, EvalError> {
        let written = self.name_list(heap, form, 0, "global")?;
        let home = self.name_list(heap, form, 1, "global")?;
        let path = path_field(heap, form, 2, "global")?;
        if let Some(&id) = self.compiled_globals.borrow().get(&path) {
            // `id` is *not* the permanent-root position — see
            // `typelisp_rt::global_new`'s doc comment (an `Option`/`Result`
            // global's own heap-referencing field pushes its own extra
            // permanent root during encoding, desyncing the two) — so
            // `global_perm_idx` resolves it the same way `rt_global_get` does.
            let perm_idx = typelisp_rt::global_perm_idx(id)
                .ok_or_else(|| EvalError::Internal(format!("global \"{}\": unknown compiled id {}", path, id)))?;
            // No decode: with one value world left, a stored word *is* the
            // value (see `eval_builtin_method`'s doc comment).
            return Ok(heap.permanent_root(perm_idx));
        }
        self.resolve_global_named(&home, &written, &path)
            .map(|slot| slot.get(heap))
            .ok_or_else(|| EvalError::Unbound(path.to_string()))
    }

    /// `(fnref (WRITTEN...) (HOME...) PATH (PARAM-REPR...))` — a named free
    /// function as a value.
    ///
    /// A user function becomes an ordinary interpreted closure over its own
    /// parameters and body with an *empty* captured environment: a top-level
    /// `defun` has nothing to capture. A built-in becomes a `BoxedObj::Builtin`
    /// naming it, which `apply_core` dispatches by name.
    fn fnref_core(&self, heap: &mut Heap, form: Value) -> Result<Value, EvalError> {
        let written = self.name_list(heap, form, 0, "fnref")?;
        let home = self.name_list(heap, form, 1, "fnref")?;
        let path = path_field(heap, form, 2, "fnref")?;
        match self.resolve_fn_named(&home, &written, &path) {
            Some(f) => self.reify(heap, &f),
            // A free built-in's own result key is not carried: the ones that
            // build a box (`parse-int`'s `Result`) spell it from their own
            // fixed signature in `typelisp_rt`, so there is nothing for the
            // reference site to say. `""` records that absence rather than a
            // key nobody wrote.
            None => Ok(heap.alloc_builtin_fn(None, path.last_segment(), "")),
        }
    }

    /// `(methodref PATH SYM (HOME...) (PARAM-REPR...))` — [`Self::fnref_core`]
    /// for a method.
    fn methodref_core(&self, heap: &mut Heap, form: Value) -> Result<Value, EvalError> {
        let type_name = path_field(heap, form, 0, "methodref")?;
        let method = sym_field(heap, form, 1, "methodref")?;
        let home = self.name_list(heap, form, 2, "methodref")?;
        let f = self.root.borrow().resolve_method(&home, &type_name, &method);
        match f {
            Some(f) => self.reify(heap, &f),
            None => {
                let recv = crate::types::intern_path_id(heap, &type_name);
                let ret_key = str_field(heap, form, 4, "methodref")?;
                Ok(heap.alloc_builtin_fn(Some(recv), &method, &ret_key))
            }
        }
    }

    /// `(compile-fn (fn (WRITTEN...) (HOME...) PATH))` or
    /// `(compile-fn (method PATH SYM (HOME...)))` — the `(compile name)` form.
    ///
    /// The nested `(fn ..)`/`(method ..)` payload is read back into the
    /// `CompileTarget` the compile driver takes. That type is a *resolution*,
    /// not an AST node, which is why it outlived the typed AST.
    fn compile_fn_core(&self, heap: &mut Heap, form: Value) -> Result<Value, EvalError> {
        let payload = core::field(heap, form, 0)
            .ok_or_else(|| EvalError::Internal("eval: (compile-fn ..) has no target".to_string()))?;
        let target = self.compile_target(heap, payload, "compile-fn")?;
        (crate::eval::interp::backend_compile_function()?)(self, heap, &target)
    }

    /// One `(fn ..)`/`(method ..)` payload read back into the `CompileTarget`
    /// it was built from — shared by every form that names a single body
    /// (`compile`, `trace`, `untrace`, `disassemble`), which is why the
    /// payload shape is shared too.
    fn compile_target(&self, heap: &mut Heap, payload: Value, who: &str) -> Result<crate::CompileTarget, EvalError> {
        Ok(match core::op_sym(heap, payload).map(|s| s.well_known()) {
            Some(wk::FN) => {
                let written = self.name_list(heap, payload, 0, who)?;
                let home = self.name_list(heap, payload, 1, who)?;
                let resolved = path_field(heap, payload, 2, who)?;
                crate::CompileTarget::Fn(crate::check::Ref { written, home, resolved })
            }
            Some(wk::METHOD) => {
                let type_name = path_field(heap, payload, 0, who)?;
                let method = sym_field(heap, payload, 1, who)?;
                let home = self.name_list(heap, payload, 2, who)?;
                crate::CompileTarget::Method { type_name, method, home }
            }
            other => {
                return Err(EvalError::Internal(format!(
                    "eval: ({} ..) target is `{:?}`, expected `fn` or `method`",
                    who, other
                )))
            }
        })
    }

    /// The definition a resolved [`crate::CompileTarget`] names.
    ///
    /// Goes through the *same* two lookups `compile::driver::compile_function`
    /// does, and for the same reason: a `Fn` target's `written`+`home` walk
    /// can land on a different definition than its `resolved` path names, so
    /// the answer has to be the thing actually found rather than the name
    /// written down. Which is also why `trace` reads its key off the
    /// [`FnDef`] instead of rebuilding one from the target.
    fn target_def(&self, target: &crate::CompileTarget) -> Option<Rc<FnDef>> {
        match target {
            crate::CompileTarget::Fn(r) => self.resolve_fn_ref(r),
            crate::CompileTarget::Method { type_name, method, home } => {
                self.root.borrow().resolve_method(home, type_name, method)
            }
        }
    }

    /// `(trace TARGET...)` and `(untrace TARGET...)` — CLHS 25.2's pair.
    ///
    /// Both return the set of names traced *after* the operation, as a
    /// `Sexpr` list of symbols in sorted order. That makes `(trace)` with no
    /// arguments CL's "just tell me what is traced", `(untrace)` with none
    /// the documented untrace-everything, and the two answers comparable.
    ///
    /// Tracing a definition that already has a compiled body is allowed and
    /// says so: the hook is in [`Self::enter`], which is above the
    /// compiled/interpreted split, so calls *into* it are still seen — but a
    /// call made from inside other compiled code never reaches `enter` and is
    /// invisible. SBCL says the same thing about its own local calls, and a
    /// trace that quietly showed half the calls would be worse than one that
    /// warns.
    fn trace_core(&self, heap: &mut Heap, form: Value, on: bool) -> Result<Value, EvalError> {
        let payloads = core::fields(heap, form)
            .map_err(|e| EvalError::Internal(format!("eval: ({} ..): {}", if on { "trace" } else { "untrace" }, e)))?;
        let who = if on { "trace" } else { "untrace" };
        if !on && payloads.is_empty() {
            self.traced.borrow_mut().clear();
        }
        for payload in payloads {
            let target = self.compile_target(heap, payload, who)?;
            let def = self.target_def(&target).ok_or_else(|| {
                EvalError::Internal(format!("{}: a name that resolved at check time does not resolve here", who))
            })?;
            if on {
                if def.compiled.borrow().is_some() {
                    let note = format!(
                        "; note: `{}` has a compiled body — calls to it from other compiled code are not traced\n",
                        def.name
                    );
                    self.trace_write(heap, &note);
                }
                self.traced.borrow_mut().insert(def.name.clone());
            } else {
                self.traced.borrow_mut().remove(&def.name);
            }
        }
        let mut names: Vec<String> = self.traced.borrow().iter().cloned().collect();
        names.sort();
        self.trace_armed.set(!names.is_empty());
        // Symbols are interned pointers rather than cells, so the `Vec` holding
        // them is not something the collector has to see; the list built from
        // them is, and `core::list` roots as it conses.
        let syms: Vec<Value> = names.iter().map(|n| heap.intern_symbol(n)).collect();
        core::list(heap, &syms).map_err(heap_err)
    }

    /// Call `f` with already-evaluated `argv`: through its compiled body if it
    /// has one, otherwise by tree-walking.
    ///
    /// **The one way to call a registered function.** Everything that decides
    /// between the two halves lives here and only here: the compiled body, the
    /// `trace`/`step` hook, the interpreted fallback. `Self::apply` is the
    /// interpreted half, deliberately module-private — reaching past this and
    /// calling it directly is how three call sites silently ignored `(compile
    /// f)` until 2026-09-12.
    ///
    /// The compiled check comes first so a later recompile would naturally take
    /// precedence. `compile_function` never populates `compiled` without going
    /// through `compiled_fn_body`, which requires `sig` — so the `expect` is an
    /// internal invariant, not a user-reachable error.
    pub fn enter(&self, heap: &mut Heap, f: &Rc<FnDef>, argv: Vec<Value>) -> Result<Value, EvalError> {
        // Two `Cell` loads on the path every call takes. Anything more — a
        // `RefCell` borrow, a hash lookup — belongs behind them, which is why
        // the set membership is tested only once one of the two is armed.
        let stepping = self.stepping.get() && self.trace_depth.get() <= self.step_quiet_depth.get();
        if stepping || (self.trace_armed.get() && self.traced.borrow().contains(&f.name)) {
            return self.enter_watched(heap, f, argv, stepping);
        }
        self.enter_plain(heap, f, argv)
    }

    /// [`Self::enter`] with nothing watching — the whole of it before `trace`
    /// existed, kept as its own function so the hook above is one branch on a
    /// `Cell` and not a second copy of the compiled/interpreted dispatch.
    fn enter_plain(&self, heap: &mut Heap, f: &Rc<FnDef>, argv: Vec<Value>) -> Result<Value, EvalError> {
        let compiled = f.compiled.borrow().clone();
        if let Some(compiled) = compiled {
            let sig = f.sig.as_ref().expect("a compiled function always has a type signature");
            return self.call_compiled(heap, compiled.as_ref(), &argv, &sig.0, &sig.1);
        }
        self.apply(heap, f, argv)
    }

    /// One call to a traced function, reported around it in CL's own shape:
    /// `  0: (fact 3)` going in and `  0: fact returned 6` coming out, two
    /// spaces of indentation per open frame.
    ///
    /// The depth is restored *before* the return is reported, so the two lines
    /// of one call line up. It is restored on the failure path too, and the
    /// failure itself is reported: an unwind past a traced frame is exactly
    /// the moment a trace is most worth having.
    fn enter_watched(
        &self,
        heap: &mut Heap,
        f: &Rc<FnDef>,
        argv: Vec<Value>,
        stepping: bool,
    ) -> Result<Value, EvalError> {
        let depth = self.trace_call_entry(heap, f, &argv, stepping)?;
        let result = self.enter_plain(heap, f, argv);
        self.trace_call_exit(heap, &f.name, depth, result.as_ref().copied());
        result
    }

    /// Reports a watched call *going in* — `  0: (fact 3)` — asks the step
    /// prompt when `stepping`, and bumps the depth for the callee. Returns the
    /// depth the call was reported at: its return line is written at the same
    /// one, and it is what [`Self::trace_call_exit`] restores.
    ///
    /// Shared by the two ways into a body — [`Self::enter_watched`]'s Rust
    /// frame and the continuation machine's `enter_fn`
    /// ([`core_cps`](super::core_cps)) — so `trace` and `step` say the same
    /// thing whichever one a call arrives through.
    pub(super) fn trace_call_entry(
        &self,
        heap: &mut Heap,
        f: &Rc<FnDef>,
        argv: &[Value],
        stepping: bool,
    ) -> Result<usize, EvalError> {
        let depth = self.trace_depth.get();
        let rendered: Vec<String> = argv.iter().map(|a| self.trace_render(heap, *a)).collect();
        let call = if rendered.is_empty() {
            format!("({})", f.name)
        } else {
            format!("({} {})", f.name, rendered.join(" "))
        };
        self.trace_line(heap, depth, &call);
        if stepping {
            match self.step_prompt(heap) {
                StepCmd::Into => {}
                // Quiet *below* this frame: the command is "run this call",
                // and the frame it was given at is the one to ask at again.
                StepCmd::Over => self.step_quiet_depth.set(depth),
                StepCmd::Continue => self.stepping.set(false),
                StepCmd::Quit => return Err(EvalError::Panic("step: aborted".to_string())),
            }
        }
        self.trace_depth.set(depth + 1);
        Ok(depth)
    }

    /// Reports a watched call *coming out* — `  0: fact returned 6`, or the
    /// exit that unwound past it — and puts back what
    /// [`Self::trace_call_entry`] moved.
    ///
    /// The depth is restored *before* the return is reported, so the two lines
    /// of one call line up. An unwind is reported rather than silent: it is
    /// exactly the moment a trace is most worth having.
    pub(super) fn trace_call_exit(&self, heap: &mut Heap, name: &str, depth: usize, outcome: Result<Value, &EvalError>) {
        self.trace_depth.set(depth);
        if self.step_quiet_depth.get() == depth {
            self.step_quiet_depth.set(usize::MAX);
        }
        match outcome {
            Ok(v) => {
                let text = self.trace_render(heap, v);
                self.trace_line(heap, depth, &format!("{} returned {}", name, text));
            }
            Err(e) => self.trace_line(heap, depth, &format!("{} exited non-locally: {}", name, e)),
        }
    }

    /// Asks what to do at a stepped call, and reads the answer from standard
    /// input.
    ///
    /// Four commands, and a bare newline means the commonest one. End of
    /// input means `continue` rather than `quit`: input running out is not a
    /// decision to abandon the computation, and looping on it would hang.
    ///
    /// The prompt goes to `*trace-output*` with everything else `step` says,
    /// and is *not* newline-terminated — `write_str` on a stdout-backed
    /// stream writes through immediately, so it is visible before the read
    /// blocks.
    pub(super) fn step_prompt(&self, heap: &mut Heap) -> StepCmd {
        self.trace_write(heap, "step [s]tep-into [n]ext [c]ontinue [q]uit> ");
        self.trace_flush(heap);
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) | Err(_) => return StepCmd::Continue,
            Ok(_) => {}
        }
        match line.trim() {
            "" | "s" | "step" => StepCmd::Into,
            "n" | "next" => StepCmd::Over,
            "c" | "continue" => StepCmd::Continue,
            "q" | "quit" => StepCmd::Quit,
            other => {
                self.trace_write(heap, &format!("; `{}` is not a step command; stepping into\n", other));
                StepCmd::Into
            }
        }
    }

    /// Writes one trace line, indented for `depth` and numbered the way CL
    /// numbers them.
    ///
    /// The destination is `*trace-output*`, which is what CL specifies and
    /// what `time` already writes to. The global holds a `standard-stream`,
    /// whose single field is the native handle — the one place outside the
    /// prelude that reads it, and the reason it is read *defensively*: an
    /// `Interp` with no prelude loaded (a unit test) has no such global, and
    /// its trace still has to go somewhere. That somewhere is the printer's
    /// own stdout, which is where `*trace-output*` points anyway by default.
    pub(super) fn trace_line(&self, heap: &mut Heap, depth: usize, body: &str) {
        let text = format!("{:width$}{}: {}\n", "", depth, body, width = 2 + depth * 2);
        self.trace_write(heap, &text);
    }

    /// Puts `text` on `*trace-output*` verbatim — [`Self::trace_line`]'s
    /// destination half, also used for the notes `trace` itself emits.
    fn trace_write(&self, heap: &mut Heap, text: &str) {
        self.write_stream_global(heap, "*trace-output*", text)
    }

    /// Pushes whatever is buffered for `*trace-output*` out to the OS.
    ///
    /// The step prompt deliberately ends without a newline, and stdout is
    /// line-buffered when it is a terminal — so without this the prompt would
    /// still be sitting in the buffer while `read_line` blocks, and a stepping
    /// session would look hung rather than waiting. Only the stream path needs
    /// it: the fallback in [`Self::write_stream_global`] goes through the
    /// printer's `write_stdout`, which flushes for exactly this reason.
    fn trace_flush(&self, heap: &mut Heap) {
        if let Some(h) = self.stream_global(heap, "*trace-output*") {
            let _ = typelisp_rt::stream::with_streams(|t| t.finish_output(h));
        }
    }

    /// Writes `text` to whatever stream the global `name` holds.
    ///
    /// This is how the tool layer honours CL's stream variables from Rust:
    /// the global holds a `standard-stream`, whose single field is the native
    /// handle. It is the only place outside the prelude that reads that
    /// field, and it reads it *defensively* — an `Interp` with no prelude
    /// loaded (a unit test) has no such global, and the output still has to go
    /// somewhere. That somewhere is the printer's own stdout, which is where
    /// these variables point by default anyway.
    fn write_stream_global(&self, heap: &mut Heap, name: &str, text: &str) {
        match self.stream_global(heap, name) {
            Some(h) => {
                let _ = typelisp_rt::stream::with_streams(|t| t.write_str(h, text));
            }
            None => {
                let opts = typelisp_print::runtime::current_opts(heap);
                let mut out = typelisp_print::pprint::Out::new();
                for c in text.chars() {
                    out.push(c);
                }
                let _ = typelisp_print::runtime::emit(heap, out, false, &opts);
            }
        }
    }

    /// The native stream handle a stream-valued global holds, if the prelude
    /// that defines it is loaded and it still holds a stream-shaped value.
    fn stream_global(&self, heap: &mut Heap, name: &str) -> Option<i64> {
        let v = self.global_value(heap, &crate::Path::root(name))?;
        let Value::Boxed(id) = v else { return None };
        if !heap.is_struct(id) || heap.struct_field_count(id) < 1 {
            return None;
        }
        match heap.struct_field(id, 0) {
            Value::Int(h) => Some(h),
            _ => None,
        }
    }

    /// `(disassemble-fn TARGET LLVM)` — the `(disassemble name [llvm])` form.
    ///
    /// Prints and answers `()`, which is CL's shape: `disassemble` is a thing
    /// you look at, not a value you compute with. It goes to
    /// `*standard-output*`, again as CL specifies, through the same handle
    /// `trace` writes its own reports to.
    fn disassemble_fn_core(&self, heap: &mut Heap, form: Value) -> Result<Value, EvalError> {
        let payload = core::field(heap, form, 0)
            .ok_or_else(|| EvalError::Internal("eval: (disassemble-fn ..) has no target".to_string()))?;
        let llvm_ir = bool_field(heap, form, 1, "disassemble-fn")?;
        let target = self.compile_target(heap, payload, "disassemble-fn")?;
        let text = (crate::eval::interp::backend_disassemble_function()?)(self, heap, &target, llvm_ir)?;
        self.write_stream_global(heap, "*standard-output*", &text);
        if !text.ends_with('\n') {
            self.write_stream_global(heap, "*standard-output*", "\n");
        }
        Ok(Value::Empty)
    }

    /// One value as `~s` would print it — the same printer `format` uses, so
    /// a `print-object` method and the `*print-*` control variables apply to
    /// a trace line exactly as they do to a program's own output.
    ///
    /// Built and laid out here rather than emitted: `emit` would merge into an
    /// open `pprint-logical-block` session, and a trace line that arrives in
    /// the middle of a program's own pretty-printed document belongs to
    /// neither.
    pub(super) fn trace_render(&self, heap: &mut Heap, v: Value) -> String {
        self.install_print_hooks();
        // `core::list` roots `v` across the one `cons` it takes to wrap it, and
        // rooting the finished list keeps `v` reachable through the format run,
        // which allocates.
        match core::list(heap, &[v]) {
            Ok(list) => {
                heap.push_root(list);
                let opts = typelisp_print::runtime::current_opts(heap);
                let text = typelisp_print::runtime::build_format(heap, "~s", list)
                    .map(|out| typelisp_print::format::finish(out, &opts));
                heap.pop_root();
                text.unwrap_or_else(|_| "#<unprintable>".to_string())
            }
            Err(_) => "#<unprintable>".to_string(),
        }
    }

    /// A registered function as a closure value, capturing nothing.
    ///
    /// A `defun`/`defmethod` body is closed over its parameters alone, so the
    /// captured environment is empty — which is what lets a named function be
    /// reified without any of `lambda`'s capture analysis.
    ///
    /// The parameter list is rebuilt in the *lambda* shape, `((SYM REPR) ...)`,
    /// because that is what a closure box holds and what `apply_core` reads back
    /// (`param_names` takes each entry's `car`). `FnDef` keeps names and
    /// representations apart, so they are zipped back together here; a function
    /// registered without a signature has no representations to zip, and unit
    /// stands in — an interpreted apply is uniform over `Value` and reads none
    /// of them, while a *compiled* caller does read them
    /// (`Interp::apply_interpreted`). A `defmacro` used to be the case with no
    /// signature; it has one now (all-`Sexpr`, which is what the checker
    /// checked its body under), so its parameters zip like any other.
    pub(crate) fn reify(&self, heap: &mut Heap, f: &Rc<FnDef>) -> Result<Value, EvalError> {
        let mut s = RootScope::new(heap);
        let reprs = f.sig.as_ref().map(|(ps, _)| ps.as_slice()).unwrap_or(&[]);

        let mut ps = core::Items::new(&mut s);
        for (i, name) in f.params.iter().enumerate() {
            let sym = ps.heap().intern_symbol(name);
            let repr = match reprs.get(i) {
                Some(r) => r.write(ps.heap()).map_err(heap_err)?,
                None => Value::Empty,
            };
            let one = core::list(ps.heap(), &[sym, repr]).map_err(heap_err)?;
            ps.push(one);
        }
        let params = ps.finish_list().map_err(heap_err)?;
        s.push_root(params);

        let ret = match f.sig.as_ref() {
            Some((_, r)) => r.write(&mut s).map_err(heap_err)?,
            None => Value::Empty,
        };
        s.push_root(ret);
        let body = core::list(&mut s, &f.body).map_err(heap_err)?;
        s.push_root(body);
        Ok(s.alloc_closure(params, ret, body, Value::Empty))
    }

    /// [`Interp::resolve_fn_ref`] from a core form's own three name fields.
    ///
    /// The `written`/`home` ancestor walk first, `resolved` only as the
    /// fallback — see `resolve_fn_ref`'s doc comment for why both exist.
    pub(super) fn resolve_fn_named(&self, home: &[String], written: &[String], resolved: &crate::Path) -> Option<Rc<FnDef>> {
        self.root.borrow().resolve_fn(home, written).or_else(|| self.root.borrow().get_fn(resolved))
    }

    /// [`Self::resolve_fn_named`]'s twin for `global`/`set-global`.
    pub(super) fn resolve_global_named(
        &self,
        home: &[String],
        written: &[String],
        resolved: &crate::Path,
    ) -> Option<crate::eval::value::Slot> {
        self.root.borrow().resolve_global(home, written).or_else(|| self.root.borrow().get_global(resolved))
    }

    /// A field holding a list of symbols (`written`, `home`), as strings.
    pub(super) fn name_list(&self, heap: &Heap, form: Value, i: usize, what: &str) -> Result<Vec<String>, EvalError> {
        let field = core::field(heap, form, i)
            .ok_or_else(|| EvalError::Internal(format!("eval: ({} ..) has no field {}", what, i)))?;
        let segs = heap
            .list_to_vec(field)
            .map_err(|e| EvalError::Internal(format!("eval: ({} ..) field {}: {}", what, i, e)))?;
        segs.iter()
            .map(|v| match v {
                Value::Symbol(id) => Ok(heap.symbol_name(*id).to_string()),
                other => Err(EvalError::Internal(format!("eval: ({} ..) field {} holds {:?}", what, i, other))),
            })
            .collect()
    }
}

/// Link a frame holding `binds` onto `env`.
///
/// Every value in `binds` must already be rooted by the caller: this
/// allocates a cell and two conses per binding, and any of them can collect.
/// The result is *not* rooted — the caller roots it, or hands it straight to
/// something that does.
pub(crate) fn extend_env(heap: &mut Heap, binds: &[(SymRef, Value)], env: Value) -> Result<Value, EvalError> {
    // No bindings, no frame. `progn` is `(let () ...)` and a `_` match arm
    // binds nothing, so this is the common case, not an edge one: consing an
    // empty frame for each would put an allocation (and so a possible
    // collection) on a path with no reason to have one, and lengthen the
    // chain every lookup walks.
    if binds.is_empty() {
        return Ok(env);
    }
    let mut s = RootScope::new(heap);
    let mut frame = Value::Empty;
    for (sym, v) in binds {
        // The cell allocation cannot itself collect (only `cons` does), but
        // the two conses below can, so each intermediate is rooted before the
        // next allocation rather than after. `frame` is rooted by the previous
        // iteration, and is `Empty` — which needs no root — on the first.
        let cell = s.alloc_cell_unregistered(*v);
        s.push_root(cell);
        let pair = core::pair(&mut s, Value::Symbol(*sym), cell).map_err(heap_err)?;
        s.push_root(pair);
        // core-build-ok: an environment frame, not a core form — `core::list`
        // wants its elements up front, and these are produced one cell at a
        // time. Each intermediate is rooted before the next allocation, as the
        // comment above sets out.
        frame = s.cons(pair, frame).map_err(heap_err)?;
        s.push_root(frame);
    }
    // core-build-ok: pushing the finished frame onto the environment chain;
    // both halves are rooted above.
    s.cons(frame, env).map_err(heap_err)
}

/// Match `pat` against `v`, returning the bindings it makes, or `None` if it
/// does not match.
///
/// Each bound value is rooted as it is collected, and stays rooted until the
/// caller's scope closes. That is not belt-and-braces: the `Path` pattern
/// builds its binding *fresh* rather than pointing into the scrutinee, so a
/// later sub-pattern's allocation would collect it.
pub(super) fn match_core_pattern(
    it: &Interp,
    heap: &mut Heap,
    env: Value,
    pat: Value,
    v: Value,
) -> Result<Option<Vec<(SymRef, Value)>>, EvalError> {
    let tag = match heap.car(pat) {
        Ok(Value::Symbol(id)) => heap.symbol_name(id).to_string(),
        _ => return Err(EvalError::Internal(format!("eval: not a pattern: {}", core::print(heap, pat)))),
    };
    match tag.as_str() {
        "pat-wild" => Ok(Some(Vec::new())),
        "pat-bind" => match core::field(heap, pat, 0) {
            Some(Value::Symbol(sym)) => {
                heap.push_root(v);
                Ok(Some(vec![(sym, v)]))
            }
            other => Err(EvalError::Internal(format!("eval: (pat-bind ..) names {:?}", other))),
        },
        "pat-lit" => {
            let lit = core::field(heap, pat, 0)
                .ok_or_else(|| EvalError::Internal("eval: (pat-lit ..) has no literal".to_string()))?;
            // Read rather than evaluated. A literal pattern only ever holds an
            // `int`/`bool`/`char` — the three the language has pattern syntax
            // for — and those are exactly the values whose `==` is what a
            // pattern means. A string would compile to `Value::Str`, whose
            // equality is *identity*, so it would silently never match; that
            // is why anything else is refused here instead of compared.
            let want = match core::op_sym(heap, lit).map(|s| s.well_known()) {
                Some(wk::INT_ANY_WIDTH) | Some(wk::BOOL) | Some(wk::CHAR) => core::field(heap, lit, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (pat-lit ..) literal has no value".to_string()))?,
                _ => {
                    return Err(EvalError::Internal(format!(
                        "eval: (pat-lit ..) holds {}, which has no by-value equality",
                        core::print(heap, lit)
                    )))
                }
            };
            Ok(if want == v { Some(Vec::new()) } else { None })
        }
        // `(pat-guard SYM TEST)` — matches iff `TEST` (a checked `bool`
        // expression that reads the value under test through `SYM`) is true.
        // This is what a `string`/`f64`/`symbol`/`bignum`/`ratio` literal and
        // an explicit `(= expr)` both lower to; see `Pattern::Guard`.
        //
        // The binding is made here and thrown away here: the frame it needs
        // exists only for the duration of the test, and `SYM` is the
        // checker's own name, never something the arm body can refer to.
        "pat-guard" => {
            let sym = match core::field(heap, pat, 0) {
                Some(Value::Symbol(id)) => id,
                other => {
                    return Err(EvalError::Internal(format!("eval: (pat-guard ..) names {:?}", other)))
                }
            };
            let test = core::field(heap, pat, 1)
                .ok_or_else(|| EvalError::Internal("eval: (pat-guard ..) has no test".to_string()))?;
            let mut s = RootScope::new(heap);
            let guard_env = extend_env(&mut s, &[(sym, v)], env)?;
            s.push_root(guard_env);
            match it.eval_core(&mut s, test, guard_env)? {
                Value::Bool(true) => Ok(Some(Vec::new())),
                Value::Bool(false) => Ok(None),
                other => Err(EvalError::Internal(format!(
                    "eval: (pat-guard ..) test produced {:?}, not a bool",
                    other
                ))),
            }
        }
        // `(pat-empty)` — the empty list. Its own node rather than
        // `(pat-ctor sexpr 0 ..)` because the empty list outlives `Sexpr`'s
        // `nil` variant; see `Checker::pattern_form`.
        "pat-empty" => Ok(if v.is_empty() { Some(Vec::new()) } else { None }),
        // `(pat-nonempty P)` — `Option<Sexpr>`'s `(some P)`. Under the niche
        // the unwrapped value is the same word, so this rejects the empty
        // list and then matches `P` against the value itself.
        "pat-nonempty" => {
            if v.is_empty() {
                return Ok(None);
            }
            let inner = core::field(heap, pat, 0)
                .ok_or_else(|| EvalError::Internal("eval: (pat-nonempty ..) has no sub-pattern".to_string()))?;
            match_core_pattern(it, heap, env, inner, v)
        }
        "pat-ctor" => {
            let path = path_field(heap, pat, 0, "pat-ctor")?;
            // The instantiation this pattern accepts — see `Pattern::Ctor`'s
            // `targs`. The path above still says `sexpr`-or-not; this says
            // which `gen<_>`.
            let key = str_field(heap, pat, 1, "pat-ctor")?;
            let variant = int_field(heap, pat, 2, "pat-ctor")? as usize;
            // Field 3 is the downcast flag. The interpreter applies the type
            // guard below either way, so it changes nothing here; it is the
            // *compiled* side that emits an instance test only when it is set.
            // Field 4 is the per-field representation list, for the bridge
            // only — the interpreter binds a field as the value it already is.
            let subs = core::fields(heap, pat).map_err(|e| EvalError::Internal(format!("eval: (pat-ctor ..): {}", e)))?;
            if subs.len() < 5 {
                return Err(EvalError::Internal(format!("eval: malformed pattern: {}", core::print(heap, pat))));
            }
            let subs = subs[5..].to_vec();
            match_ctor(it, heap, env, &path, &key, variant, &subs, v)
        }
        "pat-typetest" => {
            let path = path_field(heap, pat, 0, "pat-typetest")?;
            let key = str_field(heap, pat, 1, "pat-typetest")?;
            let inner = core::field(heap, pat, 2)
                .ok_or_else(|| EvalError::Internal("eval: (pat-typetest ..) has no sub-pattern".to_string()))?;
            // `(the sexpr p)` tests nothing: every value is a `Sexpr`.
            if crate::types::path_is_builtin(&path, "sexpr") {
                return match_core_pattern(it, heap, env, inner, v);
            }
            match v {
                Value::Boxed(id) if crate::type_key::heap_type_is_key(heap, id, &key) => {
                    match_core_pattern(it, heap, env, inner, v)
                }
                _ => Ok(None),
            }
        }
        other => Err(EvalError::Internal(format!("eval: no matching for pattern `{}`", other))),
    }
}

/// `(pat-ctor PATH KEY N DOWNCAST (REPR...) P...)` against `v`.
///
/// The `KEY` guard is what keeps two unrelated ADTs that happen to share a
/// shape apart once a heterogeneous `Sexpr` can hold either — a variant index
/// and a field count are not an identity. It is the *instantiation*, not the
/// path (`gen<i32>`, not `gen`), which is what stops a `gen<string>` from
/// reaching a `(the gen<i32> ...)` arm and having its field read as an `i32`. It is also why a genuine `Sexpr`
/// datum must only reach the `Sexpr` arm when `PATH` really is `sexpr`: that
/// arm indexes by *bare* variant number, where a struct's sole variant 0 would
/// spuriously match `Sexpr::Nil`.
fn match_ctor(
    it: &Interp,
    heap: &mut Heap,
    env: Value,
    path: &crate::Path,
    key: &str,
    variant: usize,
    subs: &[Value],
    v: Value,
) -> Result<Option<Vec<(SymRef, Value)>>, EvalError> {
    match v {
        Value::Boxed(id)
            if heap.is_enum(id)
                && crate::type_key::heap_type_is_key(heap, id, key)
                && heap.enum_variant(id) == variant
                && heap.enum_field_count(id) == subs.len() =>
        {
            let mut binds = Vec::new();
            for (i, p) in subs.iter().enumerate() {
                // No per-field decode: the stored word *is* the value. The
                // old pattern carried a `sexpr_fields` bit and a
                // `field_types` entry purely to drive one, and both reduced
                // to the identity once the value worlds merged.
                let f = heap.enum_field(id, i);
                match match_core_pattern(it, heap, env, *p, f)? {
                    Some(b) => binds.extend(b),
                    None => return Ok(None),
                }
            }
            Ok(Some(binds))
        }
        // A `defstruct` has exactly one variant (`new`, index 0), so only the
        // field count and the sub-patterns can fail here.
        Value::Boxed(id)
            if heap.is_struct(id)
                && crate::type_key::heap_type_is_key(heap, id, key)
                && variant == 0
                && heap.struct_field_count(id) == subs.len() =>
        {
            let mut binds = Vec::new();
            for (i, p) in subs.iter().enumerate() {
                let f = heap.struct_field(id, i);
                match match_core_pattern(it, heap, env, *p, f)? {
                    Some(b) => binds.extend(b),
                    None => return Ok(None),
                }
            }
            Ok(Some(binds))
        }
        sv if *path == crate::Path::root("sexpr") => match_sexpr_core(it, heap, env, variant, subs, sv),
        _ => Ok(None),
    }
}

/// A `Sexpr` constructor pattern, destructured through the heap.
fn match_sexpr_core(
    it: &Interp,
    heap: &mut Heap,
    env: Value,
    variant: usize,
    subs: &[Value],
    v: Value,
) -> Result<Option<Vec<(SymRef, Value)>>, EvalError> {
    use super::{narrow_variant, SEXPR_BIGNUM, SEXPR_BOOL, SEXPR_CHAR, SEXPR_CONS, SEXPR_F32, SEXPR_F64, SEXPR_I32, SEXPR_NIL, SEXPR_PATH, SEXPR_RATIO, SEXPR_STR, SEXPR_SYM};

    // Each of these binds the scrutinee (or a piece of it) straight through:
    // a float/bignum/ratio/string box *is* its value, so there is nothing to
    // unwrap and re-wrap.
    let one = |heap: &mut Heap, bound: Value| -> Result<Option<Vec<(SymRef, Value)>>, EvalError> {
        match subs.first() {
            Some(p) => match_core_pattern(it, heap, env, *p, bound),
            None => Err(EvalError::Internal("eval: sexpr pattern has no sub-pattern".to_string())),
        }
    };

    // A narrow integer node matches its own variant and no other: the five
    // widths are five types, and `(u8 x)` binding a `u16` node would be the
    // fold this representation exists to undo.
    if let (Some((width, signed)), Value::Boxed(id)) = (narrow_variant(variant), v) {
        return if heap.is_narrow(id, width, signed) {
            // The binding is the plain normalized word — what a statically
            // typed `u8` is everywhere but inside a `Sexpr`.
            let n = heap.narrow_box(id).expect("just tested").value;
            one(heap, Value::Int(n))
        } else {
            Ok(None)
        };
    }

    match (variant, v) {
        (SEXPR_NIL, Value::Empty) => Ok(Some(Vec::new())),
        (SEXPR_I32, Value::Int(_)) => one(heap, v),
        (SEXPR_CHAR, Value::Char(_)) => one(heap, v),
        (SEXPR_BOOL, Value::Bool(_)) => one(heap, v),
        (SEXPR_SYM, Value::Symbol(_)) => one(heap, v),
        (SEXPR_STR, Value::Str(_)) => one(heap, v),
        // One arm per width, each testing its own box: an `f32` node must not
        // match `(f64 x)`, or the pattern would be handing out a binding of a
        // type the value does not have.
        (SEXPR_F64, Value::Boxed(id)) if heap.is_f64(id) => one(heap, v),
        (SEXPR_F32, Value::Boxed(id)) if heap.is_f32(id) => one(heap, v),
        (SEXPR_BIGNUM, Value::Boxed(id)) if heap.is_bignum(id) => one(heap, v),
        (SEXPR_RATIO, Value::Boxed(id)) if heap.is_ratio(id) => one(heap, v),
        (SEXPR_CONS, Value::Cons(_)) => {
            let car = heap.car(v).map_err(heap_err)?;
            let cdr = heap.cdr(v).map_err(heap_err)?;
            let (Some(pa), Some(pd)) = (subs.first(), subs.get(1)) else {
                return Err(EvalError::Internal("eval: cons pattern needs two sub-patterns".to_string()));
            };
            let Some(mut binds) = match_core_pattern(it, heap, env, *pa, car)? else {
                return Ok(None);
            };
            match match_core_pattern(it, heap, env, *pd, cdr)? {
                Some(b) => {
                    binds.extend(b);
                    Ok(Some(binds))
                }
                None => Ok(None),
            }
        }
        // `(Path segs)` binds a *fresh* proper list of the path's segments, in
        // written order — the inverse of `construct_sexpr_core`'s `Path` arm.
        // The one pattern that allocates, which is why every bound value in
        // this file is rooted as it is collected.
        (SEXPR_PATH, Value::Path(id)) => {
            let segs = heap.path_segments(id).to_vec();
            let mut acc = Value::Empty;
            for seg in segs.into_iter().rev() {
                heap.push_root(acc);
                // core-build-ok: rebuilds a quoted `::`-path as user data, not
                // a core form. The growing tail is rooted just above, and the
                // result stays rooted deliberately — see the note below.
                acc = heap.cons(Value::Symbol(seg), acc).map_err(heap_err)?;
            }
            // Rooted, and deliberately never popped here. This list *is* the
            // binding and nothing else refers to it, so a `RootScope` around
            // this arm would truncate the root away the moment the arm
            // returns — leaving the binding collectible before the frame is
            // built, or before a later sub-pattern's own allocation. The
            // caller's scope is what releases it, which is the same contract
            // every other bound value here has.
            heap.push_root(acc);
            one(heap, acc)
        }
        _ => Ok(None),
    }
}

/// `(construct sexpr N E...)` — build a `Sexpr` datum from evaluated fields.
pub(super) fn construct_sexpr_core(heap: &mut Heap, variant: usize, argv: &[Value]) -> Result<Value, EvalError> {
    use super::{narrow_variant, SEXPR_BIGNUM, SEXPR_BOOL, SEXPR_CHAR, SEXPR_CONS, SEXPR_F32, SEXPR_F64, SEXPR_I32, SEXPR_NIL, SEXPR_PATH, SEXPR_RATIO, SEXPR_STR, SEXPR_SYM};

    let arg = |i: usize| -> Result<Value, EvalError> {
        argv.get(i)
            .copied()
            .ok_or_else(|| EvalError::Internal(format!("eval: (construct sexpr {} ..) is missing field {}", variant, i)))
    };
    // The narrow widths box on the way in, from the *variant*'s width rather
    // than from the value: the word is the same bit pattern for a `u8` `200`
    // and an `i32` `200`, which is why the box has to be told.
    if let Some((width, signed)) = narrow_variant(variant) {
        let n = super::rt_i64(&arg(0)?)?;
        return Ok(heap.alloc_narrow(width, signed, n));
    }

    match variant {
        SEXPR_NIL => Ok(Value::Empty),
        // Each of these validates the argument and then passes the value
        // itself through: the box *is* the datum, so `(eq s (sexpr-str (Str
        // s)))` holds, as CL requires.
        SEXPR_I32 => Ok(Value::Int(super::rt_i64(&arg(0)?)?)),
        SEXPR_CHAR => Ok(Value::Char(super::rt_char(&arg(0)?)?)),
        SEXPR_BOOL => Ok(Value::Bool(super::rt_bool(&arg(0)?)?)),
        SEXPR_SYM => arg(0),
        // Each width validates against its own box. The check is what makes
        // `(f64 x)` on an `f32` a failure rather than a silent widening.
        SEXPR_F64 => {
            let v = arg(0)?;
            super::rt_f64(heap, &v)?;
            Ok(v)
        }
        SEXPR_F32 => {
            let v = arg(0)?;
            super::rt_f32(heap, &v)?;
            Ok(v)
        }
        SEXPR_STR => {
            let v = arg(0)?;
            super::rt_str(heap, &v)?;
            Ok(v)
        }
        // `bignum`/`ratio` are the two that really do re-box: the argument is
        // read back out as a Rust value and a fresh box allocated, matching
        // the old evaluator exactly.
        SEXPR_BIGNUM => {
            let n = super::rt_bignum(heap, &arg(0)?)?;
            Ok(heap.alloc_bignum(n))
        }
        SEXPR_RATIO => {
            let r = super::rt_ratio(heap, &arg(0)?)?;
            Ok(heap.alloc_ratio(r))
        }
        // core-build-ok: this *is* `cons` — the user-facing primitive, over
        // user data. The arguments are rooted by the caller, which is what
        // makes the allocation safe.
        SEXPR_CONS => heap.cons(arg(0)?, arg(1)?).map_err(heap_err),
        SEXPR_PATH => {
            let ids = super::sexpr_list_to_symbols(heap, arg(0)?)?;
            Ok(heap.intern_path(&ids))
        }
        _ => Err(EvalError::Internal(format!("eval: (construct sexpr {} ..): unknown variant", variant))),
    }
}

/// The tail of a node's field list, starting at field `n`.
///
/// A body is kept as the node's own tail rather than copied into a fresh
/// list: it is already a proper list of forms sitting in the node, so a
/// closure can hold it directly with no allocation and no second copy to keep
/// in step.
pub(super) fn tail_after(heap: &Heap, form: Value, n: usize) -> Result<Value, EvalError> {
    tail_after_value(heap, heap.cdr(form).map_err(heap_err)?, n)
}

/// [`tail_after`] over an already-taken tail — for a positional list (a
/// `labels` definition) that has no tag to skip.
pub(super) fn tail_after_value(heap: &Heap, list: Value, n: usize) -> Result<Value, EvalError> {
    let mut cur = list;
    for _ in 0..n {
        cur = heap.cdr(cur).map_err(heap_err)?;
    }
    Ok(cur)
}

/// A field holding a vtable's slots: `((PATH SYM)...)`, the owning type and
/// method name for each of the trait's methods, in slot order.
pub(super) fn method_list(heap: &Heap, form: Value, i: usize, what: &str) -> Result<Vec<(crate::Path, String)>, EvalError> {
    let field = core::field(heap, form, i)
        .ok_or_else(|| EvalError::Internal(format!("eval: ({} ..) has no field {}", what, i)))?;
    let entries = heap
        .list_to_vec(field)
        .map_err(|e| EvalError::Internal(format!("eval: ({} ..) field {}: {}", what, i, e)))?;
    entries.iter().map(|e| method_entry(heap, *e)).collect()
}

/// One `(PATH SYM)` slot entry.
fn method_entry(heap: &Heap, entry: Value) -> Result<(crate::Path, String), EvalError> {
    let parts = heap
        .list_to_vec(entry)
        .map_err(|e| EvalError::Internal(format!("eval: vtable slot: {}", e)))?;
    let [ty, name] = parts[..] else {
        return Err(EvalError::Internal(format!("eval: vtable slot is not (PATH SYM): {}", core::print(heap, entry))));
    };
    let ty = match ty {
        Value::Path(id) => crate::types::path_from_id(heap, id),
        Value::Symbol(id) => crate::Path::root(heap.symbol_name(id)),
        other => return Err(EvalError::Internal(format!("eval: vtable slot type is {:?}", other))),
    };
    let Value::Symbol(name) = name else {
        return Err(EvalError::Internal(format!("eval: vtable slot method is {:?}", name)));
    };
    Ok((ty, heap.symbol_name(name).to_string()))
}

/// One supertrait's dispatch table: the trait it is for, and that trait's
/// method slots as `(implementing type, method name)`.
pub(super) type SuperTable = (crate::Path, Vec<(crate::Path, String)>);

/// The supertrait tables field: `((PATH ((PATH SYM)...))...)`.
pub(super) fn super_list(heap: &Heap, form: Value, i: usize) -> Result<Vec<SuperTable>, EvalError> {
    let field = core::field(heap, form, i)
        .ok_or_else(|| EvalError::Internal(format!("eval: (dyn-new ..) has no field {}", i)))?;
    let entries = heap
        .list_to_vec(field)
        .map_err(|e| EvalError::Internal(format!("eval: (dyn-new ..) supertraits: {}", e)))?;
    entries
        .iter()
        .map(|e| {
            let parts = heap
                .list_to_vec(*e)
                .map_err(|err| EvalError::Internal(format!("eval: (dyn-new ..) supertrait: {}", err)))?;
            let [path, slots] = parts[..] else {
                return Err(EvalError::Internal(format!(
                    "eval: (dyn-new ..) supertrait is not (PATH SLOTS): {}",
                    core::print(heap, *e)
                )));
            };
            let path = match path {
                Value::Path(id) => crate::types::path_from_id(heap, id),
                Value::Symbol(id) => crate::Path::root(heap.symbol_name(id)),
                other => return Err(EvalError::Internal(format!("eval: (dyn-new ..) supertrait path is {:?}", other))),
            };
            let slots = heap
                .list_to_vec(slots)
                .map_err(|err| EvalError::Internal(format!("eval: (dyn-new ..) supertrait slots: {}", err)))?;
            let slots = slots.iter().map(|s| method_entry(heap, *s)).collect::<Result<_, _>>()?;
            Ok((path, slots))
        })
        .collect()
}

/// The names a parameter list `((SYM R)...)` binds, in order.
fn param_names(heap: &Heap, params: Value) -> Result<Vec<SymRef>, EvalError> {
    let ps = heap
        .list_to_vec(params)
        .map_err(|e| EvalError::Internal(format!("eval: parameter list: {}", e)))?;
    ps.iter()
        .map(|p| match heap.car(*p) {
            Ok(Value::Symbol(sym)) => Ok(sym),
            other => Err(EvalError::Internal(format!("eval: parameter is not (SYM R): {:?}", other))),
        })
        .collect()
}

/// [`param_names`]'s other half: the declared representation of each entry in
/// a parameter list `((SYM R)...)`, in order.
///
/// The interpreter itself never needs these — it binds a parameter to the
/// `Value` it already is — but `Interp::apply_interpreted` does, to decode
/// the raw words a *compiled* caller passes into an interpreted closure.
pub(super) fn param_reprs(heap: &Heap, params: Value) -> Result<Vec<Repr>, EvalError> {
    let ps = heap
        .list_to_vec(params)
        .map_err(|e| EvalError::Internal(format!("eval: parameter list: {}", e)))?;
    ps.iter()
        .map(|p| {
            let r = heap.cdr(*p).and_then(|d| heap.car(d)).map_err(heap_err)?;
            Repr::read(heap, r)
                .ok_or_else(|| EvalError::Internal(format!("eval: parameter has no representation: {}", core::print(heap, *p))))
        })
        .collect()
}

/// A field holding an absolute path, as a [`crate::Path`].
///
/// A single-segment path reads back as a bare `Value::Symbol` rather than a
/// `Value::Path` — the reader only builds the latter when it sees `::` — so
/// both spellings are accepted and mean the same one-segment path.
pub(super) fn path_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<crate::Path, EvalError> {
    match core::field(heap, form, i) {
        Some(Value::Path(id)) => Ok(crate::types::path_from_id(heap, id)),
        Some(Value::Symbol(id)) => Ok(crate::Path::root(heap.symbol_name(id))),
        other => Err(EvalError::Internal(format!("eval: ({} ..) field {} is not a path: {:?}", what, i, other))),
    }
}

/// A field holding an integer.
pub(super) fn int_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<i64, EvalError> {
    match core::field(heap, form, i) {
        Some(Value::Int(n)) => Ok(n),
        other => Err(EvalError::Internal(format!("eval: ({} ..) field {} is not an integer: {:?}", what, i, other))),
    }
}

/// A field holding a string — a `construct` node's runtime type key, the one
/// place the core IR carries a name rather than a path.
pub(super) fn str_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<String, EvalError> {
    match core::field(heap, form, i) {
        Some(Value::Str(id)) => Ok(heap.string(id).to_string()),
        other => Err(EvalError::Internal(format!("eval: ({} ..) field {} is not a string: {:?}", what, i, other))),
    }
}

/// A field holding a boolean.
pub(super) fn bool_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<bool, EvalError> {
    match core::field(heap, form, i) {
        Some(Value::Bool(b)) => Ok(b),
        other => Err(EvalError::Internal(format!("eval: ({} ..) field {} is not a boolean: {:?}", what, i, other))),
    }
}

/// The cell a name is bound to in `env`, or `None` if it is unbound.
///
/// Frames are searched innermost first, and within a frame the most recently
/// added binding wins — the order `env_get`'s `.rev()` gives, so a `let` that
/// shadows an outer name (or, in a malformed one, itself) resolves the same
/// way it always has.
pub(super) fn env_lookup(heap: &Heap, env: Value, name: SymRef) -> Option<Value> {
    let mut frames = env;
    while let Ok(frame) = heap.car(frames) {
        let mut cur = frame;
        while let Ok(pair) = heap.car(cur) {
            if let Ok(Value::Symbol(id)) = heap.car(pair) {
                if id == name {
                    return heap.cdr(pair).ok();
                }
            }
            cur = heap.cdr(cur).ok()?;
        }
        frames = heap.cdr(frames).ok()?;
    }
    None
}

/// Read a binding cell.
fn cell_value(heap: &Heap, cell: Value) -> Result<Value, EvalError> {
    match cell {
        Value::Boxed(id) if heap.is_cell(id) => Ok(heap.cell_get(id)),
        other => Err(EvalError::Internal(format!("eval: binding does not hold a cell: {:?}", other))),
    }
}

/// A heap failure (arena exhaustion) as an evaluation error.
pub(super) fn heap_err(e: typelisp_mem::Error) -> EvalError {
    EvalError::Internal(format!("eval: {}", e))
}

// ---- the top level -------------------------------------------------------

impl Interp {
    /// Execute one top-level core form: register a definition, or evaluate an
    /// expression and return its value.
    ///
    /// The ten top-level tags (`tests/core_vocabulary_test.rs`'s `top_level`)
    /// are the vocabulary `TopLevel`'s Rust enum used to be. Note what is
    /// *absent*: there is no arm skipping a type-erased generic body. The
    /// checker emits `(module PATH)` and nothing else for a generic template
    /// (it keeps the raw reader form and emits only monomorphized
    /// specializations), so the two "must never run" special cases the old
    /// `exec` carried have nothing left to skip.
    pub fn exec(&self, heap: &mut Heap, tl: Value) -> Result<Option<Value>, EvalError> {
        let result = self.exec_form(heap, tl);
        // After the form has run, not before: a session's recording
        // (`(dump ...)`) is a list of definitions that worked. At *every*
        // depth, because a `(module ...)`'s items come back through here and
        // each one records itself — the container records nothing, which is
        // what keeps a file-derived module from dragging its top-level
        // expressions (including the `(dump ...)` call itself) into the dump.
        if result.is_ok() {
            self.note_definitions(heap, tl);
        }
        result
    }

    /// [`Self::exec`]'s body, one top-level form.
    fn exec_form(&self, heap: &mut Heap, tl: Value) -> Result<Option<Value>, EvalError> {
        let tag = match heap.car(tl) {
            Ok(Value::Symbol(id)) => heap.symbol_name(id).to_string(),
            _ => {
                return Err(EvalError::Internal(format!(
                    "exec: not a top-level core form: {}",
                    core::print(heap, tl)
                )))
            }
        };
        match tag.as_str() {
            // `(defun PATH ((SYM REPR)...) RET-REPR PUBLIC BODY...)`
            "defun" => {
                let name = path_field(heap, tl, 0, "defun")?;
                let (params, param_reprs) = param_list(heap, tl, 1, "defun")?;
                let ret = repr_field(heap, tl, 2, "defun")?;
                let public = bool_field(heap, tl, 3, "defun")?;
                let body = body_forms(heap, tl, 4, "defun")?;
                self.register_fn(heap, tl, name, params, param_reprs, ret, public, body, false, None);
                Ok(None)
            }
            // `(defmethod PATH SYM INSTANCE ((SYM REPR)...) RET-REPR PUBLIC BODY...)`
            //
            // The receiver is simply the first parameter, so there is no
            // `self_name` to splice in ahead of the rest the way the old node's
            // separate field needed.
            "defmethod" => {
                let type_name = path_field(heap, tl, 0, "defmethod")?;
                let method = sym_field(heap, tl, 1, "defmethod")?;
                let _instance = bool_field(heap, tl, 2, "defmethod")?;
                let (params, param_reprs) = param_list(heap, tl, 3, "defmethod")?;
                let ret = repr_field(heap, tl, 4, "defmethod")?;
                let public = bool_field(heap, tl, 5, "defmethod")?;
                let body = body_forms(heap, tl, 6, "defmethod")?;
                heap.push_permanent_root(tl);
                let def = FnDef { ffi: false,
                    name: format!("{}::{}", type_name, method),
                    params,
                    body,
                    rest: false,
                    lambda: None,
                    sig: Some((param_reprs, ret)),
                    public,
                    compiled: RefCell::new(None),
                };
                self.root
                    .borrow_mut()
                    .get_or_create(type_name.parent())
                    .methods
                    .insert((type_name.last_segment().to_string(), method), Rc::new(def));
                Ok(None)
            }
            // `(defmacro PATH (SYM...) REST (REQUIRED (OPT-BODY...) ((SYM OPT-BODY...)...)) PUBLIC BODY...)`
            //
            // A macro's body is callable exactly like a `defun`'s — expanding
            // it is calling it — so it goes in the very same `fns` table; no
            // separate macro table exists.
            "defmacro" => {
                let name = path_field(heap, tl, 0, "defmacro")?;
                let params = sym_list_field(heap, tl, 1, "defmacro")?;
                let rest = bool_field(heap, tl, 2, "defmacro")?;
                let lambda = macro_lambda(heap, tl, 3)?;
                let public = bool_field(heap, tl, 4, "defmacro")?;
                let body = body_forms(heap, tl, 5, "defmacro")?;
                heap.push_permanent_root(tl);
                // Every parameter — and the result — is `Sexpr`. The checker
                // already checks a macro body under exactly those types
                // (`Checker::check_defmacro`), so this records a signature it
                // established rather than inventing one, and recording it is
                // what makes a macro body compilable: `compiled_fn_body`
                // refuses a `FnDef` without one, which is why `(compile
                // <macro>)` used to answer "has no signature (is it a
                // defmacro?)".
                //
                // One entry per *binding*, `&rest` included — the same shape a
                // `&rest` `defun`'s signature has, and the shape
                // `bind_macro_args` produces, since it collects the rest
                // arguments into one `Sexpr` before the call.
                let sig = (vec![Repr::Sexpr; params.len()], Repr::Sexpr);
                let def = FnDef { ffi: false,
                    name: name.to_string(),
                    params,
                    body,
                    rest,
                    lambda: Some(lambda),
                    sig: Some(sig),
                    public,
                    compiled: RefCell::new(None),
                };
                self.root
                    .borrow_mut()
                    .get_or_create(name.parent())
                    .fns
                    .insert(name.last_segment().to_string(), Rc::new(def));
                Ok(None)
            }
            // `(defvar PATH REPR MUTABLE PUBLIC FORM)`
            //
            // The representation is not read: it used to pick the global's slot
            // kind, which no longer varies. `mutable` is the checker's business
            // (it rejects a write to a constant), not the runtime's.
            // `(defvar PATH REPR MUTABLE PUBLIC INIT [REASSIGN])`.
            //
            // A sixth field means the source form was `defparameter`, which
            // assigns whether or not the global is already bound. Plain
            // `defvar` initializes **only if unbound**, as CL's does: the
            // initializer is not evaluated at all the second time, so
            // re-loading a file neither repeats its side effects nor throws
            // away what the session has since stored there. Absent reads as
            // `defvar` — see `Checker::defvar_form`.
            "defvar" => {
                let name = path_field(heap, tl, 0, "defvar")?;
                let public = bool_field(heap, tl, 3, "defvar")?;
                let reassign = matches!(core::field(heap, tl, 5), Some(Value::Bool(true)));
                if !reassign && self.global_is_bound(&name) {
                    return Ok(None);
                }
                let value = core::field(heap, tl, 4)
                    .ok_or_else(|| EvalError::Internal("exec: (defvar ..) has no initializer".to_string()))?;
                let v = self.eval_core(heap, value, Value::Empty)?;
                let mut s = RootScope::new(heap);
                s.push_root(v);
                let slot = self.slot(&mut s, v);
                self.root
                    .borrow_mut()
                    .get_or_create(name.parent())
                    .globals
                    .insert(name.last_segment().to_string(), scope::GlobalDef { slot, public });
                Ok(None)
            }
            // The type itself was registered in the checker's `Registry` at
            // check time; recording the `TypeEntry` is all the interpreter
            // needs (see `scope::TypeEntry`'s doc comment).
            "defstruct" => {
                let name = path_field(heap, tl, 0, "defstruct")?;
                let fields = core::field(heap, tl, 1)
                    .ok_or_else(|| EvalError::Internal("exec: (defstruct ..) has no field list".to_string()))?;
                let fields = repr_list(heap, fields, "defstruct")?;
                self.root
                    .borrow_mut()
                    .get_or_create(name.parent())
                    .types
                    .insert(name.last_segment().to_string(), scope::TypeEntry::Struct(fields));
                Ok(None)
            }
            // `(defenum PATH (SYM...) ((REPR...)...))` — unlike `defstruct`
            // this is *not* a `TypeEntry::Struct` (an enum instance is never a
            // boxed struct), but its variants are recorded: the printer needs
            // the names and the compiled-global boundary needs each field's
            // representation.
            "defenum" => {
                let name = path_field(heap, tl, 0, "defenum")?;
                let names = sym_list_field(heap, tl, 1, "defenum")?;
                let per_variant = core::field(heap, tl, 2)
                    .ok_or_else(|| EvalError::Internal("exec: (defenum ..) has no variant fields".to_string()))?;
                let lists = heap
                    .list_to_vec(per_variant)
                    .map_err(|e| EvalError::Internal(format!("exec: (defenum ..) variant fields: {}", e)))?;
                if lists.len() != names.len() {
                    return Err(EvalError::Internal(format!(
                        "exec: (defenum ..) has {} variant name(s) but {} field list(s)",
                        names.len(),
                        lists.len()
                    )));
                }
                let mut variants = Vec::with_capacity(names.len());
                for (n, fields) in names.into_iter().zip(lists) {
                    variants.push((n, repr_list(heap, fields, "defenum")?));
                }
                self.root.borrow_mut().register_enum(&name, EnumDef { variants });
                Ok(None)
            }
            // `(module PATH BODY...)` — ensures the tree node exists (a
            // "mkdir -p") before its body registers into it. It does not have
            // to *stay* current: every nested definition's own path is
            // absolute regardless of nesting, so registration is direct
            // descent from `self.root` rather than relative to a cursor.
            //
            // Not permanently rooted itself: each definition inside roots its
            // own form, which is one root per definition and covers everything
            // that outlives this call. The module's own spine does not.
            "module" => {
                let path = path_field(heap, tl, 0, "module")?;
                self.root.borrow_mut().get_or_create(path.segments());
                let body = body_forms(heap, tl, 1, "module")?;
                let mut s = RootScope::new(heap);
                s.push_root(tl);
                let mut last = None;
                for t in body {
                    last = self.exec(&mut s, t)?;
                }
                Ok(last)
            }
            "use" => Ok(None),
            // A forward declaration produced no code — its whole effect
            // happened in the checker (`Checker::check_defsignature`).
            "defsignature" => Ok(None),
            // `(defffi PATH C-SYMBOL LIBRARY ((SYM REPR)...) RET-REPR PUBLIC (CTYPE...) RET-CTYPE)`
            //
            // The opposite of `defsignature` above: its effect is *here*. The
            // checker only wrote the signature down; what makes the name
            // callable is the thunk, and the thunk needs a C symbol resolved
            // against the running process.
            //
            // So this fails rather than registering a name that cannot run. A
            // `FnDef` with an empty body and no compiled form is not an error
            // anywhere downstream — `apply` would walk no forms and answer
            // `()` — which is exactly the silent wrong answer worth spending
            // an error to avoid.
            "defffi" => {
                let decl = read_ffi_decl(heap, tl)?;
                let name = decl.path.clone();
                let (params, param_reprs) = param_list(heap, tl, 3, "defffi")?;
                let ret = repr_field(heap, tl, 4, "defffi")?;
                let public = bool_field(heap, tl, 5, "defffi")?;
                // Before registering, so a declaration that cannot be
                // resolved leaves no name behind.
                let thunk = (crate::eval::interp::backend_define_ffi()?)(self, &decl)
                    .map_err(EvalError::Panic)?;
                heap.push_permanent_root(tl);
                let def = FnDef { ffi: true,
                    name: name.to_string(),
                    params,
                    body: Vec::new(),
                    rest: false,
                    lambda: None,
                    sig: Some((param_reprs, ret)),
                    public,
                    compiled: RefCell::new(Some(thunk)),
                };
                self.root
                    .borrow_mut()
                    .get_or_create(name.parent())
                    .fns
                    .insert(name.last_segment().to_string(), Rc::new(def));
                Ok(None)
            }
            // `(expr FORM)`
            "expr" => {
                let form = core::field(heap, tl, 0)
                    .ok_or_else(|| EvalError::Internal("exec: (expr ..) has no form".to_string()))?;
                let v = self.eval_core(heap, form, Value::Empty);
                // A `pprint-logical-block` is normally closed by the `let` the
                // special form lowers to, but a `break`/`return` that jumps out
                // of the block (or an error unwinding past it) skips that close
                // and would leave the session open — every later `print`
                // silently buffered into a document nobody flushes. A top-level
                // form is the widest a block can ever span, so closing any
                // still-open one here is both the right boundary and a complete
                // safety net.
                self.flush_pretty()?;
                Ok(Some(v?))
            }
            // `(load ...)` is resolved and applied by the *driver* at check
            // time (`project::load_file_flat`), never reaching exec — the
            // driver consumes it inline rather than queuing it. Reaching here
            // would be a driver bug.
            "load" => Err(EvalError::Internal(
                "exec: (load ..) must be handled by the driver, not exec'd".to_string(),
            )),
            other => Err(EvalError::Internal(format!("exec: no top-level form `{}`", other))),
        }
    }

    /// Apply a registered function/method body: bind its parameters to `args`
    /// and run its core forms.
    ///
    /// **The interpreted half of [`Self::enter`], and private so that it can
    /// only ever be reached through it.** This never looks at `def.compiled`;
    /// calling it on a function that has a compiled body runs the tree-walker
    /// anyway, which is a bug wherever it happens. Its only caller is
    /// `enter_plain`, four lines above.
    ///
    /// The environment is a heap chain (`extend_env`), the same one a `lambda`'s
    /// captured environment is, so a nested closure in the body captures the
    /// parameters by the ordinary mechanism rather than a second one.
    fn apply(&self, heap: &mut Heap, def: &FnDef, args: Vec<Value>) -> Result<Value, EvalError> {
        if def.params.len() != args.len() {
            return Err(EvalError::Internal(format!(
                "apply: the body takes {} argument(s), given {}",
                def.params.len(),
                args.len()
            )));
        }
        let mut s = RootScope::new(heap);
        // The arguments are the only thing holding these values while the
        // environment chain is built, and building it allocates.
        for a in &args {
            s.push_root(*a);
        }
        let binds: Vec<(SymRef, Value)> = def
            .params
            .iter()
            .zip(args)
            .map(|(name, v)| {
                let sym = match s.intern_symbol(name) {
                    Value::Symbol(id) => id,
                    _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
                };
                (sym, v)
            })
            .collect();
        let env = extend_env(&mut s, &binds, Value::Empty)?;
        s.push_root(env);
        let Some((last, rest)) = def.body.split_last() else {
            return Ok(Value::Empty);
        };
        for e in rest {
            self.eval_core(&mut s, *e, env)?;
        }
        self.eval_core(&mut s, *last, env)
    }

    /// The shared tail of `defun` registration — see `FnDef::body` for why the
    /// form is permanently rooted here.
    #[allow(clippy::too_many_arguments)]
    fn register_fn(
        &self,
        heap: &mut Heap,
        tl: Value,
        name: crate::Path,
        params: Vec<String>,
        param_reprs: Vec<Repr>,
        ret: Repr,
        public: bool,
        body: Vec<Value>,
        rest: bool,
        lambda: Option<MacroLambda>,
    ) {
        heap.push_permanent_root(tl);
        let def =
            FnDef { ffi: false, name: name.to_string(), params, body, rest, lambda, sig: Some((param_reprs, ret)), public, compiled: RefCell::new(None) };
        self.root
            .borrow_mut()
            .get_or_create(name.parent())
            .fns
            .insert(name.last_segment().to_string(), Rc::new(def));
    }
}

/// A field holding an interned symbol, as a `String`.
pub(super) fn sym_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<String, EvalError> {
    match core::field(heap, form, i) {
        Some(Value::Symbol(id)) => Ok(heap.symbol_name(id).to_string()),
        other => Err(EvalError::Internal(format!("exec: ({} ..) field {} is not a symbol: {:?}", what, i, other))),
    }
}

/// A field holding a representation.
fn repr_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<Repr, EvalError> {
    let v = core::field(heap, form, i)
        .ok_or_else(|| EvalError::Internal(format!("exec: ({} ..) has no field {}", what, i)))?;
    Repr::read(heap, v).ok_or_else(|| {
        EvalError::Internal(format!("exec: ({} ..) field {} is not a representation: {}", what, i, core::print(heap, v)))
    })
}

/// A list of representations, e.g. one variant's field reprs.
pub(super) fn repr_list(heap: &Heap, list: Value, what: &str) -> Result<Vec<Repr>, EvalError> {
    let vs = heap
        .list_to_vec(list)
        .map_err(|e| EvalError::Internal(format!("exec: ({} ..) representation list: {}", what, e)))?;
    vs.into_iter()
        .map(|v| {
            Repr::read(heap, v).ok_or_else(|| {
                EvalError::Internal(format!("exec: ({} ..) not a representation: {}", what, core::print(heap, v)))
            })
        })
        .collect()
}

/// A field holding a parameter list `((SYM REPR)...)`, split into the names it
/// binds and their representations.
fn param_list(heap: &Heap, form: Value, i: usize, what: &str) -> Result<(Vec<String>, Vec<Repr>), EvalError> {
    let list = core::field(heap, form, i)
        .ok_or_else(|| EvalError::Internal(format!("exec: ({} ..) has no parameter list", what)))?;
    let ps = heap
        .list_to_vec(list)
        .map_err(|e| EvalError::Internal(format!("exec: ({} ..) parameter list: {}", what, e)))?;
    let mut names = Vec::with_capacity(ps.len());
    let mut reprs = Vec::with_capacity(ps.len());
    for p in ps {
        let parts = heap
            .list_to_vec(p)
            .map_err(|e| EvalError::Internal(format!("exec: ({} ..) parameter: {}", what, e)))?;
        match parts.as_slice() {
            [Value::Symbol(sym), r] => {
                names.push(heap.symbol_name(*sym).to_string());
                reprs.push(Repr::read(heap, *r).ok_or_else(|| {
                    EvalError::Internal(format!("exec: ({} ..) parameter representation: {}", what, core::print(heap, *r)))
                })?);
            }
            _ => {
                return Err(EvalError::Internal(format!(
                    "exec: ({} ..) parameter is not (SYM REPR): {}",
                    what,
                    core::print(heap, p)
                )))
            }
        }
    }
    Ok((names, reprs))
}

/// The declaration a `(defffi ...)` node carries.
///
/// Public because it has two readers: `Interp::exec`, which resolves the
/// symbol and hangs the thunk on a `FnDef`, and `compile::aot`, which emits
/// the same thunk into the executable it is building. One reader so the two
/// cannot come to disagree about what the node says.
pub fn read_ffi_decl(heap: &Heap, tl: Value) -> Result<crate::eval::interp::FfiDecl, EvalError> {
    Ok(crate::eval::interp::FfiDecl {
        path: path_field(heap, tl, 0, "defffi")?,
        c_symbol: str_field(heap, tl, 1, "defffi")?,
        // `()` is "look in the running process", which is not a library name
        // — and must not read as one.
        library: match core::field(heap, tl, 2) {
            Some(Value::Str(id)) => Some(heap.string(id).to_string()),
            Some(Value::Empty) => None,
            other => {
                return Err(EvalError::Internal(format!(
                    "exec: (defffi ..) field 2 is not a library name: {:?}",
                    other
                )))
            }
        },
        params: sym_list_field(heap, tl, 6, "defffi")?,
        ret: sym_field(heap, tl, 7, "defffi")?,
    })
}

/// A field holding a bare symbol list `(SYM...)` — a `defmacro`'s parameters
/// (all `Sexpr`, so no representations), a `defenum`'s variant names, or a
/// `defffi`'s C parameter types.
fn sym_list_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<Vec<String>, EvalError> {
    let list = core::field(heap, form, i)
        .ok_or_else(|| EvalError::Internal(format!("exec: ({} ..) has no field {}", what, i)))?;
    let vs = heap
        .list_to_vec(list)
        .map_err(|e| EvalError::Internal(format!("exec: ({} ..) symbol list: {}", what, e)))?;
    vs.into_iter()
        .map(|v| match v {
            Value::Symbol(id) => Ok(heap.symbol_name(id).to_string()),
            other => Err(EvalError::Internal(format!("exec: ({} ..) not a symbol: {:?}", what, other))),
        })
        .collect()
}

/// Everything from field `skip` onward — a definition's body forms.
fn body_forms(heap: &Heap, form: Value, skip: usize, what: &str) -> Result<Vec<Value>, EvalError> {
    let all = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("exec: ({} ..): {}", what, e)))?;
    Ok(all.get(skip..).unwrap_or(&[]).to_vec())
}

/// `(REQUIRED (OPT-BODY...) ((SYM OPT-BODY...)...))` — a `defmacro`'s
/// `&optional`/`&key` structure.
fn macro_lambda(heap: &Heap, form: Value, i: usize) -> Result<MacroLambda, EvalError> {
    let v = core::field(heap, form, i)
        .ok_or_else(|| EvalError::Internal("exec: (defmacro ..) has no lambda-list structure".to_string()))?;
    let parts = heap
        .list_to_vec(v)
        .map_err(|e| EvalError::Internal(format!("exec: (defmacro ..) lambda list: {}", e)))?;
    let [required, opts, keys] = parts.as_slice() else {
        return Err(EvalError::Internal(format!(
            "exec: (defmacro ..) lambda list is not (REQUIRED OPTIONALS KEYS): {}",
            core::print(heap, v)
        )));
    };
    let required = match required {
        Value::Int(n) if *n >= 0 => *n as usize,
        other => {
            return Err(EvalError::Internal(format!(
                "exec: (defmacro ..) required count is not a non-negative integer: {:?}",
                other
            )))
        }
    };
    let opt_lists = heap
        .list_to_vec(*opts)
        .map_err(|e| EvalError::Internal(format!("exec: (defmacro ..) optionals: {}", e)))?;
    let mut optionals = Vec::with_capacity(opt_lists.len());
    for one in opt_lists {
        optionals.push(
            heap.list_to_vec(one)
                .map_err(|e| EvalError::Internal(format!("exec: (defmacro ..) optional default: {}", e)))?,
        );
    }
    let key_entries = heap
        .list_to_vec(*keys)
        .map_err(|e| EvalError::Internal(format!("exec: (defmacro ..) keys: {}", e)))?;
    let mut key_defaults = Vec::with_capacity(key_entries.len());
    for entry in key_entries {
        let parts = heap
            .list_to_vec(entry)
            .map_err(|e| EvalError::Internal(format!("exec: (defmacro ..) key: {}", e)))?;
        let Some((Value::Symbol(sym), default)) = parts.split_first().map(|(h, t)| (*h, t)) else {
            return Err(EvalError::Internal(format!(
                "exec: (defmacro ..) key is not (SYM BODY...): {}",
                core::print(heap, entry)
            )));
        };
        key_defaults.push((heap.symbol_name(sym).to_string(), default.to_vec()));
    }
    Ok(MacroLambda { required, optionals, keys: key_defaults })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Reader;

    /// A heap in stress mode: every `cons` collects first, so a value the
    /// evaluator holds without rooting dies at the very next allocation rather
    /// than one run in a hundred.
    fn stress_heap() -> Heap {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_gc_stress(true);
        h
    }

    /// Read exactly one core form. The reader conses, so this already runs
    /// under `gc_stress` when the heap is one of ours.
    fn read1(heap: &mut Heap, src: &str) -> Value {
        let r = Reader::new();
        let mut vs = r.read_all(heap, src).expect("read failed");
        assert_eq!(vs.len(), 1, "expected exactly one form in {:?}", src);
        vs.pop().unwrap()
    }

    /// Evaluate a core form written as source, in the empty environment.
    ///
    /// Also asserts that the root stack is exactly where it started: a leaked
    /// root is not a crash, it is a value that can never be collected, and the
    /// only place it shows up is here.
    fn eval_src(heap: &mut Heap, src: &str) -> Result<Value, EvalError> {
        let form = read1(heap, src);
        heap.push_root(form);
        let before = heap.root_count();
        let interp = Interp::new();
        let out = interp.eval_core(heap, form, Value::Empty);
        assert_eq!(heap.root_count(), before, "eval_core leaked or over-popped roots on {:?}", src);
        out
    }

    fn eval_ok(heap: &mut Heap, src: &str) -> Value {
        eval_src(heap, src).unwrap_or_else(|e| panic!("{:?} failed to evaluate: {}", src, e))
    }

    // ---- literals --------------------------------------------------------

    #[test]
    fn literals_evaluate_to_themselves() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(int-any-width 42)"), Value::Int(42));
        assert_eq!(eval_ok(&mut h, "(int-any-width -7)"), Value::Int(-7));
        assert_eq!(eval_ok(&mut h, "(bool true)"), Value::Bool(true));
        assert_eq!(eval_ok(&mut h, "(bool false)"), Value::Bool(false));
        assert_eq!(eval_ok(&mut h, r"(char #\a)"), Value::Char('a'));
        assert_eq!(eval_ok(&mut h, "(unit)"), Value::Empty);

        let v = eval_ok(&mut h, "(float-any-width 1.5)");
        match v {
            Value::Boxed(id) => assert_eq!(h.float_box(id), Some(typelisp_mem::FloatBox::F64(1.5))),
            other => panic!("expected a float-any-width box, got {:?}", other),
        }
        let v = eval_ok(&mut h, "(bignum 123456789012345678901234567890)");
        match v {
            Value::Boxed(id) if h.is_bignum(id) => {
                assert_eq!(h.bignum_value(id).to_string(), "123456789012345678901234567890")
            }
            other => panic!("expected a bignum box, got {:?}", other),
        }
        let v = eval_ok(&mut h, "(ratio 1/3)");
        match v {
            Value::Boxed(id) if h.is_ratio(id) => {
                let r = h.ratio_value(id);
                assert_eq!((r.numer().to_string(), r.denom().to_string()), ("1".to_string(), "3".to_string()));
            }
            other => panic!("expected a ratio box, got {:?}", other),
        }
        let v = eval_ok(&mut h, r#"(str "hi")"#);
        match v {
            Value::Str(id) => assert_eq!(h.string(id), "hi"),
            other => panic!("expected a string, got {:?}", other),
        }
        // A `sym` literal is the interned symbol itself, which is what makes
        // `(eq :foo :foo)` true for free.
        let a = eval_ok(&mut h, "(sym foo)");
        let b = eval_ok(&mut h, "(sym foo)");
        assert_eq!(a, b);
        assert!(matches!(a, Value::Symbol(_)));
    }

    /// Each evaluation of a string literal allocates a *fresh* string, so two
    /// of them are `equal` but not `eq`. Handing back the box the checker
    /// stored in the node would silently collapse `eq` to content comparison —
    /// the property `str_rt`'s doc comment exists to protect.
    #[test]
    fn a_string_literal_is_a_fresh_string_every_time() {
        let mut h = stress_heap();
        let a = eval_ok(&mut h, r#"(str "hi")"#);
        // Rooted before the second evaluation, which allocates: without this
        // the first result is collected and the second *reuses its slot*, so
        // the two compare equal and the test passes for the wrong reason.
        h.push_root(a);
        let b = eval_ok(&mut h, r#"(str "hi")"#);
        match (a, b) {
            (Value::Str(x), Value::Str(y)) => {
                assert_ne!(x, y, "two evaluations of a string literal shared one string");
                assert_eq!(h.string(x), h.string(y));
            }
            other => panic!("expected two strings, got {:?}", other),
        }
    }

    /// The same for the numeric boxes, for the same reason.
    #[test]
    fn a_float_literal_is_a_fresh_box_every_time() {
        let mut h = stress_heap();
        let a = eval_ok(&mut h, "(float-any-width 1.5)");
        h.push_root(a);
        let b = eval_ok(&mut h, "(float-any-width 1.5)");
        assert_ne!(a, b, "two evaluations of a float-any-width literal shared one box");
    }

    // ---- variables and let ----------------------------------------------

    #[test]
    fn let_binds_and_var_reads() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(let ((x int-any-width (int-any-width 1))) (var x))"), Value::Int(1));
        assert_eq!(
            eval_ok(&mut h, "(let ((x int-any-width (int-any-width 1)) (y int-any-width (int-any-width 2))) (var y))"),
            Value::Int(2)
        );
    }

    #[test]
    fn a_nested_let_shadows_the_outer_binding() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, "(let ((x int-any-width (int-any-width 1))) (let ((x int-any-width (int-any-width 2))) (var x)))"),
            Value::Int(2)
        );
        // ...and the outer binding is visible again once the inner `let` ends.
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((x int-any-width (int-any-width 1))) (let () (let ((x int-any-width (int-any-width 2))) (var x)) (var x)))"
            ),
            Value::Int(1)
        );
    }

    /// CL `let`, not `let*`: every initialiser is evaluated in the *outer*
    /// environment, so `y` here sees the outer `x` and not the `x` being bound
    /// beside it.
    #[test]
    fn let_initialisers_run_in_the_outer_environment() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((x int-any-width (int-any-width 1))) (let ((x int-any-width (int-any-width 2)) (y int-any-width (var x))) (var y)))"
            ),
            Value::Int(1)
        );
    }

    /// `progn` is not a tag of its own — it is a `let` with no bindings.
    #[test]
    fn a_let_with_no_bindings_is_progn() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(let () (int-any-width 1) (int-any-width 2))"), Value::Int(2));
        // An empty sequence is unit, matching `eval_seq`'s empty case.
        assert_eq!(eval_ok(&mut h, "(let ())"), Value::Empty);
    }

    /// A `let`'s frame survives a collection triggered from inside its own
    /// body.
    ///
    /// The frame and its cells are reachable only through the environment,
    /// which the loop roots in its state slot — but there is a gap between
    /// `extend_env` building the frame and the loop taking it, and the body
    /// runs in that gap. Every other `let` test here never allocates, so
    /// none of them can see the difference; this one conses *before* reading
    /// the binding, which under `gc_stress` collects between the two.
    #[test]
    fn a_let_frame_survives_a_collection_inside_its_own_body() {
        let mut h = stress_heap();
        let v = eval_ok(
            &mut h,
            "(let ((x int-any-width (int-any-width 7)) (y int-any-width (int-any-width 8)))
               (call (sexpr-cons) () sexpr-cons (sexpr sexpr)
                 (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2))
                 (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (var x) (var y))))",
        );
        let tail = h.cdr(v).unwrap();
        assert_eq!(h.car(tail).unwrap(), Value::Int(7));
        assert_eq!(h.cdr(tail).unwrap(), Value::Int(8));
    }

    /// The same for a body form that is not the last one: an earlier form's
    /// allocations must not cost the later forms their bindings.
    #[test]
    fn a_let_frame_survives_a_collection_in_an_earlier_body_form() {
        let mut h = stress_heap();
        let v = eval_ok(
            &mut h,
            "(let ((x int-any-width (int-any-width 7)))
               (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2))
               (var x))",
        );
        assert_eq!(v, Value::Int(7));
    }

    /// Every form in a body runs, not just the last: the earlier ones are
    /// there for their effects.
    #[test]
    fn every_form_in_a_body_is_evaluated() {
        let mut h = stress_heap();
        // The first form would fail loudly if it were skipped.
        let e = eval_src(&mut h, r#"(let () (panic (str "ran")) (int-any-width 2))"#).unwrap_err();
        assert!(matches!(e.kind(), EvalError::Panic(m) if m == "ran"), "{:?}", e);
    }

    #[test]
    fn an_unbound_variable_names_itself() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(var nope)").unwrap_err();
        assert!(matches!(e.kind(), EvalError::Unbound(n) if n == "nope"), "{:?}", e);
    }

    // ---- if --------------------------------------------------------------

    #[test]
    fn if_takes_the_branch_the_condition_selects() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(if (bool true) (int-any-width 1) (int-any-width 2))"), Value::Int(1));
        assert_eq!(eval_ok(&mut h, "(if (bool false) (int-any-width 1) (int-any-width 2))"), Value::Int(2));
        // The branch not taken is not evaluated.
        assert_eq!(
            eval_ok(&mut h, r#"(if (bool true) (int-any-width 1) (panic (str "boom")))"#),
            Value::Int(1)
        );
    }

    /// The loop's reason for existing. The Rust-tree evaluator had an
    /// iterative special case for `if` alone, because a long `cond`
    /// expands into an `if` nested in the *else* position and recursing
    /// through it overflowed the stack. Here it is a jump, so the depth costs
    /// nothing.
    ///
    /// Built directly rather than read, because the reader would have to
    /// recurse this deep to parse it and the point is the evaluator.
    #[test]
    fn a_deep_else_chain_is_constant_stack() {
        const DEPTH: usize = 20_000;
        let mut h = Heap::with_capacity(1 << 20);
        h.set_growth_limit(1 << 24);

        // Built with stress off: it is `DEPTH` nodes of setup, and collecting
        // at each of them would be quadratic for no added coverage. The
        // *evaluation* below is what runs under stress.
        let mut form = core::tagged(&mut h, "int-any-width", &[Value::Int(7)]).unwrap();
        h.push_root(form);
        for _ in 0..DEPTH {
            let cond = core::tagged(&mut h, "bool", &[Value::Bool(false)]).unwrap();
            h.push_root(cond);
            let then = core::tagged(&mut h, "int-any-width", &[Value::Int(0)]).unwrap();
            h.push_root(then);
            form = core::tagged(&mut h, "if", &[cond, then, form]).unwrap();
            h.push_root(form);
        }

        h.set_gc_stress(true);
        let before = h.root_count();
        let interp = Interp::new();
        let v = interp.eval_core(&mut h, form, Value::Empty).expect("deep chain failed");
        assert_eq!(v, Value::Int(7));
        assert_eq!(h.root_count(), before, "a tail jump grew the root stack");
    }

    /// The same property for a `let` body's last form, which is also a jump.
    #[test]
    fn a_deep_let_tail_is_constant_stack() {
        const DEPTH: usize = 20_000;
        let mut h = Heap::with_capacity(1 << 20);
        h.set_growth_limit(1 << 24);

        let mut form = core::tagged(&mut h, "int-any-width", &[Value::Int(7)]).unwrap();
        h.push_root(form);
        for _ in 0..DEPTH {
            let empty = Value::Empty;
            form = core::tagged(&mut h, "let", &[empty, form]).unwrap();
            h.push_root(form);
        }

        h.set_gc_stress(true);
        let interp = Interp::new();
        let v = interp.eval_core(&mut h, form, Value::Empty).expect("deep let tail failed");
        assert_eq!(v, Value::Int(7));
    }

    // ---- calls -----------------------------------------------------------

    #[test]
    fn a_builtin_call_evaluates_its_arguments_and_applies() {
        let mut h = stress_heap();
        let v = eval_ok(&mut h, "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2))");
        assert_eq!(h.car(v).unwrap(), Value::Int(1));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(2));

        // Arguments are themselves core forms, evaluated left to right.
        let v = eval_ok(
            &mut h,
            "(let ((a int-any-width (int-any-width 5))) (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (var a) (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 6) (unit))))",
        );
        assert_eq!(h.car(v).unwrap(), Value::Int(5));
        let rest = h.cdr(v).unwrap();
        assert_eq!(h.car(rest).unwrap(), Value::Int(6));
    }

    #[test]
    fn a_call_to_nothing_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(call (no-such-thing) () no-such-thing ())").unwrap_err();
        assert!(matches!(e.kind(), EvalError::NoSuchFunction(n) if n == "no-such-thing"), "{:?}", e);
    }

    // ---- panic and diagnostics -------------------------------------------

    #[test]
    fn panic_carries_its_message() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, r#"(panic (str "boom"))"#).unwrap_err();
        assert!(matches!(e.kind(), EvalError::Panic(m) if m == "boom"), "{:?}", e);
    }

    /// A runtime error is placed at the node it came from, read off the node's
    /// own cell — the slot the checker fills in as it lowers, and the same one
    /// the reader uses for a list form's span.
    #[test]
    fn a_runtime_error_is_placed_at_the_node_that_raised_it() {
        use std::rc::Rc;

        let mut h = stress_heap();
        let form = read1(&mut h, r#"(let () (panic (str "boom")))"#);
        h.push_root(form);

        // Place the inner `panic`, not the outer `let`, so the assertion shows
        // the *innermost* location wins rather than the one at the top.
        let inner = core::field(&h, form, 1).unwrap();
        let loc = crate::Loc::new(Rc::from("f.typl"), 3, 9).with_end(3, 22);
        match inner {
            Value::Cons(cr) => h.set_cons_loc(cr, loc.clone()),
            other => panic!("expected a node, got {:?}", other),
        }

        let interp = Interp::new();
        let e = interp.eval_core(&mut h, form, Value::Empty).unwrap_err();
        match &e {
            EvalError::At(got, inner) => {
                assert_eq!(got, &loc);
                assert!(matches!(**inner, EvalError::Panic(ref m) if m == "boom"), "{:?}", inner);
            }
            other => panic!("expected a located error, got {:?}", other),
        }
    }

    /// A tag with no evaluation names itself, rather than being taken for
    /// something else.
    ///
    /// This used to have two halves: one naming whichever vocabulary tag was
    /// not implemented yet (it named `loop`, then `assoc`, each example moving
    /// as a stage landed), and `unlowered`, Phase 2's scaffolding marker.
    /// **Every vocabulary tag now evaluates**, so the moving half had nothing
    /// left to name; `unlowered` outlived its purpose the same way and was
    /// deleted with the rest of the scaffolding.
    ///
    /// What is worth keeping is the property itself, which is about anything
    /// off the vocabulary rather than about those two: a form the evaluator
    /// does not know must say so and name the tag. A tag nobody declared
    /// exercises it exactly as well as a scaffolding one did, and needs no
    /// scaffolding to exist.
    #[test]
    fn a_tag_with_no_evaluation_names_itself() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(no-such-tag 1)").unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("`no-such-tag`")),
            "{:?}",
            e
        );
    }

    // ---- data: construct / field-get / field-set -------------------------

    /// A struct is a mutable box, so `field-set` is visible through any other
    /// reference to the same value — the property that makes `setf` on a
    /// struct field mean anything.
    #[test]
    fn a_struct_is_built_read_and_written_through_the_same_box() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, "(field-get (construct point \"point\" 0 true (int-any-width int-any-width) (int-any-width 1) (int-any-width 2)) 1 int-any-width)"),
            Value::Int(2)
        );
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((p sexpr (construct point \"point\" 0 true (int-any-width int-any-width) (int-any-width 1) (int-any-width 2))))
                   (let ((q sexpr (var p)))
                     (field-set (var q) 0 int-any-width (int-any-width 9))
                     (field-get (var p) 0 int-any-width)))",
            ),
            Value::Int(9)
        );
    }

    /// `field-set`'s object has to outlive the evaluation of its value, which
    /// allocates. Nothing else refers to the box.
    #[test]
    fn field_set_survives_a_collection_while_its_value_is_evaluated() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((p sexpr (construct point \"point\" 0 true (int-any-width int-any-width) (int-any-width 1) (int-any-width 2))))
                   (field-set (var p) 0 sexpr (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 8) (unit)))
                   (call (sexpr-car) () sexpr-car (sexpr) (field-get (var p) 0 int-any-width)))",
            ),
            Value::Int(8)
        );
    }

    /// An enum is a different box kind with a variant tag, and `MUTABLE` is
    /// what tells the two apart at the construct site.
    #[test]
    fn an_enum_carries_its_variant_and_fields() {
        let mut h = stress_heap();
        let v = eval_ok(&mut h, "(construct my-enum \"my-enum\" 2 false (int-any-width bool) (int-any-width 41) (bool true))");
        match v {
            Value::Boxed(id) => {
                assert!(h.is_enum(id), "expected an enum box");
                assert_eq!(h.enum_variant(id), 2);
                assert_eq!(h.enum_field_count(id), 2);
                assert_eq!(h.enum_field(id, 0), Value::Int(41));
                assert_eq!(h.enum_field(id, 1), Value::Bool(true));
            }
            other => panic!("expected a box, got {:?}", other),
        }
    }

    /// `(construct sexpr N ...)` builds a datum rather than a box. The
    /// pass-through arms are the interesting ones: the argument box *is* the
    /// datum, which is what makes `eq` hold across the construction.
    #[test]
    fn constructing_a_sexpr_datum() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(construct sexpr \"sexpr\" 0 false ())"), Value::Empty);
        assert_eq!(eval_ok(&mut h, "(construct sexpr \"sexpr\" 1 false (int-any-width) (int-any-width 3))"), Value::Int(3));

        let v = eval_ok(&mut h, "(construct sexpr \"sexpr\" 7 false (sexpr sexpr) (int-any-width 1) (int-any-width 2))");
        assert_eq!(h.car(v).unwrap(), Value::Int(1));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(2));

        // A string field passes the very same `Value::Str` through, so the
        // datum is `eq` to the string it was built from.
        let v = eval_ok(
            &mut h,
            r#"(let ((s sexpr (str "hi"))) (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (var s) (construct sexpr "sexpr" 6 false (str) (var s))))"#,
        );
        assert_eq!(h.car(v).unwrap(), h.cdr(v).unwrap());
    }

    // ---- match -----------------------------------------------------------

    #[test]
    fn match_takes_the_first_arm_that_matches() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(match (int-any-width 2) int-any-width ((pat-lit (int-any-width 1)) (int-any-width 10)) ((pat-lit (int-any-width 2)) (int-any-width 20)) ((pat-wild) (int-any-width 99)))",
            ),
            Value::Int(20)
        );
        // The wildcard is reached only when the literals miss.
        assert_eq!(
            eval_ok(
                &mut h,
                "(match (int-any-width 7) int-any-width ((pat-lit (int-any-width 1)) (int-any-width 10)) ((pat-wild) (int-any-width 99)))",
            ),
            Value::Int(99)
        );
        // An earlier arm wins even when a later one would also match.
        assert_eq!(
            eval_ok(&mut h, "(match (int-any-width 1) int-any-width ((pat-wild) (int-any-width 10)) ((pat-lit (int-any-width 1)) (int-any-width 20)))"),
            Value::Int(10)
        );
    }

    #[test]
    fn a_bind_pattern_binds_the_whole_scrutinee() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(match (int-any-width 5) int-any-width ((pat-bind x) (var x)))"), Value::Int(5));
    }

    /// A literal pattern is by-value equality, and only for the three types
    /// the language has pattern syntax for. A string would compare by
    /// identity and so never match, which is why it is refused rather than
    /// compared.
    #[test]
    fn a_literal_pattern_that_cannot_be_compared_by_value_is_refused() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, r"(match (char #\a) char ((pat-lit (char #\a)) (int-any-width 1)) ((pat-wild) (int-any-width 2)))"),
            Value::Int(1)
        );
        let e = eval_src(&mut h, r#"(match (str "a") str ((pat-lit (str "a")) (int-any-width 1)))"#).unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("no by-value equality")),
            "{:?}",
            e
        );
    }

    #[test]
    fn a_ctor_pattern_destructures_a_struct_and_an_enum() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(match (construct point \"point\" 0 true (int-any-width int-any-width) (int-any-width 1) (int-any-width 2)) struct
                   ((pat-ctor point \"point\" 0 false (int-any-width int-any-width) (pat-bind a) (pat-bind b)) (var b)))",
            ),
            Value::Int(2)
        );
        assert_eq!(
            eval_ok(
                &mut h,
                "(match (construct my-enum \"my-enum\" 1 false (int-any-width) (int-any-width 42)) struct
                   ((pat-ctor my-enum \"my-enum\" 0 false (int-any-width) (pat-bind x)) (int-any-width 0))
                   ((pat-ctor my-enum \"my-enum\" 1 false (int-any-width) (pat-bind x)) (var x)))",
            ),
            Value::Int(42)
        );
    }

    /// The path guard. Two ADTs with the same variant index and field count
    /// are not the same type, and a variant index alone cannot tell them
    /// apart — the reason the pattern carries a path at all.
    #[test]
    fn a_ctor_pattern_does_not_match_a_same_shaped_other_type() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(match (construct point \"point\" 0 true (int-any-width int-any-width) (int-any-width 1)) struct
                   ((pat-ctor other \"other\" 0 false (int-any-width) (pat-bind a)) (int-any-width 10))
                   ((pat-wild) (int-any-width 99)))",
            ),
            Value::Int(99)
        );
    }

    #[test]
    fn a_sexpr_ctor_pattern_destructures_a_datum() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(match (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2)) sexpr
                   ((pat-ctor sexpr \"sexpr\" 0 false ()) (int-any-width 100))
                   ((pat-ctor sexpr \"sexpr\" 7 false (sexpr sexpr) (pat-bind a) (pat-bind d)) (var d)))",
            ),
            Value::Int(2)
        );
        assert_eq!(
            eval_ok(&mut h, "(match (unit) unit ((pat-ctor sexpr \"sexpr\" 0 false ()) (int-any-width 100)) ((pat-wild) (int-any-width 0)))"),
            Value::Int(100)
        );
    }

    /// The `Path` pattern is the only one that *builds* its binding rather
    /// than pointing into the scrutinee, so the fresh list has to be rooted
    /// while the rest of the match runs. Under `gc_stress` the body's own
    /// allocation is what would collect it.
    ///
    /// The scrutinee is handed in through the environment: a `Value::Path`
    /// has no literal spelling in this vocabulary yet — `quote` arrives with
    /// the next tag — and building the environment directly is what the
    /// evaluator does anyway.
    #[test]
    fn a_path_pattern_binds_a_freshly_built_segment_list() {
        let mut h = stress_heap();
        let a = match h.intern_symbol("m") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let b = match h.intern_symbol("f") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let path = h.intern_path(&[a, b]);
        h.push_root(path);
        let name = match h.intern_symbol("p") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let env = extend_env(&mut h, &[(name, path)], Value::Empty).unwrap();
        h.push_root(env);

        let form = read1(
            &mut h,
            "(match (var p) sexpr ((pat-ctor sexpr \"sexpr\" 10 false (sexpr) (pat-bind segs))
               (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 0) (var segs))))",
        );
        h.push_root(form);

        let interp = Interp::new();
        let v = interp.eval_core(&mut h, form, env).expect("path match failed");
        // `(0 m f)` — the segments, in written order, behind the marker.
        assert_eq!(h.car(v).unwrap(), Value::Int(0));
        let segs = h.cdr(v).unwrap();
        assert_eq!(h.car(segs).unwrap(), Value::Symbol(a));
        let rest = h.cdr(segs).unwrap();
        assert_eq!(h.car(rest).unwrap(), Value::Symbol(b));
        assert_eq!(h.cdr(rest).unwrap(), Value::Empty);
    }

    /// Two `Path` patterns in one arm: the second one's allocation is what
    /// collects the first one's binding if that binding's root does not
    /// outlive the sub-pattern that made it.
    ///
    /// This is the case the single-`Path` test above cannot see. There, the
    /// freshly built list happens to survive because the very next allocation
    /// is the binding cell that already holds it. Put a second allocating
    /// pattern in between and that accident is gone.
    #[test]
    fn two_path_patterns_in_one_arm_both_keep_their_bindings() {
        let mut h = stress_heap();
        let sym = |h: &mut Heap, n: &str| match h.intern_symbol(n) {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let (a, b, c, d) = (sym(&mut h, "a"), sym(&mut h, "b"), sym(&mut h, "c"), sym(&mut h, "d"));
        let p1 = h.intern_path(&[a, b]);
        h.push_root(p1);
        let p2 = h.intern_path(&[c, d]);
        h.push_root(p2);
        let pair = h.cons(p1, p2).unwrap();
        h.push_root(pair);
        let name = sym(&mut h, "v");
        let env = extend_env(&mut h, &[(name, pair)], Value::Empty).unwrap();
        h.push_root(env);

        let form = read1(
            &mut h,
            "(match (var v) sexpr
               ((pat-ctor sexpr \"sexpr\" 7 false (sexpr sexpr)
                  (pat-ctor sexpr \"sexpr\" 10 false (sexpr) (pat-bind l))
                  (pat-ctor sexpr \"sexpr\" 10 false (sexpr) (pat-bind r)))
                 (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (var l) (var r))))",
        );
        h.push_root(form);

        let interp = Interp::new();
        let v = interp.eval_core(&mut h, form, env).expect("two-path match failed");
        // Both halves must still be the two-symbol lists they were built as.
        let left = h.car(v).unwrap();
        let right = h.cdr(v).unwrap();
        assert_eq!(h.car(left).unwrap(), Value::Symbol(a));
        assert_eq!(h.car(h.cdr(left).unwrap()).unwrap(), Value::Symbol(b));
        assert_eq!(h.car(right).unwrap(), Value::Symbol(c));
        assert_eq!(h.car(h.cdr(right).unwrap()).unwrap(), Value::Symbol(d));
    }

    /// A match arm's bindings live in a frame built exactly like a `let`'s,
    /// so they must survive a collection the arm body triggers.
    #[test]
    fn match_bindings_survive_a_collection_in_the_arm_body() {
        let mut h = stress_heap();
        let v = eval_ok(
            &mut h,
            "(match (construct point \"point\" 0 true (int-any-width int-any-width) (int-any-width 7) (int-any-width 8)) struct
               ((pat-ctor point \"point\" 0 false (int-any-width int-any-width) (pat-bind a) (pat-bind b))
                 (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2))
                 (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (var a) (var b))))",
        );
        assert_eq!(h.car(v).unwrap(), Value::Int(7));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(8));
    }

    #[test]
    fn a_match_with_no_matching_arm_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(match (int-any-width 1) int-any-width ((pat-lit (int-any-width 2)) (int-any-width 0)))").unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("no matching match arm")),
            "{:?}",
            e
        );
    }

    // ---- set / loop / break / return -------------------------------------

    #[test]
    fn set_writes_the_binding_and_yields_the_value() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, "(let ((x int-any-width (int-any-width 1))) (set x (int-any-width 9)))"),
            Value::Int(9)
        );
        assert_eq!(
            eval_ok(&mut h, "(let ((x int-any-width (int-any-width 1))) (set x (int-any-width 9)) (var x))"),
            Value::Int(9)
        );
    }

    /// The write goes through the binding *cell*, so it is visible from any
    /// scope that resolves to the same binding — an inner frame does not get
    /// a copy. That is what makes a closure capture and a compiled callee see
    /// the same value later.
    #[test]
    fn set_writes_through_the_cell_an_inner_scope_resolves_to() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((x int-any-width (int-any-width 1)))
                   (match (int-any-width 0) int-any-width ((pat-bind ignored) (set x (int-any-width 9))))
                   (var x))",
            ),
            Value::Int(9)
        );
    }

    #[test]
    fn setting_an_unbound_name_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(set nope (int-any-width 1))").unwrap_err();
        assert!(matches!(e.kind(), EvalError::Unbound(n) if n == "nope"), "{:?}", e);
    }

    #[test]
    fn break_leaves_a_loop_with_unit_and_return_with_a_value() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(loop unit (break))"), Value::Empty);
        assert_eq!(eval_ok(&mut h, "(loop int-any-width (return (int-any-width 7)))"), Value::Int(7));
        assert_eq!(eval_ok(&mut h, "(loop unit (return))"), Value::Empty);
        // Forms after the exit do not run.
        assert_eq!(
            eval_ok(&mut h, r#"(loop unit (break) (panic (str "ran past the break")))"#),
            Value::Empty
        );
    }

    /// A loop that actually iterates: walk a list, keeping the last element.
    /// Arithmetic is an instance method rather than a free function, and
    /// `assoc` has no evaluation yet — so the counter is a list instead.
    #[test]
    fn a_loop_repeats_its_body_until_something_exits() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((xs sexpr (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1)
                                   (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 2)
                                     (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 3) (unit)))))
                       (last sexpr (unit)))
                   (loop unit
                     (if (call (sexpr-null) () sexpr-null (sexpr) (var xs)) (break) (unit))
                     (set last (call (sexpr-car) () sexpr-car (sexpr) (var xs)))
                     (set xs (call (sexpr-cdr) () sexpr-cdr (sexpr) (var xs))))
                   (var last))",
            ),
            Value::Int(3)
        );
    }

    /// CL semantics: `break` leaves the *nearest* enclosing loop and no
    /// further. If it escaped further, the `set` below would never run.
    #[test]
    fn break_leaves_only_the_nearest_loop() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((n sexpr (unit)))
                   (loop unit
                     (loop unit (break))
                     (set n (int-any-width 5))
                     (break))
                   (var n))",
            ),
            Value::Int(5)
        );
        // The same for `return`, whose value belongs to the inner loop.
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((n sexpr (unit)))
                   (loop unit
                     (set n (loop int-any-width (return (int-any-width 4))))
                     (break))
                   (var n))",
            ),
            Value::Int(4)
        );
    }

    /// A loop is not a tail jump — the body repeats — so its iterations must
    /// not accumulate roots. Ten thousand allocating iterations under
    /// `gc_stress` is what would show it.
    #[test]
    fn a_long_running_loop_does_not_grow_the_root_stack() {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_growth_limit(1 << 20);
        h.set_gc_stress(true);

        let form = read1(
            &mut h,
            "(let ((xs sexpr (unit)) (n sexpr (unit)))
               (loop unit
                 (if (call (sexpr-null) () sexpr-null (sexpr) (var n)) (unit) (break))
                 (set xs (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (unit)))
                 (set n (call (sexpr-car) () sexpr-car (sexpr) (var xs)))))",
        );
        h.push_root(form);
        let before = h.root_count();
        let interp = Interp::new();
        interp.eval_core(&mut h, form, Value::Empty).expect("loop failed");
        assert_eq!(h.root_count(), before, "an iteration leaked a root");
    }

    // ---- closures --------------------------------------------------------

    #[test]
    fn a_lambda_is_applied_to_its_arguments() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, "(apply (lambda ((x int-any-width)) int-any-width (var x)) int-any-width (int-any-width) (int-any-width 5))"),
            Value::Int(5)
        );
        assert_eq!(
            eval_ok(&mut h, "(apply (lambda () int-any-width (int-any-width 7)) int-any-width () )"),
            Value::Int(7)
        );
        // Parameters shadow an outer binding of the same name.
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((x int-any-width (int-any-width 1))) (apply (lambda ((x int-any-width)) int-any-width (var x)) int-any-width (int-any-width) (int-any-width 2)))",
            ),
            Value::Int(2)
        );
    }

    /// Lexical, not dynamic: the body runs in the environment the `lambda`
    /// was *written* in, not the one it is called from. The caller here binds
    /// the same name to a different value, and the closure must not see it.
    #[test]
    fn a_closure_captures_its_defining_environment() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((n int-any-width (int-any-width 10)))
                   (let ((f sexpr (lambda () int-any-width (var n))))
                     (let ((n int-any-width (int-any-width 99)))
                       (apply (var f) int-any-width ()))))",
            ),
            Value::Int(10)
        );
    }

    /// The capture is the binding *cell*, so a later `set` through any
    /// reference to that binding is visible inside the closure. A copy would
    /// give 1 here.
    #[test]
    fn a_capture_shares_the_binding_rather_than_copying_it() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((n int-any-width (int-any-width 1)))
                   (let ((f sexpr (lambda () int-any-width (var n))))
                     (set n (int-any-width 42))
                     (apply (var f) int-any-width ())))",
            ),
            Value::Int(42)
        );
    }

    #[test]
    fn applying_the_wrong_number_of_arguments_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(apply (lambda ((x int-any-width)) int-any-width (var x)) int-any-width ())").unwrap_err();
        assert!(matches!(e.kind(), EvalError::Internal(m) if m.contains("arity mismatch")), "{:?}", e);
    }

    #[test]
    fn applying_something_that_is_not_a_function_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(apply (int-any-width 1) int-any-width ())").unwrap_err();
        assert!(matches!(e.kind(), EvalError::Internal(m) if m.contains("not a function")), "{:?}", e);
    }

    /// `labels` siblings can see each other, which is the whole reason the
    /// frame is built with empty cells before any closure exists.
    #[test]
    fn labels_siblings_can_call_each_other() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(
                &mut h,
                "(labels ((f ((x int-any-width)) int-any-width (apply (var g) int-any-width (int-any-width) (var x)))
                          (g ((y int-any-width)) int-any-width (var y)))
                   (apply (var f) int-any-width (int-any-width) (int-any-width 3)))",
            ),
            Value::Int(3)
        );
        // And a sibling can call itself.
        assert_eq!(
            eval_ok(
                &mut h,
                "(labels ((f ((xs sexpr)) sexpr
                            (if (call (sexpr-null) () sexpr-null (sexpr) (var xs))
                                (int-any-width 0)
                                (apply (var f) sexpr (sexpr) (call (sexpr-cdr) () sexpr-cdr (sexpr) (var xs))))))
                   (apply (var f) sexpr (sexpr) (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1)
                                    (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 2) (unit)))))",
            ),
            Value::Int(0)
        );
    }

    /// A tail call is a jump. The old evaluator entered a closure through a
    /// real native call, so a hundred thousand of these would not have been
    /// expressible at all — here the depth costs nothing.
    ///
    /// Driven by consing a list down to nil, since arithmetic is an instance
    /// method and `assoc` has no evaluation yet.
    #[test]
    fn a_tail_call_does_not_grow_the_stack() {
        const DEPTH: usize = 100_000;
        let mut h = Heap::with_capacity(1 << 18);
        h.set_growth_limit(1 << 22);

        // Built with stress off — the setup is `DEPTH` conses and collecting
        // at each would be quadratic. The recursion below is what is tested.
        let mut xs = Value::Empty;
        h.push_root(xs);
        for _ in 0..DEPTH {
            xs = h.cons(Value::Int(1), xs).unwrap();
            h.push_root(xs);
        }
        let name = match h.intern_symbol("xs") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let env = extend_env(&mut h, &[(name, xs)], Value::Empty).unwrap();
        h.push_root(env);

        let form = read1(
            &mut h,
            "(labels ((walk ((l sexpr)) sexpr
                        (if (call (sexpr-null) () sexpr-null (sexpr) (var l))
                            (int-any-width 0)
                            (apply (var walk) sexpr (sexpr) (call (sexpr-cdr) () sexpr-cdr (sexpr) (var l))))))
               (apply (var walk) sexpr (sexpr) (var xs)))",
        );
        h.push_root(form);

        let before = h.root_count();
        let interp = Interp::new();
        let v = interp.eval_core(&mut h, form, env).expect("deep tail recursion failed");
        assert_eq!(v, Value::Int(0));
        assert_eq!(h.root_count(), before, "a tail call grew the root stack");
    }

    /// A closure keeps its own code alive.
    ///
    /// The loop roots the form it is evaluating, so for as long as the
    /// `lambda` node is being walked its body is reachable that way — which
    /// is why this test deliberately gets rid of the form. It replaces the
    /// form's root with the closure, collects, and only then calls it. If the
    /// mark phase did not trace `params`/`body` inside the box, the body
    /// cells would be on the free list by then.
    #[test]
    fn a_closure_body_survives_collection_after_its_form_is_gone() {
        let mut h = Heap::with_capacity(1 << 12);
        // Taken *before* the read: `Reader::read_all` roots every form it
        // returns and leaves the root in place, so the form outlives any
        // slot this test manages itself. Truncating back to here is the only
        // way to actually let go of it — a `set_root` on a later slot leaves
        // the reader's own root holding the form, and the test then proves
        // nothing.
        let base = h.root_count();
        let form = read1(&mut h, "(lambda ((x int-any-width)) int-any-width (var x))");

        let interp = Interp::new();
        let f = interp.eval_core(&mut h, form, Value::Empty).expect("lambda failed");
        // The form is now referenced by nothing but the closure's own field.
        h.truncate_roots(base);
        h.push_root(f);

        // Deliberate garbage, so the collection below has something to
        // reclaim and the assertion can show it actually ran. Without it the
        // count proves nothing: most of the form's cells stay live *through
        // the closure*, which is the very thing under test, so a reclaim
        // count of zero is what a correct implementation gives here.
        const GARBAGE: usize = 50;
        for _ in 0..GARBAGE {
            h.cons(Value::Int(0), Value::Empty).unwrap();
        }
        let reclaimed = h.gc();
        assert!(reclaimed >= GARBAGE, "only {} cells were reclaimed — the collection did not run", reclaimed);

        // Call it through a fresh form, in a fresh environment.
        let name = match h.intern_symbol("g") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let env = extend_env(&mut h, &[(name, f)], Value::Empty).unwrap();
        h.push_root(env);
        let call = read1(&mut h, "(apply (var g) int-any-width (int-any-width) (int-any-width 5))");
        h.push_root(call);
        h.gc();

        let v = interp.eval_core(&mut h, call, env).expect("calling the closure failed");
        assert_eq!(v, Value::Int(5));
    }

    /// A built-in used as a function value is dispatched *by name* — the box
    /// carries only which name (and, for a method, which receiver type), so it
    /// goes through the very same `eval_builtin_method` a direct `(+ a b)` call
    /// site does. `+` is a method on `i32`, not a free builtin, so the box
    /// carries the receiver type.
    ///
    /// This test used to assert the opposite: that `apply` *refused* a
    /// compiled closure and a built-in value, naming which kind each was. Both
    /// refusals were Stage A scaffolding, and both are now real evaluation. The
    /// compiled-closure half cannot be tested here at all — it needs a genuine
    /// function pointer, and a fabricated one is jumped to and segfaults rather
    /// than being rejected — so that case belongs to `compile_test`, which has
    /// real compiled code to hand.
    #[test]
    fn a_builtin_used_as_a_function_value_dispatches_by_name() {
        let mut h = stress_heap();
        let form = read1(&mut h, "(apply (var f) int-any-width (int-any-width int-any-width) (int-any-width 1) (int-any-width 2))");
        h.push_root(form);
        let recv = crate::types::intern_path_id(&mut h, &crate::Path::root("i32"));
        let plus = h.alloc_builtin_fn(Some(recv), "+", "i32");
        h.push_root(plus);
        let name = match h.intern_symbol("f") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let env = extend_env(&mut h, &[(name, plus)], Value::Empty).unwrap();
        h.push_root(env);

        let interp = Interp::new();
        assert_eq!(interp.eval_core(&mut h, form, env).expect("applying `+` failed"), Value::Int(3));
    }

    // ---- quote -----------------------------------------------------------

    #[test]
    fn quote_yields_its_datum() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(quote ())"), Value::Empty);
        assert_eq!(eval_ok(&mut h, "(quote 42)"), Value::Int(42));

        let v = eval_ok(&mut h, "(quote (a b))");
        h.push_root(v);
        assert_eq!(core::print(&h, v), "(a b)");
    }

    /// The same `quote` form yields the *same* object each time, where the old
    /// evaluator rebuilt a fresh copy from an owned Rust tree on every
    /// evaluation. Returning the node's own field is both cheaper and what CL
    /// specifies for a literal — and it is only possible because a core form
    /// is heap data the collector can see, which the old `Typed` body was not.
    #[test]
    fn a_quoted_datum_is_the_same_object_every_time() {
        let mut h = stress_heap();
        let form = read1(&mut h, "(let () (quote (a b)))");
        h.push_root(form);
        let interp = Interp::new();
        let a = interp.eval_core(&mut h, form, Value::Empty).unwrap();
        h.push_root(a);
        let b = interp.eval_core(&mut h, form, Value::Empty).unwrap();
        assert_eq!(a, b, "two evaluations of one quote form gave different objects");
    }

    // ---- trait objects ---------------------------------------------------

    /// Boxing interns the vtable, and `dyn-value` gets the concrete value
    /// back out — the round trip that makes a `:dyn` box a view rather than a
    /// conversion.
    #[test]
    fn a_trait_object_wraps_and_unwraps_its_value() {
        let mut h = stress_heap();
        let v = eval_ok(
            &mut h,
            r#"(dyn-new "point" shape ((point area)) () struct (construct point "point" 0 true (int-any-width int-any-width) (int-any-width 3)))"#,
        );
        h.push_root(v);
        match v {
            Value::Boxed(id) => assert!(h.is_dyn(id), "expected a trait object"),
            other => panic!("expected a box, got {:?}", other),
        }
        let inner = eval_ok(
            &mut h,
            r#"(field-get (dyn-value (dyn-new "point" shape ((point area)) () struct (construct point "point" 0 true (int-any-width int-any-width) (int-any-width 3)))) 0 int-any-width)"#,
        );
        assert_eq!(inner, Value::Int(3));
    }

    /// An upcast switches the view to a supertrait's table, keeping the same
    /// concrete value. The tables it can switch to are the ones interned at
    /// the boxing site, because that is the only place the concrete type is
    /// still known.
    #[test]
    fn an_upcast_switches_the_table_and_keeps_the_value() {
        let mut h = stress_heap();
        let v = eval_ok(
            &mut h,
            r#"(dyn-upcast named
                 (dyn-new "point" shape ((point area)) ((named ((point name)))) struct (construct point "point" 0 true (int-any-width int-any-width) (int-any-width 3))))"#,
        );
        h.push_root(v);
        let inner = match v {
            Value::Boxed(id) => {
                assert!(h.is_dyn(id), "expected a trait object");
                h.dyn_value(id)
            }
            other => panic!("expected a box, got {:?}", other),
        };
        h.push_root(inner);
        match inner {
            Value::Boxed(id) => assert_eq!(h.struct_field(id, 0), Value::Int(3)),
            other => panic!("expected the concrete struct, got {:?}", other),
        }
    }

    /// Upcasting to the trait a box already has is the identity, so a no-op
    /// view does not disturb the value's identity.
    ///
    /// The box needs a supertrait for this to be reachable at all: a box with
    /// none registers no upcast tables, and the checker only emits an upcast
    /// where one is admitted — so a self-upcast of a supertrait-less box is
    /// not a state a program can reach.
    #[test]
    fn upcasting_to_the_same_trait_returns_the_same_box() {
        let mut h = stress_heap();
        let form = read1(
            &mut h,
            r#"(let ((b sexpr (dyn-new "point" shape ((point area)) ((named ((point name))))
                                struct (construct point "point" 0 true (int-any-width int-any-width) (int-any-width 3)))))
                 (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (var b) (dyn-upcast shape (var b))))"#,
        );
        h.push_root(form);
        let interp = Interp::new();
        let v = interp.eval_core(&mut h, form, Value::Empty).unwrap();
        h.push_root(v);
        assert_eq!(h.car(v).unwrap(), h.cdr(v).unwrap());
    }

    /// A trait the boxing site never registered has no table, and that is a
    /// compiler bug rather than anything user code can provoke — so it says
    /// which vtable and which trait rather than guessing at a view.
    #[test]
    fn an_upcast_with_no_registered_table_says_so() {
        let mut h = stress_heap();
        let e = eval_src(
            &mut h,
            r#"(dyn-upcast never-registered
                 (dyn-new "point" shape ((point area)) ((named ((point name))))
                   struct (construct point "point" 0 true (int-any-width int-any-width) (int-any-width 3))))"#,
        )
        .unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("no supertrait table registered")),
            "{:?}",
            e
        );
    }

    #[test]
    fn dyn_value_of_something_that_is_not_a_trait_object_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(dyn-value (int-any-width 1))").unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("not a trait object")),
            "{:?}",
            e
        );
    }

    #[test]
    fn a_form_that_is_not_a_node_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(1 2 3)").unwrap_err();
        assert!(matches!(e.kind(), EvalError::Internal(m) if m.contains("not a core form")), "{:?}", e);
    }
}
