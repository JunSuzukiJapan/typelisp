//! Lowering: the typed tree ([`super::tree`]) to core IR.
//!
//! One arm per kind of node. Lowering decides nothing a check depends on: by
//! the time a tree is lowered, every type in it is settled. The core forms
//! are built by [`CoreForms`], which the current checker builds with too.

use crate::check::core;
use crate::check::forms;
use crate::check::analyze::SEXPR_RESERVED_VARIANT;
use crate::check::lower::{Arg, CoreForms, LoweredLabel};
use crate::check::repr::Repr;
use crate::types::{path_is_builtin, prim_type_path, Path, Type};
use crate::{wk, Error, Heap, Value};
use typelisp_mem::SymRef;

use super::generic::Requests;
use super::registry::Registry;
use super::sexpr_value::{encoding, no_encoding, Encoding};
use super::subst::Subst;
use super::tree::{Node, Pat, SelectArm, Typed};

/// What lowering one tree needs: the definitions, the substitution that
/// makes a generic body one specialization (empty for any other tree), and
/// the specializations the form asks for.
pub(crate) struct Lowering<'a> {
    pub(crate) reg: &'a Registry,
    pub(crate) subst: &'a Subst,
    pub(crate) requests: &'a mut Requests,
}

impl Lowering<'_> {
    /// `ty` as this lowering writes it.
    pub(crate) fn settle(&self, ty: &Type) -> Type {
        self.subst.apply(ty)
    }
}

/// `t`'s core form, unrooted. It carries `t`'s position on its first cell.
pub(crate) fn lower(heap: &mut Heap, lw: &mut Lowering, t: &Typed) -> Result<Value, Error> {
    let form = typelisp_mem::grow_stack(|| lower_node(heap, lw, t))?;
    if let (Value::Cons(cell), Some(loc)) = (form, &t.loc) {
        if heap.cons_loc(form).is_none() {
            heap.set_cons_loc(cell, loc.clone());
        }
    }
    Ok(form)
}

/// `t` lowered and rooted on `heap`'s root stack.
pub(crate) fn lower_rooted(heap: &mut Heap, lw: &mut Lowering, t: &Typed) -> Result<Value, Error> {
    let form = lower(heap, lw, t)?;
    heap.push_root(form);
    Ok(form)
}

