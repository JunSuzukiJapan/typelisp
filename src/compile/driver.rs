//! The compile driver: what turns a checked `defun` into machine code.
//!
//! Six operations, all of which build or consume an LLVM `Module`:
//! [`install_compiled_library`] (a committed bitcode artifact),
//! [`add_compiled_function`]/[`translate_and_compile`]/[`run_compile_function`]
//! (one definition through the island), [`add_compiled_global_init`] (a
//! `defvar`'s initializer) and [`compile_scc`] (a mutually-recursive group).
//!
//! # Why these are not methods on `Interp`
//!
//! They were, and they were the last thing tying `eval::interp` to `inkwell`.
//! Compiling is something done *to* an interpreter's definitions, not
//! something an interpreter does — the tree-walking evaluator never needs it,
//! and `(compile f)` reaches it through a hook the backend installs, the same
//! way the island reaches its `llvm-*` builtins
//! (see [`crate::compile::llvm_builtins`]).
//!
//! Free functions over `&Interp` rather than an `impl` block because after the
//! front end becomes its own crate this module cannot write one: a crate may
//! only implement its own traits on foreign types.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;

use typelisp_mem::{Heap, RootScope, Value};

use crate::check::repr::Repr;
use crate::eval::interp::{
    fn_path_from_node_name, intern_names, intern_params, method_link_name, str_rt, CallEdge, EvalError,
    Interp, Uncompilable,
};
use crate::compile::externs::{
    is_native_lowered_primitive_method, is_rt_builtin_name, rt_extern_functions,
};
use crate::compile::symbols::HASHTABLE_BUILTIN_METHODS;
use crate::types::{path_is_builtin, path_is_builtin_any, Path, LLVM_METHOD_RECEIVER_TYPES, NATIVE_LOWERED_PRIMITIVES};
use crate::CompileTarget;

/// Installs a precompiled library — a committed bitcode artifact holding
/// the native bodies of definitions this `Interp` has already registered
/// interpreted — into [`FnDef::compiled`], so calls to them dispatch to
/// native code instead of being tree-walked.
///
/// Two callers, both after re-checking and `exec`ing the corresponding
/// source (which registers the `FnDef`s + checker state the compiled
/// bodies still need for signatures): the compiler island
/// ([`crate::compiler::load_aot`], interp-closure removal Stage 4) and the
/// prelude ([`crate::prelude::load`]).
///
/// Neither artifact references any external symbol other than the `rt_*`
/// runtime shims (each is one self-contained module whose functions call
/// each other directly and lower every builtin to an `rt_*`/
/// `rt_llvm_call`), so `externals` is exactly [`rt_extern_functions`] —
/// the same set `compile_scc` supplies for a JIT'd SCC.
///
/// Staleness is not this function's question: a dump's bitcode and its checked
/// state are written by one pass into one file, and the source both came from
/// is checked against the dump's own digest before a loader gets here
/// (`compile::dump::load_unit`'s callers).
///
/// `items` is what the caller *would like* installed; what actually gets
/// installed is whatever of that the artifact has a body for. See the
/// filter below for why that is the artifact's call and not the caller's.
pub fn install_compiled_library(interp: &Interp, lib: crate::compile::CompiledLibrary) -> Result<(), String> {
    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    let buffer = MemoryBuffer::create_from_memory_range_copy(lib.bitcode, lib.label);
    let module = Module::parse_bitcode_from_buffer(&buffer, crate::compile::llvm_context())
        .map_err(|e| format!("{} bitcode failed to parse: {}", lib.label, e))?;

    // The artifact decides what it carries. An item with no *body* here
    // either was never compilable (the prelude's stream methods, whose
    // builtins have no lowering) or postdates this `.bc` (a definition
    // added since, during a bootstrap load of the previous generation) —
    // either way there is nothing to install, and asking `new_multi` for
    // it would resolve a body-less declaration to an address pointing at
    // nothing.
    //
    // Checking for a body rather than a declaration matters: `rt_*` shims
    // and any not-yet-filled forward declaration answer `get_function`
    // too.
    let items: Vec<&crate::compile::symbols::CompiledItem> = lib
        .items
        .iter()
        .filter(|item| {
            module.get_function(&item.symbol_name()).is_some_and(|f| f.count_basic_blocks() > 0)
        })
        .collect();
    let internal_names: Vec<String> = items.iter().map(|item| item.symbol_name()).collect();
    // Wire only the `rt_*` shims the module actually forward-declares.
    // Skipping the rest is not a leniency: a module can only *call* what
    // it declares, so a shim with no declaration here has no call site to
    // resolve, whereas passing it to `new_multi` would fail that
    // function's "no forward declaration" guard.
    //
    // Both loads need this, not just the bootstrap one. A hash-checked
    // load verifies the `.bc` against its own SOURCE — and the shim list is
    // *Rust*, so adding one (`rt_apply_any`, Stage D; `rt_macroexpand_1`,
    // Phase 4c) leaves the hash matching while the committed bitcode still
    // declares the older set.
    // Gating this on the hash check made every such addition fail the
    // install until the artifact was regenerated, for a mapping the
    // artifact had no use for.
    let externals: Vec<(String, usize)> = rt_extern_functions()
        .iter()
        .filter(|(n, _)| module.get_function(n).is_some())
        .map(|(n, addr)| (n.to_string(), *addr))
        .collect();
    let compiled_fns = crate::compile::CompiledFn::new_multi(&module, &internal_names, &externals, lib.body_abi)
        .map_err(|e| format!("{} JIT install failed: {}", lib.label, e))?;

    for (item, cf) in items.iter().zip(compiled_fns) {
        let def = match item {
            crate::compile::symbols::CompiledItem::Fn(path) => interp.root.borrow().get_fn(path),
            crate::compile::symbols::CompiledItem::Method(type_path, method) => {
                interp.root.borrow().get_method(type_path, method)
            }
        }
        .ok_or_else(|| {
            format!("{}: `{}` is not registered — exec its SOURCE first", lib.label, item.node_name())
        })?;
        *def.compiled.borrow_mut() = Some(Rc::new(cf));
    }
    Ok(())
}

