//! Every type's methods: `(defmethod f<T> ((x T)) ...)`, whose receiver is
//! one of the method's own type parameters.
//!
//! A receiver with a method of the name of its own answers to that one
//! instead ([`super::expr::Cx::receiver_method`]). A generic body's receiver
//! has its type only once the body is specialized, so the call is checked
//! against every type's signature, and chosen again here, when it is
//! lowered with the receiver's type settled: a type parameter `T` that
//! becomes `int` calls `int`'s own method, when it has one, seen from the
//! module the body was written in. That method has to take and give what
//! every type's does at the call's types.

use crate::types::{Path, Type};
use crate::Error;
use typelisp_mem::SymRef;

use super::bounds::settled_meets;
use super::dyn_trait::{dyn_method, DynMethod};
use super::generic::method_specialization;
use super::lower::Lowering;
use super::registry::{FnDef, MethodDef};
use super::resolve::receiver_method;
use super::subst::Subst;
use super::unify::Holes;

/// What a call of every type's method calls, once its receiver's type is
/// settled, when that type has a method of the name of its own.
pub(crate) enum Chosen<'r> {
    /// The method of `owner` whose code is named `code`, at `targs`.
    Method { owner: Path, code: SymRef, def: &'r MethodDef, targs: Vec<Type> },
    /// The trait object's method in the slot `slot` of `trait_path`'s table.
    Dyn { trait_path: Path, slot: usize },
}

/// The receiver's own method a call of every type's method `method`, at the
/// settled type arguments `targs`, calls instead — `None` when the
/// receiver's type has no method of the name and every type's is called.
pub(crate) fn chosen<'r>(lw: &Lowering<'r>, method: SymRef, targs: &[Type]) -> Result<Option<Chosen<'r>>, Error> {
    let every = lw.reg.every_type_method(method).expect("a call of every type's method is of a defined one");
    let at = Subst::positional(&every.sig.type_params, targs);
    let params: Vec<Type> = every.sig.params.iter().map(|p| at.apply(p)).collect();
    let ret = at.apply(&every.sig.ret);
    let recv = &params[0];
    if let Type::Dyn(trait_path, pins) = recv {
        let Some(DynMethod { slot, params: own_params, ret: own_ret }) = dyn_method(lw.reg, trait_path, recv, pins, method)? else {
            return Ok(None);
        };
        no_defaults(method, recv, &every.sig, None)?;
        if every.sig.rest.is_some() || own_params != params[1..] || own_ret != ret {
            let own = Type::Fn(std::iter::once(recv.clone()).chain(own_params).collect(), None, Box::new(own_ret));
            return Err(disagrees(method, recv, &own, &every.sig, &at));
        }
        return Ok(Some(Chosen::Dyn { trait_path: *trait_path, slot }));
    }
    let Some((owner, def)) = receiver_method(lw.reg, lw.here, recv, method)? else { return Ok(None) };
    no_defaults(method, recv, &every.sig, Some(&def.sig))?;
    let mut holes = Holes::new();
    let own = holes.instantiate(&def.sig.type_params);
    let rest = match (&def.sig.rest, &every.sig.rest) {
        (None, None) => true,
        (Some(a), Some(b)) => holes.unify(&own.apply(a), &at.apply(b)).is_ok(),
        _ => false,
    };
    let agrees = rest
        && def.sig.params.len() == params.len()
        && def.sig.params.iter().zip(&params).all(|(p, a)| holes.unify(&own.apply(p), a).is_ok())
        && holes.unify(&own.apply(&def.sig.ret), &ret).is_ok();
    let own_targs: Vec<Type> = def.sig.type_params.iter().map(|p| holes.resolve(&own.apply(&Type::Param(*p)))).collect();
    let decided = own_targs.iter().all(|t| {
        let mut open = false;
        super::unify::walk(t, &mut |t| open |= matches!(t, Type::Infer(_)));
        !open
    });
    if !agrees || !decided {
        let shown = Type::Fn(def.sig.params.clone(), def.sig.rest.clone().map(Box::new), Box::new(def.sig.ret.clone()));
        return Err(disagrees(method, recv, &shown, &every.sig, &at));
    }
    let at_own = Subst::positional(&def.sig.type_params, &own_targs);
    for (p, bounds) in &def.sig.bounds {
        for b in bounds {
            let ty = at_own.apply(&Type::Param(*p));
            if !settled_meets(lw.reg, &ty, &b.at(&at_own)) {
                return Err(Error::TypeError(format!(
                    "`{}` on a `{}`: the receiver's own `{}` is called here, and it needs `{}` to implement `{}`, which it does not",
                    method, recv, method, ty, b.trait_path
                )));
            }
        }
    }
    let code = match &def.inst_args {
        Some(inst) => method_specialization(&owner, method, inst),
        None => method,
    };
    Ok(Some(Chosen::Method { owner, code, def, targs: own_targs }))
}

/// Refuse to call the receiver's own method in place of every type's when
/// either has `&optional` or `&key` parameters: a call that leaves one out
/// carries the default every type's method wrote.
fn no_defaults(method: SymRef, recv: &Type, every: &FnDef, own: Option<&FnDef>) -> Result<(), Error> {
    let has = |s: &FnDef| !s.optionals.is_empty() || !s.keys.is_empty();
    if has(every) || own.is_some_and(has) {
        return Err(Error::TypeError(format!(
            "`{}` on a `{}`: the receiver's own `{}` would be called here in place of every type's, \
             and one of them has `&optional` or `&key` parameters, whose defaults a call carries from \
             every type's; write the parameters as required ones",
            method, recv, method
        )));
    }
    Ok(())
}

/// The error for a receiver whose own method `method`, of the type `own`,
/// does not take and give what every type's, `every` at `at`, does.
fn disagrees(method: SymRef, recv: &Type, own: &Type, every: &FnDef, at: &Subst) -> Error {
    let want = Type::Fn(every.params.iter().map(|p| at.apply(p)).collect(), every.rest.as_ref().map(|r| Box::new(at.apply(r))), Box::new(at.apply(&every.ret)));
    Error::TypeError(format!(
        "`{}` on a `{}`: the receiver's own `{}`, of type `{}`, is called here in place of every type's, \
         and it does not take and give what every type's does here, `{}`",
        method, recv, method, own, want
    ))
}
