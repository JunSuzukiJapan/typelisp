//! The core macro layer: the control-flow macros every other layer is written
//! *in*, plus the one `defun` (`sexpr-append`) that `,@` desugars through.
//!
//! Split out of [`crate::prelude`] so it is not the prelude's property. The
//! compiler island (`typelisp::compiler`'s `SOURCE`) is the reason: it is a
//! separate body of typelisp that must not carry prelude *functions* into the
//! code it emits, and for a long time that fence was read as "no prelude names
//! at all", which cost it `cond`/`case` and left it maintaining island-local
//! `icond`/`icase` copies. Macros leave no residue — they are gone by the time
//! anything is emitted — so the fence never needed to exclude them. This layer
//! is where the ones both bodies need live, defined once.
//!
//! **The discipline this layer keeps, and why it survives its own obsolescence:**
//! every definition here uses only builtins (`check::registry`) and native
//! special forms. Nothing here may reference the prelude, because this layer
//! loads before it. That is the whole invariant; the expander restrictions the
//! island's `icond` documented (no closure, no `,@`, no self-recursion) turned
//! out not to be real — an island probe using the prelude's own `cond`/`case`
//! bootstrapped cleanly, since the island's `SOURCE` is checked with the
//! prelude already in scope. `,@` is used freely below for that reason;
//! `sexpr-append`, the function it desugars to, simply moved here with them.

use crate::{Checker, Heap, Interp, Reader};

pub const SOURCE: &str = r##"
;; `sexpr-append`: `Sexpr` list concatenation, for `,@` (unquote-splicing —
;; `Checker::check_qq_template` desugars each splice to a `(sexpr-append
;; spliced rest)` call). First in this file because every macro below that
;; splices needs it already registered.
(defun sexpr-append ((a Option<Sexpr>) (b Option<Sexpr>)) Option<Sexpr>
  (if (sexpr-consp a)
      (sexpr-cons (sexpr-car a) (sexpr-append (sexpr-cdr a) b))
      b))

;; `and`/`or`: a self-recursive macro — each expansion peels off one argument
;; and re-emits a (shorter) `(and ...)`/`(or ...)` call, which the checker
;; re-expands the same way (`Checker::check_list`'s macro arm recursively
;; re-`check`s its own expansion); termination follows from `cdr`/`null`
;; shrinking `args` by one each step. `(quote true)`/`(quote false)` rather
;; than bare `true`/`false`: a macro's `Sexpr`-typed result can only come from
;; a quasiquote template or `(quote ..)` (both go through `value_to_quoted`) —
;; `Checker::check`'s literal arms do *not* coerce to `Sexpr` the way bare `()`
;; does, so a raw `false` returned from a macro body would fail to check.
;; Macro bodies navigate their `Sexpr` argument through the `sexpr-*` layer,
;; not the user-facing `car`/`cdr`/`cons`.
(defmacro and (&rest args)
  (if (sexpr-null args)
      (quote true)
      (if (sexpr-null (sexpr-cdr args))
          (sexpr-car args)
          (list (quote if) (sexpr-car args) (sexpr-cons (quote and) (sexpr-cdr args)) (quote false)))))
(defmacro or (&rest args)
  (if (sexpr-null args)
      (quote false)
      (if (sexpr-null (sexpr-cdr args))
          (sexpr-car args)
          (list (quote if) (sexpr-car args) (quote true) (sexpr-cons (quote or) (sexpr-cdr args))))))

;; `when`/`unless`: `if` with one side missing. The taken side's trailing `()`
;; is not decoration — `if` is a fixed 3-element form here, so both sides must
;; agree in type, and the *untaken* side is bare `()`. Both resolve to `Unit`
;; (not `Sexpr::Nil`) via `Checker::check`'s `Value::Empty` special case, the
;; same two-faced `()` the rest of the language relies on (see
;; `docs/language-design.md`). Without it, `(when c 5)` would try to unify
;; `i32` against the else branch's `Unit` and fail to check.
(defmacro when (test &rest body) `(if ,test (progn ,@body ()) ()))
(defmacro unless (test &rest body) `(if ,test () (progn ,@body ())))

;; `cond`: nested `if`s, one clause peeled off per recursive expansion (same
;; self-recursion shape as `and`/`or` above). `else`-detection mirrors `case`'s.
;; A clause that is not a list is refused by name: taking it apart would fail
;; with a message about `sexpr-car` that says nothing of which form is wrong.
(defmacro cond (&rest clauses)
  (if (sexpr-null clauses)
      ()
      (let ((clause (sexpr-car clauses)))
        (if (sexpr-consp clause) () (panic "cond: each clause is a list, (TEST FORM...)"))
        (if (eq (sexpr-car clause) (quote else))
            `(progn ,@(sexpr-cdr clause))
            `(if ,(sexpr-car clause) (progn ,@(sexpr-cdr clause)) (cond ,@(sexpr-cdr clauses)))))))

;; `case-key-atom-test`: one atomic key designator to the `bool` form testing
;; it. A bare symbol stands for itself (quoted); anything else is a literal
;; compared as written. `equal`, not CL's `eql`: a `string` key has to match by
;; content, which is the departure `docs/dev/cl-equivalence-catalog.md` records
;; under eq/eql/equal/equalp. Defined before its caller because definitions are
;; checked in source order.
(module %internal (pub defun case-key-atom-test ((key-form Symbol) (key Option<Sexpr>)) Option<Sexpr>
  (if (sexpr-symp key)
      (list (quote equal) key-form (list (quote quote) key))
      (list (quote equal) key-form key))))

