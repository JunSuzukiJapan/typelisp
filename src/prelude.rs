//! The typelisp prelude: library functions written in typelisp itself
//! rather than Rust, following [language-design.md](../docs/language-design.md)
//! §4's split — anything expressible purely as a combination of existing
//! primitives belongs here, not in the checker/interpreter.
//!
//! There is no automatic "every program gets this for free" loading yet:
//! callers (the REPL, or a test harness) must explicitly call [`load`] once,
//! against the same [`Heap`]/[`Checker`]/[`Interp`] the rest of the program
//! will use, before processing any user source. The prelude source is fixed
//! and known-good, so a failure here is a bug in this file, not a user
//! error — [`load`] panics rather than threading a `Result` callers would
//! have no real recovery from.

use crate::{Checker, Heap, Interp, Reader};

/// `consp`/`null`/`atom` only need `match` on `Sexpr`'s `Cons`/`Nil`
/// constructors (already checker-level features); `equal` is the
/// recursive-structural counterpart to `eq` (registry::sexpr_assoc) —
/// `Cons` cells compare by walking both sides, and `Str` compares by
/// content (via the primitive `string` type's own `eq`, since two
/// separately-built `Sexpr::Str`s are never `eq`-identical even with the
/// same text); every other variant (`Nil`/`Int`/`Float`/`Char`/`Bool`/`Sym`)
/// is already correctly handled by `eq` itself.
pub const SOURCE: &str = r#"
(defun consp ((s Sexpr)) bool (match s ((Cons _ _) true) (_ false)))
(defun null ((s Sexpr)) bool (match s ((Nil) true) (_ false)))
(defun atom ((s Sexpr)) bool (not (consp s)))
(defun equal ((a Sexpr) (b Sexpr)) bool
  (match a
    ((Cons a1 a2) (match b ((Cons b1 b2) (and (equal a1 b1) (equal a2 b2))) (_ false)))
    ((Str s1) (match b ((Str s2) (eq s1 s2)) (_ false)))
    (_ (eq a b))))
"#;

/// Read, check, and execute [`SOURCE`] against `heap`/`chk`/`interp`,
/// registering its definitions exactly as if the caller had typed them
/// first. Must be called before any user source that references a prelude
/// name.
pub fn load(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) {
    let r = Reader::new();
    let forms = r.read_all(heap, SOURCE).expect("prelude: read failed");
    for v in forms {
        let tl = chk.check_form(heap, &*interp, v).expect("prelude: check failed");
        interp.exec(heap, tl).expect("prelude: eval failed");
    }
}
