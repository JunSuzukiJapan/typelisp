//! Tests for the type checker: Sexpr -> typed AST + checking (step 3b/3c).
//!
//! Built-in data types `Option<T>` and `Sexpr` are pre-registered. The minimal
//! goal is that `match` over them type-checks: well-typed forms pass, and
//! non-exhaustive / type-mismatched forms are rejected.

extern crate typelisp;
use typelisp::check::core;
use typelisp::{load_prelude, Checker, Error, Heap, Interp, Path, Reader, TopLevelForm, Type, Value, MONO_BUNDLE_MODULE};

/// Peels the checker's synthetic monomorphization bundle (if any), returning
/// the primary form — always the bundle's *last* element (the
/// specializations it needs come first). A form that instantiates no generic
/// function is returned unchanged.
fn primary(h: &Heap, tl: TopLevelForm) -> TopLevelForm {
    if core::op(h, tl) != Some("module") {
        return tl;
    }
    // `(module PATH BODY...)` — the path decides whether this is *the* bundle.
    let fields = core::fields(h, tl).expect("a module's fields are a proper list");
    let is_bundle = match fields[0] {
        Value::Path(id) => typelisp::path_from_id(h, id) == Path::root(MONO_BUNDLE_MODULE),
        Value::Symbol(id) => h.symbol_name(id) == MONO_BUNDLE_MODULE,
        _ => false,
    };
    if is_bundle {
        *fields.last().expect("a monomorph bundle always ends with its primary form")
    } else {
        tl
    }
}

/// What a check leaves behind that a test can inspect once the helper's `Heap`
/// is gone: the primary form's tag (`"expr"`, `"defun"`, ...), that form printed
/// back as an s-expression, and — for an `expr` form — the type the checker
/// proved for it.
///
/// Not the form itself: a checked form is cons cells in the helper's own `Heap`.
/// Not the type off the form either — the type stops at the checker, so it comes
/// from `Checker::expr_type`.
#[derive(Debug)]
struct Checked {
    tag: String,
    printed: String,
    ty: Option<Type>,
}

fn finish(h: &Heap, chk: &Checker, tl: TopLevelForm) -> Checked {
    let tl = primary(h, tl);
    Checked {
        tag: core::op(h, tl).expect("every top-level form is a tagged list").to_string(),
        printed: core::print(h, tl),
        ty: chk.expr_type().cloned(),
    }
}

/// Read one datum and check it as a single top-level form.
fn form(src: &str) -> Result<Checked, Error> {
    let mut h = Heap::with_capacity(1024);
    let r = Reader::new();
    let v = r.read(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    // Strip any source-location wrapper so kind-based assertions still match.
    let tl = chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?;
    Ok(finish(&h, &chk, tl))
}

/// Check a sequence of forms (e.g. a defun followed by a use of it), returning
/// the result of the LAST form. Earlier forms (defuns) populate the registry.
fn program(src: &str) -> Result<Checked, Error> {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?);
    }
    Ok(finish(&h, &chk, last.expect("no forms")))
}

/// Like [`program`], but with the prelude loaded first — needed for `while`/
/// `dotimes`/`dolist`/`when`/`unless`/`and`/`or`/`cond`/`if-let`, which are
/// `defmacro`s in `src/prelude.rs` rather than checker-native special forms
/// (see that file's "loop/branch primitive reduction" comment).
fn program_with_prelude(src: &str) -> Result<Checked, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?);
    }
    Ok(finish(&h, &chk, last.expect("no forms")))
}

/// The declared return type of the root-level function `name` in `src`. Read
/// from the registry, the declaration's own home — the core form carries only
/// the return's *representation* (`check::repr::Repr`), deliberately coarser
/// than its type.
fn ret_of(src: &str, name: &str) -> Type {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        chk.check_form(&mut h, &interp, v).expect("check failed");
    }
    chk.registry()
        .fn_sig(&Path::root(name))
        .unwrap_or_else(|| panic!("`{}` was not registered", name))
        .ret
        .clone()
}

/// The synthesized type of a single expression form.
/// `Option<Sexpr>` — the type an S-expression datum has since the empty list
/// became `Option`'s `none` rather than a `Sexpr` variant. Bare `Sexpr` still
/// exists and means *non-empty*, so the two are not interchangeable in either
/// direction here: these tests assert which one a form actually produces.
fn opt_sexpr() -> Type {
    Type::Named(Path::root("option"), vec![Type::Named(Path::root("sexpr"), vec![])])
}

