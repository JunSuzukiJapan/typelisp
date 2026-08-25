//! What `match` can compare, beyond a constructor and an immediate word.
//!
//! Three additions, one mechanism:
//!
//! * **Literals with no immediate word** — `string`, `f64`, `symbol`,
//!   `bignum`, `ratio`. Their machine word is a pointer or a box id, so the
//!   `pat-lit` word comparison that serves `int`/`char`/`bool` is *identity*
//!   for them, and identity would mean `"a"` never matching a separately
//!   built `"a"`.
//! * **`(= expr)`** — an arbitrary expression, for the types with no literal
//!   syntax at all (a `defstruct` instance, a global, a computed value).
//! * **A bare variant name** — `(match c (red 1) (blue 2))`, which used to
//!   bind a variable called `red` that matched everything.
//!
//! The first two lower to the same `Pattern::Guard`: a checked `bool`
//! expression, `(equals $match-scrut EXPR)`, resolved through the ordinary
//! instance-method path. So the comparison is the *type's own* `Eq` impl —
//! a user type compares the way its author said it does, and a type with no
//! `Eq` is a type error rather than an arm that silently never matches.
//!
//! Each behavioural case is checked in both worlds: interpreted, and again
//! after `(compile f)`, because a value pattern is the one pattern whose test
//! is a *call* rather than an instruction, and because a scalar scrutinee is
//! the first `match` scrutinee that must not be pushed as a GC root.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

/// Run `src` with the prelude and the compiler island loaded, returning the
/// last form's value as an `i64`.
///
/// The prelude is not optional the way it is for a `pat-lit` test: `Eq` — the
/// trait every value pattern's test calls through — is defined there. The
/// island is not optional either, since half of these run `(compile f)`.
fn run(src: &str) -> Result<i64, String> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    typelisp::compile::install_llvm_backend();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| e.to_string())?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| e.to_string())?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| EvalError::into_kind(e).to_string())? {
            last = val;
        }
    }
    match last {
        Value::Int(n) => Ok(n),
        other => Err(format!("expected an integer result, got {:?}", other)),
    }
}

fn ok(src: &str) -> i64 {
    run(src).unwrap_or_else(|e| panic!("failed: {}\nsource:\n{}", e, src))
}

fn err(src: &str) -> String {
    match run(src) {
        Ok(v) => panic!("expected an error, got {}\nsource:\n{}", v, src),
        Err(e) => e,
    }
}

/// The same program interpreted and compiled must answer the same.
///
/// `(compile f)` sits after the definitions and before the call, so the
/// island translates the whole body, patterns included — which is what says a
/// value pattern's test survives the crossing.
fn both(defs: &str, call: &str, expected: i64) {
    assert_eq!(ok(&format!("{}\n{}", defs, call)), expected, "interpreted");
    assert_eq!(ok(&format!("{}\n(compile f)\n{}", defs, call)), expected, "compiled");
}

/// Three answers packed into one integer, so a single call site can cover
/// "matches", "matches the other arm" and "falls through".
const fn digits(a: i64, b: i64, c: i64) -> i64 {
    a * 100 + b * 10 + c
}

// ---- string ---------------------------------------------------------------

#[test]
fn a_string_literal_pattern_compares_content() {
    both(
        r#"(defun f ((s string)) i32 (match s ("a" 1) ("bb" 2) (_ 0)))"#,
        r#"(+ (* 100 (f "a")) (+ (* 10 (f "bb")) (f "zz")))"#,
        digits(1, 2, 0),
    );
}

/// Content, not identity: `append` builds its result at run time, so the
/// string under test is a different object from the one in the pattern.
#[test]
fn a_string_pattern_matches_a_separately_built_string() {
    both(
        r#"(defun f ((s string)) i32 (match s ("ab" 1) (_ 0)))"#,
        r#"(f (append "a" "b"))"#,
        1,
    );
}

// ---- symbol ---------------------------------------------------------------

/// A symbol *does* have a compiled representation — an interned id in a
/// tagged word — and a literal one is materialized by interning its name at
/// run time (`rt_intern_symbol`), never by baking an id into the code. So the
/// comparison is identity on both sides, and identity is content equality
/// because the symbol table is what makes it so.
#[test]
fn a_symbol_literal_pattern_compares_the_interned_symbol() {
    both(
        "(defun f ((s symbol)) i32 (match s ('foo 1) ('bar 2) (_ 0)))",
        r#"(+ (* 100 (f (string->symbol "foo")))
              (+ (* 10 (f (string->symbol "bar"))) (f (string->symbol "zz"))))"#,
        digits(1, 2, 0),
    );
}

