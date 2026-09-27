//! `compile-file`: the AOT exit. Reads an independent typelisp source file
//! (its own fresh `Heap`/`Checker`/`Interp` — deliberately *not* the
//! caller's, since the whole point of AOT output is a self-contained
//! executable that doesn't depend on whatever else happens to be loaded in
//! the process that requested it), compiles every top-level `defun` into
//! one shared LLVM module (reusing
//! [`crate::eval::interp::Interp::add_compiled_function`], the exact same
//! per-function compile step the JIT path (`Interp::compile_function`)
//! uses — see that method's doc comment for why the module is built once,
//! up front, in this file rather than per function), and links the result
//! into a native executable via the system `cc`.
//!
//! Scope matches [`crate::compile::core_bridge`]'s current translation
//! coverage: every `defun` in the file must be non-generic with a
//! single-expression body using only literals/vars/`+`/`-`/`*`,
//! `labels`-sibling/self calls, and top-level `defun`-to-`defun` calls
//! including self-recursion (`call`, labels/closures Stage 3) — a
//! callee must already be defined earlier in the file, the same forward-
//! reference restriction `Checker::resolve_fn` enforces at type-checking
//! time regardless of AOT/JIT (see `compile::CompiledFn::new`'s doc comment
//! for the JIT-side counterpart of this same restriction). The loop below
//! needs no special handling for that: by the time a later `defun` in the
//! file is compiled, every earlier one already has its real body — not just
//! a declaration — in this same shared module, so `compile-call`'s
//! `get-function` always finds it. The file must contain a zero-parameter
//! `main` defun — the entry point — under whatever integer return type its
//! body's arithmetic happens to check as (see
//! `compiles_and_runs_arithmetic_in_main` in `tests/compile_file_test.rs`
//! for why that's `i32` more often than not).
//!
//! ## The `main` name problem
//!
//! Every compiled function uses the fixed ABI `i64 fn(i64* args, i32
//! argc)` (see `crate::compile::CompiledSignature`'s doc comment) — but the
//! C runtime's startup code calls the executable's `main` expecting
//! roughly `int main(void)` (Phase 2 doesn't thread argc/argv through to
//! typelisp). Those two signatures aren't link-compatible, so the file's
//! `main` defun is compiled under the internal name `tl_main` instead, and
//! [`build_main_wrapper`] adds a separate, hand-built LLVM function
//! actually named `main` that just calls `tl_main` and truncates its `i64`
//! result to the `i32` process exit code.
//!
//! ## `use` and modules
//!
//! A `(use ...)` in the entry file loads the dependency it names through
//! the same [`crate::project::Loader`] `typl file.typl` uses, each under
//! its own file-derived module path (`Loader::load_uses_in`). The entry
//! file itself is the one exception: unlike `Loader::load_entry` (which
//! wraps *every* file it loads, entry included, in `<stem>`), the entry
//! file here stays at the root namespace — narrower than what `Loader`
//! would do on its own, and deliberately so, since wrapping it would change
//! `tl_main`/`ENTRY_POINT_INTERNAL_NAME` and everything downstream of it
//! that assumes a bare root `main`. A `defun main` anywhere else — inside a
//! `(module ...)` in the entry file, or anywhere in a `use`d file — can
//! never be reached as an entry point, so [`collect_aot_item`] rejects it
//! outright rather than silently compiling dead code under a confusing
//! name.

use std::cell::RefCell;
use std::fs;
use std::process::Command;
use std::rc::Rc;

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple};
use inkwell::values::{CallSiteValue, InstructionOpcode, Operand};
use inkwell::AddressSpace;
use inkwell::OptimizationLevel;

use crate::check::core;
use crate::compile::symbols::CompiledItem;
use crate::{Checker, Heap, Interp, Path, Reader, TopLevelForm, Value};

const ENTRY_POINT_NAME: &str = "main";
const ENTRY_POINT_INTERNAL_NAME: &str = "tl_main";

/// Registers one checked top-level form with `interp` and records what
/// [`compile_file`] must do with it: a compiled body to emit (`items`), a
/// global to re-initialize at startup (`defvar_inits`), or nothing.
///
/// Recurses into a `(module ...)`, which covers three shapes at once — the
/// monomorphization bundle a generic instantiation comes wrapped in, the
/// `(module ...)` a user writes, and the per-target-type grouping
/// `Checker::check_impl` returns for an `impl` block. (The entry file itself
/// is *not* one of these: unlike `typl file.typl`'s own `Loader::load_entry`,
/// [`compile_file`] keeps the entry file at the root namespace and only
/// wraps a `use`d dependency in its file-derived module — see the module doc
/// comment.) These are just containers of the same items; the enclosing
/// module is already baked into
/// each item's own fully-qualified `Path`, so flattening loses nothing —
/// each `defun`/`defmethod` becomes a [`CompiledItem`] by that full path,
/// never by its last segment alone, which is what lets a module-qualified
/// name and a same-named one in another module compile to different LLVM
/// symbols.
///
/// A generic template needs no special case any more: the checker emits an
/// empty `(module PATH)` for one, which flattens to nothing here exactly as it
/// registers nothing in `Interp::exec`.
fn collect_aot_item(
    heap: &mut Heap,
    interp: &mut Interp,
    tl: TopLevelForm,
    items: &mut Vec<CompiledItem>,
    defvar_inits: &mut Vec<(Path, Value)>,
    ffi_decls: &mut Vec<typelisp_front::eval::interp::FfiDecl>,
    entry_path: &Path,
) -> Result<(), String> {
    let tag = core::op(heap, tl).map(str::to_string).unwrap_or_default();
    if tag == "module" {
        let body = core::fields(heap, tl).map_err(|e| e.to_string())?;
        for item in body.into_iter().skip(1) {
            collect_aot_item(heap, interp, item, items, defvar_inits, ffi_decls, entry_path)?;
        }
        return Ok(());
    }
    let defvar_meta = match tag.as_str() {
        "defun" => {
            let path = core::path_field(heap, tl, 0).ok_or_else(|| "compile-file: defun without a name".to_string())?;
            // A `defun main` anywhere but the program's root is never called
            // — the entry point is always `entry_path` (`Path::root("main")`)
            // — so it is almost certainly a mistake (entry code written
            // inside a `module`, or a `use`d file's own unrelated `main`)
            // rather than an intentional name. Reject it outright rather
            // than silently compiling dead code under a confusing name.
            if path.last_segment() == ENTRY_POINT_NAME && &path != entry_path {
                return Err(format!(
                    "compile-file: `{}` is named `{}`, but only the top-level `defun main` at \
                     the program's root is the entry point — rename this one (a `defun main` \
                     nested in a module, or in a `use`d file, is never called)",
                    path, ENTRY_POINT_NAME
                ));
            }
            items.push(CompiledItem::Fn(path));
            None
        }
        "defmethod" => {
            let type_path = core::path_field(heap, tl, 0).ok_or_else(|| "compile-file: defmethod without a type".to_string())?;
            let method = match core::field(heap, tl, 1) {
                Some(Value::Symbol(id)) => heap.symbol_name(id).to_string(),
                _ => return Err("compile-file: defmethod without a name".to_string()),
            };
            items.push(CompiledItem::Method(type_path, method));
            None
        }
        // The whole form travels, not just the initializer: the global's
        // storage tagging is on the form (see `Interp::add_compiled_global_init`).
        "defvar" => {
            let name = core::path_field(heap, tl, 0).ok_or_else(|| "compile-file: defvar without a name".to_string())?;
            Some((name, tl))
        }
        // No codegen of their own. `exec` still runs: it records an enum's
        // variants and a struct's field representations, which
        // `collect_struct_and_enum_types` hands to the compile bridge.
        "defenum" | "defstruct" => None,
        // A thunk to emit, but not through `add_compiled_function` — there is
        // no body to translate. Kept aside for the module-building step,
        // which puts it in before any body is translated (a compiled call
        // site looks its callee up by name, and the island aborts on a name
        // it cannot find). `exec` below still runs, and building the JIT
        // thunk it builds is how a missing symbol is reported now rather than
        // by the linker later.
        "defffi" => {
            ffi_decls.push(typelisp_front::eval::interp::read_ffi_decl(heap, tl).map_err(|e| e.to_string())?);
            None
        }
        // `use`/`import`/`shadowing-import` (the checker lowers every
        // spelling to this one tag, `Checker::check_use_forms`): no codegen,
        // and the interpreted `exec` below is what makes the used names
        // resolve — a compiled call site never mentions a `use`, only the
        // fully-qualified path it resolved to. The dependency file itself
        // was already loaded and its own items collected by the `Loader`
        // pass in `compile_file`, before this form is ever reached; this
        // form is only the using file's own record of having asked for it.
        "use" => None,
        // A bare `(main)` at top level is the line that starts the program
        // under `typl file.typl`; the executable calls `main` on its own, so
        // here it is read and dropped — which is what lets one source file
        // be run either way. No other expression is: there is nothing to run
        // it in.
        "expr" if is_entry_call(heap, tl, entry_path) => return Ok(()),
        other => {
            return Err(format!(
                "compile-file only supports top-level `defun`/`defmethod`/`defvar`/`defconstant`/`defstruct`/`defenum`/`defffi`/`use`/`module`/`impl`, found `{}`",
                other
            ))
        }
    };
    // `exec` runs the `defvar`'s initializer through the ordinary
    // interpreter (unchanged — `promote_global` below reads back whatever
    // value it produced), same as it already does for every `defun`.
    interp.exec(heap, tl).map_err(|e| e.to_string())?;
    if let Some((name, form)) = defvar_meta {
        interp.promote_global(heap, &name).map_err(|e| e.to_string())?;
        defvar_inits.push((name, form));
    }
    Ok(())
}

