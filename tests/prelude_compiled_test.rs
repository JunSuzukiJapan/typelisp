//! What the precompiled prelude has to be true for: the bodies really are
//! installed, and installing them changes nothing you can observe except
//! speed.
//!
//! `prelude_artifacts_test.rs` guards the artifact's *freshness*; this guards
//! its *effect*. Both are needed — a perfectly fresh artifact that silently
//! installs nothing would pass that one and be worthless.

use typelisp::check::core;
use typelisp::{Checker, Heap, Interp, Reader, Value};

/// A prelude-loaded environment, with the compiled bodies (`compiled = true`)
/// or purely interpreted.
fn env(compiled: bool) -> (Heap, Checker, Interp) {
    let mut heap = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    if compiled {
        typelisp::load_prelude(&mut heap, &mut chk, &mut interp);
    } else {
        typelisp::prelude::load_interpreted(&mut heap, &mut chk, &mut interp);
    }
    typelisp::load_compiler_aot(&mut heap, &mut chk, &mut interp);
    (heap, chk, interp)
}

fn eval(heap: &mut Heap, chk: &mut Checker, interp: &Interp, src: &str) -> Value {
    let r = Reader::new();
    let forms = r.read_all(heap, src).expect("read failed");
    let mut last = Value::Empty;
    for v in forms {
        let tl = chk.check_form(heap, interp, v).expect("check failed");
        if let Some(val) = interp.exec(heap, tl).expect("eval failed") {
            last = val;
        }
    }
    last
}

/// The point of the whole artifact: after an ordinary `load_prelude`, prelude
/// definitions have native bodies without anyone calling `(compile ...)`.
///
/// Spot-checks across the families that make up the compiled set — free
/// `defun`s, the numeric method catalog on three different receiver types, and
/// an `impl` block's method — rather than a count, which would churn on every
/// prelude edit while saying less.
#[test]
fn loading_the_prelude_installs_native_bodies() {
    let (_heap, _chk, interp) = env(true);
    for name in ["not", "sexpr-map", "i32::gcd", "i32::abs", "bignum::signum", "f64::rem", "i32::equals"] {
        assert!(interp.is_compiled(name), "`{}` should have a compiled body after load_prelude", name);
    }
}

/// The stream and string/character families too, since 2026-08-14's
/// hole-closing work: these are the definitions
/// `PRELUDE_COMPILE_UNSUPPORTED` used to hold back, and they are named here
/// so that emptying that list stays a claim something checks.
#[test]
fn the_families_that_used_to_be_blocked_are_compiled_now() {
    let (_heap, _chk, interp) = env(true);
    for name in [
        // was: `stream-read-char` / `stream-write-string`
        "file-stream::read-char",
        "string-output-stream::write-string",
        // was: `symbol->string`
        "keywordp",
        // was: `char::char->string`
        "reader-whitespacep",
        // was: `random-state-next` / `make-random-state-fresh`
        "random",
        "make-random-state",
    ] {
        assert!(interp.is_compiled(name), "`{}` should have a compiled body after load_prelude", name);
    }
}

/// Macro *expanders* too: a macro body is an ordinary S-expression function
/// function — expanding it is calling it — so it compiles like any other
/// definition and the artifact carries it.
///
/// This is the half of "macros can be compiled" the implementation was missing
/// until 2026-08-26: the checker already checked every macro body under
/// all-`Sexpr` types, but `exec` registered it with no signature, and
/// `compiled_fn_body` refuses a `FnDef` without one — so `(compile <macro>)`
/// answered "has no signature (is it a defmacro?)" and every expander
/// tree-walked forever. Named across the layers that have macros: the core
/// macro layer, the prelude's loop/branch set, and one that builds its
/// expansion dynamically.
#[test]
fn macro_expanders_have_native_bodies_too() {
    let (_heap, _chk, interp) = env(true);
    for name in ["cond", "case", "when", "and", "while", "dotimes", "do", "setq"] {
        assert!(interp.is_compiled(name), "`{}`'s expander should have a compiled body after load_prelude", name);
    }
}

/// A generic definition still isn't — it has no single body to compile (the
/// checker monomorphizes per use site, so it reaches the collector as an
/// empty `(module PATH)`).
///
/// The other half of the same claim: no silent half-install, which would
/// leave a body-less declaration resolving to an address pointing at nothing.
#[test]
fn a_generic_definition_has_no_precompiled_body() {
    let (_heap, _chk, interp) = env(true);
    for name in ["pathname-name", "merge-pathnames"] {
        assert!(!interp.is_compiled(name), "`{}` is generic and cannot have one body", name);
    }
}

/// Every observable except speed is unchanged. Runs the same programs against
/// a compiled prelude and a purely interpreted one and compares the results.
///
/// The cases are chosen to *reach* compiled bodies (the numeric catalog, and
/// prelude `defun`s over `Sexpr`), and to cross the compiled/interpreted
/// boundary in both directions — `sexpr-map` is compiled but the `lambda` it
/// applies is not part of the artifact.
#[test]
fn a_compiled_prelude_computes_what_the_interpreted_one_does() {
    let cases = [
        "(gcd 462 1071)",
        "(lcm 4 6)",
        "(abs -7)",
        "(signum -3)",
        "(rem 17 5)",
        "(mod -7 3)",
        "(gcd (as bignum 462) (as bignum 1071))",
        "(abs -3.5)",
        "(not false)",
        "(sexpr-map (lambda ((x Option<Sexpr>)) Option<Sexpr> (sexpr-cons x x)) (list 1 2 3))",
        "(sexpr-append (list 1 2) (list 3 4))",
        "(equal (list 1 (list 2 3)) (list 1 (list 2 3)))",
        // The families whose lowering landed on 2026-08-14. Streams matter
        // most here: the table they address moved into `typelisp-rt`, so
        // "compiled and interpreted see the same open stream" is a claim
        // about new code, not a restatement of the old.
        "(char->string #\\a)",
        "(substring \"hello world\" 6 11)",
        r#"(with-output-to-string (o) (write-string o "hi") (terpri o))"#,
        r#"(let ((s (make-string-input-stream "ab"))) (format false "~a~a" (read-char s) (read-char s)))"#,
        "(logeqv (as bignum 12) (as bignum 10))",
        "(logandc1 (as bignum 12) (as bignum 10))",
        "(keywordp :k)",
    ];
    for src in cases {
        let (mut ch, mut cc, ci) = env(true);
        let cv = eval(&mut ch, &mut cc, &ci, src);
        let compiled = core::print(&ch, cv);
        let (mut ih, mut ic, ii) = env(false);
        let iv = eval(&mut ih, &mut ic, &ii, src);
        let interpreted = core::print(&ih, iv);
        assert_eq!(compiled, interpreted, "compiled and interpreted disagree on {}", src);
    }
}
