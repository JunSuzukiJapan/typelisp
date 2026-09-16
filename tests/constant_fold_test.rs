//! Constant folding at lowering time (`check::fold`).
//!
//! Three things are held here. **What folds**: a scalar builtin over literal
//! arguments lowers to the literal it evaluates to, in the receiver type's own
//! arithmetic — `u8` wraps, `int` promotes to a bignum — and an `if` on a
//! literal condition lowers to the branch it picks. **What does not**: a call
//! that would raise, a call with a non-literal argument, a builtin whose
//! result has no literal node. **That the fold is the value**: the folded
//! program and the unfolded one agree, interpreted and compiled, because the
//! evaluator's own builtins computed the fold.
//!
//! The last test is the one that licenses `fold`'s `unreachable!`s: every
//! builtin method of every scalar receiver whose result is a literal type is
//! run through the fold on synthesized literal arguments, so a builtin the
//! evaluator's dispatch has no arm for cannot hide until a user writes it.

mod common;

use common::{eval_ok, eval_ok_compiled, eval_string, eval_string_compiled, Load, Session};
use typelisp::check::core;
use typelisp::{Path, Type};

/// `src`'s lowered form, printed, checked against the bare builtins.
fn lowered(src: &str) -> String {
    let mut s = Session::new(Load::Bare);
    let tl = s.check_one(src).expect("check failed");
    core::print(&s.heap, tl)
}

/// [`lowered`] with the prelude, for source that uses its macros.
fn lowered_with_prelude(src: &str) -> String {
    let mut s = Session::new(Load::Prelude);
    let tl = s.check_one(src).expect("check failed");
    core::print(&s.heap, tl)
}

// ---- what folds -----------------------------------------------------------

#[test]
fn int_arithmetic_folds_to_a_literal() {
    assert_eq!(lowered("(+ 1 2)"), "(expr (int 3))");
    assert_eq!(lowered("(- 10 4)"), "(expr (int 6))");
    assert_eq!(lowered("(* 6 7)"), "(expr (int 42))");
    assert_eq!(lowered("(/ 7 2)"), "(expr (int 3))");
    assert_eq!(lowered("(mod -7 2)"), "(expr (int 1))");
    assert_eq!(lowered("(max 1 2)"), "(expr (int 2))");
    assert_eq!(lowered("(logand 12 10)"), "(expr (int 8))");
    assert_eq!(lowered("(ash 1 10)"), "(expr (int 1024))");
}

#[test]
fn nested_and_variadic_calls_fold_bottom_up() {
    // `(+ 1 2 3)` is two binary calls; the inner folds first and the outer
    // then sees two literals.
    assert_eq!(lowered("(+ 1 2 3)"), "(expr (int 6))");
    assert_eq!(lowered("(* (+ 1 2) (- 10 6))"), "(expr (int 12))");
    // A fold inside a body that is otherwise not constant.
    assert_eq!(
        lowered("(defun f ((x int)) int (+ x (* 2 3)))"),
        "(defun f ((x int)) int false (assoc int + true () int (int int) \"int\" (var x) (int 6)))"
    );
}

#[test]
fn fixed_widths_fold_in_their_own_arithmetic() {
    // `u8` wraps: 200 + 100 = 300 = 44 (mod 256).
    assert_eq!(lowered("(+ (the u8 200) 100)"), "(expr (int-any-width 44))");
    assert_eq!(lowered("(- (the i32 1) 2)"), "(expr (int-any-width -1))");
    // `i8` wraps on the sign: 127 + 1 = -128.
    assert_eq!(lowered("(+ (the i8 127) 1)"), "(expr (int-any-width -128))");
}

#[test]
fn int_folds_promote_to_a_bignum_past_the_fixnum() {
    // 2^62 * 2 = 2^63, one past the fixnum range.
    assert_eq!(lowered("(* 4611686018427387904 2)"), "(expr (bignum 9223372036854775808))");
    // And demote back when the result fits again.
    assert_eq!(lowered("(- 9223372036854775808 9223372036854775807)"), "(expr (int 1))");
}

#[test]
fn float_ratio_char_and_bool_fold() {
    assert_eq!(lowered("(* 2.5 2.0)"), "(expr (float-any-width 5.0))");
    assert_eq!(lowered("(+ (the f32 0.5) 0.25)"), "(expr (float-any-width 0.75f32))");
    assert_eq!(lowered("(+ 1/2 1/3)"), "(expr (ratio 5/6))");
    assert_eq!(lowered("(< #\\a #\\b)"), "(expr (bool true))");
    assert_eq!(lowered("(upcase #\\a)"), "(expr (char #\\A))");
    assert_eq!(lowered("(eq true false)"), "(expr (bool false))");
}

