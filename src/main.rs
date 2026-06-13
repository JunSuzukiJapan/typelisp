//! Minimal entry point.
//!
//! The native compiler (LLVM/inkwell) is opt-in behind the `compile` feature;
//! the default binary exercises only the LLVM-free read path. A real REPL
//! (with a `--heap-cells N` option to size the cons arena) lands later.

use typelisp::*;

fn main() -> Result<(), Error> {
    let reader = Reader::new();
    let obj = reader.read("(+ 1 2)")?;
    println!("{:?}", obj);
    Ok(())
}
