//! The evaluator over *core forms* — the cons-cell program representation
//! (`crate::check::core`) that replaces the Rust `Typed` tree.
//!
//! This is being built additively, one tag at a time, while the checker still
//! produces `Typed` and [`Interp::eval`](super::Interp::eval) still walks it.
//! Nothing routes here yet; the tests read hand-written core forms straight
//! through the reader and evaluate them, so each tag can be finished and
//! verified with the whole suite green. The two evaluators meet — and the old
//! one is deleted — in the single commit that switches the checker over.
//!
//! # Roots
//!
//! Every value here is a heap value and [`Heap::cons`] collects whenever the
//! free list is empty, so anything the evaluator is holding *in a Rust local*
//! is invisible to the collector unless it is rooted. Two things are rooted for
//! the whole of an [`eval_core`](super::Interp::eval_core) call, in two
//! dedicated slots the trampoline overwrites in place rather than pushing onto:
//!
//! - the **environment**, which the trampoline replaces as it descends;
//! - the **form**, so a body reached by a tail jump stays reachable even when
//!   it belongs to a different tree than the one the caller rooted.
//!
//! Overwriting (`set_root`) rather than pushing is what keeps a tail loop from
//! growing the root stack without bound — the whole point of trampolining.
//!
//! # Tail positions
//!
//! `if` branches, a `let` body's last form and (from Stage A4) a call's body
//! are *jumps*, not recursive calls: the loop reassigns `form`/`env` and
//! continues. The old evaluator special-cased only `Expr::If` to survive long
//! `cond` chains; here every tail position is constant-stack, so mutual
//! recursion through a tail call is too.
//!
//! # Environment
//!
//! ```text
//! env   : (frame ...)          a cons list, innermost frame first
//! frame : ((SYM . CELL) ...)   an assoc list
//! CELL  : Value::Boxed(BoxedObj::Cell)
//! ```
//!
//! Lookup compares interned [`SymId`]s, so it is a `u32` comparison rather
//! than the `String` comparison `Env`'s `Vec<(String, Slot)>` needs. Within a
//! frame the *most recently added* binding is found first, matching
//! `env_get`'s `.rev()`.

use typelisp_mem::{Heap, RootScope, SymId, Value};

use crate::check::core;
use crate::eval::value::EvalError;

use super::Interp;

/// A core form's operator, resolved from its tag symbol.
///
/// Only the tags that have a lowering *and* an evaluation live here. A tag
/// from the vocabulary that is not yet implemented is deliberately absent, so
/// reaching it is an internal error naming the tag rather than a silent
/// mis-evaluation — the same convention as `core::unlowered`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
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
    Lambda,
    Labels,
    Apply,
    Quote,
    DynNew,
    DynUpcast,
    DynValue,
}

impl Op {
    /// The single dispatch point from a tag's text to an operator.
    ///
    /// A `match` on `&str` rather than a table indexed by [`SymId`]: a symbol
    /// id is only meaningful relative to the heap that interned it, so a table
    /// keyed by one would need a heap identity to stay honest across the
    /// throwaway heaps the drivers build. Whether that is worth its cost is a
    /// question for the wall-clock comparison at the end of the phase, and
    /// this function is the whole seam it would be answered behind.
    fn from_name(name: &str) -> Option<Op> {
        Some(match name {
            "int" => Op::Int,
            "float" => Op::Float,
            "bignum" => Op::Bignum,
            "ratio" => Op::Ratio,
            "char" => Op::Char,
            "bool" => Op::Bool,
            "str" => Op::Str,
            "sym" => Op::Sym,
            "unit" => Op::Unit,
            "var" => Op::Var,
            "let" => Op::Let,
            "if" => Op::If,
            "call" => Op::Call,
            "panic" => Op::Panic,
            "construct" => Op::Construct,
            "field-get" => Op::FieldGet,
            "field-set" => Op::FieldSet,
            "match" => Op::Match,
            "set" => Op::Set,
            "loop" => Op::Loop,
            "break" => Op::Break,
            "return" => Op::Return,
            "lambda" => Op::Lambda,
            "labels" => Op::Labels,
            "apply" => Op::Apply,
            "quote" => Op::Quote,
            "dyn-new" => Op::DynNew,
            "dyn-upcast" => Op::DynUpcast,
            "dyn-value" => Op::DynValue,
            _ => return None,
        })
    }
}

/// What one step of the trampoline produced: either the answer, or the form
/// and environment to continue with in tail position.
enum Step {
    Done(Value),
    Tail(Value, Value),
}

impl Interp {
    /// Evaluate the core form `form` in environment `env`.
    ///
    /// `env` may be [`Value::Empty`] for a form with no free variables. Both
    /// arguments are rooted here for the duration, so a caller holding them
    /// only in a Rust local is safe; a caller that *keeps* them past the call
    /// must root them itself.
    pub(crate) fn eval_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let mut s = RootScope::new(heap);
        let base = s.base();
        s.push_root(env);
        s.push_root(form);

