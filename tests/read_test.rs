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

    #[test]
    fn read_float_test() -> Result<(), Error> {
        let reader = Reader::new();
        assert_eq!(reader.read("3.14")?, Object::Float(3.14));
        assert_eq!(reader.read(".5")?, Object::Float(0.5));
        assert_eq!(reader.read("1.")?, Object::Float(1.0));
        assert_eq!(reader.read("1e3")?, Object::Float(1000.0));
        assert_eq!(reader.read("1.5e-2")?, Object::Float(0.015));
        Ok(())
    }

    #[test]
    fn read_signed_number_test() -> Result<(), Error> {
        let reader = Reader::new();
        assert_eq!(reader.read("-5")?, Object::Int(-5));
        assert_eq!(reader.read("+7")?, Object::Int(7));
        assert_eq!(reader.read("-3.5")?, Object::Float(-3.5));
        // `-` / `+` alone or `1+` remain symbols
        assert_eq!(reader.read("-")?, Object::Symbol("-".to_string()));
        assert_eq!(reader.read("1+")?, Object::Symbol("1+".to_string()));
        Ok(())
    }

    #[test]
    fn read_radix_test() -> Result<(), Error> {
        let reader = Reader::new();
        assert_eq!(reader.read("0xFF")?, Object::Int(255));
        assert_eq!(reader.read("0o17")?, Object::Int(15));
        assert_eq!(reader.read("0b1010")?, Object::Int(10));
        assert_eq!(reader.read("1_000")?, Object::Int(1000));
        Ok(())
    }

    #[test]
    fn read_suffix_test() -> Result<(), Error> {
        let reader = Reader::new();
        assert_eq!(reader.read("5u8")?, Object::IntWithSuffix(5, NumSuffix::U8));
        assert_eq!(reader.read("42i32")?, Object::IntWithSuffix(42, NumSuffix::I32));
        assert_eq!(reader.read("0xFFu8")?, Object::IntWithSuffix(255, NumSuffix::U8));
        assert_eq!(reader.read("1.0f32")?, Object::FloatWithSuffix(1.0, NumSuffix::F32));
        Ok(())
    }

    #[test]
    fn read_char_test() -> Result<(), Error> {
        let reader = Reader::new();
        assert_eq!(reader.read("#\\a")?, Object::Char('a'));
        assert_eq!(reader.read("#\\Space")?, Object::Char(' '));
        assert_eq!(reader.read("#\\Newline")?, Object::Char('\n'));
        assert_eq!(reader.read("#\\(")?, Object::Char('('));
        Ok(())
    }

    #[test]
    fn read_string_escape_test() -> Result<(), Error> {
        let reader = Reader::new();
        assert_eq!(reader.read("\"a\\nb\"")?, Object::String("a\nb".to_string()));
        assert_eq!(reader.read("\"tab\\there\"")?, Object::String("tab\there".to_string()));
        assert_eq!(reader.read("\"q\\\"q\"")?, Object::String("q\"q".to_string()));
        assert_eq!(reader.read("\"\\u{41}\"")?, Object::String("A".to_string()));
        Ok(())
    }

    #[test]
    fn read_comment_test() -> Result<(), Error> {
        let reader = Reader::new();
        assert_eq!(reader.read("; a comment\n42")?, Object::Int(42));
        assert_eq!(reader.read("#| block |# 7")?, Object::Int(7));
        assert_eq!(reader.read("#| outer #| nested |# still |# 9")?, Object::Int(9));
        // comment inside a list, including just before ')'
        assert_eq!(
            reader.read("(1 ; mid\n 2 #| x |#)")?,
            Object::List(Cons::new_cons(
                Object::new_i64(1),
                Some(Cons::new_cons(Object::new_i64(2), None))
            ))
        );
        Ok(())
    }

    #[test]
    fn read_whitespace_test() -> Result<(), Error> {
        // newlines and tabs must be treated as whitespace (was a bug)
        let reader = Reader::new();
        assert_eq!(
            reader.read("(\n\t1\t2\n)")?,
            Object::List(Cons::new_cons(
                Object::new_i64(1),
                Some(Cons::new_cons(Object::new_i64(2), None))
            ))
        );
        Ok(())
    }

    #[test]
    fn read_nil_alias_test() -> Result<(), Error> {
        let reader = Reader::new();
        assert_eq!(reader.read("nil")?, Object::Null);
        Ok(())
    }

    #[test]
    fn read_type_symbol_test() -> Result<(), Error> {
        // generic/operator tokens read as single symbols (whitespace disambiguates)
        let reader = Reader::new();
        assert_eq!(reader.read("Vec<T>")?, Object::Symbol("Vec<T>".to_string()));
        assert_eq!(reader.read("HashMap<K,V>")?, Object::Symbol("HashMap<K,V>".to_string()));
        assert_eq!(reader.read("==")?, Object::Symbol("==".to_string()));
        assert_eq!(reader.read("<=")?, Object::Symbol("<=".to_string()));
        assert_eq!(reader.read("std::process::exit")?, Object::Symbol("std::process::exit".to_string()));
        Ok(())
    }
}
