//! The substrate for *core forms* — the cons-cell program representation the
//! checker lowers to and the interpreter evaluates.
//!
//! A core form is an ordinary tagged proper list, `(tag field...)`, whose `tag`
//! is an interned symbol: `(int 42)`, `(if C T E)`, `(call WRITTEN HOME PATH
//! ARG...)`. There is no Rust AST — the program is cons cells from the reader
//! all the way through evaluation, and the self-hosted compiler in
//! `crate::compiler` pattern-matches this same shape. User data can never be
//! confused for a node because it only ever appears under `(quote D)`.
//!
//! This module holds the parts that do not depend on the tag vocabulary: how a
//! node is built without losing an intermediate to the collector, how one is
//! read back, and how one is printed. The vocabulary itself lives with the
//! checker that emits it.
//!
//! # Why the builders exist
//!
//! [`Heap::cons`](typelisp_mem::Heap::cons) collects whenever the free list is
//! empty, so *every* allocation can free anything not currently rooted. Building
//! a node means allocating once per field, which means each finished field has
//! to stay rooted while its siblings are built. Done by hand that is a
//! `push_root`/`pop_root` pair per field — the translator these builders
//! replaced balanced sixteen pops by hand in one function — and a single `?` on
//! an error path skips them all.
//!
//! So callers do not cons directly. [`Items`] roots each field as it is
//! produced, [`pair`]/[`list`]/[`tagged`] assemble the node, and a
//! [`RootScope`] unwinds everything on every exit path including `?`.
//! `tests/core_builder_guard_test.rs` enforces this across the checker that
//! emits core IR, the evaluator and bridge that consume it, and the fasl that
//! restores it.

use typelisp_mem::{Error, Heap, Loc, RootScope, Value};

use crate::types::Type;

/// What checking one expression produces: the lowered core form, and the type
/// the checker proved for it.
///
/// The type is the checker's own working value — it drives the next inference
/// step, the next arity check, the next `Repr` — and it stops here. It is not
/// written into `form`, and nothing downstream of the checker can ask for it.
/// That is the whole distinction between this and the `Typed` node it replaces:
/// `Typed` carried a `Type` into evaluation and compilation, so both had to be
/// prepared to read one back.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub form: Value,
    pub ty: Type,
}

impl Checked {
    pub fn new(form: Value, ty: Type) -> Checked {
        Checked { form, ty }
    }
}

/// Cons a proper list from `items`, in order.
///
/// Both the items and every partially-built tail stay rooted for the whole
/// build, so this is safe with a collection at any point.
pub fn list(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let mut s = RootScope::new(heap);
    // Root every item for the whole build, not just across the `cons` that
    // consumes it: consing a later item can collect an earlier one. [`Items`]
    // already keeps them rooted, but a direct caller need not have.
    for item in items {
        s.push_root(*item);
    }
    let mut acc = Value::Empty;
    for item in items.iter().rev() {
        s.push_root(acc);
        acc = s.cons(*item, acc)?;
    }
    Ok(acc)
}

/// Build the pair `(car . cdr)`.
///
/// One `cons`, but the one that still needs rooting: the allocation can
/// collect, and until it returns neither half is reachable from anything the
/// collector walks. The IR is full of pairs that are not lists — a binding's
/// `(name . kind)`, an argument's `(kind . form)`, a `match` arm's
/// `(pattern . body)` — and each was being consed by hand with its own
/// `push_root` pair around it.
pub fn pair(heap: &mut Heap, car: Value, cdr: Value) -> Result<Value, Error> {
    let mut s = RootScope::new(heap);
    s.push_root(car);
    s.push_root(cdr);
    s.cons(car, cdr)
}

/// Build the node `(tag field...)`. See [`list`] for the rooting contract.
pub fn tagged(heap: &mut Heap, tag: &str, items: &[Value]) -> Result<Value, Error> {
    let mut s = RootScope::new(heap);
    let body = list(&mut s, items)?;
    s.push_root(body);
    let tag_sym = s.intern_symbol(tag);
    s.push_root(tag_sym);
    s.cons(tag_sym, body)
}

/// Build `(tag field...)` and record `loc` as the source position it came from,
/// so a runtime error raised while evaluating this node can be placed.
///
/// The same act, and the same slot, as the reader recording a list form's own
/// span: a location is a property of the cell (see `Cell` in `typelisp-mem`),
/// and "the span this form came from" means one thing whether the form was read
/// or lowered. There used to be a second table here, because the reader's was
/// bulk-cleared per read batch while a lowered node — a registered function
/// body — has to outlive its batch; nothing is cleared now, so nothing needs a
/// table of its own.
pub fn tagged_at(heap: &mut Heap, tag: &str, items: &[Value], loc: Option<Loc>) -> Result<Value, Error> {
    let node = tagged(heap, tag, items)?;
    if let (Some(loc), Value::Cons(cr)) = (loc, node) {
        heap.set_cons_loc(cr, loc);
    }
    Ok(node)
}

