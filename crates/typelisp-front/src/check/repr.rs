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
//! classification in `compile::core_bridge`:
//!
//! * **The checker never learns the island's kind numbers.** It writes a
//!   [`Repr`] and nothing else; turning one into the integer
//!   `src/compiler.rs`'s `bind-params`/`compile-sexpr-field`/... read is the
//!   bridge's job, in [`Repr::binding_kind`]/[`Repr::field_kind`] below.
//! * **The two projections are derived, not written.** Both come from one
//!   [`Repr::class`], so they cannot disagree. They used to be independent
//!   hand-written tables and did disagree — in four places, invisibly, because
//!   the two were separate functions several hundred lines apart. One of those
//!   disagreements made a `HashTable<K,V>` field of a `defstruct` fail to
//!   compile while claiming the type had no representation at all.

use std::collections::HashSet;

use typelisp_mem::{Error, Heap, Value};

use crate::check::registry::AdtKind;
use crate::types::{path_is_builtin, path_is_builtin_any, Path, Type, LLVM_HANDLE_TYPES};

/// Whether `ty`'s values are LLVM FFI objects held as registry handles — a
/// plain untraced `i64`, never a heap pointer, which is what keeps the
/// compiler's own LLVM universe structurally out of the GC heap.
///
/// Lives here rather than in the bridge because it is a statement about a
/// type's *representation*, and [`Repr::of`] is where every such statement is
/// made. Closure-JIT tier classification must still *exclude* these types
/// (`Interp::is_jit_tier_ty`) — an interpreted `compile-function` run
/// JIT-compiling its own `labels` siblings would recurse into itself.
pub fn is_llvm_handle_ty(ty: &Type) -> bool {
    matches!(ty, Type::Named(p, _) if path_is_builtin_any(p, &LLVM_HANDLE_TYPES))
}

/// Whether `ty` is `Option`/`Result`/a built-in error type/a user `defenum`,
/// the latter decided by `kind_of` (see [`Repr::of_by`]).
///
/// `kind_of` alone is not enough: `Option`/`Result` and the concrete error
/// types are recognized *structurally*, so a caller whose lookup does not know
/// them still classifies them correctly.
pub fn is_enum_ty_by(ty: &Type, kind_of: &dyn Fn(&Path) -> Option<AdtKind>) -> bool {
    match ty {
        Type::Named(p, args) if path_is_builtin(p, "option") && args.len() == 1 => true,
        Type::Named(p, args) if path_is_builtin(p, "result") && args.len() == 2 => true,
        Type::Named(p, _) if crate::check::registry::is_builtin_error_type(p) => true,
        Type::Named(p, _) => kind_of(p) == Some(AdtKind::Sum),
        _ => false,
    }
}

