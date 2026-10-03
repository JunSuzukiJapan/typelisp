//! The JIT: LLVM's ORC (an `LLJIT` linking with JITLink), reached through
//! `llvm-sys` because inkwell does not wrap it. Every `unsafe` call into ORC
//! is in this file.
//!
//! A module reaches ORC as an object file, not as IR. Adding IR means wrapping
//! its `LLVMContext` in a `ThreadSafeContext`, which takes ownership of the
//! context, and this crate's context is the one every module in the process
//! shares ([`super::llvm_context`]). The object is made by an ordinary
//! `TargetMachine` for the triple and data layout the `LLJIT` reports, and the
//! module stays its owner's.
//!
//! Each [`JitCode`] links into a JITDylib of its own, holding the object and
//! one absolute symbol per name the caller supplies plus
//! [`codegen_helpers`]. Nothing else is searched: no process lookup, no other
//! JITDylib. A name the object refers to and nothing defines is an error
//! naming it, the same on every OS.
//!
//! On Mach-O each module gets a last function that closes its unwind table
//! ([`end_unwind_table`]), working around an LLVM bug that otherwise leaves
//! most functions without unwind information.
//!
//! The C API cannot remove a JITDylib, only clear it, and a cleared one left
//! behind still holds memory. So a cleared JITDylib is kept and handed to the
//! next [`JitCode`]: the number of JITDylibs is the most that were ever alive
//! at once.

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Condvar, Mutex, OnceLock};

use inkwell::llvm_sys;
use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetData, TargetMachine, TargetTriple};
use inkwell::OptimizationLevel;

use llvm_sys::error::{LLVMDisposeErrorMessage, LLVMErrorRef, LLVMGetErrorMessage};
use llvm_sys::orc2::lljit::{
    LLVMOrcCreateLLJIT, LLVMOrcCreateLLJITBuilder, LLVMOrcLLJITAddObjectFile, LLVMOrcLLJITGetDataLayoutStr,
    LLVMOrcLLJITGetExecutionSession, LLVMOrcLLJITGetTripleString, LLVMOrcLLJITMangleAndIntern, LLVMOrcLLJITRef,
};
use llvm_sys::orc2::{
    LLVMJITEvaluatedSymbol, LLVMJITSymbolFlags, LLVMJITSymbolGenericFlags, LLVMOrcAbsoluteSymbols,
    LLVMOrcCJITDylibSearchOrderElement, LLVMOrcCLookupSetElement, LLVMOrcCSymbolMapPair, LLVMOrcCSymbolMapPairs,
    LLVMOrcDisposeMaterializationUnit, LLVMOrcExecutionSessionCreateBareJITDylib, LLVMOrcExecutionSessionLookup,
    LLVMOrcExecutionSessionSetErrorReporter, LLVMOrcJITDylibClear, LLVMOrcJITDylibDefine, LLVMOrcJITDylibLookupFlags,
    LLVMOrcJITDylibRef, LLVMOrcLookupKind, LLVMOrcReleaseSymbolStringPoolEntry, LLVMOrcSymbolLookupFlags,
    LLVMOrcSymbolStringPoolEntryRef,
};

/// The personality routine of every landing pad this crate emits.
const RUST_EH_PERSONALITY: &str = "rust_eh_personality";

extern "C" {
    fn rust_eh_personality();
    fn _Unwind_Resume();
    fn floor(x: f64) -> f64;
    fn ceil(x: f64) -> f64;
    fn trunc(x: f64) -> f64;
    fn round(x: f64) -> f64;
    fn sqrt(x: f64) -> f64;
    fn sin(x: f64) -> f64;
    fn cos(x: f64) -> f64;
    fn exp(x: f64) -> f64;
    fn log(x: f64) -> f64;
    fn pow(x: f64, y: f64) -> f64;
    fn fmax(x: f64, y: f64) -> f64;
    fn fmin(x: f64, y: f64) -> f64;
    fn fmod(x: f64, y: f64) -> f64;
}