/// Compiles the `defun` named `name` (looked up in the scope tree) into one
/// LLVM function — named `internal_name` — added to `module`. Shared by
/// [`crate::eval::interp::Interp::compile_function`] (JIT, Phase 1) — which always passes a
/// throwaway, single-use module and the same name twice — and
/// `compile::aot::compile_file` (AOT, Phase 2) — which passes the same
/// shared, file-wide module across every `defun` in the source file,
/// asking for a different `internal_name` only for `main` (so it
/// doesn't collide with the real C `main` the AOT path synthesizes
/// separately — see that module's doc comment).
///
/// Takes `module` instead of creating/returning one, on purpose: see
/// `compiler.rs`'s doc comment for why `compile-function` (the
/// typelisp-hosted half of this) can never hand back sole ownership of
/// an `llvm-module` value once `labels`' mutual-recursion closures have
/// captured it.
///
/// Phase 1/2 scope: a non-generic `defun` whose single-expression body
/// only uses node shapes `compile::core_bridge::to_island` has a real
/// translation for (`i64` literals/vars/`+`/`-`/`*`) — anything else
/// surfaces as a `Panic` from the compiler body's own `"unsupported"`
/// handling (`compiler.rs`'s `compile-value`), not a separate check
/// here; there's exactly one place that needs to know the supported
/// shape.
pub fn add_compiled_function(
    interp: &Interp,
    heap: &mut Heap,
    module: Rc<RefCell<Module<'static>>>,
    name: &str,
    internal_name: &str,
) -> Result<(), EvalError> {
    let (params, body) = interp.compiled_fn_body(name)?;
    translate_and_compile(interp, heap, module, &params, &body, internal_name, &HashSet::new())
}

/// The AST-bridge-and-emit half of [`crate::eval::interp::Interp::add_compiled_function`],
/// factored out (no behavior change for that caller) so closure
/// unification Stage 7's [`crate::eval::interp::Interp::jit_define_closure`] can drive the same
/// translate-then-`compile-function` pipeline for a *synthetic*
/// top-level function — a "closure constructor" whose own `params` are
/// **not** a real `defun`'s declared parameters but a captured-cell
/// reference per free variable — rather than one looked up by name via
/// [`crate::eval::interp::Interp::compiled_fn_body`].
///
/// `extra_exclude_from_cell_names` is empty for every ordinary caller
/// (`add_compiled_function`'s own behavior, unchanged); Stage 7's ctor
/// passes its own synthetic parameter names there, because
/// `names_captured_by_nested`'s free-variable walk of `body` (which
/// literally *is* `(lambda ...)`, wrapping the real closure being
/// JIT'd) would otherwise "discover" that the ctor's own params are
/// captured by the nested `lambda` it wraps and — wrongly — cell-box
/// them a second time (`tagged_sym_list`'s `kind + 10`): the ctor's own
/// params are declared `Sexpr` specifically so `bind-params` passes
/// each cell reference through unchanged (kind `6`, the same tagged-
/// pointer passthrough any other boxed value gets), for the *inner*
/// `lambda`'s own (correctly, separately, computed) `lcaptured` list to
/// pick up as-is.
pub fn translate_and_compile(
    interp: &Interp,
    heap: &mut Heap,
    module: Rc<RefCell<Module<'static>>>,
    params: &[(String, Repr)],
    body: &[Value],
    internal_name: &str,
    extra_exclude_from_cell_names: &HashSet<String>,
) -> Result<(), EvalError> {
    // Every global this body reads/assigns must have a compiled-global
    // slot before translation starts — `ast_to_sexpr` looks each one up
    // by id, not by name (see `Ctx::globals`'s doc comment), so there is
    // nothing to resolve lazily mid-translation the way `compile-call`'s
    // `get-function` can for an ordinary function name.
    let targets = match crate::compile::core_bridge::collect_targets(heap, body) {
        Ok(t) => t,
        Err(e) => return Err(EvalError::Panic(e.to_string())),
    };
    for target in &targets.globals {
        interp.promote_global(heap, target)?;
    }
    // Every trait object this body boxes needs its vtable id before
    // translation starts, for the same reason a global needs its slot:
    // `ast_to_sexpr` bakes the id into the emitted IR as a constant.
    for site in &targets.dyn_boxes {
        interp.register_dyn_box(&site.concrete_key, &site.trait_path, &site.slots, &site.supers);
    }
    for to_trait in &targets.dyn_upcasts {
        interp.trait_id_for(to_trait);
    }
    for trait_path in &targets.dyn_traits {
        interp.dyn_dispatch_compiled.borrow_mut().insert(trait_path.clone());
    }
    // `core_bridge` is deliberately `Registry`-free, so hand it the type
    // definitions as a plain flattened snapshot of the scope tree —
    // exactly what `exec` recorded there from each `defstruct`/`defenum`
    // form. Collected fresh per compilation; compiling is rare enough that
    // keeping a second always-current copy isn't worth it.
    let defs = compile_definitions(interp);
    let compiled_globals = interp.compiled_globals.borrow();
    let vtable_ids = interp.vtable_ids.borrow();
    let trait_ids = interp.trait_ids.borrow();
    let dyn_tables = crate::compile::symbols::DynTables { vtables: &vtable_ids, trait_ids: &trait_ids };
    let cx = crate::compile::core_bridge::Ctx::with_dyn_tables(&defs, &compiled_globals, dyn_tables);
    let excluded = intern_names(heap, extra_exclude_from_cell_names);
    let params = intern_params(heap, params);
    let (param_list, body_sexpr) =
        match crate::compile::core_bridge::function_parts(heap, &params, body, cx, &excluded) {
            Ok(v) => v,
            Err(e) => return Err(EvalError::Panic(e.to_string())),
        };
    drop(compiled_globals);
    drop(vtable_ids);
    drop(trait_ids);

    run_compile_function(interp, heap, module, internal_name, param_list, body_sexpr)
}

