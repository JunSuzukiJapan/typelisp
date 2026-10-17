//! The optimization level, `typl -O0` to `-O3`: the command line records it,
//! and the compiled code at every level computes what `-O0`'s does. The
//! levels reach the island's IR passes (`optimize-ir`) and LLVM's pipeline,
//! both of which only the `typl` binary sets up, so this drives it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("typelisp-opt-level-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

/// Exit code, stdout, stderr.
fn output(cmd: &mut Command) -> (i32, String, String) {
    let out = cmd.output().expect("running a command");
    (
        out.status.code().expect("did not exit normally"),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn typl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_typl"))
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, text).expect("writing the program");
    file
}

/// One function per thing the island's passes rewrite, or must not.
const PROGRAM: &str = r#"
;; Propagated and folded, and the `if` on the folded test taken.
(defun folded ((n int)) int
  (let ((k 10) (flag true))
    (if flag (+ n k) (* n 2))))
;; Assigned: not propagated.
(defun assigned ((m int)) int
  (let ((s 0) (i 0))
    (loop (if (>= i m) (return s) (progn (setf s (+ s i)) (setf i (+ i 1)))))))
;; A product past the fixnum range is a bignum, not a literal.
(defun past-fixnum () int
  (let ((a 4611686018427387903) (b 3))
    (let ((c (* a b)) (d (+ b 4)))
      (if (< d 10) (+ c d) 0))))
(defun at-fixnum-edges () int
  (let ((a 4611686018427387902) (b -4611686018427387903))
    (+ (+ a 1) (- b 1))))
;; `y` reads the outer `x`, not the one this `let` binds.
(defun shadowed ((x int)) int
  (let ((x 5) (y x)) (+ x y)))
(defun nested-shadow () int
  (let ((x 1))
    (+ x (let ((x 2)) x))))
;; Halves of a wide literal.
(defun wide () int (let ((a -4294967297) (b 2)) (* a b)))
;; Captured by a closure: a cell, not a literal.
(defun captured ((n int)) int
  (let ((k 3))
    (let ((f (lambda ((z int)) int (+ z k))))
      (f n))))
;; The branch taken leaves the loop, so the `if` stays.
(defun leaves () int
  (let ((stop true) (i 0))
    (loop (if stop (return i) (setf i (+ i 1))))))
(defun comparisons () int
  (let ((a 3) (b 4))
    (+ (if (= a b) 1 0) (if (/= a b) 2 0) (if (< a b) 4 0)
       (if (<= a b) 8 0) (if (> a b) 16 0) (if (>= a b) 32 0))))
