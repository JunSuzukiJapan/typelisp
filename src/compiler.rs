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
//! real translation for today — integer literals (`(int n)`), `i64`
//! arithmetic (`(var name)`/`(assoc type method instance arg...)`, for
//! `+`/`-`/`*` only), and direct calls (`(apply name arg...)`/
//! `(labels ((name (params) body))... trailing-body)`, the labels-
//! compilation work — see `compile-labels`'s doc comment below for scope).
//! `compile-value` grows a new tag-/method-matching arm as later phases
//! teach `ast_bridge` to translate more `Expr` variants for real. No
//! `if`/`let`/`loop`/top-level `Expr::Call` yet.
//!
//! `compile-function` takes the destination `llvm-module` as a parameter
//! rather than creating its own — added in Phase 2 (the AOT exit,
//! `compile::aot`) so multiple `defun`s can be compiled into *one* shared
//! module (one `add-function` call each) without ever needing to take
//! exclusive Rust-side ownership of an `llvm-module` value back out of a
//! typelisp slot. That turns out to be impossible in general: `labels`
//! (just below) deliberately builds a reference cycle between its mutually
//! recursive closures (each one's captured environment includes every
//! sibling, itself included, to support the mutual recursion at all — see
//! `Expr::Labels`'s eval doc comment) — `compile-value` & co. close over
//! `m` along with `builder`/`env`, so that cycle keeps `m`'s slot, and
//! hence the `Rc<RefCell<Module>>` inside it, alive forever. `Rc::try_unwrap`
//! on a value `compile-function` itself returned would therefore always
//! fail. Letting the Rust caller own the one `Rc<RefCell<Module>>` from the
//! start (creating it itself, passing it in by reference, never trying to
//! reclaim sole ownership) sidesteps the cycle entirely — see
//! `Interp::add_compiled_function`.
//!
//! `compile-value`/`compile-int`/`compile-var`/`compile-assoc`/`compile-apply`/
//! `compile-labels`/... are CL `labels` (mutually recursive *local*
//! functions — see `check::ast::Expr::Labels`), not top-level `defun`s: a
//! `defun` may only ever be self-recursive, never part of a forward-
//! referencing/mutually-recursive top-level group (each `defun` form is
//! read -> checked -> exec'd before the next one exists at all — see
//! `Checker::check_defun`'s doc comment), so `compile-assoc` calling back
//! into `compile-value` would be a forward reference if both were separate
//! `defun`s. CL's non-recursive `flet` doesn't help here (these calls *are*
//! mutual recursion); `labels` is the right tool, and nesting them all
//! inside the one `compile-function` entry point that's actually called
//! from Rust (`Interp::add_compiled_function`) sidesteps the top-level
//! restriction entirely.
//!
//! Unlike Phase 0/1, `builder`/`env`/`fn-env` are now **explicit**
//! parameters threaded through every one of these functions, not values
//! they close over: `compile-labels` (below) needs to compile each
//! `labels`-sibling's body with *its own* fresh `builder`/`env` (a
//! different LLVM function, a different block, different parameter
//! bindings) while still reusing the very same `compile-value` dispatcher
//! — something closing over one fixed `builder`/`env` at the `labels`
//! block's own definition site (Phase 1's design) can't do. `fn-env:
//! HashTable<string,llvm-function>` is the new table parallel to `env`
//! that makes a name resolve to a *callable function* rather than a value
//! — see `compile-apply`'s doc comment.
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

;; See `new-env`'s comment — same reason this exists. `fn-env` maps a
;; direct-callable name (a `labels` sibling, or itself) to the
;; already-declared `llvm-function` `compile-apply` calls.
(defun new-fn-env () HashTable<string,llvm-function> (HashTable::new))