/// Whether the checked top-level form `tl` is the `(main)` line an AOT
/// source may end with — [`is_entry_call`] for a caller holding a form of any
/// kind and no entry path of its own (the `eval` environment's replay,
/// `compile::dump::capture_program_dump`, which must drop the same line).
pub(crate) fn is_trailing_main(heap: &Heap, tl: Value) -> bool {
    core::op(heap, tl) == Some("expr") && is_entry_call(heap, tl, &Path::root(ENTRY_POINT_NAME))
}

/// Whether a top-level `(expr ...)` is `(main)` — a call of the entry
/// point, resolved to `entry_path`, with no arguments — the one expression
/// an AOT source may carry. `entry_path` is always `Path::root(ENTRY_POINT_NAME)`:
/// the entry file stays at the root namespace (see the module doc comment),
/// so a bare `(main)` written in it resolves there too.
fn is_entry_call(heap: &Heap, tl: Value, entry_path: &Path) -> bool {
    let Some(form) = core::field(heap, tl, 0) else { return false };
    if core::op(heap, form) != Some("call") {
        return false;
    }
    // `(call WRITTEN HOME PATH (R...) ARG...)`: the resolved path, and no
    // arguments past the representation list.
    let is_main = core::path_field(heap, form, 2).is_some_and(|p| &p == entry_path);
    is_main && core::fields(heap, form).map(|f| f.len() == 4).unwrap_or(false)
}

