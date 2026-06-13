//! Minimal entry point.
//!
//! The native compiler (LLVM/inkwell) is opt-in behind the `compile` feature;
//! the default binary exercises only the LLVM-free read path. A real REPL
//! (with a `--heap-cells N` option to size the cons arena) lands later.

use typelisp::*;

fn main() -> Result<(), Error> {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let v = reader.read(&mut heap, "(defun factorial ((n i32)) i32 (if (<= n 1) 1 (* n (factorial (- n 1)))))")?;
    heap.push_root(v);
    println!("read ok: {} cons cells live", heap.live_count());
    Ok(())
}
