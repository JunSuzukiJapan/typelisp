//! Tests for `quote`/quasiquote/`gensym`/`defmacro` (CL-style macros).

extern crate typelisp;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

/// Read, type-check, and evaluate a program; return the last expression's
/// value alongside the heap (so `Sexpr` results can be inspected).
fn run(src: &str) -> Result<(Value, Heap), EvalError> {
    run_with_capacity(src, 1 << 16)
}

fn run_with_capacity(src: &str, capacity: usize) -> Result<(Value, Heap), EvalError> {
    let mut h = Heap::with_capacity(capacity);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok((last, h))
}

/// A `(value, heap)` pair's value as a `string`. A `string` is a heap
/// `Value::Str` since the scalar unification, so reading one needs the heap
/// the runner already hands back.
fn assert_string((v, h): (Value, Heap), expected: &str) {
    match v {
        Value::Str(id) => assert_eq!(h.string(id), expected),
        other => panic!("expected a string, got {:?}", other),
    }
}

fn eval_ok(src: &str) -> (Value, Heap) {
    run(src).expect("eval failed")
}

/// Like `run`, but with the prelude loaded first — `,@` (unquote-splicing)
/// desugars to a call to the prelude's `append` (`Checker::check_qq_template`),
/// so any test exercising it needs this instead of plain `run`.
fn run_with_prelude(src: &str) -> Result<(Value, Heap), EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok((last, h))
}

fn eval_ok_with_prelude(src: &str) -> (Value, Heap) {
    run_with_prelude(src).expect("eval failed")
}

/// Runs `src` with the prelude loaded and **a collection before every
/// allocation** (`gc_stress`), returning the heap alongside the value so a
/// `Sexpr` result can be read out of it.
///
/// There is no `capacity` parameter any more, and that absence is the point.
/// These tests used to check against a roomy heap and then execute
/// against a 96-cell one so the churn forced repeated collections; that is not
/// expressible any more. Since the checker lowers code into cons cells, the
/// checked program *is* cells in the heap it was checked against, and handing it
/// to a second heap reads those cells through the wrong symbol/string tables
/// (`sym_names` is empty there). A 96-cell heap could not hold the prelude, let
/// alone the program. Stressing one heap is both the honest shape and strictly
/// stronger pressure — a collection before *every* allocation, not only before
/// the ones a small arena happens to block on. See `eval_test.rs`'s
/// `eval_under_gc_pressure`, which this mirrors.
///
/// Stress goes on *after* the prelude loads: it is large, and collecting through
/// it would dominate the runtime without testing anything these cases are about.
fn run_with_prelude_under_gc_stress(src: &str) -> Result<(Value, Heap), EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    h.set_gc_stress(true);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok((last, h))
}

/// Render a heap-backed `Sexpr` value in reader syntax, for easy assertions.
fn sexpr_to_string(heap: &Heap, v: Value) -> String {
    match v {
        Value::Empty => "()".to_string(),
        Value::Int(i) => i.to_string(),
        Value::Boxed(id) => heap.f64_value(id).to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => format!("#\\{}", c),
        Value::Symbol(id) => heap.symbol_name(id).to_string(),
        Value::Str(id) => format!("{:?}", heap.string(id)),
        Value::Path(_) => "<path>".to_string(),
        Value::Cons(_) => {
            let mut parts = Vec::new();
            let mut cur = v;
            loop {
                match cur {
                    Value::Cons(_) => {
                        parts.push(sexpr_to_string(heap, heap.car(cur).unwrap()));
                        cur = heap.cdr(cur).unwrap();
                    }
                    Value::Empty => return format!("({})", parts.join(" ")),
                    other => return format!("({} . {})", parts.join(" "), sexpr_to_string(heap, other)),
                }
            }
        }
    }
}

fn as_sexpr_string(v: Value, h: &Heap) -> String {
    sexpr_to_string(h, v)
}

// ---- Phase A: quote ---------------------------------------------------------

#[test]
fn quote_atom() {
    let (v, h) = eval_ok("(quote 42)");
    assert_eq!(as_sexpr_string(v, &h), "42");
}

#[test]
fn quote_list() {
    let (v, h) = eval_ok("(quote (a b c))");
    assert_eq!(as_sexpr_string(v, &h), "(a b c)");
}