/// Reads `source_path`, compiles every `defun` in it (and every file it
/// `use`s), and links a native executable at `output_path`. See the module
/// doc comment for scope.
pub fn compile_file(source_path: &str, output_path: &str) -> Result<(), String> {
    crate::compile::driver::emitted_layout_is_runnable()?;
    let source =
        fs::read_to_string(source_path).map_err(|e| format!("failed to read \"{}\": {}", source_path, e))?;

    let mut heap = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    // The prelude first, and before `load_compiler` — the same order `typl`'s
    // own startup uses, and for the same reason on both sides: the prelude's
    // globals are numbered from zero as they load, so nothing may promote a
    // global ahead of it.
    //
    // An AOT executable used to get only the island, which meant an AOT
    // program could call the builtins and its own definitions and nothing
    // else: `abs`, `gcd`, `identity`, every stream and pathname helper — the
    // whole prelude — answered "no such function" at compile time. Its bodies
    // now come along in the executable (`prelude.bitcode`, linked into the
    // module below) and its globals get their storage at the executable's own
    // startup (`prelude.global_inits`).
    let prelude = crate::compile::prelude_bootstrap::load_for_aot(&mut heap, &mut chk, &mut interp)?;
    crate::load_compiler(&mut heap, &mut chk, &mut interp);

    let entry_path_on_disk = std::path::Path::new(source_path);
    let entry_dir = entry_path_on_disk
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let src_root = crate::project::find_src_root(&entry_dir).unwrap_or(entry_dir);
    // The entry *file's own* items stay at the root namespace, exactly as
    // before `use` was supported — only a *dependency* a `use` names is
    // loaded as a module of its own, under its file-derived path. This is
    // narrower than `typl file.typl`'s own `Loader::load_entry` (which
    // wraps the entry file too, in `<stem>`), and deliberately so: keeping
    // the entry file at root is what lets every existing AOT source — and
    // `ENTRY_POINT_INTERNAL_NAME`/`is_entry_call`'s bare `main` — go on
    // meaning exactly what it always has, while a `(use ...)` in it still
    // resolves like any other `use`.
    let mut loader = crate::project::Loader::new(src_root.clone());

    let reader = Reader::new();
    // One form at a time, like every other loader (`Reader::forms_in`): the
    // file an AOT build reads is the same file the interpreter reads, so it
    // has to be read the same way.
    let mut forms = reader.forms_in(source_path, &source);

    // Every top-level form in an AOT source file must be something with a
    // compiled body or none at all: `defun`/`defmethod` (bodies),
    // `defvar`/`defconstant` (a global plus an initializer),
    // `defstruct`/`defenum` (type definitions with no codegen of their own),
    // `use` (a dependency, loaded by `loader` below before the form that
    // names it is checked), or a `module`/`impl` grouping any of those.
    // Collected in declaration order: `items` so later steps know exactly
    // which of `interp`'s registered bodies are this file's (as opposed to
    // `load_compiler`'s own helper `defun`s sharing the same tables), and
    // `defvar_inits` (path, initializer expression) so the standalone
    // executable can re-establish each global's storage at its own startup
    // (see the loop below that generates one `add_compiled_global_init` step
    // per entry, and `Interp::promote_global`'s doc comment for why this
    // must promote eagerly, in this same declaration order, rather than
    // waiting for some `defun` body to reference a global the way JIT does).
    let mut items: Vec<CompiledItem> = Vec::new();
    let mut defvar_inits: Vec<(Path, Value)> = Vec::new();
    let mut ffi_decls: Vec<typelisp_front::eval::interp::FfiDecl> = Vec::new();
    let entry_path = Path::root(ENTRY_POINT_NAME);
    let mut entry_forms: Vec<TopLevelForm> = Vec::new();
    loop {
        let next = {
            let hook = typelisp_front::read::DriverReadEval::new(&mut chk, &interp);
            forms.next_form_with(&mut heap, Some(&hook)).map_err(|e| e.to_string())?
        };
        let Some((v, loc)) = next else { break };
        // A `(use ...)` among `v`'s top level (or nested in a `(module ...)`
        // it opens) loads the file it names — checked and queued into
        // `loader`'s own pending list, under its own file-derived module
        // path — *before* `v` itself is checked, so a later reference to
        // what it named resolves. No `cur_segs`: this file is never treated
        // as anything's *sibling* by the search (`Loader::load_uses_in`'s
        // own doc comment), so a `use` here only ever resolves against the
        // project's source root directly — the shape `examples/projects/
        // http`'s flat `src/` is in, and every other AOT source so far.
        loader.load_uses_in(&mut heap, &reader, &mut chk, &mut interp, std::slice::from_ref(&v)).map_err(|e| e.to_string())?;
        let tl = chk.check_form_at(&mut heap, &interp, v, Some(loc)).map_err(|e| e.to_string())?;
        // Rooted for the rest of the build, like the read form under it
        // (`next_form_with` roots each one and nothing here pops): `entry_forms`
        // is a Rust `Vec` the collector cannot see, and checking the next
        // form or running an initialiser below (`collect_aot_item`'s `exec`)
        // can collect. `defvar_inits` keeps pointing into these forms until
        // the compile pass, so they must outlive the loop too.
        heap.push_root(tl);
        entry_forms.push(tl);
    }
    for w in chk.take_warnings() {
        eprintln!("{}", w);
    }
    // Dependencies first (a `use`d file's own items, one `(module PATH
    // body...)` bundle per file — `collect_aot_item`'s existing recursion
    // into `module` flattens it), then the entry file's own forms in
    // declaration order. Execution order between the two never matters for
    // correctness (`collect_aot_item`'s doc comment), only that every item
    // is `exec`'d exactly once before the compile pass below.
    for tl in loader.take_pending() {
        collect_aot_item(&mut heap, &mut interp, tl, &mut items, &mut defvar_inits, &mut ffi_decls, &entry_path)?;
    }
    for tl in entry_forms {
        collect_aot_item(&mut heap, &mut interp, tl, &mut items, &mut defvar_inits, &mut ffi_decls, &entry_path)?;
    }

    if !items.iter().any(|i| matches!(i, CompiledItem::Fn(p) if p == &entry_path)) {
        return Err(format!(
            "no zero-argument `{}` defun found (required as the entry point)",
            ENTRY_POINT_NAME
        ));
    }

    // Deduplicated and ordered, so a file declaring three functions from one
    // library links it once and the command line is the same on every build.
    let link_libraries: Vec<String> = ffi_decls
        .iter()
        .filter_map(|d| d.library.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let ctx = crate::compile::llvm_context();
    let module = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("compiled_file");
        // Forward-declares every `rt_*` shim (no body) so `compile-call`'s
        // `get-function` finds one the same way it finds any other
        // already-defined function in this shared module — a call to a free
        // builtin (`sexpr-car`, `eval`, `stream-read-char`, ...) arrives
        // already named for its shim, `symbols::callee_symbol_name` having
        // made that choice bridge-side. Unlike the JIT path
        // (`Interp::compile_function`), no `add_global_mapping` is needed
        // here: these resolve as ordinary linker symbols against
        // `typelisp-rt`'s `staticlib` once `write_executable` links it in.
        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        for (name, _) in crate::compile::externs::rt_extern_functions() {
            module.add_function(name, fn_ty, None);
        }
        // The FFI thunks, before the prelude is linked in and long before any
        // body is translated: a compiled call to `c-strlen` resolves
        // `tl_c-strlen` with `get-function`, and the island **aborts the
        // process** on a name it cannot find. Each thunk declares its C
        // function too; unlike the JIT path nothing maps an address here —
        // these resolve as ordinary linker symbols, which is what the `-l`
        // flags below are for.
        for decl in &ffi_decls {
            crate::compile::ffi::emit_thunk(&module, decl)?;
        }
        // The prelude's compiled bodies, merged in here — *before* the first
        // `add_compiled_function` below, not after. The island's
        // `compile-call` resolves a callee with `get-function` against this
        // very module and aborts the process when it finds nothing, so a call
        // to `abs` needs `tl_abs` present by the time any body is translated,
        // not merely by the time the module is written out.
        let buffer =
            inkwell::memory_buffer::MemoryBuffer::create_from_memory_range_copy(prelude.bitcode, "prelude");
        let prelude_module = Module::parse_bitcode_from_buffer(&buffer, ctx)
            .map_err(|e| format!("the committed prelude bitcode failed to parse: {}", e))?;
        module
            .link_in_module(prelude_module)
            .map_err(|e| format!("linking the prelude into the compiled file failed: {}", e))?;
        Rc::new(RefCell::new(module))
    };

    {
        // Every body this file will emit, declared before any of them is
        // translated — the same thing the prelude artifact's generator does
        // with its own item list.
        //
        // Declaration order is not call order. A file's own `defun`s used to
        // be enough (a callee must be defined earlier in the file), but a
        // monomorphization bundle is emitted in the order the checker
        // *created* its instantiations, which is not a topological one:
        // `length <vector-iter<i32>,i32>` precedes the `vector-iter::next
        // <i32>` it calls. The island resolves a callee by looking it up in
        // this module and **aborts the process** when it finds nothing, so the
        // declaration has to be there first. Nothing exercised this until the
        // prelude arrived: a file that could not name a generic could not
        // instantiate one either.
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let m = module.borrow();
        // A *Lisp* function's type, so it must follow this process's emit ABI
        // rather than the fixed `(ptr, i32)` the `rt_*` shims keep. A
        // declaration and its definition are joined by name, so getting this
        // wrong is not a second declaration -- it is the body arriving at a
        // declaration whose type disagrees, and the prologue then reading the
        // argument pointer as a frame.
        let lisp_fn_ty = if crate::compile::EMITTED_BODY_ABI == typelisp_abi::BODY_ABI_COROUTINE {
            crate::compile::llvm_builtins::coroutine_fn_type()
        } else {
            let ptr_ty = ctx.ptr_type(AddressSpace::default());
            ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false)
        };
        for item in &items {
            let symbol = item.symbol_name();
            if m.get_function(&symbol).is_none() {
                m.add_function(&symbol, lisp_fn_ty, None);
            }
        }
    }

    // One shared module, one `add_compiled_function` call per `defun` —
    // *not* per-function modules merged afterward, see this module's doc
    // comment for why. `add_compiled_function` internally locks
    // `COMPILE_LOCK` per LLVM builtin call (via `eval_llvm_builtin_method`),
    // and `Mutex` isn't reentrant, so it must be called without the lock
    // already held — unlike the rest of this function from here on, which
    // touches the LLVM Context family of APIs directly with no calls back
    // into `Interp`/the typelisp compiler body.
    for item in &items {
        // Every user body's own LLVM symbol name gets the `tl_` prefix
        // (`crate::compile::USER_SYMBOL_PREFIX`) — `main` is no longer a
        // special case: `user_symbol_name("main")` already produces
        // `ENTRY_POINT_INTERNAL_NAME` ("tl_main"). A `defmethod`'s symbol is
        // `user_method_symbol_name`'s `tl_type::method`, exactly what a
        // compiled call site emits.
        crate::compile::driver::add_compiled_function(&interp, &mut heap, module.clone(), &item.node_name(), &item.symbol_name()).map_err(|e| e.to_string())?;
    }

    // One `add_compiled_global_init` per `defvar`, in the order
    // `promote_global` assigned their compile-time ids in — the prelude's
    // first, since `load_for_aot` ran before a line of this file was checked,
    // then the file's own in declaration order. `build_main_wrapper` below
    // emits a call to each, in this same order, from the generated `main`, so
    // the standalone executable reproduces that exact numbering at its own
    // runtime (see `Interp::promote_global`'s doc comment).
    //
    // The prelude's are not optional even for a program that never mentions
    // one: its compiled bodies address their globals by baked-in slot id, so
    // `*print-pretty*` has to have storage before anything that prints runs.
    let all_inits: Vec<(Path, Value)> =
        prelude.global_inits.iter().chain(defvar_inits.iter()).cloned().collect();
    let mut global_init_names: Vec<String> = Vec::with_capacity(all_inits.len());
    for (i, (_, form)) in all_inits.iter().enumerate() {
        let internal_name = format!("$global_init${}", i);
        crate::compile::driver::add_compiled_global_init(&interp, &mut heap, module.clone(), &internal_name, *form).map_err(|e| e.to_string())?;
        global_init_names.push(internal_name);
    }

    // Which types have a compiled `print-object`, for the printer's AOT
    // startup registration. Read off `items` rather than off the method
    // tables because the address has to name a function *this file* compiled:
    // `compile-file` compiles every top-level body in the file, so an `impl
    // print-object` in it is here whether or not anything calls it.
    let print_objects: Vec<(String, String)> = items
        .iter()
        .filter_map(|item| {
            let node = item.node_name();
            let symbol = item.symbol_name();
            // Two shapes, because a generic type's `print-object` is
            // monomorphized: `point::print-object` for a plain type, and
            // `gen::print-object <i32>` for one instantiation of a generic —
            // whose *value* carries the key `gen<i32>`, so that is what the
            // table has to be keyed by (`type_key::specialized_method_name`
            // is the same correspondence read the other way).
            let (type_name, targs) = match node.rsplit_once("::print-object") {
                Some((ty, "")) => (ty, None),
                Some((ty, rest)) => {
                    let args = rest.strip_prefix(" <")?.strip_suffix('>')?;
                    (ty, Some(args))
                }
                None => return None,
            };
            let path = Path::from_segments(type_name.split("::").map(str::to_string).collect());
            let base = crate::type_key::type_key_of(&path).into_owned();
            let key = match targs {
                Some(args) => format!("{}<{}>", base, args),
                None => base,
            };
            Some((key, symbol))
        })
        .collect();

    // The methods a `~/name/` directive in this file can reach, from the
    // checker's scan of each literal control string
    // (`Checker::format_call_methods`). The interpreter needs no such list —
    // it looks the name up in its own method table when the directive runs —
    // but a standalone executable has no table, so each pair becomes a
    // startup registration below, exactly as `print-object` does.
    let format_calls: Vec<(String, String, String)> = chk
        .format_call_methods()
        .into_iter()
        .map(|(path, method)| {
            let node = format!("{}::{}", path, method);
            let symbol = items
                .iter()
                .find(|item| item.node_name() == node)
                .map(|item| item.symbol_name())
                .ok_or_else(|| format!(
                    "compile-file: `~/{}/` names `{}`, which was not compiled into this file",
                    method, node
                ))?;
            Ok((crate::type_key::type_key_of(&path).into_owned(), method, symbol))
        })
        .collect::<Result<_, String>>()?;

    // Each global's path with the slot id `collect_aot_item` promoted it to,
    // for the `eval` environment's startup binding. Read back rather than
    // assumed to be the loop index: the ids are whatever
    // `typelisp_rt::global_new` handed out, and the executable reproducing
    // them is a property of the `$global_init$` call order, not of this list.
    let eval_globals: Vec<(String, usize)> = all_inits
        .iter()
        .map(|(path, _)| {
            let id = interp
                .compiled_global_id(path)
                .ok_or_else(|| format!("internal error: global `{}` was collected but never promoted", path))?;
            Ok((path.to_string(), id))
        })
        .collect::<Result<_, String>>()?;

    // Only when the program actually needs an interpreter ([`EVAL_SHIMS`]):
    // naming one of those shims is what makes the linker pull the checker and
    // the interpreter in, the same rule the printer's registration block
    // follows. Asked under its own short lock so the environment below —
    // which runs the checker and the interpreter, and touches no LLVM at all
    // — is built without holding it.
    let calls_eval = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let m = module.borrow();
        module_calls_any(&m, &EVAL_SHIMS)
    };
    // The `eval` environment, built here rather than at the executable's
    // startup: the prelude's checked state comes straight out of the committed
    // prelude dump, and this source's own is checked once, at compile time
    // (`typelisp_front::dump::capture_program_dump`).
    //
    // Its own throwaway `Heap`, and after `eval_globals` is read: the capture
    // creates an `Interp`, and `Interp::new` resets the runtime global table.
    // Nothing below consults it — `build_main_wrapper` emits ids as constants
    // and `write_executable` links — and `compile_file` already reset it once
    // at the top for its own `interp`.
    let eval_env: Option<Vec<u8>> = if calls_eval {
        let mut env_heap = Heap::with_capacity(EVAL_HEAP_CAPACITY);
        Some(crate::compile::dump::capture_program_dump(&mut env_heap, &source, &src_root, &eval_globals)?)
    } else {
        None
    };

    // What `main` answers with decides how the exit code is read off it: an
    // `int` is a tagged word (a fixnum's payload is the code; a bignum has
    // none), a fixed-width integer the raw word.
    let main_returns_int = chk
        .registry()
        .fn_sig(&Path::root("main"))
        .is_some_and(|sig| sig.ret == crate::Type::Int);

    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    let result = {
        let m = module.borrow();
        build_main_wrapper(
            ctx,
            &m,
            &global_init_names,
            &interp.vtable_descriptors(),
            &interp.upcast_descriptors(),
            &interp.enum_variant_descriptors(),
            &interp.field_template_descriptors(),
            &print_objects,
            &format_calls,
            eval_env.as_deref(),
            main_returns_int,
        )
            .and_then(|()| m.verify().map_err(|e| format!("module failed verification: {}", e)))
            .and_then(|()| write_executable(&m, output_path, &link_libraries))
    };
    // Destroyed here rather than left to fall out of scope: `module` was
    // declared before the guard, so its own drop would run *after* the guard
    // released, and `~Module` unregisters every value name from the shared
    // LLVM Context (see `compile::COMPILE_LOCK`). Written as a `result`
    // binding rather than `?`s so the failing paths take this route too.
    drop(module);
    result
}

