//! [`OwnedForm`]: an owned, GC-heap-independent mirror of a read form.
//!
//! A checked `Value` is an index into *a* heap, so any checker state that must
//! outlive the heap it was read in — or be re-materialized in a different one —
//! cannot hold a `Value`. This module is that carrier: `value_to_owned` detaches
//! a form from its heap, `owned_to_value` rebuilds it in another **through that
//! heap's own allocation APIs** (`intern_symbol`/`alloc_string`/`heap.cons`/...),
//! never by copying raw memory, so the result is indistinguishable from what the
//! reader would have built.
//!
//! Its users are all in the checker: an `&optional`/`&key` default expression
//! (`OptKeyParam::default`), a `deftrait` default method body checked before any
//! `impl` exists, and generic templates retained for per-instantiation
//! re-checking. It was originally written as the serialization carrier for the
//! `fasl` compiled-module format; that mechanism was removed (see
//! `docs/dev/implementation-log.md`), and this outlived it because the checker
//! needed heap-independent syntax for its own reasons.

use num_bigint::BigInt;
use num_rational::BigRational;

use typelisp_mem::specialization::{specialization, specialized, Generic};
use typelisp_mem::symbols::{intern_at_home_path, ns_of, ns_path};
use typelisp_mem::{SymRef, TypeId, TypeShape};

use crate::{type_key, Error, Heap, Loc, Value};

/// An owned, GC-heap-independent mirror of a *read form* (`Value`) — the
/// serializable carrier for generic-template `parts`. The same idea as
/// the deleted `QuotedSexpr`, plus the [`OwnedForm::Path`] variant that one
/// deliberately lacked (quote strips `::`-paths during checking; raw template
/// forms still contain them).
///
/// A `Cons` carries the two source spans its cell holds, because a location is
/// a property of the datum and this is that datum written down: dropping them
/// here would mean a form that came back from a fasl could not say where it was
/// written, and every runtime error in it would lose its `file:line:col`
/// (`tests/fasl_test.rs`'s `a_fasl_loaded_function_reports_its_own_source_position`
/// is the guard). The rebuild re-records them, which is possible precisely
/// because the location travels *with* the cell rather than in a table keyed by
/// an address that no longer exists.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OwnedForm {
    /// `Value::Empty` — the empty list `()`.
    Empty,
    Int(i64),
    Char(char),
    Bool(bool),
    /// A symbol other than a keyword.
    Sym(OwnedSym),
    /// A keyword, by name — without the colon that spells it (re-interned in
    /// the keyword module on load). Its own variant because being a keyword
    /// is where a symbol belongs, which a name does not say.
    Keyword(String),
    /// A string, by content (re-allocated on load).
    Str(String),
    /// A `::`-path, by its segments' symbols: the path stays the symbols it
    /// was read as.
    Path(Vec<OwnedSym>),
    /// One variant per float width. Not one `Float(f64)` with the width left
    /// to the reader: a dump is a round trip, and a round trip that widens an
    /// `f32` gives back a value whose box no longer matches its type.
    F32(f32),
    F64(f64),
    #[serde(with = "num_str")]
    Bignum(BigInt),
    #[serde(with = "num_str")]
    Ratio(BigRational),
    /// A cons chain, spine flattened: one entry per cell, then whatever the
    /// last cell's `cdr` holds (`Empty` for a proper list, an atom for a dotted
    /// one like `(a . b)`).
    ///
    /// Flat rather than the right-nested pair the cells actually are, because
    /// the nesting is what a *list* costs in a depth-limited format: this
    /// serializes as JSON, whose parser refuses past 128 levels of nesting, and
    /// a right-nested pair makes a list of N elements N levels deep. A checked
    /// body is a list now, and the prelude's own bodies exceeded the limit — no
    /// fasl could be read back at all (`fasl parse: recursion limit exceeded`).
    /// Depth here is structural nesting only. Raising the parser's limit instead
    /// would have traded a clean error for a stack overflow, since the limit is
    /// what stands between deep input and one; flattening removes the growth.
    ///
    /// Both walks iterate the spine for the same reason one level down: a
    /// pair-at-a-time recursion spends one Rust stack frame per list element.
    Cons { cells: Vec<OwnedCell>, tail: Box<OwnedForm> },
    /// `#(..)`: the `Sexpr` vector variant's box, by its elements.
    Vector(Vec<OwnedForm>),
    /// `#{..}`: a tuple box, by its elements (the arity names the box's type).
    Tuple(Vec<OwnedForm>),
    /// `#NA(..)`: the `Sexpr` array variant's box, by its dimensions and its
    /// elements in row-major order.
    Array { dims: Vec<i64>, data: Vec<OwnedForm> },
}

