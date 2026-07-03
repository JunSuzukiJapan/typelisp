//! Tests for the redefinition-policy safety net (`Checker::check_redef`):
//! redefining a built-in (`Registry::with_builtins`-registered) name is
//! always an error, regardless of `RedefPolicy`; redefining anything else
//! follows the configured policy (`Warn` by default).

extern crate typelisp;
use typelisp::{Checker, Error, Heap, Interp, Reader, RedefPolicy};

/// Checks every form in `src` against one `Checker` configured with `policy`,
/// returning every warning recorded along the way and the first error hit
/// (if any) — mirrors `namespace_test.rs`'s `program` harness, but keeps the
/// `Checker` so `take_warnings` can be drained after each form.
fn run(src: &str, policy: RedefPolicy) -> (Vec<String>, Result<(), Error>) {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    chk.set_redef_policy(policy);
    let interp = Interp::new();
    let mut warnings = Vec::new();
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(_) => warnings.extend(chk.take_warnings()),
            Err(e) => return (warnings, Err(e)),
        }
    }
    (warnings, Ok(()))
}

// ---- built-ins: always an error, regardless of policy ----------------------

#[test]
fn redefining_a_builtin_function_is_always_an_error() {
    // `car`, not `not`: `not` moved from a Rust builtin to a plain prelude
    // `defun` in the `loop`/`break`/`return`/`setf` stage (it had no
    // GC-heap/Rust-only dependency once `loop`/`if` existed to write it
    // with — see `src/prelude.rs`'s comment on it) — this `run` helper
    // loads no prelude at all, so it's no longer registered as anything
    // here, builtin or otherwise, and redefining it wouldn't conflict with
    // what this test means to check. `car` is a genuine `Registry::with_builtins`
    // free function (direct cons-cell access — can't be written in
    // typelisp itself), so it still exercises this rule.
    let src = "(defun car ((x Sexpr)) Sexpr x)";
    for policy in [RedefPolicy::Warn, RedefPolicy::Error, RedefPolicy::Silent] {
        let (_, result) = run(src, policy);
        assert!(result.is_err(), "policy {:?} should still reject redefining a builtin function", policy);
    }
}

#[test]
fn redefining_a_builtin_instance_method_is_always_an_error() {
    let src = "(defmethod + ((a i32) (b i32)) i32 a)";
    for policy in [RedefPolicy::Warn, RedefPolicy::Error, RedefPolicy::Silent] {
        let (_, result) = run(src, policy);
        assert!(result.is_err(), "policy {:?} should still reject redefining a builtin method", policy);
    }
}

// ---- user definitions: follow the configured policy -------------------------

#[test]
fn redefining_a_user_function_warns_by_default() {
    let src = "(defun add1 ((x i32)) i32 (+ x 1)) (defun add1 ((x i32)) i32 (+ x 2))";
    let (warnings, result) = run(src, RedefPolicy::Warn);
    assert!(result.is_ok());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("add1"), "{:?}", warnings);
}

#[test]
fn redefining_a_user_function_with_error_policy_fails() {
    let src = "(defun add1 ((x i32)) i32 (+ x 1)) (defun add1 ((x i32)) i32 (+ x 2))";
    let (_, result) = run(src, RedefPolicy::Error);
    assert!(result.is_err());
}

#[test]
fn redefining_a_user_function_with_silent_policy_has_no_warning() {
    let src = "(defun add1 ((x i32)) i32 (+ x 1)) (defun add1 ((x i32)) i32 (+ x 2))";
    let (warnings, result) = run(src, RedefPolicy::Silent);
    assert!(result.is_ok());
    assert!(warnings.is_empty());
}

#[test]
fn redefining_a_user_macro_warns_by_default() {
    let src = "(defmacro twice (x) `(+ ,x ,x)) (defmacro twice (x) `(- ,x ,x))";
    let (warnings, result) = run(src, RedefPolicy::Warn);
    assert!(result.is_ok());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("twice"), "{:?}", warnings);
}

#[test]
fn redefining_a_user_var_warns_by_default() {
    let src = "(defvar (x i32) 1) (defvar (x i32) 2)";
    let (warnings, result) = run(src, RedefPolicy::Warn);
    assert!(result.is_ok());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("x"), "{:?}", warnings);
}

#[test]
fn redefining_a_user_method_warns_by_default() {
    let src = "(defmethod double ((self i32)) i32 (* self 2)) \
               (defmethod double ((self i32)) i32 (+ self self))";
    let (warnings, result) = run(src, RedefPolicy::Warn);
    assert!(result.is_ok());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("double"), "{:?}", warnings);
}

// ---- cross-module: shadowing a name in a child module is not redefinition --

#[test]
fn same_name_in_a_child_module_is_not_a_redefinition() {
    let src = "(defun helper ((x i32)) i32 x) \
               (module m (defun helper ((x i32)) i32 (+ x 1)))";
    let (warnings, result) = run(src, RedefPolicy::Warn);
    assert!(result.is_ok());
    assert!(warnings.is_empty(), "{:?}", warnings);
}