/// Adds a real, C-ABI `main` to `module` that calls the compiled entry
/// point (`tl_main`, see [`ENTRY_POINT_INTERNAL_NAME`]) with no logical
/// arguments and returns its `i64` result truncated to an `i32` exit code.
/// See the module doc comment for why this can't just compile the file's
/// `main` defun under that name directly.
///
/// `global_init_names` (one `add_compiled_global_init`-produced zero-arg
/// function per `defvar`, in file-declaration order — see
/// `compile_file`'s own doc comments at its two call sites) are handed to
/// `rt_run_program`, which runs each, in that same order, after
/// `rt_heap_init` and before `tl_main`, every one as a task under the
/// program's scheduler: the heap needs to exist first (`rt_global_new`,
/// which every one of these eventually calls, roots into it), and every one
/// of them needs to run before `tl_main`'s own body — or anything it calls
/// — could read a global that doesn't have a slot yet.
///
/// `vtables` (one entry per trait object the file boxes, from
/// `Interp::vtable_descriptors`) are filled in first of all, before even the
/// heap exists: each slot is an `rt_vtable_set` call whose function-pointer
/// argument is a `ptrtoint` *constant* the linker resolves, so unlike the
/// JIT — which patches tables with addresses only known after each module is
/// JIT'd (`Interp::publish_vtables`) — AOT needs no runtime discovery, no
/// thunk, and no closure to carry a method's identity.
///
/// `upcasts` (from `Interp::upcast_descriptors`) is the same story for the
/// supertrait conversion table `rt_dyn_upcast` reads: pure integers, so the
/// startup sequence is three constants per entry and no symbol resolution
/// at all.
///
/// `eval_env` is `Some` only for a program that calls `eval`: the
/// environment, already built and serialized (`typelisp_front::dump`), for
/// the startup to read back into [`EVAL_HEAP_CAPACITY`] cells before `main`
/// proper begins.
fn build_main_wrapper(
    ctx: &'static Context,
    module: &Module<'static>,
    global_init_names: &[String],
    vtables: &[(u32, Vec<(Path, String)>)],
    upcasts: &[(u32, u32, u32)],
    enum_variants: &[(String, usize, String)],
    field_templates: &[(String, i64, i64, String)],
    print_objects: &[(String, String)],
    format_calls: &[(String, String, String)],
    eval_env: Option<&[u8]>,
    main_returns_int: bool,
) -> Result<(), String> {
    let tl_main = module
        .get_function(ENTRY_POINT_INTERNAL_NAME)
        .ok_or_else(|| "internal error: compiled entry point not found in module".to_string())?;

    let i32_type = ctx.i32_type();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
    let rt_heap_init = module.add_function("rt_heap_init", fn_ty, None);
    let main_fn = module.add_function(ENTRY_POINT_NAME, i32_type.fn_type(&[], false), None);
    let entry_block = ctx.append_basic_block(main_fn, "entry");
    let builder = ctx.create_builder();
    builder.position_at_end(entry_block);

    let null_args = ctx.ptr_type(AddressSpace::default()).const_null();
    let argc_zero = ctx.i32_type().const_int(0, false);

    // Trait-object vtables. Ordered before `rt_heap_init` only
    // because nothing here touches the heap; what matters is that every
    // table is complete before `tl_main` can reach a `:dyn` call site.
    if !vtables.is_empty() {
        let i64_ty = ctx.i64_type();
        let rt_vtable_set = module
            .get_function("rt_vtable_set")
            .ok_or_else(|| "internal error: rt_vtable_set not declared in module".to_string())?;
        let args_ptr = builder
            .build_alloca(i64_ty.array_type(4), "vtable_set_args")
            .map_err(|e| format!("failed to alloca vtable-set args: {}", e))?;
        for (id, slots) in vtables {
            for (slot, (type_name, method)) in slots.iter().enumerate() {
                let symbol = crate::compile::symbols::user_method_symbol_name(type_name, method);
                let target = module.get_function(&symbol).ok_or_else(|| {
                    format!(
                        "compile-file: dyn dispatch target `{}::{}` was not compiled into this file",
                        type_name, method
                    )
                })?;
                let fn_ptr = target.as_global_value().as_pointer_value().const_to_int(i64_ty);
                // `IntValue` is `Copy`, so the deref below is free — spelled
                // out because this crate is edition 2018, where an array's
                // `into_iter()` still yields references.
                // The fourth word is the ABI `target` answers to. Its LLVM
                // type is the authority -- the same question
                // `build-make-closure` asks of the function it boxes.
                let body_abi = if target.get_type() == crate::compile::llvm_builtins::coroutine_fn_type() {
                    typelisp_abi::BODY_ABI_COROUTINE
                } else {
                    typelisp_abi::BODY_ABI_CLASSIC
                };
                let set_args = [
                    i64_ty.const_int(*id as u64, false),
                    i64_ty.const_int(slot as u64, false),
                    fn_ptr,
                    i64_ty.const_int(u64::from(body_abi), false),
                ];
                for (i, v) in set_args.iter().enumerate() {
                    let v = *v;
                    let p = unsafe {
                        builder
                            .build_gep(i64_ty, args_ptr, &[i64_ty.const_int(i as u64, false)], "vtable_set_arg_ptr")
                            .map_err(|e| format!("failed to build vtable-set gep: {}", e))?
                    };
                    builder.build_store(p, v).map_err(|e| format!("failed to store vtable-set arg: {}", e))?;
                }
                builder
                    .build_call(rt_vtable_set, &[args_ptr.into(), ctx.i32_type().const_int(4, false).into()], "vtable_set_result")
                    .map_err(|e| format!("failed to build rt_vtable_set call: {}", e))?;
            }
        }
    }
    // The supertrait upcast table, for the same reason and at the same
    // point: a `:dyn` upcast in `tl_main` reads it, and it is all constants.
    if !upcasts.is_empty() {
        let i64_ty = ctx.i64_type();
        let rt_upcast_set = module
            .get_function("rt_upcast_set")
            .ok_or_else(|| "internal error: rt_upcast_set not declared in module".to_string())?;
        let args_ptr = builder
            .build_alloca(i64_ty.array_type(3), "upcast_set_args")
            .map_err(|e| format!("failed to alloca upcast-set args: {}", e))?;
        for (from, trait_id, to) in upcasts {
            let set_args = [
                i64_ty.const_int(*from as u64, false),
                i64_ty.const_int(*trait_id as u64, false),
                i64_ty.const_int(*to as u64, false),
            ];
            for (i, v) in set_args.iter().enumerate() {
                let v = *v;
                let p = unsafe {
                    builder
                        .build_gep(i64_ty, args_ptr, &[i64_ty.const_int(i as u64, false)], "upcast_set_arg_ptr")
                        .map_err(|e| format!("failed to build upcast-set gep: {}", e))?
                };
                builder.build_store(p, v).map_err(|e| format!("failed to store upcast-set arg: {}", e))?;
            }
            builder
                .build_call(rt_upcast_set, &[args_ptr.into(), ctx.i32_type().const_int(3, false).into()], "upcast_set_result")
                .map_err(|e| format!("failed to build rt_upcast_set call: {}", e))?;
        }
    }
    // The printer's two program facts (`typelisp_print::aot`): an enum's
    // variant *names* and each type's `print-object`, neither of which a
    // standalone executable can look up the way the interpreter does.
    //
    // Emitted only when this module actually calls a printing shim. That is
    // not an optimisation: a reference to the printer is what makes the
    // linker pull the whole directive engine into the executable, so a
    // program that never prints must not make one. Measured, on a
    // `(defun main () i32 42)`: 3,530,224 bytes with no printer symbol in it
    // at all when this guard does not trip, 3,877,648 and 124 of them when it
    // does.
    if module_calls_any(module, &PRINT_SHIMS) {
        let i64_ty = ctx.i64_type();
        // A `(pointer, length)` pair per string, from a module-level constant
        // — no heap involved, so this can run before `rt_heap_init` like the
        // two tables above.
        let literal = |builder: &inkwell::builder::Builder<'static>, text: &str| -> Result<(inkwell::values::IntValue<'static>, inkwell::values::IntValue<'static>), String> {
            let g = builder
                .build_global_string_ptr(text, "print_reg_str")
                .map_err(|e| format!("failed to build a printer-registration string: {}", e))?;
            Ok((
                g.as_pointer_value().const_to_int(i64_ty),
                i64_ty.const_int(text.len() as u64, false),
            ))
        };
        let call = |builder: &inkwell::builder::Builder<'static>, name: &str, words: &[inkwell::values::IntValue<'static>]| -> Result<(), String> {
            let f = module
                .get_function(name)
                .ok_or_else(|| format!("internal error: {} not declared in module", name))?;
            let args_ptr = builder
                .build_alloca(i64_ty.array_type(words.len() as u32), "print_reg_args")
                .map_err(|e| format!("failed to alloca printer-registration args: {}", e))?;
            for (i, w) in words.iter().enumerate() {
                let p = unsafe {
                    builder
                        .build_gep(i64_ty, args_ptr, &[i64_ty.const_int(i as u64, false)], "print_reg_arg_ptr")
                        .map_err(|e| format!("failed to build printer-registration gep: {}", e))?
                };
                builder.build_store(p, *w).map_err(|e| format!("failed to store printer-registration arg: {}", e))?;
            }
            builder
                .build_call(f, &[args_ptr.into(), ctx.i32_type().const_int(words.len() as u32 as u64, false).into()], "print_reg_result")
                .map_err(|e| format!("failed to build {} call: {}", name, e))?;
            Ok(())
        };
        for (key, variant, name) in enum_variants {
            let (key_ptr, key_len) = literal(&builder, key)?;
            let (name_ptr, name_len) = literal(&builder, name)?;
            call(
                &builder,
                "rt_print_enum_variant",
                &[key_ptr, key_len, i64_ty.const_int(*variant as u64, false), name_ptr, name_len],
            )?;
        }
        for (key, variant, index, template) in field_templates {
            let (key_ptr, key_len) = literal(&builder, key)?;
            let (t_ptr, t_len) = literal(&builder, template)?;
            call(
                &builder,
                "rt_print_field_template",
                &[key_ptr, key_len, i64_ty.const_int(*variant as u64, true), i64_ty.const_int(*index as u64, true), t_ptr, t_len],
            )?;
        }
        for (key, symbol) in print_objects {
            let target = module.get_function(symbol).ok_or_else(|| {
                format!("compile-file: `print-object` implementation `{}` was not compiled into this file", symbol)
            })?;
            let (key_ptr, key_len) = literal(&builder, key)?;
            let fn_ptr = classic_door(ctx, module, target)?;
            call(&builder, "rt_print_object_method", &[key_ptr, key_len, fn_ptr])?;
        }
        for (key, method, symbol) in format_calls {
            let target = module.get_function(symbol).ok_or_else(|| {
                format!("compile-file: `~/{}/` names `{}`, which was not compiled into this file", method, symbol)
            })?;
            let (key_ptr, key_len) = literal(&builder, key)?;
            let (name_ptr, name_len) = literal(&builder, method)?;
            let fn_ptr = classic_door(ctx, module, target)?;
            call(&builder, "rt_format_call_method", &[key_ptr, key_len, name_ptr, name_len, fn_ptr])?;
        }
    }
    // The environment `eval` needs, as one immutable blob. Registration only —
    // the shim just remembers the pointer — so this belongs with the other
    // pre-heap stores. Emitted, like the printer's block above, only when the
    // module actually calls `rt_eval`: naming the shim is what pulls the
    // checker and the interpreter into the executable.
    if let Some(bytes) = eval_env {
        let i64_ty = ctx.i64_type();
        let blob = module.add_global(ctx.i8_type().array_type(bytes.len() as u32), None, "typelisp_eval_env");
        blob.set_initializer(&ctx.const_string(bytes, false));
        blob.set_constant(true);
        call_shim(
            ctx,
            module,
            &builder,
            "rt_eval_state",
            &[blob.as_pointer_value().const_to_int(i64_ty), i64_ty.const_int(bytes.len() as u64, false)],
        )?;
    }
    // AOT's counterpart to the JIT path's `Interp::eval` calling
    // `runtime::set_active_heap` before every compiled call — see
    // `runtime::rt_heap_init`'s doc comment for why a standalone executable
    // has to create and register its own `Heap` here instead. Must run
    // before `tl_main` (or anything it calls) touches the heap at all.
    //
    // With `eval` in the program the capacity is given explicitly: the
    // environment built two lines down reads, checks and runs the prelude and
    // the program's own definitions into this same heap, which does not fit in
    // `rt_heap_init`'s default. The figure is the one the prelude's own
    // generator sizes its heap at (`prelude_bootstrap::build_prelude_bitcode`).
    match eval_env {
        None => {
            builder
                .build_call(rt_heap_init, &[null_args.into(), argc_zero.into()], "heap_init_result")
                .map_err(|e| format!("failed to build rt_heap_init call: {}", e))?;
        }
        Some(_) => {
            call_shim(ctx, module, &builder, "rt_heap_init", &[ctx.i64_type().const_int(EVAL_HEAP_CAPACITY as u64, false)])?;
        }
    }
    // Between the heap and the global inits, and that position is forced:
    // `rt_eval_init` builds an `Interp`, and `Interp::new` resets the runtime
    // global table — after the inits it would wipe the slots the machine code
    // addresses. See `typelisp_front::shim`'s module doc comment.
    if eval_env.is_some() {
        call_shim(ctx, module, &builder, "rt_eval_init", &[])?;
    }
    // The program proper: its global initialisers and then `tl_main`.
    //
    // Under the coroutine ABI a body answers with a status word and its
    // value is in a frame -- calling it as `f(args, argc)` is not even the
    // right arity, and a generated `main` cannot drive one itself. The
    // runtime's `rt_run_program` does: it puts a scheduler on its frame and
    // runs each initialiser, then `main`, as a task under it -- so an
    // initialiser that makes a channel or waits on a task is an ordinary
    // program rather than a startup special case. `main` hands it the
    // initialisers as a constant array of addresses (declaration order, which
    // is the order `Interp::promote_global` numbered their slots in) and the
    // entry point. Through the runtime rather than calling `tl_main` directly
    // also for the reason `rt_run_entry` gives: a `(panic ...)` that unwinds
    // out of the program needs a Rust frame to be caught in, and `main` is
    // the C entry point.
    //
    // Which shim, decided by `tl_main`'s own LLVM type -- the way the
    // `rt_vtable_set` block above asks it and for the same reason: the ABI
    // belongs to the function, not to the process. `EMITTED_BODY_ABI` says
    // what *this build* emits, and a module holds bodies this build did not
    // emit (`aot::tests` hand-builds a classic `tl_main`). An `int`-returning
    // `main` answers a tagged word; the `_int` shim reads the exit code out
    // of it (and out of a panic's raw code, which arrives through the same
    // call). Every initialiser must be a coroutine body too: the scheduler
    // has no way to drive a classic one, and a classic entry has no
    // scheduler to run one under -- either mix is refused here rather than
    // handed to a driver whose first act would be to ask for a frame the
    // prologue never published.
    //
    // A program that calls `eval` runs its scheduler through its `Interp`
    // instead (`rt_run_program_interp[_int]`, `typelisp_front::shim`) --
    // decided here, at generation time, by the same fact that decided
    // whether `rt_eval_init` runs at all (`eval_env.is_some()`), rather than
    // by a hook the two shims would otherwise have to agree on at runtime.
    // Without this an `eval`-carrying executable would run *two* schedulers
    // that never see each other: the bare one under `rt_run_program` for the
    // program's own tasks, and the `Interp`'s own for whatever an eval'd
    // `(task ...)` admits -- which is the AOT-scheduler work's remaining
    // limit this closes.
    let coroutine_fn_ty = crate::compile::llvm_builtins::coroutine_fn_type();
    let i64_ty = ctx.i64_type();
    let mut init_addrs: Vec<inkwell::values::IntValue<'static>> = Vec::with_capacity(global_init_names.len());
    for name in global_init_names {
        let f = module
            .get_function(name)
            .ok_or_else(|| format!("internal error: global-init function \"{}\" not found in module", name))?;
        if f.get_type() != coroutine_fn_ty {
            return Err(format!(
                "internal error: global initialiser \"{}\" is not a coroutine-ABI body, and only those run under the scheduler",
                name
            ));
        }
        init_addrs.push(f.as_global_value().as_pointer_value().const_to_int(i64_ty));
    }
    let entry_addr = tl_main.as_global_value().as_pointer_value().const_to_int(i64_ty);
    let call: CallSiteValue = if tl_main.get_type() == coroutine_fn_ty {
        let entry_shim = match (eval_env.is_some(), main_returns_int) {
            (true, true) => "rt_run_program_interp_int",
            (true, false) => "rt_run_program_interp",
            (false, true) => "rt_run_program_int",
            (false, false) => "rt_run_program",
        };
        let rt_run_program = module.add_function(entry_shim, fn_ty, None);
        let inits = module.add_global(i64_ty.array_type(init_addrs.len() as u32), None, "tl_global_inits");
        inits.set_initializer(&i64_ty.const_array(&init_addrs));
        inits.set_constant(true);
        let inits_addr = inits.as_pointer_value().const_to_int(i64_ty);
        let words = [inits_addr, i64_ty.const_int(init_addrs.len() as u64, false), entry_addr];
        let args_ptr = builder
            .build_alloca(i64_ty.array_type(words.len() as u32), "program_args")
            .map_err(|e| format!("failed to alloca the program arguments: {}", e))?;
        for (i, w) in words.iter().enumerate() {
            let slot = unsafe {
                builder
                    .build_gep(i64_ty, args_ptr, &[i64_ty.const_int(i as u64, false)], "program_arg_ptr")
                    .map_err(|e| format!("failed to build the program argument gep: {}", e))?
            };
            builder.build_store(slot, *w).map_err(|e| format!("failed to store a program argument: {}", e))?;
        }
        builder
            .build_call(rt_run_program, &[args_ptr.into(), ctx.i32_type().const_int(words.len() as u64, false).into()], "tl_main_result")
            .map_err(|e| format!("failed to build entry-point call: {}", e))?
    } else {
        if main_returns_int {
            return Err("an `int`-returning main under the classic ABI is not supported".to_string());
        }
        if !init_addrs.is_empty() {
            return Err("internal error: a classic-ABI entry point cannot run global initialisers (no scheduler)".to_string());
        }
        let rt_run_entry = module.add_function("rt_run_entry", i64_ty.fn_type(&[i64_ty.into()], false), None);
        builder
            .build_call(rt_run_entry, &[entry_addr.into()], "tl_main_result")
            .map_err(|e| format!("failed to build entry-point call: {}", e))?
    };
    let result = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => {
            return Err("internal error: entry point produced no value".to_string())
        }
    };
    let exit_code = builder
        .build_int_truncate(result, i32_type, "exit_code")
        .map_err(|e| format!("failed to truncate exit code: {}", e))?;
    builder.build_return(Some(&exit_code)).map_err(|e| format!("failed to build entry-point return: {}", e))?;
    Ok(())
}

