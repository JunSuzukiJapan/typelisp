//! The atomics and `Arc`/`Weak` of the runtime's crates come from
//! `typelisp_mem::sync`, which picks the core's own or, on a core without
//! compare-and-swap (Cortex-M0+), `portable-atomic`'s. One that named
//! `core::sync::atomic` or `alloc::sync` itself would build everywhere but
//! there.

use std::path::Path;

/// The crates a board's executable links that depend on `typelisp-mem`.
/// (`typelisp-rt-semihosting` does not; its one atomic is only loaded and
/// stored, which every core has.)
const CRATES: [&str; 6] = ["typelisp-mem", "typelisp-abi", "typelisp-print", "typelisp-read", "typelisp-rt", "typelisp-rt-embedded"];

fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_the_sync_module_names_the_core_s_atomics() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates");
    let door = root.join("typelisp-mem").join("src").join("sync.rs");
    let mut found = Vec::new();
    for krate in CRATES {
        let mut files = Vec::new();
        sources(&root.join(krate).join("src"), &mut files);
        for file in files.into_iter().filter(|f| *f != door) {
            for (i, line) in std::fs::read_to_string(&file).unwrap().lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if ["core::sync::atomic", "alloc::sync::", "std::sync::atomic", "std::sync::Arc", "std::sync::Weak"].iter().any(|p| code.contains(p)) {
                    found.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
                }
            }
        }
    }
    assert!(found.is_empty(), "use typelisp_mem::sync instead:\n{}", found.join("\n"));
}
