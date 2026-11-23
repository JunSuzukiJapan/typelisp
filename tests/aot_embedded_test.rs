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
//! Scratch files go under `target/aot-test-tmp/`, one name per test, for the
//! reason `tests/compile_file_test.rs`'s module doc comment gives.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const TYPL: &str = env!("CARGO_BIN_EXE_typl");

const TARGET: &str = "thumbv7em-none-eabihf";

/// The Cortex-M33 board, and the QEMU board that stands for it.
const M33: (&str, &str) = ("thumbv8m.main-none-eabihf", "mps2-an505");

/// How long a program may run under QEMU before the test calls it hung: a
/// board's program that faults or never ends would otherwise hold the test
/// forever.
const RUN_LIMIT: Duration = Duration::from_secs(120);

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    dir
}

/// Compiles `source` with `flags` after `-c`: `typl`'s own output, and the
/// output's path.
fn build(name: &str, source: &str, output: &str, flags: &[&str]) -> (Output, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("prog.typl");
    let out_path = dir.join(output);
    std::fs::write(&src, source).unwrap();
    let out = Command::new(TYPL)
        .args(["-c", src.to_str().unwrap(), "-o", out_path.to_str().unwrap()])
        .args(flags)
        .output()
        .expect("failed to start the typl binary");
    (out, out_path)
}

fn built(name: &str, source: &str, output: &str, flags: &[&str]) -> PathBuf {
    let (out, path) = build(name, source, output, flags);
    assert!(out.status.success(), "typl -c {:?} failed: {}", flags, String::from_utf8_lossy(&out.stderr));
    path
}

/// The flags for the board, then `flags`.
fn board_flags<'a>(flags: &[&'a str]) -> Vec<&'a str> {
    target_flags(TARGET, flags)
}

/// The flags for the board `target`, then `flags`.
fn target_flags<'a>(target: &'a str, flags: &[&'a str]) -> Vec<&'a str> {
    let mut all = vec!["--target", target];
    all.extend_from_slice(flags);
    all
}

/// Runs the executable at `elf` on QEMU's Cortex-M4 board, its standard
/// output and error over semihosting.
fn run_board(elf: &Path) -> Output {
    run_on("mps2-an386", elf)
}

/// Runs the executable at `elf` on the QEMU board `machine`.
fn run_on(machine: &str, elf: &Path) -> Output {
    let child = Command::new("qemu-system-arm")
        .args(["-M", machine, "-nographic", "-semihosting-config", "enable=on,target=native", "-kernel"])
        .arg(elf)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start qemu-system-arm (the board's executables run under QEMU)");
    let started = Instant::now();
    let mut child = child;
    loop {
        if child.try_wait().expect("failed to wait for qemu-system-arm").is_some() {
            return child.wait_with_output().expect("failed to read qemu-system-arm's output");
        }
        if started.elapsed() > RUN_LIMIT {
            let _ = child.kill();
            let out = child.wait_with_output().expect("failed to read qemu-system-arm's output");
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

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// What `source` prints and exits with on the board, after checking that
/// the native executable prints and exits the same.
fn same_as_native(name: &str, source: &str, flags: &[&str]) -> (String, String, Option<i32>) {
    same_as_native_on((TARGET, "mps2-an386"), name, source, flags)
}

/// [`same_as_native`] on the board `target`, run on the QEMU board
/// `machine`.
fn same_as_native_on((target, machine): (&str, &str), name: &str, source: &str, flags: &[&str]) -> (String, String, Option<i32>) {
    let elf = built(name, source, "prog.elf", &target_flags(target, flags));
    let native = built(name, source, "prog", flags);
    let b = run_on(machine, &elf);
    let n = Command::new(&native).output().expect("failed to run the native executable");
    assert_eq!(text(&b.stdout), text(&n.stdout), "stdout differs from the native executable's");
    assert_eq!(text(&b.stderr), text(&n.stderr), "stderr differs from the native executable's");
    assert_eq!(b.status.code(), n.status.code(), "exit code differs from the native executable's");
    (text(&b.stdout), text(&b.stderr), b.status.code())
}

/// What `typl -c --target thumbv7em-none-eabihf` said when it refused
/// `source`.
fn refused(name: &str, source: &str) -> String {
    let (out, _) = build(name, source, "prog.elf", &board_flags(&[]));
    assert_eq!(out.status.code(), Some(1), "typl -c built it: {}", text(&out.stdout));
    text(&out.stderr)
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
        &target_flags(M33.0, &[]),
    );
    let out = run_on(M33.1, &elf);
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

/// The object [`BOARD_FUNCTIONS_IR`] compiles to, by the `llc` of the LLVM
/// typl is built with.
fn board_functions_object(dir: &Path) -> PathBuf {
    let prefix = std::env::var("LLVM_SYS_221_PREFIX").expect("LLVM_SYS_221_PREFIX names the LLVM typl is built with");
    let ir = dir.join("board.ll");
    let object = dir.join("board.o");
    std::fs::write(&ir, BOARD_FUNCTIONS_IR).unwrap();
    let out = Command::new(Path::new(&prefix).join("bin").join("llc"))
        .args(["-mcpu=cortex-m4", "-filetype=obj", "-o"])
        .arg(&object)
        .arg(&ir)
        .output()
        .expect("failed to start llc");
    assert!(out.status.success(), "llc failed: {}", text(&out.stderr));
    object
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
