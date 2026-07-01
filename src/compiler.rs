//! The (typelisp-hosted) compiler body: AST (bridged to `Sexpr` by
//! [`crate::compile::ast_bridge`]) -> LLVM IR, built by calling the
//! `llvm-*` builtins (`crate::eval::interp`'s `eval_llvm_builtin_method`)
//! directly — the same "Rust provides the bindings, typelisp drives them"
//! split [docs/implementation-log.md](../docs/implementation-log.md) calls for. Loaded the same way
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
//! body-form...)`, see `compile-let`'s doc comment) — if/let/comparisons,
//! labels/closures Stage 5. `loop`/`break`/`return`/`setf` (`(loop
//! body-form...)`/`(break)`/`(return is-fn value-form)`/`(set name-str is-fn
//! value-form)`, see `compile-loop`/`compile-break`/`compile-return`/
//! `compile-set`'s doc comments) and `Unit` literals (`(unit)`, needed by
//! `return`'s value-less case) round out every native control-flow primitive
//! the checker has (`if`/`let`/`match`/`labels`/`loop`/`break`/`return` —
//! `match` alone remains uncompiled). `compile-value` grows a new tag-/
//! method-matching arm as later phases teach `ast_bridge` to translate more
//! `Expr` variants for real.
//!
//! **`loop`/`break`/`return`/`setf` also changed how every local variable is
//! represented** (`bind-params`/`bind-captures`/`bind-let-values`): `env`
//! now maps every name to a 1-element `alloca-args` *slot*, not the value
//! itself. A `setf` needs somewhere to write a *new* value that every later
//! read of the same name — including one from a *subsequent loop
//! iteration*, after control branches back to the top of the very same
//! basic block — will see; a plain SSA register can't do that (it's
//! immutable once defined, and nothing here builds real LLVM `phi` nodes —
//! see `compile-if`'s own alloca-based merge for why). `resolve-value`/
//! `retain-bindings`/`release-bindings` all `load-raw` a slot before
//! treating it as "the value"; `compile-set` is the only place that writes
//! to an existing slot a second time. `setf` on a *captured* name only
//! mutates this activation's own local copy (loaded once into its own slot
//! by `bind-captures`) — it does not write back to the original binding or
//! propagate to any other closure sharing that logical capture, unlike the
//! tree-walking interpreter's true shared-cell closures (`RtValue::Closure`).
//! Accepted as a documented gap rather than solved: none of `while`/
//! `dotimes`/`dolist` (or anything else this compiler is exercised against)
//! ever `setf`s a captured name, only a `let`-bound loop counter local to
//! the activation doing the looping.
//!
//! **`break`/`return` always exit the *nearest enclosing loop*, never a
//! function** (`Expr::Return`'s own doc comment — this is CL's `(return
//! value)`/`return-from nil`, not a Rust/C-style function return) — and
//! `Checker::check_lambda`/`check_labels` both reset the loop stack to empty
//! before checking a nested function's own body (see those functions' doc
//! comments), so a `break`/`return` can never need to unwind across an
//! activation boundary this compiler doesn't already track explicitly: it's
//! always a plain, local LLVM branch to a basic block already known at
//! compile time, never anything resembling a non-local exit/continuation.
//! `loop-exit`/`loop-slot`/`loop-root-base` (threaded through the whole ring
//! exactly like `cur-fn`) are the *nearest* enclosing loop's exit block,
//! 1-slot result merge, and the GC root stack's depth as of that loop's own
//! entry (`rt_root_count`, read once in `compile-loop`'s pre-header — see
//! `compile-break`/`compile-return`'s doc comments for why a `break`/
//! `return` needs that depth, not just the block/slot pair, to unwind
//! correctly through however many `let`/`match` scopes happen to be open
//! above it); `compile-loop` installs a fresh `Option::some` trio when
//! compiling its own body (nesting falls out of ordinary call-stack
//! scoping, no explicit push/pop needed — see `compile-loop`'s doc
//! comment), and `compile-lambda`/`compile-labels-bodies` reset to
//! `Option::none` when compiling a *new* function's own body, mirroring the
//! checker's reset exactly. `compile-labels`'s *trailing* body is not a new
//! function boundary (`check_labels` checks it against the *unmodified*
//! loop stack), so it forwards the trio unchanged instead.
//!
//! **Only statement position is supported for `break`/`return`** — directly
//! in a `loop` body's sequence (`compile-loop-body`), or as the *entire* form
//! of an `if` branch (`compile-if`'s own `block-terminated?` check) — both
//! because that's everything `while`/`dotimes`/`dolist`/hand-written `loop`
//! actually need, and because anything deeper (e.g. `(+ 1 (break))`, a
//! `break` nested inside an arithmetic operand or call argument) would need
//! every other `compile-*` helper that computes a sub-expression and then
//! emits more instructions afterward (`compile-assoc`'s `build-add`,
//! `compile-call-args`'/`compile-let-values`'s "next form" step, ...) to
//! also check `block-terminated?` before continuing — type-legal (`break`/
//! `return` are `Never`-typed, unifying with anything) but not attempted
//! here. A `break`/`return` in such a position compiles into IR with two
//! terminators in one block — well-known to be invalid, caught (if at all)
//! only by `llvm-module::verify`, not by anything in this file.
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
//! design notes this followed — `docs/implementation-log.md` has the full writeup); the
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
//! Scope<llvm-function>` is the table parallel to `env` that makes a name
//! resolve to a *callable function* rather than a value (see
//! `compile-apply`'s doc comment); `captured: Sexpr` (labels/closures
//! Stage 2) is the *current* direct-call scope's shared captured-name list
//! (empty outside any capturing `labels` block), threaded the same way so
//! every direct call site knows whether — and how — to build an env array
//! for its callee.
//!
//! **`env`/`fn-env` model a real "list of scopes," not one flat table per
//! function** — `Scope<V>` (`crate::check::registry::scope_def`/
//! `crate::eval::interp`'s `scope_*` builtins) is a stack of frames, each an
//! ordinary `String`-keyed map. A function activation (`compile-function`/
//! `compile-lambda`/each `labels` sibling) starts a *fresh* `Scope` —
//! `(new-env)`/`(new-fn-env)`, one frame, empty — except a `labels` sibling's
//! own `fn-env`, which instead is `(clone-frames inner-fn-env)`: every frame
//! currently on the *enclosing* scope's stack, shared by reference (no
//! per-entry copying), plus implicitly whatever this sibling's own
//! `declare-labels-siblings` pass already pushed on top of it. This is the
//! direct implementation of CLHS's `labels`: "the scope the created
//! function bindings encompasses the function definitions themselves as
//! well as the body" — and, because that sharing happens however deep the
//! nesting goes, it now also covers a `labels` block nested inside another:
//! an inner sibling can direct-call an *outer* `labels` block's own sibling,
//! fixing what used to be documented here as a known limitation. `let`
//! (`compile-let`) and `match` (`compile-match-arms`) push/pop a frame on
//! `env` the same way for their own bindings — never the old per-key
//! save/restore (`bind-let-values`/`restore-let-values`/`save-env-names`/
//! `restore-env-names`, all gone now), which could only remember one prior
//! value per name and so silently mis-restored a `let`/pattern with a
//! repeated binding name. A function's own *new* `Scope` never reaches into
//! whatever scope enclosed it — that boundary, and what crosses it, is still
//! entirely `captured`'s job (`freevars::labels_free_vars`'s unconditional
//! prefix-copy of the enclosing block's own captured-list is what lets a
//! `labels` sibling forward an outer sibling's captured values along when
//! calling it, however many levels removed — see that function's doc
//! comment).
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
//! **`compile-assoc` can call another already-`compile`d user-defined method**
//! (a `defmethod` or a `defstruct` accessor/setter, `(assoc type-name method
//! instance arg...)` for any `type-name` other than `i64`/`i32`) — the
//! composability gap that, until now, meant a `defun` whose body called
//! `p::x` could never itself be `compile`d at all. Resolved the same way a
//! top-level `defun`-to-`defun` call already is (`compile-call`): the callee
//! is found via `get-function` under the mangled name `type-name::method`
//! (exactly the literal string a standalone `(compile "type-name::method")`
//! names its own LLVM function, `Interp::add_compiled_function`'s
//! `internal_name`), and `Interp::compile_function` requires — and forward-
//! declares — every such target the same way it already does for an
//! `Expr::Call` target, so self-recursive method calls and ordinary
//! method-to-method calls both Just Work without `compile-assoc` itself
//! having to know whether `get-function` will succeed. Needed `Expr::Assoc`'s
//! own argument list to switch from the untagged `ast_list_to_sexpr` to the
//! `(kind . form)`-tagged `tagged_ast_list_to_sexpr` (`compile-call`'s own
//! argument shape) — the receiver/arguments of a general method call, unlike
//! an arithmetic operand, can be `Fn`- or `Sexpr`-typed and need the same
//! retain/GC-root treatment `compile-call-args` already gives those.
//!
//! Doesn't depend on `prelude.rs` (no `cond`/`when`/...) — only the
//! checker's native special forms (`if`/`let`/`match`/`labels`/`loop`/
//! `break`/`return`/`setf`) and builtins (`eq`/`append`/`car`/`cdr`/
//! `HashTable`'s methods, `Option`'s `some`/`none`/pattern-matching), so
//! loading order relative to the prelude doesn't matter.

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

;; Extracts a host `i64` out of an `(Int n)` Sexpr node — moved up here
;; (ahead of `compute-fn-mask`/`retain-bindings`/`release-bindings`, which
;; all need it for the `kind` tag `ast_bridge::tagged_sym_list` carries,
;; Stage 6 of the Sexpr-representation plan) from its original spot
;; alongside `compile-sexpr-tag-test`/`compile-sexpr-field`, since a
;; top-level `defun` can only call one already defined *earlier* in this
;; same source (no forward references — see this module's doc comment).
(defun sexpr-int ((s Sexpr)) i64
  (match s
    ((Int n) n)
    (_ (panic "expected an Int Sexpr node"))))

;; `names` is now a list of `(name . kind)` pairs (`ast_bridge::tagged_sym_list`
;; — `kind` generalized from a plain `is-fn` `Bool` to a 3-way `Int` tag in
;; Stage 6), not bare symbols — `kind` itself isn't needed here (binding a
;; value doesn't yet decide anything about its ownership; `retain-bindings`, called
;; right after, is what reads it).
;;
;; `env` maps every name to a *slot* (a 2-element `alloca-args` pointer), not
;; the value itself (`loop`/`break`/`return`/`setf`) — `setf` needs somewhere
;; to write a *new* value that every later read of the same name (including
;; one from a *subsequent loop iteration*, after control branches back to the
;; top of the same basic block) will see; a plain SSA register can't do that
;; (it's immutable once defined — see this module's doc comment's new
;; "mutable locals" paragraph for why a memory slot is unavoidable here, not
;; just a convenience). `resolve-value`/`retain-bindings`/`release-bindings`
;; all `load-raw` offset 0 of this slot before treating it as "the value";
;; `compile-set` is the one place that writes to it again after this initial
;; store. Offset 1 is a second word `retain-bindings` fills in right after
;; this, for a `kind = 2` (`Sexpr`) name only: the GC-root stack index that
;; name's root lands at (`rt_root_count`, called immediately before
;; `rt_push_sexpr_root`) — `compile-set` reads it back to fix that same root
;; in place on a `setf` (see `retain-bindings`'s doc comment for why an
;; update-in-place, not a second push, is what a reassignment needs).
;; Allocating the second word unconditionally (rather than only for `kind =
;; 2`) keeps this function kind-agnostic — for any other `kind`, offset 1 is
;; simply never read.
(defun bind-params ((env Scope<llvm-value>) (builder llvm-builder) (f llvm-function) (names Sexpr) (idx i32)) ()
  (match names
    ((Cons name-pair rest)
     (let ((nm (sexpr-sym-name (car name-pair))))
       (let ((slot (alloca-args builder 2)))
         (store-arg builder slot 0 (load-arg builder f idx))
         (set env nm slot))
       (bind-params env builder f rest (+ idx 1))))
    (_ ())))

;; The captures counterpart of `bind-params` (labels/closures Stage 2):
;; reads each captured name back out of a function's env array (`load-env`,
;; the env-array analogue of `load-arg`) into a fresh slot in `env` under its
;; own name (see `bind-params`'s doc comment for why a slot, not the value
;; directly), so `compile-var`'s ordinary by-name lookup finds it exactly
;; like a regular parameter. A no-op when `names` is empty — the Stage 1
;; (no-capture) path this leaves untouched.
(defun bind-captures ((env Scope<llvm-value>) (builder llvm-builder) (f llvm-function) (names Sexpr) (idx i32)) ()
  (match names
    ((Cons name-pair rest)
     (let ((nm (sexpr-sym-name (car name-pair))))
       (let ((slot (alloca-args builder 2)))
         (store-arg builder slot 0 (load-env builder f idx))
         (set env nm slot))
       (bind-captures env builder f rest (+ idx 1))))
    (_ ())))

