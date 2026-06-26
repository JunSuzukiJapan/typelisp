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
//! arithmetic (`(var name is-fn)`/`(assoc type method instance arg...)`, for
//! `+`/`-`/`*` only), `labels`-sibling/self direct calls
//! (`(apply name (is-fn . arg-form)...)`/`(labels ((captured . is-fn)...)
//! ((name ((param . is-fn)...) body))... trailing-body)`, the labels-
//! compilation work — see `compile-labels`'s doc comment below for scope),
//! top-level `defun`-to-`defun` calls including self-recursion (`(call name
//! (is-fn . arg-form)...)`, `Expr::Call`, labels/closures Stage 3 — see
//! `compile-call`'s doc comment), and general closures (labels/closures
//! Stage 4 — escaping `lambda` values/`Expr::FnRef` forwarding wrappers
//! compiled to a heap-allocated `ClosureBox`, `(lambda name
//! ((captured . is-fn)...) ((param . is-fn)...) body)`, see
//! `compile-lambda`'s doc comment; indirect dispatch through one,
//! `(apply-indirect callee-form (is-fn . arg-form)...)`, see
//! `compile-apply-indirect`'s doc comment), `bool` literals (`(bool b)`),
//! `i64` comparisons (`<`/`<=`/`>`/`>=`/`=`/`eq`/`/=`, alongside the existing
//! `+`/`-`/`*`), `if` (`(if is-fn cond-form then-form else-form)`, see
//! `compile-if`'s doc comment), and `let` (`(let ((name-sym . value-form)...)
//! single-body-form)`, see `compile-let`'s doc comment) — if/let/comparisons,
//! labels/closures Stage 5. `compile-value` grows a new tag-/method-matching
//! arm as later phases teach `ast_bridge` to translate more `Expr` variants
//! for real. No `loop` yet.
//!
//! Every captured/parameter name and call argument carries an `is-fn` `Bool`
//! tag alongside it (`ast_bridge`'s `tagged_sym_list`/`tagged_ast_list_to_sexpr`)
//! — whether that name/argument's static type is `Fn`. This is what makes
//! the automatic `ClosureBox` retain/release insertion work (a follow-up to
//! labels/closures Stage 4) possible at all: without it, nothing here could
//! tell an ordinary `i64` apart from a closure pointer at the points that
//! matter (`build-closure-retain`/`build-closure-release` would corrupt
//! memory if called on the former). See `retain-bindings`/`release-bindings`/
//! `compile-env-args`/`compile-escaping-env-args`'s doc comments for the
//! actual insertion points and the ownership rules behind them (R1-R4 in the
//! design notes this followed — `docs/TODO.md` has the full writeup); the
//! short version: every function activation retains its own `Fn`-typed
//! params/captures on entry and releases them (unconditionally, minus
//! whichever one is being returned bare) on exit, a *borrowed* value being
//! folded into an escaping `ClosureBox`'s own captured array gets an extra
//! retain right there (the box can outlive the activation that built it),
//! and a *fresh* value (a `labels` sibling boxed on demand by
//! `resolve-value`, or any other tag's result) consumed by a call/env-array
//! that doesn't itself escape gets released right after that call is done
//! with it.
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
//! restriction entirely. `resolve-value`/`compile-env-args`/
//! `compile-escaping-env-args` live in this same ring for the same reason —
//! they mutually recurse with each other (resolving a captured name's value
//! may need to box a sibling on demand, which needs to resolve *that* box's
//! own captured names' values, ...) and with the rest of the ring (`resolve-value`
//! is `compile-var`'s callee). Helpers that only ever call *out* of the ring
//! (`retain-bindings`/`release-bindings`/`release-pending-args`/
//! `bare-returned-own-name`/`name-is-borrowed?`/`form-is-borrowed?`/
//! `compute-fn-mask`/...) stay ordinary top-level `defun`s, defined before
//! `compile-function`, exactly like `sexpr-str`/`sexpr-list-length` always
//! have.
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
//! `cur-fn: llvm-function` (if/let/comparisons, labels/closures Stage 5) is
//! the same kind of explicit parameter, threaded for the same reason:
//! `compile-if` needs `append-block` to add its `then`/`else`/`merge` blocks
//! to whichever LLVM function's body is actually being compiled right now —
//! a `labels` sibling's own (`sib-fn`), a `lambda`'s own (`nested-fn`), or the
//! outermost one (`f`) — and none of those are otherwise reachable from deep
//! inside the dispatcher without closing over one fixed function the way
//! Phase 1 closed over one fixed `builder`/`env`. Every function that calls
//! `compile-value` (directly or by calling something that does) forwards its
//! own incoming `cur-fn` *except* `compile-lambda`/`compile-labels-bodies`,
//! which pass their own freshly-declared `nested-fn`/`sib-fn` instead — that
//! new function, not whatever was being compiled when the `lambda`/`labels`
//! form was reached, is what its own body's `if`s must add blocks to.
//!
//! `if`'s Fn-typed branches need one more piece of care that's specific to
//! `if` alone: the existing retain/release convention (R1-R4 below) holds
//! because every *other* boundary a value can cross — a `call`/`apply`/
//! `apply-indirect`'s return, a `labels`/`lambda` body's own result — is a
//! function activation that already retains what it owns on entry (R1) and
//! releases it on exit (R2, `bare-returned-own-name`'s exception folding R1's
//! retain into the returned value's own ownership), so whatever crosses that
//! boundary always arrives already-fresh. `if` introduces no such boundary:
//! without help, a *borrowed* branch value (e.g. a bare parameter) could flow
//! straight out as the `if`'s own result, and any consumer — which always
//! treats a non-`(var ...)`-shaped form as fresh, per `form-is-borrowed?` —
//! would then release something it was never entitled to. `compile-if`
//! closes this the same way `compile-escaping-env-args` already closes the
//! analogous gap for a `ClosureBox`'s own captured array: retain a borrowed
//! branch value right before it's stored into the shared merge slot, so by
//! the time the `if` returns, the value crossing out is unconditionally as
//! fresh as a call result — see `compile-if-branch`'s doc comment.
//!
//! Doesn't depend on `prelude.rs` (no `cond`/`when`/...) — only the
//! checker's native special forms (`if`/`let`/`match`/`labels`) and
//! builtins (`eq`/`append`/`car`/`cdr`/`HashTable`'s methods, `Option`'s
//! `some`/`none`/pattern-matching), so loading order relative to the
//! prelude doesn't matter.

