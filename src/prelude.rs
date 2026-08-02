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

use std::path::PathBuf;

use crate::fasl::{registry_mark, source_hash, Fasl, FASL_FORMAT_VERSION};
use crate::project::FASL_EXTENSION;
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
/// Helper functions a later definition depends on are still placed earlier in
/// this string, but that is now convention rather than necessity: top-level
/// `defun`s may reference each other in any order (`Checker::
/// predeclare_program`, which every loader including this one runs first).
/// `defmacro` is the exception and remains strictly define-before-use.
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
;; `consp`/`null`/`atom` and the rest of the user-facing `Sexpr` list surface
;; (`length`/`append`/`nth`/`member`/`sort`/`assoc`/`dolist`/... ) were removed
;; in the Symbol/Sexpr redesign (Phase 5, `docs/dev/symbol-sexpr-redesign.md`):
;; `Sexpr` is now an internal type (read/eval/print/`defmacro`/self-hosting
;; `compiler.rs`), navigated with the `sexpr-*` accessor layer, not a
;; user-facing list datum. A homogeneous-collection API is to be redesigned on
;; top of `Vector<T>` and the generic `cons<T,U>` pair. `,@` (unquote-splicing)
;; keeps an internal `sexpr-append` (below); `equal`/`equalp` are now Rust
;; builtins (`registry.rs`). The island navigates `Sexpr` with the `sexpr-*`
;; layer and uses `string` methods (`append`/`equal`/`length`) for name work.

;; `symbol->string`/`string->symbol` (`docs/language-design.md` §4.1's
;; conversion catalog) are now Rust builtins (`Interp::eval_builtin`) rather
;; than prelude `defun`s: `Sexpr::Sym` wraps the first-class `Symbol` type (not
;; a `string`), so the old `(match s (Sym name) name)` / `(Sym s)` definitions
;; no longer type-check. They bridge the interned `Symbol` handle to/from its
;; textual name directly against the intern table.

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
;; Macro bodies navigate their `Sexpr` argument (`args`) through the
;; `sexpr-*` layer (`sexpr-null`/`sexpr-car`/`sexpr-cdr`/`sexpr-cons`), not
;; the user-facing `car`/`cdr`/`cons` (Phase 4 repurposes those to a generic
;; `cons<T,U>` pair) — Symbol/Sexpr redesign Phase 2. The quasiquote/`list`
;; templates still emit ordinary `car`/`cdr`/`if` symbols into the *expansion*
;; (user code), untouched.
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

;; `equal`/`equalp` (CL structural equality) are now Rust builtins
;; (`registry.rs`, `Interp::eval_builtin`'s `sexpr_equal`/`sexpr_equalp`), not
;; prelude `defun`s: the Symbol/Sexpr redesign fenced `match` off `Sexpr`
;; (Phase 5), and they are the one piece of the old user-facing `Sexpr` surface
;; kept (equality, not list manipulation — `case` expands to `(equal ..)`, and
;; they are the ubiquitous structural-comparison primitive). `length`/`append`
;; and the rest of the list operations were removed (see the note by the old
;; `consp` location above); `,@` keeps an internal `sexpr-append` (below), and
;; `compiler.rs` uses the `sexpr-*` layer plus `string` methods.

;; `sexpr-append`: the island's `Sexpr` list concatenation, for `,@`
;; (unquote-splicing — `Checker::check_qq_template` desugars each splice to a
;; `(sexpr-append spliced rest)` call) and `compiler.rs`. Navigates with the
;; `sexpr-*` layer (no `match`, no user-facing `car`/`cdr`). Placed here, before
;; the macros below, since every one of them uses `,@` in its expansion, and
;; that desugaring needs `sexpr-append` already registered.
(defun sexpr-append ((a Sexpr) (b Sexpr)) Sexpr
  (if (sexpr-consp a)
      (sexpr-cons (sexpr-car a) (sexpr-append (sexpr-cdr a) b))
      b))

;; The rest of the loop/branch primitive reduction set (see the comment by
;; `and`/`or` above) — placed here, right after `sexpr-append`, since every one
;; of these uses `,@` (unquote-splicing) somewhere in its expansion, and `,@`
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
  (let ((var (sexpr-car spec)) (count-expr (sexpr-car (sexpr-cdr spec))) (limit (gensym)))
    `(let ((,var 0) (,limit ,count-expr))
       (while (< ,var ,limit) ,@body (setf ,var (+ ,var 1))))))

;; `dolist`: walk a `Sexpr` cons list, binding `var` to each element (each a
;; `Sexpr`). Unlike `doiter` — which iterates a *homogeneous* `Vector<T>`/any
;; `Iter` and yields a single static `Item` type — `dolist`'s element is the
;; heterogeneous `Sexpr` itself, so the loop body dispatches on its shape with
;; an ordinary `match` (`(match var ((cons a d) ...) ((sym s) ...) ...)`);
;; `dolist` deliberately does *not* fuse that `match` in, keeping iteration and
;; per-element pattern dispatch orthogonal (the body is a plain `progn`, so it
;; can also just use `var` without matching). `spec` is `(var list-form)` or
;; `(var list-form result-form)`; like `dotimes`/`doiter`, `gensym` gives the
;; cursor a fresh name so `list-form` is evaluated exactly once and the binding
;; can't collide with a same-named variable at the use site. `var` is bound to
;; `(sexpr-car cursor)` inside the loop, so `Checker::check` infers its type
;; (`Sexpr`) with no annotation, exactly as `doiter`/`dotimes` rely on — no
;; checker special form. The optional `result-form` becomes the whole
;; construct's value (default `()`/`Unit`); unlike CL it is evaluated *outside*
;; `var`'s scope (CL binds `var` to nil there), which is cleaner here since a
;; result form references an accumulator, never the exhausted `var`. An
;; improper/dotted list stops at the first non-`cons` cdr (the `sexpr-consp`
;; guard), never erroring. The absent-`result-form` default is spelled `(Nil)`
;; (a `Sexpr`), not `()`: this macro body itself type-checks, and the `if`'s
;; other branch (`sexpr-car`) is `Sexpr`, so a bare `()` there would be `Unit`
;; and mismatch — the spliced `(Nil)` still surfaces as `()`/`Unit` in the
;; final `let`'s tail position (the same two-faced `()` `when`/`unless` use).
(defmacro dolist (spec &rest body)
  (let ((var (sexpr-car spec))
        (list-expr (sexpr-car (sexpr-cdr spec)))
        (rest-spec (sexpr-cdr (sexpr-cdr spec)))
        (cursor (gensym)))
    (let ((result (if (sexpr-null rest-spec) (Nil) (sexpr-car rest-spec))))
      `(let ((,cursor ,list-expr))
         (while (sexpr-consp ,cursor)
           (let ((,var (sexpr-car ,cursor))) ,@body)
           (setf ,cursor (sexpr-cdr ,cursor)))
         ,result))))

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
  (if (sexpr-null clauses)
      ()
      (let ((clause (sexpr-car clauses)))
        (if (eq (sexpr-car clause) (quote else))
            `(progn ,@(sexpr-cdr clause))
            `(if ,(sexpr-car clause) (progn ,@(sexpr-cdr clause)) (cond ,@(sexpr-cdr clauses)))))))