#[test]
fn comparisons_and_conversions_fold() {
    assert_eq!(lowered("(< 1 2)"), "(expr (bool true))");
    assert_eq!(lowered("(= 1 2)"), "(expr (bool false))");
    assert_eq!(lowered("(equal 1 1)"), "(expr (bool true))");
    assert_eq!(lowered("(as f64 3)"), "(expr (float-any-width 3.0))");
    assert_eq!(lowered("(as int 2.75)"), "(expr (int 2))");
    assert_eq!(lowered("(as u8 300)"), "(expr (int-any-width 44))");
    assert_eq!(lowered("(as char 65)"), "(expr (char #\\A))");
}

#[test]
fn if_on_a_literal_condition_lowers_to_its_branch() {
    assert_eq!(lowered("(if true 1 2)"), "(expr (int 1))");
    assert_eq!(lowered("(if false 1 2)"), "(expr (int 2))");
    // The condition folded first, then the `if`.
    assert_eq!(lowered("(if (< 1 2) 3 4)"), "(expr (int 3))");
    // `and`/`or` are `if`s under the prelude's macros, so a literal chain
    // collapses all the way.
    assert_eq!(lowered_with_prelude("(and (< 1 2) (> 3 2))"), "(expr (bool true))");
    assert_eq!(lowered_with_prelude("(or false (= 1 2))"), "(expr (bool false))");
}

#[test]
fn a_dead_branch_is_still_checked() {
    let mut s = Session::new(Load::Bare);
    // `false` picks the else branch, but the then branch is still a program
    // that has to be well-typed.
    let err = s.check_one("(if false (+ 1 \"x\") 2)").expect_err("the dead branch is checked");
    assert!(matches!(err, typelisp::Error::TypeError(_)), "{:?}", err);
    // And the type is still the join of both, so a diverging branch that
    // was folded away leaves the other's type.
    let mut s = Session::new(Load::Bare);
    s.check_one("(if true (panic \"never\") 2)").expect("check");
    assert_eq!(s.checker.expr_type(), Some(&Type::Int));
}

// ---- what does not --------------------------------------------------------

#[test]
fn a_call_that_would_raise_is_left_to_raise_at_runtime() {
    assert_eq!(
        lowered("(/ 1 0)"),
        "(expr (assoc int / true () int (int int) \"int\" (int 1) (int 0)))"
    );
    assert_eq!(
        lowered("(mod (the i32 1) 0)"),
        "(expr (assoc i32 mod true () int-any-width (int-any-width int-any-width) \"i32\" (int-any-width 1) (int-any-width 0)))"
    );
    // The fold must not have turned the runtime panic into a check error.
    let mut s = Session::new(Load::Prelude);
    let err = s.eval_kind("(/ 1 0)").expect_err("divides by zero at runtime");
    assert!(format!("{:?}", err).contains("divide by zero"), "{:?}", err);
}

#[test]
fn a_non_literal_argument_is_not_folded() {
    assert_eq!(
        lowered("(let ((x 5)) (+ x 1))"),
        "(expr (let ((x int (int 5))) (assoc int + true () int (int int) \"int\" (var x) (int 1))))"
    );
}

#[test]
fn a_result_with_no_literal_node_is_not_folded() {
    // `Unit` — `int::print`, the one builtin here with a side effect — is
    // excluded by its result type; `print` itself is a special form over a
    // control string, so the method cannot be written down and is covered by
    // `every_scalar_builtin_with_a_literal_result_folds` skipping it.
    // An `Option`.
    assert!(lowered("(try-int->char 65)").contains("(assoc int try-int->char"));
    // A heap string, whose identity `eq` could observe.
    assert!(lowered("(char->string #\\a)").contains("(assoc char char->string"));
}

// ---- the fold is the value ------------------------------------------------

#[test]
fn folded_and_unfolded_programs_agree() {
    // Each case: a literal expression (folded at lowering) and the same
    // computation as a function of its operands (not foldable), compared
    // with `equal` — interpreted, and with the function compiled.
    let cases: &[(&str, &str, &str)] = &[
        ("(+ (the u8 200) 100)", "(defun g ((a u8) (b u8)) u8 (+ a b))", "(g 200 100)"),
        ("(* 4611686018427387904 2)", "(defun g ((a int) (b int)) int (* a b))", "(g 4611686018427387904 2)"),
        ("(- 9223372036854775808 9223372036854775807)", "(defun g ((a int) (b int)) int (- a b))", "(g 9223372036854775808 9223372036854775807)"),
        ("(mod -7 2)", "(defun g ((a int) (b int)) int (mod a b))", "(g -7 2)"),
        ("(+ (the f32 0.1) 0.2)", "(defun g ((a f32) (b f32)) f32 (+ a b))", "(g 0.1 0.2)"),
        ("(+ 1/2 1/3)", "(defun g ((a ratio) (b ratio)) ratio (+ a b))", "(g 1/2 1/3)"),
        ("(if (< 1 2) 3 4)", "(defun g ((a int) (b int)) int (if (< a b) 3 4))", "(g 1 2)"),
        ("(as u8 300)", "(defun g ((a int)) u8 (as u8 a))", "(g 300)"),
    ];
    for (folded, def, call) in cases {
        let expr = format!("(equal {} {})", folded, call);
        let interpreted = eval_string(&format!("{}\n(format false \"~a\" {})", def, expr));
        assert_eq!(interpreted, "true", "interpreted: {}", expr);
        let compiled = eval_string_compiled(&format!("{}\n(compile g)\n(format false \"~a\" {})", def, expr));
        assert_eq!(compiled, "true", "compiled: {}", expr);
    }
}

