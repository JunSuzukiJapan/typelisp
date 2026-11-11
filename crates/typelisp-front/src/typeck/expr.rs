//! Checking an expression: which kind of form it is, and the module that
//! checks that kind.

use std::cell::{Cell, RefCell};

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;

use super::format::{check_directives, Directives};
use super::widen::conform;
use super::literal::literal;
use super::sexpr_value::{encodable, no_encoding};
use super::locals::Locals;
use super::registry::{MethodDef, Registry, ValueDef};
use super::resolve::{ctor_named, receiver_method, static_use_named, value_named};
use super::tree::{Node, Typed};
use super::scope::TypeScope;
use super::type_syntax::TypeCx;
use super::unify::{Holes, Kind};
use super::looping::LoopExit;
use super::args::First;
use super::bounds::{bound_method, meets, Meets};
use crate::check::bound_message;
use crate::check::registry::TraitBound;
use super::{binding, block, call, catch, cast, collection_literal, control, dyn_trait, lambda, select, spawn, format, instance, looping, macros, matching, method, modify, path_call, list_form, quote, setf, sugar, tools, the, variant};
use crate::check::analyze::TOPLEVEL_FORM_HEADS;
use super::type_syntax::mentions_raw_word;
use crate::check::{MacroExpander, SPECIAL_FORM_HEADS};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

/// What every expression of one top-level form is checked against: the
/// definitions, the module the form is in, the type parameters the
/// definition declares, and the form's inference holes.
pub(crate) struct Cx<'a> {
    pub(crate) reg: &'a Registry,
    /// What expands a macro call: the evaluator, which holds the bodies.
    pub(crate) expander: &'a dyn MacroExpander,
    pub(crate) ns: NsId,
    pub(crate) scope: &'a TypeScope<'a>,
    /// Inside an `impl`, the type `Self` stands for.
    pub(crate) self_type: Option<&'a Type>,
    pub(crate) holes: RefCell<Holes>,
    /// The type arguments the form's checking must decide: each one's type,
    /// and what to say, and where, when nothing does.
    undecided: RefCell<Vec<(Type, String, Option<Loc>)>>,
    /// The holes the form's number literals were given, each an integer or
    /// a float type nothing may have decided yet.
    numbers: RefCell<Vec<Type>>,
    /// The loops being checked, innermost last: what leaving each gives.
    pub(crate) loops: RefCell<Vec<LoopExit>>,
    /// The tags the form's `catch`es and `throw`s have said a type for,
    /// recorded in the registry when it is settled.
    pub(crate) tags: RefCell<Vec<(SymRef, Type)>>,
    /// The blocks being checked, innermost last.
    pub(crate) blocks: RefCell<Vec<super::block::BlockExit>>,
    /// The blocks the form has made, to name the next one.
    block_labels: Cell<u32>,
    /// How many `(unsafe ...)` forms the expression being checked is in.
    unsafe_depth: Cell<u32>,
    /// The arena of the `unsafe` that owns the expression's allocations:
    /// none outside one, and whether a `c-alloc` has used it.
    c_arena: Cell<Option<bool>>,
    /// The control strings to check against their arguments' settled types.
    directives: RefCell<Vec<Directives>>,
    /// The bounds the form's calls require of their type arguments.
    required: RefCell<Vec<Required>>,
    /// The temporaries the form has made, to name the next one.
    temps: Cell<u32>,
}

impl<'a> Cx<'a> {
    pub(crate) fn new(reg: &'a Registry, expander: &'a dyn MacroExpander, ns: NsId, scope: &'a TypeScope<'a>, self_type: Option<&'a Type>) -> Cx<'a> {
        Cx { reg, expander, ns, scope, self_type, holes: RefCell::new(Holes::new()), undecided: RefCell::new(Vec::new()), numbers: RefCell::new(Vec::new()), loops: RefCell::new(Vec::new()), tags: RefCell::new(Vec::new()), blocks: RefCell::new(Vec::new()), block_labels: Cell::new(0), unsafe_depth: Cell::new(0), c_arena: Cell::new(None), directives: RefCell::new(Vec::new()), required: RefCell::new(Vec::new()), temps: Cell::new(0) }
    }

    /// Where a type written in this form is read.
    pub(crate) fn types(&self) -> TypeCx<'_> {
        TypeCx { reg: self.reg, ns: self.ns, scope: self.scope, self_type: self.self_type }
    }

    /// A local variable no program can name, for a value the checker holds
    /// for a moment.
    pub(crate) fn temp(&self) -> SymRef {
        let n = self.temps.get();
        self.temps.set(n + 1);
        typelisp_mem::symbols::intern(&format!("%place-tmp-{}", n))
    }

    /// Whether the expression being checked is inside an `(unsafe ...)`.
    pub(crate) fn in_unsafe(&self) -> bool {
        self.unsafe_depth.get() > 0
    }

    /// `f`, checked inside one more `(unsafe ...)`.
    pub(crate) fn within_unsafe<R>(&self, f: impl FnOnce() -> R) -> R {
        self.unsafe_depth.set(self.unsafe_depth.get() + 1);
        let r = f();
        self.unsafe_depth.set(self.unsafe_depth.get() - 1);
        r
    }

