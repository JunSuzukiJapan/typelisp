//! Stage 1 of moving the JIT from MCJIT to ORC (`~/.claude/plans/jit-orc-migration.md`):
//! does the design hold up before anything in `src/` changes?
//!
//! The design: a module becomes an object file through an ordinary
//! `TargetMachine` (the process-wide `LLVMContext` cannot be handed to a
//! `ThreadSafeContext`), each compile gets a bare JITDylib, and every name the
//! object refers to is defined there with `LLVMOrcAbsoluteSymbols` — no
//! process lookup. These probes check, on whatever machine they run on:
//!
//! 1. a panic crosses a JIT frame and reaches `catch_unwind`;
//! 2. a cleanup landing pad in a JIT frame runs, and the panic continues;
//! 3. a name the object refers to but nobody defined is an error naming it —
//!    including `_Unwind_Resume`, which code generation adds by itself;
//! 4. the module can be dropped once the object exists;
//! 5. after a JITDylib is cleared, the next panic through other JIT code still
//!    unwinds (MCJIT hangs when its engine goes mid-unwind);
//! 6. how much a cleared JITDylib leaves behind, since the C API cannot
//!    remove one.
//!
//! Throwaway: stage 2 replaces it with the real `compile::jit_engine` and the
//! existing `compiled_unwind_test.rs`.

use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use inkwell::llvm_sys;
use inkwell::module::{Linkage, Module};
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetData, TargetMachine};
use inkwell::values::ValueKind;
use inkwell::{AddressSpace, OptimizationLevel};

use llvm_sys::error::*;
use llvm_sys::orc2::lljit::*;
use llvm_sys::orc2::*;

use typelisp::compile::{llvm_context, COMPILE_LOCK};

extern "C" {
    fn rust_eh_personality();
    fn _Unwind_Resume();
}

const PANIC_MESSAGE: &str = "orc probe";

type UnwindingProbe = unsafe extern "C-unwind" fn(*const i64, u32) -> i64;

#[no_mangle]
pub extern "C-unwind" fn typelisp_orc_probe_maybe_panic(n: i64) -> i64 {
    if n == 0 {
        panic!("{}", PANIC_MESSAGE);
    }
    n * 2
}

static CLEANUP_RUNS: AtomicUsize = AtomicUsize::new(0);

#[no_mangle]
pub extern "C-unwind" fn typelisp_orc_probe_cleanup_ran() {
    CLEANUP_RUNS.fetch_add(1, Ordering::SeqCst);
}

// ---- the ORC side ----------------------------------------------------------

fn take_error(err: LLVMErrorRef) -> Result<(), String> {
    if err.is_null() {
        return Ok(());
    }
    unsafe {
        let msg = LLVMGetErrorMessage(err);
        let text = CStr::from_ptr(msg).to_string_lossy().into_owned();
        LLVMDisposeErrorMessage(msg);
        Err(text)
    }
}

/// Errors the session reports with no caller to return them to. Collected,
/// and checked by [`Jit::lookup`], so none is dropped.
static REPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

extern "C" fn report_error(_ctx: *mut c_void, err: LLVMErrorRef) {
    if let Err(text) = take_error(err) {
        REPORTED.lock().unwrap_or_else(|e| e.into_inner()).push(text);
    }
}

struct Jit {
    jit: LLVMOrcLLJITRef,
    machine: TargetMachine,
}

// SAFETY: LLJIT is thread-safe; the TargetMachine is only used under COMPILE_LOCK.
unsafe impl Send for Jit {}
unsafe impl Sync for Jit {}

static JIT: OnceLock<Jit> = OnceLock::new();

fn jit() -> &'static Jit {
    JIT.get_or_init(|| unsafe {
        Target::initialize_native(&InitializationConfig::default()).expect("native target");
        let mut jit = std::ptr::null_mut();
        take_error(LLVMOrcCreateLLJIT(&mut jit, LLVMOrcCreateLLJITBuilder())).expect("LLVMOrcCreateLLJIT");
        LLVMOrcExecutionSessionSetErrorReporter(LLVMOrcLLJITGetExecutionSession(jit), report_error, std::ptr::null_mut());

        let triple = inkwell::targets::TargetTriple::create(&CStr::from_ptr(LLVMOrcLLJITGetTripleString(jit)).to_string_lossy());
        let target = Target::from_triple(&triple).expect("target for the JIT's triple");
        let machine = target
            .create_target_machine(&triple, "", "", OptimizationLevel::None, RelocMode::PIC, CodeModel::Default)
            .expect("target machine");
        let ours = machine.get_target_data().get_data_layout().as_str().to_string_lossy().into_owned();
        let theirs = CStr::from_ptr(LLVMOrcLLJITGetDataLayoutStr(jit)).to_string_lossy().into_owned();
        assert_eq!(ours, theirs, "the TargetMachine's data layout differs from LLJIT's");
        eprintln!("orc probe: triple {}", triple);
        Jit { jit, machine }
    })
}

