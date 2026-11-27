//! `std::dbg::llvm-size-of`: how many bytes compiled code holds a value in, as
//! LLVM's type of it says. Every number is checked at its width wherever it
//! is kept, out of every operation and through every cast
//! (`common::llvm_sizes`) — compiled in the session, in an executable, and on
//! the targets whose words are narrower. The Uno's are in `aot_avr_test`.
//!
//! Needs what `aot_embedded_test` and `aot_wasm_test` need.

extern crate typelisp;
mod common;
#[path = "common/llvm_sizes.rs"]
#[allow(dead_code)]
mod llvm_sizes;

use common::board::on_board;
use common::{eval_err, eval_ok_compiled, eval_string_compiled};
use typelisp::Value;
use llvm_sizes::{Machine, Program, HOST, THIRTY_TWO};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// `say` for a machine with the runtime: a `y` or an `n` printed.
const SAY: &str = "(defun say ((b bool)) () (print \"~a\" (if b \"y\" \"n\")) ())\n";

fn all_passed(what: &str, program: &Program, said: &str) {
    let failed = program.failures(said);
    assert!(failed.is_empty(), "{}:\n{}", what, failed.join("\n"));
}

#[test]
fn every_width_compiled_in_the_session() {
    let program = llvm_sizes::program(&HOST);
    let compiles: String = program.functions().iter().map(|n| format!("(compile {})\n", n)).collect();
    let source = format!(
        "(defvar (*said* string) \"\")\n(defun say ((b bool)) () (setf *said* (append *said* (if b \"y\" \"n\"))) ())\n{}\n{}\n(run-checks)\n*said*",
        program.source, compiles
    );
    all_passed("compiled", &program, &eval_string_compiled(&source));
}

/// `typl -c` of the program for `m` with `flags`, to a file of the extension
/// `ext`.
fn built(name: &str, program: &Program, flags: &[&str], ext: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    let src = dir.join("prog.typl");
    let out = dir.join(format!("prog{}", ext));
    std::fs::write(&src, format!("{}{}\n(defun main () int (run-checks) (println \".\") 0)\n", SAY, program.source)).unwrap();
    let typl = Command::new(env!("CARGO_BIN_EXE_typl"))
        .args(["-c", src.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .args(flags)
        .output()
        .expect("failed to start the typl binary");
    assert!(typl.status.success(), "typl -c {:?}: {}", flags, String::from_utf8_lossy(&typl.stderr));
    out
}

fn output(command: &mut Command) -> String {
    let out = command.stdin(Stdio::null()).output().expect("failed to run the program");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(text.trim_end().ends_with('.'), "the program did not finish: {}{}", text, String::from_utf8_lossy(&out.stderr));
    text
}

fn each_level(name: &str, m: &Machine, flags: &[&str], ext: &str, run: impl Fn(PathBuf) -> Command) {
    let program = llvm_sizes::program(m);
    for level in ["-O0", "-O2"] {
        let file = built(&format!("{}{}", name, level), &program, &[&[level], flags].concat(), ext);
        all_passed(level, &program, &output(&mut run(file)));
    }
}

#[test]
fn every_width_in_an_executable() {
    each_level("llvm_sizes_native", &HOST, &[], "", Command::new);
}

#[test]
fn every_width_on_a_cortex_m() {
    each_level("llvm_sizes_board", &THIRTY_TWO, &["--target", "thumbv7em-none-eabihf"], ".elf", |elf| {
        on_board("thumbv7em-none-eabihf", &elf)
    });
}

#[test]
fn every_width_on_a_cortex_m0plus() {
    each_level("llvm_sizes_m0", &THIRTY_TWO, &["--target", "thumbv6m-none-eabi"], ".elf", |elf| on_board("thumbv6m-none-eabi", &elf));
}

#[test]
fn every_width_on_a_riscv32() {
    each_level("llvm_sizes_riscv32", &THIRTY_TWO, &["--target", "riscv32imac-unknown-none-elf"], ".elf", |elf| {
        on_board("riscv32imac-unknown-none-elf", &elf)
    });
}

#[test]
fn every_width_in_webassembly() {
    each_level("llvm_sizes_wasm", &THIRTY_TWO, &["--target", "wasm32-wasip1"], ".wasm", |wasm| {
        let mut node = Command::new("node");
        node.arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("run-wasi.mjs")).arg(wasm);
        node
    });
}

/// Interpreted code holds its values in no LLVM type.
#[test]
fn interpreted_code_has_no_llvm_type() {
    let err = eval_err("(use std::dbg::llvm-size-of)\n(llvm-size-of (the u8 1))");
    assert!(err.contains("interpreted code holds no value in an LLVM type"), "{}", err);
}

/// The argument is a value of any type, and is evaluated.
#[test]
fn what_llvm_size_of_takes() {
    let source = "(use std::dbg::llvm-size-of)
(defvar (n int) 0)
(defun f () u32 (llvm-size-of (progn (setf n 1) (the u16 5))))
(compile f)
(+ (as int (f)) (* 10 n))";
    assert_eq!(eval_ok_compiled(source), Value::Int(12));
    let err = common::check_err("(use std::dbg::llvm-size-of)\n(llvm-size-of)");
    assert!(err.contains("llvm-size-of"), "{}", err);
}