#[test]
fn quote_nested_list() {
    let (v, h) = eval_ok("(quote (a (b c) d))");
    assert_eq!(as_sexpr_string(v, &h), "(a (b c) d)");
}

#[test]
fn quote_reader_shorthand() {
    let (v, h) = eval_ok("'(1 \"hi\" true)");
    assert_eq!(as_sexpr_string(v, &h), "(1 \"hi\" true)");
}

#[test]
fn quote_empty_list_is_nil() {
    let (v, h) = eval_ok("(quote ())");
    assert_eq!(as_sexpr_string(v, &h), "()");
}

#[test]
fn quote_survives_gc_pressure() {
    // A tiny heap forces `Interp::alloc_quoted`'s internal `cons` calls to
    // trigger a real mark-sweep collection partway through building a nested
    // quoted literal, repeatedly across many iterations — this is the
    // regression test for the push_root/pop_root discipline guarding that
    // recursive build (each iteration's literal becomes garbage as soon as
    // the next iteration overwrites `last`, so the heap must actually reclaim
    // and reuse cells to keep up).
    let (v, h) = run_with_prelude_under_gc_stress(
        "(defun build () Option<Sexpr> (quote (a (b c) (d (e f)) g)))
         (let ((last (quote ())))
           (dotimes (i 500)
             (setf last (build)))
           last)",
    )
    .expect("eval failed");
    assert_eq!(as_sexpr_string(v, &h), "(a (b c) (d (e f)) g)");
}

// ---- Phase B: quasiquote / unquote -------------------------------------------

#[test]
fn quasiquote_no_unquote_behaves_like_quote() {
    let (v, h) = eval_ok("`(a b c)");
    assert_eq!(as_sexpr_string(v, &h), "(a b c)");
}

#[test]
fn quasiquote_unquote_references_local() {
    // `,x` requires `x` to be `Sexpr`-typed (no implicit coercion from e.g.
    // `i32`) — exactly the shape a `defmacro` parameter has, which is the
    // primary use case for unquote.
    let (v, h) = eval_ok("(let ((x (quote (+ 1 2)))) `(a ,x c))");
    assert_eq!(as_sexpr_string(v, &h), "(a (+ 1 2) c)");
}

#[test]
fn quasiquote_unquote_at_head() {
    let (v, h) = eval_ok("(let ((x (quote hello))) `(,x b))");
    assert_eq!(as_sexpr_string(v, &h), "(hello b)");
}

#[test]
fn quasiquote_dotted_unquote_tail() {
    // `(a . ,b)` — the unquote falls in the dotted-tail position, not inside
    // a sub-list; exercises that `check_qq_template` walks car/cdr generically
    // regardless of how the cons structure was originally written.
    let (v, h) = eval_ok("(let ((b (quote (x y)))) `(a . ,b))");
    assert_eq!(as_sexpr_string(v, &h), "(a x y)");
}

// ---- Phase C: gensym ----------------------------------------------------------
//
// `gensym` is a prelude `defun` over the prelude global `*gensym-counter*`
// (CL has the same variable), so these need `eval_ok_with_prelude`.

#[test]
fn gensym_returns_a_symbol() {
    let (v, h) = eval_ok_with_prelude("(gensym)");
    match v {
        Value::Symbol(_) => {}
        other => panic!("expected a Symbol, got {:?}", other),
    }
    let _ = h;
}

#[test]
fn gensym_is_fresh_each_call() {
    let (v, h) = eval_ok_with_prelude("(let ((a (gensym)) (b (gensym))) (list a b))");
    let a = h.car(v).unwrap();
    let b = h.car(h.cdr(v).unwrap()).unwrap();
    assert_ne!(a, b, "two `gensym` calls produced the same symbol");
}

#[test]
fn quasiquote_nested_list_with_unquote() {
    let (v, h) = eval_ok("(let ((x (quote 1)) (y (quote 2))) `(a (,x ,y) b))");
    assert_eq!(as_sexpr_string(v, &h), "(a (1 2) b)");
}

// ---- Phase D: defmacro ---------------------------------------------------------

