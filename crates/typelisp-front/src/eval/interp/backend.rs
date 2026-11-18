//! The compile backend the interpreter is linked with, as a table of function
//! pointers, and the declarations it is handed.

use std::rc::Rc;
use crate::{Heap, Path, SymRef, TypeId, Value};
use crate::check::repr::Repr;
use crate::eval::value::EvalError;
use super::{with_active_interp, CompiledBody, Interp};

/// One `(defffi ...)` declaration, as the backend needs to see it.
///
/// Plain data rather than the core-IR node it was read from, for the reason
/// [`Backend`] is `fn` pointers: this crosses to the crate holding LLVM, and
/// what crosses should be the answer, not the question. `Interp::exec` reads
/// the node (it owns the readers), and the backend emits the thunk (it owns
/// the codegen).
///
/// `params`/`ret` are the declared types — `i8`, `f32`, `string`, `ptr`.
/// The `Repr`s beside them in the node say how each value crosses the
/// compiled boundary; these say what the C function is declared to take,
/// which is a different question with a different answer for all six
/// integer widths. Which types are actually callable is the backend's to
/// decide, and it decides it in one place.
#[derive(Clone, Debug)]
pub struct FfiDecl {
    /// The typelisp-side name, which is also what the thunk's LLVM symbol is
    /// built from — so a compiled call site resolves it with no special case.
    pub path: crate::Path,
    /// The symbol to look up: the C function's own name.
    pub c_symbol: String,
    /// The library to look it up in. `None` means the running process, which
    /// is what reaches libc and everything else already linked.
    pub library: Option<String>,
    pub params: Vec<TypeId>,
    pub ret: TypeId,
}

/// What a function hands the compiler: its parameters, each with the
/// representation its declared type lowers to, and its body forms.
pub type FnBody = (Vec<(SymRef, Repr)>, Vec<Value>);

/// One `llvm-*`/native-scope builtin, as the island calls it.
pub type LlvmBuiltinFn = fn(&mut Heap, &Path, SymRef, &[Value]) -> Option<Result<Value, EvalError>>;

/// `(compile name)`.
pub type CompileFn = fn(&Interp, &mut Heap, &crate::CompileTarget) -> Result<Value, EvalError>;

/// `(disassemble name [llvm])`.
pub type DisassembleFn = fn(&Interp, &mut Heap, &crate::CompileTarget, bool) -> Result<String, EvalError>;

/// `(defffi ...)`.
pub type DefineFfiFn = fn(&Interp, &FfiDecl) -> Result<Rc<dyn CompiledBody>, String>;

/// Everything the interpreter needs from the LLVM backend, as plain `fn`
/// pointers.
///
/// A hook rather than direct calls because all four are the *backend*
/// (`crate::compile`), and the interpreter is the front end. Inverting the
/// reference is what lets the front end stop naming `inkwell` at all — and
/// let it become a crate that does not depend on the one holding LLVM,
/// which it now is: `typelisp-front` (`docs/dev/implementation-log.md`,
/// "typelisp-front クレートと AOT 内 eval").
///
/// Plain `fn` pointers rather than a trait object for the reason
/// [`typelisp_print::runtime::PrintHooks`] uses them: these are only ever
/// called Rust-side, never emitted as a call by the compiler, so there is no
/// ABI to pin down and `Result`/`String` cross directly.
#[derive(Clone, Copy)]
pub struct Backend {
    /// The `llvm-*`/native-scope builtins the compiler island calls to emit
    /// IR. `None` means "not one of mine".
    pub llvm_builtin: LlvmBuiltinFn,
    /// Whether `handle` is a live entry in the backend's handle registry —
    /// the one question [`Interp::decode_compiled_return`] asks about a
    /// `Repr::Handle` result it is otherwise passing straight through.
    pub handle_is_live: fn(i64) -> bool,
    /// `(compile name)`.
    pub compile_function: CompileFn,
    /// `(disassemble name [llvm])` — the text, for the caller to print. The
    /// `bool` asks for LLVM IR instead of host assembly.
    pub disassemble_function: DisassembleFn,
    /// `(compile-file source output)`.
    pub compile_file: fn(&str, &str) -> Result<(), String>,
    /// `(dump path)`.
    pub dump_image: fn(&Interp, &mut Heap, &str) -> Result<(), String>,
    /// `(defffi ...)` — resolve the C symbol and emit the thunk that reaches
    /// it, returning a body to hang on the declaration's [`FnDef`].
    ///
    /// Returns the body rather than installing it, so the one place that
    /// decides where a `FnDef` lives stays the one place that puts anything
    /// into it (`Interp::exec`).
    pub define_ffi: DefineFfiFn,
    /// Makes the entry C calls for a callback — the function and the
    /// function type C calls it at (`crate::ffi_callback`) — and answers its
    /// address. The function is compiled first if it is not already.
    pub callback_entry: fn(&Interp, &mut Heap, crate::ffi_callback::CallbackSig) -> Result<(usize, Rc<dyn CompiledBody>), String>,
}

