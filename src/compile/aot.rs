//! `compile-file`: the AOT exit. Reads an independent typelisp source file
//! (its own fresh `Heap`/`Checker`/`Interp` — deliberately *not* the
//! caller's, since the whole point of AOT output is a self-contained
//! executable that doesn't depend on whatever else happens to be loaded in
//! the process that requested it), compiles every top-level `defun` into
//! one shared LLVM module (reusing
//! [`crate::compile::driver::add_compiled_function`], the exact same
//! per-function compile step the JIT path (`driver::compile_function`)
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
//! reference restriction `typeck::call` enforces at type-checking
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
//! `main` defun keeps its ordinary mangled symbol (`tl_p::main` for `p.typl`),
//! and [`build_main_wrapper`] adds a separate, hand-built LLVM function
//! actually named `main` that just calls it and truncates its `i64`
//! result to the `i32` process exit code.
//!
//! ## `use` and modules
//!
//! The entry file is a module, exactly as under `typl file.typl`
//! (`Loader::load_entry`): its items live under the path its location
//! derives to (`project::module_segs_for`), so `p.typl`'s `point` is
//! `p::point` in an executable as it is in the interpreter, and a value
//! prints the same either way. A `(use ...)` in it loads the dependency it
//! names through the same [`crate::project::Loader`], each under its own
//! file-derived module path (`Loader::load_uses_in`). The entry point is the
//! entry file's own `main`; a `defun main` anywhere else — inside a
//! `(module ...)` in the entry file, or anywhere in a `use`d file — can
//! never be reached as an entry point, so [`collect_aot_item`] rejects it
//! outright rather than silently compiling dead code under a confusing
//! name.

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{InstructionOpcode, Operand};
use inkwell::AddressSpace;

use crate::check::core;
use crate::compile::symbols::target_symbol_name;
use crate::CompileTarget;
use crate::{Checker, Heap, Interp, Path, Reader, TopLevelForm, Value};
use typelisp_mem::{wk, ConcreteNames};
mod archive;
mod bare;
mod direct;
mod settle;
mod soft_float;
mod made;
mod masking;
mod native;
mod reach;
pub(crate) mod avr;
mod embedded;
mod wasm;

pub use archive::*;
pub(crate) use native::*;


const ENTRY_POINT_NAME: &str = "main";

/// Registers one checked top-level form with `interp` and records what
/// [`compile_file`] must do with it: a compiled body to emit (`items`), a
/// global to re-initialize at startup (`defvar_inits`), or nothing.
///
/// Recurses into a `(module ...)`, which covers three shapes at once — the
/// monomorphization bundle a generic instantiation comes wrapped in, the
/// `(module ...)` a user writes, and the per-target-type grouping
/// `typeck::impl_block` returns for an `impl` block, and the file-derived
/// module a `use`d dependency comes wrapped in. These are just containers of
/// the same items; the enclosing
/// module is already baked into
/// each item's own fully-qualified `Path`, so flattening loses nothing —
/// each `defun`/`defmethod` becomes a [`CompileTarget`] by that full path,
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
    items: &mut Vec<CompileTarget>,
    defvar_inits: &mut Vec<(Path, Value)>,
    ffi_decls: &mut Vec<typelisp_front::eval::interp::FfiDecl>,
    entry_path: &Path,
) -> Result<(), String> {
    let tag = core::op_wk(heap, tl).unwrap_or(typelisp_mem::NOT_WELL_KNOWN);
    if tag == wk::MODULE {
        let body = core::fields(heap, tl).map_err(|e| e.to_string())?;
        for item in body.into_iter().skip(1) {
            collect_aot_item(heap, interp, item, items, defvar_inits, ffi_decls, entry_path)?;
        }
        return Ok(());
    }
    let defvar_meta = match tag {
        wk::DEFUN => {
            let path = core::definition_field(heap, tl, 0).ok_or_else(|| "compile-file: defun without a name".to_string())?;
            // A `defun main` other than the entry file's own is never called
            // — the entry point is always `entry_path` — so it is almost
            // certainly a mistake (entry code written inside a `module`, or a
            // `use`d file's own unrelated `main`) rather than an intentional
            // name. Reject it outright rather than silently compiling dead
            // code under a confusing name.
            if path.last_segment() == ENTRY_POINT_NAME && &path != entry_path {
                return Err(format!(
                    "compile-file: `{}` is named `{}`, but only the entry file's top-level \
                     `defun main` (`{}`) is the entry point — rename this one (a `defun main` \
                     nested in a module, or in a `use`d file, is never called)",
                    path, ENTRY_POINT_NAME, entry_path
                ));
            }
            items.push(CompileTarget::Fn(path));
            None
        }
        wk::DEFMETHOD => {
            let type_path = core::definition_field(heap, tl, 0).ok_or_else(|| "compile-file: defmethod without a type".to_string())?;
            let method = match core::field(heap, tl, 1) {
                Some(Value::Symbol(id)) => id,
                _ => return Err("compile-file: defmethod without a name".to_string()),
            };
            items.push(CompileTarget::Method(type_path, method));
            None
        }
        // The whole form travels, not just the initializer: the global's
        // storage tagging is on the form (see `driver::add_compiled_global_init`).
        wk::DEFVAR => {
            let name = core::definition_field(heap, tl, 0).ok_or_else(|| "compile-file: defvar without a name".to_string())?;
            Some((name, tl))
        }
        // No codegen of their own. `exec` still runs: it records an enum's
        // variants and a struct's field representations, which
        // `collect_struct_and_enum_types` hands to the compile bridge.
        wk::DEFENUM | wk::DEFSTRUCT => None,
        // Nothing to emit for either. A macro has done all its work by the
        // time anything is compiled: every use was expanded while its form
        // was checked. `exec` still registers the expander, so an `eval` in
        // the executable expands the same way the source did. A
        // `defsignature` only promised the checker a definition that a
        // later form supplies.
        wk::DEFMACRO | wk::DEFSIGNATURE => None,
        // A thunk to emit, but not through `add_compiled_function` — there is
        // no body to translate. Kept aside for the module-building step,
        // which puts it in before any body is translated (a compiled call
        // site looks its callee up by name, and the island aborts on a name
        // it cannot find). `exec` below still runs, and building the JIT
        // thunk it builds is how a missing symbol is reported now rather than
        // by the linker later.
        wk::DEFFFI => {
            ffi_decls.push(typelisp_front::eval::interp::read_ffi_decl(heap, tl).map_err(|e| e.to_string())?);
            None
        }
        // `use`/`import`/`shadowing-import` (the checker lowers every
        // spelling to this one tag, `typeck::use_form`): no codegen,
        // and the interpreted `exec` below is what makes the used names
        // resolve — a compiled call site never mentions a `use`, only the
        // fully-qualified path it resolved to. The dependency file itself
        // was already loaded and its own items collected by the `Loader`
        // pass in `compile_file`, before this form is ever reached; this
        // form is only the using file's own record of having asked for it.
        wk::USE => None,
        // A bare `(main)` at top level is the line that starts the program
        // under `typl file.typl`; the executable calls `main` on its own, so
        // here it is read and dropped — which is what lets one source file
        // be run either way. No other expression is: there is nothing to run
        // it in.
        wk::EXPR if is_entry_call(heap, tl, entry_path) => return Ok(()),
        _ => {
            return Err(format!(
                "compile-file only supports top-level `defun`/`defmethod`/`defvar`/`defparameter`/`defconstant`/`defmacro`/`defsignature`/`defstruct`/`defenum`/`defffi`/`use`/`module`/`impl`, found `{}`",
                core::op_spelling(heap, tl).unwrap_or_default()
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
/// kind (the `eval` environment's replay, `compile::dump::capture_program_dump`,
/// which must drop the same line).
pub(crate) fn is_trailing_main(heap: &Heap, tl: Value, entry_path: &Path) -> bool {
    core::op_is(heap, tl, wk::EXPR) && is_entry_call(heap, tl, entry_path)
}

/// The entry point of an entry file whose module path is `segs`: its own
/// `main`, which is where a bare `(main)` written in it resolves.
pub(crate) fn entry_point_path(segs: &[String]) -> Path {
    let mut segs = segs.to_vec();
    segs.push(ENTRY_POINT_NAME.to_string());
    Path::from_segments(segs)
}

/// Whether a top-level `(expr ...)` is `(main)` — a call of the entry
/// point, resolved to `entry_path` ([`entry_point_path`]), with no
/// arguments — the one expression an AOT source may carry.
fn is_entry_call(heap: &Heap, tl: Value, entry_path: &Path) -> bool {
    let Some(form) = core::field(heap, tl, 0) else { return false };
    if !core::op_is(heap, form, wk::CALL) {
        return false;
    }
    // `(call PATH RET-R (R...) ARG...)`: the resolved path, and no arguments
    // past the representation list.
    let is_main = core::definition_field(heap, form, 0).is_some_and(|p| &p == entry_path);
    is_main && core::fields(heap, form).map(|f| f.len() == 3).unwrap_or(false)
}

/// [`compile_file`] for a caller that is itself a running program — the
/// `(compile-file ...)` builtin, reached from the REPL or from a file under
/// `typl`.
///
/// The build's fresh `Heap`/`Interp` cannot share a thread with the caller's.
/// Much of the runtime's state is per thread rather than per `Interp`: the
/// compiled-global and vtable tables (`typelisp_rt::shared`), the registered
/// heap and interpreter. `Interp::new` clears those tables for the
/// environment it starts, so building on the caller's thread left the caller
/// reading the build's tables afterwards — its first read of a compiled
/// global (the printer's settings, on the next `println`) indexed a
/// permanent root its own heap does not have. A thread of its own starts the
/// build from empty tables and leaves the caller's untouched.
///
/// The stack is sized for the island's deeply recursive compile pipeline, as
/// the bootstrappers size theirs; a spawned thread otherwise gets far less
/// than the main thread `typl -c` runs [`compile_file`] on.
pub fn compile_file_on_own_thread(source_path: &str, output_path: &str) -> Result<(), String> {
    let (source_path, output_path) = (source_path.to_string(), output_path.to_string());
    let build = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || compile_file(&source_path, &output_path))
        .map_err(|e| format!("failed to start the build thread: {}", e))?;
    match build.join() {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

/// Reads `source_path`, compiles every `defun` in it (and every file it
/// `use`s), and links a native executable at `output_path`. See the module
/// doc comment for scope.
///
/// A release build when the process is one ([`crate::release`]): the
/// checker leaves `eprint`/`eprintln` out, and the executable carries the
/// flag for whatever it `eval`s.
pub fn compile_file(source_path: &str, output_path: &str) -> Result<(), String> {
    compile_file_with(source_path, output_path, &BuildOptions::default())
}

/// What `typl -c` sets about the executable it builds, beyond the code: the
/// resources it may use at run time. The default is what
/// `(compile-file ...)` builds with.
#[derive(Clone, Debug, Default)]
pub struct BuildOptions {
    /// The cons arena's capacity, in cells (`--heap-cells`); the arena is
    /// fixed at it. `None` is the program's default: an arena of
    /// [`typelisp_rt::DEFAULT_AOT_HEAP_CAPACITY`] cells, or
    /// [`EVAL_HEAP_CAPACITY`] for a program that calls `eval`, or
    /// [`BOARD_HEAP_CAPACITY`] for a board, that grows as
    /// `Heap::with_capacity` lets it.
    pub heap_cells: Option<usize>,
    /// The bound on the bytes the running executable allocates
    /// (`--max-memory`, `typelisp_rt::mem_limit`). `None` is no bound.
    pub max_memory: Option<usize>,
    /// No cons arena at all (`--no-cons`): a program that would take a cell
    /// from it is refused at compile time (`reach`), and the executable's
    /// arena has no cells. Not with [`Self::heap_cells`].
    pub no_cons: bool,
    /// What the executable runs on (`--target`).
    pub target: Target,
    /// For a board ([`Target::board`]): the file that says where its flash
    /// and RAM are (`--linker-script`), in the form of `cortex-m-rt`'s
    /// `memory.x`. `None` is the QEMU board that stands for the target's
    /// core ([`embedded::Board::qemu_memory`]).
    pub linker_script: Option<std::path::PathBuf>,
    /// For a board: who gives the board's functions (`--board-functions`).
    pub board_functions: BoardFunctions,
    /// For a board: objects and static libraries to link the program with
    /// (`--link`) — the board's functions when it gives them as C
    /// functions, a vendor's library the program calls through `defffi`.
    pub link: Vec<std::path::PathBuf>,
}

/// Who gives a board's executable the functions it asks of the board
/// (`typelisp_host_*`: writing, reading, ending, the clock, waiting).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoardFunctions {
    /// The runtime, over semihosting: what QEMU (`-semihosting`) and a
    /// debugger answer.
    #[default]
    Semihosting,
    /// The board, as C functions the link is given (`--link`).
    C,
}

impl BoardFunctions {
    /// The choice `--board-functions` names.
    pub fn parse(name: &str) -> Result<BoardFunctions, String> {
        match name {
            "semihosting" => Ok(BoardFunctions::Semihosting),
            "c" => Ok(BoardFunctions::C),
            _ => Err(format!("--board-functions: unknown `{}` (semihosting or c)", name)),
        }
    }

    /// The runtime built for it, in `embedded_runtime_dir`: the static
    /// library of `typelisp-rt-semihosting` or of `typelisp-rt-embedded`.
    pub(crate) fn archive(self) -> &'static str {
        match self {
            BoardFunctions::Semihosting => "libtypelisp_rt_semihosting.a",
            BoardFunctions::C => "libtypelisp_rt_embedded.a",
        }
    }
}

/// What an executable is built to run on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// The machine `typl` runs on: a native executable.
    #[default]
    Host,
    /// `wasm32-wasip1`: a WebAssembly module for a WASI (preview 1) host.
    Wasip1,
    /// `wasm32-unknown-unknown`: a WebAssembly module that imports what it
    /// needs of its host from JavaScript, with the JavaScript that loads it.
    Wasm32Unknown,
    /// `thumbv7em-none-eabihf`: an ELF executable for a Cortex-M4F or M7
    /// board without an operating system.
    Thumbv7em,
    /// `thumbv8m.main-none-eabihf`: the same for a Cortex-M33 board.
    Thumbv8mMain,
    /// `avr-atmega328p`: an ELF executable for the ATmega328P (Arduino Uno),
    /// which has no runtime ([`avr`]).
    AvrAtmega328p,
}

impl Target {
    /// The target `--target` names.
    pub fn parse(name: &str) -> Result<Target, String> {
        match name {
            "wasm32-wasip1" => Ok(Target::Wasip1),
            "wasm32-unknown-unknown" => Ok(Target::Wasm32Unknown),
            "thumbv7em-none-eabihf" => Ok(Target::Thumbv7em),
            "thumbv8m.main-none-eabihf" => Ok(Target::Thumbv8mMain),
            "avr-atmega328p" => Ok(Target::AvrAtmega328p),
            _ => Err(format!(
                "--target: unknown target `{}` (the ones there are: wasm32-wasip1, wasm32-unknown-unknown, {})",
                name,
                Target::board_names()
            )),
        }
    }

    /// The targets that are boards without an operating system.
    pub const BOARDS: [Target; 3] = [Target::Thumbv7em, Target::Thumbv8mMain, Target::AvrAtmega328p];

    /// The board the target is, if it is one.
    pub fn board(self) -> Option<&'static embedded::Board> {
        match self {
            Target::Thumbv7em => Some(&embedded::CORTEX_M4),
            Target::Thumbv8mMain => Some(&embedded::CORTEX_M33),
            Target::AvrAtmega328p => Some(&avr::ATMEGA328P),
            Target::Host | Target::Wasip1 | Target::Wasm32Unknown => None,
        }
    }

    /// Whether there is a runtime for the target. An executable for one there
    /// is none for is the program's own code and nothing else.
    pub fn has_runtime(self) -> bool {
        self != Target::AvrAtmega328p
    }

    /// The boards' triples, for a message.
    pub fn board_names() -> String {
        Target::BOARDS.iter().map(|t| t.name()).collect::<Vec<_>>().join(", ")
    }

    /// The runtime crates the target has not got, whose shims a program
    /// compiled for it must not reach.
    pub(crate) fn missing_runtimes(self) -> &'static [crate::compile::externs::RuntimeCrate] {
        use crate::compile::externs::RuntimeCrate;
        match self {
            Target::Host => &[],
            Target::Wasip1 => &[RuntimeCrate::Posix],
            Target::Wasm32Unknown => &[RuntimeCrate::Sys, RuntimeCrate::Posix],
            Target::Thumbv7em | Target::Thumbv8mMain | Target::AvrAtmega328p => &[RuntimeCrate::Sys, RuntimeCrate::Posix, RuntimeCrate::Front],
        }
    }

    /// The C data model of the machine the target is.
    pub fn data_model(self) -> typelisp_mem::target::DataModel {
        use typelisp_mem::target::DataModel;
        match self {
            Target::Host => DataModel::HOST,
            Target::Wasip1 | Target::Wasm32Unknown | Target::Thumbv7em | Target::Thumbv8mMain => DataModel::ILP32,
            Target::AvrAtmega328p => DataModel::AVR,
        }
    }

    /// The target triple, as `--target` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Target::Host => "host",
            Target::Wasip1 => "wasm32-wasip1",
            Target::Wasm32Unknown => "wasm32-unknown-unknown",
            Target::Thumbv7em => "thumbv7em-none-eabihf",
            Target::Thumbv8mMain => "thumbv8m.main-none-eabihf",
            Target::AvrAtmega328p => "avr-atmega328p",
        }
    }
}