;; `if-let`: exactly the two-armed `match` `Checker::check_if_let` used to
;; build directly (a constructor-pattern arm plus a wildcard `else` arm) —
;; the same shape `while-let` (further down) uses for its own loop
;; condition. Uses no `,@`, so it could have lived next to `and`/`or`
;; instead — kept here purely to group the whole reduction set together.
(defmacro if-let (binding then els)
  (let ((pattern (sexpr-car binding)) (val (sexpr-car (sexpr-cdr binding))))
    `(match ,val (,pattern ,then) (_ ,els))))

;; `nthcdr`/`nth`/`elt`/`last`/`butlast`/`take`/`subseq`/`copy-list`/`member`/
;; `any` and the destructive `nconc`/`nreverse` were removed with the rest of
;; the user-facing `Sexpr` list surface (Symbol/Sexpr redesign Phase 5). The
;; homogeneous-collection combinators (`map`/`filter`/`foldl`/... ) live further
;; down as generic `Iter` combinators over `Vector<T>`/`HashTable<K,V>`; a
;; `cons<T,U>`/`Sexpr` list API is to be redesigned later.

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
(defun identity<T> ((x T)) T x)
(defun const<A,B> ((x A) (y B)) A x)
(defun compose<A,B,C> ((f (fn (B) C)) (g (fn (A) B))) (fn (A) C)
  (lambda ((x A)) C (f (g x))))
(defun flip<A,B,C> ((f (fn (A B) C))) (fn (B A) C)
  (lambda ((y B) (x A)) C (f x y)))

;; CL's `keywordp`: is this symbol a keyword (`:name`)? Written as a plain
;; typelisp `defun` over `symbol->string` rather than a Rust builtin — the
;; leading colon *is* part of the interned name (there is no separate keyword
;; package/table, see `Expr::SymLit`), so the test is textual and needs no
;; runtime support of its own. The reader has already rejected every
;; malformed spelling, so a leading `:` is both necessary and sufficient.
(defun keywordp ((s symbol)) bool
  (let ((n (symbol->string s)))
    (if (> (length n) 0) (eql (ref n 0) #\:) false)))

;; CL's numeric catalog beyond the primitive machine operations
;; (`+`/`-`/`*`/`/`/`mod`/comparisons/conversions, which are Rust builtins in
;; `registry.rs`): `abs`/`signum` (all numbers), `rem` (all reals), `gcd`/`lcm`
;; (integers only), and `expt` (all numbers). Written here as ordinary typelisp
;; *methods* — not Rust builtins — so they compile through the normal function
;; path (`(compile f)` where `f` uses them) instead of needing a native-lowered
;; `rt_*` shim per operation. `defmethod` dispatches on the receiver type, so
;; the same `abs`/`signum`/... name overloads across `i32`/`i64`/`bignum`/`f64`/
;; `ratio`; each is built only from that type's primitives. `mod`/`rem` follow
;; CL: `mod` is floored (integer `mod` is a builtin; `f64`/`ratio` `mod` here
;; is `a - b*floor(a/b)`), `rem` truncated (`a - b*truncate(a/b)`, i.e.
;; `a - b*(a/b)` where `/` already truncates toward zero for integers).
;;
;; Two checker quirks shape how these are spelled: (1) a binary op dispatches on
;; its *first* argument's type, and a bare integer literal there defaults to
;; `i32`, so integer negation must be `(* self -1)` (receiver-first), never
;; `(- 0 self)`. (2) A bare literal in tail position doesn't adopt an
;; `i64`/`bignum`/`ratio` result type, so `signum` uses CL's own
;; `(if (zerop x) x (/ x (abs x)))` (which also gives `f64` the CL result: the
;; zero itself for `0.0`, not Rust's `1.0`) and `lcm`'s zero case uses
;; `(- self self)` — both carry the receiver's type without a bare literal.

;; --- i32 ---
(defmethod abs ((self i32)) i32 (if (< self 0) (* self -1) self))
(defmethod signum ((self i32)) i32 (if (= self 0) self (/ self (abs self))))
(defmethod rem ((self i32) (b i32)) i32 (- self (* b (/ self b))))
(defmethod gcd ((self i32) (b i32)) i32 (if (= b 0) (abs self) (gcd b (mod self b))))
(defmethod lcm ((self i32) (b i32)) i32
  (if (or (= self 0) (= b 0)) (- self self) (/ (abs (* self b)) (gcd self b))))

;; --- i64 ---
(defmethod abs ((self i64)) i64 (if (< self 0) (* self -1) self))
(defmethod signum ((self i64)) i64 (if (= self 0) self (/ self (abs self))))
(defmethod rem ((self i64) (b i64)) i64 (- self (* b (/ self b))))
(defmethod gcd ((self i64) (b i64)) i64 (if (= b 0) (abs self) (gcd b (mod self b))))
(defmethod lcm ((self i64) (b i64)) i64
  (if (or (= self 0) (= b 0)) (- self self) (/ (abs (* self b)) (gcd self b))))

;; --- f64 --- (`mod`/`rem` via the quotient identity)
(defmethod abs ((self f64)) f64 (if (< self 0.0) (* self -1.0) self))
(defmethod signum ((self f64)) f64 (if (= self 0.0) self (/ self (abs self))))
(defmethod mod ((self f64) (b f64)) f64 (- self (* b (floor (/ self b)))))
(defmethod rem ((self f64) (b f64)) f64 (- self (* b (truncate (/ self b)))))

;; --- bignum --- (negation is `(- (int->bignum 0) self)`: the first operand is
;; a typed `bignum`, so it dispatches correctly, unlike a bare `0`. `expt` is
;; non-negative-exponent only — a negative one would be a `ratio`, which a
;; `bignum`-returning method can't hold)
(defmethod abs ((self bignum)) bignum
  (if (< self (int->bignum 0)) (- (int->bignum 0) self) self))
(defmethod signum ((self bignum)) bignum
  (if (= self (int->bignum 0)) self (/ self (abs self))))
(defmethod rem ((self bignum) (b bignum)) bignum (- self (* b (/ self b))))
(defmethod gcd ((self bignum) (b bignum)) bignum
  (if (= b (int->bignum 0)) (abs self) (gcd b (mod self b))))
(defmethod lcm ((self bignum) (b bignum)) bignum
  (if (or (= self (int->bignum 0)) (= b (int->bignum 0))) (int->bignum 0)
      (/ (abs (* self b)) (gcd self b))))
;; Exponentiation by squaring (log-depth recursion — a linear `e`-deep
;; recursion overflows the interpreter's tree-walking stack for large `e`).
(defmethod expt ((self bignum) (e bignum)) bignum
  (if (< e (int->bignum 0))
      (panic "expt: negative exponent has no bignum result (it would be a ratio)")
      (if (= e (int->bignum 0)) (int->bignum 1)
          (if (= (mod e (int->bignum 2)) (int->bignum 0))
              (let ((h (expt self (/ e (int->bignum 2))))) (* h h))
              (* self (expt self (- e (int->bignum 1))))))))

;; --- ratio --- (`ratio->bignum` truncates toward zero, so `(bignum->ratio
;; (ratio->bignum q))` is `truncate(q)`; `rem` uses it directly, `mod` adjusts
;; the remainder by `b` when their signs differ, i.e. floored)
(defmethod abs ((self ratio)) ratio
  (if (< self (int->ratio 0)) (- (int->ratio 0) self) self))
(defmethod signum ((self ratio)) ratio
  (if (= self (int->ratio 0)) self (/ self (abs self))))
(defmethod rem ((self ratio) (b ratio)) ratio
  (- self (* b (bignum->ratio (ratio->bignum (/ self b))))))
(defmethod mod ((self ratio) (b ratio)) ratio
  (let ((r (rem self b)))
    (if (or (and (< r (int->ratio 0)) (> b (int->ratio 0)))
            (and (> r (int->ratio 0)) (< b (int->ratio 0))))
        (+ r b)
        r)))
(defun ratio-expt-int ((base ratio) (n bignum)) ratio
  (if (< n (int->bignum 0))
      (ratio-expt-int (/ (int->ratio 1) base) (- (int->bignum 0) n))
      (if (= n (int->bignum 0)) (int->ratio 1)
          (* base (ratio-expt-int base (- n (int->bignum 1)))))))
(defmethod expt ((self ratio) (e ratio)) ratio
  (if (= e (bignum->ratio (ratio->bignum e)))
      (ratio-expt-int self (ratio->bignum e))
      (panic "expt: ratio exponent must be integer-valued")))

;; CL's numeric predicates (`zerop`/`plusp`/`minusp`/`evenp`/`oddp`) and
;; increment/decrement shorthands (`1+`/`1-`), built from the primitives
;; above the same way `abs`/`signum`/`rem` are — per-type `defmethod`s rather
;; than one generic definition, for the same reason (`defmethod` resolves by
;; the receiver's exact type, not by a trait bound). `evenp`/`oddp` are
;; integer-only (CL signals a type error on a non-integer; `i32`/`i64`/
;; `bignum` here). See the `--- bignum ---`/`--- ratio ---` sections above
;; for why their `0`/`1` literals are spelled `(int->bignum 0)`/`(int->ratio
;; 1)` rather than bare `0`/`1`: a literal argument doesn't pick up the
;; receiver's type on its own.
;; --- i32 ---
(defmethod zerop ((self i32)) bool (= self 0))
(defmethod plusp ((self i32)) bool (> self 0))
(defmethod minusp ((self i32)) bool (< self 0))
(defmethod evenp ((self i32)) bool (= (mod self 2) 0))
(defmethod oddp ((self i32)) bool (/= (mod self 2) 0))
(defmethod 1+ ((self i32)) i32 (+ self 1))
(defmethod 1- ((self i32)) i32 (- self 1))
;; --- i64 ---
(defmethod zerop ((self i64)) bool (= self 0))
(defmethod plusp ((self i64)) bool (> self 0))
(defmethod minusp ((self i64)) bool (< self 0))
(defmethod evenp ((self i64)) bool (= (mod self 2) 0))
(defmethod oddp ((self i64)) bool (/= (mod self 2) 0))
(defmethod 1+ ((self i64)) i64 (+ self 1))
(defmethod 1- ((self i64)) i64 (- self 1))
;; --- f64 --- (no evenp/oddp: CL requires an integer argument)
(defmethod zerop ((self f64)) bool (= self 0.0))
(defmethod plusp ((self f64)) bool (> self 0.0))
(defmethod minusp ((self f64)) bool (< self 0.0))
(defmethod 1+ ((self f64)) f64 (+ self 1.0))
(defmethod 1- ((self f64)) f64 (- self 1.0))
;; --- bignum ---
(defmethod zerop ((self bignum)) bool (= self (int->bignum 0)))
(defmethod plusp ((self bignum)) bool (> self (int->bignum 0)))
(defmethod minusp ((self bignum)) bool (< self (int->bignum 0)))
(defmethod evenp ((self bignum)) bool (= (mod self (int->bignum 2)) (int->bignum 0)))
(defmethod oddp ((self bignum)) bool (/= (mod self (int->bignum 2)) (int->bignum 0)))
(defmethod 1+ ((self bignum)) bignum (+ self (int->bignum 1)))
(defmethod 1- ((self bignum)) bignum (- self (int->bignum 1)))
;; --- ratio --- (no evenp/oddp: CL requires an integer argument)
(defmethod zerop ((self ratio)) bool (= self (int->ratio 0)))
(defmethod plusp ((self ratio)) bool (> self (int->ratio 0)))
(defmethod minusp ((self ratio)) bool (< self (int->ratio 0)))
(defmethod 1+ ((self ratio)) ratio (+ self (int->ratio 1)))
(defmethod 1- ((self ratio)) ratio (- self (int->ratio 1)))

;; `pi`: CL's `long-float` circle-ratio constant, `f64`-valued here (this
;; language's only floating-point type).
(defconstant (pi f64) 3.141592653589793 "The ratio of a circle's circumference to its diameter.")

;; The rest of CL's bitwise catalog (CLHS 12.10), all defined in terms of the
;; `logand`/`logior`/`logxor`/`lognot` primitives above (`registry.rs`) —
;; these compile via the normal path (built from already-native ops), unlike
;; the primitives themselves.
;; --- i32 ---
(defmethod logeqv ((self i32) (b i32)) i32 (lognot (logxor self b)))
(defmethod lognand ((self i32) (b i32)) i32 (lognot (logand self b)))
(defmethod lognor ((self i32) (b i32)) i32 (lognot (logior self b)))
(defmethod logandc1 ((self i32) (b i32)) i32 (logand (lognot self) b))
(defmethod logandc2 ((self i32) (b i32)) i32 (logand self (lognot b)))
(defmethod logorc1 ((self i32) (b i32)) i32 (logior (lognot self) b))
(defmethod logorc2 ((self i32) (b i32)) i32 (logior self (lognot b)))
;; --- i64 ---
(defmethod logeqv ((self i64) (b i64)) i64 (lognot (logxor self b)))
(defmethod lognand ((self i64) (b i64)) i64 (lognot (logand self b)))
(defmethod lognor ((self i64) (b i64)) i64 (lognot (logior self b)))
(defmethod logandc1 ((self i64) (b i64)) i64 (logand (lognot self) b))
(defmethod logandc2 ((self i64) (b i64)) i64 (logand self (lognot b)))
(defmethod logorc1 ((self i64) (b i64)) i64 (logior (lognot self) b))
(defmethod logorc2 ((self i64) (b i64)) i64 (logior self (lognot b)))
;; --- bignum ---
(defmethod logeqv ((self bignum) (b bignum)) bignum (lognot (logxor self b)))
(defmethod lognand ((self bignum) (b bignum)) bignum (lognot (logand self b)))
(defmethod lognor ((self bignum) (b bignum)) bignum (lognot (logior self b)))
(defmethod logandc1 ((self bignum) (b bignum)) bignum (logand (lognot self) b))
(defmethod logandc2 ((self bignum) (b bignum)) bignum (logand self (lognot b)))
(defmethod logorc1 ((self bignum) (b bignum)) bignum (logior (lognot self) b))
(defmethod logorc2 ((self bignum) (b bignum)) bignum (logior self (lognot b)))

;; `sort`/`insert-sorted`/`member`/`assoc`/`every`/`any` (user-facing `Sexpr`
;; list operations) were removed with the rest of the `Sexpr` list surface
;; (Symbol/Sexpr redesign Phase 5). The self-hosting compiler (`compiler.rs`)
;; needs none of them: it navigates its `Sexpr` AST with the `sexpr-*` accessor
;; layer and concatenates/compares names with the `string` instance methods
;; (`append`/`equal`/`length` on `string`, `registry::string_assoc`).

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
  (let ((pattern (sexpr-car binding))
        (val (sexpr-car (sexpr-cdr binding))))
    `(loop (match ,val (,pattern ,@body) (_ (break))))))

