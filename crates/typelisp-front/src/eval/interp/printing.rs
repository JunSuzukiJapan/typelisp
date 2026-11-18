//! Printing from the interpreter: `print-object` dispatch, `format` calls, and
//! the printer's hooks into the running program.

use crate::{Heap, Path, SymRef, Value};
use crate::check::repr::Repr;
use crate::eval::value::EvalError;
use super::{with_active_interp, ACTIVE_INTERP, FnDef, INTERP_READ_HOOKS, Interp};

/// The method `print-object` dispatches `v` to: its type's path and the
/// method's name there — the *specialization* at the value's type arguments
/// when `v` is a generic type's instantiation, since monomorphization makes
/// one body per instantiation and the value's type is what says which.
/// `None` for a value no method can be for.
pub(crate) fn print_object_target(heap: &Heap, v: Value) -> Option<(Path, SymRef)> {
    let Value::Boxed(id) = v else { return None };
    let typelisp_mem::TypeShape::Named(ns, ty, args) = typelisp_mem::type_shape(crate::type_key::heap_type_id(heap, id)?) else {
        return None;
    };
    let method = typelisp_mem::symbols::member_symbol("print-object");
    let name = if args.is_empty() {
        method
    } else {
        typelisp_mem::specialization::specialization(typelisp_mem::specialization::Generic::Method(ns, ty, method), &args)
    };
    Some((Path::of_symbol(ns, ty), name))
}

/// Whether `f` is a `print-object` implementation by its signature — the
/// safety net that keeps an unrelated method that merely happens to be named
/// `print-object` (a plain `defmethod`, no trait involved) from being
/// mistaken for one.
pub(crate) fn is_print_object_sig(f: &FnDef) -> bool {
    matches!(f.sig.as_ref(), Some((params, ret)) if params.len() == 2 && params[1] == Repr::Bool && *ret == Repr::Str)
}

/// Whether what a `~/name/` calls takes its argument as a raw machine word,
/// or `None` when `f` does not have the signature `~/name/` calls at all.
pub(crate) fn format_call_receiver_is_word(f: &FnDef) -> Option<bool> {
    match f.sig.as_ref() {
        Some((params, ret))
            if params.len() == 3 && params[1] == Repr::Bool && params[2] == Repr::Bool && *ret == Repr::Str =>
        {
            Some(params[0] == Repr::Narrow)
        }
        _ => None,
    }
}

/// What `~/name/` says of a definition it was handed that does not have the
/// signature it calls.
pub(crate) fn format_call_sig_error(name: &str) -> String {
    format!("format: ~/{}/ must be `((x T) (colon bool) (at bool)) -> string`", name)
}

impl Interp {
    /// Registers this `Interp` as the environment the printer asks its two
    /// program questions of, and its control variables of — see
    /// [`INTERP_PRINT_HOOKS`].
    ///
    /// Called at the point of use rather than once per session, for the same
    /// reason [`Self::enter_compiled`] re-registers the heap on every
    /// crossing: it is two stores, and a nested `Interp` (`compile-file`
    /// builds one) would otherwise leave the slot naming an `Interp` that has
    /// since returned.
    ///
    /// `pub` for the one caller that is *not* at a point of use:
    /// `crate::shim::rt_eval_init` calls it on the immortal `Interp` it has
    /// just leaked. Building that environment runs this from inside, against
    /// an `Interp` still on `rt_eval_init`'s stack, and moving it into the
    /// leak leaves the slot dangling — which the rest of the process then
    /// follows, since an AOT program has no later crossing to refresh it.
    /// Re-registering the moved address is what closes that.
    pub fn install_print_hooks(&self) {
        ACTIVE_INTERP.with(|cell| cell.set(self as *const Interp));
        typelisp_print::runtime::set_print_hooks(Some(INTERP_PRINT_HOOKS));
        typelisp_read::runtime::set_read_hooks(Some(INTERP_READ_HOOKS));
    }
}

