//! `trace` / `untrace` (CLHS 25.2): watching a named definition being called.
//!
//! The hook is one branch in `Interp::enter`, the single point every call to
//! a *named* function goes through — an ordinary call, a method call, a
//! `:dyn` dispatch, a macro expansion, the printer's call to a `print-object`
//! method and `format`'s `~/name/` all arrive there, and they arrive before
//! the compiled/interpreted split, which is why a definition with a compiled
//! body is still traced when an interpreted caller reaches it.
//!
//! The last two arrived below it until 2026-09-12: they called `Interp::apply`,
//! which is the interpreted half of that split, so tracing them was inert and
//! compiling them had no effect. The tests at the end of this file are what
//! hold that.
//!
//! Trace output goes to `*trace-output*`, as CL specifies. That global holds
//! a `standard-stream` — a struct around a native handle — so a test can
//! point it at a *string* output stream and read the transcript back, which
//! is what [`traced`] below does. Nothing here has to scrape stdout.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Error, Heap, Interp, Reader, Value};

/// Runs `src` with `*trace-output*` pointed at a string stream, and returns
/// what was written there.
///
/// The handle is kept in a global rather than a `let`, so that the redirect
/// and the read-back can be separate top-level forms with the program's own
/// forms in between.
fn traced(src: &str) -> String {
    let full = format!(
        "(defvar (*cap* i32) (stream-string-output))\n\
         (setf *trace-output* (standard-stream::new *cap*))\n\
         {}\n\
         (match (stream-take-output-string *cap*) ((ok s) s) ((err _) \"<capture failed>\"))",
        src
    );
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, &full).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected the captured transcript, got {:?}", other),
    }
}

