//! Runners that read the whole program before checking any of it.
//!
//! [`Session::eval`](super::Session::eval) reads one form at a time, in the
//! module the checker is in by then, as a driver reads a file. The runners
//! here read every form first and then check and evaluate them in order —
//! the shape many files had written out for themselves, with the choices
//! below (heap size, what is loaded, collecting at every allocation, how an
//! error is reported) differing from file to file. Each choice is kept as the
//! files made it.

use std::cell::RefCell;

use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, TopLevelForm, Value};

use super::{Load, Session};

/// The heap size of the runners whose programs start tasks, use channels or
/// otherwise allocate more than a default heap comfortably holds.
const LARGE_CELLS: usize = 1 << 18;

/// Read every form in `src`, then check and evaluate each in turn; the value
/// of the last one that produced a value wins. The error is the evaluator's,
/// with its source position.
fn eval_all_in(h: &mut Heap, chk: &mut Checker, interp: &Interp, src: &str) -> Result<Value, EvalError> {
    let vs = Reader::new().read_all(h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(h, interp, v).expect("check failed");
        if let Some(val) = interp.exec(h, tl)? {
            last = val;
        }
    }
    Ok(last)
}

impl Session {
    /// [`eval_all_in`] over this session.
    pub fn eval_all(&mut self, src: &str) -> Result<Value, EvalError> {
        eval_all_in(&mut self.heap, &mut self.checker, &self.interp, src)
    }

    /// Collect before every allocation from here on — switched on after the
    /// library has loaded, since collecting through the prelude would
    /// dominate the run without testing anything about the program.
    pub fn stressed(mut self) -> Session {
        self.heap.set_gc_stress(true);
        self
    }

    /// The three parts, for a test that drives them one form at a time with
    /// [`eval_in`].
    pub fn into_parts(self) -> (Heap, Checker, Interp) {
        (self.heap, self.checker, self.interp)
    }
}

/// [`eval_all_in`] with the failure's source position stripped.
pub fn eval_in(h: &mut Heap, chk: &mut Checker, interp: &Interp, src: &str) -> Result<Value, EvalError> {
    eval_all_in(h, chk, interp, src).map_err(EvalError::into_kind)
}

/// Run `src` on a large heap against the prelude, and the compiler island
/// too when `with_compiler`, collecting at every allocation when `stress`;
/// the heap comes back so a heap value can be read.
pub fn run_with(src: &str, with_compiler: bool, stress: bool) -> Result<(Heap, Value), EvalError> {
    let load = if with_compiler { Load::Compiler } else { Load::Prelude };
    let mut s = Session::with_capacity(load, LARGE_CELLS);
    s.heap.set_gc_stress(stress);
    let v = s.eval_all(src).map_err(EvalError::into_kind)?;
    Ok((s.heap, v))
}

/// [`run_with`] against the prelude alone.
pub fn run_whole(src: &str) -> Result<(Heap, Value), EvalError> {
    run_with(src, false, false)
}

fn text_with(src: &str, with_compiler: bool, stress: bool) -> String {
    match run_with(src, with_compiler, stress).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

/// The string a program answers with ([`run_whole`]).
pub fn text(src: &str) -> String {
    text_with(src, false, false)
}

/// [`text`] with the collector running at every allocation.
pub fn text_stressed(src: &str) -> String {
    text_with(src, false, true)
}

/// [`text`] for a source that compiles something first.
pub fn text_compiled(src: &str) -> String {
    text_with(src, true, false)
}

/// The string a whole program on a large heap answers with, its evaluation
/// required to succeed.
pub fn program_text(src: &str) -> String {
    let mut s = Session::with_capacity(Load::Prelude, LARGE_CELLS);
    let v = s.eval_all(src).expect("eval failed");
    s.text(v)
}

/// `expr` rendered with `~a` ([`program_text`]).
pub fn show(expr: &str) -> String {
    program_text(&format!("(format false \"~a\" {})", expr))
}

/// Run `src` with `load`, every failure — reading, checking, evaluating —
/// reported as its `Debug` text.
pub fn run_debug_err(load: Load, src: &str) -> Result<Value, String> {
    let mut s = Session::new(load);
    let vs = Reader::new().read_all(&mut s.heap, src).map_err(|e| format!("{:?}", e))?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = s.checker.check_form(&mut s.heap, &s.interp, v).map_err(|e| format!("{:?}", e))?;
        if let Some(val) = s.interp.exec(&mut s.heap, tl).map_err(|e| format!("{:?}", e))? {
            last = val;
        }
    }
    Ok(last)
}

