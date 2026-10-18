//! Lowering: the analyzed tree ([`Node`]) to core IR
//! (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).
//!
//! One function per kind of node, each calling the `forms` builder for its
//! core form. A node's children are [`Checked`] values whose `form` is
//! already lowered, so lowering a node builds that node's own form only.
//! Lowering decides nothing a check depends on: the types it reads were all
//! settled by the analysis.

use std::collections::BTreeMap;

use typelisp_mem::{wk, ConcreteNames, Error, Heap, RootScope, Value};

use crate::nominal_path;
use crate::types::{Path, Type};
use super::checker::*;
use super::core::{self, Checked, Items, Node};
use super::info::Info;
use super::registry::Variant;
use super::repr::Repr;
use super::resolved::Pattern;
use super::{fold, forms};

/// `node`'s core form.
pub(super) fn lower(heap: &mut Heap, node: &Node) -> Result<Value, Error> {
    match node {
        Node::Lowered => unreachable!("a `Lowered` expression is built with its form"),
        Node::If { cond, then, els } => lower_if(heap, cond, then, els),
        Node::Panic { message } => forms::panic_form(heap, message.form),
    }
}

/// A literal condition picks its branch now. Both branches were still
/// analyzed and checked: a branch that can never run is still a program that
/// has to be well-typed.
fn lower_if(heap: &mut Heap, cond: &Checked, then: &Checked, els: &Checked) -> Result<Value, Error> {
    match fold::literal_bool(heap, cond.form) {
        Some(true) => Ok(then.form),
        Some(false) => Ok(els.form),
        None => forms::if_form(heap, cond.form, then.form, els.form),
    }
}

/// The core-form builders: one per tag in `tests/core_vocabulary_test.rs`.
/// They read the type information (a type's representation) and nothing else.
impl Info {
    /// How `ty`'s values are represented at runtime — the only residue of the
    /// type system the lowered form carries (see [`Repr`]).
    ///
    /// Resolved through the live [`Registry`] rather than a pair of frozen
    /// struct/enum sets: a file's own `defstruct` is registered as that file is
    /// checked, so a set snapshotted at entry would answer `None` for a type
    /// defined a few forms earlier.
    pub(super) fn repr(&self, ty: &Type) -> Repr {
        Repr::of_by(ty, &|p| self.reg.type_def(p).map(|d| d.kind))
    }

    /// [`Self::repr`], written as the core IR spells it — for the repr
    /// positions in a lowered node (a `let` binding, a call argument, a struct
    /// field access).
    pub(super) fn repr_form(&self, heap: &mut Heap, ty: &Type) -> Result<Value, Error> {
        self.repr(ty).write(heap)
    }

