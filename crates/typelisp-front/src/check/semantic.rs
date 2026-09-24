//! Semantic highlighting of type names, for `textDocument/semanticTokens`.
//!
//! A `defstruct`/`defenum`/`deftrait` name is usually lowercase (`rect`,
//! `todo-item`, `board`), so an editor's syntax grammar — which only knows
//! about capitalization and the shape of a single line — cannot tell a *use*
//! of one from any other symbol. Both editor modes work around that by
//! scanning the buffer for the types it defines itself, but that necessarily
//! stops at the file boundary and is necessarily textual: a name shared by a
//! type and a function can fool it.
//!
//! The server does neither. The checker records a [`TypeUse`] at every
//! position its *grammar* put a type name — annotations (through
//! `types::parse_type_spanned`, which keeps each written name's exact source
//! span), definition headers, `Type::member` heads, ctor patterns — and
//! resolves each against the registry before recording. A position is only
//! ever highlighted because the checker knows a type is named there, so:
//!
//! - a type imported through `use` highlights (the module graph is resolved
//!   by the time the entry file is checked), and
//! - a *function* that happens to share a type's name can never light up —
//!   `(iter v)` is resolved as a call, and no annotation grammar ran there.
//!
//! This replaced the original text-scan design (match resolved names against
//! the document text), whose collision false-positives were documented as
//! accepted at the time; recording spans at resolution removes them outright.

use crate::types::Path;
use crate::Loc;

/// What kind of definition a type name resolved to. Chosen to line up with
/// the LSP standard token types (`struct`, `enum`, `interface`), so no editor
/// or theme needs to know anything typelisp-specific to colour them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Struct,
    Enum,
    Trait,
}

/// One occurrence of a user-defined type or trait name, recorded by the
/// checker at the moment it resolved the name. `loc` is the exact span of
/// the name itself (never the module prefix of a qualified spelling, never
/// `<...>` punctuation), so it can be handed to an editor as-is.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeUse {
    /// The resolved (fully-qualified) path of the type/trait.
    pub path: Path,
    pub kind: TypeKind,
    pub loc: Loc,
}

/// One highlighted occurrence of a type name, in LSP coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeToken {
    /// 0-based line.
    pub line: u32,
    /// 0-based column, in characters.
    pub character: u32,
    /// Length of the name, in characters.
    pub length: u32,
    pub kind: TypeKind,
}

/// The semantic tokens for `file`, assembled from the uses a check recorded.
///
/// Filters to `file` (a check also records uses inside `use`d dependency
/// files and — when the prelude is checked from source — the prelude itself;
/// each document only reports its own), converts the 1-based `Loc`s to
/// 0-based LSP positions, sorts into the document order the LSP delta
/// encoding requires, and drops duplicates — a generic template is re-checked
/// once per instantiation (`Checker::specialize_defun`), which records the
/// same annotation span once per pass.
pub fn file_type_tokens(uses: &[TypeUse], file: &str) -> Vec<TypeToken> {
    let mut tokens: Vec<TypeToken> = uses
        .iter()
        // A recorded span is always single-line (a token cannot contain a
        // newline); the guard is belt-and-braces against a degenerate Loc.
        .filter(|u| &*u.loc.file == file && !u.loc.is_degenerate() && u.loc.end_line == u.loc.line)
        .map(|u| TypeToken {
            line: u.loc.line - 1,
            character: u.loc.col - 1,
            length: u.loc.end_col - u.loc.col,
            kind: u.kind,
        })
        .collect();
    tokens.sort_by_key(|t| (t.line, t.character));
    tokens.dedup_by(|a, b| a.line == b.line && a.character == b.character);
    tokens
}

/// Encode tokens into the LSP wire format: five integers per token, with the
/// line and (within a line) the column stored as deltas from the previous one.
///
/// `type_index` maps a [`TypeKind`] to its position in the legend the server
/// advertised.
pub fn encode(tokens: &[TypeToken], type_index: impl Fn(TypeKind) -> u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(tokens.len() * 5);
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;
    for t in tokens {
        let delta_line = t.line - prev_line;
        let delta_col = if delta_line == 0 { t.character - prev_col } else { t.character };
        out.extend_from_slice(&[delta_line, delta_col, t.length, type_index(t.kind), 0]);
        prev_line = t.line;
        prev_col = t.character;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(file: &str, line: u32, col: u32, len: u32) -> Loc {
        Loc::new(std::sync::Arc::from(file), line, col).with_end(line, col + len)
    }

    fn use_at(file: &str, line: u32, col: u32, len: u32, kind: TypeKind) -> TypeUse {
        TypeUse { path: Path::root("t"), kind, loc: loc(file, line, col, len) }
    }

    #[test]
    fn tokens_are_filtered_to_the_requested_file_and_zero_based() {
        let uses = vec![
            use_at("/a.typl", 2, 5, 4, TypeKind::Struct),
            use_at("/b.typl", 1, 1, 4, TypeKind::Struct),
        ];
        assert_eq!(
            file_type_tokens(&uses, "/a.typl"),
            vec![TypeToken { line: 1, character: 4, length: 4, kind: TypeKind::Struct }]
        );
    }

    #[test]
    fn tokens_are_sorted_into_document_order_and_deduped() {
        // Recorded out of order (a dependency edit, a template re-check
        // recording the same span twice) — must come back sorted and unique,
        // since the LSP delta encoding cannot express a backwards jump.
        let uses = vec![
            use_at("/a.typl", 3, 1, 4, TypeKind::Enum),
            use_at("/a.typl", 1, 8, 4, TypeKind::Struct),
            use_at("/a.typl", 3, 1, 4, TypeKind::Enum),
            use_at("/a.typl", 1, 2, 5, TypeKind::Trait),
        ];
        let toks = file_type_tokens(&uses, "/a.typl");
        assert_eq!(
            toks,
            vec![
                TypeToken { line: 0, character: 1, length: 5, kind: TypeKind::Trait },
                TypeToken { line: 0, character: 7, length: 4, kind: TypeKind::Struct },
                TypeToken { line: 2, character: 0, length: 4, kind: TypeKind::Enum },
            ]
        );
    }

    #[test]
    fn a_degenerate_loc_is_never_a_token() {
        let mut u = use_at("/a.typl", 1, 1, 0, TypeKind::Struct);
        u.loc = Loc::new(std::sync::Arc::from("/a.typl"), 1, 1);
        assert!(file_type_tokens(&[u], "/a.typl").is_empty());
    }

    #[test]
    fn encoding_is_relative_to_the_previous_token() {
        let tokens = vec![
            TypeToken { line: 0, character: 11, length: 4, kind: TypeKind::Struct },
            TypeToken { line: 0, character: 20, length: 4, kind: TypeKind::Struct },
            TypeToken { line: 3, character: 2, length: 5, kind: TypeKind::Enum },
        ];
        let index = |k: TypeKind| match k {
            TypeKind::Struct => 0,
            TypeKind::Enum => 1,
            TypeKind::Trait => 2,
        };
        assert_eq!(
            encode(&tokens, index),
            vec![
                0, 11, 4, 0, 0, // first: absolute
                0, 9, 4, 0, 0, // same line: column delta
                3, 2, 5, 1, 0, // later line: column is absolute again
            ]
        );
    }
}
