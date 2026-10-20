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

use typelisp_mem::{wk, Error, Heap, Loc, RootScope, Value};

use crate::types::Type;

/// What analyzing one expression produces: the expression as the analysis
/// resolved it — a node of the analyzed tree (`docs/dev/explicit-symbols-plan.md`,
/// 3.10) — and the type it was proved to have.
///
/// The type is the checker's own working value — it drives the next inference
/// step, the next arity check, the next `Repr` — and it stops here: lowering
/// (`check::lower`) writes only representations into the core form, and
/// nothing downstream of the checker can ask for a type.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub ty: Type,
    pub node: Node,
    /// Where the expression was written, which its lowered form carries.
    pub loc: Option<Loc>,
}

/// One node of the analyzed tree: an expression with what it refers to
/// resolved. Its children are the [`Checked`] of each subexpression.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// `(if cond then else)`.
    If { cond: Box<Checked>, then: Box<Checked>, els: Box<Checked> },
    /// `(panic message)`.
    Panic { message: Box<Checked> },
    /// A literal: its core tag (`int`, `int-any-width`, `bignum`, `ratio`,
    /// `float-any-width`, `bool`, `char`, `str`, `sym`, `unit`) and its value,
    /// none for `unit`. The tag is the node's, decided from the literal's
    /// type, not something lowering could read off the value.
    Literal { tag: u32, value: Option<Value> },
    /// `(quote datum)`, also a self-evaluating object of no reader syntax.
    Quote { datum: Value },
    /// A local binding.
    Local { name: Box<str> },
    /// A module's global variable, by its full path, read at its declared
    /// type `ty`.
    Global { path: crate::Path, ty: Type },
    /// A named free function reified as a value, with the parameter types of
    /// the function it names.
    FnRef { path: crate::Path, params: Vec<Type> },
    /// A method of `owner` reified as a value: `method` is the name it is
    /// registered under (a specialization's own name for a generic one).
    MethodRef { owner: crate::Path, method: Box<str>, params: Vec<Type>, ret: Type },
    /// A call of the free function at `path`.
    Call { path: crate::Path, args: Vec<Checked> },
    /// A value of variant `variant` of `owner<targs>`; `field_tys` are the
    /// variant's declared field types with `targs` substituted in.
    Construct { owner: crate::Path, targs: Vec<Type>, variant: usize, mutable: bool, field_tys: Vec<Type>, args: Vec<Checked> },
    /// A niched `Option` put into the box a `Sexpr` holds it as; `key` is the
    /// `Option`'s runtime identity.
    BoxOption { key: Box<str>, value: Box<Checked> },
    /// The concrete value inside a trait object, as a `Sexpr` datum.
    DynValue { value: Box<Checked> },
    /// A call of the method `method` of `owner` (`args[0]` is the receiver
    /// when `instance`), returning `ret`. A builtin's call over literal
    /// arguments lowers to its value.
    Assoc { owner: crate::Path, method: Box<str>, instance: bool, ret: Type, args: Vec<Checked> },
    /// A trait object re-made around the table of the trait `to`.
    DynUpcast { to: crate::Path, value: Box<Checked> },
    /// The body of a generic function where `what` needs the type arguments:
    /// never executed, since every specialization checks the site again.
    ErasedGeneric { what: Box<str> },
    /// `value` boxed as a trait object of `trait_path`; `key` is its concrete
    /// type's identity, `slots` and `supers` the vtables.
    DynNew {
        key: Box<str>,
        trait_path: crate::Path,
        slots: Vec<(crate::Path, String)>,
        supers: Vec<(crate::Path, Vec<(crate::Path, String)>)>,
        value: Box<Checked>,
    },
    /// A call through a trait object's vtable slot `slot`; `targets` are the
    /// bodies it may reach.
    DynCall { trait_path: crate::Path, method: Box<str>, slot: usize, targets: Vec<(crate::Path, String)>, args: Vec<Checked> },
    /// `binds` in scope for `body`, which runs in order; each binding is its
    /// name, its type and its value. With no bindings this is `progn`.
    Let { binds: Vec<(String, Type, Checked)>, body: Vec<Checked> },
    /// An assignment to the local `name`.
    Set { name: Box<str>, value: Box<Checked> },
    /// An assignment to the global at `path`, declared `ty`.
    SetGlobal { path: crate::Path, ty: Type, value: Box<Checked> },
    /// Applying the function value `callee`, which returns `ret`.
    Apply { callee: Box<Checked>, ret: Type, args: Vec<Checked> },
    /// An anonymous function.
    Lambda { params: Vec<(String, Type)>, ret: Type, body: Vec<Checked> },
    /// Local functions in scope for each other and for `body`.
    Labels { defs: Vec<super::checker::LabelsDef>, body: Vec<Checked> },
    /// `(loop body...)`, whose exits deliver a `ty`.
    Loop { ty: Type, body: Vec<Checked> },
    /// Leaving the nearest loop with no value.
    Break,
    /// Leaving the nearest loop, with `value` or none.
    Return { value: Option<Box<Checked>> },
    /// A named escape target of value `ty`.
    Block { name: Box<str>, ty: Type, body: Box<Checked> },
    /// Leaving the enclosing block `name`, with `value` or none.
    ReturnFrom { name: Box<str>, value: Option<Box<Checked>> },
    /// `body`, or the value a `throw` to the symbol `tag` delivers; `carried`
    /// is the type the tag carries.
    Catch { tag: Value, carried: Type, body: Box<Checked> },
    /// Leaving for the nearest `catch` on the symbol `tag` with `value`.
    Throw { tag: Value, value: Box<Checked> },
    /// `protected`, then `cleanup` on every way out of it.
    UnwindProtect { protected: Box<Checked>, cleanup: Box<Checked> },
    /// An `int`'s raw machine word.
    UntagInt { value: Box<Checked> },
    /// Waiting until one of the channel operations `arms` can go, and running
    /// that arm. Its operands are locals bound around it.
    Select { arms: Vec<SelectArm> },
    /// `(match scrut arms...)`: the first arm whose pattern matches runs.
    Match { scrut: Box<Checked>, arms: Vec<(super::resolved::Pattern, Vec<Checked>)> },
    /// A niched `Option` of type `ty` holding `value`: `value` as a tagged
    /// field word, with no box.
    SomeOf { ty: Type, value: Box<Checked> },
    /// Running `call` as a task or a thread (`tag`); `key` is the identity
    /// of the handle it answers.
    Spawn { tag: u32, key: Box<str>, call: Box<Checked> },
    /// `(compile name)`.
    Compile { target: super::resolved::CompileTarget },
    /// `(trace ...)` or `(untrace ...)` (`tag`).
    Trace { tag: u32, targets: Vec<super::resolved::CompileTarget> },
    /// `(disassemble name)`, as LLVM IR when `llvm_ir`.
    Disassemble { target: super::resolved::CompileTarget, llvm_ir: bool },
    /// `(step form)`.
    Step { inner: Box<Checked> },
    /// What stands for a subexpression that failed to check, written at
    /// `loc`.
    Hole { loc: Option<Loc> },
    /// An `&optional`/`&key` default, checked once with its definition and
    /// copied into each call that omits the argument.
    Default { form: crate::owned_form::OwnedForm },
    /// `value`, which disagrees with what was settled before it. The analysis
    /// goes on with the type settled first; checking reports `what`
    /// (`check::verify`).
    Disagree { what: Disagreement, value: Box<Checked> },
}

