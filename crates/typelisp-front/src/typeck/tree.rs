//! The typed tree: what checking a form produces, before it is lowered to
//! core IR ([`super::lower`]).
//!
//! Lowering waits for the whole form to be checked because a type may still
//! hold an inference hole while the form is being checked, and the core IR
//! writes each value's representation, which only a solved type has.


use crate::owned_form::OwnedForm;
use crate::types::{Path, Type};
use crate::{Loc, Value};
use typelisp_mem::SymRef;

/// A checked expression: what it does, its type, and where it was written.
///
/// Written flat ([`flat`]): reading a tree back does not recurse once per
/// level of it, which a WebAssembly module's stack could not hold.
#[derive(Debug)]
pub(crate) struct Typed {
    pub(crate) node: Node,
    pub(crate) ty: Type,
    pub(crate) loc: Option<Loc>,
}

/// How a [`Typed`] tree is written and read: as the list of its expressions,
/// breadth first in [`Typed::children`] order, each written without the
/// expressions inside it (each of those is a unit). Reading builds the list,
/// then puts each expression's children into it from the last one back, so
/// neither direction recurses once per level of the tree.
///
/// Serde's own derive recurses through several functions per level, and
/// stacker cannot grow a WebAssembly module's stack: there, a tree of an
/// ordinary depth runs out of the stack a browser gives a Worker.
mod flat {
    use super::{Node, Typed};
    use crate::types::Type;
    use crate::Loc;
    use serde::de::Error as _;
    use serde::ser::{Error as _, SerializeSeq};
    use std::cell::Cell;

    thread_local! {
        /// While one expression of a list is written or read: the
        /// expressions met inside it so far, which are its children, written
        /// as units.
        static INNER: Cell<Option<usize>> = const { Cell::new(None) };
    }

    /// Sets [`INNER`] for one expression, and puts back what it was however
    /// the writing or reading ends.
    struct Inner(Option<usize>);

    impl Inner {
        fn enter() -> Inner {
            Inner(INNER.with(|c| c.replace(Some(0))))
        }

        /// The children met since [`Self::enter`].
        fn met(&self) -> usize {
            INNER.with(|c| c.get()).expect("set by enter")
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            INNER.with(|c| c.set(self.0));
        }
    }

    /// True, and the child counted, when this is an expression inside the
    /// one being written or read.
    fn inside() -> bool {
        INNER.with(|c| match c.get() {
            Some(n) => {
                c.set(Some(n + 1));
                true
            }
            None => false,
        })
    }

