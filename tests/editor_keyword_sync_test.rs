//! Keeps the editor definitions honest against the implementation.
//!
//! `editor/emacs/typelisp-mode.el` and `editor/vscode/syntaxes/typelisp.tmLanguage.json`
//! each carry a list of every builtin function, method, type and trait the
//! language offers. Those lists are hand-maintained and live outside the
//! compiler, so nothing stops them from quietly falling behind — which is
//! exactly what happened once already: the Emacs mode went months without
//! `bignum`, `ratio`, `format`, `print`/`println`, the `pprint` family or the
//! four builtin error types, and still listed an `inf` constant the language
//! never had.
//!
//! This test closes that loop. It loads the prelude for real, walks the
//! resulting [`Registry`] — the same environment the checker uses — and reports
//! any user-facing name that neither editor knows about. Deriving the truth
//! from the registry rather than by grepping source means a new builtin is
//! caught the moment it is registered, whichever layer defines it.
//!
//! Special forms live in `Checker::check_list`'s dispatch, which has no runtime
//! representation to enumerate, so those are read out of the source between two
//! sentinel comments (see [`special_forms_from_checker`]).
//!
//! When this test fails, add the reported names to **both** editor definitions.

extern crate typelisp;

use std::collections::{BTreeSet, HashSet};
use std::path::{Path as FsPath, PathBuf};

use typelisp::check::registry::Namespace;
use typelisp::{load_prelude, Checker, Heap, Interp};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------- the truth

/// Every user-facing name the prelude-loaded registry offers: public free
/// functions, macros, global variables, types, traits, and the associated
/// functions and trait methods reachable on them, across every namespace.
fn registry_names() -> BTreeSet<String> {
    collected().0
}

/// The field names of every registered type, and the `set-` setters that go
/// with them. A `defstruct` generates an accessor and a setter per field, so
/// these names exist in the registry without being catalog entries: `pos`,
/// `vec` and `snapshot` are fields of the built-in iterator structs, and no
/// editor should paint every occurrence of the word "pos" as a builtin.
fn field_accessors() -> BTreeSet<String> {
    collected().1
}

fn collected() -> (BTreeSet<String>, BTreeSet<String>) {
    let mut heap = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut chk, &mut interp);

    let mut names = BTreeSet::new();
    let mut fields = BTreeSet::new();
    collect_namespace(&chk.registry().root, &mut names, &mut fields);
    (names, fields)
}

fn collect_namespace(ns: &Namespace, out: &mut BTreeSet<String>, fields: &mut BTreeSet<String>) {
    // Everything, without filtering on `public`. That flag answers "is this
    // visible outside its defining module", which is a different question: the
    // prelude defines `unwrap`, `map`, `filter` and `car` at the root without a
    // `pub` marker, since the ancestor-chain rule already makes root visible
    // everywhere. Filtering on it silently dropped 52 of the 214 registered
    // names — including most of the catalog this test exists to guard.
    out.extend(ns.fns.keys().cloned());
    out.extend(ns.macros.keys().cloned());
    out.extend(ns.vars.keys().cloned());
    for (name, def) in &ns.types {
        out.insert(name.clone());
        out.extend(def.assoc.keys().cloned());
        for field in &def.field_names {
            fields.insert(field.clone());
            fields.insert(format!("set-{field}"));
        }
    }
    for (name, def) in &ns.traits {
        out.insert(name.clone());
        out.extend(def.methods.keys().cloned());
    }
    for child in ns.modules.values() {
        collect_namespace(child, out, fields);
    }
}