fn lower_node(heap: &mut Heap, lw: &mut Lowering, t: &Typed) -> Result<Value, Error> {
    match &t.node {
        Node::Literal { tag, value } => match value {
            Some(v) => core::tagged(heap, *tag, &[*v]),
            None => core::tagged(heap, *tag, &[]),
        },
        Node::Number { .. } => unreachable!("a number literal is settled with its form (`Cx::settle`)"),
        Node::Quote { datum } => forms::quote_form(heap, *datum),
        Node::Local { name } => forms::var_form(heap, *name),
        Node::Global { path } => {
            let ty = lw.settle(&t.ty);
            lw.reg.global_form(heap, path, &ty)
        }
        Node::Call { path, targs, args } => {
            let path = if targs.is_empty() {
                *path
            } else {
                let targs = targs.iter().map(|t| lw.settle(t)).collect();
                lw.requests.ask_fn(path, targs)
            };
            let mut lowered = Vec::with_capacity(args.len());
            for a in args {
                lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
            }
            let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
            lw.reg.call_form(heap, &path, &lowered)
        }
        Node::MethodCall { owner, method, instance, targs, args } => {
            let method = if targs.is_empty() {
                *method
            } else {
                let targs = targs.iter().map(|t| lw.settle(t)).collect();
                lw.requests.ask_method(owner, *method, targs)
            };
            let mut lowered = Vec::with_capacity(args.len());
            for a in args {
                lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
            }
            let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
            let ret = lw.settle(&t.ty);
            lw.reg.assoc_form(heap, owner, method, *instance, &ret, &lowered)
        }
        Node::TraitCall { trait_path, method, args } => {
            // The receiver's type is the specialization's.
            let recv = lw.settle(&args[0].ty);
            let (owner, type_args) = match &recv {
                Type::Named(path, type_args) => (*path, type_args.clone()),
                other => match prim_type_path(other) {
                    Some(path) => (path, Vec::new()),
                    None => unimplemented!("typeck: `{}::{}` on a `{}`, which names no type", trait_path, method, other),
                },
            };
            let Some(def) = lw.reg.method(&owner, *method) else {
                if lw.reg.has_inst_method(&owner, *method) {
                    unimplemented!("typeck: `{}::{}` on a `{}`, whose `impl` is for one instantiation", trait_path, method, recv)
                }
                if lw.reg.blanket_impls.get(trait_path).is_some() {
                    unimplemented!("typeck: `{}::{}` on a `{}`, through a blanket `impl`", trait_path, method, recv)
                }
                unreachable!("`{}` meets the bound on `{}`, so it has `{}`", recv, trait_path, method)
            };
            // An `impl`'s methods have the type's parameters, in order.
            let method = if def.sig.builtin || def.sig.type_params.is_empty() {
                *method
            } else if def.sig.type_params.len() == type_args.len() {
                lw.requests.ask_method(&owner, *method, type_args)
            } else {
                unimplemented!("typeck: `{}::{}`, a trait method with type parameters of its own", owner, method)
            };
            let mut lowered = Vec::with_capacity(args.len());
            for a in args {
                lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
            }
            let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
            let ret = lw.settle(&t.ty);
            lw.reg.assoc_form(heap, &owner, method, true, &ret, &lowered)
        }
        Node::DynNew { trait_path, value } => {
            let concrete = lw.settle(&value.ty);
            let (slots, supers) = super::dyn_trait::tables(lw, &concrete, trait_path)?;
            let form = lower_rooted(heap, lw, value)?;
            lw.reg.dyn_new_form(heap, trait_path, &slots, &supers, &concrete, form)
        }
        Node::DynCall { trait_path, method, slot, args } => {
            let mut lowered = Vec::with_capacity(args.len());
            for a in args {
                lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
            }
            let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
            let targets = super::dyn_trait::targets(lw.reg, trait_path, *method);
            lw.reg.dyn_call_form(heap, trait_path, *method, *slot, &targets, &lowered)
        }
        Node::Select { arms } => {
            let mut lowered = Vec::with_capacity(arms.len());
            for arm in arms {
                let form = match arm {
                    SelectArm::Recv { var, bound, chan, body } => {
                        let chan = lower_rooted(heap, lw, chan)?;
                        let body = lower_rooted(heap, lw, body)?;
                        forms::select_recv_arm(heap, *var, &lw.settle(bound), chan, body)?
                    }
                    SelectArm::Send { elem, chan, value, body } => {
                        let kind = lw.reg.repr(&lw.settle(elem)).field_kind();
                        let chan = lower_rooted(heap, lw, chan)?;
                        let value = lower_rooted(heap, lw, value)?;
                        let body = lower_rooted(heap, lw, body)?;
                        forms::select_send_arm(heap, kind, chan, value, body)?
                    }
                    SelectArm::Else { body } => {
                        let body = lower_rooted(heap, lw, body)?;
                        forms::select_else_arm(heap, body)?
                    }
                };
                heap.push_root(form);
                lowered.push(form);
            }
            forms::select_form(heap, &lowered)
        }
        Node::Spawn { tag, call } => {
            let form = lower_rooted(heap, lw, call)?;
            // A macro may have expanded to something that is not a call, and
            // a builtin call over literals folds to its value: what it
            // lowered to is the last word.
            let is_call = matches!(heap.car(form), Ok(Value::Symbol(id)) if matches!(heap.symbol_spelling(id), "call" | "assoc" | "dyn-call" | "apply"));
            if !is_call {
                return Err(Error::TypeError(format!("`{}` needs a call, and this is not one. {}", super::spawn::form_name(*tag), super::spawn::shape(*tag))));
            }
            let ret = lw.reg.repr_form(heap, &lw.settle(&call.ty))?;
            heap.push_root(ret);
            let handle = lw.settle(&t.ty);
            let spawned = forms::spawn_form(heap, *tag, ret, &handle, form);
            heap.pop_root();
            spawned
        }
        Node::DynUpcast { to, value } => {
            let form = lower_rooted(heap, lw, value)?;
            forms::dyn_upcast_form(heap, to, form)
        }
        Node::Construct { owner, targs, variant, mutable, field_tys, args } => {
            let targs: Vec<Type> = targs.iter().map(|t| lw.settle(t)).collect();
            let field_tys: Vec<Type> = field_tys.iter().map(|t| lw.settle(t)).collect();
            let args = args.iter().map(|a| lower_rooted(heap, lw, a)).collect::<Result<Vec<_>, _>>()?;
            // A niched `Option<T>` builds no box: `some v` is `v` as the
            // word a field of type `T` holds, and `none` is the empty list.
            // Only the settled type says whether it is niched.
            if let Some(payload) = option_niche(lw, owner, &targs) {
                return match args[..] {
                    [value] => forms::some_of_form(heap, &payload, value),
                    _ => {
                        let sexpr = Path::builtin(typelisp_mem::wk::SEXPR);
                        lw.reg.construct_form(heap, &sexpr, &[], SEXPR_RESERVED_VARIANT, false, &[], &[])
                    }
                };
            }
            lw.reg.construct_form(heap, owner, &targs, *variant, *mutable, &field_tys, &args)
        }
        Node::FieldGet { obj, index } => {
            let obj = lower_rooted(heap, lw, obj)?;
            let field_ty = lw.settle(&t.ty);
            lw.reg.field_get_form(heap, obj, *index, &field_ty)
        }
        Node::FieldSet { obj, index, field_ty, value } => {
            let obj = lower_rooted(heap, lw, obj)?;
            let value = lower_rooted(heap, lw, value)?;
            let field_ty = lw.settle(field_ty);
            lw.reg.field_set_form(heap, obj, *index, &field_ty, value)
        }
        Node::Set { name, value } => {
            let value = lower_rooted(heap, lw, value)?;
            forms::set_form(heap, *name, value)
        }
        Node::SetGlobal { path, value } => {
            let ty = lw.settle(&t.ty);
            let value = lower_rooted(heap, lw, value)?;
            lw.reg.set_global_form(heap, path, &ty, value)
        }
        Node::Loop { body } => {
            let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
            let repr = lw.reg.repr_form(heap, &lw.settle(&t.ty))?;
            heap.push_root(repr);
            forms::loop_form(heap, repr, &body)
        }
        Node::Block { label, body } => {
            let body = lower_rooted(heap, lw, body)?;
            let repr = lw.reg.repr_form(heap, &lw.settle(&t.ty))?;
            heap.push_root(repr);
            forms::block_form(heap, label, body, repr)
        }
        Node::ReturnFrom { label, value } => {
            let value = value.as_deref().map(|v| lower_rooted(heap, lw, v)).transpose()?;
            forms::return_from_form(heap, label, value)
        }
        Node::Catch { tag, carried, body } => {
            let body = lower_rooted(heap, lw, body)?;
            let tag = forms::quote_form(heap, Value::Symbol(*tag))?;
            heap.push_root(tag);
            let repr = lw.reg.repr_form(heap, &lw.settle(carried))?;
            heap.push_root(repr);
            forms::catch_form(heap, tag, body, repr)
        }
        Node::Throw { tag, value } => {
            let repr = lw.reg.repr_form(heap, &lw.settle(&value.ty))?;
            heap.push_root(repr);
            let value = lower_rooted(heap, lw, value)?;
            let tag = forms::quote_form(heap, Value::Symbol(*tag))?;
            heap.push_root(tag);
            forms::throw_form(heap, tag, value, repr)
        }
        Node::Break => forms::break_form(heap),
        Node::Return { value } => {
            let value = value.as_deref().map(|v| lower_rooted(heap, lw, v)).transpose()?;
            forms::return_form(heap, value)
        }
        Node::Lambda { params, ret, body } => {
            let params: Vec<(SymRef, Type)> = params.iter().map(|(n, ty)| (*n, lw.settle(ty))).collect();
            let ret = lw.settle(ret);
            let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
            lw.reg.lambda_form(heap, &params, &ret, &body)
        }
        Node::Labels { defs, body } => {
            let mut lowered = Vec::with_capacity(defs.len());
            for d in defs {
                let params: Vec<(SymRef, Type)> = d.params.iter().map(|(n, ty)| (*n, lw.settle(ty))).collect();
                let ret = lw.settle(&d.ret);
                let body = d.body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
                lowered.push(LoweredLabel { name: d.name, params, ret, body });
            }
            let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
            lw.reg.labels_core_form(heap, &lowered, &body)
        }
        Node::FnRef { path, params } => {
            let params: Vec<Type> = params.iter().map(|ty| lw.settle(ty)).collect();
            lw.reg.fnref_form(heap, path, &params)
        }
        Node::Apply { callee, args } => {
            let ret = lw.settle(&t.ty);
            let callee = lower_rooted(heap, lw, callee)?;
            let mut lowered = Vec::with_capacity(args.len());
            for a in args {
                lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
            }
            let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
            lw.reg.apply_form(heap, callee, &ret, &lowered)
        }
        Node::Tool { tag, targets, llvm_ir } => match *tag {
            wk::COMPILE => forms::compile_fn_form(heap, &targets[0]),
            wk::DISASSEMBLE => forms::disassemble_fn_form(heap, &targets[0], *llvm_ir),
            tag => forms::trace_form(heap, tag, targets),
        },
        Node::Default { form } => crate::owned_form::owned_to_value(heap, form),
        Node::Panic { message } => {
            let message = lower_rooted(heap, lw, message)?;
            forms::panic_form(heap, message)
        }
        Node::UnwindProtect { protected, cleanup } => {
            let protected = lower_rooted(heap, lw, protected)?;
            let cleanup = lower_rooted(heap, lw, cleanup)?;
            let repr = lw.reg.repr_form(heap, &lw.settle(&t.ty))?;
            heap.push_root(repr);
            forms::unwind_protect_form(heap, protected, cleanup, repr)
        }
        Node::Let { binds, body } => {
            let mut lowered = Vec::with_capacity(binds.len());
            for (name, t) in binds {
                lowered.push((*name, lw.settle(&t.ty), lower_rooted(heap, lw, t)?));
            }
            let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
            lw.reg.let_form(heap, &lowered, &body)
        }
        Node::If { cond, then, els } => {
            let cond = lower_rooted(heap, lw, cond)?;
            let then = lower_rooted(heap, lw, then)?;
            let els = lower_rooted(heap, lw, els)?;
            forms::if_form(heap, cond, then, els)
        }
        Node::Match { scrut, arms } => {
            let scrut_ty = lw.settle(&scrut.ty);
            let scrut = lower_rooted(heap, lw, scrut)?;
            let mut lowered = Vec::with_capacity(arms.len());
            for (pat, body) in arms {
                let pat = lower_pattern(heap, lw, pat)?;
                heap.push_root(pat);
                let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
                lowered.push((pat, body));
            }
            lw.reg.match_form(heap, scrut, &scrut_ty, &lowered)
        }
        Node::AsSexpr { value } => {
            let ty = lw.settle(&value.ty);
            let form = lower_rooted(heap, lw, value)?;
            match encoding(lw.reg, &ty) {
                Some(Encoding::Itself) => Ok(form),
                Some(Encoding::BoxOption) => forms::box_option_form(heap, &ty, form),
                Some(Encoding::Variant(variant)) => {
                    let sexpr = Path::builtin(typelisp_mem::wk::SEXPR);
                    lw.reg.construct_form(heap, &sexpr, &[], variant, false, &[ty], &[form])
                }
                // A generic body's type parameter, specialized to a type that
                // has no encoding.
                None => Err(no_encoding(&ty)),
            }
        }
    }
}

