//! The substrate for *core forms* — the cons-cell program representation the
//! checker lowers to and the interpreter evaluates.
//!
//! A core form is an ordinary tagged proper list, `(tag field...)`, whose `tag`
//! is an interned symbol: `(int 42)`, `(if C T E)`, `(call PATH (R...)
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

use typelisp_mem::{wk, Error, Heap, Loc, RootScope, SymRef, Value};

use crate::types::Type;

/// Why a value is not admitted as a trait object.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NotDyn {
    /// Its type is not a nominal type.
    NotNominal,
    /// Its type does not implement the trait.
    Unimplemented,
    /// Its type has no heap representation for the box to hold.
    NoHeapRepr,
    /// Its type is a niched `Option`, with no box to carry a type key.
    NichedOption,
    /// `owner`'s `impl` binds the associated type `name` to `actual`, and
    /// the `:dyn` pins it to `pin`.
    Assoc { owner: crate::Path, name: String, actual: Type, pin: Type },
    /// It is a `:dyn from` already, and `from` does not inherit the trait.
    NotInherited { from: crate::Path },
    /// It is a `:dyn from` already, whose pins make the trait's `name`
    /// `actual`, and the `:dyn` pins it to `want`.
    PinDiffers { from: crate::Path, name: String, actual: Type, want: Type },
    /// It is a `:dyn from` already, whose pins leave the trait's `name` open.
    Unpinned { from: crate::Path, name: String },
}