#[test]
fn basic_conditional_macro() {
    let (v, _h) = eval_ok(
        "(defmacro my-unless (test then else)
           `(if ,test ,else ,then))
         (my-unless (< 2 1) 99 100)",
    );
    // test = (< 2 1) = false -> `then` (99) runs.
    assert_eq!(v, Value::Int(99));
}

#[test]
fn macro_used_in_same_batch_as_its_definition() {
    // Exercises that a macro's body is registered and callable for a *later*
    // form in the very same read/check/eval batch (the eager-exec-on-check
    // path in `main.rs`'s REPL loop; this test harness's `run` already
    // interleaves check+exec per form, which is the same requirement).
    let (v, _h) = eval_ok(
        "(defmacro double (x) `(+ ,x ,x))
         (defmacro quadruple (x) `(double (double ,x)))
         (quadruple 3)",
    );
    assert_eq!(v, Value::Int(12));
}

#[test]
fn macro_arity_mismatch_is_a_type_error() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, "(defmacro double (x) `(+ ,x ,x)) (double 1 2)")
        .expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last_err = None;
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                let _ = interp.exec(&mut h, tl);
            }
            Err(e) => {
                last_err = Some(e);
                break;
            }
        }
    }
    assert!(last_err.is_some(), "expected a macro-arity type error");
}

#[test]
fn macro_is_unhygienic_and_capture_is_observable() {
    // A naive `swap!`-style macro that introduces a literal `tmp` binding:
    // calling it with `tmp` itself as an argument captures the macro's
    // internal binding, silently breaking the swap. This is the defining
    // (and documented) trade-off of CL-style non-hygienic macros.
    let (v, _h) = eval_ok(
        "(defmacro my-swap (a b)
           `(let ((tmp ,a))
              (setf ,a ,b)
              (setf ,b tmp)))
         (let ((tmp 1) (x 2))
           (my-swap tmp x)
           tmp)",
    );
    // A correct swap would leave `tmp` as 2; capture leaves it unchanged.
    assert_eq!(v, Value::Int(1));
}

#[test]
fn gensym_fixes_macro_hygiene() {
    let (v, _h) = eval_ok_with_prelude(
        "(defmacro my-swap-fixed (a b)
           (let ((g (gensym)))
             `(let ((,g ,a))
                (setf ,a ,b)
                (setf ,b ,g))))
         (let ((tmp 1) (x 2))
           (my-swap-fixed tmp x)
           tmp)",
    );
    // With a gensym'd temp name instead of a literal `tmp`, the swap is
    // correct: `tmp` (1) and `x` (2) actually exchange.
    assert_eq!(v, Value::Int(2));
}

#[test]
fn macro_expansion_survives_gc_pressure() {
    // A tiny heap forces a real GC mid-expansion (the macro call itself
    // allocates via quasiquote's `Expr::Construct{Cons,..}` nodes, and the
    // expansion result must stay rooted across that) — the regression test
    // for `Interp::expand_macro`'s push_root/pop_root discipline.
    let (v, h) = run_with_prelude_under_gc_stress(
        "(defmacro listify (a b c) `(list ,a ,b ,c))
         (defun build () Option<Sexpr> (listify (quote x) (quote y) (quote z)))
         (let ((last (quote ())))
           (dotimes (i 500)
             (setf last (build)))
           last)",
    )
    .expect("eval failed");
    assert_eq!(as_sexpr_string(v, &h), "(x y z)");
}

// ---- Phase E: defmacro &rest -------------------------------------------------

#[test]
fn rest_only_macro_collects_args_into_a_list() {
    // No fixed params at all: every call argument is collected into `body`.
    // A `defmacro` body navigates its `Sexpr` arguments with the `sexpr-*`
    // layer (`sexpr-cons` here) — the free `cons` is the generic `cons<T,U>`
    // pair now, not a `Sexpr` cons cell (Symbol/Sexpr redesign Phase 4b).
    let (v, _h) = eval_ok(
        "(defmacro my-progn (&rest body) (sexpr-cons (quote progn) body))
         (my-progn 1 2 3)",
    );
    assert_eq!(v, Value::Int(3));
}

#[test]
fn rest_args_bind_to_a_proper_list() {
    // A fixed param (`a`) plus `&rest`: quoting `rest` back shows exactly
    // what it binds — a proper `Sexpr` list of the unevaluated trailing call
    // forms, with `a`'s own argument excluded. (The expansion must itself be
    // a valid form, so `rest`'s *value* — not `rest` used bare, which would
    // try to run `(2 3)` as code — is wrapped in `quote`.)
    let (v, h) = eval_ok(
        "(defmacro capture (a &rest rest) `(quote ,rest))
         (capture 1 2 3)",
    );
    assert_eq!(as_sexpr_string(v, &h), "(2 3)");
}

