//! The same failing expression, run both ways, must fail the same way.
//!
//! A zero divisor, an index past the end of a vector or a string, a
//! non-positive `random` bound: these are failures the *language* defines, and
//! the interpreter has always reported them as a recoverable
//! `EvalError::Panic`. Compiled code used to reach `typelisp_rt::fatal()` for
//! every one of them and abort the process instead — so the same program said
//! `panic: divide by zero` interpreted and died with SIGABRT compiled, and an
//! `unwind-protect` around it never got to run its cleanup.
//!
//! The gap always showed up as *the two paths disagreeing*, so that is the
//! shape of the guard: [`CASES`] holds one source per failure, each run twice —
//! once interpreted, once with `(compile f)` in front of it — and asserts the
//! two produce the identical message. A new `fatal()` on a user-reachable path
//! fails here as an abort of this test binary rather than as a wrong answer,
//! which is exactly how it presents in real use.
//!
//! The line the messages sit on is SBCL's: a condition for what the program
//! did, `lose()` only for a broken runtime. `fatal()` keeps the second half —
//! wrong argument counts, wrong tags, a corrupted root stack — and nothing
//! here should ever reach it.

extern crate typelisp;

use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

/// One heap/checker/interpreter, kept alive across several evaluations — a
/// session, in the sense a REPL is one.
///
/// A failure has to leave this usable, which is half of what these tests are
/// about: an aborting process has no "afterwards" in which to check that a
/// cleanup ran or that the next form still evaluates.
struct Session {
    heap: Heap,
    checker: Checker,
    interp: Interp,
    reader: Reader,
}

impl Session {
    /// A session with the prelude *and* the compiler loaded.
    ///
    /// The compiler is loaded for the interpreted direction too, on purpose:
    /// then the only difference between an interpreted and a compiled run of a
    /// case is the `(compile f)` form itself, not what else is in scope.
    fn new() -> Self {
        let mut heap = Heap::with_capacity(1 << 16);
        let mut checker = Checker::new();
        let mut interp = Interp::new();
        load_prelude(&mut heap, &mut checker, &mut interp);
        load_compiler(&mut heap, &mut checker, &mut interp);
        Session { heap, checker, interp, reader: Reader::new() }
    }

    /// Evaluates every form in `src`, returning the last value or the error
    /// that stopped it.
    fn eval(&mut self, src: &str) -> Result<Value, EvalError> {
        let vs = self.reader.read_all(&mut self.heap, src).expect("read failed");
        let mut last = Value::Empty;
        for v in vs {
            let tl = self.checker.check_form(&mut self.heap, &self.interp, v).expect("check failed");
            if let Some(val) = self.interp.exec(&mut self.heap, tl).map_err(EvalError::into_kind)? {
                last = val;
            }
        }
        Ok(last)
    }

    /// [`Self::eval`] for a form expected to succeed.
    fn ok(&mut self, src: &str) -> Value {
        self.eval(src).unwrap_or_else(|e| panic!("expected success, got {:?}\nsource:\n{}", e, src))
    }

    /// The `EvalError::Panic` message `src` fails with — or a description of
    /// whatever else happened, so a mismatch names it.
    fn panics(&mut self, src: &str) -> String {
        match self.eval(src) {
            Err(EvalError::Panic(msg)) => msg,
            Err(other) => panic!("expected a Panic, got {:?}\nsource:\n{}", other, src),
            Ok(v) => panic!("expected a Panic, but it returned {:?}\nsource:\n{}", v, src),
        }
    }
}

/// [`Session::panics`] for a one-shot program, with nothing to ask afterwards.
fn panic_message(src: &str) -> String {
    Session::new().panics(src)
}

/// One failure, as (definitions, call) — the definitions always define `f`,
/// which the compiled run compiles before the call.
struct Case {
    /// What is being provoked, for the assertion message.
    what: &'static str,
    /// Top-level forms, ending with `f`'s definition.
    defs: &'static str,
    /// The call that fails. Kept separate from `defs` so `(compile f)` can be
    /// slipped in between the two.
    call: &'static str,
    /// The exact message both paths must produce.
    message: &'static str,
}

