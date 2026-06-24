//! The (typelisp-hosted) compiler body: AST (bridged to `Sexpr` by
//! [`crate::compile::ast_bridge`]) -> LLVM IR, built by calling the
//! `llvm-*` builtins (`crate::eval::interp`'s `eval_llvm_builtin_method`)
//! directly — the same "Rust provides the bindings, typelisp drives them"
//! split [docs/TODO.md](../docs/TODO.md) calls for. Loaded the same way
//! `prelude.rs` loads the standard library: read -> check -> exec each
//! top-level form once, against the same `Heap`/`Checker`/`Interp` the rest
//! of the program uses.
//!
//! Compiles the node shapes `ast_bridge::ast_to_sexpr` actually produces a
//! real translation for today — integer literals (`(int n)`) and `i64`
//! arithmetic (`(var name)`/`(assoc type method instance arg...)`, for
//! `+`/`-`/`*` only). `compile-value` grows a new tag-/method-matching arm
//! as later phases teach `ast_bridge` to translate more `Expr` variants for
//! real. No `if`/`let`/`loop` yet.
//!
//! `compile-value`/`compile-int`/`compile-var`/`compile-assoc` are CL
//! `labels` (mutually recursive *local* functions — see
//! `check::ast::Expr::Labels`), not top-level `defun`s: a `defun` may only
//! ever be self-recursive, never part of a forward-referencing/mutually-
//! recursive top-level group (each `defun` form is read -> checked ->
//! exec'd before the next one exists at all — see `Checker::check_defun`'s
//! doc comment), so `compile-assoc` calling back into `compile-value` would
//! be a forward reference if both were separate `defun`s. CL's non-
//! recursive `flet` doesn't help here (these calls *are* mutual recursion);
//! `labels` is the right tool, and nesting them all inside the one
//! `compile-function` entry point that's actually called from Rust
//! (`Interp::compile_function`) sidesteps the top-level restriction
//! entirely — its `labels` block also lets every one of them close over
//! `builder`/`env` instead of threading them through every call.
//!
//! Doesn't depend on `prelude.rs` (no `cond`/`when`/...) — only the
//! checker's native special forms (`if`/`let`/`match`/`labels`) and
//! builtins (`eq`/`append`/`car`/`cdr`/`HashTable`'s methods), so loading
//! order relative to the prelude doesn't matter.

use crate::{Checker, Heap, Interp, Reader};

pub const SOURCE: &str = r#"
(defun sexpr-str ((s Sexpr)) string
  (match s
    ((Str v) v)
    (_ (panic "expected a Str Sexpr node"))))

(defun bind-params ((env HashTable<string,llvm-value>) (builder llvm-builder) (f llvm-function) (names Sexpr) (idx i32)) ()
  (match names
    ((Cons (Sym name) rest)
     (set env name (load-arg builder f idx))
     (bind-params env builder f rest (+ idx 1)))
    (_ ())))

;; `HashTable::new` is a static call whose generic `V`/`K` can only be
;; inferred from an *expected* type (e.g. a `defun`'s declared return type),
;; never from a `let` binding's initializer (`check_let` always checks that
;; with `expected: None` — see `Checker::check_let`) — so this tiny wrapper
;; exists purely to give `HashTable::new` a return type to infer from.
(defun new-env () HashTable<string,llvm-value> (HashTable::new))

(defun compile-function ((name string) (param-names Sexpr) (body Sexpr)) llvm-module
  (let ((m (llvm-module::create "compiled")))
    (let ((f (add-function m name)))
      (let ((b (append-block f "entry")))
        (let ((builder (llvm-builder::create)))
          (position-at-end builder b)
          (let ((env (new-env)))
            (bind-params env builder f param-names 0)
            (labels ((compile-value ((e Sexpr)) llvm-value
                       (match (car e)
                         ((Sym s)
                          (if (eq s "int")
                              (compile-int e)
                              (if (eq s "var")
                                  (compile-var e)
                                  (if (eq s "assoc")
                                      (compile-assoc e)
                                      (panic (append "compile-value: unsupported tag " s))))))
                         (_ (panic "compile-value: malformed node, expected a tagged list"))))
                     (compile-int ((e Sexpr)) llvm-value
                       (match (car (cdr e))
                         ((Int n) (const-i64 builder n))
                         (_ (panic "compile-int: malformed int node"))))
                     (compile-var ((e Sexpr)) llvm-value
                       (let ((var-name (sexpr-str (car (cdr e)))))
                         (match (get env var-name)
                           ((Some v) v)
                           (None (panic (append "compile-var: unbound variable " var-name))))))
                     (compile-assoc ((e Sexpr)) llvm-value
                       (let ((method (sexpr-str (car (cdr (cdr e))))))
                         (let ((rest (cdr (cdr (cdr (cdr e))))))
                           (let ((a (compile-value (car rest))))
                             (let ((b2 (compile-value (car (cdr rest)))))
                               (if (eq method "+")
                                   (build-add builder a b2)
                                   (if (eq method "-")
                                       (build-sub builder a b2)
                                       (if (eq method "*")
                                           (build-mul builder a b2)
                                           (panic (append "compile-assoc: unsupported method " method)))))))))))
              (let ((v (compile-value body)))
                (build-ret builder v)
                m))))))))
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
