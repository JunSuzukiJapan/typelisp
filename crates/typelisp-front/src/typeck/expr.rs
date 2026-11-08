//! Checking an expression: which kind of form it is, and the module that
//! checks that kind.

use std::cell::{Cell, RefCell};

use crate::types::Type;
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;

use super::format::{check_directives, Directives};
use super::literal::literal;
use super::sexpr_value::{encodable, no_encoding};
use super::locals::Locals;
use super::registry::{Registry, ValueDef};
use super::resolve::{ctor_named, receiver_method, static_use_named, value_named};
use super::tree::{Node, Typed};
use super::scope::TypeScope;
use super::type_syntax::TypeCx;
use super::unify::{Holes, Kind};
use super::{binding, call, control, format, instance, macros, matching, method, path_call, the, variant};
use crate::check::analyze::{call_sugar, is_sexpr_expectation, TOPLEVEL_FORM_HEADS};
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
    /// The control strings to check against their arguments' settled types.
    directives: RefCell<Vec<Directives>>,
    /// The temporaries the form has made, to name the next one.
    temps: Cell<u32>,
}

impl<'a> Cx<'a> {
    pub(crate) fn new(reg: &'a Registry, expander: &'a dyn MacroExpander, ns: NsId, scope: &'a TypeScope<'a>, self_type: Option<&'a Type>) -> Cx<'a> {
        Cx { reg, expander, ns, scope, self_type, holes: RefCell::new(Holes::new()), undecided: RefCell::new(Vec::new()), numbers: RefCell::new(Vec::new()), directives: RefCell::new(Vec::new()), temps: Cell::new(0) }
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

    /// `ty` with every hole solved so far filled in.
    pub(crate) fn resolve(&self, ty: &Type) -> Type {
        self.holes.borrow().resolve(ty)
    }

    /// A hole of the kind `kind` for a number literal.
    fn number_hole(&self, kind: Kind) -> Type {
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
        Ok(())
    }
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
    let typed = typelisp_mem::grow_stack(|| check_form(cx, heap, locals, v, expected, loc.clone())).and_then(|t| {
        agree(cx, &t.ty, expected)?;
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
        Value::Symbol(id) => check_symbol(cx, heap, locals, id, loc),
        Value::Cons(_) => check_list(cx, heap, locals, v, expected, loc),
        other => unimplemented!("typeck: checking {:?} (only literals, names and lists are checked so far)", other),
    }
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

/// A bare name's value: a local, or what the module-level name holds. A
/// variant or a static method a `use` brought in is called, never a value:
/// `(none)`, not `none`.
fn check_symbol(cx: &Cx, heap: &mut Heap, locals: &Locals, id: SymRef, loc: Option<Loc>) -> Result<Typed, Error> {
    if let Some(ty) = locals.lookup(id) {
        return Ok(Typed { node: Node::Local { name: id }, ty: ty.clone(), loc });
    }
    let value = value_named(cx.reg, cx.ns, id);
    if let Some((_, ValueDef::Macro { .. })) = value {
        unimplemented!("typeck: the macro `{}` as a value", heap.symbol_spelling(id))
    }
    if ctor_named(cx.reg, cx.ns, id).is_some() || static_use_named(cx.reg, cx.ns, id).is_some() {
        return Err(Error::TypeError(format!(
            "`{}` is a constructor or a static method, which is called rather than taken as a value",
            heap.symbol_spelling(id)
        )));
    }
    match value {
        Some((_, ValueDef::Fn(_))) => unimplemented!("typeck: the function `{}` as a value", heap.symbol_spelling(id)),
        Some((path, ValueDef::Var(var))) => Ok(Typed { node: Node::Global { path }, ty: var.ty.clone(), loc }),
        Some((_, ValueDef::Macro { .. })) => unreachable!("a macro was dispatched above"),
        None => Err(Error::TypeError(format!("unbound variable: {}", heap.symbol_spelling(id)))),
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
        // Any other head is an expression whose value is applied.
        other => {
            check_expr(cx, heap, locals, other, None, elems[0].1.clone())?;
            unimplemented!("typeck: applying a function value")
        }
    };
    match head.well_known() {
        wk::LET => return binding::check_let(cx, heap, locals, args, expected, loc),
        wk::PROGN => return binding::check_progn(cx, heap, locals, args, expected, loc),
        wk::IF => return control::check_if(cx, heap, locals, args, expected, loc),
        wk::MATCH => return matching::check_match(cx, heap, locals, args, expected, loc),
        wk::FORMAT => return format::check_format(cx, heap, locals, args, loc),
        wk::PRINT | wk::PRINTLN => return format::check_print(cx, heap, locals, head.well_known(), args, loc),
        wk::THE => return the::check_the(cx, heap, locals, args),
        wk::MAKE_INSTANCE => return instance::check_make_instance(cx, heap, locals, args, expected, loc),
        wk::VALUE_MEMBER => {
            let [(Value::Symbol(recv), _), (Value::Symbol(member), _)] = args else {
                return Err(Error::TypeError("a member of a value is written `value::member`".into()));
            };
            return instance::check_field(cx, locals, *recv, *member, loc);
        }
        _ => {}
    }
    if SPECIAL_FORM_HEADS.contains(&head.well_known()) {
        unimplemented!("typeck: the special form `{}`", heap.symbol_spelling(head))
    }
    if TOPLEVEL_FORM_HEADS.contains(&head.well_known()) {
        unimplemented!("typeck: the form `{}`", heap.symbol_spelling(head))
    }
    let spelling = heap.symbol_spelling(head);
    if locals.lookup(head).is_some() {
        unimplemented!("typeck: calling the local `{}`", spelling)
    }
    let value = value_named(cx.reg, cx.ns, head);
    if call_sugar(head.name(), args.len(), || matches!(value, Some((_, ValueDef::Fn(_))))).is_some() {
        unimplemented!("typeck: the call sugar `({} ...)` with {} argument(s)", spelling, args.len())
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
    // The first argument is checked once, with nothing expected of it. Its
    // type says whose method the call is; when it is no method's, the same
    // checked argument is the function's first. A number literal no type
    // decided yet is read as its default for this (`(+ 1 x)` is `int`'s
    // `+`), and is decided only when it is a receiver.
    let first = check_expr(cx, heap, locals, *first, None, first_loc.clone())?;
    let member = member_symbol(spelling);
    if let Some((owner, def)) = receiver_method(cx.reg, cx.ns, &cx.with_default(&first.ty), member) {
        cx.decide(&first.ty);
        return method::check_method_call(cx, heap, locals, owner, member, def, Some(first), rest, expected, loc);
    }
    call_function(cx, heap, locals, value, spelling, Some(first), rest, expected, loc)
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
    first: Option<Typed>,
    rest: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    match value {
        Some((path, ValueDef::Fn(def))) => call::check_call(cx, heap, locals, path, def, first, rest, expected, loc),
        Some((_, ValueDef::Var(_))) => unimplemented!("typeck: calling the function the global `{}` holds", spelling),
        Some((_, ValueDef::Macro { .. })) => unreachable!("a macro was dispatched before the call"),
        None => Err(Error::NoSuchFunction(spelling.to_string())),
    }
}

/// Whether a value of type `found` may be where `expected` is expected,
/// solving holes to make it so. `!` is every type.
pub(crate) fn agree(cx: &Cx, found: &Type, expected: Option<&Type>) -> Result<(), Error> {
    let Some(e) = expected else { return Ok(()) };
    let unified = cx.holes.borrow_mut().unify(e, found);
    let Err(m) = unified else { return Ok(()) };
    // Where a trait object or a `Sexpr` is expected, the current checker
    // widens a value of another type into one.
    let wanted = cx.resolve(e);
    if matches!(wanted, Type::Dyn(..)) || is_sexpr_expectation(&wanted) {
        unimplemented!("typeck: a `{}` widened to the `{}` expected", cx.resolve(found), wanted)
    }
    Err(Error::TypeError(format!("type mismatch: expected `{}`, found `{}`", m.left, m.right)))
}

/// [`agree`] for the expression `t`, already checked, where `expected` is
/// expected: an error is where `t` was written.
///
/// What a checker decides only from what is expected — the empty list as
/// `Option::none` — was decided without it; that is not done yet.
pub(crate) fn agree_at(cx: &Cx, t: &Typed, expected: &Type) -> Result<(), Error> {
    if matches!(t.node, Node::Literal { tag: wk::UNIT, .. }) && matches!(cx.resolve(expected), Type::Named(p, _) if crate::types::path_is_builtin(&p, wk::OPTION)) {
        unimplemented!("typeck: `()` as `Option::none`, checked before its type was known")
    }
    agree(cx, &t.ty, Some(expected)).map_err(|e| match &t.loc {
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
