//! Tests for generic-`defun` monomorphization: a call site that instantiates
//! a generic function at concrete types makes the checker generate a
//! specialized (type-substituted, re-checked) `defun` and rewrite the call to
//! reference it — the interpreter never executes a type-erased generic body
//! (`Interp::exec` skips registering one entirely).
//!
//! The specializations ride along in a synthetic `TopLevel::Module` bundle
//! (path `MONO_BUNDLE_MODULE`, specializations first, primary form last) so
//! every existing check→exec pipeline picks them up unchanged — the shape
//! tests below pin that contract down.

extern crate typelisp;
use typelisp::{Checker, Error, Heap, Interp, Reader, RtValue, TopLevel, MONO_BUNDLE_MODULE, Path};

fn run(src: &str) -> Result<RtValue, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v)?;
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

/// Check every form (without executing) and return each form's `TopLevel`.
fn check_all(src: &str) -> Result<Vec<TopLevel>, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    vs.into_iter().map(|v| chk.check_form(&mut h, &interp, v)).collect()
}

/// The specialization `Defun`s bundled with `tl` (empty if `tl` isn't a
/// monomorph bundle).
fn bundled_specs(tl: &TopLevel) -> Vec<&TopLevel> {
    match tl {
        TopLevel::Module { path, body } if *path == Path::root(MONO_BUNDLE_MODULE) => {
            body[..body.len() - 1].iter().collect()
        }
        _ => Vec::new(),
    }
}

// ---- bundle shape -----------------------------------------------------------

#[test]
fn an_instantiating_call_comes_back_bundled_with_its_specialization() {
    let tls = check_all("(defun (identity T) ((x T)) T x) (identity 42)").unwrap();
    // The generic definition itself needs no specializations.
    assert!(matches!(&tls[0], TopLevel::Defun { type_params, .. } if !type_params.is_empty()));
    // The call comes back as [specialization, call] — one concrete Defun.
    let specs = bundled_specs(&tls[1]);
    assert_eq!(specs.len(), 1);
    match specs[0] {
        TopLevel::Defun { name, type_params, .. } => {
            assert!(type_params.is_empty(), "a specialization is fully concrete");
            assert!(
                name.local().contains(' '),
                "mangled names contain a space so no reader token can collide: {:?}",
                name
            );
        }
        other => panic!("expected a specialized Defun, got {:?}", other),
    }
}

#[test]
fn two_calls_at_the_same_type_in_one_form_share_one_specialization() {
    let tls = check_all(
        "(defun (identity T) ((x T)) T x) \
         (defun use2 () i32 (+ (identity 1) (identity 2)))",
    )
    .unwrap();
    assert_eq!(bundled_specs(&tls[1]).len(), 1);
}

#[test]
fn calls_at_different_types_get_independent_specializations() {
    let tls = check_all(
        "(defun (identity T) ((x T)) T x) \
         (defun use2 () i32 (if (identity true) (identity 1) 0))",
    )
    .unwrap();
    assert_eq!(bundled_specs(&tls[1]).len(), 2);
}

#[test]
fn a_non_generic_call_is_not_bundled() {
    let tls = check_all("(defun id ((x i32)) i32 x) (id 7)").unwrap();
    assert!(matches!(&tls[1], TopLevel::Expr(_)));
}

// ---- evaluation through specializations --------------------------------------

#[test]
fn a_generic_call_evaluates_through_its_specialization() {
    assert_eq!(eval_ok("(defun (identity T) ((x T)) T x) (identity 42)"), RtValue::Int(42));
    assert_eq!(eval_ok("(defun (identity T) ((x T)) T x) (identity true)"), RtValue::Bool(true));
}

#[test]
fn self_recursion_reuses_the_same_specialization() {
    let src = r#"
        (defun (last-of T) ((n i32) (x T)) T
          (if (= n 0) x (last-of (- n 1) x)))
        (last-of 3 99)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(99));
}

#[test]
fn mutually_recursive_generics_converge_on_the_worklist() {
    // `ping` and `pong` call each other at the same T: specializing
    // `ping <i32>` requests `pong <i32>` whose body's `ping` call hits the
    // memo instead of diverging. (`pong` is defined first — this language
    // has no forward references — so `ping`'s body can name it; `pong`'s
    // own body can name `ping` because a generic body is only *diagnostic*-
    // checked at definition time... except forward references still fail
    // there, hence `pong` recursing through itself and `ping` through both.)
    let src = r#"
        (defun (pong T) ((n i32) (x T)) T
          (if (= n 0) x (pong (- n 1) x)))
        (defun (ping T) ((n i32) (x T)) T
          (if (= n 0) x (pong n x)))
        (ping 2 7)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

#[test]
fn generic_body_calling_another_generic_specializes_transitively() {
    let src = r#"
        (defun (inner T) ((x T)) T x)
        (defun (outer T) ((x T)) T (inner x))
        (outer 5)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(5));
}