/// `pat`'s core form, unrooted.
fn lower_pattern(heap: &mut Heap, lw: &mut Lowering, pat: &Pat) -> Result<Value, Error> {
    match pat {
        Pat::Wild => core::tagged(heap, wk::PAT_WILD, &[]),
        Pat::Bind(name) => core::tagged(heap, wk::PAT_BIND, &[Value::Symbol(*name)]),
        Pat::Ctor { owner, targs, variant, downcast, field_tys, subs } => {
            let targs: Vec<Type> = targs.iter().map(|t| lw.settle(t)).collect();
            let field_tys: Vec<Type> = field_tys.iter().map(|t| lw.settle(t)).collect();
            let mut lowered = Vec::with_capacity(subs.len());
            for sub in subs {
                let one = lower_pattern(heap, lw, sub)?;
                heap.push_root(one);
                lowered.push(one);
            }
            // A niched `Option<T>`'s two shapes, as `Construct` builds them.
            if let Some(payload) = option_niche(lw, owner, &targs) {
                return match lowered[..] {
                    [sub] => lw.reg.pat_some_form(heap, &payload, sub),
                    _ => core::tagged(heap, wk::PAT_EMPTY, &[]),
                };
            }
            lw.reg.pat_ctor_form(heap, owner, &targs, *variant, *downcast, &field_tys, &lowered)
        }
        Pat::Lit { tag, value } => {
            let lit = core::tagged(heap, *tag, &[*value])?;
            heap.push_root(lit);
            let form = core::tagged(heap, wk::PAT_LIT, &[lit]);
            heap.pop_root();
            form
        }
        Pat::Guard { name, test } => {
            let test = lower_rooted(heap, lw, test)?;
            core::tagged(heap, wk::PAT_GUARD, &[Value::Symbol(*name), test])
        }
    }
}

/// When `owner<targs>` (settled) is an `Option` represented by a niche, the
/// representation of its payload.
fn option_niche(lw: &Lowering, owner: &Path, targs: &[Type]) -> Option<Repr> {
    if !path_is_builtin(owner, typelisp_mem::wk::OPTION) {
        return None;
    }
    lw.reg.repr(&Type::Named(*owner, targs.to_vec())).niche_payload().cloned()
}
