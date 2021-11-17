extern crate typelisp;

use typelisp::*;

mod tests {
    use crate::*;

    #[test]
    fn eval_int_test() -> Result<(), Error> {
        let reader = Reader::new();
        let evaluator = Evaluator::new();
        let obj = reader.read("123")?;
        let obj = evaluator.eval(&obj)?;

        assert_eq!(obj, Object::Int(123));

        Ok(())
    }

    #[test]
    fn eval_bool_test() -> Result<(), Error> {
        let reader = Reader::new();
        let evaluator = Evaluator::new();
        let obj = reader.read("true")?;
        let obj = evaluator.eval(&obj)?;
        assert_eq!(obj, Object::True);

        let reader = Reader::new();
        let evaluator = Evaluator::new();
        let obj = reader.read("false")?;
        let obj = evaluator.eval(&obj)?;
        assert_eq!(obj, Object::False);

        Ok(())
    }

    #[test]
    fn eval_symbol_test() -> Result<(), Error> {
        let reader = Reader::new();
        let evaluator = Evaluator::new();
        let obj = reader.read("some_symbol")?;
        let obj = evaluator.eval(&obj)?;
        assert_eq!(obj, Object::Symbol("some_symbol".to_string()));

        Ok(())
    }

    #[test]
    fn eval_string_test() -> Result<(), Error> {
        let reader = Reader::new();
        let evaluator = Evaluator::new();
        let obj = reader.read("\"Hello, World!\"")?;
        let obj = evaluator.eval(&obj)?;
        assert_eq!(obj, Object::String("Hello, World!".to_string()));

        Ok(())
    }

    #[test]
    fn eval_null_test() -> Result<(), Error> {
        let reader = Reader::new();
        let evaluator = Evaluator::new();

        let obj = reader.read("null")?;
        let obj = evaluator.eval(&obj)?;
        assert_eq!(obj, Object::Null);

        let obj = reader.read("()")?;
        let obj = evaluator.eval(&obj)?;
        assert_eq!(obj, Object::Null);

        Ok(())
    }
/*
    #[test]
    fn eval_add_test() -> Result<(), Error> {
        let reader = Reader::new();
        let evaluator = Evaluator::new();
        let obj = reader.read("(+ 1 2)")?;
        let obj = evaluator.eval(&obj)?;
        assert_eq!(obj, Object::Int(3));

        Ok(())
    }
*/
}