extern crate typelisp;

use std::error;
use typelisp::*;

mod tests {
    use crate::*;

    #[test]
    fn eval_int_test() -> Result<(), Box<dyn error::Error>> {
        let reader = Reader::new();
        let constructor = ASTConstructor::new();
        let mut env = Env::new();

        let obj = reader.read("123")?;
        let expr = constructor.make_ast(&obj)?;
        assert_eq!(expr, Expr::Int(123));

        let result = Compiler::compile_and_run(&expr, &mut env)?;
        assert_eq!(result, Expr::Int(123));

        Ok(())
    }
}