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
        }
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
        let mut frame = Value::Empty;
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
            s.push_root(v);
            // The cell allocation cannot itself collect (only `cons` does),
            // but the two conses below can, so each intermediate is rooted
            // before the next allocation rather than after. `frame` is rooted
            // by the previous iteration (and is `Empty`, which needs no root,
            // on the first).
            let cell = s.alloc_cell_unregistered(v);
            s.push_root(cell);
            let pair = s.cons(Value::Symbol(sym), cell).map_err(heap_err)?;
            s.push_root(pair);
            frame = s.cons(pair, frame).map_err(heap_err)?;
            s.push_root(frame);
        }
        // `env` is rooted by the caller and `frame` by the loop above, so the
        // cons that joins them is safe; the result is handed straight back to
        // the trampoline, which roots it before anything else can allocate.
        s.cons(frame, env).map_err(heap_err)
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
    #[test]
    fn a_tag_with_no_evaluation_names_itself() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(loop (break))").unwrap_err();
        assert!(
            matches!(e.kind(), EvalError::Internal(m) if m.contains("`loop`")),
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

    #[test]
    fn a_form_that_is_not_a_node_says_so() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(1 2 3)").unwrap_err();
        assert!(matches!(e.kind(), EvalError::Internal(m) if m.contains("not a core form")), "{:?}", e);
    }
}