        let mut form = form;
        let mut env = env;
        loop {
            // Read the location before stepping: it names the form the error
            // came from, and `EvalError::at` keeps the innermost one, so a
            // deeper frame that already placed the error wins.
            let loc = s.code_loc(form);
            match self.step_core(&mut s, form, env) {
                Ok(Step::Done(v)) => return Ok(v),
                Ok(Step::Tail(next_form, next_env)) => {
                    form = next_form;
                    env = next_env;
                    s.set_root(base, env);
                    s.set_root(base + 1, form);
                }
                Err(e) => {
                    return Err(match loc {
                        Some(l) => e.at(l),
                        None => e,
                    })
                }
            }
        }
    }

    /// One step: evaluate `form` far enough to produce a value, or to reduce
    /// it to a tail form the caller's loop continues with.
    fn step_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Step, EvalError> {
        let tag = match heap.car(form) {
            Ok(Value::Symbol(id)) => id,
            _ => return Err(EvalError::Internal(format!("eval: not a core form: {}", core::print(heap, form)))),
        };
        let op = match Op::from_name(heap.symbol_name(tag)) {
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
            Op::Int | Op::Char | Op::Bool | Op::Sym => self.literal_field(heap, form).map(Step::Done),
            Op::Unit => Ok(Step::Done(Value::Empty)),
            Op::Float => {
                let f = match self.literal_field(heap, form)? {
                    Value::Boxed(id) if heap.is_float(id) => heap.float_value(id),
                    other => return Err(EvalError::Internal(format!("eval: (float ..) holds {:?}", other))),
                };
                Ok(Step::Done(heap.alloc_float(f)))
            }
            Op::Bignum => {
                let n = match self.literal_field(heap, form)? {
                    Value::Boxed(id) if heap.is_bignum(id) => heap.bignum_value(id).clone(),
                    other => return Err(EvalError::Internal(format!("eval: (bignum ..) holds {:?}", other))),
                };
                Ok(Step::Done(heap.alloc_bignum(n)))
            }
            Op::Ratio => {
                let r = match self.literal_field(heap, form)? {
                    Value::Boxed(id) if heap.is_ratio(id) => heap.ratio_value(id).clone(),
                    other => return Err(EvalError::Internal(format!("eval: (ratio ..) holds {:?}", other))),
                };
                Ok(Step::Done(heap.alloc_ratio(r)))
            }
            Op::Str => {
                let s = match self.literal_field(heap, form)? {
                    Value::Str(id) => heap.string(id).to_string(),
                    other => return Err(EvalError::Internal(format!("eval: (str ..) holds {:?}", other))),
                };
                Ok(Step::Done(heap.alloc_string(s)))
            }

            // ---- variables -----------------------------------------------
            Op::Var => {
                let sym = self.sym_field(heap, form, 0, "var")?;
                match env_lookup(heap, env, sym) {
                    Some(cell) => Ok(Step::Done(cell_value(heap, cell)?)),
                    None => Err(EvalError::Unbound(heap.symbol_name(sym).to_string())),
                }
            }

            // ---- binding and control -------------------------------------
            Op::Let => {
                let binds = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (let ..) has no binding list".to_string()))?;
                // The extended environment is deliberately *not* rooted here.
                // It only ever goes two places, and both root it before they
                // can allocate: into `eval_core`, whose first act is to root
                // it, or out as the tail step, which the trampoline roots
                // with nothing allocating in between. Adding a root here
                // would look like protection against a window that does not
                // exist. `a_let_frame_survives_a_collection_in_an_earlier_
                // body_form` is what would notice if that ever stopped being
                // true.
                let env = self.extend_let(heap, binds, env)?;
                // `(let () E...)` is `progn`, so an empty body is the empty
                // sequence: unit, exactly as `eval_seq` returns for one.
                let body = core::fields(heap, form)
                    .map_err(|e| EvalError::Internal(format!("eval: (let ..) body: {}", e)))?;
                let body = &body[1..];
                let Some((last, rest)) = body.split_last() else {
                    return Ok(Step::Done(Value::Empty));
                };
                for e in rest {
                    self.eval_core(heap, *e, env)?;
                }
                Ok(Step::Tail(*last, env))
            }
            Op::If => {
                let cond = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (if ..) has no condition".to_string()))?;
                let taken = match self.eval_core(heap, cond, env)? {
                    Value::Bool(true) => 1,
                    Value::Bool(false) => 2,
                    other => return Err(EvalError::Internal(format!("eval: (if ..) condition is {:?}", other))),
                };
                let branch = core::field(heap, form, taken)
                    .ok_or_else(|| EvalError::Internal("eval: (if ..) is missing a branch".to_string()))?;
                Ok(Step::Tail(branch, env))
            }
            Op::Panic => {
                let msg = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (panic ..) has no message".to_string()))?;
                // A non-string message is not reachable from a checked program
                // and carries nothing to report, so it panics with no text —
                // the old evaluator's behaviour, kept deliberately.
                match self.eval_core(heap, msg, env)? {
                    Value::Str(id) => Err(EvalError::Panic(heap.string(id).to_string())),
                    _ => Err(EvalError::Panic(String::new())),
                }
            }

            // ---- calls ---------------------------------------------------
            Op::Call => self.call_core(heap, form, env).map(Step::Done),

            // ---- data ----------------------------------------------------
            Op::Construct => self.construct_core(heap, form, env).map(Step::Done),
            Op::FieldGet => {
                let obj = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (field-get ..) has no object".to_string()))?;
                let idx = int_field(heap, form, 1, "field-get")? as usize;
                let obj = self.eval_core(heap, obj, env)?;
                let id = super::expect_struct_box(&obj)?;
                // No decode. The old evaluator ran the stored word through
                // `decode_field_typed`, which needed the field's declared
                // type; with one value world left that function is the
                // identity, so the field *is* the value.
                Ok(Step::Done(heap.struct_field(id, idx)))
            }
            Op::FieldSet => {
                let obj = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (field-set ..) has no object".to_string()))?;
                let idx = int_field(heap, form, 1, "field-set")? as usize;
                let val = core::field(heap, form, 2)
                    .ok_or_else(|| EvalError::Internal("eval: (field-set ..) has no value".to_string()))?;
                let mut s = RootScope::new(heap);
                let obj = self.eval_core(&mut s, obj, env)?;
                // The box has to stay reachable while the value expression
                // runs — that expression can allocate, and nothing else
                // refers to the box.
                s.push_root(obj);
                let v = self.eval_core(&mut s, val, env)?;
                let id = super::expect_struct_box(&obj)?;
                s.struct_set_field(id, idx, v);
                Ok(Step::Done(Value::Empty))
            }
            Op::Match => self.match_core(heap, form, env),

            // ---- assignment and loops ------------------------------------
            Op::Set => {
                let sym = self.sym_field(heap, form, 0, "set")?;
                let val = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (set ..) has no value".to_string()))?;
                let v = self.eval_core(heap, val, env)?;
                let cell = env_lookup(heap, env, sym)
                    .ok_or_else(|| EvalError::Unbound(heap.symbol_name(sym).to_string()))?;
                // Written through the cell rather than rebuilding the frame,
                // which is what makes the assignment visible through every
                // other reference to the same binding — a closure's capture,
                // or compiled code handed the same cell.
                match cell {
                    Value::Boxed(id) if heap.is_cell(id) => heap.cell_set(id, v),
                    other => return Err(EvalError::Internal(format!("eval: binding does not hold a cell: {:?}", other))),
                }
                Ok(Step::Done(v))
            }
            // `break`/`return` are not errors: they are non-local exits
            // riding `Result`'s propagation, so that every intervening frame
            // unwinds without each one having to know about them. The
            // enclosing `loop` is the only thing that catches them, and the
            // checker guarantees there is one.
            Op::Break => Err(EvalError::Break),
            Op::Return => match core::field(heap, form, 0) {
                Some(val) => {
                    let v = self.eval_core(heap, val, env)?;
                    Err(EvalError::Return(Box::new(v)))
                }
                None => Err(EvalError::Return(Box::new(Value::Empty))),
            },
            // ---- closures ------------------------------------------------
            //
            // No JIT attempt. The old evaluator compiled every `lambda` and
            // `labels` sibling at definition time and raised a hard error if
            // the compiler declined — the closure had nowhere else to live,
            // since its body was a Rust AST the box could not hold. With the
            // body a core form there is somewhere, so the JIT becomes an
            // optimisation the checker's own driver applies rather than a
            // requirement of the representation. Re-attaching it is the
            // switch commit's business, not this stage's.
            Op::Lambda => {
                let params = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (lambda ..) has no parameter list".to_string()))?;
                // Field 1 is the return repr, for the bridge.
                let body = tail_after(heap, form, 2)?;
                // `alloc_closure` cannot collect (only `cons` does), so the
                // three values need no rooting across it.
                Ok(Step::Done(heap.alloc_closure(params, body, env)))
            }
            Op::Labels => self.labels_core(heap, form, env),
            Op::Apply => self.apply_core(heap, form, env),

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
            Op::Quote => self.literal_field(heap, form).map(Step::Done),

            // ---- trait objects -------------------------------------------
            Op::DynNew => self.dyn_new_core(heap, form, env).map(Step::Done),
            Op::DynUpcast => self.dyn_upcast_core(heap, form, env).map(Step::Done),
            Op::DynValue => {
                let inner = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (dyn-value ..) has no operand".to_string()))?;
                match self.eval_core(heap, inner, env)? {
                    Value::Boxed(id) if heap.is_dyn(id) => Ok(Step::Done(heap.dyn_value(id))),
                    other => Err(EvalError::Internal(format!("eval: (dyn-value ..): not a trait object: {:?}", other))),
                }
            }

            // Not a tail jump: the body repeats, so this is a real Rust loop
            // rather than a `Step::Tail`. Each iteration's `eval_core` balances
            // its own roots, so the root stack does not grow with the
            // iteration count.
            Op::Loop => {
                let body = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: (loop ..): {}", e)))?;
                loop {
                    for e in &body {
                        match self.eval_core(heap, *e, env) {
                            Ok(_) => {}
                            // The exit value travels in a Rust `Box`, where
                            // the collector cannot see it — safe only because
                            // unwinding allocates nothing: a `RootScope`'s
                            // drop truncates, it does not cons. The value is
                            // handed straight back to the trampoline, which
                            // roots it before anything else runs.
                            Err(EvalError::Break) => return Ok(Step::Done(Value::Empty)),
                            Err(EvalError::Return(v)) => return Ok(Step::Done(*v)),
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
        }
    }

    /// `(construct PATH N MUTABLE E...)`.
    ///
    /// Three shapes behind one tag, exactly as the old `Expr::Construct`:
    /// the built-in `Sexpr`, whose "fields" are really constructor arguments
    /// for a datum; a mutable `defstruct` box; and an enum (`Option`,
    /// `Result`, a user `defenum`). `MUTABLE` is what tells the last two
    /// apart — a struct is the mutable one.
    fn construct_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let path = path_field(heap, form, 0, "construct")?;
        let variant = int_field(heap, form, 1, "construct")? as usize;
        let mutable = bool_field(heap, form, 2, "construct")?;

        let arg_forms = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: (construct ..): {}", e)))?;
        if arg_forms.len() < 3 {
            return Err(EvalError::Internal(format!("eval: malformed construct: {}", core::print(heap, form))));
        }
        let arg_forms = arg_forms[3..].to_vec();

        let mut s = RootScope::new(heap);
        let mut argv = Vec::with_capacity(arg_forms.len());
        for a in &arg_forms {
            let v = self.eval_core(&mut s, *a, env)?;
            s.push_root(v);
            argv.push(v);
        }

        if super::is_sexpr_type(&path) {
            construct_sexpr_core(&mut s, variant, &argv)
        } else if mutable {
            // The old evaluator mapped each field through
            // `rtvalue_to_struct_field` first; that function is the identity
            // now, so the values go in as they are.
            Ok(crate::type_key::alloc_typed_struct(&mut s, &path, argv))
        } else {
            Ok(super::build_enum_value(&mut s, path, variant, argv))
        }
    }

    /// `(dyn-new STR PATH ((PATH SYM)...) ((PATH ((PATH SYM)...))...) E)`.
    ///
    /// Boxing is where the concrete type is still known, so it is where every
    /// vtable this value could ever be viewed through gets interned — its own
    /// and each supertrait's. An upcast later has only the runtime vtable id
    /// to go on, and could not re-derive them.
    fn dyn_new_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let concrete_key = match core::field(heap, form, 0) {
            Some(Value::Str(id)) => heap.string(id).to_string(),
            other => return Err(EvalError::Internal(format!("eval: (dyn-new ..) concrete key is {:?}", other))),
        };
        let trait_path = path_field(heap, form, 1, "dyn-new")?;
        let slots = method_list(heap, form, 2, "dyn-new")?;
        let supers = super_list(heap, form, 3)?;
        let value = core::field(heap, form, 4)
            .ok_or_else(|| EvalError::Internal("eval: (dyn-new ..) has no value".to_string()))?;

        let v = self.eval_core(heap, value, env)?;
        let mut s = RootScope::new(heap);
        // `alloc_dyn` cannot collect, but `register_dyn_box` reaches the
        // compiler for a trait some compiled body dispatches on, and that
        // can. The boxed value is reachable from nothing else meanwhile.
        s.push_root(v);
        let ids = self.register_dyn_box(&concrete_key, &trait_path, &slots, &supers);
        let id = *ids
            .first()
            .ok_or_else(|| EvalError::Internal("eval: (dyn-new ..) registered no vtable".to_string()))?;
        self.publish_vtable(id);
        Ok(s.alloc_dyn(id, v))
    }

    /// `(dyn-upcast PATH E)` — view a trait object through a supertrait.
    ///
    /// The concrete type is gone by now, so the target table is found through
    /// the map the boxing site filled in, keyed by the runtime vtable id. A
    /// miss is a compiler bug rather than anything user code can provoke: the
    /// boxing site registers a table for every trait the checker admits an
    /// upcast to.
    fn dyn_upcast_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let to_trait = path_field(heap, form, 0, "dyn-upcast")?;
        let value = core::field(heap, form, 1)
            .ok_or_else(|| EvalError::Internal("eval: (dyn-upcast ..) has no operand".to_string()))?;
        let v = self.eval_core(heap, value, env)?;

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

    /// `(labels ((SYM ((SYM R)...) RET-R E...) ...) E...)`.
    ///
    /// The siblings are mutually recursive, so each one has to capture an
    /// environment that already contains all of them. That is not a cycle to
    /// tie off after the fact: the frame is built first with an empty *cell*
    /// per name, so the environment is complete before any closure is made,
    /// and filling a cell afterwards is an ordinary write the closures see
    /// because they share it. It is the same mechanism `set` uses.
    fn labels_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Step, EvalError> {
        let defs = core::field(heap, form, 0)
            .ok_or_else(|| EvalError::Internal("eval: (labels ..) has no definitions".to_string()))?;
        let defs = heap
            .list_to_vec(defs)
            .map_err(|e| EvalError::Internal(format!("eval: (labels ..) definitions: {}", e)))?;

        let mut s = RootScope::new(heap);
        let placeholders: Vec<(SymId, Value)> = defs
            .iter()
            .map(|d| match s.car(*d) {
                Ok(Value::Symbol(sym)) => Ok((sym, Value::Empty)),
                other => Err(EvalError::Internal(format!("eval: (labels ..) definition names {:?}", other))),
            })
            .collect::<Result<_, _>>()?;
        let env = extend_env(&mut s, &placeholders, env)?;
        s.push_root(env);

        for d in &defs {
            // A definition is `(SYM PARAMS RET-R E...)` — positional, like a
            // `let` binding and unlike a tagged node.
            let (sym, _) = match s.car(*d) {
                Ok(Value::Symbol(sym)) => (sym, ()),
                other => return Err(EvalError::Internal(format!("eval: (labels ..) definition names {:?}", other))),
            };
            let rest = s.cdr(*d).map_err(heap_err)?;
            let params = s.car(rest).map_err(heap_err)?;
            let body = tail_after_value(&s, rest, 2)?;
            let f = s.alloc_closure(params, body, env);
            let cell = env_lookup(&s, env, sym)
                .ok_or_else(|| EvalError::Internal(format!("eval: (labels ..) lost the slot for `{}`", s.symbol_name(sym))))?;
            match cell {
                Value::Boxed(id) if s.is_cell(id) => s.cell_set(id, f),
                other => return Err(EvalError::Internal(format!("eval: (labels ..) slot is {:?}", other))),
            }
        }

        let body = tail_after(&mut s, form, 1)?;
        let body = s
            .list_to_vec(body)
            .map_err(|e| EvalError::Internal(format!("eval: (labels ..) body: {}", e)))?;
        let Some((last, rest)) = body.split_last() else {
            return Ok(Step::Done(Value::Empty));
        };
        for e in rest {
            self.eval_core(&mut s, *e, env)?;
        }
        Ok(Step::Tail(*last, env))
    }

    /// `(apply E E...)` — call the value `E` produces.
    ///
    /// An interpreted closure's body is entered as a *tail jump*, so a
    /// self-call or a mutual call in tail position costs no stack. The old
    /// evaluator could not do that: a closure was always native code, and
    /// entering it meant a real call.
    fn apply_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Step, EvalError> {
        let callee = core::field(heap, form, 0)
            .ok_or_else(|| EvalError::Internal("eval: (apply ..) has no callee".to_string()))?;
        let arg_forms = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: (apply ..): {}", e)))?;
        let arg_forms = arg_forms[1..].to_vec();

        let mut s = RootScope::new(heap);
        let f = self.eval_core(&mut s, callee, env)?;
        // An unnamed callee — `((make-adder 1) 2)` — has no binding keeping
        // its box alive while the arguments allocate.
        s.push_root(f);
        let mut argv = Vec::with_capacity(arg_forms.len());
        for a in &arg_forms {
            let v = self.eval_core(&mut s, *a, env)?;
            s.push_root(v);
            argv.push(v);
        }

        let id = match f {
            Value::Boxed(id) if s.is_closure(id) => id,
            Value::Boxed(id) if s.is_compiled_closure(id) => {
                return Err(EvalError::Internal(
                    "eval: applying a compiled closure: not until the JIT is re-attached".to_string(),
                ))
            }
            Value::Boxed(id) if s.is_builtin_fn(id) => {
                return Err(EvalError::Internal(
                    "eval: applying a built-in used as a function value: not until the checker lowers function references"
                        .to_string(),
                ))
            }
            other => return Err(EvalError::Internal(format!("eval: (apply ..) callee is not a function: {:?}", other))),
        };

        let (params, body, closure_env) = s.closure_parts(id);
        let names = param_names(&s, params)?;
        if names.len() != argv.len() {
            return Err(EvalError::Internal(format!(
                "eval: arity mismatch: the closure takes {} argument(s), given {}",
                names.len(),
                argv.len()
            )));
        }
        let binds: Vec<(SymId, Value)> = names.into_iter().zip(argv).collect();
        // Over the *captured* environment, not the caller's: that is what
        // makes this lexical scope rather than dynamic.
        let call_env = extend_env(&mut s, &binds, closure_env)?;
        s.push_root(call_env);

        let body = s
            .list_to_vec(body)
            .map_err(|e| EvalError::Internal(format!("eval: closure body: {}", e)))?;
        let Some((last, rest)) = body.split_last() else {
            return Ok(Step::Done(Value::Empty));
        };
        for e in rest {
            self.eval_core(&mut s, *e, call_env)?;
        }
        Ok(Step::Tail(*last, call_env))
    }

    /// `(match E (P E...) ...)` — the first arm whose pattern matches wins.
    fn match_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Step, EvalError> {
        let scrut = core::field(heap, form, 0)
            .ok_or_else(|| EvalError::Internal("eval: (match ..) has no scrutinee".to_string()))?;
        let v = self.eval_core(heap, scrut, env)?;

        let mut s = RootScope::new(heap);
        // The scrutinee is reachable from nothing else, and matching a
        // `Sexpr` path pattern allocates, so it is rooted for the whole
        // search rather than just across one arm.
        s.push_root(v);
        let arms = core::fields(&s, form).map_err(|e| EvalError::Internal(format!("eval: (match ..): {}", e)))?;

        for arm in &arms[1..] {
            let parts = s
                .list_to_vec(*arm)
                .map_err(|e| EvalError::Internal(format!("eval: (match ..) arm: {}", e)))?;
            let Some((pat, body)) = parts.split_first() else {
                return Err(EvalError::Internal("eval: (match ..) arm is empty".to_string()));
            };
            let Some(binds) = match_core_pattern(&mut s, *pat, v)? else {
                continue;
            };
            let env = extend_env(&mut s, &binds, env)?;
            let Some((last, rest)) = body.split_last() else {
                return Ok(Step::Done(Value::Empty));
            };
            for e in rest {
                self.eval_core(&mut s, *e, env)?;
            }
            return Ok(Step::Tail(*last, env));
        }
        Err(EvalError::Internal("eval: no matching match arm".to_string()))
    }

    /// A one-field literal node's payload.
    fn literal_field(&self, heap: &Heap, form: Value) -> Result<Value, EvalError> {
        core::field(heap, form, 0).ok_or_else(|| EvalError::Internal("eval: literal has no value".to_string()))
    }

    /// Field `i` of `form`, which must be a symbol.
    fn sym_field(&self, heap: &Heap, form: Value, i: usize, what: &str) -> Result<SymId, EvalError> {
        match core::field(heap, form, i) {
            Some(Value::Symbol(id)) => Ok(id),
            other => Err(EvalError::Internal(format!("eval: ({} ..) field {} is not a symbol: {:?}", what, i, other))),
        }
    }

    /// Build the frame a `(let ((SYM R E) ...) ...)` introduces and return the
    /// extended environment.
    ///
    /// CL `let`: every initialiser is evaluated in the *outer* environment, so
    /// all of them run before the frame is linked in. The repr tag `R` is for
    /// the compiler bridge and is not read here — with one value world left,
    /// every binding holds the same thing.
    fn extend_let(&self, heap: &mut Heap, binds: Value, env: Value) -> Result<Value, EvalError> {
        let binds = heap
            .list_to_vec(binds)
            .map_err(|e| EvalError::Internal(format!("eval: (let ..) binding list: {}", e)))?;
        // `progn` — a `let` with no bindings — extends nothing. Skipping the
        // empty frame is not just an optimisation: `progn` is how every body
        // sequence is spelled, so consing one per body would put an allocation
        // (and so a possible collection) on a path that has no reason to have
        // one, and would grow the environment a lookup has to walk.
        if binds.is_empty() {
            return Ok(env);
        }

        let mut s = RootScope::new(heap);
        let mut pairs = Vec::with_capacity(binds.len());
        for b in &binds {
            // A binding is `(SYM R E)` — a plain three-element list, not a
            // tagged node, so its elements are read positionally.
            // `core::field` would skip the first as a tag.
            let parts = s
                .list_to_vec(*b)
                .map_err(|e| EvalError::Internal(format!("eval: (let ..) binding: {}", e)))?;
            let [name, _repr, init] = parts[..] else {
                return Err(EvalError::Internal(format!(
                    "eval: (let ..) binding is not (SYM R E): {}",
                    core::print(&s, *b)
                )));
            };
            let Value::Symbol(sym) = name else {
                return Err(EvalError::Internal(format!("eval: (let ..) binding name is {:?}", name)));
            };
            let v = self.eval_core(&mut s, init, env)?;
            // Rooted for the rest of the initialisers, not just across the
            // next allocation: a value bound three initialisers ago is just
            // as collectible as this one.
            s.push_root(v);
            pairs.push((sym, v));
        }
        extend_env(&mut s, &pairs, env)
    }

    /// `(call WRITTEN HOME PATH ARG...)`.
    ///
    /// Only built-in operators are reachable so far. A call to a user-defined
    /// function needs `FnDef::body` to be a core form, which happens in the
    /// commit that switches the checker over; until then this says so rather
    /// than reporting the function as missing. The call *machinery* — argument
    /// evaluation, frames, the tail jump — is exercised through `labels` and
    /// `lambda`, so nothing about it is left untested by the wait.
    fn call_core(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let written = self.name_list(heap, form, 0, "call")?;
        let home = self.name_list(heap, form, 1, "call")?;

        let arg_forms = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: (call ..): {}", e)))?;
        // written, home, path, then the arguments.
        if arg_forms.len() < 3 {
            return Err(EvalError::Internal(format!("eval: malformed call: {}", core::print(heap, form))));
        }
        let arg_forms = &arg_forms[3..];

        let mut s = RootScope::new(heap);
        let mut argv = Vec::with_capacity(arg_forms.len());
        for a in arg_forms {
            let v = self.eval_core(&mut s, *a, env)?;
            // Rooted for the rest of the argument evaluation *and* the call:
            // an earlier argument is just as collectible as the one being
            // built now.
            s.push_root(v);
            argv.push(v);
        }

        if self.root.borrow().resolve_fn(&home, &written).is_some() {
            let name = written.join("::");
            return Err(EvalError::Internal(format!(
                "eval: call of the user-defined function `{}`: not until the checker lowers function bodies",
                name
            )));
        }
        if written.len() == 1 {
            if let Some(result) = self.eval_builtin(&mut s, &written[0], &argv) {
                return result;
            }
        }
        Err(EvalError::NoSuchFunction(written.join("::")))
    }

    /// A field holding a list of symbols (`written`, `home`), as strings.
    fn name_list(&self, heap: &Heap, form: Value, i: usize, what: &str) -> Result<Vec<String>, EvalError> {
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
fn extend_env(heap: &mut Heap, binds: &[(SymId, Value)], env: Value) -> Result<Value, EvalError> {
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
        let pair = s.cons(Value::Symbol(*sym), cell).map_err(heap_err)?;
        s.push_root(pair);
        frame = s.cons(pair, frame).map_err(heap_err)?;
        s.push_root(frame);
    }
    s.cons(frame, env).map_err(heap_err)
}

/// Match `pat` against `v`, returning the bindings it makes, or `None` if it
/// does not match.
///
/// Each bound value is rooted as it is collected, and stays rooted until the
/// caller's scope closes. That is not belt-and-braces: the `Path` pattern
/// builds its binding *fresh* rather than pointing into the scrutinee, so a
/// later sub-pattern's allocation would collect it.
fn match_core_pattern(heap: &mut Heap, pat: Value, v: Value) -> Result<Option<Vec<(SymId, Value)>>, EvalError> {
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
            let want = match core::op(heap, lit) {
                Some("int") | Some("bool") | Some("char") => core::field(heap, lit, 0)
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
        "pat-ctor" => {
            let path = path_field(heap, pat, 0, "pat-ctor")?;
            let variant = int_field(heap, pat, 1, "pat-ctor")? as usize;
            // Field 2 is the downcast flag. The interpreter applies the type
            // guard below either way, so it changes nothing here; it is the
            // *compiled* side that emits an instance test only when it is set.
            let subs = core::fields(heap, pat).map_err(|e| EvalError::Internal(format!("eval: (pat-ctor ..): {}", e)))?;
            if subs.len() < 3 {
                return Err(EvalError::Internal(format!("eval: malformed pattern: {}", core::print(heap, pat))));
            }
            let subs = subs[3..].to_vec();
            match_ctor(heap, &path, variant, &subs, v)
        }
        "pat-typetest" => {
            let path = path_field(heap, pat, 0, "pat-typetest")?;
            let inner = core::field(heap, pat, 1)
                .ok_or_else(|| EvalError::Internal("eval: (pat-typetest ..) has no sub-pattern".to_string()))?;
            // `(the sexpr p)` tests nothing: every value is a `Sexpr`.
            if crate::types::path_is_builtin(&path, "sexpr") {
                return match_core_pattern(heap, inner, v);
            }
            match v {
                Value::Boxed(id) if crate::type_key::heap_type_is(heap, id, &path) => match_core_pattern(heap, inner, v),
                _ => Ok(None),
            }
        }
        other => Err(EvalError::Internal(format!("eval: no matching for pattern `{}`", other))),
    }
}

/// `(pat-ctor PATH N DOWNCAST P...)` against `v`.
///
/// The `PATH` guard is what keeps two unrelated ADTs that happen to share a
/// shape apart once a heterogeneous `Sexpr` can hold either — a variant index
/// and a field count are not an identity. It is also why a genuine `Sexpr`
/// datum must only reach the `Sexpr` arm when `PATH` really is `sexpr`: that
/// arm indexes by *bare* variant number, where a struct's sole variant 0 would
/// spuriously match `Sexpr::Nil`.
fn match_ctor(
    heap: &mut Heap,
    path: &crate::Path,
    variant: usize,
    subs: &[Value],
    v: Value,
) -> Result<Option<Vec<(SymId, Value)>>, EvalError> {
    match v {
        Value::Boxed(id)
            if heap.is_enum(id)
                && crate::type_key::heap_type_is(heap, id, path)
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
                match match_core_pattern(heap, *p, f)? {
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
                && crate::type_key::heap_type_is(heap, id, path)
                && variant == 0
                && heap.struct_field_count(id) == subs.len() =>
        {
            let mut binds = Vec::new();
            for (i, p) in subs.iter().enumerate() {
                let f = heap.struct_field(id, i);
                match match_core_pattern(heap, *p, f)? {
                    Some(b) => binds.extend(b),
                    None => return Ok(None),
                }
            }
            Ok(Some(binds))
        }
        sv if *path == crate::Path::root("sexpr") => match_sexpr_core(heap, variant, subs, sv),
        _ => Ok(None),
    }
}

/// A `Sexpr` constructor pattern, destructured through the heap.
fn match_sexpr_core(
    heap: &mut Heap,
    variant: usize,
    subs: &[Value],
    v: Value,
) -> Result<Option<Vec<(SymId, Value)>>, EvalError> {
    use super::{SEXPR_BIGNUM, SEXPR_BOOL, SEXPR_CHAR, SEXPR_CONS, SEXPR_FLOAT, SEXPR_INT, SEXPR_NIL, SEXPR_PATH, SEXPR_RATIO, SEXPR_STR, SEXPR_SYM};

    // Each of these binds the scrutinee (or a piece of it) straight through:
    // a float/bignum/ratio/string box *is* its value, so there is nothing to
    // unwrap and re-wrap.
    let one = |heap: &mut Heap, bound: Value| -> Result<Option<Vec<(SymId, Value)>>, EvalError> {
        match subs.first() {
            Some(p) => match_core_pattern(heap, *p, bound),
            None => Err(EvalError::Internal("eval: sexpr pattern has no sub-pattern".to_string())),
        }
    };

    match (variant, v) {
        (SEXPR_NIL, Value::Empty) => Ok(Some(Vec::new())),
        (SEXPR_INT, Value::Int(_)) => one(heap, v),
        (SEXPR_CHAR, Value::Char(_)) => one(heap, v),
        (SEXPR_BOOL, Value::Bool(_)) => one(heap, v),
        (SEXPR_SYM, Value::Symbol(_)) => one(heap, v),
        (SEXPR_STR, Value::Str(_)) => one(heap, v),
        (SEXPR_FLOAT, Value::Boxed(id)) if heap.is_float(id) => one(heap, v),
        (SEXPR_BIGNUM, Value::Boxed(id)) if heap.is_bignum(id) => one(heap, v),
        (SEXPR_RATIO, Value::Boxed(id)) if heap.is_ratio(id) => one(heap, v),
        (SEXPR_CONS, Value::Cons(_)) => {
            let car = heap.car(v).map_err(heap_err)?;
            let cdr = heap.cdr(v).map_err(heap_err)?;
            let (Some(pa), Some(pd)) = (subs.first(), subs.get(1)) else {
                return Err(EvalError::Internal("eval: cons pattern needs two sub-patterns".to_string()));
            };
            let Some(mut binds) = match_core_pattern(heap, *pa, car)? else {
                return Ok(None);
            };
            match match_core_pattern(heap, *pd, cdr)? {
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
fn construct_sexpr_core(heap: &mut Heap, variant: usize, argv: &[Value]) -> Result<Value, EvalError> {
    use super::{SEXPR_BIGNUM, SEXPR_BOOL, SEXPR_CHAR, SEXPR_CONS, SEXPR_FLOAT, SEXPR_INT, SEXPR_NIL, SEXPR_PATH, SEXPR_RATIO, SEXPR_STR, SEXPR_SYM};

    let arg = |i: usize| -> Result<Value, EvalError> {
        argv.get(i)
            .copied()
            .ok_or_else(|| EvalError::Internal(format!("eval: (construct sexpr {} ..) is missing field {}", variant, i)))
    };
    match variant {
        SEXPR_NIL => Ok(Value::Empty),
        // Each of these validates the argument and then passes the value
        // itself through: the box *is* the datum, so `(eq s (sexpr-str (Str
        // s)))` holds, as CL requires.
        SEXPR_INT => Ok(Value::Int(super::rt_i64(&arg(0)?)?)),
        SEXPR_CHAR => Ok(Value::Char(super::rt_char(&arg(0)?)?)),
        SEXPR_BOOL => Ok(Value::Bool(super::rt_bool(&arg(0)?)?)),
        SEXPR_SYM => arg(0),
        SEXPR_FLOAT => {
            let v = arg(0)?;
            super::rt_f64(heap, &v)?;
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
        // The arguments are rooted by the caller, which is what makes this
        // allocation safe.
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
fn tail_after(heap: &Heap, form: Value, n: usize) -> Result<Value, EvalError> {
    tail_after_value(heap, heap.cdr(form).map_err(heap_err)?, n)
}

/// [`tail_after`] over an already-taken tail — for a positional list (a
/// `labels` definition) that has no tag to skip.
fn tail_after_value(heap: &Heap, list: Value, n: usize) -> Result<Value, EvalError> {
    let mut cur = list;
    for _ in 0..n {
        cur = heap.cdr(cur).map_err(heap_err)?;
    }
    Ok(cur)
}

/// A field holding a vtable's slots: `((PATH SYM)...)`, the owning type and
/// method name for each of the trait's methods, in slot order.
fn method_list(heap: &Heap, form: Value, i: usize, what: &str) -> Result<Vec<(crate::Path, String)>, EvalError> {
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

/// The supertrait tables field: `((PATH ((PATH SYM)...))...)`.
fn super_list(heap: &Heap, form: Value, i: usize) -> Result<Vec<(crate::Path, Vec<(crate::Path, String)>)>, EvalError> {
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
fn param_names(heap: &Heap, params: Value) -> Result<Vec<SymId>, EvalError> {
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

/// A field holding an absolute path, as a [`crate::Path`].
///
/// A single-segment path reads back as a bare `Value::Symbol` rather than a
/// `Value::Path` — the reader only builds the latter when it sees `::` — so
/// both spellings are accepted and mean the same one-segment path.
fn path_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<crate::Path, EvalError> {
    match core::field(heap, form, i) {
        Some(Value::Path(id)) => Ok(crate::types::path_from_id(heap, id)),
        Some(Value::Symbol(id)) => Ok(crate::Path::root(heap.symbol_name(id))),
        other => Err(EvalError::Internal(format!("eval: ({} ..) field {} is not a path: {:?}", what, i, other))),
    }
}

/// A field holding an integer.
fn int_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<i64, EvalError> {
    match core::field(heap, form, i) {
        Some(Value::Int(n)) => Ok(n),
        other => Err(EvalError::Internal(format!("eval: ({} ..) field {} is not an integer: {:?}", what, i, other))),
    }
}

/// A field holding a boolean.
fn bool_field(heap: &Heap, form: Value, i: usize, what: &str) -> Result<bool, EvalError> {
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
fn env_lookup(heap: &Heap, env: Value, name: SymId) -> Option<Value> {
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
fn heap_err(e: typelisp_mem::Error) -> EvalError {
    EvalError::Internal(format!("eval: {}", e))
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
        assert_eq!(eval_ok(&mut h, "(int 42)"), Value::Int(42));
        assert_eq!(eval_ok(&mut h, "(int -7)"), Value::Int(-7));
        assert_eq!(eval_ok(&mut h, "(bool true)"), Value::Bool(true));
        assert_eq!(eval_ok(&mut h, "(bool false)"), Value::Bool(false));
        assert_eq!(eval_ok(&mut h, r"(char #\a)"), Value::Char('a'));
        assert_eq!(eval_ok(&mut h, "(unit)"), Value::Empty);

        let v = eval_ok(&mut h, "(float 1.5)");
        match v {
            Value::Boxed(id) if h.is_float(id) => assert_eq!(h.float_value(id), 1.5),
            other => panic!("expected a float box, got {:?}", other),
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
        let a = eval_ok(&mut h, "(float 1.5)");
        h.push_root(a);
        let b = eval_ok(&mut h, "(float 1.5)");
        assert_ne!(a, b, "two evaluations of a float literal shared one box");
    }

    // ---- variables and let ----------------------------------------------

    #[test]
    fn let_binds_and_var_reads() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(let ((x int (int 1))) (var x))"), Value::Int(1));
        assert_eq!(
            eval_ok(&mut h, "(let ((x int (int 1)) (y int (int 2))) (var y))"),
            Value::Int(2)
        );
    }

    #[test]
    fn a_nested_let_shadows_the_outer_binding() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, "(let ((x int (int 1))) (let ((x int (int 2))) (var x)))"),
            Value::Int(2)
        );
        // ...and the outer binding is visible again once the inner `let` ends.
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((x int (int 1))) (let () (let ((x int (int 2))) (var x)) (var x)))"
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
                "(let ((x int (int 1))) (let ((x int (int 2)) (y int (var x))) (var y)))"
            ),
            Value::Int(1)
        );
    }

    /// `progn` is not a tag of its own — it is a `let` with no bindings.
    #[test]
    fn a_let_with_no_bindings_is_progn() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(let () (int 1) (int 2))"), Value::Int(2));
        // An empty sequence is unit, matching `eval_seq`'s empty case.
        assert_eq!(eval_ok(&mut h, "(let ())"), Value::Empty);
    }

    /// A `let`'s frame survives a collection triggered from inside its own
    /// body.
    ///
    /// The frame and its cells are reachable only through the environment,
    /// which the trampoline roots — but there is a gap between `extend_let`
    /// building it and the trampoline taking it, and the body runs in that
    /// gap. Every other `let` test here has a body that never allocates, so
    /// none of them can see the difference; this one conses *before* reading
    /// the binding, which under `gc_stress` collects between the two.
    #[test]
    fn a_let_frame_survives_a_collection_inside_its_own_body() {
        let mut h = stress_heap();
        let v = eval_ok(
            &mut h,
            "(let ((x int (int 7)) (y int (int 8)))
               (call (sexpr-cons) () sexpr-cons
                 (call (sexpr-cons) () sexpr-cons (int 1) (int 2))
                 (call (sexpr-cons) () sexpr-cons (var x) (var y))))",
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
            "(let ((x int (int 7)))
               (call (sexpr-cons) () sexpr-cons (int 1) (int 2))
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
        let e = eval_src(&mut h, r#"(let () (panic (str "ran")) (int 2))"#).unwrap_err();
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
        assert_eq!(eval_ok(&mut h, "(if (bool true) (int 1) (int 2))"), Value::Int(1));
        assert_eq!(eval_ok(&mut h, "(if (bool false) (int 1) (int 2))"), Value::Int(2));
        // The branch not taken is not evaluated.
        assert_eq!(
            eval_ok(&mut h, r#"(if (bool true) (int 1) (panic (str "boom")))"#),
            Value::Int(1)
        );
    }

    /// The trampoline's reason for existing. The old evaluator had an
    /// iterative special case for `Expr::If` alone, because a long `cond`
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
        let mut form = core::tagged(&mut h, "int", &[Value::Int(7)]).unwrap();
        h.push_root(form);
        for _ in 0..DEPTH {
            let cond = core::tagged(&mut h, "bool", &[Value::Bool(false)]).unwrap();
            h.push_root(cond);
            let then = core::tagged(&mut h, "int", &[Value::Int(0)]).unwrap();
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

        let mut form = core::tagged(&mut h, "int", &[Value::Int(7)]).unwrap();
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
        let v = eval_ok(&mut h, "(call (sexpr-cons) () sexpr-cons (int 1) (int 2))");
        assert_eq!(h.car(v).unwrap(), Value::Int(1));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(2));

        // Arguments are themselves core forms, evaluated left to right.
        let v = eval_ok(
            &mut h,
            "(let ((a int (int 5))) (call (sexpr-cons) () sexpr-cons (var a) (call (sexpr-cons) () sexpr-cons (int 6) (unit))))",
        );
        assert_eq!(h.car(v).unwrap(), Value::Int(5));
        let rest = h.cdr(v).unwrap();
        assert_eq!(h.car(rest).unwrap(), Value::Int(6));
    }

    #[test]
    fn a_call_to_nothing_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(call (no-such-thing) () no-such-thing)").unwrap_err();
        assert!(matches!(e.kind(), EvalError::NoSuchFunction(n) if n == "no-such-thing"), "{:?}", e);
    }

    // ---- panic and diagnostics -------------------------------------------

    #[test]
    fn panic_carries_its_message() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, r#"(panic (str "boom"))"#).unwrap_err();
        assert!(matches!(e.kind(), EvalError::Panic(m) if m == "boom"), "{:?}", e);
    }

    /// A runtime error is placed at the node it came from, read out of the
    /// heap's `code_locs` table — the one the checker will fill in as it
    /// lowers, since the reader's own `cons_locs` is cleared per read batch.
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
            Value::Cons(cr) => h.set_code_loc(cr, loc.clone()),
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

    /// A tag with no evaluation yet names itself, rather than being taken for
    /// something else. That is what keeps the passing count an honest measure
    /// of progress while the tags are implemented one at a time.
    ///
    /// The first example moves as stages land — it named `loop` until `loop`
    /// was implemented, and this test failing for that reason is the measure
    /// working, not breaking. `unlowered` is the fixed half: it is
    /// scaffolding, so it never gets an evaluation at all.
    #[test]
    fn a_tag_with_no_evaluation_names_itself() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(assoc i64 + true () (int 1) (int 2))").unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("`assoc`")),
            "{:?}",
            e
        );
        let e = eval_src(&mut h, r#"(unlowered "CheckWhile")"#).unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("`unlowered`")),
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
            eval_ok(&mut h, "(field-get (construct point 0 true (int 1) (int 2)) 1)"),
            Value::Int(2)
        );
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((p sexpr (construct point 0 true (int 1) (int 2))))
                   (let ((q sexpr (var p)))
                     (field-set (var q) 0 (int 9))
                     (field-get (var p) 0)))",
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
                "(let ((p sexpr (construct point 0 true (int 1) (int 2))))
                   (field-set (var p) 0 (call (sexpr-cons) () sexpr-cons (int 8) (unit)))
                   (call (sexpr-car) () sexpr-car (field-get (var p) 0)))",
            ),
            Value::Int(8)
        );
    }

    /// An enum is a different box kind with a variant tag, and `MUTABLE` is
    /// what tells the two apart at the construct site.
    #[test]
    fn an_enum_carries_its_variant_and_fields() {
        let mut h = stress_heap();
        let v = eval_ok(&mut h, "(construct my-enum 2 false (int 41) (bool true))");
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
        assert_eq!(eval_ok(&mut h, "(construct sexpr 0 false)"), Value::Empty);
        assert_eq!(eval_ok(&mut h, "(construct sexpr 1 false (int 3))"), Value::Int(3));

        let v = eval_ok(&mut h, "(construct sexpr 7 false (int 1) (int 2))");
        assert_eq!(h.car(v).unwrap(), Value::Int(1));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(2));

        // A string field passes the very same `Value::Str` through, so the
        // datum is `eq` to the string it was built from.
        let v = eval_ok(
            &mut h,
            r#"(let ((s sexpr (str "hi"))) (call (sexpr-cons) () sexpr-cons (var s) (construct sexpr 6 false (var s))))"#,
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
                "(match (int 2) ((pat-lit (int 1)) (int 10)) ((pat-lit (int 2)) (int 20)) ((pat-wild) (int 99)))",
            ),
            Value::Int(20)
        );
        // The wildcard is reached only when the literals miss.
        assert_eq!(
            eval_ok(
                &mut h,
                "(match (int 7) ((pat-lit (int 1)) (int 10)) ((pat-wild) (int 99)))",
            ),
            Value::Int(99)
        );
        // An earlier arm wins even when a later one would also match.
        assert_eq!(
            eval_ok(&mut h, "(match (int 1) ((pat-wild) (int 10)) ((pat-lit (int 1)) (int 20)))"),
            Value::Int(10)
        );
    }

    #[test]
    fn a_bind_pattern_binds_the_whole_scrutinee() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(match (int 5) ((pat-bind x) (var x)))"), Value::Int(5));
    }

    /// A literal pattern is by-value equality, and only for the three types
    /// the language has pattern syntax for. A string would compare by
    /// identity and so never match, which is why it is refused rather than
    /// compared.
    #[test]
    fn a_literal_pattern_that_cannot_be_compared_by_value_is_refused() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, r"(match (char #\a) ((pat-lit (char #\a)) (int 1)) ((pat-wild) (int 2)))"),
            Value::Int(1)
        );
        let e = eval_src(&mut h, r#"(match (str "a") ((pat-lit (str "a")) (int 1)))"#).unwrap_err();
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
                "(match (construct point 0 true (int 1) (int 2))
                   ((pat-ctor point 0 false (pat-bind a) (pat-bind b)) (var b)))",
            ),
            Value::Int(2)
        );
        assert_eq!(
            eval_ok(
                &mut h,
                "(match (construct my-enum 1 false (int 42))
                   ((pat-ctor my-enum 0 false (pat-bind x)) (int 0))
                   ((pat-ctor my-enum 1 false (pat-bind x)) (var x)))",
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
                "(match (construct point 0 true (int 1))
                   ((pat-ctor other 0 false (pat-bind a)) (int 10))
                   ((pat-wild) (int 99)))",
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
                "(match (call (sexpr-cons) () sexpr-cons (int 1) (int 2))
                   ((pat-ctor sexpr 0 false) (int 100))
                   ((pat-ctor sexpr 7 false (pat-bind a) (pat-bind d)) (var d)))",
            ),
            Value::Int(2)
        );
        assert_eq!(
            eval_ok(&mut h, "(match (unit) ((pat-ctor sexpr 0 false) (int 100)) ((pat-wild) (int 0)))"),
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
            "(match (var p) ((pat-ctor sexpr 10 false (pat-bind segs))
               (call (sexpr-cons) () sexpr-cons (int 0) (var segs))))",
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
            "(match (var v)
               ((pat-ctor sexpr 7 false
                  (pat-ctor sexpr 10 false (pat-bind l))
                  (pat-ctor sexpr 10 false (pat-bind r)))
                 (call (sexpr-cons) () sexpr-cons (var l) (var r))))",
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
            "(match (construct point 0 true (int 7) (int 8))
               ((pat-ctor point 0 false (pat-bind a) (pat-bind b))
                 (call (sexpr-cons) () sexpr-cons (int 1) (int 2))
                 (call (sexpr-cons) () sexpr-cons (var a) (var b))))",
        );
        assert_eq!(h.car(v).unwrap(), Value::Int(7));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(8));
    }

    #[test]
    fn a_match_with_no_matching_arm_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(match (int 1) ((pat-lit (int 2)) (int 0)))").unwrap_err();
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
            eval_ok(&mut h, "(let ((x int (int 1))) (set x (int 9)))"),
            Value::Int(9)
        );
        assert_eq!(
            eval_ok(&mut h, "(let ((x int (int 1))) (set x (int 9)) (var x))"),
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
                "(let ((x int (int 1)))
                   (match (int 0) ((pat-bind ignored) (set x (int 9))))
                   (var x))",
            ),
            Value::Int(9)
        );
    }

    #[test]
    fn setting_an_unbound_name_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(set nope (int 1))").unwrap_err();
        assert!(matches!(e.kind(), EvalError::Unbound(n) if n == "nope"), "{:?}", e);
    }

    #[test]
    fn break_leaves_a_loop_with_unit_and_return_with_a_value() {
        let mut h = stress_heap();
        assert_eq!(eval_ok(&mut h, "(loop (break))"), Value::Empty);
        assert_eq!(eval_ok(&mut h, "(loop (return (int 7)))"), Value::Int(7));
        assert_eq!(eval_ok(&mut h, "(loop (return))"), Value::Empty);
        // Forms after the exit do not run.
        assert_eq!(
            eval_ok(&mut h, r#"(loop (break) (panic (str "ran past the break")))"#),
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
                "(let ((xs sexpr (call (sexpr-cons) () sexpr-cons (int 1)
                                   (call (sexpr-cons) () sexpr-cons (int 2)
                                     (call (sexpr-cons) () sexpr-cons (int 3) (unit)))))
                       (last sexpr (unit)))
                   (loop
                     (if (call (sexpr-null) () sexpr-null (var xs)) (break) (unit))
                     (set last (call (sexpr-car) () sexpr-car (var xs)))
                     (set xs (call (sexpr-cdr) () sexpr-cdr (var xs))))
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
                   (loop
                     (loop (break))
                     (set n (int 5))
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
                   (loop
                     (set n (loop (return (int 4))))
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
               (loop
                 (if (call (sexpr-null) () sexpr-null (var n)) (unit) (break))
                 (set xs (call (sexpr-cons) () sexpr-cons (int 1) (unit)))
                 (set n (call (sexpr-car) () sexpr-car (var xs)))))",
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
            eval_ok(&mut h, "(apply (lambda ((x int)) int (var x)) (int 5))"),
            Value::Int(5)
        );
        assert_eq!(
            eval_ok(&mut h, "(apply (lambda () int (int 7)) )"),
            Value::Int(7)
        );
        // Parameters shadow an outer binding of the same name.
        assert_eq!(
            eval_ok(
                &mut h,
                "(let ((x int (int 1))) (apply (lambda ((x int)) int (var x)) (int 2)))",
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
                "(let ((n int (int 10)))
                   (let ((f sexpr (lambda () int (var n))))
                     (let ((n int (int 99)))
                       (apply (var f)))))",
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
                "(let ((n int (int 1)))
                   (let ((f sexpr (lambda () int (var n))))
                     (set n (int 42))
                     (apply (var f))))",
            ),
            Value::Int(42)
        );
    }

    #[test]
    fn applying_the_wrong_number_of_arguments_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(apply (lambda ((x int)) int (var x)))").unwrap_err();
        assert!(matches!(e.kind(), EvalError::Internal(m) if m.contains("arity mismatch")), "{:?}", e);
    }

    #[test]
    fn applying_something_that_is_not_a_function_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(apply (int 1))").unwrap_err();
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
                "(labels ((f ((x int)) int (apply (var g) (var x)))
                          (g ((y int)) int (var y)))
                   (apply (var f) (int 3)))",
            ),
            Value::Int(3)
        );
        // And a sibling can call itself.
        assert_eq!(
            eval_ok(
                &mut h,
                "(labels ((f ((xs sexpr)) sexpr
                            (if (call (sexpr-null) () sexpr-null (var xs))
                                (int 0)
                                (apply (var f) (call (sexpr-cdr) () sexpr-cdr (var xs))))))
                   (apply (var f) (call (sexpr-cons) () sexpr-cons (int 1)
                                    (call (sexpr-cons) () sexpr-cons (int 2) (unit)))))",
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
                        (if (call (sexpr-null) () sexpr-null (var l))
                            (int 0)
                            (apply (var walk) (call (sexpr-cdr) () sexpr-cdr (var l))))))
               (apply (var walk) (var xs)))",
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
    /// The trampoline roots the form it is evaluating, so for as long as the
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
        let form = read1(&mut h, "(lambda ((x int)) int (var x))");

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
        let call = read1(&mut h, "(apply (var g) (int 5))");
        h.push_root(call);
        h.gc();

        let v = interp.eval_core(&mut h, call, env).expect("calling the closure failed");
        assert_eq!(v, Value::Int(5));
    }

    /// A closure value is opaque to `apply`'s two other callee kinds, and
    /// each says which one it is rather than being taken for a closure.
    #[test]
    fn a_compiled_callee_is_named_rather_than_guessed_at() {
        let mut h = stress_heap();
        let form = read1(&mut h, "(apply (var f))");
        h.push_root(form);
        let fake = h.alloc_compiled_closure(0, Vec::new(), 0);
        h.push_root(fake);
        let name = match h.intern_symbol("f") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let env = extend_env(&mut h, &[(name, fake)], Value::Empty).unwrap();
        h.push_root(env);

        let interp = Interp::new();
        let e = interp.eval_core(&mut h, form, env).unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("compiled closure")),
            "{:?}",
            e
        );
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
            r#"(dyn-new "point" shape ((point area)) () (construct point 0 true (int 3)))"#,
        );
        h.push_root(v);
        match v {
            Value::Boxed(id) => assert!(h.is_dyn(id), "expected a trait object"),
            other => panic!("expected a box, got {:?}", other),
        }
        let inner = eval_ok(
            &mut h,
            r#"(field-get (dyn-value (dyn-new "point" shape ((point area)) () (construct point 0 true (int 3)))) 0)"#,
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
                 (dyn-new "point" shape ((point area)) ((named ((point name)))) (construct point 0 true (int 3))))"#,
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
                                (construct point 0 true (int 3)))))
                 (call (sexpr-cons) () sexpr-cons (var b) (dyn-upcast shape (var b))))"#,
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
                   (construct point 0 true (int 3))))"#,
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
        let e = eval_src(&mut h, "(dyn-value (int 1))").unwrap_err();
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