// ---- the fold covers every scalar builtin --------------------------------

/// A literal of `ty`, spelled so that it checks in a `ty` position and so
/// that no builtin below raises on it (`1` divides, shifts and converts
/// cleanly). `None` for a type that has no literal, which the assertion
/// below reports rather than skips.
fn literal_of(ty: &Type) -> Option<String> {
    Some(match ty {
        Type::Int => "1".to_string(),
        Type::F64 => "1.0".to_string(),
        Type::F32 => "(the f32 1.0)".to_string(),
        Type::Ratio => "1/2".to_string(),
        Type::Char => "#\\a".to_string(),
        Type::Bool => "true".to_string(),
        t if t.is_integer() => format!("(the {} 1)", typelisp::type_key::type_key_of_type(t)),
        _ => return None,
    })
}

fn is_literal_tag(tag: &str) -> bool {
    matches!(tag, "int" | "int-any-width" | "float-any-width" | "bignum" | "ratio" | "char" | "bool")
}

#[test]
fn every_scalar_builtin_with_a_literal_result_folds() {
    let receivers = ["i8", "i16", "i32", "u8", "u16", "u32", "int", "f32", "f64", "ratio", "char", "bool"];
    let mut unfolded = Vec::new();
    let mut no_literal = Vec::new();
    let mut exercised = 0;
    for recv in receivers {
        let def = Session::new(Load::Bare).checker.registry().type_def(&Path::root(recv)).expect("a primitive is registered").clone();
        for (name, af) in &def.assoc {
            if !af.builtin || !af.instance {
                continue;
            }
            let ret = &af.sig.ret;
            let ret_is_literal = ret.is_int_family() || ret.is_float() || matches!(ret, Type::Ratio | Type::Char | Type::Bool);
            if !ret_is_literal {
                continue;
            }
            let args: Vec<String> = match af.sig.params.iter().map(literal_of).collect() {
                Some(args) => args,
                None => {
                    no_literal.push(format!("{}::{} takes {:?}", recv, name, af.sig.params));
                    continue;
                }
            };
            exercised += 1;
            let src = format!("({} {})", name, args.join(" "));
            let mut s = Session::new(Load::Bare);
            let tl = match s.check_one(&src) {
                Ok(tl) => tl,
                Err(e) => panic!("{} did not check: {:?}", src, e),
            };
            // `(expr NODE)`: the node is the fold's output.
            let node = core::field(&s.heap, tl, 0).expect("an expr wraps one node");
            let tag = core::op(&s.heap, node).expect("a node has a tag").to_string();
            if !is_literal_tag(&tag) {
                unfolded.push(format!("{} => {}", src, core::print(&s.heap, node)));
            }
        }
    }
    assert!(no_literal.is_empty(), "builtins whose parameters have no literal to fold with:\n{}", no_literal.join("\n"));
    assert!(unfolded.is_empty(), "scalar builtins that did not fold on literal arguments:\n{}", unfolded.join("\n"));
    // The six widths alone carry ~40 builtins each; a count this low means
    // the walk above stopped seeing them.
    assert!(exercised > 200, "only {} builtins were exercised", exercised);
}

#[test]
fn folding_survives_gc_stress() {
    // Every allocation collects: the fold's fresh box and the node it goes
    // under have to be rooted through each other's construction.
    let mut s = Session::with_capacity(Load::Bare, 1 << 12);
    s.heap.set_gc_stress(true);
    let tl = s.check_one("(+ (* 2.5 2.0) (* 4611686018427387904 2))").expect_err("f64 + int is a type error");
    let _ = tl;
    let mut s = Session::with_capacity(Load::Bare, 1 << 12);
    s.heap.set_gc_stress(true);
    let tl = s.check_one("(+ (* 4611686018427387904 2) (- 1/2 1/2))").expect_err("int + ratio is a type error");
    let _ = tl;
    let mut s = Session::with_capacity(Load::Bare, 1 << 12);
    s.heap.set_gc_stress(true);
    let tl = s.check_one("(* (* 4611686018427387904 2) (* 4611686018427387904 2))").expect("check");
    assert_eq!(core::print(&s.heap, tl), "(expr (bignum 85070591730234615865843651857942052864))");
    let mut s = Session::with_capacity(Load::Bare, 1 << 12);
    s.heap.set_gc_stress(true);
    let tl = s.check_one("(* (* 2.5 2.0) (* 2.5 2.0))").expect("check");
    assert_eq!(core::print(&s.heap, tl), "(expr (float-any-width 25.0))");
    assert_eq!(eval_ok_compiled("(* (* 2.5 2.0) (* 2.5 2.0))"), eval_ok("(* (* 2.5 2.0) (* 2.5 2.0))"));
}
