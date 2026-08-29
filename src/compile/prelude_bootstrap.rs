//! The committed *prelude dump*: `crates/typelisp-front/src/prelude.typld`,
//! holding both halves of compiling the prelude — the checked state its
//! definitions produced, and the native bodies for every one that can have a
//! body.
//!
//! The same shape as [`crate::compile::bootstrap`]'s compiler island —
//! [`build_prelude_artifact`] compiles into one shared module and writes the
//! pair, `src/bin/bootstrap_prelude.rs` commits the file, [`load`] applies it —
//! with three differences worth naming.
//!
//! **This is about both execution speed and load time.** The bitcode half is
//! what makes a later call to `gcd` or `abs` run compiled instead of
//! tree-walked. The checked-state half is what removes the read and the type
//! check from every startup: 27ms + 136ms of a 1.50s `typl` startup, measured
//! by `typl-bench-prelude`.
//!
//! **Not everything can be precompiled, and the boundary is not a judgement
//! call.** A generic definition has no single body to compile — the checker
//! monomorphizes per use site — and it reaches [`collect_item`] as an empty
//! `(module PATH)`, which flattens to nothing. So "compile whatever survives
//! collection" *is* the rule, with no genericity test of its own. What it
//! leaves out is most of the list/`Iter`/pathname/stream library (61 of the
//! prelude's 86 `defun`s are generic); what it takes in is the whole numeric
//! method catalog and the concrete-receiver `impl`s.
//!
//! **There is no snapshot chain here.** The island's generator installs the
//! previous island `.bc` because it is compiling *itself*. The prelude is
//! compiled by the island, which is already native, so this generator loads
//! the prelude purely interpreted ([`crate::prelude::load_interpreted`]) and
//! never reads the artifact it is about to replace. That also keeps the
//! dependency one-directional — `prelude_compiled.bc` is built with
//! `compiler_island.bc`, and `compiler_island.bc` needs only an interpreted
//! prelude — instead of the two artifacts each requiring the other.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use inkwell::AddressSpace;

use crate::check::core;
use crate::compile::symbols::CompiledItem;
use crate::eval::interp::Uncompilable;
use crate::{EvalError, Heap, Interp, Path, Value};

/// Applies the committed prelude dump: its checked state, its definitions, and
/// the native bodies over them.
///
/// This is what `typelisp::load_prelude` names. `interp` must be freshly
/// [`Interp::new`]'d — the unit's globals are created here, in the order they
/// were compiled against, and an `Interp` that has already promoted something
/// would number them differently.
///
/// The source being fixed and the dump a committed, digest-checked artifact,
/// any failure here is a build/bug condition rather than a user error — hence
/// the panics.
pub fn load(heap: &mut Heap, chk: &mut crate::Checker, interp: &mut Interp) {
    let units = typelisp_front::dump::parse(crate::prelude::DUMP, "prelude")
        .unwrap_or_else(|e| panic!("prelude: {}", e));
    let unit = units.first().unwrap_or_else(|| panic!("prelude: the committed dump holds no units"));
    let state =
        crate::compile::dump::read_types(unit, "prelude").unwrap_or_else(|e| panic!("prelude: {}", e));
    // Before anything is applied: a dump built from a different `SOURCE` than
    // the one compiled into this binary would install definitions the source
    // no longer has, and leave an edit looking like it did nothing.
    typelisp_front::dump::verify_sources_digest(&state, crate::prelude::DUMPED_SOURCES, REGEN_SCRIPT)
        .unwrap_or_else(|e| panic!("{}", e));
    crate::compile::dump::load_unit(heap, chk, interp, state, unit.bitcode)
        .unwrap_or_else(|e| panic!("prelude: {}", e));
    // Remembered so `(dump ...)` can re-emit this unit ahead of the session's
    // own; borrowed, since it is a static in this binary.
    interp.push_dump_source(std::borrow::Cow::Borrowed(crate::prelude::DUMP));
}

/// What an AOT executable needs from the prelude on top of the definitions
/// [`load`] installs — see [`load_for_aot`].
pub struct AotPrelude {
    /// The committed prelude bitcode, to be linked into the executable's own
    /// module so a call to `abs` resolves to a body rather than to nothing.
    pub bitcode: &'static [u8],
    /// Each prelude `defvar`, in the order its compiled slot id was assigned.
    /// The executable re-runs these at its own startup: the compiled bodies
    /// address their globals by baked-in slot id, so the storage has to exist,
    /// with the same numbering, before any of them runs.
    pub global_inits: Vec<(Path, Value)>,
}