/// The failures a program can provoke on either path.
///
/// Every operand that has to be zero/out-of-range arrives as an *argument*
/// rather than a literal in the failing form, so the check is unmistakably a
/// run-time one on both sides.
const CASES: &[Case] = &[
    Case {
        what: "int division by zero",
        defs: "(defun f ((a int) (b int)) int (/ a b))",
        call: "(f 5 0)",
        message: "divide by zero",
    },
    Case {
        what: "int mod by zero",
        defs: "(defun f ((a int) (b int)) int (mod a b))",
        call: "(f 5 0)",
        message: "mod by zero",
    },
    Case {
        // The other case integer division has no answer for: the most
        // negative value over `-1`, whose quotient is one past the type's
        // maximum. An `int` would widen to a bignum, but the expression's
        // declared type is a fixed width, so it fails instead. Written as a
        // literal directly — `i32::MIN` is a number an `i32` holds, so the
        // reader and the literal range check both accept it.
        what: "i32 division overflowing",
        defs: "(defun f ((a i32) (b i32)) i32 (/ a b))",
        call: "(f -2147483648 -1)",
        message: "arithmetic overflow: -2147483648 / -1",
    },
    Case {
        // `rem` is a prelude method built out of `/`, so this is the
        // divide-by-zero above reached through a compiled prelude body.
        what: "int rem by zero",
        defs: "(defun f ((a int) (b int)) int (rem a b))",
        call: "(f 5 0)",
        message: "divide by zero",
    },
    Case {
        what: "bignum division by zero",
        defs: "(defun f ((a int) (b int)) int (/ a b))",
        call: "(f 100000000000000000000 0)",
        message: "divide by zero",
    },
    Case {
        what: "bignum mod by zero",
        defs: "(defun f ((a int) (b int)) int (mod a b))",
        call: "(f 100000000000000000000 0)",
        message: "mod by zero",
    },
    Case {
        what: "ratio division by zero",
        defs: "(defun f ((a ratio) (b ratio)) ratio (/ a b))",
        call: "(f 1/2 (int->ratio 0))",
        message: "divide by zero",
    },
    Case {
        what: "string index past the end",
        defs: r#"(defun f ((s string) (i int)) char (ref s i))"#,
        call: r#"(f "hi" 5)"#,
        message: "ref: index 5 out of range (length 2)",
    },
    Case {
        what: "negative string index",
        defs: r#"(defun f ((s string) (i int)) char (ref s i))"#,
        call: r#"(f "hi" -1)"#,
        message: "ref: index -1 out of range (length 2)",
    },
    Case {
        what: "substring range past the end",
        defs: r#"(defun f ((s string) (a int) (b int)) string (substring s a b))"#,
        call: r#"(f "hi" 0 5)"#,
        message: "substring: invalid range 0..5 (length 2)",
    },
    Case {
        what: "substring with start after end",
        defs: r#"(defun f ((s string) (a int) (b int)) string (substring s a b))"#,
        call: r#"(f "hello" 4 1)"#,
        message: "substring: invalid range 4..1 (length 5)",
    },
    Case {
        what: "vector index past the end",
        defs: "(defun make-v () Vector<int> (Vector::new))
               (defun f ((i int)) int (let ((v (make-v))) (push v 1) (get v i)))",
        call: "(f 5)",
        message: "Vector: index 5 out of bounds",
    },
    Case {
        what: "negative vector index",
        defs: "(defun make-v () Vector<int> (Vector::new))
               (defun f ((i int)) int (let ((v (make-v))) (push v 1) (get v i)))",
        call: "(f -1)",
        message: "Vector: index -1 out of bounds",
    },
    Case {
        what: "vector index past the end, on a store",
        defs: "(defun make-v () Vector<int> (Vector::new))
               (defun f ((i int)) int (let ((v (make-v))) (push v 1) (set v i 9) 0))",
        call: "(f 5)",
        message: "Vector: index 5 out of bounds",
    },
    Case {
        // `random`'s bound check lives in `rt_random_state_next`, which the
        // prelude's `random` reaches on both paths — the interpreted run of
        // `f` still calls a *compiled* prelude body.
        what: "a non-positive random bound",
        defs: "(defun f ((n int)) int (random n))",
        call: "(f 0)",
        message: "random: bound must be positive, got 0",
    },
    Case {
        what: "a negative random bound",
        defs: "(defun f ((n int)) int (random n))",
        call: "(f -5)",
        message: "random: bound must be positive, got -5",
    },
];

/// Interpreted: `f` is never compiled, so the failure is raised by the
/// evaluator (or, for `random`/`rem`, by a compiled *prelude* body called from
/// it — which is why those two are in this table at all).
#[test]
fn every_runtime_failure_reports_the_same_message_interpreted() {
    for case in CASES {
        let src = format!("{}\n{}", case.defs, case.call);
        assert_eq!(panic_message(&src), case.message, "interpreted: {}", case.what);
    }
}

