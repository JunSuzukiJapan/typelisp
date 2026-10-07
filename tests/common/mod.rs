//! The shared test harness: read, check and evaluate a program.
//!
//! Every integration test needs the same four lines — a heap, a checker, an
//! interpreter, and whatever library the program under test expects to be
//! there — followed by the same read/check/exec loop. Written out per file
//! that was ~15 lines copied into 60 of them, and a new argument to
//! `Checker::check_form` meant editing all 60.
//!
//! # What a test still chooses
//!
//! The copies were not identical, and the differences are load-bearing rather
//! than accidental, so they are parameters here rather than something this
//! module picks:
//!
//! - **What is loaded** ([`Load`]). A `lambda`/`labels`/`FnRef` value
//!   JIT-compiles through the compiler island when the island is there, so
//!   whether a closure test runs interpreted or compiled is decided by this.
//! - **The heap's size** ([`Session::with_capacity`]). A GC test that means to
//!   force a collection is choosing a capacity, not accepting a default.
//! - **Whether an error keeps its source position** ([`Session::eval`] versus
//!   [`Session::eval_kind`]). A test that asserts on an error's text sees the
//!   difference.
//!
//! # Why the session is handed back
//!
//! A `string` result is a `Value::Str` — an index into the heap it was built
//! in — so reading its text needs that heap alive. The per-file helpers
//! dropped theirs on return, which is why so many files carried a second,
//! near-identical `eval_string` runner beside `run`. Here the caller keeps the
//! [`Session`] and reads `session.text(v)`.

// Each test binary gets its own copy of this module and uses a different part
// of it, so anything another file uses is dead code in this one. That is a
// property of how Cargo builds `tests/`, not of the code.
#![allow(dead_code)]

use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, TopLevelForm, Value};

/// The default heap for a test: large enough for the prelude and the island
/// with room to run in, which is what all but a handful of the per-file
/// helpers used.
const DEFAULT_CELLS: usize = 1 << 16;

/// What a [`Session`] loads before the program under test.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Load {
    /// Nothing — the checker and evaluator against the bare builtins. For a
    /// test about a primitive, where the prelude would only be noise.
    Bare,
    /// `load_prelude`.
    Prelude,
    /// `load_prelude`, then the native compiler island. Closures and generic
    /// values reify through the island when it is loaded, so this is the
    /// configuration a test about compiled behavior wants.
    Compiler,
}

/// A heap, a checker and an interpreter that belong together.
///
/// Public fields: a test that needs to reach past [`Self::eval`] — to set a
/// redefinition policy, to read a global back, to force a collection — does it
/// directly rather than through an accessor per use.
pub struct Session {
    pub heap: Heap,
    pub checker: Checker,
    pub interp: Interp,
}

impl Session {
    /// A session over a [`DEFAULT_CELLS`]-cell heap.
    pub fn new(load: Load) -> Session {
        Session::with_capacity(load, DEFAULT_CELLS)
    }

    /// A session over a heap of exactly `cells` cells.
    ///
    /// The library is loaded *before* the program is read, which also keeps
    /// the program's own values off the root stack while the island loads.
    pub fn with_capacity(load: Load, cells: usize) -> Session {
        let mut heap = Heap::with_capacity(cells);
        let mut checker = Checker::new();
        let mut interp = Interp::new();
        if load != Load::Bare {
            load_prelude(&mut heap, &mut checker, &mut interp);
        }
        if load == Load::Compiler {
            load_compiler(&mut heap, &mut checker, &mut interp);
        }
        Session { heap, checker, interp }
    }

    /// Read, check and evaluate every form in `src`; the value of the last one
    /// that produced a value wins.
    ///
    /// Reading and checking are `expect`s, not errors: a test whose source
    /// does not parse or does not type-check is a broken test, and saying so
    /// at the point it happens beats threading it into the result every caller
    /// then has to discriminate. [`check_one`](Self::check_one) is for a test
    /// that means to observe a *checker* error.
    pub fn eval(&mut self, src: &str) -> Result<Value, EvalError> {
        let r = Reader::new();
        let vs = r.read_all(&mut self.heap, src).expect("read failed");
        let mut last = Value::Empty;
        for v in vs {
            let tl = self.checker.check_form(&mut self.heap, &self.interp, v).expect("check failed");
            if let Some(val) = self.interp.exec(&mut self.heap, tl)? {
                last = val;
            }
        }
        Ok(last)
    }

