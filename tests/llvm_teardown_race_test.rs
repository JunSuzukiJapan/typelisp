//! Tearing an interpreter down on one thread must not disturb another thread
//! that is compiling.
//!
//! Every LLVM object in this process belongs to one shared `Context`
//! (`compile::llvm_context`), and `COMPILE_LOCK` serializes the code that
//! *builds* IR. Destruction was outside that rule: dropping an
//! `ExecutionEngine` destroys the module of JIT'd code inside it, and
//! `~Module` unregisters every value name from the Context's own tables. Since
//! a `CompiledFn` dies wherever the `FnDef` holding it dies — usually an
//! interpreter going out of scope, on a thread doing nothing LLVM-related — it
//! raced any thread that happened to be compiling, and the crash landed in
//! `llvm::Value::destroyValueName` naming nothing that would suggest a drop.
//!
//! The fix is `compile::retire_llvm`: an engine's last share is handed to a
//! list and destroyed by whichever thread next takes the lock to compile.
//!
//! This test spawns its own threads rather than relying on the harness running
//! two `#[test]`s at once, so it means the same thing under
//! `scripts/test-serial.sh` (`--test-threads=1`) as under a bare `cargo test`.
//! It is a race, so it is a probe, not a proof: it reproduced the crash within
//! seconds every time before the fix, which is enough to notice a regression.

use std::sync::{Arc, Barrier};

use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

/// How many build-and-drop cycles each thread runs. The pre-fix crash landed
/// well inside this many; more only costs time.
const ROUNDS: usize = 12;

/// Threads racing each other. Two suffice for the pairing that crashed (one
/// tearing down, one installing); a third widens the window for free.
const THREADS: usize = 3;

/// One cycle: build an interpreter with the prelude installed — which parses
/// the prelude bitcode into the shared Context and JITs one engine's worth of
/// compiled bodies — run something, then drop the lot. The drop is the half
/// that used to be unsynchronized, and installing the prelude is the LLVM work
/// on the other side of the race: the crash report that started this showed
/// exactly this pairing, one thread in `MCJIT::~MCJIT` and another inside
/// `install_compiled_library`'s bitcode parse. No `load_compiler`/`(compile
/// f)` needed on top — those only make each cycle slower.
fn build_prelude_and_drop() {
    let mut heap = Heap::with_capacity(1 << 16);
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    let reader = Reader::new();
    let forms = reader
        .read_all(&mut heap, "(defun f ((a int) (b int)) int (+ a b)) (f 20 22)")
        .expect("read failed");
    let mut last = Value::Empty;
    for form in forms {
        let checked = checker.check_form(&mut heap, &interp, form).expect("check failed");
        if let Some(value) = interp.exec(&mut heap, checked).expect("eval failed") {
            last = value;
        }
    }
    assert_eq!(last, Value::Int(42), "the interpreter came back wrong");
}

/// One cycle of the *other* teardown: a real compile, which registers the
/// module and its builders in the thread-local LLVM handle registry
/// (`NativeHandle`), and a thread that then ends — taking the registry, and
/// with it the last share of those objects, down with it.
///
/// Heavier than [`build_prelude_and_drop`] (the compiler island has to be
/// installed before `(compile f)` means anything), so it runs fewer rounds.
fn compile_and_drop() {
    let mut heap = Heap::with_capacity(1 << 16);
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    typelisp::load_compiler(&mut heap, &mut checker, &mut interp);
    let reader = Reader::new();
    let forms = reader
        .read_all(&mut heap, "(defun f ((a int) (b int)) int (+ a b)) (compile f) (f 20 22)")
        .expect("read failed");
    let mut last = Value::Empty;
    for form in forms {
        let checked = checker.check_form(&mut heap, &interp, form).expect("check failed");
        if let Some(value) = interp.exec(&mut heap, checked).expect("eval failed") {
            last = value;
        }
    }
    assert_eq!(last, Value::Int(42), "the compiled function did not run");
}

/// The engine half: a thread dropping `CompiledFn`s while others install
/// bitcode.
#[test]
fn interpreters_can_be_torn_down_while_another_thread_compiles() {
    // Start together, so the first teardown lands while the others are still
    // installing rather than after they have all finished.
    let barrier = Arc::new(Barrier::new(THREADS));
    let workers: Vec<_> = (0..THREADS)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..ROUNDS {
                    build_prelude_and_drop();
                }
            })
        })
        .collect();
    for worker in workers {
        // A panic inside a worker propagates here as a test failure; the
        // failure this guards against is a segfault, which takes the whole
        // binary down and is reported as such.
        worker.join().expect("a worker thread failed");
    }
}

/// The registry half: modules and builders whose last share is the
/// thread-local handle registry, released either at the end of a compile
/// (`llvm_handles_release`) or when the thread itself ends.
///
/// **This one never reproduced a crash**, with the fix reverted or in place
/// (3/3 green either way, at six rounds and three compiles per round as well
/// as at these numbers). Unlike the engine teardown above, the registry
/// teardown is guarded on the *rule* — destruction touches the shared Context,
/// so it belongs under the lock — rather than on an observed failure. Kept
/// because nothing else drives concurrent compiles to completion and then ends
/// the threads, so a future crash on that path lands here; do not read a pass
/// as evidence the path is delicate.
#[test]
fn compiling_threads_can_exit_while_another_thread_compiles() {
    const COMPILE_ROUNDS: usize = 3;
    let barrier = Arc::new(Barrier::new(THREADS));
    let workers: Vec<_> = (0..THREADS)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..COMPILE_ROUNDS {
                    compile_and_drop();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("a worker thread failed");
    }
}
