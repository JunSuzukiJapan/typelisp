//! Checking an expression: which kind of form it is, and the module that
//! checks that kind.

use std::cell::{Cell, RefCell};

use crate::types::Type;
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;

use super::literal::literal;
use super::locals::Locals;
use super::registry::{Registry, ValueDef};
use super::resolve::{ctor_named, receiver_method, static_use_named, value_named};
use super::tree::{Node, Typed};
use super::scope::TypeScope;
use super::type_syntax::TypeCx;
use super::unify::{Holes, Mark};
use super::{binding, call, control, instance, matching, method, path_call, variant};
use crate::check::analyze::{call_sugar, is_sexpr_expectation, TOPLEVEL_FORM_HEADS};
use crate::check::SPECIAL_FORM_HEADS;
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

/// What every expression of one top-level form is checked against: the
/// definitions, the module the form is in, the type parameters the
/// definition declares, and the form's inference holes.
pub(crate) struct Cx<'a> {
    pub(crate) reg: &'a Registry,
    pub(crate) ns: NsId,
    pub(crate) scope: &'a TypeScope<'a>,
    /// Inside an `impl`, the type `Self` stands for.
    pub(crate) self_type: Option<&'a Type>,
    pub(crate) holes: RefCell<Holes>,
    /// The type arguments the form's checking must decide: each one's type,
    /// and what to say, and where, when nothing does.
    undecided: RefCell<Vec<(Type, String, Option<Loc>)>>,
    /// The temporaries the form has made, to name the next one.
    temps: Cell<u32>,
}

impl<'a> Cx<'a> {
    pub(crate) fn new(reg: &'a Registry, ns: NsId, scope: &'a TypeScope<'a>, self_type: Option<&'a Type>) -> Cx<'a> {
        Cx { reg, ns, scope, self_type, holes: RefCell::new(Holes::new()), undecided: RefCell::new(Vec::new()), temps: Cell::new(0) }
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

    /// What the form's inference knows now, to go back to with
    /// [`Self::restore`] after checking something only to look at it.
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot { holes: self.holes.borrow().mark(), undecided: self.undecided.borrow().len() }
    }

    /// Forget what the inference learned since `snap` was taken.
    pub(crate) fn restore(&self, snap: Snapshot) {
        self.holes.borrow_mut().rollback(snap.holes);
        self.undecided.borrow_mut().truncate(snap.undecided);
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

    /// The checked trees of the form, every type in them settled: each hole
    /// replaced by its solution. An error if a type argument nothing decided
    /// was required ([`Self::must_decide`]).
    pub(crate) fn settle(&self, trees: &mut [Typed]) -> Result<(), Error> {
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
        }
        Ok(())
    }
}

/// What [`Cx::snapshot`] keeps.
pub(crate) struct Snapshot {
    holes: Mark,
    undecided: usize,
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

/// A bare name's value: a local, or what the module-level name holds. A
/// variant or a static method a `use` brought in is called, never a value:
/// `(none)`, not `none`.
fn check_symbol(cx: &Cx, heap: &mut Heap, locals: &Locals, id: SymRef, loc: Option<Loc>) -> Result<Typed, Error> {
    if let Some(ty) = locals.lookup(id) {
        return Ok(Typed { node: Node::Local { name: id }, ty: ty.clone(), loc });
    }
    let value = value_named(cx.reg, cx.ns, id);
    if let Some((_, ValueDef::Macro)) = value {
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
        Some((_, ValueDef::Var)) => unimplemented!("typeck: the global variable `{}`", heap.symbol_spelling(id)),
        Some((_, ValueDef::Macro)) => unreachable!("a macro was dispatched above"),
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
        _ => unimplemented!("typeck: a list whose head is neither a name nor a path"),
    };
    match head.well_known() {
        wk::LET => return binding::check_let(cx, heap, locals, args, expected, loc),
        wk::PROGN => return binding::check_progn(cx, heap, locals, args, expected, loc),
        wk::IF => return control::check_if(cx, heap, locals, args, expected, loc),
        wk::MATCH => return matching::check_match(cx, heap, locals, args, expected, loc),
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
    if let Some((_, ValueDef::Macro)) = value {
        unimplemented!("typeck: expanding the macro `{}`", spelling)
    }
    if let Some((owner, variant)) = ctor_named(cx.reg, cx.ns, head) {
        return variant::check_variant(cx, heap, locals, owner, variant, args, expected, loc);
    }
    if let Some((owner, name)) = static_use_named(cx.reg, cx.ns, head) {
        let def = cx.reg.method(&owner, name).expect("a `use` brings in a defined static method");
        return method::check_method_call(cx, heap, locals, owner, name, def, None, args, expected, loc);
    }
    let member = member_symbol(spelling);
    if let Some((first, first_loc)) = args.first().filter(|_| cx.reg.has_method_named(member)) {
        // The receiver is checked to find its type's method. When it has
        // none, what that check solved is forgotten: the argument is checked
        // again where the function's parameter type is expected.
        let saved = cx.snapshot();
        match check_expr(cx, heap, locals, *first, None, first_loc.clone()) {
            Ok(receiver) => {
                let ty = cx.resolve(&receiver.ty);
                if let Some((owner, def)) = receiver_method(cx.reg, cx.ns, &ty, member) {
                    return method::check_method_call(cx, heap, locals, owner, member, def, Some(receiver), &args[1..], expected, loc);
                }
            }
            // With no function of the name either, the receiver's own error
            // is what is wrong with the call.
            Err(e) if value.is_none() => return Err(e),
            Err(_) => {}
        }
        cx.restore(saved);
    }
    match value {
        Some((path, ValueDef::Fn(def))) => call::check_call(cx, heap, locals, path, def, args, expected, loc),
        Some((_, ValueDef::Var)) => unimplemented!("typeck: calling the function the global `{}` holds", spelling),
        Some((_, ValueDef::Macro)) => unreachable!("a macro was dispatched above"),
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