#[test]
fn rest_macro_can_be_called_with_no_extra_args() {
    let (v, h) = eval_ok(
        "(defmacro capture (a &rest rest) `(quote ,rest))
         (capture 1)",
    );
    assert_eq!(as_sexpr_string(v, &h), "()");
}

#[test]
fn rest_macro_too_few_fixed_args_is_a_type_error() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, "(defmacro capture (a &rest rest) rest) (capture)")
        .expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last_err = None;
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                let _ = interp.exec(&mut h, tl);
            }
            Err(e) => {
                last_err = Some(e);
                break;
            }
        }
    }
    assert!(last_err.is_some(), "expected a macro-arity type error");
}

#[test]
fn rest_arg_list_construction_survives_gc_pressure() {
    // The `&rest` binding builds a fresh cons chain via `Heap::cons` inside
    // `Interp::bind_macro_args` — a small heap forces a real GC mid-build,
    // exercising that loop's push_root/pop_root discipline around the
    // growing `list` accumulator (analogous to `macro_expansion_survives_gc_pressure`
    // above, but targeting `&rest` collection specifically). The 5 trailing
    // args are bare symbols (not `(quote x)` forms) so the captured list is
    // a flat 5-cell spine, matching `listify`'s per-iteration cost above.
    let (v, h) = run_with_prelude_under_gc_stress(
        "(defmacro capture (a &rest rest) `(quote ,rest))
         (defun build () Option<Sexpr> (capture 0 a b c d e))
         (let ((last (quote ())))
           (dotimes (i 500)
             (setf last (build)))
           last)",
    )
    .expect("eval failed");
    assert_eq!(as_sexpr_string(v, &h), "(a b c d e)");
}

#[test]
fn rest_must_be_the_last_parameter() {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let v = r.read(&mut h, "(defmacro bad (&rest xs a) xs)").expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    assert!(chk.check_form(&mut h, &interp, v).is_err());
}

// ---- unquote-splicing (`,@`, roadmap step 8a) -------------------------------

#[test]
fn splice_in_the_middle_of_a_template() {
    let (v, h) = eval_ok_with_prelude("(let ((xs (quote (2 3)))) `(1 ,@xs 4))");
    assert_eq!(as_sexpr_string(v, &h), "(1 2 3 4)");
}

#[test]
fn splice_at_the_end_of_a_template() {
    let (v, h) = eval_ok_with_prelude("(let ((xs (quote (2 3)))) `(1 ,@xs))");
    assert_eq!(as_sexpr_string(v, &h), "(1 2 3)");
}

#[test]
fn splicing_an_empty_list_contributes_nothing() {
    let (v, h) = eval_ok_with_prelude("(let ((xs (quote ()))) `(1 ,@xs 2))");
    assert_eq!(as_sexpr_string(v, &h), "(1 2)");
}

#[test]
fn splice_without_a_surrounding_quote_requires_the_prelude() {
    // The whole point of `,@`: a macro's `&rest body` (one `Sexpr` list of
    // forms) splices into a template as multiple sibling forms, not as one
    // nested list — `my-progn`'s expansion is a flat `(progn 1 2 3)`, which
    // evaluates to the *last* form's value, not a single 3-element list.
    let (v, _) = eval_ok_with_prelude(
        "(defmacro my-progn (&rest body) `(progn ,@body)) (my-progn 1 2 3)",
    );
    assert_eq!(v, Value::Int(3));
}

#[test]
fn splicing_the_same_rest_param_twice_runs_its_forms_twice() {
    let (v, _) = eval_ok_with_prelude(
        "(defmacro twice (&rest body) `(progn ,@body ,@body))
         (let ((x 0)) (twice (setf x (+ x 1))) x)",
    );
    assert_eq!(v, Value::Int(2));
}

#[test]
fn unquote_splicing_without_the_prelude_loaded_is_a_clean_type_error() {
    // `,@` desugars to a call to the prelude's `append` — without the
    // prelude loaded, that name doesn't resolve, and `check_qq_template`
    // reports this directly rather than panicking or producing a confusing
    // "no such function" at eval time.
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let v = r.read(&mut h, "(let ((xs (quote (1)))) `(,@xs))").expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    assert!(chk.check_form(&mut h, &interp, v).is_err());
}

