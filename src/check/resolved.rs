//! The resolution results a checked program still carries.
//!
//! Not an AST: checking lowers a read `Sexpr` ([`Value`](crate::Value)) into
//! cons-cell core IR, so a checked program is heap data, not a Rust tree. What
//! survives here is the handful of *answers* the checker has to hand its
//! consumers because they cannot re-derive them — a name resolved to a target
//! ([`Ref`], [`CompileTarget`]) and a pattern's shape ([`Pattern`], which the
//! core `pat-*` forms are built from).
//!
//! This was `src/check/ast.rs`, holding `Typed`/`Expr`/`Arm`. The plan for the
//! cons-cell interpreter had the file deleted outright; what actually happened
//! is that the tree went and these three did not, so Stage D renamed it rather
//! than leave a file called `ast` in a checker that builds no AST.

use crate::{Path, Type};
/// A reference to a free function or global variable — everything both
/// `Interp` and the compile pipeline need to resolve one, kept as two
/// independent halves rather than a single collapsed `Path`:
///
/// - `written`/`home` are what `eval::scope::ModuleScope` (see that
///   module's doc comment) uses to *re-derive* the target itself at
///   runtime, walking its own tree from `home` — mirroring
///   `Checker::resolve_fn`/`resolve_fn_path`/`resolve_global`/
///   `resolve_global_path` (checker.rs:1061-1093, 1374-1410) — rather than
///   trusting `resolved` as a lookup key. `written` is the name exactly as
///   it appeared at the reference site (`["inc"]` bare, `["m","inc"]`
///   qualified) and `home` is the lexically enclosing module's own raw
///   segments (`Checker::ns` at the point this reference was checked, e.g.
///   `[]` at the root) — the walk's starting point. Plain `Vec<String>`
///   rather than [`Path`] because the root module has *zero* segments,
///   which `Path` (always at least one — the local name) can't represent.
/// - `resolved` is the checker's own fully-qualified answer, kept only for
///   compile-time-only consumers that have no way to redo resolution
///   themselves: `compile::core_bridge` (deliberately `Registry`-free),
///   `compile::aot`, and `check::locate`'s LSP goto-definition. `Interp`
///   itself never reads this field.
#[derive(Clone, Debug, PartialEq)]
pub struct Ref {
    pub written: Vec<String>,
    pub home: Vec<String>,
    pub resolved: Path,
}

impl Ref {
    /// A synthesized reference with no real "as written" source form (the
    /// checker's own internal rewrites — e.g. `sexpr-cons`, quasiquote's
    /// `append_fq` — where `resolved` is already exactly right and there's
    /// no user-facing shadowing concern `written`+`home` would need to
    /// disambiguate). `home` is `resolved`'s own parent module, so a
    /// bare-name ancestor-walk starting there still finds the same target
    /// as direct descent would.
    pub fn synthetic(resolved: Path) -> Ref {
        let written = vec![resolved.last_segment().to_string()];
        let home = resolved.parent().to_vec();
        Ref { written, home, resolved }
    }
}

/// The target of a `(compile name)` / `(compile type::method)` form —
/// `Checker::check_compile` builds this from whichever resolution it
/// already performs for its own generic-target check (`resolve_fn`/
/// `resolve_type_name`), instead of discarding that result and handing
/// `Interp` a bare name string to re-resolve by an unqualified, module-blind
/// search (the bug this replaces — see `docs/implementation-log.md`).
/// `Fn`'s `Ref` gets the exact same independent `written`+`home`
/// re-resolution at runtime as an ordinary `call`; `Method`'s
/// `type_name` is already a fully resolved type identity (never searched,
/// same reasoning as `assoc`/`MethodRef`), so only `home` is needed
/// for the `pub`-or-`in_scope` check.
#[derive(Clone, Debug, PartialEq)]
pub enum CompileTarget {
    Fn(Ref),
    Method { type_name: Path, method: String, home: Vec<String> },
}

/// A match pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    /// `_` — matches anything, binds nothing.
    Wildcard,
    /// A variable pattern — matches anything, binding it to the named
    /// variable.
    Bind(String),
    Int(i64),
    Bool(bool),
    Char(char),
    /// A constructor pattern, e.g. `(Some v)` / `(Cons a d)`.
    Ctor {
        type_name: Path,
        variant: usize,
        args: Vec<Pattern>,
        /// Per-field: the field's declared type with the scrutinee's own type
        /// arguments substituted in (`Checker::check_ctor_pattern`'s `subst`).
        ///
        /// Kept because the *compiled* side needs it and cannot re-derive it:
        /// the `pat-ctor` core form carries one representation per field, and a
        /// generic ADT's definition cannot supply them — `Option`'s `Some` field
        /// is declared `T`, and monomorphization erases, so a definition-keyed
        /// table could hold the fields of only one instantiation. This pattern
        /// site is where the instantiation is known. The interpreter consults
        /// none of it: it binds a field as the value it already is.
        ///
        /// A companion `sexpr_fields: Vec<bool>` sat here too — the same fact
        /// reduced to "is this field declared `Sexpr`" — for an evaluator that
        /// decoded a field by its static type. With one value world that decode
        /// is the identity, so the bit had no reader and is gone.
        field_types: Vec<Type>,
        /// Whether this `Ctor` pattern is a Sexpr-downcast (`Checker::
        /// resolve_sexpr_downcast_ctor`/`check_ctor_pattern_fields`) rather
        /// than an ordinary same-ADT pattern. Both shapes look identical
        /// otherwise — same `type_name`/`variant`/fields — the only
        /// difference is whether the checker already *knew* the scrutinee
        /// was exactly `type_name` (ordinary — the ten-thousand pre-existing
        /// call sites, where `Interp::match_pattern`'s `type_name` guard is
        /// a provably-redundant no-op) or discovered it inside a
        /// heterogeneous `Sexpr` (downcast — where that guard, and the
        /// compiled side's `rt_sexpr_instance_test` call
        /// (`core_bridge::translate_ctor_pattern`/`compiler.rs`'s
        /// `compile-pattern-test` emit only when this is `true`, is load-
        /// bearing). Not recoverable from `type_name`/the enclosing scrutinee
        /// type alone: nesting can mix ordinary and downcast patterns at
        /// different depths (e.g. an ordinary struct match whose own
        /// `Sexpr`-declared field is itself downcast-matched).
        downcast: bool,
    },
    /// `(the Type pattern)` against a `Sexpr` scrutinee — a whole-value
    /// downcast extraction (`Checker::check_ctor_pattern`'s Sexpr-downcast
    /// branch), the only way to pull a `Vector<T>`/`HashTable<K,V>` back out
    /// of a heterogeneous `Sexpr` (they have no field-destructuring ctor
    /// pattern shape) and the general form a `defstruct`/`defenum` downcast
    /// can also use to keep a mutable box's identity rather than
    /// destructuring its fields. Matched by comparing `Type`'s ADT `Path`
    /// against the runtime box's own `type_name` string — generic type
    /// arguments are not
    /// runtime-checked (the heap box's `type_name` never encodes them, the
    /// same erasure every other heap-repr ADT instance already has).
    TypeTest(Type, Box<Pattern>),
}
