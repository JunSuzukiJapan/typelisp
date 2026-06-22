//! `compile`: a function-level compiler **written in typelisp itself**, on
//! top of the LLVM-builder bindings and typed-AST bridge Rust provides (see
//! `crate::check::registry::register_compile_builtins` and
//! `crate::compile::ast_bridge`) — the same "library written in typelisp
//! calling Rust builtins" shape as [`crate::prelude`], not a Rust special form.
//!
//! Phase 1-3 ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) restricts
//! `compile` to functions whose every parameter and return type is `i64`,
//! `bool`, `f64`, `char`, or `Sexpr` (any mix — e.g. `(defun gt ((a i64) (b
//! i64)) bool (> a b))`), with a body built from integer/bool/float/char
//! literals, the `Nil` `Sexpr` literal, parameter references, `i64`/`f64`
//! arithmetic/comparison, `char`'s `eq`/`lt`, `cons`/`car`/`cdr`/`null`,
//! `let`, `if`, and calls to *other already-compiled* functions (Phase 2f —
//! a call to one not yet compiled fails the AST bridge, refusing `compile`
//! outright, rather than reaching `compile-value` with no callee to call)
//! *or* to a function in the same self/mutual-recursion group (Phase 2g —
//! `compile`'s own `name` for plain self-recursion, or `compile-group`'s
//! whole list of names for mutual recursion: every group member's bare
//! `LlvmFunction` declaration is added to one shared module *before* any
//! body is built, so a call to a not-yet-finished sibling resolves to an
//! ordinary intra-module `call`, with no JIT/global-mapping step needed at
//! all — see `ast_bridge::typed_to_ast`'s doc comment on `group`). `if` is
//! only supported in **tail
//! position** (the value an enclosing `if`/the function itself returns),
//! not as a nested sub-expression: `compile-tail` lowers a tail `if` to two
//! basic blocks each ending in their own `ret`, sidestepping the need for a
//! phi node or alloca/load/store merge this narrow first slice doesn't need
//! yet (see `compile-value`'s fallback `panic` for the case this
//! restriction rules out).
//!
//! **ループ構文**: `while` (any number of loop-carried `i64`/`bool`/`f64`/
//! `char` variables, mutated via local `setf`) is also supported, via a
//! phi node at the loop header for *every* currently in-scope variable
//! (not just the ones a body actually mutates — see `make-phis`'s doc
//! comment on why precise mutation analysis isn't worth it here). `break`/
//! `return`/`loop` aren't bridged yet (a `while` whose body uses them, or a
//! `setf` of a `Sexpr`-typed variable, fails the AST bridge outright —
//! see `ast_bridge::typed_to_ast`'s `Expr::Set` arm on why `Sexpr` is
//! excluded). This is the one other place (besides `if`) where `compile`
//! builds more than a single straight-line basic block, but unlike `if` it
//! genuinely needs phi nodes rather than sidestepping them: a loop's back
//! edge means a loop-carried variable's value at the header has two
//! possible predecessors (the preheader, on the first pass, or the latch,
//! on every subsequent one) that can't both be made to end in their own
//! `ret`/fall straight through the way `if`'s two branches can.
//!
//! **Phase 3's GC-rooting discipline.** `Sexpr` values are pointers into the
//! mark-sweep cons heap (`crate::mem::Heap`), and `Heap::cons` may trigger a
//! collection whenever its free list is empty — but JIT'd machine code's
//! registers/stack slots are completely invisible to that collection's
//! root-walk (only `Heap.roots` is). A `Sexpr` value sitting in some other
//! `let` binding or parameter — not the operands of the very `cons` call
//! about to run — would therefore be vulnerable to being reclaimed out from
//! under it the instant any *other* `cons` call (anywhere in the same
//! dynamic scope) triggers a GC. The fix mirrors what the tree-walking
//! interpreter's own `Interp::sync_roots` does dynamically, just done
//! statically at compile time instead: every `Sexpr`-typed parameter is
//! `push-root`-ed once in the function's `entry` block (`compile`'s
//! `pushed` count, computed via `count-and-push-sexpr`), and every `Sexpr`-
//! typed `let` binding is `push-root`-ed for the duration of its own scope
//! (`ALet`'s arms below) — so by the time *any* `build-cons` call runs
//! anywhere in the function, every live `Sexpr` value reachable through
//! `params`/`vals` is already a GC root. `compile-tail`'s `pushed` parameter
//! threads the *cumulative* push count from `entry` down through nested
//! `if`/`let` so the one `_` arm that actually emits a `build-ret*` can
//! `pop-root` all of them right before returning (a `let` in *tail*
//! position never pops its own pushes — control flow falls straight through
//! to a `build-ret*` deeper in, which pops everything at once; only
//! `compile-value`'s `ALet` arm, used in non-tail/value position, pops its
//! own pushes immediately after evaluating its body, since control returns
//! to its caller rather than falling through to a `ret`).