/// The same literal one level down, where it is most useful: pulling one
/// specific symbol out of read data.
#[test]
fn a_symbol_literal_pattern_works_inside_a_sexpr_pattern() {
    both(
        r#"(defun f ((s Sexpr)) i32 (match s ((sym 'foo) 1) ((str "hi") 2) ((int 42) 3) (_ 0)))"#,
        r#"(+ (* 100 (f 'foo)) (+ (* 10 (f "hi")) (f 'zz)))"#,
        digits(1, 2, 0),
    );
}

/// A literal against a `Sexpr` scrutinee, with no variant pattern around it.
///
/// `Eq` on `sexpr` is `eq` (`prelude.rs`) — CL's identity — which *is*
/// content equality for every immediate: an interned symbol, an integer, a
/// `char`, a `bool`. So these four need no `(sym ...)`/`(int ...)` wrapper.
#[test]
fn immediate_literals_match_a_sexpr_scrutinee_directly() {
    both(
        r#"(defun f ((s Sexpr)) i32
             (match s ('foo 1) (42 2) (#\a 3) (true 4) (_ 0)))"#,
        r#"(+ (* 1000 (f 'foo)) (+ (* 100 (f 42)) (+ (* 10 (f #\a)) (f true))))"#,
        1234,
    );
}

/// The literals that are *not* immediates are refused there instead.
///
/// A `string`/`f64`/`bignum`/`ratio` reaches compiled code as a pointer or a
/// box id, so `eq` on one is the identity of that object: the arm would
/// type-check and never match anything built separately. The error names the
/// variant pattern, which destructures to the scalar's own type and compares
/// *that* by value.
#[test]
fn a_by_identity_literal_against_a_sexpr_scrutinee_is_refused() {
    let e = err(r#"(defun f ((s Sexpr)) i32 (match s ("hi" 1) (_ 0)))
                   (f "hi")"#);
    assert!(e.contains("compare by identity"), "{}", e);
    assert!(e.contains(r#"(str "hi")"#), "{}", e);
    // And the named spelling does match, by content.
    both(
        r#"(defun f ((s Sexpr)) i32 (match s ((str "hi") 1) (_ 0)))"#,
        r#"(+ (* 10 (f "hi")) (f "no"))"#,
        10,
    );
}

/// `(= expr)` against a `Sexpr` scrutinee is *not* refused: identity is what
/// `eq` means, and an author who wrote `equals` in as many words is asking
/// for exactly that.
#[test]
fn an_equals_pattern_against_a_sexpr_scrutinee_is_allowed() {
    both(
        "(defvar (target Sexpr) 42)\
         \n(defun f ((s Sexpr)) i32 (match s ((= target) 1) (_ 0)))",
        "(+ (* 10 (f 42)) (f 1))",
        10,
    );
}

// ---- float / bignum / ratio ----------------------------------------------

#[test]
fn a_float_literal_pattern_compares_by_value() {
    both(
        "(defun f ((x f64)) i32 (match x (1.5 1) (2.5 2) (_ 0)))",
        "(+ (* 100 (f 1.5)) (+ (* 10 (f 2.5)) (f 9.0)))",
        digits(1, 2, 0),
    );
}

/// A `bignum` and a `ratio` are boxed like an `f64`, and equally not
/// comparable by their word.
#[test]
fn bignum_and_ratio_literal_patterns_compare_by_value() {
    both(
        "(defun f ((x bignum)) i32 (match x (123456789012345678901234567890 1) (_ 0)))\
         \n(defun g ((x ratio)) i32 (match x (1/3 1) (_ 0)))",
        "(+ (* 100 (f 123456789012345678901234567890)) (+ (* 10 (g 1/3)) (g 1/2)))",
        digits(1, 1, 0),
    );
}

// ---- (= expr) -------------------------------------------------------------

/// The escape into ordinary evaluation, and the answer for every type with no
/// literal syntax. The comparison is the type's own `Eq` impl: `point`'s says
/// two points are equal when their fields are, so a *separately built*
/// `(point::new 1 2)` matches.
#[test]
fn an_equals_pattern_compares_a_user_type_through_its_own_eq_impl() {
    both(
        "(defstruct point (x i32) (y i32))\
         \n(impl Eq point (equals ((self Self) (other Self)) bool\
         \n  (and (= self::x other::x) (= self::y other::y))))\
         \n(defvar (origin point) (point::new 0 0))\
         \n(defun f ((p point)) i32\
         \n  (match p ((= origin) 1) ((= (point::new 1 2)) 2) (_ 0)))",
        "(+ (* 100 (f (point::new 0 0))) (+ (* 10 (f (point::new 1 2))) (f (point::new 9 9))))",
        digits(1, 2, 0),
    );
}

/// A generic body can use a value pattern under a `where (Eq A)` bound.
///
/// The body is checked once with `A` still a type variable, where there is
/// no `AdtDef` to ask for an `equals` — the bound is the promise, and the
/// call becomes a real one when monomorphization re-checks the body at
/// `string`. `f` is the monomorphic caller, so `(compile f)` pulls the
/// instantiation in.
#[test]
fn a_value_pattern_works_in_a_generic_body_under_an_eq_bound() {
    both(
        "(defun same<A> ((x A) (y A)) i32 (where (Eq A))\
         \n  (match x ((= y) 1) (_ 0)))\
         \n(defun f ((a string) (b string)) i32 (same a b))",
        r#"(+ (* 10 (f "a" "a")) (f "a" "b"))"#,
        10,
    );
}

/// A type with no `Eq` impl cannot be compared, and says so at the pattern
/// rather than accepting an arm that never matches.
#[test]
fn a_type_without_eq_is_refused_at_the_pattern() {
    let e = err(
        "(defstruct opaque (x i32))\
         \n(defvar (o opaque) (opaque::new 1))\
         \n(defun f ((p opaque)) i32 (match p ((= o) 1) (_ 0)))\
         \n(f o)",
    );
    assert!(e.contains("does not implement `Eq`"), "{}", e);
}

// ---- bare variant names ---------------------------------------------------

/// `(match c (red 1) (blue 2))` — the arms are variants, not two variables
/// that each match everything.
///
/// The old reading was not merely surprising: a bind pattern is a catch-all,
/// so the first arm won every time *and* satisfied the exhaustiveness check,
/// which is why nothing reported the second arm as unreachable.
#[test]
fn a_bare_variant_name_is_that_variant() {
    both(
        "(defenum color (red) (blue))\
         \n(use color)\
         \n(defun f ((c color)) i32 (match c (red 1) (blue 2)))",
        "(+ (* 10 (f (red))) (f (blue)))",
        12,
    );
}

/// Bare names cover the variants, so no `_` is needed — which is the point of
/// their being variants at all.
#[test]
fn bare_variant_names_count_toward_exhaustiveness() {
    let e = err(
        "(defenum color (red) (blue))\
         \n(use color)\
         \n(defun f ((c color)) i32 (match c (red 1)))\
         \n(f (red))",
    );
    assert!(e.contains("non-exhaustive"), "{}", e);
}

/// A variant that carries fields still has to be written with them; the arity
/// check is what says so.
#[test]
fn a_bare_name_for_a_variant_with_fields_is_an_arity_error() {
    let e = err(
        "(defenum shape (dot) (circle i32))\
         \n(use shape)\
         \n(defun f ((s shape)) i32 (match s (dot 0) (circle 1)))\
         \n(f (dot))",
    );
    assert!(e.contains("field"), "{}", e);
}

/// A name that is *not* a variant of the scrutinee's type is still a binding,
/// which is what keeps every existing `(match v (x ...))` meaning what it did.
#[test]
fn a_bare_name_that_names_no_variant_still_binds() {
    both(
        "(defenum color (red) (blue))\
         \n(use color)\
         \n(defun f ((n i32)) i32 (match n (blues (+ blues 1))))",
        "(f 41)",
        42,
    );
}

// ---- scrutinees with no variants -----------------------------------------

/// A `match` on a type with no variants is legal — that is what gives a
/// string literal a pattern position — but it can never be exhaustive by
/// enumeration, so it needs a catch-all.
#[test]
fn a_scrutinee_with_no_variants_needs_a_catch_all() {
    let e = err(r#"(defun f ((s string)) i32 (match s ("a" 1)))"#);
    assert!(e.contains("non-exhaustive"), "{}", e);
}

/// An `i32` scrutinee: no heap value anywhere, which is what makes it the
/// case the compiled side had to stop rooting unconditionally.
#[test]
fn a_scalar_scrutinee_matches_by_literal() {
    both("(defun f ((n i32)) i32 (match n (1 10) (2 20) (_ 0)))", "(+ (f 1) (+ (f 2) (f 3)))", 30);
}

/// Two guards in one pattern: each reads the value *it* was handed, not the
/// one its sibling was. They share a name (`$match-scrut`) deliberately —
/// see `Pattern::Guard` — so this is the test that says sharing it is safe.
#[test]
fn two_value_patterns_in_one_pattern_do_not_read_each_others_value() {
    both(
        r#"(defstruct pair (a string) (b string))
           (defun f ((p pair)) i32 (match p ((new "x" "y") 1) ((new "x" _) 2) (_ 0)))"#,
        r#"(+ (* 100 (f (pair::new "x" "y"))) (+ (* 10 (f (pair::new "x" "z"))) (f (pair::new "q" "y"))))"#,
        digits(1, 2, 0),
    );
}

/// A guard's test is an ordinary expression, so it can name anything in scope
/// at the arm — including the enclosing function's parameters.
#[test]
fn a_value_pattern_can_read_the_enclosing_scope() {
    both(
        "(defun f ((s string) (want string)) i32 (match s ((= want) 1) (_ 0)))",
        r#"(+ (* 10 (f "a" "a")) (f "a" "b"))"#,
        10,
    );
}