/// The scope tree's type definitions, in the shape the compile bridge
/// takes. See [`crate::eval::interp::Interp::translate_and_compile`] for why it is a snapshot.
pub fn compile_definitions(interp: &Interp) -> crate::compile::core_bridge::Definitions {
    let (structs, enums) = interp.root.borrow().collect_struct_and_enum_types();
    let mut defs = crate::compile::core_bridge::Definitions::new();
    for (path, _fields) in structs {
        defs.record_struct(path);
    }
    for (path, _def) in enums {
        defs.record_enum(path);
    }
    defs
}

/// Drives the island's `compile-function` over one already-translated
/// function `(param_list, body_sexpr)` into `module` — the shared tail of
/// [`crate::eval::interp::Interp::translate_and_compile`] and [`crate::eval::interp::Interp::add_compiled_global_init`].
///
/// Dispatches to the *compiled* island `compile-function` whenever it is
/// installed in [`crate::eval::interp::Interp::compiled`] (interp-closure removal Stage 4: after
/// [`crate::compiler::load_aot`], so the island runs natively and its own
/// `labels`/`lambda` bodies are never built as interpreted closures),
/// falling back to the interpreted `FnDef` otherwise — a plain
/// `load_compiler` environment, or the bootstrap
/// ([`crate::compile::bootstrap`]) that produces the island bitcode in
/// the first place, where the compiled island doesn't exist yet.
///
/// The compiled `compile-function` returns the very `llvm-module` it was
/// handed (mutated in place); both callers care only about that side
/// effect and ignore the return, but it is still decoded so
/// [`crate::eval::interp::Interp::call_compiled`]'s LLVM-handle bookkeeping stays balanced. A
/// [`llvm_handles_mark`]/[`llvm_handles_release`] pair brackets the call
/// so the transient handles the native compiler registers while walking
/// the AST don't accumulate across many compiles.
pub fn run_compile_function(
    interp: &Interp,
    heap: &mut Heap,
    module: Rc<RefCell<Module<'static>>>,
    internal_name: &str,
    param_list: Value,
    body_sexpr: Value,
) -> Result<(), EvalError> {
    // The island is about to run, and its `llvm-*` builtins are the
    // backend's. Installed here rather than once at startup for the reason
    // [`crate::eval::interp::Interp::enter_compiled`] re-registers the heap on every crossing: it
    // is one store, and there is no ordering rule left to remember. (This
    // used to be one of the last front-end references to `crate::compile`,
    // waiting for the method to move to the backend; the move happened, and
    // this file is the backend.)
    crate::compile::install_llvm_backend();
    let compiler_path = Path::root("compile-function");
    let argv = vec![
        crate::compile::llvm_builtins::llvm_module_value_rc(module),
        str_rt(heap, internal_name),
        param_list,
        body_sexpr,
    ];
    let compiler_def = interp.root.borrow().get_fn(&compiler_path).ok_or_else(|| {
        EvalError::Internal("compile: compiler body not loaded — call load_compiler first".into())
    })?;
    let compiled = compiler_def.compiled.borrow().clone();
    if let Some(cf) = compiled {
        // The island's own entry point hands back an LLVM module, which
        // crosses as a raw registry index.
        let ret = Repr::Handle;
        let mark = crate::compile::llvm_builtins::llvm_handles_mark();
        let scope_mark = heap.session_root_count();
        let param_reprs =
            &compiler_def.sig.as_ref().expect("the compiler body always has a signature").0;
        let r = interp.call_compiled(heap, cf.as_ref(), &argv, param_reprs, &ret);
        crate::compile::llvm_builtins::llvm_handles_release(mark);
        // Scope boxes the island created during this compile are session
        // roots (see `LlvmRetK::Scope`); release them with the handles.
        heap.truncate_session_roots(scope_mark);
        r?;
        return Ok(());
    }
    interp.apply(heap, &compiler_def, argv)?;
    Ok(())
}

/// AOT-only counterpart of [`crate::eval::interp::Interp::add_compiled_function`]: compiles a
/// `defvar`'s initializer expression `value` into a zero-argument LLVM
/// function `internal_name` in `module` that, when called, evaluates it
/// and calls `rt_global_new` to establish that global's *runtime*
/// storage — one entry in the startup sequence `compile::aot::
/// compile_file` generates and wires into `main` (via
/// `compile::aot::build_main_wrapper`) so a standalone executable
/// allocates each of its own promoted globals before `tl_main` (the
/// file's own `main` defun) ever runs. See [`crate::eval::interp::Interp::promote_global`]'s
/// doc comment for why `compile::aot::compile_file` must call these, in
/// the same order it called `crate::eval::interp::Interp::promote_global` for each `defvar`.
///
/// Mirrors `add_compiled_function`'s own translate-then-`compile-
/// function` shape almost exactly, just with no parameters and a body
/// wrapped as `(global-init kind value-form)`
/// ([`crate::compile::symbols::ast_to_sexpr_for_global_init`],
/// `compiler.rs`'s `compile-global-init`) instead of an ordinary
/// translated function body — `value` may itself reference other
/// globals (an earlier `defvar`'s value), so the same promotion pass
/// applies here too.
pub fn add_compiled_global_init(
    interp: &Interp,
    heap: &mut Heap,
    module: Rc<RefCell<Module<'static>>>,
    internal_name: &str,
    form: Value,
) -> Result<(), EvalError> {
    // The whole `(defvar ...)` form, not just its initializer:
    // `core_bridge::global_init` reads the declared representation off the
    // form to know how the global's storage is tagged, which the
    // initializer alone does not say.
    let body = std::slice::from_ref(&form);
    let targets = match crate::compile::core_bridge::collect_targets(heap, body) {
        Ok(t) => t,
        Err(e) => return Err(EvalError::Panic(e.to_string())),
    };
    // The initializer may reference *other* globals (an earlier `defvar`'s
    // value), so the same promotion pass an ordinary body gets applies.
    for target in &targets.globals {
        interp.promote_global(heap, target)?;
    }
    for site in &targets.dyn_boxes {
        interp.register_dyn_box(&site.concrete_key, &site.trait_path, &site.slots, &site.supers);
    }
    for to_trait in &targets.dyn_upcasts {
        interp.trait_id_for(to_trait);
    }
    let defs = compile_definitions(interp);
    let compiled_globals = interp.compiled_globals.borrow();
    let vtable_ids = interp.vtable_ids.borrow();
    let trait_ids = interp.trait_ids.borrow();
    let dyn_tables = crate::compile::symbols::DynTables { vtables: &vtable_ids, trait_ids: &trait_ids };
    let cx = crate::compile::core_bridge::Ctx::with_dyn_tables(&defs, &compiled_globals, dyn_tables);
    let body_sexpr = match crate::compile::core_bridge::global_init(heap, form, cx) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Err(EvalError::Internal(
                "compile: a global initializer was built from something that is not a `defvar`".into(),
            ))
        }
        Err(e) => return Err(EvalError::Panic(e.to_string())),
    };
    drop(compiled_globals);
    drop(vtable_ids);
    drop(trait_ids);
    let mut s = RootScope::new(heap);
    s.push_root(body_sexpr);
    // A global initializer takes no parameters.
    let param_list = Value::Empty;
    run_compile_function(interp, &mut s, module, internal_name, param_list, body_sexpr)
}