/// A node's fields, each rooted from the moment it is added until the whole
/// node is built.
///
/// This is the piece hand-written `push_root` pairs get wrong: a field built
/// three allocations ago is just as collectible as one built now, so it is not
/// enough to root the value being consed — every earlier sibling has to stay
/// rooted too. Holding them here makes that automatic, and the [`RootScope`]
/// inside unwinds them however the builder is left.
///
/// ```ignore
/// let mut f = Items::new(heap);
/// let c = check(f.heap(), cond)?;   f.push(c);
/// let t = check(f.heap(), then)?;   f.push(t);
/// f.finish("if")                    // roots released here
/// ```
pub struct Items<'h> {
    scope: RootScope<'h>,
    vals: Vec<Value>,
}

impl<'h> Items<'h> {
    /// Start collecting fields on `heap`.
    pub fn new(heap: &'h mut Heap) -> Items<'h> {
        Items { scope: RootScope::new(heap), vals: Vec::new() }
    }

    /// The heap, for building the next field.
    pub fn heap(&mut self) -> &mut Heap {
        &mut self.scope
    }

    /// Add a finished field, keeping it alive until the node is built.
    pub fn push(&mut self, v: Value) {
        self.scope.push_root(v);
        self.vals.push(v);
    }

    /// Add several finished fields in order.
    pub fn extend(&mut self, vs: impl IntoIterator<Item = Value>) {
        for v in vs {
            self.push(v);
        }
    }

    /// The fields collected so far.
    pub fn as_slice(&self) -> &[Value] {
        &self.vals
    }

    /// Build `(tag field...)` from the collected fields.
    pub fn finish(mut self, tag: &str) -> Result<Value, Error> {
        let vals = std::mem::take(&mut self.vals);
        tagged(&mut self.scope, tag, &vals)
    }

    /// Build `(tag field...)` and record its source position — see
    /// [`tagged_at`].
    pub fn finish_at(mut self, tag: &str, loc: Option<Loc>) -> Result<Value, Error> {
        let vals = std::mem::take(&mut self.vals);
        tagged_at(&mut self.scope, tag, &vals, loc)
    }

    /// Build the fields as a bare proper list, with no tag — for a node's
    /// sub-list (a parameter list, a `let`'s bindings, a `match`'s arms).
    pub fn finish_list(mut self) -> Result<Value, Error> {
        let vals = std::mem::take(&mut self.vals);
        list(&mut self.scope, &vals)
    }
}

// ---- reading a node back -------------------------------------------------

/// A node's tag as an interned symbol, or `None` if `form` is not a tagged
/// list. **The way to ask which node this is.**
///
/// Every tag is in `BUILTIN_SYMBOLS`, so the answer is compared against a
/// `SymId` constant — an integer test on the symbol's identity, CL's `eq`.
/// [`op`] returns the tag's *text*, which is for printing it, not for
/// recognizing it.
pub fn op_sym(heap: &Heap, form: Value) -> Option<crate::SymId> {
    match heap.car(form).ok()? {
        Value::Symbol(id) => Some(id),
        _ => None,
    }
}

/// A node's tag *spelled out*, or `None` if `form` is not a tagged list — for
/// error messages and diagnostics. To recognize a tag use [`op_sym`].
pub fn op<'h>(heap: &'h Heap, form: Value) -> Option<&'h str> {
    match heap.car(form).ok()? {
        Value::Symbol(id) => Some(heap.symbol_name(id)),
        _ => None,
    }
}

/// `(module PATH BODY...)` — a bundle of top-level forms.
///
/// Here rather than on `Checker` because the *driver* is what knows a group of
/// forms belongs together: a file's own definitions, wrapped once the whole
/// file has been checked.
pub fn tagged_module(heap: &mut Heap, path: &crate::Path, body: &[Value]) -> Result<Value, Error> {
    let mut s = RootScope::new(heap);
    for form in body {
        s.push_root(*form);
    }
    let segs = path.segments();
    let p = if segs.len() == 1 {
        s.intern_symbol(&segs[0])
    } else {
        let ids: Vec<_> = segs
            .iter()
            .map(|seg| match s.intern_symbol(seg) {
                Value::Symbol(id) => id,
                _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
            })
            .collect();
        s.intern_path(&ids)
    };
    s.push_root(p);
    let mut items = vec![p];
    items.extend_from_slice(body);
    tagged(&mut s, "module", &items)
}

/// A node's fields, in order (everything after the tag).
pub fn fields(heap: &Heap, form: Value) -> Result<Vec<Value>, Error> {
    heap.list_to_vec(heap.cdr(form)?)
}

