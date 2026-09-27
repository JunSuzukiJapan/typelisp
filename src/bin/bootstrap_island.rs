//! Regenerates the committed compiler-island AOT artifact
//! (`src/compiler_island.typld`) — interp-closure removal Stage 3. Run through
//! `scripts/regen-compiler-island.sh` (which supplies the LLVM environment)
//! whenever `compiler.rs`'s `SOURCE` changes; the
//! `island_artifacts_are_fresh` test fails until this is re-run.
//!
//! Also re-run it after changing an `llvm-*` builder
//! (`eval_llvm_builtin_method`), which changes the emitted IR without
//! changing `SOURCE`. The freshness *hash* covers `SOURCE` only and will not
//! notice; `the_committed_island_matches_a_fresh_build` compares the built
//! bytes and will. See `compile::bootstrap`'s module doc comment.
//!
//! Paths are resolved from `CARGO_MANIFEST_DIR` (this crate's own root),
//! never hardcoded — see the project's policy on machine-specific absolute
//! paths.

use std::path::Path;

fn main() {
    // This ran on a thread with an explicit 64MB stack until 2026-09-08.
    // Building the dump drives `check::freevars`'s walk over `compile-function`'s
    // huge `labels` body, which checks and runs the self-hosted compiler, and
    // the evaluator recursed once per `if`-nesting level along the way. That
    // half now runs on a continuation stack in the heap
    // (`crates/typelisp-front/src/eval/interp/core_cps.rs`), so the Rust stack
    // stays flat. Measured: with the work moved onto a 2MB thread this still
    // wrote a byte-identical artifact, so `main`'s own stack is more room than
    // it needs and the thread has no reason to exist.
    let dump = typelisp::compile::bootstrap::build_island_artifact()
        .unwrap_or_else(|e| panic!("failed to build the compiler-island dump: {}", e));

    let root = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(root).join("src").join("compiler_island.typld");

    std::fs::write(&path, &dump).unwrap_or_else(|e| panic!("failed to write {}: {}", path.display(), e));

    println!("wrote {} ({} bytes)", path.display(), dump.len());
}