/// [`load`], plus what an AOT executable needs to *carry* the prelude rather
/// than borrow this process's copy of it.
///
/// The difference between the two loaders is the difference between a JIT and
/// an executable. `load` installs the bitcode's bodies as addresses in this
/// process ([`crate::compile::driver::install_compiled_library`]); an
/// executable has no process to install into, so it gets the bitcode itself,
/// linked into its module, and a startup sequence for the state those bodies
/// assume. Today that state is the globals; the forms are read out here rather
/// than reconstructed later because the dump is parsed exactly once.
///
/// The returned forms are permanently rooted — every caller is a one-shot
/// compile against a throwaway heap, and they have to outlive a load that
/// pushes and pops the ordinary root stack throughout.
pub fn load_for_aot(heap: &mut Heap, chk: &mut crate::Checker, interp: &mut Interp) -> Result<AotPrelude, String> {
    let units = typelisp_front::dump::parse(crate::prelude::DUMP, "prelude")?;
    let unit = units.first().ok_or_else(|| "prelude: the committed dump holds no units".to_string())?;
    let state = crate::compile::dump::read_types(unit, "prelude")?;
    typelisp_front::dump::verify_sources_digest(&state, crate::prelude::DUMPED_SOURCES, REGEN_SCRIPT)?;

    // Taken off the state before `load_unit` consumes it, and matched against
    // the recorded `globals` afterwards: this list decides the order the
    // executable initializes them in, and the ids in the bitcode are only
    // correct if that order is the one they were compiled against.
    let mut global_inits: Vec<(Path, Value)> = Vec::new();
    for f in &state.forms {
        let tl = crate::owned_form::owned_to_value(heap, f).map_err(|e| e.to_string())?;
        if !core::op_is(heap, tl, typelisp_mem::wk::DEFVAR) {
            continue;
        }
        // A *permanent* root, not `push_root`: these have to survive the whole
        // of `load_unit` and a compiler-island load after it, and the ordinary
        // root stack is a strict LIFO that every one of those callers pushes
        // and pops on. Rooted the LIFO way, they were collected out from under
        // the caller and turned up later as "a global initializer was built
        // from something that is not a `defvar`" — the cell had been recycled.
        heap.push_permanent_root(tl);
        let path =
            core::path_field(heap, tl, 0).ok_or_else(|| "prelude: defvar without a name".to_string())?;
        global_inits.push((path, tl));
    }

    let globals = state.globals.clone();
    // Types and definitions only: the bodies go into the *executable* (the
    // caller links `bitcode` into its module), so JIT-installing them here
    // would compile every prelude body a second time, per `compile-file`, for
    // addresses this process never calls.
    crate::compile::dump::load_unit_types_only(heap, chk, interp, state)?;

    // `load_unit` already refuses a numbering it cannot reproduce; what this
    // adds is that *this* list is that numbering. A `defvar` the walk above
    // missed would otherwise show up as an executable whose prelude globals
    // are one slot off — compiled code reading somebody else's storage, with
    // no error anywhere.
    if globals.len() != global_inits.len()
        || globals.iter().zip(&global_inits).any(|((name, _), (path, _))| name != &path.to_string())
    {
        return Err(format!(
            "prelude: the dump records {} global(s) but its forms hold {} `defvar`(s), or they are \
             in a different order — an AOT executable cannot reproduce the slot numbering the \
             bitcode was compiled against",
            globals.len(),
            global_inits.len()
        ));
    }

    Ok(AotPrelude { bitcode: unit.bitcode, global_inits })
}

/// What to run when the committed dump no longer matches `SOURCE`.
pub use crate::prelude::REGEN_SCRIPT;