/// A runtime representation.
///
/// Finer than either projection needs on its own: `Struct`/`Enum`/`Dyn`/
/// `Scope`/`Sexpr`/`Str`/`Sym`/`Bignum`/`Ratio`/`Fn` all reach compiled code as
/// the same tagged pointer, so [`Repr::field_kind`] maps every one of them to
/// `6`. They are kept apart anyway because they are *different things* — the
/// two projections already treat some of them differently, and a vocabulary
/// that had pre-merged them could not express that.
///
/// # Why three variants carry an element representation
///
/// Neither projection below looks inside a [`Repr::Scope`]/[`Repr::Vector`]/
/// [`Repr::HashTable`] — a container is one tagged pointer whatever it holds.
/// The *bridge* does. `src/compiler.rs` has no compiled body for a
/// `Vector<T>`/`HashTable<K,V>` builtin method, so those calls lower to
/// dedicated `vector-op`/`hashtable-op` nodes carrying the element kind the
/// generated code tags and untags the element with; and a `Scope<V>` whose `V`
/// is an LLVM handle routes to the island's frozen `native-scope` op rather
/// than the generic method path, which is a decision about `V` alone. The old
/// bridge read all three straight off the receiver's `Type`. Nothing
/// downstream of the checker has a `Type` any more, so the element
/// representation has to be written down here or it is gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Repr {
    /// Every fixed integer width — a raw machine word.
    ///
    /// Named `int-any-width` in the core IR: one representation really does
    /// serve all six, because the word already holds the number the type
    /// names (sign- or zero-extended, `types::normalize_int`). What a width
    /// still decides is how the value is *boxed*, and that is
    /// [`Repr::field_kind`]'s business, not this tag's.
    ///
    /// `Narrow` and not `Int`, since `int` became a type of its own: the
    /// six widths are the ones narrower than a word, and [`Repr::Int`] is
    /// the one that is not a raw word at all.
    Narrow,
    /// `int` — the arbitrary-precision integer, as the tagged word it
    /// already is: a fixnum (bit 0 clear, 63-bit payload) or a
    /// `BoxedObj::Bignum` reference, whichever the value's size demands
    /// (`Heap::canonical_int` is the one place that decides, and a box
    /// holding a fixnum-range value is a corruption).
    ///
    /// Tagged in a register, unlike every other numeric representation: the
    /// word has to be able to say "I am a box" on its own, because a value
    /// crosses the fixnum boundary in the middle of an `add`. That is also
    /// why it is collectable — the collector has to find the box through a
    /// frame slot — and why an `int` in a struct field or a `Sexpr` is the
    /// same bits as in a register (passthrough, `field_kind` 6).
    Int,
    /// `f64` — raw `f64::to_bits` in an `i64`.
    F64,
    /// `f32` — also a raw `f64::to_bits` pattern, of the binary32 value
    /// widened into binary64.
    ///
    /// A separate representation from [`Repr::F64`] even though the carrier
    /// word is identical, because the carrier is not the only thing a `Repr`
    /// decides: a compiled value coming *back* has to be put in a box, and
    /// the box has to say which width it is. One `Float` variant meant the
    /// return path had to guess, and it guessed `f64`.
    F32,
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
    Ratio,
    /// `random-state` — a boxed seed cell, tagged like [`Repr::Int`] and
    /// [`Repr::Ratio`] and collectable like both. It has no `Type::Named`
    /// spelling, which is why it needs a variant of its own rather than
    /// falling into the struct arm; without one it landed on [`Repr::None`]
    /// and `(match state ((some s) ...))` over an `Option<random-state>` —
    /// the prelude's `random` — aborted the island with "field type is not
    /// representable in compiled code yet".
    RandomState,
    /// The built-in `sexpr` type.
    Sexpr,
    /// A `defstruct`, or a built-in that reuses that representation
    /// (`Vector<T>`, `cons-cell<K,V>`).
    Struct,
    /// `Option`/`Result`/a concrete error type/a user `defenum`.
    Enum,
    /// A trait object — a `BoxedObj::Dyn` fat box.
    Dyn,
    /// `Scope<V>`, the compiler's own binding-frame stack, over `V`'s
    /// representation.
    Scope(Box<Repr>),
    /// `Vector<T>` over `T`'s representation. Represented exactly like a
    /// [`Repr::Struct`] — kept apart only because its element matters; see the
    /// type's own doc comment.
    Vector(Box<Repr>),
    /// `HashTable<K,V>` over `K`'s and `V`'s representations.
    HashTable(Box<Repr>, Box<Repr>),
    /// A function value: a `BoxedObj::CompiledClosure` reference.
    Fn,
    /// A raw 64-bit machine word: the FFI's `ptr`/`c-long`/`c-ulong`.
    ///
    /// Like [`Repr::Handle`] it is untagged and untraced, and unlike it there
    /// is no registry behind it — the word is whatever C said. What separates
    /// the two is [`Repr::field_kind`]: a handle is small enough to survive
    /// tagging and is stored like an integer, and this is not. All 64 bits are
    /// meaningful, so there is nowhere to put a 3-bit tag, and the answer to
    /// "how is this stored in a tagged slot" is that it is not stored in one
    /// at all. `Checker` refuses the declarations that would ask
    /// (`defstruct`/`defenum` fields, `defvar`, `Vector<ptr>`).
    ///
    /// One variant for all three types because the boundary asks one question
    /// — is this word tagged — and the answer is the same. The *width and
    /// sign*, which the FFI thunk does need, are a different question, asked
    /// of the declared C type instead (`crate::compile::ffi::CType`).
    RawWord,
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
        Repr::of_by(ty, &|p| {
            if structs.contains(p) {
                Some(AdtKind::Struct)
            } else if enums.contains(p) {
                Some(AdtKind::Sum)
            } else {
                None
            }
        })
    }

    /// Classify `ty`, resolving a nominal type's kind through `kind_of`.
    ///
    /// The form the checker uses: it has a live [`Registry`](crate::Registry)
    /// that is still being *added to* as a file is checked (every `defstruct`
    /// registers one more type), so it cannot hand over a pair of frozen sets
    /// without either rebuilding them per node or risking a stale answer for a
    /// type defined earlier in the same file. A lookup asks the registry at the
    /// moment the question is put.
    ///
    /// Order matters: the guards below are not disjoint as written (an LLVM
    /// handle and a `Scope<V>` are both `Type::Named`, and so is every struct
    /// and enum), so each earlier arm is also an exclusion for the later ones.
    pub fn of_by(ty: &Type, kind_of: &dyn Fn(&Path) -> Option<AdtKind>) -> Repr {
        match ty {
            // Ahead of `is_integer`, which these deliberately fail (they carry
            // no arithmetic), and ahead of everything else for the same reason
            // the `Handle` arm is early: falling through to the tagged
            // catch-all would shift a pointer left by three.
            Type::Ptr | Type::CLong | Type::CULong => Repr::RawWord,
            Type::Int => Repr::Int,
            _ if ty.is_integer() => Repr::Narrow,
            _ if is_llvm_handle_ty(ty) => Repr::Handle,
            Type::F32 => Repr::F32,
            _ if ty.is_float() => Repr::F64,
            Type::Char => Repr::Char,
            Type::Bool => Repr::Bool,
            Type::Unit => Repr::Unit,
            Type::Str => Repr::Str,
            Type::Symbol => Repr::Sym,
            Type::Ratio => Repr::Ratio,
            Type::RandomState => Repr::RandomState,
            Type::Named(p, _) if *p == Path::root("sexpr") => Repr::Sexpr,
            // `Option<Sexpr>` is represented *exactly* like a `Sexpr`: `none`
            // is the empty-list immediate (`Value::Empty`), `some v` is `v`
            // itself. That is the niche the empty list vacates when `nil`
            // leaves `Sexpr` (docs/dev/null-elimination-plan.md §3.1), and
            // saying it here — rather than inventing a variant — is what
            // keeps every consumer unchanged: `field_kind` 6 and
            // `binding_kind` 2 are `Repr::Sexpr`'s own numbers, and the
            // crossing decode already handles `Sexpr` and `Enum` in one arm.
            //
            // **Exactly `Option<sexpr>`, never a payload that merely
            // *represents* like one.** Were this written as "the payload's
            // repr is `Sexpr`", `Option<Option<Sexpr>>` would niche at the
            // outer level too and its two `none`s would collide on the same
            // `Value::Empty`. Requiring the argument to be the `sexpr` type
            // itself drops the outer one through to `Repr::Enum` (a real
            // box), which is the same rule Rust's niche optimization uses.
            // `Option<Option<i32>>` is an existing, tested shape
            // (`tests/compile_test.rs`) and `HashTable<K,Option<V>>::get`
            // produces one, so this is reachable, not hypothetical.
            Type::Named(p, args)
                if path_is_builtin(p, "option")
                    && args.len() == 1
                    && matches!(&args[0], Type::Named(a, _) if *a == Path::root("sexpr")) =>
            {
                Repr::Sexpr
            }
            // The three parametric builtins, ahead of the struct/enum arms
            // that would otherwise swallow them. Each classifies to the same
            // two kinds it did before it had a variant of its own — a
            // `Vector<T>` is `AdtKind::Struct`, and a `HashTable<K,V>` is
            // `AdtKind::Sum` but not one of the structurally-recognized enums,
            // so they land on [`Repr::Struct`]'s and [`Repr::None`]'s numbers
            // respectively. Their being ahead is also what keeps a
            // registry-backed `kind_of` (which *does* know `hashtable`/`scope`
            // are `Sum`) answering the same as the pre-resolved set form.
            Type::Named(p, args) if path_is_builtin(p, "scope") && args.len() == 1 => {
                Repr::Scope(Box::new(Repr::of_by(&args[0], kind_of)))
            }
            Type::Named(p, args) if path_is_builtin(p, "vector") && args.len() == 1 => {
                Repr::Vector(Box::new(Repr::of_by(&args[0], kind_of)))
            }
            Type::Named(p, args) if path_is_builtin(p, "hashtable") && args.len() == 2 => {
                Repr::HashTable(Box::new(Repr::of_by(&args[0], kind_of)), Box::new(Repr::of_by(&args[1], kind_of)))
            }
            Type::Named(p, _) if kind_of(p) == Some(AdtKind::Struct) => Repr::Struct,
            // Not the lookup alone: `Option`/`Result` and the concrete error
            // types are recognized structurally, so they classify correctly
            // even where the lookup does not know them.
            _ if is_enum_ty_by(ty, kind_of) => Repr::Enum,
            Type::Dyn(..) => Repr::Dyn,
            Type::Fn(..) => Repr::Fn,
            _ => Repr::None,
        }
    }

    /// This representation's head tag: the whole spelling for a simple one,
    /// the head of the list for a parametric one.
    pub fn tag(&self) -> &'static str {
        match self {
            Repr::Narrow => "int-any-width",
            Repr::Int => "int",
            Repr::F64 => "f64",
            Repr::F32 => "f32",
            Repr::Char => "char",
            Repr::Bool => "bool",
            Repr::Unit => "unit",
            Repr::Handle => "handle",
            Repr::RawWord => "raw-word",
            Repr::Str => "str",
            Repr::Sym => "sym",
            Repr::Ratio => "ratio",
            Repr::RandomState => "random-state",
            Repr::Sexpr => "sexpr",
            Repr::Struct => "struct",
            Repr::Enum => "enum",
            Repr::Dyn => "dyn",
            Repr::Scope(_) => "scope",
            Repr::Vector(_) => "vector",
            Repr::HashTable(..) => "hashtable",
            Repr::Fn => "fn",
            Repr::None => "none",
        }
    }

    /// Every simple representation, for [`Repr::read`] and for a test that
    /// wants to enumerate the vocabulary.
    pub const SIMPLE: [Repr; 19] = [
        Repr::Narrow,
        Repr::Int,
        Repr::RawWord,
        Repr::F64,
        Repr::F32,
        Repr::Char,
        Repr::Bool,
        Repr::Unit,
        Repr::Handle,
        Repr::Str,
        Repr::Sym,
        Repr::Ratio,
        Repr::RandomState,
        Repr::Sexpr,
        Repr::Struct,
        Repr::Enum,
        Repr::Dyn,
        Repr::Fn,
        Repr::None,
    ];

    /// Write this representation as the core IR spells it: the bare symbol
    /// `int`, or the list `(scope handle)` / `(vector int)` /
    /// `(hashtable str sexpr)`.
    pub fn write(&self, heap: &mut Heap) -> Result<Value, Error> {
        match self {
            Repr::Scope(v) => self.write_parametric(heap, &[v]),
            Repr::Vector(t) => self.write_parametric(heap, &[t]),
            Repr::HashTable(k, v) => self.write_parametric(heap, &[k, v]),
            simple => Ok(simple.write_head(heap)),
        }
    }

    fn write_head(&self, heap: &mut Heap) -> Value {
        heap.intern_symbol(self.tag())
    }

    fn write_parametric(&self, heap: &mut Heap, args: &[&Repr]) -> Result<Value, Error> {
        let mut items = Vec::with_capacity(args.len() + 1);
        let mut scope = typelisp_mem::RootScope::new(heap);
        items.push(self.write_head(&mut scope));
        for a in args {
            let v = a.write(&mut scope)?;
            scope.push_root(v);
            items.push(v);
        }
        crate::check::core::list(&mut scope, &items)
    }

    /// The inverse of [`Repr::write`]. `None` for anything that is not a
    /// representation — a misspelled tag, or a parametric head with the wrong
    /// number of arguments.
    pub fn read(heap: &Heap, v: Value) -> Option<Repr> {
        match v {
            Value::Symbol(id) => {
                let name = heap.symbol_name(id);
                Repr::SIMPLE.iter().find(|r| r.tag() == name).cloned()
            }
            Value::Cons(_) => {
                let items = heap.list_to_vec(v).ok()?;
                let head = match items.first()? {
                    Value::Symbol(id) => heap.symbol_name(*id),
                    _ => return None,
                };
                let arg = |i: usize| Repr::read(heap, *items.get(i)?).map(Box::new);
                match (head, items.len()) {
                    ("scope", 2) => Some(Repr::Scope(arg(1)?)),
                    ("vector", 2) => Some(Repr::Vector(arg(1)?)),
                    ("hashtable", 3) => Some(Repr::HashTable(arg(1)?, arg(2)?)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// What compiled code has to do with a value of this representation at a
    /// boundary — **the one decision per `Repr`**, from which both numbers the
    /// island reads are derived ([`Repr::field_kind`], [`Repr::binding_kind`]).
    ///
    /// This used to be two independent hand-written tables, and they disagreed.
    /// `Struct`, `Vector<T>` and `Dyn` were the passthrough kind `6` at a field
    /// boundary — a tagged heap pointer — while getting no GC root at a binding
    /// boundary, and `HashTable<K,V>` was classified as having no compiled
    /// representation at all (see [`Repr::field_kind`] for what that cost).
    /// Neither disagreement was a decision anyone took: the classifier this
    /// replaced was not given the struct set, so it could not recognize a
    /// `defstruct`-typed binding, and the numbers were carried over unexamined.
    ///
    /// Collapsing the two tables into this one is what makes that class of bug
    /// unwritable — a new `Repr` variant now forces exactly one judgement, and
    /// the two numbers cannot drift apart because neither is written down.
    fn class(&self) -> Class {
        match self {
            // A handle joins the integer class: its representation *is* a plain
            // untraced `i64`, so the int tag/detag bit ops are exactly right
            // and no GC root is ever wanted.
            Repr::Narrow | Repr::Handle => Class::Int,
            // *Not* `Class::Int`, which a handle is: an integer field is
            // stored tagged, and this word has no room for a tag.
            Repr::RawWord => Class::RawWord,
            Repr::F64 | Repr::F32 => Class::Float,
            Repr::Char => Class::Char,
            Repr::Bool => Class::Bool,
            // A unit type has exactly one value, already known from the
            // declared type, so the word carries no information.
            Repr::Unit => Class::Unit,
            // Already-tagged words, and the collector can reclaim what each
            // one points at.
            //
            // `HashTable<K,V>` belongs here for the same reason a `defstruct`
            // does, and always did: `Heap::alloc_hashtable` builds a
            // `BoxedObj::Struct { payload: StructPayload::Map }`. The old
            // classifier's "in neither the struct set nor the enum set" was a
            // statement about the sets it happened to be handed, not about the
            // value.
            Repr::Str
            | Repr::Int
            | Repr::Ratio
            | Repr::RandomState
            | Repr::Sexpr
            | Repr::Struct
            | Repr::Vector(_)
            | Repr::HashTable(..)
            | Repr::Enum
            | Repr::Dyn
            | Repr::Scope(_)
            | Repr::Fn => Class::Tagged { collectable: true },
            // A tagged word the collector never reclaims. The mark phase
            // (`Heap::gc`) walks `Cons`/`Str`/`Boxed` and lets every other
            // `Value` fall through, so an interned `Value::Symbol` is immortal
            // — it needs the passthrough tagging like any other tagged word,
            // and needs no root at all. This is the only representation for
            // which the two answers genuinely differ, which is why the class
            // carries the distinction instead of a second table making it.
            Repr::Sym => Class::Tagged { collectable: false },
            Repr::None => Class::NotRepresentable,
        }
    }

    /// Whether a binding of this representation needs a GC root pushed around
    /// it — `compiler.rs`'s `bind-params`/`bind-let-values`/`retain-bindings`/
    /// `release-bindings` read the number this returns.
    ///
    /// Derived from [`Repr::class`]: a value the collector can reclaim needs a
    /// root, and nothing else does.
    ///
    /// Reclassifying a `Repr` here does **not** require regenerating
    /// `src/compiler_island.bc`, despite what this comment used to claim. The
    /// island branches on the *number* (`retain-bindings`' `(eq kind 2)` push
    /// paired with `release-bindings`' matching pop); which type carries which
    /// number is decided entirely on this side, in `compile::core_bridge`.
    /// Changing what `2` *means* would need the island rebuilt; changing which
    /// reprs get a `2` does not. Verified by reclassifying `Struct` against the
    /// committed bitcode: island trio, `compile_test`, and
    /// `compiled_binding_gc_test` all stayed green.
    ///
    /// `Struct`/`Vector`/`Dyn` reaching this as `KIND_PLAIN` was not an
    /// oversight: `compile-construct-boxed-struct`'s doc comment
    /// (`src/compiler.rs`) argues the classifier "deliberately doesn't classify
    /// a `mutable` struct type as `KIND_SEXPR`", because
    /// `push-permanent-sexpr-root` already keeps every box compiled code builds
    /// alive from birth, making a per-binding push/pop redundant. That is true
    /// — and it is correctness resting on a leak, since a permanent root is
    /// never popped by design (`rt_push_permanent_sexpr_root`'s own doc). The
    /// derivation here pays that redundancy back to be independent of it.
    pub fn binding_kind(&self) -> i64 {
        /// No bookkeeping at a binding boundary.
        const KIND_PLAIN: i64 = 0;
        /// Push/pop a GC root: the value may point into the managed heap.
        const KIND_SEXPR: i64 = 2;
        match self.class() {
            Class::Tagged { collectable: true } => KIND_SEXPR,
            _ => KIND_PLAIN,
        }
    }

    /// How a value of this representation is tagged and untagged when it
    /// crosses a `BoxedObj::Struct` field boundary — `compiler.rs`'s
    /// `compile-tag-struct-field` (encode) and `compile-sexpr-field` (decode).
    ///
    /// Derived from [`Repr::class`]. The numbers are `Sexpr`'s own variant
    /// numbering (`registry::sexpr_def`: `1`=int `2`=f64 `3`=char `4`=bool
    /// `6`=str `11`=f32 `12`..`16`=i8/i16/u8/u16/u32 `17`=i32) rather than a parallel
    /// scheme, so those two island functions
    /// reuse the same per-variant bit manipulation instead of duplicating it.
    /// `Unit` is not a `Sexpr` variant and so has no number to borrow; it sits
    /// at `100`, deliberately clear of the `Sexpr` numbering so that adding a
    /// variant never collides with it. (It used to be `11`, "the first number
    /// past" — which stopped being past anything the moment `Sexpr` grew a
    /// twelfth variant.) `0` is taken by the "not representable" case.
    ///
    /// **This numbering space has a second producer.** `compile-sexpr-field`
    /// also decodes `5` (sym), `7` (cons), `9` (ratio), `10` (path) and
    /// `12`..`17` (the narrow integer widths), which come from `Sexpr`
    /// *construction* (`compile-construct-sexpr` / `match_sexpr_ctor`'s
    /// `SEXPR_*`), not from here — this function folds sym/int/ratio into the
    /// `6` passthrough. (`8`, the retired `bignum` variant, is decoded by
    /// neither: an `int`'s bignum box is variant `1`.)
    /// Anything renumbering these must account for both producers.
    ///
    /// The narrow integers are the clearest case of the two producers meaning
    /// different things by the same *kind of* value. A `u8` **field of a
    /// struct** is `1`, a raw word, because the `defstruct` already wrote the
    /// width down and nothing has to be carried alongside the value. A `u8`
    /// **inside a `Sexpr`** is variant `14`, a `BoxedObj::Narrow`, because
    /// there the width is written down nowhere else. Same type, same machine
    /// word, two encodings — decided by where the value is going, which is
    /// what this number says.
    ///
    /// Read off the `Repr`, not off [`Repr::class`]: the two float widths
    /// share a class (they compute alike) but not a kind (they box
    /// differently), and it is the boxing this number drives.
    pub fn field_kind(&self) -> i64 {
        if matches!(self, Repr::F32) {
            return 11;
        }
        match self.class() {
            Class::Int => 1,
            Class::Float => 2,
            Class::Char => 3,
            Class::Bool => 4,
            // Both directions leave the word alone.
            Class::Tagged { .. } => 6,
            // Both directions ignore the word they are handed and emit a
            // constant; the slot only has to hold something the GC can decode
            // safely.
            Class::Unit => 100,
            // A raw word has no encoding here on purpose. `1` (a plain word)
            // is what it looks like it should be and is exactly wrong: a
            // struct field is *tagged* on the way in, and shifting a pointer
            // left by one drops its top bit. That is the bug that
            // removing the 64-bit integer type was meant to make unwritable,
            // and giving this a number would write it again. `Checker` refuses
            // the declarations that would reach here, so this is the second
            // line rather than the first.
            Class::RawWord => 0,
            // The one type that genuinely reaches this is a still-generic type
            // variable ([`Repr::None`]). The island's `compile-sexpr-field`
            // panics on it with "field type is not representable in compiled
            // code yet", which is now true of everything that produces it.
            Class::NotRepresentable => 0,
        }
    }
}

/// What compiled code does with a value at a boundary — see [`Repr::class`],
/// which is the only thing that builds one.
///
/// Deliberately not `pub`: it exists to keep [`Repr::field_kind`] and
/// [`Repr::binding_kind`] from being written independently, and callers want
/// the island-facing numbers, not this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// A raw machine word.
    Int,
    Float,
    Char,
    Bool,
    /// One value, so the word carries no information.
    Unit,
    /// Already a tagged word; passes through both directions untouched.
    /// `collectable` is whether the collector can reclaim what it points at —
    /// see [`Repr::class`]'s `Sym` arm for the one case where it cannot.
    Tagged { collectable: bool },
    /// A raw 64-bit word: passed and returned untagged like [`Class::Int`],
    /// but with no way to be *stored* in a tagged slot, because tagging it
    /// would drop its top three bits. See [`Repr::RawWord`].
    RawWord,
    /// No compiled representation.
    NotRepresentable,
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

    /// Every representation the checker can produce survives being written
    /// into the core IR and read back, nested ones included.
    ///
    /// `gc_stress` is on: writing a parametric representation conses, so this
    /// also says the intermediate element forms stay rooted while their
    /// siblings are built.
    #[test]
    fn every_representation_round_trips_through_the_core_ir() {
        let mut h = Heap::with_capacity(1 << 12);
        h.set_gc_stress(true);
        let mut all: Vec<Repr> = Repr::SIMPLE.to_vec();
        all.push(Repr::Scope(Box::new(Repr::Handle)));
        all.push(Repr::Vector(Box::new(Repr::Narrow)));
        all.push(Repr::HashTable(Box::new(Repr::Str), Box::new(Repr::Sexpr)));
        // Nested, so `read` is not merely accepting a one-level list.
        all.push(Repr::Vector(Box::new(Repr::Vector(Box::new(Repr::Enum)))));
        for r in all {
            let v = r.write(&mut h).expect("write failed");
            assert_eq!(Repr::read(&h, v).as_ref(), Some(&r), "did not read back: {:?}", r);
        }
    }

    /// The spellings themselves, so a rename has to be deliberate.
    #[test]
    fn representations_print_as_the_vocabulary_spells_them() {
        let mut h = Heap::with_capacity(1 << 12);
        let printed = |h: &mut Heap, r: Repr| {
            let v = r.write(h).expect("write failed");
            crate::check::core::print(h, v)
        };
        assert_eq!(printed(&mut h, Repr::Narrow), "int-any-width");
        assert_eq!(printed(&mut h, Repr::Scope(Box::new(Repr::Handle))), "(scope handle)");
        assert_eq!(printed(&mut h, Repr::Vector(Box::new(Repr::Narrow))), "(vector int-any-width)");
        assert_eq!(
            printed(&mut h, Repr::HashTable(Box::new(Repr::Str), Box::new(Repr::Sexpr))),
            "(hashtable str sexpr)"
        );
    }

    /// Neither a misspelling nor a malformed parametric form is a
    /// representation.
    #[test]
    fn a_malformed_form_is_not_a_representation() {
        let mut h = Heap::with_capacity(1 << 12);
        let r = crate::Reader::new();
        for src in [
            "not-a-repr",
            "(vector)",
            // Arity, not spelling: both elements *are* representations, so
            // this is refused only for having two of them.
            "(vector int-any-width int-any-width)",
            "(scope not-a-repr)",
            "(42)",
            "()",
        ] {
            let v = r.read_all(&mut h, src).expect("read failed").pop().unwrap();
            assert_eq!(Repr::read(&h, v), None, "{:?} should not be a representation", src);
        }
    }

    #[test]
    fn scalars_and_adts_classify_by_their_declared_type() {
        let (structs, enums) = sets();
        let of = |t: Type| Repr::of(&t, &structs, &enums);
        assert_eq!(of(Type::I32), Repr::Narrow);
        assert_eq!(of(Type::U32), Repr::Narrow);
        assert_eq!(of(Type::F64), Repr::F64);
        assert_eq!(of(Type::F32), Repr::F32);
        assert_eq!(of(Type::Char), Repr::Char);
        assert_eq!(of(Type::Bool), Repr::Bool);
        assert_eq!(of(Type::Unit), Repr::Unit);
        assert_eq!(of(Type::Str), Repr::Str);
        assert_eq!(of(Type::Symbol), Repr::Sym);
        assert_eq!(of(Type::Ratio), Repr::Ratio);
        assert_eq!(of(Type::Named(Path::root("sexpr"), vec![])), Repr::Sexpr);
        assert_eq!(of(Type::Named(Path::root("point"), vec![])), Repr::Struct);
        assert_eq!(of(Type::Named(Path::root("my-enum"), vec![])), Repr::Enum);
        // The parametric builtins classify their elements too — the whole
        // reason they have variants of their own.
        assert_eq!(
            of(Type::Named(Path::root("vector"), vec![Type::U32])),
            Repr::Vector(Box::new(Repr::Narrow))
        );
        assert_eq!(
            of(Type::Named(Path::root("hashtable"), vec![Type::Str, Type::Bool])),
            Repr::HashTable(Box::new(Repr::Str), Box::new(Repr::Bool))
        );
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

    /// Every heap-backed representation is the passthrough kind at a field
    /// boundary *and* gets a GC root at a binding boundary. These two used to
    /// be independent hand-written tables and disagreed on four rows; this is
    /// the invariant that replaced the pinned-inconsistency test that sat here.
    ///
    /// `Sym` is the one documented exception and is asserted separately below,
    /// so adding a representation to this list is a claim that the collector
    /// can reclaim it.
    #[test]
    fn every_reclaimable_representation_is_both_passthrough_and_rooted() {
        let reclaimable = [
            Repr::Str,
            Repr::Ratio,
            Repr::Sexpr,
            Repr::Struct,
            Repr::Vector(Box::new(Repr::Narrow)),
            Repr::HashTable(Box::new(Repr::Str), Box::new(Repr::Sexpr)),
            Repr::Enum,
            Repr::Dyn,
            Repr::Scope(Box::new(Repr::Handle)),
            Repr::Fn,
        ];
        for r in &reclaimable {
            assert_eq!(r.field_kind(), 6, "{:?} should be the passthrough field kind", r);
            assert_eq!(r.binding_kind(), 2, "{:?} should be rooted at a binding boundary", r);
        }
    }

    /// A `Symbol` is the one tagged word the collector never reclaims: `Heap`'s
    /// mark phase walks `Cons`/`Str`/`Boxed` and lets every other `Value` fall
    /// through, and symbols are interned. So it takes the passthrough tagging
    /// and no root — the single place the two projections legitimately differ,
    /// which is why `Repr::class` carries `collectable` rather than a second
    /// table deciding it again.
    #[test]
    fn a_symbol_is_passthrough_but_needs_no_root() {
        assert_eq!(Repr::Sym.field_kind(), 6);
        assert_eq!(Repr::Sym.binding_kind(), 0);
    }

    /// A scalar needs no root, and `Repr::None` — a still-generic type
    /// variable, the only genuine representation gap left — is the sole
    /// producer of the field kind `0` the island panics on. A `HashTable<K,V>`
    /// used to share that number, which made a `HashTable`-typed `defstruct`
    /// field fail to compile while claiming the type had no representation
    /// (`tests/compile_test.rs`'s
    /// `compile_reads_a_hashtable_field_out_of_a_struct`).
    #[test]
    fn only_a_generic_type_variable_is_not_representable() {
        for r in [Repr::Narrow, Repr::Handle, Repr::F64, Repr::F32, Repr::Char, Repr::Bool, Repr::Unit] {
            assert_eq!(r.binding_kind(), 0, "{:?} is a scalar and needs no root", r);
            assert_ne!(r.field_kind(), 0, "{:?} is representable in a field", r);
        }
        assert_eq!(Repr::None.field_kind(), 0);
        assert_eq!(Repr::None.binding_kind(), 0);
    }
}