;; 2^n by repeated doubling — `compute-fn-mask`'s one helper, not a generic
;; utility; `n` (a captured-slot index) is always small in practice so the
;; O(n) recursion is in no way a bottleneck.
(defun pow2 ((n i32)) i64
  (if (eq n 0) 1 (* (pow2 (- n 1)) 2)))

;; Computes `build-make-closure`'s `fn_mask` argument: a bitmask, one bit per
;; captured slot, set wherever that slot's `kind` tag (`ast_bridge::binding_kind`
;; — `0`=plain, `1`=fn, `2`=sexpr; Stage 6 of the Sexpr-representation plan
;; generalized this from a plain `is-fn` `Bool`, see `tagged_sym_list`'s doc
;; comment) is exactly `1` (fn) — entirely a *host*-level computation (no LLVM
;; IR involved, unlike everything `compile-*` builds): the mask is fully
;; determined by `names`' own tags, known already at the point this runs, so
;; there's nothing to generate code for. A `2` (sexpr) slot must *not* set its
;; bit here: it's a tagged `i64`, not a `ClosureBox` pointer, and
;; `interp::get_or_define_closure_release_fn`'s cascading release would
;; corrupt memory if it tried to treat one as a closure to recursively
;; release.
(defun compute-fn-mask ((names Sexpr) (idx i32)) i64
  (match names
    ((Cons name-pair rest)
     (let ((kind (sexpr-int (cdr name-pair))))
       (if (eq kind 1)
           (+ (pow2 idx) (compute-fn-mask rest (+ idx 1)))
           (compute-fn-mask rest (+ idx 1)))))
    (_ 0)))

;; Whether `nm` appears as the name half of any `(name . kind)` pair in
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
(defun name-is-borrowed? ((env Scope<llvm-value>) (nm string)) bool
  (match (get env nm)
    ((Some _) true)
    (None false)))