    /// Whether an `unsafe` owns the allocations made here.
    pub(crate) fn c_arena_open(&self) -> bool {
        self.c_arena.get().is_some()
    }

    /// A `c-alloc` uses the arena of the `unsafe` around it.
    pub(crate) fn c_arena_used(&self) {
        self.c_arena.set(Some(true));
    }

    /// `f`, checked as a function of its own, whose allocations need an
    /// `unsafe` of its own.
    pub(crate) fn own_arena<R>(&self, f: impl FnOnce() -> R) -> R {
        let outer = self.c_arena.replace(None);
        let r = f();
        self.c_arena.set(outer);
        r
    }

    /// The name in the core IR of a new block named `name`, unique in the
    /// form.
    pub(crate) fn block_label(&self, name: SymRef) -> Box<str> {
        let n = self.block_labels.get();
        self.block_labels.set(n + 1);
        format!("{}'{}", name.name(), n).into_boxed_str()
    }

    /// `ty` with every hole solved so far filled in.
    pub(crate) fn resolve(&self, ty: &Type) -> Type {
        self.holes.borrow().resolve(ty)
    }

    /// A hole of the kind `kind` for a number literal.
    pub(crate) fn number_hole(&self, kind: Kind) -> Type {
        let hole = self.holes.borrow_mut().fresh_of(kind);
        self.numbers.borrow_mut().push(hole.clone());
        hole
    }

    /// `ty` resolved, read as its default type when it is a number literal's
    /// hole nothing has decided yet — without deciding it.
    pub(crate) fn with_default(&self, ty: &Type) -> Type {
        let holes = self.holes.borrow();
        match holes.kind_of(ty).and_then(Kind::default_type) {
            Some(t) => t,
            None => holes.resolve(ty),
        }
    }

    /// Decide every number literal's hole in `ty` nothing has decided yet
    /// as its default type: what picks a definition by the type's arguments
    /// needs them.
    pub(crate) fn decide_all(&self, ty: &Type) {
        let mut holes = Vec::new();
        super::unify::walk(&self.resolve(ty), &mut |t| {
            if let Type::Infer(_) = t {
                holes.push(t.clone());
            }
        });
        holes.iter().for_each(|h| self.decide(h));
    }

    /// The instance method `name` a receiver of type `ty` answers to, as
    /// [`receiver_method`] says. When the type has a method of that name
    /// for one instantiation, which one is picked by its type arguments, so
    /// a number literal's hole in them is decided first. So it is when the
    /// method can only be a blanket `impl`'s, whose bounds the type's
    /// arguments may decide.
    pub(crate) fn receiver_method(&self, ty: &Type, name: SymRef) -> Option<(Path, &'a MethodDef)> {
        let ty = self.with_default(ty);
        if let Some(owner) = crate::types::nominal_path(&ty) {
            let blanket = self.reg.method(&owner, name).is_none() && self.reg.blanket_with_method(name).is_some();
            if self.reg.has_inst_method(&owner, name) || blanket {
                self.decide_all(&ty);
            }
        }
        receiver_method(self.reg, self.ns, &self.resolve(&ty), name)
    }

    /// Decide `ty`, when it is a number literal's hole nothing has decided
    /// yet, as its default type.
    pub(crate) fn decide(&self, ty: &Type) {
        let mut holes = self.holes.borrow_mut();
        if let Some(t) = holes.kind_of(ty).and_then(Kind::default_type) {
            holes.unify(ty, &t).expect("a hole of a kind is solved to that kind's default");
        }
    }

    /// Require that the type argument `ty` be decided by the time the form
    /// is checked; if it is not, the error says `what`, at `loc`.
    ///
    /// Not asked at once: a later argument, or the expectation of a form
    /// around this one, may decide it — `(Tree::Node (Tree::Leaf) 1
    /// (Tree::Leaf))`'s first `Leaf` has its `T` from the `1`.
    pub(crate) fn must_decide(&self, ty: Type, what: String, loc: Option<Loc>) {
        self.undecided.borrow_mut().push((ty, what, loc));
    }

    /// Require that the type argument `ty`, given the type parameter `param`
    /// of the callee `of`, meet `bound`: now, or once the form decides `ty`.
    pub(crate) fn require_bound(&self, ty: Type, bound: TraitBound, of: String, param: String, loc: Option<Loc>) -> Result<(), Error> {
        let r = Required { ty, bound, of, param, loc };
        match meets(self, &r.ty, &r.bound) {
            Meets::Yes => Ok(()),
            Meets::No(why) => Err(r.error(self.reg, &why)),
            Meets::NotYet => {
                self.required.borrow_mut().push(r);
                Ok(())
            }
        }
    }

    /// Check `d`'s control string against its arguments once their types
    /// are settled ([`Self::settle`]): a later use may decide them.
    pub(crate) fn check_directives_later(&self, d: Directives) {
        self.directives.borrow_mut().push(d);
    }

