extern crate typelisp;

use std::error;
use typelisp::*;

mod tests {
    use crate::*;

    #[test]
    fn compile_int_test() -> Result<(), Box<dyn error::Error>> {
        let reader = Reader::new();
        let constructor = ASTConstructor::new();

        let obj = reader.read("123")?;
        let expr = constructor.make_expr(&obj)?;
        assert_eq!(expr.kind, ExprKind::Int(123, None));

        let result = Compiler::compile_and_run(&expr)?;
        assert_eq!(result, 123);

        Ok(())
    }

    #[test]
    fn compile_add_test() -> Result<(), Box<dyn error::Error>> {
        let reader = Reader::new();
        let constructor = ASTConstructor::new();

        let obj = reader.read("(+ 1 2 3)")?;
        let expr = constructor.make_expr(&obj)?;

        // (+ 1 2 3) folds left into nested BinOp::Add nodes.
        let result = Compiler::compile_and_run(&expr)?;
        assert_eq!(result, 6);

        Ok(())
    }

    #[test]
    fn compile_arith_test() -> Result<(), Box<dyn error::Error>> {
        let reader = Reader::new();
        let constructor = ASTConstructor::new();

        let cases = [("(- 10 3)", 7), ("(* 4 5)", 20), ("(/ 20 4)", 5), ("(% 17 5)", 2)];
        for (src, expected) in cases {
            let obj = reader.read(src)?;
            let expr = constructor.make_expr(&obj)?;
            assert_eq!(Compiler::compile_and_run(&expr)?, expected, "src = {}", src);
        }
        Ok(())
    }
}
