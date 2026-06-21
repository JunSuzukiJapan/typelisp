//! `compile`: a function-level compiler **written in typelisp itself**, on
//! top of the LLVM-builder bindings and typed-AST bridge Rust provides (see
//! `crate::check::registry::register_compile_builtins` and
//! `crate::compile::ast_bridge`) — the same "library written in typelisp
//! calling Rust builtins" shape as [`crate::prelude`], not a Rust special form.
//!
//! Phase 1/2e ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) restricts
//! `compile` to functions whose every parameter and return type is `i64`,
//! `bool`, or `f64` (any mix — e.g. `(defun gt ((a i64) (b i64)) bool (> a
//! b))`), with a body built from integer/bool/float literals, parameter
//! references, `i64`/`f64` arithmetic/comparison, `let`, and `if` — and `if`
//! only in **tail position** (the value an enclosing `if`/the function
//! itself returns), not as a nested sub-expression: `compile-tail` lowers a
//! tail `if` to two basic blocks each ending in their own `ret`,
//! sidestepping the need for a phi node or alloca/load/store merge this
//! narrow first slice doesn't need yet (see `compile-value`'s fallback
//! `panic` for the case this restriction rules out).

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

;; `ALet`'s `eval-bindings`/`eval-body` helpers are `labels`-local (not their
;; own top-level `defun`s) because they call `compile-value` — a `defun` may
;; only call itself or an *already-defined* `defun`, never one defined later,
;; so a separate top-level helper calling back into `compile-value` (still
;; being defined) would be a forward reference `labels` sidesteps (the helper
;; is local to `compile-value`'s own body, which is already in scope by the
;; time it's checked, exactly like ordinary self-recursion).
(defun compile-value ((b LlvmBuilder) (params Vector<string>) (vals Vector<LlvmValue>) (e AstExpr)) LlvmValue
  (match e
    ((AInt n) (llvm-const-i64 n))
    ((ABool flag) (llvm-const-bool flag))
    ((AFloat n) (llvm-const-f64 n))
    ((AVar name) (get vals (param-index params name)))
    ((ABinOp op lhs rhs) (build-op b op (compile-value b params vals lhs) (compile-value b params vals rhs)))
    ((ALet names values body)
     (labels ((eval-bindings ((i i32) (n i32) (exprs Vector<AstExpr>) (out Vector<LlvmValue>)) Vector<LlvmValue>
                (if (>= i n)
                    out
                    (progn (push out (compile-value b params vals (get exprs i)))
                           (eval-bindings (+ i 1) n exprs out))))
              (eval-body ((i i32) (n i32) (forms Vector<AstExpr>) (p2 Vector<string>) (v2 Vector<LlvmValue>)) LlvmValue
                (let ((r (compile-value b p2 v2 (get forms i))))
                  (if (>= (+ i 1) n) r (eval-body (+ i 1) n forms p2 v2)))))
       (let* ((bound-vals (eval-bindings 0 (length values) values (Vector::new 0 (llvm-const-i64 0))))
              (new-params (extend-strs params names))
              (new-vals (extend-values vals bound-vals)))
         (eval-body 0 (length body) body new-params new-vals))))
    (_ (panic "compile: `if` is only supported in tail position (Phase 1)"))))

;; Under the unified `TlValue` ABI (Phase 2), a function's logical arguments
;; are no longer separate LLVM-level parameters — they live in the `args`
;; array (`f`'s first real parameter), so reading one is an IR-building
;; operation (`load-arg`/`load-arg-bool`/`load-arg-f64`, needs `b` positioned
;; at `entry`) rather than a pure metadata lookup (Phase 1's `get-param`).
;; `name` is the function being compiled, needed so `param-is-bool`/
;; `param-is-f64` can tell which of the three to use for each position
;; (Phase 2b/2c, mixed `i64`/`bool`/`f64` signatures).
(defun fill-param-values ((name string) (f LlvmFunction) (b LlvmBuilder) (i i32) (n i32) (out Vector<LlvmValue>)) Vector<LlvmValue>
  (if (>= i n)
      out
      (progn (push out (cond ((param-is-bool name i) (load-arg-bool b f i))
                              ((param-is-f64 name i) (load-arg-f64 b f i))
                              (else (load-arg b f i))))
             (fill-param-values name f b (+ i 1) n out))))

(defun make-param-values ((name string) (f LlvmFunction) (b LlvmBuilder) (n i32)) Vector<LlvmValue>
  (fill-param-values name f b 0 n (Vector::new 0 (llvm-const-i64 0))))

;; `bool-ret`/`f64-ret` (the function's own return type, computed once by
;; `compile` via `ret-is-bool`/`ret-is-f64`) pick `build-ret`/`build-ret-bool`/
;; `build-ret-f64` at every tail leaf — every leaf returns the same
;; statically-known type regardless of which `if` branch is taken, so two
;; flags threaded through suffice (Phase 2b/2c; mutually exclusive, since a
;; function's return type is exactly one of `i64`/`bool`/`f64`).
(defun compile-tail ((bool-ret bool) (f64-ret bool) (f LlvmFunction) (b LlvmBuilder) (params Vector<string>) (vals Vector<LlvmValue>) (e AstExpr)) ()
  (match e
    ((AIf c then els)
     (let ((then-block (append-block f "then"))
           (else-block (append-block f "else")))
       (build-cond-br b (compile-value b params vals c) then-block else-block)
       (position-at-end b then-block)
       (compile-tail bool-ret f64-ret f b params vals then)
       (position-at-end b else-block)
       (compile-tail bool-ret f64-ret f b params vals els)))
    ((ALet names values body)
     (labels ((eval-bindings ((i i32) (n i32) (exprs Vector<AstExpr>) (out Vector<LlvmValue>)) Vector<LlvmValue>
                (if (>= i n)
                    out
                    (progn (push out (compile-value b params vals (get exprs i)))
                           (eval-bindings (+ i 1) n exprs out))))
              (eval-body-tail ((i i32) (n i32) (forms Vector<AstExpr>) (p2 Vector<string>) (v2 Vector<LlvmValue>)) ()
                (if (>= (+ i 1) n)
                    (compile-tail bool-ret f64-ret f b p2 v2 (get forms i))
                    (progn (compile-value b p2 v2 (get forms i))
                           (eval-body-tail (+ i 1) n forms p2 v2)))))
       (let* ((bound-vals (eval-bindings 0 (length values) values (Vector::new 0 (llvm-const-i64 0))))
              (new-params (extend-strs params names))
              (new-vals (extend-values vals bound-vals)))
         (eval-body-tail 0 (length body) body new-params new-vals))))
    (_ (cond (bool-ret (build-ret-bool b f (compile-value b params vals e)))
             (f64-ret (build-ret-f64 b f (compile-value b params vals e)))
             (else (build-ret b f (compile-value b params vals e)))))))

(defun compile ((name string)) Result<bool,Error>
  (match (ast-params name)
    ((None) (Err (Error "compile: unsupported function (must take/return only i64, bool, or f64)")))
    ((Some params)
     (match (ast-body name)
       ((None) (Err (Error "compile: unsupported function body")))
       ((Some body)
        (let* ((arity (length params))
               (bool-ret (ret-is-bool name))
               (f64-ret (ret-is-f64 name))
               (module (llvm-new-module name))
               (f (add-function module name))
               (entry (append-block f "entry"))
               (b (llvm-new-builder)))
          (position-at-end b entry)
          (let ((vals (make-param-values name f b arity)))
            (compile-tail bool-ret f64-ret f b params vals body)
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
