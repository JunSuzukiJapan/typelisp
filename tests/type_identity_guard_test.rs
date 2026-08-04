//! Keeps the "a type is identified by its whole path" invariant enforceable.
//!
//! A heap value's type identity is a *string* (`Heap::alloc_struct`/
//! `alloc_enum`), and so is a built-in type's name. Both are `String`, so
//! nothing in the type system stops a new call site from spelling either one
//! as `Path::last_segment` — the last segment alone — which is exactly how
//! two bugs got in on 2026-08-04:
//!
//! - compiled code wrote a value's type name unqualified, so a value it built
//!   was unmatchable by interpreted code, printed `<unknown-variant>`, and
//!   compared unequal to its own twin (`2dd5171`);
//! - compiled code recognized the *built-in* `vector`/`hashtable` by last
//!   segment, so `(module m (defstruct vector ...))` was read as if it were
//!   the built-in — silently wrong, or a process abort (`7aebfd2`).
//!
//! Neither reproduces at the root namespace, where the two spellings are the
//! same string — which is why the whole test suite missed both. So this test
//! does not run code: it reads `src/**.rs` and enforces that the two decisions
//! go through their one entry point each.
//!
//! `tests/compile_test.rs`'s cross-boundary conformance matrix is the other
//! half of the defence: this one polices the *spelling*, that one the
//! *behaviour*.
//!
//! **When this test fails**, don't add the marker reflexively — first check
//! whether the new site is asking one of these two questions:
//!
//! | question | use |
//! |---|---|
//! | "is this path the built-in `X`?" | `types::path_is_builtin` / `path_is_builtin_any` |
//! | "which type is this heap value?" | `type_key::type_key_of` / `heap_type_is` / `heap_type_path` |
//!
//! A site that is genuinely neither (a stored key compared to another stored
//! key, a root built-in assembled from a literal) opts out with a
//! `// type-identity-ok: <reason>` comment on the offending line or the line
//! before it. The reason is the point — it is what a later reader checks.

use std::path::{Path as FsPath, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file under `src/`, recursively.
fn src_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(&repo_root().join("src"), &mut out);
    out.sort();
    assert!(out.len() > 10, "src/ scan found only {} files — is the walk broken?", out.len());
    out
}

fn collect(dir: &FsPath, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src/ is readable") {
        let p = entry.expect("readable dir entry").path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// The opt-out marker, honoured on the offending line or the one above it.
const MARKER: &str = "type-identity-ok:";

/// A line to report: file, 1-based line number, the line itself.
struct Hit {
    file: String,
    line: usize,
    text: String,
}

/// Scan every `src/**.rs` line with `flag`, skipping lines exempted by
/// [`MARKER`], by `skip_file`, or by being a comment.
fn scan(skip_file: &dyn Fn(&str) -> bool, flag: &dyn Fn(&str) -> bool) -> Vec<Hit> {
    let mut hits = Vec::new();
    for path in src_files() {
        let rel = path.strip_prefix(repo_root()).unwrap().to_string_lossy().replace('\\', "/");
        if skip_file(&rel) {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("source is readable");
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            // Doc comments and ordinary comments describe the rule as often
            // as they break it.
            if trimmed.starts_with("//") {
                continue;
            }
            if !flag(line) {
                continue;
            }
            // The marker may sit on the line itself or anywhere in the block
            // of comment lines directly above it, so a two-line reason reads
            // naturally instead of having to end with the marker.
            let mut exempt = line.contains(MARKER);
            let mut j = i;
            while !exempt && j > 0 && lines[j - 1].trim_start().starts_with("//") {
                exempt = lines[j - 1].contains(MARKER);
                j -= 1;
            }
            if !exempt {
                hits.push(Hit { file: rel.clone(), line: i + 1, text: line.trim().to_string() });
            }
        }
    }
    hits
}

fn report(hits: &[Hit], rule: &str, fix: &str) {
    if hits.is_empty() {
        return;
    }
    let mut msg = format!("\n{}\n\n{}\n\n", rule, fix);
    for h in hits {
        msg.push_str(&format!("  {}:{}\n      {}\n", h.file, h.line, h.text));
    }
    msg.push_str(&format!(
        "\nIf a site is genuinely neither, exempt it with `// {} <reason>` on that line \
         or in the comment block above it (see this test's doc comment).\n",
        MARKER
    ));
    panic!("{}", msg);
}

/// Rule 1: deciding "is this path the built-in `X`?" by comparing the last
/// segment to a string literal, without the `is_simple` half.
#[test]
fn a_builtin_type_is_never_recognized_by_its_last_segment_alone() {
    let hits = scan(
        // `path_is_builtin`'s own body is the one place the comparison lives.
        &|rel| rel == "src/types.rs",
        &|line| {
            line.contains("last_segment()")
                && (line.contains("last_segment() == \"")
                    || line.contains("matches!(") && line.contains('"'))
        },
    );
    report(
        &hits,
        "A built-in type must be recognized by its whole path, not its last segment.",
        "`m::vector` and the built-in `vector` share a last segment but are different types \
         (fixed in 7aebfd2). Use `types::path_is_builtin(p, \"vector\")` or \
         `types::path_is_builtin_any(p, &NATIVE_LOWERED_PRIMITIVES)` — and put the name list \
         in `src/types.rs` next to the others rather than spelling it inline.",
    );
}

/// Rule 2: reading or writing a heap value's type identity outside its one
/// module.
#[test]
fn a_heap_values_type_identity_is_only_touched_through_type_key() {
    let hits = scan(
        &|rel| rel == "src/type_key.rs",
        &|line| {
            ["alloc_struct(", "alloc_enum(", "struct_type_name(", "enum_type_name("]
                .iter()
                .any(|needle| line.contains(needle))
        },
    );
    report(
        &hits,
        "A heap value's type identity must go through `src/type_key.rs`.",
        "The stored string is the value's identity, and compiled code, the interpreter, the \
         printer and `equalp` all have to agree on how it is spelled — they did not, twice \
         (2dd5171). Write it with `type_key::type_key_of`, compare it with \
         `type_key::heap_type_is`, read it back with `type_key::heap_type_path`.",
    );
}

/// The scan is only worth anything if it actually looks at the files the two
/// bugs were in — a walk that silently found nothing would pass both rules.
#[test]
fn the_scan_covers_the_files_the_invariant_lives_in() {
    let files: Vec<String> = src_files()
        .iter()
        .map(|p| p.strip_prefix(repo_root()).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    for expected in
        ["src/types.rs", "src/type_key.rs", "src/eval/interp.rs", "src/compile/ast_bridge.rs"]
    {
        assert!(files.contains(&expected.to_string()), "scan missed {}", expected);
    }
}
