//! The evaluator as an explicit state machine over a continuation stack.
//!
//! This is [`core_eval`](super::core_eval) with the Rust recursion taken out.
//! The two produce the same values for the same programs; what differs is
//! *where the rest of the computation lives*. In `core_eval` a non-tail
//! subexpression is a recursive `self.eval_core(..)` call, so the depth of a
//! Lisp recursion is the depth of the Rust stack. Here it is a `Frame` on a
//! `Vec`, so the Rust stack stays flat and the continuation becomes data —
//! which is what a task (goroutine) needs to be suspended and resumed.
//!
//! The design, and the mapping from every `Op` to the frames it needs, is in
//! `docs/dev/cps-evaluator-design.md`.
//!
//! # Roots
//!
//! Two slots at the bottom hold whatever the *current state* carries, and are
//! overwritten in place (`set_root`) rather than pushed — the same trick
//! `eval_core`'s trampoline uses to keep a tail loop from growing the root
//! stack. Above them, each frame roots its own values when pushed and drops
//! them with `truncate_roots` when unwound:
//!
//! ```text
//! roots: [ ..., state-env, state-form, frame0's values.., frame1's values.., ... ]
//!               ^ sbase                ^ frame0's base
//! ```
//!
//! A frame's subexpression never needs a root of its own: it is reachable
//! from the form the frame already roots. Only a *computed* value does, which
//! is why `State::Apply` carries one through the state slots.
//!
//! The one window that must not allocate is between `truncate_roots` (which
//! drops a frame's values) and the `set_state` that re-roots what the next
//! state carries. Nothing in between touches the heap.

use typelisp_mem::{Heap, SymRef, Value};

use crate::check::core;
use crate::eval::value::EvalError;

use super::core_eval::{env_lookup, extend_env, int_field, path_field, Op, Step};
use super::Interp;

/// What one step of the machine produced.
enum State {
    /// Evaluate this form in this environment.
    Eval(Value, Value),
    /// A value is ready. Hand it to the top frame, or return it if there is none.
    Apply(Value),
}

/// A suspended computation: what to do once the subexpression being evaluated
/// produces a value.
///
/// Frames hold the *form* rather than its pieces, so one root covers every
/// part of it — `core::field` reads the piece back on resume.
enum Frame {
    /// `(if COND THEN ELSE)` — waiting on the condition.
    If { form: Value, env: Value },
    /// `(panic MSG)` — waiting on the message.
    Panic,
    /// `(field-get OBJ IDX REPR)` — waiting on the object.
    FieldGet { idx: usize },
    /// `(dyn-value INNER)` — waiting on the trait object.
    DynValue,
    /// `(set SYM VALUE)` — waiting on the value. The binding is looked up
    /// *after* the value is in hand, exactly as the recursive evaluator does.
    Set { sym: SymRef, env: Value },
    /// A body's remaining forms. `rest` is a non-empty cons list of forms not
    /// yet evaluated; the value being resumed on is discarded, since only a
    /// sequence's last form contributes its value.
    Seq { rest: Value, env: Value },
    /// `(call WRITTEN HOME PATH (R...) ARG...)` — waiting on one argument.
    ///
    /// `done` holds the arguments already evaluated, in order; the form
    /// carries the rest. An argument evaluated three ago is as collectible as
    /// the one that just arrived, so all of them stay rooted until the call
    /// itself is made.
    CallArgs { form: Value, done: Vec<Value>, env: Value },
    /// `(let ((SYM R INIT)...) BODY...)` — waiting on one initialiser.
    ///
    /// `binds` is the whole binding list (the names are read back from it once
    /// every value is in), `rest` the bindings still to evaluate, and `done`
    /// the values collected so far, in binding order.
    LetInit {
        binds: Value,
        rest: Value,
        done: Vec<Value>,
        body: Value,
        env: Value,
    },
}

/// The continuation stack: frames, each with the root count from just before
/// it was pushed, so unwinding one drops exactly the roots it added.
struct CpsStack {
    frames: Vec<(Frame, usize)>,
}

impl CpsStack {
    fn new() -> CpsStack {
        CpsStack { frames: Vec::new() }
    }