/// How a type decided for a bounded type parameter falls short of the bound.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Unmet {
    /// It is the enclosing function's own type parameter `var`, which its
    /// `where` clause does not bound by `trait_path`.
    Undeclared { var: String, trait_path: crate::Path },
    /// `ty` does not implement `trait_path`. `via` is a method of `ty`
    /// taking only the receiver whose result does, when it has one.
    Unimplemented { ty: Type, trait_path: crate::Path, via: Option<SymRef> },
    /// `ty`'s associated type `assoc` is `actual` (`None`: it has none), not
    /// the `required` one.
    Assoc { ty: Type, assoc: String, actual: Option<Type>, required: Type },
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
pub fn tagged(heap: &mut Heap, tag: u32, items: &[Value]) -> Result<Value, Error> {
    let mut s = RootScope::new(heap);
    let body = list(&mut s, items)?;
    s.push_root(body);
    let tag_sym = Value::Symbol(typelisp_mem::symbols::well_known_symbol(tag));
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
pub fn tagged_at(heap: &mut Heap, tag: u32, items: &[Value], loc: Option<Loc>) -> Result<Value, Error> {
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
/// f.finish(wk::IF)                    // roots released here
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
    pub fn finish(mut self, tag: u32) -> Result<Value, Error> {
        let vals = std::mem::take(&mut self.vals);
        tagged(&mut self.scope, tag, &vals)
    }

    /// Build `(tag field...)` and record its source position — see
    /// [`tagged_at`].
    pub(crate) fn finish_at(mut self, tag: u32, loc: Option<Loc>) -> Result<Value, Error> {
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
/// `SymRef` constant — an integer test on the symbol's identity, CL's `eq`.
/// [`op`] returns the tag's *text*, which is for printing it, not for
/// recognizing it.
pub fn op_sym(heap: &Heap, form: Value) -> Option<crate::SymRef> {
    match heap.car(form).ok()? {
        Value::Symbol(id) => Some(id),
        _ => None,
    }
}

/// Whether `form` is a node tagged with the vocabulary member `konst` names.
///
/// The one-tag shorthand for [`op_sym`]: still an identity test on the tag
/// symbol, just spelled as a question when there is only one answer worth
/// asking about.
pub fn op_is(heap: &Heap, form: Value, konst: u32) -> bool {
    op_sym(heap, form).is_some_and(|s| s.is(konst))
}

/// Which vocabulary member a node's tag is, or `None` if `form` is not a
/// tagged list — for dispatching over the tags with a `match`.
pub fn op_wk(heap: &Heap, form: Value) -> Option<u32> {
    op_sym(heap, form).map(|s| s.well_known())
}

/// A node's tag *spelled out*, or `None` if `form` is not a tagged list — for
/// error messages and diagnostics only. To recognize a tag use [`op_wk`] or
/// [`op_is`].
pub fn op_spelling(heap: &Heap, form: Value) -> Option<&str> {
    match heap.car(form).ok()? {
        Value::Symbol(id) => Some(heap.symbol_spelling(id)),
        _ => None,
    }
}

/// `(module PATH BODY...)` — a bundle of top-level forms.
///
/// Here rather than on `Checker` because the *driver* is what knows a group of
/// forms belongs together: a file's own definitions, wrapped once the whole
/// file has been checked.
pub(crate) fn tagged_module(heap: &mut Heap, path: &crate::Path, body: &[Value]) -> Result<Value, Error> {
    let mut s = RootScope::new(heap);
    for form in body {
        s.push_root(*form);
    }
    let p = root_path_form(&mut s, path);
    s.push_root(p);
    let mut items = vec![p];
    items.extend_from_slice(body);
    tagged(&mut s, wk::MODULE, &items)
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

/// A definition, as a core form field writes one: the symbol it is defined
/// under. That symbol's home is the module that defines it (the checker
/// refuses a name it cannot own), so the symbol alone names the definition,
/// in a form read back in any module. [`definition_value`] reads it back.
///
/// # Panics
///
/// When `p`'s symbol is not of `p`'s module: `p` does not name a definition.
pub fn definition_form(p: &crate::Path) -> Value {
    let sym = p.name_symbol();
    assert!(
        sym.home() == p.ns(),
        "`{}` names no definition: its symbol is of `{}`",
        p,
        typelisp_mem::symbols::ns_path(sym.home()).join("::")
    );
    Value::Symbol(sym)
}

/// The path from the root to `p`, as the reader reads `::a::b::name` — the
/// root's empty-named symbol, then each module's symbol in the tree, then the
/// name's. The `module` and `use` forms name modules and `use` names this
/// way: a module's name is read in its parent, where it may be a symbol made
/// elsewhere (one `std` offers), so its symbol alone would not say where the
/// module is. [`root_path_value`] reads it back.
pub(crate) fn root_path_form(heap: &mut Heap, p: &crate::Path) -> Value {
    let mut segs = vec![typelisp_mem::symbols::intern_in(typelisp_mem::NsId::ROOT, "")];
    segs.extend(typelisp_mem::symbols::ns_names(p.ns()));
    segs.push(p.name_symbol());
    heap.intern_path(&segs)
}

/// The path [`root_path_form`] wrote. `None` when `v` is not a
/// path from the root.
pub fn root_path_value(heap: &Heap, v: Value) -> Option<crate::Path> {
    let Value::Path(id) = v else { return None };
    let (ns, name) = typelisp_mem::symbols::definition_of_path(&heap.path_segments(id))?;
    Some(crate::Path::of_symbol(ns, name))
}

/// A type, as a core form field writes one: the form
/// [`typelisp_mem::type_key_of_form`] reads a value's runtime identity off.
/// A type parameter — of the owning type, in a field's template, or of a
/// generic definition, in a body that is lowered but never run — is written
/// `(type-param N)` by its position.
pub(crate) fn type_form(heap: &mut Heap, ty: &Type) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    let tag = match ty {
        Type::Named(p, args) | Type::Dyn(p, args) => {
            let v = definition_form(p);
            f.push(v);
            for a in args {
                let v = type_form(f.heap(), a)?;
                f.push(v);
            }
            if matches!(ty, Type::Named(..)) { wk::TYPE_NAMED } else { wk::TYPE_DYN }
        }
        Type::Fn(ps, rest, ret) => {
            let mut l = Items::new(f.heap());
            for p in ps {
                let v = type_form(l.heap(), p)?;
                l.push(v);
            }
            let l = l.finish_list()?;
            f.push(l);
            let r = match rest {
                Some(t) => type_form(f.heap(), t)?,
                None => Value::Empty,
            };
            f.push(r);
            let r = type_form(f.heap(), ret)?;
            f.push(r);
            wk::TYPE_FN
        }
        Type::PtrTo(t) => {
            let v = type_form(f.heap(), t)?;
            f.push(v);
            wk::TYPE_PTR
        }
        Type::CStruct(p) => {
            let v = definition_form(p);
            f.push(v);
            wk::TYPE_CSTRUCT
        }
        // A field's type is written in its type's own parameters, by
        // position, and so is a generic body's.
        Type::Param(p) => {
            f.push(Value::Int(p.index as i64));
            wk::TYPE_PARAM
        }
        Type::Infer(_) => unreachable!("type_form: an inference hole left the checker"),
        Type::Proj(..) => unreachable!("type_form: a specialization writes the type an associated type is, not `{}`", ty),
        Type::Unit => wk::TYPE_UNIT,
        Type::Never => wk::TYPE_NEVER,
        Type::Ptr
        | Type::CLong
        | Type::CULong
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::F32
        | Type::F64
        | Type::Int
        | Type::Ratio
        | Type::Rational
        | Type::RandomState
        | Type::Bool
        | Type::Char
        | Type::Str
        | Type::Symbol => {
            let v = definition_form(&crate::Path::primitive(ty.scalar_name().expect("a scalar has a name")));
            f.push(v);
            wk::TYPE_NAMED
        }
    };
    f.finish(tag)
}

/// Core IR written as text, with each definition named as the checker
/// writes it ([`definition_form`]): text names a symbol of another module
/// only by its path from the root (`::f`, `::m::f`), so every such path in
/// `form` is replaced, in place, by the definition's symbol. The paths the
/// checker writes as paths ([`root_path_form`]: a `module`'s name, a `use`'s
/// fields) and quoted data are left as written, and so is a path that does
/// not start at the root. For IR that is written by hand rather than
/// lowered — the tests of the readers of core IR. Allocates nothing.
pub fn name_written_definitions(heap: &mut Heap, form: Value) -> Value {
    match form {
        Value::Path(_) => match root_path_value(heap, form) {
            Some(p) => definition_form(&p),
            None => form,
        },
        Value::Cons(_) => {
            let (mut cell, skip) = match op_wk(heap, form) {
                Some(wk::QUOTE | wk::USE) => return form,
                Some(wk::MODULE) => (heap.cdr(form).expect("a cons has a cdr"), 1),
                _ => (form, 0),
            };
            for _ in 0..skip {
                if let Value::Cons(_) = cell {
                    cell = heap.cdr(cell).expect("a cons has a cdr");
                }
            }
            while let Value::Cons(_) = cell {
                let car = heap.car(cell).expect("a cons has a car");
                let named = name_written_definitions(heap, car);
                if named != car {
                    heap.set_car(cell, named).expect("a cons has a car");
                }
                cell = heap.cdr(cell).expect("a cons has a cdr");
            }
            form
        }
        other => other,
    }
}

/// A node's `i`th field read as the definition it names
/// ([`definition_value`]). Shared by everything that walks *checked*
/// top-level forms looking for what they define (`compile::aot`,
/// `compile::prelude_bootstrap`).
pub fn definition_field(heap: &Heap, form: Value, i: usize) -> Option<crate::Path> {
    definition_value(field(heap, form, i)?)
}

/// The definition a core form names with `v`, as [`definition_form`] writes
/// one. `None` when `v` is not a symbol.
pub fn definition_value(v: Value) -> Option<crate::Path> {
    let Value::Symbol(sym) = v else { return None };
    Some(crate::Path::named_by(sym))
}

/// A node's `i`th field read as a symbol's name, or `None` if it is not a
/// symbol.
///
/// The `Option`-returning counterpart of the evaluator's own `sym_field`, which
/// reports the mismatch as an internal error instead: a walk over checked forms
/// looking for what they mention has nowhere to report to, and every field it
/// does not recognize is simply not the one it was after.
pub(crate) fn sym_field(heap: &Heap, form: Value, i: usize) -> Option<SymRef> {
    match field(heap, form, i)? {
        Value::Symbol(id) => Some(id),
        _ => None,
    }
}

/// [`sym_field`]'s symbol's spelling.
pub(crate) fn sym_name_field(heap: &Heap, form: Value, i: usize) -> Option<String> {
    match field(heap, form, i)? {
        Value::Symbol(id) => Some(heap.symbol_spelling(id).to_string()),
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
        Value::Symbol(id) => out.push_str(heap.symbol_spelling(id)),
        Value::Str(id) => {
            let _ = write!(out, "{:?}", heap.string(id));
        }
        Value::Path(id) => {
            let segs = heap.path_segments(id);
            for (i, s) in segs.iter().enumerate() {
                if i > 0 {
                    out.push_str("::");
                }
                out.push_str(heap.symbol_spelling(*s));
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
        // `{:?}` so an integral float keeps its point (`1.0`, not `1`) and
        // stays distinguishable from `(int 1)` in a comparison. The `f32` is
        // marked, so an assertion over one cannot pass by matching the `f64`
        // that happens to hold the same number.
        Value::Boxed(id) if heap.is_f32(id) => {
            let _ = write!(out, "{:?}f32", heap.f32_value(id));
        }
        Value::Boxed(id) if heap.is_f64(id) => {
            let _ = write!(out, "{:?}", heap.f64_value(id));
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

#[cfg(test)]
mod tests {
    use typelisp_mem::{type_key_of_form, Heap, TypeKey};

    use super::type_form;
    use crate::types::{Path, Type};

    /// A type written as a form and read back is the key the type spells
    /// itself as — the one property that lets the interpreter and the compiled
    /// code's runtime read the IR's types without spelling one type two ways.
    #[test]
    fn a_type_form_reads_back_as_the_types_own_key() {
        let mut heap = Heap::with_capacity(64);
        let point = Path::of(&["m", "point"]);
        let types = [
            Type::I32,
            Type::Int,
            Type::Str,
            Type::Unit,
            Type::Never,
            Type::Named(point.clone(), vec![]),
            Type::Named(Path::builtin(typelisp_mem::wk::VECTOR), vec![Type::Named(point.clone(), vec![Type::Char])]),
            Type::Named(Path::builtin(typelisp_mem::wk::HASHTABLE), vec![Type::Str, Type::I32]),
            Type::Dyn(Path::root("shape"), vec![]),
            Type::Fn(vec![Type::I32, Type::Str], Some(Box::new(Type::Int)), Box::new(Type::Bool)),
            Type::Fn(vec![], None, Box::new(Type::Unit)),
            Type::PtrTo(Box::new(Type::U8)),
            Type::CStruct(Path::root("timeval")),
        ];
        for ty in &types {
            let form = type_form(&mut heap, ty).expect("type form");
            assert_eq!(type_key_of_form(&heap, form).as_deref(), Some(ty.type_key().as_str()), "{:?}", ty);
        }
    }
}
