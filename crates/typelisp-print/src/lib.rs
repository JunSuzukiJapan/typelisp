//! The Common Lisp printer: `format`'s directive engine ([`format`], CLHS
//! 22.3) and the pretty printer ([`pprint`], CLHS 22.2).
//!
//! # Why this is a crate and not a module
//!
//! Everything here used to live in the interpreter (`typelisp::eval::format`/
//! `pprint`), which meant a compiled function that called `format`/`print`/
//! `println`/`pprint` could not be compiled at all: the lowering would have
//! had to name a shim, and the shim would have had to reach into a `typelisp`
//! that depends on the runtime rather than the other way round.
//!
//! Moving it down solves that, and the *crate* boundary (rather than another
//! module of `typelisp-rt`) is what keeps it from being a tax on every
//! program: `compile-file` links `typelisp-rt`'s `staticlib`, and a linker
//! takes archive members whole. One crate is one set of members — pulled in
//! when something calls into the printer, absent otherwise.
//!
//! # What it does not know
//!
//! Rendering a value needs two things this layer cannot have: the *names* of
//! an enum's variants, and whether a type has its own `print-object` method.
//! Both are properties of a program, not of the heap. They arrive through
//! [`PrintEnv`], which the interpreter implements directly and an AOT-linked
//! executable implements from tables its startup code registers.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use typelisp_mem::prelude::*;

use typelisp_mem::{Heap, TypeId, Value};

pub mod aot;
pub mod format;
pub mod pprint;
pub mod runtime;
pub mod shared;
pub mod shim;

/// What the printer needs to know about the *program* whose values it is
/// printing.
///
/// Two questions, both unanswerable from the heap alone:
///
/// - an enum box stores its type and variant *index*, never the variant's
///   name — `(Some 3)` prints as `(Some 3)` only because someone can map
///   `option`'s variant 0 back to `"Some"`;
/// - a type may implement the `print-object` trait, in which case its own
///   method decides the text, and finding that method means knowing the
///   program's method table.
///
/// The interpreter answers both directly. An AOT-linked executable has no
/// interpreter, so its startup code registers the same facts as data (see
/// `typelisp_rt::printer`) and answers from those.
pub trait PrintEnv {
    /// The name of `variant` of the enum of type `ty`. `Err` when this
    /// environment has never heard of that type or variant: every enum a
    /// value can be of is registered, so that is a broken lookup.
    fn enum_variant_name(&self, ty: TypeId, variant: usize) -> Result<String, String>;

    /// Whether field `index` of the struct (`variant` `None`) or enum
    /// variant (`variant` `Some`) of type `ty` is a
    /// niche-represented `Option` that prints with its wrapper — a field
    /// whose word is the payload itself or the empty list, with no box to
    /// say which `Option` it is (`typelisp_mem::option_prints_wrapped`;
    /// `Option<sexpr>` prints as the datum it is). The renderer cannot read
    /// that off the word, so it asks the program's definition: the field's
    /// declared type, instantiated by the type the value carries. `Err` for a
    /// type or field this environment has never heard of.
    fn field_is_niched_option(&self, ty: TypeId, variant: Option<usize>, index: usize) -> Result<bool, String>;

    /// The name the definition of the struct of type `ty` gives field
    /// `index`, which prints before the field's value
    /// (`#<point x: 1 y: 2>`). `Ok(None)` for a type whose fields have no
    /// names — a `Vector<T>`'s elements, a handle's id — which print
    /// positionally; `Err` for a type this environment has never heard of.
    fn field_name(&self, ty: TypeId, index: usize) -> Result<Option<String>, String>;

    /// `v`'s own `print-object` rendering, or `None` when its type has no
    /// such method (the overwhelmingly common case — the caller then falls
    /// back to the built-in representation).
    ///
    /// `escape` is CL's `*print-escape*`: true for `~s`-style output that
    /// should read back, false for `~a`-style human text. Takes `&mut Heap`
    /// because running the method allocates.
    ///
    /// `Err` is the method itself failing (a panic inside it, or a return
    /// value that isn't a string), which the printer propagates rather than
    /// swallowing.
    fn print_object(&self, heap: &mut Heap, v: Value, escape: bool) -> Result<Option<String>, String>;
}

/// The type of boxed value `id`, or `None` when the box is neither a struct
/// nor an enum.
pub(crate) fn stored_type(heap: &Heap, id: typelisp_mem::BoxId) -> Option<TypeId> {
    if heap.is_struct(id) {
        Some(heap.struct_type_key(id))
    } else if heap.is_enum(id) {
        Some(heap.enum_type_key(id))
    } else {
        None
    }
}
