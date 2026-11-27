//! `typl -c --target thumbv7em-none-eabihf`: an ELF executable for a
//! Cortex-M4F board built from the same program a native executable is, run
//! under QEMU (`qemu-system-arm -M mps2-an386`, the host functions over
//! semihosting) and compared with what the native one does, and the
//! programs it refuses at compile time. The same for a Cortex-M33
//! (`thumbv8m.main-none-eabihf`, `-M mps2-an505`), less thoroughly: the
//! two share everything but the core the code is for.
//!
//! Needs the runtime for a board built (`scripts/build-embedded-runtime.sh`,
//! which the test scripts run) and `qemu-system-arm` on `PATH`.
//!
//! The building and running is shared with the other boards' tests
//! (`common::board`).

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::board::{build, built, run_on, same_as_native_on, scratch, target_flags, text};

const TARGET: &str = "thumbv7em-none-eabihf";

/// The Cortex-M33 board.
const M33: &str = "thumbv8m.main-none-eabihf";

/// The flags for the board, then `flags`.
fn board_flags<'a>(flags: &[&'a str]) -> Vec<&'a str> {
    target_flags(TARGET, flags)
}

/// Runs the executable at `elf` on QEMU's Cortex-M4 board, its standard
/// output and error over semihosting.
fn run_board(elf: &Path) -> Output {
    run_on(TARGET, elf)
}

/// What `source` prints and exits with on the board, after checking that
/// the native executable prints and exits the same.
fn same_as_native(name: &str, source: &str, flags: &[&str]) -> (String, String, Option<i32>) {
    same_as_native_on(TARGET, name, source, flags)
}

/// What `typl -c --target thumbv7em-none-eabihf` said when it refused
/// `source`.
fn refused(name: &str, source: &str) -> String {
    common::board::refused_on(TARGET, name, source)
}

