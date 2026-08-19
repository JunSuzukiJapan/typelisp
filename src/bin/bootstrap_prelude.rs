//! Regenerates the committed prelude dump
//! (`crates/typelisp-front/src/prelude.typld`, checked state + bitcode). Run
//! through `scripts/regen-prelude-bitcode.sh`
//! (which supplies the LLVM environment) whenever `prelude.rs`'s `SOURCE`
//! changes; the `prelude_artifacts_are_fresh` test fails until this is re-run.
//!
//! Also re-run it after anything that changes the emitted IR without changing
//! that `SOURCE`: an `llvm-*` builder (`eval_llvm_builtin_method`), the
//! compile bridge, or `compiler.rs`'s `SOURCE` — the prelude is compiled *by*
//! the island, so a regenerated island produces a different prelude artifact.
//! The freshness *hash* covers `prelude.rs`'s `SOURCE` only and will not
//! notice; `the_committed_prelude_matches_a_fresh_build` compares the built
//! bytes and will.
//!
//! Paths are resolved from `CARGO_MANIFEST_DIR` (this crate's own root),
//! never hardcoded — see the project's policy on machine-specific absolute
//! paths.

use std::path::Path;

fn main() {
    // Same reason as `bootstrap_island.rs`: building this drives the island's
    // deeply recursive compile pipeline, which needs far more than the default
    // main-thread stack. `RUST_MIN_STACK` sizes only *spawned* threads, so the
    // work runs on one with an explicit 64MB stack.
    let dump = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            typelisp::compile::prelude_bootstrap::build_prelude_artifact()
                .unwrap_or_else(|e| panic!("failed to build the prelude dump: {}", e))
        })
        .expect("failed to spawn bootstrap thread")
        .join()
        .expect("bootstrap thread panicked");

    let root = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(root).join("crates").join("typelisp-front").join("src").join("prelude.typld");

    std::fs::write(&path, &dump).unwrap_or_else(|e| panic!("failed to write {}: {}", path.display(), e));

    println!("wrote {} ({} bytes)", path.display(), dump.len());
}