use crate::{Checker, Heap, Interp, Reader};

pub const SOURCE: &str = r#"
(defun sexpr-str ((s Sexpr)) string
  (match s
    ((Str v) v)
    (_ (panic "expected a Str Sexpr node"))))

;; Extracts a host `bool` out of a `(Bool b)` Sexpr node — the `is-fn` tag
;; counterpart of `sexpr-str`, used everywhere a captured/parameter-name
;; pair or call-argument pair's tag needs reading (automatic `ClosureBox`
;; retain/release insertion).
(defun sexpr-bool ((s Sexpr)) bool
  (match s
    ((Bool b) b)
    (_ (panic "expected a Bool Sexpr node"))))

;; Extracts a host `string` out of a `(Sym v)` Sexpr node — the symbol
;; counterpart of `sexpr-str` (a *param/captured-name* pair's name half is a
;; `Sym`, via `ast_bridge::tagged_sym_list`/`intern_symbol`, unlike a `var`
;; tag's or a `labels` def's name, both plain `Str`s).
(defun sexpr-sym-name ((s Sexpr)) string
  (match s
    ((Sym v) v)
    (_ (panic "expected a Sym Sexpr node"))))

;; `names` is now a list of `(name . is-fn)` pairs (`ast_bridge::tagged_sym_list`),
;; not bare symbols — `is-fn` itself isn't needed here (binding a value
;; doesn't yet decide anything about its ownership; `retain-bindings`, called
;; right after, is what reads it).
(defun bind-params ((env HashTable<string,llvm-value>) (builder llvm-builder) (f llvm-function) (names Sexpr) (idx i32)) ()
  (match names
    ((Cons name-pair rest)
     (let ((nm (sexpr-sym-name (car name-pair))))
       (set env nm (load-arg builder f idx))
       (bind-params env builder f rest (+ idx 1))))
    (_ ())))

;; The captures counterpart of `bind-params` (labels/closures Stage 2):
;; reads each captured name back out of a function's env array (`load-env`,
;; the env-array analogue of `load-arg`) into `env` under its own name, so
;; `compile-var`'s ordinary by-name lookup finds it exactly like a regular
;; parameter. A no-op when `names` is empty — the Stage 1 (no-capture) path
;; this leaves untouched.
(defun bind-captures ((env HashTable<string,llvm-value>) (builder llvm-builder) (f llvm-function) (names Sexpr) (idx i32)) ()
  (match names
    ((Cons name-pair rest)
     (let ((nm (sexpr-sym-name (car name-pair))))
       (set env nm (load-env builder f idx))
       (bind-captures env builder f rest (+ idx 1))))
    (_ ())))

;; 2^n by repeated doubling — `compute-fn-mask`'s one helper, not a generic
;; utility; `n` (a captured-slot index) is always small in practice so the
;; O(n) recursion is in no way a bottleneck.
(defun pow2 ((n i32)) i64
  (if (eq n 0) 1 (* (pow2 (- n 1)) 2)))

;; Computes `build-make-closure`'s `fn_mask` argument: a bitmask, one bit per
;; captured slot, set wherever that slot's `is-fn` tag is true — entirely a
;; *host*-level computation (no LLVM IR involved, unlike everything `compile-*`
;; builds): the mask is fully determined by `names`' own tags, known already
;; at the point this runs, so there's nothing to generate code for. See
;; `interp::get_or_define_closure_release_fn`'s doc comment for what this
;; mask is used for (the cascading release that walks it back out at
;; runtime).
(defun compute-fn-mask ((names Sexpr) (idx i32)) i64
  (match names
    ((Cons name-pair rest)
     (let ((is-fn (sexpr-bool (cdr name-pair))))
       (if is-fn
           (+ (pow2 idx) (compute-fn-mask rest (+ idx 1)))
           (compute-fn-mask rest (+ idx 1)))))
    (_ 0)))

;; Whether `nm` appears as the name half of any `(name . is-fn)` pair in
;; `names` — `bare-returned-own-name`'s one helper.
(defun sexpr-name-list-contains? ((names Sexpr) (nm string)) bool
  (match names
    ((Cons name-pair rest)
     (if (eq (sexpr-sym-name (car name-pair)) nm)
         true
         (sexpr-name-list-contains? rest nm)))
    (_ false)))

;; If `body` is literally `(var name is-fn)` and `name` is one of *this*
;; function activation's own bound names (`params`/`captured`, its own
;; `bind-params`/`bind-captures` lists) — the one case where the value about
;; to be returned is also one of the names the matching `release-bindings`
;; call is about to bulk-release — returns `Some name` so that call can
;; exclude it (R2 of the design notes this followed: excluding it from the
;; release is *all* that's needed, since this activation's own `retain-bindings`
;; call already gave it the extra +1 a returned value needs to leave with;
;; no separate "protect" retain). Any other body shape (`apply`/`call`/
;; `labels`/`lambda`/`assoc`/...) computes an inherently fresh result with
;; no alias among this activation's own bound names, so `None` — the bulk
;; release was never going to touch it anyway.
(defun bare-returned-own-name ((body Sexpr) (params Sexpr) (captured Sexpr)) Option<string>
  (match (car body)
    ((Sym s)
     (if (eq s "var")
         (let ((nm (sexpr-str (car (cdr body)))))
           (if (sexpr-name-list-contains? params nm)
               (Option::some nm)
               (if (sexpr-name-list-contains? captured nm)
                   (Option::some nm)
                   (Option::none))))
         (Option::none)))
    (_ (Option::none))))

;; Whether `nm` currently resolves through `env` (an ordinary bound
;; parameter/capture, *borrowed* from that binding's own, longer-lived
;; ownership) rather than `fn-env`'s fallback (a `labels` sibling boxed
;; *fresh*, on demand, by `resolve-value` — see that function's doc
;; comment). The borrowed/fresh distinction `compile-env-args`/
;; `compile-escaping-env-args`/`compile-call-args` all key their retain/
;; release decisions on.
(defun name-is-borrowed? ((env HashTable<string,llvm-value>) (nm string)) bool
  (match (get env nm)
    ((Some _) true)
    (None false)))