use crate::{Checker, Heap, Interp, Reader};

pub const SOURCE: &str = r#"
;; Searched from the *last* index backward (not forward) so that a `let`-
;; bound local shadowing an outer name (pushed later, at a higher index by
;; `extend-strs`/`extend-values`) is found before the outer one (Phase 2e —
;; before `let` existed, every name in `params` came only from the function's
;; own parameter list, so search direction never mattered).
(defun index-of ((name string) (i i32) (params Vector<string>)) i32
  (if (< i 0)
      (panic "compile: unknown variable")
      (if (eq name (get params i)) i (index-of name (- i 1) params))))

(defun param-index ((params Vector<string>) (name string)) i32
  (index-of name (- (length params) 1) params))

;; `extend-strs`/`extend-values` (Phase 2e, `let`-bound locals): a fresh
;; `Vector` holding `base`'s elements followed by `extra`'s — never `push`
;; onto `base`/an existing `vals` in place, since `Vector` mutation is shared
;; (`Rc<RefCell<..>>`) and `params`/`vals` must stay correct for *sibling*
;; scopes (e.g. an outer `if`'s other branch, or code after the `let` returns)
;; that still refer to the un-extended vectors.
(defun copy-strs ((src Vector<string>) (i i32) (n i32) (out Vector<string>)) Vector<string>
  (if (>= i n) out (progn (push out (get src i)) (copy-strs src (+ i 1) n out))))

(defun extend-strs ((base Vector<string>) (extra Vector<string>)) Vector<string>
  (copy-strs extra 0 (length extra) (copy-strs base 0 (length base) (Vector::new 0 ""))))

(defun copy-values ((src Vector<LlvmValue>) (i i32) (n i32) (out Vector<LlvmValue>)) Vector<LlvmValue>
  (if (>= i n) out (progn (push out (get src i)) (copy-values src (+ i 1) n out))))

(defun extend-values ((base Vector<LlvmValue>) (extra Vector<LlvmValue>)) Vector<LlvmValue>
  (copy-values extra 0 (length extra) (copy-values base 0 (length base) (Vector::new 0 (llvm-const-i64 0)))))

;; `while`'s phi-node-based loop compilation (ループ構文, `compile-value`'s
;; `AWhile` arm below): rather than analyzing a loop's body to find which of
;; the currently in-scope variables it actually mutates via `setf`, every
;; one of them gets a phi node at the loop header, unconditionally — for a
;; variable the body never touches, both incoming edges are simply the same
;; value, which is correct (if a little redundant — this compiler never runs
;; any LLVM optimization passes, so there's no `mem2reg`-style cleanup to
;; lose anyway) and far simpler than a precise mutation analysis.
;; `make-phis` builds one (still incoming-edge-less) phi per entry of
;; `seeds`, via `build-phi`. Must be called immediately after
;; `position-at-end`ing the loop header, before anything else is built
;; there (LLVM requires every phi in a block to precede non-phi
;; instructions).
(defun make-phis ((b LlvmBuilder) (seeds Vector<LlvmValue>) (i i32) (n i32) (out Vector<LlvmValue>)) Vector<LlvmValue>
  (if (>= i n) out (progn (push out (build-phi b (get seeds i))) (make-phis b seeds (+ i 1) n out))))

