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

/// A program that calls nothing of the runtime's has a `main` that calls
/// nothing of it; one that does, and waits for nothing, calls its own `main`
/// with nothing to drive it. Both answer as `typl` would, an `int` and `()`
/// alike, and a raise out of `main` ends the program as one.
#[test]
fn a_main_that_stops_nowhere_is_called_directly() {
    let bare = r#"
(defun add ((a i32) (b i32)) i32 (+ a b))
(defun main () int (if (eq (add (the i32 1) (the i32 2)) (the i32 3)) 42 -1))
"#;
    let heap = r#"
(defun len ((s string)) int (length (append s "abc")))
(defun main () int (+ 20 (len "0123456789012345678")))
"#;
    let unit = "(defun f ((s string)) int (length (append s s)))\n(defun main () () (f \"ab\") ())";
    let fails = "(defun f ((a i32)) i32 (/ a (the i32 0)))\n(defun main () int (f (the i32 1)) 7)";
    for (name, source, code, runtime) in
        [("bare", bare, 42, None), ("heap", heap, 42, Some("@rt_main_answered")), ("unit", unit, 0, Some("@rt_main_returned")), ("fails", fails, 1, Some("@rt_main_answered"))]
    {
        let (ran, ir) = built(&format!("main_{}", name), source, &[]);
        assert_eq!(ran.status.code(), Some(code), "{}: {}", name, String::from_utf8_lossy(&ran.stderr));
        let at = ir.find("define i32 @main()").expect("a `main`");
        let main = &ir[at..at + ir[at..].find("\n}\n").unwrap()];
        assert!(main.contains("::main$direct\""), "{}:\n{}", name, main);
        assert!(!main.contains("@rt_run_program"), "{}:\n{}", name, main);
        match runtime {
            Some(shim) => assert!(main.contains(shim), "{}:\n{}", name, main),
            None => assert!(!main.contains("@rt_"), "{}:\n{}", name, main),
        }
    }
}

/// A volatile access of a constant address is an instruction, and after a
/// call that cannot raise there is no check for a raise — at `-O0` too.
#[test]
fn what_is_settled_needs_no_runtime() {
    let (_, ir) = built(
        "settled",
        r#"
(defun poke () () (unsafe (volatile-write-u32 #x40004000 #x2A)))
(defun peek () u8 (unsafe (volatile-read-u8 #x40004001)))
(defun bad () () (unsafe (volatile-write-u32 #x40004002 1)))
(defun three () i32 (the i32 3))
(defun main () int (if (eq (three) (the i32 4)) (progn (poke) (peek) (bad) 1) 42))
"#,
        &[],
    );
    let (_, poke) = function(&ir, "::poke");
    assert!(poke.contains("store volatile i32 42, ptr inttoptr (i64 1073758208 to ptr)") && !poke.contains("call"), "{}", poke);
    let (_, peek) = function(&ir, "::peek");
    assert!(peek.contains("load volatile i8") && !peek.contains("call"), "{}", peek);
    // A misaligned register raises, as it does everywhere.
    assert!(function(&ir, "::bad").1.contains("@rt_c_volatile_store"));
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

/// A program that calls no `eval` has nothing of the checker or the
/// interpreter in its executable.
#[test]
fn without_eval_there_is_no_interpreter() {
    let symbols = |name: &str, source: &str| {
        let (ran, _) = built(name, source, &[]);
        assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
        let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/aot-test-tmp").join(format!("direct_{}", name)).join("prog");
        let prefix = std::env::var("LLVM_SYS_221_PREFIX").expect("LLVM_SYS_221_PREFIX names the LLVM typl is built with");
        let out = Command::new(PathBuf::from(prefix).join("bin").join("llvm-nm"))
            .args(["--defined-only", "--demangle", "--format=just-symbols"])
            .arg(exe)
            .output()
            .expect("failed to start llvm-nm");
        assert!(out.status.success(), "llvm-nm failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect::<Vec<_>>()
    };
    let interpreter = |names: &[String]| names.iter().filter(|n| n.contains("typelisp_front::typeck::") || n.contains("typelisp_front::eval::")).count();
    assert_eq!(interpreter(&symbols("no_eval", r#"(defun main () () (println "~a" (list 1 2)))"#)), 0);
    assert_ne!(interpreter(&symbols("eval", "(use std::eval::eval) (defun main () () (eval '(+ 1 2)) ())")), 0);
}
