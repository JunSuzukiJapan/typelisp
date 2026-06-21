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
//! outright, rather than reaching `compile-value` with no callee to call;
//! self/mutual recursion between compiled functions isn't supported yet for
//! the same reason: a function isn't registered as compiled until *after*
//! `compile` finishes building it). `if` is only supported in **tail
//! position** (the value an enclosing `if`/the function itself returns),
//! not as a nested sub-expression: `compile-tail` lowers a tail `if` to two
//! basic blocks each ending in their own `ret`, sidestepping the need for a
//! phi node or alloca/load/store merge this narrow first slice doesn't need
//! yet (see `compile-value`'s fallback `panic` for the case this
//! restriction rules out).
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
      ((ALet names values body)
       (let* ((bound-vals (eval-args 0 (length values) values (Vector::new 0 (llvm-const-i64 0))))
              (new-params (extend-strs params names))
              (new-vals (extend-values vals bound-vals))
              (pushed (count-and-push-sexpr b module f bound-vals 0 (length bound-vals) 0))
              (result (eval-body 0 (length body) body new-params new-vals)))
         (progn (pop-roots b module f pushed) result)))
      ;; A call to another (already-compiled — `ast_bridge::typed_to_ast`
      ;; refused to bridge it otherwise, Phase 2f) function: evaluate its
      ;; arguments in the *current* scope, resolve its declaration in this
      ;; module, and dispatch to `build-call`/`-bool`/`-f64`/`-char`/`-sexpr`
      ;; per its known return type (the same
      ;; `ret-is-bool`/`ret-is-f64`/`ret-is-char`/`ret-is-sexpr` side-query
      ;; `compile` itself uses for its own return).
      ((ACall name call-args)
       (let* ((arg-vals (eval-args 0 (length call-args) call-args (Vector::new 0 (llvm-const-i64 0))))
              (callee (get-or-declare-function module name)))
         (cond ((ret-is-bool name) (build-call-bool b callee arg-vals))
               ((ret-is-f64 name) (build-call-f64 b callee arg-vals))
               ((ret-is-char name) (build-call-char b callee arg-vals))
               ((ret-is-sexpr name) (build-call-sexpr b callee arg-vals))
               (else (build-call b callee arg-vals)))))
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

(defun compile ((name string)) Result<bool,Error>
  (match (ast-params name)
    ((None) (Err (Error "compile: unsupported function (must take/return only i64, bool, f64, char, or Sexpr)")))
    ((Some params)
     (match (ast-body name)
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