/// A symbol written down, so that reading it back gives the same symbol.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OwnedSym {
    /// By its name and the segments of its home module from the root
    /// (re-interned in that module on load, never resolved again: the home
    /// is where the symbol already was).
    Named(String, Vec<String>),
    /// A specialization, by what it is of and at which types: nothing reads
    /// as it, so its name would read back as some other symbol.
    Specialization(Box<OwnedGeneric>, Vec<OwnedType>),
}

/// A [`typelisp_mem::specialization::Generic`] written down.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OwnedGeneric {
    Fn(Vec<String>, OwnedSym),
    Method(Vec<String>, OwnedSym, OwnedSym),
}

/// A type's [`TypeShape`] written down, its modules as their segments from
/// the root and its symbols as [`OwnedSym`]s.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OwnedType {
    Named(Vec<String>, OwnedSym, Vec<OwnedType>),
    Dyn(Vec<String>, OwnedSym, Vec<OwnedType>),
    Fn(Vec<OwnedType>, Option<Box<OwnedType>>, Box<OwnedType>),
    Ptr(Box<OwnedType>),
    CStruct(Vec<String>, OwnedSym),
    Unit,
    Never,
    Param(u32),
}

impl OwnedSym {
    pub fn of(sym: SymRef) -> OwnedSym {
        match specialized(sym) {
            Some((generic, args)) => {
                let generic = match generic {
                    Generic::Fn(ns, name) => OwnedGeneric::Fn(ns_path(ns), OwnedSym::of(name)),
                    Generic::Method(ns, ty, method) => OwnedGeneric::Method(ns_path(ns), OwnedSym::of(ty), OwnedSym::of(method)),
                };
                OwnedSym::Specialization(Box::new(generic), args.into_iter().map(OwnedType::of).collect())
            }
            None => OwnedSym::Named(sym.name().to_string(), ns_path(sym.home())),
        }
    }

    pub fn symbol(&self) -> SymRef {
        match self {
            OwnedSym::Named(name, home) => intern_at_home_path(home, name),
            OwnedSym::Specialization(generic, args) => {
                let generic = match &**generic {
                    OwnedGeneric::Fn(ns, name) => Generic::Fn(ns_of(ns), name.symbol()),
                    OwnedGeneric::Method(ns, ty, method) => Generic::Method(ns_of(ns), ty.symbol(), method.symbol()),
                };
                specialization(generic, &args.iter().map(OwnedType::type_id).collect::<Vec<_>>())
            }
        }
    }
}

impl OwnedType {
    pub fn of(ty: TypeId) -> OwnedType {
        let all = |ids: Vec<TypeId>| ids.into_iter().map(OwnedType::of).collect();
        match typelisp_mem::type_shape(ty) {
            TypeShape::Named(ns, sym, args) => OwnedType::Named(ns_path(ns), OwnedSym::of(sym), all(args)),
            TypeShape::Dyn(ns, sym, pins) => OwnedType::Dyn(ns_path(ns), OwnedSym::of(sym), all(pins)),
            TypeShape::Fn(params, rest, ret) => {
                OwnedType::Fn(all(params), rest.map(|r| Box::new(OwnedType::of(r))), Box::new(OwnedType::of(ret)))
            }
            TypeShape::Ptr(pointee) => OwnedType::Ptr(Box::new(OwnedType::of(pointee))),
            TypeShape::CStruct(ns, sym) => OwnedType::CStruct(ns_path(ns), OwnedSym::of(sym)),
            TypeShape::Unit => OwnedType::Unit,
            TypeShape::Never => OwnedType::Never,
            TypeShape::Param(n) => OwnedType::Param(n),
        }
    }

    pub fn type_id(&self) -> TypeId {
        let all = |tys: &[OwnedType]| tys.iter().map(OwnedType::type_id).collect();
        // type-identity-ok: the shape the dump wrote from a TypeId, no path spelled
        typelisp_mem::intern_type(match self {
            OwnedType::Named(ns, sym, args) => TypeShape::Named(ns_of(ns), sym.symbol(), all(args)),
            OwnedType::Dyn(ns, sym, pins) => TypeShape::Dyn(ns_of(ns), sym.symbol(), all(pins)),
            OwnedType::Fn(params, rest, ret) => TypeShape::Fn(all(params), rest.as_ref().map(|r| r.type_id()), ret.type_id()),
            OwnedType::Ptr(pointee) => TypeShape::Ptr(pointee.type_id()),
            OwnedType::CStruct(ns, sym) => TypeShape::CStruct(ns_of(ns), sym.symbol()),
            OwnedType::Unit => TypeShape::Unit,
            OwnedType::Never => TypeShape::Never,
            OwnedType::Param(n) => TypeShape::Param(*n),
        })
    }
}