impl Interp {
    /// The `print-object` dispatch behind every `~a`/`~s`/`~w`/`pprint`
    /// rendering (`crate::eval::format::render_value` calls this first).
    /// `Ok(None)` means "no method applies, render `v` the built-in way".
    ///
    /// The *registration* is entirely static: writing `(impl print-object
    /// point ...)` registers an ordinary method on `point`, so the lookup
    /// here is just the module tree — there is no separate dispatch table to
    /// keep in sync, and no way to register a printer under a type it isn't
    /// for (the `impl` is type-checked like any other). What cannot be static
    /// is the *selection*: which directive consumes which argument depends on
    /// the control string's runtime contents, so only here — with `escape`
    /// in hand (CL's `*print-escape*`: `~s` true, `~a` false) — is it known
    /// what to ask the method for. That is the same split CLOS makes, where
    /// `print-object` methods are defined per class but selected at print
    /// time.
    ///
    /// The signature check is a safety net: it keeps an unrelated method that
    /// merely happens to be named `print-object` (a plain `defmethod`, no
    /// trait involved) from being mistaken for an implementation of it.
    ///
    /// GC: this runs user code, which conses, which can collect — but the
    /// value being printed needs no rooting *here*. Every builtin's arguments
    /// are already anchored for the whole call by `Self::eval_args`, which
    /// binds each one into a heap cell the collector traces on its own. Since
    /// collection is a non-moving mark-sweep that follows `car`/`cdr` and
    /// boxed nested values, that one anchor on the outermost value covers
    /// every part of it, including the elements the renderers hold in
    /// intermediate `Vec<Value>`s.
    pub(crate) fn print_object(&self, heap: &mut Heap, v: Value, escape: bool) -> Result<Option<String>, String> {
        let Some((type_path, method)) = print_object_target(heap, v) else {
            return Ok(None);
        };
        // A value already being printed by its own method is rendered the
        // built-in way instead, so `(impl print-object point (... (format
        // false "~a" self)))` degrades to `#<point 1 2>` rather than
        // recursing forever. Keyed on the value, not a depth limit, so a
        // genuinely nested self-referential structure still prints in full.
        if self.printing.borrow().contains(&v) {
            return Ok(None);
        }
        let Some(f) = self.root.borrow().get_method(&type_path, method) else {
            return Ok(None);
        };
        if !is_print_object_sig(&f) {
            return Ok(None);
        }
        self.printing.borrow_mut().push(v);
        // `enter`, not `apply`: an `impl print-object` method is an ordinary
        // function and is compiled like one when something has compiled it.
        // `apply` always tree-walks, so `(compile point::print-object)`
        // returned `true` and changed nothing about what the printer ran —
        // and `trace` never saw the call either, since `apply` is below the
        // watch. `enter` is where the compiled/interpreted choice is made.
        let result = self.enter(heap, &f, vec![v, Value::Bool(escape)]);
        self.printing.borrow_mut().pop();
        match result.map_err(|e| e.to_string())? {
            Value::Str(id) => Ok(Some(heap.string(id).to_string())),
            other => Err(format!("print-object on `{}` returned {:?}, not a string", type_path, other)),
        }
    }

    /// `~/name/`'s rendering of `v` — CL's function-call directive.
    ///
    /// `name` is the [`crate::CompileTarget::link_name`] of what the checker
    /// resolved the directive to — `(name v colon at)` resolved at the site,
    /// against the type of every argument the directive can meet — so the
    /// definition is the one registered under that name, not one looked up.
    pub(crate) fn format_call(
        &self,
        heap: &mut Heap,
        name: &str,
        v: Value,
        colon: bool,
        at: bool,
    ) -> Result<String, String> {
        let mut defs = Vec::new();
        self.root.borrow().collect_defs(&mut defs);
        let Some(f) = defs.into_iter().find(|f| f.target.link_name() == name) else {
            return Err(format!("format: ~/{}/ — no such definition", name));
        };
        // Whether the argument's *declared* representation is a raw machine
        // word — which is the question that decides what to hand over, not
        // the shape the printer happens to be holding.
        let Some(receiver_is_word) = format_call_receiver_is_word(&f) else {
            return Err(format_call_sig_error(name));
        };
        // A narrow integer arrives here as a `BoxedObj::Narrow`, because a
        // `Sexpr` is the one place a value's width is written down. A `u8`
        // *parameter* is the plain normalized word, like every statically
        // typed integer. So the box is opened on the way in — the crossing is
        // driven by the declared type, which is the only thing that can say
        // which side is which (`encode_crossing_args` got this wrong three
        // times by asking the value instead).
        let receiver = match v {
            Value::Boxed(id) if receiver_is_word => match heap.narrow_box(id) {
                Some(n) => Value::Int(n.value),
                None => v,
            },
            _ => v,
        };
        // `enter`, not `apply` — see `print_object_method`'s call for why.
        match self.enter(heap, &f, vec![receiver, Value::Bool(colon), Value::Bool(at)]).map_err(|e| e.to_string())? {
            Value::Str(id) => Ok(heap.string(id).to_string()),
            other => Err(format!("format: ~/{}/ returned {:?}, not a string", name, other)),
        }
    }

    /// The name of `variant` of the enum of type `ty` — the printer's
    /// question, answered from the scope tree.
    ///
    /// The tree holds every `TypeEntry::Enum`: a user `defenum`'s own exec,
    /// *and* the built-in sum types (`Option`/`Result`/the error types),
    /// which `Interp::new` seeds from `typeck::builtins::runtime_types` up front
    /// precisely so this lookup never needs a second table to fall back to.
    /// Coming up empty means the lookup itself is broken (a stale type, an
    /// enum the tree was never told about), not that the name lives
    /// somewhere else — which is why it is a printing error rather than
    /// anything plausible.
    fn enum_variant_name(&self, ty: crate::TypeId, variant: usize) -> Result<String, String> {
        self.root.borrow().enum_variant_name(&printed_definition(ty)?, variant)
    }