/// One compile's code: a bare JITDylib holding the object and the names it was given.
struct JitCode {
    dylib: LLVMOrcJITDylibRef,
}

impl JitCode {
    fn clear(&self) -> Result<(), String> {
        take_error(unsafe { LLVMOrcJITDylibClear(self.dylib) })
    }
}

static DYLIB_SEQ: AtomicUsize = AtomicUsize::new(0);

impl Jit {
    fn intern(&self, name: &str) -> LLVMOrcSymbolStringPoolEntryRef {
        let c = CString::new(name).unwrap();
        unsafe { LLVMOrcLLJITMangleAndIntern(self.jit, c.as_ptr()) }
    }

    /// Emits `module` as an object and adds it, with `externals`, to a new bare JITDylib.
    fn add(&self, module: &Module<'static>, externals: &[(&str, usize)]) -> Result<JitCode, String> {
        let name = CString::new(format!("orc-probe-{}", DYLIB_SEQ.fetch_add(1, Ordering::Relaxed))).unwrap();
        let dylib = unsafe { LLVMOrcExecutionSessionCreateBareJITDylib(LLVMOrcLLJITGetExecutionSession(self.jit), name.as_ptr()) };
        self.add_into(dylib, module, externals)
    }

    /// Like [`Self::add`], into an existing (cleared) JITDylib.
    fn add_into(&self, dylib: LLVMOrcJITDylibRef, module: &Module<'static>, externals: &[(&str, usize)]) -> Result<JitCode, String> {
        module.set_triple(&self.machine.get_triple());
        module.set_data_layout(&TargetData::create(&self.machine.get_target_data().get_data_layout().as_str().to_string_lossy()).get_data_layout());
        let object = self.machine.write_to_memory_buffer(module, FileType::Object).map_err(|e| e.to_string())?;
        unsafe {
            if !externals.is_empty() {
                let mut pairs: Vec<LLVMOrcCSymbolMapPair> = externals
                    .iter()
                    .map(|(name, addr)| LLVMOrcCSymbolMapPair {
                        Name: self.intern(name),
                        Sym: LLVMJITEvaluatedSymbol {
                            Address: *addr as u64,
                            Flags: LLVMJITSymbolFlags {
                                GenericFlags: LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsExported as u8
                                    | LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsCallable as u8,
                                TargetFlags: 0,
                            },
                        },
                    })
                    .collect();
                let mu = LLVMOrcAbsoluteSymbols(pairs.as_mut_ptr(), pairs.len());
                if let Err(e) = take_error(LLVMOrcJITDylibDefine(dylib, mu)) {
                    LLVMOrcDisposeMaterializationUnit(mu);
                    return Err(e);
                }
            }
            let copy = llvm_sys::core::LLVMCreateMemoryBufferWithMemoryRangeCopy(
                object.as_slice().as_ptr() as *const c_char,
                object.as_slice().len(),
                c"orc-probe-object".as_ptr(),
            );
            take_error(LLVMOrcLLJITAddObjectFile(self.jit, dylib, copy))?;
            Ok(JitCode { dylib })
        }
    }

