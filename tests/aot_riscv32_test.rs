//! `typl -c --target riscv32imac-unknown-none-elf`: an ELF executable for a
//! board with a 32-bit RISC-V core, run under QEMU (`qemu-system-riscv32 -M
//! virt -bios none`, the host functions over semihosting) and compared with
//! what the native executable does, and the programs it refuses at compile
//! time.
//!
//! A RISC-V board shares everything with a Cortex-M one
//! (`tests/aot_embedded_test.rs`) but its core: the code generated for it,
//! how the program starts and how an interrupt arrives. What is tested here
//! is what goes through those.
//!
//! Needs the runtime for a board built (`scripts/build-embedded-runtime.sh`,
//! which the test scripts run) and `qemu-system-riscv32` on `PATH`.

mod common;

use std::path::{Path, PathBuf};

use common::board::{build, built, refused_on, run_on, same_as_native_on, scratch, target_flags, text};

const TARGET: &str = "riscv32imac-unknown-none-elf";

/// The core-local interruptor of QEMU's `virt` board: a write of 1 to `MSIP`
/// sets the machine's software interrupt (3) pending, a write of 0 clears
/// it; the timer interrupt (7) comes while `MTIME` is at least `MTIMECMP`.
const MSIP: &str = "#x2000000";

#[test]
fn programs_run_as_natively() {
    let (out, _, code) = same_as_native_on(
        TARGET,
        "riscv_programs",
        r#"
(use std::sync::Chan)
(defun fact ((n int)) int (if (= n 0) 1 (* n (fact (- n 1)))))
(defun fill ((ch Chan<int>)) () (send ch 7) ())
(defun main () int
  (println "hello")
  (println "~a ~a ~a" (fact 30) (* 1.5 2.25) (/ 7 3))
  (let ((ch (the Chan<int> (Chan::new 0))))
    (task (fill ch))
    (println "~a" (unwrap (recv ch))))
  3)
"#,
        &[],
    );
    assert_eq!(out, "hello\n265252859812191058636308480000000 3.375 7/3\n7\n");
    assert_eq!(code, Some(3));
}

/// What `machine-type` names on the board.
#[test]
fn the_machine_is_a_riscv32() {
    let elf = built("riscv_machine", r#"(defun main () () (println "~a" (machine-type)))"#, "prog.elf", &target_flags(TARGET, &[]));
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "riscv32\n", "stderr: {}", text(&out.stderr));
}

/// A word is 32 bits there, and the answers are the same.
#[test]
fn a_narrow_word_answers_as_a_wide_one() {
    for level in ["-O0", "-O2"] {
        let (out, _, code) = same_as_native_on(TARGET, "riscv_word_width", include_str!("programs/word_width.typl"), &[level]);
        assert!(out.starts_with("1073741823\n1073741824\n"), "{}", out);
        assert_eq!(code, Some(0));
    }
}

