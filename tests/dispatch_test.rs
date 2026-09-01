//! Tests for `Checker::check_list`'s name-resolution order between a free
//! function and a same-named instance method on a builtin generic type
//! (`HashTable<K,V>`).
//!
//! Before this, a free function with the same name as an instance method
//! (e.g. the generic `Iter` combinator `count` vs. `HashTable<K,V>`'s `count`
//! method) always won, regardless of the call's actual argument type —
//! calling `(remove h k)` for a `HashTable<K,V>` `h` failed with a type
//! mismatch instead of reaching `HashTable`'s method.
//! `Checker::try_instance_method` now tries a receiver-typed instance method
//! on the first argument's type *before* the free function, falling back to
//! the free function only when no type-specific method matches — mirroring
//! CLOS, where an existing ordinary function of the same name becomes a
//! generic function's default method, used only when no method's
//! specializer matches the call.

extern crate typelisp;
use typelisp::{load_prelude, load_compiler, Checker, Error, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).map_err(|e| format!("{:?}", e))?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| format!("{:?}", e))?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| format!("{:?}", e))? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

// `length`/`sort` (free `Sexpr` list functions) were removed — Symbol/Sexpr
// redesign Phase 5 — so the dispatch tests that asserted a `Sexpr` receiver
// resolves to them are gone. HashTable-receiver dispatch (below) is unaffected.

#[test]
fn remove_resolves_to_hashtable_instance_method_for_a_hashtable_receiver() {
    let src = r#"
        (defun make-h () HashTable<i32,i32> (HashTable::new))
        (let ((h (make-h)))
          (set h 1 9)
          (unwrap (remove h 1)))
    "#;
    assert_eq!(eval_ok(src), Value::Int(9));
}

#[test]
fn count_resolves_to_hashtable_instance_method_for_a_hashtable_receiver() {
    let src = r#"
        (defun make-h () HashTable<i32,i32> (HashTable::new))
        (let ((h (make-h)))
          (set h 1 9)
          (count h))
    "#;
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn count_if_resolves_to_the_free_generic_combinator_over_an_iterator() {
    // `count` is `HashTable<K,V>`'s instance method (above); `count-if` is the
    // free generic `Iter` combinator `(count-if it pred)`. A two-argument call
    // whose first argument is an iterator (not a `HashTable`) has no matching
    // instance method, so it resolves to the free generic function.
    let src = "(defun mkv () Vector<i32> \
                 (let ((v (the Vector<i32> (Vector::new)))) (push v 1) (push v 2) (push v 1) v)) \
               (count-if (iter (mkv)) (lambda ((n i32)) bool (= n 1)))";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn integer_literal_still_gets_the_free_functions_exact_parameter_width() {
    // `try_instance_method` peeks at the first argument with `expected: None`
    // before falling back to the free function. If that peek's default type
    // (`i32`) leaked into the free-function call instead of being re-checked
    // against the real parameter type, this would fail to type-check at all
    // (a function expecting `u16` would see an `i32` literal).
    let src = "(defun takes-i32 ((x i32)) i32 x) (takes-i32 42)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn an_undefined_name_with_no_method_or_free_function_is_still_a_clean_error() {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let v = r.read(&mut h, "(totally-undefined-name 1 2)").expect("read failed");
    match chk.check_form(&mut h, &interp, v).map_err(Error::into_kind) {
        Err(Error::NoSuchFunction(name)) => assert_eq!(name, "totally-undefined-name"),
        other => panic!("expected NoSuchFunction, got {:?}", other),
    }
}

#[test]
fn unbound_variable_in_first_argument_position_still_surfaces_the_real_error() {
    // `try_instance_method` swallows a checking error on the first argument
    // and returns `None` (deferring to the free-function path), rather than
    // propagating it directly — this confirms the real error still surfaces
    // from there instead of being masked as `NoSuchFunction`.
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let v = r.read(&mut h, "(length some-unbound-variable)").expect("read failed");
    match chk.check_form(&mut h, &interp, v).map_err(Error::into_kind) {
        Err(Error::TypeError(msg)) => assert!(msg.contains("unbound"), "unexpected message: {}", msg),
        other => panic!("expected an unbound-variable TypeError, got {:?}", other),
    }
}