/// Functions code generation may call without the module declaring them, so
/// no caller can pass them. Every JITDylib gets all of them.
///
/// - `resume` becomes a call to `_Unwind_Resume`, and [`end_unwind_table`]
///   adds a landing pad under `rust_eh_personality`.
/// - Each `f64` intrinsic the compiler emits (`llvm.floor.f64` and the rest,
///   in `llvm_builtins`) is either an instruction or a call to the libm
///   function of the same name, depending on the target and its features —
///   x86_64 without SSE4.1 calls `floor`, arm64 has an instruction. So every
///   one is here, whether or not this machine calls it. `frem` becomes `fmod`.
///
/// An intrinsic added to the compiler needs its libm function added here. A
/// call missing from this list fails the JIT with the symbol's name, so
/// nothing runs into address 0.
fn codegen_helpers() -> [(&'static str, usize); 15] {
    [
        (RUST_EH_PERSONALITY, rust_eh_personality as *const () as usize),
        ("_Unwind_Resume", _Unwind_Resume as *const () as usize),
        ("floor", floor as *const () as usize),
        ("ceil", ceil as *const () as usize),
        ("trunc", trunc as *const () as usize),
        ("round", round as *const () as usize),
        ("sqrt", sqrt as *const () as usize),
        ("sin", sin as *const () as usize),
        ("cos", cos as *const () as usize),
        ("exp", exp as *const () as usize),
        ("log", log as *const () as usize),
        ("pow", pow as *const () as usize),
        ("fmax", fmax as *const () as usize),
        ("fmin", fmin as *const () as usize),
        ("fmod", fmod as *const () as usize),
    ]
}

/// The process's `LLJIT`, and the `TargetMachine` that makes its objects.
struct Jit {
    jit: LLVMOrcLLJITRef,
    machine: TargetMachine,
    /// The `LLJIT`'s data layout, set on every module before it is emitted.
    layout: String,
    triple: TargetTriple,
    /// Whether the JIT links Mach-O objects (see [`end_unwind_table`]).
    mach_o: bool,
}

// SAFETY: `LLJIT` is safe to use from any thread. The `TargetMachine` is used
// only by `JitCode::new`, whose callers hold `COMPILE_LOCK`.
unsafe impl Send for Jit {}
unsafe impl Sync for Jit {}

static JIT: OnceLock<Result<Jit, String>> = OnceLock::new();

/// Errors the execution session reports with no caller to hand them to. The
/// next [`JitCode::new`] returns them, so none is lost.
static REPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Cleared JITDylibs, waiting to be used again.
struct FreeDylibs(Vec<LLVMOrcJITDylibRef>);

// SAFETY: a JITDylib belongs to the execution session, which is thread-safe;
// the pointers are only handed out and taken back.
unsafe impl Send for FreeDylibs {}

static FREE_DYLIBS: Mutex<FreeDylibs> = Mutex::new(FreeDylibs(Vec::new()));

fn take_error(err: LLVMErrorRef) -> Result<(), String> {
    if err.is_null() {
        return Ok(());
    }
    // SAFETY: `err` is an error ORC handed over; `LLVMGetErrorMessage`
    // consumes it.
    unsafe {
        let message = LLVMGetErrorMessage(err);
        let text = CStr::from_ptr(message).to_string_lossy().into_owned();
        LLVMDisposeErrorMessage(message);
        Err(text)
    }
}

extern "C" fn report_error(_ctx: *mut c_void, err: LLVMErrorRef) {
    if let Err(text) = take_error(err) {
        REPORTED.lock().unwrap_or_else(|e| e.into_inner()).push(text);
    }
}

fn reported_errors() -> Result<(), String> {
    let reported = std::mem::take(&mut *REPORTED.lock().unwrap_or_else(|e| e.into_inner()));
    if reported.is_empty() {
        Ok(())
    } else {
        Err(reported.join("; "))
    }
}

fn jit() -> Result<&'static Jit, String> {
    JIT.get_or_init(|| {
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| format!("failed to initialize the native target: {}", e))?;
        let mut jit = std::ptr::null_mut();
        // SAFETY: the builder is consumed by `LLVMOrcCreateLLJIT`, and the
        // `LLJIT` lives for the rest of the process.
        unsafe {
            take_error(LLVMOrcCreateLLJIT(&mut jit, LLVMOrcCreateLLJITBuilder()))
                .map_err(|e| format!("failed to create the JIT: {}", e))?;
            LLVMOrcExecutionSessionSetErrorReporter(LLVMOrcLLJITGetExecutionSession(jit), report_error, std::ptr::null_mut());
        }
        // SAFETY: both strings belong to the `LLJIT`, which is never disposed.
        let (triple, layout) = unsafe {
            (
                CStr::from_ptr(LLVMOrcLLJITGetTripleString(jit)).to_string_lossy().into_owned(),
                CStr::from_ptr(LLVMOrcLLJITGetDataLayoutStr(jit)).to_string_lossy().into_owned(),
            )
        };
        let triple = TargetTriple::create(&triple);
        let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
        let machine = target
            .create_target_machine(&triple, "", "", OptimizationLevel::None, RelocMode::PIC, CodeModel::Default)
            .ok_or_else(|| format!("failed to create a target machine for `{}`", triple))?;
        let ours = machine.get_target_data().get_data_layout().as_str().to_string_lossy().into_owned();
        if ours != layout {
            return Err(format!(
                "the target machine for `{}` lays data out as `{}`, the JIT as `{}`",
                triple, ours, layout
            ));
        }
        let mach_o = triple.as_str().to_string_lossy().contains("-apple-");
        Ok(Jit { jit, machine, layout, triple, mach_o })
    })
    .as_ref()
    .map_err(|e| e.clone())
}

