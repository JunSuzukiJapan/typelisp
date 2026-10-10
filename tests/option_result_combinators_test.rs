//! Rust's combinators on `Option`/`Result` (`map` `and-then` `or-else`
//! `map-err` `ok-or` `unwrap-or-else` `expect`) and the `->` macro that
//! chains them. Every case runs interpreted and compiled.

mod common;
use common::{check_err, eval_err, eval_string, eval_string_compiled};

const DEFS: &str = r##"
(defun half ((n int)) Option<int> (if (= 0 (mod n 2)) (Option::some (/ n 2)) (Option::none)))
(defun parse ((s string)) Result<int,string>
  (if (equal s "1") (Result::ok 1) (Result::err (format false "bad: ~a" s))))
(defun none-int () Option<int> (Option::none))
"##;

/// `body` (a `string`-valued expression) after [`DEFS`], interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DEFS, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!(
        "{}\n(defun go () string {})\n(compile half)\n(compile parse)\n(compile none-int)\n(compile go)\n(go)",
        DEFS, body
    ));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

#[test]
fn option_map_changes_the_element_type() {
    both(
        r##"(format false "~s ~s"
             (map (Option::some 3) (lambda ((x int)) string (format false "<~a>" x)))
             (map (none-int) (lambda ((x int)) string "never")))"##,
        r##"(some "<3>") none"##,
    );
}

#[test]
fn option_and_then_and_or_else() {
    both(
        r##"(format false "~s ~s ~s ~s"
             (and-then (Option::some 8) half)
             (and-then (Option::some 3) half)
             (or-else (none-int) (lambda () Option<int> (Option::some 5)))
             (or-else (Option::some 1) (lambda () Option<int> (Option::some 5))))"##,
        "(some 4) none (some 5) (some 1)",
    );
}

#[test]
fn option_ok_or_unwrap_or_else_and_expect() {
    both(
        r##"(format false "~s ~s ~s ~s"
             (ok-or (none-int) "missing")
             (ok-or (Option::some 2) "missing")
             (unwrap-or-else (none-int) (lambda () int 42))
             (expect (Option::some 1) "never"))"##,
        r##"(err "missing") (ok 2) 42 1"##,
    );
}

#[test]
fn result_map_and_map_err() {
    both(
        r##"(format false "~s ~s ~s"
             (map (parse "1") (lambda ((x int)) int (* x 10)))
             (map-err (parse "2") (lambda ((e string)) int (length e)))
             (map-err (parse "1") (lambda ((e string)) int (length e))))"##,
        "(ok 10) (err 6) (ok 1)",
    );
}

#[test]
fn result_and_then_or_else_and_unwrap_or_else() {
    both(
        r##"(format false "~s ~s ~s ~s"
             (and-then (parse "1") (lambda ((x int)) Result<int,string> (Result::ok (+ x 1))))
             (and-then (parse "2") (lambda ((x int)) Result<int,string> (Result::ok (+ x 1))))
             (or-else (parse "z") (lambda ((e string)) Result<int,int> (Result::ok 0)))
             (unwrap-or-else (parse "z") (lambda ((e string)) int (length e))))"##,
        r##"(ok 2) (err "bad: 2") (ok 0) 6"##,
    );
}

/// Each step gets the value so far as its first argument; a bare symbol is a
/// call with that one argument.
#[test]
fn the_arrow_threads_the_first_argument() {
    both(
        r##"(format false "~s ~s ~s"
             (-> (Option::some 8) (and-then half) (and-then half) (map (lambda ((x int)) int (+ x 100))))
             (-> (Option::some 6) (and-then half) (and-then half) (unwrap-or 0))
             (-> 5 (+ 1) (* 2) list))"##,
        "(some 102) 0 (12)",
    );
}

#[test]
fn expect_panics_with_its_message() {
    let msg = eval_err(&format!("{}\n(expect (parse \"q\") \"parse failed\")", DEFS));
    assert!(msg.contains("parse failed"), "{}", msg);
    let msg = eval_err(&format!("{}\n(expect (none-int) \"no value\")", DEFS));
    assert!(msg.contains("no value"), "{}", msg);
}

#[test]
fn the_function_must_take_the_element_type() {
    let msg = check_err("(defun f () Option<int> (map (Option::some 1) (lambda ((s string)) int 0)))");
    assert!(msg.contains("string") && msg.contains("int"), "{}", msg);
}
