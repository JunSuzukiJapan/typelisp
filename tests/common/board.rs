//! Building a program for a board (`typl -c --target <triple>`) and running
//! it under the QEMU that stands for the board, its standard output and
//! error over semihosting: what the tests of every board share.
//!
//! Scratch files go under `target/aot-test-tmp/`, one name per test, for the
//! reason `tests/compile_file_test.rs`'s module doc comment gives.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// How long a program may run under QEMU before the test calls it hung: a
/// board's program that faults or never ends would otherwise hold the test
/// forever.
const RUN_LIMIT: Duration = Duration::from_secs(120);

/// The test's own scratch folder, `name`.
pub fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    dir
}

/// Compiles `source` with `flags` after `-c` into `output` in the scratch
/// folder `name`: `typl`'s own output, and the output's path.
pub fn build(name: &str, source: &str, output: &str, flags: &[&str]) -> (Output, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("prog.typl");
    let out_path = dir.join(output);
    std::fs::write(&src, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_typl"))
        .args(["-c", src.to_str().unwrap(), "-o", out_path.to_str().unwrap()])
        .args(flags)
        .output()
        .expect("failed to start the typl binary");
    (out, out_path)
}

/// [`build`], which must succeed: the output's path.
pub fn built(name: &str, source: &str, output: &str, flags: &[&str]) -> PathBuf {
    let (out, path) = build(name, source, output, flags);
    assert!(out.status.success(), "typl -c {:?} failed: {}", flags, String::from_utf8_lossy(&out.stderr));
    path
}

/// `--target triple`, then `flags`.
pub fn target_flags<'a>(triple: &'a str, flags: &[&'a str]) -> Vec<&'a str> {
    let mut all = vec!["--target", triple];
    all.extend_from_slice(flags);
    all
}

/// A command that runs `elf`, an executable `typl -c --target <triple>` wrote
/// for a board, under the QEMU that stands for the board (`Board::qemu`),
/// its standard output and error over semihosting and its standard input
/// closed.
pub fn on_board(triple: &str, elf: &Path) -> Command {
    let target = typelisp::compile::aot::Target::parse(triple).expect("a target typl knows");
    let (qemu, board) = target.board().expect("a board").qemu;
    let mut command = Command::new(qemu);
    command
        .args(board)
        .args(["-nographic", "-semihosting-config", "enable=on,target=native", "-kernel"])
        .arg(elf)
        .stdin(Stdio::null());
    command
}

/// Runs `elf` on the board `triple` ([`on_board`]), for at most
/// [`RUN_LIMIT`].
pub fn run_on(triple: &str, elf: &Path) -> Output {
    let mut child = on_board(triple, elf)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start QEMU (the board's executables run under QEMU)");
    let started = Instant::now();
    loop {
        if child.try_wait().expect("failed to wait for QEMU").is_some() {
            return child.wait_with_output().expect("failed to read QEMU's output");
        }
        if started.elapsed() > RUN_LIMIT {
            let _ = child.kill();
            let out = child.wait_with_output().expect("failed to read QEMU's output");
            panic!(
                "{} still ran after {:?}; it printed: {}{}",
                elf.display(),
                RUN_LIMIT,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `b` as text.
pub fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// What `source` prints and exits with on the board `triple`, after checking
/// that the native executable prints and exits the same.
pub fn same_as_native_on(triple: &str, name: &str, source: &str, flags: &[&str]) -> (String, String, Option<i32>) {
    let elf = built(name, source, "prog.elf", &target_flags(triple, flags));
    let native = built(name, source, "prog", flags);
    let b = run_on(triple, &elf);
    let n = Command::new(&native).output().expect("failed to run the native executable");
    assert_eq!(text(&b.stdout), text(&n.stdout), "stdout differs from the native executable's");
    assert_eq!(text(&b.stderr), text(&n.stderr), "stderr differs from the native executable's");
    assert_eq!(b.status.code(), n.status.code(), "exit code differs from the native executable's");
    (text(&b.stdout), text(&b.stderr), b.status.code())
}

/// What `typl -c --target triple` said when it refused `source`.
pub fn refused_on(triple: &str, name: &str, source: &str) -> String {
    let (out, _) = build(name, source, "prog.elf", &target_flags(triple, &[]));
    assert_eq!(out.status.code(), Some(1), "typl -c built it: {}", text(&out.stdout));
    text(&out.stderr)
}

/// The object the LLVM IR `ir_text` compiles to by the `llc` of the LLVM typl
/// is built with, for the core `llc_flags` name, in `dir`: functions a board
/// gives in C, for want of a C compiler for the board in every environment
/// the tests run in.
pub fn board_object(dir: &Path, ir_text: &str, llc_flags: &[&str]) -> PathBuf {
    let prefix = std::env::var("LLVM_SYS_221_PREFIX").expect("LLVM_SYS_221_PREFIX names the LLVM typl is built with");
    let ir = dir.join("board.ll");
    let object = dir.join("board.o");
    std::fs::write(&ir, ir_text).unwrap();
    let out = Command::new(Path::new(&prefix).join("bin").join("llc"))
        .args(llc_flags)
        .args(["-filetype=obj", "-o"])
        .arg(&object)
        .arg(&ir)
        .output()
        .expect("failed to start llc");
    assert!(out.status.success(), "llc failed: {}", text(&out.stderr));
    object
}