/// `(compile fn-name)` (or `(compile type::method)`): JIT-compiles a
/// previously-defined `defun`/`defmethod` and marks the target `FnDef`
/// node compiled (see `FnDef::compiled`) so `call`/`assoc`
/// dispatches to native code instead of tree-walking it from then on.
/// `target` is already fully resolved by `Checker::check_compile` (a
/// `Ref` re-verified here via `crate::eval::interp::Interp::resolve_fn_ref`, or a `type_name`+
/// `method` re-verified via `ModuleScope::resolve_method` — the same
/// independent re-resolution every other reference gets, not a bare
/// name to search the whole tree for by local name alone). The
/// qualified string this derives from that resolved identity only feeds
/// [`crate::eval::interp::Interp::compute_sccs`]'s *internal* graph bookkeeping — unchanged
/// from before, and still keyed by local type name for a method
/// ([`crate::eval::interp::Interp::method_key`]), since transitively-discovered call targets
/// already reach that machinery the same way. See
/// [`crate::eval::interp::Interp::add_compiled_function`] for the supported-shape scope.
///
/// labels/closures Stage 3 (single-function shape) / Stage 5 (SCC
/// generalization): unlike `compile::aot::compile_file` (one shared
/// module built up over every `defun` in file order, so a callee is
/// always already fully defined in that same module by the time its
/// caller is compiled — see that module's doc comment), `name`'s own
/// strongly connected component of the top-level call graph —
/// [`crate::eval::interp::Interp::compute_sccs`], usually just `{name}` itself, but a genuine
/// group for mutual recursion across *separate* top-level functions —
/// gets one throwaway module/engine per SCC ([`crate::eval::interp::Interp::compile_scc`]),
/// processed leaf-SCC-first. Every target *outside* the current SCC
/// (`crate::compile::symbols::collect_call_targets`/
/// `collect_assoc_targets`, gathered via [`crate::eval::interp::Interp::call_graph_edges`]) is
/// handled by hand, in three steps: (1) it must already be `compile`d by
/// the time its SCC is processed — [`crate::eval::interp::Interp::compute_sccs`]'s finish-order
/// contract guarantees this, so a violation is an internal-invariant
/// `.expect()`, not a user-facing error; (2) forward-declared — no body
/// — in this SCC's module *before* the compiler body runs for any of its
/// members (`compile-call`'s `get-function` needs to find *something* by
/// that name); (3) wired to the real, already-running JIT code's address
/// via `add_global_mapping` *after* (`crate::compile::CompiledFn::
/// new_multi`'s `externals` parameter) — can't happen any earlier, since
/// the engine that will actually run this SCC's code doesn't exist until
/// then. Self-recursion, and recursion among an SCC's own members, needs
/// none of this: every member of the SCC is forward-declared under its
/// own name in the *same* module before any of their bodies are
/// translated, so `compile-function`'s own `(add-function m name)`
/// (reusing that declaration — see [`llvm_module_add_function`]'s doc
/// comment) already gives every sibling something to call before any
/// body exists.
///
/// A user-defined method this body calls (`assoc`,
/// `crate::compile::symbols::collect_assoc_targets`) goes through the
/// exact same three steps, *keyed and named differently*: looked up in
/// [`crate::eval::interp::Interp::methods`]/[`crate::eval::interp::Interp::compiled_methods`] instead of
/// [`crate::eval::interp::Interp::fns`]/[`crate::eval::interp::Interp::compiled`], and forward-declared/wired under the
/// mangled name [`method_link_name`] builds — the same literal string
/// this very method itself uses as `internal_name` when `name` is a
/// method (see [`crate::eval::interp::Interp::add_compiled_function`]'s call below), which is
/// also exactly what `compiler.rs`'s `compile-assoc` mangles a callee's
/// `(type-name, method)` back into before its own `get-function` lookup
/// — so the three names (this method's own `internal_name`, this
/// method's entry in `externals`, and a *caller's* `compile-assoc`
/// lookup) can never drift apart. A primitive-receiver target *not*
/// registered in the scope tree's `methods` (`i64`/`i32`'s own built-in
/// arithmetic; `string`'s, since Stage 7 of the Sexpr-representation plan —
/// `docs/implementation-log.md`) needs none of this: those compile
/// natively with no external call (`compile-assoc`'s own dispatch, which
/// panics clearly on its own for any one of *their* methods it doesn't
/// actually implement, e.g. `string::upcase`). But a *user-defined*
/// method on a primitive receiver (e.g. the prelude's `impl Eq i32` →
/// `i32::equals`) is registered like any other `defstruct` method and
/// takes the normal three steps — `compile-assoc`'s dispatch falls
/// through to the same mangled-name call for it. Anything else
/// (`f64`/`char` builtins — still out of scope) panics clearly right
/// here rather than deep inside `compile-assoc`'s own `get-function`.
pub fn compile_function(interp: &Interp, heap: &mut Heap, target: &CompileTarget) -> Result<Value, EvalError> {
    let (name, already_compiled) = match target {
        CompileTarget::Fn(r) => {
            interp.resolve_fn_ref(r).ok_or_else(|| {
                EvalError::Internal(format!(
                    "compile: `{}` resolved at check time but not here",
                    r.written.join("::")
                ))
            })?;
            (r.resolved.to_string(), interp.root.borrow().fn_compiled(&r.resolved))
        }
        CompileTarget::Method { type_name, method, home } => {
            interp.root.borrow().resolve_method(home, type_name, method).ok_or_else(|| {
                EvalError::Internal(format!(
                    "compile: `{}` resolved at check time but not here",
                    method_link_name(type_name, method)
                ))
            })?;
            (method_link_name(type_name, method), interp.root.borrow().method_compiled(type_name, method))
        }
    };
    if already_compiled {
        return Ok(Value::Bool(true));
    }
    for scc in compute_sccs(interp, heap, &name)? {
        compile_scc(interp, heap, &scc)?;
    }
    Ok(Value::Bool(true))
}

