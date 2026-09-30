//! Every free function, macro and global the standard library leaves at the
//! root is named in the reference (`docs/ja/reference/`).
//!
//! The root is an ancestor of every module, so whatever sits there is in every
//! program's scope, and a user's own definition of the same name at the REPL
//! replaces it. A helper the library needs but a user never writes belongs in
//! the internal module (`typelisp::INTERNAL_MODULE`) instead. This test is
//! what notices a new one left at the root: it either gets documented or
//! moves.

extern crate typelisp;

mod common;
use common::repo_root;

use std::collections::BTreeSet;
use std::path::Path as FsPath;

use typelisp::{load_prelude, Checker, Heap, Interp};

/// The root's own free functions, macros and globals once the prelude is in.
fn root_names() -> BTreeSet<String> {
    let mut heap = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut chk, &mut interp);
    let root = &chk.registry().root;
    root.fns.keys().chain(root.macros.keys()).chain(root.vars.keys()).cloned().collect()
}

fn markdown_under(dir: &FsPath, out: &mut String) {
    for entry in std::fs::read_dir(dir).expect("readable directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            markdown_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push_str(&std::fs::read_to_string(&path).expect("readable document"));
            out.push('\n');
        }
    }
}

/// A character that can be part of a name. Names carry `-`, `*`, `>`, `?`
/// and the like, so a word boundary is anything that cannot.
fn name_char(c: char) -> bool {
    c.is_alphanumeric() || "-*+/<>=!?%&$^~_.".contains(c)
}

/// Every name-shaped word in `text`.
fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !name_char(c)).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// The names the reference writes as a range rather than one by one.
fn ranges(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if text.contains("`first`…`tenth`") {
        for n in ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"] {
            out.insert(n.to_string());
        }
    }
    if text.contains("`caar`…`cddddr`") {
        // Every `c[ad]{2,4}r`.
        let mut middles = vec![String::new()];
        for len in 1..=4 {
            middles = middles.iter().flat_map(|m| ["a", "d"].map(|c| format!("{m}{c}"))).collect();
            if len >= 2 {
                for m in &middles {
                    out.insert(format!("c{m}r"));
                }
            }
        }
    }
    out
}

#[test]
fn every_root_name_is_in_the_reference() {
    let mut text = String::new();
    markdown_under(&repo_root().join("docs/ja/reference"), &mut text);
    let documented: BTreeSet<String> = words(&text).into_iter().chain(ranges(&text)).collect();

    let names = root_names();
    assert!(names.len() > 300, "expected the prelude's root catalog, found {} names", names.len());
    let missing: Vec<String> = names.into_iter().filter(|n| !documented.contains(n)).collect();
    assert!(
        missing.is_empty(),
        "names at the root that docs/ja/reference/ never mentions -- document each one, or move it into `{}` \
         if no user writes it:\n  {}",
        typelisp::INTERNAL_MODULE,
        missing.join(" ")
    );
}

#[test]
fn the_ranges_the_reference_writes_expand_as_expected() {
    let text = "`first`…`tenth` and `caar`…`cddddr`";
    let names = ranges(text);
    assert_eq!(names.len(), 10 + 28);
    assert!(names.contains("tenth") && names.contains("cadr") && names.contains("cddddr"));
}
