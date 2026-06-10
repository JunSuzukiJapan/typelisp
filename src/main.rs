use std::error;
use typelisp::*;

/// Minimal runner: read one expression, build the AST, JIT-compile and run it.
fn main() -> Result<(), Box<dyn error::Error>> {
    let source = "(+ 1 2 3)";

    let reader = Reader::new();
    let constructor = ASTConstructor::new();

    let obj = reader.read(source)?;
    let expr = constructor.make_ast(&obj)?;
    let result = Compiler::compile_and_run(&expr)?;

    println!("{} => {:?}", source, result);
    Ok(())
}