    /// The checked trees of the form, every type in them settled: a number
    /// literal's type nothing decided is the default (`int`, `f64`), then
    /// each hole is replaced by its solution, and each number literal
    /// becomes the literal of its type — refused there if it is out of that
    /// type's range. An error too if a type argument nothing decided was
    /// required ([`Self::must_decide`]), when a value held as a `Sexpr` has
    /// a type with no encoding, or when a control string's directives do not
    /// take its arguments.
    pub(crate) fn settle(&self, heap: &mut Heap, trees: &mut [Typed]) -> Result<(), Error> {
        for hole in self.numbers.borrow().iter() {
            self.decide(hole);
        }
        // A bound met decides the holes its pins have, which may decide the
        // type another bound is on.
        loop {
            let pending = std::mem::take(&mut *self.required.borrow_mut());
            let before = pending.len();
            let mut left = Vec::new();
            for r in pending {
                match meets(self, &r.ty, &r.bound) {
                    Meets::Yes => {}
                    Meets::No(why) => return Err(r.error(self.reg, &why)),
                    Meets::NotYet => left.push(r),
                }
            }
            let stuck = left.len() == before;
            *self.required.borrow_mut() = left;
            if stuck {
                break;
            }
        }
        for (ty, what, loc) in self.undecided.borrow().iter() {
            if self.holes.borrow().unsolved(ty).is_some() {
                let e = Error::TypeError(what.clone());
                return Err(match loc {
                    Some(l) => e.at(l.clone()),
                    None => e,
                });
            }
        }
        for t in trees.iter_mut() {
            t.map_types(&|ty| self.resolve(ty));
            t.walk_mut(&mut |t| settle_number(heap, t))?;
            t.walk_mut(&mut |t| match &t.node {
                Node::AsSexpr { value } if !encodable(self.reg, &value.ty) => Err(at(no_encoding(&value.ty), &t.loc)),
                _ => Ok(()),
            })?;
        }
        for d in self.directives.borrow().iter() {
            check_directives(d, |ty| self.resolve(ty))?;
        }
        for (tag, ty) in self.tags.borrow().iter() {
            super::catch::record(self, *tag, ty)?;
        }
        if let Some(r) = self.required.borrow().first() {
            unimplemented!("typeck: the `:where` bound on `{}` of `{}`, whose type argument nothing decided", r.param, r.of)
        }
        Ok(())
    }
}

/// A bound a call requires of a type argument ([`Cx::require_bound`]).
struct Required {
    ty: Type,
    bound: TraitBound,
    of: String,
    param: String,
    loc: Option<Loc>,
}

impl Required {
    /// The error for the type argument not meeting the bound, as `why`
    /// says: with a method of the type that answers a value that does, when
    /// it has one.
    fn error(&self, reg: &Registry, why: &crate::check::core::Unmet) -> Error {
        let why = match why {
            crate::check::core::Unmet::Unimplemented { ty, trait_path, via: None } => {
                crate::check::core::Unmet::Unimplemented { ty: ty.clone(), trait_path: *trait_path, via: method_yielding(reg, ty, *trait_path) }
            }
            other => other.clone(),
        };
        at(Error::TypeError(bound_message(&self.of, &self.param, &why)), &self.loc)
    }
}

/// A method of `ty` taking only the receiver whose value implements
/// `trait_path`, the first by name when there are several.
fn method_yielding(reg: &Registry, ty: &Type, trait_path: crate::types::Path) -> Option<SymRef> {
    let owner = crate::types::nominal_path(ty)?;
    let args = match ty {
        Type::Named(_, args) => args.as_slice(),
        _ => &[],
    };
    let owner_params = reg.type_def(&owner).map_or(0, |d| d.params.len());
    let bare = TraitBound { trait_path, assoc: Default::default() };
    let mut found: Vec<SymRef> = reg
        .methods_of(owner)
        .filter(|(_, m)| {
            let sig = &m.sig;
            // The type's parameters, and none of its own.
            m.instance && sig.params.len() == 1 && !super::args::has_tail(sig) && sig.type_params.len() == owner_params
        })
        .filter(|(_, m)| super::bounds::settled_meets(reg, &super::subst::Subst::positional(&m.sig.type_params, args).apply(&m.sig.ret), &bare))
        .map(|(name, _)| name)
        .collect();
    found.sort_by_key(|n| n.name().to_string());
    found.first().copied()
}

/// `e` at `loc`, when it is known.
fn at(e: Error, loc: &Option<Loc>) -> Error {
    match loc {
        Some(l) => e.at(l.clone()),
        None => e,
    }
}

/// `t` as the literal of its settled type, when it is a number literal.
fn settle_number(heap: &mut Heap, t: &mut Typed) -> Result<(), Error> {
    let Node::Number { value } = t.node else { return Ok(()) };
    // An integer a hole of any numeric type held (`Kind::Number`) is that
    // type's.
    let value = match value {
        Value::Int(n) => {
            let v = crate::check::forms::numeric_identity_literal(heap, Some(&t.ty), n);
            heap.push_root(v);
            v
        }
        other => other,
    };
    let lit = literal(heap, value, Some(&t.ty))
        .map_err(|e| match &t.loc {
            Some(l) => e.at(l.clone()),
            None => e,
        })?
        .expect("a number is a literal");
    if let Some(v) = lit.value {
        heap.push_root(v);
    }
    t.node = Node::Literal { tag: lit.tag, value: lit.value };
    Ok(())
}