impl Jit {
    /// `name` as the object file spells it (Mach-O prefixes `_`), interned.
    /// The caller owns the returned reference.
    fn intern(&self, name: &str) -> Result<LLVMOrcSymbolStringPoolEntryRef, String> {
        let name = CString::new(name).map_err(|_| format!("symbol name `{}` contains a NUL", name))?;
        // SAFETY: `name` outlives the call; the entry comes back retained.
        Ok(unsafe { LLVMOrcLLJITMangleAndIntern(self.jit, name.as_ptr()) })
    }

    fn dylib(&self) -> LLVMOrcJITDylibRef {
        if let Some(dylib) = FREE_DYLIBS.lock().unwrap_or_else(|e| e.into_inner()).0.pop() {
            return dylib;
        }
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let name = format!("typelisp-jit-{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let name = CString::new(name).expect("no NUL in a formatted number");
        // SAFETY: the session copies the name; the JITDylib lives as long as
        // the session, which is the process.
        unsafe { LLVMOrcExecutionSessionCreateBareJITDylib(LLVMOrcLLJITGetExecutionSession(self.jit), name.as_ptr()) }
    }

    /// Defines `symbols` in `dylib` as absolute addresses.
    fn define(&self, dylib: LLVMOrcJITDylibRef, symbols: &[(&str, usize)]) -> Result<(), String> {
        let flags = LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsExported as u8
            | LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsCallable as u8;
        let mut pairs = Vec::with_capacity(symbols.len());
        for (name, addr) in symbols {
            pairs.push(LLVMOrcCSymbolMapPair {
                Name: self.intern(name)?,
                Sym: LLVMJITEvaluatedSymbol {
                    Address: *addr as u64,
                    Flags: LLVMJITSymbolFlags { GenericFlags: flags, TargetFlags: 0 },
                },
            });
        }
        // SAFETY: `LLVMOrcAbsoluteSymbols` takes over the names' references;
        // `LLVMOrcJITDylibDefine` takes the unit on success only.
        unsafe {
            let unit = LLVMOrcAbsoluteSymbols(pairs.as_mut_ptr(), pairs.len());
            if let Err(e) = take_error(LLVMOrcJITDylibDefine(dylib, unit)) {
                LLVMOrcDisposeMaterializationUnit(unit);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Looks `names` up in `dylib` alone, linking the object there on the
    /// first lookup.
    fn lookup(&self, dylib: LLVMOrcJITDylibRef, names: &[String]) -> Result<HashMap<String, usize>, String> {
        struct Answer {
            result: Mutex<Option<Result<Vec<(LLVMOrcSymbolStringPoolEntryRef, u64)>, String>>>,
            ready: Condvar,
        }
        extern "C" fn handle(err: LLVMErrorRef, pairs: LLVMOrcCSymbolMapPairs, n: usize, ctx: *mut c_void) {
            // SAFETY: `ctx` is the `Answer` `lookup` waits on, alive until
            // this has stored a result; `pairs` holds `n` entries for the
            // duration of this call.
            let answer = unsafe { &*(ctx as *const Answer) };
            let out = take_error(err).map(|()| {
                (0..n).map(|i| unsafe { ((*pairs.add(i)).Name, (*pairs.add(i)).Sym.Address) }).collect::<Vec<_>>()
            });
            *answer.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(out);
            answer.ready.notify_all();
        }

        let mut interned = Vec::with_capacity(names.len());
        for name in names {
            interned.push(self.intern(name)?);
        }
        let release = |interned: &[LLVMOrcSymbolStringPoolEntryRef]| {
            for entry in interned {
                // SAFETY: each was retained by `intern` and is released once.
                unsafe { LLVMOrcReleaseSymbolStringPoolEntry(*entry) };
            }
        };
        let mut set: Vec<LLVMOrcCLookupSetElement> = interned
            .iter()
            .map(|entry| LLVMOrcCLookupSetElement {
                Name: *entry,
                LookupFlags: LLVMOrcSymbolLookupFlags::LLVMOrcSymbolLookupFlagsRequiredSymbol,
            })
            .collect();
        let mut order = [LLVMOrcCJITDylibSearchOrderElement {
            JD: dylib,
            JDLookupFlags: LLVMOrcJITDylibLookupFlags::LLVMOrcJITDylibLookupFlagsMatchAllSymbols,
        }];
        let answer = Answer { result: Mutex::new(None), ready: Condvar::new() };
        // SAFETY: the search order and set are read during the call (the
        // set's names are retained by the session, not taken); `answer`
        // outlives the wait below.
        unsafe {
            LLVMOrcExecutionSessionLookup(
                LLVMOrcLLJITGetExecutionSession(self.jit),
                LLVMOrcLookupKind::LLVMOrcLookupKindStatic,
                order.as_mut_ptr(),
                order.len(),
                set.as_mut_ptr(),
                set.len(),
                handle,
                &answer as *const Answer as *mut c_void,
            );
        }
        let mut slot = answer.result.lock().unwrap_or_else(|e| e.into_inner());
        while slot.is_none() {
            slot = answer.ready.wait(slot).unwrap_or_else(|e| e.into_inner());
        }
        let found = slot.take().expect("the loop above waits for a result");
        drop(slot);
        let found = match found {
            Ok(found) => found,
            Err(e) => {
                release(&interned);
                return Err(e);
            }
        };
        let mut addresses = HashMap::with_capacity(names.len());
        for (name, entry) in names.iter().zip(&interned) {
            match found.iter().find(|(n, _)| n == entry) {
                Some((_, addr)) => {
                    addresses.insert(name.clone(), *addr as usize);
                }
                None => {
                    release(&interned);
                    return Err(format!("the lookup of `{}` returned no address", name));
                }
            }
        }
        release(&interned);
        Ok(addresses)
    }
}

/// Name of the function [`end_unwind_table`] adds, and of the one it calls.
const UNWIND_TABLE_END: &str = "typelisp.unwind_table_end";
const UNWIND_TABLE_END_CALLEE: &str = "typelisp.unwind_table_end.callee";

/// On Mach-O, adds a function to the end of `module` whose unwind record
/// closes the unwind table after every other function.
///
/// JITLink builds the `__unwind_info` that libunwind searches from the
/// object's compact-unwind records, and LLVM 22.1.8 builds it wrong: it
/// merges adjacent records with the same encoding — correct, a record covers
/// up to the next one — and then ends the table at the end of the *last
/// remaining record's* function (`CompactUnwindManager::writeIndexes`) rather
/// than the last function's. When the module's last functions share an
/// encoding, as most do at `-O0`, the table stops after the first of them and
/// a panic through any of the rest finds no unwind information ("failed to
/// initiate panic, error 5"). JITLink has dropped the FDEs those records made
/// redundant, so there is nothing to fall back on.
///
/// A record in DWARF mode is never merged, so a last function whose record is
/// in DWARF mode puts the end of the table after everything. This one has a
/// personality and a landing pad, which code generation describes in DWARF on
/// both x86_64 and arm64. Functions are laid out in module order, so it lands
/// last. It is never called.
///
/// Rewriting the records instead does not work: on arm64, a function whose
/// unwinding compact unwind can describe gets no FDE at all.
fn end_unwind_table(module: &Module<'static>) -> Result<(), String> {
    if module.get_function(UNWIND_TABLE_END).is_some() || module.get_function(UNWIND_TABLE_END_CALLEE).is_some() {
        return Err(format!("internal error: the module was already JIT-compiled (it has `{}`)", UNWIND_TABLE_END));
    }
    let ctx = module.get_context();
    let void_fn = ctx.void_type().fn_type(&[], false);
    let personality = match module.get_function(RUST_EH_PERSONALITY) {
        Some(f) => f,
        None => module.add_function(RUST_EH_PERSONALITY, ctx.i32_type().fn_type(&[], true), None),
    };
    let callee = module.add_function(UNWIND_TABLE_END_CALLEE, void_fn, None);
    let end = module.add_function(UNWIND_TABLE_END, void_fn, None);
    end.set_personality_function(personality);
    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(callee, "entry"));
    builder.build_return(None).map_err(|e| e.to_string())?;
    let entry = ctx.append_basic_block(end, "entry");
    let normal = ctx.append_basic_block(end, "normal");
    let pad = ctx.append_basic_block(end, "pad");
    builder.position_at_end(entry);
    builder.build_invoke(callee, &[], normal, pad, "").map_err(|e| e.to_string())?;
    builder.position_at_end(normal);
    builder.build_return(None).map_err(|e| e.to_string())?;
    builder.position_at_end(pad);
    let ptr = ctx.ptr_type(inkwell::AddressSpace::default());
    let landing = builder
        .build_landing_pad(ctx.struct_type(&[ptr.into(), ctx.i32_type().into()], false), personality, &[], true, "")
        .map_err(|e| e.to_string())?;
    builder.build_resume(landing).map_err(|e| e.to_string())?;
    Ok(())
}

/// The code of one JIT-compiled module: its JITDylib, and the address of
/// every function the module defined.
///
/// Dropping it clears the JITDylib, which frees the code and unregisters its
/// unwind information. Nothing may be running in that code or unwinding
/// through it then — see [`super::retire_llvm`].
pub struct JitCode {
    dylib: LLVMOrcJITDylibRef,
    addresses: HashMap<String, usize>,
}

// SAFETY: the JITDylib belongs to the execution session, which is safe to use
// from any thread; nothing else in a `JitCode` is shared.
unsafe impl Send for JitCode {}
unsafe impl Sync for JitCode {}

impl JitCode {
    /// Emits `module` as an object, links it against `externals` (and
    /// [`codegen_helpers`]) and resolves every function it defines.
    ///
    /// The caller must hold [`super::COMPILE_LOCK`]: this uses the shared
    /// context's module and the one `TargetMachine`.
    pub fn new(module: &Module<'static>, externals: &[(String, usize)]) -> Result<JitCode, String> {
        reported_errors()?;
        let jit = jit()?;
        module.set_triple(&jit.triple);
        module.set_data_layout(&TargetData::create(&jit.layout).get_data_layout());
        if jit.mach_o {
            end_unwind_table(module)?;
        }
        let object = jit.machine.write_to_memory_buffer(module, FileType::Object).map_err(|e| e.to_string())?;
        let defined: Vec<String> = module
            .get_functions()
            .filter(|f| f.count_basic_blocks() > 0)
            .map(|f| f.get_name().to_string_lossy().into_owned())
            .collect();

        let mut symbols: Vec<(&str, usize)> = externals.iter().map(|(name, addr)| (name.as_str(), *addr)).collect();
        for (name, addr) in codegen_helpers() {
            if !symbols.iter().any(|(n, _)| *n == name) {
                symbols.push((name, addr));
            }
        }

        // From here on the JITDylib may hold something, so a failure clears it
        // (by dropping `code`).
        let mut code = JitCode { dylib: jit.dylib(), addresses: HashMap::new() };
        let dylib = code.dylib;
        if !symbols.is_empty() {
            jit.define(dylib, &symbols)?;
        }
        let bytes = object.as_slice();
        // SAFETY: the copy is handed to ORC, which takes ownership of it
        // whether or not the add succeeds.
        unsafe {
            let copy = llvm_sys::core::LLVMCreateMemoryBufferWithMemoryRangeCopy(
                bytes.as_ptr() as *const c_char,
                bytes.len(),
                c"typelisp-jit-object".as_ptr(),
            );
            take_error(LLVMOrcLLJITAddObjectFile(jit.jit, dylib, copy))?;
        }
        if !defined.is_empty() {
            // A failed link reports the missing symbols through the session
            // and fails the lookup only with "Failed to materialize": the
            // first says what is wrong, so it goes first.
            code.addresses = jit.lookup(dylib, &defined).map_err(|e| match reported_errors() {
                Ok(()) => e,
                Err(reported) => format!("{}; {}", reported, e),
            })?;
        }
        reported_errors()?;
        Ok(code)
    }

    /// The address of `name`, one of the functions the module defined.
    pub fn address(&self, name: &str) -> Result<usize, String> {
        self.addresses
            .get(name)
            .copied()
            .ok_or_else(|| format!("`{}` is not a function this module defined", name))
    }
}

impl Drop for JitCode {
    /// Clears the JITDylib and keeps it for the next [`JitCode`]. If the
    /// clear fails, the JITDylib is not reused, and the error goes where the
    /// session's own do: the next [`JitCode::new`] returns it.
    fn drop(&mut self) {
        // SAFETY: the JITDylib is this `JitCode`'s alone until it is put back.
        match take_error(unsafe { LLVMOrcJITDylibClear(self.dylib) }) {
            Ok(()) => FREE_DYLIBS.lock().unwrap_or_else(|e| e.into_inner()).0.push(self.dylib),
            Err(e) => REPORTED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("clearing a JIT dylib failed: {}", e)),
        }
    }
}