;; Adds one `(value, block)` incoming edge to each of `phis`, pairing them
;; index-for-index with `vals` — the values live at `block`, the
;; predecessor the edge comes from (either the preheader, with the
;; pre-loop values, or the loop's actual latch block, with whatever each
;; variable holds after one pass through the body).
(defun add-incoming-all ((b LlvmBuilder) (phis Vector<LlvmValue>) (vals Vector<LlvmValue>) (block LlvmBasicBlock) (i i32) (n i32)) ()
  (if (>= i n)
      ()
      (progn (add-incoming b (get phis i) (get vals i) block) (add-incoming-all b phis vals block (+ i 1) n))))

;; Overwrites `dst`'s entries in place with `src`'s, index for index —
;; `AWhile` uses this once at the loop's `exit` block to make the *original*
;; `vals` (the very `Vector` instance every sibling form before/after the
;; `while` in the same body sequence shares — see `compile-value`'s `ALet`
;; arm) reflect the header's phi-resolved post-loop values, without
;; replacing `vals`'s identity (which sibling forms still hold a reference
;; to).
(defun copy-into ((dst Vector<LlvmValue>) (src Vector<LlvmValue>) (i i32) (n i32)) ()
  (if (>= i n) () (progn (set dst i (get src i)) (copy-into dst src (+ i 1) n))))

;; Phase 3's rooting discipline (see this module's doc comment): pushes a GC
;; root for every `Sexpr`-typed (`is-sexpr-value`) entry of `vs` — typically
;; one function's whole parameter list or one `let`'s freshly-bound values —
;; and returns how many it pushed, so the caller knows how many to
;; `pop-root` later (`vs`'s non-`Sexpr` entries need no rooting at all,
;; since only pointers into the cons heap are ever collected).
(defun count-and-push-sexpr ((b LlvmBuilder) (module LlvmModule) (f LlvmFunction) (vs Vector<LlvmValue>) (i i32) (n i32) (acc i32)) i32
  (if (>= i n)
      acc
      (if (is-sexpr-value (get vs i))
          (progn (push-root b module f (get vs i)) (count-and-push-sexpr b module f vs (+ i 1) n (+ acc 1)))
          (count-and-push-sexpr b module f vs (+ i 1) n acc))))

(defun pop-roots ((b LlvmBuilder) (module LlvmModule) (f LlvmFunction) (n i32)) ()
  (if (<= n 0) () (progn (pop-root b module f) (pop-roots b module f (- n 1)))))