// ---- loop/branch primitive reduction (src/prelude.rs) -----------------------
//
// `while`/`dotimes`/`dolist`/`when`/`unless`/`cond`/`and`/`or`/`if-let` are
// `defmacro`s over `loop`/`if`/`match` now, rather than checker-native
// special forms — see `src/prelude.rs`'s "loop/branch primitive reduction"
// comment. Most of their *behavioral* correctness is already exercised by
// `tests/eval_test.rs` (which predates this change and was retargeted to
// load the prelude); the tests below focus on what's specific to *this*
// file: `if-let` (not exercised at the eval level anywhere else) and
// `cond`'s self-recursive expansion (the first prelude macro whose own
// expansion mentions itself by name — see that macro's comment).

#[test]
fn if_let_binds_in_then_branch_and_falls_through_to_else() {
    // `default` (rather than a bare literal in the `else` branch) sidesteps
    // integer literals' `i32` default conflicting with `x`'s `u16` in the
    // `then` branch — `-1` here is a normal *call argument*, checked
    // against `f`'s own declared `u16` parameter type, so it adopts `u16`
    // directly with no such conflict.
    let (v, _) = eval_ok_with_prelude(
        "(defun f ((o Option<i32>) (default i32)) i32 (if-let ((Some x) o) (+ x 1) default))
         (f (option::some 41) -1)",
    );
    assert_eq!(v, Value::Int(42));
    let (v2, _) = eval_ok_with_prelude(
        "(defun f ((o Option<i32>) (default i32)) i32 (if-let ((Some x) o) (+ x 1) default))
         (f (option::none) -1)",
    );
    assert_eq!(v2, Value::Int(-1));
}

