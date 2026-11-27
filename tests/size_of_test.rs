//! `size-of` and `size-of-val`, and that every number is as wide as they say:
//! in every place a value is kept, through every cast, and out of every
//! operation (`common::widths`) — interpreted, compiled, in an executable, and
//! on the targets whose words are narrower.
//!
//! Needs what `aot_embedded_test` and `aot_wasm_test` need.

extern crate typelisp;
mod common;
#[path = "common/widths.rs"]
mod widths;

use common::board::on_board;
use common::{check_err, eval_ok, eval_string, eval_string_compiled, Load, Session};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use typelisp::Value;

/// What `text` says of the checks: that none failed.
fn all_passed(what: &str, text: &str) {
    let last = text.trim_end().lines().last().unwrap_or_default();
    let made: usize = last.strip_suffix(" checks").and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("{}: {}", what, text));
    assert!(made > 10_000, "{}: only {} checks", what, made);
    assert_eq!(text.trim_end().lines().count(), 1, "{}:\n{}", what, text.lines().take(40).collect::<Vec<_>>().join("\n"));
}

#[test]
fn every_width_interpreted() {
    all_passed("interpreted", &eval_string(&format!("{}\n(run-checks)", widths::program())));
}

#[test]
fn every_width_compiled() {
    let program = widths::program();
    let compiles: String = widths::functions(&program).iter().map(|n| format!("(compile {})\n", n)).collect();
    all_passed("compiled", &eval_string_compiled(&format!("{}\n{}\n(run-checks)", program, compiles)));
}

/// `typl -c` of the program with `flags`, to a file of the extension `ext`.
fn built(name: &str, flags: &[&str], ext: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    let src = dir.join("prog.typl");
    let out = dir.join(format!("prog{}", ext));
    std::fs::write(&src, format!("{}\n(defun main () int (println \"~a\" (run-checks)) 0)\n", widths::program())).unwrap();
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
    format!("{}{}", String::from_utf8_lossy(&out.stderr).lines().filter(|l| !l.contains("Warning") && !l.contains("--trace-warnings")).collect::<String>(), String::from_utf8_lossy(&out.stdout))
}

#[test]
fn every_width_in_an_executable() {
    for level in ["-O0", "-O2"] {
        let exe = built(&format!("widths_native{}", level), &[level], "");
        all_passed(level, &output(&mut Command::new(exe)));
    }
}

#[test]
fn every_width_on_a_cortex_m() {
    for level in ["-O0", "-O2"] {
        let elf = built(&format!("widths_board{}", level), &[level, "--target", "thumbv7em-none-eabihf"], ".elf");
        all_passed(level, &output(&mut on_board("thumbv7em-none-eabihf", &elf)));
    }
}

#[test]
fn every_width_on_a_riscv32() {
    for level in ["-O0", "-O2"] {
        let elf = built(&format!("widths_riscv32{}", level), &[level, "--target", "riscv32imac-unknown-none-elf"], ".elf");
        all_passed(level, &output(&mut on_board("riscv32imac-unknown-none-elf", &elf)));
    }
}

#[test]
fn every_width_in_webassembly() {
    for level in ["-O0", "-O2"] {
        let wasm = built(&format!("widths_wasm{}", level), &[level, "--target", "wasm32-wasip1"], ".wasm");
        let run = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("run-wasi.mjs");
        all_passed(level, &output(Command::new("node").arg(run).arg(wasm)));
    }
}

/// Two widths never meet unsaid: every operation on every two different number
/// types is a type error.
#[test]
fn two_widths_do_not_meet() {
    let types = ["i8", "u8", "i16", "u16", "i32", "u32", "f32", "f64", "int"];
    let mut s = Session::new(Load::Prelude);
    let mut met = Vec::new();
    for a in types {
        // The same forms are right with one width.
        for form in [format!("(lambda ((x {a}) (y {a})) () (+ x y) ())"), format!("(lambda ((x {a})) {a} x)"), format!("(lambda ((x {a})) () (let ((y (as {a} 0))) (setf y x) ()))")] {
            s.check_one(&form).unwrap_or_else(|e| panic!("{}: {}", form, e));
        }
        for b in types.iter().filter(|b| **b != a) {
            for op in ["+", "-", "*", "/", "mod", "rem", "<", "<=", ">", ">=", "=", "/=", "max", "min", "logand", "logior", "logxor"] {
                if a.starts_with('f') && op.starts_with("log") {
                    continue;
                }
                if s.check_one(&format!("(lambda ((x {}) (y {})) () ({} x y) ())", a, b, op)).is_ok() {
                    met.push(format!("({} {} {})", op, a, b));
                }
            }
            for form in [format!("(lambda ((x {})) {} x)", a, b), format!("(lambda ((x {})) () (let ((y (as {} 0))) (setf y x) ()))", a, b)] {
                if s.check_one(&form).is_ok() {
                    met.push(form);
                }
            }
        }
    }
    assert!(met.is_empty(), "accepted: {}", met.join("\n"));
}

#[test]
fn what_size_of_takes() {
    assert_eq!(eval_ok("(size-of string)"), Value::Int(std::mem::size_of::<usize>() as i64));
    assert_eq!(eval_ok("(unsafe (def-c-struct pair (a u8) (b u32)))\n(size-of pair)"), Value::Int(8));
    assert_eq!(eval_ok("(size-of (ptr u8))"), Value::Int(std::mem::size_of::<usize>() as i64));
    assert_eq!(eval_ok("(size-of Vector<u8>)"), Value::Int(std::mem::size_of::<usize>() as i64));
    for (form, message) in [
        ("(size-of)", "size-of: (size-of Type)"),
        ("(size-of-val)", "size-of-val: (size-of-val expr)"),
        ("(size-of no-such-type)", "unknown type"),
        ("(size-of-val (panic \"no\"))", "`!` has no value"),
    ] {
        let err = check_err(form);
        assert!(err.contains(message), "{}: {}", form, err);
    }
}

/// The expression is checked, not run.
#[test]
fn size_of_val_does_not_evaluate() {
    assert_eq!(eval_ok("(defvar (n int) 0)\n(size-of-val (progn (setf n 1) (the u16 5)))\nn"), Value::Int(0));
}
