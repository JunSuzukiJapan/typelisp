//! Keeps the "a type is identified by its whole path" invariant enforceable.
//!
//! A heap value's type identity is an interned `TypeKeyId`, but the *name*
//! it was interned under is a `String`, and so is a built-in type's name.
//! Both are `String`, so nothing in the type system stops a new call site
//! from spelling either one as `Path::last_segment` — the last segment alone
//! — which is exactly how two bugs got in on 2026-08-04:
//!
//! - compiled code wrote a value's type name unqualified, so it interned a
//!   *different* identity, and a value it built
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

mod common;
use common::{repo_root};
use std::path::{Path as FsPath, PathBuf};

/// Every `.rs` file the invariant can be broken in, recursively: the backend
/// (`src/`) and the front end (`crates/typelisp-front/src/`).
///
/// Those two and no more, because a type key is derived from a `Path` and
/// those are the only crates that have one. The runtime crates below them
/// (`typelisp-rt`, `typelisp-print`, `typelisp-read`) spell a handful of keys
/// as string constants precisely *because* they cannot derive them — that end
/// of the agreement is checked by
/// [`the_runtimes_type_keys_are_the_ones_type_key_of_produces`], which
/// compares the constants themselves rather than scanning for them.
fn src_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(&repo_root().join("src"), &mut out);
    collect(&repo_root().join("crates/typelisp-front/src"), &mut out);
    out.sort();
    assert!(out.len() > 10, "the scan found only {} files — is the walk broken?", out.len());
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
        &|rel| rel == "crates/typelisp-front/src/types.rs",
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
         in `crates/typelisp-front/src/types.rs` next to the others rather than spelling it inline.",
    );
}

/// Rule 2: reading or writing a heap value's type identity outside its one
/// module.
#[test]
fn a_heap_values_type_identity_is_only_touched_through_type_key() {
    let hits = scan(
        &|rel| rel == "crates/typelisp-front/src/type_key.rs",
        &|line| {
            [
                "alloc_struct(",
                "alloc_enum(",
                "intern_type_key(",
                "struct_type_key(",
                "enum_type_key(",
                "struct_type_name(",
                "enum_type_name(",
            ]
            .iter()
            .any(|needle| line.contains(needle))
        },
    );
    report(
        &hits,
        "A heap value's type identity must go through `crates/typelisp-front/src/type_key.rs`.",
        "The interned key is the value's identity, and compiled code, the interpreter, the \
         printer and `equalp` all have to agree on the name it is interned under — they did \
         not, twice (2dd5171). Mint it with `type_key::type_key_id`, write it with \
         `type_key::alloc_typed_struct`/`alloc_typed_enum`, compare it with \
         `type_key::heap_type_is`, read it back with `type_key::heap_type_path`.",
    );
}