#[test]
fn hello_world_prints_and_exits_as_natively() {
    let (out, _, code) = same_as_native("board_hello", r#"(defun main () () (println "hello ~a" 42))"#, &[]);
    assert_eq!(out, "hello 42\n");
    assert_eq!(code, Some(0));
}

/// `main`'s `int` is the exit code there too.
#[test]
fn main_s_answer_is_the_exit_code() {
    let (_, _, code) = same_as_native("board_exit_code", "(defun main () int 3)", &[]);
    assert_eq!(code, Some(3));
}

/// A throw crosses compiled frames to its catch, and an uncaught panic ends
/// the program the native way: both are status words, not Rust unwinding,
/// which a panic on the board aborts.
#[test]
fn a_throw_reaches_its_catch_and_a_panic_ends_the_program() {
    let (out, err, code) = same_as_native(
        "board_unwind",
        r#"
(defun deep ((n int)) int (throw 'done (* n 2)))
(defun middle ((n int)) int (+ 1 (deep n)))
(defun main () ()
  (println "~a" (catch 'done (middle 21)))
  (eprintln "to standard error")
  (panic "boom"))
"#,
        &[],
    );
    assert_eq!(out, "42\n");
    assert!(err.starts_with("to standard error\npanic: "), "stderr: {}", err);
    assert_eq!(code, Some(1));
}

/// Integers past a fixnum, ratios, floats, recursion, and a vector: a word
/// is 64 bits on a 32-bit core too.
#[test]
fn arithmetic_recursion_and_vectors_run_as_natively() {
    let (out, _, _) = same_as_native(
        "board_arith",
        r#"
(defun fact ((n int)) int (if (= n 0) 1 (* n (fact (- n 1)))))
(defun depth ((n int)) int (if (= n 0) 0 (+ 1 (depth (- n 1)))))
(defun main () ()
  (println "~a" (fact 30))
  (println "~a" (/ 1 3))
  (println "~a" (* 1.5 2.25))
  (println "~a" (depth 1000))
  (let ((v (the Vector<int> (Vector::new))))
    (push v 1) (push v 2)
    (println "~a" (get v 1))))
"#,
        &[],
    );
    assert_eq!(out, "265252859812191058636308480000000\n1/3\n3.375\n1000\n2\n");
}

/// Tasks, a channel between them, and `sleep`, on the one core there is.
#[test]
fn tasks_channels_and_sleep_run_on_one_core() {
    let (out, _, _) = same_as_native(
        "board_tasks",
        r#"
(use std::sync::Chan)
(defun fill ((ch Chan<int>)) () (send ch 7) ())
(defun main () ()
  (let ((ch (the Chan<int> (Chan::new 0))))
    (task (fill ch))
    (sleep 0.01)
    (println "~a" (unwrap (recv ch)))))
"#,
        &[],
    );
    assert_eq!(out, "7\n");
}

/// The demangled names of the symbols the executable at `elf` defines, by
/// the `llvm-nm` of the LLVM typl is built with.
fn symbols(elf: &Path) -> Vec<String> {
    let prefix = std::env::var("LLVM_SYS_221_PREFIX").expect("LLVM_SYS_221_PREFIX names the LLVM typl is built with");
    let out = Command::new(Path::new(&prefix).join("bin").join("llvm-nm"))
        .args(["--defined-only", "--demangle", "--format=just-symbols"])
        .arg(elf)
        .output()
        .expect("failed to start llvm-nm");
    assert!(out.status.success(), "llvm-nm failed: {}", text(&out.stderr));
    text(&out.stdout).lines().map(str::to_string).collect()
}

/// The parts of the runtime a program reaches only through what it does: the
/// scheduler (a task, a channel, `sleep`), writing through the printer,
/// arbitrary-precision integers, and the symbol and type tables (a value
/// made with its type, a name written).
const USED_ONLY_WHEN_USED: [&str; 6] = [
    "typelisp_rt::sched::",
    "typelisp_print::format::",
    "typelisp_print::pprint::",
    "num_bigint::",
    "typelisp_mem::symbols::SymTable",
    "typelisp_mem::type_shape::TypeTable",
];

/// Whether the symbol `name` is code of the module `part`: a function of it,
/// or a method of one of its types. A type of it named among the type
/// arguments of somebody else's code (dropping a `BigInt`) is not.
fn of_module(name: &str, part: &str) -> bool {
    name.starts_with(part) || name.strip_prefix('<').is_some_and(|n| n.starts_with(part))
}

/// A program that starts no task, writes nothing and makes no bignum has
/// none of them in its executable — one that only drives a register, as
/// blinking an LED does, included; one that does has them all.
#[test]
fn what_a_program_does_not_use_is_left_out() {
    let blink = r#"
(defun wait-a-bit () ()
  (let ((i (the i32 0)))
    (while (< i (the i32 10))
      (setf i (+ i (the i32 1))))
    ()))
(defun main () int
  (let ((n (the i32 0)))
    (while (< n (the i32 3))
      (unsafe (volatile-write-u32 #x40004000 #x2A))
      (wait-a-bit)
      (setf n (+ n (the i32 1)))))
  42)
"#;
    for (name, source) in [("board_left_out", "(defun main () int 42)"), ("board_left_out_blink", blink)] {
        let elf = built(name, source, "prog.elf", &board_flags(&[]));
        assert_eq!(run_board(&elf).status.code(), Some(42), "{}", name);
        let names = symbols(&elf);
        for part in USED_ONLY_WHEN_USED {
            let found: Vec<&String> = names.iter().filter(|n| of_module(n, part)).collect();
            assert!(found.is_empty(), "{}: {} is linked in: {:?}", name, part, &found[..found.len().min(5)]);
        }
    }
    // With no runtime at all: no heap to make, nothing to drive a coroutine
    // with, no interrupt handlers and no text to write.
    let elf = built("board_bare", blink, "prog.elf", &board_flags(&[]));
    let names = symbols(&elf);
    for part in ["rt_", "typelisp_rt::", "typelisp_mem::heap", "embedded_alloc", "linked_list_allocator", "typelisp_board_start"] {
        let found: Vec<&String> = names.iter().filter(|n| n.starts_with(part) || of_module(n, part)).collect();
        assert!(found.is_empty(), "{} is linked in: {:?}", part, &found[..found.len().min(5)]);
    }
    let elf = built(
        "board_left_in",
        r#"
(use std::sync::Chan)
(defun fill ((ch Chan<int>)) () (send ch (* 4611686018427387903 4)) ())
(defun main () ()
  (let ((ch (the Chan<int> (Chan::new 0))))
    (task (fill ch))
    (println "~a" (unwrap (recv ch)))))
"#,
        "prog.elf",
        &board_flags(&[]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "18446744073709551612\n", "stderr: {}", text(&out.stderr));
    let names = symbols(&elf);
    for part in USED_ONLY_WHEN_USED {
        assert!(names.iter().any(|n| of_module(n, part)), "{} is not linked in", part);
    }
}

/// The runtime's fixed-width functions are one per type, so a program
/// links the ones of the widths it uses and no others: boxing a `u8` into a
/// `Sexpr` brings `rt_u8_box` and not `rt_i16_box`, cutting an `int` to `u8`
/// brings `rt_integer_to_u8` and not `rt_integer_to_u16`. Nothing takes a
/// width as an argument any more.
#[test]
fn a_width_s_runtime_is_linked_only_when_the_width_is_used() {
    let elf = built(
        "board_one_width",
        r#"
(defun cut ((n int)) u8 (as u8 n))
(defun main () ()
  (let ((x (cut 300)))
    (println "~a ~a" (+ x (the u8 100)) (/ x (the u8 3)))))
"#,
        "prog.elf",
        &board_flags(&[]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "144 14\n", "stderr: {}", text(&out.stderr));
    let names = symbols(&elf);
    for used in ["rt_u8_box", "rt_integer_to_u8", "rt_int_div_fault"] {
        assert!(names.iter().any(|n| n == used), "{} is not linked in", used);
    }
    for t in ["i8", "i16", "i32", "u16", "u32"] {
        for shim in [format!("rt_{}_box", t), format!("rt_{}_unbox", t), format!("rt_integer_to_{}", t), format!("rt_integer_fits_{}", t), format!("rt_sexpr_{}", t)] {
            assert!(!names.contains(&shim), "{} is linked in", shim);
        }
    }
}

/// The arena grows from the small one a board starts with, and an
/// executable without one runs what needs none.
#[test]
fn the_cons_arena_grows_and_can_be_left_out() {
    let (out, _, _) = same_as_native(
        "board_conses",
        r#"
(defun build ((n int) (acc Option<Sexpr>)) Option<Sexpr>
  (if (= n 0) acc (build (- n 1) (Some (Sexpr::Cons (Sexpr::Int n) acc)))))
(defun main () () (println "~a" (is-some (build 5000 ()))))
"#,
        &[],
    );
    assert_eq!(out, "true\n");
    let (out, _, code) = same_as_native("board_no_cons", r#"(defun main () () (println "no cells"))"#, &["--no-cons"]);
    assert_eq!(out, "no cells\n");
    assert_eq!(code, Some(0));
}

/// `--max-memory` bounds the board's allocations as it does an executable's.
#[test]
fn an_allocation_past_the_memory_limit_is_a_runtime_error() {
    let elf = built(
        "board_max_memory",
        "(defun main () int (if (= (len (Vector::filled 200000 0)) 200000) 0 2))",
        "prog.elf",
        &board_flags(&["--max-memory", "1M"]),
    );
    let out = run_board(&elf);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("memory limit exceeded"), "stderr: {}", text(&out.stderr));
}

/// A board has no time zone. Decoding at a zone given runs as natively;
/// asking for the local one is a runtime panic.
#[test]
fn a_given_zone_decodes_and_the_local_one_is_unknown() {
    let (out, _, code) = same_as_native(
        "board_given_zone",
        r#"
(use std::time::decode-universal-time)
(defun main () ()
  (println "~a" (decode-universal-time (make-instance 'universal-time :day 46216 :second 43200) -9)))
"#,
        &[],
    );
    assert_eq!(out, "#<decoded-time 2026-07-15 21:00:00 +09:00 Wed>\n");
    assert_eq!(code, Some(0));
    let elf = built(
        "board_local_zone",
        r#"
(use std::time::get-decoded-time)
(defun main () () (println "~a" (get-decoded-time)))
"#,
        "prog.elf",
        &board_flags(&[]),
    );
    let out = run_board(&elf);
    assert!(text(&out.stderr).contains("the local time zone at this universal time is unknown"), "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}

/// A C function is the board's to give, so calling one is not refused: here
/// one of the board's own functions, which ends the program with its code.
#[test]
fn a_c_function_the_board_has_is_called() {
    let elf = built(
        "board_ffi",
        r#"
(defffi (host-exit "typelisp_host_exit") (i32) ())
(defun main () () (println "leaving") (unsafe (host-exit 5)))
"#,
        "prog.elf",
        &board_flags(&[]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "leaving\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(5));
}

/// A Cortex-M33 runs what a Cortex-M4 does: integers past a fixnum, floats,
/// recursion, a task and a channel.
#[test]
fn a_cortex_m33_runs_programs_as_natively() {
    let (out, _, code) = same_as_native_on(
        M33,
        "m33_programs",
        r#"
(use std::sync::Chan)
(defun fact ((n int)) int (if (= n 0) 1 (* n (fact (- n 1)))))
(defun fill ((ch Chan<int>)) () (send ch 7) ())
(defun main () int
  (println "~a ~a" (fact 30) (* 1.5 2.25))
  (let ((ch (the Chan<int> (Chan::new 0))))
    (task (fill ch))
    (println "~a" (unwrap (recv ch))))
  3)
"#,
        &[],
    );
    assert_eq!(out, "265252859812191058636308480000000 3.375\n7\n");
    assert_eq!(code, Some(3));
}

/// A Cortex-M33's interrupts reach their handlers as a Cortex-M4's do.
#[test]
fn a_cortex_m33_handles_an_interrupt() {
    let elf = built(
        "m33_interrupt",
        r#"
(use std::board::enable-interrupt)
#[interrupt(5)]
(defun on-five () () (println "handled"))
(defun main () ()
  (enable-interrupt 5)
  (unsafe (volatile-write-u32 #xE000E200 (as u32 32)))
  (println "after"))
"#,
        "prog.elf",
        &target_flags(M33, &[]),
    );
    let out = run_on(M33, &elf);
    assert_eq!(text(&out.stdout), "handled\nafter\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
}

/// The program drives the board's UART itself through its registers (CMSDK
/// APB UART 0: data at `0x40004000`, state at `+4`, whose bit 0 is "still
/// sending", control at `+8`, whose bit 0 enables sending), which
/// `-nographic` shows on standard output; the control register reads back
/// what was written to it.
#[test]
fn the_program_drives_a_register_with_volatile_reads_and_writes() {
    let elf = built(
        "board_volatile",
        r#"
(defun put-byte ((c u32)) ()
  (unsafe
    (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))
    (volatile-write-u32 #x40004000 c)))

(defun main () int
  (unsafe (volatile-write-u32 #x40004008 1))
  (put-byte 114) (put-byte 101) (put-byte 103) (put-byte 10)
  (unsafe (as int (volatile-read-u32 #x40004008))))
"#,
        "prog.elf",
        &board_flags(&[]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "reg\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}

/// An interrupt the program raises itself, by setting it pending in the
/// NVIC (`0xE000E200`, a bit per interrupt): its handler runs before the
/// program goes on, and the program's state is the handler's too.
#[test]
fn the_handler_of_an_interrupt_runs() {
    let elf = built(
        "board_interrupt",
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
  (unsafe (volatile-write-u32 #xE000E200 (as u32 32)))
  (println "end ~a" *count*)
  *count*)
"#,
        "prog.elf",
        &board_flags(&[]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "before\nhandled 1\nafter 1\nhandled 2\nend 2\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(2));
}

/// The core's timer (SysTick, every millisecond) breaks into a program that
/// makes cons cells and collects all the while; the handler allocates too.
#[test]
fn a_timer_s_handler_runs_while_the_program_allocates() {
    let elf = built(
        "board_systick",
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
        &board_flags(&[]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "#(1 2 3 4 5) true\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
}

/// A panic out of a handler ends the program as an uncaught one does.
#[test]
fn a_panic_in_a_handler_ends_the_program() {
    let elf = built(
        "board_interrupt_panic",
        r#"
(use std::board::enable-interrupt)
#[interrupt(3)]
(defun on-three () () (panic "in the handler"))
(defun main () ()
  (enable-interrupt 3)
  (unsafe (volatile-write-u32 #xE000E200 (as u32 8)))
  (println "not reached"))
"#,
        "prog.elf",
        &board_flags(&[]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "");
    assert!(text(&out.stderr).contains("panic: in the handler"), "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(1));
}

/// A handler cannot wait: it runs on top of the program it stopped.
#[test]
fn a_handler_that_sleeps_is_refused() {
    let e = refused(
        "board_interrupt_sleep",
        r#"
#[interrupt(3)]
(defun on-three () () (sleep 0.1))
(defun main () () (println "hi"))
"#,
    );
    assert!(e.contains("waits, and a handler cannot") && e.contains("`sleep`"), "unexpected error: {}", e);
}

/// Only a board has interrupts.
#[test]
fn interrupts_are_refused_off_a_board() {
    let (out, _) = build("native_interrupt", "#[interrupt(3)]\n(defun on-three () () (println \"x\"))\n(defun main () () (println \"hi\"))\n", "prog", &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("only a board has interrupts"), "stderr: {}", text(&out.stderr));
    let (out, _) = build("native_enable_interrupt", "(use std::board::enable-interrupt)\n(defun main () () (enable-interrupt 3))\n", "prog", &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("only a board has") && text(&out.stderr).contains("`enable-interrupt`"), "stderr: {}", text(&out.stderr));
}

/// The board's functions (`typelisp_host_*`) as a board gives them, written
/// in LLVM IR for want of a C compiler for the board in every environment the
/// tests run in: output to the UART of QEMU's board (CMSDK APB UART 0 at
/// `0x40004000`, which `-nographic` shows on standard output), a clock that
/// advances by itself, and the end of the program through semihosting, for
/// the exit code to reach the test.
const BOARD_FUNCTIONS_IR: &str = r#"
target triple = "thumbv7em-unknown-none-eabihf"

@clock = internal global i64 0

define void @typelisp_host_write(i32 %fd, ptr %p, i32 %len) {
entry:
  store volatile i32 1, ptr inttoptr (i32 1073758216 to ptr)
  br label %next
next:
  %i = phi i32 [0, %entry], [%i1, %sent]
  %done = icmp eq i32 %i, %len
  br i1 %done, label %out, label %wait
wait:
  %state = load volatile i32, ptr inttoptr (i32 1073758212 to ptr)
  %full = and i32 %state, 1
  %busy = icmp ne i32 %full, 0
  br i1 %busy, label %wait, label %send
send:
  %a = getelementptr i8, ptr %p, i32 %i
  %c = load i8, ptr %a
  %w = zext i8 %c to i32
  store volatile i32 %w, ptr inttoptr (i32 1073758208 to ptr)
  br label %sent
sent:
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
  call void asm sideeffect "movs r0, #0x20\0Abkpt #0xab", "{r1},~{r0},~{memory}"(ptr %block)
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

/// The object [`BOARD_FUNCTIONS_IR`] compiles to.
fn board_functions_object(dir: &Path) -> PathBuf {
    board_object(dir, BOARD_FUNCTIONS_IR)
}

/// The Cortex-M4 object the LLVM IR `ir` compiles to.
fn board_object(dir: &Path, ir_text: &str) -> PathBuf {
    common::board::board_object(dir, ir_text, &["-mcpu=cortex-m4"])
}

/// The board's functions given as C functions (`--board-functions c`,
/// `--link`) instead of over semihosting: the program's output goes out
/// through them.
#[test]
fn the_board_gives_its_functions_as_c_functions() {
    let dir = scratch("board_c_functions");
    let object = board_functions_object(&dir);
    let elf = built(
        "board_c_functions",
        r#"(defun main () int (println "through the UART ~a" 7) (sleep 0.5) 4)"#,
        "prog.elf",
        &board_flags(&["--board-functions", "c", "--link", object.to_str().unwrap()]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "through the UART 7\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(4));
}

/// C functions as the board's C compiler sees them: a `long` is 32 bits,
/// and `struct rec { long n; void *p; int x; }` has `x` at offset 8.
const ILP32_C_FUNCTIONS_IR: &str = r#"
target triple = "thumbv7em-unknown-none-eabihf"

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
/// laid out as the board's C lays it out: a C function written for the
/// board reads what the program passes it and what it stored.
#[test]
fn c_long_and_c_structs_follow_the_board_s_c() {
    let dir = scratch("board_ilp32");
    let object = board_object(&dir, ILP32_C_FUNCTIONS_IR);
    let elf = built(
        "board_ilp32",
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
        &board_flags(&["--link", object.to_str().unwrap()]),
    );
    let out = run_board(&elf);
    assert_eq!(out.status.code(), Some(42), "stdout: {} stderr: {}", text(&out.stdout), text(&out.stderr));
}

/// `(as c-long n)` keeps the bits a board's `long` has, compiled and folded
/// alike.
#[test]
fn a_c_long_cast_keeps_the_board_s_width() {
    let source = r#"
(defun wrap ((n int)) int (unsafe (as int (as c-long n))))
(defun main () int (+ (wrap 4294967301) (unsafe (as int (as c-long 4294967306)))))
"#;
    let elf = built("board_c_long_wrap", source, "prog.elf", &board_flags(&[]));
    assert_eq!(run_board(&elf).status.code(), Some(15), "2^32 + 5 is 5 and 2^32 + 10 is 10 in a 32-bit long");
}

/// Without them, the link says which are missing.
#[test]
fn board_functions_left_out_are_a_link_error() {
    let (out, _) = build("board_c_missing", r#"(defun main () () (println "x"))"#, "prog.elf", &board_flags(&["--board-functions", "c"]));
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("typelisp_host_write"), "{}", text(&out.stderr));
}

#[test]
fn a_missing_link_file_is_an_error() {
    let (out, _) = build("board_missing_link", "(defun main () int 0)", "prog.elf", &board_flags(&["--link", "no-such.o"]));
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("--link: no-such.o: no such file"), "{}", text(&out.stderr));
}

/// The board's memory from `--linker-script`: a smaller RAM, and a stack the
/// file sizes, still runs the program.
#[test]
fn the_linker_script_places_the_program_in_the_board_s_memory() {
    let dir = scratch("board_linker_script");
    let memory = dir.join("memory.x");
    std::fs::write(
        &memory,
        "MEMORY\n{\n  FLASH : ORIGIN = 0x00000000, LENGTH = 1M\n  RAM : ORIGIN = 0x20000000, LENGTH = 512K\n}\n_typelisp_stack_size = 32K;\n",
    )
    .unwrap();
    let elf = built(
        "board_linker_script",
        r#"(defun main () () (println "placed"))"#,
        "prog.elf",
        &board_flags(&["--linker-script", memory.to_str().unwrap()]),
    );
    let out = run_board(&elf);
    assert_eq!(text(&out.stdout), "placed\n", "stderr: {}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn a_missing_linker_script_is_an_error() {
    let (out, _) = build(
        "board_missing_linker_script",
        "(defun main () int 0)",
        "prog.elf",
        &board_flags(&["--linker-script", "no-such-memory.x"]),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("--linker-script: no-such-memory.x:"), "{}", text(&out.stderr));
}

#[test]
fn the_board_s_flags_are_only_for_a_board() {
    for flags in [["--linker-script", "memory.x"], ["--board-functions", "c"], ["--link", "board.o"]] {
        let (out, _) = build("board_flags_host", "(defun main () int 0)", "prog", &flags);
        assert_eq!(out.status.code(), Some(1));
        let expected = format!("{}: only for a board", flags[0]);
        assert!(text(&out.stderr).contains(&expected), "{}", text(&out.stderr));
    }
}

#[test]
fn eval_is_refused_at_compile_time() {
    let err = refused("board_refuse_eval", r#"(defun main () () (println "~a" (eval (quote (+ 2 3)))))"#);
    assert!(err.contains("--target thumbv7em-none-eabihf: the program uses what the target does not have:"), "{}", err);
    assert!(err.contains("`eval` (reached by main -> prog::main"), "{}", err);
}

#[test]
fn a_file_is_refused_at_compile_time() {
    let err = refused(
        "board_refuse_file",
        r#"
(use std::fs::write-file-string)
(defun main () () (println "~a" (write-file-string "out.txt" "x")))
"#,
    );
    assert!(err.contains("opening a file (reached by main -> prog::main -> std::fs::write-file-string"), "{}", err);
}

#[test]
fn a_socket_is_refused_at_compile_time() {
    let err = refused(
        "board_refuse_socket",
        r#"
(use std::net::tcp-listen)
(defun main () ()
  (match (tcp-listen "127.0.0.1" 0) ((Ok _) (println "ok")) ((Err _) (println "err"))))
"#,
    );
    assert!(err.contains("a socket (reached by main -> prog::main -> std::net::tcp-listen"), "{}", err);
}

#[test]
fn an_os_thread_is_refused_at_compile_time() {
    let err = refused(
        "board_refuse_thread",
        r#"
(use std::thread::Thread)
(defun main () () (println "~a" (join (Thread::spawn (lambda () int 1)))))
"#,
    );
    assert!(err.contains("an OS thread (`thread`)"), "{}", err);
}

/// A word is 32 bits there, and the answers are the same.
#[test]
fn a_narrow_word_answers_as_a_wide_one() {
    for level in ["-O0", "-O2"] {
        let (out, _, code) = same_as_native("board_word_width", include_str!("programs/word_width.typl"), &[level]);
        assert!(out.starts_with("1073741823\n1073741824\n"), "{}", out);
        assert_eq!(code, Some(0));
    }
}

/// A global of every type that is kept outside the runtime's table, and some
/// that are not.
const GLOBALS: &str = r#"
(defconstant (K int) 7)
(defvar (n u8) 250)
(defvar (f f32) 1.5)
(defvar (g f64) 2.25)
(defvar (c char) #\a)
(defvar (b bool) true)
(defvar (w u32) 4000000000)
(defvar (neg i16) -3)
(defvar (from-n u8) (+ n 1))
(defvar (acc int) 10)
(defvar (text string) "abc")
(defun advance () ()
  (setf n (+ n 10)) (setf f (* f 2.0)) (setf g (+ g 0.5)) (setf c #\z) (setf b (not b))
  (setf w (+ w 1)) (setf neg (- neg 1)) (setf acc (+ acc K)) ())
(defun main () ()
  (advance)
  (println "~a ~a ~a ~a ~a ~a ~a ~a ~a ~a ~a" n f g c b w neg from-n acc K text))
"#;

#[test]
fn globals_hold_their_values() {
    for level in ["-O0", "-O2"] {
        let (stdout, _, _) = same_as_native(&format!("board_globals{}", level), GLOBALS, &[level]);
        assert_eq!(stdout, "4 3.0 2.75 z false 4000000001 -4 251 17 7 abc\n");
    }
}

/// The program waits for what a handler assigns, at every level.
#[test]
fn the_program_sees_what_a_handler_assigns() {
    for level in ["-O0", "-O2"] {
        let elf = built(
            &format!("board_flag{}", level),
            r#"
(use std::board::enable-interrupt)
(defvar (*ticks* u32) 0)
(defconstant (CONTROL int) #xE000E010)

#[exception(SysTick)]
(defun on-tick () () (setf *ticks* (+ *ticks* 1)) ())

(defun main () int
  (unsafe
    (volatile-write-u32 #xE000E014 (as u32 25000))
    (volatile-write-u32 #xE000E018 (as u32 0))
    (volatile-write-u32 CONTROL (as u32 7)))
  (while (< *ticks* 5))
  7)
"#,
            "prog.elf",
            &board_flags(&[level]),
        );
        assert_eq!(run_board(&elf).status.code(), Some(7), "{}", level);
    }
}