/// A [`SymRef`] field written as its [`OwnedSym`]. For
/// `#[serde(with = "sym_serde")]`.
pub mod sym_serde {
    use super::{OwnedSym, SymRef};

    pub fn serialize<S: serde::Serializer>(sym: &SymRef, s: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&OwnedSym::of(*sym), s)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<SymRef, D::Error> {
        let sym: OwnedSym = serde::Deserialize::deserialize(d)?;
        Ok(sym.symbol())
    }
}

/// One cell of a flattened cons spine ([`OwnedForm::Cons`]): the element in the
/// cell's `car`, plus the two spans the cell carries — its `car`'s own extent
/// and the extent of the form this cell heads. `None` for a cell with none (a
/// synthesized or macro-expanded list).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OwnedCell {
    pub form: OwnedForm,
    #[serde(with = "loc_serde::opt")]
    pub car_loc: Option<Loc>,
    #[serde(with = "loc_serde::opt")]
    pub self_loc: Option<Loc>,
}

/// Serialization for [`Loc`], which lives in `typelisp-mem` and must keep
/// living there without a `serde` dependency: every AOT executable links that
/// crate, and only the ones that call `eval` should carry a serializer.
///
/// `serde(remote)` is exactly this case — the shape is written down here, once,
/// against a type this crate does not own. `Arc<str>` round-trips through
/// `serde`'s `rc` feature; the sharing is not preserved, which costs nothing:
/// a restored form's cells all name the same file, and re-interning one string
/// per cell is what the reader would have done anyway.
/// `bignum`/`ratio` as their decimal spellings.
///
/// Rather than turning on `num-bigint`/`num-rational`'s own `serde` features:
/// a Cargo feature is additive across the whole graph, so enabling it here
/// would give every crate that touches those types a serializer, including the
/// ones every AOT executable links. The spelling is canonical either way — a
/// `BigRational` is stored already reduced — so a round-trip through text is
/// exact, which is the same reasoning `compile_test`'s `Readback` uses to
/// compare numbers between two runs.
mod num_str {
    use std::fmt::Display;
    use std::str::FromStr;

