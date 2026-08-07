//! How a type's values are *represented* at runtime — the one vocabulary the
//! core IR carries in place of a `Type`.
//!
//! The cons-cell IR does not carry types. It does not need to: type *checking*
//! finishes in the checker, and the evaluator is uniform over `Value`. But the
//! compiler still has to know each binding's and field's representation, since
//! compiled code keeps locals in untagged native registers and has to agree
//! with the heap on what a word means. That is the whole residue of the type
//! system that has to survive lowering, and this is it.
//!
//! Two properties are the point of putting it here rather than leaving the
//! classification in `compile::ast_bridge`:
//!
//! * **The checker never learns the island's kind numbers.** It writes a
//!   [`Repr`] and nothing else; turning one into the integer
//!   `src/compiler.rs`'s `bind-params`/`compile-sexpr-field`/... read is the
//!   bridge's job, in [`Repr::binding_kind`]/[`Repr::field_kind`] below.
//! * **The two projections sit side by side.** They disagree, and that
//!   disagreement was invisible while each classifier was a separate function
//!   several hundred lines apart — see [`Repr::binding_kind`].

use std::collections::HashSet;

use crate::types::{Path, Type};

/// A runtime representation.
///
/// Finer than either projection needs on its own: `Struct`/`Enum`/`Dyn`/
/// `Scope`/`Sexpr`/`Str`/`Sym`/`Bignum`/`Ratio`/`Fn` all reach compiled code as
/// the same tagged pointer, so [`Repr::field_kind`] maps every one of them to
/// `6`. They are kept apart anyway because they are *different things* — the
/// two projections already treat some of them differently, and a vocabulary
/// that had pre-merged them could not express that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repr {
    /// `i32`/`i64` — a raw machine word.
    Int,
    /// `f64` — raw `f64::to_bits` in an `i64`.
    Float,
    Char,
    Bool,
    /// `()` — one value, so the word carries no information.
    Unit,
    /// An LLVM FFI object, held as an index into the interpreter's handle
    /// registry. A raw untraced `i64`, never a heap pointer — which is what
    /// keeps the compiler's own LLVM universe structurally out of the GC heap.
    Handle,
    Str,
    Sym,
    Bignum,
    Ratio,
    /// The built-in `sexpr` type.
    Sexpr,
    /// A `defstruct`, or a built-in that reuses that representation
    /// (`Vector<T>`, `cons-cell<K,V>`).
    Struct,
    /// `Option`/`Result`/a concrete error type/a user `defenum`.
    Enum,
    /// A trait object — a `BoxedObj::Dyn` fat box.
    Dyn,
    /// `Scope<V>`, the compiler's own binding-frame stack.
    Scope,
    /// A function value: a `BoxedObj::CompiledClosure` reference.
    Fn,
    /// No compiled representation. The one type that genuinely reaches this is
    /// a still-generic type variable — a `defstruct`'s own `T` field compiled
    /// from the generic definition, which needs monomorphization rather than
    /// more type information at the use site.
    None,
}

impl Repr {
    /// Classify `ty`.
    ///
    /// `structs`/`enums` are the type paths the checker resolved to
    /// `AdtKind::Struct`/`AdtKind::Sum`, passed as plain pre-resolved sets so
    /// this needs no live `Registry`.
    ///
    /// Order matters: the guards below are not disjoint as written (an LLVM
    /// handle and a `Scope<V>` are both `Type::Named`, and so is every struct
    /// and enum), so each earlier arm is also an exclusion for the later ones.
    pub fn of(ty: &Type, structs: &HashSet<Path>, enums: &HashSet<Path>) -> Repr {
        match ty {
            _ if ty.is_integer() => Repr::Int,
            _ if crate::compile::ast_bridge::is_llvm_handle_ty(ty) => Repr::Handle,
            _ if ty.is_float() => Repr::Float,
            Type::Char => Repr::Char,
            Type::Bool => Repr::Bool,
            Type::Unit => Repr::Unit,
            Type::Str => Repr::Str,
            Type::Symbol => Repr::Sym,
            Type::Bignum => Repr::Bignum,
            Type::Ratio => Repr::Ratio,
            Type::Named(p, _) if *p == Path::root("sexpr") => Repr::Sexpr,
            _ if crate::compile::ast_bridge::is_scope_ty(ty) => Repr::Scope,
            Type::Named(p, _) if structs.contains(p) => Repr::Struct,
            // Not `enums.contains` alone: `Option`/`Result` and the concrete
            // error types are recognized structurally, so they classify
            // correctly even where no set is available.
            _ if crate::compile::ast_bridge::is_enum_ty(ty, enums) => Repr::Enum,
            Type::Dyn(..) => Repr::Dyn,
            Type::Fn(..) => Repr::Fn,
            _ => Repr::None,
        }
    }