;; `doiter`: iterate a value of any type implementing the `Iter` trait
;; (`Item`/`next`, declared
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
  (let ((var (sexpr-car spec))
        (coll-expr (sexpr-car (sexpr-cdr spec)))
        (tmp (gensym)))
    `(let ((,tmp ,coll-expr))
       (while-let ((some ,var) (next ,tmp)) ,@body))))

;; `sexpr-map`: the island's own `Sexpr`→`Sexpr` list map, for macro bodies
;; (`case`/`do` below) that build their expansion by transforming a clause /
;; binding list at expansion time. It navigates with the `sexpr-*` layer
;; (`sexpr-consp`/`sexpr-car`/`sexpr-cdr`/`sexpr-cons`) rather than the
;; user-facing `car`/`cdr`/`consp`, per the Symbol/Sexpr redesign — the plain
;; `map` is now the generic `Iter` combinator further down (an iterator, not a
;; `Sexpr`, so it cannot walk a macro's argument list). Macro authors doing
;; the same should likewise reach for `sexpr-*` (see the redesign doc §7).
(defun sexpr-map ((f (fn (Sexpr) Sexpr)) (lst Sexpr)) Sexpr
  (if (sexpr-consp lst)
      (sexpr-cons (f (sexpr-car lst)) (sexpr-map f (sexpr-cdr lst)))
      ()))

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
       (cond ,@(sexpr-map (lambda ((c Sexpr)) Sexpr
                       (if (eq (sexpr-car c) (quote else))
                           c
                           (sexpr-cons (list (quote equal) tmp (sexpr-car c)) (sexpr-cdr c))))
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
  (let ((test (sexpr-car test-result))
        (result (sexpr-cdr test-result))
        (temps (sexpr-map (lambda ((b Sexpr)) Sexpr (list (gensym) (sexpr-car b) (sexpr-car (sexpr-cdr (sexpr-cdr b)))))
                     bindings)))
    `(let ,(sexpr-map (lambda ((b Sexpr)) Sexpr (list (sexpr-car b) (sexpr-car (sexpr-cdr b)))) bindings)
       (while (not ,test)
         ,@body
         (let ,(sexpr-map (lambda ((tr Sexpr)) Sexpr (list (sexpr-car tr) (sexpr-car (sexpr-cdr (sexpr-cdr tr))))) temps)
           ,@(sexpr-map (lambda ((tr Sexpr)) Sexpr (list (quote setf) (sexpr-car (sexpr-cdr tr)) (sexpr-car tr))) temps)))
       ,@result)))