    pub(crate) fn serialize<T: Display, S: serde::Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }

    pub(crate) fn deserialize<'de, T, D>(d: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: serde::Deserializer<'de>,
    {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

mod loc_serde {
    use std::sync::Arc;

    use super::Loc;

    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(remote = "Loc")]
    struct LocDef {
        file: Arc<str>,
        line: u32,
        col: u32,
        end_line: u32,
        end_col: u32,
    }

    pub(crate) mod opt {
        use super::{Loc, LocDef};

        #[derive(serde::Serialize, serde::Deserialize)]
        struct Wrap(#[serde(with = "LocDef")] Loc);

        pub(crate) fn serialize<S: serde::Serializer>(v: &Option<Loc>, s: S) -> Result<S::Ok, S::Error> {
            // `Wrap` is a newtype, so this costs one clone of an `Rc` and five
            // `u32`s — not the string.
            let w = v.clone().map(Wrap);
            serde::Serialize::serialize(&w, s)
        }

        pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Loc>, D::Error> {
            let w: Option<Wrap> = serde::Deserialize::deserialize(d)?;
            Ok(w.map(|Wrap(l)| l))
        }
    }
}

/// Converts a heap `Value` into its owned mirror. Read-only on `heap`.
///
/// Handles exactly the shapes the reader produces (which is all a generic
/// template's `parts` can contain — they are raw, unchecked read forms):
/// atoms, interned symbols/paths, strings, cons cells, the boxed numeric
/// literals, and the literal data `#(..)`/`#{..}`/`#NA`. Any other boxed
/// object (a struct, closure, hashtable — never reader output) is an internal
/// error, reported rather than silently mis-serialized.
pub fn value_to_owned(heap: &Heap, v: Value) -> Result<OwnedForm, Error> {
    Ok(match v {
        Value::Empty => OwnedForm::Empty,
        Value::Int(n) => OwnedForm::Int(n),
        Value::Char(c) => OwnedForm::Char(c),
        Value::Bool(b) => OwnedForm::Bool(b),
        Value::Symbol(id) if id.is_keyword() => OwnedForm::Keyword(id.name().to_string()),
        Value::Symbol(id) => OwnedForm::Sym(OwnedSym::of(id)),
        Value::Str(id) => OwnedForm::Str(heap.string(id).to_string()),
        Value::Path(id) => OwnedForm::Path(heap.path_segments(id).iter().map(|s| OwnedSym::of(*s)).collect()),
        Value::Cons(_) => {
            // The spine iteratively; only the elements recurse.
            let mut cells = Vec::new();
            let mut cur = v;
            let tail = loop {
                match cur {
                    Value::Cons(cr) => {
                        cells.push(OwnedCell {
                            form: value_to_owned(heap, heap.car(cur)?)?,
                            car_loc: heap.elem_loc(cr),
                            self_loc: heap.cons_loc(cur),
                        });
                        cur = heap.cdr(cur)?;
                    }
                    other => break value_to_owned(heap, other)?,
                }
            };
            OwnedForm::Cons { cells, tail: Box::new(tail) }
        }
        Value::Boxed(id) if heap.is_f32(id) => OwnedForm::F32(heap.f32_value(id)),
        Value::Boxed(id) if heap.is_f64(id) => OwnedForm::F64(heap.f64_value(id)),
        Value::Boxed(id) if heap.is_bignum(id) => OwnedForm::Bignum(heap.bignum_value(id).clone()),
        Value::Boxed(id) if heap.is_ratio(id) => OwnedForm::Ratio(heap.ratio_value(id).clone()),
        Value::Boxed(id) if type_key::heap_type_is_id(heap, id, TypeId::SEXPR_VECTOR) => {
            OwnedForm::Vector(struct_fields_to_owned(heap, id)?)
        }
        Value::Boxed(id) if heap.sexpr_tuple_arity(id).is_some() => OwnedForm::Tuple(struct_fields_to_owned(heap, id)?),
        Value::Boxed(id) if type_key::heap_type_is_id(heap, id, TypeId::SEXPR_ARRAY) => {
            // The fields of `typelisp_read::alloc_sexpr_array`'s box: `dims`
            // (a box of `Value::Int`s), `data`, `fill-pointer` (always `none`).
            let dims = match heap.struct_field(id, 0) {
                Value::Boxed(d) => (0..heap.struct_field_count(d))
                    .map(|i| match heap.struct_field(d, i) {
                        Value::Int(n) => Ok(n),
                        other => Err(Error::TypeError(format!("fasl: an array dimension is {:?}, not an int", other))),
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
                other => return Err(Error::TypeError(format!("fasl: an array's dimensions are {:?}", other))),
            };
            let data = match heap.struct_field(id, 1) {
                Value::Boxed(d) => struct_fields_to_owned(heap, d)?,
                other => return Err(Error::TypeError(format!("fasl: an array's data is {:?}", other))),
            };
            OwnedForm::Array { dims, data }
        }
        Value::Boxed(_) => {
            return Err(Error::TypeError(
                "fasl: a non-reader boxed value (struct/closure/hashtable) cannot be serialized".into(),
            ))
        }
    })
}

/// Rebuilds an [`OwnedForm`] as a live `Value` in `heap` — the load-time
/// half, allocating **through the heap's own APIs** (`intern_symbol`/
/// `alloc_string`/`intern_path`/`alloc_f32`/`alloc_f64`/`int_from_bigint`/`alloc_ratio`/
/// `cons`), never copying raw memory.
///
/// The returned value is *unrooted* — like a reader result, the caller must
/// root it (or store it somewhere the GC traces) before the next allocation.
/// Within a single call this function protects its own intermediates: while
/// a `Cons`'s cdr is being built, the already-built car is pushed as a GC
/// root (the `cdr`'s own allocations may trigger a collection), popped
/// LIFO before consing — the same discipline the reader itself follows.
pub fn owned_to_value(heap: &mut Heap, f: &OwnedForm) -> Result<Value, Error> {
    Ok(match f {
        OwnedForm::Empty => Value::Empty,
        OwnedForm::Int(n) => Value::Int(*n),
        OwnedForm::Char(c) => Value::Char(*c),
        OwnedForm::Bool(b) => Value::Bool(*b),
        OwnedForm::Sym(sym) => Value::Symbol(sym.symbol()),
        OwnedForm::Keyword(name) => heap.intern_keyword(name),
        OwnedForm::Str(s) => heap.alloc_string(s.clone()),
        OwnedForm::Path(segs) => {
            // Symbols are permanent (never collected), so the intermediate
            // `SymRef`s need no rooting while later segments intern.
            let ids: Vec<_> = segs.iter().map(OwnedSym::symbol).collect();
            heap.intern_path(&ids)
        }
        OwnedForm::F32(x) => heap.alloc_f32(*x),
        OwnedForm::F64(x) => heap.alloc_f64(*x),
        // A dump records an `int`'s bignum box only when the value did not
        // fit a fixnum; one that does is a corrupt or foreign dump, refused
        // rather than quietly re-canonicalized.
        OwnedForm::Bignum(n) => match i64::try_from(n) {
            Ok(v) if typelisp_mem::fixnum_fits(v) => {
                return Err(Error::TypeError(format!(
                    "fasl: a bignum box holding {} (a fixnum-range value) is not a canonical `int`",
                    n
                )))
            }
            _ => heap.int_from_bigint(n.clone()),
        },
        OwnedForm::Ratio(r) => heap.alloc_ratio(r.clone()),
        OwnedForm::Vector(elems) => {
            let roots_base = heap.root_count();
            let built = owned_elems_rooted(heap, elems).map(|vals| type_key::alloc_sexpr_vector(heap, vals));
            heap.truncate_roots(roots_base);
            built?
        }
        OwnedForm::Tuple(elems) => {
            let roots_base = heap.root_count();
            let built = owned_elems_rooted(heap, elems).map(|vals| type_key::alloc_sexpr_tuple(heap, vals));
            heap.truncate_roots(roots_base);
            built?
        }
        OwnedForm::Array { dims, data } => {
            let roots_base = heap.root_count();
            let built = owned_elems_rooted(heap, data).map(|vals| {
                typelisp_read::alloc_sexpr_array(heap, dims.iter().map(|&d| Value::Int(d)).collect(), vals)
            });
            heap.truncate_roots(roots_base);
            built?
        }
        OwnedForm::Cons { cells, tail } => {
            // Back to front, so each `cons` already has its cdr in hand — which
            // is also what keeps the rooting to one intermediate: the growing
            // tail is the only thing that has to survive the next allocation.
            // Rooted for the whole arm and released on the way out, so an error
            // partway through leaves no residue.
            let roots_base = heap.root_count();
            let built = (|| -> Result<Value, Error> {
                let mut acc = owned_to_value(heap, tail)?;
                heap.push_root(acc);
                for cell in cells.iter().rev() {
                    let car_v = owned_to_value(heap, &cell.form)?;
                    heap.push_root(car_v);
                    // core-build-ok: rebuilding a flattened spine read back
                    // from disk, where the elements arrive one at a time. Both
                    // halves are rooted above, and the explicit `roots_base`
                    // unwind below covers the error paths a `RootScope` would.
                    let consed = heap.cons(car_v, acc)?;
                    heap.push_root(consed);
                    // The spans go back into the rebuilt cell, not into a table
                    // keyed by the old one's address — which is what makes them
                    // survive at all, since that address belongs to a heap that
                    // is gone.
                    if let Value::Cons(cr) = consed {
                        if let Some(loc) = &cell.car_loc {
                            heap.set_elem_loc(cr, loc.clone());
                        }
                        if let Some(loc) = &cell.self_loc {
                            heap.set_cons_loc(cr, loc.clone());
                        }
                    }
                    acc = consed;
                }
                Ok(acc)
            })();
            heap.truncate_roots(roots_base);
            built?
        }
    })
}

/// A literal-data box's fields as owned forms, in order.
fn struct_fields_to_owned(heap: &Heap, id: typelisp_mem::BoxId) -> Result<Vec<OwnedForm>, Error> {
    (0..heap.struct_field_count(id)).map(|i| value_to_owned(heap, heap.struct_field(id, i))).collect()
}

/// Rebuilds each element, rooting it as it is built so the next one's
/// allocations cannot collect it. The roots are the caller's to release.
fn owned_elems_rooted(heap: &mut Heap, elems: &[OwnedForm]) -> Result<Vec<Value>, Error> {
    let mut vals = Vec::with_capacity(elems.len());
    for e in elems {
        let v = owned_to_value(heap, e)?;
        heap.push_root(v);
        vals.push(v);
    }
    Ok(vals)
}
