#[derive(PartialEq, Debug)]
pub enum Object {
    Null,
    Int(i64),
}

impl Object {
    pub fn new_i64(num: i64) -> Object {
        Object::Int(num)
    }
}