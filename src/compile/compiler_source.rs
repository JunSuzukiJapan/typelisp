//! `compile`: a function-level compiler **written in typelisp itself**, on
//! top of the LLVM-builder bindings and typed-AST bridge Rust provides (see
//! `crate::check::registry::register_compile_builtins` and
//! `crate::compile::ast_bridge`) — the same "library written in typelisp
//! calling Rust builtins" shape as [`crate::prelude`], not a Rust special form.
//!
//! Phase 1 ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) restricts `compile`
//! to functions whose every parameter and return type is `i64`, with a body
//! built from integer/bool literals, parameter references, `i64`
//! arithmetic/comparison, and `if` — and `if` only in **tail position** (the
//! value an enclosing `if`/the function itself returns), not as a nested
//! sub-expression: `compile-tail` lowers a tail `if` to two basic blocks each
//! ending in their own `ret`, sidestepping the need for a phi node or
//! alloca/load/store merge this narrow first slice doesn't need yet (see
//! `compile-value`'s fallback `panic` for the case this restriction rules out).

use crate::{Checker, Heap, Interp, Reader};

pub const SOURCE: &str = r#"
(defun index-of ((name string) (i i32) (params Vector<string>)) i32
  (if (>= i (length params))
      (panic "compile: unknown variable")
      (if (eq name (get params i)) i (index-of name (+ i 1) params))))

(defun param-index ((params Vector<string>) (name string)) i32
  (index-of name 0 params))

(defun compile-value ((b LlvmBuilder) (params Vector<string>) (vals Vector<LlvmValue>) (e AstExpr)) LlvmValue
  (match e
    ((AInt n) (llvm-const-i64 n))
    ((AVar name) (get vals (param-index params name)))
    ((ABinOp op lhs rhs) (build-op b op (compile-value b params vals lhs) (compile-value b params vals rhs)))
    (_ (panic "compile: `if` is only supported in tail position (Phase 1)"))))

;; Under the unified `TlValue` ABI (Phase 2), a function's logical arguments
;; are no longer separate LLVM-level parameters — they live in the `args`
;; array (`f`'s first real parameter), so reading one is an IR-building
;; operation (`load-arg`, needs `b` positioned at `entry`) rather than a pure
;; metadata lookup (Phase 1's `get-param`).
(defun fill-param-values ((f LlvmFunction) (b LlvmBuilder) (i i32) (n i32) (out Vector<LlvmValue>)) Vector<LlvmValue>
  (if (>= i n)
      out
      (progn (push out (load-arg b f i)) (fill-param-values f b (+ i 1) n out))))

(defun make-param-values ((f LlvmFunction) (b LlvmBuilder) (n i32)) Vector<LlvmValue>
  (fill-param-values f b 0 n (Vector::new 0 (llvm-const-i64 0))))

(defun compile-tail ((f LlvmFunction) (b LlvmBuilder) (params Vector<string>) (vals Vector<LlvmValue>) (e AstExpr)) ()
  (match e
    ((AIf c then els)
     (let ((then-block (append-block f "then"))
           (else-block (append-block f "else")))
       (build-cond-br b (compile-value b params vals c) then-block else-block)
       (position-at-end b then-block)
       (compile-tail f b params vals then)
       (position-at-end b else-block)
       (compile-tail f b params vals els)))
    (_ (build-ret b f (compile-value b params vals e)))))

(defun compile ((name string)) Result<bool,Error>
  (match (ast-params name)
    ((None) (Err (Error "compile: unsupported function (must take/return only i64)")))
    ((Some params)
     (match (ast-body name)
       ((None) (Err (Error "compile: unsupported function body")))
       ((Some body)
        (let* ((arity (length params))
               (module (llvm-new-module name))
               (f (add-function module name))
               (entry (append-block f "entry"))
               (b (llvm-new-builder)))
          (position-at-end b entry)
          (let ((vals (make-param-values f b arity)))
            (compile-tail f b params vals body)
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
