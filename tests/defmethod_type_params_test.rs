//! A `defmethod` with type parameters of its own: `(defmethod map<U> ((self
//! Box<T>) (f (fn (T) U))) Box<U> ...)`.
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
(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))
(defmethod zip-with<U,R> ((self Box<T>) (other Box<U>) (f (fn (T U) R))) Box<R>
  (Box::new (f self::v other::v)))
(defmethod pair<U> (Box<T> (a T) (b U)) Box<cons-cell<T,U>>
  (Box::new (cons a b)))
(defstruct Meter (m f64))
(defmethod scale<N> ((self Meter) (k N) (conv (fn (N) f64))) Meter
  (Meter::new (* self::m (conv k))))
(defmethod opt-map<U> ((self Option<T>) (f (fn (T) U))) Option<U>
  (match self ((some x) (Option::some (f x))) (none (Option::none))))
(defmethod twice<U> ((self Box<T>) (f (fn (T) U)) (g (fn (U) U))) Box<U>
  (fmap (fmap self f) g))
(defmethod same<U> ((self Box<T>) (a U) (b U)) bool (where (Eq U))
  (equals a b))
(defun apply-it ((g (fn (Box<int> (fn (int) bool)) Box<bool>))) bool
  (v (g (Box::new 5) (lambda ((x int)) bool (> x 2)))))
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
    both(r##"(v (fmap (Box::new 3) (lambda ((x int)) string (format false "n=~a" x))))"##, "n=3");
}

/// Two instantiations of the same method are two specializations, and each
/// gets its own types.
#[test]
fn each_instantiation_is_its_own() {
    both(
        r##"(let ((a (fmap (Box::new "abc") (lambda ((s string)) int (length s))))
                 (b (fmap (Box::new 4) (lambda ((n int)) bool (> n 3)))))
             (format false "~a ~a" (+ (v a) 1) (v b)))"##,
        "4 true",
    );
}

#[test]
fn several_own_parameters() {
    both(
        r##"(v (zip-with (Box::new 2) (Box::new "x")
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
    both(r##"(format false "~a" (m (scale (Meter::new 2.0) 3 (lambda ((n int)) f64 (int->float n)))))"##, "6.0");
}

/// A built-in generic owner.
#[test]
fn on_option() {
    both(
        r##"(format false "~a ~a"
             (unwrap (opt-map (Option::some 20) (lambda ((n int)) string (format false "<~a>" n))))
             (is-none (opt-map (the Option<int> (Option::none)) (lambda ((n int)) bool (> n 0)))))"##,
        "<20> true",
    );
}

/// A method that forwards its own parameter to another such method: the
/// inner call is specialized when the outer one is.
#[test]
fn forwarding_to_another_generic_method() {
    both(
        r##"(v (twice (Box::new 5)
                      (lambda ((n int)) string (format false "~a" n))
                      (lambda ((s string)) string (format false "~a!" s))))"##,
        "5!",
    );
}

/// A `where` bound on the method's own parameter is checked at the call.
#[test]
fn a_bound_on_the_methods_own_parameter() {
    both(r##"(format false "~a ~a" (same (Box::new 0) 1 1) (same (Box::new 0) "a" "b"))"##, "true false");
}

/// Passed as a function value, the expected type decides every parameter.
#[test]
fn as_a_function_value() {
    both(r##"(format false "~a" (apply-it fmap))"##, "true");
}

#[test]
fn a_parameter_nothing_decides_is_an_error() {
    let msg = check_err(&format!(
        "{}\n(defmethod make<U> ((self Box<T>)) Option<U> (Option::none))\n(defun f () () (make (Box::new 1)) ())",
        DEFS
    ));
    assert!(msg.contains("cannot infer type parameter `u`"), "{}", msg);
}

#[test]
fn an_own_parameter_may_not_reuse_the_owners_name() {
    let msg = check_err("(defstruct Box<T> (v T))\n(defmethod bad<T> ((self Box<T>) (x T)) T x)");
    assert!(msg.contains("the method's own type parameter `t` has the name of a type parameter of `box`"), "{}", msg);
    // The declared name counts too, though this receiver spells it `A`: the
    // registered signature is written in the declared names.
    let msg = check_err("(defstruct Box<T> (v T))\n(defmethod bad<T> ((self Box<A>) (x T)) T x)");
    assert!(msg.contains("the method's own type parameter `t`"), "{}", msg);
}

#[test]
fn an_own_parameter_may_not_be_declared_twice() {
    let msg = check_err("(defstruct Box<T> (v T))\n(defmethod bad<U,U> ((self Box<T>) (x U)) U x)");
    assert!(msg.contains("type parameter `u` is declared twice"), "{}", msg);
}

/// `Pair<T,int>` names one of two variables, and specialization would have
/// nothing to bind it with.
#[test]
fn a_partly_generic_receiver_is_refused() {
    let msg = check_err("(defstruct Pair<A,B> (a A) (b B))\n(defmethod bad<U> ((self Pair<T,int>) (x U)) U x)");
    assert!(msg.contains("needs a receiver that names every type parameter of `pair`"), "{}", msg);
}

/// A name the header does not declare is still unknown.
#[test]
fn an_undeclared_type_variable_is_still_unknown() {
    let msg = check_err("(defstruct Box<T> (v T))\n(defmethod bad<U> ((self Box<T>) (x W)) U x)");
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
