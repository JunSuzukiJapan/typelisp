//! The resolution results a checked program still carries.
//!
//! Not an AST: checking lowers a read `Sexpr` ([`Value`](crate::Value)) into
//! cons-cell core IR, so a checked program is heap data, not a Rust tree. What
//! survives here is the handful of *answers* the checker has to hand its
//! consumers because they cannot re-derive them — a definition a name
//! resolved to ([`CompileTarget`]) and a pattern's shape ([`Pattern`], which
//! the core `pat-*` forms are built from).
//!
//! This was `src/check/ast.rs`, holding `Typed`/`Expr`/`Arm`. The plan for the
//! cons-cell interpreter had the file deleted outright; what actually happened
//! is that the tree went and these three did not, so Stage D renamed it rather
//! than leave a file called `ast` in a checker that builds no AST.

use typelisp_mem::Value;

use crate::check::repr::Repr;
use crate::{Path, Type};
/// A definition with a body of its own, named the way the checker resolved
/// it: a free function (or a macro, whose expander is one) by its path, or a
/// method by its type's path and its own name.
///
/// The target of a `(compile name)` / `(compile type::method)` form —
/// `Checker::resolve_callable` builds it out of the one name resolution — and
/// also how the compile pipeline names a node of its call graph and an entry
/// of a precompiled library. Nothing reads a definition back out of a spelled
/// name: a function `b` of module `a` and a method `b` of type `a` are two
/// different values of this type, where their names as text would be easy to
/// confuse.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CompileTarget {
    Fn(Path),
    Method(Path, String),
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
    /// A *value* pattern: matches when the scrutinee compares equal to
    /// something the checker could not reduce to an immediate word.
    ///
    /// This is what a `string`/`f64`/`symbol`/`bignum`/`ratio` literal
    /// pattern and the `(= expr)` escape both lower to. All five literal
    /// kinds are heap values whose machine word is a pointer or a box id, so
    /// `Pattern::Int`'s "compare the word" is not equality for any of them;
    /// and `(= expr)` is an arbitrary expression, which has no word until it
    /// is evaluated. Both therefore become the same thing: *run a test*.
    ///
    /// `form` is an already-checked `bool`-typed expression — `(equals NAME
    /// expr)`, resolved through the ordinary instance-method path, so a user
    /// type's own `Eq` impl is what compares it — and `name` is the variable
    /// that expression reads the scrutinee through. The matcher binds `name`
    /// to the value under test, evaluates `form`, and matches iff it is
    /// `true`; `name` is the checker's own (`$match-scrut`), never visible to
    /// the arm body.
    ///
    /// One fixed name for every guard, not a fresh one per site: each guard's
    /// binding is established immediately before its own `form` runs (the
    /// interpreter extends the environment per guard; the compiled side
    /// stores into a fresh slot and rebinds the name before compiling that
    /// form), so two guards in one pattern never read each other's value.
    Guard {
        name: String,
        form: Value,
    },
    /// A guarded arm, `(pat :when test body...)`: `pat` must match, and then
    /// `test` — an already-checked `bool` expression that sees `pat`'s
    /// bindings — must be true; otherwise the next arm is tried. Only ever
    /// the whole of an arm's pattern, never nested inside one.
    When {
        pat: Box<Pattern>,
        test: Value,
    },
    /// A constructor pattern, e.g. `(Some v)` / `(Cons a d)`.
    Ctor {
        type_name: Path,
        /// The type arguments this pattern's ADT is instantiated at, so the
        /// lowered node can spell the *value's* runtime identity
        /// (`type_key::type_key_of_type`) rather than just the ADT's path.
        ///
        /// Recorded for the same reason `field_types` is: the pattern site is
        /// where the instantiation is known. Without it a downcast tested only
        /// the base path, and `(the gen<i32> x)` accepted a `gen<string>` —
        /// then read its field as an `i32`.
        targs: Vec<Type>,
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
    /// The empty-list word, as a niched `Option<T>`'s `none`
    /// (`Repr::Niche`, `check/repr.rs`).
    ///
    /// Its own variant rather than a `Ctor { type_name: option, variant: 1 }`
    /// because a nullary ctor pattern carries no `field_types`, so nothing
    /// downstream could tell a niched `Option`'s `none` — the empty-list
    /// *immediate* — from a boxed one's (`Option<Option<T>>`), which is a
    /// real box. The scrutinee's type arguments are known in
    /// `check_ctor_pattern_fields` and nowhere later, so the decision is
    /// made there and recorded as a shape, the same reasoning
    /// `Ctor::field_types` records for the fields it can carry.
    Empty,
    /// A niched `Option<T>`'s `(some P)`: any word but the empty list, with
    /// `P` matched against the payload — the same word, read back through
    /// `T`'s representation (a fixnum untagged to a narrow integer, a float
    /// box opened, a tagged word passed through). The representation is
    /// recorded here because the pattern is the only place downstream that
    /// has to untag, and the only place that knows `T`.
    Some(Repr, Box<Pattern>),
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
