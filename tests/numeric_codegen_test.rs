//! A fixed-width integer and an `f32` are computed at their own width in
//! compiled code: `(+ a b)` on two `u8`s is an `add i8`, not a 64-bit add
//! cut back to eight bits, and a `u32` divides with `udiv`.
//!
//! The value still travels in a 64-bit word, so each operation starts by
//! cutting the word down (`trunc`) and ends by extending the result back;
//! what this pins is that the operation in between is the type's own.

use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    dir
}

/// Compiles `source` to a native executable, runs it, and answers its exit
/// code and the IR of the whole program as the compiler emitted it
/// (`TYPELISP_AOT_IR`, before LLVM's passes).
fn compiled_ir(name: &str, source: &str) -> (Option<i32>, String) {
    let dir = scratch(name);
    let (src, exe, ir) = (dir.join("prog.typl"), dir.join("prog"), dir.join("prog.ll"));
    std::fs::write(&src, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_typl"))
        .args(["-c", src.to_str().unwrap(), "-o", exe.to_str().unwrap()])
        .env("TYPELISP_AOT_IR", &ir)
        .output()
        .expect("failed to start the typl binary");
    assert!(out.status.success(), "typl -c failed: {}", String::from_utf8_lossy(&out.stderr));
    let code = Command::new(&exe).status().expect("failed to run the executable").code();
    (code, std::fs::read_to_string(&ir).expect("TYPELISP_AOT_IR was not written"))
}

/// The body of the function whose name ends in `name` in `ir`.
fn body<'a>(ir: &'a str, name: &str) -> &'a str {
    let head = format!("::{}\"(", name);
    let start = ir.find(&head).unwrap_or_else(|| panic!("no function {} in the IR", name));
    let end = ir[start..].find("\n}\n").expect("a function body ends") + start;
    &ir[start..end]
}

/// For each type: the instructions its `+`, `/` and `<` must be, at its
/// width and with its signedness.
#[test]
fn each_fixed_width_computes_in_its_own_type() {
    let cases = [
        ("i8", "add i8", "sdiv i8", "icmp slt i8"),
        ("u8", "add i8", "udiv i8", "icmp ult i8"),
        ("i16", "add i16", "sdiv i16", "icmp slt i16"),
        ("u16", "add i16", "udiv i16", "icmp ult i16"),
        ("i32", "add i32", "sdiv i32", "icmp slt i32"),
        ("u32", "add i32", "udiv i32", "icmp ult i32"),
    ];
    for (t, add, div, lt) in cases {
        let source = format!(
            "(defun plus ((a {t}) (b {t})) {t} (+ a b))
(defun quotient ((a {t}) (b {t})) {t} (/ a b))
(defun less ((a {t}) (b {t})) bool (< a b))
(defun main () int
  (if (less (quotient (plus (the {t} 100) (the {t} 26)) (the {t} 3)) (the {t} 43)) 7 9))"
        );
        let (code, ir) = compiled_ir(&format!("numeric_codegen_{}", t), &source);
        assert_eq!(code, Some(7), "{}: 126 / 3 is 42", t);
        for (f, op) in [("plus", add), ("quotient", div), ("less", lt)] {
            let b = body(&ir, f);
            assert!(b.contains(op), "{}: `{}` has no `{}`:\n{}", t, f, op, b);
            // The 64-bit computation this replaced cut each result back with a
            // shift pair.
            assert!(!b.contains("shl i64") && !b.contains("ashr i64"), "{}: `{}` cuts a 64-bit result:\n{}", t, f, b);
        }
    }
}

/// An `f32` operation is a `float` one, an `f64` one a `double` one.
#[test]
fn each_float_width_computes_in_its_own_type() {
    for (t, op, sqrt) in [("f32", "fadd float", "@llvm.sqrt.f32"), ("f64", "fadd double", "@llvm.sqrt.f64")] {
        let source = format!(
            "(defun plus ((a {t}) (b {t})) {t} (+ a b))
(defun root ((a {t})) {t} (sqrt a))
(defun main () int (if (= (root (plus (the {t} 7.0) (the {t} 9.0))) (the {t} 4.0)) 7 9))"
        );
        let (code, ir) = compiled_ir(&format!("numeric_codegen_{}", t), &source);
        assert_eq!(code, Some(7), "{}", t);
        assert!(body(&ir, "plus").contains(op), "{}:\n{}", t, body(&ir, "plus"));
        assert!(body(&ir, "root").contains(sqrt), "{}:\n{}", t, body(&ir, "root"));
    }
}
