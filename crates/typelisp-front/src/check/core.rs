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
    Local { name: SymRef },
    /// A module's global variable, by its full path, read at its declared
    /// type `ty`.
    Global { path: crate::Path, ty: Type },
    /// A named free function reified as a value, with the parameter types of
    /// the function it names.
    FnRef { path: crate::Path, params: Vec<Type> },
    /// A method of `owner` reified as a value: `method` is the name it is
    /// registered under (a specialization's own name for a generic one).
    MethodRef { owner: crate::Path, method: SymRef, params: Vec<Type>, ret: Type },
    /// A call of the free function at `path`.
    Call { path: crate::Path, args: Vec<Checked> },
    /// A value of variant `variant` of `owner<targs>`; `field_tys` are the
    /// variant's declared field types with `targs` substituted in.
    Construct { owner: crate::Path, targs: Vec<Type>, variant: usize, mutable: bool, field_tys: Vec<Type>, args: Vec<Checked> },
    /// A niched `Option` put into the box a `Sexpr` holds it as.
    BoxOption { value: Box<Checked> },
    /// The type `ty`, as the value an internal builtin is handed a type in.
    TypeValue { ty: Type },
    /// The concrete value inside a trait object, as a `Sexpr` datum.
    DynValue { value: Box<Checked> },
    /// A call of the method `method` of `owner` (`args[0]` is the receiver
    /// when `instance`), returning `ret`. A builtin's call over literal
    /// arguments lowers to its value.
    Assoc { owner: crate::Path, method: SymRef, instance: bool, ret: Type, args: Vec<Checked> },
    /// A trait object re-made around the table of the trait `to`.
    DynUpcast { to: crate::Path, value: Box<Checked> },
    /// The body of a generic function where `what` needs the type arguments:
    /// never executed, since every specialization checks the site again.
    ErasedGeneric { what: Box<str> },
    /// `value` boxed as a trait object of `trait_path`; `slots` and `supers`
    /// are the vtables.
    DynNew {
        trait_path: crate::Path,
        slots: Vec<(crate::Path, SymRef)>,
        supers: Vec<(crate::Path, Vec<(crate::Path, SymRef)>)>,
        value: Box<Checked>,
    },
    /// A call through a trait object's vtable slot `slot`; `targets` are the
    /// bodies it may reach.
    DynCall { trait_path: crate::Path, method: SymRef, slot: usize, targets: Vec<(crate::Path, SymRef)>, args: Vec<Checked> },
    /// `binds` in scope for `body`, which runs in order; each binding is its
    /// name, its type and its value. With no bindings this is `progn`.
    Let { binds: Vec<(SymRef, Type, Checked)>, body: Vec<Checked> },
    /// An assignment to the local `name`.
    Set { name: SymRef, value: Box<Checked> },
    /// An assignment to the global at `path`, declared `ty`.
    SetGlobal { path: crate::Path, ty: Type, value: Box<Checked> },
    /// Applying the function value `callee`, which returns `ret`.
    Apply { callee: Box<Checked>, ret: Type, args: Vec<Checked> },
    /// An anonymous function.
    Lambda { params: Vec<(SymRef, Type)>, ret: Type, body: Vec<Checked> },
    /// Local functions in scope for each other and for `body`.
    Labels { defs: Vec<super::analyze::LabelsDef>, body: Vec<Checked> },
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
    /// Running `call` as a task or a thread (`tag`), answering a `handle`.
    Spawn { tag: u32, handle: Type, call: Box<Checked> },
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
    /// An empty body, whose value is `()`, where the type is to be this one.
    EmptyBody(Type),
    /// A `where` bound of `of` on its type parameter `param` that the type
    /// the call decided for it does not meet.
    Bound { of: String, param: String, unmet: Unmet },
    /// A `match` whose arms leave values of the type it matches on unmatched.
    Uncovered(Uncovered),
    /// A `make-instance` of `ty` that gives `slot`, which is not `pub` and is
    /// given outside `module`, where `ty` is defined.
    HiddenSlot { ty: String, slot: String, module: String },
    /// What needs `(unsafe ...)`, outside one.
    Unsafe(Unsafe),
    /// A `setf` of a place that cannot be assigned.
    Unassignable(Unassignable),
    /// A value of `ty` reaching `:dyn trait_path<pins>`, which it can be
    /// neither boxed as nor converted to.
    NotDyn { ty: Type, trait_path: crate::Path, pins: Vec<Type>, why: NotDyn },
}

