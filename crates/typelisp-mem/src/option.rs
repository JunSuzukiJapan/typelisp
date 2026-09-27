//! The `Option<T>` niche: which instantiations have no box, decided from a
//! type key.
//!
//! An `Option<T>` is represented as `T`'s own tagged word for `some v` and
//! the empty-list immediate ([`crate::NIL_WORD`], `Value::Empty`) for
//! `none` whenever no `T` value can *be* that immediate — which is every
//! type except the ones listed in [`option_payload_niches`]. The checker
//! decides the same thing from a `Type` (`Repr::of_by` in `typelisp-front`,
//! kept in agreement by a test there); this is the decision for the code
//! that only has a *key* in hand — the runtime producers of `Option`
//! values (`Heap::alloc_option`) and the printer, which sees a struct field's
//! type only as the key its template instantiates to.
//!
//! Lives in this crate, below both tiers, for the same reason
//! [`crate::base_type_key`] does: the interpreter and an AOT executable's
//! printer must answer identically, and the AOT tier cannot see
//! `typelisp-front`.

use crate::heap::{base_type_key, inner_type_key};
use crate::{Heap, Value};

/// Whether `Option<P>` is niche-represented, given `P`'s type key.
///
/// The payload must never be the empty-list word. A nested `Option` may be
/// (its own `none`), `()` *is* it, and a raw C word (`ptr`/`c-long`/
/// `c-ulong`) has no tag to keep it apart; `!` has no values and no
/// representation at all. Everything else — fixnums, boxes, strings,
/// symbols, characters, booleans, cons cells, and `sexpr` itself, whose
/// empty list left it for `Option<sexpr>` — is a word the immediate is not.
pub fn option_payload_niches(payload_key: &str) -> bool {
    !(base_type_key(payload_key) == "option" || matches!(payload_key, "()" | "!" | "ptr" | "c-long" | "c-ulong"))
}

/// Whether the type `key` names is a niche-represented `Option`: an
/// `option<P>` whose `P` passes [`option_payload_niches`].
pub fn is_niched_option_key(key: &str) -> bool {
    base_type_key(key) == "option" && inner_type_key(key).is_some_and(option_payload_niches)
}

/// Whether a *printer* should write a value of the type `key` names as
/// `(some ...)`/`none` around the word it holds: a niche-represented
/// `Option` ([`is_niched_option_key`]) other than `Option<sexpr>`.
/// `Option<sexpr>` is S-expression data — the S-expression *is* the value
/// and the empty list is its `none` — and prints as the datum it is, `()`
/// included, wherever it sits (`docs/ja/reference/functions/printing.md` §1). Every other niche has no
/// such reading, so the wrapper the word cannot carry is written back.
pub fn option_prints_wrapped(key: &str) -> bool {
    is_niched_option_key(key) && inner_type_key(key) != Some("sexpr")
}

/// The type arguments inside an instantiated key — the comma-separated
/// list `key`'s outermost `<…>` holds, split at its own level only:
/// `hashtable<string,option<int>>` -> `["string", "option<int>"]`. Empty
/// for a key with no instantiation.
pub fn type_key_args(key: &str) -> Vec<&str> {
    let Some(open) = key.find('<') else { return Vec::new() };
    let Some(inner) = key[open + 1..].strip_suffix('>') else { return Vec::new() };
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(&inner[start..]);
    args
}

