//! `(dump "path")` and `typl --image path`: writing a session's whole
//! environment to one file, and starting from it.
//!
//! Driven through the `typl` binary rather than the library, because the thing
//! under test is that a *second process* comes up with the first one's
//! definitions — which is exactly what a library-level test, sharing a heap and
//! an `Interp` with the code that wrote the dump, cannot show.
//!
//! The binary is the one `cargo test` has already built (`CARGO_BIN_EXE_typl`),
//! so no path is hardcoded.

use std::path::{Path, PathBuf};
use std::process::Command;

const TYPL: &str = env!("CARGO_BIN_EXE_typl");

/// A scratch directory of this test's own, so two tests never share a module
/// name — a script's file name *is* its module (`project::module_segs_for`), so
/// `mk.typl` in two tests would be two different `mk`s.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("typelisp-dump-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

fn write(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("writing a source file");
    path
}

/// Runs `typl` and returns its stdout, failing the test with both streams if it
/// exits non-zero.
fn run(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(TYPL).current_dir(dir).args(args).output().expect("running typl");
    assert!(
        out.status.success(),
        "typl {:?} failed with {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        args,
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn run_expecting_failure(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(TYPL).current_dir(dir).args(args).output().expect("running typl");
    assert!(!out.status.success(), "typl {:?} was expected to fail but succeeded", args);
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn a_function_defined_in_one_session_is_callable_from_the_image() {
    let dir = scratch("callable");
    write(&dir, "mk.typl", "(pub defun triple ((n i32)) i32 (* n 3))\n(dump \"session.typld\")\n");
    write(&dir, "use.typl", "(println \"~a\" (mk::triple 9))\n");
    run(&dir, &["mk.typl"]);
    assert_eq!(run(&dir, &["--image", "session.typld", "use.typl"]).trim(), "27");
}

/// The whole point of pairing bitcode with type information: a definition the
/// session `(compile)`d comes back compiled, not re-tree-walked.
///
/// Observed through the dump itself rather than by timing: the session unit
/// carries a bitcode section, and `load_unit` refuses to install a body for an
/// item the module has no body for, so a unit whose bitcode is empty could not
/// have carried the compiled `triple`.
#[test]
fn a_compiled_function_is_written_down_as_bitcode() {
    let dir = scratch("compiled");
    write(
        &dir,
        "mk.typl",
        "(pub defun triple ((n i32)) i32 (* n 3))\n(compile triple)\n(dump \"session.typld\")\n",
    );
    run(&dir, &["mk.typl"]);
    let bytes = std::fs::read(dir.join("session.typld")).expect("reading the dump");
    let units = typelisp::dump::parse(&bytes, "session").expect("parsing the dump");
    assert_eq!(units.len(), 3, "prelude, island, session");
    let session = typelisp::dump::read_state(units[2].types, "session").expect("reading the session unit");
    assert_eq!(session.label, "session");
    assert_eq!(session.items.len(), 1, "the one definition the session compiled");
    assert!(
        !units[2].bitcode.is_empty(),
        "the session unit carries no bitcode, so `triple`'s compiled body was not written down"
    );

    write(&dir, "use.typl", "(println \"~a\" (mk::triple 9))\n");
    assert_eq!(run(&dir, &["--image", "session.typld", "use.typl"]).trim(), "27");
}

/// A session that compiled nothing writes a unit with no bitcode section at
/// all — and that has to load, not trip the "asking LLVM to parse zero bytes"
/// path.
#[test]
fn a_session_that_compiled_nothing_still_dumps_and_loads() {
    let dir = scratch("nothing");
    write(&dir, "mk.typl", "(pub defun twice ((n i32)) i32 (+ n n))\n(dump \"session.typld\")\n");
    write(&dir, "use.typl", "(println \"~a\" (mk::twice 21))\n");
    run(&dir, &["mk.typl"]);
    let bytes = std::fs::read(dir.join("session.typld")).expect("reading the dump");
    let units = typelisp::dump::parse(&bytes, "session").expect("parsing the dump");
    assert!(units[2].bitcode.is_empty(), "nothing was compiled, so there is nothing to carry");
    assert_eq!(run(&dir, &["--image", "session.typld", "use.typl"]).trim(), "42");
}

/// The one place this deliberately differs from SBCL's `save-lisp-and-die`: a
/// dump saves definitions, so a global comes back at whatever its initializer
/// produces rather than the value the session last stored.
#[test]
fn a_global_comes_back_at_its_initializer_not_its_last_value() {
    let dir = scratch("globals");
    write(
        &dir,
        "mk.typl",
        "(pub defvar (counter i32) 1)\n\
         (setf counter 99)\n\
         (println \"before: ~a\" counter)\n\
         (dump \"session.typld\")\n",
    );
    write(&dir, "use.typl", "(println \"~a\" mk::counter)\n");
    assert_eq!(run(&dir, &["mk.typl"]).trim(), "before: 99");
    assert_eq!(run(&dir, &["--image", "session.typld", "use.typl"]).trim(), "1");
}

/// A session's top-level expressions are not history to replay: loading the
/// image must not re-run them.
#[test]
fn a_sessions_top_level_expressions_do_not_run_again() {
    let dir = scratch("expressions");
    write(
        &dir,
        "mk.typl",
        "(pub defun noisy () i32 7)\n(println \"this runs once\")\n(dump \"session.typld\")\n",
    );
    write(&dir, "use.typl", "(println \"~a\" (mk::noisy))\n");
    assert_eq!(run(&dir, &["mk.typl"]).trim(), "this runs once");
    assert_eq!(run(&dir, &["--image", "session.typld", "use.typl"]).trim(), "7");
}

/// A macro is a definition too, and its expansion has to come back with it.
#[test]
fn a_macro_defined_in_the_session_expands_from_the_image() {
    let dir = scratch("macro");
    write(
        &dir,
        "mk.typl",
        "(pub defmacro twice (x) `(+ ,x ,x))\n(dump \"session.typld\")\n",
    );
    write(&dir, "use.typl", "(println \"~a\" (mk::twice 21))\n");
    run(&dir, &["mk.typl"]);
    assert_eq!(run(&dir, &["--image", "session.typld", "use.typl"]).trim(), "42");
}

/// An image is not a general interchange format: its prelude and island units
/// were built from the sources compiled into some binary, and a binary whose
/// own sources differ has to refuse it rather than install definitions its
/// source no longer has.
///
/// Provoked by corrupting the recorded digest rather than by rebuilding the
/// compiler with a different prelude — same code path, no second build.
#[test]
fn an_image_built_from_a_different_prelude_is_refused() {
    let dir = scratch("stale");
    write(&dir, "mk.typl", "(pub defun one () i32 1)\n(dump \"session.typld\")\n");
    run(&dir, &["mk.typl"]);

    let path = dir.join("session.typld");
    let mut bytes = std::fs::read(&path).expect("reading the dump");
    // The prelude unit's `source_digest` is a `Some(u64)` inside its bincode;
    // flipping a byte of the payload is enough to make it disagree, which is
    // all this is testing.
    let types_offset = u64::from_le_bytes(bytes[14..22].try_into().unwrap()) as usize;
    bytes[types_offset + 8] ^= 0xff;
    std::fs::write(&path, &bytes).expect("writing the corrupted dump");

    write(&dir, "use.typl", "(println \"~a\" (mk::one))\n");
    let stderr = run_expecting_failure(&dir, &["--image", "session.typld", "use.typl"]);
    assert!(
        stderr.contains("regen-prelude-bitcode.sh") || stderr.contains("dump"),
        "a stale image should say what to do about it, got: {}",
        stderr
    );
}