/// Runs `src` and returns its last value as text — the `string` a
/// `(format false ...)` produced.
fn eval_string(src: &str) -> String {
    let (h, v) = eval(src);
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// Runs `src` and returns its last value, with no trace redirection.
fn eval(src: &str) -> (Heap, Value) {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    (h, last)
}

/// The `TypeError` message the first failing form produces.
fn check_error(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                interp.exec(&mut h, tl).expect("eval failed");
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

// ---- the report itself ---------------------------------------------------

/// The shape CL's own trace has: a depth number, two spaces of indent per
/// open frame, the call going in and the value coming out.
#[test]
fn a_recursive_call_is_reported_at_every_depth() {
    let out = traced(
        "(defun fact ((n i32)) i32 (if (< n 2) 1 (* n (fact (- n 1)))))\n\
         (trace fact)\n\
         (fact 3)",
    );
    assert_eq!(
        out,
        "  0: (fact 3)\n    \
           1: (fact 2)\n      \
             2: (fact 1)\n      \
             2: fact returned 1\n    \
           1: fact returned 2\n  \
         0: fact returned 6\n"
    );
}

/// Arguments and results are rendered by the *program's* printer, not by a
/// second one written for tracing — so a struct comes out the way `~s` would
/// print it, and a type with a `print-object` method comes out by its own.
#[test]
fn values_are_printed_the_way_the_program_prints_them() {
    let out = traced(
        "(defstruct pt (pub x i32) (pub y i32))\n\
         (defmethod total ((self pt)) i32 (+ self::x self::y))\n\
         (trace pt::total)\n\
         (total (pt::new 3 4))",
    );
    assert_eq!(out, "  0: (pt::total #<pt 3 4>)\n  0: pt::total returned 7\n");
}

/// A `throw` past a traced frame is reported rather than silently leaving a
/// call with no matching return line — that is the moment a trace is most
/// worth having.
#[test]
fn a_non_local_exit_out_of_a_traced_frame_is_reported() {
    let out = traced(
        "(defun boom ((n i32)) i32 (throw 'tag n))\n\
         (trace boom)\n\
         (catch 'tag (boom 7))",
    );
    assert!(out.starts_with("  0: (boom 7)\n"), "the call line is missing: {:?}", out);
    assert!(out.contains("0: boom exited non-locally:"), "the exit is not reported: {:?}", out);
}

// ---- the table -----------------------------------------------------------

/// Both forms answer with the names traced *afterwards*, which is what makes
/// `(trace)` with no arguments CL's "tell me what is traced".
#[test]
fn both_forms_answer_with_the_current_set() {
    // Sorted, so the answer does not depend on a hash table's iteration order.
    assert_eq!(
        eval_string(
            "(defun f ((n i32)) i32 n)\n\
             (defun g ((n i32)) i32 n)\n\
             (trace g f)\n\
             (format false \"~s\" (trace))"
        ),
        "(f g)"
    );
    assert_eq!(
        eval_string(
            "(defun f ((n i32)) i32 n)\n\
             (defun g ((n i32)) i32 n)\n\
             (trace f g)\n\
             (untrace g)\n\
             (format false \"~s\" (trace))"
        ),
        "(f)"
    );
    assert_eq!(
        eval_string(
            "(defun f ((n i32)) i32 n)\n\
             (trace f)\n\
             (format false \"~s\" (untrace))"
        ),
        "()"
    );
}

/// `untrace` really stops the reporting.
#[test]
fn untrace_stops_the_reporting() {
    let out = traced(
        "(defun f ((n i32)) i32 n)\n\
         (trace f)\n\
         (f 1)\n\
         (untrace f)\n\
         (f 2)",
    );
    assert_eq!(out, "  0: (f 1)\n  0: f returned 1\n");
}

/// The table is keyed by *name*, on the interpreter, not by a flag on the
/// `FnDef` — so redefining a traced function keeps tracing it, the way CL
/// traces a name rather than a body.
#[test]
fn tracing_survives_redefinition() {
    let out = traced(
        "(defun g ((n i32)) i32 (* n 2))\n\
         (trace g)\n\
         (g 5)\n\
         (defun g ((n i32)) i32 (* n 3))\n\
         (g 5)",
    );
    assert_eq!(
        out,
        "  0: (g 5)\n  0: g returned 10\n  0: (g 5)\n  0: g returned 15\n"
    );
}

/// A name inside a module is traced under its qualified name — the spelling
/// `Interp::method_key` parses back and the compile driver's link names use.
///
/// Written inside `(module m ...)` on purpose: a type or function whose path
/// has one segment cannot show a qualification bug, and this codebase has
/// been bitten three times by exactly that.
#[test]
fn a_module_qualified_name_is_traced_under_its_full_path() {
    let out = traced(
        "(module m (pub defun inc ((n i32)) i32 (+ n 1)))\n\
         (use m)\n\
         (trace m::inc)\n\
         (m::inc 1)",
    );
    assert_eq!(out, "  0: (m::inc 1)\n  0: m::inc returned 2\n");
}

// ---- what it refuses, and what it warns about ----------------------------

/// A generic definition has no single body for `trace` to watch — the same
/// refusal `compile` makes, in the same words, from the shared resolution.
#[test]
fn a_generic_function_cannot_be_traced() {
    let msg = check_error("(defun idg<T> ((x T)) T x)\n(trace idg)");
    assert!(msg.starts_with("trace: `idg` is generic"), "{}", msg);
    assert!(msg.contains("monomorphization runs per use"), "{}", msg);
}

/// An unresolvable name fails at check time, like every other unbound
/// reference: there is nothing left for the runtime to look up.
#[test]
fn an_unknown_name_is_a_check_time_error() {
    assert_eq!(check_error("(trace nope)"), "trace: no function `nope` is visible from here");
}

/// A string is not a second spelling of a name.
#[test]
fn a_string_is_not_a_name() {
    assert!(check_error("(trace \"f\")").starts_with("trace: expected symbols or `::`-paths"));
}

/// Tracing a definition that already has a compiled body is allowed, and says
/// what it cannot see: `enter` is above the compiled/interpreted split, so an
/// interpreted caller is still reported — but a call made from inside other
/// compiled code never reaches `enter` at all.
#[test]
fn tracing_a_compiled_body_warns_and_still_reports_interpreted_calls() {
    let out = traced(
        "(defun h ((n i32)) i32 (+ n 1))\n\
         (compile h)\n\
         (trace h)\n\
         (h 1)",
    );
    assert!(
        out.contains("note: `h` has a compiled body"),
        "the limitation should be stated when it applies: {:?}",
        out
    );
    assert!(out.contains("0: (h 1)\n"), "an interpreted caller is still reported: {:?}", out);
    assert!(out.contains("0: h returned 2\n"), "and so is its result: {:?}", out);
}

/// The head is reserved in expression position: this dispatch runs before the
/// local-variable lookup, so a `let` binding named `trace` does not shadow the
/// form.
///
/// Written down because the opposite is easy to assume — the comment beside
/// `pprint`'s arm in `check_list` claims that family *is* shadowable, and it
/// is not either. `trace` is treated exactly as `documentation` is, and this
/// is what that treatment does.
#[test]
fn the_form_head_is_reserved_against_a_local_binding() {
    let msg = check_error("(let ((trace (lambda ((n i32)) i32 (* n 2)))) (trace 21))");
    assert!(msg.starts_with("trace: expected symbols or `::`-paths"), "{}", msg);
}

// ---- step ----------------------------------------------------------------

/// `(step form)` evaluates `form` and answers with its value, so it has
/// `form`'s own type and stands wherever `form` could.
#[test]
fn step_has_the_type_of_the_form_it_wraps() {
    let (h, v) = eval("(+ 1 (step (* 3 4)))");
    assert!(matches!(v, Value::Int(13)), "expected 13, got {:?}", v);
    drop(h);
    assert_eq!(eval_string("(step (format false \"~d\" 7))"), "7");
}

/// With no terminal on standard input — a script, a pipe, this test — `step`
/// simply evaluates its form.
///
/// CLHS allows exactly that, and the alternative is a prompt nobody is there
/// to answer. The assertion is that *nothing else* changes: the same value,
/// and the same side effects in the same order.
#[test]
fn without_a_terminal_step_just_evaluates() {
    let stepped = traced(
        "(defun note ((n i32)) i32 (progn (println \"call ~d\" n) n))\n\
         (defun both () i32 (+ (note 1) (note 2)))\n\
         (trace note)\n\
         (step (both))",
    );
    let plain = traced(
        "(defun note ((n i32)) i32 (progn (println \"call ~d\" n) n))\n\
         (defun both () i32 (+ (note 1) (note 2)))\n\
         (trace note)\n\
         (both)",
    );
    assert_eq!(stepped, plain, "`step` changed what happened, with no terminal to step with");
    assert!(plain.contains("0: (note 1)"), "the trace should still be there: {:?}", plain);
}

/// One form, and it is a form rather than a name — the opposite of `trace`'s
/// argument, which is why the two do not share a checker path.
#[test]
fn step_takes_exactly_one_form() {
    assert_eq!(check_error("(step)"), "step: (step form) — expected exactly 1 form");
    assert_eq!(check_error("(step 1 2)"), "step: (step form) — expected exactly 1 form");
}

// ---- the collector -------------------------------------------------------

/// Rendering a trace line allocates — a one-element list per argument, and a
/// string per rendered value — while the arguments themselves are live and
/// the call has not run yet.
///
/// Under `gc_stress` the collector fires at every `cons`, so a missing root
/// fails here deterministically instead of once in a hundred runs. The
/// assertion is that the transcript is *identical* to the unstressed one:
/// the collector must not be observable in what a trace reports.
#[test]
fn trace_rendering_holds_its_roots_under_gc_stress() {
    fn run(stress: bool) -> String {
        let src = "(defstruct pt (pub x i32) (pub y i32))\n\
                   (defun shift ((p pt) (d i32) (s string)) i32 (+ p::x d))\n\
                   (defvar (*cap* i32) (stream-string-output))\n\
                   (setf *trace-output* (standard-stream::new *cap*))\n\
                   (trace shift)\n\
                   (shift (pt::new 1 2) 3 \"four\")\n\
                   (shift (pt::new 5 6) 7 \"eight\")\n\
                   (match (stream-take-output-string *cap*) ((ok s) s) ((err _) \"<capture failed>\"))";
        let mut h = Heap::with_capacity(1 << 16);
        let mut chk = Checker::new();
        let mut interp = Interp::new();
        load_prelude(&mut h, &mut chk, &mut interp);
        load_compiler(&mut h, &mut chk, &mut interp);
        // Stress only the *program's* own allocations: loading the prelude
        // under a collector that fires at every cons takes minutes, and what
        // is under test is the trace hook, not the loader.
        h.set_gc_stress(stress);
        let r = Reader::new();
        let vs = r.read_all(&mut h, src).expect("read failed");
        let mut last = Value::Empty;
        for v in vs {
            let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
            if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
                last = val;
            }
        }
        let text = match last {
            Value::Str(id) => h.string(id).to_string(),
            other => panic!("expected the captured transcript, got {:?}", other),
        };
        h.set_gc_stress(false);
        text
    }
    let plain = run(false);
    assert!(plain.contains("#<pt 1 2>"), "the unstressed transcript is wrong to begin with: {:?}", plain);
    assert_eq!(run(true), plain, "the collector is visible in what a trace reports");
}

// ---- the paths that used to arrive below the hook ------------------------

/// **The printer's own call to a `print-object` method is reported.**
///
/// `print_object_method` used to call `Interp::apply` directly, which is
/// *below* the hook — so `(trace pt::print-object)` was silently inert, and
/// this module's own doc comment ("every call to a named function goes
/// through `enter`") was not true of it. The same `apply` bypass meant
/// `(compile pt::print-object)` returned `true` and changed nothing about
/// what the printer ran: `enter` is where the compiled/interpreted choice is
/// made, and `apply` is the interpreted half of it.
#[test]
fn a_print_object_method_is_reported_when_the_printer_calls_it() {
    let out = traced(
        "(defstruct pt (n i32))\n\
         (impl print-object pt\n\
           (print-object ((self Self) (escape bool)) string (format false \"<~d>\" self::n)))\n\
         (trace pt::print-object)\n\
         (format false \"~a\" (pt::new 7))",
    );
    assert!(out.contains("(pt::print-object"), "the printer's call was not reported: {:?}", out);
    assert!(out.contains("pt::print-object returned"), "the return was not reported: {:?}", out);
}

/// The same for `format`'s `~/name/`, which reaches its method the same way.
#[test]
fn a_call_directives_method_is_reported() {
    let out = traced(
        "(defstruct money (yen i32))\n\
         (defmethod jp ((self money) (colon bool) (at bool)) string\n\
           (format false \"~d yen\" self::yen))\n\
         (trace money::jp)\n\
         (format false \"~/jp/\" (money::new 300))",
    );
    assert!(out.contains("(money::jp"), "the directive's call was not reported: {:?}", out);
}

/// **A compiled `print-object` body is the one the printer runs.**
///
/// `enter` has exactly one branch on `compiled` and no path that falls back
/// to `apply` while it is `Some` — so a trace line from the printer's call,
/// with the method compiled, says the compiled body is what ran. Before the
/// fix there was no trace line at all, and the interpreted body ran however
/// many times the method had been compiled.
///
/// **The assertion is on the call line, not on the name.** `(trace f)` on a
/// definition that has a compiled body writes a note naming it, so a bare
/// `contains("pt::print-object")` passed with the fix reverted — a test that
/// could not fail.
#[test]
fn a_compiled_print_object_method_is_still_reported() {
    let out = traced(
        "(defstruct pt (n i32))\n\
         (impl print-object pt\n\
           (print-object ((self Self) (escape bool)) string (format false \"<~d>\" self::n)))\n\
         (compile pt::print-object)\n\
         (trace pt::print-object)\n\
         (format false \"~a\" (pt::new 7))",
    );
    assert!(out.contains("(pt::print-object"), "the printer's call was not reported: {:?}", out);
}