#[test]
fn a_panic_ends_the_program_as_natively() {
    let (out, err, code) = same_as_native_on(TARGET, "riscv_panic", r#"(defun main () () (println "a") (panic "boom"))"#, &[]);
    assert_eq!(out, "a\n");
    assert!(err.contains("boom"), "{}", err);
    assert_eq!(code, Some(1));
}

/// The machine's software interrupt, which the program raises itself: its
/// handler runs before the program goes on, and the program's state is the
/// handler's too.
#[test]
fn the_handler_of_an_interrupt_runs() {
    let elf = built(
        "riscv_interrupt",
        &format!(
            r#"
(use std::board::enable-interrupt)
(defvar (*count* int) 0)

#[interrupt(3)]
(defun on-software () ()
  (unsafe (volatile-write-u32 {msip} (as u32 0)))
  (setf *count* (+ *count* 1))
  (println "handled ~a" *count*))

(defun main () int
  (enable-interrupt 3)
  (println "before")
  (unsafe (volatile-write-u32 {msip} (as u32 1)))
  (println "after ~a" *count*)
  (unsafe (volatile-write-u32 {msip} (as u32 1)))
  (println "end ~a" *count*)
  *count*)
"#,
            msip = MSIP
        ),
        "prog.elf",
        &target_flags(TARGET, &[]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "before\nhandled 1\nafter 1\nhandled 2\nend 2\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(2));
}

/// The machine's timer, every 10000 ticks, breaks into a program that makes
/// cons cells and collects all the while; the handler allocates too.
#[test]
fn a_timer_s_handler_runs_while_the_program_allocates() {
    let elf = built(
        "riscv_timer",
        r#"
(use std::board::enable-interrupt)
(defvar (*ticks* int) 0)
(defvar (*seen* Vector<int>) (Vector::new))

(defun later () ()
  (unsafe
    (let ((now (volatile-read-u32 #x200BFF8)))
      (volatile-write-u32 #x2004004 (as u32 0))
      (volatile-write-u32 #x2004000 (+ now (as u32 10000))))))

#[interrupt(7)]
(defun on-timer () ()
  (setf *ticks* (+ *ticks* 1))
  (when (< (len *seen*) 5)
    (push *seen* *ticks*))
  (later))

(defun build ((n int) (acc Option<Sexpr>)) Option<Sexpr>
  (if (= n 0) acc (build (- n 1) (Some (Sexpr::Cons (Sexpr::Int n) acc)))))

(defun main () ()
  (later)
  (enable-interrupt 7)
  (let ((built 0))
    (loop
      (when (is-some (build 2000 ())) (setf built (+ built 1)))
      (when (>= *ticks* 20) (break)))
    (println "~a ~a" *seen* (> built 0))))
"#,
        "prog.elf",
        &target_flags(TARGET, &[]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "#(1 2 3 4 5) true\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
}

/// An interrupt `mie` has no bit for, and a Cortex-M's exceptions, which the
/// core has not got.
#[test]
fn what_the_core_has_not_got_is_refused() {
    let err = refused_on(TARGET, "riscv_interrupt_32", "#[interrupt(32)]\n(defun h () () (println \"x\"))\n(defun main () () (println \"hi\"))\n");
    assert!(err.contains("handles interrupt 32: the interrupts of --target riscv32imac-unknown-none-elf are 0 to 31"), "{}", err);
    let err = refused_on(TARGET, "riscv_exception", "#[exception(SysTick)]\n(defun h () () (println \"x\"))\n(defun main () () (println \"hi\"))\n");
    assert!(err.contains("handles a core exception, and --target riscv32imac-unknown-none-elf has none"), "{}", err);
    let (out, elf) = build("riscv_enable_32", "(use std::board::enable-interrupt)\n(defun main () () (enable-interrupt 32))\n", "prog.elf", &target_flags(TARGET, &[]));
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = run_on(TARGET, &elf);
    assert!(text(&out.stderr).contains("interrupt 32 does not exist: interrupts are numbered from 0 to 31"), "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}

/// What a board has not got is refused for this one as for any.
#[test]
fn a_file_is_refused_at_compile_time() {
    let err = refused_on(
        TARGET,
        "riscv_refuse_file",
        r#"
(use std::fs::write-file-string)
(defun main () () (println "~a" (write-file-string "out.txt" "x")))
"#,
    );
    assert!(err.contains("--target riscv32imac-unknown-none-elf: the program uses what the target does not have:"), "{}", err);
    assert!(err.contains("opening a file (reached by main -> prog::main -> std::fs::write-file-string"), "{}", err);
}

/// The program drives the board's UART itself through its registers (a
/// 16550 at `0x10000000`: the byte to send at `+0`, the line status at `+5`,
/// whose bit 5 is "ready to take a byte"), which `-nographic` shows on
/// standard output.
#[test]
fn the_program_drives_a_register_with_volatile_reads_and_writes() {
    let elf = built(
        "riscv_uart",
        r#"
(defun put ((c u8)) ()
  (unsafe
    (loop (when (/= (logand (volatile-read-u8 #x10000005) (as u8 32)) (as u8 0)) (break)))
    (volatile-write-u8 #x10000000 c)))

(defun main () int
  (put (as u8 79))
  (put (as u8 75))
  (put (as u8 10))
  6)
"#,
        "prog.elf",
        &target_flags(TARGET, &[]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "OK\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(6));
}

/// The board's functions (`typelisp_host_*`) as a board gives them, written
/// in LLVM IR for want of a C compiler for the board in every environment
/// the tests run in: output to the board's UART (the 16550 at
/// `0x10000000`), a clock that advances by itself, and the end of the
/// program through semihosting, for the exit code to reach the test.
const BOARD_FUNCTIONS_IR: &str = r#"
target triple = "riscv32-unknown-none-elf"

@clock = internal global i64 0

define void @typelisp_host_write(i32 %fd, ptr %p, i32 %len) {
entry:
  br label %next
next:
  %i = phi i32 [0, %entry], [%i1, %send]
  %done = icmp eq i32 %i, %len
  br i1 %done, label %out, label %wait
wait:
  %state = load volatile i8, ptr inttoptr (i32 268435461 to ptr)
  %ready = and i8 %state, 32
  %busy = icmp eq i8 %ready, 0
  br i1 %busy, label %wait, label %send
send:
  %a = getelementptr i8, ptr %p, i32 %i
  %c = load i8, ptr %a
  store volatile i8 %c, ptr inttoptr (i32 268435456 to ptr)
  %i1 = add i32 %i, 1
  br label %next
out:
  ret void
}

define i32 @typelisp_host_read(i32 %fd, ptr %p, i32 %len) {
  ret i32 0
}

define void @typelisp_host_exit(i32 %code) noreturn {
  %block = alloca [2 x i32]
  store i32 131110, ptr %block
  %second = getelementptr i32, ptr %block, i32 1
  store i32 %code, ptr %second
  call void asm sideeffect "li a0, 0x20\0A.balign 16\0A.option push\0A.option norvc\0Aslli x0, x0, 0x1f\0Aebreak\0Asrai x0, x0, 0x7\0A.option pop", "{a1},~{a0},~{memory}"(ptr %block)
  br label %stay
stay:
  br label %stay
}

define i64 @typelisp_host_now_us() {
  %t = load i64, ptr @clock
  %t1 = add i64 %t, 1000
  store i64 %t1, ptr @clock
  ret i64 %t1
}

define i64 @typelisp_host_unix_us() {
  %t = call i64 @typelisp_host_now_us()
  ret i64 %t
}

define void @typelisp_host_sleep_us(i64 %us) {
  %t = load i64, ptr @clock
  %t1 = add i64 %t, %us
  store i64 %t1, ptr @clock
  ret void
}
"#;

/// The RISC-V object the LLVM IR `ir` compiles to.
fn board_object(dir: &Path, ir_text: &str) -> PathBuf {
    common::board::board_object(dir, ir_text, &["-mattr=+m,+a,+c"])
}

/// The board's functions given as C functions (`--board-functions c`,
/// `--link`) instead of over semihosting: the program's output goes out
/// through them.
#[test]
fn the_board_gives_its_functions_as_c_functions() {
    let dir = scratch("riscv_c_functions");
    let object = board_object(&dir, BOARD_FUNCTIONS_IR);
    let elf = built(
        "riscv_c_functions",
        r#"(defun main () int (println "through the UART ~a" 7) (sleep 0.5) 4)"#,
        "prog.elf",
        &target_flags(TARGET, &["--board-functions", "c", "--link", object.to_str().unwrap()]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "through the UART 7\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(4));
}

/// C functions as the board's C compiler sees them: a `long` is 32 bits,
/// and `struct rec { long n; void *p; int x; }` has `x` at offset 8.
const ILP32_C_FUNCTIONS_IR: &str = r#"
target triple = "riscv32-unknown-none-elf"

define i32 @negate_long(i32 %n) {
  %r = sub i32 0, %n
  ret i32 %r
}

define i32 @rec_x(ptr %r) {
  %at = getelementptr i8, ptr %r, i32 8
  %x = load i32, ptr %at
  ret i32 %x
}
"#;

/// A `c-long` is as wide as the board's C `long`, and a `def-c-struct` is
/// laid out as the board's C lays it out.
#[test]
fn c_long_and_c_structs_follow_the_board_s_c() {
    let dir = scratch("riscv_ilp32");
    let object = board_object(&dir, ILP32_C_FUNCTIONS_IR);
    let elf = built(
        "riscv_ilp32",
        r#"
(defffi (negate-long "negate_long") (c-long) c-long)
(unsafe (def-c-struct rec (n c-long) (p ptr) (x i32)))
(defffi (rec-x "rec_x") ((ptr rec)) i32)
(defun main () int
  (unsafe
    (let ((r (c-alloc rec)))
      (setf r::x (as i32 30))
      (+ (as int (rec-x r)) (as int (negate-long (as c-long -12)))))))
"#,
        "prog.elf",
        &target_flags(TARGET, &["--link", object.to_str().unwrap()]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(out.status.code(), Some(42), "stdout: {} stderr: {}", text(&out.stdout), text(&out.stderr));
}

/// A program that drives a register and calls nothing of the runtime is its
/// own code and the startup's: no heap, no printer.
#[test]
fn a_program_without_the_runtime_is_small() {
    let elf = built(
        "riscv_small",
        "(defun main () int (unsafe (volatile-write-u32 #x2000000 (as u32 0))) 42)",
        "prog.elf",
        &target_flags(TARGET, &["--release", "-O2"]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(out.status.code(), Some(42), "stderr: {}", text(&out.stderr));
    let size = std::fs::metadata(&elf).unwrap().len();
    let symbols = std::process::Command::new(std::path::Path::new(&std::env::var("LLVM_SYS_221_PREFIX").unwrap()).join("bin").join("llvm-nm"))
        .arg(&elf)
        .output()
        .expect("failed to start llvm-nm");
    let symbols = text(&symbols.stdout);
    assert!(!symbols.contains("typelisp_board_start"), "the heap is readied: {} bytes", size);
}

/// The user's memory layout (`--linker-script`): code and the start values
/// of the data in one region, and the data, the heap and the stack in
/// another, as on a board with flash. Both are in the RAM of QEMU's board,
/// which the start copies `.data` across.
#[test]
fn the_user_s_memory_layout_is_used() {
    let dir = scratch("riscv_memory_x");
    let memory_x = dir.join("memory.x");
    std::fs::write(
        &memory_x,
        r#"MEMORY
{
  FLASH : ORIGIN = 0x80000000, LENGTH = 4M
  RAM : ORIGIN = 0x80400000, LENGTH = 2M
}
REGION_ALIAS("REGION_TEXT", FLASH);
REGION_ALIAS("REGION_RODATA", FLASH);
REGION_ALIAS("REGION_DATA", RAM);
REGION_ALIAS("REGION_BSS", RAM);
REGION_ALIAS("REGION_HEAP", RAM);
REGION_ALIAS("REGION_STACK", RAM);
_typelisp_stack_size = 256K;
"#,
    )
    .unwrap();
    let elf = built(
        "riscv_memory_x",
        r#"
(defvar (*greeting* string) "from the data")
(defun fact ((n int)) int (if (= n 0) 1 (* n (fact (- n 1)))))
(defun main () int (println "~a ~a" *greeting* (fact 20)) 9)
"#,
        "prog.elf",
        &target_flags(TARGET, &["--linker-script", memory_x.to_str().unwrap()]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "from the data 2432902008176640000\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(9));
}

/// `--ram` for RISC-V: 64 KiB of the board's RAM, code and data alike.
#[test]
fn a_board_with_little_ram_runs_a_program() {
    let elf = built(
        "riscv_ram_64k",
        r#"
(defun fact ((n int)) int (if (= n 0) 1 (* n (fact (- n 1)))))
(defun main () int (println "~a" (fact 20)) 5)
"#,
        "prog.elf",
        &target_flags(TARGET, &["--ram", "64K"]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "2432902008176640000\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(5));
}

/// A stack smaller than the program needs (1 KiB, in a layout of the
/// user's) ends the program before the stack reaches the heap, and without
/// the check (`--no-stack-check`) the guard words at its bottom say at the
/// end that it ran into it.
#[test]
fn a_stack_too_small_for_the_program_is_found() {
    let dir = scratch("riscv_small_stack");
    let memory_x = dir.join("memory.x");
    std::fs::write(
        &memory_x,
        "MEMORY\n{\n  FLASH : ORIGIN = 0x80000000, LENGTH = 1M\n  RAM : ORIGIN = 0x80100000, LENGTH = 32K\n}\n\
         REGION_ALIAS(\"REGION_TEXT\", FLASH);\nREGION_ALIAS(\"REGION_RODATA\", FLASH);\nREGION_ALIAS(\"REGION_DATA\", RAM);\n\
         REGION_ALIAS(\"REGION_BSS\", RAM);\nREGION_ALIAS(\"REGION_HEAP\", RAM);\nREGION_ALIAS(\"REGION_STACK\", RAM);\n\
         _typelisp_stack_size = 1K;\n",
    )
    .unwrap();
    let source = "(defun main () int (println \"hello\") 0)";
    let script = memory_x.to_str().unwrap();
    let elf = built("riscv_stack_checked", source, "prog.elf", &target_flags(TARGET, &["--linker-script", script]));
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "");
    assert!(text(&out.stderr).contains("stack overflow: the program needs more than the 0x00000400 bytes of the stack (_typelisp_stack_size)"), "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));

    let elf = built("riscv_stack_unchecked", source, "prog.elf", &target_flags(TARGET, &["--linker-script", script, "--no-stack-check"]));
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "hello\n");
    assert!(text(&out.stderr).contains("stack overflow: the stack has run past its bottom into the heap"), "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}

/// A board's calls in progress bounded by `--frame-memory`: a recursion
/// past it ends the program, saying so, instead of taking the board's heap
/// until an allocation fails.
#[test]
fn the_frame_memory_bounds_a_recursion() {
    let elf = built(
        "riscv_frame_memory",
        r#"
(defun depth ((n int)) int (if (= n 0) 0 (+ 1 (depth (- n 1)))))
(defun main () int (println "~a" (depth 200)) (println "~a" (depth 100000)) 0)
"#,
        "prog.elf",
        &target_flags(TARGET, &["--ram", "256K", "--frame-memory", "32K"]),
    );
    let out = run_on(TARGET, &elf);
    assert_eq!(text(&out.stdout), "200\n");
    assert!(text(&out.stderr).contains("the calls in progress take more than the 32768 bytes --frame-memory allows a task ("), "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}
