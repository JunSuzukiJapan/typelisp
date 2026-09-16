//! `(defffi ...)` — declaring a C function and calling it.
//!
//! The targets are all libc, and chosen so the test suite depends on nothing
//! it has to install: `abs`, `toupper`, `getpid`, `sqrt`, `fabsf`, `srand`.
//! Between them they cover every shape a Stage-1 thunk has to build — a signed
//! narrowing argument, no arguments at all, `double` in and out, `float` in and
//! out (which crosses this boundary widened to `double`, so it is the one most
//! likely to be quietly wrong), and a `void` return.
//!
//! Every test loads the compiler, because the declaration cannot be executed
//! without it: what makes an FFI name callable is a JIT-emitted thunk, and the
//! backend that emits it is installed by loading the island.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Heap, Interp, Reader, Value};

fn eval(src: &str) -> Result<(Heap, Value), String> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| e.to_string())?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| e.to_string())?;
        match interp.exec(&mut h, tl) {
            Ok(Some(val)) => last = val,
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok((h, last))
}

fn int(src: &str) -> i64 {
    match eval(src).expect("evaluation failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

fn float(src: &str) -> f64 {
    let (h, v) = eval(src).expect("evaluation failed");
    match v {
        Value::Boxed(id) if h.is_f64(id) => h.f64_value(id),
        Value::Boxed(id) if h.is_f32(id) => f64::from(h.f32_value(id)),
        other => panic!("expected a float, got {:?}", other),
    }
}

/// The complaint about a program that must not run — from the checker or from
/// the declaration's own execution, which are one moment apart and both
/// "typelisp refused this".
fn err(src: &str) -> String {
    match eval(src) {
        Err(e) => e,
        Ok(_) => panic!("expected a failure, but the program ran"),
    }
}

// ------------------------------------------------------------- calling out

#[test]
fn calls_a_libc_function() {
    assert_eq!(int(r#"(defffi (c-abs "abs") (i32) i32) (unsafe (c-abs -5))"#), 5);
}

#[test]
fn calls_one_with_no_arguments() {
    // Whatever it is, this process has a positive pid.
    assert!(int(r#"(defffi (c-getpid "getpid") () i32) (unsafe (c-getpid))"#) > 0);
}

#[test]
fn a_name_with_no_c_symbol_uses_its_own() {
    assert_eq!(int("(defffi abs (i32) i32) (unsafe (abs -5))"), 5);
}

#[test]
fn passes_and_returns_a_double() {
    assert_eq!(float(r#"(defffi (c-sqrt "sqrt") (f64) f64) (unsafe (c-sqrt 16.0))"#), 4.0);
}

#[test]
fn passes_and_returns_a_float() {
    // `f32` crosses the compiled boundary as the bits of its value widened to
    // `f64`, so the thunk has to narrow on the way in and widen on the way
    // out. Getting either half wrong gives a number, just not this one.
    assert_eq!(float(r#"(defffi (c-fabsf "fabsf") (f32) f32) (unsafe (c-fabsf (the f32 -2.5)))"#), 2.5);
}

#[test]
fn passes_an_unsigned_argument_to_a_void_function() {
    // `srand` answers with nothing, which still has to be a word: `()`.
    assert_eq!(
        eval(r#"(defffi (c-srand "srand") (u32) ()) (unsafe (c-srand 7))"#)
            .expect("evaluation failed")
            .1,
        Value::Empty
    );
}

#[test]
fn narrows_a_signed_argument() {
    assert_eq!(int(r#"(defffi (c-toupper "toupper") (i32) i32) (unsafe (c-toupper 97))"#), 65);
}

// ------------------------------------------------------------------ strings

/// The string a program answers with.
fn text(src: &str) -> String {
    let (h, v) = eval(src).expect("evaluation failed");
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

#[test]
fn passes_a_string() {
    // A typelisp string is not NUL-terminated, so this is a copy into a C
    // string and a free afterwards, not a pointer handed over.
    assert_eq!(int(r#"(defffi (c-strlen "strlen") (string) i32) (unsafe (c-strlen "hello"))"#), 5);
}

#[test]
fn returns_a_string() {
    // Copied out of C's memory, not borrowed from it: what `getenv` returns
    // points into the process environment and is not ours to hold.
    let path = text(r#"(defffi (c-getenv "getenv") (string) string) (unsafe (c-getenv "PATH"))"#);
    assert!(!path.is_empty(), "PATH came back empty");
}

#[test]
fn a_result_pointing_into_an_argument_is_copied_before_the_argument_is_freed() {
    // `strchr` answers with a pointer *into* the string it was given — which
    // here is the temporary C string this call made. Copying the result before
    // freeing that temporary is the whole reason the thunk orders the two that
    // way; the other order reads freed memory.
    assert_eq!(
        text(r#"(defffi (c-strchr "strchr") (string i32) string) (unsafe (c-strchr "hello world" 119))"#),
        "world"
    );
}

#[test]
fn a_string_argument_survives_a_collection() {
    // `rt_ffi_string_from_cstr` allocates, so a GC can run inside the thunk
    // while the argument's own string is still live. It is rooted by
    // `encode_crossing_args` before the call, which is what makes that safe.
    assert_eq!(
        text(
            r#"
            (defffi (c-strchr "strchr") (string i32) string)
            (defun probe ((s string)) string (unsafe (c-strchr s 119)))
            (probe (append "hello " "world"))
            "#
        ),
        "world"
    );
}

#[test]
fn a_null_result_says_so_rather_than_inventing_a_string() {
    // `string` has no value for "there wasn't one".
    let e = err(
        r#"(defffi (c-getenv "getenv") (string) string) (unsafe (c-getenv "TYPELISP_DEFINITELY_UNSET_XYZZY"))"#,
    );
    assert!(e.contains("null") && e.contains("ptr"), "unexpected error: {}", e);
}

#[test]
fn a_nul_inside_a_string_argument_is_refused() {
    let e = err(
        r#"
        (defffi (c-strlen "strlen") (string) i32)
        (unsafe (c-strlen (append "ab" (char->string (int->char 0)))))
        "#,
    );
    assert!(e.contains("NUL"), "unexpected error: {}", e);
}

// ------------------------------------------------- pointers and 64-bit words

#[test]
fn round_trips_a_pointer_through_malloc_and_free() {
    // The whole point of `ptr`: hold what C handed back, hand it back to C.
    // `malloc` answers with something non-null for 16 bytes.
    assert_eq!(
        int(
            r#"
            (defffi (c-malloc "malloc") (c-ulong) ptr)
            (defffi (c-free "free") (ptr) ())
            (unsafe
              (let ((p (c-malloc 16)))
                (c-free p)
                1))
            "#
        ),
        1
    );
}

#[test]
fn a_64_bit_result_keeps_its_top_bits() {
    // `strlen` answers with a `size_t`. The value here is small, but the
    // declaration is what proves `c-ulong` reaches the thunk as a full word
    // rather than being narrowed on the way.
    assert_eq!(
        int(r#"(defffi (c-strlen "strlen") (string) c-ulong) (as i32 (unsafe (c-strlen "hello")))"#),
        5
    );
}

#[test]
fn a_c_word_is_read_as_an_int_by_the_declaration_s_signedness() {
    // `atol("-1")` puts all ones in the register. What that word *means* is
    // whatever the declaration claimed, and `bignum` is the only target that
    // can hold either reading — `as i32` truncates and `try-as i32` answers
    // `none`, so without this there is no way to read a `size_t` at all.
    //
    // This is the one width where "unsigned" and "the word's own sign bit"
    // disagree: every narrower unsigned type is already non-negative in the
    // register.
    assert_eq!(
        text(
            r#"
            (defffi (c-atol "atol") (string) c-long)
            (format false "~a" (as int (unsafe (c-atol "-1"))))
            "#
        ),
        "-1"
    );
    assert_eq!(
        text(
            r#"
            (defffi (c-atol "atol") (string) c-ulong)
            (format false "~a" (as int (unsafe (c-atol "-1"))))
            "#
        ),
        "18446744073709551615"
    );
}

#[test]
fn a_compiled_body_reads_a_c_word_the_same_way() {
    // The interpreter reads the word in `int_to_bignum` and compiled code in
    // `rt_uint_to_bignum`, chosen by the island from the receiver's `int-wsig`
    // — two places, one answer.
    assert_eq!(
        text(
            r#"
            (defffi (c-atol "atol") (string) c-ulong)
            (defun f () int (as int (unsafe (c-atol "-1"))))
            (compile f)
            (format false "~a" (f))
            "#
        ),
        "18446744073709551615"
    );
}

#[test]
fn a_c_word_converts_to_nothing_but_an_int_and_the_widths() {
    // `f64` would round and `ratio`/`char` are not what a machine word means,
    // so the message is the plain "no conversion" one rather than a special
    // case. The width casts and `bignum` are the whole catalog.
    let e = err(
        r#"
        (defffi (c-atol "atol") (string) c-ulong)
        (defun f () f64 (as f64 (unsafe (c-atol "1"))))
        "#,
    );
    assert!(e.contains("no conversion"), "unexpected error: {e}");
}

#[test]
fn a_wrapper_may_return_a_pointer() {
    // The `unsafe` form's own value is the one exception to rule B, so a
    // function that hands a pointer back can be written at all.
    assert_eq!(
        int(
            r#"
            (defffi (c-malloc "malloc") (c-ulong) ptr)
            (defffi (c-free "free") (ptr) ())
            (defun get-block () ptr (unsafe (c-malloc 8)))
            (unsafe (c-free (get-block)))
            0
            "#
        ),
        0
    );
}

// -------------------------------------------------- where a raw word may not go

#[test]
fn a_pointer_outside_unsafe_is_a_type_error() {
    let e = err(
        r#"
        (defffi (c-malloc "malloc") (c-ulong) ptr)
        (defun leak () ptr (unsafe (c-malloc 8)))
        (defun use-it () i32 (let ((p (leak))) 1))
        "#,
    );
    assert!(e.contains("unsafe"), "unexpected error: {}", e);
}

#[test]
fn a_pointer_cannot_be_a_struct_field() {
    // Tagging it would drop its top three bits — the same reason this
    // language has no 64-bit integer type.
    let e = err("(defstruct handle (p ptr))");
    assert!(e.contains("tags what it holds"), "unexpected error: {}", e);
}

#[test]
fn a_pointer_cannot_be_an_enum_field() {
    let e = err("(defenum maybe-ptr (none) (some ptr))");
    assert!(e.contains("tags what it holds"), "unexpected error: {}", e);
}

#[test]
fn a_pointer_cannot_be_a_global() {
    let e = err(
        r#"
        (defffi (c-malloc "malloc") (c-ulong) ptr)
        (defvar (block ptr) (unsafe (c-malloc 8)))
        "#,
    );
    assert!(e.contains("tags what it holds"), "unexpected error: {}", e);
}

#[test]
fn a_pointer_cannot_be_a_type_argument() {
    let e = err("(defffi (c-nope \"abs\") ((vector ptr)) i32)");
    assert!(e.contains("inside another type"), "unexpected error: {}", e);
}

#[test]
fn a_pointer_captured_by_a_closure_is_refused_at_compile_time() {
    // A captured binding is stored in a cell, and a cell tags what it holds.
    // The checker lets this through — captures are the bridge's notion — so
    // the bridge is where it is caught, and only when the function is
    // actually compiled.
    let e = err(
        r#"
        (defffi (c-malloc "malloc") (c-ulong) ptr)
        (defffi (c-free "free") (ptr) ())
        (defun leak () i32
          (unsafe
            (let ((p (c-malloc 8)))
              (labels ((cleanup () () (c-free p)))
                (cleanup)
                1))))
        (compile leak)
        "#,
    );
    assert!(e.contains("cell"), "unexpected error: {}", e);
}

#[test]
fn a_raw_word_carries_no_arithmetic() {
    // Deliberate: these are words on the way to or from C, not numbers.
    let e = err(
        r#"
        (defffi (c-strlen "strlen") (string) c-ulong)
        (unsafe (+ (c-strlen "ab") 1))
        "#,
    );
    assert!(!e.is_empty(), "arithmetic on a c-ulong must not check");
}

// --------------------------------------------------------- the compiled path

#[test]
fn a_compiled_body_can_call_one() {
    // The point of naming the thunk the way a `defun`'s body is named: the
    // island emits an ordinary call to `tl_c-abs` and never learns that C is
    // on the other end of it.
    assert_eq!(
        int(
            r#"
            (defffi (c-abs "abs") (i32) i32)
            (defun f ((n i32)) i32 (unsafe (c-abs n)))
            (compile f)
            (f -7)
            "#
        ),
        7
    );
}

#[test]
fn a_safe_wrapper_can_be_called_from_safe_code() {
    // The intended shape: `unsafe` once, at the declaration's only call site,
    // and a checked signature for everyone else.
    assert_eq!(
        int(
            r#"
            (defffi (c-abs "abs") (i32) i32)
            (defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
            (+ (abs-i32 -3) (abs-i32 4))
            "#
        ),
        7
    );
}

#[test]
fn compiling_one_is_a_no_op_rather_than_an_error() {
    // `(compile f)` walks `f`'s call graph, and a declaration is already
    // compiled — the thunk is what `compile` would otherwise be asked to
    // produce. So the request is satisfied, not refused.
    assert_eq!(
        int(r#"(defffi (c-abs "abs") (i32) i32) (compile c-abs) (unsafe (c-abs -4))"#),
        4
    );
}

#[test]
fn disassembling_one_says_why_it_cannot() {
    // The failure worth avoiding is the confident wrong answer: a module built
    // from an empty body disassembles to a function that is not the one the
    // name reaches.
    let e = err(r#"(defffi (c-abs "abs") (i32) i32) (disassemble c-abs)"#);
    assert!(e.contains("defffi"), "unexpected error: {}", e);
}

// ------------------------------------------------------------ permission

#[test]
fn calling_one_outside_unsafe_is_a_type_error() {
    let e = err(r#"(defffi (c-abs "abs") (i32) i32) (c-abs -5)"#);
    assert!(e.contains("unsafe"), "unexpected error: {}", e);
}

#[test]
fn taking_one_as_a_value_is_refused() {
    // Reifying a name builds an interpreted closure out of the definition's
    // body, and this definition has no body — so the value would be a
    // function that quietly answers `()`. Refusing beats that, and the error
    // says what to write instead.
    let e = err(
        r#"
        (defffi (c-abs "abs") (i32) i32)
        (defun run-it ((f (fn (i32) i32))) i32 (f -9))
        (unsafe (run-it c-abs))
        "#,
    );
    assert!(e.contains("cannot be used as a value") && e.contains("lambda"), "unexpected error: {}", e);
}

#[test]
fn a_lambda_around_the_call_is_a_value() {
    // The workaround the refusal names, and the reason it works: the lambda's
    // body is an ordinary call site.
    assert_eq!(
        int(
            r#"
            (defffi (c-abs "abs") (i32) i32)
            (defun run-it ((f (fn (i32) i32))) i32 (f -9))
            (run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
            "#
        ),
        9
    );
}

// --------------------------------------------------------------- refusals

#[test]
fn a_symbol_that_is_not_there_is_reported_by_name() {
    let e = err(r#"(defffi (c-nope "a_c_function_nothing_defines_xyzzy") (i32) i32)"#);
    assert!(
        e.contains("a_c_function_nothing_defines_xyzzy") && e.contains(":library"),
        "unexpected error: {}",
        e
    );
}

#[test]
fn a_library_that_is_not_there_lists_what_was_tried() {
    let e = err(r#"(defffi (c-nope "nope") (i32) i32 :library "typelisp-probe-absent")"#);
    assert!(
        e.contains("typelisp-probe-absent") && e.contains("tried"),
        "unexpected error: {}",
        e
    );
}

#[test]
fn a_type_c_cannot_be_given_is_refused() {
    // `sexpr` is a perfectly good typelisp type and no C type at all.
    let e = err(r#"(defffi (c-nope "abs") (sexpr) i32)"#);
    assert!(e.contains("cannot spell"), "unexpected error: {}", e);
}

#[test]
fn a_unit_parameter_is_refused() {
    // `()` is a return type here, not something to pass.
    let e = err(r#"(defffi (c-nope "abs") (()) i32)"#);
    assert!(e.contains("()"), "unexpected error: {}", e);
}

#[test]
fn rest_cannot_be_declared() {
    let e = err(r#"(defffi (c-printf "printf") (i32 &rest i32) i32)"#);
    assert!(e.contains("variadic"), "unexpected error: {}", e);
}

#[test]
fn a_builtin_s_name_is_refused() {
    // A compiled call to `sexpr-car` reaches the runtime shim by that name,
    // so a declaration under it would be miscompiled rather than shadowed.
    // In a module the checker's own redefinition check never sees the clash.
    let e = err(r#"(module m (defffi (sexpr-car "abs") (i32) i32))"#);
    assert!(e.contains("builtin"), "unexpected error: {}", e);
}

#[test]
fn declaring_a_name_twice_is_refused() {
    let e = err(r#"(defffi (c-abs "abs") (i32) i32) (defffi (c-abs "labs") (i32) i32)"#);
    assert!(e.contains("already defined"), "unexpected error: {}", e);
}

#[test]
fn a_generic_declaration_is_refused() {
    let e = err(r#"(defffi (c-id<t> "abs") (t) t)"#);
    assert!(!e.is_empty(), "a generic FFI declaration must not be accepted");
}