    #[derive(serde::Serialize)]
    struct ShellRef<'a> {
        node: &'a Node,
        ty: &'a Type,
        #[serde(serialize_with = "loc_ref")]
        loc: &'a Option<Loc>,
    }

    fn loc_ref<S: serde::Serializer>(v: &&Option<Loc>, s: S) -> Result<S::Ok, S::Error> {
        crate::owned_form::loc_serde::opt::serialize(v, s)
    }

    #[derive(serde::Deserialize)]
    struct Shell {
        node: Node,
        ty: Type,
        #[serde(with = "crate::owned_form::loc_serde::opt")]
        loc: Option<Loc>,
    }

    impl serde::Serialize for Typed {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if inside() {
                return s.serialize_unit();
            }
            let mut order: Vec<&Typed> = vec![self];
            let mut at = 0;
            while at < order.len() {
                let children = order[at].children();
                order.extend(children);
                at += 1;
            }
            let mut seq = s.serialize_seq(Some(order.len()))?;
            for t in order {
                let inner = Inner::enter();
                seq.serialize_element(&ShellRef { node: &t.node, ty: &t.ty, loc: &t.loc })?;
                let expected = t.children().len();
                if inner.met() != expected {
                    return Err(S::Error::custom(format!(
                        "a typed expression holds {} expressions but lists {} as its children",
                        inner.met(),
                        expected
                    )));
                }
            }
            seq.end()
        }
    }

    /// What stands for a child until the list is put together.
    fn placeholder() -> Typed {
        Typed { node: Node::Hole, ty: Type::Never, loc: None }
    }

    impl<'de> serde::Deserialize<'de> for Typed {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Typed, D::Error> {
            if inside() {
                <() as serde::Deserialize>::deserialize(d)?;
                return Ok(placeholder());
            }
            let shells = {
                let _inner = Inner::enter();
                <Vec<Shell> as serde::Deserialize>::deserialize(d)?
            };
            let mut list: Vec<Option<Typed>> =
                shells.into_iter().map(|s| Some(Typed { node: s.node, ty: s.ty, loc: s.loc })).collect();
            // Breadth first: each expression's children follow those of the
            // expressions before it.
            let mut first = Vec::with_capacity(list.len());
            let mut next = 1;
            for t in &mut list {
                first.push(next);
                next += t.as_mut().expect("not taken yet").children_mut().len();
            }
            if next != list.len() || list.is_empty() {
                return Err(D::Error::custom(format!("a typed tree of {} expressions names {} as children", list.len(), next - 1)));
            }
            for i in (0..list.len()).rev() {
                let mut t = list[i].take().expect("taken only here");
                for (k, slot) in t.children_mut().into_iter().enumerate() {
                    *slot = list[first[i] + k].take().expect("each child is put in one place");
                }
                list[i] = Some(t);
            }
            Ok(list[0].take().expect("the root"))
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Node {
    /// A literal: its core tag and its value (None for the unit value).
    Literal { tag: u32, #[serde(with = "super::wire::slot::opt")] value: Option<Value> },
    /// `(quote datum)`: the datum as it was read.
    Quote { #[serde(with = "super::wire::slot")] datum: Value },
    /// A number literal whose type was not known where it was written: an
    /// integer or a float type the rest of the form decides. It becomes a
    /// [`Node::Literal`] when the form is settled (`Cx::settle`), so
    /// lowering never sees one.
    Number { #[serde(with = "super::wire::slot")] value: Value },
    /// A local variable's value.
    Local { #[serde(with = "crate::owned_form::sym_serde")] name: SymRef },
    /// The global variable `path`'s value.
    Global { path: Path },
    /// A call to the function `path`, at the type arguments `targs` when it
    /// is generic.
    Call { path: Path, targs: Vec<Type>, args: Vec<Typed> },
    /// A call to the method `method` of the type `owner`, at the type
    /// arguments `targs` when it is generic. An instance method's receiver is
    /// the first argument.
    MethodCall { owner: Path, #[serde(with = "crate::owned_form::sym_serde")] method: SymRef, instance: bool, targs: Vec<Type>, args: Vec<Typed> },
    /// A call of the method `method` of the trait `trait_path`, on a
    /// receiver whose type is a type parameter the trait bounds. Once the
    /// tree is specialized, the receiver's type is one with an `impl` of the
    /// trait, whose method the call is. `own` are the method's own type
    /// arguments, when it declares type parameters of its own.
    TraitCall { trait_path: Path, #[serde(with = "crate::owned_form::sym_serde")] method: SymRef, own: Vec<Type>, args: Vec<Typed> },
    /// A new value of the type `owner<targs>`: its variant `variant`'s fields,
    /// whose declared types at these arguments are `field_tys`. A struct's is
    /// mutable.
    Construct { owner: Path, targs: Vec<Type>, variant: usize, mutable: bool, field_tys: Vec<Type>, args: Vec<Typed> },
    /// The field `index` of the struct `obj`.
    FieldGet { obj: Box<Typed>, index: usize },
    /// Set the field `index` (declared `field_ty` at the struct's arguments)
    /// of the struct `obj` to `value`.
    FieldSet { obj: Box<Typed>, index: usize, field_ty: Type, value: Box<Typed> },
    /// `(setf name value)` of the local `name`.
    Set { #[serde(with = "crate::owned_form::sym_serde")] name: SymRef, value: Box<Typed> },
    /// `(setf name value)` of the global variable `path`.
    SetGlobal { path: Path, value: Box<Typed> },
    /// `(loop body...)`: the body run again and again, until a `return` or
    /// a `break` in it leaves.
    Loop { body: Vec<Typed> },
    /// `(break)`: leave the nearest loop, its value `()`.
    Break,
    /// `(return value?)`: leave the nearest loop with `value` (`()` when
    /// none).
    Return { value: Option<Box<Typed>> },
    /// `(block name body...)`, or a definition's own block: `body` (a `let`
    /// with no bindings), left early by a `return-from` naming `label`.
    Block { label: Box<str>, body: Box<Typed> },
    /// `(return-from name value?)`: leave the block `label` with `value`
    /// (`()` when none).
    ReturnFrom { label: Box<str>, value: Option<Box<Typed>> },
    /// `(catch 'tag body)`: `body`'s value, or what a `throw` of `tag` in
    /// it gave, which is a `carried`.
    Catch { #[serde(with = "crate::owned_form::sym_serde")] tag: SymRef, carried: Type, body: Box<Typed> },
    /// `(throw 'tag value)`: leave for the nearest `catch` of `tag` around
    /// it at run time, with `value`.
    Throw { #[serde(with = "crate::owned_form::sym_serde")] tag: SymRef, value: Box<Typed> },
    /// The type `ty` as a value, for an internal builtin that reads a C
    /// object's layout.
    TypeValue { ty: Type },
    /// The `int` `value` as the raw word an internal builtin reads.
    UntagInt { value: Box<Typed> },
    /// `(lambda (params) ret body...)`: a function value.
    Lambda { #[serde(with = "super::wire::sym_pairs")] params: Vec<(SymRef, Type)>, ret: Type, body: Vec<Typed> },
    /// `(labels (def...) body...)`: local functions, which see each other;
    /// `lifted` are those passed to C.
    Labels { defs: Vec<super::lambda::Label>, body: Vec<Typed>, lifted: Vec<super::lambda::LiftedLabel> },
    /// A function handed to C as a callback of the type `fn_ty`: the
    /// address of an entry that calls the top-level function `target`.
    /// `lifted` is the `lambda` `target` is made of, when it is one.
    Callback { target: Path, fn_ty: Type, lifted: Option<Box<Typed>> },
    /// The function `path` as a value, `targs` its type arguments (None for
    /// a builtin, whose one entry serves them all); `params` are the types
    /// of the parameters it really takes, a `&rest` one as a list.
    FnRef { path: Path, targs: Vec<Type>, params: Vec<Type> },
    /// The method `method` of `owner` as a value, `targs` its type
    /// arguments (None for a builtin, whose one entry serves them all);
    /// `params` are the types of the parameters it really takes, a `&rest`
    /// one as a list, and `ret` what it answers.
    MethodRef { owner: Path, #[serde(with = "crate::owned_form::sym_serde")] method: SymRef, targs: Vec<Type>, params: Vec<Type>, ret: Type },
    /// A call of the function value `callee`, its `&rest` arguments as one
    /// list.
    Apply { callee: Box<Typed>, args: Vec<Typed> },
    /// `compile`, `trace`, `untrace` or `disassemble` (`tag`) of the bodies
    /// `targets`; `llvm_ir` says what `disassemble` shows.
    Tool { tag: u32, targets: Vec<crate::check::resolved::CompileTarget>, llvm_ir: bool },
    /// `(task call)` or `(thread call)` (`tag`): `call` run on its own, the
    /// value a handle on it.
    Spawn { tag: u32, call: Box<Typed> },
    /// `(select arm...)`: the first arm whose channel operation can go.
    Select { arms: Vec<SelectArm> },
    /// `(panic message)`: stop with `message`.
    Panic { message: Box<Typed> },
    /// `(step form)`: `form`, run one step at a time.
    Step { inner: Box<Typed> },
    /// `(unwind-protect protected cleanup)`: `protected`'s value, `cleanup`
    /// run after it however it leaves.
    UnwindProtect { protected: Box<Typed>, cleanup: Box<Typed> },
    /// `let`: the bindings, each checked outside all of them, then the body
    /// in order. With no bindings it is `progn`.
    Let { #[serde(with = "super::wire::sym_pairs")] binds: Vec<(SymRef, Typed)>, body: Vec<Typed> },
    If { cond: Box<Typed>, then: Box<Typed>, els: Box<Typed> },
    /// `match`: the first arm whose pattern matches the scrutinee's value is
    /// run, the names its pattern binds bound.
    Match { scrut: Box<Typed>, arms: Vec<(Pat, Vec<Typed>)> },
    /// An `&optional` or `&key` parameter's default, which the call left
    /// out: the core form its definition checked.
    Default { form: OwnedForm },
    /// `value` boxed as the trait object of the trait `trait_path` this
    /// expression's type is. Its table is laid out from `value`'s type when
    /// the tree is lowered.
    DynNew { trait_path: Path, value: Box<Typed> },
    /// A call of the method `method`, the slot `slot` of the trait
    /// `trait_path`'s table, on the trait object `args[0]`.
    DynCall { trait_path: Path, #[serde(with = "crate::owned_form::sym_serde")] method: SymRef, slot: usize, args: Vec<Typed> },
    /// The trait object `value` given the table of its trait's supertrait
    /// `to`, which the box carries.
    DynUpcast { to: Path, value: Box<Typed> },
    /// `value`, of its own type, held as a `Sexpr`. How is chosen when the
    /// tree is lowered, from the settled type ([`super::sexpr_value`]).
    AsSexpr { value: Box<Typed> },
    /// The value the trait object `value` holds, as a `Sexpr`: what a
    /// `match` takes it apart as, and what it is put into a `Sexpr` as.
    DynValue { value: Box<Typed> },
    /// Where recover mode checked past an error (`Cx::recovered`): an
    /// expression of type `!`.
    Hole,
}

/// One arm of a `select`. Its channel and value are locals the `let`
/// around the `select` binds; its body is a `let` with no bindings.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum SelectArm {
    /// `((var (recv chan)) body...)`; `var` is bound to `bound`, the
    /// `Option` of the channel's element.
    Recv { #[serde(with = "crate::owned_form::sym_serde")] var: SymRef, bound: Type, chan: Typed, body: Typed },
    /// `((send chan value) body...)`, `elem` the channel's element type.
    Send { elem: Type, chan: Typed, value: Typed, body: Typed },
    /// `(:else body...)`.
    Else { body: Typed },
}

impl SelectArm {
    fn parts(&self) -> Vec<&Typed> {
        match self {
            SelectArm::Recv { chan, body, .. } => vec![chan, body],
            SelectArm::Send { chan, value, body, .. } => vec![chan, value, body],
            SelectArm::Else { body } => vec![body],
        }
    }

    fn parts_mut(&mut self) -> Vec<&mut Typed> {
        match self {
            SelectArm::Recv { chan, body, .. } => vec![chan, body],
            SelectArm::Send { chan, value, body, .. } => vec![chan, value, body],
            SelectArm::Else { body } => vec![body],
        }
    }
}

/// A checked pattern.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Pat {
    /// `_`: matches anything.
    Wild,
    /// A name: matches anything, and binds the name to it.
    Bind(#[serde(with = "crate::owned_form::sym_serde")] SymRef),
    /// The variant `variant` of `owner<targs>`, its fields (declared
    /// `field_tys` at these arguments) matched by `subs`. A `downcast` one
    /// matches a `Sexpr` holding such a value.
    Ctor { owner: Path, targs: Vec<Type>, variant: usize, downcast: bool, field_tys: Vec<Type>, subs: Vec<Pat> },
    /// An integer, a `bool` or a `char` literal of the value's own type
    /// (`tag` its core tag): matches the value with that word.
    Lit { tag: u32, #[serde(with = "super::wire::slot")] value: Value },
    /// Matches a value for which `test` is true, the value bound to `name`
    /// while it runs.
    Guard { #[serde(with = "crate::owned_form::sym_serde")] name: SymRef, test: Box<Typed> },
    /// An arm's `(:or p1 p2 ...)`, written anywhere in its pattern, as the
    /// whole patterns it stands for: matches what one of them matches. Each
    /// binds the same names at the same types, which the arm's body reads.
    Or(Vec<Pat>),
    /// An arm's pattern `pat` with its `:when` condition `test`, checked
    /// with what `pat` binds: matches what `pat` matches when `test` is
    /// true.
    When { pat: Box<Pat>, test: Box<Typed> },
    /// `(the ty sub)`: matches a `Sexpr` holding a value of `ty`, which
    /// `sub` matches.
    TypeTest { ty: Type, sub: Box<Pat> },
    /// A pattern against a trait object: `sub`, a `Sexpr` pattern, matches
    /// the value it holds.
    Dyn { sub: Box<Pat> },
}

impl Pat {
    /// The tests in this pattern and the patterns inside it.
    fn tests(&self) -> Vec<&Typed> {
        match self {
            Pat::Guard { test, .. } => vec![&**test],
            Pat::Ctor { subs, .. } | Pat::Or(subs) => subs.iter().flat_map(Pat::tests).collect(),
            Pat::When { pat, test } => pat.tests().into_iter().chain([&**test]).collect(),
            Pat::TypeTest { sub, .. } | Pat::Dyn { sub } => sub.tests(),
            Pat::Wild | Pat::Bind(_) | Pat::Lit { .. } => Vec::new(),
        }
    }

    /// [`Self::tests`], to change.
    fn tests_mut(&mut self) -> Vec<&mut Typed> {
        match self {
            Pat::Guard { test, .. } => vec![&mut **test],
            Pat::Ctor { subs, .. } | Pat::Or(subs) => subs.iter_mut().flat_map(Pat::tests_mut).collect(),
            Pat::When { pat, test } => pat.tests_mut().into_iter().chain([&mut **test]).collect(),
            Pat::TypeTest { sub, .. } | Pat::Dyn { sub } => sub.tests_mut(),
            Pat::Wild | Pat::Bind(_) | Pat::Lit { .. } => Vec::new(),
        }
    }

    /// The literals' values in this pattern and the patterns inside it.
    fn each_value(&self, f: &mut dyn FnMut(Value)) {
        match self {
            Pat::Lit { value, .. } => f(*value),
            Pat::Ctor { subs, .. } | Pat::Or(subs) => subs.iter().for_each(|p| p.each_value(f)),
            Pat::When { pat, .. } | Pat::TypeTest { sub: pat, .. } | Pat::Dyn { sub: pat } => pat.each_value(f),
            Pat::Wild | Pat::Bind(_) | Pat::Guard { .. } => {}
        }
    }

    /// [`Self::each_value`], to change.
    fn values_mut(&mut self, f: &mut dyn FnMut(&mut Value)) {
        match self {
            Pat::Lit { value, .. } => f(value),
            Pat::Ctor { subs, .. } | Pat::Or(subs) => subs.iter_mut().for_each(|p| p.values_mut(f)),
            Pat::When { pat, .. } | Pat::TypeTest { sub: pat, .. } | Pat::Dyn { sub: pat } => pat.values_mut(f),
            Pat::Wild | Pat::Bind(_) | Pat::Guard { .. } => {}
        }
    }

    fn map_types(&mut self, f: &impl Fn(&Type) -> Type) {
        match self {
            Pat::Ctor { targs, field_tys, subs, .. } => {
                for t in targs.iter_mut().chain(field_tys.iter_mut()) {
                    *t = f(t);
                }
                subs.iter_mut().for_each(|p| p.map_types(f));
            }
            Pat::Or(alts) => alts.iter_mut().for_each(|p| p.map_types(f)),
            Pat::When { pat, .. } | Pat::Dyn { sub: pat } => pat.map_types(f),
            Pat::TypeTest { ty, sub } => {
                *ty = f(ty);
                sub.map_types(f);
            }
            Pat::Wild | Pat::Bind(_) | Pat::Lit { .. } | Pat::Guard { .. } => {}
        }
    }
}

/// A tree nested as deep as the program is is dropped with the stack grown
/// as it goes, like every other walk over it.
impl Drop for Typed {
    fn drop(&mut self) {
        let node = std::mem::replace(&mut self.node, Node::Let { binds: Vec::new(), body: Vec::new() });
        typelisp_mem::grow_stack(move || drop(node));
    }
}


impl Typed {
    /// `(Some value)`, an `Option` of `value`'s type.
    pub(crate) fn some(value: Typed, loc: Option<Loc>) -> Typed {
        let inner = value.ty.clone();
        let node = Node::Construct {
            owner: option_path(),
            targs: vec![inner.clone()],
            variant: OPTION_SOME,
            mutable: false,
            field_tys: vec![inner.clone()],
            args: vec![value],
        };
        Typed { node, ty: Type::Named(option_path(), vec![inner]), loc }
    }

    /// `None`, an `Option<inner>`.
    pub(crate) fn none(inner: Type) -> Typed {
        let node = Node::Construct { owner: option_path(), targs: vec![inner.clone()], variant: OPTION_NONE, mutable: false, field_tys: Vec::new(), args: Vec::new() };
        Typed { node, ty: Type::Named(option_path(), vec![inner]), loc: None }
    }

    /// The expressions directly inside this one.
    fn children(&self) -> Vec<&Typed> {
        match &self.node {
            Node::Literal { .. }
            | Node::Number { .. }
            | Node::Quote { .. }
            | Node::Hole
            | Node::Local { .. }
            | Node::Global { .. }
            | Node::Break
            | Node::Default { .. }
            | Node::FnRef { .. }
            | Node::MethodRef { .. }
            | Node::TypeValue { .. }
            | Node::Tool { .. } => {
                Vec::new()
            }
            Node::Call { args, .. } | Node::MethodCall { args, .. } | Node::TraitCall { args, .. } | Node::DynCall { args, .. } | Node::Construct { args, .. } | Node::Loop { body: args } | Node::Lambda { body: args, .. } => {
                args.iter().collect()
            }
            Node::Apply { callee, args } => std::iter::once(&**callee).chain(args).collect(),
            Node::Select { arms } => arms.iter().flat_map(SelectArm::parts).collect(),
            Node::Labels { defs, body, .. } => defs.iter().flat_map(|d| &d.body).chain(body).collect(),
            Node::Callback { lifted, .. } => lifted.iter().map(|l| &**l).collect(),
            Node::Return { value } | Node::ReturnFrom { value, .. } => value.iter().map(|v| &**v).collect(),
            Node::FieldGet { obj, .. }
            | Node::AsSexpr { value: obj }
            | Node::DynValue { value: obj }
            | Node::DynNew { value: obj, .. }
            | Node::DynUpcast { value: obj, .. }
            | Node::Spawn { call: obj, .. }
            | Node::Set { value: obj, .. }
            | Node::SetGlobal { value: obj, .. }
            | Node::Block { body: obj, .. }
            | Node::Catch { body: obj, .. }
            | Node::Throw { value: obj, .. }
            | Node::UntagInt { value: obj }
            | Node::Panic { message: obj }
            | Node::Step { inner: obj } => {
                vec![&**obj]
            }
            Node::FieldSet { obj, value, .. } | Node::UnwindProtect { protected: obj, cleanup: value } => vec![&**obj, &**value],
            Node::Let { binds, body } => binds.iter().map(|(_, t)| t).chain(body).collect(),
            Node::If { cond, then, els } => vec![&**cond, &**then, &**els],
            Node::Match { scrut, arms } => std::iter::once(&**scrut).chain(arms.iter().flat_map(|(pat, body)| pat.tests().into_iter().chain(body))).collect(),
        }
    }

    /// [`Self::children`], to change.
    fn children_mut(&mut self) -> Vec<&mut Typed> {
        match &mut self.node {
            Node::Literal { .. }
            | Node::Number { .. }
            | Node::Quote { .. }
            | Node::Hole
            | Node::Local { .. }
            | Node::Global { .. }
            | Node::Break
            | Node::Default { .. }
            | Node::FnRef { .. }
            | Node::MethodRef { .. }
            | Node::TypeValue { .. }
            | Node::Tool { .. } => {
                Vec::new()
            }
            Node::Call { args, .. } | Node::MethodCall { args, .. } | Node::TraitCall { args, .. } | Node::DynCall { args, .. } | Node::Construct { args, .. } | Node::Loop { body: args } | Node::Lambda { body: args, .. } => {
                args.iter_mut().collect()
            }
            Node::Apply { callee, args } => std::iter::once(&mut **callee).chain(args.iter_mut()).collect(),
            Node::Select { arms } => arms.iter_mut().flat_map(SelectArm::parts_mut).collect(),
            Node::Labels { defs, body, .. } => defs.iter_mut().flat_map(|d| d.body.iter_mut()).chain(body.iter_mut()).collect(),
            Node::Callback { lifted, .. } => lifted.iter_mut().map(|l| &mut **l).collect(),
            Node::Return { value } | Node::ReturnFrom { value, .. } => value.iter_mut().map(|v| &mut **v).collect(),
            Node::FieldGet { obj, .. }
            | Node::AsSexpr { value: obj }
            | Node::DynValue { value: obj }
            | Node::DynNew { value: obj, .. }
            | Node::DynUpcast { value: obj, .. }
            | Node::Spawn { call: obj, .. }
            | Node::Set { value: obj, .. }
            | Node::SetGlobal { value: obj, .. }
            | Node::Block { body: obj, .. }
            | Node::Catch { body: obj, .. }
            | Node::Throw { value: obj, .. }
            | Node::UntagInt { value: obj }
            | Node::Panic { message: obj }
            | Node::Step { inner: obj } => {
                vec![&mut **obj]
            }
            Node::FieldSet { obj, value, .. } | Node::UnwindProtect { protected: obj, cleanup: value } => vec![&mut **obj, &mut **value],
            Node::Let { binds, body } => binds.iter_mut().map(|(_, t)| t).chain(body.iter_mut()).collect(),
            Node::If { cond, then, els } => vec![&mut **cond, &mut **then, &mut **els],
            Node::Match { scrut, arms } => {
                std::iter::once(&mut **scrut).chain(arms.iter_mut().flat_map(|(pat, body)| pat.tests_mut().into_iter().chain(body.iter_mut()))).collect()
            }
        }
    }

    /// Call `f` on this expression and every one inside it, outside first.
    pub(crate) fn walk_mut<E>(&mut self, f: &mut impl FnMut(&mut Typed) -> Result<(), E>) -> Result<(), E> {
        typelisp_mem::grow_stack(|| {
            f(self)?;
            self.children_mut().into_iter().try_for_each(|c| c.walk_mut(f))
        })
    }

    /// [`Self::walk_mut`] without changing anything.
    pub(crate) fn walk(&self, f: &mut impl FnMut(&Typed)) {
        typelisp_mem::grow_stack(|| {
            f(self);
            self.children().into_iter().for_each(|c| c.walk(f));
        })
    }

    /// Every type in this tree replaced with `f` of it.
    pub(crate) fn map_types(&mut self, f: &impl Fn(&Type) -> Type) {
        let mapped = self.walk_mut(&mut |t| {
            t.ty = f(&t.ty);
            match &mut t.node {
                Node::Call { targs, .. } | Node::MethodCall { targs, .. } | Node::TraitCall { own: targs, .. } => targs.iter_mut().for_each(|ty| *ty = f(ty)),
                Node::Construct { targs, field_tys, .. } => targs.iter_mut().chain(field_tys.iter_mut()).for_each(|ty| *ty = f(ty)),
                Node::FieldSet { field_ty, .. } | Node::Catch { carried: field_ty, .. } | Node::TypeValue { ty: field_ty } | Node::Callback { fn_ty: field_ty, .. } => *field_ty = f(field_ty),
                Node::Lambda { params, ret, .. } => {
                    params.iter_mut().for_each(|(_, ty)| *ty = f(ty));
                    *ret = f(ret);
                }
                Node::FnRef { targs, params, .. } => targs.iter_mut().chain(params.iter_mut()).for_each(|ty| *ty = f(ty)),
                Node::MethodRef { targs, params, ret, .. } => {
                    targs.iter_mut().chain(params.iter_mut()).for_each(|ty| *ty = f(ty));
                    *ret = f(ret);
                }
                Node::Labels { defs, .. } => defs.iter_mut().for_each(|d| {
                    d.params.iter_mut().for_each(|(_, ty)| *ty = f(ty));
                    d.ret = f(&d.ret);
                }),
                Node::Match { arms, .. } => arms.iter_mut().for_each(|(pat, _)| pat.map_types(f)),
                Node::Select { arms } => arms.iter_mut().for_each(|a| match a {
                    SelectArm::Recv { bound: ty, .. } | SelectArm::Send { elem: ty, .. } => *ty = f(ty),
                    SelectArm::Else { .. } => {}
                }),
                Node::Literal { .. }
                | Node::Number { .. }
                | Node::Quote { .. }
                | Node::Hole
                | Node::Local { .. }
                | Node::Global { .. }
                | Node::FieldGet { .. }
                | Node::AsSexpr { .. }
                | Node::DynValue { .. }
                | Node::Set { .. }
                | Node::SetGlobal { .. }
                | Node::Loop { .. }
                | Node::Break
                | Node::Return { .. }
                | Node::Block { .. }
                | Node::ReturnFrom { .. }
                | Node::Throw { .. }
                | Node::UntagInt { .. }
                | Node::UnwindProtect { .. }
                | Node::Panic { .. }
                | Node::Step { .. }
                | Node::Default { .. }
                | Node::Apply { .. }
                | Node::Tool { .. }
                | Node::DynNew { .. }
                | Node::DynCall { .. }
                | Node::DynUpcast { .. }
                | Node::Spawn { .. }
                | Node::Let { .. }
                | Node::If { .. } => {}
            }
            Ok::<(), std::convert::Infallible>(())
        });
        match mapped {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    /// Call `f` on every value this tree holds, a pattern's too, outside
    /// first: the order [`Self::values_mut`] visits them in.
    pub(crate) fn each_value(&self, f: &mut impl FnMut(Value)) {
        self.walk(&mut |t| match &t.node {
            Node::Literal { value: Some(v), .. } | Node::Number { value: v } | Node::Quote { datum: v } => f(*v),
            Node::Match { arms, .. } => arms.iter().for_each(|(pat, _)| pat.each_value(&mut |v| f(v))),
            _ => {}
        });
    }

    /// [`Self::each_value`], to change.
    pub(crate) fn values_mut(&mut self, f: &mut impl FnMut(&mut Value)) {
        let walked = self.walk_mut(&mut |t| {
            match &mut t.node {
                Node::Literal { value: Some(v), .. } | Node::Number { value: v } | Node::Quote { datum: v } => f(v),
                Node::Match { arms, .. } => arms.iter_mut().for_each(|(pat, _)| pat.values_mut(&mut |v| f(v))),
                _ => {}
            }
            Ok::<(), std::convert::Infallible>(())
        });
        match walked {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }
}

fn option_path() -> Path {
    Path::builtin(typelisp_mem::wk::OPTION)
}

/// `Option`'s variant indices (`typeck::builtins`'s `Option`). Written down
/// once because three places already depend on the order — `()`'s inference
/// rule, the niche pattern shapes, and `check_construct`'s `None` shortcut.
pub(crate) const OPTION_SOME: usize = 0;

pub(crate) const OPTION_NONE: usize = 1;

#[cfg(test)]
mod flat_tests {
    use super::{Node, Typed};
    use crate::types::Type;

    fn unit() -> Typed {
        Typed { node: Node::Literal { tag: 0, value: None }, ty: Type::Unit, loc: None }
    }

    /// `depth` `if`s, each the `else` of the one around it, with a unit
    /// condition and `then`.
    fn nested(depth: usize) -> Typed {
        let mut t = unit();
        for _ in 0..depth {
            t = Typed { node: Node::If { cond: Box::new(unit()), then: Box::new(unit()), els: Box::new(t) }, ty: Type::Unit, loc: None };
        }
        t
    }

    fn depth(mut t: &Typed) -> usize {
        let mut n = 0;
        while let Node::If { els, .. } = &t.node {
            n += 1;
            t = els;
        }
        n
    }

    /// A tree far deeper than a small stack could read back recursively
    /// comes back whole on a thread with 256 KiB of it, the order of every
    /// expression's children kept.
    #[test]
    fn a_deep_tree_is_read_back_on_a_small_stack() {
        let bytes = typelisp_mem::grow_stack(|| bincode::serialize(&nested(20_000)).unwrap());
        let read = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let t: Typed = bincode::deserialize(&bytes).unwrap();
                let d = depth(&t);
                let Node::If { cond, .. } = &t.node else { panic!("not an if") };
                let first = matches!(cond.node, Node::Literal { tag: 0, value: None });
                (d, first)
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(read, (20_000, true));
    }
}