    /// Looks `name` up in `code`'s JITDylib only, which links it.
    fn lookup(&self, code: &JitCode, name: &str) -> Result<usize, String> {
        struct Answer {
            result: Mutex<Option<Result<u64, String>>>,
            ready: std::sync::Condvar,
        }
        extern "C" fn handle(err: LLVMErrorRef, pairs: LLVMOrcCSymbolMapPairs, n: usize, ctx: *mut c_void) {
            let answer = unsafe { &*(ctx as *const Answer) };
            let out = take_error(err).and_then(|()| {
                if n == 1 {
                    Ok(unsafe { (*pairs).Sym.Address })
                } else {
                    Err(format!("lookup returned {} symbols", n))
                }
            });
            *answer.result.lock().unwrap() = Some(out);
            answer.ready.notify_all();
        }
        let answer = Answer { result: Mutex::new(None), ready: std::sync::Condvar::new() };
        let symbol = self.intern(name);
        unsafe {
            let mut order = [LLVMOrcCJITDylibSearchOrderElement {
                JD: code.dylib,
                JDLookupFlags: LLVMOrcJITDylibLookupFlags::LLVMOrcJITDylibLookupFlagsMatchAllSymbols,
            }];
            let mut set = [LLVMOrcCLookupSetElement {
                Name: symbol,
                LookupFlags: LLVMOrcSymbolLookupFlags::LLVMOrcSymbolLookupFlagsRequiredSymbol,
            }];
            LLVMOrcExecutionSessionLookup(
                LLVMOrcLLJITGetExecutionSession(self.jit),
                LLVMOrcLookupKind::LLVMOrcLookupKindStatic,
                order.as_mut_ptr(),
                1,
                set.as_mut_ptr(),
                1,
                handle,
                &answer as *const Answer as *mut c_void,
            );
        }
        let mut slot = answer.result.lock().unwrap();
        while slot.is_none() {
            slot = answer.ready.wait(slot).unwrap();
        }
        unsafe { LLVMOrcReleaseSymbolStringPoolEntry(symbol) };
        let reported = std::mem::take(&mut *REPORTED.lock().unwrap_or_else(|e| e.into_inner()));
        if !reported.is_empty() {
            return Err(reported.join("; "));
        }
        slot.take().unwrap().map(|a| a as usize)
    }
}

// ---- the modules -----------------------------------------------------------

/// `probe(args, argc) = typelisp_orc_probe_maybe_panic(args[0])`.
fn build_probe(name: &str) -> Module<'static> {
    let ctx = llvm_context();
    let module = ctx.create_module(name);
    let i64_t = ctx.i64_type();
    let callee = module.add_function("typelisp_orc_probe_maybe_panic", i64_t.fn_type(&[i64_t.into()], false), Some(Linkage::External));
    let probe = module.add_function(
        "probe",
        i64_t.fn_type(&[ctx.ptr_type(AddressSpace::default()).into(), ctx.i32_type().into()], false),
        None,
    );
    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(probe, "entry"));
    let arg = builder
        .build_load(i64_t, probe.get_nth_param(0).unwrap().into_pointer_value(), "arg0")
        .unwrap()
        .into_int_value();
    let called = builder.build_call(callee, &[arg.into()], "call").unwrap();
    let ValueKind::Basic(v) = called.try_as_basic_value() else { panic!("no value") };
    builder.build_return(Some(&v.into_int_value())).unwrap();
    module.verify().expect("probe module failed verification");
    module
}

/// `protected(args, argc)`: the same call through an `invoke` whose cleanup
/// pad calls `typelisp_orc_probe_cleanup_ran` and `resume`s.
fn build_protected(name: &str) -> Module<'static> {
    let ctx = llvm_context();
    let module = ctx.create_module(name);
    let i64_t = ctx.i64_type();
    let i32_t = ctx.i32_type();
    let ptr_t = ctx.ptr_type(AddressSpace::default());
    let callee = module.add_function("typelisp_orc_probe_maybe_panic", i64_t.fn_type(&[i64_t.into()], false), Some(Linkage::External));
    let cleanup_fn = module.add_function("typelisp_orc_probe_cleanup_ran", ctx.void_type().fn_type(&[], false), Some(Linkage::External));
    let personality = module.add_function("rust_eh_personality", i32_t.fn_type(&[], true), Some(Linkage::External));
    let protected = module.add_function("protected", i64_t.fn_type(&[ptr_t.into(), i32_t.into()], false), None);
    protected.set_personality_function(personality);
    let entry = ctx.append_basic_block(protected, "entry");
    let normal = ctx.append_basic_block(protected, "normal");
    let cleanup = ctx.append_basic_block(protected, "cleanup");
    let builder = ctx.create_builder();
    builder.position_at_end(entry);
    let arg = builder
        .build_load(i64_t, protected.get_nth_param(0).unwrap().into_pointer_value(), "arg0")
        .unwrap()
        .into_int_value();
    let invoked = builder.build_invoke(callee, &[arg.into()], normal, cleanup, "invoke").unwrap();
    builder.position_at_end(normal);
    let ValueKind::Basic(v) = invoked.try_as_basic_value() else { panic!("no value") };
    builder.build_return(Some(&v.into_int_value())).unwrap();
    builder.position_at_end(cleanup);
    let exception_type = ctx.struct_type(&[ptr_t.into(), i32_t.into()], false);
    let pad = builder.build_landing_pad(exception_type, personality, &[], true, "pad").unwrap();
    builder.build_call(cleanup_fn, &[], "cleanup_call").unwrap();
    builder.build_resume(pad).unwrap();
    module.verify().expect("protected module failed verification");
    module
}

