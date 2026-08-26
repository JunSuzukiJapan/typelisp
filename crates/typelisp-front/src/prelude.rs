//! The typelisp prelude: library functions written in typelisp itself
//! rather than Rust, following [language-design.md](../docs/dev/language-design.md)
//! §4's split — anything expressible purely as a combination of existing
//! primitives belongs here, not in the checker/interpreter.
//!
//! There is no automatic "every program gets this for free" loading yet:
//! callers (the REPL, or a test harness) must explicitly load it once,
//! against the same [`Heap`]/[`Checker`]/[`Interp`] the rest of the program
//! will use, before processing any user source. The prelude source is fixed
//! and known-good, so a failure here is a bug in this file, not a user
//! error — [`load_interpreted`] panics rather than threading a `Result`
//! callers would have no real recovery from.
//!
//! What lives here is source text and the interpreted load of it. The
//! committed *precompiled* bodies, and installing them over the definitions
//! this load registers, belong to the backend
//! (`crate::compile::prelude_bootstrap`, whose `load` is what
//! `typelisp::load_prelude` names) — the prelude is a front-end asset, its
//! machine code is not.

use crate::{Checker, Heap, Interp, Reader, Value};

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
/// A helper a later definition depends on is placed earlier in this string,
/// and that is a necessity, not a convention: a top-level `defun` may only
/// call a name already seen. This file needs no `defsignature` anywhere — it
/// has no mutual recursion at top level, which is worth keeping true.
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
/// The committed prelude dump: the checked state these definitions produce,
/// paired with the bitcode holding their native bodies. Written by
/// `scripts/regen-prelude-bitcode.sh`, applied by
/// `typelisp::compile::prelude_bootstrap::load`.
///
/// It lives in *this* crate, not the backend's, because an AOT executable
/// links only `typelisp-front`: the checked-state half is what its `eval` needs
/// to have the prelude in scope, and it cannot reach a file the backend owns.
/// The bitcode half is opaque here — nothing in this crate parses it.
pub const DUMP: &[u8] = include_bytes!("prelude.typld");

/// What to run when [`DUMP`] no longer matches [`SOURCE`]. Lives here, next to
/// both, so the backend's loader and the front end's `eval`-environment builder
/// name the same script.
pub const REGEN_SCRIPT: &str = "scripts/regen-prelude-bitcode.sh";

/// Every source [`DUMP`] was built from, in load order.
///
/// Two, not one: the dump carries the core macro layer's definitions
/// ([`crate::core_macros::SOURCE`]) as well as this module's, because that
/// layer loads as part of the prelude load and a from-dump start would
/// otherwise come up with no `cond`/`case`. Staleness is judged against both
/// ([`crate::dump::verify_sources_digest`]) — hashing only `SOURCE` would let
/// an edit to the core layer ship against a dump that no longer matches it.
pub const DUMPED_SOURCES: &[&str] = &[crate::core_macros::SOURCE, SOURCE];

pub const SOURCE: &str = r##"
;; `not`: moved here from a Rust builtin (it has no dependency on the GC
;; heap or anything else Rust-only — a plain `if`/`bool` round trip) so it
;; compiles through the ordinary `core_bridge`/`compiler.rs` pipeline like any
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


;; ---------------------------------------------------------------------------
;; `gensym` (cl-parity-plan.md Phase 4c).
;;
;; A prelude function, not a Rust builtin, so that CL's `*gensym-counter*` can
;; be a real variable a program can read and set — a builtin's counter lived on
;; the `Heap` and nothing could name it. The interpreted and compiled paths
;; still share one sequence, because they now share this one definition and its
;; one global.
;;
;; The name starts with a space, which no source can write, so a generated
;; binding cannot collide with a written one. That is the whole guarantee:
;; symbols here are always interned, so two `gensym`s are distinct because
;; their *names* differ, not because they are separate uninterned objects as in
;; CL. Uninterned symbols would buy nothing — every place a symbol acts as a
;; binder is keyed by name (`Env::vars`), so two same-named uninterned symbols
;; would collide exactly where it matters most.
(pub defvar (*gensym-counter* i32) 0)

(pub defun gensym (&optional (prefix string "g")) Symbol
  "A fresh symbol, named from `prefix` and `*gensym-counter*`. CL's `gensym`."
  (let ((n *gensym-counter*))
    (progn
      (setf *gensym-counter* (+ n 1))
      (string->symbol (format false " ~a~a" prefix n)))))

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

;; `cond`: nested `if`s, one clause peeled off per recursive expansion (same
;; self-recursion shape as `and`/`or` above). `else`-detection mirrors
;; `case`'s own `(eq (car c) (quote else))`.

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

;; CL's `complement`: the predicate that answers the opposite. `const` above
;; is CL's `constantly` applied to its argument — CL's `constantly` itself
;; (the *function* that ignores its arguments) is not here, because the type
;; of the argument it ignores appears only in the return type, and this
;; checker resolves a type parameter from the *arguments* (there is no
;; explicit type application either). `(lambda ((x T)) A v)` says it inline in
;; the same space.
(pub defun complement<A> ((pred (fn (A) bool))) (fn (A) bool)
  "The predicate that answers what `pred` does not."
  (lambda ((x A)) bool (not (pred x))))
(defun compose<A,B,C> ((f (fn (B) C)) (g (fn (A) B))) (fn (A) C)
  (lambda ((x A)) C (f (g x))))
(defun flip<A,B,C> ((f (fn (A B) C))) (fn (B A) C)
  (lambda ((y B) (x A)) C (f x y)))

;; CL's `keywordp`: is this symbol a keyword (`:name`)? Written as a plain
;; typelisp `defun` over `symbol->string` rather than a Rust builtin — the
;; leading colon *is* part of the interned name (there is no separate keyword
;; package/table, see `sym`), so the test is textual and needs no
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

