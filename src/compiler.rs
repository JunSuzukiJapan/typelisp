//! The (typelisp-hosted) compiler body: AST (bridged to `Sexpr` by
//! [`crate::compile::ast_bridge`]) -> LLVM IR, built by calling the
//! `llvm-*` builtins (`crate::eval::interp`'s `eval_llvm_builtin_method`)
//! directly — the same "Rust provides the bindings, typelisp drives them"
//! split [docs/TODO.md](../docs/TODO.md) calls for. Loaded the same way
//! `prelude.rs` loads the standard library: read -> check -> exec each
//! top-level form once, against the same `Heap`/`Checker`/`Interp` the rest
//! of the program uses.
//!
//! Phase 0 only compiles the one node shape `ast_bridge::ast_to_sexpr`
//! actually produces a real translation for today — `(int n)` (from
//! `Expr::Int`) — into a zero-parameter, `i64`-returning function. That's
//! enough to prove the self-hosted body can drive the `llvm-*` builtins
//! end to end; `compile-value` grows a new tag-matching arm as later phases
//! teach `ast_bridge` to translate more `Expr` variants for real.

use crate::{Checker, Heap, Interp, Reader};

pub const SOURCE: &str = r#"
(defun compile-value ((e Sexpr) (builder llvm-builder)) llvm-value
  (match (car e)
    ((Sym s)
     (if (eq s "int")
         (match (car (cdr e))
           ((Int n) (const-i64 builder n))
           (_ (panic "compile-value: malformed int node")))
         (panic (append "compile-value: unsupported tag " s))))
    (_ (panic "compile-value: malformed node, expected a tagged list"))))

(defun compile-constant-function ((name string) (body Sexpr)) string
  (let ((m (llvm-module::create "compiled")))
    (let ((f (add-function m name)))
      (let ((b (append-block f "entry")))
        (let ((builder (llvm-builder::create)))
          (position-at-end builder b)
          (let ((v (compile-value body builder)))
            (build-ret builder v)
            (to-string m)))))))
"#;

/// Loads the compiler body. Like [`crate::load_prelude`], `SOURCE` is fixed
/// and known-good, so a failure here is a bug in this file, not a user
/// error — panics rather than threading a `Result` callers couldn't
/// meaningfully recover from.
pub fn load(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) {
    let r = Reader::new();
    let forms = r.read_all(heap, SOURCE).expect("compiler: read failed");
    for v in forms {
        let tl = chk.check_form(heap, &*interp, v).expect("compiler: check failed");
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        interp.exec(heap, tl).expect("compiler: eval failed");
    }
}