/// The cons-cell arena an `eval`-carrying executable asks `rt_heap_init` for.
/// Its startup reads, checks and runs the whole prelude plus the program's own
/// definitions into this heap before `main` proper begins, which the runtime's
/// own default (`1 << 16`) does not hold.
const EVAL_HEAP_CAPACITY: usize = 1 << 18;

/// The name of the `eval` shim (`typelisp_front::shim`), which is what "this
/// program evaluates at runtime" means at the IR level.
/// The shims whose presence means the executable needs a whole interpreter in
/// it, not just the runtime.
///
/// `rt_eval` is the obvious one. The two registration shims are here because
/// a program that installs a reader macro means to *call* it while reading,
/// and calling one goes through the prelude's `call-reader-macro`
/// (`Interp::call_reader_macro_fn`) — so `read` in such a program would
/// otherwise report that there is no evaluator, having been given the
/// function and asked to run it. The price is the same one `eval` pays.
const EVAL_SHIMS: [&str; 3] = ["rt_eval", "rt_set_macro_character", "rt_set_dispatch_macro_character"];

/// Emits one call to the `rt_*` shim `name` with `words` as its argument
/// array — the startup-registration calling pattern, spelled once.
fn call_shim(
    ctx: &'static Context,
    module: &Module<'static>,
    builder: &inkwell::builder::Builder<'static>,
    name: &str,
    words: &[inkwell::values::IntValue<'static>],
) -> Result<(), String> {
    let i64_ty = ctx.i64_type();
    let f = module
        .get_function(name)
        .ok_or_else(|| format!("internal error: {} not declared in module", name))?;
    let args_ptr = if words.is_empty() {
        ctx.ptr_type(AddressSpace::default()).const_null()
    } else {
        let p = builder
            .build_alloca(i64_ty.array_type(words.len() as u32), "shim_args")
            .map_err(|e| format!("failed to alloca {} args: {}", name, e))?;
        for (i, w) in words.iter().enumerate() {
            let slot = unsafe {
                builder
                    .build_gep(i64_ty, p, &[i64_ty.const_int(i as u64, false)], "shim_arg_ptr")
                    .map_err(|e| format!("failed to build {} gep: {}", name, e))?
            };
            builder.build_store(slot, *w).map_err(|e| format!("failed to store {} arg: {}", name, e))?;
        }
        p
    };
    builder
        .build_call(f, &[args_ptr.into(), ctx.i32_type().const_int(words.len() as u64, false).into()], "shim_result")
        .map_err(|e| format!("failed to build {} call: {}", name, e))?;
    Ok(())
}

/// The names of the printing shims (`typelisp_print::shim`), which is what
/// "this program prints" means at the IR level.
const PRINT_SHIMS: [&str; 11] = [
    "rt_format",
    "rt_print",
    "rt_println",
    "rt_pprint",
    "rt_pprint_block_start",
    "rt_pprint_block_end",
    "rt_pprint_newline",
    "rt_pprint_indent",
    "rt_pprint_tab",
    "rt_pprint_pop",
    "rt_pprint_list_exhausted",
];

/// Whether any compiled body in `module` calls one of `names`.
///
/// Asked of [`PRINT_SHIMS`] to decide whether to emit the printer's startup
/// registration at all. It has to be a question about the *emitted calls*
/// rather than about the source: every `rt_*` shim is forward-declared in
/// every module (see `compile_file`), so the declaration's presence says
/// nothing, and a program that never prints must not reference the printer —
/// referencing it is exactly what makes the linker pull the directive engine
/// in (see `typelisp_print`'s crate doc comment).
fn module_calls_any(module: &Module<'static>, names: &[&str]) -> bool {
    let mut f = module.get_first_function();
    while let Some(func) = f {
        for block in func.get_basic_blocks() {
            let mut instr = block.get_first_instruction();
            while let Some(i) = instr {
                if i.get_opcode() == InstructionOpcode::Call {
                    // A direct call's callee is its *last* operand; under
                    // opaque pointers it is the callee global itself, whose
                    // name is the symbol the linker will look for.
                    let n = i.get_num_operands();
                    if let Some(Operand::Value(v)) = n.checked_sub(1).and_then(|last| i.get_operand(last)) {
                        if v.is_pointer_value() {
                            let name = v.into_pointer_value().get_name().to_string_lossy().into_owned();
                            if names.contains(&name.as_str()) {
                                return true;
                            }
                        }
                    }
                }
                instr = i.get_next_instruction();
            }
        }
        f = func.get_next_function();
    }
    false
}

/// The path to the `typelisp-rt` crate's `staticlib` artifact, which
/// exports the `#[no_mangle]` runtime shims in [`crate::compile::runtime`]
/// (`rt_ping`/`rt_heap_init`/`rt_heap_live_count`, and from Stage 3 onward
/// `rt_cons`/`rt_car`/...) as plain C symbols. Linked into every AOT
/// executable below so calls to those shims resolve the same way a call to
/// another `defun` in the file does — see `runtime`'s module doc comment.
///
/// Deliberately `typelisp-front`'s artifact, not this (`typelisp`) crate's —
/// `typelisp` embeds all of LLVM via `inkwell`, and linking *that* into a
/// tiny AOT executable drags in LLVM's entire system-library footprint
/// (`libc++`, zlib, libffi, terminfo, ...) for no benefit; nothing at or
/// below `typelisp-front` names LLVM at all.
///
/// It is `typelisp-front`'s and not `typelisp-rt`'s only because `cc` takes
/// *one* archive and `typelisp-front` sits above `typelisp-rt`, so its
/// artifact is the one that holds both. Front's own objects — the checker,
/// the interpreter, the prelude source — are what `rt_eval` needs and
/// nothing else does, and the linker leaves every one of them out of a
/// program that does not call it: measured at +784 bytes and zero
/// `typelisp_front` symbols for `(defun main () i32 42)` against linking
/// `typelisp-rt` alone. That is the whole reason the front end is a crate;
/// see its doc comment, and `Cargo.toml`'s `profile.dev.package` entries for
/// the one thing that quietly breaks it.
///
/// Computed from `CARGO_MANIFEST_DIR` + the build profile this very test/
/// binary was compiled under (`debug_assertions` tracks `dev`/`test` vs
/// `release`) rather than hardcoded — see the project's policy on
/// machine-specific absolute paths. The workspace shares one `target/` dir at
/// the repo root, so `CARGO_MANIFEST_DIR` (this crate's own root) is the
/// right base for every member's artifacts.
///
/// **This artifact is not built by the `cargo` invocation that runs an AOT
/// test.** `cargo test` builds `typelisp-front`'s *rlib* (the dependency this
/// crate links) and never its `staticlib` target, so what is on disk here is
/// whatever the last `cargo build -p typelisp-front` / `cargo build
/// --workspace` left. Adding an `rt_*` shim therefore links against a runtime
/// that predates it and fails with an undefined symbol — observed adding
/// `rt_apply_any`, where the JIT tests all passed and only AOT broke.
/// `scripts/test-serial.sh` builds the staticlib first for exactly this
/// reason; a bare `cargo test --test compile_file_test` needs it built by
/// hand.
fn staticlib_path() -> String {
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    format!("{}/target/{}/libtypelisp_front.a", env!("CARGO_MANIFEST_DIR"), profile)
}

/// A [`TargetMachine`] for the machine this process is running on.
///
/// Shared by the two things that turn a module into machine code:
/// [`write_executable`], which writes an object file and links it, and
/// `driver::disassemble_function`, which writes assembly text and shows it to
/// a person. One target machine and one set of options, so what
/// `disassemble` prints is the code `compile-file` would actually emit —
/// including `OptimizationLevel::None`, which is why the assembly reads the
/// way the IR does.
fn host_target_machine() -> Result<TargetMachine, String> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("failed to initialize native target: {}", e))?;
    let triple = host_triple()?;
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            OptimizationLevel::None,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create a target machine for the host triple".to_string())
}

