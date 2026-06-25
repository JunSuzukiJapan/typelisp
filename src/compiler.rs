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
//! `+`/`-`/`*` only), `labels`-sibling/self direct calls
//! (`(apply name arg...)`/`(labels (captured...) ((name (params)
//! body))... trailing-body)`, the labels-compilation work — see
//! `compile-labels`'s doc comment below for scope), top-level `defun`-to-
//! `defun` calls including self-recursion (`(call name arg...)`,
//! `Expr::Call`, labels/closures Stage 3 — see `compile-call`'s doc comment),
//! and general closures (labels/closures Stage 4 — escaping `lambda`
//! values/`Expr::FnRef` forwarding wrappers compiled to a heap-allocated
//! `ClosureBox`, `(lambda name (captured...) (params...) body)`, see
//! `compile-lambda`'s doc comment; indirect dispatch through one,
//! `(apply-indirect callee-form arg...)`, see `compile-apply-indirect`'s
//! doc comment). `compile-value` grows a new tag-/method-matching arm as
//! later phases teach `ast_bridge` to translate more `Expr` variants for
//! real. No `if`/`let`/`loop` yet.
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
//! Unlike Phase 0/1, `builder`/`env`/`fn-env`/`captured` are now **explicit**
//! parameters threaded through every one of these functions, not values
//! they close over: `compile-labels` (below) needs to compile each
//! `labels`-sibling's body with *its own* fresh `builder`/`env` (a
//! different LLVM function, a different block, different parameter
//! bindings) while still reusing the very same `compile-value` dispatcher
//! — something closing over one fixed `builder`/`env` at the `labels`
//! block's own definition site (Phase 1's design) can't do. `fn-env:
//! HashTable<string,llvm-function>` is the table parallel to `env` that
//! makes a name resolve to a *callable function* rather than a value (see
//! `compile-apply`'s doc comment); `captured: Sexpr` (labels/closures
//! Stage 2) is the *current* direct-call scope's shared captured-name list
//! (empty outside any capturing `labels` block), threaded the same way so
//! every direct call site knows whether — and how — to build an env array
//! for its callee. **Known limitation**: a `labels` block nested inside
//! another doesn't share `fn-env` with its enclosing one (`compile-labels`
//! always starts `inner-fn-env` from `new-fn-env`, not from the `fn-env` it
//! itself received), so an inner sibling calling an *outer* one would panic
//! at compile time ("no direct-callable function named ...") despite
//! `ast_bridge` correctly treating that name as direct-callable — out of
//! scope for Stage 1/2 (both are single-level `labels` only) and not
//! exercised by any test here, but a real gap for whenever nested `labels`
//! compilation is attempted.
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