/// `v` checked where `expected` is expected (`None`: anything), with `locals`
/// bound. `loc` is where `v` was written, for an atom, which carries no
/// position of its own.
///
/// A value this allocates is rooted on `heap`'s root stack; the top-level
/// form's checking releases it after the tree is lowered.
pub(crate) fn check_expr(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    v: Value,
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let loc = heap.cons_loc(v).or(loc);
    let typed = typelisp_mem::grow_stack(|| check_form(cx, heap, locals, v, expected, loc.clone())).and_then(|t| conform(cx, t, expected));
    // A C word is a value only inside `(unsafe ...)`: nothing can check that
    // it points at anything. The `unsafe` form's own value is one, so that a
    // function can answer one; whoever uses it is inside an `unsafe` too.
    let typed = typed.and_then(|t| {
        let ty = cx.resolve(&t.ty);
        if !cx.in_unsafe() && mentions_raw_word(&ty) && !is_unsafe_form(heap, v) {
            return Err(Error::TypeError(format!(
                "a `{}` is a raw machine word from the C FFI and can only be a value \
                 inside `(unsafe ...)` — nothing here can check that it points at anything",
                ty
            )));
        }
        Ok(t)
    });
    typed.map_err(|e| match &loc {
        Some(l) => e.at(l.clone()),
        None => e,
    })
}

fn check_form(cx: &Cx, heap: &mut Heap, locals: &Locals, v: Value, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    // A literal's type depends on what is expected, which may be a hole an
    // earlier argument solved.
    let solved = expected.map(|e| cx.resolve(e));
    // A number where no type is known yet is one the rest of the form
    // decides, as in Rust: `(let ((x 1)) (id8 x))` makes `x` a `u8`.
    if matches!(solved, None | Some(Type::Infer(_))) {
        if let Some(kind) = number_kind(heap, v) {
            if let Value::Boxed(_) = v {
                heap.push_root(v);
            }
            return Ok(Typed { node: Node::Number { value: v }, ty: cx.number_hole(kind), loc });
        }
    }
    if let Some(lit) = literal(heap, v, solved.as_ref())? {
        if let Some(value) = lit.value {
            heap.push_root(value);
        }
        return Ok(Typed { node: Node::Literal { tag: lit.tag, value: lit.value }, ty: lit.ty, loc });
    }
    match v {
        // A symbol macro is its form, written here.
        Value::Symbol(id) if locals.symbol_macro(id).is_some() => {
            let form = locals.symbol_macro(id).expect("checked above");
            check_expr(cx, heap, locals, form, expected, loc)
        }
        Value::Symbol(id) => check_symbol(cx, heap, locals, id, expected, loc),
        Value::Path(id) => path_call::check_path_value(cx, heap, id, expected, loc),
        Value::Cons(_) => check_list(cx, heap, locals, v, expected, loc),
        // Where an `Option` is expected, the empty list is its `none`.
        Value::Empty => match solved {
            Some(Type::Named(_, args)) => {
                let mut none = Typed::none(args[0].clone());
                none.loc = loc;
                Ok(none)
            }
            _ => unreachable!("`crate::typeck::literal` made every other empty list a unit literal"),
        },
        Value::Boxed(id) => collection_literal::check_boxed(cx, heap, locals, id, expected, loc),
        other => unimplemented!("typeck: checking {:?} (only literals, names and lists are checked so far)", other),
    }
}

/// `(symbol-macrolet ((name form)...) body...)`: the body, in which each
/// `name` stands for its `form`, checked where the name is written. A
/// binding inside the body hides it as it hides a variable.
/// `(unsafe body...)`: `body` in order, where a C function may be called
/// and a C word be a value. Its value is the last form's.
/// The outermost `unsafe` of a function owns the allocations in it
/// (`super::c_memory`).
fn check_unsafe(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let owner = !cx.c_arena_open();
    if owner {
        cx.c_arena.set(Some(false));
    }
    let checked = cx.within_unsafe(|| check_body(cx, heap, locals, args, expected));
    let allocates = owner && cx.c_arena.get() == Some(true);
    if owner {
        cx.c_arena.set(None);
    }
    let (body, ty) = checked?;
    let body = Typed { node: Node::Let { binds: Vec::new(), body }, ty: ty.clone(), loc: None };
    let mut owned = super::c_memory::owned(cx, body, &ty, owner, allocates)?;
    owned.loc = loc;
    Ok(owned)
}

/// Whether `v` is an `(unsafe ...)` form.
fn is_unsafe_form(heap: &Heap, v: Value) -> bool {
    matches!(v, Value::Cons(_)) && matches!(heap.car(v), Ok(Value::Symbol(id)) if id.is(wk::UNSAFE))
}

fn check_symbol_macrolet(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let Some(((bindings, _), body)) = args.split_first() else {
        return Err(Error::TypeError("symbol-macrolet: (symbol-macrolet ((name form)...) body...)".into()));
    };
    let bindings = heap.list_to_vec(*bindings).map_err(|_| Error::TypeError("symbol-macrolet: the binding list must be a proper list".into()))?;
    let mut macros = Vec::with_capacity(bindings.len());
    for b in bindings {
        let parts = heap.list_to_vec(b).map_err(|_| Error::TypeError("symbol-macrolet: each binding is (name form)".into()))?;
        let [name, form] = parts[..] else {
            return Err(Error::TypeError("symbol-macrolet: each binding is (name form)".into()));
        };
        let Value::Symbol(name) = name else {
            return Err(Error::TypeError("symbol-macrolet: the binding name must be a symbol".into()));
        };
        crate::check::bound_name(name, "symbol-macrolet")?;
        macros.push((name, form));
    }
    let inner = locals.bind_macros(macros)?;
    let (body, ty) = check_body(cx, heap, &inner, body, expected)?;
    Ok(Typed { node: Node::Let { binds: Vec::new(), body }, ty, loc })
}

