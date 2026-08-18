//! The precompiled *prelude* artifact: `src/prelude_compiled.bc`, holding the
//! native bodies of every prelude definition that can have one, so prelude
//! calls in user code run compiled instead of being tree-walked.
//!
//! The same shape as [`crate::compile::bootstrap`]'s compiler island —
//! [`build_prelude_bitcode`] compiles into one shared module and serializes
//! it, `src/bin/bootstrap_prelude.rs` commits the bytes, [`crate::prelude::load`]
//! installs them — with three differences worth naming.
//!
//! **This is about execution speed, not load time.** The deleted fasl cache
//! serialized *checked state* to skip parsing and type-checking; this
//! serializes *machine code*. The read/check/`exec` pass still runs at load
//! (the checker state and the `FnDef`s are needed either way); what changes is
//! what a later call to `gcd` or `abs` actually executes.
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

/// The name of the i64 global the prelude bitcode carries its source hash in.
/// Distinct from the island's [`crate::compile::bootstrap::SOURCE_HASH_GLOBAL`]
/// so neither artifact can be checked against the other's hash.
pub const PRELUDE_SOURCE_HASH_GLOBAL: &str = "__typelisp_prelude_source_hash";

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
        // No codegen of their own. `exec` still registers what they define:
        // an enum's variants and a struct's field representations, which the
        // compile bridge reads back through `Interp::compile_definitions`.
        "defstruct" | "defenum" | "defmacro" | "use" => {}
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

/// Assigns every prelude global its compiled-slot id, in declaration order.
///
/// Must run at the same point on both sides — right after the prelude's forms
/// are `exec`'d, before anything touches bitcode — because the ids are baked
/// into the artifact as constants and `typelisp_rt`'s table hands them out
/// sequentially from zero per `Interp`. Eager and in order rather than lazily
/// on first reference, for exactly the reason `compile::aot::compile_file`
/// promotes its `defvar`s eagerly: a numbering that depends on *which bodies
/// happened to be compiled first* is not a numbering a second process can
/// reproduce.
pub fn promote_globals(heap: &mut Heap, interp: &Interp, plan: &PreludePlan) -> Result<(), String> {
    for path in &plan.globals {
        interp.promote_global(heap, path).map_err(|e| format!("prelude: promoting `{}`: {}", path, e))?;
    }
    Ok(())
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

/// Builds the prelude's AOT bitcode in a throwaway environment.
///
/// Mirrors [`crate::compile::bootstrap::build_island_bitcode`]: one shared
/// module, `rt_*` forward declarations, every body forward-declared before any
/// is translated (the prelude's definitions reference each other freely — the
/// numeric catalog's `gcd` calls `abs`, `signum` calls `abs`), then one
/// `add_compiled_function` per item. No `main` wrapper and no
/// `add_global_mapping`: this is a library of compiled functions, and the
/// `rt_*` addresses are supplied at install time by
/// `Interp::install_compiled_library`.
pub fn build_prelude_bitcode() -> Result<Vec<u8>, String> {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = crate::Checker::new();
    let mut interp = Interp::new();

    // Interpreted, deliberately: see this module's doc comment on why there is
    // no snapshot chain.
    let plan = crate::prelude::load_interpreted(&mut heap, &mut chk, &mut interp);
    promote_globals(&mut heap, &interp, &plan)?;
    // The island is what actually translates the bodies below, so it has to be
    // native before the first `add_compiled_function` call.
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
        crate::compile::bootstrap::embed_source_hash(ctx, &module, PRELUDE_SOURCE_HASH_GLOBAL, plan.source_hash);
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
    bitcode
}
