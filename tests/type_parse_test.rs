extern crate typelisp;

use typelisp::*;

mod tests {
    use crate::*;

    fn ty(src: &str) -> Result<Type, Error> {
        let reader = Reader::new();
        let obj = reader.read(src)?;
        parse_type(&obj)
    }

    #[test]
    fn parse_primitives() -> Result<(), Error> {
        assert_eq!(ty("i32")?, Type::I32);
        assert_eq!(ty("i64")?, Type::I64);
        assert_eq!(ty("u8")?, Type::U8);
        assert_eq!(ty("usize")?, Type::Usize);
        assert_eq!(ty("f64")?, Type::F64);
        assert_eq!(ty("bool")?, Type::Bool);
        assert_eq!(ty("char")?, Type::Char);
        assert_eq!(ty("String")?, Type::Str);
        Ok(())
    }

    #[test]
    fn parse_unit() -> Result<(), Error> {
        assert_eq!(ty("()")?, Type::Unit);
        Ok(())
    }

    #[test]
    fn parse_bare_name() -> Result<(), Error> {
        assert_eq!(ty("T")?, Type::Named("T".to_string(), vec![]));
        assert_eq!(ty("MyStruct")?, Type::Named("MyStruct".to_string(), vec![]));
        Ok(())
    }

    #[test]
    fn parse_generic() -> Result<(), Error> {
        assert_eq!(ty("Vec<T>")?, Type::Named("Vec".to_string(), vec![Type::Named("T".to_string(), vec![])]));
        assert_eq!(ty("Option<i32>")?, Type::Named("Option".to_string(), vec![Type::I32]));
        assert_eq!(
            ty("HashMap<String,i32>")?,
            Type::Named("HashMap".to_string(), vec![Type::Str, Type::I32])
        );
        // nested generics
        assert_eq!(
            ty("Vec<Option<i32>>")?,
            Type::Named(
                "Vec".to_string(),
                vec![Type::Named("Option".to_string(), vec![Type::I32])]
            )
        );
        Ok(())
    }

    #[test]
    fn parse_fn_type() -> Result<(), Error> {
        assert_eq!(
            ty("(fn (i32 i32) i32)")?,
            Type::Fn(vec![Type::I32, Type::I32], Box::new(Type::I32))
        );
        // no params, unit return
        assert_eq!(ty("(fn () ())")?, Type::Fn(vec![], Box::new(Type::Unit)));
        Ok(())
    }

    #[test]
    fn parse_tuple_type() -> Result<(), Error> {
        assert_eq!(
            ty("(tuple i32 String)")?,
            Type::Tuple(vec![Type::I32, Type::Str])
        );
        Ok(())
    }

    #[test]
    fn parse_path_type() -> Result<(), Error> {
        assert_eq!(
            ty("std::string::String")?,
            Type::Path(vec!["std".to_string(), "string".to_string(), "String".to_string()])
        );
        Ok(())
    }

    #[test]
    fn parse_errors() {
        // missing closing '>'
        assert!(ty("Vec<T").is_err());
        // unknown type constructor in list form
        assert!(ty("(foo i32)").is_err());
    }
}