;; The general-expression-form counterpart of `name-is-borrowed?`: a `var`
;; node is borrowed exactly when its name is (`compile-call-args`'s own
;; argument forms can be arbitrary expressions, not just bare names — every
;; other tag's result is fresh by construction, see this module's doc
;; comment).
(defun form-is-borrowed? ((env HashTable<string,llvm-value>) (form Sexpr)) bool
  (match (car form)
    ((Sym s)
     (if (eq s "var")
         (name-is-borrowed? env (sexpr-str (car (cdr form))))
         false))
    (_ false)))

(defun protects? ((protected Option<string>) (nm string)) bool
  (match protected
    ((Some pname) (eq pname nm))
    (None false)))

;; R1 (function entry, design notes): retains every `Fn`-typed name in
;; `names` (a tagged params or captured list, as just bound by `bind-params`/
;; `bind-captures`) once, so this activation owns an independent reference
;; for its own lifetime regardless of what the caller does with its own
;; copy afterward. `release-bindings`, at this same activation's exit,
;; undoes it.
(defun retain-bindings ((builder llvm-builder) (env HashTable<string,llvm-value>) (names Sexpr)) ()
  (match names
    ((Cons name-pair rest)
     (let ((nm (sexpr-sym-name (car name-pair))))
       (let ((is-fn (sexpr-bool (cdr name-pair))))
         (if is-fn
             (match (get env nm)
               ((Some v) (let ((ignored (build-closure-retain builder v))) ()))
               (None ()))
             ())
         (retain-bindings builder env rest))))
    (_ ())))

;; R2 (function exit, design notes): unconditionally releases every
;; `Fn`-typed name in `names`, except `protected` (see `bare-returned-own-name`)
;; — the `retain-bindings` call this activation made on entry, undone now
;; that its lifetime is ending, with the one exception being whichever name
;; (if any) is leaving as the bare return value instead of being dropped.
(defun release-bindings ((builder llvm-builder) (m llvm-module) (env HashTable<string,llvm-value>) (names Sexpr) (protected Option<string>)) ()
  (match names
    ((Cons name-pair rest)
     (let ((nm (sexpr-sym-name (car name-pair))))
       (let ((is-fn (sexpr-bool (cdr name-pair))))
         (if is-fn
             (if (protects? protected nm)
                 ()
                 (match (get env nm)
                   ((Some v) (build-closure-release builder m v))
                   (None ())))
             ())
         (release-bindings builder m env rest protected))))
    (_ ())))