/// Run `src` on a large heap with the compiler island loaded, every failure
/// reported as its display text; the heap comes back with the value.
pub fn run_compiled_display_err(src: &str) -> Result<(Heap, Value), String> {
    let mut s = Session::with_capacity(Load::Compiler, LARGE_CELLS);
    let vs = Reader::new().read_all(&mut s.heap, src).map_err(|e| e.to_string())?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = s.checker.check_form(&mut s.heap, &s.interp, v).map_err(|e| e.to_string())?;
        match s.interp.exec(&mut s.heap, tl) {
            Ok(Some(val)) => last = val,
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok((s.heap, last))
}

/// Run `src` with the compiler island loaded, its evaluation required to
/// succeed; the last value comes back with the heap it lives in.
pub fn eval_compiled_with_heap(src: &str) -> (Heap, Value) {
    let mut s = Session::new(Load::Compiler);
    let v = s.eval_all(src).expect("eval failed");
    (s.heap, v)
}

/// Run `src` against the bare builtins, with the registry's names published
/// but no prelude.
pub fn run_bare(src: &str) -> Result<Value, EvalError> {
    let mut s = Session::new(Load::Bare);
    s.checker.publish_names(&s.heap);
    s.eval_all(src).map_err(EvalError::into_kind)
}

/// Runs `src` with the prelude loaded and **a collection before every
/// allocation** (`gc_stress`), returning the heap alongside the value so a
/// `Sexpr` result can be read out of it.
///
/// One heap, stressed, rather than checking against a roomy heap and
/// executing against a small one: the checker lowers code into cons cells,
/// so the checked program *is* cells in the heap it was checked against, and
/// handing it to a second heap reads those cells through the wrong
/// symbol/string tables. Stressing one heap is also the stronger pressure — a
/// collection before *every* allocation, not only before the ones a small
/// arena happens to block on.
pub fn run_with_prelude_under_gc_stress(src: &str) -> Result<(Value, Heap), EvalError> {
    let mut s = Session::new(Load::Prelude).stressed();
    let v = s.eval_all(src)?;
    Ok((v, s.heap))
}

/// Check every form of `src` with `load`, stopping at the first error;
/// otherwise what the last one lowered to.
pub fn check_last(load: Load, src: &str) -> Result<TopLevelForm, Error> {
    let mut s = Session::new(load);
    let vs = Reader::new().read_all(&mut s.heap, src).expect("read failed");
    let mut last = None;
    for v in vs {
        last = Some(s.checker.check_form(&mut s.heap, &s.interp, v)?);
    }
    Ok(last.expect("no forms"))
}

/// The checker's complaint about a program that must not type-check, on a
/// large heap; the forms before the failing one are evaluated and must
/// succeed.
pub fn check_err_evaluating(src: &str) -> String {
    let mut s = Session::with_capacity(Load::Prelude, LARGE_CELLS);
    let vs = Reader::new().read_all(&mut s.heap, src).expect("read failed");
    for v in vs {
        match s.checker.check_form(&mut s.heap, &s.interp, v) {
            Ok(tl) => {
                s.interp.exec(&mut s.heap, tl).expect("eval failed");
            }
            Err(e) => return e.to_string(),
        }
    }
    panic!("expected a type error, but the program checked");
}

/// The checker's complaint about `src`, on a large heap and without its
/// source position; the forms before the failing one are evaluated, and
/// whether they succeed does not matter.
pub fn check_err_unlocated(src: &str) -> String {
    let mut s = Session::with_capacity(Load::Prelude, LARGE_CELLS);
    let vs = Reader::new().read_all(&mut s.heap, src).expect("read failed");
    for v in vs {
        match s.checker.check_form(&mut s.heap, &s.interp, v) {
            Ok(tl) => {
                let _ = s.interp.exec(&mut s.heap, tl);
            }
            Err(Error::At(_, inner)) => return inner.to_string(),
            Err(other) => return other.to_string(),
        }
    }
    panic!("expected a check error")
}

/// The `TypeError` message the first failing form produces, with the
/// compiler island loaded; the forms before it are evaluated and must
/// succeed.
pub fn check_error(src: &str) -> String {
    let mut s = Session::new(Load::Compiler);
    let vs = Reader::new().read_all(&mut s.heap, src).expect("read failed");
    for v in vs {
        match s.checker.check_form(&mut s.heap, &s.interp, v) {
            Ok(tl) => {
                s.interp.exec(&mut s.heap, tl).expect("eval failed");
            }
            Err(e) => {
                return match e.kind() {
                    Error::TypeError(m) => m.clone(),
                    other => panic!("expected a type error, got {:?}", other),
                }
            }
        }
    }
    panic!("expected `{}` to fail checking", src)
}

/// Assert that checking `src` against the bare builtins fails somewhere.
pub fn type_error(src: &str) {
    let mut s = Session::with_capacity(Load::Bare, 8192);
    let vs = Reader::new().read_all(&mut s.heap, src).expect("read failed");
    let mut result: Result<_, Error> = Ok(());
    for v in vs {
        if let Err(e) = s.checker.check_form(&mut s.heap, &s.interp, v) {
            result = Err(e);
            break;
        }
    }
    assert!(result.is_err(), "expected a type error");
}

/// [`eval_in`] in the compiler island's own module, where its functions are.
pub fn eval_in_island(h: &mut Heap, chk: &mut Checker, interp: &Interp, src: &str) -> Result<Value, EvalError> {
    chk.enter_file_module(&typelisp::compiler::island_module_segments()).expect("the island's module");
    let vs: Vec<Value> = Reader::new()
        .read_all_in_spanned_within(h, "<input>", src, typelisp::NsId::CORE)
        .expect("read failed")
        .into_iter()
        .map(|(v, _)| v)
        .collect();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(h, interp, v).expect("check failed");
        if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    chk.exit_file_module();
    Ok(last)
}

thread_local! {
    /// One prelude-loaded session per test thread, reused by every
    /// [`run_shared`] on it so the prelude loads once.
    static SHARED: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// [`eval_in`] against this thread's shared session.
pub fn run_shared(src: &str) -> Result<Value, EvalError> {
    SHARED.with(|cell| {
        let mut opt = cell.borrow_mut();
        let s = opt.get_or_insert_with(|| Session::new(Load::Prelude));
        eval_in(&mut s.heap, &mut s.checker, &s.interp, src)
    })
}

/// `f` over the heap of this thread's shared session — the heap a value
/// [`run_shared`] answered with lives in.
pub fn with_shared_heap<R>(f: impl FnOnce(&Heap) -> R) -> R {
    SHARED.with(|cell| {
        let opt = cell.borrow();
        let s = opt.as_ref().expect("no evaluation has run yet");
        f(&s.heap)
    })
}

/// Render a heap-backed `Sexpr` value in reader syntax, for exact-content
/// assertions.
pub fn sexpr_to_string(heap: &Heap, v: Value) -> String {
    match v {
        Value::Empty => "()".to_string(),
        Value::Int(i) => i.to_string(),
        Value::Boxed(id) => heap.f64_value(id).to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => format!("#\\{}", c),
        Value::Symbol(id) => heap.symbol_spelling(id).to_string(),
        Value::Str(id) => format!("{:?}", heap.string(id)),
        Value::Path(_) => "<path>".to_string(),
        Value::Cons(_) => {
            let mut parts = Vec::new();
            let mut cur = v;
            loop {
                match cur {
                    Value::Cons(_) => {
                        parts.push(sexpr_to_string(heap, heap.car(cur).unwrap()));
                        cur = heap.cdr(cur).unwrap();
                    }
                    Value::Empty => return format!("({})", parts.join(" ")),
                    other => return format!("({} . {})", parts.join(" "), sexpr_to_string(heap, other)),
                }
            }
        }
    }
}