/// Rule 3: the type identities the runtime crates mint for themselves must be
/// interned under the names `type_key_of` produces.
///
/// The source scan cannot reach them — `crates/typelisp-rt` is a separate
/// crate with no `Path` type to derive a key from, which is why
/// `rt_data_new` has always received its key as a string from compiled code
/// instead. `stream_builtin` is the one place that cannot: it builds
/// `Result`/`Option`/`FileError` values *itself*, on both sides of the
/// compile boundary. `sys_builtin` is the second such place, for the same
/// reason (`Result<_, ParseIntError>` and friends), and `typelisp_read::shim`
/// the third (`Result<Sexpr, ReadError>`). They all name those types through
/// `typelisp_mem`'s pre-interned table, whose entries every `Heap` mints at
/// construction so that a `TypeKeyId` for them is a compile-time constant.
///
/// This is that table's end of the agreement, checked rather than assumed — a
/// value built under a key nobody else spells is unmatchable, prints as
/// `<unknown-variant>`, and compares unequal to its own twin, silently.
#[test]
fn the_runtimes_type_keys_are_the_ones_type_key_of_produces() {
    use typelisp::types::Path;
    use typelisp::{TypeKeyId, BUILTIN_TYPE_KEYS};

    for (key, path) in [
        (TypeKeyId::OPTION, "option"),
        (TypeKeyId::RESULT, "result"),
        (TypeKeyId::FILE_ERROR, typelisp::check::registry::FILE_ERROR),
        (TypeKeyId::READ_ERROR, typelisp::check::registry::READ_ERROR),
        (TypeKeyId::PARSE_INT_ERROR, typelisp::check::registry::PARSE_INT_ERROR),
        (TypeKeyId::PARSE_FLOAT_ERROR, typelisp::check::registry::PARSE_FLOAT_ERROR),
        // `file-list-directory` builds a `Vector<string>` box directly, and
        // `read-datum-at` pairs its datum with its end index in a
        // `cons-cell<Sexpr, int>` box (CL's second return value, which this
        // language has no multiple values to carry) — both on the runtime's
        // own side of the boundary, so both need the identity too.
        (TypeKeyId::VECTOR, "vector"),
        (TypeKeyId::CONS_CELL, "cons-cell"),
        // The heap's own machinery: a `HashTable<K,V>` box, and the frames a
        // `Scope<V>` is made of.
        (TypeKeyId::HASHTABLE, "hashtable"),
        (TypeKeyId::SCOPE, "scope"),
        (TypeKeyId::SCOPE_FRAME, "scope-frame"),
        // Three prelude `defstruct`s the runtime builds itself, because the
        // facts in them belong to the runtime and not to the checker: two
        // clocks (`get-universal-time` / `get-internal-real-time`) and the
        // heap's own statistics (`heap-info`, which `room` prints).
        (TypeKeyId::UNIVERSAL_TIME, "universal-time"),
        (TypeKeyId::INTERNAL_TIME, "internal-time"),
        (TypeKeyId::HEAP_INFO, "heap-info"),
        // The socket layer's error type, built by `net_builtin`.
        (TypeKeyId::NET_ERROR, "neterror"),
    ] {
        assert_eq!(
            BUILTIN_TYPE_KEYS[key.as_u32() as usize],
            typelisp::type_key::type_key_of(&Path::root(path)),
            "the pre-interned key at index {} is not the one `type_key_of` produces for `{}`",
            key.as_u32(),
            path
        );
    }
}

/// Every key a runtime shim spells for a builtin's *result* must be the one
/// `type_key_of_type` produces for that builtin's registered return type.
///
/// A type's runtime identity includes its instantiation now, and the shims
/// below the checker (`typelisp_rt`, `typelisp_read`) have no `Path` to derive
/// one from — so they carry tables of hand-written spellings, one row per
/// builtin. Both sides are keyed by the *builtin's name*, which is what makes
/// this check mechanical rather than a second hand-written list: the registry
/// says what `stream-read-char` returns, the table says what key it builds,
/// and they must agree character for character.
///
/// Getting this wrong is silent in exactly the way the module comment
/// describes — the value would be unmatchable by a pattern the checker
/// compiled from its own spelling.
#[test]
fn the_runtime_result_keys_match_the_registry() {
    use typelisp::check::Registry;
    use typelisp::types::Path;

    let reg = Registry::with_builtins();
    let expected = |name: &str| -> String {
        let sig = reg
            .fn_sig(&Path::root(name))
            .unwrap_or_else(|| panic!("`{}` has a result-key row but no registry entry", name));
        typelisp::type_key::type_key_of_type(&sig.ret)
    };

    for (name, key) in typelisp_rt::stream_builtin::RESULT_KEYS {
        assert_eq!(*key, expected(name), "`{}`'s result key", name);
    }
    for (name, key) in typelisp_rt::sys_builtin::RESULT_KEYS {
        assert_eq!(*key, expected(name), "`{}`'s result key", name);
    }
    for (name, key) in typelisp_rt::net_builtin::RESULT_KEYS {
        assert_eq!(*key, expected(name), "`{}`'s result key", name);
    }
    assert_eq!(typelisp_read::shim::READ_RESULT_KEY, expected("read"), "`read`'s result key");
    assert_eq!(
        typelisp_rt::readtable::READER_MACRO_OPTION_KEY,
        expected("get-macro-character"),
        "`get-macro-character`'s result key"
    );
    assert_eq!(
        typelisp_rt::readtable::READER_MACRO_OPTION_KEY,
        expected("get-dispatch-macro-character"),
        "`get-dispatch-macro-character`'s result key"
    );
    assert_eq!(
        typelisp_read::shim::READ_DATUM_RESULT_KEY,
        expected("read-datum-at"),
        "`read-datum-at`'s result key"
    );

    // The *inner* keys: the box a `Result`'s `ok` payload is. Taken from the
    // registry's own type rather than by unwrapping the outer key's spelling,
    // so the check is about the identity and not about the parsing.
    let ok_payload = |name: &str| -> String {
        let sig = reg.fn_sig(&Path::root(name)).expect("registered");
        match &sig.ret {
            typelisp::Type::Named(_, args) if !args.is_empty() => {
                typelisp::type_key::type_key_of_type(&args[0])
            }
            other => panic!("`{}` returns {:?}, which has no payload type", name, other),
        }
    };
    for (name, inner) in typelisp_rt::stream_builtin::INNER_KEYS {
        assert_eq!(*inner, ok_payload(name), "`{}`'s inner key", name);
    }
    for (name, inner) in typelisp_rt::net_builtin::INNER_KEYS {
        assert_eq!(*inner, ok_payload(name), "`{}`'s inner key", name);
    }
    assert_eq!(
        typelisp_read::shim::READ_DATUM_PAIR_KEY,
        ok_payload("read-datum-at"),
        "`read-datum-at`'s pair key"
    );
}

