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

use typelisp_mem::{Heap, Value};

pub mod aot;
pub mod format;
pub mod pprint;
pub mod runtime;
pub mod shim;

/// What the printer needs to know about the *program* whose values it is
/// printing.
///
/// Two questions, both unanswerable from the heap alone:
///
/// - an enum box stores its type key and variant *index*, never the variant's
///   name — `(some 3)` prints as `(some 3)` only because someone can map
///   `("option", 0)` back to `"some"`;
/// - a type may implement the `print-object` trait, in which case its own
///   method decides the text, and finding that method means knowing the
///   program's method table.
///
/// The interpreter answers both directly. An AOT-linked executable has no
/// interpreter, so its startup code registers the same facts as data (see
/// `typelisp_rt::printer`) and answers from those.
pub trait PrintEnv {
    /// The name of `variant` of the enum whose type key is `type_key`, or
    /// `None` when this environment has never heard of that type. `None`
    /// prints as `<unknown-variant>` — a spelling disagreement, not a missing
    /// feature, so it is deliberately loud.
    fn enum_variant_name(&self, type_key: &str, variant: usize) -> Option<String>;

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

    /// `~/name/`'s rendering of `v` — CL's function-call directive, with
    /// `colon` and `at` carrying the directive's own `:` and `@` flags.
    ///
    /// **`name` is looked up as a method on `v`'s own type**, not as a global
    /// function the way CL's is. That is not a shortcut: a control string is
    /// an ordinary runtime `string`, so which directive runs on which argument
    /// is not known until it runs, and by then a registered definition carries
    /// only its [`Repr`]s — `Repr::Struct` is every `defstruct` at once, so a
    /// global-function lookup could not tell `point`'s helper from `pathname`'s
    /// and would call one with the other's value. Dispatching on the value
    /// instead is the same mechanism [`Self::print_object`] uses, and it is
    /// sound for the same reason: the method was type-checked against exactly
    /// the type that is now being handed to it.
    ///
    /// The method must be `((self Self) (colon bool) (at bool)) -> string`.
    /// A value with no heap type of its own (`i64`/`bool`/`char`) has no
    /// method table to look in, and says so.
    fn format_call(&self, heap: &mut Heap, name: &str, v: Value, colon: bool, at: bool) -> Result<String, String>;
}

/// A [`PrintEnv`] that knows nothing: every enum prints `<unknown-variant>`
/// and no type has a `print-object` method.
///
/// Not a fallback for real programs — it exists so a unit test (and the
/// bootstrap, before any program has been loaded) can render a value without
/// standing up an interpreter.
pub struct BarePrintEnv;

impl PrintEnv for BarePrintEnv {
    fn enum_variant_name(&self, _type_key: &str, _variant: usize) -> Option<String> {
        None
    }

    fn print_object(&self, _heap: &mut Heap, _v: Value, _escape: bool) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn format_call(&self, _heap: &mut Heap, name: &str, _v: Value, _colon: bool, _at: bool) -> Result<String, String> {
        Err(format!("format: ~/{}/ needs a program to look the method up in", name))
    }
}

/// The type key stored in boxed value `id`, or `None` when the box carries no
/// type name.
///
/// The printer's own reader for what `typelisp::type_key` writes. It stays a
/// *string* here on purpose: this crate has no `Path`, and every question the
/// printer asks of a type identity — display it, look up a variant name,
/// dispatch `print-object` — is answered by the key as written.
pub(crate) fn stored_type_key(heap: &Heap, id: typelisp_mem::BoxId) -> Option<&str> {
    if heap.is_struct(id) {
        Some(heap.struct_type_name(id))
    } else if heap.is_enum(id) {
        Some(heap.enum_type_name(id))
    } else {
        None
    }
}