/// [`crate::prelude::load_interpreted`], collecting along the way everything a
/// compiled artifact has to know about the prelude.
///
/// Two callers need exactly this and not the install: this module's
/// [`build_prelude_bitcode`], which is about to *produce* the artifact, and
/// the island generator ([`crate::compile::bootstrap::build_island_bitcode`]),
/// which needs prelude definitions in scope but must not depend on the
/// prelude artifact — that dependency in both directions is a chicken-and-egg
/// neither script could break.
pub fn load_interpreted_plan(heap: &mut Heap, chk: &mut crate::Checker, interp: &mut Interp) -> PreludePlan {
    let mut plan = PreludePlan::default();
    // Two separate locals, not two closures over `plan`: the callbacks are
    // live at the same time, so each has to capture something the other does
    // not touch.
    let mut source_hash: u64 = 0;
    crate::prelude::load_interpreted_with(
        heap,
        chk,
        interp,
        &mut |heap, forms| {
            source_hash = crate::compile::bootstrap::hash_read_forms(heap, forms)
                .expect("prelude: hashing the read forms failed");
        },
        &mut |heap, tl| {
            // Rooted and never popped: the generator writes these into the
            // dump long after the load, with a whole island load in between.
            // Both callers are one-shot generators against a throwaway heap.
            heap.push_root(tl);
            plan.forms.push(tl);
            collect_item(heap, tl, &mut plan).expect("prelude: collect failed");
        },
    );
    plan.source_hash = source_hash;
    plan
}

/// The gaps in what the compile path can lower, each with what it costs —
/// `(target name, what stays interpreted because of it)`.
///
/// **Empty, and that is the interesting part.** Every prelude definition that
/// survives [`collect_item`] — every non-generic one — now has a compiled
/// body. It listed twenty targets when the precompiled prelude was first
/// built: `char::char->string`, the ten stream/file builtins, the bitwise
/// bignum primitives, `bool::equal`/`symbol::eq`/`string::substring`, and
/// three free builtins. Between them they held 111 definitions interpreted;
/// closing them was the 2026-08-14 "コンパイル経路の穴" work
/// ([implementation-log.md](../../../docs/dev/implementation-log.md)).
///
/// Keep the list — and the reconcile check below — rather than deleting both:
/// its job now is to fail the build the moment a *new* gap appears. Add a
/// prelude definition that reaches a builtin nothing lowers and the generator
/// stops with that target named, instead of quietly shipping an artifact
/// missing a body that used to be there.
///
/// Recording *targets* rather than definitions stays right for the same
/// reason it was: a list of definitions is a list of consequences, five times
/// longer, and silent about what would actually have to be built.
///
/// [`build_prelude_bitcode`] recomputes the set on every build
/// ([`Interp::precheck_compilable`]) and fails unless it matches this list
/// exactly, printing what it found — in both directions, so an entry that
/// stops blocking anything is as loud as one that starts.
pub const PRELUDE_COMPILE_UNSUPPORTED: &[(&str, &str)] = &[];

/// What one pass over the prelude source yields: the definitions with
/// compilable bodies, and the globals whose compiled-slot ids have to be
/// assigned in a reproducible order.
///
/// Produced by the same code path at generation time and at load time — that
/// is the whole point. The ids `Interp::promote_global` hands out are baked
/// into the emitted IR as constants, so the generator's numbering and the
/// loader's must agree; deriving both from one walk of one source is what
/// makes them agree by construction rather than by two lists someone keeps in
/// sync.
#[derive(Default)]
pub struct PreludePlan {
    /// Every `defun`/`defmethod` with a body to compile, in declaration order.
    pub items: Vec<CompiledItem>,
    /// Every `defvar`/`defconstant`, in declaration order.
    pub globals: Vec<Path>,
    /// Every checked top-level form, in declaration order, rooted for the
    /// lifetime of the heap it was loaded into — the dump's checked-state half.
    pub forms: Vec<Value>,
    /// The staleness key for the forms this plan came from
    /// ([`crate::compile::bootstrap::hash_read_forms`]): what the generator
    /// embeds in the artifact and the loader checks it against.
    ///
    /// Carried here rather than recomputed by each side from
    /// `island_source_hash(prelude::SOURCE)`, which would read the prelude a
    /// second time into a second 256K-cell `Heap` — at every startup, for a
    /// parse that just happened.
    pub source_hash: u64,
}

