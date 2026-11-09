//! The typed tree: what checking a form produces, before it is lowered to
//! core IR ([`super::lower`]).
//!
//! Lowering waits for the whole form to be checked because a type may still
//! hold an inference hole while the form is being checked, and the core IR
//! writes each value's representation, which only a solved type has.

use crate::check::analyze::{OPTION_NONE, OPTION_SOME};
use crate::owned_form::OwnedForm;
use crate::types::{Path, Type};
use crate::{Loc, Value};
use typelisp_mem::SymRef;

/// A checked expression: what it does, its type, and where it was written.
#[derive(Debug)]
pub(crate) struct Typed {
    pub(crate) node: Node,
    pub(crate) ty: Type,
    pub(crate) loc: Option<Loc>,
}

#[derive(Debug)]
pub(crate) enum Node {
    /// A literal: its core tag and its value (none for the unit value).
    Literal { tag: u32, value: Option<Value> },
    /// `(quote datum)`: the datum as it was read.
    Quote { datum: Value },
    /// A number literal whose type was not known where it was written: an
    /// integer or a float type the rest of the form decides. It becomes a
    /// [`Node::Literal`] when the form is settled (`Cx::settle`), so
    /// lowering never sees one.
    Number { value: Value },
    /// A local variable's value.
    Local { name: SymRef },
    /// The global variable `path`'s value.
    Global { path: Path },
    /// A call to the function `path`, at the type arguments `targs` when it
    /// is generic.
    Call { path: Path, targs: Vec<Type>, args: Vec<Typed> },
    /// A call to the method `method` of the type `owner`, at the type
    /// arguments `targs` when it is generic. An instance method's receiver is
    /// the first argument.
    MethodCall { owner: Path, method: SymRef, instance: bool, targs: Vec<Type>, args: Vec<Typed> },
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
    Set { name: SymRef, value: Box<Typed> },
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
    /// `(lambda (params) ret body...)`: a function value.
    Lambda { params: Vec<(SymRef, Type)>, ret: Type, body: Vec<Typed> },
    /// `(labels (def...) body...)`: local functions, which see each other.
    Labels { defs: Vec<super::lambda::Label>, body: Vec<Typed> },
    /// The function `path` as a value; `params` are the types of the
    /// parameters it really takes, a `&rest` one as a list.
    FnRef { path: Path, params: Vec<Type> },
    /// A call of the function value `callee`, its `&rest` arguments as one
    /// list.
    Apply { callee: Box<Typed>, args: Vec<Typed> },
    /// `compile`, `trace`, `untrace` or `disassemble` (`tag`) of the bodies
    /// `targets`; `llvm_ir` says what `disassemble` shows.
    Tool { tag: u32, targets: Vec<crate::check::resolved::CompileTarget>, llvm_ir: bool },
    /// `(panic message)`: stop with `message`.
    Panic { message: Box<Typed> },
    /// `(unwind-protect protected cleanup)`: `protected`'s value, `cleanup`
    /// run after it however it leaves.
    UnwindProtect { protected: Box<Typed>, cleanup: Box<Typed> },
    /// `let`: the bindings, each checked outside all of them, then the body
    /// in order. With no bindings it is `progn`.
    Let { binds: Vec<(SymRef, Typed)>, body: Vec<Typed> },
    If { cond: Box<Typed>, then: Box<Typed>, els: Box<Typed> },
    /// `match`: the first arm whose pattern matches the scrutinee's value is
    /// run, the names its pattern binds bound.
    Match { scrut: Box<Typed>, arms: Vec<(Pat, Vec<Typed>)> },
    /// An `&optional` or `&key` parameter's default, which the call left
    /// out: the core form its definition checked.
    Default { form: OwnedForm },
    /// `value`, of its own type, held as a `Sexpr`. How is chosen when the
    /// tree is lowered, from the settled type ([`super::sexpr_value`]).
    AsSexpr { value: Box<Typed> },
}

/// A checked pattern.
#[derive(Debug)]
pub(crate) enum Pat {
    /// `_`: matches anything.
    Wild,
    /// A name: matches anything, and binds the name to it.
    Bind(SymRef),
    /// The variant `variant` of `owner<targs>`, its fields (declared
    /// `field_tys` at these arguments) matched by `subs`.
    Ctor { owner: Path, targs: Vec<Type>, variant: usize, field_tys: Vec<Type>, subs: Vec<Pat> },
}