/// Names an editor is not expected to list, by category rather than one by one
/// — a flat ignore list would rot the same way the keyword lists did.
fn is_excluded(name: &str) -> bool {
    // The self-hosted compiler island's LLVM binding surface. It is typelisp
    // code, but it is this repository's own compiler internals rather than
    // anything a user writes, and listing ~90 `build-*` names would drown the
    // real catalog.
    const ISLAND_PREFIXES: [&str; 6] =
        ["build-", "llvm-", "const-", "load-", "store-", "position-"];
    const ISLAND_EXACT: [&str; 14] = [
        "add-function",
        "add-function-with-env",
        "alloca-args",
        "append-block",
        "block-terminated?",
        "clone-frames",
        "create",
        "get-function",
        "pop-frame",
        "push-frame",
        "scope",
        "to-string",
        "verify",
        "declare-external-function",
    ];
    // Internal helpers a special form expands into (`print` -> `print-rt`), and
    // the arithmetic/comparison operators, which are punctuation rather than
    // words and are highlighted structurally, not by name.
    let operator = !name.chars().any(|c| c.is_ascii_alphabetic());
    // Single-letter names are generic type parameters (`t`, `k`, `v`).
    let type_param = name.len() == 1;
    // Earmuffed globals (`*print-pretty*` and a user's own) are matched by a
    // pattern in both editors rather than being listed by name, precisely so a
    // user's own special variable gets the same treatment.
    let earmuffed = name.len() > 2 && name.starts_with('*') && name.ends_with('*');

    // The native stream layer (`check::registry::register_stream_builtins`).
    // These take an opaque `i64` handle and exist only for the prelude's
    // trait implementations to call; a user writes `read-char`/`write-string`
    // on a stream value and never names one of these.
    let native_stream = name.starts_with("stream-") || name.starts_with("file-");
    // Prelude-private helpers with no `pub`: `unwrap-io` turns a native
    // `Result` into a panic, `io-ok` pins an error type.
    const PRELUDE_PRIVATE: [&str; 2] = ["unwrap-io", "io-ok"];

    operator
        || type_param
        || earmuffed
        || native_stream
        || PRELUDE_PRIVATE.contains(&name)
        || name.ends_with("-rt")
        || ISLAND_PREFIXES.iter().any(|p| name.starts_with(p))
        || ISLAND_EXACT.contains(&name)
        || field_accessors().contains(name)
}

// ---------------------------------------------------------------- the editors

/// Every string literal inside the `defconst typelisp-...` keyword tables of
/// the Emacs mode.
fn emacs_names() -> BTreeSet<String> {
    let path = repo_root().join("editor/emacs/typelisp-mode.el");
    let src = std::fs::read_to_string(&path).expect("emacs mode is readable");
    // The keyword tables sit between the "Keyword tables" heading and the
    // "Font lock" one; taking only that slice keeps regexps and doc strings
    // further down the file from being mistaken for keywords.
    let start = src.find(";;; Keyword tables").expect("keyword tables section");
    let end = src.find(";;; Font lock").expect("font lock section");
    string_literals(&src[start..end])
}

/// Every literal alternative in the `match` regexps of the TextMate grammar.
///
/// The grammar spells its keyword tables as alternations built by `regexp-opt`'s
/// counterpart, so recovering the words means undoing the regex escaping rather
/// than parsing a list.
fn vscode_names() -> BTreeSet<String> {
    let path = repo_root().join("editor/vscode/syntaxes/typelisp.tmLanguage.json");
    let src = std::fs::read_to_string(&path).expect("grammar is readable");
    let mut out = BTreeSet::new();
    for raw in json_string_values(&src, "match") {
        for word in alternation_words(&raw) {
            out.insert(word);
        }
    }
    out
}