;; The general-expression-form counterpart of `name-is-borrowed?`: a `var`
;; node is borrowed exactly when its name is (`compile-call-args`'s own
;; argument forms can be arbitrary expressions, not just bare names — every
;; other tag's result is fresh by construction, see this module's doc
;; comment).
(defun form-is-borrowed? ((env Scope<llvm-value>) (form Sexpr)) bool
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
;;
;; Stage 6 of the Sexpr-representation plan (`docs/implementation-log.md` — the
;; "Sexprルート挿入パス"): a `kind = 2` (sexpr) name gets the analogous
;; treatment, but via `rt_push_sexpr_root` (a GC-root-stack push, not a
;; refcount increment) instead of `build-closure-retain` — a `Sexpr`-typed
;; parameter/capture is a tagged `i64` that may point into the GC-managed
;; cons heap, so without this, any allocation anywhere later in this
;; activation's body (a `cons`, another `rt_*` call) could trigger a GC that
;; reclaims it out from under a still-live binding (`typelisp-rt`'s
;; `rt_push_sexpr_root` doc comment/tests demonstrate exactly this failure
;; mode). `m` (needed to `get-function`/`build-call` `rt_push_sexpr_root`)
;; is a new explicit parameter here — unlike a ring member (`compile-apply`
;; and friends), this is an ordinary top-level `defun`, so it can't close
;; over `compile-function`'s own `m` the way `release-bindings` already
;; takes it explicitly for the matching reason (`build-closure-release`'s
;; own `m` argument).
;;
;; `setf`-reassignment fix: a `kind = 2` name also records, in its own slot's
;; offset 1 (`bind-params`/`bind-captures`'s doc comment), the GC-root stack
;; index `rt_push_sexpr_root`'s call is about to occupy — `rt_root_count`,
;; called *before* that push, returns exactly that index (`push_root` always
;; appends at the current length). Without recording it, `compile-set` would
;; have no way to find the right root to update when a later `setf` reassigns
;; this same name — that root would stay pinned to the value bound here
;; forever, leaving every reassigned value unrooted (`typelisp-rt`'s
;; `a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root`
;; demonstrates the resulting corruption directly).
(defun retain-bindings ((builder llvm-builder) (m llvm-module) (env Scope<llvm-value>) (names Sexpr)) ()
  (match names
    ((Cons name-pair rest)
     (let ((nm (sexpr-sym-name (car name-pair))))
       (let ((kind (sexpr-int (cdr name-pair))))
         (if (eq kind 1)
             (match (get env nm)
               ((Some slot) (let ((v (load-raw builder slot 0))) (let ((ignored (build-closure-retain builder v))) ())))
               (None ()))
             (if (eq kind 2)
                 (match (get env nm)
                   ((Some slot)
                    (let ((v (load-raw builder slot 0)))
                      (let ((root-idx (build-call builder (get-function m "rt_root_count") (alloca-args builder 0) 0)))
                        (store-arg builder slot 1 root-idx)
                        (let ((args-ptr (alloca-args builder 1)))
                          (store-arg builder args-ptr 0 v)
                          (let ((ignored (build-call builder (get-function m "rt_push_sexpr_root") args-ptr 1))) ())))))
                   (None ()))
                 ()))
         (retain-bindings builder m env rest))))
    (_ ())))

;; R2 (function exit, design notes): unconditionally releases every
;; `Fn`-typed name in `names`, except `protected` (see `bare-returned-own-name`)
;; — the `retain-bindings` call this activation made on entry, undone now
;; that its lifetime is ending, with the one exception being whichever name
;; (if any) is leaving as the bare return value instead of being dropped.
;;
;; Stage 6: the GC-root-stack counterpart for a `kind = 2` (sexpr) name —
;; `rt_pop_sexpr_root`, undoing `retain-bindings`' push. Unlike the `fn` case,
;; this runs *unconditionally*, `protected` or not: popping a root never
;; frees anything (it only stops a GC root walk from visiting that slot), so
;; even the bare-returned name's root must come off here — the tagged `i64`
;; itself still flows out via this activation's own `build-ret` regardless,
;; and if its *caller* needs to keep it alive across further allocations,
;; that's the caller's own binding (a `let`, another function's params) that
;; pushes a fresh root for it, exactly as it would for any other fresh
;; `Sexpr` value. Leaving this activation's own push on the stack past its
;; own lifetime would otherwise grow `Heap`'s root stack without bound across
;; repeated calls, eventually desyncing every later `rt_pop_sexpr_root`'s
;; LIFO assumption.
(defun release-bindings ((builder llvm-builder) (m llvm-module) (env Scope<llvm-value>) (names Sexpr) (protected Option<string>)) ()
  (match names
    ((Cons name-pair rest)
     (let ((nm (sexpr-sym-name (car name-pair))))
       (let ((kind (sexpr-int (cdr name-pair))))
         (if (eq kind 1)
             (if (protects? protected nm)
                 ()
                 (match (get env nm)
                   ((Some slot) (let ((v (load-raw builder slot 0))) (build-closure-release builder m v)))
                   (None ())))
             (if (eq kind 2)
                 (let ((args-ptr (alloca-args builder 0)))
                   (let ((ignored (build-call builder (get-function m "rt_pop_sexpr_root") args-ptr 0))) ()))
                 ()))
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

;; The bare `rt_push_sexpr_root`/`rt_pop_sexpr_root` call `retain-bindings`/
;; `release-bindings`/`bind-let-values`/`restore-let-values` each inline for
;; a *named* `kind = 2` binding (Stage 6 of the Sexpr-representation plan,
;; `docs/implementation-log.md`), factored out for a *fresh*, unnamed `Sexpr` value instead
;; (a `Cons`'s own car/cdr sub-expression, a call argument) — Stage 8's own
;; residual gap, the "残る選択肢" entry that gap left for a later pass:
;; a temporary `Sexpr` value sitting only in a raw stack slot (never bound
;; to a name) has no GC root at all, so any allocation made while computing
;; a *sibling* operand (another field, another argument) before the value is
;; finally consumed (a `cons`, a call) could have it reclaimed out from
;; under that slot. `push-sexpr-root` returns nothing (unlike
;; `build-closure-retain`, there's no chained value to hand back — a GC
;; root-stack push has no return value of its own worth threading through).
(defun push-sexpr-root ((builder llvm-builder) (m llvm-module) (v llvm-value)) ()
  (let ((args-ptr (alloca-args builder 1)))
    (store-arg builder args-ptr 0 v)
    (let ((ignored (build-call builder (get-function m "rt_push_sexpr_root") args-ptr 1))) ())))

;; `push-sexpr-root`'s pop, undoing exactly one push — see that function's
;; doc comment. Callers always know at LLVM-build time how many pushes they
;; made (a fixed field count, or `compile-call-args`'s own returned count —
;; see [`pop-sexpr-roots`] for the latter), so no runtime bookkeeping (a
;; counter slot, ...) is needed to pair pushes with pops.
(defun pop-sexpr-root ((builder llvm-builder) (m llvm-module)) ()
  (let ((args-ptr (alloca-args builder 0)))
    (let ((ignored (build-call builder (get-function m "rt_pop_sexpr_root") args-ptr 0))) ())))

;; Calls `pop-sexpr-root` `n` times — `compile-call-args`'s own return value
;; (how many of its arguments were `kind = 2` and so got a `push-sexpr-root`)
;; tells each call site exactly how many to undo here, once the call those
;; pushes were protecting is done.
(defun pop-sexpr-roots ((builder llvm-builder) (m llvm-module) (n i32)) ()
  (if (eq n 0)
      ()
      (let ((ignored (pop-sexpr-root builder m)))
        (pop-sexpr-roots builder m (- n 1)))))

;; `push-sexpr-root`'s permanent counterpart (general-ADT box field GC root
;; protection): `compile-construct-box-fields` calls this for a `kind = 2`
;; field right after storing it, instead of `push-sexpr-root`. The
;; difference is *why* no pop ever follows: `push-sexpr-root`'s caller
;; always knows, at LLVM-build time, the matching `pop-sexpr-root`(s) to
;; emit later in the *same* activation (a `let`'s own end, a call's return) —
;; but a box field's value must stay reachable for as long as the box
;; itself does, which can outlive this activation entirely (the box can be
;; returned, stored elsewhere, ...). `rt_push_permanent_sexpr_root` (see its
;; own doc comment) registers the root on a separate, append-only list
;; `Heap::gc`'s mark phase also walks, so it never has to be popped and
;; never desyncs `push-sexpr-root`/`pop-sexpr-root`'s own LIFO pairing
;; elsewhere. This is the same accepted trade-off as the box itself
;; (deliberately never `build-free`'d, see `compile-construct-box`'s doc
;; comment) — both leak for the process's lifetime rather than tracking a
;; box's real lifetime.
(defun push-permanent-sexpr-root ((builder llvm-builder) (m llvm-module) (v llvm-value)) ()
  (let ((args-ptr (alloca-args builder 1)))
    (store-arg builder args-ptr 0 v)
    (let ((ignored (build-call builder (get-function m "rt_push_permanent_sexpr_root") args-ptr 1))) ())))

;; `Scope::new` is a static call whose generic `V` can only be inferred from
;; an *expected* type (e.g. a `defun`'s declared return type), never from a
;; `let` binding's initializer (`check_let` always checks that with
;; `expected: None` — see `Checker::check_let`) — so this tiny wrapper exists
;; purely to give `Scope::new` a return type to infer from.
(defun new-env () Scope<llvm-value> (Scope::new))

;; See `new-env`'s comment — same reason this exists. `fn-env` maps a
;; direct-callable name (a `labels` sibling, or itself) to the
;; already-declared `llvm-function` `compile-apply` calls.
(defun new-fn-env () Scope<llvm-function> (Scope::new))

;; A plain (non-`Scope`) `HashTable`, for `compile-let`'s own `acc` —
;; `compile-let-values`'s scratch accumulator of each binding's freshly
;; computed value, keyed by name. This is never pushed/popped as a frame of
;; its own (it's discarded the moment `bind-let-values` has read every value
;; back out of it into `env`'s real new frame), so it has no need for
;; `Scope`'s stack — see `new-env`'s comment for why this still needs its
;; own tiny wrapper rather than an inline `(HashTable::new)`.
(defun new-acc-table () HashTable<string,llvm-value> (HashTable::new))

;; Counts a plain `Sexpr` list's elements — used to size the `i64*` args
;; array a direct call needs (`compile-apply`'s `alloca-args`/`build-call`),
;; and (labels/closures Stage 2) the `i64*` env array a captured call needs.
(defun sexpr-list-length ((s Sexpr)) i32
  (match s
    ((Cons _ rest) (+ 1 (sexpr-list-length rest)))
    (_ 0)))

;; Writes each `((name . kind) . value-form)` binding's already-computed
;; value (`acc`, built by `compile-let-values` against `env` *unmodified* —
;; see that function's doc comment for why this can't be one combined pass)
;; into a *fresh* slot in `env`'s current (top) frame. Unlike the old
;; HashTable-based `env`, no save/overwrite bookkeeping is needed here at
;; all: `compile-let` has already `push-frame`d a brand new frame before
;; calling this, so `Scope::set`'s "always write into the most recent frame"
;; rule means this can never clobber an outer binding of the same name — and
;; `pop-frame` (`compile-let`'s own cleanup) discards exactly this frame,
;; restoring the outer one exactly, with no possibility of the "same name
;; bound twice" bug the old per-key save/restore (`bind-let-values`/
;; `restore-let-values`) had. A fresh slot per binding is what makes a
;; `let`-bound name `setf`-able inside its own body (`loop`/`break`/`return`/
;; `setf`). No `is-fn`-style retain/release bookkeeping happens here at all:
;; a `let`-bound name resolves through `env` exactly like a parameter/capture
;; does, so the existing `name-is-borrowed?`/`form-is-borrowed?` (which only
;; ask "is this name present in `env`") already treat it correctly with no
;; further tagging — see `compile-let`'s doc comment.
;;
;; Stage 6 of the Sexpr-representation plan (`docs/implementation-log.md`): unlike the
;; `fn` case, a `kind = 2` (sexpr) binding *does* need action here —
;; `rt_push_sexpr_root`, the same GC-root push `retain-bindings` gives a
;; `Sexpr`-typed parameter/capture, since a `let`-bound `Sexpr` value is just
;; as exposed to a later allocation reclaiming it (the "leak it" tolerance
;; the `fn` case accepts isn't available here — an unrooted `Sexpr` doesn't
;; merely leak, it gets *corrupted*). `m` is a new explicit parameter for the
;; same reason `retain-bindings` gained one.
;;
;; `setf`-reassignment fix (see `retain-bindings`'s doc comment for the full
;; rationale): a `kind = 2` binding also records, in its own slot's offset 1
;; (`bind-params`'s doc comment), the GC-root stack index this `rt_push_sexpr_root`
;; call is about to occupy (`rt_root_count`, called first) — `compile-set`
;; reads it back to update that exact root in place on a later `setf`.
(defun bind-let-values ((builder llvm-builder) (m llvm-module) (env Scope<llvm-value>) (bindings Sexpr) (acc HashTable<string,llvm-value>)) ()
  (match bindings
    ((Cons pair rest)
     (let ((name-pair (car pair)))
       (let ((nm (sexpr-sym-name (car name-pair))))
         (let ((kind (sexpr-int (cdr name-pair))))
           (match (get acc nm)
             ((Some v) (let ((slot (alloca-args builder 2)))
                         (store-arg builder slot 0 v)
                         (set env nm slot)
                         (if (eq kind 2)
                             (let ((root-idx (build-call builder (get-function m "rt_root_count") (alloca-args builder 0) 0)))
                               (store-arg builder slot 1 root-idx)
                               (let ((args-ptr (alloca-args builder 1)))
                                 (store-arg builder args-ptr 0 v)
                                 (let ((ignored (build-call builder (get-function m "rt_push_sexpr_root") args-ptr 1))) ())))
                             ())))
             (None (panic "bind-let-values: missing computed value")))
           (bind-let-values builder m env rest acc)))))
    (_ ())))

;; Pops the GC root `bind-let-values` pushed for each `kind = 2` binding —
;; the `env`-restoration half of the old `restore-let-values` is gone
;; entirely now: `compile-let`'s own `pop-frame` discards this `let`'s whole
;; frame (every binding's slot at once, exact and order-independent), so
;; there is nothing left to "undo" name by name here. Skipping this would
;; leak a `Sexpr`-typed binding's GC root past the `let`'s own end.
;;
;; `compile-let` only reaches this call when the body did *not* already
;; terminate the current block (see its own doc comment) — a `let` body that
;; exits early via `break`/`return` has already built a `build-br` to the
;; loop's exit block, and this function's own `build-call`s would otherwise
;; land *after* that terminator: not a mere leak but invalid LLVM IR (a
;; second instruction following a basic block's one-and-only terminator),
;; caught by `module.verify()` (`Interp::compile_function`, added specifically
;; because this bug could otherwise reach the JIT as undefined behavior)
;; with "Terminator found in the middle of a basic block!". So on that path
;; the `kind = 2` binding's GC root is *not* popped here — no longer a
;; problem, though: `compile-break`/`compile-return` themselves now
;; unconditionally truncate the GC root stack back to the depth it was at
;; the nearest enclosing loop's own entry (`rt_truncate_sexpr_roots`, keyed
;; off `loop-root-base` — see `compile-loop`'s doc comment) right before
;; that same jump, which discards this exact root along with any other open
;; scope's, however many are nested between here and the loop. This function
;; only ever needs to handle the *normal* (falls off the end of the body)
;; exit path now.
(defun unroot-let-sexpr-values ((builder llvm-builder) (m llvm-module) (bindings Sexpr)) ()
  (match bindings
    ((Cons pair rest)
     (let ((kind (sexpr-int (cdr (car pair)))))
       (if (eq kind 2)
           (let ((args-ptr (alloca-args builder 0)))
             (let ((ignored (build-call builder (get-function m "rt_pop_sexpr_root") args-ptr 0))) ()))
           ())
       (unroot-let-sexpr-values builder m rest)))
    (_ ())))

;; Tests whether tagged Sexpr value `v` is Sexpr variant `variant`
;; (`registry::sexpr_def`'s variant order: 0=nil 1=int 2=float 3=char
;; 4=bool 5=sym 6=str 7=cons) -- `nil`/`bool` both compile to the same
;; 3-bit tag (`typelisp-rt`'s `TAG_IMMEDIATE`, 6) and are told apart by
;; `v`'s payload bits instead (`0` vs non-zero, see that crate's
;; `encode`/`decode`); every other variant has its own dedicated tag.
(defun compile-sexpr-tag-test ((builder llvm-builder) (v llvm-value) (variant i64)) llvm-value
  (let ((tag (build-and builder v (const-i64 builder 7))))
    (if (eq variant 0)
        (build-and builder
                    (build-icmp-eq builder tag (const-i64 builder 6))
                    (build-icmp-eq builder (build-lshr builder v (const-i64 builder 3)) (const-i64 builder 0)))
        (if (eq variant 4)
            (build-and builder
                        (build-icmp-eq builder tag (const-i64 builder 6))
                        (build-icmp-ne builder (build-lshr builder v (const-i64 builder 3)) (const-i64 builder 0)))
            (build-icmp-eq builder tag
                            (const-i64 builder
                                       (if (eq variant 1) 0
                                           (if (eq variant 2) 7
                                               (if (eq variant 3) 4
                                                   (if (eq variant 5) 2
                                                       (if (eq variant 6) 3
                                                           (if (eq variant 7) 1
                                                               (panic "compile-sexpr-tag-test: unknown Sexpr variant")))))))))))))

;; Extracts Sexpr variant `variant`'s field `idx` (0-based) from tagged
;; value `v` -- the inverse of `typelisp-rt`'s `encode` for each field
;; kind representable in compiled code today: `int`'s `i64` payload
;; (signed, `build-ashr`), `char`'s scalar (`build-lshr`), `bool`'s
;; payload (`1`/`2` -> `0`/`1`, matching `compile-bool`'s own convention),
;; `cons`'s two `Sexpr` fields (via `rt_car`/`rt_cdr` -- the only field kind
;; that needs a real heap read rather than pure bit manipulation), and (Stage
;; 7) `str`'s `Str` field. `str` is deliberately the *odd one out* among
;; these: unlike `int`/`char`/`bool`, extraction here does *not* strip the
;; 3-bit tag -- it returns `v` unchanged. That's not an oversight: a bare
;; `Type::Str` value's compiled representation is *defined* to be the exact
;; same tagged immediate a `Sexpr::Str` already is (`typelisp-rt`'s
;; `rt_str_new` returns it pre-tagged for the same reason), specifically so
;; it stays heap-referencing-and-therefore-GC-root-eligible under
;; `rt_push_sexpr_root`/`rt_pop_sexpr_root`'s existing generic `decode` --
;; stripping the tag the way `char` does would make a `Str` field
;; indistinguishable from a plain integer to that machinery, silently
;; reopening the exact GC-safety gap Stage 4/6 closed for `Sexpr`. `sym`'s
;; `Str` field and `float`'s `f64` field still aren't representable in
;; compiled code (a `Sym`'s tagged payload is a `SymId`, not a `StrId` --
;; extracting its *name* as a string needs its own interning primitive, a
;; separate gap from this stage's `str`), so a `Bind` pattern trying to
;; extract either still panics clearly here rather than producing garbage --
;; never reached for a `Wildcard` sub-pattern (`compile-ctor-subpatterns`
;; skips the call entirely then).
(defun compile-sexpr-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (variant i64) (idx i32)) llvm-value
  (if (eq variant 1)
      (build-ashr builder v (const-i64 builder 3))
      (if (eq variant 3)
          (build-lshr builder v (const-i64 builder 3))
          (if (eq variant 4)
              (build-sub builder (build-lshr builder v (const-i64 builder 3)) (const-i64 builder 1))
              (if (eq variant 7)
                  (let ((args-ptr (alloca-args builder 1)))
                    (store-arg builder args-ptr 0 v)
                    (if (eq idx 0)
                        (build-call builder (get-function m "rt_car") args-ptr 1)
                        (build-call builder (get-function m "rt_cdr") args-ptr 1)))
                  (if (eq variant 6)
                      v
                      (panic "compile-sexpr-field: field type is not representable in compiled code yet")))))))

;; Shared by every atomic pattern test (`pat-lit`'s literal-equality
;; check, a Ctor pattern's tag test): appends a fresh "this test passed"
;; block, branches on `test`, and leaves `builder` positioned at that new
;; block -- the caller's subsequent code (more guards, then the arm's
;; body) falls through naturally on success; on failure control jumps
;; straight to `fail-block` instead, never returning here.
(defun compile-pattern-guard ((builder llvm-builder) (cur-fn llvm-function) (test llvm-value) (fail-block llvm-basic-block)) ()
  (let ((ok-block (append-block cur-fn "pat-ok")))
    (build-cond-br builder test ok-block fail-block)
    (position-at-end builder ok-block)))

(defun compile-function ((m llvm-module) (name string) (param-names Sexpr) (body Sexpr)) llvm-module
    (let ((f (add-function m name)))
      (let ((b (append-block f "entry")))
        (let ((builder (llvm-builder::create)))
          (position-at-end builder b)
          (let ((env (new-env)))
            (bind-params env builder f param-names 0)
            (retain-bindings builder m env param-names)
            (let ((fn-env (new-fn-env)))
              (labels ((compile-value ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (match (car e)
                           ((Sym s)
                            (if (eq s "int")
                                (compile-int builder e)
                                (if (eq s "bool")
                                    (compile-bool builder e)
                                    (if (eq s "str")
                                        (compile-str builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                    (if (eq s "unit")
                                        (compile-unit builder)
                                        (if (eq s "var")
                                            (compile-var builder env fn-env captured e)
                                            (if (eq s "assoc")
                                                (compile-assoc builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                (if (eq s "apply")
                                                    (compile-apply builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                    (if (eq s "labels")
                                                        (compile-labels builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                        (if (eq s "call")
                                                            (compile-call builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                            (if (eq s "lambda")
                                                                (compile-lambda builder env fn-env captured e)
                                                                (if (eq s "apply-indirect")
                                                                    (compile-apply-indirect builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                    (if (eq s "if")
                                                                        (compile-if builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                        (if (eq s "let")
                                                                            (compile-let builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                            (if (eq s "loop")
                                                                                (compile-loop builder env fn-env captured cur-fn e)
                                                                                (if (eq s "break")
                                                                                    (compile-break builder loop-exit loop-slot loop-root-base)
                                                                                    (if (eq s "return")
                                                                                        (compile-return builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                        (if (eq s "set")
                                                                                            (compile-set builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                            (if (eq s "match")
                                                                                                (compile-match builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                (if (eq s "construct")
                                                                                                    (compile-construct builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                    (if (eq s "field-get")
                                                                                                        (compile-field-get builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                        (if (eq s "field-set")
                                                                                                            (compile-field-set builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                            (if (eq s "trait-call")
                                                                                                                (compile-trait-call builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                                (panic (append "compile-value: unsupported tag " s)))))))))))))))))))))))))
                           (_ (panic "compile-value: malformed node, expected a tagged list"))))
                       ;; `(unit)` — `Expr::Unit`, represented (like every
                       ;; other compiled value) as a plain `i64`; `0`, the
                       ;; same encoding `compile-bool` already uses for
                       ;; `false` (`registry::llvm_module_def`'s "every
                       ;; compiled value is a plain i64" convention). Needed
                       ;; now that `translate_return`'s value-less `(return)`
                       ;; case desugars to an explicit `(unit)` value-form
                       ;; rather than a second special-cased tag.
                       (compile-unit ((builder llvm-builder)) llvm-value
                         (const-i64 builder 0))
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
                       ;; `(str (int c0) (int c1) ...)` (Stage 7 of the
                       ;; Sexpr-representation plan, `docs/implementation-log.md`)
                       ;; — a string literal's content, one `(int c)` node per
                       ;; character (`ast_bridge`'s `Expr::Str` doc comment
                       ;; explains why not a pre-allocated `Value::Str`: the
                       ;; target program's `Heap` doesn't exist yet when this
                       ;; IR is built). Builds a fresh `args-ptr` array of one
                       ;; slot per character, fills it via `store-str-chars`
                       ;; (each slot compiled through the ordinary `int`
                       ;; dispatch above — no new literal-embedding mechanism
                       ;; needed), then calls `rt_str_new`, which returns the
                       ;; result already tagged (unlike `int`/`char`/`bool`
                       ;; literals, a `str` literal's compiled form *is* the
                       ;; full `Sexpr::Str` representation — see
                       ;; `compile-sexpr-field`'s own `str` doc comment for
                       ;; why). `rt_str_new` never runs a GC itself, but its
                       ;; result is exactly as unrooted as a fresh `rt_cons`
                       ;; cell until some caller protects it — the same
                       ;; obligation every other allocating call here already
                       ;; has (e.g. `compile-construct-sexpr`'s `Cons` field
                       ;; handling, or a `kind = 2` `let`/parameter binding).
                       (compile-str ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((chars (cdr e)))
                           (let ((n (sexpr-list-length chars)))
                             (let ((args-ptr (alloca-args builder n)))
                               (store-str-chars builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr chars 0)
                               (build-call builder (get-function m "rt_str_new") args-ptr n)))))
                       ;; Fills a `compile-str`-allocated array, one compiled
                       ;; `(int c)` character per slot — the `str`-literal
                       ;; analogue of `compile-construct-box-fields`.
                       (store-str-chars ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (args-ptr llvm-value) (forms Sexpr) (idx i32)) ()
                         (match forms
                           ((Cons form rest)
                            (store-arg builder args-ptr idx (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form))
                            (store-str-chars builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr rest (+ idx 1)))
                           (_ ())))
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
                       (compile-var ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
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
                       ;; A *nested* `labels` block's own bare-reference case
                       ;; works the same way for an *enclosing* one's
                       ;; siblings too, now that `fn-env` is a real `Scope`
                       ;; shared down through nesting (`compile-labels`'s
                       ;; `clone-frames` — see this module's doc comment):
                       ;; `(get fn-env name)` finds an outer sibling exactly
                       ;; like it finds one of this block's own, and
                       ;; `captured`'s unconditional prefix-inheritance
                       ;; (`freevars::labels_free_vars`) means the env array
                       ;; built right below already holds whatever that
                       ;; outer sibling's own captures are, at the same
                       ;; leading indices `bind-captures`/`load-env` read
                       ;; them back out at on the other side.
                       (resolve-value ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (name string)) llvm-value
                         (match (get env name)
                           ((Some slot) (load-raw builder slot 0))
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
                       (compile-env-args ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (env-ptr llvm-value) (pending-ptr llvm-value) (names Sexpr) (idx i32)) ()
                         (match names
                           ((Cons name-pair rest)
                            (let ((nm (sexpr-sym-name (car name-pair))))
                              (let ((is-fn (eq (sexpr-int (cdr name-pair)) 1)))
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
                       (compile-escaping-env-args ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (env-ptr llvm-value) (names Sexpr) (idx i32)) ()
                         (match names
                           ((Cons name-pair rest)
                            (let ((nm (sexpr-sym-name (car name-pair))))
                              (let ((is-fn (eq (sexpr-int (cdr name-pair)) 1)))
                                (let ((v (resolve-value builder env fn-env captured nm)))
                                  (if is-fn
                                      (if (name-is-borrowed? env nm)
                                          (store-arg builder env-ptr idx (build-closure-retain builder v))
                                          (store-arg builder env-ptr idx v))
                                      (store-arg builder env-ptr idx v))
                                  (compile-escaping-env-args builder env fn-env captured env-ptr rest (+ idx 1))))))
                           (_ ())))
                       ;; `(assoc type-name method instance arg...)` — two
                       ;; cases. `i64`/`i32` receivers (the same runtime
                       ;; representation, `RtValue::Int(i64)`, so one code
                       ;; path covers both) compile straight to the matching
                       ;; LLVM instruction, exactly as before; this guard
                       ;; still matters because `f64` defines methods under
                       ;; the same names (`registry::float_assoc`), so
                       ;; without it `(< 1.0 2.0)` would quietly compile as
                       ;; an integer comparison. Every other `type-name` is
                       ;; now treated as a call to a *user-defined* method
                       ;; (a `defmethod`/`defstruct` accessor or setter) —
                       ;; the composability gap this arm used to dead-end
                       ;; into a clear "unsupported receiver type" panic for.
                       ;; `args` is tagged the same `(kind . form)` way
                       ;; `compile-call`'s own argument list is
                       ;; (`ast_bridge`'s `Expr::Assoc` translation switched
                       ;; from `ast_list_to_sexpr` to `tagged_ast_list_to_sexpr`
                       ;; for this), so a receiver/argument that's itself
                       ;; `Fn`- or `Sexpr`-typed gets the same retain/GC-root
                       ;; treatment `compile-call-args` already gives an
                       ;; ordinary call's arguments — `args[0]` is the
                       ;; receiver for an instance method, with no special
                       ;; casing needed here (it flows through
                       ;; `compile-call-args` like any other argument).
                       ;; The callee is found by `get-function` under the
                       ;; mangled name `type-name::method` — exactly the
                       ;; literal string a standalone `(compile
                       ;; "type-name::method")` call names its own LLVM
                       ;; function (`Interp::add_compiled_function`'s
                       ;; `internal_name`), so a self-recursive method call
                       ;; resolves to the `add-function` `compile-function`'s
                       ;; own first step already declared, the same way a
                       ;; self-recursive `Expr::Call` does in `compile-call`.
                       ;; Every *other* method this body calls must already
                       ;; be `compile`d — `Interp::compile_function` checks
                       ;; that and forward-declares each one under this same
                       ;; mangled name before this function's own body is
                       ;; compiled, mirroring `Expr::Call`'s targets — so
                       ;; `get-function` never has to fail here in practice;
                       ;; if it still can't find `mangled` (a builtin method
                       ;; with no compiled implementation, e.g. `f64`/`str`/
                       ;; `char`, still out of scope), it panics clearly on
                       ;; its own.
                       ;; The `string` branch (Stage 7 — `prim_type_path`
                       ;; names `Type::Str`'s receiver type "string", distinct
                       ;; from `Sexpr`'s own "str" variant tag) covers only
                       ;; the built-in `String` instance methods
                       ;; `compiler.rs` itself needs to make a
                       ;; `Sexpr::Str`/`Type::Str` value actually useful once
                       ;; extracted/constructed —
                       ;; `length`/`ref`/`eq`/`append` — each a thin
                       ;; `alloca-args`/`store-arg`/`build-call` wrapper
                       ;; around the matching `typelisp-rt` primitive, the
                       ;; same shape the `i64`/`i32` branch below already
                       ;; uses for arithmetic. `upcase`/`downcase`/
                       ;; `substring`/`lt` stay out of scope for this stage
                       ;; (no compiled-code primitive backs them yet) and
                       ;; fall through to the same "unsupported method" panic
                       ;; as any other not-yet-compilable builtin method.
                       (compile-assoc ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((type-name (sexpr-str (car (cdr e)))))
                           (let ((method (sexpr-str (car (cdr (cdr e))))))
                             (let ((rest (cdr (cdr (cdr (cdr e))))))
                               (if (eq type-name "string")
                                   (let ((a (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (cdr (car rest)))))
                                     (if (eq method "length")
                                         (let ((args-ptr (alloca-args builder 1)))
                                           (store-arg builder args-ptr 0 a)
                                           (build-call builder (get-function m "rt_str_length") args-ptr 1))
                                         (let ((b (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (cdr (car (cdr rest))))))
                                           (if (eq method "ref")
                                               (let ((args-ptr (alloca-args builder 2)))
                                                 (store-arg builder args-ptr 0 a)
                                                 (store-arg builder args-ptr 1 b)
                                                 (build-call builder (get-function m "rt_str_ref") args-ptr 2))
                                               (if (eq method "eq")
                                                   (let ((args-ptr (alloca-args builder 2)))
                                                     (store-arg builder args-ptr 0 a)
                                                     (store-arg builder args-ptr 1 b)
                                                     (build-call builder (get-function m "rt_str_eq") args-ptr 2))
                                                   (if (eq method "append")
                                                       (let ((args-ptr (alloca-args builder 2)))
                                                         (store-arg builder args-ptr 0 a)
                                                         (store-arg builder args-ptr 1 b)
                                                         (build-call builder (get-function m "rt_str_append") args-ptr 2))
                                                       (panic (append "compile-assoc: unsupported str method " method))))))))
                                   (if (if (eq type-name "i64") true (eq type-name "i32"))
                                       (let ((a (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (cdr (car rest)))))
                                         (let ((b2 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (cdr (car (cdr rest))))))
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
                                                                               (panic (append "compile-assoc: unsupported method " method)))))))))))))
                                       (let ((mangled (append type-name (append "::" method))))
                                         (let ((argc (sexpr-list-length rest)))
                                           (let ((args-ptr (alloca-args builder argc)))
                                             (let ((pending-ptr (alloca-args builder argc)))
                                               (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr pending-ptr rest 0)))
                                                 (let ((result (build-call builder (get-function m mangled) args-ptr argc)))
                                                   (release-pending-args builder m pending-ptr argc 0)
                                                   (pop-sexpr-roots builder m sexpr-roots)
                                                   result))))))))))))
                       ;; Fills a previously-`alloca-args`'d array, one
                       ;; compiled argument per slot, exactly as before —
                       ;; plus, since each `forms` element is now a
                       ;; `(kind . arg-form)` pair (`ast_bridge::tagged_ast_list_to_sexpr`,
                       ;; generalized from a plain `is-fn` `Bool` in Stage 8
                       ;; of the Sexpr-representation plan, `docs/implementation-log.md`),
                       ;; marking `pending-ptr` (same length, caller-
                       ;; allocated) wherever that argument is `Fn`-typed
                       ;; (`kind = 1`) *and* fresh (see `form-is-borrowed?`)
                       ;; — the same post-call release bookkeeping
                       ;; `compile-env-args` does for env arrays, here for
                       ;; ordinary call arguments instead. A `kind = 2`
                       ;; (`Sexpr`) argument gets `push-sexpr-root`ed right
                       ;; here instead, *unconditionally* (no
                       ;; borrowed/fresh distinction — unlike a `ClosureBox`
                       ;; retain, a root-stack push is always correct to
                       ;; make and just as correct to immediately undo,
                       ;; never a double-free risk): this slot's value would
                       ;; otherwise sit unrooted in the raw `args-ptr` array
                       ;; while every *later* argument is computed (each one
                       ;; a fresh opportunity to allocate and trigger a `gc()`
                       ;; that reclaims it before the call ever happens) —
                       ;; the call-argument counterpart of
                       ;; `compile-construct-sexpr`'s own `Cons`-field fix.
                       ;; Returns how many such pushes it made, so the
                       ;; caller (`compile-apply`/`compile-call`/
                       ;; `compile-apply-indirect`) knows how many
                       ;; `pop-sexpr-root` calls to make once the call these
                       ;; roots were protecting is done.
                       (compile-call-args ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (args-ptr llvm-value) (pending-ptr llvm-value) (forms Sexpr) (idx i32)) i32
                         (match forms
                           ((Cons arg-pair rest)
                            (let ((kind (sexpr-int (car arg-pair))))
                              (let ((form (cdr arg-pair)))
                                (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                                  (store-arg builder args-ptr idx v)
                                  (if (eq kind 1)
                                      (if (form-is-borrowed? env form)
                                          (store-arg builder pending-ptr idx (const-i64 builder 0))
                                          (store-arg builder pending-ptr idx v))
                                      (store-arg builder pending-ptr idx (const-i64 builder 0)))
                                  (if (eq kind 2)
                                      (let ((ignored (push-sexpr-root builder m v)))
                                        (+ 1 (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr pending-ptr rest (+ idx 1))))
                                      (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr pending-ptr rest (+ idx 1)))))))
                           (_ 0)))
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
                       (compile-apply ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (car (cdr e)))))
                           (let ((arg-forms (cdr (cdr e))))
                             (let ((argc (sexpr-list-length arg-forms)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (let ((pending-ptr (alloca-args builder argc)))
                                   (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr pending-ptr arg-forms 0)))
                                     (match (get fn-env nm)
                                       ((Some target)
                                        (let ((env-len (sexpr-list-length captured)))
                                          (if (eq env-len 0)
                                              (let ((result (build-call builder target args-ptr argc)))
                                                (release-pending-args builder m pending-ptr argc 0)
                                                (pop-sexpr-roots builder m sexpr-roots)
                                                result)
                                              (let ((env-ptr (alloca-args builder env-len)))
                                                (let ((env-pending-ptr (alloca-args builder env-len)))
                                                  (compile-env-args builder env fn-env captured env-ptr env-pending-ptr captured 0)
                                                  (let ((result (build-call-with-env builder target args-ptr argc env-ptr env-len)))
                                                    (release-pending-args builder m pending-ptr argc 0)
                                                    (release-pending-args builder m env-pending-ptr env-len 0)
                                                    (pop-sexpr-roots builder m sexpr-roots)
                                                    result))))))
                                       (None (panic (append "compile-apply: no direct-callable function named " nm)))))))))))
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
                       (compile-call ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((raw-nm (sexpr-str (car (cdr e)))))
                           (let ((nm (if (eq raw-nm "car") "rt_car"
                                         (if (eq raw-nm "cdr") "rt_cdr"
                                             (if (eq raw-nm "cons") "rt_cons"
                                                 (if (eq raw-nm "set-car") "rt_set_car"
                                                     (if (eq raw-nm "set-cdr") "rt_set_cdr"
                                                         raw-nm)))))))
                             (let ((arg-forms (cdr (cdr e))))
                               (let ((argc (sexpr-list-length arg-forms)))
                                 (let ((args-ptr (alloca-args builder argc)))
                                   (let ((pending-ptr (alloca-args builder argc)))
                                     (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr pending-ptr arg-forms 0)))
                                       (let ((result (build-call builder (get-function m nm) args-ptr argc)))
                                         (release-pending-args builder m pending-ptr argc 0)
                                         (pop-sexpr-roots builder m sexpr-roots)
                                         result)))))))))
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
                       (compile-apply-indirect ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((callee-form (car (cdr e))))
                           (let ((closure (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base callee-form)))
                             (let ((arg-forms (cdr (cdr e))))
                               (let ((argc (sexpr-list-length arg-forms)))
                                 (let ((args-ptr (alloca-args builder argc)))
                                   (let ((pending-ptr (alloca-args builder argc)))
                                     (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr pending-ptr arg-forms 0)))
                                       (let ((result (build-closure-apply builder closure args-ptr argc)))
                                         (release-pending-args builder m pending-ptr argc 0)
                                         (pop-sexpr-roots builder m sexpr-roots)
                                         (if (form-is-borrowed? env callee-form)
                                             ()
                                             (build-closure-release builder m closure))
                                         result)))))))))
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
                       ;; logic out twice). Reused as-is by `compile-return`
                       ;; (`loop`/`break`/`return`/`setf`) for a `return`
                       ;; value and by `compile-set` for a `setf`'s new
                       ;; value — both store a possibly-borrowed value into a
                       ;; slot that outlives this activation's own ordinary
                       ;; R1/R2 bookkeeping (the loop's merge slot; the
                       ;; target variable's own slot), exactly the same
                       ;; boundary an `if` merge crosses, so the same fix
                       ;; applies unchanged.
                       (compile-if-branch ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (is-fn bool) (form Sexpr)) llvm-value
                         (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
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
                       ;;
                       ;; `loop`/`break`/`return`/`setf`: either branch may
                       ;; itself end in a `break`/`return` (directly, or via
                       ;; a nested `if`/`loop`) — `compile-if-branch`'s own
                       ;; `compile-value` call would then already have left
                       ;; the current block terminated (a `build-br` straight
                       ;; to the enclosing loop's exit block), so the
                       ;; store-then-branch-to-merge that follows must be
                       ;; skipped for that branch (`block-terminated?` —
                       ;; LLVM allows only one terminator per block; jumping
                       ;; to `loop-exit` already *is* this branch's only way
                       ;; out, so it must never also try to fall through to
                       ;; `merge-block`). The merge block itself is left
                       ;; untouched either way — `compile-loop-body`'s own
                       ;; `block-terminated?` check (after this whole `if`
                       ;; returns) is what decides whether *that* level
                       ;; continues, and `merge-block`'s own eventual
                       ;; terminator comes from whatever code the caller
                       ;; emits next, exactly as before this addition (see
                       ;; this module's doc comment).
                       (compile-if ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((is-fn (sexpr-bool (car (cdr e)))))
                           (let ((cond-form (car (cdr (cdr e)))))
                             (let ((then-form (car (cdr (cdr (cdr e))))))
                               (let ((else-form (car (cdr (cdr (cdr (cdr e)))))))
                                 (let ((cond-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base cond-form)))
                                   (let ((then-block (append-block cur-fn "if-then")))
                                     (let ((else-block (append-block cur-fn "if-else")))
                                       (let ((merge-block (append-block cur-fn "if-merge")))
                                         (let ((slot (alloca-args builder 1)))
                                           (build-cond-br builder cond-v then-block else-block)
                                           (position-at-end builder then-block)
                                           (let ((then-v (compile-if-branch builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base is-fn then-form)))
                                             (if (block-terminated? builder)
                                                 ()
                                                 (let ((ignored (store-arg builder slot 0 then-v)))
                                                   (build-br builder merge-block))))
                                           (position-at-end builder else-block)
                                           (let ((else-v (compile-if-branch builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base is-fn else-form)))
                                             (if (block-terminated? builder)
                                                 ()
                                                 (let ((ignored (store-arg builder slot 0 else-v)))
                                                   (build-br builder merge-block))))
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
                       (compile-let-values ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (bindings Sexpr) (acc HashTable<string,llvm-value>)) ()
                         (match bindings
                           ((Cons pair rest)
                            (let ((nm (sexpr-sym-name (car (car pair)))))
                              (let ((form (cdr pair)))
                                (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                                  (set acc nm v)
                                  (compile-let-values builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base rest acc)))))
                           (_ ())))
                       ;; Stage 8 of the Sexpr-representation plan
                       ;; (`docs/implementation-log.md`): compiles a `let` body's statement
                       ;; sequence one form at a time, the same
                       ;; `block-terminated?` short-circuit
                       ;; `compile-loop-body` already makes (every form after
                       ;; one that left the block terminated — a `break`/
                       ;; `return`, possibly nested inside an `if`/`match` arm
                       ;; that took it — would be unreachable, and emitting
                       ;; IR into an already-terminated block is malformed).
                       ;; Unlike `compile-loop-body`, which only ever produces
                       ;; a value via `break`/`return` and so discards every
                       ;; form's own value, this *is* CL `let`'s actual
                       ;; "body is a sequence, result is the last form's
                       ;; value" semantics — needed once a `let` body can hold
                       ;; more than one statement (`dolist`'s own macro
                       ;; expansion always produces one: `,@body` followed by
                       ;; a hidden `setf`). An empty body (`(let ((x 1)))`,
                       ;; CL `let`'s own `Unit`-typed case) compiles to
                       ;; `compile-unit`, the same `Unit` encoding every other
                       ;; empty-body shape in this module already uses.
                       (compile-let-body ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (forms Sexpr)) llvm-value
                         (match forms
                           ((Cons form rest)
                            (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                              (if (block-terminated? builder)
                                  v
                                  (match rest
                                    ((Cons _ _) (compile-let-body builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base rest))
                                    (_ v)))))
                           (_ (compile-unit builder))))
                       ;; `(let ((name-sym . value-form)...) body-form...)`
                       ;; (if/let/comparisons, labels/closures Stage 5; the
                       ;; trailing variadic body, Stage 8 of the
                       ;; Sexpr-representation plan): all binding values first
                       ;; (`compile-let-values`, against the untouched `env`),
                       ;; then `push-frame` a brand new frame and write every
                       ;; binding into it (`bind-let-values`), compile the
                       ;; body (`compile-let-body`), pop any `Sexpr` GC roots
                       ;; bindings pushed (`unroot-let-sexpr-values`) — guarded
                       ;; by `block-terminated?`, since a body that exited
                       ;; early via `break`/`return` has already closed the
                       ;; current block with its own `build-br`, and calling
                       ;; `unroot-let-sexpr-values` unconditionally there would
                       ;; build instructions after that terminator (invalid
                       ;; IR — see that function's own doc comment for the
                       ;; accepted-leak tradeoff this guard implies) — and
                       ;; finally `pop-frame` to discard the whole frame at
                       ;; once — see this module's doc comment for why a
                       ;; pushed/popped *frame*, not the old per-key
                       ;; save/restore, is what a `let` (or any other nested
                       ;; scope) actually needs. Unlike `compile-if`, no
                       ;; retain/release decision is made for `Fn`-typed
                       ;; bindings here at all: folding a value into `env`
                       ;; under a new name is exactly what a parameter/capture
                       ;; binding already does, and the existing
                       ;; borrowed/fresh machinery (keyed on `env` membership,
                       ;; not on *how* a name got there) already accounts for
                       ;; it correctly — a `let`-bound *fresh* `Fn`-typed
                       ;; value that's never otherwise consumed simply leaks
                       ;; (never double-frees or corrupts), the same accepted
                       ;; tradeoff as an unreferenced boxed `labels` sibling
                       ;; or a captured reference cycle (see this module's
                       ;; doc comment).
                       (compile-let ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((bindings (car (cdr e))))
                           (let ((body-forms (cdr (cdr e))))
                             (let ((acc (new-acc-table)))
                               (compile-let-values builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base bindings acc)
                               (push-frame env)
                               (bind-let-values builder m env bindings acc)
                               (let ((result (compile-let-body builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base body-forms)))
                                 (if (block-terminated? builder)
                                     ()
                                     (unroot-let-sexpr-values builder m bindings))
                                 (pop-frame env)
                                 result)))))
                       ;; `(lambda name ((captured . kind)...) ((param . kind)...) body)`
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
                       (compile-lambda ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
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
                                         (retain-bindings nested-builder m nested-env lparams)
                                         (bind-captures nested-env nested-builder nested-fn lcaptured 0)
                                         (retain-bindings nested-builder m nested-env lcaptured)
                                         ;; `loop`/`break`/`return`/`setf`: a
                                         ;; `lambda` is a new function
                                         ;; boundary — `break`/`return` can't
                                         ;; reach an outer loop through it
                                         ;; (`Checker::check_lambda` resets
                                         ;; the loop stack the same way), so
                                         ;; this body is compiled with no
                                         ;; enclosing loop at all, regardless
                                         ;; of whatever loop (if any) the
                                         ;; `lambda` form itself sits inside.
                                         (let ((v (compile-value nested-builder nested-env (new-fn-env) lcaptured nested-fn (Option::none) (Option::none) (Option::none) lbody)))
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
                       (declare-labels-siblings ((inner-fn-env Scope<llvm-function>) (captured Sexpr) (defs Sexpr)) ()
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
                       ;; (and in `inner-fn-env`'s — really the enclosing
                       ;; scope's `fn-env`, just `push-frame`d — newest
                       ;; frame), give each its own block and builder, bind
                       ;; its own parameters into a fresh `env`, and —
                       ;; labels/closures Stage 2 — load every one of the
                       ;; block's shared captured names into that same `env`
                       ;; too (`bind-captures`, a no-op when `captured` is
                       ;; empty), R1-retaining both (this sibling's own
                       ;; activation, like any other). Each sibling's own
                       ;; `fn-env` for compiling its body is
                       ;; `(clone-frames inner-fn-env)` — sharing every frame
                       ;; currently on `inner-fn-env`'s stack (this block's
                       ;; own siblings *and* every enclosing scope's, however
                       ;; deeply `labels` is nested — see this module's doc
                       ;; comment for why CLHS's "the scope encompasses the
                       ;; function definitions themselves" is implemented
                       ;; exactly this way) by reference, not by copying their
                       ;; entries, then compiling its single body expression
                       ;; with `captured` carried forward so any call it
                       ;; makes to a sibling (or itself, or — now — an
                       ;; enclosing `labels` block's own sibling) can build
                       ;; that same env array right back, then R2-releasing
                       ;; both (minus whichever is bare-returned) before
                       ;; returning.
                       (compile-labels-bodies ((inner-fn-env Scope<llvm-function>) (captured Sexpr) (defs Sexpr)) ()
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
                                           (retain-bindings sib-builder m sib-env param-syms)
                                           (bind-captures sib-env sib-builder sib-fn captured 0)
                                           (retain-bindings sib-builder m sib-env captured)
                                           ;; `loop`/`break`/`return`/`setf`:
                                           ;; a `labels` sibling's own body is
                                           ;; a new function boundary too
                                           ;; (`Checker::check_labels` resets
                                           ;; the loop stack per def, exactly
                                           ;; like `check_lambda` — see
                                           ;; `compile-lambda`'s matching
                                           ;; comment), so no enclosing loop
                                           ;; here either.
                                           (let ((sib-fn-env (clone-frames inner-fn-env)))
                                             (let ((v (compile-value sib-builder sib-env sib-fn-env captured sib-fn (Option::none) (Option::none) (Option::none) def-body)))
                                               (let ((protected (bare-returned-own-name def-body param-syms captured)))
                                                 (release-bindings sib-builder m sib-env param-syms protected)
                                                 (release-bindings sib-builder m sib-env captured protected)
                                                 (build-ret sib-builder v)
                                                 (compile-labels-bodies inner-fn-env captured rest))))))))
                                    (None (panic (append "compile-labels-bodies: missing declaration for " nm))))))))
                           (_ ())))
                       ;; `(labels ((captured . kind)...) ((name
                       ;; ((param . kind)...) body))... trailing-body)`:
                       ;; `push-frame fn-env` a fresh frame for this block's
                       ;; own siblings (CLHS's "scope encompasses the function
                       ;; definitions themselves" — see this module's doc
                       ;; comment), declare every sibling into it, compile
                       ;; every body, then compile the trailing body *with
                       ;; the enclosing function's own `builder`/`env`* (it's
                       ;; ordinary code in the function currently being
                       ;; compiled, not a new function of its own) extended
                       ;; with `fn-env` (now holding this block's own frame on
                       ;; top of everything enclosing) and this block's own
                       ;; `inner-captured`, so it can call the siblings
                       ;; (including any that capture) too — then `pop-frame`
                       ;; to discard exactly that frame, restoring `fn-env` to
                       ;; whatever it held on entry. The `captured` parameter
                       ;; `compile-labels` itself receives — the *enclosing*
                       ;; scope's captured-name list — goes unused here: this
                       ;; block's own `inner-captured`, read straight out of
                       ;; `e`, is what every call made from within it
                       ;; (siblings or trailing body alike) needs, not
                       ;; whatever scope `labels` itself was compiled in. The
                       ;; trailing body's own retain/release treatment (if
                       ;; any) is handled by whichever of
                       ;; `compile-function`/`compile-lambda`/
                       ;; `compile-labels-bodies` this `labels` form's
                       ;; result ultimately flows back up into — this
                       ;; function introduces no function activation of its
                       ;; own, so no R1/R2 here.
                       (compile-labels ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((inner-captured (car (cdr e))))
                           (let ((defs (car (cdr (cdr e)))))
                             (let ((trailing (car (cdr (cdr (cdr e))))))
                               (push-frame fn-env)
                               (declare-labels-siblings fn-env inner-captured defs)
                               (compile-labels-bodies fn-env inner-captured defs)
                               ;; The trailing body is *not* a new function
                               ;; boundary (`Checker::check_labels` checks
                               ;; `args[1..]` against the *unmodified*
                               ;; loop stack — see this module's doc
                               ;; comment), so `loop-exit`/`loop-slot` are
                               ;; forwarded unchanged here, unlike each
                               ;; def's own body just above.
                               (let ((result (compile-value builder env fn-env inner-captured cur-fn loop-exit loop-slot loop-root-base trailing)))
                                 (pop-frame fn-env)
                                 result)))))
                       ;; `(loop body-form...)` — `Expr::Loop` (`loop`/
                       ;; `break`/`return`/`setf`). Installs a fresh
                       ;; loop-exit block and a 1-slot result merge (the same
                       ;; alloca-args/store-arg/load-raw triple `compile-if`
                       ;; already uses for its own 2-way merge, here for an
                       ;; n-way set of `break`/`return` exits instead),
                       ;; branches into a fresh loop-body block, and compiles
                       ;; `body-forms` there in sequence (`compile-loop-body`)
                       ;; — installing `(Option::some ...)` for all three as
                       ;; it recurses into its own body is what makes a
                       ;; nested `break`/`return` resolve to *this* loop
                       ;; rather than whatever (if any) loop encloses it;
                       ;; nothing further is needed to support nesting, since
                       ;; ordinary call-stack scoping restores the outer
                       ;; loop's own trio the moment this call returns (just
                       ;; like `cur-fn`/`captured` already do for `labels`/
                       ;; `lambda` nesting). Loops back to the body block
                       ;; unless the body's own last statement already
                       ;; terminated it (a bare `break`/`return` reached
                       ;; without falling through) — `block-terminated?` is
                       ;; the same check `compile-if`'s own branches make,
                       ;; for the identical reason (one terminator per block,
                       ;; max).
                       ;;
                       ;; `root-base` (the third of the trio, alongside
                       ;; `exit-block`/`slot`): reads the GC root stack's
                       ;; current depth (`rt_root_count`) once, in the
                       ;; pre-header block, *before* the loop's own body ever
                       ;; runs — every ordinary iteration returns the root
                       ;; stack to this exact depth on its own (each nested
                       ;; `let`/`match` scope pops what it pushed as control
                       ;; flow passes back out through it), so this is the
                       ;; depth a `break`/`return` reached from *anywhere*
                       ;; inside this loop's body — however many scopes deep
                       ;; — needs to unwind back to. See `compile-break`/
                       ;; `compile-return`'s own doc comments for why a
                       ;; single `rt_truncate_sexpr_roots` call against this
                       ;; value, rather than each scope's own ordinary pop,
                       ;; is what makes that unwind correct regardless of
                       ;; nesting depth.
                       (compile-loop ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (e Sexpr)) llvm-value
                         (let ((body-forms (cdr e)))
                           (let ((loop-block (append-block cur-fn "loop-body")))
                             (let ((exit-block (append-block cur-fn "loop-exit")))
                               (let ((slot (alloca-args builder 1)))
                                 (let ((root-base (build-call builder (get-function m "rt_root_count") (alloca-args builder 0) 0)))
                                   (build-br builder loop-block)
                                   (position-at-end builder loop-block)
                                   (compile-loop-body builder env fn-env captured cur-fn (Option::some exit-block) (Option::some slot) (Option::some root-base) body-forms)
                                   (if (block-terminated? builder)
                                       ()
                                       (build-br builder loop-block))
                                   (position-at-end builder exit-block)
                                   (load-raw builder slot 0)))))))
                       ;; Compiles a `loop` body's statement sequence one
                       ;; form at a time, for effect, stopping the moment any
                       ;; one of them leaves the current block already
                       ;; terminated (a `break`/`return` — directly, or
                       ;; nested inside an `if` branch that took it, see
                       ;; `compile-if`'s own identical check) — every form
                       ;; still queued after that point is unreachable, and
                       ;; emitting its IR into an already-terminated block
                       ;; would be malformed (LLVM allows only one terminator
                       ;; per block). `loop-exit`/`loop-slot`/`loop-root-base`
                       ;; are always `Option::some` here (`compile-loop`
                       ;; installs them right before this is first called) —
                       ;; threaded as `Option` only because `compile-value`'s
                       ;; shared signature must also serve every *other* call
                       ;; site, where there may be no enclosing loop at all.
                       (compile-loop-body ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (forms Sexpr)) ()
                         (match forms
                           ((Cons form rest)
                            (let ((ignored (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                              (if (block-terminated? builder)
                                  ()
                                  (compile-loop-body builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base rest))))
                           (_ ())))
                       ;; `(break)` — unconditionally jumps to the nearest
                       ;; enclosing loop's exit block, storing `Unit` (`0`,
                       ;; the same encoding `compile-unit`/`compile-bool`'s
                       ;; `false` use) as the loop's result. `loop-exit`/
                       ;; `loop-slot`/`loop-root-base` are always
                       ;; `Option::some` in any program that reaches this at
                       ;; all — `Checker::check_break` already rejects a
                       ;; `break` outside of a loop at type-checking time,
                       ;; long before this runs — so the `None` arms are
                       ;; purely defensive, the same role `resolve-value`'s
                       ;; "unbound variable" panic plays for an analogous
                       ;; "the type checker already ruled this out"
                       ;; situation.
                       ;;
                       ;; `rt_truncate_sexpr_roots(rb)`, right before the
                       ;; jump: discards every `Sexpr`-typed GC root any
                       ;; `let`/`match` scope between here and the loop
                       ;; pushed and would otherwise never get the chance to
                       ;; pop, since this jump skips straight past their own
                       ;; ordinary pop-on-scope-exit code entirely (that code
                       ;; only runs when control falls back *out* of a scope
                       ;; normally — `compile-let`'s `block-terminated?`
                       ;; guard around `unroot-let-sexpr-values`, and
                       ;; `compile-match`'s `pop-sexpr-root` at its own
                       ;; merge block, both explicitly skip that pop for
                       ;; exactly this reason, deferring the cleanup to
                       ;; here instead). A no-op if nothing is open above
                       ;; the loop (`rb` already equals the current root
                       ;; count) — see `typelisp_rt::rt_truncate_sexpr_roots`'s
                       ;; own doc comment.
                       (compile-break ((builder llvm-builder) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>)) llvm-value
                         (match loop-exit
                           ((Some eb)
                            (match loop-slot
                              ((Some slot)
                               (match loop-root-base
                                 ((Some rb)
                                  (let ((truncate-args (alloca-args builder 1)))
                                    (store-arg builder truncate-args 0 rb)
                                    (let ((ignored (build-call builder (get-function m "rt_truncate_sexpr_roots") truncate-args 1))) ())
                                    (let ((zero (const-i64 builder 0)))
                                      (store-arg builder slot 0 zero)
                                      (build-br builder eb)
                                      zero)))
                                 (None (panic "compile-break: not inside a loop"))))
                              (None (panic "compile-break: not inside a loop"))))
                           (None (panic "compile-break: not inside a loop"))))
                       ;; `(return is-fn value-form)` — `Expr::Return`,
                       ;; including the implicit `Unit` `ast_bridge` already
                       ;; substitutes for a value-less `(return)`. Like
                       ;; `compile-if-branch`'s own branch value, a
                       ;; *borrowed* `Fn`-typed value needs an explicit
                       ;; retain right here before it's stored into the
                       ;; loop's merge slot — that slot outlives this
                       ;; activation's own ordinary R1/R2 bookkeeping (it's
                       ;; read back out by `compile-loop` itself, once this
                       ;; activation has nothing more to say about it),
                       ;; exactly the same boundary `compile-if-branch`
                       ;; already closes for an `if`'s own branches — reused
                       ;; here rather than duplicated a third time. Same
                       ;; `rt_truncate_sexpr_roots(rb)` call as `compile-break`,
                       ;; for the identical reason — see that function's doc
                       ;; comment — placed after `v` itself is computed
                       ;; (`compile-if-branch` may recurse through further
                       ;; nested scopes to get there) but before `v` is
                       ;; stored into the loop's merge slot; `rt_truncate_
                       ;; sexpr_roots` never allocates, so there's no GC
                       ;; between computing `v` and storing it this couldn't
                       ;; already have happened without this call.
                       (compile-return ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((is-fn (sexpr-bool (car (cdr e)))))
                           (let ((value-form (car (cdr (cdr e)))))
                             (let ((v (compile-if-branch builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base is-fn value-form)))
                               (match loop-exit
                                 ((Some eb)
                                  (match loop-slot
                                    ((Some slot)
                                     (match loop-root-base
                                       ((Some rb)
                                        (let ((truncate-args (alloca-args builder 1)))
                                          (store-arg builder truncate-args 0 rb)
                                          (let ((ignored (build-call builder (get-function m "rt_truncate_sexpr_roots") truncate-args 1))) ())
                                          (store-arg builder slot 0 v)
                                          (build-br builder eb)
                                          v))
                                       (None (panic "compile-return: not inside a loop"))))
                                    (None (panic "compile-return: not inside a loop"))))
                                 (None (panic "compile-return: not inside a loop")))))))
                       ;; `(set name-str kind value-form)` — `Expr::Set`
                       ;; (`setf`). Reuses `compile-if-branch`'s retain logic
                       ;; for the same reason `compile-return` does (the
                       ;; target's slot outlives this activation), then
                       ;; overwrites that slot in place — `bind-params`/
                       ;; `bind-captures`/`bind-let-values` already made
                       ;; every local name resolve through a slot rather than
                       ;; a raw SSA value for exactly this (see those
                       ;; functions' doc comments): a later `resolve-value`/
                       ;; `retain-bindings`/`release-bindings` call (even one
                       ;; from a *subsequent loop iteration*, after control
                       ;; branches back to the top of the same basic block)
                       ;; will `load-raw` this same slot and see the new
                       ;; value. A name absent from `env` altogether (e.g. a
                       ;; `labels` sibling's own name, which resolves through
                       ;; `fn-env` instead, never `env`) panics — `setf` on a
                       ;; sibling's own name is type-checker-legal
                       ;; (`Checker::check_labels` registers every sibling as
                       ;; an ordinary `Fn`-typed variable) but not something
                       ;; any of `while`/`dotimes`/`dolist` (or anything else
                       ;; this compiler is exercised against) ever does — an
                       ;; accepted, documented gap rather than a solved one.
                       ;;
                       ;; `kind` (generalized from a plain `is-fn` `Bool` —
                       ;; `ast_bridge::translate_set`'s doc comment): `kind =
                       ;; 1` still drives `compile-if-branch`'s existing
                       ;; borrowed-`Fn`-retain logic exactly as `is-fn` did.
                       ;; `kind = 2` is new — the target's own slot has a
                       ;; second word (`bind-params`/`bind-captures`/
                       ;; `bind-let-values`) holding the GC-root stack index
                       ;; that name's root was pushed at; this `setf` updates
                       ;; *that exact root* via `rt_set_sexpr_root` rather
                       ;; than leaving it pointing at the value the binding
                       ;; started with. Without this, the freshly stored
                       ;; value has no GC root at all for the rest of the
                       ;; binding's scope — `typelisp-rt`'s
                       ;; `a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root`
                       ;; test demonstrates the resulting corruption directly.
                       (compile-set ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (car (cdr e)))))
                           (let ((kind (sexpr-int (car (cdr (cdr e))))))
                             (let ((is-fn (eq kind 1)))
                               (let ((value-form (car (cdr (cdr (cdr e))))))
                                 (let ((v (compile-if-branch builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base is-fn value-form)))
                                   (match (get env nm)
                                     ((Some slot)
                                      (store-arg builder slot 0 v)
                                      (if (eq kind 2)
                                          (let ((root-idx (load-raw builder slot 1)))
                                            (let ((args-ptr (alloca-args builder 2)))
                                              (store-arg builder args-ptr 0 root-idx)
                                              (store-arg builder args-ptr 1 v)
                                              (let ((ignored (build-call builder (get-function m "rt_set_sexpr_root") args-ptr 2))) ())))
                                          ())
                                      v)
                                     (None (panic (append "compile-set: unbound variable " nm))))))))))
                       ;; Tests `v` against pattern `pat`; on failure
                       ;; branches to `fail-block` (jumping straight to
                       ;; whatever should run next -- never returning to
                       ;; this call), on success falls through with
                       ;; `builder` positioned right after the last
                       ;; guard, having already bound any `pat-bind` name
                       ;; into a fresh `env` slot. Mutually recursive with
                       ;; `compile-ctor-subpatterns` (a `pat-ctor`'s
                       ;; sub-patterns can themselves be `pat-ctor`s, e.g.
                       ;; `(Cons _ (Nil))` -- `prelude.rs`'s `last`/
                       ;; `butlast`), which is why both live in this
                       ;; `labels` ring rather than as top-level `defun`s
                       ;; (the same "can't forward/mutually reference
                       ;; each other" constraint this module's doc
                       ;; comment already explains for `compile-value`
                       ;; & co.).
                       (compile-pattern-test ((builder llvm-builder) (env Scope<llvm-value>) (cur-fn llvm-function) (v llvm-value) (pat Sexpr) (fail-block llvm-basic-block)) ()
                         (match (car pat)
                           ((Sym s)
                            (if (eq s "pat-wild")
                                ()
                                (if (eq s "pat-bind")
                                    (let ((nm (sexpr-str (car (cdr pat)))))
                                      (let ((bslot (alloca-args builder 1)))
                                        (store-arg builder bslot 0 v)
                                        (set env nm bslot)))
                                    (if (eq s "pat-lit")
                                        (compile-pattern-guard builder cur-fn (build-icmp-eq builder v (const-i64 builder (sexpr-int (car (cdr pat))))) fail-block)
                                        (if (eq s "pat-ctor")
                                            (let ((variant (sexpr-int (car (cdr pat)))))
                                              (let ((subpats (car (cdr (cdr pat)))))
                                                (compile-pattern-guard builder cur-fn (compile-sexpr-tag-test builder v variant) fail-block)
                                                (compile-ctor-subpatterns builder env cur-fn v variant subpats 0 fail-block)))
                                            (panic (append "compile-pattern-test: unsupported pattern tag " s)))))))
                           (_ (panic "compile-pattern-test: malformed pattern node"))))
                       ;; Tests/extracts each of a `pat-ctor`'s
                       ;; sub-patterns in turn against variant `variant`'s
                       ;; fields, skipping the extraction call entirely
                       ;; for a `pat-wild` sub-pattern (no value to test
                       ;; or bind, so no reason to call
                       ;; `compile-sexpr-field` -- and for `cons`, no
                       ;; reason to emit an `rt_car`/`rt_cdr` call
                       ;; either).
                       (compile-ctor-subpatterns ((builder llvm-builder) (env Scope<llvm-value>) (cur-fn llvm-function) (v llvm-value) (variant i64) (subpats Sexpr) (idx i32) (fail-block llvm-basic-block)) ()
                         (match subpats
                           ((Cons p rest)
                            (if (eq (sexpr-sym-name (car p)) "pat-wild")
                                (compile-ctor-subpatterns builder env cur-fn v variant rest (+ idx 1) fail-block)
                                (let ((field-v (compile-sexpr-field builder m v variant idx)))
                                  (compile-pattern-test builder env cur-fn field-v p fail-block)
                                  (compile-ctor-subpatterns builder env cur-fn v variant rest (+ idx 1) fail-block))))
                           (_ ())))
                       ;; `(match is-fn scrutinee-form ((pattern-form .
                       ;; body-form)...))` -- `Expr::Match` over a
                       ;; `Sexpr` scrutinee (`ast_bridge::translate_match`
                       ;; -- any other scrutinee type stays
                       ;; `unsupported`, deferred to Construct/FieldGet/
                       ;; FieldSet's own stage). Compiles the scrutinee
                       ;; once, then tries each arm in textual order
                       ;; (`compile-match-arms`) into a single shared
                       ;; 1-slot merge -- the same `alloca-args`/
                       ;; `store-arg`/`load-raw` triple `compile-if`'s
                       ;; 2-way merge already uses, just for an n-way set
                       ;; of arms instead. `is-fn` is `compile-if-branch`'s
                       ;; usual borrowed-value-retain tag, reused
                       ;; unchanged for each arm's body (the same boundary
                       ;; an `if` merge crosses).
                       ;;
                       ;; `scrut-v` itself gets a `push-sexpr-root` right
                       ;; after being computed, popped again once
                       ;; `merge-block` is reached — a gap
                       ;; `tests/compile_test.rs`'s
                       ;; `compile_match_keeps_a_fresh_scrutinee_and_its_pattern_extracted_fields_rooted_across_an_arms_own_allocations`
                       ;; demonstrates directly: `scrut-v` is always
                       ;; `Sexpr`-typed here (`translate_match` only ever
                       ;; translates a `Sexpr` scrutinee for real), but
                       ;; unlike a `let` binding or call argument, nothing
                       ;; else ever rooted it — a *fresh* scrutinee (e.g. the
                       ;; direct result of a call, never bound to a name)
                       ;; had no owner at all, so a `pat-bind` field
                       ;; extracted from it (`compile-pattern-test`'s own
                       ;; `bslot`, itself never rooted either) was only ever
                       ;; safe by the GC transitively marking it *through*
                       ;; `scrut-v` — which nothing protected an allocation
                       ;; during a later arm statement (e.g. a sibling `let`
                       ;; binding's own value) from reclaiming outright.
                       ;; Rooting `scrut-v` for the match's own duration
                       ;; keeps that transitive reachability valid the whole
                       ;; time, which is enough: nothing here needs to root
                       ;; each individual `pat-bind` separately. Popped only
                       ;; on the normal `merge-block` path (that block is
                       ;; always freshly appended, never `block-terminated?`
                       ;; itself, so this is safe to emit unconditionally
                       ;; without `compile-if`/`compile-let-body`'s usual
                       ;; termination guard) — an arm that exits via a bare
                       ;; `break`/`return` instead never reaches
                       ;; `merge-block` at all, so this `pop-sexpr-root` never
                       ;; runs on that path. No longer a leak, though:
                       ;; `compile-break`/`compile-return` themselves
                       ;; unconditionally truncate the GC root stack back to
                       ;; the nearest enclosing loop's own entry depth
                       ;; (`rt_truncate_sexpr_roots`, keyed off
                       ;; `loop-root-base` — see `compile-loop`'s doc
                       ;; comment) right before that jump, which discards
                       ;; this exact root along with any other open scope's,
                       ;; regardless of how many `let`/`match` levels are
                       ;; nested between the exit site and the loop.
                       (compile-match ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((is-fn (sexpr-bool (car (cdr e)))))
                           (let ((scrut-form (car (cdr (cdr e)))))
                             (let ((arms (car (cdr (cdr (cdr e))))))
                               (let ((scrut-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base scrut-form)))
                                 (let ((ignored (push-sexpr-root builder m scrut-v)))
                                   (let ((merge-block (append-block cur-fn "match-merge")))
                                     (let ((slot (alloca-args builder 1)))
                                       (compile-match-arms builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base is-fn scrut-v slot merge-block arms)
                                       (position-at-end builder merge-block)
                                       (let ((result (load-raw builder slot 0)))
                                         (let ((ignored2 (pop-sexpr-root builder m)))
                                           result))))))))))
                       ;; Tries each `(pattern-form . body-form)` arm in
                       ;; order: `push-frame env` a fresh frame for any name
                       ;; this arm's pattern might bind, test the pattern
                       ;; (`compile-pattern-test`, binding into fresh slots —
                       ;; in that new frame — as it succeeds), compile the
                       ;; body through `compile-if-branch` (the usual
                       ;; borrowed-value-retain treatment), `pop-frame` to
                       ;; discard that whole frame at once, store into the
                       ;; shared merge slot, and branch to `merge-block`.
                       ;; `pop-frame` runs exactly *once* per arm — right
                       ;; after `compile-if-branch`, *before* branching either
                       ;; to `merge-block` (success) or falling through into
                       ;; `next-block`'s own code (failure) — never once on
                       ;; each of those two paths separately the way the old
                       ;; per-key `restore-env-names` ran twice (that
                       ;; duplication was safe for an idempotent "restore
                       ;; this key's old value" but would double-pop a stack
                       ;; frame here): compiling this arm's IR is one
                       ;; sequential pass through this function regardless of
                       ;; how many basic blocks it emits, so one `pop-frame`
                       ;; call here is already in effect for every block emitted
                       ;; afterward, including `next-block`'s. `next-block` is
                       ;; this arm's single shared failure target -- every
                       ;; guard `compile-pattern-test` emits for this arm's
                       ;; pattern (however deeply nested) branches there on
                       ;; failure; by the time that code runs, `env` has
                       ;; already had this arm's frame popped, exactly as
                       ;; needed before the next arm's own `push-frame`.
                       ;; Once every arm's pattern has failed, the checker's
                       ;; own exhaustiveness check guarantees this is
                       ;; unreachable for a well-typed program --
                       ;; `rt_match_fail` (an `abort`, see `typelisp-rt`'s
                       ;; `fatal`) is the trap for the case that guarantee
                       ;; was somehow wrong, the same role
                       ;; `resolve-value`'s "unbound variable" panic plays
                       ;; elsewhere in this file.
                       (compile-match-arms ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (is-fn bool) (scrut-v llvm-value) (slot llvm-value) (merge-block llvm-basic-block) (arms Sexpr)) ()
                         (match arms
                           ((Cons arm rest)
                            (let ((pat (car arm)))
                              (let ((body-form (cdr arm)))
                                (push-frame env)
                                (let ((next-block (append-block cur-fn "match-next")))
                                  (compile-pattern-test builder env cur-fn scrut-v pat next-block)
                                  (let ((body-v (compile-if-branch builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base is-fn body-form)))
                                    (pop-frame env)
                                    (if (block-terminated? builder)
                                        ()
                                        (let ((ignored (store-arg builder slot 0 body-v)))
                                          (build-br builder merge-block))))
                                  (position-at-end builder next-block)
                                  (compile-match-arms builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base is-fn scrut-v slot merge-block rest)))))
                           (_ (let ((args-ptr (alloca-args builder 0)))
                                (let ((fallback (build-call builder (get-function m "rt_match_fail") args-ptr 0)))
                                  (store-arg builder slot 0 fallback)
                                  (build-br builder merge-block))))))
                       ;; `(construct is-sexpr variant-i64 arg-form...)`
                       ;; (Stage 6 of the Sexpr-representation plan,
                       ;; `docs/implementation-log.md`) — `is-sexpr` (`ast_bridge::is_sexpr_type`,
                       ;; read off the node's own checked type) dispatches
                       ;; between `Sexpr`'s own 8 variants
                       ;; (`compile-construct-sexpr`) and every other ADT's
                       ;; general `malloc`'d-box representation
                       ;; (`compile-construct-box`) — see those two
                       ;; functions' doc comments.
                       (compile-construct ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((is-sexpr (sexpr-bool (car (cdr e)))))
                           (let ((type-id (sexpr-int (car (cdr (cdr e))))))
                             (let ((variant (sexpr-int (car (cdr (cdr (cdr e)))))))
                               (let ((arg-forms (cdr (cdr (cdr (cdr e))))))
                                 (if is-sexpr
                                     (compile-construct-sexpr builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base variant arg-forms)
                                     (compile-construct-box builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base type-id variant arg-forms)))))))
                       ;; Builds a general-ADT box (`Option`/`Result`/a
                       ;; `defstruct`, sum or struct kind alike): a fresh
                       ;; `build-malloc`'d `[2 + argc]`-slot array, slot `0`
                       ;; the box's own type-id (`ast_bridge::type_id_hash` of
                       ;; its type's local name — added for `compile-trait-call`'s
                       ;; runtime dispatch, see that function's doc comment;
                       ;; `0` for a box built before that stage would have
                       ;; been meaningless anyway, since nothing read it),
                       ;; slot `1` the variant tag, slot `2 + i` the `i`-th
                       ;; field's already-compiled value — `compile-field-get`/
                       ;; `compile-field-set` read/write the same `2 + idx`
                       ;; offset, so the layout only has to agree with
                       ;; itself. The final `build-ptr-to-int` turns the
                       ;; fresh pointer into the plain `i64` value every
                       ;; other compiled value already is (mirroring
                       ;; `build-make-closure`'s own final `ptrtoint`).
                       ;; Deliberately never freed/refcounted in this stage
                       ;; — see this file's own module doc comment for the
                       ;; same accepted leak this codebase already takes for
                       ;; an unreferenced boxed `labels` sibling.
                       (compile-construct-box ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (type-id i64) (variant i64) (arg-forms Sexpr)) llvm-value
                         (let ((argc (sexpr-list-length arg-forms)))
                           (let ((ptr (build-malloc builder (+ argc 2))))
                             (store-arg builder ptr 0 (const-i64 builder type-id))
                             (store-arg builder ptr 1 (const-i64 builder variant))
                             (compile-construct-box-fields builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base ptr arg-forms 2)
                             (build-ptr-to-int builder ptr))))
                       ;; Fills a `compile-construct-box`-allocated array,
                       ;; one compiled field per slot starting at `idx`
                       ;; (`1`, skipping the variant-tag slot) — the
                       ;; `compile-construct` analogue of
                       ;; `compile-call-args`'s argument-array fill. Each
                       ;; `forms` element is a `(kind . field-form)` pair
                       ;; (`ast_bridge::tagged_ast_list_to_sexpr`, the same
                       ;; tagging `compile-call-args` itself reads) rather
                       ;; than a bare form: still no `ClosureBox`
                       ;; retain/release for a `kind = 1` (`Fn`-typed) field
                       ;; (deliberately leaked, same as before), but a
                       ;; `kind = 2` (`Sexpr`-typed) field now gets
                       ;; `push-permanent-sexpr-root`ed right after being
                       ;; stored — closing the GC-root gap a general-ADT
                       ;; box's own fields otherwise have: once `compile-
                       ;; construct-box-fields` returns and whatever local
                       ;; bound the field's value goes out of scope
                       ;; (`release-bindings`'s unconditional `kind = 2`
                       ;; pop), nothing else roots a value sitting only
                       ;; inside this `malloc`'d, never-GC-scanned box —
                       ;; the next collection a *later*, unrelated
                       ;; allocation triggers would otherwise reclaim it out
                       ;; from under the box. See `push-permanent-sexpr-root`'s
                       ;; doc comment for why this needs its own root list
                       ;; rather than the ordinary `push-sexpr-root`.
                       (compile-construct-box-fields ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (ptr llvm-value) (forms Sexpr) (idx i32)) ()
                         (match forms
                           ((Cons field-pair rest)
                            (let ((kind (sexpr-int (car field-pair))))
                              (let ((form (cdr field-pair)))
                                (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                                  (store-arg builder ptr idx v)
                                  (if (eq kind 2)
                                      (push-permanent-sexpr-root builder m v)
                                      ())
                                  (compile-construct-box-fields builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base ptr rest (+ idx 1))))))
                           (_ ())))
                       ;; `Sexpr`'s own 8 variants, encoded directly as the
                       ;; tagged `i64` `typelisp-rt`'s `encode` (and this
                       ;; file's own `compile-sexpr-field`) already use —
                       ;; see `registry::sexpr_def`'s variant order (`0`=nil
                       ;; `1`=int `2`=float `3`=char `4`=bool `5`=sym `6`=str
                       ;; `7`=cons), the exact inverse of
                       ;; `compile-sexpr-tag-test`/`compile-sexpr-field`'s
                       ;; own extraction. `str`'s single field is itself a
                       ;; `Type::Str`-typed sub-expression, which
                       ;; `compile-value`'s own `str` tag (Stage 7) already
                       ;; compiles down to the *fully tagged* `Sexpr::Str`
                       ;; representation directly (`compile-sexpr-field`'s own
                       ;; doc comment explains why a bare `Type::Str` value
                       ;; and a `Sexpr::Str` are the same bits) — so unlike
                       ;; every other variant here, `str` needs no further bit
                       ;; manipulation at all, just the field's own compiled
                       ;; value passed straight through. `sym`'s `Str` field
                       ;; and `float`'s `f64` field still aren't representable
                       ;; in compiled code yet (a `Sym`'s tag payload is a
                       ;; `SymId`, a separate gap from `str`'s own — see
                       ;; `compile-sexpr-field`'s doc comment), so constructing
                       ;; either still panics clearly.
                       (compile-construct-sexpr ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (variant i64) (arg-forms Sexpr)) llvm-value
                         (if (eq variant 0)
                             (const-i64 builder 6)
                             (if (eq variant 1)
                                 (build-shl builder (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (car arg-forms)) (const-i64 builder 3))
                                 (if (eq variant 3)
                                     (build-or builder
                                                (build-shl builder (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (car arg-forms)) (const-i64 builder 3))
                                                (const-i64 builder 4))
                                     (if (eq variant 4)
                                         (let ((b (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (car arg-forms))))
                                           (build-or builder (build-shl builder (build-add builder b (const-i64 builder 1)) (const-i64 builder 3)) (const-i64 builder 6)))
                                         (if (eq variant 7)
                                             (let ((args-ptr (alloca-args builder 2)))
                                               (let ((car-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (car arg-forms))))
                                                 (store-arg builder args-ptr 0 car-v)
                                                 (push-sexpr-root builder m car-v)
                                                 (let ((cdr-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (car (cdr arg-forms)))))
                                                   (store-arg builder args-ptr 1 cdr-v)
                                                   (push-sexpr-root builder m cdr-v)
                                                   (let ((result (build-call builder (get-function m "rt_cons") args-ptr 2)))
                                                     (pop-sexpr-root builder m)
                                                     (pop-sexpr-root builder m)
                                                     result))))
                                             (if (eq variant 6)
                                                 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (car arg-forms))
                                                 (panic "compile-construct-sexpr: field type is not representable in compiled code yet"))))))))
                       ;; `(field-get idx-unary-list obj-form)` (Stage 6) —
                       ;; always a general-ADT box `compile-construct-box`
                       ;; built (this tag is only ever synthesized by
                       ;; `Checker::check_defstruct` as a field accessor's
                       ;; own body, never for a `Sexpr` — see
                       ;; `ast_bridge::translate_field_get`'s doc comment):
                       ;; the object's own value is reinterpreted as a
                       ;; pointer (`build-int-to-ptr`, the inverse of
                       ;; `compile-construct-box`'s final `build-ptr-to-int`)
                       ;; and read at slot `2 + idx` (skipping the type-id
                       ;; and variant-tag slots). `idx` is recovered from
                       ;; `idx-unary-list`'s own length
                       ;; (`ast_bridge::idx_unary_list`'s doc comment
                       ;; explains why it isn't simply a `Sexpr` `Int`).
                       (compile-field-get ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((idx (+ (sexpr-list-length (car (cdr e))) 2)))
                           (let ((obj-form (car (cdr (cdr e)))))
                             (let ((obj-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base obj-form)))
                               (let ((ptr (build-int-to-ptr builder obj-v)))
                                 (load-raw builder ptr idx))))))
                       ;; `(field-set idx-unary-list obj-form value-form)`
                       ;; (Stage 6) — see `compile-field-get`; evaluates to
                       ;; `Unit` (`0`, `compile-unit`'s own convention),
                       ;; matching `Expr::FieldSet`'s own checked type.
                       (compile-field-set ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((idx (+ (sexpr-list-length (car (cdr e))) 2)))
                           (let ((obj-form (car (cdr (cdr e)))))
                             (let ((value-form (car (cdr (cdr (cdr e))))))
                               (let ((obj-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base obj-form)))
                                 (let ((ptr (build-int-to-ptr builder obj-v)))
                                   (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base value-form)))
                                     (store-arg builder ptr idx v)
                                     (const-i64 builder 0))))))))
                       ;; `(trait-call method-str candidates-list arg-form...)`
                       ;; — `Expr::TraitCall`'s compile counterpart:
                       ;; `candidates-list` is a plain list of
                       ;; `(type-id-i64 . type-name-str)` pairs
                       ;; (`ast_bridge::translate_trait_call`, one per
                       ;; `impl` of the trait known at check time), `args`
                       ;; tagged exactly like `compile-assoc`'s own
                       ;; instance-call arguments — `args[0]` is always the
                       ;; receiver. Computes the argument array once
                       ;; (`compile-call-args`, same as `compile-assoc`),
                       ;; then re-reads the receiver straight back out of
                       ;; slot `0` of that same array (`load-raw`) rather
                       ;; than compiling it a second time — the call
                       ;; argument list already holds it, exactly the trick
                       ;; `compile-field-get` uses to read a box field, just
                       ;; applied to the local argument array instead of a
                       ;; heap box. Reinterprets that receiver as a pointer
                       ;; and reads its own type-id (`compile-construct-box`'s
                       ;; slot `0`) to drive `compile-trait-dispatch`'s
                       ;; runtime chain, merged into one shared 1-slot
                       ;; result the same `alloca-args`/`store-arg`/
                       ;; `load-raw` way `compile-if`'s 2-way merge already
                       ;; works, just for an n-way (one per candidate) set of
                       ;; blocks instead. `release-pending-args`/
                       ;; `pop-sexpr-roots` run once, at the merge point —
                       ;; safe (and simpler than duplicating them into every
                       ;; candidate block) since only one candidate's call
                       ;; block ever actually runs, and the merge block
                       ;; post-dominates every one of them.
                       (compile-trait-call ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((method (sexpr-str (car (cdr e)))))
                           (let ((candidates (car (cdr (cdr e)))))
                             (let ((arg-forms (cdr (cdr (cdr e)))))
                               (let ((argc (sexpr-list-length arg-forms)))
                                 (let ((args-ptr (alloca-args builder argc)))
                                   (let ((pending-ptr (alloca-args builder argc)))
                                     (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr pending-ptr arg-forms 0)))
                                       (let ((recv (load-raw builder args-ptr 0)))
                                         (let ((recv-ptr (build-int-to-ptr builder recv)))
                                           (let ((type-id (load-raw builder recv-ptr 0)))
                                             (let ((merge-block (append-block cur-fn "trait-call-merge")))
                                               (let ((slot (alloca-args builder 1)))
                                                 (compile-trait-dispatch builder cur-fn args-ptr argc method type-id merge-block slot candidates)
                                                 (position-at-end builder merge-block)
                                                 (let ((result (load-raw builder slot 0)))
                                                   (release-pending-args builder m pending-ptr argc 0)
                                                   (pop-sexpr-roots builder m sexpr-roots)
                                                   result))))))))))))))
                       ;; Walks `candidates` (`(type-id-i64 . type-name-str)`
                       ;; pairs), building one two-way test per entry: if the
                       ;; runtime `type-id` matches this candidate's, call
                       ;; its own mangled `type-name::method` (the same
                       ;; forward-declared/wired name `compile-assoc` already
                       ;; relies on — `ast_bridge::collect_trait_call_targets`
                       ;; requires and wires every candidate exactly like a
                       ;; static `Expr::Assoc` target), store the result into
                       ;; `slot`, and branch to `merge-block`; otherwise fall
                       ;; through to the next candidate's own test. Once every
                       ;; candidate has failed, `Checker::check_instance_method`'s
                       ;; own call-site check (every argument's concrete type
                       ;; must actually implement the bound trait) guarantees
                       ;; this is unreachable for a well-typed program —
                       ;; `rt_trait_call_fail` (an `abort`, mirroring
                       ;; `rt_match_fail`'s role for `compile-match-arms`) is
                       ;; the trap for the case that guarantee was somehow
                       ;; wrong.
                       (compile-trait-dispatch ((builder llvm-builder) (cur-fn llvm-function) (args-ptr llvm-value) (argc i32) (method string) (type-id llvm-value) (merge-block llvm-basic-block) (slot llvm-value) (candidates Sexpr)) ()
                         (match candidates
                           ((Cons pair rest)
                            (let ((cand-id (sexpr-int (car pair))))
                              (let ((cand-name (sexpr-str (cdr pair))))
                                (let ((call-block (append-block cur-fn "trait-call-call")))
                                  (let ((next-block (append-block cur-fn "trait-call-next")))
                                    (build-cond-br builder (build-icmp-eq builder type-id (const-i64 builder cand-id)) call-block next-block)
                                    (position-at-end builder call-block)
                                    (let ((mangled (append cand-name (append "::" method))))
                                      (let ((result (build-call builder (get-function m mangled) args-ptr argc)))
                                        (store-arg builder slot 0 result)
                                        (build-br builder merge-block)))
                                    (position-at-end builder next-block)
                                    (compile-trait-dispatch builder cur-fn args-ptr argc method type-id merge-block slot rest))))))
                           (_ (let ((fail-args (alloca-args builder 0)))
                                (let ((fallback (build-call builder (get-function m "rt_trait_call_fail") fail-args 0)))
                                  (store-arg builder slot 0 fallback)
                                  (build-br builder merge-block)))))))
                (let ((v (compile-value builder env fn-env '() f (Option::none) (Option::none) (Option::none) body)))
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