/// `(disassemble name)` / `(disassemble name true)` — the machine code a
/// definition compiles to, or the LLVM IR it compiles through.
///
/// Emits into a module of its own and stops before the JIT, which is what
/// makes this *free of side effects*: nothing is installed, the definition's
/// `compiled` slot is untouched, and a function that was interpreted before
/// still is afterwards. Disassembling is a question, not a request to compile.
///
/// The declarations are exactly [`compile_scc`]'s: the function itself, every
/// direct call target ([`call_graph_edges`]) and the whole `rt_*` table. They
/// have to be there even though nothing is linked — the island's
/// `compile-call` looks a callee up with `get-function`, which **aborts the
/// process** when the declaration is missing rather than returning an error.
/// Their *addresses* are what a JIT would need and this does not, which is
/// why a callee that could not itself be compiled is no obstacle here: only
/// this one body is translated.
///
/// Assembly by default, because that is what CL's `disassemble` promises —
/// the instructions this machine will actually run. The LLVM IR is the same
/// module one step earlier, and is what to look at when the question is about
/// this compiler rather than about the chip.
pub fn disassemble_function(
    interp: &Interp,
    heap: &mut Heap,
    target: &CompileTarget,
    llvm_ir: bool,
) -> Result<String, EvalError> {
    let name = match target {
        CompileTarget::Fn(r) => {
            interp.resolve_fn_ref(r).ok_or_else(|| {
                EvalError::Internal(format!(
                    "disassemble: `{}` resolved at check time but not here",
                    r.written.join("::")
                ))
            })?;
            r.resolved.to_string()
        }
        CompileTarget::Method { type_name, method, home } => {
            interp.root.borrow().resolve_method(home, type_name, method).ok_or_else(|| {
                EvalError::Internal(format!(
                    "disassemble: `{}` resolved at check time but not here",
                    method_link_name(type_name, method)
                ))
            })?;
            method_link_name(type_name, method)
        }
    };
    // An FFI declaration has no forms to translate, so the module built below
    // would hold a function with an empty body — not the thunk the name
    // actually reaches, and disassembling it would be a confident wrong
    // answer. The thunk's own machine code is not reachable from here: it was
    // JIT'd into an engine this function does not have.
    if interp.fn_is_ffi(&name) {
        return Err(EvalError::Panic(format!(
            "disassemble: `{}` is a C function declared by `defffi`. What it reaches is the C \
             function's own machine code, which this compiler did not produce and cannot show; \
             the thunk in between is emitted by `crate::compile::ffi`.",
            name
        )));
    }
    let symbol = crate::compile::symbols::user_symbol_name(&name);
    let edges = call_graph_edges(interp, heap, &name)?;

    let module = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let module = Rc::new(RefCell::new(crate::compile::llvm_context().create_module("disassemble")));
        crate::compile::llvm_builtins::declare_external_compiled_function(&module, &symbol);
        for edge in &edges {
            let target_symbol = match edge {
                CallEdge::Fn(p) => crate::compile::symbols::user_symbol_name(&p.to_string()),
                CallEdge::Method(t, m) => crate::compile::symbols::user_method_symbol_name(t, m),
            };
            if target_symbol != symbol {
                crate::compile::llvm_builtins::declare_external_compiled_function(&module, &target_symbol);
            }
        }
        for (rt_name, _) in rt_extern_functions() {
            crate::compile::llvm_builtins::declare_external_function(&module, rt_name);
        }
        module
    };

    add_compiled_function(interp, heap, module.clone(), &name, &symbol)?;

    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    let verified = crate::compile::verify_module_naming_functions(&module.borrow(), "disassemble: module");
    if llvm_ir {
        // Printed even when it does not verify. Asking for the IR is what you
        // do *because* something is wrong with it; refusing to show it then is
        // backwards. The complaint goes on the end so it is not lost.
        let text = module.borrow().print_to_string().to_string();
        return Ok(match verified {
            Ok(()) => text,
            Err(e) => format!("{}\n; {}", text, e),
        });
    }
    verified.map_err(EvalError::Panic)?;
    // Bound rather than returned directly: the `borrow()` temporary would
    // otherwise outlive `module` in tail position.
    let assembly = crate::compile::aot::assembly_of(&module.borrow());
    assembly.map_err(EvalError::Panic)
}

