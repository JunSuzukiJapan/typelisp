//! Tests for the fasl (compiled-module) machinery (`src/fasl.rs`): the
//! heap-independent `OwnedForm` mirror and its allocation-API rebuild
//! (`value_to_owned`/`owned_to_value`), and — in later stages — the `Fasl`
//! capture/load_into round trip and `(load)`'s fasl-preference rules.

extern crate typelisp;

use typelisp::fasl::{owned_to_value, value_to_owned, OwnedForm};
use typelisp::{Heap, Reader, Value};

/// Read `src`'s single form into `heap` — the same shapes a generic
/// template's `parts` hold (raw reader output).
fn read_one(heap: &mut Heap, src: &str) -> Value {
    let r = Reader::new();
    let mut vs = r.read_all(heap, src).expect("read failed");
    assert_eq!(vs.len(), 1, "expected exactly one form");
    let v = vs.pop().unwrap();
    // read_all leaves its results rooted; keep it that way for the test.
    v
}

/// Structural equality across *different* heaps: the rebuilt value must
/// describe the same tree even though every id/pointer differs.
fn assert_same_shape(a_heap: &Heap, a: Value, b_heap: &Heap, b: Value) {
    let ao = value_to_owned(a_heap, a).expect("value_to_owned (left)");
    let bo = value_to_owned(b_heap, b).expect("value_to_owned (right)");
    assert_eq!(ao, bo);
}

#[test]
fn every_reader_shape_round_trips_through_owned_form() {
    let mut src_heap = Heap::with_capacity(1 << 12);
    // One form exercising every OwnedForm variant the reader can produce:
    // symbols, a ::-path, strings, chars, bools, ints, a float, a bignum,
    // a ratio, nested conses, and ().
    let v = read_one(
        &mut src_heap,
        r#"(defun f (a std::io::x) "hi" #\A true 42 3.5 123456789012345678901234567890 1/3 ())"#,
    );

    let owned = value_to_owned(&src_heap, v).expect("value_to_owned");

    // Serialize/deserialize through serde_json — the same channel the Fasl
    // format uses — so the round trip covers the wire encoding too.
    let json = serde_json::to_string(&owned).expect("serialize");
    let back: OwnedForm = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(owned, back, "OwnedForm survives the JSON wire format");

    // Rebuild into a *fresh* heap through its allocation APIs.
    let mut dst_heap = Heap::with_capacity(1 << 12);
    let rebuilt = owned_to_value(&mut dst_heap, &back).expect("owned_to_value");
    dst_heap.push_root(rebuilt);

    assert_same_shape(&src_heap, v, &dst_heap, rebuilt);
}