;; Releases every slot of a `pending-ptr` array `compile-call-args`/
;; `compile-env-args` built (`0..argc`) — called once, right after the call/
;; env-array those slots belong to is done using them, never before (the
;; value has to stay valid for that call's full duration). Every slot gets
;; an unconditional `build-closure-release` call, including the ones marked
;; `0` for "nothing to release here" — `build-closure-release`'s underlying
;; `__typelisp_closure_release` tolerates a null closure as a no-op (see
;; that function's doc comment), which is what lets this walk be
;; unconditional: `compiler.rs` has no general `if` to skip a slot with
;; otherwise.
(defun release-pending-args ((builder llvm-builder) (m llvm-module) (pending-ptr llvm-value) (argc i32) (idx i32)) ()
  (if (eq idx argc)
      ()
      (let ((v (load-raw builder pending-ptr idx)))
        (build-closure-release builder m v)
        (release-pending-args builder m pending-ptr argc (+ idx 1)))))

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

;; The `let` analogue of `retain-bindings`'s save-then-overwrite step
;; (if/let/comparisons, labels/closures Stage 5): for each `(name . _)`
;; binding pair, if `env` already has a value under that name, stash it in
;; `saved` before overwriting it with `acc`'s already-computed value for that
;; same name (`compile-let-values` built `acc` first, against `env`
;; *unmodified* — see that function's doc comment for why this can't be one
;; combined pass). No `is-fn` bookkeeping happens here at all: a `let`-bound
;; name resolves through `env` exactly like a parameter/capture does, so the
;; existing `name-is-borrowed?`/`form-is-borrowed?` (which only ask "is this
;; name present in `env`") already treat it correctly with no further
;; tagging — see `compile-let`'s doc comment.
(defun bind-let-values ((env HashTable<string,llvm-value>) (bindings Sexpr) (acc HashTable<string,llvm-value>) (saved HashTable<string,llvm-value>)) ()
  (match bindings
    ((Cons pair rest)
     (let ((nm (sexpr-sym-name (car pair))))
       (match (get env nm)
         ((Some old) (set saved nm old))
         (None ()))
       (match (get acc nm)
         ((Some v) (set env nm v))
         (None (panic "bind-let-values: missing computed value")))
       (bind-let-values env rest acc saved)))
    (_ ())))

;; Undoes `bind-let-values` once the `let`'s body has been compiled: restores
;; whatever `env` held under each binding's name before this `let` shadowed
;; it (or removes the binding entirely if there was nothing there before).
;; Skipping this would leak a stale shadowed value into any later expression
;; that happens to reuse the same name after this `let` ends — e.g. `(+ (let
;; ((a 99)) a) a)`'s second `a` would otherwise wrongly read back 99.
;;
;; Known limitation: if `bindings` names the *same* symbol more than once
;; (`(let ((a 1) (a 2)) ...)`), `saved` can only remember one prior value per
;; name, so restoration after such a `let` may leave `env` slightly wrong —
;; though never in a way that double-frees or corrupts anything, only in
;; what value a *later, unrelated* expression sees under that name. Checked
;; typelisp source rarely if ever writes a `let` with a repeated binding
;; name; not worth a multi-value-per-name structure for this alone.
(defun restore-let-values ((env HashTable<string,llvm-value>) (bindings Sexpr) (saved HashTable<string,llvm-value>)) ()
  (match bindings
    ((Cons pair rest)
     (let ((nm (sexpr-sym-name (car pair))))
       (match (get saved nm)
         ((Some old) (set env nm old))
         (None (let ((ignored (remove env nm))) ())))
       (restore-let-values env rest saved)))
    (_ ())))

(defun compile-function ((m llvm-module) (name string) (param-names Sexpr) (body Sexpr)) llvm-module
    (let ((f (add-function m name)))
      (let ((b (append-block f "entry")))
        (let ((builder (llvm-builder::create)))
          (position-at-end builder b)
          (let ((env (new-env)))
            (bind-params env builder f param-names 0)
            (retain-bindings builder env param-names)
            (let ((fn-env (new-fn-env)))
              (labels ((compile-value ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (match (car e)
                           ((Sym s)
                            (if (eq s "int")
                                (compile-int builder e)
                                (if (eq s "bool")
                                    (compile-bool builder e)
                                    (if (eq s "var")
                                        (compile-var builder env fn-env captured e)
                                        (if (eq s "assoc")
                                            (compile-assoc builder env fn-env captured cur-fn e)
                                            (if (eq s "apply")
                                                (compile-apply builder env fn-env captured cur-fn e)
                                                (if (eq s "labels")
                                                    (compile-labels builder env fn-env captured cur-fn e)
                                                    (if (eq s "call")
                                                        (compile-call builder env fn-env captured cur-fn e)
                                                        (if (eq s "lambda")
                                                            (compile-lambda builder env fn-env captured e)
                                                            (if (eq s "apply-indirect")
                                                                (compile-apply-indirect builder env fn-env captured cur-fn e)
                                                                (if (eq s "if")
                                                                    (compile-if builder env fn-env captured cur-fn e)
                                                                    (if (eq s "let")
                                                                        (compile-let builder env fn-env captured cur-fn e)
                                                                        (panic (append "compile-value: unsupported tag " s))))))))))))))
                           (_ (panic "compile-value: malformed node, expected a tagged list"))))
                       (compile-int ((builder llvm-builder) (e Sexpr)) llvm-value
                         (match (car (cdr e))
                           ((Int n) (const-i64 builder n))
                           (_ (panic "compile-int: malformed int node"))))
                       ;; `(bool b)` (if/let/comparisons, labels/closures
                       ;; Stage 5) — every compiled value is a plain `i64`
                       ;; (see `registry::llvm_module_def`'s doc comment), so
                       ;; a `bool` literal is just `0`/`1`, the same
                       ;; representation `build-icmp-*` already produces and
                       ;; `compile-if`'s `build-cond-br` already expects.
                       (compile-bool ((builder llvm-builder) (e Sexpr)) llvm-value
                         (match (car (cdr e))
                           ((Bool b) (if b (const-i64 builder 1) (const-i64 builder 0)))
                           (_ (panic "compile-bool: malformed bool node"))))
                       ;; `(var name is-fn)` — a plain variable reference,
                       ;; ordinary or a `labels` sibling/self referenced *as
                       ;; a value* rather than called (e.g. a `labels`
                       ;; block's own trailing body bare-returning one of
                       ;; its siblings — `ast_bridge::ast_to_sexpr_scoped`'s
                       ;; `Expr::Var` arm never distinguishes the two: it
                       ;; always emits `(var name is-fn)` regardless of
                       ;; whether `name` happens to be a sibling,
                       ;; deliberately deferring that judgment to here).
                       ;; `is-fn` isn't read here — `compile-var` only needs
                       ;; the *value*; it's `compile-call-args`/
                       ;; `compile-env-args`/`compile-escaping-env-args`,
                       ;; reading this same tag straight off the Sexpr
                       ;; *before* calling this, that decide what (if
                       ;; anything) needs retaining/releasing around it. See
                       ;; `resolve-value`'s doc comment for how the
                       ;; sibling-as-value case resolves.
                       (compile-var ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (resolve-value builder env fn-env captured (sexpr-str (car (cdr e)))))
                       ;; Resolves `name` to a value two ways, in order:
                       ;; (1) an ordinary binding in `env` (a parameter, or a
                       ;; captured name already loaded by `bind-captures`) —
                       ;; *borrowed* from that binding's own ownership,
                       ;; unchanged by this lookup; (2) a currently in-scope
                       ;; `labels` sibling/self, found in `fn-env` instead —
                       ;; boxed into a *fresh* `ClosureBox` on the spot
                       ;; (`build-make-closure`, exactly the way
                       ;; `compile-lambda` already boxes a `lambda` literal),
                       ;; using *this* scope's own shared `captured` list to
                       ;; build the box's env array. That array is built via
                       ;; `compile-escaping-env-args`, not the lighter
                       ;; `compile-env-args` a direct call uses: this fresh
                       ;; box may itself escape (e.g. it's what a `labels`
                       ;; block's trailing body bare-returns), so any
                       ;; *borrowed* value folded into its own captured
                       ;; array needs its own independent retain right here
                       ;; — waiting for some later invocation's own entry
                       ;; retain (R1) would be too late, if it ever happens
                       ;; at all. `compile-escaping-env-args` is mutually
                       ;; recursive with this function (case (2) needs it;
                       ;; it needs `resolve-value` to read each captured
                       ;; name's current value) — that's why both live in
                       ;; this `labels` ring now rather than as separate
                       ;; top-level `defun`s, which can never be mutually
                       ;; recursive with each other (see `compile-call`'s
                       ;; doc comment for that constraint). Anything in
                       ;; neither table is a genuine unbound name, the only
                       ;; case that still panics.
                       ;;
                       ;; This can never recurse unboundedly: `captured`
                       ;; never itself contains a sibling/self name
                       ;; (`compile::freevars`'s `note` always excludes
                       ;; `siblings`, regardless of nesting — see that
                       ;; module's doc comment), so the
                       ;; `compile-escaping-env-args` call inside case (2)
                       ;; only ever resolves *ordinary* values through
                       ;; `env`, never re-enters case (2) itself by way of a
                       ;; sibling name. Nor can two boxes built this way
                       ;; ever reference *each other* (a true cycle): every
                       ;; call here allocates a fresh `ClosureBox` via
                       ;; `build-array_malloc`, and nothing in this compiler
                       ;; ever mutates an already-built box's env slots
                       ;; afterward to patch in a forward reference (the
                       ;; classic Scheme/OCaml `letrec`-closure trick) — so
                       ;; boxing the same sibling twice yields two distinct,
                       ;; independent heap objects, not a shared, patchable
                       ;; one. A `labels` block with two siblings that each
                       ;; capture the *other's* boxed value as data (not
                       ;; just call it) is therefore two ordinary,
                       ;; independently-releasable allocations, never a
                       ;; reference cycle — which is also why
                       ;; `get_or_define_closure_release_fn`'s recursive
                       ;; release cascade is guaranteed to terminate.
                       ;;
                       ;; Known boundary, unchanged from before this
                       ;; follow-up: a *nested* `labels` block can't reach
                       ;; an *enclosing* one's siblings this way either (its
                       ;; own `inner-fn-env` is always built fresh — see
                       ;; this module's doc comment's "known limitation"
                       ;; paragraph).
                       (resolve-value ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (name string)) llvm-value
                         (match (get env name)
                           ((Some v) v)
                           (None (match (get fn-env name)
                                   ((Some target)
                                    (let ((env-len (sexpr-list-length captured)))
                                      (let ((env-ptr (alloca-args builder env-len)))
                                        (compile-escaping-env-args builder env fn-env captured env-ptr captured 0)
                                        (build-make-closure builder target env-ptr env-len (compute-fn-mask captured 0)))))
                                   (None (panic (append "compile-var: unbound variable " name)))))))
                       ;; Fills `env-ptr` for a *direct* (non-escaping)
                       ;; call's env array — `compile-apply`'s sibling-to-
                       ;; sibling calls. A *borrowed* value needs no extra
                       ;; action here at all: the callee's own R1 entry-
                       ;; retain (on this exact array's contents, once
                       ;; `bind-captures` loads them back out on the other
                       ;; side) already gives it an independent reference
                       ;; for the callee's lifetime, leaving this scope's
                       ;; own copy untouched. A *fresh* value (a sibling
                       ;; boxed on demand by `resolve-value`, with no other
                       ;; owner) does need releasing once this call is done
                       ;; with it — `pending-ptr` (same length as `env-ptr`,
                       ;; caller-allocated) records which slots need that
                       ;; post-call release, the same way `compile-call-args`
                       ;; marks its own argument array; see
                       ;; `release-pending-args`. `names`/`idx` are what
                       ;; this call is actually walking to fill `env-ptr`;
                       ;; `captured` is held constant across every recursive
                       ;; step — the *enclosing* scope's own shared
                       ;; captured-name list, passed through unchanged
                       ;; purely so `resolve-value`'s fallback has it on
                       ;; hand if any name along the way turns out to be a
                       ;; sibling rather than an ordinary value. At every
                       ;; existing call site, `names` and `captured` happen
                       ;; to be the *same* list (building a block's own
                       ;; shared env array).
                       (compile-env-args ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (env-ptr llvm-value) (pending-ptr llvm-value) (names Sexpr) (idx i32)) ()
                         (match names
                           ((Cons name-pair rest)
                            (let ((nm (sexpr-sym-name (car name-pair))))
                              (let ((is-fn (sexpr-bool (cdr name-pair))))
                                (let ((v (resolve-value builder env fn-env captured nm)))
                                  (store-arg builder env-ptr idx v)
                                  (if is-fn
                                      (if (name-is-borrowed? env nm)
                                          (store-arg builder pending-ptr idx (const-i64 builder 0))
                                          (store-arg builder pending-ptr idx v))
                                      (store-arg builder pending-ptr idx (const-i64 builder 0)))
                                  (compile-env-args builder env fn-env captured env-ptr pending-ptr rest (+ idx 1))))))
                           (_ ())))
                       ;; Fills `env-ptr` for an *escaping* `ClosureBox`'s
                       ;; own captured-value array — `compile-lambda`'s and
                       ;; `resolve-value`'s fallback's use. A *borrowed*
                       ;; value (one of the *constructing* scope's own bound
                       ;; names) gets an explicit retain right here: this
                       ;; box can outlive the current activation, so it must
                       ;; hold an independent reference from the moment it's
                       ;; built — relying on some later invocation's own R1
                       ;; entry-retain would be too late (the box might
                       ;; never be invoked at all, or be invoked only after
                       ;; the original binding's own R2 exit-release has
                       ;; already dropped it). A *fresh* value needs nothing
                       ;; extra: it's already a singly-owned reference with
                       ;; no other claimant, so it just moves into the box.
                       (compile-escaping-env-args ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (env-ptr llvm-value) (names Sexpr) (idx i32)) ()
                         (match names
                           ((Cons name-pair rest)
                            (let ((nm (sexpr-sym-name (car name-pair))))
                              (let ((is-fn (sexpr-bool (cdr name-pair))))
                                (let ((v (resolve-value builder env fn-env captured nm)))
                                  (if is-fn
                                      (if (name-is-borrowed? env nm)
                                          (store-arg builder env-ptr idx (build-closure-retain builder v))
                                          (store-arg builder env-ptr idx v))
                                      (store-arg builder env-ptr idx v))
                                  (compile-escaping-env-args builder env fn-env captured env-ptr rest (+ idx 1))))))
                           (_ ())))
                       ;; `(assoc type-name method instance arg...)` — only
                       ;; `i64`/`i32` receivers (the same runtime
                       ;; representation, `RtValue::Int(i64)`, so one code
                       ;; path covers both) are compiled; anything else
                       ;; (`f64`, ...) panics clearly rather than silently
                       ;; misinterpreting its bit pattern as an `i64` — this
                       ;; guard didn't exist before if/let/comparisons (Stage
                       ;; 5) added methods (`<`/`<=`/.../`eq`/`/=`) that
                       ;; `f64` *also* defines under the same names
                       ;; (`registry::float_assoc`), so without it
                       ;; `(< 1.0 2.0)` would have quietly compiled as an
                       ;; integer comparison.
                       (compile-assoc ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (let ((type-name (sexpr-str (car (cdr e)))))
                           (if (if (eq type-name "i64") true (eq type-name "i32"))
                               (let ((method (sexpr-str (car (cdr (cdr e))))))
                                 (let ((rest (cdr (cdr (cdr (cdr e))))))
                                   (let ((a (compile-value builder env fn-env captured cur-fn (car rest))))
                                     (let ((b2 (compile-value builder env fn-env captured cur-fn (car (cdr rest)))))
                                       (if (eq method "+")
                                           (build-add builder a b2)
                                           (if (eq method "-")
                                               (build-sub builder a b2)
                                               (if (eq method "*")
                                                   (build-mul builder a b2)
                                                   (if (eq method "<")
                                                       (build-icmp-lt builder a b2)
                                                       (if (eq method "<=")
                                                           (build-icmp-le builder a b2)
                                                           (if (eq method ">")
                                                               (build-icmp-gt builder a b2)
                                                               (if (eq method ">=")
                                                                   (build-icmp-ge builder a b2)
                                                                   (if (if (eq method "=") true (eq method "eq"))
                                                                       (build-icmp-eq builder a b2)
                                                                       (if (eq method "/=")
                                                                           (build-icmp-ne builder a b2)
                                                                           (panic (append "compile-assoc: unsupported method " method)))))))))))))))
                               (panic (append "compile-assoc: unsupported receiver type " type-name)))))
                       ;; Fills a previously-`alloca-args`'d array, one
                       ;; compiled argument per slot, exactly as before —
                       ;; plus, since each `forms` element is now an
                       ;; `(is-fn . arg-form)` pair (`ast_bridge::tagged_ast_list_to_sexpr`),
                       ;; marking `pending-ptr` (same length, caller-
                       ;; allocated) wherever that argument is `Fn`-typed
                       ;; *and* fresh (see `form-is-borrowed?`) — the same
                       ;; post-call release bookkeeping `compile-env-args`
                       ;; does for env arrays, here for ordinary call
                       ;; arguments instead.
                       (compile-call-args ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (args-ptr llvm-value) (pending-ptr llvm-value) (forms Sexpr) (idx i32)) ()
                         (match forms
                           ((Cons arg-pair rest)
                            (let ((is-fn (sexpr-bool (car arg-pair))))
                              (let ((form (cdr arg-pair)))
                                (let ((v (compile-value builder env fn-env captured cur-fn form)))
                                  (store-arg builder args-ptr idx v)
                                  (if is-fn
                                      (if (form-is-borrowed? env form)
                                          (store-arg builder pending-ptr idx (const-i64 builder 0))
                                          (store-arg builder pending-ptr idx v))
                                      (store-arg builder pending-ptr idx (const-i64 builder 0)))
                                  (compile-call-args builder env fn-env captured cur-fn args-ptr pending-ptr rest (+ idx 1))))))
                           (_ ())))
                       ;; `(apply name (is-fn . arg-form)...)` — a direct
                       ;; call to a name `ast_bridge::translate_apply`
                       ;; already proved (at bridge-translation time)
                       ;; resolves to a currently in-scope `labels` sibling
                       ;; or self; this only has to look it up in `fn-env`,
                       ;; never re-derive that judgment. labels/closures
                       ;; Stage 2: every direct callee reachable from here
                       ;; shares this same `captured` list (see
                       ;; `compile-labels`'s doc comment), so a non-empty
                       ;; `captured` means *every* direct call here needs an
                       ;; env array built and passed via
                       ;; `build-call-with-env`, regardless of which sibling
                       ;; `nm` actually names. Every pending-release array
                       ;; built along the way (`pending-ptr` for the call
                       ;; arguments, `env-pending-ptr` for the env array, if
                       ;; any) is released right after the call returns —
                       ;; never before, since the value has to stay valid
                       ;; for the call's full duration.
                       (compile-apply ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (car (cdr e)))))
                           (let ((arg-forms (cdr (cdr e))))
                             (let ((argc (sexpr-list-length arg-forms)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (let ((pending-ptr (alloca-args builder argc)))
                                   (compile-call-args builder env fn-env captured cur-fn args-ptr pending-ptr arg-forms 0)
                                   (match (get fn-env nm)
                                     ((Some target)
                                      (let ((env-len (sexpr-list-length captured)))
                                        (if (eq env-len 0)
                                            (let ((result (build-call builder target args-ptr argc)))
                                              (release-pending-args builder m pending-ptr argc 0)
                                              result)
                                            (let ((env-ptr (alloca-args builder env-len)))
                                              (let ((env-pending-ptr (alloca-args builder env-len)))
                                                (compile-env-args builder env fn-env captured env-ptr env-pending-ptr captured 0)
                                                (let ((result (build-call-with-env builder target args-ptr argc env-ptr env-len)))
                                                  (release-pending-args builder m pending-ptr argc 0)
                                                  (release-pending-args builder m env-pending-ptr env-len 0)
                                                  result))))))
                                     (None (panic (append "compile-apply: no direct-callable function named " nm))))))))))
                       ;; `(call name (is-fn . arg-form)...)` — `Expr::Call`,
                       ;; labels/closures Stage 3: a call to a *top-level*
                       ;; `defun` (itself included, for self-recursion),
                       ;; never a `labels` sibling (those go through
                       ;; `compile-apply`/`fn-env` above instead). Looks
                       ;; `nm` up directly in `m` (the destination module
                       ;; `compile-function` itself received — closed over
                       ;; here exactly the way `declare-labels-siblings`
                       ;; below already closes over it) via `get-function`,
                       ;; not `fn-env`, since a top-level `defun` is never
                       ;; registered there. A top-level `defun` can never
                       ;; capture an outer scope (`Checker::check_defun`
                       ;; always starts from an empty `Env` — see
                       ;; `ast_bridge::translate_call`'s doc comment), so
                       ;; unlike `compile-apply` this never needs an env
                       ;; array: always a plain `build-call`. Its arguments
                       ;; can still be `Fn`-typed though (a closure passed
                       ;; to another compiled top-level function), hence the
                       ;; same `pending-ptr` release as `compile-apply`'s
                       ;; own call arguments.
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
                       (compile-call ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (car (cdr e)))))
                           (let ((arg-forms (cdr (cdr e))))
                             (let ((argc (sexpr-list-length arg-forms)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (let ((pending-ptr (alloca-args builder argc)))
                                   (compile-call-args builder env fn-env captured cur-fn args-ptr pending-ptr arg-forms 0)
                                   (let ((result (build-call builder (get-function m nm) args-ptr argc)))
                                     (release-pending-args builder m pending-ptr argc 0)
                                     result)))))))
                       ;; `(apply-indirect callee-form (is-fn . arg-form)...)`
                       ;; — `Expr::Apply`, labels/closures Stage 4, the
                       ;; general indirect-dispatch case
                       ;; (`ast_bridge::translate_indirect_apply`'s doc
                       ;; comment explains why this is a *separate* tag from
                       ;; `(apply name arg...)` rather than the plan's
                       ;; originally sketched `(apply (direct|indirect ...)
                       ;; ...)` unification — keeps Stage 1-3's already-
                       ;; shipped shape untouched). `callee-form` is compiled
                       ;; like any other value (it might be a `(var name is-fn)`,
                       ;; another `(apply-indirect ...)`, a `(lambda ...)`,
                       ;; ... — whatever produced the function value here) to
                       ;; get a `ClosureBox` `i64`, then called through
                       ;; `build-closure-apply` rather than `build-call`:
                       ;; nothing here can know ahead of time which compiled
                       ;; function it'll actually be. The callee position is
                       ;; always `Fn`-typed by construction (that's what
                       ;; makes it callable at all), so unlike call
                       ;; arguments there's no `is-fn` tag to read for it —
                       ;; only whether it's *borrowed* (`form-is-borrowed?`)
                       ;; matters, to decide whether this transient read
                       ;; needs releasing once the indirect call is done
                       ;; with it (a *fresh* callee, e.g. a bare-referenced
                       ;; `labels` sibling boxed on the spot by
                       ;; `resolve-value`, would otherwise leak one box per
                       ;; call).
                       (compile-apply-indirect ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (let ((callee-form (car (cdr e))))
                           (let ((closure (compile-value builder env fn-env captured cur-fn callee-form)))
                             (let ((arg-forms (cdr (cdr e))))
                               (let ((argc (sexpr-list-length arg-forms)))
                                 (let ((args-ptr (alloca-args builder argc)))
                                   (let ((pending-ptr (alloca-args builder argc)))
                                     (compile-call-args builder env fn-env captured cur-fn args-ptr pending-ptr arg-forms 0)
                                     (let ((result (build-closure-apply builder closure args-ptr argc)))
                                       (release-pending-args builder m pending-ptr argc 0)
                                       (if (form-is-borrowed? env callee-form)
                                           ()
                                           (build-closure-release builder m closure))
                                       result))))))))
                       ;; `compile-if`'s one helper (if/let/comparisons,
                       ;; labels/closures Stage 5): compiles `form` (one of
                       ;; an `if`'s `then`/`else` branches) and, when this
                       ;; `if` is itself `Fn`-typed (`is-fn`) *and* `form` is
                       ;; a *borrowed* value (`form-is-borrowed?` — a bare
                       ;; reference to something this scope doesn't own an
                       ;; independent copy of), retains it right here before
                       ;; handing it back. See `compile-if`'s doc comment and
                       ;; this module's doc comment for the full reasoning —
                       ;; short version: unlike every other tag, `if`
                       ;; introduces no function boundary to freshen a
                       ;; borrowed value at, so it has to do that itself, and
                       ;; this is the one place both branches share that
                       ;; decision (kept out of `compile-if` itself purely to
                       ;; avoid writing the same `if`-on-`is-fn`-and-`form-is-borrowed?`
                       ;; logic out twice).
                       (compile-if-branch ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (is-fn bool) (form Sexpr)) llvm-value
                         (let ((v (compile-value builder env fn-env captured cur-fn form)))
                           (if (if is-fn (form-is-borrowed? env form) false)
                               (build-closure-retain builder v)
                               v)))
                       ;; `(if is-fn cond-form then-form else-form)`
                       ;; (if/let/comparisons, labels/closures Stage 5): adds
                       ;; `then`/`else`/`merge` blocks to `cur-fn`, branches
                       ;; on `cond` via `build-cond-br`, compiles each branch
                       ;; (through `compile-if-branch`, which also handles
                       ;; the borrowed-value retain) into a single shared
                       ;; 1-slot `alloca-args` rather than an LLVM `phi` node
                       ;; — every compiled value is already a plain `i64`
                       ;; (see `registry::llvm_module_def`'s doc comment), so
                       ;; the existing `alloca-args`/`store-arg`/`load-raw`
                       ;; triple (already built for call-argument arrays)
                       ;; covers a 2-way merge with no new builtin needed for
                       ;; the merge itself. `cur-fn` is forwarded unchanged
                       ;; to every recursive `compile-value` call — `if`
                       ;; never starts a new LLVM function the way
                       ;; `compile-lambda`/a `labels` sibling does.
                       (compile-if ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (let ((is-fn (sexpr-bool (car (cdr e)))))
                           (let ((cond-form (car (cdr (cdr e)))))
                             (let ((then-form (car (cdr (cdr (cdr e))))))
                               (let ((else-form (car (cdr (cdr (cdr (cdr e)))))))
                                 (let ((cond-v (compile-value builder env fn-env captured cur-fn cond-form)))
                                   (let ((then-block (append-block cur-fn "if-then")))
                                     (let ((else-block (append-block cur-fn "if-else")))
                                       (let ((merge-block (append-block cur-fn "if-merge")))
                                         (let ((slot (alloca-args builder 1)))
                                           (build-cond-br builder cond-v then-block else-block)
                                           (position-at-end builder then-block)
                                           (let ((then-v (compile-if-branch builder env fn-env captured cur-fn is-fn then-form)))
                                             (store-arg builder slot 0 then-v))
                                           (build-br builder merge-block)
                                           (position-at-end builder else-block)
                                           (let ((else-v (compile-if-branch builder env fn-env captured cur-fn is-fn else-form)))
                                             (store-arg builder slot 0 else-v))
                                           (build-br builder merge-block)
                                           (position-at-end builder merge-block)
                                           (load-raw builder slot 0)))))))))))
                       ;; `compile-let`'s one helper (if/let/comparisons,
                       ;; labels/closures Stage 5): computes every binding's
                       ;; value against `env` *unmodified* into a fresh
                       ;; accumulator `acc`, in order — CL `let` semantics
                       ;; ("Binding values are checked in the *outer*
                       ;; environment" — `Checker::check_let`'s own comment),
                       ;; so an earlier binding shadowing a name a *later*
                       ;; binding's value-form also references must not leak
                       ;; into that later computation. Lives in this `labels`
                       ;; ring (unlike `bind-let-values`/`restore-let-values`)
                       ;; purely because it calls `compile-value`.
                       (compile-let-values ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (bindings Sexpr) (acc HashTable<string,llvm-value>)) ()
                         (match bindings
                           ((Cons pair rest)
                            (let ((nm (sexpr-sym-name (car pair))))
                              (let ((form (cdr pair)))
                                (let ((v (compile-value builder env fn-env captured cur-fn form)))
                                  (set acc nm v)
                                  (compile-let-values builder env fn-env captured cur-fn rest acc)))))
                           (_ ())))
                       ;; `(let ((name-sym . value-form)...) single-body-form)`
                       ;; (if/let/comparisons, labels/closures Stage 5): all
                       ;; binding values first (`compile-let-values`, against
                       ;; the untouched `env`), then shadow `env` with them
                       ;; (`bind-let-values`, saving whatever was already
                       ;; there per name into `saved`), compile the body, and
                       ;; finally undo the shadowing (`restore-let-values`) —
                       ;; see those two functions' doc comments for why the
                       ;; restore step matters and its one known limitation.
                       ;; Unlike `compile-if`, no retain/release decision is
                       ;; made for `Fn`-typed bindings here at all: folding a
                       ;; value into `env` under a new name is exactly what a
                       ;; parameter/capture binding already does, and the
                       ;; existing borrowed/fresh machinery (keyed on `env`
                       ;; membership, not on *how* a name got there) already
                       ;; accounts for it correctly — a `let`-bound *fresh*
                       ;; `Fn`-typed value that's never otherwise consumed
                       ;; simply leaks (never double-frees or corrupts), the
                       ;; same accepted tradeoff as an unreferenced boxed
                       ;; `labels` sibling or a captured reference cycle (see
                       ;; this module's doc comment).
                       (compile-let ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (let ((bindings (car (cdr e))))
                           (let ((body-form (car (cdr (cdr e)))))
                             (let ((acc (new-env)))
                               (compile-let-values builder env fn-env captured cur-fn bindings acc)
                               (let ((saved (new-env)))
                                 (bind-let-values env bindings acc saved)
                                 (let ((result (compile-value builder env fn-env captured cur-fn body-form)))
                                   (restore-let-values env bindings saved)
                                   result))))))
                       ;; `(lambda name ((captured . is-fn)...) ((param . is-fn)...) body)`
                       ;; — `Expr::Lambda` (a standalone escaping value) or a
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
                       ;; doc comment), R1-retaining its own params/captures
                       ;; on entry and R2-releasing them (minus whichever is
                       ;; bare-returned) on exit exactly like
                       ;; `compile-function`'s own top-level body and each
                       ;; `labels` sibling's body do, then builds an env
                       ;; array from `lcaptured`'s *current* values in the
                       ;; *outer* `env` (`compile-escaping-env-args`,
                       ;; retaining any borrowed one — this box can escape)
                       ;; and wraps the whole thing into a `ClosureBox`
                       ;; (`build-make-closure`, with `compute-fn-mask`'s
                       ;; bitmask so a later release of this box can
                       ;; recursively release whichever of its own captures
                       ;; are themselves closures) — the value this whole
                       ;; node evaluates to.
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
                                         (retain-bindings nested-builder nested-env lparams)
                                         (bind-captures nested-env nested-builder nested-fn lcaptured 0)
                                         (retain-bindings nested-builder nested-env lcaptured)
                                         (let ((v (compile-value nested-builder nested-env (new-fn-env) lcaptured nested-fn lbody)))
                                           (let ((protected (bare-returned-own-name lbody lparams lcaptured)))
                                             (release-bindings nested-builder m nested-env lparams protected)
                                             (release-bindings nested-builder m nested-env lcaptured protected)
                                             (build-ret nested-builder v))))))
                                   (let ((env-len (sexpr-list-length lcaptured)))
                                     (let ((env-ptr (alloca-args builder env-len)))
                                       ;; `lcaptured` is what's walked (the
                                       ;; new closure's own captured-name
                                       ;; list); `captured` (held constant,
                                       ;; not walked) is the *outer* scope's
                                       ;; own shared list — `resolve-value`'s
                                       ;; fallback needs that one, not
                                       ;; `lcaptured`, since `builder`/`env`/
                                       ;; `fn-env` here are still the outer
                                       ;; scope's (the nested function's own
                                       ;; body, with its own fresh `fn-env`,
                                       ;; was already compiled above — this
                                       ;; call is unrelated to that one).
                                       (compile-escaping-env-args builder env fn-env captured env-ptr lcaptured 0)
                                       (build-make-closure builder nested-fn env-ptr env-len (compute-fn-mask lcaptured 0))))))))))
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
                       ;; `captured` is empty), R1-retaining both (this
                       ;; sibling's own activation, like any other), before
                       ;; compiling its single body expression with
                       ;; `captured` carried forward so any call it makes to
                       ;; a sibling (or itself) can build that same env
                       ;; array right back, then R2-releasing both (minus
                       ;; whichever is bare-returned) before returning.
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
                                           (retain-bindings sib-builder sib-env param-syms)
                                           (bind-captures sib-env sib-builder sib-fn captured 0)
                                           (retain-bindings sib-builder sib-env captured)
                                           (let ((v (compile-value sib-builder sib-env inner-fn-env captured sib-fn def-body)))
                                             (let ((protected (bare-returned-own-name def-body param-syms captured)))
                                               (release-bindings sib-builder m sib-env param-syms protected)
                                               (release-bindings sib-builder m sib-env captured protected)
                                               (build-ret sib-builder v)
                                               (compile-labels-bodies inner-fn-env captured rest)))))))
                                    (None (panic (append "compile-labels-bodies: missing declaration for " nm))))))))
                           (_ ())))
                       ;; `(labels ((captured . is-fn)...) ((name
                       ;; ((param . is-fn)...) body))... trailing-body)`:
                       ;; declare every sibling, compile every body, then
                       ;; compile the trailing body *with the enclosing
                       ;; function's own `builder`/`env`* (it's ordinary
                       ;; code in the function currently being compiled, not
                       ;; a new function of its own) extended with
                       ;; `inner-fn-env` and this block's own
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
                       ;; The trailing body's own retain/release treatment
                       ;; (if any) is handled by whichever of
                       ;; `compile-function`/`compile-lambda`/
                       ;; `compile-labels-bodies` this `labels` form's
                       ;; result ultimately flows back up into — this
                       ;; function introduces no function activation of its
                       ;; own, so no R1/R2 here.
                       (compile-labels ((builder llvm-builder) (env HashTable<string,llvm-value>) (fn-env HashTable<string,llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (let ((inner-captured (car (cdr e))))
                           (let ((defs (car (cdr (cdr e)))))
                             (let ((trailing (car (cdr (cdr (cdr e))))))
                               (let ((inner-fn-env (new-fn-env)))
                                 (declare-labels-siblings inner-fn-env inner-captured defs)
                                 (compile-labels-bodies inner-fn-env inner-captured defs)
                                 (compile-value builder env inner-fn-env inner-captured cur-fn trailing)))))))
                (let ((v (compile-value builder env fn-env '() f body)))
                  (let ((protected (bare-returned-own-name body param-names '())))
                    (release-bindings builder m env param-names protected)
                    (build-ret builder v)
                    m)))))))))
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
