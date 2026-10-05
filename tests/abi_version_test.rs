//! The ABI version and its documents (`docs/dev/api_version/`).
//!
//! Three guards:
//!
//! * [`the_latest_document_describes_this_build`]: what compiled code assumes
//!   about the runtime archive (`compile::abi_signature::describe`) is what
//!   the newest document says, under the version `typelisp-abi` names. When
//!   it is not, `scripts/regen-abi-version.sh` writes the next version.
//! * [`every_version_keeps_its_document`]: no version's document is removed
//!   or edited once written.
//! * [`every_shared_constant_is_described`]: a constant added to one of the
//!   files compiled code and the runtime share numbers through, or a symbol
//!   the AOT entry sequence declares on its own, cannot be left out of the
//!   description and so change nothing the version answers to.

use std::path::{Path, PathBuf};

use typelisp::compile::abi_signature::{describe, fnv1a, history_file, parse_document, DOC_DIR, LATEST_FILE};

const REGEN: &str = "run scripts/regen-abi-version.sh";

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn doc_dir() -> PathBuf {
    root().join(DOC_DIR)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{} unreadable ({})", path.display(), e))
}

#[test]
fn the_latest_document_describes_this_build() {
    let latest = read(&doc_dir().join(LATEST_FILE));
    let (version, _, body) = parse_document(&latest).unwrap_or_else(|| panic!("{} is malformed — {}", LATEST_FILE, REGEN));
    assert_eq!(version, typelisp_abi::ABI_VERSION, "{} is version {}, typelisp-abi names {}", LATEST_FILE, version, typelisp_abi::ABI_VERSION);
    let description = describe();
    if body != description {
        let line = body.lines().zip(description.lines()).position(|(a, b)| a != b).map_or_else(
            || format!("the two differ in length ({} and {} lines)", body.lines().count(), description.lines().count()),
            |i| format!("first difference at line {} of the description", i + 1),
        );
        panic!("what compiled code assumes about the runtime archive changed since ABI version {}: {} — {}", version, line, REGEN);
    }
}

#[test]
fn every_version_keeps_its_document() {
    let dir = doc_dir();
    let newest = typelisp_abi::ABI_VERSION;
    for version in 1..=newest {
        let path = dir.join(history_file(version));
        let text = read(&path);
        let (named, hash, body) = parse_document(&text).unwrap_or_else(|| panic!("{} is malformed", path.display()));
        assert_eq!(named, version, "{} names version {}", path.display(), named);
        assert_eq!(hash, fnv1a(body), "{} was edited after it was written; a version's document never changes", path.display());
    }
    assert_eq!(
        read(&dir.join(LATEST_FILE)),
        read(&dir.join(history_file(newest))),
        "{} must be a copy of {}",
        LATEST_FILE,
        history_file(newest)
    );
    // Nothing newer than the version the source names: a document written
    // without the source following (or the other way round) is caught here.
    let history = dir.join("history");
    for entry in std::fs::read_dir(&history).unwrap_or_else(|e| panic!("{} unreadable ({})", history.display(), e)) {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let version: u32 = name
            .strip_prefix("api_")
            .and_then(|n| n.strip_suffix(".md"))
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("{} holds `{}`, which is not a version's document", history.display(), name));
        assert!((1..=newest).contains(&version), "{} is newer than typelisp-abi's version {}", name, newest);
    }
}

#[test]
fn every_shared_constant_is_described() {
    let description = describe();
    // The files compiled code and the runtime share numbers through, and the
    // constants in them that are not part of what is shared.
    let files: [(&str, &[&str]); 7] = [
        ("crates/typelisp-abi/src/lib.rs", &["ABI_VERSION", "ABI_SYMBOL"]),
        ("crates/typelisp-mem/src/tagged.rs", &[]),
        ("crates/typelisp-rt/src/c_mem.rs", &[]),
        ("crates/typelisp-rt/src/lib.rs", &[]),
        ("crates/typelisp-rt/src/sys_builtin.rs", &[]),
        ("crates/typelisp-rt/src/stream_builtin.rs", &[]),
        ("crates/typelisp-rt/src/net_builtin.rs", &[]),
    ];
    let mut missing = Vec::new();
    for (file, excluded) in files {
        for line in read(&root().join(file)).lines() {
            let Some(rest) = line.trim_start().strip_prefix("pub const ") else { continue };
            let name = rest.split(':').next().unwrap().trim();
            if excluded.contains(&name) {
                continue;
            }
            let stem = Path::new(file).file_stem().unwrap().to_str().unwrap();
            if !description.contains(&format!("| {} |", name)) && !description.contains(&format!("{}::{} |", stem, name)) {
                missing.push(format!("{} in {}", name, file));
            }
        }
    }
    // Symbols the AOT entry sequence declares itself rather than taking from
    // the shim table. The module's tests come after `mod tests`.
    let aot = read(&root().join("src/compile/aot.rs"));
    let aot = &aot[..aot.find("\nmod tests {").expect("aot.rs has a test module")];
    for piece in aot.split("add_function(\"").skip(1) {
        let name = &piece[..piece.find('"').unwrap()];
        if name.starts_with("rt_") && !description.contains(&format!("| `{}` |", name)) {
            missing.push(format!("{} declared in src/compile/aot.rs", name));
        }
    }
    assert!(
        missing.is_empty(),
        "not in compile::abi_signature::describe, so a change to them would not change the ABI version: {:?}",
        missing
    );
}