/// The `TypeKeyId` constants must be the indices `Heap::with_capacity` interns
/// [`BUILTIN_TYPE_KEYS`] at — the whole reason those constants can exist.
///
/// Rule 3 above compares *names*, so an off-by-one in the constants would
/// still let it pass while every runtime-built value silently took on a
/// neighbour's identity. This checks the ids against a real heap.
#[test]
fn the_builtin_type_key_constants_are_what_a_heap_interns() {
    use typelisp::{Heap, BUILTIN_TYPE_KEYS};

    let mut heap = Heap::with_capacity(64);
    for (i, name) in BUILTIN_TYPE_KEYS.iter().enumerate() {
        let id = heap.intern_type_key(name);
        assert_eq!(
            id.as_u32() as usize,
            i,
            "`{}` does not intern at its table index — `Heap::with_capacity` and \
             `BUILTIN_TYPE_KEYS` have drifted apart",
            name
        );
        assert_eq!(&*heap.type_key_name(id), *name);
    }
}

/// Every [`BUILTIN_SYMBOLS`] entry reports the `wk` constant that names it.
///
/// The same drift this file's other constant test guards, one table over: a
/// syntax word reporting the wrong `wk` value would silently stop being
/// recognized (`&rest` read as an ordinary parameter, `pub` as a definition
/// name), and nothing about the spelling would look wrong.
///
/// Each name must also already be canonical — interning folds to lowercase, so
/// a name spelled otherwise would intern under a different one.
#[test]
fn the_builtin_symbols_report_their_own_wk_constant() {
    use typelisp::{Value, BUILTIN_SYMBOLS};

    let mut heap = typelisp::Heap::with_capacity(64);
    for (i, name) in BUILTIN_SYMBOLS.iter().enumerate() {
        assert_eq!(*name, name.to_lowercase(), "`{}` is not the canonical form it interns under", name);
        let Value::Symbol(sym) = heap.intern_symbol(name) else {
            panic!("intern_symbol always returns a symbol");
        };
        assert_eq!(
            sym.well_known() as usize,
            i,
            "`{}` reports a `wk` value that is not its own — the macro's table and the \
             constants it generates have drifted apart",
            name
        );
        assert_eq!(sym.name(), *name);
    }
}

/// The scan is only worth anything if it actually looks at the files the two
/// bugs were in — a walk that silently found nothing would pass both rules.
#[test]
fn the_scan_covers_the_files_the_invariant_lives_in() {
    let files: Vec<String> = src_files()
        .iter()
        .map(|p| p.strip_prefix(repo_root()).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    // `src/compile/core_bridge.rs` stands where `ast_bridge.rs` did: it is the
    // successor of the file one of the two original bugs was in, and the one that
    // spells a runtime type name on the compiled side today.
    for expected in [
        "crates/typelisp-front/src/types.rs",
        "crates/typelisp-front/src/type_key.rs",
        "crates/typelisp-front/src/eval/interp.rs",
        "src/compile/core_bridge.rs",
    ] {
        assert!(files.contains(&expected.to_string()), "scan missed {}", expected);
    }
}