    /// Pushes `frame`, rooting the values it holds.
    fn push(&mut self, heap: &mut Heap, frame: Frame) {
        let base = heap.root_count();
        match &frame {
            Frame::If { form, env } => {
                heap.push_root(*form);
                heap.push_root(*env);
            }
            Frame::Set { env, .. } => heap.push_root(*env),
            Frame::CallArgs { form, done, env } => {
                heap.push_root(*form);
                heap.push_root(*env);
                for v in done {
                    heap.push_root(*v);
                }
            }
            Frame::Seq { rest, env } => {
                heap.push_root(*rest);
                heap.push_root(*env);
            }
            Frame::LetInit { binds, rest, done, body, env } => {
                heap.push_root(*binds);
                heap.push_root(*rest);
                heap.push_root(*body);
                heap.push_root(*env);
                // Each initialiser's value stays rooted for the rest of them:
                // one bound three initialisers ago is just as collectible as
                // the one that just arrived.
                for v in done {
                    heap.push_root(*v);
                }
            }
            // Nothing to root: these carry only a plain integer, or nothing
            // at all. The value they are waiting on is rooted by the state
            // slots, and the object they act on is reachable from it.
            Frame::Panic | Frame::FieldGet { .. } | Frame::DynValue => {}
        }
        self.frames.push((frame, base));
    }

    /// Removes the top frame. **The caller must `truncate_roots` to the
    /// returned base** — after it has finished reading the frame, and with no
    /// allocation between that truncation and re-rooting the next state.
    fn pop(&mut self) -> Option<(Frame, usize)> {
        self.frames.pop()
    }
}

/// Points the two state slots at whatever `state` carries.
///
/// Must not allocate: it runs in the window right after a `truncate_roots`.
fn set_state(heap: &mut Heap, sbase: usize, state: &State) {
    match state {
        State::Eval(form, env) => {
            heap.set_root(sbase, *env);
            heap.set_root(sbase + 1, *form);
        }
        State::Apply(v) => {
            heap.set_root(sbase, *v);
            heap.set_root(sbase + 1, Value::Empty);
        }
    }
}

/// The state that evaluates `forms` (a cons list) in order, with the value
/// of the last one as the result.
///
/// An empty sequence is unit, exactly as `(let () )` evaluates. A single form
/// is a *tail jump*: no frame, so a body's last form costs no stack — the
/// property `Step::Tail` gives the recursive evaluator.
fn sequence_state(heap: &Heap, forms: Value, env: Value) -> Result<(State, Option<Frame>), EvalError> {
    if matches!(forms, Value::Empty) {
        return Ok((State::Apply(Value::Empty), None));
    }
    let first = heap
        .car(forms)
        .map_err(|e| EvalError::Internal(format!("eval: body form: {}", e)))?;
    let rest = heap
        .cdr(forms)
        .map_err(|e| EvalError::Internal(format!("eval: body rest: {}", e)))?;
    let frame = if matches!(rest, Value::Empty) {
        None
    } else {
        Some(Frame::Seq { rest, env })
    };
    Ok((State::Eval(first, env), frame))
}

/// The argument forms of a `(call WRITTEN HOME PATH (R...) ARG...)` node.
///
/// The first four fields are the written name, the home module, the resolved
/// path and the argument representations — the last of these is the bridge's,
/// and evaluation is uniform over `Value`, so it is skipped here.
fn call_arg_forms(heap: &Heap, form: Value) -> Result<Vec<Value>, EvalError> {
    let fields = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: (call ..): {}", e)))?;
    if fields.len() < 4 {
        return Err(EvalError::Internal(format!("eval: malformed call: {}", core::print(heap, form))));
    }
    Ok(fields[4..].to_vec())
}

/// A `let` binding's name and initialiser.
///
/// A binding is `(SYM R E)` — a plain three-element list, not a tagged node,
/// so its elements are read positionally (`core::field` would skip the first
/// as a tag).
fn bind_parts(heap: &Heap, b: Value) -> Result<(SymRef, Value), EvalError> {
    let bad = |what: &str| EvalError::Internal(format!("eval: (let ..) binding {}", what));
    let Ok(Value::Symbol(sym)) = heap.car(b) else {
        return Err(bad("name is not a symbol"));
    };
    let after_name = heap.cdr(b).map_err(|_| bad("has no representation"))?;
    let after_repr = heap.cdr(after_name).map_err(|_| bad("has no initialiser"))?;
    let init = heap.car(after_repr).map_err(|_| bad("has no initialiser"))?;
    Ok((sym, init))
}

