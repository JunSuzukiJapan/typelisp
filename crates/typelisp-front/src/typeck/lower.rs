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
            request_print_object(lw, &lw.settle(&t.ty));
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
            request_print_object(lw, &ret);
            // A blanket `impl`'s method is specialized as its type's.
            let owner = match lw.reg.blanket_trait(owner) {
                Some(_) => crate::types::nominal_path(&lw.settle(&targs[0])).expect("a blanket `impl` covers only a type with a name"),
                None => *owner,
            };
            lw.reg.assoc_form(heap, &owner, method, *instance, &ret, &lowered)
        }
        Node::TraitCall { trait_path, method, args } => {
            // The receiver's type is the specialization's.
            let recv = lw.settle(&args[0].ty);
            // A trait object, which meets the bound by its trait: the call is
            // through its table.
            if let Type::Dyn(dyn_trait, _) = &recv {
                let slots = super::dyn_trait::vtable(lw.reg, dyn_trait)?;
                let slot = slots.iter().position(|(m, _)| m == method).expect("a trait object meets a bound of its own trait or a supertrait, whose methods are in its table");
                let mut lowered = Vec::with_capacity(args.len());
                for a in args {
                    lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
                }
                let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
                let targets = super::dyn_trait::targets(lw.reg, dyn_trait, *method);
                return lw.reg.dyn_call_form(heap, dyn_trait, *method, slot, &targets, &lowered);
            }
            let (owner, type_args) = match &recv {
                Type::Named(path, type_args) => (*path, type_args.clone()),
                other => match prim_type_path(other) {
                    Some(path) => (path, Vec::new()),
                    None => unimplemented!("typeck: `{}::{}` on a `{}`, which names no type", trait_path, method, other),
                },
            };
            let Some(def) = lw.reg.method_at(&owner, *method, &type_args) else {
                // No `impl` of the type gave it: the blanket `impl`'s,
                // specialized at the type.
                let Some(blanket) = lw.reg.blanket_with_method(*method).map(|b| b.owner) else {
                    unreachable!("`{}` meets the bound on `{}`, so it has `{}`", recv, trait_path, method)
                };
                let code = lw.requests.ask_method(&blanket, *method, vec![recv.clone()]);
                let mut lowered = Vec::with_capacity(args.len());
                for a in args {
                    lowered.push((lw.settle(&a.ty), lower_rooted(heap, lw, a)?));
                }
                let lowered: Vec<Arg> = lowered.iter().map(|(ty, form)| Arg { ty, form: *form }).collect();
                let ret = lw.settle(&t.ty);
                return lw.reg.assoc_form(heap, &owner, code, true, &ret, &lowered);
            };
            // An `impl`'s methods have the type's parameters, in order; one
            // for one instantiation has code under that instantiation's name.
            let method = if let Some(inst) = &def.inst_args {
                super::generic::method_specialization(&owner, *method, inst)
            } else if def.sig.builtin || def.sig.type_params.is_empty() {
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
            request_print_object(lw, &Type::Named(*owner, targs.clone()));
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
        Node::TypeValue { ty } => {
            let form = core::type_form(heap, &lw.settle(ty), &[])?;
            heap.push_root(form);
            core::tagged(heap, wk::TYPE_VALUE, &[form])
        }
        Node::UntagInt { value } => {
            let value = lower_rooted(heap, lw, value)?;
            crate::check::analyze::untag_int_form(heap, value)
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
        Node::Labels { defs, body, lifted } => {
            let lowered = defs.iter().map(|d| lower_label(heap, lw, d)).collect::<Result<Vec<_>, _>>()?;
            let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
            for l in lifted {
                lift_label(heap, lw, defs, l)?;
            }
            lw.reg.labels_core_form(heap, &lowered, &body)
        }
        Node::FormatCalls { control, targets } => {
            targets.iter().for_each(|t| lw.requests.format_call(t.clone()));
            lower(heap, lw, control)
        }
        Node::Callback { target, fn_ty, lifted } => {
            if let Some(lambda) = lifted {
                if !lw.subst.is_empty() {
                    unimplemented!("typeck: a `lambda` passed to C as a callback inside a generic body")
                }
                let Node::Lambda { params, ret, body } = &lambda.node else {
                    unreachable!("a lifted callback is a `lambda`")
                };
                let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
                let form = lw.reg.defun_form(heap, target, params, ret, &body)?;
                heap.push_root(form);
                lw.requests.lift(form);
            }
            let fn_ty = lw.settle(fn_ty);
            let sig = crate::ffi_callback::CallbackSig::from_fn_type(target, &fn_ty).expect("a callback's type is a function type");
            lw.requests.callback(sig);
            // `(call ADDRESS (quote FUNCTION) (type-value TYPE))`: the
            // entry's address, as a raw word.
            let function = Typed { node: Node::Quote { datum: core::definition_form(target) }, ty: crate::check::analyze::sexpr_ty(), loc: None };
            let ty = Typed { node: Node::TypeValue { ty: fn_ty }, ty: Type::Int, loc: None };
            let call = Node::Call { path: Path::internal_builtin(crate::ffi_callback::ADDRESS_BUILTIN), targs: Vec::new(), args: vec![function, ty] };
            lower(heap, lw, &Typed { node: call, ty: Type::Ptr, loc: None })
        }
        Node::MethodRef { owner, method, targs, params, ret } => {
            let method = if targs.is_empty() {
                *method
            } else {
                let targs = targs.iter().map(|t| lw.settle(t)).collect();
                lw.requests.ask_method(owner, *method, targs)
            };
            let params: Vec<Type> = params.iter().map(|ty| lw.settle(ty)).collect();
            let ret = lw.settle(ret);
            // Called through the value, a builtin still builds its box.
            request_print_object(lw, &ret);
            lw.reg.methodref_form(heap, owner, method, &params, &ret)
        }
        Node::FnRef { path, targs, params } => {
            let path = if targs.is_empty() {
                *path
            } else {
                let targs = targs.iter().map(|t| lw.settle(t)).collect();
                lw.requests.ask_fn(path, targs)
            };
            let path = &path;
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
        Node::Step { inner } => {
            let inner = lower_rooted(heap, lw, inner)?;
            crate::check::core::tagged(heap, wk::STEP, &[inner])
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
                // An arm with `:or` alternatives is an arm for each, with
                // the same condition and body.
                let (alts, test) = match pat {
                    Pat::When { pat, test } => (pat.as_ref(), Some(test.as_ref())),
                    other => (other, None),
                };
                let alts: Vec<&Pat> = match alts {
                    Pat::Or(alts) => alts.iter().collect(),
                    one => vec![one],
                };
                for alt in alts {
                    let mut pat = lower_pattern(heap, lw, alt)?;
                    if let Some(test) = test {
                        heap.push_root(pat);
                        let test = lower_rooted(heap, lw, test)?;
                        pat = core::tagged(heap, wk::PAT_WHEN, &[pat, test])?;
                    }
                    heap.push_root(pat);
                    let body = body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
                    lowered.push((pat, body));
                }
            }
            lw.reg.match_form(heap, scrut, &scrut_ty, &lowered)
        }
        Node::DynValue { value } => {
            let value = lower_rooted(heap, lw, value)?;
            crate::check::forms::dyn_value_form(heap, value)
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
        Pat::TypeTest { ty, sub } => {
            let ty = lw.settle(ty);
            let path = crate::types::nominal_path(&ty).ok_or_else(|| Error::TypeError(format!("(the `{}` pattern): not a named type", ty)))?;
            // The whole instantiation: `(the gen<i32> x)` takes no `gen<string>`.
            let key = core::type_form(heap, &ty, &[])?;
            heap.push_root(key);
            let sub = lower_pattern(heap, lw, sub)?;
            heap.push_root(sub);
            let form = core::tagged(heap, wk::PAT_TYPETEST, &[core::definition_form(&path), key, sub]);
            heap.pop_root();
            heap.pop_root();
            form
        }
        Pat::Or(_) | Pat::When { .. } => unreachable!("`:or` and `:when` are an arm's whole pattern, lowered with the arm"),
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

/// One function of a `labels`, lowered.
fn lower_label(heap: &mut Heap, lw: &mut Lowering, d: &super::lambda::Label) -> Result<LoweredLabel, Error> {
    let params: Vec<(SymRef, Type)> = d.params.iter().map(|(n, ty)| (*n, lw.settle(ty))).collect();
    let ret = lw.settle(&d.ret);
    let body = d.body.iter().map(|t| lower_rooted(heap, lw, t)).collect::<Result<Vec<_>, _>>()?;
    Ok(LoweredLabel { name: d.name, params, ret, body })
}

/// `(defun PATH ((" a0" T0)...) R (labels (DEF...) (G " a0"...)))`, the
/// top-level function C is handed for the `labels` function `l.name` of
/// `defs`, `DEF...` being those it reaches. The parameters' names are none a
/// source can write, so none shadows a function of the copied block.
fn lift_label(heap: &mut Heap, lw: &mut Lowering, defs: &[super::lambda::Label], l: &super::lambda::LiftedLabel) -> Result<(), Error> {
    if !lw.subst.is_empty() {
        unimplemented!("typeck: a `labels` function passed to C as a callback inside a generic body")
    }
    let kept = defs.iter().filter(|d| l.kept.contains(&d.name)).map(|d| lower_label(heap, lw, d)).collect::<Result<Vec<_>, _>>()?;
    let g = defs.iter().find(|d| d.name == l.name).expect("a lifted function is the block's");
    let params: Vec<(SymRef, Type)> = g.params.iter().enumerate().map(|(i, (_, t))| (typelisp_mem::symbols::intern(&format!(" a{}", i)), lw.settle(t))).collect();
    let ret = lw.settle(&g.ret);
    let callee = forms::var_form(heap, g.name)?;
    heap.push_root(callee);
    let mut args = Vec::with_capacity(params.len());
    for (n, _) in &params {
        let a = forms::var_form(heap, *n)?;
        heap.push_root(a);
        args.push(a);
    }
    let args: Vec<Arg> = params.iter().zip(&args).map(|((_, ty), form)| Arg { ty, form: *form }).collect();
    let call = lw.reg.apply_form(heap, callee, &ret, &args)?;
    heap.push_root(call);
    let block = lw.reg.labels_core_form(heap, &kept, &[call])?;
    heap.push_root(block);
    let form = lw.reg.defun_form(heap, &l.path, &params, &ret, &[block])?;
    heap.push_root(form);
    lw.requests.lift(form);
    Ok(())
}

/// Asks for the `print-object` specialization of `ty`, a box a node makes,
/// when `ty` is an instantiation of a generic type whose `print-object`
/// applies to it.
///
/// Every other specialization is asked for because some call names it. The
/// printer never does: it picks the method at run time, off the value's
/// type, so without this a generic value would print the built-in way. The
/// place to ask is where a box of the type is made, the one place that has
/// the instantiation.
fn request_print_object(lw: &mut Lowering, ty: &Type) {
    let Type::Named(owner, args) = ty else { return };
    if args.is_empty() {
        return;
    }
    let method = typelisp_mem::symbols::member_symbol("print-object");
    if lw.reg.template(&super::generic::Base::Method(*owner, method)).is_none() {
        return;
    }
    // Not a call anyone wrote: an instantiation the `impl`'s `:where` does
    // not take prints the built-in way, which is no error.
    let bound = crate::check::registry::TraitBound { trait_path: Path::in_ns(crate::types::std_module("fmt"), "print-object"), assoc: Default::default() };
    if super::bounds::settled_meets(lw.reg, ty, &bound) {
        lw.requests.ask_method(owner, method, args.clone());
    }
}
