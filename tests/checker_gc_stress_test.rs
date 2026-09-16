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
/// per cell (`check::forms::list_from_vec_locs`).
#[test]
fn variadic_arithmetic_folds_survive_collection() {
    assert_stress_agrees("(defun f () int (+ 1 2 3 4 5 6 7 8))");
    assert_stress_agrees("(defun f () int (* (+ 1 2 3) (- 10 4 3)))");
}

#[test]
fn setf_and_incf_desugaring_survives_collection() {
    assert_stress_agrees(
        "(defstruct p (x int))
         (defun f ((q p)) int (setf q::x 5) (incf q::x 2) (decf q::x) q::x)",
    );
}

#[test]
fn let_star_desugaring_survives_collection() {
    assert_stress_agrees("(defun f () int (let* ((a 1) (b (+ a 1)) (c (+ b 1))) c))");
}

/// `impl` substitutes `Self` by rewriting the read syntax tree
/// (`Checker::subst_value`), rebuilding every list it passes through.
#[test]
fn impl_self_substitution_survives_collection() {
    assert_stress_agrees(
        "(deftrait Counted ()
           (count ((self Self)) int))
         (defstruct box (n int))
         (impl Counted box
           (count ((self Self)) int self::n))",
    );
}

#[test]
fn quasiquote_expansion_survives_collection() {
    assert_stress_agrees(
        "(defmacro m (a b) `(+ ,a (* ,b 2)))
         (defun f () int (m 1 2))",
    );
}

/// The `,@` cases need `sexpr-append`, which is the prelude's — so they get
/// their own pair of runs with it loaded first.
///
/// The prelude itself is checked *unstressed*: it takes minutes per load
/// otherwise (a collection per `cons`, over ~120k cells of definitions), and
/// what is under test is the checking of `src`, not of the prelude. Stress
/// goes on immediately afterwards, which is enough — a node the checker leaks
/// while lowering `src` is collected by `src`'s own next allocation.
fn check_all_with_prelude(src: &str, stress: bool) -> Result<Vec<String>, Error> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_prelude(&mut h, &mut chk, &mut interp);
    h.set_gc_stress(stress);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut out = Vec::new();
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v)?;
        out.push(core::print(&h, tl));
        interp.exec(&mut h, tl).map_err(|e| Error::TypeError(format!("exec: {}", e)))?;
    }
    Ok(out)
}

fn assert_stress_agrees_with_prelude(src: &str) {
    let plain =
        check_all_with_prelude(src, false).unwrap_or_else(|e| panic!("plain check failed: {}\nsrc: {}", e, src));
    let stressed =
        check_all_with_prelude(src, true).unwrap_or_else(|e| panic!("stressed check failed: {}\nsrc: {}", e, src));
    assert_eq!(stressed, plain, "GC changed the checked result\nsrc: {}", src);
}

/// `,@` lowers to a `(call sexpr-append ...)` node, and *that* node was the
/// one place in the quasiquote walk handing back an unrooted form.
///
/// The splice has to sit somewhere with template left after it: the leak is
/// only observable if something allocates before the node reaches its parent,
/// and what allocates is the enclosing template node checking its own `cdr`
/// and building its `construct`. A `,@` in the final position returns straight
/// into a caller that does nothing more, which is why the prelude — full of
/// `` `(progn ,@body) `` — still tripped over it only in some macros.
#[test]
fn splicing_in_the_middle_of_a_template_survives_collection() {
    assert_stress_agrees_with_prelude(
        "(defmacro m (&rest body) `(progn ,@body 7))
         (defun f () int (m 1 2))",
    );
    // Two splices, so the first one's node also has to survive the second's
    // whole subtree being checked.
    assert_stress_agrees_with_prelude(
        "(defmacro m2 (&rest body) `(progn ,@body ,@body 7))
         (defun g () int (m2 1 2))",
    );
}

/// A whole small program, to catch anything the targeted cases miss.
#[test]
fn a_mixed_program_survives_collection() {
    assert_stress_agrees(
        "(defstruct pt (x int) (y int))
         (defun norm1 ((p pt)) int
           (let* ((a p::x) (b p::y))
             (+ (if (< a 0) (- 0 a) a) (if (< b 0) (- 0 b) b))))
         (defun f () int
           (let ((p (pt::new 3 -4)))
             (setf p::x 10)
             (incf p::y 1)
             (norm1 p)))",
    );
}
