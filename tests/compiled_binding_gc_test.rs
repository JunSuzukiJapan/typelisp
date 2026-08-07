//! Compiled locals that hold heap values must survive collection.
//!
//! `compiler.rs`'s `bind-let-values` pushes a GC root for a binding only when
//! `ast_bridge::binding_kind` gave it `KIND_SEXPR` (or the cell-boxed
//! `kind >= 10`). That classifier is *coarser* than `struct_field_kind`, which
//! calls the same types kind `6` — a tagged heap pointer: `binding_kind` is not
//! even given the struct set, so a `defstruct`/`Vector`-typed binding, a
//! `:dyn T`, and a `Symbol` all reach it as `KIND_PLAIN`.
//!
//! Whether that asymmetry is exploitable is not settled. These tests are the
//! attempt to exploit it — they hold such a value across collections that
//! `gc_stress` forces on every `cons`, and read it back afterwards, so a
//! reclaimed box shows up as wrong data rather than as luck. They pass, which
//! is evidence but not proof: they cannot show that the compiled path is what
//! held the value.
//!
//! The classification itself is deliberately left alone for now. The kind
//! numbers are read by the committed island bitcode
//! (`src/compiler_island.bc`), where `bind-let-values` and `release-bindings`
//! pair a push with a pop off the same number, and that artifact is frozen
//! until the compiler is rebuilt. The single repr vocabulary that replaces
//! both classifiers is where this gets settled.

use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn stressed() -> (Heap, Checker, Interp) {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    h.set_gc_stress(true);
    (h, chk, interp)
}

fn eval_in(h: &mut Heap, chk: &mut Checker, interp: &mut Interp, src: &str) -> Result<Value, EvalError> {
    let r = Reader::new();
    let vs = r.read_all(h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(h, interp, v).expect("check failed");
        if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

/// A struct-typed local held across collections. Its field is a `string`, so a
/// recycled box or `StrId` reads back as visibly wrong text rather than as a
/// plausible integer.
#[test]
fn a_compiled_struct_local_survives_collection() {
    let (mut h, mut chk, mut interp) = stressed();
    let src = r#"(defstruct named (tag string) (n i32))
                 (defun probe ((n i32)) string
                   (let ((p (named::new "keepme" n)))
                     (let ((a (sexpr-cons (quote x) (quote ()))))
                       (let ((b (sexpr-cons (quote y) a)))
                         (let ((c (sexpr-cons (quote z) b)))
                           p::tag)))))
                 (compile probe)
                 (probe 7)"#;
    match eval_in(&mut h, &mut chk, &mut interp, src).expect("compile+run failed under gc stress") {
        Value::Str(id) => assert_eq!(h.string(id), "keepme"),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// The same for a `Vector<T>`, which is a boxed struct too and reaches
/// `binding_kind` the same way — but whose contents grow, so a reclaimed box
/// would also lose elements rather than only its identity.
#[test]
fn a_compiled_vector_local_survives_collection() {
    let (mut h, mut chk, mut interp) = stressed();
    let src = r#"(defun probe ((n i32)) i32
                   (let ((v (the Vector<i32> (Vector::new))))
                     (push v n)
                     (push v 20)
                     (let ((a (sexpr-cons (quote x) (quote ()))))
                       (let ((b (sexpr-cons (quote y) a)))
                         (+ (get v 0) (get v 1))))))
                 (compile probe)
                 (probe 22)"#;
    let got = eval_in(&mut h, &mut chk, &mut interp, src).expect("compile+run failed under gc stress");
    assert_eq!(got, Value::Int(42));
}
