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

;; `dolist` (iterating a `Sexpr` list in user code) was removed with the rest
;; of the user-facing `Sexpr` list surface (Symbol/Sexpr redesign Phase 5).
;; `doiter` over `Vector<T>`/any `Iter` is the homogeneous-collection loop; a
;; `Sexpr`/`cons<T,U>` traversal API is to be redesigned later.

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
;; `Vector` because an `Iter` is forward-only. Every element-comparing
;; operation (`find`/`position`/`count`/`remove-if`) takes a predicate, not an
;; element: `A` carries no `Eq`-style bound, so there is no generic equality —
;; the predicate form mirrors CL's `-if` family. A freshly built result
;; vector is pinned with `the`, since a bare `(Vector::new)` has nothing to
;; infer its element type from.
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
(defun find<I,A> ((it I) (pred (fn (A) bool))) Option<A> (where (Iter I (Item A)))
  (let ((result (the Option<A> (Option::none))))
    (doiter (x it)
      (if (pred x) (progn (setf result (Option::some x)) (break)) ()))
    result))
(defun position<I,A> ((it I) (pred (fn (A) bool))) Option<i32> (where (Iter I (Item A)))
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
(defun count<I,A> ((it I) (pred (fn (A) bool))) i32 (where (Iter I (Item A)))
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
;; `member`/`assoc`/`sort` below can require `(where (Eq A))`/`(where (Ord A))`
;; instead of taking a predicate (the way `find`/`position`/`count`/`remove-if`
;; do — those keep their predicate form, mirroring CL's `-if` family).
;;
;; Modelled on Rust's `PartialEq`/`PartialOrd`, kept under the names `Eq`/`Ord`
;; (renaming would churn every `where (Ord T)`/`(Eq T)` bound below). `Eq` is
;; Rust's `PartialEq` (`equals`=`==`, `not-equals`=`!=`); `Ord` is Rust's
;; `PartialOrd` (`less`=`<`, `less-equal`=`<=`, `greater`=`>`, `greater-equal`=
;; `>=`). typelisp `deftrait` has no default method bodies, so each impl spells
;; out every method — the scalar impls all delegate uniformly to the builtin
;; per-type operators (`= /= < <= > >=`, `equal`/`eq`, all overloaded by
;; receiver type, see `registry.rs`). Method names avoid the builtin operator/
;; `eq`/`lt` names on purpose: builtins cannot be redefined, and `?`/`!` name
;; suffixes are banned project-wide (so no `eq?`/`less?`).
(deftrait Eq
  (equals ((self Self) (other Self)) bool)
  (not-equals ((self Self) (other Self)) bool))
(deftrait Ord
  (less ((self Self) (other Self)) bool)
  (less-equal ((self Self) (other Self)) bool)
  (greater ((self Self) (other Self)) bool)
  (greater-equal ((self Self) (other Self)) bool))

;; Scalar `Eq` impls. Numbers delegate to `=`/`/=`; `string`/`char`/`bool` to
;; `equal` (content comparison — a `string`'s `eq` is `Rc` identity, which
;; would make `(member "x" ...)` fail on separately built equal-content
;; strings); `symbol` to `eq` (interned, identity *is* content equality).
;; `not-equals` negates whichever equality the type uses. Only the seven scalar
;; types with a usable comparison builtin get an impl — `f32`/`i8`/`i16`/`u*`
;; have empty assoc tables (no `=`/`<` to delegate to), so they stay outside
;; `Eq`/`Ord` until they grow real arithmetic.
(impl Eq i32    (equals ((self Self) (other Self)) bool (= self other))  (not-equals ((self Self) (other Self)) bool (/= self other)))
(impl Eq i64    (equals ((self Self) (other Self)) bool (= self other))  (not-equals ((self Self) (other Self)) bool (/= self other)))
(impl Eq f64    (equals ((self Self) (other Self)) bool (= self other))  (not-equals ((self Self) (other Self)) bool (/= self other)))
(impl Eq bignum (equals ((self Self) (other Self)) bool (= self other))  (not-equals ((self Self) (other Self)) bool (/= self other)))
(impl Eq ratio  (equals ((self Self) (other Self)) bool (= self other))  (not-equals ((self Self) (other Self)) bool (/= self other)))
(impl Eq bool   (equals ((self Self) (other Self)) bool (equal self other)) (not-equals ((self Self) (other Self)) bool (not (equal self other))))
(impl Eq char   (equals ((self Self) (other Self)) bool (equal self other)) (not-equals ((self Self) (other Self)) bool (not (equal self other))))
(impl Eq string (equals ((self Self) (other Self)) bool (equal self other)) (not-equals ((self Self) (other Self)) bool (not (equal self other))))
(impl Eq symbol (equals ((self Self) (other Self)) bool (eq self other))    (not-equals ((self Self) (other Self)) bool (not (eq self other))))

;; Scalar `Ord` impls. Every scalar now has the overloaded `< <= > >=` builtins
;; (numbers always had them; `char`/`string` gained them alongside these
;; expanded traits — code-point / lexicographic order), so all five delegate
;; uniformly. `bool`/`symbol` carry no meaningful order, so no `Ord` for them.
(impl Ord i32    (less ((self Self) (other Self)) bool (< self other)) (less-equal ((self Self) (other Self)) bool (<= self other)) (greater ((self Self) (other Self)) bool (> self other)) (greater-equal ((self Self) (other Self)) bool (>= self other)))
(impl Ord i64    (less ((self Self) (other Self)) bool (< self other)) (less-equal ((self Self) (other Self)) bool (<= self other)) (greater ((self Self) (other Self)) bool (> self other)) (greater-equal ((self Self) (other Self)) bool (>= self other)))
(impl Ord f64    (less ((self Self) (other Self)) bool (< self other)) (less-equal ((self Self) (other Self)) bool (<= self other)) (greater ((self Self) (other Self)) bool (> self other)) (greater-equal ((self Self) (other Self)) bool (>= self other)))
(impl Ord bignum (less ((self Self) (other Self)) bool (< self other)) (less-equal ((self Self) (other Self)) bool (<= self other)) (greater ((self Self) (other Self)) bool (> self other)) (greater-equal ((self Self) (other Self)) bool (>= self other)))
(impl Ord ratio  (less ((self Self) (other Self)) bool (< self other)) (less-equal ((self Self) (other Self)) bool (<= self other)) (greater ((self Self) (other Self)) bool (> self other)) (greater-equal ((self Self) (other Self)) bool (>= self other)))
(impl Ord char   (less ((self Self) (other Self)) bool (< self other)) (less-equal ((self Self) (other Self)) bool (<= self other)) (greater ((self Self) (other Self)) bool (> self other)) (greater-equal ((self Self) (other Self)) bool (>= self other)))
(impl Ord string (less ((self Self) (other Self)) bool (< self other)) (less-equal ((self Self) (other Self)) bool (<= self other)) (greater ((self Self) (other Self)) bool (> self other)) (greater-equal ((self Self) (other Self)) bool (>= self other)))

;; `cons-cell<A,B>`'s Eq/Ord: recursive (structural) comparison. The field
;; comparisons go through the methods' own `where` bounds — checked as
;; diagnostics-only `TraitCall`s while `A`/`B` are open, then resolved to the
;; field types' real `equals`/`less` when the owner monomorphizes (nesting,
;; e.g. `cons-cell<cons-cell<i32,i32>,i32>`, just recurses one level per
;; instantiation). Note the bound names must be the *owner's* declared type
;; parameters (`A`/`B`, exactly as written on the `defstruct`) — a method's
;; signature can only be generic through them.
(impl Eq cons-cell<A,B>
  (equals ((self Self) (other Self)) bool (where (Eq A) (Eq B))
    (if (equals self::car other::car)
        (equals self::cdr other::cdr)
        false))
  ;; `not-equals` derives from `equals` (Rust's `PartialEq::ne` default).
  (not-equals ((self Self) (other Self)) bool (where (Eq A) (Eq B))
    (not (equals self other))))

;; Ord: lexicographic, `car` first, then `cdr`. The double-`less` form (car
;; strictly less → true; car strictly greater → false; otherwise cars are
;; equivalent, compare cdrs) deliberately avoids requiring `(Eq A)` on top of
;; `(Ord A)` — ordering alone decides equivalence, so `Ord`-only element
;; types still qualify.
(impl Ord cons-cell<A,B>
  (less ((self Self) (other Self)) bool (where (Ord A) (Ord B))
    (if (less self::car other::car)
        true
        (if (less other::car self::car)
            false
            (less self::cdr other::cdr))))
  ;; `less-equal`/`greater`/`greater-equal` derive from the total-order `less`
  ;; (Rust's `PartialOrd` defaults): `a>b ⇔ b<a`, `a<=b ⇔ ¬(b<a)`, `a>=b ⇔ ¬(a<b)`.
  (greater ((self Self) (other Self)) bool (where (Ord A) (Ord B))
    (less other self))
  (less-equal ((self Self) (other Self)) bool (where (Ord A) (Ord B))
    (not (less other self)))
  (greater-equal ((self Self) (other Self)) bool (where (Ord A) (Ord B))
    (not (less self other))))

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
;; `member`/`sort`/`assoc` require `Eq`/`Ord` (defined below with the scalar
;; impls) instead of taking a predicate — the trait-bounded halves of the
;; library. Predicate variants of the same searches already exist above
;; (`find`/`position`/`count`).
(defun member<I,A> ((x A) (it I)) bool (where (Iter I (Item A)) (Eq A))
  (let ((found false))
    (doiter (y it)
      (if (equals y x) (progn (setf found true) (break)) ()))
    found))
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
;; Non-destructive insertion sort, stable: the inner shift uses strict
;; `less`, so equal elements keep their input order. Ascending.
(defun sort<I,A> ((it I)) Vector<A> (where (Iter I (Item A)) (Ord A))
  (let ((out (the Vector<A> (Vector::new))))
    (doiter (x it)
      (let ((j (len out)))
        (push out x)
        (while (if (> j 0) (less x (get out (- j 1))) false)
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
