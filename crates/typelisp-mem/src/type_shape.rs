//! The type table: every type a value of this process can have, registered
//! by its shape.
//!
//! A shape names its definition by the module it is in and its symbol there —
//! the two halves of a definition's path — and its type arguments by their
//! own ids, so equal types have equal shapes and one id. Reading a type's
//! arguments is reading its shape; nothing takes a spelling apart.
//!
//! The table belongs to the process, like the symbol table its shapes point
//! into, not to a heap: an id means the same type to every heap, every thread
//! and every table that holds one, including the ones filled before any heap
//! exists (an interpreter's built-in types, an executable's startup).

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::RwLock;

use crate::names::ConcreteNames;
use crate::symbols::{ns_path, NsId, StdModule, SymRef};
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
    /// The owning type's Nth parameter, in a field's template: what a field
    /// declared `Option<T>` in `gen<T>` is before an instantiation of `gen`
    /// says what `T` is. No value has it; [`crate::Heap::instantiate`]
    /// replaces it.
    Param(u32),
}

impl TypeShape {
    /// The named type `name` of the module `ns`, with no arguments or with
    /// `args`.
    pub fn named(ns: NsId, name: &str, args: &[TypeId]) -> TypeShape {
        TypeShape::Named(ns, crate::symbols::intern_in(ns, name), args.to_vec())
    }

    /// The definition a named type is an instantiation of — what a table of
    /// facts about the definition (its variants' names, its fields') is keyed
    /// by.
    pub fn named_definition(&self) -> Option<(NsId, SymRef)> {
        match self {
            TypeShape::Named(ns, sym, _) => Some((*ns, *sym)),
            _ => None,
        }
    }
}

/// How the type `shape` is displayed, given its arguments' displays by
/// `name_of` and its definition's by `definition_of` — the spelling
/// [`ConcreteNames`] gives the same type.
pub(crate) fn display(
    shape: &TypeShape,
    name_of: impl Fn(TypeId) -> String,
    definition_of: impl Fn(NsId, SymRef) -> String,
) -> String {
    let names = |ids: &[TypeId]| ids.iter().map(|id| name_of(*id)).collect::<Vec<_>>();
    match shape {
        TypeShape::Named(ns, sym, args) => ConcreteNames::instantiated_type(&definition_of(*ns, *sym), &names(args)),
        TypeShape::Dyn(ns, sym, pins) => ConcreteNames::dyn_type(&definition_of(*ns, *sym), &names(pins)),
        TypeShape::Fn(params, rest, ret) => {
            let rest = rest.map(&name_of);
            ConcreteNames::fn_type(&names(params), rest.as_ref(), &name_of(*ret))
        }
        TypeShape::Ptr(pointee) => ConcreteNames::ptr_type(&name_of(*pointee)),
        TypeShape::CStruct(ns, sym) => definition_of(*ns, *sym),
        TypeShape::Unit => "()".to_string(),
        TypeShape::Never => "!".to_string(),
        TypeShape::Param(n) => ConcreteNames::template_param(*n as usize),
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
    Named(StdModule, &'static str, &'static [TypeSpec]),
    /// A function of these parameters, with no `&rest`, returning the last.
    Fn(&'static [TypeSpec], &'static TypeSpec),
    Unit,
}

/// The types the runtime's [`TypeSpec`]s are written with.
pub mod spec {
    use super::TypeSpec::{self, Named};
    use crate::symbols::StdModule;

    pub const INT: TypeSpec = Named(StdModule::PRIMITIVE, "int", &[]);
    pub const I32: TypeSpec = Named(StdModule::PRIMITIVE, "i32", &[]);
    pub const F64: TypeSpec = Named(StdModule::PRIMITIVE, "f64", &[]);
    pub const BOOL: TypeSpec = Named(StdModule::PRIMITIVE, "bool", &[]);
    pub const CHAR: TypeSpec = Named(StdModule::PRIMITIVE, "char", &[]);
    pub const STRING: TypeSpec = Named(StdModule::PRIMITIVE, "string", &[]);
    pub const SEXPR: TypeSpec = Named(StdModule::SEXPR, "Sexpr", &[]);
    pub const UNIT: TypeSpec = TypeSpec::Unit;
}

/// The interned types: each one's shape and display, and the reverse map.
/// The first [`crate::BUILTIN_TYPES`] rows are interned when the table is
/// made, in order, so their ids are the `TypeId` associated constants.
#[derive(Default)]
struct TypeTable {
    types: Vec<(TypeShape, String)>,
    ids: HashMap<TypeShape, u32>,
}

impl TypeTable {
    fn intern(&mut self, shape: TypeShape) -> TypeId {
        if let Some(&id) = self.ids.get(&shape) {
            return TypeId(id);
        }
        let name = display(&shape, |arg| self.types[arg.0 as usize].1.clone(), definition);
        let id = self.types.len() as u32;
        self.ids.insert(shape.clone(), id);
        self.types.push((shape, name));
        TypeId(id)
    }
}

static TYPES: OnceLock<RwLock<TypeTable>> = OnceLock::new();

/// `parking_lot`'s lock, because [`type_name`] hands out a guard narrowed to
/// one entry. Every read is `read_recursive`: one thread can hold two such
/// guards at once, and a plain `read` would queue behind a waiting writer.
fn table() -> &'static RwLock<TypeTable> {
    TYPES.get_or_init(|| {
        let mut table = TypeTable::default();
        for (ns, name, args) in crate::value::BUILTIN_TYPES {
            table.intern(TypeShape::named(ns.ns(), name, args));
        }
        RwLock::new(table)
    })
}