    /// The tag written into the core IR, and read back by [`Repr::parse`].
    pub fn tag(self) -> &'static str {
        match self {
            Repr::Int => "int",
            Repr::Float => "float",
            Repr::Char => "char",
            Repr::Bool => "bool",
            Repr::Unit => "unit",
            Repr::Handle => "handle",
            Repr::Str => "str",
            Repr::Sym => "sym",
            Repr::Bignum => "bignum",
            Repr::Ratio => "ratio",
            Repr::Sexpr => "sexpr",
            Repr::Struct => "struct",
            Repr::Enum => "enum",
            Repr::Dyn => "dyn",
            Repr::Scope => "scope",
            Repr::Fn => "fn",
            Repr::None => "none",
        }
    }

    /// The inverse of [`Repr::tag`].
    pub fn parse(tag: &str) -> Option<Repr> {
        const ALL: [Repr; 17] = [
            Repr::Int,
            Repr::Float,
            Repr::Char,
            Repr::Bool,
            Repr::Unit,
            Repr::Handle,
            Repr::Str,
            Repr::Sym,
            Repr::Bignum,
            Repr::Ratio,
            Repr::Sexpr,
            Repr::Struct,
            Repr::Enum,
            Repr::Dyn,
            Repr::Scope,
            Repr::Fn,
            Repr::None,
        ];
        ALL.into_iter().find(|r| r.tag() == tag)
    }

    /// Whether a binding of this representation needs a GC root pushed around
    /// it — `compiler.rs`'s `bind-params`/`bind-let-values`/`retain-bindings`/
    /// `release-bindings` read the number this returns.
    ///
    /// **This does not agree with [`Repr::field_kind`], and the disagreement is
    /// deliberate for now.** `Struct`, `Dyn`, and `Sym` are tagged heap
    /// pointers — `field_kind` calls all three the passthrough kind `6` — yet
    /// they get no root here. The classifier this replaces could not have done
    /// otherwise: it was not even given the struct set, so it had no way to
    /// recognize a `defstruct`-typed binding.
    ///
    /// Whether that is exploitable is unsettled. `tests/compiled_binding_gc_test.rs`
    /// is the attempt to exploit it and does not manage to; see its module
    /// comment for what that does and does not establish. It is left alone
    /// because these numbers are an *interface*: the committed island bitcode
    /// (`src/compiler_island.bc`) pairs a push in `bind-let-values` with a pop
    /// in `release-bindings` off the same number, and that artifact is frozen
    /// until the compiler is rebuilt. Settle it then, with both halves rebuilt
    /// together.
    pub fn binding_kind(self) -> i64 {
        /// No bookkeeping at a binding boundary.
        const KIND_PLAIN: i64 = 0;
        /// Push/pop a GC root: the value may point into the managed heap.
        const KIND_SEXPR: i64 = 2;
        match self {
            Repr::Sexpr
            | Repr::Str
            | Repr::Bignum
            | Repr::Ratio
            | Repr::Fn
            | Repr::Scope
            | Repr::Enum => KIND_SEXPR,
            Repr::Int
            | Repr::Float
            | Repr::Char
            | Repr::Bool
            | Repr::Unit
            | Repr::Handle
            | Repr::Sym
            | Repr::Struct
            | Repr::Dyn
            | Repr::None => KIND_PLAIN,
        }
    }

    /// How a value of this representation is tagged and untagged when it
    /// crosses a `BoxedObj::Struct` field boundary — `compiler.rs`'s
    /// `compile-tag-struct-field` (encode) and `compile-sexpr-field` (decode).
    ///
    /// The numbers are `Sexpr`'s own variant numbering (`registry::sexpr_def`:
    /// `1`=int `2`=float `3`=char `4`=bool `6`=str) rather than a parallel
    /// scheme, so those two functions reuse the same per-variant bit
    /// manipulation instead of duplicating it. `11` is the first number past
    /// that numbering, for `Unit`, which is not a `Sexpr` variant and so has
    /// none to borrow — `0` being taken by the "not representable" case.
    pub fn field_kind(self) -> i64 {
        match self {
            // A handle joins the integer kind: its representation *is* a plain
            // untraced `i64`, so the int tag/detag bit ops are exactly right
            // and no GC root is ever wanted.
            Repr::Int | Repr::Handle => 1,
            Repr::Float => 2,
            Repr::Char => 3,
            Repr::Bool => 4,
            // Everything already boxed or already tagged passes through
            // untouched — both directions leave a kind-`6` value alone.
            Repr::Str
            | Repr::Sym
            | Repr::Bignum
            | Repr::Ratio
            | Repr::Sexpr
            | Repr::Struct
            | Repr::Enum
            | Repr::Dyn
            | Repr::Scope
            | Repr::Fn => 6,
            // Both directions ignore the word they are handed and emit a
            // constant: a unit type has exactly one value, already known from
            // the declared type, so the slot carries no information and only
            // has to hold something the GC can decode safely.
            Repr::Unit => 11,
            Repr::None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sets() -> (HashSet<Path>, HashSet<Path>) {
        let mut structs = HashSet::new();
        structs.insert(Path::root("point"));
        let mut enums = HashSet::new();
        enums.insert(Path::root("my-enum"));
        (structs, enums)
    }

    #[test]
    fn every_tag_round_trips() {
        for tag in [
            "int", "float", "char", "bool", "unit", "handle", "str", "sym", "bignum", "ratio",
            "sexpr", "struct", "enum", "dyn", "scope", "fn", "none",
        ] {
            let r = Repr::parse(tag).unwrap_or_else(|| panic!("no Repr for {:?}", tag));
            assert_eq!(r.tag(), tag);
        }
        assert_eq!(Repr::parse("not-a-repr"), None);
    }

    #[test]
    fn scalars_and_adts_classify_by_their_declared_type() {
        let (structs, enums) = sets();
        let of = |t: Type| Repr::of(&t, &structs, &enums);
        assert_eq!(of(Type::I32), Repr::Int);
        assert_eq!(of(Type::I64), Repr::Int);
        assert_eq!(of(Type::F64), Repr::Float);
        assert_eq!(of(Type::Char), Repr::Char);
        assert_eq!(of(Type::Bool), Repr::Bool);
        assert_eq!(of(Type::Unit), Repr::Unit);
        assert_eq!(of(Type::Str), Repr::Str);
        assert_eq!(of(Type::Symbol), Repr::Sym);
        assert_eq!(of(Type::Bignum), Repr::Bignum);
        assert_eq!(of(Type::Ratio), Repr::Ratio);
        assert_eq!(of(Type::Named(Path::root("sexpr"), vec![])), Repr::Sexpr);
        assert_eq!(of(Type::Named(Path::root("point"), vec![])), Repr::Struct);
        assert_eq!(of(Type::Named(Path::root("my-enum"), vec![])), Repr::Enum);
        // Recognized structurally rather than through the set — passing an
        // empty one must not turn `Option`/`Result` into `Repr::None`.
        let none: HashSet<Path> = HashSet::new();
        assert_eq!(
            Repr::of(&Type::Named(Path::root("option"), vec![Type::I32]), &none, &none),
            Repr::Enum
        );
        assert_eq!(
            Repr::of(&Type::Named(Path::root("result"), vec![Type::I32, Type::Str]), &none, &none),
            Repr::Enum
        );
        assert_eq!(of(Type::Fn(vec![], None, Box::new(Type::Unit))), Repr::Fn);
        // An unresolved name is the genuine representation gap: a still-generic
        // type variable, which needs monomorphization rather than more type
        // information here.
        assert_eq!(of(Type::Named(Path::root("T"), vec![])), Repr::None);
    }

    /// The `Struct`/`Dyn`/`Sym` rows are the asymmetry `binding_kind`'s doc
    /// comment describes. Pinned as a test so that changing either projection
    /// has to acknowledge the other.
    #[test]
    fn the_two_projections_disagree_only_where_documented() {
        let heap_ish = [
            Repr::Str,
            Repr::Sym,
            Repr::Bignum,
            Repr::Ratio,
            Repr::Sexpr,
            Repr::Struct,
            Repr::Enum,
            Repr::Dyn,
            Repr::Scope,
            Repr::Fn,
        ];
        // Every one of them is the passthrough kind at a field boundary...
        for r in heap_ish {
            assert_eq!(r.field_kind(), 6, "{:?} should be the passthrough field kind", r);
        }
        // ...but only some of them get a GC root at a binding boundary.
        let rooted: Vec<Repr> = heap_ish.into_iter().filter(|r| r.binding_kind() == 2).collect();
        assert_eq!(
            rooted,
            vec![Repr::Str, Repr::Bignum, Repr::Ratio, Repr::Sexpr, Repr::Enum, Repr::Scope, Repr::Fn],
            "the unrooted heap representations should be exactly Struct/Sym/Dyn"
        );
    }
}
