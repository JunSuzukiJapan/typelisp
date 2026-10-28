//! A C callback's identity: which typelisp function C is handed, under which C
//! signature.
//!
//! A `defffi` parameter declared `(fn (T...) R)` takes a function C will call
//! back. C holds nothing but a code address, so each (function, signature)
//! pair needs an entry of its own that converts C's arguments, runs the
//! function and converts its answer — made by the JIT when a session first
//! asks for it, or emitted into an executable by `compile-file`. The pair is
//! what every one of those places agrees on: the checker writes it into the
//! call site, the interpreter and the runtime table look it up, and the
//! backend reads the C signature off the function type.
//!
//! The function is always a top-level one. A capture-free `lambda` or local
//! function is lifted to one by the checker before a callback is made of it, so
//! the entry never needs an environment — C would have nowhere to pass one.

use typelisp_mem::TypeId;

use crate::types::{Path, Type};

/// The internal builtin a callback argument lowers to:
/// `(" ffi-callback-address" (quote PATH) (type-value TYPE))` answers the
/// entry's address as a raw word.
///
/// The leading space is deliberate. The reader never produces a symbol that
/// starts with one, so no program can call this by name or define a function
/// that a compiled call site would mistake for it.
pub const ADDRESS_BUILTIN: u32 = typelisp_mem::wk::F_FFI_CALLBACK_ADDRESS;

/// One callback entry: the top-level function C is handed, and the function
/// type C calls it at — a `(fn (T...) R)` with no `&rest`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallbackSig {
    pub function: Path,
    pub ty: TypeId,
}

impl CallbackSig {
    /// The callback a `(fn (T...) R)` parameter type declares, for
    /// `function`. `None` when `ty` is not a fixed-arity function type.
    pub fn from_fn_type(function: &Path, ty: &Type) -> Option<CallbackSig> {
        match ty {
            Type::Fn(_, None, _) => Some(CallbackSig { function: *function, ty: crate::type_key::type_id(ty) }),
            _ => None,
        }
    }

    /// The parameter and return types C calls the function with.
    pub fn signature(&self) -> (Vec<TypeId>, TypeId) {
        match typelisp_mem::type_shape(self.ty) {
            typelisp_mem::TypeShape::Fn(params, None, ret) => (params, ret),
            _ => panic!("a callback's type `{}` is not a fixed-arity function type", &*typelisp_mem::type_name(self.ty)),
        }
    }
}
