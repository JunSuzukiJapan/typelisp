extern crate typelisp;

use typelisp::*;

mod tests {
    use crate::*;

    #[test]
    fn read_int_test() -> Result<(), Error> {
        let mut reader = Reader::new();
        let obj = reader.read("123")?;
        assert_eq!(obj, Object::Int(123));

        Ok(())
    }

    #[test]
    fn read_true_test() -> Result<(), Error> {
        let mut reader = Reader::new();
        let obj = reader.read("true")?;
        assert_eq!(obj, Object::True);

        let mut reader = Reader::new();
        let obj = reader.read("false")?;
        assert_eq!(obj, Object::False);

        Ok(())
    }

    #[test]
    fn read_symbol_test() -> Result<(), Error> {
        let mut reader = Reader::new();
        let obj = reader.read("some_symbol")?;
        assert_eq!(obj, Object::Symbol("some_symbol".to_string()));

        Ok(())
    }

    #[test]
    fn read_string_test() -> Result<(), Error> {
        let mut reader = Reader::new();
        let obj = reader.read("\"Hello, World!\"")?;
        assert_eq!(obj, Object::String("Hello, World!".to_string()));

        Ok(())
    }
}