/// Answers "would compiling `name` reach something with no compilable
/// body?" without emitting anything.
///
/// The check has to happen *before* translation, not around it: when the
/// island's `compile-call` can't find a callee's declaration it calls
/// `get-function`, which **aborts the process** rather than returning an
/// error (see [`crate::compile::runtime_function_names`]'s doc comment).
/// So a builder of a shared library module — `compile::prelude_bootstrap`,
/// which must decide per definition whether to include it — cannot simply
/// try and recover. [`crate::eval::interp::Interp::compute_sccs`] already walks exactly the
/// transitive closure that matters and reports a missing body cleanly
/// (through [`crate::eval::interp::Interp::compiled_fn_body`]), so asking it is both the cheapest
/// and the most faithful available oracle: same graph, same filters, same
/// notion of "is a real call target" the compile path itself uses.
///
/// Transitivity is the useful part. A definition that merely *calls*
/// something uncompilable is itself uncompilable — its emitted body would
/// reference a symbol nothing defines — and this reports that without the
/// caller having to close the set by hand. It also means the answer is
/// about the *root* cause: dozens of prelude stream methods are
/// uncompilable for the single reason that `stream-read-char` has no
/// lowering, and [`Uncompilable::MissingTarget`] says so for each.
pub(crate) fn precheck_compilable(interp: &Interp, heap: &Heap, name: &str) -> Result<(), Uncompilable> {
    match compute_sccs(interp, heap, name) {
        Ok(_) => Ok(()),
        Err(e) => Err(match e.into_kind() {
            EvalError::NoSuchFunction(target) => Uncompilable::MissingTarget(target),
            EvalError::Uncompilable { target, .. } => Uncompilable::MissingTarget(target),
            other => Uncompilable::Other(other),
        }),
    }
}

/// `name`'s own outgoing edges in the top-level compile call graph —
/// every concrete function/method *instantiation* `name`'s body calls,
/// filtered exactly the way `compile_function_rec` always has: self-
/// recursion and `rt_*`/native builtins excluded from
/// [`CallEdge::Fn`]; `Vector`/`HashTable`'s op-node-lowered builtin
/// methods and natively-lowered primitive-receiver builtins excluded
/// from [`CallEdge::Method`]. A method target with no registered
/// implementation at all (a builtin `compile-assoc` doesn't lower
/// natively, e.g. `f64::sqrt`) is a compile-time error here, same as
/// before Stage 5 — this is the one path that produces a real user-
/// facing error out of graph construction, everything else just shapes
/// the graph [`crate::eval::interp::Interp::compute_sccs`] walks. Shared by that graph walk and
/// [`crate::eval::interp::Interp::compile_scc`] (which needs the same edges again, in typed
/// form, to know what to forward-declare/wire as `externals`).
/// The free functions that **suspend the running task**, and so cannot be
/// compiled — see the refusal in [`call_graph_edges`].
///
/// `sleep` is here because with tasks it stops *the task*, not the thread
/// (`core_cps`'s `Waiting::Until`). Compiled it could only ever stop the
/// thread, so the same source would mean two different things depending on
/// whether it had been through `(compile ...)` — silently. `task::wait` is
/// the method-shaped member of the same set and is matched separately.
const SUSPENDING_CALLS: &[&str] = &["yield", "sleep"];

pub(crate) fn call_graph_edges(interp: &Interp, heap: &Heap, name: &str) -> Result<Vec<CallEdge>, EvalError> {
    let path = fn_path_from_node_name(name);
    let method_key = interp.method_key(name);
    let (_, body) = interp.compiled_fn_body(name)?;
    let targets = match crate::compile::core_bridge::collect_targets(heap, &body) {
        Ok(t) => t,
        Err(e) => return Err(EvalError::Panic(e.to_string())),
    };
    let mut edges = Vec::new();

    // The operations that **suspend the running task**. A compiled body cannot
    // suspend: a task is an interpreter continuation stack, and compiled code
    // runs on the Rust stack with nothing to come back to (the plan's B6). So
    // this is a deliberate refusal carrying the reason, rather than the
    // "no such function"/"no compiled implementation" a missing lowering gives
    // — `yield` and `task::wait` both exist, and neither is going to be
    // lowered.
    //
    // `go` is deliberately absent from this: starting a task suspends nothing,
    // and `compile-go` lowers it.
    let suspends = targets
        .calls
        .iter()
        .find(|p| SUSPENDING_CALLS.iter().any(|n| **p == Path::root(n)))
        .map(|p| p.to_string())
        .or_else(|| {
            targets
                .methods
                .iter()
                .find(|(t, m)| *t == Path::root("task") && m == "wait")
                .map(|(t, m)| format!("{}::{}", t, m))
        });
    if let Some(target) = suspends {
        return Err(EvalError::Panic(format!(
            "compile: \"{}\" calls \"{}\", which suspends the running task — and compiled code \
             cannot suspend, because a task is an interpreter continuation stack while a compiled \
             body runs on the Rust stack. Leave the `{}` in an interpreted caller; `go` itself compiles.",
            name, target, target
        )));
    }

    edges.extend(
        targets
            .calls
            .into_iter()
            .filter(|p| *p != path && !is_rt_builtin_name(p.last_segment()))
            .map(CallEdge::Fn),
    );

    let method_targets: Vec<(Path, String)> = targets
        .methods
        .into_iter()
        .filter(|key| method_key.as_ref() != Some(key))
        .filter(|key| {
            // `Vector<T>`'s field-backed builtin methods (`new`/`get`/
            // `set`/`len`/`push`/`pop`) are lowered to a `vector-op` node
            // (`core_bridge::translate_vector_method` -> `rt_struct_*`),
            // not a method call, so — like the native primitive methods
            // below — they are never a real call target. `vector::iter`
            // is deliberately excluded from this list: it is a genuine
            // prelude `defmethod` (`vector-iter::new`) and must be
            // `compile`d like any other method.
            if path_is_builtin(&key.0, "vector") && matches!(key.1.as_str(), "new" | "get" | "set" | "len" | "push" | "pop") {
                return false;
            }
            // `HashTable<K,V>`'s builtin methods lowered to a `hashtable-op`
            // node (`core_bridge::translate_hashtable_method`) are likewise
            // never a real call target. The prelude `defmethod`s — `iter`,
            // and `get`/`set`/`remove`/`maphash`/`size` — are deliberately
            // absent from that list, so they are validated and transitively
            // compiled like any other method.
            if path_is_builtin(&key.0, "hashtable") && HASHTABLE_BUILTIN_METHODS.contains(&key.1.as_str()) {
                return false;
            }
            // `llvm-*`/`scope` builtin methods are natively lowered to
            // the `rt_llvm_call` dispatch shim (an `llvm-op` node,
            // interp-closure removal Stage 1) — like `vector-op`/
            // `hashtable-op` above, never a real call target. A
            // heap-repr `Scope<V>` method has no compiled lowering and
            // panics inside `compile-assoc-user`'s `get-function`
            // instead, per the convention in the next comment.
            if path_is_builtin_any(&key.0, &LLVM_METHOD_RECEIVER_TYPES) {
                return false;
            }
            // A user-registered method is a real call target even on a
            // primitive receiver (`i32::equals`); only the natively
            // lowered `i64`/`i32`/`char`/`string`/`f64`/`bignum`/`ratio`
            // builtins (`+`, `<`, `=`, `lt`, `length`, `sqrt`, `fadd`,
            // `rt_bignum_add`, ...) are excluded — those become LLVM
            // instructions / `rt_str_*`/`rt_bignum_*`/`rt_ratio_*` calls
            // in `compile-assoc`, not function calls. A builtin on these
            // receivers that `compile-assoc` does *not* lower natively
            // (`i64::int->char`, `string::upcase`, ...) is kept as a target
            // so the `!interp.methods.contains_key` check below rejects it
            // with a clean up-front error — otherwise it reaches the
            // island's `get-function` guard, an unrecoverable
            // `rt_llvm_call` abort under the AOT-native island
            // (interp-closure removal Stage 8a). `is_native_lowered_primitive_method`
            // is the Rust twin of the island's `*-native-method?` list.
            interp.root.borrow().has_method(&key.0, &key.1)
                || !path_is_builtin_any(&key.0, &NATIVE_LOWERED_PRIMITIVES)
                || !is_native_lowered_primitive_method(key.0.last_segment(), &key.1)
        })
        .collect();
    for (type_name, method) in &method_targets {
        if !interp.root.borrow().has_method(type_name, method) {
            return Err(EvalError::Uncompilable {
                caller: name.to_string(),
                target: method_link_name(type_name, method),
            });
        }
    }
    edges.extend(method_targets.into_iter().map(|(p, m)| CallEdge::Method(p, m)));
    Ok(edges)
}