;; CL's character catalog beyond the primitives in `registry::char_assoc`
;; (`upcase`/`downcase`/`<`/`alphap`/`digitp`/`char->int`/`equalp`).
;; cl-parity-plan.md Phase 2a.
;;
;; Naming follows this file's existing character/predicate convention rather
;; than CL's spelling: the receiver's static type already says "char", so
;; `char-lessp`/`upper-case-p`/`char-name`/`digit-char` become `lessp`/
;; `upper-casep`/`char->name`/`digit->char` — the same collapse
;; `alpha-char-p`->`alphap` and `zerop`/`evenp` already made, and the same
;; `char->int`/`char->string` conversion spelling this type already carries.
;; `docs/functions.md` §9 lists the CL correspondence for each.
;;
;; EVERY body below works on `char->int` code points and never calls
;; `upcase`/`downcase`/`alphap`/`digitp`/`int->char`. That is not style: those
;; five are builtins the island has no lowering for (`externs::
;; native_lowered_primitive_methods`'s `"char"` and `"i32"` rows), so reaching
;; one would make this whole section interpreted-only and open a
;; `PRELUDE_COMPILE_UNSUPPORTED` hole — which is exactly what the first draft
;; of this section did, and what that list exists to catch. `char->int`, the
;; comparison operators and `string`'s `ref` *are* lowered, so everything here
;; compiles through the ordinary prelude path. (Closing the lowering gap for
;; those five is separate work; until then this constraint stands.)
;;
;; ASCII-only, like `char_assoc`'s own `upcase`/`alphap`: classifying a
;; non-ASCII code point needs Unicode tables the runtime does not carry.
(defun ascii-alpha-code ((n i32)) bool
  (or (and (>= n 65) (<= n 90)) (and (>= n 97) (<= n 122))))
(defun ascii-digit-code ((n i32)) bool (and (>= n 48) (<= n 57)))
;; Case-folds an upper-case ASCII letter down, leaving everything else alone —
;; the code-point-level half of `char`'s `equalp`, reused by the four
;; case-insensitive order comparisons below.
(defun ascii-downcase-code ((n i32)) i32
  (if (and (>= n 65) (<= n 90)) (+ n 32) n))

;; `char/=`: CL's inequality. `char` had `equal` but no `/=`, so the checker's
;; variadic `/=` sugar (`check_variadic_cmp`, functions.md §4.1) had no binary
;; method to expand onto for characters.
(defmethod /= ((self char) (b char)) bool (not (equal self b)))
;; The case-insensitive order comparisons (`char-lessp` and friends). CL
;; defines them by case-folding both operands, which is what `equalp` on
;; `char` already does for equality — these are its ordering siblings.
(defmethod lessp ((self char) (b char)) bool
  (< (ascii-downcase-code (char->int self)) (ascii-downcase-code (char->int b))))
(defmethod greaterp ((self char) (b char)) bool
  (> (ascii-downcase-code (char->int self)) (ascii-downcase-code (char->int b))))
(defmethod not-lessp ((self char) (b char)) bool
  (>= (ascii-downcase-code (char->int self)) (ascii-downcase-code (char->int b))))
(defmethod not-greaterp ((self char) (b char)) bool
  (<= (ascii-downcase-code (char->int self)) (ascii-downcase-code (char->int b))))
;; Case classification. `both-casep` is CL's `both-case-p`: whether this
;; character has *both* cases, i.e. whether case conversion means anything for
;; it — true for ASCII letters, false for digits and punctuation.
(defmethod upper-casep ((self char)) bool
  (let ((n (char->int self))) (and (>= n 65) (<= n 90))))
(defmethod lower-casep ((self char)) bool
  (let ((n (char->int self))) (and (>= n 97) (<= n 122))))
(defmethod both-casep ((self char)) bool (ascii-alpha-code (char->int self)))
(defmethod alphanumericp ((self char)) bool
  (let ((n (char->int self))) (or (ascii-alpha-code n) (ascii-digit-code n))))
;; `graphic-char-p`: printable, space included, everything else excluded.
(defmethod graphicp ((self char)) bool
  (let ((n (char->int self))) (and (>= n 32) (< n 127))))
;; `standard-char-p`: CL's 96-character standard set — the graphic characters
;; plus newline, and nothing else.
(defmethod standardp ((self char)) bool
  (let ((n (char->int self))) (or (and (>= n 32) (< n 127)) (= n 10))))

;; CL's `digit-char-p`: the character's *weight* in `radix`, or `none` when it
;; is not a digit in that radix. This is the CL-conformant reading of that
;; name; the existing `digitp` (ASCII decimal, `bool`) is deliberately left
;; alone rather than redefined, because this file's own reader
;; (`reader-scan-atom`) and the self-hosting island both call it as a
;; predicate.
;;
;; A free `defun` rather than a `defmethod`, and likewise `digit->char`: CL
;; gives both an optional radix, and `defmethod` accepts neither `&optional`
;; nor `&key` (`parse_defmethod_sig_inner` only calls `parse_param_pairs`)
;; until cl-parity-plan.md Phase 5b lifts that.
(defun digit-weight ((c char) &optional (radix i32 10)) Option<i32>
  (let* ((n (char->int c))
         ;; `-1` means "not a digit character at all", which the range test
         ;; below rejects along with a weight too large for `radix`.
         (w (cond ((ascii-digit-code n) (- n 48))
                  ((and (>= n 97) (<= n 122)) (+ (- n 97) 10))
                  ((and (>= n 65) (<= n 90)) (+ (- n 65) 10))
                  (else -1))))
    (if (and (>= w 0) (< w radix)) (option::some w) (option::none))))
;; CL's `digit-char`: the inverse — the character standing for `weight` in
;; `radix`, or `none` when the weight is out of range. Digits use `0`-`9` and
;; the rest upper-case letters, as CL specifies (so `radix` tops out at 36).
;; Indexing a literal with `string`'s `ref` rather than computing a code point
;; keeps this off `int->char`, per this section's header.
(defun digit->char ((weight i32) &optional (radix i32 10)) Option<char>
  (if (or (< weight 0) (>= weight radix))
      (option::none)
      (option::some (ref "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ" weight))))
;; CL's `char-name`/`name-char`, over exactly the characters this language's
;; reader spells by name (`read::reader`'s named-character table). Anything
;; else gets `none`, which is what CL says for every graphic character.
;; `name->char` accepts the reader's aliases (`linefeed`, `null`) too, and is
;; case-insensitive because the reader's own lookup is.
(defmethod char->name ((self char)) Option<string>
  (cond ((equal self #\newline)   (option::some "newline"))
        ((equal self #\space)     (option::some "space"))
        ((equal self #\tab)       (option::some "tab"))
        ((equal self #\return)    (option::some "return"))
        ((equal self #\page)      (option::some "page"))
        ((equal self #\nul)       (option::some "nul"))
        ((equal self #\backspace) (option::some "backspace"))
        (else (option::none))))
(defun name->char ((name string)) Option<char>
  (cond ((equalp name "space")     (option::some #\space))
        ((equalp name "newline")   (option::some #\newline))
        ((equalp name "linefeed")  (option::some #\newline))
        ((equalp name "tab")       (option::some #\tab))
        ((equalp name "return")    (option::some #\return))
        ((equalp name "page")      (option::some #\page))
        ((equalp name "nul")       (option::some #\nul))
        ((equalp name "null")      (option::some #\nul))
        ((equalp name "backspace") (option::some #\backspace))
        (else (option::none))))

;; CL's string catalog beyond the primitives in `registry::string_assoc`
;; (`upcase`/`downcase`/`length`/`ref`/`substring`/`append`/`equal`/`<`).
;; cl-parity-plan.md Phase 2b.
;;
;; Same naming rule as the character section above — the receiver's static
;; type already says "string", so `string-trim`/`string-capitalize`/
;; `string-lessp`/`make-string` become `trim`/`capitalize`/`lessp`/
;; `string::filled`. And the same compilability rule: nothing here reaches
;; `upcase`/`downcase` (unlowered on both `string` and `char`), so case
;; conversion goes through the two code-point helpers below, which index a
;; literal alphabet with `string`'s `ref` — lowered, unlike `int->char`.
(defun ascii-upcase-char ((c char)) char
  (let ((n (char->int c)))
    (if (and (>= n 97) (<= n 122)) (ref "ABCDEFGHIJKLMNOPQRSTUVWXYZ" (- n 97)) c)))
(defun ascii-downcase-char ((c char)) char
  (let ((n (char->int c)))
    (if (and (>= n 65) (<= n 90)) (ref "abcdefghijklmnopqrstuvwxyz" (- n 65)) c)))

;; `string/=`: CL's inequality, the counterpart of `char`'s `/=` above.
(defmethod /= ((self string) (b string)) bool (not (equal self b)))
;; `string-lessp` and friends: CL's case-insensitive order comparisons.
;; Lexicographic on case-folded code points, with the shorter string first on
;; a common prefix — the same order `<` gives, folded.
(defun string-fold-compare ((a string) (b string)) i32
  "-1/0/1 for a < b / a = b / a > b, comparing case-folded code points."
  (let ((i 0) (n (min (length a) (length b))) (r 0))
    (progn
      (while (and (< i n) (= r 0))
        (let ((x (ascii-downcase-code (char->int (ref a i))))
              (y (ascii-downcase-code (char->int (ref b i)))))
          (progn (if (< x y) (setf r -1) (if (> x y) (setf r 1) 0)) (setf i (+ i 1)))))
      (if (/= r 0) r
          (if (< (length a) (length b)) -1 (if (> (length a) (length b)) 1 0))))))
(defmethod lessp ((self string) (b string)) bool (< (string-fold-compare self b) 0))
(defmethod greaterp ((self string) (b string)) bool (> (string-fold-compare self b) 0))
(defmethod not-lessp ((self string) (b string)) bool (>= (string-fold-compare self b) 0))
(defmethod not-greaterp ((self string) (b string)) bool (<= (string-fold-compare self b) 0))

;; `make-string`: `n` copies of `c`. A static method, so it reads like every
;; other constructor in this language (`Vector::new`, `HashTable::new`) rather
;; than as a `make-*` free function — cl-parity-plan.md §1.1.
(defmethod filled (string (n i32) (c char)) string
  (let ((out "") (i 0))
    (progn
      (while (< i n) (progn (setf out (append out (char->string c))) (setf i (+ i 1))))
      out)))

;; `search`: the index where `sub` first occurs in `self`, or `none`.
;; CL spells the arguments the other way round (`(search pattern sequence)`);
;; this takes the receiver first like every other method here, and
;; `docs/functions.md` §8 records the difference.
;; The empty string occurs at index 0, as CL says.
(defmethod search ((self string) (sub string)) Option<i32>
  (let ((n (length self)) (m (length sub)) (i 0) (found (the Option<i32> (option::none))))
    (progn
      (while (and (<= i (- n m)) (is-none found))
        (progn
          (if (equal (substring self i (+ i m)) sub) (progn (setf found (option::some i)) ()) ())
          (setf i (+ i 1))))
      found)))
;; `mismatch`: the index of the first position where the two differ, or `none`
;; when one is a prefix of the other *and* they are the same length — i.e.
;; `none` exactly when they are `equal`. A length difference mismatches at the
;; shorter one's end, which is CL's answer too.
(defmethod mismatch ((self string) (b string)) Option<i32>
  (let ((n (min (length self) (length b))) (i 0) (found (the Option<i32> (option::none))))
    (progn
      (while (and (< i n) (is-none found))
        (progn
          (if (equal (ref self i) (ref b i)) () (progn (setf found (option::some i)) ()))
          (setf i (+ i 1))))
      (match found
        ((some k) (option::some k))
        ((none) (if (= (length self) (length b)) (option::none) (option::some n)))))))

;; The `string-trim` family. `bag` is the set of characters to strip, spelled
;; as a string (CL takes any character sequence; a string *is* the character
;; sequence this language has). A free `defun` with an `&optional` default of
;; the usual whitespace, because `defmethod` takes neither `&optional` nor
;; `&key` until cl-parity-plan.md Phase 5b.
(defun char-in-bag ((c char) (bag string)) bool
  (let ((i 0) (n (length bag)) (hit false))
    (progn
      (while (and (< i n) (not hit))
        (progn (if (equal (ref bag i) c) (progn (setf hit true) ()) ()) (setf i (+ i 1))))
      hit)))
(defun left-trim ((s string) &optional (bag string " \t\n\r")) string
  (let ((i 0) (n (length s)))
    (progn
      (while (and (< i n) (char-in-bag (ref s i) bag)) (setf i (+ i 1)))
      (substring s i n))))
(defun right-trim ((s string) &optional (bag string " \t\n\r")) string
  (let ((j (length s)))
    (progn
      (while (and (> j 0) (char-in-bag (ref s (- j 1)) bag)) (setf j (- j 1)))
      (substring s 0 j))))
(defun trim ((s string) &optional (bag string " \t\n\r")) string
  (right-trim (left-trim s bag) bag))

;; `string-capitalize`: each word's first character up, the rest down, where a
;; word is a maximal run of alphanumerics — CL's own definition.
(defmethod capitalize ((self string)) string
  (let ((out "") (i 0) (n (length self)) (in-word false))
    (progn
      (while (< i n)
        (let ((c (ref self i)))
          (progn
            (if (alphanumericp c)
                (progn
                  (setf out (append out (char->string (if in-word (ascii-downcase-char c) (ascii-upcase-char c)))))
                  (setf in-word true))
                (progn (setf out (append out (char->string c))) (setf in-word false)))
            (setf i (+ i 1)))))
      out)))

;; `split`: the pieces of `self` between occurrences of `sep`. Not CL (which
;; has no splitter at all), but the operation every program that reads lines
;; ends up writing. Adjacent separators produce empty pieces, and an empty
;; separator is rejected rather than looping forever.
(defmethod split ((self string) (sep string)) Vector<string>
  (let ((out (the Vector<string> (Vector::new))) (rest self) (go true))
    (progn
      (if (= (length sep) 0) (panic "split: the separator must not be empty") ())
      (while go
        (match (search rest sep)
          ((some k)
           (progn
             (push out (substring rest 0 k))
             (setf rest (substring rest (+ k (length sep)) (length rest)))
             ()))
          ((none) (progn (push out rest) (setf go false) ()))))
      out)))

;; `to-string`: a value's `~a` rendering as a string. CL reaches this through
;; `princ-to-string`/`write-to-string` (Phase 8a); this is the receiver-first
;; form, per scalar type rather than as one generic `defun` because `format`'s
;; `&rest` demands a concretely Sexpr-encodable element type and rejects a
;; type variable outright.
(defmethod to-string ((self i32)) string (format false "~a" self))
(defmethod to-string ((self i64)) string (format false "~a" self))
(defmethod to-string ((self f64)) string (format false "~a" self))
(defmethod to-string ((self bool)) string (format false "~a" self))
(defmethod to-string ((self char)) string (char->string self))
(defmethod to-string ((self string)) string self)

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
;; the same mutation a boxed struct's shared representation already gives
;; every `defstruct` instance.
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))

;; `vector-iter<T>` is `Vector<T>`'s own iterator: a shared reference to the
;; vector being walked (`vec`, sharing the same underlying box
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
;; ---------------------------------------------------------------------------
;; cl-parity-plan.md Phase 3e: the sequence keywords.
;;
;; CL's `:key`/`:test`/`:test-not`/`:start`/`:end`/`:from-end`/`:count`, on the
;; generic `defun`s of this library. Three small readers and four loop cores,
;; so each public function below spells only what is its own.
;;
;; **Every keyword is declared without a default.** That is forced, not
;; stylistic: a defaulted `&optional`/`&key` parameter's declared type may not
;; mention the function's own type parameters (`Checker::check_defun_opt_key`),
;; and `:key`/`:test` are function types over the element type. A defaultless
;; parameter arrives as `Option<...>`, so the default lives in the body.
;;
;; **Why the cores take a `hit` closure rather than the keywords themselves.**
;; `Option<(fn (A) A)>` is not writable as a declared type — the reader ends a
;; generic token at the first paren (`read::reader::extend_angle_token`), by
;; design, so that `(a<b c)` keeps reading as a call. A `&key` parameter gets
;; that type without anyone spelling it. So the keyword unpacking has to stay
;; in the function that declared the keywords, and what crosses into a core is
;; an ordinary `(fn (i32 A) bool)` closed over them.
(defun seq-in-bounds ((i i32) (start Option<i32>) (end Option<i32>)) bool
  "Whether index `i` lies in the `:start`/`:end` window `[start, end)`."
  (if (match start ((some s) (< i s)) ((none) false))
      false
      (match end ((some e) (< i e)) ((none) true))))

(defun seq-flag ((b Option<bool>)) bool
  "A boolean keyword (`:from-end`); false when the caller omitted it."
  (match b ((some v) v) ((none) false)))

(defun seq-limit ((n Option<i32>)) i32
  "`:count` as a plain limit; -1 (no limit) when the caller omitted it."
  (match n ((some v) v) ((none) -1)))

;; The forward scan the searches share. `last` is `:from-end`: instead of a
;; second pass it simply stops breaking, so the final match wins — which is
;; what "from the end" means for a one-shot forward cursor, and costs nothing
;; when it is off.
(defun seq-find-core<I,A> ((it I) (hit (fn (i32 A) bool)) (last bool)) Option<A>
  (where (Iter I (Item A)))
  (let ((result (the Option<A> (Option::none))) (i 0) (found false))
    (doiter (x it)
      (progn
        (when (hit i x) (progn (setf result (Option::some x)) (setf found true) ()))
        (setf i (+ i 1))
        (if (if found (not last) false) (break) ())))
    result))

(defun seq-position-core<I,A> ((it I) (hit (fn (i32 A) bool)) (last bool)) Option<i32>
  (where (Iter I (Item A)))
  "[`seq-find-core`] reporting the index instead of the element. The index is
   into the whole sequence, not into the `:start`/`:end` window — CL's rule."
  (let ((result (the Option<i32> (Option::none))) (i 0) (found false))
    (doiter (x it)
      (progn
        (when (hit i x) (progn (setf result (Option::some i)) (setf found true) ()))
        (setf i (+ i 1))
        (if (if found (not last) false) (break) ())))
    result))

(defun seq-count-core<I,A> ((it I) (hit (fn (i32 A) bool))) i32
  (where (Iter I (Item A)))
  (let ((n 0) (i 0))
    (doiter (x it)
      (progn (when (hit i x) (setf n (+ n 1))) (setf i (+ i 1)) ()))
    n))

;; `remove`/`substitute` and their `-if` variants. `act` decides what an
;; *affected* match contributes: `none` drops it, `(some v)` puts `v` in its
;; place. An element that does not match, or that `:count` has used up, passes
;; through unchanged — CL's rule, and the reason this cannot be written as a
;; filter over the window alone.
;;
;; A materialized copy, because `:count` together with `:from-end` cannot know
;; which matches to affect until it knows how many there are — that pairing is
;; the only case that walks twice, and the counting pass is skipped otherwise
;; (which also keeps a caller's `:key`/`:test` from being called twice per
;; element for nothing). The copy itself costs nothing extra in principle: the
;; result is a fresh `Vector` either way. It is spelled out rather than
;; delegated to `copy-seq`
;; because `copy-seq` is defined further down this file, and a top-level
;; `defun` may only call a name already seen.
(defun seq-edit-core<I,A> ((it I) (hit (fn (i32 A) bool)) (act (fn (A) Option<A>))
                           (limit i32) (last bool)) Vector<A>
  (where (Iter I (Item A)))
  (let ((buf (the Vector<A> (Vector::new))) (out (the Vector<A> (Vector::new)))
        (total 0) (seen 0) (i 0))
    (progn
      (doiter (x it) (push buf x))
      (when (if (< limit 0) false last)
        (while (< i (len buf))
          (progn (when (hit i (get buf i)) (setf total (+ total 1))) (setf i (+ i 1)) ())))
      (let ((skip (if (< limit 0) 0 (if last (if (> total limit) (- total limit) 0) 0))))
        (progn
          (setf i 0)
          (while (< i (len buf))
            (let ((x (get buf i)))
              (progn
                (if (hit i x)
                    (progn
                      (setf seen (+ seen 1))
                      (if (if (< limit 0) true (if last (> seen skip) (<= seen limit)))
                          (match (act x) ((some v) (push out v)) ((none) ()))
                          (push out x))
                      ())
                    (progn (push out x) ()))
                (setf i (+ i 1))
                ())))
          out)))))

;; Whether any element of `hay` satisfies `hit` — the membership test the set
;; operations share, with their `:key`/`:test` already folded into `hit`.
(defun seq-any-core<A> ((hay Vector<A>) (hit (fn (A) bool))) bool
  (let ((found false) (i 0))
    (progn
      (while (if found false (< i (len hay)))
        (progn (when (hit (get hay i)) (setf found true)) (setf i (+ i 1)) ()))
      found)))

;; Non-destructive insertion sort, stable: the inner shift uses strict `cmp`,
;; so elements equal under `cmp` keep their input order. `proj` is `:key`,
;; already defaulted to the identity by the caller — CL compares the
;; projections, not the elements.
(defun seq-sort-core<I,A> ((it I) (cmp (fn (A A) bool)) (proj (fn (A) A))) Vector<A>
  (where (Iter I (Item A)))
  (let ((out (the Vector<A> (Vector::new))))
    (doiter (x it)
      (let ((j (len out)))
        (push out x)
        (while (if (> j 0) (cmp (proj x) (proj (get out (- j 1)))) false)
          (set out j (get out (- j 1)))
          (setf j (- j 1)))
        (set out j x)))
    out))

(defun find-if<I,A> ((it I) (pred (fn (A) bool))
                     &key (key (fn (A) A)) (start i32) (end i32) (from-end bool))
    Option<A>
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-find-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (pred (proj y)) false))
      (seq-flag from-end))))
(defun position-if<I,A> ((it I) (pred (fn (A) bool))
                         &key (key (fn (A) A)) (start i32) (end i32) (from-end bool))
    Option<i32>
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-position-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (pred (proj y)) false))
      (seq-flag from-end))))
(defun position-if-not<I,A> ((it I) (pred (fn (A) bool))
                             &key (key (fn (A) A)) (start i32) (end i32) (from-end bool))
    Option<i32>
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-position-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (not (pred (proj y))) false))
      (seq-flag from-end))))
(defun count-if<I,A> ((it I) (pred (fn (A) bool))
                      &key (key (fn (A) A)) (start i32) (end i32))
    i32
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-count-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (pred (proj y)) false)))))
(defun remove-if<I,A> ((it I) (pred (fn (A) bool))
                       &key (key (fn (A) A)) (start i32) (end i32) (from-end bool) (count i32))
    Vector<A>
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-edit-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (pred (proj y)) false))
      (lambda ((y A)) Option<A> (Option::none))
      (seq-limit count) (seq-flag from-end))))
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

;; CL's `maphash` and `hash-table-size`, receiver-first and without the type
;; name in the method name (§1.1 of cl-parity-plan.md) — `(maphash h f)` and
;; `(size h)`, next to the `count` the table already had.
;;
;; `size` is `count`: this table is a Rust `HashMap`, which has no
;; user-visible capacity distinct from its occupancy, and reporting a made-up
;; number would be worse than reporting the real one. CL only promises that
;; `hash-table-size` is a non-negative integer.
(pub defmethod maphash ((self HashTable<K,V>) (f (fn (K V) ()))) ()
  "Call `f` on every key/value pair, in no particular order. CL's `maphash`."
  (doiter (e (iter self)) (f (car e) (cdr e))))

(pub defmethod size ((self HashTable<K,V>)) i32
  "How many entries the table holds. CL's `hash-table-size`, which this
language reports as the occupancy — a Rust `HashMap` has no separate
user-visible capacity."
  (count self))

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
;; core `equals`. The narrow integer widths and `f32` are further down, with
;; the arithmetic traits: they had no `=`/`<` to delegate to until Phase 1a
;; gave every width the same built-in catalog `i32` had.
(impl Eq i32    (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq i64    (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq f64    (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq bignum (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq ratio  (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq bool   (equals ((self Self) (other Self)) bool (equal self other)))
(impl Eq char   (equals ((self Self) (other Self)) bool (equal self other)))
(impl Eq string (equals ((self Self) (other Self)) bool (equal self other)))
(impl Eq symbol (equals ((self Self) (other Self)) bool (eq self other)))

;; `sexpr` is `eq`, deliberately — CL's identity, not `equal`'s recursive
;; descent. The type is a *union* of everything, so "compare two of these"
;; has no single right answer: `equal` would walk two cons trees and fold a
;; `Str`'s content, `eq` asks whether they are the same object. Picking `eq`
;; keeps `equals` on `sexpr` from being a third name for `equal` (`eq`/`eql`/
;; `equal`/`equalp` are already all four callable on it) and keeps the cost
;; of a comparison a constant.
;;
;; What this buys, since `match`'s value patterns compare through `Eq`
;; (`Pattern::Guard`): a literal pattern against a `Sexpr` scrutinee now
;; type-checks, and *matches* for every immediate — `'foo` (interned, so
;; identity is content equality), an integer, a `char`, a `bool`. What it
;; does not buy: `"a"`, a float, a bignum/ratio or a quoted list, whose
;; `eq` is the identity of a `Str`/box/cons cell. `(str "a")` — the
;; variant pattern, which destructures to a `string` and compares *that* by
;; content — is the spelling for those, and `docs/syntax.md`'s `match`
;; section says so.
(impl Eq sexpr  (equals ((self Self) (other Self)) bool (eq self other)))

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

;; The widths that had no methods at all until Phase 1a — every integer
;; other than `i32`/`i64`, plus `f32` — joining `Eq`/`Ord`. The comment
;; above used to end "...so they stay outside `Eq`/`Ord` until they grow
;; real arithmetic": they have, so they do.
(impl Eq i8    (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq i16   (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq isize (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq u8    (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq u16   (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq u32   (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq u64   (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq usize (equals ((self Self) (other Self)) bool (= self other)))
(impl Eq f32   (equals ((self Self) (other Self)) bool (= self other)))

(impl Ord i8    (less ((self Self) (other Self)) bool (< self other)))
(impl Ord i16   (less ((self Self) (other Self)) bool (< self other)))
(impl Ord isize (less ((self Self) (other Self)) bool (< self other)))
(impl Ord u8    (less ((self Self) (other Self)) bool (< self other)))
(impl Ord u16   (less ((self Self) (other Self)) bool (< self other)))
(impl Ord u32   (less ((self Self) (other Self)) bool (< self other)))
(impl Ord u64   (less ((self Self) (other Self)) bool (< self other)))
(impl Ord usize (less ((self Self) (other Self)) bool (< self other)))
(impl Ord f32   (less ((self Self) (other Self)) bool (< self other)))

;; ---------------------------------------------------------------------
;; `Hash` — CL's `sxhash`, as a trait.
;;
;; CL states the contract as an implication: `(equal x y)` implies
;; `(= (sxhash x) (sxhash y))`. Making it a trait with `Eq` as its supertrait
;; says the same thing in this language's own terms — a type that can be
;; hashed is a type whose values can be compared, and `sxhash` must agree
;; with that comparison. The reverse does not hold: two different values may
;; share a hash, and any user impl has to be written on that understanding.
;;
;; Results are non-negative and fit in 30 bits — CL calls the result a
;; *fixnum*, and staying well inside one keeps a hash usable as an index
;; without a sign check and cheap to combine.
;; `sxhash` is declared without a docstring on purpose: a lone trailing
;; string in a trait method is the *default body's* return value, not a
;; docstring (`take_leading_docstring`'s rule), and this method has no
;; default body. Its contract is the paragraph above.
(deftrait Hash (Eq)
  (sxhash ((self Self)) i64))

;; The mask that keeps a hash non-negative and fixnum-sized: 2^30-1.
(pub defconstant (*sxhash-mask* i64) 1073741823)

;; FNV-1a over a string's code points, written in typelisp rather than Rust:
;; it is pure arithmetic on values the language already has, which is the
;; side of the Rust-builtin policy line it falls on.
;;
;; The **32-bit** variant, deliberately. `i64` arithmetic here is ordinary
;; checked arithmetic, so the 64-bit variant's `h * 1099511628211` would
;; overflow on the second character; masking to 32 bits after each step keeps
;; the product below 2^56 and can never overflow. The 32-bit intermediate is
;; then narrowed to `*sxhash-mask*`, so every `sxhash` in this file lands in
;; the same 30-bit range.
(pub defconstant (*fnv-offset-basis* i64) 2166136261)
(pub defconstant (*fnv-prime* i64) 16777619)
(pub defconstant (*fnv-mask* i64) 4294967295)

(pub defun sxhash-string ((s string)) i64
  "FNV-1a over `s`'s code points — the hash `string`'s `Hash` impl uses."
  (let ((h *fnv-offset-basis*) (i 0) (n (length s)))
    (progn
      (while (< i n)
        (progn
          (setf h (logand (* (logxor h (as i64 (char->int (ref s i)))) *fnv-prime*) *fnv-mask*))
          (setf i (+ i 1))))
      (logand h *sxhash-mask*))))

(impl Hash i32    (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash i64    (sxhash ((self Self)) i64 (logand self *sxhash-mask*)))
(impl Hash i8     (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash i16    (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash isize  (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash u8     (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash u16    (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash u32    (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash u64    (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash usize  (sxhash ((self Self)) i64 (logand (as i64 self) *sxhash-mask*)))
(impl Hash bool   (sxhash ((self Self)) i64 (if self 1231 1237)))
(impl Hash char   (sxhash ((self Self)) i64 (as i64 (char->int self))))
(impl Hash string (sxhash ((self Self)) i64 (sxhash-string self)))
(impl Hash symbol (sxhash ((self Self)) i64 (sxhash-string (symbol->string self))))

;; ---------------------------------------------------------------------
;; The arithmetic traits.
;;
;; These exist for *generic* code. A call whose receiver type is known
;; goes on using the built-in operators, which lower to single LLVM
;; instructions and are not touched here. What was missing is the other
;; case: a `defun` whose parameter is a type variable had no way to ask
;; that the variable be a type you can add, so no generic function could
;; add two of its own arguments.
;;
;; The method names are `add`/`sub`/... rather than `+`/`-`/...: an `impl`
;; naming a method `+` is refused ("cannot redefine built-in method"), and
;; the refusal is right — the operator names belong to the primitive
;; types' own tables. The checker closes the gap from the other side: on a
;; receiver whose type is a `where`-bounded type variable, `(+ a b)`
;; resolves to the `Add` bound's `add` (`Checker::trait_operator_method`),
;; so generic code is still written with operators and only these
;; declarations spell the names out.
;;
;; There is no `Neg`: `(- x)` desugars to `(- (- x x) x)`
;; (`check_unary_negate_or_invert`), so negation on a type variable needs
;; `Sub` and nothing more.
(deftrait Add ()
  (add ((self Self) (other Self)) Self))
(deftrait Sub ()
  (sub ((self Self) (other Self)) Self))
(deftrait Mul ()
  (mul ((self Self) (other Self)) Self))
(deftrait Div ()
  (div ((self Self) (other Self)) Self))
;; `remainder`, not `rem`: `rem` is already a method on every numeric type
;; (above), and a trait may not declare a name its implementors already
;; define. The impls delegate to it, and `(rem a b)` on a bounded type
;; variable resolves here through the same operator spelling as `+`, so
;; the short name is still what generic code writes.
(deftrait Rem ()
  (remainder ((self Self) (other Self)) Self))
;; The bitwise catalog, spelled apart from the `logand`/`logior`/`logxor`/
;; `lognot` builtins for the same reason `add` is spelled apart from `+`.
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self))
;; What "a number" means as a bound: arithmetic and an ordering, with no
;; methods of its own — a name for the conjunction, so `(where (Number T))`
;; says in one bound what six would.
(deftrait Number (Add Sub Mul Div Rem Ord))

;; `mod`/`rem` for the widths that did not have them. Same bodies the
;; `i32`/`i64` (integer) and `f64` (float) methods above carry — floored for
;; `mod`, truncated for `rem`. The integer widths get `mod` from the built-in
;; table, so only `f32` needs one here.
(defmethod mod ((self f32) (b f32)) f32 (- self (* b (floor (/ self b)))))
(defmethod rem ((self f32) (b f32)) f32 (- self (* b (truncate (/ self b)))))
(defmethod rem ((self i8   ) (b i8   )) i8    (- self (* b (/ self b))))
(defmethod rem ((self i16  ) (b i16  )) i16   (- self (* b (/ self b))))
(defmethod rem ((self isize) (b isize)) isize (- self (* b (/ self b))))
(defmethod rem ((self u8   ) (b u8   )) u8    (- self (* b (/ self b))))
(defmethod rem ((self u16  ) (b u16  )) u16   (- self (* b (/ self b))))
(defmethod rem ((self u32  ) (b u32  )) u32   (- self (* b (/ self b))))
(defmethod rem ((self u64  ) (b u64  )) u64   (- self (* b (/ self b))))
(defmethod rem ((self usize) (b usize)) usize (- self (* b (/ self b))))

;; The impls. Each is the built-in operator under the trait's name.
(impl Add i32    (add ((self Self) (other Self)) Self (+ self other)))
(impl Add i64    (add ((self Self) (other Self)) Self (+ self other)))
(impl Add i8     (add ((self Self) (other Self)) Self (+ self other)))
(impl Add i16    (add ((self Self) (other Self)) Self (+ self other)))
(impl Add isize  (add ((self Self) (other Self)) Self (+ self other)))
(impl Add u8     (add ((self Self) (other Self)) Self (+ self other)))
(impl Add u16    (add ((self Self) (other Self)) Self (+ self other)))
(impl Add u32    (add ((self Self) (other Self)) Self (+ self other)))
(impl Add u64    (add ((self Self) (other Self)) Self (+ self other)))
(impl Add usize  (add ((self Self) (other Self)) Self (+ self other)))
(impl Add f64    (add ((self Self) (other Self)) Self (+ self other)))
(impl Add f32    (add ((self Self) (other Self)) Self (+ self other)))
(impl Add bignum (add ((self Self) (other Self)) Self (+ self other)))
(impl Add ratio  (add ((self Self) (other Self)) Self (+ self other)))

(impl Sub i32    (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub i64    (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub i8     (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub i16    (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub isize  (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub u8     (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub u16    (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub u32    (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub u64    (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub usize  (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub f64    (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub f32    (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub bignum (sub ((self Self) (other Self)) Self (- self other)))
(impl Sub ratio  (sub ((self Self) (other Self)) Self (- self other)))

(impl Mul i32    (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul i64    (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul i8     (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul i16    (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul isize  (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul u8     (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul u16    (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul u32    (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul u64    (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul usize  (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul f64    (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul f32    (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul bignum (mul ((self Self) (other Self)) Self (* self other)))
(impl Mul ratio  (mul ((self Self) (other Self)) Self (* self other)))

(impl Div i32    (div ((self Self) (other Self)) Self (/ self other)))
(impl Div i64    (div ((self Self) (other Self)) Self (/ self other)))
(impl Div i8     (div ((self Self) (other Self)) Self (/ self other)))
(impl Div i16    (div ((self Self) (other Self)) Self (/ self other)))
(impl Div isize  (div ((self Self) (other Self)) Self (/ self other)))
(impl Div u8     (div ((self Self) (other Self)) Self (/ self other)))
(impl Div u16    (div ((self Self) (other Self)) Self (/ self other)))
(impl Div u32    (div ((self Self) (other Self)) Self (/ self other)))
(impl Div u64    (div ((self Self) (other Self)) Self (/ self other)))
(impl Div usize  (div ((self Self) (other Self)) Self (/ self other)))
(impl Div f64    (div ((self Self) (other Self)) Self (/ self other)))
(impl Div f32    (div ((self Self) (other Self)) Self (/ self other)))
(impl Div bignum (div ((self Self) (other Self)) Self (/ self other)))
(impl Div ratio  (div ((self Self) (other Self)) Self (/ self other)))

(impl Rem i32    (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem i64    (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem i8     (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem i16    (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem isize  (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem u8     (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem u16    (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem u32    (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem u64    (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem usize  (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem f64    (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem f32    (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem bignum (remainder ((self Self) (other Self)) Self (rem self other)))
(impl Rem ratio  (remainder ((self Self) (other Self)) Self (rem self other)))

(impl Bits i32
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits i64
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits i8
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits i16
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits isize
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits u8
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits u16
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits u32
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits u64
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits usize
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))
(impl Bits bignum
  (bit-and ((self Self) (other Self)) Self (logand self other))
  (bit-or ((self Self) (other Self)) Self (logior self other))
  (bit-xor ((self Self) (other Self)) Self (logxor self other))
  (bit-not ((self Self)) Self (lognot self)))

;; `Number` has no methods, so its impls are the bare conjunction: this
;; type has all six.
(impl Number i32)
(impl Number i64)
(impl Number i8)
(impl Number i16)
(impl Number isize)
(impl Number u8)
(impl Number u16)
(impl Number u32)
(impl Number u64)
(impl Number usize)
(impl Number f64)
(impl Number f32)
(impl Number bignum)
(impl Number ratio)

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
;; Delegating to a sibling bounded generic *is* allowed — `elt` below is
;; `(nth n it)`, not a second copy of `nth`'s loop, and the Phase 3 catalog
;; further down leans on it throughout. This comment used to claim the
;; opposite, and the code right below it was already the counterexample.
;;
;; What was true until 2026-08-20: `validate_where_bounds` compared the
;; callee's declared associated-type pin against the caller's *raw*, each
;; written in its own type parameters, so the check only passed when the two
;; happened to spell the item variable with the same letter. `elt`/`nth` both
;; say `A` and worked; the same shape spelled `B` did not, and a structured
;; pin (`(Item cons-cell<K,V>)`) never matched at all. The callee's side is
;; now resolved through the call's own substitution first.
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
;; The item-based searches. `:test`/`:test-not` replace the `Eq` bound's
;; `equals` when given (CL's default test is `eql`; the one generic equality
;; here is `Eq`), and `:key` projects the *element* before the comparison —
;; never the item being searched for, which is CL's rule for this family.
;;
;; `same` is built as a local closure rather than a shared helper because a
;; helper would have to declare `Option<(fn (A A) bool)>`, which is not
;; writable (see the Phase 3e note above the cores).
(defun member<I,A> ((x A) (it I)
                    &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool)))
    bool
  (where (Iter I (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f p (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g p (proj q))))
                              ((none) (equals p (proj q)))))))))
      (is-some (seq-find-core it (lambda ((i i32) (y A)) bool (same x y)) false)))))
(defun find<I,A> ((x A) (it I)
                  &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool))
                       (start i32) (end i32) (from-end bool))
    Option<A>
  (where (Iter I (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f p (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g p (proj q))))
                              ((none) (equals p (proj q)))))))))
      (seq-find-core it
        (lambda ((i i32) (y A)) bool
          (if (seq-in-bounds i start end) (same x y) false))
        (seq-flag from-end)))))
(defun position<I,A> ((x A) (it I)
                      &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool))
                           (start i32) (end i32) (from-end bool))
    Option<i32>
  (where (Iter I (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f p (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g p (proj q))))
                              ((none) (equals p (proj q)))))))))
      (seq-position-core it
        (lambda ((i i32) (y A)) bool
          (if (seq-in-bounds i start end) (same x y) false))
        (seq-flag from-end)))))
(defun count<I,A> ((x A) (it I)
                   &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool))
                        (start i32) (end i32))
    i32
  (where (Iter I (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f p (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g p (proj q))))
                              ((none) (equals p (proj q)))))))))
      (seq-count-core it
        (lambda ((i i32) (y A)) bool
          (if (seq-in-bounds i start end) (same x y) false))))))
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
(defun sort<I,A> ((it I) (cmp (fn (A A) bool)) &key (key (fn (A) A))) Vector<A>
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-sort-core it cmp proj)))
;; `assoc` works over any iterator whose `Item` is a `cons-cell<K,V>` pair —
;; an alist (`Vector<cons-cell<K,V>>`) and a `HashTable<K,V>` (whose `iter`'s
;; `Item` is exactly `cons-cell<K,V>`) both qualify. Returns the whole
;; matching pair, CL-style; project the value with `(cdr p)`.
(defun assoc<I,K,V> ((k K) (it I)
                     &key (key (fn (K) K)) (test (fn (K K) bool)) (test-not (fn (K K) bool)))
    Option<cons-cell<K,V>>
  (where (Iter I (Item cons-cell<K,V>)) (Eq K))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y K)) K y)))))
    (let ((same (lambda ((p K) (q K)) bool
                  (match test
                    ((some f) (f p (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g p (proj q))))
                              ((none) (equals p (proj q)))))))))
      (seq-find-core it (lambda ((i i32) (p cons-cell<K,V>)) bool (same k (car p))) false))))
;; ---------------------------------------------------------------------------
;; The rest of CL's list/sequence catalog — cl-parity-plan.md Phase 3a/3b/3c.
;;
;; Same shape as the `Iter` library above and for the same reason: these are
;; generic `defun`s over `(where (Iter I (Item A)))`, not `defmethod`s, because
;; a `defmethod` resolves by the receiver's *exact* type and would have to be
;; written once per collection. `Vector<T>` reaches them through `(iter v)`,
;; exactly as `map`/`filter` are already reached.
;;
;; The departures from CL that the `Iter` shape forces (documented above and in
;; functions.md §6) carry over unchanged: a search reports `bool`/`Option`
;; rather than the tail cons, and everything that CL returns as a fresh list
;; comes back as a `Vector<A>`.

;; --- Phase 3a: positional accessors and copying ---
;; CL's `first`..`tenth`. `nth` already does the work; these are the named
;; arities, and they delegate rather than duplicating the loop (which
;; `validate_where_bounds` now permits for any pin, not only one that happens
;; to spell its item variable `A` — see the plan's Phase 3 notes).
(defun first<I,A>   ((it I)) Option<A> (where (Iter I (Item A))) (nth 0 it))
(defun second<I,A>  ((it I)) Option<A> (where (Iter I (Item A))) (nth 1 it))
(defun third<I,A>   ((it I)) Option<A> (where (Iter I (Item A))) (nth 2 it))
(defun fourth<I,A>  ((it I)) Option<A> (where (Iter I (Item A))) (nth 3 it))
(defun fifth<I,A>   ((it I)) Option<A> (where (Iter I (Item A))) (nth 4 it))
(defun sixth<I,A>   ((it I)) Option<A> (where (Iter I (Item A))) (nth 5 it))
(defun seventh<I,A> ((it I)) Option<A> (where (Iter I (Item A))) (nth 6 it))
(defun eighth<I,A>  ((it I)) Option<A> (where (Iter I (Item A))) (nth 7 it))
(defun ninth<I,A>   ((it I)) Option<A> (where (Iter I (Item A))) (nth 8 it))
(defun tenth<I,A>   ((it I)) Option<A> (where (Iter I (Item A))) (nth 9 it))
;; CL's `rest`: everything but the first element. A fresh `Vector`, not a tail
;; cons — an iterator has no tail to share.
(defun rest<I,A> ((it I)) Vector<A> (where (Iter I (Item A)))
  (let ((out (the Vector<A> (Vector::new))) (skipped false))
    (progn
      (doiter (x it) (if skipped (push out x) (progn (setf skipped true) ())))
      out)))
;; CL's `copy-seq`/`copy-list`: materialize an iterator into a fresh `Vector`.
;; Used throughout this section wherever a sequence has to be walked twice —
;; an `Iter` is a one-shot cursor.
(defun copy-seq<I,A> ((it I)) Vector<A> (where (Iter I (Item A)))
  (let ((out (the Vector<A> (Vector::new))))
    (progn (doiter (x it) (push out x)) out)))
;; CL's `revappend`: `a` reversed, then `b`.
(defun revappend<I,J,A> ((a I) (b J)) Vector<A>
  (where (Iter I (Item A)) (Iter J (Item A)))
  (append (iter (reverse a)) b))
;; CL's `make-list`/`make-sequence`: `n` copies of `x`. A static method, so it
;; reads like `Vector::new` — and, like `Vector::new`, its type argument comes
;; from the surrounding expected type, so a bare `let` binding needs `the`.
(defmethod filled (Vector<T> (n i32) (x T)) Vector<T>
  (let ((out (the Vector<T> (Vector::new))) (i 0))
    (progn (while (< i n) (progn (push out x) (setf i (+ i 1)))) out)))

;; CL's `caar`..`cddddr`, over *nested pairs* rather than lists: `cadr` wants a
;; `cons-cell<A,cons-cell<B,C>>`, which is what `(cons 1 (cons 2 3))` builds.
;; Free `defun`s rather than `defmethod`s on `cons-cell` — the receiver of a
;; `defmethod` only binds type variables that sit at the *top level* of its
;; type arguments (`check_defmethod`'s `written_vars`), so `((self
;; cons-cell<A,cons-cell<B,C>>))` leaves `B`/`C` unbound and `car` stops
;; resolving on it. Generated mechanically; the type of `cXr` is read
;; right-to-left, one `cons-cell` nesting per letter.
(defun caar<A,B,C> ((x cons-cell<cons-cell<A,B>,C>)) A (car (car x)))
(defun cadr<A,B,C> ((x cons-cell<A,cons-cell<B,C>>)) B (car (cdr x)))
(defun cdar<A,B,C> ((x cons-cell<cons-cell<A,B>,C>)) B (cdr (car x)))
(defun cddr<A,B,C> ((x cons-cell<A,cons-cell<B,C>>)) C (cdr (cdr x)))
(defun caaar<A,B,C,D> ((x cons-cell<cons-cell<cons-cell<A,B>,C>,D>)) A (car (car (car x))))
(defun caadr<A,B,C,D> ((x cons-cell<A,cons-cell<cons-cell<B,C>,D>>)) B (car (car (cdr x))))
(defun cadar<A,B,C,D> ((x cons-cell<cons-cell<A,cons-cell<B,C>>,D>)) B (car (cdr (car x))))
(defun caddr<A,B,C,D> ((x cons-cell<A,cons-cell<B,cons-cell<C,D>>>)) C (car (cdr (cdr x))))
(defun cdaar<A,B,C,D> ((x cons-cell<cons-cell<cons-cell<A,B>,C>,D>)) B (cdr (car (car x))))
(defun cdadr<A,B,C,D> ((x cons-cell<A,cons-cell<cons-cell<B,C>,D>>)) C (cdr (car (cdr x))))
(defun cddar<A,B,C,D> ((x cons-cell<cons-cell<A,cons-cell<B,C>>,D>)) C (cdr (cdr (car x))))
(defun cdddr<A,B,C,D> ((x cons-cell<A,cons-cell<B,cons-cell<C,D>>>)) D (cdr (cdr (cdr x))))
(defun caaaar<A,B,C,D,E> ((x cons-cell<cons-cell<cons-cell<cons-cell<A,B>,C>,D>,E>)) A (car (car (car (car x)))))
(defun caaadr<A,B,C,D,E> ((x cons-cell<A,cons-cell<cons-cell<cons-cell<B,C>,D>,E>>)) B (car (car (car (cdr x)))))
(defun caadar<A,B,C,D,E> ((x cons-cell<cons-cell<A,cons-cell<cons-cell<B,C>,D>>,E>)) B (car (car (cdr (car x)))))
(defun caaddr<A,B,C,D,E> ((x cons-cell<A,cons-cell<B,cons-cell<cons-cell<C,D>,E>>>)) C (car (car (cdr (cdr x)))))
(defun cadaar<A,B,C,D,E> ((x cons-cell<cons-cell<cons-cell<A,cons-cell<B,C>>,D>,E>)) B (car (cdr (car (car x)))))
(defun cadadr<A,B,C,D,E> ((x cons-cell<A,cons-cell<cons-cell<B,cons-cell<C,D>>,E>>)) C (car (cdr (car (cdr x)))))
(defun caddar<A,B,C,D,E> ((x cons-cell<cons-cell<A,cons-cell<B,cons-cell<C,D>>>,E>)) C (car (cdr (cdr (car x)))))
(defun cadddr<A,B,C,D,E> ((x cons-cell<A,cons-cell<B,cons-cell<C,cons-cell<D,E>>>>)) D (car (cdr (cdr (cdr x)))))
(defun cdaaar<A,B,C,D,E> ((x cons-cell<cons-cell<cons-cell<cons-cell<A,B>,C>,D>,E>)) B (cdr (car (car (car x)))))
(defun cdaadr<A,B,C,D,E> ((x cons-cell<A,cons-cell<cons-cell<cons-cell<B,C>,D>,E>>)) C (cdr (car (car (cdr x)))))
(defun cdadar<A,B,C,D,E> ((x cons-cell<cons-cell<A,cons-cell<cons-cell<B,C>,D>>,E>)) C (cdr (car (cdr (car x)))))
(defun cdaddr<A,B,C,D,E> ((x cons-cell<A,cons-cell<B,cons-cell<cons-cell<C,D>,E>>>)) D (cdr (car (cdr (cdr x)))))
(defun cddaar<A,B,C,D,E> ((x cons-cell<cons-cell<cons-cell<A,cons-cell<B,C>>,D>,E>)) C (cdr (cdr (car (car x)))))
(defun cddadr<A,B,C,D,E> ((x cons-cell<A,cons-cell<cons-cell<B,cons-cell<C,D>>,E>>)) D (cdr (cdr (car (cdr x)))))
(defun cdddar<A,B,C,D,E> ((x cons-cell<cons-cell<A,cons-cell<B,cons-cell<C,D>>>,E>)) D (cdr (cdr (cdr (car x)))))
(defun cddddr<A,B,C,D,E> ((x cons-cell<A,cons-cell<B,cons-cell<C,cons-cell<D,E>>>>)) E (cdr (cdr (cdr (cdr x)))))

;; --- Phase 3b: predicate, negated and mapping variants ---
;; The `-if`/`-if-not` pairs CL has for every search. `member-if` reports
;; `bool` like `member` does, the same deliberate departure (an iterator has no
;; tail cons to return).
(defun member-if<I,A> ((it I) (pred (fn (A) bool)) &key (key (fn (A) A))) bool
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (is-some (seq-find-core it (lambda ((i i32) (y A)) bool (pred (proj y))) false))))
(defun member-if-not<I,A> ((it I) (pred (fn (A) bool)) &key (key (fn (A) A))) bool
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (is-some (seq-find-core it (lambda ((i i32) (y A)) bool (not (pred (proj y)))) false))))
(defun notany<I,A> ((it I) (pred (fn (A) bool))) bool (where (Iter I (Item A)))
  (not (any it pred)))
(defun notevery<I,A> ((it I) (pred (fn (A) bool))) bool (where (Iter I (Item A)))
  (not (every it pred)))
(defun find-if-not<I,A> ((it I) (pred (fn (A) bool))
                         &key (key (fn (A) A)) (start i32) (end i32) (from-end bool))
    Option<A>
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-find-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (not (pred (proj y))) false))
      (seq-flag from-end))))
(defun count-if-not<I,A> ((it I) (pred (fn (A) bool))
                          &key (key (fn (A) A)) (start i32) (end i32))
    i32
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-count-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (not (pred (proj y))) false)))))
(defun remove-if-not<I,A> ((it I) (pred (fn (A) bool))
                           &key (key (fn (A) A)) (start i32) (end i32) (from-end bool) (count i32))
    Vector<A>
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-edit-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (not (pred (proj y))) false))
      (lambda ((y A)) Option<A> (Option::none))
      (seq-limit count) (seq-flag from-end))))
(defun remove<I,A> ((x A) (it I)
                    &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool))
                         (start i32) (end i32) (from-end bool) (count i32))
    Vector<A>
  (where (Iter I (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f p (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g p (proj q))))
                              ((none) (equals p (proj q)))))))))
      (seq-edit-core it
        (lambda ((i i32) (y A)) bool
          (if (seq-in-bounds i start end) (same x y) false))
        (lambda ((y A)) Option<A> (Option::none))
        (seq-limit count) (seq-flag from-end)))))
;; `remove-duplicates`: **CL's rule, which is not what this used to do.**
;; The default keeps the *last* of each group of equals; `:from-end t` keeps
;; the first (the order-preserving dedup this function used to be
;; unconditionally). Elements outside `:start`/`:end` are neither removed nor
;; compared against.
(defun remove-duplicates<I,A> ((it I)
                               &key (key (fn (A) A)) (test (fn (A A) bool))
                                    (test-not (fn (A A) bool))
                                    (start i32) (end i32) (from-end bool))
    Vector<A>
  (where (Iter I (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f (proj p) (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g (proj p) (proj q))))
                              ((none) (equals (proj p) (proj q))))))))
          (buf (copy-seq it)) (out (the Vector<A> (Vector::new)))
          (last (seq-flag from-end)) (i 0))
      (progn
        (while (< i (len buf))
          (let ((x (get buf i)))
            (progn
              (if (seq-in-bounds i start end)
                  (let ((dup false) (j (if last 0 (+ i 1))) (stop (if last i (len buf))))
                    (progn
                      (while (< j stop)
                        (progn
                          (when (if (seq-in-bounds j start end) (same x (get buf j)) false)
                            (setf dup true))
                          (setf j (+ j 1))
                          ()))
                      (if dup () (push out x))
                      ()))
                  (progn (push out x) ()))
              (setf i (+ i 1))
              ())))
        out))))
(defun substitute<I,A> ((new A) (old A) (it I)
                        &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool))
                             (start i32) (end i32) (from-end bool) (count i32))
    Vector<A>
  (where (Iter I (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f p (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g p (proj q))))
                              ((none) (equals p (proj q)))))))))
      (seq-edit-core it
        (lambda ((i i32) (y A)) bool
          (if (seq-in-bounds i start end) (same old y) false))
        (lambda ((y A)) Option<A> (Option::some new))
        (seq-limit count) (seq-flag from-end)))))
(defun substitute-if<I,A> ((new A) (pred (fn (A) bool)) (it I)
                           &key (key (fn (A) A)) (start i32) (end i32) (from-end bool) (count i32))
    Vector<A>
  (where (Iter I (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-edit-core it
      (lambda ((i i32) (y A)) bool
        (if (seq-in-bounds i start end) (pred (proj y)) false))
      (lambda ((y A)) Option<A> (Option::some new))
      (seq-limit count) (seq-flag from-end))))
;; The association-list catalog around the existing `assoc`.
(defun assoc-if<I,K,V> ((it I) (pred (fn (K) bool)) &key (key (fn (K) K)))
    Option<cons-cell<K,V>>
  (where (Iter I (Item cons-cell<K,V>)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y K)) K y)))))
    (seq-find-core it (lambda ((i i32) (p cons-cell<K,V>)) bool (pred (proj (car p)))) false)))
(defun rassoc<I,K,V> ((v V) (it I)
                      &key (key (fn (V) V)) (test (fn (V V) bool)) (test-not (fn (V V) bool)))
    Option<cons-cell<K,V>>
  (where (Iter I (Item cons-cell<K,V>)) (Eq V))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y V)) V y)))))
    (let ((same (lambda ((p V) (q V)) bool
                  (match test
                    ((some f) (f p (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g p (proj q))))
                              ((none) (equals p (proj q)))))))))
      (seq-find-core it (lambda ((i i32) (p cons-cell<K,V>)) bool (same v (cdr p))) false))))
(defun rassoc-if<I,K,V> ((it I) (pred (fn (V) bool)) &key (key (fn (V) V)))
    Option<cons-cell<K,V>>
  (where (Iter I (Item cons-cell<K,V>)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y V)) V y)))))
    (seq-find-core it (lambda ((i i32) (p cons-cell<K,V>)) bool (pred (proj (cdr p)))) false)))
(defun acons<I,K,V> ((k K) (v V) (it I)) Vector<cons-cell<K,V>>
  (where (Iter I (Item cons-cell<K,V>)))
  (let ((out (the Vector<cons-cell<K,V>> (Vector::new))))
    (progn (push out (cons k v)) (doiter (p it) (push out p)) out)))
(defun pairlis<I,J,K,V> ((ks I) (vs J)) Vector<cons-cell<K,V>>
  (where (Iter I (Item K)) (Iter J (Item V)))
  (let ((kv (copy-seq ks)) (vv (copy-seq vs)))
    (let ((out (the Vector<cons-cell<K,V>> (Vector::new))) (i 0) (n (min (len kv) (len vv))))
      (progn
        (while (< i n) (progn (push out (cons (get kv i) (get vv i))) (setf i (+ i 1))))
        out))))
;; CL's multi-sequence `mapcar`, at the arity that covers nearly every use:
;; two sequences walked in step, stopping at the shorter. (`map` above is the
;; one-sequence form.)
(defun map2<I,J,A,B,U> ((a I) (b J) (f (fn (A B) U))) Vector<U>
  (where (Iter I (Item A)) (Iter J (Item B)))
  (let ((av (copy-seq a)) (bv (copy-seq b)))
    (let ((out (the Vector<U> (Vector::new))) (i 0) (n (min (len av) (len bv))))
      (progn
        (while (< i n) (progn (push out (f (get av i) (get bv i))) (setf i (+ i 1))))
        out))))
;; CL's `mapc` (map for effect), `mapcan` (map then concatenate) and `maplist`
;; (map over successive *tails*).
(defun mapc<I,A> ((it I) (f (fn (A) ()))) () (where (Iter I (Item A)))
  (doiter (x it) (f x)))
(defun mapcan<I,A,U> ((it I) (f (fn (A) Vector<U>))) Vector<U> (where (Iter I (Item A)))
  (let ((out (the Vector<U> (Vector::new))))
    (progn (doiter (x it) (doiter (y (iter (f x))) (push out y))) out)))
(defun maplist<I,A,U> ((it I) (f (fn (Vector<A>) U))) Vector<U> (where (Iter I (Item A)))
  (let ((v (copy-seq it)))
    (let ((out (the Vector<U> (Vector::new))) (i 0) (n (len v)))
      (progn
        (while (< i n) (progn (push out (f (subseq (iter v) i n))) (setf i (+ i 1))))
        out))))
;; CL's `merge`. CL requires both inputs already sorted and merges in linear
;; time; this sorts the concatenation, which agrees on every input CL defines
;; an answer for and is also correct on the ones it does not.
(defun merge<I,J,A> ((a I) (b J) (less (fn (A A) bool)) &key (key (fn (A) A))) Vector<A>
  (where (Iter I (Item A)) (Iter J (Item A)))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (seq-sort-core (iter (append a b)) less proj)))
;; --- Phase 3c: set operations, and the list-tail relations ---
;; All `Eq`-bounded and quadratic, like CL's own list-based versions. `union`
;; and friends return elements in first-appearance order rather than CL's
;; unspecified one — a stable answer is worth more than the freedom.
;;
;; Phase 3e gave them `:key`/`:test`/`:test-not`. Unlike the item searches
;; above, `:key` here projects *both* sides of every comparison: both are
;; sequence elements.
(defun seq-equals<A> ((a Vector<A>) (b Vector<A>)) bool (where (Eq A))
  "Element-wise equality of two vectors. `Vector<T>` has no `Eq` impl of its
   own (that would need a bound on `T` an `impl` cannot express here), so the
   tail relations below compare through this instead."
  (if (/= (len a) (len b))
      false
      (let ((i 0) (ok true))
        (progn
          (while (and (< i (len a)) ok)
            (progn
              (if (equals (get a i) (get b i)) () (progn (setf ok false) ()))
              (setf i (+ i 1))))
          ok))))
;; CL's `adjoin`: `x` prepended unless it is already there. CL conses onto the
;; front, so the new element leads.
(defun adjoin<I,A> ((x A) (it I)
                    &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool)))
    Vector<A>
  (where (Iter I (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f (proj p) (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g (proj p) (proj q))))
                              ((none) (equals (proj p) (proj q))))))))
          (v (copy-seq it)))
      (if (seq-any-core v (lambda ((z A)) bool (same x z)))
          v
          (let ((out (the Vector<A> (Vector::new))))
            (progn (push out x) (doiter (y (iter v)) (push out y)) out))))))
(defun union<I,J,A> ((a I) (b J)
                     &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool)))
    Vector<A>
  (where (Iter I (Item A)) (Iter J (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f (proj p) (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g (proj p) (proj q))))
                              ((none) (equals (proj p) (proj q))))))))
          (out (the Vector<A> (Vector::new))))
      (progn
        (doiter (x a)
          (when (not (seq-any-core out (lambda ((z A)) bool (same x z)))) (push out x)))
        (doiter (y b)
          (when (not (seq-any-core out (lambda ((z A)) bool (same y z)))) (push out y)))
        out))))
(defun intersection<I,J,A> ((a I) (b J)
                            &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool)))
    Vector<A>
  (where (Iter I (Item A)) (Iter J (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f (proj p) (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g (proj p) (proj q))))
                              ((none) (equals (proj p) (proj q))))))))
          (bv (copy-seq b)) (out (the Vector<A> (Vector::new))))
      (progn
        (doiter (x a)
          (when (if (seq-any-core bv (lambda ((z A)) bool (same x z)))
                    (not (seq-any-core out (lambda ((z A)) bool (same x z))))
                    false)
            (push out x)))
        out))))
(defun set-difference<I,J,A> ((a I) (b J)
                              &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool)))
    Vector<A>
  (where (Iter I (Item A)) (Iter J (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f (proj p) (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g (proj p) (proj q))))
                              ((none) (equals (proj p) (proj q))))))))
          (bv (copy-seq b)) (out (the Vector<A> (Vector::new))))
      (progn
        (doiter (x a)
          (when (if (seq-any-core bv (lambda ((z A)) bool (same x z)))
                    false
                    (not (seq-any-core out (lambda ((z A)) bool (same x z)))))
            (push out x)))
        out))))
(defun set-exclusive-or<I,J,A> ((a I) (b J)
                                &key (key (fn (A) A)) (test (fn (A A) bool))
                                     (test-not (fn (A A) bool)))
    Vector<A>
  (where (Iter I (Item A)) (Iter J (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f (proj p) (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g (proj p) (proj q))))
                              ((none) (equals (proj p) (proj q))))))))
          (av (copy-seq a)) (bv (copy-seq b)) (out (the Vector<A> (Vector::new))))
      (progn
        (doiter (x (iter av))
          (when (if (seq-any-core bv (lambda ((z A)) bool (same x z)))
                    false
                    (not (seq-any-core out (lambda ((z A)) bool (same x z)))))
            (push out x)))
        (doiter (y (iter bv))
          (when (if (seq-any-core av (lambda ((z A)) bool (same y z)))
                    false
                    (not (seq-any-core out (lambda ((z A)) bool (same y z)))))
            (push out y)))
        out))))
(defun subsetp<I,J,A> ((a I) (b J)
                       &key (key (fn (A) A)) (test (fn (A A) bool)) (test-not (fn (A A) bool)))
    bool
  (where (Iter I (Item A)) (Iter J (Item A)) (Eq A))
  (let ((proj (match key ((some f) f) ((none) (lambda ((y A)) A y)))))
    (let ((same (lambda ((p A) (q A)) bool
                  (match test
                    ((some f) (f (proj p) (proj q)))
                    ((none) (match test-not
                              ((some g) (not (g (proj p) (proj q))))
                              ((none) (equals (proj p) (proj q))))))))
          (bv (copy-seq b)))
      (every a (lambda ((x A)) bool (seq-any-core bv (lambda ((z A)) bool (same x z))))))))
;; CL's `tailp`/`ldiff`. CL asks about shared *structure* (`tail` must be one
;; of `whole`'s own conses); with no shared structure to ask about, this asks
;; the observable question instead — is `tail` a suffix of `whole` by value.
(defun tailp<I,J,A> ((tail I) (whole J)) bool
  (where (Iter I (Item A)) (Iter J (Item A)) (Eq A))
  (let ((tv (copy-seq tail)) (wv (copy-seq whole)))
    (let ((k (- (len wv) (len tv))))
      (if (< k 0) false (seq-equals (subseq (iter wv) k (len wv)) tv)))))
(defun ldiff<I,J,A> ((whole I) (tail J)) Vector<A>
  (where (Iter I (Item A)) (Iter J (Item A)) (Eq A))
  (let ((wv (copy-seq whole)) (tv (copy-seq tail)))
    (if (tailp (iter tv) (iter wv))
        (subseq (iter wv) 0 (- (len wv) (len tv)))
        (copy-seq (iter wv)))))

;; --- Phase 3d: the destructive operations ---
;; language-design.md §0's (D4) said destructive operations would not be
;; taken. cl-parity-plan.md revisits that: `Vector<T>` is a *reference* type
;; whose elements live on the heap, so `nreverse` and friends are ordinary
;; in-place mutation with no shared-structure hazard — the thing (D4) was
;; actually protecting against. The one CL operation that really does rewrite
;; shared structure, `nconc` over conses, is not what `nconc` means here (see
;; below).
;;
;; Every one of these mutates the receiver *and* returns it, so `(nreverse v)`
;; reads like the functional `reverse` while `v` itself is also reversed. The
;; naming follows language-design.md §7.3 — no `!` suffix — which is why they
;; keep CL's own `n`/`delete` spellings rather than inventing a marker.
;;
;; `vector-push-extend` and `vector-pop` are the existing `push`/`pop`: a
;; `Vector<T>` has always grown on demand, so CL's distinction between a
;; fill-pointer vector and a simple one has nothing to attach to.
;;
;; The receivers spell `Vector`'s type parameter `T` because that is the name
;; `Vector<T>` itself was declared with — a generic `defmethod`'s registered
;; signature is rewritten into the owner's parameter names, so any name works,
;; but matching keeps the two readable side by side.
(defmethod set-contents ((self Vector<T>) (src Vector<T>)) Vector<T>
  "Replaces every element of `self` with `src`'s, in place, changing its
   length. The shared bottom half of the `delete`/`n...` family: each of them
   computes the new contents with the corresponding non-destructive function
   above and then installs the answer here, rather than repeating a
   compaction loop per operation."
  (progn
    (while (> (len self) 0) (progn (pop self) ()))
    (doiter (x (iter src)) (push self x))
    self))
(defmethod nreverse ((self Vector<T>)) Vector<T>
  (let ((i 0) (j (- (len self) 1)))
    (progn
      (while (< i j)
        (let ((tmp (get self i)))
          (progn
            (set self i (get self j))
            (set self j tmp)
            (setf i (+ i 1))
            (setf j (- j 1))
            ())))
      self)))
(defmethod delete ((self Vector<T>) (x T)) Vector<T> (where (Eq T))
  (set-contents self (remove x (iter self))))
(defmethod delete-if ((self Vector<T>) (pred (fn (T) bool))) Vector<T>
  (set-contents self (remove-if (iter self) pred)))
(defmethod delete-if-not ((self Vector<T>) (pred (fn (T) bool))) Vector<T>
  (set-contents self (filter (iter self) pred)))
(defmethod delete-duplicates ((self Vector<T>)) Vector<T> (where (Eq T))
  (set-contents self (remove-duplicates (iter self))))
(defmethod nsubstitute ((self Vector<T>) (new T) (old T)) Vector<T> (where (Eq T))
  (set-contents self (substitute new old (iter self))))
(defmethod nsubstitute-if ((self Vector<T>) (new T) (pred (fn (T) bool))) Vector<T>
  (set-contents self (substitute-if new pred (iter self))))
(defmethod nbutlast ((self Vector<T>)) Vector<T>
  (progn (if (> (len self) 0) (progn (pop self) ()) ()) self))
;; CL's `fill`/`replace`/`map-into` all write into an existing sequence and
;; leave its length alone, copying `(min (len self) (len src))` elements.
(defmethod fill ((self Vector<T>) (x T)) Vector<T>
  (let ((i 0))
    (progn (while (< i (len self)) (progn (set self i x) (setf i (+ i 1)))) self)))
(defmethod replace ((self Vector<T>) (src Vector<T>)) Vector<T>
  (let ((i 0) (n (min (len self) (len src))))
    (progn (while (< i n) (progn (set self i (get src i)) (setf i (+ i 1)))) self)))
(defmethod map-into ((self Vector<T>) (src Vector<T>) (f (fn (T) T))) Vector<T>
  (let ((i 0) (n (min (len self) (len src))))
    (progn (while (< i n) (progn (set self i (f (get src i))) (setf i (+ i 1)))) self)))
;; CL's `nconc` splices by rewriting the last cons of the first list, which is
;; why it is the one destructive operation (D4) genuinely warned about. Here
;; there is no last cons and nothing is shared: `other`'s elements are simply
;; appended into `self`, which `other` neither observes nor is affected by.
(defmethod nconc ((self Vector<T>) (other Vector<T>)) Vector<T>
  (progn (doiter (x (iter other)) (push self x)) self))
(defmethod nreconc ((self Vector<T>) (other Vector<T>)) Vector<T>
  (nconc (nreverse self) other))
;; CL's `rplaca`/`rplacd`: `cons-cell`'s own field setters, returning the cell
;; rather than `()` so they compose the way CL's do. Deliberately *not*
;; offered for `Sexpr` — beyond §1.2's "no `Sexpr` list methods", a `Sexpr`
;; cons carries its `car`'s source span inline (`value::Cell::car_loc`), and
;; overwriting the `car` would leave the old element's position attached to
;; the new one, quietly misplacing every later diagnostic.
(defmethod rplaca ((self cons-cell<A,B>) (x A)) cons-cell<A,B>
  (progn (set-car self x) self))
(defmethod rplacd ((self cons-cell<A,B>) (x B)) cons-cell<A,B>
  (progn (set-cdr self x) self))

;; ---------------------------------------------------------------------------
;; The rest of CL's numeric catalog — cl-parity-plan.md Phase 1c.
;;
;; Everything here is built from the primitives above, so it compiles through
;; the ordinary prelude path.

;; CL's `ffloor`/`fceiling`/`fround`/`ftruncate`: round, but stay a float.
;; This language's `f64` `floor`/`ceiling`/`round`/`truncate` *already* return
;; `f64` (functions.md §2) — CL's undecorated names return an integer, so it is
;; the `f`-prefixed CL names that these match. Thin aliases, kept so ported CL
;; reads unchanged.
(defmethod ffloor ((self f64)) f64 (floor self))
(defmethod fceiling ((self f64)) f64 (ceiling self))
(defmethod fround ((self f64)) f64 (round self))
(defmethod ftruncate ((self f64)) f64 (truncate self))

;; CL's `isqrt`: the greatest integer whose square does not exceed `self`.
;; Integer Newton, keeping the *previous* estimate and stopping when the next
;; one stops decreasing — testing "unchanged" instead loops forever on the
;; inputs where the iteration oscillates between two neighbouring values.
(defmethod isqrt ((self i32)) i32
  (if (< self 0)
      (panic "isqrt: negative argument")
      (if (< self 2)
          self
          (let ((g self) (k (/ (+ self 1) 2)))
            (progn
              (while (< k g) (progn (setf g k) (setf k (/ (+ g (/ self g)) 2)) ()))
              g)))))
(defmethod isqrt ((self i64)) i64
  (if (< self 0)
      (panic "isqrt: negative argument")
      (if (< self 2)
          self
          (let ((g self) (k (/ (+ self 1) 2)))
            (progn
              (while (< k g) (progn (setf g k) (setf k (/ (+ g (/ self g)) 2)) ()))
              g)))))

;; CL's integer `expt`, by squaring. CL answers a *ratio* for a negative
;; exponent; an `i32`/`i64` result cannot hold one, so that case is a panic
;; rather than a silent truncation — convert to `ratio` first if you want it
;; (`bignum`/`ratio` already have their own `expt` above).
(defmethod expt ((self i32) (e i32)) i32
  (if (< e 0)
      (panic "expt: a negative exponent on an integer is not an integer")
      (if (= e 0)
          1
          (let ((half (expt self (/ e 2))))
            (if (evenp e) (* half half) (* self (* half half)))))))
(defmethod expt ((self i64) (e i64)) i64
  (if (< e 0)
      (panic "expt: a negative exponent on an integer is not an integer")
      (if (= e 0)
          1
          (let ((half (expt self (/ e 2))))
            (if (evenp e) (* half half) (* self (* half half)))))))

;; CL's float-representation accessors (CLHS 12.1.4.1). `f64` is IEEE-754
;; binary64 here and always will be, so `float-radix`/`float-digits`/
;; `float-precision` are the constants 2/53/53 rather than queries — CL allows
;; an implementation to fix them, and a denormal is the only case where
;; `float-precision` would differ, which this does not track.
(defmethod float-radix ((self f64)) i32 2)
(defmethod float-digits ((self f64)) i32 53)
(defmethod float-precision ((self f64)) i32 (if (= self 0.0) 0 53))
(defmethod float-sign ((self f64)) f64 (if (< self 0.0) -1.0 1.0))
;; `(scale-float x n)` = `x * 2^n`. Halving/doubling in a loop rather than
;; `(* self (expt 2.0 (int->float n)))`: `int->float` is one of the builtins
;; the island has no lowering for, and reaching it would make this whole
;; section interpreted-only (the same constraint the character section above
;; works under). Every step here is exact in binary floating point.
(defmethod scale-float ((self f64) (n i32)) f64
  (let ((r self) (k (abs n)))
    (progn
      (if (>= n 0)
          (while (> k 0) (progn (setf r (* r 2.0)) (setf k (- k 1)) ()))
          (while (> k 0) (progn (setf r (/ r 2.0)) (setf k (- k 1)) ())))
      r)))
;; `decode-float`: the significand scaled into `[1/2,1)` and the exponent that
;; puts it back, as a pair. CL returns three values (significand, exponent,
;; sign); multiple values are not taken (language-design.md §0), so the sign
;; is `float-sign` and this pair carries the other two. The significand is
;; unsigned, as CL specifies.
(defmethod decode-float ((self f64)) cons-cell<f64,i32>
  (if (= self 0.0)
      (cons 0.0 0)
      (let ((m (abs self)) (e 0))
        (progn
          (while (>= m 1.0) (progn (setf m (/ m 2.0)) (setf e (+ e 1)) ()))
          (while (< m 0.5) (progn (setf m (* m 2.0)) (setf e (- e 1)) ()))
          (cons m e)))))
;; `integer-decode-float`: the same split with the significand as an exact
;; integer of `float-digits` bits, so `significand * 2^exponent` is the value
;; exactly. A `bignum`, since 53 bits do not fit an `i32`.
(defmethod integer-decode-float ((self f64)) cons-cell<bignum,i32>
  (let ((d (decode-float self)))
    (cons (float->bignum (scale-float (car d) 53)) (- (cdr d) 53))))

;; CL's `rationalize`: the *simplest* rational that reads back as exactly this
;; float — as against `float->ratio` (CL's `rational`), which is the exact
;; binary value. `(rationalize 0.1)` is `1/10`; `(float->ratio 0.1)` is
;; `3602879701896397/36028797018963968`.
;;
;; Continued fractions: successive convergents `h/k`, stopping at the first
;; one that reads back as `self`. The iteration count is capped because a
;; float that no convergent reproduces exactly (an infinity or a NaN) would
;; otherwise spin.
;;
;; The convergents are carried as `f64` holding integral values rather than as
;; integers, for the same reason `scale-float` loops: `int->float` has no
;; island lowering. Every convergent that matters is well under 2^53, so the
;; integers are exact, and `float->ratio` on an integral float is that integer
;; over 1 — which is how the answer is assembled at the end.
(defmethod rationalize ((self f64)) ratio
  (let ((h1 1.0) (h0 0.0) (k1 0.0) (k0 1.0) (b self)
        (out (int->ratio 0)) (go true) (guard 0))
    (progn
      (while go
        (let ((a (floor b)))
          (let ((h2 (+ (* a h1) h0)) (k2 (+ (* a k1) k0)))
            (progn
              (setf h0 h1) (setf h1 h2)
              (setf k0 k1) (setf k1 k2)
              (setf guard (+ guard 1))
              (if (or (= (/ h2 k2) self) (> guard 40))
                  (progn (setf out (/ (float->ratio h2) (float->ratio k2))) (setf go false) ())
                  (progn (setf b (/ 1.0 (- b a))) ()))))))
      out)))

;; CL's numeric limit constants (CLHS 12.1.4.2 / 12.1.3). "fixnum" here is the
;; immediate integer the runtime carries, which is an `i64` regardless of
;; whether a value's static type is `i32` or `i64` — so these are `i64`'s
;; bounds. `(- (* most-positive-fixnum -1) 1)` rather than the literal:
;; `-9223372036854775808` reads as the *bignum* 9223372036854775808 negated,
;; since the magnitude alone overflows `i64`.
(pub defconstant (most-positive-fixnum i64) 9223372036854775807)
(pub defconstant (most-negative-fixnum i64) (- (* most-positive-fixnum -1) 1))
(pub defconstant (most-positive-double-float f64) 1.7976931348623157e308)
(pub defconstant (most-negative-double-float f64) -1.7976931348623157e308)
(pub defconstant (least-positive-double-float f64) 5.0e-324)
(pub defconstant (least-negative-double-float f64) -5.0e-324)
(pub defconstant (least-positive-normalized-double-float f64) 2.2250738585072014e-308)
(pub defconstant (least-negative-normalized-double-float f64) -2.2250738585072014e-308)
;; CL defines `double-float-epsilon` as the smallest positive `e` with
;; `(/= (+ 1 e) 1)`, which is one ULP *above* 2^-53 rather than 2^-53 itself:
;; 2^-53 rounds back to 1.0 under round-to-nearest-even.
(pub defconstant (double-float-epsilon f64) 1.1102230246251568e-16)
(pub defconstant (double-float-negative-epsilon f64) 5.551115123125784e-17)

;; ---------------------------------------------------------------------------
;; The control forms of CL's chapter 5 that need no new machinery —
;; cl-parity-plan.md Phase 4a's macro-expressible half. (`block`/`return-from`,
;; `prog`/`prog*`, `destructuring-bind`, `remf` and `sleep` are the half that
;; does; see the plan for what each needs.)
;;
;; These expand to `setf`, which a `defmacro` may *produce* even though it may
;; not *call* one of the protected builtin forms (checker.rs's note by
;; `check_setf`): the expansion is handed back to the checker and checked
;; there, which is how `do` above already steps its variables.

;; `prog1`/`prog2`: evaluate everything, answer with the first (or second)
;; form's value. The `gensym` keeps that value from being re-evaluated.
(pub defmacro prog1 (first &rest rest)
  "`(prog1 form more...)` -- every form runs; the value is `form`'s."
  (let ((tmp (gensym))) `(let ((,tmp ,first)) ,@rest ,tmp)))
(pub defmacro prog2 (first second &rest rest)
  "`(prog2 a b more...)` -- every form runs; the value is `b`'s."
  (let ((tmp (gensym))) `(progn ,first (let ((,tmp ,second)) ,@rest ,tmp))))

;; `do*`: `do` with *sequential* binding and stepping. `do` above evaluates
;; every step against the old values before assigning any; `do*` assigns each
;; as it goes, so a later step sees the earlier ones already updated — which
;; is why this needs no temporaries at all and `do` needs one per binding.
(pub defmacro do* (bindings test-result &rest body)
  "`(do* ((var init step)...) (test result...) body...)` -- `do` with `let*`
   binding and in-order stepping."
  (let ((test (sexpr-car test-result))
        (result (sexpr-cdr test-result)))
    `(let* ,(sexpr-map (lambda ((b Sexpr)) Sexpr (list (sexpr-car b) (sexpr-car (sexpr-cdr b)))) bindings)
       (while (not ,test)
         ,@body
         ,@(sexpr-map (lambda ((b Sexpr)) Sexpr
                        (list (quote setf) (sexpr-car b) (sexpr-car (sexpr-cdr (sexpr-cdr b)))))
                      bindings))
       ,@result)))

;; `ecase`: `case` that requires a clause to match. CL signals a (correctable)
;; error; with no condition system the answer is a `panic`, which is what
;; `ecase` is *for* — saying "this really is exhaustive".
;;
;; `ccase` is `ecase` plus a restart letting the user supply a new value.
;; Restarts are not taken (language-design.md §9), and with no restart the two
;; are the same form — so `ccase` is defined as the same expansion rather than
;; left out, and `docs/functions.md` records that they coincide here.

;; `setq`: CL's variable-only assignment, and its multi-pair form. `setf` is
;; the general one here, so this is a spelling rather than a mechanism —
;; kept because ported CL is full of it.
(pub defmacro setq (&rest pairs)
  "`(setq var val ...)` -- CL's assignment. `setf` is the general form."
  (let ((out (quote ())) (rest pairs))
    (progn
      (while (not (sexpr-null rest))
        (progn
          (setf out (sexpr-append out (list (list (quote setf) (sexpr-car rest) (sexpr-car (sexpr-cdr rest))))))
          (setf rest (sexpr-cdr (sexpr-cdr rest)))))
      `(progn ,@out ()))))
;; `psetf`/`psetq`: the *parallel* versions — every value is computed before
;; any assignment happens, so `(psetq a b b a)` swaps. One expansion serves
;; both names, since `setf` already generalizes to places.
(pub defmacro psetf (&rest pairs)
  "`(psetf place val ...)` -- all values computed first, then assigned."
  (let ((tmps (quote ())) (sets (quote ())) (rest pairs))
    (progn
      (while (not (sexpr-null rest))
        (let ((g (gensym)))
          (progn
            (setf tmps (sexpr-append tmps (list (list g (sexpr-car (sexpr-cdr rest))))))
            (setf sets (sexpr-append sets (list (list (quote setf) (sexpr-car rest) g))))
            (setf rest (sexpr-cdr (sexpr-cdr rest))))))
      `(let ,tmps ,@sets ()))))
(pub defmacro psetq (&rest pairs)
  "CL's `psetq`. `psetf` with places restricted to variables; same expansion."
  `(psetf ,@pairs))

;; `pushnew`: push unless already present. A `defmethod` rather than a macro —
;; CL needs a macro because its `place` must be re-written, and a `Vector<T>`
;; mutates in place instead.
(defmethod pushnew ((self Vector<T>) (x T)) () (where (Eq T))
  (if (member x (iter self)) () (push self x)))

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
;;
;; **A generic type cannot use this yet.** `(impl print-object box<T> ...)`
;; type-checks and can be called by name, but the printer never finds it, so
;; the value still prints the built-in way. The printer looks a method up by
;; the type key the *value* carries, and monomorphization has erased the type
;; argument by then -- the key is `box`, not `box<i64>`, so there is nothing
;; in the value to choose `box<i64>`'s method by. Nor would one shared body
;; do: printing a `box<T>` means printing its `T`. See docs/dev/TODO.md.
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

;; The "what to print" controls (CLHS 22.1.1), read by `Interp::print_vars`
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

;; The rest of CLHS 22.1.1's "what to print" controls (cl-parity-plan.md
;; Phase 7b). Rebind them for one printing operation with `dlet`, which is
;; what CL's `let` on a special variable does.
;;
;; `*print-base*` is the radix integers (`i64` and `bignum`) print in, 2..36;
;; anything else is a printing error, as CL says it is. `*print-radix*` adds
;; the marker that makes the result read back as the same number whatever
;; `*read-base*` is: `#b`/`#o`/`#x` or `#NNr` before the sign, and a trailing
;; `.` in base 10.
(pub defvar (*print-base* i64) 10)
(pub defvar (*print-radix* bool) false)

;; `*print-case*` takes CL's own spelling: the keywords `:upcase`,
;; `:downcase` and `:capitalize`, which are self-evaluating `symbol`s here
;; (`Checker::check_symbol`). CL defaults to `:upcase` because its reader
;; stores symbol names upcased; this reader stores them downcased, so the
;; default that means the same thing ("print them as stored") is `:downcase`.
(pub defvar (*print-case* symbol) :downcase)

;; `*print-readably*` prints so the result reads back as an equal object:
;; escapes on whatever the caller asked for, and the `*print-level*`/
;; `*print-length*` cuts off. **CL's other half is missing on purpose**: CL
;; signals `print-not-readable` for a value with no readable form, and this
;; language has no condition to signal and no way to decide the question for
;; a type whose `print-object` may print anything at all.
(pub defvar (*print-readably* bool) false)

;; `*print-lines*` caps how many lines one pretty-printed value may take,
;; marking the cut with CL's `..`. A layout control like
;; `*print-right-margin*`, so it only bites while `*print-pretty*` is on; 0 or
;; less is CL's `nil` (no limit).
(pub defvar (*print-lines* i64) 0)

;; `*print-escape*` is the default `prin1`-vs-`princ` choice, and the one
;; thing `write`/`write-to-string` consult that the other printers do not.
;; CL says the same: `~s`/`prin1`/`pprint` bind it to true and `~a`/`princ` to
;; false for the extent of their own call, so it is only *read* where nobody
;; bound it — which is exactly `write`.
;;
;; A `print-object` method should read its own `escape` parameter rather than
;; this global: the parameter carries the per-call value the directive picked,
;; and this one only says what `write` starts from.
(pub defvar (*print-escape* bool) true)

;; ---------------------------------------------------------------------------
;; `random-state` (CLHS 12.1.6): a mutable PRNG stream. The actual
;; bit-twiddling (a fixed-width xorshift step) lives in Rust — see
;; `typelisp_rt::xorshift64_step` — since typelisp has no bitwise operators
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
           (format *trace-output* "Real time: ~,3f seconds~%"
                   (/ (int->float (- (get-internal-real-time) ,t0))
                      (int->float internal-time-units-per-second)))
           ,result)))))


;; ---------------------------------------------------------------------------
;; Pathnames (CLHS 19).
;;
;; A pathname is a file name taken apart: the directory components, the name,
;; the type (what most systems call the extension), and whether it starts at
;; the root. Taking it apart and putting it back together is pure string work,
;; so this whole layer is typelisp -- the native side never sees a pathname,
;; only the string `namestring` renders.
;;
;; Where CL accepts a *pathname designator* -- a string or a pathname --
;; wherever a file is named, this accepts any `Pathish`. Both types implement
;; it and every file operation below is generic over it, so `(open-input
;; "log/today.txt")` and `(open-input p)` are the same ordinary call with no
;; run-time test between them, and a `string` still costs no parse.
;;
;; Deliberately not all of CL's model: no host, device or version component,
;; no wild pathnames or `directory` matching, and no logical pathnames. Those
;; answer to filesystems this does not run on. The separator is `/`.

;; ---------------------------------------------------------------------------
;; Complex numbers (CL parity Phase 1d).
;;
;; **A prelude type, not a built-in one.** The plan called for the
;; `bignum`/`ratio` treatment — a heap-boxed `TAG_BOXED` pointer with Rust
;; arithmetic behind it — "following the precedent". The precedent does not
;; reach here: `bignum` and `ratio` are in Rust because `BigInt`/`BigRational`
;; arithmetic is not expressible in this language, and a complex over two
;; `f64`s is nothing but `f64` arithmetic. Written as a `defstruct` it needs no
;; new `Repr`, no `rt_*` shim, no island lowering and no artifact surgery, and
;; it compiles through the ordinary path the day it is written.
;;
;; **Two departures from CL, both forced by static typing:**
;;
;; 1. The components are `f64`. CL's complex can hold rationals, and
;;    `(complex 1 2)` is a *different type* from `(complex 1.0 2.0)`; a static
;;    type has to pick one, and the one the transcendental functions all
;;    produce is the float one.
;; 2. `(sqrt -1.0)` is still a real `sqrt` of a negative — NaN, as before. CL
;;    can return a complex from a real function because its `sqrt` answers a
;;    union type; here `sqrt` on `f64` has to return `f64`. Complex results
;;    come from complex arguments: `(sqrt (complex::new -1.0 0.0))` is `i`.
;;
;; The struct's fields are `pub` so `z::re` reads like `z::x` on any other
;; struct; `realpart`/`imagpart` are the CL spellings of the same reads.
(pub defstruct complex (pub re f64) (pub im f64))

;; CL's two-argument `atan`, which `phase` is built on. `(atan y x)` is sugar
;; for this (`Checker::check_list`, the same arity dispatch two-argument `log`
;; uses) — `defmethod` cannot overload on arity, and `atan` is a built-in `f64`
;; method already.
(pub defun atan2 ((y f64) (x f64)) f64
  "The angle of the vector `(x,y)`, in (-pi,pi]. CL's `(atan y x)`."
  (if (> x 0.0)
      (atan (/ y x))
      (if (< x 0.0)
          (if (>= y 0.0) (+ (atan (/ y x)) pi) (- (atan (/ y x)) pi))
          (if (> y 0.0)
              (/ pi 2.0)
              (if (< y 0.0) (* (/ pi 2.0) -1.0) 0.0)))))

;; CL's `complex` constructor. `complex::new` is the same thing spelled the way
;; every other struct is built.
(pub defun complex ((re f64) (im f64)) complex (complex::new re im))

;; `realpart`/`imagpart` answer for reals too, exactly as in CL: every real is
;; a complex whose imaginary part is zero. That is what lets a caller take the
;; real part of something without knowing which it has.
(pub defmethod realpart ((self complex)) f64 self::re)
(pub defmethod imagpart ((self complex)) f64 self::im)
(pub defmethod realpart ((self f64)) f64 self)
(pub defmethod imagpart ((self f64)) f64 0.0)

(pub defmethod conjugate ((self complex)) complex (complex::new self::re (* self::im -1.0)))
(pub defmethod conjugate ((self f64)) f64 self)

(pub defmethod phase ((self complex)) f64
  "The angle of `self` in the complex plane, in (-pi,pi]."
  (atan2 self::im self::re))
(pub defmethod phase ((self f64)) f64 (if (< self 0.0) pi 0.0))

(pub defun cis ((theta f64)) complex
  "`e^(i*theta)` — the unit complex at angle `theta`."
  (complex::new (cos theta) (sin theta)))

;; Arithmetic. These are `defmethod`s named for the operators, which is allowed
;; because `complex` is not a primitive with a built-in table of its own —
;; the reason `impl Add i32` cannot name its method `+` (see the arithmetic
;; traits above) does not apply here.
(pub defmethod + ((self complex) (other complex)) complex
  (complex::new (+ self::re other::re) (+ self::im other::im)))
(pub defmethod - ((self complex) (other complex)) complex
  (complex::new (- self::re other::re) (- self::im other::im)))
(pub defmethod * ((self complex) (other complex)) complex
  (complex::new (- (* self::re other::re) (* self::im other::im))
                (+ (* self::re other::im) (* self::im other::re))))
(pub defmethod / ((self complex) (other complex)) complex
  (let ((d (+ (* other::re other::re) (* other::im other::im))))
    (complex::new (/ (+ (* self::re other::re) (* self::im other::im)) d)
                  (/ (- (* self::im other::re) (* self::re other::im)) d))))

(pub defmethod = ((self complex) (other complex)) bool
  (if (= self::re other::re) (= self::im other::im) false))
(pub defmethod /= ((self complex) (other complex)) bool (not (= self other)))
(impl Eq complex (equals ((self Self) (other Self)) bool (= self other)))
;; No `Ord`: the complex numbers are not ordered, and CL's `<` rejects them
;; for the same reason.

(pub defmethod abs ((self complex)) f64
  "The modulus. A *real*, as in CL — the one `abs` whose result is not the
   receiver's own type."
  (sqrt (+ (* self::re self::re) (* self::im self::im))))

(pub defmethod zerop ((self complex)) bool
  (if (= self::re 0.0) (= self::im 0.0) false))

(pub defmethod exp ((self complex)) complex
  (let ((m (exp self::re)))
    (complex::new (* m (cos self::im)) (* m (sin self::im)))))

(pub defmethod log ((self complex)) complex
  "The principal branch: `log|z| + i*phase(z)`."
  (complex::new (log (abs self)) (phase self)))

(pub defmethod sqrt ((self complex)) complex
  "The principal square root, from the half-angle form."
  (let ((m (sqrt (abs self))) (h (/ (phase self) 2.0)))
    (complex::new (* m (cos h)) (* m (sin h)))))

(pub defmethod expt ((self complex) (power complex)) complex
  "`z^w` as `exp(w * log z)`. `(expt 0 0)` is 1, as in CL."
  (if (zerop self)
      (if (zerop power) (complex::new 1.0 0.0) (complex::new 0.0 0.0))
      (exp (* power (log self)))))

;; CL prints a complex as `#C(re im)`, and reads it back the same way — the
;; reader half is not here (this language has no `#C` syntax), so `escape`
;; changes nothing.
(impl print-object complex
  (print-object ((self Self) (escape bool)) string
    (format false "#C(~a ~a)" self::re self::im)))

(pub defstruct pathname
  (directory Vector<string>)
  (name Option<string>)
  ;; CL's "type". Named `extension` here to keep the field out of the way of
  ;; `(type ...)`, which is `impl` syntax; `pathname-type` is the reader.
  (extension Option<string>)
  (absolutep bool))

(defun split-on-slash ((s string)) Vector<string>
  "Every `/`-separated piece, empty ones included -- so a leading `/` yields a
   leading empty piece, and a trailing one a trailing empty piece."
  (let ((out (the Vector<string> (Vector::new))) (cur "") (i 0) (n (length s)))
    (progn
      (while (< i n)
        (let ((c (ref s i)))
          (progn
            (if (equal c #\/)
                (progn (push out cur) (setf cur "") ())
                (progn (setf cur (append cur (char->string c))) ()))
            (setf i (+ i 1))
            ())))
      (push out cur)
      out)))

(defun name-type-dot ((file string)) i32
  "Where the `.` separating name from type is, or -1. The *last* dot, and
   never the first character: `archive.tar.gz` is `archive.tar` of type `gz`,
   while `.gitignore` is all name, as in CL."
  (let ((at -1) (i 1) (n (length file)))
    (progn
      (while (< i n)
        (progn
          (if (equal (ref file i) #\.) (progn (setf at i) ()) ())
          (setf i (+ i 1))
          ()))
      at)))

(pub defun parse-namestring ((s string)) pathname
  "Take a file name apart. A trailing `/` (or an empty name) means a pathname
   with no name -- a directory."
  (let ((parts (split-on-slash s))
        (dirs (the Vector<string> (Vector::new)))
        (file "")
        (i 0))
    (progn
      (let ((n (len parts)))
        (while (< i n)
          (let ((seg (get parts i)))
            (progn
              (if (= i (- n 1))
                  (progn (setf file seg) ())
                  ;; Empty pieces are the leading `/` and any doubled one:
                  ;; both say something about the shape, nothing about a
                  ;; directory name.
                  (if (equal seg "") () (progn (push dirs seg) ())))
              (setf i (+ i 1))
              ()))))
      (pathname::new
        dirs
        (if (equal file "")
            (option::none)
            (let ((at (name-type-dot file)))
              (if (< at 0) (option::some file) (option::some (substring file 0 at)))))
        (let ((at (name-type-dot file)))
          (if (< at 0) (option::none) (option::some (substring file (+ at 1) (length file)))))
        (and (> (length s) 0) (equal (ref s 0) #\/))))))

(defun pathname-file-part ((p pathname)) string
  "`name.type`, as text -- \"\" for a pathname naming only a directory."
  (append
    (match p::name ((some x) x) ((none) ""))
    (match p::extension ((some x) (append "." x)) ((none) ""))))

(defun pathname-directory-part ((p pathname)) string
  "The directory components, each with its `/`, after a leading `/` if the
   pathname is absolute."
  (let ((out (if p::absolutep "/" "")) (i 0) (n (len p::directory)))
    (progn
      (while (< i n)
        (progn
          (setf out (append (append out (get p::directory i)) "/"))
          (setf i (+ i 1))
          ()))
      out)))

;; The pathname designator. Two implementations, and no more are expected:
;; the point is that a file operation can take either without asking which.
(deftrait Pathish ()
  "What can name a file: a `string` or a `pathname`."
  (namestring ((self Self)) string)
  (to-pathname ((self Self)) pathname))

(impl Pathish string
  ;; A string is already its own namestring -- the whole reason file
  ;; operations take `Pathish` rather than `pathname` is that this case costs
  ;; nothing.
  (namestring ((self Self)) string self)
  (to-pathname ((self Self)) pathname (parse-namestring self)))

(impl Pathish pathname
  (namestring ((self Self)) string (append (pathname-directory-part self) (pathname-file-part self)))
  (to-pathname ((self Self)) pathname self))

(pub defun make-pathname (&key (directory Vector<string> (Vector::new))
                               (name string)
                               (type string)
                               (absolute bool false)) pathname
  "Build a pathname from the components you have. Omitted `name`/`type` stay
   absent, which is what `merge-pathnames` fills in."
  (pathname::new directory name type absolute))

(pub defun pathname-directory<P> ((p P)) Vector<string> (where (Pathish P))
  "The directory components, outermost first."
  (let ((q (to-pathname p))) q::directory))

(pub defun pathname-name<P> ((p P)) Option<string> (where (Pathish P))
  "The name, without the type. `none` for a pathname naming a directory."
  (let ((q (to-pathname p))) q::name))

(pub defun pathname-type<P> ((p P)) Option<string> (where (Pathish P))
  "The type -- the extension after the last dot."
  (let ((q (to-pathname p))) q::extension))

(pub defun pathname-absolute-p<P> ((p P)) bool (where (Pathish P))
  "Whether it starts at the root."
  (let ((q (to-pathname p))) q::absolutep))

(pub defun directory-namestring<P> ((p P)) string (where (Pathish P))
  "Everything up to and including the last `/`."
  (pathname-directory-part (to-pathname p)))

(pub defun file-namestring<P> ((p P)) string (where (Pathish P))
  "The `name.type` part alone."
  (pathname-file-part (to-pathname p)))

(pub defun merge-pathnames<P,D> ((p P) (default D)) pathname (where (Pathish P) (Pathish D))
  "Fill in whatever `p` leaves out from `default`, as CL's does: a missing
   name or type is taken over, and a *relative* `p` is placed under
   `default`'s directory. An absolute `p` keeps its own directory."
  (let ((a (to-pathname p)) (b (to-pathname default)))
    (pathname::new
      (if a::absolutep
          a::directory
          (let ((dirs (the Vector<string> (Vector::new))))
            (progn
              (doiter (d (iter b::directory)) (push dirs d))
              (doiter (d (iter a::directory)) (push dirs d))
              dirs)))
      (match a::name ((some x) (option::some x)) ((none) b::name))
      (match a::extension ((some x) (option::some x)) ((none) b::extension))
      (if a::absolutep true b::absolutep))))

(pub defun enough-namestring<P,D> ((p P) (default D)) string (where (Pathish P) (Pathish D))
  "As much of `p` as it takes to name it relative to `default` -- `p` with
   `default`'s directory prefix removed, or all of `p` when it does not start
   there."
  (let ((full (namestring p)) (prefix (directory-namestring default)))
    (if (and (> (length prefix) 0)
             (<= (length prefix) (length full))
             (equal (substring full 0 (length prefix)) prefix))
        (substring full (length prefix) (length full))
        full)))

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
;; The native layer (`typelisp_rt::stream`) knows only about leaf backends -- files,
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
  ;; `read-item` yields the next item, or `none` at end of input. Said in a
  ;; comment because a bodyless signature cannot carry a docstring: a trailing
  ;; string *is* the default body (syntax.md §deftrait), and one typed
  ;; `string` where `Option<Item>` is declared -- which is exactly what this
  ;; docstring used to be, unnoticed until `deftrait` began checking its
  ;; default bodies at the declaration.
  (read-item ((self Self)) Option<Item>))

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

;; Pushback. Separate from `CharInput` because it is the one input operation
;; that cannot have a default body: putting a character back needs somewhere
;; to put it, and only the stream has that. The native-backed streams keep
;; theirs in the native layer; anything else gets pushback by wrapping
;; (`make-peek-stream`, below).
;;
;; This is what `read-sexpr` needs and `read-char`/`read-line` do not: finding
;; where an atom ends means looking at the character *after* it, which the
;; next read must still see.
(deftrait PeekInput (CharInput)
  "A character input stream that can put a character back. One is enough --
   that is all CL's `unread-char` promises."
  (unread-char ((self Self) (c char)) ())
  (peek-char ((self Self)) Option<char>
    "The next character, without consuming it."
    (match (read-char self)
      ((none) (option::none))
      ((some c) (progn (unread-char self c) (option::some c))))))

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
  (at-line-start ((self Self)) bool
    "Whether the next character written would begin a line. Answering this
     needs a memory of what was written last, which only the stream itself
     has -- so the default is `false`, the answer that makes `fresh-line`
     write its newline. A stream that would rather not have that newline
     overrides this (every built-in one does)."
    false)
  (fresh-line ((self Self)) ()
    "A newline, unless the stream is already at the start of a line."
    (if (at-line-start self) () (write-item self #\newline)))
  (finish-output ((self Self)) ()
    "Push buffered output to its destination. A no-op unless overridden."
    ()))

;; The byte layers, the other half of what `InputStream`/`OutputStream` left
;; `Item` open for. CL reaches these by opening a file with
;; `:element-type '(unsigned-byte 8)`; here the element type is the stream's
;; type, so a byte file is its own type and the question is settled where the
;; value is bound.
;;
;; `Item` is `i64` rather than a byte type this language does not have. The
;; value is always 0..255 -- `write-byte` refuses anything else, as CL's does.
(deftrait ByteInput ((InputStream (Item i64)))
  "A byte input stream. Every method has a default body."
  (read-byte ((self Self)) Option<i64>
    "The next byte, or `none` at end of input."
    (read-item self)))

(deftrait ByteOutput ((OutputStream (Item i64)))
  "A byte output stream. Every method has a default body."
  (write-byte ((self Self) (b i64)) ()
    "Write one byte. `b` outside 0..255 is an error."
    (write-item self b))
  (finish-output ((self Self)) ()
    "Push buffered output to its destination. A no-op unless overridden."
    ()))

;; ---------------------------------------------------------------------------
;; The native-backed stream types. Each is a struct around one handle; the
;; field is deliberately not `pub`.

(pub defstruct file-stream (h i64))
(pub defstruct binary-file-stream (h i64))
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
(impl PeekInput file-stream
  (unread-char ((self Self) (c char)) () (unwrap-io (stream-unread-char self::h c))))
(impl CharOutput file-stream
  ;; Overridden: one native call per string beats one per character, and this
  ;; is the stream people write bulk output to.
  (write-string ((self Self) (s string)) ()
    (unwrap-io (stream-write-string self::h s)))
  (at-line-start ((self Self)) bool (unwrap-io (stream-at-line-start self::h)))
  (finish-output ((self Self)) () (unwrap-io (stream-finish-output self::h))))

(impl Stream binary-file-stream
  (open-stream-p ((self Self)) bool (stream-open-p self::h))
  (close ((self Self)) () (unwrap-io (stream-close self::h))))
(impl InputStream binary-file-stream
  (type Item i64)
  (read-item ((self Self)) Option<i64> (unwrap-io (stream-read-byte self::h))))
(impl OutputStream binary-file-stream
  (type Item i64)
  (write-item ((self Self) (b i64)) () (unwrap-io (stream-write-byte self::h b))))
(impl ByteInput binary-file-stream)
(impl ByteOutput binary-file-stream
  (finish-output ((self Self)) () (unwrap-io (stream-finish-output self::h))))

(impl Stream string-input-stream
  (open-stream-p ((self Self)) bool (stream-open-p self::h))
  (close ((self Self)) () (unwrap-io (stream-close self::h))))
(impl InputStream string-input-stream
  (type Item char)
  (read-item ((self Self)) Option<char> (unwrap-io (stream-read-char self::h))))
(impl CharInput string-input-stream)
(impl PeekInput string-input-stream
  (unread-char ((self Self) (c char)) () (unwrap-io (stream-unread-char self::h c))))

(impl Stream string-output-stream
  (open-stream-p ((self Self)) bool (stream-open-p self::h))
  (close ((self Self)) () (unwrap-io (stream-close self::h))))
(impl OutputStream string-output-stream
  (type Item char)
  (write-item ((self Self) (c char)) ()
    (unwrap-io (stream-write-string self::h (char->string c)))))
(impl CharOutput string-output-stream
  (write-string ((self Self) (s string)) ()
    (unwrap-io (stream-write-string self::h s)))
  (at-line-start ((self Self)) bool (unwrap-io (stream-at-line-start self::h))))

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
(impl PeekInput standard-stream
  (unread-char ((self Self) (c char)) () (unwrap-io (stream-unread-char self::h c))))
(impl CharOutput standard-stream
  (write-string ((self Self) (s string)) ()
    (unwrap-io (stream-write-string self::h s)))
  (at-line-start ((self Self)) bool (unwrap-io (stream-at-line-start self::h)))
  (finish-output ((self Self)) () (unwrap-io (stream-finish-output self::h))))

;; CL's standard streams. Ordinary assignable globals rather than dynamically
;; bound specials — `(setf *standard-output* s)` does globally what CL's
;; `(let ((*standard-output* s)) ...)` does locally, and `dlet` (Phase 7b) does
;; the scoped version.
;;
;; **Concretely typed, so a rebinding can only be another `standard-stream`.**
;; Typing them `:dyn CharOutput` — which is what would let a program redirect
;; output into a string stream for the extent of one form, the thing CL
;; programs use dynamic binding for — does not work *yet*, and the obstacle is
;; not in the type system: a prelude global's initializer is a compiled body,
;; and a compiled body in the shipped artifact may not *create* a trait object.
;; `dyn-new` bakes a vtable id and `dyn-upcast` a trait id, both assigned per
;; site during translation, and the artifact has no startup sequence to replay
;; that numbering or publish the vtable addresses (see the check at the end of
;; `compile::prelude_bootstrap::build_dump`). Dispatching on an already-boxed
;; `:dyn` is fine, which is why the composed streams below work.
(pub defvar (*standard-input*  standard-stream) (standard-stream::new (stream-stdin)))
(pub defvar (*standard-output* standard-stream) (standard-stream::new (stream-stdout)))
(pub defvar (*error-output*    standard-stream) (standard-stream::new (stream-stderr)))

;; CL's `*trace-output*`: where a program's own progress reporting goes, as
;; opposed to its output (`*standard-output*`) or its errors (`*error-output*`).
;; `time` writes its report here, which is what CL specifies.
;;
;; CL's other three stream variables — `*terminal-io*`, `*query-io*` and
;; `*debug-io*` — are *two-way* streams, and a `two-way-stream` is built by
;; upcasting its two halves to `:dyn CharInput`/`:dyn CharOutput`. That is
;; exactly the trait-object creation a prelude body may not do (see the note
;; above), so those three are not here. A program can build its own
;; `(make-two-way-stream ...)` and pass it around; only the *prelude* is
;; barred from holding one in a global.
(pub defvar (*trace-output* standard-stream) (standard-stream::new (stream-stdout)))

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
  ;; Asked of each component separately rather than derived from
  ;; `at-line-start`: the components need not agree, and one newline decided
  ;; for all of them would put a blank line into whichever were already at a
  ;; line start.
  (fresh-line ((self Self)) ()
    (doiter (p (iter self::parts)) (fresh-line p)))
  ;; Only true when every component says so -- a broadcast stream is at the
  ;; start of a line when writing to it would begin a line everywhere.
  (at-line-start ((self Self)) bool
    (let ((all true))
      (progn
        (doiter (p (iter self::parts)) (if (at-line-start p) () (progn (setf all false) ())))
        all)))
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
  (at-line-start ((self Self)) bool (at-line-start self::out))
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

;; Gives any input stream one character of pushback, so that a stream without
;; pushback of its own (a composite, or a user type) can still be `read` from.
;; A composite like every other: a struct, one field of state, no native
;; support.
(pub defstruct peek-stream (inner :dyn CharInput) (pending Option<char>))
(impl Stream peek-stream
  (open-stream-p ((self Self)) bool (open-stream-p self::inner))
  (close ((self Self)) () (close self::inner)))
(impl InputStream peek-stream
  (type Item char)
  (read-item ((self Self)) Option<char>
    (match self::pending
      ((some c) (progn (setf self::pending (option::none)) (option::some c)))
      ((none) (read-char self::inner)))))
(impl CharInput peek-stream)
(impl PeekInput peek-stream
  ;; A second `unread-char` without a read in between overwrites the first --
  ;; the one-character promise, and what a program that keeps to it never
  ;; notices.
  (unread-char ((self Self) (c char)) () (progn (setf self::pending (option::some c)) ())))

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

(pub defun make-peek-stream ((in :dyn CharInput)) peek-stream
  "`in` with one character of pushback added, so it can be `read-sexpr`-ed.
   The built-in leaf streams already have pushback and need no wrapping."
  (peek-stream::new in (option::none)))

;; `open`'s direction, as named constants rather than as CL's `:direction`
;; keyword argument: the direction is required, and three constants read at
;; least as well as `:direction :output` when there is nothing to default.
;; (`&key` does exist -- `make-pathname` uses it, where most components are
;; genuinely optional.)
(pub defconstant (direction-input i64) 0)
(pub defconstant (direction-output i64) 1)
(pub defconstant (direction-append i64) 2)

(pub defun open-file<P> ((name P) (direction i64)) Result<file-stream, FileError> (where (Pathish P))
  "Open `name` -- a string or a `pathname` -- in one of `direction-input` /
   `direction-output` / `direction-append`. `Err` if the file cannot be
   opened: a missing file is an ordinary outcome, not a panic."
  (match (stream-open-file (namestring name) direction)
    ((ok h) (result::ok (file-stream::new h)))
    ((err e) (result::err e))))

(pub defun open-input<P> ((name P)) Result<file-stream, FileError> (where (Pathish P))
  (open-file name direction-input))

;; The byte-stream openers. CL writes these as `open` with
;; `:element-type '(unsigned-byte 8)`; the difference here is the *type* of
;; what comes back, so it is the opener that differs.
(pub defun open-binary<P> ((name P) (direction i64)) Result<binary-file-stream, FileError> (where (Pathish P))
  "Open `name` for byte I/O in one of `direction-input` / `direction-output` /
   `direction-append`."
  (match (stream-open-file (namestring name) direction)
    ((ok h) (result::ok (binary-file-stream::new h)))
    ((err e) (result::err e))))

(pub defun open-binary-input<P> ((name P)) Result<binary-file-stream, FileError> (where (Pathish P))
  (open-binary name direction-input))

(pub defun open-binary-output<P> ((name P)) Result<binary-file-stream, FileError> (where (Pathish P))
  (open-binary name direction-output))

(pub defun open-output<P> ((name P)) Result<file-stream, FileError> (where (Pathish P))
  (open-file name direction-output))

;; CL's `input-stream-p`/`output-stream-p` have no counterpart here, and do
;; not need one: direction is part of the type. A value that can be read from
;; implements `CharInput`, so the question is settled where the value is
;; bound, not asked again at run time.

;; `fresh-line` is a `CharOutput` method (with `at-line-start`), not a
;; function over one concrete type: the memory of what was written last
;; belongs to the stream, so the stream is what answers. The native-backed
;; ones answer from the native layer's `last_written`, the composites ask
;; their components, and a stream that cannot tell takes the default `false`
;; and gets its newline.

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
;; The one-object printers (CLHS 22.1.3) -- cl-parity-plan.md Phase 8a.
;;
;; `prin1` writes reader syntax (`~s`), `princ` writes it for a person (`~a`),
;; and `write` picks between the two by `*print-escape*`. The `-to-string`
;; three are the same choices with the string kept instead of written. Each of
;; the first three returns its object, as CL's do, so a call can sit inside a
;; larger expression.
;;
;; **`print`/`println` are not these.** They take a control string and are this
;; language's `format` shorthand -- a different job from CL's `print` (fresh
;; line, `prin1`, space). Both stay under the names they already have, so CL's
;; one-argument `print` has no spelling here; write `prin1`, which is what it
;; prints with.
;;
;; Macros rather than functions because the object's type has to be concrete
;; where the formatting happens: `format`'s `&rest` rejects a type variable
;; outright (the same reason `to-string` above is a method per scalar type
;; rather than one generic `defun`), and a macro is checked at the call site,
;; where the type is known. The stream is optional, as in CL, and defaults to
;; `*standard-output*`.

(pub defmacro prin1 (x &rest stream)
  "Write `x` in reader syntax to `stream` (default `*standard-output*`) and
   return it. CL's `prin1`; `~s` is the same rendering."
  (let ((v (gensym)))
    (if (sexpr-null stream)
        `(let ((,v ,x)) (progn (format *standard-output* "~s" ,v) ,v))
        `(let ((,v ,x)) (progn (format ,(sexpr-car stream) "~s" ,v) ,v)))))

(pub defmacro princ (x &rest stream)
  "Write `x` for a person to `stream` (default `*standard-output*`) and return
   it. CL's `princ`; `~a` is the same rendering."
  (let ((v (gensym)))
    (if (sexpr-null stream)
        `(let ((,v ,x)) (progn (format *standard-output* "~a" ,v) ,v))
        `(let ((,v ,x)) (progn (format ,(sexpr-car stream) "~a" ,v) ,v)))))

(pub defmacro write (x &rest stream)
  "Write `x` to `stream` (default `*standard-output*`) the way
   `*print-escape*` says, and return it. CL's `write`, minus its keyword
   arguments -- rebind the control variables with `dlet` instead, which is
   what those keywords are shorthand for."
  (let ((v (gensym)))
    (if (sexpr-null stream)
        `(let ((,v ,x))
           (progn (if *print-escape*
                      (format *standard-output* "~s" ,v)
                      (format *standard-output* "~a" ,v))
                  ,v))
        `(let ((,v ,x))
           (progn (if *print-escape*
                      (format ,(sexpr-car stream) "~s" ,v)
                      (format ,(sexpr-car stream) "~a" ,v))
                  ,v)))))

(pub defmacro prin1-to-string (x)
  "`x` in reader syntax, as a string. CL's `prin1-to-string`."
  `(format false "~s" ,x))

(pub defmacro princ-to-string (x)
  "`x` rendered for a person, as a string. CL's `princ-to-string`. The
   receiver-first spelling of the same thing is `to-string`."
  `(format false "~a" ,x))

(pub defmacro write-to-string (x)
  "`x` as a string, the way `*print-escape*` says. CL's `write-to-string`."
  (let ((v (gensym)))
    `(let ((,v ,x))
       (if *print-escape* (format false "~s" ,v) (format false "~a" ,v)))))

;; ---------------------------------------------------------------------------
;; `read` over a stream (CL's `read`; typelisp's own `read` takes a string,
;; which is CL's `read-from-string`).
;;
;; The parsing is not repeated here: these functions only find where the next
;; datum *ends* -- consuming its exact text, character by character -- and
;; hand that text to `read`. That split is the whole design. A second reader
;; written in typelisp would be a second grammar to keep in step with
;; `src/read/reader.rs`; a scanner only has to know which characters close
;; what they opened, which is far less to get wrong, and it reports every
;; syntax error in the one voice the reader already speaks.
;;
;; Text is accumulated *verbatim*, whitespace and comments included, so what
;; `read` parses is what was written -- including the spellings whose meaning
;; depends on their exact spacing (a `Vector<:dyn CharOutput>` type token
;; reads as one symbol only while it stays on one line).

(defun reader-whitespacep ((c char)) bool
  (or (equal c #\space) (equal c #\newline) (equal c #\tab) (equal c #\return) (equal c #\page)))

;; The reader's own token terminators (`is_delimiter` in `src/read/reader.rs`).
;; `,` is deliberately absent from both: it stays usable inside a token so
;; that `Pair<K,V>` reads as one symbol.
(defun reader-delimiterp ((c char)) bool
  (or (reader-whitespacep c)
      (equal c #\() (equal c #\)) (equal c #\") (equal c #\') (equal c #\`) (equal c #\;)))

(defun reader-skip-one<S> ((s S)) () (where (CharInput S))
  "Consume one character, discarding it -- always used where it has just been
   peeked at, so there is nothing to report."
  (match (read-char s) (_ ())))

(defun reader-stopp ((stop Option<char>) (c char)) bool
  "Whether `c` is the caller-supplied extra delimiter. `read-delimited-list`
   is the only caller that has one: CL gets the same effect by making the
   terminator a *terminating macro character* in the readtable, and without
   readtables (Phase 8c) the scanner has to be told directly. Without it,
   `(read-delimited-list #\\] s)` over `1]x` scans `1]x` as one atom."
  (match stop
    ((none) false)
    ((some x) (equal c x))))

(defun reader-scan-atom-until<S> ((s S) (stop Option<char>)) string (where (PeekInput S))
  "An atom's characters, up to (not including) whatever ends it. The
   terminator is put back -- that, and only that, is why `read-sexpr` needs
   `PeekInput` rather than plain `CharInput`."
  (let ((out ""))
    (loop
      (match (peek-char s)
        ((none) (break))
        ((some c)
         (if (or (reader-delimiterp c) (reader-stopp stop c))
             (break)
             (progn (reader-skip-one s) (setf out (append out (char->string c))) ())))))
    out))

(defun reader-scan-atom<S> ((s S)) string (where (PeekInput S))
  "An atom's characters, with only the reader's own delimiters ending it."
  (reader-scan-atom-until s (the Option<char> (option::none))))

(defun reader-scan-name<S> ((s S)) string (where (PeekInput S))
  "The rest of a `#\\newline`-style character name: alphanumerics and `-`."
  (let ((out ""))
    (loop
      (match (peek-char s)
        ((none) (break))
        ((some c)
         (if (or (alphap c) (digitp c) (equal c #\-))
             (progn (reader-skip-one s) (setf out (append out (char->string c))) ())
             (break)))))
    out))

(defun reader-scan-string<S> ((s S)) string (where (PeekInput S))
  "A string literal, from just after the opening quote through the closing
   one (which is returned with it). An unterminated one runs to end of input
   and `read` reports it."
  (let ((out "\""))
    (loop
      (match (read-char s)
        ((none) (break))
        ((some c)
         (progn
           (setf out (append out (char->string c)))
           (cond
             ((equal c #\") (break))
             ;; An escaped character is taken verbatim, so an escaped quote
             ;; does not end the literal.
             ((equal c #\\)
              (match (read-char s)
                ((none) (break))
                ((some e) (progn (setf out (append out (char->string e))) ()))))
             (else ()))))))
    out))

(defun reader-skip-block-comment<S> ((s S)) () (where (PeekInput S))
  "A `#| ... |#` comment, from just after the opening `#|`. Nests, as the
   reader's does."
  (let ((depth 1))
    (loop
      (if (= depth 0) (break) ())
      (match (read-char s)
        ((none) (break))
        ((some c)
         (cond
           ((equal c #\#)
            (match (peek-char s)
              ((some n) (if (equal n #\|) (progn (reader-skip-one s) (setf depth (+ depth 1)) ()) ()))
              ((none) ())))
           ((equal c #\|)
            (match (peek-char s)
              ((some n) (if (equal n #\#) (progn (reader-skip-one s) (setf depth (- depth 1)) ()) ()))
              ((none) ())))
           (else ())))))))

(defun reader-scan-atmosphere<S> ((s S)) string (where (PeekInput S))
  "Whitespace and `;` line comments, verbatim. Stops at `#|`, which
   `reader-scan-datum` handles: telling `#|` from `#\\(` takes the `#`
   consumed first, and there is only one character of pushback."
  (let ((out ""))
    (loop
      (match (peek-char s)
        ((none) (break))
        ((some c)
         (cond
           ((reader-whitespacep c)
            (progn (reader-skip-one s) (setf out (append out (char->string c))) ()))
           ((equal c #\;)
            (loop
              (match (read-char s)
                ((none) (break))
                ((some d)
                 (progn
                   (setf out (append out (char->string d)))
                   (if (equal d #\newline) (break) ()))))))
           (else (break))))))
    out))

(defun reader-scan-hash<S> ((s S) (stop Option<char>)) string (where (PeekInput S))
  "A `#`-token, from a peeked `#`. Returns \"\" for `#| ... |#`, which is a
   comment rather than a datum -- the caller keeps scanning."
  (progn
    (reader-skip-one s)
    (match (peek-char s)
      ((none) "#")
      ((some c)
       (cond
         ((equal c #\|) (progn (reader-skip-one s) (reader-skip-block-comment s) ""))
         ((equal c #\\)
          (progn
            (reader-skip-one s)
            (match (read-char s)
              ((none) "#\\")
              ((some first)
               (let ((out (append "#\\" (char->string first))))
                 ;; A multi-letter name only when it starts alphabetic --
                 ;; the reader's own rule, and what makes `#\(` end here.
                 (if (alphap first) (append out (reader-scan-name s)) out))))))
         ;; Any other `#` syntax is not one the reader has; scan it as a token
         ;; and let `read` say so.
         (else (append "#" (reader-scan-atom-until s stop))))))))

(defun reader-scan-datum-until<S> ((s S) (stop Option<char>)) string (where (PeekInput S))
  "The exact text of the next datum on `s`, or \"\" at end of input.
   Consumes the datum and the whitespace/comments before it, and nothing
   after it but the one character that ends an atom, which is put back."
  (let ((out "") (depth 0) (wanted true) (going true))
    ;; `depth` counts open parens; `wanted` says a datum is still owed -- true
    ;; at the start and after a `'`/`` ` ``/`,` prefix, which is what keeps
    ;; `'(1 2)` from stopping at the quote. The scan ends when a datum has
    ;; been taken at depth 0.
    (while going
      (progn
        (let ((skipped (reader-scan-atmosphere s)))
          ;; Leading atmosphere is dropped, interior atmosphere kept: the text
          ;; handed to `read` starts at the datum but is otherwise as written.
          (if (equal out "") () (progn (setf out (append out skipped)) ())))
        (match (peek-char s)
          ((none) (progn (setf going false) ()))
          ((some c)
           (cond
             ;; The caller's extra delimiter ends the scan without being
             ;; consumed -- but only where a datum could start, so a `]`
             ;; inside `(1 2]` still belongs to the list's own text and is
             ;; reported by `read` as the malformed list it is.
             ((and (reader-stopp stop c) (= depth 0))
              (progn (setf going false) ()))
             ((equal c #\()
              (progn (reader-skip-one s) (setf out (append out "(")) (setf depth (+ depth 1)) (setf wanted false) ()))
             ((equal c #\))
              (progn (reader-skip-one s) (setf out (append out ")")) (setf depth (- depth 1)) (setf wanted false) ()))
             ((equal c #\")
              (progn (reader-skip-one s) (setf out (append out (reader-scan-string s))) (setf wanted false) ()))
             ((or (equal c #\') (equal c #\`))
              (progn (reader-skip-one s) (setf out (append out (char->string c))) (setf wanted true) ()))
             ((equal c #\,)
              (progn
                (reader-skip-one s)
                (setf out (append out ","))
                ;; `,@` is one prefix, not `,` followed by an `@` atom.
                (match (peek-char s)
                  ((some n) (if (equal n #\@) (progn (reader-skip-one s) (setf out (append out "@")) ()) ()))
                  ((none) ()))
                (setf wanted true)
                ()))
             ((equal c #\#)
              (let ((text (reader-scan-hash s stop)))
                (if (equal text "")
                    ()                                  ; a block comment: no datum yet
                    (progn (setf out (append out text)) (setf wanted false) ()))))
             (else (progn (setf out (append out (reader-scan-atom-until s stop))) (setf wanted false) ())))))
        (if (and (= depth 0) (not wanted)) (progn (setf going false) ()) ())))
    out))

(defun reader-scan-datum<S> ((s S)) string (where (PeekInput S))
  "The next datum's text, with only the reader's own delimiters ending it."
  (reader-scan-datum-until s (the Option<char> (option::none))))

(pub defun read-sexpr-preserving-whitespace<S> ((s S)) Result<Option<Sexpr>, ReadError> (where (PeekInput S))
  "Read one datum from `s`, leaving everything after it untouched -- CL's
   `read-preserving-whitespace`. `Ok(none)` at end of input (so a read loop
   ends on a value rather than an error), `Err` if what is there is not a
   datum."
  (let ((text (reader-scan-datum s)))
    (if (equal text "")
        (result::ok (option::none))
        (match (read text)
          ((ok v) (result::ok (option::some v)))
          ((err e) (result::err e))))))

(defun reader-read-one-until<S> ((s S) (stop Option<char>)) Result<Option<Sexpr>, ReadError> (where (PeekInput S))
  "`read-sexpr-preserving-whitespace` with an extra delimiter -- what
   `read-delimited-list` reads each element with."
  (let ((text (reader-scan-datum-until s stop)))
    (if (equal text "")
        (result::ok (option::none))
        (match (read text)
          ((ok v) (result::ok (option::some v)))
          ((err e) (result::err e))))))

(pub defun read-sexpr<S> ((s S)) Result<Option<Sexpr>, ReadError> (where (PeekInput S))
  "Read one datum from `s` -- CL's `read`. `Ok(none)` at end of input (so a
   read loop ends on a value rather than an error), `Err` if what is there is
   not a datum. Reads exactly one, so the next call gets the next one.

   Consumes the one whitespace character that ended the datum, as CL's `read`
   does -- which is what makes a form typed at a terminal take its newline
   with it. `read-sexpr-preserving-whitespace` is the same read without that
   step."
  (let ((v (read-sexpr-preserving-whitespace s)))
    (progn
      ;; Only whitespace, and only one: a `)` or a `"` that ended the datum
      ;; belongs to whatever comes next.
      (match (peek-char s)
        ((none) ())
        ((some c) (if (reader-whitespacep c) (reader-skip-one s) ())))
      v)))

;; CL's `read-from-string`, whose *second* return value is where reading
;; stopped. This language has no multiple values, so the two come back as one
;; `cons-cell` -- `(car r)` is the datum, `(cdr r)` the character index to read
;; from next, which is what makes reading a string datum by datum a loop
;; rather than a re-scan.
;;
;; `read` (the builtin) is the same read without the index: CL's
;; `read-from-string` used for its first value only, which is the common case.
(pub defun read-from-string ((s string) &optional (start i64 0)) Result<cons-cell<Sexpr, i64>, ReadError>
  "One datum from `s` beginning at character index `start`, paired with the
   index reading stopped at. Consumes the whitespace character that ended the
   datum, as CL's `read-from-string` does."
  (read-datum-at s start false))

(pub defun read-from-string-preserving-whitespace ((s string) &optional (start i64 0))
    Result<cons-cell<Sexpr, i64>, ReadError>
  "`read-from-string` without consuming the whitespace that ended the datum --
   CL's `read-from-string` with `:preserve-whitespace t`. The difference shows
   in the returned index, and so in what the next read sees."
  (read-datum-at s start true))

(defun sexpr-list-from ((v Vector<Sexpr>)) Sexpr
  "The elements of `v` as a list, front to back."
  (let ((out (quote ())) (i (- (len v) 1)))
    (progn
      (while (>= i 0)
        (progn (setf out (sexpr-cons (get v i) out)) (setf i (- i 1)) ()))
      out)))

;; CL's `read-delimited-list`: every datum up to `terminator`, which is
;; consumed. Unterminated input is an error rather than a short list --
;; a missing `)` is a mistake, and CL signals it too.
;;
;; CL's third argument (`recursive-p`) has nothing to correspond to here: it
;; exists to tell CL's reader that the call is inside a reader macro, and
;; there are no reader macros (cl-parity-plan.md Phase 8c).
(pub defun read-delimited-list<S> ((terminator char) (s S)) Result<Sexpr, ReadError> (where (PeekInput S))
  "Every datum on `s` up to `terminator`, as a list. The terminator is
   consumed; reaching end of input first is an `Err`."
  (let ((acc (the Vector<Sexpr> (Vector::new))) (failed (the Option<ReadError> (option::none))) (going true))
    (progn
      (while going
        (progn
          (reader-scan-atmosphere s)
          (match (peek-char s)
            ((none)
             (progn
               (setf failed (option::some (ReadError::ReadError
                 (format false "read-delimited-list: end of input before the closing ~a" terminator))))
               (setf going false)
               ()))
            ((some c)
             (if (equal c terminator)
                 (progn (reader-skip-one s) (setf going false) ())
                 (match (reader-read-one-until s (option::some terminator))
                   ((err e) (progn (setf failed (option::some e)) (setf going false) ()))
                   ((ok found)
                    (match found
                      ((none)
                       (progn
                         (setf failed (option::some (ReadError::ReadError
                           (format false "read-delimited-list: end of input before the closing ~a" terminator))))
                         (setf going false)
                         ()))
                      ((some v) (progn (push acc v) ()))))))))))
      (match failed
        ((some e) (result::err e))
        ((none) (result::ok (sexpr-list-from acc)))))))

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
            (progn (close ,var) (result::ok ,result)))))
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
;; Whole-file convenience. `name` is any `Pathish` here as everywhere else, so
;; a string and a `pathname` are equally ordinary arguments.

(pub defun read-file-string<P> ((name P)) Result<string, FileError> (where (Pathish P))
  "The entire contents of `name`."
  (match (open-input name)
    ((ok s) (let ((text (read-all s))) (progn (close s) (result::ok text))))
    ((err e) (result::err e))))

(pub defun read-file-lines<P> ((name P)) Result<Vector<string>, FileError> (where (Pathish P))
  (match (open-input name)
    ((ok s) (let ((ls (read-lines s))) (progn (close s) (result::ok ls))))
    ((err e) (result::err e))))

(pub defun write-file-string<P> ((name P) (text string)) Result<(), FileError> (where (Pathish P))
  "Write `text` to `name`, replacing it. `Ok(())` on success -- the write is
   done for its effect, so there is no value to carry back."
  (match (open-output name)
    ((ok s) (progn (write-string s text) (close s) (result::ok ())))
    ((err e) (result::err e))))

(pub defun probe-file<P> ((name P)) bool (where (Pathish P))
  "Whether `name` exists."
  (file-exists-p (namestring name)))

(pub defun delete-file<P> ((name P)) Result<(), FileError> (where (Pathish P))
  "Remove `name`. `Ok(())` on success."
  (match (file-delete (namestring name))
    ((ok _) (result::ok ()))
    ((err e) (result::err e))))

(pub defun rename-file<P,Q> ((from P) (to Q)) Result<(), FileError> (where (Pathish P) (Pathish Q))
  "Rename `from` to `to`. `Ok(())` on success."
  (match (file-rename (namestring from) (namestring to))
    ((ok _) (result::ok ()))
    ((err e) (result::err e))))

;; The rest of CLHS 20.1: questions about a file that need no open stream.
;; Each is one call to the `file-*` primitive, with `Pathish` in front so a
;; string and a `pathname` are equally ordinary arguments -- the same shape
;; `probe-file`/`delete-file`/`rename-file` above already have.

(pub defun truename<P> ((name P)) Result<string, FileError> (where (Pathish P))
  "`name` with symlinks resolved and `.`/`..` removed, as an absolute path.
   `Err` if it does not exist -- resolving a path means looking at it."
  (file-truename (namestring name)))

(pub defun file-write-date<P> ((name P)) Result<i64, FileError> (where (Pathish P))
  "When `name` was last modified, as a universal time -- the same scale
   `get-universal-time` counts on, so `decode-universal-time` reads it."
  (file-modified-date (namestring name)))

(pub defun directory-p<P> ((name P)) bool (where (Pathish P))
  "Whether `name` is a directory. `false` for a plain file and for something
   that is not there at all; `probe-file` is what separates those two."
  (file-directory-p (namestring name)))

(pub defun directory<P> ((name P)) Result<Vector<string>, FileError> (where (Pathish P))
  "The entries of directory `name`, as full paths.

   Narrower than CL's `directory`, which matches a wildcard pathname: there
   are no wildcards in this language's pathnames, so there is nothing to
   match and the argument simply names the directory to list. `.` and `..`
   are not entries. The order is the operating system's -- sort it if you
   need a stable one."
  (file-list-directory (namestring name)))

(pub defun ensure-directories-exist<P> ((name P)) Result<(), FileError> (where (Pathish P))
  "Create directory `name` and any missing parent. `Ok(())` if it already
   exists -- that is what `ensure` means."
  (match (file-create-directories (namestring name))
    ((ok _) (result::ok ()))
    ((err e) (result::err e))))

;; ---------------------------------------------------------------------------
;; The environment the program is running in (CLHS 25.1), plus the two things
;; CL has no equivalent of and a script cannot do without.

(pub defun lisp-implementation-type () string
  "The name of this implementation. A constant, so it is written here rather
   than spent on a builtin."
  "typelisp")

(pub defun user-homedir-pathname () Option<pathname>
  "The user's home directory, or `none` when `$HOME` is unset. CL allows
   `NIL` here for exactly this case, so the `Option` is not an extra."
  (match (home-directory)
    ((none) (option::none))
    ((some h) (option::some (to-pathname h)))))

;; ---------------------------------------------------------------------------
;; Universal time, decomposed (CLHS 25.1).
;;
;; CL returns nine values from `decode-universal-time`; there are no multiple
;; values here, so the components come back as one struct. Two of CL's nine
;; are missing rather than faked: `daylight-p` and the `zone` a *default*
;; decode would have used both need a timezone database, and this language's
;; runtime has none. What is offered instead is the explicit-zone form CL also
;; has -- `zone` is an offset in hours west of Greenwich, and 0 (the default
;; here) is UTC.
;;
;; **This is the divergence to know about**: CL's `decode-universal-time` with
;; no zone argument decodes into *local* time. Here it decodes into UTC.

(pub defstruct decoded-time
  (pub second i32) (pub minute i32) (pub hour i32)
  (pub date i32) (pub month i32) (pub year i32) (pub day-of-week i32))

;; The civil-calendar conversions are Howard Hinnant's `civil_from_days` /
;; `days_from_civil`, shifted from the Unix epoch to CL's. They are exact
;; integer arithmetic over the proleptic Gregorian calendar -- no tables, no
;; leap-year special cases beyond the ones the formulas already encode.
;;
;; `days-from-civil` counts days from 1900-01-01, which is why the constant
;; 25567 (the days between the CL and Unix epochs) appears in both.

(defun days-from-civil ((year i32) (month i32) (day i32)) i32
  "Days from 1900-01-01 to this proleptic-Gregorian date. Years before 0 are
   out of range -- a universal time cannot name one."
  (let ((y (if (<= month 2) (- year 1) year))
        (era 0) (yoe 0) (doy 0) (doe 0))
    (progn
      (setf era (/ y 400))
      (setf yoe (- y (* era 400)))
      (setf doy (+ (/ (+ (* 153 (+ month (if (> month 2) -3 9))) 2) 5) (- day 1)))
      (setf doe (- (+ (* yoe 365) (/ yoe 4) doy) (/ yoe 100)))
      ;; `- 719468` lands on the Unix epoch; `+ 25567` moves from there to
      ;; CL's, which is 25567 days earlier.
      (+ (- (+ (* era 146097) doe) 719468) 25567))))

(pub defun encode-universal-time ((second i32) (minute i32) (hour i32)
                                  (date i32) (month i32) (year i32)
                                  &optional (zone i32 0)) i64
  "The universal time for this date and time. `zone` is an offset in hours
   west of Greenwich, as CL's is; 0 (the default) means the arguments are
   UTC."
  (+ (* (as i64 (days-from-civil year month date)) 86400)
     (as i64 (+ (* (+ hour zone) 3600) (* minute 60) second))))

(pub defun decode-universal-time ((ut i64) &optional (zone i32 0)) decoded-time
  "`ut` broken into its calendar components. `zone` is an offset in hours west
   of Greenwich, as CL's is; 0 (the default) decodes into UTC.

   `day-of-week` is CL's: 0 is Monday, 6 is Sunday. 1900-01-01 -- universal
   time 0 -- was a Monday, which is what makes it a plain remainder."
  (if (< ut 0)
      (panic "decode-universal-time: universal time is never negative")
      (let ((local (- ut (as i64 (* zone 3600))))
            (days 0) (secs 0) (z 0)
            (era 0) (doe 0) (yoe 0) (y 0) (doy 0) (mp 0) (d 0) (m 0))
        (progn
          ;; `mod` floors and `/` truncates, so they disagree on a negative
          ;; `local` (reachable for a small `ut` with a positive `zone`).
          ;; Taking the remainder first and dividing the difference makes the
          ;; division exact, so the two can never disagree.
          (setf secs (as i32 (mod local 86400)))
          (setf days (as i32 (/ (- local (as i64 secs)) 86400)))
          ;; `z` is days since 1970-01-01 shifted by 719468, which restarts
          ;; the era arithmetic below from a March-based year 0000-03-01.
          (setf z (+ (- days 25567) 719468))
          (setf era (/ z 146097))
          (setf doe (- z (* era 146097)))
          (setf yoe (/ (- (+ (- doe (/ doe 1460)) (/ doe 36524)) (/ doe 146096)) 365))
          (setf y (+ yoe (* era 400)))
          (setf doy (- doe (- (+ (* 365 yoe) (/ yoe 4)) (/ yoe 100))))
          (setf mp (/ (+ (* 5 doy) 2) 153))
          (setf d (+ (- doy (/ (+ (* 153 mp) 2) 5)) 1))
          (setf m (+ mp (if (< mp 10) 3 -9)))
          (if (<= m 2) (progn (setf y (+ y 1)) ()) ())
          (decoded-time::new (mod secs 60) (mod (/ secs 60) 60) (/ secs 3600)
                             d m y (mod days 7))))))

(pub defun get-decoded-time () decoded-time
  "Now, in UTC, broken into its calendar components."
  (decode-universal-time (get-universal-time)))

;; ---------------------------------------------------------------------------
;; Asking the user a question (CLHS 25.2). Both read from
;; `*standard-input*` and re-ask until the answer is one they accept, which
;; is what CL specifies; end of input is the one thing that can stop them,
;; and it answers `false`.

(pub defun y-or-n-p ((question string)) bool
  "Ask `question` and accept a single `y` or `n` (in either case)."
  (let ((answer false) (settled false))
    (progn
      (while (not settled)
        (progn
          (print "~a (y/n) " question)
          (match (read-line *standard-input*)
            ((none) (progn (setf settled true) ()))
            ((some line)
             (let ((a (trim line)))
               (if (equalp a "y")
                   (progn (setf answer true) (setf settled true) ())
                   (if (equalp a "n")
                       (progn (setf settled true) ())
                       ())))))))
      answer)))

(pub defun yes-or-no-p ((question string)) bool
  "Ask `question` and accept a full `yes` or `no` (in either case). The
   deliberate extra typing is CL's: this is the one for questions whose
   wrong answer is expensive."
  (let ((answer false) (settled false))
    (progn
      (while (not settled)
        (progn
          (print "~a (yes/no) " question)
          (match (read-line *standard-input*)
            ((none) (progn (setf settled true) ()))
            ((some line)
             (let ((a (trim line)))
               (if (equalp a "yes")
                   (progn (setf answer true) (setf settled true) ())
                   (if (equalp a "no")
                       (progn (setf settled true) ())
                       ())))))))
      answer)))

;; ---------------------------------------------------------------------------
;; Multi-dimensional arrays (CLHS 15) — cl-parity-plan.md Phase 6b.
;;
;; **A prelude type, not a built-in one**, for the reason `complex` above is
;; one: an array is a dimension list plus a flat element sequence, and both of
;; those are `Vector`s this language already has. Written as a `defstruct` it
;; needs no new `Repr`, no `rt_*` shim, no island lowering and no artifact
;; surgery — it compiles through the ordinary prelude path the day it is
;; written.
;;
;; Departures from CL, all of them either forced by static typing or already
;; answered by it:
;;
;; 1. The element type is the type parameter `T`. So `array-element-type`,
;;    `simple-vector-p`, `adjustable-array-p` and `array-has-fill-pointer-p`
;;    are questions the static type has already answered and none of them
;;    exists here (see `docs/dev/cl-missing-classes-and-methods.md`).
;; 2. Subscripts are a `Vector<i32>` rather than a `&rest` of integers: a
;;    `defmethod` resolves by arity, never by a trailing run of same-typed
;;    arguments. The CL-shaped `(aref a i j)` is checker sugar over exactly
;;    these methods — see `Checker::check_aref`.
;; 3. Creation is `Array::make`, not `Array::new`. `new` is the field-order
;;    constructor every `defstruct` generates, and this one's fields are the
;;    *representation* (dimensions, flat storage, fill pointer) rather than
;;    what a caller wants to hand over.
;; 4. CL's `make-array` keywords: `:initial-element` is the required `init`
;;    argument, since there are no unbound array cells here; `:fill-pointer`
;;    is a `&key`; and `:adjustable` has no counterpart, because `adjust`
;;    works on every array.
;;
;; Row-major order throughout, as in CL: the last subscript varies fastest.

(pub defstruct Array<T>
  "A multi-dimensional array of `T`, stored in row-major order."
  (dims Vector<i32>)
  (data Vector<T>)
  (pub fill-pointer Option<i32>))

;; CL's `make-array`. `dims` is copied, so a later `push` to the caller's own
;; vector cannot change the array's shape behind its back.
(pub defmethod make (Array<T> (dims Vector<i32>) (init T) &key (fill-pointer i32)) Array<T>
  (let ((d (the Vector<i32> (Vector::new))) (n 1))
    (progn
      (doiter (x (iter dims))
        (progn
          (if (< x 0)
              (panic (format false "Array::make: dimension ~a is negative" x))
              ())
          (push d x)
          (setf n (* n x))))
      (match fill-pointer
        ((none) ())
        ((some f)
         (progn
           (if (/= (len d) 1)
               (panic "Array::make: :fill-pointer needs a one-dimensional array")
               ())
           (if (or (< f 0) (> f n))
               (panic (format false "Array::make: :fill-pointer ~a is not in 0..~a" f n))
               ()))))
      (Array::new d (Vector::filled n init) fill-pointer))))

;; CL's `array-rank` / `array-dimension` / `array-dimensions` /
;; `array-total-size`. The type name is not repeated in the method name:
;; the receiver already says which type is being asked.
(pub defmethod rank ((self Array<T>)) i32 (len self::dims))
(pub defmethod dimension ((self Array<T>) (n i32)) i32 (get self::dims n))
(pub defmethod total-size ((self Array<T>)) i32 (len self::data))
;; A fresh vector, like CL's `array-dimensions` returns a fresh list — the
;; array's own is its representation and handing it out would let a caller
;; reshape the array by mutating what it got back.
(pub defmethod dimensions ((self Array<T>)) Vector<i32>
  (let ((out (the Vector<i32> (Vector::new))))
    (progn (doiter (x (iter self::dims)) (push out x)) out)))

;; CL's `array-in-bounds-p`: false for the wrong number of subscripts too,
;; which is what CL says (it is not an error to ask).
(pub defmethod in-bounds ((self Array<T>) (idx Vector<i32>)) bool
  (if (/= (len idx) (len self::dims))
      false
      (let ((i 0) (ok true) (n (len idx)))
        (progn
          (while (< i n)
            (progn
              (let ((s (get idx i)))
                (setf ok (and ok (>= s 0) (< s (get self::dims i)))))
              (setf i (+ i 1))))
          ok))))

;; CL's `array-row-major-index`: the flat offset `idx` names, by the Horner
;; fold that row-major order *is*. Out-of-range subscripts are an error here
;; rather than a wrong-but-in-range offset — without the check `(aref a 0 5)`
;; on a 3x3 would quietly read row 1 rather than say anything.
(pub defmethod row-major-index ((self Array<T>) (idx Vector<i32>)) i32
  (if (in-bounds self idx)
      (let ((acc 0) (i 0) (r (len self::dims)))
        (progn
          (while (< i r)
            (progn
              (setf acc (+ (* acc (get self::dims i)) (get idx i)))
              (setf i (+ i 1))))
          acc))
      (panic "aref: subscripts are out of range for this array")))

;; `aref` / `(setf (aref ...))`, spelled the way every other indexed container
;; in this language spells them. `(aref a i j)` is checker sugar for these.
(pub defmethod get ((self Array<T>) (idx Vector<i32>)) T
  (get self::data (row-major-index self idx)))
(pub defmethod set ((self Array<T>) (idx Vector<i32>) (x T)) ()
  (set self::data (row-major-index self idx) x))

;; CL's `row-major-aref` and its `setf`: the flat offset directly.
(pub defmethod row-major-get ((self Array<T>) (i i32)) T (get self::data i))
(pub defmethod row-major-set ((self Array<T>) (i i32) (x T)) () (set self::data i x))

;; CL's `length` on an array: the fill pointer when there is one, and the
;; whole array when there is not.
(pub defmethod len ((self Array<T>)) i32
  (match self::fill-pointer ((some n) n) ((none) (len self::data))))

;; `array-iter<T>` is to `Array<T>` what `vector-iter<T>` is to `Vector<T>`:
;; a separate cursor per `(iter a)` call, walking row-major order and stopping
;; at the fill pointer if there is one.
(defstruct array-iter<T> (arr Array<T>) (pos i32))
(impl Iter array-iter<T>
  (type Item T)
  (next ((self Self)) Option<T>
    (if (< self::pos (len self::arr))
        (let ((v (row-major-get self::arr self::pos)))
          (setf self::pos (+ self::pos 1))
          (Option::some v))
        (Option::none))))
(pub defmethod iter ((self Array<T>)) array-iter<T> (array-iter::new self 0))

;; CL's `vector-push-extend`: write at the fill pointer, growing the storage
;; when the pointer has reached the end. A fill-pointer array is
;; one-dimensional by construction (`Array::make` refuses any other rank), so
;; growing it means growing its single dimension.
(pub defmethod push-extend ((self Array<T>) (x T)) ()
  (match self::fill-pointer
    ((none) (panic "push-extend: this array has no fill pointer"))
    ((some n)
     (progn
       (if (< n (len self::data)) (set self::data n x) (push self::data x))
       (set self::dims 0 (len self::data))
       (setf self::fill-pointer (Option::some (+ n 1)))))))

;; CL's `vector-pop`. `Option<T>` on an empty array rather than an error, the
;; same answer `Vector<T>`'s own `pop` gives.
(pub defmethod pop ((self Array<T>)) Option<T>
  (match self::fill-pointer
    ((none) (panic "pop: this array has no fill pointer"))
    ((some n)
     (if (<= n 0)
         (Option::none)
         (let ((v (get self::data (- n 1))))
           (setf self::fill-pointer (Option::some (- n 1)))
           (Option::some v))))))

;; Decode a row-major offset back into subscripts, into `out` (which must
;; already have one slot per dimension). The inverse of `row-major-index`'s
;; fold, so it runs the dimensions backwards: the last one varies fastest.
(defun array-decode ((dims Vector<i32>) (flat i32) (out Vector<i32>)) ()
  (let ((k (- (len dims) 1)) (rest flat))
    (while (>= k 0)
      (progn
        (let ((d (get dims k)))
          (progn
            (set out k (mod rest d))
            (setf rest (/ rest d))))
        (setf k (- k 1))))))

(defun array-subs-in-bounds ((sub Vector<i32>) (dims Vector<i32>)) bool
  (let ((i 0) (ok true) (n (len sub)))
    (progn
      (while (< i n)
        (progn
          (setf ok (and ok (< (get sub i) (get dims i))))
          (setf i (+ i 1))))
      ok)))

;; CL's `adjust-array`: reshape in place, keeping every element whose
;; subscripts still name a cell. CL requires the new rank to match the old
;; one, and so does this. New cells get `init` — CL's `:initial-element`,
;; required here for the same reason `Array::make` requires it.
;;
;; Mutates and returns `()`, where CL returns the array; CL has to, because a
;; non-adjustable CL array may come back as a *different* array. Every array
;; here is adjustable, so there is never a second one to return.
(pub defmethod adjust ((self Array<T>) (newdims Vector<i32>) (init T)) ()
  (let ((nd (the Vector<i32> (Vector::new))) (n 1))
    (progn
      (if (/= (len newdims) (len self::dims))
          (panic (format false "adjust: expected ~a dimension(s), got ~a"
                         (len self::dims) (len newdims)))
          ())
      (doiter (x (iter newdims))
        (progn
          (if (< x 0)
              (panic (format false "adjust: dimension ~a is negative" x))
              ())
          (push nd x)
          (setf n (* n x))))
      (let ((moved (Vector::filled n init))
            (sub (Vector::filled (len nd) 0))
            (i 0))
        (progn
          (while (< i n)
            (progn
              (array-decode nd i sub)
              (if (array-subs-in-bounds sub self::dims)
                  (set moved i (get self::data (row-major-index self sub)))
                  ())
              (setf i (+ i 1))))
          (setf self::dims nd)
          (setf self::data moved)
          (match self::fill-pointer
            ((none) ())
            ((some f) (setf self::fill-pointer (Option::some (min f n))))))))))

;; ---------------------------------------------------------------------------
;; Bit vectors (CLHS 15.2) — cl-parity-plan.md Phase 6c.
;;
;; A prelude type for the same reason `Array<T>` above is one: a bit vector is
;; a packed word sequence and a length, and `Vector<i64>` plus the integer
;; bit operations (§4.4 of `docs/functions.md`) are all that takes.
;;
;; **32 bits to a word, not 64.** A compiled function stores a container
;; element through the tagged representation compiled code passes values in
;; (`typelisp-abi`'s `encode`: three tag bits in the low end, the integer in
;; the rest), so an `i64` wider than that payload does not survive the round
;; trip — `(set v 0 (ash 1 60))` on a `Vector<i64>` reads back as a different
;; number *when the function holding it is compiled*, which every prelude
;; method is. That is a bug in its own right and is recorded as one in
;; `docs/dev/TODO.md`; packing 32 bits a word keeps this type well inside
;; what the payload carries instead of at its edge, and `(/ i 32)` /
;; `(mod i 32)` say plainly which word a bit is in.
;;
;; The bits past the length in the final word are kept clear
;; (`bitvector-trim`), so the representation of a given bit vector is unique
;; and `lognot` cannot leave phantom set bits behind it.
;;
;; As with `Array<T>`, creation is `BitVector::make` rather than
;; `BitVector::new`: `new` is the field-order constructor `defstruct`
;; generates, and this type's fields are its packed representation.
;; `bit-vector-p` does not exist — the static type has already answered it.

(pub defstruct BitVector
  "A fixed-length sequence of bits, packed 32 to an `i64` word."
  (words Vector<i64>)
  (nbits i32))

;; CL's `(make-array n :element-type 'bit)`. Every bit starts at 0, which is
;; what CL's `:initial-element` defaults to for a bit array.
(pub defmethod make (BitVector (n i32)) BitVector
  (if (< n 0)
      (panic (format false "BitVector::make: length ~a is negative" n))
      (BitVector::new (Vector::filled (/ (+ n 31) 32) (the i64 0)) n)))

(pub defmethod len ((self BitVector)) i32 self::nbits)

;; CL's `bit` / `sbit` and their `setf`s are `get`/`set` here, the way every
;; other indexed container in this language spells them; the CL names are
;; kept as aliases below.
(pub defmethod get ((self BitVector) (i i32)) bool
  (if (or (< i 0) (>= i self::nbits))
      (panic (format false "bit: index ~a is out of range for a bit vector of ~a" i self::nbits))
      (logbitp (as i64 (mod i 32)) (get self::words (/ i 32)))))

(pub defmethod set ((self BitVector) (i i32) (b bool)) ()
  (if (or (< i 0) (>= i self::nbits))
      (panic (format false "bit: index ~a is out of range for a bit vector of ~a" i self::nbits))
      (let ((w (/ i 32)) (mask (ash (the i64 1) (as i64 (mod i 32)))))
        (set self::words w
             (if b
                 (logior (get self::words w) mask)
                 (logand (get self::words w) (lognot mask)))))))

(pub defmethod bit ((self BitVector) (i i32)) bool (get self i))
(pub defmethod set-bit ((self BitVector) (i i32) (b bool)) () (set self i b))
;; CL's `sbit` differs from `bit` only in requiring a *simple* bit vector,
;; a distinction this language's single bit-vector type does not have.
(pub defmethod sbit ((self BitVector) (i i32)) bool (get self i))
(pub defmethod set-sbit ((self BitVector) (i i32) (b bool)) () (set self i b))

;; Put the words back in canonical form. Two things can put them out of it,
;; and both have to be undone or two bit vectors holding the same bits stop
;; being the same value (`equalp` compares the words):
;;
;; 1. `lognot` sets all 64 bits of a word, not just the 32 this packing uses.
;; 2. The last word reaches past the length.
;;
;; Every operation that can do either ends here.
(defconstant (*bitvector-word-mask* i64) 4294967295)
(defun bitvector-trim ((v BitVector)) ()
  (let ((n (len v::words)) (i 0))
    (progn
      (while (< i n)
        (progn
          (set v::words i (logand (get v::words i) *bitvector-word-mask*))
          (setf i (+ i 1))))
      (let ((used (mod v::nbits 32)))
        (if (= used 0)
            ()
            (set v::words (- n 1)
                 (logand (get v::words (- n 1))
                         (- (ash (the i64 1) (as i64 used)) (the i64 1)))))))))

;; The shared body of CL's `bit-and` family: same length in, a fresh bit
;; vector out, one word at a time. `who` is only for the length-mismatch
;; message. CL's optional third argument (write into an existing vector, or
;; into the first one when it is `t`) is not offered — the caller writes
;; `(setf a (bit-and a b))`, and the aliasing question never comes up.
(defun bitvector-zip ((a BitVector) (b BitVector) (who string) (f (fn (i64 i64) i64))) BitVector
  (if (/= a::nbits b::nbits)
      (panic (format false "~a: bit vectors differ in length (~a and ~a)" who a::nbits b::nbits))
      (let ((out (BitVector::make a::nbits)) (i 0) (n (len a::words)))
        (progn
          (while (< i n)
            (progn
              (set out::words i (f (get a::words i) (get b::words i)))
              (setf i (+ i 1))))
          (bitvector-trim out)
          out))))

(pub defmethod bit-and ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-and" (lambda ((x i64) (y i64)) i64 (logand x y))))
(pub defmethod bit-ior ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-ior" (lambda ((x i64) (y i64)) i64 (logior x y))))
(pub defmethod bit-xor ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-xor" (lambda ((x i64) (y i64)) i64 (logxor x y))))
;; The rest of CL's family. Nothing new is needed for any of them: each is
;; the same word-wise walk over the integer operation of the same name.
(pub defmethod bit-eqv ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-eqv" (lambda ((x i64) (y i64)) i64 (logeqv x y))))
(pub defmethod bit-nand ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-nand" (lambda ((x i64) (y i64)) i64 (lognand x y))))
(pub defmethod bit-nor ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-nor" (lambda ((x i64) (y i64)) i64 (lognor x y))))
(pub defmethod bit-andc1 ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-andc1" (lambda ((x i64) (y i64)) i64 (logandc1 x y))))
(pub defmethod bit-andc2 ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-andc2" (lambda ((x i64) (y i64)) i64 (logandc2 x y))))
(pub defmethod bit-orc1 ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-orc1" (lambda ((x i64) (y i64)) i64 (logorc1 x y))))
(pub defmethod bit-orc2 ((self BitVector) (other BitVector)) BitVector
  (bitvector-zip self other "bit-orc2" (lambda ((x i64) (y i64)) i64 (logorc2 x y))))

(pub defmethod bit-not ((self BitVector)) BitVector
  (let ((out (BitVector::make self::nbits)) (i 0) (n (len self::words)))
    (progn
      (while (< i n)
        (progn
          (set out::words i (lognot (get self::words i)))
          (setf i (+ i 1))))
      (bitvector-trim out)
      out)))

;; ---------------------------------------------------------------------------
;; Errors: the general-purpose type, wrapping, and the two things CL had that
;; this language did not — cl-parity-plan.md Phase 7a.
;;
;; The condition *system* is not adopted (the plan's §0, and
;; language-design.md §9: there is no way to catch a `panic` and continue).
;; What is here is the mapping of CL's condition *types* onto the `Error`
;; trait, plus the two holes that mapping left open:
;;
;; - `SimpleError` — CL's `simple-error`, the type a program reaches for when
;;   it just wants to say what went wrong. Every other concrete error type
;;   here belongs to some particular fallible operation.
;; - `warn` — CL's warning path. Everything else that goes wrong in this
;;   language either returns a `Result` or panics; there was no way to say
;;   something and *continue*.
;;
;; CL's `type-error`, `unbound-variable`, `unbound-slot`,
;; `undefined-function`, `control-error` and `program-error` have no
;; counterpart on purpose: type checking, all-slots-required construction,
;; name resolution and the static escape analysis turn every one of them into
;; a compile-time error, so there is nothing left to signal at run time.

(pub defstruct SimpleError
  "An error that carries nothing but its message — CL's `simple-error`."
  (text string))
(impl Error SimpleError
  (message ((self Self)) string self::text))

;; CL's `simple-error` is what `(error "...")` signals; the same
;; convenience here is a constructor that formats, since a `Result` is
;; returned rather than signalled.
(pub defun simple-error ((msg string)) SimpleError (SimpleError::new msg))

;; An error that wraps another, so a low-level cause can travel with the
;; context that makes it readable. This is the one error type whose `source`
;; is not `none`, and the reason the `Error` trait has `source` at all.
(pub defstruct WrappedError
  "An error carrying both its own message and the error that caused it."
  (text string)
  (cause :dyn Error))
(impl Error WrappedError
  (message ((self Self)) string self::text)
  (source ((self Self)) Option<:dyn Error> (Option::some self::cause)))

;; `(wrap-error "reading the config" e)` — the same widening `as-dyn-error`
;; does, with a sentence attached. Generic over the cause's concrete type for
;; the same reason `as-dyn-error` is: the boxing happens where the call site
;; specializes this body.
(pub defun wrap-error<E> ((msg string) (cause E)) WrappedError (where (Error E))
  (WrappedError::new msg (as :dyn Error cause)))

;; The whole chain, one cause per line. CL has no counterpart (its condition
;; report is a single line); this is Rust's "caused by" chain, and it is the
;; only thing that makes `source` observable.
(pub defun describe-error<E> ((e E)) string (where (Error E))
  (let ((out (message e)) (cur (source e)) (go true))
    (progn
      (while go
        (match cur
          ((none) (progn (setf go false) ()))
          ((some c)
           (progn
             (setf out (append out (append "\n  caused by: " (message c))))
             (setf cur (source c))
             ()))))
      out)))

;; CL's `assert`. Without a condition system there are no restarts to offer,
;; so a false test is a `panic` — which is what CL's `assert` does too when
;; every restart is declined. A macro rather than a function so the failure
;; message can name the test *as written*; with a second argument, that is
;; the message instead.
(defmacro assert (c &rest msg)
  (if (sexpr-null msg)
      `(if ,c () (panic (format false "assertion failed: ~s" (quote ,c))))
      `(if ,c () (panic ,(sexpr-car msg)))))

;; CL's `warn`: say something on `*error-output*` and carry on. The only
;; thing in this language that reports without either returning a `Result` or
;; ending the program. Takes a control string like `print`/`println`, and
;; prefixes the line the way CL's warning report does.
(defmacro warn (control &rest args)
  `(progn
     (format *error-output* "WARNING: ")
     (format *error-output* ,control ,@args)
     (format *error-output* "~%")))

;; ---------------------------------------------------------------------------
;; Scoped rebinding of a global — cl-parity-plan.md Phase 7b.
;;
;; CL writes this as `let`, because a CL `let` on a special variable *is* a
;; dynamic binding. This language's `let` is always lexical and always will
;; be (language-design.md), so `(let ((*print-base* 16)) ...)` would quietly
;; bind a fresh local named `*print-base*` that nothing reads. The dynamic one
;; therefore needs a name of its own: `dlet`, after Emacs Lisp's macro of the
;; same name and the same job.
;;
;; It is not a binding at all, underneath — it saves the global, assigns, and
;; restores in an `unwind-protect` cleanup. That is enough to be indistinguishable
;; from a dynamic binding for single-threaded code, because the cleanup runs
;; however the body is left: normally, by `throw`, by `panic`, or by
;; `break`/`return` (syntax.md §8).
;;
;; What it is *not*: per-thread. A real special variable has one binding per
;; thread; this has one global that a body borrows and gives back.

(defmacro dlet1 (place value &rest body)
  (let ((saved (gensym)))
    `(let ((,saved ,place))
       (progn
         (setf ,place ,value)
         (unwind-protect (progn ,@body) (setf ,place ,saved))))))

(pub defmacro dlet (bindings &rest body)
  "Assign each global for the extent of `body`, restoring it on the way out
   however `body` is left. CL spells this `let`; this language's `let` is
   lexical, so the dynamic one has its own name."
  (if (sexpr-null bindings)
      (sexpr-cons (quote progn) body)
      (let ((b (sexpr-car bindings)))
        `(dlet1 ,(sexpr-car b) ,(sexpr-car (sexpr-cdr b))
                (dlet ,(sexpr-cdr bindings) ,@body)))))


;; CL's `with-standard-io-syntax`: run `body` with every printer control
;; variable at its standard value, so what is printed does not depend on what
;; the surrounding program happened to `setf`. Nothing but a `dlet` over the
;; whole set.
;;
;; Two departures from CL's list. `*print-case*` is `:downcase` rather than
;; `:upcase` — CL's standard value means "as stored", and this reader stores
;; symbol names downcased (see `*print-case*`'s own note). And the reader
;; variables CL also binds (`*read-base*`, `*read-default-float-format*`,
;; `*read-eval*`, `*read-suppress*`) are not in this language, so there is
;; nothing to bind; `*package*` and `*readtable*` likewise.
(pub defmacro with-standard-io-syntax (&rest body)
  `(dlet ((*print-base* 10)
          (*print-radix* false)
          (*print-case* :downcase)
          (*print-circle* false)
          (*print-level* 0)
          (*print-length* 0)
          (*print-pretty* false)
          (*print-readably* true)
          (*print-right-margin* 0)
          (*print-miser-width* 0))
     ,@body))


"##;

/// Read, check, and `exec` [`SOURCE`] against `heap`/`chk`/`interp`,
/// registering its definitions exactly as if the caller had typed them first.
///
/// Must be called before any user source that references a prelude name.
///
/// This is the whole of the prelude the *front end* has: source text, read,
/// checked, and tree-walked. Installing the committed native bodies over the
/// result is a separate, backend-side step
/// (`crate::compile::prelude_bootstrap::load`, re-exported as
/// `typelisp::load_prelude`) — which is what lets a build with no LLVM
/// backend at all still bring the prelude into scope.
///
/// [`SOURCE`] being fixed, any failure here is a build/bug condition rather
/// than a user error — hence the panics.
pub fn load_interpreted(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) {
    load_interpreted_with(heap, chk, interp, &mut |_, _| {}, &mut |_, _| {});
}

/// [`load_interpreted`] with the two points a *generator* needs to look in:
/// `on_read` sees the whole form list once, right after the read and before
/// anything has been checked (the prelude's source hash is taken over exactly
/// these forms); `on_checked` sees each top-level form after checking and
/// before `exec` (where the artifact's item list is collected from).
///
/// Both are `&mut dyn FnMut` rather than generic parameters so this stays a
/// single, non-inlined function no matter who calls it: it is the one place
/// the prelude's load order is written down, and the compiled-artifact
/// generators must not be able to drift from it.
pub fn load_interpreted_with(
    heap: &mut Heap,
    chk: &mut Checker,
    interp: &mut Interp,
    on_read: &mut dyn FnMut(&mut Heap, &[Value]),
    on_checked: &mut dyn FnMut(&mut Heap, Value),
) {
    // The core macro layer first: `SOURCE` below is written in `when`/`cond`/
    // `and`/`or`, and `,@` desugars to the `sexpr-append` that layer defines.
    // It is a separate layer rather than the top of this file because the
    // compiler island needs the same macros without taking the prelude with
    // them — see `crate::core_macros`'s module comment.
    crate::core_macros::load_with(heap, chk, interp, on_checked);

    let r = Reader::new();
    let forms = r.read_all(heap, SOURCE).expect("prelude: read failed");
    on_read(heap, &forms);
    for v in forms {
        let tl = chk.check_form(heap, &*interp, v).expect("prelude: check failed");
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        on_checked(heap, tl);
        interp.exec(heap, tl).expect("prelude: eval failed");
    }
}