    /// One repr per type, as the parallel list every argument/parameter
    /// position uses.
    pub(super) fn repr_forms(&self, heap: &mut Heap, tys: impl IntoIterator<Item = Type>) -> Result<Value, Error> {
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
    pub(super) fn global_form(&self, heap: &mut Heap, r: &Path, ty: &Type) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), r);
        f.push(path);
        let repr = self.repr_form(f.heap(), ty)?;
        f.push(repr);
        let form = f.finish(wk::GLOBAL)?;
        Ok(forms::rooted(heap, form))
    }

    /// `(set-global PATH REPR FORM)` — see [`Self::global_form`] for `PATH`.
    pub(super) fn set_global_form(&self, heap: &mut Heap, r: &Path, ty: &Type, value: Value) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), r);
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
    pub(super) fn let_form(&self, heap: &mut Heap, binds: &[(String, Type, Value)], body: &[Value]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let mut bs = Items::new(f.heap());
        for (name, ty, val) in binds {
            let sym = bs.heap().intern_symbol(name);
            let repr = self.repr_form(bs.heap(), ty)?;
            let one = core::list(bs.heap(), &[sym, repr, *val])?;
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
    pub(super) fn call_form(&self, heap: &mut Heap, r: &Path, args: &[Checked]) -> Result<Value, Error> {
        // A Rust-implemented builtin's `int`-typed parameters cross the
        // compiled boundary as raw machine words — see `int_boundary_raw`.
        let boundary = self
            .reg
            .fn_sig(r)
            .filter(|sig| sig.builtin && !sig.ffi && !int_boundary_native_fn(r.last_segment()))
            .map(|sig| (sig.params.clone(), sig.ret.clone()));
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), r);
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
    /// the result is the only thing that says whether `(vector::new)` is a
    /// container operation the island open-codes or an ordinary call.
    pub(super) fn assoc_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: &str,
        instance: bool,
        ret: &Type,
        args: &[Checked],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let ty_path = forms::path_form(f.heap(), type_name);
        f.push(ty_path);
        let m = f.heap().intern_symbol(method);
        f.push(m);
        f.push(Value::Bool(instance));
        let ret_repr = self.repr_form(f.heap(), ret)?;
        f.push(ret_repr);
        // A Rust-implemented builtin's `int`-typed parameters and result cross
        // the compiled boundary as raw machine words — see `int_boundary_raw`.
        // Decided on the *declared* signature, not the instantiated one: a
        // `Vector<int>`'s `get` takes an `int` index and returns a `T` that
        // happens to be `int`, and only the index is a machine word.
        let boundary = self
            .reg
            .type_def(type_name)
            .and_then(|d| d.assoc.get(method))
            .filter(|af| af.builtin && int_boundary_raw(type_name, method))
            .map(|af| (af.sig.params.clone(), af.sig.ret.clone()));
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
        let key = ConcreteNames::type_key(ret);
        let key = f.heap().alloc_string(key);
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
    pub(super) fn fnref_form(&self, heap: &mut Heap, r: &Path, params: &[Type]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), r);
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
    pub(super) fn methodref_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: &str,
        params: &[Type],
        ret: &Type,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let ty_path = forms::path_form(f.heap(), type_name);
        f.push(ty_path);
        let m = f.heap().intern_symbol(method);
        f.push(m);
        let reprs = self.repr_forms(f.heap(), params.to_vec())?;
        f.push(reprs);
        let key = ConcreteNames::type_key(ret);
        let key = f.heap().alloc_string(key);
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
    pub(super) fn apply_form(&self, heap: &mut Heap, callee: Value, ret: &Type, args: &[Checked]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(callee);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        let reprs = self.repr_forms(f.heap(), args.iter().map(|a| a.ty.clone()))?;
        f.push(reprs);
        f.extend(args.iter().map(|a| a.form));
        f.finish(wk::APPLY)
    }

    /// The declared field types of `adt`'s variant `variant`, with `args`
    /// substituted for the definition's own type parameters — the instantiated
    /// field types [`Self::construct_form`] takes.
    ///
    /// For a non-generic type (`Sexpr`, an error type) `args` is empty and this
    /// is just the declaration. For a generic one it is the substitution that
    /// makes the answer meaningful at all — see `construct_form`'s doc comment.
    pub(super) fn variant_field_tys(&self, adt: &Path, variant: usize, args: &[Type]) -> Vec<Type> {
        let Some(def) = self.reg.type_def(adt) else { return Vec::new() };
        let Some(v) = def.variants.get(variant) else { return Vec::new() };
        let subst: BTreeMap<String, Type> =
            def.params.iter().cloned().zip(args.iter().cloned()).collect();
        v.fields.iter().map(|t| subst_apply(t, &subst)).collect()
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
    pub(super) fn construct_form(
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
        let path = forms::path_form(f.heap(), type_name);
        f.push(path);
        // The value's runtime identity, spelled here because here is the only
        // place the instantiation is known — the same reason `field_tys` is
        // computed at the site rather than read off the definition. The path
        // above stays for the questions that are about the *definition*
        // (struct or enum, where it was defined); this is the one about the
        // *value* (which instantiation it is).
        let key = ConcreteNames::type_key(&Type::Named(type_name.clone(), targs.to_vec()));
        let key = f.heap().alloc_string(key);
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
    pub(super) fn field_get_form(&self, heap: &mut Heap, obj: Value, idx: usize, field_ty: &Type) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(obj);
        f.push(Value::Int(idx as i64));
        let repr = self.repr_form(f.heap(), field_ty)?;
        f.push(repr);
        f.finish(wk::FIELD_GET)
    }

    /// `(field-set OBJ IDX REPR VALUE)` — see [`Self::field_get_form`].
    pub(super) fn field_set_form(
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

    /// `(match SCRUT REPR (PAT BODY...)...)`.
    ///
    /// The scrutinee's representation is what a whole-value `(pat-bind x)`
    /// binds at, since such a pattern names no type of its own.
    pub(super) fn match_form(
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

    /// `(pat-wild)` / `(pat-bind SYM)` / `(pat-lit LITERAL)` /
    /// `(pat-guard SYM TEST)` /
    /// `(pat-ctor PATH VARIANT DOWNCAST SUB...)` /
    /// `(pat-typetest PATH SUB)`.
    ///
    /// A constructor pattern carries its type, its variant index and whether it
    /// is a downcast — and nothing about its fields' representations. The AST's
    /// `Pattern::Ctor` recorded `sexpr_fields` and `field_types` per pattern
    /// site; a field's representation is a property of the *type*, so the
    /// bridge reads it from that type's own `defstruct`/`defenum` form and the
    /// same fact is recorded once instead of once per site. `Pattern::Bind`'s
    /// inert `bool` disappears here for the same reason it existed —
    /// nothing read it.
    pub(super) fn pattern_form(&self, heap: &mut Heap, pat: &Pattern) -> Result<Value, Error> {
        let form = match pat {
            Pattern::Wildcard => core::tagged(heap, wk::PAT_WILD, &[])?,
            Pattern::Bind(name) => {
                let sym = heap.intern_symbol(name);
                core::tagged(heap, wk::PAT_BIND, &[sym])?
            }
            Pattern::Int(n) => {
                let lit = core::tagged(heap, wk::INT_ANY_WIDTH, &[Value::Int(*n)])?;
                let mut s = RootScope::new(heap);
                s.push_root(lit);
                core::tagged(&mut s, wk::PAT_LIT, &[lit])?
            }
            Pattern::Bool(b) => {
                let lit = core::tagged(heap, wk::BOOL, &[Value::Bool(*b)])?;
                let mut s = RootScope::new(heap);
                s.push_root(lit);
                core::tagged(&mut s, wk::PAT_LIT, &[lit])?
            }
            Pattern::Char(c) => {
                let lit = core::tagged(heap, wk::CHAR, &[Value::Char(*c)])?;
                let mut s = RootScope::new(heap);
                s.push_root(lit);
                core::tagged(&mut s, wk::PAT_LIT, &[lit])?
            }
            Pattern::Guard { name, form } => {
                // The test form is already lowered — it came out of
                // `check_at` — so this only wraps it. It is rooted where it
                // was built (`check_at` pushes every node it produces onto
                // the heap's root stack, and `check_form_at` is what
                // truncates that stack), which is what lets it sit in a
                // `Pattern` on the Rust side across the allocations its
                // siblings make.
                let sym = heap.intern_symbol(name);
                core::tagged(heap, wk::PAT_GUARD, &[sym, *form])?
            }
            // The test is rooted where `check_at` built it, as a guard's is.
            Pattern::When { pat, test } => {
                let mut s = RootScope::new(heap);
                let inner = self.pattern_form(&mut s, pat)?;
                s.push_root(inner);
                core::tagged(&mut s, wk::PAT_WHEN, &[inner, *test])?
            }
            // A niched `Option<T>`'s two shapes (`check_ctor_pattern_fields`).
            // `none` is the empty-list word; `(some P)` is any other word,
            // with `P` applied to the payload read back through `T`'s
            // representation — which the node carries, since the island has
            // to untag a narrow payload and open a float box before `P` can
            // compare or bind it.
            Pattern::Empty => core::tagged(heap, wk::PAT_EMPTY, &[])?,
            Pattern::Some(payload, sub) => {
                let repr = payload.write(heap)?;
                let mut s = RootScope::new(heap);
                s.push_root(repr);
                let inner = self.pattern_form(&mut s, sub)?;
                s.push_root(inner);
                core::tagged(&mut s, wk::PAT_SOME, &[repr, inner])?
            }
            // The empty list is its own pattern node, not `Sexpr`'s `nil` variant.
            // Today the two are the same test; they stop being the same when
            // `nil` leaves `Sexpr` and the empty list becomes `Option<Sexpr>`'s
            // `none` (docs/dev/null-elimination-plan.md §3.2.1) — at which
            // point there is no `nil` variant to name, but there is still an empty
            // list to test for. Splitting the node now is what lets that
            // change touch only the checker's choice of node, not the two
            // consumers that compile one.
            Pattern::Ctor { type_name, variant: 0, args, downcast: false, .. }
                if args.is_empty() && crate::types::path_is_builtin(type_name, "sexpr") =>
            {
                core::tagged(heap, wk::PAT_EMPTY, &[])?
            }
            Pattern::Ctor { type_name, targs, variant, args, field_types, downcast } => {
                let mut f = Items::new(heap);
                let tp = forms::path_form(f.heap(), type_name);
                f.push(tp);
                // The instantiation this pattern accepts, spelled the way the
                // constructing site spells it (`Checker::construct_form`). For
                // an ordinary same-ADT pattern the scrutinee's static type
                // already guarantees a match, so this changes nothing; for a
                // *downcast* it is the whole point — testing the base path
                // alone let a `gen<string>` value into a `(the gen<i32> ...)`
                // arm, which then read its field as an `i32`.
                let key = ConcreteNames::type_key(&Type::Named(type_name.clone(), targs.clone()));
                let key = f.heap().alloc_string(key);
                f.push(key);
                f.push(Value::Int(*variant as i64));
                f.push(Value::Bool(*downcast));
                // Per-field representations, for the same reason
                // `construct` carries them — see `construct_form`.
                let reprs = self.repr_forms(f.heap(), field_types.iter().cloned())?;
                f.push(reprs);
                for sub in args {
                    let one = self.pattern_form(f.heap(), sub)?;
                    f.push(one);
                }
                f.finish(wk::PAT_CTOR)?
            }
            Pattern::TypeTest(ty, sub) => {
                let path = nominal_path(ty)
                    .ok_or_else(|| Error::TypeError(format!("(the `{}` pattern): not a named type", ty)))?;
                let mut f = Items::new(heap);
                let tp = forms::path_form(f.heap(), &path);
                f.push(tp);
                // The full instantiation, not just the ADT path. The comment
                // here used to say a heap box's type name never encodes type
                // arguments — it does now, and this is where that stops being
                // decoration: `(the gen<i32> x)` no longer accepts a
                // `gen<string>` and then reads its field as an `i32`.
                let key = ConcreteNames::type_key(ty);
                let key = f.heap().alloc_string(key);
                f.push(key);
                let one = self.pattern_form(f.heap(), sub)?;
                f.push(one);
                f.finish(wk::PAT_TYPETEST)?
            }
        };
        Ok(forms::rooted(heap, form))
    }

    /// `(lambda ((SYM REPR)...) RET-REPR BODY...)`.
    pub(super) fn lambda_form(
        &self,
        heap: &mut Heap,
        params: &[(String, Type)],
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
    pub(super) fn param_list_form(&self, heap: &mut Heap, params: &[(String, Type)]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        for (name, ty) in params {
            let sym = f.heap().intern_symbol(name);
            let repr = self.repr_form(f.heap(), ty)?;
            let one = core::list(f.heap(), &[sym, repr])?;
            f.push(one);
        }
        f.finish_list()
    }

    /// `(labels ((SYM ((SYM REPR)...) RET-REPR BODY...)...) BODY...)` —
    /// mutually recursive local functions, each of the same shape a `lambda`
    /// has plus a name. See [`LabelsDef`] for one definition's four parts.
    pub(super) fn labels_form(&self, heap: &mut Heap, defs: &[LabelsDef], body: &[Value]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let mut ds = Items::new(f.heap());
        for (name, params, ret, def_body) in defs {
            let mut d = Items::new(ds.heap());
            let sym = d.heap().intern_symbol(name);
            d.push(sym);
            let ps = self.param_list_form(d.heap(), params)?;
            d.push(ps);
            let r = self.repr_form(d.heap(), ret)?;
            d.push(r);
            d.extend(def_body.iter().copied());
            let one = d.finish_list()?;
            ds.push(one);
        }
        let defs = ds.finish_list()?;
        f.push(defs);
        f.extend(body.iter().copied());
        f.finish(wk::LABELS)
    }

    /// `(dyn-new "KEY" TRAIT ((TYPE SYM)...) ((TRAIT ((TYPE SYM)...))...) REPR
    /// FORM)` — boxing a concrete value as a trait object.
    ///
    /// The trailing representation is the boxed value's own, and it cannot be
    /// derived from the concrete type's *name* (all the rest of the node
    /// carries): a struct and an enum differ in whether the island has to root
    /// the value across the boxing call.
    pub(super) fn dyn_new_form(
        &self,
        heap: &mut Heap,
        concrete_key: &str,
        trait_path: &Path,
        slots: &[(Path, String)],
        supers: &[(Path, Vec<(Path, String)>)],
        value_ty: &Type,
        value: Value,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let key = f.heap().alloc_string(concrete_key.to_string());
        f.push(key);
        let tp = forms::path_form(f.heap(), trait_path);
        f.push(tp);
        let slot_list = vtable_form(f.heap(), slots)?;
        f.push(slot_list);
        let mut sup = Items::new(f.heap());
        for (trait_path, slots) in supers {
            let tp = forms::path_form(sup.heap(), trait_path);
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
    pub(super) fn dyn_call_form(
        &self,
        heap: &mut Heap,
        trait_path: &Path,
        method: &str,
        slot: usize,
        impl_targets: &[(Path, String)],
        args: &[Checked],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let tp = forms::path_form(f.heap(), trait_path);
        f.push(tp);
        let m = f.heap().intern_symbol(method);
        f.push(m);
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
    pub(super) fn defun_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        params: &[(String, Type)],
        ret: &Type,
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
        f.push(path);
        let ps = self.param_list_form(f.heap(), params)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        f.extend(body.iter().copied());
        f.finish(wk::DEFUN)
    }

    /// `(defffi PATH C-SYMBOL LIBRARY ((SYM REPR)...) RET-REPR (CTYPE...) RET-CTYPE)`.
    ///
    /// Fields 3-4 are `defun`'s own, so `Interp::exec` reads them back with
    /// the readers it already has: what it registers is an ordinary [`FnDef`]
    /// with an empty body, which is what makes the thunk reachable through the
    /// same `enter` every other call goes through. Parameter names are
    /// synthesized (`a0`, `a1`, ...) — a C declaration has none, and `FnDef`
    /// wants one per parameter.
    ///
    /// The last two fields spell the C types, and are *deliberately* redundant
    /// with the `REPR`s beside them. A `Repr` folds all six integer widths into
    /// `Repr::Narrow` (see its doc comment), which is the right answer for the
    /// question a `Repr` exists to answer — how a value crosses the compiled
    /// boundary — and the wrong one for the thunk, which has to emit
    /// `trunc i64 to i8`. Two questions about one type, the way `field_kind`
    /// and `binding_kind` are. One producer (`Checker::check_defffi`), so they
    /// cannot drift.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn defffi_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        c_symbol: &str,
        library: Option<&str>,
        params: &[Type],
        ret: &Type,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
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
        let named: Vec<(String, Type)> = params
            .iter()
            .enumerate()
            .map(|(i, t)| (format!("a{}", i), if matches!(t, Type::Fn(..)) { Type::Ptr } else { t.clone() }))
            .collect();
        let ps = self.param_list_form(f.heap(), &named)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        let mut cs = Items::new(f.heap());
        for t in params {
            let k = cs.heap().intern_symbol(&ConcreteNames::type_key(t));
            cs.push(k);
        }
        let ctypes = cs.finish_list()?;
        f.push(ctypes);
        let rk = f.heap().intern_symbol(&ConcreteNames::type_key(ret));
        f.push(rk);
        f.finish(wk::DEFFFI)
    }

    /// [`Self::defun_form`] unless this `defun` is *generic*, in which case an
    /// empty `(module PATH)`.
    ///
    /// A generic definition has no core form (see [`TopLevelForm`]): its body
    /// was checked with the type variables still abstract, for
    /// definition-time diagnostics only, and every call site was rewritten to
    /// a monomorphized specialization. Emitting it would leave a silently
    /// callable stale twin behind — which is why the old evaluator opened with
    /// a `type_params.is_empty()` guard on both definition arms. Emitting
    /// nothing instead removes the guard *and* the reason for it: an empty
    /// `module` is already the "this defined a name, there is nothing to run"
    /// form (`deftrait`/`impl` produce one), so no new vocabulary is needed and
    /// nothing downstream has to recognize a body it must refuse to register.
    pub(super) fn definition_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        type_params: &[String],
        params: &[(String, Type)],
        ret: &Type,
        body: &[Value],
    ) -> Result<Value, Error> {
        if !type_params.is_empty() {
            return forms::module_form(heap, name, &[]);
        }
        self.defun_form(heap, name, params, ret, body)
    }

    /// [`Self::defmethod_form`]'s counterpart of [`Self::definition_form`] —
    /// same reasoning, for a generic *owner*'s method.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn method_definition_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: &str,
        instance: bool,
        type_params: &[String],
        params: &[(String, Type)],
        ret: &Type,
        body: &[Value],
    ) -> Result<Value, Error> {
        if !type_params.is_empty() {
            return forms::module_form(heap, type_name, &[]);
        }
        self.defmethod_form(heap, type_name, method, instance, params, ret, body)
    }

    /// `(defmethod PATH SYM INSTANCE ((SYM REPR)...) RET-REPR BODY...)`.
    ///
    /// The receiver is the first parameter for an instance method, not a field
    /// of its own the way `TopLevel::Defmethod::self_name` was: `params` is
    /// what the body binds, in order, and `instance` says whether the first
    /// entry is the receiver.
    pub(super) fn defmethod_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: &str,
        instance: bool,
        params: &[(String, Type)],
        ret: &Type,
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), type_name);
        f.push(path);
        let m = f.heap().intern_symbol(method);
        f.push(m);
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
    pub(super) fn defvar_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        ty: &Type,
        mutable: bool,
        value: Value,
        reassign: bool,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
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
    pub(super) fn defstruct_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        params: &[String],
        fields: &[Type],
        names: &[String],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
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
    pub(super) fn defenum_form(&self, heap: &mut Heap, name: &Path, params: &[String], variants: &[Variant]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
        f.push(path);
        let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
        let names = forms::sym_list(f.heap(), &names)?;
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
