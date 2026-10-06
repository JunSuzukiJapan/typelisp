//! Writes a new ABI version when what compiled code assumes about the runtime
//! archive has changed (`typelisp::compile::abi_signature`). Run through
//! `scripts/regen-abi-version.sh`; `tests/abi_version_test.rs` fails until it
//! has been.
//!
//! When the description differs from the newest document, or the typelisp
//! version's MAJOR.MINOR is no longer the ABI version's, this writes it as
//! `history/api_<next>.md` (`AbiVersion::next`), copies that to
//! `latest_api_signature.md`, and rewrites the numbers in `typelisp-abi`'s
//! `with_abi_version!`. The history file is written first and never
//! overwritten: a version's document is kept for good.
//!
//! `--bump` writes the next version even when the description is unchanged:
//! for a change the runtime has to agree with that no table shows (see
//! `abi_signature`'s module doc comment).
//!
//! Paths are resolved from `CARGO_MANIFEST_DIR`, never hardcoded.

use std::path::Path;

use typelisp::compile::abi_signature::{
    describe, document, history_file, parse_document, AbiVersion, DOC_DIR, LATEST_FILE,
};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join(DOC_DIR);
    let latest_path = dir.join(LATEST_FILE);
    let description = describe();
    let current = AbiVersion::current();
    let bump = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("--bump") => true,
        Some(other) => panic!("unknown argument `{}`; the only one is --bump", other),
    };

    let version = match std::fs::read_to_string(&latest_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // The first document there has ever been: it describes the
            // version the source already names.
            current
        }
        Err(e) => panic!("failed to read {}: {}", latest_path.display(), e),
        Ok(text) => {
            let (latest, _, body) = parse_document(&text)
                .unwrap_or_else(|| panic!("{} is not a document this tool wrote", latest_path.display()));
            assert_eq!(
                latest,
                current,
                "{} describes version {} but typelisp-abi names {}; resolve that by hand first",
                latest_path.display(),
                latest,
                current
            );
            if body == description && !bump && current.is_of_this_package() {
                println!("ABI version {} still describes the archive; nothing written", current);
                return;
            }
            current.next()
        }
    };

    let text = document(version, &description);
    let history_path = dir.join(history_file(version));
    match std::fs::read_to_string(&history_path) {
        Ok(existing) if existing == text => {}
        Ok(_) => panic!(
            "{} already exists with other contents; a version's document is never replaced",
            history_path.display()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = history_path.parent().expect("a history file has a folder");
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("failed to create {}: {}", parent.display(), e));
            std::fs::write(&history_path, &text)
                .unwrap_or_else(|e| panic!("failed to write {}: {}", history_path.display(), e));
        }
        Err(e) => panic!("failed to read {}: {}", history_path.display(), e),
    }
    std::fs::write(&latest_path, &text).unwrap_or_else(|e| panic!("failed to write {}: {}", latest_path.display(), e));

    if version != current {
        set_source_version(&root.join("crates/typelisp-abi/src/lib.rs"), current, version);
    }
    println!("wrote ABI version {} ({})", version, history_path.display());
}

/// Rewrites `with_abi_version!`'s numbers in `lib` from `from` to `to`.
fn set_source_version(lib: &Path, from: AbiVersion, to: AbiVersion) {
    let source = std::fs::read_to_string(lib).unwrap_or_else(|e| panic!("failed to read {}: {}", lib.display(), e));
    let line = |v: AbiVersion| {
        format!("macro_rules! with_abi_version {{ ($then:ident) => {{ $then!({}, {}, {}) }}; }}", v.major, v.minor, v.patch)
    };
    let (old, new) = (line(from), line(to));
    assert_eq!(source.matches(&old).count(), 1, "{} does not contain `{}` exactly once", lib.display(), old);
    std::fs::write(lib, source.replace(&old, &new)).unwrap_or_else(|e| panic!("failed to write {}: {}", lib.display(), e));
}