fn probe_externals() -> Vec<(&'static str, usize)> {
    vec![("typelisp_orc_probe_maybe_panic", typelisp_orc_probe_maybe_panic as *const () as usize)]
}

fn protected_externals() -> Vec<(&'static str, usize)> {
    vec![
        ("typelisp_orc_probe_maybe_panic", typelisp_orc_probe_maybe_panic as *const () as usize),
        ("typelisp_orc_probe_cleanup_ran", typelisp_orc_probe_cleanup_ran as *const () as usize),
        ("rust_eh_personality", rust_eh_personality as *const () as usize),
        ("_Unwind_Resume", _Unwind_Resume as *const () as usize),
    ]
}

/// Builds, adds and resolves; the module is dropped before this returns (probe 4).
fn compile(module: Module<'static>, entry: &str, externals: &[(&str, usize)]) -> Result<(JitCode, UnwindingProbe), String> {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let jit = jit();
    let code = jit.add(&module, externals)?;
    drop(module);
    let addr = jit.lookup(&code, entry)?;
    Ok((code, unsafe { std::mem::transmute::<usize, UnwindingProbe>(addr) }))
}

static PROBE: OnceLock<usize> = OnceLock::new();
static PROTECTED: OnceLock<usize> = OnceLock::new();

fn kept(slot: &'static OnceLock<usize>, build: fn(&str) -> Module<'static>, entry: &str, externals: fn() -> Vec<(&'static str, usize)>) -> UnwindingProbe {
    let addr = *slot.get_or_init(|| {
        let (code, f) = compile(build(entry), entry, &externals()).expect("compile failed");
        std::mem::forget(code);
        f as usize
    });
    unsafe { std::mem::transmute::<usize, UnwindingProbe>(addr) }
}

fn call(f: UnwindingProbe, arg: i64) -> i64 {
    let argv = [arg];
    unsafe { f(argv.as_ptr(), 1) }
}

fn without_panic_output<T>(body: impl FnOnce() -> T) -> T {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = body();
    std::panic::set_hook(previous);
    out
}

fn caught_message(f: UnwindingProbe) -> String {
    let payload = without_panic_output(|| catch_unwind(AssertUnwindSafe(|| call(f, 0))))
        .expect_err("the panic did not propagate out of the JIT frame");
    payload.downcast_ref::<String>().cloned().expect("payload was not this probe's String")
}

// ---- the probes ------------------------------------------------------------

#[test]
fn probe_1_a_panic_crosses_an_orc_frame() {
    let f = kept(&PROBE, build_probe, "probe", probe_externals);
    assert_eq!(call(f, 21), 42);
    assert_eq!(caught_message(f), PANIC_MESSAGE);
    assert_eq!(caught_message(f), PANIC_MESSAGE);
    assert_eq!(call(f, 50), 100);
}

#[test]
fn probe_2_a_cleanup_pad_in_an_orc_frame_runs_and_resumes() {
    let f = kept(&PROTECTED, build_protected, "protected", protected_externals);
    let before = CLEANUP_RUNS.load(Ordering::SeqCst);
    assert_eq!(call(f, 21), 42);
    assert_eq!(CLEANUP_RUNS.load(Ordering::SeqCst), before);
    for i in 1..=3 {
        assert_eq!(caught_message(f), PANIC_MESSAGE);
        assert_eq!(CLEANUP_RUNS.load(Ordering::SeqCst), before + i, "the landing pad did not run");
    }
    assert_eq!(call(f, 50), 100);
}

#[test]
fn probe_3_a_name_nobody_defined_is_an_error_naming_it() {
    let err = compile(build_probe("probe"), "probe", &[]).err().expect("an undefined callee was accepted");
    assert!(err.contains("typelisp_orc_probe_maybe_panic"), "error does not name the symbol: {}", err);

    // `_Unwind_Resume` is not declared in the module — code generation adds
    // it for the `resume` — and MCJIT left it to the process lookup.
    let without: Vec<_> = protected_externals().into_iter().filter(|(n, _)| *n != "_Unwind_Resume").collect();
    let err = compile(build_protected("protected"), "protected", &without).err().expect("a missing _Unwind_Resume was accepted");
    assert!(err.contains("_Unwind_Resume"), "error does not name _Unwind_Resume: {}", err);
}

#[test]
fn probe_5_clearing_a_dylib_after_an_unwind_does_not_break_the_next_one() {
    let (first, f) = compile(build_protected("protected"), "protected", &protected_externals()).expect("compile failed");
    assert_eq!(caught_message(f), PANIC_MESSAGE);
    {
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        first.clear().expect("clearing the dylib failed");
    }
    // A hang here is the MCJIT failure mode; the watchdog turns it into a failure.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (_second, g) = compile(build_protected("protected"), "protected", &protected_externals()).expect("compile failed");
        let message = caught_message(g);
        let _ = tx.send(message);
    });
    let message = rx.recv_timeout(std::time::Duration::from_secs(30)).expect("the second unwind hung or died");
    assert_eq!(message, PANIC_MESSAGE);
}

