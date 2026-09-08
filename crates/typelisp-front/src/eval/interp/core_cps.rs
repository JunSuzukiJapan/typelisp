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
use crate::check::repr::Repr;
use crate::eval::value::EvalError;

use super::core_eval::{
    bool_field, construct_sexpr_core, env_lookup, extend_env, heap_err, int_field, match_core_pattern, path_field,
    repr_list, str_field, sym_field, tail_after, tail_after_value, Op, Step, StepCmd,
};
use super::{FnDef, Interp};

use std::rc::Rc;

/// What one step of the machine produced.
enum State {
    /// Evaluate this form in this environment.
    Eval(Value, Value),
    /// A value is ready. Hand it to the top frame, or return it if there is none.
    Apply(Value),
    /// A non-local exit is in flight. Discard frames until one claims it.
    ///
    /// This is where `break`/`return`/`return-from`/`throw` live, and it is
    /// what replaces `Heap::set_in_flight_throw`: the value being carried past
    /// the frames being discarded is rooted through the state slots, so there
    /// is no need for a dedicated single-slot root — and no "at most one throw
    /// in flight" assumption to break once tasks exist.
    Unwind(EvalError),
}

/// Which node an `Args` frame is collecting arguments for.
///
/// Four core forms share one shape — a run of argument forms at a fixed offset,
/// evaluated left to right — and differ only in what happens once they are all
/// in hand. The leading fields they skip are the checker's and the bridge's
/// (names, paths, representations); evaluation is uniform over `Value`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ArgsKind {
    /// `(call WRITTEN HOME PATH (R...) ARG...)`
    Call,
    /// `(construct PATH KEY VARIANT MUTABLE (R...) ARG...)`
    Construct,
    /// `(assoc PATH SYM INSTANCE (HOME...) RET-REPR (R...) ARG...)`
    Assoc,
    /// `(dyn-call PATH SYM SLOT VTABLE (R...) ARG...)`
    DynCall,
}

impl ArgsKind {
    /// How many leading fields are not arguments.
    fn skip(self) -> usize {
        match self {
            ArgsKind::Call => 4,
            ArgsKind::Construct => 5,
            ArgsKind::Assoc => 7,
            ArgsKind::DynCall => 5,
        }
    }

    fn what(self) -> &'static str {
        match self {
            ArgsKind::Call => "call",
            ArgsKind::Construct => "construct",
            ArgsKind::Assoc => "assoc",
            ArgsKind::DynCall => "dyn-call",
        }
    }
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
    /// Waiting on one argument of a call, construction or method dispatch.
    ///
    /// `done` holds the arguments already evaluated, in order; the form
    /// carries the rest. An argument evaluated three ago is as collectible as
    /// the one that just arrived, so all of them stay rooted until the call
    /// itself is made.
    Args { form: Value, done: Vec<Value>, env: Value, kind: ArgsKind },
    /// `(step E)` — running `E` with stepping armed, holding what to restore.
    Step { was_stepping: bool, was_quiet: usize },
    /// A traced or stepped call, open around its callee's body.
    ///
    /// This is the one frame a call gets: `enter_fn` normally makes a body a
    /// tail jump, which leaves nothing to report a return from. Tracing needs
    /// the frame, so it costs the tail position — as it does in the recursive
    /// evaluator, where a traced call is a real Rust frame too.
    TracedCall { name: String, depth: usize },
    /// `(dyn-new STR PATH ((PATH SYM)...) (...) R E)` — waiting on the value
    /// to box.
    DynNew { form: Value },
    /// `(dyn-upcast PATH E)` — waiting on the trait object to re-view.
    DynUpcast { form: Value },
    /// `(apply E RET-R (R...) E...)` — waiting on the callee.
    ApplyCallee { form: Value, env: Value },
    /// `(apply E RET-R (R...) E...)` — waiting on one argument, callee in hand.
    ApplyArgs { form: Value, callee: Value, done: Vec<Value>, env: Value },
    /// `(match E R (P E...) ...)` — waiting on the scrutinee. The first arm
    /// whose pattern matches wins.
    MatchArms { form: Value, env: Value },
    /// `(field-set OBJ IDX R VALUE)` — waiting on the object.
    FieldSetObj { form: Value, env: Value },
    /// `(field-set OBJ IDX R VALUE)` — waiting on the value, object in hand.
    FieldSetVal { obj: Value, idx: usize },
    /// `(set-global (WRITTEN...) (HOME...) PATH R VALUE)` — waiting on the value.
    SetGlobal { form: Value },
    /// `(loop BODY...)` — running the body round and round. `rest` is what
    /// is left of this pass; when it runs out the frame starts over from
    /// `body`. The only way out is an exit this frame claims.
    Loop { body: Value, rest: Value, env: Value },
    /// `(block NAME BODY)` — the lexical named escape's landing site.
    ///
    /// The name is matched only to tell nested blocks apart while the signal
    /// travels: the checker already decided which block a `return-from`
    /// belongs to, so an unmatched one is not a runtime possibility the way a
    /// `throw` with no `catch` is.
    Block { name: String },
    /// `(catch 'TAG BODY)` — the dynamic escape's landing site. A throw on
    /// some *other* tag keeps travelling; swallowing it here is the bug CL's
    /// `eq` tag comparison exists to prevent.
    Catch { tag: String },
    /// `(return VALUE)` — waiting on the value to carry out.
    Return,
    /// `(return-from NAME VALUE)` — waiting on the value.
    ReturnFrom { name: String },
    /// `(throw 'TAG VALUE)` — waiting on the value.
    Throw { tag: String },
    /// `(unwind-protect PROTECTED CLEANUP)` — running `PROTECTED`. However it
    /// is left, the cleanup runs.
    Protect { cleanup: Value, env: Value },
    /// Running a cleanup after its protected form produced `value`; the
    /// cleanup's own value is discarded.
    CleanupValue { value: Value },
    /// Running a cleanup while `pending` waits to resume its flight.
    ///
    /// A non-local exit *from the cleanup itself* wins over the pending one,
    /// matching CLHS: "the cleanup-forms of unwind-protect are not protected
    /// by that unwind-protect".
    CleanupUnwind { pending: EvalError },
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
/// it was pushed (so unwinding one drops exactly the roots it added) and the
/// source location of the form that pushed it.
///
/// The location is what places a runtime error raised while *resuming* — the
/// value is in hand by then, and the form it belongs to is no longer the one
/// being evaluated. `EvalError::at` keeps the innermost, so a subexpression
/// that already placed the error still wins.
struct CpsStack {
    frames: Vec<(Frame, usize, Option<crate::Loc>)>,
}

impl CpsStack {
    fn new() -> CpsStack {
        CpsStack { frames: Vec::new() }
    }