impl PreludePlan {
    /// The items worth emitting: everything collected, minus whatever
    /// [`Interp::precheck_compilable`] rejects.
    ///
    /// Generator-side only. The *loader* does not repeat this walk — it hands
    /// `install_compiled_library` every collected item and lets the artifact
    /// answer, since a module defines exactly the bodies that were emitted
    /// into it. Asking the module is both cheaper (a symbol lookup against a
    /// startup-time budget, versus a transitive call-graph walk per
    /// definition) and impossible to disagree with, which a second run of the
    /// same predicate is not.
    pub fn compilable(&self, heap: &Heap, interp: &Interp) -> Vec<CompiledItem> {
        self.items
            .iter()
            .filter(|item| crate::compile::driver::precheck_compilable(&interp, heap, &item.node_name()).is_ok())
            .cloned()
            .collect()
    }
}

/// Records what [`PreludePlan`] must know about one checked top-level form.
///
/// Recurses into a `(module ...)`, which covers the `impl` block grouping
/// `Checker::check_impl` returns and the monomorphization bundle a generic
/// instantiation arrives in. A generic template — and a `deftrait`, which
/// lowers the same way — is an *empty* `(module PATH)`, so both flatten to
/// nothing here without a test of their own.
pub(crate) fn collect_item(heap: &Heap, tl: Value, plan: &mut PreludePlan) -> Result<(), String> {
    let tag = core::op(heap, tl).map(str::to_string).unwrap_or_default();
    match tag.as_str() {
        "module" => {
            let body = core::fields(heap, tl).map_err(|e| e.to_string())?;
            for item in body.into_iter().skip(1) {
                collect_item(heap, item, plan)?;
            }
        }
        "defun" => {
            let path = core::path_field(heap, tl, 0).ok_or_else(|| "prelude: defun without a name".to_string())?;
            plan.items.push(CompiledItem::Fn(path));
        }
        "defmethod" => {
            let type_path = core::path_field(heap, tl, 0).ok_or_else(|| "prelude: defmethod without a type".to_string())?;
            let method = match core::field(heap, tl, 1) {
                Some(Value::Symbol(id)) => heap.symbol_name(id).to_string(),
                _ => return Err("prelude: defmethod without a name".to_string()),
            };
            plan.items.push(CompiledItem::Method(type_path, method));
        }
        // `defconstant` lowers to the same core form.
        "defvar" => {
            let path = core::path_field(heap, tl, 0).ok_or_else(|| "prelude: defvar without a name".to_string())?;
            plan.globals.push(path);
        }
        // A macro body is an ordinary `Sexpr -> Sexpr` function — expanding it
        // *is* calling it, and `exec` registers it in the same `fns` table a
        // `defun` goes in — so it compiles like one and belongs in the
        // artifact. Without this the expander stayed interpreted forever,
        // which is the half of "macros can be compiled" the implementation was
        // missing (the other half, "expand before compiling the expansion",
        // was already true and then some: expansion finishes at check time,
        // before any codegen runs).
        "defmacro" => {
            let path = core::path_field(heap, tl, 0).ok_or_else(|| "prelude: defmacro without a name".to_string())?;
            plan.items.push(CompiledItem::Fn(path));
        }
        // No codegen of their own. `exec` still registers what they define:
        // an enum's variants and a struct's field representations, which the
        // compile bridge reads back through `Interp::compile_definitions`.
        "defstruct" | "defenum" | "use" => {}
        other => {
            return Err(format!(
                "prelude: top-level `{}` has no place in the compiled artifact — \
                 the prelude is definitions only (defun/defmethod/defvar/defconstant/\
                 defstruct/defenum/defmacro/deftrait/impl/module)",
                other
            ))
        }
    }
    Ok(())
}

/// Assigns every prelude global its compiled-slot id, in declaration order,
/// and returns the assignment for the dump to record.
///
/// Must run at the same point on both sides — right after the prelude's forms
/// are `exec`'d, before anything touches bitcode — because the ids are baked
/// into the artifact as constants and `typelisp_rt`'s table hands them out
/// sequentially from zero per `Interp`. Eager and in order rather than lazily
/// on first reference, for exactly the reason `compile::aot::compile_file`
/// promotes its `defvar`s eagerly: a numbering that depends on *which bodies
/// happened to be compiled first* is not a numbering a second process can
/// reproduce.
pub fn promote_globals(
    heap: &mut Heap,
    interp: &Interp,
    plan: &PreludePlan,
) -> Result<Vec<(String, usize)>, String> {
    let mut out = Vec::with_capacity(plan.globals.len());
    for path in &plan.globals {
        let id =
            interp.promote_global(heap, path).map_err(|e| format!("prelude: promoting `{}`: {}", path, e))?;
        out.push((path.to_string(), id));
    }
    Ok(out)
}

