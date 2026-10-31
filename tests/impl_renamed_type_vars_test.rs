//! A trait's `impl` may spell the target type's variables differently from
//! its declaration (`(defstruct Box<T> ...)` then `(impl Sh Box<U> ...)`), as
//! an inherent `impl` may. Every case runs interpreted and compiled.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

/// `body` (a `string`-valued expression), interpreted and inside a compiled
/// function, both expected to give `expected`.
fn both(defs: &str, body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", defs, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", defs, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

const BOX: &str = "(defstruct Box<T> (v T))";

#[test]
fn a_method_of_an_impl_with_renamed_variables() {
    both(
        &format!(
            "{}\n{}",
            BOX,
            r##"(deftrait Sh () (sh (self) string))
                (impl Sh Box<U> (sh (self) string "box"))
                (defun s<C> ((c C)) string (:where (Sh C)) (sh c))"##
        ),
        r##"(format false "~a ~a" (sh (make-instance 'Box<int> :v 3)) (s (make-instance 'Box<string> :v "a")))"##,
        "box box",
    );
}

/// The associated type names the renamed variable, and a bound that pins it
/// reads it as the owner's argument.
#[test]
fn an_associated_type_bound_to_a_renamed_variable() {
    both(
        &format!(
            "{}\n{}",
            BOX,
            r##"(deftrait Get () (:type Item) (get-it (self) Item))
                (impl Get Box<U> (:type Item U) (get-it (self) U self::v))
                (defun g<C,X> ((c C)) X (:where (Get C (Item X))) (get-it c))"##
        ),
        r##"(format false "~a ~a"
              (+ 1 (g (make-instance 'Box<int> :v 3)))
              (+ 1 (get-it (make-instance 'Box<int> :v 4))))"##,
        "4 5",
    );
}

#[test]
fn a_trait_object_of_an_impl_with_renamed_variables() {
    both(
        &format!(
            "{}\n{}",
            BOX,
            r##"(deftrait Sh () (sh (self) string))
                (impl Sh Box<U> (sh (self) string "box"))"##
        ),
        r##"(let ((d (the :dyn Sh (make-instance 'Box<int> :v 3)))) (sh d))"##,
        "box",
    );
}

/// The `impl`'s `(:where ...)` bounds the renamed variable, and holds for the
/// owner's argument in that position.
#[test]
fn an_impl_bound_on_a_renamed_variable() {
    let defs = format!(
        "{}\n{}",
        BOX,
        r##"(deftrait Sh () (sh (self) string))
            (impl Sh Box<U> (:where (Hash U)) (sh (self) string "box"))
            (defun s<C> ((c C)) string (:where (Sh C)) (sh c))"##
    );
    both(&defs, r##"(s (make-instance 'Box<int> :v 3))"##, "box");
    let msg = check_err(&format!("{}\n(defun f () string (s (make-instance 'Box<f64> :v 3.0)))", defs));
    assert!(msg.contains("`Box<f64>` does not implement trait"), "{}", msg);
}
