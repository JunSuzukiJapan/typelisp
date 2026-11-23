//! `#[interrupt(N)]` and `#[exception(NAME)]`: what the checker takes as an
//! interrupt handler, and what the interpreter makes of one — an ordinary
//! function, since there are no interrupts off a board. A board runs them in
//! `aot_embedded_test`.

extern crate typelisp;
mod common;
use common::{run_compiled_display_err as eval};
use typelisp::Value;

fn err(src: &str) -> String {
    match eval(src) {
        Err(e) => e,
        Ok(_) => panic!("expected a failure, but the program ran"),
    }
}

#[test]
fn a_handler_is_an_ordinary_function_off_a_board() {
    let src = "(defvar (*n* int) 0)
               #[interrupt(7)] (defun on-seven () () (setf *n* (+ *n* 1)) ())
               #[exception(SysTick)] (defun on-tick () () (setf *n* (+ *n* 10)) ())
               (on-seven) (on-tick) *n*";
    assert!(matches!(eval(src).expect("evaluation failed").1, Value::Int(11)));
}

#[test]
fn a_handler_takes_nothing_and_returns_unit() {
    let e = err("#[interrupt(7)] (defun f ((x int)) () x)");
    assert!(e.contains("cannot take parameters"), "unexpected error: {}", e);
    let e = err("#[interrupt(7)] (defun f () int 1)");
    assert!(e.contains("must return `()`"), "unexpected error: {}", e);
    let e = err("#[exception(SysTick)] (defvar (x int) 1)");
    assert!(e.contains("goes on a `defun`"), "unexpected error: {}", e);
}

#[test]
fn the_interrupt_and_the_exception_must_exist() {
    let e = err("#[interrupt(240)] (defun f () () ())");
    assert!(e.contains("numbered from 0 to 239"), "unexpected error: {}", e);
    let e = err("#[exception(HardFault)] (defun f () () ())");
    assert!(e.contains("`SysTick`") && e.contains("#[exception(HardFault)]"), "unexpected error: {}", e);
    let e = err("#[interrupt 7] (defun f () () ())");
    assert!(e.contains("`#[interrupt(N)]`"), "unexpected error: {}", e);
    let e = err("#[irq(7)] (defun f () () ())");
    assert!(e.contains("unknown attribute `irq`"), "unexpected error: {}", e);
}

#[test]
fn one_interrupt_has_one_handler() {
    let e = err("#[interrupt(7)] (defun f () () ()) #[interrupt(7)] (defun g () () ())");
    assert!(e.contains("is handled by") && e.contains("already"), "unexpected error: {}", e);
    let e = err("#[interrupt(7) exception(PendSV)] (defun f () () ())");
    assert!(e.contains("a function handles one interrupt"), "unexpected error: {}", e);
    let e = err("#[test interrupt(7)] (defun f () () ())");
    assert!(e.contains("a test is not an interrupt handler"), "unexpected error: {}", e);
}

/// A handler written again replaces itself.
#[test]
fn a_handler_can_be_defined_again() {
    let src = "#[interrupt(7)] (defun f () () ()) #[interrupt(7)] (defun f () () ()) 1";
    assert!(matches!(eval(src).expect("evaluation failed").1, Value::Int(1)));
}

#[test]
fn enabling_an_interrupt_off_a_board_is_a_runtime_error() {
    let e = err("(use std::board::enable-interrupt) (enable-interrupt 3)");
    assert!(e.contains("there are interrupts only on a board"), "unexpected error: {}", e);
}
