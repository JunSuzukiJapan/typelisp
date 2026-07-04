//! The typelisp prelude: library functions written in typelisp itself
//! rather than Rust, following [language-design.md](../docs/dev/language-design.md)
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
/// `HashTable<K,V>`) since a typelisp "list" already *is*
/// `Sexpr`, a single concrete type — every Lisp's lists are built from cons
/// cells, never a separate homogeneous array type. Item-based predicates (`member`/
/// `remove`/`count`/`position`) compare with `eq` (matching CL's default
/// `eql` test); their `-if` counterparts take a `(fn (Sexpr) bool)` instead.
/// Helper functions a later definition depends on are placed earlier in this
/// string — `defun` (like `defmacro`) has no forward-reference support, only
/// self-recursion (see `Checker::check_defun`'s doc comment).
///
/// `nconc`/`nreverse` are destructive (mutate existing cons cells via
/// `set-car`/`set-cdr` instead of allocating new ones — see those functions'
/// own comment below).
///
/// `Option<T>`/`Result<T,E>` accessors (`unwrap`/`unwrap-or`/`is-some`/
/// `is-none`/`is-ok`/`is-err`) are `defmethod`, not `defun` — see the
/// comment above their definitions below for why. `identity`/`const`/
/// `compose`/`flip` are generic `defun`s — see the comment above their
/// definitions for why those, unlike the accessors, fit `defun` rather than
/// `defmethod`. `gcd`/`lcm`/`signum`/`abs` (catalog §2.2f), `sort` (§2.2c,
/// the `Sexpr`-list free function), and `assoc` (§4.2's list section) round
/// out step 7c — `min`/`max`/`range` are not in the catalog, so they're left
/// undone rather than guessed at.
///
/// `until`/`while-let` (roadmap step 8b, catalog §1.1) use `,@`
/// (unquote-splicing, step 8a, by now already exercised by the loop/branch
/// primitive reduction set further up) — see their own comments below.
/// `case` (step 8c) additionally builds its expansion dynamically (mapping
/// over `&rest clauses`) rather than from one fixed template — see its own
/// comment for the key-quoting design decision this forces. `do` (step 8d)
/// completes the roadmap's step-8 macro set, parallel-stepping multiple
/// bindings via `gensym`-fresh temporaries (see its own comment).
pub const SOURCE: &str = r#"
;; `not`: moved here from a Rust builtin (it has no dependency on the GC
;; heap or anything else Rust-only — a plain `if`/`bool` round trip) so it
;; compiles through the ordinary `ast_bridge`/`compiler.rs` pipeline like any
;; other `defun`, instead of needing a "compiled code calling a Rust
;; builtin" mechanism — see `compiler.rs`'s module doc comment's `loop`/
;; `break`/`return`/`setf` stage note (`while`'s own expansion calls `not`
;; unconditionally, so this is what makes `while`/`dotimes` compilable at
;; all). `and`/`or` (below) still need special-form treatment for short-
;; circuiting; `not` never did.
(defun not ((b bool)) bool (if b false true))
(defun consp ((s Sexpr)) bool (match s ((Cons _ _) true) (_ false)))
(defun null ((s Sexpr)) bool (match s ((Nil) true) (_ false)))
(defun atom ((s Sexpr)) bool (not (consp s)))

