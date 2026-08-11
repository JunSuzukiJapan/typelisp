//! Checks the checker's own allocations against a collector that fires at
//! every single `cons` (`Heap::set_gc_stress`).
//!
//! The checker rebuilds read syntax in several places — arithmetic folds,
//! `setf`/`incf`, `let*`, `impl`'s `Self` substitution, quasiquote — and every
//! rebuild allocates. An intermediate that is not rooted across the *next*
//! allocation is collectible, but with a 64K-cell arena the free list is rarely
//! empty, so the window is tiny and the symptom (a form silently rebuilt out of
//! unrelated recycled cells) appears far from the cause. Under stress mode the
//! window is every allocation, so a missing root fails deterministically here
//! instead of once in a hundred runs in production.
//!
//! Each test asserts the stressed result *equals the unstressed one*: the
//! collector must not be observable in what the checker produces.

extern crate typelisp;
use typelisp::check::core;
use typelisp::{Checker, Error, Heap, Interp, Reader};

/// Every checked form, **printed**.
///
/// Printed rather than returned as `Vec<TopLevelForm>`: a checked form is now
/// cons cells, so a `TopLevelForm` is a heap index. Two runs against two heaps
/// produce different indices for structurally identical trees, which would make
/// the comparison below fail for a reason that has nothing to do with the
/// collector. The old `Vec<TopLevel>` was heap-independent Rust data with a
/// derived `PartialEq`, so comparing it directly was the structural comparison;
/// for a cons IR, `core::print` is (`read(print(f)) == f` holds — see its doc
/// comment).
fn check_all(src: &str, stress: bool) -> Result<Vec<String>, Error> {
    let mut h = Heap::with_capacity(1 << 14);
    h.set_gc_stress(stress);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut out = Vec::new();
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v)?;
        out.push(core::print(&h, tl));
        // Execute as we go so a `defmacro` is registered before a later form
        // calls it — macro expansion is part of what we are stressing.
        interp.exec(&mut h, tl).map_err(|e| Error::TypeError(format!("exec: {}", e)))?;
    }
    Ok(out)
}

fn assert_stress_agrees(src: &str) {
    let plain = check_all(src, false).unwrap_or_else(|e| panic!("plain check failed: {}\nsrc: {}", e, src));
    let stressed = check_all(src, true).unwrap_or_else(|e| panic!("stressed check failed: {}\nsrc: {}", e, src));
    assert_eq!(stressed, plain, "GC changed the checked result\nsrc: {}", src);
}

/// A variadic `+`/`*` folds into a chain of rebuilt list forms, one `cons`
/// per cell (`Checker`'s `list_from_vec_locs`).
#[test]
fn variadic_arithmetic_folds_survive_collection() {
    assert_stress_agrees("(defun f () i32 (+ 1 2 3 4 5 6 7 8))");
    assert_stress_agrees("(defun f () i32 (* (+ 1 2 3) (- 10 4 3)))");
}

#[test]
fn setf_and_incf_desugaring_survives_collection() {
    assert_stress_agrees(
        "(defstruct p (x i32))
         (defun f ((q p)) i32 (setf q::x 5) (incf q::x 2) (decf q::x) q::x)",
    );
}

#[test]
fn let_star_desugaring_survives_collection() {
    assert_stress_agrees("(defun f () i32 (let* ((a 1) (b (+ a 1)) (c (+ b 1))) c))");
}

/// `impl` substitutes `Self` by rewriting the read syntax tree
/// (`Checker::subst_value`), rebuilding every list it passes through.
#[test]
fn impl_self_substitution_survives_collection() {
    assert_stress_agrees(
        "(deftrait Counted ()
           (count ((self Self)) i32))
         (defstruct box (n i32))
         (impl Counted box
           (count ((self Self)) i32 self::n))",
    );
}

#[test]
fn quasiquote_expansion_survives_collection() {
    assert_stress_agrees(
        "(defmacro m (a b) `(+ ,a (* ,b 2)))
         (defun f () i32 (m 1 2))",
    );
}

/// A whole small program, to catch anything the targeted cases miss.
#[test]
fn a_mixed_program_survives_collection() {
    assert_stress_agrees(
        "(defstruct pt (x i32) (y i32))
         (defun norm1 ((p pt)) i32
           (let* ((a p::x) (b p::y))
             (+ (if (< a 0) (- 0 a) a) (if (< b 0) (- 0 b) b))))
         (defun f () i32
           (let ((p (pt::new 3 -4)))
             (setf p::x 10)
             (incf p::y 1)
             (norm1 p)))",
    );
}
