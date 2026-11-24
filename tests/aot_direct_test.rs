//! Which functions of an executable are ordinary functions of their own
//! signature (`typelisp::compile::aot`'s judgement of direct functions), and
//! that they mean what the coroutines of the JIT mean.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Compiles `source` to a native executable, runs it, and answers what it
/// did and the IR of the whole program as the compiler emitted it
/// (`TYPELISP_AOT_IR`, before LLVM's passes).
fn built(name: &str, source: &str, flags: &[&str]) -> (Output, String) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(format!("direct_{}", name));
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    let (src, exe, ir) = (dir.join("prog.typl"), dir.join("prog"), dir.join("prog.ll"));
    std::fs::write(&src, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_typl"))
        .args(["-c", src.to_str().unwrap(), "-o", exe.to_str().unwrap()])
        .args(flags)
        .env("TYPELISP_AOT_IR", &ir)
        .output()
        .expect("failed to start the typl binary");
    assert!(out.status.success(), "typl -c failed: {}", String::from_utf8_lossy(&out.stderr));
    let ran = Command::new(&exe).output().expect("failed to run the executable");
    (ran, std::fs::read_to_string(&ir).expect("TYPELISP_AOT_IR was not written"))
}

/// The line defining the function whose name ends in `name`, and its body.
fn function<'a>(ir: &'a str, name: &str) -> (&'a str, &'a str) {
    let head = format!("{}\"(", name);
    let at = ir.lines().find(|l| l.starts_with("define ") && l.contains(&head)).unwrap_or_else(|| panic!("no function {} in the IR", name));
    let start = ir.find(at).unwrap();
    let end = ir[start..].find("\n}\n").expect("a function body ends") + start;
    (at, &ir[start + at.len()..end])
}

fn is_coroutine(ir: &str, name: &str) -> bool {
    function(ir, name).1.contains("@rt_frame_open")
}

/// A value is passed and answered at its type's width, a `()` not at all,
/// and the body neither opens a frame nor asks for its arguments.
#[test]
fn a_direct_function_has_its_own_signature() {
    let (ran, ir) = built(
        "signature",
        r#"
(defun add8 ((a i8) (b i8)) i8 (+ a b))
(defun half ((a f32) (b bool) (c char) (u ())) f32 (if b (/ a (the f32 2.0)) (the f32 0.0)))
(defun twice ((a f64)) f64 (+ a a))
(defun spin ((n u16)) ()
  (let ((i (the u16 0))) (while (< i n) (setf i (+ i (the u16 1))))))
(defun main () int
  (spin (the u16 5))
  (if (and (eq (add8 (the i8 100) (the i8 100)) (the i8 -56))
           (eq (half (the f32 3.0) true #\a ()) (the f32 1.5))
           (eq (twice 2.5) 5.0))
      42 1))
"#,
        &[],
    );
    assert_eq!(ran.status.code(), Some(42), "{}", String::from_utf8_lossy(&ran.stderr));
    for (name, head) in [
        ("::add8", "define i8 @\"tl_prog::add8\"(i8 %0, i8 %1)"),
        ("::half", "define float @\"tl_prog::half\"(float %0, i1 %1, i32 %2)"),
        ("::twice", "define double @\"tl_prog::twice\"(double %0)"),
        ("::spin", "define void @\"tl_prog::spin\"(i16 %0)"),
    ] {
        let (line, body) = function(&ir, name);
        assert!(line.starts_with(head), "{}", line);
        for shim in ["@rt_frame_open", "@rt_local_frame_open", "@rt_pending_arg", "@rt_loop_safepoint", "@rt_frame_call"] {
            assert!(!body.contains(shim), "{} in {}:\n{}", shim, name, body);
        }
    }
}

/// What can stop stays a coroutine, and so does what calls it: a
/// suspension, a catch, a call through a value, a recursion (which is as
/// deep as the heap is large, and would otherwise be as deep as the machine
/// stack) — and a loop, when there is another task to give a turn to.
#[test]
fn what_can_stop_is_a_coroutine() {
    let (ran, ir) = built(
        "stops",
        r#"
(defun naps () () (sleep 0.001))
(defun calls-naps () () (naps))
(defun spins () () (let ((i 0)) (while (< i 3) (setf i (+ i 1)))))
(defun catches () int (catch 'done (throw 'done 1)))
(defun calls-value ((f (fn (int) int))) int (f 1))
(defun depth ((n int)) int (if (eq n 0) 0 (+ 1 (depth (- n 1)))))
(defun plain ((n int)) int (+ n 1))
(defun main () int
  (calls-naps)
  (spins)
  (if (and (eq (catches) 1) (eq (calls-value plain) 2) (eq (depth 1000000) 1000000)) 42 1))
"#,
        &[],
    );
    assert_eq!(ran.status.code(), Some(42), "{}", String::from_utf8_lossy(&ran.stderr));
    for name in ["::naps", "::calls-naps", "::spins", "::catches", "::calls-value", "::depth", "::main"] {
        assert!(is_coroutine(&ir, name), "{}", name);
    }
    assert!(!is_coroutine(&ir, "::plain"));
}

/// A direct function's locals that the collector can reach are roots while
/// it runs, at every level: the arena is too small to hold the garbage.
#[test]
fn a_direct_functions_locals_are_roots() {
    let (ran, ir) = built(
        "roots",
        r#"
(defun garbage ((n i32)) ()
  (let ((i (the i32 0)))
    (while (< i n) (list 7 8 9) (append "a" "b") (setf i (+ i (the i32 1))))))
(defun inner ((l string)) int
  (let ((mine (append l "abcd")))
    (garbage (the i32 5000))
    (if (equal mine "xyz?abcd") (+ (length l) (length mine)) 0)))
(defun outer ((s string)) int
  (let ((l (append "xyz" "?")) (t (append s "!")))
    (garbage (the i32 5000))
    (+ (inner l) (length t))))
(defun main () int (+ (outer "1234567") 22))
"#,
        &["--heap-cells", "4000"],
    );
    assert_eq!(ran.status.code(), Some(42), "{}", String::from_utf8_lossy(&ran.stderr));
    for name in ["::inner", "::outer"] {
        assert!(!is_coroutine(&ir, name), "{}", name);
        assert!(function(&ir, name).1.contains("@rt_local_frame_open"), "{}", name);
    }
}

/// A raise leaves direct functions one by one, closing each one's frame, to
/// the coroutine that catches it, which goes on; and one nobody catches ends
/// the program with its message.
#[test]
fn a_raise_leaves_direct_functions() {
    let (ran, ir) = built(
        "raise",
        r#"
(defun deep ((s string)) int (let ((t (append s "!"))) (throw 'done (length t)) 0))
(defun middle ((s string)) int (let ((t (append s "?"))) (+ 1000 (deep t))))
(defun once ((s string)) int (catch 'done (middle s)))
(defun fails ((a i32) (b i32)) i32 (/ a b))
(defun main () int
  (let ((sum 0) (i 0))
    (while (< i 1000)
      (setf sum (+ sum (once "12345")))
      (setf i (+ i 1)))
    (println "~a" sum))
  (fails (the i32 1) (the i32 0))
  0)
"#,
        &[],
    );
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "7000\n");
    assert_eq!(ran.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&ran.stderr).contains("zero"), "{}", String::from_utf8_lossy(&ran.stderr));
    for name in ["::deep", "::middle", "::fails"] {
        assert!(!is_coroutine(&ir, name), "{}", name);
    }
    assert!(is_coroutine(&ir, "::once"));
}
