//! `typl -c --target avr-atmega328p`: an ELF executable for the Arduino Uno's
//! ATmega328P, run under QEMU (`qemu-system-avr -M uno`), and the programs
//! it refuses at compile time.
//!
//! A program here ends with `main` answering, which stops the processor, and
//! QEMU with it only when it is told to: what the program writes to the
//! serial port (USART0) ends with a `.` the test waits for.
//!
//! Needs the linker `scripts/build-embedded-runtime.sh` puts beside the
//! runtimes for a board, and `qemu-system-avr` on `PATH`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[path = "common/widths.rs"]
#[allow(dead_code)]
mod widths;

const TYPL: &str = env!("CARGO_BIN_EXE_typl");

/// How long a program may run under QEMU before the test calls it hung.
const RUN_LIMIT: Duration = Duration::from_secs(120);

/// `(start)` readies USART0 and `(put c)` writes a byte to it.
const SERIAL: &str = r#"
(defun start () () (unsafe (volatile-write-u8 #xC1 8)) ())
(defun put ((c u8)) ()
  (while (= (logand (unsafe (volatile-read-u8 #xC0)) 32) 0))
  (unsafe (volatile-write-u8 #xC6 c))
  ())
"#;

/// Blinking the LED of an Uno: PB5 an output, then turned over again and
/// again, waiting in between.
const BLINK: &str = r#"
(define-symbol-macro PINB #x23)
(define-symbol-macro DDRB #x24)
(define-symbol-macro LED (the u8 32))
(defun wait-a-bit () ()
  (let ((i (the u16 0)))
    (while (< i (the u16 1000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))
(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))
  (while true
    (unsafe (volatile-write-u8 PINB LED))
    (wait-a-bit)))
"#;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    dir
}

fn build(name: &str, source: &str, flags: &[&str]) -> (Output, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("prog.typl");
    let elf = dir.join("prog.elf");
    std::fs::write(&src, source).unwrap();
    let out = Command::new(TYPL)
        .args(["-c", src.to_str().unwrap(), "-o", elf.to_str().unwrap(), "--target", "avr-atmega328p"])
        .args(flags)
        .output()
        .expect("failed to start the typl binary");
    (out, elf)
}

fn built(name: &str, source: &str, flags: &[&str]) -> PathBuf {
    let (out, elf) = build(name, source, flags);
    assert!(out.status.success(), "typl -c {:?} failed: {}", flags, String::from_utf8_lossy(&out.stderr));
    elf
}

/// Runs `elf` on QEMU's Uno with `flags`, and answers what it writes to its
/// standard output until `enough` says so.
fn run_until(elf: &Path, flags: &[&str], enough: impl Fn(&str) -> bool) -> String {
    let mut child = Command::new("qemu-system-avr")
        .args(["-M", "uno", "-display", "none", "-monitor", "none", "-bios"])
        .arg(elf)
        .args(flags)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start qemu-system-avr (the Uno's executables run under QEMU)");
    let mut stdout = child.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut buf = [0; 256];
        while let Ok(n @ 1..) = stdout.read(&mut buf) {
            if send.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut seen = Vec::new();
    let mut text = String::new();
    while !enough(&text) && started.elapsed() < RUN_LIMIT {
        match receive.recv_timeout(Duration::from_millis(100)) {
            Ok(bytes) => {
                seen.extend(bytes);
                text = String::from_utf8_lossy(&seen).into_owned();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    drop(receive);
    let _ = reader.join();
    assert!(enough(&text), "{} did not do it; it wrote: {:?}", elf.display(), text);
    text
}

/// What `source` writes to the serial port up to its `.`, at `-O0` and at
/// `-O2`, which must agree.
fn written(name: &str, source: &str) -> String {
    let source = format!("{}{}", SERIAL, source);
    let [plain, optimized] = ["-O0", "-O2"].map(|level| {
        let elf = built(&format!("{}{}", name, level), &source, &[level]);
        run_until(&elf, &["-serial", "stdio"], |text| text.ends_with('.'))
    });
    assert_eq!(plain, optimized, "-O0 and -O2");
    plain
}

/// The bytes `elf` puts in flash: what its loaded segments have in the file.
fn flash_bytes(elf: &Path) -> u32 {
    let bytes = std::fs::read(elf).unwrap();
    let u16_at = |at: usize| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let (table, size, count) = (u32_at(28) as usize, u16_at(42) as usize, u16_at(44) as usize);
    (0..count).map(|i| table + i * size).filter(|at| u32_at(*at) == 1).map(|at| u32_at(at + 16)).sum()
}

fn refused(name: &str, source: &str) -> String {
    let (out, _) = build(name, source, &[]);
    assert_eq!(out.status.code(), Some(1));
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The LED's port is not a device QEMU has, so it logs each access.
#[test]
fn an_led_blinks() {
    for (name, level) in [("avr_blink_o0", "-O0"), ("avr_blink_o2", "-O2")] {
        let elf = built(name, BLINK, &[level]);
        let log = scratch(name).join("log");
        let _ = std::fs::remove_file(&log);
        let mut child = Command::new("qemu-system-avr")
            .args(["-M", "uno", "-display", "none", "-monitor", "none", "-serial", "null", "-d", "unimp", "-D"])
            .arg(&log)
            .arg("-bios")
            .arg(&elf)
            .stdin(Stdio::null())
            .spawn()
            .expect("failed to start qemu-system-avr (the Uno's executables run under QEMU)");
        let started = Instant::now();
        let (mut made_output, mut turned) = (0, 0);
        while turned < 3 && started.elapsed() < RUN_LIMIT {
            std::thread::sleep(Duration::from_millis(50));
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            made_output = text.matches("gpio-b: unimplemented device write (size 1, offset 0x1, value 0x20)").count();
            turned = text.matches("gpio-b: unimplemented device write (size 1, offset 0x0, value 0x20)").count();
        }
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(made_output, 1, "{}", level);
        assert!(turned >= 3, "{}: turned over {} times", level, turned);
    }
}

/// The interrupt vectors are 104 bytes of it.
#[test]
fn a_blinking_led_takes_no_more_than_it_needs() {
    let bytes = flash_bytes(&built("avr_blink_size", BLINK, &["-O2"]));
    assert!(bytes <= 200, "{} bytes", bytes);
}

/// Multiplication and division the processor has no instruction for, and the
/// widths it has none for.
#[test]
fn arithmetic_is_at_the_type_s_width() {
    let out = written(
        "avr_arith",
        r#"
(defun hex1 ((n u8)) () (put (if (< n (the u8 10)) (+ n (the u8 48)) (+ n (the u8 87)))) ())
(defun hex8 ((n u8)) () (hex1 (ash n -4)) (hex1 (logand n (the u8 15))) ())
(defun hex32 ((n u32)) ()
  (hex8 (as u8 (logand (ash n -24) (the u32 255))))
  (hex8 (as u8 (logand (ash n -16) (the u32 255))))
  (hex8 (as u8 (logand (ash n -8) (the u32 255))))
  (hex8 (as u8 (logand n (the u32 255))))
  (put 10) ())
(defun g32 ((n u32)) u32
  (unsafe (volatile-write-u32 #x200 n) (volatile-read-u32 #x200)))
(defun g8 ((n u8)) u8 (unsafe (volatile-write-u8 #x200 n) (volatile-read-u8 #x200)))
(defun g16 ((n u16)) u16 (unsafe (volatile-write-u16 #x200 n) (volatile-read-u16 #x200)))
(defun s32 ((n i32)) () (hex32 (as u32 n)) ())
(defun u16p ((n u16)) () (hex32 (as u32 n)) ())
(defun u8p ((n u8)) () (hex32 (as u32 n)) ())
(defun i8p ((n i8)) () (hex32 (as u32 n)) ())
(defun i16p ((n i16)) () (hex32 (as u32 n)) ())
(defun main () ()
  (start)
  (hex32 (* (g32 305419896) (g32 2596069104)))
  (hex32 (mod (g32 4000000000) (the u32 7919)))
  (hex32 (/ (g32 4000000000) (the u32 7919)))
  (hex32 (/ (g32 4000000000) (the u32 4000000000)))
  (s32 (mod (as i32 (g32 4000000000)) (the i32 7919)))
  (s32 (rem (as i32 (g32 4000000000)) (the i32 7919)))
  (s32 (/ (as i32 (g32 4000000000)) (the i32 -7919)))
  (u16p (/ (g16 50000) (the u16 123)))
  (u16p (mod (g16 50000) (the u16 123)))
  (i16p (/ (as i16 (g16 50000)) (the i16 123)))
  (i16p (rem (as i16 (g16 50000)) (the i16 -123)))
  (u8p (/ (g8 250) (the u8 7)))
  (u8p (mod (g8 250) (the u8 7)))
  (i8p (/ (as i8 (g8 250)) (the i8 7)))
  (i8p (rem (as i8 (g8 250)) (the i8 4)))
  (i8p (mod (as i8 (g8 250)) (the i8 4)))
  (put 46))
"#,
    );
    let values: Vec<&str> = out.trim_end_matches('.').split_whitespace().collect();
    let expected: Vec<String> = [
        305419896u32.wrapping_mul(2596069104),
        4000000000 % 7919,
        4000000000 / 7919,
        1,
        (4000000000u32 as i32).rem_euclid(7919) as u32,
        (4000000000u32 as i32 % 7919) as u32,
        (4000000000u32 as i32 / -7919) as u32,
        50000 / 123,
        50000 % 123,
        i32::from(50000u16 as i16 / 123) as u32,
        i32::from(50000u16 as i16 % -123) as u32,
        250 / 7,
        250 % 7,
        i32::from(250u8 as i8 / 7) as u32,
        i32::from(250u8 as i8 % 4) as u32,
        i32::from((250u8 as i8).rem_euclid(4)) as u32,
    ]
    .iter()
    .map(|n| format!("{:08x}", n))
    .collect();
    assert_eq!(values, expected);
}

/// Division by a variable at every width, of operands on both sides of zero:
/// the program writes a `y` or an `n` for each.
#[test]
fn a_division_by_a_variable() {
    for t in &widths::FIXED {
        let n = t.name;
        let mut main = String::from("  (start)\n");
        let dividends = [t.min(), t.min() + 1, -100, -7, -1, 0, 1, 7, 100, 12345, t.max() - 1, t.max()];
        let divisors = [t.min(), -128, -10, -7, -1, 1, 2, 7, 10, 127, 255, 256, 1000, t.max()];
        let held = |v: &i64| (t.min()..=t.max()).contains(v);
        for a in dividends.into_iter().filter(held) {
            for b in divisors.into_iter().filter(|b| held(b) && *b != 0) {
                // No answer.
                if t.signed && a == t.min() && b == -1 {
                    continue;
                }
                let floor = a - b * (a as f64 / b as f64).floor() as i64;
                main.push_str(&format!("  (check {} {} {} {} {})\n", a, b, a / b, a % b, floor));
            }
        }
        let program = format!(
            "(defun say ((b bool)) () (put (if b 121 110)) ())
(defun check ((a {n}) (b {n}) (q {n}) (r {n}) (m {n})) () (say (= (/ a b) q)) (say (= (rem a b) r)) (say (= (mod a b) m)) ())
(defun main () ()\n{}  (put 46))\n",
            main
        );
        let out = written(&format!("avr_division_{}", n), &program);
        assert!(!out.contains('n') && out.len() > 100, "{}: {}", n, out);
    }
}

/// A division with no answer ends the program there: the processor stops,
/// and no interrupt comes any more.
#[test]
fn a_division_with_no_answer_stops_the_program() {
    for (name, ty, dividend, divisor) in [("u8", "u8", "5", "0"), ("i16", "i16", "-5", "0"), ("u32", "u32", "5", "0"), ("overflow", "i8", "-128", "-1")] {
        for op in ["/", "rem", "mod"] {
            // Those have an answer: 0.
            if name == "overflow" && op != "/" {
                continue;
            }
            for level in ["-O0", "-O2"] {
                let source = format!(
                    "{SERIAL}
(defvar (d {ty}) {divisor})
#[interrupt(13)]
(defun on-overflow () () (put 105) ())
(defun main () ()
  (start)
  (put 97)
  (unsafe (volatile-write-u8 #x6F 1) (volatile-write-u8 #x81 1))
  (when (= ({op} (the {ty} {dividend}) d) 1) (put 98))
  (put 99))
"
                );
                let name = format!("avr_fault_{}_{}{}", name, match op { "/" => "div", o => o }, level);
                let elf = built(&name, &source, &[level]);
                // The `a`, and time for anything after it to come.
                let seen = std::cell::Cell::new(None::<Instant>);
                let out = run_until(&elf, &["-serial", "stdio"], |text| {
                    if text.contains('a') && seen.get().is_none() {
                        seen.set(Some(Instant::now()));
                    }
                    seen.get().is_some_and(|at| at.elapsed() > Duration::from_millis(500))
                });
                assert_eq!(out, "a", "{}", name);
            }
        }
    }
}

/// Globals are in RAM, constants nowhere, and an initial value that is not a
/// constant is worked out at the start.
#[test]
fn globals_hold_their_values() {
    let source = r#"
(defconstant (PINB int) #x23)
(defconstant (LED u8) 32)
(defvar (n u8) 65)
(defvar (wide u32) 70000)
(defvar (flag bool) false)
(defvar (late u8) (+ n 2))
(defun bump () () (setf n (+ n 1)) (setf wide (+ wide 5)) (setf flag (not flag)) ())
(defun main () ()
  (start)
  (unsafe (volatile-write-u8 PINB LED))
  (bump)
  (put n)
  (put (if flag 89 78))
  (bump)
  (put (if flag 89 78))
  (put (if (= wide 70010) 89 78))
  (put late)
  (put 46))
"#;
    assert_eq!(written("avr_globals", source), "BYNYC.");
}

/// Every width on a 16-bit machine: how many bits each type has, where it
/// wraps, what a cast to it keeps of each other type, and what it holds in a
/// variable, a parameter and a global. What each should be is worked out
/// here; the program writes a `y` or an `n` for each.
#[test]
fn every_width_is_its_own() {
    for t in &widths::FIXED {
        let (n, bytes) = (t.name, t.bits / 8);
        let mut program = format!(
            "(defvar (kept {n}) 0)
(defun say ((b bool)) () (put (if b 121 110)) ())
(defun handed ((x {n})) {n} x)
(defun bits () u8
  (let ((x (the {n} 1)) (count (the u8 0)))
    (while (/= x 0) (setf x (ash x 1)) (setf count (+ count 1)))
    count))
(defun wrap ((x {n})) () (say (= (+ x 1) {min})) (say (= (- (+ x 1) 1) {max})) ())
(defun hold ((x {n})) () (setf kept x) (say (= kept x)) (say (= (handed x) x)) (say (= (handed kept) (handed x)))
  (unsafe (let ((c (as c-long x)) (u (as c-ulong x))) (say (= (as {n} c) x)) (say (= (as {n} u) x)) (say (= (as u32 c) (as u32 x))) (say (= (size-of-val c) 4))))
  ())
(defun sum ((a {n}) (b {n}) (want {n})) () (say (= (+ a b) want)) ())
(defun product ((a {n}) (b {n}) (want {n})) () (say (= (* a b) want)) ())
",
            min = t.min(),
            max = t.max()
        );
        let mut main = format!(
            "  (start)
  (say (= (size-of {n}) {bytes})) (say (= (size-of-val (handed 1)) {bytes})) (say (= (size-of-val (+ kept 1)) {bytes}))
  (say (= (size-of c-long) 4)) (say (= (size-of ptr) 2)) (say (= (size-of int) 2))
  (say (= (bits) {bits}))
  (wrap {max})
",
            bits = t.bits,
            max = t.max()
        );
        let wrapped = |v: i64, t: &widths::Fixed| {
            let low = v.rem_euclid(1 << t.bits);
            if t.signed && low >= 1 << (t.bits - 1) { low - (1 << t.bits) } else { low }
        };
        for v in t.values() {
            main.push_str(&format!("  (hold {})\n", v));
            main.push_str(&format!("  (sum {v} {max} {})\n", wrapped(v + t.max(), t), max = t.max()));
            main.push_str(&format!("  (product {v} {max} {})\n", wrapped(v.wrapping_mul(t.max()), t), max = t.max()));
        }
        for to in &widths::FIXED {
            program.push_str(&format!("(defun to-{0} ((x {n}) (want {0})) () (say (= (as {0} x) want)) (say (= (size-of-val (as {0} x)) {1})) ())\n", to.name, to.bits / 8));
            for v in [t.min(), -1, 1, 128, 255, 256, 65535, t.max()] {
                if (t.min()..=t.max()).contains(&v) {
                    main.push_str(&format!("  (to-{} {} {})\n", to.name, v, wrapped(v, to)));
                }
            }
        }
        program.push_str(&format!("(defun main () ()\n{}  (put 46))\n", main));
        let out = written(&format!("avr_widths_{}", n), &program);
        assert!(!out.contains('n') && out.len() > 50, "{}: {}", n, out);
    }
}

/// Timer 1 overflowing (vector 13) and the serial port ready for a byte
/// (19). The handler and the program share a wide global, every register of
/// the program survives the handler, and a function both call.
#[test]
fn interrupt_handlers_run() {
    let source = r#"
(defconstant (TCCR1B int) #x81)
(defconstant (TIMSK1 int) #x6F)
(defconstant (UCSR0B int) #xC1)
(defvar (ticks u32) 65000)
(defvar (text u8) 97)
(defvar (late u16) (* (the u16 300) (the u16 2)))
(defun add-one ((n u32)) u32 (+ n 1))

#[interrupt(13)]
(defun on-overflow () () (setf ticks (add-one ticks)) (unsafe (volatile-write-u16 #x84 65000)) ())

#[interrupt(19)]
(defun on-ready () ()
  (put text)
  (setf text (+ text 1))
  (when (= text 102) (unsafe (volatile-write-u8 UCSR0B 8)))
  ())

(defun busy () u32
  (let ((sum (the u32 0)) (i (the u32 0)))
    (while (< (- ticks (the u32 65000)) (the u32 1000))
      (setf i (+ i 1))
      (setf sum (+ sum i))
      (setf sum (- sum i)))
    (add-one sum)))

(defun main () ()
  (start)
  (put (if (= late 600) 89 78))
  (unsafe (volatile-write-u8 TIMSK1 1) (volatile-write-u8 TCCR1B 1)
          (volatile-write-u16 #x84 65000))
  (put (if (= (busy) 1) 89 78))
  (unsafe (volatile-write-u8 TIMSK1 0))
  (put (if (< ticks (the u32 66000)) 78 89))
  (unsafe (volatile-write-u8 UCSR0B 40))
  (while (< text (the u8 102)))
  (put 46))
"#;
    assert_eq!(written("avr_interrupts", source), "YYYabcde.");
}

/// A handler never sees half of what the program is storing, nor leaves it
/// half of another value: both halves of each are the same.
#[test]
fn a_wide_global_is_whole() {
    let source = r#"
(defvar (wide u32) 0)
(defvar (seen u16) 0)
(defvar (torn u8) 0)
(defun look ((n u32)) ()
  (when (/= (logand n (the u32 65535)) (ash n -16)) (setf torn 1)) ())
#[interrupt(13)]
(defun on-overflow () ()
  (look wide)
  (setf seen (+ seen 1))
  (setf wide 1431655765)
  (unsafe (volatile-write-u16 #x84 65223))
  ())
(defun main () ()
  (start)
  (unsafe (volatile-write-u8 #x6F 1) (volatile-write-u8 #x81 1))
  (let ((i (the u16 0)))
    (while (< seen (the u16 1000))
      (setf wide 0) (look wide) (setf wide 4294967295) (look wide) (setf i (+ i 1))))
  (unsafe (volatile-write-u8 #x6F 0))
  (put (if (= torn 0) 89 78))
  (put 46))
"#;
    assert_eq!(written("avr_whole_global", source), "Y.");
}

/// Each global an access of which disables interrupts is warned about, once.
#[test]
fn disabling_interrupts_for_a_wide_global_is_warned_about() {
    let program = |attribute: &str| {
        format!(
            "(defvar (wide u16) 0)\n(defvar (wider u32) 0)\n(defvar (small u8) 0)\n(defvar (unused u16) 0)\n(defconstant (fixed u16) 300)
{}(defun f () () (setf wide (+ wide fixed)) (setf wider (+ wider 1)) (setf small (+ small 1)) ())
(defun main () () (f) (while (= wide 0)))\n",
            attribute
        )
    };
    let warnings = |name: &str, attribute: &str| {
        let (out, _) = build(name, &program(attribute), &[]);
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(out.status.success(), "{}", err);
        err
    };
    let one = |name: &str, bytes: u8| {
        format!("warning: interrupts are disabled while the global variable `prog::{}` is read or written: it is {} bytes, and an interrupt handler could come between them\n", name, bytes)
    };
    assert_eq!(warnings("avr_warned", "#[interrupt(13)]\n"), one("wide", 2) + &one("wider", 4));
    assert_eq!(warnings("avr_not_warned", ""), "");
}

#[test]
fn a_handler_the_chip_cannot_run_is_refused() {
    for (name, source, why) in [
        ("reset", "#[interrupt(0)]\n(defun f () () ())\n(defun main () ())", "`prog::f` handles interrupt 0: the interrupts of --target avr-atmega328p are 1 to 25"),
        ("past", "#[interrupt(26)]\n(defun f () () ())\n(defun main () ())", "`prog::f` handles interrupt 26: the interrupts of"),
        ("exception", "#[exception(SysTick)]\n(defun f () () ())\n(defun main () ())", "`prog::f` handles a core exception, and --target avr-atmega328p has none"),
        ("catches", "#[interrupt(13)]\n(defun f () () (catch 'a ()) ())\n(defun main () ())", "`catch` (in prog::f)"),
        ("prints", "#[interrupt(13)]\n(defun f () () (println \"x\"))\n(defun main () ())", "(reached by prog::f"),
        (
            "enables",
            "(use std::board::enable-interrupt)\n(defun main () () (enable-interrupt 13))",
            "`enable-interrupt`, which is for an interrupt controller the chip has not",
        ),
    ] {
        let err = refused(&format!("avr_handler_{}", name), source);
        assert!(err.contains(why), "{}: {}", name, err);
    }
}

/// At `-O2` the `case` is a table, which is in RAM.
#[test]
fn a_table_is_readied_in_ram() {
    let source = r#"
(defun pick ((n u8)) u8
  (case n (0 72) (1 101) (2 108) (3 108) (4 111) (5 33) (6 65) (7 90) (8 77) (:else 46)))
(defun main () ()
  (start)
  (let ((i (the u8 0)))
    (while (< i (the u8 10))
      (put (pick (unsafe (volatile-write-u8 #x200 i) (volatile-read-u8 #x200))))
      (setf i (+ i 1)))))
"#;
    assert_eq!(written("avr_table", source), "Hello!AZM.");
}

#[test]
fn the_linker_script_says_where_memory_is() {
    let dir = scratch("avr_linker_script");
    let memory = dir.join("memory.x");
    std::fs::write(&memory, "MEMORY\n{\n  FLASH : ORIGIN = 0, LENGTH = 64\n  RAM : ORIGIN = 0x800100, LENGTH = 2K\n}\n").unwrap();
    let (out, _) = build("avr_linker_script", BLINK, &["--linker-script", memory.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(err.contains("will not fit in region 'FLASH'"), "{}", err);
}

/// The program says why it needs the runtime.
#[test]
fn a_program_that_needs_the_runtime_is_refused() {
    for (name, source, why) in [
        ("string", r#"(defun main () () (println "hi"))"#, "the runtime function `rt_str_new` (reached by prog::main)"),
        ("variable", "(defvar (s string) \"a\")\n(defun main () ())", "the runtime function `rt_str_new` (reached by $global_init$"),
        ("bignum", "(defun main () int 100000)", "the runtime function `rt_bignum_new` (reached by prog::main)"),
        (
            "recursion",
            "(defun f ((n u8)) u8 (if (= n 0) 0 (f (- n 1))))\n(defun main () () (f 3) ())",
            "a function that calls itself (in prog::main -> prog::f)",
        ),
        ("c", "(defffi c-go () ())\n(defun main () () (unsafe (c-go)))", "a C function (in prog::main -> prog::c-go)"),
        ("eval", "(defun main () () (eval '(+ 1 2)) ())", "the interpreter"),
        ("catch", "(defun main () () (catch 'a ()) ())", "`catch` (in prog::main)"),
    ] {
        let err = refused(&format!("avr_refused_{}", name), source);
        assert!(err.contains("--target avr-atmega328p: the target has no runtime, and the program needs it:"), "{}: {}", name, err);
        assert!(err.contains(why), "{}: {}", name, err);
    }
}

#[test]
fn the_runtime_s_flags_are_refused() {
    for flags in [["--board-functions", "c"], ["--heap-cells", "16"], ["--max-memory", "1K"]] {
        let (out, _) = build("avr_runtime_flags", BLINK, &flags);
        assert_eq!(out.status.code(), Some(1));
        let expected = format!("{}: the target avr-atmega328p has no runtime", flags[0]);
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(err.contains(&expected), "{}", err);
    }
}