/// The literal alternatives of a keyword-alternation regex.
///
/// The lookaround assertions that bracket every such rule contain a character
/// class of every symbol character, so splitting on `|` naively glues that
/// class onto the first and last alternative. Stripping the assertions first is
/// what keeps `pprint-logical-block` (the longest, hence first, special form)
/// from reading as a garbage word.
fn alternation_words(regex: &str) -> Vec<String> {
    let chars: Vec<char> = regex.chars().collect();
    let mut body = String::new();
    let mut i = 0;
    while i < chars.len() {
        // Skip `(?<!...)` and `(?!...)` wholesale, tracking `[...]` so a `)`
        // inside a character class does not end the group early.
        let is_lookbehind = chars[i..].starts_with(&['(', '?', '<', '!']);
        let is_lookahead = chars[i..].starts_with(&['(', '?', '!']);
        if is_lookbehind || is_lookahead {
            let mut depth = 0usize;
            let mut in_class = false;
            while i < chars.len() {
                match chars[i] {
                    '\\' => i += 1,
                    '[' if !in_class => in_class = true,
                    ']' if in_class => in_class = false,
                    '(' if !in_class => depth += 1,
                    ')' if !in_class => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        // Grouping punctuation is not part of any alternative. `(?:` in
        // particular would otherwise leave a `?` glued to the first word, since
        // `?` is a legitimate symbol character in typelisp.
        if chars[i..].starts_with(&['(', '?', ':']) {
            i += 3;
            continue;
        }
        if chars[i] == '(' || chars[i] == ')' {
            i += 1;
            continue;
        }
        body.push(chars[i]);
        i += 1;
    }

    body.split('|')
        .map(|piece| {
            // Undo the regex escaping and drop the grouping punctuation.
            piece
                .replace("\\\\", "\x00")
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || "-_*?!<>=/+.".contains(*c))
                .filter(|c| *c != '\x00')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// The special forms `Checker::check_list` dispatches on, read from the source
/// between the sentinel comments that bracket the dispatch.
///
/// Panics when the sentinels are missing, so a refactor that moves the dispatch
/// makes this test fail loudly rather than silently checking nothing.
fn special_forms_from_checker() -> BTreeSet<String> {
    let path = repo_root().join("src/check/checker.rs");
    let src = std::fs::read_to_string(&path).expect("checker is readable");
    let begin = src
        .find("// SPECIAL-FORM DISPATCH BEGIN")
        .expect("special-form dispatch begin sentinel (see this test's doc comment)");
    let end = src
        .find("// SPECIAL-FORM DISPATCH END")
        .expect("special-form dispatch end sentinel (see this test's doc comment)");
    assert!(begin < end, "dispatch sentinels are out of order");

    let mut out = BTreeSet::new();
    for line in src[begin..end].lines() {
        let trimmed = line.trim_start();
        // Only match-arm patterns: `"name" =>` or `"a" | "b" =>`.
        if trimmed.starts_with("//") || !trimmed.contains("=>") {
            continue;
        }
        let head = &trimmed[..trimmed.find("=>").unwrap()];
        out.extend(string_literals(head));
    }
    out
}

// ---------------------------------------------------------------- helpers

/// Every `"..."` literal in `text`, without interpreting escapes.
fn string_literals(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != '"' {
                if bytes[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            let literal: String = bytes[start..j.min(bytes.len())].iter().collect();
            if !literal.is_empty() {
                out.insert(literal);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The values of every `"key": "value"` pair in a JSON document, for one key.
fn json_string_values(src: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{}\":", key);
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find(&needle) {
        rest = &rest[at + needle.len()..];
        let Some(open) = rest.find('"') else { break };
        let tail = &rest[open + 1..];
        let mut value = String::new();
        let mut chars = tail.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                value.push(c);
                if let Some(next) = chars.next() {
                    value.push(next);
                }
            } else if c == '"' {
                break;
            } else {
                value.push(c);
            }
        }
        out.push(value);
    }
    out
}

/// Case-folded lookup set.
///
/// The reader lowercases every symbol, so the registry keys a type as `option`
/// while both editors — and the documentation, and every example — write
/// `Option`. Comparing case-sensitively would report each builtin type as
/// missing while it sits right there in the list.
fn folded(names: &BTreeSet<String>) -> HashSet<String> {
    names.iter().map(|n| n.to_ascii_lowercase()).collect()
}

fn report(missing: &BTreeSet<String>, editor: &str, file: &str) -> String {
    let list: Vec<&str> = missing.iter().map(String::as_str).collect();
    format!(
        "{} names the implementation offers are missing from the {} definitions ({}):\n  {}\n\n\
         Add them there, and to the other editor's definitions too. If a name is \
         deliberately not user-facing, extend `is_excluded` in this test with the \
         category it belongs to rather than listing the name on its own.",
        missing.len(),
        editor,
        file,
        list.join(" ")
    )
}

// ---------------------------------------------------------------- the tests

#[test]
fn emacs_mode_knows_every_registry_name() {
    let known = folded(&emacs_names());
    let missing: BTreeSet<String> = registry_names()
        .into_iter()
        .filter(|n| !is_excluded(n) && !known.contains(&n.to_ascii_lowercase()))
        .collect();
    assert!(
        missing.is_empty(),
        "{}",
        report(&missing, "Emacs mode", "editor/emacs/typelisp-mode.el")
    );
}

#[test]
fn vscode_grammar_knows_every_registry_name() {
    let known = folded(&vscode_names());
    let missing: BTreeSet<String> = registry_names()
        .into_iter()
        .filter(|n| !is_excluded(n) && !known.contains(&n.to_ascii_lowercase()))
        .collect();
    assert!(
        missing.is_empty(),
        "{}",
        report(
            &missing,
            "VS Code grammar",
            "editor/vscode/syntaxes/typelisp.tmLanguage.json"
        )
    );
}

#[test]
fn both_editors_know_every_special_form() {
    let forms = special_forms_from_checker();
    assert!(
        forms.len() > 20,
        "only {} special forms found — the dispatch sentinels probably no longer \
         bracket the match arms",
        forms.len()
    );

    let emacs = folded(&emacs_names());
    let vscode = folded(&vscode_names());
    let mut missing = Vec::new();
    for form in &forms {
        // `:dyn` is a type-position keyword with its own dedicated rule in both
        // editors rather than a place in the special-form list.
        if form == ":dyn" || is_excluded(form) {
            continue;
        }
        let key = form.to_ascii_lowercase();
        if !emacs.contains(&key) {
            missing.push(format!("{form} (Emacs)"));
        }
        if !vscode.contains(&key) {
            missing.push(format!("{form} (VS Code)"));
        }
    }
    assert!(
        missing.is_empty(),
        "special forms dispatched by Checker::check_list but unknown to an editor:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn the_two_editors_agree_with_each_other() {
    // Both are generated from the same source list, so a name in one and not
    // the other means somebody edited one file by hand. Only names the
    // *implementation* has are compared, so the grammar's own scope names and
    // the Emacs mode's helper strings do not enter into it.
    let emacs = folded(&emacs_names());
    let vscode = folded(&vscode_names());
    let real: BTreeSet<String> = registry_names()
        .into_iter()
        .chain(special_forms_from_checker())
        .filter(|n| !is_excluded(n))
        .map(|n| n.to_ascii_lowercase())
        .collect();

    let only_emacs: BTreeSet<&String> =
        real.iter().filter(|n| emacs.contains(*n) && !vscode.contains(*n)).collect();
    let only_vscode: BTreeSet<&String> =
        real.iter().filter(|n| vscode.contains(*n) && !emacs.contains(*n)).collect();
    assert!(
        only_emacs.is_empty() && only_vscode.is_empty(),
        "the editor definitions disagree:\n  only in Emacs:   {:?}\n  only in VS Code: {:?}",
        only_emacs,
        only_vscode
    );
}

/// The token-type legend `src/bin/lsp.rs` advertises, read out of its
/// `SEMANTIC_TOKEN_TYPES` constant in declaration order. `SemanticTokenType::
/// STRUCT` and friends serialize as their lowercase name, which is what a
/// client sees on the wire and what both editor definitions key on.
fn lsp_semantic_legend() -> Vec<String> {
    let src = std::fs::read_to_string(repo_root().join("src/bin/lsp.rs")).expect("lsp.rs is readable");
    let decl = src
        .find("const SEMANTIC_TOKEN_TYPES")
        .expect("lsp.rs declares SEMANTIC_TOKEN_TYPES");
    // Past the `=`, so the `[SemanticTokenType; 3]` type annotation's own
    // brackets are not mistaken for the value's.
    let eq = src[decl..].find('=').expect("legend constant has an initializer") + decl;
    let body_start = src[eq..].find('[').expect("legend is an array literal") + eq;
    let body_end = src[body_start..].find(']').expect("legend array is closed") + body_start;
    src[body_start..body_end]
        .split(',')
        .filter_map(|item| item.trim().rsplit("::").next())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_lowercase())
        .collect()
}

#[test]
fn both_editors_agree_with_the_servers_semantic_token_legend() {
    // Each editor maps a token *type name* onto a face/colour. The VS Code
    // extension additionally has to declare the legend in the same order the
    // server does, since its own fallback provider shares that legend and the
    // wire format indexes into it. A silent mismatch would mis-colour every
    // token rather than fail loudly, so pin both here.
    let legend = lsp_semantic_legend();
    assert_eq!(
        legend,
        vec!["struct", "enum", "interface"],
        "the server's legend changed; update both editor definitions and this expectation"
    );

    let el = std::fs::read_to_string(repo_root().join("editor/emacs/typelisp-mode.el"))
        .expect("emacs mode is readable");
    for name in &legend {
        assert!(
            el.contains(&format!("(\"{}\" . font-lock-", name)),
            "the Emacs mode has no face for the `{}` semantic token type",
            name
        );
    }

    let ts = std::fs::read_to_string(repo_root().join("editor/vscode/src/extension.ts"))
        .expect("extension source is readable");
    let declared = ts
        .split("const SEMANTIC_TOKEN_TYPES")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("the extension declares SEMANTIC_TOKEN_TYPES");
    let ts_legend: Vec<String> = string_literals_in_order(declared);
    assert_eq!(
        ts_legend, legend,
        "the VS Code extension's legend does not match the server's, in order"
    );
}

/// Every `"..."` literal in `text`, in source order (unlike
/// [`string_literals`], which dedupes and sorts).
fn string_literals_in_order(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && chars[j] != '"' {
                j += 1;
            }
            if j < chars.len() {
                out.push(chars[start..j].iter().collect());
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn the_editor_definitions_exist_where_the_tests_expect_them() {
    // A cheap guard so that moving or renaming an editor file fails here rather
    // than making the checks above silently vacuous.
    for rel in [
        "editor/emacs/typelisp-mode.el",
        "editor/vscode/syntaxes/typelisp.tmLanguage.json",
        "editor/vscode/package.json",
    ] {
        let path: PathBuf = repo_root().join(rel);
        assert!(FsPath::new(&path).is_file(), "missing editor file: {}", rel);
    }
}
