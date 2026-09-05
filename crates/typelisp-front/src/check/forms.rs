//! Core IR node constructors — the `(tag field...)` forms the checker lowers to.
//!
//! These were methods on `Checker`. Every one took `&self` and none read it:
//! building a node is a statement about the IR's shape, not about the checking
//! in progress. Taking a receiver said otherwise to anyone reading
//! `impl Checker`, where 271 methods sat with no line drawn between the ones
//! that need the checker's state and the ones that do not.
//!
//! So the line is here. Nothing in this module can consult the registry, the
//! namespace stack, or the specialization queue, because it has no way to reach
//! them — and a constructor that ever needs one belongs back in the checker.
//!
//! Extracting these was worth more than the lines it moved: several `Checker`
//! methods used `self` *only* to reach a constructor, so each round of moves
//! revealed the next. The two state-free methods left behind
//! (`check_load`/`check_quote`) keep their receiver on purpose — they sit in
//! dispatch tables where every sibling takes one, and a uniform family reads
//! better than a minimal signature.
//!
//! The vocabulary these build is fixed by `tests/core_vocabulary_test.rs`, and
//! the rooting discipline comes from [`crate::check::core`] (see its module doc
//! comment for why a raw `cons` is not enough).

use typelisp_mem::{Error, Heap, Loc, RootScope, Value};

use crate::check::checker::MacroLambda;
use crate::check::core::{self, Checked, Items};
use crate::check::resolved::CompileTarget;
use crate::types::{Path, Type};

/// `(var SYM)` — a reference to a local binding, by symbol identity.
/// **Returns a rooted form** — see [`Self::rooted`].
///
/// Self-rooting, unlike most builders, because a `var`/`global` node is the
/// one kind routinely built *ahead* of the node that will hold it and then
/// kept in a Rust local while that node is assembled: it is the receiver of
/// a method call (`try_field_access`, `check_field_set`,
/// `check_setf_call_place`) or of a `sexpr` retype. `assoc_form` allocates a
/// type path, a method symbol, a `home` list, a return representation and
/// argument representations before `Items::extend` finally pushes the
/// receiver — so an unrooted receiver is collected and its cell recycled
/// into one of those, silently replacing `(var q)` with whatever was built
/// next. Rooting here rather than at each call site makes the rule
/// impossible to forget at the next one; the extra root-stack entry on the
/// ordinary path costs nothing, since `check_form_at`'s `truncate_roots`
/// releases them all at once regardless of how many there were.
pub(super) fn var_form(heap: &mut Heap, name: &str) -> Result<Value, Error> {
    let sym = heap.intern_symbol(name);
    let form = core::tagged(heap, "var", &[sym])?;
    Ok(rooted(heap, form))
}

/// `(set SYM FORM)` — assignment to a local binding.
pub(super) fn set_form(heap: &mut Heap, name: &str, value: Value) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    let sym = f.heap().intern_symbol(name);
    f.push(sym);
    f.push(value);
    f.finish("set")
}

/// `(if C T E)`.
pub(super) fn if_form(heap: &mut Heap, cond: Value, then: Value, els: Value) -> Result<Value, Error> {
    core::tagged(heap, "if", &[cond, then, els])
}

/// `(quote DATUM)` — the datum, unevaluated.
///
/// The one place user data appears in a lowered program, which is what
/// keeps it from ever being read as a node. The datum is the reader's own
/// value, kept as-is: it is a heap value reachable through the node, so
/// unlike the old `QuotedSexpr` there is nothing to copy into
/// heap-independent Rust memory (the collector could not see a raw pointer
/// baked into an `Expr`).
pub(super) fn quote_form(heap: &mut Heap, datum: Value) -> Result<Value, Error> {
    core::tagged(heap, "quote", &[datum])
}

/// `(quote ())` — the empty list as a `sexpr` datum, the base case every
/// list construction ends in.
pub(super) fn quote_nil(heap: &mut Heap) -> Result<Value, Error> {
    quote_form(heap, Value::Empty)
}