(defun a-char () char (let ((c #\z)) c))
(defun a-float () f64 (let ((f 2.5)) (* f 2.0)))
(defun matched ((o Option<int>)) int
  (let ((v 100))
    (match o ((some v) v) ((none) v))))
(defun main () ()
  (println "~a ~a ~a ~a ~a ~a ~a" (folded 3) (assigned 10) (past-fixnum) (at-fixnum-edges)
           (shadowed 1) (nested-shadow) (wide))
  (println "~a ~a ~a ~a ~a ~a ~a" (captured 4) (leaves) (comparisons) (a-char) (a-float)
           (matched (Option::some 7)) (matched (Option::none))))
"#;

const EXPECTED: &str = "13 45 13835058055282163716 -1 6 3 -8589934594\n7 0 14 z 5.0 7 100\n";

#[test]
fn every_level_computes_what_the_unoptimized_build_does() {
    let dir = scratch("compile");
    let file = write(&dir, "prog.typl", PROGRAM);
    for level in 0..=3 {
        let exe = dir.join(format!("prog{}", level));
        let (code, _, err) =
            output(typl().arg(format!("-O{}", level)).arg("-c").arg(&file).arg("-o").arg(&exe));
        assert_eq!(code, 0, "-O{}: {}", level, err);
        let (code, out, err) = output(&mut Command::new(&exe));
        assert_eq!((code, out.as_str()), (0, EXPECTED), "-O{}: {}", level, err);
    }
}

/// A run JIT-compiles every closure when it is defined, at the level given.
#[test]
fn every_level_runs_what_the_unoptimized_run_does() {
    let dir = scratch("run");
    let file = write(&dir, "prog.typl", &format!("{}\n(main)\n", PROGRAM));
    for level in 0..=3 {
        let (code, out, err) = output(typl().arg(format!("-O{}", level)).arg(&file));
        assert_eq!((code, out.as_str()), (0, EXPECTED), "-O{}: {}", level, err);
    }
}

/// `optimize-ir` at `level` on core IR written as text, printed with `~s`.
///
/// Read and checked in the island's own module (`%core`): the island's
/// functions are that module's, and only there are `let`/`var`/`assoc` the
/// core IR's tags rather than ordinary symbols.
fn optimized(level: i64, ir: &str) -> String {
    use typelisp::{load_compiler, load_prelude, Checker, Heap, Interp, Reader, Value};
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    chk.enter_file_module(&typelisp::compiler::island_module_segments()).expect("the island's module");
    let src = format!("(format false \"~s\" (optimize-ir {} '{}))", level, ir);
    let forms = Reader::new()
        .read_all_in_spanned_within(&mut h, "<ir>", &src, typelisp::NsId::CORE)
        .expect("read failed");
    let mut last = Value::Empty;
    for (v, _) in forms {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

#[test]
fn the_island_rewrites_only_at_level_one_and_up() {
    let cases = [
        // Level 0: nothing.
        (0, r#"(let (((k . 2) int 0 10)) (assoc "int" "+" true "f" (2 var "n" false) (2 var "k" false)))"#,
            r#"(let (((k . 2) int 0 10)) (assoc "int" "+" true "f" (2 var "n" false) (2 var "k" false)))"#),
        (1, r#"(let (((k . 2) int 0 10)) (assoc "int" "+" true "f" (2 var "n" false) (2 var "k" false)))"#,
            r#"(let () (assoc "int" "+" true "f" (2 var "n" false) (2 int 0 10)))"#),
        (1, r#"(let (((k . 2) int 0 10)) (assoc "int" "+" true "f" (2 var "k" false) (2 var "k" false)))"#,
            r#"(let () (int 0 20))"#),
        (1, r#"(if false (assoc "int" "<" true "f" (2 int 0 1) (2 int 0 2)) (int 0 1) (int 0 2))"#,
            r#"(int 0 1)"#),
        // The branch taken leaves its block.
        (1, r#"(if false (bool true) (break) (int 0 1))"#, r#"(if false (bool true) (break) (int 0 1))"#),
        // Assigned.
        (1, r#"(let (((k . 2) int 0 1)) (set "k" 2 (int 0 2)) (var "k" false))"#,
            r#"(let (((k . 2) int 0 1)) (set "k" 2 (int 0 2)) (var "k" false))"#),
        // Shadowed: the inner one is propagated, the outer one is not.
        (1, r#"(let (((k . 2) int 0 1)) (let (((k . 2) int 0 2)) (var "k" false)))"#,
            r#"(let (((k . 2) int 0 1)) (let () (int 0 2)))"#),
        // A binding form reads an outer `k`.
        (1, r#"(let (((k . 2) int 0 1) ((j . 2) var "k" false)) (var "k" false))"#,
            r#"(let (((k . 2) int 0 1) ((j . 2) var "k" false)) (var "k" false))"#),
        // The sum is past the fixnum range.
        (1, r#"(let (((k . 2) int 1073741823 -1)) (assoc "int" "+" true "f" (2 var "k" false) (2 int 0 1)))"#,
            r#"(let () (assoc "int" "+" true "f" (2 int 1073741823 -1) (2 int 0 1)))"#),
    ];
    for (level, ir, expected) in cases {
        assert_eq!(optimized(level, ir), expected, "-O{} of {}", level, ir);
    }
}

#[test]
fn the_flag_names_one_of_four_levels() {
    let dir = scratch("flag");
    let file = write(&dir, "prog.typl", "(println \"~a\" (command-line-args))\n");
    for bad in ["-O4", "-O", "-Os"] {
        let (code, _, err) = output(typl().arg(bad).arg(&file));
        assert_eq!(code, 1, "{}", bad);
        assert!(err.contains("-O0, -O1, -O2 and -O3"), "{}: {}", bad, err);
        let (code, _, err) = output(typl().arg("-c").arg(&file).arg(bad));
        assert_eq!(code, 1, "-c ... {}", bad);
        assert!(err.contains("-O0, -O1, -O2 and -O3"), "-c ... {}: {}", bad, err);
    }
    // After the file name it is the program's own argument.
    let (code, out, err) = output(typl().arg(&file).arg("-O2"));
    assert_eq!(code, 0, "{}", err);
    assert!(out.trim_end().ends_with("-O2)"), "{}", out);
    // Written after `-c`, it is the global one.
    let file = write(&dir, "main.typl", "(defun main () () (println \"~a\" (+ 1 2)))\n");
    let exe = dir.join("main");
    let (code, _, err) = output(typl().arg("-c").arg(&file).arg("-O3").arg("-o").arg(&exe));
    assert_eq!(code, 0, "{}", err);
    let (code, out, err) = output(&mut Command::new(&exe));
    assert_eq!((code, out.as_str()), (0, "3\n"), "{}", err);
}
