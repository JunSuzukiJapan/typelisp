//! Semantic highlighting of type names — resolution-driven, across files.
//!
//! The editor modes can only see the types a *buffer* defines, and can only
//! match them textually. The language server does neither: while checking, the
//! checker records a `TypeUse` at every position its grammar actually resolved
//! a user-defined type or trait name (`Checker::take_type_uses`), so
//!
//! - a type imported through `(use model::todo-item)` highlights (the module
//!   graph is resolved by the time the entry file checks), and
//! - a *function* sharing a type's name never does — the old text-scan design
//!   documented that false positive as accepted; recording spans at
//!   resolution removes it, which the collision test below pins down.
//!
//! The document is loaded exactly the way `src/bin/lsp.rs`'s `diagnostics_for`
//! loads it (prelude, then `Loader::load_entry_src` from the project root), so
//! what is asserted here is what the server would answer.

extern crate typelisp;

use std::path::{Path as FsPath, PathBuf};

use typelisp::check::semantic::{encode, file_type_tokens, TypeKind, TypeToken, TypeUse};
use typelisp::project::{find_src_root, Loader};
use typelisp::{Checker, Heap, Interp, Reader};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Check the source `text` as the content of `path` the way the language
/// server does, and return what it recorded and would highlight.
fn analyze_src(path: &FsPath, text: &str) -> (Vec<TypeUse>, Vec<TypeToken>) {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_prelude(&mut heap, &mut checker, &mut interp);
    checker.set_recover(true);
    // Everything the prelude load itself recorded is in another "file"
    // (`file_type_tokens` filters by file), but drain it anyway so `uses`
    // holds only what checking this document produced.
    let _ = checker.take_type_uses();

    let dir = path.parent().expect("file has a parent").to_path_buf();
    let src_root = find_src_root(&dir).unwrap_or(dir);
    let mut loader = Loader::new(src_root);
    loader
        .load_entry_src(&mut heap, &reader, &mut checker, &mut interp, path, text)
        .expect("the example checks cleanly");

    let uses = checker.take_type_uses();
    let tokens = file_type_tokens(&uses, path.to_str().expect("utf-8 path"));
    (uses, tokens)
}

/// Check `rel` (a repo-relative example file) and return the recorded uses,
/// the tokens, and the file's text.
fn analyze(rel: &str) -> (Vec<TypeUse>, Vec<TypeToken>, String) {
    let path = repo_root().join(rel);
    let text = std::fs::read_to_string(&path).expect("example file is readable");
    let (uses, tokens) = analyze_src(&path, &text);
    (uses, tokens, text)
}

/// The source text each token covers, with its 1-based line number.
fn spans(text: &str, tokens: &[TypeToken]) -> Vec<(u32, String)> {
    let lines: Vec<Vec<char>> = text.lines().map(|l| l.chars().collect()).collect();
    tokens
        .iter()
        .map(|t| {
            let line = &lines[t.line as usize];
            let s: String =
                line[t.character as usize..(t.character + t.length) as usize].iter().collect();
            (t.line + 1, s)
        })
        .collect()
}

#[test]
fn a_type_imported_from_another_file_is_highlighted() {
    // `store.typl` defines no types at all -- `todo-item` comes from
    // `model.typl` through `(use model::todo-item)`. This is precisely the case
    // an editor-local scan cannot cover.
    let (uses, tokens, text) = analyze("examples/projects/todo-cli/src/store.typl");
    assert!(
        uses.iter().any(|u| u.path.local() == "todo-item" && u.kind == TypeKind::Struct),
        "the imported type was never resolved as a use; recorded {:?}",
        uses.iter().map(|u| u.path.to_string()).collect::<Vec<_>>()
    );

    let found = spans(&text, &tokens);
    assert!(!found.is_empty(), "no type uses highlighted in store.typl");
    assert!(
        found.iter().all(|(_, s)| s == "todo-item"),
        "unexpected names highlighted: {:?}",
        found
    );
    // At minimum the `HashTable<i32,todo-item>` global and the
    // `Option<todo-item>` return type -- both inside generic arguments of a
    // single source token, which is where the in-token span arithmetic earns
    // its keep.
    assert!(found.len() >= 2, "expected several uses, got {:?}", found);
}