impl Interp {
    /// Evaluate `form` in `env` with no Rust recursion.
    ///
    /// The eventual replacement for [`Interp::eval_core`](super::Interp::eval_core).
    /// Until every `Op` is here, the ones that are not reach `unimplemented!`
    /// rather than falling back to the recursive evaluator: a fallback would
    /// run correctly and hide which tags still have to move.
    pub(crate) fn eval_cps(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let sbase = heap.root_count();
        heap.push_root(env);
        heap.push_root(form);

        let mut stack = CpsStack::new();
        let mut state = State::Eval(form, env);

        let out = loop {
            match state {
                State::Eval(form, env) => match self.step_cps(heap, &mut stack, form, env) {
                    Ok(next) => {
                        set_state(heap, sbase, &next);
                        state = next;
                    }
                    Err(e) => break Err(e),
                },
                State::Apply(v) => match stack.pop() {
                    None => break Ok(v),
                    Some((frame, fbase)) => match self.resume(heap, frame, v) {
                        Ok((next, pushed)) => {
                            // Nothing from here to `set_state` may allocate:
                            // the frame's roots are gone and whatever the next
                            // state carries lives only in Rust locals until it
                            // is rooted again. `truncate_roots`, `push` and
                            // `set_root` all leave the heap alone.
                            heap.truncate_roots(fbase);
                            if let Some(f) = pushed {
                                stack.push(heap, f);
                            }
                            set_state(heap, sbase, &next);
                            state = next;
                        }
                        Err(e) => break Err(e),
                    },
                },
            }
        };

        heap.truncate_roots(sbase);
        out
    }

    /// One step of evaluation: reduce `form` to a value, or to a
    /// subexpression with a frame remembering what to do with its value.
    fn step_cps(&self, heap: &mut Heap, stack: &mut CpsStack, form: Value, env: Value) -> Result<State, EvalError> {
        let tag = match heap.car(form) {
            Ok(Value::Symbol(id)) => id,
            _ => return Err(EvalError::Internal(format!("eval: not a core form: {}", core::print(heap, form)))),
        };
        let op = Op::from_sym(tag)
            .ok_or_else(|| EvalError::Internal(format!("eval: unknown core tag `{}`", heap.symbol_name(tag))))?;

        match op {
            // Leaves: no subexpression, so no frame. These reach a value
            // without recursion even in the old evaluator, so they run
            // through its implementation unchanged.
            Op::Int
            | Op::Float
            | Op::Bignum
            | Op::Ratio
            | Op::Char
            | Op::Bool
            | Op::Str
            | Op::Sym
            | Op::Unit
            | Op::Var
            | Op::Quote
            | Op::Lambda
            | Op::FnRef
            | Op::MethodRef
            | Op::Global
            | Op::CompileFn
            | Op::Trace
            | Op::Untrace
            | Op::DisassembleFn => match self.step_core(heap, form, env)? {
                Step::Done(v) => Ok(State::Apply(v)),
                Step::Tail(..) => Err(EvalError::Internal(format!(
                    "eval: leaf op {:?} produced a tail step",
                    op
                ))),
            },

            Op::If => {
                let cond = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (if ..) has no condition".to_string()))?;
                stack.push(heap, Frame::If { form, env });
                Ok(State::Eval(cond, env))
            }

            Op::Panic => {
                let msg = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (panic ..) has no message".to_string()))?;
                stack.push(heap, Frame::Panic);
                Ok(State::Eval(msg, env))
            }

            Op::FieldGet => {
                let obj = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (field-get ..) has no object".to_string()))?;
                // Field 2 is the field's representation, read by the bridge
                // and by nothing here.
                let idx = int_field(heap, form, 1, "field-get")? as usize;
                stack.push(heap, Frame::FieldGet { idx });
                Ok(State::Eval(obj, env))
            }

            Op::DynValue => {
                let inner = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (dyn-value ..) has no operand".to_string()))?;
                stack.push(heap, Frame::DynValue);
                Ok(State::Eval(inner, env))
            }

            Op::Set => {
                let sym = self.sym_field(heap, form, 0, "set")?;
                let val = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (set ..) has no value".to_string()))?;
                stack.push(heap, Frame::Set { sym, env });
                Ok(State::Eval(val, env))
            }

            Op::Let => {
                let bad = |what: &str| EvalError::Internal(format!("eval: (let ..) {}", what));
                let after_tag = heap.cdr(form).map_err(|_| bad("is not a list"))?;
                let binds = heap.car(after_tag).map_err(|_| bad("has no binding list"))?;
                let body = heap.cdr(after_tag).map_err(|_| bad("has no body"))?;
                if matches!(binds, Value::Empty) {
                    // `(let () E...)` is `progn`.
                    let (next, pushed) = sequence_state(heap, body, env)?;
                    if let Some(f) = pushed {
                        stack.push(heap, f);
                    }
                    return Ok(next);
                }
                let first = heap.car(binds).map_err(|_| bad("binding list is not a list"))?;
                let (_, init) = bind_parts(heap, first)?;
                stack.push(
                    heap,
                    Frame::LetInit { binds, rest: binds, done: Vec::new(), body, env },
                );
                Ok(State::Eval(init, env))
            }

            Op::Call => {
                let args = call_arg_forms(heap, form)?;
                match args.first() {
                    None => {
                        let (next, pushed) = self.finish_call(heap, form, Vec::new())?;
                        if let Some(f) = pushed {
                            stack.push(heap, f);
                        }
                        Ok(next)
                    }
                    Some(&first) => {
                        stack.push(heap, Frame::CallArgs { form, done: Vec::new(), env });
                        Ok(State::Eval(first, env))
                    }
                }
            }

            other => unimplemented!("eval_cps: {:?} has not moved to the continuation stack yet", other),
        }
    }