/// Intern a type by its shape, returning the [`TypeId`] every instance of
/// that type carries. Permanent, like a symbol: equal shapes always share
/// one id, so "same type?" is `==` on the id. The display is built here,
/// once, from the arguments' displays.
pub fn intern_type(shape: TypeShape) -> TypeId {
    table().write().intern(shape)
}

/// The id `shape` interns to, if it has ever been interned — for asking "is
/// this value of that type?" without minting an id for a type no value has.
pub fn find_type(shape: &TypeShape) -> Option<TypeId> {
    table().read_recursive().ids.get(shape).map(|&id| TypeId(id))
}

/// The shape `id` was interned from.
pub fn type_shape(id: TypeId) -> TypeShape {
    table().read_recursive().types[id.0 as usize].0.clone()
}

/// The display of an interned type — for printing, `Debug`, and error
/// messages. Not for comparison: compare the ids. A guard rather than a
/// `&str` because the entry lives behind the table's lock.
pub fn type_name(id: TypeId) -> parking_lot::MappedRwLockReadGuard<'static, str> {
    parking_lot::RwLockReadGuard::map(table().read_recursive(), |t| t.types[id.0 as usize].1.as_str())
}

impl TypeId {
    /// The id's place in the table: what the island's `(type-ref N)` writes,
    /// inside the process that compiled it.
    pub fn index(self) -> u32 {
        self.0
    }

    /// The id at place `index`, or `None` when the table has no type there.
    pub fn from_index(index: u32) -> Option<TypeId> {
        ((index as usize) < table().read_recursive().types.len()).then_some(TypeId(index))
    }

    /// The id as compiled code holds it: a tagged fixnum, so that a frame
    /// slot or an argument holding one is a word the collector can read.
    pub fn word(self) -> i64 {
        crate::encode(crate::Value::Int(i64::from(self.0)))
    }

    /// The id a [`Self::word`] carried, or `None` when the word is not one.
    pub fn from_word(w: i64) -> Option<TypeId> {
        if w & crate::tagged::FIXNUM_MASK != crate::tagged::FIXNUM_TAG {
            return None;
        }
        Self::from_index(u32::try_from(w >> crate::tagged::FIXNUM_SHIFT).ok()?)
    }
}

/// The type `template` is with each [`TypeShape::Param`] N replaced by
/// `args[N]` — a field's declared type in the instantiation whose arguments
/// are `args`.
pub fn instantiate(template: TypeId, args: &[TypeId]) -> TypeId {
    let each = |ids: &[TypeId]| ids.iter().map(|t| instantiate(*t, args)).collect::<Vec<_>>();
    let shape = match type_shape(template) {
        TypeShape::Param(n) => return args[n as usize],
        TypeShape::Named(ns, sym, a) => TypeShape::Named(ns, sym, each(&a)),
        TypeShape::Dyn(ns, sym, pins) => TypeShape::Dyn(ns, sym, each(&pins)),
        TypeShape::Fn(params, rest, ret) => TypeShape::Fn(each(&params), rest.map(|r| instantiate(r, args)), instantiate(ret, args)),
        TypeShape::Ptr(pointee) => TypeShape::Ptr(instantiate(pointee, args)),
        TypeShape::CStruct(..) | TypeShape::Unit | TypeShape::Never => return template,
    };
    intern_type(shape)
}

/// Intern the type `spec` writes.
pub fn intern_spec(spec: &TypeSpec) -> TypeId {
    match spec {
        TypeSpec::Named(ns, name, args) => {
            let args: Vec<TypeId> = args.iter().map(intern_spec).collect();
            intern_type(TypeShape::named(ns.ns(), name, &args))
        }
        TypeSpec::Fn(params, ret) => {
            let params: Vec<TypeId> = params.iter().map(intern_spec).collect();
            intern_type(TypeShape::Fn(params, None, intern_spec(ret)))
        }
        TypeSpec::Unit => intern_type(TypeShape::Unit),
    }
}