;; `ALet`/`ACall`'s local helpers (`eval-args`/`eval-body`) are `labels`-local
;; (not their own top-level `defun`s) because they call `compile-value` — a
;; `defun` may only call itself or an *already-defined* `defun`, never one
;; defined later, so a separate top-level helper calling back into
;; `compile-value` (still being defined) would be a forward reference
;; `labels` sidesteps (the helper is local to `compile-value`'s own body,
;; which is already in scope by the time it's checked, exactly like ordinary
;; self-recursion). `eval-args` is shared by `ALet` (evaluating binding
;; values) and `ACall` (evaluating call arguments) — both are just "turn a
;; `Vector<AstExpr>` into a `Vector<LlvmValue>` in the *current* scope".
;;
;; `module`/`f` are threaded through purely so `ACall` can resolve its callee
;; via `get-or-declare-function` and `ACons`/`ACar`/`ACdr`/`push-root`/
;; `pop-root` (Phase 3) can reach the enclosing function's `heap` parameter
;; — every other arm ignores them.
(defun compile-value ((module LlvmModule) (b LlvmBuilder) (f LlvmFunction) (params Vector<string>) (vals Vector<LlvmValue>) (e AstExpr)) LlvmValue
  (labels ((eval-args ((i i32) (n i32) (exprs Vector<AstExpr>) (out Vector<LlvmValue>)) Vector<LlvmValue>
             (if (>= i n)
                 out
                 (progn (push out (compile-value module b f params vals (get exprs i)))
                        (eval-args (+ i 1) n exprs out))))
           (eval-body ((i i32) (n i32) (forms Vector<AstExpr>) (p2 Vector<string>) (v2 Vector<LlvmValue>)) LlvmValue
             (let ((r (compile-value module b f p2 v2 (get forms i))))
               (if (>= (+ i 1) n) r (eval-body (+ i 1) n forms p2 v2)))))
    (match e
      ((AInt n) (llvm-const-i64 n))
      ((ABool flag) (llvm-const-bool flag))
      ((AFloat n) (llvm-const-f64 n))
      ((AChar c) (llvm-const-char c))
      ((ANil) (llvm-const-nil))
      ((AVar name) (get vals (param-index params name)))
      ((ABinOp op lhs rhs) (build-op b op (compile-value module b f params vals lhs) (compile-value module b f params vals rhs)))
      ;; `cons`/`car`/`cdr` (Phase 3): `build-cons` itself roots its two
      ;; operands around the underlying (possibly GC-triggering)
      ;; `Heap::cons` call — see `build_cons`'s doc comment
      ;; (`crate::eval::interp::llvm_builder_build_cons`) — so no extra
      ;; rooting is needed here. `car`/`cdr` never allocate at all.
      ((ACons lhs rhs) (build-cons b module f (compile-value module b f params vals lhs) (compile-value module b f params vals rhs)))
      ((ACar v) (build-car b module f (compile-value module b f params vals v)))
      ((ACdr v) (build-cdr b module f (compile-value module b f params vals v)))
      ((ANullp v) (build-nullp b (compile-value module b f params vals v)))
      ;; `let` in *value* position: unlike `compile-tail`'s `ALet` (below),
      ;; control returns to *this* call's caller once `body` is evaluated
      ;; rather than falling through to a `build-ret*`, so any `Sexpr`-typed
      ;; bindings this scope pushed must be popped again right here, before
      ;; returning — see this module's doc comment.
      ;;
      ;; `copy-into vals new-vals ...` (ループ構文, `ASet`'s sibling fix):
      ;; before this arm existed pre-`setf`, nothing inside a `let`'s body
      ;; could be observed from outside it, so `new-vals` (an *extended
      ;; copy* of `vals` — see `extend-values`) never needed writing back.
      ;; Now that `setf` can mutate a binding from several scopes further
      ;; in (e.g. a `while` nested inside this `let`'s body, itself
      ;; `setf`-ing one of *this* scope's outer variables), this copies
      ;; every inherited index's possibly-updated value back into `vals` —
      ;; the same `Vector` instance every sibling form before/after this
      ;; `let`, or an enclosing `while`'s own loop-body scope, shares — so
      ;; the mutation becomes visible there too. `new-vals`'s *own* fresh
      ;; bindings (indices `>= (length vals)`) have nowhere to copy back
      ;; to, nor would it mean anything if they did (they don't exist
      ;; outside this `let`), so only the first `(length vals)` entries
      ;; are copied. This is recursive in effect: a `let` nested inside
      ;; another `let` copies back into its immediate parent, which then
      ;; copies *that* back into *its* parent, and so on out to wherever
      ;; the mutated variable actually lives.
      ((ALet names values body)
       (let* ((bound-vals (eval-args 0 (length values) values (Vector::new 0 (llvm-const-i64 0))))
              (new-params (extend-strs params names))
              (new-vals (extend-values vals bound-vals))
              (pushed (count-and-push-sexpr b module f bound-vals 0 (length bound-vals) 0))
              (result (eval-body 0 (length body) body new-params new-vals)))
         (progn (copy-into vals new-vals 0 (length vals)) (pop-roots b module f pushed) result)))
      ;; A call to another already-compiled function (Phase 2f) *or* a
      ;; member of the same self/mutual-recursion group (Phase 2g) —
      ;; `ast_bridge::typed_to_ast` refused to bridge it otherwise. Evaluate
      ;; its arguments in the *current* scope, resolve its declaration in
      ;; this module, and dispatch to `build-call`/`-bool`/`-f64`/`-char`/
      ;; `-sexpr` per its known return type (the same
      ;; `ret-is-bool`/`ret-is-f64`/`ret-is-char`/`ret-is-sexpr` side-query
      ;; `compile` itself uses for its own return). `f` (the function
      ;; *currently* being compiled, not `callee`) is passed through so
      ;; `build-call*` can forward its own `heap` parameter as the call's
      ;; 4th ABI argument — every compiled function takes one regardless of
      ;; whether its own body touches `Sexpr`.
      ((ACall name call-args)
       (let* ((arg-vals (eval-args 0 (length call-args) call-args (Vector::new 0 (llvm-const-i64 0))))
              (callee (get-or-declare-function module name)))
         (cond ((ret-is-bool name) (build-call-bool b callee f arg-vals))
               ((ret-is-f64 name) (build-call-f64 b callee f arg-vals))
               ((ret-is-char name) (build-call-char b callee f arg-vals))
               ((ret-is-sexpr name) (build-call-sexpr b callee f arg-vals))
               (else (build-call b callee f arg-vals)))))
      ;; `setf` on a local (ループ構文): evaluate the new value in the
      ;; *current* scope, then mutate `vals` in place at `name`'s index —
      ;; safe because every sibling form in the same body sequence
      ;; (`eval-body`/`eval-body-tail`, in `compile-value`/`compile-tail`)
      ;; is handed this exact same `Vector` instance, so the mutation is
      ;; visible to whatever runs next without needing to thread an
      ;; "updated vals" back out through a return value. This only holds
      ;; for straight-line code — across `AWhile`'s loop back-edge, the
      ;; analogous "make a later read see an earlier setf" job is done by
      ;; a phi node instead (see `AWhile` below for why a plain in-place
      ;; mutation isn't enough there).
      ((ASet name value)
       (let ((v (compile-value module b f params vals value)))
         (progn (set vals (param-index params name) v) v)))
      ;; `while` (ループ構文): phi-node loop compilation. `while`'s own type
      ;; is always `Unit`, so the placeholder `(llvm-const-i64 0)` results
      ;; below are never actually consumed by anything but `eval-body`'s
      ;; "discard every form but the last" sequencing — what matters is the
      ;; side effects (`ASet` mutations) and the final `copy-into`, which
      ;; makes every sibling form *after* the loop see each loop-carried
      ;; variable's correctly-merged post-loop value.
      ;;
      ;; `preheader` is captured *before* branching away, since by the time
      ;; we're back here for `add-incoming`'s preheader edge, `b` would
      ;; otherwise have moved on (see `current-block`'s doc comment).
      ;; `phis` (one per `vals` entry, unconditionally — see `make-phis`'s
      ;; doc comment) are built right after `position-at-end`ing `header`,
      ;; before `cond` is compiled, since LLVM requires every phi in a
      ;; block to precede non-phi instructions. `cond`/`body` are compiled
      ;; using `phis` as their scope's `vals` (not the outer `vals`), so
      ;; they see *this iteration's* merged values, not the stale
      ;; pre-loop ones. `body` runs against its own fresh copy of `phis`
      ;; (`body-vals`) rather than mutating `phis` itself, so `ASet`
      ;; inside the loop can't corrupt the header's own phi handles.
      ;; `latch` (like `preheader`) is read via `current-block` rather
      ;; than assumed to be `body-block`, so a nested loop (or any other
      ;; future construct that leaves `b` positioned somewhere else by the
      ;; end of `body`) still wires the back edge to the *actual*
      ;; predecessor. Finally, `copy-into` overwrites the *original* `vals`
      ;; with `phis` at `exit` — `header` dominates `exit` (its only
      ;; predecessor), so this is the one place reading a loop-carried
      ;; variable's value is always valid, on both the zero-iteration and
      ;; the looped path.
      ((AWhile cond body)
       (let* ((preheader (current-block b))
              (header (append-block f "while.header"))
              (body-block (append-block f "while.body"))
              (exit (append-block f "while.exit")))
         (progn
           (build-br b header)
           (position-at-end b header)
           (let ((phis (make-phis b vals 0 (length vals) (Vector::new 0 (llvm-const-i64 0)))))
             (progn
               (add-incoming-all b phis vals preheader 0 (length vals))
               (build-cond-br b (compile-value module b f params phis cond) body-block exit)
               (position-at-end b body-block)
               (let ((body-vals (copy-values phis 0 (length phis) (Vector::new 0 (llvm-const-i64 0)))))
                 (progn
                   (if (> (length body) 0) (eval-body 0 (length body) body params body-vals) (llvm-const-i64 0))
                   (let ((latch (current-block b)))
                     (progn (add-incoming-all b phis body-vals latch 0 (length phis)) (build-br b header)))))
               (position-at-end b exit)
               (progn (copy-into vals phis 0 (length vals)) (llvm-const-i64 0)))))))
      (_ (panic "compile: `if` is only supported in tail position (Phase 1)")))))

;; Under the unified `TlValue` ABI (Phase 2), a function's logical arguments
;; are no longer separate LLVM-level parameters — they live in the `args`
;; array (`f`'s first real parameter), so reading one is an IR-building
;; operation (`load-arg`/`load-arg-bool`/`load-arg-f64`, needs `b` positioned
;; at `entry`) rather than a pure metadata lookup (Phase 1's `get-param`).
;; `name` is the function being compiled, needed so `param-is-bool`/
;; `param-is-f64`/`param-is-char`/`param-is-sexpr` can tell which of the five
;; to use for each position (Phase 2b/2c/2d/3, mixed
;; `i64`/`bool`/`f64`/`char`/`Sexpr` signatures).
(defun fill-param-values ((name string) (f LlvmFunction) (b LlvmBuilder) (i i32) (n i32) (out Vector<LlvmValue>)) Vector<LlvmValue>
  (if (>= i n)
      out
      (progn (push out (cond ((param-is-bool name i) (load-arg-bool b f i))
                              ((param-is-f64 name i) (load-arg-f64 b f i))
                              ((param-is-char name i) (load-arg-char b f i))
                              ((param-is-sexpr name i) (load-arg-sexpr b f i))
                              (else (load-arg b f i))))
             (fill-param-values name f b (+ i 1) n out))))

(defun make-param-values ((name string) (f LlvmFunction) (b LlvmBuilder) (n i32)) Vector<LlvmValue>
  (fill-param-values name f b 0 n (Vector::new 0 (llvm-const-i64 0))))

;; `bool-ret`/`f64-ret`/`char-ret`/`sexpr-ret` (the function's own return
;; type, computed once by `compile` via
;; `ret-is-bool`/`ret-is-f64`/`ret-is-char`/`ret-is-sexpr`) pick
;; `build-ret`/`build-ret-bool`/`build-ret-f64`/`build-ret-char`/
;; `build-ret-sexpr` at every tail leaf — every leaf returns the same
;; statically-known type regardless of which `if` branch is taken, so four
;; flags threaded through suffice (Phase 2b/2c/2d/3; mutually exclusive,
;; since a function's return type is exactly one of
;; `i64`/`bool`/`f64`/`char`/`Sexpr`).
;; `pushed` (Phase 3) is the *cumulative* count of `Sexpr` GC roots pushed so
;; far in this dynamic scope (the function's own `Sexpr` parameters, plus
;; every enclosing tail-position `let`'s `Sexpr` bindings) — see this
;; module's doc comment on the rooting discipline. The one `_` arm that
;; actually builds a `ret` pops all of them right before doing so.
;; `ACall` has no arm of its own here — the `_` arm already calls
;; `compile-value` and feeds its result to `build-ret`/`-bool`/`-f64`/
;; `-char`/`-sexpr`, which is exactly right for a call in tail position too
;; (Phase 2f) — only control-flow-splitting forms (`AIf`/`ALet`, which build
;; more than one basic block) need their own `compile-tail` arm.
(defun compile-tail ((module LlvmModule) (bool-ret bool) (f64-ret bool) (char-ret bool) (sexpr-ret bool) (pushed i32) (f LlvmFunction) (b LlvmBuilder) (params Vector<string>) (vals Vector<LlvmValue>) (e AstExpr)) ()
  (match e
    ((AIf c then els)
     (let ((then-block (append-block f "then"))
           (else-block (append-block f "else")))
       (build-cond-br b (compile-value module b f params vals c) then-block else-block)
       (position-at-end b then-block)
       (compile-tail module bool-ret f64-ret char-ret sexpr-ret pushed f b params vals then)
       (position-at-end b else-block)
       (compile-tail module bool-ret f64-ret char-ret sexpr-ret pushed f b params vals els)))
    ;; `let` in *tail* position: pushes its `Sexpr`-typed bindings (added to
    ;; `pushed`'s running total) but never pops them itself — control falls
    ;; straight through `eval-body-tail`'s last form into a deeper
    ;; `compile-tail` call (possibly through more nested `if`/`let`s) that
    ;; eventually reaches this very `_` arm, which pops *everything*
    ;; (`pushed` plus whatever this `let` and any others added) in one shot
    ;; right before its `build-ret*` — see this module's doc comment.
    ((ALet names values body)
     (labels ((eval-bindings ((i i32) (n i32) (exprs Vector<AstExpr>) (out Vector<LlvmValue>)) Vector<LlvmValue>
                (if (>= i n)
                    out
                    (progn (push out (compile-value module b f params vals (get exprs i)))
                           (eval-bindings (+ i 1) n exprs out))))
              (eval-body-tail ((i i32) (n i32) (forms Vector<AstExpr>) (p2 Vector<string>) (v2 Vector<LlvmValue>) (pushed2 i32)) ()
                (if (>= (+ i 1) n)
                    (compile-tail module bool-ret f64-ret char-ret sexpr-ret pushed2 f b p2 v2 (get forms i))
                    (progn (compile-value module b f p2 v2 (get forms i))
                           (eval-body-tail (+ i 1) n forms p2 v2 pushed2)))))
       (let* ((bound-vals (eval-bindings 0 (length values) values (Vector::new 0 (llvm-const-i64 0))))
              (new-params (extend-strs params names))
              (new-vals (extend-values vals bound-vals))
              (new-pushed (count-and-push-sexpr b module f bound-vals 0 (length bound-vals) pushed)))
         (eval-body-tail 0 (length body) body new-params new-vals new-pushed))))
    ;; The result must be fully evaluated *before* `pop-root`-ing anything —
    ;; evaluating `e` may itself call `build-cons` (e.g. a tail-position
    ;; `(cons x y)`), which needs every still-live `Sexpr` value (including
    ;; ones this `pushed` count tracks) rooted for the duration of that call.
    ;; Only once `result` is a plain LLVM value (no longer requiring a `cons`
    ;; call to produce) is it safe to pop everything and `build-ret*` —
    ;; neither `pop-root` nor `build-ret*` themselves ever trigger a GC, so
    ;; nothing can reclaim `result` in between.
    (_ (let ((result (compile-value module b f params vals e)))
         (progn (pop-roots b module f pushed)
                (cond (bool-ret (build-ret-bool b f result))
                      (f64-ret (build-ret-f64 b f result))
                      (char-ret (build-ret-char b f result))
                      (sexpr-ret (build-ret-sexpr b f result))
                      (else (build-ret b f result))))))))

;; `singleton-group` builds the one-element `Vector<string>` `compile` passes
;; to `ast-body` for plain self-recursion bridging (Phase 2g). It takes the
;; destination vector as a parameter rather than building one with
;; `Vector::new` directly in a `let` binding: a `let` binding's initializer
;; is always checked with no expected type (`Checker::check_let` hardcodes
;; `None`), and `Vector::new` is a *static* call whose type parameter `t` is
;; only ever inferred from an expected type, never from its own arguments —
;; so `(let ((g (Vector::new 0 "")) ...)` fails to type-check, while passing
;; the same `(Vector::new 0 "")` as a call *argument* to an already-concrete
;; parameter type (as every other `Vector::new` site in this file already
;; does, e.g. `extend-strs`/`make-param-values`) lets it pick up that
;; parameter's declared type as its expected type instead.
(defun singleton-group ((name string) (out Vector<string>)) Vector<string>
  (progn (push out name) out))

(defun compile ((name string)) Result<bool,Error>
  (match (ast-params name)
    ((None) (Err (Error "compile: unsupported function (must take/return only i64, bool, f64, char, or Sexpr)")))
    ((Some params)
     (match (ast-body name (singleton-group name (Vector::new 0 "")))
       ((None) (Err (Error "compile: unsupported function body")))
       ((Some body)
          (let* ((arity (length params))
                 (bool-ret (ret-is-bool name))
                 (f64-ret (ret-is-f64 name))
                 (char-ret (ret-is-char name))
                 (sexpr-ret (ret-is-sexpr name))
                 (module (llvm-new-module name))
                 (f (add-function module name))
                 (entry (append-block f "entry"))
                 (b (llvm-new-builder)))
            (position-at-end b entry)
            (let* ((vals (make-param-values name f b arity))
                   (pushed (count-and-push-sexpr b module f vals 0 (length vals) 0)))
              (compile-tail module bool-ret f64-ret char-ret sexpr-ret pushed f b params vals body)
              (match (verify module)
                ((Err e) (Err e))
                ((Ok _) (llvm-finish-compile module name arity))))))))))

;; `compile-group` (Phase 2g, mutual recursion): compiles several `defun`s
;; *together* into one shared `LlvmModule`, so each can call any other
;; before any of them is registered in `Interp.compiled` — the same trick
;; `compile`'s singleton `group` above uses for plain self-recursion,
;; generalized to an arbitrary list of mutually-calling names.
;;
;; `declare-all` adds every name's bare `LlvmFunction` declaration to
;; `module` *before* any body is built, so that when member `i`'s body
;; (built by `build-one`) calls member `j` (`i` < or > `j`, order doesn't
;; matter), `get-or-declare-function` always finds `j`'s declaration already
;; present — exactly the forward-reference LLVM IR allows (a `call` to a
;; declared-but-not-yet-defined function is legal as long as a body is
;; eventually attached, which `build-one` does for every member by the time
;; `compile-group` returns).
(defun declare-all ((names Vector<string>) (module LlvmModule) (i i32) (n i32)) ()
  (if (>= i n)
      ()
      (progn (add-function module (get names i)) (declare-all names module (+ i 1) n))))

;; Builds member `i`'s entry block and body — the same steps `compile`
;; performs for its one function, except the `LlvmFunction` comes from
;; `get-or-declare-function` (reusing `declare-all`'s declaration) instead of
;; a fresh `add-function`, and `ast-body`'s `group` is the *whole* `names`
;; list rather than a singleton.
(defun build-one ((names Vector<string>) (module LlvmModule) (b LlvmBuilder) (i i32)) Result<bool,Error>
  (let* ((name (get names i)))
    (match (ast-params name)
      ((None) (Err (Error "compile-group: unsupported function (must take/return only i64, bool, f64, char, or Sexpr)")))
      ((Some params)
       (match (ast-body name names)
         ((None) (Err (Error "compile-group: unsupported function body")))
         ((Some body)
          (let* ((arity (length params))
                 (bool-ret (ret-is-bool name))
                 (f64-ret (ret-is-f64 name))
                 (char-ret (ret-is-char name))
                 (sexpr-ret (ret-is-sexpr name))
                 (f (get-or-declare-function module name))
                 (entry (append-block f "entry")))
            (position-at-end b entry)
            (let* ((vals (make-param-values name f b arity))
                   (pushed (count-and-push-sexpr b module f vals 0 (length vals) 0)))
              (compile-tail module bool-ret f64-ret char-ret sexpr-ret pushed f b params vals body)
              (Ok true)))))))))

(defun build-all ((names Vector<string>) (module LlvmModule) (b LlvmBuilder) (i i32) (n i32)) Result<bool,Error>
  (if (>= i n)
      (Ok true)
      (match (build-one names module b i)
        ((Err e) (Err e))
        ((Ok _) (build-all names module b (+ i 1) n)))))

(defun compile-group ((names Vector<string>)) Result<bool,Error>
  (let* ((module (llvm-new-module (get names 0)))
         (b (llvm-new-builder))
         (n (length names)))
    (declare-all names module 0 n)
    (match (build-all names module b 0 n)
      ((Err e) (Err e))
      ((Ok _)
       (match (verify module)
         ((Err e) (Err e))
         ((Ok _) (llvm-finish-compile-group module names)))))))
"#;

/// Read, check, and execute [`SOURCE`] against `heap`/`chk`/`interp`,
/// registering `compile` exactly as if the caller had typed it first — the
/// same fixed/known-good-so-panic-on-failure contract as [`crate::prelude::load`].
/// Must be called after [`crate::prelude::load`] (it relies on prelude names
/// like `length`/`get`/`push`).
pub fn load(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) {
    let r = Reader::new();
    let forms = r.read_all(heap, SOURCE).expect("compiler_source: read failed");
    for v in forms {
        let tl = chk.check_form(heap, &*interp, v).expect("compiler_source: check failed");
        interp.exec(heap, tl).expect("compiler_source: eval failed");
    }
}
