extern crate typelisp;

use typelisp::*;

mod tests {
    use crate::*;

    #[test]
    fn read_int_test() -> Result<(), Error> {
        let reader = Reader::new();
        let obj = reader.read("123")?;
        assert_eq!(obj, Object::Int(123));

        Ok(())
    }

    #[test]
    fn read_bool_test() -> Result<(), Error> {
        let reader = Reader::new();
        let obj = reader.read("true")?;
        assert_eq!(obj, Object::True);

        let reader = Reader::new();
        let obj = reader.read("false")?;
        assert_eq!(obj, Object::False);

        Ok(())
    }

    #[test]
    fn read_symbol_test() -> Result<(), Error> {
        let reader = Reader::new();
        let obj = reader.read("some_symbol")?;
        assert_eq!(obj, Object::Symbol("some_symbol".to_string()));

        Ok(())
    }

    #[test]
    fn read_string_test() -> Result<(), Error> {
        let reader = Reader::new();
        let obj = reader.read("\"Hello, World!\"")?;
        assert_eq!(obj, Object::String("Hello, World!".to_string()));

        Ok(())
    }

    #[test]
    fn read_null_test() -> Result<(), Error> {
        let reader = Reader::new();
        let obj = reader.read("null")?;
        assert_eq!(obj, Object::Null);

        let obj = reader.read("()")?;
        assert_eq!(obj, Object::Null);

        Ok(())
    }

    #[test]
    fn read_list_test() -> Result<(), Error> {
        let reader = Reader::new();
        let obj = reader.read("(1 2 \"foo\")")?;
        assert_eq!(
            obj,
            Object::List(Cons::new_cons(
                Object::new_i64(1),
                Some(Cons::new_cons(
                    Object::new_i64(2),
                    Some(Cons::new_cons(Object::new_string("foo".to_string()), None))
                ))
            ))
        );

        Ok(())
    }

    #[test]
    fn read_quote_test() -> Result<(), Error> {
        let reader = Reader::new();
        let obj = reader.read("'(1 2 \"foo\")")?;
        assert_eq!(
            obj,
            Object::List(Cons::new_cons(
                Object::new_symbol("quote".to_string()),
                Some(Cons::new_cons(
                    Object::List(Cons::new_cons(
                        Object::new_i64(1),
                        Some(Cons::new_cons(
                            Object::new_i64(2),
                            Some(Cons::new_cons(Object::new_string("foo".to_string()), None))
                        ))
                    )),
                    None
                ))
            ))
        );

        Ok(())
    }
}
