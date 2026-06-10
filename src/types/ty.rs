//! The shared static-type vocabulary for typelisp.
//!
//! Type names follow Rust spelling (`i32`, `f64`, `bool`, `String`, `Vec<T>`).
//! There are no reference types (the language is GC-based); strings are the
//! single `String` type, represented here as [`Type::Str`].

/// A static type.
///
/// `Eq`/`Hash` are derivable because no variant stores a float *value* — only
/// the *names* of float types (`F32`/`F64`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    // signed integers
    I8, I16, I32, I64, Isize,
    // unsigned integers
    U8, U16, U32, U64, Usize,
    // floats
    F32, F64,
    Bool,
    Char,
    /// The single `String` type.
    Str,
    /// The unit type `()`.
    Unit,
    /// A named, possibly-generic type: `Vec<T>`, `Option<i32>`, a user struct
    /// `Point<f64>`, or a bare name (`T`, `MyStruct`) with no arguments. The
    /// type checker decides whether a bare name is a struct or a type variable.
    Named(String, Vec<Type>),
    /// A function type `(fn (A B) R)`.
    Fn(Vec<Type>, Box<Type>),
    /// A tuple type `(tuple A B)`.
    Tuple(Vec<Type>),
    /// A path-qualified name like `std::string::String`.
    Path(Vec<String>),
    /// An inference variable (used only during type checking).
    Var(u32),
}

impl Type {
    /// Map a bare primitive type name to its [`Type`], if it is one.
    pub fn from_prim_name(name: &str) -> Option<Type> {
        let t = match name {
            "i8" => Type::I8,
            "i16" => Type::I16,
            "i32" => Type::I32,
            "i64" => Type::I64,
            "isize" => Type::Isize,
            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "usize" => Type::Usize,
            "f32" => Type::F32,
            "f64" => Type::F64,
            "bool" => Type::Bool,
            "char" => Type::Char,
            "String" => Type::Str,
            _ => return None,
        };
        Some(t)
    }

    /// Default type for an unsuffixed integer literal.
    pub fn default_int() -> Type {
        Type::I64
    }

    /// Default type for an unsuffixed float literal.
    pub fn default_float() -> Type {
        Type::F64
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Isize
                | Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Usize
        )
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }

    pub fn is_numeric(&self) -> bool {
        self.is_integer() || self.is_float()
    }

    /// Whether an integer type is signed (`true` for `iN`/`isize`).
    /// Returns `false` for non-integer types.
    pub fn is_signed(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Isize)
    }
}