/// Tarjan's algorithm over the top-level compile call graph, rooted at
/// `name`, restricted to the induced subgraph of not-yet-`compile`d
/// nodes (labels/closures Stage 5) — an edge into an already-compiled
/// target is a leaf for this traversal's purposes, since its address is
/// already known and needs no further graph treatment. Returns every
/// strongly connected component this traversal reaches, **in the order
/// Tarjan completes them**: a classic property of the algorithm is that
/// this finish order is a reverse topological order of the SCC
/// condensation — if `A` calls something in a different SCC `B`, `B`
/// finishes (and is pushed onto the result) before `A` does. That is
/// exactly the order [`crate::eval::interp::Interp::compile_function`] needs to hand to
/// [`crate::eval::interp::Interp::compile_scc`]: every SCC's external dependencies are already
/// compiled by the time it's processed. A single-member SCC with no
/// self-loop is the common case (an ordinary, non-recursive-with-others
/// function); a multi-member SCC is genuine mutual recursion across
/// separate top-level functions, unsupported before this stage.
pub(crate) fn compute_sccs(interp: &Interp, heap: &Heap, name: &str) -> Result<Vec<Vec<String>>, EvalError> {
    let mut counter = 0usize;
    let mut indices: HashMap<String, usize> = HashMap::new();
    let mut lowlink: HashMap<String, usize> = HashMap::new();
    let mut on_stack: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();
    let mut sccs: Vec<Vec<String>> = Vec::new();
    scc_strongconnect(interp, heap, name, &mut counter, &mut indices, &mut lowlink, &mut on_stack, &mut stack, &mut sccs)?;
    Ok(sccs)
}

/// One node's worth of Tarjan's `strongconnect` — see
/// [`crate::eval::interp::Interp::compute_sccs`]'s doc comment for the algorithm-level
/// contract. Recursive over [`crate::eval::interp::Interp::call_graph_edges`]; an edge whose
/// target is already compiled is skipped outright (never entered into
/// `indices` at all), so it never contributes a spurious singleton SCC.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn scc_strongconnect(
    interp: &Interp,
    heap: &Heap,
    node: &str,
    counter: &mut usize,
    indices: &mut HashMap<String, usize>,
    lowlink: &mut HashMap<String, usize>,
    on_stack: &mut HashSet<String>,
    stack: &mut Vec<String>,
    sccs: &mut Vec<Vec<String>>,
) -> Result<(), EvalError> {
    indices.insert(node.to_string(), *counter);
    lowlink.insert(node.to_string(), *counter);
    *counter += 1;
    stack.push(node.to_string());
    on_stack.insert(node.to_string());

    for edge in call_graph_edges(interp, heap, node)? {
        let already_compiled = match &edge {
            CallEdge::Fn(p) => interp.root.borrow().fn_compiled(p),
            CallEdge::Method(p, m) => interp.root.borrow().method_compiled(p, m),
        };
        if already_compiled {
            continue;
        }
        let target = edge.node_name();
        if !indices.contains_key(&target) {
            scc_strongconnect(interp, heap, &target, counter, indices, lowlink, on_stack, stack, sccs)?;
            let merged = lowlink[node].min(lowlink[&target]);
            lowlink.insert(node.to_string(), merged);
        } else if on_stack.contains(&target) {
            let merged = lowlink[node].min(indices[&target]);
            lowlink.insert(node.to_string(), merged);
        }
    }

    if lowlink[node] == indices[node] {
        let mut component = Vec::new();
        loop {
            let w = stack.pop().expect("node's own strongconnect frame pushed it onto the stack");
            on_stack.remove(&w);
            let is_root = w == node;
            component.push(w);
            if is_root {
                break;
            }
        }
        sccs.push(component);
    }
    Ok(())
}

