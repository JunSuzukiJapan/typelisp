//! Parallel JIT compiles against the one process-wide LLVM `Context`.
//!
//! [`COMPILE_LOCK`](typelisp::compile::COMPILE_LOCK) serializes whole
//! compiles; this hammers it from several threads at once, each with its own
//! session, compiling and running a small mutually recursive program. A lost
//! lock shows up as a crash (LLVM's unsynchronized type-uniquing tables), a
//! broken reentrancy as a hang or a panic — rather than as a rare flake in
//! the wider suite.
//!
//! It spawns its own threads, so it exercises parallelism even under
//! `scripts/test-serial.sh`'s `--test-threads=1`.

extern crate typelisp;
use typelisp::{load_compiler, Checker, EvalError, Heap, Interp, Reader, Value};

/// One thread's share: its own session compiling and running a small
/// program. The mutually recursive pair exercises the SCC path (members
/// forward-declared before any body is translated); the second compile
/// exercises compiling again into a session that already has native code.
fn compile_and_run(tag: u32) -> Result<(), EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    chk.publish_names(&h);
    typelisp::compile::install_llvm_backend();
    let mut interp = Interp::new();
    load_compiler(&mut h, &mut chk, &mut interp);
    let src = format!(
        r#"
        (defsignature odd-rec (int) bool)
        (defun even-rec ((n int)) bool (if (= n 0) true (odd-rec (- n 1))))
        (defun odd-rec ((n int)) bool (if (= n 0) false (even-rec (- n 1))))
        (defun driver-{tag} () int (if (even-rec 10) 100 200))
        (compile driver-{tag})
        (driver-{tag})
        (defun add3-{tag} ((a int) (b int) (c int)) int (+ a (+ b c)))
        (compile add3-{tag})
        (add3-{tag} 10 20 12)
        "#
    );
    let r = Reader::new();
    let mut forms = r.forms(&src);
    // A `defsignature`, three `defun`s, a `compile`, the call, one more
    // `defun`, a `compile`, the call: the calls are the 6th and 9th forms.
    let mut index = 0;
    while let Some((v, _)) = forms.next_form(&mut h).expect("read failed") {
        index += 1;
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        let val = interp.exec(&mut h, tl).map_err(EvalError::into_kind)?;
        match (index, val) {
            (6, Some(Value::Int(n))) => assert_eq!(n, 100, "thread {tag}: wrong answer from compiled code"),
            (9, Some(Value::Int(n))) => assert_eq!(n, 42, "thread {tag}: wrong answer from compiled code"),
            (6, other) | (9, other) => panic!("thread {tag}: expected an Int answer, got {:?}", other),
            _ => {}
        }
    }
    assert_eq!(index, 9, "thread {tag}: the program lost a form");
    Ok(())
}

#[test]
fn parallel_jit_compiles_from_separate_sessions_agree() {
    const THREADS: u32 = 8;
    let mut peers = Vec::new();
    for tag in 0..THREADS {
        peers.push(std::thread::spawn(move || compile_and_run(tag)));
    }
    for (tag, peer) in peers.into_iter().enumerate() {
        peer.join().unwrap_or_else(|_| panic!("thread {tag} panicked")).expect("compile or run failed");
    }
}