/// The host triple, with the minimum macOS version the executable is linked
/// for (see [`macos_version_min`]). LLVM's default names the running OS
/// (`x86_64-apple-darwin24.6.0`), which would make the object claim a newer
/// macOS than the static library beside it and the executable it goes into.
#[cfg(target_os = "macos")]
fn host_triple() -> Result<TargetTriple, String> {
    let default = TargetMachine::get_default_triple();
    let default = default.as_str().to_string_lossy();
    let arch = default
        .split_once("-apple-")
        .map(|(arch, _)| arch)
        .ok_or_else(|| format!("internal error: the host triple `{}` is not an Apple one", default))?;
    Ok(TargetTriple::create(&format!("{}-apple-macosx{}", arch, macos_version_min())))
}

#[cfg(not(target_os = "macos"))]
fn host_triple() -> Result<TargetTriple, String> {
    Ok(TargetMachine::get_default_triple())
}

/// The minimum macOS version this build of `typl` was compiled for, which is
/// what its static library's objects claim — `compile-file` links for the same
/// one rather than Apple clang's default (`build.rs` says why).
#[cfg(target_os = "macos")]
fn macos_version_min() -> &'static str {
    env!("TYPELISP_MACOSX_DEPLOYMENT_TARGET")
}

/// `module` as host assembly text — `(disassemble name)`'s answer.
///
/// Emitted to memory rather than to a file: the caller is showing it to a
/// person, not linking it, so there is nothing for a temporary file to be
/// for. Must be called with [`crate::compile::COMPILE_LOCK`] held, like every
/// other use of the shared LLVM context.
pub(crate) fn assembly_of(module: &Module<'static>) -> Result<String, String> {
    let tm = host_target_machine()?;
    let buf = tm
        .write_to_memory_buffer(module, FileType::Assembly)
        .map_err(|e| format!("failed to emit assembly: {}", e))?;
    String::from_utf8(buf.as_slice().to_vec()).map_err(|e| format!("the assembler emitted invalid UTF-8: {}", e))
}