#[test]
fn cond_with_several_clauses_selects_the_first_true_one() {
    // 4 non-`else` clauses, so `cond`'s self-recursive expansion
    // (`Checker::check_list`'s macro arm re-expanding its own output,
    // peeling off one clause per recursion) must unwind correctly more
    // than once, not just the 1-clause base case.
    let src = "(defun band ((n i32)) string
                 (cond ((< n 0) \"neg\")
                       ((= n 0) \"zero\")
                       ((< n 10) \"small\")
                       ((< n 100) \"medium\")
                       (else \"large\")))";
    assert_string(eval_ok_with_prelude(&format!("{src} (band -1)")), "neg");
    assert_string(eval_ok_with_prelude(&format!("{src} (band 0)")), "zero");
    assert_string(eval_ok_with_prelude(&format!("{src} (band 5)")), "small");
    assert_string(eval_ok_with_prelude(&format!("{src} (band 50)")), "medium");
    assert_string(eval_ok_with_prelude(&format!("{src} (band 500)")), "large");
}

#[test]
fn cond_with_no_matching_clause_and_no_else_is_unit() {
    let (v, _) = eval_ok_with_prelude("(cond ((= 1 2) ()))");
    assert_eq!(v, Value::Empty);
}

// ---- Phase F: defmacro &optional / &key (TODO T2) ---------------------------

/// Read → check → exec a program, returning the first check/eval error's
/// message (or `Ok(())` if it all succeeded) — for asserting on the exact
/// arity/keyword diagnostics a malformed macro call produces.
fn run_expect_err(src: &str) -> Result<(), String> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                if let Err(e) = interp.exec(&mut h, tl) {
                    return Err(e.to_string());
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

#[test]
fn optional_arg_supplied_and_omitted() {
    // `by` defaults to `1` when omitted, else takes the call argument.
    let (v, h) = eval_ok("(defmacro pair (x &optional (by 1)) `(quote (,x ,by))) (pair 10)");
    assert_eq!(as_sexpr_string(v, &h), "(10 1)");
    let (v, h) = eval_ok("(defmacro pair (x &optional (by 1)) `(quote (,x ,by))) (pair 10 5)");
    assert_eq!(as_sexpr_string(v, &h), "(10 5)");
}

#[test]
fn optional_without_default_binds_nil() {
    // A bare `&optional y` (no default form) binds the empty list when omitted.
    let (v, h) = eval_ok("(defmacro pair (x &optional y) `(quote (,x ,y))) (pair 1)");
    assert_eq!(as_sexpr_string(v, &h), "(1 ())");
    let (v, h) = eval_ok("(defmacro pair (x &optional y) `(quote (,x ,y))) (pair 1 2)");
    assert_eq!(as_sexpr_string(v, &h), "(1 2)");
}

#[test]
fn optional_default_may_reference_earlier_param() {
    // CL: an `&optional` default form is evaluated with the earlier params
    // bound, so `(y x)` copies `x` when `y` is omitted.
    let (v, h) = eval_ok("(defmacro dup (x &optional (y x)) `(quote (,x ,y))) (dup 4)");
    assert_eq!(as_sexpr_string(v, &h), "(4 4)");
    let (v, h) = eval_ok("(defmacro dup (x &optional (y x)) `(quote (,x ,y))) (dup 4 5)");
    assert_eq!(as_sexpr_string(v, &h), "(4 5)");
}

#[test]
fn keyword_args_match_by_name_in_any_order() {
    let prog = "(defmacro make (&key (a 0) (b 9)) `(quote (,a ,b)))";
    assert_eq!(as_sexpr_string_of(&format!("{prog} (make)")), "(0 9)");
    assert_eq!(as_sexpr_string_of(&format!("{prog} (make :a 7)")), "(7 9)");
    assert_eq!(as_sexpr_string_of(&format!("{prog} (make :b 3 :a 7)")), "(7 3)");
}

/// Convenience: eval a program and render its final `Sexpr` result.
fn as_sexpr_string_of(src: &str) -> String {
    let (v, h) = eval_ok(src);
    as_sexpr_string(v, &h)
}

#[test]
fn required_optional_and_rest_combine() {
    let prog = "(defmacro combo (a &optional (b 2) &rest cs) `(quote (,a ,b ,cs)))";
    assert_eq!(as_sexpr_string_of(&format!("{prog} (combo 1)")), "(1 2 ())");
    assert_eq!(as_sexpr_string_of(&format!("{prog} (combo 1 20)")), "(1 20 ())");
    assert_eq!(as_sexpr_string_of(&format!("{prog} (combo 1 20 30 40)")), "(1 20 (30 40))");
}

#[test]
fn too_few_args_reports_required_minimum() {
    let e = run_expect_err("(defmacro m (a &optional b) `(quote ,a)) (m)").unwrap_err();
    assert!(e.contains("at least 1"), "got: {}", e);
}

#[test]
fn too_many_args_reports_the_optional_range() {
    let e = run_expect_err("(defmacro m (a &optional b) `(quote ,a)) (m 1 2 3)").unwrap_err();
    assert!(e.contains("1 to 2"), "got: {}", e);
}

#[test]
fn unknown_keyword_is_rejected() {
    let e = run_expect_err("(defmacro m (&key (a 0)) `(quote ,a)) (m :zzz 1)").unwrap_err();
    assert!(e.contains("unknown &key argument") && e.contains(":zzz"), "got: {}", e);
}

#[test]
fn odd_keyword_plist_is_rejected() {
    let e = run_expect_err("(defmacro m (&key (a 0)) `(quote ,a)) (m :a)").unwrap_err();
    assert!(e.contains("odd number of &key arguments"), "got: {}", e);
}

#[test]
fn markers_must_appear_in_order() {
    // `&optional` after `&rest` is a malformed lambda list.
    assert!(run_expect_err("(defmacro bad (a &rest r &optional b) `(quote ,a)) (bad 1)").is_err());
    // `&key` twice.
    assert!(run_expect_err("(defmacro bad (&key a &key b) `(quote ,a)) (bad)").is_err());
}

#[test]
fn optional_default_construction_survives_gc_pressure() {
    // Each expansion evaluates the `&optional` default (a fresh `list` cons)
    // under a tight heap, forcing a GC mid-bind that must not reclaim the
    // partially-built binding environment (`Interp::bind_macro_args`' Heap
    // cells) — the `&optional`/`&key` analogue of
    // `rest_arg_list_construction_survives_gc_pressure`.
    let (v, h) = run_with_prelude_under_gc_stress(
        "(defmacro deflt (&optional (xs (quote (a b c)))) `(quote ,xs))
         (defun build () Option<Sexpr> (deflt))
         (let ((last (quote ())))
           (dotimes (i 500)
             (setf last (build)))
           last)",
    )
    .expect("eval failed");
    assert_eq!(as_sexpr_string(v, &h), "(a b c)");
}