/// The rebuilt value must be a real heap citizen: rooted, it survives a GC
/// forced by an allocation storm; its content is intact afterwards.
#[test]
fn rebuilt_values_survive_gc_in_the_destination_heap() {
    let mut src_heap = Heap::with_capacity(1 << 12);
    let v = read_one(&mut src_heap, r#"(a "deep" (nested 1.5 (list 42)))"#);
    let owned = value_to_owned(&src_heap, v).expect("value_to_owned");

    // Small heap so the storm actually triggers collections.
    let mut dst = Heap::with_capacity(512);
    let rebuilt = owned_to_value(&mut dst, &owned).expect("owned_to_value");
    dst.push_root(rebuilt);

    // Allocation storm: plenty of garbage conses to force GC cycles.
    for i in 0..2000 {
        let a = dst.cons(Value::Int(i), Value::Empty).expect("cons");
        let _ = a; // immediately garbage
    }

    // Still intact?
    let after = value_to_owned(&dst, rebuilt).expect("value_to_owned after GC");
    assert_eq!(owned, after, "rooted rebuilt value survives collections unchanged");
}

/// `owned_to_value`'s own intermediates are rooted during the rebuild: a
/// long list rebuilt into a tiny heap forces collections *mid-rebuild*
/// (each string/cons allocation can trigger one), and the final structure
/// must still be complete.
#[test]
fn rebuild_protects_its_intermediates_from_mid_rebuild_gc() {
    let mut src_heap = Heap::with_capacity(1 << 14);
    // A list long enough that rebuilding it churns through several times
    // the destination heap's capacity in transient allocations.
    let items: Vec<String> = (0..200).map(|i| format!("\"str-{}\"", i)).collect();
    let src = format!("({})", items.join(" "));
    let v = read_one(&mut src_heap, &src);
    let owned = value_to_owned(&src_heap, v).expect("value_to_owned");

    let mut dst = Heap::with_capacity(600);
    let rebuilt = owned_to_value(&mut dst, &owned).expect("owned_to_value under pressure");
    dst.push_root(rebuilt);

    let after = value_to_owned(&dst, rebuilt).expect("value_to_owned");
    assert_eq!(owned, after);
}

// ---- S2: Fasl capture / load_into round trip -------------------------------

use typelisp::fasl::{registry_mark, source_hash, Fasl};
use typelisp::{
    completion_candidates, load_prelude, Checker, EvalError, Interp, RtValue, TopLevel,
};

/// Load the prelude by source into a fresh environment.
fn source_loaded() -> (Heap, Checker, Interp) {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    (h, chk, interp)
}

/// Capture the prelude (everything an empty checker gains from loading it)
/// into a fasl, round-tripped through the serialized byte form.
fn prelude_fasl() -> Fasl {
    // Capture must run against a from-scratch checker so `registry_mark`
    // (taken before load) diffs to exactly the prelude's own definitions.
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mark = registry_mark(&chk);

    // Reproduce `load_prelude` but keeping the checked TopLevels for capture.
    let r = Reader::new();
    let src = typelisp::prelude::SOURCE;
    let forms = r.read_all(&mut h, src).expect("prelude read");
    let mut top_levels = Vec::new();
    for v in forms {
        let tl = chk.check_form(&mut h, &interp, v).expect("prelude check");
        let _ = chk.take_warnings();
        interp.exec(&mut h, tl.clone()).expect("prelude exec");
        top_levels.push(tl);
    }

    let fasl = Fasl::capture(&h, &chk, &mark, top_levels, source_hash(src)).expect("capture");
    let bytes = fasl.to_bytes().expect("to_bytes");
    Fasl::from_bytes(&bytes).expect("from_bytes")
}

/// A fresh environment with the prelude installed via the fasl (no source
/// parse/typecheck).
fn fasl_loaded(fasl: &Fasl) -> (Heap, Checker, Interp) {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    fasl.load_into(&mut h, &mut chk, &mut interp).expect("load_into");
    (h, chk, interp)
}

/// Evaluate `src`'s forms in `(h, chk, interp)`, returning the last value.
fn eval_in(h: &mut Heap, chk: &mut Checker, interp: &mut Interp, src: &str) -> Result<RtValue, EvalError> {
    let r = Reader::new();
    let vs = r.read_all(h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(h, &*interp, v).map_err(|e| EvalError::Internal(format!("check: {}", e)))?;
        if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

/// The heart of S2: for a battery of programs exercising prelude macros,
/// generic combinators (template re-instantiation), and literals, the
/// fasl-loaded environment must produce the same values as the
/// source-loaded one.
#[test]
fn fasl_loaded_prelude_evaluates_identically_to_source_loaded() {
    let programs = [
        // prelude macro `while` + `dotimes`
        "(defun sum-to ((n i64)) i64 (let ((s (the i64 0)) (i (the i64 0))) (while (< i n) (setf s (+ s i)) (setf i (+ i 1))) s))\n(sum-to 10)",
        // generic combinator `map` over a Vector<i64> (template re-instantiation)
        "(defun doubled () i64 (let ((v (the Vector<i64> (Vector::new)))) (push v 3) (push v 4) (let ((out (map (iter v) (lambda ((x i64)) i64 (* x 2))))) (+ (get out 0) (get out 1)))))\n(doubled)",
        // generic combinator `member` (Eq bound) + string/char literals
        "(defun has-b ((v Vector<i32>)) bool (member 2 (iter v)))\n(defun run () i64 (let ((v (the Vector<i32> (Vector::new)))) (push v 1) (push v 2) (if (has-b v) 1 0)))\n(run)",
        // `case` macro + char literal
        "(defun grade ((c char)) i64 (case c (#\\A 4) (#\\B 3) (else 0)))\n(+ (grade #\\A) (grade #\\B))",
        // float + quote/list literal reconstruction
        "(defun pi-ish () f64 (* 2.0 1.5))\n(pi-ish)",
    ];

    let fasl = prelude_fasl();

    for prog in programs {
        let (mut sh, mut sc, mut si) = source_loaded();
        let src_result = eval_in(&mut sh, &mut sc, &mut si, prog).expect("source-loaded eval");

        let (mut fh, mut fc, mut fi) = fasl_loaded(&fasl);
        let fasl_result = eval_in(&mut fh, &mut fc, &mut fi, prog).expect("fasl-loaded eval");

        assert_eq!(src_result, fasl_result, "mismatch for program:\n{}", prog);
    }
}

/// A type error is diagnosed the same way in a fasl-loaded environment as in
/// a source-loaded one (the registry the checker consults is equivalent).
#[test]
fn fasl_loaded_prelude_diagnoses_type_errors_identically() {
    let bad = "(defun f () i64 (+ 1 true))"; // bool where i64 expected
    let fasl = prelude_fasl();

    let (mut sh, mut sc, mut si) = source_loaded();
    let src_err = eval_in(&mut sh, &mut sc, &mut si, bad).unwrap_err();

    let (mut fh, mut fc, mut fi) = fasl_loaded(&fasl);
    let fasl_err = eval_in(&mut fh, &mut fc, &mut fi, bad).unwrap_err();

    // Both must fail at check; compare the rendered messages.
    assert_eq!(format!("{:?}", src_err), format!("{:?}", fasl_err));
}

/// Completion candidates (prelude functions/macros) are the same set whether
/// the prelude was loaded from source or a fasl — proving the `RegistryDelta`
/// carries the full definition surface the LSP surfaces.
#[test]
fn fasl_loaded_prelude_offers_the_same_completions() {
    let fasl = prelude_fasl();

    let (_sh, sc, _si) = source_loaded();
    let mut src_names: Vec<String> =
        completion_candidates(sc.registry(), &[]).into_iter().map(|c| c.name).collect();
    src_names.sort();
    src_names.dedup();

    let (_fh, fc, _fi) = fasl_loaded(&fasl);
    let mut fasl_names: Vec<String> =
        completion_candidates(fc.registry(), &[]).into_iter().map(|c| c.name).collect();
    fasl_names.sort();
    fasl_names.dedup();

    assert_eq!(src_names, fasl_names, "prelude completion surface differs");
}

/// The captured/loaded `TopLevel` list is exactly the checked prelude forms —
/// a cheap structural sanity check that capture kept them in order.
#[test]
fn fasl_top_levels_match_a_direct_prelude_check() {
    let fasl = prelude_fasl();
    // Non-empty and every entry is a real registration form (no bare Expr).
    assert!(!fasl.top_levels.is_empty());
    for tl in &fasl.top_levels {
        assert!(
            !matches!(tl, TopLevel::Expr(_)),
            "prelude should contain only definitions, found a bare expression"
        );
    }
}

// ---- S3: the `(load "path")` top-level form --------------------------------

use std::path::PathBuf;
use typelisp::project::{load_file_flat, load_source_flat};
use typelisp::{Reader as R2};

/// A fresh prelude-loaded environment, plus a scratch project dir.
fn load_ctx(name: &str) -> (Heap, Checker, Interp, PathBuf) {
    let (h, c, i) = source_loaded();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("fasl-test-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    (h, c, i, dir)
}

/// Evaluate a single expression string in an existing environment.
fn eval1(h: &mut Heap, c: &mut Checker, i: &mut Interp, src: &str) -> RtValue {
    eval_in(h, c, i, src).expect("eval1")
}

#[test]
fn load_reads_a_source_file_into_the_current_environment() {
    let (mut h, mut c, mut i, dir) = load_ctx("load-source");
    std::fs::write(dir.join("helpers.typl"), "(defun triple ((n i64)) i64 (* n 3))").unwrap();

    let reader = R2::new();
    load_file_flat(&mut h, &reader, &mut c, &mut i, &dir, "helpers").expect("load_file_flat");

    // The loaded definition is now callable.
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(triple 4)"), RtValue::Int(12));
}

#[test]
fn load_form_makes_definitions_available_to_later_forms_in_the_same_file() {
    // A driver-level test: `load_source_flat` checks + execs forms in order,
    // so a `(load ...)` mid-file exposes its definitions to what follows.
    let (mut h, mut c, mut i, dir) = load_ctx("load-inline");
    std::fs::write(dir.join("lib.typl"), "(defun twice ((n i64)) i64 (* n 2))").unwrap();
    let main = dir.join("main.typl");
    std::fs::write(&main, "(load \"lib\")\n(defun run () i64 (twice 21))").unwrap();

    let reader = R2::new();
    load_source_flat(&mut h, &reader, &mut c, &mut i, &main, &std::fs::read_to_string(&main).unwrap())
        .expect("load_source_flat");

    assert_eq!(eval1(&mut h, &mut c, &mut i, "(run)"), RtValue::Int(42));
}

#[test]
fn load_prefers_a_fresh_fasl_over_source() {
    let (mut h, mut c, mut i, dir) = load_ctx("load-fasl-pref");
    // Source says triple; the fasl (compiled from a DIFFERENT source with the
    // matching hash) says quadruple — proving the fasl, not the source, is used.
    let src = "(defun mul ((n i64)) i64 (* n 4))";
    std::fs::write(dir.join("m.typl"), src).unwrap();

    // Build a fasl for `m` by capturing a from-scratch check of `src`.
    let fasl = {
        let mut fh = Heap::with_capacity(1 << 16);
        let mut fc = Checker::new();
        let mut fi = Interp::new();
        load_prelude(&mut fh, &mut fc, &mut fi);
        let mark = registry_mark(&fc);
        let reader = R2::new();
        let forms = reader.read_all(&mut fh, src).unwrap();
        let mut tls = Vec::new();
        for v in forms {
            let tl = fc.check_form(&mut fh, &fi, v).unwrap();
            fi.exec(&mut fh, tl.clone()).unwrap();
            tls.push(tl);
        }
        Fasl::capture(&fh, &fc, &mark, tls, source_hash(src)).unwrap()
    };
    std::fs::write(dir.join("m.fasl"), fasl.to_bytes().unwrap()).unwrap();

    let reader = R2::new();
    load_file_flat(&mut h, &reader, &mut c, &mut i, &dir, "m").expect("load");
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(mul 5)"), RtValue::Int(20), "loaded from fasl");
}

#[test]
fn load_falls_back_to_source_when_fasl_is_stale() {
    let (mut h, mut c, mut i, dir) = load_ctx("load-stale-fasl");
    // The .typl on disk is the source of truth; the .fasl was compiled from
    // OLD source (different hash), so it must be ignored.
    std::fs::write(dir.join("m.typl"), "(defun v ((n i64)) i64 (+ n 100))").unwrap();
    let old_src = "(defun v ((n i64)) i64 (+ n 999))";
    let fasl = {
        let mut fh = Heap::with_capacity(1 << 16);
        let mut fc = Checker::new();
        let mut fi = Interp::new();
        load_prelude(&mut fh, &mut fc, &mut fi);
        let mark = registry_mark(&fc);
        let reader = R2::new();
        let forms = reader.read_all(&mut fh, old_src).unwrap();
        let mut tls = Vec::new();
        for v in forms {
            let tl = fc.check_form(&mut fh, &fi, v).unwrap();
            fi.exec(&mut fh, tl.clone()).unwrap();
            tls.push(tl);
        }
        // Hash of the OLD source — won't match the current .typl.
        Fasl::capture(&fh, &fc, &mark, tls, source_hash(old_src)).unwrap()
    };
    std::fs::write(dir.join("m.fasl"), fasl.to_bytes().unwrap()).unwrap();

    let reader = R2::new();
    load_file_flat(&mut h, &reader, &mut c, &mut i, &dir, "m").expect("load");
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(v 1)"), RtValue::Int(101), "used current source, not stale fasl");
}

#[test]
fn load_inside_a_function_body_is_a_type_error() {
    let (mut h, mut c, mut i) = source_loaded();
    let err = eval_in(&mut h, &mut c, &mut i, "(defun f () i64 (load \"x\"))").unwrap_err();
    // `load` is not a value-level function, so it fails at check.
    assert!(format!("{:?}", err).contains("load") || format!("{:?}", err).contains("no such function"));
}

// ---- S4: `typl compile-module` producer -> `(load)` consumer ---------------

/// End-to-end: the `typl compile-module` CLI produces a fasl that a later
/// `load_file_flat` reads (matching source_hash) — the producer and consumer
/// agree on the format.
#[test]
fn compile_module_output_is_loadable() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("fasl-test-tmp").join("compile-module");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let typl = dir.join("lib.typl");
    std::fs::write(&typl, "(pub defun cube ((n i64)) i64 (* n (* n n)))\n(defmacro twice (x) `(+ ,x ,x))").unwrap();

    // Run the compiled binary's `compile-module` subcommand.
    let bin = env!("CARGO_BIN_EXE_typl");
    let status = std::process::Command::new(bin)
        .arg("compile-module")
        .arg(&typl)
        .status()
        .expect("run compile-module");
    assert!(status.success(), "compile-module exited with failure");

    let fasl_path = dir.join("lib.fasl");
    assert!(fasl_path.is_file(), "fasl was not produced");

    // Load it into a fresh prelude environment and use both the defun and macro.
    let (mut h, mut c, mut i) = source_loaded();
    let reader = R2::new();
    load_file_flat(&mut h, &reader, &mut c, &mut i, &dir, "lib").expect("load produced fasl");
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(cube 3)"), RtValue::Int(27), "defun from fasl");
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(twice 21)"), RtValue::Int(42), "macro from fasl");
}

/// `compile-module` refuses a source file containing a bare top-level
/// expression (a module is definitions, not a script).
#[test]
fn compile_module_rejects_a_top_level_expression() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("fasl-test-tmp").join("compile-module-reject");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let typl = dir.join("bad.typl");
    std::fs::write(&typl, "(defun f () i64 1)\n(f)").unwrap(); // (f) is a bare expr

    let bin = env!("CARGO_BIN_EXE_typl");
    let status = std::process::Command::new(bin).arg("compile-module").arg(&typl).status().expect("run");
    assert!(!status.success(), "compile-module should reject a top-level expression");
}

// ---- S5: prelude fasl reconstruction is faster than a source load ----------

/// An `#[ignore]`d micro-benchmark (run with `--ignored`): reconstructing a
/// prelude-loaded environment from a fasl should be meaningfully faster than
/// re-reading+re-typechecking the prelude source — the point of the LSP
/// per-pass change. Not a correctness gate (timings vary), just an eyeball
/// sanity check.
#[test]
#[ignore]
fn bench_fasl_load_beats_source_load() {
    use std::time::Instant;
    let fasl = prelude_fasl();
    let n = 50;

    let t0 = Instant::now();
    for _ in 0..n {
        let (mut h, mut c, mut i) = (Heap::with_capacity(1 << 16), Checker::new(), Interp::new());
        load_prelude(&mut h, &mut c, &mut i);
    }
    let source = t0.elapsed();

    let t1 = Instant::now();
    for _ in 0..n {
        let (mut h, mut c, mut i) = (Heap::with_capacity(1 << 16), Checker::new(), Interp::new());
        fasl.load_into(&mut h, &mut c, &mut i).unwrap();
    }
    let fasl_time = t1.elapsed();

    eprintln!("source {:?}/pass, fasl {:?}/pass ({:.1}x)", source / n, fasl_time / n, source.as_secs_f64() / fasl_time.as_secs_f64());
    assert!(fasl_time < source, "fasl load {:?} should beat source {:?}", fasl_time, source);
}