/// The kind of type the number literal `v` may have, when it is one whose
/// type depends on where it is: an integer, or a float. A ratio is a
/// `ratio` wherever it is.
fn number_kind(heap: &Heap, v: Value) -> Option<Kind> {
    match v {
        Value::Int(_) => Some(Kind::Integer),
        Value::Boxed(id) if heap.is_bignum(id) => Some(Kind::Integer),
        Value::Boxed(id) if heap.is_f64(id) || heap.is_f32(id) => Some(Kind::Float),
        _ => None,
    }
}

/// A bare name's value: a local; where a function is expected whose first
/// parameter's type has the name as a method, that method, as a call's first
/// argument picks it; or what the module-level name holds. A variant or a
/// static method a `use` brought in is called, never a value: `(none)`, not
/// `none`.
fn check_symbol(cx: &Cx, heap: &mut Heap, locals: &Locals, id: SymRef, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    if let Some(ty) = locals.lookup(id)? {
        return Ok(Typed { node: Node::Local { name: id }, ty: ty.clone(), loc });
    }
    if let Some(expected) = expected {
        if let Type::Fn(params, _, _) = cx.resolve(expected) {
            if let Some(receiver) = params.first() {
                let member = member_symbol(heap.symbol_spelling(id));
                if let Some((owner, def)) = cx.receiver_method(receiver, member).filter(|(_, d)| d.instance) {
                    return lambda::method_value(cx, owner, member, def, expected, loc);
                }
                if let Type::Param(p) = cx.resolve(receiver) {
                    if bound_method(cx.reg, cx.scope, p, member).is_some() {
                        return Err(Error::TypeError(format!(
                            "`{}` is here a trait method of the receiver, which cannot be taken as a value",
                            heap.symbol_spelling(id)
                        )));
                    }
                }
            }
        }
    }
    let value = value_named(cx.reg, cx.ns, id);
    if let Some((_, ValueDef::Macro { .. })) = value {
        return Err(Error::TypeError(format!("`{}` is a macro, which has no value", heap.symbol_spelling(id))));
    }
    if ctor_named(cx.reg, cx.ns, id).is_some() || static_use_named(cx.reg, cx.ns, id).is_some() {
        return Err(Error::TypeError(format!(
            "`{}` is a constructor or a static method, which is called rather than taken as a value",
            heap.symbol_spelling(id)
        )));
    }
    match value {
        Some((path, ValueDef::Fn(def))) => lambda::fn_value(cx, path, def, expected, loc),
        Some((path, ValueDef::Var(var))) => Ok(Typed { node: Node::Global { path }, ty: var.ty.clone(), loc }),
        Some((_, ValueDef::Macro { .. })) => unreachable!("a macro was dispatched above"),
        None => {
            // A method of other types than the expected function's receiver.
            if let Some(Type::Fn(params, _, _)) = expected.map(|e| cx.resolve(e)) {
                if let Some(e) = params.first().and_then(|receiver| no_method(cx, receiver, member_symbol(heap.symbol_spelling(id)))) {
                    return Err(e);
                }
            }
            Err(Error::TypeError(format!("unbound variable: {}", heap.symbol_spelling(id))))
        }
    }
}