thread_local! {
    /// Installed by [`crate::compile::install_llvm_backend`], which
    /// `compiler::load_aot` calls — loading the island is what makes any of
    /// this reachable in the first place.
    ///
    /// Thread-local for the reason the active heap is: `cargo test` drives one
    /// compiler per thread, many per process.
    static BACKEND: std::cell::Cell<Option<Backend>> = const { std::cell::Cell::new(None) };
}

/// Registers the LLVM backend for this thread.
///
/// Also installs the runtime's way to make a C callback entry compiled code
/// asks for (`typelisp_rt::ffi_callback`): a session with a backend can make
/// one, and an executable, which never calls this, registered all of its own
/// at startup.
pub fn set_backend(backend: Backend) {
    BACKEND.with(|cell| cell.set(Some(backend)));
    typelisp_rt::ffi_callback::set_factory(callback_entry_from_compiled_code);
}

/// [`typelisp_rt::ffi_callback::set_factory`]'s factory: the entry for
/// `callback`, made through the interpreter compiled code is running under.
/// The call that asks is made on that interpreter's thread (the bridge puts
/// it there, `externs::needs_interpreter`), which is where the interpreter is
/// active.
fn callback_entry_from_compiled_code(callback: typelisp_rt::ffi_callback::Callback) -> Result<usize, String> {
    let (ns, sym, ty) = callback;
    let sig = crate::ffi_callback::CallbackSig { function: Path::of_symbol(ns, sym), ty };
    // SAFETY: the runtime asks only from inside a compiled call, which
    // registered this thread's heap on the way in.
    let heap = unsafe { typelisp_abi::active_heap() };
    match with_active_interp(|i| i.make_callback_entry(heap, sig)) {
        Some(r) => r,
        None => Err("ffi: a C callback was asked for with no interpreter running on this thread".to_string()),
    }
}

/// The registered backend, or an error naming what needed it.
///
/// No backend at all is an invariant break rather than a program error: an
/// `Interp` built without loading the island has no business reaching any of
/// this, and gets told so rather than a quietly missing result.
pub(super) fn backend(who: &str) -> Result<Backend, EvalError> {
    BACKEND.with(|cell| cell.get()).ok_or_else(|| {
        EvalError::Internal(format!("{} needs the LLVM backend, and none is registered on this thread", who))
    })
}

/// The registered backend's `(compile name)`, for the evaluator's own
/// `compile` special form.
pub(crate) fn backend_compile_function() -> Result<CompileFn, EvalError> {
    Ok(backend("compile")?.compile_function)
}

/// The registered backend's `(defffi ...)`, for the evaluator's own
/// declaration handling.
pub(crate) fn backend_define_ffi() -> Result<DefineFfiFn, EvalError> {
    Ok(backend("defffi")?.define_ffi)
}

/// The registered backend's `(disassemble name)`, for the evaluator's own
/// `disassemble` special form.
pub(crate) fn backend_disassemble_function() -> Result<DisassembleFn, EvalError> {
    Ok(backend("disassemble")?.disassemble_function)
}

/// Runs one `llvm-*`/native-scope builtin through the registered backend.
///
/// `None` from the backend means "not one of mine", and falls through to the
/// caller's own `None`.
pub(super) fn eval_llvm_builtin_method(
    heap: &mut Heap,
    type_name: &Path,
    method: SymRef,
    args: &[Value],
) -> Option<Result<Value, EvalError>> {
    match backend(&format!("`{}`", typelisp_mem::ConcreteNames::method_link(type_name, method.name()))) {
        Ok(b) => (b.llvm_builtin)(heap, type_name, method, args),
        Err(e) => Some(Err(e)),
    }
}

impl crate::CompileTarget {
    /// The definition's name as one string: its path for a function,
    /// [`typelisp_mem::ConcreteNames::method_link`] for a method. What the runtime's
    /// messages say, and what a rewritten `~/name/` control string carries.
    pub fn link_name(&self) -> String {
        match self {
            crate::CompileTarget::Fn(path) => path.to_string(),
            crate::CompileTarget::Method(type_name, method) => typelisp_mem::ConcreteNames::method_link(type_name, method.name()),
        }
    }

    /// The definition's name as the source writes it, `point::area` for a
    /// method — what the checker's messages say.
    pub fn written_name(&self) -> String {
        match self {
            crate::CompileTarget::Fn(path) => path.to_string(),
            crate::CompileTarget::Method(type_name, method) => format!("{}::{}", type_name, method.name()),
        }
    }
}