/// A node's `i`th field (0-based, counting past the tag).
pub fn field(heap: &Heap, form: Value, i: usize) -> Option<Value> {
    let mut cur = heap.cdr(form).ok()?;
    for _ in 0..i {
        cur = heap.cdr(cur).ok()?;
    }
    heap.car(cur).ok()
}

/// A node's `i`th field read as a [`crate::Path`].
///
/// A single-segment path reads back as a bare symbol — the reader only builds a
/// `Value::Path` when it sees `::` — so both spellings have to be accepted, and
/// forgetting the symbol case fails only for definitions at the root, which is
/// most of them. Shared by everything that walks *checked* top-level forms
/// looking for what they define (`compile::aot`, `compile::prelude_bootstrap`).
pub fn path_field(heap: &Heap, form: Value, i: usize) -> Option<crate::Path> {
    match field(heap, form, i)? {
        Value::Path(id) => Some(crate::types::path_from_id(heap, id)),
        Value::Symbol(id) => Some(crate::Path::root(heap.symbol_name(id))),
        _ => None,
    }
}

// ---- printing ------------------------------------------------------------

/// Render a core form as an s-expression.
///
/// Lowered code is data, so it can simply be printed — which is what makes the
/// checker's output testable as text (`assert_eq!(print(&h, f), "(if (bool #t)
/// (int 1) (int 2))")`) instead of by matching a Rust tree. Deliberately plain:
/// no line breaks, no elision, no cycle handling — core forms are finite trees.
pub fn print(heap: &Heap, form: Value) -> String {
    let mut out = String::new();
    write_form(heap, form, &mut out);
    out
}

fn write_form(heap: &Heap, v: Value, out: &mut String) {
    use std::fmt::Write;
    match v {
        Value::Empty => out.push_str("()"),
        Value::Int(n) => {
            let _ = write!(out, "{}", n);
        }
        // The language's own spelling, not Scheme's `#t`/`#f`. That is what
        // makes a printed core form *readable back*: the reader accepts
        // `true`/`false`, so `read(print(f)) == f` holds and a test can write
        // its expected form in exactly the syntax the reader takes.
        Value::Bool(b) => out.push_str(if b { "true" } else { "false" }),
        Value::Char(c) => {
            let _ = write!(out, "#\\{}", c);
        }
        Value::Symbol(id) => out.push_str(heap.symbol_name(id)),
        Value::Str(id) => {
            let _ = write!(out, "{:?}", heap.string(id));
        }
        Value::Path(id) => {
            let segs = heap.path_segments(id);
            for (i, s) in segs.iter().enumerate() {
                if i > 0 {
                    out.push_str("::");
                }
                out.push_str(heap.symbol_name(*s));
            }
        }
        Value::Cons(_) => {
            out.push('(');
            let mut cur = v;
            let mut first = true;
            loop {
                match cur {
                    Value::Cons(_) => {
                        if !first {
                            out.push(' ');
                        }
                        first = false;
                        let (car, cdr) = match (heap.car(cur), heap.cdr(cur)) {
                            (Ok(a), Ok(d)) => (a, d),
                            _ => break,
                        };
                        write_form(heap, car, out);
                        cur = cdr;
                    }
                    Value::Empty => break,
                    other => {
                        // Improper tail: core nodes are proper lists, so this
                        // only shows up in a `(quote D)` datum or a bug.
                        out.push_str(" . ");
                        write_form(heap, other, out);
                        break;
                    }
                }
            }
            out.push(')');
        }
        // A core form's own structure never contains a box, but its *data*
        // does: a `(float F)`/`(bignum B)`/`(ratio R)` literal carries one as
        // its field, and so does any `(quote D)` over the same. Rendering
        // those as an opaque `#<boxed>` would make an assertion over a literal
        // vacuous — every float would compare equal to every other — so each
        // numeric box prints its value. Anything else is not supposed to be
        // here at all, and says what it is so a failing assertion names the
        // surprise instead of hiding it.
        Value::Boxed(id) if heap.is_float(id) => {
            let f = heap.float_value(id);
            // `{:?}` so an integral float keeps its point (`1.0`, not `1`) and
            // stays distinguishable from `(int 1)` in a comparison.
            let _ = write!(out, "{:?}", f);
        }
        Value::Boxed(id) if heap.is_bignum(id) => {
            let _ = write!(out, "{}", heap.bignum_value(id));
        }
        Value::Boxed(id) if heap.is_ratio(id) => {
            let r = heap.ratio_value(id);
            let _ = write!(out, "{}/{}", r.numer(), r.denom());
        }
        Value::Boxed(id) => match crate::type_key::heap_type_path(heap, id) {
            Some(p) => {
                let _ = write!(out, "#<{}>", p);
            }
            None => out.push_str("#<boxed>"),
        },
    }
}
