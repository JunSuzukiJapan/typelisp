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
//! `+`/`-`/`*` only). `compile-value`/`compile-assoc` grow a new
//! tag-/method-matching arm as later phases teach `ast_bridge` to translate
//! more `Expr` variants for real. No `if`/`let`/`loop`/recursion yet — every
//! `compile-*` function here is itself a *non-recursive* tree walk one level
//! deep (an `Expr::Assoc`'s arguments are themselves only literals/vars,
//! never another `Assoc`) until that phase lands.
//!
//! Doesn't depend on `prelude.rs` (no `cond`/`when`/...) — only the
//! checker's native special forms (`if`/`let`/`match`) and builtins
//! (`eq`/`append`/`car`/`cdr`/`HashTable`'s methods), so loading order
//! relative to the prelude doesn't matter.

use crate::{Checker, Heap, Interp, Reader};

pub const SOURCE: &str = r#"
(defun sexpr-str ((s Sexpr)) string
  (match s
    ((Str v) v)
    (_ (panic "expected a Str Sexpr node"))))

;; `compile-value` recurses into itself for `Expr::Assoc`'s arguments (e.g.
;; `(+ a b)`'s `a`/`b`, which may themselves be `(assoc ...)` nodes — nested
;; arithmetic). A `defun` may only ever be self-recursive, never part of a
;; forward-referencing/mutually-recursive top-level group (each `defun` form
;; is read -> checked -> exec'd before the next one exists at all — see
;; `Checker::check_defun`'s doc comment) — so the would-be helpers
;; `compile-int-literal`/`compile-var`/`compile-assoc` are inlined into this
;; one self-recursive function instead of being separate `defun`s that would
;; need to call back into `compile-value`.
(defun compile-value ((e Sexpr) (env HashTable<string,llvm-value>) (builder llvm-builder)) llvm-value
  (match (car e)
    ((Sym s)
     (if (eq s "int")
         (match (car (cdr e))
           ((Int n) (const-i64 builder n))
           (_ (panic "compile-value: malformed int node")))
         (if (eq s "var")
             (let ((name (sexpr-str (car (cdr e)))))
               (match (get env name)
                 ((Some v) v)
                 (None (panic (append "compile-value: unbound variable " name)))))
             (if (eq s "assoc")
                 (let ((method (sexpr-str (car (cdr (cdr e))))))
                   (let ((rest (cdr (cdr (cdr (cdr e))))))
                     (let ((a (compile-value (car rest) env builder)))
                       (let ((b (compile-value (car (cdr rest)) env builder)))
                         (if (eq method "+")
                             (build-add builder a b)
                             (if (eq method "-")
                                 (build-sub builder a b)
                                 (if (eq method "*")
                                     (build-mul builder a b)
                                     (panic (append "compile-value: unsupported method " method)))))))))
                 (panic (append "compile-value: unsupported tag " s))))))
    (_ (panic "compile-value: malformed node, expected a tagged list"))))

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
            (let ((v (compile-value body env builder)))
              (build-ret builder v)
              m)))))))
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
