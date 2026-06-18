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
///
/// The rest is a `Sexpr`-list library (roadmap step 7b — cl-equivalence
/// catalog §2.2 e plus language-design.md §4.2's list section). Every
/// function operates on plain `Sexpr` — no generics are needed here (unlike
/// `Vector<T>`/`HashTable<K,V>`) since a typelisp "list" already *is*
/// `Sexpr`, a single concrete type. Item-based predicates (`member`/
/// `remove`/`count`/`position`) compare with `eq` (matching CL's default
/// `eql` test); their `-if` counterparts take a `(fn (Sexpr) bool)` instead.
/// Helper functions a later definition depends on are placed earlier in this
/// string — `defun` (like `defmacro`) has no forward-reference support, only
/// self-recursion (see `Checker::check_defun`'s doc comment).
///
/// `nconc`/`nreverse` are destructive (mutate existing cons cells via
/// `set-car`/`set-cdr` instead of allocating new ones — see those functions'
/// own comment below). Deferred to a later step (7c): `sort`, `gcd`/`lcm`/
/// `signum`, `assoc`, Option/Result helpers, and higher-order helpers
/// (`identity`/`compose`/`flip` would need generic `defun` type parameters,
/// which don't exist yet).
pub const SOURCE: &str = r#"
(defun consp ((s Sexpr)) bool (match s ((Cons _ _) true) (_ false)))
(defun null ((s Sexpr)) bool (match s ((Nil) true) (_ false)))
(defun atom ((s Sexpr)) bool (not (consp s)))
(defun equal ((a Sexpr) (b Sexpr)) bool
  (match a
    ((Cons a1 a2) (match b ((Cons b1 b2) (and (equal a1 b1) (equal a2 b2))) (_ false)))
    ((Str s1) (match b ((Str s2) (eq s1 s2)) (_ false)))
    (_ (eq a b))))

(defun length ((lst Sexpr)) i32
  (match lst
    ((Nil) 0)
    ((Cons _ d) (+ 1 (length d)))
    (_ (panic "length: not a proper list"))))

(defun append ((a Sexpr) (b Sexpr)) Sexpr
  (match a
    ((Nil) b)
    ((Cons h t) (cons h (append t b)))
    (_ (panic "append: not a proper list"))))

(defun reverse-onto ((lst Sexpr) (acc Sexpr)) Sexpr
  (match lst
    ((Nil) acc)
    ((Cons h t) (reverse-onto t (cons h acc)))
    (_ (panic "reverse: not a proper list"))))
(defun reverse ((lst Sexpr)) Sexpr (reverse-onto lst ()))

(defun nthcdr ((n i32) (lst Sexpr)) Sexpr
  (if (<= n 0) lst
    (match lst
      ((Nil) ())
      ((Cons _ d) (nthcdr (- n 1) d))
      (_ (panic "nthcdr: not a proper list")))))

(defun nth ((n i32) (lst Sexpr)) Sexpr
  (match (nthcdr n lst)
    ((Cons h _) h)
    ((Nil) ())
    (_ (panic "nth: not a proper list"))))

(defun elt ((lst Sexpr) (n i32)) Sexpr (nth n lst))

(defun last ((lst Sexpr)) Sexpr
  (match lst
    ((Cons _ (Nil)) lst)
    ((Cons _ d) (last d))
    ((Nil) lst)
    (_ (panic "last: not a proper list"))))

(defun butlast ((lst Sexpr)) Sexpr
  (match lst
    ((Nil) ())
    ((Cons _ (Nil)) ())
    ((Cons h t) (cons h (butlast t)))
    (_ (panic "butlast: not a proper list"))))

(defun take ((n i32) (lst Sexpr)) Sexpr
  (if (<= n 0) ()
    (match lst
      ((Nil) ())
      ((Cons h t) (cons h (take (- n 1) t)))
      (_ (panic "take: not a proper list")))))

(defun subseq ((lst Sexpr) (start i32) (end i32)) Sexpr
  (take (- end start) (nthcdr start lst)))

(defun copy-list ((lst Sexpr)) Sexpr
  (match lst
    ((Nil) ())
    ((Cons h t) (cons h (copy-list t)))
    (_ (panic "copy-list: not a proper list"))))

(defun member ((item Sexpr) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) ())
    ((Cons h t) (if (eq h item) lst (member item t)))
    (_ (panic "member: not a proper list"))))