fn max_rss_kib() -> i64 {
    #[repr(C)]
    struct Rusage {
        utime: [i64; 2],
        stime: [i64; 2],
        maxrss: i64,
        rest: [i64; 13],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut usage = std::mem::MaybeUninit::<Rusage>::zeroed();
    unsafe { getrusage(0, usage.as_mut_ptr()) };
    let raw = unsafe { usage.assume_init() }.maxrss;
    // macOS reports bytes, Linux KiB.
    if cfg!(target_os = "macos") { raw / 1024 } else { raw }
}

#[test]
fn probe_6_what_cleared_dylibs_leave_behind() {
    const ROUNDS: usize = 10_000;
    let mut marks = Vec::new();
    for i in 0..ROUNDS {
        let (code, f) = compile(build_probe("probe"), "probe", &probe_externals()).expect("compile failed");
        assert_eq!(call(f, 21), 42);
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        code.clear().expect("clearing the dylib failed");
        if i % 2_000 == 0 || i == ROUNDS - 1 {
            marks.push((i, max_rss_kib()));
        }
    }
    eprintln!("orc probe 6: (round, max RSS KiB) = {:?}", marks);
}

/// Baseline for probe 6: the same loop through today's MCJIT `jit_engine`,
/// each engine dropped at the end of its round. Run alone (max RSS is per process).
#[test]
fn probe_6_baseline_mcjit() {
    const ROUNDS: usize = 10_000;
    let mut marks = Vec::new();
    for i in 0..ROUNDS {
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let module = build_probe("probe");
        let externals: Vec<(String, usize)> = probe_externals().into_iter().map(|(n, a)| (n.to_string(), a)).collect();
        let engine = typelisp::compile::jit_engine(&module, &externals).expect("jit_engine failed");
        let f = unsafe { std::mem::transmute::<usize, UnwindingProbe>(engine.get_function_address("probe").unwrap()) };
        assert_eq!(call(f, 21), 42);
        drop(engine);
        drop(module);
        if i % 2_000 == 0 || i == ROUNDS - 1 {
            marks.push((i, max_rss_kib()));
        }
    }
    eprintln!("orc probe 6 (MCJIT baseline): (round, max RSS KiB) = {:?}", marks);
}

/// Probe 6 without the clear: how much of the growth the clear gives back.
#[test]
fn probe_6_without_clear() {
    const ROUNDS: usize = 10_000;
    let mut marks = Vec::new();
    for i in 0..ROUNDS {
        let (code, f) = compile(build_probe("probe"), "probe", &probe_externals()).expect("compile failed");
        assert_eq!(call(f, 21), 42);
        std::mem::forget(code);
        if i % 2_000 == 0 || i == ROUNDS - 1 {
            marks.push((i, max_rss_kib()));
        }
    }
    eprintln!("orc probe 6 (no clear): (round, max RSS KiB) = {:?}", marks);
}

/// Probe 6 reusing one cleared JITDylib instead of making a new one per round.
#[test]
fn probe_6_reusing_a_cleared_dylib() {
    const ROUNDS: usize = 10_000;
    let mut marks = Vec::new();
    let mut dylib = None;
    for i in 0..ROUNDS {
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let jit = jit();
        let module = build_probe("probe");
        let code = match dylib {
            None => jit.add(&module, &probe_externals()),
            Some(d) => jit.add_into(d, &module, &probe_externals()),
        }
        .expect("add failed");
        drop(module);
        let f = unsafe { std::mem::transmute::<usize, UnwindingProbe>(jit.lookup(&code, "probe").expect("lookup failed")) };
        assert_eq!(call(f, 21), 42);
        code.clear().expect("clearing the dylib failed");
        dylib = Some(code.dylib);
        if i % 2_000 == 0 || i == ROUNDS - 1 {
            marks.push((i, max_rss_kib()));
        }
    }
    eprintln!("orc probe 6 (reused dylib): (round, max RSS KiB) = {:?}", marks);
}