    /// [`Self::eval`] with the failure's source position stripped
    /// (`EvalError::into_kind`) — for a test that asserts on the error's own
    /// text rather than on where it was raised.
    pub fn eval_kind(&mut self, src: &str) -> Result<Value, EvalError> {
        self.eval(src).map_err(EvalError::into_kind)
    }

    /// Check one form and hand back what it lowered to, without evaluating —
    /// for a test about the checker, where the error *is* the subject.
    ///
    /// `src` must hold exactly one form.
    pub fn check_one(&mut self, src: &str) -> Result<TopLevelForm, Error> {
        let r = Reader::new();
        let mut vs = r.read_all(&mut self.heap, src).expect("read failed");
        assert_eq!(vs.len(), 1, "expected exactly one form in {:?}", src);
        let v = vs.pop().unwrap();
        self.checker.check_form(&mut self.heap, &self.interp, v).map_err(Error::into_kind)
    }

    /// Check every form in `src`, stopping at the first error — for a test
    /// whose subject is a definition several forms in.
    pub fn check_all(&mut self, src: &str) -> Result<(), Error> {
        let r = Reader::new();
        let vs = r.read_all(&mut self.heap, src).expect("read failed");
        for v in vs {
            self.checker.check_form(&mut self.heap, &self.interp, v).map_err(Error::into_kind)?;
        }
        Ok(())
    }

    /// The text of a `string` result, read out of this session's heap.
    pub fn text(&self, v: Value) -> String {
        match v {
            Value::Str(id) => self.heap.string(id).to_string(),
            other => panic!("expected a string, got {:?}", other),
        }
    }
}

/// Evaluate `src` against the prelude. Errors carry no source position — see
/// [`Session::eval_kind`].
pub fn run(src: &str) -> Result<Value, EvalError> {
    Session::new(Load::Prelude).eval_kind(src)
}

/// [`run`] with the compiler island loaded too.
pub fn run_compiled(src: &str) -> Result<Value, EvalError> {
    Session::new(Load::Compiler).eval_kind(src)
}

/// [`run`], panicking on failure — the shape most assertions want.
pub fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// [`run_compiled`], panicking on failure.
pub fn eval_ok_compiled(src: &str) -> Value {
    run_compiled(src).expect("eval failed")
}

/// The `Debug` text of the runtime error `src` is expected to raise.
pub fn eval_err(src: &str) -> String {
    match run(src) {
        Err(e) => format!("{:?}", e),
        Ok(v) => panic!("expected a runtime error, got {:?}", v),
    }
}

/// `src`'s value as text, with the session kept alive long enough to read it.
pub fn eval_string(src: &str) -> String {
    let mut s = Session::new(Load::Prelude);
    let v = s.eval(src).expect("eval failed");
    s.text(v)
}

/// [`eval_string`] with the compiler island loaded too.
pub fn eval_string_compiled(src: &str) -> String {
    let mut s = Session::new(Load::Compiler);
    let v = s.eval(src).expect("eval failed");
    s.text(v)
}

/// The message of the `TypeError` that checking `src` against the prelude is
/// expected to fail with.
pub fn check_err(src: &str) -> String {
    match Session::new(Load::Prelude).check_all(src) {
        Err(Error::TypeError(msg)) => msg,
        other => panic!("expected a TypeError, got {:?}", other),
    }
}

/// Assert that `src` evaluates to `true`, reporting the source when it does
/// not.
pub fn is_true(src: &str) {
    assert_eq!(eval_ok(src), Value::Bool(true), "{}", src);
}

/// The repository root, for a test that reads a committed file back — a guard
/// test comparing a table against the source it is supposed to mirror, or the
/// editor files that have to stay in step with the checker.
pub fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