    /// The registered definition's field template, instantiated by the
    /// type's own arguments — `gen<Option<int>>`'s field `$0` is
    /// `Option<int>`, a niche. See [`typelisp_print::PrintEnv`].
    fn field_is_niched_option(&self, ty: crate::TypeId, variant: Option<usize>, index: usize) -> Result<bool, String> {
        let template = self.root.borrow().field_template(&printed_definition(ty)?, variant, index)?;
        let typelisp_mem::TypeShape::Named(_, _, args) = typelisp_mem::type_shape(ty) else {
            unreachable!("`printed_definition` found a named type")
        };
        Ok(typelisp_mem::option_prints_wrapped(typelisp_mem::instantiate(template, &args)))
    }

    /// The name the struct's definition gives field `index`, from the scope
    /// tree. See [`typelisp_print::PrintEnv::field_name`].
    fn field_name(&self, ty: crate::TypeId, index: usize) -> Result<Option<String>, String> {
        Ok(self.root.borrow().field_name(&printed_definition(ty)?, index)?.map(str::to_string))
    }

    /// A printer control variable's current value, for
    /// [`typelisp_print::runtime::read_print_vars`]/[`read_opts`](typelisp_print::runtime::read_opts).
    /// The prelude defines every one of them, so a missing one means there is
    /// no prelude to print under.
    fn printer_global(&self, heap: &Heap, name: &str) -> Result<Value, String> {
        self.global_value(heap, &crate::types::printer_global(name))
            .ok_or_else(|| format!("{} is not defined: printing needs the prelude that defines it", name))
    }

    /// Closes and writes out a pretty-printing session left open by a
    /// non-local exit (see [`Self::exec`]). A no-op in the normal case, where
    /// the matching `pprint-block-end-rt` already flushed it.
    pub(super) fn flush_pretty(&self, heap: &mut Heap) -> Result<(), EvalError> {
        typelisp_print::runtime::flush(heap).map_err(EvalError::Panic)
    }
}

/// The printer's [`typelisp_print::runtime::PrintHooks`] pointing at the
/// interpreter — the two program facts and the two sets of control
/// variables that `typelisp-print` cannot read off the heap.
///
/// Plain `fn` pointers, so each one reaches the running `Interp` through
/// [`ACTIVE_INTERP`] rather than by capture. That is the same thread-local
/// [`rt_apply_interpreted`] already uses, refreshed at the point of use for
/// the same reason: a nested `Interp` (`compile-file` builds its own) leaves
/// the slot pointing at a finished one otherwise.
const INTERP_PRINT_HOOKS: typelisp_print::runtime::PrintHooks = typelisp_print::runtime::PrintHooks {
    enum_variant_name: |ty, variant| {
        with_active_interp(|i| i.enum_variant_name(ty, variant)).unwrap_or_else(|| Err(NO_INTERP_FOR_TYPES.to_string()))
    },
    field_is_niched_option: |ty, variant, index| {
        with_active_interp(|i| i.field_is_niched_option(ty, variant, index))
            .unwrap_or_else(|| Err(NO_INTERP_FOR_TYPES.to_string()))
    },
    field_name: |ty, index| {
        with_active_interp(|i| i.field_name(ty, index)).unwrap_or_else(|| Err(NO_INTERP_FOR_TYPES.to_string()))
    },
    print_object: |heap, v, escape| {
        with_active_interp(|i| i.print_object(heap, v, escape)).unwrap_or_else(|| Err(NO_INTERP_FOR_METHODS.to_string()))
    },
    format_call: |heap, name, v, colon, at| match with_active_interp(|i| i.format_call(heap, name, v, colon, at)) {
        Some(r) => r,
        None => Err(format!("format: ~/{}/ needs a program to look the method up in", name)),
    },
    opts: |heap| match with_active_interp(|i| typelisp_print::runtime::read_opts(heap, &|name| i.printer_global(heap, name))) {
        Some(r) => r,
        None => Err(NO_INTERP_FOR_PRINTER_VARS.to_string()),
    },
    print_vars: |heap| match with_active_interp(|i| typelisp_print::runtime::read_print_vars(heap, &|name| i.printer_global(heap, name))) {
        Some(r) => r,
        None => Err(NO_INTERP_FOR_PRINTER_VARS.to_string()),
    },
};

/// The definition of a printed box's type, which is always a named one.
fn printed_definition(ty: crate::TypeId) -> Result<Path, String> {
    crate::type_key::definition_path(ty)
        .ok_or_else(|| format!("internal error: a box's type `{}` names no definition", &*typelisp_mem::type_name(ty)))
}

const NO_INTERP_FOR_PRINTER_VARS: &str = "printing needs a running program to read the printer control variables from";
const NO_INTERP_FOR_TYPES: &str = "printing needs a running program to look its types up in";
const NO_INTERP_FOR_METHODS: &str = "printing needs a running program to look `print-object` methods up in";