/// A list form: a special form, or a call.
///
/// A call `(name arg...)` is resolved in this order
/// (`docs/dev/name-resolution.md` 3.1): a local; a macro; a variant or a
/// static method a `use` brought in; the first argument's type's method; the
/// function the module-level name holds.
fn check_list(cx: &Cx, heap: &mut Heap, locals: &Locals, v: Value, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let elems = heap.list_to_vec_locs(v)?;
    let args = &elems[1..];
    let head = match elems[0].0 {
        Value::Symbol(head) => head,
        Value::Path(id) => return path_call::check_path_call(cx, heap, locals, id, args, expected, loc),
        // `(%type-member Type member)`: a member of a type, as a form the
        // checker writes (`forms::member_form`) or `Type<args>::member`.
        Value::Cons(_) if matches!(heap.car(elems[0].0), Ok(Value::Symbol(id)) if id.is(wk::TYPE_MEMBER)) => {
            let parts = heap.list_to_vec(elems[0].0)?;
            let [_, ty @ (Value::Symbol(_) | Value::Path(_)), Value::Symbol(member)] = parts.as_slice() else {
                unimplemented!("typeck: calling a member of one instantiation (`Type<args>::member`)")
            };
            let owner = super::type_syntax::type_path(heap, &cx.types(), *ty)?;
            let spelling = format!("{}::{}", owner, member.name());
            return path_call::check_member_call(cx, heap, locals, owner, *member, &spelling, args, expected, loc);
        }
        // Any other head is an expression whose value is applied.
        other => {
            let callee = check_expr(cx, heap, locals, other, None, elems[0].1.clone())?;
            return lambda::check_apply(cx, heap, locals, callee, None, args, expected, loc);
        }
    };
    match head.well_known() {
        wk::LET => return binding::check_let(cx, heap, locals, args, expected, loc),
        wk::LET_STAR => return binding::check_let_star(cx, heap, locals, args, expected, loc),
        wk::PROGN => return binding::check_progn(cx, heap, locals, args, expected, loc),
        wk::IF => return control::check_if(cx, heap, locals, args, expected, loc),
        wk::AS => return cast::check_as(cx, heap, locals, args, false, loc),
        wk::TRY_AS => return cast::check_as(cx, heap, locals, args, true, loc),
        wk::COMPILE => return tools::check_compile(cx, heap, args, loc),
        wk::DOCUMENTATION => return super::documentation::check_documentation(cx, heap, locals, args, loc),
        wk::TRACE | wk::UNTRACE => return tools::check_trace(cx, heap, head, args, loc),
        wk::DISASSEMBLE => return tools::check_disassemble(cx, heap, args, loc),
        wk::ED => return tools::check_ed(cx, heap, locals, args, loc),
        wk::LABELS => return lambda::check_labels(cx, heap, locals, args, expected, loc),
        wk::LAMBDA => return lambda::check_lambda(cx, heap, locals, args, loc),
        wk::SELECT => return select::check_select(cx, heap, locals, args, expected, loc),
        wk::SYMBOL_MACROLET => return check_symbol_macrolet(cx, heap, locals, args, expected, loc),
        wk::MACROLET => return super::macrolet::check_macrolet(cx, heap, locals, args, expected, loc),
        wk::DYN => {
            return Err(Error::TypeError("`:dyn` may only appear in a type position (a parameter/field/return type, or a type argument)".into()));
        }
        wk::TASK | wk::THREAD => return spawn::check_spawn(cx, heap, locals, head.well_known(), args, loc),
        wk::APPLY => return lambda::check_apply_form(cx, heap, locals, args, loc),
        wk::PANIC => return control::check_panic(cx, heap, locals, args, loc),
        wk::UNWIND_PROTECT => return control::check_unwind_protect(cx, heap, locals, args, expected, loc),
        wk::MATCH => return matching::check_match(cx, heap, locals, args, expected, loc),
        wk::FORMAT => return format::check_format(cx, heap, locals, args, loc),
        wk::PRINT | wk::PRINTLN => return format::check_print(cx, heap, locals, head.well_known(), args, loc),
        wk::EPRINT | wk::EPRINTLN => return format::check_eprint(cx, heap, locals, head.well_known(), args, loc),
        wk::PPRINT | wk::PPRINT_FILL | wk::PPRINT_LINEAR | wk::PPRINT_TABULAR => return super::pprint::check_pprint(cx, heap, locals, head, args, loc),
        wk::PPRINT_LOGICAL_BLOCK => return super::pprint::check_logical_block(cx, heap, locals, args, loc),
        wk::STEP => {
            let [(form, form_loc)] = args else {
                return Err(Error::TypeError("step: (step form) — expected exactly 1 form".into()));
            };
            let inner = check_expr(cx, heap, locals, *form, expected, form_loc.clone())?;
            let ty = inner.ty.clone();
            return Ok(Typed { node: Node::Step { inner: Box::new(inner) }, ty, loc });
        }
        wk::SOURCE_FILE => {
            if !args.is_empty() {
                return Err(Error::TypeError("source-file: (source-file) takes no arguments".into()));
            }
            // A form a macro built may have no position, and so no file.
            let file = loc.as_ref().map_or_else(|| "<unknown>".to_string(), |l| l.file.to_string());
            let file = heap.alloc_string(file);
            heap.push_root(file);
            return check_expr(cx, heap, locals, file, Some(&Type::Str), loc);
        }
        wk::LOOP => return looping::check_loop(cx, heap, locals, args, expected, loc),
        wk::BLOCK => return block::check_block(cx, heap, locals, args, expected, loc),
        wk::UNSAFE => return check_unsafe(cx, heap, locals, args, expected, loc),
        wk::DEF_C_STRUCT => {
            return Err(Error::TypeError("def-c-struct is written at top level, inside `(unsafe ...)`: (unsafe (def-c-struct name (field type)...))".into()))
        }
        wk::C_ALLOC => return super::c_memory::check_c_alloc(cx, heap, locals, args, loc),
        wk::C_REF => return super::c_memory::check_c_ref(cx, heap, locals, args, loc),
        wk::C_DEREF => return super::c_memory::check_c_deref(cx, heap, locals, args, loc),
        wk::CATCH => return catch::check_catch(cx, heap, locals, args, expected, loc),
        wk::THROW => return catch::check_throw(cx, heap, locals, args, loc),
        wk::RETURN_FROM => return block::check_return_from(cx, heap, locals, args, loc),
        wk::BREAK => return looping::check_break(cx, args, loc),
        wk::RETURN => return looping::check_return(cx, heap, locals, args, loc),
        wk::QUOTE => return quote::check_quote(heap, args, loc),
        wk::QUASIQUOTE => return quote::check_quasiquote(cx, heap, locals, args, loc),
        wk::LIST => return list_form::check_list(cx, heap, locals, args, loc),
        wk::SETF => return setf::check_setf(cx, heap, locals, args, loc),
        wk::INCF | wk::DECF => return modify::check_incf_decf(cx, heap, locals, head.well_known(), args, loc),
        wk::ROTATEF | wk::SHIFTF => return modify::check_rotatef_shiftf(cx, heap, locals, head.well_known(), args, loc),
        wk::THE => return the::check_the(cx, heap, locals, args),
        wk::MAKE_INSTANCE => return instance::check_make_instance(cx, heap, locals, args, expected, loc),
        wk::VALUE_MEMBER => {
            let [(Value::Symbol(recv), _), (Value::Symbol(member), _)] = args else {
                return Err(Error::TypeError("a member of a value is written `value::member`".into()));
            };
            return instance::check_field(cx, heap, locals, *recv, *member, loc);
        }
        _ => {}
    }
    if SPECIAL_FORM_HEADS.contains(&head.well_known()) {
        unimplemented!("typeck: the special form `{}`", heap.symbol_spelling(head))
    }
    crate::check::verify::expression_head(head, TOPLEVEL_FORM_HEADS)?;
    let spelling = heap.symbol_spelling(head);
    if let Some((path, shape)) = locals.local_macro(head) {
        let expanded = macros::expand(cx.expander, heap, &path, shape, spelling, args)?;
        return check_expr(cx, heap, locals, expanded, expected, loc);
    }
    if let Some(ty) = locals.lookup(head)? {
        let callee = Typed { node: Node::Local { name: head }, ty: ty.clone(), loc: elems[0].1.clone() };
        return lambda::check_apply(cx, heap, locals, callee, None, args, expected, loc);
    }
    let value = value_named(cx.reg, cx.ns, head);
    let head_is_fn = matches!(value, Some((_, ValueDef::Fn(_))));
    if let Some(checked) = sugar::check_sugar(cx, heap, locals, head, head_is_fn, args, expected, loc.clone()) {
        return checked;
    }
    if let Some((path, ValueDef::Macro { shape, .. })) = value {
        let expanded = macros::expand(cx.expander, heap, &path, *shape, spelling, args)?;
        return check_expr(cx, heap, locals, expanded, expected, loc);
    }
    if let Some((owner, variant)) = ctor_named(cx.reg, cx.ns, head) {
        return variant::check_variant(cx, heap, locals, owner, variant, args, expected, loc);
    }
    if let Some((owner, name)) = static_use_named(cx.reg, cx.ns, head) {
        let def = cx.reg.method(&owner, name).expect("a `use` brings in a defined static method");
        return method::check_method_call(cx, heap, locals, owner, name, def, None, args, expected, loc);
    }
    let Some(((first, first_loc), rest)) = args.split_first() else {
        return call_function(cx, heap, locals, value, spelling, None, args, expected, loc);
    };
    // When the call's name is a function, the first argument is its first
    // argument, checked with the parameter's type expected, if no type or
    // trait has a method of the name, or if it is a bare name that holds no
    // value here and so has no type to pick a method by: a method taken as a
    // value, the parameter saying which (`(apply2 + 1.5 2.5)`).
    if value.is_some() && (!cx.reg.is_method_name(member_symbol(spelling)) || names_no_value(cx, locals, *first)) {
        return call_function(cx, heap, locals, value, spelling, None, args, expected, loc);
    }
    // The first argument is checked once, with nothing expected of it. Its
    // type says whose method the call is; when it is no method's, the same
    // checked argument is the function's first. A number literal no type
    // decided yet is read as its default for this (`(+ 1 x)` is `int`'s
    // `+`), and is decided only when it is a receiver.
    let written = (*first, first_loc.clone());
    let first = check_expr(cx, heap, locals, *first, None, first_loc.clone())?;
    let member = member_symbol(spelling);
    if let Type::Dyn(trait_path, pins) = cx.resolve(&first.ty) {
        if dyn_trait::has_method(cx.reg, &trait_path, member) {
            return dyn_trait::check_dyn_call(cx, heap, locals, first, &trait_path, &pins, member, rest, loc);
        }
        // A trait object answers to `Sexpr`'s methods as the value inside it.
        let sexpr = crate::types::Path::builtin(wk::SEXPR);
        if let Some(def) = cx.reg.method(&sexpr, member).filter(|m| m.instance) {
            let loc_first = first.loc.clone();
            let held = Typed { node: Node::DynValue { value: Box::new(first) }, ty: crate::check::analyze::option_of_sexpr(), loc: loc_first };
            return method::check_method_call(cx, heap, locals, sexpr, member, def, Some(held), rest, expected, loc);
        }
        // Neither a function nor the trait's method: the name may be a
        // function of the library this module has not brought in.
        if value.is_none() {
            return Err(Error::TypeError(format!(
                "no function `{}` is visible here, and it is not a method of `{}` either — a `:dyn {}` value can only call the trait's own or inherited methods",
                member, trait_path, trait_path
            )));
        }
    }
    if let Some((owner, def)) = cx.receiver_method(&first.ty, member) {
        cx.decide(&first.ty);
        return method::check_method_call(cx, heap, locals, owner, member, def, Some(first), rest, expected, loc);
    }
    if let Type::Param(p) = cx.resolve(&first.ty) {
        if let Some(bound) = bound_method(cx.reg, cx.scope, p, member) {
            let method = bound.method;
            return method::check_trait_call(cx, heap, locals, bound, method, first, rest, expected, loc);
        }
    }
    // A name that is a method of other types is no missing function: the
    // first argument's type picked the method, and it has none by this name.
    if value.is_none() {
        if let Some(e) = no_method(cx, &first.ty, member) {
            return Err(e);
        }
    }
    call_function(cx, heap, locals, value, spelling, Some(First { checked: first, written }), rest, expected, loc)
}

