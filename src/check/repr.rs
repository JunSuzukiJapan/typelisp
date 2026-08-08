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

/// Whether `ty` is a `Scope<V>`, for any `V`.
///
/// Every scope is one heap object (`StructPayload::Frames`) regardless of its
/// element type, so this needs no recursion — unlike the handle test above,
/// which a `Scope<llvm-value>` used to satisfy back when such a scope was a
/// Rust-native object behind a registry handle.
pub fn is_scope_ty(ty: &Type) -> bool {
    matches!(ty, Type::Named(p, args) if path_is_builtin(p, "scope") && args.len() == 1)
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

/// [`is_enum_ty_by`] over the pre-resolved set form — the shape the compile
/// pipeline hands around, where a path is an enum exactly when it is in the
/// set.
pub fn is_enum_ty(ty: &Type, enums: &HashSet<Path>) -> bool {
    is_enum_ty_by(ty, &|p| if enums.contains(p) { Some(AdtKind::Sum) } else { None })
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
            _ if ty.is_integer() => Repr::Int,
            _ if is_llvm_handle_ty(ty) => Repr::Handle,
            _ if ty.is_float() => Repr::Float,
            Type::Char => Repr::Char,
            Type::Bool => Repr::Bool,
            Type::Unit => Repr::Unit,
            Type::Str => Repr::Str,
            Type::Symbol => Repr::Sym,
            Type::Bignum => Repr::Bignum,
            Type::Ratio => Repr::Ratio,
            Type::Named(p, _) if *p == Path::root("sexpr") => Repr::Sexpr,
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
            Repr::Scope(_) => "scope",
            Repr::Vector(_) => "vector",
            Repr::HashTable(..) => "hashtable",
            Repr::Fn => "fn",
            Repr::None => "none",
        }
    }

    /// Every simple representation, for [`Repr::read`] and for a test that
    /// wants to enumerate the vocabulary.
    pub const SIMPLE: [Repr; 16] = [
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
    pub fn binding_kind(&self) -> i64 {
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
            | Repr::Scope(_)
            | Repr::Enum => KIND_SEXPR,
            Repr::Int
            | Repr::Float
            | Repr::Char
            | Repr::Bool
            | Repr::Unit
            | Repr::Handle
            | Repr::Sym
            | Repr::Struct
            // A `Vector<T>` is in the struct set and a `HashTable<K,V>` is in
            // neither, so both already produced this number.
            | Repr::Vector(_)
            | Repr::HashTable(..)
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
    pub fn field_kind(&self) -> i64 {
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
            // Represented exactly like a `defstruct`, which is what it was
            // classified as before it had a variant of its own.
            | Repr::Vector(_)
            | Repr::Enum
            | Repr::Dyn
            | Repr::Scope(_)
            | Repr::Fn => 6,
            // Both directions ignore the word they are handed and emit a
            // constant: a unit type has exactly one value, already known from
            // the declared type, so the slot carries no information and only
            // has to hold something the GC can decode safely.
            Repr::Unit => 11,
            // A `HashTable<K,V>` is in neither the struct set nor the enum
            // set, so this is the number it already produced. That it shares
            // the number with the genuine representation gap is an
            // inconsistency inherited from the classifier this replaced, not a
            // new one — and, like the `binding_kind` asymmetry above, it is
            // frozen until the island is rebuilt.
            Repr::HashTable(..) | Repr::None => 0,
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
        all.push(Repr::Vector(Box::new(Repr::Int)));
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
        assert_eq!(printed(&mut h, Repr::Int), "int");
        assert_eq!(printed(&mut h, Repr::Scope(Box::new(Repr::Handle))), "(scope handle)");
        assert_eq!(printed(&mut h, Repr::Vector(Box::new(Repr::Int))), "(vector int)");
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
        for src in ["not-a-repr", "(vector)", "(vector int int)", "(scope not-a-repr)", "(42)", "()"] {
            let v = r.read_all(&mut h, src).expect("read failed").pop().unwrap();
            assert_eq!(Repr::read(&h, v), None, "{:?} should not be a representation", src);
        }
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
        // The parametric builtins classify their elements too — the whole
        // reason they have variants of their own.
        assert_eq!(
            of(Type::Named(Path::root("vector"), vec![Type::I64])),
            Repr::Vector(Box::new(Repr::Int))
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
            Repr::Vector(Box::new(Repr::Int)),
            Repr::Enum,
            Repr::Dyn,
            Repr::Scope(Box::new(Repr::Handle)),
            Repr::Fn,
        ];
        // Every one of them is the passthrough kind at a field boundary...
        for r in &heap_ish {
            assert_eq!(r.field_kind(), 6, "{:?} should be the passthrough field kind", r);
        }
        // ...but only some of them get a GC root at a binding boundary.
        let rooted: Vec<Repr> = heap_ish.iter().filter(|r| r.binding_kind() == 2).cloned().collect();
        assert_eq!(
            rooted,
            vec![
                Repr::Str,
                Repr::Bignum,
                Repr::Ratio,
                Repr::Sexpr,
                Repr::Enum,
                Repr::Scope(Box::new(Repr::Handle)),
                Repr::Fn
            ],
            "the unrooted heap representations should be exactly Struct/Vector/Sym/Dyn"
        );
    }

    /// A `HashTable<K,V>` is the one representation that is heap-allocated and
    /// yet gets neither a root nor a passthrough field kind. Pinned so that
    /// the inconsistency `field_kind` documents cannot be quietly changed on
    /// one side while the island's own numbering is frozen.
    #[test]
    fn a_hash_table_keeps_the_numbers_it_had_before_it_had_a_variant() {
        let ht = Repr::HashTable(Box::new(Repr::Str), Box::new(Repr::Sexpr));
        assert_eq!(ht.binding_kind(), 0);
        assert_eq!(ht.field_kind(), 0);
    }
}
