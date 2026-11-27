//! A program that makes strings, boxes and closures and no cons cells runs in
//! bounded memory: they are collected at the program's safepoints
//! (`typelisp_mem::Heap::safepoint`) — interpreted, compiled, in an
//! executable, and on a board. And nothing that is still in use is: the same
//! programs under `gc_stress`, where every safepoint collects.
//!
//! Needs what `aot_embedded_test` needs.

extern crate typelisp;
mod common;
#[path = "common/garbage.rs"]
mod garbage;

use std::path::PathBuf;
use std::process::{Command, Output};
use common::board::on_board;
use common::{Load, Session};
use typelisp::Value;

const TYPL: &str = env!("CARGO_BIN_EXE_typl");

fn built(name: &str, source: &str, flags: &[&str]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    let (src, out) = (dir.join("prog.typl"), dir.join("prog.elf"));
    std::fs::write(&src, source).unwrap();
    let made = Command::new(TYPL).args(["-c", src.to_str().unwrap(), "-o", out.to_str().unwrap()]).args(flags).output().expect("failed to start typl");
    assert!(made.status.success(), "{}: {}", name, String::from_utf8_lossy(&made.stderr));
    out
}

fn program(defs: &str, call: &str, n: u32) -> String {
    format!("{}\n(defun main () int {})\n", defs, call.replace('N', &n.to_string()))
}

fn succeeded(name: &str, out: Output) {
    assert_eq!(out.status.code(), Some(0), "{}: {}", name, String::from_utf8_lossy(&out.stderr));
}

#[test]
fn an_executable_s_memory_is_bounded() {
    for (name, defs, call) in garbage::PROGRAMS {
        for level in ["-O0", "-O2"] {
            let exe = built(&format!("garbage-{}{}", name, level), &program(defs, call, 400_000), &[level, "--max-memory", "24M"]);
            succeeded(name, Command::new(exe).output().expect("failed to run the executable"));
        }
    }
}

#[test]
fn a_board_s_memory_is_enough() {
    for (name, defs, call) in garbage::PROGRAMS {
        for triple in ["thumbv7em-none-eabihf", "riscv32imac-unknown-none-elf"] {
            let elf = built(&format!("garbage-board-{}-{}", triple, name), &program(defs, call, 100_000), &["--target", triple, "-O2"]);
            succeeded(name, on_board(triple, &elf).output().expect("failed to start QEMU"));
        }
    }
}

/// How many collections `call`'s rounds took in a session, and its answer.
fn run(defs: &str, call: &str, n: u32, compiled: bool, stress: bool) -> (u64, Value) {
    let mut s = Session::new(Load::Compiler);
    s.eval(defs).expect("the definitions are right");
    if compiled {
        for line in defs.lines() {
            if let Some(rest) = line.strip_prefix("(defun ") {
                s.eval(&format!("(compile {})", rest.split(' ').next().unwrap())).expect("it compiles");
            }
        }
    }
    s.heap.set_gc_stress(stress);
    let before = s.heap.gc_count();
    let v = s.eval(&call.replace('N', &n.to_string())).expect("the rounds run");
    s.heap.set_gc_stress(false);
    (s.heap.gc_count() - before, v)
}

#[test]
fn compiled_code_collects_in_a_session() {
    for (name, defs, call) in garbage::PROGRAMS.iter().filter(|p| ["vectors", "closures"].contains(&p.0)) {
        let (collections, v) = run(defs, call, 2_000_000, true, false);
        assert_eq!(v, Value::Int(0), "{}", name);
        assert!(collections > 0, "{}: nothing was collected", name);
    }
}

#[test]
fn what_is_in_use_stays_when_every_safepoint_collects() {
    for (name, defs, call) in garbage::PROGRAMS {
        for compiled in [false, true] {
            let (_, v) = run(defs, call, 8, compiled, true);
            assert_eq!(v, Value::Int(0), "{} (compiled: {})", name, compiled);
        }
    }
}
