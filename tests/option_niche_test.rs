//! The `Option<T>` niche (`check/repr.rs`'s `Repr::Niche`,
//! `typelisp-mem`'s `option`): an `Option` whose payload can never be the
//! empty-list word is represented as that payload's tagged word for `some`
//! and the empty-list immediate for `none`, with no box — in the
//! interpreter, in compiled code, and across the boundary between them. The
//! exclusions are the payloads that *can* be that word: a nested `Option`,
//! `()`, a raw C word.
//!
//! What a niche costs is that the value no longer says what it is, so every
//! place a value is looked at without a static type — a `Sexpr` slot, a
//! struct field being printed, a REPL echo, a trace line — has to be told.
//! The tests in the second half are those places.

mod common;

use common::{check_err, eval_ok, eval_string, eval_string_compiled, Load, Session};
use typelisp::check::repr::Repr;
use typelisp::type_key::type_key_of_type;
use typelisp::{Path, Type, Value};

/// `expr` rendered with `~a`, interpreted and compiled (after `(compile
/// NAME)` for each of `names`), asserting the two agree; returns the text.
fn both(defs: &str, names: &[&str], expr: &str) -> String {
    let interpreted = eval_string(&format!("{}\n(format false \"~a\" {})", defs, expr));
    let compiles: String = names.iter().map(|n| format!("(compile {})\n", n)).collect();
    let compiled = eval_string_compiled(&format!("{}\n{}(format false \"~a\" {})", defs, compiles, expr));
    assert_eq!(interpreted, compiled, "interpreted and compiled disagree for {}", expr);
    interpreted
}

// ---- representation ------------------------------------------------------

/// The checker's decision (`Repr::of_by` on a `Type`) and the runtime's
/// (`option_payload_niches` on a type key) are two spellings of one rule,
/// and every producer of an `Option` value uses one or the other — so they
/// are pinned together here over every kind of payload.
#[test]
fn the_type_side_and_the_key_side_agree_on_what_niches() {
    let named = |n: &str, args: Vec<Type>| Type::Named(Path::root(n), args);
    let option = |t: Type| named("option", vec![t]);
    let payloads = vec![
        Type::Int,
        Type::I32,
        Type::U8,
        Type::F64,
        Type::F32,
        Type::Char,
        Type::Bool,
        Type::Str,
        Type::Symbol,
        Type::Ratio,
        Type::RandomState,
        Type::Unit,
        Type::Never,
        Type::Ptr,
        Type::CLong,
        Type::CULong,
        named("sexpr", vec![]),
        named("vector", vec![Type::Int]),
        named("hashtable", vec![Type::Str, Type::Int]),
        named("result", vec![Type::Int, named("parseinterror", vec![])]),
        named("llvm-value", vec![]),
        Type::Fn(vec![Type::Int, Type::Str], None, Box::new(Type::Int)),
        Type::Dyn(Path::root("shape"), vec![]),
        option(Type::Int),
        option(named("sexpr", vec![])),
        option(option(Type::Int)),
    ];
    for payload in payloads {
        let ty = option(payload.clone());
        let by_type = Repr::of_by(&ty, &|_| None).niche_payload().is_some();
        let by_key = typelisp_mem::is_niched_option_key(&type_key_of_type(&ty));
        assert_eq!(by_type, by_key, "Option<{:?}> ({})", payload, type_key_of_type(&ty));
    }
}

/// `(some 5)` typed `Option<int>` is the fixnum `5` and `(none)` the empty
/// list — nothing is allocated for either.
#[test]
fn option_of_int_is_a_fixnum_or_nil_with_no_allocation() {
    let mut s = Session::new(Load::Prelude);
    s.eval("(defun s () Option<int> (option::some 5))\n(defun n () Option<int> (option::none))").expect("defs");
    let before = s.heap.box_count();
    assert_eq!(s.eval("(s)").expect("some"), Value::Int(5));
    assert_eq!(s.eval("(n)").expect("none"), Value::Empty);
    assert_eq!(s.heap.box_count(), before, "a niched Option allocates no box");
}