;; `symbol->string`/`string->symbol` (`docs/language-design.md` §4.1's
;; conversion catalog): a `Sexpr::Sym`'s name is already stored as a plain
;; `string` (`registry::sexpr_def`'s `sym` variant field, `Type::Str`), so
;; `symbol->string` is just the `Sym` destructuring `match` arm every other
;; `Sexpr` accessor here uses. `string->symbol` is the bare `Sym` constructor
;; — interning happens underneath it the same way `gensym` interns a fresh
;; name (`Interp::eval_builtin`'s `"gensym"` arm), so no Rust builtin is
;; needed for either direction.
(defun symbol->string ((s Sexpr)) string
  (match s ((Sym name) name) (_ (panic "symbol->string: not a symbol"))))
(defun string->symbol ((s string)) Sexpr (Sym s))

;; `unreachable`/`todo` (cl-equivalence-catalog.md §1.2): placeholders for
;; "this branch can't be reached" / "not implemented yet", each a
;; zero-argument macro expanding to a fixed `panic` call — no new special
;; form needed in Rust, unlike `the` (a real checker extension, since
;; annotating a type can't be done by macro expansion alone).
(defmacro unreachable () `(panic "unreachable"))
(defmacro todo () `(panic "todo"))

;; Loop/branch primitive reduction (LLVMコンパイラ作業に先立つ整理): `loop`/
;; `break`/`return` (looping) and `if`/`match` (branching) are the only forms
;; the checker/interpreter/compiler need to understand natively going
;; forward — `while`/`dotimes`/`dolist`/`when`/`unless`/`and`/`or`/`cond`/
;; `if-let` used to be hand-rolled `Expr`-producing Rust functions in
;; `Checker::check_list` (`check_while`/`check_dotimes`/`check_dolist`/
;; `check_when`/`check_and_or`/`check_cond`/`check_if_let`), each duplicating
;; logic the *macro* system can express directly, the same way `until`/
;; `while-let`/`case`/`do` already do further down. Moving them to `defmacro`
;; means the checker only ever sees `if`/`match`/`loop`/`break`/`return` as
;; the irreducible control-flow primitives — fewer `Expr` variants for both
;; the tree-walking interpreter and (especially) the LLVM compiler
;; (`src/compile/`) to special-case. Most of the set (`while`/`dotimes`/
;; `dolist`/`when`/`unless`/`cond`/`if-let`) lives further down, right after
;; `append` — `,@` (unquote-splicing) always desugars through a call to
;; `append` (`Checker::check_qq_template`), even when nothing follows the
;; splice in the same list, so any macro using `,@` needs `append` already
;; registered. `and`/`or` are the exception, kept here: `equal` (just below)
;; already calls `and`, so they must be registered before it — and unlike
;; the others, their recursive-descent shape never needs to splice a `&rest`
;; list into a *partially-built* template, only to `cons` one fixed element
;; onto the front of it, which needs no `append` at all.

;; `and`/`or`: a self-recursive macro — each expansion peels off one
;; argument and re-emits a (shorter) `(and ...)`/`(or ...)` call, which the
;; checker re-expands the same way it already re-expands `case`'s `cond`
;; output (`Checker::check_list`'s macro arm recursively re-`check`s its own
;; expansion) — this is the first macro in the prelude whose expansion
;; mentions *itself* by name rather than a different macro, but the
;; mechanism is identical; termination follows from `cdr`/`null` shrinking
;; `args` by one each step. `(quote true)`/`(quote false)` rather than bare
;; `true`/`false`: a macro's `Sexpr`-typed result can only come from a
;; quasiquote template or `(quote ..)` (both go through `value_to_quoted`)
;; — `Checker::check`'s own literal arms (`Value::Bool` etc.) do *not*
;; coerce to `Sexpr` the way bare `()` does (`Value::Empty`'s special-cased
;; `none`/`nil` lookup), so a raw `false` returned directly from a macro
;; body would fail to check against the macro body's `Sexpr`-expected type.
(defmacro and (&rest args)
  (if (null args)
      (quote true)
      (if (null (cdr args))
          (car args)
          (list (quote if) (car args) (cons (quote and) (cdr args)) (quote false)))))
(defmacro or (&rest args)
  (if (null args)
      (quote false)
      (if (null (cdr args))
          (car args)
          (list (quote if) (car args) (quote true) (cons (quote or) (cdr args))))))

;; CL's `equal`: `eql` on everything but `Cons`/`Str`, which get structural
;; recursion and case-sensitive content comparison respectively (`Str::equal`,
;; not `Str::eq` — two separately-built `Str`s with equal content are never
;; `eq`, see `docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp section).
;; The catch-all is `eql`, not `eq`: they coincide for every immediate
;; variant (`Int`/`Char`/`Bool`/`Sym`), but a boxed `Sexpr::Float`
;; (`Value::Boxed`, see `BoxedObj`) is only `eql` — two separately-built
;; equal floats are correctly never `eq` (identity, like `Str`/`Cons`), the
;; same reason `eq` alone would be wrong here now.
(defun equal ((a Sexpr) (b Sexpr)) bool
  (match a
    ((Cons a1 a2) (match b ((Cons b1 b2) (and (equal a1 b1) (equal a2 b2))) (_ false)))
    ((Str s1) (match b ((Str s2) (equal s1 s2)) (_ false)))
    (_ (eql a b))))

;; CL's `equalp`: like `equal`, but `Str`/`Char` fields compare
;; case-insensitively (`Str::equalp`/`Char::equalp`), and numbers compare by
;; value across the `Int`/`Float` type boundary (e.g. `(Int 1)` vs `(Float
;; 1.0)` is true) via `int->float` (`registry::int_assoc`).
(defun equalp ((a Sexpr) (b Sexpr)) bool
  (match a
    ((Cons a1 a2) (match b ((Cons b1 b2) (and (equalp a1 b1) (equalp a2 b2))) (_ false)))
    ((Str s1) (match b ((Str s2) (equalp s1 s2)) (_ false)))
    ((Char c1) (match b ((Char c2) (equalp c1 c2)) (_ false)))
    ((Int a1) (match b ((Float b1) (= (int->float a1) b1)) (_ (eql a b))))
    ((Float a1) (match b ((Int b1) (= a1 (int->float b1))) (_ (eql a b))))
    (_ (eql a b))))

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

;; The rest of the loop/branch primitive reduction set (see the comment by
;; `and`/`or` above) — placed here, right after `append`, since every one of
;; these uses `,@` (unquote-splicing) somewhere in its expansion, and `,@`
;; always desugars through a call to `append` (`Checker::check_qq_template`).

;; `while`: the *first* layer of sugar over `loop`. `if` always takes
;; exactly 3 arguments (`Checker::check_if`, no CL-style implicit-`Unit`
;; 2-arg form), so the early-exit check needs an explicit `()` else branch —
;; `(break)` carries no value, so `(if (not test) (break) ())` is always
;; `Unit`-typed, matching `while`'s own `Unit` result —
;; `Checker::check_loop`'s `Never`-seeded `join_types` widens to `Unit`
;; automatically once this `break` is found, with no special-casing needed
;; (unlike the old `check_while`, which seeded the loop-stack frame with
;; `Unit` directly).
(defmacro while (test &rest body) `(loop (if (not ,test) (break) ()) ,@body))

;; `dotimes`: `gensym` replaces the old `check_dotimes`'s "leading-space,
;; unwritable-in-source" hidden binding trick with the macro system's own
;; (already-proven, see `case`'s `tmp`) hygiene mechanism.
(defmacro dotimes (spec &rest body)
  (let ((var (car spec)) (count-expr (car (cdr spec))) (limit (gensym)))
    `(let ((,var 0) (,limit ,count-expr))
       (while (< ,var ,limit) ,@body (setf ,var (+ ,var 1))))))

;; `dolist`: same hidden-binding replacement as `dotimes`, but stepping a
;; `Sexpr` list with `consp`/`car`/`cdr` instead of comparing an `i32`
;; counter — what `check_dolist` did with `Pattern::Ctor` `match` arms
;; directly, this does with already-existing library functions instead.
(defmacro dolist (spec &rest body)
  (let ((var (car spec)) (lst-expr (car (cdr spec))) (lst (gensym)))
    `(let ((,lst ,lst-expr))
       (while (consp ,lst)
         (let ((,var (car ,lst)))
           ,@body
           (setf ,lst (cdr ,lst)))))))

;; `when`/`unless`: single-armed `if`. The taken side ends in a trailing
;; `()` (after `body`, not instead of it) so its value is always `Unit`,
;; discarding whatever `body`'s own last form would otherwise evaluate to —
;; matching the old `check_when`'s explicit `body.push(unit_node())`. The
;; *untaken* side is bare `()` too; both resolve to `Unit` (not `Sexpr::Nil`)
;; via `Checker::check`'s `Value::Empty` special case, the same two-faced
;; `()` the rest of the language already relies on (see
;; `docs/language-design.md`) — without the taken side's trailing `()`,
;; `if`'s `then`/`els` types would only agree by coincidence (e.g. `(when c
;; 5)` would try to unify `i32` against `els`'s `Unit` and fail to check).
(defmacro when (test &rest body) `(if ,test (progn ,@body ()) ()))
(defmacro unless (test &rest body) `(if ,test () (progn ,@body ())))

;; `cond`: nested `if`s, one clause peeled off per recursive expansion (same
;; self-recursion shape as `and`/`or` above). `else`-detection mirrors
;; `case`'s own `(eq (car c) (quote else))`.
(defmacro cond (&rest clauses)
  (if (null clauses)
      ()
      (let ((clause (car clauses)))
        (if (eq (car clause) (quote else))
            `(progn ,@(cdr clause))
            `(if ,(car clause) (progn ,@(cdr clause)) (cond ,@(cdr clauses)))))))

;; `if-let`: exactly the two-armed `match` `Checker::check_if_let` used to
;; build directly (a constructor-pattern arm plus a wildcard `else` arm) —
;; the same shape `while-let` (further down) uses for its own loop
;; condition. Uses no `,@`, so it could have lived next to `and`/`or`
;; instead — kept here purely to group the whole reduction set together.
(defmacro if-let (binding then els)
  (let ((pattern (car binding)) (val (car (cdr binding))))
    `(match ,val (,pattern ,then) (_ ,els))))

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

;; Named `any`, not CL's `some` — that name collides with `Option`'s
;; `Some` constructor (symbols are case-folded, so `Some`/`some` are the
;; same identifier; `resolve_ctor` is tried before a free function of the
;; same name, so `(some ...)` would always try to build an `Option` value).
;; `any` (Rust's `Iterator::any`) sidesteps the collision and also avoids a
;; `?` suffix, which typelisp doesn't use for predicate names (see
;; language-design.md §7.3).
(defun any ((pred (fn (Sexpr) bool)) (lst Sexpr)) bool
  (match lst
    ((Nil) false)
    ((Cons h t) (or (pred h) (any pred t)))
    (_ (panic "any: not a proper list"))))

(defun count-if ((pred (fn (Sexpr) bool)) (lst Sexpr)) i32
  (match lst
    ((Nil) 0)
    ((Cons h t) (if (pred h) (+ 1 (count-if pred t)) (count-if pred t)))
    (_ (panic "count-if: not a proper list"))))

(defun count ((item Sexpr) (lst Sexpr)) i32
  (count-if (lambda ((x Sexpr)) bool (eq x item)) lst))

(defun position-if-from ((i i32) (pred (fn (Sexpr) bool)) (lst Sexpr)) Option<i32>
  (match lst
    ((Nil) (option::none))
    ((Cons h t) (if (pred h) (option::some i) (position-if-from (+ i 1) pred t)))
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

;; Option<T>/Result<T,E> accessors (roadmap step 7c). Written as `defmethod`,
;; not `defun`: `unwrap`/`unwrap-or` need the same name on both `Option<T>`
;; and `Result<T,E>`, which only an instance method's per-receiver-type assoc
;; table allows (a free function has exactly one signature for its name —
;; see registry::int_assoc's step-6 comment for the same constraint on `+`).
;; `(unwrap opt)` resolves via `Checker::check_instance_method`: it checks
;; `opt` first, then looks up `unwrap` in *its* type's assoc table, so the
;; same call syntax reaches `Option`'s or `Result`'s method independently.
;; The receiver's type parameter (`T`/`E` here) is resolved per call site
;; from the receiver's concrete type, exactly like `HashTable<K,V>`'s own
;; methods — no relation to `defun`'s separate generic-parameter syntax.
(defmethod unwrap ((self Option<T>)) T
  (match self ((some x) x) ((none) (panic "unwrap: called on none"))))
(defmethod unwrap-or ((self Option<T>) (default T)) T
  (match self ((some x) x) ((none) default)))
(defmethod is-some ((self Option<T>)) bool
  (match self ((some _) true) ((none) false)))
(defmethod is-none ((self Option<T>)) bool
  (not (is-some self)))

(defmethod unwrap ((self Result<T,E>)) T
  (match self ((ok x) x) ((err _) (panic "unwrap: called on err"))))
(defmethod unwrap-or ((self Result<T,E>) (default T)) T
  (match self ((ok x) x) ((err _) default)))
(defmethod is-ok ((self Result<T,E>)) bool
  (match self ((ok _) true) ((err _) false)))
(defmethod is-err ((self Result<T,E>)) bool
  (not (is-ok self)))

;; Higher-order helpers (roadmap step 7c) — the motivating use case for
;; `defun`'s generic type parameters (`Checker::check_call`'s `unify`/
;; `subst_apply` integration): each is plain `defun`, not `defmethod`, since
;; none of them is a method "on" a receiver type — they operate purely on
;; function values. `compose`/`flip` return a `lambda`, which closes over the
;; outer defun's parameters (`f`/`g`) the same way any nested lambda would.
(defun (identity T) ((x T)) T x)
(defun (const A B) ((x A) (y B)) A x)
(defun (compose A B C) ((f (fn (B) C)) (g (fn (A) B))) (fn (A) C)
  (lambda ((x A)) C (f (g x))))
(defun (flip A B C) ((f (fn (A B) C))) (fn (B A) C)
  (lambda ((y B) (x A)) C (f x y)))

;; Remaining numeric helpers (roadmap step 7c, catalog §2.2f): `gcd`/`lcm` via
;; Euclid's algorithm, `signum` via comparisons. Not generic — `<`/`mod` are
;; `i32` instance methods resolved at check time from a *concrete* receiver
;; type, so a generic `defun`'s unresolved type variable `T` can't reach them
;; (no trait-bound mechanism exists to require "T has `<`"); i32-only is the
;; same scope the catalog gives these.
(defun abs ((x i32)) i32 (if (< x 0) (- 0 x) x))
(defun gcd-pos ((a i32) (b i32)) i32 (if (= b 0) a (gcd-pos b (mod a b))))
(defun gcd ((a i32) (b i32)) i32 (gcd-pos (abs a) (abs b)))
(defun lcm ((a i32) (b i32)) i32
  (if (or (= a 0) (= b 0)) 0 (/ (abs (* a b)) (gcd a b))))
(defun signum ((x i32)) i32 (if (> x 0) 1 (if (< x 0) -1 0)))

;; `sort` (catalog §2.2c): insertion sort over `Sexpr` lists, taking a
;; comparator `(fn (Sexpr Sexpr) bool)` (CL's default `<`-style predicate).
;; Non-destructive (builds a new list), unlike `nconc`/`nreverse` above —
;; there's no existing-cons-cell structure to reuse for a sorted result.
(defun insert-sorted ((cmp (fn (Sexpr Sexpr) bool)) (item Sexpr) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) (cons item ()))
    ((Cons h t) (if (cmp item h) (cons item lst) (cons h (insert-sorted cmp item t))))
    (_ (panic "sort: not a proper list"))))
(defun sort ((cmp (fn (Sexpr Sexpr) bool)) (lst Sexpr)) Sexpr
  (match lst
    ((Nil) ())
    ((Cons h t) (insert-sorted cmp h (sort cmp t)))
    (_ (panic "sort: not a proper list"))))

;; `assoc` (catalog §4.2's list section): search an alist (a list of
;; `(key . value)` cons cells) for the first pair whose `car` is `eq` to
;; `key`. Returns the pair itself (CL semantics) or `()` if absent — `Sexpr`,
;; not `Option<Sexpr>`, matching `member`'s convention above (`Sexpr` already
;; has a nil-like absent value, so wrapping it adds nothing — `position`
;; wraps in `Option<i32>` only because `i32` has no such value).
(defun assoc ((key Sexpr) (alist Sexpr)) Sexpr
  (match alist
    ((Nil) ())
    ((Cons pair rest) (if (eq key (car pair)) pair (assoc key rest)))
    (_ (panic "assoc: not a proper list"))))

;; `until`/`while-let` (roadmap step 8b, cl-equivalence-catalog.md §1.1):
;; pure template expansions using `,@` (unquote-splicing, step 8a) —
;; `&rest body` is one `Sexpr` list of forms, and `,@body` splices them into
;; the expansion as multiple sibling forms rather than nesting that list as
;; a single (mistyped) form.
;;
;; `until` is `while` with the condition negated.
(defmacro until (test &rest body) `(while (not ,test) ,@body))
;;
;; `while-let` loops for as long as `binding`'s pattern matches `val`,
;; re-evaluating `val` each iteration (so it can be a call like `(next i)`
;; that observes mutated state) — same idea `if-let` uses
;; (`Checker::check_if_let`: a `match` whose non-matching arm is the "false"
;; branch), but built from `loop`/`match`/`break` rather than checker-level
;; machinery, and looping instead of running once. `binding` is `(pattern
;; val)`; `pattern` must be a constructor pattern (e.g. `(some x)`) to
;; usefully narrow `val`'s type — a bare variable would match unconditionally
;; (binding the whole scrutinee) and the macro would loop forever.
(defmacro while-let (binding &rest body)
  (let ((pattern (car binding))
        (val (car (cdr binding))))
    `(loop (match ,val (,pattern ,@body) (_ (break))))))