;; `Iter`: the trait `doiter` requires every iterable type to implement — a single
;; `next` method returning the next element, or `(none)` once exhausted.
;; `Self` mutates in place across calls (no immutable "next state" returned
;; alongside the element — see the trait machinery's design notes): a
;; `next` implementation is expected to update its own fields via `setf`,
;; the same mutation `RtValue::Struct`'s `Rc<RefCell<..>>` representation
;; already gives every `defstruct` instance.
(deftrait Iter ()
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
(defstruct vector-iter<T> (vec Vector<T>) (pos i32))
(impl Iter vector-iter<T>
  (type Item T)
  (next ((self Self)) Option<T>
    (if (< self::pos (len self::vec))
        (let ((v (get self::vec self::pos)))
          (setf self::pos (+ self::pos 1))
          (Option::some v))
        (Option::none))))
(defmethod iter ((self Vector<T>)) vector-iter<T> (vector-iter::new self 0))

;; Generic collection combinators (redesign Phase 4). Each takes an
;; *iterator* — any type implementing the `Iter` trait — rather than one
;; specific collection, so a single definition serves `Vector<T>`,
;; `HashTable<K,V>`, and any future `Iter` type. Call as `(map (iter coll)
;; f)`: `(iter coll)` bridges a collection to its cursor (`Vector<T>`'s
;; `vector-iter<T>`, `HashTable<K,V>`'s `hashtable-iter<K,V>`), and these walk
;; that cursor with `doiter`.
;;
;; The element type `A` is the iterator's associated `Item`, declared via
;; `(where (Iter I (Item A)))` — a *polymorphic* associated-type pin: `A` is
;; one of the function's own type parameters, inferred at each call site from
;; the predicate/mapping function's argument type and verified against the
;; iterator's real `Item` (`Checker::check_call`'s where-bound validation).
;; This is why they are generic `defun`s, not `defmethod`s: `map`/`foldl`/
;; `foldr` need type variables (`U`/`B`, the mapped element / accumulator)
;; beyond the receiver's, which a method — generic only through its owner's
;; parameters — cannot introduce.
;;
;; Results that are themselves collections materialize into a fresh `Vector`
;; (there is no generic "rebuild the original container" facility, and no
;; lazy iterator type); `foldr`/`reverse` first buffer the whole input into a
;; `Vector` because an `Iter` is forward-only. `find-if`/`position-if`/
;; `count-if`/`remove-if` take a predicate, not an element: `A` carries no
;; `Eq`-style bound here, so there is no generic equality available to them —
;; matching CL's own `-if` family. The plain, `Eq`-bounded `find`/`position`/
;; `count` (CL's own item-based versions, alongside `member`/`sort` below)
;; live further down, past the scalar `Eq`/`Ord` impls they need. A freshly
;; built result vector is pinned with `the`, since a bare `(Vector::new)` has
;; nothing to infer its element type from.
(defun map<I,A,U> ((it I) (f (fn (A) U))) Vector<U> (where (Iter I (Item A)))
  (let ((out (the Vector<U> (Vector::new))))
    (doiter (x it) (push out (f x)))
    out))
(defun filter<I,A> ((it I) (pred (fn (A) bool))) Vector<A> (where (Iter I (Item A)))
  (let ((out (the Vector<A> (Vector::new))))
    (doiter (x it) (if (pred x) (push out x) ()))
    out))
(defun foldl<I,A,B> ((it I) (f (fn (B A) B)) (init B)) B (where (Iter I (Item A)))
  (let ((acc init))
    (doiter (x it) (setf acc (f acc x)))
    acc))
(defun foldr<I,A,B> ((it I) (f (fn (A B) B)) (init B)) B (where (Iter I (Item A)))
  (let ((buf (the Vector<A> (Vector::new))))
    (doiter (x it) (push buf x))
    (let ((acc init) (i (- (len buf) 1)))
      (while (>= i 0)
        (setf acc (f (get buf i) acc))
        (setf i (- i 1)))
      acc)))
(defun reverse<I,A> ((it I)) Vector<A> (where (Iter I (Item A)))
  (let ((buf (the Vector<A> (Vector::new))))
    (doiter (x it) (push buf x))
    (let ((out (the Vector<A> (Vector::new))) (i (- (len buf) 1)))
      (while (>= i 0)
        (push out (get buf i))
        (setf i (- i 1)))
      out)))
(defun find-if<I,A> ((it I) (pred (fn (A) bool))) Option<A> (where (Iter I (Item A)))
  (let ((result (the Option<A> (Option::none))))
    (doiter (x it)
      (if (pred x) (progn (setf result (Option::some x)) (break)) ()))
    result))
(defun position-if<I,A> ((it I) (pred (fn (A) bool))) Option<i32> (where (Iter I (Item A)))
  ;; `i` counts only the mismatches seen before the match, so it equals the
  ;; index of the first match. Both `if` branches are `Unit` (`(break)` is
  ;; `Never`; the mismatch branch ends in a trailing `()`) so the loop body
  ;; type-checks.
  (let ((i 0) (result (the Option<i32> (Option::none))))
    (doiter (x it)
      (if (pred x)
          (progn (setf result (Option::some i)) (break))
          (progn (setf i (+ i 1)) ())))
    result))
(defun count-if<I,A> ((it I) (pred (fn (A) bool))) i32 (where (Iter I (Item A)))
  ;; `when`, not a bare `(if (pred x) (setf n ...) ())`: `setf` evaluates to
  ;; the value it assigned (here `i32`), so an `if` whose other branch is `()`
  ;; would fail to unify (`i32` vs `Unit`); `when` wraps the `setf` in a
  ;; `progn` with a trailing `()`, making the whole loop body `Unit`.
  (let ((n 0))
    (doiter (x it) (when (pred x) (setf n (+ n 1))))
    n))
(defun remove-if<I,A> ((it I) (pred (fn (A) bool))) Vector<A> (where (Iter I (Item A)))
  (let ((out (the Vector<A> (Vector::new))))
    (doiter (x it) (if (pred x) () (push out x)))
    out))

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
(defstruct cons-cell<A,B> (car A) (cdr B))

;; `cons`/`car`/`cdr`: the generic pair API (Symbol/Sexpr redesign Phase 4b).
;; `cons` builds a `cons-cell<A,B>`; `car`/`cdr` are `cons-cell`'s own
;; field-accessor methods (a `defstruct` generates `(car p)`/`(cdr p)` for its
;; `car`/`cdr` fields — no free `defun` needed, and a receiver-typed method is
;; what lets `(car p)` project a *statically typed* `A`). `cons` is a free
;; `defun` (not just the `cons-cell::new` constructor) so it reads like Lisp and
;; can be passed as a value. The `Sexpr` cons cell — a heterogeneous, dynamically
;; typed list node — is a different thing entirely, built/walked through the
;; `sexpr-*` island layer (`sexpr-cons`/`sexpr-car`/`sexpr-cdr`), never this
;; pair. Mutate a pair field with `(setf p::car v)`.
(defun cons<A,B> ((a A) (b B)) cons-cell<A,B> (cons-cell::new a b))

;; CL's 2-argument `floor`/`ceiling`/`round`/`truncate` (`(floor 7 2) => 3,
;; 1`): a division that reports both quotient and remainder. CL returns them
;; as two values; typelisp has no multiple-value mechanism (`docs/dev/
;; cl-missing-classes-and-methods.md` §3.4), but `cons`/`car`/`cdr` just
;; above are already a *generic, statically-typed* pair (`cons-cell<A,B>`,
;; not a homogeneous list), so the two results fit it directly with no new
;; mechanism: `(car (floor-div x y))`/`(cdr (floor-div x y))`. Named `*-div`
;; rather than reusing `floor` etc. outright because `defmethod` dispatches
;; on receiver *type*, not arity — one name can't hold both the existing
;; unary rounding method and a binary one on the same receiver type.
;;
;; Only `floor-div` touches a primitive directly (the already-floored `mod`
;; builtin/method); the other three are built from it: `ceiling-div` bumps
;; the quotient by one whenever the division isn't exact (the real quotient
;; always lies in `[fq, fq+1)`, regardless of either operand's sign, so
;; `fq+1` is always the correct other candidate); `round-div` picks whichever
;; of `fq`/`fq+1` is nearer by comparing `|2*remainder|` to `|b|` (sign-
;; agnostic since `floor-div`'s remainder always shares `b`'s sign), breaking
;; an exact tie toward the even quotient (CL's round-half-to-even — unlike
;; this file's unary `round`, whose tie-breaking rides on Rust's `f64::round`
;; and isn't relied on here); `truncate-div` doesn't share `floor-div`'s
;; helper since it needs the opposite (truncated) remainder, already
;; available as `rem`.
;; --- i32 ---
(defmethod floor-div ((self i32) (b i32)) cons-cell<i32,i32>
  (let ((r (mod self b))) (cons (/ (- self r) b) r)))
(defmethod truncate-div ((self i32) (b i32)) cons-cell<i32,i32>
  (cons (/ self b) (rem self b)))
(defmethod ceiling-div ((self i32) (b i32)) cons-cell<i32,i32>
  (let ((fd (floor-div self b)))
    (if (= (cdr fd) 0) fd
        (let ((q (+ (car fd) 1))) (cons q (- self (* q b)))))))
(defmethod round-div ((self i32) (b i32)) cons-cell<i32,i32>
  (let ((fd (floor-div self b)))
    (let ((fq (car fd)) (fr (cdr fd)))
      (let ((afr2 (abs (* fr 2))) (ab (abs b)))
        (if (< afr2 ab) fd
            (if (> afr2 ab) (let ((q (+ fq 1))) (cons q (- self (* q b))))
                (if (= (mod fq 2) 0) fd
                    (let ((q (+ fq 1))) (cons q (- self (* q b)))))))))))

;; --- i64 ---
(defmethod floor-div ((self i64) (b i64)) cons-cell<i64,i64>
  (let ((r (mod self b))) (cons (/ (- self r) b) r)))
(defmethod truncate-div ((self i64) (b i64)) cons-cell<i64,i64>
  (cons (/ self b) (rem self b)))
(defmethod ceiling-div ((self i64) (b i64)) cons-cell<i64,i64>
  (let ((fd (floor-div self b)))
    (if (= (cdr fd) 0) fd
        (let ((q (+ (car fd) 1))) (cons q (- self (* q b)))))))
(defmethod round-div ((self i64) (b i64)) cons-cell<i64,i64>
  (let ((fd (floor-div self b)))
    (let ((fq (car fd)) (fr (cdr fd)))
      (let ((afr2 (abs (* fr 2))) (ab (abs b)))
        (if (< afr2 ab) fd
            (if (> afr2 ab) (let ((q (+ fq 1))) (cons q (- self (* q b))))
                (if (= (mod fq 2) 0) fd
                    (let ((q (+ fq 1))) (cons q (- self (* q b)))))))))))

;; --- f64 ---
(defmethod floor-div ((self f64) (b f64)) cons-cell<f64,f64>
  (let ((r (mod self b))) (cons (/ (- self r) b) r)))
(defmethod truncate-div ((self f64) (b f64)) cons-cell<f64,f64>
  (cons (truncate (/ self b)) (rem self b)))
(defmethod ceiling-div ((self f64) (b f64)) cons-cell<f64,f64>
  (let ((fd (floor-div self b)))
    (if (= (cdr fd) 0.0) fd
        (let ((q (+ (car fd) 1.0))) (cons q (- self (* q b)))))))
(defmethod round-div ((self f64) (b f64)) cons-cell<f64,f64>
  (let ((fd (floor-div self b)))
    (let ((fq (car fd)) (fr (cdr fd)))
      (let ((afr2 (abs (* fr 2.0))) (ab (abs b)))
        (if (< afr2 ab) fd
            (if (> afr2 ab) (let ((q (+ fq 1.0))) (cons q (- self (* q b))))
                (if (= (mod fq 2.0) 0.0) fd
                    (let ((q (+ fq 1.0))) (cons q (- self (* q b)))))))))))

;; CL's byte-specifier mini-API (CLHS 22.1.3): `(byte size position)` builds
;; an opaque specifier consumed by `ldb`/`dpb`/`mask-field`/`deposit-field`/
;; `ldb-test`. No dedicated struct type is worth introducing for two `i32`s —
;; `cons-cell<i32,i32>` (the generic pair already used throughout this file,
;; e.g. `floor-div` above) *is* the byte specifier: `car` the size, `cdr` the
;; position. `i32` only (like the rest of this file's bit-twiddling
;; primitives) — a historical PDP-10-era API whose CL usage is overwhelmingly
;; on fixnums.
(defmethod byte-size ((self cons-cell<i32,i32>)) i32 (car self))
(defmethod byte-position ((self cons-cell<i32,i32>)) i32 (cdr self))
(defun byte ((size i32) (position i32)) cons-cell<i32,i32> (cons size position))
;; `(ldb bytespec integer)`: extract the `size`-bit field starting at
;; `position`, right-justified — `(logand (ash integer (- position)) (1-
;; (ash 1 size)))`.
(defmethod ldb ((self cons-cell<i32,i32>) (n i32)) i32
  (logand (ash n (* -1 (byte-position self))) (1- (ash 1 (byte-size self)))))
;; `(ldb-test bytespec integer)`: does that field have any 1 bits?
(defmethod ldb-test ((self cons-cell<i32,i32>) (n i32)) bool (/= (ldb self n) 0))
;; `(mask-field bytespec integer)`: like `ldb`, but left in place rather than
;; right-justified — `(logand integer (ash (1- (ash 1 size)) position))`.
(defmethod mask-field ((self cons-cell<i32,i32>) (n i32)) i32
  (logand n (ash (1- (ash 1 (byte-size self))) (byte-position self))))
;; `(dpb newbyte bytespec integer)`: deposit `newbyte`'s low `size` bits into
;; that field of `integer`, leaving every other bit of `integer` untouched.
(defmethod dpb ((newbyte i32) (self cons-cell<i32,i32>) (n i32)) i32
  (let ((mask (ash (1- (ash 1 (byte-size self))) (byte-position self))))
    (logior (logand n (lognot mask)) (logand (ash newbyte (byte-position self)) mask))))
;; `(deposit-field newbyte bytespec integer)`: like `dpb`, but `newbyte` is
;; already positioned (only its bits inside the field matter) rather than
;; right-justified.
(defmethod deposit-field ((newbyte i32) (self cons-cell<i32,i32>) (n i32)) i32
  (let ((mask (ash (1- (ash 1 (byte-size self))) (byte-position self))))
    (logior (logand n (lognot mask)) (logand newbyte mask))))

;; `(boole op a b)`: CL's 16-way generic bitwise-op selector. `op` is one of
;; the 16 `boole-*` constants below (an `i32` code, not a keyword — this
;; language has no keyword-symbol type for CL's `boole-and` etc to be).
(defconstant (boole-clr i32) 0 "boole: always 0.")
(defconstant (boole-set i32) 1 "boole: always -1 (all bits set).")
(defconstant (boole-1 i32) 2 "boole: a, unchanged.")
(defconstant (boole-2 i32) 3 "boole: b, unchanged.")
(defconstant (boole-c1 i32) 4 "boole: (lognot a).")
(defconstant (boole-c2 i32) 5 "boole: (lognot b).")
(defconstant (boole-and i32) 6 "boole: (logand a b).")
(defconstant (boole-ior i32) 7 "boole: (logior a b).")
(defconstant (boole-xor i32) 8 "boole: (logxor a b).")
(defconstant (boole-eqv i32) 9 "boole: (logeqv a b).")
(defconstant (boole-nand i32) 10 "boole: (lognand a b).")
(defconstant (boole-nor i32) 11 "boole: (lognor a b).")
(defconstant (boole-andc1 i32) 12 "boole: (logandc1 a b).")
(defconstant (boole-andc2 i32) 13 "boole: (logandc2 a b).")
(defconstant (boole-orc1 i32) 14 "boole: (logorc1 a b).")
(defconstant (boole-orc2 i32) 15 "boole: (logorc2 a b).")
(defmethod boole ((self i32) (a i32) (b i32)) i32
  (if (= self boole-clr) 0
  (if (= self boole-set) -1
  (if (= self boole-1) a
  (if (= self boole-2) b
  (if (= self boole-c1) (lognot a)
  (if (= self boole-c2) (lognot b)
  (if (= self boole-and) (logand a b)
  (if (= self boole-ior) (logior a b)
  (if (= self boole-xor) (logxor a b)
  (if (= self boole-eqv) (logeqv a b)
  (if (= self boole-nand) (lognand a b)
  (if (= self boole-nor) (lognor a b)
  (if (= self boole-andc1) (logandc1 a b)
  (if (= self boole-andc2) (logandc2 a b)
  (if (= self boole-orc1) (logorc1 a b)
  (if (= self boole-orc2) (logorc2 a b)
      (panic "boole: unknown op code"))))))))))))))))))

;; `Sexpr` deliberately has **no** `Iter` impl: `Iter`'s `Item` must be one
;; fixed type per impl (`vector-iter<T>`'s `Item` is `T`, `hashtable-
;; iter<K,V>`'s is `cons-cell<K,V>`, both parameters fixed once per
;; instantiation) — but a `Sexpr` list has no such parameter. Each `Sexpr` cons
;; cell's `car` is independently, dynamically typed (`(1 "a" foo)` is a
;; perfectly ordinary list), so there is no single, correct `Item` to
;; declare; `Item = Sexpr` would type-check but throws away exactly the
;; static type information `Iter`/`doiter` exist to provide. `Sexpr` recursion
;; through the `sexpr-*` layer (`sexpr-consp`/`sexpr-car`/`sexpr-cdr`) is the
;; correct way to walk a `Sexpr` list.

;; `HashTable<K,V>` iteration: `keys`/`values`/
;; `entries` are Rust builtins (`registry::hashtable_def`,
;; `eval_builtin_method`'s `"hashtable"` arm) — a `HashMap` has no stable,
;; resumable cursor the way `Vector<T>`'s index does, so each call snapshots
;; the table's current contents into a fresh `Vector`, the same "iterating
;; over a snapshot" trade-off `vector-iter<T>` itself already makes for
;; `Vector<T>` (mutating the source table mid-iteration is simply not
;; observed, unlike `vector-iter<T>`'s shared reference). `hashtable-iter<K,V>`
;; reuses `vector-iter<T>`'s exact cursor logic over that snapshot rather
;; than duplicating it.
(defstruct hashtable-iter<K,V> (snapshot Vector<cons-cell<K,V>>) (pos i32))
(impl Iter hashtable-iter<K,V>
  (type Item cons-cell<K,V>)
  (next ((self Self)) Option<cons-cell<K,V>>
    (if (< self::pos (len self::snapshot))
        (let ((e (get self::snapshot self::pos)))
          (setf self::pos (+ self::pos 1))
          (Option::some e))
        (Option::none))))
(defmethod iter ((self HashTable<K,V>)) hashtable-iter<K,V> (hashtable-iter::new (entries self) 0))

;; `Eq`/`Ord` (redesign Phase 6.5): user-visible equality/ordering *traits*, so
;; `member`/`assoc`/`find`/`position`/`count` below can require `(where (Eq A))`
;; instead of taking a predicate — CL's own item-based searches, alongside
;; `find-if`/`position-if`/`count-if`/`remove-if` above, which keep their
;; predicate form. `sort` takes an explicit comparator (CL's own required
;; `predicate` argument) instead of an `Ord` bound, so any strict-weak-order
;; function works, not just a type's natural `Ord` impl.
;;
;; Modelled on Rust's `PartialEq`/`PartialOrd`, kept under the names `Eq`/`Ord`
;; (renaming would churn every `where (Ord T)`/`(Eq T)` bound below). `Eq` is
;; Rust's `PartialEq` (`equals`=`==`, `not-equals`=`!=`); `Ord` is Rust's
;; `PartialOrd` (`less`=`<`, `less-equal`=`<=`, `greater`=`>`, `greater-equal`=
;; `>=`), and `Ord` inherits `Eq` exactly as `PartialOrd: PartialEq` does.
;; Only the core method of each is left to the implementor — `not-equals`,
;; `less-equal`, `greater` and `greater-equal` all have default bodies here —
;; so the scalar impls below just delegate `equals`/`less` to the builtin
;; per-type operators (`=`/`<`, `equal`/`eq`, all overloaded by receiver type,
;; see `registry.rs`). Method names avoid the builtin operator/
;; `eq`/`lt` names on purpose: builtins cannot be redefined, and `?`/`!` name
;; suffixes are banned project-wide (so no `eq?`/`less?`).
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)
  (not-equals ((self Self) (other Self)) bool
    "The negation of `equals` — Rust's `PartialEq::ne` default."
    (not (equals self other))))
(deftrait Ord (Eq)
  (less ((self Self) (other Self)) bool)
  (less-equal ((self Self) (other Self)) bool
    "`a<=b` iff not `b<a`."
    (not (less other self)))
  (greater ((self Self) (other Self)) bool
    "`a>b` iff `b<a`."
    (less other self))
  (greater-equal ((self Self) (other Self)) bool
    "`a>=b` iff not `a<b`."
    (not (less self other))))

;; Scalar `Eq` impls. Numbers delegate to `=`/`/=`; `string`/`char`/`bool` to
;; `equal` (content comparison — a `string`'s `eq` is `Rc` identity, which
;; would make `(member "x" ...)` fail on separately built equal-content
;; strings); `symbol` to `eq` (interned, identity *is* content equality).
;; `not-equals` comes from `Eq`'s default body, so each impl supplies only the
;; core `equals`. Only the seven scalar types with a usable comparison builtin
;; get an impl — `f32`/`i8`/`i16`/`u*` have empty assoc tables (no `=`/`<` to
;; delegate to), so they stay outside `Eq`/`Ord` until they grow real
;; arithmetic.
(impl Eq i32    (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq i64    (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq f64    (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq bignum (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq ratio  (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq bool   (equals ((self Self) (other Self)) bool (equal self other)))
(impl Eq char   (equals ((self Self) (other Self)) bool (equal self other)))
(impl Eq string (equals ((self Self) (other Self)) bool (equal self other)))
(impl Eq symbol (equals ((self Self) (other Self)) bool (eq self other)))

;; Scalar `Ord` impls — only the core `less`; the other three come from `Ord`'s
;; default bodies. Every scalar here has the overloaded `<` builtin (numbers
;; always had it; `char`/`string` gained one alongside these expanded traits —
;; code-point / lexicographic order). `bool`/`symbol` carry no meaningful
;; order, so no `Ord` for them. Each also needs its `Eq` impl above, since
;; `Ord` inherits `Eq`.
(impl Ord i32    (less ((self Self) (other Self)) bool (< self other)))
(impl Ord i64    (less ((self Self) (other Self)) bool (< self other)))
(impl Ord f64    (less ((self Self) (other Self)) bool (< self other)))
(impl Ord bignum (less ((self Self) (other Self)) bool (< self other)))
(impl Ord ratio  (less ((self Self) (other Self)) bool (< self other)))
(impl Ord char   (less ((self Self) (other Self)) bool (< self other)))
(impl Ord string (less ((self Self) (other Self)) bool (< self other)))

;; `cons-cell<A,B>`'s Eq/Ord: recursive (structural) comparison. The field
;; comparisons go through the methods' own `where` bounds — checked as
;; diagnostics-only `TraitCall`s while `A`/`B` are open, then resolved to the
;; field types' real `equals`/`less` when the owner monomorphizes (nesting,
;; e.g. `cons-cell<cons-cell<i32,i32>,i32>`, just recurses one level per
;; instantiation). Note the bound names must be the *owner's* declared type
;; parameters (`A`/`B`, exactly as written on the `defstruct`) — a method's
;; signature can only be generic through them.
(impl Eq cons-cell<A,B> (where (Eq A) (Eq B))
  (equals ((self Self) (other Self)) bool
    (if (equals self::car other::car)
        (equals self::cdr other::cdr)
        false)))

;; Ord: lexicographic, `car` first, then `cdr`. The double-`less` form (car
;; strictly less → true; car strictly greater → false; otherwise cars are
;; equivalent, compare cdrs) keeps `less`'s own body free of any appeal to
;; equality, so the recursion needs `(Ord A)`/`(Ord B)` and nothing more.
;; `Ord` inheriting `Eq` does oblige the *impl as a whole* to have an
;; `impl Eq cons-cell<A,B>` — which is right above — so nothing is lost;
;; before supertraits existed this shape additionally avoided writing
;; `(Eq A)` in the bound list.
;; `less-equal`/`greater`/`greater-equal` come from `Ord`'s default bodies.
(impl Ord cons-cell<A,B> (where (Ord A) (Ord B))
  (less ((self Self) (other Self)) bool
    (if (less self::car other::car)
        true
        (if (less other::car self::car)
            false
            (less self::cdr other::cdr)))))

;; The `Error` trait — what every error type implements, modeled on Rust's
;; `std::error::Error`. `Error` is a *trait*, never a type: there is no
;; general-purpose concrete error value in this language. Each fallible
;; built-in returns its own concrete type (`ParseIntError`/`ParseFloatError`/
;; `ReadError`/`EvalError`, `registry::builtin_error_defs`), a user's error
;; type is an ordinary `defstruct`/`defenum`, and code that must hold any of
;; them uniformly writes `Result<T, :dyn Error>` — the counterpart of Rust's
;; `Box<dyn Error>`. Types and traits share one name space per module
;; (`Checker::check_type_trait_clash`), which is exactly why the concrete
;; types could not also be called `Error`.
;;
;; `source` is Rust's `Error::source`: the error this one wraps, or `None` at
;; the root of the chain. Its return type mentions a trait object of the very
;; trait being declared, which is why `deftrait` pre-registers the trait
;; before parsing its own method signatures.
(deftrait Error ()
  (message ((self Self)) string)
  (source ((self Self)) Option<:dyn Error>
    "The error this one wraps, if any. Defaults to `none` — most error types
     are leaves, so only a wrapping type needs to override this."
    (option::none)))

;; The built-in error types' impls. Each carries one message string in its
;; single variant (type name and variant name coincide), and none of them
;; wraps another error, so `source` is uniformly `None` — a built-in error is
;; always the root cause. Nothing here is privileged: these read exactly like
;; the impl a user writes for their own error type.
(impl Error ParseIntError
  (message ((self Self)) string (match self ((ParseIntError m) m))))
(impl Error ParseFloatError
  (message ((self Self)) string (match self ((ParseFloatError m) m))))
(impl Error ReadError
  (message ((self Self)) string (match self ((ReadError m) m))))
(impl Error EvalError
  (message ((self Self)) string (match self ((EvalError m) m))))

;; Widen a `Result`'s concrete error type to `:dyn Error`, so results from
;; different fallible operations can flow into one `Result<T, :dyn Error>` —
;; what Rust's `?` does through `From` for `Box<dyn Error>`, spelled as an
;; ordinary call since this language has no `?` (§7.3). The `(Error E)` bound
;; is what admits the boxing while `E` is still open: the checker leaves an
;; erased-generic placeholder here and lays out the real vtable when the call
;; site's concrete `E` specializes this body (`Checker::coerce_to_dyn`).
(defun as-dyn-error<T,E> ((r Result<T,E>)) Result<T, :dyn Error> (where (Error E))
  (match r
    ((ok v) (result::ok v))
    ((err e) (result::err (as :dyn Error e)))))

;; Sequence operations over `Iter` (redesign Phase 6.5) — the typed rebuild
;; of the `Sexpr` list library removed in Phase 5 (`length`/`append`/`nth`/
;; `elt`/`take`/`subseq`/`last`/`butlast`/`member`/`every`/`any`/`sort`/
;; `assoc`). Same shape as the combinators above: generic `defun`s over any
;; `Iter`, called as `(length (iter coll))`, collection results materialized
;; into a fresh `Vector`. Not reintroduced: destructive `nconc`/`nreverse`
;; and the cons-chain-only `nthcdr`/`copy-list` (`reverse` above covers the
;; non-destructive reversal).
;;
;; Deliberate departures from CL, forced by the iterator shape:
;; - `member` returns `bool`, not the tail (an iterator has no tail cons);
;; - `last` returns the last *element* (`Option<A>`), not the last cons;
;; - `nth`/`elt` return `Option<A>` on out-of-range instead of `nil`
;;   (matching `find`/`position` above), and keep their CL argument orders
;;   (`(nth n it)` vs `(elt it n)`);
;; - `subseq` clamps `end` past the input's length instead of erroring.
;;
;; Every body is self-contained (no delegating to a sibling bounded generic:
;; where-bound propagation from one generic's body into another's bounds is
;; unimplemented — `check_call`'s bound validation would fail with "cannot
;; infer" — which is why `elt` duplicates `nth`'s loop).
(defun length<I,A> ((it I)) i32 (where (Iter I (Item A)))
  (let ((n 0))
    (doiter (x it) (setf n (+ n 1)))
    n))
;; `append` is the generic two-iterator concatenation (replacing Phase 3's
;; `vector-append`): two independent `Iter` bounds pinned to one `Item`.
(defun append<I,J,A> ((a I) (b J)) Vector<A>
  (where (Iter I (Item A)) (Iter J (Item A)))
  (let ((out (the Vector<A> (Vector::new))))
    (doiter (x a) (push out x))
    (doiter (x b) (push out x))
    out))
(defun nth<I,A> ((n i32) (it I)) Option<A> (where (Iter I (Item A)))
  (let ((i 0) (result (the Option<A> (Option::none))))
    (doiter (x it)
      (if (= i n)
          (progn (setf result (Option::some x)) (break))
          (progn (setf i (+ i 1)) ())))
    result))
(defun elt<I,A> ((it I) (n i32)) Option<A> (where (Iter I (Item A)))
  (nth n it))
(defun take<I,A> ((it I) (n i32)) Vector<A> (where (Iter I (Item A)))
  (let ((out (the Vector<A> (Vector::new))))
    (doiter (x it) (if (< (len out) n) (push out x) (break)))
    out))
(defun subseq<I,A> ((it I) (start i32) (end i32)) Vector<A> (where (Iter I (Item A)))
  (let ((i 0) (out (the Vector<A> (Vector::new))))
    (doiter (x it)
      (if (>= i end)
          (break)
          (progn (when (>= i start) (push out x)) (setf i (+ i 1)) ())))
    out))
(defun last<I,A> ((it I)) Option<A> (where (Iter I (Item A)))
  (let ((result (the Option<A> (Option::none))))
    (doiter (x it) (setf result (Option::some x)))
    result))
(defun butlast<I,A> ((it I)) Vector<A> (where (Iter I (Item A)))
  (let ((buf (the Vector<A> (Vector::new))))
    (doiter (x it) (push buf x))
    (let ((out (the Vector<A> (Vector::new))) (i 0))
      (while (< i (- (len buf) 1))
        (push out (get buf i))
        (setf i (+ i 1)))
      out)))
;; `member`/`assoc` require `Eq` (defined below with the scalar impls)
;; instead of taking a predicate — the trait-bounded half of the library.
;; Predicate variants of the same searches exist above (`find-if`/
;; `position-if`/`count-if`), and item-based `find`/`position`/`count` (CL's
;; own, `Eq`-bounded like `member`) exist right below `sort`.
(defun member<I,A> ((x A) (it I)) bool (where (Iter I (Item A)) (Eq A))
  (let ((found false))
    (doiter (y it)
      (if (equals y x) (progn (setf found true) (break)) ()))
    found))
;; CL's own item-based `find`/`position`/`count` (its default `:test` is
;; `eql`; this language's one generic equality trait is `Eq`, so these bound
;; on it like `member` does) — the counterparts of `find-if`/`position-if`/
;; `count-if` above, which take a predicate instead.
(defun find<I,A> ((x A) (it I)) Option<A> (where (Iter I (Item A)) (Eq A))
  (let ((result (the Option<A> (Option::none))))
    (doiter (y it)
      (if (equals y x) (progn (setf result (Option::some y)) (break)) ()))
    result))
(defun position<I,A> ((x A) (it I)) Option<i32> (where (Iter I (Item A)) (Eq A))
  (let ((i 0) (result (the Option<i32> (Option::none))))
    (doiter (y it)
      (if (equals y x)
          (progn (setf result (Option::some i)) (break))
          (progn (setf i (+ i 1)) ())))
    result))
(defun count<I,A> ((x A) (it I)) i32 (where (Iter I (Item A)) (Eq A))
  (let ((n 0))
    (doiter (y it) (when (equals y x) (setf n (+ n 1))))
    n))
(defun every<I,A> ((it I) (pred (fn (A) bool))) bool (where (Iter I (Item A)))
  (let ((result true))
    (doiter (x it)
      (if (pred x) () (progn (setf result false) (break))))
    result))
(defun any<I,A> ((it I) (pred (fn (A) bool))) bool (where (Iter I (Item A)))
  (let ((result false))
    (doiter (x it)
      (if (pred x) (progn (setf result true) (break)) ()))
    result))
;; Non-destructive insertion sort, stable: the inner shift uses strict `cmp`,
;; so equal elements (per `cmp`) keep their input order. CL's own `sort`
;; signature — `predicate` is a required argument, not an `Ord` bound, so any
;; strict-weak-order function works (`(lambda ((a i32) (b i32)) bool (< a
;; b))` for ascending, `(flip ...)`-wrapped or reversed for descending, a
;; key-projecting comparator, etc.) — matching CL's `(sort sequence
;; predicate)` exactly (`predicate` returns true when its first argument
;; belongs strictly before its second).
(defun sort<I,A> ((it I) (cmp (fn (A A) bool))) Vector<A> (where (Iter I (Item A)))
  (let ((out (the Vector<A> (Vector::new))))
    (doiter (x it)
      (let ((j (len out)))
        (push out x)
        (while (if (> j 0) (cmp x (get out (- j 1))) false)
          (set out j (get out (- j 1)))
          (setf j (- j 1)))
        (set out j x)))
    out))
;; `assoc` works over any iterator whose `Item` is a `cons-cell<K,V>` pair —
;; an alist (`Vector<cons-cell<K,V>>`) and a `HashTable<K,V>` (whose `iter`'s
;; `Item` is exactly `cons-cell<K,V>`) both qualify. Returns the whole
;; matching pair, CL-style; project the value with `(cdr p)`.
(defun assoc<I,K,V> ((k K) (it I)) Option<cons-cell<K,V>>
  (where (Iter I (Item cons-cell<K,V>)) (Eq K))
  (let ((result (the Option<cons-cell<K,V>> (Option::none))))
    (doiter (p it)
      (if (equals (car p) k) (progn (setf result (Option::some p)) (break)) ()))
    result))

;; ---------------------------------------------------------------------------
;; Pretty-printer controls (CLHS 22.1.1 / 22.2 — the `*print-*` variables the
;; pretty printer consults).
;;
;; CL makes these *special* variables, so a caller rebinds them with `let` for
;; the extent of one printing operation. typelisp has no dynamic binding, so
;; they are ordinary assignable globals instead: `(setf *print-pretty* true)`
;; takes effect from the next `print`/`println`/`format` on and stays in
;; effect. `Interp::pretty_opts` reads all three fresh at the start of every
;; printing operation, so an assignment is picked up immediately.
;;
;; `*print-pretty*` is `false` by default (CL leaves the initial value
;; implementation-defined): every existing program keeps its exact current
;; output, and pretty printing is something a program opts into. `pprint` and
;; friends pretty-print unconditionally, as CL's do.
;;
;; `*print-miser-width*` has no `nil` here — 0 (or less) is "miser style off",
;; the same meaning CL gives `nil`. Likewise a `*print-right-margin*` of 0 or
;; less means "no right margin", so nothing ever needs to break.
;; `print-object` (CLHS 22.1.4 / the CLOS generic function of the same name):
;; a type's own printed representation. `print`/`println`/`format`/`pprint`
;; consult it for every value they render whose type implements it — including
;; values nested inside a list — so a `defstruct`/`defenum` prints the way its
;; author decided rather than as the built-in `#<name field...>` fallback.
;;
;; CL has two separate mechanisms here and this is the one that fits a
;; statically typed language: `print-object` is a *generic function*, so a
;; class's method is written once, at the class, and type-checked there. (CL's
;; other mechanism, `set-pprint-dispatch`, keys a runtime table by type
;; *specifier* — an unchecked string here, forcing the printer to re-`match`
;; the very type its registration already knew. See docs/dev/TODO.md's T5-b.)
;;
;; `escape` is CL's `*print-escape*`: true under `~s`/`prin1`/`pprint` (reader
;; syntax), false under `~a`/`princ` (human-facing). A printer that doesn't
;; care can ignore it — Rust's `Display` and `Debug` folded into one method.
;;
;; No built-in type implements this: every existing program's output stays
;; byte-for-byte what it was, and a custom representation is something a type
;; opts into.
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))

