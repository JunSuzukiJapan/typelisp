//! [`ConcreteNames`]: the one place a generated name is spelled — a type's
//! display (`gen<i32>`), a method's link name (`point''area`), an LLVM symbol
//! (`tl_point''area`). A spelling decided twice is a lookup that fails in
//! between. Everything here works on strings, because this crate is below
//! every tier that needs it (the printer's AOT tier cannot see the front end);
//! the front end's `Type` reaches it through [`TypeKey`]. A specialization is
//! named in [`crate::specialization`], by a symbol rather than a string.
//!
//! Why no generated name can collide with one a program writes: every
//! generated name has a space, a `'` or a `<` where a source name cannot, the
//! reader ending a symbol at the first two and reading `<..>` as type
//! arguments.

use std::fmt::Display;

/// Something with a type key: a type the front end knows (its `Type`
/// implements this), or a key already spelled.
pub trait TypeKey {
    fn type_key(&self) -> String;
}

impl TypeKey for str {
    fn type_key(&self) -> String {
        self.to_string()
    }
}

impl TypeKey for String {
    fn type_key(&self) -> String {
        self.clone()
    }
}

impl<T: TypeKey + ?Sized> TypeKey for &T {
    fn type_key(&self) -> String {
        (**self).type_key()
    }
}

impl<T: TypeKey + ?Sized> TypeKey for Box<T> {
    fn type_key(&self) -> String {
        (**self).type_key()
    }
}

/// Between a method's type and its name in a link name. Not `::`: module
/// `a`'s function `b` is `a::b`, and type `a`'s method `b` has to be another
/// name. `'` is a reader delimiter, so no name written in source contains it.
const METHOD_SEPARATOR: &str = "''";

/// What a compiled definition's LLVM symbol starts with. Without it a user
/// function could take the name of a libc symbol the LLVM backend itself
/// calls into (`fmod`, the lowering of the `frem` instruction `f64`'s `mod`
/// compiles to) and resolve to that instead of its own body — found compiling
/// a test function literally named `fmod`. The `rt_*` runtime shims are never
/// looked up through it, so a user function named `rt_cons` is prefixed like
/// any other and cannot collide with the real one either.
const SYMBOL_PREFIX: &str = "tl_";

/// See the module documentation.
pub struct ConcreteNames;

impl ConcreteNames {
    // ---- type keys -------------------------------------------------------

    /// `t`'s type key.
    pub fn type_key<T: TypeKey + ?Sized>(t: &T) -> String {
        t.type_key()
    }

    /// The key of the type `base` instantiated at `args`: `gen<i32,string>`,
    /// or `base` itself when there are no arguments, so a type without
    /// parameters keeps its plain name.
    pub fn instantiated_type<A: TypeKey>(base: &str, args: &[A]) -> String {
        if args.is_empty() {
            base.to_string()
        } else {
            format!("{}<{}>", base, Self::type_args(args))
        }
    }

    /// A list of type arguments: each one's key, joined by `,`. A key has
    /// `,` only inside its own brackets, so the list reads back
    /// unambiguously. What goes between the `<>` of an
    /// instantiated type and of a specialization.
    pub fn type_args<A: TypeKey>(args: &[A]) -> String {
        args.iter().map(TypeKey::type_key).collect::<Vec<_>>().join(",")
    }

    /// The key of a trait object: `dyn shape`, `dyn iter<int>` with the
    /// trait's associated types pinned. The space keeps it from being a
    /// written name.
    pub fn dyn_type<A: TypeKey>(trait_key: &str, pins: &[A]) -> String {
        format!("dyn {}", Self::instantiated_type(trait_key, pins))
    }

    /// The key of a function type: `(fn (int,string) int)`, with a `&rest`
    /// parameter last as `&rest T`.
    pub fn fn_type<A: TypeKey, R: TypeKey>(params: &[A], rest: Option<&R>, ret: &R) -> String {
        let mut inner: Vec<String> = params.iter().map(TypeKey::type_key).collect();
        if let Some(r) = rest {
            inner.push(format!("&rest {}", r.type_key()));
        }
        format!("(fn ({}) {})", inner.join(","), ret.type_key())
    }

    /// The key of a typed pointer to `pointee`: `(ptr point)`.
    pub fn ptr_type<T: TypeKey + ?Sized>(pointee: &T) -> String {
        format!("(ptr {})", pointee.type_key())
    }

    /// The placeholder a key template writes the owning type's `index`th
    /// parameter as: `$0`. `$` never occurs in a key, so it cannot collide
    /// with a name.
    pub fn template_param(index: usize) -> String {
        format!("${}", index)
    }

    // ---- link names and symbols -----------------------------------------

    /// A method's link name: its type's whole path, [`METHOD_SEPARATOR`],
    /// then the method's name — `geo::point''area`, `box''twice <int>`. The
    /// whole path, never its last segment: two types in different modules
    /// may share both a local name and a method name. What a message names
    /// a method by, the key a dump files it under, and its LLVM symbol past
    /// the prefix.
    pub fn method_link(type_path: impl Display, method: &str) -> String {
        format!("{}{}{}", type_path, METHOD_SEPARATOR, method)
    }

    /// The type path and method name of a [`Self::method_link`], or `None`
    /// for a name with no separator — a function's.
    pub fn split_method_link(name: &str) -> Option<(&str, &str)> {
        name.split_once(METHOD_SEPARATOR)
    }

    /// The LLVM symbol a compiled definition is defined and called under:
    /// the prefix, then its link name (a function's path, or a
    /// [`Self::method_link`]).
    pub fn symbol(link_name: &str) -> String {
        format!("{}{}", SYMBOL_PREFIX, link_name)
    }

    /// A method's LLVM symbol: [`Self::symbol`] of its [`Self::method_link`]
    /// — `tl_geo::point''area`.
    pub fn method_symbol(type_path: impl Display, method: &str) -> String {
        Self::symbol(&Self::method_link(type_path, method))
    }
}

#[cfg(test)]
mod tests {
    use super::ConcreteNames as N;

    #[test]
    fn an_instantiated_type_is_spelled_with_its_arguments() {
        assert_eq!(N::instantiated_type("gen", &["i32", "Vector<cons-cell<int,int>>"]), "gen<i32,Vector<cons-cell<int,int>>>");
        assert_eq!(N::instantiated_type::<&str>("point", &[]), "point");
    }

    #[test]
    fn a_function_type_is_spelled_with_its_parameters() {
        assert_eq!(N::fn_type(&["(ptr point)", "i32"], None, &"bool"), "(fn ((ptr point),i32) bool)");
    }

    #[test]
    fn a_link_name_and_its_symbol() {
        let link = N::method_link("geo::point", "area");
        assert_eq!(link, "geo::point''area");
        assert_eq!(N::split_method_link(&link), Some(("geo::point", "area")));
        assert_eq!(N::split_method_link("geo::area"), None);
        assert_eq!(N::method_symbol("geo::point", "area"), N::symbol(&link));
        assert_eq!(N::symbol(&link), "tl_geo::point''area");
    }
}