/// A field's type key for one instantiation of its type: `template` is the
/// field's declared type as a key with each type parameter written `$N`
/// (its index), and `args` are the instantiation's argument keys
/// ([`type_key_args`] of the value's own key). `$` never occurs in a key,
/// so the substitution is a plain textual one.
///
/// A template that names a parameter the instantiation does not supply is
/// a definition/value mismatch, and aborts rather than printing a `$N`.
pub fn instantiate_key_template(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(i) = rest.find('$') {
        out.push_str(&rest[..i]);
        let digits: String = rest[i + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        let n: usize = digits.parse().unwrap_or_else(|_| {
            panic!("type key template `{}`: `$` is not followed by a parameter index", template)
        });
        let arg = args.get(n).unwrap_or_else(|| {
            panic!("type key template `{}` names parameter {} but the instantiation has {} argument(s)", template, n, args.len())
        });
        out.push_str(arg);
        rest = &rest[i + 1 + digits.len()..];
    }
    out.push_str(rest);
    out
}

impl Heap {
    /// An `Option` value under the identity `key` spells (`option<P>`): the
    /// niche when `P` allows one — `v` itself for `some`, [`Value::Empty`]
    /// for `none` — and a `BoxedObj::Enum` box (`some` = variant 0, `none` =
    /// variant 1, `option_def`'s order) otherwise. **The one producer** for
    /// Rust code that builds an `Option` from a key; the checker's
    /// `some-of`/`none` forms are its typed twin.
    pub fn alloc_option(&mut self, key: &str, v: Option<Value>) -> Value {
        assert!(
            base_type_key(key) == "option" && inner_type_key(key).is_some(),
            "alloc_option: `{}` is not an `option<P>` key",
            key
        );
        if is_niched_option_key(key) {
            return v.unwrap_or(Value::Empty);
        }
        let id = self.intern_type_key(key);
        match v {
            Some(x) => self.alloc_enum(id, 0, vec![x]),
            None => self.alloc_enum(id, 1, vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_niches_unless_it_can_be_the_empty_word() {
        for k in ["int", "i32", "u8", "f64", "f32", "char", "bool", "string", "symbol", "ratio", "sexpr", "point", "m::point", "vector<int>", "hashtable<string,int>", "result<int,parseinterror>", "dyn shape", "(fn (int) int)", "(fn (int,string) int)", "llvm-value"] {
            assert!(option_payload_niches(k), "{}", k);
            assert!(is_niched_option_key(&format!("option<{}>", k)), "option<{}>", k);
        }
        for k in ["option<int>", "option<sexpr>", "option<option<int>>", "()", "!", "ptr", "c-long", "c-ulong"] {
            assert!(!option_payload_niches(k), "{}", k);
            assert!(!is_niched_option_key(&format!("option<{}>", k)), "option<{}>", k);
        }
        assert!(!is_niched_option_key("int"));
        assert!(!is_niched_option_key("vector<option<int>>"));
        assert!(option_prints_wrapped("option<int>"));
        assert!(!option_prints_wrapped("option<sexpr>"));
        assert!(!option_prints_wrapped("option<option<int>>"));
    }

    #[test]
    fn type_key_arguments_split_at_their_own_level_only() {
        assert_eq!(type_key_args("int"), Vec::<&str>::new());
        assert_eq!(type_key_args("option<int>"), vec!["int"]);
        assert_eq!(type_key_args("hashtable<string,option<int>>"), vec!["string", "option<int>"]);
        assert_eq!(type_key_args("gen<(fn (int,string) int),vector<cons-cell<int,int>>>"), vec!["(fn (int,string) int)", "vector<cons-cell<int,int>>"]);
    }

    #[test]
    fn a_field_template_instantiates_by_textual_substitution() {
        assert_eq!(instantiate_key_template("option<$0>", &["int"]), "option<int>");
        assert_eq!(instantiate_key_template("hashtable<$1,vector<$0>>", &["string", "int"]), "hashtable<int,vector<string>>");
        assert_eq!(instantiate_key_template("int", &[]), "int");
        assert_eq!(instantiate_key_template("$10", &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"]), "k");
    }

    #[test]
    fn alloc_option_niches_or_boxes_by_the_key() {
        let mut h = Heap::with_capacity(64);
        assert_eq!(h.alloc_option("option<int>", Some(Value::Int(5))), Value::Int(5));
        assert_eq!(h.alloc_option("option<int>", None), Value::Empty);
        assert_eq!(h.box_count(), 0);
        let Value::Boxed(some) = h.alloc_option("option<option<int>>", Some(Value::Int(5))) else { panic!("boxed") };
        assert!(h.is_enum(some));
        assert_eq!(h.enum_variant(some), 0);
        let Value::Boxed(none) = h.alloc_option("option<()>", None) else { panic!("boxed") };
        assert_eq!(h.enum_variant(none), 1);
    }
}
