//! `typl -c --target thumbv6m-none-eabi`: an ELF executable for a Cortex-M0+
//! board, run under QEMU and compared with what the native executable does,
//! and the programs it refuses at compile time.
//!
//! The core has no compare-and-swap, no hardware divide and no
//! floating-point unit, 32 interrupts, and of the exceptions only
//! NonMaskableInt, SVCall, PendSV and SysTick. Its programs run on QEMU's
//! `mps2-an385` (a Cortex-M3, which runs every ARMv6-M instruction as an M0+
//! does); QEMU's own Cortex-M0 board (`microbit`) has too little RAM for the
//! runtime, and runs here a program that does without it, to show the code
//! is the core's.
//!
//! Needs the runtime for a board built (`scripts/build-embedded-runtime.sh`,
//! which the test scripts run) and `qemu-system-arm` on `PATH`.

mod common;

use std::process::{Command, Stdio};

use common::board::{build, built, refused_on, run_on, same_as_native_on, scratch, target_flags, text};

const TARGET: &str = "thumbv6m-none-eabi";

#[test]
fn programs_run_as_natively() {
    let (out, _, code) = same_as_native_on(
        TARGET,
        "m0_programs",
        r#"
(use std::sync::Chan)
(defun fact ((n int)) int (if (= n 0) 1 (* n (fact (- n 1)))))
(defun fill ((ch Chan<int>)) () (send ch 7) ())
(defun main () int
  (println "~a ~a ~a ~a" (fact 30) (* 1.5 2.25) (/ 7 3) (/ (as i32 -91) (as i32 7)))
  (let ((ch (the Chan<int> (Chan::new 0))))
    (task (fill ch))
    (println "~a" (unwrap (recv ch))))
  3)
"#,
        &[],
    );
    assert_eq!(out, "265252859812191058636308480000000 3.375 7/3 -13\n7\n");
    assert_eq!(code, Some(3));
}

/// A word is 32 bits there, and the answers are the same.
#[test]
fn a_narrow_word_answers_as_a_wide_one() {
    for level in ["-O0", "-O2"] {
        let (out, _, code) = same_as_native_on(TARGET, "m0_word_width", include_str!("programs/word_width.typl"), &[level]);
        assert!(out.starts_with("1073741823\n1073741824\n"), "{}", out);
        assert_eq!(code, Some(0));
    }
}