    /// Every argument of a `call` is in hand: resolve the callee and enter it.
    ///
    /// A user function's body becomes a sequence in a fresh environment, so
    /// **its last form is a tail jump** — a property the recursive evaluator
    /// does not have here (`Interp::apply` evaluates the last form with a
    /// native frame still waiting on it).
    fn finish_call(&self, heap: &mut Heap, form: Value, argv: Vec<Value>) -> Result<(State, Option<Frame>), EvalError> {
        let written = self.name_list(heap, form, 0, "call")?;
        let home = self.name_list(heap, form, 1, "call")?;
        let path = path_field(heap, form, 2, "call")?;

        if let Some(f) = self.resolve_fn_named(&home, &written, &path) {
            // Tracing and stepping report a call around the frame it opens,
            // which a tail jump does not leave behind. They move in the stage
            // that gives calls their own frame — until then, saying so beats
            // quietly losing the trace.
            if self.trace_armed.get() || self.stepping.get() {
                unimplemented!("eval_cps: trace/step over a call has not moved to the continuation stack yet");
            }
            // A compiled callee runs on the Rust stack: the boundary is one
            // frame from the machine's point of view, and nothing can suspend
            // inside it.
            let compiled = f.compiled.borrow().clone();
            if let Some(compiled) = compiled {
                let sig = f.sig.as_ref().expect("a compiled function always has a type signature");
                let v = self.call_compiled(heap, compiled.as_ref(), &argv, &sig.0, &sig.1)?;
                return Ok((State::Apply(v), None));
            }
            if f.params.len() != argv.len() {
                return Err(EvalError::Internal(format!(
                    "apply: the body takes {} argument(s), given {}",
                    f.params.len(),
                    argv.len()
                )));
            }
            let binds: Vec<(SymRef, Value)> = f
                .params
                .iter()
                .zip(argv)
                .map(|(name, v)| match heap.intern_symbol(name) {
                    Value::Symbol(id) => (id, v),
                    _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
                })
                .collect();
            // A top-level function closes over nothing, so its frame extends
            // the empty environment rather than the caller's — that is what
            // makes the scope lexical.
            let env = extend_env(heap, &binds, Value::Empty)?;
            // `FnDef::body` is a `Vec`, and a sequence frame walks a cons
            // list, so the body is consed up here. `core::list` roots every
            // item and every partial tail, so a collection mid-build is safe.
            let body = core::list(heap, &f.body)
                .map_err(|e| EvalError::Internal(format!("eval: call body: {}", e)))?;
            // The last allocation on this path: `sequence_state` only walks
            // conses, so `body` and `env` stay in Rust locals — live because
            // nothing collects — until the caller roots them again.
            return sequence_state(heap, body, env);
        }

        // Otherwise a built-in operator, which lives at the root and so is
        // always spelled as a bare name.
        if written.len() == 1 {
            if let Some(result) = self.eval_builtin(heap, &written[0], &argv) {
                return Ok((State::Apply(result?), None));
            }
        }
        Err(EvalError::NoSuchFunction(path.to_string()))
    }

