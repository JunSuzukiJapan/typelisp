//! Source and data nested far deeper than a thread's stack holds.
//!
//! The reader, the checker, the compiler's passes and the printers each
//! recurse once per level of nesting, so how deep a program may nest used to
//! be decided by the size of the thread it ran on — a test thread's 2MB gave
//! out at a dozen nested `dlet`s on arm64. Each of those walks now grows its
//! stack where it recurses (`typelisp_mem::grow_stack`). These run on the
//! ordinary test thread on purpose: a walk that loses its growth overflows
//! here, and aborts the whole binary rather than failing one test.
//!
//! The nested `let`s check memory as much as stack: the checker's and the
//! compiler's scopes used to copy every enclosing binding at every level, and
//! twenty thousand of them took 26GB.

extern crate typelisp;

mod common;
use common::{eval_ok, eval_ok_compiled, is_true};
use typelisp::Value;

const DEPTH: usize = 20_000;

/// `(+ 1 (+ 1 ... BASE))`, `depth` deep.
fn nested_sum(depth: usize, base: &str) -> String {
    format!("{}{}{}", "(+ 1 ".repeat(depth), base, ")".repeat(depth))
}

/// `(1 (1 ... ()))` as quoted data, `depth` deep.
fn nested_data(depth: usize) -> String {
    format!("(quote {}{})", "(1 ".repeat(depth), ")".repeat(depth))
}

#[test]
fn a_deeply_nested_expression_is_read_checked_and_run() {
    assert_eq!(eval_ok(&nested_sum(DEPTH, "0")), Value::Int(DEPTH as i64));
}

#[test]
fn deeply_nested_lets_are_checked_in_linear_space() {
    let src = format!("{}x{}", "(let ((x 1)) ".repeat(DEPTH), ")".repeat(DEPTH));
    assert_eq!(eval_ok(&src), Value::Int(1));
}

#[test]
fn a_deeply_nested_macro_call_expands() {
    let src = format!(
        "(defmacro inc (x) (list (quote +) 1 x)) {}0{}",
        "(inc ".repeat(DEPTH),
        ")".repeat(DEPTH)
    );
    assert_eq!(eval_ok(&src), Value::Int(DEPTH as i64));
}

#[test]
fn deeply_nested_data_prints() {
    // `(1 ` per level, and the `)` closing each.
    let expected = 4 * DEPTH - 1;
    is_true(&format!("(= (length (prin1-to-string {})) {})", nested_data(DEPTH), expected));
}

#[test]
fn deeply_nested_data_pretty_prints() {
    // The pretty printer indents each level a column further, so its output
    // grows with the square of the depth; a fifth of the depth is still far
    // past what the stack held.
    let depth = DEPTH / 5;
    is_true(&format!(
        "(> (length (dlet ((*print-pretty* true)) (prin1-to-string {}))) {})",
        nested_data(depth),
        4 * depth
    ));
}

#[test]
fn deeply_nested_data_compares_equal() {
    is_true(&format!("(equal {} {})", nested_data(DEPTH), nested_data(DEPTH)));
    is_true(&format!("(equalp {} {})", nested_data(DEPTH), nested_data(DEPTH)));
}

#[test]
fn a_deeply_nested_function_body_compiles() {
    // Compiled code keeps a slot per intermediate value in its own frame, so
    // this depth is kept to what that frame fits on a test thread; the walks
    // that translate it are what is under test.
    let depth = DEPTH / 4;
    let src = format!(
        "(defun f ((a int)) int {}) (compile f) (f 0)",
        nested_sum(depth, "a")
    );
    assert_eq!(eval_ok_compiled(&src), Value::Int(depth as i64));
    let src = format!(
        "(defun g ((a int)) int {}x{}) (compile g) (g 7)",
        "(let ((x a)) ".repeat(depth),
        ")".repeat(depth)
    );
    assert_eq!(eval_ok_compiled(&src), Value::Int(7));
}
