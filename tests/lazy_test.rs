//! The `lazy` module's iterator adapters and `collect`. An adapter produces
//! an element only when asked, so an infinite source works as long as
//! something downstream stops. Every case runs interpreted and compiled.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

const DEFS: &str = r##"
(defun v () Vector<int> (the Vector<int> #(1 2 3 4 5)))
(defun double ((x int)) int (* x 2))
"##;

/// `body` (a `string`-valued expression) after [`DEFS`], interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DEFS, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!(
        "{}\n(defun go () string {})\n(compile v)\n(compile double)\n(compile go)\n(go)",
        DEFS, body
    ));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

#[test]
fn map_and_filter() {
    both(
        r##"(format false "~s ~s"
             (collect (lazy::map (iter (v)) double))
             (collect (lazy::filter (iter (v)) (lambda ((x int)) bool (= 0 (mod x 2))))))"##,
        "#(2 4 6 8 10) #(2 4)",
    );
}

/// `iterate` and `repeat` never end; `take` and `take-while` are what stop
/// them.
#[test]
fn infinite_sources_stop_downstream() {
    both(
        r##"(format false "~s ~s ~s"
             (collect (lazy::take (lazy::iterate 1 double) 5))
             (collect (lazy::take-while (lazy::iterate 1 double) (lambda ((x int)) bool (< x 20))))
             (collect (lazy::take (lazy::repeat "a") 2)))"##,
        r##"#(1 2 4 8 16) #(1 2 4 8 16) #("a" "a")"##,
    );
}

#[test]
fn skip_and_chain() {
    both(
        r##"(format false "~s ~s"
             (collect (lazy::skip (iter (v)) 3))
             (collect (lazy::chain (iter (v)) (lazy::take (lazy::repeat 0) 2))))"##,
        "#(4 5) #(1 2 3 4 5 0 0)",
    );
}

/// `enumerate` and `zip` give tuples, which a tuple pattern takes apart.
#[test]
fn enumerate_and_zip_give_tuples() {
    both(
        r##"(let ((s 0))
             (doiter (#{i x} (lazy::enumerate (iter (v)))) (setf s (+ s (* i x))))
             (format false "~s ~s ~a"
               (collect (lazy::enumerate (iter (the Vector<string> #("a" "b")))))
               (collect (lazy::zip (iter (v)) (lazy::take (lazy::repeat "z") 2)))
               s))"##,
        r##"#(#{0 "a"} #{1 "b"}) #(#{1 "z"} #{2 "z"}) 40"##,
    );
}

#[test]
fn flat_map_concatenates_the_inner_iterators() {
    both(
        r##"(format false "~s"
             (collect (lazy::flat-map (iter (the Vector<int> #(1 2 3)))
                        (lambda ((n int)) lazy::take-iter<lazy::repeat-iter<int>,int>
                          (lazy::take (lazy::repeat n) n)))))"##,
        "#(1 2 2 3 3 3)",
    );
}

/// An adapter is an `Iter`, so the eager combinators take one too.
#[test]
fn eager_combinators_take_an_adapter() {
    both(
        r##"(format false "~s ~a"
             (map (lazy::take (lazy::iterate 3 double) 3) (lambda ((x int)) string (format false "<~a>" x)))
             (foldl (lazy::take (lazy::iterate 1 double) 4) (lambda ((a int) (x int)) int (+ a x)) 0))"##,
        r##"#("<3>" "<6>" "<12>") 15"##,
    );
}

/// The source is asked for nothing beyond what is taken: the mapping
/// function runs once per element `take` lets through.
#[test]
fn nothing_is_computed_ahead() {
    both(
        r##"(let ((calls 0))
             (collect (lazy::take (lazy::map (lazy::iterate 1 double)
                                             (lambda ((x int)) int (setf calls (+ calls 1)) x))
                                  3))
             (format false "~a" calls))"##,
        "3",
    );
}

/// Inside an `impl`, a `where` pin on one of the impl's own parameters says
/// what that parameter's associated type is — the shape every adapter above
/// is written in.
#[test]
fn an_impl_where_pin_types_the_source() {
    let src = r##"
(defstruct every-other-iter<I,A> (src I))
(impl Iter every-other-iter<I,A> (where (Iter I (Item A)))
  (type Item A)
  (next ((self Self)) Option<A>
    (next self::src)
    (next self::src)))
(defun every-other<I,A> ((it I)) every-other-iter<I,A> (where (Iter I (Item A))) (every-other-iter::new it))
(format false "~s" (collect (every-other (iter (the Vector<int> #(1 2 3 4))))))
"##;
    assert_eq!(eval_string(src), "#(2 4)");
}

#[test]
fn the_function_must_take_the_element_type() {
    let msg = check_err(
        r##"(defun f () Vector<int> (collect (lazy::map (iter (the Vector<int> #(1))) (lambda ((s string)) int 0))))"##,
    );
    assert!(msg.contains("string") && msg.contains("int"), "{}", msg);
}