#[test]
fn a_panic_ends_the_program_as_natively() {
    let (out, err, code) = same_as_native_on(TARGET, "m0_panic", r#"(defun main () () (println "a") (panic "boom"))"#, &[]);
    assert_eq!(out, "a\n");
    assert!(err.contains("boom"), "{}", err);
    assert_eq!(code, Some(1));
}

/// An interrupt the program raises itself, by setting it pending in the
/// NVIC (`0xE000E200`, a bit per interrupt).
#[test]
fn the_handler_of_an_interrupt_runs() {
    let elf = built(
        "m0_interrupt",
        r#"
(use std::board::enable-interrupt)
(defvar (*count* int) 0)

#[interrupt(5)]
(defun on-five () ()
  (setf *count* (+ *count* 1))
  (println "handled ~a" *count*))

(defun main () int
  (enable-interrupt 5)
  (println "before")
  (unsafe (volatile-write-u32 #xE000E200 (as u32 32)))
  (println "after ~a" *count*)
  *count*)
"#,
        "prog.elf",
        &target_flags(TARGET, &[]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "before\nhandled 1\nafter 1\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}

/// SysTick, which the core has, breaks into a program that makes cons cells
/// and collects all the while; the handler allocates too.
#[test]
fn a_timer_s_handler_runs_while_the_program_allocates() {
    let elf = built(
        "m0_systick",
        r#"
(defvar (*ticks* int) 0)
(defvar (*seen* Vector<int>) (Vector::new))

#[exception(SysTick)]
(defun on-tick () ()
  (setf *ticks* (+ *ticks* 1))
  (when (< (len *seen*) 5)
    (push *seen* *ticks*)))

(defun build ((n int) (acc Option<Sexpr>)) Option<Sexpr>
  (if (= n 0) acc (build (- n 1) (Some (Sexpr::Cons (Sexpr::Int n) acc)))))

(defun main () ()
  (unsafe
    (volatile-write-u32 #xE000E014 (as u32 25000))
    (volatile-write-u32 #xE000E018 (as u32 0))
    (volatile-write-u32 #xE000E010 (as u32 7)))
  (let ((built 0))
    (loop
      (when (is-some (build 2000 ())) (setf built (+ built 1)))
      (when (>= *ticks* 20) (break)))
    (unsafe (volatile-write-u32 #xE000E010 (as u32 0)))
    (println "~a ~a" *seen* (> built 0))))
"#,
        "prog.elf",
        &target_flags(TARGET, &[]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "#(1 2 3 4 5) true\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
}

/// The interrupts past the NVIC's 32, and the faults the core has not got.
#[test]
fn what_the_core_has_not_got_is_refused() {
    let err = refused_on(TARGET, "m0_interrupt_32", "#[interrupt(32)]\n(defun h () () (println \"x\"))\n(defun main () () (println \"hi\"))\n");
    assert!(err.contains("handles interrupt 32: the interrupts of --target thumbv6m-none-eabi are 0 to 31"), "{}", err);
    let err = refused_on(TARGET, "m0_bus_fault", "#[exception(BusFault)]\n(defun h () () (println \"x\"))\n(defun main () () (println \"hi\"))\n");
    assert!(
        err.contains("handles the exception BusFault, which --target thumbv6m-none-eabi has not got (its exceptions are NonMaskableInt, SVCall, PendSV, SysTick)"),
        "{}",
        err
    );
    let (out, elf) = build("m0_enable_32", "(use std::board::enable-interrupt)\n(defun main () () (enable-interrupt 32))\n", "prog.elf", &target_flags(TARGET, &[]));
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = run_on(TARGET, &elf);
    assert!(text(&out.stderr).contains("interrupt 32 does not exist: interrupts are numbered from 0 to 31"), "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}

/// The board's functions given as C functions (`--board-functions c`,
/// `--link`), compiled for the core.
#[test]
fn the_board_gives_its_functions_as_c_functions() {
    let dir = scratch("m0_c_functions");
    let ir = common::board::cortex_m_board_functions_ir("thumbv6m-unknown-none-eabi");
    let object = common::board::board_object(&dir, &ir, &["-mcpu=cortex-m0plus"]);
    let elf = built(
        "m0_c_functions",
        r#"(defun main () int (println "through the UART ~a" 7) (sleep 0.5) 4)"#,
        "prog.elf",
        &target_flags(TARGET, &["--board-functions", "c", "--link", object.to_str().unwrap()]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "through the UART 7\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(4));
}

/// On QEMU's Cortex-M0 itself (`microbit`), a program that does without the
/// runtime and writes to the board's UART (the nRF51's: enabled at
/// `0x40002500`, started at `0x40002008`, the byte at `0x4000251C`): what the
/// core runs is the core's own instructions. Without the runtime means no
/// loop, whose safepoint is the runtime's, no division by a variable, whose
/// check for zero is, and no `int`, which may be a bignum. It ends over
/// semihosting.
#[test]
fn a_cortex_m0_runs_the_code() {
    let dir = scratch("m0_microbit");
    let memory_x = dir.join("memory.x");
    std::fs::write(&memory_x, "MEMORY\n{\n  FLASH : ORIGIN = 0, LENGTH = 256K\n  RAM : ORIGIN = 0x20000000, LENGTH = 16K\n}\n_typelisp_stack_size = 4K;\n").unwrap();
    let elf = built(
        "m0_microbit",
        r#"
(defun put ((c u32)) ()
  (unsafe (volatile-write-u32 #x4000251C c)))

(defun digit ((n u32) (d u32)) ()
  (put (+ (as u32 48) (rem d (as u32 10)))))

(defun main () ()
  (unsafe (volatile-write-u32 #x40002500 (as u32 4)) (volatile-write-u32 #x40002008 (as u32 1)))
  (let ((n (* (as u32 12345) (as u32 6789))))
    (digit n (/ n (as u32 10000000)))
    (digit n (/ n (as u32 1000000)))
    (digit n (/ n (as u32 100000)))
    (digit n (/ n (as u32 10000)))
    (digit n (/ n (as u32 1000)))
    (digit n (/ n (as u32 100)))
    (digit n (/ n (as u32 10)))
    (digit n n)
    (put (as u32 10))))
"#,
        "prog.elf",
        &target_flags(TARGET, &["--release", "-O2", "--linker-script", memory_x.to_str().unwrap()]),
    );
    let symbols = Command::new(std::path::Path::new(&std::env::var("LLVM_SYS_221_PREFIX").unwrap()).join("bin").join("llvm-nm"))
        .arg(&elf)
        .output()
        .expect("failed to start llvm-nm");
    assert!(!text(&symbols.stdout).contains("typelisp_board_start"), "the program uses the runtime");
    let out = Command::new("qemu-system-arm")
        .args(["-M", "microbit", "-nographic", "-semihosting-config", "enable=on,target=native", "-kernel"])
        .arg(&elf)
        .stdin(Stdio::null())
        .output()
        .expect("failed to start qemu-system-arm");
    assert_eq!(text(&out.stdout), "83810205\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
}

/// `--ram`: the board's RAM is 24 KiB, at the default layout's address. The
/// runtime starts its tables and its arena as small as that asks, and a
/// program that prints and collects runs in it. Not 16 KiB (a micro:bit's):
/// the tests link the debug build of the runtime, which takes the program's
/// stack past the 4 KiB that leaves it (the release build takes under
/// 1.5 KiB).
#[test]
fn a_board_with_little_ram_runs_a_program() {
    let elf = built(
        "m0_ram_24k",
        r#"
(defun build ((n int) (acc Option<Sexpr>)) Option<Sexpr>
  (if (= n 0) acc (build (- n 1) (Some (Sexpr::Cons (Sexpr::Int n) acc)))))
(defun main () int
  (let ((k 0))
    (dotimes (i 50) (when (is-some (build 10 ())) (setf k (+ k 1))))
    (println "hello ~a" k))
  3)
"#,
        "prog.elf",
        &target_flags(TARGET, &["--ram", "24K"]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "hello 50\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(3));
}

/// `--ram` is the size of the default layout's RAM: a linker script says its
/// own, and a target that is not a board has no layout; nor has it a stack
/// the program checks (`--no-stack-check`).
#[test]
fn ram_is_for_the_default_layout_of_a_board() {
    let (out, _) = build("m0_ram_and_script", "(defun main () int 0)", "prog.elf", &target_flags(TARGET, &["--ram", "16K", "--linker-script", "memory.x"]));
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("--ram: the linker script given with --linker-script says how much RAM the board has"), "{}", text(&out.stderr));
    let (out, _) = build("m0_ram_host", "(defun main () int 0)", "prog", &["--ram", "16K"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("--ram: only for a board"), "{}", text(&out.stderr));
    let (out, _) = build("m0_ram_bad", "(defun main () int 0)", "prog.elf", &target_flags(TARGET, &["--ram", "lots"]));
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("--ram: `lots` is not a positive size"), "{}", text(&out.stderr));
    let (out, _) = build("m0_stack_check_host", "(defun main () int 0)", "prog", &["--no-stack-check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("--no-stack-check: only for a board"), "{}", text(&out.stderr));
}

/// A stack smaller than the program needs: 1 KiB, in a layout of the
/// user's. A recursion would not do: its frames are on the heap, and what
/// takes the stack deep is the runtime's own work. The check each function
/// starts with ends the program saying so. Without it (`--no-stack-check`,
/// which leaves the check out of the executable) the stack runs on into the
/// heap; the program ends as it would, and then the guard words at the
/// stack's bottom say what happened, and the exit code is a panic's.
#[test]
fn a_stack_too_small_for_the_program_is_found() {
    let dir = scratch("m0_small_stack");
    let memory_x = dir.join("memory.x");
    std::fs::write(&memory_x, "MEMORY\n{\n  FLASH : ORIGIN = 0, LENGTH = 4M\n  RAM : ORIGIN = 0x20000000, LENGTH = 32K\n}\n_typelisp_stack_size = 1K;\n").unwrap();
    let source = "(defun main () int (println \"hello\") 0)";
    let script = memory_x.to_str().unwrap();
    let elf = built("m0_stack_checked", source, "prog.elf", &target_flags(TARGET, &["--linker-script", script]));
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "");
    assert!(text(&out.stderr).contains("stack overflow: the program needs more than the 0x00000400 bytes of the stack (_typelisp_stack_size)"), "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));

    let elf = built("m0_stack_unchecked", source, "prog.elf", &target_flags(TARGET, &["--linker-script", script, "--no-stack-check"]));
    let symbols = Command::new(std::path::Path::new(&std::env::var("LLVM_SYS_221_PREFIX").unwrap()).join("bin").join("llvm-nm"))
        .arg(&elf)
        .output()
        .expect("failed to start llvm-nm");
    assert!(!text(&symbols.stdout).contains("typelisp_stack_check"), "the stack is checked");
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "hello\n");
    assert!(text(&out.stderr).contains("stack overflow: the stack has run past its bottom into the heap"), "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}
