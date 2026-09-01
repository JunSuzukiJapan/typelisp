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

use crate::{Error, Heap, Loc, Value};

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
    /// An interned symbol, by name (re-interned on load).
    Sym(String),
    /// A string, by content (re-allocated on load).
    Str(String),
    /// A `::`-path, by segment names (re-interned on load).
    Path(Vec<String>),
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
/// against a type this crate does not own. `Rc<str>` round-trips through
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

    pub fn serialize<T: Display, S: serde::Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<T, D::Error>
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
    use std::rc::Rc;

    use super::Loc;

    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(remote = "Loc")]
    struct LocDef {
        file: Rc<str>,
        line: u32,
        col: u32,
        end_line: u32,
        end_col: u32,
    }

    pub mod opt {
        use super::{Loc, LocDef};

        #[derive(serde::Serialize, serde::Deserialize)]
        struct Wrap(#[serde(with = "LocDef")] Loc);

        pub fn serialize<S: serde::Serializer>(v: &Option<Loc>, s: S) -> Result<S::Ok, S::Error> {
            // `Wrap` is a newtype, so this costs one clone of an `Rc` and five
            // `u32`s — not the string.
            let w = v.clone().map(Wrap);
            serde::Serialize::serialize(&w, s)
        }

        pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Loc>, D::Error> {
            let w: Option<Wrap> = serde::Deserialize::deserialize(d)?;
            Ok(w.map(|Wrap(l)| l))
        }
    }
}

/// Converts a heap `Value` into its owned mirror. Read-only on `heap`.
///
/// Handles exactly the shapes the reader produces (which is all a generic
/// template's `parts` can contain — they are raw, unchecked read forms):
/// atoms, interned symbols/paths, strings, cons cells, and the three boxed
/// numeric literals. Any other boxed object (a struct, closure, hashtable —
/// never reader output) is an internal error, reported rather than silently
/// mis-serialized.
pub fn value_to_owned(heap: &Heap, v: Value) -> Result<OwnedForm, Error> {
    Ok(match v {
        Value::Empty => OwnedForm::Empty,
        Value::Int(n) => OwnedForm::Int(n),
        Value::Char(c) => OwnedForm::Char(c),
        Value::Bool(b) => OwnedForm::Bool(b),
        Value::Symbol(id) => OwnedForm::Sym(heap.symbol_name(id).to_string()),
        Value::Str(id) => OwnedForm::Str(heap.string(id).to_string()),
        Value::Path(id) => OwnedForm::Path(
            heap.path_segments(id).iter().map(|s| heap.symbol_name(*s).to_string()).collect(),
        ),
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
        Value::Boxed(_) => {
            return Err(Error::TypeError(
                "fasl: a non-reader boxed value (struct/closure/hashtable) cannot be serialized".into(),
            ))
        }
    })
}

/// Rebuilds an [`OwnedForm`] as a live `Value` in `heap` — the load-time
/// half, allocating **through the heap's own APIs** (`intern_symbol`/
/// `alloc_string`/`intern_path`/`alloc_float`/`alloc_bignum`/`alloc_ratio`/
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
        OwnedForm::Sym(name) => heap.intern_symbol(name),
        OwnedForm::Str(s) => heap.alloc_string(s.clone()),
        OwnedForm::Path(segs) => {
            // Symbols are permanent (never collected), so the intermediate
            // `SymRef`s need no rooting while later segments intern.
            let ids: Vec<_> = segs
                .iter()
                .map(|s| match heap.intern_symbol(s) {
                    Value::Symbol(id) => id,
                    other => unreachable!("intern_symbol returned {:?}", other),
                })
                .collect();
            heap.intern_path(&ids)
        }
        OwnedForm::F32(x) => heap.alloc_f32(*x),
        OwnedForm::F64(x) => heap.alloc_f64(*x),
        OwnedForm::Bignum(n) => heap.alloc_bignum(n.clone()),
        OwnedForm::Ratio(r) => heap.alloc_ratio(r.clone()),
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