/// The error for a receiver of type `recv` that has no method `member`,
/// when other types have one: the name is no missing function.
fn no_method(cx: &Cx, recv: &Type, member: SymRef) -> Option<Error> {
    let owners = cx.reg.types_with_instance_method(member);
    if owners.is_empty() {
        return None;
    }
    let recv_ty = cx.resolve(recv);
    if let Type::Param(_) = recv_ty {
        return Some(Error::TypeError(format!(
            "no method `{}` for type variable `{}`: none of the traits in its `:where` bounds provides `{}`",
            member, recv_ty, member
        )));
    }
    const SHOWN: usize = 8;
    let mut list = owners.iter().take(SHOWN).map(|t| format!("`{}`", t)).collect::<Vec<_>>().join(", ");
    if owners.len() > SHOWN {
        list.push_str(&format!(" and {} more", owners.len() - SHOWN));
    }
    Some(Error::TypeError(format!(
        "no method `{}` for type `{}` (the type of the first argument, which selects the method); `{}` is a method of {}",
        member, recv_ty, member, list
    )))
}

/// Whether `v` is a bare name that names no value here without a type
/// expected of it: no local, symbol macro or module-level definition.
fn names_no_value(cx: &Cx, locals: &Locals, v: Value) -> bool {
    let Value::Symbol(id) = v else { return false };
    matches!(locals.lookup(id), Ok(None))
        && locals.symbol_macro(id).is_none()
        && value_named(cx.reg, cx.ns, id).is_none()
        && ctor_named(cx.reg, cx.ns, id).is_none()
        && static_use_named(cx.reg, cx.ns, id).is_none()
}