;; Counts a plain `Sexpr` list's elements — used to size the `i64*` args
;; array a direct call needs (`compile-apply`'s `alloca-args`/`build-call`).
(defun sexpr-list-length ((s Sexpr)) i32
  (match s
    ((Cons _ rest) (+ 1 (sexpr-list-length rest)))
    (_ 0)))

(defun compile-function ((m llvm-module) (name string) (param-names Sexpr) (body Sexpr)) llvm-module
    (let ((f (add-function m name)))
      (let ((b (append-block f "entry")))
        (let ((builder (llvm-builder::create)))
          (position-at-end builder b)
          (let ((env (new-env)))
            (bind-params env builder f param-names 0)
            (let ((fn-env (new-fn-env)))
              (labels ((compile-value ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (e Sexpr)) llvm-value
                         (match (car e)
                           ((Sym s)
                            (if (eq s "int")
                                (compile-int builder e)
                                (if (eq s "var")
                                    (compile-var env e)
                                    (if (eq s "assoc")
                                        (compile-assoc builder env fn-env e)
                                        (if (eq s "apply")
                                            (compile-apply builder env fn-env e)
                                            (if (eq s "labels")
                                                (compile-labels builder env fn-env e)
                                                (panic (append "compile-value: unsupported tag " s))))))))
                           (_ (panic "compile-value: malformed node, expected a tagged list"))))
                       (compile-int ((builder llvm-builder) (e Sexpr)) llvm-value
                         (match (car (cdr e))
                           ((Int n) (const-i64 builder n))
                           (_ (panic "compile-int: malformed int node"))))
                       (compile-var ((env HashTable<string,llvm-value>) (e Sexpr)) llvm-value
                         (let ((var-name (sexpr-str (car (cdr e)))))
                           (match (get env var-name)
                             ((Some v) v)
                             (None (panic (append "compile-var: unbound variable " var-name))))))
                       (compile-assoc ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (e Sexpr)) llvm-value
                         (let ((method (sexpr-str (car (cdr (cdr e))))))
                           (let ((rest (cdr (cdr (cdr (cdr e))))))
                             (let ((a (compile-value builder env fn-env (car rest))))
                               (let ((b2 (compile-value builder env fn-env (car (cdr rest)))))
                                 (if (eq method "+")
                                     (build-add builder a b2)
                                     (if (eq method "-")
                                         (build-sub builder a b2)
                                         (if (eq method "*")
                                             (build-mul builder a b2)
                                             (panic (append "compile-assoc: unsupported method " method))))))))))
                       ;; Fills a previously-`alloca-args`'d array, one
                       ;; compiled argument per slot — the loop shape
                       ;; `bind-params` already uses, just writing instead
                       ;; of reading, and walking a Sexpr list of *unevaluated
                       ;; forms* (each one fed back through `compile-value`)
                       ;; rather than a Sexpr list of parameter names.
                       (compile-call-args ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (args-ptr llvm-value) (forms Sexpr) (idx i32)) ()
                         (match forms
                           ((Cons form rest)
                            (let ((v (compile-value builder env fn-env form)))
                              (store-arg builder args-ptr idx v)
                              (compile-call-args builder env fn-env args-ptr rest (+ idx 1))))
                           (_ ())))
                       ;; `(apply name arg...)` — a direct call to a name
                       ;; `ast_bridge::translate_apply` already proved (at
                       ;; bridge-translation time) resolves to a currently
                       ;; in-scope `labels` sibling or self; this only has
                       ;; to look it up in `fn-env`, never re-derive that
                       ;; judgment.
                       (compile-apply ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (car (cdr e)))))
                           (let ((arg-forms (cdr (cdr e))))
                             (let ((argc (sexpr-list-length arg-forms)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (compile-call-args builder env fn-env args-ptr arg-forms 0)
                                 (match (get fn-env nm)
                                   ((Some target) (build-call builder target args-ptr argc))
                                   (None (panic (append "compile-apply: no direct-callable function named " nm)))))))))
                       ;; Declares every `labels` def's `llvm-function`
                       ;; *before* compiling any of their bodies — the
                       ;; compiled-world counterpart of `Expr::Labels`'s own
                       ;; evaluation strategy (give every function a
                       ;; placeholder slot first, fill bodies in after — see
                       ;; that variant's doc comment), which is what lets
                       ;; the bodies call each other (and themselves)
                       ;; regardless of textual order. The LLVM-level name
                       ;; is mangled with the enclosing function's own name
                       ;; (`name`, closed over from `compile-function`) so
                       ;; two different top-level `defun`s sharing the same
                       ;; module (the AOT case) can each have a `labels`
                       ;; function with the same local name without
                       ;; colliding; `inner-fn-env` is keyed by the
                       ;; *unmangled* local name, since that's the only name
                       ;; `compile-apply`'s lookups ever see.
                       (declare-labels-siblings ((inner-fn-env HashTable<string,llvm-function>) (defs Sexpr)) ()
                         (match defs
                           ((Cons def rest)
                            (let ((nm (sexpr-str (car def))))
                              (set inner-fn-env nm (add-function m (append name (append "$" nm))))
                              (declare-labels-siblings inner-fn-env rest)))
                           (_ ())))
                       ;; Second pass: now that every sibling is declared
                       ;; (and in `inner-fn-env`), give each its own block
                       ;; and builder, bind its own parameters into a fresh
                       ;; `env` (Stage 1 scope: no outer-scope capture, so
                       ;; nothing besides `inner-fn-env` carries over from
                       ;; the enclosing function — see `ast_bridge`'s
                       ;; `translate_labels` doc comment), and compile its
                       ;; single body expression.
                       (compile-labels-bodies ((inner-fn-env HashTable<string,llvm-function>) (defs Sexpr)) ()
                         (match defs
                           ((Cons def rest)
                            (let ((nm (sexpr-str (car def))))
                              (let ((param-syms (car (cdr def))))
                                (let ((def-body (car (cdr (cdr def)))))
                                  (match (get inner-fn-env nm)
                                    ((Some sib-fn)
                                     (let ((sib-block (append-block sib-fn "entry")))
                                       (let ((sib-builder (llvm-builder::create)))
                                         (position-at-end sib-builder sib-block)
                                         (let ((sib-env (new-env)))
                                           (bind-params sib-env sib-builder sib-fn param-syms 0)
                                           (let ((v (compile-value sib-builder sib-env inner-fn-env def-body)))
                                             (build-ret sib-builder v)
                                             (compile-labels-bodies inner-fn-env rest))))))
                                    (None (panic (append "compile-labels-bodies: missing declaration for " nm))))))))
                           (_ ())))
                       ;; `(labels ((name (params) body))... trailing-body)`:
                       ;; declare every sibling, compile every body, then
                       ;; compile the trailing body *with the enclosing
                       ;; function's own `builder`/`env`* (it's ordinary
                       ;; code in the function currently being compiled,
                       ;; not a new function of its own) extended with
                       ;; `inner-fn-env` so it can call the siblings too.
                       (compile-labels ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (e Sexpr)) llvm-value
                         (let ((defs (car (cdr e))))
                           (let ((trailing (car (cdr (cdr e)))))
                             (let ((inner-fn-env (new-fn-env)))
                               (declare-labels-siblings inner-fn-env defs)
                               (compile-labels-bodies inner-fn-env defs)
                               (compile-value builder env inner-fn-env trailing))))))
                (let ((v (compile-value builder env fn-env body)))
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
