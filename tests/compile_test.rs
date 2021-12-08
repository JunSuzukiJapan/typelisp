extern crate typelisp;

use typelisp::*;

mod tests {
    use crate::*;

    #[test]
    fn eval_int_test() -> Result<(), Error> {
        let reader = Reader::new();
        let constructor = ASTConstructor::new();
        let mut env = Env::new();

        let obj = reader.read("123")?;
        let expr = constructor.make_ast(&obj)?;
        assert_eq!(expr, Expr::Int(123));

        let mut compiler = Compiler::new();
        let result = compiler.compile(&expr, &mut env);

        Ok(())
    }
}