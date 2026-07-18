//! Regenerates the committed compiler-island AOT artifacts
//! (`src/compiler_island.bc` + `src/compiler_island.fasl`) — interp-closure
//! removal Stage 3. Run through `scripts/regen-compiler-island.sh` (which
//! supplies the LLVM environment) whenever `compiler.rs`'s `SOURCE` changes;
//! the `island_artifacts_are_fresh` test fails until this is re-run.
//!
//! Paths are resolved from `CARGO_MANIFEST_DIR` (this crate's own root),
//! never hardcoded — see the project's policy on machine-specific absolute
//! paths.

use std::path::Path;

fn main() {
    // Building the bitcode drives `freevars::walk` over `compile-function`'s
    // huge `labels` body — the same deep recursion the compile tests need
    // `RUST_MIN_STACK=32MB` for. `RUST_MIN_STACK` only sizes *spawned*
    // threads, not `main`, so run the work on a thread with an explicit
    // large stack (matching `scripts/test-serial.sh`'s 32MB, doubled for
    // headroom).
    let bitcode = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            typelisp::compile::bootstrap::build_island_bitcode()
                .unwrap_or_else(|e| panic!("failed to build compiler-island bitcode: {}", e))
        })
        .expect("failed to spawn bootstrap thread")
        .join()
        .expect("bootstrap thread panicked");

    let root = env!("CARGO_MANIFEST_DIR");
    let bc_path = Path::new(root).join("src").join("compiler_island.bc");

    std::fs::write(&bc_path, &bitcode).unwrap_or_else(|e| panic!("failed to write {}: {}", bc_path.display(), e));

    println!("wrote {} ({} bytes)", bc_path.display(), bitcode.len());
}