(defun find-if ((pred (fn (Sexpr) bool)) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) ())
    ((Cons h t) (if (pred h) h (find-if pred t)))
    (_ (panic "find-if: not a proper list"))))

(defun every ((pred (fn (Sexpr) bool)) (lst Sexpr)) bool
  (match lst
    ((Nil) true)
    ((Cons h t) (and (pred h) (every pred t)))
    (_ (panic "every: not a proper list"))))

;; Named `some?`, not CL's `some` — that name collides with `Option`'s
;; `Some` constructor (symbols are case-folded, so `Some`/`some` are the
;; same identifier; `resolve_ctor` is tried before a free function of the
;; same name, so `(some ...)` would always try to build an `Option` value).
(defun some? ((pred (fn (Sexpr) bool)) (lst Sexpr)) bool
  (match lst
    ((Nil) false)
    ((Cons h t) (or (pred h) (some? pred t)))
    (_ (panic "some?: not a proper list"))))

(defun count-if ((pred (fn (Sexpr) bool)) (lst Sexpr)) i32
  (match lst
    ((Nil) 0)
    ((Cons h t) (if (pred h) (+ 1 (count-if pred t)) (count-if pred t)))
    (_ (panic "count-if: not a proper list"))))

(defun count ((item Sexpr) (lst Sexpr)) i32
  (count-if (lambda ((x Sexpr)) bool (eq x item)) lst))

(defun position-if-from ((i i32) (pred (fn (Sexpr) bool)) (lst Sexpr)) Option<i32>
  (match lst
    ((Nil) (None))
    ((Cons h t) (if (pred h) (Some i) (position-if-from (+ i 1) pred t)))
    (_ (panic "position-if: not a proper list"))))
(defun position-if ((pred (fn (Sexpr) bool)) (lst Sexpr)) Option<i32>
  (position-if-from 0 pred lst))

(defun position ((item Sexpr) (lst Sexpr)) Option<i32>
  (position-if (lambda ((x Sexpr)) bool (eq x item)) lst))

(defun remove-if ((pred (fn (Sexpr) bool)) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) ())
    ((Cons h t) (if (pred h) (remove-if pred t) (cons h (remove-if pred t))))
    (_ (panic "remove-if: not a proper list"))))

(defun remove-if-not ((pred (fn (Sexpr) bool)) (lst Sexpr)) Sexpr
  (remove-if (lambda ((x Sexpr)) bool (not (pred x))) lst))

(defun remove ((item Sexpr) (lst Sexpr)) Sexpr
  (remove-if (lambda ((x Sexpr)) bool (eq x item)) lst))

(defun map ((f (fn (Sexpr) Sexpr)) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) ())
    ((Cons h t) (cons (f h) (map f t)))
    (_ (panic "map: not a proper list"))))

(defun filter ((pred (fn (Sexpr) bool)) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) ())
    ((Cons h t) (if (pred h) (cons h (filter pred t)) (filter pred t)))
    (_ (panic "filter: not a proper list"))))

(defun foldl ((f (fn (Sexpr Sexpr) Sexpr)) (init Sexpr) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) init)
    ((Cons h t) (foldl f (f init h) t))
    (_ (panic "foldl: not a proper list"))))

(defun foldr ((f (fn (Sexpr Sexpr) Sexpr)) (init Sexpr) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) init)
    ((Cons h t) (f h (foldr f init t)))
    (_ (panic "foldr: not a proper list"))))

;; Destructive (mutating) list operations, built on `set-car`/`set-cdr`
;; (`crate::eval::interp`'s `eval_builtin`, wrapping `mem::Heap::set_car`/
;; `set_cdr` — CL's `rplaca`/`rplacd`). Unlike every function above, these
;; reuse existing cons cells instead of allocating new ones, so any other
;; reference to the same cells observes the mutation too (aliasing) —
;; exactly CL's `nconc`/`nreverse` contract.
(defun nconc ((a Sexpr) (b Sexpr)) Sexpr
  (match a
    ((Nil) b)
    ((Cons _ _) (progn (set-cdr (last a) b) a))
    (_ (panic "nconc: not a proper list"))))

(defun nreverse-onto ((lst Sexpr) (prev Sexpr)) Sexpr
  (match lst
    ((Nil) prev)
    ((Cons _ d) (progn (set-cdr lst prev) (nreverse-onto d lst)))
    (_ (panic "nreverse: not a proper list"))))
(defun nreverse ((lst Sexpr)) Sexpr (nreverse-onto lst ()))
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