/// Fails unless [`PRELUDE_COMPILE_UNSUPPORTED`] names exactly the targets that
/// actually block compilation, printing what was found when it doesn't.
///
/// A rejection that is *not* a missing target ([`Uncompilable::Other`]) can
/// never be waved through: it means the compile path broke on a shape rather
/// than on a known gap, and there is no honest entry to make for it.
fn reconcile_unsupported(heap: &Heap, interp: &Interp, plan: &PreludePlan) -> Result<(), String> {
    let mut blockers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut broken: Vec<(String, EvalError)> = Vec::new();
    for item in &plan.items {
        let node = item.node_name();
        match crate::compile::driver::precheck_compilable(&interp, heap, &node) {
            Ok(()) => {}
            Err(Uncompilable::MissingTarget(target)) => blockers.entry(target).or_default().push(node),
            Err(Uncompilable::Other(e)) => broken.push((node, e)),
        }
    }

    let listed: Vec<&str> = PRELUDE_COMPILE_UNSUPPORTED.iter().map(|(t, _)| *t).collect();
    let unlisted: Vec<&String> = blockers.keys().filter(|t| !listed.contains(&t.as_str())).collect();
    let gone: Vec<&str> = listed.iter().copied().filter(|t| !blockers.contains_key(*t)).collect();
    if broken.is_empty() && unlisted.is_empty() && gone.is_empty() {
        return Ok(());
    }

    let mut msg = String::new();
    for (node, e) in &broken {
        msg.push_str(&format!("prelude: compiling `{}` broke on its own shape: {}\n", node, e));
    }
    if !unlisted.is_empty() || !gone.is_empty() {
        msg.push_str("prelude: PRELUDE_COMPILE_UNSUPPORTED does not match what the compiler can do.\n");
        for target in &unlisted {
            msg.push_str(&format!("  blocks compilation, but not listed: {}\n", target));
        }
        for target in &gone {
            msg.push_str(&format!("  listed, but blocks nothing now: {}\n", target));
        }
        msg.push_str("\nThe current set, ready to paste into PRELUDE_COMPILE_UNSUPPORTED:\n");
        for (target, blocked) in &blockers {
            msg.push_str(&format!("    ({:?}, {:?}), // {} definition(s)\n", target, blocked[0], blocked.len()));
        }
    }
    Err(msg)
}

