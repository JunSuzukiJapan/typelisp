//! Tests for generic-`defun` monomorphization: a call site that instantiates
//! a generic function at concrete types makes the checker generate a
//! specialized (type-substituted, re-checked) `defun` and rewrite the call to
//! reference it — the interpreter never executes a type-erased generic body
//! (`Interp::exec` skips registering one entirely).
//!
//! The specializations ride along in a synthetic `(module ...)` bundle
//! (path `MONO_BUNDLE_MODULE`, specializations first, primary form last) so
//! every existing check→exec pipeline picks them up unchanged — the shape
//! tests below pin that contract down.

extern crate typelisp;
use typelisp::check::core;
use typelisp::{
    load_compiler, load_prelude, Checker, Error, Heap, Interp, Path, Reader, TopLevelForm, Value, MONO_BUNDLE_MODULE,
};

fn run(src: &str) -> Result<Value, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    // Load the prelude then the native compiler island *before* reading the
    // program: a generic function used as a value reifies to an `FnRef`
    // closure, which JIT-compiles through the island. Loading first also keeps
    // the read program's GC roots off the stack while the island loads.
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?;
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// Check every form (without executing) and return each form's core form, plus
/// the `Heap` they live in — a checked form is cons cells, so the tree means
/// nothing without its heap.
fn check_all(src: &str) -> Result<(Heap, Vec<TopLevelForm>), Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut tls = Vec::new();
    for v in vs {
        tls.push(chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?);
    }
    Ok((h, tls))
}

/// The specialization forms bundled with `tl` — everything but the primary form
/// the bundle ends with. Empty if `tl` isn't a monomorph bundle.
fn bundled_specs(h: &Heap, tl: TopLevelForm) -> Vec<TopLevelForm> {
    if core::op(h, tl) != Some("module") {
        return Vec::new();
    }
    // `(module PATH BODY...)` — the path decides whether this is *the* bundle.
    let fields = core::fields(h, tl).expect("a module's fields are a proper list");
    let is_bundle = match fields[0] {
        Value::Path(id) => typelisp::path_from_id(h, id) == Path::root(MONO_BUNDLE_MODULE),
        Value::Symbol(id) => h.symbol_name(id) == MONO_BUNDLE_MODULE,
        _ => false,
    };
    if !is_bundle {
        return Vec::new();
    }
    let body = &fields[1..];
    body[..body.len() - 1].to_vec()
}

/// A `(defun PATH ...)` form's name.
fn defun_name(h: &Heap, tl: TopLevelForm) -> Path {
    assert_eq!(core::op(h, tl), Some("defun"), "expected a defun form");
    match core::field(h, tl, 0).expect("a defun always names itself") {
        Value::Path(id) => typelisp::path_from_id(h, id),
        Value::Symbol(id) => Path::root(h.symbol_name(id)),
        other => panic!("a defun's name is a path or symbol, got {:?}", other),
    }
}

/// A `(defmethod PATH SYM ...)` form's method name.
fn defmethod_name(h: &Heap, tl: TopLevelForm) -> Option<String> {
    if core::op(h, tl) != Some("defmethod") {
        return None;
    }
    match core::field(h, tl, 1).expect("a defmethod always names its method") {
        Value::Symbol(id) => Some(h.symbol_name(id).to_string()),
        other => panic!("a defmethod's method name is a symbol, got {:?}", other),
    }
}

// ---- bundle shape -----------------------------------------------------------

#[test]
fn an_instantiating_call_comes_back_bundled_with_its_specialization() {
    let (h, tls) = check_all("(defun identity<T> ((x T)) T x) (identity 42)").unwrap();
    // The generic definition itself has no core form at all: an erased body is
    // never executed, so it lowers to an empty `(module PATH)` — see
    // `Checker::defun_form_unless_generic`.
    assert_eq!(core::op(&h, tls[0]), Some("module"));
    assert_eq!(core::fields(&h, tls[0]).unwrap().len(), 1, "path only, no body");
    // The call comes back as [specialization, call] — one concrete defun.
    let specs = bundled_specs(&h, tls[1]);
    assert_eq!(specs.len(), 1);
    let name = defun_name(&h, specs[0]);
    assert!(
        name.last_segment().contains(' '),
        "mangled names contain a space so no reader token can collide: {:?}",
        name
    );
}

#[test]
fn two_calls_at_the_same_type_in_one_form_share_one_specialization() {
    let (h, tls) = check_all(
        "(defun identity<T> ((x T)) T x) \
         (defun use2 () i32 (+ (identity 1) (identity 2)))",
    )
    .unwrap();
    assert_eq!(bundled_specs(&h, tls[1]).len(), 1);
}

#[test]
fn calls_at_different_types_get_independent_specializations() {
    let (h, tls) = check_all(
        "(defun identity<T> ((x T)) T x) \
         (defun use2 () i32 (if (identity true) (identity 1) 0))",
    )
    .unwrap();
    assert_eq!(bundled_specs(&h, tls[1]).len(), 2);
}