    /// Pushes `frame`, rooting the values it holds.
    fn push(&mut self, heap: &mut Heap, frame: Frame, loc: Option<crate::Loc>) {
        let base = heap.root_count();
        match &frame {
            Frame::If { form, env } => {
                heap.push_root(*form);
                heap.push_root(*env);
            }
            Frame::Set { env, .. } => heap.push_root(*env),
            Frame::ApplyArgs { form, callee, done, env } => {
                heap.push_root(*form);
                heap.push_root(*callee);
                heap.push_root(*env);
                for v in done {
                    heap.push_root(*v);
                }
            }
            Frame::ApplyCallee { form, env } | Frame::MatchArms { form, env } | Frame::FieldSetObj { form, env } => {
                heap.push_root(*form);
                heap.push_root(*env);
            }
            // The box has to stay reachable while the value expression runs:
            // that expression can allocate, and nothing else refers to it.
            Frame::FieldSetVal { obj, .. } => heap.push_root(*obj),
            Frame::SetGlobal { form } | Frame::DynNew { form } | Frame::DynUpcast { form } => {
                heap.push_root(*form)
            }
            Frame::Loop { body, rest, env } => {
                heap.push_root(*body);
                heap.push_root(*rest);
                heap.push_root(*env);
            }
            Frame::Protect { cleanup, env } => {
                heap.push_root(*cleanup);
                heap.push_root(*env);
            }
            // The protected form's value has to outlive the cleanup, which
            // allocates — the one unwind that runs code before continuing.
            Frame::CleanupValue { value } => heap.push_root(*value),
            Frame::CleanupUnwind { pending } => {
                if let EvalError::Return(v) | EvalError::ReturnFrom(_, v) | EvalError::Throw(_, v) = pending {
                    heap.push_root(**v);
                }
            }
            // Names and tags are `String`s, and an exit carries its value in
            // the state slots rather than here. So do the stepping flags.
            Frame::Step { .. } | Frame::TracedCall { .. } => {}
            Frame::Block { .. } | Frame::Catch { .. } | Frame::Return | Frame::ReturnFrom { .. } | Frame::Throw { .. } => {}
            Frame::Args { form, done, env, .. } => {
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
        self.frames.push((frame, base, loc));
    }

    /// Removes the top frame. **The caller must `truncate_roots` to the
    /// returned base** — after it has finished reading the frame, and with no
    /// allocation between that truncation and re-rooting the next state.
    fn pop(&mut self) -> Option<(Frame, usize, Option<crate::Loc>)> {
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
        // The exit's value keeps its root for the whole flight: a cleanup on
        // the way out allocates, and every scope that rooted it is being
        // discarded. `Break` and a plain `Return` with no value carry nothing.
        State::Unwind(e) => {
            let carried = match e {
                EvalError::Return(v) | EvalError::ReturnFrom(_, v) | EvalError::Throw(_, v) => **v,
                _ => Value::Empty,
            };
            heap.set_root(sbase, carried);
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

/// The argument forms of a node whose arguments start at a fixed offset.
fn arg_forms(heap: &Heap, form: Value, kind: ArgsKind) -> Result<Vec<Value>, EvalError> {
    let fields =
        core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: ({} ..): {}", kind.what(), e)))?;
    let skip = kind.skip();
    if fields.len() < skip {
        return Err(EvalError::Internal(format!(
            "eval: malformed {}: {}",
            kind.what(),
            core::print(heap, form)
        )));
    }
    Ok(fields[skip..].to_vec())
}

/// The argument forms of an `(apply E RET-R (R...) E...)` node.
///
/// Unlike the `Args` kinds, the callee is field 0 rather than part of the run,
/// so this node needs its own pair of frames.
fn apply_arg_forms(heap: &Heap, form: Value) -> Result<Vec<Value>, EvalError> {
    let fields = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: (apply ..): {}", e)))?;
    if fields.len() < 3 {
        return Err(EvalError::Internal(format!("eval: malformed apply: {}", core::print(heap, form))));
    }
    Ok(fields[3..].to_vec())
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
                State::Eval(form, env) => {
                    // Read the location before stepping: it names the form the
                    // error came from, and `EvalError::at` keeps the innermost
                    // one, so a subexpression that already placed the error
                    // wins over the form containing it.
                    let loc = heap.cons_loc(form);
                    match self.step_cps(heap, &mut stack, form, env) {
                        Ok(next) => {
                            set_state(heap, sbase, &next);
                            state = next;
                        }
                        Err(e) => {
                            break Err(match loc {
                                Some(l) => e.at(l),
                                None => e,
                            })
                        }
                    }
                }
                State::Unwind(e) => match stack.pop() {
                    None => break Err(e),
                    Some((frame, fbase, loc)) => match self.unwind_through(heap, frame, e) {
                        Ok((next, pushed)) => {
                            heap.truncate_roots(fbase);
                            if let Some(f) = pushed {
                                stack.push(heap, f, loc);
                            }
                            set_state(heap, sbase, &next);
                            state = next;
                        }
                        Err(e) => {
                            break Err(match loc {
                                Some(l) => e.at(l),
                                None => e,
                            })
                        }
                    },
                },
                State::Apply(v) => match stack.pop() {
                    None => break Ok(v),
                    Some((frame, fbase, loc)) => match self.resume(heap, frame, v) {
                        Ok((next, pushed)) => {
                            // Nothing from here to `set_state` may allocate:
                            // the frame's roots are gone and whatever the next
                            // state carries lives only in Rust locals until it
                            // is rooted again. `truncate_roots`, `push` and
                            // `set_root` all leave the heap alone.
                            heap.truncate_roots(fbase);
                            if let Some(f) = pushed {
                                stack.push(heap, f, loc);
                            }
                            set_state(heap, sbase, &next);
                            state = next;
                        }
                        Err(e) => {
                            break Err(match loc {
                                Some(l) => e.at(l),
                                None => e,
                            })
                        }
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
        // Every frame this pushes belongs to `form`, and carries its location
        // so an error raised on resuming is placed there.
        let loc = heap.cons_loc(form);
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
                stack.push(heap, Frame::If { form, env }, loc.clone());
                Ok(State::Eval(cond, env))
            }

            Op::Panic => {
                let msg = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (panic ..) has no message".to_string()))?;
                stack.push(heap, Frame::Panic, loc.clone());
                Ok(State::Eval(msg, env))
            }

            Op::FieldGet => {
                let obj = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (field-get ..) has no object".to_string()))?;
                // Field 2 is the field's representation, read by the bridge
                // and by nothing here.
                let idx = int_field(heap, form, 1, "field-get")? as usize;
                stack.push(heap, Frame::FieldGet { idx }, loc.clone());
                Ok(State::Eval(obj, env))
            }

            Op::DynValue => {
                let inner = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (dyn-value ..) has no operand".to_string()))?;
                stack.push(heap, Frame::DynValue, loc.clone());
                Ok(State::Eval(inner, env))
            }

            Op::Set => {
                let sym = self.sym_field(heap, form, 0, "set")?;
                let val = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (set ..) has no value".to_string()))?;
                stack.push(heap, Frame::Set { sym, env }, loc.clone());
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
                        stack.push(heap, f, loc.clone());
                    }
                    return Ok(next);
                }
                let first = heap.car(binds).map_err(|_| bad("binding list is not a list"))?;
                let (_, init) = bind_parts(heap, first)?;
                stack.push(heap, Frame::LetInit { binds, rest: binds, done: Vec::new(), body, env }, loc.clone());
                Ok(State::Eval(init, env))
            }

            Op::Call => self.start_args(heap, stack, form, env, ArgsKind::Call, loc),
            Op::Construct => self.start_args(heap, stack, form, env, ArgsKind::Construct, loc),
            Op::Assoc => self.start_args(heap, stack, form, env, ArgsKind::Assoc, loc),
            Op::DynCall => self.start_args(heap, stack, form, env, ArgsKind::DynCall, loc),

            // `labels` evaluates nothing of its own: the closures are built
            // and bound, and only the body runs. Two passes, because the
            // siblings have to see each other — placeholders first, then each
            // closure written through its cell over the environment that
            // already names them all.
            // Arming the stepper is a side effect with an extent, so what to
            // restore rides on a frame — restored on the way out however the
            // body is left, which the recursive evaluator gets from the Rust
            // frame it has here.
            Op::Step => {
                let inner = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (step ..) has no form".to_string()))?;
                if !typelisp_rt::sys_builtin::stdin_is_tty() {
                    return Ok(State::Eval(inner, env));
                }
                let frame = Frame::Step {
                    was_stepping: self.stepping.get(),
                    was_quiet: self.step_quiet_depth.get(),
                };
                self.stepping.set(true);
                self.step_quiet_depth.set(usize::MAX);
                stack.push(heap, frame, loc.clone());
                Ok(State::Eval(inner, env))
            }

            Op::DynNew => {
                // Field 4 is the boxed value's representation, read by the
                // bridge and by nothing here.
                let value = core::field(heap, form, 5)
                    .ok_or_else(|| EvalError::Internal("eval: (dyn-new ..) has no value".to_string()))?;
                stack.push(heap, Frame::DynNew { form }, loc.clone());
                Ok(State::Eval(value, env))
            }

            Op::DynUpcast => {
                let value = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (dyn-upcast ..) has no operand".to_string()))?;
                stack.push(heap, Frame::DynUpcast { form }, loc.clone());
                Ok(State::Eval(value, env))
            }

            Op::Apply => {
                let callee = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (apply ..) has no callee".to_string()))?;
                stack.push(heap, Frame::ApplyCallee { form, env }, loc.clone());
                Ok(State::Eval(callee, env))
            }

            Op::Labels => {
                let defs = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (labels ..) has no definitions".to_string()))?;
                let defs = heap
                    .list_to_vec(defs)
                    .map_err(|e| EvalError::Internal(format!("eval: (labels ..) definitions: {}", e)))?;

                let names = |heap: &Heap, d: Value| match heap.car(d) {
                    Ok(Value::Symbol(sym)) => Ok(sym),
                    other => Err(EvalError::Internal(format!("eval: (labels ..) definition names {:?}", other))),
                };
                let placeholders: Vec<(SymRef, Value)> = defs
                    .iter()
                    .map(|d| names(heap, *d).map(|sym| (sym, Value::Empty)))
                    .collect::<Result<_, _>>()?;

                // The environment is rooted while the closures are built:
                // `alloc_closure` allocates, and nothing else refers to it
                // yet. The root goes above whatever frames are open, and comes
                // off before the body starts.
                let base = heap.root_count();
                let inner = extend_env(heap, &placeholders, env)?;
                heap.push_root(inner);
                for d in &defs {
                    // A definition is `(SYM PARAMS RET-R E...)` — positional,
                    // like a `let` binding and unlike a tagged node.
                    let sym = names(heap, *d)?;
                    let rest = heap.cdr(*d).map_err(heap_err)?;
                    let params = heap.car(rest).map_err(heap_err)?;
                    let ret = heap.cdr(rest).and_then(|d| heap.car(d)).map_err(heap_err)?;
                    let body = tail_after_value(heap, rest, 2)?;
                    let f = heap.alloc_closure(params, ret, body, inner);
                    let cell = env_lookup(heap, inner, sym).ok_or_else(|| {
                        EvalError::Internal(format!("eval: (labels ..) lost the slot for `{}`", heap.symbol_name(sym)))
                    })?;
                    match cell {
                        Value::Boxed(id) if heap.is_cell(id) => heap.cell_set(id, f),
                        other => {
                            return Err(EvalError::Internal(format!("eval: (labels ..) slot is {:?}", other)))
                        }
                    }
                }
                let body = tail_after(heap, form, 1)?;
                heap.truncate_roots(base);
                // Nothing allocates from here: `sequence_state` only walks
                // conses, so `inner` survives in a Rust local until the state
                // slots take it.
                let (next, pushed) = sequence_state(heap, body, inner)?;
                if let Some(f) = pushed {
                    stack.push(heap, f, loc.clone());
                }
                Ok(next)
            }

            Op::Match => {
                let scrut = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (match ..) has no scrutinee".to_string()))?;
                stack.push(heap, Frame::MatchArms { form, env }, loc.clone());
                Ok(State::Eval(scrut, env))
            }

            Op::FieldSet => {
                let obj = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (field-set ..) has no object".to_string()))?;
                stack.push(heap, Frame::FieldSetObj { form, env }, loc.clone());
                Ok(State::Eval(obj, env))
            }

            Op::SetGlobal => {
                let value = core::field(heap, form, 4)
                    .ok_or_else(|| EvalError::Internal("eval: (set-global ..) has no value form".to_string()))?;
                stack.push(heap, Frame::SetGlobal { form }, loc.clone());
                Ok(State::Eval(value, env))
            }

            Op::Loop => {
                let body = heap
                    .cdr(form)
                    .map_err(|e| EvalError::Internal(format!("eval: (loop ..): {}", e)))?;
                if matches!(body, Value::Empty) {
                    return Err(EvalError::Internal("eval: (loop ..) has no body".to_string()));
                }
                let first = heap
                    .car(body)
                    .map_err(|e| EvalError::Internal(format!("eval: (loop ..): {}", e)))?;
                stack.push(heap, Frame::Loop { body, rest: body, env }, loc.clone());
                Ok(State::Eval(first, env))
            }

            Op::Break => Ok(State::Unwind(EvalError::Break)),

            Op::Return => match core::field(heap, form, 0) {
                Some(val) => {
                    stack.push(heap, Frame::Return, loc.clone());
                    Ok(State::Eval(val, env))
                }
                None => Ok(State::Unwind(EvalError::Return(Box::new(Value::Empty)))),
            },

            Op::Block => {
                let name = self.block_name_of(heap, form)?;
                let body = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (block ..) has no body".to_string()))?;
                stack.push(heap, Frame::Block { name }, loc.clone());
                Ok(State::Eval(body, env))
            }

            Op::ReturnFrom => {
                let name = self.block_name_of(heap, form)?;
                match core::field(heap, form, 1) {
                    Some(val) => {
                        stack.push(heap, Frame::ReturnFrom { name }, loc.clone());
                        Ok(State::Eval(val, env))
                    }
                    None => Ok(State::Unwind(EvalError::ReturnFrom(name, Box::new(Value::Empty)))),
                }
            }

            Op::Catch => {
                let tag = self.throw_tag_of(heap, form, "catch")?;
                let body = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (catch ..) has no body".to_string()))?;
                stack.push(heap, Frame::Catch { tag }, loc.clone());
                Ok(State::Eval(body, env))
            }

            Op::Throw => {
                let tag = self.throw_tag_of(heap, form, "throw")?;
                let value = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (throw ..) has no value".to_string()))?;
                stack.push(heap, Frame::Throw { tag }, loc.clone());
                Ok(State::Eval(value, env))
            }

            Op::UnwindProtect => {
                let protected = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (unwind-protect ..) has no protected form".to_string()))?;
                let cleanup = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (unwind-protect ..) has no cleanup form".to_string()))?;
                stack.push(heap, Frame::Protect { cleanup, env }, loc.clone());
                Ok(State::Eval(protected, env))
            }

        }
    }

    /// Begins evaluating an argument run: the first argument, with a frame to
    /// collect it, or the node's own work when there are no arguments at all.
    fn start_args(
        &self,
        heap: &mut Heap,
        stack: &mut CpsStack,
        form: Value,
        env: Value,
        kind: ArgsKind,
        loc: Option<crate::Loc>,
    ) -> Result<State, EvalError> {
        let args = arg_forms(heap, form, kind)?;
        match args.first() {
            Some(&first) => {
                stack.push(heap, Frame::Args { form, done: Vec::new(), env, kind }, loc.clone());
                Ok(State::Eval(first, env))
            }
            None => {
                let (next, pushed) = self.finish_args(heap, form, Vec::new(), kind)?;
                if let Some(f) = pushed {
                    stack.push(heap, f, loc.clone());
                }
                Ok(next)
            }
        }
    }

    /// Every argument is in hand: do the node's own work.
    fn finish_args(
        &self,
        heap: &mut Heap,
        form: Value,
        argv: Vec<Value>,
        kind: ArgsKind,
    ) -> Result<(State, Option<Frame>), EvalError> {
        match kind {
            ArgsKind::Call => self.finish_call(heap, form, argv),
            ArgsKind::Construct => self.finish_construct(heap, form, argv),
            ArgsKind::Assoc => self.finish_assoc(heap, form, argv),
            ArgsKind::DynCall => self.finish_dyn_call(heap, form, argv),
        }
    }

    /// Enters an interpreted or compiled function body with `argv` bound.
    ///
    /// A user function's body becomes a sequence in a fresh environment, so
    /// **its last form is a tail jump** — a property the recursive evaluator
    /// does not have here (`Interp::apply` evaluates the last form with a
    /// native frame still waiting on it).
    fn enter_fn(&self, heap: &mut Heap, f: &Rc<FnDef>, argv: Vec<Value>) -> Result<(State, Option<Frame>), EvalError> {
        // Tracing and stepping report a call *around* the callee, in CL's own
        // shape: `  0: (fact 3)` going in and `  0: fact returned 6` coming
        // out. That needs a frame to come back to, so a watched call gives up
        // its tail position — the same trade the recursive evaluator makes,
        // where a traced call is a real Rust frame.
        //
        // Two `Cell` loads on the path every call takes; the set membership is
        // tested only once one of them is armed.
        let stepping = self.stepping.get() && self.trace_depth.get() <= self.step_quiet_depth.get();
        let watched = stepping || (self.trace_armed.get() && self.traced.borrow().contains(&f.name));
        let watch = if watched {
            let depth = self.trace_depth.get();
            let call = {
                let rendered: Vec<String> = argv.iter().map(|a| self.trace_render(heap, *a)).collect();
                if rendered.is_empty() {
                    format!("({})", f.name)
                } else {
                    format!("({} {})", f.name, rendered.join(" "))
                }
            };
            self.trace_line(heap, depth, &call);
            if stepping {
                match self.step_prompt(heap) {
                    StepCmd::Into => {}
                    // Quiet *below* this frame: the command is "run this
                    // call", and the frame it was given at is the one to ask
                    // at again.
                    StepCmd::Over => self.step_quiet_depth.set(depth),
                    StepCmd::Continue => self.stepping.set(false),
                    StepCmd::Quit => return Err(EvalError::Panic("step: aborted".to_string())),
                }
            }
            self.trace_depth.set(depth + 1);
            Some(Frame::TracedCall { name: f.name.clone(), depth })
        } else {
            None
        };

        // A compiled callee runs on the Rust stack: the boundary is one frame
        // from the machine's point of view, and nothing can suspend inside it.
        let compiled = f.compiled.borrow().clone();
        if let Some(compiled) = compiled {
            let sig = f.sig.as_ref().expect("a compiled function always has a type signature");
            let v = self.call_compiled(heap, compiled.as_ref(), &argv, &sig.0, &sig.1)?;
            // The call is over already, so a watch frame would have nothing to
            // wait for: report the return here instead.
            if let Some(Frame::TracedCall { name, depth }) = watch {
                self.trace_depth.set(depth);
                if self.step_quiet_depth.get() == depth {
                    self.step_quiet_depth.set(usize::MAX);
                }
                let text = self.trace_render(heap, v);
                self.trace_line(heap, depth, &format!("{} returned {}", name, text));
            }
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
        // A top-level function closes over nothing, so its frame extends the
        // empty environment rather than the caller's — that is what makes the
        // scope lexical.
        //
        // Rooted for the rest of this function: consing the body below
        // allocates, and nothing else refers to the environment yet. The root
        // goes above whatever frames are open, so the caller's truncation
        // drops it.
        let base = heap.root_count();
        let env = extend_env(heap, &binds, Value::Empty)?;
        heap.push_root(env);
        // A watched call has to hand back *its* frame, so the body cannot
        // also leave a sequence frame behind. Wrapping it as `(let () E...)`
        // — which is `progn` — makes it one form, and the `let` node opens
        // the sequence frame itself one step later. Only on the watched path,
        // which is already paying for a frame.
        if let Some(w) = watch {
            let mut items = Vec::with_capacity(f.body.len() + 1);
            items.push(Value::Empty);
            items.extend_from_slice(&f.body);
            let wrapped = core::tagged(heap, "let", &items)
                .map_err(|e| EvalError::Internal(format!("eval: watched call body: {}", e)))?;
            heap.truncate_roots(base);
            return Ok((State::Eval(wrapped, env), Some(w)));
        }
        // `FnDef::body` is a `Vec`, and a sequence frame walks a cons list, so
        // the body is consed up here. `core::list` roots every item and every
        // partial tail, so a collection mid-build is safe. It is the last
        // allocation on this path: `sequence_state` only walks conses, so
        // `body` and `env` stay in Rust locals — live because nothing
        // collects — until the caller roots them again.
        let body =
            core::list(heap, &f.body).map_err(|e| EvalError::Internal(format!("eval: call body: {}", e)))?;
        heap.truncate_roots(base);
        // Nothing allocates from here: `sequence_state` only walks conses.
        sequence_state(heap, body, env)
    }

    fn finish_call(&self, heap: &mut Heap, form: Value, argv: Vec<Value>) -> Result<(State, Option<Frame>), EvalError> {
        let written = self.name_list(heap, form, 0, "call")?;
        let home = self.name_list(heap, form, 1, "call")?;
        let path = path_field(heap, form, 2, "call")?;

        if let Some(f) = self.resolve_fn_named(&home, &written, &path) {
            return self.enter_fn(heap, &f, argv);
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

    /// The callee and every argument of an `apply` are in hand.
    ///
    /// Four kinds of callee: an interpreted closure (entered as a tail jump,
    /// so a self-call in tail position costs no stack), one that came *out* of
    /// compiled code (crossed on the Rust stack — the boundary is one frame
    /// and nothing suspends inside it), a built-in used as a function value,
    /// or something the checker should have rejected.
    fn finish_apply(
        &self,
        heap: &mut Heap,
        form: Value,
        f: Value,
        argv: Vec<Value>,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let fields = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: (apply ..): {}", e)))?;
        if fields.len() < 3 {
            return Err(EvalError::Internal(format!("eval: malformed apply: {}", core::print(heap, form))));
        }
        let ret_repr_field = fields[1];
        let arg_reprs_field = fields[2];

        let id = match f {
            Value::Boxed(id) if heap.is_closure(id) => id,

            // Its arguments cross the boundary and its result comes back, both
            // driven by the declared representations this node carries.
            Value::Boxed(id) if heap.is_compiled_closure(id) => {
                let arg_reprs = repr_list(heap, arg_reprs_field, "apply")?;
                let ret = Repr::read(heap, ret_repr_field)
                    .ok_or_else(|| EvalError::Internal("eval: (apply ..) has no return representation".to_string()))?;
                let (int_args, crossing_roots) = self.encode_crossing_args(heap, &argv, &arg_reprs, false)?;
                self.enter_compiled(heap);
                // An unwinding `(panic ...)` inside the closure runs none of
                // the pops below and leaves whatever roots the compiled body
                // pushed; the caller's `truncate_roots` past this frame is the
                // repair.
                let raw = crate::eval::crossing::catch_compiled_panic(|| Interp::call_closure_box(heap, id, &int_args))?;
                for _ in 0..crossing_roots {
                    heap.pop_root();
                }
                let v = self.decode_compiled_return(heap, raw, &ret)?;
                return Ok((State::Apply(v), None));
            }

            // A built-in used as a function value — dispatched by name through
            // the very same `eval_builtin`/`eval_builtin_method` a direct call
            // site goes through; the box carries only which name.
            Value::Boxed(id) if heap.is_builtin_fn(id) => {
                let name = heap.builtin_fn_name(id).to_string();
                let v = match heap.builtin_fn_recv(id) {
                    None => match self.eval_builtin(heap, &name, &argv) {
                        Some(r) => r?,
                        None => return Err(EvalError::NoSuchFunction(name)),
                    },
                    Some(pid) => {
                        let type_name = crate::types::path_from_id(heap, pid);
                        let ret_key = heap.builtin_fn_ret_key(id).to_string();
                        match super::eval_builtin_method(heap, &type_name, &name, &argv, &ret_key) {
                            Some(r) => r?,
                            None => {
                                return Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, name)))
                            }
                        }
                    }
                };
                return Ok((State::Apply(v), None));
            }

            other => {
                return Err(EvalError::Internal(format!(
                    "eval: (apply ..) callee is not a function: {:?}",
                    other
                )))
            }
        };

        // The call environment extends the environment the closure *captured*,
        // not the caller's: that is what makes this lexical scope rather than
        // dynamic. `closure_frame` roots it on the heap.
        let base = heap.root_count();
        let (call_env, body) = self.closure_frame(heap, id, argv)?;
        let body = core::list(heap, &body)
            .map_err(|e| EvalError::Internal(format!("eval: (apply ..) body: {}", e)))?;
        heap.truncate_roots(base);
        // Nothing allocates from here: `sequence_state` only walks conses.
        sequence_state(heap, body, call_env)
    }

    /// `(construct PATH KEY VARIANT MUTABLE (R...) ARG...)` — a struct, enum
    /// variant or `Sexpr` constructor.
    fn finish_construct(
        &self,
        heap: &mut Heap,
        form: Value,
        argv: Vec<Value>,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let path = path_field(heap, form, 0, "construct")?;
        // The value's runtime identity, spelled by the checker where the
        // instantiation was known: `gen<i32>` and `gen<string>` share `path`
        // and differ here.
        let key = str_field(heap, form, 1, "construct")?;
        let variant = int_field(heap, form, 2, "construct")? as usize;
        let mutable = bool_field(heap, form, 3, "construct")?;

        let v = if super::is_sexpr_type(&path) {
            construct_sexpr_core(heap, variant, &argv)?
        } else if mutable {
            crate::type_key::alloc_struct_keyed(heap, &key, argv)
        } else {
            crate::type_key::alloc_enum_keyed(heap, &key, variant, argv)
        };
        Ok((State::Apply(v), None))
    }

    /// `(assoc PATH SYM INSTANCE (HOME...) RET-REPR (R...) ARG...)` — a method
    /// call. The receiver, when there is one, is `argv[0]`.
    fn finish_assoc(
        &self,
        heap: &mut Heap,
        form: Value,
        argv: Vec<Value>,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let type_name = path_field(heap, form, 0, "assoc")?;
        let method = sym_field(heap, form, 1, "assoc")?;
        let home = self.name_list(heap, form, 3, "assoc")?;
        // The result's runtime identity, for the built-in methods that build a
        // box: `Vector::new` has no field to read an instantiation off.
        let ret_key = str_field(heap, form, 6, "assoc")?;

        let f = self.root.borrow().resolve_method(&home, &type_name, &method);
        match f {
            Some(f) => self.enter_fn(heap, &f, argv),
            None => match super::eval_builtin_method(heap, &type_name, &method, &argv, &ret_key) {
                Some(result) => Ok((State::Apply(result?), None)),
                None => Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, method))),
            },
        }
    }

    /// `(dyn-call PATH SYM SLOT VTABLE (R...) ARG...)` — a call through a
    /// trait object's vtable.
    ///
    /// `argv[0]` is the fat box; the callee is an ordinary method body with an
    /// ordinary receiver, so the concrete value is substituted in its place and
    /// nothing about the callee knows it was reached dynamically.
    fn finish_dyn_call(
        &self,
        heap: &mut Heap,
        form: Value,
        mut argv: Vec<Value>,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let trait_path = path_field(heap, form, 0, "dyn-call")?;
        let method = sym_field(heap, form, 1, "dyn-call")?;
        let slot = int_field(heap, form, 2, "dyn-call")? as usize;

        let (vtable_id, inner) = match argv.first() {
            Some(Value::Boxed(id)) if heap.is_dyn(*id) => (heap.dyn_vtable_id(*id), heap.dyn_value(*id)),
            other => {
                return Err(EvalError::Internal(format!(
                    "eval: (dyn-call {}::{} ..): receiver is not a trait object ({:?})",
                    trait_path, method, other
                )))
            }
        };
        let target = self.vtables.borrow().get(vtable_id as usize).and_then(|slots| slots.get(slot)).cloned();
        let Some((target_type, target_method)) = target else {
            return Err(EvalError::Internal(format!(
                "eval: (dyn-call {}::{} ..): vtable {} has no slot {}",
                trait_path, method, vtable_id, slot
            )));
        };
        argv[0] = inner;
        // Visibility was settled where the value was boxed, so this is the
        // direct lookup, not `resolve_method`'s `home`-relative one.
        let f = self.root.borrow().get_method(&target_type, &target_method);
        match f {
            Some(f) => self.enter_fn(heap, &f, argv),
            None => Err(EvalError::NoSuchFunction(format!("{}::{}", target_type, target_method))),
        }
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

            Frame::Args { form, mut done, env, kind } => {
                done.push(v);
                let args = arg_forms(heap, form, kind)?;
                match args.get(done.len()) {
                    Some(&next) => Ok((State::Eval(next, env), Some(Frame::Args { form, done, env, kind }))),
                    None => self.finish_args(heap, form, done, kind),
                }
            }

            // Only the last form of a sequence contributes a value, so this
            // one is discarded.
            Frame::Seq { rest, env } => sequence_state(heap, rest, env),

            Frame::Step { was_stepping, was_quiet } => {
                self.stepping.set(was_stepping);
                self.step_quiet_depth.set(was_quiet);
                Ok((State::Apply(v), None))
            }

            // The depth is restored *before* the return is reported, so the
            // two lines of one call line up.
            Frame::TracedCall { name, depth } => {
                self.trace_depth.set(depth);
                if self.step_quiet_depth.get() == depth {
                    self.step_quiet_depth.set(usize::MAX);
                }
                let text = self.trace_render(heap, v);
                self.trace_line(heap, depth, &format!("{} returned {}", name, text));
                Ok((State::Apply(v), None))
            }

            // Boxing is where the concrete type is still known, so it is
            // where every vtable this value could be viewed through gets
            // interned. Shared with the recursive evaluator rather than
            // copied — see `Interp::dyn_new_with_value`.
            Frame::DynNew { form } => Ok((State::Apply(self.dyn_new_with_value(heap, form, v)?), None)),
            Frame::DynUpcast { form } => Ok((State::Apply(self.dyn_upcast_with_value(heap, form, v)?), None)),

            // An unnamed callee — `((make-adder 1) 2)` — has no binding
            // keeping its box alive while the arguments allocate, so the
            // frame roots it.
            Frame::ApplyCallee { form, env } => {
                let args = apply_arg_forms(heap, form)?;
                match args.first() {
                    Some(&first) => Ok((
                        State::Eval(first, env),
                        Some(Frame::ApplyArgs { form, callee: v, done: Vec::new(), env }),
                    )),
                    None => self.finish_apply(heap, form, v, Vec::new()),
                }
            }
            Frame::ApplyArgs { form, callee, mut done, env } => {
                done.push(v);
                let args = apply_arg_forms(heap, form)?;
                match args.get(done.len()) {
                    Some(&next) => Ok((
                        State::Eval(next, env),
                        Some(Frame::ApplyArgs { form, callee, done, env }),
                    )),
                    None => self.finish_apply(heap, form, callee, done),
                }
            }

            // The scrutinee is rooted through the state slots for the whole
            // search, which matters because matching a `Sexpr` path pattern
            // allocates.
            Frame::MatchArms { form, env } => {
                let arms = core::fields(heap, form)
                    .map_err(|e| EvalError::Internal(format!("eval: (match ..): {}", e)))?;
                // The scrutinee, its representation (the bridge's), then the arms.
                for arm in arms.iter().skip(2) {
                    let parts = heap
                        .list_to_vec(*arm)
                        .map_err(|e| EvalError::Internal(format!("eval: (match ..) arm: {}", e)))?;
                    let Some((pat, body)) = parts.split_first() else {
                        return Err(EvalError::Internal("eval: (match ..) arm is empty".to_string()));
                    };
                    let Some(binds) = match_core_pattern(self, heap, env, *pat, v)? else {
                        continue;
                    };
                    // Both of these allocate, so the first result is rooted
                    // across the second. The root goes above this frame's, so
                    // the caller's truncation drops it.
                    let base = heap.root_count();
                    let new_env = extend_env(heap, &binds, env)?;
                    heap.push_root(new_env);
                    let body = core::list(heap, body)
                        .map_err(|e| EvalError::Internal(format!("eval: (match ..) arm body: {}", e)))?;
                    heap.truncate_roots(base);
                    // Nothing allocates from here: `sequence_state` only walks
                    // conses.
                    return sequence_state(heap, body, new_env);
                }
                Err(EvalError::Internal("eval: no matching match arm".to_string()))
            }

            // Field 2 is the field's representation, read by the bridge and
            // by nothing here.
            Frame::FieldSetObj { form, env } => {
                let idx = int_field(heap, form, 1, "field-set")? as usize;
                let val = core::field(heap, form, 3)
                    .ok_or_else(|| EvalError::Internal("eval: (field-set ..) has no value".to_string()))?;
                Ok((State::Eval(val, env), Some(Frame::FieldSetVal { obj: v, idx })))
            }
            Frame::FieldSetVal { obj, idx } => {
                let id = super::expect_struct_box(&obj)?;
                heap.struct_set_field(id, idx, v);
                Ok((State::Apply(Value::Empty), None))
            }

            // A global some compiled function reads or writes lives in a
            // permanent GC root instead of an ordinary cell, and must be
            // written through that same storage — otherwise an interpreted
            // read could see a stale value a compiled write already updated.
            Frame::SetGlobal { form } => {
                let written = self.name_list(heap, form, 0, "set-global")?;
                let home = self.name_list(heap, form, 1, "set-global")?;
                let path = path_field(heap, form, 2, "set-global")?;
                if let Some(&id) = self.compiled_globals.borrow().get(&path) {
                    let perm_idx = typelisp_rt::global_perm_idx(id).ok_or_else(|| {
                        EvalError::Internal(format!("global \"{}\": unknown compiled id {}", path, id))
                    })?;
                    heap.set_permanent_root(perm_idx, v);
                    return Ok((State::Apply(Value::Empty), None));
                }
                match self.resolve_global_named(&home, &written, &path) {
                    Some(slot) => {
                        slot.set(heap, v)?;
                        Ok((State::Apply(Value::Empty), None))
                    }
                    None => Err(EvalError::Unbound(path.to_string())),
                }
            }

            // A loop body's value is discarded and the body starts again. The
            // frame is rebuilt rather than kept, so the stack does not grow
            // with the iteration count.
            Frame::Loop { body, rest, env } => {
                let bad = |e| EvalError::Internal(format!("eval: (loop ..): {}", e));
                let next = heap.cdr(rest).map_err(bad)?;
                let rest = if matches!(next, Value::Empty) { body } else { next };
                let first = heap.car(rest).map_err(bad)?;
                Ok((State::Eval(first, env), Some(Frame::Loop { body, rest, env })))
            }

            // Reached without an exit: the body's value is the form's.
            Frame::Block { .. } | Frame::Catch { .. } => Ok((State::Apply(v), None)),

            Frame::Return => Ok((State::Unwind(EvalError::Return(Box::new(v))), None)),
            Frame::ReturnFrom { name } => Ok((State::Unwind(EvalError::ReturnFrom(name, Box::new(v))), None)),
            Frame::Throw { tag } => Ok((State::Unwind(EvalError::Throw(tag, Box::new(v))), None)),

            // The protected form finished normally. Its value waits in a
            // frame while the cleanup runs.
            Frame::Protect { cleanup, env } => {
                Ok((State::Eval(cleanup, env), Some(Frame::CleanupValue { value: v })))
            }
            // A cleanup's own value is discarded.
            Frame::CleanupValue { value } => Ok((State::Apply(value), None)),
            Frame::CleanupUnwind { pending } => Ok((State::Unwind(pending), None)),

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

    /// Carries `exit` past `frame` — the unwinding half of a frame's job.
    ///
    /// Most frames have nothing to say and are simply discarded: that is what
    /// makes a non-local exit skip the intervening computation. Three claim an
    /// exit (`Loop`, `Block`, `Catch`) and one runs code on the way past
    /// (`Protect`).
    fn unwind_through(&self, heap: &mut Heap, frame: Frame, exit: EvalError) -> Result<(State, Option<Frame>), EvalError> {
        match frame {
            // `break` and `return` are caught by the nearest enclosing loop —
            // the checker guarantees there is one.
            Frame::Loop { .. } => match exit {
                EvalError::Break => Ok((State::Apply(Value::Empty), None)),
                EvalError::Return(v) => Ok((State::Apply(*v), None)),
                other => Ok((State::Unwind(other), None)),
            },

            Frame::Block { name } => match exit {
                EvalError::ReturnFrom(from, v) if from == name => Ok((State::Apply(*v), None)),
                other => Ok((State::Unwind(other), None)),
            },

            Frame::Catch { tag } => match exit {
                EvalError::Throw(thrown, v) if thrown == tag => Ok((State::Apply(*v), None)),
                other => Ok((State::Unwind(other), None)),
            },

            // The flags are restored however the body is left.
            Frame::Step { was_stepping, was_quiet } => {
                self.stepping.set(was_stepping);
                self.step_quiet_depth.set(was_quiet);
                Ok((State::Unwind(exit), None))
            }

            // An unwind past a traced frame is exactly the moment a trace is
            // most worth having, so the exit is reported rather than silent.
            Frame::TracedCall { name, depth } => {
                self.trace_depth.set(depth);
                if self.step_quiet_depth.get() == depth {
                    self.step_quiet_depth.set(usize::MAX);
                }
                self.trace_line(heap, depth, &format!("{} exited non-locally: {}", name, exit));
                Ok((State::Unwind(exit), None))
            }

            // The cleanup runs on *every* way out, with the exit parked in a
            // frame until it is done.
            Frame::Protect { cleanup, env } => {
                Ok((State::Eval(cleanup, env), Some(Frame::CleanupUnwind { pending: exit })))
            }

            // An exit raised by a cleanup itself replaces the one that was in
            // flight (CLHS: the cleanup-forms are not protected by their own
            // unwind-protect). The pending exit is simply dropped.
            Frame::CleanupValue { .. } | Frame::CleanupUnwind { .. } => Ok((State::Unwind(exit), None)),

            // Everything else is computation the exit is escaping past.
            Frame::If { .. }
            | Frame::Panic
            | Frame::FieldGet { .. }
            | Frame::DynValue
            | Frame::Set { .. }
            | Frame::Seq { .. }
            | Frame::Return
            | Frame::ReturnFrom { .. }
            | Frame::Throw { .. }
            | Frame::Args { .. }
            | Frame::ApplyCallee { .. }
            | Frame::ApplyArgs { .. }
            | Frame::MatchArms { .. }
            | Frame::FieldSetObj { .. }
            | Frame::FieldSetVal { .. }
            | Frame::SetGlobal { .. }
            | Frame::DynNew { .. }
            | Frame::DynUpcast { .. }
            | Frame::LetInit { .. } => Ok((State::Unwind(exit), None)),
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

    /// Peels the location layers off an error. A runtime error is placed at
    /// the node it came from (`EvalError::At`), which is the right behaviour
    /// and not what these tests are checking.
    fn unlocated(e: &EvalError) -> &EvalError {
        match e {
            EvalError::At(_, inner) => unlocated(inner),
            other => other,
        }
    }

    fn eval_ok(heap: &mut Heap, src: &str) -> Value {
        eval_src(heap, src).unwrap_or_else(|e| panic!("{:?} failed to evaluate: {}", src, e))
    }

    /// The same form through both evaluators, asserting they agree. The whole
    /// premise of the rewrite is that it changes no program's meaning, so most
    /// tests here should be able to say exactly this.
    ///
    /// **Not for forms that produce a freshly allocated value.** `Value`
    /// compares strings and boxes by identity, and a string literal allocates
    /// a new string every time it is evaluated (deliberately — see
    /// `a_string_literal_is_a_fresh_string_every_time` in `core_eval`), so the
    /// two runs would differ on the id even when they agree on the content.
    /// Use `eval_ok` and compare what is inside.
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
        assert!(matches!(unlocated(&e), EvalError::Internal(_)), "expected an internal error, got {:?}", e);
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
        assert!(matches!(unlocated(&e), EvalError::Unbound(_)), "expected Unbound, got {:?}", e);
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

    // ---- construct / field-get -------------------------------------------

    #[test]
    fn construct_builds_a_struct_and_field_get_reads_it() {
        let mut h = stress_heap();
        let point = "(construct point \"point\" 0 true (int-any-width int-any-width) \
                       (int-any-width 1) (int-any-width 2))";
        assert_eq!(
            agrees(&mut h, &format!("(field-get {} 1 int-any-width)", point)),
            Value::Int(2)
        );
        assert_eq!(
            agrees(&mut h, &format!("(field-get {} 0 int-any-width)", point)),
            Value::Int(1)
        );
    }

    /// A field whose value is itself a construction: the outer node's earlier
    /// fields have to survive the inner one's allocation.
    #[test]
    fn a_nested_construction_does_not_lose_the_outer_fields() {
        let mut h = stress_heap();
        let inner = "(construct point \"point\" 0 true (int-any-width int-any-width) \
                       (int-any-width 3) (int-any-width 4))";
        let outer = format!(
            "(construct pair \"pair\" 0 true (str point) (str \"tag\") {})",
            inner
        );
        let src = format!("(field-get (field-get {} 1 point) 0 int-any-width)", outer);
        assert_eq!(agrees(&mut h, &src), Value::Int(3));
    }

    /// `set` on a struct field's binding, then reading it back: the value
    /// written has to be the value read, through the same cell.
    #[test]
    fn a_struct_survives_being_bound_and_read_back() {
        let mut h = stress_heap();
        let src = "(let ((p sexpr (construct point \"point\" 0 true (int-any-width int-any-width) \
                                    (int-any-width 5) (int-any-width 6))))
                     (field-get (var p) 1 int-any-width))";
        assert_eq!(agrees(&mut h, src), Value::Int(6));
    }

    // ---- labels / apply --------------------------------------------------

    #[test]
    fn a_lambda_is_applied_to_its_arguments() {
        let mut h = stress_heap();
        let src = "(apply (lambda ((x int-any-width)) int-any-width (var x)) int-any-width \
                     (int-any-width) (int-any-width 5))";
        assert_eq!(agrees(&mut h, src), Value::Int(5));
        assert_eq!(
            agrees(&mut h, "(apply (lambda () int-any-width (int-any-width 7)) int-any-width ())"),
            Value::Int(7)
        );
    }

    /// A closure sees what it captured, not what the caller has bound.
    #[test]
    fn a_closure_captures_its_defining_environment() {
        let mut h = stress_heap();
        let src = "(let ((n int-any-width (int-any-width 3)))
                     (apply (lambda () int-any-width (var n)) int-any-width ()))";
        assert_eq!(agrees(&mut h, src), Value::Int(3));
    }

    /// `labels` siblings see each other — the placeholder pass is what makes
    /// mutual recursion work.
    #[test]
    fn labels_siblings_can_call_each_other() {
        let mut h = stress_heap();
        let src = "(labels ((f ((x int-any-width)) int-any-width \
                              (apply (var g) int-any-width (int-any-width) (var x))) \
                            (g ((y int-any-width)) int-any-width (var y))) \
                     (apply (var f) int-any-width (int-any-width) (int-any-width 3)))";
        assert_eq!(agrees(&mut h, src), Value::Int(3));
    }

    /// A builtin used as a function value goes through the same dispatch a
    /// direct call site does — the `fnref` box carries only which name.
    #[test]
    fn a_builtin_can_be_applied_as_a_value() {
        let mut h = stress_heap();
        let src = "(apply (fnref (sexpr-cons) () sexpr-cons) sexpr (sexpr sexpr) \
                     (int-any-width 1) (int-any-width 2))";
        let v = agrees(&mut h, src);
        assert_eq!(h.car(v).unwrap(), Value::Int(1));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(2));
    }

    // ---- the point of the whole rewrite ----------------------------------

    /// Builds `(cons 1 (cons 1 ... (cons 1 ())))` by recursion that is **not**
    /// in tail position: each call has to come back to cons its result.
    ///
    /// In the recursive evaluator every one of those pending calls is a live
    /// Rust frame. **Measured**: the same form through `eval_core` aborts with
    /// `fatal runtime error: stack overflow` at this depth — with the default
    /// test stack *and* with `RUST_MIN_STACK=32MB`, the value
    /// `scripts/test-serial.sh` sets. Here the pending calls are frames on a
    /// `Vec` and the Rust stack stays flat, which is the property a task needs
    /// in order to be suspended at all.
    ///
    /// No `gc_stress`: the point is depth, and collecting at every one of the
    /// allocations below would make this quadratic.
    #[test]
    fn deep_non_tail_recursion_does_not_touch_the_rust_stack() {
        const DEPTH: i64 = 20_000;
        let mut h = Heap::with_capacity(1 << 20);

        let src = format!(
            "(labels ((build ((n int-any-width)) sexpr
                        (if (call (sexpr-null) () sexpr-null (sexpr) (var n))
                            (unit)
                            (call (sexpr-cons) () sexpr-cons (sexpr sexpr)
                              (int-any-width 1)
                              (apply (var build) sexpr (sexpr)
                                (call (sexpr-cdr) () sexpr-cdr (sexpr) (var n)))))))
               (apply (var build) sexpr (sexpr) (var xs)))"
        );

        // `xs` is a list of DEPTH elements, built on the Rust side so the
        // source stays small.
        let mut xs = Value::Empty;
        h.push_root(xs);
        for _ in 0..DEPTH {
            xs = h.cons(Value::Int(0), xs).expect("heap exhausted building the input");
            h.set_root(0, xs);
        }
        let sym = match h.intern_symbol("xs") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let env = extend_env(&mut h, &[(sym, xs)], Value::Empty).expect("could not build the environment");
        h.push_root(env);

        let form = read1(&mut h, &src);
        h.push_root(form);

        let interp = Interp::new();
        let v = interp
            .eval_cps(&mut h, form, env)
            .expect("deep non-tail recursion failed");

        // The result is a list of DEPTH ones.
        let mut n = 0i64;
        let mut cur = v;
        while !matches!(cur, Value::Empty) {
            assert_eq!(h.car(cur).unwrap(), Value::Int(1));
            cur = h.cdr(cur).unwrap();
            n += 1;
        }
        assert_eq!(n, DEPTH, "the rebuilt list is the wrong length");
    }

    // ---- match -----------------------------------------------------------

    #[test]
    fn match_takes_the_first_arm_that_matches() {
        let mut h = stress_heap();
        let src = "(match (int-any-width 2) int-any-width \
                     ((pat-lit (int-any-width 1)) (int-any-width 10)) \
                     ((pat-lit (int-any-width 2)) (int-any-width 20)) \
                     ((pat-wild) (int-any-width 99)))";
        assert_eq!(agrees(&mut h, src), Value::Int(20));
    }

    #[test]
    fn a_wildcard_arm_catches_what_the_others_do_not() {
        let mut h = stress_heap();
        let src = "(match (int-any-width 7) int-any-width \
                     ((pat-lit (int-any-width 1)) (int-any-width 10)) \
                     ((pat-wild) (int-any-width 99)))";
        assert_eq!(agrees(&mut h, src), Value::Int(99));
    }

    /// A binding pattern extends the environment the arm's body runs in.
    #[test]
    fn a_binding_pattern_is_visible_in_the_arm() {
        let mut h = stress_heap();
        assert_eq!(
            agrees(&mut h, "(match (int-any-width 5) int-any-width ((pat-bind x) (var x)))"),
            Value::Int(5)
        );
    }

    /// An arm body of several forms runs in order, last value wins — and the
    /// bound value has to survive the earlier ones allocating.
    #[test]
    fn an_arm_body_is_a_sequence() {
        let mut h = stress_heap();
        let src = r#"(match (str "kept") string ((pat-bind x) (str "discarded") (var x)))"#;
        match eval_ok(&mut h, src) {
            Value::Str(id) => assert_eq!(h.string(id), "kept"),
            other => panic!("expected the bound string, got {:?}", other),
        }
    }

    // ---- field-set / set-global ------------------------------------------

    #[test]
    fn field_set_writes_through_the_struct() {
        let mut h = stress_heap();
        let src = "(let ((p sexpr (construct point \"point\" 0 true (int-any-width int-any-width) \
                                    (int-any-width 1) (int-any-width 2))))
                     (field-set (var p) 1 int-any-width (int-any-width 42))
                     (field-get (var p) 1 int-any-width))";
        assert_eq!(agrees(&mut h, src), Value::Int(42));
    }

    /// The object is evaluated before the value, and has to stay reachable
    /// while the value expression allocates.
    #[test]
    fn the_object_survives_an_allocating_value_expression() {
        let mut h = stress_heap();
        let src = r#"(let ((p sexpr (construct point "point" 0 true (int-any-width str)
                                      (int-any-width 1) (str "old"))))
                       (field-set (var p) 1 str (str "new"))
                       (field-get (var p) 1 str))"#;
        match eval_ok(&mut h, src) {
            Value::Str(id) => assert_eq!(h.string(id), "new"),
            other => panic!("expected the new string, got {:?}", other),
        }
    }

    // ---- panic -----------------------------------------------------------

    #[test]
    fn panic_carries_its_message() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, r#"(panic (str "boom"))"#).expect_err("panic should not produce a value");
        match unlocated(&e) {
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
        match unlocated(&e) {
            EvalError::Panic(m) => assert_eq!(m, "late"),
            other => panic!("expected a panic, got {:?}", other),
        }
    }

    // ---- loop, break, return ---------------------------------------------

    /// Arithmetic lowers to method calls (`Op::Assoc`), which have not moved
    /// yet, so a loop here counts down a cons list with the `sexpr-*`
    /// builtins instead.
    fn countdown_list(n: usize) -> String {
        let mut src = String::from("(unit)");
        for i in (0..n).rev() {
            src = format!(
                "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width {}) {})",
                i, src
            );
        }
        src
    }

    #[test]
    fn a_loop_runs_until_it_is_broken_out_of() {
        let mut h = stress_heap();
        let src = format!(
            "(let ((lst r {}) (seen r (int-any-width 0)))
               (loop (if (call (sexpr-null) () sexpr-null (sexpr) (var lst)) (break) (unit))
                     (set seen (call (sexpr-car) () sexpr-car (sexpr) (var lst)))
                     (set lst (call (sexpr-cdr) () sexpr-cdr (sexpr) (var lst))))
               (var seen))",
            countdown_list(5)
        );
        // The last element seen before the list ran out.
        assert_eq!(agrees(&mut h, &src), Value::Int(4));
    }

    /// `return` carries a value out of the nearest loop; `break` carries unit.
    #[test]
    fn return_carries_a_value_out_of_the_loop() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(loop (return (int-any-width 7)))"), Value::Int(7));
        assert_eq!(agrees(&mut h, "(loop (break))"), Value::Empty);
        assert_eq!(agrees(&mut h, "(loop (return))"), Value::Empty);
    }

    /// The *nearest* loop claims the exit, so an inner `break` leaves the
    /// outer one running.
    #[test]
    fn break_leaves_only_the_nearest_loop() {
        let mut h = stress_heap();
        // The inner `(loop (break))` must not end the outer one, which walks
        // the whole list.
        let src = format!(
            "(let ((lst r {}) (seen r (int-any-width 0)))
               (loop (loop (break))
                     (if (call (sexpr-null) () sexpr-null (sexpr) (var lst)) (break) (unit))
                     (set seen (call (sexpr-car) () sexpr-car (sexpr) (var lst)))
                     (set lst (call (sexpr-cdr) () sexpr-cdr (sexpr) (var lst))))
               (var seen))",
            countdown_list(3)
        );
        assert_eq!(agrees(&mut h, &src), Value::Int(2));
    }

    /// A loop of any length costs one frame: it is rebuilt, not stacked. 300
    /// iterations, each allocating, with a collection at every allocation.
    #[test]
    fn a_long_loop_does_not_grow_the_stack() {
        let mut h = stress_heap();
        let src = format!(
            "(let ((lst r {}) (seen r (int-any-width 0)))
               (loop (if (call (sexpr-null) () sexpr-null (sexpr) (var lst)) (break) (unit))
                     (set seen (call (sexpr-car) () sexpr-car (sexpr) (var lst)))
                     (set lst (call (sexpr-cdr) () sexpr-cdr (sexpr) (var lst))))
               (var seen))",
            countdown_list(300)
        );
        assert_eq!(eval_ok(&mut h, &src), Value::Int(299));
    }

    // ---- block / return-from ---------------------------------------------

    #[test]
    fn return_from_leaves_the_named_block() {
        let mut h = stress_heap();
        assert_eq!(
            agrees(&mut h, r#"(block (str "b") (let () (return-from (str "b") (int-any-width 3)) (int-any-width 9)))"#),
            Value::Int(3)
        );
    }

    /// A block reached without an exit is just its body.
    #[test]
    fn a_block_without_an_exit_is_its_body() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, r#"(block (str "b") (int-any-width 4))"#), Value::Int(4));
    }

    /// The exit passes through the inner block, which does not claim it.
    #[test]
    fn return_from_passes_through_blocks_of_other_names() {
        let mut h = stress_heap();
        let src = r#"(block (str "outer")
                       (block (str "inner")
                         (return-from (str "outer") (int-any-width 8))))"#;
        assert_eq!(agrees(&mut h, src), Value::Int(8));
    }

    // ---- catch / throw ---------------------------------------------------

    #[test]
    fn a_catch_without_a_throw_is_its_body() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(catch (quote done) (int-any-width 3))"), Value::Int(3));
    }

    #[test]
    fn a_throw_reaches_the_catch_on_its_tag() {
        let mut h = stress_heap();
        let src = "(catch (quote done) (let () (throw (quote done) (int-any-width 42)) (int-any-width 9)))";
        assert_eq!(agrees(&mut h, src), Value::Int(42));
    }

    /// A throw on another tag keeps travelling — the bug CL's `eq` tag
    /// comparison exists to prevent.
    #[test]
    fn a_throw_passes_through_a_catch_on_another_tag() {
        let mut h = stress_heap();
        let src = "(catch (quote outer)
                     (catch (quote inner)
                       (throw (quote outer) (int-any-width 5))))";
        assert_eq!(agrees(&mut h, src), Value::Int(5));
    }

    /// The thrown value stays rooted for the whole flight. Under `gc_stress`
    /// the cleanup on the way out collects, so a value the state slots failed
    /// to root would be gone by the time the catch claims it. This is what
    /// replaces `Heap::set_in_flight_throw`.
    #[test]
    fn a_thrown_value_survives_a_cleanup_that_allocates() {
        let mut h = stress_heap();
        let src = r#"(catch (quote done)
                       (unwind-protect
                         (throw (quote done) (str "carried"))
                         (str "the cleanup allocates")))"#;
        match eval_ok(&mut h, src) {
            Value::Str(id) => assert_eq!(h.string(id), "carried"),
            other => panic!("the thrown value did not survive the cleanup: {:?}", other),
        }
    }

    // ---- unwind-protect --------------------------------------------------

    /// The cleanup runs and its value is discarded.
    #[test]
    fn unwind_protect_returns_the_protected_value() {
        let mut h = stress_heap();
        let src = "(let ((n r (int-any-width 0)))
                     (unwind-protect (int-any-width 1) (set n (int-any-width 99)))
                     (var n))";
        assert_eq!(agrees(&mut h, src), Value::Int(99));
        assert_eq!(
            agrees(&mut h, "(unwind-protect (int-any-width 1) (int-any-width 2))"),
            Value::Int(1)
        );
    }

    /// The cleanup runs however the protected form is left — here by `break`.
    #[test]
    fn the_cleanup_runs_on_a_break_out_of_the_protected_form() {
        let mut h = stress_heap();
        let src = "(let ((n r (int-any-width 0)))
                     (loop (unwind-protect (break) (set n (int-any-width 7))))
                     (var n))";
        assert_eq!(agrees(&mut h, src), Value::Int(7));
    }

    /// The protected form's value survives a cleanup that allocates — the one
    /// unwind that runs code before continuing.
    #[test]
    fn the_protected_value_survives_an_allocating_cleanup() {
        let mut h = stress_heap();
        let src = r#"(unwind-protect (str "kept") (str "the cleanup allocates"))"#;
        match eval_ok(&mut h, src) {
            Value::Str(id) => assert_eq!(h.string(id), "kept"),
            other => panic!("the protected value did not survive: {:?}", other),
        }
    }

    /// Nested `unwind-protect`s run innermost first: the trail is consed in
    /// the order the cleanups ran, so the outer one ends up at the head.
    #[test]
    fn nested_cleanups_run_innermost_first() {
        let mut h = stress_heap();
        let src = r#"(let ((trail r (unit)))
                       (catch (quote done)
                         (unwind-protect
                           (unwind-protect
                             (throw (quote done) (int-any-width 0))
                             (set trail (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (str "i") (var trail))))
                           (set trail (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (str "o") (var trail)))))
                       (var trail))"#;
        let v = eval_ok(&mut h, src);
        let head = h.car(v).unwrap();
        let second = h.car(h.cdr(v).unwrap()).unwrap();
        match (head, second) {
            (Value::Str(a), Value::Str(b)) => {
                assert_eq!(h.string(a), "o", "the outer cleanup should have run last");
                assert_eq!(h.string(b), "i", "the inner cleanup should have run first");
            }
            other => panic!("expected two strings, got {:?}", other),
        }
    }

    /// An exit raised by the cleanup itself wins over the one in flight
    /// (CLHS: the cleanup-forms are not protected by their own
    /// unwind-protect).
    #[test]
    fn an_exit_from_the_cleanup_beats_the_one_in_flight() {
        let mut h = stress_heap();
        let src = "(catch (quote outer)
                     (catch (quote inner)
                       (unwind-protect
                         (throw (quote inner) (int-any-width 1))
                         (throw (quote outer) (int-any-width 2)))))";
        assert_eq!(agrees(&mut h, src), Value::Int(2));
    }

    // ---- the vocabulary is closed ----------------------------------------

    /// Every `Op` is handled: `step_cps`'s match has no catch-all, so the
    /// compiler is the one holding this. Adding a tag to `Op` without giving
    /// it a frame is a build error, not a run-time surprise — which is what a
    /// `unimplemented!()` arm bought while the move was in progress.
    ///
    /// This test exists to say so where a reader looks for it; there is
    /// nothing to assert at run time.
    #[test]
    fn every_op_has_moved() {
        let mut h = stress_heap();
        // A form of each shape, as a smoke test that the arms are wired at all.
        assert_eq!(eval_ok(&mut h, "(int-any-width 1)"), Value::Int(1));
        assert_eq!(eval_ok(&mut h, "(if (bool true) (int-any-width 1) (int-any-width 2))"), Value::Int(1));
        assert_eq!(eval_ok(&mut h, "(let ((x r (int-any-width 3))) (var x))"), Value::Int(3));
        assert_eq!(eval_ok(&mut h, "(loop (return (int-any-width 4)))"), Value::Int(4));
        assert_eq!(eval_ok(&mut h, "(catch (quote t) (throw (quote t) (int-any-width 5)))"), Value::Int(5));
    }
}