/// Builds the committed prelude dump in a throwaway environment: the checked
/// state the prelude's definitions produce, and the bitcode holding their
/// bodies, written as one file.
///
/// The checker delta is captured **before the island is loaded**, which is the
/// one ordering constraint here that is not obvious: both load into the same
/// `Checker`, and afterwards nothing can say which of them added what.
///
/// Mirrors [`crate::compile::bootstrap::build_island_bitcode`]: one shared
/// module, `rt_*` forward declarations, every body forward-declared before any
/// is translated (the prelude's definitions reference each other freely — the
/// numeric catalog's `gcd` calls `abs`, `signum` calls `abs`), then one
/// `add_compiled_function` per item. No `main` wrapper and no
/// `add_global_mapping`: this is a library of compiled functions, and the
/// `rt_*` addresses are supplied at install time by
/// `Interp::install_compiled_library`.
pub fn build_prelude_artifact() -> Result<Vec<u8>, String> {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = crate::Checker::new();
    let mut interp = Interp::new();

    // Interpreted, deliberately: see this module's doc comment on why there is
    // no snapshot chain.
    let before = chk.signature(&heap)?;
    let plan = load_interpreted_plan(&mut heap, &mut chk, &mut interp);
    let globals = promote_globals(&mut heap, &interp, &plan)?;
    let delta = chk.capture_delta(&heap, &before)?;
    // The island is what actually translates the bodies below, so it has to be
    // native before the first `add_compiled_function` call — and after the
    // delta above, whose subject is the prelude alone.
    crate::load_compiler(&mut heap, &mut chk, &mut interp);

    // Decide what is in before emitting anything: the island's `compile-call`
    // aborts the process on an undeclared callee rather than returning an
    // error, so "try it and see" is not available. See
    // `Interp::precheck_compilable`.
    reconcile_unsupported(&heap, &interp, &plan)?;
    let items = plan.compilable(&heap, &interp);

    let ctx = crate::compile::llvm_context();
    let module = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("prelude_compiled");
        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        for (name, _) in crate::compile::externs::rt_extern_functions() {
            module.add_function(name, fn_ty, None);
        }
        for item in &items {
            let sym = item.symbol_name();
            if module.get_function(&sym).is_none() {
                module.add_function(&sym, fn_ty, None);
            }
        }
        Rc::new(RefCell::new(module))
    };

    // `add_compiled_function` locks `COMPILE_LOCK` per LLVM builtin call (via
    // `eval_llvm_builtin_method`) and `Mutex` isn't reentrant, so it must run
    // without the lock held — the same constraint `aot::compile_file` and the
    // island generator both document at their own loops.
    for item in &items {
        let node = item.node_name();
        crate::compile::driver::add_compiled_function(&interp, &mut heap, module.clone(), &node, &item.symbol_name()).map_err(|e| {
            format!(
                "prelude: compiling `{}` failed: {} — the precheck said it was fine, so this is a \
                 gap the call-graph walk cannot see rather than a missing lowering to record in \
                 PRELUDE_COMPILE_UNSUPPORTED",
                node, e
            )
        })?;
    }

    // Trait-object tables are the one thing a library artifact cannot carry as
    // it stands: `dyn-new` bakes a vtable id and `dyn-upcast` a trait id, and
    // both are assigned per boxing/upcast site during translation, so a
    // reproducible numbering would need the same eager, ordered replay
    // `promote_globals` does for globals — plus, for vtables, a way to publish
    // the addresses (`compile::aot::build_main_wrapper` emits `rt_vtable_set`
    // calls; a JIT-installed library has no startup sequence to put them in).
    // No prelude body reaches either today: `as-dyn-error` is generic, and the
    // stream combinators take already-boxed `:dyn` values rather than boxing
    // any. A `dyn-call` bakes only a slot index, so dispatching on `:dyn` — the
    // composed streams do — is fine.
    //
    // Checked rather than assumed, because the failure it guards is silent: a
    // baked id that means something different at load time reads the wrong
    // vtable and calls the wrong function.
    if !interp.vtable_descriptors().is_empty() || !interp.upcast_descriptors().is_empty() {
        return Err(
            "prelude: a compiled body boxes or upcasts a trait object, whose vtable/trait ids are \
             baked in per site — the artifact needs an ordered replay of those tables at load time \
             before this can be shipped (see this function's comment)"
                .to_string(),
        );
    }

    let bitcode = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let bitcode = {
            let m = module.borrow();
            m.verify().map_err(|e| format!("prelude module failed verification: {}", e)).map(|()| {
                // Carries a trailing NUL by design — see the identical call in
                // `bootstrap.rs` for why it must not be trimmed.
                m.write_bitcode_to_memory().as_slice().to_vec()
            })
        };
        // Destroyed with the guard still held — see the same `drop` in
        // `bootstrap::build_island_bitcode`.
        drop(module);
        bitcode?
    };

    let state = typelisp_front::dump::capture_types(
        &heap,
        delta,
        "prelude",
        Some(typelisp_front::dump::sources_digest(crate::prelude::DUMPED_SOURCES)),
        Some(plan.source_hash),
        &plan.forms,
        items.iter().map(unit_item).collect(),
        globals,
    )?;
    let types = typelisp_front::dump::write_state(&state)?;
    Ok(typelisp_front::dump::write(&[(types, bitcode)]))
}

/// One compiled definition in the form a dump records it.
///
/// The backend's [`CompiledItem`] and the front end's `UnitItem` are the same
/// two cases; they are separate types because the front end must not depend on
/// the backend (see `typelisp_front::dump::UnitItem`).
pub(crate) fn unit_item(item: &CompiledItem) -> typelisp_front::dump::UnitItem {
    match item {
        CompiledItem::Fn(path) => typelisp_front::dump::UnitItem::Fn(path.clone()),
        CompiledItem::Method(path, name) => {
            typelisp_front::dump::UnitItem::Method(path.clone(), name.clone())
        }
    }
}
