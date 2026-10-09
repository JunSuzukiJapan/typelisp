//! `Sexpr`'s variant numbers — the one place they are written down.
//!
//! A variant's number is its index in `registry::sexpr_def`, and that index
//! reaches much further than the registry: the interpreter's core evaluator
//! matches on it, `core_bridge` emits it into the core IR, `Repr::field_kind`
//! borrows it for struct fields, and the compiler island's `case` arms are
//! keyed on it. Every one of those reads its numbers from here; the island,
//! whose `case` keys must be literal, gets them through [`expand_island_source`].
//!
//! Numbers are only ever appended. A slot whose variant left the language
//! keeps its number ([`NIL`], [`RETIRED_BIGNUM`]), because compiled code and
//! the committed island artifact have the old numbers in them.

/// The empty list, before it became `Option<Sexpr>`'s `none`. Still in
/// `sexpr_def` so nothing after it renumbers; `Checker` refuses to let it be
/// written as a constructor or a pattern.
pub const NIL: usize = 0;
/// `int` — a fixnum or a bignum box, the value's own tagged word.
pub const INT: usize = 1;
pub const F64: usize = 2;
pub const CHAR: usize = 3;
pub const BOOL: usize = 4;
pub const SYM: usize = 5;
pub const STR: usize = 6;
pub const CONS: usize = 7;
/// Retired with the `bignum` type: a bignum box is an [`INT`] now. Never
/// constructed and never matched.
pub const RETIRED_BIGNUM: usize = 8;
pub const RATIO: usize = 9;
/// A `::`-qualified path, made matchable as a list of its segments.
pub const PATH: usize = 10;
pub const F32: usize = 11;
/// The narrow integer widths. Each is its own variant because a `Sexpr` is
/// the one place a value's type is written nowhere else.
pub const I8: usize = 12;
pub const I16: usize = 13;
pub const U8: usize = 14;
pub const U16: usize = 15;
pub const U32: usize = 16;
/// Boxed like the other widths, since [`INT`] took the bare fixnum word.
pub const I32: usize = 17;

/// How many variants `sexpr_def` has.
pub const COUNT: usize = 18;

/// Each variant's constructor name, indexed by its number.
pub const NAMES: [&str; COUNT] = [
    "nil", "int", "f64", "char", "bool", "sym", "str", "cons", "bignum", "ratio", "path", "f32", "i8", "i16", "u8",
    "u16", "u32", "i32",
];

/// Markers `core_bridge` puts in a `construct` node's variant slot for a
/// quoted datum that is not built from a stored payload but interned at run
/// time. Past [`COUNT`] so no real variant can be mistaken for one; the
/// island's `compile-construct` dispatches on them before it looks at the
/// node's other flags.
pub mod quoted {
    /// A symbol, named by its name and its owning module's segments.
    pub const SYM: usize = 100;
    /// A `::`-qualified path.
    pub const PATH: usize = 101;
    /// A symbol of the fixed vocabulary, named by its `BUILTIN_SYMBOLS` index.
    pub const WK_SYM: usize = 102;
}

/// How a value is tagged and untagged at a struct/enum field boundary
/// (`Repr::field_kind`).
///
/// The kinds share a numbering with the variants above so the island's
/// `compile-sexpr-field` decodes both with one set of arms. Where a variant's
/// encoding is the right one for a field, the kind *is* that variant's
/// number; the rest sit outside the variant range.
pub mod field_kind {
    /// Not representable in compiled code: a still-generic type variable,
    /// or a raw machine word, which must never be tagged as a fixnum.
    pub const NOT_REPRESENTABLE: usize = 0;
    /// A fixed-width integer, carried as a raw word and tagged as a fixnum on
    /// the way in. Shares its number with [`super::INT`] but not its meaning:
    /// an `int` *variant* is already a tagged word, which is [`TAGGED`]'s
    /// encoding — the island's `sexpr-variant-kind` maps one to the other.
    pub const RAW_INT: usize = super::INT;
    pub const F64: usize = super::F64;
    pub const CHAR: usize = super::CHAR;
    pub const BOOL: usize = super::BOOL;
    /// Any already-tagged word; both directions leave it alone.
    pub const TAGGED: usize = super::STR;
    pub const F32: usize = super::F32;
    /// `()`. Not a variant, so it has no number to borrow.
    pub const UNIT: usize = 100;
    /// A niched `Option` — a tagged word like [`TAGGED`], with its own number
    /// so the island can tell where an `Option` around it must be boxed.
    pub const NICHE: usize = 101;
}

/// Every name the compiler island's source may write as `#%name`, with the
/// number it stands for.
const ISLAND_NAMES: &[(&str, usize)] = &[
    ("sexpr-nil", NIL),
    ("sexpr-int", INT),
    ("sexpr-f64", F64),
    ("sexpr-char", CHAR),
    ("sexpr-bool", BOOL),
    ("sexpr-sym", SYM),
    ("sexpr-str", STR),
    ("sexpr-cons", CONS),
    ("sexpr-retired-bignum", RETIRED_BIGNUM),
    ("sexpr-ratio", RATIO),
    ("sexpr-path", PATH),
    ("sexpr-f32", F32),
    ("sexpr-i8", I8),
    ("sexpr-i16", I16),
    ("sexpr-u8", U8),
    ("sexpr-u16", U16),
    ("sexpr-u32", U32),
    ("sexpr-i32", I32),
    ("quoted-sym", quoted::SYM),
    ("quoted-path", quoted::PATH),
    ("quoted-wk-sym", quoted::WK_SYM),
    ("kind-not-representable", field_kind::NOT_REPRESENTABLE),
    ("kind-raw-int", field_kind::RAW_INT),
    ("kind-f64", field_kind::F64),
    ("kind-char", field_kind::CHAR),
    ("kind-bool", field_kind::BOOL),
    ("kind-tagged", field_kind::TAGGED),
    ("kind-f32", field_kind::F32),
    ("kind-unit", field_kind::UNIT),
    ("kind-niche", field_kind::NICHE),
];

/// The compiler island's source with every `#%name` replaced by its number.
///
/// A `case` key must be a literal, so the island cannot name a constant
/// there; it writes `#%sexpr-cons` and this turns it into `7` before the
/// source is read. A name not in the table panics rather than being left in:
/// the island's source is fixed text, so a bad name is a build bug. (One
/// that slipped past would still not compile silently — the reader has no
/// `#%` syntax.)
pub fn expand_island_source(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find("#%") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(after.len());
        let name = &after[..len];
        let Some(&(_, number)) = ISLAND_NAMES.iter().find(|(n, _)| *n == name) else {
            panic!("compiler island source: `#%{}` names no Sexpr variant or field kind", name);
        };
        out.push_str(&number.to_string());
        rest = &after[len..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_names_expand_to_their_numbers() {
        assert_eq!(expand_island_source("(case v (#%sexpr-cons 1) ((#%kind-unit #%quoted-wk-sym) 2))"), "(case v (7 1) ((100 102) 2))");
    }

    #[test]
    #[should_panic(expected = "#%sexpr-vector")]
    fn an_unknown_island_name_is_refused() {
        expand_island_source("(#%sexpr-vector)");
    }

    #[test]
    fn island_names_are_unique() {
        for (i, (a, _)) in ISLAND_NAMES.iter().enumerate() {
            assert!(ISLAND_NAMES[i + 1..].iter().all(|(b, _)| a != b), "`{}` is listed twice", a);
        }
    }
}