/// `(str "TEXT")` — a string literal node the checker synthesizes rather
/// than reads: a `pprint` layout name, a logical block's default affix.
pub(super) fn str_lit_form(heap: &mut Heap, text: &str) -> Result<Value, Error> {
    let v = heap.alloc_string(text.to_string());
    let form = core::tagged(heap, "str", &[v])?;
    Ok(rooted(heap, form))
}

/// Root a finished node before handing it back, and leave it rooted.
///
/// The same contract [`Self::check_at`] follows for every node it produces:
/// released in one place, by `check_form_at`'s `truncate_roots` at the end
/// of the top-level form. A helper that builds a node under an `Items` or
/// `RootScope` has *unrooted* it by the time it returns — the scope pops on
/// drop — so a caller that stores the node in a `Vec` and then calls
/// another builder hands it to code that allocates first: `call_form`,
/// `assoc_form` and `apply_form` all cons their `sym_list`/`repr_forms`
/// *before* rooting their argument forms. That window is where the four
/// historical root leaks in this file lived.
pub(super) fn rooted(heap: &mut Heap, form: Value) -> Value {
    heap.push_root(form);
    form
}

/// `(panic FORM)` — diverges with the message the form evaluates to.
pub(super) fn panic_form(heap: &mut Heap, msg: Value) -> Result<Value, Error> {
    core::tagged(heap, "panic", &[msg])
}

/// The node a *never-executed* position in an erased generic body lowers
/// to: `(panic (str "..."))`.
///
/// The old AST's `TraitCall` node filled this slot. It was diagnostics-only —
/// the evaluator answered it with an internal error and the bridge with
/// `(unsupported "TraitCall")` — because the checker only builds it where a
/// `where`-bounded *type variable* is the receiver, and each specialization
/// re-checks the same site with the type concrete (resolving it to a real
/// `assoc`/`dyn-new`). Since a generic definition emits no body at all
/// (`(module PATH)` and nothing else), this node is discarded rather than
/// run; giving it the vocabulary's own diverging node keeps the "cannot be
/// reached" property expressible without a tag whose only meaning is
/// "unreachable", and preserves today's behavior — an abort naming the
/// method — if the property is ever violated.
pub(super) fn erased_generic_form(heap: &mut Heap, what: &str) -> Result<Value, Error> {
    let msg = str_lit_form(
        heap,
        &format!("`{}` reached the evaluator — an erased generic body executed", what),
    )?;
    let mut s = RootScope::new(heap);
    s.push_root(msg);
    panic_form(&mut s, msg)
}

/// `(loop BODY...)` — loop forever, exited by `break`/`return`.
pub(super) fn loop_form(heap: &mut Heap, body: &[Value]) -> Result<Value, Error> {
    core::tagged(heap, "loop", body)
}

/// `(break)` — leave the nearest enclosing loop with no value.
pub(super) fn break_form(heap: &mut Heap) -> Result<Value, Error> {
    core::tagged(heap, "break", &[])
}

/// `(return)` / `(return FORM)` — leave the nearest enclosing loop.
pub(super) fn return_form(heap: &mut Heap, value: Option<Value>) -> Result<Value, Error> {
    match value {
        Some(v) => core::tagged(heap, "return", &[v]),
        None => core::tagged(heap, "return", &[]),
    }
}