    /// Hands `v` to `frame` — the frame's "what to do with the value" half.
    ///
    /// Returns the next state and, when the resumption itself suspends on a
    /// subexpression, the frame to push for it. It does **not** push that
    /// frame: the caller has to drop this frame's roots first, and the root
    /// stack is LIFO, so anything pushed here would be truncated away with
    /// them.
    ///
    /// A value this method allocates (an extended environment, say) is rooted
    /// by nothing until the caller re-roots it, so **nothing may allocate
    /// after the last value the returned state carries is built**.
    fn resume(&self, heap: &mut Heap, frame: Frame, v: Value) -> Result<(State, Option<Frame>), EvalError> {
        match frame {
            Frame::If { form, env } => {
                let taken = match v {
                    Value::Bool(true) => 1,
                    Value::Bool(false) => 2,
                    other => return Err(EvalError::Internal(format!("eval: (if ..) condition is {:?}", other))),
                };
                let branch = core::field(heap, form, taken)
                    .ok_or_else(|| EvalError::Internal("eval: (if ..) is missing a branch".to_string()))?;
                // A tail jump: no frame, the branch's value is the `if`'s.
                Ok((State::Eval(branch, env), None))
            }

            // A non-string message is not reachable from a checked program
            // and carries nothing to report, so it panics with no text — the
            // recursive evaluator's behaviour, kept deliberately.
            Frame::Panic => Err(EvalError::Panic(match v {
                Value::Str(id) => heap.string(id).to_string(),
                _ => String::new(),
            })),

            // No decode: with one value world left, the field *is* the value.
            Frame::FieldGet { idx } => {
                let id = super::expect_struct_box(&v)?;
                Ok((State::Apply(heap.struct_field(id, idx)), None))
            }

            Frame::DynValue => match v {
                Value::Boxed(id) if heap.is_dyn(id) => Ok((State::Apply(heap.dyn_value(id)), None)),
                other => Err(EvalError::Internal(format!(
                    "eval: (dyn-value ..): not a trait object: {:?}",
                    other
                ))),
            },

            // Written through the cell rather than rebuilding the frame,
            // which is what makes the assignment visible through every other
            // reference to the same binding — a closure's capture, or
            // compiled code handed the same cell.
            Frame::Set { sym, env } => {
                let cell = env_lookup(heap, env, sym)
                    .ok_or_else(|| EvalError::Unbound(heap.symbol_name(sym).to_string()))?;
                match cell {
                    Value::Boxed(id) if heap.is_cell(id) => heap.cell_set(id, v),
                    other => {
                        return Err(EvalError::Internal(format!(
                            "eval: binding does not hold a cell: {:?}",
                            other
                        )))
                    }
                }
                // `setf` returns the value assigned (docs/syntax.md §7).
                Ok((State::Apply(v), None))
            }

            Frame::CallArgs { form, mut done, env } => {
                done.push(v);
                let args = call_arg_forms(heap, form)?;
                match args.get(done.len()) {
                    Some(&next) => Ok((
                        State::Eval(next, env),
                        Some(Frame::CallArgs { form, done, env }),
                    )),
                    None => self.finish_call(heap, form, done),
                }
            }

            // Only the last form of a sequence contributes a value, so this
            // one is discarded.
            Frame::Seq { rest, env } => sequence_state(heap, rest, env),

            Frame::LetInit { binds, rest, mut done, body, env } => {
                done.push(v);
                let bad = |what: &str| EvalError::Internal(format!("eval: (let ..) {}", what));
                let next_rest = heap.cdr(rest).map_err(|_| bad("binding list is improper"))?;
                if !matches!(next_rest, Value::Empty) {
                    let b = heap.car(next_rest).map_err(|_| bad("binding list is improper"))?;
                    let (_, init) = bind_parts(heap, b)?;
                    return Ok((
                        State::Eval(init, env),
                        Some(Frame::LetInit { binds, rest: next_rest, done, body, env }),
                    ));
                }
                // Every initialiser is in. Pair the values back up with the
                // names, in binding order, and extend the environment.
                let mut pairs = Vec::with_capacity(done.len());
                let mut b = binds;
                for value in done {
                    let bind = heap.car(b).map_err(|_| bad("binding list is improper"))?;
                    let (sym, _) = bind_parts(heap, bind)?;
                    pairs.push((sym, value));
                    b = heap.cdr(b).map_err(|_| bad("binding list is improper"))?;
                }
                // The last allocation on this path: `sequence_state` only
                // walks conses, so the environment stays in a Rust local
                // until the caller roots it.
                let new_env = extend_env(heap, &pairs, env)?;
                sequence_state(heap, body, new_env)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Reader;

    /// A heap in stress mode: every `cons` collects first, so a value a frame
    /// holds without rooting dies at the very next allocation rather than one
    /// run in a hundred. Every test here uses one — a continuation frame that
    /// forgets to root what it carries is the failure this rewrite is most
    /// likely to produce.
    fn stress_heap() -> Heap {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_gc_stress(true);
        h
    }

    fn read1(heap: &mut Heap, src: &str) -> Value {
        let r = Reader::new();
        let mut vs = r.read_all(heap, src).expect("read failed");
        assert_eq!(vs.len(), 1, "expected exactly one form in {:?}", src);
        vs.pop().unwrap()
    }

    /// Evaluate a core form through the continuation machine.
    ///
    /// Asserts the root stack ends exactly where it started: a leaked root is
    /// not a crash but a value that can never be collected, and this is the
    /// only place it shows up.
    fn eval_src(heap: &mut Heap, src: &str) -> Result<Value, EvalError> {
        let form = read1(heap, src);
        heap.push_root(form);
        let before = heap.root_count();
        let interp = Interp::new();
        let out = interp.eval_cps(heap, form, Value::Empty);
        assert_eq!(heap.root_count(), before, "eval_cps leaked or over-popped roots on {:?}", src);
        out
    }

    fn eval_ok(heap: &mut Heap, src: &str) -> Value {
        eval_src(heap, src).unwrap_or_else(|e| panic!("{:?} failed to evaluate: {}", src, e))
    }

    /// The same form through both evaluators, asserting they agree. The whole
    /// premise of the rewrite is that it changes no program's meaning, so most
    /// tests here should be able to say exactly this.
    fn agrees(heap: &mut Heap, src: &str) -> Value {
        let form = read1(heap, src);
        heap.push_root(form);
        let interp = Interp::new();
        let old = interp.eval_core(heap, form, Value::Empty);
        let new = interp.eval_cps(heap, form, Value::Empty);
        match (old, new) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b, "the two evaluators disagree on {:?}", src);
                b
            }
            (Err(a), Err(b)) => panic!("both failed on {:?}: {} / {}", src, a, b),
            (a, b) => panic!("only one evaluator failed on {:?}: {:?} / {:?}", src, a, b),
        }
    }