/// What a [`Node::Disagree`] disagrees with.
#[derive(Clone, Debug, PartialEq)]
pub enum Disagreement {
    /// An expected type the value is not, and no widening reconciles.
    Expected(Type),
    /// Two ways out of one expression — branches, arms, a block's end and its
    /// `return-from`s — whose types have nothing in common.
    Branches(Type, Type),
    /// A part of an argument's type, found, where its parameter's type has
    /// the expected one.
    Mismatch(Type, Type),
    /// A type parameter an earlier argument bound to the first type, and this
    /// one to the second.
    Conflict(String, Type, Type),
    /// A call none of whose arguments decided the type parameter `param` of
    /// `of`; `noun` is how the parameter is spoken of there.
    Uninferred { noun: &'static str, param: String, of: String },
    /// A `where` bound of `of` on its type parameter `param` that the type
    /// the call decided for it does not meet.
    Bound { of: String, param: String, unmet: Unmet },
    /// A `match` whose arms leave values of the type it matches on unmatched.
    Uncovered(Uncovered),
}

/// What a `match`'s arms leave unmatched.
#[derive(Clone, Debug, PartialEq)]
pub enum Uncovered {
    /// Some of the `total` variants of `adt`; the arms cover `covered`.
    Variants { adt: crate::Path, covered: usize, total: usize },
    /// One of the two `bool`s, `missing`.
    Bool { missing: bool },
    /// Values of `ty`, which has no variants, so only a `_` arm covers it.
    ByValue { ty: Type },
}

/// How a type decided for a bounded type parameter falls short of the bound.
#[derive(Clone, Debug, PartialEq)]
pub enum Unmet {
    /// It is the enclosing function's own type parameter `var`, which its
    /// `where` clause does not bound by `trait_path`.
    Undeclared { var: String, trait_path: crate::Path },
    /// `ty` does not implement `trait_path`.
    Unimplemented { ty: Type, trait_path: crate::Path },
    /// `ty` names no type, so it has no impls.
    NoImpls { ty: Type },
    /// `ty`'s associated type `assoc` is `actual` (`None`: it has none), not
    /// the `required` one.
    Assoc { ty: Type, assoc: String, actual: Option<Type>, required: Type },
}

impl Node {
    /// The node's subexpressions, in the order they are written.
    pub fn children(&self) -> Vec<&Checked> {
        fn one(c: &Checked) -> Vec<&Checked> {
            vec![c]
        }
        match self {
            Node::If { cond, then, els } => vec![cond, then, els],
            Node::Panic { message: c }
            | Node::BoxOption { value: c, .. }
            | Node::DynValue { value: c }
            | Node::DynUpcast { value: c, .. }
            | Node::DynNew { value: c, .. }
            | Node::Set { value: c, .. }
            | Node::SetGlobal { value: c, .. }
            | Node::Block { body: c, .. }
            | Node::Catch { body: c, .. }
            | Node::Throw { value: c, .. }
            | Node::UntagInt { value: c }
            | Node::SomeOf { value: c, .. }
            | Node::Spawn { call: c, .. }
            | Node::Step { inner: c }
            | Node::Disagree { value: c, .. } => one(c),
            Node::Call { args, .. } | Node::Construct { args, .. } | Node::Assoc { args, .. } | Node::DynCall { args, .. } => {
                args.iter().collect()
            }
            Node::Let { binds, body } => binds.iter().map(|(_, _, c)| c).chain(body).collect(),
            Node::Apply { callee, args, .. } => std::iter::once(&**callee).chain(args).collect(),
            Node::Lambda { body, .. } | Node::Loop { body, .. } => body.iter().collect(),
            Node::Labels { defs, body } => defs.iter().flat_map(|(_, _, _, b)| b).chain(body).collect(),
            Node::Return { value } | Node::ReturnFrom { value, .. } => value.iter().map(|c| &**c).collect(),
            Node::UnwindProtect { protected, cleanup } => vec![protected, cleanup],
            Node::Select { arms } => arms
                .iter()
                .flat_map(|arm| match arm {
                    SelectArm::Recv { chan, body, .. } => vec![chan, body],
                    SelectArm::Send { chan, value, body, .. } => vec![chan, value, body],
                    SelectArm::Else { body } => vec![body],
                })
                .collect(),
            Node::Match { scrut, arms } => {
                let mut out = vec![&**scrut];
                for (pat, body) in arms {
                    pat.guards(&mut out);
                    out.extend(body);
                }
                out
            }
            Node::Literal { .. }
            | Node::Quote { .. }
            | Node::Local { .. }
            | Node::Global { .. }
            | Node::FnRef { .. }
            | Node::MethodRef { .. }
            | Node::ErasedGeneric { .. }
            | Node::Break
            | Node::Compile { .. }
            | Node::Trace { .. }
            | Node::Disassemble { .. }
            | Node::Hole { .. }
            | Node::Default { .. } => Vec::new(),
        }
    }
}

/// One arm of a [`Node::Select`].
#[derive(Clone, Debug, PartialEq)]
pub enum SelectArm {
    /// `((var (recv chan)) body...)`; `key` is the identity of the
    /// `Option` `var` is bound to.
    Recv { var: Box<str>, key: Box<str>, chan: Checked, body: Checked },
    /// `((send chan value) body...)`; `kind` is how the value is stored.
    Send { kind: i64, chan: Checked, value: Checked, body: Checked },
    /// `(:else body...)`.
    Else { body: Checked },
}

/// A tree is as deep as the expression it was analyzed from, and dropping it
/// visits it recursively: each level grows the stack when it runs short, as
/// the walks that built it do.
impl Drop for Checked {
    fn drop(&mut self) {
        let node = std::mem::replace(&mut self.node, Node::Break);
        typelisp_mem::grow_stack(move || drop(node));
    }
}

impl Checked {
    /// The same expression, seen at type `ty`: a widening that needs no
    /// runtime work (a `Symbol` where a `Sexpr` is expected, ...). The node
    /// stays as it is.
    pub fn retyped(mut self, ty: Type) -> Checked {
        self.ty = ty;
        self
    }

    /// The expression `node` stands for, of type `ty`.
    ///
    /// A heap value the node holds (a literal's box, a quoted datum) has to
    /// stay rooted until the tree is lowered; `Checker::literal` and
    /// `Checker::quoted` build those nodes.
    pub(super) fn of(node: Node, ty: Type) -> Checked {
        Checked { ty, node, loc: None }
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
    pub fn finish_at(mut self, tag: u32, loc: Option<Loc>) -> Result<Value, Error> {
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
        Value::Symbol(id) => Some(crate::Path::root(heap.symbol_spelling(id))),
        _ => None,
    }
}

/// A node's `i`th field read as a symbol's name, or `None` if it is not a
/// symbol.
///
/// The `Option`-returning counterpart of the evaluator's own `sym_field`, which
/// reports the mismatch as an internal error instead: a walk over checked forms
/// looking for what they mention has nowhere to report to, and every field it
/// does not recognize is simply not the one it was after.
pub fn sym_name_field(heap: &Heap, form: Value, i: usize) -> Option<String> {
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