#[test]
fn a_type_named_only_in_a_comment_is_not_highlighted() {
    // `store.typl` opens with a comment mentioning `HashTable<i32,todo-item>`.
    // A recorded span can only come from a position the checker parsed as a
    // type, so nothing in a comment can ever be one — asserted anyway, since a
    // reader who sees prose coloured as code learns to distrust the colouring.
    let (_, tokens, text) = analyze("examples/projects/todo-cli/src/store.typl");
    let comment_line = text
        .lines()
        .position(|l| l.trim_start().starts_with(";;") && l.contains("todo-item"))
        .expect("the file starts with a comment naming the type") as u32;
    assert!(
        tokens.iter().all(|t| t.line != comment_line),
        "a type named in a comment was highlighted"
    );
}

#[test]
fn a_file_defining_its_own_types_still_works() {
    // The same-file case the editors already handled, now answered by the
    // server, so switching it on cannot lose anything: definition headers,
    // `impl` heads, annotations, `:dyn` trait objects.
    let (_, tokens, text) = analyze("examples/projects/shape-canvas/src/shapes.typl");
    let found = spans(&text, &tokens);
    // `Shape` is written capitalized; the reader folds it, but the span must
    // cover the token as written. Each name appears at its definition and at
    // several uses.
    for expected in ["rect", "circle", "hline", "Shape"] {
        assert!(
            found.iter().filter(|(_, s)| s == expected).count() > 1,
            "{} was found only once (definition but no uses): {:?}",
            expected,
            found
        );
    }
    // `canvas` reaches this file through `(use canvas::canvas)` — the
    // cross-file case again, this time alongside locally defined types.
    assert!(
        found.iter().any(|(_, s)| s == "canvas"),
        "the imported `canvas` type was not highlighted: {:?}",
        found
    );
}

#[test]
fn trait_and_struct_kinds_are_reported() {
    let (uses, _, _) = analyze("examples/projects/shape-canvas/src/shapes.typl");
    assert!(uses.iter().any(|u| u.path.local() == "shape" && u.kind == TypeKind::Trait));
    assert!(uses.iter().any(|u| u.path.local() == "rect" && u.kind == TypeKind::Struct));
}

#[test]
fn builtin_types_are_left_to_the_editor_grammar() {
    // `Option`, `Result`, `Vector`, `HashTable` and `Sexpr` are already
    // coloured by both editors' own rules. Emitting semantic tokens for them
    // would override a scope that was already right, so the recorder skips
    // every `builtin`-flagged definition.
    let (_, tokens, text) = analyze("examples/projects/todo-cli/src/store.typl");
    let found = spans(&text, &tokens);
    for builtin in ["option", "result", "vector", "hashtable", "sexpr"] {
        assert!(
            found.iter().all(|(_, s)| !s.eq_ignore_ascii_case(builtin)),
            "{} should be left to the grammar, but was reported: {:?}",
            builtin,
            found
        );
    }
}

