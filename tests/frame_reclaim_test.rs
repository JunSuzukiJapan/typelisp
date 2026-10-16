//! A compiled call's frame is given back when the call is over — on its
//! return and when an unwind leaves it — rather than when a collection finds
//! it. A program that allocates no cons cells never collects, so before this
//! every frame it ever made stayed: `fib` of 32 held 1.4GB. What is left is
//! counted from outside, through `heap-info`'s box count, in an executable
//! and in a run that compiles.

use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("typelisp-frame-reclaim-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

fn run(cmd: &mut Command) -> String {
    let out = cmd.output().expect("running a command");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The box count `heap-info` printed on the line after `label`.
fn boxes_after(out: &str, label: &str) -> usize {
    let mut lines = out.lines();
    lines.find(|l| *l == label).unwrap_or_else(|| panic!("no `{}` in {}", label, out));
    let info = lines.next().unwrap_or_else(|| panic!("nothing after `{}` in {}", label, out));
    let before = info.split(" boxes").next().unwrap_or_else(|| panic!("no box count in {}", info));
    before.rsplit(' ').next().and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("no box count in {}", info))
}

/// Twenty-one thousand calls that return, and as many that are left by an
/// unwind (`throw` out of the innermost of a chain of calls).
const PROGRAM: &str = r#"
(defun fib ((n int)) int (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
(defun dive ((n int)) int (if (= n 0) (throw 'out 1) (+ 1 (dive (- n 1)))))
(defun dives ((times int)) int
  (let ((i 0) (sum 0))
    (loop (if (>= i times) (return sum)
            (progn (setf sum (+ sum (catch 'out (dive 100)))) (setf i (+ i 1)))))))
(defun main () ()
  (println "~a" (fib 20))
  (println "after returns")
  (println "~a" (heap-info))
  (println "~a" (dives 200))
  (println "after unwinds")
  (println "~a" (heap-info)))
"#;

/// Far fewer than the 21891 calls of `fib 20` or the 20200 frames the dives
/// leave: what the program itself holds, and nothing per call.
const MOST: usize = 2000;

fn check(out: &str) {
    assert!(out.starts_with("6765\n"), "{}", out);
    assert!(out.contains("\n200\n"), "{}", out);
    for label in ["after returns", "after unwinds"] {
        let boxes = boxes_after(out, label);
        assert!(boxes < MOST, "{}: {} boxes are left\n{}", label, boxes, out);
    }
}

#[test]
fn an_executable_gives_its_frames_back() {
    let dir = scratch("aot");
    let file = dir.join("prog.typl");
    std::fs::write(&file, PROGRAM).expect("writing the program");
    let exe = dir.join("prog");
    run(Command::new(env!("CARGO_BIN_EXE_typl")).arg("-c").arg(&file).arg("-o").arg(&exe));
    check(&run(&mut Command::new(&exe)));
}

#[test]
fn a_run_gives_the_frames_of_what_it_compiled_back() {
    let dir = scratch("jit");
    let file = dir.join("prog.typl");
    let text = format!("{}\n(compile fib)\n(compile dive)\n(compile dives)\n(compile main)\n(main)\n", PROGRAM);
    std::fs::write(&file, text).expect("writing the program");
    check(&run(Command::new(env!("CARGO_BIN_EXE_typl")).arg(&file)));
}