/// Compiles one strongly connected component of the top-level call graph
/// (labels/closures Stage 5) — a single function/method, or a set of
/// separate top-level `defun`/`defmethod`s mutually recursive with each
/// other — into one shared, throwaway LLVM module, replacing the single-
/// function-per-module shape every `compile_function_rec` call used
/// before this stage. Every `members` name is forward-declared under its
/// real internal name (`core_bridge::user_symbol_name`) *before* any of
/// their bodies are translated, exactly like an external call target
/// always was — so a call from one member to a sibling still without a
/// body yet resolves to that same declaration by name
/// (`compile-call`'s `get-function`), and [`crate::eval::interp::Interp::add_compiled_function`]
/// (via `compiler.rs`'s `compile-function`, whose own `(add-function m
/// name)` now reuses an existing declaration instead of minting a second,
/// disjoint one — see [`llvm_module_add_function`]'s doc comment) attaches
/// that member's real body to it in place. Targets *outside* `members`
/// are handled exactly like [`crate::eval::interp::Interp::call_graph_edges`]'s callers always
/// have: forward-declared, then wired post-hoc via `add_global_mapping`
/// (`externals`) to their already-compiled address — guaranteed to exist
/// by [`crate::eval::interp::Interp::compute_sccs`]'s finish-order contract. The whole module is
/// JIT'd exactly once via [`crate::compile::CompiledFn::new_multi`], so
/// every member shares one execution engine (mutual calls within the SCC
/// need no `add_global_mapping` entry at all — LLVM resolves them
/// directly against the sibling's own definition in this same module).
pub fn compile_scc(interp: &Interp, heap: &mut Heap, members: &[String]) -> Result<(), EvalError> {
    let member_set: HashSet<&str> = members.iter().map(|s| s.as_str()).collect();

    let mut call_targets: Vec<Path> = Vec::new();
    let mut method_targets: Vec<(Path, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for member in members {
        for edge in call_graph_edges(interp, heap, member)? {
            let target_name = edge.node_name();
            // A sibling within this same SCC resolves through the SCC's
            // own internal forward declarations below, not `externals`.
            if member_set.contains(target_name.as_str()) || !seen.insert(target_name) {
                continue;
            }
            match edge {
                CallEdge::Fn(p) => call_targets.push(p),
                CallEdge::Method(p, m) => method_targets.push((p, m)),
            }
        }
    }

    let module = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let module = Rc::new(RefCell::new(crate::compile::llvm_context().create_module("compiled")));
        for member in members {
            crate::compile::llvm_builtins::declare_external_compiled_function(&module, &crate::compile::symbols::user_symbol_name(member));
        }
        for target in &call_targets {
            crate::compile::llvm_builtins::declare_external_compiled_function(&module, &crate::compile::symbols::user_symbol_name(&target.to_string()));
        }
        for (type_name, method) in &method_targets {
            crate::compile::llvm_builtins::declare_external_compiled_function(&module, &crate::compile::symbols::user_method_symbol_name(type_name, method));
        }
        for (rt_name, _) in rt_extern_functions() {
            crate::compile::llvm_builtins::declare_external_function(&module, rt_name);
        }
        module
    };

    for member in members {
        add_compiled_function(interp, heap, module.clone(), member, &crate::compile::symbols::user_symbol_name(member))?;
    }

    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    let mut externals: Vec<(String, usize)> = call_targets
        .iter()
        .map(|p| {
            let f = interp.root.borrow().get_fn(p).expect("crate::eval::interp::Interp::compute_sccs's finish order guarantees this is already compiled");
            let addr = f.compiled.borrow().as_ref().expect("crate::eval::interp::Interp::compute_sccs's finish order guarantees this is already compiled").address();
            (crate::compile::symbols::user_symbol_name(&p.to_string()), addr)
        })
        .collect();
    externals.extend(method_targets.iter().map(|(type_name, method)| {
        let f = interp.root.borrow().get_method(type_name, method).expect("crate::eval::interp::Interp::compute_sccs's finish order guarantees this is already compiled");
        let addr = f.compiled.borrow().as_ref().expect("crate::eval::interp::Interp::compute_sccs's finish order guarantees this is already compiled").address();
        (crate::compile::symbols::user_method_symbol_name(type_name, method), addr)
    }));
    externals.extend(rt_extern_functions().iter().map(|(n, addr)| (n.to_string(), *addr)));
    // Mirrors `compile::aot::compile_file`'s own `verify()` call in the
    // same position, before handing the module to LLVM for real: a
    // typelisp-hosted `compiler.rs` bug that emits
    // instructions after a block's terminator (the `compile-let`
    // GC-root-leak fix's own doc comment names this exact risk) would
    // otherwise reach `CompiledFn::new_multi`'s `create_jit_execution_engine`
    // as malformed IR — undefined behavior in LLVM itself, not a
    // catchable Rust error. Verifying first turns that into a clean
    // `Panic` instead.
    module.borrow().verify().map_err(|e| EvalError::Panic(format!("compile: module failed verification: {}", e)))?;
    let internal_names: Vec<String> = members.iter().map(|m| crate::compile::symbols::user_symbol_name(m)).collect();
    let compiled_fns = crate::compile::CompiledFn::new_multi(&module.borrow(), &internal_names, &externals, crate::compile::EMITTED_BODY_ABI)
        .map_err(|e| EvalError::Panic(format!("compile: JIT failed: {}", e)))?;
    for (member, compiled) in members.iter().zip(compiled_fns) {
        match interp.method_key(member) {
            Some((type_path, method)) => {
                let f = interp.root.borrow().get_method(&type_path, &method).expect("member is a registered method");
                *f.compiled.borrow_mut() = Some(Rc::new(compiled));
            }
            None => {
                let f = interp.root.borrow().get_fn(&fn_path_from_node_name(member)).expect("member is a registered function");
                *f.compiled.borrow_mut() = Some(Rc::new(compiled));
            }
        }
    }
    // After the addresses above are in place, never before: a method
    // that lands in some vtable's slot may well be a member of *this*
    // SCC, so its entry point only exists as of the loop just above.
    interp.publish_vtables();
    // Explicitly, while `_guard` is still held: `module` was declared
    // before the guard, so letting it fall out of scope would destroy it
    // *after* the guard released — and `~Module` unregisters every value
    // name from the shared LLVM Context (see `compile::COMPILE_LOCK`).
    // Nothing here can leave a share behind for someone else to drop:
    // `add_compiled_function`'s clones are released before it returns.
    drop(module);
    Ok(())
}
