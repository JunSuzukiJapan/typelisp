#[derive(PartialEq, Debug)]
pub enum Object {
    Null,
    True,
    False,
    Int(i64),
    Symbol(String),
}

impl Object {
    pub fn new_i64(num: i64) -> Object {
        Object::Int(num)
    }

    pub fn new_symbol(s: String) -> Object {
        Object::Symbol(s)
    }
}