;; `pprint-exit-if-list-exhausted` (CLHS): leave the enclosing
;; `pprint-logical-block` when its list is used up. CL implements this as a
;; non-local exit from the block; typelisp has no general escape, so it exits
;; the enclosing `loop` instead — which is exactly where CL's own idiom always
;; puts it:
;;
;;   (pprint-logical-block (xs :prefix "(" :suffix ")")
;;     (loop (pprint-exit-if-list-exhausted)
;;           (print "~w" (pprint-pop))
;;           (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
(pub defmacro pprint-exit-if-list-exhausted ()
  `(if (pprint-list-exhausted) (break) ()))
(pub defvar (*print-pretty* bool) false)
(pub defvar (*print-right-margin* i64) 80)
(pub defvar (*print-miser-width* i64) 0)

;; The "what to print" controls (CLHS 22.1.1), read by `Interp::print_limits`
;; on every printing operation just like the three above.
;;
;; `*print-level*`/`*print-length*` bound how much of a nested structure is
;; shown: an object at `*print-level*` or deeper prints as `#`, and only the
;; first `*print-length*` elements of a list (or fields of a `defstruct`/
;; `defenum` value) print, the rest as `...`. CL spells "no limit" `nil`;
;; here 0 or less means no limit, the same convention `*print-right-margin*`
;; uses. Both default to unlimited, as CL's do.
;;
;; `*print-circle*` makes the printer walk the value first and label whatever
;; it reaches twice: the first occurrence prints as `#n=<object>` and every
;; later one as `#n#`. **This is what makes a circular structure printable at
;; all** — with it false (CL's default, kept here so existing output is
;; unchanged) printing a value that points back at itself recurses until the
;; process dies. A structure can only become circular through `setf` of a
;; field, e.g.
;;
;;   (defstruct node (val i64) (next Option<node>))
;;   (let ((a (node::new 1 (option::none))))
;;     (setf a::next (option::some a))
;;     (setf *print-circle* true)
;;     (println "~a" a))            ; => #1=#<node 1 (some #1#)>
(pub defvar (*print-circle* bool) false)
(pub defvar (*print-level* i64) 0)
(pub defvar (*print-length* i64) 0)

;; ---------------------------------------------------------------------------
;; `random-state` (CLHS 12.1.6): a mutable PRNG stream. The actual
;; bit-twiddling (a fixed-width xorshift step) lives in Rust — see
;; `eval::interp::xorshift64_step` — since typelisp has no bitwise operators
;; to write it in directly; `make-random-state-fresh`/`random-state-copy`/
;; `random-state-next` (registered in `check::registry::Registry::
;; with_builtins`) are the three native primitives everything below builds on.
;;
;; `*random-state*` is CL's own special variable holding the "current"
;; default stream `random` draws from when no state is given; typelisp has no
;; dynamic binding, so — like every other `*...*` global in this prelude —
;; it's an ordinary assignable one instead, seeded fresh once at prelude load.
(pub defvar (*random-state* random-state) (make-random-state-fresh))

;; CL's `random`: `(random limit &optional random-state)`. Omitting the state
;; draws from (and advances) `*random-state*`; passing one draws from (and
;; advances) that instead.
(pub defun random ((n i32) &optional (state random-state)) i32
  (match state
    ((some s) (random-state-next s n))
    ((none) (random-state-next *random-state* n))))

;; CL's `random-state-p`: always `true` for any argument that type-checks at
;; all — unlike CL, a statically typed `random-state` parameter already rules
;; out every non-`random-state` argument at compile time, so there is nothing
;; left for this to test at run time. Kept only for the CL name/signature.
(pub defun random-state-p ((x random-state)) bool true)

;; CL's `make-random-state`: `(make-random-state &optional state)`. A
;; deliberate simplification of CL's three-way `nil`/`t`/`random-state`
;; argument (a static type can't express that union cleanly) — omitting the
;; argument here means "fresh, entropy-seeded" (CL's `t` case, and the one
;; that's actually useful in practice), not "copy of `*random-state*`" (CL's
;; `nil` case); passing an explicit state still copies it, same as CL.
(pub defun make-random-state (&optional (state random-state)) random-state
  (match state
    ((some s) (random-state-copy s))
    ((none) (make-random-state-fresh))))

;; ---------------------------------------------------------------------------
;; Time (CLHS 25.1). `internal-time-units-per-second` is CL's own constant of
;; that name — the unit `get-internal-real-time` counts in; the value is this
;; implementation's choice (microseconds), not something CL fixes, matching
;; every real CL implementation's own "implementation-defined granularity"
;; latitude.
(pub defvar (internal-time-units-per-second i64) 1000000)

;; CL's `time` macro: run `form`, print how long it took to standard output,
;; and return `form`'s own value unchanged — CL doesn't specify `time`'s
;; report format either, so this prints one real-time-only line (no
;; multiple-value `values`, no separate "run time" figure — CPU time needs an
;; OS-specific call this codebase has no other use for; see docs/dev/
;; cl-missing-classes-and-methods.md §3.4 for the standing "no multiple
;; values" rule this also respects).
(pub defmacro time (form)
  (let ((t0 (gensym)) (result (gensym)))
    `(let ((,t0 (get-internal-real-time)))
       (let ((,result ,form))
         (progn
           (println "Real time: ~,3f seconds"
                     (/ (int->float (- (get-internal-real-time) ,t0))
                        (int->float internal-time-units-per-second)))
           ,result)))))


;; ---------------------------------------------------------------------------
;; Streams (CLHS 21) and files (CLHS 20).
;;
;; Where CL has a class hierarchy this has a *trait* hierarchy, which is the
;; same idea without needing subtyping: `Stream` for what every stream can do,
;; `InputStream`/`OutputStream` for direction, and `CharInput`/`CharOutput`
;; for the character-specific operations. A function that reads characters
;; takes `(where (CharInput S))` or a `:dyn CharInput`, and every stream type
;; -- built-in or user-defined -- fits.
;;
;; Two things this buys that a single `stream` type could not:
;;
;;   * Composite streams are ordinary structs *here*, not variants of a native
;;     enum. `broadcast-stream` below is eight lines and needs no support from
;;     Rust at all, because it just holds `Vector<:dyn CharOutput>`.
;;   * User types are streams. Implement `CharOutput` for your own type and
;;     every function below works on it.
;;
;; The native layer (`eval::stream`) knows only about leaf backends -- files,
;; strings, the three standard streams -- addressed by an opaque `i64` handle.
;; A concrete stream type is a struct holding one, and the field is not `pub`,
;; so handles cannot be forged.
;;
;; NOTE: closing is explicit. A stream is not closed when it becomes garbage
;; (the collector only runs when the cons arena fills, so a finalizer would
;; fire unpredictably or never). Prefer `with-open-file`, which closes for you.

(deftrait Stream ()
  "What every stream can do, whatever it carries and whichever way it goes."
  (open-stream-p ((self Self)) bool)
  (close ((self Self)) ()))

;; Direction. `Item` is left open so a byte stream can pin it to `i32` later
;; without a parallel trait hierarchy; the character layers below pin it.
(deftrait InputStream (Stream)
  "A stream that yields items."
  (type Item)
  (read-item ((self Self)) Option<Item>
    "The next item, or `none` at end of input."))

(deftrait OutputStream (Stream)
  "A stream that accepts items."
  (type Item)
  (write-item ((self Self) (x Item)) ()))

;; The character layers. Pinning `Item` to `char` in the supertrait list is
;; what lets these carry default bodies written in terms of characters --
;; every method below is a default, so implementing `CharInput` for a type
;; that already implements `InputStream` with `Item = char` costs one line.
(deftrait CharInput ((InputStream (Item char)))
  "A character input stream. Every method has a default body."
  (read-char ((self Self)) Option<char>
    "The next character, or `none` at end of input."
    (read-item self))
  (read-line ((self Self)) Option<string>
    "Up to (and consuming) the next newline. `none` only at end of input, so
     a final line with no newline is still returned."
    (match (read-item self)
      ((none) (option::none))
      ((some first)
       (let ((out "") (c first) (going true))
         (while going
           (if (equal c #\newline)
               (progn (setf going false) ())
               (progn
                 (setf out (append out (char->string c)))
                 (match (read-item self)
                   ((none) (progn (setf going false) ()))
                   ((some next) (progn (setf c next) ()))))))
         (option::some out)))))
  (read-all ((self Self)) string
    "Everything left, as one string."
    (let ((out ""))
      (loop
        (match (read-item self)
          ((none) (break))
          ((some c) (setf out (append out (char->string c))))))
      out)))

(deftrait CharOutput ((OutputStream (Item char)))
  "A character output stream. Every method has a default body."
  (write-char ((self Self) (c char)) ()
    (write-item self c))
  (write-string ((self Self) (s string)) ()
    "Write every character of `s`."
    (let ((i 0) (n (length s)))
      (while (< i n)
        (write-item self (ref s i))
        (setf i (+ i 1)))))
  (terpri ((self Self)) ()
    "Write a newline. CL's name for it."
    (write-item self #\newline))
  (write-line ((self Self) (s string)) ()
    "Write `s` followed by a newline."
    (progn (write-string self s) (write-item self #\newline)))
  (finish-output ((self Self)) ()
    "Push buffered output to its destination. A no-op unless overridden."
    ()))

;; ---------------------------------------------------------------------------
;; The native-backed stream types. Each is a struct around one handle; the
;; field is deliberately not `pub`.

(pub defstruct file-stream (h i64))
(pub defstruct string-input-stream (h i64))
(pub defstruct string-output-stream (h i64))
(pub defstruct standard-stream (h i64))

(impl Error FileError
  (message ((self Self)) string (match self ((FileError m) m))))

;; `unwrap-io` turns the native layer's `Result` into a panic, for the
;; operations whose failure means the program is already broken (writing to a
;; closed stream, a handle that is not what it claims). Operations whose
;; failure is ordinary -- opening a file, deleting one -- return `Result` to
;; the caller instead and never come through here.
(defun unwrap-io<T> ((r Result<T, FileError>)) T
  (match r
    ((ok v) v)
    ((err e) (panic (message e)))))

(impl Stream file-stream
  (open-stream-p ((self Self)) bool (stream-open-p self::h))
  (close ((self Self)) () (unwrap-io (stream-close self::h))))
(impl InputStream file-stream
  (type Item char)
  (read-item ((self Self)) Option<char> (unwrap-io (stream-read-char self::h))))
(impl OutputStream file-stream
  (type Item char)
  (write-item ((self Self) (c char)) ()
    (unwrap-io (stream-write-string self::h (char->string c)))))
(impl CharInput file-stream)
(impl CharOutput file-stream
  ;; Overridden: one native call per string beats one per character, and this
  ;; is the stream people write bulk output to.
  (write-string ((self Self) (s string)) ()
    (unwrap-io (stream-write-string self::h s)))
  (finish-output ((self Self)) () (unwrap-io (stream-finish-output self::h))))

(impl Stream string-input-stream
  (open-stream-p ((self Self)) bool (stream-open-p self::h))
  (close ((self Self)) () (unwrap-io (stream-close self::h))))
(impl InputStream string-input-stream
  (type Item char)
  (read-item ((self Self)) Option<char> (unwrap-io (stream-read-char self::h))))
(impl CharInput string-input-stream)

(impl Stream string-output-stream
  (open-stream-p ((self Self)) bool (stream-open-p self::h))
  (close ((self Self)) () (unwrap-io (stream-close self::h))))
(impl OutputStream string-output-stream
  (type Item char)
  (write-item ((self Self) (c char)) ()
    (unwrap-io (stream-write-string self::h (char->string c)))))
(impl CharOutput string-output-stream
  (write-string ((self Self) (s string)) ()
    (unwrap-io (stream-write-string self::h s))))

(impl Stream standard-stream
  (open-stream-p ((self Self)) bool (stream-open-p self::h))
  (close ((self Self)) () (unwrap-io (stream-close self::h))))
(impl InputStream standard-stream
  (type Item char)
  (read-item ((self Self)) Option<char> (unwrap-io (stream-read-char self::h))))
(impl OutputStream standard-stream
  (type Item char)
  (write-item ((self Self) (c char)) ()
    (unwrap-io (stream-write-string self::h (char->string c)))))
(impl CharInput standard-stream)
(impl CharOutput standard-stream
  (write-string ((self Self) (s string)) ()
    (unwrap-io (stream-write-string self::h s)))
  (finish-output ((self Self)) () (unwrap-io (stream-finish-output self::h))))

;; CL's standard streams. Ordinary assignable globals, not dynamically bound
;; specials (typelisp has no dynamic binding) -- `(setf *standard-output* s)`
;; does globally what CL's `(let ((*standard-output* s)) ...)` does locally.
(pub defvar (*standard-input*  standard-stream) (standard-stream::new (stream-stdin)))
(pub defvar (*standard-output* standard-stream) (standard-stream::new (stream-stdout)))
(pub defvar (*error-output*    standard-stream) (standard-stream::new (stream-stderr)))

;; ---------------------------------------------------------------------------
;; Composite streams. These are the whole argument for the trait design: each
;; is a struct and one method, with no native support whatsoever.

(pub defstruct broadcast-stream (parts Vector<:dyn CharOutput>))
(impl Stream broadcast-stream
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () (doiter (p (iter self::parts)) (close p))))
(impl OutputStream broadcast-stream
  (type Item char)
  (write-item ((self Self) (c char)) ()
    (doiter (p (iter self::parts)) (write-char p c))))
(impl CharOutput broadcast-stream
  (write-string ((self Self) (s string)) ()
    (doiter (p (iter self::parts)) (write-string p s)))
  (finish-output ((self Self)) ()
    (doiter (p (iter self::parts)) (finish-output p))))

(pub defstruct two-way-stream (in :dyn CharInput) (out :dyn CharOutput))
(impl Stream two-way-stream
  (open-stream-p ((self Self)) bool (open-stream-p self::in))
  (close ((self Self)) () (progn (close self::in) (close self::out))))
(impl InputStream two-way-stream
  (type Item char)
  (read-item ((self Self)) Option<char> (read-char self::in)))
(impl OutputStream two-way-stream
  (type Item char)
  (write-item ((self Self) (c char)) () (write-char self::out c)))
(impl CharInput two-way-stream)
(impl CharOutput two-way-stream
  (write-string ((self Self) (s string)) () (write-string self::out s))
  (finish-output ((self Self)) () (finish-output self::out)))

;; Reads from `in`, echoing every character actually read to `out`.
(pub defstruct echo-stream (in :dyn CharInput) (out :dyn CharOutput))
(impl Stream echo-stream
  (open-stream-p ((self Self)) bool (open-stream-p self::in))
  (close ((self Self)) () (progn (close self::in) (close self::out))))
(impl InputStream echo-stream
  (type Item char)
  (read-item ((self Self)) Option<char>
    (match (read-char self::in)
      ((some c) (progn (write-char self::out c) (option::some c)))
      ((none) (option::none)))))
(impl CharInput echo-stream)

;; Reads through the components in order; each one's end of input advances to
;; the next rather than ending the stream.
(pub defstruct concatenated-stream (parts Vector<:dyn CharInput>) (at i32))
(impl Stream concatenated-stream
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () (doiter (p (iter self::parts)) (close p))))
(impl InputStream concatenated-stream
  (type Item char)
  (read-item ((self Self)) Option<char>
    (let ((answer (the Option<char> (option::none))) (going true))
      (while going
        (if (>= self::at (len self::parts))
            (progn (setf going false) ())
            (match (read-char (get self::parts self::at))
              ((some c) (progn (setf answer (option::some c)) (setf going false) ()))
              ((none) (progn (setf self::at (+ self::at 1)) ())))))
      answer)))
(impl CharInput concatenated-stream)

;; ---------------------------------------------------------------------------
;; Constructors and the CL-shaped surface.

(pub defun make-string-input-stream ((s string)) string-input-stream
  "A stream that yields the characters of `s`."
  (string-input-stream::new (stream-string-input s)))

(pub defun make-string-output-stream () string-output-stream
  "A stream that accumulates what is written to it; drain it with
   `get-output-stream-string`."
  (string-output-stream::new (stream-string-output)))

(pub defun get-output-stream-string ((s string-output-stream)) string
  "Everything written to `s` since the last call, clearing it."
  (unwrap-io (stream-take-output-string s::h)))

(pub defun make-broadcast-stream ((parts Vector<:dyn CharOutput>)) broadcast-stream
  "A stream that writes to every component, in order."
  (broadcast-stream::new parts))

(pub defun make-two-way-stream ((in :dyn CharInput) (out :dyn CharOutput)) two-way-stream
  (two-way-stream::new in out))

(pub defun make-echo-stream ((in :dyn CharInput) (out :dyn CharOutput)) echo-stream
  (echo-stream::new in out))

(pub defun make-concatenated-stream ((parts Vector<:dyn CharInput>)) concatenated-stream
  (concatenated-stream::new parts 0))

;; `open`'s direction, as named constants rather than keywords: typelisp has
;; no keyword arguments on a `defun` that a `&key` would make nicer here, and
;; three constants read at least as well as `:direction :output`.
(pub defconstant (direction-input i64) 0)
(pub defconstant (direction-output i64) 1)
(pub defconstant (direction-append i64) 2)

(defun io-ok<T> ((v T)) Result<T, FileError>
  "`(result::ok v)` with the error type pinned. Written as a function so the
   declared return type supplies `FileError`: an `ok` arm on its own leaves
   `E` open, and a `match` that has no expected type cannot recover it from
   the sibling `err` arm."
  (result::ok v))

(pub defun open-file ((name string) (direction i64)) Result<file-stream, FileError>
  "Open `name`, one of `direction-input` / `direction-output` /
   `direction-append`. `Err` if the file cannot be opened -- a missing file is
   an ordinary outcome, not a panic."
  (match (stream-open-file name direction)
    ((ok h) (io-ok (file-stream::new h)))
    ((err e) (result::err e))))

(pub defun open-input ((name string)) Result<file-stream, FileError>
  (open-file name direction-input))

(pub defun open-output ((name string)) Result<file-stream, FileError>
  (open-file name direction-output))

;; CL's `input-stream-p`/`output-stream-p` have no counterpart here, and do
;; not need one: direction is part of the type. A value that can be read from
;; implements `CharInput`, so the question is settled where the value is
;; bound, not asked again at run time.

;; `fresh-line`: a newline only if the stream is not already at the start of a
;; line. Only the native-backed streams track a column, so this takes the
;; concrete type rather than the trait.
(pub defun fresh-line ((s file-stream)) ()
  (if (unwrap-io (stream-at-line-start s::h)) () (write-char s #\newline)))

;; ---------------------------------------------------------------------------
;; Convenience over the traits. These are ordinary generic functions -- the
;; point of the trait layer is that they need no cases.

(pub defun write-lines<S,I> ((s S) (lines I)) () (where (CharOutput S) (Iter I (Item string)))
  "Write each of `lines`, one per line."
  (doiter (l lines) (write-line s l)))

(pub defun copy-stream<I,O> ((from I) (to O)) () (where (CharInput I) (CharOutput O))
  "Drain `from` into `to`."
  (loop
    (match (read-item from)
      ((none) (break))
      ((some c) (write-char to c)))))

(pub defun read-lines<S> ((s S)) Vector<string> (where (CharInput S))
  "Every remaining line of `s`."
  (let ((out (the Vector<string> (Vector::new))))
    (loop
      (match (read-line s)
        ((none) (break))
        ((some l) (push out l))))
    out))

;; ---------------------------------------------------------------------------
;; The `with-...` macros, which are the reason `close` rarely appears in user
;; code. Each binds the stream, runs the body, then closes -- note the body's
;; value is produced *before* the close, so `(with-open-file ...)` still
;; returns something useful.

(pub defmacro with-open-file (spec &rest body)
  "`(with-open-file (var name direction) body...)` -- open, run the body, close.
   Yields `Result<body-value, FileError>`: opening can fail, and the body's
   value is produced before the close so it is still useful."
  (let ((var (sexpr-car spec))
        (name (sexpr-car (sexpr-cdr spec)))
        (direction (sexpr-car (sexpr-cdr (sexpr-cdr spec))))
        (s (gensym))
        (e (gensym))
        (result (gensym)))
    `(match (open-file ,name ,direction)
       ((ok ,s)
        (let ((,var ,s))
          (let ((,result (progn ,@body)))
            (progn (close ,var) (io-ok ,result)))))
       ((err ,e) (result::err ,e)))))

(pub defmacro with-input-from-string (spec &rest body)
  "`(with-input-from-string (var string) body...)`."
  (let ((var (sexpr-car spec)) (text (sexpr-car (sexpr-cdr spec))))
    `(let ((,var (make-string-input-stream ,text)))
       (let ((r (progn ,@body)))
         (progn (close ,var) r)))))

(pub defmacro with-output-to-string (spec &rest body)
  "`(with-output-to-string (var) body...)` -- returns everything written."
  (let ((var (sexpr-car spec)))
    `(let ((,var (make-string-output-stream)))
       (progn ,@body (get-output-stream-string ,var)))))

;; ---------------------------------------------------------------------------
;; Whole-file convenience (CLHS 20 is otherwise mostly pathnames, which this
;; language does not have -- a file is named by a string).

(pub defun read-file-string ((name string)) Result<string, FileError>
  "The entire contents of `name`."
  (match (open-input name)
    ((ok s) (let ((text (read-all s))) (progn (close s) (io-ok text))))
    ((err e) (result::err e))))

(pub defun read-file-lines ((name string)) Result<Vector<string>, FileError>
  (match (open-input name)
    ((ok s) (let ((ls (read-lines s))) (progn (close s) (io-ok ls))))
    ((err e) (result::err e))))

(pub defun write-file-string ((name string) (text string)) Result<(), FileError>
  "Write `text` to `name`, replacing it. `Ok(())` on success -- the write is
   done for its effect, so there is no value to carry back."
  (match (open-output name)
    ((ok s) (progn (write-string s text) (close s) (io-ok ())))
    ((err e) (result::err e))))

(pub defun probe-file ((name string)) bool
  "Whether `name` exists."
  (file-exists-p name))

(pub defun delete-file ((name string)) Result<(), FileError>
  "Remove `name`. `Ok(())` on success."
  (match (file-delete name)
    ((ok _) (io-ok ()))
    ((err e) (result::err e))))

(pub defun rename-file ((from string) (to string)) Result<(), FileError>
  "Rename `from` to `to`. `Ok(())` on success."
  (match (file-rename from to)
    ((ok _) (io-ok ()))
    ((err e) (result::err e))))


"#;

/// Read, check, and execute [`SOURCE`] against `heap`/`chk`/`interp`,
/// registering its definitions exactly as if the caller had typed them
/// first. Must be called before any user source that references a prelude
/// name.
///
/// This is the pure source path — no fasl cache, so it stays hermetic (tests
/// and any embedding depend only on `SOURCE`, never on `~/.cache` state). The
/// binaries opt into the on-disk cache via [`load_cached`]/[`prelude_fasl`].
pub fn load(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) {
    let r = Reader::new();
    let forms = r.read_all(heap, SOURCE).expect("prelude: read failed");
    chk.predeclare_program(heap, &forms);
    for v in forms {
        let tl = chk.check_form(heap, &*interp, v).expect("prelude: check failed");
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        interp.exec(heap, tl).expect("prelude: eval failed");
    }
}

/// Like [`load`], but backed by the fasl (compiled-module) cache: a fresh
/// `prelude-<hash>-v<version>.fasl` under the user cache directory is loaded
/// directly ([`Fasl::load_into`] — no reading/typechecking). On a miss the
/// source is checked as usual and the result cached (best-effort) for next
/// time. The resulting `heap`/`chk`/`interp` are identical to [`load`]'s (see
/// `tests/fasl_test.rs`'s equivalence tests) — the cache only removes work,
/// never changes the outcome. For the CLI/REPL, whose startup pays this cost
/// every run.
pub fn load_cached(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) {
    if let Some(fasl) = cached_fasl() {
        fasl.load_into(heap, chk, interp).expect("prelude: fasl load failed");
        return;
    }
    let fasl = source_load_capturing(heap, chk, interp);
    write_cache(&fasl);
}

/// The prelude as a [`Fasl`], for a caller (the LSP) that reconstructs a
/// fresh prelude-loaded environment many times and wants to pay the
/// read/typecheck cost only once. Cache-hit returns the stored fasl; a miss
/// builds one in a throwaway environment, caches it, and returns it.
pub fn prelude_fasl() -> Fasl {
    if let Some(fasl) = cached_fasl() {
        return fasl;
    }
    let mut heap = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let fasl = source_load_capturing(&mut heap, &mut chk, &mut interp);
    write_cache(&fasl);
    fasl
}

/// Source-loads the prelude into the given (empty) environment and returns a
/// [`Fasl`] capturing exactly what it added — the shared cache-miss path of
/// [`load`]/[`prelude_fasl`].
fn source_load_capturing(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) -> Fasl {
    let mark = registry_mark(chk);
    let r = Reader::new();
    let forms = r.read_all(heap, SOURCE).expect("prelude: read failed");
    chk.predeclare_program(heap, &forms);
    let mut top_levels = Vec::new();
    for v in forms {
        let tl = chk.check_form(heap, &*interp, v).expect("prelude: check failed");
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        interp.exec(heap, tl.clone()).expect("prelude: eval failed");
        top_levels.push(tl);
    }
    Fasl::capture(heap, chk, &mark, top_levels, source_hash(SOURCE)).expect("prelude: fasl capture failed")
}

/// The prelude fasl's cache path:
/// `<cache-dir>/prelude-<hash>-v<ver>.fasl`, where `<cache-dir>` is
/// `$TYPL_CACHE_DIR` if set, else `$HOME/.typl/cache`. A *dedicated* typelisp
/// directory — deliberately **not** the shared XDG `~/.cache`, so clearing
/// another app's caches (or a blanket `rm -rf ~/.cache/*`) can't take
/// typelisp's with it. Resolved at runtime, never hardcoded
/// ([[feedback-no-hardcoded-absolute-paths]]). `None` if neither is set (no
/// cache used — the source path still works).
fn cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("TYPL_CACHE_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".typl").join("cache")))?;
    Some(base.join(format!("prelude-{:016x}-v{}.{}", source_hash(SOURCE), FASL_FORMAT_VERSION, FASL_EXTENSION)))
}

/// Loads the cached prelude fasl if present and valid (its `source_hash`
/// matches this build's [`SOURCE`]). Any failure — missing file, unreadable,
/// parse error, version/hash mismatch — is a silent miss.
fn cached_fasl() -> Option<Fasl> {
    let path = cache_path()?;
    let bytes = std::fs::read(&path).ok()?;
    let fasl = Fasl::from_bytes(&bytes).ok()?;
    (fasl.source_hash == source_hash(SOURCE)).then_some(fasl)
}

/// Writes `fasl` to the cache path (best-effort: a read-only or unwritable
/// cache directory just means the next load re-checks the source).
fn write_cache(fasl: &Fasl) {
    if let Some(path) = cache_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = fasl.to_bytes() {
            let _ = std::fs::write(&path, bytes);
        }
    }
}