/// Compiled: the same source with `(compile f)` in front of the call, so the
/// failing operation is the `rt_*` shim's own check. Before this worked, each
/// of these aborted the process.
#[test]
fn every_runtime_failure_reports_the_same_message_compiled() {
    for case in CASES {
        let src = format!("{}\n(compile f)\n{}", case.defs, case.call);
        assert_eq!(panic_message(&src), case.message, "compiled: {}", case.what);
    }
}

// ---- what recovering actually buys ---------------------------------------
//
// The parity above is the guard; these are the behaviors it exists for. An
// aborting process cannot run a cleanup, cannot be unwound through, and leaves
// no session behind to ask — so each of those is asserted directly, on the
// compiled path where none of them used to hold.

/// A compiled `unwind-protect` runs its cleanup when the body dies of a zero
/// divisor, and the failure still arrives afterwards.
///
/// This is the case the whole change is for. The cleanup's effect is read back
/// *after* the failure, from the same session — which is only a question one
/// can ask because the process is still there to ask it.
#[test]
fn a_compiled_cleanup_runs_when_a_zero_divisor_unwinds_through_it() {
    let mut s = Session::new();
    s.ok(r#"
        (defvar (ran int) 0)
        (defun f ((b int)) int (unwind-protect (/ 5 b) (setf ran 1)))
        (compile f)
        "#);
    assert_eq!(s.panics("(f 0)"), "divide by zero");
    assert_eq!(s.ok("ran"), Value::Int(1), "the cleanup did not run");
}

/// Nested compiled cleanups all run on the way out of an out-of-bounds vector
/// index, innermost first.
///
/// `trace` records the order as decimal digits (inner appends `1`, outer `2`),
/// so `12` means both ran and in that order.
#[test]
fn compiled_cleanups_run_in_order_when_a_bad_index_unwinds_through_them() {
    let mut s = Session::new();
    s.ok(r#"
        (defvar (trace int) 0)
        (defun make-v () Vector<int> (Vector::new))
        (defun f ((i int)) int
          (unwind-protect
            (unwind-protect (get (make-v) i) (setf trace (+ (* trace 10) 1)))
            (setf trace (+ (* trace 10) 2))))
        (compile f)
        "#);
    assert_eq!(s.panics("(f 0)"), "Vector: index 0 out of bounds");
    assert_eq!(s.ok("trace"), Value::Int(12), "the cleanups did not both run, innermost first");
}

/// A `catch` does not claim a runtime failure — it claims throws carrying its
/// tag, and this is neither — but a cleanup inside it still runs and the
/// failure keeps travelling.
#[test]
fn a_compiled_catch_does_not_swallow_a_runtime_failure() {
    let mut s = Session::new();
    s.ok(r#"
        (defvar (ran int) 0)
        (defun f ((b int)) int
          (catch 'done (unwind-protect (/ 5 b) (setf ran 1))))
        (compile f)
        "#);
    assert_eq!(s.panics("(f 0)"), "divide by zero");
    assert_eq!(s.ok("ran"), Value::Int(1), "the cleanup inside the catch did not run");
}

/// The failure survives being raised inside a compiled callee and unwinding
/// through a compiled caller — the frames in between are LLVM's, not Rust's.
#[test]
fn a_runtime_failure_unwinds_through_intermediate_compiled_frames() {
    let src = r#"
        (defun deep ((b int)) int (/ 5 b))
        (defun middle ((b int)) int (+ 1 (deep b)))
        (defun f ((b int)) int (middle b))
        (compile f)
        (f 0)
        "#;
    assert_eq!(panic_message(src), "divide by zero");
}

/// And the session goes on afterwards. A REPL that died on its user's first
/// `(rem 5 0)` was the visible cost of the old behavior; here the same heap,
/// checker and interpreter keep answering — including after failing twice,
/// which is where a mishandled unwind shows up (see `compiled_unwind_test`'s
/// engine-lifetime probe).
#[test]
fn a_session_survives_a_compiled_runtime_failure_and_keeps_evaluating() {
    let mut s = Session::new();
    s.ok("(defun f ((a int) (b int)) int (/ a b)) (compile f)");
    assert_eq!(s.panics("(f 5 0)"), "divide by zero");
    // The same compiled function, this time with a divisor it can use.
    assert_eq!(s.ok("(f 10 2)"), Value::Int(5), "the session did not survive");
    assert_eq!(s.panics("(f 1 0)"), "divide by zero");
    assert_eq!(s.ok("(f 9 3)"), Value::Int(3), "the session did not survive the second failure");
}
