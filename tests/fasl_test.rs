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
    // read_all leaves its results rooted; keep it that way for the test.
    vs.pop().unwrap()
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

/// A form's source spans survive the round trip: both the span of a list form
/// and the span of each element inside it.
///
/// The spans live in the cons cell (see `Cell` in `typelisp-mem`), so the
/// serializer has to carry them explicitly and the rebuild has to re-record
/// them — the old address-keyed tables could not have travelled at all, since
/// the address belongs to a heap that no longer exists by then.
#[test]
fn source_spans_survive_the_owned_form_round_trip() {
    let mut src = Heap::with_capacity(1 << 12);
    let r = Reader::new();
    let v = r
        .read_all_in(&mut src, "lib.typl", "(inc x)")
        .expect("read")
        .pop()
        .expect("one form");

    let form_span = src.cons_loc(v).expect("the list form has a span");
    let elem_spans: Vec<_> = src.list_to_vec_locs(v).expect("proper list").into_iter().map(|(_, l)| l).collect();
    assert!(elem_spans.iter().all(Option::is_some), "every element has a span: {:?}", elem_spans);

    // Through the wire form, so the spans survive serialization and not merely
    // the in-memory conversion.
    let owned = value_to_owned(&src, v).expect("value_to_owned");
    let json = serde_json::to_string(&owned).expect("serialize");
    let back: OwnedForm = serde_json::from_str(&json).expect("deserialize");

    let mut dst = Heap::with_capacity(1 << 12);
    let rebuilt = owned_to_value(&mut dst, &back).expect("owned_to_value");
    dst.push_root(rebuilt);

    assert_eq!(dst.cons_loc(rebuilt), Some(form_span), "the list form's own span");
    let rebuilt_spans: Vec<_> =
        dst.list_to_vec_locs(rebuilt).expect("proper list").into_iter().map(|(_, l)| l).collect();
    assert_eq!(rebuilt_spans, elem_spans, "each element's own span");
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
    completion_candidates, load_compiler, load_prelude, Checker, EvalError, Interp,
};

/// Load the prelude by source into a fresh environment.
fn source_loaded() -> (Heap, Checker, Interp) {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    // Some comparison programs build closures, which JIT through the compiler
    // island, so both environments must have it loaded.
    load_compiler(&mut h, &mut chk, &mut interp);
    (h, chk, interp)
}

/// Capture the prelude (everything an empty checker gains from loading it)
/// into a fasl, round-tripped through the serialized byte form.
fn prelude_fasl() -> Fasl {
    // Capture must run against a from-scratch checker so `registry_mark`
    // (taken before load) diffs to exactly the prelude's own definitions.
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mark = registry_mark(&chk);

    // Reproduce `load_prelude` but keeping the checked top-level forms for
    // capture.
    // `predeclare_program` is part of that: since the two-pass top level
    // (2026-08-01), the prelude relies on forward references between its own
    // `defun`s, so a loop that skips the pass is not "load_prelude" at all —
    // it is a stricter checker that the real thing never runs under.
    let r = Reader::new();
    let src = typelisp::prelude::SOURCE;
    let forms = r.read_all(&mut h, src).expect("prelude read");
    chk.predeclare_program(&mut h, &forms);
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
    // Match `source_loaded`: the island is what JITs any closure a comparison
    // program builds.
    load_compiler(&mut h, &mut chk, &mut interp);
    (h, chk, interp)
}