/// Emits `module` to an object file and links it into a native executable
/// at `output_path` via the system `cc`. Must be called with
/// [`crate::compile::COMPILE_LOCK`] held.
/// The address `typelisp-print` should hold for `target`: its own, when
/// `target` answers to the classic ABI, and otherwise a four-instruction
/// classic function that hands it to `rt_drive_body`.
///
/// The printer's two registries (`~/name/` and `print-object`) call what they
/// are given as `(ptr, i32) -> i64`, and that crate has no driver to reach for
/// — it deliberately does not depend on `typelisp-rt`. So the wrapper is
/// emitted here, where the callee's LLVM type says which ABI it answers to.
/// Built once per method and reused if the same one is registered twice.
fn classic_door(
    ctx: &'static Context,
    module: &Module<'static>,
    target: inkwell::values::FunctionValue<'static>,
) -> Result<inkwell::values::IntValue<'static>, String> {
    let i64_ty = ctx.i64_type();
    if target.get_type() != crate::compile::llvm_builtins::coroutine_fn_type() {
        return Ok(target.as_global_value().as_pointer_value().const_to_int(i64_ty));
    }
    let name = format!("{}$classic", target.get_name().to_string_lossy());
    if let Some(existing) = module.get_function(&name) {
        return Ok(existing.as_global_value().as_pointer_value().const_to_int(i64_ty));
    }
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let classic_ty = i64_ty.fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
    let door = module.add_function(&name, classic_ty, None);
    let drive = match module.get_function("rt_drive_body") {
        Some(f) => f,
        None => module.add_function("rt_drive_body", classic_ty, None),
    };
    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(door, "entry"));
    let args = builder
        .build_alloca(i64_ty.array_type(3), "drive_args")
        .map_err(|e| format!("failed to alloca the driver arguments: {}", e))?;
    let words = [
        target.as_global_value().as_pointer_value().const_to_int(i64_ty),
        builder
            .build_ptr_to_int(
                door.get_nth_param(0).expect("the classic signature has two parameters").into_pointer_value(),
                i64_ty,
                "callee_args",
            )
            .map_err(|e| format!("failed to take the argument array's address: {}", e))?,
        builder
            .build_int_z_extend(
                door.get_nth_param(1).expect("the classic signature has two parameters").into_int_value(),
                i64_ty,
                "callee_argc",
            )
            .map_err(|e| format!("failed to widen the argument count: {}", e))?,
    ];
    for (i, w) in words.iter().enumerate() {
        let p = unsafe {
            builder
                .build_gep(i64_ty, args, &[i64_ty.const_int(i as u64, false)], "drive_arg_ptr")
                .map_err(|e| format!("failed to index the driver arguments: {}", e))?
        };
        builder.build_store(p, *w).map_err(|e| format!("failed to store a driver argument: {}", e))?;
    }
    let out = builder
        .build_call(drive, &[args.into(), ctx.i32_type().const_int(3, false).into()], "drive_result")
        .map_err(|e| format!("failed to build the rt_drive_body call: {}", e))?;
    let out = match out.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => {
            return Err("internal error: rt_drive_body produced no value".to_string())
        }
    };
    builder.build_return(Some(&out)).map_err(|e| format!("failed to build the door's return: {}", e))?;
    Ok(door.as_global_value().as_pointer_value().const_to_int(i64_ty))
}