#[test]
fn a_generic_instantiated_from_a_defvar_initializer_works() {
    let src = r#"
        (defun (identity T) ((x T)) T x)
        (defvar g (identity 11))
        g
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(11));
}

#[test]
fn a_generic_instantiated_inside_a_module_works() {
    let src = r#"
        (module m
          (pub defun (identity T) ((x T)) T x)
          (pub defun use-it () i32 (identity 3)))
        (m::use-it)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

#[test]
fn a_generic_instantiated_from_a_macro_body_works() {
    // The defmacro's body calls `identity` at T=Sexpr; its bundle is exec'd
    // like any other form here, so the later use of the macro (expanding at
    // check time via MacroExpander) finds the specialization registered.
    let src = r#"
        (defun (identity T) ((x T)) T x)
        (defmacro pass (x) (identity x))
        (pass 42)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

#[test]
fn a_generic_rest_function_specializes() {
    let src = r#"
        (defun (firstn T) ((a T) &rest (xs T)) T a)
        (firstn 1 2 3)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn a_type_parameter_shadows_a_user_type_of_the_same_name() {
    // `canon` consults `type_var_bindings` *before* `resolve_type_name`, so
    // a user type spelled like the type parameter cannot capture the
    // template's annotations during specialization.
    let src = r#"
        (defstruct t (v i32))
        (defun (identity T) ((x T)) T x)
        (identity 42)
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

// ---- generic-owner methods (defmethod / defstruct accessors) ------------------

#[test]
fn a_method_on_a_generic_type_specializes_at_the_call_site() {
    let tls = check_all(
        "(defstruct (box T) (v T)) \
         (defmethod get-v ((self box<T>)) T self::v) \
         (get-v (box::new 42))",
    )
    .unwrap();
    // The call bundles the specialized `get-v` — and, transitively, the
    // specialized `v` field getter its body's `self::v` needs.
    let specs = bundled_specs(&tls[2]);
    let methods: Vec<&str> = specs
        .iter()
        .filter_map(|tl| match tl {
            TopLevel::Defmethod { method, type_params, .. } => {
                assert!(type_params.is_empty(), "specializations are concrete");
                Some(method.as_str())
            }
            _ => None,
        })
        .collect();
    assert!(methods.iter().any(|m| m.starts_with("get-v <")), "specialized get-v in {:?}", methods);
    assert!(methods.iter().any(|m| m.starts_with("v <")), "transitively specialized getter in {:?}", methods);
}

#[test]
fn a_generic_method_call_evaluates_through_its_specialization() {
    let src = r#"
        (defstruct (box T) (v T))
        (defmethod get-v ((self box<T>)) T self::v)
        (get-v (box::new 42))
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

#[test]
fn a_generic_method_with_a_match_body_specializes() {
    let src = r#"
        (defmethod unwrap2 ((self Option<T>)) T
          (match self ((some x) x) ((none) (panic "none"))))
        (unwrap2 (option::some 9))
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(9));
}

#[test]
fn generic_defstruct_field_read_and_setf_work_through_specialized_accessors() {
    let src = r#"
        (defstruct (box T) (v T))
        (defvar b (box::new 1))
        (setf b::v 5)
        b::v
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(5));
}

#[test]
fn one_generic_type_instantiated_at_two_types_gets_independent_methods() {
    let src = r#"
        (defstruct (box T) (v T))
        (defmethod get-v ((self box<T>)) T self::v)
        (let ((a (box::new 1)) (b (box::new true)))
          (if (get-v b) (get-v a) 0))
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

// ---- erased bodies never run --------------------------------------------------

#[test]
fn the_erased_generic_body_is_not_registered_in_the_interpreter() {
    // `(compile identity)` needs `Interp::fns["identity"]`; a generic defun
    // deliberately never registers its erased body, so this must fail (at
    // check or at exec) rather than compile a type-erased twin. (The
    // checker-side FnRef story for generics is a later stage; this pins the
    // exec-side guarantee.)
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, "(defun (identity T) ((x T)) T x) (compile identity)").expect("read failed");
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mut failed = false;
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                if interp.exec(&mut h, tl).is_err() {
                    failed = true;
                }
            }
            Err(_) => failed = true,
        }
    }
    assert!(failed, "compiling an erased generic body must not succeed");
}

// ---- polymorphic recursion is rejected ----------------------------------------

#[test]
fn polymorphic_recursion_is_a_type_error_not_a_hang() {
    // `f` calls itself at `Option<T>` — every instantiation requests a new
    // one, so the drain budget must cut it off with a TypeError.
    let src = r#"
        (defun (f T) ((n i32) (x T)) i32
          (if (= n 0) n (f (- n 1) (option::some x))))
        (f 3 1)
    "#;
    match check_all(src) {
        Err(Error::TypeError(msg)) => {
            assert!(msg.contains("monomorphization"), "unexpected message: {}", msg)
        }
        other => panic!("expected a TypeError, got {:?}", other),
    }
}