/// Why a value is not admitted as a trait object.
#[derive(Clone, Debug, PartialEq)]
pub enum NotDyn {
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
    /// It is a `:dyn from` already, and the trait is not in `from`'s
    /// supertrait chain.
    OutsideChain { from: crate::Path },
    /// It is a `:dyn from` already, whose pins make the trait's `name`
    /// `actual`, and the `:dyn` pins it to `want`.
    PinDiffers { from: crate::Path, name: String, actual: Type, want: Type },
    /// It is a `:dyn from` already, whose pins leave the trait's `name` open.
    Unpinned { from: crate::Path, name: String },
}

/// Why a place a `setf` names cannot be assigned.
#[derive(Clone, Debug, PartialEq)]
pub enum Unassignable {
    /// The global `name`, a constant.
    Constant { name: String },
    /// A C struct field holding a struct of `ty` by value, whose own fields
    /// are what is assigned.
    WholeCStruct { ty: Type },
}

/// What only `(unsafe ...)` permits, or what it cannot answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Unsafe {
    /// A call to the C function `name` a `defffi` declared.
    CCall { name: crate::Path },
    /// A value of `ty`, which mentions a raw machine word from the C FFI.
    RawWord { ty: Type },
    /// An `(unsafe ...)` answering `ty`, which mentions a typed pointer into
    /// memory freed when the `unsafe` is left.
    TypedPtrAnswer { ty: Type },
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
            | Node::TypeValue { .. }
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
    /// `((var (recv chan)) body...)`; `bound` is the `Option` `var` is
    /// bound to.
    Recv { var: SymRef, bound: Type, chan: Checked, body: Checked },
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
    /// The type `ty`, as the value an internal builtin is handed a type in.
    pub fn type_value(ty: &Type) -> Checked {
        Checked::of(Node::TypeValue { ty: ty.clone() }, Type::Int)
    }

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
pub fn root_path_form(heap: &mut Heap, p: &crate::Path) -> Value {
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
///
/// `params` are the owning type's parameters, for a field's template: the
/// `N`th is written `(type-param N)`. Any other type variable is a generic
/// definition's own parameter, in a body that is lowered but never run, and
/// is written `(type-var "NAME")`.
pub fn type_form(heap: &mut Heap, ty: &Type, params: &[String]) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    let tag = match ty {
        Type::Named(p, args) | Type::Dyn(p, args) => {
            let v = definition_form(p);
            f.push(v);
            for a in args {
                let v = type_form(f.heap(), a, params)?;
                f.push(v);
            }
            if matches!(ty, Type::Named(..)) { wk::TYPE_NAMED } else { wk::TYPE_DYN }
        }
        Type::Fn(ps, rest, ret) => {
            let mut l = Items::new(f.heap());
            for p in ps {
                let v = type_form(l.heap(), p, params)?;
                l.push(v);
            }
            let l = l.finish_list()?;
            f.push(l);
            let r = match rest {
                Some(t) => type_form(f.heap(), t, params)?,
                None => Value::Empty,
            };
            f.push(r);
            let r = type_form(f.heap(), ret, params)?;
            f.push(r);
            wk::TYPE_FN
        }
        Type::PtrTo(t) => {
            let v = type_form(f.heap(), t, params)?;
            f.push(v);
            wk::TYPE_PTR
        }
        Type::CStruct(p) => {
            let v = definition_form(p);
            f.push(v);
            wk::TYPE_CSTRUCT
        }
        Type::Var(name) => match params.iter().position(|p| p == name) {
            Some(i) => {
                f.push(Value::Int(i as i64));
                wk::TYPE_PARAM
            }
            None => {
                let v = f.heap().alloc_string(name.clone());
                f.push(v);
                wk::TYPE_VAR
            }
        },
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
pub fn sym_field(heap: &Heap, form: Value, i: usize) -> Option<SymRef> {
    match field(heap, form, i)? {
        Value::Symbol(id) => Some(id),
        _ => None,
    }
}

/// [`sym_field`]'s symbol's spelling.
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
            let form = type_form(&mut heap, ty, &[]).expect("type form");
            assert_eq!(type_key_of_form(&heap, form).as_deref(), Some(ty.type_key().as_str()), "{:?}", ty);
        }
    }
}