#[test]
fn a_function_sharing_a_types_name_is_not_highlighted_at_its_call_sites() {
    // Type and function names live in different tables, so both `pad`s below
    // are legal. The old textual scan coloured *every* occurrence of the name
    // `pad`, including the function's definition and call — the documented
    // false positive this redesign exists to remove. Resolution-driven
    // recording only marks the two genuine type positions.
    let src = "\
(defstruct pad (n i32))
(defun pad ((x i32)) i32 x)
(defun use-fn () i32 (pad 3))
(defun use-ty ((p pad)) i32 p::n)
";
    // A path under a directory with no `typelisp.toml` above it, so the loader
    // treats it as a standalone entry (text is passed directly; the file need
    // not exist on disk).
    let path = PathBuf::from("/tmp/typelisp-semantic-collision-test/f.typl");
    let (_, tokens) = analyze_src(&path, src);
    let per_line: Vec<u32> = tokens.iter().map(|t| t.line + 1).collect();
    // Line 1: the `defstruct pad` header. Line 4: the `(p pad)` annotation.
    assert!(per_line.contains(&1), "definition header not highlighted: {:?}", tokens);
    assert!(per_line.contains(&4), "annotation not highlighted: {:?}", tokens);
    // Lines 2 and 3 name only the *function* `pad` — never a type.
    assert!(
        !per_line.contains(&2) && !per_line.contains(&3),
        "a function's name was highlighted as a type: {:?}",
        tokens
    );
}

#[test]
fn a_document_with_a_type_error_keeps_its_highlighting() {
    // The server checks in error-recovery mode, so a document mid-edit still
    // produces a tree -- and, with it, the type-name spans recorded up to and
    // past the failing form. Losing the colouring on every transient type
    // error (which is most of the time while typing) would be worse than the
    // problem this feature solves.
    let src = "\
(defstruct rect (w i32))
(defun bad ((r rect)) i32 \"not an int\")
(defun good ((r rect)) i32 r::w)
";
    let path = PathBuf::from("/tmp/typelisp-semantic-recover-test/f.typl");
    let (_, tokens) = analyze_src(&path, src);
    let lines: Vec<u32> = tokens.iter().map(|t| t.line + 1).collect();
    for line in [1, 2, 3] {
        assert!(
            lines.contains(&line),
            "line {} lost its type highlighting because of the error on line 2: {:?}",
            line,
            tokens
        );
    }
}

#[test]
fn a_static_method_call_still_names_the_type() {
    // `(rect::new 1)` — the type sits in the token's second-to-last segment;
    // exactly the `rect` part must be highlighted, not `::new`.
    let src = "\
(defstruct rect (w i32))
(defun make () rect (rect::new 5))
";
    let path = PathBuf::from("/tmp/typelisp-semantic-assoc-test/f.typl");
    let (_, tokens) = analyze_src(&path, src);
    let line2: Vec<&TypeToken> = tokens.iter().filter(|t| t.line == 1).collect();
    // The return annotation `rect` (0-based col 15) and the callee's `rect`
    // (inside `(rect::new`, 0-based col 21), both 4 chars.
    assert!(
        line2.iter().any(|t| t.character == 21 && t.length == 4),
        "the `rect` in `rect::new` was not highlighted: {:?}",
        tokens
    );
    assert!(
        line2.iter().any(|t| t.character == 15 && t.length == 4),
        "the return annotation was not highlighted: {:?}",
        tokens
    );
}

#[test]
fn the_wire_encoding_round_trips_to_absolute_positions() {
    // The LSP format stores deltas, so an off-by-one in the encoder shifts every
    // token after it. Decode back and compare against the spans themselves.
    let (_, tokens, _) = analyze("examples/projects/shape-canvas/src/shapes.typl");
    assert!(tokens.len() > 5, "need a few tokens to exercise the deltas");

    let data = encode(&tokens, |k| match k {
        TypeKind::Struct => 0,
        TypeKind::Enum => 1,
        TypeKind::Trait => 2,
    });
    assert_eq!(data.len(), tokens.len() * 5);

    let (mut line, mut col) = (0u32, 0u32);
    let mut decoded = Vec::new();
    for c in data.chunks_exact(5) {
        line += c[0];
        col = if c[0] == 0 { col + c[1] } else { c[1] };
        decoded.push((line, col, c[2]));
    }
    let expected: Vec<(u32, u32, u32)> =
        tokens.iter().map(|t| (t.line, t.character, t.length)).collect();
    assert_eq!(decoded, expected);
}