fn ty(src: &str) -> Type {
    let c = form(src).expect("check failed");
    c.ty.unwrap_or_else(|| panic!("expected expression, got a `{}` form", c.tag))
}

/// Like [`ty`], but with the prelude loaded first — see [`program_with_prelude`].
fn ty_with_prelude(src: &str) -> Type {
    let c = program_with_prelude(src).expect("check failed");
    c.ty.unwrap_or_else(|| panic!("expected expression, got a `{}` form", c.tag))
}

/// Assert that checking `src` fails with a `TypeError`.
fn assert_type_error(src: &str) {
    match form(src) {
        Err(Error::TypeError(_)) => {}
        other => panic!("expected TypeError, got {:?}", other),
    }
}

/// Like [`assert_type_error`], but with the prelude loaded first — see
/// [`program_with_prelude`].
fn assert_type_error_with_prelude(src: &str) {
    match program_with_prelude(src) {
        Err(Error::TypeError(_)) => {}
        other => panic!("expected TypeError, got {:?}", other),
    }
}

// ---- literals ---------------------------------------------------------------

#[test]
fn literal_types() {
    assert_eq!(ty("42"), Type::I32); // integer literals default to i32
    assert_eq!(ty("3.14"), Type::F64);
    assert_eq!(ty("true"), Type::Bool);
    assert_eq!(ty("#\\a"), Type::Char);
    assert_eq!(ty("\"hi\""), Type::Str);
    assert_eq!(ty("()"), Type::Unit);
}

// ---- Symbol -----------------------------------------------------------------

#[test]
fn gensym_is_typed_symbol() {
    // The original motivation: `gensym` returns a `Symbol`, not the
    // heterogeneous `Sexpr`. It is a prelude `defun` (over the prelude global
    // `*gensym-counter*`), hence `ty_with_prelude`.
    assert_eq!(ty_with_prelude("(gensym)"), Type::Symbol);
}

#[test]
fn symbol_string_bridges_are_typed() {
    assert_eq!(ty("(string->symbol \"x\")"), Type::Symbol);
    assert_eq!(ty_with_prelude("(symbol->string (gensym))"), Type::Str);
}

#[test]
fn symbol_is_accepted_where_sexpr_expected() {
    // A `Symbol` is a valid `Sexpr` datum, so it flows into a `list`/`sexpr-cons`
    // code position (the checker wraps it into `Sexpr::Sym`).
    assert_eq!(ty_with_prelude("(sexpr-cons (gensym) ())"), opt_sexpr());
}

// ---- if ---------------------------------------------------------------------

#[test]
fn if_expression() {
    assert_eq!(ty("(if true 1 2)"), Type::I32);
}

#[test]
fn if_condition_must_be_bool() {
    assert_type_error("(if 1 2 3)");
}

#[test]
fn if_branches_must_agree() {
    assert_type_error("(if true 1 true)");
}

// ---- let --------------------------------------------------------------------

#[test]
fn let_binding_infers_and_uses() {
    assert_eq!(ty("(let ((x 1)) x)"), Type::I32);
    assert_eq!(ty("(let ((x true) (y 2)) y)"), Type::I32);
}

#[test]
fn unbound_variable_errors() {
    assert_type_error("nope");
}

// ---- defun + call -----------------------------------------------------------

#[test]
fn defun_registers_and_checks_body() {
    let c = program("(defun id ((x i32)) i32 x)").unwrap();
    assert_eq!(c.tag, "defun");
    assert_eq!(ret_of("(defun id ((x i32)) i32 x)", "id"), Type::I32);
}

