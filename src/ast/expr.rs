pub enum Expr {
    Null,
    Int(i64),
    Plus(Vec<Box<Expr>>),
}