/// `(block NAME BODY...)` — a named escape target, left by
/// `(return-from NAME v)`.
///
/// The name is carried as a plain string field, not as a quoted symbol the way
/// [`catch_form`] carries its tag. The difference is the one that separates the
/// two mechanisms: a `block`'s name is resolved *where it is written* — the
/// checker matches a `return-from` to an enclosing frame at check time, and
/// nothing compares names at run time — whereas a `throw`'s tag is compared
/// against live `catch` frames while unwinding, so it has to survive as a
/// value. Written as a string for the same reason `lambda`'s parameter names
/// are: the island reads it to label a basic block, and never as a datum.
pub(super) fn block_form(heap: &mut Heap, name: &str, body: Value) -> Result<Value, Error> {
    // One body form, not a sequence: the checker wraps the body in a
    // binding-less `let` first, so every consumer — the evaluator, the bridge,
    // the island — has one form to run and none of them re-implements
    // sequencing. `catch` carries its body the same way, for the same reason.
    //
    // `body` is rooted *first*, before anything else here allocates. It
    // arrives from `Checker::let_form`, which builds under an `Items` and so
    // has already unrooted it — and the very next line allocates a string and
    // a node for the name. This is the window `rooted`'s doc comment
    // describes, and this was the fifth leak to live in it: without the root,
    // `gc_stress` reports `push_root given freed cell` from inside `tagged`.
    let body = rooted(heap, body);
    let name = str_lit_form(heap, name)?;
    core::tagged(heap, "block", &[name, body])
}

/// `(return-from NAME)` / `(return-from NAME FORM)` — leave the enclosing
/// [`block_form`] of that name.
pub(super) fn return_from_form(heap: &mut Heap, name: &str, value: Option<Value>) -> Result<Value, Error> {
    let name = str_lit_form(heap, name)?;
    match value {
        Some(v) => core::tagged(heap, "return-from", &[name, v]),
        None => core::tagged(heap, "return-from", &[name]),
    }
}

/// `(catch SYMBOL BODY REPR)` — run `BODY`, and if a `(throw SYMBOL v)` fires
/// anywhere it reaches (through any number of calls), produce `v` instead.
///
/// The symbol is carried as a plain `Sexpr` symbol datum rather than a string
/// so the evaluator can compare it with `eq` the way CL specifies, without
/// interning a second time.
///
/// `REPR` is how the thrown value is represented — the same reason `apply`
/// carries its argument and return reprs. The value crosses a boundary the
/// compiled side cannot read a type from: it is handed to `rt_throw` as one
/// machine word and comes back out of `rt_throw_take_value` as another, and
/// only the repr says whether that word is a tagged `Sexpr` or a raw one.
/// The interpreter ignores the field.
pub(super) fn catch_form(heap: &mut Heap, tag: Value, body: Value, repr: Value) -> Result<Value, Error> {
    core::tagged(heap, "catch", &[tag, body, repr])
}

/// `(throw SYMBOL FORM REPR)` — leave for the nearest dynamically enclosing
/// `catch` on `SYMBOL`, delivering `FORM`'s value as its result. `REPR` is the
/// thrown value's representation, for the reason [`catch_form`]'s doc comment
/// gives.
pub(super) fn throw_form(heap: &mut Heap, tag: Value, value: Value, repr: Value) -> Result<Value, Error> {
    core::tagged(heap, "throw", &[tag, value, repr])
}

/// `(unwind-protect PROTECTED CLEANUP)` — run `PROTECTED`, then `CLEANUP`,
/// whether `PROTECTED` finished normally or left by any non-local exit.
pub(super) fn unwind_protect_form(heap: &mut Heap, protected: Value, cleanup: Value) -> Result<Value, Error> {
    core::tagged(heap, "unwind-protect", &[protected, cleanup])
}

/// `(dyn-upcast TRAIT FORM)`.
pub(super) fn dyn_upcast_form(heap: &mut Heap, to_trait: &Path, value: Value) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    let tp = path_form(f.heap(), to_trait);
    f.push(tp);
    f.push(value);
    f.finish("dyn-upcast")
}

/// `(dyn-value FORM)` — the concrete value inside a trait object, typed as
/// `sexpr` so `match`ing a trait object back down reuses the existing
/// downcast patterns.
pub(super) fn dyn_value_form(heap: &mut Heap, value: Value) -> Result<Value, Error> {
    core::tagged(heap, "dyn-value", &[value])
}