;; The captures counterpart of `bind-params` (labels/closures Stage 2):
;; reads each captured name back out of a function's env array (`load-env`,
;; the env-array analogue of `load-arg`) into `env` under its own name, so
;; `compile-var`'s ordinary by-name lookup finds it exactly like a regular
;; parameter. A no-op when `names` is empty — the Stage 1 (no-capture) path
;; this leaves untouched.
(defun bind-captures ((env HashTable<string,llvm-value>) (builder llvm-builder) (f llvm-function) (names Sexpr) (idx i32)) ()
  (match names
    ((Cons (Sym name) rest)
     (set env name (load-env builder f idx))
     (bind-captures env builder f rest (+ idx 1)))
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
;; array a direct call needs (`compile-apply`'s `alloca-args`/`build-call`),
;; and (labels/closures Stage 2) the `i64*` env array a captured call needs.
(defun sexpr-list-length ((s Sexpr)) i32
  (match s
    ((Cons _ rest) (+ 1 (sexpr-list-length rest)))
    (_ 0)))

;; Looks up `name` in `env`, panicking if unbound — the lookup
;; `compile-var` does for an ordinary variable reference, factored out so
;; `compile-env-args` (labels/closures Stage 2: building the env array a
;; captured call passes) can reuse the exact same by-name lookup without
;; re-deriving it from a `(var name)`-tagged `Sexpr` node it doesn't have.
(defun lookup-var ((env HashTable<string,llvm-value>) (name string)) llvm-value
  (match (get env name)
    ((Some v) v)
    (None (panic (append "compile-var: unbound variable " name)))))

;; Fills a fresh `alloca-args` array with each captured name's *current*
;; value, looked up in the caller's own `env` — the env-array analogue of
;; `compile-call-args`, just reading already-computed values by name
;; instead of compiling fresh argument expressions. Every direct callee
;; reachable from a given call site shares the same captured-name list (see
;; `compile-labels`'s doc comment), so the caller's own `env` already holds
;; a value for each one: either bound directly (an enclosing `defun`'s own
;; parameter), or itself loaded via `bind-captures` if the caller is a
;; sibling that captures the very same names.
(defun compile-env-args ((builder llvm-builder) (env HashTable<string,llvm-value>) (env-ptr llvm-value) (names Sexpr) (idx i32)) ()
  (match names
    ((Cons (Sym name) rest)
     (store-arg builder env-ptr idx (lookup-var env name))
     (compile-env-args builder env env-ptr rest (+ idx 1)))
    (_ ())))

(defun compile-function ((m llvm-module) (name string) (param-names Sexpr) (body Sexpr)) llvm-module
    (let ((f (add-function m name)))
      (let ((b (append-block f "entry")))
        (let ((builder (llvm-builder::create)))
          (position-at-end builder b)
          (let ((env (new-env)))
            (bind-params env builder f param-names 0)
            (let ((fn-env (new-fn-env)))
              (labels ((compile-value ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (match (car e)
                           ((Sym s)
                            (if (eq s "int")
                                (compile-int builder e)
                                (if (eq s "var")
                                    (compile-var env e)
                                    (if (eq s "assoc")
                                        (compile-assoc builder env fn-env captured e)
                                        (if (eq s "apply")
                                            (compile-apply builder env fn-env captured e)
                                            (if (eq s "labels")
                                                (compile-labels builder env fn-env captured e)
                                                (if (eq s "call")
                                                    (compile-call builder env fn-env captured e)
                                                    (if (eq s "lambda")
                                                        (compile-lambda builder env fn-env captured e)
                                                        (if (eq s "apply-indirect")
                                                            (compile-apply-indirect builder env fn-env captured e)
                                                            (panic (append "compile-value: unsupported tag " s)))))))))))
                           (_ (panic "compile-value: malformed node, expected a tagged list"))))
                       (compile-int ((builder llvm-builder) (e Sexpr)) llvm-value
                         (match (car (cdr e))
                           ((Int n) (const-i64 builder n))
                           (_ (panic "compile-int: malformed int node"))))
                       (compile-var ((env HashTable<string,llvm-value>) (e Sexpr)) llvm-value
                         (lookup-var env (sexpr-str (car (cdr e)))))
                       (compile-assoc ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (let ((method (sexpr-str (car (cdr (cdr e))))))
                           (let ((rest (cdr (cdr (cdr (cdr e))))))
                             (let ((a (compile-value builder env fn-env captured (car rest))))
                               (let ((b2 (compile-value builder env fn-env captured (car (cdr rest)))))
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
                       (compile-call-args ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (args-ptr llvm-value) (forms Sexpr) (idx i32)) ()
                         (match forms
                           ((Cons form rest)
                            (let ((v (compile-value builder env fn-env captured form)))
                              (store-arg builder args-ptr idx v)
                              (compile-call-args builder env fn-env captured args-ptr rest (+ idx 1))))
                           (_ ())))
                       ;; `(apply name arg...)` — a direct call to a name
                       ;; `ast_bridge::translate_apply` already proved (at
                       ;; bridge-translation time) resolves to a currently
                       ;; in-scope `labels` sibling or self; this only has
                       ;; to look it up in `fn-env`, never re-derive that
                       ;; judgment. labels/closures Stage 2: every direct
                       ;; callee reachable from here shares this same
                       ;; `captured` list (see `compile-labels`'s doc
                       ;; comment), so a non-empty `captured` means *every*
                       ;; direct call here needs an env array built and
                       ;; passed via `build-call-with-env`, regardless of
                       ;; which sibling `nm` actually names.
                       (compile-apply ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (car (cdr e)))))
                           (let ((arg-forms (cdr (cdr e))))
                             (let ((argc (sexpr-list-length arg-forms)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (compile-call-args builder env fn-env captured args-ptr arg-forms 0)
                                 (match (get fn-env nm)
                                   ((Some target)
                                    (let ((env-len (sexpr-list-length captured)))
                                      (if (eq env-len 0)
                                          (build-call builder target args-ptr argc)
                                          (let ((env-ptr (alloca-args builder env-len)))
                                            (compile-env-args builder env env-ptr captured 0)
                                            (build-call-with-env builder target args-ptr argc env-ptr env-len)))))
                                   (None (panic (append "compile-apply: no direct-callable function named " nm)))))))))
                       ;; `(call name arg...)` — `Expr::Call`, labels/closures
                       ;; Stage 3: a call to a *top-level* `defun` (itself
                       ;; included, for self-recursion), never a `labels`
                       ;; sibling (those go through `compile-apply`/`fn-env`
                       ;; above instead). Looks `nm` up directly in `m` (the
                       ;; destination module `compile-function` itself
                       ;; received — closed over here exactly the way
                       ;; `declare-labels-siblings` below already closes over
                       ;; it) via `get-function`, not `fn-env`, since a
                       ;; top-level `defun` is never registered there. A
                       ;; top-level `defun` can never capture an outer scope
                       ;; (`Checker::check_defun` always starts from an empty
                       ;; `Env` — see `ast_bridge::translate_call`'s doc
                       ;; comment), so unlike `compile-apply` this never needs
                       ;; an env array: always a plain `build-call`.
                       ;;
                       ;; For an *other* already-`compile`d function, `m`
                       ;; either already has `nm`'s real body in it (AOT's one
                       ;; shared, file-ordered module — see `compile::aot`'s
                       ;; doc comment) or a JIT-only forward declaration with
                       ;; no body that `Interp::compile_function` wires to
                       ;; the real address via `add_global_mapping` after
                       ;; this whole function is compiled (see that method's
                       ;; doc comment) — `get-function` doesn't need to know
                       ;; which case it's in, since both already exist in `m`
                       ;; by the time this runs. For a self-recursive call,
                       ;; `nm` is `name` itself, already declared by
                       ;; `compile-function`'s own first step (`add-function`,
                       ;; above) before this body was ever reached.
                       (compile-call ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (car (cdr e)))))
                           (let ((arg-forms (cdr (cdr e))))
                             (let ((argc (sexpr-list-length arg-forms)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (compile-call-args builder env fn-env captured args-ptr arg-forms 0)
                                 (build-call builder (get-function m nm) args-ptr argc))))))
                       ;; `(apply-indirect callee-form arg...)` — `Expr::Apply`,
                       ;; labels/closures Stage 4, the general indirect-
                       ;; dispatch case (`ast_bridge::translate_indirect_apply`'s
                       ;; doc comment explains why this is a *separate* tag
                       ;; from `(apply name arg...)` rather than the plan's
                       ;; originally sketched `(apply (direct|indirect ...)
                       ;; ...)` unification — keeps Stage 1-3's already-
                       ;; shipped shape untouched). `callee-form` is compiled
                       ;; like any other value (it might be a `(var name)`,
                       ;; another `(apply-indirect ...)`, a `(lambda ...)`,
                       ;; ... — whatever produced the function value here) to
                       ;; get a `ClosureBox` `i64`, then called through
                       ;; `build-closure-apply` rather than `build-call`:
                       ;; nothing here can know ahead of time which compiled
                       ;; function it'll actually be.
                       (compile-apply-indirect ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (let ((closure (compile-value builder env fn-env captured (car (cdr e)))))
                           (let ((arg-forms (cdr (cdr e))))
                             (let ((argc (sexpr-list-length arg-forms)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (compile-call-args builder env fn-env captured args-ptr arg-forms 0)
                                 (build-closure-apply builder closure args-ptr argc))))))
                       ;; `(lambda name (captured...) (params...) body)` —
                       ;; `Expr::Lambda` (a standalone escaping value) or a
                       ;; synthesized `Expr::FnRef` forwarding wrapper
                       ;; (`ast_bridge::translate_fnref`) — labels/closures
                       ;; Stage 4. Unlike every other tag `compile-value`
                       ;; dispatches on, this one's result is *itself* a
                       ;; fresh function, not a value computed from existing
                       ;; ones: declares `lname` under the captures ABI
                       ;; (`add-function-with-env` — *every* closure-boxed
                       ;; function uses that ABI regardless of whether
                       ;; `lcaptured` here is empty, so
                       ;; `compile-apply-indirect`'s `build-closure-apply`
                       ;; never has to branch on which kind of function it's
                       ;; calling through), compiles its single-expression
                       ;; body with a *fresh* env/fn-env (a `lambda` never
                       ;; gets direct-call access to whatever `labels` scope
                       ;; encloses it — see `ast_bridge::translate_lambda`'s
                       ;; doc comment for the documented gap that follows),
                       ;; then builds an env array from `lcaptured`'s
                       ;; *current* values in the *outer* `env`
                       ;; (`compile-env-args`, exactly as a capturing direct
                       ;; call already does) and wraps the whole thing into a
                       ;; `ClosureBox` (`build-make-closure`) — the value
                       ;; this whole node evaluates to.
                       (compile-lambda ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (let ((lname (sexpr-str (car (cdr e)))))
                           (let ((lcaptured (car (cdr (cdr e)))))
                             (let ((lparams (car (cdr (cdr (cdr e))))))
                               (let ((lbody (car (cdr (cdr (cdr (cdr e)))))))
                                 (let ((nested-fn (add-function-with-env m lname)))
                                   (let ((nested-block (append-block nested-fn "entry")))
                                     (let ((nested-builder (llvm-builder::create)))
                                       (position-at-end nested-builder nested-block)
                                       (let ((nested-env (new-env)))
                                         (bind-params nested-env nested-builder nested-fn lparams 0)
                                         (bind-captures nested-env nested-builder nested-fn lcaptured 0)
                                         (let ((v (compile-value nested-builder nested-env (new-fn-env) lcaptured lbody)))
                                           (build-ret nested-builder v)))))
                                   (let ((env-len (sexpr-list-length lcaptured)))
                                     (let ((env-ptr (alloca-args builder env-len)))
                                       (compile-env-args builder env env-ptr lcaptured 0)
                                       (build-make-closure builder nested-fn env-ptr env-len)))))))))
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
                       ;; `compile-apply`'s lookups ever see. labels/closures
                       ;; Stage 2: every sibling in one block shares the same
                       ;; ABI, so the choice between `add-function` (no
                       ;; captures) and `add-function-with-env` (captures) is
                       ;; made once from `captured`'s length, not per-def.
                       (declare-labels-siblings ((inner-fn-env HashTable<string,llvm-function>) (captured Sexpr) (defs Sexpr)) ()
                         (match defs
                           ((Cons def rest)
                            (let ((nm (sexpr-str (car def))))
                              (let ((mangled (append name (append "$" nm))))
                                (if (eq (sexpr-list-length captured) 0)
                                    (set inner-fn-env nm (add-function m mangled))
                                    (set inner-fn-env nm (add-function-with-env m mangled)))
                                (declare-labels-siblings inner-fn-env captured rest))))
                           (_ ())))
                       ;; Second pass: now that every sibling is declared
                       ;; (and in `inner-fn-env`), give each its own block
                       ;; and builder, bind its own parameters into a fresh
                       ;; `env`, and — labels/closures Stage 2 — load every
                       ;; one of the block's shared captured names into that
                       ;; same `env` too (`bind-captures`, a no-op when
                       ;; `captured` is empty), before compiling its single
                       ;; body expression with `captured` carried forward so
                       ;; any call it makes to a sibling (or itself) can
                       ;; build that same env array right back.
                       (compile-labels-bodies ((inner-fn-env HashTable<string,llvm-function>) (captured Sexpr) (defs Sexpr)) ()
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
                                           (bind-captures sib-env sib-builder sib-fn captured 0)
                                           (let ((v (compile-value sib-builder sib-env inner-fn-env captured def-body)))
                                             (build-ret sib-builder v)
                                             (compile-labels-bodies inner-fn-env captured rest))))))
                                    (None (panic (append "compile-labels-bodies: missing declaration for " nm))))))))
                           (_ ())))
                       ;; `(labels (captured...) ((name (params) body))...
                       ;; trailing-body)`: declare every sibling, compile
                       ;; every body, then compile the trailing body *with
                       ;; the enclosing function's own `builder`/`env`* (it's
                       ;; ordinary code in the function currently being
                       ;; compiled, not a new function of its own) extended
                       ;; with `inner-fn-env` and this block's own
                       ;; `inner-captured` so it can call the siblings
                       ;; (including any that capture) too. The `captured`
                       ;; parameter `compile-labels` itself receives — the
                       ;; *enclosing* scope's captured-name list — goes
                       ;; unused here: this block's own `inner-captured`,
                       ;; read straight out of `e`, is what every call made
                       ;; from within it (siblings or trailing body alike)
                       ;; needs, not whatever scope `labels` itself was
                       ;; compiled in. See this module's doc comment for the
                       ;; nested-`labels` limitation that follows from that.
                       (compile-labels ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (let ((inner-captured (car (cdr e))))
                           (let ((defs (car (cdr (cdr e)))))
                             (let ((trailing (car (cdr (cdr (cdr e))))))
                               (let ((inner-fn-env (new-fn-env)))
                                 (declare-labels-siblings inner-fn-env inner-captured defs)
                                 (compile-labels-bodies inner-fn-env inner-captured defs)
                                 (compile-value builder env inner-fn-env inner-captured trailing)))))))
                (let ((v (compile-value builder env fn-env '() body)))
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
