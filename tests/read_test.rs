extern crate typelisp;

use typelisp::*;

mod tests {
    use crate::*;

    #[test]
    fn read_int_test() {
        let mut reader = Reader::new();
        let obj = reader.read("123");
        assert_eq!(obj, Object::Int(123));
    }
}