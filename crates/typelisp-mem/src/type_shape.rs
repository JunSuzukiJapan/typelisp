//! What the heap's type table registers: a type's shape.
//!
//! A shape names its definition by the module it is in and its symbol there —
//! the two halves of a definition's path — and its type arguments by their
//! own ids, so equal types have equal shapes and one id. Reading a type's
//! arguments is reading its shape; nothing takes a spelling apart.

use crate::names::ConcreteNames;
use crate::symbols::{ns_path, NsId, SymRef};
use crate::value::TypeId;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TypeShape {
    /// A struct, an enum, a built-in type: the definition and its arguments.
    Named(NsId, SymRef, Vec<TypeId>),
    /// A trait object: the trait and the associated types pinned on it.
    Dyn(NsId, SymRef, Vec<TypeId>),
    /// A function: its parameters, its `&rest` parameter, its result.
    Fn(Vec<TypeId>, Option<TypeId>, TypeId),
    /// A pointer to a C value of the type.
    Ptr(TypeId),
    /// A C struct.
    CStruct(NsId, SymRef),
    Unit,
    Never,
}

impl TypeShape {
    /// The named type `name` of the module `ns`, with no arguments or with
    /// `args`.
    pub fn named(ns: NsId, name: &str, args: &[TypeId]) -> TypeShape {
        TypeShape::Named(ns, crate::symbols::intern_exact(ns, name), args.to_vec())
    }
}

/// How the type `shape` is displayed, given its arguments' displays by
/// `name_of` — the spelling [`ConcreteNames`] gives the same type.
pub(crate) fn display(shape: &TypeShape, name_of: impl Fn(TypeId) -> String) -> String {
    let names = |ids: &[TypeId]| ids.iter().map(|id| name_of(*id)).collect::<Vec<_>>();
    match shape {
        TypeShape::Named(ns, sym, args) => ConcreteNames::instantiated_type(&definition(*ns, *sym), &names(args)),
        TypeShape::Dyn(ns, sym, pins) => ConcreteNames::dyn_type(&definition(*ns, *sym), &names(pins)),
        TypeShape::Fn(params, rest, ret) => {
            let rest = rest.map(&name_of);
            ConcreteNames::fn_type(&names(params), rest.as_ref(), &name_of(*ret))
        }
        TypeShape::Ptr(pointee) => ConcreteNames::ptr_type(&name_of(*pointee)),
        TypeShape::CStruct(ns, sym) => definition(*ns, *sym),
        TypeShape::Unit => "()".to_string(),
        TypeShape::Never => "!".to_string(),
    }
}

/// A definition's path from the root, as a display spells it: the module's
/// segments and the name, joined by `::`.
fn definition(ns: NsId, sym: SymRef) -> String {
    let mut segs = ns_path(ns);
    segs.push(sym.name().to_string());
    segs.join("::")
}

/// A type written out in Rust, as a constant: what the runtime gives a value
/// it builds with no checker in hand (a builtin's `Result<i32, NetError>`).
/// `tests/type_identity_guard_test.rs` holds each one equal to the type the
/// registry declares.
#[derive(Debug)]
pub enum TypeSpec {
    /// The named type `name` of the module, with these arguments.
    Named(NsId, &'static str, &'static [TypeSpec]),
    /// A function of these parameters, with no `&rest`, returning the last.
    Fn(&'static [TypeSpec], &'static TypeSpec),
    Unit,
}

/// The types the runtime's [`TypeSpec`]s are written with.
pub mod spec {
    use super::TypeSpec::{self, Named};
    use crate::symbols::NsId;

    pub const INT: TypeSpec = Named(NsId::ROOT, "int", &[]);
    pub const I32: TypeSpec = Named(NsId::ROOT, "i32", &[]);
    pub const F64: TypeSpec = Named(NsId::ROOT, "f64", &[]);
    pub const BOOL: TypeSpec = Named(NsId::ROOT, "bool", &[]);
    pub const CHAR: TypeSpec = Named(NsId::ROOT, "char", &[]);
    pub const STRING: TypeSpec = Named(NsId::ROOT, "string", &[]);
    pub const SEXPR: TypeSpec = Named(NsId::ROOT, "sexpr", &[]);
    pub const UNIT: TypeSpec = TypeSpec::Unit;
}

impl crate::Heap {
    /// Intern the type `spec` writes.
    pub fn intern_spec(&self, spec: &TypeSpec) -> TypeId {
        match spec {
            TypeSpec::Named(ns, name, args) => {
                let args: Vec<TypeId> = args.iter().map(|a| self.intern_spec(a)).collect();
                self.intern_type(TypeShape::named(*ns, name, &args))
            }
            TypeSpec::Fn(params, ret) => {
                let params: Vec<TypeId> = params.iter().map(|p| self.intern_spec(p)).collect();
                let ret = self.intern_spec(ret);
                self.intern_type(TypeShape::Fn(params, None, ret))
            }
            TypeSpec::Unit => self.intern_type(TypeShape::Unit),
        }
    }
}
