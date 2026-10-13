//! [`ConcreteNames`]: the one place a generic thing's concrete name is spelled
//! and read back.
//!
//! A generic type, function or method has no single name: each instantiation
//! is a thing of its own, and has to be told apart from every other by a
//! string — a heap value's type key (`gen<i32>`), a specialization's
//! registered name (`unwrap <i32>`), a method's link name (`point''area`), an
//! LLVM symbol (`tl_point''area`). Each of those is built in one tier and read
//! back in another, so a spelling decided twice is a lookup that fails in
//! between. Everything here works on strings, because this crate is below
//! every tier that needs it (the printer's AOT tier cannot see the front end);
//! the front end's `Type` reaches it through [`TypeKey`].
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
    /// `,` only inside its own brackets, so [`Self::type_arg_list`] reads the
    /// list back unambiguously. What goes between the `<>` of an
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

    /// The parameter and return keys of a function type's key, or `None`
    /// for any other key — and for one with a `&rest` parameter, which no
    /// caller can pass on.
    pub fn split_fn_type(key: &str) -> Option<(Vec<String>, String)> {
        let rest = key.strip_prefix("(fn (")?.strip_suffix(')')?;
        let mut depth = 0i32;
        let mut close = None;
        for (i, c) in rest.char_indices() {
            match c {
                '(' | '<' => depth += 1,
                '>' => depth -= 1,
                ')' if depth == 0 => {
                    close = Some(i);
                    break;
                }
                ')' => depth -= 1,
                _ => {}
            }
        }
        let close = close?;
        let params_src = &rest[..close];
        let ret = rest[close + 1..].strip_prefix(' ')?.to_string();
        let params: Vec<String> = if params_src.is_empty() {
            Vec::new()
        } else {
            Self::split_top_level(params_src).into_iter().map(str::to_string).collect()
        };
        if params.iter().any(|p| p.starts_with("&rest ")) {
            return None;
        }
        Some((params, ret))
    }

    /// The key of a typed pointer to `pointee`: `(ptr point)`.
    pub fn ptr_type<T: TypeKey + ?Sized>(pointee: &T) -> String {
        format!("(ptr {})", pointee.type_key())
    }

    /// The pointee's key of a typed pointer's key, or `None` for any other
    /// key.
    pub fn ptr_target(key: &str) -> Option<&str> {
        key.strip_prefix("(ptr ")?.strip_suffix(')')
    }

    /// A type key without its instantiation: `option<char>` -> `option`,
    /// `point` -> `point`. What looks a *type* up from the key a *value*
    /// carries — registration is per type, a key names an instantiation.
    pub fn base_type(key: &str) -> &str {
        match key.find('<') {
            Some(i) if key.ends_with('>') => &key[..i],
            _ => key,
        }
    }

    /// A key split into its type and the arguments as written:
    /// `gen<i32>` -> `("gen", Some("i32"))`, `point` -> `("point", None)`.
    pub fn split_type(key: &str) -> (&str, Option<&str>) {
        let base = Self::base_type(key);
        if base.len() == key.len() {
            (key, None)
        } else {
            (base, Some(&key[base.len() + 1..key.len() - 1]))
        }
    }

    /// The type arguments inside a key, split at the outermost level only:
    /// `hashtable<string,option<int>>` -> `["string", "option<int>"]`. Empty
    /// for a key with no instantiation.
    pub fn type_arg_list(key: &str) -> Vec<&str> {
        match Self::split_type(key).1 {
            Some(inner) => Self::split_top_level(inner),
            None => Vec::new(),
        }
    }

    /// The one type argument inside `key`: `vector<cons-cell<i32,string>>`
    /// -> `cons-cell<i32,string>`. `None` when the key names no
    /// instantiation or names more than one argument — "the first of
    /// several" is not a question any caller is asking.
    pub fn single_type_arg(key: &str) -> Option<&str> {
        match Self::type_arg_list(key).as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }

    /// `src` cut at its commas outside any `<>` or `()` — a function type's
    /// parameter list is spelled `(fn (int,string) int)`, and its commas are
    /// its own.
    fn split_top_level(src: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut depth = 0i32;
        let mut start = 0;
        for (i, c) in src.char_indices() {
            match c {
                '<' | '(' => depth += 1,
                '>' | ')' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(&src[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        out.push(&src[start..]);
        out
    }

    /// The placeholder a key template writes the owning type's `index`th
    /// parameter as: `$0`. `$` never occurs in a key, so it cannot collide
    /// with a name.
    pub fn template_param(index: usize) -> String {
        format!("${}", index)
    }

    /// A key template ([`Self::template_param`]s in place of the owning
    /// type's parameters) at one instantiation's argument keys.
    ///
    /// A template that names a parameter the instantiation does not supply is
    /// a definition/value mismatch, and aborts rather than answering with a
    /// `$N` in it.
    pub fn instantiate_template(template: &str, args: &[&str]) -> String {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            let digits: String = rest[i + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            let n: usize = digits.parse().unwrap_or_else(|_| {
                panic!("type key template `{}`: `$` is not followed by a parameter index", template)
            });
            let arg = args.get(n).unwrap_or_else(|| {
                panic!(
                    "type key template `{}` names parameter {} but the instantiation has {} argument(s)",
                    template,
                    n,
                    args.len()
                )
            });
            out.push_str(arg);
            rest = &rest[i + 1 + digits.len()..];
        }
        out.push_str(rest);
        out
    }

    // ---- specializations -------------------------------------------------

    /// The name a generic function or method has at type arguments `args`:
    /// `unwrap <i32>`. A method's arguments are its type's, then its own. The
    /// space keeps it from being a written name.
    pub fn specialization<A: TypeKey>(base: &str, args: &[A]) -> String {
        format!("{} <{}>", base, Self::type_args(args))
    }

    /// A function's path segments at type arguments `args`: the last segment
    /// becomes its [`Self::specialization`], the module stays.
    pub fn specialized_path<A: TypeKey>(segments: &[String], args: &[A]) -> Vec<String> {
        let (last, parent) = segments.split_last().expect("a path has at least one segment");
        let mut out = parent.to_vec();
        out.push(Self::specialization(last, args));
        out
    }

    /// A name split into what was written and the type arguments of its
    /// specialization: `print-object <i32>` -> `("print-object", Some("i32"))`,
    /// `area` -> `("area", None)`.
    pub fn split_specialization(name: &str) -> (&str, Option<&str>) {
        match name.split_once(" <") {
            Some((base, rest)) => match rest.strip_suffix('>') {
                Some(args) => (base, Some(args)),
                None => (name, None),
            },
            None => (name, None),
        }
    }

    /// The name `method` has for a value whose key is `value_key`: its
    /// specialization at the value's type arguments (`("gen<i32>",
    /// "print-object")` -> `print-object <i32>`), or `method` itself for a
    /// type without parameters. How a dispatcher holding a value finds the
    /// body registered for that value's instantiation.
    pub fn method_for_value(value_key: &str, method: &str) -> String {
        match Self::split_type(value_key).1 {
            Some(args) => format!("{} <{}>", method, args),
            None => method.to_string(),
        }
    }

    /// The other direction of [`Self::method_for_value`]: the key of the
    /// values `name` — `method` or one of its specializations, registered on
    /// the type `type_key` — is for. `None` when `name` is not `method`.
    pub fn value_key_for_method(type_key: &str, method: &str, name: &str) -> Option<String> {
        match Self::split_specialization(name) {
            (base, _) if base != method => None,
            (_, Some(args)) => Some(format!("{}<{}>", type_key, args)),
            (_, None) => Some(type_key.to_string()),
        }
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

    /// The link name a [`Self::symbol`] was built from, or `None` for a
    /// symbol that is not one.
    pub fn link_name_of_symbol(symbol: &str) -> Option<&str> {
        symbol.strip_prefix(SYMBOL_PREFIX)
    }

    /// The key a blanket `impl` of `trait_path` materialized at the type
    /// `target_key` is remembered under among the specializations already
    /// asked for — kept apart from every specialization's name by its
    /// leading word and space.
    pub fn blanket_instance(trait_path: impl Display, target_key: &str) -> String {
        format!("blanket {} {}", trait_path, target_key)
    }
}

#[cfg(test)]
mod tests {
    use super::ConcreteNames as N;

    #[test]
    fn an_instantiated_type_reads_back() {
        let key = N::instantiated_type("gen", &["i32", "vector<cons-cell<int,int>>"]);
        assert_eq!(key, "gen<i32,vector<cons-cell<int,int>>>");
        assert_eq!(N::split_type(&key), ("gen", Some("i32,vector<cons-cell<int,int>>")));
        assert_eq!(N::type_arg_list(&key), vec!["i32", "vector<cons-cell<int,int>>"]);
        assert_eq!(N::single_type_arg(&key), None);
        assert_eq!(N::single_type_arg("option<char>"), Some("char"));
        assert_eq!(N::instantiated_type::<&str>("point", &[]), "point");
        assert_eq!(N::split_type("point"), ("point", None));
    }

    #[test]
    fn a_function_type_reads_back() {
        let key = N::fn_type(&["(ptr point)", "i32"], None, &"bool");
        assert_eq!(key, "(fn ((ptr point),i32) bool)");
        assert_eq!(N::split_fn_type(&key), Some((vec!["(ptr point)".into(), "i32".into()], "bool".into())));
        assert_eq!(N::split_fn_type("(fn () ())"), Some((vec![], "()".into())));
        assert_eq!(N::split_fn_type("(fn (ptr,ptr) i32)"), Some((vec!["ptr".into(), "ptr".into()], "i32".into())));
        assert_eq!(N::split_fn_type("(fn (hashtable<string,int>) int)"), Some((vec!["hashtable<string,int>".into()], "int".into())));
        assert_eq!(N::split_fn_type("ptr"), None);
        assert_eq!(N::split_fn_type(&N::fn_type(&["i32"], Some(&"i32"), &"i32")), None);
        assert_eq!(N::split_type(&key).1, None);
        assert_eq!(N::type_arg_list(&N::instantiated_type("gen", &[key.as_str(), "int"])), vec![key.as_str(), "int"]);
    }

    #[test]
    fn a_specialization_reads_back() {
        let name = N::specialization("print-object", &["i32"]);
        assert_eq!(name, "print-object <i32>");
        assert_eq!(N::split_specialization(&name), ("print-object", Some("i32")));
        assert_eq!(N::method_for_value("gen<i32>", "print-object"), name);
        assert_eq!(N::method_for_value("point", "print-object"), "print-object");
        assert_eq!(N::value_key_for_method("gen", "print-object", &name).as_deref(), Some("gen<i32>"));
        assert_eq!(N::value_key_for_method("point", "print-object", "print-object").as_deref(), Some("point"));
        assert_eq!(N::value_key_for_method("point", "print-object", "area"), None);
        assert_eq!(N::specialized_path(&["m".into(), "f".into()], &["int"]), vec!["m".to_string(), "f <int>".into()]);
    }

    #[test]
    fn a_link_name_and_its_symbol_read_back() {
        let link = N::method_link("geo::point", "area <int>");
        assert_eq!(link, "geo::point''area <int>");
        assert_eq!(N::split_method_link(&link), Some(("geo::point", "area <int>")));
        assert_eq!(N::split_method_link("geo::area"), None);
        let sym = N::symbol(&link);
        assert_eq!(N::link_name_of_symbol(&sym), Some(link.as_str()));
        assert_eq!(N::method_symbol("geo::point", "area <int>"), sym);
        assert_eq!(N::link_name_of_symbol("rt_heap_init"), None);
    }

    #[test]
    fn type_arguments_split_at_their_own_level_only() {
        assert_eq!(N::type_arg_list("int"), Vec::<&str>::new());
        assert_eq!(N::type_arg_list("option<int>"), vec!["int"]);
        assert_eq!(N::type_arg_list("hashtable<string,option<int>>"), vec!["string", "option<int>"]);
        assert_eq!(
            N::type_arg_list("gen<(fn (int,string) int),vector<cons-cell<int,int>>>"),
            vec!["(fn (int,string) int)", "vector<cons-cell<int,int>>"]
        );
    }

    #[test]
    fn a_template_takes_an_instantiations_arguments() {
        let t = format!("option<{}>", N::template_param(1));
        assert_eq!(N::instantiate_template(&t, &["int", "string"]), "option<string>");
        assert_eq!(N::instantiate_template("hashtable<$1,vector<$0>>", &["string", "int"]), "hashtable<int,vector<string>>");
        assert_eq!(N::instantiate_template("int", &[]), "int");
        assert_eq!(N::instantiate_template("$10", &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"]), "k");
    }
}