;; `case-key-test`: one clause's key designator to the `bool` form that tests
;; it against `key-form` -- the `gensym`'d temporary `case` binds the scrutinee
;; to, so it is evaluated once no matter how many keys there are.
;;
;; **Keys are literals, not expressions** -- CL's rule, and as of 2026-08-26
;; this language's. A key is one of:
;;
;;   - a self-evaluating literal (`1`, `"one"`, `#\a`, `true`, `1.5`) -- used
;;     as written;
;;   - a bare symbol (`a`) -- the *symbol* `a`, i.e. exactly what `'a` used to
;;     have to be written as. Nothing about the generated comparison changed:
;;     it is still `(equal tmp (quote a))`, so a `Sexpr` scrutinee behaves as
;;     before. What changed is that the quote is now the macro's job;
;;   - a *list* of the above (`(1 2 3)`) -- a key list, matching if any element
;;     does, which is what could not work while keys were expressions (the list
;;     read as a call, and `(case n ((1 2 3) ...))` failed with `value is not
;;     callable: I32`).
;;
;; `(quote a)` in key position is refused rather than read CL's way. In CL it
;; is silently the two-key list `{quote, a}` -- a famous footgun, and one this
;; language would hit harder because `'a` was the *documented* spelling until
;; this change. Refusing names the fix; reading it CL's way would leave an arm
;; that type-checks and never matches, which is exactly what the `match` value
;; patterns refuse to do, for the same reason.
;;
;; Iterates with `loop`/`break`/`setf` rather than `while`: `while` and `not`
;; are prelude definitions, and this layer loads before the prelude.
(module %internal (pub defun case-key-test ((key-form Symbol) (key Option<Sexpr>)) Option<Sexpr>
  (if (sexpr-consp key)
      (if (if (sexpr-symp (sexpr-car key)) (equal (sexpr-sym-name (sexpr-car key)) "quote") false)
          (panic "case: a quoted key is not a key list -- write the bare symbol, not 'sym")
          (let ((acc (quote ())) (rest key))
            (loop
              (if (sexpr-null rest) (break) ())
              (setf acc (sexpr-append acc (list (%internal::case-key-atom-test key-form (sexpr-car rest)))))
              (setf rest (sexpr-cdr rest)))
            (sexpr-cons (quote or) acc)))
      (%internal::case-key-atom-test key-form key))))

;; `case`: bind the scrutinee once, then a `cond` over the clause tests.
;;
;; The temporary's name is fixed rather than `gensym`'d, and still hygienic:
;; it starts with a space, which no source can write, so it cannot collide
;; with a user binding — the same guarantee `gensym` itself relies on, minus
;; the counter. A fixed name suffices because the binding is established
;; immediately around the single `cond` that reads it, so a `case` nested in
;; another `case`'s clause shadows it correctly. `gensym` is not available
;; here anyway: it is a prelude `defun` (it owns CL's `*gensym-counter*` and
;; formats the name through `format`), and this layer loads before the prelude.
(defmacro case (expr &rest clauses)
  (let ((tmp (string->symbol " case-key")) (out (quote ())) (rest clauses))
    (loop
      (if (sexpr-null rest) (break) ())
      (let ((c (sexpr-car rest)))
        (if (sexpr-consp c) () (panic "case: each clause is a list, (KEYS FORM...)"))
        (setf out (sexpr-append out (list (if (eq (sexpr-car c) (quote else))
                                              c
                                              (sexpr-cons (%internal::case-key-test tmp (sexpr-car c)) (sexpr-cdr c)))))))
      (setf rest (sexpr-cdr rest)))
    `(let ((,tmp ,expr)) (cond ,@out))))

;; `ecase`/`ccase`: `case` with no fallthrough. CL's `ccase` differs only in
;; offering restarts, and there are none to offer here.
(pub defmacro ecase (expr &rest clauses)
  "`case` with no fallthrough: a value matching no clause panics."
  `(case ,expr ,@clauses (else (panic "ecase: no clause matched"))))
(pub defmacro ccase (expr &rest clauses)
  "CL's `ccase`. With no restarts to offer, identical to `ecase`."
  `(case ,expr ,@clauses (else (panic "ccase: no clause matched"))))
"##;

/// Reads, checks and executes [`SOURCE`] against the given environment.
///
/// Must run before both [`crate::prelude::SOURCE`] and the island's, which is
/// why it is not folded into either: this layer is what they are written in.
/// [`SOURCE`] being fixed, any failure here is a build/bug condition rather
/// than a user error — hence the panics.
pub fn load(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) {
    load_with(heap, chk, interp, &mut |_, _| {});
}

/// [`load`] with the point a *generator* needs to look in: `on_checked` sees
/// each top-level form after checking and before `exec`.
///
/// The prelude's dump generator passes its own callback here, so this layer's
/// definitions land in that dump alongside the prelude's own — they are gone
/// from a from-dump start otherwise, and `case`/`cond` would not exist. That
/// is also why [`crate::dump::sources_digest`] exists: the prelude dump is
/// built from two sources now, and staleness has to be judged against both.
pub fn load_with(
    heap: &mut Heap,
    chk: &mut Checker,
    interp: &mut Interp,
    on_checked: &mut dyn FnMut(&mut Heap, crate::Value),
) {
    let r = Reader::new();
    let mut forms = r.forms_in(crate::LIBRARY_FILE, SOURCE);
    while let Some((v, _)) = forms.next_form(heap).expect("core macros: read failed") {
        let tl = chk.check_form(heap, &*interp, v).expect("core macros: check failed");
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        on_checked(heap, tl);
        interp.exec(heap, tl).expect("core macros: eval failed");
    }
}