/// A call to what the module-level name `spelling` holds, `value`: its
/// arguments `first`, already checked, and `rest`.
#[allow(clippy::too_many_arguments)]
fn call_function(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    value: Option<(crate::types::Path, &ValueDef)>,
    spelling: &str,
    first: Option<First>,
    rest: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    match value {
        Some((path, ValueDef::Fn(def))) => call::check_call(cx, heap, locals, path, def, first, rest, expected, loc),
        Some((path, ValueDef::Var(var))) => {
            let callee = Typed { node: Node::Global { path }, ty: var.ty.clone(), loc: None };
            lambda::check_apply(cx, heap, locals, callee, first, rest, expected, loc)
        }
        Some((_, ValueDef::Macro { .. })) => unreachable!("a macro was dispatched before the call"),
        None => Err(Error::NoSuchFunction(spelling.to_string())),
    }
}

/// Whether a value of type `found` may be where `expected` is expected,
/// solving holes to make it so, with no widening: for a type that is no
/// value's (a receiver's, a body's). `!` is every type.
pub(crate) fn agree(cx: &Cx, found: &Type, expected: Option<&Type>) -> Result<(), Error> {
    let Some(e) = expected else { return Ok(()) };
    let unified = cx.holes.borrow_mut().unify(e, found);
    let Err(m) = unified else { return Ok(()) };
    Err(Error::TypeError(format!("type mismatch: expected `{}`, found `{}`", m.left, m.right)))
}

/// [`conform`] for the expression `t`, already checked, where `expected`
/// is expected: an error is where `t` was written.
///
/// What a checker decides only from what is expected was decided without
/// it: the empty list, checked as the unit value, is `Option::none` where an
/// `Option` is expected.
pub(crate) fn conform_at(cx: &Cx, t: Typed, expected: &Type) -> Result<Typed, Error> {
    if matches!(t.node, Node::Literal { tag: wk::UNIT, .. }) {
        if let Type::Named(p, args) = cx.resolve(expected) {
            if crate::types::path_is_builtin(&p, wk::OPTION) {
                let mut none = Typed::none(args[0].clone());
                none.loc = t.loc.clone();
                return Ok(none);
            }
        }
    }
    let loc = t.loc.clone();
    conform(cx, t, Some(expected)).map_err(|e| match &loc {
        Some(l) => e.at(l.clone()),
        None => e,
    })
}

/// `forms` checked in order, the last where `expected` is expected: the
/// body of a `let`, a `progn` or a function. Its type is the last form's, or
/// `()` when there is none.
pub(crate) fn check_body(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    forms: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
) -> Result<(Vec<Typed>, Type), Error> {
    let mut out = Vec::with_capacity(forms.len());
    for (i, (v, loc)) in forms.iter().enumerate() {
        let last = i + 1 == forms.len();
        out.push(check_expr(cx, heap, locals, *v, if last { expected } else { None }, loc.clone())?);
    }
    let ty = match out.last() {
        Some(t) => t.ty.clone(),
        None => {
            agree(cx, &Type::Unit, expected)?;
            Type::Unit
        }
    };
    Ok((out, ty))
}