/// `(defmacro PATH (SYM...) REST (REQUIRED (OPT-BODY...) ((SYM OPT-BODY...)...)) PUBLIC BODY...)`.
///
/// A macro is an ordinary callable body — expanding it is calling it — so
/// the only extra structure is how the non-required regions of its lambda
/// list are filled at expansion time.
pub(super) fn defmacro_form(
    heap: &mut Heap,
    name: &Path,
    params: &[String],
    rest: bool,
    lambda: &MacroLambda,
    public: bool,
    body: &[Value],
) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    let path = path_form(f.heap(), name);
    f.push(path);
    let ps = sym_list(f.heap(), params)?;
    f.push(ps);
    f.push(Value::Bool(rest));
    let lam = {
        let mut l = Items::new(f.heap());
        l.push(Value::Int(lambda.required as i64));
        let mut opts = Items::new(l.heap());
        for default in &lambda.optionals {
            let one = core::list(opts.heap(), default)?;
            opts.push(one);
        }
        let opts = opts.finish_list()?;
        l.push(opts);
        let mut keys = Items::new(l.heap());
        for (name, default) in &lambda.keys {
            let mut k = Items::new(keys.heap());
            let sym = k.heap().intern_symbol(name);
            k.push(sym);
            k.extend(default.iter().copied());
            let one = k.finish_list()?;
            keys.push(one);
        }
        let keys = keys.finish_list()?;
        l.push(keys);
        l.finish_list()?
    };
    f.push(lam);
    f.push(Value::Bool(public));
    f.extend(body.iter().copied());
    f.finish("defmacro")
}

/// `(module PATH FORM...)` — a namespace and the forms defined in it.
///
/// Also the grouping device for a definition that expands into several
/// (a `defstruct` and its accessors, a batch of monomorphized
/// specializations): every nested definition's own name is already
/// absolute, so the wrapper only has to ensure the namespace exists.
pub(super) fn module_form(heap: &mut Heap, path: &Path, body: &[Value]) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    let p = path_form(f.heap(), path);
    f.push(p);
    f.extend(body.iter().copied());
    f.finish("module")
}

/// `(use ALIAS TARGET)`.
pub(super) fn use_form(heap: &mut Heap, alias: &Path, target: &Path) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    let a = path_form(f.heap(), alias);
    f.push(a);
    let t = path_form(f.heap(), target);
    f.push(t);
    f.finish("use")
}

/// `(defsignature)` — a forward declaration, which produces no code. The
/// whole of its effect happened while checking (the signature is in the
/// registry), so what reaches `Interp::exec` is an empty marker, the way
/// `(use ...)` is one.
pub(super) fn defsignature_form(heap: &mut Heap) -> Result<Value, Error> {
    core::tagged(heap, "defsignature", &[])
}

/// `(load "PATH")` — recorded, not performed: the checker cannot do file
/// I/O, so the driver reads this and loads the file before checking on.
pub(super) fn load_form(heap: &mut Heap, path: &str) -> Result<Value, Error> {
    let s = heap.alloc_string(path.to_string());
    core::tagged(heap, "load", &[s])
}

/// `(expr FORM)` — a bare top-level expression, to be evaluated.
pub(super) fn expr_form(heap: &mut Heap, form: Value) -> Result<Value, Error> {
    core::tagged(heap, "expr", &[form])
}

/// `(compile-fn (fn (WRITTEN...) (HOME...) PATH))` or
/// `(compile-fn (method PATH SYM (HOME...)))`.
pub(super) fn compile_fn_form(heap: &mut Heap, target: &CompileTarget) -> Result<Value, Error> {
    let payload = compile_target_payload(heap, target)?;
    core::tagged(heap, "compile-fn", &[payload])
}

/// `(trace PAYLOAD...)` / `(untrace PAYLOAD...)`, whose payloads are the same
/// `(fn ..)`/`(method ..)` resolutions `compile-fn` carries — the four forms
/// that name a single body all resolve it the same way, so they all travel in
/// the same shape.
pub(super) fn trace_form(heap: &mut Heap, tag: &str, targets: &[CompileTarget]) -> Result<Value, Error> {
    let mut payloads = Vec::with_capacity(targets.len());
    for t in targets {
        payloads.push(compile_target_payload(heap, t)?);
    }
    core::tagged(heap, tag, &payloads)
}