/// Evaluate `src`'s forms in `(h, chk, interp)`, returning the last value.
fn eval_in(h: &mut Heap, chk: &mut Checker, interp: &mut Interp, src: &str) -> Result<Value, EvalError> {
    let r = Reader::new();
    let vs = r.read_all(h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(h, &*interp, v).map_err(|e| EvalError::Internal(format!("check: {}", e)))?;
        if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

/// An evaluation result in a heap-independent form, so results from two
/// separately-loaded environments (each with its own `Heap`) can be compared.
/// `value_to_owned` already mirrors the `Value` side structurally; since the
/// scalar unification an `f64` lives there too — as a `BoxedObj::Float` —
/// rather than self-contained inside `RtValue`, so comparing the `RtValue`s
/// directly would compare two unrelated box ids.
fn owned_result(h: &Heap, v: &Value) -> String {
    format!("{:?}", value_to_owned(h, *v).expect("value_to_owned"))
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

        assert_eq!(
            owned_result(&sh, &src_result),
            owned_result(&fh, &fasl_result),
            "mismatch for program:\n{}",
            prog
        );
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

/// A docstring survives a fasl round trip: capture a small documented
/// module on top of the fasl-loaded prelude (the same shape a dependent
/// source file's own module fasl takes — `Checker::registry().docs`
/// diffed via `registry_mark`, same as `def_locs`), reload it into a fresh
/// environment built *entirely* from fasls (no source at all beyond the
/// module's own defun body), and confirm `(documentation add)` still
/// resolves — the scenario `FASL_FORMAT_VERSION` bump 15's doc comment
/// warns a stale pre-15 cache would silently break.
#[test]
fn fasl_capture_preserves_docstrings_for_documentation() {
    let prelude = prelude_fasl();

    let (mut h, mut chk, interp) = fasl_loaded(&prelude);
    let mark = registry_mark(&chk);
    let src = r#"
        (defun add ((x i32) (y i32)) i32
          "Adds two integers."
          (+ x y))
    "#;
    let r = Reader::new();
    let forms = r.read_all(&mut h, src).expect("read");
    let mut top_levels = Vec::new();
    for v in forms {
        let tl = chk.check_form(&mut h, &interp, v).expect("check");
        interp.exec(&mut h, tl.clone()).expect("exec");
        top_levels.push(tl);
    }
    let module_fasl = Fasl::capture(&h, &chk, &mark, top_levels, source_hash(src)).expect("capture");
    let bytes = module_fasl.to_bytes().expect("to_bytes");
    let module_fasl = Fasl::from_bytes(&bytes).expect("from_bytes");

    // A fresh environment assembled purely from fasls — prelude, then the
    // module — with no source read at all.
    let (mut fh, mut fc, mut fi) = fasl_loaded(&prelude);
    module_fasl.load_into(&mut fh, &mut fc, &mut fi).expect("module load_into");

    let result = eval_in(&mut fh, &mut fc, &mut fi, r#"(unwrap-or (documentation add) "none")"#).expect("eval");
    match result {
        Value::Str(id) => assert_eq!(fh.string(id), "Adds two integers."),
        other => panic!("expected a string, got {:?}", other),
    }
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

/// The captured/loaded top-level list is exactly the checked prelude forms —
/// a cheap structural sanity check that capture kept them in order.
#[test]
fn fasl_top_levels_match_a_direct_prelude_check() {
    let fasl = prelude_fasl();
    // Non-empty and every entry is a real registration form (no bare `expr`).
    assert!(!fasl.top_levels.is_empty());
    for tl in &fasl.top_levels {
        // A captured form is an `OwnedForm`, not a live heap value, so the tag
        // is read straight off the cons cell's `car`.
        let tag = match tl {
            OwnedForm::Cons { cells, .. } => match cells.first().map(|c| &c.form) {
                Some(OwnedForm::Sym(name)) => name.as_str(),
                other => panic!("a top-level form's tag is a symbol, got {:?}", other),
            },
            other => panic!("a top-level form is a tagged list, got {:?}", other),
        };
        assert_ne!(tag, "expr", "prelude should contain only definitions, found a bare expression");
    }
}

/// A runtime error raised inside a fasl-loaded function still reports the
/// `file:line:col` it was written at.
///
/// The guard for a property that is easy to lose without noticing: a source
/// location is *not* part of a value, so it only survives serialization if
/// something deliberately carries it. Today the checked node owns it (a `loc`
/// field, serialized with the rest of the node); a cons-cell program keeps it
/// beside the cell instead, and "beside" is exactly what a serializer walking
/// the cells does not see. The test is written against the property, not the
/// mechanism, so it holds either way — and fails loudly if a rewrite of the
/// carrier drops it.
///
/// No prelude: `defun`/`panic`/`i64` are all the language's own, so a bare
/// checker suffices and the test costs milliseconds.
#[test]
fn a_fasl_loaded_function_reports_its_own_source_position() {
    // Line 3, and `(panic ...)` starts at column 20 — both asserted below, so
    // this layout is load-bearing. Anything that shifts it must update them.
    let src = "\
(defun ok () i64 1)
(pub defun boom () i64
                   (panic \"from the fasl\"))
";
    let file = "lib.typl";

    // Capture: check the module in its own environment, keeping the forms.
    let fasl = {
        let mut h = Heap::with_capacity(1 << 14);
        let mut chk = Checker::new();
        let interp = Interp::new();
        let mark = registry_mark(&chk);
        let r = Reader::new();
        let forms = r.read_all_in_spanned(&mut h, file, src).expect("read");
        let plain: Vec<Value> = forms.iter().map(|(v, _)| *v).collect();
        chk.predeclare_program(&mut h, &plain);
        let mut top_levels = Vec::new();
        for (v, loc) in forms {
            let tl = chk.check_form_at(&mut h, &interp, v, Some(loc)).expect("check");
            let _ = chk.take_warnings();
            interp.exec(&mut h, tl.clone()).expect("exec");
            top_levels.push(tl);
        }
        let fasl = Fasl::capture(&h, &chk, &mark, top_levels, source_hash(src)).expect("capture");
        // Through the wire form, so the position has to survive serialization
        // rather than merely surviving in memory.
        let bytes = fasl.to_bytes().expect("to_bytes");
        Fasl::from_bytes(&bytes).expect("from_bytes")
    };

    // Load into a *fresh* environment — a different heap, so every cons cell,
    // symbol id and string id differs from the ones the position was recorded
    // against.
    let mut h = Heap::with_capacity(1 << 14);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    fasl.load_into(&mut h, &mut chk, &mut interp).expect("load_into");

    // Not `eval_in`: it strips the location (`EvalError::into_kind`), which is
    // the very thing under test.
    let err = {
        let r = Reader::new();
        let vs = r.read_all(&mut h, "(boom)").expect("read");
        let tl = chk.check_form(&mut h, &interp, vs[0]).expect("check the call");
        interp.exec(&mut h, tl).expect_err("boom panics")
    };
    let loc = match &err {
        EvalError::At(loc, _) => loc.clone(),
        other => panic!("expected a located error, got {:?}", other),
    };
    assert!(matches!(err.kind(), EvalError::Panic(m) if m == "from the fasl"), "{:?}", err);
    assert_eq!(&*loc.file, file, "the file the function was written in");
    assert_eq!((loc.line, loc.col), (3, 20), "the `panic` form's own position");
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
fn eval1(h: &mut Heap, c: &mut Checker, i: &mut Interp, src: &str) -> Value {
    eval_in(h, c, i, src).expect("eval1")
}

#[test]
fn load_reads_a_source_file_into_the_current_environment() {
    let (mut h, mut c, mut i, dir) = load_ctx("load-source");
    std::fs::write(dir.join("helpers.typl"), "(defun triple ((n i64)) i64 (* n 3))").unwrap();

    let reader = R2::new();
    load_file_flat(&mut h, &reader, &mut c, &mut i, &dir, "helpers").expect("load_file_flat");

    // The loaded definition is now callable.
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(triple 4)"), Value::Int(12));
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

    assert_eq!(eval1(&mut h, &mut c, &mut i, "(run)"), Value::Int(42));
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
    std::fs::write(dir.join("m.fastl"), fasl.to_bytes().unwrap()).unwrap();

    let reader = R2::new();
    load_file_flat(&mut h, &reader, &mut c, &mut i, &dir, "m").expect("load");
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(mul 5)"), Value::Int(20), "loaded from fasl");
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
    std::fs::write(dir.join("m.fastl"), fasl.to_bytes().unwrap()).unwrap();

    let reader = R2::new();
    load_file_flat(&mut h, &reader, &mut c, &mut i, &dir, "m").expect("load");
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(v 1)"), Value::Int(101), "used current source, not stale fasl");
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

    let fasl_path = dir.join("lib.fastl");
    assert!(fasl_path.is_file(), "fasl was not produced");

    // Load it into a fresh prelude environment and use both the defun and macro.
    let (mut h, mut c, mut i) = source_loaded();
    let reader = R2::new();
    load_file_flat(&mut h, &reader, &mut c, &mut i, &dir, "lib").expect("load produced fasl");
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(cube 3)"), Value::Int(27), "defun from fasl");
    assert_eq!(eval1(&mut h, &mut c, &mut i, "(twice 21)"), Value::Int(42), "macro from fasl");
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

