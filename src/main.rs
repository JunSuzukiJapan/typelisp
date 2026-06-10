use std::error;
use typelisp::*;

/// Minimal runner: read a program, type-check it, JIT-compile and run it,
/// printing the value of the last top-level expression.
fn main() -> Result<(), Box<dyn error::Error>> {
    let source = "(defun fact ((n i64)) i64 \
                    (if (<= n 1) 1 (* n (fact (- n 1))))) \
                  (fact 10)";

    let objs = Reader::new().read_all(source)?;
    let forms = ASTConstructor::new().make_program(&objs)?;
    let checked = Checker::new().check_program(forms)?;
    let result = Compiler::run_program(&checked.forms)?;

    println!("(fact 10) => {}", result);
    Ok(())
}