#[test]
fn a_non_generic_call_is_not_bundled() {
    let (h, tls) = check_all("(defun id ((x i32)) i32 x) (id 7)").unwrap();
    assert_eq!(core::op(&h, tls[1]), Some("expr"));
}

// ---- evaluation through specializations --------------------------------------

#[test]
fn a_generic_call_evaluates_through_its_specialization() {
    assert_eq!(eval_ok("(defun identity<T> ((x T)) T x) (identity 42)"), Value::Int(42));
    assert_eq!(eval_ok("(defun identity<T> ((x T)) T x) (identity true)"), Value::Bool(true));
}

#[test]
fn self_recursion_reuses_the_same_specialization() {
    let src = r#"
        (defun last-of<T> ((n i32) (x T)) T
          (if (= n 0) x (last-of (- n 1) x)))
        (last-of 3 99)
    "#;
    assert_eq!(eval_ok(src), Value::Int(99));
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
        (defun pong<T> ((n i32) (x T)) T
          (if (= n 0) x (pong (- n 1) x)))
        (defun ping<T> ((n i32) (x T)) T
          (if (= n 0) x (pong n x)))
        (ping 2 7)
    "#;
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn generic_body_calling_another_generic_specializes_transitively() {
    let src = r#"
        (defun inner<T> ((x T)) T x)
        (defun outer<T> ((x T)) T (inner x))
        (outer 5)
    "#;
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn a_generic_instantiated_from_a_defvar_initializer_works() {
    let src = r#"
        (defun identity<T> ((x T)) T x)
        (defvar (g i32) (identity 11))
        g
    "#;
    assert_eq!(eval_ok(src), Value::Int(11));
}

#[test]
fn a_generic_instantiated_inside_a_module_works() {
    let src = r#"
        (module m
          (pub defun identity<T> ((x T)) T x)
          (pub defun use-it () i32 (identity 3)))
        (m::use-it)
    "#;
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn a_generic_instantiated_from_a_macro_body_works() {
    // The defmacro's body calls `identity` at T=Sexpr; its bundle is exec'd
    // like any other form here, so the later use of the macro (expanding at
    // check time via MacroExpander) finds the specialization registered.
    let src = r#"
        (defun identity<T> ((x T)) T x)
        (defmacro pass (x) (identity x))
        (pass 42)
    "#;
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn a_generic_rest_function_specializes() {
    let src = r#"
        (defun firstn<T> ((a T) &rest (xs T)) T a)
        (firstn 1 2 3)
    "#;
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn a_type_parameter_shadows_a_user_type_of_the_same_name() {
    // `canon` consults `type_var_bindings` *before* `resolve_type_name`, so
    // a user type spelled like the type parameter cannot capture the
    // template's annotations during specialization.
    let src = r#"
        (defstruct t (v i32))
        (defun identity<T> ((x T)) T x)
        (identity 42)
    "#;
    assert_eq!(eval_ok(src), Value::Int(42));
}

// ---- generic-owner methods (defmethod / defstruct accessors) ------------------

#[test]
fn a_method_on_a_generic_type_specializes_at_the_call_site() {
    let (h, tls) = check_all(
        "(defstruct box<T> (v T)) \
         (defmethod get-v ((self box<T>)) T self::v) \
         (get-v (box::new 42))",
    )
    .unwrap();
    // The call bundles the specialized `get-v` — and, transitively, the
    // specialized `v` field getter its body's `self::v` needs.
    let specs = bundled_specs(&h, tls[2]);
    let methods: Vec<String> = specs.iter().filter_map(|tl| defmethod_name(&h, *tl)).collect();
    assert!(methods.iter().any(|m| m.starts_with("get-v <")), "specialized get-v in {:?}", methods);
    assert!(methods.iter().any(|m| m.starts_with("v <")), "transitively specialized getter in {:?}", methods);
}

#[test]
fn a_generic_method_call_evaluates_through_its_specialization() {
    let src = r#"
        (defstruct box<T> (v T))
        (defmethod get-v ((self box<T>)) T self::v)
        (get-v (box::new 42))
    "#;
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn a_generic_method_with_a_match_body_specializes() {
    let src = r#"
        (defmethod unwrap2 ((self Option<T>)) T
          (match self ((some x) x) ((none) (panic "none"))))
        (unwrap2 (option::some 9))
    "#;
    assert_eq!(eval_ok(src), Value::Int(9));
}

#[test]
fn generic_defstruct_field_read_and_setf_work_through_specialized_accessors() {
    let src = r#"
        (defstruct box<T> (v T))
        (defvar (b box<i32>) (box::new 1))
        (setf b::v 5)
        b::v
    "#;
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn one_generic_type_instantiated_at_two_types_gets_independent_methods() {
    let src = r#"
        (defstruct box<T> (v T))
        (defmethod get-v ((self box<T>)) T self::v)
        (let ((a (box::new 1)) (b (box::new true)))
          (if (get-v b) (get-v a) 0))
    "#;
    assert_eq!(eval_ok(src), Value::Int(1));
}

// ---- type-directed decoding (the erasure misdecode, fixed) --------------------

#[test]
fn a_sexpr_instantiated_generic_field_returns_the_datum_not_a_misdecoded_scalar() {
    // The exact misdecode the old shape heuristic documented as a known
    // erasure limitation (pre-monomorphization `interp.rs`'s
    // `decode_struct_field` doc comment): a `box<Sexpr>` field holding the
    // quoted datum `42` is stored as a bare `Value::Int`, and a shape-driven
    // decode handed it back as `RtValue::Int` — contradicting the field's
    // static type. With the accessor monomorphized (`v <sexpr>`, `FieldGet`
    // node type `Sexpr`) the decode is type-directed and exact.
    let src = r#"
        (defstruct box<T> (v T))
        (defvar (b box<Sexpr>) (box::new '42))
        b::v
    "#;
    match run(src).expect("eval failed") {
        Value::Int(42) => {}
        other => panic!("expected the Sexpr datum 42, got {:?}", other),
    }
}

#[test]
fn a_vector_of_sexpr_element_returns_the_datum() {
    // `Vector<Sexpr>`'s `get` return type is `Sexpr` at every (specialized)
    // call site, so the element decode is type-directed the same way.
    let src = r#"
        (defvar (v Vector<Sexpr>) (Vector::new))
        (push v '7)
        (get v 0)
    "#;
    match run(src).expect("eval failed") {
        Value::Int(7) => {}
        other => panic!("expected the Sexpr datum 7, got {:?}", other),
    }
}

// ---- generic functions as values (FnRef/MethodRef) -----------------------------

#[test]
fn a_generic_function_passed_as_an_argument_specializes_from_the_parameter_type() {
    let src = r#"
        (defun identity<T> ((x T)) T x)
        (defun call-it ((f (fn (i32) i32)) (n i32)) i32 (f n))
        (call-it identity 41)
    "#;
    assert_eq!(eval_ok(src), Value::Int(41));
}

#[test]
fn a_generic_function_annotated_with_the_specializes() {
    let src = r#"
        (defun identity<T> ((x T)) T x)
        (defun apply1 ((f (fn (bool) bool))) bool (f true))
        (apply1 (the (fn (bool) bool) identity))
    "#;
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn a_generic_method_passed_as_an_argument_specializes() {
    let src = r#"
        (defstruct box<T> (v T))
        (defmethod get-v ((self box<T>)) T self::v)
        (defun call-it ((f (fn (box<i32>) i32)) (b box<i32>)) i32 (f b))
        (call-it get-v (box::new 7))
    "#;
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn a_generic_function_value_without_type_context_is_a_check_error() {
    // An unannotated `let` binding gives the checker no function type to
    // resolve T from — there is no erased generic value to fall back to
    // anymore. (`defvar` can't even express the untyped case: its type
    // annotation is mandatory.)
    match check_all("(defun identity<T> ((x T)) T x) (let ((f identity)) 0)") {
        Err(Error::TypeError(msg)) => assert!(msg.contains("generic function"), "unexpected: {}", msg),
        Err(other) => panic!("expected TypeError, got {:?}", other),
        Ok(_) => panic!("expected TypeError, got a successful check"),
    }
}

#[test]
fn a_generic_function_value_in_a_typed_defvar_specializes() {
    // `defvar`'s mandatory type annotation is exactly the context a generic
    // function value needs: `f`'s declared `(fn (i32) i32)` resolves T=i32
    // and the global ends up holding the specialization.
    let src = r#"
        (defun identity<T> ((x T)) T x)
        (defvar (f (fn (i32) i32)) identity)
        (f 41)
    "#;
    assert_eq!(eval_ok(src), Value::Int(41));
}

#[test]
fn compiling_a_generic_function_is_a_check_error() {
    match check_all("(defun identity<T> ((x T)) T x) (compile identity)") {
        Err(Error::TypeError(msg)) => assert!(msg.contains("generic"), "unexpected: {}", msg),
        Err(other) => panic!("expected TypeError, got {:?}", other),
        Ok(_) => panic!("expected TypeError, got a successful check"),
    }
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
    let vs = r.read_all(&mut h, "(defun identity<T> ((x T)) T x) (compile identity)").expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
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
        (defun f<T> ((n i32) (x T)) i32
          (if (= n 0) n (f (- n 1) (option::some x))))
        (f 3 1)
    "#;
    match check_all(src) {
        Err(Error::TypeError(msg)) => {
            assert!(msg.contains("monomorphization"), "unexpected message: {}", msg)
        }
        Err(other) => panic!("expected a TypeError, got {:?}", other),
        Ok(_) => panic!("expected a TypeError, got a successful check"),
    }
}