#[test]
fn defun_body_must_match_return_type() {
    assert!(matches!(
        program("(defun bad ((x i32)) bool x)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn call_checks_argument_and_result_types() {
    assert_eq!(ty_program("(defun id ((x i32)) i32 x) (id 7)"), Type::I32);
}

#[test]
fn call_rejects_wrong_argument_type() {
    assert!(matches!(
        program("(defun id ((x i32)) i32 x) (id true)"),
        Err(Error::TypeError(_))
    ));
}

/// Like `ty` but over a multi-form program (last form's type).
fn ty_program(src: &str) -> Type {
    let c = program(src).expect("check failed");
    c.ty.unwrap_or_else(|| panic!("expected expression, got a `{}` form", c.tag))
}

// ---- constructors -----------------------------------------------------------

#[test]
fn construct_some_infers_type_argument() {
    assert_eq!(ty("(option::some 1)"), Type::Named(Path::root("option"), vec![Type::I32]));
}

#[test]
fn sexpr_cons_yields_an_option_sexpr() {
    // `(sexpr-cons (Int 1) ())`: both slots are `Option<Sexpr>`, and `()`
    // adopts `none` there. (The free `cons`/`Cons` name is the generic
    // `cons<T,U>` pair now — the Sexpr cons cell is built through the
    // `sexpr-*` layer — Phase 4b.)
    assert_eq!(ty("(sexpr-cons (Int 1) ())"), opt_sexpr());
}

#[test]
fn nil_is_no_longer_a_sexpr_constructor() {
    // The empty list moved out of `Sexpr` and into `Option<Sexpr>`'s `none`.
    // `nil`'s variant slot is kept reserved rather than reused (so the IR's
    // variant numbering is untouched), which means `(Nil)` would otherwise
    // still *resolve* — hence an explicit gate with a migration message
    // rather than an "unknown constructor" error.
    let err = program("(Nil)").expect_err("`nil` should be retired");
    let msg = format!("{:?}", err);
    assert!(msg.contains("no longer a `Sexpr` constructor"), "unexpected error: {}", msg);
}

/// `Sexpr` widens into `Option<Sexpr>`; `Option<Sexpr>` does *not* narrow
/// into `Sexpr`.
///
/// The widening is free — the niche gives the two types the same bits, and a
/// `Sexpr` is by construction never the empty-list word. The narrowing is the
/// claim "this is not the empty list", and only a `match` or `unwrap` can
/// discharge it.
///
/// This asymmetry is the whole static-checking yield of moving the empty list
/// out of `Sexpr`, and it was silently absent at first: `Option<Sexpr>` is
/// `is_heap_repr` (an `option` is a `Sum` with variants), so the retype meant
/// for "store a user ADT in a `Sexpr` slot" claimed it too. The empty list
/// then reached a bare `Sexpr`, and a *ten-arm exhaustive* `match` on it fell
/// off the end at runtime — the checker having proved it could not.
#[test]
fn an_option_sexpr_does_not_narrow_to_a_bare_sexpr() {
    // Widening: fine, and free.
    assert!(program("(defun f ((s Option<Sexpr>)) i32 1) (f (Int 1))").is_ok());
    // Narrowing: rejected.
    let err = program("(defun f ((s Sexpr)) i32 1) (f (the Option<Sexpr> ()))")
        .expect_err("narrowing should be rejected");
    let msg = format!("{:?}", err);
    assert!(msg.contains("type mismatch"), "unexpected error: {}", msg);
    // A user ADT still widens into an S-expression slot — the rule this
    // exclusion is carved out of must stay intact.
    assert!(program(
        "(defstruct point (x i32)) (defun f ((s Option<Sexpr>)) i32 1) (f (point::new 1))"
    )
    .is_ok());
}

#[test]
fn the_empty_list_is_an_option_sexpr_none() {
    // `()` adopts `none` where an S-expression is expected — which is now the
    // same rule as "`()` adopts `Option::None` when an `Option<T>` is
    // expected", not a second special case beside it.
    assert_eq!(ty("(sexpr-cons () ())"), opt_sexpr());
    assert_eq!(ty("(the Option<Sexpr> ())"), opt_sexpr());
}

#[test]
fn construct_int_field_adopts_the_declared_width() {
    // Sexpr::Int holds an i64; the integer literal must adopt that type.
    assert_eq!(ty("(Int 5)"), Type::Named(Path::root("sexpr"), vec![]));
}

// ---- match ------------------------------------------------------------------

#[test]
fn match_option_exhaustive_unwrap_or() {
    // The canonical minimal goal: unwrap-or over Option<i32> type-checks.
    let src = "(defun unwrap-or ((opt Option<i32>) (default i32)) i32 \
                 (match opt \
                   ((Some v) v) \
                   ((None) default)))";
    assert_eq!(program(src).unwrap().tag, "defun");
    assert_eq!(ret_of(src, "unwrap-or"), Type::I32);
}

#[test]
fn match_non_exhaustive_is_rejected() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (match opt ((Some v) v)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

#[test]
fn match_wildcard_makes_exhaustive() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (match opt ((Some v) v) (_ 0)))";
    assert_eq!(program(src).expect("check failed").tag, "defun");
}

#[test]
fn match_arms_must_share_result_type() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (match opt ((Some v) v) ((None) true)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

#[test]
fn match_binds_constructor_fields() {
    // `v` is bound at i32 inside the Some arm, so returning it as i32 is fine.
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (match opt ((Some v) v) ((None) 0)))";
    assert_eq!(program(src).expect("check failed").tag, "defun");
}

/// A scrutinee that is *not* a data type is matched by value: literal
/// patterns and `(= expr)` guards, with no variants to count. That makes a
/// catch-all mandatory — the only way such a `match` is ever exhaustive.
///
/// This used to be refused outright ("expected a data type, found I32"),
/// which is also why a string literal had no legal pattern position.
#[test]
fn match_on_a_type_with_no_variants_needs_a_catch_all() {
    assert_eq!(ty("(match 1 (_ 0))"), Type::I32);
    assert_type_error("(match 1 (1 10))");
}

// ---- match arms fill each other's inference holes ---------------------------
//
// `(result::ok v)` fixes `T` and says nothing about `E`; `(result::err e)`
// does the reverse. Neither arm can type itself where the `match` has no
// expected type, so the arms are probed, their partial types merged, and the
// arms re-checked against the result (`Checker::check_match`'s B4).

#[test]
fn match_arms_pool_a_results_two_type_arguments() {
    let src = "(defun f ((r Result<i32, string>)) i32 \
                 (let ((out (match r \
                              ((ok v) (result::ok v)) \
                              ((err e) (result::err e))))) \
                   (match out ((ok v) v) ((err e) 0))))";
    assert_eq!(program(src).expect("check failed").tag, "defun");
}

#[test]
fn a_lone_unpinnable_arm_is_still_rejected() {
    // Nothing here determines `E`: the other arm diverges, so the pool the
    // second pass draws on is empty and the original error stands.
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (let ((out (match opt \
                              ((Some v) (result::ok v)) \
                              ((None) (panic \"no\"))))) \
                   0))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

#[test]
fn an_unpinnable_constructor_outside_a_match_is_still_rejected() {
    // The hole is a `match`-arm device only — everywhere else an
    // un-inferrable type argument is as fatal as it ever was.
    assert_type_error("(let ((r (result::ok 1))) 0)");
}

#[test]
fn a_probed_arm_takes_its_type_from_a_concrete_sibling() {
    // The `err` arm needs nothing inferred, so the `ok` arm's missing `E`
    // comes from an arm that is not itself a bare constructor.
    let src = "(defun f ((r Result<i32, string>) (d Result<i32, string>)) i32 \
                 (let ((out (match r \
                              ((ok v) (result::ok v)) \
                              ((err e) d)))) \
                   0))";
    assert_eq!(program(src).expect("check failed").tag, "defun");
}

#[test]
fn arms_of_different_shapes_are_still_rejected() {
    // A hole is filled only by the *same* type constructor: `Option` and
    // `Result` never merge, so neither arm ends up with an expectation and
    // both report what they could not infer.
    let src = "(defun f ((r Result<i32, string>)) i32 \
                 (let ((out (match r \
                              ((ok v) (result::ok v)) \
                              ((err e) (option::none))))) \
                   0))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

// ---- if-let -----------------------------------------------------------------

#[test]
fn if_let_binds_in_then_branch() {
    // if-let binding is `(pattern value)`: here pattern `(Some v)`, value `opt`.
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (if-let ((Some v) opt) v 0))";
    assert_eq!(program_with_prelude(src).expect("check failed").tag, "defun");
}

#[test]
fn if_let_branches_must_agree() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (if-let ((Some v) opt) v true))";
    assert!(matches!(program_with_prelude(src), Err(Error::TypeError(_))));
}

// ---- setf / while -----------------------------------------------------------

#[test]
fn setf_checks_against_variable_type() {
    assert_eq!(ty("(let ((x 0)) (setf x 9))"), Type::I32);
    assert_type_error("(let ((x 0)) (setf x true))");
}

#[test]
fn setf_unbound_variable_errors() {
    assert_type_error("(setf nope 1)");
}

#[test]
fn cannot_assign_to_constant() {
    let mut h = typelisp::Heap::with_capacity(1024);
    let r = Reader::new();
    let vs = r.read_all(&mut h, "(defconstant (k i32) 5) (setf k 6)").unwrap();
    let mut chk = Checker::new();
    let interp = typelisp::Interp::new();
    let mut result = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e.into_kind());
        }
    }
    assert!(matches!(result, Err(Error::TypeError(_))));
}

#[test]
fn while_condition_must_be_bool_and_is_unit() {
    assert_eq!(ty_with_prelude("(let ((i 0)) (while (< i 0) (setf i 1)))"), Type::Unit);
    assert_type_error_with_prelude("(let ((i 0)) (while 1 (setf i 1)))");
}

// ---- lambda -----------------------------------------------------------------

#[test]
fn lambda_has_function_type() {
    assert_eq!(
        ty("(lambda ((x i32)) i32 x)"),
        Type::Fn(vec![Type::I32], None, Box::new(Type::I32))
    );
}

#[test]
fn calling_a_non_function_errors() {
    assert_type_error("(let ((x 5)) (x 1))");
}

#[test]
fn apply_checks_argument_types() {
    assert_type_error("((lambda ((x i32)) i32 x) true)");
}

#[test]
fn named_function_has_function_type() {
    assert_eq!(
        ty_program("(defun inc ((x i32)) i32 (+ x 1)) inc"),
        Type::Fn(vec![Type::I32], None, Box::new(Type::I32))
    );
}

#[test]
fn dotimes_count_must_be_i32() {
    assert_type_error_with_prelude("(dotimes (i true) ())");
}

// ---- labels -------------------------------------------------------------------

#[test]
fn labels_function_has_function_type_in_its_own_body() {
    // `fact` calling itself recursively type-checks, proving its own name is
    // visible (with a function type) inside its own body — the gap a bare
    // `lambda` can't close.
    assert_eq!(
        ty("(labels ((fact ((n i32)) i32 (if (= n 0) 1 (* n (fact (- n 1)))))) fact)"),
        Type::Fn(vec![Type::I32], None, Box::new(Type::I32))
    );
}

#[test]
fn labels_rejects_a_call_with_the_wrong_argument_type() {
    assert_type_error("(labels ((f ((n i32)) i32 n)) (f true))");
}

#[test]
fn break_does_not_cross_labels_boundary() {
    assert_type_error("(loop (labels ((f () () (break))) (f)))");
}

// ---- &rest / apply (variadic functions) --------------------------------------

#[test]
fn defun_rest_has_a_variadic_function_type() {
    assert_eq!(
        ty_program("(defun f ((a i32) &rest (xs i32)) i32 a) f"),
        Type::Fn(vec![Type::I32], Some(Box::new(Type::I32)), Box::new(Type::I32))
    );
}

#[test]
fn defun_rest_with_no_fixed_params_has_a_variadic_function_type() {
    assert_eq!(
        ty_program("(defun f (&rest (xs i32)) i32 0) f"),
        Type::Fn(vec![], Some(Box::new(Type::I32)), Box::new(Type::I32))
    );
}

#[test]
fn lambda_rest_has_a_variadic_function_type() {
    assert_eq!(
        ty("(lambda ((a i32) &rest (xs i32)) i32 a)"),
        Type::Fn(vec![Type::I32], Some(Box::new(Type::I32)), Box::new(Type::I32))
    );
}

#[test]
fn rest_param_is_seen_as_a_sexpr_list_inside_the_body() {
    // `sexpr-car` only accepts an S-expression argument — type-checking
    // succeeds, proving `xs` is bound to an ordinary Lisp list inside the
    // body, not some homogeneous array type. `Option<Sexpr>` specifically:
    // a `&rest` that collected nothing *is* the empty list, which bare
    // `Sexpr` cannot spell.
    assert_eq!(
        form("(defun f ((a i32) &rest (xs i32)) Option<Sexpr> (sexpr-car xs))")
            .expect("check failed")
            .tag,
        "defun"
    );
    // `lambda`'s `&rest` binds the same type as `defun`'s — they drifted
    // apart once during the migration, which no test then caught.
    assert_eq!(
        ty("((lambda ((a i32) &rest (xs i32)) Option<Sexpr> (sexpr-car xs)) 1 2)"),
        opt_sexpr()
    );
}

#[test]
fn calling_a_rest_function_with_only_the_fixed_arguments_is_fine() {
    assert!(program("(defun f ((a i32) &rest (xs i32)) i32 a) (f 1)").is_ok());
}

#[test]
fn calling_a_rest_function_with_extra_arguments_is_fine() {
    assert!(program("(defun f ((a i32) &rest (xs i32)) i32 a) (f 1 2 3)").is_ok());
}

#[test]
fn calling_a_rest_function_with_too_few_fixed_arguments_is_a_type_error() {
    assert!(matches!(
        program("(defun f ((a i32) &rest (xs i32)) i32 a) (f)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn calling_a_rest_function_with_a_wrong_typed_extra_argument_is_a_type_error() {
    assert!(matches!(
        program("(defun f ((a i32) &rest (xs i32)) i32 a) (f 1 true)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn rest_must_be_followed_by_exactly_one_parameter_in_a_defun() {
    assert!(matches!(
        program("(defun f (&rest (xs i32) (y i32)) i32 0)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn generic_rest_function_infers_the_element_type() {
    assert_eq!(ty_program("(defun firstn<T> ((a T) &rest (xs T)) T a) (firstn 1 2 3)"), Type::I32);
}

#[test]
fn apply_calls_a_variadic_function_with_a_runtime_sexpr_list() {
    let src = "(defun f ((a i32) &rest (xs i32)) i32 a) \
               (apply f 1 (quote (2 3)))";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn apply_on_a_non_variadic_function_is_a_type_error() {
    assert!(matches!(
        program("(apply (lambda ((a i32)) i32 a) 1)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn apply_with_the_wrong_number_of_fixed_arguments_is_a_type_error() {
    // The lambda needs exactly one fixed argument (`a`) before the rest
    // list; this supplies zero.
    assert!(matches!(
        program("(apply (lambda ((a i32) &rest (xs i32)) i32 a) (quote ()))"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn apply_auto_wraps_a_scalar_rest_list_argument_into_sexpr() {
    // Was a `TypeError` before the Sexpr-user-ADT design plan
    // (`~/.claude/plans/async-conjuring-hanrahan.md`): `apply`'s trailing
    // "rest list" argument is checked against `expected = Sexpr`
    // (`Checker::check_apply_form`), which now goes through the same
    // scalar-auto-wrap fallback `(list 1 2)` does (`Checker::check_inner`'s
    // expected-type reconciliation, not scoped to `list`/`&rest` alone) —
    // `2` auto-wraps to `Sexpr::Int(2)`, same CL-conformant relaxation.
    // (A non-list shape there — as here — is still rejected, just at
    // *runtime*, the same way CL's own `apply` signals a runtime condition
    // for a malformed trailing list rather than a compile-time error.)
    assert_eq!(ty("(apply (lambda ((a i32) &rest (xs i32)) i32 a) 1 2)"), Type::I32);
}

// ---- cons / car / cdr / list / dolist ----------------------------------------

#[test]
fn sexpr_car_and_cdr_yield_an_option_sexpr() {
    assert_eq!(ty("(sexpr-car (sexpr-cons (Int 1) ()))"), opt_sexpr());
    assert_eq!(ty("(sexpr-cdr (sexpr-cons (Int 1) ()))"), opt_sexpr());
}

#[test]
fn sexpr_car_auto_wraps_a_scalar_argument_into_sexpr() {
    // Was a `TypeError` before the Sexpr-user-ADT design plan — `1` now
    // auto-wraps to `Sexpr::Int(1)` wherever `Sexpr` is expected (see
    // `list_elements_are_auto_wrapped_into_sexpr` below). `sexpr-car`'s own
    // "must be a `Cons`" requirement is still enforced, just at *runtime*
    // now (a `Sexpr::Int` isn't a cons) — the same CL-conformant shift
    // `apply_auto_wraps_a_scalar_rest_list_argument_into_sexpr` documents.
    assert_eq!(ty("(sexpr-car 1)"), opt_sexpr());
}

#[test]
fn sexpr_cons_usable_as_function_value() {
    let src = "(defun apply2 ((f (fn (Option<Sexpr> Option<Sexpr>) Option<Sexpr>)) \
                              (a Option<Sexpr>) (b Option<Sexpr>)) Option<Sexpr> (f a b)) \
               (apply2 sexpr-cons (Int 1) ())";
    assert_eq!(ty_program(src), opt_sexpr());
}

#[test]
fn list_builds_sexpr_cons_chain() {
    assert_eq!(ty("(list (Int 1) (Int 2))"), opt_sexpr());
    // `(list)` is the empty list, so its type has to be the one that can
    // hold it.
    assert_eq!(ty("(list)"), opt_sexpr());
}

#[test]
fn list_elements_are_auto_wrapped_into_sexpr() {
    // Was a `TypeError` before the Sexpr-user-ADT design plan
    // (`~/.claude/plans/async-conjuring-hanrahan.md`, `tests/
    // sexpr_user_adt_test.rs` has the fuller coverage): `check_list_lit`
    // checks each element against `expected = Sexpr`, and a scalar with a
    // `Sexpr` encoding (`i32`/`f64`/.../`Str`) now auto-wraps through
    // its constructor there — `(list 1 2)` mirrors CL's `(list 1 2)`
    // instead of demanding the caller pre-wrap every element by hand.
    assert_eq!(ty("(list 1 2)"), opt_sexpr());
}

// `dolist` (iterating a `Sexpr` list) was removed — Symbol/Sexpr redesign
// Phase 5.

// ---- loop / break / return ---------------------------------------------------

#[test]
fn loop_with_only_break_is_unit() {
    assert_eq!(ty("(loop (break))"), Type::Unit);
}

#[test]
fn loop_with_return_value_takes_that_type() {
    assert_eq!(ty("(loop (return 5))"), Type::I32);
}

#[test]
fn loop_with_no_exit_is_never() {
    assert_eq!(ty("(loop 1)"), Type::Never);
}

#[test]
fn loop_break_and_return_must_agree() {
    assert_type_error("(if true (loop (break)) (loop (return 5)))");
}

#[test]
fn break_outside_loop_errors() {
    assert_type_error("(break)");
}

#[test]
fn return_outside_loop_errors() {
    assert_type_error("(return 1)");
}

#[test]
fn break_takes_no_arguments() {
    assert_type_error("(loop (break 1))");
}

#[test]
fn return_inside_while_must_be_unit() {
    assert_type_error_with_prelude("(while true (return 1))");
    assert_eq!(ty_with_prelude("(while true (return))"), Type::Unit);
}

#[test]
fn break_does_not_cross_lambda_boundary() {
    assert_type_error("(loop ((lambda () () (break))))");
}

#[test]
fn nested_loop_break_targets_innermost() {
    // The inner loop's `break` exits the inner loop only; it must not
    // contribute to the outer loop's exit type (which here comes solely from
    // the outer `return`). If it leaked, this would be a type error (Unit vs
    // i32).
    assert_eq!(ty("(loop (loop (break)) (return 5))"), Type::I32);
}

// ---- core form shape --------------------------------------------------------

#[test]
fn a_checked_form_is_a_core_expression_with_a_type() {
    let c = form("(if true 1 2)").unwrap();
    assert_eq!(c.printed, "(expr (if (bool true) (int 1) (int 2)))");
    assert_eq!(c.ty, Some(Type::I32));
}

// ---- the (type annotation) ---------------------------------------------------

#[test]
fn the_overrides_an_integer_literals_default_type() {
    assert_eq!(ty("(the u16 5)"), Type::U16);
    assert_eq!(ty("(the f32 1.5)"), Type::F32);
}

#[test]
fn the_produces_the_same_expr_as_its_inner_form() {
    // `the` contributes no node of its own — checking `(the i32 5)` yields
    // exactly the same `(int 5)` the bare literal would.
    let c = form("(the i32 5)").unwrap();
    assert_eq!(c.printed, "(expr (int 5))");
    assert_eq!(c.ty, Some(Type::I32));
}

#[test]
fn the_mismatch_is_a_type_error() {
    assert_type_error("(the bool 5)");
}

#[test]
fn the_rejects_wrong_arity() {
    assert_type_error("(the i32)");
    assert_type_error("(the i32 5 6)");
}

#[test]
fn the_pins_a_generic_calls_type_argument() {
    // `identity` is a generic `defun` (T -> T); annotating its argument's
    // type with `the` is enough to resolve `T`, the same as an outer
    // `expected` type would.
    assert_eq!(ty_with_prelude("(identity (the u16 5))"), Type::U16);
}

// ---- exit ---------------------------------------------------------------------

#[test]
fn exit_type_checks_as_never() {
    // `exit`'s actual process termination can only be observed
    // out-of-process — see `tests/exit_test.rs`. This only checks the type
    // level: `Never` satisfies any expected type, like `panic`.
    assert_eq!(ret_of("(defun f () i32 (if true 1 (exit 1)))", "f"), Type::I32);
}

#[test]
fn exit_rejects_wrong_arity() {
    assert_type_error("(exit)");
    assert_type_error("(exit 1 2)");
}

// ---- unreachable / todo (prelude macros) ---------------------------------------

#[test]
fn unreachable_and_todo_type_check_as_never() {
    assert_eq!(ret_of("(defun f () i32 (if true 1 (unreachable)))", "f"), Type::I32);
    assert_eq!(ret_of("(defun g () i32 (if true 1 (todo)))", "g"), Type::I32);
}

/// A type error carries the source location of the offending sub-form (down to
/// the innermost list form that failed), so messages read `file:line:col: ...`.
#[test]
fn type_error_carries_source_location() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    // The `(+ x "oops")` form is on line 2; the whole thing is checked with a
    // filename so the location names it.
    let vs = r
        .read_all_in(&mut h, "prog.typl", "(defun f ((x i32)) i32\n  (+ x \"oops\"))")
        .expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut err = None;
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            err = Some(e);
            break;
        }
    }
    let err = err.expect("expected a type error");
    let loc = err.loc().expect("type error should carry a location");
    assert_eq!(&*loc.file, "prog.typl");
    assert_eq!(loc.line, 2, "should point at the `(+ ...)` form on line 2");
    // Underlying kind is still a TypeError.
    assert!(matches!(err.kind(), Error::TypeError(_)));
}

// ---- equality is type-checked, with no free-function escape hatch ------------

/// `equal`/`equalp` compare two values *of the same type*, and nothing else.
///
/// The language already enforced this for every type that carries its own
/// `equal` method: `(equalp 1 "a")` is a type error because `i32`'s method
/// demands two `i32`s. But those two were free builtins typed
/// `(Sexpr, Sexpr) -> bool`, so for a type with *no* such method — `Option<T>`,
/// `Result<T,E>`, a user `defstruct` — both arguments widened into
/// S-expression data independently and any two values of any two types
/// compared, answering `false` rather than failing to check.
///
/// The hole opened exactly where a mistake is hardest to see, and it made a
/// second one possible: with `Option<Sexpr>` parameters, `(equalp opt
/// (option::some x))` had its constructor call take the *parameter's* type
/// argument and become the empty-list niche — a bare datum — while `opt`
/// crossed as a boxed enum, so a comparison between two genuinely equal
/// values answered `false`.
///
/// One type parameter used twice closes both.
#[test]
fn equality_requires_its_two_arguments_to_have_one_type() {
    for src in [
        // Two unrelated user structs — the case that used to answer `false`.
        "(defstruct pa (x i32)) (defstruct pb (y string)) (equalp (pa::new 1) (pb::new \"z\"))",
        // Two different instantiations of the same generic.
        "(equalp (the Option<char> (option::none)) (the Option<i32> (option::none)))",
        // And the case that was already an error, still is.
        "(equalp 1 \"a\")",
    ] {
        let err = program(src).expect_err("mismatched equality should not check");
        let msg = format!("{:?}", err);
        assert!(msg.contains("type mismatch"), "unexpected error for {}:\n{}", src, msg);
    }
}

/// The same-type cases all still check and still answer structurally — the
/// point being that closing the hole cost none of the reach `equal`/`equalp`
/// actually need.
#[test]
fn equality_still_compares_every_same_typed_pair() {
    for src in [
        "(equal (quote (1 2 3)) (quote (1 2 3)))",
        "(equalp (option::some #\\x) (option::some #\\x))",
        "(defstruct pt (x i32)) (equalp (pt::new 1) (pt::new 1))",
    ] {
        assert!(program(src).is_ok(), "should check: {}", src);
    }
}

/// `'foo` is a `Symbol`, not S-expression data.
///
/// Every other atom has an unquoted spelling that already carries its precise
/// type — `42`, `"s"`, `#\a`, `true`. The symbol is the one value whose *only*
/// literal syntax is the quote, so typing it as data threw away what the
/// program said, and did it invisibly: the symbol still worked everywhere
/// S-expression data is wanted (it widens by a bare retype), so the loss only
/// surfaced somewhere it was compared.
#[test]
fn a_quoted_symbol_is_a_symbol() {
    assert_eq!(ty("(quote foo)"), Type::Symbol);
    // A compound datum stays S-expression data — there is no other type for it.
    assert_eq!(ty("(quote (a b))"), opt_sexpr());
    assert_eq!(ty("(quote ())"), opt_sexpr());
    // An integer literal is deliberately left alone: it has no one type to be
    // given, it adopts the width its context asks for.
    assert_eq!(ty("(quote 42)"), opt_sexpr());
    // It still reaches a `Symbol` parameter, and still reaches an
    // S-expression one.
    assert!(program(r#"(defun f ((s Symbol)) string (symbol->string s)) (f (quote foo))"#).is_ok());
    assert!(program(r#"(defun g ((s Option<Sexpr>)) bool (sexpr-symp s)) (g (quote foo))"#).is_ok());
}

/// Equality between a built symbol and a quoted one checks *in either order*.
///
/// It did not before: `T` binds from the first argument, so the order decided
/// whether the other side had to widen (fine) or narrow (impossible). That
/// asymmetry was a symptom of `'foo` having lost its type, not a property of
/// generic inference — with both sides `Symbol` there is nothing to widen.
#[test]
fn equality_between_a_built_and_a_quoted_symbol_is_order_independent() {
    assert!(program(r#"(equal (string->symbol "foo") (quote foo))"#).is_ok());
    assert!(program(r#"(equal (quote foo) (string->symbol "foo"))"#).is_ok());
}
