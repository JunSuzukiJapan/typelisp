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
#[derive(Clone, Debug, PartialEq)]
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
    Float(f64),
    Bignum(BigInt),
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
#[derive(Clone, Debug, PartialEq)]
pub struct OwnedCell {
    pub form: OwnedForm,
    pub car_loc: Option<Loc>,
    pub self_loc: Option<Loc>,
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
        Value::Boxed(id) if heap.is_float(id) => OwnedForm::Float(heap.float_value(id)),
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
            // `SymId`s need no rooting while later segments intern.
            let ids: Vec<_> = segs
                .iter()
                .map(|s| match heap.intern_symbol(s) {
                    Value::Symbol(id) => id,
                    other => unreachable!("intern_symbol returned {:?}", other),
                })
                .collect();
            heap.intern_path(&ids)
        }
        OwnedForm::Float(x) => heap.alloc_float(*x),
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
