//! Lowering: the analyzed tree ([`Node`]) to core IR
//! (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).
//!
//! One arm per kind of node, each lowering the node's children and then
//! calling the builder for its core form. Lowering decides nothing a check
//! depends on: the types it reads were all settled by the analysis.


use typelisp_mem::{wk, Error, Heap, RootScope, SymRef, Value};

use crate::types::{Path, Type};
use super::analyze::*;
use super::core::{self, Items};
use super::info::Info;
use super::registry::{AdtKind, Variant};
use super::repr::Repr;
use super::forms;

/// A child already lowered: its type, for the representation a builder
/// writes beside it, and its form.
pub(crate) struct Arg<'a> {
    pub(crate) ty: &'a Type,
    pub(crate) form: Value,
}

/// A `labels` function, its body lowered.
pub(crate) struct LoweredLabel {
    pub(crate) name: SymRef,
    pub(crate) params: Vec<(SymRef, Type)>,
    pub(crate) ret: Type,
    pub(crate) body: Vec<Value>,
}

/// The core-form builders: one per tag in `tests/core_vocabulary_test.rs`.
/// They read the type information (a type's representation) and nothing else.
pub(crate) trait CoreForms {
    /// The kind of the type `p` names, if it is a type this context knows.
    fn type_kind(&self, p: &Path) -> Option<AdtKind>;

    /// A Rust-implemented builtin function's declared parameter and result
    /// types, if `p` names one (and it is not a C function).
    fn builtin_fn_sig(&self, p: &Path) -> Option<(Vec<Type>, Type)>;

    /// [`Self::builtin_fn_sig`] for a method of the type `owner`.
    fn builtin_method_sig(&self, owner: &Path, method: SymRef) -> Option<(Vec<Type>, Type)>;

    /// How `ty`'s values are represented at runtime — the only residue of the
    /// type system the lowered form carries (see [`Repr`]).
    ///
    /// Resolved through the live [`Registry`] rather than a pair of frozen
    /// struct/enum sets: a file's own `defstruct` is registered as that file is
    /// checked, so a set snapshotted at entry would answer `None` for a type
    /// defined a few forms earlier.
    fn repr(&self, ty: &Type) -> Repr {
        Repr::of_by(ty, &|p| self.type_kind(p))
    }

    /// [`Self::repr`], written as the core IR spells it — for the repr
    /// positions in a lowered node (a `let` binding, a call argument, a struct
    /// field access).
    fn repr_form(&self, heap: &mut Heap, ty: &Type) -> Result<Value, Error> {
        self.repr(ty).write(heap)
    }

