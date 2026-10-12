//! A method with type parameters of its own: `(impl Box<T> (map<U> (self
//! (f (fn (T) U))) Box<U> ...))`.
//!
//! The receiver supplies the owner's type arguments; the method's own are
//! inferred from the other arguments, as a generic `defun`'s are. Each
//! instantiation is a separate specialization, so every case runs
//! interpreted and compiled: the compiled side calls the specialization by
//! its mangled name, which now carries both kinds of argument.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

const DEFS: &str = r##"
(defstruct Box<T> (v T))
(impl Box<T>
  (fmap<U> (self (f (fn (T) U))) Box<U>
    (make-instance 'Box :v (f self::v)))
  (zip-with<U,R> (self (other Box<U>) (f (fn (T U) R))) Box<R>
    (make-instance 'Box :v (f self::v other::v)))
  (pair<U> ((a T) (b U)) Box<cons-cell<T,U>>
    (make-instance 'Box :v (cons a b))))
(defstruct Meter (m f64))
(impl Meter
  (scale<N> (self (k N) (conv (fn (N) f64))) Meter
    (make-instance 'Meter :m (* self::m (conv k)))))
(impl Box<T>
  (twice<U> (self (f (fn (T) U)) (g (fn (U) U))) Box<U>
    (fmap (fmap self f) g))
  (same<U> (self (a U) (b U)) bool (where (Eq U))
    (equals a b)))
(defun apply-it ((g (fn (Box<int> (fn (int) bool)) Box<bool>))) bool
  (v (g (make-instance 'Box :v 5) (lambda ((x int)) bool (> x 2)))))
"##;

/// `body` (a `string`-valued expression) after [`DEFS`], interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DEFS, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", DEFS, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

/// The owner's parameter comes from the receiver, the method's own from the
/// function argument.
#[test]
fn the_receivers_and_the_methods_own_parameters_together() {
    both(r##"(v (fmap (make-instance 'Box :v 3) (lambda ((x int)) string (format false "n=~a" x))))"##, "n=3");
}

/// Two instantiations of the same method are two specializations, and each
/// gets its own types.
#[test]
fn each_instantiation_is_its_own() {
    both(
        r##"(let ((a (fmap (make-instance 'Box :v "abc") (lambda ((s string)) int (length s))))
                 (b (fmap (make-instance 'Box :v 4) (lambda ((n int)) bool (> n 3)))))
             (format false "~a ~a" (+ (v a) 1) (v b)))"##,
        "4 true",
    );
}

#[test]
fn several_own_parameters() {
    both(
        r##"(v (zip-with (make-instance 'Box :v 2) (make-instance 'Box :v "x")
                         (lambda ((n int) (s string)) string (format false "~a~a" s n))))"##,
        "x2",
    );
}

/// A static method: no receiver, so the arguments decide the owner's
/// parameter and the method's own alike.
#[test]
fn a_static_method() {
    both(r##"(let ((p (v (Box::pair 1 "one")))) (format false "~a ~a" (car p) (cdr p)))"##, "1 one");
}

/// The owner need not be generic: the method's own parameters alone make it
/// a template.
#[test]
fn an_owner_without_type_parameters() {
    both(r##"(format false "~a" (m (scale (make-instance 'Meter :m 2.0) 3 (lambda ((n int)) f64 (int->float n)))))"##, "6.0");
}

/// A method that forwards its own parameter to another such method: the
/// inner call is specialized when the outer one is.
#[test]
fn forwarding_to_another_generic_method() {
    both(
        r##"(v (twice (make-instance 'Box :v 5)
                      (lambda ((n int)) string (format false "~a" n))
                      (lambda ((s string)) string (format false "~a!" s))))"##,
        "5!",
    );
}

/// A `where` bound on the method's own parameter is checked at the call.
#[test]
fn a_bound_on_the_methods_own_parameter() {
    both(r##"(format false "~a ~a" (same (make-instance 'Box :v 0) 1 1) (same (make-instance 'Box :v 0) "a" "b"))"##, "true false");
}

/// Passed as a function value, the expected type decides every parameter.
#[test]
fn as_a_function_value() {
    both(r##"(format false "~a" (apply-it fmap))"##, "true");
}

#[test]
fn a_parameter_nothing_decides_is_an_error() {
    let msg = check_err(&format!(
        "{}\n(impl Box<T> (make<U> (self) Option<U> (Option::none)))\n(defun f () () (make (make-instance 'Box :v 1)) ())",
        DEFS
    ));
    assert!(msg.contains("cannot infer type parameter `u`"), "{}", msg);
}

#[test]
fn an_own_parameter_may_not_reuse_the_owners_name() {
    let msg = check_err("(defstruct Box<T> (v T))\n(impl Box<T> (bad<T> (self (x T)) T x))");
    assert!(msg.contains("the method's own type parameter `t` has the name of a type parameter of `box`"), "{}", msg);
    // The declared name counts too, though this receiver spells it `A`: the
    // registered signature is written in the declared names.
    let msg = check_err("(defstruct Box<T> (v T))\n(impl Box<A> (bad<T> (self (x T)) T x))");
    assert!(msg.contains("the method's own type parameter `t`"), "{}", msg);
}

#[test]
fn an_own_parameter_may_not_be_declared_twice() {
    let msg = check_err("(defstruct Box<T> (v T))\n(impl Box<T> (bad<U,U> (self (x U)) U x))");
    assert!(msg.contains("type parameter `u` is declared twice"), "{}", msg);
}

/// `Pair<T,int>` names one of two variables, and specialization would have
/// nothing to bind it with.
#[test]
fn a_partly_generic_receiver_is_refused() {
    let msg = check_err("(defstruct Pair<A,B> (a A) (b B))\n(impl Pair<T,int> (bad<U> (self (x U)) U x))");
    assert!(msg.contains("needs a receiver that names every type parameter of `pair`"), "{}", msg);
}

/// A name the header does not declare is still unknown.
#[test]
fn an_undeclared_type_variable_is_still_unknown() {
    let msg = check_err("(defstruct Box<T> (v T))\n(impl Box<T> (bad<U> (self (x W)) U x))");
    assert!(msg.contains("unknown type `w`"), "{}", msg);
}

/// A trait declares its methods' signatures, and those have no type
/// parameters of their own, so an `impl` cannot add one.
#[test]
fn an_impl_method_cannot_add_type_parameters() {
    let msg = check_err(
        "(deftrait Show () (show ((self Self)) string))\n(defstruct P (x int))\n(impl Show P (show<U> ((self P)) string \"p\"))",
    );
    assert!(msg.contains("`show<u>` is not a method of"), "{}", msg);
}