/// `(disassemble-fn PAYLOAD LLVM)` — one target and whether to print LLVM IR
/// instead of host assembly.
pub(super) fn disassemble_fn_form(heap: &mut Heap, target: &CompileTarget, llvm_ir: bool) -> Result<Value, Error> {
    let payload = compile_target_payload(heap, target)?;
    core::tagged(heap, "disassemble-fn", &[payload, Value::Bool(llvm_ir)])
}

/// `(fn (WRITTEN...) (HOME...) PATH)` or `(method PATH SYM (HOME...))` — one
/// resolved target, as the node payload every form that takes one carries.
fn compile_target_payload(heap: &mut Heap, target: &CompileTarget) -> Result<Value, Error> {
    match target {
        CompileTarget::Fn(r) => {
            let mut f = Items::new(heap);
            let written = sym_list(f.heap(), &r.written)?;
            f.push(written);
            let home = sym_list(f.heap(), &r.home)?;
            f.push(home);
            let path = path_form(f.heap(), &r.resolved);
            f.push(path);
            f.finish("fn")
        }
        CompileTarget::Method { type_name, method, home } => {
            let mut f = Items::new(heap);
            let tp = path_form(f.heap(), type_name);
            f.push(tp);
            let m = f.heap().intern_symbol(method);
            f.push(m);
            let home = sym_list(f.heap(), home)?;
            f.push(home);
            f.finish("method")
        }
    }
}

/// A `Never`-typed placeholder for a sub-expression that failed to check.
///
/// Reuses `panic` (already `Never`-typed, already handled by every
/// downstream consumer) rather than adding a tag of its own, so recovery
/// needs no changes outside the checker. `Type::Never` unifies with any
/// expected type, so a hole flowing into a typed position never produces a
/// cascade of follow-on errors. `loc` is recorded on the node so
/// `locate_node` can still land the cursor on it for completion/hover.
pub(super) fn hole(heap: &mut Heap, loc: Option<Loc>) -> Result<Checked, Error> {
    let msg = heap.alloc_string("<check-error>".to_string());
    let mut f = Items::new(heap);
    let text = core::tagged(f.heap(), "str", &[msg])?;
    f.push(text);
    let form = f.finish_at("panic", loc)?;
    Ok(Checked::new(form, Type::Never))
}

/// Rebuild an `&optional`/`&key` default's checked form into *this* call
/// site's own cells — the read side of [`Self::check_opt_key_default`].
///
/// A fresh subtree per site, not a shared one: the sites are independent
/// programs and a shared subtree would give them one identity. `decl_ty` is
/// the type the default was checked against, so it is the type the spliced
/// argument has (`OptKeyParam::effective_ty` is `decl_ty` exactly when a
/// default exists).
pub(super) fn splice_default(
    heap: &mut Heap,
    default: &crate::owned_form::OwnedForm,
    decl_ty: &Type,
) -> Result<Checked, Error> {
    let form = crate::owned_form::owned_to_value(heap, default)?;
    Ok(Checked::new(rooted(heap, form), decl_ty.clone()))
}