    /// One repr per type, as the parallel list every argument/parameter
    /// position uses.
    fn repr_forms(&self, heap: &mut Heap, tys: impl IntoIterator<Item = Type>) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        for ty in tys {
            let r = self.repr(&ty).write(f.heap())?;
            f.push(r);
        }
        f.finish_list()
    }

    // ---- core-form builders -------------------------------------------
    //
    // One function per tag in `tests/core_vocabulary_test.rs`, so the shape a
    // node is built in is written down exactly once and the reader who wants to
    // know what `(assoc ...)`'s fields are can find the answer next to the
    // consumers' own readers. Every one of them builds through `core::Items`,
    // which keeps each finished field rooted until the node is assembled — see
    // `check::core`'s module doc comment for why that is not optional.

    /// `(global PATH REPR)` — `PATH` is the global the checker resolved the
    /// name to, and every consumer reads it as is: nothing after checking
    /// resolves a name again.
    ///
    /// **Returns a rooted form**, for the same reason [`Self::var_form`] does —
    /// see there.
    fn global_form(&self, heap: &mut Heap, r: &Path, ty: &Type) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(r);
        f.push(path);
        let repr = self.repr_form(f.heap(), ty)?;
        f.push(repr);
        let form = f.finish(wk::GLOBAL)?;
        Ok(forms::rooted(heap, form))
    }

    /// `(set-global PATH REPR FORM)` — see [`Self::global_form`] for `PATH`.
    fn set_global_form(&self, heap: &mut Heap, r: &Path, ty: &Type, value: Value) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(r);
        f.push(path);
        let repr = self.repr_form(f.heap(), ty)?;
        f.push(repr);
        f.push(value);
        f.finish(wk::SET_GLOBAL)
    }

    /// `(let ((SYM REPR FORM)...) BODY...)`.
    ///
    /// Also the only sequencing node: `progn` is `(let () ...)`, a `let` with
    /// no bindings, so there is one shape for "run these in order" rather than
    /// two.
    fn let_form(&self, heap: &mut Heap, binds: &[(SymRef, Type, Value)], body: &[Value]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let mut bs = Items::new(f.heap());
        for (name, ty, val) in binds {
            let repr = self.repr_form(bs.heap(), ty)?;
            let one = core::list(bs.heap(), &[Value::Symbol(*name), repr, *val])?;
            bs.push(one);
        }
        let binds = bs.finish_list()?;
        f.push(binds);
        f.extend(body.iter().copied());
        f.finish(wk::LET)
    }


    /// `(call PATH (REPR...) ARG...)` — a call to a free function, `PATH` the
    /// one the checker resolved the name to.
    ///
    /// The representations are the *arguments'* own, one per argument in the
    /// same order: the island reads them to decide the GC-root bookkeeping
    /// around each argument as it crosses into a compiled frame.
    fn call_form(&self, heap: &mut Heap, r: &Path, args: &[Arg]) -> Result<Value, Error> {
        // A Rust-implemented builtin's `int`-typed parameters cross the
        // compiled boundary as raw machine words — see `int_boundary_raw`.
        let boundary = self.builtin_fn_sig(r).filter(|_| !int_boundary_native_fn(r.name_symbol()));
        let mut f = Items::new(heap);
        let path = core::definition_form(r);
        f.push(path);
        // An untagged argument is a raw word from here on, and its
        // representation has to say so: a rooted slot holding a raw word
        // would be read by the collector as a reference.
        let declared_int = |i: usize| boundary.as_ref().and_then(|(ps, _)| ps.get(i)).is_some_and(|t| *t == Type::Int);
        let reprs = self.repr_forms(
            f.heap(),
            args.iter().enumerate().map(|(i, a)| if declared_int(i) { Type::I32 } else { a.ty.clone() }),
        )?;
        f.push(reprs);
        for (i, a) in args.iter().enumerate() {
            let form = if declared_int(i) { untag_int_form(f.heap(), a.form)? } else { a.form };
            f.push(form);
        }
        let call = f.finish(wk::CALL)?;
        match boundary {
            Some((_, Type::Int)) => tag_int_form(heap, call),
            _ => Ok(call),
        }
    }

    /// `(assoc PATH SYM INSTANCE RET-REPR (REPR...) RET-KEY ARG...)` — an
    /// instance method (`args[0]` is the receiver) or a static associated
    /// function.
    ///
    /// `ret` is here for the receiver-less case: with no receiver to classify,
    /// the result is the only thing that says whether `(Vector::new)` is a
    /// container operation the island open-codes or an ordinary call.
    fn assoc_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: SymRef,
        instance: bool,
        ret: &Type,
        args: &[Arg],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let ty_path = core::definition_form(type_name);
        f.push(ty_path);
        f.push(Value::Symbol(method));
        f.push(Value::Bool(instance));
        let ret_repr = self.repr_form(f.heap(), ret)?;
        f.push(ret_repr);
        // A Rust-implemented builtin's `int`-typed parameters and result cross
        // the compiled boundary as raw machine words — see `int_boundary_raw`.
        // Decided on the *declared* signature, not the instantiated one: a
        // `Vector<int>`'s `get` takes an `int` index and returns a `T` that
        // happens to be `int`, and only the index is a machine word.
        let boundary = self.builtin_method_sig(type_name, method).filter(|_| int_boundary_raw(type_name, method));
        let declared_int = |i: usize| boundary.as_ref().and_then(|(ps, _)| ps.get(i)).is_some_and(|t| *t == Type::Int);
        // An untagged argument is a raw word from here on, and its
        // representation has to say so (`call_form` makes the same point).
        let reprs = self.repr_forms(
            f.heap(),
            args.iter().enumerate().map(|(i, a)| if declared_int(i) { Type::I32 } else { a.ty.clone() }),
        )?;
        f.push(reprs);
        // The *result's* runtime identity. A built-in container method builds
        // its box in Rust (`Vector::new`, `HashTable::keys`), below the
        // checker and with no `Path` to derive an instantiation from — so the
        // instantiation travels from here, the one place that knows it. The
        // representation above cannot stand in: `Repr::Vector(Int)` is
        // `Vector<i32>` and `Vector<i64>` at once.
        let key = core::type_form(f.heap(), ret, &[])?;
        f.push(key);
        for (i, a) in args.iter().enumerate() {
            let form = if declared_int(i) { untag_int_form(f.heap(), a.form)? } else { a.form };
            f.push(form);
        }
        let assoc = f.finish(wk::ASSOC)?;
        match boundary {
            Some((_, Type::Int)) => tag_int_form(heap, assoc),
            _ => Ok(assoc),
        }
    }

    /// `(fnref PATH (PARAM-REPR...))` — a named free function reified as a
    /// value.
    ///
    /// The parameter representations are carried because the bridge turns this
    /// into a closure construction, and there is no call site here to read them
    /// from. A `&rest` parameter is simply the last entry (always `sexpr`), so
    /// it needs no flag of its own.
    fn fnref_form(&self, heap: &mut Heap, r: &Path, params: &[Type]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(r);
        f.push(path);
        let reprs = self.repr_forms(f.heap(), params.to_vec())?;
        f.push(reprs);
        f.finish(wk::FNREF)
    }

    /// `(methodref PATH SYM (PARAM-REPR...) RET-KEY)` — see
    /// [`Self::fnref_form`] for the representation list.
    ///
    /// `RET-KEY` is the runtime identity of what the method returns, for the
    /// same reason [`Self::assoc_form`] carries one: a built-in container
    /// method builds its box in Rust, and reaching it *through a function
    /// value* is too late to ask what instantiation the reference was at. The
    /// value carries it (`BoxedObj::Builtin`'s `ret_key`).
    fn methodref_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: SymRef,
        params: &[Type],
        ret: &Type,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let ty_path = core::definition_form(type_name);
        f.push(ty_path);
        f.push(Value::Symbol(method));
        let reprs = self.repr_forms(f.heap(), params.to_vec())?;
        f.push(reprs);
        let key = core::type_form(f.heap(), ret, &[])?;
        f.push(key);
        f.finish(wk::METHODREF)
    }

    /// `(apply CALLEE RET-REPR (REPR...) ARG...)` — applying a function *value*.
    ///
    /// The return representation is here for the same reason `assoc` carries
    /// one: the callee may be a *compiled* closure, and a value coming back out
    /// of compiled code has to be decoded by its declared representation, never
    /// by its shape. Unlike a `call`, there is no name to look a signature up
    /// by — the callee is a value — so the call site is the only place that
    /// knows. The island ignores the field (`apply`/`apply-indirect` take
    /// argument pairs only).
    fn apply_form(&self, heap: &mut Heap, callee: Value, ret: &Type, args: &[Arg]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(callee);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        let reprs = self.repr_forms(f.heap(), args.iter().map(|a| a.ty.clone()))?;
        f.push(reprs);
        f.extend(args.iter().map(|a| a.form));
        f.finish(wk::APPLY)
    }

    /// `(construct PATH VARIANT MUTABLE (REPR...) ARG...)`.
    ///
    /// `field_tys` are the variant's *declared* field types with this site's
    /// type arguments already substituted in — not the argument expressions'
    /// own types, which can be narrower (a `!`-typed argument, a `Symbol` value
    /// widening into a `Sexpr` field). Which of the two decides the shape a
    /// field is stored in is settled: the declaration does, on both sides of
    /// every boundary.
    ///
    /// The representations travel with the *site* rather than being read back
    /// from the type's own `defstruct`/`defenum`, because for a generic ADT the
    /// definition cannot answer the question: `Option`'s `Some` field is
    /// declared `T`, and `T` has no representation. The instantiation is known
    /// only here. A definition-keyed table cannot be patched to cover it
    /// either — monomorphization erases, so `Maybe<i64>` and `Maybe<string>`
    /// are both at the path `Maybe` and only one set of fields could be
    /// recorded there. (Getting this wrong emitted field kind `0` and the
    /// island aborted with `compile-sexpr-field: field type is not
    /// representable in compiled code yet`.)
    fn construct_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        targs: &[Type],
        variant: usize,
        mutable: bool,
        field_tys: &[Type],
        args: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(type_name);
        f.push(path);
        // The value's runtime identity, spelled here because here is the only
        // place the instantiation is known — the same reason `field_tys` is
        // computed at the site rather than read off the definition. The path
        // above stays for the questions that are about the *definition*
        // (struct or enum, where it was defined); this is the one about the
        // *value* (which instantiation it is).
        let key = core::type_form(f.heap(), &Type::Named(type_name.clone(), targs.to_vec()), &[])?;
        f.push(key);
        f.push(Value::Int(variant as i64));
        f.push(Value::Bool(mutable));
        let reprs = self.repr_forms(f.heap(), field_tys.iter().cloned())?;
        f.push(reprs);
        f.extend(args.iter().copied());
        f.finish(wk::CONSTRUCT)
    }

    /// `(field-get OBJ IDX REPR)`.
    ///
    /// Unlike `construct` this *does* spell out a representation: it names only
    /// an index, and the object is an arbitrary expression whose type is
    /// exactly what the lowered form no longer carries.
    fn field_get_form(&self, heap: &mut Heap, obj: Value, idx: usize, field_ty: &Type) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(obj);
        f.push(Value::Int(idx as i64));
        let repr = self.repr_form(f.heap(), field_ty)?;
        f.push(repr);
        f.finish(wk::FIELD_GET)
    }

    /// `(field-set OBJ IDX REPR VALUE)` — see [`Self::field_get_form`].
    fn field_set_form(
        &self,
        heap: &mut Heap,
        obj: Value,
        idx: usize,
        field_ty: &Type,
        value: Value,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(obj);
        f.push(Value::Int(idx as i64));
        let repr = self.repr_form(f.heap(), field_ty)?;
        f.push(repr);
        f.push(value);
        f.finish(wk::FIELD_SET)
    }

    /// `(pat-ctor PATH KEY VARIANT DOWNCAST (REPR...) SUB...)`: the variant
    /// `variant` of `type_name<targs>`, its fields (declared `field_tys` at
    /// these arguments) matched by the patterns `subs`, already lowered and
    /// rooted.
    ///
    /// The instantiation is spelled the way the constructing site spells it
    /// ([`Self::construct_form`]): for an ordinary pattern the scrutinee's
    /// static type already guarantees it, and for a downcast it is the test
    /// itself, so a `gen<string>` value is not taken for a `gen<i32>`. The
    /// field representations travel with the site for the reason
    /// `construct_form` gives.
    #[allow(clippy::too_many_arguments)]
    fn pat_ctor_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        targs: &[Type],
        variant: usize,
        downcast: bool,
        field_tys: &[Type],
        subs: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let tp = core::definition_form(type_name);
        f.push(tp);
        let key = core::type_form(f.heap(), &Type::Named(*type_name, targs.to_vec()), &[])?;
        f.push(key);
        f.push(Value::Int(variant as i64));
        f.push(Value::Bool(downcast));
        let reprs = self.repr_forms(f.heap(), field_tys.iter().cloned())?;
        f.push(reprs);
        f.extend(subs.iter().copied());
        f.finish(wk::PAT_CTOR)
    }

    /// `(pat-some REPR SUB)`: a niched `Option<T>`'s `some`, any word but the
    /// empty list, its payload read back through `T`'s representation
    /// `payload` — the island untags a narrow payload and opens a float box
    /// before `SUB` (already lowered and rooted) can compare or bind it. Its
    /// `none` is `(pat-empty)`.
    fn pat_some_form(&self, heap: &mut Heap, payload: &Repr, sub: Value) -> Result<Value, Error> {
        let repr = payload.write(heap)?;
        let mut s = RootScope::new(heap);
        s.push_root(repr);
        core::tagged(&mut s, wk::PAT_SOME, &[repr, sub])
    }

    /// `(match SCRUT REPR (PAT BODY...)...)`.
    ///
    /// The scrutinee's representation is what a whole-value `(pat-bind x)`
    /// binds at, since such a pattern names no type of its own.
    fn match_form(
        &self,
        heap: &mut Heap,
        scrut: Value,
        scrut_ty: &Type,
        arms: &[(Value, Vec<Value>)],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(scrut);
        let repr = self.repr_form(f.heap(), scrut_ty)?;
        f.push(repr);
        for (pat, body) in arms {
            let mut a = Items::new(f.heap());
            a.push(*pat);
            a.extend(body.iter().copied());
            let arm = a.finish_list()?;
            f.push(arm);
        }
        f.finish(wk::MATCH)
    }

    /// `(lambda ((SYM REPR)...) RET-REPR BODY...)`.
    fn lambda_form(
        &self,
        heap: &mut Heap,
        params: &[(SymRef, Type)],
        ret: &Type,
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let ps = self.param_list_form(f.heap(), params)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        f.extend(body.iter().copied());
        f.finish(wk::LAMBDA)
    }

    /// `((SYM REPR)...)` — a parameter list, shared by `lambda`/`labels` and
    /// the top-level definition forms.
    fn param_list_form(&self, heap: &mut Heap, params: &[(SymRef, Type)]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        for (name, ty) in params {
            let repr = self.repr_form(f.heap(), ty)?;
            let one = core::list(f.heap(), &[Value::Symbol(*name), repr])?;
            f.push(one);
        }
        f.finish_list()
    }

    /// `(labels ((NAME ((SYM REPR)...) REPR BODY...)...) BODY...)`, the
    /// functions' bodies and `body` lowered already.
    fn labels_core_form(&self, heap: &mut Heap, defs: &[LoweredLabel], body: &[Value]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let mut ds = Items::new(f.heap());
        for def in defs {
            let mut d = Items::new(ds.heap());
            d.push(Value::Symbol(def.name));
            let ps = self.param_list_form(d.heap(), &def.params)?;
            d.push(ps);
            let r = self.repr_form(d.heap(), &def.ret)?;
            d.push(r);
            d.extend(def.body.iter().copied());
            let one = d.finish_list()?;
            ds.push(one);
        }
        let defs = ds.finish_list()?;
        f.push(defs);
        f.extend(body.iter().copied());
        f.finish(wk::LABELS)
    }

    /// `(dyn-new TYPE TRAIT ((TYPE SYM)...) ((TRAIT ((TYPE SYM)...))...) REPR
    /// FORM)` — boxing a concrete value as a trait object.
    ///
    /// The trailing representation is the boxed value's own, and it cannot be
    /// derived from the concrete type's *name* (all the rest of the node
    /// carries): a struct and an enum differ in whether the island has to root
    /// the value across the boxing call.
    fn dyn_new_form(
        &self,
        heap: &mut Heap,
        trait_path: &Path,
        slots: &[(Path, SymRef)],
        supers: &[(Path, Vec<(Path, SymRef)>)],
        value_ty: &Type,
        value: Value,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let key = core::type_form(f.heap(), value_ty, &[])?;
        f.push(key);
        let tp = core::definition_form(trait_path);
        f.push(tp);
        let slot_list = vtable_form(f.heap(), slots)?;
        f.push(slot_list);
        let mut sup = Items::new(f.heap());
        for (trait_path, slots) in supers {
            let tp = core::definition_form(trait_path);
            let mut one = Items::new(sup.heap());
            one.push(tp);
            let sl = vtable_form(one.heap(), slots)?;
            one.push(sl);
            let entry = one.finish_list()?;
            sup.push(entry);
        }
        let supers = sup.finish_list()?;
        f.push(supers);
        let repr = self.repr_form(f.heap(), value_ty)?;
        f.push(repr);
        f.push(value);
        f.finish(wk::DYN_NEW)
    }

    /// `(dyn-call TRAIT SYM SLOT ((TYPE SYM)...) (REPR...) ARG...)` — a call
    /// through a trait object's vtable, `args[0]` the receiver.
    ///
    /// The implementation list is not what dispatches — that is the vtable's
    /// job, and the point is that the target is unknown until run time — but it
    /// tells the compiler which bodies must exist natively before this call
    /// site can run natively.
    fn dyn_call_form(
        &self,
        heap: &mut Heap,
        trait_path: &Path,
        method: SymRef,
        slot: usize,
        impl_targets: &[(Path, SymRef)],
        args: &[Arg],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let tp = core::definition_form(trait_path);
        f.push(tp);
        f.push(Value::Symbol(method));
        f.push(Value::Int(slot as i64));
        let targets = vtable_form(f.heap(), impl_targets)?;
        f.push(targets);
        let reprs = self.repr_forms(f.heap(), args.iter().map(|a| a.ty.clone()))?;
        f.push(reprs);
        f.extend(args.iter().map(|a| a.form));
        f.finish(wk::DYN_CALL)
    }


    // ---- top-level form builders --------------------------------------

    /// `(defun PATH ((SYM REPR)...) RET-REPR BODY...)`.
    ///
    /// No `pub`: visibility was settled where each reference was checked, and
    /// nothing after checking resolves a name.
    fn defun_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        params: &[(SymRef, Type)],
        ret: &Type,
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(name);
        f.push(path);
        let ps = self.param_list_form(f.heap(), params)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        f.extend(body.iter().copied());
        f.finish(wk::DEFUN)
    }

    /// `(defffi PATH C-SYMBOL LIBRARY ((SYM REPR)...) RET-REPR (TYPE...) RET-TYPE)`.
    ///
    /// Fields 3-4 are `defun`'s own, so `Interp::exec` reads them back with
    /// the readers it already has: what it registers is an ordinary [`FnDef`]
    /// with an empty body, which is what makes the thunk reachable through the
    /// same `enter` every other call goes through. Parameter names are
    /// synthesized (`a0`, `a1`, ...) — a C declaration has none, and `FnDef`
    /// wants one per parameter.
    ///
    /// The last two fields are the declared types, *deliberately* redundant
    /// with the `REPR`s beside them. A `Repr` folds all six integer widths into
    /// `Repr::Narrow` (see its doc comment), which is the right answer for the
    /// question a `Repr` exists to answer — how a value crosses the compiled
    /// boundary — and the wrong one for the thunk, which has to emit
    /// `trunc i64 to i8`. Two questions about one type, the way `field_kind`
    /// and `binding_kind` are. One producer (`Checker::check_defffi`), so they
    /// cannot drift.
    #[allow(clippy::too_many_arguments)]
    fn defffi_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        c_symbol: &str,
        library: Option<&str>,
        params: &[Type],
        ret: &Type,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(name);
        f.push(path);
        let sym = f.heap().alloc_string(c_symbol.to_string());
        f.push(sym);
        // No `:library` is `()`, not a name to look up — the empty string is
        // a library whose name is empty, and the two must not read alike.
        let lib = match library {
            Some(l) => f.heap().alloc_string(l.to_string()),
            None => Value::Empty,
        };
        f.push(lib);
        // A callback parameter crosses as the entry's address — a raw word —
        // whatever function type it was declared with.
        let named: Vec<(SymRef, Type)> = params
            .iter()
            .enumerate()
            .map(|(i, t)| (typelisp_mem::symbols::intern(&format!("a{}", i)), if matches!(t, Type::Fn(..)) { Type::Ptr } else { t.clone() }))
            .collect();
        let ps = self.param_list_form(f.heap(), &named)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        let mut cs = Items::new(f.heap());
        for t in params {
            let k = core::type_form(cs.heap(), t, &[])?;
            cs.push(k);
        }
        let types = cs.finish_list()?;
        f.push(types);
        let rk = core::type_form(f.heap(), ret, &[])?;
        f.push(rk);
        f.finish(wk::DEFFFI)
    }

    /// `(defmethod PATH SYM INSTANCE ((SYM REPR)...) RET-REPR BODY...)`.
    ///
    /// The receiver is the first parameter for an instance method, not a field
    /// of its own the way `TopLevel::Defmethod::self_name` was: `params` is
    /// what the body binds, in order, and `instance` says whether the first
    /// entry is the receiver.
    fn defmethod_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: SymRef,
        instance: bool,
        params: &[(SymRef, Type)],
        ret: &Type,
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(type_name);
        f.push(path);
        f.push(Value::Symbol(method));
        f.push(Value::Bool(instance));
        let ps = self.param_list_form(f.heap(), params)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        f.extend(body.iter().copied());
        f.finish(wk::DEFMETHOD)
    }


    /// `(defvar PATH REPR MUTABLE INIT)` — a global variable (`mutable`) or
    /// constant — plus a fifth field for `defparameter`.
    ///
    /// That fifth field says **assign even if the global is already bound** —
    /// the difference between CL's `defparameter` and its `defvar`, which is a
    /// run-time question and so cannot be settled here. It is a trailing field
    /// rather than a tag of its own because every other consumer of a `defvar`
    /// node (the dump, the AOT collector, the prelude generator, the pretty
    /// printer, `Interp::note_definitions`) reads the fields it cares about by
    /// index and is right to ignore this one; only `Interp::exec` looks. An
    /// absent field reads as `false`, so a node built before this existed —
    /// one sitting in an older dump — means `defvar`.
    fn defvar_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        ty: &Type,
        mutable: bool,
        value: Value,
        reassign: bool,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(name);
        f.push(path);
        let r = self.repr_form(f.heap(), ty)?;
        f.push(r);
        f.push(Value::Bool(mutable));
        f.push(value);
        if reassign {
            f.push(Value::Bool(true));
        }
        f.finish(wk::DEFVAR)
    }

    /// `(defstruct PATH (REPR...) (TEMPLATE...) (NAME...))` — the type's
    /// field representations, which is where the bridge reads a
    /// `construct`'s and a pattern's field kinds from, then each field's type
    /// as a key template (`type_key::field_key_template`), which is how the
    /// printer tells a niche-represented `Option` field from the bare word it
    /// holds, then each field's name, which the printer writes before its
    /// value (`#<point x: 1 y: 2>`).
    fn defstruct_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        params: &[String],
        fields: &[Type],
        names: &[String],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(name);
        f.push(path);
        let reprs = self.repr_forms(f.heap(), fields.to_vec())?;
        f.push(reprs);
        let templates = field_template_list(f.heap(), params, fields)?;
        f.push(templates);
        let names = string_list(f.heap(), names)?;
        f.push(names);
        f.finish(wk::DEFSTRUCT)
    }

    /// `(defenum PATH (SYM...) ((REPR...)...) ((TEMPLATE...)...))` — the
    /// variant names, then each variant's field representations, then each
    /// variant's field key templates (see [`Self::defstruct_form`]).
    fn defenum_form(&self, heap: &mut Heap, name: &Path, params: &[String], variants: &[Variant]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = core::definition_form(name);
        f.push(path);
        let mut names = Items::new(f.heap());
        names.extend(variants.iter().map(|v| Value::Symbol(v.name)));
        let names = names.finish_list()?;
        f.push(names);
        let mut vs = Items::new(f.heap());
        for v in variants {
            let reprs = self.repr_forms(vs.heap(), v.fields.to_vec())?;
            vs.push(reprs);
        }
        let per_variant = vs.finish_list()?;
        f.push(per_variant);
        let mut ts = Items::new(f.heap());
        for v in variants {
            let templates = field_template_list(ts.heap(), params, &v.fields)?;
            ts.push(templates);
        }
        let per_variant_templates = ts.finish_list()?;
        f.push(per_variant_templates);
        f.finish(wk::DEFENUM)
    }
}

impl CoreForms for Info {
    fn type_kind(&self, p: &Path) -> Option<AdtKind> {
        self.reg.type_def(p).map(|d| d.kind)
    }

    fn builtin_fn_sig(&self, p: &Path) -> Option<(Vec<Type>, Type)> {
        self.reg.fn_sig(p).filter(|sig| sig.builtin && !sig.ffi).map(|sig| (sig.params.clone(), sig.ret.clone()))
    }

    fn builtin_method_sig(&self, owner: &Path, method: SymRef) -> Option<(Vec<Type>, Type)> {
        self.reg
            .type_def(owner)
            .and_then(|d| d.assoc.get(method))
            .filter(|af| af.builtin)
            .map(|af| (af.sig.params.clone(), af.sig.ret.clone()))
    }
}

/// The builders that are this checker's own: they read its registry's
/// name-keyed type parameters, or lower its tree's nodes themselves.
impl Info {

}