    // ---- leaves ----------------------------------------------------------

    #[test]
    fn leaves_reach_a_value_without_a_frame() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(int-any-width 42)"), Value::Int(42));
        assert_eq!(agrees(&mut h, "(bool true)"), Value::Bool(true));
        assert_eq!(agrees(&mut h, r"(char #\a)"), Value::Char('a'));
        assert_eq!(agrees(&mut h, "(unit)"), Value::Empty);
        assert!(matches!(agrees(&mut h, "(sym foo)"), Value::Symbol(_)));
    }

    // ---- if: the first form that needs a frame ---------------------------

    #[test]
    fn if_takes_the_branch_the_condition_names() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(if (bool true) (int-any-width 1) (int-any-width 2))"), Value::Int(1));
        assert_eq!(agrees(&mut h, "(if (bool false) (int-any-width 1) (int-any-width 2))"), Value::Int(2));
    }

    /// The branch not taken is never evaluated — `panic` in the dead branch
    /// would be an error if it ran.
    #[test]
    fn the_branch_not_taken_does_not_run() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, r#"(if (bool true) (int-any-width 1) (panic (str "boom")))"#),
            Value::Int(1)
        );
    }

    /// A condition that is itself an `if` stacks a second frame while the
    /// first is still suspended. This is the property the whole design exists
    /// for, at depth 2.
    #[test]
    fn a_condition_that_is_itself_an_if_stacks_two_frames() {
        let mut h = stress_heap();
        let src = "(if (if (bool false) (bool false) (bool true)) (int-any-width 7) (int-any-width 9))";
        assert_eq!(agrees(&mut h, src), Value::Int(7));
    }

    /// Deeply nested conditions: 200 frames, each holding a form and an
    /// environment. Under `gc_stress` every step collects, so a frame that
    /// failed to root what it carries loses it here.
    #[test]
    fn nested_conditions_keep_their_frames_alive_under_collection() {
        let mut h = stress_heap();
        let mut src = String::from("(bool true)");
        for _ in 0..200 {
            src = format!("(if {} (bool true) (bool false))", src);
        }
        let src = format!("(if {} (int-any-width 1) (int-any-width 2))", src);
        assert_eq!(eval_ok(&mut h, &src), Value::Int(1));
    }

    /// A condition that is not a boolean is an internal error, not a silent
    /// branch — the checker should have made it impossible.
    #[test]
    fn a_non_boolean_condition_is_an_internal_error() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(if (int-any-width 1) (int-any-width 1) (int-any-width 2))")
            .expect_err("a non-boolean condition should not evaluate");
        assert!(matches!(e, EvalError::Internal(_)), "expected an internal error, got {:?}", e);
    }

    // ---- let, sequences, assignment --------------------------------------

    #[test]
    fn let_binds_and_the_body_sees_the_binding() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(let ((x r (int-any-width 5))) (var x))"), Value::Int(5));
    }

    /// Bindings are parallel: every initialiser runs in the *outer*
    /// environment, so the second cannot see the first.
    #[test]
    fn initialisers_run_in_the_outer_environment() {
        let mut h = stress_heap();
        let e = eval_src(
            &mut h,
            "(let ((x r (int-any-width 1)) (y r (var x))) (var y))",
        )
        .expect_err("the second initialiser must not see the first binding");
        assert!(matches!(e, EvalError::Unbound(_)), "expected Unbound, got {:?}", e);
    }

    /// `(let () E...)` is `progn`: every form runs, the last one's value wins.
    #[test]
    fn an_empty_binding_list_is_progn() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(let () (int-any-width 1) (int-any-width 2))"), Value::Int(2));
        assert_eq!(agrees(&mut h, "(let () (int-any-width 9))"), Value::Int(9));
        assert_eq!(agrees(&mut h, "(let ())"), Value::Empty);
    }

    /// `set` writes through the binding's cell and returns the value assigned.
    #[test]
    fn set_writes_through_the_cell_and_returns_the_value() {
        let mut h = stress_heap();
        let src = "(let ((x r (int-any-width 1))) (set x (int-any-width 42)) (var x))";
        assert_eq!(agrees(&mut h, src), Value::Int(42));
        let src = "(let ((x r (int-any-width 1))) (set x (int-any-width 42)))";
        assert_eq!(agrees(&mut h, src), Value::Int(42));
    }

    /// Many bindings, each initialiser allocating, with a collection between
    /// every one: a value bound early has to stay rooted while the later
    /// initialisers run.
    #[test]
    fn earlier_initialisers_survive_later_ones() {
        let mut h = stress_heap();
        let binds: String = (0..40)
            .map(|i| format!("(x{} r (str \"s{}\"))", i, i))
            .collect::<Vec<_>>()
            .join(" ");
        let src = format!("(let ({}) (var x0))", binds);
        match eval_ok(&mut h, &src) {
            Value::Str(id) => assert_eq!(h.string(id), "s0"),
            other => panic!("expected the first binding's string, got {:?}", other),
        }
    }

    /// Nested `let`s: 150 frames deep, each holding an environment, under
    /// collection at every allocation.
    #[test]
    fn deeply_nested_lets_keep_their_environments() {
        let mut h = stress_heap();
        let mut src = String::from("(var x0)");
        for i in (0..150).rev() {
            src = format!("(let ((x{} r (int-any-width {}))) {})", i, i, src);
        }
        assert_eq!(eval_ok(&mut h, &src), Value::Int(0));
    }

    /// A sequence's last form is a tail jump — no frame — so a body of any
    /// length costs the same stack as a body of one.
    #[test]
    fn a_long_body_does_not_grow_the_continuation_stack() {
        let mut h = stress_heap();
        let forms: String = (0..500)
            .map(|i| format!("(int-any-width {})", i))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(eval_ok(&mut h, &format!("(let () {})", forms)), Value::Int(499));
    }

    // ---- calls -----------------------------------------------------------

    /// A built-in operator: arguments left to right, then the operator.
    #[test]
    fn a_builtin_call_evaluates_its_arguments_then_applies() {
        let mut h = stress_heap();
        let v = agrees(
            &mut h,
            "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2))",
        );
        assert_eq!(h.car(v).unwrap(), Value::Int(1));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(2));
    }

    /// No arguments means no frame at all: the call is made straight from
    /// `step_cps`, which is the path a nullary builtin takes.
    #[test]
    fn a_call_with_no_arguments_needs_no_frame() {
        let mut h = stress_heap();
        let v = eval_ok(
            &mut h,
            "(call (make-random-state-fresh) () make-random-state-fresh ())",
        );
        assert!(matches!(v, Value::Boxed(_)), "expected a random-state box, got {:?}", v);
    }

    /// Nested calls: the inner one runs while the outer's argument frame is
    /// suspended, and the outer's earlier arguments have to survive it.
    #[test]
    fn an_argument_that_is_itself_a_call_suspends_the_outer_one() {
        let mut h = stress_heap();
        let src = "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) \
                     (str \"first\") \
                     (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2)))";
        let v = agrees(&mut h, src);
        match h.car(v).unwrap() {
            Value::Str(id) => assert_eq!(h.string(id), "first"),
            other => panic!("the first argument did not survive the nested call: {:?}", other),
        }
    }

    /// Arguments are evaluated left to right, and each one stays rooted while
    /// the later ones allocate. 30 string arguments under `gc_stress`: if the
    /// frame dropped an early one, the list would come back wrong.
    #[test]
    fn earlier_arguments_survive_later_ones() {
        let mut h = stress_heap();
        let mut src = String::from("(unit)");
        for i in (0..30).rev() {
            src = format!(
                "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (str \"a{}\") {})",
                i, src
            );
        }
        let mut v = eval_ok(&mut h, &src);
        for i in 0..30 {
            match h.car(v).unwrap() {
                Value::Str(id) => assert_eq!(h.string(id), format!("a{}", i)),
                other => panic!("element {} is {:?}", i, other),
            }
            v = h.cdr(v).unwrap();
        }
    }

    // ---- panic -----------------------------------------------------------

    #[test]
    fn panic_carries_its_message() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, r#"(panic (str "boom"))"#).expect_err("panic should not produce a value");
        match e {
            EvalError::Panic(m) => assert_eq!(m, "boom"),
            other => panic!("expected a panic, got {:?}", other),
        }
    }

    /// The message is evaluated, not taken literally.
    #[test]
    fn a_panic_message_is_a_computed_value() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, r#"(let ((m r (str "late"))) (panic (var m)))"#)
            .expect_err("panic should not produce a value");
        match e {
            EvalError::Panic(m) => assert_eq!(m, "late"),
            other => panic!("expected a panic, got {:?}", other),
        }
    }

    // ---- what has not moved yet ------------------------------------------

    /// Ops still on the recursive evaluator reach `unimplemented!` rather than
    /// falling back to it. The panic is the point: a fallback would run
    /// correctly and hide which tags are left.
    #[test]
    #[should_panic(expected = "has not moved to the continuation stack yet")]
    fn an_op_that_has_not_moved_says_so() {
        let mut h = stress_heap();
        let _ = eval_src(&mut h, "(loop (int-any-width 1))");
    }
}
