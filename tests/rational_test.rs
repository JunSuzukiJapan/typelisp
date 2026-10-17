//! `rational` — CL's `rational`, an `int` or a `ratio`: what `int`'s `/`
//! answers, what arithmetic mixing an `int` with a `ratio` answers, and the
//! integer quotients (`truncate`/`floor`/`ceiling`/`round`) that took over
//! `int`'s old truncating `/`.

mod common;

use common::{check_err, eval_string, eval_string_compiled};

/// `(format false "~a" EXPR)` against the prelude.
fn show(expr: &str) -> String {
    eval_string(&format!("(format false \"~a\" {})", expr))
}

/// CL's `/` on two integers: the exact quotient, an integer when it is one.
#[test]
fn integer_division_is_rational() {
    assert_eq!(show("(/ 7 2)"), "7/2");
    assert_eq!(show("(/ 6 3)"), "2");
    assert_eq!(show("(/ -1 3)"), "-1/3");
    assert_eq!(show("(/ 100000000000000000000 3)"), "100000000000000000000/3");
    assert_eq!(show("(let ((a 7) (b 2)) (/ a b))"), "7/2");
}

/// An `int` and a `ratio` meet in a `rational`, in either order.
#[test]
fn mixing_int_and_ratio_is_rational() {
    assert_eq!(show("(+ 1/2 1)"), "3/2");
    assert_eq!(show("(+ 1 1/2)"), "3/2");
    assert_eq!(show("(* 2 1/2)"), "1");
    assert_eq!(show("(- 1 1/3)"), "2/3");
    assert_eq!(show("(/ 1/2 2)"), "1/4");
    assert_eq!(show("(< 1/2 1)"), "true");
    assert_eq!(show("(= 2 4/2)"), "true");
    assert_eq!(show("(max 1 3/2)"), "3/2");
    assert_eq!(show("(+ (/ 1 2) (/ 1 3) 1)"), "11/6");
    assert_eq!(show("(let ((r 1/2)) (+ r 1))"), "3/2");
}

/// The integer quotients CL's `/` no longer gives: `truncate`, `floor`,
/// `ceiling` and `round` (ties to even), each with a required divisor.
#[test]
fn integer_quotients() {
    assert_eq!(show("(truncate 7 2)"), "3");
    assert_eq!(show("(truncate -7 2)"), "-3");
    assert_eq!(show("(floor -7 2)"), "-4");
    assert_eq!(show("(floor 7 -2)"), "-4");
    assert_eq!(show("(ceiling 7 2)"), "4");
    assert_eq!(show("(ceiling 6 2)"), "3");
    assert_eq!(show("(round 5 2)"), "2");
    assert_eq!(show("(round 7 2)"), "4");
    assert_eq!(show("(round -5 2)"), "-2");
    assert_eq!(show("(round 7 3)"), "2");
}

/// Back out of a `rational`: `try-as int` asks whether it is an integer,
/// `as int` truncates, `as ratio` always succeeds.
#[test]
fn converting_a_rational() {
    assert_eq!(show("(try-as int (/ 8 2))"), "(some 4)");
    assert_eq!(show("(try-as int (/ 7 2))"), "none");
    assert_eq!(show("(as int (/ -7 2))"), "-3");
    assert_eq!(show("(as ratio (/ 4 2))"), "2");
    assert_eq!(show("(as f64 (/ 1 4))"), "0.25");
    assert_eq!(show("(as rational 5)"), "5");
    assert_eq!(show("(as rational 1/2)"), "1/2");
}

/// The rational catalog, as CL has it.
#[test]
fn rational_methods() {
    assert_eq!(show("(floor (/ 7 2))"), "3");
    assert_eq!(show("(round (/ 5 2))"), "2");
    assert_eq!(show("(numerator (/ 6 4))"), "3");
    assert_eq!(show("(denominator (/ 6 4))"), "2");
    assert_eq!(show("(abs (/ -1 3))"), "1/3");
    assert_eq!(show("(signum (/ -1 3))"), "-1");
    assert_eq!(show("(expt (/ 2 3) 2)"), "4/9");
    assert_eq!(show("(expt (/ 4 2) 3)"), "8");
    assert_eq!(show("(sqrt (/ 1 4))"), "0.5");
    assert_eq!(show("(float (/ 1 4))"), "0.25");
    assert_eq!(show("(rational 0.5)"), "1/2");
    assert_eq!(show("(zerop (- (/ 1 2) 1/2))"), "true");
    assert_eq!(show("(1+ (/ 1 2))"), "3/2");
    assert_eq!(show("(mod (/ -7 2) 2)"), "1/2");
    assert_eq!(
        show("(let ((d (floor-div (/ 7 2) 1/3))) (format false \"~a ~a\" (car d) (cdr d)))"),
        "10 1/6"
    );
}

/// A `rational` is a hash key, and the same value is the same key however it
/// was computed.
#[test]
fn rational_is_a_hash_key() {
    assert_eq!(
        show("(let ((h (the HashTable<rational,int> (HashTable::new)))) (set h (/ 1 2) 1) (set h (/ 4 2) 2) (format false \"~a ~a\" (get h 1/2) (get h 2)))"),
        "(some 1) (some 2)"
    );
}

/// `Div`'s `Output`: an `int` divides into a `rational`, so a generic
/// function that divides within `T` does not take one.
#[test]
fn div_names_its_output() {
    assert_eq!(
        eval_string(
            "(defun quot<T,R> ((a T) (b T)) R (:where (Div T (Output R))) (/ a b))
             (format false \"~a ~a\" (the rational (quot 7 2)) (the f64 (quot 7.0 2.0)))"
        ),
        "7/2 3.5"
    );
    let msg = check_err(
        "(defun halve<T> ((a T) (b T)) T (:where (Number T) (Div T (Output T))) (/ a b))
         (halve 7 2)",
    );
    assert!(msg.contains("associated type `output` is `rational`"), "{}", msg);
    assert_eq!(
        eval_string("(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T)) (+ a b c)) (format false \"~a\" (sum3 (/ 1 2) (/ 1 3) 1))"),
        "11/6"
    );
}

/// Compiled code agrees with the interpreter on every path above.
#[test]
fn compiled_rational_agrees_with_the_interpreter() {
    let src = r#"
        (defun f ((a int) (b int)) rational (+ (/ a b) 1/3))
        (defun g ((q rational)) Option<int> (try-as int q))
        (defun h ((a int) (r ratio)) bool (< a r))
        (defun k ((q rational)) int (round q))
        (defun m ((a int) (b int)) int (floor a b))
        (defun n ((r ratio)) rational (* r 2))
    "#;
    let call = r#"(format false "~a ~a ~a ~a ~a ~a ~a ~a ~a ~a"
                    (f 1 2) (f 2 3) (f 100000000000000000000 3)
                    (g (/ 6 3)) (g (/ 7 3)) (h 1 3/2) (k (/ 7 2)) (m -7 2) (n 1/2) (n 1/3))"#;
    let interpreted = eval_string(&format!("{src}\n{call}"));
    assert_eq!(interpreted, "5/6 1 100000000000000000001/3 (some 2) none true 4 -4 1 2/3");
    let compiled = eval_string_compiled(&format!(
        "{src}\n(compile f)\n(compile g)\n(compile h)\n(compile k)\n(compile m)\n(compile n)\n{call}"
    ));
    assert_eq!(compiled, interpreted);
}