/// [`compile_file`] with the run-time resources `options` gives.
pub fn compile_file_with(source_path: &str, output_path: &str, options: &BuildOptions) -> Result<(), String> {
    if options.no_cons && options.heap_cells.is_some() {
        return Err("--no-cons and --heap-cells cannot be combined: without cons cells there is no arena to size".to_string());
    }
    crate::compile::driver::emitted_layout_is_runnable()?;
    // Code for another machine: C's `long` and a pointer are as wide as they
    // are there, for the checker's layouts and casts as much as for the code
    // generated.
    let _data_model = typelisp_mem::target::DataModel::producing(options.target.data_model());
    // Another machine's C functions are that machine's: while its program
    // compiles, `defffi` does not look for them here. (Called here, they
    // would be called with the other machine's widths.)
    let _c_functions = crate::compile::ffi::CFunctionsOn::target(options.target != Target::Host);
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
    // are compiled into the executable as far as the program reaches them
    // (below), and its globals get their storage at the executable's own
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
    // The entry file is the module its location derives to, as under `typl
    // file.typl` (see the module doc comment): the same path, so the same
    // type names and the same printed values.
    let entry_segs = crate::project::module_segs_for(entry_path_on_disk, &src_root).map_err(|e| e.to_string())?;
    let mut loader = crate::project::Loader::new(src_root.clone());

    let reader = Reader::new();
    // One form at a time, like every other loader (`Reader::forms_within`):
    // the file an AOT build reads is the same file the interpreter reads, so
    // it has to be read the same way — its symbols interned in its module.
    let mut forms = reader.forms_within(source_path, &source, typelisp_front::mem::symbols::ns_of(&entry_segs));
    chk.enter_file_module(&entry_segs).map_err(|e| e.to_string())?;

    // Every top-level form in an AOT source file must be something with a
    // compiled body or none at all: `defun`/`defmethod` (bodies),
    // `defvar`/`defconstant` (a global plus an initializer),
    // `defstruct`/`defenum` (type definitions with no codegen of their own),
    // `defmacro`/`defsignature` (nothing left to emit once checked),
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
    let mut items: Vec<CompileTarget> = Vec::new();
    let mut defvar_inits: Vec<(Path, Value)> = Vec::new();
    let mut ffi_decls: Vec<typelisp_front::eval::interp::FfiDecl> = Vec::new();
    let entry_path = entry_point_path(&entry_segs);
    let mut entry_forms: Vec<TopLevelForm> = Vec::new();
    loop {
        let next = {
            let hook = typelisp_front::read::DriverReadEval::new(&mut chk, &mut interp);
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
        // A `defmacro` (or a readtable registration) has to be registered
        // before the next form is checked, or a use of it there finds no
        // expander. Run now and not collected, the rule `Loader` keeps for a
        // `use`d file's own forms: it leaves nothing to compile.
        if typelisp_front::project::needs_immediate_exec(&heap, tl) {
            interp.exec(&mut heap, tl).map_err(|e| e.to_string())?;
            continue;
        }
        // Rooted for the rest of the build, like the read form under it
        // (`next_form_with` roots each one and nothing here pops): `entry_forms`
        // is a Rust `Vec` the collector cannot see, and checking the next
        // form or running an initialiser below (`collect_aot_item`'s `exec`)
        // can collect. `defvar_inits` keeps pointing into these forms until
        // the compile pass, so they must outlive the loop too.
        heap.push_root(tl);
        entry_forms.push(tl);
    }
    chk.exit_file_module();
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

    if !items.iter().any(|i| matches!(i, CompileTarget::Fn(p) if p == &entry_path)) {
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
    // One guard across the whole build — module creation, every island-driven
    // `llvm-*` builtin in the loops below, the `main` wrapper, verification
    // and the object emission — so no other thread's IR building interleaves
    // with this one on the shared Context. The inner takes nest (see
    // `COMPILE_LOCK`); the non-LLVM work in between (the `eval` environment)
    // runs under it too, which only costs other threads' wait.
    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    // From here on everything emitted is the executable's, but for the
    // `eval` environment's checking.
    let emitting = || crate::compile::carriers::word_of(options.target.data_model().pointer_bits());
    let word_width = emitting();
    let module = {
        let module = ctx.create_module("compiled_file");
        // Forward-declares every `rt_*` shim (no body) so `compile-call`'s
        // `get-function` finds one the same way it finds any other
        // already-defined function in this shared module — a call to a free
        // builtin (`sexpr-car`, `eval`, `stream-read-char`, ...) arrives
        // already named for its shim, `symbols::callee_symbol_name` having
        // made that choice bridge-side. Unlike the JIT path
        // (`driver::compile_function`), no addresses are passed
        // here: these resolve as ordinary linker symbols against
        // `typelisp-rt`'s `staticlib` once `write_executable` links it in.
        crate::compile::externs::declare_runtime_functions(&module);
        // The FFI thunks, long before any
        // body is translated: a compiled call to `c-strlen` resolves
        // `tl_c-strlen` with `get-function`, and the island **aborts the
        // process** on a name it cannot find. Each thunk declares its C
        // function too; unlike the JIT path nothing maps an address here —
        // these resolve as ordinary linker symbols, which is what the `-l`
        // flags below are for.
        //
        // Where there is no runtime they are direct functions, emitted once
        // which functions are is known.
        for decl in ffi_decls.iter().filter(|_| options.target.has_runtime()) {
            crate::compile::ffi::emit_thunk(&module, decl)?;
        }
        Rc::new(RefCell::new(module))
    };

    // Every body this file will emit, translated for the island before any
    // is emitted: whether a function is direct (`crate::compile::direct`) is
    // read off what it does, and decides how every call to it is emitted.
    // Rooted for the rest of the build, like the forms they come from.
    let translated = |heap: &mut Heap, item: &CompileTarget| -> Result<direct::Body, String> {
        let (params, body) = interp.compiled_fn_body(item).map_err(|e| e.to_string())?;
        let ret = interp.compiled_fn_ret(item).map_err(|e| e.to_string())?;
        let (param_list, body) =
            crate::compile::driver::translate(&interp, heap, &params, &body, &std::collections::HashSet::new()).map_err(|e| e.to_string())?;
        heap.push_root(param_list);
        heap.push_root(body);
        let params = params.into_iter().map(|(_, r)| r).collect();
        Ok(direct::Body { symbol: target_symbol_name(item), params, ret, param_list, body })
    };
    let mut bodies: Vec<direct::Body> = Vec::with_capacity(items.len());
    // The prelude's functions by symbol. A specialization of one that this
    // file asks for too is the same function and is emitted once, as the
    // program's.
    let mut prelude_items: std::collections::HashMap<String, &CompileTarget> =
        prelude.items.iter().map(|i| (target_symbol_name(i), i)).collect();
    for item in &items {
        let body = translated(&mut heap, item)?;
        prelude_items.remove(&body.symbol);
        bodies.push(body);
    }
    let mut init_bodies: Vec<Value> = Vec::with_capacity(defvar_inits.len());
    // The program's own globals that can be static (`llvm_builtins::globals`),
    // by place in `defvar_inits`, each with its id and whether it can be
    // assigned.
    let mut statics: std::collections::BTreeMap<usize, (usize, bool)> = Default::default();
    let mut static_inits: Vec<direct::Body> = Vec::new();
    for (i, (path, form)) in defvar_inits.iter().enumerate() {
        let body = crate::compile::driver::translate_global_init(&interp, &mut heap, *form).map_err(|e| e.to_string())?;
        heap.push_root(body);
        init_bodies.push(body);
        let assigned = !matches!(core::field(&heap, *form, 2), Some(Value::Bool(false)));
        if static_global(&heap, body, assigned, options.target) {
            let id = interp.compiled_global_id(path).ok_or_else(|| format!("internal error: global `{}` was never promoted", path))?;
            statics.insert(i, (id, assigned));
            static_inits.push(direct::Body {
                symbol: format!("$global_init${}", prelude.global_inits.len() + i),
                params: Vec::new(),
                ret: crate::check::repr::Repr::Unit,
                param_list: Value::Empty,
                body,
            });
        }
    }
    let other_inits: Vec<Value> = init_bodies.iter().enumerate().filter(|(i, _)| !statics.contains_key(i)).map(|(_, b)| *b).collect();
    // The functions whose address the executable hands the runtime, each
    // entered under the coroutine ABI.
    let mut pinned: std::collections::HashSet<String> = std::collections::HashSet::new();
    if options.target.has_runtime() {
        pinned.insert(target_symbol_name(&CompileTarget::Fn(entry_path)));
    }
    pinned.extend(items.iter().chain(&prelude.items).filter(|i| print_object_receiver(i).is_some()).map(target_symbol_name));
    let handlers: Vec<(u16, String)> =
        chk.interrupt_handlers().into_iter().map(|(vector, path)| (vector, target_symbol_name(&CompileTarget::Fn(path)))).collect();
    // With no runtime, what the board enters instead.
    let mut vectors: Vec<(u16, String)> = Vec::new();
    if options.target.has_runtime() {
        pinned.extend(handlers.iter().map(|(_, symbol)| symbol.clone()));
    } else {
        for ((vector, path), (_, symbol)) in chk.interrupt_handlers().iter().zip(&handlers) {
            vectors.push((avr::interrupt_number(options.target, *vector, path)?, symbol.clone()));
        }
    }
    let entered: Vec<String> = vectors.iter().map(|(_, symbol)| symbol.clone()).collect();
    pinned.extend(chk.ffi_callbacks().into_iter().map(|sig| ConcreteNames::symbol(&sig.function.to_string())));
    for (_, slots) in interp.vtable_descriptors() {
        pinned.extend(slots.iter().map(|(type_name, method)| ConcreteNames::method_symbol(type_name, method.name())));
    }
    let entry_symbol = target_symbol_name(&CompileTarget::Fn(entry_path));
    let direct::Direct { sigs, emit, entry: direct_entry, called, interprets, why_not } = direct::direct_functions(&mut heap, &module.borrow(), &bodies, &static_inits, &other_inits, &pinned, &entry_symbol, &entered, options.target.has_runtime(), |heap, symbol| {
        prelude_items.get(symbol).map(|item| translated(heap, item)).transpose()
    })?;
    bodies.extend(emit);
    if interprets {
        statics.clear();
    }
    if !options.target.has_runtime() {
        // What a program cannot do without the runtime is told from what it
        // is made of; what is left is emitted as far as the entry calls it,
        // and whether that calls the runtime is seen once it is (`avr`).
        let mut whys = why_not;
        whys.extend(
            defvar_inits.iter().enumerate().filter(|(i, _)| !statics.contains_key(i)).map(|(_, (path, _))| format!("the global variable `{}`", path)),
        );
        // A `defconstant`'s value is worked out at compile time.
        whys.extend(
            defvar_inits
                .iter()
                .filter(|(path, _)| chk.var_type(path).is_some_and(|(ty, assigned)| assigned && ty == crate::Type::Int))
                .map(|(path, _)| format!("the global variable `{}`, an `int`, which has no fixed width: write a type of a fixed width, such as `i16`", path)),
        );
        whys.extend(chk.ffi_callbacks().iter().map(|sig| format!("`{}`, given to C as a callback", sig.function)));
        if !whys.is_empty() {
            return Err(avr::needs_runtime(options.target, &whys));
        }
        bodies.retain(|b| called.contains(&b.symbol));
    }
    let mut sigs = sigs;
    if !options.target.has_runtime() {
        for decl in &ffi_decls {
            sigs.insert(ConcreteNames::symbol(&decl.path.to_string()), crate::compile::ffi::direct_sig(decl)?);
            crate::compile::ffi::emit_direct_thunk(&module.borrow(), decl)?;
        }
    }
    let direct_names: Vec<String> = sigs.keys().cloned().collect();
    let producing_direct = crate::compile::direct::producing(sigs, options.target.has_runtime());
    let producing_static =
        crate::compile::llvm_builtins::globals::producing(
            statics.values().map(|(id, assigned)| (*id as i64, *assigned)).collect(),
        );
    // What runs at startup for each static global, its variable, and whether
    // that can be assigned.
    let static_globals: Vec<(String, String, bool)> = statics
        .iter()
        .map(|(i, (id, assigned))| {
            (format!("$global_init${}", prelude.global_inits.len() + i), crate::compile::llvm_builtins::globals::symbol(*id), *assigned)
        })
        .collect();

    {
        // Every body this file will emit, declared before any of them is
        // emitted — the same thing the prelude artifact's generator does
        // with its own item list.
        //
        // Declaration order is not call order. A file's own `defun`s used to
        // be enough (a callee must be defined earlier in the file), but a
        // monomorphization bundle is emitted in the order the checker
        // *created* its instantiations, which is not a topological one:
        // `length <vector-iter<i32>,i32>` precedes the `vector-iter::next
        // <i32>` it calls. The island resolves a callee by looking it up in
        // this module and **aborts the process** when it finds nothing, so the
        // declaration has to be there first. Under the whole-build guard
        // above.
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
            crate::compile::carriers::word().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false)
        };
        let symbols = bodies.iter().map(|b| &b.symbol).chain(prelude_items.keys());
        for symbol in symbols {
            if m.get_function(symbol).is_none() {
                let ty = crate::compile::direct::sig_of(symbol).map_or(lisp_fn_ty, |sig| sig.fn_type());
                m.add_function(symbol, ty, None);
            }
        }
    }

    // One shared module, one `compile-function` per body — *not*
    // per-function modules merged afterward, see this module's doc comment
    // for why. Each LLVM builtin takes `COMPILE_LOCK`, nesting inside the
    // whole-build guard above. Every user body's own LLVM symbol name is
    // `ConcreteNames::symbol`'s — `main` is no longer a special case:
    // `build_main_wrapper` calls it by that same symbol (`tl_p::main` for
    // `p.typl`). A method's symbol is `ConcreteNames::method_symbol`'s
    // `tl_type''method`, exactly what a compiled call site emits.
    for b in &bodies {
        crate::compile::driver::run_compile_function(&interp, &mut heap, module.clone(), &b.symbol, b.param_list, b.body).map_err(|e| e.to_string())?;
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
    let mut global_init_names: Vec<String> = Vec::with_capacity(prelude.global_inits.len() + defvar_inits.len());
    let prelude_inits: &[(Path, Value)] = if options.target.has_runtime() { &prelude.global_inits } else { &[] };
    for (_, form) in prelude_inits {
        let internal_name = format!("$global_init${}", global_init_names.len());
        crate::compile::driver::add_compiled_global_init(&interp, &mut heap, module.clone(), &internal_name, *form).map_err(|e| e.to_string())?;
        global_init_names.push(internal_name);
    }
    for (i, body) in init_bodies.iter().enumerate() {
        let internal_name = format!("$global_init${}", global_init_names.len());
        if !statics.contains_key(&i) {
            crate::compile::driver::run_compile_function(&interp, &mut heap, module.clone(), &internal_name, Value::Empty, *body).map_err(|e| e.to_string())?;
        }
        global_init_names.push(internal_name);
    }
    let all_inits: Vec<(Path, Value)> =
        prelude.global_inits.iter().chain(defvar_inits.iter()).cloned().collect();
    // The rest of the prelude the executable may need: the `print-object`s
    // the printer may choose, the functions whose address the runtime is
    // handed, and what they, the globals' initializers and
    // anything else emitted so far refer to. Which of it can run, the finished
    // `main` says; what cannot is not linked in.
    let mut wanted: Vec<String> = prelude
        .items
        .iter()
        .filter(|i| print_object_receiver(i).is_some())
        .map(target_symbol_name)
        .chain(pinned)
        .filter(|_| options.target.has_runtime())
        .collect();
    loop {
        let missing = |m: &Module<'static>, symbol: &str| m.get_function(symbol).is_some_and(|f| f.count_basic_blocks() == 0);
        wanted.retain(|symbol| prelude_items.contains_key(symbol) && missing(&module.borrow(), symbol));
        wanted.extend(
            prelude_items
                .keys()
                .filter(|symbol| {
                    let m = module.borrow();
                    let f = m.get_function(symbol);
                    use inkwell::values::BasicValue;
                    f.is_some_and(|f| f.count_basic_blocks() == 0 && f.as_global_value().as_pointer_value().get_first_use().is_some())
                })
                .cloned(),
        );
        wanted.sort();
        wanted.dedup();
        if wanted.is_empty() {
            break;
        }
        for symbol in std::mem::take(&mut wanted) {
            let b = translated(&mut heap, prelude_items[&symbol])?;
            crate::compile::driver::run_compile_function(&interp, &mut heap, module.clone(), &b.symbol, b.param_list, b.body).map_err(|e| e.to_string())?;
        }
    }
    let skipped_safepoints = crate::compile::direct::skipped_safepoints();
    // Every body is emitted. What compiles from here on compiles for this
    // process (the `eval` environment's checking), and nothing there is direct.
    drop(producing_direct);
    drop(producing_static);
    let running_inits = settle::settle_inits(&module.borrow(), &static_globals, options.target.has_runtime())?;
    settle::settle(&module.borrow(), &direct_names, options.target.has_runtime())?;
    if !options.target.has_runtime() {
        bare::check_stack(&module.borrow(), &direct_names)?;
        bare::link_into(ctx, &module.borrow())?;
        let result = {
            let m = module.borrow();
            let entry = direct_entry.as_deref().and_then(|name| m.get_function(name)).ok_or("internal error: the entry point was not compiled")?;
            let main_returns_int = chk.fn_ret(&entry_path).is_some_and(|ret| ret == crate::Type::Int);
            let inits: Vec<_> = running_inits.iter().filter_map(|name| m.get_function(name)).collect();
            let handlers = vectors
                .iter()
                .map(|(n, symbol)| {
                    let f = if *symbol == entry_symbol { Some(entry) } else { m.get_function(symbol) };
                    f.map(|f| (*n, f)).ok_or_else(|| format!("internal error: the interrupt handler `{}` was not compiled", symbol))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let entered: Vec<_> = handlers.iter().map(|(_, f)| *f).collect();
            let variables: Vec<String> = static_globals.iter().map(|(_, variable, _)| variable.clone()).collect();
            let mask_globals = || -> Result<(), String> {
                let masked = masking::mask(&m, &variables, entry, &entered)?;
                for ((i, _), variable) in statics.iter().zip(&variables) {
                    if let Some(masked) = masked.get(variable) {
                        let accessed = match (masked.read, masked.written) {
                            (true, true) => "read or written",
                            (true, false) => "read",
                            _ => "written",
                        };
                        eprintln!(
                            "warning: interrupts are disabled while the global variable `{}` is {}: it is {} bytes, and an interrupt handler that uses it could come between them",
                            defvar_inits[*i].0, accessed, masked.bytes
                        );
                    }
                }
                Ok(())
            };
            let reached: Result<(), String> =
                inits.iter().copied().chain([entry]).chain(handlers.iter().map(|(_, f)| *f)).try_for_each(|f| avr::check_reaches_no_runtime(&m, options.target, f));
            reached
                .and_then(|()| mask_globals())
                .and_then(|()| bare_main(ctx, &m, &inits, !handlers.is_empty(), entry, main_returns_int))
                .and_then(|()| handlers.iter().try_for_each(|(n, f)| avr::vector(&m, *n, *f)))
                .and_then(|()| crate::compile::type_table::settle_for_export(&m).map(|_| ()))
                .and_then(|()| m.verify().map_err(|e| format!("module failed verification: {}", e)))
                .and_then(|()| direct::check_only_called(&m, &direct_names))
                .and_then(|()| write_ir(&m))
                .and_then(|()| avr::write_executable(&m, output_path, &items, &handlers.iter().map(|(n, _)| *n).collect::<Vec<_>>(), options))
        };
        // Under the guard, as at the end of this function.
        drop(module);
        return result;
    }
    if !running_inits.is_empty() {
        return Err(format!("internal error: a static global's initial value is not a constant ({})", running_inits.join(", ")));
    }

    // Which types have a compiled `print-object`, for the printer's AOT
    // startup registration to choose from (`live_types` keeps those
    // the executable can make a value of): this file's, and the prelude's own (`complex`,
    // `pathname`, the streams, the error types, ...), which no item of this
    // file names — they are chosen by the printer at run time, never called.
    // Their bodies are in the module already, so registering them is all that
    // is missing;
    // without it an executable printed a `complex` as `#<complex re: 1.0 im:
    // 2.0>` where `typl` said `#C(1.0 2.0)`. Read off the items rather than
    // off the method tables because the address has to name a function the
    // module has a body for: `compile-file` compiles every top-level body in
    // the file, so an `impl print-object` in it is here whether or not
    // anything calls it.
    //
    // A primitive's is left out: the executable's printer asks the
    // registration about a struct's or an enum's value only
    // (`typelisp_print::aot`), and no primitive's value is either.
    let mut print_objects: Vec<(typelisp_mem::TypeId, String)> = Vec::new();
    for item in items.iter().chain(&prelude.items) {
        if let Some(ty) = print_object_receiver(item) {
            let primitive = matches!(typelisp_mem::type_shape(ty), typelisp_mem::TypeShape::Named(ns, ..) if ns == typelisp_mem::StdModule::PRIMITIVE.ns());
            if !primitive && !print_objects.iter().any(|(t, _)| *t == ty) {
                print_objects.push((ty, target_symbol_name(item)));
            }
        }
    }

    // Each global's path with the slot id `collect_aot_item` promoted it to,
    // for the `eval` environment's startup binding. Read back rather than
    // assumed to be the loop index: the ids are whatever
    // `typelisp_rt::global_new` handed out, and the executable reproducing
    // them is a property of the `$global_init$` call order, not of this list.
    let eval_globals: Vec<(Path, usize)> = all_inits
        .iter()
        .map(|(path, _)| {
            let id = interp
                .compiled_global_id(path)
                .ok_or_else(|| format!("internal error: global `{}` was collected but never promoted", path))?;
            Ok((*path, id))
        })
        .collect::<Result<_, String>>()?;

    // Which globals each compiled body reads or assigns, by id: what decides
    // which of the prelude's globals the executable initialises
    // (`build_main_wrapper`). Read off the bodies' core forms, where a
    // global is still named — the IR addresses it by an id stored as a
    // constant, which says nothing a walk can rely on.
    let globals_read = |heap: &Heap, forms: &[Value]| -> Result<Vec<usize>, String> {
        let targets = crate::compile::core_bridge::collect_targets(heap, forms).map_err(|e| e.to_string())?;
        targets
            .globals
            .iter()
            .map(|path| {
                interp
                    .compiled_global_id(path)
                    .ok_or_else(|| format!("internal error: global `{}` is read and was never promoted", path))
            })
            .collect()
    };
    let mut body_globals: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for item in items.iter().chain(&prelude.items) {
        let (_, body) = interp.compiled_fn_body(item).map_err(|e| e.to_string())?;
        let symbol = target_symbol_name(item);
        let read = globals_read(&heap, &body)?;
        // A direct copy is the same body.
        if let Some(copy) = direct_entry.as_ref().filter(|_| symbol == entry_symbol) {
            body_globals.insert(copy.clone(), read.clone());
        }
        body_globals.insert(symbol, read);
    }
    let is_static = |i: usize| i.checked_sub(prelude.global_inits.len()).is_some_and(|i| statics.contains_key(&i));
    let globals = Globals {
        inits: all_inits
            .iter()
            .zip(&eval_globals)
            .enumerate()
            .map(|(i, ((_, form), (_, id)))| {
                Ok(GlobalInit { id: *id, reads: globals_read(&heap, std::slice::from_ref(form))?, always: i >= prelude.global_inits.len() && !is_static(i), is_static: is_static(i) })
            })
            .collect::<Result<_, String>>()?,
        bodies: body_globals,
    };

    // The printer control variables' compiled ids, for the startup to
    // register: the prelude defines all of them, so each was promoted with
    // the rest of its globals above.
    let printer_globals: Vec<(&str, usize)> = typelisp_print::runtime::PRINTER_GLOBALS
        .iter()
        .map(|name| {
            let id = interp
                .compiled_global_id(&crate::types::printer_global(name))
                .ok_or_else(|| format!("internal error: printer variable {} was never promoted", name))?;
            Ok((*name, id))
        })
        .collect::<Result<_, String>>()?;

    // Only when the program actually needs an interpreter ([`EVAL_SHIMS`]):
    // naming one of those shims is what makes the linker pull the checker and
    // the interpreter in, the same rule the printer's registration block
    // follows.
    let calls_eval = {
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
    drop(word_width);
    let eval_env: Option<Vec<u8>> = if calls_eval {
        let mut env_heap = Heap::with_capacity(EVAL_HEAP_CAPACITY);
        Some(crate::compile::dump::capture_program_dump(&mut env_heap, &source, &src_root, &entry_segs, &eval_globals)?)
    } else {
        None
    };

    let _word_width = emitting();

    // What `main` answers with decides how the exit code is read off it: an
    // `int` is a tagged word (a fixnum's payload is the code; a bignum has
    // none), a fixed-width integer the raw word.
    let main_returns_int = chk.fn_ret(&entry_path).is_some_and(|ret| ret == crate::Type::Int);
    // The namespace an eval'd form is checked in: the entry file's module, as
    // `typl file.typl` places it (`run_file`), so a bare name in it resolves
    // to the program's own definition.
    let eval_ns = typelisp_front::mem::symbols::ns_of(&entry_segs);
    // The executable's type names are written from that same module, as
    // `typl file.typl` writes them: the view the printer's registration
    // carries (`emit_print_registration`) is the one the checker publishes
    // for it.
    chk.set_current_ns(eval_ns);

    // Under the whole-build guard above (no second take here: rebinding
    // `_guard` in this scope would drop the outer one first).
    let result = {
        let m = module.borrow();
        // One entry per C callback the program passes (`typeck::callback`),
        // each running a function compiled into this module; the generated
        // `main` registers them before anything runs.
        let mut callback_entries = Vec::new();
        for (i, sig) in chk.ffi_callbacks().into_iter().enumerate() {
            let entry = crate::compile::ffi::emit_callback_entry_in_module(&m, &format!("tl_ffi_callback_entry${}", i), &sig)?;
            callback_entries.push((sig, entry));
        }
        // The functions a board runs when an interrupt comes.
        let mut interrupt_handlers = Vec::new();
        for (vector, path) in chk.interrupt_handlers() {
            if options.target.board().is_none() {
                return Err(format!(
                    "`{}` is an interrupt handler, and only a board has interrupts: compile it with --target for one ({})",
                    path,
                    Target::board_names()
                ));
            }
            let symbol = target_symbol_name(&CompileTarget::Fn(path));
            let handler = m
                .get_function(&symbol)
                .ok_or_else(|| format!("internal error: the interrupt handler `{}` was not compiled into the module", path))?;
            interrupt_handlers.push((vector, handler));
        }
        build_main_wrapper(
            ctx,
            &m,
            &entry_symbol,
            direct_entry.as_deref(),
            &global_init_names,
            &globals,
            &interp.vtable_descriptors(),
            &interp.upcast_descriptors(),
            &interp.enum_variant_descriptors(),
            &interp.field_template_descriptors(),
            &interp.field_name_descriptors(),
            &print_objects,
            &printer_globals,
            &callback_entries,
            &interrupt_handlers,
            eval_env.as_deref().map(|bytes| (bytes, eval_ns, crate::release::is_release())),
            main_returns_int,
            options.target,
            HeapInit {
                cells: match options.heap_cells {
                    Some(cells) => cells,
                    None if options.no_cons => 0,
                    None if eval_env.is_some() => EVAL_HEAP_CAPACITY,
                    None if options.target.board().is_some() => BOARD_HEAP_CAPACITY,
                    None => typelisp_rt::DEFAULT_AOT_HEAP_CAPACITY,
                },
                fixed: options.heap_cells.is_some() || options.no_cons,
                max_memory: options.max_memory,
            },
        )
            .and_then(|()| m.verify().map_err(|e| format!("module failed verification: {}", e)))
            .and_then(|()| direct::check_only_called(&m, &direct_names))
            .and_then(|()| {
                // A direct function's loop offers the scheduler no turn, which
                // is right only in a program with no other task to run.
                let entry = m.get_function(ENTRY_POINT_NAME).expect("build_main_wrapper defined `main`");
                let waits = reach::uses(&m, entry, |name| crate::compile::externs::needs_scheduler(name).then(String::new));
                if skipped_safepoints > 0 && !waits.is_empty() {
                    return Err("internal error: a direct function has a loop, and the program runs other tasks".to_string());
                }
                Ok(())
            })
            .and_then(|()| {
                if !options.no_cons {
                    return Ok(());
                }
                let entry = m.get_function(ENTRY_POINT_NAME).expect("build_main_wrapper defined `main`");
                let uses = reach::uses(&m, entry, |name| {
                    crate::compile::externs::CONS_MAKING_SHIMS.iter().find(|(shim, _)| *shim == name).map(|(_, what)| what.to_string())
                });
                if uses.is_empty() { Ok(()) } else { Err(reach::report("--no-cons: the program uses cons cells:", &uses)) }
            })
            .and_then(|()| {
                let shim = |table: &'static [(&'static str, &'static str)]| {
                    move |name: &str| table.iter().find(|(shim, _)| *shim == name).map(|(_, what)| what.to_string())
                };
                if options.target.board().is_none() {
                    let entry = m.get_function(ENTRY_POINT_NAME).expect("build_main_wrapper defined `main`");
                    let uses = reach::uses(&m, entry, shim(crate::compile::externs::BOARD_ONLY_SHIMS));
                    if !uses.is_empty() {
                        return Err(reach::report("the program uses what only a board has:", &uses));
                    }
                }
                for (_, handler) in &interrupt_handlers {
                    let uses = reach::uses(&m, *handler, shim(crate::compile::externs::ALWAYS_WAITING_SHIMS));
                    if !uses.is_empty() {
                        let header = format!(
                            "the interrupt handler `{}` waits, and a handler cannot — it runs on top of the program it stopped:",
                            handler.get_name().to_string_lossy()
                        );
                        return Err(reach::report(&header, &uses));
                    }
                }
                Ok(())
            })
            .and_then(|()| {
                write_ir(&m)
            })
            .and_then(|()| match options.target {
                Target::Host => write_executable(&m, output_path, &link_libraries, &items),
                Target::AvrAtmega328p => Err("internal error: a target without a runtime is built without a driver".to_string()),
                Target::Wasip1 | Target::Wasm32Unknown | Target::Thumbv7em | Target::Thumbv8mMain => {
                    let missing = options.target.missing_runtimes();
                    // A board calls its vendor's C functions; a WebAssembly
                    // module has no C library to find them in.
                    let c_functions = options.target.board().is_none();
                    let entry = m.get_function(ENTRY_POINT_NAME).expect("build_main_wrapper defined `main`");
                    let uses = reach::uses(&m, entry, |name| {
                        if let Some(what) = crate::compile::externs::unavailable_in(missing, name) {
                            return Some(what.to_string());
                        }
                        (c_functions && ffi_decls.iter().any(|d| d.c_symbol == name)).then(|| format!("the C function `{}`", name))
                    });
                    if !uses.is_empty() {
                        let header = format!("--target {}: the program uses what the target does not have:", options.target.name());
                        return Err(reach::report(&header, &uses));
                    }
                    if options.target.board().is_some() {
                        embedded::write_executable(&m, output_path, &items, options)
                    } else {
                        wasm::write_module(&m, output_path, &items, options.target)
                    }
                }
            })
    };
    // Destroyed here with the guard still held rather than left to fall out
    // of scope: `~Module` unregisters every value name from the shared LLVM
    // Context (see `compile::COMPILE_LOCK`). Written as a `result` binding
    // rather than `?`s so the failing paths take this route too.
    drop(module);
    result
}


/// The type of the values `target` prints when it is a `print-object`: the
/// type it is written for, at the type arguments it was specialized at.
fn print_object_receiver(target: &CompileTarget) -> Option<typelisp_mem::TypeId> {
    use typelisp_mem::specialization::{specialized, Generic};
    let CompileTarget::Method(type_path, method) = target else { return None };
    let print_object = typelisp_mem::symbols::member_symbol("print-object");
    let args = match specialized(*method) {
        Some((Generic::Method(_, _, base), args)) if base == print_object => args,
        None if *method == print_object => Vec::new(),
        _ => return None,
    };
    Some(typelisp_front::type_key::type_key_id(type_path, args))
}

/// Adds a real, C-ABI `main` to `module` that calls the compiled entry
/// point (the symbol `entry_symbol`, `tl_p::main` for `p.typl` — called
/// `tl_main` below) with no logical
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
/// `Interp::vtable_descriptors`) are filled in after the heap and
/// `rt_eval_init` (whose `Interp::new` clears the table): each slot is an
/// `rt_vtable_set` call whose function-pointer argument is a `ptrtoint` *constant* the linker resolves, so unlike the
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
/// the startup to read back into the heap before `main` proper begins, the module an eval'd form is checked in, and whether this
/// is a release build.
fn build_main_wrapper(
    ctx: &'static Context,
    module: &Module<'static>,
    entry_symbol: &str,
    direct_entry: Option<&str>,
    global_init_names: &[String],
    globals: &Globals,
    vtables: &[(u32, Vec<(Path, typelisp_mem::SymRef)>)],
    upcasts: &[(u32, u32, u32)],
    enum_variants: &[(Path, usize, String)],
    field_templates: &[(Path, i64, i64, crate::TypeId)],
    field_names: &[(Path, i64, String)],
    print_objects: &[(typelisp_mem::TypeId, String)],
    printer_globals: &[(&str, usize)],
    callback_entries: &[(typelisp_front::ffi_callback::CallbackSig, inkwell::values::FunctionValue<'static>)],
    interrupt_handlers: &[(u16, inkwell::values::FunctionValue<'static>)],
    eval_env: Option<(&[u8], typelisp_mem::NsId, bool)>,
    main_returns_int: bool,
    target: Target,
    heap: HeapInit,
) -> Result<(), String> {
    let tl_main = module
        .get_function(entry_symbol)
        .ok_or_else(|| "internal error: compiled entry point not found in module".to_string())?;

    let i32_type = ctx.i32_type();
    let main_fn = module.add_function(ENTRY_POINT_NAME, i32_type.fn_type(&[], false), None);
    let entry_block = ctx.append_basic_block(main_fn, "entry");
    let builder = ctx.create_builder();
    builder.position_at_end(entry_block);

    // The archive's ABI version, checked by the linker: only an archive of
    // the version this compiler emits for defines `typelisp_abi::ABI_SYMBOL`
    // (`docs/dev/api_version/`). Volatile, so the reference survives
    // optimisation; the byte read is never used.
    let abi_marker = module.add_global(ctx.i8_type(), None, typelisp_abi::ABI_SYMBOL);
    let abi_read = builder
        .build_load(ctx.i8_type(), abi_marker.as_pointer_value(), "abi")
        .map_err(|e| format!("failed to build the ABI marker load: {}", e))?;
    inkwell::values::BasicValue::as_instruction_value(&abi_read)
        .expect("a load is an instruction")
        .set_volatile(true)
        .map_err(|e| format!("failed to mark the ABI marker load volatile: {:?}", e))?;

    // A program nothing of which calls the runtime: no heap, and nothing to
    // start. What it runs is all there is.
    let outside = !callback_entries.is_empty() || !interrupt_handlers.is_empty() || eval_env.is_some();
    let direct_entry = match direct_entry.filter(|_| !outside) {
        Some(name) => Some(module.get_function(name).ok_or_else(|| format!("internal error: `{}` was not compiled", name))?),
        None => None,
    };
    if let Some(entry) = direct_entry {
        if globals.inits.iter().all(|i| !i.always) && !reach::reached(module, &[entry], None).functions.iter().any(|f| is_runtime(module, f)) {
            run_bare_entry(ctx, &builder, entry, main_returns_int)?;
            return crate::compile::type_table::settle_for_export(module).map(|_| ());
        }
    }

    // A board's memory and interrupt controller, readied for the runtime
    // before anything allocates.
    if target.board().is_some() {
        let start = module.add_function("typelisp_board_start", ctx.void_type().fn_type(&[], false), None);
        builder.build_call(start, &[], "").map_err(|e| format!("failed to build the board's start: {}", e))?;
    }

    // The C callback entries (`typelisp_rt::ffi_callback`): the entry's
    // address under the function's definition and the function type C calls
    // it at, which is what a callback argument looks up.
    for (i, (sig, entry)) in callback_entries.iter().enumerate() {
        let word_ty = crate::compile::carriers::word();
        let definition = typelisp_mem::type_words::definition_words(sig.function.ns(), sig.function.name_symbol());
        let (definition, len) =
            crate::compile::type_table::constant_words(module, &format!("ffi_callback_function${}", i), &definition);
        let words = [
            entry.as_global_value().as_pointer_value().const_to_int(word_ty),
            crate::compile::type_table::load(&builder, module, sig.ty)?,
            definition.as_pointer_value().const_to_int(word_ty),
            word_ty.const_int(len as u64, false),
        ];
        call_shim(module, &builder, "rt_ffi_callback_register", &words)?;
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
    //
    // In a function of its own, which `main` calls here and which is given
    // its body once `main` is complete: which `print-object`s it registers
    // depends on what the rest of the executable can make
    // ([`live_types`]).
    let print_register = if module_calls_any(module, &PRINT_SHIMS) {
        let register = module.add_function("typelisp_print_register", ctx.void_type().fn_type(&[], false), None);
        builder.build_call(register, &[], "").map_err(|e| format!("failed to build the printer registration call: {}", e))?;
        Some(register)
    } else {
        None
    };
    // The environment `eval` needs, as one immutable blob. Registration only —
    // the shim just remembers the pointer — so this belongs with the other
    // pre-heap stores. Emitted, like the printer's block above, only when the
    // module actually calls `rt_eval`: naming the shim is what pulls the
    // checker and the interpreter into the executable.
    if let Some((bytes, ns, release)) = eval_env {
        let word_ty = crate::compile::carriers::word();
        let blob = module.add_global(ctx.i8_type().array_type(bytes.len() as u32), None, "typelisp_eval_env");
        blob.set_initializer(&ctx.const_string(bytes, false));
        blob.set_constant(true);
        let (ns_words, ns_len) =
            crate::compile::type_table::constant_words(module, "typelisp_eval_ns", &typelisp_mem::type_words::ns_words(ns));
        call_shim(
            module,
            &builder,
            "rt_eval_state",
            &[
                blob.as_pointer_value().const_to_int(word_ty),
                word_ty.const_int(bytes.len() as u64, false),
                ns_words.as_pointer_value().const_to_int(word_ty),
                word_ty.const_int(ns_len as u64, false),
                word_ty.const_int(release as u64, false),
            ],
        )?;
    }
    // AOT's counterpart to the JIT path's `Interp::eval` calling
    // `runtime::set_active_heap` before every compiled call — see
    // `runtime::rt_heap_init`'s doc comment for why a standalone executable
    // has to create and register its own `Heap` here instead. Must run
    // before `tl_main` (or anything it calls) touches the heap at all.
    //
    // The bound on memory is set by the same call, before the arena is
    // allocated, so the arena counts toward it (0 is no bound).
    call_shim(
        module,
        &builder,
        "rt_heap_init",
        &[
            crate::compile::carriers::word().const_int(heap.cells as u64, false),
            crate::compile::carriers::word().const_int(heap.max_memory.unwrap_or(0) as u64, false),
            crate::compile::carriers::word().const_int(heap.fixed as u64, false),
        ],
    )?;
    // Between the heap and the vtables and global inits, and that position
    // is forced: `rt_eval_init` builds an `Interp`, and `Interp::new` resets
    // the runtime's global and vtable tables — after them it would wipe the
    // slots the machine code addresses. See `typelisp_front::shim`'s module
    // doc comment.
    if eval_env.is_some() {
        call_shim(module, &builder, "rt_eval_init", &[])?;
    }
    // A board's interrupt handlers (`typelisp_rt::interrupt`), each under its
    // vector-table entry: after the heap, which a handler runs on.
    for (vector, handler) in interrupt_handlers {
        let word_ty = crate::compile::carriers::word();
        let words = [word_ty.const_int(u64::from(*vector), false), handler.as_global_value().as_pointer_value().const_to_int(word_ty)];
        call_shim(module, &builder, "rt_interrupt_register", &words)?;
    }
    // Trait-object vtables: after `rt_eval_init` (above), whose `Interp::new`
    // clears the table, and complete before `tl_main` can reach a `:dyn`
    // call site.
    if !vtables.is_empty() {
        let word_ty = crate::compile::carriers::word();
        for (id, slots) in vtables {
            for (slot, (type_name, method)) in slots.iter().enumerate() {
                let symbol = ConcreteNames::method_symbol(type_name, method.name());
                let target = module.get_function(&symbol).ok_or_else(|| {
                    format!(
                        "compile-file: dyn dispatch target `{}::{}` was not compiled into this file",
                        type_name,
                        method.name()
                    )
                })?;
                let fn_ptr = target.as_global_value().as_pointer_value().const_to_int(word_ty);
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
                    word_ty.const_int(*id as u64, false),
                    word_ty.const_int(slot as u64, false),
                    fn_ptr,
                    word_ty.const_int(u64::from(body_abi), false),
                ];
                call_shim(module, &builder, "rt_vtable_set", &set_args)?;
            }
        }
    }
    // The supertrait upcast table, for the same reason and at the same
    // point: a `:dyn` upcast in `tl_main` reads it, and it is all constants.
    if !upcasts.is_empty() {
        let word_ty = crate::compile::carriers::word();
        for (from, trait_id, to) in upcasts {
            let set_args = [
                word_ty.const_int(*from as u64, false),
                word_ty.const_int(*trait_id as u64, false),
                word_ty.const_int(*to as u64, false),
            ];
            call_shim(module, &builder, "rt_upcast_set", &set_args)?;
        }
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
    let word_ty = crate::compile::carriers::word();
    let mut init_addrs: Vec<inkwell::values::IntValue<'static>> = Vec::with_capacity(global_init_names.len());
    for (name, init) in global_init_names.iter().zip(&globals.inits) {
        if init.is_static {
            init_addrs.push(word_ty.const_zero());
            continue;
        }
        let f = module
            .get_function(name)
            .ok_or_else(|| format!("internal error: global-init function \"{}\" not found in module", name))?;
        if f.get_type() != coroutine_fn_ty {
            return Err(format!(
                "internal error: global initialiser \"{}\" is not a coroutine-ABI body, and only those run under the scheduler",
                name
            ));
        }
        init_addrs.push(f.as_global_value().as_pointer_value().const_to_int(word_ty));
    }
    let entry_addr = tl_main.as_global_value().as_pointer_value().const_to_int(word_ty);
    // The entry shim, and the one without a scheduler that takes its place
    // when the program turns out to need none (below).
    let mut entry: Option<(inkwell::values::FunctionValue<'static>, &str)> = None;
    let result = if tl_main.get_type() == coroutine_fn_ty {
        // A native executable's tasks run on OS threads too; a WebAssembly
        // module has none.
        let threads = matches!(target, Target::Host);
        let entry_shim = match (eval_env.is_some(), main_returns_int, threads) {
            (true, true, _) => "rt_run_program_interp_int",
            (true, false, _) => "rt_run_program_interp",
            (false, true, true) => "rt_run_program_threads_int",
            (false, false, true) => "rt_run_program_threads",
            (false, true, false) => "rt_run_program_int",
            (false, false, false) => "rt_run_program",
        };
        let rt_run_program = declare_entry(module, entry_shim)?;
        if eval_env.is_none() {
            entry = Some((rt_run_program, if main_returns_int { "rt_run_program_alone_int" } else { "rt_run_program_alone" }));
        }
        let inits = module.add_global(word_ty.array_type(init_addrs.len() as u32), None, "tl_global_inits");
        inits.set_initializer(&word_ty.const_array(&init_addrs));
        inits.set_constant(true);
        let inits_addr = inits.as_pointer_value().const_to_int(word_ty);
        let words = [inits_addr.into(), word_ty.const_int(init_addrs.len() as u64, false).into(), entry_addr.into()];
        let sig = crate::compile::externs::runtime_sig(entry_shim).expect("an entry shim is in the entry table");
        crate::compile::runtime_abi::call_declared(&builder, rt_run_program, sig, &words)?.1.into_int_value()
    } else {
        if main_returns_int {
            return Err("an `int`-returning main under the classic ABI is not supported".to_string());
        }
        if !init_addrs.is_empty() {
            return Err("internal error: a classic-ABI entry point cannot run global initialisers (no scheduler)".to_string());
        }
        crate::compile::runtime_abi::call_word(&builder, module, "rt_run_entry", &[entry_addr.into()])?.1
    };
    let exit_code = builder
        .build_int_truncate(result, i32_type, "exit_code")
        .map_err(|e| format!("failed to truncate exit code: {}", e))?;
    builder.build_return(Some(&exit_code)).map_err(|e| format!("failed to build entry-point return: {}", e))?;

    // Whether the executable writes through the printer, which reads the
    // printer control variables.
    let mut writes = false;
    if let Some(register) = print_register {
        // What the printer needs to write a value, only when the executable
        // can write one: asked of the finished `main`, whose registration
        // has no body yet and is not walked. Without it, the registered
        // `print-object` methods would bring the printer in after all.
        let reaches = |shim: fn(&str) -> bool| {
            eval_env.is_some() || !reach::uses(module, main_fn, |name| shim(name).then(String::new)).is_empty()
        };
        let live = if reaches(|name| PRINT_SHIMS.contains(&name)) {
            Some(live_types(module, main_fn, print_objects, eval_env.is_some())?)
        } else {
            None
        };
        writes = reaches(writes_through_the_printer);
        emit_print_registration(
            ctx, module, register, enum_variants, field_templates, field_names, print_objects, live.as_ref(), writes, printer_globals,
        )?;
    }


    // The prelude's globals nothing the executable can run reads are left
    // out: their initialisers, and what those make, are not linked in. The
    // program's own `defvar`s all run, as `typl` runs them. Not for an
    // executable that calls `eval`, whose forms can read any global.
    if eval_env.is_none() {
        leave_out_unread_globals(module, main_fn, globals, if writes { printer_globals } else { &[] })?;
    }

    // A program that nothing it can run makes wait — no task, thread,
    // channel, `sleep` or socket — runs without a scheduler, and the
    // scheduler is not linked into it. Asked of the finished `main`, which
    // refers to everything that can run: the initialisers, `tl_main`, the
    // registered `print-object` methods, the interrupt handlers and C
    // callbacks.
    if let Some((scheduled, alone)) = entry {
        let waits = reach::uses(module, main_fn, |name| crate::compile::externs::needs_scheduler(name).then(String::new));
        if waits.is_empty() {
            let alone = declare_entry(module, alone)?;
            scheduled.as_global_value().as_pointer_value().replace_all_uses_with(alone.as_global_value().as_pointer_value());
            // SAFETY: a declaration with no uses left.
            unsafe { scheduled.delete() };
            // With no global to initialise either, an entry that is direct
            // runs on this frame, and what drives a coroutine is not linked
            // in.
            let nothing_to_initialise = module
                .get_global("tl_global_inits")
                .and_then(|g| g.get_initializer())
                // SAFETY: a live constant of `module`.
                .is_some_and(|i| unsafe { inkwell::llvm_sys::core::LLVMIsNull(inkwell::values::AsValueRef::as_value_ref(&i)) } != 0);
            if let Some(direct) = direct_entry.filter(|_| nothing_to_initialise) {
                run_direct_entry(module, &builder, main_fn, alone, direct, main_returns_int)?;
            }
        }
    }

    // The types the executable's code reads, the prelude's included: interned
    // first of all, since everything after reads them out of the tables. A
    // table nothing the executable can run reads from is not interned, and
    // with it go the shapes and what interns them. Asked last, of the
    // finished `main`, whose initialiser array now names only the
    // initialisers that run.
    crate::compile::type_table::settle_for_export(module)?;
    let read = reach::reached(module, &[main_fn], None).globals;
    let first = entry_block.get_first_instruction().expect("`main` was just given a body");
    // After the board's start, which is what they allocate from.
    let first = match entry_block.get_instructions().find(|i| calls(i, "typelisp_board_start")) {
        Some(start) => start.get_next_instruction().expect("`main` goes on after the board's start"),
        None => first,
    };
    builder.position_before(&first);
    for init in crate::compile::type_table::init_functions(module) {
        let f = module.get_function(&init).expect("an init function the module defines");
        let table = crate::compile::type_table::table_of(f)?;
        if read.contains(&inkwell::values::AsValueRef::as_value_ref(&table)) {
            builder.build_call(f, &[], "").map_err(|e| format!("failed to build the call to `{}`: {}", init, e))?;
        }
    }
    Ok(())
}

/// Whether the function `name` is the runtime's: one the module only
/// declares, that is not LLVM's own and not a C function the program calls.
fn is_runtime(module: &Module<'static>, name: &str) -> bool {
    name.starts_with("rt_") && module.get_function(name).is_some_and(|f| f.count_basic_blocks() == 0)
}

/// Whether `i` is a call of the function `name`.
fn calls(i: &inkwell::values::InstructionValue<'static>, name: &str) -> bool {
    i.get_opcode() == InstructionOpcode::Call
        && matches!(i.get_operand(i.get_num_operands() - 1), Some(Operand::Value(v)) if v.is_pointer_value() && v.into_pointer_value().get_name().to_bytes() == name.as_bytes())
}

/// Puts, in `main`, a call of the direct entry point and of what reads its
/// answer in place of the call that would drive the program (`driven`).
fn run_direct_entry(
    module: &Module<'static>,
    builder: &inkwell::builder::Builder<'static>,
    main_fn: inkwell::values::FunctionValue<'static>,
    driven: inkwell::values::FunctionValue<'static>,
    entry: inkwell::values::FunctionValue<'static>,
    main_returns_int: bool,
) -> Result<(), String> {
    let name = driven.get_name().to_string_lossy().into_owned();
    let call = main_fn
        .get_basic_blocks()
        .into_iter()
        .flat_map(|b| b.get_instructions().collect::<Vec<_>>())
        .find(|i| calls(i, &name))
        .ok_or_else(|| format!("internal error: `main` does not call `{}`", name))?;
    builder.position_before(&call);
    let word_ty = crate::compile::carriers::word();
    let answer = builder.build_call(entry, &[], "answer").map_err(|e| format!("failed to call the entry point: {}", e))?;
    let code = match answer.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(word) if main_returns_int => call_entry(builder, module, "rt_main_answered", &[word.into_int_value()])?,
        _ => call_entry(builder, module, "rt_main_returned", &[])?,
    };
    let call: inkwell::values::IntValue<'static> = call.try_into().map_err(|()| "internal error: the program's answer is not a word".to_string())?;
    debug_assert_eq!(call.get_type(), word_ty);
    call.replace_all_uses_with(code);
    call.as_instruction().expect("a call").erase_from_basic_block();
    Ok(())
}

/// The whole program's IR, for reading, whatever the target.
/// An AOT module is the one the island's output is hardest to
/// see: it is neither `disassemble`'s single function nor a
/// committed dump, and by the time anything is wrong it is
/// machine code. `TYPELISP_AOT_IR=<path>` writes it here.
fn write_ir(module: &Module<'static>) -> Result<(), String> {
    if let Ok(path) = std::env::var("TYPELISP_AOT_IR") {
        fs::write(&path, module.print_to_string().to_string()).map_err(|e| format!("failed to write {}: {}", path, e))?;
    }
    Ok(())
}

/// Whether the global `body` (`core_bridge::global_init`) makes can be static
/// (`llvm_builtins::globals`), `assigned` being whether it can be assigned.
///
/// Without a runtime there is no heap, and any can. With one, what is kept
/// must be nothing the collector has to see — a number, a character or a
/// `bool`, or a fixnum that stays — and its initial value a literal, so that
/// nothing has to run to make it.
fn static_global(heap: &Heap, body: Value, assigned: bool, target: Target) -> bool {
    use crate::compile::llvm_builtins::globals::kept_in;
    use typelisp_front::sexpr_variant::field_kind as k;
    let Ok(parts) = core::fields(heap, body) else { return false };
    let [_, Value::Int(kind), value] = parts[..] else { return false };
    if kept_in(kind).is_none() {
        return false;
    }
    if !target.has_runtime() {
        return true;
    }
    let scalar = usize::try_from(kind).is_ok_and(|kind| kind >= k::FIXED || [k::F64, k::F32, k::CHAR, k::BOOL].contains(&kind));
    let literal = core::op_wk(heap, value).unwrap_or(typelisp_mem::NOT_WELL_KNOWN);
    match literal {
        // An `int` that is not a fixnum is no literal (`core_bridge`).
        wk::INT => !assigned,
        wk::INT_ANY_WIDTH | wk::FLOAT_ANY_WIDTH | wk::CHAR | wk::BOOL => scalar,
        _ => false,
    }
}

/// `main` of a program nothing of which calls the runtime: a call of its
/// static globals' initializers, and of its direct entry point. Interrupts may
/// come, if `interrupts` says so, from when the globals are initialized.
fn bare_main(
    ctx: &'static Context,
    module: &Module<'static>,
    inits: &[inkwell::values::FunctionValue<'static>],
    interrupts: bool,
    entry: inkwell::values::FunctionValue<'static>,
    main_returns_int: bool,
) -> Result<(), String> {
    let main_fn = module.add_function(ENTRY_POINT_NAME, ctx.i32_type().fn_type(&[], false), None);
    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(main_fn, "entry"));
    for init in inits {
        builder.build_call(*init, &[], "").map_err(|e| format!("failed to call a global's initializer: {}", e))?;
    }
    if interrupts {
        avr::enable_interrupts(&builder)?;
    }
    run_bare_entry(ctx, &builder, entry, main_returns_int)
}

/// Calls the direct entry point and returns the exit code its answer is.
fn run_bare_entry(
    ctx: &'static Context,
    builder: &inkwell::builder::Builder<'static>,
    entry: inkwell::values::FunctionValue<'static>,
    main_returns_int: bool,
) -> Result<(), String> {
    let i32_type = ctx.i32_type();
    let call = builder.build_call(entry, &[], "answer").map_err(|e| format!("failed to call the entry point: {}", e))?;
    let code = match call.try_as_basic_value() {
        // With no runtime there is no bignum, so an `int` is a fixnum.
        inkwell::values::ValueKind::Basic(word) if main_returns_int => {
            let n = builder
                .build_right_shift(word.into_int_value(), word.get_type().into_int_type().const_int(1, false), true, "code")
                .map_err(|e| format!("failed to read the exit code: {}", e))?;
            builder.build_int_cast_sign_flag(n, i32_type, true, "exit_code").map_err(|e| format!("failed to read the exit code: {}", e))?
        }
        _ => i32_type.const_zero(),
    };
    builder.build_return(Some(&code)).map_err(|e| format!("failed to build entry-point return: {}", e))?;
    Ok(())
}

/// Calls the entry shim `name`, whose answer is a word.
fn call_entry(
    builder: &inkwell::builder::Builder<'static>,
    module: &Module<'static>,
    name: &str,
    words: &[inkwell::values::IntValue<'static>],
) -> Result<inkwell::values::IntValue<'static>, String> {
    let f = declare_entry(module, name)?;
    let sig = crate::compile::externs::runtime_sig(name).expect("an entry shim is in the entry table");
    let words: Vec<_> = words.iter().map(|w| (*w).into()).collect();
    Ok(crate::compile::runtime_abi::call_declared(builder, f, sig, &words)?.1.into_int_value())
}

/// What each global's initialiser and each compiled body read, by global
/// id: what decides which globals an executable initialises
/// ([`leave_out_unread_globals`]).
#[derive(Default)]
struct Globals {
    /// One per initialiser, in the order `main` runs them.
    inits: Vec<GlobalInit>,
    /// Each body's symbol, with the globals it reads or assigns.
    bodies: std::collections::HashMap<String, Vec<usize>>,
}

struct GlobalInit {
    /// The global it initialises.
    id: usize,
    /// The globals its form reads.
    reads: Vec<usize>,
    /// A `defvar` of the program's own, which runs whether or not anything
    /// reads it.
    always: bool,
    /// A static global: it has no initialiser to run.
    is_static: bool,
}

/// Replaces, in `main`'s initialiser array (`tl_global_inits`), the
/// initialiser of every global nothing the executable can run reads with 0,
/// which the runtime takes as "no slot" (`typelisp_rt::global_skip`).
///
/// A global is read when a body reached from `main` — not through the
/// array — reads it, when the initialiser of a global that runs reads it,
/// or when it is one of `printer_globals`; a global runs when it is read or
/// is the program's own. The bodies those initialisers reach count too, so
/// the walk goes on until it finds nothing new.
fn leave_out_unread_globals(
    module: &Module<'static>,
    main_fn: inkwell::values::FunctionValue<'static>,
    globals: &Globals,
    printer_globals: &[(&str, usize)],
) -> Result<(), String> {
    let Some(array) = module.get_global("tl_global_inits") else { return Ok(()) };
    let init_fns: Vec<Option<inkwell::values::FunctionValue<'static>>> = (0..globals.inits.len())
        .map(|i| {
            if globals.inits[i].is_static {
                return Ok(None);
            }
            module
                .get_function(&format!("$global_init${}", i))
                .map(Some)
                .ok_or_else(|| format!("internal error: global initialiser {} is not in the module", i))
        })
        .collect::<Result<_, String>>()?;
    let mut read: std::collections::HashSet<usize> = printer_globals.iter().map(|(_, id)| *id).collect();
    loop {
        let runs = |init: &GlobalInit| init.always || read.contains(&init.id);
        let mut roots = vec![main_fn];
        let mut now = read.clone();
        for (init, f) in globals.inits.iter().zip(&init_fns) {
            if let Some(f) = f.filter(|_| runs(init)) {
                roots.push(f);
                now.extend(&init.reads);
            }
        }
        for name in reach::reached(module, &roots, Some(inkwell::values::AsValueRef::as_value_ref(&array))).functions {
            if let Some(ids) = globals.bodies.get(&name) {
                now.extend(ids);
            }
        }
        if now == read {
            break;
        }
        read = now;
    }
    let word_ty = crate::compile::carriers::word();
    let words: Vec<inkwell::values::IntValue<'static>> = globals
        .inits
        .iter()
        .zip(&init_fns)
        .map(|(init, f)| {
            match f.filter(|_| init.always || read.contains(&init.id)) {
                Some(f) => f.as_global_value().as_pointer_value().const_to_int(word_ty),
                None => word_ty.const_zero(),
            }
        })
        .collect();
    array.set_initializer(&word_ty.const_array(&words));
    Ok(())
}

/// What the executable can make a value of, which decides what the
/// printer's registration holds ([`emit_print_registration`]).
struct Live {
    /// A program that calls `eval` can make a value of any type.
    every: bool,
    /// Whether each of `print_objects` is registered.
    print_objects: Vec<bool>,
    /// The definitions of the types the executable's code reads.
    definitions: std::collections::HashSet<(typelisp_mem::NsId, typelisp_mem::SymRef)>,
}

impl Live {
    /// Whether a value of a type the definition at `path` makes can exist,
    /// so that the printer needs what the registration says about it: its
    /// variants' names, its fields' types and names.
    fn has_definition(&self, path: &Path) -> bool {
        if self.every {
            return true;
        }
        let definition = typelisp_front::type_key::type_key_id(path, Vec::new());
        if !made::tracked(definition) && typelisp_mem::runtime_builds(definition) {
            return true;
        }
        match typelisp_mem::type_shape(definition) {
            typelisp_mem::TypeShape::Named(ns, sym, _) => self.definitions.contains(&(ns, sym)),
            _ => false,
        }
    }
}

/// What the executable can make a value of. A value is made by compiled
/// code, which reads its type out of a type table to hand it to the
/// runtime, or by the runtime itself ([`typelisp_mem::runtime_builds`]). So
/// a type is live when the code the executable can run reads it, or one it
/// is an argument of, or when the runtime builds it: for the few types the
/// runtime makes only in some builtins ([`made`]), when the executable
/// reaches one of those. What can run is what
/// `main` reaches, and what each live type's `print-object` does in turn;
/// the printer's own registration (still without a body here) is not
/// walked, since naming a type there makes no value of it.
fn live_types(
    module: &Module<'static>,
    main_fn: inkwell::values::FunctionValue<'static>,
    print_objects: &[(typelisp_mem::TypeId, String)],
    calls_eval: bool,
) -> Result<Live, String> {
    if calls_eval {
        return Ok(Live { every: true, print_objects: vec![true; print_objects.len()], definitions: Default::default() });
    }
    let mut reach = reach::TypeReach::new(module)?;
    reach.walk(module, main_fn)?;
    let mut live = vec![false; print_objects.len()];
    loop {
        let read = with_arguments(&reach.types);
        let runtime_makes = |ty: typelisp_mem::TypeId| -> Result<bool, String> {
            if made::tracked(ty) {
                made::made(ty, |shim| reach.reaches(module, shim))
            } else {
                Ok(typelisp_mem::runtime_builds(ty))
            }
        };
        let mut newly = Vec::new();
        for (i, (ty, _)) in print_objects.iter().enumerate() {
            if !live[i] && (read.contains(ty) || runtime_makes(*ty)?) {
                newly.push(i);
            }
        }
        let grew = !newly.is_empty();
        for i in newly {
            let symbol = &print_objects[i].1;
            live[i] = true;
            let method = module.get_function(symbol).ok_or_else(|| {
                format!("compile-file: `print-object` implementation `{}` was not compiled into this file", symbol)
            })?;
            reach.walk(module, method)?;
        }
        if !grew {
            // The definitions the code reads, and the tracked ones a reached
            // builtin makes.
            let mut definitions = std::collections::HashSet::new();
            for (module_path, name) in made::TRACKED {
                let ty = typelisp_mem::intern_spec(&typelisp_mem::TypeSpec::Named(*module_path, name, &[]));
                if made::made(ty, |shim| reach.reaches(module, shim))? {
                    definitions.insert(ty);
                }
            }
            let definitions = read
                .into_iter()
                .chain(definitions)
                .filter_map(|ty| match typelisp_mem::type_shape(ty) {
                    typelisp_mem::TypeShape::Named(ns, sym, _) => Some((ns, sym)),
                    _ => None,
                })
                .collect();
            return Ok(Live { every: false, print_objects: live, definitions });
        }
    }
}

/// `types` and every type each is made of: a value of one can hold values
/// of those.
fn with_arguments(types: &std::collections::HashSet<typelisp_mem::TypeId>) -> std::collections::HashSet<typelisp_mem::TypeId> {
    use typelisp_mem::TypeShape;
    let mut all = std::collections::HashSet::new();
    let mut pending: Vec<typelisp_mem::TypeId> = types.iter().copied().collect();
    while let Some(ty) = pending.pop() {
        if !all.insert(ty) {
            continue;
        }
        match typelisp_mem::type_shape(ty) {
            TypeShape::Named(_, _, args) | TypeShape::Dyn(_, _, args) => pending.extend(args),
            TypeShape::Fn(params, rest, ret) => {
                pending.extend(params);
                pending.extend(rest);
                pending.push(ret);
            }
            TypeShape::Ptr(pointee) => pending.push(pointee),
            TypeShape::CStruct(..) | TypeShape::Unit | TypeShape::Never | TypeShape::Param(_) => {}
        }
    }
    all
}

/// The body of `register`, which the generated `main` calls before the heap
/// exists: the printer's program facts (`typelisp_print::aot`), an enum's
/// variant names, the fields' types and names, each type's `print-object`
/// and the printer control variables, none of which a standalone executable
/// can look up the way the interpreter does. Only for what `live` says a
/// value of can exist ([`live_types`]): the rest is never printed. Without
/// `live` the executable writes no value; unless it `writes`, it does not
/// write through the printer at all and the printer control variables are
/// left out too. How type names are written is always registered: a
/// runtime error message uses it as well.
fn emit_print_registration(
    ctx: &'static Context,
    module: &Module<'static>,
    register: inkwell::values::FunctionValue<'static>,
    enum_variants: &[(Path, usize, String)],
    field_templates: &[(Path, i64, i64, crate::TypeId)],
    field_names: &[(Path, i64, String)],
    print_objects: &[(typelisp_mem::TypeId, String)],
    live: Option<&Live>,
    writes: bool,
    printer_globals: &[(&str, usize)],
) -> Result<(), String> {
    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(register, "entry"));
    let word_ty = crate::compile::carriers::word();
    // A `(pointer, length)` pair per string, from a module-level constant
    // — no heap involved, so this can run before `rt_heap_init`.
    let literal = |builder: &inkwell::builder::Builder<'static>, text: &str| -> Result<(inkwell::values::IntValue<'static>, inkwell::values::IntValue<'static>), String> {
        let g = builder
            .build_global_string_ptr(text, "print_reg_str")
            .map_err(|e| format!("failed to build a printer-registration string: {}", e))?;
        Ok((
            g.as_pointer_value().const_to_int(word_ty),
            word_ty.const_int(text.len() as u64, false),
        ))
    };
    let call = |builder: &inkwell::builder::Builder<'static>, name: &str, words: &[inkwell::values::IntValue<'static>]| -> Result<(), String> {
        call_shim(module, builder, name, words)
    };
    // The definitions and the field types, read out of the module's type
    // table (`compile::type_table`): a type's id is this process's number,
    // so the executable carries the shapes and its startup interns them.
    let ty = |ty: crate::TypeId| crate::compile::type_table::load(&builder, module, ty);
    let definition = |path: &Path| ty(typelisp_front::type_key::type_key_id(path, Vec::new()));
    if let Some(live) = live {
        for (path, variant, name) in enum_variants.iter().filter(|(path, ..)| live.has_definition(path)) {
            let (name_ptr, name_len) = literal(&builder, name)?;
            call(&builder, "rt_print_enum_variant", &[definition(path)?, word_ty.const_int(*variant as u64, false), name_ptr, name_len])?;
        }
        for (path, variant, index, template) in field_templates.iter().filter(|(path, ..)| live.has_definition(path)) {
            call(
                &builder,
                "rt_print_field_template",
                &[definition(path)?, word_ty.const_int(*variant as u64, true), word_ty.const_int(*index as u64, true), ty(*template)?],
            )?;
        }
        for (path, index, name) in field_names.iter().filter(|(path, ..)| live.has_definition(path)) {
            let (name_ptr, name_len) = literal(&builder, name)?;
            call(&builder, "rt_print_field_name", &[definition(path)?, word_ty.const_int(*index as u64, true), name_ptr, name_len])?;
        }
        for ((receiver, symbol), live) in print_objects.iter().zip(&live.print_objects) {
            // A type no value of can exist is registered with no method, so
            // that printing one after all is a loud error rather than the
            // default rendering.
            let fn_ptr = if *live {
                let target = module.get_function(symbol).ok_or_else(|| {
                    format!("compile-file: `print-object` implementation `{}` was not compiled into this file", symbol)
                })?;
                classic_door(ctx, module, target)?
            } else {
                word_ty.const_zero()
            };
            call(&builder, "rt_print_object_method", &[ty(*receiver)?, fn_ptr])?;
        }
    }
    if writes {
        for (name, id) in printer_globals {
            let (name_ptr, name_len) = literal(&builder, name)?;
            call(&builder, "rt_print_global", &[name_ptr, name_len, word_ty.const_int(*id as u64, false)])?;
        }
    }
    // Type names are written for the entry file's module, as the interpreter
    // writes them — the view the checker publishes for it (`compile_file`).
    let view = typelisp_print::shared::name_view();
    let words = |name: &str, words: &[typelisp_mem::type_words::Word]| {
        let (global, len) = crate::compile::type_table::constant_words(module, name, words);
        (global.as_pointer_value().const_to_int(word_ty), word_ty.const_int(len as u64, false))
    };
    let (module_ptr, module_len) = words("print_name_module", &typelisp_mem::type_words::ns_words(view.module()));
    // In the order of the names, so the executable is the same each time.
    let mut imports: Vec<(&(typelisp_mem::NsId, typelisp_mem::symbols::SymRef), &&'static str)> = view.imports().iter().collect();
    imports.sort_by_key(|(_, name)| **name);
    // One table, which the runtime decodes only when a name is written
    // (`typelisp_print::aot::rt_print_names`).
    let mut table = vec![module_ptr, module_len, word_ty.const_int(imports.len() as u64, false)];
    for (i, ((ns, sym), name)) in imports.into_iter().enumerate() {
        let (def_ptr, def_len) =
            words(&format!("print_name_import${}", i), &typelisp_mem::type_words::definition_words(*ns, *sym));
        let (name_ptr, name_len) = literal(&builder, name)?;
        table.extend([def_ptr, def_len, name_ptr, name_len]);
    }
    let names = module.add_global(word_ty.array_type(table.len() as u32), None, "print_names");
    names.set_initializer(&word_ty.const_array(&table));
    names.set_constant(true);
    names.set_linkage(inkwell::module::Linkage::Private);
    call(&builder, "rt_print_names", &[names.as_pointer_value().const_to_int(word_ty)])?;
    builder.build_return(None).map_err(|e| format!("failed to build the printer registration's return: {}", e))?;
    Ok(())
}

/// What the generated `main` asks `rt_heap_init` for: the cons arena's
/// capacity, whether it stays at that capacity, and the bound on the
/// process's allocations.
struct HeapInit {
    cells: usize,
    fixed: bool,
    max_memory: Option<usize>,
}

/// The cons-cell arena a board's executable starts with, which grows as the
/// heap lets it: the native default (`1 << 16` cells) is 3 MiB, more than
/// most boards have RAM.
const BOARD_HEAP_CAPACITY: usize = 1 << 10;

/// The cons-cell arena an `eval`-carrying executable asks `rt_heap_init` for.
/// Its startup reads, checks and runs the whole prelude plus the program's own
/// definitions into this heap before `main` proper begins, which the runtime's
/// own default (`1 << 16`) does not hold.
const EVAL_HEAP_CAPACITY: usize = 1 << 18;

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

/// Emits one call to the runtime function `name` with `words` — the
/// startup-registration calling pattern, spelled once.
fn call_shim(
    module: &Module<'static>,
    builder: &inkwell::builder::Builder<'static>,
    name: &str,
    words: &[inkwell::values::IntValue<'static>],
) -> Result<(), String> {
    let words: Vec<inkwell::values::BasicValueEnum<'static>> = words.iter().map(|w| (*w).into()).collect();
    crate::compile::runtime_abi::call(builder, module, name, &words)?;
    Ok(())
}

/// The program entry `name` (`rt_run_program` and its kind), declared under
/// its signature.
fn declare_entry(module: &Module<'static>, name: &str) -> Result<inkwell::values::FunctionValue<'static>, String> {
    let sig = crate::compile::externs::runtime_sig(name).ok_or_else(|| format!("internal error: `{}` is not a program entry", name))?;
    Ok(crate::compile::runtime_abi::declare(module, name, sig))
}

/// The names of the printing shims (`typelisp_print::shim`), which is what
/// "this program prints" means at the IR level.
const PRINT_SHIMS: [&str; 10] = [
    // The two pieces of an expanded `format` that write any value: the
    // others write a number, a string or text, and need neither table.
    "rt_fmt_value",
    "rt_fmt_integer_any",
    "rt_pprint",
    "rt_pprint_block_start",
    "rt_pprint_block_end",
    "rt_pprint_newline",
    "rt_pprint_indent",
    "rt_pprint_tab",
    "rt_pprint_pop",
    "rt_pprint_list_exhausted",
];

/// Whether the shim `name` writes through the printer (`typelisp_print::shim`:
/// an expanded `format`'s pieces and the pretty printer's), which reads the
/// printer control variables whatever it writes.
fn writes_through_the_printer(name: &str) -> bool {
    name.starts_with("rt_fmt_") || name.starts_with("rt_pprint")
}

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
    let word_ty = crate::compile::carriers::word();
    if target.get_type() != crate::compile::llvm_builtins::coroutine_fn_type() {
        return Ok(target.as_global_value().as_pointer_value().const_to_int(word_ty));
    }
    let name = format!("{}$classic", target.get_name().to_string_lossy());
    if let Some(existing) = module.get_function(&name) {
        return Ok(existing.as_global_value().as_pointer_value().const_to_int(word_ty));
    }
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let classic_ty = word_ty.fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
    let door = module.add_function(&name, classic_ty, None);
    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(door, "entry"));
    let words = [
        target.as_global_value().as_pointer_value().const_to_int(word_ty),
        builder
            .build_ptr_to_int(
                door.get_nth_param(0).expect("the classic signature has two parameters").into_pointer_value(),
                word_ty,
                "callee_args",
            )
            .map_err(|e| format!("failed to take the argument array's address: {}", e))?,
        builder
            .build_int_z_extend(
                door.get_nth_param(1).expect("the classic signature has two parameters").into_int_value(),
                word_ty,
                "callee_argc",
            )
            .map_err(|e| format!("failed to widen the argument count: {}", e))?,
    ];
    let words = words.map(inkwell::values::BasicValueEnum::from);
    let (_, out) = crate::compile::runtime_abi::call(&builder, module, "rt_drive_body", &words)?;
    builder.build_return(Some(&out)).map_err(|e| format!("failed to build the door's return: {}", e))?;
    Ok(door.as_global_value().as_pointer_value().const_to_int(word_ty))
}


#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;
    
    use inkwell::AddressSpace;

    use super::{build_main_wrapper, remove_archives_under, write_archive_once, write_executable, RemoveLib, STATICLIB_NAME};
    use crate::compile::{llvm_context, COMPILE_LOCK};

    fn tmp_path(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp");
        std::fs::create_dir_all(&dir).expect("failed to create the AOT test scratch dir");
        dir.join(name)
    }

    /// `--remove-lib`, `--others` and `--all` each remove the build-id
    /// folders they name, and none of them touches what is not a build id.
    #[test]
    fn remove_lib_removes_the_build_ids_it_names_and_nothing_else() {
        let root = tmp_path("remove-lib");
        let (own, old, older) = ("00000000000000aa", "00000000000000bb", "00000000000000cc");
        let fill = || {
            let _ = std::fs::remove_dir_all(&root);
            for id in [own, old, older, "not-a-build-id"] {
                write_archive_once(&root.join(id), b"archive").expect("the archive is written");
            }
        };
        let left = || {
            let mut names: Vec<String> =
                std::fs::read_dir(&root).expect("the folder is there").map(|e| e.expect("an entry").file_name().into_string().expect("utf-8")).collect();
            names.sort();
            names
        };
        let removed = |which| -> Vec<String> {
            remove_archives_under(&root, Some(own), which)
                .expect("removal succeeds")
                .iter()
                .map(|p| p.file_name().expect("a name").to_str().expect("utf-8").to_string())
                .collect()
        };

        fill();
        assert_eq!(removed(RemoveLib::This), vec![own]);
        assert_eq!(left(), vec![old, older, "not-a-build-id"]);
        assert_eq!(removed(RemoveLib::This), Vec::<String>::new());

        fill();
        assert_eq!(removed(RemoveLib::Others), vec![old, older]);
        assert_eq!(left(), vec![own, "not-a-build-id"]);

        fill();
        assert_eq!(removed(RemoveLib::All), vec![own, old, older]);
        assert_eq!(left(), vec!["not-a-build-id"]);

        std::fs::remove_dir_all(&root).expect("the folder can be removed");
        assert_eq!(removed(RemoveLib::All), Vec::<String>::new());
    }

    /// A release `typl` writes its archive at the first link and reuses it
    /// after: the second write is not a write, and nothing but the archive is
    /// left in the folder.
    #[test]
    fn the_archive_is_written_once_and_left_in_place() {
        let dir = tmp_path("archive-once");
        let _ = std::fs::remove_dir_all(&dir);
        let first = write_archive_once(&dir, b"first").expect("the first write succeeds");
        assert_eq!(first, dir.join(STATICLIB_NAME));
        let second = write_archive_once(&dir, b"second").expect("the second call succeeds");
        assert_eq!(second, first);
        assert_eq!(std::fs::read(&first).expect("the archive is there"), b"first");
        let names: Vec<_> = std::fs::read_dir(&dir).expect("the folder is there").map(|e| e.expect("an entry").file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from(STATICLIB_NAME)]);
        std::fs::remove_dir_all(&dir).expect("the folder can be removed");
    }

    /// Stage 0's proof that AOT-linked native code can call a `#[no_mangle]`
    /// Rust function from this crate's own `staticlib` artifact (see
    /// `super::link_archive`'s doc comment) through nothing more than an
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
        let fn_ty = crate::compile::carriers::word().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let tl_main = module.add_function("tl_main", fn_ty, None);

        let builder = ctx.create_builder();
        let entry = ctx.append_basic_block(tl_main, "entry");
        builder.position_at_end(entry);
        let forty_one = crate::compile::carriers::word().const_int(41, false);
        let sig = crate::compile::runtime_abi::shim(typelisp_rt::rt_ping as unsafe extern "C" fn(typelisp_mem::Word) -> typelisp_mem::Word).sig;
        let rt_ping = crate::compile::runtime_abi::declare(&module, "rt_ping", &sig);
        let (_, result) = crate::compile::runtime_abi::call_declared(&builder, rt_ping, &sig, &[forty_one.into()]).unwrap();
        // A `main` that returns normally exits 0 whatever it answers
        // (`rt_run_entry`), so the answer leaves through `exit` instead.
        let exit_ty = ctx.void_type().fn_type(&[ctx.i32_type().into()], false);
        let exit = module.add_function("exit", exit_ty, None);
        let code = builder.build_int_truncate(result.into_int_value(), ctx.i32_type(), "code").unwrap();
        builder.build_call(exit, &[code.into()], "").unwrap();
        builder.build_unreachable().unwrap();

        build_main_wrapper(ctx, &module, "tl_main", None, &[], &super::Globals::default(), &[], &[], &[], &[], &[], &[], &[], &[], &[], None, false, super::Target::Host, super::HeapInit { cells: typelisp_rt::DEFAULT_AOT_HEAP_CAPACITY, fixed: false, max_memory: None }).expect("build_main_wrapper failed");
        module.verify().expect("module failed verification");

        let out_path = tmp_path("rt_ping_test");
        write_executable(&module, out_path.to_str().unwrap(), &[], &[]).expect("write_executable failed");

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
        let fn_ty = crate::compile::carriers::word().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let tl_main = module.add_function("tl_main", fn_ty, None);

        let builder = ctx.create_builder();
        let entry = ctx.append_basic_block(tl_main, "entry");
        builder.position_at_end(entry);
        let sig = crate::compile::runtime_abi::shim(typelisp_rt::rt_heap_live_count as unsafe extern "C" fn() -> typelisp_mem::Word).sig;
        let rt_heap_live_count = crate::compile::runtime_abi::declare(&module, "rt_heap_live_count", &sig);
        let (_, result) = crate::compile::runtime_abi::call_declared(&builder, rt_heap_live_count, &sig, &[]).unwrap();
        builder.build_return(Some(&result)).unwrap();

        build_main_wrapper(ctx, &module, "tl_main", None, &[], &super::Globals::default(), &[], &[], &[], &[], &[], &[], &[], &[], &[], None, false, super::Target::Host, super::HeapInit { cells: typelisp_rt::DEFAULT_AOT_HEAP_CAPACITY, fixed: false, max_memory: None }).expect("build_main_wrapper failed");
        module.verify().expect("module failed verification");

        let out_path = tmp_path("rt_heap_init_test");
        write_executable(&module, out_path.to_str().unwrap(), &[], &[]).expect("write_executable failed");

        let status = Command::new(&out_path).status().expect("failed to run the compiled executable");
        assert_eq!(status.code(), Some(0));
    }
}