/// A float payload is a float box already, and the `Option` is that same
/// box — no second box around it.
#[test]
fn option_of_f64_carries_the_float_box_directly() {
    let mut s = Session::new(Load::Prelude);
    s.eval("(defun s () Option<f64> (option::some 1.5))").expect("defs");
    let Value::Boxed(id) = s.eval("(s)").expect("some") else { panic!("expected the float box") };
    assert!(s.heap.is_f64(id));
    assert_eq!(s.heap.f64_value(id), 1.5);
}

/// The inner `Option`'s `none` is the word the outer one would use, so the
/// outer stays a box — and its two `none`s are different values.
#[test]
fn option_of_option_still_boxes_and_its_two_nones_differ() {
    let mut s = Session::new(Load::Prelude);
    s.eval(
        "(defun outer-none () Option<Option<int>> (option::none))\n\
         (defun inner-none () Option<Option<int>> (option::some (option::none)))\n\
         (defun both () Option<Option<int>> (option::some (option::some 7)))",
    )
    .expect("defs");
    let Value::Boxed(outer) = s.eval("(outer-none)").expect("outer") else { panic!("the outer none is a box") };
    assert!(s.heap.is_enum(outer));
    assert_eq!(s.heap.enum_variant(outer), 1);
    let Value::Boxed(inner) = s.eval("(inner-none)").expect("inner") else { panic!("the outer some is a box") };
    assert_eq!(s.heap.enum_variant(inner), 0);
    // The inner `none` rides inside as the empty word.
    assert_eq!(s.heap.enum_field(inner, 0), Value::Empty);
    let Value::Boxed(some) = s.eval("(both)").expect("both") else { panic!("boxed") };
    assert_eq!(s.heap.enum_field(some, 0), Value::Int(7));
    assert_eq!(
        eval_string("(defun f ((o Option<Option<int>>)) int (match o ((some (some n)) n) ((some (none)) -1) ((none) -2)))\n(format false \"~a ~a ~a\" (f (option::some (option::some 7))) (f (option::some (option::none))) (f (option::none)))"),
        "7 -1 -2"
    );
}

/// `()` has one value, and it is the empty word: an `Option<()>` boxes.
#[test]
fn option_of_unit_still_boxes() {
    let mut s = Session::new(Load::Prelude);
    s.eval("(defun s () Option<()> (option::some ()))\n(defun n () Option<()> (option::none))").expect("defs");
    let Value::Boxed(some) = s.eval("(s)").expect("some") else { panic!("boxed") };
    assert_eq!(s.heap.enum_variant(some), 0);
    let Value::Boxed(none) = s.eval("(n)").expect("none") else { panic!("boxed") };
    assert_eq!(s.heap.enum_variant(none), 1);
}

// ---- both tiers -----------------------------------------------------------