/// Emit a [`Type`] as the type *syntax* that parses back to it — the inverse
/// of [`crate::types::parse_type`], and the reason
/// [`crate::types::parse_applied_type`]'s list spelling of a generic exists.
///
/// A generic comes out applied (`(vector char)`) rather than as the name
/// `vector<char>`, because the arguments here are arbitrary `Type`s and the
/// name grammar cannot spell all of them: `Vector<(fn (i32) i32)>` has no
/// written form at all. Emitting the list form makes the round trip total,
/// which is what `Checker::subst_value` needs to substitute a trait's
/// associated type into a signature no matter what the `impl` bound it to.
///
/// Every nested result is rooted as it is produced: each sibling's emission
/// allocates, and a `Vec<Value>` is invisible to the collector.
pub(super) fn type_to_form(heap: &mut Heap, t: &Type) -> Result<Value, Error> {
    match t {
        Type::Unit => Ok(Value::Empty),
        // Not in `prim_type_path` (`!` has no method table), so spelled here.
        Type::Never => Ok(heap.intern_symbol("!")),
        Type::Named(p, args) if args.is_empty() => Ok(type_name_value(heap, p)),
        Type::Named(p, args) => {
            let head = type_name_value(heap, p);
            let mut s = RootScope::new(heap);
            s.push_root(head);
            let mut items: Vec<(Value, Option<Loc>)> = vec![(head, None)];
            for a in args {
                let f = type_to_form(&mut s, a)?;
                s.push_root(f);
                items.push((f, None));
            }
            list_from_vec_locs(&mut s, &items)
        }
        // The reader's joined `(:dyn Trait)` form. The associated-type pins
        // ride along as the head's arguments, which is where
        // `types::parse_dyn_type` reads them back out of.
        Type::Dyn(p, pins) => {
            let head = heap.intern_symbol(":dyn");
            let mut s = RootScope::new(heap);
            s.push_root(head);
            let inner = type_to_form(&mut s, &Type::Named(p.clone(), pins.clone()))?;
            s.push_root(inner);
            list_from_vec_locs(&mut s, &[(head, None), (inner, None)])
        }
        Type::Fn(params, rest, ret) => {
            let fn_sym = heap.intern_symbol("fn");
            let mut s = RootScope::new(heap);
            s.push_root(fn_sym);
            let mut ps: Vec<(Value, Option<Loc>)> = Vec::with_capacity(params.len() + 2);
            for p in params {
                let f = type_to_form(&mut s, p)?;
                s.push_root(f);
                ps.push((f, None));
            }
            if let Some(r) = rest {
                let marker = s.intern_symbol("&rest");
                s.push_root(marker);
                let f = type_to_form(&mut s, r)?;
                s.push_root(f);
                ps.push((marker, None));
                ps.push((f, None));
            }
            let param_list = list_from_vec_locs(&mut s, &ps)?;
            s.push_root(param_list);
            let ret_form = type_to_form(&mut s, ret)?;
            s.push_root(ret_form);
            list_from_vec_locs(&mut s, &[(fn_sym, None), (param_list, None), (ret_form, None)])
        }
        prim => {
            let p = crate::types::prim_type_path(prim)
                .expect("every remaining `Type` variant is a primitive with a path");
            Ok(type_name_value(heap, &p))
        }
    }
}

/// A type name as the reader would have produced it: one symbol for a plain
/// name, an interned path for a `::`-qualified one. Both tables are
/// permanent, so the result needs no rooting of its own.
fn type_name_value(heap: &mut Heap, p: &Path) -> Value {
    if p.is_simple() {
        heap.intern_symbol(p.last_segment())
    } else {
        Value::Path(crate::types::intern_path_id(heap, p))
    }
}

/// Build a proper list `Value` from `items`, in order — the inverse of
/// `heap.list_to_vec`, used by `Checker::check_impl` to reassemble a
/// receiver/parameter form after substituting just its type position.
/// Build a list, tagging each spine cell with its element's source span
/// (`Heap::set_elem_loc`) — used by `check_impl`'s method-signature
/// rebuild so the elements carried over unchanged keep the positions
/// they were read with.
pub(super) fn list_from_vec_locs(heap: &mut Heap, items: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    // Both halves of this need rooting across the conses below, since every
    // `cons` can collect: the partially-built tail, and the items
    // themselves — an item is often a freshly substituted form
    // (`Self::subst_value`) reachable from nowhere else yet.
    let mut s = RootScope::new(heap);
    for (item, _) in items {
        s.push_root(*item);
    }
    let mut out = Value::Empty;
    for (item, loc) in items.iter().rev() {
        s.push_root(out);
        // core-build-ok: rebuilds read syntax, not a core node, so it needs
        // per-cell `set_elem_loc` that `core::list` does not do. Both the
        // items and the growing tail are rooted in `s` above.
        out = s.cons(*item, out)?;
        if let (Value::Cons(cr), Some(l)) = (out, loc) {
            s.set_elem_loc(cr, l.clone());
        }
    }
    Ok(out)
}