impl Pat {
    fn map_types(&mut self, f: &impl Fn(&Type) -> Type) {
        if let Pat::Ctor { targs, field_tys, subs, .. } = self {
            for t in targs.iter_mut().chain(field_tys.iter_mut()) {
                *t = f(t);
            }
            subs.iter_mut().for_each(|p| p.map_types(f));
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
    /// `(some value)`, an `Option` of `value`'s type.
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

    /// `none`, an `Option<inner>`.
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
            | Node::Local { .. }
            | Node::Global { .. }
            | Node::Break
            | Node::Default { .. }
            | Node::FnRef { .. }
            | Node::Tool { .. } => {
                Vec::new()
            }
            Node::Call { args, .. } | Node::MethodCall { args, .. } | Node::Construct { args, .. } | Node::Loop { body: args } | Node::Lambda { body: args, .. } => {
                args.iter().collect()
            }
            Node::Apply { callee, args } => std::iter::once(&**callee).chain(args).collect(),
            Node::Labels { defs, body } => defs.iter().flat_map(|d| &d.body).chain(body).collect(),
            Node::Return { value } => value.iter().map(|v| &**v).collect(),
            Node::FieldGet { obj, .. }
            | Node::AsSexpr { value: obj }
            | Node::Set { value: obj, .. }
            | Node::SetGlobal { value: obj, .. }
            | Node::Panic { message: obj } => {
                vec![&**obj]
            }
            Node::FieldSet { obj, value, .. } | Node::UnwindProtect { protected: obj, cleanup: value } => vec![&**obj, &**value],
            Node::Let { binds, body } => binds.iter().map(|(_, t)| t).chain(body).collect(),
            Node::If { cond, then, els } => vec![&**cond, &**then, &**els],
            Node::Match { scrut, arms } => std::iter::once(&**scrut).chain(arms.iter().flat_map(|(_, body)| body)).collect(),
        }
    }

    /// [`Self::children`], to change.
    fn children_mut(&mut self) -> Vec<&mut Typed> {
        match &mut self.node {
            Node::Literal { .. }
            | Node::Number { .. }
            | Node::Quote { .. }
            | Node::Local { .. }
            | Node::Global { .. }
            | Node::Break
            | Node::Default { .. }
            | Node::FnRef { .. }
            | Node::Tool { .. } => {
                Vec::new()
            }
            Node::Call { args, .. } | Node::MethodCall { args, .. } | Node::Construct { args, .. } | Node::Loop { body: args } | Node::Lambda { body: args, .. } => {
                args.iter_mut().collect()
            }
            Node::Apply { callee, args } => std::iter::once(&mut **callee).chain(args.iter_mut()).collect(),
            Node::Labels { defs, body } => defs.iter_mut().flat_map(|d| d.body.iter_mut()).chain(body.iter_mut()).collect(),
            Node::Return { value } => value.iter_mut().map(|v| &mut **v).collect(),
            Node::FieldGet { obj, .. }
            | Node::AsSexpr { value: obj }
            | Node::Set { value: obj, .. }
            | Node::SetGlobal { value: obj, .. }
            | Node::Panic { message: obj } => {
                vec![&mut **obj]
            }
            Node::FieldSet { obj, value, .. } | Node::UnwindProtect { protected: obj, cleanup: value } => vec![&mut **obj, &mut **value],
            Node::Let { binds, body } => binds.iter_mut().map(|(_, t)| t).chain(body.iter_mut()).collect(),
            Node::If { cond, then, els } => vec![&mut **cond, &mut **then, &mut **els],
            Node::Match { scrut, arms } => {
                std::iter::once(&mut **scrut).chain(arms.iter_mut().flat_map(|(_, body)| body.iter_mut())).collect()
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
    fn walk(&self, f: &mut impl FnMut(&Typed)) {
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
                Node::Call { targs, .. } | Node::MethodCall { targs, .. } => targs.iter_mut().for_each(|ty| *ty = f(ty)),
                Node::Construct { targs, field_tys, .. } => targs.iter_mut().chain(field_tys.iter_mut()).for_each(|ty| *ty = f(ty)),
                Node::FieldSet { field_ty, .. } => *field_ty = f(field_ty),
                Node::Lambda { params, ret, .. } => {
                    params.iter_mut().for_each(|(_, ty)| *ty = f(ty));
                    *ret = f(ret);
                }
                Node::FnRef { params, .. } => params.iter_mut().for_each(|ty| *ty = f(ty)),
                Node::Labels { defs, .. } => defs.iter_mut().for_each(|d| {
                    d.params.iter_mut().for_each(|(_, ty)| *ty = f(ty));
                    d.ret = f(&d.ret);
                }),
                Node::Match { arms, .. } => arms.iter_mut().for_each(|(pat, _)| pat.map_types(f)),
                Node::Literal { .. }
                | Node::Number { .. }
                | Node::Quote { .. }
                | Node::Local { .. }
                | Node::Global { .. }
                | Node::FieldGet { .. }
                | Node::AsSexpr { .. }
                | Node::Set { .. }
                | Node::SetGlobal { .. }
                | Node::Loop { .. }
                | Node::Break
                | Node::Return { .. }
                | Node::UnwindProtect { .. }
                | Node::Panic { .. }
                | Node::Default { .. }
                | Node::Apply { .. }
                | Node::Tool { .. }
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

    /// Call `f` on every value this tree holds.
    pub(crate) fn each_value(&self, f: &mut impl FnMut(Value)) {
        self.walk(&mut |t| match &t.node {
            Node::Literal { value: Some(v), .. } | Node::Number { value: v } | Node::Quote { datum: v } => f(*v),
            _ => {}
        });
    }
}

fn option_path() -> Path {
    Path::builtin(typelisp_mem::wk::OPTION)
}