;; `doiter` (TODO.md, [[typelisp-trait-mechanism-and-doiter]]): iterate a
;; value of any type implementing the `Iter` trait (`Item`/`next`, declared
;; further down once `deftrait`/`impl` exist) by repeatedly calling `next`,
;; binding `var` to each yielded `Item` until `(none)`. Same hidden-binding
;; trick `dotimes`/`dolist` use (`gensym` evaluates `coll-expr` exactly
;; once into `tmp`, so the value being iterated isn't re-built every
;; iteration) layered on top of `while-let` — `(next ,tmp)` is `while-let`'s
;; re-evaluated-every-iteration `val`, which is exactly the "observe `next`'s
;; mutated state each time" behavior `Iter` needs. `var`'s type is never
;; named here: `Checker::check_ctor_pattern` infers a `(some var)` pattern's
;; binding type from the scrutinee's own type (`next`'s return, `Option<Item>`)
;; the same way it already does for every other `match`, so this needs no
;; checker-level special form — `(next tmp)` resolves through the ordinary
;; instance-method/trait-bound call machinery (`Checker::check_instance_method`),
;; concrete or `where`-bounded type variable alike.
(defmacro doiter (spec &rest body)
  (let ((var (car spec))
        (coll-expr (car (cdr spec)))
        (tmp (gensym)))
    `(let ((,tmp ,coll-expr))
       (while-let ((some ,var) (next ,tmp)) ,@body))))

;; `case` (roadmap step 8c, catalog §1.1): `(case expr (key1 body1...)
;; (key2 body2...) ... (else default...))` expands to `(cond ((eq tmp key1)
;; body1...) ((eq tmp key2) body2...) ... (else default...))`, where `tmp` is
;; a `gensym`-fresh binding for `expr`'s value — `expr` is evaluated exactly
;; once (matching CL's `case`), not once per clause, and `gensym` keeps the
;; binding from capturing/being captured by a same-named variable at the use
;; site (this macro's only hygiene concern, since every other name it
;; introduces — `cond`/`eq`/`let` — is a keyword or builtin, not a binding
;; the call site could collide with).
;;
;; Unlike CL, a clause's `key` is **not** implicitly quoted — `case` is a
;; `defmacro`, which only ever sees `clauses` as unevaluated `Sexpr` forms
;; with no static type (typelisp has no dynamic `eql`-across-any-type the
;; way CL does), so there's no way to know at expansion time whether `expr`
;; is e.g. `i32` (where `key` must stay a bare literal like `1`) or `Sexpr`
;; (where a symbol key needs an explicit `'sym` — written by the caller,
;; same as any other quoted symbol). This means `(case x (1 ...))` works
;; directly, but a symbol-keyed clause must be written `(case x ('a ...))`.
;; Each key is matched via `equal`, not CL's own `eql` — a deliberate,
;; documented departure (`docs/cl-equivalence-catalog.md`'s eq/eql/equal/
;; equalp section): ANSI CL's `case` uses `eql`, which never does structural
;; string comparison, so a string-literal key would (almost) never actually
;; match in real CL either. This language's `case` is built on `equal`
;; instead specifically so a `string` key still works by content — the same
;; design goal that originally motivated giving every scalar type its own
;; `eq` method (now corrected to real identity; `equal` is the one that
;; keeps `case` working uniformly across types, `string` included).
(defmacro case (expr &rest clauses)
  (let ((tmp (gensym)))
    `(let ((,tmp ,expr))
       (cond ,@(map (lambda ((c Sexpr)) Sexpr
                       (if (eq (car c) (quote else))
                           c
                           (cons (list (quote equal) tmp (car c)) (cdr c))))
                     clauses)))))

;; `do` (roadmap step 8d, catalog §1.1): `(do ((var1 init1 step1)
;; (var2 init2 step2) ...) (test result...) body...)`. Each binding's `step`
;; is required (no CL-style "omit step to leave the variable unstepped" —
;; not in the catalog, so left unimplemented rather than guessed at).
;;
;; Expands to `(let ((var1 init1) (var2 init2) ...) (while (not test) body...
;; <parallel step update>) result...)`. The step update must be *parallel*
;; (CL semantics: every step expression is evaluated against the *old*
;; values before any variable is updated — `(do ((a 0 b) (b 1 (+ a b))) ...)`
;; must see `b`'s old value when computing the new `a`, not the just-updated
;; one), so each step's value is first evaluated into a `gensym`-fresh
;; temporary, and only then assigned back with `setf` — a sequential
;; `(setf var1 step1) (setf var2 step2)` would let `step2` observe `var1`'s
;; *new* value, breaking parallelism.
;;
;; Building this means mapping over `bindings` three different ways (the
;; `(var init)` pairs for the `let`, the `(temp step)` pairs for computing
;; new values, and the `(var temp)` pairs for assigning them back) — `map`
;; only takes one list, so rather than three separate `gensym` calls per
;; binding needing to line up across three passes, `temps` is computed once
;; up front as `((temp1 var1 step1) (temp2 var2 step2) ...)` and each later
;; pass just projects the two fields it needs out of that same triple.
(defmacro do (bindings test-result &rest body)
  (let ((test (car test-result))
        (result (cdr test-result))
        (temps (map (lambda ((b Sexpr)) Sexpr (list (gensym) (car b) (car (cdr (cdr b)))))
                     bindings)))
    `(let ,(map (lambda ((b Sexpr)) Sexpr (list (car b) (car (cdr b)))) bindings)
       (while (not ,test)
         ,@body
         (let ,(map (lambda ((tr Sexpr)) Sexpr (list (car tr) (car (cdr (cdr tr))))) temps)
           ,@(map (lambda ((tr Sexpr)) Sexpr (list (quote setf) (car (cdr tr)) (car tr))) temps)))
       ,@result)))

;; `Iter`/`doiter` (TODO.md's `doiter` entry, [[typelisp-todo-md-staleness]]):
;; the trait `doiter` requires every iterable type to implement — a single
;; `next` method returning the next element, or `(none)` once exhausted.
;; `Self` mutates in place across calls (no immutable "next state" returned
;; alongside the element — see the trait machinery's design notes): a
;; `next` implementation is expected to update its own fields via `setf`,
;; the same mutation `RtValue::Struct`'s `Rc<RefCell<..>>` representation
;; already gives every `defstruct` instance.
(deftrait Iter
  (type Item)
  (next ((self Self)) Option<Item>))

;; `vector-iter<T>` is `Vector<T>`'s own iterator: a shared reference to the
;; vector being walked (`vec`, sharing the same underlying `RtValue::Struct`
;; — pushing to the original after creating an iterator is visible through
;; it, matching Rust's own growable-vec iterators) plus a cursor position
;; (`pos`). `Vector<T>` is deliberately *not* `Iter` itself (its `next` would
;; have to choose between starting over and corrupting external iteration
;; state — `vector-iter<T>` is the conventional fix: a separate, independent
;; cursor per `(v::iter)` call).
(defstruct (vector-iter T) (vec Vector<T>) (pos i32))
(impl Iter vector-iter<T>
  (type Item T)
  (next ((self Self)) Option<T>
    (if (< self::pos (len self::vec))
        (let ((v (get self::vec self::pos)))
          (setf self::pos (+ self::pos 1))
          (Option::some v))
        (Option::none))))
(defmethod iter ((self Vector<T>)) vector-iter<T> (vector-iter::new self 0))

;; `cons-cell<A,B>`: a generic 2-field product, needed below purely because
;; typelisp has no built-in tuple syntax. Named and shaped after Lisp's own
;; convention for storing two values — a cons cell, `car`/`cdr` — rather
;; than an arbitrary `first`/`second` struct; unlike the built-in `Sexpr`
;; `Cons` (whose `car`/`cdr` are each dynamically, independently typed —
;; see the note on `Sexpr` deliberately having no `Iter` impl, just below),
;; `cons-cell<A,B>`'s `car`/`cdr` are the type parameters `A`/`B`, fixed
;; once per instantiation — sound to build generically the same way
;; `Vector<T>`/`vector-iter<T>` are. `HashTable<K,V>::entries`
;; (`registry::hashtable_def`) yields a `Vector<cons-cell<K,V>>` of `(key
;; . value)` pairs.
(defstruct (cons-cell A B) (car A) (cdr B))

;; `Sexpr` deliberately has **no** `Iter` impl: `Iter`'s `Item` must be one
;; fixed type per impl (`vector-iter<T>`'s `Item` is `T`, `hashtable-
;; iter<K,V>`'s is `cons-cell<K,V>`, both parameters fixed once per
;; instantiation) — but a `Sexpr` list has no such parameter. Each `cons`
;; cell's `car` is independently, dynamically typed (`(1 "a" foo)` is a
;; perfectly ordinary list), so there is no single, correct `Item` to
;; declare; `Item = Sexpr` would type-check but throws away exactly the
;; static type information `Iter`/`doiter` exist to provide. Plain `Sexpr`
;; recursion (`car`/`cdr`/`consp`/`null`) or `dolist` (which needs no
;; `Item` — it just binds each element's type as `Sexpr`, same as this
;; would, but without pretending to be a generic trait impl) are the
;; correct way to walk a `Sexpr` list.

;; `HashTable<K,V>` iteration (TODO.md's remaining item): `keys`/`values`/
;; `entries` are Rust builtins (`registry::hashtable_def`,
;; `eval_builtin_method`'s `"hashtable"` arm) — a `HashMap` has no stable,
;; resumable cursor the way `Vector<T>`'s index does, so each call snapshots
;; the table's current contents into a fresh `Vector`, the same "iterating
;; over a snapshot" trade-off `vector-iter<T>` itself already makes for
;; `Vector<T>` (mutating the source table mid-iteration is simply not
;; observed, unlike `vector-iter<T>`'s shared reference). `hashtable-iter<K,V>`
;; reuses `vector-iter<T>`'s exact cursor logic over that snapshot rather
;; than duplicating it.
(defstruct (hashtable-iter K V) (snapshot Vector<cons-cell<K,V>>) (pos i32))
(impl Iter hashtable-iter<K,V>
  (type Item cons-cell<K,V>)
  (next ((self Self)) Option<cons-cell<K,V>>
    (if (< self::pos (len self::snapshot))
        (let ((e (get self::snapshot self::pos)))
          (setf self::pos (+ self::pos 1))
          (Option::some e))
        (Option::none))))
(defmethod iter ((self HashTable<K,V>)) hashtable-iter<K,V> (hashtable-iter::new (entries self) 0))
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
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        interp.exec(heap, tl).expect("prelude: eval failed");
    }
}