fn write_executable(module: &Module<'static>, output_path: &str, libraries: &[String]) -> Result<(), String> {
    // The whole program's IR, for reading. An AOT module is the one the
    // island's output is hardest to see: it is neither `disassemble`'s
    // single function nor a committed dump, and by the time anything is
    // wrong it is machine code. `TYPELISP_AOT_IR=<path>` writes it here.
    if let Ok(path) = std::env::var("TYPELISP_AOT_IR") {
        std::fs::write(&path, module.print_to_string().to_string())
            .map_err(|e| format!("failed to write {}: {}", path, e))?;
    }
    let target_machine = host_target_machine()?;

    // Named from `output_path` (not e.g. the process id) so it can't
    // collide across concurrently-running `cargo test` threads in the same
    // process — every caller already needs a distinct `output_path` of its
    // own, for the same reason.
    let object_path = format!("{}.o", output_path);
    target_machine
        .write_to_file(module, FileType::Object, std::path::Path::new(&object_path))
        .map_err(|e| format!("failed to emit object file: {}", e))?;

    // `-l` for every library a `defffi` named. Nothing is needed for a
    // declaration without one: what it reaches is already linked (libc comes
    // with `cc`, and the rest is in the static library beside it).
    let mut link = Command::new("cc");
    #[cfg(target_os = "macos")]
    link.arg(format!("-mmacosx-version-min={}", macos_version_min()));
    let status = link
        .arg(&object_path)
        .arg(staticlib_path())
        .args(libraries.iter().map(|l| format!("-l{}", l)))
        .arg("-o")
        .arg(output_path)
        .status()
        .map_err(|e| format!("failed to invoke the system linker (`cc`): {}", e));
    let _ = fs::remove_file(&object_path);
    let status = status?;

    if !status.success() {
        return Err(format!("linker failed with status {}", status));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;

    use inkwell::AddressSpace;

    use super::{build_main_wrapper, write_executable, ENTRY_POINT_INTERNAL_NAME};
    use crate::compile::{llvm_context, COMPILE_LOCK};

    fn tmp_path(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp");
        std::fs::create_dir_all(&dir).expect("failed to create the AOT test scratch dir");
        dir.join(name)
    }

    /// Stage 0's proof that AOT-linked native code can call a `#[no_mangle]`
    /// Rust function from this crate's own `staticlib` artifact (see
    /// `super::staticlib_path`'s doc comment) through nothing more than an
    /// ordinary `declare` + `call` against the shared compiled-function ABI
    /// — no per-shim linking mechanism needed, the same way Stage 3 of
    /// labels/closures already lets one JIT-compiled function call another
    /// by name. Bypasses the typelisp compiler entirely (hand-builds the
    /// module via inkwell) since this only needs to test the link step, not
    /// anything `core_bridge`/`compiler.rs` does.
    #[test]
    fn aot_output_can_call_an_rt_extern_function() {
        let ctx = llvm_context();
        let _guard = COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("rt_ping_test");

        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let rt_ping = module.add_function("rt_ping", fn_ty, None);
        let tl_main = module.add_function(ENTRY_POINT_INTERNAL_NAME, fn_ty, None);

        let builder = ctx.create_builder();
        let entry = ctx.append_basic_block(tl_main, "entry");
        builder.position_at_end(entry);
        let one_slot = builder.build_alloca(ctx.i64_type(), "one_slot").unwrap();
        builder.build_store(one_slot, ctx.i64_type().const_int(41, false)).unwrap();
        let argc_one = ctx.i32_type().const_int(1, false);
        let call = builder.build_call(rt_ping, &[one_slot.into(), argc_one.into()], "rt_ping_result").unwrap();
        let result = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => panic!("rt_ping call produced no value"),
        };
        builder.build_return(Some(&result)).unwrap();

        build_main_wrapper(ctx, &module, &[], &[], &[], &[], &[], &[], &[], None, false).expect("build_main_wrapper failed");
        module.verify().expect("module failed verification");

        let out_path = tmp_path("rt_ping_test");
        write_executable(&module, out_path.to_str().unwrap(), &[]).expect("write_executable failed");

        let status = Command::new(&out_path).status().expect("failed to run the compiled executable");
        assert_eq!(status.code(), Some(42));
    }

    /// Stage 1's AOT-side proof: `build_main_wrapper` now inserts a call to
    /// `rt_heap_init` before `tl_main` ever runs (see its doc comment), so a
    /// freshly-started AOT executable can call `rt_heap_live_count` (or any
    /// future `rt-cons`/`rt-car`/...) with no Rust embedder around to have
    /// registered a `Heap` for it — this `tl_main` never calls `rt_heap_init`
    /// itself, only `build_main_wrapper`'s generated `main` does.
    #[test]
    fn aot_main_wrapper_initializes_a_heap_before_tl_main_runs() {
        let ctx = llvm_context();
        let _guard = COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("rt_heap_init_test");

        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let rt_heap_live_count = module.add_function("rt_heap_live_count", fn_ty, None);
        let tl_main = module.add_function(ENTRY_POINT_INTERNAL_NAME, fn_ty, None);

        let builder = ctx.create_builder();
        let entry = ctx.append_basic_block(tl_main, "entry");
        builder.position_at_end(entry);
        let null_args = ptr_ty.const_null();
        let argc_zero = ctx.i32_type().const_int(0, false);
        let call = builder.build_call(rt_heap_live_count, &[null_args.into(), argc_zero.into()], "live_count").unwrap();
        let result = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => panic!("rt_heap_live_count call produced no value"),
        };
        builder.build_return(Some(&result)).unwrap();

        build_main_wrapper(ctx, &module, &[], &[], &[], &[], &[], &[], &[], None, false).expect("build_main_wrapper failed");
        module.verify().expect("module failed verification");

        let out_path = tmp_path("rt_heap_init_test");
        write_executable(&module, out_path.to_str().unwrap(), &[]).expect("write_executable failed");

        let status = Command::new(&out_path).status().expect("failed to run the compiled executable");
        assert_eq!(status.code(), Some(0));
    }
}