/// Every payload kind the niche tags differently — a narrow integer (a
/// fixnum), an `int` (its own word), a float (a box), a char and a bool
/// (immediates), a string, a struct and a `Sexpr` (tagged words) — built
/// and matched on each side of the compile boundary, and crossing it in
/// both directions.
#[test]
fn some_and_none_round_trip_through_compiled_and_interpreted_code_for_every_niche_kind() {
    const DEFS: &str = r#"
        (defstruct pt (x int))
        (defun u8-of ((o Option<u8>)) int (match o ((some v) (as int v)) ((none) -1)))
        (defun int-of ((o Option<int>)) int (match o ((some v) v) ((none) -1)))
        (defun f64-of ((o Option<f64>)) f64 (match o ((some v) v) ((none) -1.0)))
        (defun f32-of ((o Option<f32>)) f32 (match o ((some v) v) ((none) (as f32 -1.0))))
        (defun char-of ((o Option<char>)) char (match o ((some v) v) ((none) #\-)))
        (defun bool-of ((o Option<bool>)) int (match o ((some true) 1) ((some _) 0) ((none) -1)))
        (defun str-of ((o Option<string>)) string (match o ((some v) v) ((none) "-")))
        (defun pt-of ((o Option<pt>)) int (match o ((some p) p::x) ((none) -1)))
        (defun sexpr-of ((o Option<Sexpr>)) Sexpr (match o ((some v) v) ((none) 0)))
        (defun mk-u8 ((b bool)) Option<u8> (if b (option::some (the u8 200)) (option::none)))
        (defun mk-int ((b bool)) Option<int> (if b (option::some 100000000000000000000) (option::none)))
        (defun mk-f64 ((b bool)) Option<f64> (if b (option::some 2.5) (option::none)))
        (defun mk-f32 ((b bool)) Option<f32> (if b (option::some (as f32 0.5)) (option::none)))
        (defun mk-char ((b bool)) Option<char> (if b (option::some #\z) (option::none)))
        (defun mk-bool ((b bool)) Option<bool> (if b (option::some false) (option::none)))
        (defun mk-str ((b bool)) Option<string> (if b (option::some "hi") (option::none)))
        (defun mk-pt ((b bool)) Option<pt> (if b (option::some (pt::new 9)) (option::none)))
        (defun mk-sexpr ((b bool)) Option<Sexpr> (if b (option::some (unwrap (list 1 2))) (option::none)))
    "#;
    const NAMES: &[&str] = &[
        "u8-of", "int-of", "f64-of", "f32-of", "char-of", "bool-of", "str-of", "pt-of", "sexpr-of", "mk-u8",
        "mk-int", "mk-f64", "mk-f32", "mk-char", "mk-bool", "mk-str", "mk-pt", "mk-sexpr",
    ];
    // Interpreted producer into compiled consumer and back: `both` compiles
    // every name, so each pairing crosses the boundary in one direction or
    // the other depending on which side the call is evaluated on.
    assert_eq!(both(DEFS, NAMES, "(u8-of (mk-u8 true))"), "200");
    assert_eq!(both(DEFS, NAMES, "(u8-of (mk-u8 false))"), "-1");
    assert_eq!(both(DEFS, NAMES, "(int-of (mk-int true))"), "100000000000000000000");
    assert_eq!(both(DEFS, NAMES, "(int-of (mk-int false))"), "-1");
    assert_eq!(both(DEFS, NAMES, "(f64-of (mk-f64 true))"), "2.5");
    assert_eq!(both(DEFS, NAMES, "(f64-of (mk-f64 false))"), "-1.0");
    assert_eq!(both(DEFS, NAMES, "(f32-of (mk-f32 true))"), "0.5");
    assert_eq!(both(DEFS, NAMES, "(f32-of (mk-f32 false))"), "-1.0");
    assert_eq!(both(DEFS, NAMES, "(char-of (mk-char true))"), "z");
    assert_eq!(both(DEFS, NAMES, "(char-of (mk-char false))"), "-");
    assert_eq!(both(DEFS, NAMES, "(bool-of (mk-bool true))"), "0");
    assert_eq!(both(DEFS, NAMES, "(bool-of (mk-bool false))"), "-1");
    assert_eq!(both(DEFS, NAMES, "(str-of (mk-str true))"), "hi");
    assert_eq!(both(DEFS, NAMES, "(str-of (mk-str false))"), "-");
    assert_eq!(both(DEFS, NAMES, "(pt-of (mk-pt true))"), "9");
    assert_eq!(both(DEFS, NAMES, "(pt-of (mk-pt false))"), "-1");
    assert_eq!(both(DEFS, NAMES, "(sexpr-of (mk-sexpr true))"), "(1 2)");
    assert_eq!(both(DEFS, NAMES, "(sexpr-of (mk-sexpr false))"), "0");
}

/// A literal sub-pattern under `some` compares the payload after it is read
/// back through the niche — a narrow integer untagged, a float box opened.
#[test]
fn a_literal_under_some_compares_the_untagged_payload() {
    const DEFS: &str = r#"
        (defun is-seven ((o Option<u8>)) bool (match o ((some 7) true) (_ false)))
        (defun is-half ((o Option<f64>)) bool (match o ((some 0.5) true) (_ false)))
        (defun is-a ((o Option<char>)) bool (match o ((some #\a) true) (_ false)))
    "#;
    const NAMES: &[&str] = &["is-seven", "is-half", "is-a"];
    assert_eq!(both(DEFS, NAMES, "(list (is-seven (option::some (the u8 7))) (is-seven (option::some (the u8 8))) (is-seven (option::none)))"), "(true false false)");
    assert_eq!(both(DEFS, NAMES, "(list (is-half (option::some 0.5)) (is-half (option::some 0.25)) (is-half (option::none)))"), "(true false false)");
    assert_eq!(both(DEFS, NAMES, "(list (is-a (option::some #\\a)) (is-a (option::some #\\b)) (is-a (option::none)))"), "(true false false)");
}

/// The builtins that answer an `Option` from Rust — `HashTable::get`,
/// `Vector::pop`, `try-as` — answer the niche too, in both tiers, and a
/// `HashTable<K,Option<V>>::get` (whose answer is an `Option<Option<V>>`)
/// still boxes.
#[test]
fn builtin_option_results_use_the_niche_in_both_tiers() {
    const DEFS: &str = r#"
        (defun lookup ((h HashTable<string,int>) (k string)) int (match (get h k) ((some v) v) ((none) -1)))
        (defun popped ((v Vector<int>)) int (match (pop v) ((some x) x) ((none) -1)))
        (defun narrow ((n int)) int (match (try-as u8 n) ((some x) (as int x)) ((none) -1)))
        (defun nested ((h HashTable<string,Option<int>>) (k string)) int
          (match (get h k) ((some (some v)) v) ((some (none)) -2) ((none) -1)))
        (defun table () HashTable<string,int> (let ((h (the HashTable<string,int> (HashTable::new)))) (set h "a" 1) h))
        (defun vec () Vector<int> (let ((v (the Vector<int> (Vector::new)))) (push v 5) v))
        (defun nested-table () HashTable<string,Option<int>>
          (let ((h (the HashTable<string,Option<int>> (HashTable::new)))) (set h "a" (option::some 1)) (set h "b" (option::none)) h))
    "#;
    const NAMES: &[&str] = &["lookup", "popped", "narrow", "nested", "table", "vec", "nested-table"];
    assert_eq!(both(DEFS, NAMES, "(list (lookup (table) \"a\") (lookup (table) \"z\"))"), "(1 -1)");
    assert_eq!(both(DEFS, NAMES, "(let ((v (vec))) (list (popped v) (popped v)))"), "(5 -1)");
    assert_eq!(both(DEFS, NAMES, "(list (narrow 200) (narrow 300))"), "(200 -1)");
    assert_eq!(
        both(DEFS, NAMES, "(list (nested (nested-table) \"a\") (nested (nested-table) \"b\") (nested (nested-table) \"z\"))"),
        "(1 -2 -1)"
    );
}

/// An omitted defaultless `&optional`/`&key` argument is an `Option<T>` the
/// *call site* builds — `Checker::option_form`, the same producer `(some x)`
/// goes through — so the callee's `pat-some`/`pat-empty` read the shape they
/// expect. (The first full run of this stage had the call site still
/// boxing, and every prelude function with an omitted optional failed.)
#[test]
fn an_omitted_optional_argument_is_the_niche_the_callee_matches() {
    const DEFS: &str = r#"
        (defun opt ((a int) &optional (b int)) int (match b ((some v) (+ a v)) ((none) (- a))))
        (defun key ((a int) &key (b string)) string (match b ((some s) s) ((none) "none")))
    "#;
    assert_eq!(both(DEFS, &["opt", "key"], "(list (opt 1) (opt 1 2))"), "(-1 3)");
    assert_eq!(both(DEFS, &["opt", "key"], "(list (key 1) (key 1 :b \"x\"))"), "(none x)");
}

// ---- the places a value is looked at without its type --------------------

/// A niched `Option` handed to the printer prints as `(some ...)`/`none`,
/// exactly as a boxed one does: as a direct argument (boxed at the `Sexpr`
/// boundary by `box-option`), as a struct field, as an enum field, as a
/// `Vector` element, and nested inside a boxed `Option` — the renderer is
/// told by the static type in each case, since the word cannot say.
#[test]
fn a_niched_option_still_prints_as_some_and_none() {
    const DEFS: &str = r#"
        (defstruct pair (a Option<int>) (b Option<string>))
        (defstruct gen<T> (v T))
        (defenum shape (circle Option<f64>) (dot))
        (defun mk-pair () pair (pair::new (option::some 1) (option::none)))
        (defun mk-gen () gen<Option<int>> (gen::new (option::some 3)))
        (defun mk-gen-none () gen<Option<int>> (gen::new (option::none)))
        (defun mk-shape () shape (shape::circle (option::some 1.5)))
        (defun mk-vec () Vector<Option<int>> (let ((v (the Vector<Option<int>> (Vector::new)))) (push v (option::some 1)) (push v (option::none)) v))
        (defun mk-nested () Option<Option<int>> (option::some (option::none)))
        (defun mk-ok () Result<Option<int>,ParseIntError> (result::ok (option::some 4)))
        (defun opt ((b bool)) Option<int> (if b (option::some 5) (option::none)))
    "#;
    const NAMES: &[&str] = &["mk-pair", "mk-gen", "mk-gen-none", "mk-shape", "mk-vec", "mk-nested", "mk-ok", "opt"];
    assert_eq!(both(DEFS, NAMES, "(opt true)"), "(some 5)");
    assert_eq!(both(DEFS, NAMES, "(opt false)"), "none");
    assert_eq!(both(DEFS, NAMES, "(mk-pair)"), "#<pair (some 1) none>");
    assert_eq!(both(DEFS, NAMES, "(mk-gen)"), "#<gen<option<int>> (some 3)>");
    assert_eq!(both(DEFS, NAMES, "(mk-gen-none)"), "#<gen<option<int>> none>");
    assert_eq!(both(DEFS, NAMES, "(mk-shape)"), "(circle (some 1.5))");
    assert_eq!(both(DEFS, NAMES, "(mk-vec)"), "#<vector<option<int>> (some 1) none>");
    assert_eq!(both(DEFS, NAMES, "(mk-nested)"), "(some none)");
    assert_eq!(both(DEFS, NAMES, "(mk-ok)"), "(ok (some 4))");
    // In a list, the element's own expectation is `Option<Sexpr>`, so a bare
    // `(some 5)` *is* the niche `5`; spelled as an `Option<int>` it is boxed
    // for the slot and keeps its wrapper.
    assert_eq!(both(DEFS, NAMES, "(list (opt true) (opt false))"), "((some 5) none)");
}

/// `macroexpand-1`'s `Option<Sexpr>` was built as a box by the interpreter
/// even though the type niches (`tests/macro_tools_test.rs` noted the
/// mismatch); `Heap::alloc_option` builds every Rust-side `Option` the way
/// its type says, so it is the empty word now — and still prints as `none`.
#[test]
fn macroexpand_1_result_uses_the_niche() {
    let mut s = Session::new(Load::Prelude);
    let chk = std::rc::Rc::new(std::cell::RefCell::new(typelisp::Checker::new()));
    // `macroexpand` needs the live checker wired in, as the driver wires it.
    let mut interp = typelisp::Interp::new();
    typelisp::load_prelude(&mut s.heap, &mut chk.borrow_mut(), &mut interp);
    interp.set_checker(std::rc::Rc::clone(&chk));
    let r = typelisp::Reader::new();
    let vs = r.read_all(&mut s.heap, "(match (macroexpand-1 '(+ 1 2)) ((ok o) o) ((err _) (option::some 0)))").expect("read");
    let mut last = Value::Bool(false);
    for v in vs {
        let tl = chk.borrow_mut().check_form(&mut s.heap, &interp, v).expect("check");
        if let Some(val) = interp.exec(&mut s.heap, tl).expect("exec") {
            last = val;
        }
    }
    assert_eq!(last, Value::Empty, "the `none` of a niched Option<Sexpr> is the empty word");
}

/// A trace line renders a niched `Option` argument or result by the
/// function's declared representation — `(some 5)`, not `5`.
#[test]
fn a_traced_call_prints_a_niched_option_by_its_declared_type() {
    let out = eval_string(
        "(defvar (*cap* i32) (stream-string-output))\n\
         (setf *trace-output* (standard-stream::new *cap*))\n\
         (defun opt ((o Option<int>)) Option<int> (match o ((some n) (option::some (+ n 1))) ((none) (option::none))))\n\
         (trace opt)\n\
         (opt (option::some 5))\n\
         (opt (option::none))\n\
         (match (stream-take-output-string *cap*) ((ok s) s) ((err _) \"<capture failed>\"))",
    );
    assert!(out.contains("(opt (some 5))"), "{:?}", out);
    assert!(out.contains("opt returned (some 6)"), "{:?}", out);
    assert!(out.contains("(opt none)"), "{:?}", out);
    assert!(out.contains("opt returned none"), "{:?}", out);
}

/// A niched `Option` has no box to carry a type key, so it cannot be a
/// trait object — the same per-type refusal a primitive gets.
#[test]
fn a_niched_option_cannot_be_boxed_as_a_trait_object() {
    let msg = check_err(
        "(deftrait Speak () (say ((self Self)) string))\n\
         (impl Speak Option<int> (say ((self Self)) string \"opt\"))\n\
         (defun hear ((s :dyn Speak)) string (say s))\n\
         (hear (the Option<int> (option::some 1)))",
    );
    assert!(msg.contains("niche-represented Option"), "{}", msg);
}

/// A `Sexpr` holds a niched `Option` boxed (`box-option`), so downcasting
/// one back out reads that box through its constructors — and a downcast
/// that binds the whole value would bind the box under a type that says
/// "niche", so it is refused.
#[test]
fn a_downcast_out_of_a_sexpr_reads_the_box_through_a_constructor() {
    const DEFS: &str = r#"
        (defun opt ((b bool)) Option<int> (if b (option::some 5) (option::none)))
        (defun back ((s Sexpr)) int
          (match s ((the Option<int> (some n)) n) ((the Option<int> (none)) -1) (_ -2)))
    "#;
    assert_eq!(both(DEFS, &["opt", "back"], "(list (back (unwrap (list (opt true)))) (back (unwrap (list (opt false)))) (back 7))"), "(-2 -2 -2)");
    assert_eq!(
        both(DEFS, &["opt", "back"], "(list (back (unwrap (sexpr-car (list (opt true))))) (back (unwrap (sexpr-car (list (opt false))))))"),
        "(5 -1)"
    );
    let msg = check_err("(defun f ((s Sexpr)) int (match s ((the Option<int> o) 1) (_ 0)))");
    assert!(msg.contains("must name a constructor"), "{}", msg);
}

/// `eq`/`equal` see the payload: two `some`s of the same fixnum are the same
/// word, and `none` is never a `some`.
#[test]
fn equality_on_a_niched_option_compares_the_payload() {
    assert_eq!(eval_ok("(equal (the Option<int> (option::some 1)) (the Option<int> (option::some 1)))"), Value::Bool(true));
    assert_eq!(eval_ok("(equal (the Option<int> (option::some 1)) (the Option<int> (option::none)))"), Value::Bool(false));
    assert_eq!(eval_ok("(equal (the Option<string> (option::some \"a\")) (the Option<string> (option::some \"a\")))"), Value::Bool(true));
}