/// Wraps `body` in `(let* ((name0 val0) (name1 val1) ...) body)`, or
/// returns `body` unchanged when `bindings` is empty — the sequential-
/// binding counterpart `Self::place_dedup`'s temporaries need (a later
/// binding's `val` may itself reference the current place's own earlier
/// temporaries, e.g. `Self::check_rotatef_shiftf`'s value-reading
/// bindings reference the just-bound subform temporaries).
pub(super) fn wrap_let_star(heap: &mut Heap, bindings: Vec<(Value, Value)>, body: Value) -> Result<Value, Error> {
    if bindings.is_empty() {
        return Ok(body);
    }
    let mut binding_pairs = Vec::with_capacity(bindings.len());
    for (name, val) in bindings {
        let pair = list_from_vec_locs(heap, &[(name, None), (val, None)])?;
        binding_pairs.push((pair, None));
    }
    let bindings_list = list_from_vec_locs(heap, &binding_pairs)?;
    let let_star_sym = heap.intern_symbol("let*");
    list_from_vec_locs(heap, &[(let_star_sym, None), (bindings_list, None), (body, None)])
}

/// The 0-argument identity element for one of CL's variadic
/// arithmetic/bitwise operators: `(+) = 0`, `(*) = 1`, `(logior) =
/// (logxor) = 0`, `(logand) = -1` (all bits set — the identity for
/// AND). `value` is the caller-supplied integer form of that constant;
/// this just picks *which* numeric type's literal to build it as.
/// Follows `expected` when it names one of this language's five numeric
/// types, falling back to `i32` otherwise (a bare `Value::Int` — this
/// language's own default for a context-free integer literal, see
/// `Checker::int_lit_ty`) — there being no argument to infer a type from
/// is exactly the situation that fallback exists for. Builds a
/// heap-boxed float/bignum/ratio directly (`Heap::alloc_f64`/`alloc_f32`/
/// `alloc_bignum`/`alloc_ratio`) rather than a `(int->bignum 0)`-style
/// call form — the same representation the reader itself produces for a
/// literal, so `Self::check`'s existing `Value::Boxed` handling picks up
/// the right type with no special-casing needed here.
pub(super) fn numeric_identity_literal(heap: &mut Heap, expected: Option<&Type>, value: i64) -> Value {
    match expected {
        Some(Type::F64) => heap.alloc_f64(value as f64),
        Some(Type::F32) => heap.alloc_f32(value as f32),
        Some(Type::Bignum) => heap.alloc_bignum(num_bigint::BigInt::from(value)),
        Some(Type::Ratio) => heap.alloc_ratio(num_rational::BigRational::from_integer(num_bigint::BigInt::from(value))),
        _ => Value::Int(value),
    }
}

/// A path, as a core form field writes one: a bare symbol for a single
/// segment, a `::` path for more.
///
/// The asymmetry is the reader's, not a choice made here — reading `point`
/// yields a `Value::Symbol` and only `a::b` yields a `Value::Path`. Writing a
/// one-segment path as a `Value::Path` would print as `point` and read back as
/// a symbol, so a form would not survive its own round trip
/// (`tests/core_vocabulary_test.rs`). Both consumers accept either spelling.
pub(super) fn path_form(heap: &mut Heap, p: &Path) -> Value {
    let segs = p.segments();
    if segs.len() == 1 {
        return heap.intern_symbol(&segs[0]);
    }
    let ids: Vec<_> = segs.iter().map(|s| match heap.intern_symbol(s) {
        Value::Symbol(id) => id,
        _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
    }).collect();
    heap.intern_path(&ids)
}

/// `(a b c)` — a list of symbols, for a reference's `written`/`home` name
/// lists.
///
/// `home` is often empty: the root module has zero segments, which is exactly
/// why these are lists of symbols rather than a `Path` (always at least one
/// segment).
pub(super) fn sym_list(heap: &mut Heap, names: &[String]) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    for n in names {
        let sym = f.heap().intern_symbol(n);
        f.push(sym);
    }
    f.finish_list()
}
